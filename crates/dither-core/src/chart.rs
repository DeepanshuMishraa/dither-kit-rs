//! Chart rasterizers: area, line, bar, pie/donut, radar, and sparkline.
//!
//! Each chart renders its *plot area* into a [`PixelGrid`] and returns the
//! geometry a backend needs for chrome (axes, labels, tooltips, hit testing).
//! Text is never rasterized here; backends draw it with native text.

use crate::color::{Rgba, Tone};
use crate::dither::Fill;
use crate::grid::{Bloom, PixelGrid};

/// One data series (dither-kit's `config` entry plus its `dataKey` values).
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub label: String,
    pub tone: Tone,
    pub fill: Fill,
    pub values: Vec<f64>,
    /// Hidden series are skipped (toggled from a clickable legend).
    pub hidden: bool,
}

impl Series {
    pub fn new(label: impl Into<String>, tone: Tone, values: impl Into<Vec<f64>>) -> Self {
        Self {
            label: label.into(),
            tone,
            fill: Fill::Gradient,
            values: values.into(),
            hidden: false,
        }
    }

    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

/// A "nice" linear value scale starting at zero.
#[derive(Clone, Debug, PartialEq)]
pub struct Scale {
    pub max: f64,
    pub ticks: Vec<f64>,
}

impl Scale {
    /// Rounds `max` up to a human-friendly bound with about `count` ticks.
    pub fn nice(max: f64, count: usize) -> Self {
        let max = if max.is_finite() && max > 0.0 { max } else { 1.0 };
        let raw = max / count.max(1) as f64;
        let mag = 10f64.powf(raw.log10().floor());
        let step = [1.0, 2.0, 2.5, 5.0, 10.0]
            .iter()
            .map(|m| m * mag)
            .find(|s| *s >= raw)
            .unwrap_or(10.0 * mag);
        let top = (max / step).ceil() * step;
        let ticks = (0..=((top / step).round() as usize))
            .map(|i| i as f64 * step)
            .collect();
        Self { max: top, ticks }
    }

    /// Pixel row (0 = top) for `value` in a plot `h` pixels tall.
    pub fn y(&self, value: f64, h: usize) -> f32 {
        let span = h.saturating_sub(1) as f32;
        span - (value / self.max).clamp(0.0, 1.0) as f32 * span
    }
}

/// Compact number formatting for axes and tooltips: `950`, `1.2k`, `3.4M`.
pub fn format_value(v: f64) -> String {
    let a = v.abs();
    let (n, suffix) = if a >= 1e9 {
        (v / 1e9, "B")
    } else if a >= 1e6 {
        (v / 1e6, "M")
    } else if a >= 1e3 {
        (v / 1e3, "k")
    } else {
        (v, "")
    };
    if (n - n.round()).abs() < 0.05 {
        format!("{}{suffix}", n.round() as i64)
    } else {
        format!("{n:.1}{suffix}")
    }
}

/// Smooth, overshoot-free interpolation at fractional index `f`.
pub fn interpolate(values: &[f64], f: f32) -> f64 {
    match values.len() {
        0 => 0.0,
        1 => values[0],
        n => {
            let f = f.clamp(0.0, (n - 1) as f32);
            let i = (f.floor() as usize).min(n - 2);
            let t = (f - i as f32) as f64;
            let at = |k: isize| values[k.clamp(0, n as isize - 1) as usize];
            let (p0, p1, p2, p3) = (
                at(i as isize - 1),
                at(i as isize),
                at(i as isize + 1),
                at(i as isize + 2),
            );
            let v = 0.5
                * (2.0 * p1
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t);
            v.clamp(p1.min(p2), p1.max(p2))
        }
    }
}

/// Bresenham line, optionally dashed (`dash` = draw every n-th pixel).
pub fn line(g: &mut PixelGrid, (x0, y0): (i32, i32), (x1, y1): (i32, i32), color: Rgba, dash: u32) {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err, mut step) = (x0, y0, dx + dy, 0u32);
    loop {
        if dash <= 1 || step % dash == 0 {
            g.set(x, y, color);
        }
        step += 1;
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn dot(g: &mut PixelGrid, x: i32, y: i32, ring: Rgba, center: Rgba) {
    for oy in -1..=1 {
        for ox in -1..=1 {
            g.set(x + ox, y + oy, if ox == 0 && oy == 0 { center } else { ring });
        }
    }
}

/// Guide color for grid lines and crosshairs.
pub fn guide() -> Rgba {
    Tone::Grey.shade(0.55).fade(0.45)
}

/// Kind of cartesian chart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CartesianKind {
    #[default]
    Area,
    Line,
    Bar,
}

/// Options for area, line, and bar charts.
#[derive(Clone, Debug)]
pub struct Cartesian<'a> {
    pub kind: CartesianKind,
    pub series: &'a [Series],
    /// Stack series on top of each other (dither-kit's `stackType="stacked"`).
    pub stacked: bool,
    /// Entrance progress, 0..1 (already eased by the caller).
    pub progress: f32,
    /// Hovered data index: draws the crosshair / highlights the band.
    pub hover: Option<usize>,
    /// Emphasised series; others are dimmed.
    pub selected: Option<usize>,
    pub bloom: Bloom,
    /// Fixed scale maximum; computed from the data when `None`.
    pub y_max: Option<f64>,
    pub grid_lines: bool,
    pub tick_count: usize,
}

impl<'a> Cartesian<'a> {
    pub fn new(kind: CartesianKind, series: &'a [Series]) -> Self {
        Self {
            kind,
            series,
            stacked: false,
            progress: 1.0,
            hover: None,
            selected: None,
            bloom: Bloom::Off,
            y_max: None,
            grid_lines: true,
            tick_count: 4,
        }
    }

    pub fn len(&self) -> usize {
        self.series.iter().map(|s| s.values.len()).max().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn scale(&self) -> Scale {
        let visible = self.series.iter().filter(|s| !s.hidden);
        let max = self.y_max.unwrap_or_else(|| {
            if self.stacked {
                (0..self.len())
                    .map(|i| {
                        visible
                            .clone()
                            .map(|s| s.values.get(i).copied().unwrap_or(0.0))
                            .sum::<f64>()
                    })
                    .fold(0.0, f64::max)
            } else {
                visible.flat_map(|s| s.values.iter().copied()).fold(0.0, f64::max)
            }
        });
        Scale::nice(max, self.tick_count)
    }

    /// Pixel x of data index `i` in a plot `w` pixels wide.
    pub fn column_x(&self, i: usize, w: usize) -> f32 {
        let n = self.len().max(1);
        match self.kind {
            CartesianKind::Bar => (i as f32 + 0.5) * w as f32 / n as f32,
            _ if n == 1 => w as f32 / 2.0,
            _ => i as f32 * (w.saturating_sub(1)) as f32 / (n - 1) as f32,
        }
    }

    /// The data index nearest to pixel column `x`.
    pub fn index_at(&self, x: f32, w: usize) -> Option<usize> {
        let n = self.len();
        if n == 0 || w == 0 {
            return None;
        }
        let i = match self.kind {
            CartesianKind::Bar => (x / w as f32 * n as f32).floor(),
            _ => (x / (w.max(2) - 1) as f32 * (n - 1) as f32).round(),
        };
        Some((i.max(0.0) as usize).min(n - 1))
    }

    fn alpha_for(&self, series: usize) -> f32 {
        match self.selected {
            Some(s) if s != series => 0.3,
            _ => 1.0,
        }
    }

    pub fn render(&self, w: usize, h: usize) -> PixelGrid {
        let mut g = PixelGrid::new(w, h);
        if w < 2 || h < 2 {
            return g;
        }
        let scale = self.scale();
        if self.grid_lines {
            for &t in scale.ticks.iter().skip(1) {
                let y = scale.y(t, h).round() as i32;
                line(&mut g, (0, y), (w as i32 - 1, y), guide(), 3);
            }
        }
        line(
            &mut g,
            (0, h as i32 - 1),
            (w as i32 - 1, h as i32 - 1),
            guide(),
            2,
        );

        match self.kind {
            CartesianKind::Area | CartesianKind::Line => self.render_curves(&mut g, &scale),
            CartesianKind::Bar => self.render_bars(&mut g, &scale),
        }
        g.bloom(self.bloom);
        if self.kind != CartesianKind::Bar {
            self.render_hover(&mut g, &scale);
        }
        g
    }

    fn render_curves(&self, g: &mut PixelGrid, scale: &Scale) {
        let (w, h) = (g.width(), g.height());
        let n = self.len();
        let reveal = self.progress.clamp(0.0, 1.0) * w as f32;
        let grow = 0.6 + 0.4 * self.progress.clamp(0.0, 1.0) as f64;
        // Pass 1: geometry. Pass 2: dithered bodies. Pass 3: strokes on top,
        // so overlapping (unstacked) areas never hide each other's outline.
        let mut base = vec![0.0f64; w];
        let mut cols: Vec<Vec<Option<(f32, f32)>>> = vec![vec![None; w]; self.series.len()];
        for x in 0..w {
            if x as f32 > reveal {
                break;
            }
            let f = if n > 1 {
                x as f32 / (w - 1) as f32 * (n - 1) as f32
            } else {
                0.0
            };
            for (si, s) in self.series.iter().enumerate().filter(|(_, s)| !s.hidden) {
                let v = interpolate(&s.values, f) * grow;
                let b = if self.stacked { base[x] } else { 0.0 };
                cols[si][x] = Some((scale.y(b + v, h), scale.y(b, h)));
                if self.stacked {
                    base[x] += v;
                }
            }
        }
        let body_strength = if self.kind == CartesianKind::Area {
            1.0
        } else {
            0.28
        };
        for (si, s) in self.series.iter().enumerate() {
            let alpha = self.alpha_for(si);
            for (x, col) in cols[si].iter().enumerate() {
                let Some((top, bottom)) = *col else { continue };
                let xi = x as i32;
                // Bright under the curve, fading toward the baseline.
                let span = (h - 1) as f32 - top;
                for y in (top.ceil() as i32 + 1)..=(bottom as i32) {
                    let t = 1.0 - (y as f32 - top) / span.max(1.0);
                    if s.fill.covers(xi, y, (0.1 + 0.9 * t) * body_strength) {
                        g.set(xi, y, s.tone.shade(0.32 + 0.4 * t).fade(alpha));
                    }
                }
            }
        }
        for (si, s) in self.series.iter().enumerate() {
            let stroke = s.tone.shade(0.78).fade(self.alpha_for(si));
            let mut prev: Option<i32> = None;
            for (x, col) in cols[si].iter().enumerate() {
                let Some((top, _)) = *col else { continue };
                // Join vertically to the previous column so steep slopes stay continuous.
                let ty = top.round() as i32;
                let from = prev.unwrap_or(ty);
                for y in from.min(ty)..=from.max(ty) {
                    g.set(x as i32, y, stroke);
                }
                prev = Some(ty);
            }
        }
        // Data point markers on line charts.
        if self.kind == CartesianKind::Line && w / n.max(1) >= 6 {
            for (si, s) in self.series.iter().enumerate().filter(|(_, s)| !s.hidden) {
                for (i, v) in s.values.iter().enumerate() {
                    let x = self.column_x(i, w);
                    if x > reveal {
                        break;
                    }
                    let b = if self.stacked {
                        self.stack_below(si, i)
                    } else {
                        0.0
                    };
                    let y = scale.y((b + v) * grow, h).round() as i32;
                    let c = s.tone.shade(0.78).fade(self.alpha_for(si));
                    dot(g, x.round() as i32, y, c, c);
                }
            }
        }
    }

    fn stack_below(&self, series: usize, i: usize) -> f64 {
        self.series[..series]
            .iter()
            .filter(|s| !s.hidden)
            .map(|s| s.values.get(i).copied().unwrap_or(0.0))
            .sum()
    }

    fn render_hover(&self, g: &mut PixelGrid, scale: &Scale) {
        let Some(i) = self.hover.filter(|i| *i < self.len()) else {
            return;
        };
        let (w, h) = (g.width(), g.height());
        let x = self.column_x(i, w).round() as i32;
        line(g, (x, 0), (x, h as i32 - 1), Tone::Grey.shade(0.75).fade(0.7), 2);
        for (si, s) in self.series.iter().enumerate().filter(|(_, s)| !s.hidden) {
            let Some(v) = s.values.get(i) else { continue };
            let b = if self.stacked {
                self.stack_below(si, i)
            } else {
                0.0
            };
            let y = scale.y(b + v, h).round() as i32;
            dot(g, x, y, s.tone.shade(0.7), s.tone.shade(0.98));
        }
    }

    fn render_bars(&self, g: &mut PixelGrid, scale: &Scale) {
        let (w, h) = (g.width(), g.height());
        let n = self.len();
        if n == 0 {
            return;
        }
        let visible: Vec<usize> = (0..self.series.len())
            .filter(|i| !self.series[*i].hidden)
            .collect();
        let k = if self.stacked { 1 } else { visible.len().max(1) };
        let band = w as f32 / n as f32;
        let group = (band * 0.72).max(1.0);
        let gap = if group / k as f32 > 3.0 { 1.0 } else { 0.0 };
        let bar_w = ((group - gap * (k as f32 - 1.0)) / k as f32).max(1.0);
        let p = self.progress.clamp(0.0, 1.0) as f64;
        for i in 0..n {
            let x0 = i as f32 * band + (band - group) / 2.0;
            let band_alpha = match self.hover {
                Some(hv) if hv != i => 0.45,
                _ => 1.0,
            };
            let mut base = 0.0;
            for (slot, &si) in visible.iter().enumerate() {
                let s = &self.series[si];
                let v = s.values.get(i).copied().unwrap_or(0.0) * p;
                let left = if self.stacked {
                    x0
                } else {
                    x0 + slot as f32 * (bar_w + gap)
                };
                let (bx0, bx1) = (left.round() as i32, (left + bar_w).round() as i32 - 1);
                let top = scale.y(base + v, h);
                let bottom = scale.y(base, h) as i32;
                let alpha = self.alpha_for(si) * band_alpha;
                let ty = top.round() as i32;
                for y in ty..=bottom {
                    let t = 1.0 - (y as f32 - top) / ((h - 1) as f32 - top).max(1.0);
                    for x in bx0..=bx1.max(bx0) {
                        let lit = y == ty || s.fill.covers(x, y, 0.3 + 0.7 * t);
                        if lit {
                            let level = if y == ty { 0.82 } else { 0.32 + 0.38 * t };
                            g.set(x, y, s.tone.shade(level).fade(alpha));
                        }
                    }
                }
                if self.stacked {
                    base += v;
                }
            }
        }
    }
}

/// A sparkline: an area chart with no axes or grid.
pub fn sparkline(values: &[f64], tone: Tone, w: usize, h: usize, hovered: bool, progress: f32) -> PixelGrid {
    let series = [Series::new("", tone, values.to_vec())];
    let mut c = Cartesian::new(CartesianKind::Area, &series);
    c.grid_lines = false;
    c.progress = progress;
    c.bloom = if hovered { Bloom::Low } else { Bloom::Off };
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(0.0, f64::max);
    // Give sparklines headroom so the stroke never touches the top edge.
    c.y_max = Some((hi * 1.1).max(lo + 1e-9));
    let mut g = c.render(w, h);
    // Drop the baseline guide: sparklines are chrome-free.
    for x in 0..w as i32 {
        if g.at(x, h as i32 - 1) == Some(guide()) {
            g.plot(x, h as i32 - 1, None);
        }
    }
    g
}

/// One pie or donut slice.
#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub label: String,
    pub value: f64,
    pub tone: Tone,
    pub fill: Fill,
}

impl Slice {
    pub fn new(label: impl Into<String>, value: f64, tone: Tone) -> Self {
        Self {
            label: label.into(),
            value,
            tone,
            fill: Fill::Gradient,
        }
    }

    pub fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }
}

/// Pie / donut chart options.
#[derive(Clone, Debug)]
pub struct Pie<'a> {
    pub slices: &'a [Slice],
    /// Hole radius as a fraction of the outer radius (0 = pie, 0.5 = donut).
    pub inner_radius: f32,
    pub progress: f32,
    pub hover: Option<usize>,
    pub bloom: Bloom,
}

/// Rendered pie geometry for hit testing and labels.
#[derive(Clone, Debug)]
pub struct PieGeometry {
    pub center: (f32, f32),
    pub radius: f32,
    pub inner: f32,
    /// Start/end angle per slice, radians clockwise from 12 o'clock.
    pub angles: Vec<(f32, f32)>,
}

impl PieGeometry {
    /// Slice under pixel `(x, y)`.
    pub fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        let (dx, dy) = (x - self.center.0, y - self.center.1);
        let r = (dx * dx + dy * dy).sqrt();
        if r < self.inner || r > self.radius + 2.0 {
            return None;
        }
        let a = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
        self.angles.iter().position(|(a0, a1)| a >= *a0 && a < *a1)
    }
}

impl<'a> Pie<'a> {
    pub fn new(slices: &'a [Slice]) -> Self {
        Self {
            slices,
            inner_radius: 0.0,
            progress: 1.0,
            hover: None,
            bloom: Bloom::Off,
        }
    }

    pub fn geometry(&self, w: usize, h: usize) -> PieGeometry {
        let total: f64 = self.slices.iter().map(|s| s.value.max(0.0)).sum();
        let sweep = std::f32::consts::TAU * self.progress.clamp(0.0, 1.0);
        let mut acc = 0.0f32;
        let angles = self
            .slices
            .iter()
            .map(|s| {
                let a = if total > 0.0 {
                    (s.value.max(0.0) / total) as f32 * sweep
                } else {
                    0.0
                };
                let r = (acc, acc + a);
                acc += a;
                r
            })
            .collect();
        let radius = (w.min(h) as f32 / 2.0 - 2.5).max(1.0);
        PieGeometry {
            center: (w as f32 / 2.0, h as f32 / 2.0),
            radius,
            inner: radius * self.inner_radius.clamp(0.0, 0.95),
            angles,
        }
    }

    pub fn render(&self, w: usize, h: usize) -> PixelGrid {
        let geo = self.geometry(w, h);
        let mut g = PixelGrid::new(w, h);
        let multi = self.slices.len() > 1;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (dx, dy) = (x as f32 + 0.5 - geo.center.0, y as f32 + 0.5 - geo.center.1);
                let r = (dx * dx + dy * dy).sqrt();
                let a = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
                let Some(i) = geo.angles.iter().position(|(a0, a1)| a >= *a0 && a < *a1) else {
                    continue;
                };
                let lift = if self.hover == Some(i) { 2.0 } else { 0.0 };
                let outer = geo.radius - 2.0 + lift;
                if r < geo.inner || r > outer {
                    continue;
                }
                let (a0, a1) = geo.angles[i];
                if multi && ((a - a0).min(a1 - a) * r) < 0.9 {
                    continue; // hairline gap between slices
                }
                let s = &self.slices[i];
                let dim = match self.hover {
                    Some(hv) if hv != i => 0.5,
                    _ => 1.0,
                };
                let t = ((r - geo.inner) / (outer - geo.inner).max(1.0)).clamp(0.0, 1.0);
                let rim = outer - r < 1.2;
                if rim || s.fill.covers(x, y, 0.3 + 0.7 * t) {
                    let level = if rim { 0.8 } else { 0.3 + 0.4 * t };
                    g.set(x, y, s.tone.shade(level).fade(dim));
                }
            }
        }
        g.bloom(self.bloom);
        g
    }
}

/// Radar (spider) chart options.
#[derive(Clone, Debug)]
pub struct Radar<'a> {
    /// One value per axis in each series.
    pub series: &'a [Series],
    pub max: Option<f64>,
    pub progress: f32,
    pub selected: Option<usize>,
    pub rings: usize,
    pub bloom: Bloom,
}

/// Rendered radar geometry: centre, radius, and each axis tip for labels.
#[derive(Clone, Debug)]
pub struct RadarGeometry {
    pub center: (f32, f32),
    pub radius: f32,
    pub axis_tips: Vec<(f32, f32)>,
}

impl<'a> Radar<'a> {
    pub fn new(series: &'a [Series]) -> Self {
        Self {
            series,
            max: None,
            progress: 1.0,
            selected: None,
            rings: 4,
            bloom: Bloom::Off,
        }
    }

    fn axes(&self) -> usize {
        self.series.iter().map(|s| s.values.len()).max().unwrap_or(0)
    }

    pub fn geometry(&self, w: usize, h: usize) -> RadarGeometry {
        let n = self.axes().max(1);
        let center = (w as f32 / 2.0, h as f32 / 2.0);
        let radius = (w.min(h) as f32 / 2.0 - 2.0).max(1.0);
        let axis_tips = (0..n)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / n as f32;
                (center.0 + a.cos() * radius, center.1 + a.sin() * radius)
            })
            .collect();
        RadarGeometry {
            center,
            radius,
            axis_tips,
        }
    }

    pub fn render(&self, w: usize, h: usize) -> PixelGrid {
        let geo = self.geometry(w, h);
        let mut g = PixelGrid::new(w, h);
        let n = self.axes();
        if n < 3 {
            return g;
        }
        let c = (geo.center.0.round() as i32, geo.center.1.round() as i32);
        let at = |i: usize, f: f32| {
            let (tx, ty) = geo.axis_tips[i % n];
            (
                (geo.center.0 + (tx - geo.center.0) * f).round() as i32,
                (geo.center.1 + (ty - geo.center.1) * f).round() as i32,
            )
        };
        // Web: rings and spokes.
        for ring in 1..=self.rings.max(1) {
            let f = ring as f32 / self.rings.max(1) as f32;
            for i in 0..n {
                line(&mut g, at(i, f), at(i + 1, f), guide(), 2);
            }
        }
        for i in 0..n {
            line(&mut g, c, at(i, 1.0), guide(), 3);
        }
        let max = self.max.unwrap_or_else(|| {
            Scale::nice(
                self.series
                    .iter()
                    .flat_map(|s| s.values.iter().copied())
                    .fold(0.0, f64::max),
                self.rings,
            )
            .max
        });
        let p = self.progress.clamp(0.0, 1.0);
        for (si, s) in self.series.iter().enumerate().filter(|(_, s)| !s.hidden) {
            let alpha = match self.selected {
                Some(sel) if sel != si => 0.3,
                _ => 1.0,
            };
            let pts: Vec<(f32, f32)> = (0..n)
                .map(|i| {
                    let f = (s.values.get(i).copied().unwrap_or(0.0) / max).clamp(0.0, 1.0) as f32 * p;
                    let (tx, ty) = geo.axis_tips[i];
                    (
                        geo.center.0 + (tx - geo.center.0) * f,
                        geo.center.1 + (ty - geo.center.1) * f,
                    )
                })
                .collect();
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    if !inside(&pts, px, py) {
                        continue;
                    }
                    let d = ((px - geo.center.0).powi(2) + (py - geo.center.1).powi(2)).sqrt() / geo.radius;
                    if s.fill.covers(x, y, 0.2 + 0.55 * d) {
                        g.set(x, y, s.tone.shade(0.32 + 0.35 * d).fade(alpha));
                    }
                }
            }
            let stroke = s.tone.shade(0.8).fade(alpha);
            for i in 0..n {
                let a = (pts[i].0.round() as i32, pts[i].1.round() as i32);
                let b = (
                    pts[(i + 1) % n].0.round() as i32,
                    pts[(i + 1) % n].1.round() as i32,
                );
                line(&mut g, a, b, stroke, 1);
            }
            for &(x, y) in &pts {
                dot(
                    &mut g,
                    x.round() as i32,
                    y.round() as i32,
                    stroke,
                    s.tone.shade(0.97).fade(alpha),
                );
            }
        }
        g.bloom(self.bloom);
        g
    }
}

/// Even-odd point-in-polygon test.
fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            c = !c;
        }
        j = i;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scale() {
        let s = Scale::nice(87.0, 4);
        assert_eq!(s.max, 100.0);
        assert_eq!(s.ticks, vec![0.0, 25.0, 50.0, 75.0, 100.0]);
        assert_eq!(Scale::nice(0.0, 4).max, 1.0);
    }

    #[test]
    fn format_values() {
        assert_eq!(format_value(950.0), "950");
        assert_eq!(format_value(1200.0), "1.2k");
        assert_eq!(format_value(3_000_000.0), "3M");
    }

    #[test]
    fn interpolation_hits_samples_without_overshoot() {
        let v = [0.0, 10.0, 0.0, 5.0];
        for (i, x) in v.iter().enumerate() {
            assert_eq!(interpolate(&v, i as f32), *x);
        }
        for i in 0..30 {
            let y = interpolate(&v, i as f32 / 10.0);
            assert!((0.0..=10.0).contains(&y));
        }
    }

    #[test]
    fn charts_render_pixels() {
        let s = [
            Series::new("a", Tone::Blue, vec![3.0, 8.0, 5.0, 9.0]),
            Series::new("b", Tone::Pink, vec![2.0, 4.0, 6.0, 3.0]).fill(Fill::Hatched),
        ];
        for kind in [CartesianKind::Area, CartesianKind::Line, CartesianKind::Bar] {
            let mut c = Cartesian::new(kind, &s);
            c.stacked = true;
            c.hover = Some(1);
            assert!(c.render(80, 30).lit_count() > 50, "{kind:?}");
        }
        let slices = [
            Slice::new("x", 1.0, Tone::Green),
            Slice::new("y", 2.0, Tone::Orange),
        ];
        let mut pie = Pie::new(&slices);
        pie.inner_radius = 0.5;
        assert!(pie.render(40, 40).lit_count() > 100);
        let geo = pie.geometry(40, 40);
        assert_eq!(geo.index_at(20.0, 4.0), Some(0));
        let radar = [Series::new("r", Tone::Purple, vec![5.0, 3.0, 4.0, 2.0, 5.0])];
        assert!(Radar::new(&radar).render(40, 40).lit_count() > 100);
    }

    #[test]
    fn hover_index_round_trips() {
        let s = [Series::new("a", Tone::Blue, vec![1.0; 7])];
        for kind in [CartesianKind::Area, CartesianKind::Bar] {
            let c = Cartesian::new(kind, &s);
            for i in 0..7 {
                assert_eq!(c.index_at(c.column_x(i, 120), 120), Some(i));
            }
        }
    }
}
