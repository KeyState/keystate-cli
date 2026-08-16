//! Integration tests: run the real `keystate` binary against a live
//! Keycloak + Postgres stack and verify its output.
//!
//! These are `#[ignore]`d because they need the docker-compose stack:
//!
//! ```text
//! docker compose up -d --wait --wait-timeout 300
//! cargo test --test integration -- --ignored
//! docker compose down -v
//! ```
//!
//! The tests exercise the binary end to end and *verify the output artifacts*:
//! config.json (sorted, volatile-free), report.json (complete), byte-identical
//! re-extraction, and `--check` drift semantics with exit code 3.

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

/// The compose stack's Postgres URL (see docker-compose.yml).
const DB_URL: &str = "postgres://keycloak:keycloak@localhost:5432/keycloak";
const REALM: &str = "master";

fn keystate() -> Command {
    Command::cargo_bin("keystate").expect("binary builds")
}

/// Run a plain extraction into `output` using env credentials.
fn extract_to(output: &Path) {
    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args([
            "extract",
            "--backend",
            "keycloak",
            "--realm",
            REALM,
            "--output",
        ])
        .arg(output)
        .assert()
        .success();
}

/// The realm's output root: `<output>/keycloak/<realm>`.
fn root(output: &Path) -> std::path::PathBuf {
    output.join("keycloak").join(REALM)
}

#[test]
#[ignore = "requires the docker-compose stack (docker compose up -d --wait)"]
fn extract_writes_config_and_report_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    extract_to(dir.path());

    let latest = std::fs::read_to_string(root(dir.path()).join("latest")).expect("latest pointer");
    let run = root(dir.path()).join(latest.trim());

    let config_path = run.join("config.json");
    let report_path = run.join("report.json");
    assert!(config_path.is_file(), "config.json missing");
    assert!(report_path.is_file(), "report.json missing");

    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    assert!(
        config.get("volatile").is_none(),
        "volatile leaked into config"
    );
    assert_eq!(config["backend"]["backend"], "keycloak");
    assert_eq!(config["realm"]["name"], REALM);
    assert!(
        config["realm"]["enabled"] == serde_json::json!(true),
        "master realm should be enabled"
    );

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["backend"], "keycloak");
    assert_eq!(
        report["issues"],
        serde_json::json!([]),
        "a default realm must extract completely"
    );
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn re_extraction_is_byte_identical_and_history_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    extract_to(dir.path());
    extract_to(dir.path());

    let root = root(dir.path());
    let runs: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    assert!(
        runs.len() >= 2,
        "expected timestamped history, got: {runs:?}"
    );

    let latest = std::fs::read_to_string(root.join("latest")).unwrap();
    let latest_dir = root.join(latest.trim());
    let previous = runs.iter().find(|p| *p != &latest_dir).expect("prior run");
    assert_eq!(
        std::fs::read(latest_dir.join("config.json")).unwrap(),
        std::fs::read(previous.join("config.json")).unwrap(),
        "re-running against an unchanged source must be byte-identical"
    );
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn check_on_a_fresh_directory_reports_new_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args([
            "extract",
            "--backend",
            "keycloak",
            "--realm",
            REALM,
            "--output",
        ])
        .arg(dir.path())
        .arg("--check")
        .arg("--output-format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"check\":\"new\""))
        .stdout(predicate::str::contains("\"status\":\"ok\""));
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn check_reports_unchanged_then_drift_with_exit_code_3() {
    let dir = tempfile::tempdir().unwrap();
    extract_to(dir.path());

    // No baseline change -> unchanged, exit 0, JSON status.
    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args([
            "extract",
            "--backend",
            "keycloak",
            "--realm",
            REALM,
            "--output",
        ])
        .arg(dir.path())
        .arg("--check")
        .arg("--output-format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"check\":\"unchanged\""))
        .stdout(predicate::str::contains("\"status\":\"ok\""));

    // Corrupt the stored config -> would-change, exit code 3.
    let latest = std::fs::read_to_string(root(dir.path()).join("latest")).unwrap();
    std::fs::write(
        root(dir.path()).join(latest.trim()).join("config.json"),
        b"{\"mutated\": true}",
    )
    .unwrap();

    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args([
            "extract",
            "--backend",
            "keycloak",
            "--realm",
            REALM,
            "--output",
        ])
        .arg(dir.path())
        .arg("--check")
        .arg("--output-format")
        .arg("json")
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"check\":\"would-change\""))
        .stdout(predicate::str::contains("\"status\":\"drift\""));
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn config_file_with_env_interpolation_drives_extraction() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("keystate.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[backend]
name = "keycloak"

[extract]
realm = "master"
db_url = "${{KEYSTATE_TEST_DB_URL}}"

[output]
directory = "{}"
"#,
            dir.path().join("out").display()
        ),
    )
    .unwrap();

    keystate()
        .env("KEYSTATE_TEST_DB_URL", DB_URL)
        .args(["extract", "--config"])
        .arg(&config_path)
        .assert()
        .success();

    let out_root = dir.path().join("out");
    assert!(
        root(&out_root).join("latest").is_file(),
        "config-file run must write output"
    );
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn invalid_realm_fails_before_connecting() {
    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args([
            "extract",
            "--realm",
            "../evil",
            "--output",
            "/tmp/keystate-out",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "not a valid output path component",
        ));
}

#[test]
#[ignore = "requires the docker-compose stack"]
fn unknown_backend_is_rejected() {
    keystate()
        .env("KEYSTATE_DB_URL", DB_URL)
        .args(["extract", "--backend", "ferriskey", "--realm", REALM])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown backend"));
}
