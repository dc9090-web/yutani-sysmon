//! When to fetch (SPEC §5): the panel city on the interval with ±10%
//! jitter, offline backoff, `Retry-After`, the manual-refresh debounce, and
//! the daily call budget.

use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

/// Manual refresh stays disabled this long after a request completes.
pub const DEBOUNCE: TimeDelta = TimeDelta::seconds(30);
/// Opening the popup refetches the viewed city if its data is older.
pub const STALE_ON_OPEN: TimeDelta = TimeDelta::minutes(10);
/// Scheduled and automatic refreshes stop at this many calls in a UTC day.
pub const SOFT_CAP: u32 = 900;
/// Manual refreshes stop here: OpenWeather's free allowance.
pub const HARD_CAP: u32 = 1000;

/// How a refresh of the panel city ended, as far as timing goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Result {
    Ok,
    Offline,
    RateLimited(Duration),
    /// The key is missing or not accepted: wait for a new key.
    Halt,
}

/// Plans the panel city's refreshes.
#[derive(Debug, Default)]
pub struct Scheduler {
    /// Offline failures in a row.
    failures: u32,
    /// No requests before this (`Retry-After`).
    pub paused_until: Option<DateTime<Utc>>,
    /// The next scheduled refresh, if one is planned.
    pub due: Option<DateTime<Utc>>,
    /// When the last panel refresh succeeded.
    pub ok_at: Option<DateTime<Utc>>,
}

impl Scheduler {
    /// Records a finished panel refresh and plans the next one. `jitter` is
    /// in −1…1 and scales the ±10% spread.
    pub fn completed(&mut self, result: Result, now: DateTime<Utc>, interval: TimeDelta, jitter: f64) -> Option<DateTime<Utc>> {
        self.due = match result {
            Result::Ok => {
                self.failures = 0;
                self.ok_at = Some(now);
                Some(now + jittered(interval, jitter))
            }
            Result::Offline => {
                self.failures += 1;
                Some(now + backoff(self.failures, interval))
            }
            Result::RateLimited(wait) => {
                let until = now + TimeDelta::from_std(wait).unwrap_or(TimeDelta::minutes(10));
                self.paused_until = Some(until);
                Some(until)
            }
            Result::Halt => {
                self.failures = 0;
                None
            }
        };
        self.due
    }

    /// Plans a refresh at `at` (startup, a new key, a budget reset).
    pub fn plan(&mut self, at: DateTime<Utc>) -> DateTime<Utc> {
        let at = self.paused_until.map_or(at, |p| at.max(p));
        self.due = Some(at);
        at
    }

    /// The interval changed: count it from the last good refresh.
    pub fn retime(&mut self, now: DateTime<Utc>, interval: TimeDelta, jitter: f64) -> Option<DateTime<Utc>> {
        if self.failures == 0
            && !self.paused(now)
            && let (Some(ok), Some(_)) = (self.ok_at, self.due)
        {
            self.due = Some((ok + jittered(interval, jitter)).max(now));
        }
        self.due
    }

    /// A 429 pause is in force.
    pub fn paused(&self, now: DateTime<Utc>) -> bool {
        self.paused_until.is_some_and(|p| p > now)
    }
}

/// Calls made in one UTC day. Persisted with the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub day: NaiveDate,
    pub calls: u32,
}

impl Budget {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { day: now.date_naive(), calls: 0 }
    }

    /// Starts a new count at 00:00 UTC.
    fn roll(&mut self, now: DateTime<Utc>) {
        if now.date_naive() != self.day {
            *self = Self::new(now);
        }
    }

    pub fn add(&mut self, calls: u32, now: DateTime<Utc>) {
        self.roll(now);
        self.calls += calls;
    }

    /// Calls so far today.
    pub fn today(&self, now: DateTime<Utc>) -> u32 {
        if now.date_naive() == self.day { self.calls } else { 0 }
    }

    /// Scheduled and automatic refreshes are allowed.
    pub fn automatic_ok(&self, now: DateTime<Utc>) -> bool {
        self.today(now) < SOFT_CAP
    }

    /// The refresh button is allowed.
    pub fn manual_ok(&self, now: DateTime<Utc>) -> bool {
        self.today(now) < HARD_CAP
    }

    /// The next 00:00 UTC.
    pub fn resets_at(now: DateTime<Utc>) -> DateTime<Utc> {
        (now.date_naive() + TimeDelta::days(1)).and_hms_opt(0, 0, 0).unwrap_or_default().and_utc()
    }
}

/// `interval` spread by ±10%.
fn jittered(interval: TimeDelta, jitter: f64) -> TimeDelta {
    let ms = interval.num_milliseconds() as f64 * (1.0 + 0.1 * jitter.clamp(-1.0, 1.0));
    TimeDelta::milliseconds(ms.round() as i64)
}

/// 1, 2, 4… minutes, capped at the interval.
fn backoff(failures: u32, interval: TimeDelta) -> TimeDelta {
    let minutes = 1i64 << failures.saturating_sub(1).min(16);
    TimeDelta::minutes(minutes).min(interval)
}

/// A random value in −1…1, from the standard library's per-process seed.
pub fn jitter() -> f64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    (h.finish() as f64 / u64::MAX as f64) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-08T10:00:00Z").unwrap().with_timezone(&Utc)
    }
    const THIRTY: TimeDelta = TimeDelta::minutes(30);

    #[test]
    fn interval_with_jitter() {
        let mut s = Scheduler::default();
        assert_eq!(s.completed(Result::Ok, t0(), THIRTY, 0.0), Some(t0() + THIRTY));
        assert_eq!(s.completed(Result::Ok, t0(), THIRTY, 1.0), Some(t0() + TimeDelta::minutes(33)));
        assert_eq!(s.completed(Result::Ok, t0(), THIRTY, -1.0), Some(t0() + TimeDelta::minutes(27)));
        for _ in 0..100 {
            assert!((-1.0..=1.0).contains(&jitter()));
        }
    }

    #[test]
    fn offline_backoff() {
        let mut s = Scheduler::default();
        let mins: Vec<i64> = (0..7).map(|_| (s.completed(Result::Offline, t0(), THIRTY, 0.0).unwrap() - t0()).num_minutes()).collect();
        assert_eq!(mins, [1, 2, 4, 8, 16, 30, 30]);
        s.completed(Result::Ok, t0(), THIRTY, 0.0);
        assert_eq!(s.completed(Result::Offline, t0(), THIRTY, 0.0), Some(t0() + TimeDelta::minutes(1)));
    }

    #[test]
    fn rate_limit_pauses() {
        let mut s = Scheduler::default();
        let until = t0() + TimeDelta::minutes(10);
        assert_eq!(s.completed(Result::RateLimited(Duration::from_secs(600)), t0(), THIRTY, 0.0), Some(until));
        assert!(s.paused(t0() + TimeDelta::minutes(9)));
        assert!(!s.paused(until));
        // A plan made during the pause waits for it.
        assert_eq!(s.plan(t0()), until);
    }

    #[test]
    fn halt_and_retime() {
        let mut s = Scheduler::default();
        assert_eq!(s.completed(Result::Halt, t0(), THIRTY, 0.0), None);
        assert_eq!(s.retime(t0(), TimeDelta::minutes(15), 0.0), None);
        s.completed(Result::Ok, t0(), TimeDelta::minutes(60), 0.0);
        let now = t0() + TimeDelta::minutes(20);
        assert_eq!(s.retime(now, TimeDelta::minutes(15), 0.0), Some(now));
        assert_eq!(s.retime(now, THIRTY, 0.0), Some(t0() + THIRTY));
    }

    #[test]
    fn daily_budget() {
        let mut b = Budget::new(t0());
        b.add(899, t0());
        assert!(b.automatic_ok(t0()));
        b.add(1, t0());
        assert!(!b.automatic_ok(t0()) && b.manual_ok(t0()));
        b.add(100, t0());
        assert!(!b.manual_ok(t0()));
        // A new UTC day starts over.
        let midnight = Budget::resets_at(t0());
        assert_eq!(midnight, DateTime::parse_from_rfc3339("2026-10-09T00:00:00Z").unwrap().with_timezone(&Utc));
        assert_eq!(b.today(midnight - TimeDelta::seconds(1)), 1000);
        assert_eq!(b.today(midnight), 0);
        assert!(b.automatic_ok(midnight));
        b.add(4, midnight);
        assert_eq!((b.day, b.calls), (midnight.date_naive(), 4));
    }

    #[test]
    fn default_interval_stays_well_under_budget() {
        // ACCEPTANCE D-05: 30 min ±10% × 4 calls stays under 220 a day.
        let worst = (24.0 * 60.0 / 27.0_f64).ceil() as u32 * 4;
        assert!(worst <= 220, "{worst}");
    }
}
