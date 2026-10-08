//! NAT-PMP (RFC 6886) port-mapping messages, as Proton documents them:
//! internal port 0, suggested external port 1, lifetime 60 (SPEC §4.3).

/// Proton's gateway inside the tunnel.
pub const GATEWAY: [u8; 4] = [10, 2, 0, 1];
pub const PORT: u16 = 5351;
pub const LIFETIME: u32 = 60;
/// Renew this often.
pub const RENEW_SECS: u64 = 45;
/// Wait for each try before giving up on a round.
pub const RETRIES_MS: [u64; 4] = [250, 500, 1000, 2000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Udp,
    Tcp,
}

impl Proto {
    fn opcode(self) -> u8 {
        match self {
            Self::Udp => 1,
            Self::Tcp => 2,
        }
    }
}

/// The 12-byte mapping request.
pub fn request(proto: Proto) -> [u8; 12] {
    let mut b = [0u8; 12];
    b[1] = proto.opcode();
    // internal port 0, suggested external 1
    b[6..8].copy_from_slice(&1u16.to_be_bytes());
    b[8..12].copy_from_slice(&LIFETIME.to_be_bytes());
    b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    pub proto: Proto,
    /// 0 is success; 2 refused, 5 unsupported opcode, …
    pub result: u16,
    pub epoch: u32,
    pub internal: u16,
    pub external: u16,
    pub lifetime: u32,
}

impl Mapping {
    pub fn ok(&self) -> bool {
        self.result == 0 && self.external != 0
    }
}

/// Decodes a 16-byte mapping response; `None` for anything else.
pub fn response(b: &[u8]) -> Option<Mapping> {
    if b.len() < 16 || b[0] != 0 {
        return None;
    }
    let proto = match b[1] {
        129 => Proto::Udp,
        130 => Proto::Tcp,
        _ => return None,
    };
    let u16_at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Some(Mapping { proto, result: u16_at(2), epoch: u32_at(4), internal: u16_at(8), external: u16_at(10), lifetime: u32_at(12) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/tests/fixtures/natpmp/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn requests_match_byte_for_byte() {
        assert_eq!(request(Proto::Udp).as_slice(), fixture("request_udp.bin"));
        assert_eq!(request(Proto::Tcp).as_slice(), fixture("request_tcp.bin"));
    }

    #[test]
    fn responses() {
        let u = response(&fixture("response_udp_ok_53186.bin")).unwrap();
        assert_eq!((u.proto, u.result, u.external, u.lifetime, u.ok()), (Proto::Udp, 0, 53186, 60, true));
        let t = response(&fixture("response_tcp_ok_53186.bin")).unwrap();
        assert_eq!((t.proto, t.external), (Proto::Tcp, 53186));
        let refused = response(&fixture("response_udp_refused.bin")).unwrap();
        assert_eq!((refused.result, refused.ok()), (2, false));
        assert_eq!(response(&fixture("response_udp_unsupported_opcode.bin")).unwrap().result, 5);
        assert_eq!(response(&[0u8; 8]), None);
        assert_eq!(response(&[1u8; 16]), None);
    }
}
