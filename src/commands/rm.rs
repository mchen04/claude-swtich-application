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
    let existing = kc.read(&target).map_err(|_| Error::ProfileNotFound(args.name.clone()))?;

    if global.dry_run {
        let mut plan = Plan::new();
        plan.push(Action::KeychainDelete { account: target.clone() });
        let opts = OutputOpts { json: global.json, no_color: global.no_color };
        crate::output::emit(opts, &plan)?;
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;
    kc.delete(&target)?;

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
    if changed {
        state.save(&path)?;
    }

    let mut manifest = Manifest::new("rm");
    manifest.push(BackupAction::KeychainReplace {
        account: target,
        before_b64: Some(crate::backup::b64(&existing)),
        after_b64: None,
    });
    let _ = manifest.write(paths);

    eprintln!("removed profile `{}`", args.name);
    Ok(())
}
