//! State banners (SPEC §10): a component-coloured box with a title, text
//! and a button.

use cosmic::iced::Length;
use cosmic::widget::{self, Column};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::ui::{space_m, space_xs, space_xxs};

pub fn banner<'a, M: Clone + 'static>(title: String, body: String, action: Option<(String, M)>) -> Element<'a, M> {
    let mut col = Column::new()
        .spacing(space_xxs())
        .push(widget::text::body(title).font(cosmic::font::semibold()))
        .push(widget::text::caption(body).class(Ink::Muted.text()));
    if let Some((label, msg)) = action {
        col = col.push(widget::button::standard(label).on_press(msg));
    }
    let boxed = widget::container(col).padding([space_xs(), space_xs()]).width(Length::Fill).class(theme::Container::custom(|t| {
        let c = t.cosmic();
        widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: c.corner_radii.radius_s.into(), ..Default::default() },
            ..Default::default()
        }
    }));
    widget::container(boxed).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}
