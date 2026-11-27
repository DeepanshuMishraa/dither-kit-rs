//! Painting core [`PixelGrid`]s with GPUI quads, plus the shared motion clock.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dither_core::{DotStyle, PixelGrid, Rgba as CoreRgba};
use gpui_kit::{App, Bounds, Canvas, Global, Hsla, Pixels, Rgba, Window, canvas, fill, point, px, size};
use gpui_motion_kit::AnimationState;
use gpui_motion_kit::core::{Animation, Easing, Repeat, RepeatMode, Target};

/// Converts a core color into a GPUI color.
pub fn hsla(c: CoreRgba) -> Hsla {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: c.a as f32 / 255.0,
    }
    .into()
}

/// Grid resolution for `bounds` at `pixel` logical pixels per dither pixel.
pub fn grid_size(bounds: Bounds<Pixels>, pixel: Pixels) -> (usize, usize) {
    let w = (bounds.size.width / pixel).floor().max(0.0) as usize;
    let h = (bounds.size.height / pixel).floor().max(0.0) as usize;
    (w, h)
}

/// Paints `grid` with its top-left at `origin`, merging horizontal runs of
/// equal pixels into single quads (typically a 3-6x reduction in quads).
pub fn paint_grid(grid: &PixelGrid, origin: gpui_kit::Point<Pixels>, pixel: Pixels, window: &mut Window) {
    for run in grid.runs() {
        let at = point(origin.x + pixel * run.x as f32, origin.y + pixel * run.y as f32);
        window.paint_quad(fill(
            Bounds::new(at, size(pixel * run.len as f32, pixel)),
            hsla(run.color),
        ));
    }
}

/// Paints `grid` in `style`: [`DotStyle::Square`] merges runs like
/// [`paint_grid`]; [`DotStyle::Round`] draws one rounded quad per lit pixel,
/// a disc with a gap around it.
pub fn paint_grid_styled(
    grid: &PixelGrid,
    origin: gpui_kit::Point<Pixels>,
    pixel: Pixels,
    style: DotStyle,
    window: &mut Window,
) {
    match style {
        DotStyle::Square => paint_grid(grid, origin, pixel, window),
        DotStyle::Round => {
            let d = pixel * DotStyle::ROUND_DIAMETER;
            let inset = (pixel - d) / 2.0;
            for y in 0..grid.height() {
                for x in 0..grid.width() {
                    let Some(c) = grid.get(x, y) else { continue };
                    if c.a == 0 {
                        continue;
                    }
                    let at = point(
                        origin.x + pixel * x as f32 + inset,
                        origin.y + pixel * y as f32 + inset,
                    );
                    window.paint_quad(fill(Bounds::new(at, size(d, d)), hsla(c)).corner_radii(d / 2.0));
                }
            }
        }
    }
}

/// The app-wide dot style, set with [`set_dot_style`].
struct GlobalDotStyle(DotStyle);

impl Global for GlobalDotStyle {}

/// Sets how every dither component in the app draws its pixels (default
/// [`DotStyle::Square`]); components can still override it with
/// `.dot_style(..)`. Open windows redraw with the new style.
pub fn set_dot_style(style: DotStyle, cx: &mut App) {
    cx.set_global(GlobalDotStyle(style));
    cx.refresh_windows();
}

/// The app-wide dot style.
pub fn dot_style(cx: &App) -> DotStyle {
    cx.try_global::<GlobalDotStyle>()
        .map_or(DotStyle::Square, |g| g.0)
}

/// A canvas that builds a grid sized to its own layout bounds and paints it.
///
/// `build(width, height)` runs during prepaint, after layout, so the grid
/// always matches the element's real size. Style the returned canvas
/// (`.absolute().size_full()`, `.size(..)`) like any element.
pub fn pixel_canvas(
    pixel: Pixels,
    build: impl FnOnce(usize, usize) -> PixelGrid + 'static,
) -> Canvas<PixelGrid> {
    pixel_canvas_with(pixel, None, build)
}

/// [`pixel_canvas`] with a dot style; `None` follows the app-wide
/// [`set_dot_style`].
pub fn pixel_canvas_with(
    pixel: Pixels,
    style: Option<DotStyle>,
    build: impl FnOnce(usize, usize) -> PixelGrid + 'static,
) -> Canvas<PixelGrid> {
    let pixel = pixel.max(px(1.0));
    canvas(
        move |bounds, _: &mut Window, _: &mut App| {
            let (w, h) = grid_size(bounds, pixel);
            build(w, h)
        },
        move |bounds, grid, window, cx| {
            let style = style.unwrap_or_else(|| dot_style(cx));
            paint_grid_styled(&grid, bounds.origin, pixel, style, window)
        },
    )
}

/// Seconds for a component's `shimmer` flag ([`PixelGrid::shimmer`]),
/// scheduling the next frame so the dots keep moving. `None` when the user
/// prefers reduced motion, so shimmering pictures hold still.
pub fn shimmer_time(window: &Window, cx: &App) -> Option<f32> {
    static START: OnceLock<Instant> = OnceLock::new();
    if cx.reduce_motion() {
        return None;
    }
    window.request_animation_frame();
    Some(START.get_or_init(Instant::now).elapsed().as_secs_f32())
}

/// Shimmers `grid` at `time` when it is `Some`.
pub fn shimmered(mut grid: PixelGrid, time: Option<f32>) -> PixelGrid {
    if let Some(t) = time {
        grid.shimmer(t);
    }
    grid
}

/// A looping seconds clock for procedural effects (backgrounds, sparkles),
/// driven by `gpui-motion-kit` so it follows the app's frame scheduling and
/// the operating-system reduced-motion preference.
pub struct Clock {
    player: AnimationState,
    clip: Animation,
}

const PERIOD_SECS: u64 = 3600;

impl Clock {
    pub fn new() -> Self {
        let clip = Animation::to(
            Target::default().x(PERIOD_SECS as f32),
            Duration::from_secs(PERIOD_SECS),
            Easing::Linear,
        )
        .repeat(Repeat::Forever, RepeatMode::Loop, Duration::ZERO)
        .expect("a non-zero linear clip repeats");
        Self {
            player: AnimationState::new(Target::default()),
            clip,
        }
    }

    /// Current time in seconds; schedules the next frame while running.
    /// Returns a frozen time when the user prefers reduced motion.
    pub fn seconds(&mut self, window: &mut Window, cx: &App) -> f32 {
        if cx.reduce_motion() {
            return 0.0;
        }
        self.player.frame(&self.clip, window, cx).target().x_px()
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}
