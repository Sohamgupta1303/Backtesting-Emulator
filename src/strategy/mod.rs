//! The `Strategy` trait and the context strategies observe the world
//! through.
//!
//! [`StrategyContext`] is the load-bearing type for lookahead prevention:
//! a strategy is only ever handed a reference to one of these, and it will
//! only ever expose historical data up to and including the current bar.
//! There is no field or method on this type — now or in any future
//! milestone — that reaches into data the engine hasn't emitted yet.

pub mod context;
pub mod ma_crossover;
pub mod mean_reversion;
pub mod momentum;
mod stats;

#[cfg(test)]
mod context_tests;
#[cfg(test)]
mod ma_crossover_tests;
#[cfg(test)]
mod mean_reversion_tests;
#[cfg(test)]
mod momentum_tests;
#[cfg(test)]
mod test_support;

pub use context::{RingBuffer, StrategyContext};
pub use ma_crossover::MaCrossover;
pub use mean_reversion::MeanReversion;
pub use momentum::Momentum;

use crate::events::{MarketEvent, SignalEvent};

/// A trading strategy: observes market events, emits signals expressing
/// directional intent.
pub trait Strategy {
    /// Called once per bar. May return zero or more signals.
    fn on_market(&mut self, event: &MarketEvent, ctx: &StrategyContext) -> Vec<SignalEvent>;

    /// Number of leading bars the strategy needs before it can produce
    /// meaningful signals (e.g. a 50-bar moving average needs 50 bars of
    /// history first). The engine suppresses signals until this many bars
    /// have been observed.
    fn warmup_bars(&self) -> usize;
}

/// A strategy that never trades. Used as the CLI's placeholder until
/// reference strategies land, and as the baseline for the "bars/sec with a
/// no-op strategy" benchmark in the performance milestone — it isolates
/// the cost of the event loop itself from any strategy logic.
#[derive(Debug, Default)]
pub struct NoOpStrategy;

impl Strategy for NoOpStrategy {
    fn on_market(&mut self, _event: &MarketEvent, _ctx: &StrategyContext) -> Vec<SignalEvent> {
        Vec::new()
    }

    fn warmup_bars(&self) -> usize {
        0
    }
}
