//! The event loop.
//!
//! Ties together the data feed, strategy, sizing model, execution model,
//! and portfolio into the pipeline described in the architecture doc:
//! `MarketEvent -> Strategy -> SignalEvent -> SizingModel -> OrderEvent ->
//! ExecutionModel -> FillEvent -> Portfolio`.
//!
//! Per bar: the execution model gets first look (so orders resting from
//! *previous* bars can fill at this bar's open), then the strategy is
//! dispatched, then the queue is drained fully before the next bar is
//! pulled. This ordering is what makes the T+1-fill rule hold structurally
//! rather than by convention — see `execution::simulated::SimulatedExecution`
//! for the mechanism.

#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::data::DataFeed;
use crate::events::{Event, OrderId};
use crate::execution::ExecutionModel;
use crate::portfolio::{Portfolio, SizingModel};
use crate::strategy::{Strategy, StrategyContext};

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

/// Summary counters returned after a run, useful for tests and for eyeballing
/// that the loop actually processed the expected number of bars.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub market_events: usize,
    pub signals_emitted: usize,
    pub orders_submitted: usize,
    pub orders_rejected: usize,
    pub fills: usize,
}

/// Drives the simulation: pulls market data, advances the clock, dispatches
/// to the strategy, sizes and submits orders, and applies fills to the
/// portfolio — draining the event queue in FIFO order at every timestamp.
pub struct Engine {
    queue: VecDeque<Event>,
    data_feed: Box<dyn DataFeed>,
    strategy: Box<dyn Strategy>,
    sizing: Box<dyn SizingModel>,
    execution: Box<dyn ExecutionModel>,
    portfolio: Portfolio,
    clock: SimClock,
    bars_seen: usize,
    next_order_id: OrderId,
}

impl Engine {
    pub fn new(
        data_feed: Box<dyn DataFeed>,
        strategy: Box<dyn Strategy>,
        sizing: Box<dyn SizingModel>,
        execution: Box<dyn ExecutionModel>,
        initial_cash: f64,
    ) -> Self {
        Self {
            queue: VecDeque::new(),
            data_feed,
            strategy,
            sizing,
            execution,
            portfolio: Portfolio::new(initial_cash),
            clock: SimClock::new(),
            bars_seen: 0,
            next_order_id: 1,
        }
    }

    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    pub fn portfolio(&self) -> &Portfolio {
        &self.portfolio
    }

    /// Runs until the data feed is exhausted. Returns a summary of what was
    /// processed.
    pub fn run(&mut self) -> RunSummary {
        let mut summary = RunSummary::default();

        while let Some(market_event) = self.data_feed.next() {
            self.clock.advance_to(market_event.bar.timestamp);
            self.bars_seen += 1;

            // Step 1: let resting orders from previous bars attempt to
            // fill at *this* bar's open, before the strategy sees this bar
            // at all.
            for fill in self.execution.on_bar(&market_event) {
                self.queue.push_back(Event::Fill(fill));
            }

            // Step 2: dispatch this bar to the strategy. Signals are
            // suppressed (dropped, not merely delayed) until warmup is
            // satisfied, per `Strategy::warmup_bars`.
            let ctx = StrategyContext::default();
            let signals = self.strategy.on_market(&market_event, &ctx);
            if self.bars_seen > self.strategy.warmup_bars() {
                for signal in signals {
                    self.queue.push_back(Event::Signal(signal));
                }
            }

            // Bar close is information the strategy has already legitimately
            // seen, so it's safe to use for equity/sizing bookkeeping this
            // same timestamp (only *fills* are deferred to T+1).
            self.portfolio
                .update_price(&market_event.symbol, market_event.bar.close);

            // Step 3: drain the queue. Fills (from step 1) update the
            // portfolio; signals become sized orders; orders are checked
            // for affordability and submitted to the execution model for
            // eligibility starting next bar.
            while let Some(event) = self.queue.pop_front() {
                match event {
                    Event::Fill(fill) => {
                        self.portfolio.apply_fill(&fill);
                        summary.fills += 1;
                    }
                    Event::Signal(signal) => {
                        summary.signals_emitted += 1;
                        if let Some(mut order) = self.sizing.size(&signal, &self.portfolio) {
                            order.id = self.next_order_id;
                            self.next_order_id += 1;
                            self.queue.push_back(Event::Order(order));
                        }
                    }
                    Event::Order(order) => {
                        let reference_price = market_event.bar.close;
                        if self.portfolio.can_afford(&order, reference_price) {
                            summary.orders_submitted += 1;
                            self.execution.submit(order);
                        } else {
                            summary.orders_rejected += 1;
                            eprintln!("warning: order rejected (insufficient cash): {order:?}");
                        }
                    }
                    Event::Market(_) => {
                        unreachable!("MarketEvents are dispatched directly, never queued")
                    }
                }
            }

            // Step 4: snapshot equity once the queue is fully drained for
            // this timestamp.
            self.portfolio.snapshot_equity(market_event.bar.timestamp);
            summary.market_events += 1;
        }

        summary
    }
}
