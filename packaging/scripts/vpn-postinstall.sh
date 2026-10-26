#!/bin/sh
# Pick up the helper's unit and D-Bus policy; restart a running helper so
# it uses the new binary. The helper itself starts on demand.
systemctl daemon-reload 2>/dev/null || true
systemctl reload dbus 2>/dev/null || busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig 2>/dev/null || true
systemctl try-restart cosmic-vpn-helper.service 2>/dev/null || true
