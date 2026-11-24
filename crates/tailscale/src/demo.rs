//! `TAILSCALE_DEMO=1` (feature `demo`): no daemon; the applet steps through
//! every state every 8 s, built from the test fixtures.

use chrono::{DateTime, TimeZone, Utc};

use crate::api::{BusEvent, Notify, Prefs, Status};
use crate::state::{Page, State};

pub const STEP: std::time::Duration = std::time::Duration::from_secs(8);
pub const SCENES: usize = 12;
/// The demo's local user.
const USER: &str = "alex";

pub fn enabled() -> bool {
    std::env::var_os("TAILSCALE_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

/// `TAILSCALE_DEMO_SCENE=n` starts on scene `n`.
pub fn first() -> usize {
    std::env::var("TAILSCALE_DEMO_SCENE").ok().and_then(|s| s.parse().ok()).unwrap_or(0) % SCENES
}

/// `TAILSCALE_SHOT=path`: save the window as a PAM image after the first frames, then exit.
pub fn shot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("TAILSCALE_SHOT").filter(|v| !v.is_empty()).map(Into::into)
}

pub fn save_shot(path: &std::path::Path, shot: &cosmic::iced::window::Screenshot) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", shot.size.width, shot.size.height)?;
    f.write_all(&shot.rgba)?;
    f.flush()
}

/// The fixtures' "now": their last-seen times are written against it.
pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
}

pub struct Scene {
    pub name: &'static str,
    pub state: State,
}

const RUNNING: &str = include_str!("../tests/fixtures/status_running.json");
const STOPPED: &str = include_str!("../tests/fixtures/status_stopped.json");
const NEEDS_LOGIN: &str = include_str!("../tests/fixtures/status_needs_login.json");
const EXIT_NODE: &str = include_str!("../tests/fixtures/status_exit_node.json");
const MULLVAD: &str = include_str!("../tests/fixtures/status_mullvad.json");
const HEALTH: &str = include_str!("../tests/fixtures/bus_health.ndjson");

fn status(json: &str) -> Status {
    serde_json::from_str(json).expect("fixture parses")
}

/// A state as the bus and one status fetch would leave it.
fn state(bus_state: u8, operator: &str, exit: &str, status_json: &str) -> State {
    let mut s = State::new(USER.to_owned(), 1000);
    let prefs = Prefs { want_running: bus_state != 4, operator_user: operator.to_owned(), exit_node_id: exit.to_owned(), ..Prefs::default() };
    s.apply(BusEvent::Notify(Notify { state: Some(bus_state), prefs: Some(prefs), ..Notify::default() }));
    s.snap.status = status(status_json);
    s
}

pub fn scene(i: usize) -> Scene {
    let connected = || state(6, USER, "", RUNNING);
    let with_exit = |json| state(6, USER, "nAAAA2CNTRL", json);
    let (name, state) = match i % SCENES {
        0 => ("connected", connected()),
        1 => ("exit node", with_exit(EXIT_NODE)),
        2 => {
            let mut s = with_exit(EXIT_NODE);
            s.page = Page::ExitNode;
            ("exit page", s)
        }
        3 => {
            let mut s = state(6, USER, "", MULLVAD);
            s.page = Page::ExitNode;
            ("exit page, by country", s)
        }
        4 => ("connecting", state(5, USER, "", STOPPED)),
        5 => ("off", state(4, USER, "", STOPPED)),
        6 => ("operator needed", state(6, "", "", RUNNING)),
        7 => ("sign-in needed", state(2, USER, "", NEEDS_LOGIN)),
        8 => {
            let mut s = connected();
            s.apply(BusEvent::Down);
            ("not running", s)
        }
        9 => {
            let mut s = connected();
            let n: Notify = serde_json::from_str(HEALTH.lines().next().unwrap_or_default()).expect("fixture parses");
            s.apply(BusEvent::Notify(Notify { health: n.health, ..Notify::default() }));
            ("health error", s)
        }
        10 => {
            let mut s = connected();
            if let Some(me) = s.snap.status.self_node.as_mut() {
                me.key_expiry = Some(now() + chrono::Duration::days(5) + chrono::Duration::hours(1));
            }
            ("key expiring", s)
        }
        _ => {
            let mut s = connected();
            s.page = Page::Settings;
            ("settings", s)
        }
    };
    Scene { name, state }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{self, Link};
    use crate::state::Page;

    #[test]
    fn every_scene_builds() {
        let links: Vec<Link> = (0..SCENES).map(|i| model::link(&scene(i).state.snap)).collect();
        use Link::*;
        assert_eq!(links, [Connected, Connected, Connected, Connected, Connecting, Off, Connected, NeedsLogin, Unavailable, Connected, Connected, Connected]);
        assert_eq!(scene(SCENES + 2).name, scene(2).name, "scene numbers wrap");
    }

    #[test]
    fn scenes_show_what_they_say() {
        let at = |name: &str| (0..SCENES).map(scene).find(|s| s.name == name).unwrap().state;
        assert_eq!(at("exit page").page, Page::ExitNode);
        assert_eq!(at("exit page, by country").page, Page::ExitNode);
        assert_eq!(at("settings").page, Page::Settings);
        assert!(!at("operator needed").can_write());
        assert!(at("connected").can_write());
        assert_eq!(model::word(&at("health error").snap, now()).0, "panel-error");
        assert_eq!(model::key_days_left(&at("key expiring").snap, now()), Some(5));
        assert_eq!(model::exit_name(&at("exit node").snap), Some(("bravo".to_owned(), true)));
    }
}
