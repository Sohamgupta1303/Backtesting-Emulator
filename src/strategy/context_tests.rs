//! `unwrap` is idiomatic in test assertions.
#![allow(clippy::unwrap_used)]

use chrono::{TimeZone, Utc};

use super::RingBuffer;
use crate::data::types::Bar;

fn bar(day: u32, close: f64) -> Bar {
    Bar {
        timestamp: Utc.with_ymd_and_hms(2021, 1, day, 0, 0, 0).unwrap(),
        open: close,
        high: close,
        low: close,
        close,
        volume: 1_000.0,
    }
}

#[test]
fn holds_up_to_capacity_bars() {
    let mut buffer = RingBuffer::new(3);
    buffer.push(bar(1, 100.0));
    buffer.push(bar(2, 101.0));
    assert_eq!(buffer.len(), 2);
    assert_eq!(
        buffer.as_vec().iter().map(|b| b.close).collect::<Vec<_>>(),
        vec![100.0, 101.0]
    );
}

#[test]
fn evicts_oldest_once_full() {
    let mut buffer = RingBuffer::new(3);
    for day in 1..=5 {
        buffer.push(bar(day, 100.0 + day as f64));
    }
    // Capacity 3: only the last 3 pushes (days 3, 4, 5) should remain.
    assert_eq!(buffer.len(), 3);
    assert_eq!(
        buffer.as_vec().iter().map(|b| b.close).collect::<Vec<_>>(),
        vec![103.0, 104.0, 105.0]
    );
}

#[test]
fn capacity_of_zero_is_treated_as_one() {
    let mut buffer = RingBuffer::new(0);
    buffer.push(bar(1, 100.0));
    buffer.push(bar(2, 101.0));
    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer.as_vec()[0].close, 101.0);
}
