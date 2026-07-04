//! Throughput benchmark: bars/sec through the full event loop, with (a) a
//! no-op strategy (isolates the cost of the loop itself: queue draining,
//! ring buffer maintenance, equity snapshotting) and (b) MA crossover (a
//! real strategy doing real work). Run with `cargo bench`.

use std::collections::VecDeque;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

use chronos_bt::data::types::{Bar, SymbolId};
use chronos_bt::data::DataFeed;
use chronos_bt::engine::Engine;
use chronos_bt::events::MarketEvent;
use chronos_bt::execution::simulated::SimulatedExecution;
use chronos_bt::portfolio::sizing::FixedFraction;
use chronos_bt::strategy::{MaCrossover, NoOpStrategy, Strategy};

/// A deterministic, allocation-free-per-bar synthetic price series: no
/// RNG dependency, just a drift plus a bounded oscillation, so bar
/// generation itself doesn't skew the measured throughput.
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

fn build_engine(bars: &[Bar], strategy: Box<dyn Strategy>) -> Engine {
    let symbol = SymbolId::new("BENCH");
    let feed = VecFeed(
        bars.iter()
            .map(|bar| MarketEvent { symbol: symbol.clone(), bar: *bar })
            .collect(),
    );
    Engine::new(
        Box::new(feed),
        strategy,
        Box::new(FixedFraction::new(0.10)),
        Box::new(SimulatedExecution::new()),
        100_000.0,
    )
}

fn bench_no_op_strategy(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_op_strategy");
    group.sample_size(10);
    for &n in &[1_000_000usize, 10_000_000usize] {
        let bars = synthetic_bars(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(format!("{n}_bars"), |b| {
            b.iter_batched(
                || build_engine(&bars, Box::new(NoOpStrategy)),
                |mut engine| engine.run(),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_ma_crossover(c: &mut Criterion) {
    let mut group = c.benchmark_group("ma_crossover");
    group.sample_size(10);
    for &n in &[1_000_000usize, 10_000_000usize] {
        let bars = synthetic_bars(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_function(format!("{n}_bars"), |b| {
            b.iter_batched(
                || build_engine(&bars, Box::new(MaCrossover::new(10, 30))),
                |mut engine| engine.run(),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, bench_no_op_strategy, bench_ma_crossover);
criterion_main!(benches);
