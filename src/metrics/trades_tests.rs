//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use chrono::{TimeZone, Utc};

use super::trades::{total_commission, total_slippage_cost, trade_stats};
use crate::data::types::SymbolId;
use crate::events::{FillEvent, Side};
use crate::portfolio::ClosedTrade;

fn symbol() -> SymbolId {
    SymbolId::new("TEST")
}

fn ts(day: u32) -> chrono::DateTime<chrono::Utc> {
    Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap()
}

fn trade(entry_day: u32, exit_day: u32, quantity: f64, realized_pnl: f64) -> ClosedTrade {
    ClosedTrade {
        symbol: symbol(),
        entry_timestamp: ts(entry_day),
        exit_timestamp: ts(exit_day),
        quantity,
        realized_pnl,
    }
}

#[test]
fn trade_stats_on_empty_trades_is_all_zero() {
    let stats = trade_stats(&[]);
    assert_eq!(stats.number_of_trades, 0);
    assert_eq!(stats.win_rate, 0.0);
    assert_eq!(stats.profit_factor, None);
}

#[test]
fn trade_stats_hand_computed() {
    // Trades: +100 (2 days), -50 (4 days), +30 (3 days).
    // win_rate = 2/3; average_win = (100+30)/2 = 65; average_loss = -50;
    // profit_factor = 130/50 = 2.6; average holding = (2+4+3)/3 = 3.0 days.
    let trades = [
        trade(1, 3, 10.0, 100.0),
        trade(1, 5, 10.0, -50.0),
        trade(1, 4, 10.0, 30.0),
    ];
    let stats = trade_stats(&trades);
    assert_eq!(stats.number_of_trades, 3);
    assert!((stats.win_rate - (2.0 / 3.0)).abs() < 1e-9);
    assert_eq!(stats.average_win, 65.0);
    assert_eq!(stats.average_loss, -50.0);
    assert_eq!(stats.profit_factor, Some(2.6));
    assert_eq!(stats.average_holding_period_days, 3.0);
}

#[test]
fn profit_factor_is_none_with_no_losing_trades() {
    let trades = [trade(1, 2, 10.0, 100.0)];
    assert_eq!(trade_stats(&trades).profit_factor, None);
}

fn fill(
    side: Side,
    quantity: f64,
    fill_price: f64,
    reference_price: f64,
    commission: f64,
) -> FillEvent {
    FillEvent {
        order_id: 1,
        symbol: symbol(),
        timestamp: ts(1),
        side,
        quantity_filled: quantity,
        fill_price,
        commission,
        reference_price,
    }
}

#[test]
fn total_commission_sums_every_fill() {
    let fills = [
        fill(Side::Buy, 10.0, 100.0, 100.0, 1.5),
        fill(Side::Sell, 10.0, 110.0, 110.0, 2.0),
    ];
    assert_eq!(total_commission(&fills), 3.5);
}

#[test]
fn total_slippage_cost_hand_computed() {
    // Buy: paid 101 vs reference 100 -> cost of 1/share * 10 = 10.
    // Sell: received 108 vs reference 110 -> cost of 2/share * 10 = 20.
    let fills = [
        fill(Side::Buy, 10.0, 101.0, 100.0, 0.0),
        fill(Side::Sell, 10.0, 108.0, 110.0, 0.0),
    ];
    assert_eq!(total_slippage_cost(&fills), 30.0);
}
