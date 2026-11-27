//! Standalone widgets: gradient, background, button, avatar, meter, legend.

use dither_core::{
    Background as CoreBackground, ButtonStyle, Fill, Gradient as CoreGradient, Rgba, Tone, avatar,
    button_texture, ease, meter,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Widget;

use crate::raster::{Look, color, fill_bg, look_builders};
use crate::theme::Theme;

/// A dithered gradient filling its whole area.
#[derive(Clone, Copy, Debug)]
pub struct Gradient {
    pub gradient: CoreGradient,
    pub theme: Theme,
    pub look: Look,
}

impl Gradient {
    pub fn new(gradient: CoreGradient) -> Self {
        Self {
            gradient,
            theme: Theme::default(),
            look: Look::default(),
        }
    }

    look_builders!();
}

impl Widget for Gradient {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (w, h) = self.look.raster().pixel_size(area);
        self.look
            .paint(self.gradient.render(w, h), area, buf, self.theme.background);
    }
}

/// An animated dithered background; pass a running `time` in seconds.
#[derive(Clone, Copy, Debug)]
pub struct Background {
    pub background: CoreBackground,
    pub theme: Theme,
    pub look: Look,
}

impl Background {
    pub fn new(background: CoreBackground, time: f32) -> Self {
        Self {
            background,
            theme: Theme::default(),
            look: Look {
                time,
                ..Look::default()
            },
        }
    }

    look_builders!();
}

impl Widget for Background {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (w, h) = self.look.raster().pixel_size(area);
        self.look.paint(
            self.background.render(w, h, self.look.time),
            area,
            buf,
            self.theme.background,
        );
    }
}

/// Interaction state of a [`Button`]; keep one per button in your app state.
///
/// Call [`ButtonState::tick`] every frame so texture density eases toward
/// its target instead of snapping.
#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    density: Option<f32>,
}

impl ButtonState {
    /// Advances the density animation by `dt` seconds.
    pub fn tick(&mut self, style: ButtonStyle, dt: f32) {
        let target = style.density(self.hovered || self.focused, self.pressed);
        let current = self.density.unwrap_or(target);
        self.density = Some(ease::approach(current, target, dt, 0.08));
    }

    pub fn density(&self, style: ButtonStyle) -> f32 {
        self.density
            .unwrap_or_else(|| style.density(self.hovered || self.focused, self.pressed))
    }

    /// Whether the cell `(col, row)` is inside `area` (for mouse hit tests).
    pub fn hit(area: Rect, col: u16, row: u16) -> bool {
        area.contains((col, row).into())
    }
}

/// A dithered button whose texture gets denser on hover and press.
#[derive(Clone, Debug)]
pub struct Button<'a> {
    pub label: &'a str,
    pub tone: Tone,
    pub style: ButtonStyle,
    pub state: ButtonState,
    pub disabled: bool,
    pub theme: Theme,
    pub look: Look,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str, tone: Tone) -> Self {
        Self {
            label,
            tone,
            style: ButtonStyle::Solid,
            state: ButtonState::default(),
            disabled: false,
            theme: Theme::default(),
            look: Look::default(),
        }
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn state(mut self, state: ButtonState) -> Self {
        self.state = state;
        self
    }

    look_builders!();

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Widget for Button<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let tone = if self.disabled { Tone::Grey } else { self.tone };
        if let Some(surface) = self.style.surface(tone) {
            fill_bg(area, buf, surface);
        }
        let raster = self.look.raster();
        let (sx, sy) = raster.dots_per_cell();
        let (w, h) = raster.pixel_size(area);
        let density = if self.disabled {
            0.15
        } else {
            self.state.density(self.style)
        };
        let mut tex = button_texture(w, h, tone, self.style, density, self.look.time);
        // Clear a calm band behind the label so text stays legible.
        let label_w = self.label.chars().count() + 2;
        let lx = (area.width as usize).saturating_sub(label_w) / 2;
        let ly = area.height as usize / 2;
        for y in ly * sy..(ly + 1) * sy {
            for x in lx * sx..(lx + label_w) * sx {
                tex.plot(x as i32, y as i32, None);
            }
        }
        self.look.paint(tex, area, buf, self.theme.background);
        let fg = if self.disabled {
            self.theme.muted
        } else {
            self.theme.foreground
        };
        let mut style = Style::default().fg(color(fg)).add_modifier(Modifier::BOLD);
        if self.state.focused && !self.disabled {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        let x = area.x + lx as u16 + 1;
        let y = area.y + area.height / 2;
        buf.set_span(
            x,
            y,
            &Span::styled(self.label, style),
            area.right().saturating_sub(x),
        );
    }
}

/// A deterministic dithered identicon for a name.
#[derive(Clone, Debug)]
pub struct Avatar<'a> {
    pub name: &'a str,
    pub hue: Option<f32>,
    /// 1 = 8x4 cells, 2 = 16x8 cells, ...
    pub scale: usize,
    pub theme: Theme,
    pub look: Look,
}

impl<'a> Avatar<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            hue: None,
            scale: 1,
            theme: Theme::default(),
            look: Look::default(),
        }
    }

    look_builders!();

    pub fn hue(mut self, hue: f32) -> Self {
        self.hue = Some(hue);
        self
    }

    pub fn scale(mut self, scale: usize) -> Self {
        self.scale = scale.max(1);
        self
    }

    /// Cell size of the avatar at its scale.
    pub fn size(&self) -> (u16, u16) {
        ((8 * self.scale) as u16, (4 * self.scale) as u16)
    }
}

impl Widget for Avatar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let src = avatar(self.name, self.hue);
        let (cols, rows) = self.size();
        let cell = Rect::new(area.x, area.y, cols.min(area.width), rows.min(area.height));
        let (w, h) = self.look.raster().pixel_size(Rect::new(0, 0, cols, rows));
        let mut g = dither_core::PixelGrid::new(w, h);
        for y in 0..h {
            for x in 0..w {
                g.plot(x as i32, y as i32, src.get(x * 8 / w, y * 8 / h));
            }
        }
        self.look.paint(g, cell, buf, self.theme.background);
    }
}

/// A dithered progress meter, one cell tall or more.
#[derive(Clone, Copy, Debug)]
pub struct Meter {
    pub value: f32,
    pub tone: Tone,
    pub fill: Fill,
    pub theme: Theme,
    pub look: Look,
}

impl Meter {
    pub fn new(value: f32, tone: Tone) -> Self {
        Self {
            value,
            tone,
            fill: Fill::Gradient,
            theme: Theme::default(),
            look: Look::default(),
        }
    }

    look_builders!();

    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }
}

impl Widget for Meter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (w, h) = self.look.raster().pixel_size(area);
        self.look.paint(
            meter(w, h, self.value, self.tone, self.fill),
            area,
            buf,
            self.theme.background,
        );
    }
}

/// One legend entry.
#[derive(Clone, Debug)]
pub struct LegendItem<'a> {
    pub label: &'a str,
    pub tone: Tone,
    pub hidden: bool,
    pub selected: bool,
}

/// A horizontal legend with dithered swatches. Returns each item's hit
/// area through [`Legend::layout`] so clicks can toggle series.
#[derive(Clone, Debug)]
pub struct Legend<'a> {
    pub items: Vec<LegendItem<'a>>,
    pub theme: Theme,
    pub center: bool,
}

impl<'a> Legend<'a> {
    pub fn new(items: Vec<LegendItem<'a>>) -> Self {
        Self {
            items,
            theme: Theme::default(),
            center: false,
        }
    }

    pub fn centered(mut self) -> Self {
        self.center = true;
        self
    }

    /// The rectangles each item occupies inside `area`.
    pub fn layout(&self, area: Rect) -> Vec<Rect> {
        let widths: Vec<u16> = self
            .items
            .iter()
            .map(|i| i.label.chars().count() as u16 + 4)
            .collect();
        let total: u16 = widths.iter().sum::<u16>() + widths.len().saturating_sub(1) as u16;
        let mut x = if self.center {
            area.x + area.width.saturating_sub(total) / 2
        } else {
            area.x
        };
        widths
            .iter()
            .map(|w| {
                let r = Rect::new(x, area.y, (*w).min(area.right().saturating_sub(x)), 1);
                x = x.saturating_add(w + 1);
                r
            })
            .collect()
    }

    /// Index of the item at a mouse position.
    pub fn hit(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        self.layout(area)
            .iter()
            .position(|r| r.contains((col, row).into()))
    }
}

impl Widget for Legend<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rects = self.layout(area);
        for (item, r) in self.items.iter().zip(rects) {
            if r.width < 3 {
                break;
            }
            let swatch = if item.hidden {
                Tone::Grey.shade(0.35)
            } else {
                item.tone.base()
            };
            buf.set_span(
                r.x,
                r.y,
                &Span::styled("▚▚", Style::default().fg(color(swatch))),
                2,
            );
            let mut style = Style::default().fg(color(if item.hidden {
                self.theme.muted
            } else {
                self.theme.foreground
            }));
            if item.hidden {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            if item.selected {
                style = style.add_modifier(Modifier::BOLD);
            }
            buf.set_span(
                r.x + 3,
                r.y,
                &Span::styled(item.label, style),
                r.width.saturating_sub(3),
            );
        }
    }
}

/// Picks black or white text for a dithered surface of `tone`.
pub fn label_color(tone: Tone) -> Rgba {
    if tone.base().luminance() > 0.45 {
        Rgba::BLACK
    } else {
        Rgba::WHITE
    }
}
