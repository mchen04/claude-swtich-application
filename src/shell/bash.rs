pub const SNIPPET: &str = r#"# managed by `cs`: do not edit
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
"#;
