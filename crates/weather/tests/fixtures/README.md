These are sample OpenWeather responses for parser and mapping tests. **None were captured live.** They're built from the documented field lists (One Call 4.0, Current Weather 2.5, 5-day/3-hour Forecast 2.5, Geocoding 1.0, checked Oct 2026). Verify them against one real call per endpoint during milestone 2, and replace any that differ.

"Now" = `1791417600` = 2026-10-08T00:00:00Z = 11:00 in Sydney (UTC+11, `timezone_offset` 39600). Units are `metric`.

| File | Expected result |
|---|---|
| `onecall4/current.json` | 23°, feels 23°, scattered clouds → `partly-cloudy-day`, wind 15 km/h SW, UV 6 High |
| `onecall4/timeline_1h_page1.json` + `page2.json` | Take the first 24 points across both pages. `next` and `prev` are pagination links. |
| `onecall4/timeline_1day.json` | 10 days; show 7. Today's min/max are widened to cover the hourly range. |
| `onecall4/current_with_alert.json` + `alert.json` | Alert banner "Severe Thunderstorm Warning · Bureau of Meteorology · 12:00–19:00" (Brisbane, UTC+10); a dot on the panel icon |
| `onecall4/current_polar_windy.json` | No sunrise/sunset → `—`; wind 12.6 m/s on code 01n → `windy-night` (Detailed set) |
| `free25/weather.json` + `forecast.json` | Free mode: 9 points over 24 h in 3-hour steps; 5 days aggregated by **local** date; UV `—`; no alerts |
| `geocoding/direct_melbourne.json` | 5 results; display "Melbourne · Victoria, AU · -37.81, 144.96" and so on |
| `geocoding/direct_empty.json` | "No matches" |
| `errors/401_invalid_key.json` | "Key not accepted" state |
| `errors/429_rate_limited.json` | Back off and show "Rate limited" (SPEC §5) |
