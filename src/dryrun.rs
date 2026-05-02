use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Move { from: PathBuf, to: PathBuf },
    Copy { from: PathBuf, to: PathBuf },
    WriteFile { path: PathBuf, bytes: usize },
    KeychainWrite { account: String, bytes: usize },
    KeychainDelete { account: String },
    SpawnProcess { cmd: String, args: Vec<String> },
    Note { message: String },
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
}

impl Plan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, a: Action) {
        self.actions.push(a);
    }

}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.actions.is_empty() {
            return writeln!(f, "(no changes)");
        }
        for (i, a) in self.actions.iter().enumerate() {
            write!(f, "{:>3}. ", i + 1)?;
            match a {
                Action::Move { from, to } => {
                    writeln!(f, "move    {} -> {}", from.display(), to.display())?
                }
                Action::Copy { from, to } => {
                    writeln!(f, "copy    {} -> {}", from.display(), to.display())?
                }
                Action::WriteFile { path, bytes } => {
                    writeln!(f, "write   {} ({} bytes)", path.display(), bytes)?
                }
                Action::KeychainWrite { account, bytes } => {
                    writeln!(f, "kc-set  {} ({} bytes)", account, bytes)?
                }
                Action::KeychainDelete { account } => writeln!(f, "kc-rm   {}", account)?,
                Action::SpawnProcess { cmd, args } => {
                    writeln!(f, "spawn   {} {}", cmd, args.join(" "))?
                }
                Action::Note { message } => writeln!(f, "note    {}", message)?,
            }
        }
        Ok(())
    }
}
