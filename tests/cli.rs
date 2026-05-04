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
        map.insert(
            (*acct).to_string(),
            serde_json::Value::String((*blob).to_string()),
        );
    }
    let p = dir.join("keychain-fixture.json");
    std::fs::write(
        &p,
        serde_json::to_vec(&serde_json::Value::Object(map)).unwrap(),
    )
    .unwrap();
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
    cs().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude profile switching"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("master"))
        .stdout(predicate::str::contains("usage"));
}

#[test]
fn shows_version() {
    cs().arg("--version").assert().success();
}

#[test]
fn doctor_runs_in_isolated_env() {
    let (_dir, claude_home, cs_home) = isolated();
    cs().env("CLAUDE_HOME", &claude_home)
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
    cs().env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("cs doctor"))
        .stdout(predicate::str::contains("Tooling"));
}

#[test]
fn unknown_name_errors_with_not_found() {
    let (_dir, claude_home, cs_home) = isolated();
    cs().env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("does-not-exist-yet")
        .assert()
        .failure()
        .stderr(predicate::str::contains("profile not found"));
}

#[test]
fn list_empty_text() {
    let (_dir, claude_home, cs_home) = isolated();
    cs().env("CLAUDE_HOME", &claude_home)
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
    cs().env("CLAUDE_HOME", &claude_home)
        .env("CS_HOME", &cs_home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("no active profile"));
}

// --- switch + profile management round-trip -----------------------------------

fn phase_c_env(
    claude_home: &std::path::Path,
    cs_home: &std::path::Path,
    fixture: &std::path::Path,
) -> Command {
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
    std::fs::write(claude_home.join("settings.json"), b"{\"theme\":\"dark\"}\n").unwrap();

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

    assert_eq!(
        std::fs::read(cs_home.join("profiles/personal/settings.json")).unwrap(),
        b"{\"theme\":\"dark\"}\n"
    );
}

#[test]
fn save_overwrites_existing() {
    let (dir, claude_home, cs_home) = isolated();
    let first_blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &first_blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success()
        .stderr(predicate::str::contains("saved profile"));

    // Replace the canonical entry with a different account, then re-save the same profile name.
    let second_blob = fake_oauth("rotated@example.com", 7200);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &second_blob),
            ("Claude Code-credentials-personal", &first_blob),
        ],
    );

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success()
        .stderr(predicate::str::contains("overwrote profile"));

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rotated@example.com"))
        .stdout(predicate::str::contains("primary@example.com").not());
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
        canonical_now["Claude Code-credentials-personal"]
            .as_str()
            .unwrap()
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
    assert!(!cs_home.join("profiles/work").exists());
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
        &[("test-user", &blob), ("Claude Code-credentials-a", &blob)],
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

// --- master profile -----------------------------------------------------------

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
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.insert(
                    format!("L:{rel}"),
                    target.to_string_lossy().into_owned().into_bytes(),
                );
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

fn master_env(
    claude_home: &std::path::Path,
    cs_home: &std::path::Path,
    fixture: &std::path::Path,
) -> Command {
    let mut c = cs();
    c.env("CLAUDE_HOME", claude_home)
        .env("CS_HOME", cs_home)
        .env("CS_TEST_KEYCHAIN_FIXTURE", fixture);
    c
}

fn seeded_master_setup() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let (dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);
    let blob = fake_oauth("personal@example.com", 3600);
    // Only the canonical entry — `cs save personal` will create the profile entry.
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);
    (dir, claude_home, cs_home, fixture)
}

#[test]
fn master_set_then_uninstall_is_byte_clean() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;
    let before = dir_snapshot(&claude_home);

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();

    // Validate symlinks now exist and point into the personal profile dir.
    let target = std::fs::read_link(claude_home.join("skills")).unwrap();
    assert!(
        target.starts_with(cs_home.join("profiles/personal")),
        "skills symlink should point into profiles/personal: {}",
        target.display()
    );
    assert!(std::fs::symlink_metadata(claude_home.join("CLAUDE.md"))
        .unwrap()
        .file_type()
        .is_symlink());

    master_env(&claude_home, &cs_home, &fixture)
        .args(["uninstall"])
        .assert()
        .success();

    let after = dir_snapshot(&claude_home);
    assert_eq!(before, after, "master set→uninstall is not byte-clean");
}

#[test]
fn master_set_idempotent() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();
    // Second invocation: same master, no-op.
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already symlinked"));
}

#[test]
fn master_status_reports_designated_master() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();

    let output = master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(v["master"], "personal");
    assert_eq!(v["items"].as_array().unwrap().len(), 4);
}

#[test]
fn master_change_moves_content() {
    let (dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);
    let blob = fake_oauth("a@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "work"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();

    // Switch master to work — work has none of the four candidates.
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "work"])
        .assert()
        .success();

    let target = std::fs::read_link(claude_home.join("skills")).unwrap();
    assert!(
        target.starts_with(cs_home.join("profiles/work")),
        "skills should now point into work: {}",
        target.display()
    );
    assert!(cs_home.join("profiles/work/skills/foo/SKILL.md").exists());
    assert!(!cs_home.join("profiles/personal/skills").exists());

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["master"], "work");
}

#[test]
fn master_change_refuses_when_target_non_empty() {
    let (dir, claude_home, cs_home) = isolated();
    write_seed(&claude_home);
    let blob = fake_oauth("a@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "work"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();

    // Manually plant content in the work profile dir to block the change.
    std::fs::create_dir_all(cs_home.join("profiles/work/skills/blocker")).unwrap();
    std::fs::write(
        cs_home.join("profiles/work/skills/blocker/SKILL.md"),
        b"blocker\n",
    )
    .unwrap();

    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "work"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn rm_master_profile_refuses() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();

    master_env(&claude_home, &cs_home, &fixture)
        .args(["rm", "personal"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("master profile"))
        .stderr(predicate::str::contains("cs master --unset"));
}

#[test]
fn rename_master_profile_updates_state_and_symlinks() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["rename", "personal", "personal2"])
        .assert()
        .success();

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert_eq!(state["master"], "personal2");

    let target = std::fs::read_link(claude_home.join("skills")).unwrap();
    assert!(
        target.starts_with(cs_home.join("profiles/personal2")),
        "skills should now point into profiles/personal2: {}",
        target.display()
    );
    assert!(cs_home
        .join("profiles/personal2/skills/foo/SKILL.md")
        .exists());
}

#[test]
fn master_unset_restores_claude_home() {
    let (dir, claude_home, cs_home, fixture) = seeded_master_setup();
    let _ = dir;
    let before = dir_snapshot(&claude_home);

    master_env(&claude_home, &cs_home, &fixture)
        .args(["save", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "personal"])
        .assert()
        .success();
    master_env(&claude_home, &cs_home, &fixture)
        .args(["master", "--unset"])
        .assert()
        .success();

    // ~/.claude should be back to the seeded state (no symlinks).
    assert!(!std::fs::symlink_metadata(claude_home.join("skills"))
        .unwrap()
        .file_type()
        .is_symlink());
    let after = dir_snapshot(&claude_home);
    assert_eq!(before, after, "master --unset is not byte-clean");

    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cs_home.join("state.json")).unwrap()).unwrap();
    assert!(state["master"].is_null());
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

// --- usage % view -------------------------------------------------------------

#[test]
fn save_rejects_path_traversal_name() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", "foo/bar"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid profile name"));
}

#[test]
fn save_rejects_dotfile_name() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(dir.path(), &[("test-user", &blob)]);

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["save", ".dotfile"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid profile name"));
}

#[test]
fn rm_refuses_to_delete_through_symlinked_profile_dir() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("primary@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &blob),
            ("Claude Code-credentials-sneaky", &blob),
        ],
    );

    // Plant a real directory somewhere outside cs_home, then symlink the
    // profile dir to it. `cs rm sneaky` must refuse rather than chase
    // the symlink and `rm -rf` the real target.
    let outside = dir.path().join("outside-target");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("important.txt"), b"do not delete").unwrap();

    let profiles = cs_home.join("profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    let link = profiles.join("sneaky");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["rm", "sneaky"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));

    assert!(outside.join("important.txt").exists(),
        "rm chased a symlink and deleted the real target");
    assert!(link.exists(), "symlink itself should be intact");
}

#[test]
fn refresh_kills_claude_after_timeout() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("work@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &blob),
            ("Claude Code-credentials-work", &blob),
        ],
    );

    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let shim = bin_dir.join("claude");
    std::fs::write(&shim, "#!/bin/sh\nsleep 9999\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&shim).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim, perms).unwrap();
    }

    // Prepend the shim dir so `claude` resolves to it; keep /bin and
    // /usr/bin so the shim itself can find `sleep`.
    let mut path = bin_dir.as_os_str().to_owned();
    path.push(":/usr/bin:/bin");

    let started = std::time::Instant::now();
    phase_c_env(&claude_home, &cs_home, &fixture)
        .env("PATH", &path)
        .args(["refresh", "work"])
        .timeout(std::time::Duration::from_secs(70))
        .assert()
        .failure()
        .stderr(predicate::str::contains("timed out"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(70),
        "refresh did not bound the subprocess (took {:?})",
        started.elapsed()
    );
}

#[test]
fn setup_refuses_when_rc_is_unreadable_and_leaves_it_intact() {
    // A `.zshrc` with invalid UTF-8 must abort `cs setup` with an error,
    // not silently overwrite the user's file with a blank wrapper.
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let rc = home.join(".zshrc");
    let original: &[u8] = &[0xFF, 0xFE, b'\n'];
    std::fs::write(&rc, original).unwrap();

    cs().env("HOME", &home)
        .args(["setup", "--shell", "zsh"])
        .assert()
        .failure();

    assert_eq!(std::fs::read(&rc).unwrap(), original);
}

#[test]
fn usage_default_shows_pct_columns() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("work@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &blob),
            ("Claude Code-credentials-work", &blob),
        ],
    );

    let limits_dir = dir.path().join("limits");
    std::fs::create_dir_all(&limits_dir).unwrap();
    std::fs::write(
        limits_dir.join("work.json"),
        br#"{
            "five_hour":  { "utilization": 37, "resets_at": "2099-01-01T00:00:00Z" },
            "seven_day":  { "utilization": 64, "resets_at": "2099-01-01T00:00:00Z" },
            "seven_day_sonnet": null,
            "seven_day_opus":   null,
            "extra_usage":      { "is_enabled": false }
        }"#,
    )
    .unwrap();

    let json = phase_c_env(&claude_home, &cs_home, &fixture)
        .env("CS_TEST_LIMITS_FIXTURE", &limits_dir)
        .args(["usage", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json).expect("valid json");
    let row = &v["rows"][0];
    assert_eq!(row["profile"], "work");
    assert_eq!(row["five_h_pct_left"], 63);
    assert_eq!(row["weekly_pct_left"], 36);
    assert!(row["error"].is_null());

    let text_out = phase_c_env(&claude_home, &cs_home, &fixture)
        .env("CS_TEST_LIMITS_FIXTURE", &limits_dir)
        .arg("usage")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(text_out).unwrap();
    assert!(text.contains("5H LEFT"), "missing 5H LEFT header: {text}");
    assert!(text.contains("63%"), "missing 63% cell: {text}");
}

#[test]
fn usage_token_expired_shows_dash() {
    let (dir, claude_home, cs_home) = isolated();
    let canonical = fake_oauth("primary@example.com", 3600);
    // The work profile's OAuth blob is already expired.
    let work_expired = fake_oauth("work@example.com", -3_600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &canonical),
            ("Claude Code-credentials-work", &work_expired),
        ],
    );

    let json = phase_c_env(&claude_home, &cs_home, &fixture)
        .args(["usage", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json).expect("valid json");
    let row = &v["rows"][0];
    assert_eq!(row["profile"], "work");
    assert!(row["five_h_pct_left"].is_null());
    let err = row["error"].as_str().expect("error string set");
    assert!(err.contains("token expired"), "unexpected error: {err}");
    assert!(err.contains("cs refresh"), "missing refresh hint: {err}");
}

#[test]
fn usage_rate_limited_serves_cached_then_warns() {
    let (dir, claude_home, cs_home) = isolated();
    let blob = fake_oauth("work@example.com", 3600);
    let fixture = fixture_path(
        dir.path(),
        &[
            ("test-user", &blob),
            ("Claude Code-credentials-work", &blob),
        ],
    );

    // Prime the on-disk cache so rate_limited can fall back to it.
    let cache_dir = cs_home.join("cache").join("usage-limits");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("work.json"),
        br#"{
            "fetched_at_unix": 1700000000,
            "payload": {
                "five_hour": { "utilization": 20, "resets_at": null },
                "seven_day": { "utilization": 50, "resets_at": null },
                "seven_day_sonnet": null,
                "seven_day_opus": null
            }
        }"#,
    )
    .unwrap();

    let fail_dir = dir.path().join("fail");
    std::fs::create_dir_all(&fail_dir).unwrap();
    std::fs::write(fail_dir.join("work.txt"), b"rate_limited").unwrap();

    let json = phase_c_env(&claude_home, &cs_home, &fixture)
        .env("CS_TEST_LIMITS_FAIL", &fail_dir)
        .args(["usage", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json).expect("valid json");
    let row = &v["rows"][0];
    assert_eq!(row["five_h_pct_left"], 80);
    assert_eq!(row["weekly_pct_left"], 50);
    assert!(row["error"].is_null());
    let warnings = v["warnings"].as_array().expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("rate-limited")),
        "expected rate-limited warning: {warnings:?}"
    );
}
