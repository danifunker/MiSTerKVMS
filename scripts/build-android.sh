#!/usr/bin/env bash
# Build the Android app's core: Rust core (phones + the emulator) →
# jniLibs → Kotlin bindings. The Gradle app (android/) builds on these.
# Usage: scripts/build-android.sh
#
# Needs: the Android NDK (ANDROID_NDK_HOME, or ANDROID_HOME with an ndk/
# folder: cargo-ndk picks the newest), `cargo install cargo-ndk`.
# Works in Git Bash on Windows as well as on Linux and macOS.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

# The oldest Android the app runs on; same as minSdk in the Gradle app.
# AAudio (cpal's Android backend) needs 26 or later.
MIN_SDK=29
APP=android/app
JNILIBS=$APP/src/main/jniLibs
BINDINGS=$APP/src/generated/kotlin

for t in aarch64-linux-android x86_64-linux-android; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done
command -v cargo-ndk >/dev/null || { echo "cargo-ndk missing: cargo install cargo-ndk" >&2; exit 1; }

echo "==> Rust core (arm64-v8a, x86_64; API $MIN_SDK)"
rm -rf "$JNILIBS"
cargo ndk -P "$MIN_SDK" -t arm64-v8a -t x86_64 -o "$JNILIBS" build --release -p kvms-core

echo "==> Kotlin bindings"
# Read from the built library itself: the exports are the same on every ABI.
rm -rf "$BINDINGS"
cargo run -q --release -p uniffi-bindgen -- generate --no-format \
  --library target/aarch64-linux-android/release/libkvms_core.so --language kotlin --out-dir "$BINDINGS"

echo "Done: $JNILIBS, $BINDINGS"
