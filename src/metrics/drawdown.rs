//! Max drawdown and the Calmar ratio.

use chrono::{DateTime, Utc};

use crate::portfolio::EquityPoint;

/// The worst peak-to-trough decline over an equity curve, plus when it
/// happened and whether (and when) equity recovered back to the prior
/// peak by the end of the run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawdownReport {
    /// Magnitude of the decline, as a fraction (e.g. `0.25` = a 25% drop).
    pub max_drawdown: f64,
    pub peak_timestamp: DateTime<Utc>,
    pub trough_timestamp: DateTime<Utc>,
    /// The first time equity climbed back to (or above) the pre-drawdown
    /// peak, if it ever did before the run ended.
    pub recovery_timestamp: Option<DateTime<Utc>>,
}

/// Computes the max drawdown over `equity_curve`. `None` if the curve is
/// empty.
pub fn max_drawdown(equity_curve: &[EquityPoint]) -> Option<DrawdownReport> {
    let first = equity_curve.first()?;

    let mut running_peak = first.equity;
    let mut running_peak_timestamp = first.timestamp;

    let mut worst = DrawdownReport {
        max_drawdown: 0.0,
        peak_timestamp: first.timestamp,
        trough_timestamp: first.timestamp,
        recovery_timestamp: None,
    };
    let mut worst_peak_equity = first.equity;

    for point in equity_curve {
        if point.equity > running_peak {
            running_peak = point.equity;
            running_peak_timestamp = point.timestamp;
        }
        let drawdown = if running_peak > 0.0 {
            (running_peak - point.equity) / running_peak
        } else {
            0.0
        };
        if drawdown > worst.max_drawdown {
            worst.max_drawdown = drawdown;
            worst.peak_timestamp = running_peak_timestamp;
            worst.trough_timestamp = point.timestamp;
            worst_peak_equity = running_peak;
        }
    }

    worst.recovery_timestamp = equity_curve
        .iter()
        .filter(|point| point.timestamp > worst.trough_timestamp)
        .find(|point| point.equity >= worst_peak_equity)
        .map(|point| point.timestamp);

    Some(worst)
}

/// CAGR divided by max drawdown magnitude: return earned per unit of
/// worst-case pain endured. `None` if there was no drawdown at all (the
/// ratio is undefined, not infinite).
pub fn calmar_ratio(cagr: f64, max_drawdown: f64) -> Option<f64> {
    if max_drawdown == 0.0 {
        return None;
    }
    Some(cagr / max_drawdown)
}
