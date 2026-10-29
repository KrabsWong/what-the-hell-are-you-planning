//! Transactional workspace snapshots, split into independently inspectable entity tables.
use crate::domain::Store;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use std::path::Path;

const TABLES: [&str; 4] = ["tasks", "projects", "runs", "policies"];

pub fn load(path: &Path) -> Result<Option<Store>, String> {
    if !path.exists() {
        return Ok(None);
    }
    read(path).map_err(|e| format!("Cannot read SQLite workspace {}: {e}", path.display()))
}

fn read(path: &Path) -> Result<Option<Store>, Box<dyn std::error::Error>> {
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    let tx = db.transaction()?;
    let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != 1 {
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
    Ok(Some(serde_json::from_value(value)?))
}

pub fn save(path: &Path, store: &Store) -> Result<(), String> {
    write(path, store).map_err(|e| format!("Cannot save SQLite workspace {}: {e}", path.display()))
}

fn write(path: &Path, store: &Store) -> Result<(), Box<dyn std::error::Error>> {
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
    if version != 0 && version != 1 {
        return Err(format!("Unsupported SQLite workspace version: {version}").into());
    }
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);")?;
    let mut metadata = serde_json::to_value(store)?;
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
    tx.pragma_update(None, "user_version", 1)?;
    tx.commit()?;
    Ok(())
}
