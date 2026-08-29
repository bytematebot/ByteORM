//! Latency statistics over a sample vector.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub samples: usize,
    /// Operations per second, computed from the wall clock of the whole
    /// measured phase, so it reflects concurrency instead of 1/mean.
    pub ops_per_sec: f64,
    pub mean_us: f64,
    pub stddev_us: f64,
    pub min_us: f64,
    pub p50_us: f64,
    pub p90_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub wall_ms: f64,
}

impl Stats {
    pub fn from_samples(mut samples: Vec<Duration>, wall: Duration) -> Self {
        assert!(!samples.is_empty(), "no samples");
        samples.sort_unstable();
        let us: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1e6).collect();
        let n = us.len();
        let mean = us.iter().sum::<f64>() / n as f64;
        let var = us.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
        let wall_s = wall.as_secs_f64();

        Self {
            samples: n,
            ops_per_sec: if wall_s > 0.0 { n as f64 / wall_s } else { 0.0 },
            mean_us: mean,
            stddev_us: var.sqrt(),
            min_us: us[0],
            p50_us: percentile(&us, 0.50),
            p90_us: percentile(&us, 0.90),
            p95_us: percentile(&us, 0.95),
            p99_us: percentile(&us, 0.99),
            max_us: us[n - 1],
            wall_ms: wall.as_secs_f64() * 1e3,
        }
    }
}

/// Nearest-rank percentile on an already sorted slice.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!(percentile(&v, 0.50), 50.0);
        assert_eq!(percentile(&v, 0.99), 99.0);
        assert_eq!(percentile(&v, 1.0), 100.0);
    }

    #[test]
    fn stats_report_throughput_from_wall_clock() {
        let samples = vec![Duration::from_millis(1); 10];
        let s = Stats::from_samples(samples, Duration::from_millis(5));
        assert_eq!(s.samples, 10);
        assert!((s.ops_per_sec - 2000.0).abs() < 1.0);
        assert!((s.p50_us - 1000.0).abs() < 1.0);
    }
}
