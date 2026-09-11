#!/usr/bin/env bash
# Package every per-target binary into dist/:
#   tu-<ver>-<triple>.tar.gz  (contains: tu, README.md, LICENSE)
#   tu_<ver>_<arch>.deb       (amd64 / arm64 / armhf)
#   tu-<ver>-1.<arch>.rpm     (x86_64 / aarch64)
#   tu_<arch>.deb, tu-<arch>.rpm
#                             version-less copies so README install
#                             commands can point at releases/latest
#                             without a bump on every release
#   SHA256SUMS                (covers every file above)
set -euo pipefail

export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"

# Version comes from Cargo.toml so a release bump is a one-file change;
# override with VERSION=... if you must.
DIST_DIR="dist"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
VERSION="${VERSION:-$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)}"
[ -n "$VERSION" ] || { echo "could not read version from Cargo.toml" >&2; exit 1; }

mkdir -p "$DIST_DIR"

TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    x86_64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-gnu
    aarch64-unknown-linux-musl
    armv7-unknown-linux-gnueabihf
    arm-unknown-linux-gnueabihf
)

echo ">>> tarballs"
for t in "${TARGETS[@]}"; do
    bin="target/$t/release/tu"
    if [[ ! -f "$bin" ]]; then
        echo "    SKIP $t (binary missing)"
        continue
    fi
    stage="$(mktemp -d)"
    cp "$bin" "$stage/tu"
    cp README.md LICENSE "$stage/"
    # COPYFILE_DISABLE keeps macOS bsdtar from adding ._* / xattr
    # headers that GNU tar on Linux then warns about on extract.
    COPYFILE_DISABLE=1 tar -czf "$DIST_DIR/tu-$VERSION-$t.tar.gz" -C "$stage" tu README.md LICENSE
    rm -rf "$stage"
    echo "    $DIST_DIR/tu-$VERSION-$t.tar.gz"
done

echo ">>> .deb (cargo-deb)"
# The armhf .deb is built from the ARMv6 binary: it runs on every
# 32-bit Raspberry Pi OS, from Pi 1 / Zero (ARM1176) up to Pi 5.
DEB_TARGETS=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    arm-unknown-linux-gnueabihf
)
for t in "${DEB_TARGETS[@]}"; do
    if [[ ! -f "target/$t/release/tu" ]]; then
        echo "    SKIP $t (binary missing)"
        continue
    fi
    # --no-build: reuse the zigbuilt binary; --no-strip: zig already stripped via release profile.
    cargo deb --target "$t" --no-build --no-strip --output "$DIST_DIR" >/dev/null
    echo "    $DIST_DIR/$(ls -t "$DIST_DIR" | grep "\.deb$" | head -1)"
done

echo ">>> .rpm (cargo-generate-rpm)"
RPM_TARGETS=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
)
for t in "${RPM_TARGETS[@]}"; do
    if [[ ! -f "target/$t/release/tu" ]]; then
        echo "    SKIP $t (binary missing)"
        continue
    fi
    cargo generate-rpm --target "$t" --output "$DIST_DIR" >/dev/null
    echo "    $DIST_DIR/$(ls -t "$DIST_DIR" | grep "\.rpm$" | head -1)"
done

echo ">>> version-less package aliases"
# GitHub serves /releases/latest/download/<name> for any asset name, so
# a stable name lets the README stay correct across releases. The
# package metadata inside still carries the real version.
for f in "$DIST_DIR"/tu_"$VERSION"_*.deb; do
    [[ -f "$f" ]] || continue
    alias="$DIST_DIR/tu_${f##*_}"
    cp "$f" "$alias"
    echo "    $alias"
done
for f in "$DIST_DIR"/tu-"$VERSION"-1.*.rpm; do
    [[ -f "$f" ]] || continue
    alias="$DIST_DIR/tu-${f##*-1.}"
    cp "$f" "$alias"
    echo "    $alias"
done

echo ">>> install.sh"
cp install.sh "$DIST_DIR/"

echo ">>> SHA256SUMS"
cd "$DIST_DIR"
shasum -a 256 *.tar.gz *.deb *.rpm install.sh > SHA256SUMS
cat SHA256SUMS

echo
echo "=== dist/ ==="
ls -lh
