//! Default-route detection from `/proc/net/route` and `/proc/net/ipv6_route`.

/// IPv4: the row with destination `00000000` and the lowest metric.
pub fn ipv4_default(route: &str) -> Option<String> {
    route
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // Iface Destination Gateway Flags RefCnt Use Metric Mask …
            (f.len() > 7 && f[1] == "00000000" && f[7] == "00000000").then(|| (f[6].parse::<u32>().unwrap_or(u32::MAX), f[0]))
        })
        .min_by_key(|(m, _)| *m)
        .map(|(_, i)| i.to_owned())
}

/// IPv6: destination all zeros, prefix length 0, lowest metric. Skips
/// `lo` (the kernel lists unreachable defaults on it).
pub fn ipv6_default(route: &str) -> Option<String> {
    route
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // dest plen src splen nexthop metric refcnt use flags iface
            if f.len() < 10 || f[0].bytes().any(|b| b != b'0') || f[1] != "00" || f[9] == "lo" {
                return None;
            }
            Some((u32::from_str_radix(f[5], 16).unwrap_or(u32::MAX), f[9]))
        })
        .min_by_key(|(m, _)| *m)
        .map(|(_, i)| i.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = include_str!("../../tests/fixtures/route");
    const ROUTE_NONE: &str = include_str!("../../tests/fixtures/route-no-default");
    const IPV6: &str = include_str!("../../tests/fixtures/ipv6_route");

    #[test]
    fn picks_lowest_metric_ipv4() {
        assert_eq!(ipv4_default(ROUTE).as_deref(), Some("enp5s0"));
        assert_eq!(ipv4_default(ROUTE_NONE), None);
        assert_eq!(ipv4_default(""), None);
    }

    #[test]
    fn ipv6_fallback() {
        assert_eq!(ipv6_default(IPV6).as_deref(), Some("wlan0"));
    }
}
