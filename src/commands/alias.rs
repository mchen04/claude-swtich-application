use std::fs;

use crate::cli::{AliasArgs, GlobalOpts};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::shell::{self, Shell};

/// `cs alias <name>` writes a shell alias that runs `cs <name>` in the user's rc file.
/// We add a separate cs-managed block per alias name to keep diffs surgical.
pub fn run(_paths: &Paths, global: &GlobalOpts, args: &AliasArgs) -> Result<()> {
    let shell = Shell::detect(args.shell)?;
    let rc = shell
        .rc_path()
        .ok_or_else(|| Error::Config("HOME unset".into()))?;

    let alias_line = format!("alias {n}='cs {n}'", n = args.name);
    let body = format!("{alias_line}\n");
    let marker_begin = format!("# >>> cs alias {} >>>", args.name);
    let marker_end = format!("# <<< cs alias {} <<<", args.name);

    let existing = fs::read_to_string(&rc).unwrap_or_default();
    let updated = shell::upsert_block_named(&existing, &marker_begin, &marker_end, &body);

    if global.dry_run {
        eprintln!(
            "would add alias `{}` -> `cs {}` in {}",
            args.name,
            args.name,
            rc.display()
        );
        return Ok(());
    }

    fs::write(&rc, &updated).map_err(|e| Error::io_at(&rc, e))?;
    eprintln!("added alias `{}` to {}", args.name, rc.display());
    Ok(())
}
