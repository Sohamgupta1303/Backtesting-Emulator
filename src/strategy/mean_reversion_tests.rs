use super::test_support::run_signals;
use super::MeanReversion;
use crate::events::Direction;

#[test]
fn no_signal_before_lookback_is_satisfied() {
    let mut strategy = MeanReversion::new(4, 1.0, 0.6);
    let directions = run_signals(&mut strategy, &[100.0, 100.0, 100.0]);
    assert!(directions.is_empty());
}

#[test]
fn no_signal_when_the_window_has_zero_spread() {
    // All four bars identical: population stddev is 0, so the z-score is
    // undefined (not infinite) and must not produce a signal.
    let mut strategy = MeanReversion::new(4, 1.0, 0.6);
    let directions = run_signals(&mut strategy, &[100.0, 100.0, 100.0, 100.0]);
    assert!(directions.is_empty());
}

#[test]
fn hand_computed_long_entry_then_exit() {
    let mut strategy = MeanReversion::new(4, 1.0, 0.6);

    // Hand-computed (population stddev, dividing by n=4):
    //   bar5 window = [100,100,100,70]: mean=92.5, stddev=sqrt(675/4)=12.99
    //     z = (70 - 92.5) / 12.99 = -1.732  -> <= -1.0 (entry): Long
    //   bar6 window = [100,100,70,100]: same 4 values, same mean/stddev
    //     z = (100 - 92.5) / 12.99 = +0.577 -> |z| < 0.6 (exit): Exit
    let closes = [100.0, 100.0, 100.0, 100.0, 70.0, 100.0];
    let directions = run_signals(&mut strategy, &closes);
    assert_eq!(directions, vec![Direction::Long, Direction::Exit]);
}

#[test]
fn hand_computed_short_entry_then_exit() {
    let mut strategy = MeanReversion::new(4, 1.0, 0.6);

    // Mirror image of the long scenario:
    //   bar5 window = [100,100,100,130]: mean=107.5, stddev=sqrt(675/4)=12.99
    //     z = (130 - 107.5) / 12.99 = +1.732 -> >= 1.0 (entry): Short
    //   bar6 window = [100,100,130,100]: same 4 values, same mean/stddev
    //     z = (100 - 107.5) / 12.99 = -0.577 -> |z| < 0.6 (exit): Exit
    let closes = [100.0, 100.0, 100.0, 100.0, 130.0, 100.0];
    let directions = run_signals(&mut strategy, &closes);
    assert_eq!(directions, vec![Direction::Short, Direction::Exit]);
}
