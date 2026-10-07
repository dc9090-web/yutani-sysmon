# System Monitor applet — English strings. Keys are referenced from docs/SPEC.md.

applet-name = System Monitor
applet-settings = Applet settings
back = Back

# Panel labels (keep ≤ 4 characters)
label-cpu = CPU
label-gpu = GPU
label-ram = RAM
label-disk = DISK
key-read = R
key-write = W

# Sections
cpu = CPU
gpu = GPU
memory = Memory
disk-io = Disk I/O
cpu-caption = { $model } · { $cores } cores / { $threads } threads
gpu-caption = { $name } · amdgpu
mem-caption = { $total } GiB total
gib-used = GiB used
all-drives = All drives
all-drives-caption = Sum of physical drives
drive-caption = { $model } · { $size }

# Stats and meters
clock-avg = Clock (avg)
clock = Clock
tctl = Tctl
package-power = Package power
edge = Edge
junction = Junction
vram = VRAM
power = Power
ram = RAM
swap = Swap
no-swap = No swap
read = Read
write = Write
read-since-login = Read since login
written = Written
nvme-temp = NVMe temp
hot-suffix = { $key } · Hot

# Settings
show-in-panel = Show in panel
usage-pct = Usage %
ram-used-pct = RAM used %
read-write = Read / write
at-least-one = At least one stays on
panel-style = Panel style
style-numbers = Numbers
style-graph = Graph
style-both = Both
disk = Disk
gpu-picker = GPU

# States
gpu-unavailable = No supported GPU found · AMD (amdgpu) only
disk-missing = { $dev } not found · using All drives
needs-permission = Needs permission

# Accessible name / tooltip (only enabled metrics are joined, comma-separated)
a11y-cpu = CPU { $pct }%
a11y-gpu = GPU { $pct }%
a11y-ram = RAM { $pct }%
a11y-disk = disk read { $read }, write { $write }
a11y-hot = { $metric } is hot ({ $temp })
