# Notes

Ideas and known limits kept out of v1 (the handoffs say: don't add features that aren't in SPEC.md; write them here).

## Decisions taken during the build

- **Three applets, one workspace.** Each applet is its own binary, desktop entry, App ID and config. `crates/common` holds the code both use (formatting, ring buffers, sysfs readers, the graph canvas, theme inks, popup pieces), so format and graph math exist once.
- **No `wgpu`.** libcosmic is built without it, so the applets render with tiny-skia and hold no GPU context.
- **No `a11y` feature.** It pulls in AccessKit/AT-SPI per applet. The tooltip carries the accessible summary text from the `.ftl` files. Turning it on later is a one-line feature change.
- **Sampling runs in `update` on the 1 Hz tick**, not on a separate worker. With the single-thread executor they'd be the same thread anyway, and the reads are tiny. `view` never does I/O.
- **Popup-only readings** (CPU clock, GPU VRAM/clock/power, NVMe temp) are read only while the popup is open, plus once as it opens.
- **Runtime-suspended dGPUs** are detected via `power/runtime_status` and never woken by sensor reads. This is an efficiency guard, not a feature.
- **GPU names** come from libdrm's `amdgpu.ids` (device + revision, exact) before `pci.ids` (often just the family, e.g. "RX 9070/9070 XT/9070 GRE").
- **Panel values are centred** under their labels (after DC's review), still inside a fixed 4-character cell so the width never changes.
- **Package power without RAPL access** shows `—` with a plain "Package power" key; the reason goes to the log once, and the README documents the opt-in udev rule.
- **Segmented controls** use `space_xxxs` side padding so "✓ Numbers" fits a third of the 312 px content width.

- **AI Usage at 100 %** shows `MAX` and a solid bar in the panel (DC's call), because COSMIC's dark warning and destructive colours are nearly identical. The popup keeps the real bar and says "Limit reached".
- **AI Usage diagnostics** keep only the JSON key paths of an unrecognised response, not the raw body, so no usage values are ever held for copying.
- **AI Usage timing** arms one timer for the next fetch; the 30 s clock tick also fetches if that time has passed, since tokio timers stall during suspend.
- **AI Usage clock-time text** uses English weekday/month abbreviations and AM/PM from code, not the `.ftl` file; every other string is in the `.ftl` file (a test checks every `fl!` key exists).
- **AI Usage accessibility:** like the siblings, libcosmic is built without `a11y`, so the meters aren't exposed to AT-SPI. The tooltip carries the full summary ("Session 42% used, resets in 2h 13m; …"), and levels are always stated in words too.

## Ideas

- Bits/s option for Network Traffic (open decision in the handoff).
- Ship the RAPL udev rule in distro packages, or keep it a README step (open decision).
- Final App IDs and crate names (placeholders today).
- AI Usage: localise weekday/month names in clock-time resets.
- AI Usage: `just install` copies a bare `Exec=`, which the session panel can't find under `PREFIX=~/.local` (the README has the sed fix).
- A `PKGBUILD` like Yutani's for CachyOS/Arch.
- Per-core CPU grid, fan speeds, disk space, °F: all out of scope for v1.

## Seen on this machine

- `com.system76.CosmicPanelWorkspacesButton` sometimes panics with "Argument list too long" when the panel reloads its applet list. It predates these applets (first seen 19:10 on 2026-10-07) and isn't caused by them.
- **Claude Code 2.1.292 credentials** (checked 2026-10-08) match AI Usage SPEC §3.2. The only extra keys are `claudeAiOauth.rateLimitTier` and `refreshTokenExpiresAt`; the lenient parser ignores them.
