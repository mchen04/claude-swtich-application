use std::fs;
use std::path::PathBuf;

use crate::cli::{GlobalOpts, MigrateArgs};
use crate::error::{Error, Result};
use crate::keychain::Keychain;
use crate::paths::Paths;

pub fn run(paths: &Paths, _kc: &dyn Keychain, global: &GlobalOpts, args: &MigrateArgs) -> Result<()> {
    let cfg_path = args.from.clone().unwrap_or_else(|| paths.config_file.clone());
    let bytes = fs::read(&cfg_path).map_err(|e| Error::io_at(&cfg_path, e))?;
    let text = String::from_utf8_lossy(&bytes);

    // Legacy claude-switch stores key=value lines (e.g. `default=personal`,
    // `profile.personal.email=foo@bar`). We don't have a fixture on this machine, so we
    // surface the file contents and instruct the user to re-save profiles via `claude
    // /login` + `cs save`.
    let path: PathBuf = cfg_path.clone();
    if global.dry_run {
        eprintln!("would inspect {} ({} bytes)", path.display(), bytes.len());
        return Ok(());
    }
    eprintln!("legacy config at {} ({} bytes)", path.display(), bytes.len());
    eprintln!(
        "cs cannot reuse legacy keychain entries because acct names differ; for each \
         profile listed below, run `claude /login`, then `cs save <name>`."
    );
    for line in text.lines() {
        if line.trim_start().starts_with("profile.") {
            eprintln!("  {line}");
        }
    }
    Ok(())
}
