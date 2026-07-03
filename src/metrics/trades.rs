//! Trade-level stats (win rate, profit factor, holding period) and cost
//! totals (commission, slippage) — computed from the portfolio's fill and
//! closed-trade logs.

use serde::Serialize;

use crate::events::{FillEvent, Side};
use crate::portfolio::ClosedTrade;

/// Aggregate stats over a set of closed trades.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TradeStats {
    pub number_of_trades: usize,
    /// Fraction of trades with positive realized PnL. `0.0` if there were
    /// no trades.
    pub win_rate: f64,
    /// Average realized PnL of winning trades (`0.0` if there were none).
    pub average_win: f64,
    /// Average realized PnL of losing trades — naturally negative
    /// (`0.0` if there were none).
    pub average_loss: f64,
    /// Gross profit / gross loss. `None` if there were no losing trades
    /// (the ratio is undefined, not infinite).
    pub profit_factor: Option<f64>,
    /// Average time between a trade's entry and its exit, in days.
    pub average_holding_period_days: f64,
}

/// Computes [`TradeStats`] over `trades`. A "zero trades" result (all
/// zeros, `profit_factor: None`) if `trades` is empty.
pub fn trade_stats(trades: &[ClosedTrade]) -> TradeStats {
    let number_of_trades = trades.len();
    if number_of_trades == 0 {
        return TradeStats {
            number_of_trades: 0,
            win_rate: 0.0,
            average_win: 0.0,
            average_loss: 0.0,
            profit_factor: None,
            average_holding_period_days: 0.0,
        };
    }

    let wins: Vec<f64> = trades
        .iter()
        .map(|t| t.realized_pnl)
        .filter(|pnl| *pnl > 0.0)
        .collect();
    let losses: Vec<f64> = trades
        .iter()
        .map(|t| t.realized_pnl)
        .filter(|pnl| *pnl < 0.0)
        .collect();

    let win_rate = wins.len() as f64 / number_of_trades as f64;
    let average_win = if wins.is_empty() {
        0.0
    } else {
        wins.iter().sum::<f64>() / wins.len() as f64
    };
    let average_loss = if losses.is_empty() {
        0.0
    } else {
        losses.iter().sum::<f64>() / losses.len() as f64
    };

    let gross_profit: f64 = wins.iter().sum();
    let gross_loss: f64 = losses.iter().sum::<f64>().abs();
    let profit_factor = if gross_loss == 0.0 {
        None
    } else {
        Some(gross_profit / gross_loss)
    };

    let total_holding_days: f64 = trades
        .iter()
        .map(|t| (t.exit_timestamp - t.entry_timestamp).num_seconds() as f64 / 86_400.0)
        .sum();
    let average_holding_period_days = total_holding_days / number_of_trades as f64;

    TradeStats {
        number_of_trades,
        win_rate,
        average_win,
        average_loss,
        profit_factor,
        average_holding_period_days,
    }
}

/// The slippage cost of a single fill: the difference between what was
/// actually paid/received and what would have happened with zero
/// slippage, always framed as a positive number when slippage worked
/// against the trader (which, for `SimulatedExecution`, it always does).
pub fn slippage_cost(fill: &FillEvent) -> f64 {
    match fill.side {
        Side::Buy => (fill.fill_price - fill.reference_price) * fill.quantity_filled,
        Side::Sell => (fill.reference_price - fill.fill_price) * fill.quantity_filled,
    }
}

/// Total commission paid across every fill.
pub fn total_commission(fills: &[FillEvent]) -> f64 {
    fills.iter().map(|fill| fill.commission).sum()
}

/// Total slippage cost paid across every fill.
pub fn total_slippage_cost(fills: &[FillEvent]) -> f64 {
    fills.iter().map(slippage_cost).sum()
}
