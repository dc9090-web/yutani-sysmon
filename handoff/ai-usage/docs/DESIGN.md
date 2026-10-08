# AI Usage applet: design spec

The source of truth is `design/tokens.json`, which holds libcosmic's stock theme values. The prototype in `design/prototype/` renders every screen.

**Rule 1:** read values from `theme.cosmic()` at runtime. The hex values below are for checking screenshots only.

## 1. Colour

The general UI tokens (`bg`, `on-bg`, `on-bg-muted`, `bg-component`, dividers, menu hover, focus) are shared with the sibling applets; see `design/tokens.json`. These are the AI Usage tokens:

| Token | Dark | Light | libcosmic | Use |
|---|---|---|---|---|
| `quota-fill` | #63d0df (stock accent) | #00525a | `cosmic.accent.base` | Meter fill below 80%. **Follows the user's accent**, like `progress_bar`. |
| `quota-warn` | #ffa37d | #792c00 | `cosmic.warning.base` | Fill and value at 80–99% used; pace text when ahead at this level |
| `quota-full` | #ffa09a | #890418 | `cosmic.destructive.base` | Fill and value at 100%; "Limit reached" text |
| `pace-marker` | #e7e7e7 | #121212 | `cosmic.background.on` | Pace tick |
| `meter-track` | #2e2e2e | #f5f5f5 | `cosmic.background.component.base` | Unfilled meter |
| `small-widget` | #27272740 | #cacaca40 | `cosmic.background.small_widget` | Plan chip |
| `warning` | #ffa37d | #792c00 | `cosmic.warning.base` | Stale freshness text ("Offline · 25m ago") |

**Known limit:** in dark mode, `quota-warn` and `quota-full` are nearly identical, about 1.0:1 against each other. The popup always carries the words "Limit reached". The panel currently tells them apart only by the missing pace tick. See the open decision in CLAUDE.md.

## 2. Robot icon

- **File:** `resources/icons/hicolor/scalable/apps/<APP_ID>-symbolic.svg` (a copy is in `design/icons/ai-usage-robot-symbolic.svg`).
- **Shape:** a 16px grid, single ink `#232323` (recoloured by `.symbolic(true)`). A 10.8 × 9.15 head with 2.6 corner radius and a 1.5 stroke; a 1.4-stroke antenna with a 1.1 dot; 1.25-radius eyes; a 1.3-stroke mouth; 1.3 × 3.2 ear nubs.
- **Panel:** first item, at `suggested_size(true)` (16/20/28/32/48 for XS–XL), `background.on`.
- **Popup header:** 20px.
- **Applet list:** the desktop entry `Icon=`.
- **Don't** tint it by state, animate it, or replace it with a provider logo.

## 3. Type

| Style | Size / line | Weight | Font | Use |
|---|---|---|---|---|
| metric-label | 10/12 (9/11 vertical) | 600 | mono | Panel labels `5h` `Week` `Fable` `Reset` |
| rate-panel | 12/17 (11/14 vertical) | 400 | mono | Panel values (Percent / Both) |
| metric-value | 24/32 | 700 | mono | Row value ("42%") |
| heading | 14/21 | 700 | sans | "Claude", window names, banner titles, settings title |
| body | 14/21 | 400 | sans | Toggle rows, segments |
| caption | 12/17 | 400 | sans | Email, descriptions, reset text, pace text, "used"/"left", freshness |
| chip | 12/20 | 600 | sans | Plan chip |

## 4. Geometry

### Panel (size S, horizontal, 40px bar): default Bars, all three windows

```text
 ╭─────────────────────────────────────────╮
 │ [robot 20]  5h        Week      Fable   │   gap space_xs between items
 │             ▬▬▬│▭▭    ▬▬▬▬│▭    ▬▬▬▬▬│   │   bar 32×6 radius_xl, tick 2×10
 ╰─────────────────────────────────────────╯
```

Percent style: the value replaces the bar, right-aligned, 4 characters. Both style: the value then the bar on the second line, `space_xxs` apart. The `Reset` chunk is a 5-character countdown.

### Popup (360 wide, content 312)

```text
┌──────────────────── 360 ────────────────────┐
│ [robot] Claude [Max]        Updated 2m ago ⟳│  header: pad 8 / 24 left / 12 right
│         dc@example.com                      │
│ ─────────────────────────────────────────── │  inset divider
│ Session                              42% used│  heading / metric-value + caption
│ 5-hour window                               │
│ ▬▬▬▬▬▬▬▬▬▬▬▬▬▭▭▭▭▭│▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭   │  meter 8px, tick 2×14 at expected %
│ ◷ Resets in 2h 12m           14% under pace │  caption row
│ Weekly …                                    │
│ Fable …                                     │
│ ─────────────────────────────────────────── │
│ ⚙ Applet settings                          › │
└─────────────────────────────────────────────┘
```

- Rows: padding `space_xxs` × `space_m`, internal gap `space_xxs`.
- Banners: a `bg-component` box with `radius_s`, padded `space_xxs` × `space_xs`, inset by `space_m`. The title is `body` 600, the text is `caption`, and any action is a standard button.

## 5. Meter rules

- The fill width is the displayed % (used, or 100 − used).
- The tick sits at the expected % (or 100 − expected in Left mode), and is hidden at 100% used or with no `resets_at`.
- Stale: fill at 45% opacity, value in `on-bg-muted`.
- No animation. Values jump on refresh.

## 6. Screens

`design/screenshots/` shows the panel styles, the popup, settings, every state and the icon study, in both themes.
