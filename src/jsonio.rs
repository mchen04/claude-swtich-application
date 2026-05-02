use std::fs;
use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};

pub fn load_or_default<T: Default + DeserializeOwned>(path: &Path) -> Result<T> {
    match fs::read(path) {
        Ok(b) if b.is_empty() => Ok(T::default()),
        Ok(b) => serde_json::from_slice(&b).map_err(Error::Json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(Error::io_at(path, e)),
    }
}

pub fn save_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    atomic_write_bytes(path, &bytes)
}

/// Write `bytes` to `path` atomically via a sibling tempfile + `rename(2)`.
pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    let tmp = path.with_extension(format!(".tmp.{}.{}", std::process::id(), now_nanos()));
    fs::write(&tmp, bytes).map_err(|e| Error::io_at(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| Error::io_at(path, e))?;
    Ok(())
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
