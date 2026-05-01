use std::fs::{File, OpenOptions};

use fs2::FileExt;

use crate::error::{Error, Result};
use crate::paths::Paths;

/// Advisory exclusive lock on `~/.claude-cs/.lock`. Held until the guard drops.
pub struct CsLock {
    _file: File,
}

impl CsLock {
    pub fn acquire(paths: &Paths) -> Result<Self> {
        paths.ensure_cs_home()?;
        let path = paths.lock_file();
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::io_at(&path, e))?;
        file.try_lock_exclusive().map_err(|_| {
            Error::Refused(format!(
                "another `cs` write is in progress (lock held: {})",
                path.display()
            ))
        })?;
        Ok(Self { _file: file })
    }
}
