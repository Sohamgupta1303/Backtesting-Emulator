//! Mean reversion: bets that prices unusually far from their recent
//! average will snap back toward it — the opposite philosophy from
//! trend-following (MA crossover, momentum).
//!
//! Computes a "z-score": how many standard deviations the current price
//! is from its `lookback`-bar rolling mean. A large *negative* z-score
//! (price far below average) is read as "oversold" — bet on a bounce
//! upward (long). A large *positive* z-score (price far above average) is
//! read as "overbought" — bet on a pullback (short). Once the price
//! drifts back close to normal, the position is closed.

use crate::data::types::Bar;
use crate::events::{Direction, MarketEvent, SignalEvent};

use super::stats::{mean, population_stddev};
use super::{Strategy, StrategyContext};

/// Which side (if any) the strategy currently holds, so a signal is only
/// emitted when this actually changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionState {
    Flat,
    Long,
    Short,
}

/// Enters long when the z-score drops to or below `-entry_threshold`,
/// enters short when it rises to or above `entry_threshold`, and exits
/// once the z-score's magnitude falls back under `exit_threshold`.
#[derive(Debug, Clone, Copy)]
pub struct MeanReversion {
    lookback: usize,
    entry_threshold: f64,
    exit_threshold: f64,
    state: PositionState,
}

impl MeanReversion {
    /// `entry_threshold` must be strictly greater than `exit_threshold`
    /// (which must be non-negative) — otherwise a single bar could
    /// qualify as both an entry and an exit signal at once.
    pub fn new(lookback: usize, entry_threshold: f64, exit_threshold: f64) -> Self {
        assert!(
            lookback >= 2,
            "MeanReversion: lookback must be at least 2 bars"
        );
        assert!(
            exit_threshold >= 0.0 && entry_threshold > exit_threshold,
            "MeanReversion: entry_threshold ({entry_threshold}) must be greater than \
             exit_threshold ({exit_threshold}), which must itself be non-negative"
        );
        Self {
            lookback,
            entry_threshold,
            exit_threshold,
            state: PositionState::Flat,
        }
    }

    /// The current bar's z-score against the trailing `lookback`-bar
    /// window, or `None` if there isn't enough history yet, or if the
    /// window has zero variation (a z-score against zero spread is
    /// undefined, not infinite).
    fn z_score(&self, history: &[Bar]) -> Option<f64> {
        if history.len() < self.lookback {
            return None;
        }
        let window = &history[history.len() - self.lookback..];
        let closes: Vec<f64> = window.iter().map(|bar| bar.close).collect();
        let avg = mean(&closes);
        let spread = population_stddev(&closes);
        if spread == 0.0 {
            return None;
        }
        let current = window[window.len() - 1].close;
        Some((current - avg) / spread)
    }
}

impl Strategy for MeanReversion {
    fn on_market(&mut self, event: &MarketEvent, ctx: &StrategyContext) -> Vec<SignalEvent> {
        let Some(z) = self.z_score(&ctx.history) else {
            return Vec::new();
        };

        let next_state = match self.state {
            PositionState::Flat => {
                if z <= -self.entry_threshold {
                    PositionState::Long
                } else if z >= self.entry_threshold {
                    PositionState::Short
                } else {
                    PositionState::Flat
                }
            }
            PositionState::Long | PositionState::Short => {
                if z.abs() < self.exit_threshold {
                    PositionState::Flat
                } else {
                    self.state
                }
            }
        };

        if next_state == self.state {
            return Vec::new();
        }
        self.state = next_state;

        let direction = match next_state {
            PositionState::Long => Direction::Long,
            PositionState::Short => Direction::Short,
            PositionState::Flat => Direction::Exit,
        };

        vec![SignalEvent {
            symbol: event.symbol.clone(),
            timestamp: event.bar.timestamp,
            direction,
            strength: 1.0,
        }]
    }

    fn warmup_bars(&self) -> usize {
        self.lookback
    }
}
