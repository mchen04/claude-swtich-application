use std::fs;

use crate::cli::{GlobalOpts, NameArg};
use crate::error::Result;
use crate::keychain::Keychain;
use crate::paths::Paths;

/// Hidden helper invoked by the shell wrapper when CS_SHELL_WRAPPER=1 is set. Copies
/// per-profile env vars (`~/.claude-cs/profiles/<name>/env`) into
/// `~/.claude-cs/.last-env`, which the wrapper sources after the call returns.
pub fn emit_env(
    paths: &Paths,
    _kc: &dyn Keychain,
    _global: &GlobalOpts,
    args: &NameArg,
) -> Result<()> {
    let env_file = paths.profile_dir(&args.name).join("env");
    let last_env = paths.cs_home.join(".last-env");
    let bytes = fs::read(&env_file).unwrap_or_default();
    if let Some(parent) = last_env.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&last_env, &bytes);
    Ok(())
}
