//! `SimulatedExecution`: market and limit order fills, with configurable
//! slippage, commission, latency, limit-fill strictness, partial fills,
//! and time-in-force.
//!
//! This is where the engine's central anti-lookahead guarantee is
//! mechanically enforced. An order submitted while processing bar T is
//! staged, not filled immediately — it only becomes eligible to fill on
//! the *next* call to [`ExecutionModel::on_bar`], which the engine always
//! makes with bar T+1. There is no code path here that can fill an order
//! against the bar that produced it. `latency_bars` adds further delay
//! *on top of* that built-in one bar.

use std::collections::VecDeque;

use crate::events::{FillEvent, MarketEvent, OrderEvent, OrderType};

use super::models::{CommissionModel, LimitFillPolicy, SlippageModel};
use super::ExecutionModel;

/// An order waiting to fill: how many more bars must pass (beyond the
/// built-in one) before it's even eligible to try, and how many bars
/// it's already been eligible-but-unfilled for (for time-in-force).
#[derive(Debug, Clone)]
struct RestingOrder {
    order: OrderEvent,
    bars_remaining: u32,
    bars_waited: u32,
}

/// Fills orders against bar data. Market orders fill at the open of the
/// bar after they were submitted (plus any configured `latency_bars`).
/// Limit orders become eligible the same way, then wait — potentially
/// across many further bars — until the bar's price range satisfies the
/// limit under the configured [`LimitFillPolicy`].
///
/// Under [`SlippageModel::VolumeImpact`], an order larger than
/// `participation_limit * bar_volume` only fills that much this bar; the
/// remaining quantity keeps resting as its own order (subject to the same
/// time-in-force clock). Partial fills only happen under `VolumeImpact` —
/// `None` and `FixedBps` always fill an order's full size in one shot,
/// since neither models any notion of available volume.
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
    /// If set, an order that has been eligible for this many bars without
    /// (fully) filling is cancelled rather than left resting forever.
    /// `None` (the default) means rest indefinitely.
    time_in_force_bars: Option<u32>,
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

    /// Configures time-in-force: an order still resting unfilled after
    /// this many eligible bars is cancelled. Defaults to no expiry.
    pub fn with_time_in_force(mut self, bars: u32) -> Self {
        self.time_in_force_bars = Some(bars);
        self
    }

    /// Splits `quantity` into (fillable now, remaining to keep resting)
    /// under the configured slippage model. Only `VolumeImpact` caps
    /// participation; every other model fills the full amount.
    fn split_for_volume_cap(&self, quantity: f64, bar_volume: f64) -> (f64, f64) {
        match self.slippage {
            SlippageModel::VolumeImpact {
                participation_limit,
                ..
            } => {
                let cap = (participation_limit * bar_volume).max(0.0);
                if quantity <= cap {
                    (quantity, 0.0)
                } else {
                    (cap, quantity - cap)
                }
            }
            SlippageModel::None | SlippageModel::FixedBps(_) => (quantity, 0.0),
        }
    }
}

impl ExecutionModel for SimulatedExecution {
    fn on_bar(&mut self, bar: &MarketEvent) -> Vec<FillEvent> {
        for order in self.staged.drain(..) {
            self.resting.push_back(RestingOrder {
                order,
                bars_remaining: self.latency_bars,
                bars_waited: 0,
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

            if let Some(tif) = self.time_in_force_bars {
                if resting.bars_waited >= tif {
                    eprintln!(
                        "warning: order {} expired (time-in-force) without filling",
                        resting.order.id
                    );
                    continue; // cancelled: dropped, not re-queued
                }
            }

            match resting.order.order_type {
                OrderType::Market => {
                    let (fill_qty, remainder_qty) =
                        self.split_for_volume_cap(resting.order.quantity, bar.bar.volume);

                    if fill_qty > 0.0 {
                        let fill_price = self.slippage.adjusted_price(
                            bar.bar.open,
                            resting.order.side,
                            fill_qty,
                            bar.bar.volume,
                        );
                        let commission = self.commission.commission(fill_qty, fill_price);
                        fills.push(FillEvent {
                            order_id: resting.order.id,
                            symbol: resting.order.symbol.clone(),
                            timestamp: bar.bar.timestamp,
                            side: resting.order.side,
                            quantity_filled: fill_qty,
                            fill_price,
                            commission,
                        });
                    }

                    if remainder_qty > 0.0 {
                        // The remainder inherits (rather than resets) the
                        // original order's time-in-force clock: a partial
                        // fill is progress, not a fresh start, so it
                        // doesn't buy the remainder extra patience.
                        still_resting.push_back(RestingOrder {
                            order: OrderEvent {
                                quantity: remainder_qty,
                                ..resting.order
                            },
                            bars_remaining: 0,
                            bars_waited: resting.bars_waited + 1,
                        });
                    }
                }
                OrderType::Limit { price } => {
                    match self
                        .limit_fill_policy
                        .fill_price(resting.order.side, price, &bar.bar)
                    {
                        // No slippage on limit fills, and no partial fills
                        // either: guaranteeing a price and size is the
                        // entire point of a limit order.
                        Some(fill_price) => {
                            let commission = self
                                .commission
                                .commission(resting.order.quantity, fill_price);
                            fills.push(FillEvent {
                                order_id: resting.order.id,
                                symbol: resting.order.symbol,
                                timestamp: bar.bar.timestamp,
                                side: resting.order.side,
                                quantity_filled: resting.order.quantity,
                                fill_price,
                                commission,
                            });
                        }
                        None => {
                            still_resting.push_back(RestingOrder {
                                bars_waited: resting.bars_waited + 1,
                                ..resting
                            });
                        }
                    }
                }
            }
        }

        self.resting = still_resting;
        fills
    }

    fn submit(&mut self, order: OrderEvent) {
        self.staged.push_back(order);
    }
}
