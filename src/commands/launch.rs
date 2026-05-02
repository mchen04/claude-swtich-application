use std::fmt;
use std::os::unix::process::CommandExt;
use std::process::Command;

use serde::Serialize;

use crate::cli::{GlobalOpts, RunArgs, ShellArgs};
use crate::dryrun::{Action, Plan};
use crate::error::{Error, Result};
use crate::isolation::{self, LaunchSpec};
use crate::keychain::Keychain;
use crate::output::{emit, emit_json, emit_text, OutputOpts};
use crate::paths::Paths;

pub fn run_run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &RunArgs,
) -> Result<()> {
    if global.dry_run {
        let env = isolation::preview_env_for_claude(paths, kc, &args.name)?;
        let mut plan = Plan::new();
        push_env_notes(&mut plan, &env);
        plan.push(Action::SpawnProcess {
            cmd: "claude".into(),
            args: args.args.clone(),
        });
        return emit(
            OutputOpts {
                json: global.json,
            },
            &plan,
        );
    }
    let spec = isolation::build_claude_launch(paths, kc, &args.name, args.args.clone())?;
    exec_spec(spec)
}

pub fn run_shell(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &ShellArgs,
) -> Result<()> {
    let env = if args.print_env || global.dry_run {
        isolation::preview_env_for_claude(paths, kc, &args.name)?
    } else {
        isolation::env_for_claude(paths, kc, &args.name)?
    };

    if args.print_env {
        let report = ShellEnvReport { exports: env };
        if global.json {
            return emit_json(&report);
        }
        return emit_text(OutputOpts { json: false }, &report);
    }

    if global.dry_run {
        let mut plan = Plan::new();
        push_env_notes(&mut plan, &env);
        let shell = detect_shell()?;
        plan.push(Action::SpawnProcess {
            cmd: shell,
            args: vec!["-i".to_string()],
        });
        return emit(
            OutputOpts {
                json: global.json,
            },
            &plan,
        );
    }

    let shell = detect_shell()?;
    let err = Command::new(&shell)
        .arg("-i")
        .envs(env.iter().cloned())
        .exec();
    Err(Error::Subprocess {
        cmd: shell,
        message: err.to_string(),
    })
}

fn exec_spec(spec: LaunchSpec) -> Result<()> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args).envs(spec.env.iter().cloned());
    let err = cmd.exec();
    Err(Error::Subprocess {
        cmd: spec.program,
        message: err.to_string(),
    })
}

fn detect_shell() -> Result<String> {
    std::env::var("SHELL").map_err(|_| Error::Config("could not detect shell from $SHELL".into()))
}

fn push_env_notes(plan: &mut Plan, env: &[(String, String)]) {
    for (key, value) in env {
        plan.push(Action::Note {
            message: format!("export {key}={value}"),
        });
    }
}

#[derive(Debug, Serialize)]
struct ShellEnvReport {
    exports: Vec<(String, String)>,
}

impl fmt::Display for ShellEnvReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.exports {
            writeln!(f, "export {key}={}", shell_quote(value))?;
        }
        Ok(())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
