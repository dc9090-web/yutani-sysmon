//! `WEATHER_DEMO=1` (feature `demo`): no key and no network; the applet
//! steps through every state every 10 s, built from `tests/fixtures/`.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeDelta, Utc};

use crate::api::{KeyCheck, free25, geocoding, onecall4};
use crate::app::{KeyStatus, LocState, Page, Search};
use crate::config::{IconSet, Location, Units};
use crate::key::Key;
use crate::model::{Snapshot, Tier};
use crate::scheduler::{Budget, SOFT_CAP};

pub const STEP: std::time::Duration = std::time::Duration::from_secs(10);

/// The fixtures' "now": 2026-10-08T00:00:00Z.
const FIXTURE_NOW: i64 = 1_791_417_600;

pub fn enabled() -> bool {
    std::env::var_os("WEATHER_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

/// `WEATHER_DEMO_SCENE=n` starts on scene `n`.
pub fn first() -> usize {
    std::env::var("WEATHER_DEMO_SCENE").ok().and_then(|s| s.parse().ok()).unwrap_or(0) % SCENES
}

/// `WEATHER_SHOT=path`: after the first frames, save the window (RGBA, as a
/// PAM image) to `path` and exit.
pub fn shot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("WEATHER_SHOT").filter(|v| !v.is_empty()).map(Into::into)
}

/// Writes a screenshot as a PAM image (RGBA, no encoder needed).
pub fn save_shot(path: &std::path::Path, shot: &cosmic::iced::window::Screenshot) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", shot.size.width, shot.size.height)?;
    f.write_all(&shot.rgba)?;
    f.flush()
}

macro_rules! fixture {
    ($p:literal) => {
        include_bytes!(concat!("../tests/fixtures/", $p)).as_slice()
    };
}

/// A One Call snapshot from the fixtures.
fn full(current: &[u8], alert: bool) -> Snapshot {
    let now = onecall4::current(current).expect("fixture parses");
    let mut hourly = onecall4::hours(fixture!("onecall4/timeline_1h_page1.json")).expect("fixture parses").hours;
    hourly.extend(onecall4::hours(fixture!("onecall4/timeline_1h_page2.json")).expect("fixture parses").hours);
    hourly.truncate(24);
    let mut daily = onecall4::days(fixture!("onecall4/timeline_1day.json")).expect("fixture parses");
    daily.truncate(7);
    let alerts = if alert { vec![onecall4::alert(fixture!("onecall4/alert.json")).expect("fixture parses")] } else { vec![] };
    let mut s =
        Snapshot { tier: Tier::Full, units: Units::Metric, tz_offset: now.tz_offset, current: now.current, hourly, daily, alerts, fetched_at: Utc::now() };
    s.widen_today();
    s
}

fn free() -> Snapshot {
    let (tz, current) = free25::current(fixture!("free25/weather.json")).expect("fixture parses");
    let f = free25::forecast(fixture!("free25/forecast.json")).expect("fixture parses");
    let mut s = Snapshot {
        tier: Tier::Free,
        units: Units::Metric,
        tz_offset: tz,
        current,
        hourly: f.hourly(),
        daily: f.daily(),
        alerts: vec![],
        fetched_at: Utc::now(),
    };
    s.widen_today();
    s
}

/// Moves every time in `s` by whole days so the fixture's day is today, and marks it fetched `age_min` minutes ago.
fn shifted(mut s: Snapshot, now: DateTime<Utc>, age_min: i64) -> Snapshot {
    let d = (now.timestamp() - FIXTURE_NOW).div_euclid(86_400) * 86_400;
    let c = &mut s.current;
    c.dt += d;
    c.sunrise = c.sunrise.map(|t| t + d);
    c.sunset = c.sunset.map(|t| t + d);
    for h in &mut s.hourly {
        h.dt += d;
    }
    for x in &mut s.daily {
        x.dt += d;
    }
    for a in &mut s.alerts {
        a.start += d;
        a.end += d;
    }
    s.fetched_at = now - TimeDelta::minutes(age_min);
    s
}

fn loc(id: &str, name: &str, region: &str, lat: f64, lon: f64) -> Location {
    Location { id: id.into(), name: name.into(), region: region.into(), lat, lon }
}

pub struct Scene {
    pub name: &'static str,
    pub key: Option<Key>,
    pub key_invalid: bool,
    pub key_status: KeyStatus,
    pub tier: Option<Tier>,
    pub locations: Vec<Location>,
    pub units: Units,
    pub icons: IconSet,
    pub show_city: bool,
    pub show_hilo: bool,
    pub places: HashMap<String, LocState>,
    pub list_open: bool,
    pub budget: Budget,
    pub paused_until: Option<DateTime<Utc>>,
    pub page: Page,
    pub search: String,
    pub results: Search,
    pub open_alerts: HashSet<String>,
}

pub const SCENES: usize = 13;

pub fn scene(i: usize, now: DateTime<Utc>) -> Scene {
    let key = Key::new("00000000000000000000000000000000");
    let sydney = loc("syd", "Sydney", "New South Wales, AU", -33.87, 151.21);
    let brisbane = loc("bne", "Brisbane", "Queensland, AU", -27.47, 153.03);
    let svalbard = loc("lyr", "Longyearbyen", "Svalbard, SJ", 78.22, 15.65);
    let place = |s: Snapshot, offline: bool| LocState { snapshot: Some(s), fetching: false, completed_at: None, offline };
    let mut s = Scene {
        name: "normal",
        key,
        key_invalid: false,
        key_status: KeyStatus::Checked(KeyCheck::Works(Tier::Full)),
        tier: Some(Tier::Full),
        locations: vec![sydney.clone(), brisbane.clone(), svalbard.clone()],
        units: Units::Metric,
        icons: IconSet::Detailed,
        show_city: false,
        show_hilo: false,
        places: HashMap::from([
            ("syd".to_owned(), place(shifted(full(fixture!("onecall4/current.json"), false), now, 4), false)),
            ("bne".to_owned(), place(shifted(full(fixture!("onecall4/current_with_alert.json"), true), now, 4), false)),
            ("lyr".to_owned(), place(shifted(full(fixture!("onecall4/current_polar_windy.json"), false), now, 4), false)),
        ]),
        list_open: false,
        budget: Budget::new(now),
        paused_until: None,
        page: Page::Main,
        search: String::new(),
        results: Search::Idle,
        open_alerts: HashSet::new(),
    };
    let first = |s: &mut Scene, l: &Location| {
        s.locations.retain(|x| x.id != l.id);
        s.locations.insert(0, l.clone());
    };
    match i % SCENES {
        0 => {}
        1 => {
            s.name = "alert";
            first(&mut s, &brisbane);
            s.show_hilo = true;
            s.open_alerts.insert("bom-qld-2026-10-08-0042".into());
        }
        2 => {
            s.name = "polar-windy";
            first(&mut s, &svalbard);
            s.show_city = true;
        }
        3 => {
            s.name = "system-icons";
            s.icons = IconSet::System;
            s.show_city = true;
            s.show_hilo = true;
        }
        4 => {
            s.name = "free-plan";
            s.tier = Some(Tier::Free);
            s.key_status = KeyStatus::Checked(KeyCheck::Works(Tier::Free));
            s.locations = vec![sydney.clone()];
            s.places = HashMap::from([("syd".to_owned(), place(shifted(free(), now, 4), false))]);
        }
        5 => {
            s.name = "offline";
            if let Some(p) = s.places.get_mut("syd") {
                p.offline = true;
                if let Some(x) = p.snapshot.as_mut() {
                    x.fetched_at = now - TimeDelta::minutes(52);
                }
            }
        }
        6 => {
            s.name = "rate-limited";
            s.paused_until = Some(now + TimeDelta::minutes(8));
        }
        7 => {
            s.name = "daily-limit";
            s.budget = Budget { day: now.date_naive(), calls: SOFT_CAP };
        }
        8 => {
            s.name = "location-list";
            s.list_open = true;
        }
        9 => {
            s.name = "settings";
            s.page = Page::Settings;
            s.search = "Melbourne".into();
            s.results = Search::Results(geocoding::places(fixture!("geocoding/direct_melbourne.json"), "en").expect("fixture parses"));
        }
        10 => {
            s.name = "no-key";
            s.key = None;
            s.key_status = KeyStatus::Unchecked;
            s.places.clear();
        }
        11 => {
            s.name = "key-not-accepted";
            s.key_invalid = true;
            s.key_status = KeyStatus::Checked(KeyCheck::Invalid);
            s.tier = None;
        }
        _ => {
            s.name = "no-locations";
            s.locations.clear();
            s.places.clear();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_builds() {
        let now = Utc::now();
        let names: Vec<_> = (0..SCENES).map(|i| scene(i, now).name).collect();
        for n in ["normal", "alert", "free-plan", "offline", "rate-limited", "daily-limit", "no-key", "key-not-accepted", "no-locations", "polar-windy"] {
            assert!(names.contains(&n), "{n} missing from the demo");
        }
        // Times move to today, by whole days.
        let s = scene(0, now);
        let syd = s.places["syd"].snapshot.as_ref().unwrap();
        assert!((syd.current.dt - now.timestamp()).abs() < 86_400);
        assert_eq!((syd.current.dt - FIXTURE_NOW) % 86_400, 0);
        assert_eq!(syd.hourly.len(), 24);
    }
}
