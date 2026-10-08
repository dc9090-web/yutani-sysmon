//! The web tunnel through NetworkManager (SPEC §6): a one-time `nmcli`
//! import, then activation, deactivation and state over D-Bus. NM is the
//! source of truth, including changes made in COSMIC Settings.

use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use vpn_common::conf::{self, Conf};
use vpn_common::names::{WEB_IF, WEB_V6_PLACEHOLDER};
use vpn_common::status::TunnelState;
use zbus::zvariant::OwnedObjectPath;
use zeroize::Zeroizing;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

/// The web tunnel's state as NM reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebState {
    pub state: TunnelState,
    /// The reason, when an activation failed.
    pub error: String,
    pub other_vpn: bool,
}

/// NM's `NMActiveConnectionStateReason`, for failures.
fn reason_text(reason: u32) -> &'static str {
    match reason {
        3 => "the device disconnected",
        4 => "the VPN service stopped",
        5 => "invalid IP configuration",
        6 => "connection timed out",
        7 | 8 => "the VPN service failed to start",
        9 => "no secrets",
        10 => "login failed",
        11 => "the connection was removed",
        12 => "a dependency failed",
        13 | 14 => "the device failed",
        _ => "unknown reason",
    }
}

async fn nm_proxy(conn: &zbus::Connection, path: &str, iface: &'static str) -> zbus::Result<zbus::Proxy<'static>> {
    zbus::Proxy::new(conn, NM, zbus::zvariant::OwnedObjectPath::try_from(path)?, iface).await
}

async fn connection_path(conn: &zbus::Connection, uuid: &str) -> zbus::Result<OwnedObjectPath> {
    nm_proxy(conn, "/org/freedesktop/NetworkManager/Settings", "org.freedesktop.NetworkManager.Settings").await?.call("GetConnectionByUuid", &(uuid,)).await
}

/// Active connections: (path, uuid, type, state).
async fn active(conn: &zbus::Connection) -> zbus::Result<Vec<(OwnedObjectPath, String, String, u32)>> {
    let root = nm_proxy(conn, NM_PATH, NM).await?;
    let paths: Vec<OwnedObjectPath> = root.get_property("ActiveConnections").await?;
    let mut out = Vec::new();
    for p in paths {
        let Ok(a) = nm_proxy(conn, p.as_str(), "org.freedesktop.NetworkManager.Connection.Active").await else { continue };
        let (Ok(uuid), Ok(kind), Ok(state)) =
            (a.get_property::<String>("Uuid").await, a.get_property::<String>("Type").await, a.get_property::<u32>("State").await)
        else {
            continue;
        };
        out.push((p, uuid, kind, state));
    }
    Ok(out)
}

fn map_state(s: u32) -> TunnelState {
    match s {
        1 => TunnelState::Connecting,
        2 => TunnelState::On,
        _ => TunnelState::Off,
    }
}

/// The web tunnel's state now.
pub async fn state(conn: &zbus::Connection, uuid: &str) -> zbus::Result<WebState> {
    let all = active(conn).await?;
    let ours = all.iter().find(|a| a.1 == uuid);
    let other_vpn = all.iter().any(|a| a.1 != uuid && (a.2 == "vpn" || a.2 == "wireguard") && a.3 <= 2);
    Ok(WebState { state: ours.map_or(TunnelState::Off, |a| map_state(a.3)), error: String::new(), other_vpn })
}

pub async fn activate(conn: zbus::Connection, uuid: String) -> Result<(), String> {
    let path = connection_path(&conn, &uuid).await.map_err(|e| format!("NetworkManager: {e}"))?;
    let root = nm_proxy(&conn, NM_PATH, NM).await.map_err(|e| e.to_string())?;
    let none = OwnedObjectPath::try_from("/").expect("valid path");
    let _active: OwnedObjectPath =
        root.call("ActivateConnection", &(path, none.clone(), none)).await.map_err(|e| format!("NetworkManager: activation failed ({})", short(&e)))?;
    Ok(())
}

pub async fn deactivate(conn: zbus::Connection, uuid: String) -> Result<(), String> {
    let all = active(&conn).await.map_err(|e| e.to_string())?;
    let root = nm_proxy(&conn, NM_PATH, NM).await.map_err(|e| e.to_string())?;
    for (p, ..) in all.into_iter().filter(|a| a.1 == uuid) {
        root.call::<_, _, ()>("DeactivateConnection", &(p,)).await.map_err(|e| format!("NetworkManager: {}", short(&e)))?;
    }
    Ok(())
}

/// Deletes the imported connection (Replace).
pub async fn delete(conn: zbus::Connection, uuid: String) -> Result<(), String> {
    let path = connection_path(&conn, &uuid).await.map_err(|e| e.to_string())?;
    nm_proxy(&conn, path.as_str(), "org.freedesktop.NetworkManager.Settings.Connection")
        .await
        .map_err(|e| e.to_string())?
        .call::<_, _, ()>("Delete", &())
        .await
        .map_err(|e| format!("NetworkManager: {}", short(&e)))
}

fn short(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(_, Some(m), _) => m.clone(),
        other => other.to_string(),
    }
}

/// The conf text NM imports: `wg-web` as the interface, and when Proton
/// gives no IPv6, a placeholder address and `::/0` so IPv6 goes into the
/// tunnel and dies there instead of leaking (SPEC §6.1 step 5).
pub fn import_text(text: &str, conf: &Conf) -> Zeroizing<String> {
    let v6_addr = conf.addresses.iter().any(|a| a.addr.is_ipv6());
    let v6_route = conf.allowed_ips.iter().any(|a| a.is_default() && a.addr.is_ipv6());
    let mut out = Zeroizing::new(String::with_capacity(text.len() + 64));
    let mut section = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_ascii_lowercase();
        }
        let key = t.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
        if section == "[interface]" && key == "address" && !v6_addr {
            out.push_str(&format!("{line}, {WEB_V6_PLACEHOLDER}\n"));
        } else if section == "[peer]" && key == "allowedips" && !v6_route {
            out.push_str(&format!("{line}, ::/0\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from).join("cosmic-vpn")
}

/// Imports the web conf into NM; returns the new connection's UUID.
pub async fn import(text: Zeroizing<String>, label: String) -> Result<String, String> {
    let parsed = conf::parse(&text, "wg-web.conf").map_err(|e| e.to_string())?;
    let body = import_text(&text, &parsed);
    drop(parsed);
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    // The file stem is NM's interface name.
    let path = dir.join(format!("{WEB_IF}.conf"));
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).map_err(|e| e.to_string())?;
        f.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    }
    drop(body);
    let out = tokio::process::Command::new("nmcli").args(["--terse", "connection", "import", "type", "wireguard", "file"]).arg(&path).output().await;
    // The key file goes as soon as NM has it.
    let _ = std::fs::remove_file(&path);
    let out = out.map_err(|e| format!("nmcli: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        return Err(format!("nmcli: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let uuid = parse_uuid(&stdout).ok_or_else(|| format!("nmcli: no UUID in “{}”", stdout.trim()))?;
    let id = format!("Proton {label}");
    let m = tokio::process::Command::new("nmcli")
        .args(["connection", "modify", &uuid, "connection.id", &id, "connection.autoconnect", "no"])
        .output()
        .await
        .map_err(|e| format!("nmcli: {e}"))?;
    if !m.status.success() {
        tracing::warn!("renaming the connection: {}", String::from_utf8_lossy(&m.stderr).trim());
    }
    Ok(uuid)
}

/// The UUID in nmcli's "Connection 'x' (uuid) successfully added."
pub fn parse_uuid(s: &str) -> Option<String> {
    s.split(|c: char| c == '(' || c == ')' || c.is_whitespace())
        .find(|w| w.len() == 36 && w.chars().filter(|c| *c == '-').count() == 4 && w.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .map(str::to_owned)
}

/// `wg-web`'s byte counters.
pub fn counters() -> Option<(u64, u64)> {
    let read = |n: &str| std::fs::read_to_string(format!("/sys/class/net/{WEB_IF}/statistics/{n}")).ok()?.trim().parse::<u64>().ok();
    Some((read("rx_bytes")?, read("tx_bytes")?))
}

/// The web tunnel's state whenever NM's active connections change, or our
/// connection's state does (with the failure reason).
pub fn watch(uuid: String) -> impl futures_util::Stream<Item = WebState> {
    cosmic::iced::stream::channel(8, move |mut out: cosmic::iced::futures::channel::mpsc::Sender<WebState>| async move {
        use cosmic::iced::futures::SinkExt;
        loop {
            let run = async {
                let conn = zbus::Connection::system().await?;
                let props = zbus::fdo::PropertiesProxy::builder(&conn).destination(NM)?.path(NM_PATH)?.build().await?;
                let mut root_changes = props.receive_properties_changed().await?;
                let rule = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .sender(NM)?
                    .interface("org.freedesktop.NetworkManager.Connection.Active")?
                    .member("StateChanged")?
                    .build();
                let mut active_changes = zbus::MessageStream::for_match_rule(rule, &conn, Some(16)).await?;
                let mut last = WebState::default();
                let mut first = true;
                loop {
                    let mut now = state(&conn, &uuid).await?;
                    if now.state == TunnelState::Off && last.state == TunnelState::Connecting && !last.error.is_empty() {
                        now.error = last.error.clone();
                    }
                    if first || now != last {
                        first = false;
                        if out.send(now.clone()).await.is_err() {
                            return Ok::<_, zbus::Error>(());
                        }
                        last = now;
                    }
                    tokio::select! {
                        _ = root_changes.next() => {}
                        Some(Ok(msg)) = active_changes.next() => {
                            // (state, reason): a failed activation ends in "deactivated" with a reason.
                            if let Ok((s, reason)) = msg.body().deserialize::<(u32, u32)>()
                                && s == 4 && reason > 2 && last.state == TunnelState::Connecting
                            {
                                let failed = WebState { state: TunnelState::Error, error: format!("NetworkManager: activation failed ({})", reason_text(reason)), other_vpn: last.other_vpn };
                                if out.send(failed.clone()).await.is_err() {
                                    return Ok(());
                                }
                                last = failed;
                            }
                        }
                        () = tokio::time::sleep(Duration::from_secs(30)) => {}
                    }
                }
            };
            if let Err(e) = run.await {
                tracing::debug!("NetworkManager: {e}");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

/// Whether NM knows a connection with this UUID.
pub async fn exists(conn: &zbus::Connection, uuid: &str) -> bool {
    connection_path(conn, uuid).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../vpn-common/tests/fixtures/conf/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn ipv6_block_is_added_only_when_missing() {
        let text = fixture("web_ch12.conf");
        let c = conf::parse(&text, "x").unwrap();
        let out = import_text(&text, &c);
        assert!(out.contains("Address = 10.2.0.2/32, fd00:ca6c::3/128\n"));
        assert!(out.contains("AllowedIPs = 0.0.0.0/0, ::/0\n"));
        let reparsed = conf::parse(&out, "x").unwrap();
        assert!(reparsed.summary.has_v6);
        // A conf with IPv6 already is left alone.
        let v6 = fixture("p2p_natpmp_on_v6.conf");
        let c = conf::parse(&v6, "x").unwrap();
        assert_eq!(import_text(&v6, &c).trim_end(), v6.trim_end());
    }

    #[test]
    fn nmcli_uuid() {
        let s = "Connection 'wg-web' (3f2a9c4e-1b2d-4c5e-8f90-123456789abc) successfully added.\n";
        assert_eq!(parse_uuid(s).as_deref(), Some("3f2a9c4e-1b2d-4c5e-8f90-123456789abc"));
        assert_eq!(parse_uuid("Error: failed"), None);
    }

    #[test]
    fn states() {
        assert_eq!(map_state(1), TunnelState::Connecting);
        assert_eq!(map_state(2), TunnelState::On);
        assert_eq!(map_state(3), TunnelState::Off);
        assert_eq!(map_state(4), TunnelState::Off);
        assert_eq!(reason_text(9), "no secrets");
    }
}
