//! Windows cursor scheme (install.inf) -> Xcursor theme layout.
use crate::bail;
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Windows registry cursor role -> Xcursor names (first is the real file, the rest are symlinks).
pub const ROLES: &[(&str, &[&str])] = &[
    ("Arrow", &["left_ptr", "default", "arrow", "top_left_arrow"]),
    ("Help", &["help", "question_arrow", "whats_this", "left_ptr_help"]),
    ("AppStarting", &["progress", "left_ptr_watch", "half-busy", "08e8e1c95fe2fc01f976f1e063a24ccd", "3ecb610c1bf2410f44200f48c40d3599"]),
    ("Wait", &["wait", "watch", "clock"]),
    ("Crosshair", &["crosshair", "cross", "tcross", "cell"]),
    ("PrecisionHair", &["crosshair", "cross", "tcross", "cell"]),
    ("IBeam", &["text", "xterm", "ibeam"]),
    ("NWPen", &["pencil", "draft"]),
    ("No", &["not-allowed", "crossed_circle", "forbidden", "no-drop", "dnd-no-drop", "circle", "03b6e0fcb3499374a867c041f52298f0"]),
    ("SizeNS", &["ns-resize", "size_ver", "sb_v_double_arrow", "v_double_arrow", "row-resize", "split_v", "n-resize", "s-resize", "top_side", "bottom_side", "double_arrow", "00008160000006810000408080010102", "2870a09082c103050810ffdffffe0204"]),
    ("SizeWE", &["ew-resize", "size_hor", "sb_h_double_arrow", "h_double_arrow", "col-resize", "split_h", "e-resize", "w-resize", "left_side", "right_side", "14fef782d02440884392942c11205230", "043a9f68147c53184671403ffa811cc5"]),
    ("SizeNWSE", &["nwse-resize", "size_fdiag", "bd_double_arrow", "nw-resize", "se-resize", "top_left_corner", "bottom_right_corner", "c7088f0f3e6c8088236ef8e1e3e70000", "38c5dff7c7b8962045400281044508d2"]),
    ("SizeNESW", &["nesw-resize", "size_bdiag", "fd_double_arrow", "ne-resize", "sw-resize", "top_right_corner", "bottom_left_corner", "fcf1c3c7cd4491d801f1e1c78f100000", "50585d75b494802d0151028115016902"]),
    ("SizeAll", &["move", "fleur", "all-scroll", "size_all", "4498f0e0c1937ffe01fd06f973665830", "9081237383d90e509aa00f00170e968f"]),
    ("UpArrow", &["up-arrow", "center_ptr"]),
    ("Hand", &["pointer", "hand1", "hand2", "pointing_hand", "9d800788f1b08800ae810202380a0822", "e29285e634086352946a0e7090d73106"]),
    ("Pin", &["pin"]),
    ("Person", &["person"]),
];

pub struct Scheme {
    pub name: String,
    /// (file path, xcursor names with the primary first)
    pub files: Vec<(PathBuf, Vec<String>)>,
    pub warnings: Vec<String>,
}

fn split_fields(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut q) = (vec![], String::new(), false);
    for ch in line.chars() {
        match ch {
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(ch),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_lowercase();
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().map(|f| f.to_string_lossy().to_lowercase() == want).unwrap_or(false)
    })
}

pub fn parse_inf(inf: &Path) -> Result<Scheme> {
    let raw = std::fs::read(inf)?;
    // INF files are frequently UTF-16LE with BOM or legacy ANSI.
    let text = if raw.starts_with(&[0xff, 0xfe]) {
        let u: Vec<u16> = raw[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&raw).into_owned()
    };
    let dir = inf.parent().unwrap_or(Path::new("."));
    let (mut strings, mut section) = (HashMap::<String, String>::new(), String::new());
    let mut lines = Vec::new();
    for l in text.lines() {
        let l = l.trim().trim_start_matches('\u{feff}');
        if l.is_empty() || l.starts_with(';') {
            continue;
        }
        if l.starts_with('[') {
            section = l.trim_matches(|c| c == '[' || c == ']').to_lowercase();
            continue;
        }
        if section == "strings" {
            if let Some((k, v)) = l.split_once('=') {
                strings.insert(k.trim().to_lowercase(), v.trim().trim_matches('"').to_string());
            }
        } else {
            lines.push(l.to_string());
        }
    }
    let subst = |s: &str| -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('%') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            match after.find('%') {
                Some(j) => {
                    out.push_str(strings.get(&after[..j].to_lowercase()).map(|s| s.as_str()).unwrap_or(""));
                    rest = &after[j + 1..];
                }
                None => {
                    out.push_str(&rest[i..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    };

    let mut by_file: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut warnings = Vec::new();
    let mut found_any = false;
    for l in &lines {
        let f = split_fields(l);
        if f.len() < 5 || !f[1].eq_ignore_ascii_case("Control Panel\\Cursors") || f[2].is_empty() {
            continue;
        }
        found_any = true;
        let val = subst(&f[4]);
        let base = val.rsplit(|c| c == '\\' || c == '/').next().unwrap_or("").trim().trim_matches('"').to_string();
        let role = f[2].to_lowercase();
        let Some((_, names)) = ROLES.iter().find(|(r, _)| r.to_lowercase() == role) else {
            warnings.push(format!("unknown cursor role '{}' ignored", f[2]));
            continue;
        };
        let Some(path) = find_ci(dir, &base) else {
            warnings.push(format!("role {} refers to missing file '{}'", f[2], base));
            continue;
        };
        match by_file.iter_mut().find(|(p, _)| *p == path) {
            Some((_, v)) => {
                for n in names.iter() {
                    if !v.iter().any(|x| x == n) {
                        v.push(n.to_string());
                    }
                }
            }
            None => by_file.push((path, names.iter().map(|s| s.to_string()).collect())),
        }
    }
    if !found_any {
        bail!("no 'Control Panel\\Cursors' registry entries found in {}", inf.display());
    }
    let name = strings
        .get("scheme_name")
        .or_else(|| strings.get("scheme"))
        .cloned()
        .unwrap_or_else(|| dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Windows Cursors".into()));
    Ok(Scheme { name, files: by_file, warnings })
}
