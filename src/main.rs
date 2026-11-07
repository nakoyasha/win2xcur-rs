use std::path::{Path, PathBuf};
use win2xcur::ani;
use win2xcur::convert::{self, FilterChoice, Options};
use win2xcur::dib::InvertMode;
use win2xcur::error::{Error, Result};
use win2xcur::image::Image;
use win2xcur::render::{contact_sheet, Cell};
use win2xcur::{bail, png, theme, xcursor};

const HELP: &str = "\
win2xcur - pixel-exact Windows cursor (.cur/.ani) to Xcursor converter

USAGE
  win2xcur convert <files...> -o <dir> [options]   convert .cur/.ani files to Xcursor files
  win2xcur theme <dir|install.inf> -o <dir> [options]
                                                   convert a whole Windows cursor scheme into an
                                                   installable Xcursor theme (names + symlinks)
  win2xcur inspect <files...>                      print sizes, hotspots and timing
  win2xcur render <file> -o <sheet.png> [--size N] [--zoom K]
                                                   draw all frames of a cursor to a PNG (debug)

OPTIONS
  -o, --output <path>     output directory (convert/theme) or PNG file (render)
      --sizes <list>      extra sizes to generate, comma separated   [default: 32,48,64,80,96,112,128]
      --native-only       only emit the sizes stored in the file (no scaled copies)
      --filter <f>        auto | nearest | bilinear | lanczos        [default: auto]
                          auto = exact pixel blocks for pixel-art, Lanczos for smooth cursors
      --invert <mode>     black | white | transparent                [default: black]
                          X11 cannot invert the screen; this is what 'inverted' pixels become
      --shadow            add an approximation of Windows' pointer shadow
      --name <name>       theme name (theme command)
";

struct Args {
    pos: Vec<String>,
    out: Option<PathBuf>,
    opts: Options,
    name: Option<String>,
    size: Option<u32>,
    zoom: Option<u32>,
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut a = Args { pos: vec![], out: None, opts: Options::default(), name: None, size: None, zoom: None };
    let mut i = 0;
    while i < args.len() {
        let (key, inline) = match args[i].split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (args[i].clone(), None),
        };
        let val = |i: &mut usize| -> Result<String> {
            if let Some(v) = inline.clone() {
                return Ok(v);
            }
            *i += 1;
            match args.get(*i) {
                Some(v) => Ok(v.clone()),
                None => Err(Error(format!("{} needs a value", key))),
            }
        };
        match key.as_str() {
            "-o" | "--output" => a.out = Some(PathBuf::from(val(&mut i)?)),
            "--sizes" => {
                a.opts.sizes = val(&mut i)?
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().parse::<u32>().map_err(|_| Error(format!("bad size '{}'", s))))
                    .collect::<Result<_>>()?;
            }
            "--native-only" => a.opts.sizes.clear(),
            "--filter" => {
                a.opts.filter = match val(&mut i)?.as_str() {
                    "auto" => FilterChoice::Auto,
                    "nearest" => FilterChoice::Nearest,
                    "bilinear" => FilterChoice::Bilinear,
                    "lanczos" => FilterChoice::Lanczos,
                    o => bail!("unknown filter '{}'", o),
                }
            }
            "--invert" => {
                a.opts.invert = match val(&mut i)?.as_str() {
                    "black" => InvertMode::Black,
                    "white" => InvertMode::White,
                    "transparent" => InvertMode::Transparent,
                    o => bail!("unknown invert mode '{}'", o),
                }
            }
            "--shadow" => a.opts.shadow = true,
            "--name" => a.name = Some(val(&mut i)?),
            "--size" => a.size = Some(val(&mut i)?.parse().map_err(|_| Error("bad --size".into()))?),
            "--zoom" => a.zoom = Some(val(&mut i)?.parse().map_err(|_| Error("bad --zoom".into()))?),
            k if k.starts_with('-') && k.len() > 1 => bail!("unknown option '{}'", k),
            _ => a.pos.push(args[i].clone()),
        }
        i += 1;
    }
    Ok(a)
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "cursor".into())
}

fn convert_file(path: &Path, opts: &Options) -> Result<(Vec<u8>, usize, usize, usize)> {
    let data = std::fs::read(path)?;
    let anim = ani::load(&data, opts.invert).map_err(|e| Error(format!("{}: {}", path.display(), e)))?;
    let inverted: usize = anim.steps.iter().flat_map(|s| &s.images).map(|i| i.inverted).max().unwrap_or(0);
    let images = convert::to_xcursor(&anim, opts)?;
    let sizes = images.iter().map(|i| i.nominal).collect::<std::collections::BTreeSet<_>>().len();
    Ok((xcursor::write(&images), sizes, anim.steps.len(), inverted))
}

fn cmd_convert(a: Args) -> Result<()> {
    if a.pos.is_empty() {
        bail!("no input files");
    }
    let out = a.out.clone().unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&out)?;
    for f in &a.pos {
        let p = Path::new(f);
        let (bytes, sizes, steps, inv) = convert_file(p, &a.opts)?;
        let dest = out.join(stem(p));
        std::fs::write(&dest, bytes)?;
        println!("{} -> {} ({} sizes x {} frames)", p.display(), dest.display(), sizes, steps);
        if inv > 0 {
            eprintln!("  note: {} 'invert screen' pixels were approximated (--invert {:?})", inv, a.opts.invert);
        }
    }
    Ok(())
}

fn cmd_theme(a: Args) -> Result<()> {
    let Some(src) = a.pos.first() else { bail!("give a directory or install.inf") };
    let src = Path::new(src);
    let inf = if src.is_dir() {
        std::fs::read_dir(src)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("inf")).unwrap_or(false))
            .ok_or_else(|| Error(format!("no .inf file in {}", src.display())))?
    } else {
        src.to_path_buf()
    };
    let scheme = theme::parse_inf(&inf)?;
    for w in &scheme.warnings {
        eprintln!("warning: {}", w);
    }
    let name = a.name.clone().unwrap_or(scheme.name.clone());
    let root = a.out.clone().unwrap_or_else(|| PathBuf::from(".")).join(&name);
    let cdir = root.join("cursors");
    std::fs::create_dir_all(&cdir)?;
    for (file, names) in &scheme.files {
        let (bytes, sizes, steps, inv) = convert_file(file, &a.opts)?;
        let primary = &names[0];
        std::fs::write(cdir.join(primary), bytes)?;
        for alias in &names[1..] {
            let dest = cdir.join(alias);
            let _ = std::fs::remove_file(&dest);
            #[cfg(unix)]
            std::os::unix::fs::symlink(primary, &dest)?;
            #[cfg(not(unix))]
            std::fs::copy(cdir.join(primary), &dest).map(|_| ())?;
        }
        println!("{:<18} -> {:<14} (+{} aliases, {} sizes x {} frames)", file.file_name().unwrap().to_string_lossy(), primary, names.len() - 1, sizes, steps);
        if inv > 0 {
            eprintln!("  note: {} 'invert screen' pixels approximated", inv);
        }
    }
    std::fs::write(root.join("index.theme"), format!("[Icon Theme]\nName={}\nComment=Converted from a Windows cursor scheme by win2xcur\n", name))?;
    println!("theme written to {}", root.display());
    Ok(())
}

fn cmd_inspect(a: Args) -> Result<()> {
    for f in &a.pos {
        let d = std::fs::read(f)?;
        println!("{}", f);
        if xcursor::is_xcursor(&d) {
            let imgs = xcursor::read(&d)?;
            let mut sizes: Vec<u32> = imgs.iter().map(|i| i.nominal).collect();
            sizes.dedup();
            for s in sizes {
                let v: Vec<_> = imgs.iter().filter(|i| i.nominal == s).collect();
                let tot: u32 = v.iter().map(|i| i.delay).sum();
                println!("  nominal {:>3}: {} frame(s) {}x{} hotspot ({},{}) total {} ms", s, v.len(), v[0].width, v[0].height, v[0].hot_x, v[0].hot_y, tot);
            }
            continue;
        }
        let anim = ani::load(&d, InvertMode::Black)?;
        if let Some(t) = &anim.title { println!("  title: {}", t); }
        if let Some(t) = &anim.author { println!("  author: {}", t); }
        let total: u32 = anim.steps.iter().map(|s| convert::jiffies_to_ms(s.jiffies)).sum();
        println!("  {} step(s), {} ms per loop", anim.steps.len(), total);
        for (i, s) in anim.steps.iter().enumerate() {
            let imgs: Vec<String> = s.images.iter().map(|m| format!("{}x{} hot({},{}){}", m.image.w, m.image.h, m.hot_x, m.hot_y, if m.inverted > 0 { format!(" inv={}", m.inverted) } else { String::new() })).collect();
            println!("  step {:>2}: {} jiffies ({} ms)  {}", i, s.jiffies, convert::jiffies_to_ms(s.jiffies), imgs.join(", "));
        }
    }
    Ok(())
}

fn cmd_render(a: Args) -> Result<()> {
    let (Some(f), Some(out)) = (a.pos.first(), a.out.as_ref()) else { bail!("usage: render <file> -o <sheet.png>") };
    let d = std::fs::read(f)?;
    let mut owned: Vec<(Image, u32, u32)> = vec![];
    if xcursor::is_xcursor(&d) {
        let imgs = xcursor::read(&d)?;
        let want = a.size.unwrap_or(32);
        let best = imgs.iter().map(|i| i.nominal).min_by_key(|&n| (n as i64 - want as i64).abs()).ok_or_else(|| Error("empty cursor".into()))?;
        for i in imgs.iter().filter(|i| i.nominal == best) {
            owned.push((Image { w: i.width, h: i.height, rgba: xcursor::unpremultiply(&i.argb) }, i.hot_x, i.hot_y));
        }
        println!("rendering nominal size {}", best);
    } else {
        let anim = ani::load(&d, a.opts.invert)?;
        for s in &anim.steps {
            let m = match a.size {
                Some(n) => s.images.iter().min_by_key(|m| (m.dim() as i64 - n as i64).abs()).unwrap(),
                None => s.images.iter().max_by_key(|m| m.dim()).unwrap(),
            };
            owned.push((m.image.clone(), m.hot_x, m.hot_y));
        }
    }
    let maxd = owned.iter().map(|(i, _, _)| i.w.max(i.h)).max().unwrap_or(32);
    let zoom = a.zoom.unwrap_or((160 / maxd).max(1));
    let cells: Vec<Cell> = owned.iter().map(|(i, x, y)| Cell { image: i, hot_x: *x, hot_y: *y }).collect();
    let sheet = contact_sheet(&cells, zoom, 6);
    std::fs::write(out, png::encode(&sheet))?;
    println!("{} frame(s) at {}x zoom -> {}", owned.len(), zoom, out.display());
    Ok(())
}

fn run(args: Vec<String>) -> Result<()> {
    match args.first().map(|s| s.as_str()) {
        None | Some("-h") | Some("--help") | Some("help") => {
            print!("{}", HELP);
            Ok(())
        }
        Some("-V") | Some("--version") => {
            println!("win2xcur {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("convert") => cmd_convert(parse_args(&args[1..])?),
        Some("theme") => cmd_theme(parse_args(&args[1..])?),
        Some("inspect") => cmd_inspect(parse_args(&args[1..])?),
        Some("render") => cmd_render(parse_args(&args[1..])?),
        Some(_) => cmd_convert(parse_args(&args)?), // original-style: win2xcur files... -o dir
    }
}

fn main() {
    if let Err(e) = run(std::env::args().skip(1).collect()) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
