# Acceptance checklist

Run every check in **dark and light**, at panel sizes S, M and L, on a horizontal top panel and a vertical dock. Compare with `design/screenshots/`.

## Key and privacy

- [ ] K-01: The key is stored in the system keyring (check with `secret-tool search application <APP_ID>`). With no Secret Service, it's stored in the 0600 fallback file and the caption shows.
- [ ] K-02: The key never appears in config files, logs (`RUST_LOG=trace`), panics, the UI or the clipboard. `appid=` is redacted in logs.
- [ ] K-03: Test reports Full for a One Call key, Free for a plain free key, and Invalid for a bad key (with the activation note).
- [ ] K-04: The only network host is `api.openweathermap.org`.

## Data

- [ ] D-01: Every fixture in `tests/fixtures/` parses to its documented result. Replace any fixture that disagrees with a live call during milestone 2.
- [ ] D-02: Full tier: 24 hourly points (two 1h pages), 7 days, UV, alerts. Free tier: 9 points in 3-hour steps, 5 days grouped by local date, UV `—`, no alerts, plus the note.
- [ ] D-03: All times and day names use the location's `timezone_offset`. Test Sydney (+11) on a machine set to UTC.
- [ ] D-04: Units switch refetches and converts: km/h ↔ mph, hPa ↔ inHg; temperatures come from the API in the right unit.
- [ ] D-05: The scheduled refresh touches only the panel location. With the 30-minute default, ≤ 4 calls per refresh and ≤ 220 per day (log the counter).
- [ ] D-06: At 900 calls in a UTC day, scheduled refreshes stop and the caption shows. They resume after 00:00 UTC.
- [ ] D-07: 429 → backoff with the countdown; 5xx/offline → stale dimming, then recovery.
- [ ] D-08: The cache shows last data on startup with the correct age, and data older than 12 h is discarded.
- [ ] D-09: Geocoding: "Melbourne" lists 5 results with distinct regions and coordinates; a near-duplicate is refused with "Already saved"; empty results show "No matches".

## Icons

- [ ] I-01: The Detailed mapping matches SPEC §6 for every row of `icon-sets-*.png`. That includes the windy override at ≥ 10.8 m/s, also when units are imperial.
- [ ] I-02: Switching Detailed ↔ System changes every icon at once (panel, current, daily, settings rows); the sets never mix.
- [ ] I-03: The Detailed SVGs are embedded (`include_bytes!`). `just install` installs **no** `wx-pd-*` files, and none are in the release tarball or package.
- [ ] I-04: Icons are recoloured symbolically in both themes, and muted when stale.

## Panel and popup

- [ ] P-01: The panel width is constant from −12° to 40°. Show city and Show high/low work. The alert dot appears with the alert fixture.
- [ ] P-02: The location header list switches the popup view without changing the panel city. The radio in settings changes the panel city.
- [ ] P-03: The hourly canvas matches DESIGN §4 (labels every 3 h, rain % from 20%, the now dot).
- [ ] P-04: Daily rows share one week-wide scale; today's dot sits at the current temperature.
- [ ] P-05: The details' wind icon rotates to the direction; sunrise and sunset show `—` with the polar fixture.
- [ ] P-06: The attribution "Weather data: OpenWeather" is always visible on Main and links to openweathermap.org.
- [ ] P-07: Every state in SPEC §10 matches `states-*.png`.

## Code

- [ ] C-01: `just check` and `just test` pass. `format.rs`, the icon mapping, each API parser and the free-tier daily aggregation have fixture tests.
- [ ] C-02: There are no hex colours in the code; every colour comes from `theme.cosmic()`.
- [ ] C-03: Every visible string is in the `.ftl` file.
