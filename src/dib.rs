//! Decoder for the BMP/DIB payload inside .cur/.ico entries (XOR bitmap + 1-bit AND mask).
use crate::bail;
use crate::error::Result;
use crate::image::Image;
use crate::util::*;

/// What to do with "invert screen" pixels (AND=1, XOR!=0). X11 cursors cannot invert the backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InvertMode {
    Black,
    White,
    Transparent,
}

pub struct Decoded {
    pub image: Image,
    pub inverted: usize,
}

fn extract(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    ((((v & mask) >> shift) as u64 * 255 + max as u64 / 2) / max as u64) as u8
}

pub fn decode(d: &[u8], invert: InvertMode) -> Result<Decoded> {
    let hdr = u32le(d, 0)? as usize;
    if hdr < 40 {
        bail!("unsupported DIB header size {}", hdr);
    }
    let (w, hraw) = (i32le(d, 4)?, i32le(d, 8)?);
    let bpp = u16le(d, 14)? as usize;
    let comp = u32le(d, 16)?;
    let clrused = u32le(d, 32)? as usize;
    if w <= 0 || hraw == 0 || w > 32767 || hraw.unsigned_abs() > 65536 {
        bail!("bad DIB dimensions {}x{}", w, hraw);
    }
    if comp != 0 && comp != 3 {
        bail!("compressed DIB (type {}) unsupported", comp);
    }
    if !matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) {
        bail!("unsupported DIB bit depth {}", bpp);
    }
    let w = w as usize;
    let top_down = hraw < 0;
    let htot = hraw.unsigned_abs() as usize;

    let (mut rm, mut gm, mut bm, mut am) = match bpp {
        16 => (0x7C00u32, 0x03E0u32, 0x001Fu32, 0u32),
        _ => (0x00FF_0000, 0x0000_FF00, 0x0000_00FF, if bpp == 32 { 0xFF00_0000 } else { 0 }),
    };
    let mut mask_bytes = 0;
    if comp == 3 && (bpp == 16 || bpp == 32) {
        rm = u32le(d, 40)?;
        gm = u32le(d, 44)?;
        bm = u32le(d, 48)?;
        if hdr >= 56 {
            am = u32le(d, 52)?;
        }
        if hdr == 40 {
            mask_bytes = 12;
        }
    }
    let pal_n = if bpp <= 8 { if clrused > 0 { clrused.min(1 << bpp) } else { 1 << bpp } } else { 0 };
    let pal_off = hdr + mask_bytes;
    let pix_off = pal_off + pal_n * 4;
    let color_stride = ((w * bpp + 31) / 32) * 4;
    let mask_stride = ((w + 31) / 32) * 4;
    let need = |h: usize, m: bool| pix_off + color_stride * h + if m { mask_stride * h } else { 0 };
    let mut h = htot / 2;
    let mut has_mask = true;
    if need(h, true) > d.len() {
        if need(htot, false) <= d.len() {
            h = htot;
            has_mask = false;
        } else if need(h, false) <= d.len() {
            has_mask = false;
        } else {
            bail!("truncated DIB pixel data");
        }
    }
    if h == 0 {
        bail!("empty DIB");
    }
    let mask_off = pix_off + color_stride * h;
    let pal = |i: usize| -> (u8, u8, u8) {
        match d.get(pal_off + i * 4..pal_off + i * 4 + 3) {
            Some(p) if i < pal_n => (p[2], p[1], p[0]),
            _ => (0, 0, 0),
        }
    };

    let mut rgba = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let fy = if top_down { y } else { h - 1 - y };
        let row = &d[pix_off + fy * color_stride..pix_off + (fy + 1) * color_stride];
        for x in 0..w {
            let (r, g, b, a) = match bpp {
                1 | 4 | 8 => {
                    let idx = match bpp {
                        8 => row[x] as usize,
                        4 => (if x % 2 == 0 { row[x / 2] >> 4 } else { row[x / 2] & 15 }) as usize,
                        _ => ((row[x / 8] >> (7 - x % 8)) & 1) as usize,
                    };
                    let (r, g, b) = pal(idx);
                    (r, g, b, 255)
                }
                16 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]) as u32;
                    (extract(v, rm), extract(v, gm), extract(v, bm), 255)
                }
                24 => (row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255),
                _ => {
                    let v = u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]);
                    if comp == 3 {
                        (extract(v, rm), extract(v, gm), extract(v, bm), extract(v, am))
                    } else {
                        (row[x * 4 + 2], row[x * 4 + 1], row[x * 4], row[x * 4 + 3])
                    }
                }
            };
            if bpp == 32 && a != 0 {
                any_alpha = true;
            }
            let o = (y * w + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[r, g, b, a]);
        }
    }

    let mut inverted = 0;
    if !any_alpha {
        // Windows rule: a 32-bit cursor with no non-zero alpha falls back to the AND mask.
        for y in 0..h {
            let fy = if top_down { y } else { h - 1 - y };
            for x in 0..w {
                let bit = if has_mask {
                    (d[mask_off + fy * mask_stride + x / 8] >> (7 - x % 8)) & 1
                } else {
                    0
                };
                let o = (y * w + x) * 4;
                if bit == 0 {
                    rgba[o + 3] = 255;
                } else if rgba[o] | rgba[o + 1] | rgba[o + 2] != 0 {
                    inverted += 1;
                    let (c, a) = match invert {
                        InvertMode::Black => (0, 255),
                        InvertMode::White => (255, 255),
                        InvertMode::Transparent => (0, 0),
                    };
                    rgba[o..o + 4].copy_from_slice(&[c, c, c, a]);
                } else {
                    rgba[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
    }
    Ok(Decoded { image: Image { w: w as u32, h: h as u32, rgba }, inverted })
}
