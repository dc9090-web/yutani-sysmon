# System files for `cosmic-vpn-helper`

| File | Install to |
|---|---|
| `io.github.dc.CosmicVpnHelper.conf` | `/usr/share/dbus-1/system.d/` |
| `io.github.dc.CosmicVpnHelper.service` | `/usr/share/dbus-1/system-services/` |
| `cosmic-vpn-helper.service` | `/usr/lib/systemd/system/` |
| `io.github.dc.cosmicvpn.policy` | `/usr/share/polkit-1/actions/` |
| binary | `/usr/lib/cosmic-vpn-helper/cosmic-vpn-helper` |

The helper is started on demand by D-Bus and runs as root, with a limited capability set.

- **No `/home` access:** the applet edits `qBittorrent.conf` and creates the isolated Firefox's profile itself (as you) before asking the helper to launch them; the helper only needs to stat the command it's given.
- **Two namespaces, built the same way:** `vpn-p2p` (`wg-p2p`, conf in `/etc/cosmic-vpn/p2p.conf`) for qBittorrent and `vpn-browser` (`wg-browser`, `/etc/cosmic-vpn/browser.conf`) for Firefox. They share the `0xca6c` firewall mark and its routing rule, which is removed with the last of them.
- **No mount-namespace sandboxing** (`ProtectSystem`, `PrivateTmp`, `ProtectHome`): the namespace is a bind mount under `/run/netns` that systemd and `ip netns` must see on the host. A private mount namespace hides it (and failed to start on CachyOS with `226/NAMESPACE`). Capabilities, address families, `NoNewPrivileges`, W^X and the syscall architecture are still limited.
- **Path:** Arch-based systems have no `/usr/libexec`, so the binary lives under `/usr/lib`.
