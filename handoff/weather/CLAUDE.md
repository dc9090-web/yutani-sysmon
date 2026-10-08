# CLAUDE.md: Weather applet for COSMIC

You're building a COSMIC panel applet in Rust on libcosmic. It shows OpenWeather conditions and forecasts for up to 5 saved cities, using the user's own API key. This folder is the full design and spec handoff. Read these files in this order before writing code:

1. `docs/SPEC.md`: key handling, endpoints (full and free tiers), refresh and quota, icon mapping, formatting, panel, popup, settings, states. **This is the contract.**
2. `docs/DESIGN.md`: visual spec mapped to libcosmic, hourly canvas and daily-row geometry.
3. `docs/ACCEPTANCE.md`: the checks to pass.
4. `design/prototype/index.html`: the interactive reference. `design/prototype/bundle.js` holds the reference mapping and formatting (`wxDetailedClass`, `wxIconClass`, `compass`, `uvWord`, `wxHourlySVG`, `wxDailyHTML`).
5. `design/screenshots/`: the target look.
6. `tests/fixtures/`: sample responses for every endpoint. Make them your first tests, and **verify each against one live call** in milestone 2.

This applet is a sibling of the Network Traffic, System Monitor and AI Usage applets. Reuse their panel-button sizing, settings rows, segmented controls and justfile where they exist.

## Stack

- Rust, edition 2024.
- `libcosmic` from git with features `applet`, `applet-token`, `tokio`, `wayland`. Pin a commit.
- `cosmic-config` for settings; `oo7` for the Secret Service keyring; `reqwest` (rustls-tls, json); `serde` / `serde_json`; `chrono`; `uuid`.
- `i18n-embed`, `i18n-embed-fl`, `rust-embed`, `tracing`.

## Commands (justfile)

- `just build`, `just check` (clippy `-D warnings` + fmt), `just test`.
- `just install`: binary and desktop entry under `$PREFIX`. **No icon files are installed**: the Detailed icons are embedded, and the applet-list icon is the system `weather-few-clouds-symbolic`.
- `just run`: `RUST_LOG=cosmic_applet_weather=debug cargo run`.
- `just demo`: `WEATHER_DEMO=1`. This loads `tests/fixtures/` instead of the network and cycles through the states every 10 s (feature `demo`, off in release builds).

## Rules

- **The API key lives only in the keyring** (fallback: a 0600 file). Never put it in config, logs, the UI or panics. Redact `appid=`.
- **The Pixeden icons (`resources/icons/weather/`) are embed-only.** Use `include_bytes!`; never install or package the files. Read that folder's README. The public source release is blocked on DC's licence confirmation.
- **Use the location's `timezone_offset` for every local time and day**, never the machine's time zone.
- **Respect the quota.** Scheduled refreshes go to the panel location only; follow the 1h `next` link once; cache alerts by ID; stop scheduled refreshes at 900 calls a day.
- **Attribution "Weather data: OpenWeather" is mandatory** and always visible.
- **Theme values come from `theme.cosmic()`.** `wx-temp` = `palette.accent_orange`, `wx-rain` = `palette.accent_blue`. Never colour-code conditions.
- **One icon set at a time** (Detailed or System); the mapping is exactly SPEC §6.
- **No I/O in `view`.** Fetch in tasks or subscriptions, and keep the cache in memory with disk persistence.
- Unit-test `format.rs`, the icon mapping, each parser (`onecall4`, `free25`, `geocoding`) and the free-tier daily aggregation against the fixtures.
- `design/` is reference only. Don't ship it, and don't port the HTML or CSS.

## Milestones (stop and report after each)

1. **Skeleton.** The applet shows in the panel; the popup opens and closes; config loads with defaults.
2. **Key + API.** Keyring storage, Test, tier detection; One Call 4.0, free 2.5 and geocoding parsers with fixture tests; then a live check of each endpoint, with any fixture differences reported.
3. **Scheduler + cache.** Interval with jitter, panel-only refresh, popup-open refresh, manual debounce, the call counter and daily cap, backoff, the disk cache.
4. **Panel.** Icon (both sets) and temperature, city, high/low, the alert dot, sizes S–XL and vertical, stale state, tooltip.
5. **Popup.** Location header and switcher, current, alerts, hourly canvas, daily rows, details, attribution; the settings page (locations and search, icon set, units, panel toggles, refresh, key).
6. **States and polish.** Every SPEC §10 state, demo mode, a11y, i18n, scrolling on short screens, then the ACCEPTANCE pass.

## Open decisions (ask DC; don't guess)

- The final App ID and crate name. `io.github.dc.CosmicAppletWeather` is a placeholder.
- The licence.
- **Pixeden icons:** DC confirms redistribution with Pixeden before any public release. Until then, keep the repo private, or move `resources/icons/weather/` out of the public tree and default `icon_set` to System.
