#!/bin/sh
# Install tu to ~/.local/bin (or $TU_INSTALL_DIR) — no sudo required.
#
#   curl -fsSL https://raw.githubusercontent.com/hungryZoo/tu/main/install.sh | sh
#
# Environment:
#   TU_VERSION=1.0.0     pin a release (default: latest on GitHub)
#   TU_INSTALL_DIR=...   install directory (default: ~/.local/bin)
set -eu

REPO="hungryZoo/tu"
DEFAULT_BIN_DIR="${HOME}/.local/bin"
BIN_DIR="${TU_INSTALL_DIR:-${TU_BIN_DIR:-$DEFAULT_BIN_DIR}}"
VERSION="${TU_VERSION:-}"
_TMP_DIR=""

cleanup() {
    if [ -n "$_TMP_DIR" ] && [ -d "$_TMP_DIR" ]; then
        rm -rf "$_TMP_DIR"
    fi
}
trap cleanup EXIT

err() {
    echo "error: $*" >&2
    exit 1
}

info() {
    echo "==> $*"
}

usage() {
    cat <<'EOF'
tu installer — no sudo required

Usage:
  install.sh [OPTIONS]

Options:
  --bin-dir DIR   Install directory (default: ~/.local/bin)
  --version VER   Release version without leading v (default: latest)
  -h, --help      Show this help

Environment:
  TU_INSTALL_DIR, TU_BIN_DIR, TU_VERSION
EOF
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "need '$1' (command not found)"
}

detect_triple() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin)
            case "$arch" in
                arm64|aarch64) echo "aarch64-apple-darwin" ;;
                x86_64)
                    if sysctl -n hw.optional.x86_64 2>/dev/null | grep -q '^1$'; then
                        echo "x86_64-apple-darwin"
                    else
                        err "unsupported macOS architecture: $arch"
                    fi
                    ;;
                *) err "unsupported macOS architecture: $arch" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-musl" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
                armv7l|armv6l) echo "armv7-unknown-linux-gnueabihf" ;;
                *) err "unsupported Linux architecture: $arch" ;;
            esac
            ;;
        *)
            err "unsupported OS: $os (tu supports macOS and Linux)"
            ;;
    esac
}

get_latest_version() {
    json="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")" \
        || err "failed to fetch latest release from GitHub"
    echo "$json" \
        | grep '"tag_name"' \
        | head -1 \
        | cut -d '"' -f 4 \
        | sed 's/^v//'
}

parse_args() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --bin-dir)
                BIN_DIR="$2"
                shift 2
                ;;
            --bin-dir=*)
                BIN_DIR="${1#*=}"
                shift
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --version=*)
                VERSION="${1#*=}"
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                err "unknown option: $1 (try --help)"
                ;;
        esac
    done
}

main() {
    parse_args "$@"

    need_cmd curl
    need_cmd tar

    if ! command -v tmux >/dev/null 2>&1; then
        echo "warning: tmux not found on PATH — install it before running tu" >&2
    fi

    triple="$(detect_triple)"
    info "platform: $triple"

    if [ -n "$VERSION" ]; then
        version="$VERSION"
    else
        version="$(get_latest_version)"
    fi
    [ -n "$version" ] || err "could not determine release version"
    info "version: $version"

    asset="tu-${version}-${triple}.tar.gz"
    url="https://github.com/${REPO}/releases/download/v${version}/${asset}"

    _TMP_DIR="$(mktemp -d)"

    info "downloading $url"
    if ! curl -fsSL "$url" -o "$_TMP_DIR/$asset"; then
        err "download failed — no release asset for $triple (v$version)"
    fi

    tar -xzf "$_TMP_DIR/$asset" -C "$_TMP_DIR" tu

    mkdir -p "$BIN_DIR"
    install -m 0755 "$_TMP_DIR/tu" "$BIN_DIR/tu"
    info "installed tu to $BIN_DIR/tu"

    if ! echo ":$PATH:" | grep -q ":${BIN_DIR}:"; then
        echo
        echo "Note: $BIN_DIR is not on your PATH."
        echo "Add this to your shell profile:"
        echo "  export PATH=\"$BIN_DIR:\$PATH\""
    fi

    echo
    echo "tu is installed! Run: tu"
}

main "$@"
