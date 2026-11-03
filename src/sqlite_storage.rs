//! Transactional workspace snapshots, split into independently inspectable entity tables.
use crate::domain::Store;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static BACKUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const TABLES: [&str; 4] = ["tasks", "projects", "runs", "policies"];

pub fn load(path: &Path, root: &Path) -> Result<Option<Store>, String> {
    if !path.exists() {
        return Ok(None);
    }
    read(path, root).map_err(|e| format!("Cannot read SQLite workspace {}: {e}", path.display()))
}

fn read(path: &Path, root: &Path) -> Result<Option<Store>, Box<dyn std::error::Error>> {
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let tx = db.transaction()?;
    read_connection(&tx, root)
}

fn read_connection(tx: &Connection, root: &Path) -> Result<Option<Store>, Box<dyn std::error::Error>> {
    let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !matches!(version, 1 | 2) {
        return Err(format!("Unsupported SQLite workspace version: {version}").into());
    }
    let metadata: Option<String> = tx
        .query_row("SELECT payload FROM workspace WHERE id=1", [], |r| r.get(0))
        .optional()?;
    let Some(metadata) = metadata else {
        return Err("SQLite workspace metadata is missing".into());
    };
    let mut value: Value = serde_json::from_str(&metadata)?;
    for table in TABLES {
        let mut statement =
            tx.prepare(&format!("SELECT payload FROM {table} ORDER BY position"))?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(serde_json::from_str::<Value>(&row?)?);
        }
        value[table] = Value::Array(items);
    }
    if version == 2 { transform_paths(&mut value, root, false)?; }
    Ok(Some(serde_json::from_value(value)?))
}

pub fn save(path: &Path, root: &Path, store: &Store) -> Result<(), String> {
    // Validate requested changes before a one-time upgrade changes the old file.
    let mut value = serde_json::to_value(store).map_err(|e| e.to_string())?;
    transform_paths(&mut value, root, true)?;
    upgrade_paths(path, root, |_| {})?;
    write(path, root, store, &mut |_| {}).map_err(|e| format!("Cannot save SQLite workspace {}: {e}", path.display()))
}

fn write(path: &Path, root: &Path, store: &Store, progress: &mut impl FnMut(&'static str)) -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = serde_json::to_value(store)?;
    transform_paths(&mut metadata, root, true)?;
    // Private permissions from creation, including before SQLite opens the file.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(options.open(path)?);
    let mut db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.pragma_update(None, "journal_mode", "DELETE")?;
    db.pragma_update(None, "synchronous", "FULL")?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !matches!(version, 0..=2) {
        return Err(format!("Unsupported SQLite workspace version: {version}").into());
    }
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);")?;
    for table in TABLES {
        tx.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {table} (position INTEGER PRIMARY KEY, payload TEXT NOT NULL); DELETE FROM {table};"))?;
        let items = metadata.as_object_mut().unwrap().remove(table).unwrap();
        let mut statement = tx.prepare(&format!(
            "INSERT INTO {table}(position,payload) VALUES (?1,?2)"
        ))?;
        for (position, item) in items.as_array().unwrap().iter().enumerate() {
            statement.execute(params![position as i64, serde_json::to_string(item)?])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO workspace(id,payload) VALUES(1,?1)",
        [serde_json::to_string(&metadata)?],
    )?;
    tx.pragma_update(None, "user_version", 2)?;
    if version == 1 {
        progress("Verifying upgraded data…");
        if read_connection(&tx, root)?.as_ref() != Some(store) {
            return Err("Path upgrade changed workspace data; the transaction was rolled back".into());
        }
        check_integrity(&tx)?;
    }
    tx.commit()?;
    Ok(())
}

/// The caller owns the workspace lock. The backup retains the old schema and absolute paths.
pub fn upgrade_paths(path: &Path, root: &Path, mut progress: impl FnMut(&'static str)) -> Result<Option<PathBuf>, String> {
    upgrade(path, root, &mut progress).map_err(|e| format!("Cannot upgrade workspace paths: {e}"))
}

fn check_integrity(db: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let mut statement = db.prepare("PRAGMA integrity_check")?;
    let checks = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    if checks != ["ok"] { return Err("SQLite integrity check failed".into()); }
    Ok(())
}

fn upgrade(path: &Path, root: &Path, progress: &mut impl FnMut(&'static str)) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if !path.exists() { return Ok(None); }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        2 => return Ok(None),
        1 => {},
        _ => return Err(format!("Unsupported SQLite workspace version: {version}").into()),
    }
    let original = read_connection(&db, root)?.ok_or("Workspace data is missing")?;
    // Preflight conversion before creating a backup or touching the original database.
    let mut value = serde_json::to_value(&original)?;
    transform_paths(&mut value, root, true)?;
    progress("Backing up existing data…");
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let backup = path.with_file_name(format!("workspace.before-path-upgrade-{timestamp}-{}-{}.sqlite3", std::process::id(), BACKUP_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(options.open(&backup)?);
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        db.execute("VACUUM INTO ?1", [backup.to_str().ok_or("Backup path must be UTF-8")?])?;
        let backup_db = Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        check_integrity(&backup_db)?;
        if read_connection(&backup_db, root)?.as_ref() != Some(&original) {
            return Err("Backup verification failed; original workspace was not changed".into());
        }
        std::fs::File::open(&backup)?.sync_all()?;
        #[cfg(unix)]
        std::fs::File::open(backup.parent().ok_or("Invalid backup directory")?)?.sync_all()?;
        progress("Converting workspace paths…");
        write(path, root, &original, progress)?;
        Ok(())
    })();
    result.map_err(|e| format!("{e}. Upgrade backup: {}", backup.display()))?;
    Ok(Some(backup))
}

/// Only operational paths are rewritten. Free text and historical evidence are immutable.
/// Runtime stores remain absolute so execution, recovery and signature comparisons share a root.
fn transform_paths(value: &mut Value, root: &Path, encode: bool) -> Result<(), String> {
    if !root.is_absolute() { return Err("Workspace storage root must be absolute".into()); }
    fn path(value: &mut Value, root: &Path, encode: bool) -> Result<(), String> {
        let Some(text) = value.as_str() else { return Err("Invalid workspace path".into()); };
        if text.is_empty() { return Ok(()); }
        let original = Path::new(text);
        if encode {
            // External legacy directories must survive until explicitly imported into the workspace.
            if !original.is_absolute() { return Err("Runtime workspace paths must be absolute".into()); }
            if let Ok(relative) = original.strip_prefix(root) {
                if text.contains('\\') || relative.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
                    return Err("Workspace path must not contain parent traversal".into());
                }
                *value = Value::String(if relative.as_os_str().is_empty() { ".".into() } else { relative.to_string_lossy().into_owned() });
            }
        } else if !original.is_absolute() {
            if text.contains('\\') || original.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
                return Err("Invalid relative workspace path".into());
            }
            let resolved = if text == "." { root.to_path_buf() } else { root.join(original) };
            *value = Value::String(resolved.to_string_lossy().into_owned());
        }
        Ok(())
    }
    if let Some(projects) = value.get_mut("projects").and_then(Value::as_array_mut) {
        for project in projects { if let Some(directory) = project.get_mut("directory") { path(directory, root, encode)?; } }
    }
    if let Some(runs) = value.get_mut("runs").and_then(Value::as_array_mut) {
        for run in runs {
            for field in ["directory", "workspace_source"] {
                if let Some(directory) = run.get_mut(field).filter(|v| !v.is_null()) { path(directory, root, encode)?; }
            }
            // Known signature tuples: request plan (7), project plan (5), research (5).
            // Convert the saved snapshot, never regenerate it from current inputs: stale results
            // must remain stale after relocation.
            let original_signature = run.get("input_signature").and_then(Value::as_str).unwrap_or_default().to_string();
            if let Ok(mut signature) = serde_json::from_str::<Value>(&original_signature) {
                let index = if run.get("project_plan").is_some_and(|v| !v.is_null()) {
                    match signature.as_array().map(Vec::len) { Some(7) => Some(5), Some(5) => Some(3), _ => None }
                } else if run.get("research_kind").is_some_and(|v| !v.is_null()) && signature.as_array().is_some_and(|v| v.len() == 5) {
                    Some(2)
                } else { None };
                if let Some(index) = index {
                    let before = signature[index].clone();
                    path(&mut signature[index], root, encode)?;
                    if signature[index] != before { run["input_signature"] = Value::String(replace_signature_path(&original_signature, index, &signature[index])?); }
                }
            }
        }
    }
    Ok(())
}

/// Preserve the exact JSON bytes of all other inputs (including struct key ordering).
/// Signatures are compared as strings, so serializing the whole Value would invalidate them.
pub(crate) fn replace_signature_path(signature: &str, index: usize, replacement: &Value) -> Result<String, String> {
    let mut offset = signature.find('[').ok_or("Invalid input signature")? + 1;
    for position in 0..=index {
        let mut values = serde_json::Deserializer::from_str(&signature[offset..]).into_iter::<Value>();
        values.next().ok_or("Missing signature field")?.map_err(|e| e.to_string())?;
        let end = offset + values.byte_offset();
        if position == index {
            let start = offset + signature[offset..end].len() - signature[offset..end].trim_start().len();
            return Ok(format!("{}{}{}", &signature[..start], replacement, &signature[end..]));
        }
        offset = end;
        while signature.as_bytes().get(offset).is_some_and(u8::is_ascii_whitespace) { offset += 1; }
        if signature.as_bytes().get(offset) != Some(&b',') { return Err("Invalid signature separator".into()); }
        offset += 1;
    }
    Err("Missing signature field".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::Stage, persistence::Storage};

    fn temp() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(crate::opencode::new_session_id());
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn legacy_upgrade_is_lossless_and_relocates_only_operational_paths() {
        let root = temp();
        let path = root.join("workspace.sqlite3");
        let mut original = Store::mock();
        original.default_model = "test/model".into();
        original.projects[0].directory = root.join("projects/project-1").to_string_lossy().into();
        original.set_stage(1, Stage::InProgress).unwrap();
        original.simulate_run(1).unwrap();
        let run = original.runs.last_mut().unwrap();
        run.directory = original.projects[0].directory.clone();
        run.workspace_source = Some(root.to_string_lossy().into());
        run.prompt = format!("Historical path: {}", run.directory);
        run.output = run.prompt.clone();
        // Reproduce the old v1 format, with absolute paths and no rewrite on reading.
        save(&path, Path::new("/unrelated-legacy-root"), &original).unwrap();
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(load(&path, &root).unwrap(), Some(original.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let mut invalid = original.clone();
        invalid.projects[0].directory = root.join("bad\\name").to_string_lossy().into();
        assert!(save(&path, &root, &invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        // A failed upgrade rolls back both data and schema version.
        db.execute_batch("CREATE TRIGGER reject_projects BEFORE INSERT ON projects BEGIN SELECT RAISE(ABORT, 'failure'); END;").unwrap();
        assert!(save(&path, &root, &original).is_err());
        assert_eq!(db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(load(&path, &root).unwrap(), Some(original.clone()));
        db.execute_batch("DROP TRIGGER reject_projects;").unwrap();
        drop(db);
        save(&path, &root, &original).unwrap();
        assert_eq!(load(&path, &root).unwrap(), Some(original.clone()));
        let moved = root.with_extension("moved");
        std::fs::rename(&root, &moved).unwrap();
        let restored = Storage::new(moved.join("workspace.sqlite3")).load().unwrap().unwrap();
        let mut expected = original;
        expected.projects[0].directory = moved.join("projects/project-1").to_string_lossy().into();
        expected.runs.last_mut().unwrap().directory = expected.projects[0].directory.clone();
        expected.runs.last_mut().unwrap().workspace_source = Some(moved.to_string_lossy().into());
        assert_eq!(restored, expected);
        std::fs::remove_dir_all(moved).unwrap();
    }

    fn legacy_fixture(root: &Path) -> (Storage, Store) {
        let storage = Storage::new(root.join("workspace.sqlite3"));
        let mut store = Store::empty();
        store.projects[0].directory = root.join("projects/project-1").to_string_lossy().into();
        save(storage.path(), Path::new("/old-storage-format"), &store).unwrap();
        Connection::open(storage.path()).unwrap().pragma_update(None, "user_version", 1).unwrap();
        (storage, store)
    }

    #[test]
    fn explicit_upgrade_keeps_verified_private_backup_and_is_idempotent() {
        let root = temp();
        let (storage, original) = legacy_fixture(&root);
        let _lock = storage.acquire_lock().unwrap();
        let mut stages = vec![];
        let backup = storage.upgrade_paths(|stage| stages.push(stage)).unwrap().unwrap();
        assert_eq!(stages, ["Backing up existing data…", "Converting workspace paths…", "Verifying upgraded data…"]);
        assert_eq!(load(&backup, &root).unwrap(), Some(original.clone()));
        assert_eq!(load(storage.path(), &root).unwrap(), Some(original));
        let backup_db = Connection::open(&backup).unwrap();
        assert_eq!(backup_db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(), 1);
        let payload: String = backup_db.query_row("SELECT payload FROM projects LIMIT 1", [], |r| r.get(0)).unwrap();
        assert!(serde_json::from_str::<Value>(&payload).unwrap()["directory"].as_str().unwrap().starts_with(root.to_str().unwrap()));
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let entries = std::fs::read_dir(&root).unwrap().count();
        let bytes = std::fs::read(storage.path()).unwrap();
        assert!(storage.upgrade_paths(|_| panic!("v2 must not run migration stages")).unwrap().is_none());
        assert_eq!(std::fs::read(storage.path()).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), entries);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upgrade_verification_failure_rolls_back_and_keeps_each_backup() {
        let root = temp();
        let (storage, original) = legacy_fixture(&root);
        let db = Connection::open(storage.path()).unwrap();
        db.execute_batch("CREATE TRIGGER corrupt_project AFTER INSERT ON projects BEGIN UPDATE projects SET payload=json_set(payload,'$.name','changed by trigger') WHERE position=NEW.position; END;").unwrap();
        let bytes = std::fs::read(storage.path()).unwrap();
        for expected_backups in 1..=2 {
            assert!(storage.upgrade_paths(|_| {}).unwrap_err().contains("rolled back"));
            assert_eq!(std::fs::read(storage.path()).unwrap(), bytes);
            assert_eq!(load(storage.path(), &root).unwrap(), Some(original.clone()));
            let backups: Vec<_> = std::fs::read_dir(&root).unwrap().filter_map(|entry| {
                let path = entry.unwrap().path();
                path.file_name().unwrap().to_str().unwrap().starts_with("workspace.before-path-upgrade-").then_some(path)
            }).collect();
            assert_eq!(backups.len(), expected_backups);
            for backup in backups { assert_eq!(load(&backup, &root).unwrap(), Some(original.clone())); }
        }
        db.execute_batch("DROP TRIGGER corrupt_project").unwrap();
        drop(db);
        // An ordinary save cannot bypass the verified-backup upgrade.
        storage.save(&original).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().filter(|entry| entry.as_ref().unwrap().file_name().to_str().unwrap().starts_with("workspace.before-path-upgrade-")).count(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upgrade_rejects_future_or_damaged_data_without_creating_backup() {
        let root = temp();
        let (storage, _) = legacy_fixture(&root);
        let db = Connection::open(storage.path()).unwrap();
        db.pragma_update(None, "user_version", 999).unwrap();
        let bytes = std::fs::read(storage.path()).unwrap();
        assert!(storage.upgrade_paths(|_| panic!("future schema must not start migration")).is_err());
        assert_eq!(std::fs::read(storage.path()).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute("UPDATE projects SET payload='invalid json'", []).unwrap();
        let bytes = std::fs::read(storage.path()).unwrap();
        assert!(storage.upgrade_paths(|_| panic!("invalid data must not start migration")).is_err());
        assert_eq!(std::fs::read(storage.path()).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signature_paths_relocate_without_refreshing_stale_inputs() {
        let old = Path::new("/old/workspace");
        let new = Path::new("/new/workspace");
        let signatures = [
            serde_json::json!([1, "old project name", "brief", "/old/workspace/projects/p", []]),
            serde_json::json!([1, 1, "old request", "date", "name", "/old/workspace/projects/p", null]),
            serde_json::json!([[], [9], "/old/workspace/projects/p", "analyze", 1]),
        ];
        for (index, signature) in signatures.into_iter().enumerate() {
            let mut value = serde_json::json!({"projects":[], "runs":[{
                "project_plan": if index < 2 { Some(1) } else { None },
                "research_kind":"analyze", "input_signature":signature.to_string(),
                "directory":"/old/workspace/projects/p", "workspace_source":null
            }]});
            transform_paths(&mut value, old, true).unwrap();
            transform_paths(&mut value, new, false).unwrap();
            let restored: Value = serde_json::from_str(value["runs"][0]["input_signature"].as_str().unwrap()).unwrap();
            let mut expected = signature;
            expected[[3, 5, 2][index]] = Value::String("/new/workspace/projects/p".into());
            assert_eq!(restored, expected);
        }
    }

    #[test]
    fn signature_rewrite_preserves_nested_struct_order_and_whitespace() {
        let original = r#"[ [{"z":1,"a":2}], [], "/old/workspace/project", "analyze", 1 ]"#;
        let rewritten = replace_signature_path(original, 2, &Value::String("projects/p".into())).unwrap();
        assert_eq!(rewritten, original.replace("/old/workspace/project", "projects/p"));
        assert_eq!(replace_signature_path(&rewritten, 2, &Value::String("/old/workspace/project".into())).unwrap(), original);
    }

    #[test]
    fn relative_paths_reject_escape_and_preserve_external_legacy_locations() {
        for invalid in ["../outside", "projects/../../outside", "projects\\outside"] {
            let mut value = serde_json::json!({"projects":[{"directory":invalid}]});
            assert!(transform_paths(&mut value, Path::new("/workspace"), false).is_err());
        }
        let mut value = serde_json::json!({"projects":[{"directory":"/workspace-other/project"}]});
        let original = value.clone();
        transform_paths(&mut value, Path::new("/workspace"), true).unwrap();
        transform_paths(&mut value, Path::new("/moved"), false).unwrap();
        assert_eq!(value, original);
    }

    // Opt-in regression against a snapshot only. Never open the real workspace from this test.
    #[test]
    #[ignore = "requires an explicitly prepared local workspace snapshot"]
    fn local_workspace_snapshot_roundtrip_and_relocation() {
        let copy = std::path::PathBuf::from(std::env::var_os("WTHAYP_TEST_SNAPSHOT").unwrap());
        let old = std::path::PathBuf::from(std::env::var_os("WTHAYP_TEST_ORIGINAL_ROOT").unwrap());
        assert!(copy.starts_with(std::env::temp_dir()) || copy.starts_with("/private/tmp"));
        assert_ne!(copy, old);
        let path = copy.join("workspace.sqlite3");
        let storage = Storage::new(path.clone()).with_workspace_root(old.clone());
        let original = storage.load().unwrap().unwrap();
        // Reproduce v1 from this already captured dataset, never reopen the live workspace.
        let legacy_path = copy.join("legacy-upgrade-check.sqlite3");
        save(&legacy_path, Path::new("/unrelated-snapshot-root"), &original).unwrap();
        Connection::open(&legacy_path).unwrap().pragma_update(None, "user_version", 1).unwrap();
        let legacy = Storage::new(legacy_path.clone()).with_workspace_root(old.clone());
        let lock = legacy.acquire_lock().unwrap();
        let mut phases = vec![];
        let backup = legacy.upgrade_paths(|phase| phases.push(phase)).unwrap().unwrap();
        assert!(load(&backup, &old).unwrap().as_ref() == Some(&original), "upgrade backup changed snapshot data");
        assert!(legacy.load().unwrap().as_ref() == Some(&original), "explicit upgrade changed snapshot data");
        assert_eq!(phases, ["Backing up existing data…", "Converting workspace paths…", "Verifying upgraded data…"]);
        assert!(legacy.upgrade_paths(|_| panic!("must not upgrade twice")).unwrap().is_none());
        drop(lock);
        let signatures: Vec<_> = original.tasks.iter().map(|t| (t.id, original.research_result(t.id).map(|r| r.id), original.goal_execution_approved(t.id))).collect();
        storage.save(&original).unwrap();
        assert!(storage.load().unwrap().as_ref() == Some(&original), "same-root save/load changed existing data");
        assert_eq!(crate::workspace::initialize(copy.parent().unwrap(), None).unwrap(), copy);
        let relocated = Storage::new(path.clone()).load().unwrap().unwrap();
        for (id, result, approved) in signatures {
            assert_eq!(relocated.research_result(id).map(|r| r.id), result, "result validity changed for task {id}");
            assert_eq!(relocated.goal_execution_approved(id), approved, "approval changed for task {id}");
        }
        for project in &relocated.projects {
            assert!(Path::new(&project.directory).starts_with(&copy));
            assert!(Path::new(&project.directory).is_dir());
        }
        for task in &relocated.tasks {
            for attachment in &task.attachments { crate::attachments::resolve(&copy, attachment).unwrap(); }
        }
        for run in &relocated.runs {
            for attachment in &run.attachments { crate::attachments::resolve(&copy, attachment).unwrap(); }
        }
        // Compare the entire store after resolving it back against the original root.
        Storage::new(path.clone()).save(&relocated).unwrap();
        assert!(storage.load().unwrap().as_ref() == Some(&original), "relocation changed non-path data");
        let db = Connection::open(&path).unwrap();
        assert_eq!(db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
        for table in ["projects", "runs"] {
            let mut statement = db.prepare(&format!("SELECT payload FROM {table}")).unwrap();
            for payload in statement.query_map([], |r| r.get::<_, String>(0)).unwrap() {
                let value: Value = serde_json::from_str(&payload.unwrap()).unwrap();
                for field in ["directory", "workspace_source"] {
                    if let Some(path) = value[field].as_str().filter(|s| !s.is_empty()) { assert!(!Path::new(path).is_absolute(), "snapshot still has absolute operational paths"); }
                }
            }
        }
        println!("Snapshot verified: {} projects, {} tasks, {} runs; full roundtrip, result validity, approvals, attachments, relative paths, integrity_check", original.projects.len(), original.tasks.len(), original.runs.len());
    }
}
