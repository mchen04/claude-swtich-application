use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::Paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexProfileSummary {
    pub auth_mode: Option<String>,
    pub last_refresh: Option<String>,
    pub account_id: Option<String>,
    pub has_refresh_token: bool,
    pub has_api_key: bool,
}

#[derive(Debug, Deserialize)]
struct RawCodexAuth {
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default)]
    last_refresh: Option<String>,
    #[serde(default)]
    tokens: Option<RawCodexTokens>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawCodexTokens {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

pub fn load_codex_summary(path: &Path) -> Result<CodexProfileSummary> {
    let bytes = fs::read(path).map_err(|e| Error::io_at(path, e))?;
    let raw: RawCodexAuth = serde_json::from_slice(&bytes)?;
    let tokens = raw.tokens.unwrap_or_default();
    Ok(CodexProfileSummary {
        auth_mode: raw.auth_mode,
        last_refresh: raw.last_refresh,
        account_id: tokens.account_id,
        has_refresh_token: tokens
            .refresh_token
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        has_api_key: raw
            .openai_api_key
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
    })
}

pub fn codex_profile_path(paths: &Paths, name: &str) -> PathBuf {
    paths.profile_codex_auth(name)
}

pub fn read_codex_active_blob(paths: &Paths) -> Result<Vec<u8>> {
    let path = paths.codex_auth();
    fs::read(&path).map_err(|e| Error::io_at(path, e))
}

pub fn read_codex_profile_blob(paths: &Paths, name: &str) -> Result<Vec<u8>> {
    let path = codex_profile_path(paths, name);
    fs::read(&path).map_err(|e| Error::io_at(path, e))
}

pub fn write_codex_profile_blob(paths: &Paths, name: &str, bytes: &[u8]) -> Result<()> {
    let path = codex_profile_path(paths, name);
    write_blob_atomic(&path, bytes)
}

pub fn write_codex_active_blob(paths: &Paths, bytes: &[u8]) -> Result<()> {
    let path = paths.codex_auth();
    write_blob_atomic(&path, bytes)
}

fn write_blob_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::other(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    let tmp = parent.join(format!(".cs.tmp.{}.{}", std::process::id(), now_nanos()));
    fs::write(&tmp, bytes).map_err(|e| Error::io_at(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| Error::io_at(path, e))
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
