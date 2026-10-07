# Acceptance checklist

Run every check in **both** the dark and light themes, and at panel sizes XS, S and M, on a horizontal top panel and a vertical dock. Compare against `design/screenshots/`.

## Panel

- [ ] A-01: The three modes render as in `panel-*.png`. Both is the default.
- [ ] A-02: The panel button's width is identical for values `0 B/s`, `9.84 kB/s`, `12.4 MB/s` and `—` (measure with a screenshot).
- [ ] A-03: **The down and up indicators line up vertically** in every indicator style, at every panel size, and in the popup totals.
- [ ] A-04: Only the indicators are coloured: `accent_blue` for down, `accent_orange` for up. Numbers use `background.on`.
- [ ] A-05: On a vertical panel at size S and up, compact numbers (`860K`, `12M`) show. At vertical XS, only the graph shows.
- [ ] A-06: The tooltip and accessible name read like "Network traffic: download 12.4 MB/s, upload 860 kB/s (enp5s0)".
- [ ] A-07: Hover and open states use the AppletIcon pill fill. The keyboard focus ring is visible.

## Data

- [ ] D-01: Rates match `nload` / `bmon` within 5% under a steady `iperf3` load.
- [ ] D-02: Unplugging Ethernet shows Disconnected within 2 s. Replugging resumes without restarting the applet.
- [ ] D-03: With Automatic selected, switching the default route (e.g. Ethernet → Wi-Fi) changes the interface the applet follows within 5 s.
- [ ] D-04: Removing a selected USB adapter shows the "not found · using Automatic" warning. Reinserting it resumes that adapter.
- [ ] D-05: A counter wrap or reset produces one empty sample, not a spike.
- [ ] D-06: `lo` is never listed.
- [ ] D-07: Idle CPU is below 0.5% of one core (`pidstat 1 30`).

## Popup

- [ ] P-01: The popup is 360 px wide, with radius M, a 1 px divider border and no shadow (`popup-main-*.png`).
- [ ] P-02: The graph has a shared scale, download as area plus stroke, upload as line only, 3 grid lines, the scale label top-right and the span label bottom-left.
- [ ] P-03: The graph's history is kept when switching adapters (all interfaces are sampled).
- [ ] P-04: "Applet settings" opens the settings page; Back and Esc return to Main; reopening the popup starts on Main.
- [ ] P-05: "Network settings…" opens COSMIC Settings → Network and closes the popup.

## Settings

- [ ] S-01: Mode, indicator and adapter changes apply to the panel within one frame and survive a panel restart.
- [ ] S-02: The indicator picker shows all 6 options; the selected one has an accent fill; the caption names it.
- [ ] S-03: The adapter list is ordered: Automatic, physical, then virtual. Disconnected interfaces are selectable and labelled.
- [ ] S-04: Editing the config file externally (`~/.config/cosmic/<APP_ID>/v1/`) updates the applet live.

## Code

- [ ] C-01: `just check` and `just test` pass.
- [ ] C-02: There are no hex colours or pixel spacing literals outside `format.rs` tests and the documented 360/312/120 graph and popup sizes.
- [ ] C-03: Every visible string is in the `.ftl` file.
