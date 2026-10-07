//! Number formatting shared by both applets. Ported from the handoffs'
//! `design/prototype/bundle.js` (`formatRate`, `formatCompact`, `formatPct`,
//! …), which is the reference implementation.

/// Shown for any unknown value. Never `0`.
pub const DASH: &str = "—";

const RATE_UNITS: [&str; 4] = ["B/s", "kB/s", "MB/s", "GB/s"];
const BYTE_UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];

/// A number and its unit, kept apart so the panel can set them in separate
/// fixed-width cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// Right-aligned in a 4-character field, e.g. `" 860"`.
    pub num: String,
    pub unit: &'static str,
}

impl Split {
    fn dash() -> Self {
        Self { num: format!("{DASH:>4}"), unit: "" }
    }

    /// The number without its alignment padding.
    pub fn trimmed(&self) -> &str {
        self.num.trim_start()
    }
}

fn scaled(v: f64, units: &[&'static str; 4]) -> Split {
    let mut v = v.max(0.0);
    let mut i = 0;
    while v >= 999.5 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    let s = if i == 0 {
        format!("{}", v.round() as u64)
    } else if v < 9.995 {
        format!("{v:.2}")
    } else if v < 99.95 {
        format!("{v:.1}")
    } else {
        format!("{}", v.round() as u64)
    };
    Split { num: format!("{s:>4}"), unit: units[i] }
}

/// Decimal SI rate, 3 significant figures: `0 B/s`, `9.84 kB/s`, `12.4 MB/s`.
pub fn format_rate(bps: Option<f64>) -> Split {
    match bps {
        Some(v) if v.is_finite() => scaled(v, &RATE_UNITS),
        _ => Split::dash(),
    }
}

/// [`format_rate`] without the `/s`: `4.21 GB`.
pub fn format_bytes(total: Option<f64>) -> Split {
    match total {
        Some(v) if v.is_finite() => scaled(v, &BYTE_UNITS),
        _ => Split::dash(),
    }
}

/// At most 4 characters, for vertical panels: `0`, `860K`, `12M`, `1.2G`.
pub fn format_compact(bps: Option<f64>) -> String {
    let Some(mut v) = bps.filter(|v| v.is_finite()) else {
        return DASH.to_owned();
    };
    v = v.max(0.0);
    let mut i = 0;
    while v >= 999.5 && i < 3 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        return if v < 1.0 { "0".to_owned() } else { format!("{}", v.round() as u64) };
    }
    let suffix = ["", "K", "M", "G"][i];
    if v < 9.95 { format!("{v:.1}{suffix}") } else { format!("{}{suffix}", v.round() as u64) }
}

/// Nice ceiling (1, 2 or 5 × 10ⁿ). Zero or less gives 1000 (1 kB/s).
pub fn nice_max(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return 1000.0;
    }
    let p = 10f64.powf(v.log10().floor());
    let m = v / p;
    let k = if m <= 1.0 {
        1.0
    } else if m <= 2.0 {
        2.0
    } else if m <= 5.0 {
        5.0
    } else {
        10.0
    };
    k * p
}

/// Integer percent, clamped, in a 4-character field: `"  7%"`, `"100%"`.
pub fn format_pct(v: Option<f32>) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{:>4}", format!("{}%", v.clamp(0.0, 100.0).round() as u32)),
        _ => format!("{DASH:>4}"),
    }
}

/// `71 °C`, with a no-break space.
pub fn format_temp(c: Option<f32>) -> String {
    c.filter(|c| c.is_finite()).map_or_else(|| DASH.to_owned(), |c| format!("{}\u{a0}°C", c.round() as i32))
}

/// `4.23 GHz` from MHz.
pub fn format_ghz(mhz: Option<f32>) -> String {
    match mhz.filter(|m| m.is_finite()) {
        Some(m) if m < 10_000.0 => format!("{:.2}\u{a0}GHz", m / 1000.0),
        Some(m) => format!("{:.1}\u{a0}GHz", m / 1000.0),
        None => DASH.to_owned(),
    }
}

/// `212 W`.
pub fn format_w(w: Option<f32>) -> String {
    w.filter(|w| w.is_finite()).map_or_else(|| DASH.to_owned(), |w| format!("{}\u{a0}W", w.round() as i64))
}

/// Binary GiB, 1 decimal: `18.4`.
pub fn format_gib(bytes: Option<u64>) -> String {
    bytes.map_or_else(|| DASH.to_owned(), |b| format!("{:.1}", b as f64 / (1u64 << 30) as f64))
}

/// Decimal size for drive captions: `2.00 TB`, `512 GB`.
pub fn format_size(bytes: u64) -> String {
    let units = ["B", "kB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 999.5 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    let s = if i == 0 || v >= 99.95 {
        format!("{}", v.round() as u64)
    } else if v < 9.995 {
        format!("{v:.2}")
    } else {
        format!("{v:.1}")
    };
    format!("{s}\u{a0}{}", units[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(v: f64) -> (String, &'static str) {
        let s = format_rate(Some(v));
        (s.num, s.unit)
    }

    #[test]
    fn rate_examples() {
        assert_eq!(rate(0.0), ("   0".into(), "B/s"));
        assert_eq!(rate(9840.0), ("9.84".into(), "kB/s"));
        assert_eq!(rate(12.4e6), ("12.4".into(), "MB/s"));
        assert_eq!(rate(860e3), (" 860".into(), "kB/s"));
        assert_eq!(rate(15.0e6), ("15.0".into(), "MB/s"));
        assert_eq!(rate(487e3), (" 487".into(), "kB/s"));
        assert_eq!(rate(1.2e6), ("1.20".into(), "MB/s"));
        assert_eq!(rate(686e3), (" 686".into(), "kB/s"));
        assert_eq!(rate(20e6), ("20.0".into(), "MB/s"));
        assert_eq!(rate(2e9), ("2.00".into(), "GB/s"));
    }

    #[test]
    fn rate_edges() {
        // B/s is always an integer; 999.5 rolls over.
        assert_eq!(rate(999.4), (" 999".into(), "B/s"));
        assert_eq!(rate(999.5), ("1.00".into(), "kB/s"));
        assert_eq!(rate(12.3), ("  12".into(), "B/s"));
        assert_eq!(rate(-5.0), ("   0".into(), "B/s"));
        // Every value fits the 4-character field.
        for e in 0..10 {
            for m in [1.0, 2.5, 9.99, 99.9, 999.4, 999.6] {
                assert_eq!(format_rate(Some(m * 10f64.powi(e))).num.chars().count(), 4);
            }
        }
    }

    #[test]
    fn rate_unknown_is_dash() {
        let s = format_rate(None);
        assert_eq!(s.trimmed(), "—");
        assert_eq!(s.num.chars().count(), 4);
        assert_eq!(format_rate(Some(f64::NAN)).trimmed(), "—");
    }

    #[test]
    fn compact_examples() {
        assert_eq!(format_compact(Some(0.0)), "0");
        assert_eq!(format_compact(Some(0.4)), "0");
        assert_eq!(format_compact(Some(860e3)), "860K");
        assert_eq!(format_compact(Some(12e6)), "12M");
        assert_eq!(format_compact(Some(15e6)), "15M");
        assert_eq!(format_compact(Some(487e3)), "487K");
        assert_eq!(format_compact(Some(1.2e9)), "1.2G");
        assert_eq!(format_compact(Some(1.8e6)), "1.8M");
        assert_eq!(format_compact(Some(339e3)), "339K");
        assert_eq!(format_compact(None), "—");
        for e in 0..9 {
            for m in [1.0, 9.94, 9.96, 99.9, 999.4, 999.6] {
                assert!(format_compact(Some(m * 10f64.powi(e))).chars().count() <= 4);
            }
        }
    }

    #[test]
    fn bytes_examples() {
        let b = format_bytes(Some(4.21e9));
        assert_eq!((b.trimmed(), b.unit), ("4.21", "GB"));
        let b = format_bytes(Some(312e6));
        assert_eq!((b.trimmed(), b.unit), ("312", "MB"));
        let b = format_bytes(Some(38.2e9));
        assert_eq!((b.trimmed(), b.unit), ("38.2", "GB"));
    }

    #[test]
    fn nice() {
        assert_eq!(nice_max(0.0), 1000.0);
        assert_eq!(nice_max(1.0), 1.0);
        assert_eq!(nice_max(1.5e7), 2e7);
        assert_eq!(nice_max(3e6), 5e6);
        assert_eq!(nice_max(6e6), 1e7);
        assert_eq!(nice_max(2e3), 2e3);
    }

    #[test]
    fn pct_examples() {
        assert_eq!(format_pct(Some(7.0)), "  7%");
        assert_eq!(format_pct(Some(23.0)), " 23%");
        assert_eq!(format_pct(Some(100.0)), "100%");
        assert_eq!(format_pct(Some(140.0)), "100%");
        assert_eq!(format_pct(Some(-3.0)), "  0%");
        assert_eq!(format_pct(None), "   —");
    }

    #[test]
    fn unit_examples() {
        assert_eq!(format_temp(Some(71.2)), "71\u{a0}°C");
        assert_eq!(format_temp(None), "—");
        assert_eq!(format_ghz(Some(4230.0)), "4.23\u{a0}GHz");
        assert_eq!(format_ghz(Some(3670.0)), "3.67\u{a0}GHz");
        assert_eq!(format_w(Some(212.4)), "212\u{a0}W");
        assert_eq!(format_w(None), "—");
        assert_eq!(format_gib(Some((18.4 * (1u64 << 30) as f64) as u64)), "18.4");
        assert_eq!(format_gib(None), "—");
        assert_eq!(format_size(2_000_398_934_016), "2.00\u{a0}TB");
        assert_eq!(format_size(512_110_190_592), "512\u{a0}GB");
    }
}
