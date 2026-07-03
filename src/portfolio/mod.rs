//! Portfolio state and the `SizingModel` trait.
//!
//! Full behavior — mark-to-market, order rejection for insufficient cash,
//! sizing model implementations — lands in milestone 2. This module
//! currently defines the data shapes and trait referenced by the engine's
//! dependency graph.

pub mod sizing;

use std::collections::HashMap;

use crate::data::types::{Price, Quantity, SymbolId};
use crate::events::{OrderEvent, SignalEvent};

/// An open position in a single symbol.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub quantity: Quantity,
    pub avg_entry_price: Price,
    pub realized_pnl: f64,
}

/// Cash, open positions, and (in later milestones) the equity time series.
#[derive(Debug, Clone, PartialEq)]
pub struct Portfolio {
    pub cash: f64,
    pub positions: HashMap<SymbolId, Position>,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            cash: initial_cash,
            positions: HashMap::new(),
        }
    }
}

/// Converts a `SignalEvent` (intent) into a sized `OrderEvent`, given the
/// current portfolio state.
pub trait SizingModel {
    fn size(&self, signal: &SignalEvent, portfolio: &Portfolio) -> Option<OrderEvent>;
}
