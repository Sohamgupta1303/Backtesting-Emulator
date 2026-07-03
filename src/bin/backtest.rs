//! Minimal CLI entry point.
//!
//! This is a placeholder that proves the milestone-1 pipeline end to end
//! (load CSV -> run event loop -> report bar count). The real `clap`-based
//! CLI (strategy selection, TOML config, `--set` overrides) is built in
//! milestone 5.

use chronos_bt::data::csv_feed::{CsvFeed, TimestampPolicy};
use chronos_bt::data::types::SymbolId;
use chronos_bt::engine::Engine;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data/sample/spy_daily.csv".to_string());

    let feed = CsvFeed::load(&path, SymbolId::new("SPY"), TimestampPolicy::Reject)?;
    let mut engine = Engine::new(Box::new(feed));
    let summary = engine.run();

    println!("loaded: {path}");
    println!("market events processed: {}", summary.market_events);
    Ok(())
}
