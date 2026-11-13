//! Painting core [`PixelGrid`]s with GPUI quads, plus the shared motion clock.

use std::time::Duration;

use dither_core::{PixelGrid, Rgba as CoreRgba};
use gpui_kit::{App, Bounds, Canvas, Hsla, Pixels, Rgba, Window, canvas, fill, point, px, size};
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

/// A canvas that builds a grid sized to its own layout bounds and paints it.
///
/// `build(width, height)` runs during prepaint, after layout, so the grid
/// always matches the element's real size. Style the returned canvas
/// (`.absolute().size_full()`, `.size(..)`) like any element.
pub fn pixel_canvas(
    pixel: Pixels,
    build: impl FnOnce(usize, usize) -> PixelGrid + 'static,
) -> Canvas<PixelGrid> {
    let pixel = pixel.max(px(1.0));
    canvas(
        move |bounds, _: &mut Window, _: &mut App| {
            let (w, h) = grid_size(bounds, pixel);
            build(w, h)
        },
        move |bounds, grid, window, _| paint_grid(&grid, bounds.origin, pixel, window),
    )
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
