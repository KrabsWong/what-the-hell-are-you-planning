//! Startup selection and migration for the incremental v4 persistence backend.
//!
//! The caller must already hold the workspace lock. Version 3 databases are
//! backed up and verified before replacement; an existing v4 database is only
//! accepted after a logical load succeeds. The incarnation is persisted in a
//! sidecar so a later restart opens the same identity even if migration runs
//! from a fresh process.

use crate::persistence::Storage;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LiveStartup {
    pub path: PathBuf,
    pub root: PathBuf,
    pub incarnation: u64,
    /// v3 migrations already produce a verified logical store, so startup can
    /// reuse it. Existing v4 startups validate cheaply and load exactly once at
    /// the normal hydration point.
    pub migrated_store: Option<crate::domain::Store>,
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_extension("v4-incarnation")
}

fn read_sidecar(path: &Path) -> Result<Option<u64>, String> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value = text
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("invalid incarnation in {}: {error}", path.display()))?;
    if value == 0 {
        return Err(format!("invalid zero incarnation in {}", path.display()));
    }
    Ok(Some(value))
}

fn write_sidecar(path: &Path, incarnation: u64) -> Result<(), String> {
    if read_sidecar(path)?.is_some() {
        return Ok(());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let temporary = parent.join(format!(
        ".v4-incarnation-{}-{}.tmp",
        std::process::id(),
        incarnation
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    writeln!(file, "{incarnation}")
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    drop(file);
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("cannot install {}: {error}", path.display()))?;
    if let Ok(directory) = std::fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn fresh_incarnation() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(1);
    nanos ^ (u64::from(std::process::id()) << 32) ^ 0x9E37_79B9_7F4A_7C15
}

/// Select the backend for an existing database. Returns `None` for non-SQLite
/// paths and for databases that still need the pre-v3 upgrade route.
pub(crate) fn select_existing(storage: &Storage) -> Result<Option<LiveStartup>, String> {
    let path = storage.path();
    if !path.extension().is_some_and(|extension| extension == "sqlite3") || !path.is_file() {
        return Ok(None);
    }
    let root = storage.workspace_root().to_path_buf();
    let version = crate::sqlite_v4::schema_version(path)?;
    let incarnation_path = sidecar(path);
    match version {
        4 => {
            let incarnation = match read_sidecar(&incarnation_path)? {
                Some(value) => value,
                None => {
                    let value = crate::sqlite_v4::read_incarnation(path)?;
                    write_sidecar(&incarnation_path, value)?;
                    value
                }
            };
            let database = crate::sqlite_v4::SqliteV4::open(path, &root, incarnation)?;
            database.verify_integrity()?;
            Ok(Some(LiveStartup {
                path: path.to_path_buf(),
                root,
                incarnation,
                migrated_store: None,
            }))
        }
        3 => {
            let incarnation = match read_sidecar(&incarnation_path)? {
                Some(value) => value,
                None => {
                    let value = fresh_incarnation();
                    write_sidecar(&incarnation_path, value)?;
                    value
                }
            };
            crate::sqlite_v4::migrate_v3_to_v4_locked(path, &root, incarnation)?;
            let database = crate::sqlite_v4::SqliteV4::open(path, &root, incarnation)?;
            database.verify_integrity()?;
            Ok(Some(LiveStartup {
                path: path.to_path_buf(),
                root,
                incarnation,
                migrated_store: None,
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Store;

    #[test]
    fn existing_v4_reuses_persisted_incarnation() {
        let root = std::env::temp_dir().join(format!(
            "t14-live-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("workspace.sqlite3");
        let store = Store::empty();
        let incarnation = 77_001;
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert_eq!(selected.incarnation, incarnation);
        assert_eq!(selected.path, path);
        assert!(selected.migrated_store.is_none());
        assert_eq!(read_sidecar(&sidecar(&selected.path)).unwrap(), Some(incarnation));
    }

    #[test]
    fn non_sqlite_paths_stay_on_legacy_storage() {
        let root = std::env::temp_dir().join(format!(
            "t14-live-json-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("workspace.json");
        std::fs::write(&path, b"{}").unwrap();
        assert!(select_existing(&Storage::new(path)).unwrap().is_none());
    }
}
