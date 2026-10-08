//! The OpenWeather client (SPEC §3–4): tier detection, one refresh per tier
//! and the city search. Each request counts toward the daily call budget, so
//! every fetch reports how many it made.

pub mod free25;
pub mod geocoding;
pub mod onecall4;

use std::time::Duration;

use chrono::Utc;
use reqwest::Url;
use serde::Deserialize;

use crate::config::Units;
use crate::key::{Key, redact};
use crate::model::{Alert, Condition, Snapshot, Tier};

pub const HOST: &str = "api.openweathermap.org";
const BASE: &str = "https://api.openweathermap.org";

/// Hourly points kept on the full tier.
pub const HOURLY_POINTS: usize = 24;
/// Daily rows shown on the full tier.
pub const DAYS: usize = 7;
/// `Retry-After` when a 429 doesn't say.
const DEFAULT_RETRY: Duration = Duration::from_secs(10 * 60);

/// Why a request failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 401: the key is wrong, not active yet, or lacks the subscription.
    Unauthorized,
    /// 429, with how long to wait.
    RateLimited(Duration),
    /// 5xx, timeout, DNS, connection.
    Offline,
    /// Another non-success status.
    Status(u16),
    /// A body the parsers couldn't read.
    Format(Format),
}

/// A response that didn't parse. Holds serde's message, never a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format(pub String);

impl Format {
    fn new(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<serde_json::Error> for Format {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<Format> for Error {
    fn from(f: Format) -> Self {
        Self::Format(f)
    }
}

/// `weather[]`, shared by every endpoint.
#[derive(Deserialize)]
struct RawWeather {
    id: Option<u16>,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    description: String,
}

/// `weather[0]`, or an empty condition.
fn condition(w: &[RawWeather]) -> Condition {
    w.first().map(|w| Condition { id: w.id, icon: w.icon.clone(), description: w.description.clone() }).unwrap_or_default()
}

/// The shared client: rustls, 5 s to connect, 15 s in all, and redirects
/// only within the API host.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("cosmic-applet-weather/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|a| if a.url().host_str() == Some(HOST) && a.previous().len() < 5 { a.follow() } else { a.stop() }))
        .build()
        .expect("TLS backend initialises")
}

/// What to fetch for one location.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub lat: f64,
    pub lon: f64,
    pub units: Units,
    /// The OpenWeather `lang` code (`owm_lang`).
    pub lang: String,
}

/// A refresh's result and the calls it made, success or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub result: Result<Snapshot, Error>,
    pub calls: u32,
}

/// Counts requests as they're made.
#[derive(Debug, Default)]
struct Calls(u32);

fn url(path: &str, params: &[(&str, String)], key: &Key) -> Url {
    let mut u = Url::parse(BASE).expect("valid base URL").join(path).expect("valid path");
    u.query_pairs_mut().extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str()))).append_pair("appid", key.expose());
    u
}

fn location_params(req: &Request) -> Vec<(&'static str, String)> {
    vec![("lat", req.lat.to_string()), ("lon", req.lon.to_string()), ("units", req.units.param().to_owned()), ("lang", req.lang.clone())]
}

/// One GET. The URL is redacted before it's logged.
async fn get(client: &reqwest::Client, url: Url, calls: &mut Calls) -> Result<Vec<u8>, Error> {
    calls.0 += 1;
    let shown = redact(url.as_str());
    tracing::debug!("GET {shown}");
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("request failed: {}", redact(&e.without_url().to_string()));
            return Err(Error::Offline);
        }
    };
    let status = response.status().as_u16();
    match status {
        200..=299 => {}
        401 => return Err(Error::Unauthorized),
        429 => return Err(Error::RateLimited(retry_after(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok())))),
        500..=599 => return Err(Error::Offline),
        _ => {
            tracing::debug!(status, "unexpected status from {shown}");
            return Err(Error::Status(status));
        }
    }
    match response.bytes().await {
        Ok(b) => Ok(b.to_vec()),
        Err(e) => {
            tracing::debug!("reading response: {}", redact(&e.without_url().to_string()));
            Err(Error::Offline)
        }
    }
}

/// `Retry-After` in seconds; 10 min if it's missing or a date.
fn retry_after(v: Option<&str>) -> Duration {
    v.and_then(|s| s.trim().parse::<u64>().ok()).map_or(DEFAULT_RETRY, Duration::from_secs)
}

/// The result of checking a key (SPEC §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCheck {
    Works(Tier),
    /// Both endpoints answered 401.
    Invalid,
    /// Couldn't tell (offline, rate limited, server error). Keep the last tier.
    Unknown,
}

/// Tries One Call 4.0, then Current Weather 2.5, at 0°, 0°.
pub async fn detect_tier(client: reqwest::Client, key: Key) -> (KeyCheck, u32) {
    let mut calls = Calls::default();
    let probe = [("lat", "0".to_owned()), ("lon", "0".to_owned())];
    let check = match get(&client, url("/data/4.0/onecall/current", &probe, &key), &mut calls).await {
        Ok(_) => KeyCheck::Works(Tier::Full),
        Err(Error::Unauthorized | Error::Status(_)) => match get(&client, url("/data/2.5/weather", &probe, &key), &mut calls).await {
            Ok(_) => KeyCheck::Works(Tier::Free),
            Err(Error::Unauthorized) => KeyCheck::Invalid,
            Err(_) => KeyCheck::Unknown,
        },
        Err(_) => KeyCheck::Unknown,
    };
    tracing::debug!(?check, calls = calls.0, "key checked");
    (check, calls.0)
}

/// One location on the full tier: `current`, two `timeline/1h` pages,
/// `timeline/1day`, and any alert not in `known` (SPEC §4.1).
pub async fn fetch_full(client: reqwest::Client, key: Key, req: Request, known: Vec<Alert>) -> Fetched {
    let mut calls = Calls::default();
    let result = full(&client, &key, &req, &known, &mut calls).await;
    Fetched { result, calls: calls.0 }
}

async fn full(client: &reqwest::Client, key: &Key, req: &Request, known: &[Alert], calls: &mut Calls) -> Result<Snapshot, Error> {
    let params = location_params(req);
    let now = onecall4::current(&get(client, url("/data/4.0/onecall/current", &params, key), calls).await?)?;

    let first = onecall4::hours(&get(client, url("/data/4.0/onecall/timeline/1h", &params, key), calls).await?)?;
    let mut hourly = first.hours;
    // Follow `next` once.
    if hourly.len() < HOURLY_POINTS
        && let Some(next) = first.next.as_deref().and_then(|n| follow(n, req, key))
    {
        hourly.extend(onecall4::hours(&get(client, next, calls).await?)?.hours);
    }
    hourly.truncate(HOURLY_POINTS);

    let mut daily = onecall4::days(&get(client, url("/data/4.0/onecall/timeline/1day", &params, key), calls).await?)?;
    daily.truncate(DAYS);

    let mut alerts = Vec::new();
    for id in &now.alert_ids {
        if let Some(a) = known.iter().find(|a| &a.id == id) {
            alerts.push(a.clone());
            continue;
        }
        let path = format!("/data/4.0/onecall/alert/{}", encode_segment(id));
        match get(client, url(&path, &[("lang", req.lang.clone())], key), calls).await.and_then(|b| Ok(onecall4::alert(&b)?)) {
            Ok(a) => alerts.push(a),
            // The rest of the forecast is still good without one alert.
            Err(e @ (Error::Status(_) | Error::Format(_))) => tracing::warn!(?e, "skipping an alert that couldn't be read"),
            Err(e) => return Err(e),
        }
    }
    alerts.sort_by_key(|a| std::cmp::Reverse(a.start));

    let mut snapshot =
        Snapshot { tier: Tier::Full, units: req.units, tz_offset: now.tz_offset, current: now.current, hourly, daily, alerts, fetched_at: Utc::now() };
    snapshot.widen_today();
    Ok(snapshot)
}

/// The `next` link, if it points at the API: our own key replaces whatever
/// `appid` it carries, and `units` / `lang` are added when missing.
fn follow(next: &str, req: &Request, key: &Key) -> Option<Url> {
    let mut u = Url::parse(next).ok()?;
    if u.scheme() != "https" || u.host_str() != Some(HOST) {
        tracing::warn!("ignoring a next link to another host");
        return None;
    }
    let mut pairs: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| k != "appid").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    for (k, v) in [("units", req.units.param().to_owned()), ("lang", req.lang.clone())] {
        if !pairs.iter().any(|(pk, _)| pk == k) {
            pairs.push((k.to_owned(), v));
        }
    }
    u.query_pairs_mut().clear().extend_pairs(pairs).append_pair("appid", key.expose());
    Some(u)
}

/// An alert ID as one path segment.
fn encode_segment(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// One location on the free tier: Current Weather 2.5 and the 3-hour
/// Forecast 2.5 (SPEC §4.2).
pub async fn fetch_free(client: reqwest::Client, key: Key, req: Request) -> Fetched {
    let mut calls = Calls::default();
    let result = free(&client, &key, &req, &mut calls).await;
    Fetched { result, calls: calls.0 }
}

async fn free(client: &reqwest::Client, key: &Key, req: &Request, calls: &mut Calls) -> Result<Snapshot, Error> {
    let params = location_params(req);
    let (tz_offset, current) = free25::current(&get(client, url("/data/2.5/weather", &params, key), calls).await?)?;
    let forecast = free25::forecast(&get(client, url("/data/2.5/forecast", &params, key), calls).await?)?;
    let mut snapshot = Snapshot {
        tier: Tier::Free,
        units: req.units,
        tz_offset,
        current,
        hourly: forecast.hourly(),
        daily: forecast.daily(),
        alerts: Vec::new(),
        fetched_at: Utc::now(),
    };
    snapshot.widen_today();
    Ok(snapshot)
}

/// City search (SPEC §4.3). One call.
pub async fn search(client: reqwest::Client, key: Key, query: String, lang: String) -> (Result<Vec<geocoding::Place>, Error>, u32) {
    let mut calls = Calls::default();
    let params = [("q", query), ("limit", geocoding::LIMIT.to_string())];
    let result = match get(&client, url("/geo/1.0/direct", &params, &key), &mut calls).await {
        Ok(body) => geocoding::places(&body, &lang).map_err(Error::from),
        Err(e) => Err(e),
    };
    (result, calls.0)
}

/// The OpenWeather `lang` code for a UI language: mostly ISO 639-1, with
/// OpenWeather's own codes where they differ. Unsupported languages get `en`.
pub fn owm_lang(lang: &str, region: Option<&str>, script: Option<&str>) -> String {
    const SUPPORTED: [&str; 44] = [
        "af", "ar", "az", "bg", "ca", "da", "de", "el", "en", "eu", "fa", "fi", "fr", "gl", "he", "hi", "hr", "hu", "id", "it", "ja", "lt", "mk", "nl", "pl",
        "pt", "ro", "ru", "sk", "sl", "es", "sr", "sv", "th", "tr", "uk", "vi", "zu", "cz", "kr", "la", "al", "no", "pt_br",
    ];
    let lang = lang.to_ascii_lowercase();
    let region = region.map(str::to_ascii_lowercase);
    let code = match (lang.as_str(), region.as_deref()) {
        ("zh", Some("tw" | "hk" | "mo")) => "zh_tw",
        ("zh", _) if script.is_some_and(|s| s.eq_ignore_ascii_case("hant")) => "zh_tw",
        ("zh", _) => "zh_cn",
        ("pt", Some("br")) => "pt_br",
        ("cs", _) => "cz",
        ("ko", _) => "kr",
        ("lv", _) => "la",
        ("sq", _) => "al",
        ("nb" | "nn", _) => "no",
        (l, _) => l,
    };
    if code.starts_with("zh_") || SUPPORTED.contains(&code) { code.to_owned() } else { "en".to_owned() }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn key() -> Key {
        Key::new("0123456789abcdef0123456789abcdef").unwrap()
    }

    fn sydney() -> Request {
        Request { lat: -33.8688, lon: 151.2093, units: Units::Metric, lang: "en".into() }
    }

    #[test]
    fn urls() {
        let u = url("/data/4.0/onecall/current", &location_params(&sydney()), &key());
        assert_eq!(
            u.as_str(),
            "https://api.openweathermap.org/data/4.0/onecall/current?lat=-33.8688&lon=151.2093&units=metric&lang=en&appid=0123456789abcdef0123456789abcdef"
        );
        assert!(!redact(u.as_str()).contains("0123456789abcdef"));
        let q = url("/geo/1.0/direct", &[("q", "São Paulo, BR".into())], &key());
        assert_eq!(q.query(), Some("q=S%C3%A3o+Paulo%2C+BR&appid=0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn next_link() {
        let page = onecall4::hours(&fixture("onecall4/timeline_1h_page1.json")).unwrap();
        let u = follow(page.next.as_deref().unwrap(), &sydney(), &key()).unwrap();
        // The placeholder appid is replaced; lang is added.
        assert_eq!(
            u.as_str(),
            "https://api.openweathermap.org/data/4.0/onecall/timeline/1h?lat=-33.8688&lon=151.2093&units=metric&start=1791489600&lang=en&appid=0123456789abcdef0123456789abcdef"
        );
        assert_eq!(follow("https://evil.example/data?appid=x", &sydney(), &key()), None);
        assert_eq!(follow("http://api.openweathermap.org/data", &sydney(), &key()), None);
        assert_eq!(follow("not a url", &sydney(), &key()), None);
    }

    #[test]
    fn segment_encoding() {
        assert_eq!(encode_segment("bom-qld-2026-10-08-0042"), "bom-qld-2026-10-08-0042");
        assert_eq!(encode_segment("a/b c?"), "a%2Fb%20c%3F");
    }

    #[test]
    fn retry_after_header() {
        assert_eq!(retry_after(Some("120")), Duration::from_secs(120));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), DEFAULT_RETRY);
        assert_eq!(retry_after(None), DEFAULT_RETRY);
    }

    #[test]
    fn languages() {
        assert_eq!(owm_lang("en", Some("US"), None), "en");
        assert_eq!(owm_lang("de", None, None), "de");
        assert_eq!(owm_lang("pt", Some("BR"), None), "pt_br");
        assert_eq!(owm_lang("pt", Some("PT"), None), "pt");
        assert_eq!(owm_lang("zh", Some("CN"), None), "zh_cn");
        assert_eq!(owm_lang("zh", None, Some("Hant")), "zh_tw");
        assert_eq!(owm_lang("zh", Some("TW"), None), "zh_tw");
        assert_eq!(owm_lang("cs", None, None), "cz");
        assert_eq!(owm_lang("ko", None, None), "kr");
        assert_eq!(owm_lang("nb", None, None), "no");
        assert_eq!(owm_lang("xx", None, None), "en");
    }

    #[test]
    fn error_bodies() {
        // 401/429 are handled by status; their bodies are ordinary JSON objects.
        for f in ["errors/401_invalid_key.json", "errors/429_rate_limited.json"] {
            let v: serde_json::Value = serde_json::from_slice(&fixture(f)).unwrap();
            assert!(v.get("cod").is_some() && v.get("message").is_some());
        }
    }

    #[test]
    fn full_assembly_widens_today() {
        // The full tier's pieces, put together the way `full` does.
        let now = onecall4::current(&fixture("onecall4/current.json")).unwrap();
        let mut hourly = onecall4::hours(&fixture("onecall4/timeline_1h_page1.json")).unwrap().hours;
        hourly.extend(onecall4::hours(&fixture("onecall4/timeline_1h_page2.json")).unwrap().hours);
        hourly.truncate(HOURLY_POINTS);
        let mut daily = onecall4::days(&fixture("onecall4/timeline_1day.json")).unwrap();
        daily.truncate(DAYS);
        let mut s = Snapshot {
            tier: Tier::Full,
            units: Units::Metric,
            tz_offset: now.tz_offset,
            current: now.current,
            hourly,
            daily,
            alerts: vec![],
            fetched_at: Utc::now(),
        };
        assert_eq!((s.hourly.len(), s.daily.len()), (24, 7));
        assert_eq!(s.hourly[23].dt - s.hourly[0].dt, 23 * 3600);
        s.widen_today();
        let today = crate::model::local_date(s.current.dt, s.tz_offset);
        let hourly_today: Vec<f64> = s.hourly.iter().filter(|h| crate::model::local_date(h.dt, s.tz_offset) == today).map(|h| h.temp).collect();
        let d = &s.daily[0];
        assert!(hourly_today.iter().chain([&s.current.temp]).all(|t| (d.min..=d.max).contains(t)));
        assert!(d.min <= 15.1 && d.max >= 27.0);
        // Tomorrow is untouched.
        assert_eq!((s.daily[1].min, s.daily[1].max), {
            let raw = onecall4::days(&fixture("onecall4/timeline_1day.json")).unwrap();
            (raw[1].min, raw[1].max)
        });
    }

    #[test]
    fn free_assembly() {
        let (tz, current) = free25::current(&fixture("free25/weather.json")).unwrap();
        let f = free25::forecast(&fixture("free25/forecast.json")).unwrap();
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
        assert_eq!((s.hourly.len(), s.daily.len(), s.current.uvi, s.alerts.len()), (9, 5, None, 0));
        assert!((s.daily[0].min..=s.daily[0].max).contains(&s.current.temp));
    }

    /// Milestone 2's live check: one real call per endpoint with the stored
    /// key, parsed, and the response's key paths compared with the fixture's.
    /// Prints key paths only, never values or the key.
    ///
    /// `cargo test -p cosmic-applet-weather live_check -- --ignored --nocapture`
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "makes real OpenWeather calls with the stored key"]
    async fn live_check() {
        let Some((key, storage)) = crate::key::load().await else { panic!("no stored key: save one in Applet settings first") };
        println!("key: found in {storage:?}");
        let client = client();
        let (check, _) = detect_tier(client.clone(), key.clone()).await;
        println!("tier: {check:?}");
        let req = sydney();
        let p = location_params(&req);
        let raw = |path: &str, params: Vec<(&'static str, String)>| {
            let u = url(path, &params, &key);
            let client = client.clone();
            async move {
                let mut c = Calls::default();
                let r = get(&client, u, &mut c).await;
                (r, c.0)
            }
        };
        let report = |name: &str, fixture_name: &str, body: &Result<Vec<u8>, Error>, parsed: Option<bool>| match body {
            Err(e) => println!("\n{name}: {e:?}"),
            Ok(b) => {
                let live: serde_json::Value = serde_json::from_slice(b).unwrap_or_default();
                let fix: serde_json::Value = serde_json::from_slice(&fixture(fixture_name)).unwrap();
                let (l, f) = (paths(&live), paths(&fix));
                let missing: Vec<_> = f.difference(&l).collect();
                let extra: Vec<_> = l.difference(&f).collect();
                println!(
                    "\n{name}: parsed {}",
                    match parsed {
                        Some(true) => "ok",
                        Some(false) => "FAILED",
                        None => "-",
                    }
                );
                println!("  in fixture, not live: {missing:?}");
                println!("  live, not in fixture: {extra:?}");
            }
        };

        let (b, _) = raw("/data/4.0/onecall/current", p.clone()).await;
        let cur = b.as_ref().ok().map(|b| onecall4::current(b));
        report("onecall4/current", "onecall4/current.json", &b, cur.as_ref().map(Result::is_ok));
        let (b, _) = raw("/data/4.0/onecall/timeline/1h", p.clone()).await;
        let page = b.as_ref().ok().map(|b| onecall4::hours(b));
        report("onecall4/timeline/1h page 1", "onecall4/timeline_1h_page1.json", &b, page.as_ref().map(Result::is_ok));
        if let Some(Ok(page)) = &page {
            println!("  points: {}, next: {}", page.hours.len(), page.next.is_some());
            if let Some(next) = page.next.as_deref().and_then(|n| follow(n, &req, &key)) {
                let mut c = Calls::default();
                let b = get(&client, next, &mut c).await;
                let p2 = b.as_ref().ok().map(|b| onecall4::hours(b));
                report("onecall4/timeline/1h page 2", "onecall4/timeline_1h_page2.json", &b, p2.as_ref().map(Result::is_ok));
            }
        }
        let (b, _) = raw("/data/4.0/onecall/timeline/1day", p.clone()).await;
        let days = b.as_ref().ok().map(|b| onecall4::days(b));
        report("onecall4/timeline/1day", "onecall4/timeline_1day.json", &b, days.as_ref().map(Result::is_ok));
        if let Some(Ok(days)) = &days {
            println!("  days: {}", days.len());
        }
        match cur.and_then(Result::ok).and_then(|c| c.alert_ids.first().cloned()) {
            Some(id) => {
                let (b, _) = raw(&format!("/data/4.0/onecall/alert/{}", encode_segment(&id)), vec![("lang", "en".into())]).await;
                let a = b.as_ref().ok().map(|b| onecall4::alert(b));
                report("onecall4/alert", "onecall4/alert.json", &b, a.as_ref().map(Result::is_ok));
            }
            None => println!("\nonecall4/alert: no active alert for Sydney, skipped"),
        }

        let (b, _) = raw("/data/2.5/weather", p.clone()).await;
        let w = b.as_ref().ok().map(|b| free25::current(b));
        report("free25/weather", "free25/weather.json", &b, w.as_ref().map(Result::is_ok));
        let (b, _) = raw("/data/2.5/forecast", p.clone()).await;
        let f = b.as_ref().ok().map(|b| free25::forecast(b));
        report("free25/forecast", "free25/forecast.json", &b, f.as_ref().map(Result::is_ok));
        if let Some(Ok(f)) = &f {
            println!("  steps: {}, hourly: {}, days: {}", f.entries.len(), f.hourly().len(), f.daily().len());
        }

        let (b, _) = raw("/geo/1.0/direct", vec![("q", "Melbourne".into()), ("limit", "5".into())]).await;
        let g = b.as_ref().ok().map(|b| geocoding::places(b, "en"));
        report("geocoding/direct", "geocoding/direct_melbourne.json", &b, g.as_ref().map(Result::is_ok));
        if let Some(Ok(g)) = &g {
            println!("  results: {:?}", g.iter().map(|p| format!("{} · {}", p.name, p.region)).collect::<Vec<_>>());
        }
        let (b, _) = raw("/geo/1.0/direct", vec![("q", "Xqzvbnmplk".into()), ("limit", "5".into())]).await;
        report("geocoding/direct (no match)", "geocoding/direct_empty.json", &b, None);

        // A 401 with a made-up key: the error body's shape.
        let bad = Key::new("00000000000000000000000000000000").unwrap();
        let response = client.get(url("/data/2.5/weather", &p, &bad)).send().await;
        match response {
            Ok(r) => {
                let status = r.status().as_u16();
                let live: serde_json::Value = r.json().await.unwrap_or_default();
                let fix: serde_json::Value = serde_json::from_slice(&fixture("errors/401_invalid_key.json")).unwrap();
                println!("\nerrors/401: HTTP {status}, same keys as the fixture: {}", paths(&live) == paths(&fix));
            }
            Err(_) => println!("\nerrors/401: offline"),
        }
    }

    /// Key paths in a JSON value (`a.b`, arrays as `a[]`).
    fn paths(v: &serde_json::Value) -> std::collections::BTreeSet<String> {
        fn walk(v: &serde_json::Value, prefix: &str, out: &mut std::collections::BTreeSet<String>) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, x) in m {
                        let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                        out.insert(p.clone());
                        walk(x, &p, out);
                    }
                }
                serde_json::Value::Array(a) => {
                    let p = format!("{prefix}[]");
                    for x in a {
                        walk(x, &p, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = std::collections::BTreeSet::new();
        walk(v, "", &mut out);
        out
    }
}
