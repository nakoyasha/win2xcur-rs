//! Xcursor file format reader/writer (premultiplied ARGB32).
use crate::bail;
use crate::error::Result;
use crate::util::*;

const IMAGE_TYPE: u32 = 0xfffd_0002;

#[derive(Clone, Debug)]
pub struct XcImage {
    pub nominal: u32,
    pub width: u32,
    pub height: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub delay: u32,
    /// Premultiplied ARGB, row-major.
    pub argb: Vec<u32>,
}

pub fn premultiply(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4)
        .map(|p| {
            let a = p[3] as u32;
            let m = |c: u8| (c as u32 * a + 127) / 255;
            (a << 24) | (m(p[0]) << 16) | (m(p[1]) << 8) | m(p[2])
        })
        .collect()
}

pub fn unpremultiply(argb: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(argb.len() * 4);
    for &p in argb {
        let a = p >> 24;
        let u = |c: u32| if a == 0 { 0 } else { ((c * 255 + a / 2) / a).min(255) as u8 };
        out.extend_from_slice(&[u((p >> 16) & 255), u((p >> 8) & 255), u(p & 255), a as u8]);
    }
    out
}

pub fn write(images: &[XcImage]) -> Vec<u8> {
    let mut out = Vec::new();
    let put = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"Xcur");
    put(&mut out, 16);
    put(&mut out, 0x10000);
    put(&mut out, images.len() as u32);
    let mut pos = 16 + images.len() as u32 * 12;
    for im in images {
        put(&mut out, IMAGE_TYPE);
        put(&mut out, im.nominal);
        put(&mut out, pos);
        pos += 36 + im.width * im.height * 4;
    }
    for im in images {
        for v in [36, IMAGE_TYPE, im.nominal, 1, im.width, im.height, im.hot_x, im.hot_y, im.delay] {
            put(&mut out, v);
        }
        for &p in &im.argb {
            put(&mut out, p);
        }
    }
    out
}

pub fn is_xcursor(d: &[u8]) -> bool {
    d.len() >= 16 && &d[0..4] == b"Xcur"
}

pub fn read(d: &[u8]) -> Result<Vec<XcImage>> {
    if !is_xcursor(d) {
        bail!("not an Xcursor file");
    }
    let hdr = u32le(d, 4)? as usize;
    let ntoc = u32le(d, 12)? as usize;
    let mut out = Vec::new();
    for i in 0..ntoc {
        let e = hdr + i * 12;
        if u32le(d, e)? != IMAGE_TYPE {
            continue; // comments etc.
        }
        let nominal = u32le(d, e + 4)?;
        let p = u32le(d, e + 8)? as usize;
        let (w, h) = (u32le(d, p + 16)?, u32le(d, p + 20)?);
        let mut argb = Vec::with_capacity((w * h) as usize);
        for k in 0..(w * h) as usize {
            argb.push(u32le(d, p + 36 + k * 4)?);
        }
        out.push(XcImage {
            nominal, width: w, height: h,
            hot_x: u32le(d, p + 24)?, hot_y: u32le(d, p + 28)?, delay: u32le(d, p + 32)?, argb,
        });
    }
    Ok(out)
}
