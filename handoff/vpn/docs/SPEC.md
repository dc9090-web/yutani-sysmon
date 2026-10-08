# VPN applet (ProtonVPN · WireGuard): specification

This is the functional contract. Visual details are in `DESIGN.md`; the checks are in `ACCEPTANCE.md`. Wherever this file and the prototype disagree, this file wins. Items marked **VERIFY** must be confirmed on DC's machine before they're relied on.

## 1. What it does

The applet has two independent switches, each backed by its own Proton WireGuard `.conf`.

| Switch | Config | Mechanism | Purpose |
|---|---|---|---|
| **Torrents · qBittorrent** | P2P server, *NAT-PMP (Port Forwarding)* on | Network namespace `vpn-p2p` built by a root helper | Only qBittorrent uses it. Kill switch by construction. Forwarded port pushed to qBittorrent. |
| **All web traffic** | Any Proton server | NetworkManager WireGuard connection | Full-tunnel for the rest of the system |

- The switches are independent. Both can be on at once: the torrent tunnel keeps its own exit, port and kill switch.
- Nothing else on the system routes through the torrent tunnel.
- qBittorrent never touches the web tunnel or the physical network.

## 2. Components

```
cosmic-applet-vpn      (user, panel applet, libcosmic)
  ├─ D-Bus (system bus) ──► cosmic-vpn-helper  (root, D-Bus activated, polkit-guarded)
  │                           ├─ netns vpn-p2p + wg-p2p (rtnetlink / wireguard netlink)
  │                           ├─ NAT-PMP client inside the netns (native, RFC 6886)
  │                           ├─ qBittorrent launcher (systemd transient unit in the netns, as the user)
  │                           └─ qBittorrent Web UI client inside the netns (port push)
  ├─ D-Bus (system bus) ──► NetworkManager     (web tunnel: activate / deactivate / state)
  └─ nmcli (subprocess)  ──► NetworkManager     (one-time import of the web .conf only)
```

- **Cargo workspace:** crates `applet`, `helper` and `common` (shared D-Bus types, conf parser, NAT-PMP codec).
- **Binaries:** `cosmic-applet-vpn` and `cosmic-vpn-helper`.
- **Dependencies:**
  - Shared: `zbus` 5 (async, tokio), `serde`, `thiserror`, `tracing`.
  - Helper only: `rtnetlink`, `netlink-packet-route`, `wireguard-uapi` (or `defguard_wireguard_rs`), `nix` (setns, uid), `tokio`, `reqwest` (rustls, cookies).
  - Applet only: `libcosmic`, `cosmic-config`, `oo7`.
- **Never shell out from the helper for the network setup.** The `ip` / `wg` commands in §4 are the *reference* for what the netlink calls must do.

## 3. Proton config handling

### 3.1 Expected format (VERIFY against DC's files)

```ini
[Interface]
# Key for <device name>
# Bouncing = 6
# NetShield = 1
# Moderate NAT = off
# NAT-PMP (Port Forwarding) = on
# VPN Accelerator = on
PrivateKey = <base64>
Address = 10.2.0.2/32            # may also list an IPv6 /128
DNS = 10.2.0.1                   # may also list an IPv6 resolver

[Peer]
# NL#256
PublicKey = <base64>
AllowedIPs = 0.0.0.0/0           # may also list ::/0
Endpoint = 185.107.56.23:51820
```

### 3.2 Parser (`common::conf`)

- **Model:** INI with `[Interface]` and exactly one `[Peer]`. Keys are case-insensitive. `#` comments are kept for the hints below.
- **Required:**
  - `PrivateKey`: 44-char base64 → 32 bytes.
  - `Address`: ≥ 1 IPv4 CIDR.
  - Peer `PublicKey`.
  - `Endpoint`: an IP (v4 or [v6]) and a port. Hostnames are allowed but resolved once, at import, by the helper.
  - `AllowedIPs` must contain `0.0.0.0/0`.
- **Optional:** `DNS`, `MTU`, `PresharedKey`, `PersistentKeepalive`. Ignore `PostUp` / `PreDown`; **refuse configs containing `PostUp`/`PostDown`/`PreUp`/`PreDown`** (they're never Proton's, and could run commands).
- **Hints from comments:**
  - `server_label`: the first `# …` comment line in `[Peer]` (e.g. `NL#256`). Fallback: the file name without `.conf`.
  - `natpmp_hint`: `on` / `off` / `unknown`, from `# NAT-PMP (Port Forwarding) = on|off`.
  - `moderate_nat_hint`: from `# Moderate NAT = on|off`. Port forwarding needs it **off**: warn if it's on.
- **City:** Proton doesn't put it in the conf. Show only the label (`NL#256`). Adding a city later is optional, via a small built-in country-code → country name table (`NL` → Netherlands). **No network lookups.**
- **`parse_structure(text)`:** a second entry point that accepts `PrivateKey = REDACTED`. Use it in a `--check-conf <file>` CLI flag so DC can verify his real configs without sharing the key (`tests/fixtures/conf/redacted_key.conf`).
- **The private key** is parsed to validate only. The *applet* never stores or logs it. It goes to the helper once (§3.3) or to NetworkManager (§6.1), and is then zeroised (`zeroize`).

### 3.3 Import flows

**Torrent conf** (Settings → Import .conf):
1. The XDG portal file chooser (`ashpd`, filter `*.conf`) opens.
2. The applet reads the file and parses it. On error it shows the exact reason inline (§10).
3. The applet calls `helper.ImportP2p(conf_text)`. polkit: `io.github.dc.cosmicvpn.import` (auth_admin_keep).
4. The helper re-parses the text (it never trusts the applet), writes `/etc/cosmic-vpn/p2p.conf` (0600 root:root, atomic rename), and returns a `ConfSummary` with no key material.
5. The applet stores `ConfSummary` in cosmic-config (§9).

**Web conf**: see §6.1.

## 4. Torrent tunnel (helper)

### 4.1 Bring-up: `P2pUp()`

The reference commands, which must be implemented with netlink:

```sh
ip netns add vpn-p2p
ip -n vpn-p2p link set lo up
ip link add wg-p2p type wireguard                 # created in the HOST netns → its UDP socket lives in the host
wg set wg-p2p private-key <k> fwmark 0xca6c peer <pub> endpoint <ep> allowed-ips 0.0.0.0/0[,::/0] persistent-keepalive 25
ip link set wg-p2p netns vpn-p2p                  # move only the interface
ip -n vpn-p2p addr add 10.2.0.2/32 dev wg-p2p     # + v6 address if present
ip -n vpn-p2p link set wg-p2p mtu ${MTU:-1420} up
ip -n vpn-p2p route add default dev wg-p2p        # + ip -6 route add default dev wg-p2p if v6
mkdir -p /etc/netns/vpn-p2p && echo "nameserver 10.2.0.1" > /etc/netns/vpn-p2p/resolv.conf
ip rule add fwmark 0xca6c lookup main priority 100   # host: encrypted P2P packets bypass any other VPN (§6.3)
ip -6 rule add fwmark 0xca6c lookup main priority 100
```

- **Kill switch:** the namespace's only interfaces are `lo` and `wg-p2p`. If the tunnel stalls, packets go nowhere. If the helper dies, the namespace persists in the same safe state. Nothing in the namespace can ever reach the physical NIC.
- **IPv6:** if the conf has no IPv6 address, still add `::/0` to allowed-ips and an IPv6 default route via `wg-p2p`, using `fd00:ca6c::2/128` as the address. v6 traffic then dies inside the tunnel instead of being unreachable-with-fallback. (VERIFY: Proton drops it silently.)
- **Persistent keepalive 25 s:** this keeps the NAT-PMP mapping and the handshake fresh while idle.
- **State machine:** `Off → Connecting → On`.
  - It becomes `On` once the first handshake has happened (latest-handshake > 0), polled every 500 ms for up to 15 s.
  - With no handshake by 15 s: `Error("No handshake from <label> · check the server or your connection")`. The namespace is kept, so the kill switch holds, and the helper retries every 10 s while the switch is on.
- **`Stale`:** latest-handshake older than 180 s while `On`. When it's stale:
  1. Re-resolve the endpoint if it was a hostname.
  2. Run `wg set … endpoint` again, to force a re-handshake.
  3. Return to `On` on the next handshake.
- **Idempotency:** if `vpn-p2p` already exists at helper start (after a crash or restart), adopt it. Don't recreate it.

### 4.2 Tear-down: `P2pDown(quit_qbit: bool)`

1. If `quit_qbit`, stop qBittorrent (§5.3) and wait for it to exit, up to 10 s.
2. Stop the NAT-PMP loop.
3. `ip netns del vpn-p2p`. This destroys `wg-p2p` and everything in the namespace.
4. Remove `/etc/netns/vpn-p2p` and the `ip rule`s.

If qBittorrent is still running (`quit_qbit=false`), **refuse with `Error::QbitRunning`** unless `force=true`. A process left in a deleted namespace keeps a netns with no interfaces, which is safe but confusing. The applet asks: "qBittorrent is still running. Quit it and turn off?" with Quit and Cancel.

### 4.3 Port forwarding (NAT-PMP, native)

- **Gateway:** `10.2.0.1`, UDP 5351, socket created **inside** `vpn-p2p` (a thread does `setns(CLONE_NEWNET)` then creates the socket).
- **Requests**, per RFC 6886: version 0. Send UDP (opcode 1) then TCP (opcode 2), each with internal port 0, suggested external port 1, lifetime 60. This mirrors Proton's documented `natpmpc -a 1 0 udp 60 -g 10.2.0.1` and `… tcp …`. Retry 250 ms, 500 ms, 1 s, 2 s, then fail that round.
- **Response:** result code 0 means success. Take the mapped external port from bytes 10–11. UDP and TCP must return the **same** port; if they don't, use the UDP one and log a warning.
- **Renew** every **45 s**. On each success, set `renewed_at = now`.
- **Port change** (Proton may reassign after a reconnect): emit `PortChanged(new)`, then push it to qBittorrent (§5.4).
- **Failure:**
  - 3 consecutive failed rounds while the tunnel is `On` → `port.state = Fail`.
  - The first-ever round fails with a non-zero result code (e.g. 2 refused, 5 unsupported opcode) or no response → `Fail("not a P2P server")`. This means the server or config lacks port forwarding.
  - In `Fail`, retry every 60 s (cheap) in case the hint was wrong.
- The first successful round also sets `natpmp_capable = true` in the `ConfSummary`, which the applet persists. This replaces the "NAT-PMP ?" flag (§8).

### 4.4 Status

The helper emits `StatusChanged(a{sv})` on every change, plus at most once a second while rates change. Fields:

| Key | Type | Notes |
|---|---|---|
| `state` | s | `off` `connecting` `on` `stale` `error` |
| `error` | s | One line, user-facing, only when `state=error` |
| `handshake_age` | u | Seconds since the latest handshake; 0 if none |
| `rx_bps` `tx_bps` | t | Bytes/s, computed from wg transfer counters over a 2 s window |
| `port_state` | s | `off` `req` `ok` `fail` |
| `port` | q | 0 if none |
| `port_renewed_age` | u | Seconds |
| `qbit` | s | `stopped` `starting` `vpn` (running in the namespace). `outside` is set by the applet, not the helper (§5.5). |
| `listen_port_applied` | s | `none` `auto` `conf` `manual` `failed` |

`GetStatus()` returns the same dictionary.

## 5. qBittorrent

### 5.1 Detection

The applet resolves `qbit_command` on startup and on settings open:
1. A user override, if set.
2. `qbittorrent` on `PATH`.
3. Flatpak `org.qbittorrent.qBittorrent`, run as `flatpak run org.qbittorrent.qBittorrent`.
4. Otherwise: "qBittorrent not found" (§10).

The config path depends on the install:
- Native: `~/.config/qBittorrent/qBittorrent.conf`.
- Flatpak: `~/.var/app/org.qbittorrent.qBittorrent/config/qBittorrent/qBittorrent.conf`.

### 5.2 Launch inside the namespace: `LaunchQbit(argv, env, port)`

- The applet calls the helper (polkit `io.github.dc.cosmicvpn.control`, `allow_active=yes`).
- The helper runs a **transient system unit** as the calling user. The equivalent command:

```sh
systemd-run --unit=cosmic-vpn-qbittorrent --uid=<uid> --gid=<gid> --collect \
  -p NetworkNamespacePath=/run/netns/vpn-p2p \
  -p BindReadOnlyPaths=/etc/netns/vpn-p2p/resolv.conf:/etc/resolv.conf \
  -p InaccessiblePaths=-/run/systemd/resolve -p InaccessiblePaths=-/run/nscd -p InaccessiblePaths=-/run/avahi-daemon \
  -E WAYLAND_DISPLAY=… -E XDG_RUNTIME_DIR=… -E DBUS_SESSION_BUS_ADDRESS=… -E DISPLAY=… -E XAUTHORITY=… \
  -E HOME=… -E LANG=… -E XDG_CURRENT_DESKTOP=… -E XDG_SESSION_TYPE=… -E PATH=… \
  <argv…>
```

Implement this through systemd's D-Bus `StartTransientUnit`, not the `systemd-run` binary.

- **`InaccessiblePaths` stops DNS leaks.** With `nss-resolve`, nscd or mDNS, lookups would otherwise go through host daemons that sit outside the tunnel. Hiding their sockets forces glibc to fall back to `/etc/resolv.conf`, which is bind-mounted to Proton's DNS. (VERIFY on Pop!_OS: `getent hosts example.com` inside the unit queries 10.2.0.1.)
- **Environment:** the helper accepts only these keys: `WAYLAND_DISPLAY`, `DISPLAY`, `XAUTHORITY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `HOME`, `LANG`, `LC_*`, `XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`, `QT_QPA_PLATFORM`, `PATH`.
- **The `uid`** is taken from the D-Bus caller's credentials (`GetConnectionUnixUser`), never from the arguments.
- **Wayland and the session bus are path-based unix sockets,** so they work across network namespaces. X11 falls back from the abstract socket to `/tmp/.X11-unix` automatically.
- **Listening port at launch:** if `port` is non-zero, the helper first edits the user's `qBittorrent.conf`. This happens only when qBittorrent isn't running. The edit is atomic, preserves the file's owner and mode, and only touches these keys:
  - qBittorrent ≥ 4.4: `[BitTorrent] Session\Port=<port>`.
  - `[Network] PortForwardingEnabled=false`, which is UPnP/NAT-PMP inside qBittorrent; irrelevant in the namespace, but kept quiet.
  - Older versions: `[Preferences] Connection\PortRangeMin=<port>` and `Connection\UPnP=false`.
  - **VERIFY these key names against DC's installed version.** If neither layout is recognised, skip the edit and rely on §5.4.
  - Result: `listen_port_applied = conf`.
- **qBittorrent's "Network interface" setting:** leave it at "Any interface". The namespace only has `wg-p2p`. Binding to `wg-p2p` explicitly is fine but unnecessary.
- **States:** `qbit = starting` until the unit is `active` *and* the process has existed for 2 s, then `vpn`. If the unit fails, `qbit = stopped` with the error toast "qBittorrent failed to start (exit N)".

### 5.3 Quit: `QuitQbit()`

1. Gracefully, if the Web UI is reachable: `POST /api/v2/app/shutdown`.
2. Otherwise `systemctl stop cosmic-vpn-qbittorrent` (SIGTERM, then SIGKILL after 10 s).

### 5.4 Live port push: `SetQbitPort(port, user, pass)`

- **Runs inside the namespace:** qBittorrent's Web UI listens on 127.0.0.1 *there*, so the helper makes the call from a thread that has done `setns`.
- **Calls:**
  1. `POST http://127.0.0.1:<webui_port>/api/v2/auth/login` (form: `username`, `password`) → `SID` cookie. Skip this if `user` is empty, for the "Bypass authentication for clients on localhost" setup.
  2. `POST /api/v2/app/setPreferences` with form `json={"listen_port":<port>,"upnp":false,"random_port":false}`.
  3. `GET /api/v2/app/preferences` to confirm `listen_port == port`.
  4. On success, `listen_port_applied = auto`.
- **Credentials:** the applet passes them per call, read from the keyring. The helper never stores them, and they're never logged.
- **Errors:**
  - 403 on login means the wrong password → the settings status line says "Web UI login failed".
  - Connection refused means the Web UI is off → `listen_port_applied = manual` → the card shows "Set manually · WebUI off".
- **Triggers:**
  - After `qbit` becomes `vpn`, if `port_state=ok` and the port differs from what was written to the conf.
  - On `PortChanged`.
  - Only if the `auto_port` setting is on.
- **`TestQbitWebUi(user, pass)`** runs the same login and preferences GET, returning `ok` / `unreachable` / `auth_failed` / `not_running`.

### 5.5 Leak detection (applet side, no root)

- **Interval:** every 3 s while the popup is open, otherwise every 10 s.
- **Detection:**
  1. Scan `/proc/[0-9]*/comm` for `qbittorrent` (Flatpak shows the same comm) where the uid is the user's.
  2. Compare `stat(/proc/<pid>/ns/net)` (dev, ino) with `stat(/run/netns/vpn-p2p)`.
  3. Any match that **isn't** in the namespace → `qbit = outside`. This applies even if the torrent switch is off.
- **When outside:** show the leak banner and the red dot.
- **"Restart in VPN":**
  1. Ask it to quit gracefully: send `SIGTERM` to the PIDs (they're the user's own).
  2. Wait up to 10 s for exit.
  3. Turn the torrent tunnel on if it's off.
  4. Run `LaunchQbit`.
- **A torrent client the applet doesn't manage** (another client, or a second qBittorrent profile) isn't detected. Say so in the README.

## 6. Web tunnel (NetworkManager)

### 6.1 Import

1. Run the file chooser and parse the conf with the same parser as §3.2.
2. Copy the text to `$XDG_RUNTIME_DIR/cosmic-vpn/wg-web.conf` (0600). The file stem becomes NM's interface name, so it must be `wg-web` (≤ 15 chars).
3. `nmcli --terse connection import type wireguard file …/wg-web.conf` → parse the UUID from stdout.
4. `nmcli connection modify <uuid> connection.id "Proton <label>" connection.autoconnect no`.
5. **IPv6 leak block:** if the conf has no IPv6 Address or no `::/0`, add `ipv6.method manual ipv6.addresses fd00:ca6c::3/128 +wireguard.peer…allowed-ips ::/0`. Use `nmcli connection modify <uuid> wireguard.peer-routes yes` and set the peer's allowed-ips to `0.0.0.0/0,::/0` via D-Bus `Update2` if nmcli can't. (VERIFY: with this, `curl -6 https://ipv6.icanhazip.com` fails while the tunnel is on.)
6. Delete the temp file, and zeroise the buffer that held the key.
7. Store `web_nm_uuid` and the `ConfSummary` in config.

**Replace:** delete the old NM connection (`Settings.Connection.Delete`), then import the new one. This is only allowed while the web tunnel is off.

### 6.2 Toggle and state (D-Bus)

- **On:** `org.freedesktop.NetworkManager.ActivateConnection(conn_path, "/", "/")`.
- **Off:** `DeactivateConnection(active_path)`.
- **State:** subscribe to `ActiveConnection.StateChanged`. Map NM's states as follows:

| NM state | Applet state |
|---|---|
| 1 (activating) | `connecting` |
| 2 (activated) | `on` |
| 3–4 (deactivating / deactivated) | `off` |
| Activation failure | `error` with the NM reason string, e.g. "NetworkManager: activation failed (no secrets)" |

- **Rates:** `/sys/class/net/wg-web/statistics/{rx,tx}_bytes` over a 2 s window.
- **Handshake age:** read through the helper (`GetWebHandshake()`), because unprivileged wireguard netlink reads are denied. Without the helper, hide the "· Ns ago" part and the stale detection.
- **Changes from outside:** if the user toggles the connection in COSMIC's network settings or with nmcli, the applet follows NM's state. NM is the source of truth.

### 6.3 Coexistence

- **Torrent packets bypass the web tunnel.** The wg-p2p encrypted UDP packets carry `fwmark 0xca6c`. The helper's `ip rule … fwmark 0xca6c lookup main priority 100` sits ahead of NM's WireGuard policy rules, so torrent traffic exits via the physical default route and never double-tunnels. (VERIFY: with both on, `ss -uapn` and `ip route get <p2p endpoint> mark 0xca6c` show the physical interface.)
- **DNS stays separate.** The web tunnel's DNS (NM → systemd-resolved) never affects the namespace, which uses its own `resolv.conf`.

## 7. Applet UI behaviour

### 7.1 Panel button

The visual spec is in DESIGN §3.

- **Icon:**
  - `vpn-reticle-off` / `-p2p` / `-web` / `-both`, chosen by which tunnels are `on` or `stale`. `connecting` and `error` count as not up.
  - **Issue dot:** red if `qbit=outside` or either tunnel is `error`; amber if either tunnel is `stale`, or P2P is on and `port_state=fail`.
- **Chunks:**
  - `P2P` and `WEB`, each showing `ON` / `OFF` / `…` / `ERR` (stale still shows `ON`, in amber).
  - `PORT <n>` when `show_port` is on, P2P is `on` and `port_state=ok`.
- **Clicks:** left-click toggles the popup. No other click actions.
- **Tooltip and accessible name:** "VPN · Torrents on, port 53186 · Web off" (plus " · qBittorrent outside VPN" etc. when there's an issue).

### 7.2 Popup

The layout is in DESIGN §4; the behaviour is in the interactive prototype `design/prototype/index.html`.

- **Toggling a switch:**
  - Optimistic: show `connecting` immediately.
  - Disable that switch until the helper or NM reports a terminal state (`on`, `off` or `error`), or for 20 s.
- **Torrents on:**
  1. `P2pUp`.
  2. On `on` + `port ok`, if `launch_qbit` is on and `qbit` is `stopped`: `LaunchQbit(argv, env, port)`.
  3. If the port isn't known within 5 s, launch anyway with port 0 and push the port later (§5.4).
- **Torrents off:** `P2pDown(quit_qbit=setting)`.
- **Copy port:** writes to the clipboard and shows the toast "Port 53186 copied" for 1.4 s.
- **"Launch in VPN"** (shown when P2P is on and qBittorrent isn't running): `LaunchQbit`.

### 7.3 Restore at login

- On applet start, if `restore` is on, re-apply `last_p2p` / `last_web`.
- **The web tunnel is not auto-activated if NM already has another VPN active.** Show the warning instead.
- **On every switch change**, persist `last_*`. A tunnel that's switched off by an error does **not** change `last_*`.

## 8. Settings page

| Row | Control | Default | Storage |
|---|---|---|---|
| Torrents config | File, label, endpoint; flags `NAT-PMP ✓` / `NAT-PMP ?` / `No NAT-PMP`, `Moderate NAT on` (warning), `P2P`, `Kill switch · namespace`; Replace (disabled while on) | none | `p2p_conf: ConfSummary` |
| Web config | File, label, endpoint; flags `In NetworkManager ✓` + connection name; Replace (disabled while on) | none | `web_conf: ConfSummary`, `web_nm_uuid` |
| Launch with torrent tunnel | toggler | on | `launch_qbit` |
| Quit when tunnel turns off | toggler | on | `quit_qbit` |
| Set listening port automatically | toggler | on | `auto_port` |
| ↳ Web UI port | numeric field 1–65535 | 8080 | `webui_port` |
| ↳ Username | text, empty = localhost bypass | `admin` | `webui_user` |
| ↳ Password | password, keyring `cosmic-applet-vpn/qbit-webui` | — | oo7 |
| ↳ Status | "✓ Web UI reachable · password in the system keyring" / "Web UI not reachable · enable it in qBittorrent → Options → Web UI" / "Web UI login failed" / "Start qBittorrent to test" | — | — |
| Show forwarded port | toggler | on | `show_port` |
| Restore tunnels at login | toggler | on | `restore` |
| qBittorrent command | text, placeholder shows the detected command | auto | `qbit_command: Option<String>` |

- **Status tests** (`TestQbitWebUi`) run on page open and 600 ms after any field edit (debounced). They run only if `qbit=vpn`.
- **The "qBittorrent command" row** isn't in the mockups. Put it at the bottom of the qBittorrent section in the same `ca-field` style.

## 9. Config (cosmic-config, `io.github.dc.CosmicAppletVpn`, version 1)

```rust
pub struct ConfSummary { file_name: String, server_label: String, endpoint: String, addr_v4: String,
    has_v6: bool, dns: Vec<String>, natpmp_hint: Hint, moderate_nat_hint: Hint, natpmp_capable: Option<bool> }
pub struct Config {
    p2p_conf: Option<ConfSummary>, web_conf: Option<ConfSummary>, web_nm_uuid: Option<String>,
    launch_qbit: bool /*true*/, quit_qbit: bool /*true*/, auto_port: bool /*true*/,
    webui_port: u16 /*8080*/, webui_user: String /*"admin"*/, show_port: bool /*true*/,
    restore: bool /*true*/, last_p2p: bool, last_web: bool, qbit_command: Option<String>,
}
```

No secrets go in config.

## 10. States and errors

| Situation | Where | Text / behaviour |
|---|---|---|
| No configs imported | Popup | Banner: "Import your Proton WireGuard configs" + steps + `Import configs` (opens settings). Both switches hidden. |
| One config missing | Card | That card's switch is disabled, with the caption "No config · Import in settings". |
| Helper not installed / D-Bus name not activatable | Popup | Banner "System helper missing" (copy as in the prototype). The torrent card is hidden; the web card still works. |
| polkit denied | Toast | "Permission denied". The switch reverts. |
| Conf invalid | Settings inline (`ca-keystat err`) | "Not a WireGuard config", "Missing PrivateKey", "AllowedIPs must include 0.0.0.0/0", "Scripts (PostUp…) are not allowed" |
| P2P handshake timeout | Card | `error`: "No handshake from NL#256 · check the server or your connection"; retries every 10 s |
| Stale | Card + panel | Amber "No handshake for 3m · reconnecting…", box dimmed |
| Port forwarding unavailable | Card + panel | "Unavailable · not a P2P server", amber dot |
| Moderate NAT on | Settings flag | Amber "Moderate NAT on · port forwarding won't work" |
| Web UI off | Card | "Set manually · WebUI off" + the port stays copyable |
| qBittorrent not found | Card | qBittorrent row: "Not installed"; launch hidden |
| qBittorrent outside VPN | Banner + card + red dot | As in the prototype; "Restart in VPN" |
| Turn off while qBittorrent runs and `quit_qbit=false` | Dialog | "qBittorrent is still running. Quit it and turn off?" Quit / Cancel |
| NM activation failed | Web card | `error` with NM's reason |
| Another VPN active in NM at login | Web card | Not auto-restored; caption "Another VPN is active" |

## 11. Security rules

- **Private keys:**
  - Never logged, displayed, put in config, or put in a D-Bus *signal*.
  - They cross D-Bus once, at import, in a method call to the root helper.
  - Zeroise buffers afterwards.
- **The helper:**
  - **Inputs:** validates every input (conf re-parse, port ranges, the env allowlist, argv[0] must be an absolute path the caller owns or a path under `/usr`, `/var/lib/flatpak` or `~/.local/share/flatpak`).
  - **Users:** it serves only the active session's users (polkit `allow_active`).
  - **Runtime:** it runs with `ProtectSystem=strict`, `ReadWritePaths=/etc/cosmic-vpn /etc/netns /run/netns`, `CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_SETUID CAP_SETGID CAP_DAC_OVERRIDE CAP_CHOWN CAP_FOWNER` (see `resources/system/`).
  - **qBittorrent.conf edit:** done after dropping to the user's uid (`setresuid` in a child), never as root.
- **polkit actions:**
  - `io.github.dc.cosmicvpn.control` (up/down/launch/quit/port/test/status): `allow_active=yes`.
  - `io.github.dc.cosmicvpn.import`: `auth_admin_keep`.

## 12. D-Bus API (helper)

Bus name `io.github.dc.CosmicVpnHelper`, object `/io/github/dc/CosmicVpnHelper`, interface `io.github.dc.CosmicVpnHelper1`.

| Method / signal | Signature | polkit |
|---|---|---|
| `ImportP2p(conf: s) → summary: a{sv}` | | import |
| `P2pUp()` / `P2pDown(quit_qbit: b, force: b)` | | control |
| `LaunchQbit(argv: as, env: a{ss}, port: q)` / `QuitQbit()` | | control |
| `SetQbitPort(port: q, webui_port: q, user: s, pass: s) → result: s` | | control |
| `TestQbitWebUi(webui_port: q, user: s, pass: s) → result: s` | | control |
| `GetStatus() → a{sv}` / `GetWebHandshake() → u` | | none (read-only) |
| signal `StatusChanged(a{sv})`, `PortChanged(q)` | | — |
| property `Version: u` (= 1) | | — |

Errors use D-Bus error names `io.github.dc.CosmicVpnHelper1.Error.{InvalidConf, NotImported, PermissionDenied, QbitRunning, NetlinkFailed, WebUi}` with a human message.
