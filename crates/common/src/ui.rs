//! Popup building blocks shared by both applets.

use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::font::Weight;
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::{Alignment, Font, Length};
use cosmic::widget::{self, Row, divider};
use cosmic::{Element, applet, theme};

use crate::format::Split;
use crate::ink::Ink;
use crate::panel::Type;

pub fn mono_font(weight: Weight) -> Font {
    Font { weight, ..cosmic::font::mono() }
}

/// Mono text at an exact size and line height.
pub fn mono<'a>(s: impl Into<String>, ty: Type, weight: Weight, ink: Option<Ink>) -> widget::Text<'a, cosmic::Theme> {
    let t = widget::text(s.into()).size(ty.size).line_height(LineHeight::Absolute(ty.line.into())).font(mono_font(weight));
    match ink {
        Some(i) => t.class(i.text()),
        None => t,
    }
}

/// A text cell of fixed width, aligned within it.
pub fn cell<'a, M: 'a>(content: impl Into<Element<'a, M>>, width: f32, align: Horizontal) -> Element<'a, M> {
    widget::container(content).width(Length::Fixed(width)).align_x(align).align_y(Vertical::Center).into()
}

pub fn space_xxs() -> u16 {
    theme::spacing().space_xxs
}
pub fn space_xxxs() -> u16 {
    theme::spacing().space_xxxs
}
pub fn space_xs() -> u16 {
    theme::spacing().space_xs
}
pub fn space_s() -> u16 {
    theme::spacing().space_s
}
pub fn space_m() -> u16 {
    theme::spacing().space_m
}

/// Inset divider between popup sections.
pub fn inset_divider<'a, M: 'a>() -> Element<'a, M> {
    applet::padded_control(divider::horizontal::default()).padding([space_xxs(), space_s()]).into()
}

/// "⚙ Applet settings ›".
pub fn settings_row<'a, M: Clone + 'static>(label: String, msg: M) -> Element<'a, M> {
    applet::menu_button(
        Row::new()
            .spacing(space_xs())
            .align_y(Alignment::Center)
            .push(widget::icon::from_name("emblem-system-symbolic").size(16).symbolic(true))
            .push(widget::text::body(label).width(Length::Fill))
            .push(widget::icon::from_name("go-next-symbolic").size(16).symbolic(true)),
    )
    .on_press(msg)
    .into()
}

/// Settings page header: Back and the title, then a full-width divider.
pub fn back_header<'a, M: Clone + 'static>(title: String, back_label: String, msg: M) -> Element<'a, M> {
    let back = widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(16)).on_press(msg).tooltip(back_label).class(theme::Button::Standard);
    widget::Column::new()
        .push(
            widget::container(Row::new().spacing(space_xs()).align_y(Alignment::Center).push(back).push(widget::text::heading(title)))
                .padding([space_xxs(), space_s()]),
        )
        .push(divider::horizontal::default())
        .into()
}

/// A caption-heading label over a settings group.
pub fn group_label<'a, M: 'a>(label: String) -> Element<'a, M> {
    applet::padded_control(widget::text::caption_heading(label)).padding([space_xxs(), space_m(), 0, space_m()]).into()
}

/// A series swatch: a filled bar (area series) or a line (line series).
pub fn swatch<'a, M: 'a>(ink: Ink, area: bool) -> Element<'a, M> {
    let (w, h) = (16.0, if area { 8.0 } else { 3.0 });
    widget::container(widget::space().width(Length::Fixed(w)).height(Length::Fixed(h)))
        .class(theme::Container::custom(move |t| {
            let c = ink.color(t);
            widget::container::Style {
                background: Some(if area { crate::ink::fill(t, ink) } else { c }.into()),
                border: cosmic::iced::Border { radius: 2.0.into(), ..Default::default() },
                ..Default::default()
            }
        }))
        .into()
}

/// Download/Upload (or Read/Write) readout: swatch and caption, then the
/// value in mono 20/30 bold with the unit as a caption.
pub fn rate_readout<'a, M: 'a>(label: String, ink: Ink, area: bool, value: &Split) -> Element<'a, M> {
    let head =
        Row::new().spacing(space_xxxs() + 2).align_y(Alignment::Center).push(swatch(ink, area)).push(widget::text::caption(label).class(Ink::Muted.text()));
    let unit = if value.unit.is_empty() { None } else { Some(widget::text::caption(value.unit).class(Ink::Muted.text())) };
    let val = Row::new()
        .spacing(space_xxxs() + 2)
        .align_y(Alignment::End)
        .push(mono(value.trimmed().to_owned(), Type::new(20.0, 30.0), Weight::Bold, None))
        .push_maybe(unit.map(|u| widget::container(u).padding([0, 0, 5, 0])));
    widget::Column::new().push(head).push(val).width(Length::Fill).into()
}

/// The `bg-component` well with `radius_s` that holds a graph.
pub fn well<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    widget::container(content)
        .class(theme::Container::custom(|t| {
            let c = t.cosmic();
            widget::container::Style {
                background: Some(Ink::Component.color(t).into()),
                border: cosmic::iced::Border { radius: c.corner_radii.radius_s.into(), ..Default::default() },
                ..Default::default()
            }
        }))
        .into()
}

/// A radio row inside a `menu_button`: ring, optional icon, name and caption.
pub fn radio_row<'a, M: Clone + 'static>(selected: bool, icon: Option<&'static str>, name: String, caption: Option<(String, Ink)>, msg: M) -> Element<'a, M> {
    let mut text = widget::Column::new().push(widget::text::body(name));
    if let Some((c, ink)) = caption {
        text = text.push(widget::text::caption(c).class(ink.text()));
    }
    let mut row = Row::new().spacing(space_xs()).align_y(Alignment::Center).push(radio_dot(selected));
    if let Some(i) = icon {
        row = row.push(widget::icon::from_name(i).size(16).symbolic(true));
    }
    applet::menu_button(row.push(text.width(Length::Fill))).on_press(msg).into()
}

/// 16 px radio: a 2 px ring, accent with an 8 px dot when selected.
fn radio_dot<'a, M: 'a>(selected: bool) -> Element<'a, M> {
    let dot = widget::container(widget::space().width(Length::Fixed(8.0)).height(Length::Fixed(8.0))).class(theme::Container::custom(move |t| {
        widget::container::Style {
            background: selected.then(|| cosmic::iced::Color::from(t.cosmic().accent.base).into()),
            border: cosmic::iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        }
    }));
    widget::container(dot)
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .center(Length::Fixed(16.0))
        .class(theme::Container::custom(move |t| widget::container::Style {
            border: cosmic::iced::Border { radius: 8.0.into(), width: 2.0, color: if selected { t.cosmic().accent.base.into() } else { Ink::Muted.color(t) } },
            ..Default::default()
        }))
        .into()
}

/// An 8 px dot in a metric colour.
pub fn dot<'a, M: 'a>(ink: Ink) -> Element<'a, M> {
    widget::container(widget::space().width(Length::Fixed(8.0)).height(Length::Fixed(8.0)))
        .class(theme::Container::custom(move |t| widget::container::Style {
            background: Some(ink.color(t).into()),
            border: cosmic::iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}
