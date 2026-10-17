# MiSTerKVMS

KVM for MiSTer FPGA: keyboard/mouse to the MiSTer through mrext Remote, video and audio from our capture server (`mkvm://`) or a local capture card. One Rust core, native shells per platform. See README.md.

## Map

- `core/`: shared Rust core (`kvms-core`): Controller, mrext client, keymaps, video sources, renderer, audio, recording. UniFFI for Swift, C API (`capi.rs`) for Windows.
- `cli/`: `misterkvms-cli` (package `kvms-cli`): screenshots, recording, input, cores from the command line; JSON out. `mcp` serves it to agents (`cli/src/mcp.rs`, docs/MCP.md).
- `mkvm/`: the mkvm protocol between the capture server and viewers.
- `capture/`: capture cards, video + audio (`kvms-capture`).
- `codec/`: picture encode/decode/convert/scale (`kvms-codec`).
- `server/`: `misterkvms-capture`, the capture server.
- `apple/`: macOS app (SwiftUI + AppKit). `ios/`: iPhone app.
- `windows/`: WinUI 3 app (C#). `linux/`: GTK 4 + libadwaita app (Rust).
- `docs/`: plans and handoffs (CAPTURE-SERVER, PORTING, IPHONE, MCP, BACKLOG).
- `scripts/`: builds and packaging.

## Build and test

- `cargo build -p kvms-cli`, `cargo test -p kvms-cli`
- `cargo test -p kvms-core`; `cargo test -p misterkvms-capture -p kvms-capture`
- Apps: `scripts/build-mac.sh [--run]`, `scripts/build-linux.sh [--run]`, `scripts\build-windows.ps1 [-Run]`, `scripts/build-ios.sh`.
- Hardware settings: `local.settings.json` (git-ignored; copy `local.settings.example.json`). `--local test:pattern` works without a card.

## Rules

- When a `misterkvms-cli` command or its arguments change, update the MCP tools (`TOOLS` in `cli/src/mcp.rs`) and docs/MCP.md in the same change, and run `cargo test -p kvms-cli` (the sync test fails otherwise).
- The CLI prints one JSON object on stdout; errors are JSON on stderr, exit code 1. In `mcp` mode stdout is JSON-RPC only.
- Commit messages: a short summary line in plain words, then what changed and why.
