//! Data models for usage tracking via the ccusage subprocess.

use serde::{Deserialize, Serialize};

pub mod ccusage;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActiveBlock {
    pub block_id: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    pub burn_rate_per_min: Option<f64>,
    pub projection_pct: Option<f64>,
    pub remaining_minutes: Option<u64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyTotal {
    pub date: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub models: Vec<String>,
}
