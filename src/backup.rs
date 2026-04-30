use std::fs;
use std::path::{Path, PathBuf};
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
}

impl Manifest {
    pub fn new(op: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            op: op.into(),
            actor: format!("cs {}", env!("CARGO_PKG_VERSION")),
            actions: Vec::new(),
        }
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
    base64_encode(bytes)
}

#[allow(dead_code)]
pub fn b64_opt(bytes: Option<&[u8]>) -> Option<String> {
    bytes.map(base64_encode)
}

/// Tiny RFC 4648 base64 encoder so we don't pull in the full `base64` crate just for
/// manifest blobs. Output uses standard alphabet with `=` padding.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let b2 = input[i + 2] as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let remaining = input.len() - i;
    if remaining == 1 {
        let b0 = input[i] as u32;
        let n = b0 << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if remaining == 2 {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let n = (b0 << 16) | (b1 << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

#[allow(dead_code)]
pub fn read_settings(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
