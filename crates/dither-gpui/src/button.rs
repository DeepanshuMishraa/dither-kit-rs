//! `DitherButton`: a GPUI Kit `Button` with a dithered texture that gets
//! denser on hover and press.
//!
//! Behavior (focus, Tab order, Enter/Space activation, accessibility role,
//! focus ring, disabled state) comes from Kit's `Button`; this component only
//! owns presentation, so it behaves exactly like every other Kit button.

use std::rc::Rc;
use std::time::Duration;

use dither_core::{ButtonStyle, Tone, button_texture};
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Disableable};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClickEvent, ElementId, MouseButton, Pixels, SharedString, Window, div, px};
use gpui_motion_kit::ValueState;
use gpui_motion_kit::core::{Spring, Transition};

use crate::pixels::{Clock, hsla, pixel_canvas};

/// Retained per-button presentation state (keyed by the button id).
struct ButtonFx {
    hovered: bool,
    pressed: bool,
    density: ValueState,
    clock: Clock,
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct DitherButton {
    id: ElementId,
    label: SharedString,
    tone: Tone,
    style: ButtonStyle,
    disabled: bool,
    pixel: Pixels,
    on_click: Option<ClickHandler>,
}

impl DitherButton {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            tone: Tone::Green,
            style: ButtonStyle::Solid,
            disabled: false,
            pixel: px(2.0),
            on_click: None,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// `Solid` (default), `Outline`, or `Ghost`.
    pub fn variant(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn outline(self) -> Self {
        self.variant(ButtonStyle::Outline)
    }

    pub fn ghost(self) -> Self {
        self.variant(ButtonStyle::Ghost)
    }

    pub fn pixel_size(mut self, pixel: Pixels) -> Self {
        self.pixel = pixel;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Disableable for DitherButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for DitherButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fx_id = ElementId::from((self.id.clone(), "dither-fx"));
        let fx = window.use_keyed_state(fx_id, cx, |_, _| ButtonFx {
            hovered: false,
            pressed: false,
            density: ValueState::new(self.style.density(false, false) as f64),
            clock: Clock::new(),
        });

        let tone = if self.disabled { Tone::Grey } else { self.tone };
        let style = self.style;
        let spring = Spring::visual(Duration::from_millis(220), 0.15).unwrap_or_default();
        let (density, time) = fx.update(cx, |fx, cx| {
            let hovered = fx.hovered && !self.disabled;
            let target = if self.disabled {
                0.12
            } else {
                style.density(hovered, fx.pressed)
            };
            let d = fx
                .density
                .frame(target as f64, Transition::Spring(spring), window, cx)
                .value() as f32;
            // Only tick the sparkle clock while the texture is "excited".
            let t = if d > 0.4 {
                fx.clock.seconds(window, cx)
            } else {
                0.0
            };
            (d, t)
        });

        let theme = cx.theme();
        let fg = match style {
            ButtonStyle::Solid => hsla(tone.shade(0.97)),
            _ => theme.foreground,
        };
        let surface = style.surface(tone).map(hsla).unwrap_or(theme.transparent);
        let variant = ButtonCustomVariant::new(cx)
            .color(surface)
            .foreground(fg)
            .hover(surface)
            .active(surface);

        let pixel = self.pixel;
        let texture = pixel_canvas(pixel, move |w, h| {
            button_texture(w, h, tone, style, density, time)
        })
        .absolute()
        .size_full();

        let fx_hover = fx.clone();
        let content = div()
            .id("dither-content")
            .relative()
            .h_full()
            .px_4()
            .flex()
            .items_center()
            .justify_center()
            .child(texture)
            .child(
                div()
                    .relative()
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .child(self.label.clone()),
            )
            .on_hover(move |hovered, _, cx| {
                fx_hover.update(cx, |fx, cx| {
                    fx.hovered = *hovered;
                    if !*hovered {
                        fx.pressed = false;
                    }
                    cx.notify();
                })
            });

        let (fx_down, fx_up, fx_out) = (fx.clone(), fx.clone(), fx);
        let set_pressed = |fx: &gpui_kit::Entity<ButtonFx>, pressed: bool, cx: &mut App| {
            fx.update(cx, |fx, cx| {
                fx.pressed = pressed;
                cx.notify();
            })
        };
        let mut button = Button::new(self.id)
            .custom(variant)
            .p_0()
            .h_9()
            .overflow_hidden()
            .rounded(theme.radius)
            .disabled(self.disabled)
            .accessibility_label(self.label)
            .child(content)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| set_pressed(&fx_down, true, cx))
            .on_mouse_up(MouseButton::Left, move |_, _, cx| set_pressed(&fx_up, false, cx))
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| set_pressed(&fx_out, false, cx));
        if let Some(handler) = self.on_click {
            button = button.on_click(move |e, w, cx| handler(e, w, cx));
        }
        button
    }
}
