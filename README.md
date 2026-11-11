<p align="center"><img src="assets/icon.svg" width="112" alt="ccam icon: two overlapping circles"></p>

<h1 align="center">ccam: switch between multiple Claude Code accounts</h1>

<p align="center">
  <a href="https://github.com/jatinkrmalik/ccam/actions/workflows/ci.yml"><img src="https://github.com/jatinkrmalik/ccam/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/jatinkrmalik/ccam/releases/latest"><img src="https://img.shields.io/github/v/release/jatinkrmalik/ccam?color=d97757" alt="Latest release"></a>
  <a href="https://crates.io/crates/ccam"><img src="https://img.shields.io/crates/v/ccam?color=b7410e" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/ccam-cli"><img src="https://img.shields.io/npm/v/ccam-cli?color=cb3837" alt="npm"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-64748b" alt="Windows, macOS, Linux">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-16a34a" alt="MIT license"></a>
</p>

**ccam** (Claude Code account manager) lets you use multiple Claude Code accounts on one machine and switch between them with one command. Keep a work login and a personal login side by side, and choose which one `claude` uses per terminal, per project or globally. It is like nvm, but for Claude Code logins.

```bash
ccam account add work   # a second Claude Code login
ccam login work         # sign in once
ccam use work           # plain `claude` now runs as work
```

- **No more logging out and back in.** Every account stays signed in, with its own history.
- **Per-project accounts.** Drop a `.ccamrc` in a repository and `claude` picks the right account there.
- **One-word commands.** `ccam alias cw work` makes `cw` start Claude Code as your work account.
- **Shared setup.** Skills, agents, commands, plugins and settings are shared across accounts.
- **Cross-platform.** One small Rust binary for Windows, macOS and Linux; no runtime needed.

ccam puts a `claude` shim on your PATH. Every time you run `claude`, the shim picks an account, then starts the real Claude Code with that account's config folder (`CLAUDE_CONFIG_DIR`).

Status: early release, CLI only. Not affiliated with Anthropic.

## Accounts

An account is one Claude Code login. Each has its own config folder (`CLAUDE_CONFIG_DIR`), so its own sign-in and history. Your existing `~/.claude` is the built-in `default` account; ccam never modifies or deletes it.

`ccam profile` is an alias for `ccam account`, if you prefer that word.

## Install

**macOS and Linux**

```bash
curl -fsSL https://raw.githubusercontent.com/jatinkrmalik/ccam/main/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/jatinkrmalik/ccam/main/install.ps1 | iex
```

Both download the latest release, verify its checksum, install into `~/.ccam/bin` and put that folder at the front of your PATH. Open a new terminal afterwards.

**Other ways**

| Method | Command | Then |
|---|---|---|
| Homebrew (macOS, Linux) | `brew install jatinkrmalik/ccam/ccam` | `ccam setup` |
| npm | `npm install -g ccam-cli` | `ccam setup` |
| Cargo | `cargo install ccam` | `ccam setup` |
| Manual | Download from the [releases page](https://github.com/jatinkrmalik/ccam/releases) and unpack | `ccam setup` |

`ccam setup` copies `ccam`, the `claude` shim and your alias commands into `~/.ccam/bin`, and prints the PATH line for your shell. That folder must come before the real `claude` on PATH; `ccam doctor --fix` sets that up for you. Run `ccam setup` again after upgrading.

You need Claude Code itself installed first; ccam switches accounts, it does not install Claude Code.

## Use

```bash
ccam account add work    # new account; shares skills, plugins, settings from ~/.claude
ccam login work          # sign in once
ccam use work            # global choice for plain `claude`
ccam alias cw work       # `cw` now starts Claude Code as work
ccam list                # accounts, who is signed in, aliases
ccam current             # what `claude` would use here, and why
ccam doctor              # check the setup; `ccam doctor --fix` repairs it
```

```
  default          you@example.com
* work             you@work.example   aliases: cw
```

`ccam run work -- --continue` runs one command as an account without changing anything.

## Where the choice comes from

The first source that names an account wins:

| Order | Source | Set with |
|---|---|---|
| 1 | An alias command or `ccam run` | `cw`, `ccam run work` |
| 2 | This terminal | `eval "$(ccam env work)"` (PowerShell: `ccam env work \| Invoke-Expression`) |
| 3 | Nearest `.ccamrc`, in this folder or a parent | `ccam local work` |
| 4 | Global | `ccam use work` |
| 5 | Built-in | `default` |

If `CLAUDE_CONFIG_DIR` is already set and nothing in rows 1 or 2 names an account, ccam leaves it alone.

## `.ccamrc`

One file per project, like `.nvmrc` or `.sdkmanrc`.

```
# .ccamrc
account = work
```

An account that does not exist on this machine is skipped with a warning, and the next source is used. A shared repository can commit a `.ccamrc` without breaking teammates who name their accounts differently.

## Aliases

```bash
ccam alias cw work   # create
ccam alias           # list
ccam unalias cw      # delete
```

An alias is a saved choice and also a command. Its name works anywhere an account name does (`ccam use cw`). ccam refuses a name that would hide another command on your PATH unless you pass `--force`.

## Shared setup

`ccam account add` links these from `~/.claude` (or `--from <dir>`) into the new account, when they exist: `skills`, `agents`, `commands`, `plugins`, `CLAUDE.md`, `settings.json`, `keybindings.json`. Folders are symlinks on macOS/Linux and junctions on Windows (no admin rights needed). Use `--isolated` for an empty account.

Not shared: login, history, and anything in `.claude.json`, which includes MCP servers added at user scope. Add those once per account.

## Status line

The shim exports `CCAM_ACTIVE_ACCOUNT` to Claude Code, so a status line script or shell prompt can show it.

## Debugging: pinning a build

Off by default. Claude Code updates itself and projects do not depend on a particular build, so normally every account runs whatever is installed. Pinning exists for rolling back a bad release or reproducing a bug on a specific build.

```bash
ccam version on                  # enable (ccam version off to disable)
ccam version list                # installed builds and tags
ccam version install 2.1.311     # runs `claude install`, then keeps the build
ccam version tag stable 2.1.311  # name a build
```

While it is on, a selector can carry a version, anywhere an account name is accepted:

| Selector | Means |
|---|---|
| `work@2.1.311` | account `work`, that build |
| `work@stable` | account `work`, the build tagged `stable` |
| `@latest` | account unchanged, installed build |

- `.ccamrc` accepts a second line, `version = 2.1.311`.
- Account and version resolve separately through the same five sources; `ccam use` and `ccam local` change only the parts the selector names.
- `ccam list` becomes a grid of accounts against versions, with alias names in the cells.
- Selected builds are kept in `~/.ccam/versions` (hard-linked, so no extra disk space) and run with auto-update off.
- `CCAM_VERSIONS=1` turns it on for one terminal.

While it is off, version pins in `.ccamrc`, the global choice and aliases are kept but ignored, and `ccam current` says so. Tags and kept builds are not deleted.

## Troubleshooting

```bash
ccam doctor
```

`ccam doctor` checks the whole setup and says what to do about anything that is wrong:

- ccam and the `claude` shim are installed and up to date
- `~/.ccam/bin` is on PATH, ahead of the real `claude`
- Claude Code itself is installed
- each account is signed in and its shared folders still exist
- aliases and the nearest `.ccamrc` point at accounts that exist

```
  ok    ccam is installed in /home/you/.ccam/bin
  ok    the `claude` shim is in place
  FAIL  PATH: /usr/local/bin has a `claude` and comes before /home/you/.ccam/bin
        fix it with: ccam doctor --fix
  ok    Claude Code 2.1.316 at /usr/local/bin/claude
  ok    account default: signed in as you@example.com
  warn  account work: not signed in
        run: ccam login work
```

`ccam doctor --fix` repairs what it can: it reinstalls the shim and alias commands, and puts `~/.ccam/bin` at the front of PATH. On Windows it changes your user PATH; on macOS and Linux it adds one line to your shell's startup file (`.zshrc`, `.bashrc`, `.bash_profile`, fish's `config.fish`, or `.profile`). Plain `ccam doctor` never changes anything.

## FAQ

### How do I use two Claude accounts with Claude Code?

Install ccam, then run `ccam account add work` and `ccam login work`. Your original login stays as the `default` account. Switch with `ccam use work` and `ccam use default`, or give each one a command with `ccam alias`.

### Can I run two Claude Code accounts at the same time?

Yes. Each terminal can use a different account: run `cw` in one and `claude` in another, or use `ccam run work` for a single session. A running session keeps the account it started with.

### How do I set a Claude Code account per project?

Run `ccam local work` in the project folder. It writes a `.ccamrc` file, and `claude` uses that account in the folder and everything below it.

### Does ccam store or see my Claude credentials?

No. ccam only sets `CLAUDE_CONFIG_DIR`, the setting Claude Code itself provides for moving its config folder. Sign-in is done by Claude Code, and the credentials stay where Claude Code keeps them.

### Does it work with the Claude desktop app?

No. The desktop app keeps its own sign-in and does not go through the `claude` command. ccam manages the Claude Code CLI.

### How do I uninstall it?

Remove `~/.ccam/bin` from your PATH and delete `~/.ccam`. That also deletes the logins and history of the accounts you added with ccam. Your `~/.claude` is untouched.

## Limits

- A switch affects new sessions only. A running session keeps its account.
- ccam manages the `claude` CLI. The Claude desktop app keeps its own sign-in and its own Claude Code build and does not go through the shim.
- On Windows without Developer Mode, `settings.json` is shared as a hard link. If Claude Code replaces the file instead of editing it in place, the accounts' copies drift apart.
- `ccam version install` uses Claude Code's installer, which may also change the installed build.
- Used day to day on Windows only so far. macOS and Linux are built and tested automatically, but have had little real use; please report problems.
- `ccam account remove` deletes that account's login and history, and any aliases pointing at it. It never follows links into the shared folders.

## Layout

```
~/.ccam/
  global            global choice, same format as .ccamrc
  bin/              ccam, the claude shim, alias commands
  accounts/<name>/  one Claude Code config folder per account
  aliases/<name>    saved choices
  versions-on       present when version pinning is on
  versions/<v>/     kept Claude Code builds
  tags/<name>       version tags
```

Set `CCAM_HOME` to move it.

## Development

Build from source with Rust:

```bash
cargo build --release
```

```bash
cargo test
```

On macOS and Linux, the end-to-end test runs every command against a throwaway home folder, with a stand-in for Claude Code:

```bash
bash tests/e2e.sh target/release/ccam
```

On Windows, if a build fails with `LNK1104`, the project path is too long; set `CARGO_TARGET_DIR` to a short folder.

### Releasing

1. Set the same new version in `Cargo.toml` and `npm/package.json`, run `cargo build` to refresh `Cargo.lock`, and merge that to `main`.
2. Create the release as a pre-release: `gh release create vX.Y.Z --prerelease --target main --title "ccam X.Y.Z" --notes-file notes.md`.

The release workflow then builds and attaches the binaries, publishes to crates.io and npm, and marks the release as latest. Starting as a pre-release keeps the installers on the previous version until the new binaries are attached. The [Homebrew tap](https://github.com/jatinkrmalik/homebrew-ccam) picks the new release up within an hour.
