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

### 1. S4: iPad (done 27 Oct)

The app is universal now, not iPhone-only: a native iPad app in every
orientation, with hardware keyboard and mouse/trackpad input on both
devices (IPHONE.md §5.6). Checked in the iPad Pro 13" simulator (first
run, demo, sessions in portrait and landscape, sheets, the real MiSTers).
The hardware keyboard was tested against the real Pi; the **mouse and
trackpad aren't tested** (the simulator tools can't send a mouse): try one
on a real iPad before submitting. These changes need a new TestFlight
build: **push to `main` when you approve** (CI uploads it).

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

### 3. Screenshots (done 27 Oct)

From the real MiSTers in the simulator, private details hidden
(APP-STORE.md §3b): `build/screenshots-iphone/` (6 × 1320×2868),
`build/screenshots-ipad/` (5 × 2752×2064), and an App Preview,
`build/video-iphone/app-preview-886x1920.mp4` (29 s, silent). Retake them
the same way if the UI changes: `xcrun simctl status_bar booted override
--time 9:41 …`, `xcrun simctl io <device> screenshot`, the
`misterkvms-cli key menu --host ss1|pi` right before each shot.

### 4. S2: demo video (me)

1–2 minutes on a real iPhone: add a MiSTer, picture and sound, touchpad,
typing, the remote, switching MiSTers, internet pairing. Unlisted YouTube;
the link goes in the review notes. Remind me; it can't be done here.

### 5. App Store Connect, iPhone version (me, one step at a time)

1. **Apps → MiSTerKVMS → iOS App** version page. Its version must match
   the build's (TestFlight → iOS builds); change it if not.
2. **Screenshots**: the 6.9" iPhone slot (Apple scales them for smaller
   ones) and the 13" iPad slot; the App Preview in the 6.9" slot if
   wanted.
3. **Promotional text, description, keywords**: APP-STORE.md §3. Support
   URL `https://github.com/danifunker/MiSTerKVMS/issues`, marketing URL
   `https://github.com/danifunker/MiSTerKVMS`, copyright `2026 Dani
   Sarfati`.
4. **Build**: the TestFlight build with the iPad and hardware input
   (CI uploads it after the push I approve).
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
