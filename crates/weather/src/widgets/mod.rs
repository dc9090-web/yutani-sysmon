//! The popup's pieces and the panel button's content.

pub mod alert;
pub mod banner;
pub mod current;
pub mod daily;
pub mod details;
pub mod header;
pub mod hourly;
pub mod panel;

use cosmic::iced::Color;

use crate::config::{IconSet, Units};

/// What every section needs to draw a snapshot.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub units: Units,
    pub icons: IconSet,
    /// 24-hour clock, from the time applet.
    pub military: bool,
    /// Offline: the content is dimmed (SPEC §10).
    pub dim: bool,
}

/// Opacity of dimmed content.
pub const DIM: f32 = 0.55;

/// `wx-temp`: the hourly line and the range segments.
pub fn wx_temp(t: &cosmic::Theme) -> Color {
    t.cosmic().palette.accent_orange.into()
}

/// `wx-rain`: rain chance text.
pub fn wx_rain(t: &cosmic::Theme) -> Color {
    t.cosmic().palette.accent_blue.into()
}

/// `wx-rain-fill`: `wx-rain` at 25% (dark) or 20% (light).
pub fn wx_rain_fill(t: &cosmic::Theme) -> Color {
    let c = wx_rain(t);
    Color { a: if t.cosmic().is_dark { 0.25 } else { 0.20 }, ..c }
}

/// `wx-now`: `background.on`.
pub fn wx_now(t: &cosmic::Theme) -> Color {
    t.cosmic().background(t.transparent).on.into()
}

/// `c` at the dimmed opacity when `dim`.
pub fn fade(c: Color, dim: bool) -> Color {
    if dim { Color { a: c.a * DIM, ..c } } else { c }
}

/// Icon opacity for `dim`.
pub fn opacity(dim: bool) -> f32 {
    if dim { DIM } else { 1.0 }
}
