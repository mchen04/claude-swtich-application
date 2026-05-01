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
    let codex_blob = provider::read_codex_active_blob(paths).ok();

    let mut will_save_claude = false;
    if let Some(blob) = canonical_blob.as_deref() {
        OauthCreds::parse(blob)?;
        will_save_claude = true;
    }
    let mut will_save_codex = false;
    if let Some(blob) = codex_blob.as_deref() {
        let _ = serde_json::from_slice::<serde_json::Value>(blob)?;
        will_save_codex = true;
    }

    if !will_save_claude && !will_save_codex {
        return Err(Error::Other(
            "no active Claude or Codex credentials to save (run `claude /login` or `codex login` first)"
                .into(),
        ));
    }

    let pre_existing_claude = kc.read(&target).ok();
    let codex_profile = paths.profile_codex_auth(&args.name);
    let pre_existing_codex = codex_profile.exists();
    if !global.dry_run {
        if will_save_claude && pre_existing_claude.is_some() {
            return Err(Error::ProfileExists(args.name.clone()));
        }
        if will_save_codex && pre_existing_codex {
            return Err(Error::ProfileExists(args.name.clone()));
        }
    }

    if global.dry_run {
        let mut plan = Plan::new();
        if will_save_claude {
            plan.push(Action::KeychainWrite {
                account: target.clone(),
                bytes: canonical_blob.as_deref().map(|b| b.len()).unwrap_or(0),
            });
        }
        if will_save_codex {
            plan.push(Action::WriteFile {
                path: codex_profile.clone(),
                bytes: codex_blob.as_deref().map(|b| b.len()).unwrap_or(0),
            });
        }
        if pre_existing_claude.is_some() || pre_existing_codex {
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

    if will_save_claude {
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
    }

    if will_save_codex {
        let codex_blob = codex_blob.expect("checked above");
        provider::write_codex_profile_blob(paths, &args.name, &codex_blob)?;
        manifest.push(BackupAction::Note {
            message: format!("saved codex profile blob to {}", codex_profile.display()),
        });
    }

    if let Err(e) = manifest.write(paths) {
        tracing::warn!(op = "save", error = %e, "failed to write rollback manifest");
        eprintln!("warning: failed to write rollback manifest: {e}");
    }

    match (will_save_claude, will_save_codex) {
        (true, true) => eprintln!("saved profile `{}` for claude + codex", args.name),
        (true, false) => eprintln!("saved profile `{}` for claude", args.name),
        (false, true) => eprintln!("saved profile `{}` for codex", args.name),
        (false, false) => {}
    }
    Ok(())
}
