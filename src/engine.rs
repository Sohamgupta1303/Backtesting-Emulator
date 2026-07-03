//! The event loop.
//!
//! Milestone 1 scope: pull `MarketEvent`s from a [`DataFeed`] one at a time,
//! advance the simulation clock, and drain the queue. Strategy, portfolio,
//! and execution wiring land in later milestones — this loop intentionally
//! does nothing with a market event yet beyond counting it, so that the
//! clock/queue plumbing can be tested in isolation first.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::data::DataFeed;
use crate::events::Event;

/// Tracks simulated "now." Enforces the core anti-lookahead invariant at
/// the clock level: time may only move forward. Any attempt to move it
/// backward indicates a bug upstream (e.g. an unsorted data feed) and is a
/// programming error, not a recoverable runtime condition — hence the
/// debug-only assertion rather than a `Result`.
#[derive(Debug, Default)]
pub struct SimClock {
    current: Option<DateTime<Utc>>,
}

impl SimClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now(&self) -> Option<DateTime<Utc>> {
        self.current
    }

    /// Advances the clock to `timestamp`. Panics in debug builds if
    /// `timestamp` is before the current time.
    pub fn advance_to(&mut self, timestamp: DateTime<Utc>) {
        if let Some(current) = self.current {
            debug_assert!(
                timestamp >= current,
                "SimClock moved backward: {current} -> {timestamp}"
            );
        }
        self.current = Some(timestamp);
    }
}

/// Drives the simulation: pulls market data, advances the clock, and
/// drains the resulting event queue in FIFO order.
pub struct Engine {
    queue: VecDeque<Event>,
    data_feed: Box<dyn DataFeed>,
    clock: SimClock,
}

/// Summary counters returned after a run, useful for tests and for eyeballing
/// that the loop actually processed the expected number of bars.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub market_events: usize,
}

impl Engine {
    pub fn new(data_feed: Box<dyn DataFeed>) -> Self {
        Self {
            queue: VecDeque::new(),
            data_feed,
            clock: SimClock::new(),
        }
    }

    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// Runs until the data feed is exhausted, draining the queue after each
    /// market event. Returns a summary of what was processed.
    pub fn run(&mut self) -> RunSummary {
        let mut summary = RunSummary::default();

        while let Some(market_event) = self.data_feed.next() {
            self.clock.advance_to(market_event.bar.timestamp);
            self.queue.push_back(Event::Market(market_event));

            while let Some(event) = self.queue.pop_front() {
                match event {
                    Event::Market(_) => summary.market_events += 1,
                    // Strategy/portfolio/execution stages are wired in
                    // later milestones; nothing produces these yet.
                    Event::Signal(_) | Event::Order(_) | Event::Fill(_) => {
                        unreachable!("no stage produces Signal/Order/Fill events until milestone 2")
                    }
                }
            }
        }

        summary
    }
}
