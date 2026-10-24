# MiSTerKVMS privacy policy

Effective 24 October 2026.

This policy covers the MiSTerKVMS apps distributed through app stores: the
iPhone app, the Mac App Store app and the Android app. They're free, open
source (https://github.com/danifunker/MiSTerKVMS, MIT License) and made by
one developer, Dani Sarfati.

## In short

The apps collect nothing. They have no accounts, ads, analytics, crash
reporting or tracking, and they send nothing to the developer or to anyone
else. They connect only to the devices you set up yourself: your MiSTerKVMS
Capture computers and, on the Mac, your MiSTers.

## What the apps send, and where

MiSTerKVMS shows a MiSTer FPGA's screen and plays its sound, and sends it
keyboard and mouse input. To do that the apps connect to:

- **MiSTerKVMS Capture**, the program you run on your own computer with a
  capture card. The app receives its picture and sound, and sends it your
  input (keys, mouse movements and clicks, text you choose to type on the
  MiSTer) and this device's name. Other people watching the same MiSTer
  see that name while you have control ("Dani's phone has control").
- **Your MiSTers** (the Mac App Store app only), through mrext Remote on
  your network, for the same input.
- **A capture card plugged into your Mac** (the Mac App Store app only),
  which macOS treats as a camera and microphone, so it asks for that
  permission. The card's picture and sound are shown and played on the
  Mac and go nowhere else.
- **Your local network**, to find capture servers (Bonjour / DNS-SD). This
  only looks on the network you're on.

When you turn on **Use over the internet** for a MiSTer (iPhone and
Android), the app pairs
with your capture computer while you're on the same network. The capture
computer gives this device its own key and keeps a list of paired devices
on that computer: each one's name, when it was paired and last connected,
and the address it last connected from. You can remove a device from that
list on the capture computer's setup page at any time. Connections from
outside your home are encrypted with that key.

All of this stays between your device and computers you run. None of it
goes to the developer.

## What's kept on your device

- The MiSTers you add (names, addresses, settings) and your app settings.
- The keys for using your MiSTers over the internet (iPhone and Android):
  in the Keychain on iPhone, and encrypted with a key held in the Android
  Keystore on Android. They're not included in backups or moved to another
  device.
- On the Mac, recordings and screenshots you save, in the Movies and
  Pictures folders.
- On Android, the latest screenshot you copied, in the app's cache, so that
  other apps can paste it.

Deleting the app deletes all of these, except recordings and screenshots
you saved on the Mac.

## Clipboard

The apps use the clipboard only when you ask them to: the screenshot button
copies the picture, and Paste in Type Text reads the text you copied.

## Links

Links in the app (source code, credits) open in your browser. The sites
they lead to, such as GitHub, have their own privacy policies.

## Children

The apps aren't directed at children and don't knowingly collect anything
from anyone.

## Changes

Changes to this policy are published here, in the project's repository,
with a new effective date. The history of every change is public.

## Contact

Questions about this policy: open an issue at
https://github.com/danifunker/MiSTerKVMS/issues.
