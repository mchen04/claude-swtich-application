# cs — Spec

One CLI to swap Claude Code accounts in under a second **and** a multi-account dashboard for usage + token health across every saved profile, with a master-profile model so skills/commands/agents are shared across accounts instead of duplicated.

**Name:** `cs`. Long form: `claude-switch`.

---

## 1. Problem

Power users run several Claude Code accounts (personal Pro, work Max, side projects, research). Today:

- Switching means logout/login — slow, breaks flow.
- The reference `claude-switch` (bash + Keychain) handles the swap but offers no live view, no auto-refresh, and no shared config across profiles.
- Skills, slash-commands, agents, and `CLAUDE.md` live under `~/.claude/` and follow whichever account last logged in. There's no notion of "this skill is *mine*, not *this account's*."
- The 5-hour rolling quota, daily message counts, and OAuth expiry are only visible *inside* a Claude session via `/usage`. You can't see them across accounts at a glance.

## 2. Goals

1. **Sub-second account switch** from any shell, including from inside an active Claude session.
2. **Live TUI** streaming usage (messages, tokens, 5-hour window, expiry) for the active account, refreshing every ~1s without flicker.
3. **Master profile** — one source of truth for skills/commands/agents/CLAUDE.md; profiles inherit by symlink.
4. **Per-profile overrides** — `settings.json`, MCP servers, env vars can diverge per profile.
5. **Drop-in compatible** with the existing `claude-switch` Keychain layout so users migrate without re-saving profiles.

## 3. Non-Goals

- Replacing the Anthropic Console / billing dashboard.
- Multi-tenant team sharing.
- Windows in v1 (macOS first; Linux secret-service in v2).
- Modifying Claude Code internals — we only read its on-disk state and rotate Keychain entries.

## 4. What `claude-switch` already does

| Concern | claude-switch (bash) |
|---|---|
| Active credentials | macOS Keychain `Claude Code-credentials` (JSON: `claudeAiOauth.{accessToken, refreshToken, expiresAt, subscriptionType, email}`) |
| Saved profile | Keychain `Claude Code-credentials-<profile>` |
| Active profile pointer | `~/.claude/.active-profile` |
| Config | `~/.config/claude-switch/config` |
| Token health | `expiresAt` epoch ms → "Xh left" / "expired" |
| Refresh | Calls Claude CLI auth flow on expiry |
| Switch | Overwrites canonical Keychain entry from saved one |

We **keep this layout** for compatibility and add on top.

## 5. File model: master, per-profile, machine-local

Three categories, named explicitly so we never swap the wrong thing.

**Master (shared, symlinked into `~/.claude/`):** `skills/`, `commands/`, `agents/`, `CLAUDE.md`. One of your saved profiles is designated the **master profile**; the four shared paths live inside that profile's directory and are symlinked into `~/.claude/`. Other profiles pick them up via the same symlinks. A missing `agents/` is fine.

**Per-profile (swapped on switch):** `settings.json` (which already carries `enabledPlugins`) and an optional `env` file sourced into the shell. We do **not** swap the entire `plugins/` directory — plugin enablement lives in `settings.json`; the cached repos under `plugins/` are content-addressed and harmless to share.

**Machine-local (never touched):** `settings.local.json` (permissions, MCP enables), and runtime/cache dirs — `telemetry/`, `statsig/`, `todos/`, `tasks/`, `sessions/`, `session-env/`, `shell-snapshots/`, `paste-cache/`, `file-history/`, `history.jsonl`, `policy-limits.json`, `projects/` (see §15 Q3). These follow the machine, not the account.

```
~/.claude/                  ← what Claude Code reads (path unchanged)
├── skills/      → ~/.claude-cs/profiles/<master>/skills/      (symlink)
├── commands/    → ~/.claude-cs/profiles/<master>/commands/    (symlink)
├── agents/      → ~/.claude-cs/profiles/<master>/agents/      (symlink, if present)
├── CLAUDE.md    → ~/.claude-cs/profiles/<master>/CLAUDE.md    (symlink)
├── settings.json                                              ← per-profile, swapped
├── settings.local.json                                        ← machine-local, untouched
└── (runtime dirs)                                             ← machine-local, untouched

~/.claude-cs/
├── profiles/
│   ├── <master>/        ← skills/, commands/, agents/, CLAUDE.md, settings.json, env
│   └── <other>/         ← settings.json, env (inherits the four via master's symlinks)
└── state.json           ← {active, previous, default, master, …}
```

**First run:** `cs master <name>` moves existing master-eligible content under `~/.claude/` into `~/.claude-cs/profiles/<name>/` and replaces the originals with symlinks. Reversible via `cs master --unset` or `cs uninstall`.

## 6. CLI surface

`cs` with no args opens the TUI.

**Switching**
- `cs` — TUI dashboard
- `cs <profile>` — switch and exit
- `cs <profile> -- <claude args>` — switch then launch `claude` with passthrough flags
- `cs -` — toggle to previous profile

**Profiles**
- `cs save <name>` — snapshot current creds + per-profile files
- `cs list` — table: name, email, plan, expiry, last-used
- `cs rm <name>` / `cs rename <old> <new>`
- `cs export <name>` / `cs import <file>` — age-encrypted bundle for moving across machines
- `cs default <name>` — set the profile used by `cs default-go` (renamed for clarity; original `claude-switch` overloaded `default`)

**Master profile**
- `cs master <name>` — designate `<name>` as master (or change master)
- `cs master` — show status (which profile is master, per-item symlink state)
- `cs master --unset` — clear designation; move shared content back to `~/.claude`

**Usage**
- `cs status` — one-shot snapshot for scripts
- `cs usage` — multi-account dashboard: one row per saved Claude profile with
  % of the rolling 5-hour block and weekly cap remaining, plus reset countdowns
  and plan. Mirrors what Claude Code's `/usage` slash command shows.
  `--watch` repaints every 1s.
- `cs refresh [profile]` — force OAuth refresh
- `cs doctor` — Keychain access, symlink integrity, `claude` on PATH, clock skew

**Setup**
- `cs setup` — interactive wizard
- `cs alias <c|cs|cc|...>` — install/replace shell shortcut
- `cs uninstall` — restore pre-cs `~/.claude` layout
- `cs migrate` — import `~/.config/claude-switch/config`

**Auto-switch**
- `cs link <profile> <dir>` — when cwd is under `<dir>`, default to `<profile>`
- `cs links` — list directory→profile mappings

## 7. Live TUI

Single-screen, keyboard-driven, ~1s refresh, ~30 lines tall.

```
┌─ cs ────────────────────────────────────────────────────────┐
│ Active: work        michael@zerg.ai        Max 20× plan     │
├─────────────────────────────────────────────────────────────┤
│ Profiles                                                    │
│  ▸ work       ●  3h42m left   1,243 msgs today    [1]       │
│    personal      4h12m left     201 msgs today    [2]       │
│    research     EXPIRED          ↻ refresh         [3]      │
│                                                             │
│ Active session  (~/code/zerg)                               │
│   tokens in   142,331    cache hit  91%                     │
│   tokens out   18,402    est. cost  $0.42                   │
│   5h window  ████████░░░░░░░░░░  47%   resets 18:42         │
│   weekly     ███░░░░░░░░░░░░░░░  18%                        │
│                                                             │
│ Today across all profiles                                   │
│   work       1,243 msgs   367 tools   8 sessions            │
│   personal     201 msgs    52 tools   2 sessions            │
├─────────────────────────────────────────────────────────────┤
│ [1-9] switch  [s] save  [r] refresh  [o] override  [q] quit │
└─────────────────────────────────────────────────────────────┘
```

- Number keys `1`–`9` switch instantly while the TUI is open.
- Active session pane updates by tailing the current session's `*.jsonl` (see §8).
- Expiry colors yellow at <30 min, red at <5 min.
- `q` / Esc exits without switching.

## 8. Usage data sources

**Decision: call `/api/oauth/usage` directly per profile.** This is the
undocumented endpoint that backs Claude Code's `/usage` slash command. Its
response carries the bucket utilizations the subscription quota is actually
keyed to (rolling 5-hour and weekly), so what `cs usage` shows for a profile
matches what `/usage` would show for that same logged-in account — to within
the 300s cache window.

| Datum | Source | Notes |
|---|---|---|
| Email, plan, OAuth expiry | Keychain `claudeAiOauth.*` | Already used by claude-switch |
| % of 5-hour block + reset, % of weekly cap + reset | `GET /api/oauth/usage` with the profile's saved OAuth bearer | Per-profile token, no Claude session needed |

**Endpoint contract:**

- `GET https://api.anthropic.com/api/oauth/usage`
- `Authorization: Bearer <claudeAiOauth.accessToken>`
- `anthropic-beta: oauth-2025-04-20`
- Response: `{ five_hour: {utilization, resets_at}, seven_day: {utilization, resets_at}, seven_day_sonnet, seven_day_opus, extra_usage }`. `utilization` is percent **used** (0–100); we render `100 − utilization`.

**Caching:** the endpoint 429s aggressively. Each profile's response is
written to `~/.claude-cs/cache/usage-limits/<profile>.json` with a
`fetched_at_unix` stamp; subsequent reads within 300s reuse the cached
payload, and a 429 falls back to the last cached value (even past TTL) with
a warning surfaced in the report.

**Refresh:** `cs usage --watch` redraws every second from cached payloads;
the live fetch only happens when the cache is older than 300s. Reset
countdowns recompute every tick from the cached `resets_at` timestamps so
the display feels live without re-hitting the endpoint.

**Per-profile attribution:** the OAuth token is per-profile, so the response
is per-profile by construction — no isolated home or session tagging
needed for `cs usage`. (The isolated `~/.claude-cs/profiles/<name>/providers/claude/home/`
homes still exist for `cs run` / `cs shell` so the `projects/` jsonl is
isolated, but `cs usage` no longer reads it.)

## 9. Switch flow

1. Load `<profile>` creds from Keychain `Claude Code-credentials-<profile>`.
2. If `expiresAt < now + 60s`, refresh via stored refresh_token (no browser).
3. Write to canonical Keychain entry `Claude Code-credentials`.
4. Swap per-profile files: `settings.json`, `env`.
5. Source `env` into the current shell (only when invoked through the shell function wrapper installed by `cs setup`; subprocess invocations skip this step).
6. Update `~/.claude/.active-profile`.
7. Tag the next session as `<profile>` for the usage tracker.
8. Optional: signal running Claude TUIs to re-read creds (depends on §15 Q4).

## 10. Security

- Credentials stay in Keychain. Read with `security find-generic-password -w`, write with `security add-generic-password` — same as `claude-switch`.
- `cs export` produces an age-encrypted bundle (passphrase prompted); `cs import` reverses it.
- No telemetry, no network calls except the OAuth refresh against Anthropic's auth endpoint.
- `cs doctor` checks Keychain ACLs, symlink targets, and warns if any profile dir is world-readable.

## 11. Quality-of-life features

**v1 cut:** auto-switch on cwd, shell prompt indicator, expiry notifications, quota alarms, audit log.

**v2 candidates:** `cs run <profile> -- <cmd>` (one-off without permanent switch); hot-swap inside an active Claude session (depends on cred hot-reload landing in Claude Code); profile groups/tags; per-profile MCP first-class subcommand; cost estimator; idle-eject warning ("you've been on `work` for 8h"); fzf integration; session bookmarks pinned to a profile.

## 12. Stack

**Rust + [Ratatui](https://ratatui.rs) + `crossterm` + `tokio`.** Single static binary, brew-installable, sub-20ms startup matters because `cs work` should feel instant. Filesystem watching via `notify`, Keychain via `security-framework`, JSON via `serde_json`. Same stack as our internal `ztc-tui`, so idioms transfer directly.

**Rejected:** pure bash (the reference) — fine for the swap, painful for the TUI and master-profile management. Go + Bubble Tea — viable but adds a stack for no gain. Python + Textual — extra runtime, slow start.

## 13. Migration

- Same Keychain naming → existing `claude-switch` users keep their profiles.
- `~/.claude/.active-profile` honored.
- `cs migrate` reads `~/.config/claude-switch/config` and adopts settings.

## 14. Milestones

| M | Scope |
|---|---|
| M1 | Core swap: `cs <profile>`, `save`, `list`, `rm`, `refresh`, `status`, `doctor`. Keychain compat. |
| M2 | Master profile: `cs master <name>` / `--unset`, symlink layout, `uninstall` reverses cleanly. |
| M3 | TUI: profile pane, session pane, key-driven switch, fs-watch refresh, `/api/oauth/usage` integration. |
| M4 | QoL v1 cut: cwd auto-switch, prompt indicator, expiry/quota notifications, audit log. |
| M5 | Linux secret-service backend; `export`/`import`; brew release. |

The original M2 plan included per-profile overrides and a `share-skill` command;
those were dropped because the master-as-profile model already lets a profile
diverge by simply not being master, and per-path overrides added file-tracking
complexity without a clear use case.

## 15. Open questions

1. **Hot-reload inside an active Claude session** — ship a `/cs <profile>` plugin in v1, or wait until Claude Code supports cred hot-reload? Current plan: punt to v2; v1 falls back to "exit and re-enter."
2. **`~/.claude/projects/`** — leave shared (current behavior, session history follows you across accounts) or namespace per profile (cleaner separation). Lean **shared**; per-profile attribution is handled by `session-tags.jsonl` (§8).
3. **Telemetry** — off by default forever, or opt-in for crash reports? Lean **off**; revisit only if we hit reproducibility pain.
