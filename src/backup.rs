use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupAction {
    /// Replace a keychain blob. `before` is base64; absent if the entry didn't exist.
    KeychainReplace {
        account: String,
        before_b64: Option<String>,
        after_b64: Option<String>,
    },
    /// Replace the state file.
    StateReplace {
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    },
    /// Replace ~/.claude/settings.json. before/after store the file bytes inline (small).
    SettingsReplace {
        before_b64: Option<String>,
        after_b64: Option<String>,
    },
    /// Filesystem move (used by master init).
    FsMove { from: PathBuf, to: PathBuf },
    /// Symlink creation.
    SymlinkCreate { link: PathBuf, target: PathBuf },
    /// Symlink removal.
    SymlinkRemove { link: PathBuf, target: PathBuf },
    /// Free-form note.
    Note { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub timestamp: String,
    pub op: String,
    pub actor: String,
    pub actions: Vec<BackupAction>,
    /// Profile that was master at the time the manifest was written.
    #[serde(default)]
    pub master_profile: Option<String>,
}

impl Manifest {
    pub fn new(op: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            op: op.into(),
            actor: format!("cs {}", env!("CARGO_PKG_VERSION")),
            actions: Vec::new(),
            master_profile: None,
        }
    }

    pub fn with_master(mut self, name: impl Into<String>) -> Self {
        self.master_profile = Some(name.into());
        self
    }

    pub fn push(&mut self, a: BackupAction) {
        self.actions.push(a);
    }

    pub fn write(&self, paths: &Paths) -> Result<PathBuf> {
        let dir = paths.backups_dir().join(slug_for_now());
        fs::create_dir_all(&dir).map_err(|e| Error::io_at(&dir, e))?;
        let path = dir.join("manifest.json");
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(&path, &bytes).map_err(|e| Error::io_at(&path, e))?;
        Ok(path)
    }
}

fn slug_for_now() -> String {
    // 2026-04-30T07-18-32Z — filesystem-safe rfc3339 substitute
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let dt = chrono::DateTime::<chrono::Utc>::from(SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(ts as u64));
    dt.format("%Y-%m-%dT%H-%M-%S%.3fZ").to_string()
}

pub fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::b64;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(b64(b""), "");
        assert_eq!(b64(b"f"), "Zg==");
        assert_eq!(b64(b"fo"), "Zm8=");
        assert_eq!(b64(b"foo"), "Zm9v");
        assert_eq!(b64(b"foob"), "Zm9vYg==");
        assert_eq!(b64(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64(b"foobar"), "Zm9vYmFy");
    }
}
