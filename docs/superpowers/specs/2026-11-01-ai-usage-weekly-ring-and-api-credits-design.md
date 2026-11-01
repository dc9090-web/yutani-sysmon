# AI Usage: weekly ring and API credits (design, 2026-11-01)

## Why

Claude removed the 5-hour session window from DC's plan on or before 2026-11-01: the usage endpoint now returns `"five_hour": null`. The applet handled that without crashing (a null window is "absent"), but the avatar ring, the first panel chunk and the Reset countdown all keyed off Session, so they went blank. At the same time, Ultra plans now include $200 of Claude Platform (Console) credits a month, which DC wants to see next to the plan limits.

Live response on 2026-11-01 (DC's Ultra account): `five_hour: null`; `seven_day` 76 % resetting 2026-11-01T20:00Z; `limits[]` with `weekly_all` (76 %) and the Fable `weekly_scoped` entry (48 %); `extra_usage` / `spend` blocks present but disabled; `seven_day_breakdown` rows (Claude Code 74 %, Cowork 24 %, …).

## Part 1: the weekly window takes over the session slot

- **Ring:** the avatar ring shows the **Weekly** window's % left, with the same colours and stale rules as before. Tooltip text: "Week 24% left", "Weekly limit reached", "Weekly: no data".
- **Panel order:** Session (only when the endpoint returns it), Weekly, Fable, API credits, Reset. Nothing else moves.
- **Reset chunk:** the `Reset` countdown follows the Weekly window ("3d4h"). The setting is renamed `show_weekly_reset` ("Weekly reset · Time until the 7-day window resets"). The old `show_session_reset` key is ignored; the config stays at version 1.
- **Invariant:** with every window toggle off, `show_weekly` is forced on (was `show_session`).
- **Session toggle:** listed on the settings page only while the current data has a Session window, so plans that still have one keep the control.
- **Fixtures and demo:** a `weekly_only.json` fixture mirrors the live shape (values anonymised). The demo's first scene uses it; the ring scene sets Weekly to 87 %.
- **Docs:** README's AI Usage section, NOTES.md, the handoff SPEC §7.1 / §8.2 / §10 and the handoff CLAUDE.md rule "Avatar ring = Session % left" are updated to Weekly.

## Part 2: API credits row

### Source

The monthly credits are Claude Platform promotional credits, visible only on the Console billing page. The usage endpoint does not carry them, and the Console's own credits endpoint rejects OAuth tokens. The only supported route is the Admin API cost report:

```text
GET https://api.anthropic.com/v1/organizations/cost_report
    ?starting_at=<cycle start, RFC 3339>&ending_at=<now>&bucket_width=1d&limit=31
x-api-key: <Admin API key, sk-ant-admin01-…>
anthropic-version: 2023-06-01
```

Response: `data[]` of daily buckets, each with `results[]` carrying `amount` (a decimal string in cents) and `currency`; `has_more` / `next_page` paginate. The applet sums every `amount` in the cycle and shows it against the monthly credit. Costs are reported before credits are applied, so purchased credits spent in the same cycle count too; for a credits-only organisation the sum is exactly "credits used". Data lags Anthropic by about five minutes.

The Admin API is unavailable to Console "individual" accounts; the organisation must be set up as an organisation first (DC did this on 2026-11-01). Personal keys (`sk-ant-usr-…`) scoped to a workspace are rejected with 401.

### Billing cycle

`credit_renew_day` (1–31, default 1) is the day of the month the credits arrive. The cycle starts at 00:00 UTC on that day, clamped to the month's length (day 31 in November → the 30th; day 29–31 in February → the 28th or 29th). If today is before the renew day, the cycle started last month. The next cycle start is "Renews 28 Nov" / "Renews in 26d 4h". A fetch is scheduled 30 s after the renewal, like the window resets.

### Settings (config version 1, new fields with defaults)

- `show_credits: bool = true`: the API panel chunk (only drawn once a key is saved).
- `credit_dollars: u16 = 200`: 100 or 200; anything else → 200.
- `credit_renew_day: u8 = 1`: 1–31; anything else → 1.

The Admin key lives in the Secret Service keyring (attributes `application=<APP_ID>`, `kind=admin-key`), never in config, logs, the UI or the clipboard. There is no file fallback: with no keyring the row stays off and the settings page says so.

Settings page, under Account: group "API credits" with a secure input ("Paste your Admin API key" / "Key saved · paste a new one to replace it"), **Test** (saves the typed key, then fetches; with nothing typed it refetches with the saved key) and **Remove**; a status caption ("Key works", "Key not accepted", "Couldn't check: offline"); a caption noting keys come from platform.claude.com/settings/admin-keys and that spend lags by a few minutes. Then "Monthly credit" ($100 | $200) and "Renews on day" (spin button 1–31). "Show in panel" gains an "API credits" toggler, shown only while a key is saved.

### Display

- **Popup row** (after the quota rows, before the no-Fable caption): "API credits" over "$200 a month · Claude Platform"; the value "$37.20" (mono 24 bold) with "of $200" beside it, or "$162.80" "left" in Left mode; the meter with the pace tick (expected = elapsed fraction of the cycle); the foot: "Renews in 26d 4h" (or the clock form, "Renews 28 Nov") on the left and the pace text on the right. Levels follow the usual 80 % / 100 % thresholds on the used fraction. At or over the limit: "Credits used up · renews 28 Nov" in the destructive colour, meter full.
- **Panel chunk:** label `API`, same value / bar / both styles as the windows, `MAX` and solid at the limit.
- **Tooltip:** "… ; API credits $37 of $200 used, renews in 26d 4h".
- **States:** no key → no row, no chunk. Key rejected (401/403) → row foot "Admin key rejected" (destructive), last value dimmed, polling of the cost report stops until the key changes. Offline / rate limited → last value dimmed, like the windows; the header's state text is unchanged (it describes the usage request). Format not recognised → "Spend format not recognised" in the foot. No spend yet in the cycle → "$0.00 of $200".
- **Freshness:** credits are fetched alongside every usage fetch (same interval, popup open, manual refresh) and once right after a key is saved. A cost fetch never changes the usage scheduler's state.

### Modules

- `credits.rs`: `AdminKey` (redacting `Debug`), keyring load / save / forget, `Cycle` maths, the request, `parse`, `Outcome`, the live view (`CreditsLive`: spent, limit, level, pace, renews_at).
- `app.rs`: credits state (key, spend, status, fetching), the settings group, the row and chunk, the tooltip suffix.
- `widgets/row.rs`: the row layout shared by quota rows and the credits row.
- `format.rs`: `dollars(cents)` ("$37.20", "$200"), `renew_text`.
- Fixtures: `tests/fixtures/cost/normal.json`, `tests/fixtures/cost/paged.json`, `tests/fixtures/usage/weekly_only.json`.

### Tests

- `credits::parse` against the fixtures (sum, pagination, non-USD or malformed amounts → unrecognised, empty data → $0).
- `Cycle::at(day, now)` for the clamping cases above and a month boundary; `renews_at` always after `now`.
- Pace and level from cents; dollars formatting; the key never appears in `Debug` output; every `fl!` key exists.
- Demo: a scene with credits at $37.20 and one at the limit.

## Out of scope

- The `seven_day_breakdown` rows (Claude Code, Cowork, …) and the "extra usage" spend block.
- A true balance (purchased credits, rollover): the cost report can't see it.
- A version bump or release.
