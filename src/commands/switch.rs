use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::cli::GlobalOpts;
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::profile::OauthCreds;
use crate::state::State;

pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    target_name: &str,
    passthrough: &[String],
) -> Result<()> {
    let _lock = CsLock::acquire(paths)?;
    paths.ensure_cs_home()?;
    run_locked(paths, kc, global, target_name)?;

    if !passthrough.is_empty() {
        let err = Command::new("claude").args(passthrough).exec();
        return Err(Error::Subprocess {
            cmd: "claude".into(),
            message: err.to_string(),
        });
    }
    Ok(())
}

/// Same as [`run`] minus the lock acquisition and the `claude` exec. Callers
/// (currently the auto-switch tick) MUST already hold a [`CsLock`] before
/// invoking this so the re-check it performed before deciding to switch
/// remains valid through the swap.
pub(crate) fn run_locked(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    target_name: &str,
) -> Result<()> {
    let claude_target = read_target_claude(kc, target_name)?;

    let canonical = keychain::canonical_account();
    let prev_canonical_blob = kc.read(&canonical).ok();
    let prev_settings = fs::read(paths.claude_settings()).ok();
    let state_path = paths.state_file();
    let mut state = State::load(&state_path).unwrap_or_default();
    let prior_active = state.active.clone();

    let target_creds = OauthCreds::parse(&claude_target)?;
    if target_creds.is_expired(std::time::Duration::from_secs(60)) {
        eprintln!(
            "warning: target Claude profile `{}` token is near expiry; consider `cs refresh {}` first",
            target_name, target_name
        );
    }

    kc.write(&canonical, &claude_target)?;
    match kc.read(&canonical) {
        Ok(b) if b == claude_target => {}
        Ok(_) | Err(_) => {
            rollback_claude(kc, &canonical, prev_canonical_blob.as_deref());
            return Err(Error::Other(
                "canonical Keychain write verification failed; rolled back to previous".into(),
            ));
        }
    }

    let profile_settings = paths.profile_claude_settings(target_name);
    if profile_settings.exists() {
        if let Err(e) = atomic_replace(&profile_settings, &paths.claude_settings()) {
            rollback_claude(kc, &canonical, prev_canonical_blob.as_deref());
            if let Some(prev) = prev_settings.as_deref() {
                let _ = write_bytes_atomic(&paths.claude_settings(), prev);
            }
            return Err(e);
        }
    }

    if prior_active.as_deref() != Some(target_name) {
        state.previous = prior_active;
    }
    state.active = Some(target_name.to_string());
    state.save(&state_path)?;

    let marker = paths.active_profile_marker();
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&marker, target_name.as_bytes()) {
        tracing::warn!(?marker, error=%e, "could not write .active-profile marker");
    }

    if !global.json {
        eprintln!("switched -> {target_name} (claude)");
    }

    if running_claude_processes() > 0 {
        eprintln!(
            "note: detected running `claude` process(es); restart them to pick up the new account"
        );
    }
    Ok(())
}

pub fn run_previous(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    passthrough: &[String],
) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let prev = state.previous.clone().ok_or(Error::NoPreviousProfile)?;
    run(paths, kc, global, &prev, passthrough)
}

fn read_target_claude(kc: &dyn Keychain, target_name: &str) -> Result<Vec<u8>> {
    let target_account = keychain::profile_account(target_name);
    let target_blob = kc
        .read(&target_account)
        .map_err(|_| Error::ProfileNotFound(target_name.to_string()))?;
    OauthCreds::parse(&target_blob)?;
    Ok(target_blob)
}

fn rollback_claude(kc: &dyn Keychain, canonical: &str, prev: Option<&[u8]>) {
    if let Some(prev) = prev {
        if let Err(e) = kc.write(canonical, prev) {
            eprintln!("error: keychain rollback failed for {canonical}: {e}");
            tracing::error!(account = %canonical, error = %e, "keychain rollback failed");
        }
    }
}

fn atomic_replace(src: &Path, dst: &Path) -> Result<()> {
    let bytes = fs::read(src).map_err(|e| Error::io_at(src, e))?;
    write_bytes_atomic(dst, &bytes)
}

fn write_bytes_atomic(dst: &Path, bytes: &[u8]) -> Result<()> {
    crate::jsonio::atomic_write_bytes(dst, bytes)
}

fn running_claude_processes() -> usize {
    let out = Command::new("/usr/bin/pgrep")
        .args(["-x", "claude"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .count(),
        _ => 0,
    }
}
