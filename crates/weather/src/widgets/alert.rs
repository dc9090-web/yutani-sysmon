//! Alert banners (SPEC §9.1): the warning icon, the event, the sender and
//! local times; clicking shows the description, clamped to 4 lines.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::ui::{space_m, space_xs, space_xxs};

use crate::fl;
use crate::format;
use crate::model::Alert;

/// About 4 lines of caption text across the banner.
const CLAMP_CHARS: usize = 4 * 46;

pub fn alert<'a, M: Clone + 'static>(a: &Alert, tz: i32, military: bool, lang: &str, open: bool, toggle: M) -> Element<'a, M> {
    let meta = fl!("alert-meta", sender = a.sender.clone(), start = format::clock(Some(a.start), tz, military), end = format::clock(Some(a.end), tz, military));
    let mut text = Column::new()
        .width(Length::Fill)
        .push(widget::text::body(a.event.clone()).font(cosmic::font::semibold()).class(Ink::On.text()))
        .push(widget::text::caption(meta).class(Ink::Muted.text()));
    if open && let Some(d) = a.description(lang) {
        text =
            text.push(widget::container(widget::text::caption(format::ellipsize(d.trim(), CLAMP_CHARS)).class(Ink::On.text())).padding([space_xxs(), 0, 0, 0]));
    }
    let content = Row::new()
        .spacing(space_xs())
        .align_y(Alignment::Start)
        .push(widget::icon::from_name("dialog-warning-symbolic").size(16).symbolic(true).icon().class(Ink::Warning.svg()))
        .push(text);
    let boxed = widget::container(content).padding([space_xs(), space_xs()]).width(Length::Fill).class(theme::Container::custom(|t| {
        let c = t.cosmic();
        widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: c.corner_radii.radius_s.into(), ..Default::default() },
            ..Default::default()
        }
    }));
    let button = widget::button::custom(boxed).class(theme::Button::Image).padding(0).width(Length::Fill).on_press(toggle);
    widget::container(button).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}
