//! Apps launched inside a namespace: a transient systemd unit running as the
//! caller, with the tunnel's DNS bound over the host's (SPEC §5.2).

use std::collections::HashMap;
use std::path::Path;

use zbus::zvariant::{OwnedObjectPath, Value};

use crate::netns::Space;

/// The only environment keys passed to a launched app (SPEC §5.2).
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

/// Who the app runs as: the D-Bus caller.
pub struct User {
    pub uid: u32,
    pub gid: u32,
    pub home: String,
}

/// Starts `argv` inside `sp` as `user`, in the transient unit `unit`.
pub async fn launch(
    conn: &zbus::Connection,
    unit: &str,
    description: &str,
    sp: &Space,
    argv: Vec<String>,
    env: HashMap<String, String>,
    user: User,
) -> zbus::Result<()> {
    let env: Vec<String> = env.into_iter().filter(|(k, v)| env_allowed(k) && !v.contains('\n')).map(|(k, v)| format!("{k}={v}")).collect();
    let resolv = sp.resolv();
    let mut binds = vec![(resolv.clone(), "/etc/resolv.conf".to_owned(), false, 0u64)];
    // Flatpak apps read a copy of the host's resolv.conf kept by the session
    // helper; point that at the tunnel's DNS too.
    let flatpak_resolv = format!("/run/user/{}/.flatpak-helper/monitor/resolv.conf", user.uid);
    if Path::new(&flatpak_resolv).exists() {
        binds.push((resolv, flatpak_resolv, true, 0));
    }
    let exec = vec![(argv[0].clone(), argv.clone(), false)];
    let props: Vec<(&str, Value<'_>)> = vec![
        ("Description", Value::from(description)),
        ("Type", Value::from("exec")),
        ("ExecStart", Value::from(exec)),
        ("User", Value::from(user.uid.to_string())),
        ("Group", Value::from(user.gid.to_string())),
        ("WorkingDirectory", Value::from(user.home)),
        ("Environment", Value::from(env)),
        ("NetworkNamespacePath", Value::from(sp.path)),
        ("BindReadOnlyPaths", Value::from(binds)),
        // Host DNS daemons sit outside the tunnel: hide their sockets so
        // glibc falls back to the bind-mounted resolv.conf.
        ("InaccessiblePaths", Value::from(vec!["-/run/systemd/resolve", "-/run/nscd", "-/run/avahi-daemon"])),
        ("CollectMode", Value::from("inactive-or-failed")),
        // `flatpak run` exits once the app is up (the app moves to a scope of
        // the user's systemd), and a browser that restarts itself hands over
        // to a new process; without this, systemd would SIGTERM the app then.
        // The helper stops apps itself, by process.
        ("KillMode", Value::from("process")),
        ("TimeoutStopUSec", Value::from(10_000_000u64)),
    ];
    let aux: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let _job: OwnedObjectPath = systemd(conn).await?.call("StartTransientUnit", &(unit, "fail", props, aux)).await?;
    Ok(())
}

/// The unit's state: (`ActiveState`, main exit status).
pub async fn state(conn: &zbus::Connection, unit: &str) -> Option<(String, i32)> {
    let path: OwnedObjectPath = systemd(conn).await.ok()?.call("GetUnit", &(unit,)).await.ok()?;
    let proxy = zbus::Proxy::new(conn, "org.freedesktop.systemd1", path.clone(), "org.freedesktop.systemd1.Unit").await.ok()?;
    let state: String = proxy.get_property("ActiveState").await.ok()?;
    let service = zbus::Proxy::new(conn, "org.freedesktop.systemd1", path, "org.freedesktop.systemd1.Service").await.ok()?;
    let code: i32 = service.get_property("ExecMainStatus").await.unwrap_or(0);
    Some((state, code))
}

pub async fn stop(conn: &zbus::Connection, unit: &str) {
    if let Ok(m) = systemd(conn).await {
        let _: zbus::Result<OwnedObjectPath> = m.call("StopUnit", &(unit, "replace")).await;
    }
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
