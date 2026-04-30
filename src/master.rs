use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup::{BackupAction, Manifest};
use crate::error::{Error, Result};
use crate::paths::{Paths, SHARED_ITEMS};
use crate::state::State;
use crate::symlinks;

#[derive(Debug, Serialize)]
pub struct MasterStatus {
    pub master: Option<String>,
    pub master_dir: Option<PathBuf>,
    pub items: Vec<MasterItem>,
}

#[derive(Debug, Serialize)]
pub struct MasterItem {
    pub name: String,
    pub claude_path: PathBuf,
    pub master_path: Option<PathBuf>,
    pub state: ItemState,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// Path is missing in `~/.claude/`.
    Missing,
    /// Plain file or directory in `~/.claude/` (no master designated, or shadowing).
    Local,
    /// Symlink correctly pointing into the master profile's dir, target exists.
    Symlinked,
    /// Symlink into the master profile's dir, but target is missing.
    SymlinkBroken,
    /// Symlink pointing somewhere else (not into the designated master profile).
    SymlinkForeign,
}

pub fn status(paths: &Paths, state: &State) -> Result<MasterStatus> {
    let master = state.master.clone();
    let master_dir = master.as_deref().map(|m| paths.profile_dir(m));
    let mut items = Vec::new();
    for name in SHARED_ITEMS {
        let claude_path = paths.claude_home.join(name);
        let master_path = master_dir.as_ref().map(|d| d.join(name));
        let item_state = classify(&claude_path, master_dir.as_deref(), master_path.as_deref());
        items.push(MasterItem {
            name: (*name).to_string(),
            claude_path,
            master_path,
            state: item_state,
        });
    }
    Ok(MasterStatus { master, master_dir, items })
}

fn classify(claude_path: &Path, master_dir: Option<&Path>, master_path: Option<&Path>) -> ItemState {
    let meta = match fs::symlink_metadata(claude_path) {
        Ok(m) => m,
        Err(_) => return ItemState::Missing,
    };
    if !meta.file_type().is_symlink() {
        return ItemState::Local;
    }
    let Some(master_dir) = master_dir else {
        return ItemState::SymlinkForeign;
    };
    if !symlinks::points_into(claude_path, master_dir) {
        return ItemState::SymlinkForeign;
    }
    match master_path {
        Some(p) if p.exists() => ItemState::Symlinked,
        _ => ItemState::SymlinkBroken,
    }
}

#[derive(Debug, Serialize, Default)]
pub struct SetReport {
    pub master: String,
    pub previous_master: Option<String>,
    pub moved: Vec<String>,
    pub already: Vec<String>,
    pub skipped_empty: Vec<String>,
    pub manifest_path: Option<PathBuf>,
}

/// Designate `name` as the master profile. Handles both first-time set and
/// master-change in one function (dispatch on whether `state.master` is currently set).
pub fn set(paths: &Paths, state: &mut State, name: &str, dry_run: bool) -> Result<SetReport> {
    let new_dir = paths.profile_dir(name);
    let mut report = SetReport {
        master: name.to_string(),
        previous_master: state.master.clone(),
        ..SetReport::default()
    };

    if !dry_run {
        fs::create_dir_all(&new_dir).map_err(|e| Error::io_at(&new_dir, e))?;
    }

    match state.master.clone() {
        None => set_first_time(paths, state, name, &new_dir, dry_run, &mut report)?,
        Some(prev) if prev == name => {
            // Re-designating the same master is a no-op (idempotent).
            for item in SHARED_ITEMS {
                let claude_path = paths.claude_home.join(item);
                let master_path = new_dir.join(item);
                match classify(&claude_path, Some(&new_dir), Some(&master_path)) {
                    ItemState::Symlinked => report.already.push((*item).to_string()),
                    ItemState::Missing => report.skipped_empty.push((*item).to_string()),
                    _ => {}
                }
            }
        }
        Some(prev) => set_change_master(paths, state, &prev, name, &new_dir, dry_run, &mut report)?,
    }

    Ok(report)
}

fn set_first_time(
    paths: &Paths,
    state: &mut State,
    name: &str,
    new_dir: &Path,
    dry_run: bool,
    report: &mut SetReport,
) -> Result<()> {
    let mut manifest = Manifest::new("master-set").with_master(name);

    for item in SHARED_ITEMS {
        let claude_path = paths.claude_home.join(item);
        let master_path = new_dir.join(item);
        let meta = fs::symlink_metadata(&claude_path).ok();
        let Some(meta) = meta else {
            report.skipped_empty.push((*item).to_string());
            continue;
        };
        if meta.file_type().is_symlink() {
            // Pre-existing symlink — leave as-is and warn via state.
            if symlinks::points_into(&claude_path, new_dir) {
                report.already.push((*item).to_string());
            } else {
                report.skipped_empty.push((*item).to_string());
            }
            continue;
        }
        if meta.file_type().is_dir() && is_empty_dir(&claude_path) {
            // Empty directory: create matching empty dir under master, replace with link.
            if dry_run {
                report.moved.push((*item).to_string());
                continue;
            }
            fs::create_dir_all(&master_path).map_err(|e| Error::io_at(&master_path, e))?;
            fs::remove_dir(&claude_path).map_err(|e| Error::io_at(&claude_path, e))?;
            symlinks::replace(&master_path, &claude_path)?;
            manifest.push(BackupAction::FsMove {
                from: claude_path.clone(),
                to: master_path.clone(),
            });
            manifest.push(BackupAction::SymlinkCreate {
                link: claude_path.clone(),
                target: master_path.clone(),
            });
            report.moved.push((*item).to_string());
            continue;
        }
        if master_path.exists() {
            return Err(Error::Other(format!(
                "{} already exists in master profile — manual reconciliation required",
                master_path.display()
            )));
        }
        if dry_run {
            report.moved.push((*item).to_string());
            continue;
        }
        move_path(&claude_path, &master_path)?;
        symlinks::replace(&master_path, &claude_path)?;
        manifest.push(BackupAction::FsMove {
            from: claude_path.clone(),
            to: master_path.clone(),
        });
        manifest.push(BackupAction::SymlinkCreate {
            link: claude_path.clone(),
            target: master_path.clone(),
        });
        report.moved.push((*item).to_string());
    }

    if !dry_run {
        if !manifest.actions.is_empty() {
            let path = manifest.write(paths)?;
            report.manifest_path = Some(path);
        }
        state.master = Some(name.to_string());
        state.save(&paths.state_file())?;
    }
    Ok(())
}

fn set_change_master(
    paths: &Paths,
    state: &mut State,
    prev_name: &str,
    new_name: &str,
    new_dir: &Path,
    dry_run: bool,
    report: &mut SetReport,
) -> Result<()> {
    let prev_dir = paths.profile_dir(prev_name);

    // Refuse if any of the four candidates already exists under the new dir.
    for item in SHARED_ITEMS {
        let candidate = new_dir.join(item);
        if candidate.exists() {
            return Err(Error::Other(format!(
                "cannot change master to `{new_name}`: {} already exists; \
                 remove it manually or pick a different profile",
                candidate.display()
            )));
        }
    }

    let mut manifest = Manifest::new("master-change").with_master(new_name);

    for item in SHARED_ITEMS {
        let from = prev_dir.join(item);
        let to = new_dir.join(item);
        let claude_path = paths.claude_home.join(item);
        if !from.exists() {
            // Old master didn't have this item; skip.
            continue;
        }
        if dry_run {
            report.moved.push((*item).to_string());
            continue;
        }
        move_path(&from, &to)?;
        manifest.push(BackupAction::FsMove { from: from.clone(), to: to.clone() });
        // Retarget the ~/.claude/* symlink to the new location.
        symlinks::replace(&to, &claude_path)?;
        manifest.push(BackupAction::SymlinkCreate {
            link: claude_path.clone(),
            target: to.clone(),
        });
        report.moved.push((*item).to_string());
    }

    if !dry_run {
        if !manifest.actions.is_empty() {
            let path = manifest.write(paths)?;
            report.manifest_path = Some(path);
        }
        state.master = Some(new_name.to_string());
        state.save(&paths.state_file())?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Default)]
pub struct UnsetReport {
    pub previous_master: Option<String>,
    pub restored: Vec<String>,
    pub manifest_path: Option<PathBuf>,
}

/// Clear the master designation: move content from the master profile dir back
/// to `~/.claude/`, drop the four symlinks, and clear `state.master`.
pub fn unset(paths: &Paths, state: &mut State, dry_run: bool) -> Result<UnsetReport> {
    let mut report = UnsetReport {
        previous_master: state.master.clone(),
        ..UnsetReport::default()
    };
    let Some(master_name) = state.master.clone() else {
        return Ok(report); // No-op.
    };
    let master_dir = paths.profile_dir(&master_name);
    let mut manifest = Manifest::new("master-unset").with_master(&master_name);

    for item in SHARED_ITEMS {
        let claude_path = paths.claude_home.join(item);
        let master_path = master_dir.join(item);
        // Drop the symlink first (if it exists and points into master).
        let is_symlink = fs::symlink_metadata(&claude_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let points_in = is_symlink && symlinks::points_into(&claude_path, &master_dir);
        if dry_run {
            if points_in && master_path.exists() {
                report.restored.push((*item).to_string());
            }
            continue;
        }
        if points_in {
            symlinks::remove(&claude_path)?;
            manifest.push(BackupAction::SymlinkRemove {
                link: claude_path.clone(),
                target: master_path.clone(),
            });
        }
        if master_path.exists() {
            // Move content back to ~/.claude.
            move_path(&master_path, &claude_path)?;
            manifest.push(BackupAction::FsMove {
                from: master_path.clone(),
                to: claude_path.clone(),
            });
            report.restored.push((*item).to_string());
        }
    }

    if !dry_run {
        if !manifest.actions.is_empty() {
            let path = manifest.write(paths)?;
            report.manifest_path = Some(path);
        }
        state.master = None;
        state.save(&paths.state_file())?;
    }
    Ok(report)
}

/// Retarget the `~/.claude/{skills,commands,agents,CLAUDE.md}` symlinks to a
/// new master profile directory. Used when the master profile is renamed.
pub fn retarget_symlinks(paths: &Paths, new_master: &str) -> Result<Vec<BackupAction>> {
    let new_dir = paths.profile_dir(new_master);
    let mut actions = Vec::new();
    for item in SHARED_ITEMS {
        let claude_path = paths.claude_home.join(item);
        let new_target = new_dir.join(item);
        let meta = match fs::symlink_metadata(&claude_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        if !new_target.exists() {
            continue;
        }
        symlinks::replace(&new_target, &claude_path)?;
        actions.push(BackupAction::SymlinkCreate {
            link: claude_path,
            target: new_target,
        });
    }
    Ok(actions)
}

#[derive(Debug, Serialize, Default)]
pub struct UninstallReport {
    pub restored: Vec<String>,
    pub left_in_place: Vec<String>,
    pub manifest_replayed: Option<PathBuf>,
}

pub fn uninstall(paths: &Paths, keep_master: bool, dry_run: bool) -> Result<UninstallReport> {
    let mut report = UninstallReport::default();
    let state_path = paths.state_file();
    let mut state = State::load(&state_path).unwrap_or_default();
    let Some(master_name) = state.master.clone() else {
        // Nothing designated; only stray symlinks (if any) need cleanup.
        let status = status(paths, &state)?;
        for item in status.items {
            if matches!(item.state, ItemState::SymlinkBroken | ItemState::SymlinkForeign)
                && !dry_run
            {
                let _ = symlinks::remove(&item.claude_path);
            }
        }
        return Ok(report);
    };
    let master_dir = paths.profile_dir(&master_name);

    if keep_master {
        // Drop symlinks, leave content in the master profile dir.
        if !dry_run {
            for item in SHARED_ITEMS {
                let claude_path = paths.claude_home.join(item);
                let is_symlink = fs::symlink_metadata(&claude_path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                if is_symlink && symlinks::points_into(&claude_path, &master_dir) {
                    symlinks::remove(&claude_path)?;
                    report.left_in_place.push((*item).to_string());
                }
            }
            eprintln!("left in profile `{}`", master_name);
        }
    } else if !dry_run {
        let unset_report = unset(paths, &mut state, false)?;
        report.restored = unset_report.restored;
        report.manifest_replayed = unset_report.manifest_path;
    }
    Ok(report)
}

fn is_empty_dir(p: &Path) -> bool {
    fs::read_dir(p).map(|mut i| i.next().is_none()).unwrap_or(false)
}

fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    match fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc_exdev()) => copy_recursive(from, to)
            .and_then(|_| remove_recursive(from)),
        Err(e) => Err(Error::io_at(to, e)),
    }
}

fn libc_exdev() -> i32 {
    18 // EXDEV on Linux/macOS
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).map_err(|e| Error::io_at(src, e))?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        let target = fs::read_link(src).map_err(|e| Error::io_at(src, e))?;
        std::os::unix::fs::symlink(&target, dst).map_err(|e| Error::io_at(dst, e))?;
    } else if ft.is_dir() {
        fs::create_dir_all(dst).map_err(|e| Error::io_at(dst, e))?;
        for entry in fs::read_dir(src).map_err(|e| Error::io_at(src, e))? {
            let entry = entry.map_err(|e| Error::io_at(src, e))?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dst).map_err(|e| Error::io_at(dst, e))?;
    }
    Ok(())
}

fn remove_recursive(p: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(p).map_err(|e| Error::io_at(p, e))?;
    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(p).map_err(|e| Error::io_at(p, e))
    } else {
        fs::remove_file(p).map_err(|e| Error::io_at(p, e))
    }
}
