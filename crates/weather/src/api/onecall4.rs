//! One Call 4.0 (SPEC §4.1): `current`, `timeline/1h`, `timeline/1day` and
//! `alert/{id}`.

use serde::Deserialize;

use super::{Format, RawWeather, condition};
use crate::model::{Alert, Current, Day, Hour};

/// The `current` call: conditions, the zone offset and the active alert IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentPart {
    pub tz_offset: i32,
    pub current: Current,
    pub alert_ids: Vec<String>,
}

/// One page of `timeline/1h`.
#[derive(Debug, Clone, PartialEq)]
pub struct HourPage {
    pub hours: Vec<Hour>,
    /// The next page's URL, as the API gives it.
    pub next: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    timezone_offset: i32,
    data: Vec<T>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Deserialize)]
struct RawCurrent {
    dt: i64,
    temp: f64,
    feels_like: f64,
    pressure: f64,
    humidity: f64,
    uvi: Option<f64>,
    wind_speed: f64,
    wind_gust: Option<f64>,
    #[serde(default)]
    wind_deg: f64,
    sunrise: Option<i64>,
    sunset: Option<i64>,
    #[serde(default)]
    weather: Vec<RawWeather>,
    #[serde(default)]
    alerts: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawHour {
    dt: i64,
    temp: f64,
    #[serde(default)]
    pop: f64,
    #[serde(default)]
    weather: Vec<RawWeather>,
}

#[derive(Deserialize)]
struct RawDay {
    dt: i64,
    temp: RawDayTemp,
    #[serde(default)]
    pop: f64,
    #[serde(default)]
    weather: Vec<RawWeather>,
}

#[derive(Deserialize)]
struct RawDayTemp {
    min: f64,
    max: f64,
}

#[derive(Deserialize)]
struct RawAlert {
    id: String,
    #[serde(default)]
    sender_name: String,
    #[serde(default)]
    event: String,
    start: i64,
    end: i64,
    #[serde(default)]
    description: Descriptions,
}

/// `description`: documented as `[{ language, description }]`; a plain
/// string is accepted too.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Descriptions {
    List(Vec<RawDescription>),
    Text(String),
    #[default]
    None,
}

#[derive(Deserialize)]
struct RawDescription {
    #[serde(default)]
    language: String,
    #[serde(default)]
    description: String,
}

pub fn current(body: &[u8]) -> Result<CurrentPart, Format> {
    let env: Envelope<RawCurrent> = serde_json::from_slice(body)?;
    let c = env.data.into_iter().next().ok_or(Format::new("current: empty data"))?;
    // Alert IDs are documented as strings; take `id` from objects as well.
    let alert_ids = c.alerts.iter().filter_map(|a| a.as_str().or_else(|| a.get("id").and_then(serde_json::Value::as_str))).map(str::to_owned).collect();
    Ok(CurrentPart {
        tz_offset: env.timezone_offset,
        current: Current {
            dt: c.dt,
            temp: c.temp,
            feels_like: c.feels_like,
            pressure: c.pressure,
            humidity: c.humidity,
            uvi: c.uvi,
            wind_speed: c.wind_speed,
            wind_gust: c.wind_gust,
            wind_deg: c.wind_deg,
            sunrise: c.sunrise,
            sunset: c.sunset,
            condition: condition(&c.weather),
        },
        alert_ids,
    })
}

pub fn hours(body: &[u8]) -> Result<HourPage, Format> {
    let env: Envelope<RawHour> = serde_json::from_slice(body)?;
    let hours = env.data.into_iter().map(|h| Hour { dt: h.dt, temp: h.temp, pop: h.pop.clamp(0.0, 1.0), condition: condition(&h.weather) }).collect();
    Ok(HourPage { hours, next: env.next })
}

pub fn days(body: &[u8]) -> Result<Vec<Day>, Format> {
    let env: Envelope<RawDay> = serde_json::from_slice(body)?;
    Ok(env.data.into_iter().map(|d| Day { dt: d.dt, min: d.temp.min, max: d.temp.max, pop: d.pop.clamp(0.0, 1.0), condition: condition(&d.weather) }).collect())
}

pub fn alert(body: &[u8]) -> Result<Alert, Format> {
    let a: RawAlert = serde_json::from_slice(body)?;
    let descriptions = match a.description {
        Descriptions::List(l) => l.into_iter().map(|d| (d.language, d.description)).collect(),
        Descriptions::Text(t) => vec![(String::new(), t)],
        Descriptions::None => Vec::new(),
    };
    Ok(Alert { id: a.id, sender: a.sender_name, event: a.event, start: a.start, end: a.end, descriptions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::fixture;

    #[test]
    fn current_sydney() {
        let p = current(&fixture("onecall4/current.json")).unwrap();
        assert_eq!(p.tz_offset, 39600);
        let c = &p.current;
        assert_eq!((c.dt, c.temp, c.feels_like, c.pressure, c.humidity), (1791417600, 23.4, 22.8, 1016.0, 62.0));
        assert_eq!((c.uvi, c.wind_speed, c.wind_gust, c.wind_deg), (Some(6.4), 4.2, Some(7.1), 225.0));
        assert_eq!((c.sunrise, c.sunset), (Some(1791400320), Some(1791445620)));
        assert_eq!(c.condition, crate::model::Condition { id: Some(802), icon: "03d".into(), description: "scattered clouds".into() });
        assert!(p.alert_ids.is_empty());
    }

    #[test]
    fn current_with_alert() {
        let p = current(&fixture("onecall4/current_with_alert.json")).unwrap();
        assert_eq!(p.tz_offset, 36000);
        assert_eq!(p.alert_ids, ["bom-qld-2026-10-08-0042"]);
        assert_eq!(p.current.condition.id, Some(201));
    }

    #[test]
    fn current_polar() {
        let p = current(&fixture("onecall4/current_polar_windy.json")).unwrap();
        assert_eq!((p.current.sunrise, p.current.sunset), (None, None));
        assert_eq!((p.current.wind_speed, p.current.condition.icon.as_str()), (12.6, "01n"));
    }

    #[test]
    fn current_alert_objects_and_empty_data() {
        let body = br#"{"timezone_offset":0,"data":[{"dt":1,"temp":1,"feels_like":1,"pressure":1,"humidity":1,"wind_speed":1,"alerts":[{"id":"a1"},"a2",3]}]}"#;
        let p = current(body).unwrap();
        assert_eq!(p.alert_ids, ["a1", "a2"]);
        assert_eq!(p.current.condition, Default::default());
        assert!(current(br#"{"timezone_offset":0,"data":[]}"#).is_err());
        assert!(current(b"not json").is_err());
    }

    #[test]
    fn hourly_pages() {
        let p1 = hours(&fixture("onecall4/timeline_1h_page1.json")).unwrap();
        let p2 = hours(&fixture("onecall4/timeline_1h_page2.json")).unwrap();
        assert_eq!((p1.hours.len(), p2.hours.len()), (20, 20));
        assert!(p1.next.as_deref().unwrap().contains("start=1791489600"));
        assert_eq!(p2.hours[0].dt, 1791489600);
        assert_eq!((p1.hours[0].dt, p1.hours[0].temp, p1.hours[0].pop), (1791417600, 21.5, 0.34));
        // Hourly steps.
        assert!(p1.hours.windows(2).all(|w| w[1].dt - w[0].dt == 3600));
    }

    #[test]
    fn daily() {
        let d = days(&fixture("onecall4/timeline_1day.json")).unwrap();
        assert_eq!(d.len(), 10);
        assert_eq!((d[0].dt, d[0].min, d[0].max, d[0].pop), (1791417600, 15.1, 27.0, 0.05));
        assert_eq!(d[0].condition.icon, "01d");
        assert_eq!((d[9].condition.id, d[9].pop), (Some(500), 0.3));
    }

    #[test]
    fn alert_brisbane() {
        let a = alert(&fixture("onecall4/alert.json")).unwrap();
        assert_eq!(a.id, "bom-qld-2026-10-08-0042");
        assert_eq!((a.event.as_str(), a.sender.as_str()), ("Severe Thunderstorm Warning", "Bureau of Meteorology"));
        assert_eq!((a.start, a.end), (1791424800, 1791450000));
        assert!(a.description("en").unwrap().starts_with("Severe thunderstorms are likely"));
        // Another UI language falls back to the first text.
        assert_eq!(a.description("de"), a.description("en"));
        let plain = alert(br#"{"id":"x","start":1,"end":2,"description":"Text"}"#).unwrap();
        assert_eq!(plain.description("en"), Some("Text"));
    }
}
