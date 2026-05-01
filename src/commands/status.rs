use std::fmt;

use serde::Serialize;

use crate::cli::{GlobalOpts, StatusArgs};
use crate::error::{Error, Result};
use crate::keychain::{self, Keychain};
use crate::output::{emit, emit_json, OutputOpts};
use crate::paths::Paths;
use crate::profile::{human_expiry, OauthCreds, ProfileSummary};
use crate::state::State;

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub active: Option<ProfileSummary>,
    pub default: Option<String>,
    pub previous: Option<String>,
    /// True when the requested profile is not the active one.
    pub asked_about: Option<String>,
}

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &StatusArgs) -> Result<()> {
    let report = build(
        paths,
        kc,
        args.name.as_deref().or(global.profile.as_deref()),
    )?;
    if global.json {
        emit_json(&report)?;
    } else {
        let opts = OutputOpts {
            json: false,
            no_color: global.no_color,
        };
        emit(opts, &report)?;
    }
    Ok(())
}

pub fn build(paths: &Paths, kc: &dyn Keychain, requested: Option<&str>) -> Result<StatusReport> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let target = requested
        .map(|s| s.to_string())
        .or_else(|| state.active.clone());

    let active_summary = match &target {
        Some(name) => Some(load_summary(paths, kc, &state, name)?),
        None => None,
    };

    Ok(StatusReport {
        active: active_summary,
        default: state.default.clone(),
        previous: state.previous.clone(),
        asked_about: requested.map(|s| s.to_string()),
    })
}

fn load_summary(
    paths: &Paths,
    kc: &dyn Keychain,
    state: &State,
    name: &str,
) -> Result<ProfileSummary> {
    let account = keychain::profile_account(name);
    let mut summary = match kc.read(&account) {
        Ok(bytes) => {
            let creds = OauthCreds::parse(&bytes)?;
            ProfileSummary::from_creds(name, &creds)
        }
        Err(_) => {
            if paths.profile_codex_auth(name).exists() {
                let mut s = ProfileSummary::unknown(name);
                s.providers.push("codex".to_string());
                s
            } else {
                return Err(Error::ProfileNotFound(name.to_string()));
            }
        }
    };
    if state.active.as_deref() == Some(name) {
        summary.is_active = true;
    }
    if state.default.as_deref() == Some(name) {
        summary.is_default = true;
    }
    Ok(summary)
}

impl fmt::Display for StatusReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.active {
            None => {
                writeln!(f, "(no active profile)")?;
                if let Some(d) = &self.default {
                    writeln!(f, "default: {d}  (run `cs default-go` to switch)")?;
                }
            }
            Some(p) => {
                let header = if p.is_active { "active" } else { "profile" };
                writeln!(f, "{header}: {}", p.name)?;
                if let Some(e) = &p.email {
                    writeln!(f, "  email   : {e}")?;
                }
                if let Some(plan) = &p.plan {
                    writeln!(f, "  plan    : {plan}")?;
                }
                if !p.providers.is_empty() {
                    writeln!(f, "  providers: {}", p.providers.join(","))?;
                }
                if let Some(secs) = p.expires_in_secs {
                    writeln!(
                        f,
                        "  token   : {} ({})",
                        human_expiry(secs),
                        p.expires_at.as_deref().unwrap_or("?")
                    )?;
                }
                if p.is_default {
                    writeln!(f, "  default : yes")?;
                }
                if let Some(prev) = &self.previous {
                    writeln!(f, "  previous: {prev}")?;
                }
            }
        }
        Ok(())
    }
}
