//! One location's weather, as the parsers build it (SPEC §11). It's also the
//! cache format, so it holds weather data only, never the key.
//!
//! Times are Unix seconds; `tz_offset` turns them into the location's local
//! time (SPEC §4.1: never the machine's time zone).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Units;

/// Which OpenWeather plan the key has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// One Call 4.0.
    Full,
    /// Current Weather 2.5 + 5-day/3-hour Forecast 2.5.
    Free,
}

/// `weather[0]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub id: Option<u16>,
    /// `NNd` / `NNn`.
    pub icon: String,
    /// Already localised by the API's `lang`.
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Current {
    pub dt: i64,
    /// °C or °F, as requested.
    pub temp: f64,
    pub feels_like: f64,
    /// hPa.
    pub pressure: f64,
    /// %.
    pub humidity: f64,
    /// `None` on the free tier.
    pub uvi: Option<f64>,
    /// m/s (metric) or mph (imperial), as the API returns it.
    pub wind_speed: f64,
    pub wind_gust: Option<f64>,
    pub wind_deg: f64,
    /// Absent during polar day and night.
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    pub condition: Condition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Hour {
    pub dt: i64,
    pub temp: f64,
    /// 0–1.
    pub pop: f64,
    pub condition: Condition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    pub dt: i64,
    pub min: f64,
    pub max: f64,
    pub pop: f64,
    pub condition: Condition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub sender: String,
    pub event: String,
    pub start: i64,
    pub end: i64,
    /// `(language, text)` pairs, in the API's order.
    pub descriptions: Vec<(String, String)>,
}

impl Alert {
    /// The text in `lang`, else the first one.
    pub fn description(&self, lang: &str) -> Option<&str> {
        self.descriptions.iter().find(|(l, _)| l.eq_ignore_ascii_case(lang)).or(self.descriptions.first()).map(|(_, d)| d.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub tier: Tier,
    pub units: Units,
    /// Seconds east of UTC.
    pub tz_offset: i32,
    pub current: Current,
    /// 24 points (full) or 9 points (free).
    pub hourly: Vec<Hour>,
    /// 7 days (full) or 5 days (free); the first is today.
    pub daily: Vec<Day>,
    /// Most recent `start` first.
    pub alerts: Vec<Alert>,
    pub fetched_at: DateTime<Utc>,
}

impl Snapshot {
    /// Widens today's min/max to cover the current temperature and today's
    /// hourly points, so the current temperature always falls within
    /// today's range (SPEC §4.1).
    pub fn widen_today(&mut self) {
        let tz = self.tz_offset;
        let today = local_date(self.current.dt, tz);
        let Some(day) = self.daily.first_mut().filter(|d| local_date(d.dt, tz) == today) else { return };
        let temps = self.hourly.iter().filter(|h| local_date(h.dt, tz) == today).map(|h| h.temp).chain([self.current.temp]);
        for t in temps {
            day.min = day.min.min(t);
            day.max = day.max.max(t);
        }
    }
}

/// The local date at `ts` in a zone `tz_offset` seconds east of UTC.
pub fn local_date(ts: i64, tz_offset: i32) -> NaiveDate {
    DateTime::from_timestamp(ts + i64::from(tz_offset), 0).unwrap_or_default().date_naive()
}
