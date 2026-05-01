use std::fs;

use crate::cli::{AliasArgs, GlobalOpts};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::shell::Shell;

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
    let updated = upsert_block_named(&existing, &marker_begin, &marker_end, &body);

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

fn upsert_block_named(existing: &str, begin: &str, end: &str, body: &str) -> String {
    if let (Some(start), Some(stop)) = (existing.find(begin), existing.find(end)) {
        if start < stop {
            let end_line = existing[stop..]
                .find('\n')
                .map(|n| stop + n + 1)
                .unwrap_or(existing.len());
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..start]);
            out.push_str(begin);
            out.push('\n');
            out.push_str(body.trim_end_matches('\n'));
            out.push('\n');
            out.push_str(end);
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
    out.push_str(begin);
    out.push('\n');
    out.push_str(body.trim_end_matches('\n'));
    out.push('\n');
    out.push_str(end);
    out.push('\n');
    out
}
