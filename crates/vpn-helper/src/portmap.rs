//! One NAT-PMP round against Proton's gateway, inside the namespace
//! (SPEC §4.3): UDP then TCP, each retried at 250 ms, 500 ms, 1 s and 2 s.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use vpn_common::natpmp::{self, GATEWAY, Mapping, Proto, RETRIES_MS};

use crate::netns;

/// How a round ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    /// The mapped port (UDP's, if TCP disagreed).
    Port(u16),
    /// The gateway answered with a non-zero result code.
    Refused(u16),
    /// No answer at all.
    NoResponse,
}

fn ask(sock: &UdpSocket, proto: Proto) -> Option<Mapping> {
    let gw = SocketAddrV4::new(Ipv4Addr::from(GATEWAY), natpmp::PORT);
    let req = natpmp::request(proto);
    let mut buf = [0u8; 32];
    for wait in RETRIES_MS {
        if sock.send_to(&req, gw).is_err() {
            return None;
        }
        let _ = sock.set_read_timeout(Some(Duration::from_millis(wait)));
        while let Ok((n, from)) = sock.recv_from(&mut buf) {
            if from == gw.into()
                && let Some(m) = natpmp::response(&buf[..n]).filter(|m| m.proto == proto)
            {
                return Some(m);
            }
        }
    }
    None
}

/// One round, run on a thread inside the namespace.
pub fn round() -> Round {
    let result = netns::in_netns(|| {
        let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else { return Round::NoResponse };
        let Some(udp) = ask(&sock, Proto::Udp) else { return Round::NoResponse };
        if !udp.ok() {
            return Round::Refused(udp.result);
        }
        match ask(&sock, Proto::Tcp) {
            Some(tcp) if tcp.ok() && tcp.external != udp.external => {
                tracing::warn!(udp = udp.external, tcp = tcp.external, "NAT-PMP mapped different UDP and TCP ports; using UDP's");
            }
            Some(tcp) if !tcp.ok() => tracing::warn!(result = tcp.result, "NAT-PMP TCP mapping refused"),
            None => tracing::warn!("no NAT-PMP answer for TCP"),
            _ => {}
        }
        Round::Port(udp.external)
    });
    result.unwrap_or_else(|e| {
        tracing::warn!("NAT-PMP: {e}");
        Round::NoResponse
    })
}
