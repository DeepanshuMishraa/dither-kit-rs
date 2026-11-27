//! Painting a [`PixelGrid`] into a Ratatui buffer.
//!
//! A terminal cell is much bigger than a dither dot, so how dots become
//! cells decides whether the result looks crisp or blocky. [`Raster`] picks
//! the method:
//!
//! - [`Raster::Kitty`] draws real image pixels, each dither dot an
//!   anti-aliased round dot with a gap around it, through the kitty
//!   graphics protocol's Unicode placeholders (kitty, Ghostty). This is the
//!   highest-fidelity mode and matches the GPUI and Rune frontends.
//! - [`Raster::Braille`] packs a 2×4 block of dots into each cell with
//!   braille glyphs. Dots stay small and separate in any truecolor
//!   terminal; each cell takes one colour, the average of its lit dots.
//! - [`Raster::HalfBlock`] is the old two-pixels-per-cell `▀` mode: one
//!   colour per pixel, but pixels are as wide as a cell and touch.
//!
//! [`Raster::current`] is what widgets use unless you pass one; it
//! auto-detects kitty graphics support once and otherwise falls back to
//! braille. Override it for the whole app with [`Raster::set_default`].

use std::sync::RwLock;

use dither_core::{PixelGrid, Rgba};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

pub use crate::kitty::Kitty;

/// How dither dots are drawn into terminal cells. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Raster {
    /// True-pixel round dots through the kitty graphics protocol.
    Kitty(Kitty),
    /// 2×4 dots per cell with braille glyphs; works everywhere.
    Braille,
    /// 1×2 square pixels per cell with half-block glyphs.
    HalfBlock,
}

static DEFAULT: RwLock<Option<Raster>> = RwLock::new(None);

impl Default for Raster {
    fn default() -> Self {
        Raster::current()
    }
}

impl Raster {
    /// The app-wide raster: the one set with [`Raster::set_default`], or
    /// else [`Raster::detect`]ed on first use.
    pub fn current() -> Raster {
        if let Some(r) = *DEFAULT.read().unwrap_or_else(|e| e.into_inner()) {
            return r;
        }
        let detected = Raster::detect();
        let mut slot = DEFAULT.write().unwrap_or_else(|e| e.into_inner());
        *slot.get_or_insert(detected)
    }

    /// Sets the raster every widget uses unless given its own.
    pub fn set_default(raster: Raster) {
        *DEFAULT.write().unwrap_or_else(|e| e.into_inner()) = Some(raster);
    }

    /// Kitty graphics when the terminal supports Unicode placeholders and
    /// reports its cell size in pixels, braille otherwise.
    ///
    /// `DITHER_RASTER=kitty|braille|halfblock` overrides the choice.
    pub fn detect() -> Raster {
        match std::env::var("DITHER_RASTER")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "kitty" => Raster::Kitty(Kitty::assumed()),
            "braille" => Raster::Braille,
            "halfblock" | "half-block" => Raster::HalfBlock,
            _ => Kitty::detect().map_or(Raster::Braille, Raster::Kitty),
        }
    }

    /// Dither dots per terminal cell, `(columns, rows)`.
    pub fn dots_per_cell(self) -> (usize, usize) {
        match self {
            Raster::Kitty(k) => k.dots_per_cell(),
            Raster::Braille => (2, 4),
            Raster::HalfBlock => (1, 2),
        }
    }

    /// Dot resolution of a cell area.
    pub fn pixel_size(self, area: Rect) -> (usize, usize) {
        let (sx, sy) = self.dots_per_cell();
        (area.width as usize * sx, area.height as usize * sy)
    }

    /// Paints `grid` into `area`. Cells whose dots are all transparent keep
    /// what is already in the buffer; translucent dots are blended over the
    /// cell's background (or `fallback_bg` when it has no truecolor one).
    pub fn paint(self, grid: &PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
        match self {
            Raster::Kitty(k) => k.paint(grid, area, buf),
            Raster::Braille => paint_braille(grid, area, buf, fallback_bg),
            Raster::HalfBlock => paint_half_block(grid, area, buf, fallback_bg),
        }
    }
}

/// Converts a core color to a Ratatui truecolor.
pub fn color(c: Rgba) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn to_rgba(c: Color) -> Option<Rgba> {
    match c {
        Color::Rgb(r, g, b) => Some(Rgba::rgb(r, g, b)),
        Color::Black => Some(Rgba::BLACK),
        Color::White => Some(Rgba::WHITE),
        _ => None,
    }
}

/// Dot resolution of a cell area with the [current](Raster::current) raster.
pub fn pixel_size(area: Rect) -> (usize, usize) {
    Raster::current().pixel_size(area)
}

/// Paints `grid` with the [current](Raster::current) raster.
pub fn paint(grid: &PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
    Raster::current().paint(grid, area, buf, fallback_bg);
}

/// The part of `area` inside the buffer, and its offset from `area`'s
/// origin in cells, so clipped paints still sample the right dots.
pub(crate) fn clip(area: Rect, buf: &Buffer) -> (Rect, u16, u16) {
    let clipped = area.intersection(buf.area);
    (
        clipped,
        clipped.x.saturating_sub(area.x),
        clipped.y.saturating_sub(area.y),
    )
}

fn paint_half_block(grid: &PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
    let (area, ox, oy) = clip(area, buf);
    for cy in 0..area.height {
        for cx in 0..area.width {
            let (px, py) = ((cx + ox) as usize, (cy + oy) as usize * 2);
            let top = grid.get(px, py);
            let bottom = grid.get(px, py + 1);
            if top.is_none() && bottom.is_none() {
                continue;
            }
            let Some(cell) = buf.cell_mut((area.x + cx, area.y + cy)) else {
                continue;
            };
            let under = to_rgba(cell.bg).unwrap_or(fallback_bg);
            let blend = |c: Rgba| c.over(under);
            match (top, bottom) {
                (Some(t), Some(b)) => {
                    cell.set_char('▀').set_fg(color(blend(t))).set_bg(color(blend(b)));
                }
                (Some(t), None) => {
                    cell.set_char('▀').set_fg(color(blend(t)));
                }
                (None, Some(b)) => {
                    cell.set_char('▄').set_fg(color(blend(b)));
                }
                (None, None) => unreachable!(),
            }
        }
    }
}

/// Braille dot bits for `(column, row)` inside a 2×4 cell.
const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

fn paint_braille(grid: &PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
    let (area, ox, oy) = clip(area, buf);
    for cy in 0..area.height {
        for cx in 0..area.width {
            let (x0, y0) = ((cx + ox) as usize * 2, (cy + oy) as usize * 4);
            let mut bits = 0u8;
            let (mut r, mut g, mut b, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut lit: [Option<Rgba>; 8] = [None; 8];
            let mut n = 0;
            for (dx, col) in BRAILLE_BITS.iter().enumerate() {
                for (dy, bit) in col.iter().enumerate() {
                    if let Some(c) = grid.get(x0 + dx, y0 + dy)
                        && c.a > 0
                    {
                        bits |= bit;
                        lit[n] = Some(c);
                        n += 1;
                    }
                }
            }
            if bits == 0 {
                continue;
            }
            let Some(cell) = buf.cell_mut((area.x + cx, area.y + cy)) else {
                continue;
            };
            let under = to_rgba(cell.bg).unwrap_or(fallback_bg);
            // One colour per cell: lit dots averaged, brighter ones weighted
            // more so a few highlights still read as highlights.
            for c in lit.iter().flatten() {
                let o = c.over(under);
                let w = 0.25 + o.luminance() + c.a as f32 / 255.0;
                r += o.r as f32 * w;
                g += o.g as f32 * w;
                b += o.b as f32 * w;
                wsum += w;
            }
            let avg = Rgba::rgb(
                (r / wsum).round() as u8,
                (g / wsum).round() as u8,
                (b / wsum).round() as u8,
            );
            let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
            cell.set_char(ch).set_fg(color(avg));
        }
    }
}

/// Per-widget drawing options shared by every widget: which raster to use
/// and whether the dots shimmer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// `None` uses [`Raster::current`].
    pub raster: Option<Raster>,
    /// Animate the dots ([`PixelGrid::shimmer`]); redraw every frame with
    /// a running `time`.
    pub shimmer: bool,
    /// Seconds, for shimmer and other motion.
    pub time: f32,
}

impl Look {
    pub fn raster(&self) -> Raster {
        self.raster.unwrap_or_else(Raster::current)
    }

    /// Paints `grid`, shimmering it first when enabled.
    pub fn paint(&self, mut grid: PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
        if self.shimmer {
            grid.shimmer(self.time);
        }
        self.raster().paint(&grid, area, buf, fallback_bg);
    }
}

/// Builders for widgets holding a `look: Look` field.
macro_rules! look_builders {
    () => {
        /// Draws with `raster` instead of [`Raster::current`](crate::Raster::current).
        pub fn raster(mut self, raster: crate::Raster) -> Self {
            self.look.raster = Some(raster);
            self
        }

        /// Animates the dots: each one breathes, a sheen sweeps across and
        /// a few flash. Redraw every frame and pass the running [`time`](Self::time).
        pub fn shimmer(mut self, shimmer: bool) -> Self {
            self.look.shimmer = shimmer;
            self
        }

        /// Running time in seconds, for shimmer and other motion.
        pub fn time(mut self, time: f32) -> Self {
            self.look.time = time;
            self
        }
    };
}
pub(crate) use look_builders;

/// Fills `area` with a solid background color.
pub fn fill_bg(area: Rect, buf: &mut Buffer, bg: Rgba) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(color(bg));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_packs_eight_dots_per_cell() {
        let mut g = PixelGrid::new(2, 4);
        g.set(0, 0, Rgba::WHITE);
        g.set(1, 3, Rgba::WHITE);
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        Raster::Braille.paint(&g, area, &mut buf, Rgba::BLACK);
        assert_eq!(buf[(0, 0)].symbol(), "\u{2881}");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(255, 255, 255));
    }

    #[test]
    fn clipped_paints_keep_their_source_offset() {
        // Two cells wide: only the right cell's dots are lit.
        let mut g = PixelGrid::new(4, 8);
        g.set(2, 0, Rgba::WHITE);
        let mut buf = Buffer::empty(Rect::new(1, 0, 1, 1));
        Raster::Braille.paint(&g, Rect::new(0, 0, 2, 1), &mut buf, Rgba::BLACK);
        assert_eq!(buf[(1, 0)].symbol(), "\u{2801}");
        let mut buf = Buffer::empty(Rect::new(1, 0, 1, 1));
        let mut g = PixelGrid::new(2, 2);
        g.set(1, 1, Rgba::WHITE);
        Raster::HalfBlock.paint(&g, Rect::new(0, 0, 2, 1), &mut buf, Rgba::BLACK);
        assert_eq!(buf[(1, 0)].symbol(), "▄");
    }

    #[test]
    fn transparent_cells_are_left_alone() {
        let g = PixelGrid::new(4, 8);
        let area = Rect::new(0, 0, 2, 2);
        let mut buf = Buffer::empty(area);
        buf.set_string(0, 0, "ab", ratatui::style::Style::default());
        for r in [
            Raster::Braille,
            Raster::HalfBlock,
            Raster::Kitty(Kitty::new(10, 20)),
        ] {
            r.paint(&g, area, &mut buf, Rgba::BLACK);
            assert_eq!(buf[(0, 0)].symbol(), "a");
        }
    }
}
