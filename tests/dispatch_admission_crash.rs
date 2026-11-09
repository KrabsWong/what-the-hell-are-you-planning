#![cfg(all(feature = "qa", unix))]

use rusqlite::Connection;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "t14-l1b-crash-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn child(phase: &str, root: &Path) -> ExitStatus {
    child_with_crash(phase, root, None)
}

fn child_with_crash(phase: &str, root: &Path, crash: Option<&str>) -> ExitStatus {
    let mut command = Command::new(env!("CARGO_BIN_EXE_what-the-hell-are-you-planning"));
    command.args(["--qa-dispatch-crash", phase, root.to_str().unwrap()]);
    if let Some(point) = crash {
        command.env("T14_LB_CRASH", point);
    }
    let mut child = command
        .spawn()
        .expect("spawn dispatch crash child");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("child status") {
            return status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("dispatch crash child {phase} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn count(path: &Path, table: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .unwrap()
}

fn run_status(path: &Path) -> String {
    let connection = Connection::open(path).unwrap();
    let payload: String = connection
        .query_row("SELECT payload FROM runs LIMIT 1", [], |row| row.get(0))
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&payload).unwrap()["status"]
        .as_str()
        .unwrap()
        .to_string()
}

fn intent_keys(root: &Path) -> Vec<String> {
    let payload = match std::fs::read(root.join("dispatch-intent.json")) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot read intent file: {error}"),
    };
    let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["key"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn claim_before_spawn_survives_crash_without_a_launch_record() {
    let root = temp_root("claim");
    let status = child("claim-before-spawn", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT), "child was not abrupt");
    let path = root.join("workspace.sqlite3");
    assert_eq!(count(&path, "runs"), 1);
    assert_eq!(count(&path, "automation_occurrences"), 0);
    assert!(
        !root.join("launches.log").exists(),
        "no process was started before the claim crash"
    );
    let keys = intent_keys(&root);
    assert!(
        keys.iter()
            .any(|key| key == "launch:dispatch:None:run:4000"),
        "durable launch claim missing: {keys:?}"
    );
    assert!(
        keys.iter().any(|key| key == "dispatch:None:run:4000"),
        "original dispatch intent must be retained"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn durable_before_ack_crash_keeps_the_dispatch_intent() {
    let root = temp_root("durable");
    let status = child("abort-after-durable", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    let path = root.join("workspace.sqlite3");
    assert_eq!(count(&path, "runs"), 1);
    assert!(!root.join("launches.log").exists());
    let keys = intent_keys(&root);
    assert!(keys.iter().any(|key| key == "dispatch:None:run:4000"));
    assert!(!keys.iter().any(|key| key.starts_with("launch:")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ack_before_claim_crash_leaves_recoverable_run_without_claim() {
    let root = temp_root("ack");
    let status = child("abort-after-ack", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    let path = root.join("workspace.sqlite3");
    assert_eq!(count(&path, "runs"), 1);
    assert_eq!(run_status(&path), "Queued");
    assert!(!root.join("launches.log").exists());
    // The confirmed dispatch intent is gone; no launch claim was written, so a
    // restart sees a durable but unlaunched run and can recover it explicitly.
    assert!(!root.join("dispatch-intent.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resolution_failed_intent_survives_crash_before_execution() {
    let root = temp_root("resolution-failed");
    let status = child("resolution-failed", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    let keys = intent_keys(&root);
    assert!(
        keys.iter()
            .any(|key| key == "resolve:dispatch:None:run:4000:failed"),
        "failed decision missing: {keys:?}"
    );
    assert!(keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    assert_eq!(run_status(&root.join("workspace.sqlite3")), "Queued");
    let status = child("restart-failed", &root);
    assert!(status.success(), "failed-restart execution failed: {status:?}");
    assert_eq!(run_status(&root.join("workspace.sqlite3")), "Blocked");
    assert!(!root.join("launches.log").exists());
    let keys = intent_keys(&root);
    assert!(!keys.iter().any(|key| key.starts_with("resolve:")));
    assert!(!keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resolution_rerun_intent_keeps_a_stable_replacement_identity() {
    let root = temp_root("resolution-rerun");
    let status = child("resolution-rerun", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    let keys = intent_keys(&root);
    assert!(
        keys.iter()
            .any(|key| key == "resolve:dispatch:None:run:4000:rerun:5000"),
        "stable replacement identity missing: {keys:?}"
    );
    assert!(keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    assert!(!root.join("launches.log").exists());
    let status = child("restart-rerun", &root);
    assert!(status.success(), "rerun-restart execution failed: {status:?}");
    assert_eq!(run_status(&root.join("workspace.sqlite3")), "Blocked");
    let launch = std::fs::read_to_string(root.join("launches.log")).unwrap();
    assert_eq!(launch.lines().collect::<Vec<_>>(), vec!["5000"]);
    let keys = intent_keys(&root);
    assert!(!keys.iter().any(|key| key.starts_with("resolve:")));
    assert!(!keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn restart_crash_after_resolution_status_and_claim_release_recovers() {
    let root = temp_root("restart-crash-status");
    let path = root.join("workspace.sqlite3");
    assert_eq!(
        child("resolution-failed", &root).signal(),
        Some(libc::SIGABRT)
    );
    let status = child_with_crash("restart-crash-failed", &root, Some("status"));
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    assert_eq!(run_status(&path), "Blocked");
    let keys = intent_keys(&root);
    assert!(keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    assert!(keys.iter().any(|key| key.starts_with("resolve:")));
    let status = child("restart-failed", &root);
    assert!(status.success(), "recovery after status crash failed: {status:?}");
    assert!(!root.join("launches.log").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn restart_crash_after_claim_release_completes_rerun_once() {
    let root = temp_root("restart-crash-claim");
    let path = root.join("workspace.sqlite3");
    assert_eq!(
        child("resolution-rerun", &root).signal(),
        Some(libc::SIGABRT)
    );
    let status = child_with_crash("restart-crash-rerun", &root, Some("claim"));
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    assert_eq!(run_status(&path), "Blocked");
    assert!(!root.join("launches.log").exists());
    let status = child("restart-rerun", &root);
    assert!(status.success(), "rerun recovery failed: {status:?}");
    let launch = std::fs::read_to_string(root.join("launches.log")).unwrap();
    assert_eq!(launch.lines().collect::<Vec<_>>(), vec!["5000"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn spawn_before_status_crash_keeps_one_launch_and_the_claim() {
    let root = temp_root("spawn");
    let status = child("spawn-before-status", &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    let path = root.join("workspace.sqlite3");
    assert_eq!(count(&path, "runs"), 1);
    assert_eq!(run_status(&path), "Queued", "status was not persisted before the crash");
    let launch = std::fs::read_to_string(root.join("launches.log")).unwrap();
    assert_eq!(launch.lines().collect::<Vec<_>>(), vec!["4000"]);
    let keys = intent_keys(&root);
    assert!(keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
    // A durable restart must see the claim; it must not treat the run as a
    // fresh, never-launched dispatch.
    let status = child("recover-check", &root);
    assert!(status.success(), "restart check failed: {status:?}");
    let launch = std::fs::read_to_string(root.join("launches.log")).unwrap();
    assert_eq!(
        launch.lines().count(),
        1,
        "restart must not record a second launch"
    );
    let _ = std::fs::remove_dir_all(root);
}
