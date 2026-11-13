//! Small easing helpers for backends without a motion engine (e.g. TUIs).
//! GPUI components use `gpui-motion-kit` springs instead.

/// Cubic ease-out: fast start, gentle landing.
pub fn out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Cubic ease-in-out.
pub fn in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Exponential approach of `current` toward `target` over `dt` seconds with
/// time constant `tau`; frame-rate independent smoothing.
pub fn approach(current: f32, target: f32, dt: f32, tau: f32) -> f32 {
    target + (current - target) * (-dt / tau.max(1e-4)).exp()
}
