# MCP server: `misterkvms-cli mcp`

`misterkvms-cli mcp` serves the CLI's commands as tools for AI agents (Claude Code, Claude Desktop, any [MCP](https://modelcontextprotocol.io) client): look at the screen, type, press keys, move the mouse, wait for the picture, record, load a core. It speaks MCP over stdin/stdout (JSON-RPC 2.0, one message per line; protocol 2025-06-18, or the client's own if older). Logs go to stderr, nothing but JSON-RPC to stdout.

A tool call runs the same code as the command: the call becomes the command line it stands for, parsed and run like the CLI's, and the JSON the command prints is the tool's result. A screenshot also comes back as a PNG image, so the agent can see the screen. A failing command is a result with `isError: true` and `{"error": "..."}`.

## Run it

From the repo, Claude Code picks it up from `.mcp.json` (it asks before using it the first time):

```json
{ "mcpServers": { "misterkvms": { "command": "cargo", "args": ["run", "-q", "-p", "kvms-cli", "--bin", "misterkvms-cli", "--", "mcp"] } } }
```

That builds the CLI on first start, so the first start is slow. Elsewhere, point a client at an installed `misterkvms-cli` (on macOS `/Applications/MiSTerKVMS.app/Contents/MacOS/misterkvms-cli`):

```bash
claude mcp add misterkvms -- misterkvms-cli mcp
claude mcp add misterkvms -- misterkvms-cli --settings ~/mister.json --host ss1 mcp
```

Claude Desktop and other clients take the same command and arguments in their own config (`"command": "misterkvms-cli", "args": ["mcp"]`).

Global options given before `mcp` (`--settings`, `--host`, `--mister`, `--stream`, `--local`, `--local-audio`) are the defaults for every call; a call's own `host`, `mister`, `stream`, `local`, `local_audio`, `settings` override them. A `stream` or `local` in a call replaces both of the server's, as on the command line.

To try it by hand:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"screenshot","arguments":{"local":"test:pattern"}}}' \
  | misterkvms-cli mcp
```

## Settings

The same as the CLI's: `local.settings.json`, searched from the server's folder upwards (or `--settings`, or `KVMS_SETTINGS`). See `local.settings.example.json` and the CLI section of the README. Without one, every call needs `mister` and `stream` or `local`.

- `mister_host` (or `mister`): the MiSTer, running [mrext Remote](https://github.com/wizzomafizzo/mrext) (input, status, native screenshots).
- `stream` (a capture server stream, `mkvm://host/stream`) or `local` (a capture card on this computer, or `test:pattern`): where the picture comes from. With neither, the stream that shows `mister_host` is found on the network.
- `machines` and `default`: several MiSTers by name, picked with `host`.
- `ssh_user`, `core_dest` (load_core), `type_delay_ms`, `type_hold_ms` (type).

`record` needs ffmpeg; `load_core` needs SSH to the MiSTer with a key (no password prompt). Relative paths are relative to the server's folder (the repo root under Claude Code); results give them made absolute.

## Tools

Every tool also takes `host`, `mister`, `stream`, `local`, `local_audio` and `settings` (the global options above). Durations are strings: `10s`, `500ms`, `2m`, or plain seconds.

| Tool | Arguments | Returns | CLI |
|---|---|---|---|
| `status` | none | JSON: machine, `mister` {host, connected, core, game}, `video` {stream or local, found, streams or cards}, settings file | `status` |
| `machines` | none | JSON: default, machines [{name, mister_host, stream, local}], settings file | `machines` |
| `screenshot` | `out` (path, optional: also keep the PNG there), `native` (bool: MiSTer's own screenshot, the core's resolution), `full` (bool: keep the black borders) | PNG image, and JSON: source, width, height (capture) or core, mister_path (native), path if `out` | `screenshot <out> [--native] [--full]` |
| `record` | `duration` (required), `out` (required; .mp4, .mov, .mkv or .wav), `full` (bool) | JSON: path, frames, fps, width, height, audio_seconds (.mp4: path and video stats) | `record <duration> <out> [--full]` |
| `frames` | `duration` (required), `dir` (required), `all` (bool: repeated frames too), `full` (bool) | JSON: dir, frames_seen, saved, width, height | `frames <duration> <dir> [--all] [--full]` |
| `type` | `text` (required; `\n` Enter, `\t` Tab, `\e` Esc, `\b` Backspace), `delay` (ms between keys), `hold` (ms each key is held) | JSON: typed, skipped | `type <text> [--delay] [--hold]` |
| `key` | `combo` (required: `F12`, `enter`, `ctrl+alt+del`, `shift+a`, `menu`), `hold` (ms, default 80) | JSON: pressed | `key <combo> [--hold]` |
| `mouse` | `action` (required: `move`, `click`, `down`, `up`, `pos`); `dx`, `dy` (move, pixels; any size, sent as several packets of at most 127); `button` (click/down/up: `left`, `right`, `middle`; default left); `x`, `y` (pos, 0-1000 per mille) | JSON: ok | `mouse move <dx> <dy>`, `mouse click\|down\|up [button]`, `mouse pos <x> <y>` |
| `wait_for_change` | `timeout` (default 10s), `until_still` (wait for the picture to settle this long instead), `min_pixels` (default 50), `full` (bool) | JSON: changed or still, timed_out, after_ms | `wait-for-change [--timeout] [--until-still] [--min-pixels] [--full]` |
| `stream` | `address` (required, `mkvm://host/stream`), `duration` (default 5s), `max` (`WIDTHxHEIGHT`), `fps`, `quality` (1-100), `save` (path for the last picture as PNG) | JSON: what arrived: fps, size, bandwidth, latency | `stream <address> [--duration] [--max] [--fps] [--quality] [--save]` |
| `streams` | `wait` (default 2s) | JSON: streams [{name, address, mister, ...}] | `streams [--wait]` |
| `load_core` | `rbf` (required, a .rbf on this computer, or a .mgl whose core and files are already on the MiSTer), `dest` (folder on the MiSTer, default `/media/fat/_Development` or `core_dest`) | JSON: loaded (the path on the MiSTer) | `load-core <rbf> [--dest]` |

Not tools: `soak` (an hours-long memory test that prints a line every few seconds) and `mcp` itself.

Tools run on their own threads, so a `wait_for_change` can be waiting while a `key` press goes out.

## Keeping it in step

The tools are one table, `TOOLS` in `cli/src/mcp.rs`; argument help and defaults come from the clap definitions in `cli/src/main.rs`. When a command or its arguments change, change `TOOLS` and the table above in the same change. `cargo test -p kvms-cli` fails when a command has no tool (and isn't in `NOT_TOOLS`), when a command's argument has no tool argument, or when a tool argument isn't the command's.
