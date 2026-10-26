//! The applet's own Claude sign-in (SPEC §3.4).
//!
//! OAuth with PKCE against claude.ai, using Claude Code's public client and
//! a loopback redirect. The result is a login of its own, separate from
//! Claude Code's: the applet refreshes it before it expires, so it keeps
//! working when Claude Code isn't run on this computer. Claude Code's
//! credentials file is never touched. The session lives in the keyring.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::auth::{Account, Token, plan_label, redact};
use crate::config::APP_ID;

/// Claude Code's public OAuth client.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
/// All the usage endpoint needs.
const SCOPE: &str = "user:profile";
/// Refresh this long before the access token expires.
const MARGIN: TimeDelta = TimeDelta::minutes(5);
/// Give up on the browser after this long.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const LABEL: &str = "AI Usage: Claude sign-in";

/// The applet's login. `Debug` never shows the tokens.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    access_token: String,
    refresh_token: String,
    /// Milliseconds since the epoch.
    expires_at: i64,
    pub email: Option<String>,
    pub plan: Option<String>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("expires_at", &self.expires_at).field("email", &self.email).field("plan", &self.plan).finish_non_exhaustive()
    }
}

impl Session {
    pub fn account(&self) -> Account {
        Account { plan: self.plan.clone(), email: self.email.clone() }
    }

    pub fn token(&self) -> Token {
        Token::new(self.access_token.clone())
    }

    /// The access token has expired or will within [`MARGIN`].
    pub fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        DateTime::from_timestamp_millis(self.expires_at).is_none_or(|t| t - MARGIN <= now)
    }
}

/// Why signing in didn't finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInError {
    /// The browser didn't come back in time.
    TimedOut,
    /// The user declined, or claude.ai refused the request.
    Denied,
    /// Couldn't listen for the redirect, or open the browser.
    Local,
    /// The token exchange failed.
    Exchange,
}

/// Why a refresh didn't work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshError {
    /// The refresh token is no longer valid: sign in again.
    Revoked,
    /// Network or server trouble: try later.
    Offline,
}

// ---- signing in -------------------------------------------------------------------

/// Opens the browser at claude.ai's sign-in page and waits for it to
/// redirect back to a loopback listener, then exchanges the code.
pub async fn sign_in(client: reqwest::Client) -> Result<Session, SignInError> {
    let v4 = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
        tracing::error!("listening for the sign-in redirect: {e}");
        SignInError::Local
    })?;
    let port = v4.local_addr().map_err(|_| SignInError::Local)?.port();
    // `localhost` may resolve to ::1 first.
    let v6 = TcpListener::bind(("::1", port)).await.ok();
    let redirect_uri = format!("http://localhost:{port}/callback");
    let verifier = random_string();
    let state = random_string();
    let url = authorize_url(&redirect_uri, &challenge(&verifier), &state);

    if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
        tracing::error!("opening the browser: {e}");
        return Err(SignInError::Local);
    }
    tracing::debug!(port, "waiting for the sign-in redirect");

    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_code(&v4, v6.as_ref(), &state)).await.map_err(|_| SignInError::TimedOut)??;
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": redirect_uri,
        "client_id": CLIENT_ID,
        "code_verifier": verifier,
        "state": state,
    });
    let json = match post_token(&client, &body).await {
        Ok(j) => j,
        Err(status) => {
            tracing::warn!(status, "sign-in token exchange failed");
            return Err(SignInError::Exchange);
        }
    };
    let mut session = session_from(&json, None).ok_or(SignInError::Exchange)?;
    profile(&client, &mut session).await;
    Ok(session)
}

fn authorize_url(redirect_uri: &str, challenge: &str, state: &str) -> String {
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("valid URL");
    url.query_pairs_mut()
        .append_pair("code", "true")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    url.into()
}

/// 32 random bytes, base64url.
fn random_string() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS random source");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The PKCE S256 challenge for `verifier`.
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Accepts connections until one is the redirect carrying our `state`.
async fn wait_for_code(v4: &TcpListener, v6: Option<&TcpListener>, state: &str) -> Result<String, SignInError> {
    loop {
        let accepted = match v6 {
            Some(v6) => tokio::select! {
                r = v4.accept() => r,
                r = v6.accept() => r,
            },
            None => v4.accept().await,
        };
        let Ok((stream, _)) = accepted else { continue };
        if let Some(result) = handle(stream, state).await {
            return result;
        }
    }
}

/// Answers one request. `Some` once the redirect has arrived.
async fn handle(mut stream: TcpStream, state: &str) -> Option<Result<String, SignInError>> {
    let mut buf = vec![0u8; 8192];
    let mut len = 0;
    while len < buf.len() {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf[len..])).await {
            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
            Ok(Ok(n)) => len += n,
        }
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf[..len]);
    let target = head.lines().next().and_then(|l| l.split(' ').nth(1)).unwrap_or("");
    let result = parse_callback(target, state);
    let (status, text) = match &result {
        None => ("404 Not Found", "Not found."),
        Some(Ok(_)) => ("200 OK", "Signed in. You can close this tab and go back to the AI Usage applet."),
        Some(Err(_)) => ("400 Bad Request", "Sign-in didn't finish. Close this tab and try again from the AI Usage applet."),
    };
    let html = format!("<!doctype html><meta charset=utf-8><title>AI Usage</title><body style=\"font:16px system-ui;margin:3em\"><p>{text}</p>");
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}", html.len());
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
    result
}

/// The code from a `/callback?code=…&state=…` request target. `None` for
/// any other path, or a callback with the wrong state.
fn parse_callback(target: &str, state: &str) -> Option<Result<String, SignInError>> {
    let url = url::Url::parse("http://localhost").ok()?.join(target).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let get = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
    if get("state").as_deref() != Some(state) {
        tracing::warn!("sign-in redirect with the wrong state, ignored");
        return None;
    }
    if let Some(e) = get("error") {
        tracing::warn!(error = %e, "sign-in refused");
        return Some(Err(SignInError::Denied));
    }
    // The manual flow appends `#state` to the code; the loopback one shouldn't.
    let code = get("code")?;
    let code = code.split('#').next().unwrap_or_default().to_owned();
    Some(if code.is_empty() { Err(SignInError::Denied) } else { Ok(code) })
}

// ---- refreshing -------------------------------------------------------------------

/// Swaps the refresh token for a new access token (and, usually, a new
/// refresh token). Keeps the email and plan, re-reading them if it can.
pub async fn refresh(client: &reqwest::Client, s: &Session) -> Result<Session, RefreshError> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": s.refresh_token,
        "client_id": CLIENT_ID,
        "scope": SCOPE,
    });
    match post_token(client, &body).await {
        Ok(json) => {
            let mut next = session_from(&json, Some(s)).ok_or(RefreshError::Offline)?;
            profile(client, &mut next).await;
            tracing::debug!("applet login renewed");
            Ok(next)
        }
        // invalid_grant and friends: the refresh token is dead.
        Err(400 | 401 | 403) => Err(RefreshError::Revoked),
        Err(status) => {
            tracing::debug!(status, "refresh failed");
            Err(RefreshError::Offline)
        }
    }
}

/// POSTs to the token endpoint. The JSON on 200, else the status (0 when
/// there was no response).
async fn post_token(client: &reqwest::Client, body: &Value) -> Result<Value, u16> {
    let response = client.post(TOKEN_URL).json(body).send().await.map_err(|e| {
        tracing::debug!("token request failed: {}", redact(&e.without_url().to_string()));
        0u16
    })?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(status);
    }
    response.json::<Value>().await.map_err(|_| 0)
}

/// A session from a token response. A refresh may leave out the refresh
/// token or the account; `previous` fills those in.
fn session_from(json: &Value, previous: Option<&Session>) -> Option<Session> {
    let access_token = json.get("access_token").and_then(Value::as_str).filter(|t| !t.is_empty())?.to_owned();
    let refresh_token =
        json.get("refresh_token").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_owned).or_else(|| previous.map(|p| p.refresh_token.clone()))?;
    let expires_in = json.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
    let email = json
        .pointer("/account/email_address")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| previous.and_then(|p| p.email.clone()));
    Some(Session {
        access_token,
        refresh_token,
        expires_at: (Utc::now() + TimeDelta::seconds(expires_in)).timestamp_millis(),
        email,
        plan: previous.and_then(|p| p.plan.clone()),
    })
}

/// Fills in the email and plan from the profile endpoint. Best effort.
async fn profile(client: &reqwest::Client, s: &mut Session) {
    let response = client.get(PROFILE_URL).bearer_auth(&s.access_token).header("anthropic-beta", "oauth-2025-04-20").send().await;
    let Ok(response) = response.and_then(reqwest::Response::error_for_status) else { return };
    let Ok(json) = response.json::<Value>().await else { return };
    let (email, plan) = parse_profile(&json);
    s.email = email.or(s.email.take());
    s.plan = plan.or(s.plan.take());
}

/// `account.email`, and the plan from `organization.organization_type`
/// ("claude_max" → "Max").
fn parse_profile(json: &Value) -> (Option<String>, Option<String>) {
    let email = json.pointer("/account/email").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    let plan = json
        .pointer("/organization/organization_type")
        .and_then(Value::as_str)
        .map(|t| t.strip_prefix("claude_").unwrap_or(t))
        .filter(|t| !t.is_empty())
        .map(plan_label);
    (email, plan)
}

// ---- keyring ----------------------------------------------------------------------

fn attributes() -> [(&'static str, &'static str); 2] {
    [("application", APP_ID), ("kind", "claude-oauth")]
}

/// The saved session, if there is one.
pub async fn load() -> Option<Session> {
    match from_keyring().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("keyring unavailable: {e}");
            None
        }
    }
}

#[expect(clippy::result_large_err, reason = "oo7's own error type")]
async fn from_keyring() -> oo7::Result<Option<Session>> {
    let keyring = oo7::Keyring::new().await?;
    keyring.unlock().await?;
    let Some(item) = keyring.search_items(&attributes()).await?.into_iter().next() else { return Ok(None) };
    if item.is_locked().await? {
        item.unlock().await?;
    }
    let secret = item.secret().await?;
    // Never log the parse error: it can quote the secret.
    Ok(serde_json::from_slice(secret.as_bytes()).ok())
}

/// Saves the session, replacing any earlier one.
pub async fn save(s: &Session) {
    let Ok(json) = serde_json::to_string(s) else { return };
    let r: oo7::Result<()> = async {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        keyring.create_item(LABEL, &attributes(), json.as_bytes(), true).await
    }
    .await;
    if let Err(e) = r {
        tracing::error!("saving the sign-in to the keyring: {e}");
    }
}

/// Removes the saved session.
pub async fn forget() {
    let r: oo7::Result<()> = async {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        keyring.delete(&attributes()).await
    }
    .await;
    if let Err(e) = r {
        tracing::warn!("removing the sign-in from the keyring: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge() {
        // RFC 7636 appendix B.
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let a = random_string();
        assert_eq!(a.len(), 43);
        assert_ne!(a, random_string());
    }

    #[test]
    fn authorize_url_shape() {
        let u = url::Url::parse(&authorize_url("http://localhost:4242/callback", "CH", "ST")).unwrap();
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(u.host_str(), Some("claude.com"));
        assert_eq!(q["client_id"], CLIENT_ID);
        assert_eq!(q["redirect_uri"], "http://localhost:4242/callback");
        assert_eq!(q["scope"], "user:profile");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!((q["code_challenge"].as_str(), q["state"].as_str()), ("CH", "ST"));
    }

    #[test]
    fn callback_parsing() {
        assert_eq!(parse_callback("/callback?code=abc&state=s1", "s1"), Some(Ok("abc".into())));
        assert_eq!(parse_callback("/callback?code=abc%23s1&state=s1", "s1"), Some(Ok("abc".into())));
        assert_eq!(parse_callback("/callback?error=access_denied&state=s1", "s1"), Some(Err(SignInError::Denied)));
        assert_eq!(parse_callback("/callback?code=abc&state=other", "s1"), None);
        assert_eq!(parse_callback("/favicon.ico", "s1"), None);
        assert_eq!(parse_callback("", "s1"), None);
    }

    #[test]
    fn token_responses() {
        let first = serde_json::json!({
            "access_token": "sk-ant-oat01-A", "refresh_token": "sk-ant-ort01-A", "expires_in": 28800,
            "account": { "uuid": "u", "email_address": "dc@example.com" }
        });
        let s = session_from(&first, None).unwrap();
        assert_eq!(s.email.as_deref(), Some("dc@example.com"));
        assert!(!s.needs_refresh(Utc::now()));
        assert!(s.needs_refresh(Utc::now() + TimeDelta::hours(8) - TimeDelta::minutes(4)));
        // A refresh without a new refresh token or account keeps the old ones.
        let s = Session { plan: Some("Max".into()), ..s };
        let next = session_from(&serde_json::json!({ "access_token": "sk-ant-oat01-B", "expires_in": 60 }), Some(&s)).unwrap();
        assert_eq!((next.refresh_token.as_str(), next.email.as_deref(), next.plan.as_deref()), ("sk-ant-ort01-A", Some("dc@example.com"), Some("Max")));
        assert!(next.needs_refresh(Utc::now()));
        assert_eq!(session_from(&serde_json::json!({ "access_token": "x" }), None), None);
    }

    #[test]
    fn profile_parsing() {
        let v = serde_json::json!({ "account": { "email": "dc@example.com" }, "organization": { "organization_type": "claude_max" } });
        assert_eq!(parse_profile(&v), (Some("dc@example.com".into()), Some("Max".into())));
        assert_eq!(parse_profile(&serde_json::json!({})), (None, None));
    }

    #[test]
    fn tokens_never_printed() {
        let s = session_from(&serde_json::json!({ "access_token": "sk-ant-oat01-A", "refresh_token": "sk-ant-ort01-A" }), None).unwrap();
        assert!(!format!("{s:?}").contains("sk-ant-"));
    }
}
