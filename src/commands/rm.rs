use crate::cli::{GlobalOpts, NameArg};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::state::State;

pub fn run(paths: &Paths, kc: &dyn Keychain, _global: &GlobalOpts, args: &NameArg) -> Result<()> {
    let state_pre = State::load(&paths.state_file()).unwrap_or_default();
    if state_pre.master.as_deref() == Some(args.name.as_str()) {
        return Err(Error::MasterProfileLocked(args.name.clone()));
    }

    let target = keychain::profile_account(&args.name);
    let kc_present = kc.read(&target).is_ok();
    let profile_dir = paths.profile_dir(&args.name);
    let has_profile_dir = profile_dir.exists();
    if !kc_present && !has_profile_dir {
        return Err(Error::ProfileNotFound(args.name.clone()));
    }

    let _lock = CsLock::acquire(paths)?;
    if kc_present {
        kc.delete(&target)?;
    }
    if has_profile_dir {
        std::fs::remove_dir_all(&profile_dir).map_err(|e| Error::io_at(&profile_dir, e))?;
    }

    // Clean up state references — never touch the canonical Claude Code Keychain entry.
    let path = paths.state_file();
    let mut state = State::load(&path).unwrap_or_default();
    let mut changed = false;
    for slot in [&mut state.active, &mut state.previous, &mut state.default] {
        if slot.as_deref() == Some(&args.name) {
            *slot = None;
            changed = true;
        }
    }
    if changed {
        state.save(&path)?;
    }

    eprintln!("removed profile `{}`", args.name);
    Ok(())
}
