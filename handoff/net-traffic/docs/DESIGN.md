# Network Traffic applet: design spec

The source of truth is `design/tokens.json`, which holds libcosmic's stock theme values extracted from `dark_default()` and `light_default()`. The prototype in `design/prototype/` renders every screen.

**Rule 1:** in Rust, never hard-code these values. Read them from `theme.cosmic()` (or `cosmic::theme::active().cosmic()`) so the user's accent, theme, roundness and density apply. Use the hex values only for checking screenshots.

## 1. Token → libcosmic mapping

### Colour

| Token | Dark | Light | libcosmic |
|---|---|---|---|
| `bg` | #1b1b1b | #d7d7d7 | `cosmic.background.base` (popup_container applies it) |
| `bg-component` | #2e2e2e | #f5f5f5 | `cosmic.background.component.base` |
| `bg-component-hover` | #434343 | #f6f6f6 | `cosmic.background.component.hover` |
| `on-bg` | #e7e7e7 | #121212 | `cosmic.background.on` |
| `on-bg-muted` | #9e9e9e | #484848 | `cosmic.palette.neutral_7` (dark) / `neutral_6` (light). Added: libcosmic has no named muted role. |
| `on-component` | #c0c0c0 | #272727 | `cosmic.background.component.on` |
| `divider` | #444444 | #b0b0b0 | `cosmic.background.divider` (popup border) |
| `component-divider` | #c0c0c033 | #27272733 | `cosmic.background.component.divider` (inset dividers, graph grid) |
| `menu-hover` | #63636333 | same | `cosmic.text_button.hover` (`Button::AppletMenu` / `AppletIcon` handle this) |
| `menu-pressed` | #16161680 | #bebebe80 | `cosmic.text_button.pressed` |
| `accent` / `focus` | #63d0df | #00525a | `cosmic.accent.base` / `cosmic.accent.focus` |
| `on-accent` | #000000 | #ffffff | `cosmic.accent.on` |
| `success` | #5edb8c | #00572c | `cosmic.success.base` |
| `warning` | #ffa37d | #792c00 | `cosmic.warning.base` |
| `destructive` | #ffa09a | #890418 | `cosmic.destructive.base` |
| `net-down` | #63d0df | #00525a | `cosmic.palette.accent_blue`. **Fixed, never the user's accent.** |
| `net-down-fill` | net-down @ 25% | net-down @ 20% | Derived alpha |
| `net-up` | #ffad00 | #624000 | `cosmic.palette.accent_orange`. Fixed. |

### Spacing (density "Standard")

| Token | px | libcosmic | Use |
|---|---|---|---|
| `space-xxxs` | 4 | `space_xxxs()` | Indicator gap, graph inner padding |
| `space-xxs` | 8 | `space_xxs()` | Row vertical padding; panel graph-to-numbers gap |
| `space-xs` | 12 | `space_xs()` | Gap between readout columns and between row parts |
| `space-s` | 16 | `space_s()` | Inset divider side margin |
| `space-m` | 24 | `space_m()` | Row horizontal padding (`menu_control_padding`) |

### Radius (roundness "Round")

| Token | px | libcosmic | Use |
|---|---|---|---|
| `radius-0` | 0 | `radius_0()` | Menu rows |
| `radius-s` | 8 | `radius_s()` | Graph well |
| `radius-m` | 16 | `radius_m()` | Popup (popup_container applies it) |
| `radius-xl` | 160 | `radius_xl()` | Pills: segmented track, picker, panel hover, back button |

### Type

The interface font is Open Sans and the mono font is Noto Sans Mono; both are libcosmic defaults.

| Style | Size / line | Weight | libcosmic | Use |
|---|---|---|---|---|
| heading | 14/21 | 700 | `text::heading` | Adapter name, settings title |
| body | 14/21 | 400 | `text::body` | Rows, segments |
| caption-heading | 12/17 | 600 | `text::caption_heading` | Section labels |
| caption | 12/17 | 400 | `text::caption` | Meta, units, axis labels, totals label |
| monotext | 14/20 | 400 | `text::monotext` | Panel numbers, size M+ |
| rate-panel | 12/17 | 400 | `text::text(..).size(12).font(cosmic::font::mono())` | Panel numbers, size XS/S |
| rate-readout | 20/30 | 700 | `.size(20).font(mono bold)` | Popup readouts |

## 2. Popup geometry

```text
┌──────────────────── 360 ────────────────────┐  radius_m, 1px background.divider, no shadow
│ 8px top padding                             │
│ [24] icon 20  enp5s0                   [24] │  padded_control (8 × 24)
│               Automatic · Ethernet · Connected
│ [24] ▬ Download        ─ Upload        [24] │  two cols, gap 12
│      531 kB/s           154 kB/s            │  mono 20/30 bold + caption unit
│ [24] ┌──────── graph 312×120 ───────┐  [24] │  bg-component, radius_s, pad 4
│      │                     20.0 MB/s│       │  scale: mono 12, on-bg-muted, top-right
│      │ 60 s                         │       │  span: caption, bottom-left
│      └──────────────────────────────┘       │
│ [24] Since login             ⌄ 4.21 GB [24] │
│                              ⌃  312 MB      │
│ [16] ─────────── inset divider ─────── [16] │
│ ⚙ Applet settings                        ›  │  menu_button, full-bleed hover
│ Network settings…                           │
│ 8px bottom padding                          │
└─────────────────────────────────────────────┘
```

## 3. Panel geometry (size S, horizontal: 40 px bar)

```text
 ╭──────────────────────────────╮  pill hover (radius_xl), padding major 14 / minor 10
 │ [graph 48×20]  ⌄  15.0 MB/s  │  gap 8 between graph and numbers
 │                ⌃   487 kB/s  │  indicator 1em | number 4ch right | unit 4ch left
 ╰──────────────────────────────╯
```

## 4. Traffic graph (popup and panel)

- **Y scale:** shared by both series. `max = nice(max(all samples) × 1.05)`, where `nice` rounds up to 1, 2 or 5 × 10ⁿ. If every sample is 0, max = 1000 (1 kB/s).
- **X axis:** 60 slots, newest at the right edge. With fewer samples, draw only the right-hand part; don't stretch.
- **Download:** area fill `net-down-fill` plus a 1.5 px `net-down` stroke (1.25 px in the panel), straight segments with round joins.
- **Upload:** a 1.5 px `net-up` line with **no fill**, drawn above download.
- **Grid (popup only):** 3 horizontal lines at 25/50/75% height, 1 px `component-divider`. There are no vertical lines.
- **Labels (popup only):**
  - Scale: top-right, `format_rate(max)` trimmed, in mono 12 `on-bg-muted`.
  - Span: bottom-left, "60 s", in caption `on-bg-muted`. When disconnected, the span label shows the reason instead.
- **Panel sparkline:** 1 px inset, no grid, no labels.
- **Not allowed:** spline smoothing, animation between samples, separate scales.

## 5. Component states

| Element | Rest | Hover | Pressed | Selected | Focus |
|---|---|---|---|---|---|
| Panel button | transparent | `menu-hover` pill | `menu-pressed` | `menu-hover` while the popup is open | 2px `focus` ring |
| Menu row | transparent | `menu-hover`, square | `menu-pressed` | n/a | 2px `focus`, inset |
| Segment / picker | `bg-component` track | `bg-component-hover` | | `accent` fill, `on-accent` text, 600 weight | 2px `focus` |
| Radio | 2px `on-bg-muted` ring | | | `accent` ring and 8px dot | |

## 6. Contrast (checked, WCAG 2)

| Pair | Dark | Light |
|---|---|---|
| on-bg / bg | 13.9 | 13.0 |
| on-bg-muted / bg | 6.4 | 6.4 |
| net-down / bg | 9.5 | 6.2 |
| net-up / bg | 9.2 | 6.5 |
| destructive / bg | 8.8 | 7.0 |

**Known limit:** net-down and net-up are nearly equal in lightness (1.03 dark, 1.05 light). That's acceptable only because indicator shape, area-vs-line and the labels separate them. Never draw both series as plain lines.

## 7. Screens

`design/screenshots/` has every screen in both themes, rendered from the prototype. Open `design/prototype/index.html` in a browser to click through the panel, popup and settings, and to switch the theme.
