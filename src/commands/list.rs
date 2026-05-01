use std::collections::BTreeSet;
use std::fmt;
use std::fs;

use serde::Serialize;

use crate::cli::GlobalOpts;
use crate::error::Result;
use crate::keychain::{self, Keychain};
use crate::output::{emit, emit_json, OutputOpts};
use crate::paths::Paths;
use crate::profile::{human_expiry, OauthCreds, ProfileSummary};
use crate::state::State;

#[derive(Debug, Serialize)]
pub struct ListReport {
    pub active: Option<String>,
    pub default: Option<String>,
    pub profiles: Vec<ProfileSummary>,
}

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts) -> Result<()> {
    let report = build(paths, kc)?;
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

pub fn build(paths: &Paths, kc: &dyn Keychain) -> Result<ListReport> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let accounts = kc.list().unwrap_or_default();
    let mut names = BTreeSet::new();
    for account in &accounts {
        if let Some(name) = keychain::parse_profile_name(account) {
            names.insert(name.to_string());
        }
    }
    let root = paths.profiles_dir();
    if root.exists() {
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if !ft.is_dir() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if paths
                        .profile_provider_home(&name, crate::provider::Provider::Codex.as_str())
                        .exists()
                    {
                        names.insert(name);
                    }
                }
            }
        }
    }
    let mut profiles = Vec::new();
    for name in names {
        let account = keychain::profile_account(&name);
        let mut summary = match kc.read(&account) {
            Ok(bytes) => match OauthCreds::parse(&bytes) {
                Ok(creds) => ProfileSummary::from_creds(&name, &creds),
                Err(_) => ProfileSummary::unknown(&name),
            },
            Err(_) => ProfileSummary::unknown(&name),
        };
        summary.providers.clear();
        if kc.read(&account).is_ok() {
            summary.providers.push("claude".to_string());
        }
        if paths
            .profile_provider_home(&name, crate::provider::Provider::Codex.as_str())
            .exists()
        {
            summary.providers.push("codex".to_string());
        }
        profiles.push(summary);
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));

    for p in &mut profiles {
        if state.active.as_deref() == Some(&p.name) {
            p.is_active = true;
        }
        if state.default.as_deref() == Some(&p.name) {
            p.is_default = true;
        }
    }

    Ok(ListReport {
        active: state.active,
        default: state.default,
        profiles,
    })
}

impl fmt::Display for ListReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.profiles.is_empty() {
            writeln!(f, "(no profiles saved)")?;
            writeln!(
                f,
                "Save Claude with `cs save <name>` or initialize Codex with `cs codex init <name>`."
            )?;
            return Ok(());
        }
        writeln!(
            f,
            "{:<3}{:<18}{:<32}{:<10}{:<24}",
            "", "PROFILE", "EMAIL", "PLAN", "EXPIRES"
        )?;
        for p in &self.profiles {
            let mark = match (p.is_active, p.is_default) {
                (true, true) => "*D",
                (true, false) => "* ",
                (false, true) => " D",
                _ => "  ",
            };
            let email = p.email.as_deref().unwrap_or("—");
            let plan = p.plan.as_deref().unwrap_or("—");
            let expires = match p.expires_in_secs {
                Some(secs) => human_expiry(secs),
                None => "—".into(),
            };
            let providers = if p.providers.is_empty() {
                "—".to_string()
            } else {
                p.providers.join("+")
            };
            writeln!(
                f,
                "{:<3}{:<18}{:<32}{:<10}{:<24} providers={}",
                mark, p.name, email, plan, expires, providers
            )?;
        }
        if self.active.is_none() {
            writeln!(f)?;
            writeln!(f, "(no active profile — use `cs <name>` to switch)")?;
        }
        Ok(())
    }
}
