//! Lock-free histogram for the timing traces (input latency, period jitter), written by the
//! TX thread and read by anyone.

use std::sync::atomic::{AtomicU64, Ordering};

const BUCKETS: usize = 400;
const BUCKET_US: u64 = 25;

/// 25 µs buckets up to 10 ms; larger values land in the last bucket
pub struct Histogram {
    buckets: [AtomicU64; BUCKETS],
}

impl Default for Histogram {
    fn default() -> Self {
        Self { buckets: std::array::from_fn(|_| AtomicU64::new(0)) }
    }
}

impl Histogram {
    pub fn record_us(&self, value_us: u64) {
        let index = ((value_us / BUCKET_US) as usize).min(BUCKETS - 1);
        self.buckets[index].fetch_add(1, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).sum()
    }

    /// Upper edge of the bucket holding the given percentile (0-100)
    pub fn percentile_us(&self, percentile: f64) -> Option<u64> {
        let counts: Vec<u64> = self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).collect();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return None;
        }
        let target = ((total as f64) * percentile / 100.0).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (index, count) in counts.iter().enumerate() {
            seen += count;
            if seen >= target {
                return Some((index as u64 + 1) * BUCKET_US);
            }
        }
        None
    }

    pub fn reset(&self) {
        self.buckets.iter().for_each(|b| b.store(0, Ordering::Relaxed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let h = Histogram::default();
        assert_eq!(h.percentile_us(99.0), None);
        for _ in 0..99 {
            h.record_us(100);
        }
        h.record_us(5000);
        assert_eq!(h.count(), 100);
        assert_eq!(h.percentile_us(50.0), Some(125));
        assert_eq!(h.percentile_us(99.0), Some(125));
        assert_eq!(h.percentile_us(100.0), Some(5025));
        h.record_us(1_000_000);
        assert_eq!(h.percentile_us(100.0), Some(10_000));
    }
}
