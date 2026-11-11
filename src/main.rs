//! cvm: account and version manager for Claude Code.
//!
//! One binary, three roles, chosen by the name it is invoked as:
//!   * `cvm`           -> management CLI
//!   * `claude`        -> shim: resolves account + version, then runs the real Claude Code
//!   * an alias name   -> same as the shim, with the alias's selector forced
//!
//! Two independent axes:
//!   * account: a Claude Code config directory (CLAUDE_CONFIG_DIR) with its own login.
//!     The built-in `default` account is the untouched `~/.claude`.
//!   * version: a Claude Code build. `latest` means whatever is installed.
//!
//! A selector names a point on those axes: `work`, `work@2.1.311`, `work@stable`, `@stable`.

use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

type Res<T> = Result<T, String>;

/// println! that does not panic when stdout is a closed pipe (e.g. `cvm list | head`).
macro_rules! say {
    ($($t:tt)*) => {{ let _ = writeln!(io::stdout(), $($t)*); }};
}

/// Items linked from the shared source folder into every new account.
const SHARED: &[&str] = &[
    "skills",
    "agents",
    "commands",
    "plugins",
    "CLAUDE.md",
    "settings.json",
    "keybindings.json",
];
const DEFAULT: &str = "default";
const EXTERNAL: &str = "(external)";
const LATEST: &str = "latest";
const RC_FILE: &str = ".cvmrc";
const EXE_SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };

// ---------------------------------------------------------------- paths

fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(var)
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .expect("cannot determine home directory")
}
fn cvm_home() -> PathBuf {
    env::var_os("CVM_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cvm"))
}
fn accounts_dir() -> PathBuf {
    cvm_home().join("accounts")
}
fn bin_dir() -> PathBuf {
    cvm_home().join("bin")
}
fn versions_dir() -> PathBuf {
    cvm_home().join("versions")
}
fn tags_dir() -> PathBuf {
    cvm_home().join("tags")
}
fn aliases_dir() -> PathBuf {
    cvm_home().join("aliases")
}
fn global_rc() -> PathBuf {
    cvm_home().join("global")
}
/// Where Claude Code's native installer keeps its builds.
fn native_versions_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("claude")
        .join("versions")
}
fn account_dir(name: &str) -> PathBuf {
    accounts_dir().join(name)
}
fn exe_name(stem: &str) -> String {
    format!("{stem}{EXE_SUFFIX}")
}

fn valid_name(kind: &str, name: &str) -> Res<()> {
    let ok = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit() || c == '-');
    if !ok {
        return Err(format!(
            "invalid {kind} name '{name}' (letters, digits, - and _; must start with a letter)"
        ));
    }
    Ok(())
}
fn account_exists(name: &str) -> bool {
    name == DEFAULT || account_dir(name).is_dir()
}
fn require_account(name: &str) -> Res<()> {
    if !account_exists(name) {
        return Err(format!("no account '{name}' (create it with: cvm account add {name})"));
    }
    Ok(())
}
fn read_trim(p: &Path) -> Option<String> {
    let s = fs::read_to_string(p).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}
fn write_file(p: &Path, s: &str) -> Res<()> {
    if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    fs::write(p, format!("{s}\n")).map_err(|e| format!("{}: {e}", p.display()))
}
fn names_in(dir: PathBuf) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    v.sort();
    v
}

// ---------------------------------------------------------------- selectors and .cvmrc

/// A choice on either axis, or both. `version` is as written: a build number, a tag, or `latest`.
#[derive(Default, Clone, PartialEq, Debug)]
struct Sel {
    account: Option<String>,
    version: Option<String>,
}

impl Sel {
    fn merged_over(&self, base: &Sel) -> Sel {
        Sel {
            account: self.account.clone().or_else(|| base.account.clone()),
            version: self.version.clone().or_else(|| base.version.clone()),
        }
    }
    fn text(&self) -> String {
        match (&self.account, &self.version) {
            (Some(a), Some(v)) => format!("{a}@{v}"),
            (Some(a), None) => a.clone(),
            (None, Some(v)) => format!("@{v}"),
            (None, None) => String::new(),
        }
    }
}

/// `work`, `work@2.1.311`, `@stable`
fn parse_sel_text(s: &str) -> Res<Sel> {
    let s = s.trim();
    let (a, v) = match s.split_once('@') {
        Some((a, v)) => (a, Some(v)),
        None => (s, None),
    };
    if a.is_empty() && v.is_none() {
        return Err("empty selector (expected account, account@version or @version)".into());
    }
    if v == Some("") {
        return Err(format!("'{s}': nothing after @"));
    }
    Ok(Sel {
        account: (!a.is_empty()).then(|| a.to_string()),
        version: v.map(str::to_string),
    })
}

/// `.cvmrc` format: `account = work` / `version = 2.1.311`, `#` comments.
fn parse_rc(text: &str) -> Sel {
    let mut sel = Sel::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
        if v.is_empty() {
            continue;
        }
        match k.trim().to_lowercase().as_str() {
            "account" => sel.account = Some(v),
            "version" => sel.version = Some(v),
            _ => {}
        }
    }
    sel
}
fn format_rc(sel: &Sel) -> String {
    let mut lines = Vec::new();
    if let Some(a) = &sel.account {
        lines.push(format!("account = {a}"));
    }
    if let Some(v) = &sel.version {
        lines.push(format!("version = {v}"));
    }
    lines.join("\n")
}
fn read_rc(p: &Path) -> Option<Sel> {
    fs::read_to_string(p).ok().map(|t| parse_rc(&t))
}
/// Nearest `.cvmrc` in the current directory or a parent.
fn find_rc() -> Option<(PathBuf, Sel)> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let p = dir.join(RC_FILE);
        if let Some(sel) = read_rc(&p) {
            return Some((p, sel));
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// A build number as written, or a tag expanded to one. None means "whatever is installed".
fn expand_version(v: &str) -> Res<Option<String>> {
    if v == LATEST || v == "installed" {
        Ok(None)
    } else if v.starts_with(|c: char| c.is_ascii_digit()) {
        Ok(Some(v.to_string()))
    } else {
        read_trim(&tags_dir().join(v))
            .map(Some)
            .ok_or_else(|| format!("no version tag '{v}' (create it with: cvm version tag {v} <version>)"))
    }
}

/// Turn user input into a checked selector: expands aliases, verifies account and version exist.
fn selector(input: &str) -> Res<Sel> {
    let sel = match read_trim(&aliases_dir().join(input)) {
        Some(saved) if !input.contains('@') => parse_sel_text(&saved)?,
        _ => parse_sel_text(input)?,
    };
    if let Some(a) = &sel.account {
        require_account(a)?;
    }
    if let Some(v) = &sel.version {
        if let Some(real) = expand_version(v)? {
            keep_version(&real)?;
        }
    }
    Ok(sel)
}

// ---------------------------------------------------------------- resolution

struct Resolved {
    account: String,
    account_src: String,
    /// None = leave CLAUDE_CONFIG_DIR alone (default account, or one set by the caller).
    config_dir: Option<PathBuf>,
    /// Build to run; None = the installed Claude Code.
    version: Option<String>,
    /// As written by the user: `stable`, `2.1.311`, `latest`.
    version_label: String,
    version_src: String,
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|s| !s.is_empty())
}

/// Each axis is resolved on its own: command line, then this terminal, then the nearest
/// `.cvmrc`, then the global choice, then the built-in default.
fn resolve(forced: &Sel, forced_src: &str) -> Res<Resolved> {
    let rc = find_rc();
    let global = read_rc(&global_rc()).unwrap_or_default();

    // --- account
    let mut picked: Option<(String, String)> = None;
    if let Some(a) = &forced.account {
        require_account(a)?;
        picked = Some((a.clone(), forced_src.to_string()));
    } else if let Some(a) = env_nonempty("CVM_ACCOUNT") {
        require_account(&a).map_err(|e| format!("{e} [from CVM_ACCOUNT]"))?;
        picked = Some((a, "CVM_ACCOUNT (this terminal)".into()));
    } else if env::var_os("CLAUDE_CONFIG_DIR").is_some() {
        picked = Some((EXTERNAL.into(), "CLAUDE_CONFIG_DIR already set; cvm left it alone".into()));
    }
    if picked.is_none() {
        // Files may name an account that does not exist on this machine (a shared repo's
        // .cvmrc, say). That is a warning, not an error: fall through to the next source.
        let from_files = [
            rc.as_ref().and_then(|(p, s)| s.account.clone().map(|a| (a, p.display().to_string()))),
            global.account.clone().map(|a| (a, "global (cvm use)".to_string())),
        ];
        for (a, src) in from_files.into_iter().flatten() {
            if account_exists(&a) {
                picked = Some((a, src));
                break;
            }
            eprintln!("cvm: warning: {src} names account '{a}', which does not exist here; ignoring it");
        }
    }
    let (account, account_src) = picked.unwrap_or((DEFAULT.into(), "built-in default".into()));
    let config_dir = (account != DEFAULT && account != EXTERNAL).then(|| account_dir(&account));

    // --- version
    let (label, version_src) = if let Some(v) = &forced.version {
        (v.clone(), forced_src.to_string())
    } else if let Some(v) = env_nonempty("CVM_VERSION") {
        (v, "CVM_VERSION (this terminal)".into())
    } else if let Some((p, v)) = rc.as_ref().and_then(|(p, s)| s.version.clone().map(|v| (p, v))) {
        (v, p.display().to_string())
    } else if let Some(v) = global.version.clone() {
        (v, "global (cvm use)".into())
    } else {
        (LATEST.into(), "built-in default".into())
    };
    let version = expand_version(&label).map_err(|e| format!("{e} [from {version_src}]"))?;

    Ok(Resolved { account, account_src, config_dir, version, version_label: label, version_src })
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// First executable called `stem` on PATH that is not in cvm's own bin folder.
fn find_on_path(stem: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"].iter().map(|x| format!("{stem}.{x}")).collect()
    } else {
        vec![stem.to_string()]
    };
    let me = env::current_exe().ok();
    let shim_dir = bin_dir();
    let mut dirs: Vec<PathBuf> = env::var_os("PATH").map(|p| env::split_paths(&p).collect()).unwrap_or_default();
    dirs.push(home().join(".local").join("bin"));
    for dir in dirs {
        if same_path(&dir, &shim_dir) {
            continue;
        }
        for n in &names {
            let c = dir.join(n);
            if c.is_file() && !me.as_ref().is_some_and(|m| same_path(m, &c)) {
                return Some(c);
            }
        }
    }
    None
}

fn find_installed_claude() -> Res<PathBuf> {
    find_on_path("claude")
        .ok_or_else(|| "could not find Claude Code on PATH (install it first: https://code.claude.com)".into())
}

fn kept_binary(v: &str) -> PathBuf {
    versions_dir().join(v).join(exe_name("claude"))
}

/// Copy (hard-link when possible) a natively installed build into cvm's own store,
/// so Claude Code's auto-cleanup of old builds cannot remove it.
fn keep_version(v: &str) -> Res<PathBuf> {
    let dst = kept_binary(v);
    if dst.is_file() {
        return Ok(dst);
    }
    let src = native_versions_dir().join(v);
    if !src.is_file() {
        return Err(format!("Claude Code {v} is not installed (run: cvm version install {v})"));
    }
    fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::hard_link(&src, &dst)
        .or_else(|_| fs::copy(&src, &dst).map(|_| ()))
        .map_err(|e| format!("could not store version {v}: {e}"))?;
    Ok(dst)
}

fn real_claude(r: &Resolved) -> Res<PathBuf> {
    match &r.version {
        Some(v) => keep_version(v).map_err(|e| format!("{e} [version from {}]", r.version_src)),
        None => find_installed_claude(),
    }
}

/// Run the real Claude Code with the resolved account and version. Never returns on success.
fn launch(r: &Resolved, args: &[OsString]) -> Res<()> {
    let exe = real_claude(r)?;
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .env("CVM_ACTIVE_ACCOUNT", &r.account)
        .env("CVM_ACTIVE_VERSION", r.version.as_deref().unwrap_or(LATEST));
    if let Some(dir) = &r.config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    if r.version.is_some() {
        // A pinned build must not replace itself.
        cmd.env("DISABLE_AUTOUPDATER", "1");
    }
    run(cmd).map_err(|e| format!("failed to run {}: {e}", exe.display()))
}

#[cfg(unix)]
fn run(mut cmd: Command) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    Err(cmd.exec())
}
#[cfg(windows)]
fn run(mut cmd: Command) -> io::Result<()> {
    // Claude Code handles Ctrl+C itself; the shim must outlive it to pass the exit code on.
    let _ = ctrlc::set_handler(|| {});
    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------- links

fn is_junction(_p: &Path) -> bool {
    #[cfg(windows)]
    {
        junction::exists(_p).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn link_item(src: &Path, dst: &Path) -> io::Result<&'static str> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)?;
        Ok("symlink")
    }
    #[cfg(windows)]
    {
        if src.is_dir() {
            junction::create(src, dst)?;
            return Ok("junction");
        }
        if std::os::windows::fs::symlink_file(src, dst).is_ok() {
            return Ok("symlink");
        }
        if fs::hard_link(src, dst).is_ok() {
            return Ok("hard link");
        }
        fs::copy(src, dst)?;
        Ok("copy (will not stay in sync)")
    }
}

/// Delete a tree without ever following links or junctions into shared folders.
fn remove_tree(p: &Path) -> io::Result<()> {
    let ft = fs::symlink_metadata(p)?.file_type();
    if ft.is_symlink() || is_junction(p) {
        fs::remove_dir(p).or_else(|_| fs::remove_file(p))
    } else if ft.is_dir() {
        for e in fs::read_dir(p)? {
            remove_tree(&e?.path())?;
        }
        fs::remove_dir(p)
    } else {
        fs::remove_file(p)
    }
}

/// Put a command called `stem` in cvm's bin folder that runs this same binary.
fn make_launcher(stem: &str) -> Res<()> {
    let bin = bin_dir();
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let target = bin.join(exe_name(stem));
    let installed = bin.join(exe_name("cvm"));
    let me = env::current_exe().map_err(|e| e.to_string())?;
    let source = if installed.is_file() { installed } else { me };
    if same_path(&source, &target) {
        return Ok(());
    }
    let _ = fs::remove_file(&target);
    #[cfg(unix)]
    let made = if stem == "cvm" {
        fs::copy(&source, &target).map(|_| ())
    } else {
        std::os::unix::fs::symlink("cvm", &target)
    };
    #[cfg(windows)]
    let made = fs::hard_link(&source, &target).or_else(|_| fs::copy(&source, &target).map(|_| ()));
    made.map_err(|e| format!("{}: {e}", target.display()))
}

// ---------------------------------------------------------------- CLI

#[derive(Parser)]
#[command(name = "cvm", version, about = "Account and version manager for Claude Code")]
#[command(after_help = "A selector is account, account@version or @version, e.g. work, work@2.1.311, \
work@stable, @latest. An alias name works anywhere a selector does.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Manage accounts (separate Claude Code logins)
    Account {
        #[command(subcommand)]
        cmd: AccountCmd,
    },
    /// Manage Claude Code versions
    Version {
        #[command(subcommand)]
        cmd: VersionCmd,
    },
    /// Sign an account in (same as: cvm account login)
    Login { account: String },
    /// Save a selector under a short name, and as a command: cvm alias cw work@stable
    Alias {
        name: Option<String>,
        selector: Option<String>,
        /// Create the command even if another program with that name is on PATH
        #[arg(long)]
        force: bool,
    },
    /// Delete an alias and its command
    Unalias { name: String },
    /// Set the global choice for new `claude` sessions
    Use { selector: String },
    /// Set the choice for the current folder (writes .cvmrc)
    Local { selector: String },
    /// Print the shell command that switches only this terminal
    Env {
        selector: Option<String>,
        /// bash, zsh, fish, powershell or cmd (default: detected)
        #[arg(long)]
        shell: Option<String>,
        /// Print the command that clears the per-terminal choice
        #[arg(long)]
        unset: bool,
    },
    /// Run Claude Code once with a selector: cvm run work@stable -- --continue
    Run {
        selector: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Show which account and version `claude` would use here, and why
    #[command(alias = "which")]
    Current,
    /// Show accounts against versions, with aliases
    #[command(alias = "ls")]
    List,
    /// Install cvm, the `claude` shim and alias commands into cvm's bin folder
    Setup,
}

#[derive(Subcommand)]
enum AccountCmd {
    /// Create an account
    Add {
        name: String,
        /// Start empty instead of linking skills, plugins, settings etc. from the shared folder
        #[arg(long)]
        isolated: bool,
        /// Folder to share from (default: ~/.claude)
        #[arg(long, value_name = "DIR")]
        from: Option<PathBuf>,
    },
    /// List accounts
    #[command(alias = "ls")]
    List,
    /// Sign in (runs `claude auth login` inside the account)
    Login { name: String },
    /// Delete an account, including its login and history
    #[command(alias = "rm")]
    Remove {
        name: String,
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum VersionCmd {
    /// List versions and tags
    #[command(alias = "ls")]
    List,
    /// Install a version with Claude Code's own installer and keep it
    Install { version: String },
    /// Give a version a name: cvm version tag stable 2.1.311
    Tag { name: String, version: String },
    /// Remove a version name
    Untag { name: String },
}

fn account_email(account: &str) -> Option<String> {
    let file = if account == DEFAULT {
        home().join(".claude.json")
    } else {
        account_dir(account).join(".claude.json")
    };
    let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    json["oauthAccount"]["emailAddress"].as_str().map(str::to_string)
}

fn account_names() -> Vec<String> {
    let mut names = vec![DEFAULT.to_string()];
    names.extend(names_in(accounts_dir()).into_iter().filter(|n| account_dir(n).is_dir()));
    names
}

/// (alias name, saved selector)
fn aliases() -> Vec<(String, Sel)> {
    names_in(aliases_dir())
        .into_iter()
        .filter_map(|n| {
            let sel = parse_sel_text(&read_trim(&aliases_dir().join(&n))?).ok()?;
            Some((n, sel))
        })
        .collect()
}

/// (tag name, version)
fn tags() -> Vec<(String, String)> {
    names_in(tags_dir()).into_iter().filter_map(|n| read_trim(&tags_dir().join(&n)).map(|v| (n, v))).collect()
}

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn kept_versions() -> Vec<String> {
    let mut v: Vec<String> = names_in(versions_dir()).into_iter().filter(|v| kept_binary(v).is_file()).collect();
    v.sort_by_key(|v| version_key(v));
    v
}

fn describe(sel: &Sel) -> String {
    let account = sel.account.as_deref().unwrap_or("(not set)");
    let version = match sel.version.as_deref() {
        None => "(not set)".to_string(),
        Some(v) => match expand_version(v) {
            Ok(Some(real)) if real != v => format!("{v} ({real})"),
            _ => v.to_string(),
        },
    };
    format!("account {account}, version {version}")
}

// ---------------------------------------------------------------- commands

fn cmd_account_add(name: &str, isolated: bool, from: Option<PathBuf>) -> Res<()> {
    valid_name("account", name)?;
    if account_exists(name) {
        return Err(format!("account '{name}' already exists"));
    }
    if aliases_dir().join(name).is_file() {
        return Err(format!("'{name}' is already an alias; pick another name or run: cvm unalias {name}"));
    }
    let dir = account_dir(name);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    say!("Created account '{name}' at {}", dir.display());
    if !isolated {
        let src = from.unwrap_or_else(|| home().join(".claude"));
        let src = fs::canonicalize(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        for item in SHARED {
            let s = src.join(item);
            if !s.exists() {
                continue;
            }
            match link_item(&s, &dir.join(item)) {
                Ok(how) => say!("  shared {item} ({how})"),
                Err(e) => say!("  could not share {item}: {e}"),
            }
        }
    }
    say!("Next: cvm login {name}");
    Ok(())
}

fn cmd_account_list() -> Res<()> {
    let current = resolve(&Sel::default(), "").map(|r| r.account).unwrap_or_default();
    for name in account_names() {
        let mark = if name == current { "*" } else { " " };
        let who = account_email(&name).unwrap_or_else(|| "not signed in".into());
        say!("{mark} {name:<16} {who}");
    }
    Ok(())
}

fn cmd_account_remove(name: &str, yes: bool) -> Res<()> {
    if name == DEFAULT {
        return Err("the default account is your normal ~/.claude; cvm will not delete it".into());
    }
    require_account(name)?;
    if !yes {
        print!("Delete account '{name}', including its login and history? [y/N] ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            say!("Kept.");
            return Ok(());
        }
    }
    remove_tree(&account_dir(name)).map_err(|e| format!("could not delete: {e}"))?;
    say!("Removed account '{name}'.");
    for (alias, sel) in aliases() {
        if sel.account.as_deref() == Some(name) {
            remove_alias(&alias);
            say!("Removed alias '{alias}' (it pointed at '{name}').");
        }
    }
    let mut global = read_rc(&global_rc()).unwrap_or_default();
    if global.account.as_deref() == Some(name) {
        global.account = None;
        write_file(&global_rc(), &format_rc(&global))?;
        say!("'{name}' was the global account; back to default.");
    }
    Ok(())
}

fn login(name: &str) -> Res<()> {
    require_account(name)?;
    let sel = Sel { account: Some(name.to_string()), version: None };
    launch(&resolve(&sel, "command line")?, &["auth".into(), "login".into()])
}

fn remove_alias(name: &str) {
    let _ = fs::remove_file(aliases_dir().join(name));
    let _ = fs::remove_file(bin_dir().join(exe_name(name)));
}

fn cmd_alias(name: Option<String>, sel_text: Option<String>, force: bool) -> Res<()> {
    let (Some(name), Some(sel_text)) = (name.clone(), sel_text) else {
        if let Some(n) = name {
            let saved = read_trim(&aliases_dir().join(&n)).ok_or_else(|| format!("no alias '{n}'"))?;
            say!("{n} = {saved}");
            return Ok(());
        }
        let all = aliases();
        if all.is_empty() {
            say!("No aliases. Create one with: cvm alias <name> <selector>");
        }
        for (n, sel) in all {
            say!("{n:<12} = {}", sel.text());
        }
        return Ok(());
    };
    valid_name("alias", &name)?;
    if name == "cvm" || name == "claude" {
        return Err(format!("'{name}' is reserved"));
    }
    if account_exists(&name) {
        return Err(format!("'{name}' is already an account name"));
    }
    if !force {
        if let Some(other) = find_on_path(&name) {
            return Err(format!(
                "'{name}' would hide an existing command ({}); pick another name or pass --force",
                other.display()
            ));
        }
    }
    let sel = selector(&sel_text)?;
    write_file(&aliases_dir().join(&name), &sel.text())?;
    make_launcher(&name)?;
    say!("{name} = {}   ({})", sel.text(), describe(&sel));
    say!("Run `{name}` to start Claude Code with it.");
    Ok(())
}

/// Shared by `use` (global file) and `local` (.cvmrc): named parts change, the rest stay.
fn set_choice(file: &Path, input: &str, what: &str) -> Res<()> {
    let sel = selector(input)?;
    let merged = sel.merged_over(&read_rc(file).unwrap_or_default());
    write_file(file, &format_rc(&merged))?;
    say!("{what}: {}", describe(&merged));
    Ok(())
}

fn detect_shell() -> String {
    if let Some(sh) = env::var_os("SHELL") {
        let sh = sh.to_string_lossy().to_lowercase();
        return if sh.contains("fish") { "fish".into() } else { "bash".into() };
    }
    if cfg!(windows) { "powershell".into() } else { "bash".into() }
}

fn cmd_env(input: Option<String>, shell: Option<String>, unset: bool) -> Res<()> {
    let shell = shell.unwrap_or_else(detect_shell).to_lowercase();
    let shell = shell.as_str();
    let set = |var: &str, val: &str| match shell {
        "fish" => format!("set -gx {var} {val}"),
        "powershell" | "pwsh" => format!("$env:{var} = '{val}'"),
        "cmd" => format!("set {var}={val}"),
        _ => format!("export {var}='{val}'"),
    };
    let clear = |var: &str| match shell {
        "fish" => format!("set -e {var}"),
        "powershell" | "pwsh" => format!("Remove-Item Env:{var} -ErrorAction SilentlyContinue"),
        "cmd" => format!("set {var}="),
        _ => format!("unset {var}"),
    };
    let mut parts = Vec::new();
    if unset {
        parts.push(clear("CVM_ACCOUNT"));
        parts.push(clear("CVM_VERSION"));
    } else {
        let sel = selector(&input.ok_or("give a selector, or --unset")?)?;
        if let Some(a) = &sel.account {
            parts.push(set("CVM_ACCOUNT", a));
        }
        if let Some(v) = &sel.version {
            parts.push(set("CVM_VERSION", v));
        }
    }
    say!("{}", parts.join(if shell == "cmd" { " & " } else { "; " }));
    Ok(())
}

fn cmd_current() -> Res<()> {
    let r = resolve(&Sel::default(), "")?;
    say!("account : {}  <- {}", r.account, r.account_src);
    let dir = match (&r.config_dir, env::var_os("CLAUDE_CONFIG_DIR")) {
        (Some(d), _) => d.clone(),
        (None, Some(d)) => PathBuf::from(d),
        (None, None) => home().join(".claude"),
    };
    say!("config  : {}", dir.display());
    let shown = match &r.version {
        Some(real) if *real != r.version_label => format!("{} ({real})", r.version_label),
        _ => r.version_label.clone(),
    };
    say!("version : {shown}  <- {}", r.version_src);
    match real_claude(&r) {
        Ok(p) => say!("binary  : {}", p.display()),
        Err(e) => say!("binary  : {e}"),
    }
    Ok(())
}

/// Accounts down the side, versions across the top, alias names in the cells.
fn cmd_list() -> Res<()> {
    let now = resolve(&Sel::default(), "").ok();
    let tags = tags();
    let aliases = aliases();

    let mut cols: Vec<Option<String>> = vec![None];
    cols.extend(kept_versions().into_iter().map(Some));
    let header = |c: &Option<String>| match c {
        None => LATEST.to_string(),
        Some(v) => {
            let names: Vec<&str> = tags.iter().filter(|(_, tv)| tv == v).map(|(n, _)| n.as_str()).collect();
            if names.is_empty() { v.clone() } else { format!("{v} ({})", names.join(", ")) }
        }
    };
    let rows = account_names();
    let cell = |account: &str, col: &Option<String>| -> String {
        let mut names: Vec<&str> = aliases
            .iter()
            .filter(|(_, s)| {
                s.account.as_deref().unwrap_or(DEFAULT) == account
                    && s.version.as_deref().map(|v| expand_version(v).ok().flatten()).unwrap_or(None) == *col
            })
            .map(|(n, _)| n.as_str())
            .collect();
        let current = now.as_ref().is_some_and(|r| r.account == account && r.version == *col);
        if current {
            names.insert(0, "*");
        }
        if names.is_empty() { ".".into() } else { names.join(" ") }
    };

    let w0 = rows.iter().map(String::len).max().unwrap_or(0).max(7) + 2;
    let widths: Vec<usize> = cols
        .iter()
        .map(|c| rows.iter().map(|r| cell(r, c).len()).max().unwrap_or(0).max(header(c).len()) + 3)
        .collect();
    let mut line = format!("{:<w0$}", "");
    for (c, w) in cols.iter().zip(&widths) {
        line.push_str(&format!("{:<w$}", header(c)));
    }
    say!("{}", line.trim_end());
    for r in &rows {
        let mut line = format!("{r:<w0$}");
        for (c, w) in cols.iter().zip(&widths) {
            line.push_str(&format!("{:<w$}", cell(r, c)));
        }
        let who = account_email(r).unwrap_or_else(|| "not signed in".into());
        say!("{line}  {who}");
    }
    say!("\n* = what `claude` uses here");
    for (n, s) in &aliases {
        say!("{n} = {}", s.text());
    }
    Ok(())
}

fn cmd_setup() -> Res<()> {
    let bin = bin_dir();
    make_launcher("cvm")?;
    make_launcher("claude")?;
    let made = aliases();
    for (n, _) in &made {
        make_launcher(n)?;
    }
    say!("Installed cvm, the claude shim and {} alias command(s) in {}", made.len(), bin.display());
    let claude = exe_name("claude");
    let first = env::var_os("PATH")
        .and_then(|p| env::split_paths(&p).find(|d| d.join(&claude).is_file() || d.join("claude.cmd").is_file()));
    if first.as_ref().is_some_and(|d| same_path(d, &bin)) {
        say!("PATH is already set up: `claude` goes through cvm.");
    } else {
        say!("\nPut that folder ahead of the real `claude` on your PATH:");
        if cfg!(windows) {
            say!("  PowerShell profile:  $env:PATH = \"{};$env:PATH\"", bin.display());
        }
        say!("  bash/zsh rc file:    export PATH=\"{}:$PATH\"", bin.display().to_string().replace('\\', "/"));
        say!("  fish:                fish_add_path {}", bin.display());
    }
    Ok(())
}

fn cmd_version(cmd: VersionCmd) -> Res<()> {
    match cmd {
        VersionCmd::List => {
            let kept = kept_versions();
            let mut all: Vec<String> = names_in(native_versions_dir());
            all.extend(kept.iter().cloned());
            all.retain(|v| v.starts_with(|c: char| c.is_ascii_digit()));
            all.sort_by_key(|v| version_key(v));
            all.dedup();
            let tags = tags();
            if all.is_empty() {
                say!("No versions found. Install one with: cvm version install <version>");
            }
            for v in all {
                let k = if kept.contains(&v) { "kept" } else { "    " };
                let names: Vec<&str> = tags.iter().filter(|(_, tv)| *tv == v).map(|(n, _)| n.as_str()).collect();
                say!("  {v:<12} {k}  {}", names.join(", "));
            }
            say!("  {LATEST:<12}       whatever Claude Code is installed");
            Ok(())
        }
        VersionCmd::Install { version } => {
            if !kept_binary(&version).is_file() && !native_versions_dir().join(&version).is_file() {
                let claude = find_installed_claude()?;
                say!("Running: claude install {version}");
                let ok = Command::new(claude).args(["install", &version]).status().map_err(|e| e.to_string())?;
                if !ok.success() {
                    return Err(format!("claude install {version} failed"));
                }
                say!("Note: Claude Code's installer may also have switched the installed `claude` to {version}.");
            }
            let p = keep_version(&version)?;
            say!("Claude Code {version} is kept at {}", p.display());
            Ok(())
        }
        VersionCmd::Tag { name, version } => {
            valid_name("tag", &name)?;
            if name == LATEST || name == "installed" {
                return Err(format!("'{name}' is reserved"));
            }
            if !version.starts_with(|c: char| c.is_ascii_digit()) {
                return Err(format!("'{version}' is not a version number"));
            }
            keep_version(&version)?;
            write_file(&tags_dir().join(&name), &version)?;
            say!("{name} = {version}");
            Ok(())
        }
        VersionCmd::Untag { name } => {
            fs::remove_file(tags_dir().join(&name)).map_err(|_| format!("no version tag '{name}'"))?;
            say!("Removed tag '{name}'.");
            Ok(())
        }
    }
}

fn dispatch(cli: Cli) -> Res<()> {
    match cli.cmd {
        Cmd::Account { cmd } => match cmd {
            AccountCmd::Add { name, isolated, from } => cmd_account_add(&name, isolated, from),
            AccountCmd::List => cmd_account_list(),
            AccountCmd::Login { name } => login(&name),
            AccountCmd::Remove { name, yes } => cmd_account_remove(&name, yes),
        },
        Cmd::Version { cmd } => cmd_version(cmd),
        Cmd::Login { account } => login(&account),
        Cmd::Alias { name, selector, force } => cmd_alias(name, selector, force),
        Cmd::Unalias { name } => {
            if !aliases_dir().join(&name).is_file() {
                return Err(format!("no alias '{name}'"));
            }
            remove_alias(&name);
            say!("Removed alias '{name}'.");
            Ok(())
        }
        Cmd::Use { selector } => set_choice(&global_rc(), &selector, "Global"),
        Cmd::Local { selector } => set_choice(Path::new(RC_FILE), &selector, "This folder (.cvmrc)"),
        Cmd::Env { selector, shell, unset } => cmd_env(selector, shell, unset),
        Cmd::Run { selector: input, args } => launch(&resolve(&selector(&input)?, "command line")?, &args),
        Cmd::Current => cmd_current(),
        Cmd::List => cmd_list(),
        Cmd::Setup => cmd_setup(),
    }
}

fn main() {
    let mut args = env::args_os();
    let invoked_as = args
        .next()
        .and_then(|a| Path::new(&a).file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();

    let alias = read_trim(&aliases_dir().join(&invoked_as)).filter(|_| invoked_as != "cvm");
    let result = if let Some(saved) = alias {
        // Invoked through an alias command: its selector wins over everything else.
        let rest: Vec<OsString> = args.collect();
        parse_sel_text(&saved)
            .and_then(|sel| resolve(&sel, &format!("alias '{invoked_as}'")))
            .and_then(|r| launch(&r, &rest))
    } else if invoked_as == "claude" {
        let rest: Vec<OsString> = args.collect();
        resolve(&Sel::default(), "").and_then(|r| launch(&r, &rest))
    } else {
        dispatch(Cli::parse())
    };
    if let Err(e) = result {
        eprintln!("cvm: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(a: Option<&str>, v: Option<&str>) -> Sel {
        Sel { account: a.map(Into::into), version: v.map(Into::into) }
    }

    #[test]
    fn selector_forms() {
        assert_eq!(parse_sel_text("work").unwrap(), sel(Some("work"), None));
        assert_eq!(parse_sel_text("work@2.1.311").unwrap(), sel(Some("work"), Some("2.1.311")));
        assert_eq!(parse_sel_text("@stable").unwrap(), sel(None, Some("stable")));
        assert!(parse_sel_text("").is_err());
        assert!(parse_sel_text("work@").is_err());
    }

    #[test]
    fn selector_round_trips_through_text() {
        for s in ["work", "work@stable", "@2.1.311"] {
            assert_eq!(parse_sel_text(s).unwrap().text(), s);
        }
    }

    #[test]
    fn rc_parsing() {
        let rc = parse_rc("# project settings\naccount = work\nversion=\"2.1.311\"  # pinned\nother = x\n");
        assert_eq!(rc, sel(Some("work"), Some("2.1.311")));
        assert_eq!(parse_rc("version = stable"), sel(None, Some("stable")));
        assert_eq!(parse_rc("account =\nnonsense"), Sel::default());
    }

    #[test]
    fn rc_round_trips() {
        let s = sel(Some("work"), Some("stable"));
        assert_eq!(parse_rc(&format_rc(&s)), s);
    }

    #[test]
    fn merge_keeps_unnamed_parts() {
        let base = sel(Some("work"), Some("stable"));
        assert_eq!(sel(None, Some("latest")).merged_over(&base), sel(Some("work"), Some("latest")));
        assert_eq!(sel(Some("home"), None).merged_over(&base), sel(Some("home"), Some("stable")));
    }

    #[test]
    fn version_expansion_without_tags() {
        assert_eq!(expand_version("latest").unwrap(), None);
        assert_eq!(expand_version("2.1.311").unwrap(), Some("2.1.311".into()));
    }

    #[test]
    fn versions_sort_numerically() {
        let mut v = vec!["2.1.99", "2.1.311", "2.1.100"];
        v.sort_by_key(|v| version_key(v));
        assert_eq!(v, ["2.1.99", "2.1.100", "2.1.311"]);
    }

    #[test]
    fn names() {
        assert!(valid_name("alias", "cw").is_ok());
        assert!(valid_name("alias", "work-2").is_ok());
        assert!(valid_name("alias", "2fast").is_err());
        assert!(valid_name("alias", "a b").is_err());
        assert!(valid_name("alias", "").is_err());
    }
}
