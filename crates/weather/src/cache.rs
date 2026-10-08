//! The disk cache (SPEC §5): the last good snapshot per location at
//! `$XDG_CACHE_HOME/<APP_ID>/<location-id>.json`, plus the day's call count.
//! Weather data only, never the key. All I/O runs off the UI thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};

use crate::config::APP_ID;
use crate::model::Snapshot;
use crate::scheduler::Budget;

/// Cached data older than this is discarded.
pub const MAX_AGE: TimeDelta = TimeDelta::hours(12);

const BUDGET_FILE: &str = "calls.json";

pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join(APP_ID))
}

/// A location id as a file name: UUIDs pass; anything else is refused.
fn file_name(id: &str) -> Option<String> {
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).then(|| format!("{id}.json"))
}

/// What was on disk at startup.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub snapshots: HashMap<String, Snapshot>,
    pub budget: Option<Budget>,
}

/// Reads the snapshots for `ids` (dropping any older than 12 h) and the call count.
pub async fn load(dir: PathBuf, ids: Vec<String>, now: DateTime<Utc>) -> Loaded {
    tokio::task::spawn_blocking(move || load_sync(&dir, &ids, now)).await.unwrap_or_default()
}

fn load_sync(dir: &Path, ids: &[String], now: DateTime<Utc>) -> Loaded {
    let mut out = Loaded::default();
    for id in ids {
        let Some(name) = file_name(id) else { continue };
        let path = dir.join(name);
        let Ok(text) = std::fs::read(&path) else { continue };
        match serde_json::from_slice::<Snapshot>(&text) {
            Ok(s) if now - s.fetched_at <= MAX_AGE => {
                out.snapshots.insert(id.clone(), s);
            }
            Ok(_) => {
                tracing::debug!(id, "cached data over 12 h old, discarded");
                let _ = std::fs::remove_file(&path);
            }
            Err(e) => tracing::debug!(id, "unreadable cache file: {e}"),
        }
    }
    out.budget = std::fs::read(dir.join(BUDGET_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    out
}

/// Writes one location's snapshot.
pub async fn save(dir: PathBuf, id: String, snapshot: Snapshot) {
    let Some(name) = file_name(&id) else { return };
    let r = tokio::task::spawn_blocking(move || match serde_json::to_vec(&snapshot) {
        Ok(json) => write_atomic(&dir, &name, &json),
        Err(e) => Err(std::io::Error::other(e)),
    })
    .await;
    if let Ok(Err(e)) = r {
        tracing::warn!("writing the cache: {e}");
    }
}

/// Writes the day's call count.
pub async fn save_budget(dir: PathBuf, budget: Budget) {
    let r = tokio::task::spawn_blocking(move || match serde_json::to_vec(&budget) {
        Ok(json) => write_atomic(&dir, BUDGET_FILE, &json),
        Err(e) => Err(std::io::Error::other(e)),
    })
    .await;
    if let Ok(Err(e)) = r {
        tracing::warn!("writing the call count: {e}");
    }
}

/// Removes a deleted location's file.
pub async fn remove(dir: PathBuf, id: String) {
    if let Some(name) = file_name(&id) {
        let _ = tokio::fs::remove_file(dir.join(name)).await;
    }
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Units;
    use crate::model::{Current, Tier};

    fn snapshot(fetched_at: DateTime<Utc>) -> Snapshot {
        Snapshot {
            tier: Tier::Free,
            units: Units::Metric,
            tz_offset: 39600,
            current: Current { temp: 23.4, ..Default::default() },
            hourly: vec![],
            daily: vec![],
            alerts: vec![],
            fetched_at,
        }
    }

    #[test]
    fn round_trip_and_expiry() {
        let dir = std::env::temp_dir().join(format!("weather-cache-test-{}", std::process::id()));
        let now = Utc::now();
        let fresh = snapshot(now - TimeDelta::hours(11));
        let old = snapshot(now - TimeDelta::hours(13));
        write_atomic(&dir, "a-1.json", &serde_json::to_vec(&fresh).unwrap()).unwrap();
        write_atomic(&dir, "b-2.json", &serde_json::to_vec(&old).unwrap()).unwrap();
        let b = Budget { day: now.date_naive(), calls: 42 };
        write_atomic(&dir, BUDGET_FILE, &serde_json::to_vec(&b).unwrap()).unwrap();

        let ids = ["a-1", "b-2", "missing", "../escape"].map(String::from);
        let l = load_sync(&dir, &ids, now);
        assert_eq!(l.snapshots.get("a-1"), Some(&fresh));
        assert!(!l.snapshots.contains_key("b-2"));
        assert!(!dir.join("b-2.json").exists(), "old data is deleted");
        assert_eq!(l.budget, Some(b));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names() {
        assert_eq!(file_name("3f2a-9c"), Some("3f2a-9c.json".into()));
        assert_eq!(file_name("../x"), None);
        assert_eq!(file_name(""), None);
    }
}
