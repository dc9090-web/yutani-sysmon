//! The parts of the local API's JSON the applet reads. Every field is
//! optional and unknown ones are ignored, so a Tailscale update that adds,
//! drops or renames a field degrades one reading, never the whole reply.

use chrono::{DateTime, Datelike, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Reads a field as `T`, or `T::default()` if it's null or has another shape.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).unwrap_or_default())
}

/// A Go timestamp; its zero value (year 1) means "never".
fn time<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
    let t: Option<DateTime<Utc>> = lenient(d)?;
    Ok(t.filter(|t| t.year() > 1))
}

/// The values of a JSON object, each read as `T`; entries of another shape are dropped.
fn values<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => m.into_iter().filter_map(|(_, v)| serde_json::from_value(v).ok()).collect(),
        _ => Vec::new(),
    })
}

/// `GET /localapi/v0/status`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Status {
    #[serde(default, rename = "BackendState", deserialize_with = "lenient")]
    pub backend_state: String,
    #[serde(default, rename = "Self", deserialize_with = "lenient")]
    pub self_node: Option<Peer>,
    #[serde(default, rename = "Peer", deserialize_with = "values")]
    pub peers: Vec<Peer>,
    #[serde(default, rename = "Health", deserialize_with = "lenient")]
    pub health: Vec<String>,
    #[serde(default, rename = "CurrentTailnet", deserialize_with = "lenient")]
    pub tailnet: Option<Tailnet>,
    #[serde(default, rename = "ExitNodeStatus", deserialize_with = "lenient")]
    pub exit_node: Option<ExitNodeStatus>,
    #[serde(default, rename = "HaveNodeKey", deserialize_with = "lenient")]
    pub have_node_key: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Tailnet {
    #[serde(default, rename = "Name", deserialize_with = "lenient")]
    pub name: String,
    #[serde(default, rename = "MagicDNSSuffix", deserialize_with = "lenient")]
    pub magic_dns_suffix: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ExitNodeStatus {
    #[serde(default, rename = "ID", deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, rename = "Online", deserialize_with = "lenient")]
    pub online: bool,
}

/// A device: this one (`Status::self_node`) or a peer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Peer {
    #[serde(default, rename = "ID", deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, rename = "HostName", deserialize_with = "lenient")]
    pub host_name: String,
    #[serde(default, rename = "DNSName", deserialize_with = "lenient")]
    pub dns_name: String,
    #[serde(default, rename = "OS", deserialize_with = "lenient")]
    pub os: String,
    #[serde(default, rename = "TailscaleIPs", deserialize_with = "lenient")]
    pub ips: Vec<String>,
    #[serde(default, rename = "CurAddr", deserialize_with = "lenient")]
    pub cur_addr: String,
    #[serde(default, rename = "Relay", deserialize_with = "lenient")]
    pub relay: String,
    #[serde(default, rename = "PeerRelay", deserialize_with = "lenient")]
    pub peer_relay: String,
    #[serde(default, rename = "LastSeen", deserialize_with = "time")]
    pub last_seen: Option<DateTime<Utc>>,
    #[serde(default, rename = "Online", deserialize_with = "lenient")]
    pub online: bool,
    #[serde(default, rename = "Active", deserialize_with = "lenient")]
    pub active: bool,
    #[serde(default, rename = "ExitNode", deserialize_with = "lenient")]
    pub exit_node: bool,
    #[serde(default, rename = "ExitNodeOption", deserialize_with = "lenient")]
    pub exit_node_option: bool,
    #[serde(default, rename = "Location", deserialize_with = "lenient")]
    pub location: Option<Location>,
    #[serde(default, rename = "KeyExpiry", deserialize_with = "time")]
    pub key_expiry: Option<DateTime<Utc>>,
}

/// Set on location-based (Mullvad) exit nodes.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Location {
    #[serde(default, rename = "Country", deserialize_with = "lenient")]
    pub country: String,
    #[serde(default, rename = "CountryCode", deserialize_with = "lenient")]
    pub country_code: String,
    #[serde(default, rename = "City", deserialize_with = "lenient")]
    pub city: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Prefs {
    #[serde(default, rename = "WantRunning", deserialize_with = "lenient")]
    pub want_running: bool,
    #[serde(default, rename = "ExitNodeID", deserialize_with = "lenient")]
    pub exit_node_id: String,
    #[serde(default, rename = "ExitNodeAllowLANAccess", deserialize_with = "lenient")]
    pub exit_node_allow_lan: bool,
    #[serde(default, rename = "OperatorUser", deserialize_with = "lenient")]
    pub operator_user: String,
    #[serde(default, rename = "LoggedOut", deserialize_with = "lenient")]
    pub logged_out: bool,
}

/// One line of `watch-ipn-bus`; only what changed is set.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Notify {
    #[serde(default, rename = "State", deserialize_with = "lenient")]
    pub state: Option<u8>,
    #[serde(default, rename = "Prefs", deserialize_with = "lenient")]
    pub prefs: Option<Prefs>,
    #[serde(default, rename = "Health", deserialize_with = "lenient")]
    pub health: Option<Health>,
    #[serde(default, rename = "BrowseToURL", deserialize_with = "lenient")]
    pub browse_to_url: Option<String>,
    #[serde(default, rename = "ErrMessage", deserialize_with = "lenient")]
    pub err_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Health {
    /// Sorted by code, so the order is stable between messages.
    pub warnings: Vec<Warning>,
}

impl<'de> Deserialize<'de> for Health {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default, rename = "Warnings", deserialize_with = "values")]
            warnings: Vec<Warning>,
        }
        let mut warnings = Raw::deserialize(d)?.warnings;
        warnings.sort_by(|a, b| a.code.cmp(&b.code));
        Ok(Self { warnings })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Warning {
    #[serde(default, rename = "WarnableCode", deserialize_with = "lenient")]
    pub code: String,
    #[serde(default, rename = "Title", deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, rename = "Text", deserialize_with = "lenient")]
    pub text: String,
    #[serde(default, rename = "ImpactsConnectivity", deserialize_with = "lenient")]
    pub impacts_connectivity: bool,
}

/// `GET /localapi/v0/suggest-exit-node`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Suggestion {
    #[serde(default, rename = "ID", deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, rename = "Name", deserialize_with = "lenient")]
    pub name: String,
}

/// The body of `PATCH /localapi/v0/prefs`: each changed field with its `…Set` flag.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MaskedPrefs {
    #[serde(rename = "WantRunning", skip_serializing_if = "Option::is_none")]
    want_running: Option<bool>,
    #[serde(rename = "WantRunningSet", skip_serializing_if = "Option::is_none")]
    want_running_set: Option<bool>,
    #[serde(rename = "ExitNodeID", skip_serializing_if = "Option::is_none")]
    exit_node_id: Option<String>,
    #[serde(rename = "ExitNodeIDSet", skip_serializing_if = "Option::is_none")]
    exit_node_id_set: Option<bool>,
    #[serde(rename = "ExitNodeIP", skip_serializing_if = "Option::is_none")]
    exit_node_ip: Option<String>,
    #[serde(rename = "ExitNodeIPSet", skip_serializing_if = "Option::is_none")]
    exit_node_ip_set: Option<bool>,
    #[serde(rename = "ExitNodeAllowLANAccess", skip_serializing_if = "Option::is_none")]
    allow_lan: Option<bool>,
    #[serde(rename = "ExitNodeAllowLANAccessSet", skip_serializing_if = "Option::is_none")]
    allow_lan_set: Option<bool>,
}

impl MaskedPrefs {
    pub fn want_running(on: bool) -> Self {
        Self { want_running: Some(on), want_running_set: Some(true), ..Self::default() }
    }

    /// `""` clears the exit node. The IP form is cleared too, so the ID is the only selector.
    pub fn exit_node(id: &str) -> Self {
        Self {
            exit_node_id: Some(id.to_owned()),
            exit_node_id_set: Some(true),
            exit_node_ip: Some(String::new()),
            exit_node_ip_set: Some(true),
            ..Self::default()
        }
    }

    pub fn allow_lan(on: bool) -> Self {
        Self { allow_lan: Some(on), allow_lan_set: Some(true), ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx<T: serde::de::DeserializeOwned>(name: &str) -> T {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(text.lines().next().filter(|_| name.ends_with(".ndjson")).unwrap_or(&text)).unwrap()
    }

    #[test]
    fn parses_running() {
        let s: Status = fx("status_running.json");
        assert_eq!(s.backend_state, "Running");
        assert_eq!(s.peers.len(), 6);
        assert!(s.have_node_key);
        assert_eq!(s.self_node.unwrap().ips[0], "100.64.0.1");
        assert_eq!(s.tailnet.unwrap().magic_dns_suffix, "example.ts.net");
        assert_eq!(s.health.len(), 1);
    }

    #[test]
    fn zero_time_is_none() {
        let s: Status = fx("status_running.json");
        let by = |h: &str| s.peers.iter().find(|p| p.host_name == h).unwrap().clone();
        assert_eq!(by("bravo").last_seen, None);
        assert_eq!(by("echo").last_seen.unwrap().to_rfc3339(), "2026-11-18T00:00:00+00:00");
    }

    #[test]
    fn parses_needs_login() {
        let s: Status = fx("status_needs_login.json");
        assert!(!s.have_node_key);
        assert!(s.peers.is_empty());
        assert!(s.self_node.unwrap().ips.is_empty());
    }

    #[test]
    fn parses_exit_node_and_location() {
        let s: Status = fx("status_exit_node.json");
        assert_eq!(s.exit_node, Some(ExitNodeStatus { id: "nAAAA2CNTRL".into(), online: true }));
        let m: Status = fx("status_mullvad.json");
        let z = m.peers.iter().find(|p| p.location.as_ref().is_some_and(|l| l.city == "Zurich")).unwrap();
        assert_eq!(z.location.as_ref().unwrap().country, "Switzerland");
        assert!(z.exit_node_option);
    }

    #[test]
    fn parses_with_fields_removed() {
        let s: Status = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Status::default());
        let s: Status = serde_json::from_str(r#"{"Peer":null,"Self":null,"Health":null,"BackendState":null}"#).unwrap();
        assert_eq!(s, Status::default());
        let n: Notify = serde_json::from_str(r#"{"State":6,"Prefs":{}}"#).unwrap();
        assert_eq!(n.state, Some(6));
        assert_eq!(n.prefs.unwrap().operator_user, "");
        // A field that changes type costs that field, not the message.
        let n: Notify = serde_json::from_str(r#"{"State":"Running","Prefs":{"WantRunning":"yes","ExitNodeID":"x"}}"#).unwrap();
        assert_eq!(n.state, None);
        assert_eq!(n.prefs.unwrap().exit_node_id, "x");
    }

    #[test]
    fn bus_lines() {
        let n: Notify = fx("bus_running.ndjson");
        assert_eq!(n.state, Some(6));
        assert!(n.prefs.unwrap().want_running);
        let h: Notify = fx("bus_health.ndjson");
        let w = h.health.unwrap().warnings;
        assert_eq!(w.len(), 2);
        assert_eq!(w.iter().filter(|w| w.impacts_connectivity).count(), 1);
        assert_eq!(w[0].code, "accept-routes");
    }

    #[test]
    fn masked_prefs_json() {
        assert_eq!(serde_json::to_string(&MaskedPrefs::want_running(true)).unwrap(), r#"{"WantRunning":true,"WantRunningSet":true}"#);
        assert_eq!(
            serde_json::to_string(&MaskedPrefs::exit_node("nAAAA2CNTRL")).unwrap(),
            r#"{"ExitNodeID":"nAAAA2CNTRL","ExitNodeIDSet":true,"ExitNodeIP":"","ExitNodeIPSet":true}"#
        );
        assert_eq!(serde_json::to_string(&MaskedPrefs::allow_lan(false)).unwrap(), r#"{"ExitNodeAllowLANAccess":false,"ExitNodeAllowLANAccessSet":true}"#);
    }
}
