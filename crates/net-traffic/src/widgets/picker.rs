//! The direction-indicator picker: six equal toggle buttons in a pill
//! track, each showing the down icon above the up icon.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, theme};

use common::ink::Ink;

use super::rates::indicator;
use crate::config::Indicator;

pub fn picker<'a, M: Clone + 'static>(selected: Indicator, on_pick: impl Fn(Indicator) -> M) -> Element<'a, M> {
    let mut row = Row::new().spacing(2).width(Length::Fill);
    for ind in Indicator::ALL {
        let on = ind == selected;
        let (d, u) = if on { (Ink::OnAccent, Ink::OnAccent) } else { (Ink::Blue, Ink::Orange) };
        let glyphs = Column::new().align_x(Alignment::Center).push(indicator(ind, true, 14.0, 14.0, 10.0, d)).push(indicator(ind, false, 14.0, 14.0, 10.0, u));
        let btn = widget::button::custom(widget::container(glyphs).center_x(Length::Fill).padding([4, 0]))
            .width(Length::Fill)
            .on_press(on_pick(ind))
            .class(if on { theme::Button::Suggested } else { theme::Button::Text });
        row = row.push(btn);
    }
    widget::container(row)
        .padding(2)
        .width(Length::Fill)
        .class(theme::Container::custom(|t| widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}
