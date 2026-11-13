---
name: dither-kit-rs
description: Build ordered-dither charts, buttons, gradients, animated backgrounds, avatars and meters in Rust UIs with dither-kit-rs. Use when a Rust project depends on (or should depend on) dither-core, dither-gpui or dither-ratatui, or when the user asks for dithered / pixel / retro-textured charts or components in GPUI Kit desktop apps or Ratatui terminal apps.
---

# dither-kit-rs

One renderer-neutral engine (`dither-core`) rasterizes every component into a `PixelGrid`;
two thin frontends paint it:

| Crate | Use for | Add with |
| --- | --- | --- |
| `dither-gpui` | GPUI Kit desktop apps | `cargo add dither-gpui gpui-kit@=0.7.3` |
| `dither-ratatui` | Ratatui terminal apps | `cargo add dither-ratatui ratatui@0.30` |
| `dither-core` | Any other renderer (game engine, image encoder, canvas) | `cargo add dither-core` |

Rules that matter:

- MSRV is **Rust 1.95**, edition 2024.
- `dither-gpui` requires `gpui-kit` pinned to **exactly `=0.7.3`** (the version `gpui-motion-kit 0.5` is built against). Do not bump it.
- `dither-ratatui` needs a **truecolor terminal**. Each cell holds two square pixels (`▀`/`▄` half blocks).
- Both frontends re-export the core as `dither_core`; do not add `dither-core` separately unless you use it directly.
- On Linux, GPUI needs `libfontconfig-dev libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libvulkan1` and a Vulkan driver.

## Shared vocabulary (`dither_core`, in both preludes)

| Type | Values |
| --- | --- |
| `Tone` | `Green` `Blue` `Purple` `Pink` `Orange` `Red` `Grey`, or `Tone::Hue(0.0..360.0)`. `tone.shade(0.0..=1.0)` gives an OKLCH ramp. `Tone::parse("blue")`. |
| `Fill` | `Gradient` (Bayer 8×8, default), `Dotted`, `Hatched`, `Solid` |
| `Bloom` | `Off` `Low` `High` `Aura` — dithered glow around lit pixels |
| `Matrix` | `Bayer2` `Bayer4` `Bayer8` `Halftone` `Noise` |
| `Field` | `Plasma` `Waves` `Ripple` `Noise` `Aurora` (animated backgrounds) |
| `Direction` | `Up` `Down` `Left` `Right` `Radial` |
| `ButtonStyle` | `Solid` `Outline` `Ghost` |
| `CartesianKind` | `Area` `Line` `Bar` |

Data types:

```rust
Series::new("Desktop", Tone::Blue, vec![186., 305., 237.]).fill(Fill::Hatched).hidden(false)
Slice::new("Chrome", 275.0, Tone::Blue).fill(Fill::Dotted)
```

## GPUI Kit (`dither-gpui`)

```rust
use dither_gpui::prelude::*;   // components + core vocabulary + Disableable
use gpui_kit::*;
```

Components are `RenderOnce` values. Interactive ones take an `id` (`impl Into<ElementId>`);
hover, selection, entrance progress and tooltip position live in keyed element state, so
you only pass data. Give each instance a **unique, stable id**.

| Component | Constructor | Notable builders |
| --- | --- | --- |
| Cartesian chart | `DitherChart::area/line/bar(id, Vec<Series>)` or `::new(id, kind, series)` | `.labels([..])` `.stacked(bool)` `.height(px)` `.legend(bool)` `.clickable_legend(bool)` `.tooltip(bool)` `.y_axis(bool)` `.interactive(bool)` `.marker_index(Option<usize>)` `.description(str)` `.on_hover_change(..)` `.on_selection_change(..)` |
| Pie / donut | `DitherPie::new(id, Vec<Slice>)` | `.inner_radius(0.0..1.0)` `.size(px)` `.legend(bool)` `.on_hover_change(..)` |
| Radar | `DitherRadar::new(id, Vec<Series>, axes)` | `.size(px)` `.legend(bool)` |
| Sparkline | `DitherSparkline::new(id, Vec<f64>, Tone)` | `.height(px)` `.hovered(bool)` |
| Button | `DitherButton::new(id, label)` | `.tone(Tone)` `.variant(ButtonStyle)` `.outline()` `.ghost()` `.pixel_size(px)` `.on_click(..)` `.disabled(bool)` |
| Gradient | `DitherGradient::new(Tone)` | `.to(Tone)` `.direction(Direction)` `.matrix(Matrix)` `.density(f32)` `.pixel_size(px)` |
| Background | `DitherBackground::new(id, Field, Tone)` | `.accent(Tone)` `.density(f32)` `.matrix(Matrix)` `.pixel_size(px)` `.speed(f32)` |
| Avatar | `DitherAvatar::new(name)` | `.size(px)` `.hue(f32)` |
| Meter | `DitherMeter::new(id, 0.0..=1.0, Tone)` | `.fill(Fill)` `.height(px)` `.pixel_size(px)` `.label(str)` |

All charts (`DitherChart`, `DitherPie`, `DitherRadar`, `DitherSparkline`) also have
`.animate(bool)`, `.animation_duration(Duration)`, `.replay_token(u64)` (change the value to
replay the entrance), `.bloom(Bloom)` and `.pixel_size(px)` (default 3px).

Callbacks take `&Option<usize>`, so they work with `cx.listener`:

```rust
DitherChart::area("visitors", vec![
    Series::new("Desktop", Tone::Blue, vec![186., 305., 237., 73., 209., 214.]),
    Series::new("Mobile",  Tone::Pink, vec![80., 200., 120., 190., 130., 140.]).fill(Fill::Hatched),
])
.labels(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
.stacked(true)
.bloom(Bloom::Low)
.replay_token(self.replay)
.on_hover_change(cx.listener(|this, index: &Option<usize>, _, cx| {
    this.focus = *index;
    cx.notify();
}))
```

Minimal app:

```rust
fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
        open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Dashboard { replay: 0 }))
            .expect("open window");
    });
}
```

Built in, do not reimplement: theme tokens (axes/tooltips/legends follow the active Kit
theme, light and dark), reduced-motion handling, focus/Tab/Enter/Space on buttons (wraps
Kit's `Button`), accessibility roles (charts are labelled `Figure`s, meters are
`ProgressIndicator`s). Lower level: `pixel_canvas`, `paint_grid`, `hsla`, `tone_color`,
`legend(entries, on_click, cx)`, `swatch(tone)`, `Clock`.

## Ratatui (`dither-ratatui`)

```rust
use dither_ratatui::{Avatar, Background, Button, ButtonState, Chart, Gradient, Legend, LegendItem,
    Meter, PieChart, RadarChart, Sparkline, Theme, prelude::*};
use dither_ratatui::dither_core::ease;
```

The prelude renames core types that clash with widgets: `GradientSpec` (core `Gradient`) and
`BackgroundSpec` (core `Background`).

Widgets are plain values borrowing your data; **all state lives in your app**
(hover index, selection, animation progress, time). Render with `frame.render_widget`.

| Widget | Constructor | Builders |
| --- | --- | --- |
| `Chart` | `Chart::area/line/bar(&series)` | `.labels(&labels)` `.stacked(bool)` `.progress(0..1)` `.hover(Option<usize>)` `.selected(Option<usize>)` `.bloom(Bloom)` `.legend(bool)` `.theme(Theme)` |
| `PieChart` | `PieChart::new(&slices)` | `.donut(inner_radius)` `.progress(..)` `.hover(..)` `.bloom(..)` |
| `RadarChart` | `RadarChart::new(&series, &axes)` | `.progress(..)` `.selected(..)` `.bloom(..)` |
| `Sparkline` | `Sparkline::new(&values, Tone)` | `.hovered(bool)` `.progress(..)` |
| `Button` | `Button::new("Deploy", Tone::Green)` | `.style(ButtonStyle)` `.state(ButtonState)` `.time(secs)` `.disabled(bool)` |
| `Gradient` | `Gradient::new(GradientSpec::new(Tone::Blue).to(Tone::Purple))` | `theme` field |
| `Background` | `Background::new(BackgroundSpec::new(Field::Aurora, Tone::Purple), time_secs)` | `theme` field |
| `Avatar` | `Avatar::new("Ada Lovelace")` | `.hue(f32)` `.scale(usize)`; `.size()` gives cells |
| `Meter` | `Meter::new(0.0..=1.0, Tone)` | `.fill(Fill)` |
| `Legend` | `Legend::new(vec![LegendItem { label, tone, hidden, selected }])` | `.centered()`; `.layout(area)`, `.hit(area, col, row)` |

`Theme::dark()` (default) and `Theme::light()`.

Mouse hit-testing (compute against the same `area` you render into):

```rust
let chart = Chart::area(&series).labels(&labels);
app.hover = chart.index_at(area, mouse.column, mouse.row);     // data index
if let Some(i) = chart.legend_hit(area, mouse.column, mouse.row) { /* toggle series i */ }
let slice = PieChart::new(&slices).index_at(area, col, row);
if ButtonState::hit(button_area, col, row) { app.button.hovered = true; }
app.button.tick(ButtonStyle::Solid, dt);                       // advance hover/press density
```

Entrance animation: keep a `t` that advances each frame and pass
`.progress(ease::out_cubic(t.min(1.0)))`. Animated `Background` and button sparkles take a
running time in seconds. Redraw on a timer (~30–60 fps) while anything animates.

## Core only (`dither-core`)

```rust
use dither_core::*;

let mut grid: PixelGrid = Gradient::new(Tone::Blue).to(Tone::Purple).direction(Direction::Radial).render(64, 32);
let grid = Background::new(Field::Plasma, Tone::Green).accent(Tone::Blue).render(w, h, time);
let grid = Cartesian::new(CartesianKind::Area, &series).render(w, h);
let grid = Pie::new(&slices).render(w, h);
let grid = avatar("Ada Lovelace", None);          // 8×8 mirrored identicon
let grid = meter(w, h, 0.62, Tone::Orange, Fill::Dotted);
let grid = sparkline(&values, Tone::Blue, w, h, false, 1.0);
grid.bloom(Bloom::Low);                            // in place, needs `mut`

for run in grid.runs() {
    // run.x, run.y, run.len, run.color: Rgba { r, g, b, a } — one horizontal run of equal pixels
}
```

Output is deterministic (ordered Bayer thresholds are stateless per pixel), so it is safe for
snapshot tests and does not shimmer while animating.

## Reference

- Docs and live demos: https://deepanshumishraa.github.io/dither-kit-rs/
- Galleries: `cargo run -p dither-gpui --example gallery`, `cargo run -p dither-ratatui --example gallery`
- Source: https://github.com/DeepanshuMishraa/dither-kit-rs
