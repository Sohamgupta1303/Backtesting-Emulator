//! The `Strategy` trait and the context strategies observe the world
//! through.
//!
//! [`StrategyContext`] is the load-bearing type for lookahead prevention:
//! a strategy is only ever handed a reference to one of these, and it will
//! only ever expose historical data up to and including the current bar.
//! There is no field or method on this type — now or in any future
//! milestone — that reaches into data the engine hasn't emitted yet. The
//! ring buffer of recent bars lands in milestone 3; this module currently
//! defines the trait shape so the engine's dependency graph is in place
//! from day one.

use crate::events::{MarketEvent, SignalEvent};

/// The view of the world a [`Strategy`] is allowed to see: historical data
/// up to and including the current bar, current position, current cash.
/// Deliberately holds no reference to the full dataset or anything beyond
/// "now" — populated fully in milestone 3.
#[derive(Debug, Default)]
pub struct StrategyContext {}

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
