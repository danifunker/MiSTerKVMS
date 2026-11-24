# JetKVM

A JetKVM can be a MiSTer's video and keyboard/mouse: its HDMI input takes the MiSTer's picture, and its USB cable is a real USB keyboard and mouse to the MiSTer.

A JetKVM is a capture device with the id `jetkvm://<address>` (`capture/src/jetkvm.rs`), and its keyboard and mouse are a session input (`MisterSession::with_jetkvm`), so both ways to use one share the same code. A JetKVM allows one session, so its video and its keyboard and mouse share one connection (`kvms_jetkvm::shared`).

- **Through the capture server:** in the setup page's card list (JetKVMs on the PC's networks are found in about a second; "Add JetKVM" takes one by address), or `misterkvms-capture add <name> --video jetkvm://<address> --audio none --mister <mister>`. The server serves it as a normal stream (1080p30 from a Mini, about 30 fps to viewers). Every app works unchanged, including the phones, and several people can watch.
- **Direct, from the Mac and Windows apps:** "A capture card on this Mac/PC, or a JetKVM", then the JetKVM in the card list (or "JetKVM not listed?" with its address). One viewer at a time. The iPhone, Android and Linux apps watch through a capture server only.
- **CLI:** `--local jetkvm://<address>` (or `local` in `local.settings.json`); `misterkvms-cli jetkvms` lists the JetKVMs on the network.

**Keyboard and mouse** go through the JetKVM's USB, a real USB keyboard and mouse to the MiSTer, unless the host or stream says "mrext". mrext is then optional: with a MiSTer address it still supplies the running core's name. Held keys stay down (the client sends the keepalive the JetKVM needs every 50 ms; without it a key is let go after 100 ms).

**Passwords** (a JetKVM set up with one). The core never stores secrets; it gets the password in memory (`set_jetkvm_password`) before it opens the JetKVM:

- Mac app: the login Keychain (service "MiSTerKVMS JetKVM", one item per address).
- Windows app: Credential Manager ("MiSTerKVMS JetKVM/<address>").
- Capture server: in `capture.json` (owner-only on macOS and Linux), set in the setup page, which never gets it back, only whether one is set. `misterkvms-capture add` takes it from `JETKVM_PASSWORD`.
- CLI: `jetkvm_password` in `local.settings.json` (git-ignored), or `JETKVM_PASSWORD`.
- Phones never see it: they go through a capture server.

The JetKVM's own login is plain HTTP on the LAN; that's the device (its web page does the same).

**Sound:** the JetKVM Mini has none from HDMI. Its "Enable Audio" setting is a USB sound card for the computer it's plugged into, and a MiSTer doesn't play sound to USB. Take the MiSTer's analog out to a line-in on the capture PC or this computer and choose that as the host's or stream's audio. (The full-size JetKVM streams HDMI audio since firmware 0.5.9; that isn't wired up here, untested.)

## Setting up

1. Connect the MiSTer's HDMI out to the JetKVM's HDMI in, and the JetKVM's USB cable to one of the MiSTer's USB ports.
2. Open the JetKVM's address (shown on its screen) in a browser and finish its setup wizard. Until then it refuses every connection. "No password" is simplest on a home network.
3. Set the MiSTer's video mode (below), then restart the MiSTer.
4. Check: `misterkvms-cli jetkvm <address> --save jetkvm.png --key down` saves a picture and moves the MiSTer menu down one row.

## Troubleshooting

**"No HDMI signal detected" on the JetKVM's page, or "connected but gets no picture" from MiSTerKVMS.** Almost always the MiSTer's video mode. A PC picks a mode the JetKVM supports; MiSTer sends the `video_mode` in `MiSTer.ini` (on the SD card) regardless, so the JetKVM's EDID setting doesn't help.

- **JetKVM Mini:** it only takes **1080p at 30 Hz**, and MiSTer has no numbered mode for that. Use the custom mode (standard 1080p30 timing):

  ```ini
  video_mode=1920,88,44,148,1080,4,5,36,74250,+hsync,+vsync
  vsync_adjust=0
  ```

  Put it under `[MiSTer]` **and** in every core section that sets its own `video_mode` (or `video_mode_ntsc` / `video_mode_pal`), or those cores still send 60 Hz and show nothing. `vsync_adjust=1` or `2` changes the refresh rate to match the core, which the Mini can't follow, so turn it off in those sections too. Motion is less smooth at 30 Hz (every other frame of a 60 Hz core).
- **Other JetKVMs:** try `video_mode=0` (720p60) to see whether the mode is the problem.
- Still nothing: try the HDMI cable that works with another device (the DE10-Nano's HDMI is weak with long cables), and restart the MiSTer while the JetKVM is already on.

**Another viewer took over / the picture stops.** A JetKVM has one session: opening its web page, or another app connecting directly, disconnects the current one. Use the capture server when more than one person watches.

**Keys repeat or stick.** The client sends one press and one release per key (hidrpc type `0x05`). On the Mini, a full keyboard report with no keys did not release them.

**"JetKVM isn't set up yet" / "JetKVM needs a password" / "wrong password".** Finish the setup wizard; enter its password in the host editor (apps) or the stream editor (setup page).

## Protocol notes

The JetKVM Mini's firmware (0.5.20, github.com/jetkvm/mini) is built on Espressif's WebRTC stack, not the Go/pion code in github.com/jetkvm/kvm, though it speaks the same signaling. Where they differ, `jetkvm/src/lib.rs` follows the device:

- `GET /device/status` (no login): `{isSetup, sku}`; `sku` names the model (`jetkvm-mini-ethernet`), good for finding JetKVMs on the network.
- Login: `POST /auth/login-local {password}` → `authToken` cookie. Not needed in no-password mode.
- Signaling: WebSocket `/webrtc/signaling/client`, which needs an `Origin` header of the device's own address (403 otherwise). Offer `{"type":"offer","data":{"sd": base64(JSON {type, sdp})}}`; answer `{"type":"answer","data": base64(JSON)}`; trickled `new-ice-candidate`.
- The Mini answers with mids `0`, `1`, … whatever the offer said (and leaves out the data channel section unless the offer's mids are numbered that way), so the offer is sent with numbered mids and the answer mapped back. Its answer puts `c=` after the `a=` lines; the client moves it up before parsing.
- Video: H.264 at 1080p30, ~0.5-0.7 Mbit/s on a desktop, decoded with openh264 on its own thread. Unoptimised openh264 (a plain debug build) can't keep up: the socket backs up and you see ~5 fps of ever-older pictures, which looks like frozen video. The workspace builds openh264 and kvms-jetkvm (its BGRA conversion) optimised even in dev for this reason.
- The Mini's firmware is public: github.com/jetkvm/mini (esp_peer under `third_party/esp_peer`). `GET /webrtc/stats` (no login in no-password mode) shows what the device thinks it sends: fps, bitrate target, REMB received, RTP packets sent.
- Input: data channel `hidrpc`, binary, first byte the type: `0x01` handshake (send version 1, it answers), `0x05` key press/release (usage, 0/1), `0x03` absolute pointer (x, y big-endian i32 0..32767, buttons), `0x06` relative mouse (dx, dy, buttons), `0x04` wheel.
- No mDNS service to browse: the device only answers lookups of its own hostname.
- `JETKVM_DEBUG=1` makes the client print the offer, the answer and every message the device sends on the `rpc` channel. `Connection::rpc("getVideoLogStatus")` asks the device for its video diagnostics.
