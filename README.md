# cvm

![version](https://img.shields.io/badge/version-0.2.0-2563eb)
![status](https://img.shields.io/badge/status-prototype-f59e0b)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-64748b)
![tested on](https://img.shields.io/badge/tested%20on-Windows-0ea5e9)
![built with Rust](https://img.shields.io/badge/built%20with-Rust-b7410e?logo=rust&logoColor=white)
![license](https://img.shields.io/badge/license-MIT-16a34a)

Account and version manager for Claude Code: nvm, but for logins as well as builds.

cvm puts a `claude` shim on your PATH. Every time you run `claude`, the shim picks an account and a version, then starts the real Claude Code. Works on Windows, macOS and Linux; one small binary, no runtime.

Prototype status: CLI only, tested on Windows. Not affiliated with Anthropic.

## The model

Two independent choices:

- **Account**: who is signed in. Each account is its own Claude Code config folder (`CLAUDE_CONFIG_DIR`) with its own login and history. Your existing `~/.claude` is the built-in `default` account; cvm never modifies or deletes it.
- **Version**: which build runs. A number (`2.1.311`), a tag you define (`stable`), or `latest`, meaning whatever Claude Code is installed.

A **selector** names one or both:

| Selector | Means |
|---|---|
| `work` | account `work`, version unchanged |
| `work@2.1.311` | account `work`, that build |
| `work@stable` | account `work`, the build tagged `stable` |
| `@latest` | account unchanged, installed build |

An **alias** is a saved selector with a short name. It is also a command.

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
cvm account add work            # new account; shares skills, plugins, settings from ~/.claude
cvm login work                  # sign in once
cvm version tag stable 2.1.311  # name a build
cvm alias cw work@stable        # `cw` now starts Claude Code as work on stable
cvm use work                    # global choice for plain `claude`
cvm list                        # accounts against versions
cvm current                     # what `claude` would use here, and why
```

`cvm list` shows the grid, with alias names in the cells and `*` on the current choice:

```
         latest   2.1.310 (old)   2.1.311 (stable)
default  .        .               .                    you@example.com
home     ch       .               *                    not signed in
work     we       .               cw                   not signed in
```

`cvm run work@2.1.310 -- --continue` runs one command with a selector without changing anything.

## Where the choice comes from

Account and version are resolved separately. For each, the first source that sets it wins:

| Order | Source | Set with |
|---|---|---|
| 1 | An alias command or `cvm run` | `cw`, `cvm run work@stable` |
| 2 | This terminal | `eval "$(cvm env work@stable)"` (PowerShell: `cvm env work@stable \| Invoke-Expression`) |
| 3 | Nearest `.cvmrc`, in this folder or a parent | `cvm local work@stable` |
| 4 | Global | `cvm use work@stable` |
| 5 | Built-in | account `default`, version `latest` |

`cvm use` and `cvm local` change only the parts the selector names. `cvm use @latest` drops a global version pin and keeps the account.

If `CLAUDE_CONFIG_DIR` is already set and nothing in rows 1 or 2 names an account, cvm leaves the account alone.

## `.cvmrc`

One file per project, like `.nvmrc` or `.sdkmanrc`. Both lines are optional.

```
# .cvmrc
account = work
version = 2.1.311
```

- `version` can be a number, a tag or `latest`.
- An account that does not exist on this machine is skipped with a warning, and the next source is used. A shared repository can commit a `.cvmrc` without breaking teammates who name their accounts differently.
- A version that is not installed is an error; cvm tells you to run `cvm version install`.

## Versions

```bash
cvm version list             # installed builds, which are kept, and their tags
cvm version install 2.1.311  # runs `claude install`, then keeps the build
cvm version tag stable 2.1.311
cvm version untag stable
```

Any build you select is kept in `~/.cvm/versions` (hard-linked, so no extra disk space), where Claude Code's own cleanup cannot remove it. Pinned builds run with auto-update off. Tags are stored by name, so moving `stable` to a new build moves every alias and `.cvmrc` that uses it.

## Aliases

```bash
cvm alias cw work@stable   # create
cvm alias                  # list
cvm unalias cw             # delete
```

An alias name works anywhere a selector does (`cvm use cw`). cvm refuses a name that would hide another command on your PATH unless you pass `--force`.

## Shared setup

`cvm account add` links these from `~/.claude` (or `--from <dir>`) into the new account, when they exist: `skills`, `agents`, `commands`, `plugins`, `CLAUDE.md`, `settings.json`, `keybindings.json`. Folders are symlinks on macOS/Linux and junctions on Windows (no admin rights needed). Use `--isolated` for an empty account.

Not shared: login, history, and anything in `.claude.json`, which includes MCP servers added at user scope. Add those once per account.

## Status line

The shim exports `CVM_ACTIVE_ACCOUNT` and `CVM_ACTIVE_VERSION` to Claude Code, so a status line script or shell prompt can show them.

## Limits

- A switch affects new sessions only. A running session keeps its account.
- cvm manages the `claude` CLI. The Claude desktop app keeps its own sign-in and its own Claude Code build and does not go through the shim.
- On Windows without Developer Mode, `settings.json` is shared as a hard link. If Claude Code replaces the file instead of editing it in place, the accounts' copies drift apart.
- `cvm version install` uses Claude Code's installer, which may also change the installed (`latest`) build.
- Built and tested on Windows only. The macOS and Linux code paths have not been run.
- `cvm account remove` deletes that account's login and history, and any aliases pointing at it. It never follows links into the shared folders.

## Layout

```
~/.cvm/
  global            global choice, same format as .cvmrc
  bin/              cvm, the claude shim, alias commands
  accounts/<name>/  one Claude Code config folder per account
  versions/<v>/     kept Claude Code builds
  tags/<name>       version tags
  aliases/<name>    saved selectors
```

Set `CVM_HOME` to move it.

## Development

```bash
cargo test
```

On Windows, if a build fails with `LNK1104`, the project path is too long; set `CARGO_TARGET_DIR` to a short folder.
