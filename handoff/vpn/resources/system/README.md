# System files for `cosmic-vpn-helper`

| File | Install to |
|---|---|
| `io.github.dc.CosmicVpnHelper.conf` | `/usr/share/dbus-1/system.d/` |
| `io.github.dc.CosmicVpnHelper.service` | `/usr/share/dbus-1/system-services/` |
| `cosmic-vpn-helper.service` | `/usr/lib/systemd/system/` |
| `io.github.dc.cosmicvpn.policy` | `/usr/share/polkit-1/actions/` |
| binary | `/usr/libexec/cosmic-vpn-helper` |

The helper is started on demand by D-Bus and runs as root.

- **Hardening:** set in the unit.
- **`ReadWritePaths` includes `/home`:** only so the helper can edit `qBittorrent.conf` after dropping to the user's uid (SPEC §5.2).
- **`/home` alternative:** remove it and do that edit in the applet instead, if you prefer. The helper then only receives the port.
