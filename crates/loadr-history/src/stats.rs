//! Robust regression detection: median + MAD + modified z-score.
//!
//! Resistant to the one-off CI outlier that would blow up a mean/stddev.

/// The regression verdict for one metric value against its history.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub value: f64,
    pub median: f64,
    pub mad: f64,
    pub z: f64,
    pub sample_n: usize,
    pub regression: bool,
    pub low_confidence: bool,
}

/// Detect a regression of `value` against `history` (prior runs, excluding the
/// run under test). `higher_is_worse` is true for latency/error metrics, false
/// for throughput.
pub fn detect(value: f64, history: &[f64], higher_is_worse: bool) -> Verdict {
    let n = history.len();
    let median = median(history);

    // Cold start: too little history for robust stats — fall back to a ±10%
    // pairwise check and mark it low-confidence.
    if n < 5 {
        let regression = n > 0 && worse_by_pct(value, median, higher_is_worse, 0.10);
        return Verdict {
            value,
            median,
            mad: 0.0,
            z: 0.0,
            sample_n: n,
            regression,
            low_confidence: true,
        };
    }

    let mad = mad(history, median);
    if mad == 0.0 {
        // Identical history — no spread to z-score against; use ±10%.
        let regression = worse_by_pct(value, median, higher_is_worse, 0.10);
        return Verdict {
            value,
            median,
            mad,
            z: 0.0,
            sample_n: n,
            regression,
            low_confidence: false,
        };
    }

    // Iglewicz–Hoaglin modified z-score.
    let z = 0.6745 * (value - median) / mad;
    let regression = if higher_is_worse { z > 3.5 } else { z < -3.5 };
    Verdict {
        value,
        median,
        mad,
        z,
        sample_n: n,
        regression,
        low_confidence: false,
    }
}

fn worse_by_pct(value: f64, median: f64, higher_is_worse: bool, pct: f64) -> bool {
    if median == 0.0 {
        return false;
    }
    if higher_is_worse {
        value > median * (1.0 + pct)
    } else {
        value < median * (1.0 - pct)
    }
}

/// Median of a slice (returns 0.0 for empty).
pub fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Median absolute deviation from the given center.
pub fn mad(xs: &[f64], center: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let devs: Vec<f64> = xs.iter().map(|x| (x - center).abs()).collect();
    median(&devs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_history_no_regression() {
        let h = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0];
        let v = detect(101.0, &h, true);
        assert!(!v.regression, "z={}", v.z);
        assert!(!v.low_confidence);
    }

    #[test]
    fn clear_latency_spike_is_a_regression() {
        let h = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0];
        let v = detect(400.0, &h, true);
        assert!(v.regression, "z={} should exceed 3.5", v.z);
    }

    #[test]
    fn throughput_drop_is_a_regression() {
        let h = vec![1000.0, 1010.0, 990.0, 1005.0, 995.0, 1000.0];
        let v = detect(500.0, &h, false);
        assert!(v.regression, "throughput halved: z={}", v.z);
    }

    #[test]
    fn cold_start_is_low_confidence() {
        let v = detect(200.0, &[100.0, 100.0], true);
        assert!(v.low_confidence);
        assert!(v.regression); // 200 is >10% over the 100 median
    }
}
