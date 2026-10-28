#!/usr/bin/env bash
# Build the Android app: Rust core (phones + the emulator) → jniLibs →
# Kotlin bindings → Gradle.
# Usage: scripts/build-android.sh [--core-only] [--install] [--release]
#   --core-only  stop after the core and bindings (then build in Android
#                Studio: open android/)
#   --install    install the debug build on the running emulator or phone
#   --release    the Play bundle (.aab), signed when the upload key is set
#
# Env:
#   VERSION        versionName (default: from Cargo.toml).
#   BUILD_NUMBER   versionCode (default: git commit count); must grow with
#                  every Play upload.
#   ANDROID_KEYSTORE_PATH, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS
#                  the upload key (docs/ANDROID.md); one password for the
#                  store and the key.
#   JAVA_HOME      JDK 17+ (default: Android Studio's own, if installed).
#
# Needs: the Android SDK and NDK (ANDROID_HOME; cargo-ndk picks the newest
# NDK unless ANDROID_NDK_HOME says which), `cargo install cargo-ndk`.
# Works in Git Bash on Windows as well as on Linux and macOS.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}
BUILD_NUMBER=${BUILD_NUMBER:-$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)}
cd "$ROOT"

CORE_ONLY=0; INSTALL=0; RELEASE=0
for arg in "$@"; do
  case "$arg" in
    --core-only) CORE_ONLY=1 ;;
    --install) INSTALL=1 ;;
    --release) RELEASE=1 ;;
    *) echo "unknown option $arg" >&2; exit 2 ;;
  esac
done

# The oldest Android the app runs on; same as minSdk in android/app.
# AAudio (cpal's Android backend) needs 26 or later.
MIN_SDK=29
JNILIBS=android/app/src/main/jniLibs
# The bindings' package, com.danifunker.misterkvms.core (core/uniffi.toml).
SOURCES=android/app/src/main/kotlin
BINDINGS=$SOURCES/com/danifunker/misterkvms/core

for t in aarch64-linux-android x86_64-linux-android; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done
command -v cargo-ndk >/dev/null || { echo "cargo-ndk missing: cargo install cargo-ndk" >&2; exit 1; }

echo "==> Rust core ($VERSION build $BUILD_NUMBER; arm64-v8a, x86_64; API $MIN_SDK)"
rm -rf "$JNILIBS"
cargo ndk -P "$MIN_SDK" -t arm64-v8a -t x86_64 -o "$JNILIBS" build --release -p kvms-core

echo "==> Kotlin bindings"
# Read from the built library itself: the exports are the same on every ABI.
rm -rf "$BINDINGS"
cargo run -q --release -p uniffi-bindgen -- generate --no-format \
  --library target/aarch64-linux-android/release/libkvms_core.so --language kotlin --out-dir "$SOURCES"

[[ "$CORE_ONLY" == "1" ]] && { echo "Done: $JNILIBS, $BINDINGS"; exit 0; }

if [[ -z "${JAVA_HOME:-}" ]]; then
  for jbr in "/c/Program Files/Android/Android Studio/jbr" \
             "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
             "/opt/android-studio/jbr"; do
    [[ -x "$jbr/bin/java" || -x "$jbr/bin/java.exe" ]] && { export JAVA_HOME="$jbr"; break; }
  done
fi

cd android
GRADLE=(./gradlew --console=plain "-PversionName=$VERSION" "-PversionCode=$BUILD_NUMBER")
if [[ "$RELEASE" == "1" ]]; then
  echo "==> Play bundle"
  # The native symbols zip too (CI uploads it with the bundle), so Play's
  # crash reports name the core's functions.
  "${GRADLE[@]}" bundleRelease mergeReleaseNativeDebugMetadata
  AAB=app/build/outputs/bundle/release/app-release.aab
  if [[ -n "${ANDROID_KEYSTORE_PATH:-}" ]]; then
    echo "Built android/$AAB (signed with the upload key)"
  else
    echo "Built android/$AAB (unsigned: set the ANDROID_KEYSTORE_* variables to sign it)"
  fi
elif [[ "$INSTALL" == "1" ]]; then
  echo "==> Debug build, installed"
  "${GRADLE[@]}" installDebug
else
  echo "==> Debug build"
  "${GRADLE[@]}" assembleDebug
  echo "Built android/app/build/outputs/apk/debug/app-debug.apk"
fi
