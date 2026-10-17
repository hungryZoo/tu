#!/usr/bin/env bash
# Single source of truth for names, read from Cargo.toml. Source this
# from the other scripts; never hard-code `tu` or the version again.
#
#   CRATE    crates.io name       (tmux-tu — plain `tu` was taken)
#   BIN      the executable       ([[bin]] name; also tarball / .deb / .rpm prefix)
#   VERSION  [package] version
#
# Only the crate name is pinned by crates.io; everything the user sees
# (the command, package names, release asset names) uses BIN.
_meta_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_meta_toml="$_meta_root/Cargo.toml"

CRATE="$(sed -n '/^\[package\]/,/^\[/p' "$_meta_toml" | grep -m1 '^name = ' | cut -d'"' -f2)"
BIN="$(sed -n '/^\[\[bin\]\]/,/^\[/p' "$_meta_toml" | grep -m1 '^name = ' | cut -d'"' -f2)"
VERSION="${VERSION:-$(sed -n '/^\[package\]/,/^\[/p' "$_meta_toml" | grep -m1 '^version = ' | cut -d'"' -f2)}"

for _v in CRATE BIN VERSION; do
    [[ -n "${!_v}" ]] || { echo "meta.sh: could not read $_v from Cargo.toml" >&2; exit 1; }
done
export CRATE BIN VERSION
