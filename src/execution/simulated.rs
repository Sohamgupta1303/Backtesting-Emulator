//! `SimulatedExecution`: market-order-only fills at the next bar's open.
//!
//! This is where the engine's central anti-lookahead guarantee is
//! mechanically enforced. An order submitted while processing bar T is
//! staged, not filled immediately — it only becomes eligible to fill on
//! the *next* call to [`ExecutionModel::on_bar`], which the engine always
//! makes with bar T+1. There is no code path here that can fill an order
//! against the bar that produced it.
//!
//! Slippage, commissions, limit orders, latency beyond the T+1 rule, and
//! partial fills are added in the execution-realism milestone.

use std::collections::VecDeque;

use crate::events::{FillEvent, MarketEvent, OrderEvent, OrderType};

use super::ExecutionModel;

/// Fills market orders at the open of the bar *after* the one they were
/// submitted during. Limit orders and slippage/commission models land in
/// a later milestone; only `OrderType::Market` is handled here.
#[derive(Debug, Default)]
pub struct SimulatedExecution {
    /// Orders eligible to fill on this or a future `on_bar` call.
    resting: VecDeque<OrderEvent>,
    /// Orders submitted since the last `on_bar` call; promoted to
    /// `resting` at the *start* of the next `on_bar`, immediately before
    /// that call's fill scan -- so an order submitted during bar T's
    /// processing fills at bar T+1's open, never bar T's.
    staged: VecDeque<OrderEvent>,
}

impl SimulatedExecution {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ExecutionModel for SimulatedExecution {
    fn on_bar(&mut self, bar: &MarketEvent) -> Vec<FillEvent> {
        // Orders submitted during the *previous* bar's processing (after
        // that bar's `on_bar` call already ran) are promoted to resting
        // now, before this bar's fill scan -- that's what makes them
        // eligible to fill against *this* bar's open, exactly one bar
        // after they were submitted.
        self.resting.extend(self.staged.drain(..));

        self.resting
            .drain(..)
            .map(|order| {
                let OrderType::Market = order.order_type else {
                    unreachable!("limit orders are not implemented until a later milestone")
                };
                FillEvent {
                    order_id: order.id,
                    symbol: order.symbol,
                    timestamp: bar.bar.timestamp,
                    side: order.side,
                    quantity_filled: order.quantity,
                    fill_price: bar.bar.open,
                    commission: 0.0,
                }
            })
            .collect()
    }

    fn submit(&mut self, order: OrderEvent) {
        self.staged.push_back(order);
    }
}
