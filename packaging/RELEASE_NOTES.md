Five tiny, native COSMIC panel applets, now with **one package per applet** for Pop!_OS / Ubuntu, Fedora and Arch. Install the ones you want, then add them in *Settings → Desktop → Panel → Applets*: they're all listed as **Yutani …**, so search for *Yutani*.

### ✨ New since 0.1.0

- 🛡️ **VPN applet** *(new)*: two ProtonVPN WireGuard switches: a torrent-only tunnel that only qBittorrent can use (kill switch by construction, NAT-PMP port forwarding pushed into qBittorrent, leak detection) and a NetworkManager tunnel for all web traffic. Mono, Neon or Auto panel icon; either chunk can hide while its tunnel is off.
- 🖥️ **System Monitor**: **NVIDIA** GPUs (proprietary driver, via NVML) and **Intel** integrated GPUs (`i915` / `xe`), alongside AMD. A sleeping laptop dGPU is never woken.
- 🤖 **AI Usage**: its **own Claude sign-in**, kept in the keyring and renewed automatically, so it keeps working when you use Claude elsewhere and never run `claude` on this computer. Claude Code's login still works, read-only.
- 🔎 Every applet is now named **Yutani …** in the applet list.

### 📦 Install

Built on Ubuntu 24.04 for x86_64; runs on Pop!_OS 24.04, Ubuntu 24.04, Fedora, Arch / CachyOS and newer.

| Distro | Install |
| --- | --- |
| Pop!_OS / Ubuntu / Debian | `sudo apt install ./yutani-sysmon_*_amd64.deb` |
| Fedora | `sudo dnf install ./yutani-sysmon-*.x86_64.rpm` |
| Arch / CachyOS / Manjaro | `sudo pacman -U yutani-sysmon-*-x86_64.pkg.tar.zst` |

Swap `sysmon` for `net-traffic`, `ai-usage`, `weather` or `vpn`. `yutani-vpn` also installs its root helper with its D-Bus, systemd and polkit files; removing the package stops the helper and takes down a tunnel left up. On Arch you can instead build from source with [`packaging/arch/PKGBUILD`](https://github.com/dc9090-web/yutani-sysmon/blob/main/packaging/arch/PKGBUILD). `SHA256SUMS` lists every file's checksum.

If a newly installed applet doesn't appear in the list, log out and back in.
