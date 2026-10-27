# Store publishing handoff: what's left for Google Play and the App Store

Paste the prompt below into Claude Code in the MiSTerKVMS checkout to pick
the store work up again. Written 25 October 2026.

---

You're continuing the store publishing work for MiSTerKVMS on `main` (we
don't use feature branches). `git pull`, then read `docs/ANDROID.md` (the
Android app: decisions, CI, Play Console setup and what's done),
`docs/APP-STORE.md` (the iPhone/Mac App Store checklist S1–S6 and listing
draft), `docs/PRIVACY.md` and `git log --oneline -20`.

Rules: commit on `main` with clear messages. **Don't push unless I say
so**: a push to `main` that touches more than docs publishes a desktop
release and uploads the store apps that changed (iPhone, Mac App Store,
and Android to Play's internal track). Go **one step at a time** for
anything I do in a console or a browser (Play Console, App Store Connect):
give me the step, wait for me. Before asking me whether something is set
up, check what you can yourself: `gh secret list` / `gh variable list`,
the capture server's API on this PC (`curl http://127.0.0.1:5971/api/remote`,
`/api/state`), Play/CI tags (`play/*`, `testflight/*`).

## Where things stand

- **Android app** (D0–D5 of the old ANDROID-HANDOFF.md): done and tested on
  the emulator (demo, MiSTer-SS1's stream, touchpad, keyboard, quick
  buttons, menus, internet pairing with the key in the Keystore).
  `1 (1.0.15)` was uploaded by hand to Play's **Internal testing**; testers
  see the temporary name "com.danifunker.misterkvms (unreviewed)" until the
  listing is reviewed.
- **Android CI**: the `build-android` job is in `release.yml`, and it
  uploads. Run #48 (26 Oct, third attempt) signed `48 (1.0.16)`, uploaded
  it to Internal testing and tagged `play/48`. It took two fixes, both
  outside the repository:
  - "Google Play Android Developer API has not been used in project
    788191439451": the API had never been enabled in the service
    account's Google Cloud project. Enabled it in the Cloud console.
  - "The caller does not have permission" when committing the edit (the
    bundle had uploaded): the service account's permission for this app in
    Play Console (Users and permissions → the account → App permissions →
    MiSTerKVMS: *Release apps to testing tracks*) wasn't in effect yet.
    Fixed in Play Console, then re-ran the failed job.
- **Privacy policy**: `docs/PRIVACY.md`, pushed. URL for both stores:
  https://github.com/danifunker/MiSTerKVMS/blob/main/docs/PRIVACY.md
- **iPhone app**: on TestFlight from CI (latest tag `testflight/47`). The
  Mac App Store build uploads too (`MAC_APP_STORE_UPLOAD=true`).

## What's left

### Google Play (D6)

1. ~~**First CI upload.**~~ Done: `play/48` (see "Where things stand").
   Small fix still open: `ANDROID_KEY_ALIAS` is a secret whose value is
   `upload`, so GitHub masks the word "upload" everywhere in the logs (run
   #48 shows `r0adkll/***-google-play`, "Successfully ***ed"); make it a
   repository variable (`vars.ANDROID_KEY_ALIAS`), or the literal `upload`
   in the workflow, and delete the secret.
2. **App content** (Play Console → Policy and programs → App content), one
   form at a time:
   - Privacy policy: the URL above.
   - **Data safety:** check Google's current definition of "collected"
     first (the device's name and input leave the phone, but only to the
     user's own capture PC; nothing reaches the developer), then answer;
     the policy says the apps collect nothing.
   - Ads: none. App access: everything works without an account; reviewers
     use demo mode ("Try it without a MiSTer" on first launch). Content
     rating questionnaire (expect Everyone). Target audience: not children
     (13+ or 18+). News app: no. Government app, financial and health
     features: no.
3. **Store listing**: app name MiSTerKVMS; short description (80
   characters); full description adapted from APP-STORE.md §3 (Android
   wording: "phone", the soft keyboard); app icon 512×512 (from
   `apple/Icon`, the iPhone icon); feature graphic 1024×500 (make one);
   2–8 phone screenshots from demo mode on the emulator (portrait, no
   status bar clutter); category Tools or Entertainment; contact email.
4. ~~**Real launcher icon**~~ Done (27 Oct): the iPhone icon's artwork as an
   adaptive icon, written by `python apple/Icon/make_icon.py --android`
   (foreground PNGs per density, gradient background); the white display
   outline stays as the themed (monochrome) icon.
5. **Closed test or not**: the personal account looked exempt from the
   12-tester, 14-day closed test. Once setup is done, check **Test and
   release → Production**: if it asks for a closed test, start one early.
6. **Production**: promote a tested internal build by hand.

### App Store (APP-STORE.md)

1. **S4 iPad check** and **S5 IPv6-only check** (code; simulator).
2. **S2 demo video** (me, on a real iPhone): 1–2 minutes, unlisted YouTube.
3. **Screenshots** for the 6.9" iPhone (1320×2868) from demo mode in the
   simulator.
4. **App Store Connect** (APP-STORE.md §2): App Information, pricing (free),
   EU trader status (my call), App Privacy (the policy URL; Data Not
   Collected), age rating, version page, review notes, manual release.

### Not yet tested on real hardware (no Android phone here yet)

Sound on Android; stream discovery on a real network (the emulator can't
see mDNS); three-finger gestures (Android Studio's Device Streaming has
real phones); using a MiSTer over the internet from outside home
(`home.dani.tech:5970`). Play's pre-launch report runs each internal
upload on real devices: read the one for `48 (1.0.16)`.

### Housekeeping

- The capture PC has a paired device "Android emulator" from testing:
  keep it while testing on the emulator, revoke it on the setup page after.
- The debug build takes ~5 s to show its first screen on the emulator (an
  unoptimised Compose build); check the release build's cold start on a
  fresh emulator boot.
