//! CSV market data ingestion.
//!
//! Expected schema: `timestamp,open,high,low,close,volume`. The timestamp
//! column accepts either RFC3339 (`2020-01-02T00:00:00Z`), a bare date
//! (`2020-01-02`, assumed UTC midnight), or unix milliseconds — whichever
//! format is present is auto-detected from the first row.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::types::{Bar, SymbolId};
use super::DataFeed;
use crate::events::MarketEvent;

/// How to handle a CSV file whose timestamps are not strictly
/// non-decreasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampPolicy {
    /// Fail with [`DataError::OutOfOrderTimestamp`].
    Reject,
    /// Sort all rows into ascending timestamp order before use.
    SortAscending,
}

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("failed to open data file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("csv error at row {row}: {source}")]
    Csv {
        row: usize,
        #[source]
        source: csv::Error,
    },

    #[error("row {row}: could not parse timestamp {raw:?}")]
    InvalidTimestamp { row: usize, raw: String },

    #[error(
        "row {row}: bar failed validation (NaN/negative price, or high/low/open/close \
         ordering violated): {bar:?}"
    )]
    InvalidBar { row: usize, bar: Bar },

    #[error(
        "row {row}: timestamp {curr} is not after previous timestamp {prev} \
         (strictly non-decreasing order required; pass TimestampPolicy::SortAscending \
         to sort instead of rejecting)"
    )]
    OutOfOrderTimestamp {
        row: usize,
        prev: DateTime<Utc>,
        curr: DateTime<Utc>,
    },

    #[error("data file contains no rows")]
    Empty,
}

#[derive(Debug, Deserialize)]
struct RawRecord {
    timestamp: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

/// Parses a timestamp string as RFC3339, a bare `YYYY-MM-DD` date (assumed
/// UTC midnight), or unix milliseconds — in that order.
fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|dt| Utc.from_utc_datetime(&dt));
    }
    if let Ok(millis) = raw.parse::<i64>() {
        return Utc.timestamp_millis_opt(millis).single();
    }
    None
}

/// A [`DataFeed`] backed by a fully-loaded, validated CSV file.
///
/// The whole file is read and validated up front (not streamed) so that
/// ordering validation, optional sorting, and gap detection can see the
/// full timestamp sequence. This is a deliberate simplicity-over-throughput
/// tradeoff for v1; see `PERFORMANCE.md` (added in the performance
/// milestone) for the streaming alternative if this ever becomes a
/// bottleneck.
pub struct CsvFeed {
    bars: VecDeque<MarketEvent>,
}

impl CsvFeed {
    /// Loads and validates a CSV file at `path` for `symbol`.
    pub fn load(
        path: impl AsRef<Path>,
        symbol: SymbolId,
        policy: TimestampPolicy,
    ) -> Result<Self, DataError> {
        let path_ref = path.as_ref();
        let file = File::open(path_ref).map_err(|source| DataError::Io {
            path: path_ref.display().to_string(),
            source,
        })?;
        Self::from_reader(file, symbol, policy)
    }

    /// Loads and validates CSV data from any [`Read`] source. Factored out
    /// from [`Self::load`] so tests can exercise validation logic against
    /// in-memory strings without touching disk.
    pub fn from_reader(
        reader: impl Read,
        symbol: SymbolId,
        policy: TimestampPolicy,
    ) -> Result<Self, DataError> {
        let mut csv_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(reader);

        let mut bars: Vec<Bar> = Vec::new();
        for (i, record) in csv_reader.deserialize::<RawRecord>().enumerate() {
            let row = i + 2; // 1-indexed, plus the header line
            let raw = record.map_err(|source| DataError::Csv { row, source })?;
            let timestamp =
                parse_timestamp(&raw.timestamp).ok_or_else(|| DataError::InvalidTimestamp {
                    row,
                    raw: raw.timestamp.clone(),
                })?;
            let bar = Bar {
                timestamp,
                open: raw.open,
                high: raw.high,
                low: raw.low,
                close: raw.close,
                volume: raw.volume,
            };
            if !bar.is_valid() {
                return Err(DataError::InvalidBar { row, bar });
            }
            bars.push(bar);
        }

        if bars.is_empty() {
            return Err(DataError::Empty);
        }

        match policy {
            TimestampPolicy::SortAscending => {
                bars.sort_by_key(|b| b.timestamp);
            }
            TimestampPolicy::Reject => {
                for (i, pair) in bars.windows(2).enumerate() {
                    let (prev, curr) = (pair[0], pair[1]);
                    if curr.timestamp < prev.timestamp {
                        return Err(DataError::OutOfOrderTimestamp {
                            row: i + 3, // header + 1-indexed + the row after `prev`
                            prev: prev.timestamp,
                            curr: curr.timestamp,
                        });
                    }
                }
            }
        }

        for warning in detect_gaps(&bars) {
            eprintln!("warning: {warning}");
        }

        let events = bars
            .into_iter()
            .map(|bar| MarketEvent {
                symbol: symbol.clone(),
                bar,
            })
            .collect();

        Ok(Self { bars: events })
    }
}

/// Returns one message per gap larger than 4x the smallest positive spacing
/// seen between consecutive bars (the finest cadence is taken as the
/// "declared frequency" — v1 has no explicit "daily"/"5min" config). The
/// 4x tolerance exists specifically so ordinary weekends (a 3x gap in daily
/// data) don't trip the detector; only gaps beyond a normal long weekend
/// are flagged. This is a best-effort heuristic, not a calendar-aware
/// check — it doesn't know about market holidays.
pub(crate) fn detect_gaps(bars: &[Bar]) -> Vec<String> {
    if bars.len() < 3 {
        return Vec::new();
    }
    let base = bars
        .windows(2)
        .map(|pair| (pair[1].timestamp - pair[0].timestamp).num_seconds())
        .filter(|&d| d > 0)
        .min();
    let Some(base) = base else {
        return Vec::new();
    };
    let threshold = base * 4;

    bars.windows(2)
        .enumerate()
        .filter_map(|(i, pair)| {
            let delta = (pair[1].timestamp - pair[0].timestamp).num_seconds();
            (delta > threshold).then(|| {
                format!(
                    "gap detected between row {} ({}) and row {} ({}): {}s, expected ~{}s",
                    i + 2,
                    pair[0].timestamp,
                    i + 3,
                    pair[1].timestamp,
                    delta,
                    base
                )
            })
        })
        .collect()
}

impl DataFeed for CsvFeed {
    fn next(&mut self) -> Option<MarketEvent> {
        self.bars.pop_front()
    }
}
