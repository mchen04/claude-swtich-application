#![allow(dead_code)] // active/previous fields used starting Phase C

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub active: Option<String>,
    pub previous: Option<String>,
    pub default: Option<String>,
    /// Milliseconds since epoch.
    pub switched_at_ms: Option<u64>,
    /// Milliseconds since epoch — earliest jsonl mtime to attribute to `active`.
    pub since_ms: Option<u64>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) if bytes.is_empty() => Ok(Self::default()),
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(Error::Json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io_at(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(&tmp, &bytes).map_err(|e| Error::io_at(&tmp, e))?;
        fs::rename(&tmp, path).map_err(|e| Error::io_at(path, e))?;
        Ok(())
    }
}
