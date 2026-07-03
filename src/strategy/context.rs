//! The ring buffer and [`StrategyContext`] that make lookahead prevention
//! structural rather than a matter of discipline.

use std::collections::VecDeque;

use crate::data::types::{Bar, Quantity};

/// A fixed-capacity FIFO window of the most recent bars for one symbol.
/// Once full, pushing a new bar evicts the oldest one — like a security
/// camera that only keeps the last N hours of footage. This is what bounds
/// a strategy's view of the past: there is no way to reach further back
/// than `capacity` bars, no matter what the strategy code tries to do.
#[derive(Debug, Clone)]
pub struct RingBuffer {
    bars: VecDeque<Bar>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            bars: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Appends `bar`, evicting the oldest entry first if already at
    /// capacity.
    pub fn push(&mut self, bar: Bar) {
        if self.bars.len() == self.capacity {
            self.bars.pop_front();
        }
        self.bars.push_back(bar);
    }

    /// A snapshot of the buffered bars, oldest first. Returned by value
    /// (a small `Copy` struct per bar) so `StrategyContext` can hand it to
    /// a strategy without holding a reference back into engine-owned
    /// state — simpler to reason about than a borrow, and cheap since the
    /// buffer is small (bounded by a strategy's own warmup requirement).
    pub fn as_vec(&self) -> Vec<Bar> {
        self.bars.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }
}

/// The view of the world a [`Strategy`](super::Strategy) is allowed to
/// see: recent history for the symbol currently being processed (oldest
/// first, ending with the current bar), the strategy's current position
/// size in that symbol, and current cash. There is no field or method
/// here — now or in any future milestone — that reaches into data the
/// engine hasn't emitted yet.
#[derive(Debug, Clone, Default)]
pub struct StrategyContext {
    pub history: Vec<Bar>,
    pub position_quantity: Quantity,
    pub cash: f64,
}

impl StrategyContext {
    /// The most recent bar in `history` — i.e. the bar currently being
    /// processed. `None` only if warmup hasn't produced any history yet.
    pub fn latest(&self) -> Option<&Bar> {
        self.history.last()
    }
}
