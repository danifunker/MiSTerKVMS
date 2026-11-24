# JetKVM

A JetKVM can be a MiSTer's video and keyboard/mouse: its HDMI input takes the MiSTer's picture, and its USB cable is a real USB keyboard and mouse to the MiSTer.

A JetKVM is a capture device with the id `jetkvm://<address>` (`capture/src/jetkvm.rs`), so two ways to use one share the same code:

- **Through the capture server:** `misterkvms-capture add <name> --video jetkvm://<address> --audio none --mister <mister>`. The server connects to the JetKVM and serves it as a normal stream (1080p30, tested with a JetKVM Mini). Every app works unchanged and several people can watch. The setup page doesn't offer JetKVMs yet; add them with the command.
- **Direct:** a local source of `jetkvm://<address>`, e.g. `misterkvms-cli --local jetkvm://<address> screenshot x.png`. One viewer at a time. The apps' host editor only lists capture cards for now, so this works from the CLI (and `local` in the settings file) but can't be chosen in the apps yet.

Status: video both ways; keyboard and mouse through the JetKVM's USB from `misterkvms-cli jetkvm`. The apps still send input through mrext; JetKVM input in sessions, audio, and choosing a JetKVM in the apps and the setup page are next.

## Setting up

1. Connect the MiSTer's HDMI out to the JetKVM's HDMI in, and the JetKVM's USB cable to one of the MiSTer's USB ports.
2. Open the JetKVM's address (shown on its screen) in a browser and finish its setup wizard. Until then it refuses every connection. "No password" is simplest on a home network; with a password, set `JETKVM_PASSWORD` for the CLI.
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

**"JetKVM isn't set up yet" / "JetKVM needs a password".** Finish the setup wizard; set `JETKVM_PASSWORD` if it has a password.

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
