//! Setting qBittorrent's listening port in its `qBittorrent.conf` before it
//! starts (SPEC §5.2). Only the listed keys change; order, comments and
//! everything else are kept.

/// Which key layout a file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// qBittorrent ≥ 4.4: `[BitTorrent] Session\Port`, `[Network] PortForwardingEnabled`.
    Session,
    /// Older: `[Preferences] Connection\PortRangeMin`, `Connection\UPnP`.
    Legacy,
}

/// Where each key lives: (section, key) for the port, then for the UPnP switch.
fn keys(layout: Layout) -> ((&'static str, &'static str), (&'static str, &'static str, &'static str)) {
    match layout {
        Layout::Session => (("BitTorrent", "Session\\Port"), ("Network", "PortForwardingEnabled", "false")),
        Layout::Legacy => (("Preferences", "Connection\\PortRangeMin"), ("Preferences", "Connection\\UPnP", "false")),
    }
}

/// The index of each line's section.
fn sections(lines: &[&str]) -> Vec<Option<String>> {
    let mut cur = None;
    lines
        .iter()
        .map(|l| {
            let t = l.trim();
            if t.starts_with('[') && t.ends_with(']') {
                cur = Some(t[1..t.len() - 1].to_owned());
            }
            cur.clone()
        })
        .collect()
}

fn find(lines: &[&str], secs: &[Option<String>], section: &str, key: &str) -> Option<usize> {
    lines.iter().enumerate().find(|(i, l)| secs[*i].as_deref() == Some(section) && l.split_once('=').is_some_and(|(k, _)| k.trim() == key)).map(|(i, _)| i)
}

pub fn detect(text: &str) -> Option<Layout> {
    let lines: Vec<&str> = text.lines().collect();
    let secs = sections(&lines);
    [Layout::Session, Layout::Legacy].into_iter().find(|l| {
        let ((s, k), _) = keys(*l);
        find(&lines, &secs, s, k).is_some()
    })
}

/// `text` with the listening port set and qBittorrent's own UPnP/NAT-PMP
/// off, or `None` if neither layout is recognised.
pub fn set_listen_port(text: &str, port: u16) -> Option<String> {
    let layout = detect(text)?;
    let ((port_sec, port_key), (upnp_sec, upnp_key, upnp_val)) = keys(layout);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let refs: Vec<&str> = text.lines().collect();
    let secs = sections(&refs);
    let i = find(&refs, &secs, port_sec, port_key)?;
    lines[i] = format!("{port_key}={port}");
    match find(&refs, &secs, upnp_sec, upnp_key) {
        Some(j) => lines[j] = format!("{upnp_key}={upnp_val}"),
        None => {
            // Add the key at the end of its section, or a new section at the end.
            let header = refs.iter().position(|l| l.trim() == format!("[{upnp_sec}]"));
            match header {
                Some(h) => {
                    let mut end = h + 1;
                    while end < refs.len() && secs[end].as_deref() == Some(upnp_sec) && !refs[end].trim().is_empty() {
                        end += 1;
                    }
                    lines.insert(end, format!("{upnp_key}={upnp_val}"));
                }
                None => {
                    if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                        lines.push(String::new());
                    }
                    lines.push(format!("[{upnp_sec}]"));
                    lines.push(format!("{upnp_key}={upnp_val}"));
                }
            }
        }
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/qbittorrent/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn both_layouts_match_expected_files() {
        for (src, want) in
            [("qBittorrent-4.6.conf", "qBittorrent-4.6.expected-53186.conf"), ("qBittorrent-4.3-legacy.conf", "qBittorrent-4.3-legacy.expected-53186.conf")]
        {
            assert_eq!(set_listen_port(&fixture(src), 53186).as_deref(), Some(fixture(want).as_str()), "{src}");
        }
        assert_eq!(detect(&fixture("qBittorrent-4.6.conf")), Some(Layout::Session));
        assert_eq!(detect(&fixture("qBittorrent-4.3-legacy.conf")), Some(Layout::Legacy));
    }

    #[test]
    fn unknown_layout_is_left_alone() {
        assert_eq!(set_listen_port("[Preferences]\nWebUI\\Port=8080\n", 53186), None);
        assert_eq!(set_listen_port("", 1), None);
    }

    #[test]
    fn missing_upnp_key_is_added_in_its_section() {
        // qBittorrent 5.x writes [Network] without PortForwardingEnabled until it's changed.
        let src = "[BitTorrent]\nSession\\Port=27201\n\n[Network]\nCookies=@Invalid()\n\n[Preferences]\nGeneral\\Locale=en\n";
        let out = set_listen_port(src, 53186).unwrap();
        assert_eq!(
            out,
            "[BitTorrent]\nSession\\Port=53186\n\n[Network]\nCookies=@Invalid()\nPortForwardingEnabled=false\n\n[Preferences]\nGeneral\\Locale=en\n"
        );
        // No [Network] section at all: one is added at the end.
        let out = set_listen_port("[BitTorrent]\nSession\\Port=1\n", 2).unwrap();
        assert_eq!(out, "[BitTorrent]\nSession\\Port=2\n\n[Network]\nPortForwardingEnabled=false\n");
    }
}
