use crate::backup::{BackupAction, Manifest};
use crate::cli::{GlobalOpts, NameArg};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::output::OutputOpts;
use crate::paths::Paths;
use crate::state::State;

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &NameArg) -> Result<()> {
    let state_pre = State::load(&paths.state_file()).unwrap_or_default();
    if state_pre.master.as_deref() == Some(args.name.as_str()) {
        return Err(Error::MasterProfileLocked(args.name.clone()));
    }

    let target = keychain::profile_account(&args.name);
    let existing = kc.read(&target).ok();
    let profile_dir = paths.profile_dir(&args.name);
    let has_profile_dir = profile_dir.exists();
    if existing.is_none() && !has_profile_dir {
        return Err(Error::ProfileNotFound(args.name.clone()));
    }

    if global.dry_run {
        let mut plan = Plan::new();
        if existing.is_some() {
            plan.push(Action::KeychainDelete {
                account: target.clone(),
            });
        }
        if has_profile_dir {
            plan.push(Action::Note {
                message: format!("would remove {}", profile_dir.display()),
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
    if existing.is_some() {
        kc.delete(&target)?;
    }
    if has_profile_dir {
        std::fs::remove_dir_all(&profile_dir).map_err(|e| Error::io_at(&profile_dir, e))?;
    }

    // Clean up state references to this profile (default/active/previous) — but never
    // touch the canonical Claude Code Keychain entry. If the user removes the active
    // profile, leave `active` cleared so `cs status` shows "no active".
    let path = paths.state_file();
    let mut state = State::load(&path).unwrap_or_default();
    let mut changed = false;
    if state.active.as_deref() == Some(&args.name) {
        state.active = None;
        changed = true;
    }
    if state.previous.as_deref() == Some(&args.name) {
        state.previous = None;
        changed = true;
    }
    if state.default.as_deref() == Some(&args.name) {
        state.default = None;
        changed = true;
    }
    if state.active_claude.as_deref() == Some(&args.name) {
        state.active_claude = None;
        changed = true;
    }
    if state.previous_claude.as_deref() == Some(&args.name) {
        state.previous_claude = None;
        changed = true;
    }
    if changed {
        state.save(&path)?;
    }

    let mut manifest = Manifest::new("rm");
    if let Some(existing) = existing {
        manifest.push(BackupAction::KeychainReplace {
            account: target,
            before_b64: Some(crate::backup::b64(&existing)),
            after_b64: None,
        });
    }
    if has_profile_dir {
        manifest.push(BackupAction::Note {
            message: format!("removed profile dir {}", profile_dir.display()),
        });
    }
    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "rm", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    eprintln!("removed profile `{}`", args.name);
    Ok(())
}
