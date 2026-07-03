//! Determinism test: this simulation is single-threaded with no async
//! runtime and no RNG-based models in use here, so the same data and
//! config must produce byte-identical output on every run -- no hidden
//! sources of nondeterminism (hash-map iteration order leaking into
//! output, wall-clock timestamps, thread scheduling, etc).

use chronos_bt::data::csv_feed::{CsvFeed, TimestampPolicy};
use chronos_bt::data::types::SymbolId;
use chronos_bt::engine::Engine;
use chronos_bt::execution::simulated::SimulatedExecution;
use chronos_bt::metrics::report;
use chronos_bt::metrics::returns::TRADING_DAYS_PER_YEAR;
use chronos_bt::portfolio::sizing::FixedFraction;
use chronos_bt::strategy::MaCrossover;

const INITIAL_CASH: f64 = 100_000.0;

/// Runs the full pipeline against the checked-in sample dataset and
/// returns the resulting metrics report serialized as pretty JSON --
/// exactly what would be embedded in `results.json`.
fn run_once() -> String {
    let feed = CsvFeed::load(
        "data/sample/spy_daily.csv",
        SymbolId::new("SPY"),
        TimestampPolicy::Reject,
    )
    .expect("sample data should load");

    let mut engine = Engine::new(
        Box::new(feed),
        Box::new(MaCrossover::new(10, 30)),
        Box::new(FixedFraction::new(0.10)),
        Box::new(SimulatedExecution::new()),
        INITIAL_CASH,
    );
    engine.run();

    let full_report =
        report::compute_report(engine.portfolio(), INITIAL_CASH, TRADING_DAYS_PER_YEAR, 0.0);
    serde_json::to_string_pretty(&full_report).expect("report should serialize")
}

#[test]
fn same_data_and_config_produce_byte_identical_results() {
    let first = run_once();
    let second = run_once();
    assert_eq!(
        first, second,
        "two runs with identical inputs must be byte-identical"
    );
}
