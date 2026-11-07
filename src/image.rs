//! RGBA image (straight alpha) with resampling and the optional pointer shadow.

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Nearest,
    Bilinear,
    Lanczos,
}

impl Image {
    pub fn new(w: u32, h: u32) -> Self {
        Image { w, h, rgba: vec![0; w as usize * h as usize * 4] }
    }

    /// True when every pixel is fully opaque or fully transparent (pixel art / 1-bit mask cursors).
    pub fn is_binary_alpha(&self) -> bool {
        self.rgba.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255)
    }

    pub fn resize(&self, nw: u32, nh: u32, filter: Filter) -> Image {
        if nw == self.w && nh == self.h {
            return self.clone();
        }
        match filter {
            Filter::Nearest => self.nearest(nw, nh),
            Filter::Bilinear => self.resample(nw, nh, |x| (1.0 - x.abs()).max(0.0), 1.0),
            Filter::Lanczos => self.resample(nw, nh, lanczos3, 3.0),
        }
    }

    /// Centre-sampled nearest neighbour: for integer upscales every source pixel becomes an exact k x k block.
    fn nearest(&self, nw: u32, nh: u32) -> Image {
        let mut out = Image::new(nw, nh);
        let (sw, sh) = (self.w as u64, self.h as u64);
        for y in 0..nh as u64 {
            let sy = ((2 * y + 1) * sh / (2 * nh as u64)).min(sh - 1);
            for x in 0..nw as u64 {
                let sx = ((2 * x + 1) * sw / (2 * nw as u64)).min(sw - 1);
                let s = ((sy * sw + sx) * 4) as usize;
                let o = ((y * nw as u64 + x) * 4) as usize;
                out.rgba[o..o + 4].copy_from_slice(&self.rgba[s..s + 4]);
            }
        }
        out
    }

    fn resample(&self, nw: u32, nh: u32, kernel: fn(f32) -> f32, support: f32) -> Image {
        let (sw, sh, nw_, nh_) = (self.w as usize, self.h as usize, nw as usize, nh as usize);
        let mut src = vec![0f32; sw * sh * 4];
        for i in 0..sw * sh {
            let a = self.rgba[i * 4 + 3] as f32 / 255.0;
            for c in 0..3 {
                src[i * 4 + c] = self.rgba[i * 4 + c] as f32 / 255.0 * a;
            }
            src[i * 4 + 3] = a;
        }
        let wx = weights(sw, nw_, kernel, support);
        let wy = weights(sh, nh_, kernel, support);
        let mut tmp = vec![0f32; nw_ * sh * 4];
        for y in 0..sh {
            for (x, ws) in wx.iter().enumerate() {
                for &(s, w) in ws {
                    for c in 0..4 {
                        tmp[(y * nw_ + x) * 4 + c] += src[(y * sw + s) * 4 + c] * w;
                    }
                }
            }
        }
        let mut out = Image::new(nw, nh);
        for (y, ws) in wy.iter().enumerate() {
            for x in 0..nw_ {
                let mut acc = [0f32; 4];
                for &(s, w) in ws {
                    for c in 0..4 {
                        acc[c] += tmp[(s * nw_ + x) * 4 + c] * w;
                    }
                }
                let a = acc[3].clamp(0.0, 1.0);
                let o = (y * nw_ + x) * 4;
                if a > 0.0 {
                    for c in 0..3 {
                        out.rgba[o + c] = (acc[c].clamp(0.0, a) / a * 255.0 + 0.5) as u8;
                    }
                    out.rgba[o + 3] = (a * 255.0 + 0.5) as u8;
                }
            }
        }
        out
    }

    /// Approximation of Windows' pointer shadow: a blurred, offset, half-transparent black copy
    /// *behind* the cursor. The canvas grows to the right/bottom only, so the hotspot is unchanged.
    pub fn with_shadow(&self, offset: u32, sigma: f32, opacity: f32) -> Image {
        let rad = (sigma * 3.0).ceil() as usize;
        let pad = offset as usize + rad;
        let (w, h) = (self.w as usize + pad, self.h as usize + pad);
        let mut a = vec![0f32; w * h];
        for y in 0..self.h as usize {
            for x in 0..self.w as usize {
                a[(y + offset as usize) * w + x + offset as usize] = self.rgba[(y * self.w as usize + x) * 4 + 3] as f32 / 255.0;
            }
        }
        if sigma > 0.0 {
            let k: Vec<f32> = (-(rad as i32)..=rad as i32)
                .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
                .collect();
            let ks: f32 = k.iter().sum();
            let blur = |src: &[f32], horiz: bool| -> Vec<f32> {
                let mut o = vec![0f32; w * h];
                for y in 0..h {
                    for x in 0..w {
                        let mut s = 0.0;
                        for (i, kv) in k.iter().enumerate() {
                            let d = i as i32 - rad as i32;
                            let (xx, yy) = if horiz { (x as i32 + d, y as i32) } else { (x as i32, y as i32 + d) };
                            if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                                s += src[yy as usize * w + xx as usize] * kv;
                            }
                        }
                        o[y * w + x] = s / ks;
                    }
                }
                o
            };
            a = blur(&blur(&a, true), false);
        }
        let mut out = Image::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let sa = a[y * w + x] * opacity;
                let (mut fr, mut fg, mut fb, mut fa) = (0f32, 0f32, 0f32, 0f32);
                if x < self.w as usize && y < self.h as usize {
                    let i = (y * self.w as usize + x) * 4;
                    fa = self.rgba[i + 3] as f32 / 255.0;
                    fr = self.rgba[i] as f32 / 255.0;
                    fg = self.rgba[i + 1] as f32 / 255.0;
                    fb = self.rgba[i + 2] as f32 / 255.0;
                }
                let oa = fa + sa * (1.0 - fa);
                let o = (y * w + x) * 4;
                if oa > 0.0 {
                    out.rgba[o] = (fr * fa / oa * 255.0 + 0.5) as u8;
                    out.rgba[o + 1] = (fg * fa / oa * 255.0 + 0.5) as u8;
                    out.rgba[o + 2] = (fb * fa / oa * 255.0 + 0.5) as u8;
                    out.rgba[o + 3] = (oa * 255.0 + 0.5) as u8;
                }
            }
        }
        out
    }
}

fn lanczos3(x: f32) -> f32 {
    let x = x.abs();
    if x >= 3.0 {
        0.0
    } else if x < 1e-6 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        3.0 * px.sin() * (px / 3.0).sin() / (px * px)
    }
}

fn weights(sn: usize, dn: usize, kernel: fn(f32) -> f32, support: f32) -> Vec<Vec<(usize, f32)>> {
    let scale = sn as f32 / dn as f32;
    let fscale = scale.max(1.0);
    let sup = support * fscale;
    (0..dn)
        .map(|d| {
            let centre = (d as f32 + 0.5) * scale;
            let lo = ((centre - sup).floor() as i64).max(0);
            let hi = ((centre + sup).ceil() as i64).min(sn as i64);
            let mut ws: Vec<(usize, f32)> =
                (lo..hi).map(|s| (s as usize, kernel((s as f32 + 0.5 - centre) / fscale))).collect();
            let sum: f32 = ws.iter().map(|w| w.1).sum();
            if sum != 0.0 {
                for w in ws.iter_mut() {
                    w.1 /= sum;
                }
            }
            ws
        })
        .collect()
}
