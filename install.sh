#!/bin/sh
# ccam installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/jatinkrmalik/ccam/main/install.sh | sh
#
# Downloads the latest release binary, verifies its checksum, runs `ccam setup`
# and puts ~/.ccam/bin at the front of PATH in your shell's startup file.
#
# Options (environment variables):
#   CCAM_VERSION=v0.4.0     install a specific release instead of the latest
#   CCAM_NO_MODIFY_PATH=1   do not touch any shell startup file
#   CCAM_HOME=/some/dir     install somewhere other than ~/.ccam
set -eu

REPO="jatinkrmalik/ccam"

say() { printf '%s\n' "$*"; }
fail() { printf 'ccam install: %s\n' "$*" >&2; exit 1; }

fetch() { # fetch <url> <output file>
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        fail "need curl or wget"
    fi
}

latest_version() {
    if command -v curl >/dev/null 2>&1; then
        url=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
    else
        url=$(wget -q -S --spider "https://github.com/$REPO/releases/latest" 2>&1 |
            sed -n 's/^ *[Ll]ocation: *//p' | tail -n 1 | tr -d '\r')
    fi
    version=${url##*/}
    case "$version" in
        v[0-9]*) printf '%s' "$version" ;;
        *) fail "could not find the latest release" ;;
    esac
}

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Linux)
        case "$arch" in
            x86_64 | amd64) target="x86_64-unknown-linux-musl" ;;
            *) fail "no prebuilt binary for Linux $arch yet; install with: cargo install ccam" ;;
        esac
        ;;
    Darwin)
        case "$arch" in
            arm64 | aarch64) target="aarch64-apple-darwin" ;;
            x86_64) target="x86_64-apple-darwin" ;;
            *) fail "unsupported Mac architecture: $arch" ;;
        esac
        ;;
    *)
        fail "this script is for macOS and Linux. On Windows, run in PowerShell: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
        ;;
esac

version=${CCAM_VERSION:-}
[ -n "$version" ] || version=$(latest_version)

name="ccam-$version-$target"
archive="$name.tar.gz"
base="https://github.com/$REPO/releases/download/$version"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

say "Downloading ccam $version ($target)"
fetch "$base/$archive" "$tmp/$archive"
fetch "$base/$archive.sha256" "$tmp/$archive.sha256"

(
    cd "$tmp"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$archive.sha256" >/dev/null
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$archive.sha256" >/dev/null
    else
        say "warning: no sha256sum or shasum found; skipping the checksum"
    fi
) || fail "checksum did not match; nothing was installed"

tar -xzf "$tmp/$archive" -C "$tmp"
"$tmp/$name/ccam" setup >/dev/null

bin="${CCAM_HOME:-$HOME/.ccam}/bin"
say "Installed ccam $version in $bin"

line="export PATH=\"$bin:\$PATH\""
rc=""
case "${SHELL:-}" in
    */zsh) rc="$HOME/.zshrc" ;;
    */bash) if [ "$os" = "Darwin" ]; then rc="$HOME/.bash_profile"; else rc="$HOME/.bashrc"; fi ;;
    */fish)
        rc="$HOME/.config/fish/config.fish"
        line="fish_add_path --prepend \"$bin\""
        ;;
    *) rc="$HOME/.profile" ;;
esac

if [ -n "${CCAM_NO_MODIFY_PATH:-}" ]; then
    say "Add this to your shell startup file:"
    say "  $line"
elif [ -f "$rc" ] && grep -F "$bin" "$rc" >/dev/null 2>&1; then
    say "PATH is already set in $rc"
else
    mkdir -p "$(dirname "$rc")"
    printf '\n# ccam: Claude Code account manager\n%s\n' "$line" >>"$rc"
    say "Added $bin to PATH in $rc"
fi

say ""
say "Open a new terminal, then:"
say "  ccam account add work"
say "  ccam login work"
say "  ccam use work"
