//! Minimal zlib/DEFLATE decoder (RFC 1950/1951), after zlib's `puff`.
use crate::bail;
use crate::error::Result;

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    buf: u64,
    n: u32,
}

impl<'a> Bits<'a> {
    fn bits(&mut self, need: u32) -> Result<u32> {
        while self.n < need {
            if self.pos >= self.d.len() {
                bail!("inflate: unexpected end of stream");
            }
            self.buf |= (self.d[self.pos] as u64) << self.n;
            self.pos += 1;
            self.n += 8;
        }
        let v = (self.buf & ((1u64 << need) - 1)) as u32;
        self.buf >>= need;
        self.n -= need;
        Ok(v)
    }
}

struct Huff {
    count: [u16; 16],
    symbol: Vec<u16>,
}

fn build(lengths: &[u8]) -> Huff {
    let mut count = [0u16; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    let mut offs = [0u16; 16];
    for i in 1..15 {
        offs[i + 1] = offs[i] + count[i];
    }
    let mut symbol = vec![0u16; lengths.len()];
    for (s, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbol[offs[l as usize] as usize] = s as u16;
            offs[l as usize] += 1;
        }
    }
    Huff { count, symbol }
}

fn decode(b: &mut Bits, h: &Huff) -> Result<u16> {
    let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
    for len in 1..16 {
        code |= b.bits(1)? as i32;
        let count = h.count[len] as i32;
        if code - count < first {
            return Ok(h.symbol[(index + (code - first)) as usize]);
        }
        index += count;
        first += count;
        first <<= 1;
        code <<= 1;
    }
    bail!("inflate: invalid Huffman code")
}

const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn codes(b: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Result<()> {
    loop {
        let sym = decode(b, lit)? as usize;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let s = sym - 257;
            if s >= 29 {
                bail!("inflate: bad length symbol");
            }
            let len = LBASE[s] as usize + b.bits(LEXT[s] as u32)? as usize;
            let ds = decode(b, dist)? as usize;
            if ds >= 30 {
                bail!("inflate: bad distance symbol");
            }
            let d = DBASE[ds] as usize + b.bits(DEXT[ds] as u32)? as usize;
            if d > out.len() {
                bail!("inflate: distance too far back");
            }
            for _ in 0..len {
                let c = out[out.len() - d];
                out.push(c);
            }
        }
    }
}

pub fn inflate_raw(data: &[u8]) -> Result<Vec<u8>> {
    let mut b = Bits { d: data, pos: 0, buf: 0, n: 0 };
    let mut out = Vec::new();
    loop {
        let last = b.bits(1)?;
        match b.bits(2)? {
            0 => {
                b.buf = 0;
                b.n = 0;
                let len = crate::util::u16le(data, b.pos)? as usize;
                let nlen = crate::util::u16le(data, b.pos + 2)? as usize;
                if len != (!nlen & 0xffff) {
                    bail!("inflate: stored block length mismatch");
                }
                let s = b.pos + 4;
                match data.get(s..s + len) {
                    Some(x) => out.extend_from_slice(x),
                    None => bail!("inflate: truncated stored block"),
                }
                b.pos = s + len;
            }
            1 => {
                let mut l = [0u8; 288];
                for (i, v) in l.iter_mut().enumerate() {
                    *v = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                codes(&mut b, &mut out, &build(&l), &build(&[5u8; 30]))?;
            }
            2 => {
                let nlen = b.bits(5)? as usize + 257;
                let ndist = b.bits(5)? as usize + 1;
                let ncode = b.bits(4)? as usize + 4;
                if nlen > 286 || ndist > 30 {
                    bail!("inflate: bad dynamic block counts");
                }
                const ORDER: [usize; 19] =
                    [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in ORDER.iter().take(ncode) {
                    cl[o] = b.bits(3)? as u8;
                }
                let clh = build(&cl);
                let mut lengths = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = decode(&mut b, &clh)?;
                    if sym < 16 {
                        lengths[i] = sym as u8;
                        i += 1;
                    } else {
                        let (val, rep) = match sym {
                            16 => {
                                if i == 0 {
                                    bail!("inflate: repeat with no previous length");
                                }
                                (lengths[i - 1], 3 + b.bits(2)? as usize)
                            }
                            17 => (0, 3 + b.bits(3)? as usize),
                            _ => (0, 11 + b.bits(7)? as usize),
                        };
                        if i + rep > nlen + ndist {
                            bail!("inflate: too many code lengths");
                        }
                        for _ in 0..rep {
                            lengths[i] = val;
                            i += 1;
                        }
                    }
                }
                codes(&mut b, &mut out, &build(&lengths[..nlen]), &build(&lengths[nlen..]))?;
            }
            _ => bail!("inflate: invalid block type"),
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

pub fn zlib(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 2 || data[0] & 0x0f != 8 {
        bail!("not a zlib stream");
    }
    if data[1] & 0x20 != 0 {
        bail!("zlib preset dictionary unsupported");
    }
    inflate_raw(&data[2..])
}
