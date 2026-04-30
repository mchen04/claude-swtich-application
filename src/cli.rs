use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "cs",
    version,
    about = "Claude Code account switcher",
    long_about = "Sub-second Claude Code account switching with master-profile sharing of \
                  skills/commands/agents/CLAUDE.md and a live usage dashboard.",
    propagate_version = true,
    disable_help_subcommand = true,
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

    /// Disable ANSI color in text output.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Print planned actions without executing them.
    #[arg(long, global = true)]
    pub dry_run: bool,

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
    /// Show usage for the current block / day / month.
    Usage(UsageArgs),
    /// Print a one-shot dashboard snapshot.
    Dashboard,
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
    /// Create a shell alias for a profile.
    Alias(AliasArgs),
    /// Migrate from the legacy claude-switch tool.
    Migrate(MigrateArgs),
    /// Manage the master profile (shared skills/commands/agents/CLAUDE.md).
    #[command(subcommand)]
    Master(MasterCmd),
    /// Override a master path for the current profile.
    Override(OverrideArgs),
    /// Drop an override; restore the master symlink.
    Unoverride(OverrideArgs),
    /// Promote a profile-local skill into the master.
    ShareSkill(NameArg),
    /// Bind the current working directory to a profile.
    Link(LinkArgs),
    /// Show all cwd→profile bindings.
    Links,
    /// Remove cs from the system (symlinks, wrapper, optionally master).
    Uninstall(UninstallArgs),
    /// Launch the Ratatui usage TUI.
    Tui,

    /// Hidden: invoked by main.rs after rewriting `cs <name> [-- args...]`.
    #[command(name = "__switch", hide = true)]
    Switch(SwitchArgs),
    /// Hidden: invoked by main.rs after rewriting `cs -`.
    #[command(name = "__switch-previous", hide = true)]
    SwitchPrevious(PassthroughArgs),
    /// Hidden helper used by the shell wrapper.
    #[command(name = "__wrapper-emit-env", hide = true)]
    WrapperEmitEnv(NameArg),
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

#[derive(Debug, Args)]
pub struct UsageArgs {
    /// Update the display continuously.
    #[arg(long)]
    pub watch: bool,
    /// Show 5-hour blocks (default if no other flag).
    #[arg(long, conflicts_with_all = ["daily", "monthly"])]
    pub blocks: bool,
    /// Show daily totals.
    #[arg(long, conflicts_with_all = ["blocks", "monthly"])]
    pub daily: bool,
    /// Show monthly totals.
    #[arg(long, conflicts_with_all = ["blocks", "daily"])]
    pub monthly: bool,
}

#[derive(Debug, Args)]
pub struct SaveArgs {
    pub name: String,
    /// Snapshot from the currently-active Keychain entry (default).
    #[arg(long, default_value_t = true)]
    pub from_active: bool,
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
    #[arg(long)]
    pub non_interactive: bool,
}

#[derive(Debug, Args)]
pub struct AliasArgs {
    pub name: String,
    #[arg(long, value_enum, default_value_t = ShellChoice::Auto)]
    pub shell: ShellChoice,
}

#[derive(Debug, Args)]
pub struct MigrateArgs {
    /// Path to the legacy claude-switch config (optional).
    #[arg(long = "from")]
    pub from: Option<std::path::PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum MasterCmd {
    /// Move ~/.claude shared content under cs's master and symlink it back.
    Init,
    /// Print the master symlink status.
    Status,
}

#[derive(Debug, Args)]
pub struct OverrideArgs {
    pub profile: String,
    /// Path within the master to override (e.g. skills/foo).
    pub path: String,
}

#[derive(Debug, Args)]
pub struct LinkArgs {
    /// Profile to bind (defaults to the active profile).
    pub profile: Option<String>,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Leave the master directory in place.
    #[arg(long)]
    pub keep_master: bool,
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
    "dashboard",
    "save",
    "rm",
    "rename",
    "default",
    "default-go",
    "refresh",
    "setup",
    "alias",
    "migrate",
    "master",
    "override",
    "unoverride",
    "share-skill",
    "link",
    "links",
    "uninstall",
    "tui",
    "help",
    "__switch",
    "__switch-previous",
    "__wrapper-emit-env",
];
