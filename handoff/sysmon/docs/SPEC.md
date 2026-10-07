# System Monitor applet: functional spec

Version 1.0 · target: COSMIC panel (libcosmic `main`, applet API as in pop-os/cosmic-applets 1.10). This is a sibling of the Network Traffic applet; it shares its design system and patterns.

## 1. Scope

A COSMIC panel applet that shows **CPU**, **GPU (AMD / amdgpu only)**, **Memory** and **Disk I/O**:

- **Panel:** one labelled chunk per enabled metric.
- **Popup:** a detail section per metric, with temperatures, clocks and power.

**Out of scope for v1:**
- NVIDIA or Intel GPUs
- Per-core CPU grid
- Per-process lists
- Disk space
- Fan speeds
- History beyond 60 s
- Alerts or notifications
- °F

## 2. Identity

| Item | Value |
|---|---|
| App ID | `io.github.dc.CosmicAppletSysMon` (placeholder; change before publishing) |
| Binary / crate | `cosmic-applet-sysmon` |
| Desktop entry | `resources/io.github.dc.CosmicAppletSysMon.desktop` |
| Applet icon | `resources/icons/hicolor/scalable/apps/<APP_ID>-symbolic.svg` (cosmic-icons `utilities-system-monitor-symbolic`) |
| Strings | `i18n/en/cosmic_applet_sysmon.ftl` |

## 3. Data sources

Sample everything once per second in one subscription (`cosmic::iced::time::every(Duration::from_secs(1))`; check the path against the libcosmic version you pin). Compute rates from `Instant` deltas, never by assuming exactly 1 s.

- Each metric keeps a 60-sample ring buffer of `Option<f32>`.
- `None` means unreadable, and draws as a gap.
- Re-discover devices (GPU, drives, hwmon paths) at start, every 30 s, and when the popup opens.
- Cache resolved sysfs paths. Never glob on every tick.

### 3.1 CPU

| Reading | Source | Rule |
|---|---|---|
| Usage % | `/proc/stat`, first line `cpu` | `busy = total − (idle + iowait)`; `usage = Δbusy / Δtotal × 100` |
| Model | `/proc/cpuinfo` `model name` (first) | Trim; collapse spaces; drop a trailing "N-Core Processor". |
| Cores / threads | `/proc/cpuinfo` | Threads = number of `processor` entries. Cores = unique (`physical id`, `core id`) pairs. |
| Clock (avg) | `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq` (kHz) | Mean over online CPUs, shown in GHz. If absent, show `—`. |
| Tctl | hwmon whose `name` is `k10temp`: the `temp*_input` whose `temp*_label` is `Tctl` (m°C) | Fallback: `coretemp` "Package id 0". Otherwise `—`. |
| Package power | `/sys/class/powercap/intel-rapl:0/energy_uj` (also used for AMD Zen) | `W = ΔµJ / Δt / 1e6`. Handle wrap with `max_energy_range_uj`. **The file is root-only (0400) by default.** If `EACCES`, show `—`, and the a11y text says "needs permission". README documents an optional udev rule. Never prompt for privilege escalation. |

### 3.2 GPU (amdgpu)

**Discovery:** each `/sys/class/drm/card[0-9]*` whose `device/driver` symlink basename is `amdgpu`. Skip `card*-*` connector entries.

- **Several cards:** default to the one with the most VRAM (`mem_info_vram_total`). Setting `gpu` overrides this, by PCI slot (the basename of `device` realpath, e.g. `0000:03:00.0`).
- **No amdgpu card:** GPU is unavailable (see §7).

| Reading | Source (under `card/device/`) | Rule |
|---|---|---|
| Busy % | `gpu_busy_percent` | Integer 0–100 |
| VRAM | `mem_info_vram_used`, `mem_info_vram_total` (bytes) | GiB, 1 decimal |
| Name | `/usr/share/hwdata/pci.ids` (or `/usr/share/misc/pci.ids`), looked up by `vendor`/`device`/`subsystem_*` | Fallback: `AMD GPU (1002:xxxx)` |
| Clock | `hwmon/hwmon*/freq1_input` (Hz; label `sclk`) | GHz, 2 decimals. Fallback: the `*` line of `pp_dpm_sclk`. |
| Edge temp | hwmon `temp*_input` with label `edge` | |
| Junction temp | hwmon `temp*_input` with label `junction` | Some APUs lack it, so show `—`. |
| Power | hwmon `power1_average`, else `power1_input` (µW) | W, integer |
| Power cap | hwmon `power1_cap` (µW) | Drives the meter maximum. Without it, there's no meter and only the W stat shows. |

### 3.3 Memory

From `/proc/meminfo` (kB):
- Used = `MemTotal − MemAvailable`. RAM % = Used / MemTotal.
- Swap used = `SwapTotal − SwapFree`.
- If `SwapTotal = 0`, the Swap meter reads "No swap" with an empty track.

Display memory in binary GiB, 1 decimal.

### 3.4 Disk I/O

**Drives:** each `/sys/block/*` that has a `device` symlink. Exclude `loop*`, `ram*`, `zram*`, `dm-*` and `md*` (v1 counts physical drives only).

- **Reads and writes:** from `/sys/block/<d>/stat`, field 3 (sectors read) and field 7 (sectors written), × 512 bytes (always 512 in this file, whatever the hardware sector size).
- **Rates:** Δbytes / Δt. **"All drives"** is the sum over physical drives (the default).
- **Model:** `device/model` (trim). Size = `size` × 512, shown in decimal TB/GB.
- **NVMe temp:** the hwmon under `/sys/block/<d>/device/hwmon*` (or `/sys/class/nvme/nvmeN/hwmon*`), `temp1_input` ("Composite"). With All drives, show the hottest NVMe.
- **Totals since login:** the sum of positive deltas since the applet started.

## 4. Formatting

| Function | Rule | Examples |
|---|---|---|
| `format_pct(v)` | Round; clamp 0–100; right-align in a 4-character field | `"  7%"`, `" 23%"`, `"100%"` |
| `format_temp(c)` | Integer, U+00A0, `°C` | `71 °C` |
| `format_ghz(mhz)` | /1000, 2 decimals | `4.23 GHz` |
| `format_w(w)` | Integer | `212 W` |
| `format_gib(bytes)` | /2³⁰, 1 decimal | `18.4` |
| Disk rates | Same as the network applet's `format_rate` / `format_compact` (decimal SI) | `1.20 MB/s`, `1.8M` |
| Unknown | Em dash | `—` |

`design/prototype/bundle.js` holds the reference implementation (`formatPct`, `formatTemp`, `formatGHz`, `formatW`, `formatGiB`, `formatRate`, `formatCompact`). Port it and unit-test it against these examples.

## 5. Panel button

- **Build:** `button::custom(row![chunks].spacing(space_xs)).class(Button::AppletIcon)`, padded with `core.applet.suggested_padding(true)`, inside `core.applet.autosize_window`. This is the same approach as the network applet.
- **Chunks:** one per enabled metric, **always in the order CPU, GPU, RAM, DISK**.
- **Panel style** (setting `style`; default `Numbers`):
  - **Numbers:** `column![label, value]`
  - **Graph:** `column![label, sparkline 32×15]`
  - **Both:** `row![sparkline 32×20, column![label, value]]`
- **Label:** mono 10/12, weight 600, in the metric colour. DISK's label is muted, because its R/W keys carry the colour.
- **Value:** mono 12/17, right-aligned in a fixed 4-character field.
- **DISK value:** `row![R, compact(read), W, compact(write)]`, with the keys in `disk-read` / `disk-write` (mono 10, weight 600), each rate fixed at 4 characters, and 6px before `W`.
- **Vertical panel (size S and up):** chunks stack (`column`, spacing `space_xxs`), centred. Label 9/11, value 11/14; DISK stacks its R and W lines. At **vertical XS**, show Graph style regardless of the setting.
- **Width:** constant for a given set of metrics, style and panel size.
- **Warning:** the value turns `warning` when the metric's temperature is hot (see §7).
- **Tooltip and accessible name:** `a11y-summary` (only the enabled metrics), plus `a11y-hot` when hot.
- **Click:** toggles the popup.

## 6. Popup

`core.applet.popup_container(content.padding([8, 0, 8, 0]))`, 360px wide.

- **Pages:** `Main` and `Settings`. The popup always opens on Main.
- **Height:** about 920px with all four sections. If the output's logical height is under 1000px, wrap the sections in a `scrollable`.

### 6.1 Main page

The four sections appear in the order CPU, GPU, Memory, Disk, **regardless of the panel toggles**, separated by inset dividers. Then an inset divider and a `menu_button` with ⚙ "Applet settings" ›.

Each section (padding `space_xxs` × `space_m`, internal spacing `space_xxs`) has:

1. **Top row.** Left: an 8px dot in the metric colour and the name (`text::heading`), with a caption under it. Right: the headline value in mono 24/32 bold.
2. **Graph:** 312 × 64 `canvas` in a `bg-component` well with `radius_s`.
   - CPU, GPU and Memory: area plus stroke in the metric colour, fixed 0–100 scale, one grid line at 50%, no label.
   - Disk: read area plus write line, shared nice-max scale, label top-right.
3. **Details**, by section:

| Section | Caption | Headline | Details |
|---|---|---|---|
| CPU | `<model> · <cores> cores / <threads> threads` | usage % | Stats: Clock (avg), Tctl, Package power |
| GPU | `<name> · amdgpu` | busy % | Meters: VRAM `used / total GiB`, Power `W / cap W`. Stats: Clock, Edge, Junction |
| Memory | `<total> GiB total` | used GiB + "GiB used" | Meters: RAM %, Swap `used / total GiB` |
| Disk I/O | `All drives`, or `<dev> · <model>` | none | Read and Write readouts (network RateReadout style). Stats: Read since login, Written, NVMe temp |

- **Stats:** a 3-column grid. Key in `caption` muted; value in mono 14/20.
- **Meters:** a 2-column grid. Label and value row, then a 6px bar with `radius_xl` ends: track `bg-component`, fill in the metric colour.
- **Meter build:** `progress_bar::determinate_linear(frac)` restyled, or two stacked containers; choose whichever matches the design more closely.

### 6.2 Settings page

1. A Back button (`go-previous-symbolic`) and the title "Applet settings", then a full-width divider.
2. **Show in panel:** 4 rows, each `[8px colour dot] [name / caption] [toggler]`.
   - The defaults are CPU, GPU and RAM on, Disk off.
   - The last enabled toggler is disabled, with the caption `at-least-one`.
   - GPU is disabled when unavailable, with the caption `gpu-unavailable`.
3. **Panel style:** a segmented control with Numbers, Graph, Both.
4. **Disk:** a radio list of `menu_button` rows. "All drives" comes first (caption "Sum of physical drives"), then each drive (caption `<model> · <size>`).
5. **GPU** (only if more than one amdgpu card): a radio list showing each card's name and PCI slot.

Changes apply immediately and persist straight away.

## 7. Thresholds and states

| Condition | Rule | Panel | Popup |
|---|---|---|---|
| CPU hot | Tctl ≥ 85 °C | CPU value in `warning` | Tctl value in `warning`; key "Tctl · Hot" |
| GPU hot | junction ≥ `temp2_crit` − 10, else ≥ 95 °C; or edge ≥ 85 °C | GPU value in `warning` | That stat in `warning` with "· Hot" |
| NVMe hot | `temp1_input` ≥ `temp1_max`, else ≥ 70 °C | none | NVMe temp in `warning` with "· Hot" |
| RAM high | ≥ 90% | none | RAM meter fill in `warning` |
| VRAM high | ≥ 95% | none | VRAM meter fill in `warning` |
| No amdgpu | discovery is empty | GPU chunk removed; toggle disabled | GPU section collapses to the top row: caption `gpu-unavailable`, headline `—` |
| Unreadable reading | read error or missing file | `—` | That stat shows `—`, and the layout stays the same |
| Counter reset | value decreased | one `None` sample | gap in the graph |

Hysteresis: a warning clears only after the reading has dropped 3 °C (or 3 percentage points) below its threshold, so values don't flicker.

## 8. Settings (cosmic-config, version 1)

```text
Config {
  show_cpu: bool = true
  show_gpu: bool = true        // ignored when no amdgpu
  show_mem: bool = true
  show_disk: bool = false
  style: PanelStyle = Numbers  // Numbers | Graph | Both
  disk: DiskChoice = All       // All | Named(String)   e.g. "nvme0n1"
  gpu: GpuChoice = Auto        // Auto | Slot(String)   e.g. "0000:03:00.0"
}
```

**Invariant:** at least one `show_*` is true. If a loaded config has none, force `show_cpu = true`. If `Named(d)` is missing, use All and show the warning caption `disk-missing`.

## 9. Accessibility and i18n

- Every string comes from the `.ftl` file.
- Each metric colour always has its text label (CPU, GPU, RAM, DISK, R, W).
- Meters expose their value as text.
- Focus order: sections have no focusable elements, then Applet settings. On the settings page: Back → togglers → segments → disk radios → GPU radios.
- Esc closes the popup. On the settings page, Esc goes back to Main first.

## 10. Performance budget

- Idle CPU below 0.5% of one core.
- Reads per tick: `/proc/stat`, `/proc/meminfo`, each `/sys/block/*/stat`, and the cached GPU and hwmon files. That's under 30 small reads.
- `pci.ids` is parsed once, lazily, for the selected GPU only.

## 11. Suggested module layout

```text
src/
  main.rs, app.rs, config.rs, localize.rs
  sample/
    mod.rs        // Sampler: 1 Hz subscription, ring buffers, totals
    cpu.rs        // /proc/stat, cpuinfo, cpufreq, k10temp/coretemp, RAPL
    gpu.rs        // amdgpu discovery + readings, pci.ids lookup
    mem.rs        // /proc/meminfo
    disk.rs       // /sys/block discovery + stat, nvme hwmon
    hwmon.rs      // find hwmon by name/label, cached paths
  format.rs       // + unit tests
  widgets/
    chunk.rs      // panel metric chunk
    section.rs    // popup metric section
    meter.rs, stats.rs, graph.rs (canvas: single series + read/write)
```
