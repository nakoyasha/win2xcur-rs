//! .cur / .ico container.
use crate::bail;
use crate::dib::{self, InvertMode};
use crate::error::Result;
use crate::image::Image;
use crate::png;
use crate::util::*;

#[derive(Clone, Debug)]
pub struct CursorImage {
    pub image: Image,
    pub hot_x: u32,
    pub hot_y: u32,
    /// Number of "invert screen" pixels that had to be approximated.
    pub inverted: usize,
}

impl CursorImage {
    pub fn dim(&self) -> u32 {
        self.image.w.max(self.image.h)
    }
}

pub fn looks_like_cur(d: &[u8]) -> bool {
    d.len() >= 6 && d[0] == 0 && d[1] == 0 && (d[2] == 1 || d[2] == 2) && d[3] == 0
}

pub fn parse(d: &[u8], invert: InvertMode) -> Result<Vec<CursorImage>> {
    if !looks_like_cur(d) {
        bail!("not a .cur/.ico file");
    }
    let kind = u16le(d, 2)?;
    let count = u16le(d, 4)? as usize;
    let mut out = Vec::new();
    for i in 0..count {
        let e = 6 + i * 16;
        let hx = u16le(d, e + 4)? as u32;
        let hy = u16le(d, e + 6)? as u32;
        let size = u32le(d, e + 8)? as usize;
        let off = u32le(d, e + 12)? as usize;
        if off >= d.len() {
            bail!("cursor entry {} points past end of file", i);
        }
        let data = &d[off..(off.saturating_add(size)).min(d.len())];
        let (image, inverted) = if png::is_png(data) {
            (png::decode(data)?, 0)
        } else {
            let dec = dib::decode(data, invert)?;
            (dec.image, dec.inverted)
        };
        let (hot_x, hot_y) = if kind == 2 { (hx, hy) } else { (0, 0) };
        out.push(CursorImage {
            hot_x: hot_x.min(image.w - 1),
            hot_y: hot_y.min(image.h - 1),
            image,
            inverted,
        });
    }
    if out.is_empty() {
        bail!("cursor file contains no images");
    }
    Ok(out)
}
