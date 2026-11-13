//! Standalone surfaces: gradient, animated background, avatar, meter.

use std::time::Duration;

use dither_core::{Background as BackgroundSpec, Fill, Gradient as GradientSpec, Tone, avatar, meter};
use gpui_kit::TestSupportExt as _;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{App, Div, ElementId, Pixels, Role, SharedString, StyleRefinement, Window, div, px};
use gpui_motion_kit::ValueState;
use gpui_motion_kit::core::{Spring, Transition};

use crate::pixels::{Clock, pixel_canvas};

/// A dithered gradient. By default it fills its parent (`absolute`, inset
/// 0) like dither-kit's `DitherGradient`; restyle it to size it yourself.
#[derive(IntoElement)]
pub struct DitherGradient {
    base: Div,
    spec: GradientSpec,
    pixel: Pixels,
}

impl DitherGradient {
    pub fn new(from: Tone) -> Self {
        Self {
            base: div().absolute().inset_0(),
            spec: GradientSpec::new(from),
            pixel: px(3.0),
        }
    }

    /// Blends toward a second tone instead of fading out.
    pub fn to(mut self, to: Tone) -> Self {
        self.spec = self.spec.to(to);
        self
    }

    pub fn direction(mut self, direction: dither_core::Direction) -> Self {
        self.spec = self.spec.direction(direction);
        self
    }

    pub fn matrix(mut self, matrix: dither_core::Matrix) -> Self {
        self.spec = self.spec.matrix(matrix);
        self
    }

    pub fn density(mut self, density: f32) -> Self {
        self.spec = self.spec.density(density);
        self
    }

    /// Logical size of one dither pixel (default 3px).
    pub fn pixel_size(mut self, pixel: Pixels) -> Self {
        self.pixel = pixel;
        self
    }
}

impl Styled for DitherGradient {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for DitherGradient {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let spec = self.spec;
        self.base.overflow_hidden().child(
            pixel_canvas(self.pixel, move |w, h| spec.render(w, h))
                .absolute()
                .size_full(),
        )
    }
}

/// An animated dithered background (plasma, waves, ripple, noise, aurora).
///
/// Fills its parent by default. Animation pauses entirely when the user
/// prefers reduced motion. Keep `density` low behind text.
#[derive(IntoElement)]
pub struct DitherBackground {
    id: ElementId,
    base: Div,
    spec: BackgroundSpec,
    pixel: Pixels,
    speed: f32,
}

impl DitherBackground {
    pub fn new(id: impl Into<ElementId>, field: dither_core::Field, tone: Tone) -> Self {
        Self {
            id: id.into(),
            base: div().absolute().inset_0(),
            spec: BackgroundSpec::new(field, tone),
            pixel: px(4.0),
            speed: 1.0,
        }
    }

    pub fn accent(mut self, accent: Tone) -> Self {
        self.spec = self.spec.accent(accent);
        self
    }

    pub fn density(mut self, density: f32) -> Self {
        self.spec = self.spec.density(density);
        self
    }

    pub fn matrix(mut self, matrix: dither_core::Matrix) -> Self {
        self.spec = self.spec.matrix(matrix);
        self
    }

    pub fn pixel_size(mut self, pixel: Pixels) -> Self {
        self.pixel = pixel;
        self
    }

    /// Time multiplier (default 1.0).
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
}

impl Styled for DitherBackground {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for DitherBackground {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let clock = window.use_keyed_state(self.id.clone(), cx, |_, _| Clock::new());
        let t = clock.update(cx, |clock, cx| clock.seconds(window, cx)) * self.speed;
        let spec = self.spec;
        self.base.overflow_hidden().child(
            pixel_canvas(self.pixel, move |w, h| spec.render(w, h, t))
                .absolute()
                .size_full(),
        )
    }
}

/// A deterministic identicon generated from a name.
#[derive(IntoElement)]
pub struct DitherAvatar {
    name: SharedString,
    size: Pixels,
    hue: Option<f32>,
}

impl DitherAvatar {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            size: px(40.0),
            hue: None,
        }
    }

    /// Edge length; rounded down to a multiple of 8 so pixels stay crisp.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Overrides the hue (0..360) derived from the name.
    pub fn hue(mut self, hue: f32) -> Self {
        self.hue = Some(hue);
        self
    }
}

impl RenderOnce for DitherAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pixel = px((f32::from(self.size) / 8.0).floor().max(1.0));
        let (name, hue) = (self.name.clone(), self.hue);
        div()
            .id(SharedString::from(format!("dither-avatar-{}", self.name)))
            .role(Role::Image)
            .aria_label(self.name)
            .size(pixel * 8.0)
            .flex_none()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .child(pixel_canvas(pixel, move |_, _| avatar(&name, hue)).size_full())
    }
}

/// A dithered progress meter whose value animates with a spring.
#[derive(IntoElement)]
pub struct DitherMeter {
    id: ElementId,
    value: f32,
    tone: Tone,
    fill: Fill,
    pixel: Pixels,
    height: Pixels,
    label: Option<SharedString>,
}

impl DitherMeter {
    pub fn new(id: impl Into<ElementId>, value: f32, tone: Tone) -> Self {
        Self {
            id: id.into(),
            value: value.clamp(0.0, 1.0),
            tone,
            fill: Fill::Gradient,
            pixel: px(2.0),
            height: px(10.0),
            label: None,
        }
    }

    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn pixel_size(mut self, pixel: Pixels) -> Self {
        self.pixel = pixel;
        self
    }

    /// Accessible name, e.g. "Upload progress".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for DitherMeter {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ValueState::new(self.value as f64));
        let spring = Spring::visual(Duration::from_millis(450), 0.0).unwrap_or_default();
        let shown = state.update(cx, |v, cx| {
            v.frame(self.value as f64, Transition::Spring(spring), window, cx)
                .value()
        });
        let (tone, fill) = (self.tone, self.fill);
        let percent = format!("{}%", (self.value * 100.0).round() as u32);
        div()
            .id(self.id)
            .test_support()
            .role(Role::ProgressIndicator)
            .aria_label(self.label.unwrap_or_else(|| "Progress".into()))
            .aria_description(percent)
            .relative()
            .w_full()
            .h(self.height)
            .child(
                pixel_canvas(self.pixel, move |w, h| meter(w, h, shown as f32, tone, fill))
                    .absolute()
                    .size_full(),
            )
    }
}
