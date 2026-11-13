//! dither-kit gallery for GPUI Kit.
//!
//! Run with: `cargo run -p dither-gpui --example gallery`

use dither_gpui::prelude::Fill;
use dither_gpui::prelude::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Sizable, Theme, ThemeMode, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

struct Gallery {
    kind: CartesianKind,
    stacked: bool,
    bloom: Bloom,
    fill: Fill,
    replay: u64,
    field: Field,
    deploys: u32,
    upload: f32,
}

impl Gallery {
    fn new() -> Self {
        Self {
            kind: CartesianKind::Area,
            stacked: false,
            bloom: Bloom::Low,
            fill: Fill::Gradient,
            replay: 0,
            field: Field::Plasma,
            deploys: 0,
            upload: 0.35,
        }
    }

    fn traffic(&self) -> Vec<Series> {
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
        .map(|s| s.fill(self.fill))
        .collect()
    }
}

fn card(title: &'static str, subtitle: &'static str, body: impl IntoElement, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .flex_1()
        .min_w(px(300.0))
        .gap_4()
        .p_5()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            v_flex()
                .gap_0p5()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(div().text_xs().text_color(theme.muted_foreground).child(subtitle)),
        )
        .child(body)
}

fn section(title: &'static str, cx: &App) -> Div {
    div()
        .pt_4()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(title.to_uppercase())
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let hero = div()
            .relative()
            .h(px(200.0))
            .rounded(theme.radius_lg)
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(rgb(0x0b0b0f))
            .child(
                DitherBackground::new("hero-bg", self.field, Tone::Purple)
                    .accent(Tone::Pink)
                    .density(0.7),
            )
            .child(
                v_flex()
                    .absolute()
                    .left_6()
                    .bottom_6()
                    .gap_2()
                    .p_4()
                    .rounded(theme.radius)
                    .bg(theme.popover.opacity(0.92))
                    .border_1()
                    .border_color(theme.border)
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("dither-kit"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Ordered-dither charts, buttons and surfaces for GPUI Kit."),
                    )
                    .child(h_flex().gap_1().children(Field::ALL.into_iter().map(|f| {
                        Button::new(SharedString::from(format!("field-{}", f.name())))
                            .xsmall()
                            .label(f.name())
                            .when(f == self.field, |b| b.primary())
                            .when(f != self.field, |b| b.ghost())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.field = f;
                                cx.notify();
                            }))
                    }))),
            );

        let kinds = [
            (CartesianKind::Area, "Area"),
            (CartesianKind::Line, "Line"),
            (CartesianKind::Bar, "Bar"),
        ];
        let toolbar = h_flex()
            .gap_2()
            .flex_wrap()
            .items_center()
            .children(kinds.into_iter().map(|(k, label)| {
                Button::new(label)
                    .small()
                    .label(label)
                    .when(k == self.kind, |b| b.primary())
                    .when(k != self.kind, |b| b.outline())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = k;
                        this.replay += 1;
                        cx.notify();
                    }))
            }))
            .child(
                Button::new("fill")
                    .small()
                    .outline()
                    .label(format!("Fill: {}", self.fill.name()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let i = Fill::ALL.iter().position(|f| *f == this.fill).unwrap_or(0);
                        this.fill = Fill::ALL[(i + 1) % Fill::ALL.len()];
                        cx.notify();
                    })),
            )
            .child(
                Button::new("bloom")
                    .small()
                    .outline()
                    .label(format!("Bloom: {}", self.bloom.name()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.bloom = this.bloom.next();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("replay")
                    .small()
                    .ghost()
                    .label("Replay")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.replay += 1;
                        cx.notify();
                    })),
            )
            .child(
                Switch::new("stacked")
                    .label("Stacked")
                    .checked(self.stacked)
                    .on_change(cx.listener(|this, v: &bool, _, cx| {
                        this.stacked = *v;
                        this.replay += 1;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new("theme")
                    .small()
                    .ghost()
                    .label(if theme.is_dark() {
                        "Light mode"
                    } else {
                        "Dark mode"
                    })
                    .on_click(|_, window, cx| {
                        let mode = if cx.theme().is_dark() {
                            ThemeMode::Light
                        } else {
                            ThemeMode::Dark
                        };
                        Theme::change(mode, Some(window), cx);
                    }),
            );

        let main_chart = card(
            "Visitors",
            "Last 12 months · hover for details, click the legend to focus a series",
            DitherChart::new("traffic", self.kind, self.traffic())
                .labels(MONTHS)
                .stacked(self.stacked)
                .bloom(self.bloom)
                .replay_token(self.replay)
                .height(px(260.0)),
            cx,
        )
        .min_w(px(520.0));

        let pie = card(
            "Browsers",
            "Share of sessions",
            DitherPie::new(
                "browsers",
                vec![
                    Slice::new("Chrome", 275., Tone::Blue),
                    Slice::new("Safari", 200., Tone::Pink),
                    Slice::new("Firefox", 187., Tone::Orange),
                    Slice::new("Edge", 173., Tone::Green),
                    Slice::new("Other", 90., Tone::Grey),
                ],
            )
            .inner_radius(0.58)
            .size(px(210.0))
            .bloom(self.bloom)
            .replay_token(self.replay),
            cx,
        );

        let radar = card(
            "Class stats",
            "Two builds compared on six attributes",
            DitherRadar::new(
                "classes",
                vec![
                    Series::new("Rook", Tone::Purple, vec![80., 95., 60., 70., 85., 65.]),
                    Series::new("Knight", Tone::Green, vec![65., 60., 90., 85., 55., 80.]),
                ],
                ["Speed", "Power", "Range", "Focus", "Grip", "Stamina"],
            )
            .size(px(200.0))
            .bloom(self.bloom)
            .replay_token(self.replay),
            cx,
        );

        let stats = [
            ("Revenue", "$48.2k", "+12.4%", Tone::Green),
            ("Latency", "182ms", "−8.1%", Tone::Purple),
            ("Errors", "0.42%", "+0.1%", Tone::Red),
        ];
        let sparklines = h_flex()
            .gap_4()
            .flex_wrap()
            .children(
                stats
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, value, delta, tone))| {
                        let values: Vec<f64> = (0..48)
                            .map(|x| {
                                50.0 + 30.0
                                    * ((x as f64 * 0.35 + i as f64).sin() + 0.4 * (x as f64 * 0.9).cos())
                                    + x as f64
                            })
                            .collect();
                        v_flex()
                            .flex_1()
                            .min_w(px(220.0))
                            .gap_2()
                            .p_4()
                            .rounded(theme.radius_lg)
                            .border_1()
                            .border_color(theme.border)
                            .child(div().text_xs().text_color(theme.muted_foreground).child(label))
                            .child(
                                h_flex()
                                    .items_baseline()
                                    .gap_2()
                                    .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(value))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(dither_gpui::tone_color(tone))
                                            .child(delta),
                                    ),
                            )
                            .child(
                                DitherSparkline::new(SharedString::from(format!("spark-{i}")), values, tone)
                                    .height(px(44.0))
                                    .replay_token(self.replay),
                            )
                    }),
            );

        let buttons =
            card(
                "Buttons",
                "Kit buttons with a texture that thickens on hover and press",
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_3()
                            .flex_wrap()
                            .child(DitherButton::new("deploy", "Deploy").tone(Tone::Green).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.deploys += 1;
                                    this.upload = (this.upload + 0.15).min(1.0);
                                    cx.notify();
                                }),
                            ))
                            .child(DitherButton::new("preview", "Preview").tone(Tone::Blue).outline())
                            .child(DitherButton::new("dismiss", "Dismiss").tone(Tone::Pink).ghost())
                            .child(
                                DitherButton::new("delete", "Delete deploys")
                                    .tone(Tone::Red)
                                    .disabled(self.deploys == 0)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.deploys = 0;
                                        this.upload = 0.0;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(match self.deploys {
                                0 => "No deploys yet. Delete stays disabled until you deploy.".to_string(),
                                n => format!("{n} deploy(s) queued."),
                            }),
                    ),
                cx,
            );

        let meters = card(
            "Meters",
            "Springs toward new values; each fill style shown",
            v_flex()
                .gap_3()
                .children(Fill::ALL.into_iter().enumerate().map(|(i, f)| {
                    let v = (self.upload + i as f32 * 0.17).fract();
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(
                            div()
                                .w(px(64.0))
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(f.name()),
                        )
                        .child(
                            div().flex_1().child(
                                DitherMeter::new(SharedString::from(format!("meter-{i}")), v, Tone::ALL[i])
                                    .fill(f)
                                    .label(f.name()),
                            ),
                        )
                        .child(
                            div()
                                .w(px(36.0))
                                .text_xs()
                                .child(format!("{}%", (v * 100.0) as u32)),
                        )
                })),
            cx,
        );

        let avatars = card(
            "Avatars",
            "Deterministic identicons from names",
            h_flex().gap_4().flex_wrap().children(
                [
                    "Ada Lovelace",
                    "Grace Hopper",
                    "Alan Turing",
                    "Linus",
                    "Margaret H.",
                    "Ken Thompson",
                ]
                .map(|name| {
                    v_flex()
                        .items_center()
                        .gap_1()
                        .child(DitherAvatar::new(name).size(px(48.0)))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(name))
                }),
            ),
            cx,
        );

        let gradients = card(
            "Gradients",
            "Two-tone blends, fades and halftone",
            h_flex().gap_3().children(
                [
                    DitherGradient::new(Tone::Blue).to(Tone::Purple),
                    DitherGradient::new(Tone::Orange)
                        .to(Tone::Pink)
                        .direction(Direction::Right),
                    DitherGradient::new(Tone::Green),
                    DitherGradient::new(Tone::Red)
                        .to(Tone::Orange)
                        .direction(Direction::Radial)
                        .matrix(Matrix::Halftone),
                    DitherGradient::new(Tone::Hue(190.0))
                        .direction(Direction::Down)
                        .matrix(Matrix::Bayer4),
                ]
                .into_iter()
                .map(|g| {
                    div()
                        .relative()
                        .flex_1()
                        .h(px(120.0))
                        .rounded(theme.radius)
                        .overflow_hidden()
                        .child(g)
                }),
            ),
            cx,
        );

        div()
            .id("gallery")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(1180.0))
                    .mx_auto()
                    .p_6()
                    .gap_4()
                    .child(hero)
                    .child(section("Charts", cx))
                    .child(toolbar)
                    .child(
                        h_flex()
                            .gap_4()
                            .flex_wrap()
                            .items_start()
                            .child(main_chart)
                            .child(pie),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .flex_wrap()
                            .items_start()
                            .child(radar)
                            .child(v_flex().flex_1().min_w(px(460.0)).gap_4().child(sparklines)),
                    )
                    .child(section("Controls", cx))
                    .child(
                        h_flex()
                            .gap_4()
                            .flex_wrap()
                            .items_start()
                            .child(buttons)
                            .child(meters),
                    )
                    .child(section("Surfaces", cx))
                    .child(
                        h_flex()
                            .gap_4()
                            .flex_wrap()
                            .items_start()
                            .child(avatars)
                            .child(gradients),
                    ),
            )
    }
}

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1240.0), px(900.0)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("dither-kit gallery".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        open_window(options, cx, |_, cx| cx.new(|_| Gallery::new())).expect("failed to open window");
    });
}
