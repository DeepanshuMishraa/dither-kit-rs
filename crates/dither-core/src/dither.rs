//! Threshold matrices and fill styles.
//!
//! Everything in dither-kit is *ordered* dithering: a continuous intensity
//! (0..1) becomes on/off pixels by comparing it against a position-dependent
//! threshold. Ordered dithering is stateless per pixel, so it is cheap,
//! deterministic, stable across frames (no shimmer while animating), and
//! identical in every backend.

/// Classic recursive Bayer matrices.
const BAYER2: [u8; 4] = [0, 2, 3, 1];

const fn bayer4() -> [u8; 16] {
    let mut m = [0u8; 16];
    let mut y = 0;
    while y < 4 {
        let mut x = 0;
        while x < 4 {
            let inner = BAYER2[(y % 2) * 2 + (x % 2)];
            let outer = BAYER2[(y / 2) * 2 + (x / 2)];
            m[y * 4 + x] = 4 * inner + outer;
            x += 1;
        }
        y += 1;
    }
    m
}

const BAYER4: [u8; 16] = bayer4();

const fn bayer8() -> [u8; 64] {
    let mut m = [0u8; 64];
    let mut y = 0;
    while y < 8 {
        let mut x = 0;
        while x < 8 {
            let inner = BAYER4[(y % 4) * 4 + (x % 4)];
            let outer = BAYER2[((y / 4) % 2) * 2 + ((x / 4) % 2)];
            m[y * 8 + x] = 4 * inner + outer;
            x += 1;
        }
        y += 1;
    }
    m
}

const BAYER8: [u8; 64] = bayer8();

/// A clustered-dot (halftone) 4x4 threshold order.
const HALFTONE4: [u8; 16] = [12, 5, 6, 13, 4, 0, 1, 7, 11, 3, 2, 8, 15, 10, 9, 14];

/// A threshold matrix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Matrix {
    Bayer2,
    Bayer4,
    #[default]
    Bayer8,
    /// Clustered dots, like newspaper halftone.
    Halftone,
    /// Hash-based white noise (stable per pixel).
    Noise,
}

impl Matrix {
    /// Threshold in the open interval (0, 1) for pixel `(x, y)`.
    #[inline]
    pub fn threshold(self, x: i32, y: i32) -> f32 {
        let (xu, yu) = (x.rem_euclid(8) as usize, y.rem_euclid(8) as usize);
        match self {
            Matrix::Bayer2 => (BAYER2[(yu % 2) * 2 + xu % 2] as f32 + 0.5) / 4.0,
            Matrix::Bayer4 => (BAYER4[(yu % 4) * 4 + xu % 4] as f32 + 0.5) / 16.0,
            Matrix::Bayer8 => (BAYER8[yu * 8 + xu] as f32 + 0.5) / 64.0,
            Matrix::Halftone => (HALFTONE4[(yu % 4) * 4 + xu % 4] as f32 + 0.5) / 16.0,
            Matrix::Noise => (hash2(x, y) as f32 + 0.5) / (u32::MAX as f32 + 1.0),
        }
    }

    /// Whether a pixel of the given intensity is lit.
    #[inline]
    pub fn on(self, x: i32, y: i32, intensity: f32) -> bool {
        intensity > self.threshold(x, y)
    }
}

/// How a shape's interior is textured; mirrors dither-kit's `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    /// Ordered Bayer gradient: density follows intensity.
    #[default]
    Gradient,
    /// Sparse, evenly spaced dots.
    Dotted,
    /// Diagonal hatching.
    Hatched,
    /// Every covered pixel is lit.
    Solid,
}

impl Fill {
    pub const ALL: [Fill; 4] = [Fill::Gradient, Fill::Dotted, Fill::Hatched, Fill::Solid];

    pub fn name(self) -> &'static str {
        match self {
            Fill::Gradient => "gradient",
            Fill::Dotted => "dotted",
            Fill::Hatched => "hatched",
            Fill::Solid => "solid",
        }
    }

    /// Whether a covered pixel with `intensity` (0..1) is lit.
    #[inline]
    pub fn covers(self, x: i32, y: i32, intensity: f32) -> bool {
        match self {
            Fill::Gradient => Matrix::Bayer8.on(x, y, intensity),
            Fill::Dotted => {
                x.rem_euclid(2) == 0
                    && (y + (x / 2).rem_euclid(2)).rem_euclid(2) == 0
                    && Matrix::Bayer4.on(x / 2, y / 2, intensity * 1.4)
            }
            Fill::Hatched => (x + y).rem_euclid(3) == 0 && Matrix::Bayer4.on(x, y, 0.25 + intensity),
            Fill::Solid => intensity > 0.02,
        }
    }
}

/// A fast, well-mixed 32-bit hash of a pixel coordinate.
#[inline]
pub fn hash2(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// Hash of a coordinate as a float in 0..1.
#[inline]
pub fn hash01(x: i32, y: i32) -> f32 {
    hash2(x, y) as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayer8_is_a_permutation() {
        let mut seen = [false; 64];
        for v in BAYER8 {
            assert!(!seen[v as usize]);
            seen[v as usize] = true;
        }
    }

    #[test]
    fn density_tracks_intensity() {
        for m in [Matrix::Bayer2, Matrix::Bayer4, Matrix::Bayer8, Matrix::Halftone] {
            let lit = (0..64).filter(|i| m.on(i % 8, i / 8, 0.5)).count();
            assert_eq!(lit, 32, "{m:?}");
            assert_eq!((0..64).filter(|i| m.on(i % 8, i / 8, 0.0)).count(), 0);
            assert_eq!((0..64).filter(|i| m.on(i % 8, i / 8, 1.0)).count(), 64);
        }
    }
}
