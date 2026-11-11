# cvm

Profile and version manager for Claude Code: nvm, but for accounts.

Each profile is its own Claude Code config folder (`CLAUDE_CONFIG_DIR`), so it has its own login, history and `.claude.json`. cvm also puts a `claude` shim on your PATH that picks the right profile and version every time you run `claude`. Works on Windows, macOS and Linux; one small binary, no runtime.

Prototype status: CLI only. Not affiliated with Anthropic.

## Install

```bash
cargo build --release
```

```bash
./target/release/cvm setup
```

`setup` copies `cvm` and the `claude` shim into `~/.cvm/bin` and prints the PATH line to add for your shell. That folder must come before the real `claude` on PATH.

On Windows, if the build fails with `LNK1104`, the project path is too long; set `CARGO_TARGET_DIR` to a short folder.

## Use

```bash
cvm add work            # new profile; shares skills, plugins, settings from ~/.claude
cvm login work          # sign in once
cvm use work            # all new `claude` sessions use it
cvm list                # profiles, who is signed in, pinned versions
cvm current             # what `claude` would use here, and why
```

Your existing `~/.claude` is the built-in `default` profile and is never modified or deleted.

### Three ways to choose a profile

| Scope | Command | Stored in |
|---|---|---|
| This terminal only | `eval "$(cvm env work)"` (PowerShell: `cvm env work \| Invoke-Expression`) | `CVM_PROFILE` |
| This folder and below | `cvm local work` | `.claude-profile` |
| Everywhere else | `cvm use work` | `~/.cvm/active` |

The first match wins, in that order. If `CLAUDE_CONFIG_DIR` is already set and `CVM_PROFILE` is not, cvm leaves it alone. `cvm run work -- --continue` runs one command as a profile without switching anything.

### Versions

```bash
cvm version list                      # builds Claude Code has installed, and which are kept
cvm version install 2.1.311           # runs `claude install`, then keeps the build
cvm version pin 2.1.311 --profile work
cvm version local 2.1.311             # writes .claude-version for this folder
cvm version unpin --profile work
```

Order: `CVM_VERSION`, then `.claude-version`, then the profile's pin, then whatever Claude Code is installed. Pinned builds are kept in `~/.cvm/versions` (hard-linked, so no extra disk space) so Claude Code's own cleanup cannot remove them, and they run with auto-update off.

## Shared setup

`cvm add` links these from `~/.claude` (or `--from <dir>`) into the new profile, when they exist: `skills`, `agents`, `commands`, `plugins`, `CLAUDE.md`, `settings.json`, `keybindings.json`. Folders are symlinks on macOS/Linux and junctions on Windows (no admin rights needed). Use `--isolated` for an empty profile.

Not shared: login, history, and anything in `.claude.json`, which includes MCP servers added at user scope. Add those once per profile.

## Limits

- A switch affects new sessions only. A running session keeps its account.
- It manages the `claude` CLI. The Claude desktop app signs in separately.
- On Windows without Developer Mode, `settings.json` is shared as a hard link. If Claude Code replaces the file instead of editing it in place, the profiles' copies drift apart.
- `cvm version install` uses Claude Code's installer, which may also change the version the unpinned `claude` runs.
- Built and tested on Windows only so far. The macOS and Linux paths compile-guard on `cfg(unix)` and have not been run.
- `cvm remove` deletes a profile's login and history. It never follows links into the shared folders.

## Layout

```
~/.cvm/
  active            global profile name
  bin/              cvm + claude shim
  profiles/<name>/  one Claude Code config folder per profile
  pins/<name>       version pinned to a profile
  versions/<v>/     kept Claude Code builds
```

Set `CVM_HOME` to move it.
