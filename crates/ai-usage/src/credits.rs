//! The monthly Claude Platform credits that come with Max, Ultra and Team
//! plans, shown as this billing cycle's API spend against the credit. The
//! balance itself isn't exposed to OAuth logins, so the spend comes from
//! the Admin API cost report with an Admin API key the user pastes in.
//! Everything that knows that endpoint's shape, or holds the key, lives
//! here. The key is kept in the Secret Service keyring only.

use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, TimeDelta, Utc};
use serde_json::Value;

use crate::auth::redact;
use crate::config::APP_ID;
use crate::format::{Level, Pace, pace};

pub const URL: &str = "https://api.anthropic.com/v1/organizations/cost_report";
const LABEL: &str = "Claude Admin API key";
/// Pages of the cost report read for one cycle before giving up.
const MAX_PAGES: usize = 8;

/// An Admin API key. `Debug` never shows it, and there is no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct AdminKey(String);

impl AdminKey {
    /// A pasted key, trimmed; `None` if nothing is left.
    pub fn new(s: &str) -> Option<Self> {
        let s = s.trim();
        (!s.is_empty()).then(|| Self(s.to_owned()))
    }

    /// The raw value, for the `x-api-key` header only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AdminKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminKey(<redacted>)")
    }
}

/// Anthropic keys start `sk-ant-`: Admin keys have been `sk-ant-admin01-…`
/// and, since the platform.claude.com Console, `sk-ant-usr-…`. Other shapes
/// get a warning but can still be tested.
pub fn looks_valid(s: &str) -> bool {
    s.trim().starts_with("sk-ant-")
}

// ---- billing cycle ----------------------------------------------------------------

/// The month the credits cover: from one arrival to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    pub start: DateTime<Utc>,
    /// When the next credits arrive.
    pub end: DateTime<Utc>,
}

impl Cycle {
    /// The cycle containing `now`, for credits that arrive at 00:00 UTC on
    /// `day` of each month. Days past the end of a month land on its last
    /// day (the 31st → 30 Nov, 28 or 29 Feb).
    pub fn at(day: u8, now: DateTime<Utc>) -> Self {
        let day = u32::from(day.clamp(1, 31));
        let today = now.date_naive();
        let this_month = renewal(today.year(), today.month(), day);
        let start = if this_month <= today {
            this_month
        } else {
            let (y, m) = previous_month(today.year(), today.month());
            renewal(y, m, day)
        };
        let (y, m) = next_month(start.year(), start.month());
        let end = renewal(y, m, day);
        let at_midnight = |d: NaiveDate| d.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc();
        Self { start: at_midnight(start), end: at_midnight(end) }
    }

    pub fn length(&self) -> TimeDelta {
        self.end - self.start
    }
}

fn renewal(year: i32, month: u32, day: u32) -> NaiveDate {
    let (ny, nm) = next_month(year, month);
    let last = NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid month").pred_opt().expect("not year zero").day();
    NaiveDate::from_ymd_opt(year, month, day.min(last)).expect("clamped to the month")
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 { (year + 1, 1) } else { (year, month + 1) }
}

fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 { (year - 1, 12) } else { (year, month - 1) }
}

// ---- the cost report --------------------------------------------------------------

/// This cycle's spend, in cents, and when it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spend {
    pub cents: i64,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Cents spent in the cycle so far.
    Spent(i64),
    /// 401 / 403: not an Admin key, or revoked.
    Unauthorized,
    RateLimited(Duration),
    /// 5xx, timeout, DNS, connection.
    Offline,
    /// A response that isn't the cost report's shape; the HTTP status.
    Unrecognised(u16),
}

/// Reads the cycle's spend so far. The key is dropped when this returns.
pub async fn fetch(client: reqwest::Client, key: AdminKey, cycle: Cycle, now: DateTime<Utc>) -> Outcome {
    let end = now.min(cycle.end);
    if end <= cycle.start {
        return Outcome::Spent(0);
    }
    let stamp = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut total = 0;
    let mut page: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut request = client.get(URL).header("x-api-key", key.expose()).header("anthropic-version", "2023-06-01").query(&[
            ("starting_at", stamp(cycle.start)),
            ("ending_at", stamp(end)),
            ("bucket_width", "1d".into()),
            ("limit", "31".into()),
        ]);
        if let Some(p) = &page {
            request = request.query(&[("page", p)]);
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("cost request failed: {}", redact(&e.without_url().to_string()));
                return Outcome::Offline;
            }
        };
        let status = response.status();
        match status.as_u16() {
            401 | 403 => return Outcome::Unauthorized,
            429 => return Outcome::RateLimited(crate::api::retry_after(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()))),
            500..=599 => return Outcome::Offline,
            _ => {}
        }
        let body = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!("reading cost response: {}", redact(&e.without_url().to_string()));
                return Outcome::Offline;
            }
        };
        let parsed = status.is_success().then(|| serde_json::from_slice::<Value>(&body).ok()).flatten().and_then(|json| parse(&json));
        let Some((cents, next)) = parsed else { return Outcome::Unrecognised(status.as_u16()) };
        total += cents;
        match next {
            Some(p) => page = Some(p),
            None => return Outcome::Spent(total),
        }
    }
    tracing::warn!("cost report: more than {MAX_PAGES} pages for one cycle");
    Outcome::Unrecognised(200)
}

/// The cents in one page of the cost report and the next page's token,
/// or `None` if the shape isn't the report's.
pub fn parse(json: &Value) -> Option<(i64, Option<String>)> {
    // Summed before rounding: amounts carry fractions of a cent.
    let mut cents = 0f64;
    for bucket in json.get("data")?.as_array()? {
        for r in bucket.get("results")?.as_array()? {
            if r.get("currency").and_then(Value::as_str).is_some_and(|c| c != "USD") {
                return None;
            }
            cents += amount(r.get("amount")?)?;
        }
    }
    let more = json.get("has_more").and_then(Value::as_bool).unwrap_or(false);
    let next = if more { json.get("next_page").and_then(Value::as_str).map(str::to_owned) } else { None };
    Some((cents.round() as i64, next))
}

/// A cents amount: a decimal string ("1240.500000"), or a number.
fn amount(v: &Value) -> Option<f64> {
    let x = match v {
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    x.is_finite().then_some(x)
}

// ---- the live view ----------------------------------------------------------------

/// What the row and chunk show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub spent_cents: i64,
    pub limit_cents: i64,
    pub cycle: Cycle,
}

impl Live {
    /// Percent of the credit spent (can pass 100).
    pub fn used(&self) -> f32 {
        if self.limit_cents <= 0 {
            return 0.0;
        }
        (self.spent_cents as f64 / self.limit_cents as f64 * 100.0).max(0.0) as f32
    }

    pub fn level(&self) -> Level {
        Level::of(self.used())
    }

    /// Used, or 100 − used in Left mode, both within 0–100.
    pub fn shown(&self, left: bool) -> f32 {
        let used = self.used().min(100.0);
        if left { 100.0 - used } else { used }
    }

    pub fn left_cents(&self) -> i64 {
        (self.limit_cents - self.spent_cents).max(0)
    }

    pub fn pace(&self, now: DateTime<Utc>) -> Option<Pace> {
        pace(self.cycle.length(), self.used(), self.cycle.end, now)
    }
}

// ---- keyring ----------------------------------------------------------------------

fn attributes() -> [(&'static str, &'static str); 2] {
    [("application", APP_ID), ("kind", "admin-key")]
}

/// The saved key, if there is one.
pub async fn load() -> Option<AdminKey> {
    match from_keyring().await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("keyring unavailable: {e}");
            None
        }
    }
}

#[expect(clippy::result_large_err, reason = "oo7's own error type")]
async fn from_keyring() -> oo7::Result<Option<AdminKey>> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    let Some(item) = keyring.search_items(&attributes()).await?.into_iter().next() else { return Ok(None) };
    if item.is_locked().await? {
        item.unlock().await?;
    }
    let secret = item.secret().await?;
    Ok(std::str::from_utf8(secret.as_bytes()).ok().and_then(AdminKey::new))
}

/// Saves the key, replacing any earlier one. `false` if there's no keyring.
pub async fn save(key: &AdminKey) -> bool {
    let r: oo7::Result<()> = async {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        keyring.create_item(LABEL, &attributes(), key.expose(), true).await
    }
    .await;
    match r {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("saving the Admin key to the keyring: {e}");
            false
        }
    }
}

/// Removes the saved key.
pub async fn forget() {
    let r: oo7::Result<()> = async {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        keyring.delete(&attributes()).await
    }
    .await;
    if let Err(e) = r {
        tracing::warn!("removing the Admin key from the keyring: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn fixture(name: &str) -> Value {
        let p = format!("{}/tests/fixtures/cost/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn cycle_boundaries() {
        let c = |day, now| Cycle::at(day, at(now));
        // Before the renew day: the cycle started last month.
        assert_eq!(c(28, "2026-11-01T06:00:00Z"), Cycle { start: at("2026-10-28T00:00:00Z"), end: at("2026-11-28T00:00:00Z") });
        // On and after it: this month.
        assert_eq!(c(28, "2026-11-28T00:00:00Z").start, at("2026-11-28T00:00:00Z"));
        assert_eq!(c(28, "2026-11-30T23:59:59Z").end, at("2026-12-28T00:00:00Z"));
        // Day 31 clamps to the month's length on both ends.
        assert_eq!(c(31, "2026-12-01T00:00:00Z"), Cycle { start: at("2026-11-30T00:00:00Z"), end: at("2026-12-31T00:00:00Z") });
        assert_eq!(c(31, "2027-02-10T00:00:00Z"), Cycle { start: at("2027-01-31T00:00:00Z"), end: at("2027-02-28T00:00:00Z") });
        assert_eq!(c(29, "2028-02-29T12:00:00Z").start, at("2028-02-29T00:00:00Z"));
        // Year boundary.
        assert_eq!(c(15, "2027-01-03T00:00:00Z"), Cycle { start: at("2026-12-15T00:00:00Z"), end: at("2027-01-15T00:00:00Z") });
        // Day 1, and out-of-range days clamp.
        assert_eq!(c(1, "2026-11-01T00:00:00Z"), Cycle { start: at("2026-11-01T00:00:00Z"), end: at("2026-12-01T00:00:00Z") });
        assert_eq!(c(0, "2026-11-01T00:00:00Z"), c(1, "2026-11-01T00:00:00Z"));
        assert_eq!(c(99, "2026-11-01T00:00:00Z"), c(31, "2026-11-01T00:00:00Z"));
        // The end is always ahead of now.
        for day in 1..=31 {
            for now in ["2026-02-28T23:59:59Z", "2026-03-01T00:00:00Z", "2026-12-31T23:59:59Z", "2027-01-31T00:00:00Z"] {
                let cy = c(day, now);
                assert!(cy.start <= at(now) && at(now) < cy.end, "day {day} at {now}: {cy:?}");
            }
        }
    }

    #[test]
    fn parse_fixtures() {
        assert_eq!(parse(&fixture("normal")), Some((3720, None)));
        assert_eq!(parse(&fixture("paged")), Some((100, Some("page_MjAyNi0xMC0yOQ==".into()))));
        // An empty report is $0, not an error.
        assert_eq!(parse(&serde_json::json!({ "data": [], "has_more": false })), Some((0, None)));
        // has_more without a token ends the read.
        assert_eq!(parse(&serde_json::json!({ "data": [], "has_more": true, "next_page": null })), Some((0, None)));
        // Numbers, and rounding after the sum (two halves make one cent, not two).
        assert_eq!(parse(&serde_json::json!({ "data": [{ "results": [{ "amount": 12.4 }, { "amount": "0.6" }] }] })), Some((13, None)));
        assert_eq!(parse(&serde_json::json!({ "data": [{ "results": [{ "amount": "0.5" }, { "amount": "0.5" }] }] })), Some((1, None)));
        // Not the report, another currency, or a bad amount.
        assert_eq!(parse(&serde_json::json!({ "windows": [] })), None);
        assert_eq!(parse(&serde_json::json!({ "data": [{ "results": [{ "currency": "EUR", "amount": "1" }] }] })), None);
        assert_eq!(parse(&serde_json::json!({ "data": [{ "results": [{ "amount": "lots" }] }] })), None);
        assert_eq!(parse(&serde_json::json!({ "data": [{ "results": [{ "currency": "USD" }] }] })), None);
    }

    #[test]
    fn live_view() {
        let now = at("2026-11-01T06:00:00Z");
        let l = Live { spent_cents: 3720, limit_cents: 20000, cycle: Cycle::at(28, now) };
        assert!((l.used() - 18.6).abs() < 1e-4);
        assert_eq!((l.level(), l.left_cents()), (Level::Normal, 16280));
        assert!((l.shown(true) - 81.4).abs() < 1e-4);
        // 4 days 6 h into a 31-day cycle: 13.7% expected, 5% ahead.
        let p = l.pace(now).unwrap();
        assert!((p.expected - 13.709).abs() < 0.01, "{}", p.expected);
        assert_eq!(p.text(), "5% ahead of pace");
        // Over the credit: level Limit, nothing left, the bar full.
        let over = Live { spent_cents: 20510, ..l };
        assert_eq!((over.level(), over.left_cents(), over.shown(false), over.shown(true)), (Level::Limit, 0, 100.0, 0.0));
        assert_eq!(Live { limit_cents: 0, ..l }.used(), 0.0);
    }

    #[test]
    fn key_shape_and_redaction() {
        assert!(looks_valid("  sk-ant-admin01-abc "));
        assert!(looks_valid("sk-ant-usr-abc"));
        assert!(!looks_valid("ghp_abc"));
        assert_eq!(AdminKey::new("  "), None);
        let k = AdminKey::new(" sk-ant-admin01-abc ").unwrap();
        assert_eq!(k.expose(), "sk-ant-admin01-abc");
        assert!(!format!("{k:?}").contains("sk-ant"));
    }

    #[tokio::test]
    #[ignore = "uses the session keyring"]
    async fn keyring_roundtrip() {
        let k = AdminKey::new("sk-ant-admin01-test").unwrap();
        assert!(save(&k).await);
        assert_eq!(load().await, Some(k));
        forget().await;
        assert_eq!(load().await, None);
    }
}
