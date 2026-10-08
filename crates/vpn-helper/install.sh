#!/bin/sh
# Installs (or with `--uninstall`, removes) the VPN applet's root helper and
# its D-Bus, systemd and polkit files. Run as root from the repository root
# after `cargo build --release -p cosmic-vpn-helper`.
set -eu
[ "$(id -u)" -eq 0 ] || { echo "run with sudo" >&2; exit 1; }
cd "$(dirname "$0")/../.."
SYS=crates/vpn-helper/resources/system
ICONS=crates/vpn/resources/icons

if [ "${1:-}" = "--uninstall" ]; then
    systemctl stop cosmic-vpn-helper.service 2>/dev/null || true
    # The namespace and its rules, if a tunnel was left up.
    ip netns del vpn-p2p 2>/dev/null || true
    while ip rule del fwmark 0xca6c priority 100 2>/dev/null; do :; done
    while ip -6 rule del fwmark 0xca6c priority 100 2>/dev/null; do :; done
    rm -rf /etc/netns/vpn-p2p
    rm -f /usr/lib/cosmic-vpn-helper/cosmic-vpn-helper /usr/share/dbus-1/system.d/io.github.dc.CosmicVpnHelper.conf \
        /usr/share/dbus-1/system-services/io.github.dc.CosmicVpnHelper.service /usr/lib/systemd/system/cosmic-vpn-helper.service \
        /usr/share/polkit-1/actions/io.github.dc.cosmicvpn.policy
    rmdir /usr/lib/cosmic-vpn-helper 2>/dev/null || true
    for i in off p2p web both; do rm -f /usr/share/icons/hicolor/scalable/apps/vpn-reticle-$i-symbolic.svg; done
    systemctl daemon-reload
    systemctl reload dbus 2>/dev/null || true
    echo "Removed. /etc/cosmic-vpn (the torrent conf) is kept; delete it yourself if you want."
    exit 0
fi

[ -x target/release/cosmic-vpn-helper ] || { echo "build it first: cargo build --release -p cosmic-vpn-helper" >&2; exit 1; }
install -Dm0755 target/release/cosmic-vpn-helper /usr/lib/cosmic-vpn-helper/cosmic-vpn-helper
install -Dm0644 $SYS/io.github.dc.CosmicVpnHelper.conf /usr/share/dbus-1/system.d/io.github.dc.CosmicVpnHelper.conf
install -Dm0644 $SYS/io.github.dc.CosmicVpnHelper.service /usr/share/dbus-1/system-services/io.github.dc.CosmicVpnHelper.service
install -Dm0644 $SYS/cosmic-vpn-helper.service /usr/lib/systemd/system/cosmic-vpn-helper.service
install -Dm0644 $SYS/io.github.dc.cosmicvpn.policy /usr/share/polkit-1/actions/io.github.dc.cosmicvpn.policy
for i in off p2p web both; do install -Dm0644 $ICONS/vpn-reticle-$i-symbolic.svg /usr/share/icons/hicolor/scalable/apps/vpn-reticle-$i-symbolic.svg; done
systemctl daemon-reload
# The bus must re-read its policy for the new name.
systemctl reload dbus 2>/dev/null || busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig
# Restart a running helper so it picks up the new binary.
systemctl try-restart cosmic-vpn-helper.service
echo "Installed. The helper starts on demand."
