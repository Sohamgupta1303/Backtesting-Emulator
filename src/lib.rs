//! chronos-bt: an event-driven backtesting engine.
//!
//! Determinism is a feature, not an afterthought: this simulation is
//! single-threaded and uses no async runtime, so the same input and RNG
//! seed always produce byte-identical output. The engine is a single-writer
//! discrete event loop, and the core anti-lookahead guarantee — a strategy
//! can only observe data already emitted by the event loop — is enforced
//! by the module boundaries in [`strategy::StrategyContext`], not by
//! convention.
#![deny(clippy::unwrap_used)]

pub mod config;
pub mod data;
pub mod engine;
pub mod events;
pub mod execution;
pub mod metrics;
pub mod portfolio;
pub mod strategy;
