# MiSTerKVMS for the Mac App Store

The same Mac app as the DMG (`apple/Sources/MiSTerKVMS`, the same Rust
core), built by `MiSTerKVMS.xcodeproj` here with the `APP_STORE` compile
flag. What's different:

| | DMG (scripts/build-mac.sh) | App Store (this project) |
|---|---|---|
| Build | SwiftPM (`apple/Package.swift`) | Xcode project, synchronized folders on the same sources |
| Sandbox | no | yes (`MiSTerKVMS.entitlements`: network, camera and audio input for a local capture card, Movies for recordings, Pictures for screenshots) |
| Capture server, CLI | inside the app | not included: client only |
| System shortcuts (⌘Tab, ⌘Space) to the MiSTer | with Accessibility (event tap) | no: sandboxed apps can't have Accessibility; in-app capture only |
| Bundle id | `io.github.misterkvms.MiSTerKVMS` | `com.danifunker.misterkvms`, the iPhone app's: one listing for both (universal purchase) |
| Saved MiSTers | `~/Library/Application Support/MiSTerKVMS` | the sandbox container's copy of that folder (starts empty) |
| Signing | Developer ID, notarized | Apple's cloud signing with the App Store Connect API key, as for the iPhone app |

Code that differs is in `#if APP_STORE` blocks (the event tap, the
Accessibility prompt and banner). The generated bindings compile into the
app here rather than a `KVMSCore` module, so `import KVMSCore` is under
`#if canImport(KVMSCore)`, and `Model/CoreNamespace.swift` keeps
`KVMSCore.findStreams()` and friends working. New Swift files are picked up
by both builds without touching the project.

## Building

```bash
scripts/build-mac-appstore.sh
```

builds it signed to run locally (sandboxed) in
`build/mac-appstore/DerivedData/Build/Products/Release/MiSTerKVMS.app`.
`--upload` archives, signs and uploads it (needs `DEVELOPMENT_TEAM` and the
`ASC_*` key, as `scripts/build-ios.sh --testflight`).

CI (`build-mac-appstore` in `.github/workflows/release.yml`) builds it on
every pull request. On `main` it uploads when the app changed
(`scripts/app-fingerprint.sh mac`; each upload is tagged
`testflight-mac/<build>`) and the repository variable
`MAC_APP_STORE_UPLOAD` is `true`.

## One-time setup (in order)

1. **App Store Connect → Apps → MiSTerKVMS → (left column, by the iOS app)
   "+" / Add platform → macOS.** The iPhone app's record gets a macOS
   version; the bundle id is already registered.
2. **Repository variable** (not a secret): `MAC_APP_STORE_UPLOAD` = `true`
   (GitHub → Settings → Secrets and variables → Actions → Variables, or
   `gh variable set MAC_APP_STORE_UPLOAD --body true`). The next push to
   `main` that changes the Mac app uploads it; a manual run with "Upload the
   iPhone and Mac App Store apps" ticked uploads it at once.
3. **TestFlight → Mac:** add yourself to the internal group to install it
   with the TestFlight app on the Mac.

The secrets are the iPhone app's (`APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`,
`APPLE_API_ISSUER_ID`, `APPLE_TEAM_ID`, and `MACOS_CERT_P12` /
`MACOS_CERT_PASSWORD` / `APPLE_DEV_CERT` for the archive's development
identity: docs/IPHONE.md §6.1); the key's Admin role covers the Mac's
cloud-managed distribution and installer certificates.
