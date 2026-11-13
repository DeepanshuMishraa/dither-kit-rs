//! Chrome colors for axes, labels, legends, and tooltips.

use dither_core::Rgba;

/// Text and surface colors used around the dithered pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Color translucent pixels are blended over when a cell has no
    /// truecolor background of its own.
    pub background: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub border: Rgba,
    pub popover: Rgba,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            background: Rgba::hex(0x0b0b0f),
            foreground: Rgba::hex(0xe7e7ea),
            muted: Rgba::hex(0x8a8a94),
            border: Rgba::hex(0x2c2c34),
            popover: Rgba::hex(0x16161c),
        }
    }

    pub const fn light() -> Self {
        Self {
            background: Rgba::hex(0xfafafa),
            foreground: Rgba::hex(0x18181b),
            muted: Rgba::hex(0x71717a),
            border: Rgba::hex(0xe4e4e7),
            popover: Rgba::hex(0xffffff),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
