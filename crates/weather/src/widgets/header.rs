//! The popup header (SPEC §9.1): the city and region as one button that
//! opens the location list; freshness text and the refresh button.

use cosmic::iced::alignment::Horizontal;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::ui::{space_m, space_xs, space_xxs, space_xxxs};

use crate::fl;

/// The freshness text's widest line before it wraps.
const STATUS_MAX: f32 = 130.0;

pub struct Header<M> {
    pub city: String,
    pub region: String,
    /// `None`: no switcher (one location, or a state with no location).
    pub switch: Option<M>,
    /// Freshness text and whether it's a warning.
    pub status: Option<(String, bool)>,
    /// `None`: no refresh button. `Some(None)`: shown, disabled.
    pub refresh: Option<Option<M>>,
    pub fetching: bool,
}

pub fn header<'a, M: Clone + 'static>(h: Header<M>) -> Element<'a, M> {
    let title = Row::new().spacing(space_xxxs()).align_y(Alignment::Center).push(widget::text::heading(h.city));
    let left = match h.switch {
        Some(msg) => {
            let col = Column::new()
                .push(title.push(widget::icon::from_name("go-down-symbolic").size(16).symbolic(true)))
                .push_maybe((!h.region.is_empty()).then(|| widget::text::caption(h.region.clone()).class(Ink::Muted.text())));
            Element::from(widget::button::custom(col).class(theme::Button::AppletMenu).padding([space_xxxs(), space_xxs()]).on_press(msg))
        }
        None => Column::new()
            .push(title)
            .push_maybe((!h.region.is_empty()).then(|| widget::text::caption(h.region).class(Ink::Muted.text())))
            .padding([space_xxxs(), space_xxs()])
            .into(),
    };
    let status = h.status.map(|(s, warn)| {
        widget::container(widget::text::caption(s).class(if warn { Ink::Warning } else { Ink::Muted }.text()).align_x(Horizontal::Right)).max_width(STATUS_MAX)
    });
    let button = h.refresh.map(|msg| {
        widget::button::icon(widget::icon::from_name("view-refresh-symbolic").size(16)).on_press_maybe(msg).tooltip(if h.fetching {
            fl!("refreshing")
        } else {
            fl!("refresh-now")
        })
    });
    widget::container(
        Row::new().spacing(space_xs()).align_y(Alignment::Center).push(widget::container(left).width(Length::Fill)).push_maybe(status).push_maybe(button),
    )
    .padding([space_xxs(), space_xs(), space_xxs(), space_m() - space_xxs()])
    .into()
}
