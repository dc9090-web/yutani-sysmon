//! Condition icons (SPEC §6): the embedded Detailed set (Pixeden, never
//! installed as files) or COSMIC's System `weather-*` icons. One set at a time.

use cosmic::widget::icon;

use crate::config::IconSet;
use crate::model::Condition;

/// Wind at or above this (m/s) turns clear and cloudy codes windy.
pub const WINDY_MS: f64 = 10.8;

/// Embeds `wx-pd-<name>-symbolic.svg` files by name.
macro_rules! detailed {
    ($($name:literal),* $(,)?) => {
        fn detailed_bytes(name: &str) -> Option<&'static [u8]> {
            match name {
                $($name => Some(include_bytes!(concat!("../resources/icons/weather/wx-pd-", $name, "-symbolic.svg"))),)*
                _ => None,
            }
        }
    };
}

detailed![
    "clear-day",
    "clear-night",
    "cloudy",
    "compass",
    "compass-alt",
    "drizzle",
    "drizzle-day",
    "drizzle-night",
    "heavy-rain",
    "heavy-rain-day",
    "heavy-rain-night",
    "humidity",
    "partly-cloudy-day",
    "partly-cloudy-night",
    "rain",
    "rain-day",
    "rain-night",
    "snow",
    "snow-day",
    "snow-night",
    "storm",
    "storm-day",
    "storm-night",
    "storm-rain",
    "temp-high",
    "temp-low",
    "temp-mid",
    "windy",
    "windy-day",
    "windy-night",
];

/// The icon code's two digits and whether it's night (`NNn`).
fn split(code: &str) -> (&str, bool) {
    (code.get(..2).unwrap_or(""), code.get(2..) == Some("n"))
}

/// The Detailed icon name (`wx-pd-<name>`) for a condition. `wind_ms` is the
/// wind in m/s, already converted from mph in imperial mode.
pub fn detailed_name(code: &str, id: Option<u16>, wind_ms: Option<f64>) -> &'static str {
    let (c, night) = split(code);
    let dn = |day: &'static str, nite: &'static str| if night { nite } else { day };
    if wind_ms.is_some_and(|w| w >= WINDY_MS) && matches!(c, "01" | "02" | "03" | "04") {
        return if c == "04" { "windy" } else { dn("windy-day", "windy-night") };
    }
    if let Some(id) = id.filter(|i| *i > 0) {
        match id {
            200..=202 | 230..=299 => return "storm-rain",
            203..=229 => return dn("storm-day", "storm-night"),
            300..=399 => return dn("drizzle-day", "drizzle-night"),
            500 | 501 => return dn("rain-day", "rain-night"),
            502..=504 => return "heavy-rain",
            511 | 611..=616 => return "snow",
            520..=599 => return dn("heavy-rain-day", "heavy-rain-night"),
            600..=602 => return "snow",
            620..=699 => return dn("snow-day", "snow-night"),
            700..=799 => return "windy",
            _ => {}
        }
    }
    match c {
        "01" => dn("clear-day", "clear-night"),
        "02" | "03" => dn("partly-cloudy-day", "partly-cloudy-night"),
        "09" => dn("heavy-rain-day", "heavy-rain-night"),
        "10" => dn("rain-day", "rain-night"),
        "11" => dn("storm-day", "storm-night"),
        "13" => dn("snow-day", "snow-night"),
        "50" => "windy",
        _ => "cloudy",
    }
}

/// The System icon name (`<name>-symbolic`), by icon code.
pub fn system_name(code: &str) -> &'static str {
    let (c, night) = split(code);
    match c {
        "01" if night => "weather-clear-night",
        "01" => "weather-clear",
        "02" | "03" if night => "weather-few-clouds-night",
        "02" | "03" => "weather-few-clouds",
        "09" => "weather-showers",
        "10" => "weather-showers-scattered",
        "11" => "weather-storm",
        "13" => "weather-snow",
        "50" => "weather-fog",
        _ => "weather-overcast",
    }
}

/// One embedded Detailed icon by name (`humidity`, `compass` …).
pub fn detailed(name: &str) -> icon::Handle {
    match detailed_bytes(name) {
        Some(b) => icon::from_svg_bytes(b).symbolic(true),
        None => icon::from_name("weather-overcast-symbolic").symbolic(true).handle(),
    }
}

/// The icon for a condition in the chosen set.
pub fn condition(set: IconSet, c: &Condition, wind_ms: Option<f64>) -> icon::Handle {
    match set {
        IconSet::Detailed => detailed(detailed_name(&c.icon, c.id, wind_ms)),
        IconSet::System => icon::from_name(format!("{}-symbolic", system_name(&c.icon))).symbolic(true).handle(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_detailed_name_is_embedded() {
        let codes = ["01", "02", "03", "04", "09", "10", "11", "13", "50", "xx"];
        let ids = [
            None,
            Some(200),
            Some(210),
            Some(231),
            Some(300),
            Some(500),
            Some(502),
            Some(511),
            Some(520),
            Some(600),
            Some(611),
            Some(620),
            Some(701),
            Some(800),
            Some(801),
            Some(803),
        ];
        for c in codes {
            for n in ["d", "n"] {
                for id in ids {
                    for wind in [None, Some(12.0)] {
                        let name = detailed_name(&format!("{c}{n}"), id, wind);
                        assert!(detailed_bytes(name).is_some(), "{name}");
                    }
                }
            }
        }
    }

    #[test]
    fn detailed_by_id() {
        let d = |id: u16, code: &str| detailed_name(code, Some(id), Some(1.0));
        // SPEC §6, row by row.
        assert_eq!(d(201, "11d"), "storm-rain");
        assert_eq!(d(232, "11n"), "storm-rain");
        assert_eq!(d(211, "11d"), "storm-day");
        assert_eq!(d(221, "11n"), "storm-night");
        assert_eq!(d(301, "09d"), "drizzle-day");
        assert_eq!(d(321, "09n"), "drizzle-night");
        assert_eq!(d(500, "10d"), "rain-day");
        assert_eq!(d(501, "10n"), "rain-night");
        assert_eq!(d(503, "10d"), "heavy-rain");
        assert_eq!(d(511, "13d"), "snow");
        assert_eq!(d(601, "13n"), "snow");
        assert_eq!(d(615, "13d"), "snow");
        assert_eq!(d(521, "09d"), "heavy-rain-day");
        assert_eq!(d(531, "09n"), "heavy-rain-night");
        assert_eq!(d(621, "13d"), "snow-day");
        assert_eq!(d(622, "13n"), "snow-night");
        assert_eq!(d(741, "50d"), "windy");
        assert_eq!(d(800, "01d"), "clear-day");
        assert_eq!(d(800, "01n"), "clear-night");
        assert_eq!(d(801, "02d"), "partly-cloudy-day");
        assert_eq!(d(802, "03n"), "partly-cloudy-night");
        assert_eq!(d(803, "04d"), "cloudy");
        assert_eq!(d(804, "04n"), "cloudy");
    }

    #[test]
    fn detailed_windy_override() {
        // ≥ 10.8 m/s on codes 01–04 only.
        assert_eq!(detailed_name("01n", Some(800), Some(12.6)), "windy-night");
        assert_eq!(detailed_name("02d", Some(801), Some(10.8)), "windy-day");
        assert_eq!(detailed_name("04d", Some(804), Some(11.0)), "windy");
        assert_eq!(detailed_name("01d", Some(800), Some(10.7)), "clear-day");
        assert_eq!(detailed_name("10d", Some(500), Some(20.0)), "rain-day");
        // Imperial: 25 mph is 11.2 m/s once converted.
        let ms = crate::format::wind_ms(25.0, crate::config::Units::Imperial);
        assert_eq!(detailed_name("03d", Some(802), Some(ms)), "windy-day");
        assert_eq!(detailed_name("03d", Some(802), Some(crate::format::wind_ms(24.0, crate::config::Units::Imperial))), "partly-cloudy-day");
    }

    #[test]
    fn detailed_by_code() {
        let d = |code: &str| detailed_name(code, None, None);
        assert_eq!(d("01d"), "clear-day");
        assert_eq!(d("02n"), "partly-cloudy-night");
        assert_eq!(d("03d"), "partly-cloudy-day");
        assert_eq!(d("04n"), "cloudy");
        assert_eq!(d("09d"), "heavy-rain-day");
        assert_eq!(d("10n"), "rain-night");
        assert_eq!(d("11d"), "storm-day");
        assert_eq!(d("13n"), "snow-night");
        assert_eq!(d("50d"), "windy");
        assert_eq!(d(""), "cloudy");
        // Ids outside the table fall back to the code.
        assert_eq!(detailed_name("13d", Some(605), None), "snow-day");
    }

    #[test]
    fn system_by_code() {
        let s = system_name;
        assert_eq!(s("01d"), "weather-clear");
        assert_eq!(s("01n"), "weather-clear-night");
        assert_eq!(s("02d"), "weather-few-clouds");
        assert_eq!(s("03n"), "weather-few-clouds-night");
        assert_eq!(s("04d"), "weather-overcast");
        assert_eq!(s("09n"), "weather-showers");
        assert_eq!(s("10d"), "weather-showers-scattered");
        assert_eq!(s("11d"), "weather-storm");
        assert_eq!(s("13n"), "weather-snow");
        assert_eq!(s("50d"), "weather-fog");
        assert_eq!(s(""), "weather-overcast");
    }

    #[test]
    fn fixtures_map_as_documented() {
        use crate::api::{onecall4, tests::fixture};
        let c = onecall4::current(&fixture("onecall4/current.json")).unwrap().current;
        assert_eq!(detailed_name(&c.condition.icon, c.condition.id, Some(c.wind_speed)), "partly-cloudy-day");
        let p = onecall4::current(&fixture("onecall4/current_polar_windy.json")).unwrap().current;
        assert_eq!(detailed_name(&p.condition.icon, p.condition.id, Some(p.wind_speed)), "windy-night");
        let a = onecall4::current(&fixture("onecall4/current_with_alert.json")).unwrap().current;
        assert_eq!(detailed_name(&a.condition.icon, a.condition.id, Some(a.wind_speed)), "storm-rain");
    }
}
