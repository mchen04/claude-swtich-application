use crate::cli::{GlobalOpts, LinkArgs};
use crate::error::{Error, Result};
use crate::keychain;
use crate::links;
use crate::lock::CsLock;
use crate::output::emit_json;
use crate::paths::Paths;
use crate::state::State;

pub fn link(paths: &Paths, global: &GlobalOpts, args: &LinkArgs) -> Result<()> {
    let kc = crate::keychain::default_keychain();
    let cwd = links::canonical_cwd()?;
    let profile = match args.profile.clone() {
        Some(p) => p,
        None => State::load(&paths.state_file())
            .ok()
            .and_then(|s| s.active)
            .ok_or(Error::NoActiveProfile)?,
    };
    let acct = keychain::profile_account(&profile);
    if kc.read(&acct).is_err() {
        return Err(Error::ProfileNotFound(profile));
    }

    if global.dry_run {
        eprintln!("would bind {cwd} -> {profile}");
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;
    let mut file = links::load(paths)?;
    file.bindings.insert(cwd.clone(), profile.clone());
    links::save(paths, &file)?;
    eprintln!("bound {cwd} -> {profile}");
    Ok(())
}

pub fn list(paths: &Paths, global: &GlobalOpts) -> Result<()> {
    let file = links::load(paths)?;
    if global.json {
        emit_json(&file)?;
        return Ok(());
    }
    if file.bindings.is_empty() {
        println!("(no cwd bindings)");
        return Ok(());
    }
    for (cwd, profile) in &file.bindings {
        println!("{cwd} -> {profile}");
    }
    Ok(())
}

