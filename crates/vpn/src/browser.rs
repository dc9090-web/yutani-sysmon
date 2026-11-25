//! The isolated browser on the applet's side: finding Firefox and the
//! profile it gets. It runs as its own instance with its own profile, since
//! a Firefox started the usual way would only open a window in the one
//! that's already running, outside the tunnel.

use std::path::{Path, PathBuf};

use crate::qbit::{home, on_path};

pub const FLATPAK_ID: &str = "org.mozilla.firefox";

/// How to start the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The full command, profile arguments included.
    pub argv: Vec<String>,
    /// The profile only this browser uses; created before the first launch.
    pub profile: PathBuf,
    /// For the settings placeholder.
    pub display: String,
}

/// A Flatpak app only sees its own directory under `~/.var/app`.
fn profile(flatpak: bool) -> PathBuf {
    if flatpak { home().join(".var/app").join(FLATPAK_ID).join("cosmic-vpn-profile") } else { home().join(".local/share/cosmic-vpn/firefox-profile") }
}

fn flatpak_installed() -> bool {
    [home().join(".local/share/flatpak/app").join(FLATPAK_ID), Path::new("/var/lib/flatpak/app").join(FLATPAK_ID)].iter().any(|p| p.exists())
}

/// `--no-remote`: never hand over to a Firefox that's already running.
fn command(mut argv: Vec<String>, flatpak: bool, display: String) -> Command {
    let profile = profile(flatpak);
    argv.extend(["--no-remote".to_owned(), "--profile".to_owned(), profile.to_string_lossy().into_owned()]);
    Command { argv, profile, display }
}

/// The override, else `firefox` on `PATH`, else the Flatpak.
pub fn detect(override_cmd: Option<&str>) -> Option<Command> {
    if let Some(cmd) = override_cmd.map(str::trim).filter(|c| !c.is_empty()) {
        let mut argv: Vec<String> = cmd.split_whitespace().map(str::to_owned).collect();
        // The helper wants an absolute argv[0].
        if !argv[0].starts_with('/')
            && let Some(p) = on_path(&argv[0])
        {
            argv[0] = p.to_string_lossy().into_owned();
        }
        return Some(command(argv, cmd.contains(FLATPAK_ID), cmd.to_owned()));
    }
    if let Some(p) = on_path("firefox") {
        return Some(command(vec![p.to_string_lossy().into_owned()], false, "firefox".into()));
    }
    if flatpak_installed() {
        let flatpak = on_path("flatpak").unwrap_or_else(|| PathBuf::from("/usr/bin/flatpak"));
        return Some(command(vec![flatpak.to_string_lossy().into_owned(), "run".into(), FLATPAK_ID.into()], true, format!("flatpak run {FLATPAK_ID}")));
    }
    None
}

/// Makes the profile directory, private to the user.
pub fn prepare(profile: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if profile.is_dir() {
        return Ok(());
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_instance_and_profile() {
        let c = detect(Some("/opt/firefox/firefox")).unwrap();
        assert_eq!(c.argv[0], "/opt/firefox/firefox");
        assert_eq!(&c.argv[1..3], ["--no-remote", "--profile"]);
        assert!(c.argv[3].ends_with(".local/share/cosmic-vpn/firefox-profile"));
        assert_eq!(Path::new(&c.argv[3]), c.profile);
        // `sh` is on every PATH: a bare command becomes absolute.
        assert!(detect(Some("sh")).unwrap().argv[0].starts_with('/'));
        let f = detect(Some("flatpak run org.mozilla.firefox")).unwrap();
        assert_eq!(&f.argv[1..3], ["run", FLATPAK_ID]);
        assert!(f.profile.ends_with(".var/app/org.mozilla.firefox/cosmic-vpn-profile"));
    }

    #[test]
    fn profile_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vpn-browser-test-{}", std::process::id()));
        let p = dir.join("a/profile");
        prepare(&p).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o700);
        prepare(&p).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
