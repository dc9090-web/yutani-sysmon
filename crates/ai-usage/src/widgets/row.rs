//! Popup rows: a quota row (name and value, the meter with its pace tick,
//! then the reset and pace text) and the API credits row in the same shape.

use chrono::{DateTime, Utc};
use cosmic::Element;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};

use common::ink::Ink;
use common::panel::Type;
use common::ui::{mono, space_m, space_xxs, space_xxxs};

use crate::credits;
use crate::fl;
use crate::format::{self, Level, Live, Pace};
use crate::model::Kind;
use crate::widgets::meter::Meter;

/// What every row shares.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub now: DateTime<Utc>,
    pub left: bool,
    pub absolute: bool,
    pub military: bool,
    pub stale: bool,
}

pub fn name(kind: Kind) -> String {
    match kind {
        Kind::Weekly => fl!("weekly"),
        Kind::Fable => fl!("fable"),
    }
}

fn description(kind: Kind) -> String {
    match kind {
        Kind::Weekly => fl!("weekly-desc"),
        Kind::Fable => fl!("fable-desc"),
    }
}

/// "Resets in 2h 13m" (or the clock form), or "Resetting now".
pub fn reset_phrase(l: &Live, ctx: &Ctx) -> Option<String> {
    if l.resetting {
        return Some(fl!("resetting-now"));
    }
    l.resets_at.map(|t| format::reset_text(t, ctx.now, ctx.absolute, ctx.military, &chrono::Local))
}

/// One row's pieces.
struct Parts<'a, M> {
    title: String,
    description: String,
    /// The big mono value and the caption beside it ("used", "of $200").
    value: String,
    caption: String,
    value_ink: Option<Ink>,
    meter: Meter,
    foot: Element<'a, M>,
}

fn value_ink(stale: bool, level: Level) -> Option<Ink> {
    match (stale, level) {
        (true, _) => Some(Ink::Muted),
        (false, Level::Normal) => None,
        (false, Level::Warning) => Some(Ink::Warning),
        (false, Level::Limit) => Some(Ink::Destructive),
    }
}

fn layout<'a, M: 'a>(p: Parts<'a, M>) -> Element<'a, M> {
    let title = Column::new().width(Length::Fill).push(widget::text::heading(p.title)).push(widget::text::caption(p.description).class(Ink::Muted.text()));
    let value = Row::new()
        .spacing(space_xxxs())
        .align_y(Alignment::End)
        .push(mono(p.value, Type::new(24.0, 32.0), Weight::Bold, p.value_ink))
        .push(widget::container(widget::text::caption(p.caption).class(Ink::Muted.text())).padding([0, 0, 4, 0]));
    let top = Row::new().align_y(Alignment::Center).push(title).push(value);
    widget::container(Column::new().spacing(space_xxs()).push(top).push(p.meter.view(Length::Fill)).push(p.foot))
        .padding([space_xxs(), space_m()])
        .width(Length::Fill)
        .into()
}

/// The usual foot: a clock icon and `when` on the left, the pace on the right.
fn foot<'a, M: 'a>(when: Option<String>, pace: Option<Pace>, level: Level) -> Element<'a, M> {
    let mut row = Row::new().align_y(Alignment::Center);
    let left = Row::new()
        .spacing(space_xxxs() + 2)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .push_maybe(
            when.is_some()
                .then(|| widget::icon(widget::icon::from_name("appointment-soon-symbolic").size(12).symbolic(true).handle()).size(12).class(Ink::Muted.svg())),
        )
        .push_maybe(when.map(|w| widget::text::caption(w).class(Ink::Muted.text())));
    row = row.push(left);
    if let Some(p) = pace {
        let ink = if p.ahead() && level != Level::Normal { Ink::Warning } else { Ink::Muted };
        row = row.push(widget::text::caption(p.text()).class(ink.text()));
    }
    row.into()
}

/// A foot that's just a line of text.
fn note<'a, M: 'a>(text: String, ink: Ink) -> Element<'a, M> {
    widget::text::caption(text).class(ink.text()).into()
}

/// The pace tick's position, flipped in Left mode.
fn tick(pace: Option<Pace>, left: bool) -> Option<f32> {
    pace.map(|p| if left { 100.0 - p.expected } else { p.expected })
}

pub fn quota_row<'a, M: 'a>(l: &Live, ctx: &Ctx) -> Element<'a, M> {
    let level = l.level();
    let shown = l.shown(ctx.left);
    let pace = l.pace(ctx.now);
    // In the popup the bar shows the real amount even at the limit; the words say "Limit reached".
    let meter = Meter { fill: shown, tick: tick(pace, ctx.left), level, stale: ctx.stale, track: 8.0, tick_height: 14.0, solid_at_limit: false };
    let reset = reset_phrase(l, ctx);
    let foot = if level == Level::Limit {
        let text = match &reset {
            Some(r) => fl!("limit-reached", reset = lower_first(r)),
            None => fl!("limit-reached", reset = String::new()).trim_end_matches([' ', '·']).to_owned(),
        };
        note(text, Ink::Destructive)
    } else {
        foot(reset, pace, level)
    };
    layout(Parts {
        title: name(l.kind),
        description: description(l.kind),
        value: format::pct(shown),
        caption: if ctx.left { fl!("left") } else { fl!("used") },
        value_ink: value_ink(ctx.stale, level),
        meter,
        foot,
    })
}

/// The API credits row: this cycle's spend against the monthly credit.
/// `problem` replaces the foot (a refused key, an unreadable report).
pub fn credits_row<'a, M: 'a>(c: &credits::Live, ctx: &Ctx, stale: bool, problem: Option<(String, Ink)>) -> Element<'a, M> {
    let level = c.level();
    let shown = c.shown(ctx.left);
    let pace = c.pace(ctx.now);
    let meter = Meter { fill: shown, tick: tick(pace, ctx.left), level, stale, track: 8.0, tick_height: 14.0, solid_at_limit: false };
    let renew = format::renew_text(c.cycle.end, ctx.now, ctx.absolute, &chrono::Local);
    let foot = match problem {
        Some((text, ink)) => note(text, ink),
        None if level == Level::Limit => note(fl!("credits-used-up", renew = lower_first(&renew)), Ink::Destructive),
        None => foot(Some(renew), pace, level),
    };
    let limit = format::dollars_short(c.limit_cents);
    let (value, caption) =
        if ctx.left { (format::dollars(c.left_cents()), fl!("left")) } else { (format::dollars(c.spent_cents), fl!("credits-of", limit = limit.clone())) };
    layout(Parts { title: fl!("credits"), description: fl!("credits-desc", limit = limit), value, caption, value_ink: value_ink(stale, level), meter, foot })
}

/// "Resets in 2h" → "resets in 2h", for the middle of a sentence.
pub fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}
