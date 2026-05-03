use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::jsonio;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub active: Option<String>,
    pub previous: Option<String>,
    pub default: Option<String>,
    /// Profile that owns the shared skills/commands/agents/CLAUDE.md tree.
    #[serde(default)]
    pub master: Option<String>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        jsonio::load_or_default(path)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        jsonio::save_atomic(path, self)
    }
}
