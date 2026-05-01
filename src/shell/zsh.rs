pub const SNIPPET: &str = r#"# managed by `cs`: do not edit
# Wraps `claude` so the credential swap, env file, and post-call hand-back happen
# transparently. CS_SHELL_WRAPPER lets the binary emit ~/.claude-cs/.last-env which we
# source after the call so per-profile env vars survive.
__cs_source_env() {
  if [ -f "$HOME/.claude-cs/.last-env" ]; then
    set -a
    . "$HOME/.claude-cs/.last-env"
    set +a
    rm -f "$HOME/.claude-cs/.last-env"
  fi
}
claude() {
  CS_SHELL_WRAPPER=1 command claude "$@"
  local rc=$?
  __cs_source_env
  return $rc
}
codex() {
  CS_SHELL_WRAPPER=1 command codex "$@"
  local rc=$?
  __cs_source_env
  return $rc
}
"#;
