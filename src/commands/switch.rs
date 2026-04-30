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
    let target_account = keychain::profile_account(target_name);
    let target_blob = kc
        .read(&target_account)
        .map_err(|_| Error::ProfileNotFound(target_name.to_string()))?;
    let target_creds = OauthCreds::parse(&target_blob)?;

    let canonical = keychain::canonical_account();
    let prev_canonical_blob = kc.read(&canonical).ok();

    if global.dry_run {
        let mut plan = Plan::new();
        plan.push(Action::KeychainWrite {
            account: canonical.clone(),
            bytes: target_blob.len(),
        });
        plan.push(Action::WriteFile {
            path: paths.state_file(),
            bytes: 0,
        });
        plan.push(Action::WriteFile {
            path: paths.active_profile_marker(),
            bytes: target_name.len(),
        });
        if let Some(profile_settings) = profile_settings_path(paths, target_name) {
            if profile_settings.exists() {
                plan.push(Action::Copy {
                    from: profile_settings,
                    to: paths.claude_settings(),
                });
            }
        }
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

    if target_creds.is_expired(std::time::Duration::from_secs(60)) {
        eprintln!(
            "warning: target profile `{}` token is near expiry; consider `cs refresh {}` first",
            target_name, target_name
        );
    }

    // Atomic-ish keychain swap with verify + rollback.
    kc.write(&canonical, &target_blob)?;
    match kc.read(&canonical) {
        Ok(b) if b == target_blob => {}
        Ok(_) | Err(_) => {
            if let Some(prev) = &prev_canonical_blob {
                let _ = kc.write(&canonical, prev);
            }
            return Err(Error::Other(
                "canonical Keychain write verification failed; rolled back to previous".into(),
            ));
        }
    }

    // Per-profile settings.json (if present).
    let mut prev_settings: Option<Vec<u8>> = None;
    if let Some(profile_settings) = profile_settings_path(paths, target_name) {
        if profile_settings.exists() {
            let dst = paths.claude_settings();
            prev_settings = fs::read(&dst).ok();
            atomic_replace(&profile_settings, &dst)?;
        }
    }

    // State update.
    let state_path = paths.state_file();
    let mut state = State::load(&state_path).unwrap_or_default();
    let prev_state_value = serde_json::to_value(&state).ok();
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let prior_active = state.active.clone();
    if prior_active.as_deref() != Some(target_name) {
        state.previous = prior_active;
    }
    state.active = Some(target_name.to_string());
    state.switched_at_ms = Some(now_ms);
    state.since_ms = Some(now_ms);
    state.save(&state_path)?;

    // .active-profile marker for compat with the legacy claude-switch tool.
    let marker = paths.active_profile_marker();
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&marker, target_name.as_bytes()) {
        tracing::warn!(?marker, error=%e, "could not write .active-profile marker");
    }

    // Manifest.
    let mut manifest = Manifest::new("switch");
    manifest.push(BackupAction::KeychainReplace {
        account: canonical,
        before_b64: prev_canonical_blob.as_deref().map(crate::backup::b64),
        after_b64: Some(crate::backup::b64(&target_blob)),
    });
    manifest.push(BackupAction::StateReplace {
        before: prev_state_value,
        after: serde_json::to_value(&state).ok(),
    });
    if let Some(prev) = &prev_settings {
        manifest.push(BackupAction::SettingsReplace {
            before_b64: Some(crate::backup::b64(prev)),
            after_b64: fs::read(paths.claude_settings()).ok().as_deref().map(crate::backup::b64),
        });
    }
    let _ = manifest.write(paths);

    if !global.json {
        eprintln!("switched -> {target_name}");
    }

    if running_claude_processes() > 0 {
        eprintln!(
            "note: detected running `claude` process(es); restart them to pick up the new account"
        );
    }

    if !passthrough.is_empty() {
        let err = Command::new("claude").args(passthrough).exec();
        // exec() only returns on failure.
        return Err(Error::Subprocess {
            cmd: "claude".into(),
            message: err.to_string(),
        });
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

fn profile_settings_path(paths: &Paths, name: &str) -> Option<std::path::PathBuf> {
    Some(paths.profile_dir(name).join("settings.json"))
}

fn atomic_replace(src: &Path, dst: &Path) -> Result<()> {
    let parent = dst
        .parent()
        .ok_or_else(|| Error::Other(format!("settings dst has no parent: {}", dst.display())))?;
    fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    let tmp = parent.join(format!(
        ".cs-settings.{}.tmp",
        std::process::id()
    ));
    let bytes = fs::read(src).map_err(|e| Error::io_at(src, e))?;
    fs::write(&tmp, &bytes).map_err(|e| Error::io_at(&tmp, e))?;
    fs::rename(&tmp, dst).map_err(|e| Error::io_at(dst, e))?;
    Ok(())
}

fn running_claude_processes() -> usize {
    let out = Command::new("/usr/bin/pgrep").args(["-x", "claude"]).output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .count(),
        _ => 0,
    }
}
