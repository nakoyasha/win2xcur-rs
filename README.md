# win2xcur (Rust)

Pixel-exact converter from Windows cursors (`.cur`, `.ani`) to Xcursor. Single static binary, **zero dependencies**
(own DEFLATE/PNG decoder, so PNG-compressed Vista-style cursors work too).

```
cargo build --release
win2xcur theme  <pack-dir|install.inf> -o ~/.icons          # whole scheme -> installable theme
win2xcur convert Normal.ani Text.cur -o out/                 # individual files
win2xcur inspect Normal.ani                                  # sizes, hotspots, timing
win2xcur render  out/left_ptr -o sheet.png --size 48         # contact sheet of frames (debug)
```

## What "1:1" means here
* **Pixels untouched at native size.** The file's own bitmap is stored unscaled (8/4/1/16/24/32-bit DIB, PNG).
  Transparency follows Windows' rules: 32-bit alpha if any alpha is non-zero, otherwise the AND mask.
  Straight alpha is premultiplied exactly once, as Xcursor requires.
* **Same on-screen size.** Xcursor shows an image at its real pixel size, so a 32 px Windows cursor is 32 px.
  Cursors within ~6% of a standard size (e.g. 32x33) are filed under that size with their pixels untouched.
* **Windows' size ladder.** Extra nominal sizes 32,48,...,128 (the cursor-size slider / 100-400% DPI steps) are
  generated so whatever `XCURSOR_SIZE` the desktop uses, the closest size exists. `--sizes` / `--native-only` change this.
* **Pixel art stays pixel art.** `--filter auto` enlarges hard-edged cursors with centre-sampled nearest neighbour
  (exact k x k blocks for integer factors), and uses Lanczos for smooth/antialiased ones.
* **Timing & order.** `rate`/`seq` chunks honoured; jiffies (1/60 s) -> ms, rounded.
* **Hotspots** are kept (scaled proportionally for generated sizes).
* **Inverted pixels** (AND=1, XOR!=0, e.g. classic I-beam) can't exist in X11; they become black (`--invert white|transparent`).
* `theme` reads `install.inf` (UTF-8/UTF-16), maps registry roles (Arrow, Wait, SizeNWSE, ...) to Xcursor names and
  legacy hash names, and writes symlinks + `index.theme`.

## Tests
`cargo test --release` (synthetic fixtures), and against a real pack:
`WIN2XCUR_TEST_PACK=/path/to/cursors cargo test --release`.

## Not covered
RLE-compressed DIBs, interlaced PNGs, OS/2 headers. The Windows pointer *shadow* is a system effect rather than part
of the file; `--shadow` is an approximation.
