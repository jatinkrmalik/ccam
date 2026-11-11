//! cvm: profile and version manager for Claude Code.
//!
//! One binary, two roles:
//!   * invoked as `cvm`     -> management CLI
//!   * invoked as `claude`  -> shim: resolves profile + version, then runs the real Claude Code
//!
//! A profile is a Claude Code config directory (CLAUDE_CONFIG_DIR). The built-in
//! `default` profile is the untouched `~/.claude`.

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

/// Items linked from the shared source folder into every new profile.
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
const EXE: &str = if cfg!(windows) { "claude.exe" } else { "claude" };

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
fn profiles_dir() -> PathBuf {
    cvm_home().join("profiles")
}
fn bin_dir() -> PathBuf {
    cvm_home().join("bin")
}
fn versions_dir() -> PathBuf {
    cvm_home().join("versions")
}
fn pins_dir() -> PathBuf {
    cvm_home().join("pins")
}
/// Where Claude Code's native installer keeps its builds.
fn native_versions_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("claude")
        .join("versions")
}
fn profile_dir(name: &str) -> PathBuf {
    profiles_dir().join(name)
}

fn valid_name(name: &str) -> Res<()> {
    let ok = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(format!("invalid profile name '{name}' (use letters, digits, - and _)"));
    }
    Ok(())
}
fn profile_exists(name: &str) -> bool {
    name == DEFAULT || profile_dir(name).is_dir()
}
fn require_profile(name: &str) -> Res<()> {
    valid_name(name)?;
    if !profile_exists(name) {
        return Err(format!("no profile '{name}' (create it with: cvm add {name})"));
    }
    Ok(())
}
fn read_trim(p: &Path) -> Option<String> {
    let s = fs::read_to_string(p).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}
fn write_file(p: &Path, s: &str) -> Res<()> {
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    fs::write(p, format!("{s}\n")).map_err(|e| format!("{}: {e}", p.display()))
}
/// Look for `file` in the current directory and each parent.
fn find_up(file: &str) -> Option<(PathBuf, String)> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let p = dir.join(file);
        if let Some(v) = read_trim(&p) {
            return Some((p, v));
        }
        if !dir.pop() {
            return None;
        }
    }
}

// ---------------------------------------------------------------- resolution

struct Resolved {
    profile: String,
    profile_src: String,
    /// None = leave CLAUDE_CONFIG_DIR alone (default profile, or one set by the caller).
    config_dir: Option<PathBuf>,
    version: Option<String>,
    version_src: String,
}

fn resolve(forced_profile: Option<&str>) -> Res<Resolved> {
    let (profile, profile_src) = if let Some(p) = forced_profile {
        (p.to_string(), "command line".to_string())
    } else if let Some(p) = env::var("CVM_PROFILE").ok().filter(|s| !s.is_empty()) {
        (p, "CVM_PROFILE (this terminal)".into())
    } else if env::var_os("CLAUDE_CONFIG_DIR").is_some() {
        ("(external)".into(), "CLAUDE_CONFIG_DIR already set; cvm left it alone".into())
    } else if let Some((path, p)) = find_up(".claude-profile") {
        (p, path.display().to_string())
    } else if let Some(p) = read_trim(&cvm_home().join("active")) {
        (p, "global (cvm use)".into())
    } else {
        (DEFAULT.into(), "built-in default".into())
    };

    let config_dir = if profile == DEFAULT || profile == "(external)" {
        None
    } else {
        require_profile(&profile).map_err(|e| format!("{e} [selected by {profile_src}]"))?;
        Some(profile_dir(&profile))
    };

    let (version, version_src) = if let Some(v) = env::var("CVM_VERSION").ok().filter(|s| !s.is_empty()) {
        (Some(v), "CVM_VERSION (this terminal)".to_string())
    } else if let Some((path, v)) = find_up(".claude-version") {
        (Some(v), path.display().to_string())
    } else if let Some(v) = read_trim(&pins_dir().join(&profile)) {
        (Some(v), format!("pinned for profile '{profile}'"))
    } else {
        (None, "not pinned; using the installed Claude Code".into())
    };

    Ok(Resolved { profile, profile_src, config_dir, version, version_src })
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The real Claude Code on PATH, skipping our own shim.
fn find_installed_claude() -> Res<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd", "claude.bat"] } else { &["claude"] };
    let me = env::current_exe().ok();
    let shim_dir = bin_dir();
    let mut dirs: Vec<PathBuf> = env::var_os("PATH").map(|p| env::split_paths(&p).collect()).unwrap_or_default();
    dirs.push(home().join(".local").join("bin"));
    for dir in dirs {
        if same_path(&dir, &shim_dir) {
            continue;
        }
        for n in names {
            let c = dir.join(n);
            if c.is_file() && !me.as_ref().is_some_and(|m| same_path(m, &c)) {
                return Ok(c);
            }
        }
    }
    Err("could not find Claude Code on PATH (install it first: https://code.claude.com)".into())
}

fn kept_binary(v: &str) -> PathBuf {
    versions_dir().join(v).join(EXE)
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
        Some(v) => keep_version(v),
        None => find_installed_claude(),
    }
}

/// Run the real Claude Code with the resolved profile and version. Never returns on success.
fn launch(r: &Resolved, args: &[OsString]) -> Res<()> {
    let exe = real_claude(r)?;
    let mut cmd = Command::new(&exe);
    cmd.args(args).env("CVM_ACTIVE_PROFILE", &r.profile);
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

// ---------------------------------------------------------------- CLI

#[derive(Parser)]
#[command(name = "cvm", version, about = "Profile and version manager for Claude Code")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a profile (a separate Claude Code account)
    Add {
        name: String,
        /// Start empty instead of linking skills, plugins, settings etc. from the shared folder
        #[arg(long)]
        isolated: bool,
        /// Folder to share from (default: ~/.claude)
        #[arg(long, value_name = "DIR")]
        from: Option<PathBuf>,
    },
    /// List profiles
    #[command(alias = "ls")]
    List,
    /// Make a profile the global default for new `claude` sessions
    Use { name: String },
    /// Pin a profile to the current folder (writes .claude-profile)
    Local { name: String },
    /// Show which profile and version `claude` would use here, and why
    #[command(alias = "which")]
    Current,
    /// Sign a profile in (runs `claude auth login` inside it)
    Login { name: String },
    /// Run Claude Code once as a profile: cvm run work -- --continue
    Run {
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Print the shell command that switches only this terminal
    Env {
        name: Option<String>,
        /// bash, zsh, fish, powershell or cmd (default: detected)
        #[arg(long)]
        shell: Option<String>,
        /// Print the command that clears the per-terminal profile
        #[arg(long)]
        unset: bool,
    },
    /// Delete a profile, including its login and history
    #[command(alias = "rm")]
    Remove {
        name: String,
        #[arg(long, short)]
        yes: bool,
    },
    /// Install the `claude` shim into cvm's bin folder
    Setup,
    /// Manage Claude Code versions
    Version {
        #[command(subcommand)]
        cmd: VersionCmd,
    },
}

#[derive(Subcommand)]
enum VersionCmd {
    /// List available versions
    #[command(alias = "ls")]
    List,
    /// Install a version with Claude Code's own installer and keep it
    Install { version: String },
    /// Pin a version to a profile (default: the active one)
    Pin {
        version: String,
        #[arg(long)]
        profile: Option<String>,
    },
    /// Remove a profile's version pin
    Unpin {
        #[arg(long)]
        profile: Option<String>,
    },
    /// Pin a version to the current folder (writes .claude-version)
    Local { version: String },
}

fn account_email(profile: &str) -> Option<String> {
    let file = if profile == DEFAULT {
        home().join(".claude.json")
    } else {
        profile_dir(profile).join(".claude.json")
    };
    let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    json["oauthAccount"]["emailAddress"].as_str().map(str::to_string)
}

fn profile_names() -> Vec<String> {
    let mut names = vec![DEFAULT.to_string()];
    let mut found: Vec<String> = fs::read_dir(profiles_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    found.sort();
    names.extend(found);
    names
}

fn cmd_add(name: &str, isolated: bool, from: Option<PathBuf>) -> Res<()> {
    valid_name(name)?;
    if profile_exists(name) {
        return Err(format!("profile '{name}' already exists"));
    }
    let dir = profile_dir(name);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    say!("Created profile '{name}' at {}", dir.display());
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

fn cmd_list() -> Res<()> {
    let current = resolve(None).map(|r| r.profile).unwrap_or_default();
    for name in profile_names() {
        let mark = if name == current { "*" } else { " " };
        let who = account_email(&name).unwrap_or_else(|| "not signed in".into());
        let pin = read_trim(&pins_dir().join(&name)).map(|v| format!("  [claude {v}]")).unwrap_or_default();
        say!("{mark} {name:<16} {who}{pin}");
    }
    Ok(())
}

fn cmd_current() -> Res<()> {
    let r = resolve(None)?;
    say!("profile : {}  <- {}", r.profile, r.profile_src);
    let dir = match (&r.config_dir, env::var_os("CLAUDE_CONFIG_DIR")) {
        (Some(d), _) => d.clone(),
        (None, Some(d)) => PathBuf::from(d),
        (None, None) => home().join(".claude"),
    };
    say!("config  : {}", dir.display());
    say!("version : {}  <- {}", r.version.as_deref().unwrap_or("installed"), r.version_src);
    match real_claude(&r) {
        Ok(p) => say!("binary  : {}", p.display()),
        Err(e) => say!("binary  : {e}"),
    }
    Ok(())
}

fn detect_shell() -> String {
    if let Some(sh) = env::var_os("SHELL") {
        let sh = sh.to_string_lossy().to_lowercase();
        return if sh.contains("fish") { "fish".into() } else { "bash".into() };
    }
    if cfg!(windows) { "powershell".into() } else { "bash".into() }
}

fn cmd_env(name: Option<String>, shell: Option<String>, unset: bool) -> Res<()> {
    let shell = shell.unwrap_or_else(detect_shell).to_lowercase();
    let line = match (name, unset) {
        (_, true) => match shell.as_str() {
            "fish" => "set -e CVM_PROFILE;".to_string(),
            "powershell" | "pwsh" => "Remove-Item Env:CVM_PROFILE -ErrorAction SilentlyContinue".to_string(),
            "cmd" => "set CVM_PROFILE=".to_string(),
            _ => "unset CVM_PROFILE".to_string(),
        },
        (Some(n), false) => {
            require_profile(&n)?;
            match shell.as_str() {
                "fish" => format!("set -gx CVM_PROFILE {n};"),
                "powershell" | "pwsh" => format!("$env:CVM_PROFILE = '{n}'"),
                "cmd" => format!("set CVM_PROFILE={n}"),
                _ => format!("export CVM_PROFILE='{n}'"),
            }
        }
        (None, false) => return Err("give a profile name, or --unset".into()),
    };
    say!("{line}");
    Ok(())
}

fn cmd_remove(name: &str, yes: bool) -> Res<()> {
    if name == DEFAULT {
        return Err("the default profile is your normal ~/.claude; cvm will not delete it".into());
    }
    require_profile(name)?;
    if !yes {
        print!("Delete profile '{name}', including its login and history? [y/N] ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            say!("Kept.");
            return Ok(());
        }
    }
    remove_tree(&profile_dir(name)).map_err(|e| format!("could not delete: {e}"))?;
    let _ = fs::remove_file(pins_dir().join(name));
    if read_trim(&cvm_home().join("active")).as_deref() == Some(name) {
        let _ = fs::remove_file(cvm_home().join("active"));
        say!("'{name}' was the global profile; back to default.");
    }
    say!("Removed profile '{name}'.");
    Ok(())
}

fn cmd_setup() -> Res<()> {
    let bin = bin_dir();
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let me = env::current_exe().map_err(|e| e.to_string())?;
    let cvm_name = if cfg!(windows) { "cvm.exe" } else { "cvm" };
    for target in [bin.join(cvm_name), bin.join(EXE)] {
        if same_path(&me, &target) {
            continue;
        }
        let _ = fs::remove_file(&target);
        #[cfg(unix)]
        let made = if target.ends_with(EXE) {
            std::os::unix::fs::symlink(cvm_name, &target)
        } else {
            fs::copy(&me, &target).map(|_| ())
        };
        #[cfg(windows)]
        let made = fs::hard_link(&me, &target).or_else(|_| fs::copy(&me, &target).map(|_| ()));
        made.map_err(|e| format!("{}: {e}", target.display()))?;
    }
    say!("Installed cvm and the claude shim in {}", bin.display());
    let first = env::var_os("PATH")
        .and_then(|p| env::split_paths(&p).find(|d| d.join(EXE).is_file() || d.join("claude.cmd").is_file()));
    if first.as_ref().is_some_and(|d| same_path(d, &bin)) {
        say!("PATH is already set up: `claude` now goes through cvm.");
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

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn cmd_version(cmd: VersionCmd) -> Res<()> {
    let target = |profile: Option<String>| -> Res<String> {
        let p = match profile {
            Some(p) => p,
            None => resolve(None)?.profile,
        };
        require_profile(&p)?;
        Ok(p)
    };
    match cmd {
        VersionCmd::List => {
            let names = |d: PathBuf| -> Vec<String> {
                fs::read_dir(d).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok()).collect()
            };
            let kept = names(versions_dir());
            let mut all = names(native_versions_dir());
            all.extend(kept.iter().cloned());
            all.retain(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()));
            all.sort_by_key(|v| version_key(v));
            all.dedup();
            let pins: Vec<(String, String)> =
                profile_names().into_iter().filter_map(|p| read_trim(&pins_dir().join(&p)).map(|v| (p, v))).collect();
            if all.is_empty() {
                say!("No versions found. Install one with: cvm version install <version>");
            }
            for v in all {
                let k = if kept.contains(&v) { "kept" } else { "    " };
                let used: Vec<&str> = pins.iter().filter(|(_, pv)| *pv == v).map(|(p, _)| p.as_str()).collect();
                let used = if used.is_empty() { String::new() } else { format!("  <- {}", used.join(", ")) };
                say!("  {v:<12} {k}{used}");
            }
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
                say!("Note: Claude Code's installer may also have switched the unpinned `claude` to {version}.");
            }
            let p = keep_version(&version)?;
            say!("Claude Code {version} is kept at {}", p.display());
            Ok(())
        }
        VersionCmd::Pin { version, profile } => {
            let p = target(profile)?;
            keep_version(&version)?;
            write_file(&pins_dir().join(&p), &version)?;
            say!("Profile '{p}' now runs Claude Code {version}.");
            Ok(())
        }
        VersionCmd::Unpin { profile } => {
            let p = target(profile)?;
            let _ = fs::remove_file(pins_dir().join(&p));
            say!("Profile '{p}' follows the installed Claude Code again.");
            Ok(())
        }
        VersionCmd::Local { version } => {
            keep_version(&version)?;
            write_file(Path::new(".claude-version"), &version)?;
            say!("This folder now runs Claude Code {version} (.claude-version).");
            Ok(())
        }
    }
}

fn dispatch(cli: Cli) -> Res<()> {
    match cli.cmd {
        Cmd::Add { name, isolated, from } => cmd_add(&name, isolated, from),
        Cmd::List => cmd_list(),
        Cmd::Use { name } => {
            require_profile(&name)?;
            write_file(&cvm_home().join("active"), &name)?;
            say!("New `claude` sessions now use profile '{name}'.");
            Ok(())
        }
        Cmd::Local { name } => {
            require_profile(&name)?;
            write_file(Path::new(".claude-profile"), &name)?;
            say!("This folder now uses profile '{name}' (.claude-profile).");
            Ok(())
        }
        Cmd::Current => cmd_current(),
        Cmd::Login { name } => {
            require_profile(&name)?;
            launch(&resolve(Some(&name))?, &["auth".into(), "login".into()])
        }
        Cmd::Run { name, args } => {
            require_profile(&name)?;
            launch(&resolve(Some(&name))?, &args)
        }
        Cmd::Env { name, shell, unset } => cmd_env(name, shell, unset),
        Cmd::Remove { name, yes } => cmd_remove(&name, yes),
        Cmd::Setup => cmd_setup(),
        Cmd::Version { cmd } => cmd_version(cmd),
    }
}

fn main() {
    let mut args = env::args_os();
    let invoked_as = args
        .next()
        .and_then(|a| Path::new(&a).file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();

    let result = if invoked_as == "claude" {
        let rest: Vec<OsString> = args.collect();
        resolve(None).and_then(|r| launch(&r, &rest))
    } else {
        dispatch(Cli::parse())
    };
    if let Err(e) = result {
        eprintln!("cvm: {e}");
        std::process::exit(1);
    }
}
