//! Text for values (SPEC §7). Everything local uses the location's
//! `tz_offset`, never the machine's time zone.

use chrono::{DateTime, Datelike, TimeDelta, Timelike, Utc, Weekday};

use crate::config::Units;
use crate::fl;

/// `round(t)°`, no unit letter; `—` when unknown.
pub fn temp(t: Option<f64>) -> String {
    match t.filter(|t| t.is_finite()) {
        Some(t) => format!("{}°", round(t)),
        None => "—".to_owned(),
    }
}

/// The panel's fixed field: `temp` right-aligned in 4 characters.
pub fn panel_temp(t: Option<f64>) -> String {
    format!("{:>4}", temp(t))
}

/// Rounds half away from zero, without `-0`.
fn round(t: f64) -> i64 {
    let r = t.round() as i64;
    if r == 0 { 0 } else { r }
}

/// The API's wind speed (m/s metric, mph imperial) as `15 km/h` / `9 mph`.
pub fn wind(speed: f64, units: Units) -> String {
    match units {
        Units::Metric => format!("{} km/h", round(speed * 3.6)),
        Units::Imperial => format!("{} mph", round(speed)),
    }
}

/// The API's wind speed in m/s, for the windy-icon threshold.
pub fn wind_ms(speed: f64, units: Units) -> f64 {
    match units {
        Units::Metric => speed,
        Units::Imperial => speed * 0.44704,
    }
}

/// 16-point compass direction.
pub fn compass(deg: f64) -> &'static str {
    const POINTS: [&str; 16] = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
    let d = deg.rem_euclid(360.0);
    POINTS[(d / 22.5).round() as usize % 16]
}

/// `1016 hPa` / `30.00 inHg`.
pub fn pressure(hpa: f64, units: Units) -> String {
    match units {
        Units::Metric => format!("{} hPa", round(hpa)),
        Units::Imperial => format!("{:.2} inHg", hpa * 0.02953),
    }
}

/// `6 High`; `—` on the free tier.
pub fn uv(uvi: Option<f64>) -> String {
    let Some(u) = uvi.filter(|u| u.is_finite()) else { return "—".to_owned() };
    let n = round(u.max(0.0));
    let word = match n {
        ..=2 => fl!("uv-low"),
        3..=5 => fl!("uv-moderate"),
        6..=7 => fl!("uv-high"),
        8..=10 => fl!("uv-very-high"),
        _ => fl!("uv-extreme"),
    };
    format!("{n} {word}")
}

/// `46%` when the chance is at least 20 %, else nothing.
pub fn pop(p: f64) -> Option<String> {
    (p >= 0.2).then(|| format!("{}%", round(p * 100.0)))
}

/// Local wall-clock time at `ts` in a zone `tz` seconds east of UTC.
fn local(ts: i64, tz: i32) -> DateTime<Utc> {
    DateTime::from_timestamp(ts + i64::from(tz), 0).unwrap_or_default()
}

/// `HH:MM`, or `h:MM AM` on a 12-hour clock; `—` when unknown.
pub fn clock(ts: Option<i64>, tz: i32, military: bool) -> String {
    let Some(ts) = ts else { return "—".to_owned() };
    let t = local(ts, tz);
    if military {
        format!("{:02}:{:02}", t.hour(), t.minute())
    } else {
        let (pm, h) = t.hour12();
        format!("{h}:{:02} {}", t.minute(), if pm { "PM" } else { "AM" })
    }
}

/// The hourly canvas's hour label: `HH`, or `9p` / `12a` on a 12-hour clock.
pub fn hour(ts: i64, tz: i32, military: bool) -> String {
    let t = local(ts, tz);
    if military {
        format!("{:02}", t.hour())
    } else {
        let (pm, h) = t.hour12();
        format!("{h}{}", if pm { "p" } else { "a" })
    }
}

/// `Today`, else the short weekday at the location.
pub fn day(ts: i64, tz: i32, today: bool) -> String {
    if today {
        return fl!("today");
    }
    match local(ts, tz).weekday() {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
    .to_owned()
}

/// The first letter upper-cased (descriptions arrive lower-case).
pub fn sentence(s: &str) -> String {
    let mut c = s.trim().chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// An age as `4m`, `2h` or `1d`; `None` under a minute.
pub fn ago(age: TimeDelta) -> Option<String> {
    let m = age.num_minutes();
    match m {
        ..1 => None,
        1..60 => Some(format!("{m}m")),
        60..1440 => Some(format!("{}h", m / 60)),
        _ => Some(format!("{}d", m / 1440)),
    }
}

/// Time left as `4 m` style for the rate-limit header: whole minutes, at least 1.
pub fn countdown(until: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (until - now).num_seconds().max(0);
    format!("{}m", ((secs + 59) / 60).max(1))
}

/// Shortens `s` to `max` characters with an ellipsis.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYDNEY: i32 = 39600;

    #[test]
    fn temperatures() {
        assert_eq!(temp(Some(23.4)), "23°");
        assert_eq!(temp(Some(-0.4)), "0°");
        assert_eq!(temp(Some(-12.5)), "-13°");
        assert_eq!(temp(None), "—");
        assert_eq!(temp(Some(f64::NAN)), "—");
        // The fixed 4-character panel field.
        assert_eq!(panel_temp(Some(9.0)), "  9°");
        assert_eq!(panel_temp(Some(23.0)), " 23°");
        assert_eq!(panel_temp(Some(-12.0)), "-12°");
        assert_eq!(panel_temp(Some(40.0)).chars().count(), 4);
    }

    #[test]
    fn wind_and_pressure() {
        assert_eq!(wind(4.2, Units::Metric), "15 km/h");
        assert_eq!(wind(9.4, Units::Imperial), "9 mph");
        assert!((wind_ms(25.0, Units::Imperial) - 11.176).abs() < 1e-9);
        assert_eq!(wind_ms(4.2, Units::Metric), 4.2);
        assert_eq!(pressure(1016.0, Units::Metric), "1016 hPa");
        assert_eq!(pressure(1016.0, Units::Imperial), "30.00 inHg");
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(225.0), "SW");
        assert_eq!(compass(11.2), "N");
        assert_eq!(compass(11.3), "NNE");
        assert_eq!(compass(348.75), "N");
        assert_eq!(compass(-90.0), "W");
        assert_eq!(compass(720.0 + 90.0), "E");
    }

    #[test]
    fn uv_words() {
        assert_eq!(uv(Some(6.4)), "6 High");
        assert_eq!(uv(Some(2.4)), "2 Low");
        assert_eq!(uv(Some(2.5)), "3 Moderate");
        assert_eq!(uv(Some(7.0)), "7 High");
        assert_eq!(uv(Some(10.0)), "10 Very high");
        assert_eq!(uv(Some(11.2)), "11 Extreme");
        assert_eq!(uv(None), "—");
    }

    #[test]
    fn rain_chance() {
        assert_eq!(pop(0.46), Some("46%".into()));
        assert_eq!(pop(0.2), Some("20%".into()));
        assert_eq!(pop(0.19), None);
        assert_eq!(pop(1.0), Some("100%".into()));
    }

    #[test]
    fn location_local_times() {
        // 2026-10-08T00:00:00Z is 11:00 in Sydney, whatever this machine's zone.
        let now = 1_791_417_600;
        assert_eq!(clock(Some(now), SYDNEY, true), "11:00");
        assert_eq!(clock(Some(1_791_400_320), SYDNEY, true), "06:12");
        assert_eq!(clock(Some(1_791_445_620), SYDNEY, true), "18:47");
        assert_eq!(clock(Some(1_791_445_620), SYDNEY, false), "6:47 PM");
        assert_eq!(clock(Some(now - 11 * 3600), SYDNEY, false), "12:00 AM");
        assert_eq!(clock(None, SYDNEY, true), "—");
        assert_eq!(hour(now + 7 * 3600, SYDNEY, true), "18");
        assert_eq!(hour(now + 10 * 3600, SYDNEY, false), "9p");
        assert_eq!(hour(now + 13 * 3600, SYDNEY, false), "12a");
        assert_eq!(hour(now + 13 * 3600, SYDNEY, true), "00");
        // Thursday in Sydney, still Wednesday at UTC−5.
        assert_eq!(day(now, SYDNEY, false), "Thu");
        assert_eq!(day(now, -5 * 3600, false), "Wed");
        assert_eq!(day(now, SYDNEY, true), "Today");
    }

    #[test]
    fn text_helpers() {
        assert_eq!(sentence("scattered clouds"), "Scattered clouds");
        assert_eq!(sentence("ébène"), "Ébène");
        assert_eq!(sentence(""), "");
        assert_eq!(ellipsize("Sydney", 10), "Sydney");
        assert_eq!(ellipsize("Thiruvananthapuram", 10), "Thiruvana…");
        assert_eq!(ago(TimeDelta::seconds(59)), None);
        assert_eq!(ago(TimeDelta::minutes(4)), Some("4m".into()));
        assert_eq!(ago(TimeDelta::minutes(130)), Some("2h".into()));
        assert_eq!(ago(TimeDelta::hours(30)), Some("1d".into()));
        let t = DateTime::from_timestamp(1_791_417_600, 0).unwrap();
        assert_eq!(countdown(t + TimeDelta::seconds(61), t), "2m");
        assert_eq!(countdown(t, t), "1m");
    }
}
