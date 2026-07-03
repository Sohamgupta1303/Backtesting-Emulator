use super::test_support::run_signals;
use super::Momentum;
use crate::events::Direction;

#[test]
fn no_signal_before_lookback_plus_one_bars() {
    let mut strategy = Momentum::new(3, 0.05);
    // Needs lookback + 1 = 4 bars; only 3 given.
    let directions = run_signals(&mut strategy, &[100.0, 100.0, 100.0]);
    assert!(directions.is_empty());
}

#[test]
fn hand_computed_long_then_exit_on_trailing_return() {
    let mut strategy = Momentum::new(3, 0.05);

    // Hand-computed trailing 3-bar returns (comparing each bar's close to
    // the close 3 bars earlier):
    //   bar4=100 vs bar1=100 -> 0.0%      (below 5% threshold: no signal)
    //   bar5=110 vs bar2=100 -> +10.0%    (above threshold: Long)
    //   bar6=115 vs bar3=100 -> +15.0%    (still long: no change)
    //   bar7=120 vs bar4=100 -> +20.0%    (still long: no change)
    //   bar8=125 vs bar5=110 -> +13.6%    (still long: no change)
    //   bar9=100 vs bar6=115 -> -13.0%    (below threshold: Exit)
    //   bar10=95 vs bar7=120 -> -20.8%    (still flat: no change)
    //   bar11=90 vs bar8=125 -> -28.0%    (still flat: no change)
    //   bar12=85 vs bar9=100 -> -15.0%    (still flat: no change)
    let closes = [
        100.0, 100.0, 100.0, 100.0, 110.0, 115.0, 120.0, 125.0, 100.0, 95.0, 90.0, 85.0,
    ];
    let directions = run_signals(&mut strategy, &closes);
    assert_eq!(directions, vec![Direction::Long, Direction::Exit]);
}
