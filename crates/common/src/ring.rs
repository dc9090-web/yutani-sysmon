//! Fixed 60-slot history. No allocation after construction.

/// Samples kept per series: 60 s at 1 Hz.
pub const HISTORY: usize = 60;

/// A ring of `Option<f32>`; `None` is a gap (unreadable or counter reset).
#[derive(Debug, Clone)]
pub struct Ring {
    buf: [Option<f32>; HISTORY],
    /// Index of the next write.
    head: usize,
    len: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Self { buf: [None; HISTORY], head: 0, len: 0 }
    }
}

impl Ring {
    pub fn push(&mut self, v: Option<f32>) {
        self.buf[self.head] = v;
        self.head = (self.head + 1) % HISTORY;
        self.len = (self.len + 1).min(HISTORY);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = Option<f32>> + '_ {
        let start = (self.head + HISTORY - self.len) % HISTORY;
        (0..self.len).map(move |i| self.buf[(start + i) % HISTORY])
    }

    pub fn last(&self) -> Option<f32> {
        if self.len == 0 { None } else { self.buf[(self.head + HISTORY - 1) % HISTORY] }
    }

    /// Largest known sample, or 0.
    pub fn max(&self) -> f32 {
        self.iter().flatten().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_oldest_first() {
        let mut r = Ring::default();
        for i in 0..65 {
            r.push(Some(i as f32));
        }
        assert_eq!(r.len(), HISTORY);
        assert_eq!(r.iter().next(), Some(Some(5.0)));
        assert_eq!(r.last(), Some(64.0));
        assert_eq!(r.max(), 64.0);
    }

    #[test]
    fn gaps() {
        let mut r = Ring::default();
        r.push(Some(1.0));
        r.push(None);
        assert_eq!(r.iter().collect::<Vec<_>>(), vec![Some(1.0), None]);
        assert_eq!(r.last(), None);
    }
}
