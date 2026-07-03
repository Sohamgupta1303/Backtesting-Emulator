use super::test_support::run_signals;
use super::MaCrossover;
use crate::events::Direction;

#[test]
fn no_signal_before_slow_period_is_satisfied() {
    let mut strategy = MaCrossover::new(2, 4);
    // Only 3 bars: shorter than the slow period (4), so nothing should fire.
    let directions = run_signals(&mut strategy, &[10.0, 10.0, 10.0]);
    assert!(directions.is_empty());
}

#[test]
fn goes_long_then_exits_across_a_full_trend_reversal() {
    let mut strategy = MaCrossover::new(3, 6);

    // A clean rise from 1 to 20, then a clean fall back to 1. Strictly
    // monotonic in each direction (no repeated values), so the fast
    // average is provably above the slow average throughout the entire
    // rise and provably below it throughout the entire fall -- exactly
    // one crossing in each direction.
    let rising: Vec<f64> = (1..=20).map(|n| n as f64).collect();
    let falling: Vec<f64> = (1..20).rev().map(|n| n as f64).collect();
    let closes: Vec<f64> = rising.into_iter().chain(falling).collect();

    let directions = run_signals(&mut strategy, &closes);
    assert_eq!(directions, vec![Direction::Long, Direction::Exit]);
}

#[test]
fn first_long_signal_fires_exactly_when_slow_period_is_first_satisfied() {
    let mut strategy = MaCrossover::new(3, 6);
    // Hand-computed: bars 1..=6 = [1,2,3,4,5,6]. slow avg (all 6) = 3.5;
    // fast avg (last 3: 4,5,6) = 5. 5 > 3.5, so the very first bar with 6
    // bars of history should already emit Long.
    let directions = run_signals(&mut strategy, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(directions, vec![Direction::Long]);
}
