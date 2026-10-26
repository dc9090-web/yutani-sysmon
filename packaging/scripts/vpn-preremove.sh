#!/bin/sh
# Stop the helper and take down a tunnel left up, on a real removal only.
# Upgrades pass "upgrade" (deb) or the number of versions left (rpm); Arch
# runs this only on removal and passes the version. No `exit`: Arch
# inlines this into a function.
case "${1:-}" in
    upgrade | deconfigure | [1-9] | [1-9][0-9]) ;;
    *)
        systemctl stop cosmic-vpn-helper.service 2>/dev/null || true
        ip netns del vpn-p2p 2>/dev/null || true
        while ip rule del fwmark 0xca6c priority 100 2>/dev/null; do :; done
        while ip -6 rule del fwmark 0xca6c priority 100 2>/dev/null; do :; done
        rm -rf /etc/netns/vpn-p2p
        ;;
esac
