#!/usr/bin/env bash
# Run on a developer machine after the `release` workflow has published
# vX.Y.Z on GitHub. Uses the local `gh` login and `cargo login`, so no
# repository secrets are needed:
#
#   1. downloads SHA256SUMS from the release
#   2. points Formula/tu.rb in hungryZoo/homebrew-tap at the new assets
#      and pushes it
#   3. publishes the crate to crates.io (skips if the version is already
#      there)
#
#   scripts/post-release.sh            # version read from Cargo.toml
#   scripts/post-release.sh 1.2.0
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-$(grep -m1 '^version = ' "$ROOT/Cargo.toml" | cut -d'"' -f2)}"
TAG="v$VERSION"
TAP_REPO="hungryZoo/homebrew-tap"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> $TAG: fetching SHA256SUMS"
gh release download "$TAG" --repo hungryZoo/tu --pattern SHA256SUMS -O "$WORK/SHA256SUMS"

echo "==> updating Formula/tu.rb in $TAP_REPO"
gh repo clone "$TAP_REPO" "$WORK/tap" -- -q --depth 1
"$ROOT/scripts/update-formula.sh" "$VERSION" "$WORK/SHA256SUMS" "$WORK/tap/Formula/tu.rb"
if git -C "$WORK/tap" diff --quiet -- Formula/tu.rb; then
    echo "    formula already at $TAG"
else
    git -C "$WORK/tap" commit -qam "tu $VERSION"
    git -C "$WORK/tap" push -q origin HEAD
    echo "    pushed"
fi

echo "==> crates.io"
if curl -fsS -A "tu-post-release" "https://crates.io/api/v1/crates/tmux-tu/$VERSION" >/dev/null 2>&1; then
    echo "    tmux-tu $VERSION already published"
else
    (cd "$ROOT" && git checkout -q "$TAG" -- Cargo.toml Cargo.lock src && cargo publish --locked)
    (cd "$ROOT" && git checkout -q -- Cargo.toml Cargo.lock src)
fi

echo "==> done: $TAG on GitHub, Homebrew tap, crates.io"
