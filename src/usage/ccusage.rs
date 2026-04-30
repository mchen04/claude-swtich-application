use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::error::{Error, Result};

use super::{ActiveBlock, DailyTotal};

const BLOCKS_TTL: Duration = Duration::from_secs(5);
const DAILY_TTL: Duration = Duration::from_secs(30);

/// Subprocess-backed wrapper for `ccusage`. Prefers `bunx ccusage` (faster start); falls
/// back to `npx ccusage`. Single-flight via the per-cache mutex.
pub struct CcusageClient {
    blocks: Mutex<Cache<Vec<ActiveBlock>>>,
    daily: Mutex<Cache<Vec<DailyTotal>>>,
}

#[derive(Default)]
struct Cache<T: Clone> {
    value: Option<T>,
    fetched_at: Option<Instant>,
}

impl Default for CcusageClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CcusageClient {
    pub fn new() -> Self {
        Self {
            blocks: Mutex::new(Cache { value: None, fetched_at: None }),
            daily: Mutex::new(Cache { value: None, fetched_at: None }),
        }
    }

    pub fn active_blocks(&self) -> Result<Vec<ActiveBlock>> {
        {
            let cache = self.blocks.lock().unwrap();
            if let (Some(v), Some(t)) = (cache.value.as_ref(), cache.fetched_at) {
                if t.elapsed() < BLOCKS_TTL {
                    return Ok(v.clone());
                }
            }
        }
        let raw = run_ccusage(&["blocks", "--json", "--active"])?;
        let parsed = parse_blocks(&raw)?;
        let mut cache = self.blocks.lock().unwrap();
        cache.value = Some(parsed.clone());
        cache.fetched_at = Some(Instant::now());
        Ok(parsed)
    }

    pub fn daily(&self) -> Result<Vec<DailyTotal>> {
        {
            let cache = self.daily.lock().unwrap();
            if let (Some(v), Some(t)) = (cache.value.as_ref(), cache.fetched_at) {
                if t.elapsed() < DAILY_TTL {
                    return Ok(v.clone());
                }
            }
        }
        let raw = run_ccusage(&["daily", "--json"])?;
        let parsed = parse_daily(&raw)?;
        let mut cache = self.daily.lock().unwrap();
        cache.value = Some(parsed.clone());
        cache.fetched_at = Some(Instant::now());
        Ok(parsed)
    }
}

fn run_ccusage(extra_args: &[&str]) -> Result<Vec<u8>> {
    if std::env::var_os("CS_TEST_DISABLE_CCUSAGE").is_some() {
        return Err(Error::Subprocess {
            cmd: "ccusage".into(),
            message: "disabled in test".into(),
        });
    }
    if let Some(fixture) = std::env::var_os("CS_TEST_CCUSAGE_FIXTURE") {
        // Test injection: read the JSON shape from disk based on the first arg
        // (`blocks` or `daily`).
        let mode = extra_args.first().copied().unwrap_or("");
        let path = std::path::Path::new(&fixture).join(format!("{mode}.json"));
        return std::fs::read(&path).map_err(|e| Error::io_at(&path, e));
    }
    let runners: &[(&str, &[&str])] = &[("bunx", &["ccusage"]), ("npx", &["--yes", "ccusage"])];
    let mut last_err: Option<String> = None;
    for (cmd, prefix) in runners {
        let mut args: Vec<&str> = prefix.to_vec();
        args.extend_from_slice(extra_args);
        let out = Command::new(cmd).args(&args).output();
        match out {
            Ok(o) if o.status.success() => return Ok(o.stdout),
            Ok(o) => {
                last_err = Some(format!(
                    "{cmd} ccusage exited {}: {}",
                    o.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&o.stderr)
                ));
            }
            Err(e) => last_err = Some(format!("{cmd}: {e}")),
        }
    }
    Err(Error::Subprocess {
        cmd: "ccusage".into(),
        message: last_err.unwrap_or_else(|| "no runner available".into()),
    })
}

#[derive(Debug, Deserialize)]
struct RawBlocksEnvelope {
    #[serde(default)]
    blocks: Vec<RawBlock>,
}

#[derive(Debug, Deserialize)]
struct RawBlock {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "startTime")]
    start_time: Option<String>,
    #[serde(default, rename = "endTime")]
    end_time: Option<String>,
    #[serde(default, rename = "isActive")]
    is_active: Option<bool>,
    #[serde(default, rename = "tokenCounts")]
    token_counts: Option<RawTokenCounts>,
    #[serde(default, rename = "totalTokens")]
    total_tokens: Option<u64>,
    #[serde(default, rename = "costUSD")]
    cost_usd: Option<f64>,
    /// ccusage 18.x: object with `tokensPerMinute`, `costPerHour`, etc.
    #[serde(default, rename = "burnRate")]
    burn_rate: Option<RawBurnRate>,
    /// ccusage 18.x: object with `totalTokens`, `totalCost`, `remainingMinutes`.
    #[serde(default)]
    projection: Option<RawProjection>,
    #[serde(default, rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // structural fields kept for forward-compat / future TUI panes
struct RawBurnRate {
    #[serde(default, rename = "tokensPerMinute")]
    tokens_per_minute: f64,
    #[serde(default, rename = "costPerHour")]
    cost_per_hour: f64,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct RawProjection {
    #[serde(default, rename = "totalTokens")]
    total_tokens: u64,
    #[serde(default, rename = "totalCost")]
    total_cost: f64,
    #[serde(default, rename = "remainingMinutes")]
    remaining_minutes: u64,
}

#[derive(Debug, Deserialize, Default)]
struct RawTokenCounts {
    #[serde(default, rename = "inputTokens")]
    input_tokens: u64,
    #[serde(default, rename = "outputTokens")]
    output_tokens: u64,
    #[serde(default, rename = "cacheCreationInputTokens")]
    cache_creation_input_tokens: u64,
    #[serde(default, rename = "cacheReadInputTokens")]
    cache_read_input_tokens: u64,
}

fn parse_blocks(bytes: &[u8]) -> Result<Vec<ActiveBlock>> {
    let env: RawBlocksEnvelope = serde_json::from_slice(bytes)?;
    Ok(env
        .blocks
        .into_iter()
        .filter(|b| b.is_active.unwrap_or(true))
        .map(|b| {
            let counts = b.token_counts.unwrap_or_default();
            ActiveBlock {
                block_id: b.id,
                start: b.start_time,
                end: b.end_time,
                tokens_in: counts.input_tokens,
                tokens_out: counts.output_tokens,
                cache_creation_tokens: counts.cache_creation_input_tokens,
                cache_read_tokens: counts.cache_read_input_tokens,
                cost_usd: b.cost_usd.unwrap_or(0.0),
                burn_rate_per_min: b.burn_rate.as_ref().map(|r| r.tokens_per_minute),
                projection_pct: b.projection.as_ref().and_then(|p| {
                    let total = (counts.input_tokens
                        + counts.output_tokens
                        + counts.cache_creation_input_tokens
                        + counts.cache_read_input_tokens) as f64;
                    if p.total_tokens > 0 {
                        Some(total / p.total_tokens as f64)
                    } else {
                        None
                    }
                }),
                resets_at: b.reset_time,
            }
            .with_total(b.total_tokens)
        })
        .collect())
}

impl ActiveBlock {
    fn with_total(self, _: Option<u64>) -> Self {
        // Total tokens is derivable from the components; ccusage's `totalTokens` is
        // sometimes absent. Keep the explicit field for forward-compatibility.
        self
    }
}

#[derive(Debug, Deserialize)]
struct RawDailyEnvelope {
    #[serde(default)]
    daily: Vec<RawDailyEntry>,
}

#[derive(Debug, Deserialize)]
struct RawDailyEntry {
    #[serde(default)]
    date: Option<String>,
    #[serde(default, rename = "inputTokens")]
    input_tokens: u64,
    #[serde(default, rename = "outputTokens")]
    output_tokens: u64,
    #[serde(default, rename = "cacheCreationTokens")]
    cache_creation_tokens: u64,
    #[serde(default, rename = "cacheReadTokens")]
    cache_read_tokens: u64,
    #[serde(default, rename = "totalTokens")]
    total_tokens: u64,
    #[serde(default, rename = "totalCost")]
    total_cost: f64,
    #[serde(default, rename = "modelsUsed")]
    models_used: Vec<String>,
}

fn parse_daily(bytes: &[u8]) -> Result<Vec<DailyTotal>> {
    let env: RawDailyEnvelope = serde_json::from_slice(bytes)?;
    Ok(env
        .daily
        .into_iter()
        .map(|d| DailyTotal {
            date: d.date.unwrap_or_default(),
            tokens_in: d.input_tokens,
            tokens_out: d.output_tokens,
            cache_creation_tokens: d.cache_creation_tokens,
            cache_read_tokens: d.cache_read_tokens,
            total_tokens: d.total_tokens,
            cost_usd: d.total_cost,
            models: d.models_used,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BLOCKS: &[u8] = br#"{
        "blocks": [{
            "id": "block-1",
            "startTime": "2026-04-30T05:00:00Z",
            "endTime": "2026-04-30T10:00:00Z",
            "isActive": true,
            "tokenCounts": {
                "inputTokens": 1000,
                "outputTokens": 200,
                "cacheCreationInputTokens": 50,
                "cacheReadInputTokens": 800
            },
            "totalTokens": 2050,
            "costUSD": 0.123,
            "burnRate": { "tokensPerMinute": 12.0, "costPerHour": 1.5 },
            "projection": { "totalTokens": 4100, "totalCost": 0.5, "remainingMinutes": 200 },
            "resetTime": "2026-04-30T10:00:00Z"
        }]
    }"#;

    const SAMPLE_DAILY: &[u8] = br#"{
        "daily": [
            {
                "date": "2026-04-30",
                "inputTokens": 1000,
                "outputTokens": 200,
                "cacheCreationTokens": 50,
                "cacheReadTokens": 800,
                "totalTokens": 2050,
                "totalCost": 0.123,
                "modelsUsed": ["claude-opus-4-7"]
            }
        ]
    }"#;

    #[test]
    fn parse_blocks_extracts_active() {
        let v = parse_blocks(SAMPLE_BLOCKS).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].tokens_in, 1000);
        assert_eq!(v[0].cache_read_tokens, 800);
        assert!((v[0].cost_usd - 0.123).abs() < 1e-9);
        assert!((v[0].burn_rate_per_min.unwrap() - 12.0).abs() < 1e-9);
        // projection_pct = sum_tokens(2050) / projection.total_tokens(4100) = 0.5
        assert!((v[0].projection_pct.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_daily_extracts_totals() {
        let v = parse_daily(SAMPLE_DAILY).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].total_tokens, 2050);
        assert_eq!(v[0].models, vec!["claude-opus-4-7".to_string()]);
    }

    #[test]
    fn parse_blocks_empty_envelope() {
        let v = parse_blocks(b"{}").unwrap();
        assert!(v.is_empty());
    }
}
