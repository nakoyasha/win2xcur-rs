//! Animation -> Xcursor image set, with Windows-faithful timing, hotspots and scaling.
use crate::ani::Animation;
use crate::bail;
use crate::cur::CursorImage;
use crate::dib::InvertMode;
use crate::error::Result;
use crate::image::Filter;
use crate::xcursor::{premultiply, XcImage};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterChoice {
    /// Nearest for hard-edged (pixel-art) sources when enlarging, Lanczos otherwise.
    Auto,
    Nearest,
    Bilinear,
    Lanczos,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Extra nominal sizes to synthesise. Native sizes of the file are always included unscaled.
    pub sizes: Vec<u32>,
    pub filter: FilterChoice,
    pub shadow: bool,
    pub invert: InvertMode,
}

/// Windows' cursor-size slider steps by 16 px starting at 32 px (also what 100%/150%/200% DPI scaling gives).
pub fn default_sizes() -> Vec<u32> {
    (32..=128).step_by(16).collect()
}

impl Default for Options {
    fn default() -> Self {
        Options { sizes: default_sizes(), filter: FilterChoice::Auto, shadow: false, invert: InvertMode::Black }
    }
}

/// 1 jiffy = 1/60 s, rounded to whole milliseconds.
pub fn jiffies_to_ms(j: u32) -> u32 {
    ((j as u64 * 1000 + 30) / 60) as u32
}

/// Choose which source image to derive size `s` from: exact match, else the largest smaller one
/// (so pixel art is enlarged by an integer factor where possible), else the smallest larger one.
fn pick_source(images: &[CursorImage], s: u32) -> &CursorImage {
    if let Some(i) = images.iter().find(|i| i.dim() == s) {
        return i;
    }
    if let Some(i) = images.iter().filter(|i| i.dim() < s).max_by_key(|i| i.dim()) {
        return i;
    }
    images.iter().min_by_key(|i| i.dim()).unwrap()
}

fn render_at(src: &CursorImage, s: u32, opts: &Options) -> (crate::image::Image, u32, u32) {
    let (w, h, d) = (src.image.w as u64, src.image.h as u64, src.dim() as u64);
    let (mut img, hx, hy) = if d == s as u64 {
        (src.image.clone(), src.hot_x, src.hot_y)
    } else {
        let nw = ((w * s as u64 + d / 2) / d).max(1) as u32;
        let nh = ((h * s as u64 + d / 2) / d).max(1) as u32;
        let filter = match opts.filter {
            FilterChoice::Nearest => Filter::Nearest,
            FilterChoice::Bilinear => Filter::Bilinear,
            FilterChoice::Lanczos => Filter::Lanczos,
            FilterChoice::Auto => {
                if src.image.is_binary_alpha() && s as u64 > d { Filter::Nearest } else { Filter::Lanczos }
            }
        };
        let scale = |v: u32, n: u32, o: u64| ((v as u64 * n as u64 * 2 + o) / (2 * o)) as u32;
        (
            src.image.resize(nw, nh, filter),
            scale(src.hot_x, nw, w).min(nw - 1),
            scale(src.hot_y, nh, h).min(nh - 1),
        )
    };
    if opts.shadow {
        let off = ((s as f32 * 0.06).round() as u32).max(1);
        img = img.with_shadow(off, s as f32 * 0.03, 0.5);
    }
    (img, hx, hy)
}

pub fn to_xcursor(anim: &Animation, opts: &Options) -> Result<Vec<XcImage>> {
    if anim.steps.is_empty() || anim.steps.iter().any(|s| s.images.is_empty()) {
        bail!("animation has no frames");
    }
    let ladder: Vec<u32> = opts.sizes.iter().copied().filter(|&s| s > 0).collect();
    // A native image keeps its exact pixels. Its nominal size snaps to a requested size that is within
    // ~6% (e.g. a 32x33 cursor is filed as 32) so Xcursor selects the untouched pixels, not a resampled copy.
    let nominal_of = |d: u32| -> u32 {
        ladder.iter().copied().find(|&s| (s as i64 - d as i64).abs() * 16 <= s as i64).unwrap_or(d)
    };
    let mut sizes: BTreeSet<u32> = ladder.iter().copied().collect();
    for st in &anim.steps {
        for im in &st.images {
            sizes.insert(nominal_of(im.dim()));
        }
    }
    let animated = anim.steps.len() > 1;
    let mut out = Vec::new();
    for &s in &sizes {
        for st in &anim.steps {
            let (img, hot_x, hot_y) = match st.images.iter().find(|i| nominal_of(i.dim()) == s) {
                Some(native) => (native.image.clone(), native.hot_x, native.hot_y),
                None => render_at(pick_source(&st.images, s), s, opts),
            };
            let (img, hot_x, hot_y) = if opts.shadow && st.images.iter().any(|i| nominal_of(i.dim()) == s) {
                let off = ((s as f32 * 0.06).round() as u32).max(1);
                (img.with_shadow(off, s as f32 * 0.03, 0.5), hot_x, hot_y)
            } else {
                (img, hot_x, hot_y)
            };
            out.push(XcImage {
                nominal: s,
                width: img.w,
                height: img.h,
                hot_x,
                hot_y,
                delay: if animated { jiffies_to_ms(st.jiffies).max(1) } else { 0 },
                argb: premultiply(&img.rgba),
            });
        }
    }
    Ok(out)
}
