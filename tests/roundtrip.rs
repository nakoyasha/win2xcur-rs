use win2xcur::ani;
use win2xcur::convert::{self, FilterChoice, Options};
use win2xcur::dib::InvertMode;
use win2xcur::{inflate, png, xcursor};
use win2xcur::image::Image;

/// Build a .cur with one 32-bit BGRA entry (straight alpha), bottom-up, with a zero AND mask.
fn cur32(w: u32, h: u32, rgba: &[u8], hot: (u16, u16)) -> Vec<u8> {
    let mut dib = Vec::new();
    for v in [40u32, w, h * 2, 0x0020_0001, 0, 0, 0, 0, 0, 0] {
        dib.extend_from_slice(&v.to_le_bytes());
    }
    for y in (0..h).rev() {
        for x in 0..w {
            let p = &rgba[((y * w + x) * 4) as usize..][..4];
            dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    dib.extend(std::iter::repeat(0).take((((w + 31) / 32) * 4 * h) as usize));
    wrap_cur(w, h, hot, dib)
}

fn wrap_cur(w: u32, h: u32, hot: (u16, u16), dib: Vec<u8>) -> Vec<u8> {
    let mut f = vec![0, 0, 2, 0, 1, 0, w as u8, h as u8, 0, 0];
    f.extend_from_slice(&hot.0.to_le_bytes());
    f.extend_from_slice(&hot.1.to_le_bytes());
    f.extend_from_slice(&(dib.len() as u32).to_le_bytes());
    f.extend_from_slice(&22u32.to_le_bytes());
    f.extend(dib);
    f
}

/// 2-colour 8bpp cursor using only the AND mask for transparency / inversion.
fn cur8(w: u32, h: u32, idx: &[u8], mask: &[u8]) -> Vec<u8> {
    let mut dib = Vec::new();
    for v in [40u32, w, h * 2, 0x0008_0001, 0, 0, 0, 0, 2, 0] {
        dib.extend_from_slice(&v.to_le_bytes());
    }
    dib.extend_from_slice(&[0, 0, 0, 0, 255, 255, 255, 0]); // palette: black, white
    let stride = ((w + 3) / 4 * 4) as usize;
    for y in (0..h as usize).rev() {
        let mut row = idx[y * w as usize..(y + 1) * w as usize].to_vec();
        row.resize(stride, 0);
        dib.extend(row);
    }
    let ms = (((w + 31) / 32) * 4) as usize;
    for y in (0..h as usize).rev() {
        let mut row = vec![0u8; ms];
        for x in 0..w as usize {
            if mask[y * w as usize + x] != 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        dib.extend(row);
    }
    wrap_cur(w, h, (0, 0), dib)
}

fn ani_from(frames: &[Vec<u8>], jif: u32, rates: Option<&[u32]>, seq: Option<&[u32]>) -> Vec<u8> {
    let chunk = |id: &[u8; 4], body: &[u8]| {
        let mut c = id.to_vec();
        c.extend_from_slice(&(body.len() as u32).to_le_bytes());
        c.extend_from_slice(body);
        if body.len() % 2 == 1 { c.push(0); }
        c
    };
    let nsteps = seq.map(|s| s.len()).unwrap_or(frames.len()) as u32;
    let mut anih = Vec::new();
    for v in [36u32, frames.len() as u32, nsteps, 0, 0, 0, 0, jif, if seq.is_some() { 3 } else { 1 }] {
        anih.extend_from_slice(&v.to_le_bytes());
    }
    let mut body = b"ACON".to_vec();
    body.extend(chunk(b"anih", &anih));
    if let Some(r) = rates {
        body.extend(chunk(b"rate", &r.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()));
    }
    if let Some(s) = seq {
        body.extend(chunk(b"seq ", &s.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()));
    }
    let mut list = b"fram".to_vec();
    for f in frames { list.extend(chunk(b"icon", f)); }
    body.extend(chunk(b"LIST", &list));
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

fn native() -> Options {
    Options { sizes: vec![], ..Options::default() }
}

#[test]
fn alpha_cursor_is_exact_at_native_size() {
    // 4x4 with varied alpha incl. semi-transparent
    let mut px = vec![];
    for i in 0..16u8 {
        px.extend_from_slice(&[i * 16, 255 - i * 8, i * 3, if i % 4 == 0 { 0 } else { 255 - i * 10 }]);
    }
    let a = ani::load(&cur32(4, 4, &px, (1, 2)), InvertMode::Black).unwrap();
    let xc = convert::to_xcursor(&a, &native()).unwrap();
    assert_eq!(xc.len(), 1);
    let x = &xc[0];
    assert_eq!((x.nominal, x.width, x.height, x.hot_x, x.hot_y, x.delay), (4, 4, 4, 1, 2, 0));
    let back = xcursor::unpremultiply(&x.argb);
    for (i, (b, o)) in back.chunks(4).zip(px.chunks(4)).enumerate() {
        assert_eq!(b[3], o[3], "alpha px {}", i);
        if o[3] == 255 {
            assert_eq!(b, o, "opaque px {}", i);
        }
    }
    // file round trip
    let again = xcursor::read(&xcursor::write(&xc)).unwrap();
    assert_eq!(again[0].argb, x.argb);
}

#[test]
fn ani_sequence_and_rates() {
    let f = |c: u8| cur32(2, 2, &[c, 0, 0, 255].repeat(4), (0, 0));
    let a = ani_from(&[f(10), f(20), f(30)], 6, Some(&[6, 12, 30, 60]), Some(&[0, 1, 2, 1]));
    let anim = ani::load(&a, InvertMode::Black).unwrap();
    assert_eq!(anim.steps.len(), 4);
    let xc = convert::to_xcursor(&anim, &native()).unwrap();
    assert_eq!(xc.iter().map(|i| i.delay).collect::<Vec<_>>(), vec![100, 200, 500, 1000]);
    assert_eq!(xc.iter().map(|i| xcursor::unpremultiply(&i.argb)[0]).collect::<Vec<_>>(), vec![10, 20, 30, 20]);
}

#[test]
fn jiffy_rounding() {
    assert_eq!(convert::jiffies_to_ms(5), 83);
    assert_eq!(convert::jiffies_to_ms(1), 17);
    assert_eq!(convert::jiffies_to_ms(60), 1000);
}

#[test]
fn and_mask_transparency_and_inversion() {
    // idx: black white black white ; mask: opaque, opaque, transparent(black=AND1,XOR0), invert(white,AND1)
    let c = cur8(4, 1, &[0, 1, 0, 1], &[0, 0, 1, 1]);
    let anim = ani::load(&c, InvertMode::Black).unwrap();
    let im = &anim.steps[0].images[0];
    assert_eq!(&im.image.rgba[0..4], &[0, 0, 0, 255]);
    assert_eq!(&im.image.rgba[4..8], &[255, 255, 255, 255]);
    assert_eq!(im.image.rgba[11], 0); // transparent
    assert_eq!(&im.image.rgba[12..16], &[0, 0, 0, 255]); // inverted -> black
    assert_eq!(im.inverted, 1);
    let w = ani::load(&c, InvertMode::White).unwrap();
    assert_eq!(&w.steps[0].images[0].image.rgba[12..16], &[255, 255, 255, 255]);
}

#[test]
fn pixel_art_upscale_is_exact_blocks_and_hotspot_scales() {
    let px: Vec<u8> = (0..16u8).flat_map(|i| [i * 15, 0, 255 - i * 15, 255]).collect();
    let a = ani::load(&cur32(4, 4, &px, (1, 3)), InvertMode::Black).unwrap();
    let o = Options { sizes: vec![8, 12], filter: FilterChoice::Auto, ..Options::default() };
    let xc = convert::to_xcursor(&a, &o).unwrap();
    let x8 = xc.iter().find(|i| i.nominal == 8).unwrap();
    assert_eq!((x8.width, x8.height, x8.hot_x, x8.hot_y), (8, 8, 2, 6));
    let back = xcursor::unpremultiply(&x8.argb);
    for y in 0..8usize {
        for x in 0..8usize {
            assert_eq!(&back[(y * 8 + x) * 4..][..4], &px[((y / 2) * 4 + x / 2) * 4..][..4]);
        }
    }
    // sizes are grouped ascending, nominal 4 (native) present too
    assert_eq!(xc.iter().map(|i| i.nominal).collect::<Vec<_>>(), vec![4, 8, 12]);
}

#[test]
fn near_standard_native_size_is_not_resampled() {
    // 32x33 (like Person.ani): must be stored untouched under nominal 32
    let px = vec![200u8, 10, 10, 255].repeat(32 * 33);
    let a = ani::load(&cur32(32, 33, &px, (0, 0)), InvertMode::Black).unwrap();
    let xc = convert::to_xcursor(&a, &Options::default()).unwrap();
    let n32 = xc.iter().find(|i| i.nominal == 32).unwrap();
    assert_eq!((n32.width, n32.height), (32, 33));
    assert!(!xc.iter().any(|i| i.nominal == 33));
}

#[test]
fn png_roundtrip_and_stored_inflate() {
    let mut img = Image::new(3, 2);
    for (i, v) in img.rgba.iter_mut().enumerate() { *v = (i * 7) as u8; }
    let enc = png::encode(&img);
    assert_eq!(png::decode(&enc).unwrap(), img);
    assert_eq!(inflate::zlib(&[0x78, 0x01, 1, 3, 0, 0xfc, 0xff, b'a', b'b', b'c', 0, 0, 0, 0]).unwrap(), b"abc");
}

/// Optional: run against the real pack: WIN2XCUR_TEST_PACK=/path/to/dir
#[test]
fn real_pack_matches_source_pixels() {
    let Ok(dir) = std::env::var("WIN2XCUR_TEST_PACK") else { return };
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("ani")).unwrap_or(false) {
            let anim = ani::load(&std::fs::read(&p).unwrap(), InvertMode::Black).unwrap();
            let xc = xcursor::read(&xcursor::write(&convert::to_xcursor(&anim, &Options::default()).unwrap())).unwrap();
            let nat = anim.steps[0].images[0].dim();
            let nom = if (nat as i64 - 32).abs() * 16 <= 32 { 32 } else { nat };
            let frames: Vec<_> = xc.iter().filter(|i| i.nominal == nom).collect();
            assert_eq!(frames.len(), anim.steps.len());
            for (f, s) in frames.iter().zip(&anim.steps) {
                let src = &s.images[0];
                assert_eq!((f.width, f.height, f.hot_x, f.hot_y), (src.image.w, src.image.h, src.hot_x, src.hot_y), "{:?}", p);
                assert_eq!(f.delay, 83);
                let back = xcursor::unpremultiply(&f.argb);
                for (b, o) in back.chunks(4).zip(src.image.rgba.chunks(4)) {
                    assert_eq!(b[3], o[3]);
                    if o[3] == 255 { assert_eq!(b, o); }
                }
            }
            n += 1;
        }
    }
    assert!(n > 0);
}
