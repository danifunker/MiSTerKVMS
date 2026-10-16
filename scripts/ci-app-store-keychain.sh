#!/usr/bin/env bash
# CI, before an App Store archive (scripts/build-ios.sh --testflight,
# scripts/build-mac-appstore.sh --upload): a temporary keychain holding an
# Apple Development identity, so Xcode signs the archive with it instead of
# asking Apple for a new development certificate on every run (each runner is
# a fresh Mac, and those certificates piled up until the account's limit).
# Apple's cloud-managed distribution certificate still signs the export.
#
# The identity is two secrets put together:
#   MACOS_CERT_P12 + MACOS_CERT_PASSWORD  the Developer ID .p12 (build-mac signs
#                                         the DMG with it): it brings the key
#   APPLE_DEV_CERT                        the Apple Development certificate
#                                         (base64 .cer) Apple issued for the
#                                         same key (the same CSR)
# Prints the keychain's path; delete it with `security delete-keychain`.
set -euo pipefail
: "${MACOS_CERT_P12:?}" "${MACOS_CERT_PASSWORD:?}" "${APPLE_DEV_CERT:?}" "${RUNNER_TEMP:?}"

keychain="$RUNNER_TEMP/app-store.keychain-db"
kc_pass=$(openssl rand -base64 24)
security create-keychain -p "$kc_pass" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$kc_pass" "$keychain"

echo "$MACOS_CERT_P12" | base64 --decode > "$RUNNER_TEMP/key.p12"
echo "$APPLE_DEV_CERT" | base64 --decode > "$RUNNER_TEMP/development.cer"
security import "$RUNNER_TEMP/key.p12" -k "$keychain" -P "$MACOS_CERT_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security >&2
security import "$RUNNER_TEMP/development.cer" -k "$keychain" >&2
rm -f "$RUNNER_TEMP/key.p12" "$RUNNER_TEMP/development.cer"
security set-key-partition-list -S apple-tool:,apple: -s -k "$kc_pass" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')

# The certificate pairs with the key only if Apple issued it for that key.
if ! security find-identity -v -p codesigning "$keychain" | grep -q "Apple Development"; then
  echo "APPLE_DEV_CERT has no matching key in MACOS_CERT_P12: issue the Apple Development certificate from the CSR the Developer ID certificate came from" >&2
  security delete-keychain "$keychain"
  exit 1
fi
security find-identity -v -p codesigning "$keychain" | grep "Apple Development" | sed 's/^ *[0-9]*) [0-9A-F]* /identity: /' >&2
echo "$keychain"
