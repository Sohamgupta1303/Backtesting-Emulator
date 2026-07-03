//! The `backtest` CLI.
//!
//! ```text
//! backtest run --data data/sample/spy_daily.csv --strategy ma_crossover
//! backtest run --data ... --strategy mean_reversion --config configs/default.toml
//! backtest run --data ... --strategy mean_reversion --set entry_threshold=2.5
//! ```
//!
//! Config (TOML) controls initial capital, sizing, and execution model
//! parameters, plus whichever parameters the chosen `--strategy` needs.
//! `--set key=value` overrides individual strategy parameters. The fully
//! resolved config is printed at the start of every run and embedded in
//! `results.json`, so a run is reproducible from that file alone.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use chronos_bt::config::Config;
use chronos_bt::data::csv_feed::{CsvFeed, TimestampPolicy};
use chronos_bt::data::types::SymbolId;
use chronos_bt::engine::Engine;
use chronos_bt::execution::simulated::SimulatedExecution;
use chronos_bt::metrics::report;
use chronos_bt::metrics::returns::TRADING_DAYS_PER_YEAR;
use chronos_bt::portfolio::sizing::FixedFraction;
use chronos_bt::strategy::{MaCrossover, MeanReversion, Momentum, Strategy};

#[derive(Parser)]
#[command(
    name = "backtest",
    about = "chronos-bt: an event-driven backtesting engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a backtest.
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Path to the CSV data file.
    #[arg(long)]
    data: PathBuf,
    /// Which reference strategy to run.
    #[arg(long, value_parser = ["ma_crossover", "mean_reversion", "momentum"])]
    strategy: String,
    /// Optional TOML config file (initial capital, sizing, execution,
    /// strategy parameters).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override one strategy parameter: `--set key=value` (repeatable).
    #[arg(long = "set", value_parser = parse_key_val)]
    set: Vec<(String, String)>,
    /// Directory to write `equity_curve.png` and `results.json` into.
    #[arg(long, default_value = "report")]
    out_dir: PathBuf,
}

fn parse_key_val(raw: &str) -> Result<(String, String), String> {
    let (key, value) = raw
        .split_once('=')
        .ok_or_else(|| format!("invalid --set value {raw:?}: expected key=value"))?;
    Ok((key.to_string(), value.to_string()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => run(args),
    }
}

fn run(args: RunArgs) -> Result<()> {
    let mut config = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading config file {}", path.display()))?;
            toml::from_str::<Config>(&text)
                .with_context(|| format!("parsing config file {}", path.display()))?
        }
        None => Config::default(),
    };
    for (key, value) in &args.set {
        config
            .strategy
            .insert(key.clone(), toml::Value::String(value.clone()));
    }

    println!("Resolved config:");
    println!(
        "{}",
        toml::to_string_pretty(&config).context("serializing resolved config")?
    );

    let feed = CsvFeed::load(&args.data, SymbolId::new("SYMBOL"), TimestampPolicy::Reject)
        .with_context(|| format!("loading data file {}", args.data.display()))?;

    let strategy = build_strategy(&args.strategy, &config.strategy)?;
    let sizing = FixedFraction::new(config.sizing.fraction);
    let mut execution = SimulatedExecution::new()
        .with_slippage(config.execution.slippage.to_model())
        .with_commission(config.execution.commission.to_model())
        .with_limit_fill_policy(config.execution.limit_fill_policy.to_model())
        .with_latency_bars(config.execution.latency_bars);
    if let Some(bars) = config.execution.time_in_force_bars {
        execution = execution.with_time_in_force(bars);
    }

    let mut engine = Engine::new(
        Box::new(feed),
        strategy,
        Box::new(sizing),
        Box::new(execution),
        config.initial_capital,
    );
    let summary = engine.run();

    println!();
    println!("market events processed: {}", summary.market_events);
    println!("orders submitted:       {}", summary.orders_submitted);
    println!("orders rejected:        {}", summary.orders_rejected);
    println!("fills:                  {}", summary.fills);
    println!();

    let full_report = report::compute_report(
        engine.portfolio(),
        config.initial_capital,
        TRADING_DAYS_PER_YEAR,
        0.0,
    );
    println!("{}", report::render_text_report(&full_report));

    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("creating output directory {}", args.out_dir.display()))?;

    let png_path = args.out_dir.join("equity_curve.png");
    report::render_equity_curve_png(engine.portfolio(), &png_path)?;
    println!("wrote {}", png_path.display());

    let results_path = args.out_dir.join("results.json");
    let results = serde_json::json!({
        "config": config,
        "metrics": full_report,
    });
    report::write_json(&results_path, &results)?;
    println!("wrote {}", results_path.display());

    Ok(())
}

fn build_strategy(name: &str, params: &HashMap<String, toml::Value>) -> Result<Box<dyn Strategy>> {
    match name {
        "ma_crossover" => {
            let fast = get_usize(params, "fast", 10)?;
            let slow = get_usize(params, "slow", 30)?;
            Ok(Box::new(MaCrossover::new(fast, slow)))
        }
        "mean_reversion" => {
            let lookback = get_usize(params, "lookback", 20)?;
            let entry_threshold = get_f64(params, "entry_threshold", 2.0)?;
            let exit_threshold = get_f64(params, "exit_threshold", 0.5)?;
            Ok(Box::new(MeanReversion::new(
                lookback,
                entry_threshold,
                exit_threshold,
            )))
        }
        "momentum" => {
            let lookback = get_usize(params, "lookback", 20)?;
            let threshold = get_f64(params, "threshold", 0.05)?;
            Ok(Box::new(Momentum::new(lookback, threshold)))
        }
        other => bail!(
            "unknown strategy {other:?}; expected one of: ma_crossover, mean_reversion, momentum"
        ),
    }
}

fn get_f64(params: &HashMap<String, toml::Value>, key: &str, default: f64) -> Result<f64> {
    match params.get(key) {
        None => Ok(default),
        Some(toml::Value::Float(f)) => Ok(*f),
        Some(toml::Value::Integer(i)) => Ok(*i as f64),
        Some(toml::Value::String(s)) => s
            .parse::<f64>()
            .with_context(|| format!("invalid numeric value for {key}: {s:?}")),
        Some(other) => bail!("invalid type for {key}: expected a number, got {other:?}"),
    }
}

fn get_usize(params: &HashMap<String, toml::Value>, key: &str, default: usize) -> Result<usize> {
    match params.get(key) {
        None => Ok(default),
        Some(toml::Value::Integer(i)) => Ok(*i as usize),
        Some(toml::Value::String(s)) => s
            .parse::<usize>()
            .with_context(|| format!("invalid integer value for {key}: {s:?}")),
        Some(other) => bail!("invalid type for {key}: expected an integer, got {other:?}"),
    }
}
