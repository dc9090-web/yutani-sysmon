//! qBittorrent on the applet's side (SPEC §5): finding it, setting its
//! listening port in its conf file before launch, the environment it gets,
//! and leak detection (a qBittorrent running outside the namespace).

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use vpn_common::names::NETNS_PATH;
use vpn_common::qbitconf;

pub const FLATPAK_ID: &str = "org.qbittorrent.qBittorrent";

/// How to start qBittorrent, and where its settings live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub argv: Vec<String>,
    pub conf: PathBuf,
    /// For the settings placeholder.
    pub display: String,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

fn native_conf() -> PathBuf {
    home().join(".config/qBittorrent/qBittorrent.conf")
}

fn flatpak_conf() -> PathBuf {
    home().join(".var/app").join(FLATPAK_ID).join("config/qBittorrent/qBittorrent.conf")
}

fn flatpak_installed() -> bool {
    [home().join(".local/share/flatpak/app").join(FLATPAK_ID), Path::new("/var/lib/flatpak/app").join(FLATPAK_ID)].iter().any(|p| p.exists())
}

/// The override, else `qbittorrent` on `PATH`, else the Flatpak (SPEC §5.1).
pub fn detect(override_cmd: Option<&str>) -> Option<Command> {
    if let Some(cmd) = override_cmd.map(str::trim).filter(|c| !c.is_empty()) {
        let mut argv: Vec<String> = cmd.split_whitespace().map(str::to_owned).collect();
        // The helper wants an absolute argv[0].
        if !argv[0].starts_with('/')
            && let Some(p) = on_path(&argv[0])
        {
            argv[0] = p.to_string_lossy().into_owned();
        }
        let conf = if cmd.contains(FLATPAK_ID) { flatpak_conf() } else { native_conf() };
        return Some(Command { display: cmd.to_owned(), argv, conf });
    }
    if let Some(p) = on_path("qbittorrent") {
        return Some(Command { argv: vec![p.to_string_lossy().into_owned()], conf: native_conf(), display: "qbittorrent".into() });
    }
    if flatpak_installed() {
        let flatpak = on_path("flatpak").unwrap_or_else(|| PathBuf::from("/usr/bin/flatpak"));
        return Some(Command {
            argv: vec![flatpak.to_string_lossy().into_owned(), "run".into(), FLATPAK_ID.into()],
            conf: flatpak_conf(),
            display: format!("flatpak run {FLATPAK_ID}"),
        });
    }
    None
}

/// Removes qBittorrent's single-instance lock left by an instance that
/// didn't exit cleanly, but only when no qBittorrent is running at all.
///
/// qBittorrent's lock file records the holder's PID, and under Flatpak every
/// instance is PID 2 in its sandbox: a new instance sees "PID 2 is alive"
/// (itself), takes the stale lock for a running copy, and quits at once.
/// Returns whether a stale lock was removed.
pub fn clear_stale_lock(conf: &Path) -> bool {
    if !scan().is_empty() {
        return false;
    }
    let Some(dir) = conf.parent() else { return false };
    let lock = dir.join("lockfile");
    if !lock.exists() {
        return false;
    }
    let removed = std::fs::remove_file(&lock).is_ok();
    let _ = std::fs::remove_file(dir.join("ipc-socket"));
    if removed {
        tracing::info!("removed qBittorrent's stale instance lock");
    }
    removed
}

/// Writes `port` into qBittorrent's conf (only while it isn't running).
/// `Ok(false)` if the file's layout isn't recognised.
pub fn write_port(conf: &Path, port: u16) -> std::io::Result<bool> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let text = std::fs::read_to_string(conf)?;
    let Some(new) = qbitconf::set_listen_port(&text, port) else { return Ok(false) };
    if new == text {
        return Ok(true);
    }
    let mode = std::fs::metadata(conf)?.permissions().mode();
    let tmp = conf.with_extension("conf.cosmic-vpn.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(new.as_bytes())?;
    f.sync_all()?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(tmp, conf)?;
    Ok(true)
}

const ENV_KEYS: [&str; 12] = [
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

fn wanted(k: &str) -> bool {
    ENV_KEYS.contains(&k) || k.starts_with("LC_")
}

/// The environment qBittorrent needs to show its window. Panel applets run
/// on the panel's own Wayland connection, without the session's
/// `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR` or session bus, so those come from
/// the user's systemd manager (the session's environment). The helper keeps
/// only allowlisted keys.
pub fn environment() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().filter(|(k, _)| wanted(k)).collect();
    let runtime = format!("/run/user/{}", nix::unistd::Uid::current().as_raw());
    let session = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={runtime}/bus"))
        .output();
    match session {
        Ok(out) if out.status.success() => env.extend(parse_environment(&String::from_utf8_lossy(&out.stdout))),
        Ok(out) => tracing::warn!("reading the session environment: {}", String::from_utf8_lossy(&out.stderr).trim()),
        Err(e) => tracing::warn!("reading the session environment: {e}"),
    }
    env.entry("XDG_RUNTIME_DIR".into()).or_insert_with(|| runtime.clone());
    env.entry("DBUS_SESSION_BUS_ADDRESS".into()).or_insert_with(|| format!("unix:path={runtime}/bus"));
    env
}

/// `KEY=value` lines from `systemctl --user show-environment`, allowlisted.
/// Values with special characters come `$'…'`-quoted; those are skipped.
fn parse_environment(text: &str) -> HashMap<String, String> {
    text.lines().filter_map(|l| l.split_once('=')).filter(|(k, v)| wanted(k) && !v.starts_with("$'")).map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

/// The namespace's inode, if it exists.
pub fn netns_id() -> Option<(u64, u64)> {
    std::fs::metadata(NETNS_PATH).ok().map(|m| (m.dev(), m.ino()))
}

/// qBittorrent processes of this user, and whether each runs in the namespace.
pub fn scan() -> Vec<(i32, bool)> {
    let uid = nix::unistd::Uid::current().as_raw();
    let ns = netns_id();
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.filter_map(Result::ok)
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let base = e.path();
            if std::fs::metadata(&base).ok()?.uid() != uid {
                return None;
            }
            let comm = std::fs::read_to_string(base.join("comm")).ok()?;
            if comm.trim() != "qbittorrent" {
                return None;
            }
            let net = std::fs::metadata(base.join("ns/net")).ok().map(|m| (m.dev(), m.ino()));
            Some((pid, ns.is_some() && net == ns))
        })
        .collect()
}

/// PIDs of qBittorrent running outside the namespace.
pub fn outside() -> Vec<i32> {
    scan().into_iter().filter(|(_, inside)| !inside).map(|(p, _)| p).collect()
}

/// Asks those processes to quit (they're the user's own) and waits up to 10 s.
pub async fn terminate(pids: Vec<i32>) -> bool {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    for p in &pids {
        let _ = kill(Pid::from_raw(*p), Signal::SIGTERM);
    }
    for _ in 0..40 {
        if pids.iter().all(|p| !Path::new(&format!("/proc/{p}")).exists()) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_command() {
        let c = detect(Some("flatpak run org.qbittorrent.qBittorrent")).unwrap();
        assert!(c.argv[0].starts_with('/'));
        assert_eq!(&c.argv[1..], ["run", FLATPAK_ID]);
        assert!(c.conf.ends_with(".var/app/org.qbittorrent.qBittorrent/config/qBittorrent/qBittorrent.conf"));
        let n = detect(Some("/opt/qbt/qbittorrent")).unwrap();
        assert_eq!(n.argv, ["/opt/qbt/qbittorrent"]);
        assert!(n.conf.ends_with(".config/qBittorrent/qBittorrent.conf"));
    }

    #[test]
    fn conf_port_written_in_place() {
        let dir = std::env::temp_dir().join(format!("vpn-qbit-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("qBittorrent.conf");
        std::fs::write(&p, "[BitTorrent]\nSession\\Port=1\n\n[Network]\nPortForwardingEnabled=true\n").unwrap();
        assert!(write_port(&p, 53186).unwrap());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "[BitTorrent]\nSession\\Port=53186\n\n[Network]\nPortForwardingEnabled=false\n");
        std::fs::write(&p, "[Other]\nx=1\n").unwrap();
        assert!(!write_port(&p, 2).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_lock_is_cleared() {
        // Only meaningful when no qBittorrent runs (as in CI and here).
        if !scan().is_empty() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("vpn-qbit-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conf = dir.join("qBittorrent.conf");
        std::fs::write(dir.join("lockfile"), "2\nqbittorrent\nhost\n\nboot\n").unwrap();
        std::fs::write(dir.join("ipc-socket"), "").unwrap();
        assert!(clear_stale_lock(&conf));
        assert!(!dir.join("lockfile").exists() && !dir.join("ipc-socket").exists());
        assert!(!clear_stale_lock(&conf));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn environment_is_allowlisted() {
        let env = environment();
        assert!(env.keys().all(|k| !k.starts_with("LD_") && k != "SHELL"));
        assert!(env.contains_key("XDG_RUNTIME_DIR") && env.contains_key("DBUS_SESSION_BUS_ADDRESS"));
    }

    #[test]
    fn session_environment_lines() {
        let text = "HOME=/home/u\nWAYLAND_DISPLAY=wayland-1\nLD_PRELOAD=/x.so\nLC_TIME=en_AU.UTF-8\nXDG_DATA_DIRS=$'/a\\nb'\nSHELL=/bin/zsh\n";
        let env = parse_environment(text);
        assert_eq!(env.get("WAYLAND_DISPLAY").map(String::as_str), Some("wayland-1"));
        assert_eq!(env.get("LC_TIME").map(String::as_str), Some("en_AU.UTF-8"));
        assert!(!env.contains_key("LD_PRELOAD") && !env.contains_key("SHELL") && !env.contains_key("XDG_DATA_DIRS"));
    }
}
