use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};

/// Price representation used throughout the engine.
///
/// v1 uses `f64` for simplicity and because standard quant metrics (Sharpe,
/// drawdown, etc.) are naturally floating-point computations anyway.
/// Production trading systems typically use fixed-point integer ticks
/// (e.g. cents or exchange-defined tick sizes) to avoid floating-point
/// rounding drift accumulating over millions of arithmetic operations and
/// to guarantee exact equality comparisons. That tradeoff is out of scope
/// for v1 but documented here and in the README.
pub type Price = f64;

/// Share/contract quantity. `f64` for the same reasons as [`Price`].
pub type Quantity = f64;

/// A ticker/instrument identifier.
///
/// Backed by `Arc<str>` rather than `String` so that cloning a `SymbolId`
/// (which happens on every event as it flows through the pipeline) is a
/// cheap reference-count bump instead of a heap allocation + copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(Arc<str>);

impl SymbolId {
    pub fn new(symbol: impl Into<Arc<str>>) -> Self {
        Self(symbol.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SymbolId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SymbolId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A single OHLCV price bar for one symbol at one point in time.
///
/// `timestamp` is always `DateTime<Utc>` — the engine never works with
/// naive datetimes, so there is no ambiguity about timezone when comparing
/// or ordering bars across data sources.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub timestamp: DateTime<Utc>,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
}

impl Bar {
    /// Checks the invariants a well-formed bar must satisfy:
    /// no NaN/negative prices, and `low <= {open, close} <= high`.
    pub fn is_valid(&self) -> bool {
        let prices = [self.open, self.high, self.low, self.close];
        if prices.iter().any(|p| p.is_nan() || *p < 0.0) {
            return false;
        }
        if self.volume.is_nan() || self.volume < 0.0 {
            return false;
        }
        self.high >= self.low
            && self.high >= self.open
            && self.high >= self.close
            && self.low <= self.open
            && self.low <= self.close
    }
}
