<p align="center"><img src="assets/icon.svg" width="112" alt="ccam icon"></p>

# ccam

![version](https://img.shields.io/badge/version-0.4.0-2563eb)
![status](https://img.shields.io/badge/status-prototype-f59e0b)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-64748b)
![tested on](https://img.shields.io/badge/tested%20on-Windows-0ea5e9)
![built with Rust](https://img.shields.io/badge/built%20with-Rust-b7410e?logo=rust&logoColor=white)
![license](https://img.shields.io/badge/license-MIT-16a34a)

ccam (Claude Code account manager) is an account switcher for Claude Code: keep several logins on one machine and choose between them per terminal, per project or globally.

ccam puts a `claude` shim on your PATH. Every time you run `claude`, the shim picks an account, then starts the real Claude Code. Works on Windows, macOS and Linux; one small binary, no runtime.

Prototype status: CLI only, tested on Windows. Not affiliated with Anthropic.

## Accounts

An account is one Claude Code login. Each has its own config folder (`CLAUDE_CONFIG_DIR`), so its own sign-in and history. Your existing `~/.claude` is the built-in `default` account; ccam never modifies or deletes it.

`ccam profile` is an alias for `ccam account`, if you prefer that word.

## Install

```bash
cargo build --release
```

```bash
./target/release/ccam setup
```

`setup` copies `ccam`, the `claude` shim and your alias commands into `~/.ccam/bin`, and prints the PATH line for your shell. That folder must come before the real `claude` on PATH. Run `setup` again after rebuilding.

## Use

```bash
ccam account add work    # new account; shares skills, plugins, settings from ~/.claude
ccam login work          # sign in once
ccam use work            # global choice for plain `claude`
ccam alias cw work       # `cw` now starts Claude Code as work
ccam list                # accounts, who is signed in, aliases
ccam current             # what `claude` would use here, and why
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

## Limits

- A switch affects new sessions only. A running session keeps its account.
- ccam manages the `claude` CLI. The Claude desktop app keeps its own sign-in and its own Claude Code build and does not go through the shim.
- On Windows without Developer Mode, `settings.json` is shared as a hard link. If Claude Code replaces the file instead of editing it in place, the accounts' copies drift apart.
- `ccam version install` uses Claude Code's installer, which may also change the installed build.
- Built and tested on Windows only. The macOS and Linux code paths have not been run.
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

```bash
cargo test
```

On Windows, if a build fails with `LNK1104`, the project path is too long; set `CARGO_TARGET_DIR` to a short folder.
