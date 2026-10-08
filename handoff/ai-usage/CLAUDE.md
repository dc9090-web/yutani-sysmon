# CLAUDE.md: AI Usage applet for COSMIC

You're building a COSMIC panel applet in Rust on libcosmic. It shows the Claude subscription's Session (5-hour), Weekly and Fable usage limits, with reset times and pace, using Claude Code's existing login. This folder is the full design and spec handoff. Read these files in this order before writing code:

1. `docs/SPEC.md`: auth rules, endpoint, parsing, refresh, panel, popup, settings, states. **This is the contract.**
2. `docs/DESIGN.md`: visual spec mapped to libcosmic theme accessors and widgets.
3. `docs/ACCEPTANCE.md`: the checks to pass before calling a milestone done.
4. `design/prototype/index.html`: the interactive reference. `design/prototype/bundle.js` holds the reference formatting and pace math.
5. `design/screenshots/`: the target look, dark and light.
6. `tests/fixtures/`: sample endpoint responses and fake Claude homes. Make them your first tests.

This applet is a sibling of the Network Traffic and System Monitor applets. If those repos exist, reuse their panel-button sizing, toggler and segmented-control settings rows, and justfile. YapCap (github.com/TopiCsarno/yapcap, MPL-2.0) is the functional reference for the endpoint. **Read it if you need to, but don't copy its code.**

## Stack

- Rust, edition 2024.
- `libcosmic` from git with features `applet`, `applet-token`, `tokio`, `wayland`. Pin a commit and record it in the README.
- `cosmic-config` for settings.
- `reqwest` (default-features off; `rustls-tls`, `json`), `serde` / `serde_json`, `chrono`, `notify` (credentials file watch).
- `i18n-embed`, `i18n-embed-fl`, `rust-embed`, `tracing`.

## Commands (justfile)

- `just build`, `just check` (clippy `-D warnings` + fmt), `just test`, `just install` (binary, desktop entry and icon under `$PREFIX`, default `/usr/local`).
- `just run`: `RUST_LOG=cosmic_applet_ai_usage=debug cargo run`. To see it, install it and add it via Settings → Desktop → Panel → Applets.
- `just demo`: run with `AI_USAGE_DEMO=1`. This skips auth and the network and cycles through the fixture states every 10 s, so the UI can be reviewed without an account. Gate it behind a cargo feature `demo` so release builds don't include it.

## Rules

- **Never write to, refresh or rotate Claude Code's credentials.** Read only (SPEC §3.3). This is the most important rule in the project.
- **The token never reaches logs, the UI, the clipboard or a panic message.** Use a redacting newtype.
- **One host only: `api.anthropic.com`.** No telemetry; no usage cache on disk.
- **The endpoint is undocumented.** Keep it isolated in `api.rs`. Unknown shapes go to the "Format not recognised" state, never a panic.
- **Theme values come from `theme.cosmic()`.** The quota fill is the user's accent (`cosmic.accent`); the warning and limit colours are `cosmic.warning` / `cosmic.destructive`.
- **Window order is fixed:** Session, Weekly, Fable. The robot icon always comes first in the panel.
- **The panel width never changes as values change.** Use the mono font and fixed-width fields.
- **No I/O in `view`.** Use a scheduler subscription, and a 30 s local tick for countdowns.
- Unit-test `format.rs` (every example in SPEC §6), `api.rs` (every fixture) and `auth.rs` (every fake home).
- `design/` is reference only. Don't ship it, and don't port the HTML or CSS.

## Milestones (stop and report after each)

1. **Skeleton.** The robot shows in the panel; the popup opens and closes; config loads with defaults and enforces the invariant.
2. **Auth + API.** Credentials discovery, scope and expiry checks, the file watcher, the usage request and the parser. All fixture tests pass. Log state transitions at debug level, never tokens.
3. **Scheduler.** Interval with jitter, refresh on popup open, manual refresh with debounce, backoff, Retry-After, reset-time wakeups, and the 30 s local tick.
4. **Panel.** Robot, then the three styles × used/left, plus the reset chunk; horizontal and vertical, XS–XL; levels; stale dimming; tooltip.
5. **Popup.** Header, banners and rows (pace tick, reset text, pace text); the settings page.
6. **States and polish.** Every SPEC §9 state, the demo mode, a11y, i18n, then the ACCEPTANCE pass.

## Open decisions (ask DC; don't guess)

- The final App ID and crate name. `io.github.dc.CosmicAppletAiUsage` is a placeholder.
- The licence.
- At 100% used in **dark** mode, COSMIC's warning (#ffa37d) and destructive (#ffa09a) colours are nearly identical. Ask whether the panel should also show `MAX` and a solid bar at 100%. The design currently only drops the pace tick.
