mod backup;
mod cli;
mod commands;
mod dashboard;
mod doctor;
mod dryrun;
mod error;
mod keychain;
mod links;
mod lock;
mod logging;
mod master;
mod output;
mod paths;
mod profile;
mod shell;
mod state;
mod symlinks;
mod usage;

use clap::Parser;

use crate::cli::{Cli, Command, KNOWN_SUBCOMMANDS};
use crate::error::Result;
use crate::keychain::Keychain;
use crate::paths::Paths;

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    let rewritten = rewrite_bare_invocation(raw);
    run_with_args(rewritten);
}

fn run_with_args(args: Vec<String>) {
    let cli = match Cli::try_parse_from(&args) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    crate::logging::init(cli.global.verbose);

    let paths = match Paths::from_env() {
        Ok(p) => p,
        Err(e) => die(e),
    };
    let kc: Box<dyn Keychain> = crate::keychain::default_keychain();

    if let Err(e) = dispatch(&paths, &*kc, &cli) {
        die(e);
    }
}

fn dispatch(paths: &Paths, kc: &dyn Keychain, cli: &Cli) -> Result<()> {
    match &cli.command {
        None => commands::dashboard::run(paths, kc, &cli.global),
        Some(Command::Doctor(a)) => commands::doctor::run(paths, kc, &cli.global, a),
        Some(Command::List) => commands::list::run(paths, kc, &cli.global),
        Some(Command::Status(a)) => commands::status::run(paths, kc, &cli.global, a),
        Some(Command::Usage(a)) => commands::usage::run(paths, &cli.global, a),
        Some(Command::Dashboard) => commands::dashboard::run(paths, kc, &cli.global),
        Some(Command::Save(a)) => commands::save::run(paths, kc, &cli.global, a),
        Some(Command::Rm(a)) => commands::rm::run(paths, kc, &cli.global, a),
        Some(Command::Rename(a)) => commands::rename::run(paths, kc, &cli.global, a),
        Some(Command::Default(a)) => commands::default::set(paths, kc, &cli.global, a),
        Some(Command::DefaultGo) => commands::default::go(paths, kc, &cli.global),
        Some(Command::Refresh(a)) => commands::refresh::run(paths, kc, &cli.global, a),
        Some(Command::Setup(a)) => commands::setup::run(paths, &cli.global, a),
        Some(Command::Alias(a)) => commands::alias::run(paths, &cli.global, a),
        Some(Command::Migrate(a)) => commands::migrate::run(paths, kc, &cli.global, a),
        Some(Command::Master(c)) => commands::master::run(paths, &cli.global, c),
        Some(Command::Override(a)) => commands::override_::add(paths, &cli.global, a),
        Some(Command::Unoverride(a)) => commands::override_::drop(paths, &cli.global, a),
        Some(Command::ShareSkill(a)) => commands::share_skill::run(paths, &cli.global, a),
        Some(Command::Link(a)) => commands::link::link(paths, &cli.global, a),
        Some(Command::Links) => commands::link::list(paths, &cli.global),
        Some(Command::Uninstall(a)) => commands::uninstall::run(paths, &cli.global, a),
        Some(Command::Tui) => commands::tui::run(),
        Some(Command::Switch(a)) => {
            commands::switch::run(paths, kc, &cli.global, &a.name, &a.passthrough)
        }
        Some(Command::SwitchPrevious(a)) => {
            commands::switch::run_previous(paths, kc, &cli.global, &a.passthrough)
        }
        Some(Command::WrapperEmitEnv(a)) => commands::wrapper::emit_env(paths, kc, &cli.global, a),
    }
}

/// Rewrite `cs <name> [args...]` and `cs -` into the hidden internal subcommands so
/// clap parses them correctly. We only rewrite when the first positional is *not* a
/// known subcommand. Global flags (`--json`, `-v`, etc.) before the positional are
/// preserved in place.
fn rewrite_bare_invocation(raw: Vec<String>) -> Vec<String> {
    if raw.len() < 2 {
        return raw;
    }
    // Find the first non-flag argument.
    let mut idx: Option<usize> = None;
    let mut i = 1;
    while i < raw.len() {
        let arg = &raw[i];
        if arg == "--" {
            // Bare `cs -- ...` is a syntax error; let clap report it.
            return raw;
        }
        if arg.starts_with("--") {
            // long flag — may carry an inline value (--profile=foo) or a separate one
            if !arg.contains('=') && expects_value_long(arg) && i + 1 < raw.len() {
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if arg.len() > 1 && arg.starts_with('-') && arg != "-" {
            // short flag bundle like -vv. Doesn't take a value in our CLI.
            i += 1;
            continue;
        }
        idx = Some(i);
        break;
    }
    let Some(idx) = idx else { return raw };
    let candidate = &raw[idx];

    if candidate == "-" {
        let mut new = raw[..idx].to_vec();
        new.push("__switch-previous".into());
        new.extend(raw[idx + 1..].iter().cloned());
        return new;
    }
    if KNOWN_SUBCOMMANDS.iter().any(|s| s == candidate) {
        return raw;
    }
    // `cs <name> [args...]` → `cs __switch <name> [args...]`
    let mut new = raw[..idx].to_vec();
    new.push("__switch".into());
    new.extend(raw[idx..].iter().cloned());
    new
}

fn expects_value_long(arg: &str) -> bool {
    // The only global flag taking a value is --profile.
    matches!(arg, "--profile")
}

fn die(e: error::Error) -> ! {
    eprintln!("error: {e}");
    let mut src: Option<&dyn std::error::Error> = std::error::Error::source(&e);
    while let Some(s) = src {
        eprintln!("  caused by: {s}");
        src = s.source();
    }
    std::process::exit(1);
}
