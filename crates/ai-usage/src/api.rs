//! The usage endpoint (SPEC §4). It's undocumented, so everything that
//! knows its shape lives here.

use std::collections::BTreeSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::auth::{Token, redact};
use crate::model::{Kind, Usage, Window};

pub const URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Status and JSON key paths of a response we couldn't read. Never values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub status: u16,
    pub keys: Vec<String>,
}

impl Diagnostics {
    /// The "Copy diagnostics" text.
    pub fn text(&self) -> String {
        let mut s = format!("cosmic-applet-ai-usage {}\nGET {URL}\nHTTP {}\nkeys:\n", env!("CARGO_PKG_VERSION"), self.status);
        for k in &self.keys {
            s.push_str("  ");
            s.push_str(k);
            s.push('\n');
        }
        s
    }
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Usage(Usage),
    /// 401 / 403.
    Unauthorized,
    /// 429, with how long to wait.
    RateLimited(Duration),
    /// 5xx, timeout, DNS, connection.
    Offline,
    Unrecognised(Diagnostics),
}

/// The shared client: rustls, 5 s connect timeout, 20 s overall.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("cosmic-applet-ai-usage/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("TLS backend initialises")
}

/// One request. The token is dropped when this returns.
pub async fn fetch(client: reqwest::Client, token: Token) -> Outcome {
    let response = client.get(URL).bearer_auth(token.expose()).header("anthropic-beta", "oauth-2025-04-20").send().await;
    drop(token);
    let response = match response {
        Ok(r) => r,
        Err(e) => {
            // reqwest errors carry the URL, never the headers; redact anyway.
            tracing::debug!("usage request failed: {}", redact(&e.without_url().to_string()));
            return Outcome::Offline;
        }
    };
    let status = response.status();
    match status.as_u16() {
        401 | 403 => return Outcome::Unauthorized,
        429 => return Outcome::RateLimited(retry_after(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()))),
        500..=599 => return Outcome::Offline,
        _ => {}
    }
    let body = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!("reading usage response: {}", redact(&e.without_url().to_string()));
            return Outcome::Offline;
        }
    };
    let json = serde_json::from_slice::<Value>(&body).ok();
    if status.is_success()
        && let Some(usage) = json.as_ref().and_then(parse)
    {
        return Outcome::Usage(usage);
    }
    Outcome::Unrecognised(Diagnostics { status: status.as_u16(), keys: json.as_ref().map(key_paths).unwrap_or_default() })
}

/// `Retry-After` in seconds; 300 if it's missing or a date.
fn retry_after(v: Option<&str>) -> Duration {
    Duration::from_secs(v.and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(300))
}

/// The windows in a response, or `None` if neither `five_hour` nor
/// `seven_day` is there.
pub fn parse(json: &Value) -> Option<Usage> {
    let session = window(json.get("five_hour"), "utilization", Kind::Session);
    let weekly = window(json.get("seven_day"), "utilization", Kind::Weekly);
    if session.is_none() && weekly.is_none() {
        return None;
    }
    let fable = json.get("limits").and_then(Value::as_array).and_then(|l| l.iter().find(|e| is_fable(e))).and_then(|e| window(Some(e), "percent", Kind::Fable));
    Some(Usage { windows: [session, weekly, fable].into_iter().flatten().collect() })
}

/// A `{ <field>: 0–100, resets_at }` object. A null or missing percentage
/// means the window is absent.
fn window(v: Option<&Value>, field: &str, kind: Kind) -> Option<Window> {
    let v = v?;
    let used = v.get(field)?.as_f64().filter(|x| x.is_finite())?.clamp(0.0, 100.0) as f32;
    let resets_at = v.get("resets_at").and_then(Value::as_str).and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc));
    Some(Window { kind, used, resets_at })
}

/// The weekly limit scoped to the Fable model.
fn is_fable(e: &Value) -> bool {
    let s = |k: &str| e.get(k).and_then(Value::as_str);
    let model = e.pointer("/scope/model");
    let name = model.and_then(|m| m.get("display_name")).and_then(Value::as_str);
    let id = model.and_then(|m| m.get("id")).and_then(Value::as_str);
    s("group") == Some("weekly")
        && s("kind") == Some("weekly_scoped")
        && (name.is_some_and(|n| n.eq_ignore_ascii_case("fable")) || id.is_some_and(|i| i.to_ascii_lowercase().starts_with("fable")))
}

/// Every key path in a JSON value (`a.b`, arrays as `a[]`), sorted.
pub fn key_paths(v: &Value) -> Vec<String> {
    fn walk(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                    out.insert(p.clone());
                    walk(x, &p, out);
                }
            }
            Value::Array(a) => {
                let p = format!("{prefix}[]");
                for x in a {
                    walk(x, &p, out);
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(v, "", &mut out);
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let p = format!("{}/tests/fixtures/usage/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    fn at(s: &str) -> Option<DateTime<Utc>> {
        Some(DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc))
    }

    fn used(u: &Usage) -> Vec<(Kind, f32)> {
        u.windows.iter().map(|w| (w.kind, w.used)).collect()
    }

    #[test]
    fn normal_with_fable() {
        let u = parse(&fixture("normal_with_fable")).unwrap();
        assert_eq!(used(&u), [(Kind::Session, 42.0), (Kind::Weekly, 61.0), (Kind::Fable, 84.0)]);
        assert_eq!(u.get(Kind::Session).unwrap().resets_at, at("2026-10-08T12:13:00Z"));
        assert_eq!(u.get(Kind::Fable).unwrap().resets_at, at("2026-10-11T14:00:00Z"));
    }

    #[test]
    fn no_fable_limit() {
        let u = parse(&fixture("no_fable_limit")).unwrap();
        assert_eq!(used(&u), [(Kind::Session, 12.0), (Kind::Weekly, 34.0)]);
    }

    #[test]
    fn legacy_no_limits_array() {
        // No Fable window, and no fallback to seven_day_opus / seven_day_sonnet.
        let u = parse(&fixture("legacy_no_limits_array")).unwrap();
        assert_eq!(used(&u), [(Kind::Session, 5.0), (Kind::Weekly, 10.0)]);
        assert!(u.windows.iter().all(|w| w.resets_at.is_none()));
    }

    #[test]
    fn session_limit_reached() {
        let u = parse(&fixture("session_limit_reached")).unwrap();
        assert_eq!(used(&u), [(Kind::Session, 100.0), (Kind::Weekly, 88.0), (Kind::Fable, 100.0)]);
    }

    #[test]
    fn unknown_shape() {
        let v = fixture("unknown_shape");
        assert_eq!(parse(&v), None);
        assert_eq!(key_paths(&v), ["windows", "windows[].name", "windows[].used"]);
    }

    #[test]
    fn clamping_and_nulls() {
        let v = serde_json::json!({
            "five_hour": { "utilization": 140.0, "resets_at": "not a date" },
            "seven_day": { "utilization": null, "resets_at": null },
            "limits": [{ "kind": "weekly_scoped", "group": "weekly", "percent": -3, "scope": { "model": { "id": "fable-5" } } }]
        });
        let u = parse(&v).unwrap();
        assert_eq!(used(&u), [(Kind::Session, 100.0), (Kind::Fable, 0.0)]);
        assert_eq!(u.get(Kind::Session).unwrap().resets_at, None);
        // Both main windows null: not recognised.
        assert_eq!(parse(&serde_json::json!({ "five_hour": { "utilization": null }, "seven_day": null })), None);
        assert_eq!(parse(&serde_json::json!([1, 2])), None);
    }

    #[test]
    fn fable_matching() {
        let entry = |group: &str, kind: &str, name: &str, id: &str| {
            serde_json::json!({ "five_hour": { "utilization": 1 }, "limits": [
                { "group": group, "kind": kind, "percent": 50, "scope": { "model": { "display_name": name, "id": id } } }
            ]})
        };
        let has = |v: Value| parse(&v).unwrap().get(Kind::Fable).is_some();
        assert!(has(entry("weekly", "weekly_scoped", "FABLE", "x")));
        assert!(has(entry("weekly", "weekly_scoped", "Other", "fable-6")));
        assert!(!has(entry("weekly", "weekly_scoped", "Opus", "opus-5")));
        assert!(!has(entry("daily", "weekly_scoped", "Fable", "fable-5")));
        assert!(!has(entry("weekly", "weekly", "Fable", "fable-5")));
    }

    #[test]
    fn diagnostics_have_no_values() {
        let v = fixture("normal_with_fable");
        let d = Diagnostics { status: 200, keys: key_paths(&v) };
        let text = d.text();
        assert!(text.contains("HTTP 200") && text.contains("limits[].scope.model.display_name"));
        assert!(!text.contains("Fable\n") && !text.contains("84") && !text.contains("2026-10"));
    }

    #[test]
    fn retry_after_header() {
        assert_eq!(retry_after(Some("120")), Duration::from_secs(120));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), Duration::from_secs(300));
        assert_eq!(retry_after(None), Duration::from_secs(300));
    }
}
