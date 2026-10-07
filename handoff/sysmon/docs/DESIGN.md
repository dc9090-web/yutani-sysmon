# System Monitor applet: design spec

The source of truth is `design/tokens.json`, which holds libcosmic's stock theme values. The prototype in `design/prototype/` renders every screen.

**Rule 1:** read values from `theme.cosmic()` at runtime. The hex values below are for checking screenshots only.

## 1. Colour

The general UI tokens (`bg`, `on-bg`, `on-bg-muted`, `bg-component`, dividers, menu hover, accent, `warning`) are identical to the Network Traffic applet. See the table in that package's DESIGN.md §1, or `design/tokens.json`. These are the System Monitor additions:

| Token | Dark | Light | libcosmic | Use |
|---|---|---|---|---|
| `cpu` | #a1c0eb | #2e496d | `cosmic.palette.accent_indigo` | CPU label, graph stroke, dot |
| `cpu-fill` | cpu @ 25% | cpu @ 20% | derived alpha | CPU graph area |
| `gpu` | #e79cfe | #68217c | `cosmic.palette.accent_purple` | GPU label, graph, VRAM and Power meter fill |
| `gpu-fill` | gpu @ 25% | gpu @ 20% | derived | GPU graph area |
| `mem` | #92cf9c | #185529 | `cosmic.palette.accent_green` | Memory label, graph, RAM and Swap meter fill |
| `mem-fill` | mem @ 25% | mem @ 20% | derived | Memory graph area |
| `disk-read` | #63d0df | #00525a | `cosmic.palette.accent_blue` | Read area, stroke and R key (same as network download) |
| `disk-write` | #ffad00 | #624000 | `cosmic.palette.accent_orange` | Write line and W key (same as network upload) |
| `meter-track` | #2e2e2e | #f5f5f5 | `cosmic.background.component.base` | Unfilled meter |
| `warning` | #ffa37d | #792c00 | `cosmic.warning.base` | Hot values, high meters |

**Contrast** (WCAG 2) of each metric colour on `bg` and `bg-component`:

| Colour | Dark | Light |
|---|---|---|
| cpu | 9.2 / 7.3 | 6.4 / 8.4 |
| gpu | 8.7 / 6.8 | 7.0 / 9.2 |
| mem | 9.5 / 7.5 | 6.2 / 8.1 |

## 2. Type

The fonts are Open Sans and Noto Sans Mono, libcosmic's defaults.

| Style | Size / line | Weight | Font | Use |
|---|---|---|---|---|
| metric-label | 10/12 (9/11 vertical) | 600 | mono | Panel labels CPU, GPU, RAM, DISK; R/W keys |
| rate-panel | 12/17 (11/14 vertical) | 400 | mono | Panel values |
| metric-value | 24/32 | 700 | mono | Popup headline per section |
| rate-readout | 20/30 | 700 | mono | Disk read/write readouts |
| heading | 14/21 | 700 | sans (`text::heading`) | Section names, settings title |
| body | 14/21 | 400 | sans (`text::body`) | Toggle and radio rows |
| caption / caption-heading | 12/17 | 400 / 600 | sans | Device captions, stat keys, meter labels, section labels |
| stat value | 14/20 | 400 | mono (`text::monotext`) | Stat values |
| meter value | 12/17 | 400 | mono | Meter value text |

## 3. Sizes and spacing

| Token | Value | Use |
|---|---|---|
| `popup-width` | 360 | Popup (popup_container) |
| content width | 312 | 360 − 2 × `space_m` |
| `metric-graph-height` | 64 | Popup graphs |
| `panel-metric-graph-width` | 32 | Panel sparkline (horizontal); `panel-icon-*` square on vertical |
| `meter-height` | 6 | Meter bar, `radius_xl` ends |
| chunk gap | `space_xs` (12) | Between panel chunks |
| section padding | `space_xxs` × `space_m` | Each popup section |
| stats grid | 3 equal columns, row gap 8, column gap 12 | |
| meters grid | 2 equal columns, gap 12 | |

## 4. Panel geometry (size S horizontal, 40px bar)

```text
 Numbers:  CPU   GPU   RAM   DISK
            16%    1%   29%  R1.8M W339K
 Graph:    CPU   GPU   RAM   DISK
          ~~~~  ▁▇▇▁  ────  ▁▃▆▁            (32×15 sparklines under the labels)
 Both:    [~~~~] CPU    [────] RAM
                  16%            29%        (32×20 sparkline, then label/value)
```

- The label is left-aligned; the value is right-aligned in its 4-character field.
- Graph scales: percentages are fixed 0–100; disk uses a shared nice-max (download/upload style).

## 5. Popup geometry

```text
┌────────────────── 360 ──────────────────┐
│ ● CPU                               5%  │  dot 8 · heading · metric-value right
│ AMD Ryzen 9 7950X · 16 cores / 32 th…   │  caption
│ ┌──────────── 312 × 64 ─────────────┐   │  bg-component, radius_s, grid @50%
│ └───────────────────────────────────┘   │
│ Clock (avg)   Tctl        Package power │  3-col stats
│ 3.67 GHz      44 °C       46 W          │
│ ───────────── inset divider ─────────── │
│ ● GPU                              90%  │
│ …graph…                                 │
│ VRAM  9.1 / 20.0 GiB  Power 252 W/315 W │  2-col meters
│ ▬▬▬▬▬▬▬───────────    ▬▬▬▬▬▬▬▬▬▬▬▬───   │
│ Clock  Edge  Junction                   │
│ ─────────────────────────────────────── │
│ ● Memory                  18.3 GiB used │
│ …graph… / RAM · Swap meters             │
│ ─────────────────────────────────────── │
│ ● Disk I/O                              │
│ ▬ Read 1.20 MB/s   ─ Write 686 kB/s     │  network RateReadout style
│ …graph (auto scale, label top-right)…   │
│ Read since login  Written  NVMe temp    │
│ ─────────────────────────────────────── │
│ ⚙ Applet settings                     › │
└─────────────────────────────────────────┘
```

## 6. Graph rules

- **Single-series** (CPU, GPU, Memory): metric-colour area fill plus a 1.5px stroke (1.25px in the panel), straight segments, round joins.
- **Fixed scale:** 0–100, with one grid line at 50% (`component-divider`) and no label.
- **Disk:** exactly the network graph. Read is area plus stroke, write is a line only, on a shared `nice()` max, with the scale label top-right in mono 12 muted.
- **History:** 60 slots, newest on the right. A gap for `None`. No smoothing, no animation.

## 7. States

See `design/screenshots/states-*.png`.

- **Hot:** the value is `warning` and the key gains "· Hot". In the panel, only the value is tinted.
- **No AMD GPU:** the GPU section collapses to its top row.
- **Unreadable:** `—`, with the layout unchanged.
- **Meters:** the fill is `warning` at the RAM ≥ 90% or VRAM ≥ 95% thresholds.
