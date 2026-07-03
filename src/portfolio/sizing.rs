//! Concrete `SizingModel` implementations.

use super::{Portfolio, SizingModel};
use crate::events::{Direction, OrderEvent, OrderType, Side, SignalEvent};

/// Sizes every new position at a fixed fraction of current equity (e.g.
/// `0.10` = 10% of equity per position). `Exit` signals close whatever
/// position is currently open, regardless of fraction.
///
/// Volatility-targeted sizing (`VolatilityTarget`) is a later milestone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedFraction {
    pub fraction: f64,
}

impl FixedFraction {
    pub fn new(fraction: f64) -> Self {
        Self { fraction }
    }
}

impl SizingModel for FixedFraction {
    fn size(&self, signal: &SignalEvent, portfolio: &Portfolio) -> Option<OrderEvent> {
        let price = *portfolio.last_prices.get(&signal.symbol)?;
        if price <= 0.0 {
            return None;
        }
        let position = portfolio.position(&signal.symbol);

        let (side, quantity) = match signal.direction {
            Direction::Exit => {
                if position.quantity == 0.0 {
                    return None;
                }
                let side = if position.quantity > 0.0 {
                    Side::Sell
                } else {
                    Side::Buy
                };
                (side, position.quantity.abs())
            }
            Direction::Long | Direction::Short => {
                let target_quantity = (self.fraction * portfolio.equity()) / price;
                let signed_target = match signal.direction {
                    Direction::Long => target_quantity,
                    _ => -target_quantity,
                };
                let delta = signed_target - position.quantity;
                if delta.abs() < 1e-9 {
                    return None;
                }
                let side = if delta > 0.0 { Side::Buy } else { Side::Sell };
                (side, delta.abs())
            }
        };

        Some(OrderEvent {
            id: 0, // placeholder; the engine assigns the real id
            symbol: signal.symbol.clone(),
            timestamp: signal.timestamp,
            side,
            order_type: OrderType::Market,
            quantity,
        })
    }
}
