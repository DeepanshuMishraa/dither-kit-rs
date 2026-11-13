//! Standalone dithered surfaces: gradients, animated backgrounds, button
//! textures, avatars, and meters.

use crate::color::{Rgba, Tone};
use crate::dither::{Fill, Matrix, hash01};
use crate::grid::PixelGrid;

/// Direction a gradient travels from its `from` tone toward `to`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// `from` at the bottom, `to` at the top.
    #[default]
    Up,
    Down,
    Left,
    Right,
    /// `from` in the centre, `to` at the edges.
    Radial,
}

impl Direction {
    /// Position (0..1) of pixel `(x, y)` along this direction.
    pub fn position(self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let fx = if w > 1 { x as f32 / (w - 1) as f32 } else { 0.5 };
        let fy = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.5 };
        match self {
            Direction::Up => 1.0 - fy,
            Direction::Down => fy,
            Direction::Right => fx,
            Direction::Left => 1.0 - fx,
            Direction::Radial => {
                let (dx, dy) = (fx - 0.5, fy - 0.5);
                ((dx * dx + dy * dy).sqrt() / std::f32::consts::FRAC_1_SQRT_2).min(1.0)
            }
        }
    }
}

/// A dithered gradient (dither-kit's `DitherGradient`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub from: Tone,
    /// Second tone. `None` fades `from` out to transparent instead.
    pub to: Option<Tone>,
    pub direction: Direction,
    pub matrix: Matrix,
    /// Overall pixel density, 0..1.
    pub density: f32,
}

impl Gradient {
    pub fn new(from: Tone) -> Self {
        Self {
            from,
            to: None,
            direction: Direction::Up,
            matrix: Matrix::Bayer8,
            density: 1.0,
        }
    }

    pub fn to(mut self, to: Tone) -> Self {
        self.to = Some(to);
        self
    }

    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    pub fn matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = matrix;
        self
    }

    pub fn density(mut self, density: f32) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }

    pub fn render(&self, w: usize, h: usize) -> PixelGrid {
        let mut g = PixelGrid::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let p = self.direction.position(x, y, w, h);
                let (xi, yi) = (x as i32, y as i32);
                let color = match self.to {
                    // Two tones: dither *between* them, then thin overall density.
                    Some(to) => {
                        let tone = if self.matrix.on(xi, yi, p) { to } else { self.from };
                        let lit = self.matrix.on(xi + 3, yi + 5, self.density * (0.55 + 0.45 * p));
                        lit.then(|| tone.shade(0.35 + 0.35 * p))
                    }
                    // One tone fading out.
                    None => self
                        .matrix
                        .on(xi, yi, (1.0 - p) * self.density)
                        .then(|| self.from.shade(0.3 + 0.4 * (1.0 - p))),
                };
                g.plot(xi, yi, color);
            }
        }
        g
    }
}

/// Procedural animated intensity fields for backgrounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Field {
    /// Interfering sine blobs.
    #[default]
    Plasma,
    /// Horizontal rolling waves.
    Waves,
    /// Concentric ripples from the centre.
    Ripple,
    /// Smooth drifting value noise.
    Noise,
    /// Vertical curtains, like northern lights.
    Aurora,
}

impl Field {
    pub const ALL: [Field; 5] = [
        Field::Plasma,
        Field::Waves,
        Field::Ripple,
        Field::Noise,
        Field::Aurora,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Field::Plasma => "plasma",
            Field::Waves => "waves",
            Field::Ripple => "ripple",
            Field::Noise => "noise",
            Field::Aurora => "aurora",
        }
    }

    /// Intensity (0..1) at normalised position `(u, v)` and time `t` seconds.
    pub fn sample(self, u: f32, v: f32, t: f32) -> f32 {
        let s = match self {
            Field::Plasma => {
                let a = (u * 7.0 + t * 0.9).sin();
                let b = (v * 6.0 - t * 0.7).sin();
                let c = ((u + v) * 5.0 + t * 0.5).sin();
                let d = (((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 12.0 - t * 1.2).sin();
                (a + b + c + d) / 8.0 + 0.5
            }
            Field::Waves => {
                let crest = 0.5 + 0.18 * (u * 6.0 + t * 1.1).sin() + 0.08 * (u * 15.0 - t * 1.7).sin();
                (1.0 - (v - crest).abs() * 2.6).clamp(0.0, 1.0) * 0.9 + v * 0.15
            }
            Field::Ripple => {
                let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
                0.5 + 0.5 * (d * 30.0 - t * 3.0).sin() * (1.0 - d * 1.2).max(0.0)
            }
            Field::Noise => {
                value_noise(u * 6.0 + t * 0.35, v * 4.0 - t * 0.2) * 0.7
                    + value_noise(u * 13.0 - t * 0.5, v * 9.0) * 0.3
            }
            Field::Aurora => {
                let curtain =
                    value_noise(u * 5.0 + t * 0.25, t * 0.15) * 0.6 + 0.4 * (u * 9.0 + t * 0.6).sin().abs();
                let fall = (1.0 - (v - 0.35 * curtain).max(0.0) * 1.6).clamp(0.0, 1.0);
                curtain * fall
            }
        };
        s.clamp(0.0, 1.0)
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let n = |a: i32, b: i32| hash01(a, b);
    let top = n(xi, yi) + (n(xi + 1, yi) - n(xi, yi)) * sx;
    let bot = n(xi, yi + 1) + (n(xi + 1, yi + 1) - n(xi, yi + 1)) * sx;
    top + (bot - top) * sy
}

/// An animated dithered background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Background {
    pub field: Field,
    pub tone: Tone,
    /// Optional second tone mixed in by intensity.
    pub accent: Option<Tone>,
    pub matrix: Matrix,
    /// Maximum density, 0..1. Keep low behind text.
    pub density: f32,
}

impl Background {
    pub fn new(field: Field, tone: Tone) -> Self {
        Self {
            field,
            tone,
            accent: None,
            matrix: Matrix::Bayer8,
            density: 0.6,
        }
    }

    pub fn accent(mut self, accent: Tone) -> Self {
        self.accent = Some(accent);
        self
    }

    pub fn matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = matrix;
        self
    }

    pub fn density(mut self, density: f32) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }

    pub fn render(&self, w: usize, h: usize, time: f32) -> PixelGrid {
        let mut g = PixelGrid::new(w, h);
        let (fw, fh) = (w.max(1) as f32, h.max(1) as f32);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / fw, y as f32 / fh);
                // Stretch contrast so mid-grey fields don't dither to a flat checkerboard.
                let i = smoothstep(0.18, 0.82, self.field.sample(u, v, time));
                let (xi, yi) = (x as i32, y as i32);
                if self.matrix.on(xi, yi, i * self.density) {
                    let tone = match self.accent {
                        Some(a) if Matrix::Bayer4.on(xi + 1, yi + 2, (i - 0.62) * 2.2) => a,
                        _ => self.tone,
                    };
                    g.set(xi, yi, tone.shade(0.3 + 0.5 * i));
                }
            }
        }
        g
    }
}

/// Visual style of a dithered button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
    /// Dense dither over a tinted surface.
    #[default]
    Solid,
    /// Dithered border; interior texture appears on hover.
    Outline,
    /// No border; texture only on hover/press.
    Ghost,
}

impl ButtonStyle {
    /// Texture density at rest, hovered, and pressed.
    pub fn density(self, hovered: bool, pressed: bool) -> f32 {
        let (rest, hover, press) = match self {
            ButtonStyle::Solid => (0.32, 0.55, 0.85),
            ButtonStyle::Outline => (0.0, 0.3, 0.6),
            ButtonStyle::Ghost => (0.0, 0.25, 0.5),
        };
        if pressed {
            press
        } else if hovered {
            hover
        } else {
            rest
        }
    }

    /// Background tint for the label surface, if any.
    pub fn surface(self, tone: Tone) -> Option<Rgba> {
        match self {
            ButtonStyle::Solid => Some(tone.shade(0.22)),
            _ => None,
        }
    }
}

/// The texture of a `DitherButton` at a given density (0..1).
///
/// `time` drives sparkles once the button is "excited" (density > 0.4).
pub fn button_texture(
    w: usize,
    h: usize,
    tone: Tone,
    style: ButtonStyle,
    density: f32,
    time: f32,
) -> PixelGrid {
    let mut g = PixelGrid::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as i32, y as i32);
            let edge = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            if edge && style != ButtonStyle::Ghost {
                // A crisp dithered frame: every other pixel along the edge.
                if (xi + yi) % 2 == 0 || style == ButtonStyle::Solid {
                    g.set(xi, yi, tone.shade(0.6));
                }
                continue;
            }
            let fy = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.5 };
            // Brighter toward the top, like a lit surface.
            let i = density * (1.05 - 0.45 * fy);
            if Fill::Gradient.covers(xi, yi, i) {
                g.set(xi, yi, tone.shade(0.45 + 0.25 * (1.0 - fy)));
            }
        }
    }
    if density > 0.4 {
        g.sparkle(time, 0.04 * density, tone.shade(0.97), false);
    }
    g
}

/// A deterministic 8x8 identicon (dither-kit's `DitherAvatar`).
///
/// 32 pattern bits are mirrored across one of two axes (left/right or
/// top/bottom), over 180 hues: ~1.5 trillion distinct avatars.
pub fn avatar(name: &str, hue: Option<f32>) -> PixelGrid {
    let h = fnv1a(name.as_bytes());
    let bits = h as u32;
    let mirror_x = (h >> 32) & 1 == 0;
    let tone = Tone::Hue(hue.unwrap_or(((h >> 33) % 180) as f32 * 2.0));
    let accent_bits = (h >> 41) as u32;
    let mut g = PixelGrid::new(8, 8);
    for i in 0..32u32 {
        // Half of the 8x8 grid: 4x8 (mirrored left/right) or 8x4 (top/bottom).
        let (x, y) = if mirror_x { (i % 4, i / 4) } else { (i % 8, i / 8) };
        let (mx, my) = if mirror_x { (7 - x, y) } else { (x, 7 - y) };
        let on = bits >> i & 1 == 1;
        let accent = on && (accent_bits >> (i % 23)) & 1 == 1 && (accent_bits >> ((i + 7) % 23)) & 1 == 1;
        let c = if accent {
            tone.shade(0.92)
        } else if on {
            tone.shade(0.62)
        } else {
            tone.shade(0.16)
        };
        g.set(x as i32, y as i32, c);
        g.set(mx as i32, my as i32, c);
    }
    g
}

/// 64-bit FNV-1a: tiny, stable across platforms and Rust versions.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A dithered horizontal meter / progress bar. `value` is 0..1.
pub fn meter(w: usize, h: usize, value: f32, tone: Tone, fill: Fill) -> PixelGrid {
    let mut g = PixelGrid::new(w, h);
    let filled = (value.clamp(0.0, 1.0) * w as f32).round() as usize;
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as i32, y as i32);
            if x < filled {
                // Intensity rises toward the leading edge.
                let i = 0.35 + 0.65 * (x as f32 + 1.0) / filled.max(1) as f32;
                if fill.covers(xi, yi, i) || x + 1 == filled {
                    g.set(xi, yi, tone.shade(0.4 + 0.35 * i));
                }
            } else if (xi + yi) % 4 == 0 {
                g.set(xi, yi, Tone::Grey.shade(0.35).fade(0.6));
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatars_are_stable_and_symmetric() {
        let a = avatar("Ada Lovelace", None);
        assert_eq!(a, avatar("Ada Lovelace", None));
        assert_ne!(a, avatar("Grace Hopper", None));
        let sym_x = (0..8).all(|y| (0..8).all(|x| a.get(x, y) == a.get(7 - x, y)));
        let sym_y = (0..8).all(|y| (0..8).all(|x| a.get(x, y) == a.get(x, 7 - y)));
        assert!(sym_x || sym_y);
        assert_eq!(a.lit_count(), 64);
    }

    #[test]
    fn denser_buttons_light_more_pixels() {
        let rest = button_texture(40, 10, Tone::Blue, ButtonStyle::Solid, 0.3, 0.0).lit_count();
        let press = button_texture(40, 10, Tone::Blue, ButtonStyle::Solid, 0.85, 0.0).lit_count();
        assert!(press > rest);
    }

    #[test]
    fn fields_stay_in_range() {
        for f in Field::ALL {
            for i in 0..50 {
                let s = f.sample(i as f32 / 50.0, (i * 7 % 50) as f32 / 50.0, i as f32 * 0.3);
                assert!((0.0..=1.0).contains(&s), "{f:?} {s}");
            }
        }
    }
}
