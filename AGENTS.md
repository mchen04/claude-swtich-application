# cs — Agent Guide

Claude Code account switcher with master-profile sharing and a multi-account
usage dashboard.

## Build

```bash
cargo build --release
```

## Test

```bash
cargo test                           # 31 integration + 13 unit tests
cargo clippy --all-targets -- -D warnings
```

## Architecture

Three-layer architecture:

1. **CLI layer** (`cli.rs`, `main.rs`) — clap argument parsing and dispatch.
2. **Command layer** (`commands/*.rs`) — one file per subcommand; handles dry-run,
   locking, and output formatting.
3. **Core layer** — filesystem, keychain, state, and provider isolation.

```
main.rs → dispatch → commands/<cmd>.rs → keychain / paths / state / master
```

## Test Seams

All filesystem and keychain access is injectable:

- `CLAUDE_HOME` — canonical Claude Code home (default `~/.claude`)
- `CS_HOME` — cs state directory (default `~/.claude-cs`)
- `CS_TEST_KEYCHAIN=1` — swap the macOS keychain backend for an in-memory JSON mock
- `CS_TEST_KEYCHAIN_FIXTURE=/path/to.json` — seed the mock with `{account: blob}` entries
- `CS_TEST_LIMITS_FIXTURE=/dir` — read `/api/oauth/usage` payloads from
  `<dir>/<profile>.json` instead of hitting the network
- `CS_TEST_LIMITS_FAIL=/dir` — force a failure mode per profile via
  `<dir>/<profile>.txt` containing one of `expired`, `rate_limited`, `http`

## Key Design Decisions

- **Keychain compat** — reuses the same `Claude Code-credentials` service and
  `Claude Code-credentials-<profile>` account naming as the legacy `claude-switch` bash tool.
- **Master profile** — one designated profile owns `skills/`, `commands/`, `agents/`, and
  `CLAUDE.md`; every other profile inherits them via symlinks in `~/.claude/`.
- **Per-profile isolation** — `cs run <profile>` materializes an isolated home
  under `~/.claude-cs/profiles/<name>/providers/claude/home/` and exports
  `CLAUDE_CONFIG_DIR` + `CLAUDE_HOME` so claude reads only that profile's
  `projects/` jsonl.
- **Usage dashboard** — `cs usage` calls `/api/oauth/usage` per profile using
  the saved OAuth token; responses are cached at
  `~/.claude-cs/cache/usage-limits/<profile>.json` for 300s to avoid 429s.
- **Atomic writes** — all file mutations go through `jsonio::atomic_write_bytes` (tempfile +
  `rename(2)`) to avoid torn writes.
- **Rollback manifests** — every destructive op writes
  `~/.claude-cs/.backups/<ts>/manifest.json` recording before/after state.

## Code Conventions

- Use `Error::io_at(path, source)` when an io error carries a path.
- Acquire `CsLock` before any mutation that touches state, keychain, or profile dirs.
- Prefer `jsonio::load_or_default` for JSON config files (treats missing as default).
- Emit JSON via `output::emit_json`; emit text via `output::emit_text(OutputOpts { json: false }, &displayable)`.
- Dry-run builds a `dryrun::Plan`, then emits it without acquiring locks.

## Deferred Features

- Ratatui TUI (`cs tui`) — stub removed; planned for Phase F.
- Session live tailer (`SessionLive`) — removed; depends on TUI.
- `cs export` / `cs import` with age encryption.
- `cs audit` / `cs revert` for rollback manifest replay.
- Linux secret-service backend (currently falls back to mock on non-macOS).
- `--no-color` CLI flag accepted but not yet wired to output stripping.
