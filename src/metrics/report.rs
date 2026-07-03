//! Assembles the full metrics suite into one report, and renders it as a
//! text table, an equity-curve PNG, or (by embedding it alongside the run
//! config) `results.json`.

use std::path::Path;

use plotters::prelude::*;
use plotters::series::AreaSeries;
use serde::Serialize;

use crate::portfolio::Portfolio;

use super::drawdown::{self, DrawdownReport};
use super::returns;
use super::trades::{self, TradeStats};

/// Every metric computed from a completed backtest run.
#[derive(Debug, Clone, Serialize)]
pub struct FullReport {
    pub total_return: f64,
    pub cagr: f64,
    pub annualized_volatility: f64,
    pub sharpe_ratio: Option<f64>,
    pub sortino_ratio: Option<f64>,
    pub max_drawdown: Option<DrawdownReport>,
    pub calmar_ratio: Option<f64>,
    pub trade_stats: TradeStats,
    pub total_commission: f64,
    pub total_slippage_cost: f64,
    /// Echoed back so the report is self-describing: readers don't have
    /// to go find the run config to know what annualization/risk-free
    /// assumptions the ratios above used.
    pub periods_per_year: f64,
    pub risk_free_rate: f64,
}

/// Computes every metric from a portfolio's equity curve, fill log, and
/// closed-trade log. `periods_per_year` must match the data's bar
/// frequency (e.g. `252.0` for daily equities) — nothing here assumes a
/// frequency silently. `risk_free_rate` is an annual rate (`0.0` is a
/// common simplifying default).
pub fn compute_report(
    portfolio: &Portfolio,
    initial_cash: f64,
    periods_per_year: f64,
    risk_free_rate: f64,
) -> FullReport {
    let cagr = returns::cagr(&portfolio.equity_curve, initial_cash, periods_per_year);
    let period_returns = returns::period_returns(&portfolio.equity_curve, initial_cash);
    let max_drawdown = drawdown::max_drawdown(&portfolio.equity_curve);

    FullReport {
        total_return: returns::total_return(&portfolio.equity_curve, initial_cash),
        cagr,
        annualized_volatility: returns::annualized_volatility(&period_returns, periods_per_year),
        sharpe_ratio: returns::sharpe_ratio(&period_returns, risk_free_rate, periods_per_year),
        sortino_ratio: returns::sortino_ratio(&period_returns, risk_free_rate, periods_per_year),
        calmar_ratio: max_drawdown.and_then(|d| drawdown::calmar_ratio(cagr, d.max_drawdown)),
        max_drawdown,
        trade_stats: trades::trade_stats(&portfolio.closed_trades),
        total_commission: trades::total_commission(&portfolio.fills),
        total_slippage_cost: trades::total_slippage_cost(&portfolio.fills),
        periods_per_year,
        risk_free_rate,
    }
}

fn format_ratio(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{v:.3}"),
        None => "n/a".to_string(),
    }
}

fn push_row(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("{label:<28}{value:>18}\n"));
}

/// Renders `report` as an aligned, human-readable text table.
pub fn render_text_report(report: &FullReport) -> String {
    let mut out = String::new();
    push_row(
        &mut out,
        "Total return",
        &format!("{:.2}%", report.total_return * 100.0),
    );
    push_row(&mut out, "CAGR", &format!("{:.2}%", report.cagr * 100.0));
    push_row(
        &mut out,
        "Annualized volatility",
        &format!("{:.2}%", report.annualized_volatility * 100.0),
    );
    push_row(&mut out, "Sharpe ratio", &format_ratio(report.sharpe_ratio));
    push_row(
        &mut out,
        "Sortino ratio",
        &format_ratio(report.sortino_ratio),
    );

    match &report.max_drawdown {
        Some(dd) => {
            push_row(
                &mut out,
                "Max drawdown",
                &format!("{:.2}%", dd.max_drawdown * 100.0),
            );
            push_row(
                &mut out,
                "  peak",
                &dd.peak_timestamp.date_naive().to_string(),
            );
            push_row(
                &mut out,
                "  trough",
                &dd.trough_timestamp.date_naive().to_string(),
            );
            push_row(
                &mut out,
                "  recovery",
                &dd.recovery_timestamp
                    .map(|t| t.date_naive().to_string())
                    .unwrap_or_else(|| "not recovered".to_string()),
            );
        }
        None => push_row(&mut out, "Max drawdown", "n/a"),
    }
    push_row(&mut out, "Calmar ratio", &format_ratio(report.calmar_ratio));

    push_row(
        &mut out,
        "Number of trades",
        &report.trade_stats.number_of_trades.to_string(),
    );
    push_row(
        &mut out,
        "Win rate",
        &format!("{:.1}%", report.trade_stats.win_rate * 100.0),
    );
    push_row(
        &mut out,
        "Average win",
        &format!("{:.2}", report.trade_stats.average_win),
    );
    push_row(
        &mut out,
        "Average loss",
        &format!("{:.2}", report.trade_stats.average_loss),
    );
    push_row(
        &mut out,
        "Profit factor",
        &format_ratio(report.trade_stats.profit_factor),
    );
    push_row(
        &mut out,
        "Avg holding period (days)",
        &format!("{:.1}", report.trade_stats.average_holding_period_days),
    );

    push_row(
        &mut out,
        "Total commission paid",
        &format!("{:.2}", report.total_commission),
    );
    push_row(
        &mut out,
        "Total slippage cost",
        &format!("{:.2}", report.total_slippage_cost),
    );
    out
}

/// Renders an equity curve (top) and drawdown-from-peak (bottom) chart to
/// a PNG at `path`. The x-axis is bar index rather than calendar date — a
/// known simplification, see the README's limitations section.
pub fn render_equity_curve_png(
    portfolio: &Portfolio,
    path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let equity_curve = &portfolio.equity_curve;
    if equity_curve.is_empty() {
        anyhow::bail!("cannot render an equity curve chart: no equity points recorded");
    }

    let min_equity = equity_curve
        .iter()
        .map(|p| p.equity)
        .fold(f64::INFINITY, f64::min);
    let max_equity = equity_curve
        .iter()
        .map(|p| p.equity)
        .fold(f64::NEG_INFINITY, f64::max);
    let padding = ((max_equity - min_equity) * 0.05).max(1.0);

    let drawdowns = drawdown::drawdown_series(equity_curve);
    let max_drawdown = drawdowns.iter().copied().fold(0.0_f64, f64::max).max(0.01);

    let num_bars = equity_curve.len() as f64;

    let root = BitMapBackend::new(path.as_ref(), (1200, 800)).into_drawing_area();
    root.fill(&WHITE)?;
    let (equity_area, drawdown_area) = root.split_vertically(560);

    let mut equity_chart = ChartBuilder::on(&equity_area)
        .caption("Equity Curve", ("sans-serif", 24))
        .margin(15)
        .x_label_area_size(30)
        .y_label_area_size(80)
        .build_cartesian_2d(
            0.0..num_bars.max(1.0),
            (min_equity - padding)..(max_equity + padding),
        )?;
    equity_chart
        .configure_mesh()
        .y_desc("Equity ($)")
        .x_desc("Bar")
        .draw()?;
    equity_chart.draw_series(LineSeries::new(
        equity_curve
            .iter()
            .enumerate()
            .map(|(i, p)| (i as f64, p.equity)),
        &BLUE,
    ))?;

    let mut drawdown_chart = ChartBuilder::on(&drawdown_area)
        .margin(15)
        .x_label_area_size(30)
        .y_label_area_size(80)
        .build_cartesian_2d(0.0..num_bars.max(1.0), 0.0..max_drawdown)?;
    drawdown_chart
        .configure_mesh()
        .y_desc("Drawdown")
        .x_desc("Bar")
        .draw()?;
    drawdown_chart.draw_series(AreaSeries::new(
        drawdowns.iter().enumerate().map(|(i, dd)| (i as f64, *dd)),
        0.0,
        RED.mix(0.4),
    ))?;

    root.present()?;
    Ok(())
}

/// Writes any serializable value (typically a combined config+metrics
/// struct) as pretty-printed JSON to `path`.
pub fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> anyhow::Result<()> {
    let file = std::fs::File::create(path.as_ref())?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}
