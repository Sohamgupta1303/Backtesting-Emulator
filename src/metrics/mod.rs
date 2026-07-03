//! Performance metrics (Sharpe, Sortino, max drawdown, etc.) and reporting.

pub mod drawdown;
pub mod report;
pub mod returns;
pub mod trades;

#[cfg(test)]
mod drawdown_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod returns_tests;
#[cfg(test)]
mod trades_tests;
