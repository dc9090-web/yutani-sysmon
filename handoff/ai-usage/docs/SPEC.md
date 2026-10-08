# AI Usage applet: functional spec

Version 1.0 · target: COSMIC panel (libcosmic `main`, applet API as in pop-os/cosmic-applets 1.10). This is a sibling of the Network Traffic and System Monitor applets; it shares their design system and patterns. The functional reference is [YapCap](https://github.com/TopiCsarno/yapcap) (MPL-2.0). This applet is Claude-only and reuses Claude Code's login instead of having its own sign-in. **Don't copy YapCap code.** This spec restates only the API facts.

## 1. Scope

A COSMIC panel applet showing how much of a Claude subscription's usage limits have been used:

| Window | Source field | Length | Shown as |
|---|---|---|---|
| **Session** | `five_hour` | 5 h, rolling | `5h` / "Session · 5-hour window" |
| **Weekly** | `seven_day` | 7 d | `Week` / "Weekly · All models · 7 days" |
| **Fable** | `limits[]` entry, `kind = weekly_scoped`, model display name "Fable" | 7 d | `Fable` / "Fable · Fable · 7 days" |

Each window has a percentage used, a reset time and a pace.

**Out of scope for v1:**
- Other providers
- Multiple accounts
- The applet's own OAuth sign-in
- Extra usage / credits
- Notifications
- Historical charts
- Token counts (the endpoint reports percentages only)

## 2. Identity

| Item | Value |
|---|---|
| App ID | `io.github.dc.CosmicAppletAiUsage` (placeholder; change before publishing) |
| Binary / crate | `cosmic-applet-ai-usage` |
| Desktop entry | `resources/io.github.dc.CosmicAppletAiUsage.desktop` |
| Icon | `resources/icons/hicolor/scalable/apps/<APP_ID>-symbolic.svg`: the custom robot. The same file is embedded for the panel and the popup header. |
| Strings | `i18n/en/cosmic_applet_ai_usage.ftl` |

## 3. Authentication: read-only reuse of Claude Code's login

### 3.1 Locating the credentials

1. `$CLAUDE_CONFIG_DIR/.credentials.json` if that variable is set; otherwise `~/.claude/.credentials.json`.
2. Account email: `~/.claude.json` → `oauthAccount.emailAddress`. Optional; if it's missing, hide the email line.

### 3.2 Expected format

Parse leniently: ignore unknown fields, and never fail on extra keys.

```json
{ "claudeAiOauth": {
    "accessToken": "…", "refreshToken": "…",
    "expiresAt": 1791460000000,          // ms since epoch
    "scopes": ["user:inference", "user:profile", "…"],
    "subscriptionType": "max" } }        // "pro" | "max" | "team" | "enterprise" | …
```

Check this against the installed Claude Code version. If it has changed, say so in `NOTES.md` and ask DC before adapting.

### 3.3 Rules (non-negotiable)

- **Read-only.** Never write, chmod, move or lock either file.
- **Never refresh the token.** A refresh would replace Claude Code's saved token and could log Claude Code out. When `expiresAt` ≤ now, or the endpoint returns 401, go to the *Login expired* state.
- **Watch the file** (the `notify` crate, or poll its mtime every 30 s). When it changes, re-read it and fetch immediately. Claude Code renews the token whenever the user runs it.
- **Scope check:** `scopes` must contain `user:profile`. If it doesn't, go to the *Not signed in* state with the reason `login-no-profile`.
- **Don't keep the token around.** Hold it in memory only, and drop it after each request (re-read the file each time). Never log it; redact anything matching `sk-ant-` in logs.
- **Plan label:** title-case `subscriptionType` ("Max", "Pro"); unknown values are shown verbatim, title-cased.

## 4. Usage endpoint

`GET https://api.anthropic.com/api/oauth/usage`

| Header | Value |
|---|---|
| `Authorization` | `Bearer <accessToken>` |
| `anthropic-beta` | `oauth-2025-04-20` |
| `User-Agent` | `cosmic-applet-ai-usage/<version>` |

- **Client:** `reqwest` with rustls, 5 s connect timeout, 20 s total timeout. A single in-flight request at most.
- **The endpoint is undocumented** and may change without notice. Isolate it in one module (`api.rs`), with fixture tests (`tests/fixtures/usage/*.json`).

### 4.1 Parsing

- `five_hour` → Session and `seven_day` → Weekly. Each has `utilization` (0–100, f32) and `resets_at` (RFC 3339, nullable).
- **Fable:** the first `limits[]` entry where `group == "weekly"`, `kind == "weekly_scoped"`, and `scope.model.display_name` equals "Fable" (case-insensitive) or `scope.model.id` starts with `fable`. Use its `percent` and `resets_at`. If it's absent, there's no Fable window (*No Fable limit* state). Never fall back to `seven_day_opus` / `seven_day_sonnet`.
- **Clamping:** clamp `utilization` to 0–100. A null `utilization` means the window is absent.
- **Unrecognised format:** if `five_hour` and `seven_day` are both absent, go to the *Format not recognised* state. Keep the raw JSON in memory for "Copy diagnostics".
- **Fixtures:** each must parse to the states named in `tests/fixtures/usage/README.md`.

### 4.2 Responses

| Response | Action |
|---|---|
| 200 | Store the snapshot with `fetched_at = now`. |
| 401 / 403 | *Login expired*. Stop polling until the credentials file changes. |
| 429 | *Rate limited*. Wait for `Retry-After` seconds (default 300), then resume. |
| 5xx, timeout, DNS | *Offline*. Retry with exponential backoff (1, 2, 4… min, capped at the refresh interval). |

## 5. Refresh

- **Interval:** setting `refresh_minutes`, one of 1, 5 or 15 (default **5**). Add ±10% jitter.
- **On popup open:** refetch if the data is more than 60 s old.
- **Manual refresh:** the header button. Disabled while a request is in flight, and for 10 s after one completes (debounce).
- **Local clock:** recompute countdowns, pace and "Updated Nm ago" every 30 s without refetching.
- **Elapsed window:** if `resets_at` ≤ now, treat that window as 0% used with "Resetting now" until the next fetch. Schedule a fetch at `resets_at + 30 s`.

## 6. Calculations and formatting

- **Pace.** `expected = clamp((length − (resets_at − now)) / length × 100, 0, 100)`, where length is 5 h or 7 d. `delta = used − expected`.
  - If |delta| < 3: "On pace".
  - If delta > 0: "{n}% ahead of pace".
  - Otherwise: "{n}% under pace".
  - If `resets_at` is null, show no pace and no tick.
- **Used / Left.** The `amount` setting flips the displayed value (100 − used), the meter fill and the tick position. Thresholds always use *used*.
- **Levels:** used < 80 → normal; 80 ≤ used < 100 → warning; used ≥ 100 → limit reached.
- **Relative reset text:** "Resets in 2h 13m", "Resets in 3d 4h", "Resets in 13m", "Resetting now". The compact panel form is `2h13m`, `3d4h`, `13m`, `now`.
- **Absolute reset text:** "Resets 15:40", "Resets tomorrow 09:00", "Resets Sat 09:00", "Resets 12 Oct 09:00".
  - 24-hour time unless the COSMIC time applet's config (`com.system76.CosmicAppletTime`, key `military_time`) says otherwise. If it can't be read, use 24-hour time.
  - Local time zone.
- **Freshness:** "Updated just now", "Updated 2m ago", "Updated 3h ago".
- `design/prototype/bundle.js` holds the reference implementation (`formatReset`, `pace`, `quotaRowHTML`, `quotaChunkHTML`). Unit-test every example above.

## 7. Panel button

- **Build:** `button::custom(row![robot, chunks…].spacing(space_xs).align_y(Center)).class(Button::AppletIcon)`, padded with `core.applet.suggested_padding(true)`, inside `core.applet.autosize_window`.
- **Robot icon:** always first, sized to `core.applet.suggested_size(true)`, `.symbolic(true)`.
- **Chunks:** one per enabled window, **in the order Session, Weekly, Fable**, then the optional `Reset` chunk (the session countdown, compact form, 5-character field).
- **Styles** (setting `style`; default **Bars**):
  - **Percent:** `column![label, value]`, with the value right-aligned in a 4-character field.
  - **Bars:** `column![label, bar 32×6 + pace tick]`.
  - **Both:** `column![label, row![value, bar]]`.
- **Label:** mono 10/12, weight 600, `on-bg-muted`. Labels are `5h`, `Week`, `Fable`, `Reset`.
- **Colours:** `quota-fill` (accent) in the normal state, `warning` at the warning level, `destructive` at the limit. At the limit, the pace tick is hidden.
- **Vertical panels (S and up):** chunks stack centred, the bar shrinks to 24px, label 9/11, value 11/14.
- **Vertical XS:** robot plus bars only.
- **Width:** constant for a given configuration; values never change it.
- **Stale data** (offline, rate limited, expired): bars at 45% opacity, values muted. The robot stays at full opacity.
- **Not signed in, or no data:** the robot only, with a tooltip.
- **Tooltip and accessible name:** "Session 42% used, resets in 2h 13m; Weekly 61% used, resets in 3d 4h; Fable 84% used, resets in 3d 4h", plus a state suffix ("· offline, updated 25m ago").
- **Click:** toggles the popup.

## 8. Popup

`core.applet.popup_container(content.padding([8, 0, 8, 0]))`, 360px wide. Two pages, `Main` and `Settings`; the popup always opens on Main.

### 8.1 Main page

1. **Header:** robot (20px), "Claude" (`text::heading`) and a plan chip, with the email (caption) under them. On the right, the freshness text (caption, muted; warning when stale) and a refresh icon button (`view-refresh-symbolic`).
2. Inset divider.
3. **State banner**, if there is one (see §9).
4. **One quota row per available window**, in the order Session, Weekly, Fable, regardless of the panel toggles. Each row:
   - **Top:** the name (heading) over the description (caption) on the left; the value (mono 24/32 bold) plus "used" or "left" on the right.
   - **Meter:** 8px, `radius_xl`, with a 2 × 14 pace tick.
   - **Bottom:** a 12px `appointment-soon-symbolic` icon and the reset text on the left; the pace text on the right.
5. Inset divider, then `menu_button` ⚙ "Applet settings" ›.

### 8.2 Settings page

1. Back button and "Applet settings", then a full-width divider.
2. **Show in panel:** toggler rows for Session, Weekly and Fable (all on by default) and "Session reset" (off).
   - At least one window stays on; the last enabled toggler is disabled, with a caption saying why.
   - The Fable toggler is disabled with the caption `no-fable-limit` when the account has no Fable window.
3. **Panel style:** a segmented control with Percent, Bars (default) and Both.
4. **Show:** Used (default) or Left.
5. **Reset times:** Relative (default) or Clock time.
6. **Refresh every:** 1 min, 5 min (default) or 15 min.

Every change applies immediately and persists.

## 9. States

| State | Trigger | Panel | Popup |
|---|---|---|---|
| Normal | 200 OK, fresh | Live bars | Rows |
| Not signed in | No credentials file, or `user:profile` missing | Robot only | Banner: "Sign in with Claude Code". Body: run `claude` and log in. No rows. |
| Login expired | `expiresAt` ≤ now, or 401/403 | Stale bars | Banner: "Claude Code login expired · Open Claude Code once to renew it." Rows dimmed. |
| Offline | Network error | Stale bars | Header: "Offline · 25m ago" (warning). Rows dimmed. |
| Rate limited | 429 | Stale bars | Header: "Rate limited · retry in 4m" (warning). Rows dimmed. |
| Format not recognised | §4.1 | Robot only | Banner: "Usage format not recognised", with a "Copy diagnostics" button that copies the HTTP status and the JSON **keys only**, never the values or the token. |
| No Fable limit | No matching `limits[]` entry | No Fable chunk | No Fable row; caption line "No Fable limit on this plan."; Fable toggler disabled |
| Limit reached | used ≥ 100 | Value and fill in `destructive`, no tick | Row: "Limit reached · resets in 1h 05m" (destructive) |

Stale data older than 24 h is discarded: show the banner without rows.

## 10. Settings (cosmic-config, version 1)

```text
Config {
  show_session: bool = true
  show_weekly: bool = true
  show_fable: bool = true
  show_session_reset: bool = false
  style: PanelStyle = Bars          // Percent | Bars | Both
  amount: Amount = Used             // Used | Left
  reset_format: ResetFormat = Relative   // Relative | Absolute
  refresh_minutes: u8 = 5           // 1 | 5 | 15 (anything else → 5)
}
```

**Invariant:** at least one `show_*` window is true. If a loaded config has none, force `show_session = true`.

## 11. Privacy and security

- The only network destination is `api.anthropic.com`. There's no telemetry, no update check and no other hosts.
- Nothing is written to disk except cosmic-config settings. **Don't cache usage on disk.** A cold start shows "Updated —" until the first fetch.
- The token never appears in logs, the UI, the clipboard or panic messages. Wrap it in a newtype with a redacting `Debug`.

## 12. Accessibility and i18n

- Every string comes from the `.ftl` file.
- Each meter exposes its name, value and "used" or "left" to assistive tech.
- The level is always stated in words as well as colour ("Limit reached"; the pace text).
- Focus order: refresh button → banner button (if any) → Applet settings. On the settings page: Back → togglers → segments.
- Esc closes the popup. On the settings page, Esc goes back to Main first.

## 13. Suggested module layout

```text
src/
  main.rs, app.rs, config.rs, localize.rs
  auth.rs        // locate + parse credentials (read-only), file watcher, scope/expiry checks, redacting Token type
  api.rs         // usage request + response parsing (fixtures in tests/fixtures/usage)
  scheduler.rs   // refresh interval, jitter, backoff, Retry-After, reset-time wakeups
  model.rs       // Window { kind, used, resets_at, length }, Snapshot, State
  format.rs      // reset text, pace, freshness (+ unit tests)
  widgets/
    panel.rs     // robot + chunks
    row.rs       // quota row with pace tick (canvas or custom widget)
    header.rs, banner.rs
```
