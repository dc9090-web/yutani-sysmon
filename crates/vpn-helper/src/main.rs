//! `cosmic-vpn-helper`: the COSMIC VPN applet's root helper. D-Bus activated
//! on the system bus, polkit-guarded; it owns the `vpn-p2p` namespace, the
//! `wg-p2p` tunnel, NAT-PMP and the qBittorrent unit (SPEC §2, §4, §5).

mod netns;
mod portmap;
mod qbit;
mod service;

use vpn_common::dbus::{BUS_NAME, OBJECT_PATH};

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .without_time()
        .init();
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("cosmic-vpn-helper must run as root (it's started by D-Bus)");
        std::process::exit(1);
    }
    let helper = service::Helper::new();
    helper.adopt().await;
    let conn = zbus::connection::Builder::system()?.name(BUS_NAME)?.serve_at(OBJECT_PATH, helper.clone())?.build().await?;
    tracing::info!("ready on {BUS_NAME}");
    helper.watch(conn).await;
    Ok(())
}
