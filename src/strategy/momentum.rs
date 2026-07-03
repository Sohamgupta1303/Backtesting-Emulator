//! Momentum: the simplest possible trend-following strategy.
//!
//! Look at the return over the last `lookback` bars (how much the price
//! moved). If it's up by more than `threshold`, bet that the trend
//! continues and go long. Otherwise, stay flat. Unlike MA crossover this
//! doesn't smooth anything — it's a single, direct measurement of recent
//! price change.

use crate::data::types::Bar;
use crate::events::{Direction, MarketEvent, SignalEvent};

use super::{Strategy, StrategyContext};

/// Long whenever the trailing `lookback`-bar return exceeds `threshold`
/// (e.g. `threshold = 0.05` means "up more than 5% over the lookback
/// window"); flat otherwise.
#[derive(Debug, Clone, Copy)]
pub struct Momentum {
    lookback: usize,
    threshold: f64,
    /// Whether we currently want to be long, so a signal is only emitted
    /// when this actually changes.
    is_long: bool,
}

impl Momentum {
    pub fn new(lookback: usize, threshold: f64) -> Self {
        assert!(lookback >= 1, "Momentum: lookback must be at least 1 bar");
        Self {
            lookback,
            threshold,
            is_long: false,
        }
    }

    /// Trailing return over `lookback` bars, or `None` if there isn't
    /// enough history yet (needs the current bar plus `lookback` bars
    /// before it, i.e. `lookback + 1` bars total).
    fn trailing_return(&self, history: &[Bar]) -> Option<f64> {
        let len = history.len();
        if len <= self.lookback {
            return None;
        }
        let current = history[len - 1].close;
        let past = history[len - 1 - self.lookback].close;
        if past == 0.0 {
            return None;
        }
        Some((current - past) / past)
    }
}

impl Strategy for Momentum {
    fn on_market(&mut self, event: &MarketEvent, ctx: &StrategyContext) -> Vec<SignalEvent> {
        let Some(trailing_return) = self.trailing_return(&ctx.history) else {
            return Vec::new();
        };
        let should_be_long = trailing_return > self.threshold;
        if should_be_long == self.is_long {
            return Vec::new();
        }
        self.is_long = should_be_long;

        let direction = if should_be_long {
            Direction::Long
        } else {
            Direction::Exit
        };
        vec![SignalEvent {
            symbol: event.symbol.clone(),
            timestamp: event.bar.timestamp,
            direction,
            strength: 1.0,
        }]
    }

    fn warmup_bars(&self) -> usize {
        self.lookback + 1
    }
}
