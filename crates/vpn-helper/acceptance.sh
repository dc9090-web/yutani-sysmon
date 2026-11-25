#!/bin/sh
# Live acceptance checks for the torrent tunnel (ACCEPTANCE.md, "Security and
# leaks"), or with `browser` as the argument, the browser tunnel. Run with
# sudo while that tunnel is on. Prints IPs and routes only; never a key. The
# kill-switch test briefly blackholes the tunnel's endpoint, then restores it.
set -u
[ "$(id -u)" -eq 0 ] || { echo "run with sudo" >&2; exit 1; }
case "${1:-p2p}" in
    p2p) NS=vpn-p2p; IF=wg-p2p; CONF=/etc/cosmic-vpn/p2p.conf; WHAT=torrent ;;
    browser) NS=vpn-browser; IF=wg-browser; CONF=/etc/cosmic-vpn/browser.conf; WHAT=browser ;;
    *) echo "usage: $0 [p2p|browser]" >&2; exit 2 ;;
esac
ip netns list | grep -q "^$NS" || { echo "the $WHAT tunnel is off" >&2; exit 1; }
EP=$(sed -n 's/^[Ee]ndpoint *= *//p' "$CONF" | sed 's/:[0-9]*$//; s/^\[//; s/\]$//')
ADDR=$(sed -n 's/^[Aa]ddress *= *//p' "$CONF" | tr ',' '\n' | sed 's/ //g; s|/.*||' | grep -v : | head -1)
ok() { echo "  PASS  $*"; }
bad() { echo "  FAIL  $*"; }
inns() { ip netns exec "$NS" "$@"; }

echo "1. Exit IPs"
HOST4=$(curl -4 -s --max-time 10 https://ifconfig.me); NS4=$(inns curl -4 -s --max-time 10 https://ifconfig.me)
NS6=$(inns curl -6 -s --max-time 10 https://ifconfig.me || true)
echo "  host IPv4      $HOST4"
echo "  $WHAT IPv4   ${NS4:-none}"
echo "  $WHAT IPv6   ${NS6:-none}"
[ -n "$NS4" ] && [ "$NS4" != "$HOST4" ] && ok "the $WHAT tunnel exits through its own IP" || bad "$WHAT exit IP"

echo "2. Namespace contents (only lo and $IF, routes only via $IF)"
inns ip -br link | sed 's/^/  /'
inns ip route | sed 's/^/  v4 /'; inns ip -6 route | sed 's/^/  v6 /'
[ "$(inns ip -br link | awk '{print $1}' | sed 's/@.*//' | sort | tr '\n' ' ')" = "lo $IF " ] && ok "no way out but the tunnel" || bad "unexpected interfaces"

echo "3. The tunnel's own packets bypass the web tunnel"
ip route get "$EP" mark 0xca6c | head -1 | sed 's/^/  /'

echo "4. Conf file"
ls -l "$CONF" | sed 's/^/  /'
[ "$(stat -c '%a %U %G' "$CONF")" = "600 root root" ] && ok "0600 root:root" || bad "permissions"

echo "5. Kill switch: blackholing the endpoint $EP for 15 s"
ip route add blackhole "$EP/32"
sleep 3
if inns curl -4 -s --max-time 8 https://ifconfig.me >/dev/null; then bad "traffic still left the namespace"; else ok "nothing leaves the namespace with the tunnel down"; fi
if command -v tcpdump >/dev/null; then
    NIC=$(ip route get 1.1.1.1 mark 0xca6c | sed -n 's/.* dev \([^ ]*\).*/\1/p')
    echo "  watching $NIC for 8 s for anything from the namespace's addresses…"
    N=$(timeout 8 tcpdump -ni "$NIC" -c 50 "src host ${ADDR:-10.2.0.2}" 2>/dev/null | wc -l)
    [ "$N" -eq 0 ] && ok "no tunnel-inside packets on $NIC" || bad "$N packets with the tunnel's address on $NIC"
fi
ip route del blackhole "$EP/32"
echo "  restored; waiting for a fresh handshake…"
for i in $(seq 1 30); do inns curl -4 -s --max-time 5 https://ifconfig.me >/dev/null && break; sleep 1; done
inns curl -4 -s --max-time 8 https://ifconfig.me >/dev/null && ok "tunnel recovered" || bad "tunnel didn't recover"
echo "Done. Send the output back; it contains no keys."
