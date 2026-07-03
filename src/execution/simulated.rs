//! `SimulatedExecution`: market-order-only fills at the next bar's open,
//! with configurable slippage and commission.
//!
//! This is where the engine's central anti-lookahead guarantee is
//! mechanically enforced. An order submitted while processing bar T is
//! staged, not filled immediately — it only becomes eligible to fill on
//! the *next* call to [`ExecutionModel::on_bar`], which the engine always
//! makes with bar T+1. There is no code path here that can fill an order
//! against the bar that produced it.
//!
//! Limit orders, latency beyond the T+1 rule, and partial fills are added
//! later in the execution-realism milestone.

use std::collections::VecDeque;

use crate::events::{FillEvent, MarketEvent, OrderEvent, OrderType};

use super::models::{CommissionModel, SlippageModel};
use super::ExecutionModel;

/// Fills market orders at the open of the bar *after* the one they were
/// submitted during, applying the configured slippage and commission
/// models. Limit orders are not yet implemented; only `OrderType::Market`
/// is handled here.
#[derive(Debug, Default)]
pub struct SimulatedExecution {
    /// Orders eligible to fill on this or a future `on_bar` call.
    resting: VecDeque<OrderEvent>,
    /// Orders submitted since the last `on_bar` call; promoted to
    /// `resting` at the *start* of the next `on_bar`, immediately before
    /// that call's fill scan -- so an order submitted during bar T's
    /// processing fills at bar T+1's open, never bar T's.
    staged: VecDeque<OrderEvent>,
    slippage: SlippageModel,
    commission: CommissionModel,
}

impl SimulatedExecution {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the slippage model. Defaults to [`SlippageModel::None`].
    pub fn with_slippage(mut self, model: SlippageModel) -> Self {
        self.slippage = model;
        self
    }

    /// Configures the commission model. Defaults to a zero flat fee.
    pub fn with_commission(mut self, model: CommissionModel) -> Self {
        self.commission = model;
        self
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
                let fill_price = self.slippage.adjusted_price(
                    bar.bar.open,
                    order.side,
                    order.quantity,
                    bar.bar.volume,
                );
                let commission = self.commission.commission(order.quantity, fill_price);
                FillEvent {
                    order_id: order.id,
                    symbol: order.symbol,
                    timestamp: bar.bar.timestamp,
                    side: order.side,
                    quantity_filled: order.quantity,
                    fill_price,
                    commission,
                }
            })
            .collect()
    }

    fn submit(&mut self, order: OrderEvent) {
        self.staged.push_back(order);
    }
}
