use std::env;
use std::fs;

use crate::cli::{GlobalOpts, UninstallArgs};
use crate::commands::auto_switch as auto_switch_cmd;
use crate::error::Result;
use crate::lock::CsLock;
use crate::master;
use crate::paths::Paths;
use crate::shell::{self, Shell};

pub fn run(paths: &Paths, _global: &GlobalOpts, args: &UninstallArgs) -> Result<()> {
    let _lock = CsLock::acquire(paths)?;
    let report = master::uninstall(paths, args.keep_master)?;
    for n in &report.restored {
        eprintln!("restored {n}");
    }
    for n in &report.left_in_place {
        eprintln!("left in profile: {n}");
    }

    // Remove shell wrapper from rc files (best-effort, idempotent).
    for sh in [Shell::Zsh, Shell::Bash] {
        if let Some(rc) = sh.rc_path() {
            if let Ok(existing) = fs::read_to_string(&rc) {
                let updated = shell::remove_block(&existing);
                if updated != existing {
                    crate::jsonio::atomic_write_bytes(&rc, updated.as_bytes())?;
                    eprintln!("removed cs wrapper from {}", rc.display());
                }
            }
        }
    }

    // Tear down the auto-switch launchd agent if it was installed.
    if env::var_os("CS_TEST_NO_LAUNCHCTL").is_none() {
        auto_switch_cmd::bootout_launchctl();
    }
    let plist = paths.launch_agents_plist();
    if plist.exists() {
        if let Err(e) = fs::remove_file(&plist) {
            tracing::warn!(path = %plist.display(), error = %e, "could not remove plist");
        } else {
            eprintln!("removed {}", plist.display());
        }
    }
    let settings = paths.cs_settings();
    if settings.exists() {
        if let Err(e) = fs::remove_file(&settings) {
            tracing::warn!(path = %settings.display(), error = %e, "could not remove cs settings");
        }
    }

    eprintln!(
        "Keychain entries are left untouched. Remove them via `cs rm <name>` or \
         `security delete-generic-password -s 'Claude Code-credentials' -a 'Claude Code-credentials-<name>'`."
    );
    Ok(())
}
