use serde::{Deserialize, Serialize};

pub mod ccusage;
pub mod session_tags;
pub mod stats_cache;

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

#[derive(Debug, Clone, Serialize, Default)]
pub struct DailyByProfile {
    pub profile: String,
    pub date: String,
    pub messages: u64,
    pub sessions: u64,
    pub tool_calls: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SessionLive {
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_hit_pct: Option<f64>,
    pub est_cost_usd: f64,
    pub source: SessionLiveSource,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionLiveSource {
    /// Reserved for the jsonl tailer (Phase F follow-up).
    #[allow(dead_code)]
    Jsonl,
    #[default]
    CcusageOnly,
}
