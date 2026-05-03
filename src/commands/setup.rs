use std::fs;

use crate::cli::{GlobalOpts, SetupArgs};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::shell::{self, Shell};

pub fn run(_paths: &Paths, _global: &GlobalOpts, args: &SetupArgs) -> Result<()> {
    let shell = Shell::detect(args.shell)?;
    let rc = shell
        .rc_path()
        .ok_or_else(|| Error::Config("HOME unset".into()))?;
    let existing = fs::read_to_string(&rc).unwrap_or_default();
    let updated = shell::upsert_block(&existing, shell.snippet());

    if !args.non_interactive && existing == updated {
        eprintln!(
            "{} already contains the cs wrapper; nothing to do",
            rc.display()
        );
        return Ok(());
    }
    fs::write(&rc, &updated).map_err(|e| Error::io_at(&rc, e))?;
    eprintln!("installed cs wrapper into {}", rc.display());
    eprintln!(
        "restart your shell or `source {}` to activate",
        rc.display()
    );
    Ok(())
}
