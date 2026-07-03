use chrono::{TimeZone, Utc};

use super::models::{CommissionModel, SlippageModel};
use super::simulated::SimulatedExecution;
use super::ExecutionModel;
use crate::data::types::{Bar, SymbolId};
use crate::events::{MarketEvent, OrderEvent, OrderType, Side};

fn bar_event(day: u32, open: f64, volume: f64) -> MarketEvent {
    MarketEvent {
        symbol: SymbolId::new("TEST"),
        bar: Bar {
            timestamp: Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap(),
            open,
            high: open,
            low: open,
            close: open,
            volume,
        },
    }
}

fn market_order(quantity: f64, side: Side) -> OrderEvent {
    OrderEvent {
        id: 1,
        symbol: SymbolId::new("TEST"),
        timestamp: Utc.with_ymd_and_hms(2021, 1, 1, 0, 0, 0).unwrap(),
        side,
        order_type: OrderType::Market,
        quantity,
    }
}

#[test]
fn applies_configured_slippage_and_commission_to_the_fill() {
    let mut execution = SimulatedExecution::new()
        .with_slippage(SlippageModel::FixedBps(50.0)) // 0.5% adverse
        .with_commission(CommissionModel::PerShare(0.01));

    execution.on_bar(&bar_event(1, 100.0, 1_000.0)); // nothing resting yet
    execution.submit(market_order(10.0, Side::Buy));

    let fills = execution.on_bar(&bar_event(2, 200.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    // Fill price: bar 2's open (200) + 0.5% adverse = 201.0.
    assert_eq!(fills[0].fill_price, 201.0);
    // Commission: 10 shares * $0.01/share = $0.10.
    assert_eq!(fills[0].commission, 0.10);
}

#[test]
fn defaults_to_no_slippage_and_no_commission() {
    let mut execution = SimulatedExecution::new();
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(market_order(10.0, Side::Buy));

    let fills = execution.on_bar(&bar_event(2, 200.0, 1_000.0));
    assert_eq!(fills[0].fill_price, 200.0);
    assert_eq!(fills[0].commission, 0.0);
}
