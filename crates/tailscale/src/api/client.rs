//! HTTP/1 to `tailscaled` over its Unix socket: one short connection per
//! request, and one long one for the event bus.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::Stream;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

use super::types::{MaskedPrefs, Notify, Prefs, Status, Suggestion};

const DEFAULT_SOCKET: &str = "/var/run/tailscale/tailscaled.sock";
/// Initial state, initial prefs and initial health (`ipn.NotifyWatchOpt` bits 1, 2 and 7).
const BUS_MASK: u32 = 2 | 4 | 128;
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// A bus line longer than this is not one we understand: drop the connection.
const LINE_LIMIT: usize = 8 << 20;
const ERROR_TEXT: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// No daemon on the socket.
    Unavailable,
    /// 403: this user isn't the operator.
    Denied,
    /// Another status, with the start of its text.
    Other(u16, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BusEvent {
    Notify(Notify),
    /// The bus is not connected; a reconnect is under way.
    Down,
}

/// `$TS_SOCKET`, else tailscaled's default socket.
pub fn socket_path() -> PathBuf {
    std::env::var_os("TS_SOCKET").filter(|v| !v.is_empty()).map_or_else(|| PathBuf::from(DEFAULT_SOCKET), PathBuf::from)
}

pub async fn status() -> Result<Status, ApiError> {
    status_at(&socket_path()).await
}

pub async fn patch_prefs(prefs: MaskedPrefs) -> Result<Prefs, ApiError> {
    patch_prefs_at(&socket_path(), prefs).await
}

/// The daemon's recommended exit node, if it has one.
pub async fn suggest_exit_node() -> Result<Option<Suggestion>, ApiError> {
    suggest_exit_node_at(&socket_path()).await
}

/// Starts a sign-in; the URL to open then arrives on the bus.
pub async fn login_interactive() -> Result<(), ApiError> {
    read(send(&socket_path(), Method::POST, "login-interactive", None).await?).await.map(drop)
}

pub fn watch() -> impl Stream<Item = BusEvent> {
    watch_at(socket_path())
}

async fn send(socket: &Path, method: Method, endpoint: &str, body: Option<String>) -> Result<Response<Incoming>, ApiError> {
    let stream = UnixStream::connect(socket).await.map_err(|_| ApiError::Unavailable)?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.map_err(|_| ApiError::Unavailable)?;
    // The connection drives the reply's body, so it lives until that is read or dropped.
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let request = Request::builder()
        .method(method)
        .uri(format!("/localapi/v0/{endpoint}"))
        .header(hyper::header::HOST, "local-tailscaled.sock")
        .body(Full::new(Bytes::from(body.unwrap_or_default())))
        .expect("a valid request");
    sender.send_request(request).await.map_err(|_| ApiError::Unavailable)
}

/// The body of a 2xx reply; other statuses become errors.
async fn read(response: Response<Incoming>) -> Result<Bytes, ApiError> {
    let code = response.status();
    let body = response.into_body().collect().await.map_err(|_| ApiError::Unavailable)?.to_bytes();
    if code.is_success() {
        Ok(body)
    } else if code == StatusCode::FORBIDDEN {
        Err(ApiError::Denied)
    } else {
        Err(ApiError::Other(code.as_u16(), String::from_utf8_lossy(&body).trim().chars().take(ERROR_TEXT).collect()))
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|_| ApiError::Other(200, "unreadable reply".to_owned()))
}

async fn status_at(socket: &Path) -> Result<Status, ApiError> {
    parse(&read(send(socket, Method::GET, "status", None).await?).await?)
}

async fn patch_prefs_at(socket: &Path, prefs: MaskedPrefs) -> Result<Prefs, ApiError> {
    let body = serde_json::to_string(&prefs).expect("prefs serialise");
    parse(&read(send(socket, Method::PATCH, "prefs", Some(body)).await?).await?)
}

async fn suggest_exit_node_at(socket: &Path) -> Result<Option<Suggestion>, ApiError> {
    match read(send(socket, Method::GET, "suggest-exit-node", None).await?).await {
        Ok(body) => Ok(parse::<Suggestion>(&body).ok().filter(|s| !s.id.is_empty())),
        Err(ApiError::Unavailable) => Err(ApiError::Unavailable),
        Err(_) => Ok(None),
    }
}

struct Watch {
    socket: PathBuf,
    body: Option<Incoming>,
    buf: Vec<u8>,
    ready: VecDeque<Notify>,
    /// The wait before the next connection attempt; `None` for the first.
    retry: Option<Duration>,
}

impl Watch {
    async fn next(&mut self) -> BusEvent {
        loop {
            if let Some(n) = self.ready.pop_front() {
                return BusEvent::Notify(n);
            }
            let Some(body) = self.body.as_mut() else {
                if let Some(wait) = self.retry {
                    tokio::time::sleep(wait).await;
                }
                self.retry = Some(self.retry.map_or(RETRY_MIN, |w| (w * 2).min(RETRY_MAX)));
                match send(&self.socket, Method::GET, &format!("watch-ipn-bus?mask={BUS_MASK}"), None).await {
                    Ok(r) if r.status().is_success() => self.body = Some(r.into_body()),
                    _ => return BusEvent::Down,
                }
                continue;
            };
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Some(data) = frame.data_ref() {
                        self.buf.extend_from_slice(data);
                        self.take_lines();
                    }
                    if self.buf.len() <= LINE_LIMIT {
                        continue;
                    }
                }
                Some(Err(_)) | None => {}
            }
            self.body = None;
            self.buf.clear();
            return BusEvent::Down;
        }
    }

    /// Moves every complete line out of the buffer, keeping the ones that parse.
    fn take_lines(&mut self) {
        while let Some(end) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=end).collect();
            match serde_json::from_slice::<Notify>(&line) {
                Ok(n) => {
                    self.retry = Some(RETRY_MIN);
                    self.ready.push_back(n);
                }
                Err(_) if line.iter().all(u8::is_ascii_whitespace) => {}
                Err(_) => tracing::debug!("skipped a {}-byte bus line", line.len()),
            }
        }
    }
}

/// The event bus as an endless stream. When it can't connect or the
/// connection ends it yields `Down`, then retries after 1 s, doubling to 30 s.
fn watch_at(socket: PathBuf) -> impl Stream<Item = BusEvent> {
    let watch = Watch { socket, body: None, buf: Vec::new(), ready: VecDeque::new(), retry: None };
    futures_util::stream::unfold(watch, |mut w| async move { Some((w.next().await, w)) })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use futures_util::StreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::mpsc;

    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn temp_socket() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!("ts-applet-test-{}-{}.sock", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_file(&p);
        p
    }

    async fn read_request(s: &mut UnixStream) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = s.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).into_owned();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let len = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                if body.len() >= len {
                    return text;
                }
            }
            assert!(n > 0, "client closed before sending a full request");
        }
    }

    /// Serves one canned reply per connection, in order, and reports each request.
    /// A reply's parts are written with a pause between them; `hold` keeps the
    /// last connection open afterwards.
    fn serve(replies: Vec<Vec<String>>, hold: bool) -> (PathBuf, mpsc::UnboundedReceiver<String>) {
        let path = temp_socket();
        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let last = replies.len() - 1;
            for (i, parts) in replies.into_iter().enumerate() {
                let (mut s, _) = listener.accept().await.unwrap();
                tx.send(read_request(&mut s).await).unwrap();
                for part in parts {
                    s.write_all(part.as_bytes()).await.unwrap();
                    s.flush().await.unwrap();
                    tokio::task::yield_now().await;
                }
                if hold && i == last {
                    std::future::pending::<()>().await;
                }
            }
        });
        (path, rx)
    }

    fn with_length(code: &str, body: &str) -> Vec<String> {
        vec![format!("HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len())]
    }

    const CHUNKED: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n";

    fn chunk(s: &str) -> String {
        format!("{:x}\r\n{s}\r\n", s.len())
    }

    #[tokio::test]
    async fn status_ok() {
        let (path, mut reqs) = serve(vec![with_length("200 OK", &fixture("status_running.json"))], false);
        assert_eq!(status_at(&path).await.unwrap().peers.len(), 6);
        let req = reqs.recv().await.unwrap();
        assert!(req.starts_with("GET /localapi/v0/status HTTP/1.1\r\n"), "{req}");
        assert!(req.to_ascii_lowercase().contains("host: local-tailscaled.sock\r\n"), "{req}");
    }

    #[tokio::test]
    async fn status_chunked() {
        let body = fixture("status_running.json");
        let (a, rest) = body.split_at(body.len() / 3);
        let (b, c) = rest.split_at(rest.len() / 2);
        let (path, _reqs) = serve(vec![vec![CHUNKED.into(), chunk(a), chunk(b), chunk(c), "0\r\n\r\n".into()]], false);
        assert_eq!(status_at(&path).await.unwrap().peers.len(), 6);
    }

    #[tokio::test]
    async fn patch_denied() {
        let (path, mut reqs) = serve(vec![with_length("403 Forbidden", &fixture("prefs_denied.txt"))], false);
        assert_eq!(patch_prefs_at(&path, MaskedPrefs::want_running(false)).await, Err(ApiError::Denied));
        let req = reqs.recv().await.unwrap();
        assert!(req.starts_with("PATCH /localapi/v0/prefs HTTP/1.1\r\n"), "{req}");
        assert!(req.ends_with(r#"{"WantRunning":false,"WantRunningSet":true}"#), "{req}");
    }

    #[tokio::test]
    async fn patch_ok_returns_prefs() {
        let (path, _reqs) = serve(vec![with_length("200 OK", r#"{"WantRunning":true,"ExitNodeID":"nAAAA2CNTRL","OperatorUser":"daniel"}"#)], false);
        let p = patch_prefs_at(&path, MaskedPrefs::exit_node("nAAAA2CNTRL")).await.unwrap();
        assert_eq!((p.want_running, p.exit_node_id.as_str(), p.operator_user.as_str()), (true, "nAAAA2CNTRL", "daniel"));
    }

    #[tokio::test]
    async fn other_error_keeps_code_and_short_text() {
        let (path, _reqs) = serve(vec![with_length("500 Internal Server Error", &"x".repeat(500))], false);
        assert_eq!(status_at(&path).await, Err(ApiError::Other(500, "x".repeat(120))));
    }

    #[tokio::test]
    async fn no_socket() {
        assert_eq!(status_at(&temp_socket()).await, Err(ApiError::Unavailable));
    }

    #[tokio::test]
    async fn suggest_none_on_error() {
        let (path, _reqs) = serve(vec![with_length("500 Internal Server Error", "no suggestion")], false);
        assert_eq!(suggest_exit_node_at(&path).await, Ok(None));
        let (path, _reqs) = serve(vec![with_length("200 OK", r#"{"ID":"nAAAA2CNTRL","Name":"bravo.example.ts.net."}"#)], false);
        assert_eq!(suggest_exit_node_at(&path).await.unwrap().unwrap().id, "nAAAA2CNTRL");
    }

    fn states(events: &[BusEvent]) -> Vec<Option<u8>> {
        events.iter().map(|e| if let BusEvent::Notify(n) = e { n.state } else { None }).collect()
    }

    #[tokio::test]
    async fn watch_lines_split() {
        let lines = fixture("bus_starting.ndjson");
        let (first, second) = lines.trim_end().split_once('\n').unwrap();
        let (a, b) = first.split_at(first.len() / 2);
        let (path, mut reqs) = serve(vec![vec![CHUNKED.into(), chunk(a), chunk(&format!("{b}\nnot json\n")), chunk(&format!("{second}\n"))]], true);
        let events: Vec<BusEvent> = watch_at(path).take(2).collect().await;
        assert_eq!(states(&events), [Some(5), Some(6)]);
        assert!(reqs.recv().await.unwrap().starts_with("GET /localapi/v0/watch-ipn-bus?mask=134 HTTP/1.1\r\n"));
    }

    #[tokio::test(start_paused = true)]
    async fn watch_ends_then_reconnect() {
        let line = fixture("bus_running.ndjson");
        let one = vec![CHUNKED.to_owned(), chunk(&line), "0\r\n\r\n".to_owned()];
        let (path, _reqs) = serve(vec![one.clone(), one], true);
        let events: Vec<BusEvent> = watch_at(path).take(3).collect().await;
        assert_eq!(events[1], BusEvent::Down);
        assert_eq!(states(&events), [Some(6), None, Some(6)]);
    }

    #[tokio::test(start_paused = true)]
    async fn watch_down_while_no_socket_then_connects() {
        let path = temp_socket();
        let mut w = Box::pin(watch_at(path.clone()));
        assert_eq!(w.next().await, Some(BusEvent::Down));
        assert_eq!(w.next().await, Some(BusEvent::Down));
        let listener = UnixListener::bind(&path).unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read_request(&mut s).await;
            s.write_all(format!("{CHUNKED}{}", chunk(&fixture("bus_stopped.ndjson"))).as_bytes()).await.unwrap();
            std::future::pending::<()>().await;
        });
        loop {
            match w.next().await.unwrap() {
                BusEvent::Down => {}
                BusEvent::Notify(n) => break assert_eq!(n.state, Some(4)),
            }
        }
    }
}
