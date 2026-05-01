use std::fmt;
use std::fs;
use std::process::Command as ProcCommand;

use serde::Serialize;

use crate::cli::{GlobalOpts, ProviderArgs, ProviderCommand};
use crate::error::{Error, Result};
use crate::keychain::Keychain;
use crate::lock::CsLock;
use crate::output::{emit_json, emit_text, OutputOpts};
use crate::paths::Paths;
use crate::provider::{self, CodexProfileSummary, Provider};
use crate::state::State;

pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    provider_kind: Provider,
    args: &ProviderArgs,
) -> Result<()> {
    match provider_kind {
        Provider::Claude => run_claude(paths, kc, global, args),
        Provider::Codex => run_codex(paths, global, args),
    }
}

fn run_claude(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &ProviderArgs,
) -> Result<()> {
    match &args.command {
        ProviderCommand::List => super::list::run(paths, kc, global),
        ProviderCommand::Status(a) => super::status::run(paths, kc, global, a),
        ProviderCommand::Save(a) => super::save::run(paths, kc, global, a),
        ProviderCommand::Switch(a) => {
            super::switch::run_claude_only(paths, kc, global, &a.name, &[])
        }
        ProviderCommand::Refresh(a) => super::refresh::run(paths, kc, global, a),
    }
}

fn run_codex(paths: &Paths, global: &GlobalOpts, args: &ProviderArgs) -> Result<()> {
    match &args.command {
        ProviderCommand::List => list_codex(paths, global),
        ProviderCommand::Status(a) => status_codex(
            paths,
            global,
            a.name.as_deref().or(global.profile.as_deref()),
        ),
        ProviderCommand::Save(a) => save_codex(paths, global, &a.name),
        ProviderCommand::Switch(a) => switch_codex(paths, global, &a.name),
        ProviderCommand::Refresh(a) => refresh_codex(
            paths,
            global,
            a.name.as_deref().or(global.profile.as_deref()),
        ),
    }
}

#[derive(Debug, Serialize)]
struct CodexListReport {
    active: Option<String>,
    default: Option<String>,
    profiles: Vec<CodexListEntry>,
}

#[derive(Debug, Serialize)]
struct CodexListEntry {
    name: String,
    summary: CodexProfileSummary,
    is_active: bool,
    is_default: bool,
}

fn list_codex(paths: &Paths, global: &GlobalOpts) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let mut profiles: Vec<CodexListEntry> = vec![];
    let root = paths.profiles_dir();
    if root.exists() {
        for entry in fs::read_dir(&root).map_err(|e| Error::io_at(&root, e))? {
            let entry = entry.map_err(|e| Error::io_at(&root, e))?;
            if !entry
                .file_type()
                .map_err(|e| Error::io_at(entry.path(), e))?
                .is_dir()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let auth_path = paths.profile_codex_auth(&name);
            if !auth_path.exists() {
                continue;
            }
            let summary = provider::load_codex_summary(&auth_path)?;
            profiles.push(CodexListEntry {
                name: name.clone(),
                summary,
                is_active: state.active_codex.as_deref() == Some(name.as_str()),
                is_default: state.default.as_deref() == Some(name.as_str()),
            });
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    let report = CodexListReport {
        active: state.active_codex,
        default: state.default,
        profiles,
    };
    if global.json {
        emit_json(&report)
    } else {
        emit_text(
            OutputOpts {
                json: false,
                no_color: global.no_color,
            },
            &CodexListText(&report),
        )
    }
}

#[derive(Debug, Serialize)]
struct CodexStatusReport {
    active: Option<CodexStatusEntry>,
    default: Option<String>,
    previous: Option<String>,
    asked_about: Option<String>,
}

#[derive(Debug, Serialize)]
struct CodexStatusEntry {
    name: String,
    summary: CodexProfileSummary,
    is_active: bool,
    is_default: bool,
}

fn status_codex(paths: &Paths, global: &GlobalOpts, requested: Option<&str>) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let target = requested
        .map(|s| s.to_string())
        .or_else(|| state.active_codex.clone());
    let active = if let Some(name) = target {
        let path = paths.profile_codex_auth(&name);
        if !path.exists() {
            return Err(Error::ProfileNotFound(name));
        }
        Some(CodexStatusEntry {
            summary: provider::load_codex_summary(&path)?,
            is_active: state.active_codex.as_deref() == Some(name.as_str()),
            is_default: state.default.as_deref() == Some(name.as_str()),
            name,
        })
    } else {
        None
    };
    let report = CodexStatusReport {
        active,
        default: state.default,
        previous: state.previous_codex,
        asked_about: requested.map(|s| s.to_string()),
    };
    if global.json {
        emit_json(&report)
    } else {
        emit_text(
            OutputOpts {
                json: false,
                no_color: global.no_color,
            },
            &CodexStatusText(&report),
        )
    }
}

fn save_codex(paths: &Paths, global: &GlobalOpts, name: &str) -> Result<()> {
    let active_blob = provider::read_codex_active_blob(paths).map_err(|e| {
        Error::Other(format!(
            "no active Codex credential to save (run `codex login` first): {e}"
        ))
    })?;
    let _summary = serde_json::from_slice::<serde_json::Value>(&active_blob)?;
    let dst = paths.profile_codex_auth(name);
    if dst.exists() && !global.dry_run {
        return Err(Error::ProfileExists(name.to_string()));
    }
    if global.dry_run {
        eprintln!(
            "would write {} bytes -> {}",
            active_blob.len(),
            dst.display()
        );
        return Ok(());
    }
    let _lock = CsLock::acquire(paths)?;
    provider::write_codex_profile_blob(paths, name, &active_blob)?;
    eprintln!("saved codex profile `{name}` ({} bytes)", active_blob.len());
    Ok(())
}

fn switch_codex(paths: &Paths, global: &GlobalOpts, name: &str) -> Result<()> {
    let blob = provider::read_codex_profile_blob(paths, name)
        .map_err(|_| Error::ProfileNotFound(name.to_string()))?;
    let _summary = serde_json::from_slice::<serde_json::Value>(&blob)?;
    if global.dry_run {
        eprintln!("would switch codex -> {name}");
        return Ok(());
    }
    let _lock = CsLock::acquire(paths)?;
    let prev = provider::read_codex_active_blob(paths).ok();
    provider::write_codex_active_blob(paths, &blob)?;
    let verify = provider::read_codex_active_blob(paths)?;
    if verify != blob {
        if let Some(prev_bytes) = prev.as_deref() {
            let _ = provider::write_codex_active_blob(paths, prev_bytes);
        }
        return Err(Error::Other(
            "codex auth write verification failed; rolled back to previous".into(),
        ));
    }
    let state_path = paths.state_file();
    let mut state = State::load(&state_path).unwrap_or_default();
    if state.active_codex.as_deref() != Some(name) {
        state.previous_codex = state.active_codex.clone();
    }
    state.active_codex = Some(name.to_string());
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    state.switched_at_ms = Some(now_ms);
    state.since_ms = Some(now_ms);
    state.active = Some(name.to_string());
    state.previous = state.previous_codex.clone();
    state.save(&state_path)?;
    eprintln!("switched codex -> {name}");
    Ok(())
}

fn refresh_codex(paths: &Paths, global: &GlobalOpts, requested: Option<&str>) -> Result<()> {
    let state = State::load(&paths.state_file()).unwrap_or_default();
    let target = requested
        .map(|s| s.to_string())
        .or_else(|| state.active_codex.clone())
        .ok_or(Error::NoActiveProfile)?;
    let auth_path = paths.profile_codex_auth(&target);
    if !auth_path.exists() {
        return Err(Error::ProfileNotFound(target));
    }
    if global.dry_run {
        eprintln!(
            "would run `codex login status` to validate refresh for `{}`",
            target
        );
        return Ok(());
    }
    if state.active_codex.as_deref() != Some(target.as_str()) {
        switch_codex(paths, global, &target)?;
    }
    let out = ProcCommand::new("codex").args(["login", "status"]).output();
    match out {
        Ok(o) if o.status.success() => {
            eprintln!(
                "codex auth checked via `codex login status` for `{}`",
                target
            );
            Ok(())
        }
        Ok(o) => Err(Error::Subprocess {
            cmd: "codex login status".into(),
            message: format!(
                "exit {}: {}",
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stderr)
            ),
        }),
        Err(e) => Err(Error::Subprocess {
            cmd: "codex login status".into(),
            message: e.to_string(),
        }),
    }
}

struct CodexListText<'a>(&'a CodexListReport);
impl fmt::Display for CodexListText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.profiles.is_empty() {
            writeln!(f, "(no codex profiles saved)")?;
            writeln!(f, "save one with `cs codex save <name>`")?;
            return Ok(());
        }
        writeln!(
            f,
            "{:<3}{:<18}{:<12}{:<28}{:<8}",
            "", "PROFILE", "AUTH", "ACCOUNT", "REFRESH"
        )?;
        for p in &self.0.profiles {
            let mark = match (p.is_active, p.is_default) {
                (true, true) => "*D",
                (true, false) => "* ",
                (false, true) => " D",
                _ => "  ",
            };
            let auth = p.summary.auth_mode.as_deref().unwrap_or("—");
            let acct = p.summary.account_id.as_deref().unwrap_or("—");
            let refresh = if p.summary.has_refresh_token {
                "yes"
            } else {
                "no"
            };
            writeln!(
                f,
                "{:<3}{:<18}{:<12}{:<28}{:<8}",
                mark, p.name, auth, acct, refresh
            )?;
        }
        Ok(())
    }
}

struct CodexStatusText<'a>(&'a CodexStatusReport);
impl fmt::Display for CodexStatusText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.active {
            None => {
                writeln!(f, "(no active codex profile)")?;
                if let Some(d) = &self.0.default {
                    writeln!(f, "default: {d}")?;
                }
            }
            Some(s) => {
                writeln!(f, "active codex profile: {}", s.name)?;
                writeln!(
                    f,
                    "  auth mode : {}",
                    s.summary.auth_mode.as_deref().unwrap_or("unknown")
                )?;
                writeln!(
                    f,
                    "  account   : {}",
                    s.summary.account_id.as_deref().unwrap_or("—")
                )?;
                if let Some(ts) = &s.summary.last_refresh {
                    writeln!(f, "  refreshed : {ts}")?;
                }
                writeln!(
                    f,
                    "  api key   : {}",
                    if s.summary.has_api_key {
                        "present"
                    } else {
                        "absent"
                    }
                )?;
                writeln!(
                    f,
                    "  refresh   : {}",
                    if s.summary.has_refresh_token {
                        "present"
                    } else {
                        "absent"
                    }
                )?;
                if let Some(prev) = &self.0.previous {
                    writeln!(f, "  previous  : {prev}")?;
                }
            }
        }
        Ok(())
    }
}
