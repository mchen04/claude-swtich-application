use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use crate::backup::{BackupAction, Manifest};
use crate::cli::GlobalOpts;
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::output::OutputOpts;
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
    let claude_target = read_target_claude(kc, target_name)?;

    if global.dry_run {
        let mut plan = Plan::new();
        plan.push(Action::KeychainWrite {
            account: keychain::canonical_account(),
            bytes: claude_target.len(),
        });
        let profile_settings = profile_settings_path(paths, target_name);
        if profile_settings.exists() {
            plan.push(Action::Copy {
                from: profile_settings,
                to: paths.claude_settings(),
            });
        }
        plan.push(Action::WriteFile {
            path: paths.state_file(),
            bytes: 0,
        });
        plan.push(Action::WriteFile {
            path: paths.active_profile_marker(),
            bytes: target_name.len(),
        });
        if !passthrough.is_empty() {
            plan.push(Action::SpawnProcess {
                cmd: "claude".into(),
                args: passthrough.to_vec(),
            });
        }
        let opts = OutputOpts {
            json: global.json,
            no_color: global.no_color,
        };
        crate::output::emit(opts, &plan)?;
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;
    paths.ensure_cs_home()?;

    let canonical = keychain::canonical_account();
    let prev_canonical_blob = kc.read(&canonical).ok();
    let prev_settings = fs::read(paths.claude_settings()).ok();
    let state_path = paths.state_file();
    let mut state = State::load(&state_path).unwrap_or_default();
    let prev_state_value = serde_json::to_value(&state).ok();
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

    let profile_settings = profile_settings_path(paths, target_name);
    if profile_settings.exists() {
        if let Err(e) = atomic_replace(&profile_settings, &paths.claude_settings()) {
            rollback_claude(kc, &canonical, prev_canonical_blob.as_deref());
            if let Some(prev) = prev_settings.as_deref() {
                let _ = write_bytes_atomic(&paths.claude_settings(), prev);
            }
            return Err(e);
        }
    }

    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if prior_active.as_deref() != Some(target_name) {
        state.previous = prior_active;
    }
    state.active = Some(target_name.to_string());
    state.switched_at_ms = Some(now_ms);
    state.since_ms = Some(now_ms);
    if state.active_claude.as_deref() != Some(target_name) {
        state.previous_claude = state.active_claude.clone();
    }
    state.active_claude = Some(target_name.to_string());
    state.save(&state_path)?;

    let marker = paths.active_profile_marker();
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&marker, target_name.as_bytes()) {
        tracing::warn!(?marker, error=%e, "could not write .active-profile marker");
    }

    let mut manifest = Manifest::new("switch");
    let target_blob = kc.read(&canonical).ok();
    manifest.push(BackupAction::KeychainReplace {
        account: canonical.clone(),
        before_b64: prev_canonical_blob.as_deref().map(crate::backup::b64),
        after_b64: target_blob.as_deref().map(crate::backup::b64),
    });
    if let Some(prev) = &prev_settings {
        manifest.push(BackupAction::SettingsReplace {
            before_b64: Some(crate::backup::b64(prev)),
            after_b64: fs::read(paths.claude_settings())
                .ok()
                .as_deref()
                .map(crate::backup::b64),
        });
    }
    manifest.push(BackupAction::StateReplace {
        before: prev_state_value,
        after: serde_json::to_value(&state).ok(),
    });
    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "switch", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    if !global.json {
        eprintln!("switched -> {target_name} (claude)");
    }

    if running_claude_processes() > 0 {
        eprintln!(
            "note: detected running `claude` process(es); restart them to pick up the new account"
        );
    }

    if !passthrough.is_empty() {
        let err = Command::new("claude").args(passthrough).exec();
        return Err(Error::Subprocess {
            cmd: "claude".into(),
            message: err.to_string(),
        });
    }
    Ok(())
}

pub fn run_claude_only(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    target_name: &str,
    passthrough: &[String],
) -> Result<()> {
    read_target_claude(kc, target_name)?;
    run(paths, kc, global, target_name, passthrough)
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

fn profile_settings_path(paths: &Paths, name: &str) -> std::path::PathBuf {
    paths.profile_claude_settings(name)
}

fn atomic_replace(src: &Path, dst: &Path) -> Result<()> {
    let bytes = fs::read(src).map_err(|e| Error::io_at(src, e))?;
    write_bytes_atomic(dst, &bytes)
}

fn write_bytes_atomic(dst: &Path, bytes: &[u8]) -> Result<()> {
    let parent = dst
        .parent()
        .ok_or_else(|| Error::Other(format!("settings dst has no parent: {}", dst.display())))?;
    fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    let tmp = parent.join(format!(".cs-settings.{}.tmp", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| Error::io_at(&tmp, e))?;
    fs::rename(&tmp, dst).map_err(|e| Error::io_at(dst, e))?;
    Ok(())
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
