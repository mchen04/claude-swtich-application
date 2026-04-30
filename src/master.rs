use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup::{BackupAction, Manifest};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::symlinks;

/// Items we share across profiles via master.
pub const CANDIDATES: &[&str] = &["skills", "commands", "agents", "CLAUDE.md"];

#[derive(Debug, Serialize)]
pub struct MasterStatus {
    pub master_dir: PathBuf,
    pub items: Vec<MasterItem>,
}

#[derive(Debug, Serialize)]
pub struct MasterItem {
    pub name: String,
    pub claude_path: PathBuf,
    pub master_path: PathBuf,
    pub state: ItemState,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Missing,
    NotInMaster,
    Symlinked,
    SymlinkOutsideMaster,
    #[allow(dead_code)] // reserved for future "regular file blocks the move" reporting
    BlockingFile,
}

pub fn status(paths: &Paths) -> Result<MasterStatus> {
    let master_dir = paths.master_dir();
    let mut items = Vec::new();
    for name in CANDIDATES {
        let claude_path = paths.claude_home.join(name);
        let master_path = master_dir.join(name);
        let state = classify(&claude_path, &master_path, &master_dir);
        items.push(MasterItem {
            name: (*name).to_string(),
            claude_path,
            master_path,
            state,
        });
    }
    Ok(MasterStatus { master_dir, items })
}

fn classify(claude_path: &Path, master_path: &Path, master_dir: &Path) -> ItemState {
    let meta = fs::symlink_metadata(claude_path);
    match meta {
        Err(_) => ItemState::Missing,
        Ok(m) if m.file_type().is_symlink() => {
            if symlinks::points_into(claude_path, master_dir) {
                if master_path.exists() {
                    ItemState::Symlinked
                } else {
                    ItemState::SymlinkOutsideMaster
                }
            } else {
                ItemState::SymlinkOutsideMaster
            }
        }
        Ok(_) => ItemState::NotInMaster,
    }
}

#[derive(Debug, Serialize, Default)]
pub struct InitReport {
    pub moved: Vec<String>,
    pub already: Vec<String>,
    pub skipped_empty: Vec<String>,
    pub blocked: Vec<String>,
    pub manifest_path: Option<PathBuf>,
}

pub fn init(paths: &Paths, dry_run: bool) -> Result<InitReport> {
    let master_dir = paths.master_dir();
    if !dry_run {
        fs::create_dir_all(&master_dir).map_err(|e| Error::io_at(&master_dir, e))?;
    }
    let mut report = InitReport::default();
    let mut manifest = Manifest::new("master-init");

    for name in CANDIDATES {
        let claude_path = paths.claude_home.join(name);
        let master_path = master_dir.join(name);
        let state = classify(&claude_path, &master_path, &master_dir);
        match state {
            ItemState::Missing => {
                report.skipped_empty.push((*name).to_string());
            }
            ItemState::Symlinked => {
                report.already.push((*name).to_string());
            }
            ItemState::SymlinkOutsideMaster => {
                report.blocked.push(format!(
                    "{} is a symlink not pointing into master; remove it manually first",
                    claude_path.display()
                ));
            }
            ItemState::BlockingFile => {
                report.blocked.push(format!(
                    "{} blocks the move; resolve manually",
                    claude_path.display()
                ));
            }
            ItemState::NotInMaster => {
                if is_empty_dir(&claude_path) {
                    report.skipped_empty.push((*name).to_string());
                    continue;
                }
                if master_path.exists() {
                    report.blocked.push(format!(
                        "{} already exists in master — manual reconciliation required",
                        master_path.display()
                    ));
                    continue;
                }
                if dry_run {
                    report.moved.push((*name).to_string());
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
                report.moved.push((*name).to_string());
            }
        }
    }

    if !dry_run && !manifest.actions.is_empty() {
        let path = manifest.write(paths)?;
        report.manifest_path = Some(path);
    }
    Ok(report)
}

#[derive(Debug, Serialize, Default)]
pub struct UninstallReport {
    pub restored: Vec<String>,
    pub left_in_place: Vec<String>,
    pub manifest_replayed: Option<PathBuf>,
}

pub fn uninstall(paths: &Paths, keep_master: bool, dry_run: bool) -> Result<UninstallReport> {
    let mut report = UninstallReport::default();
    let manifest_path = latest_master_manifest(paths)?;
    if let Some(p) = &manifest_path {
        if dry_run {
            eprintln!("would replay {}", p.display());
        } else {
            replay_master_manifest(p, paths, keep_master, &mut report)?;
            report.manifest_replayed = Some(p.clone());
        }
    } else {
        // No manifest: best-effort by inspecting current symlinks.
        let status = status(paths)?;
        for item in status.items {
            if item.state == ItemState::Symlinked && !dry_run {
                if !keep_master && item.master_path.exists() {
                    move_path(&item.master_path, &item.claude_path)?;
                    report.restored.push(item.name);
                } else {
                    symlinks::remove(&item.claude_path)?;
                    report.left_in_place.push(item.name);
                }
            }
        }
    }
    Ok(report)
}

fn latest_master_manifest(paths: &Paths) -> Result<Option<PathBuf>> {
    let backups = paths.backups_dir();
    if !backups.exists() {
        return Ok(None);
    }
    let mut best: Option<(String, PathBuf)> = None;
    for entry in fs::read_dir(&backups).map_err(|e| Error::io_at(&backups, e))? {
        let entry = entry.map_err(|e| Error::io_at(&backups, e))?;
        let path = entry.path().join("manifest.json");
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| Error::io_at(&path, e))?;
        let m: Manifest = serde_json::from_slice(&bytes)?;
        if m.op == "master-init" {
            let key = entry.file_name().to_string_lossy().into_owned();
            if best.as_ref().map(|(k, _)| k.as_str() < key.as_str()).unwrap_or(true) {
                best = Some((key, path));
            }
        }
    }
    Ok(best.map(|(_, p)| p))
}

fn replay_master_manifest(
    manifest_path: &Path,
    paths: &Paths,
    keep_master: bool,
    report: &mut UninstallReport,
) -> Result<()> {
    let bytes = fs::read(manifest_path).map_err(|e| Error::io_at(manifest_path, e))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    // Reverse order — undo SymlinkCreate before FsMove.
    let mut actions: Vec<BackupAction> = manifest.actions.clone();
    actions.reverse();
    for action in actions {
        match action {
            BackupAction::SymlinkCreate { link, .. } => {
                if let Err(e) = symlinks::remove(&link) {
                    eprintln!("warn: could not remove symlink {}: {e}", link.display());
                }
            }
            BackupAction::FsMove { from, to } => {
                // `from` is the original (~/.claude/...), `to` is master path. Undo: move
                // master back to ~/.claude.
                if !keep_master {
                    if !from.exists() {
                        if to.exists() {
                            move_path(&to, &from)?;
                            report
                                .restored
                                .push(from.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                        }
                    } else {
                        // Original path already restored (probably user undid manually).
                    }
                } else {
                    report.left_in_place.push(
                        from.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
            }
            _ => {}
        }
    }
    // Optionally clean the empty master dir if everything was restored.
    if !keep_master {
        let master = paths.master_dir();
        if master.exists() && fs::read_dir(&master).map(|i| i.count() == 0).unwrap_or(false) {
            let _ = fs::remove_dir(&master);
        }
    }
    Ok(())
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
