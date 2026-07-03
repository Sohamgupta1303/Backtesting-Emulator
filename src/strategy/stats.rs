//! Small statistics helpers shared across reference strategies.

use crate::data::types::Bar;

/// The average closing price over the last `period` bars.
///
/// Panics if `bars.len() < period` — callers are expected to already know
/// they have enough history (typically by checking against their own
/// `warmup_bars()`) before calling this.
pub fn simple_moving_average(bars: &[Bar], period: usize) -> f64 {
    let start = bars.len() - period;
    bars[start..].iter().map(|bar| bar.close).sum::<f64>() / period as f64
}

/// The arithmetic mean of `values`. Panics if `values` is empty.
pub fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// The *population* standard deviation of `values` (divides by `n`, not
/// `n - 1`) — chosen over the sample variant so a `lookback` of 1 doesn't
/// divide by zero, and because this is describing the spread of a
/// specific rolling window, not estimating a larger population's spread
/// from a sample of it. Panics if `values` is empty.
pub fn population_stddev(values: &[f64]) -> f64 {
    let avg = mean(values);
    let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}
