Five tiny, native COSMIC panel applets, now with **one package per applet** for Pop!_OS / Ubuntu, Fedora and Arch. Install the ones you want, then add them in *Settings → Desktop → Panel → Applets*: they're all listed as **Yutani …**, so search for *Yutani*.

### ✨ New since 0.3.1

- 🤖 **AI Usage**: the API credits row no longer shows "spend format not recognised" on the day your credits renew. Anthropic's cost report only has whole UTC days and refused the request while the new cycle had none; the row now reads $0.00 until the first day is complete. The settings tip and README say plainly that today's spend shows after midnight UTC.
- 🔒 **VPN**: a third switch, **Browser · Firefox**, opens Firefox as its own instance, with its own profile, inside a network namespace with its own WireGuard tunnel. Closing Firefox takes the tunnel down, and turning the switch off closes Firefox.

### 📦 Install

Built on Ubuntu 24.04 for x86_64; runs on Pop!_OS 24.04, Ubuntu 24.04, Fedora, Arch / CachyOS and newer.

| Distro | Install |
| --- | --- |
| Pop!_OS / Ubuntu / Debian | `sudo apt install ./yutani-sysmon_*_amd64.deb` |
| Fedora | `sudo dnf install ./yutani-sysmon-*.x86_64.rpm` |
| Arch / CachyOS / Manjaro | `sudo pacman -U yutani-sysmon-*-x86_64.pkg.tar.zst` |

Swap `sysmon` for `net-traffic`, `ai-usage`, `weather` or `vpn`. `yutani-vpn` also installs its root helper with its D-Bus, systemd and polkit files; removing the package stops the helper and takes down a tunnel left up. On Arch you can instead build from source with [`packaging/arch/PKGBUILD`](https://github.com/dc9090-web/yutani-sysmon/blob/main/packaging/arch/PKGBUILD). `SHA256SUMS` lists every file's checksum.

If a newly installed applet doesn't appear in the list, log out and back in.
