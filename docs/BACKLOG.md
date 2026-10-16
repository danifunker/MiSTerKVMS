# Backlog

Not scheduled. The App Store release comes first ([APP-STORE.md](APP-STORE.md)).
The mrext wish list is in [PLANNING.md](PLANNING.md) §7 (we don't change mrext).

## iPhone app

- **Hardware keyboard**: real key presses (hold, repeat, any layout) from
  `pressesBegan`/`UIKey.keyCode` HID usages to the core's key events.
- **Hardware mouse and trackpad**: `GCMouse` relative movement and buttons;
  needs testing on a device with a mouse.
- **Game controller**: needs a virtual gamepad in mrext first.
- **iPad layout, landscape** (the iPhone app runs on iPad in compatibility
  mode until then).
- **Paste box**: type a pasted text into the MiSTer.
- **Pairing by QR code** from outside the home network (shown on the setup
  page).

## Video and network

- **H.264** (CAPTURE-SERVER.md C8, IPHONE.md I4): server encodes (hardware
  on Windows, openh264), clients decode (VideoToolbox on Apple); a bitrate
  cap for the internet. Until then, away from home needs a fast connection.
- **Opus audio** over the internet.
- **QUIC** transport (no head-of-line blocking on lossy links).

## Capture server

- **Linux, no frames after a while** (Alan): a stream once opened but sent no
  frames until the server restarted, possibly after the setup page's
  preview. Needs reproducing on real hardware.
- **Linux .deb and service on real hardware**: CI installs the .deb on a
  runner with the test pattern; ask Alan to try it with a card.
- **Capture server in the Mac App Store app** ("Share this Mac's capture
  cards"): the App Store build is client only (apple/AppStore/README.md).
  The sandbox allows the server (listening, Bonjour, AVFoundation capture,
  mrext), but not a boot-time service or self-update. So: split the server
  into a library the app runs in-process, a menu-bar icon so it keeps
  running without a window, and an "Open at login" switch
  (SMAppService.mainApp; Apple requires the user's consent). It runs only
  while someone is logged in; updates come from the App Store. (The DMG app
  runs it as a launch agent already: Settings > Capture Server.)
- **Linux ARM** (Raspberry Pi as a capture PC).
- **Tray and installer** outside Windows.

## Other apps

- **Android app** on the same Rust core (UniFFI has Kotlin bindings), with
  the iPhone app's pairing and secure transport.

## Housekeeping

- **Revoke the old Developer-role App Store Connect API key** once the
  Admin key has run CI for a while.
