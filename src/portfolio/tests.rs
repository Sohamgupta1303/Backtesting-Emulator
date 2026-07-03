//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use chrono::{TimeZone, Utc};

use super::sizing::FixedFraction;
use super::{Portfolio, SizingModel};
use crate::data::types::SymbolId;
use crate::events::{Direction, FillEvent, OrderEvent, OrderType, Side, SignalEvent};

fn symbol() -> SymbolId {
    SymbolId::new("TEST")
}

fn ts(day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap()
}

fn fill(day: u32, side: Side, quantity: f64, price: f64, commission: f64) -> FillEvent {
    FillEvent {
        order_id: 1,
        symbol: symbol(),
        timestamp: ts(day),
        side,
        quantity_filled: quantity,
        fill_price: price,
        commission,
        reference_price: price, // no slippage modeled in these tests
    }
}

#[test]
fn buying_reduces_cash_and_opens_a_long_position() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 1.0));

    assert_eq!(portfolio.cash, 10_000.0 - 1_000.0 - 1.0);
    let position = portfolio.position(&symbol());
    assert_eq!(position.quantity, 10.0);
    assert_eq!(position.avg_entry_price, 100.0);
    assert_eq!(position.realized_pnl, 0.0);
}

#[test]
fn selling_to_close_a_long_realizes_pnl() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0));
    portfolio.apply_fill(&fill(2, Side::Sell, 10.0, 110.0, 0.0));

    let position = portfolio.position(&symbol());
    assert_eq!(position.quantity, 0.0);
    assert_eq!(position.realized_pnl, 100.0); // 10 shares * $10 gain
    assert_eq!(portfolio.cash, 10_000.0 + 100.0);
}

#[test]
fn short_position_realizes_pnl_on_price_decline() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Sell, 10.0, 100.0, 0.0)); // open short
    portfolio.apply_fill(&fill(2, Side::Buy, 10.0, 90.0, 0.0)); // cover

    let position = portfolio.position(&symbol());
    assert_eq!(position.quantity, 0.0);
    assert_eq!(position.realized_pnl, 100.0); // shorted at 100, covered at 90: $10/share gain
    assert_eq!(portfolio.cash, 10_000.0 + 100.0);
}

#[test]
fn flipping_long_to_short_realizes_pnl_and_opens_fresh_position() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0)); // +10 long @ 100
    portfolio.apply_fill(&fill(2, Side::Sell, 15.0, 110.0, 0.0)); // close 10, open 5 short @ 110

    let position = portfolio.position(&symbol());
    assert_eq!(position.quantity, -5.0);
    assert_eq!(position.avg_entry_price, 110.0);
    assert_eq!(position.realized_pnl, 100.0); // 10 shares * $10 gain on the closing leg
                                              // The flip opens a brand new leg: entry_timestamp resets to the flip's fill.
    assert_eq!(position.entry_timestamp, Some(ts(2)));
}

#[test]
fn every_fill_is_recorded_in_the_fill_log() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 1.0));
    portfolio.apply_fill(&fill(2, Side::Sell, 10.0, 110.0, 1.0));

    assert_eq!(portfolio.fills.len(), 2);
    assert_eq!(portfolio.fills[0].fill_price, 100.0);
    assert_eq!(portfolio.fills[1].fill_price, 110.0);
}

#[test]
fn closing_a_position_records_a_closed_trade_with_holding_period() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0)); // opens: entry_timestamp = day 1
    portfolio.apply_fill(&fill(5, Side::Sell, 10.0, 110.0, 0.0)); // closes on day 5

    assert_eq!(portfolio.closed_trades.len(), 1);
    let trade = &portfolio.closed_trades[0];
    assert_eq!(trade.entry_timestamp, ts(1));
    assert_eq!(trade.exit_timestamp, ts(5));
    assert_eq!(trade.quantity, 10.0);
    assert_eq!(trade.realized_pnl, 100.0);
}

#[test]
fn entry_timestamp_is_none_while_flat_and_set_on_reopening() {
    let mut portfolio = Portfolio::new(10_000.0);
    assert_eq!(portfolio.position(&symbol()).entry_timestamp, None);

    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0));
    assert_eq!(portfolio.position(&symbol()).entry_timestamp, Some(ts(1)));

    portfolio.apply_fill(&fill(2, Side::Sell, 10.0, 100.0, 0.0)); // closes fully -> flat
    assert_eq!(portfolio.position(&symbol()).entry_timestamp, None);

    portfolio.apply_fill(&fill(3, Side::Buy, 5.0, 100.0, 0.0)); // reopens
    assert_eq!(portfolio.position(&symbol()).entry_timestamp, Some(ts(3)));
}

#[test]
fn equity_reflects_cash_plus_mark_to_market_position_value() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0));
    portfolio.update_price(&symbol(), 120.0);

    assert_eq!(portfolio.equity(), 9_000.0 + 10.0 * 120.0);
    assert_eq!(portfolio.gross_exposure(), 10.0 * 120.0);
}

#[test]
fn conservation_holds_across_a_sequence_of_fills() {
    // cash + position market value must equal equity at every step -- no
    // money is created or destroyed by the bookkeeping itself, no matter
    // how the position flips between long, short, and flat.
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.update_price(&symbol(), 100.0);

    let fills = [
        fill(1, Side::Buy, 10.0, 100.0, 1.0),
        fill(2, Side::Buy, 5.0, 105.0, 1.0),
        fill(3, Side::Sell, 8.0, 110.0, 1.0),
        fill(4, Side::Sell, 20.0, 95.0, 1.0), // flips long -> short
        fill(5, Side::Buy, 7.0, 90.0, 1.0),   // partially covers the short
    ];

    for f in &fills {
        portfolio.apply_fill(f);
        portfolio.update_price(&f.symbol, f.fill_price);

        let position_value = portfolio.position(&symbol()).quantity * f.fill_price;
        assert!(
            (portfolio.cash + position_value - portfolio.equity()).abs() < 1e-9,
            "cash + position value must equal equity"
        );
    }
}

#[test]
fn cannot_afford_a_buy_that_exceeds_cash() {
    let portfolio = Portfolio::new(1_000.0);
    let order = OrderEvent {
        id: 1,
        symbol: symbol(),
        timestamp: ts(1),
        side: Side::Buy,
        order_type: OrderType::Market,
        quantity: 100.0,
    };
    assert!(!portfolio.can_afford(&order, 100.0)); // 100 shares * $100 > $1,000
    assert!(portfolio.can_afford(&order, 5.0)); // 100 shares * $5 <= $1,000
}

#[test]
fn fixed_fraction_sizes_to_target_equity_fraction() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.update_price(&symbol(), 100.0);
    let sizing = FixedFraction::new(0.10); // 10% of equity

    let signal = SignalEvent {
        symbol: symbol(),
        timestamp: ts(1),
        direction: Direction::Long,
        strength: 1.0,
    };
    let order = sizing.size(&signal, &portfolio).unwrap();
    assert_eq!(order.side, Side::Buy);
    assert_eq!(order.quantity, 10.0); // 10% of $10,000 / $100 per share
}

#[test]
fn fixed_fraction_exit_closes_the_full_position() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.update_price(&symbol(), 100.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0));
    let sizing = FixedFraction::new(0.10);

    let signal = SignalEvent {
        symbol: symbol(),
        timestamp: ts(2),
        direction: Direction::Exit,
        strength: 1.0,
    };
    let order = sizing.size(&signal, &portfolio).unwrap();
    assert_eq!(order.side, Side::Sell);
    assert_eq!(order.quantity, 10.0);
}

#[test]
fn fixed_fraction_returns_none_when_already_at_target() {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.update_price(&symbol(), 100.0);
    portfolio.apply_fill(&fill(1, Side::Buy, 10.0, 100.0, 0.0)); // exactly 10% of equity
    let sizing = FixedFraction::new(0.10);

    let signal = SignalEvent {
        symbol: symbol(),
        timestamp: ts(2),
        direction: Direction::Long,
        strength: 1.0,
    };
    assert!(sizing.size(&signal, &portfolio).is_none());
}
