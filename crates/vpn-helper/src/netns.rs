//! The private namespaces and their WireGuard interfaces (SPEC §4.1–4.2),
//! over netlink only. The reference commands are in SPEC §4.1. There are two
//! [`Space`]s, built the same way: one for torrents, one for the browser.
//!
//! Anything that must run *inside* the namespace runs on a fresh thread
//! that enters it and then exits, so no shared thread ever changes namespace.

use std::fs::File;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::TryStreamExt;
use netlink_packet_route::rule::{RuleAction, RuleAttribute};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sched::{CloneFlags, setns, unshare};
use rtnetlink::{IpVersion, LinkUnspec, RouteMessageBuilder};
use vpn_common::conf::{Cidr, Conf};
use vpn_common::names::{BROWSER_IF, BROWSER_NETNS_PATH, BROWSER_V6_PLACEHOLDER, FWMARK, NETNS_PATH, P2P_IF, P2P_V6_PLACEHOLDER, RULE_PRIORITY};
use wireguard_uapi::linux::set::{AllowedIp, Device, Peer, WgDeviceF, WgPeerF};
use wireguard_uapi::{DeviceInterface, RouteSocket, WgSocket};

const NETNS_DIR: &str = "/run/netns";

/// A namespace whose only way out is its own WireGuard interface.
#[derive(Debug)]
pub struct Space {
    /// The bind-mounted namespace file under `/run/netns`.
    pub path: &'static str,
    pub ifname: &'static str,
    /// Holds the `resolv.conf` bound over `/etc/resolv.conf` for apps inside.
    pub resolv_dir: &'static str,
    /// The address that makes IPv6 die in the tunnel when Proton gives none.
    v6_placeholder: &'static str,
}

impl Space {
    pub fn resolv(&self) -> String {
        format!("{}/resolv.conf", self.resolv_dir)
    }
}

pub static P2P: Space = Space { path: NETNS_PATH, ifname: P2P_IF, resolv_dir: "/etc/netns/vpn-p2p", v6_placeholder: P2P_V6_PLACEHOLDER };
pub static BROWSER: Space =
    Space { path: BROWSER_NETNS_PATH, ifname: BROWSER_IF, resolv_dir: "/etc/netns/vpn-browser", v6_placeholder: BROWSER_V6_PLACEHOLDER };
/// The kernel's main routing table.
const TABLE_MAIN: u32 = 254;
const DEFAULT_MTU: u32 = 1420;
const KEEPALIVE: u16 = 25;

pub type Result<T> = std::result::Result<T, String>;

fn err(what: &str) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// Runs `f` on a new thread inside the namespace.
pub fn in_netns<T: Send + 'static>(sp: &'static Space, f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    std::thread::spawn(move || {
        let ns = File::open(sp.path).map_err(err("opening the namespace"))?;
        setns(&ns, CloneFlags::CLONE_NEWNET).map_err(|e| format!("entering the namespace: {e}"))?;
        Ok(f())
    })
    .join()
    .map_err(|_| "a namespace thread panicked".to_owned())?
}

/// Runs an async job inside the namespace, on its own small runtime.
pub fn in_netns_async<T: Send + 'static, F: std::future::Future<Output = T>>(sp: &'static Space, f: impl FnOnce() -> F + Send + 'static) -> Result<T> {
    in_netns(sp, move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(err("starting a runtime"))?;
        Ok(rt.block_on(f()))
    })?
}

/// Whether the namespace exists and is mounted.
pub fn exists(sp: &Space) -> bool {
    // A bind-mounted nsfs file has a different device from its directory.
    let (Ok(f), Ok(d)) = (std::fs::metadata(sp.path), std::fs::metadata(NETNS_DIR)) else { return false };
    use std::os::unix::fs::MetadataExt;
    f.dev() != d.dev()
}

/// `ip netns add`: a namespace pinned by a bind mount.
fn create_netns(sp: &'static Space) -> Result<()> {
    std::fs::create_dir_all(NETNS_DIR).map_err(err("creating /run/netns"))?;
    // Make /run/netns a shared mount point, as `ip netns` does, so the bind
    // mount is visible to every mount namespace (systemd, the applet).
    let none: Option<&str> = None;
    if mount(none, NETNS_DIR, none, MsFlags::MS_SHARED | MsFlags::MS_REC, none).is_err() {
        mount(Some(NETNS_DIR), NETNS_DIR, none, MsFlags::MS_BIND | MsFlags::MS_REC, none).map_err(|e| format!("binding /run/netns: {e}"))?;
        mount(none, NETNS_DIR, none, MsFlags::MS_SHARED | MsFlags::MS_REC, none).map_err(|e| format!("sharing /run/netns: {e}"))?;
    }
    File::options().write(true).create_new(true).open(sp.path).map_err(err("creating the namespace file"))?;
    std::thread::spawn(move || {
        unshare(CloneFlags::CLONE_NEWNET).map_err(|e| format!("unshare: {e}"))?;
        let none: Option<&str> = None;
        mount(Some("/proc/thread-self/ns/net"), sp.path, none, MsFlags::MS_BIND, none).map_err(|e| format!("pinning the namespace: {e}"))
    })
    .join()
    .map_err(|_| "the namespace thread panicked".to_owned())?
    .inspect_err(|_| {
        let _ = std::fs::remove_file(sp.path);
    })
}

/// `ip netns del`. Destroys the tunnel and everything in the namespace once
/// no process is left inside.
fn delete_netns(sp: &Space) -> Result<()> {
    if Path::new(sp.path).exists() {
        let _ = umount2(sp.path, MntFlags::MNT_DETACH);
        std::fs::remove_file(sp.path).map_err(err("removing the namespace file"))?;
    }
    Ok(())
}

/// Resolves the endpoint (hostnames once, here in the host namespace).
pub fn resolve(conf: &Conf) -> Result<SocketAddr> {
    if let Some(ip) = conf.endpoint.ip() {
        return Ok(SocketAddr::new(ip, conf.endpoint.port));
    }
    (conf.endpoint.host.as_str(), conf.endpoint.port)
        .to_socket_addrs()
        .map_err(err("resolving the endpoint"))?
        .next()
        .ok_or_else(|| "the endpoint has no address".to_owned())
}

/// Keys, peer and fwmark for a WireGuard device in the current namespace.
fn configure_wg(ifname: &str, conf: &Conf, endpoint: &SocketAddr) -> Result<()> {
    let v4_any = Ipv4Addr::UNSPECIFIED.into();
    let v6_any = Ipv6Addr::UNSPECIFIED.into();
    let allowed = vec![AllowedIp { ipaddr: &v4_any, cidr_mask: Some(0) }, AllowedIp { ipaddr: &v6_any, cidr_mask: Some(0) }];
    let mut peer = Peer::from_public_key(&conf.peer_public_key)
        .flags(vec![WgPeerF::ReplaceAllowedIps])
        .endpoint(endpoint)
        .persistent_keepalive_interval(conf.keepalive.unwrap_or(KEEPALIVE).max(KEEPALIVE))
        .allowed_ips(allowed);
    if let Some(psk) = &conf.preshared_key {
        peer = peer.preshared_key(psk.bytes());
    }
    let device = Device::from_ifname(ifname).flags(vec![WgDeviceF::ReplacePeers]).private_key(conf.private_key.bytes()).fwmark(FWMARK).peers(vec![peer]);
    let mut wg = WgSocket::connect().map_err(|e| format!("WireGuard netlink: {e}"))?;
    wg.set_device(device).map_err(|e| format!("configuring {ifname}: {e}"))
}

/// Points the peer at `endpoint` again, forcing a fresh handshake. Runs inside the namespace.
pub fn reset_endpoint(sp: &'static Space, conf: &Conf, endpoint: SocketAddr) -> Result<()> {
    let pk = conf.peer_public_key;
    in_netns(sp, move || {
        let peer = Peer::from_public_key(&pk).endpoint(&endpoint);
        let mut wg = WgSocket::connect().map_err(|e| format!("WireGuard netlink: {e}"))?;
        wg.set_device(Device::from_ifname(sp.ifname).peers(vec![peer])).map_err(|e| format!("resetting the endpoint: {e}"))
    })?
}

/// The peer's counters, read inside the namespace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgStats {
    /// Unix seconds; 0 = no handshake yet.
    pub latest_handshake: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

fn read_stats(ifname: &str) -> Result<WgStats> {
    let mut wg = WgSocket::connect().map_err(|e| format!("WireGuard netlink: {e}"))?;
    let dev = wg.get_device(DeviceInterface::from_name(ifname)).map_err(|e| format!("reading {ifname}: {e}"))?;
    // `dev` holds the private key: only the peer counters leave this function.
    let stats = dev.peers.first().map_or_else(WgStats::default, |p| WgStats {
        latest_handshake: p.last_handshake_time.as_secs(),
        rx_bytes: p.rx_bytes,
        tx_bytes: p.tx_bytes,
    });
    drop(dev);
    Ok(stats)
}

/// The tunnel's counters (inside its namespace).
pub fn stats(sp: &'static Space) -> Result<WgStats> {
    in_netns(sp, || read_stats(sp.ifname))?
}

/// The web tunnel's handshake age in seconds (host namespace); 0 if none.
pub fn web_handshake_age(ifname: &str) -> u32 {
    std::thread::spawn({
        let ifname = ifname.to_owned();
        move || read_stats(&ifname)
    })
    .join()
    .ok()
    .and_then(Result::ok)
    .filter(|s| s.latest_handshake > 0)
    .map_or(0, |s| now().saturating_sub(s.latest_handshake).min(u64::from(u32::MAX)) as u32)
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

async fn link_index(handle: &rtnetlink::Handle, name: &str) -> Result<Option<u32>> {
    let mut links = handle.link().get().match_name(name.to_owned()).execute();
    match links.try_next().await {
        Ok(Some(l)) => Ok(Some(l.header.index)),
        Ok(None) => Ok(None),
        // ENODEV: no such link.
        Err(rtnetlink::Error::NetlinkError(e)) if e.raw_code() == -19 => Ok(None),
        Err(e) => Err(format!("looking up {name}: {e}")),
    }
}

/// The fwmark rules exist (v4 and v6).
async fn add_rules(handle: &rtnetlink::Handle) -> Result<()> {
    for v in [IpVersion::V4, IpVersion::V6] {
        if find_rules(handle, v.clone()).await?.is_empty() {
            let req = handle.rule().add().fw_mark(FWMARK).priority(RULE_PRIORITY).table_id(TABLE_MAIN).action(RuleAction::ToTable);
            let r = match v {
                IpVersion::V4 => req.v4().execute().await,
                IpVersion::V6 => req.v6().execute().await,
            };
            r.map_err(|e| format!("adding the fwmark rule: {e}"))?;
        }
    }
    Ok(())
}

async fn find_rules(handle: &rtnetlink::Handle, v: IpVersion) -> Result<Vec<netlink_packet_route::rule::RuleMessage>> {
    let rules: Vec<_> = handle.rule().get(v).execute().try_collect().await.map_err(|e| format!("listing rules: {e}"))?;
    Ok(rules
        .into_iter()
        .filter(|r| r.attributes.contains(&RuleAttribute::FwMark(FWMARK)) && r.attributes.contains(&RuleAttribute::Priority(RULE_PRIORITY)))
        .collect())
}

async fn del_rules(handle: &rtnetlink::Handle) -> Result<()> {
    for v in [IpVersion::V4, IpVersion::V6] {
        for r in find_rules(handle, v).await? {
            handle.rule().del(r).execute().await.map_err(|e| format!("removing the fwmark rule: {e}"))?;
        }
    }
    Ok(())
}

async fn host_handle() -> Result<rtnetlink::Handle> {
    let (conn, handle, _) = rtnetlink::new_connection().map_err(err("netlink"))?;
    tokio::spawn(conn);
    Ok(handle)
}

fn write_resolv(sp: &Space, conf: &Conf) -> Result<()> {
    std::fs::create_dir_all(sp.resolv_dir).map_err(err("creating the resolv.conf directory"))?;
    let mut text = String::new();
    for d in &conf.dns {
        text.push_str(&format!("nameserver {d}\n"));
    }
    if text.is_empty() {
        text.push_str("nameserver 10.2.0.1\n");
    }
    let tmp = format!("{}/.resolv.conf.tmp", sp.resolv_dir);
    std::fs::write(&tmp, text).map_err(err("writing resolv.conf"))?;
    std::fs::rename(tmp, sp.resolv()).map_err(err("writing resolv.conf"))
}

/// Brings the tunnel up (SPEC §4.1). Idempotent: an existing namespace with
/// its interface inside is adopted, not rebuilt.
pub async fn up(sp: &'static Space, conf: &Conf) -> Result<()> {
    let ifname = sp.ifname;
    if exists(sp) && stats(sp).is_ok() {
        tracing::info!("adopting the existing namespace");
        write_resolv(sp, conf)?;
        return add_rules(&host_handle().await?).await;
    }
    let endpoint = resolve(conf)?;
    let host = host_handle().await?;
    if !exists(sp) {
        create_netns(sp)?;
    }
    // A stale interface left in the host namespace by a crash mid-setup.
    if let Some(i) = link_index(&host, ifname).await? {
        let _ = host.link().del(i).execute().await;
    }
    // Created in the HOST namespace, so its UDP socket stays here.
    {
        let mut rs = RouteSocket::connect().map_err(|e| format!("netlink: {e}"))?;
        rs.add_device(ifname).map_err(|e| format!("creating {ifname}: {e}"))?;
    }
    let setup = async {
        configure_wg(ifname, conf, &endpoint)?;
        let index = link_index(&host, ifname).await?.ok_or_else(|| format!("{ifname} vanished"))?;
        let ns = File::open(sp.path).map_err(err("opening the namespace"))?;
        host.link().set(LinkUnspec::new_with_index(index).setns_by_fd(ns.as_raw_fd()).build()).execute().await.map_err(|e| format!("moving {ifname}: {e}"))?;
        let addrs: Vec<Cidr> = conf.addresses.clone();
        let mtu = conf.mtu.map_or(DEFAULT_MTU, u32::from);
        in_netns_async(sp, move || inside_setup(sp, addrs, mtu))??;
        write_resolv(sp, conf)?;
        add_rules(&host).await
    };
    if let Err(e) = setup.await {
        // Never leave a half-built tunnel behind; the namespace itself stays
        // (it has no way out, so the kill switch holds).
        if let Ok(Some(i)) = link_index(&host, ifname).await {
            let _ = host.link().del(i).execute().await;
        }
        return Err(e);
    }
    Ok(())
}

/// Inside the namespace: `lo` up, addresses, MTU, the tunnel up, default routes.
async fn inside_setup(sp: &'static Space, addrs: Vec<Cidr>, mtu: u32) -> Result<()> {
    let ifname = sp.ifname;
    let (conn, h, _) = rtnetlink::new_connection().map_err(err("netlink in the namespace"))?;
    tokio::spawn(conn);
    let lo = link_index(&h, "lo").await?.ok_or("no lo in the namespace")?;
    h.link().set(LinkUnspec::new_with_index(lo).up().build()).execute().await.map_err(|e| format!("lo up: {e}"))?;
    let wg = link_index(&h, ifname).await?.ok_or_else(|| format!("{ifname} isn't in the namespace"))?;
    let mut has_v6 = false;
    for a in &addrs {
        has_v6 |= a.addr.is_ipv6();
        h.address().add(wg, a.addr, a.prefix).execute().await.map_err(|e| format!("adding {a}: {e}"))?;
    }
    if !has_v6 {
        // No IPv6 from Proton: v6 still routes into the tunnel and dies there.
        let p = Cidr::parse(sp.v6_placeholder).expect("valid placeholder");
        h.address().add(wg, p.addr, p.prefix).execute().await.map_err(|e| format!("adding {p}: {e}"))?;
    }
    h.link().set(LinkUnspec::new_with_index(wg).mtu(mtu).up().build()).execute().await.map_err(|e| format!("{ifname} up: {e}"))?;
    let v4 = RouteMessageBuilder::<Ipv4Addr>::new().destination_prefix(Ipv4Addr::UNSPECIFIED, 0).output_interface(wg).build();
    h.route().add(v4).execute().await.map_err(|e| format!("IPv4 default route: {e}"))?;
    let v6 = RouteMessageBuilder::<Ipv6Addr>::new().destination_prefix(Ipv6Addr::UNSPECIFIED, 0).output_interface(wg).build();
    h.route().add(v6).execute().await.map_err(|e| format!("IPv6 default route: {e}"))?;
    Ok(())
}

/// Tears the tunnel down (SPEC §4.2 steps 3–4). The fwmark rules are shared
/// by both spaces, so they go only with the last one.
pub async fn down(sp: &Space) -> Result<()> {
    delete_netns(sp)?;
    let _ = std::fs::remove_dir_all(sp.resolv_dir);
    if exists(&P2P) || exists(&BROWSER) {
        return Ok(());
    }
    del_rules(&host_handle().await?).await
}

/// Every process inside the namespace.
pub fn pids(sp: &Space) -> Vec<i32> {
    use std::os::unix::fs::MetadataExt;
    let Ok(ns) = std::fs::metadata(sp.path).map(|m| (m.dev(), m.ino())) else { return Vec::new() };
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let net = std::fs::metadata(e.path().join("ns/net")).ok()?;
            ((net.dev(), net.ino()) == ns).then_some(pid)
        })
        .collect()
}
