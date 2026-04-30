use std::path::Path;

use serde::Deserialize;

use crate::error::Result;

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
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(StatsCache::default()),
        Err(e) => return Err(crate::error::Error::io_at(path, e)),
    };
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}
