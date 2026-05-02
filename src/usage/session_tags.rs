use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTag {
    pub session_id: String,
    pub profile: String,
    pub tagged_at_ms: u64,
}

pub fn load(path: &Path) -> Result<Vec<SessionTag>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::io_at(path, e)),
    };
    let mut out = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(tag) = serde_json::from_slice::<SessionTag>(line) {
            out.push(tag);
        }
    }
    Ok(out)
}


