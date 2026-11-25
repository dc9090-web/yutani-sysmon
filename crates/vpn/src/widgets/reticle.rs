//! The reticle icon with its issue dot (DESIGN §3), in Mono or Neon.
//!
//! Neon tints the reticle with the theme's cyan (`palette.accent_blue`), or
//! pink (`accent_pink`) when there's an issue, over a soft glow drawn as
//! stacked translucent discs. Off stays muted with no glow. Auto draws Neon
//! while a tunnel is up and Mono while none is.
//!
//! The two chevrons are the torrent and web tunnels. The browser tunnel is a
//! third mark, a short bar that closes the bottom of the frame while it's up.

use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Color, Length, Point, Rectangle};
use cosmic::widget::{self, icon};
use cosmic::{Element, Renderer, theme};

use common::graph::layered;
use common::ink::Ink;

use crate::config::IconStyle;
use crate::model::{Issue, Reticle};

const DOT: f32 = 7.0;

/// The reticles are embedded, so they show before (or without) an install;
/// the installed copies are for the applet list.
fn svg(r: Reticle) -> &'static str {
    match r {
        Reticle::Off => include_str!("../../resources/icons/vpn-reticle-off-symbolic.svg"),
        Reticle::P2p => include_str!("../../resources/icons/vpn-reticle-p2p-symbolic.svg"),
        Reticle::Web => include_str!("../../resources/icons/vpn-reticle-web-symbolic.svg"),
        Reticle::Both => include_str!("../../resources/icons/vpn-reticle-both-symbolic.svg"),
    }
}

/// The reticle's SVG, with the browser tunnel's bar on the frame's bottom
/// edge (which sits 1 px in when both chevrons are solid).
fn svg_bytes(r: Reticle, browser: bool) -> std::borrow::Cow<'static, [u8]> {
    let base = svg(r);
    if !browser {
        return base.as_bytes().into();
    }
    let y = if r == Reticle::Both { 13.35 } else { 14.35 };
    let bar = format!(r##"<rect x="6.25" y="{y}" width="3.5" height="1.3" fill="#222222"/></svg>"##);
    base.replacen("</svg>", &bar, 1).into_bytes().into()
}
const HALO: f32 = 2.0;

fn neon_color(t: &cosmic::Theme, issue: bool) -> Color {
    let p = &t.cosmic().palette;
    if issue { p.accent_pink.into() } else { p.accent_blue.into() }
}

/// The glow behind a lit Neon reticle.
struct Glow {
    issue: bool,
}

impl<M> canvas::Program<M, cosmic::Theme, Renderer> for Glow {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = neon_color(theme, self.issue);
        let centre = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = bounds.width.min(bounds.height) / 2.0;
        // Many faint discs, largest first: denser towards the middle.
        for i in 0..8 {
            let f = 1.0 - i as f32 * 0.09;
            frame.fill(&Path::circle(centre, r * f), Color { a: 0.05, ..c });
        }
        vec![frame.into_geometry()]
    }
}

/// The reticle at `size` px, with room for the dot so the width never changes.
pub fn reticle<'a, M: 'a>(r: Reticle, browser: bool, issue: Option<Issue>, style: IconStyle, size: f32) -> Element<'a, M> {
    let handle = icon::from_svg_bytes(svg_bytes(r, browser)).symbolic(true);
    let lit = r != Reticle::Off || browser;
    let style = style.resolve(lit);
    let is_issue = issue.is_some();
    let glyph = widget::icon(handle).size(size as u16);
    let glyph: Element<'a, M> = match style {
        IconStyle::Mono | IconStyle::Auto => glyph.class(Ink::On.svg()).into(),
        IconStyle::Neon if !lit && !is_issue => glyph.class(Ink::Muted.svg()).into(),
        IconStyle::Neon => {
            let tinted = glyph.class(theme::Svg::Custom(std::rc::Rc::new(move |t| cosmic::iced::widget::svg::Style { color: Some(neon_color(t, is_issue)) })));
            // The glow reaches a little past the glyph.
            let g = size + 2.0 * HALO;
            cosmic::iced::widget::stack([
                layered(Glow { issue: is_issue }, Length::Fixed(g), Length::Fixed(g)),
                widget::container(tinted).center(Length::Fixed(g)).into(),
            ])
            .into()
        }
    };
    let base = if style == IconStyle::Neon && (lit || is_issue) {
        widget::container(glyph).width(Length::Fixed(size + 2.0 * HALO)).height(Length::Fixed(size + 2.0 * HALO))
    } else {
        widget::container(glyph).padding([HALO, HALO, HALO, HALO])
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_bar() {
        for r in [Reticle::Off, Reticle::P2p, Reticle::Web, Reticle::Both] {
            assert_eq!(&*svg_bytes(r, false), svg(r).as_bytes());
            let with = String::from_utf8(svg_bytes(r, true).into_owned()).unwrap();
            assert!(with.trim_end().ends_with("/></svg>") && with.matches("</svg>").count() == 1 && with.contains("<rect "), "{with}");
        }
    }
}
