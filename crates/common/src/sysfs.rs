//! Small, allocation-light readers for procfs and sysfs files.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Read a short file (a sysfs attribute) into a stack buffer and hand the
/// trimmed text to `f`. Attributes longer than 128 bytes are cut off.
pub fn with_small<T>(path: impl AsRef<Path>, f: impl FnOnce(&str) -> Option<T>) -> io::Result<Option<T>> {
    let mut buf = [0u8; 128];
    let mut file = File::open(path)?;
    let mut n = 0;
    loop {
        match file.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => {
                n += k;
                if n == buf.len() {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let s = std::str::from_utf8(&buf[..n]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(f(s.trim()))
}

/// An unsigned integer attribute, e.g. `rx_bytes`.
pub fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    with_small(path, |s| s.parse().ok()).ok().flatten()
}

/// A signed integer attribute, e.g. `speed` (which may be -1).
pub fn read_i64(path: impl AsRef<Path>) -> Option<i64> {
    with_small(path, |s| s.parse().ok()).ok().flatten()
}

/// A short text attribute, trimmed.
pub fn read_string(path: impl AsRef<Path>) -> Option<String> {
    with_small(path, |s| Some(s.to_owned())).ok().flatten()
}

/// Read a whole file into `buf`, reusing its allocation.
pub fn read_into(path: impl AsRef<Path>, buf: &mut String) -> io::Result<()> {
    buf.clear();
    File::open(path)?.read_to_string(buf)?;
    Ok(())
}

/// Hex attribute such as `0x1002`.
pub fn read_hex(path: impl AsRef<Path>) -> Option<u32> {
    with_small(path, |s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).ok().flatten()
}

/// A monotonic counter turned into a per-second rate and a running total.
/// A counter that goes backwards (wrap or reset) gives one `None` and
/// re-baselines.
#[derive(Debug, Clone, Default)]
pub struct Counter {
    last: Option<u64>,
    /// Sum of positive deltas since the first reading.
    pub total: u64,
}

impl Counter {
    /// Feed a new reading taken `dt` seconds after the previous one.
    pub fn update(&mut self, now: Option<u64>, dt: f64) -> Option<f64> {
        let prev = self.last;
        self.last = now;
        let (prev, now) = (prev?, now?);
        if now < prev || dt <= 0.0 {
            return None;
        }
        let d = now - prev;
        self.total += d;
        Some(d as f64 / dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_rates_and_resets() {
        let mut c = Counter::default();
        assert_eq!(c.update(Some(1000), 1.0), None);
        assert_eq!(c.update(Some(3000), 2.0), Some(1000.0));
        assert_eq!(c.update(Some(10), 1.0), None);
        assert_eq!(c.update(Some(510), 0.5), Some(1000.0));
        assert_eq!(c.total, 2500);
        assert_eq!(c.update(None, 1.0), None);
        assert_eq!(c.update(Some(600), 1.0), None);
    }
}
