use crate::backup::{BackupAction, Manifest};
use crate::cli::{GlobalOpts, SaveArgs};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::output::OutputOpts;
use crate::paths::Paths;
use crate::profile::OauthCreds;

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &SaveArgs) -> Result<()> {
    let canonical = keychain::canonical_account();
    let target = keychain::profile_account(&args.name);

    let canonical_blob = kc
        .read(&canonical)
        .map_err(|e| Error::Other(format!(
            "no active Claude Code credential to save (run `claude /login` first): {e}"
        )))?;
    // Validate parseability up front so we never write a malformed blob to a profile.
    OauthCreds::parse(&canonical_blob)?;

    let pre_existing = kc.read(&target).ok();
    if pre_existing.is_some() && !global.dry_run {
        return Err(Error::ProfileExists(args.name.clone()));
    }

    if global.dry_run {
        let mut plan = Plan::new();
        plan.push(Action::KeychainWrite {
            account: target.clone(),
            bytes: canonical_blob.len(),
        });
        if pre_existing.is_some() {
            plan.push(Action::Note {
                message: format!("would refuse: profile `{}` already exists (use `cs rm` first)", args.name),
            });
        }
        let opts = OutputOpts { json: global.json, no_color: global.no_color };
        crate::output::emit(opts, &plan)?;
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;

    kc.write(&target, &canonical_blob)?;

    // Verify byte-equal round-trip; rollback on mismatch.
    match kc.read(&target) {
        Ok(roundtrip) if roundtrip == canonical_blob => {}
        Ok(_) | Err(_) => {
            let _ = kc.delete(&target);
            return Err(Error::Other(format!(
                "Keychain write verification failed for {target}; rolled back"
            )));
        }
    }

    let mut manifest = Manifest::new("save");
    manifest.push(BackupAction::KeychainReplace {
        account: target.clone(),
        before_b64: None,
        after_b64: Some(crate::backup::b64(&canonical_blob)),
    });
    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "save", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    eprintln!("saved profile `{}` ({} bytes)", args.name, canonical_blob.len());
    Ok(())
}
