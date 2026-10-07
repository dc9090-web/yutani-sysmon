//! Popup pieces: stats grid, meters, section header.

use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::panel::Type;
use common::ui::{dot, mono, space_xs, space_xxs, space_xxxs};

/// One stat: key over value. `hot` tints the value and suffixes the key.
pub struct Stat {
    pub key: String,
    pub value: String,
    pub hot: bool,
}

/// A 3-column grid of stats.
pub fn stats<'a, M: 'a>(items: Vec<Stat>) -> Element<'a, M> {
    let mut row = Row::new().spacing(space_xs()).width(Length::Fill);
    for s in items {
        row = row.push(Column::new().width(Length::FillPortion(1)).push(widget::text::caption(s.key).class(Ink::Muted.text())).push(mono(
            s.value,
            Type::new(14.0, 20.0),
            Weight::Normal,
            s.hot.then_some(Ink::Warning),
        )));
    }
    row.into()
}

/// One meter: label and value on a row, then a 6 px bar.
pub struct Meter {
    pub label: String,
    pub value: String,
    /// 0..=1; `None` draws an empty track.
    pub frac: Option<f32>,
    pub ink: Ink,
}

fn bar<'a, M: 'a>(frac: Option<f32>, ink: Ink) -> Element<'a, M> {
    let f = frac.unwrap_or(0.0).clamp(0.0, 1.0);
    let filled = (f * 1000.0).round() as u16;
    let pill = move |t: &cosmic::Theme, color: cosmic::iced::Color| widget::container::Style {
        background: Some(color.into()),
        border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
        ..Default::default()
    };
    let mut row = Row::new().width(Length::Fill).height(Length::Fixed(6.0));
    if filled > 0 {
        row = row.push(
            widget::container(widget::space().width(Length::Fill).height(Length::Fixed(6.0)))
                .width(Length::FillPortion(filled))
                .class(theme::Container::custom(move |t| pill(t, ink.color(t)))),
        );
    }
    if filled < 1000 {
        row = row.push(widget::space().width(Length::FillPortion(1000 - filled)).height(Length::Fixed(6.0)));
    }
    widget::container(row).width(Length::Fill).class(theme::Container::custom(move |t| pill(t, Ink::Component.color(t)))).into()
}

/// A 2-column grid of meters.
pub fn meters<'a, M: 'a>(items: Vec<Meter>) -> Element<'a, M> {
    let mut row = Row::new().spacing(space_xs()).width(Length::Fill);
    for m in items {
        row = row.push(
            Column::new()
                .width(Length::FillPortion(1))
                .spacing(space_xxxs())
                .push(Row::new().align_y(Alignment::Center).push(widget::text::caption(m.label).class(Ink::Muted.text()).width(Length::Fill)).push(mono(
                    m.value,
                    Type::new(12.0, 17.0),
                    Weight::Normal,
                    None,
                )))
                .push(bar(m.frac, m.ink)),
        );
    }
    row.into()
}

/// Section top row: dot and name with the caption under them; headline on
/// the right.
pub fn top_row<'a, M: 'a>(ink: Ink, name: String, caption: Element<'a, M>, headline: Option<Element<'a, M>>) -> Element<'a, M> {
    let title = Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(dot(ink)).push(widget::text::heading(name));
    let left = Column::new().width(Length::Fill).push(title).push(caption);
    let mut row = Row::new().align_y(Alignment::Start).push(left);
    if let Some(h) = headline {
        row = row.push(h);
    }
    row.into()
}
