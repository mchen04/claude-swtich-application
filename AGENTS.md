# cs — Agent Guide

Claude Code account switcher with master-profile sharing and live usage TUI.

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
- `CODEX_HOME` — Codex home (default `~/.codex`)
- `CS_TEST_KEYCHAIN=1` — swap the macOS keychain backend for an in-memory JSON mock
- `CS_TEST_KEYCHAIN_FIXTURE=/path/to.json` — seed the mock with `{account: blob}` entries
- `CS_TEST_DISABLE_CCUSAGE=1` — force ccusage subprocesses to fail (for fallback testing)

## Key Design Decisions

- **Keychain compat** — reuses the same `Claude Code-credentials` service and
  `Claude Code-credentials-<profile>` account naming as the legacy `claude-switch` bash tool.
- **Master profile** — one designated profile owns `skills/`, `commands/`, `agents/`, and
  `CLAUDE.md`; every other profile inherits them via symlinks in `~/.claude/`.
- **Provider isolation** — `cs run <profile> <provider>` materializes a per-profile home
  under `~/.claude-cs/profiles/<name>/providers/<provider>/home/` and exports the
  appropriate env vars (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, etc.).
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
