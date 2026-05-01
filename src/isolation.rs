use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::paths::{Paths, SHARED_ITEMS};
use crate::profile::OauthCreds;
use crate::provider::{self, Provider};
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

pub fn build_provider_launch(
    paths: &Paths,
    kc: &dyn Keychain,
    provider: Provider,
    name: &str,
    args: Vec<String>,
) -> Result<LaunchSpec> {
    let env = env_for_provider(paths, kc, provider, name)?;
    Ok(LaunchSpec::new(provider.as_str(), args, env))
}

pub fn env_for_provider(
    paths: &Paths,
    kc: &dyn Keychain,
    provider: Provider,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let home = materialize_provider_home(paths, kc, provider, name)?;
    Ok(env_for_provider_home(provider, &home))
}

pub fn preview_env_for_provider(
    paths: &Paths,
    kc: &dyn Keychain,
    provider: Provider,
    name: &str,
) -> Result<Vec<(String, String)>> {
    if !has_provider(paths, kc, provider, name)? {
        return Err(Error::ProfileNotFound(name.to_string()));
    }
    let home = provider_home_path(paths, provider, name);
    Ok(env_for_provider_home(provider, &home))
}

pub fn preview_env_for_shell(
    paths: &Paths,
    kc: &dyn Keychain,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    if has_provider(paths, kc, Provider::Claude, name)? {
        env.extend(preview_env_for_provider(paths, kc, Provider::Claude, name)?);
    }
    if has_provider(paths, kc, Provider::Codex, name)? {
        env.extend(preview_env_for_provider(paths, kc, Provider::Codex, name)?);
    }
    if env.is_empty() {
        return Err(Error::ProfileNotFound(name.to_string()));
    }
    Ok(env)
}

fn env_for_provider_home(provider: Provider, home: &Path) -> Vec<(String, String)> {
    match provider {
        Provider::Claude => vec![
            (
                "CLAUDE_CONFIG_DIR".to_string(),
                home.to_string_lossy().into_owned(),
            ),
            (
                "CLAUDE_HOME".to_string(),
                home.to_string_lossy().into_owned(),
            ),
        ],
        Provider::Codex => vec![(
            "CODEX_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        )],
    }
}

pub fn env_for_shell(
    paths: &Paths,
    kc: &dyn Keychain,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    if has_provider(paths, kc, Provider::Claude, name)? {
        env.extend(env_for_provider(paths, kc, Provider::Claude, name)?);
    }
    if has_provider(paths, kc, Provider::Codex, name)? {
        env.extend(env_for_provider(paths, kc, Provider::Codex, name)?);
    }
    if env.is_empty() {
        return Err(Error::ProfileNotFound(name.to_string()));
    }
    Ok(env)
}

pub fn has_provider(
    paths: &Paths,
    kc: &dyn Keychain,
    provider: Provider,
    name: &str,
) -> Result<bool> {
    Ok(match provider {
        Provider::Claude => {
            let account = keychain::profile_account(name);
            match kc.read(&account) {
                Ok(bytes) => {
                    OauthCreds::parse(&bytes)?;
                    true
                }
                Err(_) => false,
            }
        }
        Provider::Codex => provider_home_path(paths, Provider::Codex, name).exists(),
    })
}

fn materialize_provider_home(
    paths: &Paths,
    kc: &dyn Keychain,
    provider: Provider,
    name: &str,
) -> Result<PathBuf> {
    match provider {
        Provider::Claude => materialize_claude_home(paths, kc, name),
        Provider::Codex => ensure_codex_home(paths, name),
    }
}

fn provider_home_path(paths: &Paths, provider: Provider, name: &str) -> PathBuf {
    paths.profile_provider_home(name, provider.as_str())
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

pub fn ensure_codex_home(paths: &Paths, name: &str) -> Result<PathBuf> {
    let home = paths.profile_provider_home(name, "codex");
    fs::create_dir_all(&home).map_err(|e| Error::io_at(&home, e))?;

    if !paths.profile_codex_config(name).exists() && paths.codex_config().exists() {
        copy_into_home(&paths.codex_config(), &home.join("config.toml"))?;
    }

    let shared_skills = paths.codex_skills_dir();
    if fs::symlink_metadata(&shared_skills).is_ok() {
        ensure_symlink(&shared_skills, &home.join("skills"))?;
    }

    Ok(home)
}

fn copy_into_home(src: &Path, dst: &Path) -> Result<()> {
    let bytes = fs::read(src).map_err(|e| Error::io_at(src, e))?;
    provider::write_path_atomic(dst, &bytes)
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
