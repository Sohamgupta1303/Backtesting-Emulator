//! The `ExecutionModel` trait: how submitted orders turn into fills.
//!
//! Slippage, commissions, limit orders, and partial fills land in the
//! execution-realism milestone; [`simulated::SimulatedExecution`] currently
//! only handles market orders with T+1-open fills.

pub mod models;
pub mod simulated;

#[cfg(test)]
mod models_tests;
#[cfg(test)]
mod simulated_tests;

use crate::events::{FillEvent, MarketEvent, OrderEvent};

/// Simulates order execution against market data.
pub trait ExecutionModel {
    /// Called with each new bar, before it's dispatched to the strategy, so
    /// resting orders from *previous* bars (limit orders, latency-delayed
    /// market orders) get a chance to fill first. Must never fill an order
    /// against the bar that generated it — see the anti-lookahead rule in
    /// the architecture doc.
    fn on_bar(&mut self, bar: &MarketEvent) -> Vec<FillEvent>;

    /// Accepts a new order for later fill attempts.
    fn submit(&mut self, order: OrderEvent);
}
