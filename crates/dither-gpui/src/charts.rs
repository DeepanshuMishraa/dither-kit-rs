//! Chart components: area, line, bar, pie/donut, radar, sparkline, legend.
//!
//! Charts are `RenderOnce` values with a caller-supplied `ElementId`; their
//! interaction state (hover, selection, entrance animation, tooltip glide)
//! is retained in keyed element state, so the caller only owns data.

use std::rc::Rc;
use std::time::Duration;

use dither_core::{
    Bloom, Cartesian, CartesianKind, Pie, Radar, Series, Slice, Tone, format_value, sparkline,
};
use gpui_kit::TestSupportExt as _;
use gpui_kit::base::{ElementExt as _, Selectable as _};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, Div, ElementId, Entity, FontWeight, Hsla, MouseMoveEvent, Pixels, Role,
    SharedString, Window, div, px, relative,
};
use gpui_motion_kit::ValueState;
use gpui_motion_kit::core::{Easing, Spring, Transition};

use crate::pixels::{grid_size, hsla, pixel_canvas};

type IndexHandler = Rc<dyn Fn(&Option<usize>, &mut Window, &mut App)>;

/// Called with the index of a clicked legend entry.
pub type LegendClick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Retained interaction state shared by the chart components.
struct ChartState {
    hover: Option<usize>,
    selected: Option<usize>,
    entrance: ValueState,
    glide: Option<ValueState>,
    plot: Option<Bounds<Pixels>>,
    token: u64,
}

impl ChartState {
    fn new(animate: bool, token: u64) -> Self {
        Self {
            hover: None,
            selected: None,
            entrance: ValueState::new(if animate { 0.0 } else { 1.0 }),
            glide: None,
            plot: None,
            token,
        }
    }

    /// Entrance progress (0..1), restarting when `token` changes.
    fn progress(&mut self, anim: &Entrance, window: &mut Window, cx: &App) -> f32 {
        if self.token != anim.replay_token {
            self.token = anim.replay_token;
            self.entrance = ValueState::new(0.0);
        }
        if !anim.animate {
            return 1.0;
        }
        let tween = Transition::Tween(anim.duration, Easing::EaseOut);
        self.entrance.frame(1.0, tween, window, cx).value() as f32
    }
}

/// Entrance-animation options common to all charts.
#[derive(Clone, Copy)]
struct Entrance {
    animate: bool,
    duration: Duration,
    replay_token: u64,
}

impl Default for Entrance {
    fn default() -> Self {
        Self {
            animate: true,
            duration: Duration::from_millis(900),
            replay_token: 0,
        }
    }
}

macro_rules! entrance_builders {
    () => {
        /// Animate the entrance (default true; skipped under reduced motion).
        pub fn animate(mut self, animate: bool) -> Self {
            self.entrance.animate = animate;
            self
        }

        pub fn animation_duration(mut self, duration: Duration) -> Self {
            self.entrance.duration = duration;
            self
        }

        /// Change this value to replay the entrance animation.
        pub fn replay_token(mut self, token: u64) -> Self {
            self.entrance.replay_token = token;
            self
        }

        /// Glow around lit pixels.
        pub fn bloom(mut self, bloom: Bloom) -> Self {
            self.bloom = bloom;
            self
        }

        /// Logical size of one dither pixel (default 3px).
        pub fn pixel_size(mut self, pixel: Pixels) -> Self {
            self.pixel = pixel;
            self
        }
    };
}

/// A small dithered color swatch for legends and tooltips.
pub fn swatch(tone: Tone) -> impl IntoElement {
    div().size(px(10.0)).flex_none().child(
        pixel_canvas(px(2.0), move |w, h| {
            let mut g = dither_core::PixelGrid::new(w, h);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let level = if (x + y) % 2 == 0 { 0.72 } else { 0.48 };
                    g.set(x, y, tone.shade(level));
                }
            }
            g
        })
        .size_full(),
    )
}

/// One legend entry.
pub struct LegendEntry {
    pub label: SharedString,
    pub tone: Tone,
    pub selected: bool,
    pub dimmed: bool,
}

/// A centered legend. With `on_click`, entries are focusable Kit buttons
/// with ids `("legend-item", index)`, scoped by the enclosing chart.
pub fn legend(entries: Vec<LegendEntry>, on_click: Option<LegendClick>, cx: &App) -> Div {
    let theme = cx.theme();
    let (fg, muted) = (theme.foreground, theme.muted_foreground);
    h_flex()
        .w_full()
        .justify_center()
        .flex_wrap()
        .gap_1()
        .children(entries.into_iter().enumerate().map(move |(i, e)| -> AnyElement {
            let content = h_flex()
                .gap_1p5()
                .text_xs()
                .text_color(if e.dimmed { muted } else { fg })
                .when(e.selected, |d| d.font_weight(FontWeight::SEMIBOLD))
                .child(swatch(e.tone))
                .child(e.label.clone());
            match &on_click {
                Some(handler) => {
                    let handler = handler.clone();
                    Button::new(("legend-item", i))
                        .ghost()
                        .xsmall()
                        .selected(e.selected)
                        .accessibility_label(e.label)
                        .child(content)
                        .on_click(move |_, window, cx| handler(i, window, cx))
                        .into_any_element()
                }
                None => content.px_1p5().into_any_element(),
            }
        }))
}

fn tooltip(title: Option<SharedString>, rows: Vec<(Tone, SharedString, f64)>, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .min_w(px(132.0))
        .gap_1()
        .p_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_md()
        .text_xs()
        .when_some(title, |d, t| {
            d.child(div().font_weight(FontWeight::SEMIBOLD).child(t))
        })
        .children(rows.into_iter().map(|(tone, label, value)| {
            h_flex()
                .gap_3()
                .justify_between()
                .child(
                    h_flex()
                        .gap_1p5()
                        .text_color(theme.muted_foreground)
                        .child(swatch(tone))
                        .child(label),
                )
                .child(div().font_weight(FontWeight::MEDIUM).child(format_value(value)))
        }))
}

/// Area, line, or bar chart with axes, legend, crosshair, and a gliding tooltip.
#[derive(IntoElement)]
pub struct DitherChart {
    id: ElementId,
    kind: CartesianKind,
    series: Rc<Vec<Series>>,
    labels: Rc<Vec<SharedString>>,
    stacked: bool,
    bloom: Bloom,
    pixel: Pixels,
    height: Pixels,
    legend: bool,
    clickable_legend: bool,
    tooltip: bool,
    y_axis: bool,
    interactive: bool,
    marker_index: Option<usize>,
    entrance: Entrance,
    description: Option<SharedString>,
    on_hover_change: Option<IndexHandler>,
    on_selection_change: Option<IndexHandler>,
}

impl DitherChart {
    pub fn new(id: impl Into<ElementId>, kind: CartesianKind, series: Vec<Series>) -> Self {
        Self {
            id: id.into(),
            kind,
            series: Rc::new(series),
            labels: Rc::new(Vec::new()),
            stacked: false,
            bloom: Bloom::Off,
            pixel: px(3.0),
            height: px(220.0),
            legend: true,
            clickable_legend: true,
            tooltip: true,
            y_axis: true,
            interactive: true,
            marker_index: None,
            entrance: Entrance::default(),
            description: None,
            on_hover_change: None,
            on_selection_change: None,
        }
    }

    pub fn area(id: impl Into<ElementId>, series: Vec<Series>) -> Self {
        Self::new(id, CartesianKind::Area, series)
    }

    pub fn line(id: impl Into<ElementId>, series: Vec<Series>) -> Self {
        Self::new(id, CartesianKind::Line, series)
    }

    pub fn bar(id: impl Into<ElementId>, series: Vec<Series>) -> Self {
        Self::new(id, CartesianKind::Bar, series)
    }

    /// X-axis category labels, one per data index.
    pub fn labels<S: Into<SharedString>>(mut self, labels: impl IntoIterator<Item = S>) -> Self {
        self.labels = Rc::new(labels.into_iter().map(Into::into).collect());
        self
    }

    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    /// Plot height (default 220px).
    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }

    /// Whether legend entries select (emphasise) a series (default true).
    pub fn clickable_legend(mut self, clickable: bool) -> Self {
        self.clickable_legend = clickable;
        self
    }

    pub fn tooltip(mut self, tooltip: bool) -> Self {
        self.tooltip = tooltip;
        self
    }

    pub fn y_axis(mut self, y_axis: bool) -> Self {
        self.y_axis = y_axis;
        self
    }

    /// `false` makes the chart decorative: no crosshair, tooltip, or legend clicks.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Controlled crosshair position; overrides pointer hover.
    pub fn marker_index(mut self, index: Option<usize>) -> Self {
        self.marker_index = index;
        self
    }

    /// Accessible summary of what the chart shows.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn on_hover_change(mut self, f: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover_change = Some(Rc::new(f));
        self
    }

    pub fn on_selection_change(
        mut self,
        f: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selection_change = Some(Rc::new(f));
        self
    }

    entrance_builders!();

    fn len(&self) -> usize {
        self.series.iter().map(|s| s.values.len()).max().unwrap_or(0)
    }

    /// Horizontal position (0..1) of data index `i`.
    fn column_fraction(&self, i: usize) -> f32 {
        let n = self.len().max(1);
        match self.kind {
            CartesianKind::Bar => (i as f32 + 0.5) / n as f32,
            _ if n == 1 => 0.5,
            _ => i as f32 / (n - 1) as f32,
        }
    }
}

fn set_hover(
    state: &Entity<ChartState>,
    idx: Option<usize>,
    cb: &Option<IndexHandler>,
    window: &mut Window,
    cx: &mut App,
) {
    let changed = state.update(cx, |s, cx| {
        let changed = s.hover != idx;
        if changed {
            s.hover = idx;
            cx.notify();
        }
        changed
    });
    if let (true, Some(cb)) = (changed, cb) {
        cb(&idx, window, cx);
    }
}

fn legend_toggle(state: Entity<ChartState>, cb: Option<IndexHandler>) -> LegendClick {
    Rc::new(move |i, window, cx| {
        let selected = state.update(cx, |s, cx| {
            s.selected = if s.selected == Some(i) { None } else { Some(i) };
            cx.notify();
            s.selected
        });
        if let Some(cb) = &cb {
            cb(&selected, window, cx);
        }
    })
}

impl RenderOnce for DitherChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            ChartState::new(self.entrance.animate, self.entrance.replay_token)
        });
        let pixel = self.pixel.max(px(1.0));
        let height = pixel * (self.height / pixel).floor();
        let n = self.len();
        let (kind, stacked, bloom, interactive) = (self.kind, self.stacked, self.bloom, self.interactive);
        let glide_spring = Spring::visual(Duration::from_millis(260), 0.1).unwrap_or_default();

        let (progress, hover, selected, glide_x, plot) = state.update(cx, |s, cx| {
            let progress = s.progress(&self.entrance, window, cx);
            let hover = if interactive {
                self.marker_index.or(s.hover).filter(|i| *i < n)
            } else {
                None
            };
            let glide_x = match (hover, s.plot) {
                (Some(i), Some(b)) => {
                    let (w, _) = grid_size(b, pixel);
                    let core = Cartesian::new(kind, &self.series);
                    let target = (f32::from(pixel) * core.column_x(i, w)) as f64;
                    let glide = s.glide.get_or_insert_with(|| ValueState::new(target));
                    Some(
                        glide
                            .frame(target, Transition::Spring(glide_spring), window, cx)
                            .value() as f32,
                    )
                }
                _ => {
                    s.glide = None;
                    None
                }
            };
            (progress, hover, s.selected, glide_x, s.plot)
        });

        let theme = cx.theme();
        let (muted, fg) = (theme.muted_foreground, theme.foreground);
        let scale = {
            let mut c = Cartesian::new(kind, &self.series);
            c.stacked = stacked;
            c.scale()
        };

        // Dithered plot.
        let series = self.series.clone();
        let canvas = pixel_canvas(pixel, move |w, h| {
            let mut c = Cartesian::new(kind, &series);
            c.stacked = stacked;
            c.progress = progress;
            c.hover = hover;
            c.selected = selected;
            c.bloom = bloom;
            c.render(w, h)
        })
        .absolute()
        .size_full();

        let tooltip_el = match (self.tooltip, hover, glide_x, plot) {
            (true, Some(i), Some(x), Some(b)) => {
                let rows = self
                    .series
                    .iter()
                    .filter(|s| !s.hidden)
                    .filter_map(|s| {
                        s.values
                            .get(i)
                            .map(|v| (s.tone, SharedString::from(s.label.clone()), *v))
                    })
                    .collect();
                let plot_w = f32::from(b.size.width);
                let tip = tooltip(self.labels.get(i).cloned(), rows, cx)
                    .absolute()
                    .top(px(6.0));
                Some(if x < plot_w * 0.62 {
                    tip.left(px(x + 14.0))
                } else {
                    tip.right(px(plot_w - x + 14.0))
                })
            }
            _ => None,
        };

        let (st_move, st_hover, st_prepaint) = (state.clone(), state.clone(), state.clone());
        let (cb_move, cb_hover) = (self.on_hover_change.clone(), self.on_hover_change.clone());
        let hit_series = self.series.clone();
        let plot_el = div()
            .id("plot")
            .test_support()
            .relative()
            .w_full()
            .h(height)
            .on_prepaint(move |bounds, _, cx| st_prepaint.update(cx, |s, _| s.plot = Some(bounds)))
            .child(canvas)
            .children(tooltip_el)
            .when(interactive, |d| {
                d.on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
                    let Some(b) = st_move.read(cx).plot else { return };
                    let (w, _) = grid_size(b, pixel);
                    let core = Cartesian::new(kind, &hit_series);
                    let idx = core.index_at((e.position.x - b.origin.x) / pixel, w);
                    set_hover(&st_move, idx, &cb_move, window, cx);
                })
                .on_hover(move |hovered, window, cx| {
                    if !hovered {
                        set_hover(&st_hover, None, &cb_hover, window, cx);
                    }
                })
            });

        // X labels, thinned to avoid collisions.
        let max_labels = plot.map_or(n, |b| (f32::from(b.size.width) / 44.0).floor().max(1.0) as usize);
        let step = n.div_ceil(max_labels.max(1)).max(1);
        let x_labels = div().relative().w_full().h(px(18.0)).mt_1().text_xs().children(
            self.labels
                .iter()
                .enumerate()
                .take(n)
                .filter(|(i, _)| i % step == 0 || hover == Some(*i))
                .map(|(i, l)| {
                    h_flex()
                        .absolute()
                        .left(relative(self.column_fraction(i)))
                        .w(px(0.0))
                        .justify_center()
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_color(if hover == Some(i) { fg } else { muted })
                                .child(l.clone()),
                        )
                }),
        );

        let axis_w = px(scale
            .ticks
            .iter()
            .map(|t| format_value(*t).len())
            .max()
            .unwrap_or(1) as f32
            * 7.0
            + 10.0);
        let y_axis = div()
            .relative()
            .flex_none()
            .w(axis_w)
            .h(height)
            .text_xs()
            .text_color(muted)
            .children(scale.ticks.iter().map(|t| {
                div()
                    .absolute()
                    .right(px(8.0))
                    .top(relative(1.0 - (*t / scale.max) as f32))
                    .mt(px(-8.0))
                    .child(format_value(*t))
            }));

        let legend_el = self.legend.then(|| {
            let entries = self
                .series
                .iter()
                .enumerate()
                .map(|(i, s)| LegendEntry {
                    label: s.label.clone().into(),
                    tone: s.tone,
                    selected: selected == Some(i),
                    dimmed: s.hidden || selected.is_some_and(|sel| sel != i),
                })
                .collect();
            let click = (interactive && self.clickable_legend)
                .then(|| legend_toggle(state.clone(), self.on_selection_change.clone()));
            legend(entries, click, cx)
        });

        let description = self.description.clone().unwrap_or_else(|| {
            let names: Vec<&str> = self.series.iter().map(|s| s.label.as_str()).collect();
            format!("{:?} chart of {} over {} points", kind, names.join(", "), n).into()
        });

        v_flex()
            .id(self.id)
            .test_support()
            .role(Role::Figure)
            .aria_label(description)
            .w_full()
            .gap_3()
            .children(legend_el)
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .when(self.y_axis, |d| d.child(y_axis))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(plot_el)
                            .when(!self.labels.is_empty(), |d| d.child(x_labels)),
                    ),
            )
    }
}

/// Pie or donut chart.
#[derive(IntoElement)]
pub struct DitherPie {
    id: ElementId,
    slices: Rc<Vec<Slice>>,
    inner_radius: f32,
    size: Pixels,
    pixel: Pixels,
    bloom: Bloom,
    legend: bool,
    entrance: Entrance,
    on_hover_change: Option<IndexHandler>,
}

impl DitherPie {
    pub fn new(id: impl Into<ElementId>, slices: Vec<Slice>) -> Self {
        Self {
            id: id.into(),
            slices: Rc::new(slices),
            inner_radius: 0.0,
            size: px(200.0),
            pixel: px(3.0),
            bloom: Bloom::Off,
            legend: true,
            entrance: Entrance::default(),
            on_hover_change: None,
        }
    }

    /// Hole radius as a fraction of the outer radius (e.g. `0.55`).
    pub fn inner_radius(mut self, fraction: f32) -> Self {
        self.inner_radius = fraction;
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }

    pub fn on_hover_change(mut self, f: impl Fn(&Option<usize>, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover_change = Some(Rc::new(f));
        self
    }

    entrance_builders!();

    fn core(slices: &[Slice], inner: f32, progress: f32, hover: Option<usize>, bloom: Bloom) -> Pie<'_> {
        let mut p = Pie::new(slices);
        p.inner_radius = inner;
        p.progress = progress;
        p.hover = hover;
        p.bloom = bloom;
        p
    }
}

impl RenderOnce for DitherPie {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            ChartState::new(self.entrance.animate, self.entrance.replay_token)
        });
        let pixel = self.pixel.max(px(1.0));
        let side = pixel * (self.size / pixel).floor();
        let (progress, hover) = state.update(cx, |s, cx| (s.progress(&self.entrance, window, cx), s.hover));
        let (inner, bloom) = (self.inner_radius, self.bloom);
        let theme = cx.theme();

        let slices = self.slices.clone();
        let canvas = pixel_canvas(pixel, move |w, h| {
            DitherPie::core(&slices, inner, progress, hover, bloom).render(w, h)
        })
        .absolute()
        .size_full();

        let center = (inner >= 0.45).then(|| {
            let (label, value): (SharedString, f64) = match hover.and_then(|i| self.slices.get(i)) {
                Some(s) => (s.label.clone().into(), s.value),
                None => ("Total".into(), self.slices.iter().map(|s| s.value).sum()),
            };
            v_flex()
                .absolute()
                .inset_0()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(format_value(value)),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(label))
        });

        let (st_move, st_hover, st_prepaint) = (state.clone(), state.clone(), state.clone());
        let (cb_move, cb_hover) = (self.on_hover_change.clone(), self.on_hover_change.clone());
        let hit = self.slices.clone();
        let plot = div()
            .id("pie")
            .test_support()
            .relative()
            .size(side)
            .on_prepaint(move |bounds, _, cx| st_prepaint.update(cx, |s, _| s.plot = Some(bounds)))
            .child(canvas)
            .children(center)
            .on_mouse_move(move |e: &MouseMoveEvent, window, cx| {
                let Some(b) = st_move.read(cx).plot else { return };
                let (w, h) = grid_size(b, pixel);
                let geo = DitherPie::core(&hit, inner, 1.0, None, Bloom::Off).geometry(w, h);
                let idx = geo.index_at(
                    (e.position.x - b.origin.x) / pixel,
                    (e.position.y - b.origin.y) / pixel,
                );
                set_hover(&st_move, idx, &cb_move, window, cx);
            })
            .on_hover(move |hovered, window, cx| {
                if !hovered {
                    set_hover(&st_hover, None, &cb_hover, window, cx);
                }
            });

        let legend_el = self.legend.then(|| {
            let entries = self
                .slices
                .iter()
                .enumerate()
                .map(|(i, s)| LegendEntry {
                    label: s.label.clone().into(),
                    tone: s.tone,
                    selected: hover == Some(i),
                    dimmed: hover.is_some_and(|h| h != i),
                })
                .collect();
            legend(entries, None, cx)
        });

        let summary: Vec<String> = self
            .slices
            .iter()
            .map(|s| format!("{} {}", s.label, format_value(s.value)))
            .collect();
        v_flex()
            .id(self.id)
            .test_support()
            .role(Role::Figure)
            .aria_label(SharedString::from(format!("Pie chart: {}", summary.join(", "))))
            .items_center()
            .gap_3()
            .child(plot)
            .children(legend_el)
    }
}

/// Radar (spider) chart.
#[derive(IntoElement)]
pub struct DitherRadar {
    id: ElementId,
    series: Rc<Vec<Series>>,
    axes: Rc<Vec<SharedString>>,
    size: Pixels,
    pixel: Pixels,
    bloom: Bloom,
    legend: bool,
    entrance: Entrance,
}

impl DitherRadar {
    pub fn new<S: Into<SharedString>>(
        id: impl Into<ElementId>,
        series: Vec<Series>,
        axes: impl IntoIterator<Item = S>,
    ) -> Self {
        Self {
            id: id.into(),
            series: Rc::new(series),
            axes: Rc::new(axes.into_iter().map(Into::into).collect()),
            size: px(200.0),
            pixel: px(3.0),
            bloom: Bloom::Off,
            legend: true,
            entrance: Entrance::default(),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }

    entrance_builders!();
}

impl RenderOnce for DitherRadar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            ChartState::new(self.entrance.animate, self.entrance.replay_token)
        });
        let pixel = self.pixel.max(px(1.0));
        let cells = (self.size / pixel).floor() as usize;
        let side = pixel * cells as f32;
        let (progress, selected) =
            state.update(cx, |s, cx| (s.progress(&self.entrance, window, cx), s.selected));
        let bloom = self.bloom;
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let series = self.series.clone();
        let canvas = pixel_canvas(pixel, move |w, h| {
            let mut r = Radar::new(&series);
            r.progress = progress;
            r.selected = selected;
            r.bloom = bloom;
            r.render(w, h)
        })
        .absolute()
        .size_full();

        // Axis labels positioned from the same geometry the pixels use.
        let geo = Radar::new(&self.series).geometry(cells, cells);
        let px_f = f32::from(pixel);
        let side_f = f32::from(side);
        let labels = self
            .axes
            .iter()
            .zip(geo.axis_tips.iter())
            .map(|(label, &(tx, ty))| {
                let (dx, dy) = (tx - geo.center.0, ty - geo.center.1);
                let (x, y) = (tx * px_f, ty * px_f);
                let el = div().absolute().whitespace_nowrap().text_xs().text_color(muted);
                let el = if dx > 2.0 {
                    el.left(px(x + 6.0))
                } else if dx < -2.0 {
                    el.right(px(side_f - x + 6.0))
                } else {
                    el.left(px(x)).w(px(0.0)).flex().justify_center()
                };
                let el = if dy < -2.0 && dx.abs() <= 2.0 {
                    el.bottom(px(side_f - y + 4.0))
                } else if dy > 2.0 && dx.abs() <= 2.0 {
                    el.top(px(y + 4.0))
                } else {
                    el.top(px(y - 8.0))
                };
                el.child(label.clone())
            });

        let legend_el = self.legend.then(|| {
            let entries = self
                .series
                .iter()
                .enumerate()
                .map(|(i, s)| LegendEntry {
                    label: s.label.clone().into(),
                    tone: s.tone,
                    selected: selected == Some(i),
                    dimmed: selected.is_some_and(|sel| sel != i),
                })
                .collect();
            legend(entries, Some(legend_toggle(state.clone(), None)), cx)
        });

        let names: Vec<&str> = self.series.iter().map(|s| s.label.as_str()).collect();
        v_flex()
            .id(self.id)
            .test_support()
            .role(Role::Figure)
            .aria_label(SharedString::from(format!(
                "Radar chart comparing {}",
                names.join(", ")
            )))
            .items_center()
            .gap_3()
            .child(
                div()
                    .px(px(56.0))
                    .py(px(20.0))
                    .child(div().relative().size(side).child(canvas).children(labels)),
            )
            .children(legend_el)
    }
}

/// A chrome-free dithered sparkline.
#[derive(IntoElement)]
pub struct DitherSparkline {
    id: ElementId,
    values: Rc<Vec<f64>>,
    tone: Tone,
    height: Pixels,
    pixel: Pixels,
    hovered: Option<bool>,
    entrance: Entrance,
    bloom: Bloom,
}

impl DitherSparkline {
    pub fn new(id: impl Into<ElementId>, values: Vec<f64>, tone: Tone) -> Self {
        Self {
            id: id.into(),
            values: Rc::new(values),
            tone,
            height: px(40.0),
            pixel: px(2.0),
            hovered: None,
            entrance: Entrance::default(),
            bloom: Bloom::Off,
        }
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    /// Parent-driven hover lift; when unset the sparkline tracks its own hover.
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = Some(hovered);
        self
    }

    entrance_builders!();
}

impl RenderOnce for DitherSparkline {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            ChartState::new(self.entrance.animate, self.entrance.replay_token)
        });
        let (progress, self_hover) = state.update(cx, |s, cx| {
            (s.progress(&self.entrance, window, cx), s.hover.is_some())
        });
        let hovered = self.hovered.unwrap_or(self_hover);
        let (values, tone, bloom) = (self.values.clone(), self.tone, self.bloom);
        let st = state.clone();
        div()
            .id(self.id)
            .test_support()
            .role(Role::Image)
            .aria_label("Sparkline")
            .relative()
            .w_full()
            .h(self.height)
            .on_hover(move |h, _, cx| {
                st.update(cx, |s, cx| {
                    s.hover = h.then_some(0);
                    cx.notify();
                })
            })
            .child(
                pixel_canvas(self.pixel, move |w, h| {
                    let mut g = sparkline(&values, tone, w, h, hovered, progress);
                    if !hovered {
                        g.bloom(bloom);
                    }
                    g
                })
                .absolute()
                .size_full(),
            )
    }
}

/// Theme-aware color for a tone, for use in surrounding chrome.
pub fn tone_color(tone: Tone) -> Hsla {
    hsla(tone.base())
}
