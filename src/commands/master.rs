use std::fmt;

use crate::cli::{GlobalOpts, MasterCmd};
use crate::error::Result;
use crate::lock::CsLock;
use crate::master;
use crate::output::{emit_json, emit_text, OutputOpts};
use crate::paths::Paths;

pub fn run(paths: &Paths, global: &GlobalOpts, cmd: &MasterCmd) -> Result<()> {
    match cmd {
        MasterCmd::Init => {
            if !global.dry_run {
                let _lock = CsLock::acquire(paths)?;
                let report = master::init(paths, false)?;
                if global.json {
                    emit_json(&report)?;
                } else {
                    emit_text(OutputOpts { json: false, no_color: global.no_color }, &TextInit(&report))?;
                }
                Ok(())
            } else {
                let report = master::init(paths, true)?;
                if global.json {
                    emit_json(&report)?;
                } else {
                    emit_text(OutputOpts { json: false, no_color: global.no_color }, &TextInit(&report))?;
                }
                Ok(())
            }
        }
        MasterCmd::Status => {
            let st = master::status(paths)?;
            if global.json {
                emit_json(&st)?;
            } else {
                emit_text(OutputOpts { json: false, no_color: global.no_color }, &TextStatus(&st))?;
            }
            Ok(())
        }
    }
}

struct TextInit<'a>(&'a master::InitReport);
impl fmt::Display for TextInit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = self.0;
        if !r.moved.is_empty() {
            writeln!(f, "moved -> master:")?;
            for n in &r.moved {
                writeln!(f, "  {n}")?;
            }
        }
        if !r.already.is_empty() {
            writeln!(f, "already symlinked: {}", r.already.join(", "))?;
        }
        if !r.skipped_empty.is_empty() {
            writeln!(f, "skipped (missing/empty): {}", r.skipped_empty.join(", "))?;
        }
        if !r.blocked.is_empty() {
            writeln!(f, "blocked:")?;
            for b in &r.blocked {
                writeln!(f, "  ! {b}")?;
            }
        }
        if let Some(p) = &r.manifest_path {
            writeln!(f, "manifest: {}", p.display())?;
        }
        Ok(())
    }
}

struct TextStatus<'a>(&'a master::MasterStatus);
impl fmt::Display for TextStatus<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "master: {}", self.0.master_dir.display())?;
        for item in &self.0.items {
            writeln!(f, "  {:<12} {:?}  {}", item.name, item.state, item.claude_path.display())?;
        }
        Ok(())
    }
}
