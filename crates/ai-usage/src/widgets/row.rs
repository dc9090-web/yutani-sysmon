//! A popup quota row: name and value, the meter with its pace tick, then
//! the reset and pace text.

use chrono::{DateTime, Utc};
use cosmic::Element;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};

use common::ink::Ink;
use common::panel::Type;
use common::ui::{mono, space_m, space_xxs, space_xxxs};

use crate::fl;
use crate::format::{self, Level, Live};
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
        Kind::Session => fl!("session"),
        Kind::Weekly => fl!("weekly"),
        Kind::Fable => fl!("fable"),
    }
}

fn description(kind: Kind) -> String {
    match kind {
        Kind::Session => fl!("session-desc"),
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

pub fn quota_row<'a, M: 'a>(l: &Live, ctx: &Ctx) -> Element<'a, M> {
    let level = l.level();
    let shown = l.shown(ctx.left);
    let pace = l.pace(ctx.now);
    let value_ink = match (ctx.stale, level) {
        (true, _) => Some(Ink::Muted),
        (false, Level::Normal) => None,
        (false, Level::Warning) => Some(Ink::Warning),
        (false, Level::Limit) => Some(Ink::Destructive),
    };

    let title =
        Column::new().width(Length::Fill).push(widget::text::heading(name(l.kind))).push(widget::text::caption(description(l.kind)).class(Ink::Muted.text()));
    let value = Row::new()
        .spacing(space_xxxs())
        .align_y(Alignment::End)
        .push(mono(format::pct(shown), Type::new(24.0, 32.0), Weight::Bold, value_ink))
        .push(widget::container(widget::text::caption(if ctx.left { fl!("left") } else { fl!("used") }).class(Ink::Muted.text())).padding([0, 0, 4, 0]));
    let top = Row::new().align_y(Alignment::Center).push(title).push(value);

    let tick = pace.map(|p| if ctx.left { 100.0 - p.expected } else { p.expected });
    // In the popup the bar shows the real amount even at the limit; the words say "Limit reached".
    let meter = Meter { fill: shown, tick, level, stale: ctx.stale, track: 8.0, tick_height: 14.0, solid_at_limit: false }.view(Length::Fill);

    let reset = reset_phrase(l, ctx);
    let foot: Element<'a, M> = if level == Level::Limit {
        let text = match &reset {
            Some(r) => fl!("limit-reached", reset = lower_first(r)),
            None => fl!("limit-reached", reset = String::new()).trim_end_matches([' ', '·']).to_owned(),
        };
        widget::text::caption(text).class(Ink::Destructive.text()).into()
    } else {
        let mut row = Row::new().align_y(Alignment::Center);
        let left =
            Row::new()
                .spacing(space_xxxs() + 2)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .push_maybe(reset.is_some().then(|| {
                    widget::icon(widget::icon::from_name("appointment-soon-symbolic").size(12).symbolic(true).handle()).size(12).class(Ink::Muted.svg())
                }))
                .push_maybe(reset.map(|r| widget::text::caption(r).class(Ink::Muted.text())));
        row = row.push(left);
        if let Some(p) = pace {
            let ink = if p.ahead() && level != Level::Normal { Ink::Warning } else { Ink::Muted };
            row = row.push(widget::text::caption(p.text()).class(ink.text()));
        }
        row.into()
    };

    widget::container(Column::new().spacing(space_xxs()).push(top).push(meter).push(foot)).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}

/// "Resets in 2h" → "resets in 2h", for the middle of a sentence.
pub fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}
