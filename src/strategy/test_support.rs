//! Shared test helpers for exercising a `Strategy` directly (bypassing the
//! engine/ring buffer), used by both the MA crossover and momentum tests.
#![allow(clippy::unwrap_used)]

use chrono::{Days, TimeZone, Utc};

use super::{Strategy, StrategyContext};
use crate::data::types::{Bar, SymbolId};
use crate::events::{Direction, MarketEvent};

/// `day` counts days past 2021-01-01 rather than a literal day-of-month,
/// so callers can pass arbitrarily large sequences without overflowing a
/// calendar month.
pub(super) fn bar(day: u32, close: f64) -> Bar {
    let epoch = Utc.with_ymd_and_hms(2021, 1, 1, 0, 0, 0).unwrap();
    Bar {
        timestamp: epoch + Days::new(day as u64),
        open: close,
        high: close,
        low: close,
        close,
        volume: 1_000.0,
    }
}

/// Feeds `closes` to `strategy` one bar at a time (a growing history, not
/// through the engine/ring buffer) and returns the direction of every
/// signal that was actually emitted, in order.
pub(super) fn run_signals(strategy: &mut dyn Strategy, closes: &[f64]) -> Vec<Direction> {
    let mut history = Vec::new();
    let mut directions = Vec::new();
    for (i, &close) in closes.iter().enumerate() {
        let current_bar = bar(i as u32 + 1, close);
        history.push(current_bar);
        let event = MarketEvent {
            symbol: SymbolId::new("TEST"),
            bar: current_bar,
        };
        let ctx = StrategyContext {
            history: history.clone(),
            position_quantity: 0.0,
            cash: 0.0,
        };
        for signal in strategy.on_market(&event, &ctx) {
            directions.push(signal.direction);
        }
    }
    directions
}
