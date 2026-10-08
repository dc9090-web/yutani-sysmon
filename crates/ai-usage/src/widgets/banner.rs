//! State banners: a component-coloured box with a title, text and an
//! optional button.

use cosmic::iced::Length;
use cosmic::iced::font::Weight;
use cosmic::iced::widget::text::Span;
use cosmic::widget::{self, Column};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::ui::{mono_font, space_m, space_xs, space_xxs};

/// Caption text where `code` spans render in the mono font.
fn caption_with_code<'a, M: 'a>(s: String) -> Element<'a, M> {
    let spans: Vec<Span<'a, (), cosmic::font::Font>> = s
        .split('`')
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(i, part)| {
            let span = Span::new(part.to_owned());
            if i % 2 == 1 { span.font(mono_font(Weight::Normal)) } else { span }
        })
        .collect();
    cosmic::iced::widget::rich_text(spans).size(12.0).line_height(cosmic::iced::widget::text::LineHeight::Absolute(17.0.into())).class(Ink::Muted.text()).into()
}

pub fn banner<'a, M: Clone + 'static>(title: String, body: String, action: Option<(String, M)>) -> Element<'a, M> {
    let mut col = Column::new().spacing(space_xxs()).push(widget::text::body(title).font(cosmic::font::semibold())).push(caption_with_code(body));
    if let Some((label, msg)) = action {
        col = col.push(widget::button::standard(label).on_press(msg));
    }
    let boxed = widget::container(col).padding([space_xxs(), space_xs()]).width(Length::Fill).class(theme::Container::custom(|t| {
        let c = t.cosmic();
        widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: c.corner_radii.radius_s.into(), ..Default::default() },
            ..Default::default()
        }
    }));
    widget::container(boxed).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}
