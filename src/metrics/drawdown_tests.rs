//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use chrono::{TimeZone, Utc};

use super::drawdown::{calmar_ratio, max_drawdown};
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
fn max_drawdown_is_none_for_an_empty_curve() {
    assert!(max_drawdown(&[]).is_none());
}

#[test]
fn max_drawdown_hand_computed_with_recovery() {
    // Equity: 100, 120, 90, 80, 130, 125.
    // Peak climbs to 120 (day 2), drawdown to 80 (day 4): (120-80)/120 = 1/3.
    // A later peak of 130 (day 5) is a *new* peak, not part of this
    // drawdown's depth -- the worst decline stays anchored to day 2's 120.
    // Recovery: first point after the trough (day 4) with equity >= 120
    // is day 5 (130).
    let curve = [
        point(1, 100.0),
        point(2, 120.0),
        point(3, 90.0),
        point(4, 80.0),
        point(5, 130.0),
        point(6, 125.0),
    ];
    let report = max_drawdown(&curve).unwrap();
    assert!((report.max_drawdown - (1.0 / 3.0)).abs() < 1e-9);
    assert_eq!(report.peak_timestamp, curve[1].timestamp);
    assert_eq!(report.trough_timestamp, curve[3].timestamp);
    assert_eq!(report.recovery_timestamp, Some(curve[4].timestamp));
}

#[test]
fn max_drawdown_recovery_is_none_if_never_recovered() {
    let curve = [point(1, 100.0), point(2, 50.0)];
    let report = max_drawdown(&curve).unwrap();
    assert_eq!(report.max_drawdown, 0.5);
    assert_eq!(report.recovery_timestamp, None);
}

#[test]
fn calmar_ratio_hand_computed() {
    // CAGR 0.20, max drawdown 0.25 -> 0.20 / 0.25 = 0.8.
    assert_eq!(calmar_ratio(0.20, 0.25), Some(0.8));
}

#[test]
fn calmar_ratio_is_none_with_zero_drawdown() {
    assert_eq!(calmar_ratio(0.20, 0.0), None);
}
