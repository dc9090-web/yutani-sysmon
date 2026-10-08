# VPN applet (ProtonVPN · WireGuard): design and spec package

This is the handoff for building a COSMIC panel applet with two switches, each driven by its own Proton WireGuard `.conf`:

- **Torrents · qBittorrent:** qBittorrent runs inside a network namespace whose only way out is the WireGuard tunnel. That's the kill switch: if the tunnel drops, torrents stop, and nothing can fall back to your real connection. Proton's NAT-PMP port forwarding is renewed automatically and set as qBittorrent's listening port.
- **All web traffic:** a NetworkManager WireGuard connection for everything else.

Both can be on at once. The torrent tunnel bypasses the web tunnel and keeps its own exit IP and port.

## Using it with Claude Code

1. Copy this folder into a new repo.
2. Start Claude Code in it. It reads `CLAUDE.md` automatically.
3. Prompt: *"Read CLAUDE.md and the docs, then do milestone 1."* Continue one milestone at a time. Milestones 4–5 need `sudo` on your machine for live checks.

## Before you build: Proton setup

1. Go to account.protonvpn.com → **Downloads → WireGuard configuration**.
2. **Torrent config:**
   - Platform: Linux.
   - Server: a **P2P** server (the ⇄ icon).
   - Turn **NAT-PMP (Port Forwarding)** on and **Moderate NAT** off.
   - NetShield is up to you.
   - Download it.
3. **Web config:** any server. Download it.
4. Keep both files private: each contains a private key. The applet stores them root-only (torrent) and in NetworkManager (web).
5. **In qBittorrent:**
   - Tools → Options → Web UI → enable it on port 8080, and either set a username/password or tick "Bypass authentication for clients on localhost".
   - Turn UPnP/NAT-PMP off under Connection.
   - The applet sets the listening port itself.

## Contents

| Path | What it is |
|---|---|
| `CLAUDE.md` | Stack, rules, milestones, open decisions |
| `docs/SPEC.md` | Architecture, namespace and kill switch, NAT-PMP, qBittorrent launch, port push and leak detection, NetworkManager, UI behaviour, settings, states, security, D-Bus API |
| `docs/DESIGN.md` | Colours mapped to libcosmic, type, panel, popup and settings layout, icons, a11y |
| `docs/ACCEPTANCE.md` | Sign-off checklist, including leak tests |
| `design/prototype/` | Interactive HTML reference. Open `index.html`; the top bar switches pages and theme. `icon-lab.html` is the icon exploration. |
| `design/screenshots/` | Target renders, dark and light, at 2× |
| `resources/icons/` | The 4 custom reticle icons (off / p2p / web / both) |
| `resources/system/` | D-Bus policy and activation file, systemd unit and polkit actions for the helper |
| `resources/*.desktop` | Applet desktop entry |
| `i18n/en/*.ftl` | Every user-visible string |
| `tests/fixtures/` | Sample confs (keys are dummies), NAT-PMP packets, `wg` dumps, qBittorrent Web UI responses |

## Limits

- **Only the qBittorrent started by the applet is protected.** If you start it any other way, the applet detects it and warns you within 10 s, but it can't stop that traffic.
- Other torrent clients aren't detected.
- **Port forwarding needs** a Proton plan with P2P servers, and a config with NAT-PMP on.

## Where the values come from

- The design values come from libcosmic `main` (commit 60ad2cc, 2026-10-06), the same as the sibling applets.
- The Proton facts come from Proton's published Linux port-forwarding guide: NAT-PMP via gateway 10.2.0.1, 60 s lease, 45 s renew, the same port for UDP and TCP.
- **The config layout in SPEC §3.1 must be verified** against your actual downloaded files (with the key redacted).
