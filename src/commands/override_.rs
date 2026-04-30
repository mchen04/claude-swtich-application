use std::fs;

use crate::cli::{GlobalOpts, OverrideArgs};
use crate::error::{Error, Result};
use crate::lock::CsLock;
use crate::paths::Paths;
use crate::symlinks;

pub fn add(paths: &Paths, global: &GlobalOpts, args: &OverrideArgs) -> Result<()> {
    let master_path = paths.master_dir().join(&args.path);
    if !master_path.exists() {
        return Err(Error::Other(format!(
            "{} not found in master — run `cs master init` first",
            master_path.display()
        )));
    }
    let override_path = paths.profile_dir(&args.profile).join("overrides").join(&args.path);

    if global.dry_run {
        eprintln!(
            "would copy {} -> {} (override active when profile is `{}`)",
            master_path.display(),
            override_path.display(),
            args.profile
        );
        return Ok(());
    }

    let _lock = CsLock::acquire(paths)?;
    if let Some(parent) = override_path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
    }
    copy_recursive(&master_path, &override_path)?;
    eprintln!(
        "override created at {}; activates on next switch to `{}`",
        override_path.display(),
        args.profile
    );
    Ok(())
}

pub fn drop(paths: &Paths, global: &GlobalOpts, args: &OverrideArgs) -> Result<()> {
    let override_path = paths.profile_dir(&args.profile).join("overrides").join(&args.path);
    if !override_path.exists() {
        return Err(Error::Other(format!(
            "no override at {}",
            override_path.display()
        )));
    }
    if global.dry_run {
        eprintln!("would remove override {}", override_path.display());
        return Ok(());
    }
    let _lock = CsLock::acquire(paths)?;
    remove_recursive(&override_path)?;
    let claude_link = paths.claude_home.join(&args.path);
    let master_path = paths.master_dir().join(&args.path);
    if master_path.exists() {
        symlinks::replace(&master_path, &claude_link)?;
    }
    eprintln!("removed override {}", override_path.display());
    Ok(())
}

fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).map_err(|e| Error::io_at(src, e))?;
    if meta.file_type().is_dir() {
        fs::create_dir_all(dst).map_err(|e| Error::io_at(dst, e))?;
        for entry in fs::read_dir(src).map_err(|e| Error::io_at(src, e))? {
            let entry = entry.map_err(|e| Error::io_at(src, e))?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(src).map_err(|e| Error::io_at(src, e))?;
        std::os::unix::fs::symlink(&target, dst).map_err(|e| Error::io_at(dst, e))?;
    } else {
        fs::copy(src, dst).map_err(|e| Error::io_at(dst, e))?;
    }
    Ok(())
}

fn remove_recursive(p: &std::path::Path) -> Result<()> {
    let meta = fs::symlink_metadata(p).map_err(|e| Error::io_at(p, e))?;
    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(p).map_err(|e| Error::io_at(p, e))
    } else {
        fs::remove_file(p).map_err(|e| Error::io_at(p, e))
    }
}
