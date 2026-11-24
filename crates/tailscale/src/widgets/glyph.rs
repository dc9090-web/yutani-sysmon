//! The mesh glyph with its issue dot. The glyphs are embedded, so they show
//! before (or without) an install; the installed copies are for the applet list.

use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::widget::{self, icon};
use cosmic::{Element, theme};

use common::ink::Ink;

use crate::model::{IconKind, Issue};

const DOT: f32 = 7.0;
const HALO: f32 = 2.0;

fn svg(kind: IconKind) -> &'static [u8] {
    match kind {
        IconKind::Off => include_bytes!("../../resources/icons/tailscale-mesh-off-symbolic.svg"),
        IconKind::Connecting => include_bytes!("../../resources/icons/tailscale-mesh-connecting-symbolic.svg"),
        IconKind::On => include_bytes!("../../resources/icons/tailscale-mesh-on-symbolic.svg"),
        IconKind::Exit => include_bytes!("../../resources/icons/tailscale-mesh-exit-symbolic.svg"),
    }
}

/// The glyph at `size` px, with room for the dot so the width never changes.
pub fn glyph<'a, M: 'a>(kind: IconKind, issue: Option<Issue>, size: f32) -> Element<'a, M> {
    let ink = if kind == IconKind::Off { Ink::Muted } else { Ink::On };
    let image = widget::icon(icon::from_svg_bytes(svg(kind)).symbolic(true)).size(size as u16).class(ink.svg());
    let base = widget::container(image).padding([HALO, HALO, HALO, HALO]);
    let Some(issue) = issue else { return base.into() };
    let ink = match issue {
        Issue::Error => Ink::Destructive,
        Issue::Warning => Ink::Warning,
    };
    let dot = widget::container(widget::space().width(Length::Fixed(DOT + 2.0 * HALO)).height(Length::Fixed(DOT + 2.0 * HALO))).class(
        theme::Container::custom(move |t| widget::container::Style {
            background: Some(ink.color(t).into()),
            border: cosmic::iced::Border { radius: (DOT / 2.0 + HALO).into(), width: HALO, color: t.cosmic().background(t.transparent).base.into() },
            ..Default::default()
        }),
    );
    let corner = widget::container(dot)
        .width(Length::Fixed(size + 2.0 * HALO))
        .height(Length::Fixed(size + 2.0 * HALO))
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top);
    cosmic::iced::widget::stack([base.into(), corner.into()]).into()
}
