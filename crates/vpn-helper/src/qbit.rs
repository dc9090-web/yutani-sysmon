//! qBittorrent: launched as a transient systemd unit inside the namespace
//! (SPEC §5.2), stopped (§5.3), and its listening port set through the Web
//! UI from inside the namespace (§5.4).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use vpn_common::names::{NETNS_PATH, QBIT_UNIT};
use zbus::zvariant::{OwnedObjectPath, Value};
use zeroize::Zeroizing;

use crate::netns;

const RESOLV: &str = "/etc/netns/vpn-p2p/resolv.conf";

/// The only environment keys passed to qBittorrent (SPEC §5.2).
const ENV_ALLOW: [&str; 12] = [
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "HOME",
    "LANG",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "QT_QPA_PLATFORM",
    "PATH",
    "XDG_DATA_DIRS",
];

pub fn env_allowed(key: &str) -> bool {
    ENV_ALLOW.contains(&key) || key.starts_with("LC_")
}

/// argv[0] must be an absolute path the caller owns, or a system or Flatpak path.
pub fn argv0_allowed(path: &str, uid: u32, home: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let p = Path::new(path);
    if !p.is_absolute() || path.contains("/../") {
        return false;
    }
    let Ok(meta) = std::fs::metadata(p) else { return false };
    if !meta.is_file() {
        return false;
    }
    let trusted = ["/usr/", "/var/lib/flatpak/"].iter().any(|d| path.starts_with(d)) || p.starts_with(home.join(".local/share/flatpak"));
    trusted || meta.uid() == uid
}

fn systemd(conn: &zbus::Connection) -> impl std::future::Future<Output = zbus::Result<zbus::Proxy<'static>>> + '_ {
    zbus::Proxy::new(conn, "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager")
}

/// Starts qBittorrent in the namespace as `uid`.
pub async fn launch(conn: &zbus::Connection, argv: Vec<String>, env: HashMap<String, String>, uid: u32, gid: u32, home: String) -> zbus::Result<()> {
    let env: Vec<String> = env.into_iter().filter(|(k, v)| env_allowed(k) && !v.contains('\n')).map(|(k, v)| format!("{k}={v}")).collect();
    let mut binds = vec![(RESOLV.to_owned(), "/etc/resolv.conf".to_owned(), false, 0u64)];
    // Flatpak apps read a copy of the host's resolv.conf kept by the session
    // helper; point that at the tunnel's DNS too.
    let flatpak_resolv = format!("/run/user/{uid}/.flatpak-helper/monitor/resolv.conf");
    if Path::new(&flatpak_resolv).exists() {
        binds.push((RESOLV.to_owned(), flatpak_resolv, true, 0));
    }
    let exec = vec![(argv[0].clone(), argv.clone(), false)];
    let props: Vec<(&str, Value<'_>)> = vec![
        ("Description", Value::from("qBittorrent inside the VPN namespace")),
        ("Type", Value::from("exec")),
        ("ExecStart", Value::from(exec)),
        ("User", Value::from(uid.to_string())),
        ("Group", Value::from(gid.to_string())),
        ("WorkingDirectory", Value::from(home)),
        ("Environment", Value::from(env)),
        ("NetworkNamespacePath", Value::from(NETNS_PATH)),
        ("BindReadOnlyPaths", Value::from(binds)),
        // Host DNS daemons sit outside the tunnel: hide their sockets so
        // glibc falls back to the bind-mounted resolv.conf.
        ("InaccessiblePaths", Value::from(vec!["-/run/systemd/resolve", "-/run/nscd", "-/run/avahi-daemon"])),
        ("CollectMode", Value::from("inactive-or-failed")),
        // `flatpak run` exits once the app is up (the app moves to a scope of
        // the user's systemd); without this, systemd would SIGTERM it then.
        // The helper stops qBittorrent itself, by process (see `stop`).
        ("KillMode", Value::from("process")),
        ("TimeoutStopUSec", Value::from(10_000_000u64)),
    ];
    let aux: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let _job: OwnedObjectPath = systemd(conn).await?.call("StartTransientUnit", &(QBIT_UNIT, "fail", props, aux)).await?;
    Ok(())
}

/// The unit's state: (`ActiveState`, main exit status).
pub async fn unit_state(conn: &zbus::Connection) -> Option<(String, i32)> {
    let path: OwnedObjectPath = systemd(conn).await.ok()?.call("GetUnit", &(QBIT_UNIT,)).await.ok()?;
    let unit = zbus::Proxy::new(conn, "org.freedesktop.systemd1", path.clone(), "org.freedesktop.systemd1.Unit").await.ok()?;
    let state: String = unit.get_property("ActiveState").await.ok()?;
    let service = zbus::Proxy::new(conn, "org.freedesktop.systemd1", path, "org.freedesktop.systemd1.Service").await.ok()?;
    let code: i32 = service.get_property("ExecMainStatus").await.unwrap_or(0);
    Some((state, code))
}

/// qBittorrent processes inside the namespace. The unit only launches it:
/// `flatpak run` moves the app into a scope of the user's own systemd, so
/// the process, not the unit, is what counts.
pub fn pids() -> Vec<i32> {
    use std::os::unix::fs::MetadataExt;
    let Ok(ns) = std::fs::metadata(NETNS_PATH).map(|m| (m.dev(), m.ino())) else { return Vec::new() };
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.filter_map(Result::ok)
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
            if comm.trim() != "qbittorrent" {
                return None;
            }
            let net = std::fs::metadata(e.path().join("ns/net")).ok()?;
            ((net.dev(), net.ino()) == ns).then_some(pid)
        })
        .collect()
}

/// The unit is starting or running.
pub async fn unit_busy(conn: &zbus::Connection) -> bool {
    unit_state(conn).await.is_some_and(|(s, _)| s == "active" || s == "activating")
}

/// Asks qBittorrent to quit: stops the unit and sends SIGTERM to the
/// processes in the namespace (qBittorrent shuts down cleanly on it).
pub async fn stop(conn: &zbus::Connection) {
    if let Ok(m) = systemd(conn).await {
        let _: zbus::Result<OwnedObjectPath> = m.call("StopUnit", &(QBIT_UNIT, "replace")).await;
    }
    signal(nix::sys::signal::Signal::SIGTERM);
}

fn signal(sig: nix::sys::signal::Signal) {
    for p in pids() {
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(p), sig);
    }
}

/// Waits up to `limit` for qBittorrent to exit, then kills what's left.
pub async fn wait_stopped(conn: &zbus::Connection, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if tokio::task::spawn_blocking(pids).await.unwrap_or_default().is_empty() && !unit_busy(conn).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("qBittorrent didn't quit in time; killing it");
            signal(nix::sys::signal::Signal::SIGKILL);
            return false;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A Web UI call's result, as `SetQbitPort` / `TestQbitWebUi` return it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebUi {
    Ok,
    Unreachable,
    AuthFailed,
    NotRunning,
}

impl WebUi {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unreachable => "unreachable",
            Self::AuthFailed => "auth_failed",
            Self::NotRunning => "not_running",
        }
    }
}

/// Logs in (unless `user` is empty, for localhost bypass) and optionally
/// sets the port; then checks the preferences. Runs inside the namespace,
/// where qBittorrent's Web UI listens on 127.0.0.1.
pub fn webui(webui_port: u16, user: Zeroizing<String>, pass: Zeroizing<String>, set_port: Option<u16>) -> WebUi {
    let r = netns::in_netns_async(move || async move {
        // No cookie store: qBittorrent marks its session cookie Secure, which a
        // store won't send over http://127.0.0.1. The cookie is carried by hand.
        let client = match reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build() {
            Ok(c) => c,
            Err(_) => return WebUi::Unreachable,
        };
        let base = format!("http://127.0.0.1:{webui_port}/api/v2");
        let unreachable = |e: reqwest::Error| {
            tracing::debug!("Web UI: {}", vpn_common::redact(&e.without_url().to_string()));
            WebUi::Unreachable
        };
        let mut cookie = Zeroizing::new(String::new());
        // With "Bypass authentication for clients on localhost" no login is
        // needed: try that first, so a stale saved password never counts
        // toward qBittorrent's failed-login ban.
        let bypass = match client.get(format!("{base}/app/preferences")).send().await {
            Ok(r) => r.status().is_success(),
            Err(e) => return unreachable(e),
        };
        if !bypass && !user.is_empty() {
            let form = [("username", user.as_str()), ("password", pass.as_str())];
            let resp = match client.post(format!("{base}/auth/login")).form(&form).send().await {
                Ok(r) => r,
                Err(e) => return unreachable(e),
            };
            if resp.status().as_u16() == 403 {
                return WebUi::AuthFailed;
            }
            *cookie = session_cookie(resp.headers().get_all(reqwest::header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()));
            if resp.text().await.unwrap_or_default().trim() != "Ok." {
                return WebUi::AuthFailed;
            }
        }
        let with_cookie = |r: reqwest::RequestBuilder| if cookie.is_empty() { r } else { r.header(reqwest::header::COOKIE, cookie.as_str()) };
        if let Some(port) = set_port {
            let json = format!("{{\"listen_port\":{port},\"upnp\":false,\"random_port\":false}}");
            match with_cookie(client.post(format!("{base}/app/setPreferences"))).form(&[("json", json)]).send().await {
                Ok(r) if r.status().as_u16() == 403 => return WebUi::AuthFailed,
                Ok(r) if !r.status().is_success() => return WebUi::Unreachable,
                Ok(_) => {}
                Err(e) => return unreachable(e),
            }
        }
        let prefs = match with_cookie(client.get(format!("{base}/app/preferences"))).send().await {
            Ok(r) if r.status().as_u16() == 403 => return WebUi::AuthFailed,
            Ok(r) => r.bytes().await.ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()),
            Err(e) => return unreachable(e),
        };
        match (prefs, set_port) {
            (Some(p), Some(port)) if p.get("listen_port").and_then(serde_json::Value::as_u64) == Some(u64::from(port)) => WebUi::Ok,
            (Some(_), Some(_)) => WebUi::Unreachable,
            (Some(_), None) => WebUi::Ok,
            (None, _) => WebUi::Unreachable,
        }
    });
    r.unwrap_or(WebUi::NotRunning)
}

/// `name=value` pairs from `Set-Cookie` headers, as one `Cookie` header.
fn session_cookie<'a>(headers: impl Iterator<Item = &'a str>) -> String {
    headers.filter_map(|h| h.split(';').next()).map(str::trim).filter(|c| c.contains('=')).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_allowlist() {
        for k in ["WAYLAND_DISPLAY", "HOME", "LC_TIME", "PATH"] {
            assert!(env_allowed(k), "{k}");
        }
        for k in ["LD_PRELOAD", "LD_LIBRARY_PATH", "SHELL", "QT_PLUGIN_PATH"] {
            assert!(!env_allowed(k), "{k}");
        }
    }

    #[test]
    fn cookies_carried_by_hand() {
        // qBittorrent 5 names it QBT_SID_<port> and marks it Secure.
        let h = ["QBT_SID_8080=abc123; HttpOnly; SameSite=Strict; Secure; path=/"];
        assert_eq!(session_cookie(h.into_iter()), "QBT_SID_8080=abc123");
        let two = ["SID=x; path=/", "other=y"];
        assert_eq!(session_cookie(two.into_iter()), "SID=x; other=y");
        assert_eq!(session_cookie(std::iter::empty()), "");
    }

    #[test]
    fn argv0_rules() {
        let home = Path::new("/home/nobody-here");
        assert!(argv0_allowed("/usr/bin/env", 12345, home));
        assert!(!argv0_allowed("usr/bin/env", 0, home));
        assert!(!argv0_allowed("/usr/bin/../bin/env", 0, home));
        assert!(!argv0_allowed("/usr/bin/does-not-exist", 0, home));
        // A file outside the trusted paths, owned by someone else.
        assert!(!argv0_allowed("/etc/hostname", 12345, home));
        assert!(argv0_allowed("/etc/hostname", 0, home));
    }
}
