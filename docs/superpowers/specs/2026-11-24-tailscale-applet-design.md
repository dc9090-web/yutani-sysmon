# Tailscale applet (design, 2026-11-24)

## Why

DC runs Tailscale on every machine and wants it in the COSMIC panel next to the other five Yutani applets: see at a glance whether the tailnet is connected, connecting or broken, switch it on and off, pick an exit node, and look up a device's address without opening a terminal or the admin console. The reference is [trayscale](https://github.com/DeedleFake/trayscale) (Go + GTK); this is the same idea as a native Rust COSMIC applet, as light as its siblings.

Decided with DC: scope **B** (essentials plus a device list) and the **hyper** HTTP library over the daemon's Unix socket (chosen over a hand-written client because it is the more stable option). Everything else below marked *(call)* is the designer's choice and open to correction.

## Goals

- A sixth applet in this workspace: own crate, binary, desktop entry, App ID, config and package, like the others.
- Panel: an icon and a short word that show **off / connecting / connected / needs attention**, never by colour alone.
- Popup: an on/off switch; the exit node in use and a picker to change it; this device's name and addresses; health and key-expiry warnings; a searchable device list with one-click copy.
- Talks to `tailscaled` directly over its local API. No `tailscale` CLI calls, no daemon of our own, no root helper.
- Idle cost in line with the siblings: about 0.1 % of one core and RSS no higher than Weather's (32.7 MB). Measure on the live panel and add the row to the README table.

## Non-goals (v1)

Taildrop, switching accounts/profiles, Serve/Funnel, advertising routes or this machine as an exit node, toggles for accept-routes / SSH / shields-up / MagicDNS, per-peer ping or traffic counters, first-time `tailscale up` with flags, installing Tailscale. Write any that come up into `NOTES.md`.

## Anti-goals

- Polling while the popup is closed, or spawning processes per refresh.
- Calling `sudo`/`pkexec` or adding a privileged helper to get write access.
- Breaking when a Tailscale update adds, removes or renames a JSON field.
- Logging or displaying node keys, auth URLs or the tailnet owner's e-mail in logs. Committed fixtures must be anonymised (the repo is public).
- Using Tailscale's logo as the icon.

## What was verified on DC's machine (tailscaled 1.104.7)

- Socket `/var/run/tailscale/tailscaled.sock` is world read/write. Requests are plain HTTP/1.1 to host `local-tailscaled.sock`, path prefix `/localapi/v0/`. No auth header is needed on Linux (the daemon checks the peer's uid).
- As an ordinary user: `GET status` → 200, `GET prefs` → 200, `GET watch-ipn-bus?mask=6` → a stream of one JSON object per line (first line carried `Version`, `SessionID`, `State`, `Prefs`; then silence for 25 s on an idle tailnet), `GET suggest-exit-node` → `{"ID": "...", "Name": "..."}`.
- As an ordinary user: `PATCH prefs` → **403 `prefs write access denied`**. Writes need the user to be the daemon's *operator*, set once with `sudo tailscale set --operator=<user>`. On DC's machine no operator is set.
- `status` top-level keys: `Version, TUN, BackendState, HaveNodeKey, AuthURL, TailscaleIPs, Self, Health, MagicDNSSuffix, CurrentTailnet, CertDomains, ExtraRecords, Peer, User, ClientVersion` (and `ExitNodeStatus` when one is in use). `Health` is a list of strings; right now it holds one non-fatal warning ("Some peers are advertising routes but --accept-routes is false").
- Peer fields seen: `ID, HostName, DNSName, OS, TailscaleIPs, CurAddr, Relay, PeerRelay, LastSeen, Online, ExitNode, ExitNodeOption, Active`, among others. DC's tailnet has 34 peers, two of them exit-node options, no Mullvad nodes.

The builder should confirm the following against Tailscale's source for the installed version (`ipn/backend.go`, `ipn/prefs.go`, `ipn/ipnstate`, `ipn/localapi`) before relying on them:

- Bus state numbers: 0 NoState, 1 InUseOtherUser, 2 NeedsLogin, 3 NeedsMachineAuth, 4 Stopped, 5 Starting, 6 Running.
- Bus mask bits: 2 initial state, 4 initial prefs, and the bits for "no private keys", "initial health state" and "rate limit".
- The bus health payload (`Health.Warnings`, each with a severity, text and an "impacts connectivity" flag).
- The `MaskedPrefs` write shape: each changed field plus a `<Field>Set: true` sibling, e.g. `{"WantRunning":true,"WantRunningSet":true}`.
- Whether `login-interactive` (POST) is the right call to start a sign-in and that the URL then arrives on the bus as `BrowseToURL`.

## Approaches considered

1. **Hand-written HTTP client on the Unix socket.** Lightest, nothing new to depend on, but we would own the reply framing and only test it against today's daemon.
2. **hyper over the Unix socket (chosen).** Mature HTTP handling; already in the workspace's dependency tree through `reqwest`, so no new third-party code overall. Slightly larger binary. DC picked this for stability.
3. **Shelling out to the `tailscale` CLI.** Rejected: a process per refresh, no events, not native.

## Architecture

New crate `crates/tailscale`, binary `cosmic-applet-tailscale`, App ID `io.github.dc.CosmicAppletTailscale`, laid out like `crates/vpn` (`main.rs`, `app.rs`, `model.rs`, `config.rs`, `localize.rs`, `demo.rs`, `widgets/`, plus `api.rs`). It reuses `crates/common` (panel sizing, inks, `ui::*` rows) and the VPN applet's patterns for the panel button, popup, in-popup settings page, toast, `APPLET_PREVIEW` window and `demo` feature.

Dependencies beyond the workspace set: `hyper` 1 (client, http1), `hyper-util` (tokio I/O adapter), `http-body-util`, `tokio` (`net`, `time`, `macros`), `serde_json`, `futures-util`, and whatever the siblings already use for timestamps and the current user name (`chrono`, `nix`). No `reqwest`, `zbus`, `oo7` or TLS.

### `api.rs`: the local API client

- One function to open a `UnixStream` to the socket and run a hyper HTTP/1 connection on it; a new connection per request is fine (requests are rare).
- Socket path: `/var/run/tailscale/tailscaled.sock`; the env var `TS_SOCKET` overrides it for tests *(call)*.
- `status() -> Status`, `patch_prefs(MaskedPrefs) -> Result<Prefs, ApiError>`, `suggest_exit_node() -> Option<Suggestion>`, `login_interactive()`, and `watch() -> impl Stream<Item = Notify>` that reads the bus line by line.
- Errors distinguish: **Unavailable** (socket missing or refused), **Denied** (403), **Other** (status code + short text). Bodies are never logged.
- Every serde struct takes only the fields the applet uses, all `Option`/defaulted, unknown fields ignored. A missing field degrades to "unknown" in the UI, never to a failed parse of the whole reply.

### Data flow

- **Bus subscription** (always on): `watch-ipn-bus` asking for initial state, initial prefs, initial health and peer changes (mask 4230). "No private keys" is a no-op in 1.104 and the "rate limit" bit is rejected, so neither is set. Each line updates state, prefs and health in the model. If the stream ends or the socket is unavailable, the state becomes Unavailable and the subscription retries with backoff from 1 s up to 5 s.
- **Status fetch** (`GET status`): once at start, after each bus message (500 ms later with the popup open, 5 s with it closed), when the popup opens, and every 5 s **only while the popup is open**. This supplies the device list, exit-node options, `Self`, tailnet name and `ExitNodeStatus`.
- **No timer runs while the popup is closed.** The process sleeps on the socket.
- **Writes** go through `patch_prefs`. The control shows busy until the bus confirms the new state, with a 20 s limit (as VPN's `BUSY_LIMIT`), after which it shows the last error. A 403 raises the operator banner.
- `view` does no I/O.

### Operator check

The applet compares `Prefs.OperatorUser` with the current user's name (uid 0 always passes). If they differ, the popup shows a banner *before* the user hits a 403:

> **Tailscale needs a one-time permission** · Switching on/off and choosing an exit node are reserved for the operator. Run this once: `sudo tailscale set --operator=daniel` **[Copy]**

The user name is filled in. The switch and exit-node rows stay visible but disabled with that reason as their tooltip. The banner clears by itself when the bus reports the new prefs.

## Model

`Link` is derived from the bus state and socket reachability:

| Link | From | Panel word | Tone |
|---|---|---|---|
| Unavailable | socket missing/refused | `ERR` | error |
| NeedsLogin | state 2 | `ERR` | error |
| NeedsApproval | state 3 | `ERR` | error |
| OtherUser | state 1 | `ERR` | error |
| Off | state 4 (or 0 with `WantRunning` false) | `OFF` | off |
| Connecting | state 5 (or 0 while starting) | `…` | busy |
| Connected | state 6 | `ON` | on |

Issue dot on the icon (as VPN's) *(call)*:

- **Red, and the word becomes `ERR`** even while the daemon reports Running: an expired node key, or a health warning that impacts connectivity. So red is never told apart from amber by colour alone.
- **Amber, word stays `ON`**: a key expiring within 14 days, or an exit node that is selected but offline.
- **No dot**: health warnings that don't impact connectivity. They appear only in the popup. DC's machine has a standing one ("…--accept-routes is false") that the applet can't clear, and a permanent amber dot would make the dot meaningless.

When "Show status in panel" is off, the chunk still comes back for Connecting and every `ERR` case, as VPN forces its chunks back for connecting and error states. Only `ON` and `OFF` are ever icon-only.

Peer connection text, from the status reply: offline → "Last seen 6d ago"; online and not `Active` → "Idle"; `CurAddr` set → "Direct"; else `PeerRelay` set → "Peer relay"; else `Relay` set → "Relay · syd".

All of this lives in `model.rs` as pure functions with unit tests, like VPN's `issue`/`chunk`/`header`.

## Panel

```text
 ╭───────────────╮
 │ [icon•]  TS   │   one chunk: label over value, fixed 3ch width
 │          ON   │
 ╰───────────────╯
```

- Icon: an original single-ink symbolic glyph in the family of the VPN reticle (three linked nodes), in four variants: off, connecting, on, on-with-exit-node. *(call; not Tailscale's dot-grid logo.)* Embedded in the binary and installed to `hicolor/scalable/apps/`, as VPN does.
- Chunk `TS` with the word from the table, coloured by tone, sized and stepped up on M+ panels like the siblings. A setting hides the chunk to leave the icon alone.
- Tooltip carries the full summary: "Tailscale connected · exit node bravo · 12 of 34 devices online".
- Vertical panels: icon above the chunk, centred.

## Popup (360 wide)

```text
┌──────────────────────── 360 ────────────────────────┐
│ [icon] Tailscale                           [■■■○]    │ header + switch
│        Connected · example.ts.net                    │ caption, tone-coloured
│ [⚠ Tailscale needs a one-time permission … [Copy]]   │ banners (only when needed)
│ [⚠ Some peers are advertising routes but …       ]   │ health warnings
│ ═════════════════════════════════════════════════════ │
│ This device   alpha                                  │
│ Address       100.64.0.1                        [⧉]  │
│ Name          alpha.example.ts.net              [⧉]  │
│ ─────────────────────────────────────────────────    │
│ Exit node     bravo                               ›  │ opens the picker page
│ ─────────────────────────────────────────────────    │
│ Devices · 12 of 34 online                            │
│ [ Search devices                                  ]  │
│ ● app-01          linux                      Direct  │ click = copy address
│ ● nas             linux                 Relay · syd  │
│ ○ app-03          linux            Last seen 6d ago  │
│ …                                          (scrolls) │
│ ─────────────────────────────────────────────────    │
│ ⚙ Applet settings                                 ›  │
└──────────────────────────────────────────────────────┘
```

- **Header caption** by state: "Connected · <tailnet>", "Connecting…", "Off", "Sign-in needed", "Waiting for approval", "Tailscale isn't running", "In use by another user".
- **Switch**: writes `WantRunning`. It stays usable while Connecting so a stuck start can be cancelled *(call; VPN locks its switch here)*.
- **Banners** (VPN's `banner` style, in this order): operator needed; sign-in needed with a **Sign in** button (starts an interactive login and opens the URL from the bus in the browser, the way AI Usage opens its sign-in page); Tailscale not running ("Start it with `sudo systemctl enable --now tailscaled`", copyable); key expired / expires in N days; then each health warning as a caption line (at most three, then "+N more").
- **This device**: host name, first IPv4 with copy, MagicDNS name (trailing dot removed) with copy. Hidden when there is no node key.
- **Exit node row**: current exit node's name, "None", or "<name> · offline" in amber.
- **Devices**: every peer except location-based (Mullvad) exit nodes. Online first, then by name; offline ones dimmed. The name is the first label of `DNSName`, falling back to `HostName`. Clicking a row copies its IPv4 (or its MagicDNS name, per a setting) and shows the toast "Copied 100.64.0.2". The search field filters on name and address and appears only when there are more than 8 devices *(call)*. The list scrolls inside libcosmic's 1000 px popup cap; the header, banners and settings row stay fixed, as Weather does.
- States with no node key (not running, needs login) show only header, banners and settings.

### Exit node page (in-popup, like the settings page)

Back header "Exit node", then radio rows (`ui::radio_row`):

- **None**
- **Recommended: <name>** when `suggest-exit-node` returns one (fetched when the page opens)
- every peer with `ExitNodeOption`, online first; offline ones disabled with "Offline · last seen …". If peers carry a location, they are grouped under country headings with the city as the caption, and a search field appears when there are more than 8.

Below: a toggle row "Allow local network access · Keep printers and other LAN devices reachable while using an exit node" (`ExitNodeAllowLANAccess`), enabled only while an exit node is set.

Choosing a row writes `ExitNodeID` (and clears `ExitNodeIP`), returns to the main page and shows busy on the exit-node row until the bus confirms.

### Settings page

- **Show status in panel** (the `TS` chunk), default on.
- **Show offline devices**, default on.
- **Clicking a device copies**: Address | Name, default Address.

Stored with cosmic-config under the App ID, version 1. No secrets are stored anywhere.

## Style and text

Same rules as the siblings: every colour from `theme.cosmic()` through `common::ink`, no hex values (keep VPN's `no_hex_colours` test); every string in `i18n/en/cosmic_applet_tailscale.ftl` (keep the `every_string_key_exists` test); no `a11y` feature, so tooltips carry the summaries and every state has a word.

## Testing

- Fixtures in `crates/tailscale/tests/fixtures/`, captured from the live daemon and **anonymised** (host names, IPs, node IDs, keys, tailnet name, e-mail): status when running, stopped, needs-login, with an exit node in use, with location-based exit nodes; bus lines for each state and for health warnings; a 403 body.
- Unit tests: parsing each fixture; parsing with fields removed; `Link`, dot, word, header caption and peer-connection mapping; device sort and filter; exit-node grouping; "last seen" text; the exact JSON of each prefs write.
- A client test against a throwaway Unix socket server (`TS_SOCKET`) covering a normal reply, a streamed bus, a 403 and a refused connection.
- `just tailscale-demo`: the popup in a window cycling every state from the fixtures (`TAILSCALE_DEMO=1`, feature `demo`), as `vpn-demo` does.
- DC approved this design on 2026-11-24 and asked for it to be built, installed and tested on his workstation, fixing what turns up. Build and test against the fake socket first; the "signed out" and "stopped" fixtures are hand-written from Tailscale's source so the machine is never logged out of the tailnet. The operator command needs DC's sudo.
- Live checks on DC's machine: before the operator command (banner shown, controls disabled), after it (switch off/on, pick and clear an exit node, LAN toggle), `systemctl stop tailscaled` (ERR, then recovery without restarting the applet), suspend/resume, and the CPU/RSS measurement.

## Repo wiring

Workspace member; `justfile` (`applets`, install, uninstall, `run`, demo recipe); `packaging/tailscale.yaml` (package `yutani-tailscale`) and the loop in `packaging/build.sh`; `packaging/arch/PKGBUILD`; `.github/workflows/release.yml`; desktop entry "Yutani Tailscale"; README (six applets, a Tailscale section with the operator command, the footprint row); `NOTES.md` decisions. The version bump and release are separate steps DC asks for.

## Left to the builder

Widget breakdown and message types; exact spacing where VPN's DESIGN doesn't already answer it; the icon drawings (show DC before finalising); debounce and backoff constants; how the clipboard and browser are reached (reuse what VPN and AI Usage do).

## Changed after the first install (2026-11-24)

DC installed the first build and asked for two changes, which supersede the Panel and Model sections above where they differ:

- **Panel icon:** Tailscale's own dot-grid mark (DC's SVG) with one status circle over its corner, green when connected, orange while connecting, red when not connected. The `TS ON` word, the mesh glyph, the amber/red issue dot and the `show_status` setting are gone. Non-blocking warnings show in the popup and tooltip only.
- **Popup rows:** long names wrap inside their columns; nothing may leave the popup or clip an icon.
