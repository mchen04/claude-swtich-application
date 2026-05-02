use std::env;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Paths shared across profiles via the master profile's symlinks.
pub const SHARED_ITEMS: &[&str] = &["skills", "commands", "agents", "CLAUDE.md"];

/// Central registry of all filesystem paths used by `cs`. All paths are derived from
/// environment variables (`CLAUDE_HOME`, `CS_HOME`, `CODEX_HOME`) with sensible defaults
/// so tests can redirect every filesystem touch into a temp directory.
#[derive(Debug, Clone)]
pub struct Paths {
    pub claude_home: PathBuf,
    pub codex_home: PathBuf,
    pub cs_home: PathBuf,
    pub config_file: PathBuf,
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
        let codex_home = env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));

        let config_file = env::var_os("CS_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config").join("claude-switch").join("config"));

        Ok(Self {
            claude_home,
            codex_home,
            cs_home,
            config_file,
        })
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.cs_home.join("profiles")
    }

    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir().join(name)
    }

    pub fn profile_provider_dir(&self, name: &str, provider: &str) -> PathBuf {
        self.profile_dir(name).join("providers").join(provider)
    }

    pub fn profile_claude_settings(&self, name: &str) -> PathBuf {
        self.profile_dir(name).join("settings.json")
    }

    pub fn profile_codex_auth(&self, name: &str) -> PathBuf {
        self.profile_provider_home(name, "codex").join("auth.json")
    }

    pub fn profile_codex_config(&self, name: &str) -> PathBuf {
        self.profile_provider_home(name, "codex")
            .join("config.toml")
    }

    pub fn profile_provider_home(&self, name: &str, provider: &str) -> PathBuf {
        self.profile_provider_dir(name, provider).join("home")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.cs_home.join(".backups")
    }

    pub fn state_file(&self) -> PathBuf {
        self.cs_home.join("state.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.cs_home.join(".lock")
    }

    pub fn last_env_file(&self) -> PathBuf {
        self.cs_home.join(".last-env")
    }

    pub fn session_tags_file(&self) -> PathBuf {
        self.cs_home.join("session-tags.jsonl")
    }

    pub fn active_profile_marker(&self) -> PathBuf {
        self.claude_home.join(".active-profile")
    }

    pub fn claude_settings(&self) -> PathBuf {
        self.claude_home.join("settings.json")
    }

    pub fn stats_cache(&self) -> PathBuf {
        self.claude_home.join("stats-cache.json")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.claude_home.join("projects")
    }

    pub fn codex_auth(&self) -> PathBuf {
        self.codex_home.join("auth.json")
    }

    pub fn codex_config(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }

    pub fn codex_skills_dir(&self) -> PathBuf {
        self.codex_home.join("skills")
    }

    pub fn ensure_cs_home(&self) -> Result<()> {
        std::fs::create_dir_all(&self.cs_home).map_err(|e| Error::io_at(&self.cs_home, e))?;
        Ok(())
    }

}
