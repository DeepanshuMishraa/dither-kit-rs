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
//! ([`frame`]) and stop when reduced motion is on. Wrap any part of a view
//! in [`shimmer`] to make its dots twinkle the same way.
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
    static SHIMMER: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

/// Builds `view` with every dither canvas inside it shimmering at animation
/// `frame` of `fps`: each dot breathes, a sheen sweeps across and a few
/// flash, while shapes stay put. Drive `frame` from the app's `poll` with
/// [`frame`] as for backgrounds, and pass a fixed frame (or skip the
/// wrapper) when reduced motion is on.
///
/// ```ignore
/// dither_rune::shimmer(s.frame, 12.0, || dither_rune::meter("m", 3.0, 0.6, Tone::Blue, Fill::Gradient))
/// ```
pub fn shimmer<R>(frame: u64, fps: f32, view: impl FnOnce() -> R) -> R {
    /// Restores the enclosing scope's time, also when `view` panics.
    struct Restore(Option<f32>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SHIMMER.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(SHIMMER.with(|s| s.replace(Some(background_time(frame, fps)))));
    view()
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
/// On the first frame, before layout, it paints nothing. Non-positive or
/// non-finite `dot`, invalid layout sizes and overflowing grid sizes also paint nothing.
pub fn canvas(key: &str, dot: f32, inputs: impl Hash, build: impl FnOnce(usize, usize) -> PixelGrid) -> El {
    let e = el().key(key).a11y_hidden();
    let Some([w, h]) = rune::memo::layout_size(key) else {
        return e;
    };
    let Some((cols, rows)) = canvas_dimensions(w, h, dot) else {
        return e;
    };
    let mut hasher = DefaultHasher::new();
    let shimmer = SHIMMER.with(std::cell::Cell::get);
    (key, cols, rows, dot.to_bits(), shimmer.map(f32::to_bits)).hash(&mut hasher);
    inputs.hash(&mut hasher);
    e.shape(cached(hasher.finish(), || {
        let mut grid = build(cols, rows);
        if let Some(t) = shimmer {
            grid.shimmer(t);
        }
        shape(&grid, dot)
    }))
}

fn canvas_dimensions(w: f32, h: f32, dot: f32) -> Option<(usize, usize)> {
    if !dot.is_finite() || dot <= 0.0 || !w.is_finite() || !h.is_finite() {
        return None;
    }
    let (cols, rows) = ((w / dot).floor(), (h / dot).floor());
    if cols < 1.0 || rows < 1.0 || cols >= usize::MAX as f32 || rows >= usize::MAX as f32 {
        return None;
    }
    let (cols, rows) = (cols as usize, rows as usize);
    // Reject overflow before the caller can allocate a PixelGrid.
    let bytes = cols.checked_mul(rows)?.checked_mul(std::mem::size_of::<Rgba>())?;
    (bytes <= isize::MAX as usize).then_some((cols, rows))
}

fn progress_key(value: f32) -> u32 {
    value.clamp(0.0, 1.0).to_bits()
}

/// Hash sample bits without allocating a temporary copy on cache hits.
struct SampleBits<'a>(&'a [f64]);

impl Hash for SampleBits<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        for value in self.0 {
            value.to_bits().hash(state);
        }
    }
}

fn background_time(frame: u64, fps: f32) -> f32 {
    if fps.is_finite() && fps > 0.0 {
        frame as f32 / fps
    } else {
        0.0
    }
}

/// The animation frame for `time` seconds at `fps`, for a background's
/// inputs, so the grid changes only that often. Non-positive and non-finite
/// rates pause at frame zero.
pub fn frame(time: f64, fps: f32) -> u64 {
    if fps.is_finite() && fps > 0.0 {
        (time * f64::from(fps)).max(0.0) as u64
    } else {
        0
    }
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
    let v = progress_key(value);
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
    let bits = SampleBits(values);
    let p = progress_key(progress);
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
/// `frame` of `fps`. Positive rates below 1 are supported; non-positive or
/// non-finite rates show time zero. Keep the density low behind text.
pub fn background(key: &str, dot: f32, spec: Background, frame: u64, fps: f32) -> El {
    let inputs = background_inputs(spec, frame, fps);
    canvas(key, dot, inputs, |w, h| {
        spec.render(w, h, background_time(frame, fps))
    })
}

fn background_inputs(spec: Background, frame: u64, fps: f32) -> impl Hash {
    (
        spec.field as u8,
        tone_key(spec.tone),
        spec.accent.map(tone_key),
        spec.matrix as u8,
        spec.density.to_bits(),
        background_time(frame, fps).to_bits(),
        frame,
    )
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
    fn shimmer_scope_is_restored() {
        assert_eq!(SHIMMER.with(std::cell::Cell::get), None);
        let inner = shimmer(6, 12.0, || {
            let nested = shimmer(24, 12.0, || SHIMMER.with(std::cell::Cell::get));
            (nested, SHIMMER.with(std::cell::Cell::get))
        });
        assert_eq!(inner, (Some(2.0), Some(0.5)));
        assert_eq!(SHIMMER.with(std::cell::Cell::get), None);
        let caught = std::panic::catch_unwind(|| shimmer(6, 12.0, || panic!("view failed")));
        assert!(caught.is_err());
        assert_eq!(SHIMMER.with(std::cell::Cell::get), None, "restored after a panic");
    }

    #[test]
    fn frames_advance_at_the_requested_rate() {
        assert_eq!(frame(0.0, 12.0), 0);
        assert_eq!(frame(1.0, 12.0), 12);
        assert_eq!(frame(-1.0, 12.0), 0);
    }

    fn digest(value: impl Hash) -> u64 {
        let mut h = DefaultHasher::new();
        value.hash(&mut h);
        h.finish()
    }

    #[test]
    fn canvas_rejects_invalid_pixel_sizes_and_dimensions() {
        for dot in [
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
        ] {
            assert_eq!(canvas_dimensions(100.0, 100.0, dot), None, "dot={dot}");
        }
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(canvas_dimensions(size, 100.0, 3.0), None);
            assert_eq!(canvas_dimensions(100.0, size, 3.0), None);
        }
        assert_eq!(canvas_dimensions(10.0, 8.0, 3.0), Some((3, 2)));
    }

    #[test]
    fn progress_cache_keys_distinguish_different_rasterizations() {
        let (a, b) = (0.5001, 0.5009);
        assert_ne!(
            dither_core::meter(4096, 2, a, Tone::Blue, Fill::Solid),
            dither_core::meter(4096, 2, b, Tone::Blue, Fill::Solid)
        );
        assert_ne!(
            dither_core::sparkline(&[1.0, 3.0, 2.0], Tone::Blue, 4096, 8, false, a),
            dither_core::sparkline(&[1.0, 3.0, 2.0], Tone::Blue, 4096, 8, false, b)
        );
        assert_ne!(progress_key(a), progress_key(b));
        assert_eq!(progress_key(-1.0), progress_key(0.0));
        assert_eq!(progress_key(2.0), progress_key(1.0));
    }

    #[test]
    fn background_respects_sub_one_fps_and_pauses_invalid_rates() {
        assert_eq!(frame(8.0, 0.5), 4);
        assert_eq!(background_time(4, 0.5), 8.0);
        assert_eq!(background_time(4, 2.0), 2.0);
        for fps in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(background_time(4, fps), 0.0);
            assert_eq!(frame(8.0, fps), 0);
        }
    }

    #[test]
    fn background_cache_invalidates_when_frame_rate_changes() {
        assert_ne!(
            digest(background_inputs(
                Background::new(Field::Plasma, Tone::Blue),
                4,
                0.5
            )),
            digest(background_inputs(
                Background::new(Field::Plasma, Tone::Blue),
                4,
                2.0
            ))
        );
    }

    #[test]
    fn borrowed_sample_hash_preserves_bits_and_sequence_boundaries() {
        let samples = [0.0, -0.0, 1.5, f64::from_bits(0x7ff8000000000001)];
        let owned: Vec<u64> = samples.iter().map(|v| v.to_bits()).collect();
        assert_eq!(digest(SampleBits(&samples)), digest(owned));
        assert_ne!(digest(SampleBits(&[0.0])), digest(SampleBits(&[-0.0])));
        assert_ne!(digest(SampleBits(&samples)), digest(SampleBits(&samples[..3])));
    }
}
