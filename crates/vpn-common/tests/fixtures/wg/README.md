# `wg show <if> dump` samples

These are a reference for the status mapping. The helper reads the same fields via netlink, never by running `wg`.

- **Line 1 (interface):** `private-key public-key listen-port fwmark`. The private key is shown as `REDACTED` here; real output contains it. **Never log it.**
- **Line 2+ (peer):** `public-key preshared-key endpoint allowed-ips latest-handshake(unix) rx-bytes tx-bytes keepalive`.

Expected mapping:
- `dump_connected.txt` with `now = 1791437812` → `state=on`, `handshake_age=12`.
- The same with `now = 1791438000` → `state=stale` (200 s > 180 s).
- `dump_no_handshake.txt` → `connecting` until 15 s after `P2pUp`, then `error` ("No handshake from NL#256 …").
