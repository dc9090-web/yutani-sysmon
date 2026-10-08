# Weather applet: functional spec

Version 1.0 · target: COSMIC panel (libcosmic `main`, applet API as in pop-os/cosmic-applets 1.10). This is a sibling of the Network Traffic, System Monitor and AI Usage applets, and shares their design system.

## 1. Scope

A COSMIC panel applet that shows the weather for up to 5 saved cities, using **OpenWeather** with the user's own API key.

- **Panel:** icon and temperature for one chosen city.
- **Popup:** current conditions, alerts, the next 24 hours, a 7-day forecast and details.

**Out of scope for v1:**
- Radar and maps
- Air quality
- Minute-by-minute precipitation
- Notifications
- Automatic location detection (GeoClue)
- History
- Other providers

## 2. Identity

| Item | Value |
|---|---|
| App ID | `io.github.dc.CosmicAppletWeather` (placeholder; change before publishing) |
| Binary / crate | `cosmic-applet-weather` |
| Desktop entry | `resources/io.github.dc.CosmicAppletWeather.desktop` (`Icon=weather-few-clouds-symbolic`, a system icon, so nothing is installed) |
| Weather icons | **Detailed:** `resources/icons/weather/wx-pd-*-symbolic.svg`, **embedded only** (see the README there). **System:** COSMIC `weather-*-symbolic`, loaded by name. |
| Strings | `i18n/en/cosmic_applet_weather.ftl` |

## 3. API key

- **Entry:** a password-style `text_input` in Settings, with a **Test** button. Trim whitespace. A key is 32 hex characters; warn on a different shape, but still allow testing.
- **Storage:** the Secret Service keyring via the `oo7` crate, under attribute `application = <APP_ID>`, label "OpenWeather API key".
  - If no Secret Service is available, fall back to `$XDG_CONFIG_HOME/<APP_ID>/api-key` with mode **0600**, and show the caption `key-stored-file`.
  - **Never** put the key in cosmic-config, logs, URLs shown in the UI, error strings or the clipboard.
  - Redact `appid=…` in every logged URL.
- **Tier detection** (on Test, at startup, and after a 401):
  1. `GET /data/4.0/onecall/current?lat=0&lon=0&appid=KEY` returns 200 → **Full** (One Call 4.0).
  2. If not, `GET /data/2.5/weather?lat=0&lon=0&appid=KEY` returns 200 → **Free**.
  3. Both return 401 → **Invalid**. The caption mentions that new keys can take up to 2 hours to activate.
  4. Network errors → **Unknown**. Keep the last known tier.
- **Status line:** "Key works · One Call 4.0" (success) / "Key works · free plan (limited)" (warning) / "Key not accepted" (destructive) / "Couldn't check: offline" (muted).

## 4. Data sources

Base URL `https://api.openweathermap.org`. Send `units=metric` or `imperial` and `lang=<locale, OWM code>`.

### 4.1 Full tier: One Call 4.0 (needs the "One Call by Call" subscription)

Each refresh of one location makes these calls:

| Call | Endpoint | Use |
|---|---|---|
| 1 | `/data/4.0/onecall/current?lat&lon` | `data[0]`: temp, feels_like, pressure, humidity, uvi, wind_speed, wind_gust, wind_deg, sunrise, sunset (may be absent), `weather[0]` id/description/icon, and `alerts[]` (alert IDs) |
| 2–3 | `/data/4.0/onecall/timeline/1h?lat&lon` | 20 records per page. Follow `next` **once** and keep the first 24 points: dt, temp, pop, `weather[0]`. |
| 4 | `/data/4.0/onecall/timeline/1day?lat&lon` | 10 records; show 7: dt, temp.min, temp.max, pop, `weather[0]` |
| + | `/data/4.0/onecall/alert/{id}` | Only for alert IDs not already cached. Cache them by ID until their `end`. |

- **Local time:** use the top-level `timezone_offset` (seconds) for every local time, day name and hour label. Never use the machine's time zone.
- **Today's min/max:** widen them to include the hourly temperatures for today's local date, so the current temperature always falls within today's range.

### 4.2 Free tier: Current Weather 2.5 + 5-day/3-hour Forecast 2.5

| Call | Endpoint | Use |
|---|---|---|
| 1 | `/data/2.5/weather?lat&lon` | `main.temp`, `feels_like`, `pressure`, `humidity`, `wind.*`, `sys.sunrise` / `sunset`, `weather[0]`, `timezone` (offset) |
| 2 | `/data/2.5/forecast?lat&lon` | `list[]`, 40 × 3 h. **Hourly:** the first 9 points (24 h). **Daily:** group by **local** date (using `city.timezone`): min of `temp_min`, max of `temp_max`, max `pop`, and the icon and id of the entry nearest local 13:00. Show 5 days. |

There's no UV and no alerts in free mode. Show `—` for UV and the `free-plan-note` caption.

### 4.3 Geocoding (both tiers)

`GET /geo/1.0/direct?q=<query>&limit=5`:
- Each result gives the name, `local_names[<lang>]` (prefer it when present), lat, lon, country (ISO 3166) and state.
- **Region text:** `"<state>, <country>"`, or `"<country>"` alone.
- **Search behaviour:** debounce 400 ms, minimum 2 characters, cancel in-flight requests when the query changes.

### 4.4 HTTP

- `reqwest` (rustls, json). 5 s connect timeout, 15 s total.
- A single in-flight refresh per location. Follow no redirects to other hosts.
- `User-Agent: cosmic-applet-weather/<version>`.

## 5. Refresh and quota

- **Schedule:** setting `refresh_minutes` ∈ {15, **30**, 60}, with ±10% jitter. On schedule, refresh **only the panel location**.
- **Other locations:** fetched when shown in the popup, if their cache is older than the interval.
- **On popup open:** refresh the viewed location if its data is older than 10 minutes.
- **Manual refresh button:** disabled while a request is in flight and for 30 s afterwards.
- **Call budget (full tier):** 4 calls per refresh. With the 30-minute default that's about 200 calls a day, against **1,000 free a day**.
  - Count calls per UTC day in memory (persisted with the cache).
  - At **900**, stop scheduled refreshes until 00:00 UTC and show the "Daily call limit reached" caption. Manual refresh is still allowed up to 1,000.
  - Tell the user, in the README and in the Settings caption, to set their OpenWeather billing **daily limit to 1,000** so they can never be charged.
- **Errors:**

| Response | Action |
|---|---|
| 401 | Re-run tier detection once. If still invalid, go to the *Key not accepted* state and stop polling until the key changes. |
| 429 | Back off for `Retry-After` (default 10 min). Header text: "Rate limited · retry in N m". |
| 5xx, timeout, DNS | *Offline*. Retry with backoff of 1, 2, 4… minutes, capped at the interval. |

- **Cache:** store the last successful snapshot per location at `$XDG_CACHE_HOME/<APP_ID>/<location-id>.json`. It holds weather data only, never the key. On startup, show the cached data with "Updated Nm ago" and refresh immediately. Discard cached data older than 12 h.

## 6. Icons

Setting `icon_set` ∈ {**Detailed**, System}. One set applies everywhere; never mix them.

**Detailed mapping** (`wx-pd-<name>-symbolic`). Inputs: the condition `id`, the icon code (`NNd` / `NNn`, where `n` means night), and the wind in **m/s** (convert from mph in imperial mode before comparing).

1. If wind ≥ 10.8 m/s and the code is 01–04: use `windy-day` / `windy-night`, or `windy` for 04.
2. Otherwise, by condition id:

| id | Icon |
|---|---|
| 200–202, 230–232 | `storm-rain` |
| other 2xx | `storm-day` / `storm-night` |
| 3xx | `drizzle-day` / `drizzle-night` |
| 500–501 | `rain-day` / `rain-night` |
| 502–504 | `heavy-rain` |
| 511, 600–602, 611–616 | `snow` |
| 520–531 | `heavy-rain-day` / `heavy-rain-night` |
| 620–622 | `snow-day` / `snow-night` |
| 7xx | `windy` |
| 800 | `clear-day` / `clear-night` |
| 801–802 | `partly-cloudy-day` / `partly-cloudy-night` |
| 803–804 | `cloudy` |

3. With no id, fall back by icon code: 01 → clear, 02/03 → partly-cloudy, 04 → cloudy, 09 → heavy-rain-day/night, 10 → rain-day/night, 11 → storm-day/night, 13 → snow-day/night, 50 → windy.

**System mapping** (COSMIC, by icon code):

| Code | Icon |
|---|---|
| 01 | `weather-clear` (night: `weather-clear-night`) |
| 02, 03 | `weather-few-clouds` (night: `weather-few-clouds-night`) |
| 04 | `weather-overcast` |
| 09 | `weather-showers` |
| 10 | `weather-showers-scattered` |
| 11 | `weather-storm` |
| 13 | `weather-snow` |
| 50 | `weather-fog` |

`design/prototype/bundle.js` holds the reference implementation (`wxDetailedClass`, `wxIconClass`). Port it and unit-test every row against the fixtures.

**Extras (Detailed only):** `humidity` before the Humidity value; `compass` before the Wind value, rotated to `wind_deg + 135°` (the glyph's needle points NE). The System set uses a drawn arrow rotated to `wind_deg + 180°`.

## 7. Formatting

| Value | Metric | Imperial |
|---|---|---|
| Temperature | `round(t)°`, no unit letter | Same (the API returns °F) |
| Wind | m/s × 3.6 → `15 km/h` | mph → `9 mph` |
| Compass | 16-point (`N`, `NNE` … `NNW`) from `wind_deg` | Same |
| Pressure | `1016 hPa` | hPa × 0.02953 → `30.00 inHg` |
| UV | `round(uvi)` + word: Low 0–2, Moderate 3–5, High 6–7, Very high 8–10, Extreme 11+ | Same |
| Times | Location-local `HH:MM`, or `h:MM AM` if the COSMIC time applet's `military_time = false` | Same |
| Rain chance | `round(pop × 100)%`, shown only when ≥ 20% | Same |
| Panel field | Right-aligned in 4 characters: `"  9°"`, `" 23°"`, `"-12°"` | Same |

The description comes from `weather[0].description` (already localised via `lang`), sentence-cased.

## 8. Panel button

- **Build:** `button::custom(row![icon, temp, hilo?]).class(Button::AppletIcon)` inside `core.applet.autosize_window`.
- **Icon:** at `suggested_size(true)`, `.symbolic(true)`.
- **Temperature:** `text::monotext` 14/20 in the 4-character field. On panels M/L use 16/22; on XL use 20/28.
- **`show_city`:** `column![city (10/12, 600, muted, max 10 characters with ellipsis), temp 12/17]`.
- **`show_hilo`:** today's high over low, stacked, mono 10/12, muted.
- **Alert:** a 7px warning dot at the icon's top-right, with a 2px background halo.
- **Stale data** (> 2 × interval old, or offline): the temperature turns muted.
- **No key or no locations:** the icon `weather-few-clouds-symbolic` with no temperature; the tooltip explains why.
- **Tooltip and accessible name:** `"<City>: 23°, Partly cloudy · H 27° L 15°"`, plus `" · <alert event>"`.
- **Click:** toggles the popup.

## 9. Popup

`popup_container`, 360px wide. Two pages, Main and Settings; the popup always opens on Main, showing the panel city.

### 9.1 Main page

1. **Location header.**
   - The city (`heading`) with `go-down-symbolic`, over the region (`caption`), as one button. It opens an inline list of the saved locations (icon, name, temperature, and "· panel" on the panel city). Choosing one changes **only the popup view**.
   - On the right: freshness text and a refresh icon button.
2. **Current conditions.**
   - A 48px icon.
   - The temperature (Open Sans 300, 40/48), the description (`body`), and "Feels like N°" (`caption`).
   - H/L right-aligned in mono 14/20; the low is muted.
3. **Alert banners** (full tier): one per active alert, most recent `start` first.
   - The warning icon in `warning`, the event (600), and "<sender> · HH:MM–HH:MM".
   - Clicking toggles the description (first `description[]` matching the UI language, else the first entry), clamped to 4 lines.
4. "Next 24 hours" heading, then the **hourly canvas**: 296 × 112 in a `bg-component` well with `radius_s`. See DESIGN §4.
5. "7-day forecast" heading ("5-day forecast" in free mode), then the **daily rows** (DESIGN §5).
6. **Details grid** (3 × 2): Wind, Humidity, UV index, Sunrise, Sunset, Pressure.
7. Inset divider, then `menu_button` ⚙ "Applet settings" ›.
8. **Attribution caption** "Weather data: OpenWeather", linking to `https://openweathermap.org`. **Required by OpenWeather; never hide it.**

### 9.2 Settings page

1. Back button and "Applet settings".
2. **Locations:**
   - Rows: radio (panel city), icon, name and region (+ "· in panel"), temperature, and a delete icon button. Delete is disabled on the last remaining location.
   - The caption "N of 5 saved".
   - The search input "Add a city", with up to 5 results. Each result shows `list-add-symbolic`, the name, and "region · lat, lon" (2 decimals).
   - Choosing a result appends the city; it's ignored if within 0.05° of a saved one (caption "Already saved").
   - At 5 saved, hide the search with the caption "5 of 5 saved · remove one to add another".
3. **Weather icons:** a segmented control, Detailed / System, with a caption.
4. **Units:** a segmented control, "Metric · °C, km/h" / "Imperial · °F, mph". Changing it refetches immediately.
5. **Panel:** togglers "Show city name" and "Show high / low".
6. **Refresh every:** 15 / 30 / 60 min.
7. **OpenWeather API key:** the masked input, Test button, status line, and the caption "Set your OpenWeather daily limit to 1,000 in billing to stay free."

## 10. States

| State | Trigger | Panel | Popup |
|---|---|---|---|
| No API key | No key stored | Default icon, no temp | Banner "Add an OpenWeather API key" + "Open settings" |
| Key not accepted | 401 after re-detection | Default icon | Banner "API key not accepted" + activation note + "Open settings" |
| No locations | Empty list | Default icon | Banner "Choose a location" + "Add location" |
| Free plan | Tier = Free | Normal | 3-hour steps, 5 days, UV `—`, no alerts; `free-plan-note` caption |
| Offline | Network error | Stale (muted temp) | Header "Offline · 52m ago" in warning; content dimmed to 55% |
| Rate limited | 429 | Stale | Header "Rate limited · retry in N m" |
| Daily limit | ≥ 900 calls today | Normal (last data) | Caption "Daily call limit reached · resumes 10:00" (local time of 00:00 UTC) |
| Alert | Alert IDs present | Warning dot | Banner(s) |
| Polar day/night | No sunrise/sunset | — | `—` for those stats |

## 11. Settings (cosmic-config, version 1)

```text
Config {
  locations: Vec<Location { id: String /*uuid*/, name: String, region: String, lat: f64, lon: f64 }>  // ≤ 5
  panel_location: Option<String>   // id; None → first
  units: Units = Metric            // Metric | Imperial
  icon_set: IconSet = Detailed     // Detailed | System
  show_city: bool = false
  show_hilo: bool = false
  refresh_minutes: u8 = 30         // 15 | 30 | 60 (else 30)
}
// The API key is NOT in config (SPEC §3).
```

**Suggested modules:**

```text
src/
  main.rs, app.rs, config.rs, localize.rs
  key.rs       // oo7 keyring + 0600 fallback, redaction
  api/
    mod.rs     // client, tier detection, call counter, backoff
    onecall4.rs, free25.rs, geocoding.rs   // each with fixture tests
  model.rs     // Snapshot { current, hourly[24|9], daily[7|5], alerts, tz_offset, fetched_at, tier }
  cache.rs     // per-location JSON cache
  icons.rs     // include_bytes! for wx-pd-*, the Detailed/System mapping (§6)
  format.rs    // §7 (+ tests)
  widgets/ panel.rs, current.rs, hourly.rs (canvas), daily.rs, details.rs, locations.rs, alert.rs
```

## 12. Accessibility and i18n

- Every string comes from the `.ftl` file. Pass the UI language to OWM `lang`.
- Condition icons always have the text description next to them in the current block. The day rows expose "Fri, light rain, 46% chance of rain, 15° to 27°" to assistive tech.
- The hourly canvas has a text summary as its accessible name (DESIGN §4).
- Focus order: location button → refresh → alert banners → Applet settings → attribution link.
- Esc closes the location list first, then the popup.
