//! Current conditions (SPEC §9.1): a 48 px icon, the temperature, the
//! description and "Feels like"; today's high and low on the right.

use cosmic::Element;
use cosmic::iced::font::Weight;
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};

use common::ink::Ink;
use common::panel::Type;
use common::ui::{mono, space_m, space_s, space_xxs};

use super::{Ctx, opacity};
use crate::fl;
use crate::format;
use crate::icons;
use crate::model::Snapshot;

const MONO: Type = Type::new(14.0, 20.0);

pub fn current<'a, M: 'a>(s: &Snapshot, ctx: Ctx) -> Element<'a, M> {
    let c = &s.current;
    let wind = format::wind_ms(c.wind_speed, ctx.units);
    let icon = widget::icon(icons::condition(ctx.icons, &c.condition, Some(wind))).size(48).opacity(opacity(ctx.dim));
    let text = |ink: Ink| if ctx.dim { Ink::Muted.text() } else { ink.text() };
    let hero = widget::text(format::temp(Some(c.temp)))
        .size(40.0)
        .line_height(LineHeight::Absolute(48.0.into()))
        .font(cosmic::iced::Font { weight: Weight::Light, ..cosmic::font::default() })
        .class(text(Ink::On));
    let cond = Column::new()
        .width(Length::Fill)
        .push(hero)
        .push(widget::text::body(format::sentence(&c.condition.description)).class(text(Ink::On)))
        .push(widget::text::caption(fl!("feels-like", temp = format::temp(Some(c.feels_like)))).class(Ink::Muted.text()));
    let today = s.daily.first();
    let hl = Column::new()
        .align_x(Alignment::End)
        .push(mono(fl!("high", temp = format::temp(today.map(|d| d.max))), MONO, Weight::Normal, Some(if ctx.dim { Ink::Muted } else { Ink::On })))
        .push(mono(fl!("low", temp = format::temp(today.map(|d| d.min))), MONO, Weight::Normal, Some(Ink::Muted)));
    widget::container(Row::new().spacing(space_s()).align_y(Alignment::Center).push(icon).push(cond).push(hl))
        .padding([space_xxs(), space_m()])
        .width(Length::Fill)
        .into()
}
