# Weather applet: design spec

The source of truth is `design/tokens.json`, which holds libcosmic's stock theme values. `design/prototype/` renders every screen.

**Rule 1:** read values from `theme.cosmic()` at runtime. The hex values below are for checking screenshots only.

## 1. Colour

The shared UI tokens (`bg`, `on-bg`, `on-bg-muted`, `bg-component`, dividers, accent, `warning`, `success`, `destructive`) come from `design/tokens.json`. These are the Weather tokens:

| Token | Dark | Light | libcosmic | Use |
|---|---|---|---|---|
| `wx-temp` | #ffad00 | #624000 | `cosmic.palette.accent_orange` | Hourly temperature line; 7-day range segment |
| `wx-rain` | #63d0df | #00525a | `cosmic.palette.accent_blue` | Rain % text |
| `wx-rain-fill` | wx-rain @ 25% | wx-rain @ 20% | derived alpha | Hourly rain bars |
| `wx-now` | #e7e7e7 | #121212 | `cosmic.background.on` | "Now" dot on the hourly line and today's range bar |
| `meter-track` | #2e2e2e | #f5f5f5 | `cosmic.background.component.base` | Range bar track |

- Weather icons are single ink in `background.on`; muted (`on-bg-muted`) when the data is stale.
- **Never colour-code conditions or temperatures:** no blue-to-red gradients, no tinted icons. The icon, the description and the shared range scale carry the meaning.
- Contrast: `wx-temp` and `wx-rain` are both ≥ 6:1 on `bg` in both themes (checked).

## 2. Type

| Style | Size / line | Weight | Font | Use |
|---|---|---|---|---|
| wx-temp-hero | 40/48 | 300 | sans | Current temperature in the popup |
| heading | 14/21 | 700 | sans | City in the header; settings title |
| body | 14/21 | 400 | sans | Description; day names; location rows |
| caption / caption-heading | 12/17 | 400 / 600 | sans | Region, feels-like, section headings, stat keys, attribution |
| monotext | 14/20 | 400 | mono | Panel temp (S); H/L; stats; daily low/high |
| panel temp M/L, XL | 16/22, 20/28 | 400 | mono | Larger panels |
| hourly labels | 12 (temps), 11 (hours), 10 (rain %) | 400 | mono / sans / mono | Hourly canvas |

## 3. Panel

```text
 ╭──────────────╮   ╭────────────────────╮   ╭───────────────────────╮
 │ [☁•]  23°    │   │ [☁]  Sydney        │   │ [☁]  23°   27°        │
 ╰──────────────╯   │      23°           │   │            15°        │
   icon+temp        ╰────────────────────╯   ╰───────────────────────╯
   (• = alert dot)    show_city                show_hilo
```

- The icon is at the symbolic icon size (S 20, M 28, L 32, XL 48), with `space_xxs` gap.
- The temperature field is a fixed 4 characters, so the width never changes as the temperature changes.

## 4. Hourly canvas (296 × 112)

- **Temperature band:** y 22–66. Scale to the 24 h min–max, with a span of at least 4°. A 2px `wx-temp` polyline with round joins and caps.
- **Now:** a 3.5px-radius `wx-now` dot on the first point.
- **Labels every 3 h** (index 0, 3, 6 … 21):
  - The temperature (mono 12, `on-bg`), 8px above the point, centred.
  - The hour (sans 11, muted) at y ≈ 110: "Now" first, then local `HH` (or `9p` / `12a` in 12-hour mode).
- **Rain bars:** an 18px band ending at the baseline (y 96). Each bar is 9px wide with radius 1.5, height = pop × 18, in `wx-rain-fill`. When pop ≥ 20%, a label (mono 10, `wx-rain`) sits above the bar at y ≈ 76.
- **Baseline:** 1px `component-divider` at y 96.
- **Free tier:** 9 points over the same width; label every point.
- No grid, no smoothing, no animation.
- **Accessible name:** "Next 24 hours: 15° to 27°, rain chance up to 46% at 15:00".

## 5. Daily rows

```text
Today  [☁]        18°  ▭▭▬▬▬▬▬▬●▭  27°
Fri    [🌧]   46%  15°  ▭▬▬▬▬▬▬▬▭▭  27°
│3.4em│20px│ 3em │2.8ch│   1fr    │2.8ch│     row height 32, gap space_xxs
```

- **Range scale:** from the week's lowest min to its highest max. The `wx-temp` segment runs from the day's min to its max on a 6px `meter-track` track with `radius_xl` ends.
- **Today:** a 10px `wx-now` dot at the current temperature, clamped to the segment, with a 2px `bg` halo.
- **Rain %:** only when ≥ 20%, in mono 12 `wx-rain`.

## 6. Popup layout (360 wide, content 312)

```text
┌────────────────────── 360 ──────────────────────┐
│ Sydney ⌄                      Updated 4m ago  ⟳ │ header
│ New South Wales, AU                             │
│ [⛅48]  27°                               H 27° │ current
│         Partly cloudy                     L 15° │
│         Feels like 26°                          │
│ [⚠ Severe Thunderstorm Warning              ]   │ alert (if any)
│ [  Bureau of Meteorology · 12:00–19:00      ]   │
│ ─────────────────────────────────────────────── │
│ Next 24 hours                                   │
│ [ hourly canvas 296 × 112 ]                     │
│ ─────────────────────────────────────────────── │
│ 7-day forecast                                  │
│ Today … Wed (7 rows)                            │
│ ─────────────────────────────────────────────── │
│ Wind ◎ 15 km/h SW   Humidity 💧62%   UV 6 High  │ details 3 × 2
│ Sunrise 06:12       Sunset 18:47   1016 hPa     │
│ ─────────────────────────────────────────────── │
│ ⚙ Applet settings                             › │
│ Weather data: OpenWeather                       │ attribution (required)
└─────────────────────────────────────────────────┘
```

The total is about 820px. If the output's logical height is under 900px, wrap everything between the header and "Applet settings" in a `scrollable`.

## 7. Icons

See `design/prototype/icon-sets.html` and `screenshots/icon-sets-*.png` for every condition in both sets at 40, 20 and 16px.

- The **Detailed** icons are outline drawings and look lighter than COSMIC's solid System icons at 16px. Don't add a stroke to compensate; use them as drawn.
- **Detailed coverage gaps:** fog and mist use the wind-lines icon; sleet uses snow.

## 8. Screens

`design/screenshots/` contains the panel variants, popup (metric, imperial, System icons, alert), settings, locations search, icon sets and states, each in dark and light.
