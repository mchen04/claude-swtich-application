use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::cli::{GlobalOpts, OptionalNameArg};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::profile::OauthCreds;
use crate::state::State;

/// Strategy: Anthropic doesn't expose a public OAuth refresh endpoint for first-party
/// Claude Code creds, so we delegate refresh to the `claude` CLI itself. We write the
/// stale profile blob into the canonical Keychain entry, run a no-op `claude` invocation
/// (which triggers Claude Code's own refresh), then copy the freshly-refreshed canonical
/// blob back into the profile entry.
pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    _global: &GlobalOpts,
    args: &OptionalNameArg,
) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let name = args
        .name
        .clone()
        .or_else(|| state.active.clone())
        .ok_or(Error::NoActiveProfile)?;
    let target = keychain::profile_account(&name);

    let stale = kc
        .read(&target)
        .map_err(|_| Error::ProfileNotFound(name.clone()))?;
    let creds = OauthCreds::parse(&stale)?;
    if !creds.is_expired(Duration::from_secs(60)) && args.name.is_some() {
        eprintln!(
            "profile `{}` token still valid for {}s — refreshing anyway",
            name,
            creds.expires_in().map(|d| d.as_secs() as i64).unwrap_or(0)
        );
    }

    let _lock = CsLock::acquire(paths)?;

    let canonical = keychain::canonical_account();
    let prev_canonical = kc.read(&canonical).ok();

    // Stage stale creds in canonical so Claude Code refreshes them.
    kc.write(&canonical, &stale)?;

    if which("claude").is_none() {
        rollback_canonical(kc, &canonical, prev_canonical.as_deref());
        return Err(Error::Other(
            "`claude` CLI not on PATH; run `claude /login` for this profile manually".into(),
        ));
    }

    const REFRESH_TIMEOUT: Duration = Duration::from_secs(60);

    let mut child = match Command::new("claude")
        .args(["/status"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            rollback_canonical(kc, &canonical, prev_canonical.as_deref());
            return Err(Error::Subprocess {
                cmd: "claude /status".into(),
                message: e.to_string(),
            });
        }
    };

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if started.elapsed() > REFRESH_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    rollback_canonical(kc, &canonical, prev_canonical.as_deref());
                    return Err(Error::Subprocess {
                        cmd: "claude /status".into(),
                        message: format!("timed out after {}s", REFRESH_TIMEOUT.as_secs()),
                    });
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                rollback_canonical(kc, &canonical, prev_canonical.as_deref());
                return Err(Error::Subprocess {
                    cmd: "claude /status".into(),
                    message: e.to_string(),
                });
            }
        }
    };

    if !status.success() {
        let mut stderr = Vec::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_end(&mut stderr);
        }
        rollback_canonical(kc, &canonical, prev_canonical.as_deref());
        return Err(Error::Subprocess {
            cmd: "claude /status".into(),
            message: format!(
                "exit {}: {}",
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&stderr)
            ),
        });
    }

    let refreshed = kc.read(&canonical)?;
    if refreshed == stale {
        rollback_canonical(kc, &canonical, prev_canonical.as_deref());
        return Err(Error::Other(
            "Claude Code did not refresh the credential; run `claude /login` for this profile manually"
                .into(),
        ));
    }
    kc.write(&target, &refreshed)?;
    rollback_canonical(kc, &canonical, prev_canonical.as_deref());

    eprintln!("refreshed `{}`", name);
    Ok(())
}

fn rollback_canonical(kc: &dyn Keychain, canonical: &str, prev: Option<&[u8]>) {
    let Some(prev) = prev else { return };
    if let Err(e) = kc.write(canonical, prev) {
        eprintln!("error: could not restore canonical keychain entry {canonical}: {e}");
        tracing::error!(account = %canonical, error = %e, "canonical keychain restore failed");
    }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&path) {
        let candidate = p.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
