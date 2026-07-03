//! `unwrap` is idiomatic in test assertions (a panic on `Err`/`None` is the
//! desired failure mode), so the crate-wide `unwrap_used` lint is relaxed
//! here.
#![allow(clippy::unwrap_used)]

use super::csv_feed::{detect_gaps, CsvFeed, DataError, TimestampPolicy};
use super::types::{Bar, SymbolId};
use super::DataFeed;
use chrono::{TimeZone, Utc};

fn bar_at(day: u32) -> Bar {
    Bar {
        timestamp: Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap(),
        open: 100.0,
        high: 101.0,
        low: 99.0,
        close: 100.5,
        volume: 1000.0,
    }
}

fn symbol() -> SymbolId {
    SymbolId::new("TEST")
}

#[test]
fn loads_valid_csv() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01,100,101,99,100.5,1000\n\
               2021-01-02,100.5,102,100,101.5,1200\n";
    let mut feed = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject).unwrap();

    let first = feed.next().unwrap();
    assert_eq!(first.bar.open, 100.0);
    assert_eq!(first.bar.close, 100.5);
    let second = feed.next().unwrap();
    assert_eq!(second.bar.close, 101.5);
    assert!(feed.next().is_none());
}

#[test]
fn auto_detects_rfc3339_and_unix_millis() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01T00:00:00Z,100,101,99,100.5,1000\n\
               1609545600000,100.5,102,100,101.5,1200\n"; // 2021-01-02T00:00:00Z
    let mut feed = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject).unwrap();
    let first = feed.next().unwrap();
    assert_eq!(
        first.bar.timestamp.to_rfc3339(),
        "2021-01-01T00:00:00+00:00"
    );
    let second = feed.next().unwrap();
    assert_eq!(
        second.bar.timestamp.to_rfc3339(),
        "2021-01-02T00:00:00+00:00"
    );
}

#[test]
fn rejects_out_of_order_timestamps_by_default() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-02,100,101,99,100.5,1000\n\
               2021-01-01,100.5,102,100,101.5,1200\n";
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::OutOfOrderTimestamp { .. })));
}

#[test]
fn sorts_out_of_order_timestamps_when_configured() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-02,100,101,99,100.5,1000\n\
               2021-01-01,100.5,102,100,101.5,1200\n";
    let mut feed =
        CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::SortAscending).unwrap();
    let first = feed.next().unwrap();
    assert_eq!(
        first.bar.timestamp.to_rfc3339(),
        "2021-01-01T00:00:00+00:00"
    );
    let second = feed.next().unwrap();
    assert_eq!(
        second.bar.timestamp.to_rfc3339(),
        "2021-01-02T00:00:00+00:00"
    );
}

#[test]
fn rejects_negative_price() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01,-100,101,99,100.5,1000\n";
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::InvalidBar { .. })));
}

#[test]
fn rejects_nan_price() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01,NaN,101,99,100.5,1000\n";
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::InvalidBar { .. })));
}

#[test]
fn rejects_high_below_low() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01,100,98,99,100.5,1000\n"; // high < low
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::InvalidBar { .. })));
}

#[test]
fn rejects_high_below_open_or_close() {
    let csv = "timestamp,open,high,low,close,volume\n\
               2021-01-01,100,100,99,105,1000\n"; // close > high
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::InvalidBar { .. })));
}

#[test]
fn rejects_empty_file() {
    let csv = "timestamp,open,high,low,close,volume\n";
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::Empty)));
}

#[test]
fn rejects_unparsable_timestamp() {
    let csv = "timestamp,open,high,low,close,volume\n\
               not-a-date,100,101,99,100.5,1000\n";
    let result = CsvFeed::from_reader(csv.as_bytes(), symbol(), TimestampPolicy::Reject);
    assert!(matches!(result, Err(DataError::InvalidTimestamp { .. })));
}

#[test]
fn ordinary_weekend_gap_is_not_flagged() {
    // Fri Jan 1 -> Mon Jan 4: a normal 3x weekend gap against a 1-day
    // baseline cadence. Must not be reported.
    let bars = [bar_at(1), bar_at(4), bar_at(5), bar_at(6)];
    assert!(detect_gaps(&bars).is_empty());
}

#[test]
fn a_missing_week_is_flagged() {
    // Jan 4 -> Jan 11: a full week missing relative to the 1-day baseline.
    let bars = [bar_at(4), bar_at(5), bar_at(6), bar_at(11)];
    let warnings = detect_gaps(&bars);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("gap detected"));
}
