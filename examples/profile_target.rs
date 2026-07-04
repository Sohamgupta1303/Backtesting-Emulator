//! Not part of the public API -- a throwaway target for profiling the
//! engine loop with `samply` (`cargo build --release --example
//! profile_target`, then `samply record ./target/release/examples/profile_target`).
//! Runs MA crossover over a large synthetic run several times in a row so
//! a sampling profiler has enough wall-clock time to collect samples.

use std::collections::VecDeque;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};

use chronos_bt::data::types::{Bar, SymbolId};
use chronos_bt::data::DataFeed;
use chronos_bt::engine::Engine;
use chronos_bt::events::MarketEvent;
use chronos_bt::execution::simulated::SimulatedExecution;
use chronos_bt::portfolio::sizing::FixedFraction;
use chronos_bt::strategy::MaCrossover;

fn synthetic_bars(n: usize) -> Vec<Bar> {
    let start: DateTime<Utc> = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    let mut bars = Vec::with_capacity(n);
    let mut price = 100.0_f64;
    for i in 0..n {
        let wiggle = ((i as f64) * 0.01).sin() * 0.5;
        let close = (price * 1.0001 + wiggle).max(1.0);
        let open = price;
        let high = open.max(close) + 0.1;
        let low = (open.min(close) - 0.1).max(0.01);
        bars.push(Bar {
            timestamp: start + ChronoDuration::seconds(i as i64 * 86_400),
            open,
            high,
            low,
            close,
            volume: 1_000_000.0,
        });
        price = close;
    }
    bars
}

struct VecFeed(VecDeque<MarketEvent>);

impl DataFeed for VecFeed {
    fn next(&mut self) -> Option<MarketEvent> {
        self.0.pop_front()
    }
}

fn main() {
    let bars = synthetic_bars(10_000_000);
    for round in 0..8 {
        let symbol = SymbolId::new("PROFILE");
        let feed = VecFeed(
            bars.iter()
                .map(|bar| MarketEvent { symbol: symbol.clone(), bar: *bar })
                .collect(),
        );
        let mut engine = Engine::new(
            Box::new(feed),
            Box::new(MaCrossover::new(10, 30)),
            Box::new(FixedFraction::new(0.10)),
            Box::new(SimulatedExecution::new()),
            100_000.0,
        );
        let summary = engine.run();
        println!("round {round}: {} bars, {} fills", summary.market_events, summary.fills);
    }
}
