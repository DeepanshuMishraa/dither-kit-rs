//! Interactive dither-kit gallery for the terminal.
//!
//! Run with: `cargo run -p dither-ratatui --example gallery`
//! Needs a truecolor terminal. Mouse hover/click is supported.
//!
//! Keys: 1/2/3 or Tab switch pages · ←/→ move crosshair · k chart kind ·
//! s stack · f fill · b bloom · r replay · Space/Enter press focused button ·
//! ↑/↓ focus buttons · q quit

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::execute;
use dither_ratatui::dither_core::ease;
use dither_ratatui::prelude::*;
use dither_ratatui::{
    Avatar, Background, Button, ButtonState, Chart, Gradient, Meter, PieChart, RadarChart, Sparkline, Theme,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::{DefaultTerminal, Frame};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const SKILLS: [&str; 6] = ["Speed", "Power", "Range", "Focus", "Grip", "Stamina"];
const BUTTONS: [(&str, Tone, ButtonStyle); 4] = [
    ("Deploy", Tone::Green, ButtonStyle::Solid),
    ("Preview", Tone::Blue, ButtonStyle::Outline),
    ("Dismiss", Tone::Pink, ButtonStyle::Ghost),
    ("Delete", Tone::Red, ButtonStyle::Solid),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Charts,
    Controls,
    Backgrounds,
}

struct App {
    page: Page,
    start: Instant,
    replay: Instant,
    last_tick: Instant,
    series: Vec<Series>,
    radar: Vec<Series>,
    slices: Vec<Slice>,
    kind: CartesianKind,
    stacked: bool,
    fill: usize,
    bloom: Bloom,
    hover: Option<usize>,
    pie_hover: Option<usize>,
    buttons: [ButtonState; 4],
    pressed_until: [Option<Instant>; 4],
    focus: usize,
    clicks: [u32; 4],
    field: usize,
    tone: usize,
    // Areas from the last draw, for mouse hit testing.
    chart_area: Rect,
    pie_area: Rect,
    button_areas: [Rect; 4],
    quit: bool,
}

impl App {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            page: Page::Charts,
            start: now,
            replay: now,
            last_tick: now,
            series: vec![
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
            ],
            radar: vec![
                Series::new("Rook", Tone::Purple, vec![80., 95., 60., 70., 85., 65.]),
                Series::new("Knight", Tone::Green, vec![65., 60., 90., 85., 55., 80.]),
            ],
            slices: vec![
                Slice::new("Chrome", 275., Tone::Blue),
                Slice::new("Safari", 200., Tone::Pink),
                Slice::new("Firefox", 187., Tone::Orange),
                Slice::new("Edge", 173., Tone::Green),
                Slice::new("Other", 90., Tone::Grey),
            ],
            kind: CartesianKind::Area,
            stacked: false,
            fill: 0,
            bloom: Bloom::Low,
            hover: None,
            pie_hover: None,
            buttons: [ButtonState::default(); 4],
            pressed_until: [None; 4],
            focus: 0,
            clicks: [0; 4],
            field: 0,
            tone: 2,
            chart_area: Rect::default(),
            pie_area: Rect::default(),
            button_areas: [Rect::default(); 4],
            quit: false,
        }
    }

    fn time(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    fn progress(&self) -> f32 {
        ease::out_cubic(self.replay.elapsed().as_secs_f32() / 0.9)
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32();
        self.last_tick = now;
        for (i, b) in self.buttons.iter_mut().enumerate() {
            b.focused = self.page == Page::Controls && self.focus == i;
            if self.pressed_until[i].is_some_and(|t| now >= t) {
                self.pressed_until[i] = None;
                b.pressed = false;
            }
            b.tick(BUTTONS[i].2, dt);
        }
    }

    fn press(&mut self, i: usize) {
        if i == 3 && self.clicks[0] == 0 {
            return; // "Delete" stays disabled until something is deployed.
        }
        self.buttons[i].pressed = true;
        self.pressed_until[i] = Some(Instant::now() + Duration::from_millis(160));
        self.clicks[i] += 1;
    }

    fn chart(&self) -> Chart<'_> {
        Chart::new(self.kind, &self.series)
            .labels(&MONTHS)
            .stacked(self.stacked)
            .progress(self.progress())
            .hover(self.hover)
            .bloom(self.bloom)
    }

    fn set_fill(&mut self) {
        let f = Fill::ALL[self.fill % 4];
        for s in &mut self.series {
            s.fill = f;
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('1') => self.page = Page::Charts,
            KeyCode::Char('2') => self.page = Page::Controls,
            KeyCode::Char('3') => self.page = Page::Backgrounds,
            KeyCode::Tab => {
                self.page = match self.page {
                    Page::Charts => Page::Controls,
                    Page::Controls => Page::Backgrounds,
                    Page::Backgrounds => Page::Charts,
                }
            }
            KeyCode::Char('r') => self.replay = Instant::now(),
            KeyCode::Char('b') => self.bloom = self.bloom.next(),
            KeyCode::Char('s') => {
                self.stacked = !self.stacked;
                self.replay = Instant::now();
            }
            KeyCode::Char('k') => {
                self.kind = match self.kind {
                    CartesianKind::Area => CartesianKind::Line,
                    CartesianKind::Line => CartesianKind::Bar,
                    CartesianKind::Bar => CartesianKind::Area,
                };
                self.replay = Instant::now();
            }
            KeyCode::Char('f') => {
                self.fill += 1;
                self.set_fill();
            }
            KeyCode::Left | KeyCode::Right if self.page == Page::Charts => {
                let n = MONTHS.len();
                self.hover = Some(match (self.hover, code) {
                    (None, _) => 0,
                    (Some(i), KeyCode::Left) => (i + n - 1) % n,
                    (Some(i), _) => (i + 1) % n,
                });
            }
            KeyCode::Left | KeyCode::Right if self.page == Page::Backgrounds => {
                let d = if code == KeyCode::Left {
                    Field::ALL.len() - 1
                } else {
                    1
                };
                self.field = (self.field + d) % Field::ALL.len();
            }
            KeyCode::Char('t') => self.tone = (self.tone + 1) % Tone::ALL.len(),
            KeyCode::Up if self.page == Page::Controls => self.focus = (self.focus + 3) % 4,
            KeyCode::Down if self.page == Page::Controls => self.focus = (self.focus + 1) % 4,
            KeyCode::Enter | KeyCode::Char(' ') if self.page == Page::Controls => self.press(self.focus),
            _ => {}
        }
    }

    fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        match self.page {
            Page::Charts => {
                let chart = self.chart();
                let idx = chart.index_at(self.chart_area, col, row);
                let legend = chart.legend_hit(self.chart_area, col, row);
                let pie = PieChart::new(&self.slices)
                    .donut(0.55)
                    .index_at(self.pie_area, col, row);
                if matches!(kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) {
                    self.hover = idx;
                    self.pie_hover = pie;
                }
                if let (MouseEventKind::Down(MouseButton::Left), Some(i)) = (kind, legend) {
                    self.series[i].hidden = !self.series[i].hidden;
                }
            }
            Page::Controls => {
                for i in 0..4 {
                    let inside = self.button_areas[i].contains((col, row).into());
                    self.buttons[i].hovered = inside;
                    if inside && kind == MouseEventKind::Down(MouseButton::Left) {
                        self.focus = i;
                        self.press(i);
                    }
                }
            }
            Page::Backgrounds => {
                if kind == MouseEventKind::Down(MouseButton::Left) {
                    self.field = (self.field + 1) % Field::ALL.len();
                }
            }
        }
    }
}

fn card(title: &str) -> Block<'_> {
    let t = Theme::default();
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(dither_ratatui::raster::color(t.border)))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(dither_ratatui::raster::color(t.muted)),
        ))
}

fn draw(f: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    let bg = dither_ratatui::raster::color(theme.background);
    let fg = dither_ratatui::raster::color(theme.foreground);
    f.render_widget(Block::new().style(Style::default().bg(bg).fg(fg)), f.area());
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());

    let tab = |label: &'static str, page: Page| {
        if app.page == page {
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(fg)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            )
        } else {
            Span::styled(
                format!(" {label} "),
                Style::default().fg(dither_ratatui::raster::color(theme.muted)),
            )
        }
    };
    f.render_widget(
        Line::from(vec![
            Span::styled(
                " ▚ dither-kit ",
                Style::default()
                    .fg(dither_ratatui::raster::color(Tone::Green.base()))
                    .bold(),
            ),
            tab("1 Charts", Page::Charts),
            tab("2 Controls", Page::Controls),
            tab("3 Backgrounds", Page::Backgrounds),
        ]),
        header,
    );
    let help = match app.page {
        Page::Charts => format!(
            " ←/→ crosshair · k kind ({:?}) · s stack ({}) · f fill ({}) · b bloom ({}) · r replay · click legend to toggle · q quit",
            app.kind,
            if app.stacked { "on" } else { "off" },
            Fill::ALL[app.fill % 4].name(),
            app.bloom.name()
        ),
        Page::Controls => " ↑/↓ focus · Enter/Space press · mouse hover & click · t tone · q quit".into(),
        Page::Backgrounds => format!(
            " ←/→ or click field ({}) · t tone ({}) · q quit",
            Field::ALL[app.field].name(),
            Tone::ALL[app.tone].name()
        ),
    };
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(dither_ratatui::raster::color(theme.muted))),
        footer,
    );

    match app.page {
        Page::Charts => draw_charts(f, app, body),
        Page::Controls => draw_controls(f, app, body),
        Page::Backgrounds => draw_backgrounds(f, app, body),
    }
}

fn draw_charts(f: &mut Frame, app: &mut App, area: Rect) {
    let [top, bottom] = Layout::vertical([Constraint::Percentage(62), Constraint::Fill(1)]).areas(area);
    let [main, side] = Layout::horizontal([Constraint::Percentage(66), Constraint::Fill(1)]).areas(top);

    let block = card("Visitors · last 12 months");
    let inner = block.inner(main);
    f.render_widget(block, main);
    app.chart_area = inner;
    f.render_widget(app.chart(), inner);

    let block = card("Browsers");
    let inner = block.inner(side);
    f.render_widget(block, side);
    app.pie_area = inner;
    let pie = PieChart::new(&app.slices)
        .donut(0.55)
        .progress(app.progress())
        .hover(app.pie_hover)
        .bloom(app.bloom);
    f.render_widget(pie, inner);

    let [radar_area, sparks] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).areas(bottom);
    let block = card("Class stats");
    let inner = block.inner(radar_area);
    f.render_widget(block, radar_area);
    f.render_widget(
        RadarChart::new(&app.radar, &SKILLS)
            .progress(app.progress())
            .bloom(app.bloom),
        inner,
    );

    let rows = Layout::vertical([Constraint::Fill(1); 3]).split(sparks);
    let stats = [
        ("Revenue", "$48.2k", "+12.4%", Tone::Green),
        ("Latency", "182ms", "-8.1%", Tone::Purple),
        ("Errors", "0.42%", "+0.1%", Tone::Red),
    ];
    for (i, (label, value, delta, tone)) in stats.into_iter().enumerate() {
        let block = card(label);
        let inner = block.inner(rows[i]);
        f.render_widget(block, rows[i]);
        let [text, line] = Layout::horizontal([Constraint::Length(18), Constraint::Fill(1)]).areas(inner);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(value, Style::default().bold())),
                Line::from(Span::styled(
                    delta,
                    Style::default().fg(dither_ratatui::raster::color(tone.shade(0.75))),
                )),
            ]),
            text,
        );
        let values: Vec<f64> = (0..40)
            .map(|x| {
                50.0 + 30.0 * ((x as f64 * 0.35 + i as f64).sin() + 0.4 * (x as f64 * 0.9).cos()) + x as f64
            })
            .collect();
        f.render_widget(
            Sparkline::new(&values, tone)
                .progress(app.progress())
                .hovered(app.hover.is_some()),
            line,
        );
    }
}

fn draw_controls(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.time();
    let [buttons, avatars, meters, gradients] = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Fill(1),
    ])
    .areas(area);

    let block = card("Buttons");
    let inner = block.inner(buttons);
    f.render_widget(block, buttons);
    let cols = Layout::horizontal([Constraint::Length(20); 4])
        .spacing(2)
        .split(inner);
    for i in 0..4 {
        let (label, tone, style) = BUTTONS[i];
        let [b, caption] = Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(cols[i]);
        app.button_areas[i] = b;
        let disabled = i == 3 && app.clicks[0] == 0;
        f.render_widget(
            Button::new(label, tone)
                .style(style)
                .state(app.buttons[i])
                .time(t)
                .disabled(disabled),
            b,
        );
        let note = if disabled {
            "disabled until deploy".to_string()
        } else {
            format!("{:?} · {} clicks", style, app.clicks[i])
        };
        f.render_widget(
            Paragraph::new(note)
                .style(Style::default().fg(dither_ratatui::raster::color(Theme::default().muted))),
            caption,
        );
    }

    let block = card("Avatars · generated from names");
    let inner = block.inner(avatars);
    f.render_widget(block, avatars);
    let names = [
        "Ada Lovelace",
        "Grace Hopper",
        "Alan Turing",
        "Linus",
        "Margaret H.",
        "Ken Thompson",
    ];
    let cols = Layout::horizontal([Constraint::Length(14); 6])
        .spacing(1)
        .split(inner);
    for (name, col) in names.iter().zip(cols.iter()) {
        let av = Avatar::new(name);
        let (w, h) = av.size();
        f.render_widget(av, Rect::new(col.x + (col.width - w) / 2, col.y, w, h));
        f.render_widget(
            Paragraph::new(*name).centered(),
            Rect::new(col.x, col.y + h + 1, col.width, 1),
        );
    }

    let block = card("Meters");
    let inner = block.inner(meters);
    f.render_widget(block, meters);
    let rows = Layout::vertical([Constraint::Length(1); 5]).split(inner);
    for (i, fill) in Fill::ALL.iter().enumerate() {
        let v = (0.5 + 0.45 * (t * 0.7 + i as f32).sin()).clamp(0.0, 1.0);
        let [label, bar] = Layout::horizontal([Constraint::Length(12), Constraint::Fill(1)]).areas(rows[i]);
        f.render_widget(
            Paragraph::new(format!("{:<8}{:>3}%", fill.name(), (v * 100.0) as u32)),
            label,
        );
        f.render_widget(Meter::new(v, Tone::ALL[(i + app.tone) % 7]).fill(*fill), bar);
    }

    let block = card("Gradients");
    let inner = block.inner(gradients);
    f.render_widget(block, gradients);
    let cols = Layout::horizontal([Constraint::Fill(1); 5])
        .spacing(1)
        .split(inner);
    let specs = [
        GradientSpec::new(Tone::Blue).to(Tone::Purple),
        GradientSpec::new(Tone::Orange)
            .to(Tone::Pink)
            .direction(Direction::Right),
        GradientSpec::new(Tone::Green),
        GradientSpec::new(Tone::Red)
            .to(Tone::Orange)
            .direction(Direction::Radial)
            .matrix(Matrix::Halftone),
        GradientSpec::new(Tone::Hue(190.0))
            .direction(Direction::Down)
            .matrix(Matrix::Bayer4),
    ];
    for (spec, col) in specs.into_iter().zip(cols.iter()) {
        f.render_widget(Gradient::new(spec), *col);
    }
}

fn draw_backgrounds(f: &mut Frame, app: &mut App, area: Rect) {
    let tone = Tone::ALL[app.tone];
    let accent = Tone::ALL[(app.tone + 2) % 7];
    let spec = BackgroundSpec::new(Field::ALL[app.field], tone)
        .accent(accent)
        .density(0.75);
    f.render_widget(Background::new(spec, app.time()), area);
    let w = 44.min(area.width);
    let card_area = Rect::new(area.x + (area.width - w) / 2, area.y + area.height / 2 - 3, w, 6);
    let theme = Theme::default();
    let panel = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(dither_ratatui::raster::color(theme.border)))
        .style(Style::default().bg(dither_ratatui::raster::color(theme.popover)));
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{} background", Field::ALL[app.field].name()),
                Style::default().bold(),
            )),
            Line::from(""),
            Line::from(format!("{} + {} · ordered Bayer 8×8", tone.name(), accent.name()))
                .style(Style::default().fg(dither_ratatui::raster::color(theme.muted))),
        ])
        .centered()
        .block(panel),
        card_area,
    );
}

fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut app = App::new();
    while !app.quit {
        app.tick();
        terminal.draw(|f| draw(f, &mut app))?;
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k.code),
                Event::Mouse(m) => app.on_mouse(m.kind, m.column, m.row),
                _ => {}
            }
        }
    }
    Ok(())
}

/// `--snapshot <dir>`: renders every page off-screen to PPM images (for docs
/// and visual review) instead of starting the interactive app.
fn snapshot(dir: &str) -> io::Result<()> {
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    std::fs::create_dir_all(dir)?;
    let (cols, rows, cw, ch) = (160u16, 50u16, 6usize, 12usize);
    let mut app = App::new();
    app.replay -= Duration::from_secs(2);
    app.hover = Some(7);
    app.pie_hover = Some(1);
    app.buttons[0].hovered = true;
    app.buttons[1].focused = true;
    for (page, name) in [
        (Page::Charts, "charts"),
        (Page::Controls, "controls"),
        (Page::Backgrounds, "backgrounds"),
    ] {
        app.page = page;
        let mut terminal = ratatui::Terminal::new(TestBackend::new(cols, rows)).expect("test backend");
        terminal.draw(|f| draw(f, &mut app)).expect("test backend");
        let buf = terminal.backend().buffer().clone();
        let rgb = |c: Color, d: [u8; 3]| match c {
            Color::Rgb(r, g, b) => [r, g, b],
            _ => d,
        };
        let (w, h) = (cols as usize * cw, rows as usize * ch);
        let mut img = vec![0u8; w * h * 3];
        for y in 0..rows {
            for x in 0..cols {
                let cell = &buf[(x, y)];
                let bg = rgb(cell.bg, [11, 11, 15]);
                let fg = rgb(cell.fg, [231, 231, 234]);
                let sym = cell.symbol();
                for py in 0..ch {
                    for px in 0..cw {
                        let c = match sym {
                            "▀" => {
                                if py < ch / 2 {
                                    fg
                                } else {
                                    bg
                                }
                            }
                            "▄" => {
                                if py >= ch / 2 {
                                    fg
                                } else {
                                    bg
                                }
                            }
                            " " => bg,
                            _ => {
                                if (1..cw - 1).contains(&px)
                                    && (3..ch - 2).contains(&py)
                                    && (px + py) % 2 == 0
                                {
                                    fg
                                } else {
                                    bg
                                }
                            }
                        };
                        let i = ((y as usize * ch + py) * w + x as usize * cw + px) * 3;
                        img[i..i + 3].copy_from_slice(&c);
                    }
                }
            }
        }
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        out.extend_from_slice(&img);
        std::fs::write(format!("{dir}/{name}.ppm"), out)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        return snapshot(args.get(i + 1).map_or("snapshots", String::as_str));
    }
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = run(&mut terminal);
    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}
