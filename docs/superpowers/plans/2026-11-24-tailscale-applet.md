# Tailscale Applet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A sixth COSMIC panel applet, `cosmic-applet-tailscale`, that shows Tailscale's state, switches it on and off, picks an exit node and lists devices.

**Architecture:** A new crate `crates/tailscale` laid out like `crates/vpn`. `api.rs` speaks HTTP/1 to `tailscaled`'s Unix socket with hyper; a long-lived `watch-ipn-bus` stream pushes state, prefs and health; `GET status` is fetched on events and every 5 s only while the popup is open. `model.rs` turns those into everything the UI shows, as pure tested functions.

**Tech Stack:** Rust 2024, libcosmic (workspace pin `60ad2cc`), hyper 1 + hyper-util + http-body-util, tokio, serde/serde_json, chrono, nix, i18n-embed.

**Spec:** `docs/superpowers/specs/2026-11-24-tailscale-applet-design.md`

## Global Constraints

- No `tailscale` CLI calls, no `sudo`/`pkexec`, no helper. The only process ever spawned is `xdg-open` for the sign-in URL.
- No timer runs while the popup is closed. `view` does no I/O.
- Every colour from `theme.cosmic()` via `common::ink`; no hex values. Every user-visible string in `i18n/en/cosmic_applet_tailscale.ftl`.
- Every serde struct: only the fields used, each `#[serde(default)]`, unknown fields ignored. Response bodies, node keys, auth URLs and e-mail addresses are never logged.
- Fixtures are anonymised: hosts `alpha`, `bravo`…, tailnet `example.ts.net`, IPs from `100.64.0.0/24` and `fd7a:115c:a1e0::/64` with invented suffixes, IDs `nAAAA1CNTRL`…, keys `nodekey:00…`, owner `user@example.com`.
- App ID `io.github.dc.CosmicAppletTailscale`; binary `cosmic-applet-tailscale`; package `yutani-tailscale`; fluent domain `cosmic_applet_tailscale`.
- Bus mask: `2 | 4 | 128` = `134` (initial state, initial prefs, initial health). State numbers: 0 NoState, 1 InUseOtherUser, 2 NeedsLogin, 3 NeedsMachineAuth, 4 Stopped, 5 Starting, 6 Running (confirmed in tailscale v1.104.7 `ipn/backend.go`).
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass after every task.
- Work on branch `tailscale-applet`. Commit after each task; do not push, tag or release.

## Build environment (this workstation)

The workstation is a headless Ubuntu VM with no Rust toolchain, no `libxkbcommon-dev`/`libwayland-dev` and no sudo, so every `cargo` command in this plan runs in a container. Task 1 creates the wrapper `.dev/cargo` (kept out of git through `.git/info/exclude`), and from then on "Run: `cargo …`" means `.dev/cargo …`. There is no COSMIC session here: the panel and popup can only be seen on DC's COSMIC machine (Task 9).

## Review Focus

1. **`tailscaled` restarts or is stopped while the applet runs** → the panel shows `ERR` within a couple of seconds and recovers by itself when the daemon returns. (Task 2 test `watch_ends_then_reconnect`, Task 4 test `unavailable_then_back`.)
2. **A write is refused (403)** → the control stops showing busy at once and the operator banner appears; nothing is retried in a loop. (Task 2 `patch_denied`, Task 4 `denied_raises_banner`.)
3. **A peer with no `DNSName`, no IPv4 address or a zero `LastSeen`** → it still gets a row, named by `HostName` (or its ID), with copy disabled and "Last seen" omitted. (Task 3 `peer_degenerate`.)
4. **A tailnet with hundreds of location-based exit nodes** → none appear in the device list; the exit page groups them by country and stays searchable. (Task 3 `mullvad_grouping`.)
5. **A reply missing fields a future Tailscale version drops** (`Self`, `Peer`, `Prefs.OperatorUser`, `Health`) → parsing succeeds and the UI shows "unknown", not an error state. (Task 1 `parses_with_fields_removed`.)

---

### Task 1: Crate skeleton, build wrapper, API types and fixtures

**Files:**
- Create: `.dev/Dockerfile`, `.dev/cargo` (untracked)
- Modify: `Cargo.toml` (add `"crates/tailscale"` to `members`)
- Create: `crates/tailscale/Cargo.toml`, `crates/tailscale/src/main.rs`, `crates/tailscale/src/api/mod.rs`, `crates/tailscale/src/api/types.rs`
- Create: `crates/tailscale/tests/fixtures/{status_running,status_stopped,status_needs_login,status_exit_node,status_mullvad}.json`, `bus_{running,stopped,starting,needs_login,health}.ndjson`, `prefs_denied.txt`, `README.md`

**Interfaces — Produces** (`api::types`, all `Debug, Clone, Default, PartialEq, Deserialize`):

```rust
pub struct Status { pub backend_state: String, pub self_node: Option<Peer>, pub peers: Vec<Peer>,   // from the "Peer" map's values
                    pub health: Vec<String>, pub tailnet: Option<Tailnet>, pub exit_node: Option<ExitNodeStatus>, pub have_node_key: bool }
pub struct Tailnet { pub name: String, pub magic_dns_suffix: String }
pub struct ExitNodeStatus { pub id: String, pub online: bool }
pub struct Peer { pub id: String, pub host_name: String, pub dns_name: String, pub os: String, pub ips: Vec<String>,
                  pub cur_addr: String, pub relay: String, pub peer_relay: String, pub last_seen: Option<DateTime<Utc>>, // Go zero time → None
                  pub online: bool, pub active: bool, pub exit_node: bool, pub exit_node_option: bool,
                  pub location: Option<Location>, pub key_expiry: Option<DateTime<Utc>> }
pub struct Location { pub country: String, pub country_code: String, pub city: String }
pub struct Prefs { pub want_running: bool, pub exit_node_id: String, pub exit_node_allow_lan: bool, pub operator_user: String, pub logged_out: bool }
pub struct Notify { pub state: Option<u8>, pub prefs: Option<Prefs>, pub health: Option<Health>, pub browse_to_url: Option<String>, pub err_message: Option<String> }
pub struct Health { pub warnings: Vec<Warning> }           // from the "Warnings" map's values, sorted by code
pub struct Warning { pub code: String, pub title: String, pub text: String, pub impacts_connectivity: bool }
pub struct Suggestion { pub id: String, pub name: String }
#[derive(Debug, Clone, Default, PartialEq, Serialize)] pub struct MaskedPrefs { /* private */ }
impl MaskedPrefs { pub fn want_running(on: bool) -> Self; pub fn exit_node(id: &str) -> Self; pub fn allow_lan(on: bool) -> Self; }
```

- [ ] **Step 1: Build wrapper.** `.dev/Dockerfile`: `FROM rust:1.92-bookworm`, apt `libxkbcommon-dev libwayland-dev pkg-config`, `rustup component add clippy rustfmt`. `.dev/cargo`: builds the image `yutani-dev` if absent, then `docker run --rm -u $(id -u):$(id -g) -v "$PWD":/w -w /w -v yutani-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/w/target yutani-dev cargo "$@"`. Add `.dev/` to `.git/info/exclude`. Create branch `tailscale-applet`.
  Run: `.dev/cargo test -p yutani-monitor-common` — Expected: existing tests pass (proves the container builds libcosmic).
- [ ] **Step 2: Fixtures.** Capture `status` and one bus line from the live socket (`curl --unix-socket /var/run/tailscale/tailscaled.sock http://local-tailscaled.sock/localapi/v0/…`), anonymise per Global Constraints, trim to 6 peers (3 online: one direct, one relayed, one idle; 2 offline; 1 exit-node option). Hand-write the other states from the same shape: `status_stopped` (`BackendState: "Stopped"`), `status_needs_login` (`"NeedsLogin"`, `HaveNodeKey: false`, no `Self.TailscaleIPs`, `Peer: null`), `status_exit_node` (`ExitNodeStatus {ID, Online: true}`, that peer `ExitNode: true`), `status_mullvad` (5 extra peers with `Location` in 2 countries, `ExitNodeOption: true`). Bus lines: `{"State":6,"Prefs":{…}}` etc.; `bus_health.ndjson` has `Health.Warnings` with one `ImpactsConnectivity: true` entry and one without. `prefs_denied.txt` is `prefs write access denied`. `README.md` says how they were made and that nothing real remains.
- [ ] **Step 3: Failing tests** in `api/types.rs`:

```rust
#[test] fn parses_running() { let s: Status = fx("status_running.json"); assert_eq!(s.backend_state, "Running"); assert_eq!(s.peers.len(), 6);
    assert_eq!(s.self_node.unwrap().ips[0], "100.64.0.1"); assert_eq!(s.tailnet.unwrap().magic_dns_suffix, "example.ts.net"); }
#[test] fn zero_time_is_none() { /* a peer with LastSeen "0001-01-01T00:00:00Z" → last_seen == None */ }
#[test] fn parses_needs_login() { let s: Status = fx("status_needs_login.json"); assert!(!s.have_node_key); assert!(s.peers.is_empty()); }
#[test] fn parses_with_fields_removed() { let s: Status = serde_json::from_str("{}").unwrap(); assert_eq!(s, Status::default());
    let n: Notify = serde_json::from_str(r#"{"State":6,"Prefs":{}}"#).unwrap(); assert_eq!(n.prefs.unwrap().operator_user, ""); }
#[test] fn bus_health() { /* first line of bus_health.ndjson → 2 warnings, exactly one impacts_connectivity */ }
#[test] fn masked_prefs_json() {
    assert_eq!(serde_json::to_string(&MaskedPrefs::want_running(true)).unwrap(), r#"{"WantRunning":true,"WantRunningSet":true}"#);
    assert_eq!(serde_json::to_string(&MaskedPrefs::exit_node("nAAAA2CNTRL")).unwrap(), r#"{"ExitNodeID":"nAAAA2CNTRL","ExitNodeIDSet":true,"ExitNodeIP":"","ExitNodeIPSet":true}"#);
    assert_eq!(serde_json::to_string(&MaskedPrefs::allow_lan(false)).unwrap(), r#"{"ExitNodeAllowLANAccess":false,"ExitNodeAllowLANAccessSet":true}"#); }
```

- [ ] **Step 4:** Run `cargo test -p cosmic-applet-tailscale` — Expected: fails to compile (types missing).
- [ ] **Step 5: Implement** `Cargo.toml` (deps: the workspace set as `crates/vpn` uses, plus `hyper = { version = "1", features = ["client", "http1"] }`, `hyper-util = { version = "0.1", features = ["tokio"] }`, `http-body-util = "0.1"`, `tokio = { version = "1", features = ["net", "time", "macros", "rt"] }`, `serde_json = "1"`, `futures-util` and `chrono` as ai-usage declares them, `nix` as vpn declares it; `[features] demo = []`), a `main.rs` with `mod api;` and an empty `fn main()`, and the types with `#[serde(rename = "…")]` to the Go names. `MaskedPrefs` holds `Option`s with `skip_serializing_if`, field order as in the test.
- [ ] **Step 6:** Run `cargo test -p cosmic-applet-tailscale` — Expected: 6 passed.
- [ ] **Step 7:** Commit `Tailscale: crate skeleton, API types and fixtures`.

### Task 2: Local API client

**Files:**
- Create: `crates/tailscale/src/api/client.rs`
- Modify: `crates/tailscale/src/api/mod.rs`

**Interfaces — Produces:**

```rust
#[derive(Debug, Clone, PartialEq)] pub enum ApiError { Unavailable, Denied, Other(u16, String) }   // String: first 120 chars of the body, never logged above debug
pub fn socket_path() -> PathBuf;                         // $TS_SOCKET, else /var/run/tailscale/tailscaled.sock
pub async fn status() -> Result<Status, ApiError>;                         // GET  /localapi/v0/status
pub async fn patch_prefs(p: MaskedPrefs) -> Result<Prefs, ApiError>;       // PATCH /localapi/v0/prefs
pub async fn suggest_exit_node() -> Result<Option<Suggestion>, ApiError>;  // GET  /localapi/v0/suggest-exit-node; any non-200 → Ok(None)
pub async fn login_interactive() -> Result<(), ApiError>;                  // POST /localapi/v0/login-interactive
#[derive(Debug, Clone, PartialEq)] pub enum BusEvent { Notify(Notify), Down }
pub fn watch() -> impl Stream<Item = BusEvent>;          // never ends: on any failure yields Down once, waits 1 s doubling to 30 s, reconnects; resets to 1 s after a parsed line
```

Every request sets `Host: local-tailscaled.sock`. One fresh connection per request. `ConnectionRefused`/`NotFound`/`PermissionDenied` on connect → `Unavailable`; 403 → `Denied`. `watch` requests `/localapi/v0/watch-ipn-bus?mask=134` and splits the body on `\n` across frame boundaries; a line that fails to parse is skipped with a `debug!` of its length only.

- [ ] **Step 1: Failing tests** in `client.rs`, each starting a `tokio::net::UnixListener` on a temp path, setting `TS_SOCKET` (serialise the tests with a static mutex), and answering with raw bytes:
  - `status_ok`: serves `status_running.json` with `Content-Length` → `status().await.unwrap().peers.len() == 6`.
  - `status_chunked`: same body with `Transfer-Encoding: chunked` in 3 chunks → same result.
  - `patch_denied`: 403 + `prefs_denied.txt` → `Err(ApiError::Denied)`; asserts the request line was `PATCH /localapi/v0/prefs HTTP/1.1` and the body was the `want_running(false)` JSON.
  - `no_socket`: `TS_SOCKET` points at a missing path → `Err(ApiError::Unavailable)`.
  - `watch_lines_split`: chunked stream delivering one bus line split mid-JSON across two chunks, then a second line → two `BusEvent::Notify` with states 5 then 6.
  - `watch_ends_then_reconnect`: server closes after one line, accepts again and sends another → events are `Notify, Down, Notify` (use `tokio::time::pause` so the 1 s backoff is instant).
- [ ] **Step 2:** Run `cargo test -p cosmic-applet-tailscale client` — Expected: FAIL (functions missing).
- [ ] **Step 3: Implement** with `hyper::client::conn::http1::handshake(TokioIo::new(UnixStream))`, the connection task spawned on the current runtime, bodies collected with `http_body_util::BodyExt`. `watch` is built with `futures_util::stream::unfold`.
- [ ] **Step 4:** Run the same command — Expected: 6 passed.
- [ ] **Step 5:** Commit `Tailscale: local API client over the Unix socket`.

### Task 3: Model

**Files:**
- Create: `crates/tailscale/src/model.rs`

**Interfaces — Consumes:** `api::types::*`. **Produces:**

```rust
pub enum Link { Unavailable, NeedsLogin, NeedsApproval, OtherUser, Off, Connecting, Connected }
pub enum Tone { Off, Busy, On, Warn, Err }
pub enum Issue { Error, Warning }
pub enum IconKind { Off, Connecting, On, Exit }
pub struct Snapshot { pub reachable: bool, pub state: Option<u8>, pub prefs: Prefs, pub health: Vec<Warning>, pub status: Status }

pub fn link(s: &Snapshot) -> Link;                               // !reachable → Unavailable; 1,2,3,4,5,6 per the spec table; 0/None → Off if !want_running else Connecting
pub fn issue(s: &Snapshot, now: DateTime<Utc>) -> Option<Issue>; // spec "Issue dot" rules; 14-day key window
pub fn word(s: &Snapshot, now: DateTime<Utc>) -> (&'static str, Tone);   // ftl keys panel-on / panel-off / panel-connecting / panel-error
pub fn show_chunk(s: &Snapshot, now: DateTime<Utc>, show_status: bool) -> bool;  // false only when !show_status and the word is ON or OFF
pub fn icon(s: &Snapshot) -> IconKind;                           // Exit when Connected and prefs.exit_node_id is non-empty
pub fn header(s: &Snapshot) -> (&'static str, Option<Tone>);     // head-connected, head-connecting, head-off, head-login, head-approval, head-unavailable, head-other-user
pub fn can_write(operator_user: &str, user: &str, uid: u32) -> bool;     // uid == 0 || (!user.is_empty() && operator_user == user)
pub fn key_days_left(s: &Snapshot, now: DateTime<Utc>) -> Option<i64>;   // Some only when ≤ 14 (negative = expired)

pub struct Device { pub id: String, pub name: String, pub os: String, pub ip: Option<String>, pub dns: Option<String>, pub online: bool, pub conn: Conn }
pub enum Conn { Direct, PeerRelay, Relay(String), Idle, LastSeen(Option<DateTime<Utc>>) }
pub fn devices(st: &Status, query: &str, show_offline: bool) -> Vec<Device>;   // drops peers with a location; online first, then name (case-insensitive); query matches name or any IP
pub fn online_count(st: &Status) -> (usize, usize);              // (online, total), same exclusion
pub fn last_seen(t: DateTime<Utc>, now: DateTime<Utc>) -> String; // "just now" < 1 min, "5m ago", "3h ago", "6d ago"

pub struct ExitOption { pub id: String, pub name: String, pub caption: Option<String>, pub online: bool, pub last_seen: Option<DateTime<Utc>> }
pub struct ExitGroup { pub heading: Option<String>, pub options: Vec<ExitOption> }   // heading None = the tailnet's own nodes, listed first
pub fn exit_groups(st: &Status, query: &str) -> Vec<ExitGroup>;  // then one group per country, alphabetical; caption = city; online first within a group
pub fn exit_name(s: &Snapshot) -> Option<(String, bool)>;        // (name, online) of the node whose id == prefs.exit_node_id; unknown id → (id, false)
```

`Device.name` is the first label of `dns_name`, else `host_name`, else `id`. `Device.dns` is `dns_name` without the trailing dot. `Device.ip` is the first address without `:`.

- [ ] **Step 1: Failing tests** (fixtures through `include_str!`; a fixed `now` of `2026-11-24T00:00:00Z`):
  - `links`: one assertion per row of the spec table, plus `state None + want_running false → Off`.
  - `words_and_dots`: Connected, no warnings → `("panel-on", On)`, `issue == None`; Connected + a non-connectivity warning → still `None`; Connected + `impacts_connectivity` → `("panel-error", Err)`, `Some(Error)`; Connected + key expired yesterday → `Err`; key expiring in 10 days → `("panel-on", On)` with `Some(Warning)`; exit node selected and offline → `Some(Warning)`.
  - `chunk_forced_back`: `show_status false` → hidden for ON and OFF, shown for Connecting and every ERR case.
  - `operator`: `can_write("daniel","daniel",1000)`, `!can_write("","daniel",1000)`, `!can_write("bob","daniel",1000)`, `can_write("","root",0)`.
  - `device_order_and_filter`: `status_running` → names in order online-then-alphabetical; query `"brav"` → 1 row; query `"100.64.0.3"` → 1 row; `show_offline false` → 3 rows; `online_count == (3, 6)`.
  - `conn_text`: the direct, relayed, idle and offline fixture peers map to `Direct`, `Relay("syd")`, `Idle`, `LastSeen(Some(_))`.
  - `peer_degenerate`: a peer with empty `DNSName`, `TailscaleIPs: []`, zero `LastSeen` → `name == host_name`, `ip == None`, `conn == LastSeen(None)`.
  - `mullvad_grouping`: `status_mullvad` → `devices()` has no located peer; `exit_groups` is `[None-heading group, "Country A", "Country B"]`; query on a city narrows to one option.
  - `last_seen_text`: 30 s → "just now", 5 min, 3 h, 6 d.
- [ ] **Step 2:** Run `cargo test -p cosmic-applet-tailscale model` — Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run again — Expected: 9 passed.
- [ ] **Step 5:** Commit `Tailscale: model`.

### Task 4: App shell — panel, data flow, switch

**Files:**
- Create: `crates/tailscale/src/{app.rs,config.rs,localize.rs}`, `crates/tailscale/src/widgets/{mod.rs,panel.rs,glyph.rs}`, `crates/tailscale/i18n.toml`, `crates/tailscale/i18n/en/cosmic_applet_tailscale.ftl`, `crates/tailscale/resources/io.github.dc.CosmicAppletTailscale.desktop`, `crates/tailscale/resources/icons/tailscale-mesh-{off,connecting,on,exit}-symbolic.svg`
- Modify: `crates/tailscale/src/main.rs`

**Interfaces — Consumes:** Tasks 2–3. **Produces:**

```rust
// config.rs
pub const APP_ID: &str = "io.github.dc.CosmicAppletTailscale";
pub enum CopyWhat { Address, Name }                       // default Address
pub struct Config { pub show_status: bool /*true*/, pub show_offline: bool /*true*/, pub copy: CopyWhat }   // CosmicConfigEntry, version 1
// app.rs
pub enum Page { Main, ExitNode, Settings }
pub enum Busy { None, Switch(Instant), Exit(Instant) }    // cleared by the bus, or after BUSY_LIMIT = 20 s
pub struct App { /* core, popup, config, snap: Snapshot, user: String, uid: u32, page, busy, denied: bool, last_error: Option<String>, query, exit_query, suggestion, toast */ }
impl App { pub fn apply(&mut self, ev: BusEvent) -> bool; /* returns true when a status fetch should follow */ pub fn on_patch(&mut self, r: Result<Prefs, ApiError>); }
```

Data flow, in `update`/`subscription`:
- `Subscription::run(api::watch)` → `Message::Bus`. `apply` sets `snap.reachable`, `state`, `prefs`, `health`; `BusEvent::Down` clears `status` and sets `reachable = false`. A `browse_to_url` is opened with `xdg-open` only if a sign-in was requested from this applet in the last 60 s.
- After a bus event: a status fetch debounced 500 ms (one pending at a time). Also on start and on popup open.
- `cosmic::iced::time::every(5 s)` is in the subscription list **only while the popup is open**.
- Switch: `patch_prefs(MaskedPrefs::want_running(on))`, `busy = Switch(now)`; `on_patch(Err(Denied))` sets `denied = true` and clears busy; `Err(Other)` stores `last_error`; `Ok(prefs)` updates `snap.prefs`.
- The panel button, popup open/close, escape handling, `APPLET_PREVIEW` window and `style()` are copied from `crates/vpn/src/app.rs` (`panel_view`, `open_popup`, `escape`, `view_window`). The panel chunk reuses the VPN chunk widget's sizing with label `TS`, 3 ch wide.
- Icons: four original 16-grid single-ink SVGs (three nodes joined by lines; `off` = outline nodes, `connecting` = one node filled, `on` = all filled, `exit` = all filled plus an outward arrow from one node), embedded with `include_bytes!` as VPN's reticle does, and referenced by the desktop entry as `tailscale-mesh-on-symbolic`.
- `.ftl` keys for this task: `panel-on = ON`, `panel-off = OFF`, `panel-connecting = …`, `panel-error = ERR`, `panel-label = TS`, the seven `head-*` captions from the spec, `title = Tailscale`, `tooltip-*` summary strings, `settings = Applet settings`.
- Desktop entry: copy VPN's, `Name=Yutani Tailscale`, `Comment=Tailscale status, on/off switch, exit nodes and devices`, `Exec=cosmic-applet-tailscale`.

- [ ] **Step 1: Failing tests** in `app.rs` (construct `App` through a `#[cfg(test)] fn test_app() -> App` that needs no compositor):
  - `unavailable_then_back`: `apply(Down)` → `link == Unavailable` and `status == Status::default()`; then `apply(Notify{state: Some(6), prefs: Some(..)})` → `Connected`, returns `true`.
  - `denied_raises_banner`: `busy = Switch(_)`, `on_patch(Err(ApiError::Denied))` → `denied`, `busy == None`.
  - `busy_clears_on_confirm`: `busy = Switch`, `apply(Notify{state: Some(4)})` → `busy == None`.
  - `busy_times_out`: `Busy::Switch(Instant::now() - 21 s)` → `App::busy_expired()` is true.
  - `browse_only_when_asked`: a `browse_to_url` with no sign-in requested → `App::should_open(url)` false; after `login_requested = Some(now)` → true.
  - `no_hex_colours` and `every_string_key_exists`, as in `crates/vpn/src/app.rs:1660-1710` with this crate's sources and `.ftl`, including the `panel-*` and `head-*` keys the model returns.
- [ ] **Step 2:** Run `cargo test -p cosmic-applet-tailscale app` — Expected: FAIL.
- [ ] **Step 3:** Implement; the popup's main page for now is header (icon, title, caption, switch) and the settings row.
- [ ] **Step 4:** Run `cargo test -p cosmic-applet-tailscale` and `cargo clippy -p cosmic-applet-tailscale --all-targets -- -D warnings` — Expected: all pass, no warnings.
- [ ] **Step 5:** Commit `Tailscale: panel, event stream and on/off switch`.

### Task 5: Popup main page — banners, this device, devices

**Files:**
- Modify: `crates/tailscale/src/app.rs`, `crates/tailscale/i18n/en/cosmic_applet_tailscale.ftl`
- Create: `crates/tailscale/src/widgets/{banner.rs,device.rs}`

**Interfaces — Produces:**

```rust
pub enum Banner { Operator { command: String }, SignIn, NotRunning { command: &'static str }, KeyExpired, KeyExpiring(i64), Health(String), HealthMore(usize) }
pub fn banners(s: &Snapshot, can_write: bool, user: &str, now: DateTime<Utc>) -> Vec<Banner>;   // in app.rs or model.rs; spec order; at most 3 Health then HealthMore(n)
```

Exact copy (`.ftl`):
- `operator-title = Tailscale needs a one-time permission`; `operator-body = Switching on/off and choosing an exit node are reserved for the operator. Run this once:`; command `sudo tailscale set --operator={user}`.
- `not-running-title = Tailscale isn't running`; command `sudo systemctl enable --now tailscaled`.
- `signin-title = Sign-in needed`; button `Sign in`.
- `key-expired = This device's key has expired. Sign in again to reconnect.`; `key-expiring = This device's key expires in { $days } days.`
- `devices-heading = Devices · { $online } of { $total } online`; `search-devices = Search devices`; `copied = Copied { $text }`; `this-device = This device`, `address = Address`, `name = Name`, `exit-node = Exit node`, `exit-none = None`, `exit-offline = { $name } · offline`.
- Connection words: `conn-direct = Direct`, `conn-peer-relay = Peer relay`, `conn-relay = Relay · { $code }`, `conn-idle = Idle`, `conn-last-seen = Last seen { $ago }`, `conn-offline = Offline`.

Behaviour: layout as the spec's sketch. Banners use VPN's `banner()`/`boxed()` shapes with a copy button calling `cosmic::iced::clipboard::write`. The switch and the exit-node row are disabled when `!can_write`, with `operator-title` as their tooltip. "This device" is hidden when `!status.have_node_key`. The device list sits in a `scrollable` between fixed header/banners and the fixed settings row; the search field shows when `online_count().1 > 8`. A row press copies `Device.ip` (or `Device.dns` when `config.copy == Name`) and shows the 1.4 s toast; a row with nothing to copy is not pressable. Offline rows use `Ink::Muted`.

- [ ] **Step 1: Failing tests:**
  - `banner_order`: unreachable → `[NotRunning]` only; needs-login → `[SignIn]` (plus `Operator` first when `!can_write`); connected, `!can_write`, key in 10 days, 5 health strings → `[Operator, KeyExpiring(10), Health×3, HealthMore(2)]`.
  - `operator_command_names_user`: `Banner::Operator.command == "sudo tailscale set --operator=daniel"`.
  - `copy_target`: `App::copy_text(&device)` gives the IP by default, the DNS name with `CopyWhat::Name`, `None` for the degenerate peer.
  - `search_threshold`: 6 devices → no search field; 9 → shown (`App::show_search()`).
- [ ] **Step 2:** Run tests — Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the crate's tests and clippy — Expected: pass.
- [ ] **Step 5:** Commit `Tailscale: banners, this device and the device list`.

### Task 6: Exit node page and settings page

**Files:**
- Modify: `crates/tailscale/src/app.rs`, the `.ftl`
- Create: `crates/tailscale/src/widgets/exit.rs`

Behaviour:
- Opening `Page::ExitNode` fires `suggest_exit_node()`; the result is stored in `suggestion` and dropped when the page closes.
- Rows (`common::ui::radio_row`): `exit-none`; `exit-recommended = Recommended: { $name }` when a suggestion exists and is not already selected; then `model::exit_groups` with `common::ui::group_label` headings. Offline options are not pressable and carry `exit-offline-seen = Offline · last seen { $ago }`. Search field when the options total more than 8.
- Pressing a row: `patch_prefs(MaskedPrefs::exit_node(id))` (`""` for None), `busy = Exit(now)`, `page = Main`. Busy clears when a bus `Prefs` arrives whose `exit_node_id` matches.
- Toggle row `exit-lan-title = Allow local network access`, `exit-lan-caption = Keep printers and other LAN devices reachable while using an exit node`; enabled only when `prefs.exit_node_id` is non-empty and `can_write`; writes `MaskedPrefs::allow_lan`.
- Settings page (`common::ui::back_header`, VPN's `toggle_row`): `set-status = Show status in panel`, `set-offline = Show offline devices`, and a two-segment control `set-copy = Clicking a device copies` with `Address` / `Name`. Each writes its config key through the `save` helper pattern in `crates/vpn/src/app.rs:219`.

- [ ] **Step 1: Failing tests:**
  - `exit_select_sets_busy_and_page`: `App::select_exit("nAAAA2CNTRL")` → `busy == Exit(_)`, `page == Main`, and the returned `MaskedPrefs` equals `MaskedPrefs::exit_node("nAAAA2CNTRL")`.
  - `exit_busy_clears_on_matching_prefs`: a bus `Prefs` with another id leaves busy set; the matching id clears it.
  - `recommended_hidden_when_selected`: suggestion id == `prefs.exit_node_id` → `App::recommended()` is `None`.
  - `lan_toggle_enabled`: false with no exit node, false when `!can_write`, true otherwise.
- [ ] **Step 2:** Run — Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the crate's tests and clippy — Expected: pass.
- [ ] **Step 5:** Commit `Tailscale: exit node picker and settings`.

### Task 7: Demo mode and `--status`

**Files:**
- Create: `crates/tailscale/src/demo.rs`
- Modify: `crates/tailscale/src/main.rs`, `crates/tailscale/src/app.rs`

Behaviour:
- `demo.rs` mirrors `crates/vpn/src/demo.rs`: `enabled()` reads `TAILSCALE_DEMO`, `first()` reads `TAILSCALE_DEMO_SCENE`, `shot_path()` reads `TAILSCALE_SHOT`, `STEP = 8 s`, `pub fn scene(i: usize) -> Scene { name, snap: Snapshot, can_write: bool, page: Page, busy: bool }`. Scenes, in order: connected; connected + exit node; exit page; exit page with location groups; connecting; off; operator banner; sign-in needed; not running; health error (`ERR` while Running); key expiring; settings. With the demo on, no API call is made.
- `cosmic-applet-tailscale --status`: one bus read (first line) plus one `status()`, then prints the derived view and exits 0 (1 if the socket is unavailable):

```text
link        Connected
panel       ON (dot: none)
operator    not set for daniel (writes will be refused)
tailnet     example.ts.net
this device alpha  100.64.0.1
exit node   none
devices     3 of 6 online
warnings    1 (0 affecting connectivity)
```

  It prints no keys, URLs or e-mail addresses. This is the headless live check on machines without COSMIC.

- [ ] **Step 1: Failing tests:** `demo::tests::every_scene_builds` (all scenes construct and `model::link` matches the scene's name) and `status_report` (`fn report(&Snapshot, user, uid, now) -> String` on `status_running` + `bus_running` equals the block above).
- [ ] **Step 2:** Run `cargo test -p cosmic-applet-tailscale --features demo` — Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run tests with and without `--features demo`, then `cargo run -q -p cosmic-applet-tailscale -- --status` against the live daemon (mount the socket into the container: add `-v /var/run/tailscale:/var/run/tailscale` to `.dev/cargo`) — Expected: tests pass; the report shows `link Connected`, `operator not set for daniel`, `devices N of 34 online`.
- [ ] **Step 5:** Commit `Tailscale: demo scenes and --status`.

### Task 8: Repo wiring and docs

**Files:**
- Modify: `justfile`, `packaging/build.sh`, `packaging/arch/PKGBUILD`, `README.md`, `NOTES.md`, `crates/common/src/lib.rs` (doc comment only if it still says "both applets")
- Create: `packaging/tailscale.yaml`

- `justfile`: add `tailscale` to `applets`; install/uninstall lines for the binary, desktop entry and four icons (same form as the VPN lines); recipes `tailscale-demo` (`TAILSCALE_DEMO=1 APPLET_PREVIEW=1 cargo run -p cosmic-applet-tailscale --features demo`) and `tailscale-status`.
- `packaging/tailscale.yaml`: copy `packaging/vpn.yaml` without the helper files and scripts; `name: yutani-tailscale`; description "Yutani Tailscale: COSMIC panel applet with Tailscale status, an on/off switch, an exit-node picker and a device list."
- `packaging/build.sh`: add `tailscale` to the loop. `PKGBUILD`: add `yutani-tailscale` to `pkgname` and a `package_yutani-tailscale()` like the non-helper applets. `release.yml` needs no change (it builds the workspace and runs `build.sh`).
- `README.md`: "Five" → "Six" in the intro and repo description text; a row in "What's in the box" (🔗 **Tailscale**); a "Tailscale" section covering the panel states, the popup, the one-time `sudo tailscale set --operator=$USER`, and that it talks only to the local daemon's socket; the footprint row left as "to be measured" until Task 9.
- `NOTES.md`: the spec's *(call)* decisions, the build-in-a-container note, and the v1 non-goals as ideas.

- [ ] **Step 1:** Make the edits.
- [ ] **Step 2:** Run `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo build --release -p cosmic-applet-tailscale` — Expected: all pass; note the release binary's size.
- [ ] **Step 3:** Commit `Tailscale: packaging, justfile and docs`.

### Task 9: Live checks

- [ ] **Step 1 (this workstation, read-only):** `--status` against the real daemon; `systemctl`-free daemon-loss check by pointing `TS_SOCKET` at a path that doesn't exist (exit 1, `link Unavailable`).
- [ ] **Step 2 (needs DC):** DC runs `sudo tailscale set --operator=daniel` on the machine being tested. Then, through a small `#[ignore]` test `live_writes` (run with `cargo test -p cosmic-applet-tailscale live_writes -- --ignored`): set `ExitNodeAllowLANAccess` true then back to its old value and assert both replies. It never touches `WantRunning` or the exit node, so a remote session is not cut.
- [ ] **Step 3 (DC's COSMIC machine):** build or install there (`just install`, add the applet in Settings → Desktop → Panel → Applets) and walk the spec's live list: banner before the operator command; switch off/on; pick and clear an exit node; LAN toggle; `systemctl stop tailscaled` and recovery; suspend/resume; `ps` RSS and 60 s CPU, written into the README table.
- [ ] **Step 4:** Fix what turns up, re-run the workspace tests, commit.
