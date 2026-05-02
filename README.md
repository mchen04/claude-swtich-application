# cs — claude-switch

Sub-second switching between Claude Code accounts, with master-profile sharing of
skills/commands/agents/CLAUDE.md and a multi-account usage dashboard showing the
% of the 5-hour block and weekly cap remaining for every saved profile.

> Status: v0.1 — phases A–E (CLI surface) complete. Ratatui TUI and cwd
> auto-switch deferred to follow-up phases.

## Why

You have a personal Max plan and a work Pro plan. Today, switching means
`claude /logout`, `/login`, and waiting. `cs` swaps the macOS Keychain entry
in-place, updates per-profile `~/.claude/settings.json`, and exits in under a
second — without losing token state or per-account config.

## Install

```bash
git clone https://github.com/mchen04/claude-swtich-application
cd claude-swtich-application
cargo build --release
cp target/release/cs ~/.local/bin/   # or wherever your PATH points
```

Then run once:

```bash
cs doctor          # read-only environment audit
cs setup           # installs the shell wrapper into ~/.zshrc
```

## Quick start

```bash
# 1. log into your first account inside Claude Code
claude /login              # follow the OAuth flow

# 2. snapshot it as a named profile
cs save personal

# 3. log into a second account, save it
claude /login
cs save work

# 4. switch between them
cs personal                # sub-second swap
cs work
cs -                       # toggle to the previous profile

# 5. inspect state
cs list                    # all saved profiles, marked with active/default
cs status --json | jq      # active profile + token expiry
cs                         # multi-account dashboard: % of 5h + weekly cap left
```

## Multi-account dashboard

`cs` (no args) and `cs usage` both render a single table with one row per
saved Claude profile. The numbers come straight from `/api/oauth/usage` —
the same endpoint that backs Claude Code's `/usage` slash command — using
each profile's saved OAuth token. Responses are cached on disk for 300s
to stay clear of the endpoint's aggressive 429s.

```
   PROFILE          5H LEFT   5H RESETS    7D LEFT   7D RESETS    PLAN
*  work             63%       2h14m        36%       3d04h        max
   personal         91%       4h51m        88%       6d22h        pro
   research         —         —            —         —            max     ↳ token expired — `cs refresh research`
```

- **5H LEFT** — `100 − utilization` from the rolling 5-hour bucket.
- **5H RESETS** — countdown to that bucket's reset.
- **7D LEFT** — same, for the weekly cap.
- **7D RESETS** — countdown to the weekly reset (days+hours past 24h).
- **PLAN** — subscription plan from the OAuth blob (max / pro / team / —).
- `*` marks the active profile; rows sort active first, then alphabetically.
- A row with no live data shows `—` and a hint (`token expired — \`cs refresh <name>\``,
  or `rate-limited` when the endpoint keeps 429-ing past the on-disk cache).

`cs usage --watch` repaints the same table every second. The 300s limits
cache means watch reuses cached % values 99% of the time and only re-fetches
when the cache expires; reset countdowns recompute every tick. `cs usage --json`
emits the report struct for scripting.

## Master profile (shared config)

Anything in `~/.claude/{skills,commands,agents,CLAUDE.md}` is duplicated across
accounts by default. Designate one of your saved profiles as **master** and the
four candidates move into that profile's directory; every other profile picks
them up via the same symlinks.

```bash
cs master personal         # designate `personal` as master
                           # moves ~/.claude/{skills,commands,agents,CLAUDE.md}
                           # into ~/.claude-cs/profiles/personal/ and symlinks
                           # them back into ~/.claude/.
cs master                  # status: which profile is master, per-item state
cs master work             # change master to `work` (refuses if work already
                           # has any of the four candidates)
cs master --unset          # clear the designation; move content back to ~/.claude
cs uninstall               # rolls back to plain ~/.claude — byte-identical
```

`uninstall` is byte-clean by design — verified by an integration test that
diffs the directory snapshot before/after.

## Commands

```
cs                         multi-account usage dashboard (default with no args)
cs <profile>               switch to <profile>
cs <profile> -- claude …   switch then exec claude with passthrough args
cs -                       switch to previous profile

cs list                    list saved profiles
cs status [<profile>]      active profile details (token expiry, plan)
cs save <name>             save canonical Claude Code creds as a profile
cs rm <name>               remove a saved profile
cs rename <from> <to>      rename a saved profile
cs default <name>          set the default profile
cs default-go              switch to the default profile
cs refresh [<profile>]     refresh OAuth via `claude /status` delegation

cs usage                   per-profile % of 5h + weekly cap remaining
cs usage --watch           live updates every 1s (cache holds for 300s)
cs usage --json            emit the report as JSON

cs run <name> -- <args>    launch claude in an isolated per-profile home
cs shell <name>            enter a shell with CLAUDE_CONFIG_DIR exported

cs master                  show master designation + per-item symlink state
cs master <name>           designate <name> as master (or change master)
cs master --unset          clear master designation; restore ~/.claude

cs link [<profile>]        bind cwd → profile (auto-switch in v0.2)
cs links                   list all cwd bindings

cs setup [--shell zsh|bash]   install/repair the shell wrapper
cs alias <name>               add `alias <name>='cs <name>'`
cs migrate [--from <path>]    inspect a legacy claude-switch config

cs doctor                  read-only health check
cs uninstall [--keep-master]  remove cs (symlinks, wrapper)

Global flags: --json --no-color --dry-run --profile <name> -v / -vv
```

## Layout

```
~/.claude/                          # canonical Claude Code home
├── settings.json                   # rewritten on switch from per-profile copy
├── .active-profile                 # tracking marker for compat
├── skills/  → ~/.claude-cs/profiles/<master>/skills/   (after `cs master <name>`)
├── commands/ → ~/.claude-cs/profiles/<master>/commands/
├── agents/   → ~/.claude-cs/profiles/<master>/agents/
└── CLAUDE.md → ~/.claude-cs/profiles/<master>/CLAUDE.md

~/.claude-cs/                       # cs's home
├── profiles/<name>/
│   ├── settings.json               # per-profile (replaces canonical on switch)
│   ├── env                         # KEY=VAL pairs sourced post-switch
│   ├── providers/claude/home/      # isolated CLAUDE_CONFIG_DIR for `cs run`/`cs shell`
│   └── skills/, commands/, …       # only on the profile designated as master
├── state.json                      # {active, previous, default, master, switched_at}
├── links.json                      # cwd → profile bindings
└── .backups/<ts>/manifest.json     # every destructive op is reversible
```

Keychain entries:

- `Claude Code-credentials` / `acct=$USER` — Claude Code's canonical entry (read/written by `cs` on switch)
- `Claude Code-credentials` / `acct=Claude Code-credentials-<name>` — saved profile

## Safety

- Every destructive operation acquires `~/.claude-cs/.lock` (advisory `flock`)
  to prevent two `cs` invocations from clobbering each other.
- Every Keychain write is verified byte-equal and rolled back on mismatch.
- Every destructive op writes `~/.claude-cs/.backups/<ts>/manifest.json`
  recording the before/after blobs (base64) so a future `cs revert <ts>` can
  replay in reverse.
- `--dry-run` is supported on every mutation and prints the planned actions.

## Dev

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Test seam: every filesystem and Keychain access is routed through `Paths` and
the `Keychain` trait so tests can inject a tmpdir + a JSON-backed mock Keychain
via `CS_TEST_KEYCHAIN=1` and `CS_TEST_KEYCHAIN_FIXTURE=/path/to/fixture.json`.
For per-profile usage limits fixtures, point `CS_TEST_LIMITS_FIXTURE=/dir` at a
directory containing `<profile>.json` (matching the `/api/oauth/usage` response
shape). `CS_TEST_LIMITS_FAIL=/dir` (file `<profile>.txt` with `expired`,
`rate_limited`, or `http`) exercises error paths without networking.

## Status / roadmap

- [x] Phase A — skeleton, `cs doctor`, env probes
- [x] Phase B — `cs list`, `cs status` (text + JSON)
- [x] Phase C — save/rm/rename/default/switch/`-`/refresh/setup/alias/migrate
- [x] Phase D — master profile designation + uninstall (byte-clean roundtrip)
- [x] Phase E — multi-account % dashboard via `/api/oauth/usage`, `cs link`/`cs links`
- [ ] Phase F — Ratatui TUI (stub removed; deferred)
- [ ] Phase G — cwd auto-switch precmd hook, expiry/quota notifications, `cs audit`, `cs revert`
- [ ] Phase H — `cs export`/`cs import` with `age`, Linux secret-service backend, brew tap

## Acknowledgements

Usage data comes from Anthropic's `/api/oauth/usage` endpoint — the same one
that backs Claude Code's `/usage` slash command.
