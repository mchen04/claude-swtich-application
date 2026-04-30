use std::fs;

use crate::cli::{GlobalOpts, NameArg};
use crate::error::{Error, Result};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::state::State;

pub fn run(paths: &Paths, global: &GlobalOpts, args: &NameArg) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let active = state.active.clone().ok_or(Error::NoActiveProfile)?;
    let src = paths
        .profile_dir(&active)
        .join("overrides")
        .join("skills")
        .join(&args.name);
    if !src.exists() {
        return Err(Error::Other(format!(
            "no profile-local skill at {} — nothing to share",
            src.display()
        )));
    }
    let dst = paths.master_dir().join("skills").join(&args.name);
    if dst.exists() {
        return Err(Error::Other(format!(
            "skill already in master at {}",
            dst.display()
        )));
    }
    if global.dry_run {
        eprintln!("would move {} -> {}", src.display(), dst.display());
        return Ok(());
    }
    let _lock = CsLock::acquire(paths)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    fs::rename(&src, &dst).map_err(|e| Error::io_at(&dst, e))?;
    eprintln!("promoted skill `{}` from `{}` to master", args.name, active);
    Ok(())
}
