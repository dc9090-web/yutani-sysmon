Five tiny, native COSMIC panel applets, now with **one package per applet** for Pop!_OS / Ubuntu, Fedora and Arch. Install the ones you want, then add them in *Settings → Desktop → Panel → Applets*: they're all listed as **Yutani …**, so search for *Yutani*.

### ✨ New since 0.2.0

- 🤖 **AI Usage**: Claude has dropped the 5-hour window from most plans, so the avatar ring, the first panel bar and the reset countdown now follow the **weekly** limit. The popup's **Session** row is the 7-day, all-models limit with its reset time.
- 💳 **API credits** *(new)*: this billing cycle's Claude Platform spend against the monthly credit that comes with Max, Ultra and Team plans, as a popup row and an optional `API` panel bar. Paste an **Admin API key** in Settings (kept in the keyring, with Test and Remove), set the monthly credit ($100 / $200) and the day it renews. Keys from the new Console (`sk-ant-usr-…`) are accepted.
- ⚙️ Settings: `show_session_reset` is now `show_weekly_reset`; new `show_credits`, `credit_dollars` and `credit_renew_day`. The settings page scrolls inside the popup.

### 📦 Install

Built on Ubuntu 24.04 for x86_64; runs on Pop!_OS 24.04, Ubuntu 24.04, Fedora, Arch / CachyOS and newer.

| Distro | Install |
| --- | --- |
| Pop!_OS / Ubuntu / Debian | `sudo apt install ./yutani-sysmon_*_amd64.deb` |
| Fedora | `sudo dnf install ./yutani-sysmon-*.x86_64.rpm` |
| Arch / CachyOS / Manjaro | `sudo pacman -U yutani-sysmon-*-x86_64.pkg.tar.zst` |

Swap `sysmon` for `net-traffic`, `ai-usage`, `weather` or `vpn`. `yutani-vpn` also installs its root helper with its D-Bus, systemd and polkit files; removing the package stops the helper and takes down a tunnel left up. On Arch you can instead build from source with [`packaging/arch/PKGBUILD`](https://github.com/dc9090-web/yutani-sysmon/blob/main/packaging/arch/PKGBUILD). `SHA256SUMS` lists every file's checksum.

If a newly installed applet doesn't appear in the list, log out and back in.
