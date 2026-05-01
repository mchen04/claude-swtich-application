use std::fmt;
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::Command as ProcCommand;

use serde::Serialize;

use crate::cli::{
    ClaudeArgs, ClaudeCommand, CodexArgs, CodexCommand, GlobalOpts, NameArg, StatusArgs,
};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::isolation;
use crate::keychain::Keychain;
use crate::lock::CsLock;
use crate::output::{emit, emit_json, emit_text, OutputOpts};
use crate::paths::Paths;
use crate::provider::{self, CodexProfileSummary, Provider};

pub fn run_claude(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &ClaudeArgs,
) -> Result<()> {
    match &args.command {
        ClaudeCommand::List => super::list::run(paths, kc, global),
        ClaudeCommand::Status(a) => super::status::run(paths, kc, global, a),
        ClaudeCommand::Save(a) => super::save::run(paths, kc, global, a),
        ClaudeCommand::Switch(a) => super::switch::run_claude_only(paths, kc, global, &a.name, &[]),
        ClaudeCommand::Run(a) => {
            super::launch::run_provider_args(paths, kc, global, Provider::Claude, a)
        }
        ClaudeCommand::Shell(a) => {
            super::launch::shell_provider(paths, kc, global, Provider::Claude, a)
        }
        ClaudeCommand::Refresh(a) => super::refresh::run(paths, kc, global, a),
    }
}

pub fn run_codex(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &CodexArgs,
) -> Result<()> {
    match &args.command {
        CodexCommand::List => list_codex(paths, global),
        CodexCommand::Status(a) => status_codex(paths, global, a),
        CodexCommand::Init(a) => init_codex(paths, kc, global, a),
        CodexCommand::Login(a) => login_codex(paths, kc, global, a),
        CodexCommand::Run(a) => {
            super::launch::run_provider_args(paths, kc, global, Provider::Codex, a)
        }
        CodexCommand::Shell(a) => {
            super::launch::shell_provider(paths, kc, global, Provider::Codex, a)
        }
    }
}

#[derive(Debug, Serialize)]
struct CodexListReport {
    profiles: Vec<CodexListEntry>,
}

#[derive(Debug, Serialize)]
struct CodexListEntry {
    name: String,
    initialized: bool,
    has_auth: bool,
    summary: Option<CodexProfileSummary>,
}

fn list_codex(paths: &Paths, global: &GlobalOpts) -> Result<()> {
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
            let home = paths.profile_provider_home(&name, Provider::Codex.as_str());
            if !home.exists() {
                continue;
            }
            let auth_path = paths.profile_codex_auth(&name);
            let summary = if auth_path.exists() {
                Some(provider::load_codex_summary(&auth_path)?)
            } else {
                None
            };
            profiles.push(CodexListEntry {
                name,
                initialized: true,
                has_auth: summary.is_some(),
                summary,
            });
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    let report = CodexListReport { profiles };
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
    name: String,
    initialized: bool,
    has_auth: bool,
    home: String,
    summary: Option<CodexProfileSummary>,
}

fn status_codex(paths: &Paths, global: &GlobalOpts, args: &StatusArgs) -> Result<()> {
    let name = args
        .name
        .clone()
        .or(global.profile.clone())
        .ok_or_else(|| Error::Other("profile name required: `cs codex status <name>`".into()))?;
    let home = paths.profile_provider_home(&name, Provider::Codex.as_str());
    if !home.exists() {
        return Err(Error::ProfileNotFound(name));
    }
    let auth_path = paths.profile_codex_auth(&name);
    let summary = if auth_path.exists() {
        Some(provider::load_codex_summary(&auth_path)?)
    } else {
        None
    };
    let report = CodexStatusReport {
        name,
        initialized: true,
        has_auth: summary.is_some(),
        home: home.display().to_string(),
        summary,
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

fn init_codex(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts, args: &NameArg) -> Result<()> {
    if global.dry_run {
        let env = isolation::preview_env_for_provider(paths, kc, Provider::Codex, &args.name)?;
        let mut plan = Plan::new();
        for (key, value) in env {
            plan.push(Action::Note {
                message: format!("export {key}={value}"),
            });
        }
        return emit(
            OutputOpts {
                json: global.json,
                no_color: global.no_color,
            },
            &plan,
        );
    }

    let _lock = CsLock::acquire(paths)?;
    let home = isolation::ensure_codex_home(paths, &args.name)?;
    eprintln!(
        "initialized codex profile `{}` at {}",
        args.name,
        home.display()
    );
    Ok(())
}

fn login_codex(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &NameArg,
) -> Result<()> {
    let env = if global.dry_run {
        isolation::preview_env_for_provider(paths, kc, Provider::Codex, &args.name)?
    } else {
        let _lock = CsLock::acquire(paths)?;
        isolation::env_for_provider(paths, kc, Provider::Codex, &args.name)?
    };
    if global.dry_run {
        let mut plan = Plan::new();
        for (key, value) in &env {
            plan.push(Action::Note {
                message: format!("export {key}={value}"),
            });
        }
        plan.push(Action::SpawnProcess {
            cmd: "codex".into(),
            args: vec!["login".into()],
        });
        return emit(
            OutputOpts {
                json: global.json,
                no_color: global.no_color,
            },
            &plan,
        );
    }

    let err = ProcCommand::new("codex").arg("login").envs(env).exec();
    Err(Error::Subprocess {
        cmd: "codex login".into(),
        message: err.to_string(),
    })
}

struct CodexListText<'a>(&'a CodexListReport);

impl fmt::Display for CodexListText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.profiles.is_empty() {
            writeln!(f, "(no codex profiles initialized)")?;
            writeln!(f, "create one with `cs codex init <name>`")?;
            return Ok(());
        }
        writeln!(f, "{:<18}{:<8}{:<36}REFRESH", "PROFILE", "AUTH", "ACCOUNT")?;
        for profile in &self.0.profiles {
            let (auth, account, refresh) = match &profile.summary {
                Some(summary) => (
                    summary.auth_mode.as_deref().unwrap_or("yes"),
                    summary.account_id.as_deref().unwrap_or("—"),
                    if summary.has_refresh_token {
                        "yes"
                    } else {
                        "no"
                    },
                ),
                None => ("no", "—", "—"),
            };
            writeln!(
                f,
                "{:<18}{:<8}{:<36}{}",
                profile.name, auth, account, refresh
            )?;
        }
        Ok(())
    }
}

struct CodexStatusText<'a>(&'a CodexStatusReport);

impl fmt::Display for CodexStatusText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "codex profile: {}", self.0.name)?;
        writeln!(f, "  home      : {}", self.0.home)?;
        writeln!(
            f,
            "  auth      : {}",
            if self.0.has_auth {
                "present"
            } else {
                "missing (run `cs codex login <name>`)"
            }
        )?;
        if let Some(summary) = &self.0.summary {
            if let Some(mode) = &summary.auth_mode {
                writeln!(f, "  mode      : {mode}")?;
            }
            if let Some(account) = &summary.account_id {
                writeln!(f, "  account   : {account}")?;
            }
            writeln!(
                f,
                "  refresh   : {}",
                if summary.has_refresh_token {
                    "yes"
                } else {
                    "no"
                }
            )?;
        }
        Ok(())
    }
}
