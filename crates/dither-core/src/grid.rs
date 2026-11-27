//! The renderer-neutral pixel buffer every component draws into.

use crate::color::Rgba;
use crate::dither::{Matrix, hash01};

/// A width x height buffer of optional pixels. `None` is transparent.
///
/// Backends only need to paint this: GPUI paints one quad per horizontal
/// run of equal pixels ([`PixelGrid::runs`]); Ratatui packs two pixel rows
/// into each terminal cell with half-block glyphs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PixelGrid {
    width: usize,
    height: usize,
    cells: Vec<Option<Rgba>>,
}

/// A horizontal run of identical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub x: usize,
    pub y: usize,
    pub len: usize,
    pub color: Rgba,
}

impl PixelGrid {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![None; width * height],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<Rgba> {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x]
        } else {
            None
        }
    }

    /// Signed, bounds-checked get.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> Option<Rgba> {
        if x < 0 || y < 0 {
            None
        } else {
            self.get(x as usize, y as usize)
        }
    }

    /// Signed, bounds-checked write; out-of-bounds writes are ignored.
    #[inline]
    pub fn plot(&mut self, x: i32, y: i32, color: Option<Rgba>) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.cells[y as usize * self.width + x as usize] = color;
        }
    }

    /// Writes `color` only if the pixel is lit (`Some`), for convenience.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, color: Rgba) {
        self.plot(x, y, Some(color));
    }

    pub fn clear(&mut self) {
        self.cells.fill(None);
    }

    pub fn cells(&self) -> &[Option<Rgba>] {
        &self.cells
    }

    pub fn lit_count(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }

    /// Draws `other` on top of `self` at offset `(dx, dy)`.
    pub fn overlay(&mut self, other: &PixelGrid, dx: i32, dy: i32) {
        for y in 0..other.height {
            for x in 0..other.width {
                if let Some(c) = other.get(x, y) {
                    self.set(x as i32 + dx, y as i32 + dy, c);
                }
            }
        }
    }

    /// Multiplies every pixel's alpha.
    pub fn fade(&mut self, factor: f32) {
        for c in self.cells.iter_mut().flatten() {
            *c = c.fade(factor);
        }
    }

    /// Horizontal runs of identical lit pixels, row by row.
    pub fn runs(&self) -> impl Iterator<Item = Run> + '_ {
        (0..self.height).flat_map(move |y| {
            let row = &self.cells[y * self.width..(y + 1) * self.width];
            let mut x = 0;
            std::iter::from_fn(move || {
                while x < row.len() {
                    let Some(color) = row[x] else {
                        x += 1;
                        continue;
                    };
                    let start = x;
                    while x < row.len() && row[x] == Some(color) {
                        x += 1;
                    }
                    return Some(Run {
                        x: start,
                        y,
                        len: x - start,
                        color,
                    });
                }
                None
            })
        })
    }

    /// Adds a dithered glow around lit pixels (dither-kit's `bloom`).
    pub fn bloom(&mut self, bloom: Bloom) {
        let (radius, strength): (i32, f32) = match bloom {
            Bloom::Off => return,
            Bloom::Low => (1, 0.45),
            Bloom::High => (2, 0.6),
            Bloom::Aura => (4, 0.7),
        };
        let src = self.clone();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if src.at(x, y).is_some() {
                    continue;
                }
                // Nearest lit neighbour within the radius (Chebyshev distance).
                let mut best: Option<(i32, Rgba)> = None;
                'search: for d in 1..=radius {
                    for oy in -d..=d {
                        for ox in -d..=d {
                            if ox.abs() != d && oy.abs() != d {
                                continue;
                            }
                            if let Some(c) = src.at(x + ox, y + oy)
                                && c.a > 160
                            {
                                best = Some((d, c));
                                break 'search;
                            }
                        }
                    }
                }
                if let Some((d, c)) = best {
                    let glow = strength * (1.0 - (d as f32 - 1.0) / radius as f32);
                    if Matrix::Bayer4.on(x, y, glow) {
                        self.set(x, y, c.fade(0.35 + 0.25 * glow));
                    }
                }
            }
        }
    }

    /// Makes a still picture feel alive: every lit pixel breathes on its own
    /// phase, a soft sheen sweeps left to right, and a few pixels flash
    /// toward white. Shapes never move, so charts stay readable.
    ///
    /// `time` is in seconds; call it once per frame on a freshly rendered
    /// grid. Every backend's `shimmer` flag is built on this.
    pub fn shimmer(&mut self, time: f32) {
        use std::f32::consts::TAU;
        let w = self.width.max(1) as f32;
        for y in 0..self.height {
            for x in 0..self.width {
                let Some(c) = self.cells[y * self.width + x] else {
                    continue;
                };
                let (xi, yi) = (x as i32, y as i32);
                let phase = hash01(xi ^ 0x2f1b, yi) * TAU;
                let speed = 0.8 + 1.6 * hash01(yi ^ 0x33c7, xi);
                let breathe = 0.5 + 0.5 * (time * speed + phase).sin();
                // A broad sheen crossing the grid about every four seconds.
                let u = x as f32 / w;
                let sheen = (1.0 - ((u - (time * 0.25).fract() * 1.6 + 0.3).abs() * 5.0)).max(0.0);
                let mut out = c.fade(0.5 + 0.5 * breathe);
                let lift = sheen * 0.45;
                if hash01(xi ^ 0x7a3d, yi ^ 0x11) < 0.06 {
                    let flash = (time * speed * 0.9 + phase * 1.7).sin().max(0.0).powi(10);
                    out = out.mix(Rgba::WHITE.with_alpha(c.a), (lift + flash * 0.85).min(1.0));
                    out.a = c.a;
                } else if lift > 0.0 {
                    out = out.mix(Rgba::WHITE.with_alpha(out.a), lift * 0.5);
                    out.a = c.a.min((out.a as f32 * (1.0 + lift)) as u8);
                }
                self.cells[y * self.width + x] = Some(out);
            }
        }
    }

    /// Sprinkles twinkling highlight pixels ("sparkles wink").
    ///
    /// `density` is the fraction of pixels that can sparkle; `time` is in
    /// seconds. Sparkles only land on already-lit pixels unless `anywhere`.
    pub fn sparkle(&mut self, time: f32, density: f32, color: Rgba, anywhere: bool) {
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if hash01(x, y ^ 0x5bd1) >= density {
                    continue;
                }
                if !anywhere && self.at(x, y).is_none() {
                    continue;
                }
                let phase = hash01(y, x ^ 0x1b56) * std::f32::consts::TAU;
                let speed = 1.5 + 2.5 * hash01(x ^ 0x77, y);
                let wink = ((time * speed + phase).sin().max(0.0)).powi(12);
                if wink > 0.35 {
                    self.set(x, y, color.fade(wink));
                }
            }
        }
    }
}

/// Glow strength around lit pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bloom {
    #[default]
    Off,
    Low,
    High,
    Aura,
}

impl Bloom {
    pub const ALL: [Bloom; 4] = [Bloom::Off, Bloom::Low, Bloom::High, Bloom::Aura];

    pub fn name(self) -> &'static str {
        match self {
            Bloom::Off => "off",
            Bloom::Low => "low",
            Bloom::High => "high",
            Bloom::Aura => "aura",
        }
    }

    pub fn next(self) -> Bloom {
        match self {
            Bloom::Off => Bloom::Low,
            Bloom::Low => Bloom::High,
            Bloom::High => Bloom::Aura,
            Bloom::Aura => Bloom::Off,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_merge_equal_pixels() {
        let mut g = PixelGrid::new(6, 2);
        let a = Rgba::hex(0xff0000);
        let b = Rgba::hex(0x00ff00);
        for x in 0..3 {
            g.set(x, 0, a);
        }
        g.set(4, 0, b);
        g.set(5, 1, a);
        let runs: Vec<_> = g.runs().collect();
        assert_eq!(
            runs,
            vec![
                Run {
                    x: 0,
                    y: 0,
                    len: 3,
                    color: a
                },
                Run {
                    x: 4,
                    y: 0,
                    len: 1,
                    color: b
                },
                Run {
                    x: 5,
                    y: 1,
                    len: 1,
                    color: a
                },
            ]
        );
    }

    #[test]
    fn shimmer_moves_colour_but_never_shape() {
        let mut g = PixelGrid::new(16, 8);
        for x in 0..16 {
            g.set(x, 3, Rgba::hex(0x3b82f6));
        }
        let (mut a, mut b) = (g.clone(), g.clone());
        a.shimmer(0.0);
        b.shimmer(1.3);
        assert_ne!(a, b, "time changes the picture");
        for y in 0..8 {
            for x in 0..16 {
                assert_eq!(a.get(x, y).is_some(), g.get(x, y).is_some());
            }
        }
    }

    #[test]
    fn bloom_only_adds_pixels() {
        let mut g = PixelGrid::new(9, 9);
        g.set(4, 4, Rgba::WHITE);
        let before = g.lit_count();
        g.bloom(Bloom::Aura);
        assert!(g.lit_count() > before);
        assert_eq!(g.get(4, 4), Some(Rgba::WHITE));
    }
}
