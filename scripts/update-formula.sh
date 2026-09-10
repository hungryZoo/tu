#!/usr/bin/env bash
# Point a Homebrew formula at a release: rewrite each tarball url +
# sha256 from a SHA256SUMS file. The formula lives in the tap repo
# (hungryZoo/homebrew-tap), not here, so pass its path:
#
#   scripts/update-formula.sh 1.1.1 dist/SHA256SUMS ../homebrew-tap/Formula/tu.rb
set -euo pipefail

VERSION="${1:?usage: update-formula.sh VERSION SHA256SUMS FORMULA}"
SUMS="${2:?usage: update-formula.sh VERSION SHA256SUMS FORMULA}"
FORMULA="${3:?usage: update-formula.sh VERSION SHA256SUMS FORMULA}"
[ -f "$FORMULA" ] || { echo "no formula at $FORMULA" >&2; exit 1; }

sha_for() {
    awk -v f="tu-$VERSION-$1.tar.gz" '$2 == f { print $1 }' "$SUMS"
}

for triple in aarch64-apple-darwin x86_64-apple-darwin \
              aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
    sha="$(sha_for "$triple")"
    [ -n "$sha" ] || { echo "no checksum for $triple in $SUMS" >&2; exit 1; }
    url="https://github.com/hungryZoo/tu/releases/download/v$VERSION/tu-$VERSION-$triple.tar.gz"
    # Replace the url line for this triple, then the sha256 line that follows it.
    python3 - "$FORMULA" "$triple" "$url" "$sha" <<'PY'
import re, sys
path, triple, url, sha = sys.argv[1:]
text = open(path).read()
pat = re.compile(
    r'(url ")[^"]*-' + re.escape(triple) + r'\.tar\.gz(")\n(\s*sha256 ")[0-9a-f]*(")'
)
new, n = pat.subn(lambda m: f'{m.group(1)}{url}{m.group(2)}\n{m.group(3)}{sha}{m.group(4)}', text)
if n != 1:
    sys.exit(f"expected one url/sha256 pair for {triple}, found {n}")
open(path, "w").write(new)
PY
done

# Homebrew derives `version` from the url, so no explicit version stanza to bump.
echo "$FORMULA now points at v$VERSION"
