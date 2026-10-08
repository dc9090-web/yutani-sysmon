//! Claude Code's login, read-only (SPEC §3).
//!
//! The applet never writes, refreshes or rotates these files. It reads the
//! credentials before each request and drops the token once the request is
//! done.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc};
use serde_json::Value;

/// An OAuth access token. `Debug` never shows it, and there is no `Display`.
#[derive(Clone)]
pub struct Token(String);

impl Token {
    /// The raw value, for the `Authorization` header only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// Replaces anything that looks like a token (`sk-ant-…`) before text reaches a log.
pub fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("sk-ant-") {
        out.push_str(&rest[..i]);
        out.push_str("<redacted>");
        let tail = &rest[i..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Where Claude Code keeps its login.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Paths {
    pub credentials: PathBuf,
    /// `.claude.json` candidates for the account email, in order.
    pub account: Vec<PathBuf>,
}

impl Paths {
    pub fn discover() -> Option<Self> {
        Self::from(std::env::var_os("CLAUDE_CONFIG_DIR"), std::env::home_dir())
    }

    /// `$CLAUDE_CONFIG_DIR/.credentials.json`, else `~/.claude/.credentials.json`.
    fn from(config_dir: Option<OsString>, home: Option<PathBuf>) -> Option<Self> {
        let config_dir = config_dir.filter(|d| !d.is_empty()).map(PathBuf::from);
        let credentials = match (&config_dir, &home) {
            (Some(d), _) => d.join(".credentials.json"),
            (None, Some(h)) => h.join(".claude/.credentials.json"),
            (None, None) => return None,
        };
        let account = config_dir.iter().map(|d| d.join(".claude.json")).chain(home.iter().map(|h| h.join(".claude.json"))).collect();
        Some(Self { credentials, account })
    }
}

/// Why there's no usable login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoLogin {
    /// No credentials file, or no Claude login in it.
    Missing,
    /// The login lacks the `user:profile` scope (`login-no-profile`).
    NoProfileScope,
    /// The file exists but isn't JSON in the expected shape.
    Unreadable,
}

/// What the header shows: no secrets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    /// "Max", "Pro"…
    pub plan: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Login {
    pub token: Token,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum Auth {
    Ready(Account, Login),
    Expired(Account),
    NotSignedIn(NoLogin),
}

/// Reads the login. Only ever opens the files for reading.
pub fn read(paths: &Paths, now: DateTime<Utc>) -> Auth {
    let bytes = match std::fs::read(&paths.credentials) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Auth::NotSignedIn(NoLogin::Missing),
        Err(e) => {
            tracing::warn!("reading {}: {}", paths.credentials.display(), e.kind());
            return Auth::NotSignedIn(NoLogin::Unreadable);
        }
    };
    let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
        // Never log the parse error: it can quote the file.
        tracing::warn!("{} is not valid JSON", paths.credentials.display());
        return Auth::NotSignedIn(NoLogin::Unreadable);
    };
    let Some(oauth) = json.get("claudeAiOauth").filter(|v| v.is_object()) else {
        return Auth::NotSignedIn(NoLogin::Missing);
    };
    let Some(token) = oauth.get("accessToken").and_then(Value::as_str).filter(|t| !t.is_empty()) else {
        return Auth::NotSignedIn(NoLogin::Missing);
    };
    let scopes = oauth.get("scopes").and_then(Value::as_array);
    if !scopes.is_some_and(|s| s.iter().any(|x| x.as_str() == Some("user:profile"))) {
        return Auth::NotSignedIn(NoLogin::NoProfileScope);
    }
    let account =
        Account { plan: oauth.get("subscriptionType").and_then(Value::as_str).filter(|s| !s.is_empty()).map(plan_label), email: read_email(&paths.account) };
    let expires_at = oauth.get("expiresAt").and_then(Value::as_i64).and_then(DateTime::from_timestamp_millis);
    if expires_at.is_some_and(|t| t <= now) {
        return Auth::Expired(account);
    }
    Auth::Ready(account, Login { token: Token(token.to_owned()), expires_at })
}

/// `oauthAccount.emailAddress` from the first `.claude.json` that has one.
fn read_email(candidates: &[PathBuf]) -> Option<String> {
    candidates.iter().find_map(|p| {
        let json: Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
        json.pointer("/oauthAccount/emailAddress").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned)
    })
}

/// "max" → "Max", "team_premium" → "Team premium".
fn plan_label(s: &str) -> String {
    let s = s.replace('_', " ");
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

// ---- watching ---------------------------------------------------------------------

/// Size and mtime of the credentials file, or `None` if it's absent.
fn signature(p: &Path) -> Option<(u64, SystemTime)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.len(), m.modified().ok()?))
}

/// Yields `()` whenever the credentials file appears, changes or goes away.
///
/// inotify on its directory catches changes at once; a 30 s poll of its
/// mtime covers the directory not existing yet, or a missed event.
pub fn watch(credentials: &Path) -> impl cosmic::iced::futures::Stream<Item = ()> + use<> {
    let path = credentials.to_owned();
    cosmic::iced::stream::channel(1, async move |mut output| {
        use notify::Watcher;
        let (tx, mut rx) = mpsc::unbounded::<()>();
        let name = path.file_name().map(ToOwned::to_owned);
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && ev.paths.iter().any(|p| p.file_name() == name.as_deref())
            {
                let _ = tx.unbounded_send(());
            }
        })
        .ok();
        let mut watching = false;
        let mut last = signature(&path);
        loop {
            if !watching
                && let (Some(w), Some(dir)) = (watcher.as_mut(), path.parent())
                && dir.is_dir()
            {
                watching = w.watch(dir, notify::RecursiveMode::NonRecursive).is_ok();
                tracing::debug!(watching, "credentials watch");
            }
            let timeout = tokio::time::sleep(Duration::from_secs(30));
            tokio::select! {
                Some(()) = rx.next() => {
                    // Let a write finish, then fold the burst of events.
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    while rx.try_recv().is_ok() {}
                }
                () = timeout => {}
            }
            let now = signature(&path);
            if now != last {
                last = now;
                tracing::debug!("credentials file changed");
                if output.send(()).await.is_err() {
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude-home");

    /// 2026-10-08T10:00:00Z, the fixtures' "now".
    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_791_453_600_000).unwrap()
    }

    fn home(name: &str) -> Paths {
        Paths::from(None, Some(PathBuf::from(FIXTURES).join(name))).unwrap()
    }

    #[test]
    fn valid_home() {
        let Auth::Ready(account, login) = read(&home("valid"), now()) else { panic!("valid home should be signed in") };
        assert_eq!(account, Account { plan: Some("Max".into()), email: Some("dc@example.com".into()) });
        assert_eq!(login.expires_at, DateTime::from_timestamp_millis(1_791_460_000_000));
        assert!(login.token.expose().starts_with("sk-ant-oat01-FIXTURE"));
    }

    #[test]
    fn expired_home() {
        assert!(matches!(read(&home("expired"), now()), Auth::Expired(Account { plan: Some(_), email: None })));
    }

    #[test]
    fn missing_scope_home() {
        assert!(matches!(read(&home("missing-scope"), now()), Auth::NotSignedIn(NoLogin::NoProfileScope)));
    }

    #[test]
    fn no_home() {
        assert!(matches!(read(&home("does-not-exist"), now()), Auth::NotSignedIn(NoLogin::Missing)));
    }

    #[test]
    fn config_dir_wins() {
        let dir = PathBuf::from(FIXTURES).join("expired/.claude");
        let p = Paths::from(Some(dir.clone().into_os_string()), Some(PathBuf::from(FIXTURES).join("valid"))).unwrap();
        assert_eq!(p.credentials, dir.join(".credentials.json"));
        assert!(matches!(read(&p, now()), Auth::Expired(_)));
        // An empty variable is ignored.
        let p = Paths::from(Some(OsString::new()), Some(PathBuf::from("/h"))).unwrap();
        assert_eq!(p.credentials, PathBuf::from("/h/.claude/.credentials.json"));
        assert_eq!(Paths::from(None, None), None);
    }

    #[test]
    fn expiry_boundary() {
        let at = DateTime::from_timestamp_millis(1_791_460_000_000).unwrap();
        assert!(matches!(read(&home("valid"), at), Auth::Expired(_)));
        assert!(matches!(read(&home("valid"), at - chrono::TimeDelta::milliseconds(1)), Auth::Ready(..)));
    }

    #[test]
    fn token_never_printed() {
        let Auth::Ready(_, login) = read(&home("valid"), now()) else { panic!() };
        let shown = format!("{login:?} {:?}", read(&home("valid"), now()));
        assert!(!shown.contains("sk-ant-"), "{shown}");
    }

    #[test]
    fn redaction() {
        assert_eq!(redact("Bearer sk-ant-oat01-abc_DEF-9 failed"), "Bearer <redacted> failed");
        assert_eq!(redact("a sk-ant-x, b sk-ant-y"), "a <redacted>, b <redacted>");
        assert_eq!(redact("nothing here"), "nothing here");
    }

    #[test]
    fn plan_labels() {
        assert_eq!(plan_label("max"), "Max");
        assert_eq!(plan_label("pro"), "Pro");
        assert_eq!(plan_label("team_premium"), "Team premium");
        assert_eq!(plan_label("Enterprise"), "Enterprise");
    }
}
