//! # dither-rune
//!
//! A [Rune](https://github.com/anomalyco/rune) frontend for
//! [dither-kit-rs](https://github.com/DeepanshuMishraa/dither-kit-rs). Every
//! dither-kit component rasterizes into a [`PixelGrid`] in `dither-core`;
//! this crate paints that grid as a Rune [`Shape`] (one rectangle per
//! horizontal run of equal pixels, as `dither-gpui` paints GPU quads), so
//! dots stay crisp at any scale and nothing is resampled.
//!
//! [`canvas`] sizes a grid to the element's own laid-out size, like
//! `dither-gpui`'s `pixel_canvas`; the component helpers ([`meter`],
//! [`avatar`], [`sparkline`], [`gradient`], [`background`], [`chart`]) wrap
//! it. Grids are cached by key, size and inputs, so a view that re-runs
//! without changes does not rasterize again. Animated backgrounds take the
//! time as an input: drive it from the app's `poll` at a modest frame rate
//! ([`frame`]) and stop when reduced motion is on.
#![allow(missing_docs)]

use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

pub use dither_core::{
    self as core, Background, Bloom, Cartesian, CartesianKind, Direction, Field, Fill, Gradient, Matrix, Pie,
    PixelGrid, Radar, Rgba, Series, Slice, Tone,
};
use rune::{Color, El, Shape, el};

/// sRGB byte to linear light, the space Rune's [`Color`] uses.
fn linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A dither-core colour as a Rune colour.
pub fn color(c: Rgba) -> Color {
    Color {
        r: linear(c.r),
        g: linear(c.g),
        b: linear(c.b),
        a: f32::from(c.a) / 255.0,
    }
}

/// The dither-kit tone nearest a Rune colour: its OKLCH hue, or grey when
/// the colour has almost no chroma.
pub fn tone(c: Color) -> Tone {
    let [_, a, b, _] = c.to_oklab();
    if (a * a + b * b).sqrt() < 0.03 {
        Tone::Grey
    } else {
        Tone::Hue(b.atan2(a).to_degrees().rem_euclid(360.0))
    }
}

/// The grid as a shape, `dot` points per pixel, one rectangle per run.
pub fn shape(grid: &PixelGrid, dot: f32) -> Shape {
    let mut s = Shape::new();
    for run in grid.runs() {
        let (x0, y0) = (run.x as f32 * dot, run.y as f32 * dot);
        let (x1, y1) = (x0 + run.len as f32 * dot, y0 + dot);
        s = s.fill(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], color(run.color));
    }
    s
}

/// A fixed-size element showing `grid` at `dot` points per pixel.
pub fn pixels(grid: &PixelGrid, dot: f32) -> El {
    el().size(grid.width() as f32 * dot, grid.height() as f32 * dot)
        .a11y_hidden()
        .shape(shape(grid, dot))
}

/// Cache size: past this many grids the cache starts over.
const CACHE_LIMIT: usize = 512;

thread_local! {
    static CACHE: RefCell<HashMap<u64, Shape>> = RefCell::new(HashMap::new());
}

fn cached(id: u64, build: impl FnOnce() -> Shape) -> Shape {
    if let Some(s) = CACHE.with(|c| c.borrow().get(&id).cloned()) {
        return s;
    }
    let s = build();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(id, s.clone());
    });
    s
}

/// An element keyed `key` that fills the space its caller gives it and
/// paints `build(columns, rows)` with `dot`-point pixels sized to its own
/// layout. `inputs` must change whenever the picture should (values, tone,
/// animation frame): the grid is rebuilt only then or when the size changes.
/// On the first frame, before layout, it paints nothing.
pub fn canvas(key: &str, dot: f32, inputs: impl Hash, build: impl FnOnce(usize, usize) -> PixelGrid) -> El {
    let e = el().key(key).a11y_hidden();
    let Some([w, h]) = rune::memo::layout_size(key) else {
        return e;
    };
    let (cols, rows) = ((w / dot).floor() as usize, (h / dot).floor() as usize);
    if cols == 0 || rows == 0 {
        return e;
    }
    let mut hasher = DefaultHasher::new();
    (key, cols, rows, dot.to_bits()).hash(&mut hasher);
    inputs.hash(&mut hasher);
    e.shape(cached(hasher.finish(), || shape(&build(cols, rows), dot)))
}

/// The animation frame for `time` seconds at `fps`, for a background's
/// inputs, so the grid changes only that often.
pub fn frame(time: f64, fps: f32) -> u64 {
    (time * f64::from(fps)).max(0.0) as u64
}

fn tone_key(t: Tone) -> (u8, u32) {
    match t {
        Tone::Hue(h) => (255, h.to_bits()),
        named => (Tone::ALL.iter().position(|n| *n == named).unwrap_or(0) as u8, 0),
    }
}

/// A dithered meter filling its element: dense up to `value` (0..1) with an
/// intensity that rises to the leading edge, a faint hatch after it.
pub fn meter(key: &str, dot: f32, value: f32, tone: Tone, fill: Fill) -> El {
    let v = (value.clamp(0.0, 1.0) * 1000.0) as u32;
    canvas(key, dot, (v, tone_key(tone), fill as u8), |w, h| {
        dither_core::meter(w, h, value, tone, fill)
    })
}

/// dither-kit's 8×8 identicon for `name`, `size` points square.
pub fn avatar(name: &str, size: f32, hue: Option<f32>) -> El {
    let mut hasher = DefaultHasher::new();
    ("avatar", name, size.to_bits(), hue.map(f32::to_bits)).hash(&mut hasher);
    let dot = size / 8.0;
    el().size(size, size)
        .a11y_hidden()
        .shape(cached(hasher.finish(), || {
            shape(&dither_core::avatar(name, hue), dot)
        }))
}

/// A chrome-free dithered area sparkline of `values`, drawn in to
/// `progress` (0..1).
pub fn sparkline(key: &str, dot: f32, values: &[f64], tone: Tone, progress: f32) -> El {
    let bits: Vec<u64> = values.iter().map(|v| v.to_bits()).collect();
    let p = (progress.clamp(0.0, 1.0) * 1000.0) as u32;
    canvas(key, dot, (bits, tone_key(tone), p), |w, h| {
        dither_core::sparkline(values, tone, w, h, false, progress)
    })
}

/// A dithered gradient filling its element.
pub fn gradient(key: &str, dot: f32, spec: Gradient) -> El {
    let inputs = (
        tone_key(spec.from),
        spec.to.map(tone_key),
        spec.direction as u8,
        spec.matrix as u8,
        spec.density.to_bits(),
    );
    canvas(key, dot, inputs, |w, h| spec.render(w, h))
}

/// A dithered field (plasma, waves, ripple, noise or aurora) at animation
/// `frame` of `fps`. Keep the density low behind text.
pub fn background(key: &str, dot: f32, spec: Background, frame: u64, fps: f32) -> El {
    let inputs = (
        spec.field as u8,
        tone_key(spec.tone),
        spec.accent.map(tone_key),
        spec.matrix as u8,
        spec.density.to_bits(),
        frame,
    );
    canvas(key, dot, inputs, |w, h| {
        spec.render(w, h, frame as f32 / fps.max(1.0))
    })
}

/// Any other dither-core picture (a pie, radar or cartesian chart) sized to
/// its element; `inputs` identifies what it shows.
pub fn chart(key: &str, dot: f32, inputs: impl Hash, build: impl FnOnce(usize, usize) -> PixelGrid) -> El {
    canvas(key, dot, inputs, build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_bytes_round_trip_through_rune_colours() {
        for (v, hex) in [(0u8, "#000000ff"), (128, "#808080ff"), (255, "#ffffffff")] {
            assert_eq!(color(Rgba::rgb(v, v, v)).hex(), hex);
        }
        assert_eq!(color(Rgba::new(255, 0, 0, 0)).a, 0.0);
    }

    #[test]
    fn tones_follow_hue_and_greys_stay_grey() {
        assert_eq!(tone(rune::color::hex("#808080").unwrap()), Tone::Grey);
        let Tone::Hue(h) = tone(rune::color::hex("#5eead4").unwrap()) else {
            panic!("teal is not grey")
        };
        assert!((150.0..200.0).contains(&h), "teal hue {h}");
    }

    #[test]
    fn a_shape_has_one_rectangle_per_run() {
        let mut g = PixelGrid::new(4, 2);
        g.set(0, 0, Rgba::WHITE);
        g.set(1, 0, Rgba::WHITE);
        g.set(3, 1, Rgba::BLACK);
        assert_eq!(g.runs().count(), 2);
        assert!(!shape(&g, 2.0).is_empty());
        assert!(shape(&PixelGrid::new(3, 3), 2.0).is_empty());
    }

    #[test]
    fn frames_advance_at_the_requested_rate() {
        assert_eq!(frame(0.0, 12.0), 0);
        assert_eq!(frame(1.0, 12.0), 12);
        assert_eq!(frame(-1.0, 12.0), 0);
    }
}
