//! # dither-ratatui
//!
//! Dithered charts, buttons, gradients, backgrounds, avatars, and meters for
//! [Ratatui](https://ratatui.rs) terminal apps. Pixels come from
//! [`dither_core`] and reach the terminal through a [`Raster`]: real round
//! image dots with kitty graphics (kitty, Ghostty), braille dots elsewhere,
//! or the old half blocks. A truecolor terminal is required. Every widget
//! takes `.shimmer(true)` to animate its dots.
//!
//! ```no_run
//! use dither_ratatui::{Chart, prelude::*};
//! # fn draw(frame: &mut ratatui::Frame) {
//! let series = [Series::new("Desktop", Tone::Blue, vec![186.0, 305.0, 237.0, 73.0, 209.0, 214.0])];
//! let labels = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
//! frame.render_widget(Chart::area(&series).labels(&labels), frame.area());
//! # }
//! ```

pub mod charts;
pub mod kitty;
pub mod raster;
pub mod theme;
pub mod widgets;

pub use charts::{Chart, PieChart, RadarChart, Sparkline};
pub use raster::{Kitty, Raster};
pub use theme::Theme;
pub use widgets::{Avatar, Background, Button, ButtonState, Gradient, Legend, LegendItem, Meter};

/// Re-exported core vocabulary.
pub mod prelude {
    pub use dither_core::{
        Background as BackgroundSpec, Bloom, ButtonStyle, CartesianKind, Direction, Field, Fill,
        Gradient as GradientSpec, Matrix, Rgba, Series, Slice, Tone,
    };
}

pub use dither_core;
