//! Every dither-rune element in one window, with the same data as the GPUI
//! and terminal galleries.
//!
//! `cargo run -p dither-rune --example gallery` opens the window.
//! `cargo run -p dither-rune --example gallery -- --snap gallery.png` renders
//! it headless; add `--scene bars` or `--scene still` to pick a state.

use std::time::Duration;

use dither_rune::{
    Background, Cartesian, CartesianKind, Direction, Field, Fill, Gradient, Matrix, Pie, Radar, Series,
    Slice, Tone,
};
use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, ScrollSpec, Tx, col, el, row, text};

/// Frames a second for the animated background. A dither field reads as
/// motion at a few frames a second, and each redraw stays cheap.
const FPS: f32 = 8.0;

/// Size of one dither dot, in points.
const DOT: f32 = 3.0;

struct State {
    kind: CartesianKind,
    stacked: bool,
    fill: Fill,
    field: Field,
    paused: bool,
    frame: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            kind: CartesianKind::Area,
            stacked: false,
            fill: Fill::Gradient,
            field: Field::Plasma,
            paused: false,
            frame: 0,
        }
    }
}

fn traffic(fill: Fill) -> Vec<Series> {
    vec![
        Series::new(
            "Desktop",
            Tone::Blue,
            vec![
                186., 305., 237., 173., 209., 214., 280., 320., 260., 290., 340., 310.,
            ],
        ),
        Series::new(
            "Mobile",
            Tone::Pink,
            vec![
                80., 200., 120., 190., 130., 140., 160., 150., 210., 180., 220., 260.,
            ],
        ),
        Series::new(
            "Tablet",
            Tone::Orange,
            vec![40., 60., 55., 70., 65., 90., 85., 70., 95., 100., 90., 120.],
        ),
    ]
    .into_iter()
    .map(|s| s.fill(fill))
    .collect()
}

fn browsers() -> Vec<Slice> {
    vec![
        Slice::new("Chrome", 275., Tone::Blue),
        Slice::new("Safari", 200., Tone::Pink),
        Slice::new("Firefox", 187., Tone::Orange),
        Slice::new("Edge", 173., Tone::Green),
        Slice::new("Other", 90., Tone::Grey),
    ]
}

fn classes() -> Vec<Series> {
    vec![
        Series::new("Rook", Tone::Purple, vec![80., 95., 60., 70., 85., 65.]),
        Series::new("Knight", Tone::Green, vec![65., 60., 90., 85., 55., 80.]),
    ]
}

/// A toggle-style button: filled with the accent while `on`.
fn chip(key: String, label: &str, on: bool, press: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(true);
    el().key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .px(10.0)
        .py(4.0)
        .radius(6.0)
        .fill(if on { tok::accent(0, true) } else { t.control_off })
        .on_press(press)
        .child(text(label).font(type_::LABEL).color(t.text))
}

fn card(title: &str, subtitle: &str, body: El) -> El {
    let t = tok::theme(true);
    col()
        .gap(10.0)
        .padding(16.0)
        .radius(10.0)
        .fill(t.card)
        .border(1.0, t.card_border)
        .child(text(title).font(type_::TITLE).color(t.text))
        .child(text(subtitle).font(type_::BODY).color(t.text_secondary))
        .child(body)
}

/// A row of colour swatches naming each series.
fn legend(entries: &[(&str, Tone)]) -> El {
    let t = tok::theme(true);
    let mut r = row().gap(12.0).items_center().flex_wrap();
    for (label, tone) in entries {
        r = r.child(
            row()
                .gap(6.0)
                .items_center()
                .child(
                    el().size(8.0, 8.0)
                        .radius(2.0)
                        .fill(dither_rune::color(tone.base())),
                )
                .child(text(*label).font(type_::LABEL).color(t.text_secondary)),
        );
    }
    r
}

fn hero(s: &State) -> El {
    let t = tok::theme(true);
    let spec = Background::new(s.field, Tone::Purple)
        .accent(Tone::Pink)
        .density(0.7);
    let mut fields = row().gap(4.0);
    for f in Field::ALL {
        fields = fields.child(chip(
            format!("field-{}", f.name()),
            f.name(),
            f == s.field,
            move |s| s.field = f,
        ));
    }
    fields = fields.child(chip(
        "pause".into(),
        if s.paused { "Play" } else { "Pause" },
        s.paused,
        |s| s.paused = !s.paused,
    ));
    el().w_full()
        .h(200.0)
        .radius(10.0)
        .border(1.0, t.card_border)
        .child(
            dither_rune::background("hero-bg", DOT, spec, s.frame, FPS)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        )
        .child(
            col()
                .absolute(24.0, 88.0)
                .gap(8.0)
                .padding(14.0)
                .radius(8.0)
                .fill(t.card)
                .border(1.0, t.card_border)
                .child(text("dither-kit").font(type_::TITLE).color(t.text))
                .child(
                    text("Ordered-dither charts, meters and surfaces for Rune.")
                        .font(type_::BODY)
                        .color(t.text_secondary),
                )
                .child(fields),
        )
}

fn traffic_card(s: &State) -> El {
    let series = traffic(s.fill);
    let mut controls = row().gap(4.0).flex_wrap();
    for (kind, name) in [
        (CartesianKind::Area, "Area"),
        (CartesianKind::Line, "Line"),
        (CartesianKind::Bar, "Bar"),
    ] {
        controls = controls.child(chip(format!("kind-{name}"), name, kind == s.kind, move |s| {
            s.kind = kind
        }));
    }
    controls = controls.child(chip("stacked".into(), "Stacked", s.stacked, |s| {
        s.stacked = !s.stacked
    }));
    for f in Fill::ALL {
        controls = controls.child(chip(
            format!("fill-{}", f.name()),
            f.name(),
            f == s.fill,
            move |s| s.fill = f,
        ));
    }
    let (kind, stacked) = (s.kind, s.stacked);
    let chart = dither_rune::chart("traffic", DOT, (kind as u8, stacked, s.fill as u8), |w, h| {
        let mut c = Cartesian::new(kind, &series);
        c.stacked = stacked;
        c.render(w, h)
    })
    .w_full()
    .h(220.0);
    card(
        "Traffic",
        "Sessions per month by device",
        col().gap(10.0).child(controls).child(chart).child(legend(&[
            ("Desktop", Tone::Blue),
            ("Mobile", Tone::Pink),
            ("Tablet", Tone::Orange),
        ])),
    )
}

fn pie_card() -> El {
    let slices = browsers();
    let pie = dither_rune::chart("browsers", DOT, "browsers", |w, h| {
        let mut p = Pie::new(&slices);
        p.inner_radius = 0.58;
        p.render(w, h)
    })
    .size(210.0, 210.0);
    let entries: Vec<(&str, Tone)> = slices.iter().map(|s| (s.label.as_str(), s.tone)).collect();
    card(
        "Browsers",
        "Share of sessions",
        col().gap(10.0).items_center().child(pie).child(legend(&entries)),
    )
}

fn radar_card() -> El {
    let series = classes();
    let radar = dither_rune::chart("classes", DOT, "classes", |w, h| Radar::new(&series).render(w, h))
        .size(210.0, 210.0);
    card(
        "Class stats",
        "Two builds compared on six attributes",
        col()
            .gap(10.0)
            .items_center()
            .child(radar)
            .child(legend(&[("Rook", Tone::Purple), ("Knight", Tone::Green)])),
    )
}

fn sparklines_card() -> El {
    let t = tok::theme(true);
    let stats = [
        ("Revenue", "$48.2k", Tone::Green),
        ("Latency", "182ms", Tone::Purple),
        ("Errors", "0.42%", Tone::Red),
    ];
    let mut r = row().gap(16.0).flex_wrap();
    for (i, (label, value, tone)) in stats.into_iter().enumerate() {
        let values: Vec<f64> = (0..48)
            .map(|x| {
                50.0 + 30.0 * ((x as f64 * 0.35 + i as f64).sin() + 0.4 * (x as f64 * 0.9).cos()) + x as f64
            })
            .collect();
        r = r.child(
            col()
                .gap(4.0)
                .w(200.0)
                .child(text(label).font(type_::LABEL).color(t.text_secondary))
                .child(text(value).font(type_::TITLE).color(t.text))
                .child(
                    dither_rune::sparkline(&format!("spark-{i}"), 2.0, &values, tone, 1.0)
                        .w_full()
                        .h(48.0),
                ),
        );
    }
    card("Sparklines", "Chrome-free trends", r)
}

fn meters_card() -> El {
    let t = tok::theme(true);
    let mut c = col().gap(10.0);
    for (i, f) in Fill::ALL.into_iter().enumerate() {
        let v = [0.82, 0.64, 0.47, 0.29][i];
        c = c.child(
            row()
                .gap(10.0)
                .items_center()
                .child(text(f.name()).font(type_::LABEL).color(t.text_secondary).w(64.0))
                .child(
                    dither_rune::meter(&format!("meter-{i}"), DOT, v, Tone::ALL[i], f)
                        .grow(1.0)
                        .h(14.0),
                )
                .child(
                    text(format!("{}%", (v * 100.0) as u32))
                        .font(type_::LABEL)
                        .color(t.text)
                        .w(36.0),
                ),
        );
    }
    card("Meters", "One per fill pattern", c)
}

fn avatars_card() -> El {
    let t = tok::theme(true);
    let mut r = row().gap(16.0).flex_wrap();
    for name in [
        "Ada Lovelace",
        "Grace Hopper",
        "Alan Turing",
        "Linus",
        "Margaret H.",
        "Ken Thompson",
    ] {
        r = r.child(
            col()
                .gap(4.0)
                .items_center()
                .child(dither_rune::avatar(name, 48.0, None))
                .child(text(name).font(type_::LABEL).color(t.text_secondary)),
        );
    }
    card("Avatars", "Deterministic identicons from names", r)
}

fn gradients_card() -> El {
    let specs = [
        Gradient::new(Tone::Blue).to(Tone::Purple),
        Gradient::new(Tone::Orange)
            .to(Tone::Pink)
            .direction(Direction::Right),
        Gradient::new(Tone::Green),
        Gradient::new(Tone::Red)
            .to(Tone::Orange)
            .direction(Direction::Radial)
            .matrix(Matrix::Halftone),
        Gradient::new(Tone::Hue(190.0))
            .direction(Direction::Down)
            .matrix(Matrix::Bayer4),
    ];
    let mut r = row().gap(12.0);
    for (i, spec) in specs.into_iter().enumerate() {
        r = r.child(
            dither_rune::gradient(&format!("gradient-{i}"), DOT, spec)
                .grow(1.0)
                .h(120.0)
                .radius(8.0),
        );
    }
    card("Gradients", "Two-tone blends, fades and halftone", r)
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let pair = row().gap(16.0).flex_wrap().child(pie_card()).child(radar_card());
    col()
        .w_full()
        .h_full()
        .fill(t.bg)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(
            col()
                .padding(24.0)
                .gap(16.0)
                .child(hero(s))
                .child(traffic_card(s))
                .child(pair)
                .child(sparklines_card())
                .child(meters_card())
                .child(avatars_card())
                .child(gradients_card()),
        )
}

/// Steps the background a frame at a time, and sleeps while paused.
fn poll(s: &mut State, time: f64, tx: &mut Tx) -> bool {
    if s.paused {
        return false;
    }
    let frame = dither_rune::frame(time, FPS);
    let moved = frame != s.frame;
    s.frame = frame;
    tx.wake_after(Duration::from_secs_f32(1.0 / FPS));
    moved
}

fn main() {
    rune::run(AppSpec::<State> {
        poll: Some(poll),
        scenes: &[
            ("bars", |s| {
                s.kind = CartesianKind::Bar;
                s.stacked = true;
            }),
            ("still", |s| s.paused = true),
        ],
        ..AppSpec::new("dither-kit gallery", (900.0, 760.0), State::default, view)
    });
}
