//! What the UI shows, derived from the daemon's state, prefs, health and
//! status: the panel word and dot, the header caption, the device list and
//! the exit-node options. Pure functions; no I/O.

use chrono::{DateTime, Utc};

use crate::api::{Peer, Prefs, Status, Warning};

/// A node key this close to expiring gets a warning.
const KEY_WARN_DAYS: i64 = 14;

/// Everything known about the daemon right now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// The event bus is connected.
    pub reachable: bool,
    /// `ipn.State` from the bus; `None` before its first message.
    pub state: Option<u8>,
    pub prefs: Prefs,
    pub health: Vec<Warning>,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// No daemon on the socket.
    Unavailable,
    NeedsLogin,
    /// Waiting for an admin to approve this device.
    NeedsApproval,
    /// Another local user owns the daemon.
    OtherUser,
    Off,
    Connecting,
    Connected,
}

impl Link {
    fn is_error(self) -> bool {
        matches!(self, Self::Unavailable | Self::NeedsLogin | Self::NeedsApproval | Self::OtherUser)
    }
}

/// How a state is coloured; always with a word too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Off,
    Busy,
    On,
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// Red dot, and the panel word becomes `ERR`.
    Error,
    /// Amber dot; the word stays.
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconKind {
    Off,
    Connecting,
    On,
    /// Connected and routing through an exit node.
    Exit,
}

pub fn link(s: &Snapshot) -> Link {
    if !s.reachable {
        return Link::Unavailable;
    }
    match s.state {
        Some(1) => Link::OtherUser,
        Some(2) => Link::NeedsLogin,
        Some(3) => Link::NeedsApproval,
        Some(4) => Link::Off,
        Some(6) => Link::Connected,
        Some(5) => Link::Connecting,
        // No state yet, or one this version doesn't know.
        _ if s.prefs.want_running => Link::Connecting,
        _ => Link::Off,
    }
}

/// Whole days until this device's key expires (negative once it has), when
/// that is within the warning window.
pub fn key_days_left(s: &Snapshot, now: DateTime<Utc>) -> Option<i64> {
    let expiry = s.status.self_node.as_ref()?.key_expiry?;
    let days = (expiry - now).num_seconds().div_euclid(86_400);
    (days <= KEY_WARN_DAYS).then_some(days)
}

/// The selected exit node's peer, once the status lists it.
fn exit_peer(s: &Snapshot) -> Option<&Peer> {
    let id = &s.prefs.exit_node_id;
    if id.is_empty() { None } else { s.status.peers.iter().find(|p| &p.id == id) }
}

/// The selected exit node's name and whether it's online. An ID the status
/// doesn't list is shown as the ID, offline.
pub fn exit_name(s: &Snapshot) -> Option<(String, bool)> {
    let id = &s.prefs.exit_node_id;
    if id.is_empty() {
        return None;
    }
    Some(exit_peer(s).map_or_else(|| (id.clone(), false), |p| (peer_name(p), p.online)))
}

pub fn issue(s: &Snapshot, now: DateTime<Utc>) -> Option<Issue> {
    let link = link(s);
    if link.is_error() {
        return Some(Issue::Error);
    }
    if link != Link::Connected {
        return None;
    }
    let days = key_days_left(s, now);
    if days.is_some_and(|d| d < 0) || s.health.iter().any(|w| w.impacts_connectivity) {
        return Some(Issue::Error);
    }
    if days.is_some() || exit_peer(s).is_some_and(|p| !p.online) {
        return Some(Issue::Warning);
    }
    None
}

/// The panel word's `.ftl` key and its tone.
pub fn word(s: &Snapshot, now: DateTime<Utc>) -> (&'static str, Tone) {
    if issue(s, now) == Some(Issue::Error) {
        return ("panel-error", Tone::Err);
    }
    match link(s) {
        Link::Off => ("panel-off", Tone::Off),
        Link::Connecting => ("panel-connecting", Tone::Busy),
        _ => ("panel-on", Tone::On),
    }
}

/// Whether the word is in the panel: always, unless the user hid it and it
/// would only say `ON` or `OFF` (connecting and errors always show).
pub fn show_chunk(s: &Snapshot, now: DateTime<Utc>, show_status: bool) -> bool {
    show_status || !matches!(word(s, now).1, Tone::On | Tone::Off)
}

pub fn icon(s: &Snapshot) -> IconKind {
    match link(s) {
        Link::Connected if !s.prefs.exit_node_id.is_empty() => IconKind::Exit,
        Link::Connected => IconKind::On,
        Link::Connecting => IconKind::Connecting,
        _ => IconKind::Off,
    }
}

/// The header caption's `.ftl` key and tone.
pub fn header(s: &Snapshot) -> (&'static str, Option<Tone>) {
    match link(s) {
        Link::Connected => ("head-connected", Some(Tone::On)),
        Link::Connecting => ("head-connecting", Some(Tone::Busy)),
        Link::Off => ("head-off", None),
        Link::NeedsLogin => ("head-login", Some(Tone::Err)),
        Link::NeedsApproval => ("head-approval", Some(Tone::Err)),
        Link::OtherUser => ("head-other-user", Some(Tone::Err)),
        Link::Unavailable => ("head-unavailable", Some(Tone::Err)),
    }
}

/// Whether tailscaled lets this user change prefs: root, or its operator.
pub fn can_write(operator_user: &str, user: &str, uid: u32) -> bool {
    uid == 0 || (!user.is_empty() && operator_user == user)
}

/// The first label of the MagicDNS name, else the host name, else the ID.
fn peer_name(p: &Peer) -> String {
    let label = p.dns_name.split('.').next().unwrap_or_default();
    [label, &p.host_name, &p.id].into_iter().find(|s| !s.is_empty()).unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conn {
    Direct,
    /// Through another device on the tailnet.
    PeerRelay,
    /// Through a DERP relay, by region code.
    Relay(String),
    /// Online, no traffic lately.
    Idle,
    /// Offline; when it was last seen, if known.
    LastSeen(Option<DateTime<Utc>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub os: String,
    /// Its first IPv4 address.
    pub ip: Option<String>,
    /// Its MagicDNS name, without the trailing dot.
    pub dns: Option<String>,
    pub online: bool,
    pub conn: Conn,
}

fn device(p: &Peer) -> Device {
    let conn = if !p.online {
        Conn::LastSeen(p.last_seen)
    } else if !p.active {
        Conn::Idle
    } else if !p.cur_addr.is_empty() {
        Conn::Direct
    } else if !p.peer_relay.is_empty() {
        Conn::PeerRelay
    } else if !p.relay.is_empty() {
        Conn::Relay(p.relay.clone())
    } else {
        Conn::Idle
    };
    let dns = p.dns_name.trim_end_matches('.');
    Device {
        id: p.id.clone(),
        name: peer_name(p),
        os: p.os.clone(),
        ip: p.ips.iter().find(|ip| !ip.contains(':')).cloned(),
        dns: (!dns.is_empty()).then(|| dns.to_owned()),
        online: p.online,
        conn,
    }
}

/// The tailnet's own devices; location-based (Mullvad) exit nodes aren't devices.
fn own(st: &Status) -> impl Iterator<Item = &Peer> {
    st.peers.iter().filter(|p| p.location.is_none())
}

/// Online first, then by name.
fn by_presence_then_name(a: (bool, &str), b: (bool, &str)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
}

/// The device list. `query` matches the name or any address, ignoring case.
pub fn devices(st: &Status, query: &str, show_offline: bool) -> Vec<Device> {
    let q = query.trim().to_lowercase();
    let mut list: Vec<Device> = own(st)
        .filter(|p| show_offline || p.online)
        .filter(|p| q.is_empty() || peer_name(p).to_lowercase().contains(&q) || p.dns_name.to_lowercase().contains(&q) || p.ips.iter().any(|ip| ip.contains(&q)))
        .map(device)
        .collect();
    list.sort_by(|a, b| by_presence_then_name((a.online, &a.name), (b.online, &b.name)));
    list
}

/// `(online, total)` of the tailnet's own devices, this one not counted.
pub fn online_count(st: &Status) -> (usize, usize) {
    (own(st).filter(|p| p.online).count(), own(st).count())
}

/// "5m ago", "3h ago", "6d ago".
pub fn last_seen(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let sec = (now - t).num_seconds();
    match sec {
        ..60 => "just now".to_owned(),
        60..3600 => format!("{}m ago", sec / 60),
        3600..86_400 => format!("{}h ago", sec / 3600),
        _ => format!("{}d ago", sec / 86_400),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitOption {
    pub id: String,
    pub name: String,
    /// The city, for location-based nodes.
    pub caption: Option<String>,
    pub online: bool,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitGroup {
    /// The country; `None` for the tailnet's own exit nodes, which come first.
    pub heading: Option<String>,
    pub options: Vec<ExitOption>,
}

/// How many exit nodes there are to choose from.
pub fn exit_count(st: &Status) -> usize {
    st.peers.iter().filter(|p| p.exit_node_option).count()
}

/// The exit nodes on offer: the tailnet's own, then location-based ones by
/// country. `query` matches the name, city or country, ignoring case.
pub fn exit_groups(st: &Status, query: &str) -> Vec<ExitGroup> {
    let q = query.trim().to_lowercase();
    let mut groups: std::collections::BTreeMap<Option<String>, Vec<ExitOption>> = std::collections::BTreeMap::new();
    for p in st.peers.iter().filter(|p| p.exit_node_option) {
        let name = peer_name(p);
        let loc = p.location.as_ref();
        let hit = q.is_empty() || name.to_lowercase().contains(&q) || loc.is_some_and(|l| l.city.to_lowercase().contains(&q) || l.country.to_lowercase().contains(&q));
        if hit {
            let option = ExitOption { id: p.id.clone(), name, caption: loc.map(|l| l.city.clone()), online: p.online, last_seen: p.last_seen };
            groups.entry(loc.map(|l| l.country.clone())).or_default().push(option);
        }
    }
    // `None` sorts before every `Some`, so the tailnet's own nodes lead.
    groups
        .into_iter()
        .map(|(heading, mut options)| {
            options.sort_by(|a, b| {
                by_presence_then_name((a.online, a.caption.as_deref().unwrap_or(&a.name)), (b.online, b.caption.as_deref().unwrap_or(&b.name))).then_with(|| a.name.cmp(&b.name))
            });
            ExitGroup { heading, options }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn status(name: &str) -> Status {
        serde_json::from_str(&fixture(name)).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
    }

    /// Connected, with the running fixture.
    fn snap() -> Snapshot {
        Snapshot { reachable: true, state: Some(6), prefs: Prefs { want_running: true, ..Prefs::default() }, health: Vec::new(), status: status("status_running.json") }
    }

    fn with_state(state: Option<u8>) -> Snapshot {
        Snapshot { state, ..snap() }
    }

    fn warning(impacts: bool) -> Warning {
        Warning { code: "w".into(), title: "T".into(), text: "text".into(), impacts_connectivity: impacts }
    }

    fn key_in(s: &mut Snapshot, d: chrono::Duration) {
        s.status.self_node.as_mut().unwrap().key_expiry = Some(now() + d);
    }

    #[test]
    fn links() {
        assert_eq!(link(&Snapshot { reachable: false, ..snap() }), Link::Unavailable);
        assert_eq!(link(&with_state(Some(1))), Link::OtherUser);
        assert_eq!(link(&with_state(Some(2))), Link::NeedsLogin);
        assert_eq!(link(&with_state(Some(3))), Link::NeedsApproval);
        assert_eq!(link(&with_state(Some(4))), Link::Off);
        assert_eq!(link(&with_state(Some(5))), Link::Connecting);
        assert_eq!(link(&with_state(Some(6))), Link::Connected);
        assert_eq!(link(&with_state(Some(0))), Link::Connecting);
        assert_eq!(link(&with_state(None)), Link::Connecting);
        let mut off = with_state(None);
        off.prefs.want_running = false;
        assert_eq!(link(&off), Link::Off);
        assert_eq!(link(&with_state(Some(42))), Link::Connecting);
    }

    #[test]
    fn words_and_dots() {
        let s = snap();
        assert_eq!(word(&s, now()), ("panel-on", Tone::On));
        assert_eq!(issue(&s, now()), None);

        let minor = Snapshot { health: vec![warning(false)], ..snap() };
        assert_eq!(issue(&minor, now()), None);
        assert_eq!(word(&minor, now()), ("panel-on", Tone::On));

        let broken = Snapshot { health: vec![warning(false), warning(true)], ..snap() };
        assert_eq!(issue(&broken, now()), Some(Issue::Error));
        assert_eq!(word(&broken, now()), ("panel-error", Tone::Err));

        let mut expired = snap();
        key_in(&mut expired, chrono::Duration::hours(-1));
        assert_eq!(word(&expired, now()), ("panel-error", Tone::Err));
        assert_eq!(key_days_left(&expired, now()), Some(-1));

        let mut expiring = snap();
        key_in(&mut expiring, chrono::Duration::days(10));
        assert_eq!(issue(&expiring, now()), Some(Issue::Warning));
        assert_eq!(word(&expiring, now()), ("panel-on", Tone::On));
        assert_eq!(key_days_left(&expiring, now()), Some(10));

        let mut far = snap();
        key_in(&mut far, chrono::Duration::days(90));
        assert_eq!((issue(&far, now()), key_days_left(&far, now())), (None, None));

        for state in [1, 2, 3] {
            assert_eq!(word(&with_state(Some(state)), now()), ("panel-error", Tone::Err));
        }
        assert_eq!(word(&Snapshot { reachable: false, ..snap() }, now()), ("panel-error", Tone::Err));
        assert_eq!(word(&with_state(Some(4)), now()), ("panel-off", Tone::Off));
        assert_eq!(word(&with_state(Some(5)), now()), ("panel-connecting", Tone::Busy));
    }

    #[test]
    fn health_only_counts_while_connected() {
        // While off or starting, "not connected" warnings are expected, not an error.
        let off = Snapshot { state: Some(4), health: vec![warning(true)], ..snap() };
        assert_eq!(word(&off, now()), ("panel-off", Tone::Off));
        let starting = Snapshot { state: Some(5), health: vec![warning(true)], ..snap() };
        assert_eq!(word(&starting, now()), ("panel-connecting", Tone::Busy));
    }

    #[test]
    fn exit_node_dot_and_icon() {
        let mut s = Snapshot { status: status("status_exit_node.json"), ..snap() };
        s.prefs.exit_node_id = "nAAAA2CNTRL".into();
        assert_eq!(icon(&s), IconKind::Exit);
        assert_eq!(exit_name(&s), Some(("bravo".to_owned(), true)));
        assert_eq!(issue(&s, now()), None);

        s.prefs.exit_node_id = "nAAAA7CNTRL".into(); // golf: offline
        assert_eq!(exit_name(&s), Some(("golf".to_owned(), false)));
        assert_eq!(issue(&s, now()), Some(Issue::Warning));

        // Selected but not in the status yet (or ever): named by ID, no dot until it's known.
        s.prefs.exit_node_id = "nZZZZ9CNTRL".into();
        assert_eq!(exit_name(&s), Some(("nZZZZ9CNTRL".to_owned(), false)));
        assert_eq!(issue(&s, now()), None);

        assert_eq!(exit_name(&snap()), None);
        assert_eq!(icon(&snap()), IconKind::On);
        assert_eq!(icon(&with_state(Some(5))), IconKind::Connecting);
        assert_eq!(icon(&with_state(Some(4))), IconKind::Off);
        assert_eq!(icon(&with_state(Some(2))), IconKind::Off);
    }

    #[test]
    fn chunk_forced_back() {
        assert!(show_chunk(&snap(), now(), true));
        assert!(!show_chunk(&snap(), now(), false));
        assert!(!show_chunk(&with_state(Some(4)), now(), false));
        assert!(show_chunk(&with_state(Some(5)), now(), false));
        assert!(show_chunk(&with_state(Some(2)), now(), false));
        assert!(show_chunk(&Snapshot { reachable: false, ..snap() }, now(), false));
        assert!(show_chunk(&Snapshot { health: vec![warning(true)], ..snap() }, now(), false));
    }

    #[test]
    fn headers() {
        assert_eq!(header(&snap()), ("head-connected", Some(Tone::On)));
        assert_eq!(header(&with_state(Some(5))), ("head-connecting", Some(Tone::Busy)));
        assert_eq!(header(&with_state(Some(4))), ("head-off", None));
        assert_eq!(header(&with_state(Some(2))), ("head-login", Some(Tone::Err)));
        assert_eq!(header(&with_state(Some(3))), ("head-approval", Some(Tone::Err)));
        assert_eq!(header(&with_state(Some(1))), ("head-other-user", Some(Tone::Err)));
        assert_eq!(header(&Snapshot { reachable: false, ..snap() }), ("head-unavailable", Some(Tone::Err)));
    }

    #[test]
    fn operator() {
        assert!(can_write("daniel", "daniel", 1000));
        assert!(!can_write("", "daniel", 1000));
        assert!(!can_write("bob", "daniel", 1000));
        assert!(!can_write("", "", 1000));
        assert!(can_write("", "root", 0));
    }

    fn names(d: &[Device]) -> Vec<&str> {
        d.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn device_order_and_filter() {
        let st = status("status_running.json");
        assert_eq!(names(&devices(&st, "", true)), ["bravo", "charlie", "delta", "echo", "foxtrot", "golf"]);
        assert_eq!(names(&devices(&st, "BRAV", true)), ["bravo"]);
        assert_eq!(names(&devices(&st, " 100.64.0.3 ", true)), ["charlie"]);
        assert_eq!(names(&devices(&st, "", false)), ["bravo", "charlie", "delta"]);
        assert_eq!(online_count(&st), (3, 6));
        let d = &devices(&st, "", true)[0];
        assert_eq!((d.ip.as_deref(), d.dns.as_deref(), d.os.as_str()), (Some("100.64.0.2"), Some("bravo.example.ts.net"), "linux"));
    }

    #[test]
    fn online_sorts_before_offline_whatever_the_name() {
        let mut st = status("status_running.json");
        for p in &mut st.peers {
            if p.host_name == "golf" {
                p.online = true;
                p.dns_name = "Alpha2.example.ts.net.".into();
            }
        }
        assert_eq!(names(&devices(&st, "", true)), ["Alpha2", "bravo", "charlie", "delta", "echo", "foxtrot"]);
    }

    #[test]
    fn conn_text() {
        let d = devices(&status("status_running.json"), "", true);
        assert_eq!(d[0].conn, Conn::Direct);
        assert_eq!(d[1].conn, Conn::Relay("syd".into()));
        assert_eq!(d[2].conn, Conn::Idle);
        assert_eq!(d[3].conn, Conn::LastSeen(Some(Utc.with_ymd_and_hms(2026, 11, 18, 0, 0, 0).unwrap())));
        let mut st = status("status_running.json");
        for p in &mut st.peers {
            p.cur_addr.clear();
            p.peer_relay = "nRELAY".into();
        }
        assert_eq!(devices(&st, "charlie", true)[0].conn, Conn::PeerRelay);
    }

    #[test]
    fn peer_degenerate() {
        let st: Status = serde_json::from_str(
            r#"{"Peer":{"a":{"ID":"nAAAA9CNTRL","HostName":"hotel","DNSName":"","TailscaleIPs":[],"LastSeen":"0001-01-01T00:00:00Z","Online":false},
                        "b":{"ID":"nAAAA8CNTRL","TailscaleIPs":["fd7a:115c:a1e0::8"],"Online":true},
                        "c":{}}}"#,
        )
        .unwrap();
        let d = devices(&st, "", true);
        assert_eq!(names(&d), ["nAAAA8CNTRL", "", "hotel"]);
        let hotel = &d[2];
        assert_eq!((hotel.ip.clone(), hotel.dns.clone(), hotel.conn.clone()), (None, None, Conn::LastSeen(None)));
        assert_eq!(d[0].ip, None, "an IPv6-only peer has no IPv4 to copy");
    }

    #[test]
    fn mullvad_grouping() {
        let st = status("status_mullvad.json");
        assert_eq!(devices(&st, "", true).len(), 6);
        assert_eq!(online_count(&st), (3, 6));
        let g = exit_groups(&st, "");
        let headings: Vec<Option<&str>> = g.iter().map(|g| g.heading.as_deref()).collect();
        assert_eq!(headings, [None, Some("Australia"), Some("Switzerland")]);
        let own: Vec<(&str, bool)> = g[0].options.iter().map(|o| (o.name.as_str(), o.online)).collect();
        assert_eq!(own, [("bravo", true), ("golf", false)]);
        assert_eq!(g[0].options[0].caption, None);
        let au: Vec<Option<&str>> = g[1].options.iter().map(|o| o.caption.as_deref()).collect();
        assert_eq!(au, [Some("Melbourne"), Some("Perth"), Some("Sydney")]);
        let z = exit_groups(&st, "zur");
        assert_eq!((z.len(), z[0].options.len(), z[0].heading.as_deref()), (1, 1, Some("Switzerland")));
        assert_eq!(exit_groups(&st, "switz")[0].options.len(), 2);
        assert_eq!(exit_count(&st), 7);
        assert!(exit_groups(&status("status_needs_login.json"), "").is_empty());
    }

    #[test]
    fn last_seen_text() {
        let ago = |d: chrono::Duration| last_seen(now() - d, now());
        assert_eq!(ago(chrono::Duration::seconds(30)), "just now");
        assert_eq!(ago(chrono::Duration::minutes(5)), "5m ago");
        assert_eq!(ago(chrono::Duration::hours(3)), "3h ago");
        assert_eq!(ago(chrono::Duration::days(6)), "6d ago");
        assert_eq!(ago(chrono::Duration::seconds(-90)), "just now", "a clock ahead of ours");
    }
}
