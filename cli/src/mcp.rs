//! `misterkvms-cli mcp`: the commands as tools for AI agents, an MCP server
//! on stdin/stdout (JSON-RPC 2.0, one message per line). See docs/MCP.md.
//!
//! Every tool is one entry in `TOOLS`. A call is turned into the command
//! line it stands for, parsed by the same clap definitions and run by the
//! same `exec` as the CLI, so the tools can't drift far from the commands;
//! the test at the bottom fails when a command or argument has no tool.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{CommandFactory, Parser};
use serde_json::{json, Map, Value};

use crate::{Cli, Global};

const PROTOCOL: &str = "2025-06-18";
const KNOWN_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A positional argument.
    Pos,
    /// An option with a value (`--name value`).
    Opt,
    /// A switch (`--name`), a boolean.
    Switch,
    /// Picks the command's subcommand (`mouse move`).
    Sub,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ty {
    Str,
    Int,
    Num,
    Bool,
    /// "10s", "500ms", "2m", or seconds.
    Duration,
    /// A file or folder; made absolute against the server's folder.
    Path,
    Enum(&'static [&'static str]),
}

struct Param {
    /// The clap argument id (the field name), also the tool argument's name.
    name: &'static str,
    kind: Kind,
    ty: Ty,
    required: bool,
    /// Empty: the clap help text.
    help: &'static str,
}

struct Tool {
    name: &'static str,
    /// The clap subcommand it runs.
    command: &'static str,
    description: &'static str,
    read_only: bool,
    params: &'static [Param],
}

const fn p(name: &'static str, kind: Kind, ty: Ty, required: bool, help: &'static str) -> Param {
    Param { name, kind, ty, required, help }
}

use Kind::{Opt, Pos, Sub, Switch};
use Ty::{Bool, Duration as Dur, Enum, Int, Num, Path, Str};

/// The global options, on every tool.
const GLOBAL: &[Param] = &[
    p("host", Opt, Str, false, "Named machine from the settings file's \"machines\" (see the machines tool; default: its \"default\")."),
    p("mister", Opt, Str, false, ""),
    p("stream", Opt, Str, false, ""),
    p("local", Opt, Str, false, ""),
    p("local_audio", Opt, Str, false, ""),
    p("settings", Opt, Path, false, ""),
];

/// Every tool. Keep in step with `Cmd` in main.rs and with docs/MCP.md.
const TOOLS: &[Tool] = &[
    Tool {
        name: "status",
        command: "status",
        description: "What's running on the MiSTer (core, game), whether mrext Remote answers, and whether the video source \
                      (capture server stream or local card) is found. Returns JSON. Start here.",
        read_only: true,
        params: &[],
    },
    Tool {
        name: "machines",
        command: "machines",
        description: "List the named machines in the settings file (name, mister_host, stream, local) and the default one. \
                      Pass a name as `host` to any tool.",
        read_only: true,
        params: &[],
    },
    Tool {
        name: "screenshot",
        command: "screenshot",
        description: "The current picture, returned as a PNG image plus JSON (source, size, path). From the capture with \
                      MiSTer's black borders cropped, or with native=true MiSTer's own screenshot at the core's resolution.",
        read_only: false,
        params: &[
            p("out", Pos, Path, false, "Also keep the PNG here (default: only returned)."),
            p("native", Switch, Bool, false, ""),
            p("full", Switch, Bool, false, ""),
        ],
    },
    Tool {
        name: "record",
        command: "record",
        description: "Record for a duration to a file: .mp4 (video + audio, the apps' recorder), .mov/.mkv (through ffmpeg), \
                      .wav (audio only). Needs ffmpeg. Returns JSON (path, frames, fps, size).",
        read_only: false,
        params: &[
            p("duration", Pos, Dur, true, "How long, e.g. 10s, 500ms, 2m."),
            p("out", Pos, Path, true, "Output file (.mp4, .mov, .mkv or .wav)."),
            p("full", Switch, Bool, false, ""),
        ],
    },
    Tool {
        name: "frames",
        command: "frames",
        description: "Save each changed frame as PNG into a folder for a duration (to check animation and timing). \
                      Returns JSON (frames seen, saved, size).",
        read_only: false,
        params: &[
            p("duration", Pos, Dur, true, "How long, e.g. 5s."),
            p("dir", Pos, Path, true, "Folder for the PNGs (created if needed)."),
            p("all", Switch, Bool, false, ""),
            p("full", Switch, Bool, false, "Keep MiSTer's black borders."),
        ],
    },
    Tool {
        name: "type",
        command: "type",
        description: "Type text on the MiSTer's keyboard (US layout). Escapes: \\n Enter, \\t Tab, \\e Esc, \\b Backspace. \
                      Returns JSON (typed, skipped characters).",
        read_only: false,
        params: &[
            p("text", Pos, Str, true, "The text to type."),
            p("delay", Opt, Int, false, ""),
            p("hold", Opt, Int, false, ""),
        ],
    },
    Tool {
        name: "key",
        command: "key",
        description: "Press a key or combo: F12, enter, esc, up, ctrl+alt+del, shift+a, menu (= F12, the MiSTer menu).",
        read_only: false,
        params: &[
            p("combo", Pos, Str, true, "Key or combo, e.g. menu, enter, ctrl+alt+del."),
            p("hold", Opt, Int, false, "ms the keys are held (default 80)."),
        ],
    },
    Tool {
        name: "mouse",
        command: "mouse",
        description: "Mouse: move (relative dx, dy in pixels), click / down / up (button), pos (absolute x, y in \
                      per-mille of the screen, 0-1000; core-dependent).",
        read_only: false,
        params: &[
            p("action", Sub, Enum(&["move", "click", "down", "up", "pos"]), true, "What to do."),
            p("dx", Pos, Num, false, "move: pixels right (negative: left)."),
            p("dy", Pos, Num, false, "move: pixels down (negative: up)."),
            p("button", Pos, Enum(&["left", "right", "middle"]), false, "click/down/up: which button (default left)."),
            p("x", Pos, Int, false, "pos: 0-1000 across."),
            p("y", Pos, Int, false, "pos: 0-1000 down."),
        ],
    },
    Tool {
        name: "wait_for_change",
        command: "wait-for-change",
        description: "Block until the screen changes, or with until_still until it has stopped changing for that long. \
                      Returns JSON (changed / still / timed_out, after_ms). Good after a key press or loading a core.",
        read_only: true,
        params: &[
            p("timeout", Opt, Dur, false, "Give up after this long (default 10s)."),
            p("until_still", Opt, Dur, false, ""),
            p("min_pixels", Opt, Int, false, ""),
            p("full", Switch, Bool, false, "Watch MiSTer's black borders too."),
        ],
    },
    Tool {
        name: "stream",
        command: "stream",
        description: "Watch a capture server stream like a viewer for a while and report what arrived (fps, size, \
                      bandwidth, latency) as JSON. For testing capture servers and networks.",
        read_only: false,
        params: &[
            p("address", Pos, Str, true, "Stream address, mkvm://host/stream (see the streams tool)."),
            p("duration", Opt, Dur, false, "How long to watch (default 5s)."),
            p("max", Opt, Str, false, ""),
            p("fps", Opt, Num, false, ""),
            p("quality", Opt, Int, false, ""),
            p("save", Opt, Path, false, ""),
        ],
    },
    Tool {
        name: "streams",
        command: "streams",
        description: "List the capture server streams found on the network (name, address, which MiSTer each shows).",
        read_only: true,
        params: &[p("wait", Opt, Dur, false, "How long to listen (default 2s).")],
    },
    Tool {
        name: "load_core",
        command: "load-core",
        description: "Copy a core (.rbf) to the MiSTer over SSH (key login, no password) and load it. Returns JSON (loaded: \
                      the path on the MiSTer).",
        read_only: false,
        params: &[p("rbf", Pos, Path, true, "The .rbf file on this computer."), p("dest", Opt, Str, false, "")],
    },
];

// MARK: - the server

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// Serve until stdin closes. `defaults` are the global options given before
/// `mcp`; a tool call's own override them.
pub fn serve(defaults: Global) -> Result<(), String> {
    let (input, output) = take_stdio()?;
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let defaults = Arc::new(defaults);
    let mut calls = Vec::new();
    eprintln!("misterkvms-cli mcp: ready ({} tools)", TOOLS.len());
    for line in BufReader::new(input).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                send(&out, &error(Value::Null, -32700, &format!("parse error: {e}")));
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            continue; // a notification (initialized, cancelled): nothing to answer
        };
        if method.is_empty() {
            if msg.get("result").is_none() && msg.get("error").is_none() {
                send(&out, &error(id, -32600, "invalid request"));
            }
            continue; // a response to something we never asked
        }
        match method.as_str() {
            // Tools can take a while (record, wait_for_change): each runs on its
            // own thread, so pings and other calls are answered meanwhile.
            "tools/call" => {
                let (out, defaults) = (out.clone(), defaults.clone());
                calls.push(std::thread::spawn(move || {
                    let reply = match call(&params, &defaults) {
                        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                        Err(e) => error(id, -32602, &e),
                    };
                    send(&out, &reply);
                }));
                calls.retain(|c| !c.is_finished());
            }
            _ => send(&out, &handle(id, &method, &params)),
        }
    }
    // stdin closed: finish what was asked before leaving.
    for c in calls {
        let _ = c.join();
    }
    Ok(())
}

fn handle(id: Value, method: &str, params: &Value) -> Value {
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL);
            let version = if KNOWN_PROTOCOLS.contains(&asked) { asked } else { PROTOCOL };
            let here = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "misterkvms", "title": "MiSTerKVMS", "version": env!("CARGO_PKG_VERSION") },
                "instructions": format!(
                    "Drives a MiSTer FPGA: screenshots (returned as images), typing, keys, mouse, waiting for the screen, \
                     recording, loading cores. Which MiSTer and where its video comes from is in local.settings.json; \
                     call machines or status first, and pass host to pick a named machine. Relative paths are relative \
                     to {here}."
                ),
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_list() }),
        _ => return error(id, -32601, &format!("method not found: {method}")),
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn send(out: &Out, msg: &Value) {
    let mut out = out.lock().unwrap();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

/// tools/call. Err: the call itself is wrong (unknown tool); a command that
/// fails, or arguments it doesn't take, are a result with isError.
fn call(params: &Value, defaults: &Global) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("tools/call needs a name")?;
    let tool = TOOLS.iter().find(|t| t.name == name).ok_or_else(|| format!("unknown tool: {name}"))?;
    let empty = Map::new();
    let args = match params.get("arguments") {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(m)) => m,
        Some(_) => return Ok(failed("arguments must be an object".into())),
    };
    Ok(match run_tool(tool, args, defaults) {
        Ok(content) => json!({ "content": content }),
        Err(e) => failed(e),
    })
}

fn failed(e: String) -> Value {
    json!({ "content": [{ "type": "text", "text": json!({ "error": e }).to_string() }], "isError": true })
}

fn run_tool(tool: &Tool, args: &Map<String, Value>, defaults: &Global) -> Result<Vec<Value>, String> {
    let mut args = args.clone();
    // screenshot: the picture is the answer; a file only if asked for.
    let temp = (tool.name == "screenshot" && args.get("out").is_none_or(Value::is_null)).then(|| {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        std::env::temp_dir().join(format!("misterkvms-mcp-{}-{nanos}.png", std::process::id()))
    });
    if let Some(t) = &temp {
        args.insert("out".into(), Value::String(t.display().to_string()));
    }
    let cli = Cli::try_parse_from(argv(tool, &args)?).map_err(|e| e.render().to_string().trim().to_string())?;
    let global = cli.global.or(defaults);
    let result = crate::settings(global).and_then(|s| crate::exec(cli.cmd, &s));
    let mut content = Vec::new();
    match (tool.name, result) {
        ("screenshot", Ok(mut v)) => {
            let path = PathBuf::from(v["path"].as_str().unwrap_or_default());
            let png = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()));
            if temp.is_some() {
                let _ = std::fs::remove_file(&path);
                if let Some(o) = v.as_object_mut() {
                    o.remove("path");
                }
            }
            content.push(json!({ "type": "text", "text": v.to_string() }));
            content.push(json!({ "type": "image", "mimeType": "image/png", "data": base64(&png?) }));
        }
        (_, Ok(v)) => content.push(json!({ "type": "text", "text": v.to_string() })),
        (_, Err(e)) => {
            if let Some(t) = &temp {
                let _ = std::fs::remove_file(t);
            }
            return Err(e);
        }
    }
    Ok(content)
}

impl Global {
    /// These options, the server's for any not given. A video source given
    /// here (stream or local) replaces the server's, as on the command line.
    fn or(self, base: &Global) -> Global {
        let source_given = self.stream.is_some() || self.local.is_some();
        Global {
            settings: self.settings.or_else(|| base.settings.clone()),
            host: self.host.or_else(|| base.host.clone()),
            mister: self.mister.or_else(|| base.mister.clone()),
            stream: if source_given { self.stream } else { base.stream.clone() },
            local: if source_given { self.local } else { base.local.clone() },
            local_audio: self.local_audio.or_else(|| base.local_audio.clone()),
        }
    }
}

/// The command line a tool call stands for.
fn argv(tool: &Tool, args: &Map<String, Value>) -> Result<Vec<String>, String> {
    let root = Cli::command();
    let cmd = root.find_subcommand(tool.command).ok_or_else(|| format!("no command {}", tool.command))?;
    let known: Vec<&str> = GLOBAL.iter().chain(tool.params).map(|p| p.name).collect();
    if let Some(bad) = args.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(format!("{} doesn't take '{bad}' (it takes: {})", tool.name, known.join(", ")));
    }
    let mut line = vec!["misterkvms-cli".to_string()];
    let mut sub = Vec::new();
    let mut opts = Vec::new();
    let mut positional = Vec::new();
    let mut sub_cmd = cmd;
    for p in tool.params.iter().filter(|p| p.kind == Sub) {
        let v = string(p, args.get(p.name))?.ok_or_else(|| format!("{} needs '{}'", tool.name, p.name))?;
        sub_cmd = cmd.find_subcommand(&v).ok_or_else(|| format!("unknown {} '{v}'", p.name))?;
        sub.push(v);
    }
    for (p, scope) in GLOBAL.iter().map(|p| (p, &root)).chain(tool.params.iter().map(|p| (p, sub_cmd))) {
        let Some(v) = args.get(p.name).filter(|v| !v.is_null()) else {
            if p.required {
                return Err(format!("{} needs '{}'", tool.name, p.name));
            }
            continue;
        };
        let long = || {
            let arg = scope.get_arguments().find(|a| a.get_id() == p.name).or_else(|| cmd.get_arguments().find(|a| a.get_id() == p.name));
            arg.and_then(|a| a.get_long()).map(|l| format!("--{l}")).ok_or_else(|| format!("no --{} for {}", p.name, tool.command))
        };
        match p.kind {
            Sub => {}
            Switch => match v.as_bool() {
                Some(true) => opts.push(long()?),
                Some(false) => {}
                None => return Err(format!("'{}' is true or false", p.name)),
            },
            Opt => {
                // --name=value: a value starting with - stays a value.
                opts.push(format!("{}={}", long()?, string(p, Some(v))?.unwrap_or_default()));
            }
            Pos => positional.push(string(p, Some(v))?.unwrap_or_default()),
        }
    }
    line.push(tool.command.to_string());
    line.extend(sub);
    line.extend(opts);
    if !positional.is_empty() {
        // After --, text starting with - (or a negative move) isn't an option.
        line.push("--".into());
        line.extend(positional);
    }
    Ok(line)
}

/// A tool argument as it goes on the command line.
fn string(p: &Param, v: Option<&Value>) -> Result<Option<String>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(None) };
    let s = match (p.ty, v) {
        (Int | Num | Dur, Value::Number(n)) => n.to_string(),
        (Int | Num, Value::String(s)) => s.clone(),
        (Int, _) | (Num, _) => return Err(format!("'{}' is a number", p.name)),
        (Path, Value::String(s)) => {
            let path = PathBuf::from(s);
            match std::env::current_dir() {
                Ok(here) if path.is_relative() => here.join(path).display().to_string(),
                _ => s.clone(),
            }
        }
        (Enum(values), Value::String(s)) if !values.contains(&s.as_str()) => {
            return Err(format!("'{}' is one of {}", p.name, values.join(", ")))
        }
        (_, Value::String(s)) => s.clone(),
        _ => return Err(format!("'{}' is a string", p.name)),
    };
    Ok(Some(s))
}

fn tool_list() -> Vec<Value> {
    let root = Cli::command();
    TOOLS
        .iter()
        .map(|t| {
            let cmd = root.find_subcommand(t.command);
            let mut props = Map::new();
            let mut required = Vec::new();
            for p in t.params.iter().chain(GLOBAL) {
                // The help and default from clap, unless the tool says otherwise.
                let arg = cmd
                    .into_iter()
                    .flat_map(|c| std::iter::once(c).chain(c.get_subcommands()))
                    .chain(std::iter::once(&root))
                    .find_map(|c| c.get_arguments().find(|a| a.get_id() == p.name));
                let help = match (p.help, arg.and_then(|a| a.get_help())) {
                    ("", Some(h)) => h.to_string(),
                    (h, _) => h.to_string(),
                };
                let mut schema = match p.ty {
                    Str | Path => json!({ "type": "string" }),
                    Dur => json!({ "type": "string", "pattern": "^[0-9.]+(ms|s|m)?$" }),
                    Int => json!({ "type": "integer" }),
                    Num => json!({ "type": "number" }),
                    Bool => json!({ "type": "boolean" }),
                    Enum(values) => json!({ "type": "string", "enum": values }),
                };
                schema["description"] = Value::String(help);
                if let Some(d) = arg.and_then(|a| a.get_default_values().first()).and_then(|d| d.to_str()) {
                    if p.ty != Bool {
                        schema["default"] = match p.ty {
                            Int | Num => serde_json::from_str(d).unwrap_or(Value::String(d.into())),
                            _ => Value::String(d.into()),
                        };
                    }
                }
                if p.required {
                    required.push(p.name);
                }
                props.insert(p.name.into(), schema);
            }
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": { "type": "object", "properties": props, "required": required, "additionalProperties": false },
                "annotations": { "readOnlyHint": t.read_only },
            })
        })
        .collect()
}

// MARK: - stdio

/// The real stdin and stdout, for JSON-RPC only. Afterwards the process's
/// own stdout goes to stderr and its stdin is empty, so nothing else (a
/// println, ffmpeg, ssh) can write into the protocol or read from it.
#[cfg(unix)]
fn take_stdio() -> Result<(std::fs::File, std::fs::File), String> {
    use std::os::fd::FromRawFd;
    use std::os::raw::c_int;
    extern "C" {
        fn dup(fd: c_int) -> c_int;
        fn dup2(fd: c_int, to: c_int) -> c_int;
    }
    let null = std::fs::File::open("/dev/null").map_err(|e| e.to_string())?;
    // SAFETY: plain fd juggling on 0/1/2; the dups are owned by the Files returned.
    unsafe {
        use std::os::fd::AsRawFd;
        let (input, output) = (dup(0), dup(1));
        if input < 0 || output < 0 || dup2(2, 1) < 0 || dup2(null.as_raw_fd(), 0) < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok((std::fs::File::from_raw_fd(input), std::fs::File::from_raw_fd(output)))
    }
}

#[cfg(windows)]
fn take_stdio() -> Result<(std::fs::File, std::fs::File), String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};
    #[link(name = "kernel32")]
    extern "system" {
        fn SetStdHandle(which: u32, handle: RawHandle) -> i32;
    }
    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    let null = std::fs::File::open("NUL").map_err(|e| e.to_string())?;
    let (input, output) = (std::io::stdin().as_raw_handle(), std::io::stdout().as_raw_handle());
    // SAFETY: the original handles are kept by the Files returned; the process's
    // standard handles become NUL and stderr.
    unsafe {
        if SetStdHandle(STD_INPUT_HANDLE, null.into_raw_handle()) == 0
            || SetStdHandle(STD_OUTPUT_HANDLE, std::io::stderr().as_raw_handle()) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok((std::fs::File::from_raw_handle(input), std::fs::File::from_raw_handle(output)))
    }
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cmd;
    use clap::ArgAction;

    /// Commands that aren't tools: `mcp` itself, and `soak` (an hours-long
    /// memory test that prints a line every few seconds).
    const NOT_TOOLS: &[&str] = &["mcp", "soak"];

    fn args_of(c: &clap::Command) -> Vec<&clap::Arg> {
        c.get_arguments().filter(|a| !a.is_global_set() && !matches!(a.get_id().as_str(), "help" | "version")).collect()
    }

    /// A clap command with no tool, or a tool argument clap doesn't know,
    /// fails here: add the tool to TOOLS (and docs/MCP.md), or the command to NOT_TOOLS.
    #[test]
    fn every_command_is_a_tool() {
        let root = Cli::command();
        for c in root.get_subcommands() {
            let name = c.get_name();
            let tool = TOOLS.iter().find(|t| t.command == name);
            assert!(tool.is_some() || NOT_TOOLS.contains(&name), "`{name}` has no MCP tool: add it to TOOLS in cli/src/mcp.rs and to docs/MCP.md");
            assert!(tool.is_none() || !NOT_TOOLS.contains(&name), "`{name}` is a tool and in NOT_TOOLS");
        }
        for t in TOOLS {
            let c = root.find_subcommand(t.command).unwrap_or_else(|| panic!("tool {} runs `{}`, which isn't a command", t.name, t.command));
            // The command and its subcommands (mouse move, ...) together.
            let scopes: Vec<&clap::Command> = std::iter::once(c).chain(c.get_subcommands()).collect();
            for arg in scopes.iter().flat_map(|s| args_of(s)) {
                let id = arg.get_id().as_str();
                assert!(t.params.iter().any(|p| p.name == id), "`{} --{id}` has no argument in tool {}", t.command, t.name);
            }
            for p in t.params {
                if p.kind == Sub {
                    let mut subs: Vec<&str> = c.get_subcommands().map(|s| s.get_name()).collect();
                    let Enum(values) = p.ty else { panic!("{}.{} picks a subcommand: make it an Enum", t.name, p.name) };
                    let mut values = values.to_vec();
                    subs.sort();
                    values.sort();
                    assert_eq!(subs, values, "{}.{}: the values must be `{}`'s subcommands", t.name, p.name, t.command);
                    continue;
                }
                let arg = scopes
                    .iter()
                    .flat_map(|s| args_of(s))
                    .find(|a| a.get_id() == p.name)
                    .unwrap_or_else(|| panic!("tool {} has '{}', which `{}` doesn't take", t.name, p.name, t.command));
                let kind = if arg.is_positional() {
                    Pos
                } else if matches!(arg.get_action(), ArgAction::SetTrue) {
                    Switch
                } else {
                    Opt
                };
                assert_eq!(p.kind, kind, "{}.{}", t.name, p.name);
                assert_eq!(p.ty == Bool, kind == Switch, "{}.{}: switches are Bool", t.name, p.name);
            }
        }
        for arg in root.get_arguments().filter(|a| !matches!(a.get_id().as_str(), "help" | "version")) {
            assert!(GLOBAL.iter().any(|p| p.name == arg.get_id()), "global --{} has no argument in GLOBAL", arg.get_id());
        }
    }

    fn sample(p: &Param) -> Value {
        match p.ty {
            Str => json!("a"),
            Path => json!("x/y.png"),
            Dur => json!("1s"),
            Int => json!(1),
            Num => json!(-1.5),
            Bool => json!(true),
            Enum(v) => json!(v[0]),
        }
    }

    /// Every tool, with every argument it takes, makes a command line clap accepts.
    #[test]
    fn every_call_parses() {
        let root = Cli::command();
        for t in TOOLS {
            let c = root.find_subcommand(t.command).unwrap();
            let actions: Vec<Option<&str>> = match t.params.iter().find(|p| p.kind == Sub) {
                Some(Param { ty: Enum(v), .. }) => v.iter().map(|v| Some(*v)).collect(),
                _ => vec![None],
            };
            for action in actions {
                let scope = action.map_or(c, |a| c.find_subcommand(a).unwrap());
                let mut args = Map::new();
                for p in GLOBAL.iter().filter(|p| p.name != "stream" && p.name != "settings") {
                    args.insert(p.name.into(), sample(p));
                }
                for p in t.params {
                    match (p.kind, action) {
                        (Sub, Some(a)) => {
                            args.insert(p.name.into(), json!(a));
                        }
                        _ if scope.get_arguments().any(|a| a.get_id() == p.name) => {
                            args.insert(p.name.into(), sample(p));
                        }
                        _ => {}
                    }
                }
                let line = argv(t, &args).unwrap();
                if let Err(e) = Cli::try_parse_from(&line) {
                    panic!("{} {action:?}: {line:?}: {e}", t.name);
                }
            }
        }
    }

    #[test]
    fn calls() {
        let mut args = Map::new();
        args.insert("action".into(), json!("move"));
        args.insert("dx".into(), json!(10));
        args.insert("dy".into(), json!(-5));
        args.insert("host".into(), json!("pi"));
        let line = argv(TOOLS.iter().find(|t| t.name == "mouse").unwrap(), &args).unwrap();
        assert_eq!(line, ["misterkvms-cli", "mouse", "move", "--host=pi", "--", "10", "-5"]);
        let cli = Cli::try_parse_from(&line).unwrap();
        assert!(matches!(cli.cmd, Cmd::Mouse { action: crate::MouseCmd::Move { dx, dy } } if dx == 10.0 && dy == -5.0));

        let ty = TOOLS.iter().find(|t| t.name == "type").unwrap();
        let mut args = Map::new();
        args.insert("text".into(), json!("-dir\\n"));
        let cli = Cli::try_parse_from(argv(ty, &args).unwrap()).unwrap();
        assert!(matches!(cli.cmd, Cmd::Type { text, .. } if text == "-dir\\n"));

        args.insert("bogus".into(), json!(1));
        assert!(argv(ty, &args).is_err());
        assert!(argv(ty, &Map::new()).is_err());
    }

    #[test]
    fn base64_matches() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn answers() {
        let v = handle(json!(1), "initialize", &json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(v["result"]["protocolVersion"], "2025-03-26");
        let v = handle(json!(2), "tools/list", &Value::Null);
        assert_eq!(v["result"]["tools"].as_array().unwrap().len(), TOOLS.len());
        assert_eq!(handle(json!(3), "nope", &Value::Null)["error"]["code"], -32601);
        let r = call(&json!({ "name": "type", "arguments": { "nope": 1 } }), &Global::default()).unwrap();
        assert_eq!(r["isError"], true);
        assert!(call(&json!({ "name": "nope" }), &Global::default()).is_err());
    }
}
