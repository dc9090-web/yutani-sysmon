//! When to fetch next (SPEC §5): the interval with jitter, backoff while
//! offline, `Retry-After`, wake-ups at window resets and login expiry, and
//! the manual-refresh debounce.

use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

/// Manual refresh stays disabled this long after a request completes.
pub const DEBOUNCE: TimeDelta = TimeDelta::seconds(10);
/// Opening the popup refetches data older than this.
pub const STALE_ON_OPEN: TimeDelta = TimeDelta::seconds(60);
/// Fetch this long after a window's reset time.
pub const AFTER_RESET: TimeDelta = TimeDelta::seconds(30);

/// How the last request ended, as far as timing goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Result {
    /// 200, recognised or not.
    Ok,
    Offline,
    RateLimited(Duration),
    /// Login missing or expired: wait for the credentials file to change.
    Halt,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    /// Offline failures in a row.
    failures: u32,
    /// When the last request finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// When the last 200 arrived.
    pub ok_at: Option<DateTime<Utc>>,
    /// No requests before this (`Retry-After`).
    pub paused_until: Option<DateTime<Utc>>,
    /// The next fetch, if one is planned.
    pub due: Option<DateTime<Utc>>,
}

impl Scheduler {
    /// Records a finished request and plans the next one.
    ///
    /// `jitter` is in −1…1 and scales the ±10% spread; `wakeups` are times
    /// worth an extra fetch (window resets + 30 s, login expiry).
    pub fn completed(
        &mut self,
        result: Result,
        now: DateTime<Utc>,
        interval: TimeDelta,
        jitter: f64,
        wakeups: impl IntoIterator<Item = DateTime<Utc>>,
    ) -> Option<DateTime<Utc>> {
        self.completed_at = Some(now);
        self.due = match result {
            Result::Ok => {
                self.failures = 0;
                self.paused_until = None;
                self.ok_at = Some(now);
                Some(earliest(now + jittered(interval, jitter), now, wakeups))
            }
            Result::Offline => {
                self.failures += 1;
                Some(now + backoff(self.failures, interval))
            }
            Result::RateLimited(wait) => {
                let until = now + TimeDelta::from_std(wait).unwrap_or(TimeDelta::minutes(5));
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

    /// The interval setting changed: count it from the last good fetch.
    pub fn retime(&mut self, now: DateTime<Utc>, interval: TimeDelta, jitter: f64) -> Option<DateTime<Utc>> {
        if self.failures == 0
            && self.paused_until.is_none_or(|p| p <= now)
            && let (Some(ok), Some(_)) = (self.ok_at, self.due)
        {
            self.due = Some((ok + jittered(interval, jitter)).max(now));
        }
        self.due
    }

    /// Whether the refresh button is enabled.
    pub fn can_refresh(&self, now: DateTime<Utc>, fetching: bool) -> bool {
        !fetching && self.completed_at.is_none_or(|t| now - t >= DEBOUNCE) && self.paused_until.is_none_or(|p| p <= now)
    }

    /// When the debounce or a `Retry-After` pause ends, for redrawing the button.
    pub fn unblocks_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let debounce = self.completed_at.map(|t| t + DEBOUNCE);
        [debounce, self.paused_until].into_iter().flatten().filter(|t| *t > now).max()
    }

    /// Whether opening the popup should refetch.
    pub fn stale_on_open(&self, now: DateTime<Utc>, fetching: bool) -> bool {
        !fetching && self.ok_at.is_none_or(|t| now - t > STALE_ON_OPEN) && self.paused_until.is_none_or(|p| p <= now)
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

/// The earliest of `base` and the wake-ups still ahead.
fn earliest(base: DateTime<Utc>, now: DateTime<Utc>, wakeups: impl IntoIterator<Item = DateTime<Utc>>) -> DateTime<Utc> {
    wakeups.into_iter().filter(|t| *t > now).fold(base, DateTime::min)
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
    const FIVE: TimeDelta = TimeDelta::minutes(5);

    #[test]
    fn interval_with_jitter() {
        let mut s = Scheduler::default();
        assert_eq!(s.completed(Result::Ok, t0(), FIVE, 0.0, []), Some(t0() + FIVE));
        assert_eq!(s.completed(Result::Ok, t0(), FIVE, 1.0, []), Some(t0() + TimeDelta::seconds(330)));
        assert_eq!(s.completed(Result::Ok, t0(), FIVE, -1.0, []), Some(t0() + TimeDelta::seconds(270)));
        for _ in 0..100 {
            let j = jitter();
            assert!((-1.0..=1.0).contains(&j));
        }
    }

    #[test]
    fn reset_wakeup_comes_first() {
        let mut s = Scheduler::default();
        let reset = t0() + TimeDelta::minutes(2) + AFTER_RESET;
        let past = t0() - TimeDelta::minutes(1);
        assert_eq!(s.completed(Result::Ok, t0(), FIVE, 0.0, [past, reset, t0() + TimeDelta::hours(1)]), Some(reset));
    }

    #[test]
    fn offline_backoff() {
        let mut s = Scheduler::default();
        let mins: Vec<i64> = (0..5).map(|_| (s.completed(Result::Offline, t0(), FIVE, 0.0, []).unwrap() - t0()).num_minutes()).collect();
        assert_eq!(mins, [1, 2, 4, 5, 5]);
        // A success resets the backoff.
        s.completed(Result::Ok, t0(), FIVE, 0.0, []);
        assert_eq!(s.completed(Result::Offline, t0(), FIVE, 0.0, []), Some(t0() + TimeDelta::minutes(1)));
        // With a 1-minute interval the cap is 1 minute.
        let mut s = Scheduler::default();
        s.completed(Result::Offline, t0(), TimeDelta::minutes(1), 0.0, []);
        assert_eq!(s.completed(Result::Offline, t0(), TimeDelta::minutes(1), 0.0, []), Some(t0() + TimeDelta::minutes(1)));
    }

    #[test]
    fn retry_after_pauses() {
        let mut s = Scheduler::default();
        let until = t0() + TimeDelta::seconds(120);
        assert_eq!(s.completed(Result::RateLimited(Duration::from_secs(120)), t0(), FIVE, 0.0, []), Some(until));
        let later = t0() + TimeDelta::seconds(119);
        assert!(!s.can_refresh(later, false));
        assert!(!s.stale_on_open(later, false));
        assert_eq!(s.unblocks_at(later), Some(until));
        assert!(s.can_refresh(until, false));
    }

    #[test]
    fn halt_stops_polling() {
        let mut s = Scheduler::default();
        assert_eq!(s.completed(Result::Halt, t0(), FIVE, 0.0, [t0() + TimeDelta::minutes(1)]), None);
        assert_eq!(s.retime(t0(), TimeDelta::minutes(1), 0.0), None);
    }

    #[test]
    fn debounce() {
        let mut s = Scheduler::default();
        assert!(s.can_refresh(t0(), false));
        assert!(!s.can_refresh(t0(), true));
        s.completed(Result::Ok, t0(), FIVE, 0.0, []);
        assert!(!s.can_refresh(t0() + TimeDelta::seconds(9), false));
        assert_eq!(s.unblocks_at(t0() + TimeDelta::seconds(9)), Some(t0() + DEBOUNCE));
        assert!(s.can_refresh(t0() + DEBOUNCE, false));
        assert_eq!(s.unblocks_at(t0() + DEBOUNCE), None);
    }

    #[test]
    fn popup_open() {
        let mut s = Scheduler::default();
        assert!(s.stale_on_open(t0(), false));
        s.completed(Result::Ok, t0(), FIVE, 0.0, []);
        assert!(!s.stale_on_open(t0() + TimeDelta::seconds(60), false));
        assert!(s.stale_on_open(t0() + TimeDelta::seconds(61), false));
        assert!(!s.stale_on_open(t0() + TimeDelta::seconds(61), true));
    }

    #[test]
    fn interval_change() {
        let mut s = Scheduler::default();
        s.completed(Result::Ok, t0(), TimeDelta::minutes(15), 0.0, []);
        // Two minutes later, switch to 1 min: due now.
        let now = t0() + TimeDelta::minutes(2);
        assert_eq!(s.retime(now, TimeDelta::minutes(1), 0.0), Some(now));
        // Switch to 5 min: five minutes after the last fetch.
        assert_eq!(s.retime(now, FIVE, 0.0), Some(t0() + FIVE));
    }
}
