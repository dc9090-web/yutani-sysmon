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

- **AI Usage avatars** (handoff update, 2026-10-08): the panel icon can be the Robot or a Cyborg avatar with a session ring. The PNG is picked as the smallest pre-scaled size at least the image diameter × the output scale, so it's only ever scaled down. The ring is a cached canvas that redraws only when the session %, stale flag or theme colours change. DC confirmed the rights to publish the avatar images.
- **AI Usage panel-icon tiles** use caption-size labels: "Cyborg · cyan" wraps at body size in COSMIC's font within a third of the popup.
- **Larger panels (M+)** step AI Usage's text and bars up (labels 11/14, values 14/20, bars 40 × 8, 16 px between chunks), per the updated DESIGN §2c.

- **Weather icons:** DC confirmed with Pixeden (2026-10-08) that the Detailed `wx-pd-*` icons may sit in this public repo. They're still embed-only: never installed or packaged as loose files.
- **Weather `oo7` 0.6** needs Rust 1.92, so the weather crate sets its own `rust-version`. The siblings stay at 1.90.
- **Weather key entry:** Test saves the typed key (keyring, else the 0600 file), then checks it. The field clears once the key is saved, so the key isn't kept in UI state. Test with an empty field rechecks the stored key. Three strings were added for this (`key-placeholder`, `key-placeholder-saved`, `key-shape`).
- **Weather today's range** is widened on the free tier too, not just One Call, so the current temperature always sits inside today's bar.
- **Weather `next` link:** followed only to `https://api.openweathermap.org`. Its `appid` is replaced with ours, and `units` / `lang` are added if missing.
- **Weather alerts:** an alert that returns another status or doesn't parse is skipped, and the rest of the refresh still counts. A 401, 429 or offline error fails the refresh as usual.
- **Weather `lang`:** the UI language is mapped to OpenWeather's codes (`cs`→`cz`, `ko`→`kr`, `lv`→`la`, `pt-BR`→`pt_br`, `zh-TW`→`zh_tw` …). Unsupported languages get `en`.
- **Weather call budget** counts every OpenWeather call, including key checks, searches and free-plan calls, not just billed One Call ones. Refreshes from opening the popup, switching cities or changing units count as automatic, so they stop at 900; only the refresh button goes on to 1,000. The "resumes" time is 00:00 UTC on this machine's clock.
- **Weather 429** pauses every city's requests, not just the one that got it.
- **Weather popup height:** libcosmic caps popups at 1000 px, so the main page's middle and the settings page both scroll within the output height and that cap. The attribution and "Applet settings" always stay visible. (The spec only asked for this under 900 px.)
- **Weather accessibility:** like the siblings, libcosmic is built without `a11y`. The hourly canvas's summary ("Next 24 hours: 15° to 27°, rain chance up to 46% at 15:00") and each day row's ("Fri, light rain, 46% chance of rain, 15° to 27°") are tooltips, and the panel tooltip carries the full summary.
- **Weather "Today"** labels the daily row whose local date is today, not simply the first row. Late at night the free forecast can start tomorrow.
- **Weather settings order** follows SPEC §9.2 (Weather icons, then Units); the handoff screenshot shows them the other way round. The locations caption is the `.ftl`'s "N of 5 saved", without the screenshot's "· the radio picks the panel city".
- **Weather alert descriptions** are clamped to about 4 lines' worth of characters with an ellipsis, since iced text has no line clamp.
- **Weather hourly canvas** fills the width it's given (296 px in the design) rather than a fixed 296, so its last label never spills past the well.
- **Weather demo** shifts the fixtures by whole days, so local times stay as written (sunrise 06:12) while the day names follow today.

## Ideas

- Bits/s option for Network Traffic (open decision in the handoff).
- Ship the RAPL udev rule in distro packages, or keep it a README step (open decision).
- Final App IDs and crate names (placeholders today).
- Weather: confirm the One Call 4.0 fixtures against live calls once a key with the subscription is available (`just live-check`).
- Weather: ACCEPTANCE D-09 expects 5 distinct regions for "Melbourne"; the live API returns two "Victoria, AU" results with different coordinates.
- AI Usage: localise weekday/month names in clock-time resets.
- AI Usage: `just install` copies a bare `Exec=`, which the session panel can't find under `PREFIX=~/.local` (the README has the sed fix).
- A `PKGBUILD` like Yutani's for CachyOS/Arch.
- Per-core CPU grid, fan speeds, disk space, °F: all out of scope for v1.

## Seen on this machine

- `com.system76.CosmicPanelWorkspacesButton` sometimes panics with "Argument list too long" when the panel reloads its applet list. It predates these applets (first seen 19:10 on 2026-10-07) and isn't caused by them.
- **Claude Code 2.1.292 credentials** (checked 2026-10-08) match AI Usage SPEC §3.2. The only extra keys are `claudeAiOauth.rateLimitTier` and `refreshTokenExpiresAt`; the lenient parser ignores them.
- **Weather live check** (2026-10-08, free key): Current Weather 2.5, Forecast 2.5, geocoding and the 401 body all parse, and the fixtures have every field the live responses have. The live responses only add fields the parsers ignore (`rain.3h`, `main.sea_level`, more `local_names`). One Call 4.0 returned 401 without the subscription, so its fixtures are still unchecked. Live "Melbourne" search returns two "Victoria, AU" results with different coordinates.
