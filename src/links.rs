use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::jsonio;
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
    jsonio::load_or_default(&path(paths))
}

pub fn save(paths: &Paths, file: &LinksFile) -> Result<()> {
    jsonio::save_atomic(&path(paths), file)
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
