use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "cs",
    version,
    about = "Claude profile switcher",
    long_about = "Sub-second Claude profile switching, master-profile sharing of \
                  skills/commands/agents/CLAUDE.md, and a per-profile usage dashboard \
                  showing % of the 5-hour block and weekly cap remaining.",
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Args, Clone, Default)]
pub struct GlobalOpts {
    /// Emit machine-readable JSON instead of text where supported.
    #[arg(long, global = true)]
    pub json: bool,

    /// Operate against an explicit profile (overrides active).
    #[arg(long = "profile", global = true)]
    pub profile: Option<String>,

    /// Increase log verbosity (-v info, -vv debug+trace).
    #[arg(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run read-only health checks against the local environment.
    Doctor(DoctorArgs),
    /// List saved profiles.
    List,
    /// Show active profile (or named profile) details.
    Status(StatusArgs),
    /// Show per-profile % of 5h block and weekly cap remaining (mirrors `/usage`).
    Usage(UsageArgs),
    /// Snapshot the current Claude Code Keychain entry into a named profile.
    Save(SaveArgs),
    /// Remove a saved profile.
    Rm(NameArg),
    /// Rename a saved profile.
    Rename(RenameArgs),
    /// Set the default profile (used when no argument is passed).
    Default(NameArg),
    /// Switch to the default profile.
    DefaultGo,
    /// Force-refresh OAuth credentials for a profile.
    Refresh(OptionalNameArg),
    /// Install or repair the shell wrapper.
    Setup(SetupArgs),
    /// Designate a profile as master, show status, or clear the designation.
    Master(MasterArgs),
    /// Remove cs from the system (symlinks, wrapper, optionally master).
    Uninstall(UninstallArgs),
    /// Toggle auto-switching when the active profile hits its 5h or 7d cap.
    #[command(name = "auto-switch")]
    AutoSwitch(AutoSwitchArgs),

    /// Hidden: invoked by main.rs after rewriting `cs <name> [-- args...]`.
    #[command(name = "__switch", hide = true)]
    Switch(SwitchArgs),
    /// Hidden: invoked by main.rs after rewriting `cs -`.
    #[command(name = "__switch-previous", hide = true)]
    SwitchPrevious(PassthroughArgs),
    /// Hidden helper used by the shell wrapper.
    #[command(name = "__wrapper-emit-env", hide = true)]
    WrapperEmitEnv(NameArg),
    /// Hidden: launchd-driven tick that swaps profiles when the active one caps.
    #[command(name = "__autoswitch-tick", hide = true)]
    AutoswitchTick,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Attempt safe automatic repairs.
    #[arg(long)]
    pub fix: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Profile to inspect (defaults to active).
    pub name: Option<String>,
}

#[derive(Debug, Args, Default)]
pub struct UsageArgs {
    /// Update the display continuously (~1s cadence). Cached limits stay reused; only
    /// the reset countdowns recompute every tick.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Debug, Args)]
pub struct SaveArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct NameArg {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct OptionalNameArg {
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct RenameArgs {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Shell to configure.
    #[arg(long, value_enum, default_value_t = ShellChoice::Auto)]
    pub shell: ShellChoice,
}

#[derive(Debug, Args)]
pub struct MasterArgs {
    /// Profile to designate as master. Omit to print status.
    #[arg(conflicts_with = "unset")]
    pub name: Option<String>,
    /// Clear the master designation; move shared content back to ~/.claude.
    #[arg(long, conflicts_with = "name")]
    pub unset: bool,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Leave the master directory in place.
    #[arg(long)]
    pub keep_master: bool,
}

#[derive(Debug, Args)]
pub struct AutoSwitchArgs {
    /// `on` to enable, `off` to disable. Omit to print current status.
    #[arg(value_enum)]
    pub mode: Option<OnOff>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

#[derive(Debug, Args)]
pub struct SwitchArgs {
    pub name: String,
    /// Args to pass to `claude` after switching (after `--`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

#[derive(Debug, Args)]
pub struct PassthroughArgs {
    /// Args to pass to `claude` after switching (after `--`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ShellChoice {
    Auto,
    Zsh,
    Bash,
}

pub const KNOWN_SUBCOMMANDS: &[&str] = &[
    "doctor",
    "list",
    "status",
    "usage",
    "save",
    "rm",
    "rename",
    "default",
    "default-go",
    "refresh",
    "setup",
    "master",
    "uninstall",
    "auto-switch",
    "help",
    "__switch",
    "__switch-previous",
    "__wrapper-emit-env",
    "__autoswitch-tick",
];
