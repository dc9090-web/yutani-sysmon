//! VPN: a COSMIC panel applet with two ProtonVPN WireGuard switches: a
//! qBittorrent-only tunnel in its own network namespace (kill switch, port
//! forwarding), and a NetworkManager tunnel for all web traffic.

mod app;
mod config;
#[cfg(feature = "demo")]
mod demo;
mod helper;
mod keyring;
mod localize;
mod model;
mod nm;
mod qbit;
mod widgets;

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--check-conf") {
        std::process::exit(check_conf(&args[2..]));
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

/// `cosmic-applet-vpn --check-conf <file>…`: prints what the parser makes of
/// each conf, without its private key (SPEC §3.2). Works on real files, and
/// on copies with `PrivateKey = REDACTED`.
fn check_conf(files: &[String]) -> i32 {
    use vpn_common::conf;
    if files.is_empty() {
        eprintln!("usage: cosmic-applet-vpn --check-conf <file.conf>…");
        return 2;
    }
    let mut status = 0;
    for f in files {
        println!("{f}");
        let text = match std::fs::read_to_string(f) {
            Ok(t) => zeroize::Zeroizing::new(t),
            Err(e) => {
                println!("  can't read: {e}");
                status = 1;
                continue;
            }
        };
        let name = std::path::Path::new(f).file_name().map_or_else(|| f.clone(), |n| n.to_string_lossy().into_owned());
        match conf::parse_structure(&text, &name) {
            Ok(s) => {
                let full = conf::parse(&text, &name);
                let c = &s.summary;
                println!("  structure     OK");
                println!(
                    "  private key   {}",
                    match (&full, s.has_private_key) {
                        (Ok(_), _) => "present, valid (not shown)",
                        (Err(_), true) => "present but not a valid key (redacted copy?)",
                        (Err(_), false) => "MISSING",
                    }
                );
                println!("  server label  {}", c.server_label);
                println!("  endpoint      {}", c.endpoint);
                println!("  address v4    {}", c.addr_v4);
                println!("  IPv6          {}", if c.has_v6 { "yes" } else { "no (the applet blocks IPv6 in the tunnel)" });
                println!("  DNS           {}", c.dns.join(", "));
                println!("  NAT-PMP hint  {}", c.natpmp_hint.as_str());
                println!("  Moderate NAT  {}", c.moderate_nat_hint.as_str());
                if let Ok(full) = &full {
                    println!("  MTU           {}", full.mtu.map_or_else(|| "default".into(), |m| m.to_string()));
                    println!("  keepalive     {}", full.keepalive.map_or_else(|| "none (the applet uses 25 s)".into(), |k| format!("{k} s")));
                    println!("  preshared key {}", if full.preshared_key.is_some() { "present (not shown)" } else { "none" });
                }
                let p2p_ok = c.natpmp_hint != conf::Hint::Off && c.moderate_nat_hint != conf::Hint::On;
                println!("  for torrents  {}", if p2p_ok { "suitable (needs a P2P server)" } else { "NOT suitable: needs NAT-PMP on and Moderate NAT off" });
            }
            Err(e) => {
                println!("  REFUSED: {e}");
                status = 1;
            }
        }
    }
    status
}

/// `APPLET_PREVIEW=settings`: the preview opens on the settings page.
pub fn preview_settings() -> bool {
    std::env::var_os("APPLET_PREVIEW").is_some_and(|v| v == "settings")
}

/// `APPLET_PREVIEW` is set: run as a window showing the popup.
pub fn preview() -> bool {
    std::env::var_os("APPLET_PREVIEW").is_some()
}
