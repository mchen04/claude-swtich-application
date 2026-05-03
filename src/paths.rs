use std::env;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Paths shared across profiles via the master profile's symlinks.
pub const SHARED_ITEMS: &[&str] = &["skills", "commands", "agents", "CLAUDE.md"];

/// Central registry of all filesystem paths used by `cs`. All paths are derived from
/// environment variables (`CLAUDE_HOME`, `CS_HOME`) with sensible defaults so tests
/// can redirect every filesystem touch into a temp directory.
#[derive(Debug, Clone)]
pub struct Paths {
    pub claude_home: PathBuf,
    pub cs_home: PathBuf,
}

impl Paths {
    /// Resolve all paths from environment variables or platform defaults.
    pub fn from_env() -> Result<Self> {
        let home = match env::var_os("HOME") {
            Some(v) => PathBuf::from(v),
            None => directories::BaseDirs::new()
                .map(|b| b.home_dir().to_path_buf())
                .ok_or_else(|| Error::Config("could not determine HOME".into()))?,
        };

        let claude_home = env::var_os("CLAUDE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));

        let cs_home = env::var_os("CS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude-cs"));

        Ok(Self {
            claude_home,
            cs_home,
        })
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.cs_home.join("profiles")
    }

    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir().join(name)
    }

    pub fn profile_claude_settings(&self, name: &str) -> PathBuf {
        self.profile_dir(name).join("settings.json")
    }

    pub fn state_file(&self) -> PathBuf {
        self.cs_home.join("state.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.cs_home.join(".lock")
    }

    pub fn active_profile_marker(&self) -> PathBuf {
        self.claude_home.join(".active-profile")
    }

    pub fn claude_settings(&self) -> PathBuf {
        self.claude_home.join("settings.json")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.claude_home.join("projects")
    }

    pub fn usage_limits_cache_dir(&self) -> PathBuf {
        self.cs_home.join("cache").join("usage-limits")
    }

    pub fn ensure_cs_home(&self) -> Result<()> {
        std::fs::create_dir_all(&self.cs_home).map_err(|e| Error::io_at(&self.cs_home, e))?;
        Ok(())
    }
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidArgument("profile name is empty".into()));
    }
    if name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name == ".."
        || name.contains('\0')
    {
        return Err(Error::InvalidArgument(format!(
            "invalid profile name `{name}`: must not contain `/`, `\\`, `\\0`, or start with `.`"
        )));
    }
    Ok(())
}
