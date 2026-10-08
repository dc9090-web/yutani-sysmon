//! `AI_USAGE_DEMO=1` (feature `demo`): no login and no network; the
//! applet steps through every state every 10 s, built from the fixtures.

use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;

use crate::api::{self, Diagnostics};
use crate::auth::{Account, NoLogin};
use crate::model::{Snapshot, State, Usage};

pub const STEP: std::time::Duration = std::time::Duration::from_secs(10);

/// `AI_USAGE_DEMO_SCENE=n` starts on scene `n`.
pub fn first() -> usize {
    std::env::var("AI_USAGE_DEMO_SCENE").ok().and_then(|s| s.parse().ok()).unwrap_or(0) % SCENES
}

/// `AI_USAGE_SHOT=path`: after the first frames, save the window (RGBA,
/// as a PAM image) to `path` and exit. For README screenshots.
pub fn shot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("AI_USAGE_SHOT").filter(|v| !v.is_empty()).map(Into::into)
}

/// Writes a screenshot as a PAM image (RGBA, no encoder needed).
pub fn save_shot(path: &std::path::Path, shot: &cosmic::iced::window::Screenshot) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", shot.size.width, shot.size.height)?;
    f.write_all(&shot.rgba)?;
    f.flush()
}

pub fn enabled() -> bool {
    std::env::var_os("AI_USAGE_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

fn json(name: &str) -> Value {
    let s = match name {
        "normal_with_fable" => include_str!("../tests/fixtures/usage/normal_with_fable.json"),
        "no_fable_limit" => include_str!("../tests/fixtures/usage/no_fable_limit.json"),
        "session_limit_reached" => include_str!("../tests/fixtures/usage/session_limit_reached.json"),
        "legacy_no_limits_array" => include_str!("../tests/fixtures/usage/legacy_no_limits_array.json"),
        _ => include_str!("../tests/fixtures/usage/unknown_shape.json"),
    };
    serde_json::from_str(s).expect("fixtures are valid JSON")
}

/// A fixture with its reset times moved so that its "now"
/// (2026-10-08T10:00:00Z) is the real now.
fn usage(name: &str, now: DateTime<Utc>) -> Usage {
    let fixture_now = DateTime::parse_from_rfc3339("2026-10-08T10:00:00Z").expect("valid").with_timezone(&Utc);
    let mut u = api::parse(&json(name)).expect("fixture parses");
    for w in &mut u.windows {
        w.resets_at = w.resets_at.map(|t| now + (t - fixture_now));
    }
    u
}

pub struct Scene {
    pub account: Account,
    pub state: State,
    pub snapshot: Option<Snapshot>,
}

pub const SCENES: usize = 10;

pub fn scene(i: usize, now: DateTime<Utc>) -> Scene {
    let max = Account { plan: Some("Max".into()), email: Some("dc@example.com".into()) };
    let snap = |name: &str, age_min: i64| Some(Snapshot { usage: usage(name, now), fetched_at: now - TimeDelta::minutes(age_min) });
    let (account, state, snapshot) = match i % SCENES {
        0 => (max, State::Normal, snap("normal_with_fable", 2)),
        1 => (Account { plan: Some("Pro".into()), ..max }, State::Normal, snap("no_fable_limit", 0)),
        2 => (max, State::Normal, snap("session_limit_reached", 1)),
        3 => (max, State::Normal, snap("legacy_no_limits_array", 0)),
        4 => (max, State::Offline, snap("normal_with_fable", 25)),
        5 => (max, State::RateLimited(now + TimeDelta::minutes(4)), snap("normal_with_fable", 6)),
        6 => (max, State::Expired, snap("normal_with_fable", 180)),
        7 => (Account::default(), State::NotSignedIn(NoLogin::Missing), None),
        8 => (Account::default(), State::NotSignedIn(NoLogin::NoProfileScope), None),
        _ => (max, State::Unrecognised(Diagnostics { status: 200, keys: api::key_paths(&json("unknown_shape")) }), None),
    };
    Scene { account, state, snapshot }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_builds() {
        let now = Utc::now();
        let names: Vec<_> = (0..SCENES).map(|i| scene(i, now).state.name()).collect();
        for s in ["normal", "offline", "rate-limited", "login-expired", "not-signed-in", "format-not-recognised"] {
            assert!(names.contains(&s), "{s} missing from the demo");
        }
        // Reset times are moved to the real clock.
        let s = scene(0, now).snapshot.unwrap();
        let session = s.usage.windows[0].resets_at.unwrap();
        assert_eq!(session - now, TimeDelta::minutes(2 * 60 + 13));
    }
}
