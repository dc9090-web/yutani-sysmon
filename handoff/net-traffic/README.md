# Network Traffic applet: design and spec package

This is the handoff for building a COSMIC panel applet that shows live download and upload rates for one network adapter, as numbers, a graph, or both.

## Using it with Claude Code

1. Copy this folder into a new, empty repo (or unzip it there).
2. Start Claude Code in the repo. It reads `CLAUDE.md` automatically.
3. Prompt: *"Read CLAUDE.md and the docs, then do milestone 1."* Review the result, then continue one milestone at a time.

## Contents

| Path | What it is |
|---|---|
| `CLAUDE.md` | Build instructions, stack, rules, milestones, open decisions |
| `docs/SPEC.md` | Functional spec: data sources, formatting, panel, popup, settings, states |
| `docs/DESIGN.md` | Visual spec: tokens mapped to libcosmic accessors, geometry, graph rules, contrast |
| `docs/ACCEPTANCE.md` | Test checklist for sign-off |
| `design/prototype/` | Interactive HTML reference. Open `index.html`; the top bar switches pages and theme. `bundle.js` holds the reference formatting and graph math. |
| `design/screenshots/` | Target renders of each screen, dark and light, at 2× |
| `design/tokens.json` / `tokens.css` | libcosmic stock dark and light values (reference only) |
| `design/icons/` | The icons used, copied from pop-os/cosmic-icons (CC BY-SA 4.0), plus the two custom icons |
| `resources/` | Files that ship: desktop entry, applet icon, custom `net-*-bar-symbolic` icons |
| `i18n/en/*.ftl` | Every user-visible string |

## Where the values come from

- Colours come from libcosmic `main` at commit 60ad2cc (2026-10-06), using `Theme::dark_default()` and `Theme::light_default()`.
- Spacing and radii come from libcosmic's defaults; type sizes from `widget/text.rs`; popup sizes from `applet/mod.rs`.
- Panel sizes come from `cosmic-panel-config`, and the desktop entry keys from `cosmic-applet-network` 1.10.

These are a snapshot. At runtime the applet reads the live theme.
