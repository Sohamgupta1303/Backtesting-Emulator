//! The event types that flow through the engine's queue.
//!
//! `MarketEvent -> SignalEvent -> OrderEvent -> FillEvent` is the one-way
//! pipeline described in the architecture doc. Each event only carries the
//! information the next stage needs to do its job — in particular,
//! `SignalEvent` carries *intent* (direction + strength), not a sized order.
//! Sizing is a distinct concern owned by the portfolio manager.

use chrono::{DateTime, Utc};

use crate::data::types::{Bar, Price, Quantity, SymbolId};

/// A unique identifier for an order, assigned by whatever sizes it.
pub type OrderId = u64;

/// The four event kinds that travel through the engine's FIFO queue.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Market(MarketEvent),
    Signal(SignalEvent),
    Order(OrderEvent),
    Fill(FillEvent),
}

/// A new bar has become available for `symbol`.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketEvent {
    pub symbol: SymbolId,
    pub bar: Bar,
}

/// The direction of exposure a strategy wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Long,
    Short,
    Exit,
}

/// A strategy's expressed intent: "I want exposure in this direction, with
/// this much conviction." This is deliberately not a share count — the
/// portfolio manager decides how much capital that conviction deserves.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalEvent {
    pub symbol: SymbolId,
    pub timestamp: DateTime<Utc>,
    pub direction: Direction,
    pub strength: f64,
}

/// Which side of the market an order/fill is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// How an order should be executed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderType {
    Market,
    Limit { price: Price },
}

/// A concrete, sized instruction to trade, produced by the portfolio
/// manager/sizing model from a `SignalEvent`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderEvent {
    pub id: OrderId,
    pub symbol: SymbolId,
    pub timestamp: DateTime<Utc>,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Quantity,
}

/// A (possibly partial) execution report for a previously submitted order.
#[derive(Debug, Clone, PartialEq)]
pub struct FillEvent {
    pub order_id: OrderId,
    pub symbol: SymbolId,
    pub timestamp: DateTime<Utc>,
    pub side: Side,
    pub quantity_filled: Quantity,
    pub fill_price: Price,
    pub commission: f64,
}
