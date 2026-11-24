//! Tailscale: a COSMIC panel applet showing the tailnet's state, with an
//! on/off switch, an exit-node picker and a device list. It talks only to
//! `tailscaled`'s local API on its Unix socket.

mod api;
mod app;
mod config;
#[cfg(feature = "demo")]
mod demo;
mod localize;
mod model;
mod report;
mod state;
mod widgets;

fn main() -> cosmic::iced::Result {
    if std::env::args().nth(1).as_deref() == Some("--status") {
        std::process::exit(status());
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,cosmic::theme=off")))
        .with_writer(std::io::stderr)
        .init();
    localize::localize();
    if preview() {
        let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(360.0, 1000.0));
        return cosmic::app::run::<app::App>(settings, ());
    }
    cosmic::applet::run::<app::App>(())
}

/// `APPLET_PREVIEW=settings`: the preview opens on the settings page.
pub fn preview_settings() -> bool {
    std::env::var_os("APPLET_PREVIEW").is_some_and(|v| v == "settings")
}

/// `APPLET_PREVIEW` is set: run as a window showing the popup.
pub fn preview() -> bool {
    std::env::var_os("APPLET_PREVIEW").is_some()
}

/// `cosmic-applet-tailscale --status`: one look at the daemon through the
/// applet's own client and model, printed as text. Exits 1 if there is no
/// daemon on the socket.
fn status() -> i32 {
    use futures_util::StreamExt;
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("starting the runtime: {e}");
            return 2;
        }
    };
    runtime.block_on(async {
        let (user, uid) = app::current_user();
        let mut st = state::State::new(user, uid);
        let mut bus = Box::pin(api::watch());
        let first = tokio::time::timeout(std::time::Duration::from_secs(3), bus.next()).await;
        st.apply(first.ok().flatten().unwrap_or(api::BusEvent::Down));
        if st.snap.reachable {
            match api::status().await {
                Ok(s) => st.snap.status = s,
                Err(e) => eprintln!("status: {e:?}"),
            }
        }
        print!("{}", report::report(&st, chrono::Utc::now()));
        if st.snap.reachable { 0 } else { 1 }
    })
}
