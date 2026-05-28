#!/usr/bin/env bash
# Build tu for every supported target. Native macOS builds use cargo,
# Linux cross builds use cargo-zigbuild. Output binaries live under
# `target/<triple>/release/tu` and are *not* copied here -- packaging
# happens in scripts/package-all.sh.
set -euo pipefail

export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"

NATIVE_TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
)

ZIG_TARGETS=(
    x86_64-unknown-linux-gnu
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-gnu
    aarch64-unknown-linux-musl
    armv7-unknown-linux-gnueabihf
)

for t in "${NATIVE_TARGETS[@]}"; do
    echo ">>> cargo build --release --target $t"
    cargo build --release --target "$t"
done

for t in "${ZIG_TARGETS[@]}"; do
    echo ">>> cargo zigbuild --release --target $t"
    cargo zigbuild --release --target "$t"
done

echo "=== Built binaries ==="
for t in "${NATIVE_TARGETS[@]}" "${ZIG_TARGETS[@]}"; do
    b="target/$t/release/tu"
    if [[ -f "$b" ]]; then
        printf '  %-32s %s\n' "$t" "$(ls -lh "$b" | awk '{print $5}')"
    else
        printf '  %-32s MISSING\n' "$t"
    fi
done
