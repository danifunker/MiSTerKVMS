# MiSTerKVMS privacy policy

Effective 30 October 2026.

This policy covers the MiSTerKVMS apps distributed through app stores: the
iPhone app, the Mac App Store app and the Android app. They're free, open
source (https://github.com/danifunker/MiSTerKVMS, MIT License) and made by
one developer, Dani Sarfati.

## In short

The apps have no accounts, ads, analytics, crash reporting or tracking,
and nothing reaches the developer or any third party. What they send (your
keyboard and mouse input and this device's name) goes only to equipment you
set up and run yourself: your MiSTerKVMS Capture computer, a server you host
at home. Your input is only there to control your MiSTer: it's passed
straight on and never stored, and nothing tracks what you do. The capture
computer keeps only this device's name and pairing key (below), and shares
them with no one. On
the Mac the app can also talk to your MiSTers and to a capture card plugged
into the Mac.

## What the apps send, and where

MiSTerKVMS shows a MiSTer FPGA's screen and plays its sound, and sends it
keyboard and mouse input. To do that the apps connect to:

- **MiSTerKVMS Capture**, the program you run on your own computer with a
  capture card. The app receives its picture and sound, and sends it your
  input (keys, mouse movements and clicks, text you choose to type on the
  MiSTer) and this device's name. Other people watching the same MiSTer
  see that name while you have control ("Dani's phone has control").

  Your input is used only to control the MiSTer. The capture computer
  passes each key, click and movement straight on to the MiSTer and doesn't
  store or log it; while you have control it remembers in memory only which
  keys are held down, so it can release them when someone else takes over.
  Nothing records, analyses or tracks what you do with your MiSTer.
- **Your MiSTers** (the Mac App Store app only), through mrext Remote on
  your network, for the same input.
- **Your local network**, to find capture servers (Bonjour / DNS-SD). This
  only looks on the network you're on.

When you turn on **Use over the internet** for a MiSTer (iPhone and
Android), the app pairs with your capture computer while you're on the
same network. The capture computer gives this device its own key and keeps
a list of paired devices on that computer: each one's name, when it was
paired and last connected, and the address it last connected from. Its log
file notes when a device pairs and who has control, by the device's name,
never what anyone typed or did.

## Encryption

Everything the apps send to your capture computer is encrypted (Noise, with
ChaCha20-Poly1305), at home and away, in every app. They don't connect to a
capture computer that can't encrypt (MiSTerKVMS Capture before 1.0.26):
update it there.

The first time an app connects to a capture computer it remembers that
computer's identity, and won't connect if another computer answers in its
place. The Mac App Store app's direct connection to a MiSTer (mrext Remote)
isn't encrypted: mrext doesn't offer that, and it stays on your network.

## Removing a device from your capture computer

- **From the app:** turn off **Use over the internet** for the MiSTer, or
  delete the MiSTer. The app tells the capture computer to remove this
  device (its name, key and connection times). If the capture computer
  can't be reached then, it keeps the entry until you remove it there.
- **Other devices** (an old or lost phone): on the capture computer, open
  its setup page, http://localhost:5971, and remove them under Devices.

Deleting the app removes everything it kept on the device (below). The
capture computer's log file is yours: delete it there whenever you like.

All of this stays between your device and computers you run. No one else
receives it: not the developer, and no third-party service is involved.

## This device's name

- **iPhone:** iOS doesn't tell apps your iPhone's name, so the app calls it
  "iPhone" until you name it (Settings, or when you first use a MiSTer over
  the internet).
- **Android:** the app starts from the device name in Android's settings
  (or the model name); you can change it in the app's Settings.
- **Mac:** the app starts from the Mac's computer name; you can change it
  in the app's Settings.

The name goes only to your capture computers, as above.

## Recordings and screenshots

Both are of the MiSTer's picture (and, for recordings, its sound), never
of your device's own screen, and both happen only when you press the
button.

- **Mac App Store app:** Record saves a video with sound, as an MP4 file in
  Movies/MiSTerKVMS. Screenshots are copied to the clipboard or saved as PNG
  files in Pictures.
- **iPhone and Android:** there's no recording. The screenshot button copies
  the picture to the clipboard; on Android the app keeps that picture in
  its cache, so that other apps can paste it.

Screenshots can have a bar under the picture with the MiSTer core that was
running and the time (on by default; Settings turns it off).

The files are yours: they stay on your device unless you share them.

## What's kept on your device

- The MiSTers you add (names, addresses, settings) and your app settings.
- The keys for using your MiSTers over the internet (iPhone and Android).
  On iPhone they're in the Keychain, for this device only: never synced to
  iCloud Keychain or restored onto another iPhone. On Android they're
  encrypted with a key held in the Android Keystore, and the app's data
  isn't backed up or moved to another device.
- Recordings and screenshots you saved on the Mac.

Deleting the app deletes all of these, except recordings and screenshots
you saved on the Mac.

## Permissions

- **iPhone:** Local Network, which iOS asks for the first time the app looks
  for or connects to capture servers. Nothing else.
- **Android:** network access only (internet, network state, and Wi-Fi
  multicast, which Android needs before version 14 to find capture servers
  on your network). No location, contacts, camera, microphone or storage
  permissions.
- **Mac App Store app:** network access; the camera and microphone, which
  macOS asks for only if you use a capture card plugged into the Mac (macOS
  treats the card as a camera and microphone; its picture and sound are
  shown and played on the Mac and go nowhere else); and the Movies and
  Pictures folders, for recordings and screenshots.

## Clipboard

The apps use the clipboard only when you ask them to: the screenshot button
copies the picture, and Paste in Type Text reads the text you copied (on
iPhone through Apple's paste button, so iOS doesn't ask).

## Links

Links in the app (source code, credits) open in your browser. The sites
they lead to, such as GitHub, have their own privacy policies.

## Children

The apps aren't directed at children, and nothing they send reaches the
developer, from anyone.

## Changes

Changes to this policy are published here, in the project's repository,
with a new effective date. The history of every change is public.

## Contact

Questions about this policy: open an issue at
https://github.com/danifunker/MiSTerKVMS/issues.
