use crate::error::Result;

pub fn run() -> Result<()> {
    eprintln!("cs tui: TUI is deferred to Phase F. Use `cs usage --watch` for a live view (Phase E).");
    Ok(())
}
