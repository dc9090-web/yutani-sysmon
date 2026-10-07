# CLAUDE.md: Network Traffic applet for COSMIC

You're building a COSMIC panel applet in Rust on libcosmic. This folder is the full design and spec handoff. Read these files in this order before writing code:

1. `docs/SPEC.md`: behaviour, data sources, settings, states. **This is the contract.**
2. `docs/DESIGN.md`: visual spec mapped to libcosmic theme accessors and widgets.
3. `docs/ACCEPTANCE.md`: the checks you must pass before calling a milestone done.
4. `design/prototype/index.html`: the interactive reference. `design/prototype/bundle.js` holds the reference formatting and graph math (`formatRate`, `formatCompact`, `niceMax`, `drawGraph`).
5. `design/screenshots/`: the target look, dark and light.

## Stack

- Rust, edition 2024.
- `libcosmic` from git (`https://github.com/pop-os/libcosmic`), with features `applet`, `applet-token`, `tokio`, `wayland`. Pin a commit in `Cargo.toml` and record it in the README.
- `cosmic-config` from the same libcosmic git repo, for settings.
- `i18n-embed`, `i18n-embed-fl`, `rust-embed` for Fluent strings (`i18n/en/*.ftl`).
- `tracing`, `tracing-subscriber` for logs. No `println!`.
- Follow the structure of pop-os/cosmic-applets (e.g. `cosmic-applet-network`, `cosmic-applet-time`) and libcosmic's `examples/applet` for the popup flow.

## Commands

Create a `justfile` with these recipes:
- `just build`: `cargo build --release`
- `just check`: `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
- `just test`: `cargo test`
- `just install`: install the binary to `/usr/local/bin` (or `$PREFIX`), the desktop entry to `share/applications`, and icons to `share/icons/hicolor`
- `just run`: `RUST_LOG=cosmic_applet_net_traffic=debug cargo run`. An applet only renders inside the panel; to test, install it and add it via Settings → Desktop → Panel → Applets.

## Rules

- **Theme values come from `theme.cosmic()`.** Never hard-code hex colours, spacing or radii. The only fixed colours are the series colours, and even those come from `cosmic.palette.accent_blue` / `accent_orange`.
- **Never put the direction indicator in the same string as the number.** Each rate line is a row of 3 fixed-width cells: indicator | number | unit (SPEC §5).
- **Panel width never changes as values change.** Use fixed-width cells and the mono font.
- **No I/O in `view`.** Sampling happens in a subscription; `view` only formats.
- **Every user-visible string goes through `fl!()`.**
- **Don't add features that aren't in SPEC.md.** Write ideas in `NOTES.md` instead.
- Unit-test `format.rs` against every example in SPEC §4, and route parsing with fixture files.
- The custom icons in `resources/icons` ship with the applet. The other icons are system icons loaded by name.
- `design/` is reference only. Don't ship it, and don't port the HTML or CSS.

## Milestones (stop and report after each)

1. **Skeleton.** The applet runs in the panel and shows a static icon; the popup opens and closes; config loads with defaults.
2. **Data.** Interface enumeration, the 1 Hz sampler, ring buffers, default route, `format.rs` with tests. Log rates at debug level.
3. **Panel.** All three modes and all six indicators, horizontal and vertical, sizes XS–XL. Tooltip.
4. **Popup main page.** Header, readouts, graph canvas, totals, menu rows.
5. **Settings page.** Mode, indicator, adapter; saved and live-applied.
6. **States and polish.** Every state in SPEC §8, a11y, i18n, then the ACCEPTANCE pass.

## Open decisions (ask DC; don't guess)

- The final App ID and crate name. `io.github.dc.CosmicAppletNetTraffic` is a placeholder.
- The licence. The stock COSMIC applets are GPL-3.0-or-later.
- Whether to add a bits/s option. It's out of scope for v1.
