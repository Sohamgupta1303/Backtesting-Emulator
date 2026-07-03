//! Moving-average crossover: the oldest, simplest trend-following idea.
//!
//! Compute two averages of recent closing prices — a "fast" one that
//! reacts quickly to new prices, and a "slow" one that moves steadily.
//! When the fast average is above the slow one, momentum is read as
//! picking up, so we want to be long. When it drops back below, momentum
//! is read as fading, so we exit. We only emit a signal *at the moment of
//! the cross* (tracked via `is_long`), not on every bar — this keeps the
//! event log to one signal per actual regime change, matching what
//! "crossover" means.

use crate::data::types::Bar;
use crate::events::{Direction, MarketEvent, SignalEvent};

use super::stats::simple_moving_average;
use super::{Strategy, StrategyContext};

/// Long when the `fast`-bar average closing price is above the
/// `slow`-bar average; flat otherwise.
#[derive(Debug, Clone, Copy)]
pub struct MaCrossover {
    fast: usize,
    slow: usize,
    /// Whether we currently want to be long, so a signal is only emitted
    /// when this actually changes (the crossover "event"), not every bar.
    is_long: bool,
}

impl MaCrossover {
    /// `fast` must be shorter than `slow` — otherwise there's no
    /// meaningful "fast reacts quicker than slow" relationship to trade
    /// on.
    pub fn new(fast: usize, slow: usize) -> Self {
        assert!(
            fast < slow,
            "MaCrossover: fast period ({fast}) must be shorter than slow period ({slow})"
        );
        Self {
            fast,
            slow,
            is_long: false,
        }
    }

    fn desired_direction(&self, history: &[Bar]) -> Option<Direction> {
        if history.len() < self.slow {
            return None; // not enough history yet; engine also enforces this via warmup_bars
        }
        let fast_avg = simple_moving_average(history, self.fast);
        let slow_avg = simple_moving_average(history, self.slow);
        let should_be_long = fast_avg > slow_avg;

        if should_be_long == self.is_long {
            return None; // no regime change, nothing to signal
        }
        Some(if should_be_long {
            Direction::Long
        } else {
            Direction::Exit
        })
    }
}

impl Strategy for MaCrossover {
    fn on_market(&mut self, event: &MarketEvent, ctx: &StrategyContext) -> Vec<SignalEvent> {
        let Some(direction) = self.desired_direction(&ctx.history) else {
            return Vec::new();
        };
        self.is_long = matches!(direction, Direction::Long);

        vec![SignalEvent {
            symbol: event.symbol.clone(),
            timestamp: event.bar.timestamp,
            direction,
            strength: 1.0,
        }]
    }

    fn warmup_bars(&self) -> usize {
        self.slow
    }
}
