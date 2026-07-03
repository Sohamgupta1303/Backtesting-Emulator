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
    /// Orders eligible to fill on the next `on_bar` call.
    resting: VecDeque<OrderEvent>,
    /// Orders submitted since the last `on_bar` call; promoted to
    /// `resting` at the end of the next `on_bar`, so they wait one full
    /// bar before becoming eligible.
    staged: VecDeque<OrderEvent>,
}

impl SimulatedExecution {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ExecutionModel for SimulatedExecution {
    fn on_bar(&mut self, bar: &MarketEvent) -> Vec<FillEvent> {
        let fills = self
            .resting
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
            .collect();

        // Orders submitted during the bar that just finished processing
        // are promoted to resting now, so they'll fill on the *next* call
        // to `on_bar` (i.e. one full bar of latency), never this one.
        self.resting.extend(self.staged.drain(..));

        fills
    }

    fn submit(&mut self, order: OrderEvent) {
        self.staged.push_back(order);
    }
}
