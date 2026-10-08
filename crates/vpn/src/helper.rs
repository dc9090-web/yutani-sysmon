//! The applet's side of the root helper's D-Bus API (SPEC §12).

use std::collections::HashMap;

use futures_util::StreamExt;
use vpn_common::conf::{ConfSummary, Hint};
use vpn_common::dbus::{BUS_NAME, INTERFACE, OBJECT_PATH};
use vpn_common::status::Status;
use zbus::zvariant::OwnedValue;

/// Why a helper call failed, as far as the UI cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// polkit said no: "Permission denied".
    Denied,
    /// `P2pDown` refused because qBittorrent is running.
    QbitRunning,
    /// The helper isn't installed.
    Missing,
    /// Anything else, with the helper's message.
    Other(String),
}

impl From<zbus::Error> for CallError {
    fn from(e: zbus::Error) -> Self {
        match &e {
            zbus::Error::MethodError(name, msg, _) => {
                let name = name.as_str();
                let msg = msg.clone().unwrap_or_default();
                if name.ends_with(".PermissionDenied") {
                    Self::Denied
                } else if name.ends_with(".QbitRunning") {
                    Self::QbitRunning
                } else if name == "org.freedesktop.DBus.Error.ServiceUnknown" || name == "org.freedesktop.DBus.Error.NameHasNoOwner" {
                    Self::Missing
                } else {
                    Self::Other(if msg.is_empty() { name.to_owned() } else { msg })
                }
            }
            _ => Self::Other(e.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, CallError>;

async fn proxy(conn: &zbus::Connection) -> Result<zbus::Proxy<'static>> {
    Ok(zbus::Proxy::new(conn, BUS_NAME, OBJECT_PATH, INTERFACE).await?)
}

/// The helper is installed (activatable) or running.
pub async fn available(conn: &zbus::Connection) -> bool {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(conn).await else { return false };
    let name = zbus::names::BusName::try_from(BUS_NAME).expect("valid name");
    if dbus.name_has_owner(name).await.unwrap_or(false) {
        return true;
    }
    dbus.list_activatable_names().await.is_ok_and(|names| names.iter().any(|n| n.as_str() == BUS_NAME))
}

/// Sends the conf text once, for the helper to re-parse and store root-only.
pub async fn import_p2p(conn: zbus::Connection, conf: zeroize::Zeroizing<String>, file_name: String) -> Result<ConfSummary> {
    let p = proxy(&conn).await?;
    let d: HashMap<String, OwnedValue> = p.call("ImportP2p", &(conf.as_str(),)).await?;
    drop(conf);
    let text = |k: &str| d.get(k).and_then(|v| <&str>::try_from(v).ok()).unwrap_or_default().to_owned();
    Ok(ConfSummary {
        file_name,
        server_label: text("server_label"),
        endpoint: text("endpoint"),
        addr_v4: text("addr_v4"),
        has_v6: d.get("has_v6").and_then(|v| bool::try_from(v).ok()).unwrap_or_default(),
        dns: d.get("dns").and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok()).unwrap_or_default(),
        natpmp_hint: Hint::parse(&text("natpmp_hint")),
        moderate_nat_hint: Hint::parse(&text("moderate_nat_hint")),
        natpmp_capable: None,
    })
}

pub async fn p2p_up(conn: zbus::Connection) -> Result<()> {
    Ok(proxy(&conn).await?.call("P2pUp", &()).await?)
}

pub async fn p2p_down(conn: zbus::Connection, quit_qbit: bool, force: bool) -> Result<()> {
    Ok(proxy(&conn).await?.call("P2pDown", &(quit_qbit, force)).await?)
}

pub async fn launch_qbit(conn: zbus::Connection, argv: Vec<String>, env: HashMap<String, String>, port: u16) -> Result<()> {
    Ok(proxy(&conn).await?.call("LaunchQbit", &(argv, env, port)).await?)
}

/// `ok` / `unreachable` / `auth_failed` / `not_running`.
pub async fn set_qbit_port(conn: zbus::Connection, port: u16, webui_port: u16, user: String, pass: zeroize::Zeroizing<String>) -> Result<String> {
    Ok(proxy(&conn).await?.call("SetQbitPort", &(port, webui_port, user, pass.as_str())).await?)
}

pub async fn test_webui(conn: zbus::Connection, webui_port: u16, user: String, pass: zeroize::Zeroizing<String>) -> Result<String> {
    Ok(proxy(&conn).await?.call("TestQbitWebUi", &(webui_port, user, pass.as_str())).await?)
}

pub async fn status(conn: zbus::Connection) -> Result<Status> {
    let d: HashMap<String, OwnedValue> = proxy(&conn).await?.call("GetStatus", &()).await?;
    Ok(Status::from_dict(&d))
}

pub async fn web_handshake(conn: zbus::Connection) -> Result<u32> {
    Ok(proxy(&conn).await?.call("GetWebHandshake", &()).await?)
}

/// What the helper signals.
#[derive(Debug, Clone)]
pub enum Event {
    Status(Status),
    PortChanged(u16),
}

/// The helper's signals, forever (re-subscribing if the bus drops). Doesn't
/// activate the helper: signals only arrive once something else has.
pub fn events() -> impl futures_util::Stream<Item = Event> {
    cosmic::iced::stream::channel(16, |mut out: cosmic::iced::futures::channel::mpsc::Sender<Event>| async move {
        use cosmic::iced::futures::SinkExt;
        loop {
            let run = async {
                let conn = zbus::Connection::system().await?;
                let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(BUS_NAME)?.interface(INTERFACE)?.path(OBJECT_PATH)?.build();
                let mut stream = zbus::MessageStream::for_match_rule(rule, &conn, Some(32)).await?;
                while let Some(Ok(msg)) = stream.next().await {
                    let hdr = msg.header();
                    let event = match hdr.member().map(|m| m.as_str()) {
                        Some("StatusChanged") => msg.body().deserialize::<HashMap<String, OwnedValue>>().ok().map(|d| Event::Status(Status::from_dict(&d))),
                        Some("PortChanged") => msg.body().deserialize::<u16>().ok().map(Event::PortChanged),
                        _ => None,
                    };
                    if let Some(e) = event
                        && out.send(e).await.is_err()
                    {
                        return Ok::<_, zbus::Error>(());
                    }
                }
                Ok(())
            };
            if let Err(e) = run.await {
                tracing::debug!("helper signals: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    })
}
