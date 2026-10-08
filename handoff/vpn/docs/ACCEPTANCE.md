# VPN applet: acceptance checklist

Run on DC's machine (COSMIC on Pop!_OS) with both real Proton configs. Tick every box.

## Security and leaks (must all pass)

- [ ] **Exit IP.** With torrents on, `sudo ip netns exec vpn-p2p curl -s https://ifconfig.me` returns the Proton exit IP, and the host's `curl` returns the real IP (web off).
- [ ] **Kill switch.**
  - With torrents on, drop the tunnel: `sudo ip netns exec vpn-p2p wg set wg-p2p peer <pub> remove`.
  - qBittorrent's traffic drops to 0, and `ip netns exec vpn-p2p curl` fails.
  - Nothing appears on the physical NIC: `sudo tcpdump -i <nic> host not <endpoint-ip>` while qBittorrent tries to connect.
- [ ] **DNS.**
  - Inside the qBittorrent unit, a lookup goes to 10.2.0.1: `sudo nsenter --net=/run/netns/vpn-p2p tcpdump -i wg-p2p port 53` shows it.
  - `resolvectl query` on the host shows nothing for tracker names.
- [ ] **IPv6.** Inside the namespace, `curl -6 https://ipv6.icanhazip.com` fails, or returns a Proton v6 address if the conf has one.
- [ ] **Web tunnel IPv6.** With the web tunnel on, the host has no IPv6 egress outside the tunnel (SPEC §6.1 step 5).
- [ ] **Both on.**
  - The host's IP is the web exit, and the namespace's IP is the P2P exit.
  - `ip route get <p2p-endpoint> mark 0xca6c` uses the physical interface (no double tunnel).
  - The forwarded port still renews.
- [ ] **Leak detection.**
  - Start `qbittorrent` normally from the launcher. Within 10 s, the red dot and the banner appear.
  - "Restart in VPN" moves it into the namespace, and the banner clears.
- [ ] **No secrets.** `grep -r PrivateKey ~/.config/cosmic ~/.cache` finds nothing. Logs at `RUST_LOG=trace` contain no key, no Web UI password and no `SID`.
- [ ] **Configs.** `/etc/cosmic-vpn/p2p.conf` is 0600 root:root. A conf with `PostUp` is rejected.
- [ ] **polkit.** Import asks for admin authentication once (kept); up/down doesn't prompt for an active local session.

## Port forwarding

- [ ] With a NAT-PMP P2P conf, a port appears within 5 s of "Connected", and "↻ Ns ago" resets every ~45 s.
- [ ] **External check:** `nc -l` inside the namespace on the port, then from outside (phone hotspot) the port is reachable. Alternatively, qBittorrent's connection status turns green.
- [ ] **Applied to qBittorrent:**
  - Launched from the applet, qBittorrent's Options → Connection shows the forwarded port, with UPnP/NAT-PMP off.
  - Web UI on: a reconnect that changes the port updates qBittorrent with no restart.
  - Web UI off: the card shows "Set manually · WebUI off".
- [ ] A non-P2P conf → "Unavailable · not a P2P server", amber dot, no PORT chunk.

## Function

- [ ] **Torrents on:** Connecting → Connected → port → qBittorrent launches (setting on). **Off:** qBittorrent quits gracefully (no "unclean shutdown" recheck on the next start), then the tunnel is removed (`ip netns list` is empty).
- [ ] **Web on/off** follows NetworkManager, including toggles made in COSMIC Settings while the popup is open.
- [ ] **Restore at login:** log out with both on, then log in → both come back. Turn the web tunnel off by an error → `last_web` isn't cleared.
- [ ] **Helper crash:** `sudo systemctl kill cosmic-vpn-helper` with torrents on → the kill switch holds, the next applet call re-activates the helper, and it adopts the existing namespace.
- [ ] **Suspend/resume** with both on → handshakes resume within 30 s and the port renews (it may change; qBittorrent is updated).
- [ ] **Replace configs:** disabled while on; works when off; the NM connection is replaced, not duplicated.
- [ ] **Flatpak qBittorrent** (if installed) is detected and launched in the namespace.

## Visual (compare with `design/screenshots/`, dark and light)

- [ ] **Panel:** reticle state per tunnel; chunks never shift width; issue dots; vertical layout; S–XL sizes.
- [ ] **Popup:** off, connecting, connected (both), leak, stale, no port, Web UI off, error, no configs, helper missing.
- [ ] **Settings:** both configs, first run, no-NAT-PMP variant; field focus ring; status lines.
- [ ] **Theme:** the icon tints with the theme. No hard-coded colours (switch the accent and the theme live).
- [ ] **Readability:** at 125% text scale the popup still fits 360 wide with no clipped text (ellipsis on server lines only).

## Quality

- [ ] `just check` is clean (clippy `-D warnings`, fmt). `just test` passes, including the parser, NAT-PMP codec and status-mapping tests against `tests/fixtures/`.
- [ ] Every user string comes from `i18n/en/cosmic_applet_vpn.ftl`.
- [ ] No blocking I/O in `view`. With the popup open, the applet's CPU is < 1% at idle (1 s status tick).
