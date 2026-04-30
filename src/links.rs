use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::Paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinksFile {
    /// Map of canonical cwd path → profile name.
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
}

pub fn path(paths: &Paths) -> std::path::PathBuf {
    paths.cs_home.join("links.json")
}

pub fn load(paths: &Paths) -> Result<LinksFile> {
    match fs::read(path(paths)) {
        Ok(b) if b.is_empty() => Ok(LinksFile::default()),
        Ok(b) => serde_json::from_slice(&b).map_err(Error::Json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LinksFile::default()),
        Err(e) => Err(Error::io_at(path(paths), e)),
    }
}

pub fn save(paths: &Paths, file: &LinksFile) -> Result<()> {
    let p = path(paths);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    let bytes = serde_json::to_vec_pretty(file)?;
    fs::write(&p, bytes).map_err(|e| Error::io_at(&p, e))?;
    Ok(())
}

pub fn canonical_cwd() -> Result<String> {
    let cwd = std::env::current_dir().map_err(Error::IoBare)?;
    let canon = fs::canonicalize(&cwd).unwrap_or(cwd);
    Ok(canon.to_string_lossy().into_owned())
}

#[allow(dead_code)] // wired up by Phase G's cwd auto-switch precmd hook
pub fn lookup(file: &LinksFile, cwd: &Path) -> Option<String> {
    let cwd_str = cwd.to_string_lossy();
    file.bindings
        .iter()
        .filter(|(k, _)| cwd_str.starts_with(k.as_str()))
        .max_by_key(|(k, _)| k.len())
        .map(|(_, v)| v.clone())
}
