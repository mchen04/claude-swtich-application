use std::path::PathBuf;

use crate::cli::ShellChoice;
use crate::error::{Error, Result};

pub mod bash;
pub mod zsh;

pub const BEGIN_MARKER: &str = "# >>> cs (claude-switch) >>>";
pub const END_MARKER: &str = "# <<< cs (claude-switch) <<<";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
}

impl Shell {
    pub fn detect(choice: ShellChoice) -> Result<Self> {
        match choice {
            ShellChoice::Zsh => Ok(Self::Zsh),
            ShellChoice::Bash => Ok(Self::Bash),
            ShellChoice::Auto => {
                let env_shell = std::env::var("SHELL").unwrap_or_default();
                if env_shell.ends_with("/zsh") || env_shell == "zsh" {
                    Ok(Self::Zsh)
                } else if env_shell.ends_with("/bash") || env_shell == "bash" {
                    Ok(Self::Bash)
                } else {
                    Err(Error::Config(format!(
                        "could not detect shell from $SHELL=`{env_shell}`; pass --shell zsh|bash"
                    )))
                }
            }
        }
    }

    pub fn rc_path(self) -> Option<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        match self {
            Shell::Zsh => Some(home.join(".zshrc")),
            Shell::Bash => Some(home.join(".bashrc")),
        }
    }

    pub fn snippet(self) -> &'static str {
        match self {
            Shell::Zsh => zsh::SNIPPET,
            Shell::Bash => bash::SNIPPET,
        }
    }
}

/// Replace any existing `# >>> cs ... # <<< cs` block with `body`, or append it if absent.
/// Returns the new file contents.
pub fn upsert_block(existing: &str, body: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(BEGIN_MARKER), existing.find(END_MARKER)) {
        if start < end {
            let end_line = existing[end..]
                .find('\n')
                .map(|n| end + n + 1)
                .unwrap_or(existing.len());
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..start]);
            out.push_str(BEGIN_MARKER);
            out.push('\n');
            out.push_str(body.trim_end_matches('\n'));
            out.push('\n');
            out.push_str(END_MARKER);
            out.push('\n');
            out.push_str(&existing[end_line..]);
            return out;
        }
    }
    let mut out = String::with_capacity(existing.len() + body.len() + 64);
    out.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(BEGIN_MARKER);
    out.push('\n');
    out.push_str(body.trim_end_matches('\n'));
    out.push('\n');
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

/// Remove the `# >>> cs ... # <<< cs` block if present. Returns the new file contents.
#[allow(dead_code)] // used by `cs uninstall` (Phase D)
pub fn remove_block(existing: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(BEGIN_MARKER), existing.find(END_MARKER)) {
        if start < end {
            let end_line = existing[end..]
                .find('\n')
                .map(|n| end + n + 1)
                .unwrap_or(existing.len());
            let mut out = String::with_capacity(existing.len());
            // Trim a trailing blank line we may have inserted before the block.
            let head = existing[..start].trim_end_matches('\n');
            out.push_str(head);
            if !head.is_empty() {
                out.push('\n');
            }
            out.push_str(&existing[end_line..]);
            return out;
        }
    }
    existing.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_appends_when_missing() {
        let s = upsert_block("export FOO=1\n", "alias x=y");
        assert!(s.contains(BEGIN_MARKER));
        assert!(s.contains("alias x=y"));
        assert!(s.contains(END_MARKER));
    }

    #[test]
    fn upsert_replaces_existing() {
        let initial = upsert_block("export A=1\n", "alias x=y");
        let updated = upsert_block(&initial, "alias x=z");
        assert!(updated.contains("alias x=z"));
        assert!(!updated.contains("alias x=y"));
        assert_eq!(updated.matches(BEGIN_MARKER).count(), 1);
    }

    #[test]
    fn remove_drops_block_idempotent() {
        let with = upsert_block("export A=1\n", "alias x=y");
        let without = remove_block(&with);
        assert!(!without.contains(BEGIN_MARKER));
        assert_eq!(remove_block(&without), without);
    }
}
