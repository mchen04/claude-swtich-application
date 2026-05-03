use std::fmt::Display;
use std::io::{self, Write};

use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputOpts {
    pub json: bool,
}

pub fn emit<T: Serialize + Display>(opts: OutputOpts, value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if opts.json {
        serde_json::to_writer_pretty(&mut out, value)?;
        out.write_all(b"\n")?;
    } else {
        writeln!(out, "{value}")?;
    }
    Ok(())
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    out.write_all(b"\n")?;
    Ok(())
}

pub fn emit_text<T: Display>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{value}")?;
    Ok(())
}
