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
- **Mac App Store**: macOS 1.0.16 (build 48) submitted for review on its
  own on 27 Oct, manual release. Screenshots are from the real MiSTers
  (APP-STORE.md §3a; private details replaced); listing text and review
  notes as in §3a. App Information, App Privacy (Data Not Collected, the
  policy URL, published), content rights (yes: MiSTer-Kun), age rating and
  pricing are shared with the iPhone app and done. If review answers, it's
  in App Store Connect → App Review.

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
   - **Data safety** (redo after the release with encryption, 1.0.26+;
     reasoning in "Data safety answers" below):
     - Collects data: **Yes**. Shared: **No**.
     - Device or other IDs: the device name (and, when paired, the key id
       the capture PC issued). Collected, not shared, required, for app
       functionality. Not processed ephemerally (the capture PC lists paired
       devices and logs names).
     - App activity, other user-generated content (keys, clicks, typed
       text): collected, **processed ephemerally**, required, app
       functionality. Not shown on the listing.
     - Encrypted in transit: **Yes** (every connection, IPHONE.md §3).
     - Users can request deletion: **Yes** (in the app, and the setup page;
       PRIVACY.md "Removing a device").
   - **Store listing line** for the device name, e.g.: "MiSTerKVMS sends
     your phone's name to your own MiSTerKVMS Capture computer, which shows
     it to others watching the same MiSTer ("Pixel has control") and lists
     it under paired devices. Nothing is sent to the developer."
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

The iPhone version goes in its own submission (the Mac one is in review).

1. ~~S4~~: the app is universal now (iPad, hardware keyboard and mouse;
   IPHONE.md §5.6); needs a push for a new TestFlight build. **S5 IPv6-only
   check** (code; not a blocker).
2. **S2 demo video** (me, on a real iPhone): 1–2 minutes, unlisted YouTube.
3. ~~Screenshots~~: iPhone 6.9", iPad 13" and an App Preview, in `build/`
   (APP-STORE.md §3b).
4. **App Store Connect**: the shared parts were done for the Mac (see
   "Where things stand"); left for the iPhone: its version page, review
   notes and manual release. EU trader status: check it was answered.

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

## Data safety answers (Google Play)

Google counts as "collected" anything the app sends off the device, whoever
receives it, so the device name and the input count, though they go only to
the user's own capture PC. Passed straight on (input) is still declared,
marked processed ephemerally, and then not shown on the listing. The
end-to-end exemption doesn't fit: the capture PC decrypts and acts on the
data. "Encrypted in transit" may be claimed only if it covers all such data,
which it does since 1.0.26: one encrypted protocol, no plain fallback, the
key id inside the encryption. Deletion: there is no prescribed mechanism, only that it be easy
to find; the app removes its own pairing, and PRIVACY.md and the app point
to the setup page for the others.
