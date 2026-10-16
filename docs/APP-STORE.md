# MiSTerKVMS on the App Store

Plan for taking the iPhone app from TestFlight to a free App Store app.
Everything not needed for that is in [BACKLOG.md](BACKLOG.md).

Status: TestFlight builds go out from CI (IPHONE.md §6.1); export compliance
is declared (exempt). S1 and S6 are done (demo mode, first-run screen,
credits in Settings → About); the rest is to do.

## 1. What review needs that we don't have yet

| # | Item | Who | Notes |
|---|---|---|---|
| S1 | **A way for review to try the app** (done) | code | Reviewers have no MiSTer or capture PC, and pairing only works on the home network, so a public demo server can't work. **Demo mode** (first-run screen: "Try it without a MiSTer"; Settings → Demo mode) adds a Demo host: the built-in test pattern (`capture/src/pattern.rs`, rainbow bars and a beep) with MiSTer-Kun stepping across it in place of the white box (`test:demo`), through the same video, sound, touchpad and keyboard paths. The core writes the input a MiSTer would have received as lines over the picture ("Shift+Tab", "double click", "mouse moved +120, −40"), each gone after 5 s (`core/src/demo.rs`). Also gives the App Store screenshots something to show. |
| S2 | **Demo video** | you | Apple usually asks for one for apps that need hardware: 1–2 minutes on a real iPhone: add a MiSTer, the picture and sound, the touchpad, typing, the remote, switching MiSTers, internet pairing. An unlisted YouTube link in the review notes. |
| S3 | **Privacy policy** | code + you | Required for every app. Draft in `docs/PRIVACY.md` (the app collects nothing; what it sends goes only to the user's own capture PC: the phone's name, input, its key). Its GitHub URL works as the policy URL. |
| S4 | **iPad check** | code | An iPhone-only app (`TARGETED_DEVICE_FAMILY = 1`) still runs on iPad in compatibility mode, and review often tests there. Check the session screen, keyboard and remote in the iPad simulator. |
| S5 | **IPv6-only check** | code | Review's network is IPv6-only (NAT64). The demo needs no network; connecting by name must not assume IPv4. Check typed addresses and names resolve to IPv6, and the error when nothing answers is clear. |
| S6 | **First-run pass** (done) | code | A first-run screen (only on a phone with no MiSTers saved) says what's needed and offers the demo; the empty host list says the same and offers it too. Settings → About has the version and the credits (MiSTer-Kun, MiSTer, mrext), as in the desktop apps. |

## 2. App Store Connect (you; ~1 hour once S1–S3 exist)

1. **App Information**: name *MiSTerKVMS*, subtitle (30 characters, e.g.
   "Remote screen for MiSTer FPGA"), category Utilities (secondary:
   Entertainment), content rights: no third-party content.
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
   the 6.9" iPhone (1320×2868; Apple scales them for smaller ones; 3–10),
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

> MiSTerKVMS puts your MiSTer FPGA on your iPhone: its screen and sound
> from a capture card, and its keyboard and mouse under your fingers.
>
> - The picture fills the screen; the screen is a touchpad for the mouse.
> - The iPhone keyboard types on the MiSTer, with a bar for function keys,
>   Esc, Tab and arrows.
> - A small remote you can move and fold away: arrows, OK, F12 for the
>   MiSTer menu, backspace.
> - Several MiSTers: swipe with three fingers to switch.
> - At home it finds your capture PC on its own. Away from home, pair once
>   on your Wi-Fi and connect over the internet, encrypted with a key for
>   this iPhone only.
>
> Needs a MiSTer FPGA running mrext Remote, a capture card showing it, and
> MiSTerKVMS Capture (free, Windows, Mac or Linux) on the computer the card
> is plugged into.

## 4. Order

1. ~~S1 demo host, S6~~ (done); S4–S5 checks.
2. S3 privacy policy page; S2 video (you, with the TestFlight build).
3. Screenshots from the demo host in the simulator.
4. App Store Connect (§2), submit, answer review.
