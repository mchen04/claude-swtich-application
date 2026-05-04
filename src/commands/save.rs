use crate::cli::{GlobalOpts, SaveArgs};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::profile::OauthCreds;

pub fn run(paths: &Paths, kc: &dyn Keychain, _global: &GlobalOpts, args: &SaveArgs) -> Result<()> {
    crate::paths::validate_profile_name(&args.name)?;
    let canonical = keychain::canonical_account();
    let target = keychain::profile_account(&args.name);

    let canonical_blob = kc.read(&canonical).map_err(|_| {
        Error::Other("no active Claude credential to save (run `claude /login` first)".into())
    })?;
    OauthCreds::parse(&canonical_blob)?;

    let existed = kc.read(&target).is_ok();

    let claude_settings = std::fs::read(paths.claude_settings()).ok();

    let _lock = CsLock::acquire(paths)?;
    keychain::write_verified(kc, &target, &canonical_blob)?;
    if let Some(settings) = claude_settings.as_deref() {
        crate::jsonio::atomic_write_bytes(&paths.profile_claude_settings(&args.name), settings)?;
    }

    if existed {
        eprintln!("overwrote profile `{}` with current claude credentials", args.name);
    } else {
        eprintln!("saved profile `{}` for claude", args.name);
    }
    Ok(())
}
