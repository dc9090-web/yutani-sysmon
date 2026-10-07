# Network Traffic applet: functional spec

Version 1.0 · target: COSMIC panel (libcosmic `main`, applet API as in pop-os/cosmic-applets 1.10)

## 1. Scope

A COSMIC panel applet that shows live download and upload throughput for one network interface:

- **Panel:** numbers, a sparkline, or both.
- **Popup:** live readouts, a 60 s graph, session totals and settings.

**Out of scope for v1:**
- Per-process traffic
- Bits/s units
- Wi-Fi SSID and signal (would need NetworkManager)
- History beyond 60 s
- Alerts and data caps
- Multiple instances showing different adapters

## 2. Identity

| Item | Value |
|---|---|
| App ID | `io.github.dc.CosmicAppletNetTraffic` (placeholder; change before publishing) |
| Binary / crate | `cosmic-applet-net-traffic` |
| Desktop entry | `resources/io.github.dc.CosmicAppletNetTraffic.desktop` |
| Panel icon (applet list) | `resources/icons/hicolor/scalable/apps/<APP_ID>-symbolic.svg` |
| Strings | `i18n/en/cosmic_applet_net_traffic.ftl` (Fluent via `i18n-embed-fl`) |

## 3. Data source

### 3.1 Interfaces

Enumerate `/sys/class/net/*`. For each entry:

| Field | Source | Rule |
|---|---|---|
| name | directory name | Show exactly as the kernel reports it. |
| skip | `type` = 772 (loopback) | Never list `lo`. |
| kind | `wireless/` subdir exists → **Wi-Fi**; else `device` symlink exists → **Ethernet**; else **Virtual** | Virtual covers wg, tun, docker, br, veth and similar. |
| up | `operstate` = `up`, or (`operstate` = `unknown` and `flags` & 0x1) | Tunnels report `unknown`. |
| carrier | `carrier` = 1 | A read error counts as no carrier. |
| speed | `speed` (Mb/s) | Ethernet only. Ignore values ≤ 0 or read errors. |

Re-enumerate every 5 s and whenever a read fails. Also re-enumerate when the popup opens, so hot-plugged USB adapters appear.

**Ordering in the adapter list:**
1. Automatic
2. Physical interfaces (Ethernet, then Wi-Fi), alphabetical
3. Virtual interfaces, alphabetical
4. Hide `veth*` interfaces when there are more than 3.

### 3.2 Counters

- Read `/sys/class/net/<if>/statistics/rx_bytes` (download) and `tx_bytes` (upload).
- Sample **all** listed interfaces once per second with a `Subscription` (`cosmic::iced::time::every(Duration::from_secs(1))`; check the exact path against the libcosmic version you pin). Sampling every interface means switching adapters shows history immediately.
- Rate is `Δbytes / Δt`, with `Δt` from `std::time::Instant` (monotonic). Never assume exactly 1 s.
- If a counter decreases (wrap or reset), record `None` for that sample and re-baseline.
- Keep a ring buffer of 60 `Option<f64>` samples per interface, per direction.
- Keep per-interface totals for the session: the sum of positive deltas since the applet started. "Since login" means since the applet started, which is at login.
- File reads are small. Do them on the subscription's worker, never in `view`.

### 3.3 Automatic adapter

- **IPv4:** parse `/proc/net/route` and take the row with `Destination == 00000000` and the lowest `Metric`. Its `Iface` is the default route.
- **IPv6 fallback:** if there's no IPv4 default, use `/proc/net/ipv6_route` (destination all zeros, prefix length 0, lowest metric).
- Re-evaluate every 5 s.
- If there's no default route, follow the first physical interface that is up. If none is up, show the Disconnected state.

## 4. Formatting

All numbers use decimal SI.

| Function | Rule | Examples |
|---|---|---|
| `format_rate(bps)` | Divide by 1000 while ≥ 999.5; units `B/s`, `kB/s`, `MB/s`, `GB/s`. Below 10 → 2 decimals; below 100 → 1 decimal; otherwise an integer. B/s is always an integer. | `0 B/s`, `9.84 kB/s`, `12.4 MB/s`, `860 kB/s` |
| Panel field | Number right-aligned in a **4-character** column; unit left-aligned in a 4-character column. | `" 860" "kB/s"` |
| `format_compact(bps)` (vertical panel) | At most 4 characters; suffix K/M/G; one decimal below 10. | `0`, `860K`, `12M`, `1.2G` |
| `format_bytes(total)` | Same as `format_rate` with no `/s`. | `4.21 GB` |
| Unknown | An em dash `—`, never `0`. | |

Unit-test these against the examples here and in `design/prototype/bundle.js` (`formatRate`, `formatCompact`), which is the reference implementation.

## 5. Panel button

- **Modes** (setting `mode`):
  - `Numbers`
  - `Graph`
  - `Both` (default): graph first, then numbers.
- **Build:** the content goes inside `cosmic::widget::button::custom(content).class(cosmic::theme::Button::AppletIcon)`, padded with `core.applet.suggested_padding(true)`. Don't use `core.applet.button_from_element`, because it fixes the width to the icon size. Wrap the panel view in `core.applet.autosize_window(...)` so the panel gives the applet its natural width. Check this against how cosmic-applet-time sizes its text button.
- **Width:** fixed for a given mode and panel size; it must not change as values change.
- **Click:** toggles the popup (`app_popup` / `destroy_popup`, as in libcosmic `examples/applet`).
- **Tooltip and accessible name:** `a11y-summary`, or `a11y-disconnected`. Use `core.applet.applet_tooltip`.

**Layout by orientation and panel size.** `icon` is `core.applet.suggested_size(true)`:

| Orientation | Panel size | Graph | Numbers |
|---|---|---|---|
| Horizontal | XS, S | `icon.h × 2.4` wide (48 px at S) × `icon.h` | Two stacked lines, mono 12/17 |
| Horizontal | M, L, XL | same rule | Two stacked lines, mono 14/20 |
| Vertical | S and up | square, `icon.w` | Two stacked lines, compact form, mono 11/14, indicator 10 px, no unit |
| Vertical | XS | square | **Numbers can't fit in 32 px: show the graph only in every mode.** The tooltip carries the numbers. |

**Rate line structure:**
- Each line is a 3-cell row: **indicator | number | unit**.
- The indicator cell is a fixed width, 1em; 2.4ch for `rxtx`.
- The number cell is right-aligned with a fixed width of 4ch; the unit cell is left-aligned with a fixed width of 4ch.
- The two lines sit in a `column![]`.
- Never format the indicator into the number string. Alignment of the two indicators is a hard requirement (ACCEPTANCE A-03).

## 6. Popup

The popup is `core.applet.popup_container(content.padding([8, 0, 8, 0]))`, 360 px wide (libcosmic's fixed value). It has two pages, held in state: `Page::Main` and `Page::Settings`. Every time the popup opens, it starts on Main.

### 6.1 Main page

From top to bottom:

1. **Header** (`padded_control`): the adapter type icon (`network-wired-symbolic`, `network-wireless-symbolic`, or `network-transmit-receive-symbolic` for virtual), then a column with:
   - the name, `text::heading`
   - a caption, `text::caption`: `[Automatic · ]<Type>[ · <speed>] · <State>`. The state word is coloured success (Connected/Up) or destructive (Disconnected).
2. **Readouts** (`padded_control`): two equal columns, Download and Upload. Each has:
   - a label row: a series swatch plus a caption label
   - a value: mono 20/30 bold, with the unit in caption style
3. **Traffic graph** (`padded_control`): a `canvas` 312 × 120 px in a container with `bg-component` and radius S. See DESIGN.md §4.
4. **Totals row** (`padded_control`): the caption "Since login" on the left; on the right, two stacked rate-style lines of indicator | value | unit (totals have no `/s`).
5. **Divider:** `padded_control(divider::horizontal::default()).padding([space_xxs, space_s])`.
6. **Settings row:** `menu_button` with `emblem-system-symbolic`, "Applet settings" and a trailing `go-next-symbolic`. It sets `Page::Settings`.
7. **Network settings row:** `menu_button` labelled "Network settings…". It launches `cosmic-settings network` and closes the popup.

### 6.2 Settings page

1. **Header row:** an icon button (`go-previous-symbolic`, 32 px, pill) that returns to Main, then the title "Applet settings" (`text::heading`). Below it, a full-width divider.
2. **Show in panel:** a caption heading, then a segmented control (`segmented_control::horizontal` + `SingleSelectModel`) with Numbers, Graph, Both.
3. **Inset divider.**
4. **Direction indicator:** a caption heading, then a 6-option picker. Below it, a caption naming the selection: `<name> · <note>`.
   - Preferred build: a row of 6 equal custom toggle buttons in a pill track, each showing the down icon above the up icon in the series colours. The selected button has an accent fill and icons in `on-accent`. This matches the design.
   - Fallback: a segmented control with one combined monochrome icon per option.
5. **Inset divider.**
6. **Adapter:** a caption heading, then a radio list of `menu_button` rows. Each row is `[radio] [type icon] [name / caption]`.
   - The first row is Automatic, with caption `adapter-automatic-caption`.
   - Disconnected interfaces stay selectable, with the caption in destructive.

Changes apply immediately and are saved straight away; there's no Save button.

## 7. Settings (cosmic-config)

Derive `CosmicConfigEntry`, version 1, and watch it with `core.watch_config::<Config>(APP_ID)`.

```text
Config {
  mode: DisplayMode        = Both        // Numbers | Graph | Both
  indicator: Indicator     = Chevrons    // Arrows | Triangles | Chevrons | RxTxIcons | Bar | RxTx
  adapter: AdapterChoice   = Automatic   // Automatic | Named(String)
}
```

**Indicator assets** (down, up):

| Variant | Down | Up | Loaded as |
|---|---|---|---|
| Arrows | `↓` | `↑` | Text, default sans font |
| Triangles | `pan-down-symbolic` | `pan-up-symbolic` | `icon::from_name` |
| Chevrons | `go-down-symbolic` | `go-up-symbolic` | `icon::from_name` |
| RxTxIcons | `network-receive-symbolic` | `network-transmit-symbolic` | `icon::from_name` |
| Bar | `net-down-bar-symbolic` | `net-up-bar-symbolic` | `icon::from_svg_bytes(include_bytes!(…))` from `resources/icons` |
| RxTx | `RX` | `TX` | Text, mono 600, 10 px (9 px on vertical panels) |

Size every symbolic icon to the line: 12 px with 12 px text, 14 px with 14 px text, 10 px on vertical panels. Always load them with `.symbolic(true)` and colour them via the series colour.

## 8. States

| State | Trigger | Panel | Popup |
|---|---|---|---|
| Normal | Selected interface up and readable | Live values | Live values |
| Idle | Up, Δ = 0 | `0 B/s` (a real zero) | Zeros; flat graph at the baseline |
| Disconnected | Not up, or no carrier | Indicators and dashes in destructive; flat graph | Caption "Disconnected"; readouts `—`; graph flat, with the reason (`reason-cable-unplugged` when there's no carrier, else `reason-interface-down`) where the span label goes |
| Adapter missing | `Named(x)` not present | Behaves as Automatic | Caption `adapter-missing · using-automatic` in warning. Keep `x` selected in settings, shown as Disconnected, so it resumes when the adapter returns. |
| Read error | Counter read fails twice in a row | As Disconnected | Reason `reason-read-error` |
| No interfaces | Nothing besides `lo` | Graph flat; dashes | Header: `adapter-automatic-none` |

## 9. Accessibility and i18n

- Every string comes from the `.ftl` file; nothing user-visible is hard-coded.
- Direction is never conveyed by colour alone. Indicator shape, the Download/Upload words and the a11y strings carry it.
- Focus order: header → (no focusable readouts) → Applet settings → Network settings. On the settings page: Back → segments → picker → radios.
- Esc closes the popup. On the settings page, the first Esc goes back to Main.

## 10. Performance budget

- Idle CPU below 0.5% of one core, with up to 20 interfaces.
- No allocation per tick in `view` beyond formatting strings.
- Redraw the graph only on new samples.
- Memory: 20 interfaces × 2 × 60 × 16 B is negligible.

## 11. Suggested module layout

```text
src/
  main.rs        // cosmic::applet::run::<App>(())
  app.rs         // Application impl: state, update, view (panel), view_window (popup)
  config.rs      // Config + enums, CosmicConfigEntry
  net/
    mod.rs       // Interface model, enumeration (/sys/class/net)
    sampler.rs   // 1 Hz subscription, ring buffers, totals
    route.rs     // default-route detection
  format.rs      // format_rate / format_compact / format_bytes (+ unit tests)
  widgets/
    rates.rs     // indicator | number | unit row builder
    graph.rs     // canvas::Program for panel sparkline and popup graph
    picker.rs    // indicator picker
  localize.rs    // i18n-embed setup
```
