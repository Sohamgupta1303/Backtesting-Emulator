use chrono::{TimeZone, Utc};

use super::models::{CommissionModel, LimitFillPolicy, SlippageModel};
use super::simulated::SimulatedExecution;
use super::ExecutionModel;
use crate::data::types::{Bar, SymbolId};
use crate::events::{MarketEvent, OrderEvent, OrderType, Side};

fn bar_event(day: u32, open: f64, volume: f64) -> MarketEvent {
    ohlc_bar_event(day, open, open, open, open, volume)
}

fn ohlc_bar_event(
    day: u32,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> MarketEvent {
    MarketEvent {
        symbol: SymbolId::new("TEST"),
        bar: Bar {
            timestamp: Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap(),
            open,
            high,
            low,
            close,
            volume,
        },
    }
}

fn market_order(quantity: f64, side: Side) -> OrderEvent {
    order(quantity, side, OrderType::Market)
}

fn limit_order(quantity: f64, side: Side, price: f64) -> OrderEvent {
    order(quantity, side, OrderType::Limit { price })
}

fn order(quantity: f64, side: Side, order_type: OrderType) -> OrderEvent {
    OrderEvent {
        id: 1,
        symbol: SymbolId::new("TEST"),
        timestamp: Utc.with_ymd_and_hms(2021, 1, 1, 0, 0, 0).unwrap(),
        side,
        order_type,
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
    // Reference price is the pre-slippage price (bar 2's open): the gap
    // between it and fill_price is exactly the slippage cost (1.0/share).
    assert_eq!(fills[0].reference_price, 200.0);
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

#[test]
fn optimistic_buy_limit_fills_when_only_the_low_touches_the_price() {
    let mut execution = SimulatedExecution::new(); // Optimistic is the default
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(limit_order(10.0, Side::Buy, 95.0));

    // Bar 2: open=100, close=98 -- never actually at 95 except a brief dip
    // to a low of 94. Optimistic policy still fills, at the limit price.
    let fills = execution.on_bar(&ohlc_bar_event(2, 100.0, 101.0, 94.0, 98.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].fill_price, 95.0);
    // Limit fills never carry slippage: reference price equals fill price.
    assert_eq!(fills[0].reference_price, 95.0);
}

#[test]
fn conservative_buy_limit_requires_the_close_to_satisfy_the_price() {
    let mut execution =
        SimulatedExecution::new().with_limit_fill_policy(LimitFillPolicy::Conservative);
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(limit_order(10.0, Side::Buy, 95.0));

    // Same bar as above: low touches 94, but close is 98 -- conservative
    // mode requires the *close* to be at or below the limit, so no fill.
    let fills = execution.on_bar(&ohlc_bar_event(2, 100.0, 101.0, 94.0, 98.0, 1_000.0));
    assert!(fills.is_empty());

    // The next bar actually closes at or below the limit: now it fills.
    let fills = execution.on_bar(&ohlc_bar_event(3, 98.0, 99.0, 93.0, 94.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].fill_price, 95.0);
}

#[test]
fn sell_limit_fills_when_the_high_reaches_the_price() {
    let mut execution = SimulatedExecution::new();
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(limit_order(10.0, Side::Sell, 110.0));

    let fills = execution.on_bar(&ohlc_bar_event(2, 100.0, 112.0, 99.0, 101.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].fill_price, 110.0);
}

#[test]
fn limit_order_rests_across_multiple_bars_until_triggered() {
    let mut execution = SimulatedExecution::new();
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(limit_order(10.0, Side::Buy, 90.0));

    // Two bars in a row that never reach 90: no fill, order keeps resting.
    assert!(execution
        .on_bar(&ohlc_bar_event(2, 100.0, 101.0, 95.0, 99.0, 1_000.0))
        .is_empty());
    assert!(execution
        .on_bar(&ohlc_bar_event(3, 99.0, 100.0, 96.0, 98.0, 1_000.0))
        .is_empty());

    // Third bar finally dips to the limit: fills now, not before.
    let fills = execution.on_bar(&ohlc_bar_event(4, 98.0, 99.0, 88.0, 92.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].fill_price, 90.0);
}

#[test]
fn volume_impact_partially_fills_and_rests_the_remainder() {
    let mut execution = SimulatedExecution::new().with_slippage(SlippageModel::VolumeImpact {
        participation_limit: 0.1,
        impact_coefficient: 0.0, // isolate the partial-fill mechanic from price impact
    });
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(market_order(200.0, Side::Buy));

    // Bar 2: volume 1,000 -> cap = 10% * 1,000 = 100. The order wants 200,
    // so only 100 fills now; the remaining 100 keeps resting.
    let fills = execution.on_bar(&bar_event(2, 150.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].quantity_filled, 100.0);
    assert_eq!(fills[0].fill_price, 150.0); // no price impact configured

    // Bar 3: much higher volume -> the remaining 100 easily fits under
    // the new cap, and fills in full.
    let fills = execution.on_bar(&bar_event(3, 160.0, 10_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].quantity_filled, 100.0);
    assert_eq!(fills[0].fill_price, 160.0);
}

#[test]
fn time_in_force_cancels_a_limit_order_that_never_fills() {
    let mut execution = SimulatedExecution::new().with_time_in_force(2);
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(limit_order(10.0, Side::Buy, 50.0)); // far below where price ever trades

    // Two eligible bars where the limit is never reached: two attempts,
    // no fill either time.
    assert!(execution
        .on_bar(&ohlc_bar_event(2, 100.0, 102.0, 90.0, 95.0, 1_000.0))
        .is_empty());
    assert!(execution
        .on_bar(&ohlc_bar_event(3, 95.0, 97.0, 90.0, 93.0, 1_000.0))
        .is_empty());

    // Third eligible bar: time-in-force (2) has now elapsed -- the order
    // is cancelled here, even though it never triggered.
    assert!(execution
        .on_bar(&ohlc_bar_event(4, 93.0, 95.0, 88.0, 90.0, 1_000.0))
        .is_empty());

    // Proof it's really gone, not just still waiting: even though this
    // bar's low finally reaches the limit price, nothing fills.
    assert!(execution
        .on_bar(&ohlc_bar_event(5, 90.0, 91.0, 40.0, 45.0, 1_000.0))
        .is_empty());
}

#[test]
fn time_in_force_also_cancels_an_unfilled_partial_fill_remainder() {
    let mut execution = SimulatedExecution::new()
        .with_slippage(SlippageModel::VolumeImpact {
            participation_limit: 0.1,
            impact_coefficient: 0.0,
        })
        .with_time_in_force(1);
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(market_order(200.0, Side::Buy));

    // Bar 2: partially fills 100 of 200 (cap = 10% of 1,000 volume). The
    // remainder of 100 keeps resting, inheriting the clock -- it's
    // already used its one allotted wait.
    let fills = execution.on_bar(&bar_event(2, 150.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].quantity_filled, 100.0);

    // Bar 3: time_in_force(1) has now elapsed for the remainder -- it's
    // cancelled here rather than partially filling again.
    assert!(execution.on_bar(&bar_event(3, 160.0, 1_000.0)).is_empty());
}

#[test]
fn latency_delays_eligibility_beyond_the_built_in_one_bar() {
    let mut execution = SimulatedExecution::new().with_latency_bars(2);
    execution.on_bar(&bar_event(1, 100.0, 1_000.0));
    execution.submit(market_order(10.0, Side::Buy));

    // Without latency this would fill on bar 2 (the built-in T+1 rule).
    // With 2 extra bars of latency, it must not fill until bar 4.
    assert!(execution.on_bar(&bar_event(2, 200.0, 1_000.0)).is_empty());
    assert!(execution.on_bar(&bar_event(3, 300.0, 1_000.0)).is_empty());
    let fills = execution.on_bar(&bar_event(4, 400.0, 1_000.0));
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].fill_price, 400.0);
}
