# iPhone App Store handoff: getting the iPhone version submitted

Paste the prompt below into Claude Code in a MiSTerKVMS checkout on a Mac
with Xcode and the iOS simulators. Written 27 October 2026.

---

You're getting the MiSTerKVMS iPhone app ready for App Store review, on
`main` (we don't use feature branches). `git pull`, then read
`docs/APP-STORE.md` (the checklist S1–S6, the App Store Connect steps and
the listing text), `docs/STORE-HANDOFF.md` ("Where things stand"),
`docs/IPHONE.md` §6 (building, TestFlight, export compliance) and
`git log --oneline -20`.

Rules: commit on `main` with clear messages. **Don't push unless I say
so**: a push to `main` that touches more than docs publishes a desktop
release and uploads the store apps that changed. Go **one step at a time**
for anything I do in App Store Connect: give me the step, wait for me.

## Where things stand

- **Mac App Store**: macOS 1.0.16 (build 48) is in review, submitted on its
  own. The iPhone version gets **its own submission**; don't add it to the
  Mac one.
- **Shared with the Mac, already done in App Store Connect**: App
  Information (subtitle, category Utilities / Entertainment), content
  rights (**yes**: MiSTer-Kun, see `apple/Icon/CREDITS.md`), App Privacy
  (Data Not Collected, the policy URL, published), age rating 4+, pricing
  Free. No encryption documentation: `ITSAppUsesNonExemptEncryption` is
  `false` (IPHONE.md §6). Check whether **EU trader status** was answered.
- **iPhone app**: on TestFlight from CI (tags `testflight/*`). Demo mode
  and the first-run screen are done (S1, S6).

## What's left, in order

### 1. S4: iPad check (code, simulator)

The app is iPhone-only (`TARGETED_DEVICE_FAMILY = 1`), so on iPad it runs
in compatibility mode, and review often tests there. Build for the
simulator (`scripts/build-ios.sh`), run it on an iPad simulator, and go
through: first-run screen, demo host, session screen (picture, touchpad,
keyboard and function bar, quick buttons and Arrange mode), Settings,
switching hosts. Fix anything cut off, misplaced or unusable; landscape on
iPad is the likely trouble spot.

### 2. S5: IPv6-only check (code; not a blocker for review)

Review's network is IPv6-only with NAT64, but reviewers have no MiSTer or
capture PC: they use the demo host, which needs no network, and a typed
address fails for them on any network. So this doesn't block submission;
do the S4 check, screenshots and App Store Connect first and come back to
it. It matters for real users away from home on IPv6-only mobile networks
who typed their home address as an IPv4 number (a name such as
`home.example.com` already works). The likely bug: both
connect paths, `tcp()` in `core/src/link.rs` (the capture server: the one
the phone uses) and `connect()` in `core/src/session.rs` (mrext),
resolve with Rust's `to_socket_addrs()` and take only the first address.

- An IPv4 literal ("192.168.1.20", or an internet address typed as an
  IP) is parsed by Rust directly, without `getaddrinfo`, so iOS never
  synthesizes the NAT64 IPv6 address: on an IPv6-only network it can't
  connect. Fix: resolve through `getaddrinfo` for literals too (on Apple,
  a literal passed to `getaddrinfo` with `AI_DEFAULT` comes back as the
  synthesized IPv6 address), for example with the `dns-lookup` crate or
  `libc::getaddrinfo` directly.
- Taking only the first address: try each resolved address in turn (IPv6
  first, as `getaddrinfo` orders them) before giving up, with the timeout
  spread over them.
- Discovery (`core/src/discover.rs`) is IPv4-only mDNS: fine, the review
  network has no capture server to find. The demo host needs no network.
- Test: core tests for the resolve-and-try logic; then for real, a Mac's
  **System Settings → General → Sharing → Internet Sharing** with ⌥ held
  offers **Create NAT64 Network**; a real iPhone joining that Wi-Fi is on
  an IPv6-only network (the sharing Mac itself isn't, so the simulator on
  it can't test this). Check typed addresses and names connect, and the
  error when nothing answers is clear.

### 3. Screenshots (6.9" iPhone, 1320×2868)

Use the **real MiSTers, not demo mode** (my preference). That needs this
computer on the home network and `local.settings.json` (git-ignored; copy
`local.settings.example.json`): MiSTer-SS1 at 192.168.99.92, MiSTer-Pi at
192.168.99.143, capture server on DANIHTPC (192.168.99.82, streams
`mkvm://192.168.99.82/MiSTer-SS1` and `/MiSTer-Pi`). The `misterkvms` MCP
tools (docs/MCP.md) press keys on them: `key menu` opens the MiSTer's core
menu (it closes itself after a while, so open it right before each shot).

- Simulator: an **iPhone 17 Pro Max** (6.9", 1320×2868 screenshots). The
  simulator shares this Mac's network, so it finds the capture server.
- Clean status bar first: `xcrun simctl status_bar booted override --time
  9:41 --batteryState charged --batteryLevel 100 --wifiBars 3
  --cellularBars 4`.
- Capture with `xcrun simctl io booted screenshot shot.png` (exact size, no
  permissions needed). Drive the app with the iOS Simulator tools.
- 3–10 shots, portrait: the host list with both MiSTers, a MiSTer's menu
  full screen, the keyboard with the function bar, the quick buttons, the
  stats, Settings.
- **Cover private details** with placeholders, as on the Mac: stream
  address → `mkvm://capture-pc/`, MiSTer IP → `192.168.1.50`, the viewer /
  device name → "My iPhone", and any internet address or pairing details.
  On the Mac this was done with Pillow in a scratch virtualenv
  (`python3 -m venv`, `pip install pillow`): paint each value over with a
  blend of the pixels left and right of it, then draw the placeholder in
  SF (`/System/Library/Fonts/SFNS.ttf`, weight Medium, matched to the size
  on screen). Look at every shot zoomed in before calling it done.
- App Store Connect refuses images with an alpha channel: save as JPEG (or
  PNG without alpha). Put them in `build/screenshots-iphone/` (git-ignored)
  and tell me where they are.

### 4. S2: demo video (me)

1–2 minutes on a real iPhone: add a MiSTer, picture and sound, touchpad,
typing, the remote, switching MiSTers, internet pairing. Unlisted YouTube;
the link goes in the review notes. Remind me; it can't be done here.

### 5. App Store Connect, iPhone version (me, one step at a time)

1. **Apps → MiSTerKVMS → iOS App** version page. Its version must match
   the build's (TestFlight → iOS builds); change it if not.
2. **Screenshots**: the 6.9" iPhone slot (Apple scales them for smaller
   ones).
3. **Promotional text, description, keywords**: APP-STORE.md §3. Support
   URL `https://github.com/danifunker/MiSTerKVMS/issues`, marketing URL
   `https://github.com/danifunker/MiSTerKVMS`, copyright `2026 Dani
   Sarfati`.
4. **Build**: the latest TestFlight build, with the S4 fixes if any were
   needed (CI uploads it after the push I approve).
5. **App Review Information**: sign-in not required, my contact, the notes
   from APP-STORE.md §2 step 7 plus the video link.
6. **Version Release**: manual.
7. **Add for Review**: the draft must hold only the iOS version, then
   **Submit for Review**.

## Notes

- Things that changed my answers on the Mac: content rights is **yes**
  (MiSTer-Kun), not no; App Privacy was "No, we do not collect data";
  App Encryption Documentation stays empty.
- If Apple has written back about the Mac review (App Store Connect → App
  Review), read it first: the same points will come up for the iPhone.
- Update `docs/APP-STORE.md` and `docs/STORE-HANDOFF.md` as items get
  done.
