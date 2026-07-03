//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use chrono::{TimeZone, Utc};

use super::returns::{
    annualized_volatility, cagr, period_returns, sharpe_ratio, sortino_ratio, total_return,
};
use crate::portfolio::EquityPoint;

fn point(day: u32, equity: f64) -> EquityPoint {
    EquityPoint {
        timestamp: Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap(),
        equity,
        cash: equity,
        gross_exposure: 0.0,
    }
}

#[test]
fn total_return_is_zero_on_an_empty_curve() {
    assert_eq!(total_return(&[], 10_000.0), 0.0);
}

#[test]
fn total_return_hand_computed() {
    let curve = [point(1, 11_000.0), point(2, 12_000.0)];
    // (12,000 - 10,000) / 10,000 = 0.20
    assert_eq!(total_return(&curve, 10_000.0), 0.20);
}

#[test]
fn cagr_hand_computed_over_two_periods() {
    // Total growth 1.21x over 2 periods, annualized to 4 periods/year:
    // 1.21^(4/2) - 1 = 1.21^2 - 1 = 1.4641 - 1 = 0.4641
    let curve = [point(1, 11_000.0), point(2, 12_100.0)];
    let result = cagr(&curve, 10_000.0, 4.0);
    assert!((result - 0.4641).abs() < 1e-9);
}

#[test]
fn cagr_is_negative_one_on_total_wipeout() {
    let curve = [point(1, 0.0)];
    assert_eq!(cagr(&curve, 10_000.0, 252.0), -1.0);
}

#[test]
fn period_returns_hand_computed() {
    let curve = [point(1, 11_000.0), point(2, 9_900.0)];
    let returns = period_returns(&curve, 10_000.0);
    // 10,000 -> 11,000: +10%; 11,000 -> 9,900: -10%
    assert_eq!(returns.len(), 2);
    assert!((returns[0] - 0.10).abs() < 1e-9);
    assert!((returns[1] - (-0.10)).abs() < 1e-9);
}

#[test]
fn annualized_volatility_hand_computed() {
    // Returns [0.10, -0.10, 0.10, -0.10]: mean = 0, sample variance
    // (n-1=3) = (0.01*4)/3 = 0.013333..., stdev = 0.11547...
    let returns = [0.10, -0.10, 0.10, -0.10];
    let vol = annualized_volatility(&returns, 1.0); // periods_per_year=1 isolates the raw stdev
    assert!((vol - 0.115470054).abs() < 1e-6);
}

#[test]
fn sharpe_ratio_is_none_for_a_flat_equity_curve() {
    let returns = [0.0, 0.0, 0.0];
    assert_eq!(sharpe_ratio(&returns, 0.0, 252.0), None);
}

#[test]
fn sharpe_ratio_is_none_with_fewer_than_two_returns() {
    assert_eq!(sharpe_ratio(&[0.01], 0.0, 252.0), None);
}

#[test]
fn sharpe_ratio_hand_computed() {
    // Returns [0.02, -0.01, 0.03, 0.0], risk-free rate 0 (so excess ==
    // returns). mean = 0.01; sample stdev (n-1=3): deviations from mean
    // are [0.01, -0.02, 0.02, -0.01], squares sum = 0.0001+0.0004+0.0004+0.0001
    // = 0.001, variance = 0.001/3 = 0.000333..., stdev = 0.0182574...
    // Sharpe (periods_per_year=1, to isolate the raw ratio) = 0.01 / 0.0182574 = 0.547723
    let returns = [0.02, -0.01, 0.03, 0.0];
    let result = sharpe_ratio(&returns, 0.0, 1.0).unwrap();
    assert!((result - 0.547722558).abs() < 1e-6);
}

#[test]
fn sortino_ratio_is_none_when_there_is_no_downside() {
    let returns = [0.01, 0.02, 0.03];
    assert_eq!(sortino_ratio(&returns, 0.0, 252.0), None);
}

#[test]
fn sortino_ratio_hand_computed() {
    // Returns [0.02, -0.01, 0.03, -0.02], risk-free 0. mean = 0.005.
    // Downside (negative) returns: -0.01, -0.02; squares: 0.0001, 0.0004;
    // sum = 0.0005; divided by total count (4) = 0.000125;
    // downside deviation = sqrt(0.000125) = 0.0111803...
    // Sortino (periods_per_year=1) = 0.005 / 0.0111803 = 0.447213...
    let returns = [0.02, -0.01, 0.03, -0.02];
    let result = sortino_ratio(&returns, 0.0, 1.0).unwrap();
    assert!((result - 0.447213595).abs() < 1e-6);
}
