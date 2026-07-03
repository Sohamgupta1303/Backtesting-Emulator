//! Engine-level correctness tests: these are the credibility of the
//! project, per the spec. `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use std::collections::VecDeque;

use chrono::{DateTime, TimeZone, Utc};

use super::Engine;
use crate::data::types::{Bar, SymbolId};
use crate::data::DataFeed;
use crate::events::{Direction, MarketEvent, OrderEvent, OrderType, Side, SignalEvent};
use crate::execution::simulated::SimulatedExecution;
use crate::portfolio::{Portfolio, SizingModel};
use crate::strategy::{Strategy, StrategyContext};

const INITIAL_CASH: f64 = 100_000.0;

fn symbol() -> SymbolId {
    SymbolId::new("TEST")
}

fn ts(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap()
}

fn bar(day: u32, open: f64, close: f64) -> Bar {
    Bar {
        timestamp: ts(day),
        open,
        high: open.max(close),
        low: open.min(close),
        close,
        volume: 1_000.0,
    }
}

/// An in-memory [`DataFeed`] for tests, so scenarios can be built directly
/// from `Bar`s without round-tripping through CSV text.
struct VecFeed(VecDeque<MarketEvent>);

impl VecFeed {
    fn new(bars: Vec<Bar>) -> Self {
        Self(
            bars.into_iter()
                .map(|bar| MarketEvent {
                    symbol: symbol(),
                    bar,
                })
                .collect(),
        )
    }
}

impl DataFeed for VecFeed {
    fn next(&mut self) -> Option<MarketEvent> {
        self.0.pop_front()
    }
}

/// A strategy that emits a scripted signal on specific (0-indexed) bar
/// numbers, so test scenarios are fully deterministic and hand-computable.
struct ScriptedStrategy {
    bar_index: usize,
    script: Vec<(usize, Direction)>,
}

impl ScriptedStrategy {
    fn new(script: Vec<(usize, Direction)>) -> Self {
        Self {
            bar_index: 0,
            script,
        }
    }
}

impl Strategy for ScriptedStrategy {
    fn on_market(&mut self, event: &MarketEvent, _ctx: &StrategyContext) -> Vec<SignalEvent> {
        let idx = self.bar_index;
        self.bar_index += 1;
        self.script
            .iter()
            .filter(|(i, _)| *i == idx)
            .map(|(_, direction)| SignalEvent {
                symbol: event.symbol.clone(),
                timestamp: event.bar.timestamp,
                direction: *direction,
                strength: 1.0,
            })
            .collect()
    }

    fn warmup_bars(&self) -> usize {
        0
    }
}

/// Sizes every `Long` signal at a fixed share count, and every `Exit`
/// signal as "close whatever is currently open" -- deliberately bypassing
/// `FixedFraction`'s equity-fraction math so these tests are hand-checkable
/// arithmetic, independent of the sizing model under test elsewhere.
struct FixedQuantity(f64);

impl SizingModel for FixedQuantity {
    fn size(&self, signal: &SignalEvent, portfolio: &Portfolio) -> Option<OrderEvent> {
        let (side, quantity) = match signal.direction {
            Direction::Long => (Side::Buy, self.0),
            Direction::Exit => {
                let current = portfolio.position(&signal.symbol).quantity;
                if current == 0.0 {
                    return None;
                }
                let side = if current > 0.0 { Side::Sell } else { Side::Buy };
                (side, current.abs())
            }
            Direction::Short => return None, // unused by these tests
        };
        Some(OrderEvent {
            id: 0, // engine assigns the real id
            symbol: signal.symbol.clone(),
            timestamp: signal.timestamp,
            side,
            order_type: OrderType::Market,
            quantity,
        })
    }
}

fn build_engine(bars: Vec<Bar>, script: Vec<(usize, Direction)>, quantity: f64) -> Engine {
    Engine::new(
        Box::new(VecFeed::new(bars)),
        Box::new(ScriptedStrategy::new(script)),
        Box::new(FixedQuantity(quantity)),
        Box::new(SimulatedExecution::new()),
        INITIAL_CASH,
    )
}

#[test]
fn market_orders_fill_at_next_bar_open_not_current_bar_close() {
    // Bar 0 is unremarkable; the strategy signals Long here. Bar 1 has a
    // massive price jump -- if the engine ever filled against the bar that
    // generated the order (bar 0's close) or against bar 1's close instead
    // of its open, this test would catch it.
    let bars = vec![
        bar(1, 100.0, 100.0), // bar 0: open == close == 100
        bar(2, 500.0, 505.0), // bar 1: massive jump; open=500, close=505
        bar(3, 505.0, 505.0), // bar 2: lets the fill settle before asserting
    ];
    let mut engine = build_engine(bars, vec![(0, Direction::Long)], 10.0);
    let summary = engine.run();

    assert_eq!(summary.fills, 1);
    let position = engine.portfolio().position(&symbol());
    assert_eq!(position.quantity, 10.0);
    // The load-bearing assertion: filled at bar 1's OPEN (500), never at
    // bar 0's close (100) and never at bar 1's close (505).
    assert_eq!(position.avg_entry_price, 500.0);
    assert_eq!(
        engine.portfolio().cash,
        INITIAL_CASH - 10.0 * 500.0,
        "cash must reflect a fill at 500 (bar 1 open), not 100 or 505"
    );
}

#[test]
fn no_order_can_fill_before_it_is_submitted() {
    // A signal on the very last bar of the feed has nowhere to fill --
    // there is no bar T+1. This proves fills are never pulled forward
    // out of thin air; an order with no future bar simply never fills.
    let bars = vec![bar(1, 100.0, 101.0), bar(2, 101.0, 102.0)];
    let mut engine = build_engine(bars, vec![(1, Direction::Long)], 10.0);
    let summary = engine.run();

    assert_eq!(summary.orders_submitted, 1);
    assert_eq!(summary.fills, 0);
    assert_eq!(engine.portfolio().position(&symbol()).quantity, 0.0);
    assert_eq!(engine.portfolio().cash, INITIAL_CASH);
}

#[test]
fn known_answer_ten_bar_scenario() {
    // Hand-computed scenario: buy 10 shares on the signal at bar index 2,
    // which (per the T+1 rule) fills at bar 3's open of 103; exit on the
    // signal at bar index 6, filling at bar 7's open of 107.
    //
    // Expected by hand:
    //   buy:  10 shares @ 103 -> cash = 100,000 - 1,030 = 98,970
    //   sell: 10 shares @ 107 -> cash = 98,970 + 1,070 = 100,040
    //   realized PnL = (107 - 103) * 10 = 40
    //   final position: flat: quantity = 0, equity = cash = 100,040
    let bars = (0..10)
        .map(|i| {
            let o = 100.0 + i as f64;
            bar(i + 1, o, o + 1.0)
        })
        .collect();

    let mut engine = build_engine(bars, vec![(2, Direction::Long), (6, Direction::Exit)], 10.0);
    let summary = engine.run();

    assert_eq!(summary.fills, 2);
    let position = engine.portfolio().position(&symbol());
    assert_eq!(position.quantity, 0.0);
    assert_eq!(position.realized_pnl, 40.0);
    assert_eq!(engine.portfolio().cash, 100_040.0);
    assert_eq!(engine.portfolio().equity(), 100_040.0);
}

#[test]
fn conservation_holds_across_a_full_engine_run() {
    // cash + position market value must equal equity after every bar,
    // across a run that opens, holds, and closes a position through the
    // full engine (not just Portfolio::apply_fill in isolation).
    let bars: Vec<Bar> = (0..20)
        .map(|i| {
            let o = 100.0 + (i as f64 * 0.7).sin() * 5.0 + i as f64 * 0.3;
            bar(i + 1, o, o + 0.5)
        })
        .collect();

    let mut engine = build_engine(
        bars,
        vec![
            (1, Direction::Long),
            (10, Direction::Exit),
            (12, Direction::Long),
        ],
        10.0,
    );
    engine.run();

    let portfolio = engine.portfolio();
    let last_price = *portfolio.last_prices.get(&symbol()).unwrap();
    let position_value = portfolio.position(&symbol()).quantity * last_price;
    assert!(
        (portfolio.cash + position_value - portfolio.equity()).abs() < 1e-9,
        "cash + position value must equal equity at the end of the run"
    );

    // Every recorded equity snapshot must also satisfy `equity == cash +
    // position value` at the time it was taken. The scripted strategy
    // here never shorts, so `gross_exposure` (always non-negative) equals
    // signed position value, making this checkable directly from the
    // stored snapshot rather than needing to replay history.
    for point in &portfolio.equity_curve {
        assert!(
            point.equity.is_finite() && point.cash.is_finite(),
            "equity/cash must never become NaN or infinite"
        );
        assert!(
            (point.equity - point.cash - point.gross_exposure).abs() < 1e-9,
            "equity must equal cash + position value at every snapshot: {point:?}"
        );
    }
}
