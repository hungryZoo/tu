#!/bin/sh
# Install tu without sudo, and put it on PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/hungryZoo/tu/main/install.sh | sh
#
# Where it goes:
#   Termux (Android)  $PREFIX/bin   — already on PATH, nothing else to do
#   everywhere else   ~/.local/bin  — added to PATH in your shell's startup
#                                     file if it isn't there yet
#
# Environment:
#   TU_VERSION=1.1.6       pin a release (default: latest on GitHub)
#   TU_INSTALL_DIR=...     install directory
#   TU_NO_MODIFY_PATH=1    never touch shell startup files
set -eu

REPO="hungryZoo/tu"
VERSION="${TU_VERSION:-}"
MODIFY_PATH=1
case "${TU_NO_MODIFY_PATH:-}" in ""|0) ;; *) MODIFY_PATH=0 ;; esac
_TMP_DIR=""
PATH_MARK="# Added by tu installer (https://github.com/hungryZoo/tu)"

is_termux() {
    [ -n "${TERMUX_VERSION:-}" ] && return 0
    case "${PREFIX:-}" in */com.termux/*) return 0 ;; esac
    return 1
}

if is_termux && [ -n "${PREFIX:-}" ]; then
    # $PREFIX/bin is on Termux's PATH out of the box and user-writable,
    # so no startup file needs editing (Termux has no ~/.bashrc by default).
    DEFAULT_BIN_DIR="$PREFIX/bin"
else
    DEFAULT_BIN_DIR="${HOME}/.local/bin"
fi
BIN_DIR="${TU_INSTALL_DIR:-${TU_BIN_DIR:-$DEFAULT_BIN_DIR}}"

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
  --bin-dir DIR       Install directory (default: ~/.local/bin;
                      $PREFIX/bin on Termux)
  --version VER       Release version without leading v (default: latest)
  --no-modify-path    Don't add the install directory to PATH in your
                      shell startup file
  -h, --help          Show this help

Environment:
  TU_INSTALL_DIR, TU_BIN_DIR, TU_VERSION, TU_NO_MODIFY_PATH=1
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
                x86_64) echo "x86_64-apple-darwin" ;;
                *) err "unsupported macOS architecture: $arch" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-musl" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
                armv7l|armv8l|armv6l)
                    # Android has no glibc, and the 32-bit ARM builds are
                    # glibc-linked. The 64-bit ones are static musl and run
                    # on Termux as-is.
                    if is_termux; then
                        err "no 32-bit ARM build for Termux yet (aarch64 and x86_64 only). Most phones from recent years are aarch64; 'uname -m' says $arch here, which usually means a 32-bit Termux install."
                    fi
                    case "$arch" in
                        armv6l) echo "arm-unknown-linux-gnueabihf" ;;  # Pi 1 / Zero / Zero W
                        *) echo "armv7-unknown-linux-gnueabihf" ;;
                    esac
                    ;;
                *) err "unsupported Linux architecture: $arch" ;;
            esac
            ;;
        *)
            err "unsupported OS: $os (tu supports macOS and Linux)"
            ;;
    esac
}

get_latest_version() {
    # Follow the /releases/latest redirect first: it needs no API
    # quota, so it keeps working behind shared IPs that have already
    # burned through the unauthenticated GitHub API rate limit.
    final="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/${REPO}/releases/latest" 2>/dev/null || true)"
    tag="${final##*/tag/}"
    if [ -n "$final" ] && [ "$tag" != "$final" ] && [ -n "$tag" ]; then
        echo "$tag" | sed 's/^v//'
        return
    fi
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
            --no-modify-path)
                MODIFY_PATH=0
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

# ------------------------------------------------------------------ PATH

tmux_hint() {
    if is_termux; then echo "pkg install tmux"
    elif [ "$(uname -s)" = Darwin ]; then echo "brew install tmux"
    elif command -v apt-get >/dev/null 2>&1; then echo "sudo apt install tmux"
    elif command -v dnf >/dev/null 2>&1; then echo "sudo dnf install tmux"
    elif command -v pacman >/dev/null 2>&1; then echo "sudo pacman -S tmux"
    elif command -v apk >/dev/null 2>&1; then echo "sudo apk add tmux"
    else echo "use your package manager"
    fi
}

on_path() {
    case ":${PATH:-}:" in *":$1:"*) return 0 ;; esac
    return 1
}

# "$HOME/.local/bin" for dirs under $HOME, so the startup file keeps
# working if the home directory moves; otherwise the literal path.
path_expr() {
    case "$1" in
        "$HOME"/*) echo "\$HOME/${1#"$HOME"/}" ;;
        *) echo "$1" ;;
    esac
}

# ~ for display only.
pretty() {
    case "$1" in
        "$HOME"/*) echo "~/${1#"$HOME"/}" ;;
        *) echo "$1" ;;
    esac
}

# Startup file(s) for the user's login shell, one per line. Under
# `curl | sh` the running shell is sh, so look at $SHELL, not at us.
startup_files() {
    shell_name="$(basename "${SHELL:-sh}")"
    case "$shell_name" in
        zsh)
            echo "${ZDOTDIR:-$HOME}/.zshrc"
            ;;
        bash)
            if [ "$(uname -s)" = Darwin ]; then
                # macOS Terminal starts login shells, which read
                # .bash_profile and never .bashrc.
                echo "$HOME/.bash_profile"
            else
                # Terminal windows are non-login shells: .bashrc. A login
                # shell (ssh, a tty) reads .bash_profile if it exists and
                # only sees .bashrc when that file sources it.
                echo "$HOME/.bashrc"
                if [ -f "$HOME/.bash_profile" ] && ! grep -q bashrc "$HOME/.bash_profile" 2>/dev/null; then
                    echo "$HOME/.bash_profile"
                fi
            fi
            ;;
        fish)
            echo "${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/tu.fish"
            ;;
        *)
            # sh, dash, ksh, mksh, ash, …: the POSIX login file.
            echo "$HOME/.profile"
            ;;
    esac
}

path_line_for() {
    expr="$(path_expr "$BIN_DIR")"
    case "$1" in
        *.fish)
            echo "contains -- \"$expr\" \$PATH; or set -gx PATH \"$expr\" \$PATH"
            ;;
        *)
            # Guarded, so sourcing the file twice doesn't grow PATH.
            echo "case \":\$PATH:\" in *\":$expr:\"*) ;; *) export PATH=\"$expr:\$PATH\" ;; esac"
            ;;
    esac
}

# Add BIN_DIR to one startup file. Creates the file (and its directory)
# if missing — Termux, minimal containers and fresh accounts often have
# no ~/.bashrc. Leaves it alone if it already mentions BIN_DIR.
add_to_file() {
    file="$1"
    expr="$(path_expr "$BIN_DIR")"
    if [ -f "$file" ] && { grep -qF "$BIN_DIR" "$file" || grep -qF "$expr" "$file"; }; then
        info "PATH: $(pretty "$file") already mentions $(pretty "$BIN_DIR")"
        return 0
    fi
    mkdir -p "$(dirname "$file")"
    if [ -s "$file" ] && [ -n "$(tail -c 1 "$file")" ]; then
        printf '\n' >> "$file"
    fi
    {
        if [ -s "$file" ]; then printf '\n'; fi
        echo "$PATH_MARK"
        path_line_for "$file"
    } >> "$file"
    info "PATH: added $(pretty "$BIN_DIR") in $(pretty "$file")"
    CHANGED=1
}

ensure_on_path() {
    if on_path "$BIN_DIR"; then
        return 0
    fi

    expr="$(path_expr "$BIN_DIR")"
    if [ "$MODIFY_PATH" = 0 ]; then
        echo
        echo "Note: $(pretty "$BIN_DIR") is not on your PATH (left alone: --no-modify-path)."
        echo "Add this to your shell's startup file:"
        echo "  export PATH=\"$expr:\$PATH\""
        return 0
    fi

    CHANGED=0
    startup_files | while IFS= read -r f; do
        add_to_file "$f"
    done

    echo
    echo "New terminals will find tu. To use it in this one right away, run:"
    case "$(basename "${SHELL:-sh}")" in
        fish) echo "  set -gx PATH \"$BIN_DIR\" \$PATH" ;;
        *) echo "  export PATH=\"$BIN_DIR:\$PATH\"" ;;
    esac
    echo "To undo, delete the lines below '$PATH_MARK'."
}

main() {
    parse_args "$@"

    need_cmd curl
    need_cmd tar

    if ! command -v tmux >/dev/null 2>&1; then
        echo "warning: tmux not found on PATH — install it before running tu ($(tmux_hint))" >&2
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

    ensure_on_path

    echo
    echo "tu is installed! Run: tu"
}

main "$@"
