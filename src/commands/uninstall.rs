use std::fs;

use crate::cli::{GlobalOpts, UninstallArgs};
use crate::error::{Error, Result};
use crate::lock::CsLock;
use crate::master;
use crate::paths::Paths;
use crate::shell::{self, Shell};

pub fn run(paths: &Paths, global: &GlobalOpts, args: &UninstallArgs) -> Result<()> {
    if global.dry_run {
        let report = master::uninstall(paths, args.keep_master, true)?;
        eprintln!("{:#?}", report);
        return Ok(());
    }
    let _lock = CsLock::acquire(paths)?;
    let report = master::uninstall(paths, args.keep_master, false)?;
    for n in &report.restored {
        eprintln!("restored {n}");
    }
    for n in &report.left_in_place {
        eprintln!("left in master: {n}");
    }

    // Remove shell wrapper from rc files (best-effort, idempotent).
    for sh in [Shell::Zsh, Shell::Bash] {
        if let Some(rc) = sh.rc_path() {
            if let Ok(existing) = fs::read_to_string(&rc) {
                let updated = shell::remove_block(&existing);
                if updated != existing {
                    fs::write(&rc, updated).map_err(|e| Error::io_at(&rc, e))?;
                    eprintln!("removed cs wrapper from {}", rc.display());
                }
            }
        }
    }

    eprintln!(
        "Keychain entries are left untouched. Remove them via `cs rm <name>` or \
         `security delete-generic-password -s 'Claude Code-credentials' -a 'Claude Code-credentials-<name>'`."
    );
    Ok(())
}
