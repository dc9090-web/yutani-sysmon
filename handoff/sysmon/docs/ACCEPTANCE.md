# Acceptance checklist

Run every check in **dark and light**, at panel sizes XS, S and M, on a horizontal top panel and a vertical dock. Compare with `design/screenshots/`.

## Panel

- [ ] A-01: Each style (Numbers, Graph, Both) renders as in `panel-*.png`. The default is Numbers with CPU, GPU and RAM.
- [ ] A-02: The order is always CPU, GPU, RAM, DISK, whatever order the toggles were switched in.
- [ ] A-03: The panel width stays constant while values change (0%↔100%, 0↔1.2G). Measure with screenshots.
- [ ] A-04: Labels are in the metric colours; values are `background.on`. DISK's R/W keys are in the read/write colours.
- [ ] A-05: At vertical S and up, chunks stack centred. At vertical XS, Graph style is forced.
- [ ] A-06: The tooltip lists only the enabled metrics, and adds the "is hot" text when hot.
- [ ] A-07: At Tctl ≥ 85 °C (simulate with fixture or `stress-ng`), the CPU value turns warning. It clears only after Tctl drops below 82 °C (hysteresis).

## Data

- [ ] D-01: CPU % is within ±3 points of `top`/`btop` over 30 s under `stress-ng --cpu 8`.
- [ ] D-02: GPU busy %, VRAM, clock, temps and power match `amdgpu_top` (or `radeontop` plus sysfs) within ±3% or ±2 °C.
- [ ] D-03: Memory used matches `free -b` "used" computed as total − available.
- [ ] D-04: Disk rates are within 5% of `iostat -d 1` during `fio` sequential read and write.
- [ ] D-05: With RAPL root-only, Package power shows `—` and there are no errors in the log after the first warning. With the udev rule, it shows watts within 10% of `turbostat`.
- [ ] D-06: On a machine with no amdgpu, the GPU section is collapsed, the GPU toggle is disabled, and nothing panics.
- [ ] D-07: Idle CPU is below 0.5% of one core (`pidstat 1 30`).

## Popup

- [ ] P-01: The popup is 360px wide with four sections in order and inset dividers (`popup-main-*.png`).
- [ ] P-02: CPU, GPU and Memory graphs use a fixed 0–100 scale with a 50% grid line and no label. The disk graph uses an auto scale with a label.
- [ ] P-03: Meters: VRAM `used / total GiB` and Power `W / cap`. RAM is %; Swap is `used / total`, or "No swap".
- [ ] P-04: On a screen under 1000 logical px tall, the sections scroll and the settings row stays reachable.
- [ ] P-05: Applet settings opens the settings page; Back and Esc return; reopening the popup starts on Main.

## Settings

- [ ] S-01: The togglers apply live. The last enabled toggler is disabled, with the "At least one stays on" caption.
- [ ] S-02: Panel style applies live.
- [ ] S-03: The disk list shows All drives first, then each physical drive with its model and size. Choosing a drive changes the Disk section caption and its data.
- [ ] S-04: The GPU picker appears only when there's more than one amdgpu card.
- [ ] S-05: The config persists across a panel restart, and external edits apply live.

## Code

- [ ] C-01: `just check` and `just test` pass. Parser fixtures cover a Ryzen with k10temp, an RDNA3 dGPU, an APU with no junction sensor, and a box with no swap.
- [ ] C-02: There are no hex literals; every colour comes from `theme.cosmic()`.
- [ ] C-03: Every visible string is in the `.ftl` file.
- [ ] C-04: The code never calls `sudo` or `pkexec`, and never writes to sysfs.
