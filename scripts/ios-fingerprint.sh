#!/usr/bin/env bash
# Fingerprint of everything the iPhone app is built from, at HEAD:
#   - ios/ and the scripts that build it
#   - the workspace crates kvms-core compiles for iOS (found by cargo, so a new
#     dependency between our crates is picked up), and uniffi-bindgen
#   - the exact versions of the crates.io packages it links for iOS
#   - the workspace manifest (version, release profile)
# CI uploads to TestFlight only when this changes: each upload is tagged
# testflight/<build> with its fingerprint as the tag message.
# Usage: scripts/ios-fingerprint.sh [--list]   (--list: the inputs, unhashed)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)

inputs() {
    local packages
    packages=$(cargo tree -p kvms-core --target aarch64-apple-ios -e normal,build \
        --prefix none --locked --format '{p}' | sed 's/ (\*)$//' | grep . | sort -u)
    # crates.io packages: name and version (the lockfile pins their contents).
    grep -v " (/" <<<"$packages"
    # Our crates: the git tree of each one's folder.
    local dirs
    dirs=$(sed -n "s|.* ($ROOT/\(.*\))\$|\1|p" <<<"$packages")
    local path tree
    for path in $dirs uniffi-bindgen ios scripts/build-ios.sh Cargo.toml; do
        tree=$(git rev-parse --verify -q "HEAD:$path") || { echo "not in git: $path" >&2; exit 1; }
        echo "$path $tree"
    done
}

if [[ "${1:-}" == --list ]]; then
    inputs
else
    inputs | shasum -a 256 | cut -d' ' -f1
fi
