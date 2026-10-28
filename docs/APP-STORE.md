# MiSTerKVMS on the App Store

Plan for taking the iPhone app from TestFlight to a free App Store app. The
Mac app can join the same listing (universal purchase): a sandboxed,
client-only build of it, see [apple/AppStore/README.md](../apple/AppStore/README.md).
Everything not needed for that is in [BACKLOG.md](BACKLOG.md).

Status: TestFlight builds go out from CI (IPHONE.md §6.1); export compliance
is declared (exempt). S1, S4 and S6 are done (demo mode, iPad, first-run
screen, credits in Settings → About); screenshots are taken (§3b).

## 1. What review needs that we don't have yet

| # | Item | Who | Notes |
|---|---|---|---|
| S1 | **A way for review to try the app** (done) | code | Reviewers have no MiSTer or capture PC, and pairing only works on the home network, so a public demo server can't work. **Demo mode** (first-run screen: "Try it without a MiSTer"; Settings → Demo mode) adds a Demo host: the built-in test pattern (`capture/src/pattern.rs`, rainbow bars and a beep) with MiSTer-Kun stepping across it in place of the white box (`test:demo`), through the same video, sound, touchpad and keyboard paths. The core writes the input a MiSTer would have received as lines over the picture ("Shift+Tab", "double click", "mouse moved +120, −40"), each gone after 5 s (`core/src/demo.rs`). Also gives the App Store screenshots something to show. |
| S2 | **Demo video** | you | Apple usually asks for one for apps that need hardware: 1–2 minutes on a real iPhone: add a MiSTer, the picture and sound, the touchpad, typing, the remote, switching MiSTers, internet pairing. An unlisted YouTube link in the review notes. |
| S3 | **Privacy policy** | code + you | Required for every app. Written: `docs/PRIVACY.md` (iPhone, Mac App Store and Android apps; they collect nothing, and what they send goes only to the user's own capture PC and MiSTers). Policy URL: https://github.com/danifunker/MiSTerKVMS/blob/main/docs/PRIVACY.md |
| S4 | **iPad** (done) | code | The app is universal now (IPHONE.md §5.6): a native iPad app in every orientation, with hardware keyboard and mouse input. Checked in the iPad Pro 13" simulator: first-run screen, demo, session in portrait and landscape, sheets, the real MiSTers. A universal app needs **13" iPad screenshots** too. |
| S5 | **IPv6-only check** | code | Review's network is IPv6-only (NAT64). The demo needs no network; connecting by name must not assume IPv4. Check typed addresses and names resolve to IPv6, and the error when nothing answers is clear. |
| S6 | **First-run pass** (done) | code | A first-run screen (only on a phone with no MiSTers saved) says what's needed and offers the demo; the empty host list says the same and offers it too. Settings → About has the version and the credits (MiSTer-Kun, MiSTer, mrext), as in the desktop apps. |

## 2. App Store Connect (you; ~1 hour once S1–S3 exist)

1. **App Information**: name *MiSTerKVMS*, subtitle (30 characters, e.g.
   "Remote screen for MiSTer FPGA"), category Utilities (secondary:
   Entertainment), content rights: yes, third-party content with the rights (MiSTer-Kun in the icon and the demo, given to the community to use freely: apple/Icon/CREDITS.md).
2. **Pricing and Availability**: Free, all countries. A free app needs no
   Paid Apps agreement, banking or tax forms.
3. **EU trader status** (Digital Services Act): required to sell in the EU.
   A free hobby app can declare **not a trader**; a trader's address and
   phone are shown publicly on the listing. Your call.
4. **App Privacy**: privacy policy URL (S3); data collection **Data Not
   Collected** (nothing reaches us; the capture PC is the user's own).
5. **Age rating** questionnaire: no objectionable content, unrestricted web
   access no → 4+.
6. **Version page** (1.0.N, matching the TestFlight build): screenshots for
   the 6.9" iPhone (1320×2868; Apple scales them for smaller ones; 3–10)
   and the 13" iPad (2752×2064 landscape), an optional App Preview video,
   description, keywords (100 characters), support URL (the GitHub repo's
   issues), copyright, the build.
7. **App Review Information**: your contact; sign-in not required; notes:
   what the app is for, that it needs a MiSTer FPGA with mrext and a capture
   PC, the video link (S2), and how to try it without them: *on first launch
   tap "Try it without a MiSTer" (or Settings → Demo mode, then Demo in the
   list). It shows a test pattern with sound; tapping, dragging, typing and
   the on-screen remote are listed on screen as what the MiSTer would
   receive.*
8. **Release**: manual after approval, so you choose the day.

## 3. Listing text (draft)

**Subtitle:** Remote screen for MiSTer FPGA

**Keywords:** MiSTer,FPGA,KVM,remote,retro,emulation,capture,keyboard,mouse,touchpad

**Description:**

> MiSTerKVMS puts your MiSTer FPGA on your iPhone or iPad: its screen and
> sound from a capture card, and its keyboard and mouse under your fingers.
>
> - The picture fills the screen; the screen is a touchpad for the mouse.
> - The iPhone keyboard types on the MiSTer, with a bar for function keys,
>   Esc, Tab and arrows.
> - A small remote you can move and fold away: arrows, OK, F12 for the
>   MiSTer menu, backspace.
> - A hardware keyboard, mouse or trackpad works too: on an iPad with a
>   Magic Keyboard the MiSTer gets real key presses.
> - Several MiSTers: swipe with three fingers to switch.
> - At home it finds your capture PC on its own. Away from home, pair once
>   on your Wi-Fi and connect over the internet, encrypted with a key for
>   this iPhone only.
>
> Needs a MiSTer FPGA running mrext Remote, a capture card showing it, and
> MiSTerKVMS Capture (free, Windows, Mac or Linux) on the computer the card
> is plugged into.

## 3a. The Mac version (same listing)

App Information (name, subtitle, category, privacy, age rating) is shared
with the iPhone app; the macOS version page has its own screenshots,
description, keywords and build (`testflight-mac/*` from CI).

**Screenshots:** 16:10, 1–10 of them; 2880×1800 is the largest size taken.
Real MiSTers, not the demo: the host list with both MiSTers, a MiSTer's
menu, another core's picture, the toolbar (screenshot, record), Settings.
Size the window to 1440×900 points (the Dock may cap it a little shorter),
capture it without its shadow (⌘⇧4, Space, ⌥-click), then
`scripts/mac-appstore-screenshots.sh build/screenshots-mac ~/Desktop/Screenshot*.png`
fits each to 2880×1800 and writes JPEGs (App Store Connect refuses alpha).
Taken 27 Oct from TestFlight build 46: the Pi's and SS1's core menus, the
stats popover and Settings, with the stream address, MiSTer address and
viewer name replaced by placeholders (`mkvm://capture-pc/`, `192.168.1.50`,
"My Mac").

**Keywords:** MiSTer,FPGA,KVM,remote,retro,emulation,capture,keyboard,mouse,record

**Description:**

> MiSTerKVMS puts your MiSTer FPGA in a window on your Mac: its screen and
> sound from a capture card, and your Mac's keyboard and mouse driving it.
>
> - Click the picture and the keyboard and mouse go to the MiSTer; press
>   Right ⌘ + Right ⌥ to get them back. F12 opens the MiSTer menu.
> - Fit, fill or integer scaling, zoom, and MiSTer's black borders cropped.
> - Screenshots (copied or saved) and recordings with sound, to Pictures
>   and Movies.
> - Several MiSTers in the sidebar, each with its picture and the core it's
>   running.
> - Finds MiSTerKVMS Capture on your network on its own, or uses a capture
>   card plugged into this Mac.
>
> Needs a MiSTer FPGA running mrext Remote, and a capture card showing it:
> plugged into this Mac, or into a computer running MiSTerKVMS Capture
> (free, Windows, Mac or Linux).

**Review notes (Mac):** as for the iPhone, plus: *the Mac app also offers
the demo: hold the + button in the sidebar → Add Demo Host, or "Try
Without a MiSTer" when no hosts are saved. Sandboxed: system shortcuts such
as ⌘Tab stay with macOS, the rest of the keyboard goes to the MiSTer.*

## 3b. iPhone and iPad screenshots and preview

Taken 27 Oct in the simulator from the real MiSTers (SS1 and the Pi through
the capture server), with the status bar at 9:41. Nothing private shows:
the host list and switcher show stream names only; the device name was set
to "My iPhone" (the iPad shows the "My iPad" placeholder); the build's
version in Settings was painted over as 1.0.22 (Pillow, SF, colours sampled).
JPEG, no alpha, in `build/` (git-ignored):

- `build/screenshots-iphone/` (6.9", iPhone 18 Pro Max, 1320×2868): the
  Pi's menu with the keyboard, SS1 full screen with stats, the host list,
  the MiSTer switcher, the controls sheet, Settings.
- `build/screenshots-iphone-6.3/`: the same six scaled to 1206×2622. App
  Store Connect (Oct 2026) asks for "iPhone with Dynamic Island (medium
  display)", 6.1"/6.3", not 6.9".
- `build/screenshots-duo/` (iPhone Duo, unfolded, 1398×2034; iOS 27.1
  simulator): the Pi with the keyboard (quick buttons folded), the Pi full
  screen with stats, the host list, the switcher, the controls sheet,
  Settings. The Duo's status column takes the right edge; the toolbar
  buttons move into it on its own.
- `build/screenshots-ipad/` (13", iPad Pro M5, 2752×2064 landscape): the
  Pi's menu, SS1's system settings, the switcher, the controls sheet,
  Settings.
- `build/video-iphone/app-preview-886x1920.mp4`: a 29 s App Preview
  (886×1920, H.264, silent stereo AAC) cut from a 68 s simulator walkthrough
  (`walkthrough-1080.mp4`): host list, the Pi, D-pad and typing in its menu,
  switching to SS1, hiding the keyboard. No sound (the simulator records
  none), so it's not the S2 review video.

## 4. Order

1. ~~S1 demo host, S6, S4 iPad~~ (done); S5 IPv6 check.
2. ~~S3 privacy policy page~~; S2 video (you, with the TestFlight build).
3. ~~Screenshots~~ (§3b; real MiSTers, not the demo).
4. App Store Connect (§2), submit, answer review.
