//! `jetkvm`: connect straight to a JetKVM, watch its video for a while,
//! optionally tap a key through its USB keyboard, and report.

use std::path::Path;
use std::time::{Duration, Instant};

use kvms_codec::Bgra;
use kvms_jetkvm::{Connection, State};
use serde_json::{json, Value};

/// What to send once video is up.
pub struct Input {
    pub key: Option<String>,
    /// "DX,DY"
    pub move_by: Option<String>,
    /// "X,Y" in per-mille
    pub pos: Option<String>,
    pub click: Option<String>,
}

fn button_bit(name: &str) -> Result<u8, String> {
    match name {
        "left" => Ok(1),
        "right" => Ok(2),
        "middle" => Ok(4),
        other => Err(format!("--click: unknown button '{other}' (left, right, middle)")),
    }
}

/// Relative movement in steps the report can carry (-127..=127 each).
fn move_relative(conn: &Connection, mut dx: i32, mut dy: i32) {
    while dx != 0 || dy != 0 {
        let sx = dx.clamp(-127, 127);
        let sy = dy.clamp(-127, 127);
        conn.mouse_relative(sx as i8, sy as i8, 0);
        dx -= sx;
        dy -= sy;
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn probe(address: &str, password: Option<&str>, duration: Duration, save: Option<&Path>, input: Input) -> Result<Value, String> {
    let key = input.key.as_deref();
    let combo = key.map(kvms_core::text::parse_combo).transpose()?;
    let move_by = match input.move_by.as_deref() {
        Some(m) => {
            let (x, y) = m.split_once(',').ok_or("--move is DX,DY")?;
            Some((x.trim().parse::<i32>().map_err(|_| "bad --move DX")?, y.trim().parse::<i32>().map_err(|_| "bad --move DY")?))
        }
        None => None,
    };
    let pos = match input.pos.as_deref() {
        Some(m) => {
            let (x, y) = m.split_once(',').ok_or("--pos is X,Y")?;
            let x: u32 = x.trim().parse().map_err(|_| "bad --pos X")?;
            let y: u32 = y.trim().parse().map_err(|_| "bad --pos Y")?;
            let scale = |v: u32| v.min(1000) * kvms_jetkvm::POINTER_MAX / 1000;
            Some((scale(x), scale(y)))
        }
        None => None,
    };
    let click = input.click.as_deref().map(button_bit).transpose()?;
    let status = kvms_jetkvm::status(address)?;
    let started = Instant::now();
    let conn = Connection::connect(address, password)?;

    let first = conn.wait_frame(0, Duration::from_secs(10)).map_err(|e| format!("{e}; stats {:?}", conn.stats()))?;
    let first_ms = started.elapsed().as_millis() as u64;

    let mut pressed = Value::Null;
    if let Some(keys) = &combo {
        conn.wait_hid(Duration::from_secs(3))?;
        let keys: Vec<u8> = keys.iter().filter_map(|&k| u8::try_from(k).ok()).collect();
        for &k in &keys {
            conn.key(k, true);
        }
        std::thread::sleep(Duration::from_millis(40));
        for &k in keys.iter().rev() {
            conn.key(k, false);
        }
        pressed = json!(key);
    }
    if move_by.is_some() || pos.is_some() || click.is_some() {
        conn.wait_hid(Duration::from_secs(3))?;
        if let Some((dx, dy)) = move_by {
            move_relative(&conn, dx, dy);
        }
        if let Some((x, y)) = pos {
            conn.pointer(x, y, 0);
            std::thread::sleep(Duration::from_millis(30));
        }
        if let Some(b) = click {
            // After --pos, click with the absolute pointer (where it is).
            match pos {
                Some((x, y)) => {
                    conn.pointer(x, y, b);
                    std::thread::sleep(Duration::from_millis(60));
                    conn.pointer(x, y, 0);
                }
                None => {
                    conn.mouse_relative(0, 0, b);
                    std::thread::sleep(Duration::from_millis(60));
                    conn.mouse_relative(0, 0, 0);
                }
            }
        }
    }

    let watch_from = Instant::now();
    let decoded_before = conn.stats().decoded;
    let mut last = first;
    while watch_from.elapsed() < duration {
        let left = duration.saturating_sub(watch_from.elapsed());
        match conn.wait_frame(last.number, left.max(Duration::from_millis(1))) {
            Ok(f) => last = f,
            Err(_) if !matches!(conn.state(), State::Closed(_)) => break,
            Err(e) => return Err(e),
        }
    }
    let stats = conn.stats();
    let fps = (stats.decoded - decoded_before) as f64 / watch_from.elapsed().as_secs_f64();

    if let Some(path) = save {
        let pic = Bgra { width: last.width, height: last.height, stride: last.width * 4, data: last.bgra.clone() };
        crate::stream::save_png(path, &pic)?;
    }
    let state = conn.state();
    conn.close();

    Ok(json!({
        "address": address,
        "sku": status.sku,
        "state": format!("{state:?}"),
        "codec": stats.codec,
        "width": last.width,
        "height": last.height,
        "first_picture_ms": first_ms,
        "fps": (fps * 10.0).round() / 10.0,
        "pictures": stats.decoded,
        "video_packets": stats.video_packets,
        "decode_errors": stats.decode_errors,
        "hid_reports": stats.hid_sent,
        "pressed": pressed,
        "moved": input.move_by,
        "pos": input.pos,
        "clicked": input.click,
        "saved": save,
    }))
}
