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
