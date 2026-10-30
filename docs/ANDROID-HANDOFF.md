# Android handoff: the Android app and its Play Store pipeline

Paste the prompt below into Claude Code on the computer you'll build Android
on, in the MiSTerKVMS checkout.

---

You're continuing work on MiSTerKVMS on `main` (we don't use feature
branches). `git pull`, then read `README.md`, `docs/IPHONE.md` (the iPhone
app's plan: its decisions, pairing, secure transport, the app, CI),
`docs/APP-STORE.md`, `docs/CAPTURE-SERVER.md` and `git log --oneline -15`.

Rules: commit on `main` as you go with clear messages. **Don't push unless I
say so**: every push to `main` that touches more than docs publishes a
release (and uploads the store apps that changed). Don't change mrext, never
edit the MiSTers' ini files, and don't change the mkvm protocol without
updating the `mkvm` tests (the server and every app speak it).

## The goal

An **Android version of the iPhone app** (`ios/`): the same remote-control
app, a thin shell on the shared Rust core, published on **Google Play**, with
a CI pipeline that works like the iPhone and Mac App Store ones: every push
to `main` that changes the app uploads a signed build to a Play testing
track; production releases are promoted by hand.

Agreed so far:

| # | Decision |
|---|---|
| A1 | Same scope as the iPhone app's first version (IPHONE.md P1–P17): phone, portrait, connects to MiSTerKVMS Capture only (never mrext), LAN without a key, internet with a paired device key over the Noise transport, touchpad, system keyboard + function bar, movable quick buttons, show/hide menu, one MiSTer at a time with a three-finger swipe to switch, demo mode. |
| A2 | Package name **`com.danifunker.misterkvms`** (same as the iOS bundle ID). Permanent once the first bundle is uploaded. |
| A3 | Kotlin + Jetpack Compose shell; the core through **UniFFI Kotlin bindings** from the existing `uniffi-bindgen`. Logic stays in Rust. |
| A4 | Target the API level Play requires: **Android 16 (API 36)** for new apps and updates since 31 Aug 2026. Keep it in one place in Gradle; Google raises it every August. |
| A5 | CI uploads to the **internal testing** track; promoting to closed testing / production is manual in Play Console (like "release manually" on the App Store). |

Ask me before deciding anything not covered here (e.g. minimum Android
version: suggest one, API 29+ is a reasonable default for Vulkan and AAudio).

## What the iPhone app is (for parity)

`ios/MiSTerKVMS/`, about 3,800 lines of SwiftUI:

- `Views/`: `WelcomeView` (first run, "Try it without a MiSTer"),
  `HostListView`, `AddHostView` (discovered streams or typed address, "Also
  use over the internet"), `HostSettingsView`, `AppSettingsView` (device
  name, demo mode, About with credits), `SessionScreen`, `QuickButtons`
  (arrows/OK, F12, ⌫; Arrange mode, collapse chevron), `TypeTextScreen`.
- `Input/`: `TouchpadView` (gestures in IPHONE.md §5.3), `KeyboardHost`
  (system keyboard → core's US text mapping, function bar with sticky
  modifiers).
- `Model/`: `AppModel` (drives the core's `Controller`), `DeviceKeychain`
  (device keys, this device only), `StreamBrowser` (`NWBrowser` for
  `_mkvm._tcp`), `Keys`.
- `PrivacyInfo.xcprivacy`, `Info.plist` (local network strings, Bonjour).

Read these before writing the Android equivalents; keep names and behaviour
the same so the two apps stay easy to compare.

## Technical notes (things you'll hit)

- **Toolchain on this computer:** JDK 17+, Android SDK + NDK (Android Studio
  or `cmdline-tools`; NDK r27 or newer), `rustup target add
  aarch64-linux-android x86_64-linux-android`, `cargo install cargo-ndk`.
- **The core on Android:** it builds as a `cdylib` already; `cargo ndk -t
  arm64-v8a -t x86_64 build -p kvms-core --release` → `libkvms_core.so` into
  `jniLibs/`. Check every `cfg(target_os = "ios")` / `"macos"` in `core/`,
  `capture/`, `codec/` and add `android` where it belongs (local capture is
  off on Android, as on iOS; `capture/` should give the stubs).
- **Video surface:** `NativeSurface` (`core/src/video/mod.rs`) has `AppKit`,
  `UiKit`, `Win32`, `CompositionHandle`. Add an Android one carrying the
  `ANativeWindow*` from a `SurfaceView`'s `Surface`
  (`ANativeWindow_fromSurface` through JNI, or `ndk`/`jni` crates);
  `raw-window-handle`'s `AndroidNdkWindowHandle`; wgpu uses Vulkan. Handle
  surface destroy/recreate (rotation, background) without dropping the
  session.
- **Video decode:** MJPEG in `codec/` is pure Rust and works as is. H.264
  (IPHONE.md I4) isn't built yet; when it is, Android decodes with
  `MediaCodec`.
- **Audio:** cpal uses AAudio on Android. Request audio focus in the app.
- **Discovery:** the core's `mdns-sd` is compiled in but, as on iOS, use the
  platform browser: `NsdManager` for `_mkvm._tcp`, pass the address to the
  core. Permissions: `INTERNET`, `ACCESS_NETWORK_STATE`,
  `ACCESS_WIFI_STATE`, and `NEARBY_WIFI_DEVICES` if the API level asks for
  it for NSD.
- **Device keys:** Android Keystore (an AES key in the Keystore wrapping the
  device keys in app storage, not backed up: `android:allowBackup` off or a
  backup rule excluding them). Never write them to `profiles.json`, as on
  iOS.
- **Keyboard:** a hidden view with a custom `InputConnection` so the soft
  keyboard sends characters (`commitText`) and Backspace/Enter
  (`deleteSurroundingText`, `sendKeyEvent`) to the core's text mapping;
  same function bar as iOS. Hardware keyboards are out of scope (A1).
- **Lifecycle:** close the connection in `onStop` (releasing control), and
  reconnect in `onStart`; reconnect on network changes
  (`ConnectivityManager.NetworkCallback`), picking LAN or internet again,
  like iOS.
- **UniFFI Kotlin:** `uniffi-bindgen generate --language kotlin` from the
  built library; the bindings use JNA (`net.java.dev.jna:jna:…@aar`).

## Test setup

- Capture server on Windows (DANIHTPC, 192.168.99.82) streams `MiSTer-SS1`
  (→ MiSTer 192.168.99.92) and `MiSTer-Pi` (→ 192.168.99.143). Setup page
  http://192.168.99.82:5971.
- Demo mode needs no network: use it for emulator work and screenshots.
- `misterkvms-cli streams` / `stream <address>` to check the server from the
  computer.

## Milestones

| # | Milestone | Done when |
|---|---|---|
| D0 | Core on Android | `kvms-core` builds with `cargo-ndk` for arm64-v8a and x86_64; Android `NativeSurface`; Kotlin bindings generated by `scripts/build-android.sh` |
| D1 | Picture | an `android/` Gradle project shows a real capture stream (and the demo) in a `SurfaceView` in the emulator and on a phone, with sound |
| D2 | Parity on the LAN | everything in "What the iPhone app is" except internet use |
| D3 | Internet | pairing on the LAN, keys in the Keystore, secure transport from cellular through the forwarded port |
| D4 | Signed release build | `scripts/build-android.sh --release` makes a signed `.aab` with the upload key; I upload the first one by hand (below) |
| D5 | CI | the `build-android` job (below) uploads to the internal track; `docs/ANDROID.md` written like IPHONE.md, with this setup list |
| D6 | Play listing | store listing, screenshots from demo mode, App content forms, closed test if required, production |

Report after each milestone; write `docs/ANDROID.md` (decisions table,
layout, building, CI, Play setup) once D1 works.

## CI: the `build-android` job

Model it on `build-ios` in `.github/workflows/release.yml`:

- Runs on `ubuntu-latest` next to the other jobs, `needs: version`. A
  failure doesn't hold back the desktop release (not in `publish`'s needs).
- Pull requests, or missing secrets: build an unsigned/debug build only.
- Push to `main` with all secrets set: signed `.aab`, upload to the
  **internal** track with the Play Developer API (Gradle Play Publisher,
  `fastlane supply` or `r0adkll/upload-google-play`; pick one and say why).
- **Only when the app changed:** add `android` to
  `scripts/app-fingerprint.sh` (`TARGET=aarch64-linux-android`,
  `OWN=(android scripts/build-android.sh)`), compare in the `version` job
  against the newest `play/<versionCode>` tag, tag each upload
  `play/<versionCode>` with the fingerprint as the message. The manual-run
  checkbox ("Upload the iPhone and Mac App Store apps…") should cover Play
  too: rename it to say so.
- `versionName` = the release version (1.0.N); `versionCode` = the workflow
  run number (only grows, as Play requires).
- Secrets: `ANDROID_UPLOAD_KEYSTORE` (base64 `.jks`),
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`,
  `PLAY_SERVICE_ACCOUNT_JSON`. Never print them; decode the keystore to
  `$RUNNER_TEMP` and delete it in an `always()` step.
- Update the workflow's header comment and README (Install: Android).

## Play Console setup (me; list it in ANDROID.md too)

The account was recently re-enabled. Whether the **12-tester / 14-day closed
test** applies depends on when it was first created (personal accounts
created after 13 Nov 2023; organization accounts are exempt): if the
dashboard shows an "Apply for production access" checklist, it applies.

1. **Create app:** MiSTerKVMS, App, Free.
2. **First bundle by hand:** the API can't upload until one bundle is
   uploaded in the Console. Upload the D4 `.aab` to Internal testing and
   accept **Play App Signing** (Google keeps the app signing key; we keep
   the upload key).
3. **Upload key:** `keytool -genkeypair -v -keystore upload.jks -alias upload
   -keyalg RSA -keysize 4096 -validity 10000`; back it up; set the four
   `ANDROID_*` secrets (`base64 -i upload.jks | gh secret set
   ANDROID_UPLOAD_KEYSTORE`).
4. **Service account:** Google Cloud project → enable *Google Play Android
   Developer API* → service account → JSON key. Play Console → Users and
   permissions → invite its email → this app: *Release to testing tracks*.
   `gh secret set PLAY_SERVICE_ACCOUNT_JSON < key.json`.
5. **Internal testing:** tester list with me.
6. **Closed testing** (only if required): 12+ testers opted in for 14
   continuous days, then apply for production access. Start early.
7. **App content:** privacy policy (`docs/PRIVACY.md`, shared with iOS: it doesn't exist yet, APP-STORE.md S3; write it if it's still missing),
   Data safety (answers in STORE-HANDOFF.md), Ads (none), content rating
   questionnaire, target audience (not children), App access (reviewers use
   demo mode: "Try it without a MiSTer").
8. **Store listing:** short description (80 chars), full description
   (adapt APP-STORE.md §3), 512×512 icon, 1024×500 feature graphic, 2–8
   phone screenshots from demo mode.
