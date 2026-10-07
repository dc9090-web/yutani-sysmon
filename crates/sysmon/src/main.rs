//! System Monitor: a COSMIC panel applet showing CPU, AMD GPU, memory and
//! disk I/O.

mod app;
mod config;
mod localize;
mod sample;
mod widgets;

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,cosmic::theme=off")))
        .with_writer(std::io::stderr)
        .init();
    localize::localize();
    if preview() {
        // The popup in an ordinary window, for checks without a panel:
        // `APPLET_PREVIEW=1 cosmic-applet-sysmon`.
        let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(360.0, 1040.0));
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
