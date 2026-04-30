use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn cs() -> Command {
    let mut cmd = Command::cargo_bin("cs").expect("binary built");
    cmd.env("CS_TEST_KEYCHAIN", "1");
    // Tests should never read the real $USER's canonical Keychain entry — pin a value
    // for determinism.
    cmd.env("USER", "test-user");
    cmd
}

fn isolated() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let claude_home = dir.path().join("claude");
    let cs_home = dir.path().join("cs-home");
    std::fs::create_dir_all(&claude_home).unwrap();
    std::fs::create_dir_all(&cs_home).unwrap();
    (dir, claude_home, cs_home)
}

/// Each test gets a fresh shared mock keychain by setting CS_TEST_KEYCHAIN_FIXTURE to a
/// JSON file the binary loads at startup. We pre-seed the canonical entry with a valid
/// OAuth blob.
fn fixture_path(dir: &std::path::Path, blobs: &[(&str, &str)]) -> PathBuf {
    let mut map = serde_json::Map::new();
    for (acct, blob) in blobs {
        map.insert((*acct).to_string(), serde_json::Value::String((*blob).to_string()));
    }
    let p = dir.join("keychain-fixture.json");
    std::fs::write(&p, serde_json::to_vec(&serde_json::Value::Object(map)).unwrap()).unwrap();
    p
}

fn fake_oauth(email: &str, expires_in_secs: i64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let exp = now_ms + expires_in_secs * 1000;
    serde_json::json!({
        "claudeAiOauth": {
            "accessToken": format!("tok-{email}"),
            "refreshToken": format!("ref-{email}"),
            "expiresAt": exp,
            "scopes": ["user:profile"],
            "subscriptionType": "max",
            "email": email
        }
    })
    .to_string()
}

#[test]
fn shows_help() {
    cs()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code account switching"))
        .stdout(predicate::str::contains("doctor"));
}

#[test]
fn shows_version() {
    cs().arg("--version").assert().success();
}

#[test]
fn doctor_runs_in_isolated_env() {
    let (_dir, claude_home, cs_home) = isolated();
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("doctor")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"backend\""))
        .stdout(predicate::str::contains("\"tooling\""));
}

#[test]
fn doctor_text_runs_in_isolated_env() {
    let (_dir, claude_home, cs_home) = isolated();
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("cs doctor"))
        .stdout(predicate::str::contains("Tooling"));
}

#[test]
fn tui_stub_prints_friendly_message() {
    cs().arg("tui").assert().success();
}

#[test]
fn unknown_name_errors_with_not_found() {
    let (_dir, claude_home, cs_home) = isolated();
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("does-not-exist-yet")
        .assert()
        .failure()
        .stderr(predicate::str::contains("profile not found"));
}

#[test]
fn list_empty_text() {
    let (_dir, claude_home, cs_home) = isolated();
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("no profiles saved"));
}

#[test]
fn list_empty_json_schema() {
    let (_dir, claude_home, cs_home) = isolated();
    let output = cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
    assert!(v.get("active").is_some(), "missing active");
    assert!(v.get("default").is_some(), "missing default");
    assert!(v.get("profiles").is_some(), "missing profiles");
    assert!(v["profiles"].as_array().unwrap().is_empty());
}

#[test]
fn status_no_active_text() {
    let (_dir, claude_home, cs_home) = isolated();
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("no active profile"));
}

// --- Phase C: switch + profile management round-trip --------------------------

fn phase_c_env(claude_home: &std::path::Path, cs_home: &std::path::Path, fixture: &std::path::Path) -> Command {
    let mut c = cs();
    c.env("CLAUDE_HOME", claude_home)
        .env("CS_HOME", cs_home)
        .env("CS_TEST_KEYCHAIN_FIXTURE", fixture);
    c
}

#[test]
fn save_round_trip() {
    let (dir, claude_home, cs_home) = isolated();
    let canonical_blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &canonical_blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("personal"))
        .stdout(predicate::str::contains("primary@example.com"));
}

#[test]
fn save_refuses_overwrite() {
    let (dir, claude_home, cs_home) = isolated();
    let canonical_blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &canonical_blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .failure();
}

#[test]
fn switch_changes_canonical_and_state() {
    let (dir, claude_home, cs_home) = isolated();
    let work_blob = fake_oauth("work@example.com", 3600);
    let personal_blob = fake_oauth("personal@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &work_blob),
            ("Claude Code-credentials-personal", &personal_blob),
            ("Claude Code-credentials-work", &work_blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["personal"])
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["active"], "personal");

    let canonical_now: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture).unwrap()).unwrap();
    assert_eq!(
        canonical_now["test-user"].as_str().unwrap(),
        canonical_now["Claude Code-credentials-personal"].as_str().unwrap()
    );
}

#[test]
fn switch_previous_toggles() {
    let (dir, claude_home, cs_home) = isolated();
    let work_blob = fake_oauth("work@example.com", 3600);
    let personal_blob = fake_oauth("personal@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &work_blob),
            ("Claude Code-credentials-personal", &personal_blob),
            ("Claude Code-credentials-work", &work_blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["personal"])
        .assert()
        .success();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["work"])
        .assert()
        .success();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .arg("-")
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["active"], "personal");
    assert_eq!(state["previous"], "work");
}

#[test]
fn rm_deletes_profile_and_clears_active() {
    let (dir, claude_home, cs_home) = isolated();
    let work_blob = fake_oauth("work@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &work_blob),
            ("Claude Code-credentials-work", &work_blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["work"])
        .assert()
        .success();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["rm", "work"])
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert!(state["active"].is_null());
}

#[test]
fn rename_preserves_active_pointer() {
    let (dir, claude_home, cs_home) = isolated();
    let work_blob = fake_oauth("work@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &work_blob),
            ("Claude Code-credentials-work", &work_blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["work"])
        .assert()
        .success();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["rename", "work", "office"])
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["active"], "office");
}

#[test]
fn default_then_default_go() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("a@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &blob),
            ("Claude Code-credentials-a", &blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["default", "a"])
        .assert()
        .success();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["default-go"])
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["active"], "a");
    assert_eq!(state["default"], "a");
}

// --- Phase D: master init / uninstall round-trip ------------------------------

fn write_seed(claude_home: &std::path::Path) {
    std::fs::create_dir_all(claude_home.join("skills/foo")).unwrap();
    std::fs::write(claude_home.join("skills/foo/SKILL.md"), b"# foo skill\n").unwrap();
    std::fs::create_dir_all(claude_home.join("commands")).unwrap();
    std::fs::write(claude_home.join("commands/hello.md"), b"hello command\n").unwrap();
    std::fs::write(claude_home.join("CLAUDE.md"), b"top level\n").unwrap();
    // commands has content, agents/ does not exist (matches real machine).
}

fn dir_snapshot(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::collections::BTreeMap;
    let mut map = BTreeMap::new();
    fn walk(root: &std::path::Path, base: &std::path::Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(base).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.insert(format!("L:{rel}"), target.to_string_lossy().into_owned().into_bytes());
            } else if meta.file_type().is_dir() {
                out.insert(format!("D:{rel}"), Vec::new());
                walk(root, &path, out);
            } else {
                out.insert(format!("F:{rel}"), std::fs::read(&path).unwrap());
            }
        }
    }
    walk(root, root, &mut map);
    map
}

#[test]
fn master_init_then_uninstall_is_byte_clean() {
    let (_dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);
    let before = dir_snapshot(&claude_home);

    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["master", "init"])
        .assert()
        .success();

    // Validate symlinks now exist.
    assert!(std::fs::symlink_metadata(claude_home.join("skills"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(std::fs::symlink_metadata(claude_home.join("commands"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(std::fs::symlink_metadata(claude_home.join("CLAUDE.md"))
        .unwrap()
        .file_type()
        .is_symlink());

    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["uninstall"])
        .assert()
        .success();

    let after = dir_snapshot(&claude_home);
    assert_eq!(before, after, "init→uninstall is not byte-clean");
}

#[test]
fn master_init_idempotent() {
    let (_dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);

    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["master", "init"])
        .assert()
        .success();
    // Second init should not move anything; should report "already symlinked".
    cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["master", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already symlinked"));
}

#[test]
fn master_status_reports_states() {
    let (_dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);

    let output = cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["master", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(v["items"].is_array());
    assert_eq!(v["items"].as_array().unwrap().len(), 4);
}

#[test]
fn dry_run_save_does_not_mutate() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("a@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["--dry-run", "save", "personal"])
        .assert()
        .success();

    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture).unwrap()).unwrap();
    assert!(after.get("Claude Code-credentials-personal").is_none());
}

#[test]
fn status_no_active_json_shape() {
    let (_dir, claude_home, cs_home) = isolated();
    let output = cs()
        .env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
    for k in ["active", "default", "previous", "asked_about"] {
        assert!(v.get(k).is_some(), "missing {k}");
    }
}
