//! The monotonic clock events are stamped with (design D8).

use std::sync::OnceLock;

use model::Millis;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

/// Milliseconds on the performance counter: monotonic, and the same in
/// every process on the machine.
pub fn now() -> Millis {
    static FREQUENCY: OnceLock<i64> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut f = 0;
        // SAFETY: writes one i64 through a valid pointer; cannot fail on
        // Windows XP and later.
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    });
    let mut count = 0;
    // SAFETY: as above.
    let _ = unsafe { QueryPerformanceCounter(&mut count) };
    let ms = i128::from(count) * 1000 / i128::from(frequency);
    Millis(u64::try_from(ms).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_decreases() {
        let mut last = now();
        for _ in 0..1000 {
            let t = now();
            assert!(t >= last);
            last = t;
        }
    }
}
