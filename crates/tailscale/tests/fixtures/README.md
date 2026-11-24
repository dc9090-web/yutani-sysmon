# Fixtures

Shaped like tailscaled 1.104.7's local API replies (`/localapi/v0/status`, one line per `watch-ipn-bus` message, and the 403 body of a refused prefs write), but every value is invented: hosts `alpha`…`golf`, tailnet `example.ts.net`, addresses from `100.64.0.0/24`, IDs `nAAAA<n>CNTRL`, all-zero node keys, owner `user@example.com`. Nothing from a real tailnet is in here; keep it that way.

`status_running.json` has one direct peer (bravo, also an exit-node option), one relayed (charlie), one idle (delta) and three offline. `status_mullvad.json` adds five location-based exit nodes in two countries.
