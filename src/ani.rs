//! RIFF "ACON" animated cursors, plus a uniform loader for static .cur files.
use crate::bail;
use crate::cur::{self, CursorImage};
use crate::dib::InvertMode;
use crate::error::Result;
use crate::util::*;

pub struct Step {
    /// All sizes available for this animation frame.
    pub images: Vec<CursorImage>,
    /// Display time in jiffies (1/60 s).
    pub jiffies: u32,
}

pub struct Animation {
    pub steps: Vec<Step>,
    pub title: Option<String>,
    pub author: Option<String>,
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end_matches('\0').trim().to_string()
}

pub fn parse_ani(d: &[u8], invert: InvertMode) -> Result<Animation> {
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"ACON" {
        bail!("not an ANI file");
    }
    let end = (8 + u32le(d, 4)? as usize).min(d.len());
    let (mut n_frames, mut n_steps, mut def_rate) = (0usize, 0usize, 10u32);
    let (mut rates, mut seq): (Vec<u32>, Vec<u32>) = (vec![], vec![]);
    let mut frames: Vec<Vec<CursorImage>> = vec![];
    let (mut title, mut author) = (None, None);

    fn walk(
        d: &[u8], mut off: usize, end: usize, invert: InvertMode,
        st: &mut (usize, usize, u32, Vec<u32>, Vec<u32>, Vec<Vec<CursorImage>>, Option<String>, Option<String>),
    ) -> Result<()> {
        while off + 8 <= end {
            let id = &d[off..off + 4];
            let sz = u32le(d, off + 4)? as usize;
            let body_end = (off + 8 + sz).min(end);
            let body = &d[off + 8..body_end];
            match id {
                b"LIST" if body.len() >= 4 => walk(d, off + 12, body_end, invert, st)?,
                b"anih" if body.len() >= 36 => {
                    st.0 = u32le(body, 4)? as usize;
                    st.1 = u32le(body, 8)? as usize;
                    st.2 = u32le(body, 28)?;
                }
                b"rate" => st.3 = body.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
                b"seq " => st.4 = body.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
                b"icon" => st.5.push(cur::parse(body, invert)?),
                b"INAM" => st.6 = Some(text(body)),
                b"IART" => st.7 = Some(text(body)),
                _ => {}
            }
            off += 8 + sz + (sz & 1);
        }
        Ok(())
    }
    let mut st = (n_frames, n_steps, def_rate, rates, seq, frames, title, author);
    walk(d, 12, end, invert, &mut st)?;
    (n_frames, n_steps, def_rate, rates, seq, frames, title, author) = st;
    let _ = n_frames;

    if frames.is_empty() {
        bail!("ANI contains no frames");
    }
    let order: Vec<usize> = if !seq.is_empty() {
        seq.iter().map(|&s| s as usize).collect()
    } else {
        let n = if n_steps > 0 { n_steps } else { frames.len() };
        (0..n).map(|i| i % frames.len()).collect()
    };
    let mut steps = Vec::with_capacity(order.len());
    for (i, &f) in order.iter().enumerate() {
        let Some(imgs) = frames.get(f) else { bail!("sequence references missing frame {}", f) };
        steps.push(Step { images: imgs.clone(), jiffies: rates.get(i).copied().unwrap_or(def_rate) });
    }
    Ok(Animation { steps, title: title.filter(|s| !s.is_empty()), author: author.filter(|s| !s.is_empty()) })
}

/// Load .ani or .cur (detected by content, not extension).
pub fn load(d: &[u8], invert: InvertMode) -> Result<Animation> {
    if d.len() >= 4 && &d[0..4] == b"RIFF" {
        parse_ani(d, invert)
    } else {
        Ok(Animation { steps: vec![Step { images: cur::parse(d, invert)?, jiffies: 0 }], title: None, author: None })
    }
}
