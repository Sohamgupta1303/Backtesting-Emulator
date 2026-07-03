//! Configurable slippage and commission models used by
//! [`SimulatedExecution`](super::simulated::SimulatedExecution).
//!
//! Every variant's doc comment says what real-world effect it's
//! approximating and where that approximation breaks down — the point of
//! this whole layer is to be honest about what it does and doesn't model.

use crate::data::types::Bar;
use crate::events::Side;

/// How much a fill's price moves *against* the trader, relative to a
/// reference price (a market order's reference is the fill bar's open).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SlippageModel {
    /// No slippage: fills happen exactly at the reference price. A useful
    /// baseline for isolating strategy performance from execution cost,
    /// but unrealistic for anything except the most liquid instruments
    /// traded in small size.
    #[default]
    None,
    /// A fixed adverse move, in basis points (1 bps = 0.01%), applied
    /// regardless of order size. Approximates the typical cost of
    /// crossing the bid-ask spread for a liquid instrument. Breaks down
    /// for orders that are large relative to available volume, where
    /// real price impact grows with size — see `VolumeImpact`.
    FixedBps(f64),
    /// Price impact that grows with the order's size relative to the
    /// bar's volume: the larger the order relative to what actually
    /// traded, the worse the average fill price. `impact_coefficient`
    /// scales how much the price moves per unit of participation (order
    /// quantity / bar volume). `participation_limit` is the fraction of a
    /// bar's volume a single order may consume before the remainder must
    /// rest as a partial fill — wired up when partial fills are
    /// implemented. This approximates an order eating through a limit
    /// order book, but doesn't model any specific book shape.
    VolumeImpact {
        participation_limit: f64,
        impact_coefficient: f64,
    },
}

impl SlippageModel {
    /// The fill price after applying this model's adverse move to
    /// `reference_price`, for a trade of `quantity` on `side`, within a
    /// bar that traded `bar_volume` total. Always worse for the trader:
    /// higher for a buy, lower for a sell.
    pub fn adjusted_price(
        &self,
        reference_price: f64,
        side: Side,
        quantity: f64,
        bar_volume: f64,
    ) -> f64 {
        let adverse_fraction = match self {
            SlippageModel::None => 0.0,
            SlippageModel::FixedBps(bps) => bps / 10_000.0,
            SlippageModel::VolumeImpact {
                impact_coefficient, ..
            } => {
                let participation = if bar_volume > 0.0 {
                    quantity / bar_volume
                } else {
                    0.0
                };
                impact_coefficient * participation
            }
        };
        let adverse_amount = reference_price * adverse_fraction;
        match side {
            Side::Buy => reference_price + adverse_amount,
            Side::Sell => reference_price - adverse_amount,
        }
    }
}

/// How a fill's commission (trading fee) is computed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommissionModel {
    /// A flat fee per share/contract traded (e.g. `$0.005`/share).
    PerShare(f64),
    /// A flat fee per trade, regardless of size — typical of many
    /// retail/crypto brokers.
    PerTradeFlat(f64),
    /// A fee proportional to trade notional, in basis points (e.g. `2.0`
    /// = 0.02% of the dollar value traded) — typical of institutional
    /// commission schedules.
    BpsOfNotional(f64),
}

impl Default for CommissionModel {
    /// A $0.00 flat fee -- a no-cost baseline, not a realistic assumption.
    fn default() -> Self {
        CommissionModel::PerTradeFlat(0.0)
    }
}

impl CommissionModel {
    pub fn commission(&self, quantity: f64, fill_price: f64) -> f64 {
        match self {
            CommissionModel::PerShare(rate) => quantity * rate,
            CommissionModel::PerTradeFlat(fee) => *fee,
            CommissionModel::BpsOfNotional(bps) => quantity * fill_price * (bps / 10_000.0),
        }
    }
}

/// How strictly a limit order's trigger condition is checked against a
/// bar's price range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LimitFillPolicy {
    /// A buy limit triggers if the bar's *low* reached the limit price or
    /// below; a sell limit triggers if the bar's *high* reached the limit
    /// price or above. This is "optimistic" because it assumes that if
    /// the price merely touched the limit at any point during the bar,
    /// the order could have been filled there — real order books don't
    /// guarantee that (the touch might have been instantaneous, with no
    /// size available at that exact price).
    #[default]
    Optimistic,
    /// Requires the bar's *close* to satisfy the condition instead of the
    /// intrabar low/high. Stricter and less prone to phantom fills, at
    /// the cost of triggering fewer, later fills than would likely happen
    /// in reality.
    Conservative,
}

impl LimitFillPolicy {
    /// The fill price if a limit order for `side` at `limit_price` would
    /// trigger against `bar` under this policy, or `None` if it wouldn't.
    /// Always exactly `limit_price` when it triggers — no slippage is
    /// applied to limit fills, since guaranteeing a price is the entire
    /// point of a limit order.
    pub fn fill_price(&self, side: Side, limit_price: f64, bar: &Bar) -> Option<f64> {
        let triggered = match (side, self) {
            (Side::Buy, LimitFillPolicy::Optimistic) => bar.low <= limit_price,
            (Side::Buy, LimitFillPolicy::Conservative) => bar.close <= limit_price,
            (Side::Sell, LimitFillPolicy::Optimistic) => bar.high >= limit_price,
            (Side::Sell, LimitFillPolicy::Conservative) => bar.close >= limit_price,
        };
        triggered.then_some(limit_price)
    }
}
