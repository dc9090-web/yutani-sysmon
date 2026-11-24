//! `cosmic-applet-tailscale --status`: what the applet makes of the daemon
//! right now, as text. For checking a machine without a COSMIC session.
//! Prints no keys, URLs or account names.

use chrono::{DateTime, Utc};

use crate::model::{self, Dot, Link};
use crate::state::State;

pub fn report(st: &State, now: DateTime<Utc>) -> String {
    let snap = &st.snap;
    let link = model::link(snap);
    let dot = match model::dot(snap, now) {
        Dot::Green => "green",
        Dot::Orange => "orange",
        Dot::Red => "red",
    };
    let mut out = format!("link        {link:?}\npanel       {dot} dot\n");
    if link == Link::Unavailable {
        return out;
    }
    let operator = &snap.prefs.operator_user;
    let user = &st.user;
    out += &if st.can_write() {
        format!("operator    {user} (writes allowed)\n")
    } else if operator.is_empty() {
        format!("operator    not set for {user} (writes will be refused)\n")
    } else {
        format!("operator    set to another user, not {user} (writes will be refused)\n")
    };
    let tailnet = snap.status.tailnet.as_ref().map(|t| t.magic_dns_suffix.as_str()).filter(|s| !s.is_empty()).unwrap_or("unknown");
    out += &format!("tailnet     {tailnet}\n");
    if let Some(me) = &snap.status.self_node {
        let ip = me.ips.iter().find(|ip| !ip.contains(':')).map_or("no address", String::as_str);
        out += &format!("this device {}  {ip}\n", me.host_name);
    }
    out += &match model::exit_name(snap) {
        None => "exit node   none\n".to_owned(),
        Some((name, online)) => format!("exit node   {name} ({})\n", if online { "online" } else { "offline" }),
    };
    let (online, total) = model::online_count(&snap.status);
    out += &format!("devices     {online} of {total} online\n");
    let warnings = if snap.health.is_empty() { snap.status.health.len() } else { snap.health.len() };
    let affecting = snap.health.iter().filter(|w| w.impacts_connectivity).count();
    out += &format!("warnings    {warnings} ({affecting} affecting connectivity)\n");
    out
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::api::{BusEvent, Notify};

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn state(bus: &str, status: &str, user: &str) -> State {
        let mut s = State::new(user.into(), 1000);
        let n: Notify = serde_json::from_str(fixture(bus).lines().next().unwrap()).unwrap();
        s.apply(BusEvent::Notify(n));
        s.snap.status = serde_json::from_str(&fixture(status)).unwrap();
        s
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn status_report() {
        let s = state("bus_running.ndjson", "status_running.json", "daniel");
        let want = "\
link        Connected
panel       green dot
operator    not set for daniel (writes will be refused)
tailnet     example.ts.net
this device alpha  100.64.0.1
exit node   none
devices     3 of 6 online
warnings    1 (0 affecting connectivity)
";
        assert_eq!(report(&s, now()), want);
    }

    #[test]
    fn report_other_states() {
        let mut s = state("bus_running.ndjson", "status_exit_node.json", "daniel");
        s.snap.prefs.operator_user = "daniel".into();
        s.snap.prefs.exit_node_id = "nAAAA2CNTRL".into();
        let r = report(&s, now());
        assert!(r.contains("operator    daniel (writes allowed)\n"), "{r}");
        assert!(r.contains("exit node   bravo (online)\n"), "{r}");
        s.snap.prefs.operator_user = "bob".into();
        assert!(report(&s, now()).contains("operator    set to another user, not daniel (writes will be refused)\n"));

        let h = state("bus_health.ndjson", "status_running.json", "daniel");
        let r = report(&h, now());
        assert!(r.contains("panel       red dot\n"), "{r}");
        assert!(r.contains("warnings    2 (1 affecting connectivity)\n"), "{r}");

        let mut down = State::new("daniel".into(), 1000);
        down.apply(BusEvent::Down);
        assert_eq!(report(&down, now()), "link        Unavailable\npanel       red dot\n");
    }

    #[test]
    fn report_leaks_nothing_private() {
        let s = state("bus_running.ndjson", "status_running.json", "daniel");
        let r = report(&s, now());
        assert!(!r.contains("nodekey") && !r.contains('@') && !r.contains("http"), "{r}");
    }
}
