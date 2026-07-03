//! `SimulatedExecution`: market and limit order fills, with configurable
//! slippage, commission, latency, and limit-fill strictness.
//!
//! This is where the engine's central anti-lookahead guarantee is
//! mechanically enforced. An order submitted while processing bar T is
//! staged, not filled immediately — it only becomes eligible to fill on
//! the *next* call to [`ExecutionModel::on_bar`], which the engine always
//! makes with bar T+1. There is no code path here that can fill an order
//! against the bar that produced it. `latency_bars` adds further delay
//! *on top of* that built-in one bar.
//!
//! Partial fills and time-in-force (expiring a resting order after N bars)
//! are added later in the execution-realism milestone.

use std::collections::VecDeque;

use crate::events::{FillEvent, MarketEvent, OrderEvent, OrderType};

use super::models::{CommissionModel, LimitFillPolicy, SlippageModel};
use super::ExecutionModel;

/// An order waiting to fill, plus how many more bars must pass (beyond
/// the built-in one) before it's even eligible to try.
#[derive(Debug, Clone)]
struct RestingOrder {
    order: OrderEvent,
    bars_remaining: u32,
}

/// Fills orders against bar data. Market orders fill at the open of the
/// bar after they were submitted (plus any configured `latency_bars`).
/// Limit orders become eligible the same way, then wait — potentially
/// across many further bars — until the bar's price range satisfies the
/// limit under the configured [`LimitFillPolicy`].
#[derive(Debug, Default)]
pub struct SimulatedExecution {
    /// Orders that have cleared latency and are actively checked for a
    /// fill every bar (immediately for market orders; each bar's price
    /// range for limit orders, until triggered).
    resting: VecDeque<RestingOrder>,
    /// Orders submitted since the last `on_bar` call; promoted to
    /// `resting` at the *start* of the next `on_bar`, immediately before
    /// that call's fill scan -- so an order submitted during bar T's
    /// processing is never eligible before bar T+1.
    staged: VecDeque<OrderEvent>,
    slippage: SlippageModel,
    commission: CommissionModel,
    limit_fill_policy: LimitFillPolicy,
    /// Extra bars of delay before a newly-resting order is even checked
    /// for a fill, on top of the built-in one-bar minimum. Zero by
    /// default.
    latency_bars: u32,
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

    /// Configures how strictly limit orders are checked against a bar's
    /// price range. Defaults to [`LimitFillPolicy::Optimistic`].
    pub fn with_limit_fill_policy(mut self, policy: LimitFillPolicy) -> Self {
        self.limit_fill_policy = policy;
        self
    }

    /// Configures extra bars of delay before an order becomes eligible to
    /// fill, on top of the built-in one-bar minimum. Defaults to `0`.
    pub fn with_latency_bars(mut self, bars: u32) -> Self {
        self.latency_bars = bars;
        self
    }
}

impl ExecutionModel for SimulatedExecution {
    fn on_bar(&mut self, bar: &MarketEvent) -> Vec<FillEvent> {
        for order in self.staged.drain(..) {
            self.resting.push_back(RestingOrder {
                order,
                bars_remaining: self.latency_bars,
            });
        }

        let mut fills = Vec::new();
        let mut still_resting = VecDeque::new();

        while let Some(mut resting) = self.resting.pop_front() {
            if resting.bars_remaining > 0 {
                resting.bars_remaining -= 1;
                still_resting.push_back(resting);
                continue;
            }

            let base_price = match resting.order.order_type {
                OrderType::Market => Some(bar.bar.open),
                OrderType::Limit { price } => {
                    self.limit_fill_policy
                        .fill_price(resting.order.side, price, &bar.bar)
                }
            };

            let Some(base_price) = base_price else {
                // Latency has cleared but the limit condition hasn't
                // triggered yet -- keep waiting, indefinitely for now
                // (time-in-force expiry lands later in this milestone).
                still_resting.push_back(resting);
                continue;
            };

            let order = resting.order;
            let fill_price = match order.order_type {
                OrderType::Market => self.slippage.adjusted_price(
                    base_price,
                    order.side,
                    order.quantity,
                    bar.bar.volume,
                ),
                // No slippage on limit fills: guaranteeing a price is the
                // entire point of a limit order.
                OrderType::Limit { .. } => base_price,
            };
            let commission = self.commission.commission(order.quantity, fill_price);

            fills.push(FillEvent {
                order_id: order.id,
                symbol: order.symbol,
                timestamp: bar.bar.timestamp,
                side: order.side,
                quantity_filled: order.quantity,
                fill_price,
                commission,
            });
        }

        self.resting = still_resting;
        fills
    }

    fn submit(&mut self, order: OrderEvent) {
        self.staged.push_back(order);
    }
}
