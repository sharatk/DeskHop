//! Monotonic time, supplied by the host with every event.

/// Milliseconds on a monotonic clock. Only differences are meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Millis(pub u64);

impl Millis {
    /// Milliseconds from `earlier` to `self`, or 0 if `earlier` is later.
    pub const fn since(self, earlier: Millis) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}
