//! The OpenWeather API key (SPEC §3): the Secret Service keyring, or a 0600
//! file when there's none. The key never reaches config, logs, the UI or
//! error text; `redact` cleans any URL before it's logged.

use std::fmt;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use crate::config::APP_ID;

const LABEL: &str = "OpenWeather API key";

/// The key. `Debug` never shows it and there's no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);

impl Key {
    /// A pasted key, trimmed; `None` if nothing is left.
    pub fn new(s: &str) -> Option<Self> {
        let s = s.trim();
        (!s.is_empty()).then(|| Self(s.to_owned()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(<redacted>)")
    }
}

/// A key is 32 hex characters. Other shapes get a warning but can still be tested.
pub fn looks_valid(s: &str) -> bool {
    let s = s.trim();
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Where the key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Keyring,
    /// No Secret Service: `$XDG_CONFIG_HOME/<APP_ID>/api-key`, mode 0600.
    File,
}

fn attributes(app: &str) -> [(&'static str, &str); 1] {
    [("application", app)]
}

fn file_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join(APP_ID).join("api-key"))
}

/// The stored key, from the keyring first, then the fallback file.
pub async fn load() -> Option<(Key, Storage)> {
    match from_keyring(APP_ID).await {
        Ok(Some(k)) => return Some((k, Storage::Keyring)),
        Ok(None) => {}
        Err(e) => tracing::debug!("keyring unavailable: {e}"),
    }
    let path = file_path()?;
    let text = tokio::fs::read_to_string(&path).await.ok()?;
    Key::new(&text).map(|k| (k, Storage::File))
}

#[expect(clippy::result_large_err, reason = "oo7's own error type")]
async fn from_keyring(app: &str) -> oo7::Result<Option<Key>> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    let Some(item) = keyring.search_items(&attributes(app)).await?.into_iter().next() else { return Ok(None) };
    if item.is_locked().await? {
        item.unlock().await?;
    }
    let secret = item.secret().await?;
    Ok(std::str::from_utf8(secret.as_bytes()).ok().and_then(Key::new))
}

/// Saves the key, replacing any earlier one. Uses the keyring when it
/// works, else the 0600 file. `None` if neither worked.
pub async fn store(key: &Key) -> Option<Storage> {
    match to_keyring(APP_ID, key).await {
        Ok(()) => {
            // A key saved to the file earlier would otherwise come back if
            // the keyring is ever unavailable.
            if let Some(p) = file_path() {
                let _ = tokio::fs::remove_file(p).await;
            }
            return Some(Storage::Keyring);
        }
        Err(e) => tracing::warn!("keyring unavailable, using the private file: {e}"),
    }
    let path = file_path()?;
    let key = key.clone();
    let written = tokio::task::spawn_blocking(move || write_private(&path, key.expose())).await;
    match written {
        Ok(Ok(())) => Some(Storage::File),
        Ok(Err(e)) => {
            tracing::error!("saving the key file: {e}");
            None
        }
        Err(e) => {
            tracing::error!("saving the key file: {e}");
            None
        }
    }
}

#[expect(clippy::result_large_err, reason = "oo7's own error type")]
async fn to_keyring(app: &str, key: &Key) -> oo7::Result<()> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    keyring.create_item(LABEL, &attributes(app), key.expose(), true).await
}

/// Writes `text` to `path` with mode 0600 in a 0700 directory.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    // An existing file keeps its old mode on open; make sure.
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(text.as_bytes())
}

/// Replaces the value of every `appid=` in `s` before it reaches a log.
pub fn redact(s: &str) -> String {
    const NEEDLE: &str = "appid=";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(NEEDLE) {
        let after = i + NEEDLE.len();
        out.push_str(&rest[..after]);
        out.push_str("<redacted>");
        let tail = &rest[after..];
        let end = tail.find(['&', '#', ' ', ')', '"', '\'', '\n']).unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_never_shows() {
        let k = Key::new("  0123456789abcdef0123456789abcdef\n").unwrap();
        assert_eq!(k.expose(), "0123456789abcdef0123456789abcdef");
        assert_eq!(format!("{k:?}"), "Key(<redacted>)");
        assert_eq!(Key::new(" \t"), None);
    }

    #[test]
    fn shape() {
        assert!(looks_valid("0123456789abcdef0123456789ABCDEF"));
        assert!(looks_valid(" 0123456789abcdef0123456789abcdef "));
        assert!(!looks_valid("0123456789abcdef0123456789abcde"));
        assert!(!looks_valid("0123456789abcdef0123456789abcdeg"));
        assert!(!looks_valid(""));
    }

    #[test]
    fn redaction() {
        assert_eq!(
            redact("GET https://api.openweathermap.org/data/2.5/weather?lat=0&appid=abc123&units=metric"),
            "GET https://api.openweathermap.org/data/2.5/weather?lat=0&appid=<redacted>&units=metric"
        );
        assert_eq!(redact("x?appid=abc (y) appid=def"), "x?appid=<redacted> (y) appid=<redacted>");
        assert_eq!(redact("nothing here"), "nothing here");
    }

    #[test]
    fn private_file() {
        let dir = std::env::temp_dir().join(format!("weather-key-test-{}", std::process::id()));
        let path = dir.join("sub").join("api-key");
        write_private(&path, "secret").unwrap();
        // Again, over an existing file with a wider mode.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, "secret2").unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret2");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Saves, replaces, reads back and deletes a key in the real Secret
    /// Service, under a test attribute so the applet's own entry is untouched.
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the session keyring"]
    async fn keyring_roundtrip() {
        let app = concat!("io.github.dc.CosmicAppletWeather", ".test");
        let a = Key::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let b = Key::new("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        to_keyring(app, &a).await.unwrap();
        to_keyring(app, &b).await.unwrap();
        assert_eq!(from_keyring(app).await.unwrap(), Some(b));
        let keyring = oo7::Keyring::new().await.unwrap();
        assert_eq!(keyring.search_items(&attributes(app)).await.unwrap().len(), 1, "replace keeps one item");
        keyring.delete(&attributes(app)).await.unwrap();
        assert_eq!(from_keyring(app).await.unwrap(), None);
    }
}
