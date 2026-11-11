<p align="center"><img src="assets/icon.svg" width="112" alt="cvm icon"></p>

# cvm

![version](https://img.shields.io/badge/version-0.3.0-2563eb)
![status](https://img.shields.io/badge/status-prototype-f59e0b)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-64748b)
![tested on](https://img.shields.io/badge/tested%20on-Windows-0ea5e9)
![built with Rust](https://img.shields.io/badge/built%20with-Rust-b7410e?logo=rust&logoColor=white)
![license](https://img.shields.io/badge/license-MIT-16a34a)

Account switcher for Claude Code: keep several logins on one machine and choose between them per terminal, per project or globally.

cvm puts a `claude` shim on your PATH. Every time you run `claude`, the shim picks an account, then starts the real Claude Code. Works on Windows, macOS and Linux; one small binary, no runtime.

Prototype status: CLI only, tested on Windows. Not affiliated with Anthropic.

## Accounts

An account is one Claude Code login. Each has its own config folder (`CLAUDE_CONFIG_DIR`), so its own sign-in and history. Your existing `~/.claude` is the built-in `default` account; cvm never modifies or deletes it.

`cvm profile` is an alias for `cvm account`, if you prefer that word.

## Install

```bash
cargo build --release
```

```bash
./target/release/cvm setup
```

`setup` copies `cvm`, the `claude` shim and your alias commands into `~/.cvm/bin`, and prints the PATH line for your shell. That folder must come before the real `claude` on PATH. Run `setup` again after rebuilding.

## Use

```bash
cvm account add work    # new account; shares skills, plugins, settings from ~/.claude
cvm login work          # sign in once
cvm use work            # global choice for plain `claude`
cvm alias cw work       # `cw` now starts Claude Code as work
cvm list                # accounts, who is signed in, aliases
cvm current             # what `claude` would use here, and why
```

```
  default          you@example.com
* work             you@work.example   aliases: cw
```

`cvm run work -- --continue` runs one command as an account without changing anything.

## Where the choice comes from

The first source that names an account wins:

| Order | Source | Set with |
|---|---|---|
| 1 | An alias command or `cvm run` | `cw`, `cvm run work` |
| 2 | This terminal | `eval "$(cvm env work)"` (PowerShell: `cvm env work \| Invoke-Expression`) |
| 3 | Nearest `.cvmrc`, in this folder or a parent | `cvm local work` |
| 4 | Global | `cvm use work` |
| 5 | Built-in | `default` |

If `CLAUDE_CONFIG_DIR` is already set and nothing in rows 1 or 2 names an account, cvm leaves it alone.

## `.cvmrc`

One file per project, like `.nvmrc` or `.sdkmanrc`.

```
# .cvmrc
account = work
```

An account that does not exist on this machine is skipped with a warning, and the next source is used. A shared repository can commit a `.cvmrc` without breaking teammates who name their accounts differently.

## Aliases

```bash
cvm alias cw work   # create
cvm alias           # list
cvm unalias cw      # delete
```

An alias is a saved choice and also a command. Its name works anywhere an account name does (`cvm use cw`). cvm refuses a name that would hide another command on your PATH unless you pass `--force`.

## Shared setup

`cvm account add` links these from `~/.claude` (or `--from <dir>`) into the new account, when they exist: `skills`, `agents`, `commands`, `plugins`, `CLAUDE.md`, `settings.json`, `keybindings.json`. Folders are symlinks on macOS/Linux and junctions on Windows (no admin rights needed). Use `--isolated` for an empty account.

Not shared: login, history, and anything in `.claude.json`, which includes MCP servers added at user scope. Add those once per account.

## Status line

The shim exports `CVM_ACTIVE_ACCOUNT` to Claude Code, so a status line script or shell prompt can show it.

## Debugging: pinning a build

Off by default. Claude Code updates itself and projects do not depend on a particular build, so normally every account runs whatever is installed. Pinning exists for rolling back a bad release or reproducing a bug on a specific build.

```bash
cvm version on                  # enable (cvm version off to disable)
cvm version list                # installed builds and tags
cvm version install 2.1.311     # runs `claude install`, then keeps the build
cvm version tag stable 2.1.311  # name a build
```

While it is on, a selector can carry a version, anywhere an account name is accepted:

| Selector | Means |
|---|---|
| `work@2.1.311` | account `work`, that build |
| `work@stable` | account `work`, the build tagged `stable` |
| `@latest` | account unchanged, installed build |

- `.cvmrc` accepts a second line, `version = 2.1.311`.
- Account and version resolve separately through the same five sources; `cvm use` and `cvm local` change only the parts the selector names.
- `cvm list` becomes a grid of accounts against versions, with alias names in the cells.
- Selected builds are kept in `~/.cvm/versions` (hard-linked, so no extra disk space) and run with auto-update off.
- `CVM_VERSIONS=1` turns it on for one terminal.

While it is off, version pins in `.cvmrc`, the global choice and aliases are kept but ignored, and `cvm current` says so. Tags and kept builds are not deleted.

## Limits

- A switch affects new sessions only. A running session keeps its account.
- cvm manages the `claude` CLI. The Claude desktop app keeps its own sign-in and its own Claude Code build and does not go through the shim.
- On Windows without Developer Mode, `settings.json` is shared as a hard link. If Claude Code replaces the file instead of editing it in place, the accounts' copies drift apart.
- `cvm version install` uses Claude Code's installer, which may also change the installed build.
- Built and tested on Windows only. The macOS and Linux code paths have not been run.
- `cvm account remove` deletes that account's login and history, and any aliases pointing at it. It never follows links into the shared folders.

## Layout

```
~/.cvm/
  global            global choice, same format as .cvmrc
  bin/              cvm, the claude shim, alias commands
  accounts/<name>/  one Claude Code config folder per account
  aliases/<name>    saved choices
  versions-on       present when version pinning is on
  versions/<v>/     kept Claude Code builds
  tags/<name>       version tags
```

Set `CVM_HOME` to move it.

## Development

```bash
cargo test
```

On Windows, if a build fails with `LNK1104`, the project path is too long; set `CARGO_TARGET_DIR` to a short folder.
