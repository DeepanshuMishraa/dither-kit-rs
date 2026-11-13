//! # dither-gpui
//!
//! Dithered charts, buttons, gradients, animated backgrounds, avatars, and
//! meters for [GPUI Kit](https://gpui-kit.com) desktop apps.
//!
//! Pixels come from [`dither_core`] and are painted as GPU quads (horizontal
//! runs merged). Motion — entrance tweens, hover springs, tooltip glide, and
//! the procedural clock — uses [`gpui_motion_kit`], which honours the OS
//! reduced-motion preference. Chrome (axes, tooltips, legends) uses the
//! active Kit theme tokens, and buttons wrap Kit's `Button`, so focus,
//! keyboard activation, and accessibility come from Kit.
//!
//! ```ignore
//! use dither_gpui::prelude::*;
//!
//! DitherChart::area("visitors", vec![
//!     Series::new("Desktop", Tone::Blue, vec![186., 305., 237., 73., 209., 214.]),
//!     Series::new("Mobile", Tone::Pink, vec![80., 200., 120., 190., 130., 140.]),
//! ])
//! .labels(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
//! .bloom(Bloom::Low)
//! ```

pub mod button;
pub mod charts;
pub mod pixels;
pub mod surfaces;

pub use button::DitherButton;
pub use charts::{
    DitherChart, DitherPie, DitherRadar, DitherSparkline, LegendEntry, legend, swatch, tone_color,
};
pub use pixels::{Clock, hsla, paint_grid, pixel_canvas};
pub use surfaces::{DitherAvatar, DitherBackground, DitherGradient, DitherMeter};

pub use dither_core;

/// Everything needed to build dithered UI.
pub mod prelude {
    pub use crate::{
        DitherAvatar, DitherBackground, DitherButton, DitherChart, DitherGradient, DitherMeter, DitherPie,
        DitherRadar, DitherSparkline,
    };
    pub use dither_core::{
        Bloom, ButtonStyle, CartesianKind, Direction, Field, Fill, Matrix, Series, Slice, Tone,
    };
    pub use gpui_kit::component::Disableable;
}
