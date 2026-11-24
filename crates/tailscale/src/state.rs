//! The applet's state apart from the window: what the daemon last said,
//! what the user has asked for and is waiting on, and which page is open.
//! `App` feeds it events and draws from it.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::api::{ApiError, BusEvent, MaskedPrefs, Prefs, Suggestion};
use crate::config::CopyWhat;
use crate::model::{self, Device, Link, Snapshot};

/// A request with no answer after this long is given up on.
pub const BUSY_LIMIT: Duration = Duration::from_secs(20);
/// A sign-in URL is only opened this soon after the user asked to sign in.
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
/// Health warnings listed before "+N more".
const HEALTH_SHOWN: usize = 3;
/// A list longer than this gets a search field.
const SEARCH_OVER: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    ExitNode,
    Settings,
}

/// The request in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Switch,
    Exit,
}

/// A notice above the popup's content, in the order they're shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banner {
    /// tailscaled isn't on its socket.
    NotRunning {
        command: &'static str,
    },
    /// This user may not change settings; the command grants it.
    Operator {
        command: String,
    },
    SignIn,
    KeyExpired,
    /// Days until this device's key expires.
    KeyExpiring(i64),
    /// The last request failed.
    Error(String),
    Health(String),
    /// Further health warnings, not listed.
    HealthMore(usize),
}

/// What the panel tooltip adds to the state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub exit: Option<String>,
    pub online: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub snap: Snapshot,
    /// The local user, for the operator check.
    pub user: String,
    pub uid: u32,
    pub page: Page,
    pub busy: Option<(Busy, Instant)>,
    /// A write was refused since the last prefs.
    denied: bool,
    pub last_error: Option<String>,
    pub query: String,
    pub exit_query: String,
    /// The daemon's recommended exit node, while the exit page is open.
    pub suggestion: Option<Suggestion>,
    pub login_requested: Option<Instant>,
    /// The exit node asked for and not yet confirmed.
    wanted_exit: Option<String>,
}

impl State {
    /// Until the bus speaks the applet shows "connecting", not an error.
    pub fn new(user: String, uid: u32) -> Self {
        Self {
            snap: Snapshot { reachable: true, state: None, prefs: Prefs { want_running: true, ..Prefs::default() }, ..Snapshot::default() },
            user,
            uid,
            page: Page::Main,
            busy: None,
            denied: false,
            last_error: None,
            query: String::new(),
            exit_query: String::new(),
            suggestion: None,
            login_requested: None,
            wanted_exit: None,
        }
    }

    /// Takes a bus event in; true when the status should be fetched again.
    pub fn apply(&mut self, event: BusEvent) -> bool {
        let n = match event {
            BusEvent::Down => {
                self.snap = Snapshot { reachable: false, prefs: std::mem::take(&mut self.snap.prefs), ..Snapshot::default() };
                self.busy = None;
                self.wanted_exit = None;
                return false;
            }
            BusEvent::Notify(n) => n,
        };
        self.snap.reachable = true;
        if let Some(state) = n.state {
            self.snap.state = Some(state);
            if self.busy.is_some_and(|b| b.0 == Busy::Switch) {
                self.busy = None;
            }
        }
        if let Some(prefs) = n.prefs {
            self.set_prefs(prefs);
        }
        if let Some(health) = n.health {
            self.snap.health = health.warnings;
        }
        if let Some(e) = n.err_message.filter(|e| !e.is_empty()) {
            self.last_error = Some(e);
        }
        true
    }

    fn set_prefs(&mut self, prefs: Prefs) {
        if model::can_write(&prefs.operator_user, &self.user, self.uid) {
            self.denied = false;
        }
        if self.wanted_exit.as_ref() == Some(&prefs.exit_node_id) {
            self.wanted_exit = None;
            if self.busy.is_some_and(|b| b.0 == Busy::Exit) {
                self.busy = None;
            }
        }
        self.snap.prefs = prefs;
    }

    /// The reply to a prefs write.
    pub fn on_patch(&mut self, reply: Result<Prefs, ApiError>) {
        self.busy = None;
        self.wanted_exit = None;
        match reply {
            Ok(prefs) => {
                self.last_error = None;
                self.set_prefs(prefs);
            }
            Err(ApiError::Denied) => self.denied = true,
            // The bus reports the daemon going away.
            Err(ApiError::Unavailable) => {}
            Err(ApiError::Other(code, text)) => self.last_error = Some(format!("{code} {text}").trim().to_owned()),
        }
    }

    pub fn busy_expired(&self, now: Instant) -> bool {
        self.busy.is_some_and(|(_, since)| now.duration_since(since) >= BUSY_LIMIT)
    }

    /// Whether a URL from the bus is the answer to a sign-in asked for here.
    pub fn should_open(&self, now: Instant) -> bool {
        self.login_requested.is_some_and(|t| now.duration_since(t) <= LOGIN_WINDOW)
    }

    pub fn can_write(&self) -> bool {
        !self.denied && model::can_write(&self.snap.prefs.operator_user, &self.user, self.uid)
    }

    /// The on/off switch: the write to send.
    pub fn switch(&mut self, on: bool, now: Instant) -> MaskedPrefs {
        self.busy = Some((Busy::Switch, now));
        MaskedPrefs::want_running(on)
    }

    /// Picks an exit node (`""` for none) and returns to the main page.
    pub fn select_exit(&mut self, id: &str, now: Instant) -> MaskedPrefs {
        self.busy = Some((Busy::Exit, now));
        self.wanted_exit = Some(id.to_owned());
        self.open(Page::Main);
        MaskedPrefs::exit_node(id)
    }

    /// Changes page; searches, the suggestion and the last error don't follow.
    pub fn open(&mut self, page: Page) {
        self.page = page;
        self.query.clear();
        self.exit_query.clear();
        self.suggestion = None;
        self.last_error = None;
    }

    /// The suggested exit node's ID and name, unless it's the one in use.
    pub fn recommended(&self) -> Option<(&str, String)> {
        let s = self.suggestion.as_ref().filter(|s| !s.id.is_empty() && s.id != self.snap.prefs.exit_node_id)?;
        let label = s.name.split('.').next().filter(|l| !l.is_empty()).unwrap_or(&s.id);
        Some((&s.id, label.to_owned()))
    }

    pub fn lan_enabled(&self) -> bool {
        self.can_write() && !self.snap.prefs.exit_node_id.is_empty()
    }

    pub fn show_search(&self) -> bool {
        model::online_count(&self.snap.status).1 > SEARCH_OVER
    }

    pub fn show_exit_search(&self) -> bool {
        model::exit_count(&self.snap.status) > SEARCH_OVER
    }

    pub fn summary(&self) -> Summary {
        let (online, total) = model::online_count(&self.snap.status);
        Summary { exit: model::exit_name(&self.snap).map(|e| e.0), online, total }
    }

    pub fn banners(&self, now: DateTime<Utc>) -> Vec<Banner> {
        let link = model::link(&self.snap);
        if link == Link::Unavailable {
            return vec![Banner::NotRunning { command: "sudo systemctl enable --now tailscaled" }];
        }
        let mut v = Vec::new();
        // Before the first prefs arrive there is nothing to judge the operator by.
        if self.snap.state.is_some() && !self.can_write() {
            v.push(Banner::Operator { command: format!("sudo tailscale set --operator={}", self.user) });
        }
        if link == Link::NeedsLogin {
            v.push(Banner::SignIn);
        }
        match model::key_days_left(&self.snap, now) {
            Some(d) if d < 0 => v.push(Banner::KeyExpired),
            Some(d) => v.push(Banner::KeyExpiring(d)),
            None => {}
        }
        if let Some(e) = &self.last_error {
            v.push(Banner::Error(e.clone()));
        }
        // The bus's warnings when it has sent any, else the status reply's.
        let health: Vec<&str> = if self.snap.health.is_empty() {
            self.snap.status.health.iter().map(String::as_str).collect()
        } else {
            self.snap.health.iter().map(|w| w.text.as_str()).collect()
        };
        let health: Vec<&str> = health.into_iter().filter(|t| !t.is_empty()).collect();
        v.extend(health.iter().take(HEALTH_SHOWN).map(|t| Banner::Health((*t).to_owned())));
        if health.len() > HEALTH_SHOWN {
            v.push(Banner::HealthMore(health.len() - HEALTH_SHOWN));
        }
        v
    }
}

/// What clicking a device row copies, if it has it.
pub fn copy_text(d: &Device, what: CopyWhat) -> Option<String> {
    match what {
        CopyWhat::Address => d.ip.clone(),
        CopyWhat::Name => d.dns.clone(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::api::{Notify, Status};

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
    }

    fn prefs(operator: &str) -> Prefs {
        Prefs { want_running: true, operator_user: operator.into(), ..Prefs::default() }
    }

    fn notify(state: u8, operator: &str) -> BusEvent {
        BusEvent::Notify(Notify { state: Some(state), prefs: Some(prefs(operator)), ..Notify::default() })
    }

    /// Connected as `daniel`, who is the operator, with the running fixture.
    fn connected() -> State {
        let mut s = State::new("daniel".into(), 1000);
        assert!(s.apply(notify(6, "daniel")));
        s.snap.status = serde_json::from_str(&fixture("status_running.json")).unwrap();
        s
    }

    #[test]
    fn starts_connecting_not_in_error() {
        let s = State::new("daniel".into(), 1000);
        assert_eq!(model::link(&s.snap), Link::Connecting);
        assert!(s.banners(now()).is_empty());
    }

    #[test]
    fn unavailable_then_back() {
        let mut s = connected();
        s.busy = Some((Busy::Switch, Instant::now()));
        assert!(!s.apply(BusEvent::Down));
        assert_eq!(model::link(&s.snap), Link::Unavailable);
        assert_eq!(s.snap.status, Status::default());
        assert_eq!(s.busy, None);
        assert!(s.apply(notify(6, "daniel")));
        assert_eq!(model::link(&s.snap), Link::Connected);
    }

    #[test]
    fn partial_notify_keeps_the_rest() {
        let mut s = connected();
        s.apply(BusEvent::Notify(Notify { state: Some(4), ..Notify::default() }));
        assert_eq!(model::link(&s.snap), Link::Off);
        assert_eq!(s.snap.prefs.operator_user, "daniel");
        let h: Notify = serde_json::from_str(fixture("bus_health.ndjson").lines().next().unwrap()).unwrap();
        s.apply(BusEvent::Notify(Notify { health: h.health, ..Notify::default() }));
        assert_eq!(s.snap.health.len(), 2);
        assert_eq!(s.snap.state, Some(4));
    }

    #[test]
    fn denied_raises_banner() {
        let mut s = connected();
        let _ = s.switch(false, Instant::now());
        assert_eq!(s.busy.map(|b| b.0), Some(Busy::Switch));
        s.on_patch(Err(ApiError::Denied));
        assert_eq!(s.busy, None);
        assert!(!s.can_write());
        assert_eq!(s.banners(now())[0], Banner::Operator { command: "sudo tailscale set --operator=daniel".into() });
        // Prefs naming this user as operator lift it again.
        s.apply(notify(6, "daniel"));
        assert!(s.can_write());
    }

    #[test]
    fn busy_clears_on_confirm_reply_or_error() {
        let mut s = connected();
        assert_eq!(s.switch(false, Instant::now()), MaskedPrefs::want_running(false));
        s.apply(BusEvent::Notify(Notify { state: Some(4), ..Notify::default() }));
        assert_eq!(s.busy, None);

        let _ = s.switch(true, Instant::now());
        s.on_patch(Ok(prefs("daniel")));
        assert_eq!(s.busy, None);
        assert!(s.snap.prefs.want_running);

        let _ = s.switch(false, Instant::now());
        s.on_patch(Err(ApiError::Other(500, "boom".into())));
        assert_eq!(s.busy, None);
        assert_eq!(s.last_error.as_deref(), Some("500 boom"));
        assert_eq!(s.banners(now())[0], Banner::Error("500 boom".into()));
        s.on_patch(Ok(prefs("daniel")));
        assert_eq!(s.last_error, None);
    }

    #[test]
    fn busy_times_out() {
        let mut s = connected();
        let start = Instant::now();
        let _ = s.switch(false, start);
        assert!(!s.busy_expired(start + Duration::from_secs(19)));
        assert!(s.busy_expired(start + Duration::from_secs(21)));
        s.busy = None;
        assert!(!s.busy_expired(start + Duration::from_secs(21)));
    }

    #[test]
    fn browse_only_when_asked() {
        let mut s = connected();
        let t = Instant::now();
        assert!(!s.should_open(t));
        s.login_requested = Some(t);
        assert!(s.should_open(t + Duration::from_secs(59)));
        assert!(!s.should_open(t + Duration::from_secs(61)));
    }

    #[test]
    fn banner_order() {
        let mut down = connected();
        down.apply(BusEvent::Down);
        assert_eq!(down.banners(now()), [Banner::NotRunning { command: "sudo systemctl enable --now tailscaled" }]);

        let mut login = State::new("daniel".into(), 1000);
        login.apply(notify(2, ""));
        assert_eq!(login.banners(now()), [Banner::Operator { command: "sudo tailscale set --operator=daniel".into() }, Banner::SignIn]);
        login.apply(notify(2, "daniel"));
        assert_eq!(login.banners(now()), [Banner::SignIn]);

        let mut s = connected();
        s.apply(notify(6, ""));
        s.snap.status.self_node.as_mut().unwrap().key_expiry = Some(now() + chrono::Duration::days(10));
        s.snap.status.health = (1..=5).map(|i| format!("warning {i}")).collect();
        assert_eq!(
            s.banners(now()),
            [
                Banner::Operator { command: "sudo tailscale set --operator=daniel".into() },
                Banner::KeyExpiring(10),
                Banner::Health("warning 1".into()),
                Banner::Health("warning 2".into()),
                Banner::Health("warning 3".into()),
                Banner::HealthMore(2),
            ]
        );
        s.snap.status.self_node.as_mut().unwrap().key_expiry = Some(now() - chrono::Duration::days(2));
        assert_eq!(s.banners(now())[1], Banner::KeyExpired);
    }

    #[test]
    fn bus_health_text_wins_over_status_strings() {
        let mut s = connected();
        let h: Notify = serde_json::from_str(fixture("bus_health.ndjson").lines().next().unwrap()).unwrap();
        s.apply(BusEvent::Notify(Notify { health: h.health, ..Notify::default() }));
        assert_eq!(
            s.banners(now()),
            [
                Banner::Health("Some peers are advertising routes but --accept-routes is false".into()),
                Banner::Health("Tailscale could not connect to any relay server.".into())
            ]
        );
    }

    #[test]
    fn root_needs_no_operator() {
        let mut s = State::new("root".into(), 0);
        s.apply(notify(6, ""));
        assert!(s.can_write());
        assert!(s.banners(now()).is_empty());
    }

    #[test]
    fn copy_target() {
        let s = connected();
        let d = &model::devices(&s.snap.status, "bravo", true)[0];
        assert_eq!(copy_text(d, CopyWhat::Address).as_deref(), Some("100.64.0.2"));
        assert_eq!(copy_text(d, CopyWhat::Name).as_deref(), Some("bravo.example.ts.net"));
        let bare = Device { id: "x".into(), name: "x".into(), os: String::new(), ip: None, dns: None, online: false, conn: model::Conn::LastSeen(None) };
        assert_eq!(copy_text(&bare, CopyWhat::Address), None);
        assert_eq!(copy_text(&bare, CopyWhat::Name), None);
    }

    #[test]
    fn search_threshold() {
        let mut s = connected();
        assert!(!s.show_search());
        let extra = s.snap.status.peers[0].clone();
        s.snap.status.peers.extend(std::iter::repeat_n(extra, 3));
        assert!(s.show_search(), "9 devices");
        assert!(!s.show_exit_search());
        s.snap.status = serde_json::from_str(&fixture("status_mullvad.json")).unwrap();
        assert!(!s.show_exit_search(), "7 exit nodes");
    }

    #[test]
    fn exit_select_sets_busy_and_page() {
        let mut s = connected();
        s.page = Page::ExitNode;
        s.exit_query = "br".into();
        assert_eq!(s.select_exit("nAAAA2CNTRL", Instant::now()), MaskedPrefs::exit_node("nAAAA2CNTRL"));
        assert_eq!(s.busy.map(|b| b.0), Some(Busy::Exit));
        assert_eq!(s.page, Page::Main);
        assert_eq!(s.exit_query, "");
    }

    #[test]
    fn exit_busy_clears_on_matching_prefs() {
        let mut s = connected();
        let _ = s.select_exit("nAAAA2CNTRL", Instant::now());
        s.apply(BusEvent::Notify(Notify { prefs: Some(Prefs { exit_node_id: "nAAAA7CNTRL".into(), ..prefs("daniel") }), ..Notify::default() }));
        assert_eq!(s.busy.map(|b| b.0), Some(Busy::Exit));
        s.apply(BusEvent::Notify(Notify { prefs: Some(Prefs { exit_node_id: "nAAAA2CNTRL".into(), ..prefs("daniel") }), ..Notify::default() }));
        assert_eq!(s.busy, None);
    }

    #[test]
    fn recommended_hidden_when_selected() {
        let mut s = connected();
        assert_eq!(s.recommended(), None);
        s.suggestion = Some(Suggestion { id: "nAAAA2CNTRL".into(), name: "bravo.example.ts.net.".into() });
        assert_eq!(s.recommended().map(|r| r.0), Some("nAAAA2CNTRL"));
        assert_eq!(s.recommended().map(|r| r.1), Some("bravo".to_owned()));
        s.snap.prefs.exit_node_id = "nAAAA2CNTRL".into();
        assert_eq!(s.recommended(), None);
    }

    #[test]
    fn lan_toggle_enabled() {
        let mut s = connected();
        assert!(!s.lan_enabled(), "no exit node");
        s.snap.prefs.exit_node_id = "nAAAA2CNTRL".into();
        assert!(s.lan_enabled());
        s.on_patch(Err(ApiError::Denied));
        assert!(!s.lan_enabled(), "not the operator");
    }

    #[test]
    fn leaving_a_page_clears_its_search() {
        let mut s = connected();
        s.open(Page::ExitNode);
        s.exit_query = "syd".into();
        s.suggestion = Some(Suggestion::default());
        s.last_error = Some("old".into());
        s.open(Page::Main);
        assert_eq!((s.exit_query.as_str(), s.suggestion.is_none(), s.last_error.is_none()), ("", true, true));
    }

    #[test]
    fn tooltip_parts() {
        let mut s = connected();
        assert_eq!(s.summary(), Summary { exit: None, online: 3, total: 6 });
        s.snap.prefs.exit_node_id = "nAAAA2CNTRL".into();
        assert_eq!(s.summary().exit.as_deref(), Some("bravo"));
    }
}
