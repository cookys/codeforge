#!/bin/sh
# codeforge installer — POSIX sh
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/cookys/codeforge/main/install.sh | sh
#
# Env vars:
#   CODEFORGE_VERSION       Override version (e.g. v0.0.2). Default: latest.
#   CODEFORGE_INSTALL_DIR   Install to this dir. Default: $CARGO_HOME/bin,
#                           else ~/.cargo/bin (if it exists), else
#                           ~/.local/bin (created if missing).
#   CODEFORGE_FORCE         Set to 1 to reinstall even if same version
#                           is already present.

set -eu

REPO="cookys/codeforge"

# ─── helpers ────────────────────────────────────────────────────────────

say() { printf '%s\n' "$*" >&2; }
err() { say "error: $*"; exit 1; }
need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "$1 not found. $2"
}
sha256_check() {
    expected_line="$1"
    file="$2"
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        err "neither sha256sum nor shasum -a 256 found; install one of them."
    fi
    expected=$(printf '%s' "$expected_line" | awk '{print $1}')
    if [ "$actual" != "$expected" ]; then
        say "SHA256 mismatch for $(basename "$file")"
        say "  expected: $expected"
        say "  actual:   $actual"
        err "refusing to install possibly-corrupted binary."
    fi
}

# ─── detect platform ────────────────────────────────────────────────────

need_cmd curl  "install via: apt install curl / brew install curl"
need_cmd tar   "install via: apt install tar  / brew install gnu-tar"
need_cmd uname "uname is part of coreutils; your system is broken."

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux)  os_part=unknown-linux-gnu ;;
    Darwin) os_part=apple-darwin ;;
    *)      err "unsupported OS: $os. Supported: Linux, Darwin (macOS)." ;;
esac

case "$arch" in
    x86_64|amd64)  arch_part=x86_64 ;;
    aarch64|arm64) arch_part=aarch64 ;;
    *) err "unsupported arch: $arch. Supported: x86_64, aarch64 (arm64).
For Rust users on other architectures, try:
  cargo install --git https://github.com/${REPO}" ;;
esac

target="${arch_part}-${os_part}"

# ─── resolve version ────────────────────────────────────────────────────

if [ -n "${CODEFORGE_VERSION:-}" ]; then
    version="${CODEFORGE_VERSION#v}"
    say "using CODEFORGE_VERSION=v$version"
else
    say "resolving latest release ..."
    api_json=$(curl -fsSL --retry 2 \
        "https://api.github.com/repos/${REPO}/releases/latest" \
        || err "GitHub API request failed (rate-limited?).
  Workaround: set CODEFORGE_VERSION=vX.Y.Z and re-run.")
    version=$(printf '%s' "$api_json" \
        | grep -m1 '"tag_name"' \
        | sed 's/.*"v\([^"]*\)".*/\1/')
    [ -n "$version" ] || err "could not parse tag_name from GitHub API response."
fi

# ─── pick install dir ───────────────────────────────────────────────────

if [ -n "${CODEFORGE_INSTALL_DIR:-}" ]; then
    install_dir="$CODEFORGE_INSTALL_DIR"
elif [ -n "${CARGO_HOME:-}" ] && [ -d "$CARGO_HOME/bin" ]; then
    install_dir="$CARGO_HOME/bin"
elif [ -d "$HOME/.cargo/bin" ]; then
    install_dir="$HOME/.cargo/bin"
else
    install_dir="$HOME/.local/bin"
fi
mkdir -p "$install_dir"

bin_path="$install_dir/codeforge"

# ─── existing-install check ─────────────────────────────────────────────

if [ -x "$bin_path" ] && [ -z "${CODEFORGE_FORCE:-}" ]; then
    existing=$("$bin_path" --version 2>/dev/null | awk '{print $2}' || echo "?")
    if [ "$existing" = "$version" ]; then
        say "✓ codeforge v$version already installed at $bin_path"
        say "  (set CODEFORGE_FORCE=1 to reinstall)"
        exit 0
    fi
    say "upgrading codeforge $existing → $version at $bin_path"
fi

# ─── download + verify + install ────────────────────────────────────────

archive="codeforge-${version}-${target}.tar.gz"
sha_file="${archive}.sha256"
url="https://github.com/${REPO}/releases/download/v${version}/${archive}"
sha_url="${url}.sha256"

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t cf-install)
# shellcheck disable=SC2064
trap "rm -rf $tmp" EXIT

say "downloading $archive ..."
curl -fSL --retry 2 -o "$tmp/$archive"  "$url" \
    || err "download failed: $url
  Check that v$version exists at https://github.com/${REPO}/releases"
curl -fSL --retry 2 -o "$tmp/$sha_file" "$sha_url" \
    || err "sha256 download failed: $sha_url"

expected_line=$(cat "$tmp/$sha_file")
sha256_check "$expected_line" "$tmp/$archive"

say "extracting ..."
tar xzf "$tmp/$archive" -C "$tmp"

src="$tmp/codeforge-${version}-${target}/codeforge"
[ -f "$src" ] || err "tarball layout unexpected: missing $src"

# Atomic install: write to .tmp then rename
install_tmp="${bin_path}.tmp.$$"
cp "$src" "$install_tmp"
chmod 0755 "$install_tmp"
mv "$install_tmp" "$bin_path"

# ─── verify ─────────────────────────────────────────────────────────────

installed=$("$bin_path" --version 2>/dev/null | awk '{print $2}' || echo "?")
[ "$installed" = "$version" ] || err "post-install version check failed (got '$installed', expected '$version')."

say "✓ codeforge v$version installed at $bin_path"

# ─── PATH hint ──────────────────────────────────────────────────────────

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        say ""
        say "NOTE: $install_dir is not on your \$PATH."
        case "${SHELL:-}" in
            */zsh)  rc="~/.zshrc" ;;
            */bash) rc="~/.bashrc" ;;
            */fish) rc="~/.config/fish/config.fish" ;;
            *)      rc="your shell rc file" ;;
        esac
        say "      Add to $rc:"
        say "          export PATH=\"$install_dir:\$PATH\""
        ;;
esac

# ─── next steps ─────────────────────────────────────────────────────────

cat >&2 <<EOF

Next steps:
  codeforge install                    # wire ~/.claude/settings.json
  cd ~/projects/<your-repo>
  codeforge init                       # initialize the project store
  codeforge adopt                      # pick a starter pet

See: https://github.com/${REPO}/blob/main/doc/getting-started.md
EOF
