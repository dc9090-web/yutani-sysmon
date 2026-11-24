## Panel
panel-label = TS
panel-on = ON
panel-off = OFF
panel-connecting = …
panel-error = ERR
tooltip-connected = Tailscale connected
tooltip-connecting = Tailscale connecting
tooltip-off = Tailscale off
tooltip-login = Tailscale needs a sign-in
tooltip-approval = Tailscale is waiting for approval
tooltip-other-user = Tailscale is in use by another user
tooltip-unavailable = Tailscale isn't running
tooltip-exit = · exit node { $name }
tooltip-devices = · { $online } of { $total } devices online
tooltip-attention = · needs attention

## Header
title = Tailscale
head-connected = Connected
head-connected-tailnet = Connected · { $tailnet }
head-trouble = Connected, but not working
head-connecting = Connecting…
head-off = Off
head-login = Sign-in needed
head-approval = Waiting for approval
head-other-user = In use by another user
head-unavailable = Tailscale isn't running

## Banners
operator-title = Tailscale needs a one-time permission
operator-body = Switching on/off and choosing an exit node are reserved for the operator. Run this once:
not-running-title = Tailscale isn't running
not-running-body = Start the Tailscale service:
signin-title = Sign-in needed
signin-body = This device isn't signed in to a tailnet.
signin-button = Sign in
key-expired = This device's key has expired. Sign in again to reconnect.
key-expiring = This device's key expires in { $days } days.
key-expiring-today = This device's key expires today.
health-more = +{ $count } more
err-timeout = Tailscale didn't answer. Try again.
err-request = Tailscale refused the change: { $text }
copy = Copy
copied = Copied { $text }

## This device
this-device = This device
address = Address
name = Name
exit-node = Exit node
exit-none = None
exit-offline = { $name } · offline
exit-changing = Changing…

## Devices
devices-heading = Devices · { $online } of { $total } online
devices-none = No other devices in this tailnet
devices-no-match = No devices match
search-devices = Search devices
conn-direct = Direct
conn-peer-relay = Peer relay
conn-relay = Relay · { $code }
conn-idle = Idle
conn-last-seen = Last seen { $ago }
conn-offline = Offline

## Exit node page
back = Back
exit-recommended = Recommended: { $name }
exit-own = This tailnet
exit-offline-seen = Offline · last seen { $ago }
exit-offline-plain = Offline
exit-no-options = No device in this tailnet offers an exit node
exit-no-match = No exit nodes match
search-exit = Search exit nodes
exit-lan-title = Allow local network access
exit-lan-caption = Keep printers and other LAN devices reachable while using an exit node

## Settings
settings = Applet settings
sec-panel = Panel
sec-devices = Devices
set-status = Show status in panel
set-status-caption = The ON / OFF word next to the icon. Connecting and errors always show.
set-offline = Show offline devices
set-offline-caption = List devices that aren't connected right now
set-copy = Clicking a device copies
copy-address = Address
copy-name = Name
