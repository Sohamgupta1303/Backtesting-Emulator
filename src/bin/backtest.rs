//! Minimal CLI entry point.
//!
//! This is a placeholder that proves the pipeline end to end (load CSV ->
//! run event loop -> report a summary) using the MA crossover reference
//! strategy. The real `clap`-based CLI (strategy selection, TOML config,
//! `--set` overrides) is built in milestone 5.

use chronos_bt::data::csv_feed::{CsvFeed, TimestampPolicy};
use chronos_bt::data::types::SymbolId;
use chronos_bt::engine::Engine;
use chronos_bt::execution::simulated::SimulatedExecution;
use chronos_bt::portfolio::sizing::FixedFraction;
use chronos_bt::strategy::MaCrossover;

const INITIAL_CASH: f64 = 100_000.0;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data/sample/spy_daily.csv".to_string());

    let feed = CsvFeed::load(&path, SymbolId::new("SPY"), TimestampPolicy::Reject)?;
    let mut engine = Engine::new(
        Box::new(feed),
        Box::new(MaCrossover::new(10, 30)),
        Box::new(FixedFraction::new(0.10)),
        Box::new(SimulatedExecution::new()),
        INITIAL_CASH,
    );
    let summary = engine.run();

    println!("loaded: {path}");
    println!("market events processed: {}", summary.market_events);
    println!("signals emitted: {}", summary.signals_emitted);
    println!("orders submitted: {}", summary.orders_submitted);
    println!("fills: {}", summary.fills);
    println!("final equity: {:.2}", engine.portfolio().equity());
    Ok(())
}
