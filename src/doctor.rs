use std::fmt;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

use crate::error::Result;
use crate::keychain::{self, Keychain};
use crate::master::{self, ItemState, MasterStatus};
use crate::paths::Paths;
use crate::state::State;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub paths: PathReport,
    pub tooling: Vec<ToolCheck>,
    pub keychain: KeychainReport,
    pub master: MasterStatus,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PathReport {
    pub claude_home: PathInfo,
    pub cs_home: PathInfo,
    pub projects_dir: PathInfo,
    pub state_file: PathInfo,
}

#[derive(Debug, Serialize)]
pub struct PathInfo {
    pub path: PathBuf,
    pub exists: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
}

impl PathInfo {
    fn probe(p: PathBuf) -> Self {
        let meta = std::fs::symlink_metadata(&p).ok();
        let exists = meta.is_some();
        let is_symlink = meta
            .as_ref()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let is_dir = std::fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false);
        Self {
            path: p,
            exists,
            is_dir,
            is_symlink,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolCheck {
    pub name: String,
    pub found: bool,
    pub version: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KeychainReport {
    pub backend: &'static str,
    pub canonical_present: bool,
    pub profile_count: usize,
    pub profile_accounts: Vec<String>,
    pub error: Option<String>,
}

pub fn run(paths: &Paths, kc: &dyn Keychain) -> Result<DoctorReport> {
    let path_report = PathReport {
        claude_home: PathInfo::probe(paths.claude_home.clone()),
        cs_home: PathInfo::probe(paths.cs_home.clone()),
        projects_dir: PathInfo::probe(paths.projects_dir()),
        state_file: PathInfo::probe(paths.state_file()),
    };

    let tooling = vec![
        check_tool("claude", &["--version"]),
        check_presence("security", "/usr/bin/security"),
        check_tool("jq", &["--version"]),
        check_tool("age", &["--version"]),
    ];

    let keychain = check_keychain(kc);

    let state = State::load(&paths.state_file()).unwrap_or_default();
    let master = master::status(paths, &state)?;

    let generated_at = chrono::Utc::now().to_rfc3339();

    Ok(DoctorReport {
        paths: path_report,
        tooling,
        keychain,
        master,
        generated_at,
    })
}

fn check_tool(name: &str, version_args: &[&str]) -> ToolCheck {
    let out = Command::new(name).args(version_args).output();
    match out {
        Ok(o) if o.status.success() => {
            let v = String::from_utf8_lossy(&o.stdout)
                .trim()
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            ToolCheck {
                name: name.to_string(),
                found: true,
                version: if v.is_empty() { None } else { Some(v) },
                note: None,
            }
        }
        Ok(o) => ToolCheck {
            name: name.to_string(),
            found: true,
            version: None,
            note: Some(format!(
                "exited {}",
                o.status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into())
            )),
        },
        Err(_) => ToolCheck {
            name: name.to_string(),
            found: false,
            version: None,
            note: Some("not on PATH".into()),
        },
    }
}

fn check_presence(name: &str, expected_path: &str) -> ToolCheck {
    let exists = std::path::Path::new(expected_path).exists();
    ToolCheck {
        name: name.to_string(),
        found: exists,
        version: if exists {
            Some(expected_path.to_string())
        } else {
            None
        },
        note: if exists {
            None
        } else {
            Some(format!("not at {expected_path}"))
        },
    }
}

fn check_keychain(kc: &dyn Keychain) -> KeychainReport {
    match kc.list() {
        Ok(list) => {
            let canonical_present = list.iter().any(|a| !keychain::is_profile_account(a));
            let profiles: Vec<String> = list
                .into_iter()
                .filter(|a| keychain::is_profile_account(a))
                .collect();
            KeychainReport {
                backend: backend_name(),
                canonical_present,
                profile_count: profiles.len(),
                profile_accounts: profiles,
                error: None,
            }
        }
        Err(e) => KeychainReport {
            backend: backend_name(),
            canonical_present: false,
            profile_count: 0,
            profile_accounts: vec![],
            error: Some(e.to_string()),
        },
    }
}

fn backend_name() -> &'static str {
    if std::env::var_os("CS_TEST_KEYCHAIN").is_some() {
        return "mock";
    }
    if cfg!(target_os = "macos") {
        "macos-keychain"
    } else {
        "mock"
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "cs doctor — {}", self.generated_at)?;
        writeln!(f)?;
        writeln!(f, "Paths")?;
        writeln!(f, "  CLAUDE_HOME : {}", fmt_path(&self.paths.claude_home))?;
        writeln!(f, "  CS_HOME     : {}", fmt_path(&self.paths.cs_home))?;
        writeln!(f, "  projects/   : {}", fmt_path(&self.paths.projects_dir))?;
        writeln!(f, "  state.json  : {}", fmt_path(&self.paths.state_file))?;
        writeln!(f)?;
        writeln!(f, "Tooling")?;
        for t in &self.tooling {
            let mark = if t.found { "ok " } else { "MISS" };
            let v = t.version.as_deref().unwrap_or("");
            let note = t.note.as_deref().unwrap_or("");
            writeln!(f, "  [{mark}] {:<20} {v} {note}", t.name)?;
        }
        writeln!(f)?;
        writeln!(f, "Keychain ({})", self.keychain.backend)?;
        writeln!(
            f,
            "  canonical entry : {}",
            if self.keychain.canonical_present {
                "present"
            } else {
                "missing"
            }
        )?;
        writeln!(f, "  saved profiles  : {}", self.keychain.profile_count)?;
        for a in &self.keychain.profile_accounts {
            writeln!(f, "                    {a}")?;
        }
        if let Some(err) = &self.keychain.error {
            writeln!(f, "  error           : {err}")?;
        }
        writeln!(f)?;
        writeln!(f, "Master profile")?;
        match &self.master.master {
            Some(name) => writeln!(f, "  designated      : {name}")?,
            None => writeln!(f, "  designated      : (none)")?,
        }
        if let Some(dir) = &self.master.master_dir {
            writeln!(f, "  dir             : {}", dir.display())?;
        }
        for item in &self.master.items {
            let mark = match item.state {
                ItemState::Symlinked => "ok ",
                ItemState::Missing => "—  ",
                ItemState::Local => "loc",
                ItemState::SymlinkBroken => "BAD",
                ItemState::SymlinkForeign => "FRN",
            };
            writeln!(
                f,
                "  [{mark}] {:<12} {}",
                item.name,
                item.claude_path.display()
            )?;
        }
        Ok(())
    }
}

fn fmt_path(p: &PathInfo) -> String {
    let kind = if !p.exists {
        "missing"
    } else if p.is_symlink {
        "symlink"
    } else if p.is_dir {
        "dir"
    } else {
        "file"
    };
    format!("{} [{}]", p.path.display(), kind)
}
