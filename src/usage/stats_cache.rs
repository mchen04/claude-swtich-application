use std::path::Path;

use serde::Deserialize;

use crate::error::Result;
use crate::jsonio;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StatsCache {
    #[serde(default, rename = "dailyActivity")]
    pub daily_activity: Vec<DailyActivityEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DailyActivityEntry {
    pub date: String,
    #[serde(default, rename = "messageCount")]
    pub message_count: u64,
    #[serde(default, rename = "sessionCount")]
    pub session_count: u64,
    #[serde(default, rename = "toolCallCount")]
    pub tool_call_count: u64,
}

pub fn load(path: &Path) -> Result<StatsCache> {
    jsonio::load_or_default(path)
}
