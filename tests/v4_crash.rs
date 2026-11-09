#![cfg(all(feature = "qa", unix))]

use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "t14-subprocess-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn child(mode: &str, path: &Path, root: &Path) -> ExitStatus {
    let marker = path.with_extension("marker");
    let _ = std::fs::remove_file(&marker);
    let mut child = Command::new(env!("CARGO_BIN_EXE_what-the-hell-are-you-planning"))
        .args(["--qa-v4-crash", mode, path.to_str().unwrap(), root.to_str().unwrap(), "51"])
        .env("T14_CRASH_MARKER", &marker)
        .spawn()
        .expect("spawn crash child");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().expect("child status") {
            return status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("crash child {mode} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn query_json(path: &Path, table: &str, field: &str) -> serde_json::Value {
    let connection = Connection::open(path).unwrap();
    let payload: String = connection
        .query_row(&format!("SELECT payload FROM {table} LIMIT 1"), [], |row| row.get(0))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    value[field].clone()
}

fn count(path: &Path, table: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .unwrap()
}

#[test]
fn subprocess_after_commit_crash_is_abrupt_and_retry_is_idempotent() {
    let root = temp_root("after-commit");
    let path = root.join("workspace.sqlite3");
    assert!(child("seed", &path, &root).success());
    let status = child("after-commit", &path, &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT), "after-commit was not abrupt");
    assert_eq!(
        std::fs::read_to_string(path.with_extension("marker")).unwrap(),
        "after-commit"
    );
    assert_eq!(count(&path, "commits"), 1);
    assert_eq!(query_json(&path, "automations", "cursor"), 9_999);
    assert_eq!(query_json(&path, "automation_occurrences", "unread"), true);
    let occurrence_count = count(&path, "automation_occurrences");
    let status = child("retry-same", &path, &root);
    assert!(status.success(), "exact retry failed: {status:?}");
    assert_eq!(count(&path, "commits"), 1);
    assert_eq!(count(&path, "automation_occurrences"), occurrence_count);
    assert_eq!(query_json(&path, "automations", "cursor"), 9_999);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn subprocess_mid_group_crash_leaves_no_partial_cursor_or_occurrence() {
    let root = temp_root("mid-group");
    let path = root.join("workspace.sqlite3");
    assert!(child("seed", &path, &root).success());
    let cursor_before = query_json(&path, "automations", "cursor");
    let status = child("mid-group", &path, &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    assert_eq!(
        std::fs::read_to_string(path.with_extension("marker")).unwrap(),
        "mid-group-reached"
    );
    assert_eq!(count(&path, "commits"), 0);
    assert_eq!(query_json(&path, "automations", "cursor"), cursor_before);
    assert_eq!(query_json(&path, "automation_occurrences", "unread"), false);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn subprocess_migration_crash_keeps_readable_v3_latest_state() {
    let root = temp_root("migration");
    let path = root.join("workspace.sqlite3");
    assert!(child("seed-v3", &path, &root).success());
    let baseline = {
        let connection = Connection::open(&path).unwrap();
        let payload: String = connection
            .query_row(
                "SELECT payload FROM tasks WHERE json_extract(payload,'$.id')=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        serde_json::from_str::<serde_json::Value>(&payload).unwrap()["title"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_ne!(baseline, "crash latest");
    // Keep a writer connection open across the child crash so the committed
    // change remains WAL-only while migration runs.
    let writer = Connection::open(&path).unwrap();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    writer
        .execute(
            "UPDATE tasks SET payload=json_set(payload,'$.title','crash latest') WHERE json_extract(payload,'$.id')=1",
            [],
        )
        .unwrap();
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    assert!(wal.is_file() && std::fs::metadata(&wal).unwrap().len() > 0);
    let main_only = Connection::open_with_flags(
        format!("file:{}?immutable=1", path.display()),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let main_payload: Option<String> = main_only
        .query_row(
            "SELECT payload FROM tasks WHERE json_extract(payload,'$.id')=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    let main_title = main_payload
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .and_then(|value| value.get("title").and_then(|v| v.as_str()).map(str::to_owned));
    assert_eq!(main_title.as_deref(), Some(baseline.as_str()));
    let status = child("migration-crash", &path, &root);
    assert_eq!(status.signal(), Some(libc::SIGABRT));
    assert_eq!(
        std::fs::read_to_string(path.with_extension("marker")).unwrap(),
        "migration-checkpoint-reached"
    );
    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let payload: String = connection
        .query_row(
            "SELECT payload FROM tasks WHERE json_extract(payload,'$.id')=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value["title"], "crash latest");
    drop(writer);
    let _ = std::fs::remove_dir_all(root);
}
