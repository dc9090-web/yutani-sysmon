# System Monitor applet: design and spec package

This is the handoff for building a COSMIC panel applet that shows CPU, AMD GPU, memory and disk I/O, as labelled numbers, sparklines, or both, with a detailed popup.

## Using it with Claude Code

1. Copy this folder into a new, empty repo.
2. Start Claude Code in it. It reads `CLAUDE.md` automatically.
3. Prompt: *"Read CLAUDE.md and the docs, then do milestone 1."* Continue one milestone at a time.

## Contents

| Path | What it is |
|---|---|
| `CLAUDE.md` | Stack, rules, milestones, open decisions |
| `docs/SPEC.md` | Exact procfs/sysfs sources and fallbacks, formatting, panel, popup, settings, thresholds, states |
| `docs/DESIGN.md` | Colours mapped to libcosmic palette accessors, type, geometry, graph rules |
| `docs/ACCEPTANCE.md` | Sign-off checklist, including comparisons against `btop`, `amdgpu_top`, `iostat` and `turbostat` |
| `design/prototype/` | Interactive HTML reference. Open `index.html`; the top bar switches pages and theme. |
| `design/screenshots/` | Target renders, dark and light, at 2× |
| `design/tokens.json` / `tokens.css` | libcosmic stock values (reference only) |
| `resources/` | Desktop entry, applet icon, and an **optional** RAPL udev rule |
| `i18n/en/*.ftl` | Every user-visible string |

## Package power (RAPL)

CPU package power reads `/sys/class/powercap/intel-rapl:0/energy_uj`. Since CVE-2020-8694, that file is root-only. The applet never escalates privileges; without access, it shows `—`. `resources/60-cosmic-applet-sysmon-rapl.rules` makes the file world-readable. That re-opens the power side-channel on that machine, so it's opt-in.

## Where the values come from

The design values come from libcosmic `main` at commit 60ad2cc (2026-10-06). At runtime, the applet follows the live theme.
