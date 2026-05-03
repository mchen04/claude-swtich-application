# cs — Agent Guide

Claude Code account switcher (macOS) with master-profile sharing and a
multi-account usage dashboard.

## Build & test

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

## Architecture

```
main.rs → cli::Cli → commands/<cmd>.rs → keychain / paths / state / master / usage
```

- **`cli.rs`** — clap argument definitions; bare `cs <name>` and `cs -` are
  rewritten in `main.rs::rewrite_bare_invocation` into hidden `__switch` /
  `__switch-previous` subcommands.
- **`commands/*.rs`** — one file per subcommand. Each acquires `CsLock` before
  any mutation.
- **`keychain/`** — `macos.rs` is the production backend (security-framework);
  `mock.rs` is the JSON-fixture backend used in tests.
- **`paths.rs`** — single source of truth for filesystem locations; honors
  `CLAUDE_HOME` and `CS_HOME` env vars for test isolation.
- **`master.rs`** + **`symlinks.rs`** — master-profile move + symlink
  management for `skills/`, `commands/`, `agents/`, `CLAUDE.md`.
- **`usage/limits.rs`** — `/api/oauth/usage` client with on-disk cache.
- **`shell/`** — zsh / bash wrapper installation.

## Test seams

- `CLAUDE_HOME=/tmp/...` — override canonical Claude home.
- `CS_HOME=/tmp/...` — override cs state directory.
- `CS_TEST_KEYCHAIN=1` — swap macOS Keychain for the in-memory mock.
- `CS_TEST_KEYCHAIN_FIXTURE=/path.json` — seed the mock with `{account: blob}`.
- `CS_TEST_LIMITS_FIXTURE=/dir` — read `/api/oauth/usage` payloads from
  `<dir>/<profile>.json` instead of hitting the network.
- `CS_TEST_LIMITS_FAIL=/dir` — force a failure mode per profile via
  `<dir>/<profile>.txt` containing `expired`, `rate_limited`, or `http`.

## Key invariants

- **Keychain compat** — service `Claude Code-credentials`, account
  `Claude Code-credentials-<name>` for saved profiles, account `$USER` for
  the canonical entry. Same naming as the legacy `claude-switch` bash tool.
- **Verified writes** — every Keychain write is read back and byte-compared;
  on mismatch we roll back to the previous credential.
- **Atomic file writes** — all JSON mutations go through
  `jsonio::atomic_write_bytes` (tempfile + `rename(2)`).
- **Locking** — `CsLock` (advisory `flock` on `~/.claude-cs/.lock`) is
  acquired before any mutation that touches state, keychain, or profile dirs.
- **Usage cache** — `~/.claude-cs/cache/usage-limits/<profile>.json`. Single
  300s TTL for both one-shot and `--watch`; the endpoint is aggressively
  rate-limited and a triggered 429 can lock out for 30+ minutes
  (anthropics/claude-code#31637). 429s fall back to the last cached value past
  TTL with a warning. Watch's "live feel" comes from per-tick countdown
  re-rendering, not from refetches.

## Code conventions

- Use `Error::io_at(path, source)` whenever an io error carries a path.
- Prefer `jsonio::load_or_default` for JSON config files (treats missing as default).
- Emit JSON via `output::emit_json`; emit text via `output::emit_text`.

## Scope

macOS only. The `security-framework` dep is a hard requirement; non-macOS
builds fall back to the mock keychain and are intended only for tests.
