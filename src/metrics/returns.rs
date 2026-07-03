//! Return- and risk-based metrics: total return, CAGR, volatility, Sharpe,
//! and Sortino.

use crate::portfolio::EquityPoint;

/// Trading days per year for daily-frequency US equity data — the most
/// common annualization factor, but *not* universal. Every function here
/// takes `periods_per_year` explicitly rather than assuming this value,
/// so callers using weekly, monthly, or intraday data aren't silently
/// given a wrong answer.
pub const TRADING_DAYS_PER_YEAR: f64 = 252.0;

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// *Sample* standard deviation (divides by `n - 1`), the standard choice
/// when treating `values` as a sample used to estimate the volatility of
/// an underlying process — as opposed to the *population* variant used in
/// `strategy::stats`, which describes a specific window's own spread.
/// Returns `0.0` for fewer than two values (a single point has no spread
/// to estimate).
fn sample_stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let variance =
        values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

/// Overall return from `initial_cash` to the final equity point, as a
/// fraction (e.g. `0.25` = +25%). `0.0` if the equity curve is empty.
pub fn total_return(equity_curve: &[EquityPoint], initial_cash: f64) -> f64 {
    let Some(last) = equity_curve.last() else {
        return 0.0;
    };
    (last.equity - initial_cash) / initial_cash
}

/// Compound Annual Growth Rate: the constant annualized growth rate that
/// would produce the same overall return over the same number of
/// periods. `-1.0` (total loss) if equity ended at or below zero; `0.0`
/// if the equity curve is empty.
pub fn cagr(equity_curve: &[EquityPoint], initial_cash: f64, periods_per_year: f64) -> f64 {
    let Some(last) = equity_curve.last() else {
        return 0.0;
    };
    let num_periods = equity_curve.len() as f64;
    let total_growth = last.equity / initial_cash;
    if total_growth <= 0.0 {
        return -1.0;
    }
    total_growth.powf(periods_per_year / num_periods) - 1.0
}

/// Per-period returns implied by the equity curve, starting from
/// `initial_cash` (so there are exactly as many returns as equity
/// points — the first return covers `initial_cash -> equity_curve[0]`).
pub fn period_returns(equity_curve: &[EquityPoint], initial_cash: f64) -> Vec<f64> {
    let mut previous = initial_cash;
    let mut returns = Vec::with_capacity(equity_curve.len());
    for point in equity_curve {
        returns.push(if previous == 0.0 {
            0.0
        } else {
            (point.equity - previous) / previous
        });
        previous = point.equity;
    }
    returns
}

/// Annualized volatility: the sample standard deviation of per-period
/// returns, scaled up to a yearly figure by `sqrt(periods_per_year)`.
pub fn annualized_volatility(returns: &[f64], periods_per_year: f64) -> f64 {
    sample_stddev(returns) * periods_per_year.sqrt()
}

/// Annualized Sharpe ratio: excess return over `risk_free_rate` (an
/// annual rate; `0.0` is a common simplifying default) per unit of
/// volatility. `None` if there are too few returns or the returns have
/// zero variance (a flat equity curve) — the ratio is undefined there,
/// not zero.
pub fn sharpe_ratio(returns: &[f64], risk_free_rate: f64, periods_per_year: f64) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let rf_per_period = risk_free_rate / periods_per_year;
    let excess: Vec<f64> = returns.iter().map(|r| r - rf_per_period).collect();
    let stdev = sample_stddev(&excess);
    if stdev == 0.0 {
        return None;
    }
    Some((mean(&excess) / stdev) * periods_per_year.sqrt())
}

/// Annualized Sortino ratio: like [`sharpe_ratio`], but the denominator
/// only counts *downside* deviation (periods below the risk-free rate),
/// per the standard convention of dividing the sum of squared downside
/// deviations by the *total* number of periods, not just the down ones.
/// `None` if there are too few returns or there is no downside deviation
/// at all (either everything is flat, or every period beat the risk-free
/// rate — the ratio is undefined, not infinite or zero).
pub fn sortino_ratio(returns: &[f64], risk_free_rate: f64, periods_per_year: f64) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let rf_per_period = risk_free_rate / periods_per_year;
    let excess: Vec<f64> = returns.iter().map(|r| r - rf_per_period).collect();
    let downside_variance = excess
        .iter()
        .filter(|e| **e < 0.0)
        .map(|e| e.powi(2))
        .sum::<f64>()
        / excess.len() as f64;
    let downside_deviation = downside_variance.sqrt();
    if downside_deviation == 0.0 {
        return None;
    }
    Some((mean(&excess) / downside_deviation) * periods_per_year.sqrt())
}
