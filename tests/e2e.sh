#!/bin/bash
# End-to-end test of ccam on Linux and macOS.
#
#   tests/e2e.sh target/release/ccam
#
# Runs entirely inside a throwaway home folder: your own ~/.ccam, ~/.claude and
# shell startup files are never read or changed. Claude Code is not needed; a
# stand-in `claude` script reports which account and config folder it was given.
set -u
[ $# -eq 1 ] || { echo "usage: $0 <path to ccam binary>" >&2; exit 2; }
BIN_SRC="$1"
[ -f "$BIN_SRC" ] || { echo "$BIN_SRC: no such file" >&2; exit 2; }
T="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/realbin"
# bash reads .bash_profile on macOS and .bashrc on Linux; ccam follows the same rule.
if [ "$(uname -s)" = "Darwin" ]; then BASHRC=.bash_profile; else BASHRC=.bashrc; fi
pass=0; fail=0
ok()   { pass=$((pass+1)); echo "  PASS  $1"; }
bad()  { fail=$((fail+1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2"; }
check(){ # check "<name>" "<actual>" "<expected substring>"
  case "$2" in *"$3"*) ok "$1" ;; *) bad "$1" "wanted '$3' in: $2" ;; esac; }
section(){ echo; echo "== $1"; }

H="$T/home"; mkdir -p "$H"
cp "$BIN_SRC" "$T/ccam-main"; chmod +x "$T/ccam-main"
cat > "$T/realbin/claude" <<'EOF'
#!/bin/sh
if [ "$1" = "--version" ]; then echo "9.9.9 (Claude Code)"; exit 0; fi
echo "FAKE config=${CLAUDE_CONFIG_DIR:-unset} account=${CCAM_ACTIVE_ACCOUNT:-none} args=$*"
[ "$1" = "exit7" ] && exit 7
exit 0
EOF
chmod +x "$T/realbin/claude"
BASEPATH="$T/realbin:/usr/bin:/bin"
C="$T/ccam-main"
e() { env -i HOME="$H" SHELL=/bin/bash PATH="$P" "$@"; }   # run with the test environment

section "1. doctor before anything is set up"
P="$BASEPATH"
out=$(e "$C" doctor 2>&1); code=$?
echo "$out" | sed 's/^/    /'
check "reports not installed" "$out" "is not installed"
check "reports PATH missing" "$out" "is not on PATH"
check "finds the real claude" "$out" "Claude Code 9.9.9 at $T/realbin/claude"
[ $code -eq 1 ] && ok "exit code 1" || bad "exit code 1" "got $code"

section "2. doctor --fix"
out=$(e "$C" doctor --fix 2>&1); code=$?
echo "$out" | sed 's/^/    /'
[ -x "$H/.ccam/bin/ccam" ] && ok "ccam installed in bin" || bad "ccam installed in bin"
[ -L "$H/.ccam/bin/claude" ] && ok "claude shim symlink created" || bad "claude shim symlink created"
check "$BASHRC got the PATH line" "$(cat "$H/$BASHRC" 2>&1)" "export PATH=\"$H/.ccam/bin:\$PATH\""
[ $code -eq 0 ] && ok "exit code 0 after fix" || bad "exit code 0 after fix" "got $code"
found=$(env -i HOME="$H" PATH="$BASEPATH" bash -c '. ~/'"$BASHRC"'; command -v claude')
check "a shell that reads $BASHRC finds the shim first" "$found" "$H/.ccam/bin/claude"
e "$C" doctor --fix >/dev/null 2>&1
check "--fix twice adds the line once" "$(grep -c '.ccam/bin' "$H/$BASHRC")" "1"

section "3. doctor when healthy"
P="$H/.ccam/bin:$BASEPATH"
out=$(e ccam doctor 2>&1); code=$?
echo "$out" | sed 's/^/    /'
check "all good" "$out" "Everything looks good."
[ $code -eq 0 ] && ok "exit code 0" || bad "exit code 0" "got $code"

section "4. real claude ahead on PATH"
P="$BASEPATH:$H/.ccam/bin"
out=$(e "$C" doctor 2>&1)
check "reports shadowing" "$out" "$T/realbin has a \`claude\` and comes before"
P="$H/.ccam/bin:$BASEPATH"

section "5. accounts and shared setup"
mkdir -p "$H/.claude/skills/demo" "$H/.claude/agents" "$H/.claude/plugins"
echo "# rules" > "$H/.claude/CLAUDE.md"; echo '{}' > "$H/.claude/settings.json"
out=$(e ccam account add work 2>&1); echo "$out" | sed 's/^/    /'
W="$H/.ccam/accounts/work"
for item in skills agents plugins CLAUDE.md settings.json; do
  [ -L "$W/$item" ] && ok "$item is a symlink" || bad "$item is a symlink" "$(ls -la "$W" 2>&1)"
done
[ -d "$W/skills/demo" ] && ok "shared skills readable through the link" || bad "shared skills readable through the link"
[ ! -e "$W/keybindings.json" ] && ok "absent items are not linked" || bad "absent items are not linked"
check "duplicate account refused" "$(e ccam account add work 2>&1)" "already exists"
check "bad name refused" "$(e ccam account add 9bad 2>&1)" "invalid account name"

section "6. the claude shim"
check "default account leaves config alone" "$(e claude hello 2>&1)" "FAKE config=unset account=default args=hello"
e ccam use work >/dev/null
check "global choice applies" "$(e claude hello world 2>&1)" "FAKE config=$W account=work args=hello world"
e claude exit7 >/dev/null 2>&1; code=$?
[ $code -eq 7 ] && ok "exit code passes through the shim" || bad "exit code passes through the shim" "got $code"
check "arguments with spaces survive" "$(e claude "two words" -x 2>&1)" "args=two words -x"
e ccam use default >/dev/null
check "back to default" "$(e claude 2>&1)" "account=default"

section "7. aliases"
out=$(e ccam alias cw work 2>&1); echo "$out" | sed 's/^/    /'
[ -L "$H/.ccam/bin/cw" ] && ok "alias command is a symlink" || bad "alias command is a symlink" "$(ls -la "$H/.ccam/bin")"
check "alias command runs as work" "$(e cw --continue 2>&1)" "config=$W account=work args=--continue"
check "plain claude still default" "$(e claude 2>&1)" "account=default"
check "alias refused when it would hide a command" "$(e ccam alias ls work 2>&1)" "ccam:"
[ ! -e "$H/.ccam/bin/ls" ] && ok "no ls command was created" || bad "no ls command was created"
check "alias name usable as selector" "$(e ccam use cw 2>&1; e claude 2>&1)" "account=work"
e ccam use default >/dev/null

section "8. per-project .ccamrc"
mkdir -p "$T/proj/sub/deep"
( cd "$T/proj" && e ccam local work >/dev/null )
check ".ccamrc written" "$(cat "$T/proj/.ccamrc" 2>&1)" "account = work"
check "used in the project folder" "$(cd "$T/proj" && e claude 2>&1)" "account=work"
check "used in a nested folder" "$(cd "$T/proj/sub/deep" && e claude 2>&1)" "account=work"
check "not used outside" "$(cd "$T" && e claude 2>&1)" "account=default"
printf 'account = ghost\n' > "$T/proj/sub/.ccamrc"
out=$(cd "$T/proj/sub" && e claude 2>&1)
check "unknown account in .ccamrc warns" "$out" "does not exist here"
check "and falls through" "$out" "account=default"
check "doctor flags it" "$(cd "$T/proj/sub" && e ccam doctor 2>&1)" "names account 'ghost'"
rm "$T/proj/sub/.ccamrc"

section "9. per-terminal and one-off"
envline=$(e ccam env work --shell bash 2>&1); echo "    $envline"
check "env prints an export" "$envline" "export CCAM_ACCOUNT='work'"
check "terminal choice applies" "$(env -i HOME="$H" SHELL=/bin/bash PATH="$P" CCAM_ACCOUNT=work claude 2>&1)" "account=work"
check "terminal choice beats .ccamrc" "$(cd "$T/proj" && env -i HOME="$H" SHELL=/bin/bash PATH="$P" CCAM_ACCOUNT=default claude 2>&1)" "account=default"
check "ccam run" "$(e ccam run work -- --resume x 2>&1)" "config=$W account=work args=--resume x"
e ccam run work -- exit7 >/dev/null 2>&1; code=$?
[ $code -eq 7 ] && ok "ccam run passes the exit code" || bad "ccam run passes the exit code" "got $code"
check "existing CLAUDE_CONFIG_DIR is left alone" "$(env -i HOME="$H" SHELL=/bin/bash PATH="$P" CLAUDE_CONFIG_DIR=/somewhere claude 2>&1)" "config=/somewhere"
check "fish syntax" "$(e ccam env work --shell fish 2>&1)" "set -gx CCAM_ACCOUNT"

section "10. list, current, sign-in display"
printf '{"oauthAccount":{"emailAddress":"work@example.com"}}' > "$W/.claude.json"
out=$(e ccam list 2>&1); echo "$out" | sed 's/^/    /'
check "list shows the signed-in address" "$out" "work@example.com"
check "list shows the alias" "$out" "cw"
out=$(cd "$T/proj" && e ccam current 2>&1); echo "$out" | sed 's/^/    /'
check "current names the source" "$out" ".ccamrc"
check "current shows the real binary" "$out" "$T/realbin/claude"

section "11. doctor with problems"
rm -rf "$H/.claude/agents"
rm -f "$H/.ccam/bin/cw"
out=$(e ccam doctor 2>&1); code=$?; echo "$out" | sed 's/^/    /'
check "broken shared link reported" "$out" "shared agents no longer point anywhere"
check "missing alias command reported" "$out" "alias cw has no command"
[ $code -eq 0 ] && ok "warnings only: exit 0" || bad "warnings only: exit 0" "got $code"
e ccam doctor --fix >/dev/null 2>&1
[ -L "$H/.ccam/bin/cw" ] && ok "--fix restores the alias command" || bad "--fix restores the alias command"

section "12. removing an account"
out=$(e ccam account remove work --yes 2>&1); echo "$out" | sed 's/^/    /'
[ ! -e "$W" ] && ok "account folder deleted" || bad "account folder deleted"
[ -d "$H/.claude/skills/demo" ] && ok "shared skills folder untouched" || bad "shared skills folder untouched" "the delete followed a link"
[ -f "$H/.claude/CLAUDE.md" ] && ok "shared CLAUDE.md untouched" || bad "shared CLAUDE.md untouched"
[ ! -e "$H/.ccam/bin/cw" ] && ok "alias command removed with it" || bad "alias command removed with it"
check "default cannot be removed" "$(e ccam account remove default --yes 2>&1)" "will not delete"
check "project now falls back with a warning" "$(cd "$T/proj" && e claude 2>&1)" "account=default"

section "13. doctor --fix for other shells"
for sh in zsh fish sh; do
  HH="$T/home-$sh"; mkdir -p "$HH"
  env -i HOME="$HH" SHELL="/usr/bin/$sh" PATH="$BASEPATH" "$C" doctor --fix >/dev/null 2>&1
  case $sh in zsh) f="$HH/.zshrc" ;; fish) f="$HH/.config/fish/config.fish" ;; sh) f="$HH/.profile" ;; esac
  check "$sh: line written to $(basename "$f")" "$(cat "$f" 2>&1)" "$HH/.ccam/bin"
done
check "fish uses fish_add_path" "$(cat "$T/home-fish/.config/fish/config.fish")" "fish_add_path --prepend"
if command -v zsh >/dev/null; then
  found=$(env -i HOME="$T/home-zsh" PATH="$BASEPATH" zsh -c '. ~/.zshrc; command -v claude')
  check "zsh that reads .zshrc finds the shim first" "$found" "$T/home-zsh/.ccam/bin/claude"
fi

section "14. version pinning switch"
check "off by default" "$(e ccam run default@1.2.3 2>&1)" "version pinning is off"
e ccam version on >/dev/null 2>&1
check "can be turned on" "$(e ccam doctor 2>&1)" "version pinning is on"
e ccam version off >/dev/null 2>&1

echo
echo "RESULT: $pass passed, $fail failed"
[ $fail -eq 0 ]
