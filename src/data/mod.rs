pub mod csv_feed;
pub mod fast_hash;
pub mod types;

#[cfg(test)]
mod csv_feed_tests;

use crate::events::MarketEvent;

/// A source of chronologically ordered market data.
///
/// The `next` contract is the entire lookahead-prevention guarantee at the
/// data-ingestion boundary: implementations must yield bars in strictly
/// non-decreasing timestamp order, and once a bar has been returned there
/// is no way for a caller to ask the feed for anything *after* it out of
/// order. The engine (not the feed) is responsible for pacing consumption
/// one bar at a time so strategies never see ahead of the simulation clock.
pub trait DataFeed {
    /// Returns the next market event, or `None` when the feed is exhausted.
    fn next(&mut self) -> Option<MarketEvent>;
}
