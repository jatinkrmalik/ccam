# ccam

Switch between multiple Claude Code accounts on one machine. Like nvm, for Claude Code logins: choose an account per terminal, per project or globally. Works on Windows, macOS and Linux.

```bash
npm install -g ccam-cli
ccam setup
```

`ccam setup` puts a `claude` shim in `~/.ccam/bin` and prints the PATH line to add. Then:

```bash
ccam account add work   # a second login, sharing your skills, plugins and settings
ccam login work         # sign in once
ccam use work           # plain `claude` now runs as work
ccam alias cw work      # or: `cw` starts Claude Code as work
```

This package downloads the prebuilt `ccam` binary for your platform from the matching [GitHub release](https://github.com/jatinkrmalik/ccam/releases) and verifies its checksum.

Full documentation: https://github.com/jatinkrmalik/ccam

Not affiliated with Anthropic. MIT license.
