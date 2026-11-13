//! Colors, named tones, and perceptual (OKLCH) shade ramps.

/// An 8-bit sRGB color with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Parses `0xRRGGBB`.
    pub const fn hex(hex: u32) -> Self {
        Self::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Multiplies the alpha channel by `factor` (0..=1).
    pub fn fade(self, factor: f32) -> Self {
        self.with_alpha((self.a as f32 * factor.clamp(0.0, 1.0)).round() as u8)
    }

    /// Linear interpolation in sRGB space (`t = 0` is `self`).
    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgba::new(
            l(self.r, other.r),
            l(self.g, other.g),
            l(self.b, other.b),
            l(self.a, other.a),
        )
    }

    /// Composites `self` over an opaque `background`, returning an opaque color.
    pub fn over(self, background: Rgba) -> Rgba {
        background
            .with_alpha(255)
            .mix(self.with_alpha(255), self.a as f32 / 255.0)
    }

    /// Relative luminance (WCAG), used to choose legible label colors.
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// Converts an OKLCH color (`l` 0..1, `c` ~0..0.37, `h` degrees) to sRGB,
    /// reducing chroma until the color fits inside the sRGB gamut.
    pub fn oklch(l: f32, c: f32, h: f32) -> Rgba {
        let mut c = c.max(0.0);
        loop {
            if let Some(rgb) = oklch_to_srgb(l, c, h) {
                return rgb;
            }
            c -= 0.005;
            if c <= 0.0 {
                return oklch_to_srgb(l, 0.0, h).unwrap_or(Rgba::BLACK);
            }
        }
    }
}

fn oklch_to_srgb(l: f32, c: f32, h: f32) -> Option<Rgba> {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let bb = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    let eps = 1e-4;
    if [r, g, bb].iter().any(|v| *v < -eps || *v > 1.0 + eps) {
        return None;
    }
    let enc = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    };
    Some(Rgba::rgb(enc(r), enc(g), enc(bb)))
}

/// A named dither color, or any hue in degrees.
///
/// Each tone expands into a perceptual shade ramp via [`Tone::shade`], so
/// every component gets matching dark, mid, and highlight colors.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tone {
    #[default]
    Green,
    Blue,
    Purple,
    Pink,
    Orange,
    Red,
    Grey,
    /// Any hue, 0..360 degrees.
    Hue(f32),
}

impl Tone {
    /// The named tones, in legend order.
    pub const ALL: [Tone; 7] = [
        Tone::Green,
        Tone::Blue,
        Tone::Purple,
        Tone::Pink,
        Tone::Orange,
        Tone::Red,
        Tone::Grey,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tone::Green => "green",
            Tone::Blue => "blue",
            Tone::Purple => "purple",
            Tone::Pink => "pink",
            Tone::Orange => "orange",
            Tone::Red => "red",
            Tone::Grey => "grey",
            Tone::Hue(_) => "hue",
        }
    }

    /// Parses a tone name (`"blue"`) or a hue number (`"210"`).
    pub fn parse(s: &str) -> Option<Tone> {
        let s = s.trim().to_ascii_lowercase();
        Some(match s.as_str() {
            "green" => Tone::Green,
            "blue" => Tone::Blue,
            "purple" | "violet" => Tone::Purple,
            "pink" => Tone::Pink,
            "orange" => Tone::Orange,
            "red" => Tone::Red,
            "grey" | "gray" => Tone::Grey,
            other => Tone::Hue(other.parse::<f32>().ok()?.rem_euclid(360.0)),
        })
    }

    pub fn hue(self) -> f32 {
        match self {
            Tone::Green => 150.0,
            Tone::Blue => 250.0,
            Tone::Purple => 300.0,
            Tone::Pink => 350.0,
            Tone::Orange => 55.0,
            Tone::Red => 25.0,
            Tone::Grey => 260.0,
            Tone::Hue(h) => h.rem_euclid(360.0),
        }
    }

    fn chroma(self) -> f32 {
        match self {
            Tone::Grey => 0.012,
            _ => 0.19,
        }
    }

    /// A color along this tone's ramp: `0.0` is near-black, `0.5` is the
    /// signature color, `1.0` is a pale highlight.
    pub fn shade(self, level: f32) -> Rgba {
        let level = level.clamp(0.0, 1.0);
        let l = 0.22 + level * 0.72;
        // Chroma peaks around the middle of the ramp and tapers at both ends.
        let c = self.chroma() * (1.0 - (level - 0.55).abs() * 1.3).clamp(0.25, 1.0);
        Rgba::oklch(l, c, self.hue())
    }

    /// The tone's signature color.
    pub fn base(self) -> Rgba {
        self.shade(0.55)
    }
}

impl From<f32> for Tone {
    fn from(hue: f32) -> Self {
        Tone::Hue(hue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_round_trip() {
        assert_eq!(Rgba::oklch(1.0, 0.0, 0.0), Rgba::WHITE);
        assert_eq!(Rgba::oklch(0.0, 0.0, 0.0), Rgba::BLACK);
    }

    #[test]
    fn shades_get_brighter() {
        for tone in Tone::ALL {
            let lums: Vec<f32> = (0..=10)
                .map(|i| tone.shade(i as f32 / 10.0).luminance())
                .collect();
            assert!(lums.windows(2).all(|w| w[1] >= w[0]), "{tone:?}: {lums:?}");
        }
    }

    #[test]
    fn parse_tones() {
        assert_eq!(Tone::parse("Gray"), Some(Tone::Grey));
        assert_eq!(Tone::parse("400"), Some(Tone::Hue(40.0)));
        assert_eq!(Tone::parse("nope"), None);
    }
}
