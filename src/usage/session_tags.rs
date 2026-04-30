use std::collections::HashMap;
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

#[allow(dead_code)] // jsonl tailer (Phase F follow-up) writes via this
pub fn append(path: &Path, tag: &SessionTag) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    let mut buf = serde_json::to_vec(tag)?;
    buf.push(b'\n');
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| Error::io_at(path, e))?;
    f.write_all(&buf).map_err(|e| Error::io_at(path, e))?;
    Ok(())
}

#[allow(dead_code)] // attribution helper for the jsonl tailer
pub fn lookup(tags: &[SessionTag]) -> HashMap<String, String> {
    tags.iter().map(|t| (t.session_id.clone(), t.profile.clone())).collect()
}
