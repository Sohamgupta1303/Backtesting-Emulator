//! A fast, non-cryptographic hasher for `HashMap<SymbolId, _>`.
//!
//! Profiling the event loop (see `PERFORMANCE.md`) showed ~29% of total
//! samples going to SipHash — Rust's *default* `HashMap` hasher, chosen
//! for its resistance to hash-flooding attacks on untrusted input. That
//! resistance is irrelevant here: symbol keys come from the user's own
//! data file, not an adversary, and every bar does several lookups keyed
//! by `SymbolId` (position, last price, ring buffer). This is a small
//! reimplementation of the FxHash algorithm (as used by rustc and
//! Firefox) — simple enough to write inline rather than add a dependency
//! for it.

use std::hash::{BuildHasherDefault, Hasher};

use super::types::SymbolId;

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            let word = u64::from_ne_bytes(buf);
            self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
        }
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type FxBuildHasher = BuildHasherDefault<FxHasher>;

/// A `HashMap` keyed by [`SymbolId`], using [`FxBuildHasher`] instead of
/// the default SipHash — see the module doc for why that's safe here.
pub type SymbolMap<V> = std::collections::HashMap<SymbolId, V, FxBuildHasher>;
