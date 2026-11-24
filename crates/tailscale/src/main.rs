//! Tailscale: a COSMIC panel applet showing the tailnet's state, with an
//! on/off switch, an exit-node picker and a device list. It talks only to
//! `tailscaled`'s local API on its Unix socket.

mod api;
mod app;
mod config;
mod localize;
mod model;
mod state;
mod widgets;

fn main() -> cosmic::iced::Result {
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
