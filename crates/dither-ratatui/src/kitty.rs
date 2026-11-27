//! Kitty graphics: dither dots as real, round, anti-aliased image pixels.
//!
//! The grid is drawn into an RGBA image at the terminal's true cell pixel
//! size, every dot a soft-edged disc with a gap around it, then sent with
//! the kitty graphics protocol and shown through
//! [Unicode placeholders](https://sw.kovidgoyal.net/kitty/graphics-protocol/#unicode-placeholders):
//! each covered cell holds `U+10EEEE` plus row/column diacritics, with the
//! image id as its foreground colour. Because the image lives in ordinary
//! cells, Ratatui's diffing, clearing and overdrawing all just work: text
//! drawn over a placeholder cell replaces that part of the image.
//!
//! The encoded image is cached per placement and rebuilt only when the grid
//! changes, so static charts cost nothing per frame.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU16;
use std::rc::Rc;

use dither_core::PixelGrid;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::style::Color;

/// Kitty graphics options: the terminal's cell size in pixels and how many
/// dither dots fit across one cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kitty {
    /// Cell size in device pixels, `(width, height)`.
    pub cell: (u16, u16),
    /// Dots across one cell; rows follow from the cell's aspect ratio so
    /// dots sit on a square pitch.
    pub dots_across: u16,
    /// Dot diameter as a fraction of the dot pitch (0..1). Lower values
    /// leave bigger gaps.
    pub dot_size: f32,
}

/// The character kitty reserves for image placeholders.
const PLACEHOLDER: char = '\u{10EEEE}';

/// Cell size assumed when the terminal does not report one.
const FALLBACK_CELL: (u16, u16) = (10, 20);

impl Kitty {
    /// Options for a terminal whose cells are `cell_w`×`cell_h` pixels.
    pub fn new(cell_w: u16, cell_h: u16) -> Self {
        Self {
            cell: (cell_w.max(1), cell_h.max(1)),
            dots_across: 2,
            dot_size: 0.72,
        }
    }

    /// Dots across one cell (default 2). More dots means finer detail.
    pub fn dots_across(mut self, dots: u16) -> Self {
        self.dots_across = dots.max(1);
        self
    }

    /// Dot diameter as a fraction of the pitch (default 0.72).
    pub fn dot_size(mut self, fraction: f32) -> Self {
        self.dot_size = fraction.clamp(0.1, 1.0);
        self
    }

    /// Kitty options when the terminal can show them, else `None`.
    ///
    /// `DITHER_RASTER=kitty|braille|halfblock` forces a choice. Otherwise
    /// kitty and Ghostty are detected from the environment (not inside tmux,
    /// which needs passthrough), and the cell size comes from the tty.
    pub fn detect() -> Option<Self> {
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        match env("DITHER_RASTER").to_ascii_lowercase().as_str() {
            "kitty" => return Some(Self::assumed()),
            "braille" | "halfblock" | "half-block" => return None,
            _ => {}
        }
        if !env("TMUX").is_empty() {
            return None;
        }
        let term = env("TERM");
        let supported = term.contains("kitty")
            || term.contains("ghostty")
            || !env("KITTY_WINDOW_ID").is_empty()
            || env("TERM_PROGRAM").eq_ignore_ascii_case("ghostty");
        if !supported {
            return None;
        }
        Self::cell_size().map(|(w, h)| Self::new(w, h))
    }

    /// Kitty options for this tty's cell size, or a 10×20 cell when the
    /// terminal does not report one. For forcing kitty graphics on.
    pub fn assumed() -> Self {
        Self::cell_size().map_or(Self::new(FALLBACK_CELL.0, FALLBACK_CELL.1), |(w, h)| {
            Self::new(w, h)
        })
    }

    #[cfg(feature = "crossterm")]
    fn cell_size() -> Option<(u16, u16)> {
        let s = ratatui::crossterm::terminal::window_size().ok()?;
        if s.width == 0 || s.height == 0 || s.columns == 0 || s.rows == 0 {
            return None;
        }
        Some((s.width / s.columns, s.height / s.rows))
    }

    #[cfg(not(feature = "crossterm"))]
    fn cell_size() -> Option<(u16, u16)> {
        None
    }

    /// Dither dots per cell, `(columns, rows)`.
    pub fn dots_per_cell(self) -> (usize, usize) {
        let across = self.dots_across.max(1) as f32;
        let pitch = self.cell.0 as f32 / across;
        let down = (self.cell.1 as f32 / pitch).round().max(1.0);
        (across as usize, down as usize)
    }

    /// Draws `grid` as round dots into a straight-alpha RGBA image covering
    /// `cols`×`rows` cells. Returns `(width, height, pixels)`.
    pub fn image(self, grid: &PixelGrid, cols: u16, rows: u16) -> (u32, u32, Vec<u8>) {
        let (cw, ch) = (self.cell.0 as usize, self.cell.1 as usize);
        let (w, h) = (cols as usize * cw, rows as usize * ch);
        let mut img = vec![0u8; w * h * 4];
        let (sx, sy) = self.dots_per_cell();
        let (px, py) = (cw as f32 / sx as f32, ch as f32 / sy as f32);
        let radius = 0.5 * self.dot_size * px.min(py);
        for gy in 0..(rows as usize * sy).min(grid.height()) {
            for gx in 0..(cols as usize * sx).min(grid.width()) {
                let Some(c) = grid.get(gx, gy) else { continue };
                if c.a == 0 {
                    continue;
                }
                let (cx, cy) = ((gx as f32 + 0.5) * px, (gy as f32 + 0.5) * py);
                let x0 = (cx - radius - 1.0).floor().max(0.0) as usize;
                let y0 = (cy - radius - 1.0).floor().max(0.0) as usize;
                let x1 = ((cx + radius + 1.0).ceil() as usize).min(w);
                let y1 = ((cy + radius + 1.0).ceil() as usize).min(h);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                        let cover = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                        if cover <= 0.0 {
                            continue;
                        }
                        let i = (y * w + x) * 4;
                        let a = (c.a as f32 * cover).round() as u8;
                        if a > img[i + 3] {
                            img[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, a]);
                        }
                    }
                }
            }
        }
        (w as u32, h as u32, img)
    }

    pub(crate) fn paint(self, grid: &PixelGrid, area: Rect, buf: &mut Buffer) {
        // The image always covers the whole requested area, so placeholder
        // rows and columns stay relative to its origin when it is clipped.
        let max = DIACRITICS.len() as u16;
        let full = Rect::new(area.x, area.y, area.width.min(max), area.height.min(max));
        let (visible, ox, oy) = crate::raster::clip(full, buf);
        if visible.is_empty() {
            return;
        }
        let (sx, sy) = self.dots_per_cell();
        let lit = |c: u16, r: u16| {
            (0..sy).any(|dy| {
                (0..sx).any(|dx| {
                    grid.get(c as usize * sx + dx, r as usize * sy + dy)
                        .is_some_and(|p| p.a > 0)
                })
            })
        };
        let cells: Vec<(u16, u16)> = (oy..oy + visible.height)
            .flat_map(|r| (ox..ox + visible.width).map(move |c| (c, r)))
            .filter(|&(c, r)| lit(c, r))
            .collect();
        let Some(&last) = cells.last() else { return };

        let id = image_id(full);
        let (transmit, fresh) = self.transmission(grid, id, full.width, full.height);
        // After the app has called `flush` once, uploads leave out of band
        // and no cell carries them, so nothing drawn on top can drop one.
        let embed = !OUT_OF_BAND.with(Cell::get);
        if !embed && fresh {
            PENDING.with(|p| p.borrow_mut().push(transmit.clone()));
        }
        let fg = Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8);
        let mut symbol = String::with_capacity(16);
        for (c, r) in cells {
            let Some(cell) = buf.cell_mut((full.x + c, full.y + r)) else {
                continue;
            };
            symbol.clear();
            // Without `flush`, the image data rides along in the last placed
            // cell; Ratatui only re-sends it when it changes.
            if embed && (c, r) == last {
                symbol.push_str(&transmit);
            }
            symbol.push(PLACEHOLDER);
            symbol.push(diacritic(r));
            symbol.push(diacritic(c));
            // Transparent image pixels show the cell background, so keep the
            // surface colour; everything else is reset so no modifier changes
            // how the placeholder is read.
            let bg = cell.bg;
            cell.reset();
            cell.set_symbol(&symbol)
                .set_fg(fg)
                .set_bg(bg)
                .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MIN));
        }
    }

    /// The escape sequence that uploads the image and creates its virtual
    /// placement, cached until the picture changes.
    /// Also says whether it was just built (the picture changed).
    fn transmission(self, grid: &PixelGrid, id: u32, cols: u16, rows: u16) -> (Rc<String>, bool) {
        let mut h = DefaultHasher::new();
        (cols, rows, self.cell, self.dots_across, self.dot_size.to_bits()).hash(&mut h);
        (grid.width(), grid.height()).hash(&mut h);
        for c in grid.cells() {
            c.map(|c| (c.r, c.g, c.b, c.a)).hash(&mut h);
        }
        let key = h.finish();
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some((k, s)) = cache.get(&id)
                && *k == key
            {
                return (s.clone(), false);
            }
            let (w, hpx, img) = self.image(grid, cols, rows);
            let s = Rc::new(encode(&img, w, hpx, id, cols, rows));
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(id, (key, s.clone()));
            (s, true)
        })
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<u32, (u64, Rc<String>)>> = RefCell::new(HashMap::new());
    static PENDING: RefCell<Vec<Rc<String>>> = const { RefCell::new(Vec::new()) };
    static OUT_OF_BAND: Cell<bool> = const { Cell::new(false) };
}

/// Sends image uploads straight to the terminal instead of inside a cell.
/// Call it after every `terminal.draw(..)`, on the thread that draws:
///
/// ```no_run
/// # fn run(terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
/// terminal.draw(|frame| { /* render dither widgets */ })?;
/// dither_ratatui::kitty::flush(&mut std::io::stdout())?;
/// # Ok(()) }
/// ```
///
/// Without it, each picture's upload travels in one of its own placeholder
/// cells, which works until something is drawn over that exact cell (a
/// tooltip, a popup): then the upload is lost and the picture stays blank
/// until it changes or the cell is uncovered. The first call switches every
/// later frame to out-of-band uploads, which nothing on screen can cover.
pub fn flush(out: &mut impl std::io::Write) -> std::io::Result<()> {
    OUT_OF_BAND.with(|o| o.set(true));
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    if pending.is_empty() {
        return Ok(());
    }
    for upload in pending {
        out.write_all(upload.as_bytes())?;
    }
    out.flush()
}

/// A stable 24-bit image id for a placement area (never 0).
fn image_id(area: Rect) -> u32 {
    let mut h = DefaultHasher::new();
    ("dither-kitty", area.x, area.y, area.width, area.height).hash(&mut h);
    let id = (h.finish() as u32) & 0x00ff_ffff;
    if id == 0 { 1 } else { id }
}

/// Zlib-compressed RGBA, base64'd and split into 4096-byte APC chunks.
fn encode(rgba: &[u8], w: u32, h: u32, id: u32, cols: u16, rows: u16) -> String {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(rgba, 1);
    let b64 = base64(&z);
    let chunks: Vec<&[u8]> = b64.as_bytes().chunks(4096).collect();
    let mut out = String::with_capacity(b64.len() + chunks.len() * 16 + 96);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,U=1,q=2,i={id},f=32,o=z,s={w},v={h},c={cols},r={rows},m={more};"
            ));
        } else {
            out.push_str(&format!("\x1b_Gq=2,m={more};"));
        }
        // Base64 output is ASCII.
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str("\x1b\\");
    }
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn diacritic(i: u16) -> char {
    char::from_u32(DIACRITICS[(i as usize).min(DIACRITICS.len() - 1)]).unwrap_or('\u{305}')
}

/// Row/column diacritics from kitty's `rowcolumn-diacritics.txt`.
#[rustfmt::skip]
static DIACRITICS: [u32; 297] = [
    0x305, 0x30D, 0x30E, 0x310, 0x312, 0x33D, 0x33E, 0x33F, 0x346, 0x34A, 0x34B, 0x34C, 0x350,
    0x351, 0x352, 0x357, 0x35B, 0x363, 0x364, 0x365, 0x366, 0x367, 0x368, 0x369, 0x36A, 0x36B,
    0x36C, 0x36D, 0x36E, 0x36F, 0x483, 0x484, 0x485, 0x486, 0x487, 0x592, 0x593, 0x594, 0x595,
    0x597, 0x598, 0x599, 0x59C, 0x59D, 0x59E, 0x59F, 0x5A0, 0x5A1, 0x5A8, 0x5A9, 0x5AB, 0x5AC,
    0x5AF, 0x5C4, 0x610, 0x611, 0x612, 0x613, 0x614, 0x615, 0x616, 0x617, 0x657, 0x658, 0x659,
    0x65A, 0x65B, 0x65D, 0x65E, 0x6D6, 0x6D7, 0x6D8, 0x6D9, 0x6DA, 0x6DB, 0x6DC, 0x6DF, 0x6E0,
    0x6E1, 0x6E2, 0x6E4, 0x6E7, 0x6E8, 0x6EB, 0x6EC, 0x730, 0x732, 0x733, 0x735, 0x736, 0x73A,
    0x73D, 0x73F, 0x740, 0x741, 0x743, 0x745, 0x747, 0x749, 0x74A, 0x7EB, 0x7EC, 0x7ED, 0x7EE,
    0x7EF, 0x7F0, 0x7F1, 0x7F3, 0x816, 0x817, 0x818, 0x819, 0x81B, 0x81C, 0x81D, 0x81E, 0x81F,
    0x820, 0x821, 0x822, 0x823, 0x825, 0x826, 0x827, 0x829, 0x82A, 0x82B, 0x82C, 0x82D, 0x951,
    0x953, 0x954, 0xF82, 0xF83, 0xF86, 0xF87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17,
    0x1A75, 0x1A76, 0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F,
    0x1B70, 0x1B71, 0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1,
    0x1DC3, 0x1DC4, 0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3,
    0x1DD4, 0x1DD5, 0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF,
    0x1DE0, 0x1DE1, 0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5,
    0x20D6, 0x20D7, 0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0,
    0x2DE1, 0x2DE2, 0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC,
    0x2DED, 0x2DEE, 0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8,
    0x2DF9, 0x2DFA, 0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1,
    0xA8E0, 0xA8E1, 0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB,
    0xA8EC, 0xA8ED, 0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE,
    0xAABF, 0xAAC1, 0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38,
    0x1D185, 0x1D186, 0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242,
    0x1D243, 0x1D244,
];

#[cfg(test)]
mod tests {
    use super::*;
    use dither_core::Rgba;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn dots_are_round_with_gaps() {
        let k = Kitty::new(10, 20);
        assert_eq!(k.dots_per_cell(), (2, 4));
        let mut g = PixelGrid::new(2, 4);
        g.set(0, 0, Rgba::WHITE);
        let (w, _, img) = k.image(&g, 1, 1);
        let alpha = |x: usize, y: usize| img[(y * w as usize + x) * 4 + 3];
        assert_eq!(alpha(2, 2), 255, "dot centre is solid");
        assert_eq!(alpha(0, 0), 0, "corner of the pitch stays empty");
        assert_eq!(alpha(7, 2), 0, "neighbouring empty dot stays empty");
    }

    #[test]
    fn placeholders_keep_the_surface_background() {
        let mut g = PixelGrid::new(2, 4);
        g.set(0, 0, Rgba::WHITE);
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_bg(Color::Rgb(1, 2, 3));
        Kitty::new(10, 20).paint(&g, area, &mut buf);
        assert_eq!(buf[(0, 0)].bg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn clipped_images_keep_their_placeholder_offsets() {
        let mut g = PixelGrid::new(4, 4);
        for y in 0..4 {
            g.set(3, y, Rgba::WHITE);
        }
        let mut buf = Buffer::empty(Rect::new(1, 0, 1, 1));
        Kitty::new(10, 20).paint(&g, Rect::new(0, 0, 2, 1), &mut buf);
        assert!(
            buf[(1, 0)].symbol().ends_with("\u{10EEEE}\u{305}\u{30D}"),
            "row 0, column 1"
        );
    }

    #[test]
    fn flush_moves_uploads_out_of_the_cells() {
        let mut g = PixelGrid::new(4, 8);
        g.set(0, 0, Rgba::WHITE);
        let area = Rect::new(7, 3, 2, 2);
        let mut out = Vec::new();
        flush(&mut out).unwrap();
        let mut buf = Buffer::empty(area);
        Kitty::new(10, 20).paint(&g, area, &mut buf);
        assert!(
            !buf[(7, 3)].symbol().contains('\x1b'),
            "no cell carries the upload"
        );
        flush(&mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().starts_with("\x1b_Ga=T,U=1"));
        // Unchanged pictures are not sent again.
        let mut again = Vec::new();
        Kitty::new(10, 20).paint(&g, area, &mut buf);
        flush(&mut again).unwrap();
        assert!(again.is_empty());
        OUT_OF_BAND.with(|o| o.set(false));
    }

    #[test]
    fn placeholders_carry_position_and_id() {
        let mut g = PixelGrid::new(4, 8);
        for x in 0..4 {
            for y in 0..8 {
                g.set(x, y, Rgba::WHITE);
            }
        }
        let area = Rect::new(0, 0, 2, 2);
        let mut buf = Buffer::empty(area);
        Kitty::new(10, 20).paint(&g, area, &mut buf);
        let first = buf[(0, 0)].symbol().to_string();
        assert_eq!(first, "\u{10EEEE}\u{305}\u{305}");
        let last = buf[(1, 1)].symbol();
        assert!(
            last.starts_with("\x1b_Ga=T,U=1"),
            "transmission rides in the last cell"
        );
        assert!(last.ends_with("\u{10EEEE}\u{30D}\u{30D}"));
        let id = image_id(area);
        assert_eq!(
            buf[(0, 0)].fg,
            Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
        );
    }
}
