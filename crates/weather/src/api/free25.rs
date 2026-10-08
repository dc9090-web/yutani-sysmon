//! The free tier (SPEC §4.2): Current Weather 2.5 and the 5-day/3-hour
//! Forecast 2.5, with the daily rows aggregated by local date.

use serde::Deserialize;

use super::{Format, RawWeather, condition};
use crate::model::{Current, Day, Hour, local_date};

/// The hourly view: the first 9 three-hour points (24 h).
pub const HOURLY_POINTS: usize = 9;
/// Daily rows shown in free mode.
pub const DAYS: usize = 5;

#[derive(Deserialize)]
struct RawCurrent {
    dt: i64,
    timezone: i32,
    main: RawMain,
    #[serde(default)]
    wind: RawWind,
    #[serde(default)]
    sys: RawSys,
    #[serde(default)]
    weather: Vec<RawWeather>,
}

#[derive(Deserialize)]
struct RawMain {
    temp: f64,
    #[serde(default)]
    feels_like: f64,
    #[serde(default)]
    pressure: f64,
    #[serde(default)]
    humidity: f64,
}

#[derive(Deserialize, Default)]
struct RawWind {
    #[serde(default)]
    speed: f64,
    #[serde(default)]
    deg: f64,
    gust: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RawSys {
    sunrise: Option<i64>,
    sunset: Option<i64>,
}

#[derive(Deserialize)]
struct RawForecast {
    list: Vec<RawEntry>,
    city: RawCity,
}

#[derive(Deserialize)]
struct RawCity {
    timezone: i32,
}

#[derive(Deserialize)]
struct RawEntry {
    dt: i64,
    main: RawEntryMain,
    #[serde(default)]
    pop: f64,
    #[serde(default)]
    weather: Vec<RawWeather>,
}

#[derive(Deserialize)]
struct RawEntryMain {
    temp: f64,
    temp_min: f64,
    temp_max: f64,
}

/// One 3-hour step of the forecast.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub hour: Hour,
    pub temp_min: f64,
    pub temp_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub tz_offset: i32,
    pub entries: Vec<Entry>,
}

/// `/data/2.5/weather`: the conditions and the zone offset. No UV.
pub fn current(body: &[u8]) -> Result<(i32, Current), Format> {
    let c: RawCurrent = serde_json::from_slice(body)?;
    let current = Current {
        dt: c.dt,
        temp: c.main.temp,
        feels_like: c.main.feels_like,
        pressure: c.main.pressure,
        humidity: c.main.humidity,
        uvi: None,
        wind_speed: c.wind.speed,
        wind_gust: c.wind.gust,
        wind_deg: c.wind.deg,
        sunrise: c.sys.sunrise,
        sunset: c.sys.sunset,
        condition: condition(&c.weather),
    };
    Ok((c.timezone, current))
}

/// `/data/2.5/forecast`: 40 three-hour steps.
pub fn forecast(body: &[u8]) -> Result<Forecast, Format> {
    let f: RawForecast = serde_json::from_slice(body)?;
    let entries = f
        .list
        .into_iter()
        .map(|e| Entry {
            hour: Hour { dt: e.dt, temp: e.main.temp, pop: e.pop.clamp(0.0, 1.0), condition: condition(&e.weather) },
            temp_min: e.main.temp_min,
            temp_max: e.main.temp_max,
        })
        .collect();
    Ok(Forecast { tz_offset: f.city.timezone, entries })
}

impl Forecast {
    /// The first 9 points: 24 h in 3-hour steps.
    pub fn hourly(&self) -> Vec<Hour> {
        self.entries.iter().take(HOURLY_POINTS).map(|e| e.hour.clone()).collect()
    }

    /// Up to 5 days, grouped by the location's local date: the lowest
    /// `temp_min`, the highest `temp_max`, the highest `pop`, and the
    /// condition of the step nearest local 13:00.
    pub fn daily(&self) -> Vec<Day> {
        const MIDDAY: i64 = 13 * 3600;
        let tz = self.tz_offset;
        let mut days: Vec<(Day, i64)> = Vec::new();
        for e in &self.entries {
            let date = local_date(e.hour.dt, tz);
            // Distance from local 13:00 within the step's local day.
            let off_midday = ((e.hour.dt + i64::from(tz)).rem_euclid(86_400) - MIDDAY).abs();
            match days.last_mut() {
                Some((d, best)) if local_date(d.dt, tz) == date => {
                    d.min = d.min.min(e.temp_min);
                    d.max = d.max.max(e.temp_max);
                    d.pop = d.pop.max(e.hour.pop);
                    if off_midday < *best {
                        *best = off_midday;
                        d.dt = e.hour.dt;
                        d.condition = e.hour.condition.clone();
                    }
                }
                _ => {
                    if days.len() == DAYS {
                        break;
                    }
                    let day = Day { dt: e.hour.dt, min: e.temp_min, max: e.temp_max, pop: e.hour.pop, condition: e.hour.condition.clone() };
                    days.push((day, off_midday));
                }
            }
        }
        days.into_iter().map(|(d, _)| d).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::fixture;
    use crate::model::Condition;

    #[test]
    fn current_sydney() {
        let (tz, c) = current(&fixture("free25/weather.json")).unwrap();
        assert_eq!(tz, 39600);
        assert_eq!((c.dt, c.temp, c.feels_like, c.pressure, c.humidity), (1791417600, 23.4, 22.8, 1016.0, 62.0));
        assert_eq!((c.uvi, c.wind_speed, c.wind_gust, c.wind_deg), (None, 4.2, Some(7.1), 225.0));
        assert_eq!((c.sunrise, c.sunset), (Some(1791400320), Some(1791445620)));
        assert_eq!(c.condition.icon, "03d");
    }

    #[test]
    fn hourly_nine_steps() {
        let f = forecast(&fixture("free25/forecast.json")).unwrap();
        assert_eq!((f.tz_offset, f.entries.len()), (39600, 40));
        let h = f.hourly();
        assert_eq!(h.len(), 9);
        assert!(h.windows(2).all(|w| w[1].dt - w[0].dt == 3 * 3600));
        assert_eq!(h[8].dt - h[0].dt, 24 * 3600);
    }

    #[test]
    fn daily_by_local_date() {
        let f = forecast(&fixture("free25/forecast.json")).unwrap();
        let days = f.daily();
        assert_eq!(days.len(), 5);
        // Day boundaries follow Sydney (UTC+11), not UTC or this machine.
        let dates: Vec<String> = days.iter().map(|d| local_date(d.dt, 39600).to_string()).collect();
        assert_eq!(dates, ["2026-10-08", "2026-10-09", "2026-10-10", "2026-10-11", "2026-10-12"]);
        // Each day aggregates exactly its own local date's steps.
        for d in &days {
            let date = local_date(d.dt, 39600);
            let steps: Vec<&Entry> = f.entries.iter().filter(|e| local_date(e.hour.dt, 39600) == date).collect();
            let min = steps.iter().map(|e| e.temp_min).fold(f64::INFINITY, f64::min);
            let max = steps.iter().map(|e| e.temp_max).fold(f64::NEG_INFINITY, f64::max);
            let pop = steps.iter().map(|e| e.hour.pop).fold(0.0, f64::max);
            assert_eq!((d.min, d.max, d.pop), (min, max, pop), "{date}");
            // The icon comes from the step nearest local 13:00.
            let hour = (d.dt + 39600).rem_euclid(86_400) / 3600;
            assert!((12..=14).contains(&hour), "{date}: {hour}:00");
        }
    }

    #[test]
    fn midday_pick_and_ties() {
        let entry = |dt: i64, icon: &str| Entry {
            hour: Hour { dt, temp: 10.0, pop: 0.0, condition: Condition { id: Some(800), icon: icon.into(), description: String::new() } },
            temp_min: 9.0,
            temp_max: 11.0,
        };
        // UTC zone: 10:00, 13:00 and 16:00 on one day; 13:00 wins.
        let base = 1_791_331_200; // 2026-10-07T00:00:00Z
        let f = Forecast { tz_offset: 0, entries: vec![entry(base + 10 * 3600, "a"), entry(base + 13 * 3600, "b"), entry(base + 16 * 3600, "c")] };
        assert_eq!(f.daily()[0].condition.icon, "b");
        // 11:30 and 14:30 tie at 90 min: the first one stays.
        let f = Forecast { tz_offset: 0, entries: vec![entry(base + 41_400, "a"), entry(base + 52_200, "b")] };
        assert_eq!(f.daily()[0].condition.icon, "a");
    }
}
