//! Screenshots: the picture as it is now (cropped like the view, without
//! our predictive cursor) as a PNG, with an optional bar under it saying the
//! core and when it was taken. The shells put it on the clipboard or in a
//! file.

use font8x8::legacy::BASIC_LEGACY;

/// One screenshot, ready to copy or save.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Screenshot {
    pub png: Vec<u8>,
    /// e.g. "SNES 2026-10-17 14.32.05.png" (no folder).
    pub file_name: String,
    pub width: u32,
    pub height: u32,
}

/// A picture in RGB, rows packed.
pub(crate) struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgb {
    /// The `(x, y, w, h)` part of a BGRA frame.
    pub fn from_bgra(data: &[u8], stride: u32, (x, y, w, h): (u32, u32, u32, u32)) -> Self {
        let mut out = Vec::with_capacity((w * h * 3) as usize);
        for row in y..y + h {
            let start = (row * stride + x * 4) as usize;
            for px in data[start..start + (w * 4) as usize].chunks_exact(4) {
                out.extend_from_slice(&[px[2], px[1], px[0]]);
            }
        }
        Self { width: w, height: h, data: out }
    }
}

const BAR: [u8; 3] = [0x12, 0x12, 0x14];
const TEXT: [u8; 3] = [0xE8, 0xE8, 0xE8];
const DIM: [u8; 3] = [0x9A, 0x9A, 0xA0];

/// The name the screenshot is shown and saved under: the core ("Menu" for
/// the MiSTer menu) and `taken_at` (the shell's local time, e.g.
/// "2026-10-17 14:32:05"), safe as a file name everywhere.
pub(crate) fn title(core: &str) -> String {
    let core = core.trim();
    if core.is_empty() { "Menu".into() } else { core.into() }
}

pub(crate) fn file_name(core: &str, taken_at: &str) -> String {
    let name = format!("{} {}", title(core), taken_at.trim());
    let safe: String = name
        .chars()
        .map(|c| match c {
            ':' => '.',
            '/' | '\\' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    format!("{}.png", safe.trim())
}

/// `picture` as a PNG; with `stamp`, a bar under it with the core on the
/// left and `taken_at` on the right.
pub(crate) fn make(picture: Rgb, core: &str, taken_at: &str, stamp: bool) -> Result<Screenshot, String> {
    let picture = if stamp { with_bar(picture, &title(core), taken_at.trim()) } else { picture };
    let png = encode_png(&picture)?;
    Ok(Screenshot { png, file_name: file_name(core, taken_at), width: picture.width, height: picture.height })
}

/// Text size in whole pixels per font pixel: 8 px letters on a 360-line
/// picture, 24 on 1080.
fn text_scale(height: u32) -> u32 {
    ((height + 180) / 360).max(1)
}

fn with_bar(picture: Rgb, left: &str, right: &str) -> Rgb {
    let s = text_scale(picture.height);
    let (w, glyph, pad) = (picture.width, 8 * s, 4 * s);
    let bar_h = glyph + 2 * pad;
    let mut data = picture.data;
    data.reserve((w * bar_h * 3) as usize);
    for _ in 0..w * bar_h {
        data.extend_from_slice(&BAR);
    }
    let mut out = Rgb { width: w, height: picture.height + bar_h, data };
    let top = picture.height + pad;

    // The time always fits if anything does; the core gives way.
    let right_w = text_width(right, s);
    let right_x = w.saturating_sub(pad + right_w);
    draw_text(&mut out, right, right_x, top, s, DIM);
    let room = right_x.saturating_sub(2 * pad) / glyph;
    let left: String = if left.chars().count() as u32 > room {
        let keep = room.saturating_sub(1) as usize;
        left.chars().take(keep).chain(std::iter::once('~')).collect()
    } else {
        left.into()
    };
    if room > 0 {
        draw_text(&mut out, &left, pad, top, s, TEXT);
    }
    out
}

fn text_width(text: &str, scale: u32) -> u32 {
    text.chars().count() as u32 * 8 * scale
}

/// 8×8 letters, each font pixel `scale`×`scale`. Characters outside ASCII
/// show as '?'. Anything past the right edge is cut off.
fn draw_text(img: &mut Rgb, text: &str, x: u32, y: u32, scale: u32, color: [u8; 3]) {
    for (i, c) in text.chars().enumerate() {
        let code = if c.is_ascii() && !c.is_ascii_control() { c as usize } else { '?' as usize };
        let glyph = BASIC_LEGACY[code];
        let gx = x + i as u32 * 8 * scale;
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits >> col & 1 == 0 {
                    continue;
                }
                fill(img, gx + col * scale, y + row as u32 * scale, scale, color);
            }
        }
    }
}

fn fill(img: &mut Rgb, x: u32, y: u32, size: u32, color: [u8; 3]) {
    for py in y..(y + size).min(img.height) {
        for px in x..(x + size).min(img.width) {
            let i = ((py * img.width + px) * 3) as usize;
            img.data[i..i + 3].copy_from_slice(&color);
        }
    }
}

fn encode_png(img: &Rgb) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, img.width, img.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&img.data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(w: u32, h: u32) -> Rgb {
        Rgb { width: w, height: h, data: vec![0x80; (w * h * 3) as usize] }
    }

    #[test]
    fn names() {
        assert_eq!(file_name("SNES", "2026-10-17 14:32:05"), "SNES 2026-10-17 14.32.05.png");
        assert_eq!(file_name("", "2026-10-17 14:32:05"), "Menu 2026-10-17 14.32.05.png");
        assert_eq!(file_name("A/B", "x"), "A-B x.png");
    }

    #[test]
    fn crops_bgra() {
        // 2×2 BGRA, take the bottom-right pixel.
        let data = [0, 0, 0, 255, 1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255];
        let rgb = Rgb::from_bgra(&data, 8, (1, 1, 1, 1));
        assert_eq!(rgb.data, vec![9, 8, 7]);
    }

    #[test]
    fn bar_adds_rows_and_text() {
        let shot = make(grey(1440, 1080), "SNES", "2026-10-17 14:32:05", true).unwrap();
        assert_eq!((shot.width, shot.height), (1440, 1080 + 48));
        let plain = make(grey(1440, 1080), "SNES", "2026-10-17 14:32:05", false).unwrap();
        assert_eq!(plain.height, 1080);
        assert_eq!(&plain.png[1..4], b"PNG");
    }

    #[test]
    fn long_core_gives_way_on_a_small_picture() {
        let img = with_bar(grey(160, 120), "AVeryLongCoreNameIndeed", "2026-10-17 14:32:05");
        assert_eq!(img.height, 120 + 16);
    }
}
