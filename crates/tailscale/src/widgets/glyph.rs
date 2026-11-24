//! The panel icon: Tailscale's dot-grid mark with a status circle over its
//! corner. The mark is embedded, so it shows before (or without) an install;
//! the installed copy is for the applet list.

use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::widget::{self, icon};
use cosmic::{Element, theme};

use common::ink::Ink;

use crate::model::Dot;

const MARK: &[u8] = include_bytes!("../../resources/icons/io.github.dc.CosmicAppletTailscale-symbolic.svg");
/// The circle is this fraction of the icon's size, and never under 7 px.
const DOT_SHARE: f32 = 0.42;
const HALO: f32 = 2.0;

/// The circle's colour, from the theme's palette. The palette's red and
/// orange stay apart in dark themes, where `warning` and `destructive` don't.
fn dot_color(dot: Dot, t: &cosmic::Theme) -> cosmic::iced::Color {
    let p = &t.cosmic().palette;
    match dot {
        Dot::Red => p.accent_red.into(),
        Dot::Orange => p.accent_orange.into(),
        Dot::Green => p.accent_green.into(),
    }
}

/// The mark at `size` px, in the theme's foreground. With `dot`, the status
/// circle sits over its bottom-right corner; the width is the same either way.
pub fn glyph<'a, M: 'a>(dot: Option<Dot>, size: f32) -> Element<'a, M> {
    let image = widget::icon(icon::from_svg_bytes(MARK).symbolic(true)).size(size as u16).class(Ink::On.svg());
    let base = widget::container(image).padding([HALO, HALO, HALO, HALO]);
    let Some(dot) = dot else { return base.into() };
    let d = (size * DOT_SHARE).round().max(7.0);
    let circle = widget::container(widget::space().width(Length::Fixed(d + 2.0 * HALO)).height(Length::Fixed(d + 2.0 * HALO))).class(theme::Container::custom(
        move |t| widget::container::Style {
            background: Some(dot_color(dot, t).into()),
            border: cosmic::iced::Border { radius: (d / 2.0 + HALO).into(), width: HALO, color: t.cosmic().background(t.transparent).base.into() },
            ..Default::default()
        },
    ));
    let corner = widget::container(circle)
        .width(Length::Fixed(size + 2.0 * HALO))
        .height(Length::Fixed(size + 2.0 * HALO))
        .align_x(Horizontal::Right)
        .align_y(Vertical::Bottom);
    cosmic::iced::widget::stack([base.into(), corner.into()]).into()
}
