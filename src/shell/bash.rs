pub const SNIPPET: &str = r#"# managed by `cs`: do not edit
claude() {
  CS_SHELL_WRAPPER=1 command claude "$@"
  local rc=$?
  if [ -f "$HOME/.claude-cs/.last-env" ]; then
    set -a
    . "$HOME/.claude-cs/.last-env"
    set +a
    rm -f "$HOME/.claude-cs/.last-env"
  fi
  return $rc
}
"#;
