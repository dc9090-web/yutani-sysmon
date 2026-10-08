//! The daily rows (DESIGN §5): day, icon, rain chance, low, the range bar
//! on one week-wide scale, high. Today's bar carries a dot at the current
//! temperature.

use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::panel::Type;
use common::ui::{cell, mono, space_m, space_xs};

use super::{Ctx, fade, opacity, wx_now, wx_rain, wx_temp};
use crate::format;
use crate::icons;
use crate::model::{Snapshot, local_date};

const ROW_H: f32 = 32.0;
const DAY_W: f32 = 48.0;
const POP_W: f32 = 36.0;
const MONO: Type = Type::new(14.0, 20.0);
const TRACK_H: f32 = 6.0;
const DOT: f32 = 10.0;
/// Fill portions for positions on the range bar.
const SCALE: f64 = 1000.0;

/// Position on the week's scale, in `SCALE` units.
fn at(t: f64, lo: f64, span: f64) -> u16 {
    (((t - lo) / span).clamp(0.0, 1.0) * SCALE).round() as u16
}

/// Empty space taking `portion` of the row.
fn gap<'a, M: 'a>(portion: u16) -> Element<'a, M> {
    widget::space().width(Length::FillPortion(portion)).height(Length::Fixed(TRACK_H)).into()
}

/// The 6 px track with the day's segment, and today's dot.
fn range_bar<'a, M: 'a>(lo: u16, hi: u16, dot: Option<u16>, dim: bool) -> Element<'a, M> {
    let hi = hi.max(lo + 1);
    let segment = widget::container(widget::space().width(Length::Fill).height(Length::Fixed(TRACK_H))).width(Length::FillPortion(hi - lo)).class(
        theme::Container::custom(move |t| widget::container::Style {
            background: Some(fade(wx_temp(t), dim).into()),
            border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
            ..Default::default()
        }),
    );
    let track =
        widget::container(Row::new().push_maybe((lo > 0).then(|| gap(lo))).push(segment).push_maybe((hi < SCALE as u16).then(|| gap(SCALE as u16 - hi))))
            .width(Length::Fill)
            .height(Length::Fixed(TRACK_H))
            .class(theme::Container::custom(move |t| widget::container::Style {
                background: Some(fade(Ink::Component.color(t), dim).into()),
                border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
                ..Default::default()
            }));
    let track = widget::container(track).height(Length::Fixed(DOT + 4.0)).align_y(Alignment::Center);
    let Some(dot) = dot else { return track.into() };
    // The dot's centre runs from DOT/2 to width − DOT/2 as `dot` goes 0 → SCALE.
    let ring = widget::container(widget::space().width(Length::Fixed(DOT + 4.0)).height(Length::Fixed(DOT + 4.0))).class(theme::Container::custom(move |t| {
        widget::container::Style {
            background: Some(fade(wx_now(t), dim).into()),
            border: cosmic::iced::Border { radius: (DOT / 2.0 + 2.0).into(), width: 2.0, color: t.cosmic().background(t.transparent).base.into() },
            ..Default::default()
        }
    }));
    let ring = widget::container(ring).padding(0);
    let dots = Row::new()
        .align_y(Alignment::Center)
        .height(Length::Fixed(DOT + 4.0))
        .push_maybe((dot > 0).then(|| gap(dot)))
        .push(ring)
        .push_maybe((dot < SCALE as u16).then(|| gap(SCALE as u16 - dot)));
    cosmic::iced::widget::stack([track.into(), dots.into()]).width(Length::Fill).into()
}

pub fn daily<'a, M: 'a>(s: &Snapshot, ctx: Ctx) -> Element<'a, M> {
    let days = &s.daily;
    let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
    let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).max(1.0);
    let today = local_date(s.current.dt, s.tz_offset);
    let ink = if ctx.dim { Ink::Muted } else { Ink::On };
    let mut col = Column::new();
    for d in days {
        let is_today = local_date(d.dt, s.tz_offset) == today;
        let icon = widget::icon(icons::condition(ctx.icons, &d.condition, None)).size(20).opacity(opacity(ctx.dim));
        let pop = format::pop(d.pop).map(|p| {
            let dim = ctx.dim;
            mono(p, Type::new(12.0, 17.0), Weight::Normal, None).class(cosmic::theme::Text::Custom(if dim { pop_dim } else { pop_ink }))
        });
        let (l, h) = (at(d.min, lo, span), at(d.max, lo, span));
        let dot = is_today.then(|| at(s.current.temp, lo, span).clamp(l, h.max(l)));
        let row = Row::new()
            .spacing(space_xs())
            .align_y(Alignment::Center)
            .height(Length::Fixed(ROW_H))
            .push(cell(widget::text::body(format::day(d.dt, s.tz_offset, is_today)).class(ink.text()), DAY_W, Horizontal::Left))
            .push(icon)
            .push(cell(pop.map_or_else(|| Element::from(widget::space()), Element::from), POP_W, Horizontal::Right))
            .push(cell(mono(format::temp(Some(d.min)), MONO, Weight::Normal, Some(Ink::Muted)), MONO.ch(4.0), Horizontal::Right))
            .push(range_bar(l, h, dot, ctx.dim))
            .push(cell(mono(format::temp(Some(d.max)), MONO, Weight::Normal, Some(ink)), MONO.ch(4.0), Horizontal::Right));
        col = col.push(widget::tooltip(row, widget::text::caption(summary(s, d)), widget::tooltip::Position::Top));
    }
    widget::container(col).padding([0, space_m()]).width(Length::Fill).into()
}

fn pop_ink(t: &cosmic::Theme) -> cosmic::iced::widget::text::Style {
    cosmic::iced::widget::text::Style { color: Some(wx_rain(t)), ..Default::default() }
}

fn pop_dim(t: &cosmic::Theme) -> cosmic::iced::widget::text::Style {
    cosmic::iced::widget::text::Style { color: Some(fade(wx_rain(t), true)), ..Default::default() }
}

/// "Fri, light rain, 46% chance of rain, 15° to 27°": the row's tooltip.
pub fn summary(s: &Snapshot, d: &crate::model::Day) -> String {
    let today = local_date(s.current.dt, s.tz_offset);
    crate::fl!(
        "a11y-day",
        day = format::day(d.dt, s.tz_offset, local_date(d.dt, s.tz_offset) == today),
        desc = d.condition.description.clone(),
        pop = ((d.pop * 100.0).round() as i64),
        low = format::temp(Some(d.min)),
        high = format::temp(Some(d.max))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_scale() {
        // One scale for the week: 12° to 29°.
        let (lo, span) = (12.0, 17.0);
        assert_eq!(at(12.0, lo, span), 0);
        assert_eq!(at(29.0, lo, span), 1000);
        assert_eq!(at(20.5, lo, span), 500);
        assert_eq!(at(40.0, lo, span), 1000);
    }

    #[test]
    fn row_summaries() {
        use crate::api::{onecall4, tests::fixture};
        let now = onecall4::current(&fixture("onecall4/current.json")).unwrap();
        let mut days = onecall4::days(&fixture("onecall4/timeline_1day.json")).unwrap();
        days.truncate(7);
        let s = Snapshot {
            tier: crate::model::Tier::Full,
            units: crate::config::Units::Metric,
            tz_offset: now.tz_offset,
            current: now.current,
            hourly: vec![],
            daily: days,
            alerts: vec![],
            fetched_at: chrono::Utc::now(),
        };
        let rows: Vec<String> = s.daily.iter().map(|d| summary(&s, d)).collect();
        assert_eq!(rows.len(), 7);
        assert!(rows[0].starts_with("Today, clear sky, 5% chance of rain, 15° to 27°"), "{}", rows[0]);
        assert!(rows[1].starts_with("Fri, "), "{}", rows[1]);
    }
}
