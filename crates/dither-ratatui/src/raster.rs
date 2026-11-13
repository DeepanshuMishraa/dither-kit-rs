//! Painting a [`PixelGrid`] into a Ratatui buffer.
//!
//! Each terminal cell shows two vertically stacked pixels using the upper
//! half block `▀` (foreground = top pixel, background = bottom pixel). Since
//! a cell is roughly twice as tall as it is wide, pixels come out square.

use dither_core::{PixelGrid, Rgba};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

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

/// Pixel resolution of a cell area: one pixel per column, two per row.
pub fn pixel_size(area: Rect) -> (usize, usize) {
    (area.width as usize, area.height as usize * 2)
}

/// Paints `grid` into `area`. Transparent pixels keep whatever is already in
/// the buffer; translucent pixels are blended over the cell's background (or
/// `fallback_bg` when the cell has no truecolor background).
pub fn paint(grid: &PixelGrid, area: Rect, buf: &mut Buffer, fallback_bg: Rgba) {
    let area = area.intersection(buf.area);
    for cy in 0..area.height {
        for cx in 0..area.width {
            let (px, py) = (cx as usize, cy as usize * 2);
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
