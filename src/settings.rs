use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::jsonio;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub auto_switch: bool,
    #[serde(default)]
    pub last_switch_unix: Option<u64>,
    #[serde(default)]
    pub last_capped_notify_unix: Option<u64>,
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        jsonio::load_or_default(path)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        jsonio::save_atomic(path, self)
    }
}
