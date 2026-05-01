use crate::cli::{DoctorArgs, GlobalOpts};
use crate::doctor;
use crate::error::Result;
use crate::keychain::Keychain;
use crate::output::{emit_json, OutputOpts};
use crate::paths::Paths;

pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    _args: &DoctorArgs,
) -> Result<()> {
    let report = doctor::run(paths, kc)?;
    if global.json {
        emit_json(&report)?;
    } else {
        let opts = OutputOpts {
            json: false,
            no_color: global.no_color,
        };
        crate::output::emit(opts, &report)?;
    }
    Ok(())
}
