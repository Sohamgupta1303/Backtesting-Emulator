//! Portfolio state: cash, positions, equity tracking, and the
//! `SizingModel` trait.

pub mod sizing;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::data::types::{Price, Quantity, SymbolId};
use crate::events::{FillEvent, OrderEvent, Side, SignalEvent};

/// An open position in a single symbol. `quantity` is signed: positive is
/// long, negative is short.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub quantity: Quantity,
    pub avg_entry_price: Price,
    pub realized_pnl: f64,
    /// When the *current* leg of exposure began (reset whenever the
    /// position opens from flat or flips through zero). Used to compute
    /// holding period for trade-level metrics; `None` while flat.
    pub entry_timestamp: Option<DateTime<Utc>>,
}

/// One completed (fully or partially) closing fill: the realized PnL from
/// reducing, closing, or flipping a position, with enough context to
/// compute trade-level stats (win rate, average holding period, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedTrade {
    pub symbol: SymbolId,
    pub entry_timestamp: DateTime<Utc>,
    pub exit_timestamp: DateTime<Utc>,
    pub quantity: Quantity,
    pub realized_pnl: f64,
}

/// One point on the equity curve, recorded once per bar after the event
/// queue has been fully drained for that timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquityPoint {
    pub timestamp: DateTime<Utc>,
    pub equity: f64,
    pub cash: f64,
    pub gross_exposure: f64,
}

/// Cash, open positions, and the equity time series. No margin in v1: a
/// position's mark-to-market value is `quantity * last_seen_price`, and
/// short positions carry no borrow cost — both are known simplifications
/// documented in the README rather than modeled.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Portfolio {
    pub cash: f64,
    pub positions: HashMap<SymbolId, Position>,
    pub last_prices: HashMap<SymbolId, Price>,
    pub equity_curve: Vec<EquityPoint>,
    /// Every fill ever applied, in order — the full trade log, used by
    /// metrics to total commissions/slippage paid.
    pub fills: Vec<FillEvent>,
    /// Every completed (fully or partially) closing fill, used by
    /// metrics for trade-level stats (win rate, profit factor, etc.).
    pub closed_trades: Vec<ClosedTrade>,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            cash: initial_cash,
            positions: HashMap::new(),
            last_prices: HashMap::new(),
            equity_curve: Vec::new(),
            fills: Vec::new(),
            closed_trades: Vec::new(),
        }
    }

    pub fn position(&self, symbol: &SymbolId) -> Position {
        self.positions.get(symbol).copied().unwrap_or_default()
    }

    /// Records the most recently observed price for `symbol`, used for
    /// mark-to-market valuation and by sizing models to translate a
    /// target dollar allocation into a share quantity. Bar *close* is the
    /// natural choice here — a strategy has already legitimately seen it,
    /// so using it for sizing/valuation is not a lookahead violation (only
    /// *fills* are constrained to the next bar's open).
    pub fn update_price(&mut self, symbol: &SymbolId, price: Price) {
        self.last_prices.insert(symbol.clone(), price);
    }

    /// Total mark-to-market portfolio value: cash plus the signed value of
    /// every open position at its last observed price.
    pub fn equity(&self) -> f64 {
        self.cash
            + self
                .positions
                .iter()
                .map(|(symbol, position)| {
                    let price = self.last_prices.get(symbol).copied().unwrap_or(0.0);
                    position.quantity * price
                })
                .sum::<f64>()
    }

    /// Sum of the absolute mark-to-market value of every open position —
    /// long and short both count as exposure, they don't offset.
    pub fn gross_exposure(&self) -> f64 {
        self.positions
            .iter()
            .map(|(symbol, position)| {
                let price = self.last_prices.get(symbol).copied().unwrap_or(0.0);
                (position.quantity * price).abs()
            })
            .sum()
    }

    /// Appends an [`EquityPoint`] using current cash/position state. Called
    /// once per bar, after the queue has been drained for that timestamp.
    pub fn snapshot_equity(&mut self, timestamp: DateTime<Utc>) {
        self.equity_curve.push(EquityPoint {
            timestamp,
            equity: self.equity(),
            cash: self.cash,
            gross_exposure: self.gross_exposure(),
        });
    }

    /// Whether `order` could be paid for in cash at its `reference_price`
    /// (the last observed price — the *actual* fill price is unknown until
    /// execution, so this is a pre-trade approximation, not a guarantee).
    /// Only buys are constrained: shorting has no margin requirement in
    /// v1, and closing/reducing a position never needs more cash than it
    /// releases.
    pub fn can_afford(&self, order: &OrderEvent, reference_price: Price) -> bool {
        match order.side {
            Side::Buy => order.quantity * reference_price <= self.cash + 1e-9,
            Side::Sell => true,
        }
    }

    /// Applies a fill: updates cash, and updates the position using
    /// weighted-average-cost accounting. Handles opening, adding to,
    /// reducing, closing, and flipping (long-to-short or vice versa) a
    /// position, realizing PnL on whatever portion of the fill closes
    /// existing exposure. Also appends to the fill log and, for any
    /// closing portion, to `closed_trades`.
    pub fn apply_fill(&mut self, fill: &FillEvent) {
        self.fills.push(fill.clone());

        let signed_quantity = match fill.side {
            Side::Buy => fill.quantity_filled,
            Side::Sell => -fill.quantity_filled,
        };
        let cash_delta = match fill.side {
            Side::Buy => -(fill.quantity_filled * fill.fill_price),
            Side::Sell => fill.quantity_filled * fill.fill_price,
        };
        self.cash += cash_delta - fill.commission;

        let position = self.positions.entry(fill.symbol.clone()).or_default();
        let old_quantity = position.quantity;
        let new_quantity = old_quantity + signed_quantity;

        let same_direction =
            old_quantity == 0.0 || old_quantity.signum() == signed_quantity.signum();
        if same_direction {
            // Opening or adding to a position: extend the weighted-average
            // entry price over the combined size.
            let old_notional = old_quantity.abs() * position.avg_entry_price;
            let added_notional = signed_quantity.abs() * fill.fill_price;
            position.avg_entry_price = if new_quantity != 0.0 {
                (old_notional + added_notional) / new_quantity.abs()
            } else {
                0.0
            };
            if old_quantity == 0.0 {
                // Opening a fresh position from flat: a new leg begins now.
                position.entry_timestamp = Some(fill.timestamp);
            }
        } else {
            // Reducing, closing, or flipping through zero: realize PnL on
            // whatever portion of the fill closes existing exposure.
            let closing_quantity = signed_quantity.abs().min(old_quantity.abs());
            let pnl_per_share = if old_quantity > 0.0 {
                fill.fill_price - position.avg_entry_price
            } else {
                position.avg_entry_price - fill.fill_price
            };
            let closed_pnl = pnl_per_share * closing_quantity;
            position.realized_pnl += closed_pnl;

            let entry_timestamp = position.entry_timestamp.unwrap_or(fill.timestamp);
            self.closed_trades.push(ClosedTrade {
                symbol: fill.symbol.clone(),
                entry_timestamp,
                exit_timestamp: fill.timestamp,
                quantity: closing_quantity,
                realized_pnl: closed_pnl,
            });

            if new_quantity == 0.0 {
                position.avg_entry_price = 0.0;
                position.entry_timestamp = None;
            } else if new_quantity.signum() != old_quantity.signum() {
                // Flipped past zero: the leftover quantity opens a fresh
                // position at this fill's price and timestamp.
                position.avg_entry_price = fill.fill_price;
                position.entry_timestamp = Some(fill.timestamp);
            }
        }
        position.quantity = new_quantity;
    }
}

/// Converts a `SignalEvent` (intent) into a sized `OrderEvent`, given the
/// current portfolio state. The `id` field of the returned order is a
/// placeholder — the engine assigns the real, unique `OrderId` when it
/// takes the order off the queue, since a sizing model only holds `&self`
/// and shouldn't need interior mutability just to hand out ids.
pub trait SizingModel {
    fn size(&self, signal: &SignalEvent, portfolio: &Portfolio) -> Option<OrderEvent>;
}
