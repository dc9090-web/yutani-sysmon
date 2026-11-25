#!/bin/sh
# /etc/cosmic-vpn (the torrent and browser confs) is kept.
systemctl daemon-reload 2>/dev/null || true
systemctl reload dbus 2>/dev/null || true
