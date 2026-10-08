//! The popup header: robot, "Claude", plan chip and email; freshness and
//! the refresh button on the right.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};

use common::ink::Ink;
use common::ui::{space_xs, space_xxs};

use crate::fl;

/// The freshness text's widest line before it wraps.
const STATUS_MAX: f32 = 120.0;

pub struct Header {
    pub plan: Option<String>,
    /// The line under "Claude": the email, or "Not signed in".
    pub subtitle: Option<String>,
    /// Freshness text and whether it's a warning.
    pub status: Option<(String, bool)>,
    pub can_refresh: bool,
    pub fetching: bool,
}

fn chip<'a, M: 'a>(s: String) -> Element<'a, M> {
    widget::container(widget::text(s).size(12.0).line_height(cosmic::iced::widget::text::LineHeight::Absolute(20.0.into())).font(cosmic::font::semibold()))
        .padding([0, space_xs()])
        .class(theme::Container::custom(|t| {
            let c = t.cosmic();
            widget::container::Style {
                background: Some(cosmic::iced::Color::from(c.background(t.transparent).small_widget).into()),
                border: cosmic::iced::Border { radius: c.corner_radii.radius_xl.into(), ..Default::default() },
                ..Default::default()
            }
        }))
        .into()
}

pub fn header<'a, M: Clone + 'static>(h: Header, robot: Element<'a, M>, refresh: M) -> Element<'a, M> {
    let title = Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(widget::text::heading(fl!("provider-claude"))).push_maybe(h.plan.map(chip));
    let left = Column::new().width(Length::Fill).push(title).push_maybe(h.subtitle.map(|s| widget::text::caption(s).class(Ink::Muted.text())));
    // Long states ("Rate limited · retry in 4m") wrap rather than run into the title.
    let status = h.status.map(|(s, warn)| {
        widget::container(
            widget::text::caption(s).class(if warn { Ink::Warning } else { Ink::Muted }.text()).align_x(cosmic::iced::alignment::Horizontal::Right),
        )
        .max_width(STATUS_MAX)
    });
    let button = widget::button::icon(widget::icon::from_name("view-refresh-symbolic").size(16))
        .on_press_maybe(h.can_refresh.then_some(refresh))
        .tooltip(if h.fetching { fl!("refreshing") } else { fl!("refresh-now") });
    applet::padded_control(Row::new().spacing(space_xs()).align_y(Alignment::Center).push(robot).push(left).push_maybe(status).push(button))
        .padding([space_xxs(), 12, space_xxs(), 24])
        .into()
}
