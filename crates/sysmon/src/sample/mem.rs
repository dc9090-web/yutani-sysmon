//! `/proc/meminfo`.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mem {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

impl Mem {
    pub fn ram_pct(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.used as f64 / self.total as f64 * 100.0) as f32)
    }
}

/// Values in bytes. `None` if MemTotal or MemAvailable is missing.
pub fn parse(text: &str) -> Option<Mem> {
    let (mut total, mut avail, mut st, mut sf) = (None, None, 0, 0);
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(key), Some(v)) = (it.next(), it.next().and_then(|v| v.parse::<u64>().ok())) else { continue };
        let b = v * 1024;
        match key {
            "MemTotal:" => total = Some(b),
            "MemAvailable:" => avail = Some(b),
            "SwapTotal:" => st = b,
            "SwapFree:" => sf = b,
            _ => {}
        }
    }
    let (total, avail) = (total?, avail?);
    Some(Mem { total, used: total.saturating_sub(avail), swap_total: st, swap_used: st.saturating_sub(sf) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let m =
            parse("MemTotal:       65731868 kB\nMemFree: 1 kB\nMemAvailable:   53291428 kB\nSwapTotal:      8388604 kB\nSwapFree:       7969276 kB\n").unwrap();
        assert_eq!(m.total, 65731868 * 1024);
        assert_eq!(m.used, (65731868 - 53291428) * 1024);
        assert_eq!(m.swap_used, (8388604 - 7969276) * 1024);
        assert!((m.ram_pct().unwrap() - 18.93).abs() < 0.01);
    }

    #[test]
    fn real_fixtures() {
        let m = parse(include_str!("../../tests/fixtures/meminfo-ryzen")).unwrap();
        assert!(m.total > 0 && m.used <= m.total);
        let n = parse(include_str!("../../tests/fixtures/meminfo-noswap")).unwrap();
        assert_eq!((n.swap_total, n.swap_used), (0, 0));
        assert_eq!(parse("garbage"), None);
    }
}
