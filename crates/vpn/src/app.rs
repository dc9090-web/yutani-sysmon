//! The applet: state, update, the panel button and the popup.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};
use zeroize::Zeroizing;

use common::format::format_rate;
use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ui::{self, cell, mono, space_m, space_s, space_xs, space_xxs, space_xxxs};

use vpn_common::conf::{self, ConfSummary, Hint};
use vpn_common::status::{ListenApplied, PortState, QbitState, Status, TunnelState};

use crate::config::{APP_ID, Config, IconStyle};
use crate::fl;
use crate::helper::{self, CallError};
use crate::model::{self, Reticle, Tone, Web};
use crate::nm::{self, WebState};
use crate::qbit;
use crate::widgets::panel::{self, Chunk, tone_ink};
use crate::widgets::reticle::reticle;
use crate::{keyring, preview};

/// A switch stays disabled this long at most while waiting for a terminal state.
const BUSY_LIMIT: Duration = Duration::from_secs(20);
/// Launch qBittorrent without a port if none arrives this soon.
const PORT_WAIT: Duration = Duration::from_secs(5);
const TOAST: Duration = Duration::from_millis(1400);
/// The Web UI test runs this long after the last field edit.
const TEST_DEBOUNCE: Duration = Duration::from_millis(600);
/// Key column of the torrent box.
const KEY_W: f32 = 92.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tunnel {
    P2p,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Launch,
    Quit,
    AutoPort,
    ShowPort,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WebUiPort,
    User,
    Pass,
    Command,
}

/// The Web UI test's last result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebUiTest {
    Ok,
    Unreachable,
    AuthFailed,
    NotRunning,
}

impl WebUiTest {
    fn parse(s: &str) -> Self {
        match s {
            "ok" => Self::Ok,
            "auth_failed" => Self::AuthFailed,
            "not_running" => Self::NotRunning,
            _ => Self::Unreachable,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Surface(cosmic::surface::Action<Message>),
    TogglePopup(Rectangle, cosmic::iced::Vector),
    PopupClosed(Id),
    Config(Config),
    Page(Page),
    Escape,
    Tick,
    /// The system bus and whether the helper is installed.
    Bus(Option<zbus::Connection>, bool),
    Helper(helper::Event),
    /// A later check for the helper (it may have been installed since start).
    HelperFound(bool),
    HelperStatus(Status),
    Web(WebState),
    WebHandshake(Option<u32>),
    Switch(Tunnel, bool),
    /// A helper or NM call finished: the tunnel and its error, if any.
    Called(Tunnel, Result<(), CallError>),
    WebCalled(Result<(), String>),
    ConfirmQuit(bool),
    Launch,
    Launched(Result<(), CallError>),
    PortPushed(u16, Result<String, CallError>),
    CopyPort(u16),
    RestartInVpn,
    Terminated,
    ToastEnd(u64),
    Import(Tunnel),
    Picked(Tunnel, Option<(String, Zeroizing<String>)>),
    Imported(Tunnel, Result<ConfSummary, String>),
    WebImported(ConfSummary, Result<String, String>),
    Set(Toggle, bool),
    Edit(Field, String),
    TestDue(u64),
    Tested(Result<String, CallError>),
    PassLoaded(Zeroizing<String>),
    PassStored(bool),
    IconStyle(Entity),
    #[cfg(feature = "demo")]
    DemoNext,
    #[cfg(feature = "demo")]
    TakeShot,
    #[cfg(feature = "demo")]
    Shot(cosmic::iced::window::Screenshot),
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    page: Page,
    config: Config,
    handler: Option<cosmic_config::Config>,
    bus: Option<zbus::Connection>,
    /// The helper is installed; `None` until checked.
    helper_ok: Option<bool>,
    /// The torrent tunnel, as the helper last reported it.
    p2p: Status,
    web: Web,
    /// Waiting for a terminal state after a switch, since this instant.
    busy: HashMap<Tunnel, Instant>,
    /// The torrent switch is wanted on, for launching and errors.
    p2p_wanted: bool,
    /// qBittorrent processes outside the namespace.
    outside: Vec<i32>,
    restarting: bool,
    confirm_quit: bool,
    /// The tunnel came up with no port yet: launch at this time anyway.
    launch_at: Option<Instant>,
    launched: bool,
    /// The port written to qBittorrent.conf at launch, or pushed since.
    applied_port: Option<u16>,
    qbit_cmd: Option<qbit::Command>,
    web_counters: Option<(Instant, u64, u64)>,
    toast: Option<(String, u64)>,
    toast_id: u64,
    importing: Option<Tunnel>,
    import_error: HashMap<Tunnel, String>,
    pass: Zeroizing<String>,
    pass_input: String,
    pass_saved: bool,
    port_input: String,
    test: Option<WebUiTest>,
    test_gen: u64,
    ticks: u64,
    restored: bool,
    icon_model: SingleSelectModel,
    #[cfg(feature = "demo")]
    demo: Option<usize>,
}

impl App {
    fn demo(&self) -> bool {
        #[cfg(feature = "demo")]
        return self.demo.is_some();
        #[cfg(not(feature = "demo"))]
        false
    }

    /// The torrent status as shown: the helper's, with the leak and an
    /// optimistic `connecting` while a switch is pending.
    fn p2p_view(&self) -> Status {
        let mut s = self.p2p.clone();
        if !self.outside.is_empty() {
            s.qbit = QbitState::Outside;
        }
        if self.busy.contains_key(&Tunnel::P2p) && s.state == TunnelState::Off && self.p2p_wanted {
            s.state = TunnelState::Connecting;
        }
        s
    }

    fn web_view(&self) -> Web {
        let mut w = self.web.clone();
        if self.busy.contains_key(&Tunnel::Web) && w.state == TunnelState::Off && self.config.last_web {
            w.state = TunnelState::Connecting;
        }
        w
    }

    fn save(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<bool, cosmic_config::Error>) {
        match &self.handler {
            Some(h) => {
                if let Err(e) = set(&mut self.config, h) {
                    tracing::error!("saving config: {e}");
                }
            }
            None => tracing::error!("no config handler; setting not saved"),
        }
    }

    fn toast(&mut self, text: String) -> Task<Message> {
        self.toast_id += 1;
        let id = self.toast_id;
        self.toast = Some((text, id));
        cosmic::task::future(async move {
            tokio::time::sleep(TOAST).await;
            Message::ToastEnd(id)
        })
    }

    /// Checks again whether the helper is installed.
    fn recheck_helper(&self) -> Task<Message> {
        let Some(bus) = self.bus() else { return Task::none() };
        if self.helper_ok == Some(true) {
            return Task::none();
        }
        cosmic::task::future(async move { Message::HelperFound(helper::available(&bus).await) })
    }

    fn bus(&self) -> Option<zbus::Connection> {
        if self.demo() { None } else { self.bus.clone() }
    }

    // ---- torrent tunnel ---------------------------------------------------------------

    fn p2p_on(&mut self) -> Task<Message> {
        let Some(bus) = self.bus() else { return Task::none() };
        self.p2p_wanted = true;
        self.launched = false;
        self.applied_port = None;
        self.busy.insert(Tunnel::P2p, Instant::now());
        self.save(|c, h| c.set_last_p2p(h, true));
        cosmic::task::future(async move { Message::Called(Tunnel::P2p, helper::p2p_up(bus).await) })
    }

    fn p2p_off(&mut self, quit: bool) -> Task<Message> {
        let Some(bus) = self.bus() else { return Task::none() };
        self.p2p_wanted = false;
        self.launch_at = None;
        self.busy.insert(Tunnel::P2p, Instant::now());
        self.save(|c, h| c.set_last_p2p(h, false));
        cosmic::task::future(async move { Message::Called(Tunnel::P2p, helper::p2p_down(bus, quit, false).await) })
    }

    /// Starts qBittorrent in the namespace, with the port in its conf when known.
    fn launch(&mut self) -> Task<Message> {
        let (Some(bus), Some(cmd)) = (self.bus(), self.qbit_cmd.clone()) else { return Task::none() };
        if self.p2p.qbit != QbitState::Stopped || !qbit::outside().is_empty() {
            return Task::none();
        }
        self.launched = true;
        self.launch_at = None;
        qbit::clear_stale_lock(&cmd.conf);
        let port = if self.p2p.port_state == PortState::Ok { self.p2p.port } else { 0 };
        let mut sent = 0;
        if port != 0 {
            match qbit::write_port(&cmd.conf, port) {
                Ok(true) => {
                    sent = port;
                    self.applied_port = Some(port);
                }
                Ok(false) => tracing::info!("qBittorrent.conf layout not recognised; the port goes through the Web UI"),
                Err(e) => tracing::info!("qBittorrent.conf: {e}"),
            }
        }
        tracing::debug!(argv = ?cmd.argv, port = sent, "launching qBittorrent");
        let env = qbit::environment();
        cosmic::task::future(async move { Message::Launched(helper::launch_qbit(bus, cmd.argv, env, sent).await) })
    }

    /// Pushes the forwarded port through the Web UI if it isn't applied yet.
    fn push_port(&mut self) -> Task<Message> {
        let s = &self.p2p;
        if !self.config.auto_port || s.qbit != QbitState::Vpn || s.port_state != PortState::Ok || s.port == 0 || self.applied_port == Some(s.port) {
            return Task::none();
        }
        let Some(bus) = self.bus() else { return Task::none() };
        let port = s.port;
        // Don't retry this port on every status update.
        self.applied_port = Some(port);
        let (wp, user, pass) = (self.config.webui_port, self.config.webui_user.clone(), self.pass.clone());
        cosmic::task::future(async move { Message::PortPushed(port, helper::set_qbit_port(bus, port, wp, user, pass).await) })
    }

    /// A new status from the helper.
    fn on_status(&mut self, s: Status) -> Task<Message> {
        let prev = std::mem::replace(&mut self.p2p, s);
        let mut tasks = Vec::new();
        let terminal =
            matches!(self.p2p.state, TunnelState::On | TunnelState::Error | TunnelState::Stale) || (self.p2p.state == TunnelState::Off && !self.p2p_wanted);
        if terminal {
            self.busy.remove(&Tunnel::P2p);
        }
        if self.p2p.qbit_exit != 0 && prev.qbit_exit != self.p2p.qbit_exit {
            tasks.push(self.toast(fl!("toast-qbit-failed", code = self.p2p.qbit_exit)));
        }
        if self.p2p.qbit == QbitState::Stopped && prev.qbit != QbitState::Stopped {
            self.applied_port = None;
        }
        // The port is first known: remember the server can forward ports.
        if self.p2p.port_state == PortState::Ok
            && prev.port_state != PortState::Ok
            && let Some(mut c) = self.config.p2p_conf.clone()
            && c.natpmp_capable != Some(true)
        {
            c.natpmp_capable = Some(true);
            self.save(|cfg, h| cfg.set_p2p_conf(h, Some(c)));
        }
        // Launch with the tunnel (SPEC §7.2).
        if self.p2p_wanted && self.config.launch_qbit && !self.launched && self.p2p.state == TunnelState::On && self.p2p.qbit == QbitState::Stopped {
            if self.p2p.port_state == PortState::Ok || self.p2p.port_state == PortState::Fail {
                tasks.push(self.launch());
            } else if self.launch_at.is_none() {
                self.launch_at = Some(Instant::now() + PORT_WAIT);
            }
        }
        tasks.push(self.push_port());
        Task::batch(tasks)
    }

    // ---- web tunnel -------------------------------------------------------------------

    fn web_switch(&mut self, on: bool) -> Task<Message> {
        let (Some(bus), Some(uuid)) = (self.bus(), self.config.web_nm_uuid.clone()) else { return Task::none() };
        self.busy.insert(Tunnel::Web, Instant::now());
        self.save(|c, h| c.set_last_web(h, on));
        self.web.error.clear();
        if on {
            cosmic::task::future(async move { Message::WebCalled(nm::activate(bus, uuid).await) })
        } else {
            cosmic::task::future(async move { Message::WebCalled(nm::deactivate(bus, uuid).await) })
        }
    }

    // ---- restore at login -------------------------------------------------------------

    fn restore(&mut self) -> Task<Message> {
        if self.restored || self.demo() || !self.config.restore || self.helper_ok.is_none() {
            return Task::none();
        }
        self.restored = true;
        let mut tasks = Vec::new();
        if self.config.last_p2p && self.helper_ok == Some(true) && self.config.p2p_conf.is_some() && self.p2p.state == TunnelState::Off {
            tracing::info!("restoring the torrent tunnel");
            tasks.push(self.p2p_on());
        }
        if self.config.last_web && self.config.web_nm_uuid.is_some() && self.web.state == TunnelState::Off {
            if self.web.other_vpn {
                tracing::info!("another VPN is active; not restoring the web tunnel");
            } else {
                tracing::info!("restoring the web tunnel");
                tasks.push(self.web_switch(true));
            }
        }
        Task::batch(tasks)
    }

    // ---- settings ---------------------------------------------------------------------

    fn test_soon(&mut self) -> Task<Message> {
        self.test_gen += 1;
        let g = self.test_gen;
        cosmic::task::future(async move {
            tokio::time::sleep(TEST_DEBOUNCE).await;
            Message::TestDue(g)
        })
    }

    fn run_test(&mut self) -> Task<Message> {
        if self.p2p.qbit != QbitState::Vpn {
            self.test = Some(WebUiTest::NotRunning);
            return Task::none();
        }
        let Some(bus) = self.bus() else { return Task::none() };
        let (wp, user, pass) = (self.config.webui_port, self.config.webui_user.clone(), self.pass.clone());
        cosmic::task::future(async move { Message::Tested(helper::test_webui(bus, wp, user, pass).await) })
    }

    fn pick(&mut self, t: Tunnel) -> Task<Message> {
        self.import_error.remove(&t);
        cosmic::task::future(async move {
            let picked = async {
                use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};
                let req = SelectedFiles::open_file()
                    .title("WireGuard config")
                    .modal(true)
                    .filter(FileFilter::new("WireGuard config (*.conf)").glob("*.conf"))
                    .send()
                    .await
                    .ok()?;
                let files = req.response().ok()?;
                let uri = files.uris().first()?.clone();
                let path = uri.to_file_path().ok()?;
                let text = Zeroizing::new(tokio::fs::read_to_string(&path).await.ok()?);
                Some((path.file_name()?.to_string_lossy().into_owned(), text))
            };
            Message::Picked(t, picked.await)
        })
    }

    fn import(&mut self, t: Tunnel, name: String, text: Zeroizing<String>) -> Task<Message> {
        let parsed = match conf::parse(&text, &name) {
            Ok(c) => c,
            Err(e) => {
                self.import_error.insert(t, fl_err(e.ftl_key()));
                return Task::none();
            }
        };
        let summary = parsed.summary.clone();
        drop(parsed);
        let Some(bus) = self.bus() else { return Task::none() };
        self.importing = Some(t);
        match t {
            Tunnel::P2p => cosmic::task::future(async move {
                Message::Imported(
                    Tunnel::P2p,
                    helper::import_p2p(bus, text, name).await.map_err(|e| match e {
                        CallError::Denied => fl!("toast-denied"),
                        CallError::Missing => fl!("banner-helper-title"),
                        CallError::Other(m) => m,
                        CallError::QbitRunning => String::new(),
                    }),
                )
            }),
            Tunnel::Web => {
                let old = self.config.web_nm_uuid.clone();
                let label = summary.server_label.clone();
                cosmic::task::future(async move {
                    // Replace: the old connection goes first, never duplicated.
                    if let Some(old) = old
                        && nm::exists(&bus, &old).await
                        && let Err(e) = nm::delete(bus.clone(), old).await
                    {
                        return Message::WebImported(summary, Err(e));
                    }
                    Message::WebImported(summary, nm::import(text, label).await)
                })
            }
        }
    }

    // ---- panel ------------------------------------------------------------------------

    fn chunks(&self) -> Vec<Chunk> {
        let p = self.p2p_view();
        let w = self.web_view();
        let mk = |label: String, state: TunnelState| {
            let (key, tone) = model::chunk(state);
            Chunk { label, value: fl_word(key), tone, chars: 3.0 }
        };
        let mut v = vec![mk(fl!("panel-p2p"), p.state), mk(fl!("panel-web"), w.state)];
        if let Some(port) = model::port_chunk(&p, self.config.show_port) {
            v.push(Chunk { label: fl!("panel-port"), value: port.to_string(), tone: Tone::Busy, chars: 5.0 });
        }
        v
    }

    fn tooltip(&self) -> String {
        let p = self.p2p_view();
        let w = self.web_view();
        let word = |s: TunnelState| match s {
            TunnelState::On | TunnelState::Stale => "on",
            TunnelState::Connecting => "connecting",
            TunnelState::Error => "failed",
            TunnelState::Off => "off",
        };
        let mut p2p = word(p.state).to_owned();
        if let Some(port) = model::port_chunk(&p, true) {
            p2p.push_str(&fl!("panel-tooltip-port", port = port));
        }
        let mut text = fl!("panel-tooltip", p2p = p2p, web = word(w.state));
        if p.qbit == QbitState::Outside {
            text.push(' ');
            text.push_str(&fl!("panel-tooltip-leak"));
        }
        text
    }

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;
        let size = if p.horizontal { ih } else { iw };
        let thickness = size + 2.0 * f32::from(minor);
        let pv = self.p2p_view();
        let wv = self.web_view();
        let icon = reticle(Reticle::of(pv.state, wv.state), model::issue(&pv, &wv), self.config.icon_style, size);
        let content = panel::content(icon, self.chunks(), &p);
        let (pad, w, h) =
            if p.horizontal { ([0, major], Length::Shrink, Length::Fixed(thickness)) } else { ([major, 0], Length::Fixed(thickness), Length::Shrink) };
        let button = widget::button::custom(widget::container(content).center_x(w).center_y(h))
            .padding(pad)
            .class(theme::Button::AppletIcon)
            .on_press_with_rectangle(|offset, bounds| Message::TogglePopup(bounds, offset));
        let tip = self.core.applet.applet_tooltip::<Message>(button, self.tooltip(), self.popup.is_some(), Message::Surface, None);
        self.core.applet.autosize_window(tip).into()
    }

    // ---- popup ------------------------------------------------------------------------

    fn popup_view(&self) -> Element<'_, Message> {
        let content = match self.page {
            Page::Main => self.main_page(),
            Page::Settings => self.settings_page(),
        };
        let mut col = Column::new().push(content);
        if let Some((t, _)) = &self.toast {
            col = col.push(widget::container(toast(t.clone())).center_x(Length::Fill).padding([space_xxs(), 0]));
        }
        widget::container(col).padding([8, 0, 8, 0]).into()
    }

    fn header(&self) -> Element<'_, Message> {
        let pv = self.p2p_view();
        let wv = self.web_view();
        let (key, tone) = model::header(&pv, &wv);
        let ink = tone.map_or(Ink::Muted, tone_ink);
        let icon = reticle(Reticle::of(pv.state, wv.state), None, IconStyle::Mono, 24.0);
        widget::container(
            Row::new()
                .spacing(space_s())
                .align_y(Alignment::Center)
                .push(icon)
                .push(Column::new().push(widget::text::heading(fl!("title"))).push(widget::text::caption(fl_word(key)).class(ink.text()))),
        )
        .padding([space_xs(), space_m()])
        .into()
    }

    fn main_page(&self) -> Element<'_, Message> {
        let mut col = Column::new().push(self.header());
        let pv = self.p2p_view();
        if pv.qbit == QbitState::Outside {
            col = col.push(self.leak_banner());
        }
        let no_configs = self.config.p2p_conf.is_none() && self.config.web_conf.is_none();
        if no_configs {
            return col
                .push(banner(fl!("banner-import-title"), fl!("banner-import-body"), Some((fl!("banner-import-btn"), Message::Page(Page::Settings)))))
                .push(ui::inset_divider())
                .push(ui::settings_row(fl!("settings"), Message::Page(Page::Settings)))
                .into();
        }
        if self.helper_ok == Some(false) {
            col = col.push(banner(fl!("banner-helper-title"), fl!("banner-helper-body"), None));
        }
        col = col.push(widget::divider::horizontal::default());
        if self.helper_ok != Some(false) {
            col = col.push(self.p2p_card()).push(ui::inset_divider());
        }
        col.push(self.web_card()).push(ui::inset_divider()).push(ui::settings_row(fl!("settings"), Message::Page(Page::Settings))).into()
    }

    fn leak_banner(&self) -> Element<'_, Message> {
        let button = widget::button::standard(if self.restarting { fl!("leak-closing") } else { fl!("leak-restart") })
            .on_press_maybe((!self.restarting && !self.demo()).then_some(Message::RestartInVpn));
        let text = Column::new()
            .spacing(space_xxxs())
            .width(Length::Fill)
            .push(widget::text::body(fl!("leak-title")).font(cosmic::font::semibold()).class(Ink::Destructive.text()))
            .push(widget::text::caption(fl!("leak-body")).class(Ink::Muted.text()))
            .push(widget::container(button).padding([space_xxxs(), 0, 0, 0]));
        let row = Row::new()
            .spacing(space_xs())
            .push(widget::icon::from_name("dialog-warning-symbolic").size(16).symbolic(true).icon().class(Ink::Destructive.svg()))
            .push(text);
        widget::container(boxed(row.into())).padding([0, space_m(), space_xs(), space_m()]).into()
    }

    /// Icon, title, caption and switch.
    fn card_head(&self, t: Tunnel, state: TunnelState, summary: Option<&ConfSummary>) -> Element<'_, Message> {
        let (icon, title, caption) = match t {
            Tunnel::P2p => ("folder-download-symbolic", fl!("card-p2p-title"), summary.map(|s| fl!("card-p2p-caption", server = s.server_label.clone()))),
            Tunnel::Web => ("web-browser-symbolic", fl!("card-web-title"), summary.map(|s| fl!("card-web-caption", server = s.server_label.clone()))),
        };
        let configured = match t {
            Tunnel::P2p => self.config.p2p_conf.is_some(),
            Tunnel::Web => self.config.web_nm_uuid.is_some(),
        };
        let busy = self.busy.contains_key(&t) || state == TunnelState::Connecting;
        let on = state != TunnelState::Off;
        let toggler = widget::toggler(on).on_toggle_maybe((configured && !busy && !self.demo()).then_some(move |v| Message::Switch(t, v)));
        let caption = caption.unwrap_or_else(|| fl!("card-no-config"));
        Row::new()
            .spacing(space_s())
            .align_y(Alignment::Center)
            .push(widget::icon::from_name(icon).size(20).symbolic(true))
            .push(Column::new().width(Length::Fill).push(widget::text::heading(title)).push(widget::text::caption(caption).class(Ink::Muted.text())))
            .push(toggler)
            .into()
    }

    /// "Connected · 12s ago" and the rates.
    fn status_row(&self, state: TunnelState, error: &str, age: Option<u32>, server: &str, rates: Option<(u64, u64)>) -> Element<'_, Message> {
        let (text, tone) = match state {
            TunnelState::Off => (fl!("st-off"), Tone::Off),
            TunnelState::Connecting => (fl!("st-connecting"), Tone::Busy),
            TunnelState::On => (age.map_or_else(|| fl!("st-on", ago = "").trim_end_matches([' ', '·']).to_owned(), |a| fl!("st-on", ago = ago(a))), Tone::On),
            TunnelState::Stale => (fl!("st-stale", dur = dur(age.unwrap_or(0))), Tone::Warn),
            TunnelState::Error if error.is_empty() => (fl!("st-handshake-timeout", server = server.to_owned()), Tone::Err),
            TunnelState::Error => (error.to_owned(), Tone::Err),
        };
        let weight = if tone == Tone::Off { cosmic::font::default() } else { cosmic::font::semibold() };
        let mut row = Row::new()
            .spacing(space_xs())
            .align_y(Alignment::Center)
            .push(widget::text::caption(text).font(weight).class(tone_ink(tone).text()).width(Length::Fill));
        if let Some((rx, tx)) = rates.filter(|_| state == TunnelState::On) {
            row = row.push(rate(true, rx)).push(rate(false, tx));
        }
        row.into()
    }

    fn p2p_card(&self) -> Element<'_, Message> {
        let s = self.p2p_view();
        let summary = self.config.p2p_conf.as_ref();
        let server = summary.map_or_else(String::new, |c| c.server_label.clone());
        let mut col = Column::new().spacing(space_xs()).push(self.card_head(Tunnel::P2p, s.state, summary)).push(self.status_row(
            s.state,
            &s.error,
            Some(s.handshake_age).filter(|a| *a > 0),
            &server,
            Some((s.rx_bps, s.tx_bps)),
        ));
        if self.confirm_quit {
            col = col.push(boxed(
                Column::new()
                    .spacing(space_xxs())
                    .push(widget::text::body(fl!("confirm-quit-title")).font(cosmic::font::semibold()))
                    .push(widget::text::caption(fl!("confirm-quit-body")).class(Ink::Muted.text()))
                    .push(
                        Row::new()
                            .spacing(space_xs())
                            .push(widget::button::destructive(fl!("confirm-quit")).on_press(Message::ConfirmQuit(true)))
                            .push(widget::button::standard(fl!("cancel")).on_press(Message::ConfirmQuit(false))),
                    )
                    .into(),
            ));
        }
        let note = |icon: &'static str, ink: Ink, text: String| -> Element<'_, Message> {
            Row::new()
                .spacing(space_xs())
                .push(widget::icon::from_name(icon).size(12).symbolic(true).icon().class(ink.svg()))
                .push(widget::text::caption(text).class(if ink == Ink::Destructive { Ink::Destructive } else { Ink::Muted }.text()).width(Length::Fill))
                .into()
        };
        if s.state.up() {
            col = col.push(self.p2p_box(&s));
            col = col.push(if s.qbit == QbitState::Outside {
                note("dialog-warning-symbolic", Ink::Destructive, fl!("note-outside"))
            } else {
                note("connection-secure-symbolic", Ink::Success, fl!("note-killswitch"))
            });
        } else if s.state != TunnelState::Off {
            col = col.push(note("connection-secure-symbolic", Ink::Success, fl!("note-armed")));
        } else {
            col = col.push(note("connection-secure-symbolic", Ink::Muted, fl!("note-p2p-off")));
        }
        widget::container(col).padding([space_xs(), space_m()]).width(Length::Fill).into()
    }

    /// Forwarded port, qBittorrent and listening port.
    fn p2p_box(&self, s: &Status) -> Element<'_, Message> {
        let dim = s.state == TunnelState::Stale;
        let key = |k: String| cell(widget::text::caption(k).class(Ink::Muted.text()), KEY_W, Horizontal::Left);
        let row = |k: String, v: Element<'static, Message>| -> Element<'static, Message> {
            Row::new().spacing(space_xs()).align_y(Alignment::Center).height(Length::Fixed(28.0)).push(key(k)).push(v).into()
        };
        let port: Element<'static, Message> = match s.port_state {
            PortState::Ok => Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(mono(s.port.to_string(), Type::new(16.0, 22.0), Weight::Bold, Some(if dim { Ink::Muted } else { Ink::On })))
                .push(
                    widget::button::icon(widget::icon::from_name("edit-copy-symbolic").size(14)).on_press(Message::CopyPort(s.port)).tooltip(fl!("copy-port")),
                )
                .push(widget::tooltip(
                    Row::new()
                        .spacing(space_xxxs())
                        .align_y(Alignment::Center)
                        .push(widget::icon::from_name("view-refresh-symbolic").size(12).symbolic(true))
                        .push(widget::text::caption(ago(s.port_renewed_age)).class(Ink::Muted.text())),
                    widget::text::caption(fl!("port-renewed-tip")),
                    widget::tooltip::Position::Top,
                ))
                .into(),
            PortState::Fail => Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::text::body(fl!("port-unavailable")).class(Ink::Warning.text()))
                .push(widget::text::caption(fl!("port-unavailable-why")).class(Ink::Muted.text()))
                .into(),
            _ => widget::text::caption(fl!("port-requesting")).class(Ink::Muted.text()).into(),
        };
        let qb: Element<'static, Message> = match (s.qbit, &self.qbit_cmd) {
            (QbitState::Vpn, _) => {
                Row::new().spacing(space_xs()).align_y(Alignment::Center).push(ui::dot(Ink::Success)).push(widget::text::body(fl!("qbit-vpn"))).into()
            }
            (QbitState::Starting, _) => widget::text::caption(fl!("qbit-starting")).class(Ink::Muted.text()).into(),
            (QbitState::Outside, _) => {
                Row::new().spacing(space_xs()).align_y(Alignment::Center).push(ui::dot(Ink::Destructive)).push(widget::text::body(fl!("qbit-outside"))).into()
            }
            (QbitState::Stopped, None) => widget::text::caption(fl!("qbit-missing")).class(Ink::Muted.text()).into(),
            (QbitState::Stopped, Some(_)) => Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::text::caption(fl!("qbit-stopped")).class(Ink::Muted.text()).width(Length::Fill))
                .push(
                    widget::button::custom(
                        Row::new()
                            .spacing(space_xxxs())
                            .align_y(Alignment::Center)
                            .push(widget::icon::from_name("media-playback-start-symbolic").size(10).symbolic(true))
                            .push(widget::text::caption(fl!("qbit-launch"))),
                    )
                    .class(theme::Button::Standard)
                    .padding([2, 12])
                    .on_press_maybe((!self.demo()).then_some(Message::Launch)),
                )
                .into(),
        };
        let listen: Element<'static, Message> = match (s.port_state, s.qbit, s.listen_port_applied) {
            (PortState::Ok, QbitState::Vpn, ListenApplied::Auto | ListenApplied::Conf) => Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::icon::from_name("object-select-symbolic").size(14).symbolic(true).icon().class(Ink::Success.svg()))
                .push(widget::text::body(fl!("listen-set", port = s.port.to_string())))
                .into(),
            (PortState::Ok, QbitState::Vpn, ListenApplied::Manual) => Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::text::body(fl!("listen-manual")).class(Ink::Warning.text()))
                .push(widget::text::caption(fl!("listen-manual-why")).class(Ink::Muted.text()))
                .into(),
            (PortState::Ok, QbitState::Vpn, ListenApplied::Failed) => widget::text::body(fl!("webui-auth")).class(Ink::Warning.text()).into(),
            (PortState::Ok, QbitState::Stopped | QbitState::Starting, _) => widget::text::caption(fl!("listen-pending")).class(Ink::Muted.text()).into(),
            _ => widget::text::caption("—").class(Ink::Muted.text()).into(),
        };
        let col = Column::new().push(row(fl!("row-port"), port)).push(row(fl!("row-qbit"), qb)).push(row(fl!("row-listen"), listen));
        let b = boxed(col.into());
        if dim { widget::container(b).class(theme::Container::custom(|_| widget::container::Style::default())).into() } else { b }
    }

    fn web_card(&self) -> Element<'_, Message> {
        let w = self.web_view();
        let summary = self.config.web_conf.as_ref().filter(|_| self.config.web_nm_uuid.is_some());
        let server = summary.map_or_else(String::new, |c| c.server_label.clone());
        let mut col = Column::new().spacing(space_xs()).push(self.card_head(Tunnel::Web, w.state, summary)).push(self.status_row(
            w.state,
            &w.error,
            w.handshake_age.filter(|a| *a > 0),
            &server,
            Some((w.rx_bps, w.tx_bps)),
        ));
        if w.state == TunnelState::Off && w.other_vpn && self.config.last_web {
            col = col.push(widget::text::caption(fl!("web-other-vpn")).class(Ink::Warning.text()));
        }
        if w.state != TunnelState::Off && self.p2p_view().state != TunnelState::Off {
            col = col.push(
                Row::new()
                    .spacing(space_xs())
                    .push(widget::icon::from_name("connection-secure-symbolic").size(12).symbolic(true).icon().class(Ink::Muted.svg()))
                    .push(widget::text::caption(fl!("note-both")).class(Ink::Muted.text())),
            );
        }
        widget::container(col).padding([space_xs(), space_m()]).width(Length::Fill).into()
    }

    // ---- settings page ----------------------------------------------------------------

    fn config_row(&self, t: Tunnel) -> Element<'_, Message> {
        let (icon, title, summary, on) = match t {
            Tunnel::P2p => (
                "folder-download-symbolic",
                fl!("card-p2p-title").split(" · ").next().unwrap_or_default().to_owned(),
                self.config.p2p_conf.as_ref(),
                self.p2p.state != TunnelState::Off,
            ),
            Tunnel::Web => (
                "web-browser-symbolic",
                fl!("card-web-title"),
                self.config.web_conf.as_ref().filter(|_| self.config.web_nm_uuid.is_some()),
                self.web.state != TunnelState::Off,
            ),
        };
        let importing = self.importing == Some(t);
        let mut text = Column::new().width(Length::Fill).push(widget::text::body(title));
        let action: Element<'_, Message> = match summary {
            None => {
                text = text.push(widget::text::caption(if importing { fl!("cfg-importing") } else { fl!("cfg-none") }).class(Ink::Muted.text()));
                widget::button::custom(
                    Row::new()
                        .spacing(space_xxxs())
                        .align_y(Alignment::Center)
                        .push(widget::icon::from_name("document-open-symbolic").size(12).symbolic(true))
                        .push(widget::text::caption(fl!("cfg-import"))),
                )
                .class(theme::Button::Standard)
                .padding([2, 12])
                .on_press_maybe((!importing && !self.demo() && (t == Tunnel::Web || self.helper_ok == Some(true))).then_some(Message::Import(t)))
                .into()
            }
            Some(c) => {
                text = text
                    .push(mono(c.file_name.clone(), Type::new(12.0, 17.0), Weight::Normal, Some(Ink::Muted)))
                    .push(widget::text::caption(format!("{} · {}", c.server_label, c.endpoint)).class(Ink::Muted.text()));
                let mut chips = Row::new().spacing(space_xxxs());
                match t {
                    Tunnel::P2p => {
                        chips = chips.push(match (c.natpmp_capable, c.natpmp_hint) {
                            (Some(true), _) | (None, Hint::On) => chip(fl!("flag-natpmp-ok"), Some(Ink::Success)),
                            (Some(false), _) | (None, Hint::Off) => chip(fl!("flag-natpmp-no"), Some(Ink::Warning)),
                            (None, Hint::Unknown) => chip(fl!("flag-natpmp-unknown"), None),
                        });
                        chips = chips.push(chip(fl!("flag-p2p"), None)).push(chip(fl!("flag-killswitch"), None));
                    }
                    Tunnel::Web => {
                        chips = chips.push(chip(fl!("flag-in-nm"), Some(Ink::Success))).push(chip(format!("Proton {}", c.server_label), None));
                    }
                }
                text = text.push(widget::container(chips.wrap().vertical_spacing(space_xxxs())).padding([space_xxxs(), 0, 0, 0]));
                if t == Tunnel::P2p && c.moderate_nat_hint == Hint::On {
                    text = text.push(widget::text::caption(fl!("flag-moderate-nat")).class(Ink::Warning.text()));
                }
                widget::button::icon(widget::icon::from_name("document-open-symbolic").size(16))
                    .on_press_maybe((!on && !importing && !self.demo()).then_some(Message::Import(t)))
                    .tooltip(fl!("cfg-replace"))
                    .into()
            }
        };
        if let Some(e) = self.import_error.get(&t) {
            text = text.push(widget::text::caption(e.clone()).class(Ink::Destructive.text()));
        }
        widget::container(
            Row::new().spacing(space_s()).align_y(Alignment::Center).push(widget::icon::from_name(icon).size(20).symbolic(true)).push(text).push(action),
        )
        .padding([space_xxs(), space_m()])
        .into()
    }

    fn toggle_row(&self, t: Toggle, title: String, caption: String, on: bool) -> Element<'_, Message> {
        applet::padded_control(
            Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(Column::new().width(Length::Fill).push(widget::text::body(title)).push(widget::text::caption(caption).class(Ink::Muted.text())))
                .push(widget::toggler(on).on_toggle(move |v| Message::Set(t, v))),
        )
        .into()
    }

    fn field_row<'a>(&'a self, label: String, input: Element<'a, Message>) -> Element<'a, Message> {
        applet::padded_control(
            Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::text::body(label).width(Length::Fill))
                .push(widget::container(input).width(Length::Fixed(140.0))),
        )
        .into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let help = |s: String| widget::container(widget::text::caption(s).class(Ink::Muted.text())).padding([0, space_m()]);
        let mut col = Column::new()
            .push(ui::group_label(fl!("sec-configs")))
            .push(self.config_row(Tunnel::P2p))
            .push(self.config_row(Tunnel::Web))
            .push(help(fl!("cfg-help")))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("sec-qbit")))
            .push(self.toggle_row(Toggle::Launch, fl!("set-launch"), fl!("set-launch-cap"), self.config.launch_qbit))
            .push(self.toggle_row(Toggle::Quit, fl!("set-quit"), fl!("set-quit-cap"), self.config.quit_qbit))
            .push(self.toggle_row(Toggle::AutoPort, fl!("set-autoport"), fl!("set-autoport-cap"), self.config.auto_port));
        if self.config.auto_port {
            let mono_input = |placeholder: String, value: &str, f: Field| {
                widget::text_input(placeholder, value.to_owned()).font(cosmic::font::mono()).on_input(move |s| Message::Edit(f, s))
            };
            col = col
                .push(self.field_row(fl!("set-webui-port"), mono_input("8080".into(), &self.port_input, Field::WebUiPort).into()))
                .push(self.field_row(fl!("set-webui-user"), mono_input(String::new(), &self.config.webui_user, Field::User).into()))
                .push(
                    self.field_row(
                        fl!("set-webui-pass"),
                        widget::secure_input(String::new(), self.pass_input.clone(), None, true)
                            .font(cosmic::font::mono())
                            .on_input(|s| Message::Edit(Field::Pass, s))
                            .into(),
                    ),
                );
            let status = match self.test {
                Some(WebUiTest::Ok) => Some((fl!("webui-ok"), Ink::Success)),
                Some(WebUiTest::Unreachable) => Some((fl!("webui-unreachable"), Ink::Destructive)),
                Some(WebUiTest::AuthFailed) => Some((fl!("webui-auth"), Ink::Destructive)),
                Some(WebUiTest::NotRunning) => Some((fl!("webui-not-running"), Ink::Muted)),
                None => None,
            };
            if let Some((s, ink)) = status {
                col = col.push(widget::container(widget::text::caption(s).class(ink.text())).padding([space_xxxs(), space_m()]));
            }
        } else {
            col = col.push(help(fl!("manual-help")));
        }
        let detected = qbit::detect(None).map_or_else(|| fl!("qbit-cmd-none"), |c| c.display);
        col = col
            .push(self.field_row(
                fl!("set-qbit-cmd"),
                widget::text_input(detected, self.config.qbit_command.clone().unwrap_or_default()).on_input(|s| Message::Edit(Field::Command, s)).into(),
            ))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("sec-panel")))
            .push(self.toggle_row(Toggle::ShowPort, fl!("set-show-port"), fl!("set-show-port-cap"), self.config.show_port))
            .push(applet::padded_control(Column::new().spacing(space_xxs()).push(widget::text::body(fl!("set-icon-style"))).push(
                widget::segmented_control::horizontal(&self.icon_model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(Message::IconStyle),
            )))
            .push(help(if self.config.icon_style == IconStyle::Neon { fl!("icon-neon-cap") } else { fl!("icon-mono-cap") }))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("sec-startup")))
            .push(self.toggle_row(Toggle::Restore, fl!("set-restore"), fl!("set-restore-cap"), self.config.restore));
        Column::new()
            .push(ui::back_header(fl!("settings"), fl!("cancel"), Message::Page(Page::Main)))
            .push(widget::container(widget::scrollable(col)).max_height(860.0))
            .into()
    }

    // ---- demo -------------------------------------------------------------------------

    #[cfg(feature = "demo")]
    fn show_scene(&mut self, i: usize) {
        let s = crate::demo::scene(i);
        tracing::debug!(scene = i, name = s.name, "demo");
        self.demo = Some(i);
        self.p2p = s.p2p;
        self.web = s.web;
        self.outside = if s.leak { vec![0] } else { Vec::new() };
        self.helper_ok = Some(s.helper);
        self.config.p2p_conf = s.p2p_conf;
        self.config.web_conf = s.web_conf.clone();
        self.config.web_nm_uuid = s.web_conf.map(|_| "demo".into());
        self.config.icon_style = s.icon_style;
        self.config.show_port = true;
        self.page = s.page;
        self.test = s.test;
        self.qbit_cmd = Some(qbit::Command { argv: vec!["/usr/bin/qbittorrent".into()], conf: Default::default(), display: "qbittorrent".into() });
        self.confirm_quit = s.confirm;
        self.p2p_wanted = s.p2p_wanted;
        self.busy.clear();
        self.sync_icon_model();
    }

    #[cfg(feature = "demo")]
    fn panel_preview(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let pv = self.p2p_view();
        let wv = self.web_view();
        let icon = reticle(Reticle::of(pv.state, wv.state), model::issue(&pv, &wv), self.config.icon_style, 20.0);
        let content = panel::content(icon, self.chunks(), &p);
        widget::container(widget::container(content).padding([4, 12]).class(theme::Container::Card)).padding([8, 24]).into()
    }

    fn sync_icon_model(&mut self) {
        let want = self.config.icon_style;
        let found = self.icon_model.iter().find(|e| self.icon_model.data::<IconStyle>(*e) == Some(&want));
        if let Some(e) = found {
            self.icon_model.activate(e);
        }
    }
}

// ---- small widgets ------------------------------------------------------------------------

/// A `bg-component` box with `radius_s` (8).
fn boxed(content: Element<'_, Message>) -> Element<'_, Message> {
    widget::container(content)
        .padding([space_xs(), 12])
        .width(Length::Fill)
        .class(theme::Container::custom(|t| widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_s.into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}

fn banner<'a>(title: String, body: String, action: Option<(String, Message)>) -> Element<'a, Message> {
    let mut col = Column::new()
        .spacing(space_xxs())
        .push(widget::text::body(title).font(cosmic::font::semibold()))
        .push(widget::text::caption(body).class(Ink::Muted.text()));
    if let Some((label, msg)) = action {
        col = col.push(widget::button::standard(label).on_press(msg));
    }
    widget::container(boxed(col.into())).padding([space_xxs(), space_m()]).into()
}

/// A flag chip: `small-widget` fill, 11/18 semibold.
fn chip<'a>(text: String, ink: Option<Ink>) -> Element<'a, Message> {
    let t = widget::text(text)
        .size(11.0)
        .line_height(cosmic::iced::widget::text::LineHeight::Absolute(18.0.into()))
        .font(cosmic::font::semibold())
        .wrapping(cosmic::iced::widget::text::Wrapping::None);
    let t = match ink {
        Some(i) => t.class(i.text()),
        None => t,
    };
    widget::container(t)
        .padding([0, space_xs()])
        .class(theme::Container::custom(|th| {
            let c = th.cosmic();
            widget::container::Style {
                background: Some(cosmic::iced::Color::from(c.background(th.transparent).small_widget).into()),
                border: cosmic::iced::Border { radius: c.corner_radii.radius_xl.into(), ..Default::default() },
                ..Default::default()
            }
        }))
        .into()
}

/// The inverse pill toast.
fn toast<'a>(text: String) -> Element<'a, Message> {
    widget::container(widget::text::caption(text).font(cosmic::font::semibold()).class(cosmic::theme::Text::Custom(|t| cosmic::iced::widget::text::Style {
        color: Some(t.cosmic().background(t.transparent).base.into()),
        ..Default::default()
    })))
    .padding([space_xxxs(), space_s()])
    .class(theme::Container::custom(|t| widget::container::Style {
        background: Some(Ink::On.color(t).into()),
        border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
        ..Default::default()
    }))
    .into()
}

/// `⌄ 4.80 MB/s` / `⌃ 610 kB/s` (the Network applet's formatting).
fn rate<'a>(down: bool, bps: u64) -> Element<'a, Message> {
    let r = format_rate(Some(bps as f64));
    let ink = if down { Ink::Blue } else { Ink::Orange };
    let ty = Type::new(12.0, 17.0);
    Row::new()
        .spacing(space_xxxs())
        .align_y(Alignment::Center)
        .push(widget::icon::from_name(if down { "go-down-symbolic" } else { "go-up-symbolic" }).size(12).symbolic(true).icon().class(ink.svg()))
        .push(cell(mono(r.num.clone(), ty, Weight::Normal, Some(Ink::On)), ty.ch(4.0), Horizontal::Right))
        .push(cell(mono(r.unit, ty, Weight::Normal, Some(Ink::On)), ty.ch(4.0), Horizontal::Left))
        .into()
}

fn ago(sec: u32) -> String {
    if sec < 60 { fl!("ago-s", n = sec) } else { fl!("ago-m", n = (sec / 60)) }
}

fn dur(sec: u32) -> String {
    if sec < 60 { fl!("dur-s", n = sec) } else { fl!("dur-m", n = (sec / 60)) }
}

/// A `.ftl` string chosen at runtime (the keys the model returns).
fn fl_word(key: &str) -> String {
    match key {
        "panel-off" => fl!("panel-off"),
        "panel-on" => fl!("panel-on"),
        "panel-connecting" => fl!("panel-connecting"),
        "panel-error" => fl!("panel-error"),
        "head-none" => fl!("head-none"),
        "head-p2p" => fl!("head-p2p"),
        "head-web" => fl!("head-web"),
        "head-both" => fl!("head-both"),
        "head-warn" => fl!("head-warn"),
        "head-leak" => fl!("head-leak"),
        "head-error" => fl!("head-error"),
        other => other.to_owned(),
    }
}

/// A conf error's message.
fn fl_err(key: &str) -> String {
    match key {
        "err-not-wg" => fl!("err-not-wg"),
        "err-no-key" => fl!("err-no-key"),
        "err-no-default" => fl!("err-no-default"),
        "err-scripts" => fl!("err-scripts"),
        "err-peers" => fl!("err-peers"),
        "err-no-address" => fl!("err-no-address"),
        "err-no-peer-key" => fl!("err-no-peer-key"),
        "err-no-endpoint" => fl!("err-no-endpoint"),
        other => other.to_owned(),
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let (handler, config) = Config::load();
        tracing::debug!(p2p = config.p2p_conf.is_some(), web = config.web_nm_uuid.is_some(), "config loaded");
        let mut app = Self {
            core,
            popup: None,
            page: if crate::preview_settings() { Page::Settings } else { Page::Main },
            qbit_cmd: qbit::detect(config.qbit_command.as_deref()),
            port_input: config.webui_port.to_string(),
            config,
            handler,
            bus: None,
            helper_ok: None,
            p2p: Status::default(),
            web: Web::default(),
            busy: HashMap::new(),
            p2p_wanted: false,
            outside: Vec::new(),
            restarting: false,
            confirm_quit: false,
            launch_at: None,
            launched: false,
            applied_port: None,
            web_counters: None,
            toast: None,
            toast_id: 0,
            importing: None,
            import_error: HashMap::new(),
            pass: Zeroizing::default(),
            pass_input: String::new(),
            pass_saved: false,
            test: None,
            test_gen: 0,
            ticks: 0,
            restored: false,
            icon_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("icon-mono")).data(IconStyle::Mono))
                .insert(|b| b.text(fl!("icon-neon")).data(IconStyle::Neon))
                .build(),
            #[cfg(feature = "demo")]
            demo: None,
        };
        app.sync_icon_model();
        #[cfg(feature = "demo")]
        if crate::demo::enabled() {
            app.show_scene(crate::demo::first());
            if crate::demo::shot_path().is_some() {
                return (
                    app,
                    cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        Message::TakeShot
                    }),
                );
            }
            return (app, Task::none());
        }
        app.outside = qbit::outside();
        let bus = cosmic::task::future(async {
            match zbus::Connection::system().await {
                Ok(c) => {
                    let ok = helper::available(&c).await;
                    Message::Bus(Some(c), ok)
                }
                Err(e) => {
                    tracing::warn!("system bus: {e}");
                    Message::Bus(None, false)
                }
            }
        });
        let pass = cosmic::task::future(async { Message::PassLoaded(keyring::load().await) });
        (app, Task::batch([bus, pass]))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
            self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
            escape(),
        ];
        if !self.demo() {
            subs.push(Subscription::run(helper::events).map(Message::Helper));
            if let Some(uuid) = self.config.web_nm_uuid.clone() {
                subs.push(Subscription::run_with(uuid, |u| nm::watch(u.clone())).map(Message::Web));
            }
        }
        #[cfg(feature = "demo")]
        if self.demo.is_some() && crate::demo::shot_path().is_none() {
            subs.push(cosmic::iced::time::every(crate::demo::STEP).map(|_| Message::DemoNext));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::TogglePopup(bounds, offset) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                self.page = Page::Main;
                self.outside = if self.demo() { self.outside.clone() } else { qbit::outside() };
                let open = cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
                return Task::batch([open, self.recheck_helper()]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                let c = c.enforce();
                if c != self.config && !self.demo() {
                    if c.qbit_command != self.config.qbit_command {
                        self.qbit_cmd = qbit::detect(c.qbit_command.as_deref());
                    }
                    self.config = c;
                    self.sync_icon_model();
                }
            }
            Message::Page(p) => {
                self.page = p;
                self.confirm_quit = false;
                if p == Page::Settings && !self.demo() {
                    self.qbit_cmd = qbit::detect(self.config.qbit_command.as_deref());
                    return Task::batch([self.run_test(), self.recheck_helper()]);
                }
            }
            Message::Escape => {
                if self.confirm_quit {
                    self.confirm_quit = false;
                } else if self.page == Page::Settings {
                    self.page = Page::Main;
                } else if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
            Message::Tick => {
                if self.demo() {
                    return Task::none();
                }
                self.ticks += 1;
                let now = Instant::now();
                self.busy.retain(|_, since| now.duration_since(*since) < BUSY_LIMIT);
                let mut tasks = Vec::new();
                // Leak scan: every 3 s with the popup open, else every 10 s.
                let every = if self.popup.is_some() { 3 } else { 10 };
                if self.ticks.is_multiple_of(every) {
                    self.outside = qbit::outside();
                }
                if self.launch_at.is_some_and(|t| now >= t) && self.p2p.state == TunnelState::On {
                    tasks.push(self.launch());
                }
                // Web rates and handshake every 2 s while it's up.
                if self.web.state.up() && self.ticks.is_multiple_of(2) {
                    if let Some((rx, tx)) = nm::counters() {
                        if let Some((t0, rx0, tx0)) = self.web_counters {
                            let dt = now.duration_since(t0).as_secs_f64().max(0.5);
                            self.web.rx_bps = (rx.saturating_sub(rx0) as f64 / dt) as u64;
                            self.web.tx_bps = (tx.saturating_sub(tx0) as f64 / dt) as u64;
                        }
                        self.web_counters = Some((now, rx, tx));
                    }
                    if self.helper_ok == Some(true)
                        && let Some(bus) = self.bus()
                    {
                        tasks.push(cosmic::task::future(async move { Message::WebHandshake(helper::web_handshake(bus).await.ok()) }));
                    }
                } else if !self.web.state.up() {
                    self.web_counters = None;
                }
                return Task::batch(tasks);
            }
            Message::HelperFound(ok) => {
                if ok && self.helper_ok != Some(true) {
                    tracing::info!("the helper is now installed");
                    self.helper_ok = Some(true);
                    if let Some(b) = self.bus() {
                        return cosmic::task::future(async move { Message::HelperStatus(helper::status(b).await.unwrap_or_default()) });
                    }
                }
                self.helper_ok = Some(ok);
            }
            Message::Bus(bus, ok) => {
                tracing::debug!(helper = ok, "system bus");
                self.bus = bus.clone();
                self.helper_ok = Some(ok);
                let mut tasks = Vec::new();
                if let Some(bus) = bus {
                    if ok {
                        let b = bus.clone();
                        tasks.push(cosmic::task::future(async move {
                            match helper::status(b).await {
                                Ok(s) => Message::HelperStatus(s),
                                Err(_) => Message::HelperStatus(Status::default()),
                            }
                        }));
                    }
                    if let Some(uuid) = self.config.web_nm_uuid.clone() {
                        tasks.push(cosmic::task::future(async move { Message::Web(nm::state(&bus, &uuid).await.unwrap_or_default()) }));
                    } else {
                        tasks.push(self.restore());
                    }
                }
                return Task::batch(tasks);
            }
            Message::HelperStatus(s) => {
                // Adopt a tunnel that's already up (after a logout or applet restart).
                if s.state != TunnelState::Off {
                    self.p2p_wanted = true;
                    self.launched = s.qbit != QbitState::Stopped;
                }
                let t = self.on_status(s);
                return Task::batch([t, self.restore()]);
            }
            Message::Helper(helper::Event::Status(s)) => return self.on_status(s),
            Message::Helper(helper::Event::PortChanged(p)) => {
                tracing::debug!(port = p, "port changed");
                self.p2p.port = p;
                return self.push_port();
            }
            Message::Web(w) => {
                let terminal = matches!(w.state, TunnelState::On | TunnelState::Error | TunnelState::Off)
                    && !(w.state == TunnelState::Off && self.config.last_web && self.busy.contains_key(&Tunnel::Web) && w.error.is_empty());
                if terminal || w.state == TunnelState::Error {
                    self.busy.remove(&Tunnel::Web);
                }
                self.web.state = w.state;
                self.web.error = w.error;
                self.web.other_vpn = w.other_vpn;
                if !w.state.up() {
                    self.web.handshake_age = None;
                    self.web.rx_bps = 0;
                    self.web.tx_bps = 0;
                }
                return self.restore();
            }
            Message::WebHandshake(age) => self.web.handshake_age = age,
            Message::Switch(t, on) => {
                self.confirm_quit = false;
                return match (t, on) {
                    (Tunnel::P2p, true) => self.p2p_on(),
                    (Tunnel::P2p, false) => self.p2p_off(self.config.quit_qbit),
                    (Tunnel::Web, on) => self.web_switch(on),
                };
            }
            Message::Called(t, r) => {
                if let Err(e) = r {
                    self.busy.remove(&t);
                    match e {
                        CallError::QbitRunning => {
                            self.p2p_wanted = true;
                            self.confirm_quit = true;
                        }
                        CallError::Denied => {
                            self.p2p_wanted = self.p2p.state != TunnelState::Off;
                            return self.toast(fl!("toast-denied"));
                        }
                        CallError::Missing => self.helper_ok = Some(false),
                        CallError::Other(m) => {
                            tracing::warn!("torrent tunnel: {m}");
                            return self.toast(fl!("toast-error", why = m));
                        }
                    }
                }
            }
            Message::WebCalled(r) => {
                if let Err(e) = r {
                    tracing::warn!("web tunnel: {e}");
                    self.busy.remove(&Tunnel::Web);
                    self.web.state = TunnelState::Error;
                    self.web.error = e;
                }
            }
            Message::ConfirmQuit(quit) => {
                self.confirm_quit = false;
                if quit {
                    return self.p2p_off(true);
                }
            }
            Message::Launch => return self.launch(),
            Message::Launched(r) => match r {
                Ok(()) => {}
                Err(CallError::Denied) => {
                    self.launched = false;
                    return self.toast(fl!("toast-denied"));
                }
                Err(e) => {
                    self.launched = false;
                    let why = match e {
                        CallError::Other(m) => m,
                        _ => String::new(),
                    };
                    tracing::warn!("launching qBittorrent: {why}");
                    return self.toast(fl!("toast-error", why = why));
                }
            },
            Message::PortPushed(port, r) => {
                tracing::debug!(port, result = ?r, "port pushed");
                if r.is_err() {
                    self.applied_port = None;
                }
            }
            Message::CopyPort(p) => {
                let t = self.toast(fl!("port-copied", port = p.to_string()));
                return Task::batch([cosmic::iced::clipboard::write(p.to_string()), t]);
            }
            Message::RestartInVpn => {
                self.restarting = true;
                let pids = qbit::outside();
                return cosmic::task::future(async move {
                    qbit::terminate(pids).await;
                    Message::Terminated
                });
            }
            Message::Terminated => {
                self.restarting = false;
                self.outside = qbit::outside();
                if !self.outside.is_empty() {
                    return Task::none();
                }
                // Tunnel on (it launches qBittorrent), or launch now.
                if self.p2p.state == TunnelState::Off {
                    self.launched = false;
                    let saved = self.config.launch_qbit;
                    let t = self.p2p_on();
                    if !saved {
                        self.launch_at = Some(Instant::now() + PORT_WAIT * 3);
                    }
                    return t;
                }
                return self.launch();
            }
            Message::ToastEnd(id) => {
                if self.toast.as_ref().is_some_and(|(_, i)| *i == id) {
                    self.toast = None;
                }
            }
            Message::Import(t) => return self.pick(t),
            Message::Picked(t, picked) => {
                if let Some((name, text)) = picked {
                    return self.import(t, name, text);
                }
            }
            Message::Imported(t, r) => {
                self.importing = None;
                match r {
                    Ok(s) => {
                        tracing::info!(server = %s.server_label, "torrent config imported");
                        self.save(|c, h| c.set_p2p_conf(h, Some(s)));
                    }
                    Err(e) => {
                        self.import_error.insert(t, fl!("cfg-import-failed", why = e));
                    }
                }
            }
            Message::WebImported(s, r) => {
                self.importing = None;
                match r {
                    Ok(uuid) => {
                        tracing::info!(server = %s.server_label, "web config imported into NetworkManager");
                        self.save(|c, h| c.set_web_nm_uuid(h, Some(uuid)));
                        self.save(|c, h| c.set_web_conf(h, Some(s)));
                    }
                    Err(e) => {
                        self.import_error.insert(Tunnel::Web, fl!("cfg-import-failed", why = e));
                    }
                }
            }
            Message::Set(t, on) => match t {
                Toggle::Launch => self.save(|c, h| c.set_launch_qbit(h, on)),
                Toggle::Quit => self.save(|c, h| c.set_quit_qbit(h, on)),
                Toggle::AutoPort => {
                    self.save(|c, h| c.set_auto_port(h, on));
                    return self.push_port();
                }
                Toggle::ShowPort => self.save(|c, h| c.set_show_port(h, on)),
                Toggle::Restore => self.save(|c, h| c.set_restore(h, on)),
            },
            Message::Edit(f, s) => match f {
                Field::WebUiPort => {
                    let digits: String = s.chars().filter(char::is_ascii_digit).take(5).collect();
                    self.port_input = digits.clone();
                    if let Ok(p) = digits.parse::<u16>()
                        && p != 0
                    {
                        self.save(|c, h| c.set_webui_port(h, p));
                    }
                    return self.test_soon();
                }
                Field::User => {
                    self.save(|c, h| c.set_webui_user(h, s));
                    return self.test_soon();
                }
                Field::Pass => {
                    self.pass_input = s;
                    self.pass_saved = false;
                    return self.test_soon();
                }
                Field::Command => {
                    let v = (!s.trim().is_empty()).then_some(s);
                    self.save(|c, h| c.set_qbit_command(h, v));
                    self.qbit_cmd = qbit::detect(self.config.qbit_command.as_deref());
                }
            },
            Message::TestDue(g) => {
                if g == self.test_gen {
                    let mut tasks = Vec::new();
                    if !self.pass_saved && self.pass_input != *self.pass {
                        self.pass = Zeroizing::new(self.pass_input.clone());
                        self.pass_saved = true;
                        let p = self.pass.clone();
                        tasks.push(cosmic::task::future(async move { Message::PassStored(keyring::store(p).await) }));
                    }
                    tasks.push(self.run_test());
                    return Task::batch(tasks);
                }
            }
            Message::Tested(r) => {
                self.test = Some(match r {
                    Ok(s) => WebUiTest::parse(&s),
                    Err(_) => WebUiTest::Unreachable,
                });
            }
            Message::PassLoaded(p) => {
                self.pass_input = p.to_string();
                self.pass = p;
                self.pass_saved = true;
            }
            Message::PassStored(ok) => tracing::debug!(ok, "Web UI password stored"),
            Message::IconStyle(e) => {
                if let Some(v) = self.icon_model.data::<IconStyle>(e).copied() {
                    self.save(|c, h| c.set_icon_style(h, v));
                    self.sync_icon_model();
                }
            }
            #[cfg(feature = "demo")]
            Message::DemoNext => {
                if let Some(i) = self.demo {
                    self.show_scene(i + 1);
                }
            }
            #[cfg(feature = "demo")]
            Message::TakeShot => {
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::screenshot(id).map(|s| cosmic::Action::App(Message::Shot(s)));
                }
            }
            #[cfg(feature = "demo")]
            Message::Shot(s) => {
                if let Some(path) = crate::demo::shot_path() {
                    match crate::demo::save_shot(&path, &s) {
                        Ok(()) => tracing::info!("saved {}", path.display()),
                        Err(e) => tracing::error!("saving {}: {e}", path.display()),
                    }
                }
                std::process::exit(0);
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if preview() {
            #[cfg(feature = "demo")]
            if self.demo.is_some() {
                return widget::container(Column::new().push(self.panel_preview()).push(self.popup_view())).width(Length::Fixed(360.0)).into();
            }
            return widget::container(self.popup_view()).width(Length::Fixed(360.0)).into();
        }
        self.panel_view()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        widget::text("").into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        (!preview()).then(applet::style)
    }
}

fn escape() -> Subscription<Message> {
    cosmic::iced::event::listen_with(|event, _, _| match event {
        cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => Some(Message::Escape),
        _ => None,
    })
}

fn open_popup(bounds: Rectangle, offset: cosmic::iced::Vector) -> cosmic::surface::Action<Message> {
    app_popup::<App>(
        |_| Default::default(),
        move |state: &mut App| {
            let new_id = Id::unique();
            state.popup = Some(new_id);
            let parent = state.core.main_window_id().unwrap_or(Id::RESERVED);
            let mut settings = state.core.applet.get_popup_settings(parent, new_id, None, None, None);
            settings.positioner.anchor_rect =
                Rectangle { x: (bounds.x - offset.x) as i32, y: (bounds.y - offset.y) as i32, width: bounds.width as i32, height: bounds.height as i32 };
            settings
        },
        Some(Box::new(|state: &App| Element::from(state.core.applet.popup_container(state.popup_view())).map(cosmic::Action::App))),
    )
}

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 4] = [include_str!("app.rs"), include_str!("widgets/panel.rs"), include_str!("widgets/reticle.rs"), include_str!("model.rs")];

    #[test]
    fn no_hex_colours() {
        for src in SOURCES {
            assert!(!src.contains(concat!("Color::from_rgb", "(")), "hard-coded colour");
            assert!(!src.contains(concat!("color", "!(")), "hard-coded colour");
        }
    }

    #[test]
    fn every_string_key_exists() {
        let ftl = include_str!("../i18n/en/cosmic_applet_vpn.ftl");
        let keys: Vec<&str> = ftl.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())).collect();
        for src in SOURCES {
            for part in src.split(concat!("fl", "!(\"")).skip(1) {
                let key = part.split('"').next().unwrap();
                assert!(keys.contains(&key), "{key} missing from the .ftl file");
            }
        }
        // Keys the model hands to `fl_word`.
        for k in [
            "head-none",
            "head-p2p",
            "head-web",
            "head-both",
            "head-warn",
            "head-leak",
            "head-error",
            "panel-on",
            "panel-off",
            "panel-connecting",
            "panel-error",
        ] {
            assert!(keys.contains(&k), "{k}");
        }
        for e in [
            vpn_common::conf::ConfError::NotWireGuard,
            vpn_common::conf::ConfError::NoKey,
            vpn_common::conf::ConfError::NoDefaultRoute,
            vpn_common::conf::ConfError::Scripts,
            vpn_common::conf::ConfError::Peers,
            vpn_common::conf::ConfError::NoAddress,
            vpn_common::conf::ConfError::NoPeerKey,
            vpn_common::conf::ConfError::NoEndpoint,
        ] {
            assert!(keys.contains(&e.ftl_key()), "{}", e.ftl_key());
        }
    }
}
