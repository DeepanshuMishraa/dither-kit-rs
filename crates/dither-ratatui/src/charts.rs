//! Chart widgets: area, line, bar, pie/donut, radar, sparkline.

use dither_core::{
    Bloom, Cartesian, CartesianKind, Pie, Radar, Series, Slice, Tone, format_value, sparkline,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Widget};

use crate::raster::{color, paint, pixel_size};
use crate::theme::Theme;
use crate::widgets::{Legend, LegendItem};

/// Area, line, or bar chart with axes, legend, crosshair, and tooltip.
#[derive(Clone, Debug)]
pub struct Chart<'a> {
    pub kind: CartesianKind,
    pub series: &'a [Series],
    pub labels: &'a [&'a str],
    pub stacked: bool,
    /// Entrance progress 0..1 (ease it yourself, e.g. `dither_core::ease::out_cubic`).
    pub progress: f32,
    pub hover: Option<usize>,
    pub selected: Option<usize>,
    pub bloom: Bloom,
    pub legend: bool,
    pub tooltip: bool,
    pub y_axis: bool,
    pub theme: Theme,
}

impl<'a> Chart<'a> {
    pub fn new(kind: CartesianKind, series: &'a [Series]) -> Self {
        Self {
            kind,
            series,
            labels: &[],
            stacked: false,
            progress: 1.0,
            hover: None,
            selected: None,
            bloom: Bloom::Off,
            legend: true,
            tooltip: true,
            y_axis: true,
            theme: Theme::default(),
        }
    }

    pub fn area(series: &'a [Series]) -> Self {
        Self::new(CartesianKind::Area, series)
    }

    pub fn line(series: &'a [Series]) -> Self {
        Self::new(CartesianKind::Line, series)
    }

    pub fn bar(series: &'a [Series]) -> Self {
        Self::new(CartesianKind::Bar, series)
    }

    pub fn labels(mut self, labels: &'a [&'a str]) -> Self {
        self.labels = labels;
        self
    }

    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    pub fn hover(mut self, hover: Option<usize>) -> Self {
        self.hover = hover;
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn bloom(mut self, bloom: Bloom) -> Self {
        self.bloom = bloom;
        self
    }

    pub fn legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    fn core(&self) -> Cartesian<'a> {
        let mut c = Cartesian::new(self.kind, self.series);
        c.stacked = self.stacked;
        c.progress = self.progress;
        c.hover = self.hover;
        c.selected = self.selected;
        c.bloom = self.bloom;
        c
    }

    fn legend_widget(&self) -> Legend<'a> {
        Legend::new(
            self.series
                .iter()
                .enumerate()
                .map(|(i, s)| LegendItem {
                    label: s.label.as_str(),
                    tone: s.tone,
                    hidden: s.hidden,
                    selected: self.selected == Some(i),
                })
                .collect(),
        )
        .centered()
    }

    /// The legend row inside `area`, if the legend is shown.
    pub fn legend_area(&self, area: Rect) -> Option<Rect> {
        (self.legend && area.height > 3).then(|| Rect::new(area.x, area.y, area.width, 1))
    }

    /// Legend item under a mouse position (for click-to-toggle).
    pub fn legend_hit(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        self.legend_area(area)
            .and_then(|r| self.legend_widget().hit(r, col, row))
    }

    /// The cell rectangle holding the dithered plot.
    pub fn plot_area(&self, area: Rect) -> Rect {
        let mut r = area;
        if self.legend_area(area).is_some() {
            r.y += 2;
            r.height = r.height.saturating_sub(2);
        }
        if !self.labels.is_empty() {
            r.height = r.height.saturating_sub(1);
        }
        if self.y_axis {
            let w = self.axis_width();
            r.x += w;
            r.width = r.width.saturating_sub(w);
        }
        r
    }

    fn axis_width(&self) -> u16 {
        self.core()
            .scale()
            .ticks
            .iter()
            .map(|t| format_value(*t).len())
            .max()
            .unwrap_or(1) as u16
            + 1
    }

    /// Data index under terminal column `col`, for mouse hover.
    pub fn index_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        let plot = self.plot_area(area);
        if !plot.contains((col, row).into()) {
            return None;
        }
        self.core().index_at((col - plot.x) as f32, plot.width as usize)
    }

    /// Number of data points (for keyboard navigation).
    pub fn len(&self) -> usize {
        self.core().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Widget for Chart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 6 || area.height < 3 {
            return;
        }
        let muted = Style::default().fg(color(self.theme.muted));
        if let Some(r) = self.legend_area(area) {
            self.legend_widget().render(r, buf);
        }
        let plot = self.plot_area(area);
        let core = self.core();
        let (pw, ph) = pixel_size(plot);
        paint(&core.render(pw, ph), plot, buf, self.theme.background);

        // Y axis tick labels, right-aligned against the plot.
        let scale = core.scale();
        if self.y_axis {
            for &t in &scale.ticks {
                let row = plot.y + (scale.y(t, ph) / 2.0).floor() as u16;
                let text = format_value(t);
                let x = plot.x.saturating_sub(text.len() as u16 + 1);
                buf.set_string(x, row.min(plot.bottom().saturating_sub(1)), text, muted);
            }
        }
        // X labels, skipping any that would collide.
        if !self.labels.is_empty() {
            let row = plot.bottom();
            let mut next_free = plot.x;
            for (i, label) in self.labels.iter().enumerate().take(core.len()) {
                let cx = plot.x as f32 + core.column_x(i, pw);
                let len = label.chars().count() as u16;
                let x = (cx - len as f32 / 2.0).round().max(plot.x as f32) as u16;
                let x = x.min(plot.right().saturating_sub(len));
                if x < next_free {
                    continue;
                }
                let style = if self.hover == Some(i) {
                    Style::default()
                        .fg(color(self.theme.foreground))
                        .add_modifier(Modifier::BOLD)
                } else {
                    muted
                };
                buf.set_string(x, row, *label, style);
                next_free = x + len + 1;
            }
        }
        if let (true, Some(i)) = (self.tooltip, self.hover.filter(|i| *i < core.len())) {
            let anchor = plot.x + core.column_x(i, pw).round() as u16;
            let title = self.labels.get(i).copied();
            let rows: Vec<(Tone, &str, f64)> = self
                .series
                .iter()
                .filter(|s| !s.hidden)
                .filter_map(|s| s.values.get(i).map(|v| (s.tone, s.label.as_str(), *v)))
                .collect();
            tooltip(buf, plot, anchor, title, &rows, &self.theme);
        }
    }
}

fn tooltip(
    buf: &mut Buffer,
    plot: Rect,
    anchor: u16,
    title: Option<&str>,
    rows: &[(Tone, &str, f64)],
    theme: &Theme,
) {
    let value_w = rows.iter().map(|r| format_value(r.2).len()).max().unwrap_or(0);
    let label_w = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
    let inner_w = (label_w + value_w + 5).max(title.map_or(0, |t| t.chars().count())) as u16;
    let h = rows.len() as u16 + 2 + title.is_some() as u16;
    let w = inner_w + 2;
    if plot.width < w + 2 || plot.height < h {
        return;
    }
    let x = if anchor + 2 + w <= plot.right() {
        anchor + 2
    } else {
        anchor.saturating_sub(w + 1).max(plot.x)
    };
    let r = Rect::new(x, plot.y, w, h);
    Clear.render(r, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color(theme.border)))
        .style(Style::default().bg(color(theme.popover)))
        .render(r, buf);
    let mut y = r.y + 1;
    if let Some(t) = title {
        buf.set_string(
            r.x + 1,
            y,
            t,
            Style::default()
                .fg(color(theme.foreground))
                .add_modifier(Modifier::BOLD),
        );
        y += 1;
    }
    for (tone, label, v) in rows {
        let value = format_value(*v);
        let line = Line::from(vec![
            Span::styled("■ ", Style::default().fg(color(tone.base()))),
            Span::styled(
                format!("{label:<label_w$}"),
                Style::default().fg(color(theme.muted)),
            ),
            Span::styled(
                format!(" {value:>value_w$}"),
                Style::default().fg(color(theme.foreground)),
            ),
        ]);
        buf.set_line(r.x + 1, y, &line, inner_w);
        y += 1;
    }
}

/// Pie or donut chart with an optional centre label and legend.
#[derive(Clone, Debug)]
pub struct PieChart<'a> {
    pub slices: &'a [Slice],
    pub inner_radius: f32,
    pub progress: f32,
    pub hover: Option<usize>,
    pub bloom: Bloom,
    pub legend: bool,
    pub theme: Theme,
}

impl<'a> PieChart<'a> {
    pub fn new(slices: &'a [Slice]) -> Self {
        Self {
            slices,
            inner_radius: 0.0,
            progress: 1.0,
            hover: None,
            bloom: Bloom::Off,
            legend: true,
            theme: Theme::default(),
        }
    }

    pub fn donut(mut self, inner_radius: f32) -> Self {
        self.inner_radius = inner_radius;
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    pub fn hover(mut self, hover: Option<usize>) -> Self {
        self.hover = hover;
        self
    }

    pub fn bloom(mut self, bloom: Bloom) -> Self {
        self.bloom = bloom;
        self
    }

    fn core(&self) -> Pie<'a> {
        let mut p = Pie::new(self.slices);
        p.inner_radius = self.inner_radius;
        p.progress = self.progress;
        p.hover = self.hover;
        p.bloom = self.bloom;
        p
    }

    /// The square-ish cell area that holds the pie.
    pub fn plot_area(&self, area: Rect) -> Rect {
        let h = area.height.saturating_sub(if self.legend { 2 } else { 0 });
        let side_cols = area.width.min(h * 2);
        Rect::new(
            area.x + (area.width - side_cols) / 2,
            area.y,
            side_cols,
            side_cols / 2,
        )
    }

    /// Slice under a mouse position.
    pub fn index_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        let plot = self.plot_area(area);
        if !plot.contains((col, row).into()) {
            return None;
        }
        let (w, h) = pixel_size(plot);
        let geo = self.core().geometry(w, h);
        geo.index_at((col - plot.x) as f32 + 0.5, ((row - plot.y) * 2) as f32 + 1.0)
    }
}

impl Widget for PieChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let plot = self.plot_area(area);
        let (w, h) = pixel_size(plot);
        paint(&self.core().render(w, h), plot, buf, self.theme.background);
        // Centre label inside a donut: hovered slice or the total.
        if self.inner_radius >= 0.45 {
            let (label, value) = match self.hover.and_then(|i| self.slices.get(i)) {
                Some(s) => (s.label.clone(), s.value),
                None => ("Total".to_string(), self.slices.iter().map(|s| s.value).sum()),
            };
            let cy = plot.y + plot.height / 2;
            let value = format_value(value);
            let center = |s: &str| plot.x + (plot.width.saturating_sub(s.chars().count() as u16)) / 2;
            buf.set_string(
                center(&value),
                cy.saturating_sub(1),
                &value,
                Style::default()
                    .fg(color(self.theme.foreground))
                    .add_modifier(Modifier::BOLD),
            );
            buf.set_string(
                center(&label),
                cy,
                &label,
                Style::default().fg(color(self.theme.muted)),
            );
        }
        if self.legend && area.height >= plot.height + 2 {
            let items = self
                .slices
                .iter()
                .enumerate()
                .map(|(i, s)| LegendItem {
                    label: s.label.as_str(),
                    tone: s.tone,
                    hidden: false,
                    selected: self.hover == Some(i),
                })
                .collect();
            Legend::new(items)
                .centered()
                .render(Rect::new(area.x, plot.bottom() + 1, area.width, 1), buf);
        }
    }
}

/// Radar (spider) chart with axis labels.
#[derive(Clone, Debug)]
pub struct RadarChart<'a> {
    pub series: &'a [Series],
    pub axes: &'a [&'a str],
    pub progress: f32,
    pub selected: Option<usize>,
    pub bloom: Bloom,
    pub legend: bool,
    pub theme: Theme,
}

impl<'a> RadarChart<'a> {
    pub fn new(series: &'a [Series], axes: &'a [&'a str]) -> Self {
        Self {
            series,
            axes,
            progress: 1.0,
            selected: None,
            bloom: Bloom::Off,
            legend: true,
            theme: Theme::default(),
        }
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn bloom(mut self, bloom: Bloom) -> Self {
        self.bloom = bloom;
        self
    }
}

impl Widget for RadarChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Leave a margin for axis labels on every side.
        let label_w = self.axes.iter().map(|a| a.chars().count()).max().unwrap_or(0) as u16 + 1;
        let legend_h = if self.legend { 2 } else { 0 };
        let avail_h = area.height.saturating_sub(2 + legend_h);
        let side_cols = area.width.saturating_sub(label_w * 2).min(avail_h * 2);
        let plot = Rect::new(
            area.x + (area.width - side_cols) / 2,
            area.y + 1,
            side_cols,
            side_cols / 2,
        );
        let (w, h) = pixel_size(plot);
        let mut radar = Radar::new(self.series);
        radar.progress = self.progress;
        radar.selected = self.selected;
        radar.bloom = self.bloom;
        paint(&radar.render(w, h), plot, buf, self.theme.background);
        let geo = radar.geometry(w, h);
        let muted = Style::default().fg(color(self.theme.muted));
        for (label, (tx, ty)) in self.axes.iter().zip(geo.axis_tips) {
            let (dx, dy) = (tx - geo.center.0, ty - geo.center.1);
            let len = label.chars().count() as f32;
            let col = plot.x as f32 + tx + dx.signum() * 1.5
                - if dx.abs() < 2.0 {
                    len / 2.0
                } else if dx < 0.0 {
                    len
                } else {
                    0.0
                };
            let row = plot.y as f32
                + (ty / 2.0)
                + if dy < -2.0 {
                    -1.0
                } else if dy > 2.0 {
                    0.5
                } else {
                    0.0
                };
            let (col, row) = (
                col.round().max(area.x as f32) as u16,
                row.round().max(area.y as f32) as u16,
            );
            if row < area.bottom() {
                buf.set_string(col, row, *label, muted);
            }
        }
        if self.legend {
            let items = self
                .series
                .iter()
                .enumerate()
                .map(|(i, s)| LegendItem {
                    label: s.label.as_str(),
                    tone: s.tone,
                    hidden: s.hidden,
                    selected: self.selected == Some(i),
                })
                .collect();
            Legend::new(items).centered().render(
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
                buf,
            );
        }
    }
}

/// A chrome-free dithered sparkline.
#[derive(Clone, Debug)]
pub struct Sparkline<'a> {
    pub values: &'a [f64],
    pub tone: Tone,
    pub hovered: bool,
    pub progress: f32,
    pub theme: Theme,
}

impl<'a> Sparkline<'a> {
    pub fn new(values: &'a [f64], tone: Tone) -> Self {
        Self {
            values,
            tone,
            hovered: false,
            progress: 1.0,
            theme: Theme::default(),
        }
    }

    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }
}

impl Widget for Sparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (w, h) = pixel_size(area);
        paint(
            &sparkline(self.values, self.tone, w, h, self.hovered, self.progress),
            area,
            buf,
            self.theme.background,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    fn data() -> Vec<Series> {
        vec![
            Series::new(
                "Desktop",
                Tone::Blue,
                vec![186.0, 305.0, 237.0, 73.0, 209.0, 214.0],
            ),
            Series::new(
                "Mobile",
                Tone::Pink,
                vec![80.0, 200.0, 120.0, 190.0, 130.0, 140.0],
            ),
        ]
    }

    #[test]
    fn chart_draws_axes_labels_and_tooltip() {
        let s = data();
        let labels = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
        let area = Rect::new(0, 0, 60, 20);
        let mut buf = Buffer::empty(area);
        Chart::area(&s)
            .labels(&labels)
            .hover(Some(2))
            .render(area, &mut buf);
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Jan") && text.contains("Jun"));
        assert!(
            text.contains("Desktop") && text.contains("237"),
            "tooltip shows hovered values"
        );
        assert!(text.contains('▀') || text.contains('▄'));
    }

    #[test]
    fn mouse_maps_to_data_index() {
        let s = data();
        let chart = Chart::bar(&s);
        let area = Rect::new(0, 0, 60, 20);
        let plot = chart.plot_area(area);
        assert_eq!(chart.index_at(area, plot.x, plot.y + 2), Some(0));
        assert_eq!(chart.index_at(area, plot.right() - 1, plot.y + 2), Some(5));
        assert_eq!(chart.index_at(area, 0, 0), None);
    }

    #[test]
    fn pie_hit_test() {
        let slices = [
            Slice::new("A", 1.0, Tone::Green),
            Slice::new("B", 1.0, Tone::Orange),
        ];
        let pie = PieChart::new(&slices);
        let area = Rect::new(0, 0, 40, 22);
        let plot = pie.plot_area(area);
        // Right half is the first slice (clockwise from 12 o'clock).
        let col = plot.x + plot.width * 3 / 4;
        let row = plot.y + plot.height / 2;
        assert_eq!(pie.index_at(area, col, row), Some(0));
    }
}
