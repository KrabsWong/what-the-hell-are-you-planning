//! Best-effort durable stop intent for the T13 S1 emergency-stop exception.
//!
//! This is deliberately a small workspace-local sidecar rather than a new Store
//! field or a second execution engine. It records only occurrence/run identity,
//! so it stays portable and survives a normal workspace save failure. A malformed
//! or unsupported file is reported; the app stays viewable, but automation
//! recovery/admission is paused until the file is resolved. Mutations never
//! rewrite a file whose version is newer than this build understands.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StopRecord {
    pub occurrence: u64,
    #[serde(default)]
    pub run_ids: Vec<u64>,
    pub requested_at: i64,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
struct StopFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    records: Vec<StopRecord>,
}

pub(crate) fn journal_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("workspace/stop-intents.json")
}

/// Load records without failing startup on a missing, old, or malformed file.
/// The optional error is diagnostic and may be shown to the user.
pub(crate) fn load(workspace_root: &Path) -> (Vec<StopRecord>, Option<String>) {
    let path = journal_path(workspace_root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), None),
        Err(error) => {
            return (
                Vec::new(),
                Some(format!("Cannot read stop intents: {error}")),
            );
        }
    };
    let file: StopFile = match serde_json::from_slice(&bytes) {
        Ok(file) => file,
        Err(error) => {
            return (
                Vec::new(),
                Some(format!(
                    "Automation recovery is paused because the stop-intent file is malformed: {error}"
                )),
            );
        }
    };
    if file.version > VERSION {
        return (
            Vec::new(),
            Some(format!(
                "Automation recovery is paused because stop intents come from a newer application version (version {}).",
                file.version
            )),
        );
    }
    (file.records, None)
}

/// Record or replace the intent for one occurrence. Atomic within the workspace.
pub(crate) fn record(workspace_root: &Path, record: &StopRecord) -> Result<(), String> {
    let mut file = read_file(workspace_root)?;
    file.version = VERSION;
    file.records
        .retain(|value| value.occurrence != record.occurrence);
    file.records.push(record.clone());
    write_file(workspace_root, &file)
}

/// Clear one occurrence after a durable stop or a terminal resolution.
pub(crate) fn clear(workspace_root: &Path, occurrence: u64) -> Result<(), String> {
    let mut file = read_file(workspace_root)?;
    file.version = VERSION;
    file.records.retain(|value| value.occurrence != occurrence);
    write_file(workspace_root, &file)
}

fn read_file(workspace_root: &Path) -> Result<StopFile, String> {
    let path = journal_path(workspace_root);
    match fs::read(&path) {
        Ok(bytes) => {
            let file: StopFile = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Cannot parse stop intents: {error}"))?;
            if file.version > VERSION {
                return Err(format!(
                    "Stop intents use newer version {}; the file was left unchanged.",
                    file.version
                ));
            }
            Ok(file)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StopFile::default()),
        Err(error) => Err(format!("Cannot read stop intents: {error}")),
    }
}

fn write_file(workspace_root: &Path, file: &StopFile) -> Result<(), String> {
    let path = journal_path(workspace_root);
    if file.records.is_empty() {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("Cannot clear stop intents: {error}")),
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Stop intent path has no parent.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create stop intent folder: {error}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "Stop intent path has no file name.".to_string())?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|error| format!("Cannot serialize stop intents: {error}"))?;
    let result = (|| -> std::io::Result<()> {
        let mut handle = fs::File::create(&temporary)?;
        handle.write_all(&bytes)?;
        handle.sync_all()?;
        drop(handle);
        fs::rename(&temporary, &path)?;
        // Directory fsync is best effort: some filesystems do not allow opening
        // a directory this way. The file itself was synced before the rename.
        #[cfg(unix)]
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Cannot replace stop intents: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        // A process-wide sequence avoids two parallel tests sharing a directory
        // when the wall clock returns the same coarse timestamp.
        let unique = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wthayp-stop-intent-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            unique
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn missing_file_is_empty_and_round_trip_is_portable() {
        let root = directory();
        let (empty, error) = load(&root);
        assert!(empty.is_empty());
        assert!(error.is_none());
        let record = StopRecord {
            occurrence: 7,
            run_ids: vec![3, 4],
            requested_at: 1_000,
            reason: "user stop".into(),
        };
        super::record(&root, &record).unwrap();
        let (loaded, error) = load(&root);
        assert_eq!(loaded, vec![record.clone()]);
        assert!(error.is_none());
        let bytes = fs::read_to_string(journal_path(&root)).unwrap();
        assert!(
            !bytes.contains(root.to_string_lossy().as_ref()),
            "portable ids only"
        );
        clear(&root, 7).unwrap();
        let (empty, error) = load(&root);
        assert!(empty.is_empty());
        assert!(error.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_completes_atomically_without_leaving_a_temporary_file() {
        let root = directory();
        let record = StopRecord {
            occurrence: 9,
            run_ids: vec![5],
            requested_at: 2_000,
            reason: "qa".into(),
        };
        super::record(&root, &record).unwrap();
        let parent = journal_path(&root).parent().unwrap().to_path_buf();
        let names: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|name| name == "stop-intents.json"));
        assert!(
            !names.iter().any(|name| name.ends_with(".tmp")),
            "temporary file leaked: {names:?}"
        );
        assert_eq!(load(&root).0, vec![record]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mutation_never_down_converts_or_rewrites_a_future_journal() {
        let root = directory();
        fs::create_dir_all(journal_path(&root).parent().unwrap()).unwrap();
        let original = br#"{"version":99,"unknown_future_field":{"keep":true},"records":[{"occurrence":1,"run_ids":[2],"requested_at":1,"reason":"future"}]}"#;
        fs::write(journal_path(&root), original).unwrap();
        let record = StopRecord {
            occurrence: 7,
            run_ids: vec![3],
            requested_at: 2_000,
            reason: "must not write".into(),
        };
        assert!(super::record(&root, &record).is_err());
        assert_eq!(fs::read(journal_path(&root)).unwrap(), original);
        assert!(clear(&root, 1).is_err());
        assert_eq!(fs::read(journal_path(&root)).unwrap(), original);
        let (records, error) = load(&root);
        assert!(records.is_empty());
        assert!(error.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_or_newer_files_are_ignored_with_a_diagnostic() {
        let root = directory();
        fs::create_dir_all(journal_path(&root).parent().unwrap()).unwrap();
        fs::write(journal_path(&root), b"{not json").unwrap();
        let (records, error) = load(&root);
        assert!(records.is_empty());
        assert!(error.is_some());

        fs::write(
            journal_path(&root),
            br#"{"version":99,"records":[{"occurrence":1}]}"#,
        )
        .unwrap();
        let (records, error) = load(&root);
        assert!(records.is_empty());
        assert!(error.is_some());
        let _ = fs::remove_dir_all(root);
    }
}
