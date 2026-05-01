use std::fs;

use crate::backup::{BackupAction, Manifest};
use crate::cli::{GlobalOpts, RenameArgs};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::master;
use crate::output::OutputOpts;
use crate::paths::Paths;
use crate::state::State;

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &RenameArgs) -> Result<()> {
    if args.from == args.to {
        return Err(Error::InvalidArgument(
            "source and target are the same".into(),
        ));
    }
    let from_acct = keychain::profile_account(&args.from);
    let to_acct = keychain::profile_account(&args.to);

    let blob = kc.read(&from_acct).ok();
    let from_codex = paths.profile_codex_auth(&args.from);
    let to_codex = paths.profile_codex_auth(&args.to);
    let has_from_codex = from_codex.exists();
    if blob.is_none() && !has_from_codex {
        return Err(Error::ProfileNotFound(args.from.clone()));
    }
    if blob.is_some() && kc.read(&to_acct).is_ok() {
        return Err(Error::ProfileExists(args.to.clone()));
    }
    if has_from_codex && to_codex.exists() {
        return Err(Error::ProfileExists(args.to.clone()));
    }

    let from_dir = paths.profile_dir(&args.from);
    let to_dir = paths.profile_dir(&args.to);
    let dir_exists = from_dir.exists();
    if dir_exists && to_dir.exists() {
        return Err(Error::ProfileExists(args.to.clone()));
    }

    if global.dry_run {
        let mut plan = Plan::new();
        if let Some(blob) = blob.as_ref() {
            plan.push(Action::KeychainWrite {
                account: to_acct.clone(),
                bytes: blob.len(),
            });
            plan.push(Action::KeychainDelete {
                account: from_acct.clone(),
            });
        }
        if has_from_codex && !dir_exists {
            plan.push(Action::Move {
                from: from_codex.clone(),
                to: to_codex.clone(),
            });
        }
        if dir_exists {
            plan.push(Action::Move {
                from: from_dir.clone(),
                to: to_dir.clone(),
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
    if let Some(blob) = blob.as_ref() {
        kc.write(&to_acct, blob)?;
        if kc.read(&to_acct).map(|b| b == *blob).unwrap_or(false) {
            kc.delete(&from_acct)?;
        } else {
            let _ = kc.delete(&to_acct);
            return Err(Error::Other(format!(
                "Keychain write verification failed for {to_acct}; rolled back"
            )));
        }
    }
    if has_from_codex && !dir_exists {
        if let Some(parent) = to_codex.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
        }
        fs::rename(&from_codex, &to_codex).map_err(|e| Error::io_at(&to_codex, e))?;
    }

    if dir_exists {
        if let Some(parent) = to_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
        }
        fs::rename(&from_dir, &to_dir).map_err(|e| Error::io_at(&to_dir, e))?;
    }

    // Update state references.
    let path = paths.state_file();
    let mut state = State::load(&path).unwrap_or_default();
    let mut changed = false;
    for slot in [&mut state.active, &mut state.previous, &mut state.default] {
        if slot.as_deref() == Some(&args.from) {
            *slot = Some(args.to.clone());
            changed = true;
        }
    }
    for slot in [
        &mut state.active_claude,
        &mut state.previous_claude,
        &mut state.active_codex,
        &mut state.previous_codex,
    ] {
        if slot.as_deref() == Some(&args.from) {
            *slot = Some(args.to.clone());
            changed = true;
        }
    }
    let was_master = state.master.as_deref() == Some(args.from.as_str());
    if was_master {
        state.master = Some(args.to.clone());
        changed = true;
    }
    if changed {
        state.save(&path)?;
    }

    let mut manifest = Manifest::new("rename");
    if let Some(blob) = blob.as_ref() {
        manifest.push(BackupAction::KeychainReplace {
            account: from_acct,
            before_b64: Some(crate::backup::b64(blob)),
            after_b64: None,
        });
        manifest.push(BackupAction::KeychainReplace {
            account: to_acct,
            before_b64: None,
            after_b64: Some(crate::backup::b64(blob)),
        });
    }
    if has_from_codex && !dir_exists {
        manifest.push(BackupAction::FsMove {
            from: from_codex.clone(),
            to: to_codex.clone(),
        });
    }
    if dir_exists {
        manifest.push(BackupAction::FsMove {
            from: from_dir.clone(),
            to: to_dir.clone(),
        });
    }
    if was_master {
        let symlink_actions = master::retarget_symlinks(paths, &args.to)?;
        for a in symlink_actions {
            manifest.push(a);
        }
        manifest.master_profile = Some(args.to.clone());
    }
    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "rename", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    eprintln!("renamed `{}` -> `{}`", args.from, args.to);
    Ok(())
}
