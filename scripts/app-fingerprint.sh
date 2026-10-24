#!/usr/bin/env bash
# Fingerprint of everything a store app is built from, at HEAD:
#   ios      the iPhone app: ios/, scripts/build-ios.sh
#   mac      the Mac App Store app: apple/Sources/MiSTerKVMS, apple/AppStore,
#            scripts/build-mac-appstore.sh
#   android  the Android app: android/, scripts/build-android.sh
# plus, for each: the workspace crates kvms-core compiles for that platform
# (found by cargo, so a new dependency between our crates is picked up),
# uniffi-bindgen, the exact versions of the crates.io packages it links, and
# the workspace manifest (version, release profile).
# CI uploads an app only when this changes: each upload is tagged
# testflight/<build> (iPhone), testflight-mac/<build> or play/<versionCode>
# with its fingerprint as the tag message.
# Usage: scripts/app-fingerprint.sh ios|mac|android [--list]   (--list: the inputs, unhashed)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)

case "${1:-}" in
  ios) TARGET=aarch64-apple-ios; OWN=(ios scripts/build-ios.sh) ;;
  mac) TARGET=aarch64-apple-darwin; OWN=(apple/Sources/MiSTerKVMS apple/AppStore scripts/build-mac-appstore.sh) ;;
  android) TARGET=aarch64-linux-android; OWN=(android scripts/build-android.sh) ;;
  *) echo "usage: $0 ios|mac|android [--list]" >&2; exit 2 ;;
esac

inputs() {
    local packages
    packages=$(cargo tree -p kvms-core --target "$TARGET" -e normal,build \
        --prefix none --locked --format '{p}' | sed 's/ (\*)$//' | grep . | sort -u)
    # crates.io packages: name and version (the lockfile pins their contents).
    grep -v " (/" <<<"$packages"
    # Our crates: the git tree of each one's folder.
    local dirs
    dirs=$(sed -n "s|.* ($ROOT/\(.*\))\$|\1|p" <<<"$packages")
    local path tree
    for path in $dirs uniffi-bindgen "${OWN[@]}" Cargo.toml; do
        tree=$(git rev-parse --verify -q "HEAD:$path") || { echo "not in git: $path" >&2; exit 1; }
        echo "$path $tree"
    done
}

if [[ "${2:-}" == --list ]]; then
    inputs
else
    inputs | shasum -a 256 | cut -d' ' -f1
fi
