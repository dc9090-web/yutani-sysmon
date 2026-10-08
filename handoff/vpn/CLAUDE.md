# CLAUDE.md: VPN applet for COSMIC (ProtonVPN · WireGuard)

You're building a COSMIC panel applet in Rust on libcosmic, plus a small root helper. The applet has two switches:

- **Torrents · qBittorrent:** a WireGuard tunnel inside a network namespace that only qBittorrent uses, with a built-in kill switch and Proton NAT-PMP port forwarding pushed into qBittorrent.
- **All web traffic:** a NetworkManager WireGuard connection.

This folder is the full design and spec handoff. Read these files in this order before writing code:

1. `docs/SPEC.md`: architecture, config parsing, namespace setup, NAT-PMP, qBittorrent launch, port push and leak detection, NetworkManager, UI behaviour, settings, states, security, D-Bus API. **This is the contract.**
2. `docs/DESIGN.md`: visual spec mapped to libcosmic.
3. `docs/ACCEPTANCE.md`: the checks to pass. The security and leak section is non-negotiable.
4. `design/prototype/index.html`: the interactive reference. Flip the switches; the "Simulate" bar triggers edge cases. `design/prototype/bundle.js` holds the reference state-to-text mapping (`vpnPanelHTML`, `vpnIcon`, `tunnelCardHTML`, `statusHTML`, `vpnIssue`, `vpnSettingsHTML`).
5. `design/screenshots/`: the target look, dark and light.
6. `tests/fixtures/`: sample confs, NAT-PMP packets, `wg` dumps and qBittorrent Web UI responses. Make them your first tests.

This applet is a sibling of the Network Traffic, System Monitor, AI Usage and Weather applets. Reuse their panel-button sizing, settings rows, rate formatting and justfile where they exist.

## Stack

- Rust, edition 2024, a **Cargo workspace** with three crates:
  - `applet` (binary `cosmic-applet-vpn`).
  - `helper` (binary `cosmic-vpn-helper`).
  - `common` (D-Bus types, conf parser, NAT-PMP codec, status enums).
- **applet:** `libcosmic` from git (features `applet`, `applet-token`, `tokio`, `wayland`; pin a commit), `cosmic-config`, `zbus` 5, `oo7` (keyring), `ashpd` (file chooser), `zeroize`, `i18n-embed`, `i18n-embed-fl`, `rust-embed`, `tracing`.
- **helper:** `tokio`, `zbus` 5 (system bus, with polkit via `zbus_polkit` or a direct `org.freedesktop.PolicyKit1` call), `rtnetlink`, `netlink-packet-route`, `wireguard-uapi` or `defguard_wireguard_rs`, `nix`, `reqwest` (rustls, cookies), `zeroize`, `tracing`.

## Commands (justfile)

- `just build`, `just check` (clippy `-D warnings` + fmt), `just test`.
- `just install` (needs root): installs
  - both binaries;
  - the desktop entry;
  - the 4 reticle icons to `share/icons/hicolor/scalable/apps/`;
  - `resources/system/*` to `/usr/share/dbus-1/system.d/`, `/usr/share/dbus-1/system-services/`, `/usr/lib/systemd/system/` and `/usr/share/polkit-1/actions/`;
  - then `systemctl daemon-reload`.
- `just uninstall`: removes all of the above, plus any `vpn-p2p` namespace and the `ip rule`s.
- `just run`: `RUST_LOG=cosmic_applet_vpn=debug cargo run -p applet`.
- `just demo`: `VPN_DEMO=1`. Uses a fake helper and fake NM driven by `tests/fixtures/`, cycling the states every 8 s (feature `demo`, off in release builds). This allows UI work without root or real configs.

## Rules

- **Never log, display, persist or signal a WireGuard private key or the Web UI password.** Zeroise buffers. Redact `PrivateKey`, `password=` and `SID=` in any debug output.
- **The kill switch is structural.** qBittorrent must only ever run inside `vpn-p2p`, whose only interfaces are `lo` and `wg-p2p`. Never add a veth, a bridge or a route to the host. Never start qBittorrent from the applet outside the namespace.
- **WireGuard placement:** create `wg-p2p` in the host namespace, then move it into the netns, so its UDP socket stays on the host. Then mark it with `fwmark 0xca6c` and add the host `ip rule` (SPEC §4.1, §6.3).
- **No shelling out in the helper** for networking: use netlink. `systemd-run` is replaced by `StartTransientUnit` over D-Bus. The applet may call `nmcli` only for the one-time import.
- **The helper trusts nothing from the applet:** it re-parses confs, uses the caller's uid from D-Bus credentials, allowlists the environment, and checks polkit for every method in SPEC §12.
- **Theme values come from `theme.cosmic()`.** No hard-coded colours. Status always has a word, never colour alone.
- **No I/O in `view`.** Helper calls and NM run in tasks or subscriptions. Status arrives through `StatusChanged` and the NM signals.
- **Testing:** unit-test the conf parser (all fixtures), the NAT-PMP request/response codec, the `wg` status mapping, `vpnIssue` / icon / chunk mapping, and the qBittorrent conf editor (both key layouts).
- `design/` is reference only. Don't ship it, and don't port the HTML or CSS.

## Milestones (stop and report after each)

1. **Skeleton + demo.** The workspace builds. The applet shows the reticle in the panel, and the popup opens and closes. Config loads with defaults. `just demo` cycles the states.
2. **Parser + settings.** Conf parser with fixture tests. The settings page with import via the portal (summary only; no helper yet). The flags. Keyring storage of the Web UI password.
3. **Web tunnel.** NM import (including the IPv6 block), activate and deactivate, the state subscription, rates; replace flow. Then run the ACCEPTANCE web items live.
4. **Helper: tunnel.** D-Bus service, polkit, `ImportP2p`, `P2pUp`/`P2pDown` with netlink, the fwmark rule, status signals, stale handling, adopting the namespace after a crash. Then run the kill-switch, exit-IP and both-on checks live.
5. **Helper: port + qBittorrent.** NAT-PMP loop, `LaunchQbit` via a transient unit (with DNS isolation), the conf-file port at launch, Web UI `SetQbitPort` and `Test`, `QuitQbit`. Then the applet's leak detection and "Restart in VPN". Live port and DNS checks.
6. **States and polish.** Every SPEC §10 state, restore at login, a11y, i18n, the full ACCEPTANCE pass.

## Open decisions (ask DC; don't guess)

- **The final App ID and crate names.** `io.github.dc.CosmicAppletVpn` and `io.github.dc.CosmicVpnHelper` are placeholders.
- **The licence.**
- **Neon icon style.** An optional "Panel icon style: Mono / Neon" setting (cyan glow, magenta for issues) was designed in the design system (`VpnCyberIcons`, `VpnIconLab`) but **isn't in scope** until DC confirms.
- **DC's real configs.** The format in SPEC §3.1 comes from Proton's published layout. Before milestone 2 is closed, check the parser against DC's two `.conf` files, **with `PrivateKey = REDACTED`**. Never ask for or accept a real private key.
- **The qBittorrent version** and whether it's native or Flatpak, to confirm the conf keys in SPEC §5.2.
