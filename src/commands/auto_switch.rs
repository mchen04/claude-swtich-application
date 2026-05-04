use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::cli::{AutoSwitchArgs, GlobalOpts, OnOff};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::settings::Settings;

pub fn run(paths: &Paths, _global: &GlobalOpts, args: &AutoSwitchArgs) -> Result<()> {
    paths.ensure_cs_home()?;
    let settings_path = paths.cs_settings();
    let mut settings = Settings::load(&settings_path).unwrap_or_default();
    match args.mode {
        None => print_status(&settings, paths),
        Some(OnOff::On) => enable(&mut settings, &settings_path, paths),
        Some(OnOff::Off) => disable(&mut settings, &settings_path, paths),
    }
}

fn print_status(settings: &Settings, paths: &Paths) -> Result<()> {
    let on = if settings.auto_switch { "on" } else { "off" };
    println!("auto-switch: {on}");
    if let Some(ts) = settings.last_switch_unix {
        let dt = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts),
        );
        println!("last switch: {}", dt.to_rfc3339());
    }
    let plist = paths.launch_agents_plist();
    if plist.exists() {
        println!("plist: {}", plist.display());
    } else {
        println!("plist: (not installed)");
    }
    Ok(())
}

fn enable(settings: &mut Settings, settings_path: &Path, paths: &Paths) -> Result<()> {
    let exe = env::current_exe()
        .map_err(|e| Error::Other(format!("could not resolve cs executable path: {e}")))?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);

    let plist_path = paths.launch_agents_plist();
    let log_dir = paths.autoswitch_log_dir();
    fs::create_dir_all(&log_dir).map_err(|e| Error::io_at(&log_dir, e))?;
    let stdout_log = log_dir.join("cs-autoswitch.out.log");
    let stderr_log = log_dir.join("cs-autoswitch.err.log");

    let plist = render_plist(&exe, &paths.home, &stdout_log, &stderr_log);
    crate::jsonio::atomic_write_bytes(&plist_path, plist.as_bytes())?;

    settings.auto_switch = true;
    settings.save(settings_path)?;

    if env::var_os("CS_TEST_NO_LAUNCHCTL").is_none() {
        bootstrap_launchctl(&plist_path)?;
    }
    eprintln!("auto-switch: on");
    eprintln!("plist installed at {}", plist_path.display());
    eprintln!(
        "note: if you move the cs binary, re-run `cs auto-switch on` to refresh the plist"
    );
    Ok(())
}

fn disable(settings: &mut Settings, settings_path: &Path, paths: &Paths) -> Result<()> {
    if env::var_os("CS_TEST_NO_LAUNCHCTL").is_none() {
        bootout_launchctl();
    }
    let plist_path = paths.launch_agents_plist();
    if plist_path.exists() {
        if let Err(e) = fs::remove_file(&plist_path) {
            tracing::warn!(path = %plist_path.display(), error = %e, "could not remove plist");
        }
    }
    settings.auto_switch = false;
    settings.save(settings_path)?;
    eprintln!("auto-switch: off");
    Ok(())
}

fn render_plist(exe: &Path, home: &Path, stdout: &Path, stderr: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.claude-switch.autoswitch</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>__autoswitch-tick</string>
  </array>
  <key>StartInterval</key>
  <integer>300</integer>
  <key>RunAtLoad</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key>
    <string>{}</string>
  </dict>
</dict>
</plist>
"#,
        xml_escape(&exe.to_string_lossy()),
        xml_escape(&stdout.to_string_lossy()),
        xml_escape(&stderr.to_string_lossy()),
        xml_escape(&home.to_string_lossy()),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn bootstrap_launchctl(plist: &Path) -> Result<()> {
    let target = format!("gui/{}", current_uid());
    // bootout first to make this idempotent (no-op if not loaded).
    let _ = Command::new("launchctl")
        .args([
            "bootout",
            &format!("{target}/com.claude-switch.autoswitch"),
        ])
        .output();
    let out = Command::new("launchctl")
        .args(["bootstrap", &target, &plist.to_string_lossy()])
        .output()
        .map_err(|e| Error::Other(format!("launchctl bootstrap failed: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Other(format!(
            "launchctl bootstrap failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

pub(crate) fn bootout_launchctl() {
    let _ = Command::new("launchctl")
        .args([
            "bootout",
            &format!("gui/{}/com.claude-switch.autoswitch", current_uid()),
        ])
        .output();
}

fn current_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}
