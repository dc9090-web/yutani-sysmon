# VPN applet: design spec

The source of truth is `design/tokens.json`, which holds libcosmic's stock theme values. `design/prototype/` renders every screen, and `design/screenshots/` holds the target renders at 2×.

**Rule 1:** read values from `theme.cosmic()` at runtime. The hex values are for checking screenshots only.

## 1. Colour

- **No VPN-specific tokens.** Everything maps to shared roles:

| Meaning | Token | libcosmic |
|---|---|---|
| ON, Connected, Running in VPN, kill-switch lock, ✓ | `success` | `cosmic.success.base` |
| Stale handshake, no port forwarding, Set manually, Moderate NAT, amber dot | `warning` | `cosmic.warning.base` |
| ERR, leak banner title, Running outside VPN, red dot | `destructive` | `cosmic.destructive.base` |
| OFF, captions, keys, muted notes | `on-bg-muted` | (see tokens.json; neutral_7 / neutral_6) |
| Rates: download chevron / upload chevron | `net-down` / `net-up` | `palette.accent_blue` / `accent_orange` (same as the Network applet) |
| Inset boxes (torrent details, leak banner, banners) | `bg-component` | `cosmic.background.component.base` |
| Flag chips | `small-widget` | `cosmic.background.small_widget` |

- **Status is never colour alone:** every coloured state has a word (`ON`, `ERR`, "Unavailable", "Running outside VPN").
- **Warning vs destructive:** in dark mode `warning` (#ffa37d) and `destructive` (#ffa09a) are nearly the same hue. The destructive cases always add the ⚠ icon, a bold title and the word "outside" or "failed". Keep it that way.

## 2. Type

| Style | Size / line | Weight | Font | Use |
|---|---|---|---|---|
| heading | 14/21 | 700 | sans | Popup title, card titles |
| body | 14/21 | 400 | sans | Values in the torrent box; settings labels |
| caption | 12/17 | 400 | sans | Server line, keys, notes, help text |
| status | 12/17 | 600 | sans | "Connected · 12s ago" etc. (off = 400 muted) |
| port-big | 16/22 | 700 | mono | Forwarded port in the card |
| chunk label / value | 10/12 600 / 12/17 600 | — | mono | Panel `P2P` `WEB` `PORT` |
| chip | 11/18 | 600 | sans | Flag chips in settings |
| field | 13/20 | 400 | mono | Web UI port / user / password inputs |

## 3. Panel

```text
 ╭──────────────────────────────╮
 │ [⌜»⌟•]  P2P  WEB  PORT       │   chunk = label over value, fixed min width
 │         ON   OFF  53186      │   (3ch, 3ch, 5ch) so nothing jumps
 ╰──────────────────────────────╯
```

**Icon:**
- `resources/icons/vpn-reticle-{off,p2p,web,both}-symbolic.svg` at `suggested_size(true)` (S 20, M 28, L 32, XL 48), single ink `background.on`.
- The icons are drawn on a 16 grid: brackets 1.3px with square caps; a lit chevron is a solid band, an unlit one a 0.98px line; both brackets close in 1px when both tunnels are up. **Install them to `$PREFIX/share/icons/hicolor/scalable/apps/`** and load them by name with `icon::from_name(..).symbolic(true)`.

**Chunks and spacing:**
- Gap between elements: `space_xxs` (8).
- Chunk value colours follow the table in §1. `…` (connecting) uses `on-bg` at 400.

**Issue dot:**
- 7px at the icon's top-right (offset −3, −3), with a 2px `background.base` halo.
- Red = `destructive`, amber = `warning`.

**Vertical panel:** a column of icon, P2P, WEB, PORT, each centred, gap 4.

**Hover / press:** `Button::AppletIcon`.

## 4. Popup (360 wide, `popup_container`)

```text
┌──────────────────────── 360 ────────────────────────┐
│ [⌜»⌟24] ProtonVPN · WireGuard                        │ header, pad 8/24
│         Both tunnels on            (success caption) │
│ [⚠ qBittorrent is running outside the VPN        ]   │ leak banner (only when outside)
│ [  It was started normally… [Restart in VPN]     ]   │
│ ═════════════════════════════════════════════════════ │ full-bleed divider
│ [⤓20] Torrents · qBittorrent              [■■■○] 48×24│ card head
│       NL#256 · Amsterdam · P2P                       │
│ Connected · 12s ago            ⌄ 4.80 MB/s ⌃ 610 kB/s│ status row
│ ┌──────────────────────────────────────────────────┐ │ box: bg-component, radius 8,
│ │ Forwarded port   53186 [⧉]  ↻ 12s ago            │ │      pad 8/12, row gap 6,
│ │ qBittorrent      ● Running in VPN                │ │      row min-height 28,
│ │ Listening port   ✓ Set to 53186                  │ │      key column 92
│ └──────────────────────────────────────────────────┘ │
│ 🔒 Kill switch on · qBittorrent can only reach …     │ note 12/17 muted, lock 12 success
│ ─────────────────────────────────────────────────    │ inset divider
│ [🌐20] All web traffic                    [■■■○]     │
│        CH#12 · Zurich · NetworkManager               │
│ Connected · 31s ago            ⌄ 1.20 MB/s ⌃ 240 kB/s│
│ 🔒 Torrents keep their own tunnel and port.          │ (only when both on)
│ ─────────────────────────────────────────────────    │
│ ⚙ Applet settings                                 ›  │ menu_button
└──────────────────────────────────────────────────────┘
```

**Cards:**
- Padding 8 vertical / 24 horizontal; gap 8 between rows.
- The torrent box is shown only when `on` / `stale`.
- `stale` dims the box and rates to 50% opacity.

**Toggler:**
- The standard libcosmic `toggler` (48×24).
- While `connecting`, render it on at 60% opacity and ignore input.

**Status row:** status text on the left. Rates on the right with the Network applet's `ratesHTML` formatting (4-char number field, decimal SI).

**Forwarded port row:**
- The port in `port-big`.
- Copy is an `icon_button` (28, icon 14, `edit-copy-symbolic`).
- Renewal: `view-refresh` glyph + "12s ago" caption (tooltip "NAT-PMP lease renewed every 45 s").

**qBittorrent row:**
- 8px dot + text.
- `Launch in VPN` is a small standard button (12/20, padding 2/12), right-aligned, with `media-playback-start-symbolic` at 10px.

**Leak banner:**
- `bg-component`, radius 8, margin 0 24 8.
- Columns: icon 16 (`dialog-warning`, destructive) | text.
- Title in body 600 destructive. Body in caption muted. Then the `Restart in VPN` button (small).

**Toast:** inverse pill (`on-bg` fill, `bg` text, 12/17 600), bottom-centre of the popup, 1.4 s.

## 5. Settings page (in-popup)

- **Header:** back `icon_button`, "Applet settings" heading, full-bleed divider.
- **Section headings:** caption-heading muted, padding 4/24.
- **Config row:**
  - Columns: icon 20 | text column | replace `icon_button` (28, `document-open-symbolic`).
  - Text column: title body; file name caption **mono**; "label · endpoint" caption.
  - Then the chips row (gap 4).
- **Empty config row:** "No config imported" caption + `Import .conf` small button.
- **Toggle rows:** `settings::item` style. Title body, caption muted below, toggler right. Padding 8/24.
- **Fields:** label body on the left, input 120×28 on the right. Input: `bg-component`, radius 8, mono 13, focus border `accent`.
- **Status line:** caption, colour by result (`success` / `destructive`), padding 4/24.
- **Help text:** caption muted, padding 0/24.

## 6. Icons used

| Where | Icon | Source |
|---|---|---|
| Panel, popup header | `vpn-reticle-*-symbolic` | **custom**, `resources/icons/` |
| Torrent card | `folder-download-symbolic` | COSMIC theme |
| Web card | `web-browser-symbolic` | COSMIC theme |
| Kill-switch note | `connection-secure-symbolic` | COSMIC theme |
| Copy | `edit-copy-symbolic` | COSMIC theme |
| Import / replace | `document-open-symbolic` | COSMIC theme |
| Launch | `media-playback-start-symbolic` | COSMIC theme |
| Leak / errors | `dialog-warning-symbolic` | COSMIC theme |
| Settings / back / next | `emblem-system`, `go-previous`, `go-next` | COSMIC theme |

- COSMIC icons are loaded by name. `design/icons/` holds copies (CC-BY-SA 4.0) for reference only.
- **Applet-list icon:** `vpn-reticle-both-symbolic`.

## 7. Accessibility

- **Switches:** `role=switch` with labels "Torrent tunnel" / "Web tunnel"; busy while connecting.
- **Leak banner:** announced (`role=alert`).
- **Port copy:** button label "Copy port".
- **Focus order:**
  - Popup: header → leak button → torrent switch → copy → launch → web switch → settings.
  - Settings: top to bottom.
- Every state text works without colour (§1).
