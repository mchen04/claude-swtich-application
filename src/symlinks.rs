use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Read the symlink at `path`, returning Some(target) if it is a symlink, None otherwise.
#[allow(dead_code)] // exposed for future cs status / cs links commands
pub fn read(path: &Path) -> Option<PathBuf> {
    fs::read_link(path).ok()
}

/// Atomic-ish symlink replacement: create at a sibling tempfile, then `rename(2)` over
/// the destination. The rename is atomic on the same filesystem.
pub fn replace(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    let tmp = link.with_extension(format!("cs-link-tmp.{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    unix_fs::symlink(target, &tmp).map_err(|e| Error::io_at(&tmp, e))?;
    fs::rename(&tmp, link).map_err(|e| Error::io_at(link, e))?;
    Ok(())
}

/// Remove a symlink without following it. No-op if the path doesn't exist.
pub fn remove(link: &Path) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(m) if m.file_type().is_symlink() => {
            fs::remove_file(link).map_err(|e| Error::io_at(link, e))
        }
        Ok(_) => Err(Error::Other(format!(
            "{} exists but is not a symlink — refusing to remove",
            link.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io_at(link, e)),
    }
}

/// True iff `link` is a symlink whose target (canonicalized) starts with `under`
/// (canonicalized). Used to detect "already migrated" candidates.
pub fn points_into(link: &Path, under: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(link) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let target = match fs::read_link(link) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let absolute = if target.is_relative() {
        link.parent().map(|p| p.join(&target)).unwrap_or(target)
    } else {
        target
    };
    let canon_target = fs::canonicalize(&absolute).unwrap_or(absolute);
    let canon_under = fs::canonicalize(under).unwrap_or_else(|_| under.to_path_buf());
    canon_target.starts_with(canon_under)
}
