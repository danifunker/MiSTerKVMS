//! A JetKVM as a capture device: id `jetkvm://<address>` (e.g.
//! `jetkvm://192.168.1.50`). Its video arrives over WebRTC and is decoded to
//! BGRA by `kvms-jetkvm`, so the capture server and the apps' local source
//! use it like a card. A password, if the JetKVM has one, comes from
//! `JETKVM_PASSWORD` for now (docs/JETKVM.md).
//!
//! A JetKVM has one session: opening this one takes over from its web page
//! or any other viewer.

use std::time::Duration;

use kvms_jetkvm::Connection;

use crate::{clock, CaptureError, PixelFormat, VideoFormat, VideoFrame};

pub const PREFIX: &str = "jetkvm://";

/// How long to wait for the first picture when opening.
const FIRST_PICTURE: Duration = Duration::from_secs(10);

pub(crate) struct Reader {
    conn: Connection,
    last: u64,
    /// The first picture, read while opening to learn the size.
    first: Option<VideoFrame>,
}

fn to_frame(f: kvms_jetkvm::Frame) -> VideoFrame {
    VideoFrame {
        format: PixelFormat::Bgra,
        width: f.width,
        height: f.height,
        stride: f.width * 4,
        data: f.bgra,
        timestamp: clock::now_100ns(),
    }
}

impl Reader {
    pub(crate) fn open(id: &str) -> Result<(Self, VideoFormat), CaptureError> {
        let address = id.strip_prefix(PREFIX).unwrap_or(id).trim_end_matches('/');
        if address.is_empty() {
            return Err(CaptureError::NotFound("jetkvm:// needs an address".into()));
        }
        let password = std::env::var("JETKVM_PASSWORD").ok();
        let conn = Connection::connect(address, password.as_deref()).map_err(CaptureError::NotFound)?;
        // The size comes with the first picture. No picture at all: the
        // JetKVM has no HDMI signal, and wait_frame says what to check.
        let first = conn.wait_frame(0, FIRST_PICTURE).map_err(CaptureError::Other)?;
        let number = first.number;
        let first = to_frame(first);
        let format = VideoFormat { pixel: PixelFormat::Bgra, width: first.width, height: first.height, fps: 30.0 };
        Ok((Self { conn, last: number, first: Some(first) }, format))
    }

    pub(crate) fn next_frame(&mut self, timeout: Duration) -> Result<Option<VideoFrame>, CaptureError> {
        if let Some(f) = self.first.take() {
            return Ok(Some(f));
        }
        match self.conn.take_frame(self.last, timeout) {
            Ok(Some(f)) => {
                self.last = f.number;
                Ok(Some(to_frame(f)))
            }
            Ok(None) => Ok(None),
            Err(why) => Err(CaptureError::Lost(why)),
        }
    }
}
