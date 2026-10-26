Five COSMIC panel applets, one package each: **Network Traffic**, **System Monitor**, **AI Usage**, **Weather** and **VPN**. Install the ones you want, then add them in *Settings → Desktop → Panel → Applets* (search for **Yutani**).

Built on Ubuntu 24.04 for x86_64; runs on Pop!_OS 24.04, Ubuntu 24.04, Fedora, Arch and newer.

| Distro | Install |
| --- | --- |
| Pop!_OS / Ubuntu / Debian | `sudo apt install ./yutani-sysmon_*_amd64.deb` |
| Fedora | `sudo dnf install ./yutani-sysmon-*.x86_64.rpm` |
| Arch / CachyOS / Manjaro | `sudo pacman -U yutani-sysmon-*-x86_64.pkg.tar.zst` |

Swap `sysmon` for `net-traffic`, `ai-usage`, `weather` or `vpn`. `yutani-vpn` also installs its root helper with its D-Bus, systemd and polkit files; removing the package stops the helper and takes down a tunnel left up. `SHA256SUMS` lists every file's checksum.

If a newly installed applet doesn't appear in the list, log out and back in.
