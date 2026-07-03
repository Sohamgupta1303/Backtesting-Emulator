//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use crate::data::types::SymbolId;
use crate::events::{FillEvent, Side};
use crate::portfolio::Portfolio;

use super::report::{compute_report, render_equity_curve_png, render_text_report};

fn symbol() -> SymbolId {
    SymbolId::new("TEST")
}

fn sample_portfolio() -> Portfolio {
    let mut portfolio = Portfolio::new(10_000.0);
    portfolio.update_price(&symbol(), 100.0);
    for (day, price) in [(1, 100.0), (2, 105.0), (3, 95.0), (4, 110.0)] {
        portfolio.update_price(&symbol(), price);
        portfolio.snapshot_equity(chrono::Utc::now() + chrono::Duration::days(day));
    }
    portfolio.apply_fill(&FillEvent {
        order_id: 1,
        symbol: symbol(),
        timestamp: chrono::Utc::now(),
        side: Side::Buy,
        quantity_filled: 10.0,
        fill_price: 100.0,
        commission: 1.0,
        reference_price: 99.5,
    });
    portfolio
}

#[test]
fn compute_report_runs_end_to_end_without_panicking() {
    let portfolio = sample_portfolio();
    let report = compute_report(&portfolio, 10_000.0, 252.0, 0.0);
    assert!(report.total_return.is_finite());
    assert!(report.cagr.is_finite());
}

#[test]
fn render_text_report_includes_expected_labels() {
    let portfolio = sample_portfolio();
    let report = compute_report(&portfolio, 10_000.0, 252.0, 0.0);
    let text = render_text_report(&report);
    assert!(text.contains("Total return"));
    assert!(text.contains("Sharpe ratio"));
    assert!(text.contains("Max drawdown"));
    assert!(text.contains("Profit factor"));
    assert!(text.contains("Total commission paid"));
}

#[test]
fn render_equity_curve_png_writes_a_file() {
    let portfolio = sample_portfolio();
    let dir = std::env::temp_dir().join(format!("chronos-bt-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("equity_curve.png");

    render_equity_curve_png(&portfolio, &path).unwrap();
    assert!(path.exists());
    assert!(std::fs::metadata(&path).unwrap().len() > 0);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn render_equity_curve_png_errors_on_empty_curve() {
    let portfolio = Portfolio::new(10_000.0);
    let path = std::env::temp_dir().join("chronos-bt-test-empty.png");
    assert!(render_equity_curve_png(&portfolio, &path).is_err());
}
