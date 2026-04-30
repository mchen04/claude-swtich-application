use crate::cli::GlobalOpts;
use crate::dashboard;
use crate::error::Result;
use crate::keychain::Keychain;
use crate::output::{emit_json, emit_text, OutputOpts};
use crate::paths::Paths;
use crate::usage::ccusage::CcusageClient;

pub fn run(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts) -> Result<()> {
    let client = CcusageClient::new();
    let snap = dashboard::snapshot(paths, kc, Some(&client))?;
    if global.json {
        emit_json(&snap)?;
    } else {
        emit_text(
            OutputOpts { json: false, no_color: global.no_color },
            &snap,
        )?;
    }
    Ok(())
}
