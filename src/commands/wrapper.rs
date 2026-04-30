use std::fs;

use crate::cli::{GlobalOpts, NameArg};
use crate::error::Result;
use crate::keychain::Keychain;
use crate::paths::Paths;

/// Hidden helper invoked by the shell wrapper when CS_SHELL_WRAPPER=1 is set. Emits
/// per-profile env vars to ~/.claude-cs/.last-env (sourced post-call by the wrapper).
/// Phase C ships an empty implementation; profile-local env files arrive in a follow-up.
pub fn emit_env(paths: &Paths, _kc: &dyn Keychain, _global: &GlobalOpts, args: &NameArg) -> Result<()> {
    let env_file = paths.profile_dir(&args.name).join("env");
    let last_env = paths.last_env_file();
    let bytes = fs::read(&env_file).unwrap_or_default();
    if let Some(parent) = last_env.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&last_env, &bytes);
    Ok(())
}
