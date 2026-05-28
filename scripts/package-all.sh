#!/usr/bin/env bash
# Package every per-target binary into dist/:
#   tu-1.0.0-<triple>.tar.gz  (contains: tu, README.md, LICENSE)
#   tu_1.0.0_<arch>.deb       (x86_64 / aarch64 / armv7)
#   tu-1.0.0-1.<arch>.rpm     (x86_64 / aarch64)
#   SHA256SUMS                (covers every file above)
set -euo pipefail

export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"

VERSION="1.0.0"
DIST_DIR="dist"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

mkdir -p "$DIST_DIR"

TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    x86_64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-gnu
    aarch64-unknown-linux-musl
    armv7-unknown-linux-gnueabihf
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
    tar -czf "$DIST_DIR/tu-$VERSION-$t.tar.gz" -C "$stage" tu README.md LICENSE
    rm -rf "$stage"
    echo "    $DIST_DIR/tu-$VERSION-$t.tar.gz"
done

echo ">>> .deb (cargo-deb)"
DEB_TARGETS=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    armv7-unknown-linux-gnueabihf
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

echo ">>> SHA256SUMS"
cd "$DIST_DIR"
shasum -a 256 *.tar.gz *.deb *.rpm > SHA256SUMS
cat SHA256SUMS

echo
echo "=== dist/ ==="
ls -lh
