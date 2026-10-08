# NAT-PMP packets (RFC 6886), big-endian

**Requests** are 12 bytes: `version=0 | opcode (1 UDP, 2 TCP) | reserved u16 | internal port u16 = 0 | suggested external u16 = 1 | lifetime u32 = 60`. This mirrors `natpmpc -a 1 0 udp 60 -g 10.2.0.1` from Proton's guide.

**Responses** are 16 bytes: `version | opcode+128 | result u16 | epoch u32 | internal u16 | mapped external u16 | lifetime u32`.

| File | Meaning |
|---|---|
| `request_udp.bin` / `request_tcp.bin` | What the helper must send, byte for byte |
| `response_*_ok_53186.bin` | Success; port 53186, lease 60 s |
| `response_udp_refused.bin` | result 2 → `port_state = fail` ("not a P2P server" if it's the first round) |
| `response_udp_unsupported_opcode.bin` | result 5 → same |

Hex: `request_udp` = `00010000000000010000003c`, `response_udp_ok_53186` = `008100000001e2400000cfc20000003c`.
