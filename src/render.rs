//! Debug rendering: contact sheet of frames over a checkerboard, hotspot marked in red.
use crate::image::Image;

pub struct Cell<'a> {
    pub image: &'a Image,
    pub hot_x: u32,
    pub hot_y: u32,
}

pub fn contact_sheet(cells: &[Cell], zoom: u32, cols: usize) -> Image {
    let z = zoom.max(1) as usize;
    let cw = cells.iter().map(|c| c.image.w as usize).max().unwrap_or(1) * z + 8;
    let ch = cells.iter().map(|c| c.image.h as usize).max().unwrap_or(1) * z + 8;
    let cols = cols.max(1).min(cells.len().max(1));
    let rows = (cells.len() + cols - 1) / cols;
    let mut out = Image::new((cols * cw) as u32, (rows * ch) as u32);
    for y in 0..out.h as usize {
        for x in 0..out.w as usize {
            let v = if (x / 8 + y / 8) % 2 == 0 { 232 } else { 252 };
            let o = (y * out.w as usize + x) * 4;
            out.rgba[o..o + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    for (i, c) in cells.iter().enumerate() {
        let (ox, oy) = ((i % cols) * cw + 4, (i / cols) * ch + 4);
        for y in 0..c.image.h as usize * z {
            for x in 0..c.image.w as usize * z {
                let s = ((y / z) * c.image.w as usize + x / z) * 4;
                let a = c.image.rgba[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let o = ((oy + y) * out.w as usize + ox + x) * 4;
                for k in 0..3 {
                    out.rgba[o + k] = ((c.image.rgba[s + k] as u32 * a + out.rgba[o + k] as u32 * (255 - a)) / 255) as u8;
                }
            }
        }
        // hotspot: red outline around the hotspot pixel
        let (hx, hy) = (c.hot_x as usize * z, c.hot_y as usize * z);
        for t in 0..z {
            for &(x, y) in &[(hx + t, hy), (hx + t, hy + z - 1), (hx, hy + t), (hx + z - 1, hy + t)] {
                let o = ((oy + y) * out.w as usize + ox + x) * 4;
                out.rgba[o..o + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    }
    out
}
