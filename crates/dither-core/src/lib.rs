//! # dither-core
//!
//! Renderer-neutral engine behind dither-kit-rs: ordered dithering, named
//! tones with perceptual shade ramps, and rasterizers for charts, gradients,
//! backgrounds, buttons, avatars, and meters. Everything draws into a
//! [`PixelGrid`]; the `dither-gpui` and `dither-ratatui` crates paint it.
//!
//! No dependencies, no allocation per frame beyond the grid itself, and
//! fully deterministic output, so snapshots and tests are stable.

pub mod chart;
pub mod color;
pub mod dither;
pub mod ease;
pub mod grid;
pub mod paint;

pub use chart::{
    Cartesian, CartesianKind, Pie, PieGeometry, Radar, RadarGeometry, Scale, Series, Slice, format_value,
    sparkline,
};
pub use color::{Rgba, Tone};
pub use dither::{Fill, Matrix};
pub use grid::{Bloom, PixelGrid, Run};
pub use paint::{Background, ButtonStyle, Direction, Field, Gradient, avatar, button_texture, meter};
