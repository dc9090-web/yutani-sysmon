//! The D-Bus API (SPEC §12), polkit checks, and the loops that watch the
//! tunnels, renew the port and follow qBittorrent's unit and the browser.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use vpn_common::conf::{self, Conf, ConfSummary};
use vpn_common::dbus::{ACTION_CONTROL, ACTION_IMPORT, OBJECT_PATH, VERSION};
use vpn_common::names::{BROWSER_CONF, P2P_CONF, WEB_IF};
use vpn_common::natpmp::RENEW_SECS;
use vpn_common::status::{self, BrowserState, BrowserStatus, ListenApplied, PortState, QbitState, Status, TunnelState};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{fdo, interface, message::Header};
use zeroize::Zeroizing;

use crate::netns::{BROWSER, P2P, Space, WgStats};
use crate::portmap::{self, Round};
use crate::{browser, netns, qbit, unit};

/// After a failed round: retry this soon while the port was never mapped,
/// and every 60 s once given up (SPEC §4.3).
const PORT_RETRY: Duration = Duration::from_secs(10);
const PORT_FAIL_RETRY: Duration = Duration::from_secs(60);
/// In `error`, re-send the endpoint this often.
const ERROR_RETRY_SECS: u64 = 10;
/// qBittorrent counts as running once its unit has been active this long.
const QBIT_SETTLE: Duration = Duration::from_secs(2);
/// After a launch, qBittorrent counts as starting for this long before it shows up.
const QBIT_APPEAR: Duration = Duration::from_secs(20);
/// The browser gets this long to show up in its namespace after a launch.
const BROWSER_APPEAR: Duration = Duration::from_secs(20);
/// With no browser process for this long, it was closed (a restart hands
/// over to a new process within moments): its tunnel goes down.
const BROWSER_GONE: Duration = Duration::from_secs(3);
const BROWSER_QUIT: Duration = Duration::from_secs(10);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.dc.CosmicVpnHelper1.Error")]
pub enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    InvalidConf(String),
    NotImported(String),
    PermissionDenied(String),
    QbitRunning(String),
    NetlinkFailed(String),
    WebUi(String),
}

/// A tunnel that's switched on, and what it takes to read its state.
#[derive(Default)]
struct Link {
    /// The switch is on (brought up, or adopted at start).
    on: bool,
    conf: Option<Conf>,
    up_since: u64,
    last_retry: u64,
    /// Counters two seconds back, for the rates.
    samples: Vec<(Instant, u64, u64)>,
}

/// What one look at a tunnel's counters says.
struct Seen {
    state: TunnelState,
    error: String,
    /// `None`: the interface is gone, keep the last age.
    handshake_age: Option<u32>,
    /// (rx, tx) in bytes per second, once there are two samples.
    rates: Option<(u64, u64)>,
    /// Stale, or no handshake past the timeout: set the endpoint again.
    nudge: Option<Conf>,
}

impl Link {
    fn start(&mut self) {
        self.on = true;
        self.up_since = netns::now();
    }

    /// The tunnel's state from its counters (`None`: its interface is gone).
    fn observe(&mut self, stats: Option<WgStats>, prev: TunnelState, now: u64, gone: &str) -> Seen {
        let Some(s) = stats else {
            // Still being built just after the switch: not an error yet.
            let building = now.saturating_sub(self.up_since) < status::HANDSHAKE_TIMEOUT_SECS;
            let (state, error) = if building { (TunnelState::Connecting, String::new()) } else { (TunnelState::Error, gone.to_owned()) };
            return Seen { state, error, handshake_age: None, rates: None, nudge: None };
        };
        let state = status::classify(s.latest_handshake, now, self.up_since);
        let error = if state == TunnelState::Error {
            let label = self.conf.as_ref().map_or("the server", |c| c.summary.server_label.as_str());
            format!("No handshake from {label} · check the server or your connection")
        } else {
            String::new()
        };
        let handshake_age = if s.latest_handshake == 0 { 0 } else { now.saturating_sub(s.latest_handshake).min(u64::from(u32::MAX)) as u32 };
        // Rates over a 2 s window.
        let t = Instant::now();
        self.samples.push((t, s.rx_bytes, s.tx_bytes));
        self.samples.retain(|(at, ..)| t.duration_since(*at) <= Duration::from_millis(2500));
        let rates = match (self.samples.first(), self.samples.last()) {
            (Some(first), Some(last)) => {
                let dt = last.0.duration_since(first.0).as_secs_f64();
                (dt > 0.5).then(|| ((last.1.saturating_sub(first.1) as f64 / dt) as u64, (last.2.saturating_sub(first.2) as f64 / dt) as u64))
            }
            _ => None,
        };
        let due = match state {
            TunnelState::Stale => prev != TunnelState::Stale || now.saturating_sub(self.last_retry) >= ERROR_RETRY_SECS,
            TunnelState::Error => now.saturating_sub(self.last_retry) >= ERROR_RETRY_SECS,
            _ => false,
        };
        if due {
            self.last_retry = now;
        }
        Seen { state, error, handshake_age: Some(handshake_age), rates, nudge: if due { self.conf.clone() } else { None } }
    }
}

/// The browser tunnel and the browser in it.
#[derive(Default)]
struct Browser {
    status: BrowserStatus,
    link: Link,
    /// When the browser was last launched: it counts as starting until it appears.
    launched_at: Option<Instant>,
    /// A browser process has been seen in the namespace since the tunnel came up.
    seen: bool,
    /// Since when no browser process is left.
    gone_since: Option<Instant>,
    last_sent: Option<BrowserStatus>,
}

#[derive(Default)]
struct Inner {
    status: Status,
    link: Link,
    endpoint: Option<SocketAddr>,
    browser: Browser,
    ever_mapped: bool,
    port_failures: u32,
    renewed_at: Option<Instant>,
    next_round: Option<Instant>,
    qbit_since: Option<Instant>,
    /// When qBittorrent was last launched: it counts as starting until it appears.
    launched_at: Option<Instant>,
    last_sent: Option<Status>,
}

#[derive(Clone)]
pub struct Helper {
    inner: Arc<Mutex<Inner>>,
}

impl Helper {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())) }
    }

    /// At start: adopt the namespaces left by an earlier run (SPEC §4.1).
    pub async fn adopt(&self) {
        if let Some(conf) = adoptable(&P2P, P2P_CONF).await {
            tracing::info!("adopting the torrent namespace");
            let mut i = self.inner.lock().await;
            i.endpoint = netns::resolve(&conf).ok();
            i.link.conf = Some(conf);
            i.link.start();
            i.status.state = TunnelState::Connecting;
            i.next_round = Some(Instant::now());
        }
        if let Some(conf) = adoptable(&BROWSER, BROWSER_CONF).await {
            tracing::info!("adopting the browser namespace");
            // A browser still open in it keeps the tunnel; an empty one is
            // left for the applet to fill or close.
            let seen = !tokio::task::spawn_blocking(browser::pids).await.unwrap_or_default().is_empty();
            let mut i = self.inner.lock().await;
            let b = &mut i.browser;
            b.link.conf = Some(conf);
            b.link.start();
            b.status.state = TunnelState::Connecting;
            b.seen = seen;
        }
    }

    /// The 1 s loop: tunnel state, rates, stale handling, qBittorrent's unit.
    pub async fn watch(self, conn: zbus::Connection) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            self.step(&conn).await;
            self.browser_step(&conn).await;
        }
    }

    async fn step(&self, conn: &zbus::Connection) {
        let on = self.inner.lock().await.link.on;
        let stats = if on { tokio::task::spawn_blocking(|| netns::stats(&P2P)).await.ok().and_then(Result::ok) } else { None };
        let unit = qbit::unit_state(conn).await;
        let pids = tokio::task::spawn_blocking(qbit::pids).await.unwrap_or_default();
        let now = netns::now();
        let mut port_changed = None;
        let (send, retry) = {
            let mut i = self.inner.lock().await;
            let i = &mut *i;
            let mut retry = None;
            if i.link.on {
                let seen = i.link.observe(stats, i.status.state, now, "The torrent tunnel's interface is gone");
                i.status.state = seen.state;
                i.status.error = seen.error;
                if let Some(age) = seen.handshake_age {
                    i.status.handshake_age = age;
                }
                if let Some((rx, tx)) = seen.rates {
                    i.status.rx_bps = rx;
                    i.status.tx_bps = tx;
                }
                retry = seen.nudge;
                i.status.port_renewed_age = i.renewed_at.map_or(0, |r| r.elapsed().as_secs().min(u64::from(u32::MAX)) as u32);
            }
            // qBittorrent: running once its process has been in the namespace for 2 s.
            let prev_qbit = i.status.qbit;
            let launching = i.launched_at.is_some_and(|t| t.elapsed() < QBIT_APPEAR);
            i.status.qbit = if !pids.is_empty() {
                i.launched_at = None;
                match i.qbit_since {
                    Some(since) if since.elapsed() >= QBIT_SETTLE => QbitState::Vpn,
                    Some(_) => QbitState::Starting,
                    None => {
                        i.qbit_since = Some(Instant::now());
                        QbitState::Starting
                    }
                }
            } else {
                match &unit {
                    Some((s, code)) if s == "failed" => {
                        i.launched_at = None;
                        i.status.qbit_exit = *code;
                        QbitState::Stopped
                    }
                    Some((s, _)) if s == "active" || s == "activating" => QbitState::Starting,
                    _ if launching => QbitState::Starting,
                    _ => {
                        if i.launched_at.take().is_some() {
                            tracing::warn!("qBittorrent was launched but never appeared in the namespace");
                        }
                        QbitState::Stopped
                    }
                }
            };
            if i.status.qbit == QbitState::Stopped {
                i.qbit_since = None;
                if prev_qbit != QbitState::Stopped && i.status.listen_port_applied != ListenApplied::Manual {
                    i.status.listen_port_applied = ListenApplied::None;
                }
            }
            if i.status.port_state == PortState::Ok && i.last_sent.as_ref().is_some_and(|s| s.port != i.status.port) {
                port_changed = Some(i.status.port);
            }
            let changed = i.last_sent.as_ref() != Some(&i.status);
            let send = (changed || i.link.on).then(|| i.status.clone());
            if let Some(s) = &send {
                i.last_sent = Some(s.clone());
            }
            (send, retry)
        };
        if let Some(conf) = retry
            && let Some(ep) = nudge(&P2P, conf).await
        {
            self.inner.lock().await.endpoint = Some(ep);
        }
        let emitter = SignalEmitter::new(conn, OBJECT_PATH).expect("valid path");
        if let Some(port) = port_changed {
            let _ = Helper::port_changed(&emitter, port).await;
        }
        if let Some(s) = send {
            let _ = Helper::status_changed(&emitter, s.to_dict()).await;
        }
        if on {
            self.port_step().await;
        }
    }

    /// The browser tunnel's second: its state and rates, and whether the
    /// browser is still there. The tunnel only lives as long as the browser:
    /// once it's closed (or never appears), the namespace goes too.
    async fn browser_step(&self, conn: &zbus::Connection) {
        let on = self.inner.lock().await.browser.link.on;
        let (stats, pids) = if on {
            let stats = tokio::task::spawn_blocking(|| netns::stats(&BROWSER)).await.ok().and_then(Result::ok);
            (stats, tokio::task::spawn_blocking(browser::pids).await.unwrap_or_default())
        } else {
            (None, Vec::new())
        };
        let now = netns::now();
        let mut close = None;
        let retry = {
            let mut i = self.inner.lock().await;
            let b = &mut i.browser;
            // Switched off while this step was reading.
            if !on || !b.link.on {
                None
            } else {
                let seen = b.link.observe(stats, b.status.state, now, "The browser tunnel's interface is gone");
                b.status.state = seen.state;
                b.status.error = seen.error;
                if let Some(age) = seen.handshake_age {
                    b.status.handshake_age = age;
                }
                if let Some((rx, tx)) = seen.rates {
                    b.status.rx_bps = rx;
                    b.status.tx_bps = tx;
                }
                b.status.browser = if !pids.is_empty() {
                    b.seen = true;
                    b.launched_at = None;
                    b.gone_since = None;
                    BrowserState::Running
                } else if b.launched_at.is_some_and(|t| t.elapsed() < BROWSER_APPEAR) {
                    BrowserState::Starting
                } else if b.launched_at.is_some() {
                    tracing::warn!("the browser was launched but never appeared in the namespace");
                    close = Some("The browser didn't start");
                    BrowserState::Stopped
                } else if b.seen {
                    if b.gone_since.get_or_insert_with(Instant::now).elapsed() >= BROWSER_GONE {
                        tracing::info!("the browser was closed");
                        close = Some("");
                    }
                    BrowserState::Stopped
                } else {
                    BrowserState::Stopped
                };
                seen.nudge
            }
        };
        if let Some(conf) = retry {
            nudge(&BROWSER, conf).await;
        }
        if let Some(why) = close {
            self.browser_close(conn, why).await;
        }
        let send = {
            let mut i = self.inner.lock().await;
            let b = &mut i.browser;
            let changed = b.last_sent.as_ref() != Some(&b.status);
            let send = (changed || b.link.on).then(|| b.status.clone());
            if let Some(s) = &send {
                b.last_sent = Some(s.clone());
            }
            send
        };
        if let Some(s) = send {
            let emitter = SignalEmitter::new(conn, OBJECT_PATH).expect("valid path");
            let _ = Helper::browser_changed(&emitter, s.to_dict()).await;
        }
    }

    /// Quits the browser and takes its tunnel down; `why` stays in the
    /// status as the reason, if it wasn't asked for.
    async fn browser_close(&self, conn: &zbus::Connection, why: &str) {
        {
            // Off first, so a step that runs meanwhile leaves it alone.
            let mut i = self.inner.lock().await;
            let last_sent = i.browser.last_sent.take();
            i.browser = Browser { status: BrowserStatus { error: why.to_owned(), ..BrowserStatus::default() }, last_sent, ..Browser::default() };
        }
        browser::stop(conn, BROWSER_QUIT).await;
        if let Err(e) = netns::down(&BROWSER).await {
            tracing::error!("taking the browser tunnel down: {e}");
        }
        tracing::info!("browser tunnel down");
    }

    /// A NAT-PMP round when one is due and the tunnel is up.
    async fn port_step(&self) {
        let due = {
            let i = self.inner.lock().await;
            i.link.on && i.status.state.up() && i.next_round.is_none_or(|t| Instant::now() >= t)
        };
        if !due {
            return;
        }
        {
            let mut i = self.inner.lock().await;
            // A round takes a few seconds at most; don't start another meanwhile.
            i.next_round = Some(Instant::now() + Duration::from_secs(30));
            if i.status.port_state == PortState::Off {
                i.status.port_state = PortState::Req;
            }
        }
        let round = tokio::task::spawn_blocking(portmap::round).await.unwrap_or(Round::NoResponse);
        let mut i = self.inner.lock().await;
        match round {
            Round::Port(p) => {
                if i.status.port != p {
                    tracing::info!(port = p, "forwarded port");
                }
                i.status.port = p;
                i.status.port_state = PortState::Ok;
                i.ever_mapped = true;
                i.port_failures = 0;
                i.renewed_at = Some(Instant::now());
                i.next_round = Some(Instant::now() + Duration::from_secs(RENEW_SECS));
            }
            fail => {
                i.port_failures += 1;
                tracing::debug!(?fail, failures = i.port_failures, "NAT-PMP round failed");
                // The first round refused or unanswered: not a P2P server.
                let give_up = (!i.ever_mapped && matches!(fail, Round::Refused(_))) || i.port_failures >= 3;
                if give_up {
                    i.status.port_state = PortState::Fail;
                    i.status.port = 0;
                    i.next_round = Some(Instant::now() + PORT_FAIL_RETRY);
                } else {
                    i.next_round = Some(Instant::now() + PORT_RETRY);
                }
            }
        }
    }

    async fn reset(&self) {
        let mut i = self.inner.lock().await;
        let qbit = i.status.qbit;
        let exit = i.status.qbit_exit;
        let browser = std::mem::take(&mut i.browser);
        *i = Inner { status: Status { qbit, qbit_exit: exit, ..Status::default() }, last_sent: i.last_sent.take(), browser, ..Inner::default() };
    }
}

/// The conf of a namespace that's still up with its tunnel inside.
async fn adoptable(sp: &'static Space, conf: &'static str) -> Option<Conf> {
    if !netns::exists(sp) {
        return None;
    }
    let conf = load_conf(conf)?;
    tokio::task::spawn_blocking(move || netns::stats(sp)).await.ok()?.ok()?;
    Some(conf)
}

/// Re-resolves a hostname endpoint and sets it again (SPEC §4.1, stale).
async fn nudge(sp: &'static Space, conf: Conf) -> Option<SocketAddr> {
    let r = tokio::task::spawn_blocking(move || {
        let ep = netns::resolve(&conf)?;
        netns::reset_endpoint(sp, &conf, ep).map(|()| ep)
    })
    .await;
    match r {
        Ok(Ok(ep)) => Some(ep),
        Ok(Err(e)) => {
            tracing::warn!("reconnecting: {e}");
            None
        }
        Err(_) => None,
    }
}

fn load_conf(path: &str) -> Option<Conf> {
    let text = Zeroizing::new(std::fs::read_to_string(path).ok()?);
    conf::parse(&text, path.rsplit('/').next().unwrap_or(path)).ok()
}

/// Writes a conf root-only (0600), atomically.
fn store_conf(path: &str, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let path = std::path::Path::new(path);
    let dir = path.parent().expect("has a parent");
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let tmp = dir.join(".conf.tmp");
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

fn summary_dict(s: &ConfSummary) -> HashMap<String, OwnedValue> {
    let v = |x: Value<'_>| OwnedValue::try_from(x).expect("plain values convert");
    HashMap::from([
        ("server_label".into(), v(Value::from(s.server_label.clone()))),
        ("endpoint".into(), v(Value::from(s.endpoint.clone()))),
        ("addr_v4".into(), v(Value::from(s.addr_v4.clone()))),
        ("has_v6".into(), v(Value::from(s.has_v6))),
        ("dns".into(), v(Value::from(s.dns.clone()))),
        ("natpmp_hint".into(), v(Value::from(s.natpmp_hint.as_str()))),
        ("moderate_nat_hint".into(), v(Value::from(s.moderate_nat_hint.as_str()))),
    ])
}

/// The caller's uid, from the bus (never from arguments).
async fn caller_uid(conn: &zbus::Connection, hdr: &Header<'_>) -> Result<u32, Error> {
    let sender = hdr.sender().ok_or_else(|| Error::PermissionDenied("no sender".into()))?;
    let dbus = fdo::DBusProxy::new(conn).await?;
    Ok(dbus.get_connection_unix_user(sender.clone().into()).await.map_err(zbus::Error::from)?)
}

/// Who a launched app runs as (the caller), once `argv[0]` is one it may run.
async fn launcher(conn: &zbus::Connection, hdr: &Header<'_>, argv: &[String]) -> Result<unit::User, Error> {
    let uid = caller_uid(conn, hdr).await?;
    let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)).ok().flatten().ok_or_else(|| Error::PermissionDenied("unknown user".into()))?;
    let Some(argv0) = argv.first() else { return Err(Error::PermissionDenied("empty command".into())) };
    if !unit::argv0_allowed(argv0, uid, &user.dir) {
        return Err(Error::PermissionDenied(format!("{argv0} isn't an allowed command")));
    }
    Ok(unit::User { uid, gid: user.gid.as_raw(), home: user.dir.to_string_lossy().into_owned() })
}

/// Asks polkit whether the caller may do `action` (it may prompt).
async fn authorize(conn: &zbus::Connection, hdr: &Header<'_>, action: &str) -> Result<(), Error> {
    let sender = hdr.sender().ok_or_else(|| Error::PermissionDenied("no sender".into()))?.to_string();
    let authority =
        zbus::Proxy::new(conn, "org.freedesktop.PolicyKit1", "/org/freedesktop/PolicyKit1/Authority", "org.freedesktop.PolicyKit1.Authority").await?;
    let subject = ("system-bus-name", HashMap::from([("name", Value::from(sender))]));
    let details: HashMap<&str, &str> = HashMap::new();
    // Flag 1: allow an authentication dialog.
    let (ok, _challenge, _details): (bool, bool, HashMap<String, String>) =
        authority.call("CheckAuthorization", &(subject, action, details, 1u32, "")).await.map_err(|e| Error::PermissionDenied(format!("polkit: {e}")))?;
    if ok { Ok(()) } else { Err(Error::PermissionDenied("Permission denied".into())) }
}

#[interface(name = "io.github.dc.CosmicVpnHelper1")]
impl Helper {
    #[zbus(property)]
    fn version(&self) -> u32 {
        VERSION
    }

    /// Re-parses and stores the torrent conf; returns its summary (no key material).
    async fn import_p2p(
        &self,
        conf: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<HashMap<String, OwnedValue>, Error> {
        authorize(conn, &hdr, ACTION_IMPORT).await?;
        let conf = Zeroizing::new(conf);
        let parsed = conf::parse(&conf, "p2p.conf").map_err(|e| Error::InvalidConf(e.to_string()))?;
        store_conf(P2P_CONF, &conf).map_err(|e| Error::NetlinkFailed(format!("storing the conf: {e}")))?;
        tracing::info!(server = %parsed.summary.server_label, "torrent conf imported");
        Ok(summary_dict(&parsed.summary))
    }

    async fn p2p_up(&self, #[zbus(connection)] conn: &zbus::Connection, #[zbus(header)] hdr: Header<'_>) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        let conf = load_conf(P2P_CONF).ok_or_else(|| Error::NotImported("No torrent config imported".into()))?;
        {
            let mut i = self.inner.lock().await;
            if i.link.on && i.status.state != TunnelState::Off {
                return Ok(());
            }
            i.link.start();
            i.status.state = TunnelState::Connecting;
            i.status.error.clear();
            i.next_round = None;
        }
        tracing::info!(server = %conf.summary.server_label, "torrent tunnel up");
        match netns::up(&P2P, &conf).await {
            Ok(()) => {
                let mut i = self.inner.lock().await;
                i.endpoint = netns::resolve(&conf).ok();
                i.link.conf = Some(conf);
                Ok(())
            }
            Err(e) => {
                tracing::error!("bringing the tunnel up: {e}");
                let mut i = self.inner.lock().await;
                i.status.state = TunnelState::Error;
                i.status.error = e.clone();
                i.link.conf = Some(conf);
                Err(Error::NetlinkFailed(e))
            }
        }
    }

    async fn p2p_down(&self, quit_qbit: bool, force: bool, #[zbus(connection)] conn: &zbus::Connection, #[zbus(header)] hdr: Header<'_>) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        let running = !tokio::task::spawn_blocking(qbit::pids).await.unwrap_or_default().is_empty() || qbit::unit_busy(conn).await;
        if running {
            if quit_qbit {
                qbit::stop(conn).await;
                qbit::wait_stopped(conn, Duration::from_secs(10)).await;
            } else if !force {
                return Err(Error::QbitRunning("qBittorrent is still running".into()));
            }
        }
        self.reset().await;
        netns::down(&P2P).await.map_err(Error::NetlinkFailed)?;
        tracing::info!("torrent tunnel down");
        Ok(())
    }

    async fn launch_qbit(
        &self,
        argv: Vec<String>,
        env: HashMap<String, String>,
        port: u16,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        if !netns::exists(&P2P) {
            return Err(Error::NotImported("The torrent tunnel is off".into()));
        }
        let user = launcher(conn, &hdr, &argv).await?;
        if !tokio::task::spawn_blocking(qbit::pids).await.unwrap_or_default().is_empty() || qbit::unit_busy(conn).await {
            return Ok(());
        }
        qbit::launch(conn, argv, env, user.uid, user.gid, user.home).await.map_err(|e| Error::NetlinkFailed(format!("starting qBittorrent: {e}")))?;
        let mut i = self.inner.lock().await;
        i.status.qbit = QbitState::Starting;
        i.status.qbit_exit = 0;
        i.qbit_since = None;
        i.launched_at = Some(Instant::now());
        // The applet wrote the port into qBittorrent.conf before calling.
        i.status.listen_port_applied = if port != 0 { ListenApplied::Conf } else { ListenApplied::None };
        Ok(())
    }

    async fn quit_qbit(&self, #[zbus(connection)] conn: &zbus::Connection, #[zbus(header)] hdr: Header<'_>) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        qbit::stop(conn).await;
        qbit::wait_stopped(conn, Duration::from_secs(10)).await;
        Ok(())
    }

    async fn set_qbit_port(
        &self,
        port: u16,
        webui_port: u16,
        user: String,
        pass: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<String, Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        let (user, pass) = (Zeroizing::new(user), Zeroizing::new(pass));
        if port == 0 || webui_port == 0 {
            return Err(Error::WebUi("invalid port".into()));
        }
        if !netns::exists(&P2P) {
            return Ok(qbit::WebUi::NotRunning.as_str().to_owned());
        }
        let r = tokio::task::spawn_blocking(move || qbit::webui(webui_port, user, pass, Some(port))).await.unwrap_or(qbit::WebUi::Unreachable);
        let mut i = self.inner.lock().await;
        i.status.listen_port_applied = match r {
            qbit::WebUi::Ok => ListenApplied::Auto,
            qbit::WebUi::Unreachable | qbit::WebUi::NotRunning => ListenApplied::Manual,
            qbit::WebUi::AuthFailed => ListenApplied::Failed,
        };
        Ok(r.as_str().to_owned())
    }

    async fn test_qbit_web_ui(
        &self,
        webui_port: u16,
        user: String,
        pass: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<String, Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        let (user, pass) = (Zeroizing::new(user), Zeroizing::new(pass));
        if !netns::exists(&P2P) || self.inner.lock().await.status.qbit != QbitState::Vpn {
            return Ok(qbit::WebUi::NotRunning.as_str().to_owned());
        }
        let r = tokio::task::spawn_blocking(move || qbit::webui(webui_port, user, pass, None)).await.unwrap_or(qbit::WebUi::Unreachable);
        Ok(r.as_str().to_owned())
    }

    /// Re-parses and stores the browser conf; returns its summary (no key material).
    async fn import_browser(
        &self,
        conf: String,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<HashMap<String, OwnedValue>, Error> {
        authorize(conn, &hdr, ACTION_IMPORT).await?;
        let conf = Zeroizing::new(conf);
        let parsed = conf::parse(&conf, "browser.conf").map_err(|e| Error::InvalidConf(e.to_string()))?;
        store_conf(BROWSER_CONF, &conf).map_err(|e| Error::NetlinkFailed(format!("storing the conf: {e}")))?;
        tracing::info!(server = %parsed.summary.server_label, "browser conf imported");
        Ok(summary_dict(&parsed.summary))
    }

    async fn browser_up(&self, #[zbus(connection)] conn: &zbus::Connection, #[zbus(header)] hdr: Header<'_>) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        let conf = load_conf(BROWSER_CONF).ok_or_else(|| Error::NotImported("No browser config imported".into()))?;
        {
            let mut i = self.inner.lock().await;
            let b = &mut i.browser;
            if b.link.on && b.status.state != TunnelState::Off {
                return Ok(());
            }
            *b = Browser { last_sent: b.last_sent.take(), ..Browser::default() };
            b.link.start();
            b.link.conf = Some(conf.clone());
            b.status.state = TunnelState::Connecting;
        }
        tracing::info!(server = %conf.summary.server_label, "browser tunnel up");
        if let Err(e) = netns::up(&BROWSER, &conf).await {
            tracing::error!("bringing the browser tunnel up: {e}");
            self.browser_close(conn, &e).await;
            return Err(Error::NetlinkFailed(e));
        }
        Ok(())
    }

    /// Quits the browser and takes its tunnel down.
    async fn browser_down(&self, #[zbus(connection)] conn: &zbus::Connection, #[zbus(header)] hdr: Header<'_>) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        self.browser_close(conn, "").await;
        Ok(())
    }

    async fn launch_browser(
        &self,
        argv: Vec<String>,
        env: HashMap<String, String>,
        #[zbus(connection)] conn: &zbus::Connection,
        #[zbus(header)] hdr: Header<'_>,
    ) -> Result<(), Error> {
        authorize(conn, &hdr, ACTION_CONTROL).await?;
        if !netns::exists(&BROWSER) {
            return Err(Error::NotImported("The browser tunnel is off".into()));
        }
        let user = launcher(conn, &hdr, &argv).await?;
        if !tokio::task::spawn_blocking(browser::pids).await.unwrap_or_default().is_empty() {
            return Ok(());
        }
        browser::launch(conn, argv, env, user).await.map_err(|e| Error::NetlinkFailed(format!("starting the browser: {e}")))?;
        let mut i = self.inner.lock().await;
        i.browser.status.browser = BrowserState::Starting;
        i.browser.launched_at = Some(Instant::now());
        i.browser.gone_since = None;
        Ok(())
    }

    async fn get_browser_status(&self) -> HashMap<String, OwnedValue> {
        self.inner.lock().await.browser.status.to_dict()
    }

    async fn get_status(&self) -> HashMap<String, OwnedValue> {
        self.inner.lock().await.status.to_dict()
    }

    async fn get_web_handshake(&self) -> u32 {
        tokio::task::spawn_blocking(|| netns::web_handshake_age(WEB_IF)).await.unwrap_or(0)
    }

    #[zbus(signal)]
    async fn status_changed(emitter: &SignalEmitter<'_>, status: HashMap<String, OwnedValue>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn browser_changed(emitter: &SignalEmitter<'_>, status: HashMap<String, OwnedValue>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn port_changed(emitter: &SignalEmitter<'_>, port: u16) -> zbus::Result<()>;
}
