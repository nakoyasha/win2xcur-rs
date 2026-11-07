//! PNG decode (non-interlaced) and a tiny encoder (stored deflate) for debug output.
use crate::bail;
use crate::error::Result;
use crate::image::Image;

pub const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 13, 10, 26, 10];

pub fn is_png(d: &[u8]) -> bool {
    d.len() >= 8 && d[..8] == SIG
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (a as i32, b as i32, c as i32);
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn sample(row: &[u8], i: usize, depth: u8) -> u16 {
    match depth {
        8 => row[i] as u16,
        16 => u16::from_be_bytes([row[2 * i], row[2 * i + 1]]),
        d => {
            let per = 8 / d as usize;
            let shift = 8 - d as usize * (i % per + 1);
            ((row[i / per] >> shift) & ((1u8 << d) - 1)) as u16
        }
    }
}

fn to8(v: u16, depth: u8) -> u8 {
    match depth {
        8 => v as u8,
        16 => (v >> 8) as u8,
        d => (v as u32 * 255 / ((1u32 << d) - 1)) as u8,
    }
}

pub fn decode(d: &[u8]) -> Result<Image> {
    if !is_png(d) {
        bail!("not a PNG");
    }
    let mut pos = 8;
    let (mut w, mut h, mut depth, mut ct, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let (mut plte, mut trns, mut idat) = (Vec::new(), Vec::new(), Vec::new());
    while pos + 8 <= d.len() {
        let len = u32::from_be_bytes([d[pos], d[pos + 1], d[pos + 2], d[pos + 3]]) as usize;
        let ty = &d[pos + 4..pos + 8];
        let body = match d.get(pos + 8..pos + 8 + len) {
            Some(b) => b,
            None => bail!("truncated PNG chunk"),
        };
        match ty {
            b"IHDR" => {
                if body.len() < 13 {
                    bail!("bad IHDR");
                }
                w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize;
                h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
                depth = body[8];
                ct = body[9];
                interlace = body[12];
            }
            b"PLTE" => plte = body.to_vec(),
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    if w == 0 || h == 0 || w > 32767 || h > 32767 {
        bail!("bad PNG dimensions");
    }
    if interlace != 0 {
        bail!("interlaced PNG unsupported");
    }
    let ch = match ct {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => bail!("bad PNG colour type"),
    };
    if !matches!(depth, 1 | 2 | 4 | 8 | 16) {
        bail!("bad PNG bit depth");
    }
    let bits = ch * depth as usize;
    let stride = (w * bits + 7) / 8;
    let bpb = ((bits + 7) / 8).max(1);
    let raw = crate::inflate::zlib(&idat)?;
    if raw.len() < (stride + 1) * h {
        bail!("PNG pixel data truncated");
    }
    let mut img = vec![0u8; stride * h];
    for y in 0..h {
        let ft = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= bpb { img[y * stride + x - bpb] } else { 0 };
            let b = if y > 0 { img[(y - 1) * stride + x] } else { 0 };
            let c = if x >= bpb && y > 0 { img[(y - 1) * stride + x - bpb] } else { 0 };
            img[y * stride + x] = match ft {
                0 => line[x],
                1 => line[x].wrapping_add(a),
                2 => line[x].wrapping_add(b),
                3 => line[x].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => line[x].wrapping_add(paeth(a, b, c)),
                _ => bail!("bad PNG filter"),
            };
        }
    }
    let mut out = Image::new(w as u32, h as u32);
    let key = |n: usize| -> Option<u16> {
        if trns.len() >= n * 2 {
            Some(u16::from_be_bytes([trns[n * 2 - 2], trns[n * 2 - 1]]))
        } else {
            None
        }
    };
    for y in 0..h {
        let row = &img[y * stride..(y + 1) * stride];
        for x in 0..w {
            let o = (y * w + x) * 4;
            let px: [u8; 4] = match ct {
                0 => {
                    let v = sample(row, x, depth);
                    let g = to8(v, depth);
                    let a = if trns.len() >= 2 && key(1) == Some(v) { 0 } else { 255 };
                    [g, g, g, a]
                }
                2 => {
                    let (r, g, b) =
                        (sample(row, x * 3, depth), sample(row, x * 3 + 1, depth), sample(row, x * 3 + 2, depth));
                    let tr = trns.len() >= 6
                        && u16::from_be_bytes([trns[0], trns[1]]) == r
                        && u16::from_be_bytes([trns[2], trns[3]]) == g
                        && u16::from_be_bytes([trns[4], trns[5]]) == b;
                    [to8(r, depth), to8(g, depth), to8(b, depth), if tr { 0 } else { 255 }]
                }
                3 => {
                    let i = sample(row, x, depth) as usize;
                    if i * 3 + 2 >= plte.len() {
                        [0, 0, 0, 255]
                    } else {
                        [plte[i * 3], plte[i * 3 + 1], plte[i * 3 + 2], *trns.get(i).unwrap_or(&255)]
                    }
                }
                4 => {
                    let g = to8(sample(row, x * 2, depth), depth);
                    [g, g, g, to8(sample(row, x * 2 + 1, depth), depth)]
                }
                _ => [
                    to8(sample(row, x * 4, depth), depth),
                    to8(sample(row, x * 4 + 1, depth), depth),
                    to8(sample(row, x * 4 + 2, depth), depth),
                    to8(sample(row, x * 4 + 3, depth), depth),
                ],
            };
            out.rgba[o..o + 4].copy_from_slice(&px);
        }
    }
    Ok(out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

pub fn encode(img: &Image) -> Vec<u8> {
    let mut raw = Vec::with_capacity((img.w as usize * 4 + 1) * img.h as usize);
    for y in 0..img.h as usize {
        raw.push(0);
        raw.extend_from_slice(&img.rgba[y * img.w as usize * 4..(y + 1) * img.w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    if chunks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = SIG.to_vec();
    let mut chunk = |ty: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(body);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&img.w.to_be_bytes());
    ihdr.extend_from_slice(&img.h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}
