use crate::backup::{BackupAction, Manifest};
use crate::cli::{GlobalOpts, SaveArgs};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::output::OutputOpts;
use crate::paths::Paths;
use crate::profile::OauthCreds;
use crate::provider;

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &SaveArgs) -> Result<()> {
    let canonical = keychain::canonical_account();
    let target = keychain::profile_account(&args.name);

    let canonical_blob = kc.read(&canonical).ok();
    let claude_settings = std::fs::read(paths.claude_settings()).ok();

    let mut will_save_claude = false;
    if let Some(blob) = canonical_blob.as_deref() {
        OauthCreds::parse(blob)?;
        will_save_claude = true;
    }

    if !will_save_claude {
        return Err(Error::Other(
            "no active Claude credential to save (run `claude /login` first)".into(),
        ));
    }

    let pre_existing_claude = kc.read(&target).ok();
    if !global.dry_run && pre_existing_claude.is_some() {
        return Err(Error::ProfileExists(args.name.clone()));
    }

    if global.dry_run {
        let mut plan = Plan::new();
        plan.push(Action::KeychainWrite {
            account: target.clone(),
            bytes: canonical_blob.as_deref().map(|b| b.len()).unwrap_or(0),
        });
        if let Some(bytes) = claude_settings.as_deref() {
            plan.push(Action::WriteFile {
                path: paths.profile_claude_settings(&args.name),
                bytes: bytes.len(),
            });
        }
        if pre_existing_claude.is_some() {
            plan.push(Action::Note {
                message: format!(
                    "would refuse: profile `{}` already exists (use `cs rm` first)",
                    args.name
                ),
            });
        }
        let opts = OutputOpts {
            json: global.json,
            no_color: global.no_color,
        };
        crate::output::emit(opts, &plan)?;
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;
    let mut manifest = Manifest::new("save");

    let canonical_blob = canonical_blob.expect("checked above");
    kc.write(&target, &canonical_blob)?;
    match kc.read(&target) {
        Ok(roundtrip) if roundtrip == canonical_blob => {}
        Ok(_) | Err(_) => {
            let _ = kc.delete(&target);
            return Err(Error::Other(format!(
                "Keychain write verification failed for {target}; rolled back"
            )));
        }
    }
    manifest.push(BackupAction::KeychainReplace {
        account: target.clone(),
        before_b64: None,
        after_b64: Some(crate::backup::b64(&canonical_blob)),
    });
    if let Some(settings) = claude_settings.as_deref() {
        provider::write_path_atomic(&paths.profile_claude_settings(&args.name), settings)?;
    }

    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "save", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    eprintln!("saved profile `{}` for claude", args.name);
    Ok(())
}
