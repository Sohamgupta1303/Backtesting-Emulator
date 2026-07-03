//! The run configuration: everything a backtest run needs beyond the
//! data file and strategy choice, loadable from TOML and overridable via
//! `--set key=value` on the CLI. Embedded verbatim into `results.json` so
//! a run is fully reproducible from that file alone.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::execution::models::{CommissionModel, LimitFillPolicy, SlippageModel};

fn default_initial_capital() -> f64 {
    100_000.0
}

fn default_fraction() -> f64 {
    0.10
}

/// The fully-resolved configuration for one run: defaults, overridden by
/// whatever was loaded from a TOML file, overridden again by any
/// `--set key=value` CLI flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    #[serde(default)]
    pub sizing: SizingConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    /// Strategy parameters, interpreted according to whichever strategy
    /// name was selected on the CLI (`--strategy`). Kept as a loose
    /// key/value map rather than a fixed struct so any of the three
    /// reference strategies can share one config shape.
    #[serde(default)]
    pub strategy: HashMap<String, toml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_capital: default_initial_capital(),
            sizing: SizingConfig::default(),
            execution: ExecutionConfig::default(),
            strategy: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizingConfig {
    #[serde(default = "default_fraction")]
    pub fraction: f64,
}

impl Default for SizingConfig {
    fn default() -> Self {
        Self {
            fraction: default_fraction(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub slippage: SlippageConfig,
    #[serde(default)]
    pub commission: CommissionConfig,
    #[serde(default)]
    pub limit_fill_policy: LimitFillPolicyConfig,
    #[serde(default)]
    pub latency_bars: u32,
    #[serde(default)]
    pub time_in_force_bars: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum SlippageConfig {
    #[default]
    None,
    FixedBps {
        bps: f64,
    },
    VolumeImpact {
        participation_limit: f64,
        impact_coefficient: f64,
    },
}

impl SlippageConfig {
    pub fn to_model(&self) -> SlippageModel {
        match self {
            SlippageConfig::None => SlippageModel::None,
            SlippageConfig::FixedBps { bps } => SlippageModel::FixedBps(*bps),
            SlippageConfig::VolumeImpact {
                participation_limit,
                impact_coefficient,
            } => SlippageModel::VolumeImpact {
                participation_limit: *participation_limit,
                impact_coefficient: *impact_coefficient,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum CommissionConfig {
    PerShare { rate: f64 },
    PerTradeFlat { fee: f64 },
    BpsOfNotional { bps: f64 },
}

impl Default for CommissionConfig {
    fn default() -> Self {
        CommissionConfig::PerTradeFlat { fee: 0.0 }
    }
}

impl CommissionConfig {
    pub fn to_model(&self) -> CommissionModel {
        match self {
            CommissionConfig::PerShare { rate } => CommissionModel::PerShare(*rate),
            CommissionConfig::PerTradeFlat { fee } => CommissionModel::PerTradeFlat(*fee),
            CommissionConfig::BpsOfNotional { bps } => CommissionModel::BpsOfNotional(*bps),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LimitFillPolicyConfig {
    #[default]
    Optimistic,
    Conservative,
}

impl LimitFillPolicyConfig {
    pub fn to_model(&self) -> LimitFillPolicy {
        match self {
            LimitFillPolicyConfig::Optimistic => LimitFillPolicy::Optimistic,
            LimitFillPolicyConfig::Conservative => LimitFillPolicy::Conservative,
        }
    }
}
