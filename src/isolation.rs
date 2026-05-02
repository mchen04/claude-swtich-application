use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::paths::{Paths, SHARED_ITEMS};
use crate::profile::OauthCreds;
use crate::symlinks;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl LaunchSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>, env: Vec<(String, String)>) -> Self {
        Self {
            program: program.into(),
            args,
            env,
        }
    }
}

pub fn build_claude_launch(
    paths: &Paths,
    kc: &dyn Keychain,
    name: &str,
    args: Vec<String>,
) -> Result<LaunchSpec> {
    let env = env_for_claude(paths, kc, name)?;
    Ok(LaunchSpec::new("claude", args, env))
}

pub fn env_for_claude(
    paths: &Paths,
    kc: &dyn Keychain,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let home = materialize_claude_home(paths, kc, name)?;
    Ok(claude_env(&home))
}

pub fn preview_env_for_claude(
    paths: &Paths,
    kc: &dyn Keychain,
    name: &str,
) -> Result<Vec<(String, String)>> {
    if !has_claude(kc, name)? {
        return Err(Error::ProfileNotFound(name.to_string()));
    }
    let home = paths.profile_provider_home(name, "claude");
    Ok(claude_env(&home))
}

fn claude_env(home: &Path) -> Vec<(String, String)> {
    vec![
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            home.to_string_lossy().into_owned(),
        ),
        (
            "CLAUDE_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        ),
    ]
}

pub fn has_claude(kc: &dyn Keychain, name: &str) -> Result<bool> {
    let account = keychain::profile_account(name);
    Ok(match kc.read(&account) {
        Ok(bytes) => {
            OauthCreds::parse(&bytes)?;
            true
        }
        Err(_) => false,
    })
}

fn materialize_claude_home(paths: &Paths, kc: &dyn Keychain, name: &str) -> Result<PathBuf> {
    let account = keychain::profile_account(name);
    let bytes = kc
        .read(&account)
        .map_err(|_| Error::ProfileNotFound(name.to_string()))?;
    OauthCreds::parse(&bytes)?;

    let home = paths.profile_provider_home(name, "claude");
    fs::create_dir_all(&home).map_err(|e| Error::io_at(&home, e))?;

    let profile_settings = paths.profile_claude_settings(name);
    if profile_settings.exists() {
        copy_into_home(&profile_settings, &home.join("settings.json"))?;
    } else if paths.claude_settings().exists() {
        copy_into_home(&paths.claude_settings(), &home.join("settings.json"))?;
    }

    for item in SHARED_ITEMS {
        let source = paths.claude_home.join(item);
        if fs::symlink_metadata(&source).is_ok() {
            ensure_symlink(&source, &home.join(item))?;
        }
    }

    Ok(home)
}

fn copy_into_home(src: &Path, dst: &Path) -> Result<()> {
    let bytes = fs::read(src).map_err(|e| Error::io_at(src, e))?;
    crate::jsonio::atomic_write_bytes(dst, &bytes)
}

fn ensure_symlink(target: &Path, link: &Path) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(link).ok().as_deref() == Some(target) {
                return Ok(());
            }
            fs::remove_file(link).map_err(|e| Error::io_at(link, e))?;
        }
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(link).map_err(|e| Error::io_at(link, e))?;
        }
        Ok(_) => {
            fs::remove_file(link).map_err(|e| Error::io_at(link, e))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io_at(link, e)),
    }
    symlinks::replace(target, link)
}
