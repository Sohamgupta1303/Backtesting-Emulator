# Performance

This documents one real, profiled optimization — not a pile of speculative
tweaks. Methodology: measure with `criterion`, profile with a sampling
profiler to find where time *actually* goes, fix the single largest cost,
re-measure, and report the honest delta.

## Hardware

- Apple M4, 10 cores, 16 GB RAM
- macOS (Darwin 24.6.0), rustc 1.96.1
- Benchmarks run via `cargo bench` (release profile, `debug = true` for
  symbol names during profiling)

## Baseline

`cargo bench` on synthetic data (deterministic, allocation-cheap price
series — see `benches/engine_throughput.rs`):

| Benchmark                     | Bars       | Time (median) | Throughput      |
|--------------------------------|-----------|---------------|------------------|
| `no_op_strategy` (empty loop)  | 1,000,000 | 73.7 ms       | 13.56 Melem/s    |
| `no_op_strategy`               | 10,000,000| 742.6 ms      | 13.47 Melem/s    |
| `ma_crossover` (real strategy) | 1,000,000 | 183.8 ms      | 5.44 Melem/s     |
| `ma_crossover`                 | 10,000,000| 3.058 s       | 3.27 Melem/s     |

The no-op path (which does no strategy work at all) sustains ~13.5M
bars/sec — comfortably in the "single-digit millions" range the project
targets. MA crossover is 2.5–4x slower *purely from bookkeeping* (a
50-bar moving average is a handful of floating point operations; it should
not cost multiple bars-per-second by itself), and its measurements had
noticeably higher variance run to run (the 1M-bar case ranged 131–233 ms
across samples) — both signs of a real cost worth finding, not just
strategy math.

## Profiling

Profiled with [`samply`](https://github.com/mstange/samply) (a sampling
profiler; `perf`/`dtrace` were unavailable in this environment) against a
dedicated profiling target (`examples/profile_target.rs`) running MA
crossover over 10M bars, repeated 8x for enough wall-clock sampling time.
Self-time (leaf-frame) samples, symbolicated and aggregated by function:

| % of samples | Function                                              |
|---------------|--------------------------------------------------------|
| 27.9%         | `Vec::from_iter` (via `spec_from_iter_nested`)          |
| 15.1%         | `sip::Hasher::write`                                    |
| 8.3%          | `BuildHasher::hash_one`                                 |
| 6.3%          | `_platform_memcmp`                                       |
| 6.0%          | `hashbrown::raw::RawIterRange::fold_impl`                |
| 5.3%          | `Engine::run`                                            |
| 3.2%          | `MaCrossover::on_market`                                 |
| 2.7%          | `Portfolio::snapshot_equity`                              |
| 2.2%          | `SimulatedExecution::on_bar`                              |

Two real costs stood out, both far larger than the strategy's own logic:

1. **28% in `Vec::from_iter`** — `RingBuffer::as_vec()` (called once per
   bar to build `StrategyContext::history`) allocates a brand-new
   `Vec<Bar>` every single bar. For `MaCrossover::new(10, 30)`, that's a
   fresh 30-element heap allocation + copy, 10 million times.
2. **~29% combined in SipHash-related functions** (`Hasher::write`,
   `hash_one`, and the `memcmp` used to compare keys) — every
   `HashMap<SymbolId, _>` lookup (position, last price, ring buffer, once
   per bar each) goes through Rust's *default* hasher, SipHash, which is
   deliberately slow because it's designed to resist hash-flooding
   attacks on untrusted input. `SymbolId` keys come from the user's own
   data file, not an adversary — that resistance buys nothing here.

## The optimization

Fixed #2: replaced the default `HashMap` hasher with a small,
non-cryptographic hasher (`FxHash`, as used by rustc and Firefox — ~15
lines, implemented inline in `src/data/fast_hash.rs` rather than adding a
dependency for it) for every `HashMap<SymbolId, _>` in the engine and
portfolio.

Chose this over fixing #1 (the ring buffer allocation) because: it
addresses a *larger* combined cost (29% vs. 28%), it's a lower-risk change
(swapping a `HashMap`'s hasher touches zero public type shapes that
strategies or tests construct directly, whereas `StrategyContext::history`
is a `Vec<Bar>` field several test files build by hand), and it's a single
well-understood fix rather than a data-structure redesign. Fixing #1 is
a natural next optimization if this project's scope grows further — noted
below, not attempted here, per "one honest optimization beats ten
speculative ones."

### Results

| Benchmark          | Bars       | Before (time / throughput)   | After (time / throughput)    | Change            |
|--------------------|-----------|-------------------------------|-------------------------------|--------------------|
| `no_op_strategy`   | 1,000,000 | 73.7 ms / 13.56 Melem/s        | 67.3 ms / 14.85 Melem/s        | **-8.5%** time     |
| `no_op_strategy`   | 10,000,000| 742.6 ms / 13.47 Melem/s       | 666.5 ms / 15.00 Melem/s       | **-10.2%** time    |
| `ma_crossover`     | 1,000,000 | 183.8 ms / 5.44 Melem/s        | 114.1 ms / 8.76 Melem/s        | **-25.0%** time    |
| `ma_crossover`     | 10,000,000| 3.058 s / 3.27 Melem/s         | 1.140 s / 8.77 Melem/s         | **-62.7%** time    |

(All deltas are criterion's own paired comparison against the pre-change
run, `p < 0.05` in every case — not noise.)

MA crossover at 10M bars is now **2.7x faster** and matches its own 1M-bar
throughput almost exactly (8.77 vs. 8.76 Melem/s) — before the fix, 10M
bars was noticeably *slower per-element* than 1M bars (3.27 vs. 5.44
Melem/s), a classic sign of a cost that scales with total work rather than
being a fixed overhead. That asymmetry is now gone. The no-op path, which
never touched the ring buffer's `Vec` allocation as heavily (its history
capacity is only 1 element, since it has no warmup), still improved
8–10% from the hasher change alone, confirming the HashMap lookups were a
real, broadly-felt cost and not an artifact specific to MA crossover.

## Known remaining cost (not fixed here)

`RingBuffer::as_vec()`'s per-bar allocation (~28% of samples, see above)
is the next candidate if further optimization is warranted. The fix would
change `StrategyContext::history` from an owned `Vec<Bar>` to something
that doesn't reallocate every bar (e.g. a persistent ring buffer exposing
a slice, or a small-vector type that avoids heap allocation for the common
small-warmup case) — deliberately not done in this pass, since it would
touch a type several test files construct directly, and the spec's
guidance is one measured, low-risk optimization over several speculative
ones.
