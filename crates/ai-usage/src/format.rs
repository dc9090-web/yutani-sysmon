//! Reset times, pace and freshness (SPEC §6). Pure functions of the data
//! and a clock, so every example in the spec is a unit test.

use chrono::{DateTime, Datelike, TimeDelta, TimeZone, Timelike, Utc};

use crate::fl;
use crate::model::{Kind, Window};

/// Level of a window, always from the *used* amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    /// 80–99% used.
    Warning,
    /// 100% used.
    Limit,
}

impl Level {
    pub fn of(used: f32) -> Self {
        if used >= 100.0 {
            Self::Limit
        } else if used >= 80.0 {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

/// A window as it should look at `now`: once `resets_at` has passed it
/// counts as 0% used and "Resetting now" until the next fetch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub kind: Kind,
    pub used: f32,
    pub resets_at: Option<DateTime<Utc>>,
    pub resetting: bool,
}

impl Live {
    pub fn of(w: &Window, now: DateTime<Utc>) -> Self {
        let resetting = w.resets_at.is_some_and(|t| t <= now);
        Self { kind: w.kind, used: if resetting { 0.0 } else { w.used }, resets_at: w.resets_at, resetting }
    }

    pub fn level(&self) -> Level {
        Level::of(self.used)
    }

    /// Used, or 100 − used in Left mode.
    pub fn shown(&self, left: bool) -> f32 {
        if left { 100.0 - self.used } else { self.used }
    }

    pub fn pace(&self, now: DateTime<Utc>) -> Option<Pace> {
        if self.resetting {
            return None;
        }
        pace(self.kind.length(), self.used, self.resets_at?, now)
    }
}

/// Where spending evenly would be, and how far ahead or under it the window is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    /// 0–100.
    pub expected: f32,
    /// used − expected.
    pub delta: f32,
}

impl Pace {
    /// "On pace", "29% ahead of pace", "14% under pace".
    pub fn text(&self) -> String {
        let n = self.delta.abs().round() as i64;
        if self.delta.abs() < 3.0 {
            fl!("on-pace")
        } else if self.delta > 0.0 {
            fl!("ahead-of-pace", pct = n)
        } else {
            fl!("under-pace", pct = n)
        }
    }

    pub fn ahead(&self) -> bool {
        self.delta >= 3.0
    }
}

pub fn pace(length: TimeDelta, used: f32, resets_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<Pace> {
    let len = length.num_milliseconds() as f64;
    let elapsed = len - (resets_at - now).num_milliseconds() as f64;
    let expected = (elapsed / len * 100.0).clamp(0.0, 100.0) as f32;
    Some(Pace { expected, delta: used - expected })
}

/// "2h 13m", "3d 4h", "13m", or `None` once it has passed. `compact` drops
/// the space: "2h13m".
pub fn duration(until: TimeDelta, compact: bool) -> Option<String> {
    if until <= TimeDelta::zero() {
        return None;
    }
    let m = until.num_minutes();
    let (d, h, mm) = (m / 1440, (m % 1440) / 60, m % 60);
    let sp = if compact { "" } else { " " };
    Some(if d > 0 {
        format!("{d}d{sp}{h}h")
    } else if h > 0 {
        format!("{h}h{sp}{mm:02}m")
    } else {
        format!("{mm}m")
    })
}

/// The panel's Reset chunk: `2h13m`, `3d4h`, `13m`, `now`.
pub fn compact_reset(resets_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    match resets_at {
        Some(t) => duration(t - now, true).unwrap_or_else(|| fl!("label-reset-now")),
        None => fl!("label-reset-none"),
    }
}

/// "15:40" or "3:40 PM".
fn clock<T: Timelike>(t: &T, military: bool) -> String {
    if military {
        format!("{:02}:{:02}", t.hour(), t.minute())
    } else {
        let (pm, h) = t.hour12();
        format!("{h}:{:02} {}", t.minute(), if pm { "PM" } else { "AM" })
    }
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The reset phrase: "Resets in 2h 13m", "Resets 15:40", "Resets tomorrow
/// 09:00", "Resets Sat 09:00", "Resets 12 Oct 09:00", "Resetting now".
pub fn reset_text<Z: TimeZone>(resets_at: DateTime<Utc>, now: DateTime<Utc>, absolute: bool, military: bool, tz: &Z) -> String {
    let Some(rel) = duration(resets_at - now, false) else { return fl!("resetting-now") };
    if !absolute {
        return fl!("resets-in", time = rel);
    }
    let (r, n) = (resets_at.with_timezone(tz), now.with_timezone(tz));
    let t = clock(&r, military);
    match (r.date_naive() - n.date_naive()).num_days() {
        0 => fl!("resets-at", time = t),
        1 => fl!("resets-tomorrow-at", time = t),
        2..=6 => fl!("resets-at", time = format!("{} {t}", WEEKDAYS[r.weekday().num_days_from_monday() as usize])),
        _ => fl!("resets-at", time = format!("{} {} {t}", r.day(), MONTHS[r.month0() as usize])),
    }
}

/// "just now" is `None`; otherwise "2m" or "3h".
pub fn ago(since: TimeDelta) -> Option<String> {
    let m = (since.num_seconds() as f64 / 60.0).round() as i64;
    match m {
        ..1 => None,
        1..60 => Some(format!("{m}m")),
        _ => Some(format!("{}h", m / 60)),
    }
}

/// "Updated just now", "Updated 2m ago", "Updated 3h ago".
pub fn freshness(fetched_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match ago(now - fetched_at) {
        None => fl!("updated-just-now"),
        Some(t) => fl!("updated-ago", time = t),
    }
}

/// Time left of a pause, rounded up: "4m", "1h 2m"…
pub fn countdown(until: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let left = until - now;
    let up = TimeDelta::minutes((left.num_seconds() + 59).div_euclid(60).max(1));
    duration(up, false).unwrap_or_else(|| "1m".to_owned())
}

/// A panel value in a 4-character field: "  4%", " 42%", "100%"; `MAX` at
/// the limit.
pub fn pct_cell(value: f32, limit: bool) -> String {
    if limit {
        return " MAX".to_owned();
    }
    format!("{:>4}", format!("{}%", value.clamp(0.0, 100.0).round() as i64))
}

/// The popup's big value: "42%".
pub fn pct(value: f32) -> String {
    format!("{}%", value.clamp(0.0, 100.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-08T10:00:00Z").unwrap().with_timezone(&Utc)
    }
    fn mins(m: i64) -> TimeDelta {
        TimeDelta::minutes(m)
    }

    #[test]
    fn relative() {
        let r = |d: TimeDelta| reset_text(now() + d, now(), false, true, &Utc);
        assert_eq!(r(mins(2 * 60 + 13)), "Resets in 2h 13m");
        assert_eq!(r(mins((3 * 24 + 4) * 60 + 30)), "Resets in 3d 4h");
        assert_eq!(r(mins(13)), "Resets in 13m");
        assert_eq!(r(mins(65)), "Resets in 1h 05m");
        assert_eq!(r(TimeDelta::seconds(30)), "Resets in 0m");
        assert_eq!(r(TimeDelta::zero()), "Resetting now");
        assert_eq!(r(mins(-5)), "Resetting now");
    }

    #[test]
    fn compact() {
        let c = |d: TimeDelta| compact_reset(Some(now() + d), now());
        assert_eq!(c(mins(2 * 60 + 13)), "2h13m");
        assert_eq!(c(mins((3 * 24 + 4) * 60)), "3d4h");
        assert_eq!(c(mins(13)), "13m");
        assert_eq!(c(mins(-1)), "now");
        assert_eq!(compact_reset(None, now()), "—");
    }

    #[test]
    fn absolute() {
        // Local time UTC+11 (Sydney in October): now is Thu 8 Oct 21:00.
        let tz = FixedOffset::east_opt(11 * 3600).unwrap();
        let a = |rfc: &str, military: bool| {
            let t = DateTime::parse_from_rfc3339(rfc).unwrap().with_timezone(&Utc);
            reset_text(t, now(), true, military, &tz)
        };
        assert_eq!(a("2026-10-08T23:40:00+11:00", true), "Resets 23:40");
        assert_eq!(a("2026-10-09T09:00:00+11:00", true), "Resets tomorrow 09:00");
        assert_eq!(a("2026-10-10T09:00:00+11:00", true), "Resets Sat 09:00");
        assert_eq!(a("2026-10-14T09:00:00+11:00", true), "Resets Wed 09:00");
        assert_eq!(a("2026-10-15T09:00:00+11:00", true), "Resets 15 Oct 09:00");
        assert_eq!(a("2026-10-08T15:40:00+11:00", false), "Resetting now");
        assert_eq!(a("2026-10-08T23:40:00+11:00", false), "Resets 11:40 PM");
        assert_eq!(a("2026-10-09T00:05:00+11:00", false), "Resets tomorrow 12:05 AM");
        assert_eq!(a("2026-10-12T09:00:00+11:00", false), "Resets Mon 9:00 AM");
    }

    #[test]
    fn pace_examples() {
        let five = Kind::Session.length();
        // 2h 13m before reset: 2h 47m of 5h elapsed = 55.7% expected.
        let p = pace(five, 42.0, now() + mins(133), now()).unwrap();
        assert!((p.expected - 55.666).abs() < 0.01);
        assert_eq!(p.text(), "14% under pace");
        let week = Kind::Weekly.length();
        // 3d 4h left of 7d: 3d 20h elapsed = 54.8%.
        let p = pace(week, 84.0, now() + mins((3 * 24 + 4) * 60), now()).unwrap();
        assert_eq!(p.text(), "29% ahead of pace");
        assert!(p.ahead());
        let p = pace(week, 56.0, now() + mins((3 * 24 + 4) * 60), now()).unwrap();
        assert_eq!(p.text(), "On pace");
        // Clamped at both ends.
        assert_eq!(pace(five, 0.0, now() + mins(600), now()).unwrap().expected, 0.0);
        assert_eq!(pace(five, 0.0, now() - mins(1), now()).unwrap().expected, 100.0);
    }

    #[test]
    fn no_pace_without_reset() {
        let w = Window { kind: Kind::Session, used: 5.0, resets_at: None };
        assert_eq!(Live::of(&w, now()).pace(now()), None);
    }

    #[test]
    fn elapsed_window() {
        let w = Window { kind: Kind::Session, used: 100.0, resets_at: Some(now() - mins(1)) };
        let l = Live::of(&w, now());
        assert!(l.resetting);
        assert_eq!((l.used, l.level(), l.pace(now())), (0.0, Level::Normal, None));
        let w = Window { resets_at: Some(now() + mins(1)), ..w };
        assert_eq!(Live::of(&w, now()).level(), Level::Limit);
    }

    #[test]
    fn levels_and_left() {
        assert_eq!([0.0, 79.9, 80.0, 99.9, 100.0].map(Level::of), [Level::Normal, Level::Normal, Level::Warning, Level::Warning, Level::Limit]);
        let w = Live { kind: Kind::Weekly, used: 61.0, resets_at: None, resetting: false };
        assert_eq!((w.shown(false), w.shown(true)), (61.0, 39.0));
    }

    #[test]
    fn freshness_text() {
        assert_eq!(freshness(now(), now()), "Updated just now");
        assert_eq!(freshness(now() - TimeDelta::seconds(29), now()), "Updated just now");
        assert_eq!(freshness(now() - mins(2), now()), "Updated 2m ago");
        assert_eq!(freshness(now() - mins(3 * 60 + 10), now()), "Updated 3h ago");
        assert_eq!(ago(mins(25)).as_deref(), Some("25m"));
    }

    #[test]
    fn countdowns() {
        assert_eq!(countdown(now() + TimeDelta::seconds(210), now()), "4m");
        assert_eq!(countdown(now() + TimeDelta::seconds(1), now()), "1m");
        assert_eq!(countdown(now() - TimeDelta::seconds(5), now()), "1m");
    }

    #[test]
    fn cells() {
        assert_eq!(pct_cell(4.4, false), "  4%");
        assert_eq!(pct_cell(42.0, false), " 42%");
        assert_eq!(pct_cell(100.0, false), "100%");
        assert_eq!(pct_cell(100.0, true), " MAX");
        assert!([pct_cell(0.0, false), pct_cell(100.0, false), pct_cell(100.0, true)].iter().all(|s| s.chars().count() == 4));
        assert_eq!(pct(61.4), "61%");
    }
}
