//! Per-profile rate-limit dashboard backed by `/api/oauth/usage`.
//!
//! Calls the same undocumented endpoint Claude Code's `/usage` slash command uses,
//! caches the response on disk for 300s to stay clear of the endpoint's aggressive
//! 429s, and surfaces the bucket utilizations the user cares about.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::profile::OauthCreds;

const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
/// Default max age for one-shot calls. 300s matches community guidance for the
/// 429-prone `/api/oauth/usage` endpoint.
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(300);
/// Tighter max age used by `cs usage --watch` so the % values actually move
/// while the user works. 2 calls/min/profile stays well clear of 429s.
pub const WATCH_MAX_AGE: Duration = Duration::from_secs(30);
const TOKEN_LEEWAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    /// Percent already used (0.0–100.0). The endpoint returns this as a float —
    /// `18.0`, `0.0`, etc. — so we keep the precision and round at render time.
    pub utilization: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLimits {
    pub five_hour: Bucket,
    pub seven_day: Bucket,
    #[serde(default)]
    pub seven_day_sonnet: Option<Bucket>,
    #[serde(default)]
    pub seven_day_opus: Option<Bucket>,
}

#[derive(Debug)]
pub struct LimitsOutcome {
    pub limits: UsageLimits,
    /// True when the live fetch failed with 429 and we served past-TTL cache.
    pub stale: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LimitsError {
    #[error("token expired — `cs refresh {0}`")]
    TokenExpired(String),
    #[error("rate-limited (try again in a few minutes)")]
    RateLimited,
    #[error("http: {0}")]
    Http(String),
    #[error("parse: {0}")]
    Parse(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    fetched_at_unix: u64,
    payload: UsageLimits,
}

pub fn fetch_for(
    profile: &str,
    creds: &OauthCreds,
    paths: &Paths,
    max_age: Duration,
) -> Result<LimitsOutcome, LimitsError> {
    if let Some(fail_mode) = test_fail_mode(profile) {
        return match fail_mode.as_str() {
            "expired" => Err(LimitsError::TokenExpired(profile.to_string())),
            "rate_limited" => match read_cache(paths, profile) {
                Some(cache) => Ok(LimitsOutcome {
                    limits: cache.payload,
                    stale: true,
                }),
                None => Err(LimitsError::RateLimited),
            },
            "http" => Err(LimitsError::Http("simulated http failure".into())),
            other => Err(LimitsError::Http(format!("unknown fail mode: {other}"))),
        };
    }

    if let Some(payload) = read_fixture(profile)? {
        return Ok(LimitsOutcome {
            limits: payload,
            stale: false,
        });
    }

    if let Some(cache) = read_cache(paths, profile) {
        if cache_is_fresh(&cache, max_age) {
            return Ok(LimitsOutcome {
                limits: cache.payload,
                stale: false,
            });
        }
    }

    if creds.is_expired(TOKEN_LEEWAY) {
        return Err(LimitsError::TokenExpired(profile.to_string()));
    }

    let token = &creds.oauth.access_token;
    match http_get_limits(token) {
        Ok(payload) => {
            write_cache(paths, profile, &payload);
            Ok(LimitsOutcome {
                limits: payload,
                stale: false,
            })
        }
        Err(LimitsError::RateLimited) => match read_cache(paths, profile) {
            Some(cache) => Ok(LimitsOutcome {
                limits: cache.payload,
                stale: true,
            }),
            None => Err(LimitsError::RateLimited),
        },
        Err(e) => Err(e),
    }
}

fn http_get_limits(token: &str) -> Result<UsageLimits, LimitsError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build();
    let response = agent
        .get(ENDPOINT)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", OAUTH_BETA)
        .set("Accept", "application/json")
        .call();

    match response {
        Ok(resp) => {
            let body: UsageLimits = resp.into_json().map_err(|e| LimitsError::Parse(e.to_string()))?;
            Ok(body)
        }
        Err(ureq::Error::Status(429, _)) => Err(LimitsError::RateLimited),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(LimitsError::Http("unauthorized".into()))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(LimitsError::Http(format!("status {code}: {body}")))
        }
        Err(ureq::Error::Transport(t)) => Err(LimitsError::Http(t.to_string())),
    }
}

fn test_fail_mode(profile: &str) -> Option<String> {
    let dir = std::env::var_os("CS_TEST_LIMITS_FAIL")?;
    let path = Path::new(&dir).join(format!("{profile}.txt"));
    let content = std::fs::read_to_string(&path).ok()?;
    Some(content.trim().to_string())
}

fn read_fixture(profile: &str) -> Result<Option<UsageLimits>, LimitsError> {
    let Some(dir) = std::env::var_os("CS_TEST_LIMITS_FIXTURE") else {
        return Ok(None);
    };
    let path = Path::new(&dir).join(format!("{profile}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| LimitsError::Parse(e.to_string()))?;
    let payload: UsageLimits =
        serde_json::from_slice(&bytes).map_err(|e| LimitsError::Parse(e.to_string()))?;
    Ok(Some(payload))
}

fn cache_path(paths: &Paths, profile: &str) -> PathBuf {
    paths.usage_limits_cache_dir().join(format!("{profile}.json"))
}

fn read_cache(paths: &Paths, profile: &str) -> Option<CacheFile> {
    let bytes = std::fs::read(cache_path(paths, profile)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn cache_is_fresh(cache: &CacheFile, max_age: Duration) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.saturating_sub(cache.fetched_at_unix) < max_age.as_secs()
}

fn write_cache(paths: &Paths, profile: &str, payload: &UsageLimits) {
    let dir = paths.usage_limits_cache_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("usage-limits cache mkdir failed at {}: {e}", dir.display());
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache = CacheFile {
        fetched_at_unix: now,
        payload: payload.clone(),
    };
    let path = cache_path(paths, profile);
    match serde_json::to_vec(&cache) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, bytes) {
                tracing::warn!("usage-limits cache write failed at {}: {e}", path.display());
            }
        }
        Err(e) => tracing::warn!("usage-limits cache encode failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoint_payload() {
        // Real responses return utilization as a float (e.g. 18.0, 0.0).
        let raw = br#"{
            "five_hour": { "utilization": 18.0, "resets_at": "2026-05-02T19:00:00Z" },
            "seven_day": { "utilization": 0.0,  "resets_at": "2026-05-06T12:00:00Z" },
            "seven_day_sonnet": null,
            "seven_day_opus": null,
            "extra_usage": { "is_enabled": false }
        }"#;
        let p: UsageLimits = serde_json::from_slice(raw).unwrap();
        assert!((p.five_hour.utilization - 18.0).abs() < 1e-9);
        assert!((p.seven_day.utilization - 0.0).abs() < 1e-9);
        assert!(p.seven_day_sonnet.is_none());
    }

    #[test]
    fn cache_freshness_window() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let fresh = CacheFile {
            fetched_at_unix: now,
            payload: UsageLimits {
                five_hour: Bucket {
                    utilization: 0.0,
                    resets_at: None,
                },
                seven_day: Bucket {
                    utilization: 0.0,
                    resets_at: None,
                },
                seven_day_sonnet: None,
                seven_day_opus: None,
            },
        };
        assert!(cache_is_fresh(&fresh, DEFAULT_MAX_AGE));

        let stale = CacheFile {
            fetched_at_unix: now.saturating_sub(DEFAULT_MAX_AGE.as_secs() + 5),
            payload: fresh.payload.clone(),
        };
        assert!(!cache_is_fresh(&stale, DEFAULT_MAX_AGE));

        // Same cache, tighter max_age (watch mode): a 60s-old entry is fresh
        // under 300s but stale under 30s — forcing a re-fetch.
        let aged = CacheFile {
            fetched_at_unix: now.saturating_sub(60),
            payload: fresh.payload.clone(),
        };
        assert!(cache_is_fresh(&aged, DEFAULT_MAX_AGE));
        assert!(!cache_is_fresh(&aged, WATCH_MAX_AGE));
    }
}
