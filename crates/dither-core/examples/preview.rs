//! Renders a contact sheet of every dither-kit surface to a PPM image.
//!
//! These are the exact pixel grids the GPUI and Ratatui backends paint, so
//! the sheet is a renderer-independent visual check.
//!
//! `cargo run -p dither-core --example preview -- out.ppm`

use dither_core::*;

const BG: Rgba = Rgba::hex(0x0b0b0f);
const SCALE: usize = 3;

struct Sheet {
    w: usize,
    h: usize,
    px: Vec<Rgba>,
}

impl Sheet {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![BG; w * h],
        }
    }

    /// Places `grid` at grid-pixel position `(gx, gy)`, scaled up.
    fn put(&mut self, grid: &PixelGrid, gx: usize, gy: usize) {
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(c) = grid.get(x, y) else { continue };
                for sy in 0..SCALE {
                    for sx in 0..SCALE {
                        let (px, py) = ((gx + x) * SCALE + sx, (gy + y) * SCALE + sy);
                        if px < self.w && py < self.h {
                            let i = py * self.w + px;
                            self.px[i] = c.over(self.px[i]);
                        }
                    }
                }
            }
        }
    }

    fn ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.w, self.h).into_bytes();
        for c in &self.px {
            out.extend_from_slice(&[c.r, c.g, c.b]);
        }
        out
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "preview.ppm".into());
    let mut s = Sheet::new(400 * SCALE, 330 * SCALE);

    let traffic = [
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
    ];
    let mut area = Cartesian::new(CartesianKind::Area, &traffic);
    area.hover = Some(7);
    area.bloom = Bloom::Low;
    s.put(&area.render(250, 90), 6, 6);

    let mut pie_slices = vec![
        Slice::new("Chrome", 275., Tone::Blue),
        Slice::new("Safari", 200., Tone::Pink),
        Slice::new("Firefox", 187., Tone::Orange),
        Slice::new("Edge", 173., Tone::Green),
        Slice::new("Other", 90., Tone::Grey),
    ];
    pie_slices[2].fill = Fill::Dotted;
    let mut pie = Pie::new(&pie_slices);
    pie.inner_radius = 0.55;
    pie.hover = Some(1);
    s.put(&pie.render(96, 96), 290, 3);

    let mut bars = Cartesian::new(CartesianKind::Bar, &traffic[..2]);
    bars.stacked = true;
    bars.hover = Some(4);
    s.put(&bars.render(120, 70), 6, 108);

    let mut line_series = traffic.to_vec();
    line_series[1].fill = Fill::Hatched;
    let line = Cartesian::new(CartesianKind::Line, &line_series[..2]);
    s.put(&line.render(120, 70), 134, 108);

    let radar_series = [
        Series::new("Rook", Tone::Purple, vec![80., 95., 60., 70., 85., 65.]),
        Series::new("Knight", Tone::Green, vec![65., 60., 90., 85., 55., 80.]),
    ];
    s.put(&Radar::new(&radar_series).render(84, 84), 290, 106);

    for (i, (tone, style, density)) in [
        (Tone::Green, ButtonStyle::Solid, 0.32),
        (Tone::Green, ButtonStyle::Solid, 0.85),
        (Tone::Blue, ButtonStyle::Outline, 0.3),
        (Tone::Pink, ButtonStyle::Ghost, 0.5),
    ]
    .into_iter()
    .enumerate()
    {
        let mut b = button_texture(56, 14, tone, style, density, 1.3);
        if let Some(surface) = style.surface(tone) {
            let mut bg = PixelGrid::new(56, 14);
            for y in 0..14 {
                for x in 0..56 {
                    bg.set(x, y, surface);
                }
            }
            bg.overlay(&b, 0, 0);
            b = bg;
        }
        s.put(&b, 6 + i * 62, 190);
    }

    for (i, name) in [
        "Ada Lovelace",
        "Grace Hopper",
        "Alan Turing",
        "Linus",
        "Margaret H.",
        "Ken Thompson",
    ]
    .iter()
    .enumerate()
    {
        let a = avatar(name, None);
        let mut big = PixelGrid::new(24, 24);
        for y in 0..24 {
            for x in 0..24 {
                big.plot(x as i32, y as i32, a.get(x / 3, y / 3));
            }
        }
        s.put(&big, 256 + (i % 3) * 30, 186 + (i / 3) * 30);
    }

    for (i, f) in Fill::ALL.into_iter().enumerate() {
        s.put(
            &meter(120, 4, 0.35 + 0.18 * i as f32, Tone::ALL[i], f),
            6 + (i % 2) * 125,
            212 + (i / 2) * 8,
        );
    }

    let grads = [
        Gradient::new(Tone::Blue).to(Tone::Purple),
        Gradient::new(Tone::Orange)
            .to(Tone::Pink)
            .direction(Direction::Right),
        Gradient::new(Tone::Green),
        Gradient::new(Tone::Red)
            .to(Tone::Orange)
            .direction(Direction::Radial)
            .matrix(Matrix::Halftone),
    ];
    for (i, g) in grads.iter().enumerate() {
        s.put(&g.render(46, 90), 6 + i * 50, 234);
    }
    let bg = Background::new(Field::Aurora, Tone::Green)
        .accent(Tone::Blue)
        .density(0.8);
    s.put(&bg.render(186, 90, 2.0), 208, 244);

    std::fs::write(&path, s.ppm()).expect("write preview");
    println!("wrote {path}");
}
