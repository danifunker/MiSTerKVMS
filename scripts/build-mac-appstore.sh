#!/usr/bin/env bash
# Build the Mac App Store app: the same Swift sources as the DMG app
# (apple/Sources/MiSTerKVMS), built by apple/AppStore/MiSTerKVMS.xcodeproj
# with APP_STORE set: sandboxed, client only (no capture server or CLI
# inside), no Accessibility event tap. See apple/AppStore/README.md.
# Usage: scripts/build-mac-appstore.sh [--upload]
#   (none)    build it, signed to run locally: build/mac-appstore/.../MiSTerKVMS.app
#   --upload  archive, sign and upload to App Store Connect (TestFlight for
#             Mac); needs DEVELOPMENT_TEAM and the ASC_* key, as for
#             scripts/build-ios.sh --testflight
#
# Env: DEVELOPMENT_TEAM, VERSION, BUILD_NUMBER, ASC_KEY_PATH, ASC_KEY_ID,
# ASC_ISSUER_ID (see scripts/build-ios.sh).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}
BUILD_NUMBER=${BUILD_NUMBER:-$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)}
OUT="$ROOT/build/mac-appstore"
cd "$ROOT"

UPLOAD=0
for arg in "$@"; do
  case "$arg" in
    --upload) UPLOAD=1 ;;
    *) echo "unknown option $arg" >&2; exit 2 ;;
  esac
done

# rustup's toolchain (Homebrew's Rust builds for the host macOS only).
export PATH="$HOME/.cargo/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=14.0
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
for t in "${TARGETS[@]}"; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done

echo "==> Rust core ($VERSION build $BUILD_NUMBER)"
for t in "${TARGETS[@]}"; do cargo build --release -p kvms-core --target "$t"; done
mkdir -p target/universal "$OUT/headers"
lipo -create -output target/universal/libkvms_core.a \
  target/aarch64-apple-darwin/release/libkvms_core.a target/x86_64-apple-darwin/release/libkvms_core.a

echo "==> Swift bindings"
cargo run -q --release -p uniffi-bindgen --target aarch64-apple-darwin -- generate \
  --library target/aarch64-apple-darwin/release/libkvms_core.dylib --language swift --out-dir target/uniffi-swift-mac
# The Swift file is shared with the DMG build (scripts/build-mac.sh): keep its
# header in step too.
mkdir -p apple/Sources/KVMSCore/Generated apple/Sources/kvms_coreFFI/include
cp target/uniffi-swift-mac/kvms_core.swift apple/Sources/KVMSCore/Generated/
cp target/uniffi-swift-mac/kvms_coreFFI.h apple/Sources/kvms_coreFFI/include/
cp target/uniffi-swift-mac/kvms_coreFFI.h "$OUT/headers/"
cp target/uniffi-swift-mac/kvms_coreFFI.modulemap "$OUT/headers/module.modulemap"

echo "==> KVMSCore.xcframework"
rm -rf "$OUT/KVMSCore.xcframework"
xcodebuild -create-xcframework -library target/universal/libkvms_core.a -headers "$OUT/headers" \
  -output "$OUT/KVMSCore.xcframework" >/dev/null

# xcodebuild's full output goes to a log; errors and warnings are shown.
xcb() {
  local log="$OUT/xcodebuild-$1.log"; shift
  if ! xcodebuild "$@" > "$log" 2>&1; then
    grep -E "error:|warning: |ERROR|BUILD FAILED|EXPORT FAILED" "$log" | tail -40
    echo "xcodebuild failed; full log: $log" >&2
    exit 1
  fi
  grep -E "warning: " "$log" | grep -v appintentsmetadataprocessor | sort -u | head -20 || true
}
PROJECT=(-project apple/AppStore/MiSTerKVMS.xcodeproj -scheme MiSTerKVMS -configuration Release)
VERSIONS=(MARKETING_VERSION="$VERSION" CURRENT_PROJECT_VERSION="$BUILD_NUMBER")

if [[ "$UPLOAD" == "1" ]]; then
  : "${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM}" "${ASC_KEY_PATH:?set ASC_KEY_PATH}"
  : "${ASC_KEY_ID:?set ASC_KEY_ID}" "${ASC_ISSUER_ID:?set ASC_ISSUER_ID}"
  AUTH=(-allowProvisioningUpdates -authenticationKeyPath "$ASC_KEY_PATH"
        -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
  echo "==> Archive (team $DEVELOPMENT_TEAM)"
  rm -rf "$OUT/MiSTerKVMS.xcarchive" "$OUT/export"
  xcb archive "${PROJECT[@]}" -destination "generic/platform=macOS" -derivedDataPath "$OUT/DerivedData" \
    -archivePath "$OUT/MiSTerKVMS.xcarchive" DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" CODE_SIGN_STYLE=Automatic \
    "${VERSIONS[@]}" "${AUTH[@]}" archive
  cat > "$OUT/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key><string>app-store-connect</string>
	<key>destination</key><string>export</string>
	<key>teamID</key><string>$DEVELOPMENT_TEAM</string>
	<key>signingStyle</key><string>automatic</string>
	<key>uploadSymbols</key><true/>
	<key>manageAppVersionAndBuildNumber</key><false/>
</dict>
</plist>
PLIST
  echo "==> Export the App Store .pkg"
  xcb export -exportArchive -archivePath "$OUT/MiSTerKVMS.xcarchive" \
    -exportOptionsPlist "$OUT/ExportOptions.plist" -exportPath "$OUT/export" "${AUTH[@]}"
  PKG=$(find "$OUT/export" -maxdepth 1 -name '*.pkg' | head -1)
  [[ -n "$PKG" ]] || { echo "export made no .pkg" >&2; exit 1; }
  echo "==> Upload to App Store Connect ($VERSION build $BUILD_NUMBER)"
  xcrun altool --upload-package "$PKG" --type macos --api-key "$ASC_KEY_ID" --api-issuer "$ASC_ISSUER_ID" \
    --p8-file-path "$ASC_KEY_PATH" --output-format normal
  echo "Uploaded $VERSION ($BUILD_NUMBER): it shows up in TestFlight (Mac) once Apple has processed it."
  exit 0
fi

echo "==> Mac App Store app (signed to run locally)"
xcb build "${PROJECT[@]}" -destination "generic/platform=macOS" -derivedDataPath "$OUT/DerivedData" \
  "${VERSIONS[@]}" CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= build
APP="$OUT/DerivedData/Build/Products/Release/MiSTerKVMS.app"
[[ -d "$APP" ]] || { echo "build failed" >&2; exit 1; }
lipo -archs "$APP/Contents/MacOS/MiSTerKVMS" | sed 's/^/architectures: /'
echo "Built $APP"
