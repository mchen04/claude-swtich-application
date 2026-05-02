use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::jsonio;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub active: Option<String>,
    pub previous: Option<String>,
    /// Active profile for Claude provider.
    #[serde(default)]
    pub active_claude: Option<String>,
    /// Previous profile for Claude provider.
    #[serde(default)]
    pub previous_claude: Option<String>,
    pub default: Option<String>,
    /// Profile that owns the shared skills/commands/agents/CLAUDE.md tree.
    #[serde(default)]
    pub master: Option<String>,
    /// Milliseconds since epoch.
    pub switched_at_ms: Option<u64>,
    /// Milliseconds since epoch — earliest jsonl mtime to attribute to `active`.
    pub since_ms: Option<u64>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        jsonio::load_or_default(path)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        jsonio::save_atomic(path, self)
    }
}
