# CLAUDE.md: System Monitor applet for COSMIC

You're building a COSMIC panel applet in Rust on libcosmic. It shows CPU, AMD GPU, memory and disk I/O. This folder is the full design and spec handoff. Read these files in this order before writing code:

1. `docs/SPEC.md`: data sources, formatting, panel, popup, settings, thresholds. **This is the contract.**
2. `docs/DESIGN.md`: visual spec mapped to libcosmic theme accessors and widgets.
3. `docs/ACCEPTANCE.md`: the checks to pass before calling a milestone done.
4. `design/prototype/index.html`: the interactive reference. `design/prototype/bundle.js` holds the reference formatting and graph math.
5. `design/screenshots/`: the target look, dark and light.

This applet is a sibling of the Network Traffic applet (same author, same design system). If that repo exists, reuse its `format.rs`, graph canvas and panel-button sizing rather than re-deriving them.

## Stack

- Rust, edition 2024.
- `libcosmic` from git with features `applet`, `applet-token`, `tokio`, `wayland`. Pin a commit and record it in the README.
- `cosmic-config` (same git repo) for settings.
- `i18n-embed`, `i18n-embed-fl`, `rust-embed` for strings.
- `tracing` for logs.
- **No** `sysinfo` or other system-stats crates. The spec defines exact sysfs/procfs sources and fallbacks; read them directly.
- Patterns: pop-os/cosmic-applets (`cosmic-applet-time` for a text-width panel button), and libcosmic `examples/applet` for the popup flow.

## Commands (justfile)

- `just build`: `cargo build --release`
- `just check`: `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
- `just test`: `cargo test`
- `just install`: install the binary, the desktop entry and the icon under `$PREFIX` (default `/usr/local`). Do **not** install the udev rule automatically.
- `just run`: `RUST_LOG=cosmic_applet_sysmon=debug cargo run`. To see it, install it and add it via Settings → Desktop → Panel → Applets.

## Rules

- **Theme values come from `theme.cosmic()`.** The metric colours come from `cosmic.palette.accent_indigo` (CPU), `accent_purple` (GPU) and `accent_green` (memory). Disk read/write use `accent_blue` / `accent_orange`. Never use the user's accent for a metric.
- **Metric order is fixed: CPU, GPU, RAM, DISK.** Every colour is paired with its label.
- **Panel width never changes as values change.** Use the mono font and fixed 4-character value fields.
- **No I/O in `view`.** One 1 Hz sampler subscription; cache sysfs paths.
- **Never escalate privileges.** If RAPL is unreadable, show `—`. The udev rule in `resources/` is opt-in, documented in the README only.
- **AMD GPUs only in v1.** Don't add NVML or Intel code paths. Write ideas in `NOTES.md`.
- Unit-test every `format.rs` example in SPEC §4. Test the parsers (`/proc/stat`, `/proc/meminfo`, `/sys/block/*/stat`, hwmon label lookup, `pci.ids` lookup) against fixture files in `tests/fixtures/`.
- `design/` is reference only. Don't ship it, and don't port the HTML or CSS.

## Milestones (stop and report after each)

1. **Skeleton.** The applet runs in the panel; the popup opens and closes; config loads with defaults and enforces the invariant.
2. **Sampler.** CPU, memory and disk readings with tests. Then GPU discovery and readings, then hwmon temps and RAPL. Log at debug level.
3. **Panel.** All metric combinations × three styles, horizontal and vertical, sizes XS–XL; warnings; tooltip.
4. **Popup main page.** Four sections: graphs, meters, stats.
5. **Settings page.** Togglers with the invariant, panel style, disk and GPU pickers; saved and live-applied.
6. **States and polish.** No GPU, unreadable sensors, hysteresis, scrolling on short screens, a11y, i18n, then the ACCEPTANCE pass.

## Open decisions (ask DC; don't guess)

- The final App ID and crate name. `io.github.dc.CosmicAppletSysMon` is a placeholder.
- The licence. Stock COSMIC applets use GPL-3.0-or-later.
- Whether the RAPL udev rule ships in packages or stays a README note.
