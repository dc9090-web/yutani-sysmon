# Weather applet: design and spec package

This is the handoff for building a COSMIC panel applet that shows OpenWeather conditions and forecasts for up to 5 saved cities: the panel city's icon and temperature, and a popup with current conditions, alerts, the next 24 hours, 7 days and details.

## Using it with Claude Code

1. Copy this folder into a new, **private** repo (see the icon licence below).
2. Start Claude Code in it. It reads `CLAUDE.md` automatically.
3. Prompt: *"Read CLAUDE.md and the docs, then do milestone 1."* Continue one milestone at a time.

## Before you build: OpenWeather setup

1. Create an account and an API key at openweathermap.org → API keys. New keys can take up to 2 hours to activate.
2. **For the full forecast** (hourly, 7-day, UV, alerts), subscribe to **One Call by Call** (One Call API 4.0). It includes 1,000 free calls a day, then charges per call. Sign-up needs a card.
3. **Set the daily call limit to 1,000** in your OpenWeather billing settings, so the account can never be charged. The applet also stops at 900 a day on its own.
4. Without the subscription, a plain free key still works in **free mode**: 3-hour steps, 5 days, no UV or alerts.

## Contents

| Path | What it is |
|---|---|
| `CLAUDE.md` | Stack, rules, milestones, open decisions |
| `docs/SPEC.md` | Key storage, endpoints for both tiers, quota, icon mapping, formatting, panel, popup, settings, states |
| `docs/DESIGN.md` | Colours mapped to libcosmic, type, panel, hourly canvas and daily-row geometry |
| `docs/ACCEPTANCE.md` | Sign-off checklist |
| `tests/fixtures/` | Sample responses: One Call 4.0, free 2.5, geocoding, errors (built from the docs; verify live) |
| `design/prototype/` | Interactive HTML reference. Open `index.html`; the top bar switches pages and theme. |
| `design/screenshots/` | Target renders, dark and light, at 2× |
| `resources/icons/weather/` | The 30 Detailed (Pixeden) icons, **embed-only**, with the licence |
| `resources/*.desktop` | Desktop entry (uses the system icon `weather-few-clouds-symbolic`) |
| `i18n/en/*.ftl` | Every user-visible string |

## Icon licence

The Detailed icons come from the Pixeden "Weather App Icons" pack. Its licence allows personal and commercial use, but **not redistributing the files**. The spec embeds them in the binary.

**Don't publish this repo publicly** until you've confirmed with Pixeden. The fallback is the System icon set (COSMIC's own), which needs no files.

## Where the values come from

The design values come from libcosmic `main` (commit 60ad2cc, 2026-10-06). The API facts come from OpenWeather's documentation for One Call 4.0, the 3.0→4.0 migration notes, pricing, geocoding and condition codes (read October 2026).
