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
    /// Non-fatal recovery rejection (foreign/stale sidecar) to surface in UI.
    pub recovery_warning: Option<String>,
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_extension("v4-incarnation")
}

fn normalize_loaded(store: &mut crate::domain::Store) -> Result<(), String> {
    if store.automation_schema_version() == 0 {
        store.set_automation_schema_version(crate::automation::AUTOMATION_SCHEMA_VERSION);
    } else if store.automation_schema_version() > crate::automation::AUTOMATION_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported automation schema version {} (expected {}). The workspace was left unchanged.",
            store.automation_schema_version(),
            crate::automation::AUTOMATION_SCHEMA_VERSION
        ));
    }
    store.validate_automation_integrity()?;
    store.normalize_topics();
    store.normalize_planning_ownership();
    Ok(())
}

fn recovery_sidecar(path: &Path) -> PathBuf {
    path.with_extension("live-recovery.json")
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct RecoveryGroup {
    /// Generation assigned by the ordered worker. Zero means the group was
    /// still unsubmitted at snapshot time and must be appended after every
    /// accepted generation.
    pub generation: u64,
    /// Exact intent hash for an accepted group. Empty for an unsubmitted group.
    pub intent_hash: String,
    pub changes: Vec<crate::persistence_journal::EntityChange>,
    pub metadata: Vec<crate::persistence_journal::MetadataDelta>,
    pub effects: Vec<u64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct RecoveryPlan {
    /// Unique id written into the same atomic group as the recovered changes.
    /// A matching durable marker retires the sidecar exactly once.
    #[serde(default)]
    pub snapshot_id: String,
    pub incarnation: u64,
    /// Durable generation observed when the emergency plan was written.
    pub durable_generation: u64,
    /// Accepted groups in generation order followed by the final unsubmitted
    /// dirty delta. Boundaries are preserved so a partial drain can be replayed
    /// without overwriting committed groups or dropping later edits.
    pub groups: Vec<RecoveryGroup>,
}

/// Durable emergency copy of accepted-but-unacknowledged live changes. Used
/// only when an application quit bypasses the graceful receipt drain; startup
/// reapplies it before normal hydration so no edit is silently lost.
pub(crate) fn write_recovery(
    path: &Path,
    plan: &RecoveryPlan,
) -> Result<(), String> {
    if plan.groups.iter().all(|group| {
        group.changes.is_empty() && group.metadata.is_empty() && group.effects.is_empty()
    }) {
        return Ok(());
    }
    let payload = serde_json::to_vec(plan)
        .map_err(|error| format!("cannot encode live recovery: {error}"))?;
    let target = recovery_sidecar(path);
    let temporary = target.with_extension("json.tmp");
    let mut file = std::fs::File::create(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    file.write_all(&payload)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    drop(file);
    std::fs::rename(&temporary, &target)
        .map_err(|error| format!("cannot install {}: {error}", target.display()))?;
    if let Some(parent) = target.parent() {
        let _ = std::fs::File::open(parent).map(|directory| directory.sync_all());
    }
    Ok(())
}

pub(crate) fn read_recovery(path: &Path) -> Result<Option<RecoveryPlan>, String> {
    let target = recovery_sidecar(path);
    let payload = match std::fs::read(&target) {
        Ok(payload) => payload,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", target.display())),
    };
    let plan: RecoveryPlan = serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid live recovery {}: {error}", target.display()))?;
    Ok(Some(plan))
}

pub(crate) fn clear_recovery(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(recovery_sidecar(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot remove live recovery: {error}")),
    }
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
    let value = nanos ^ (u64::from(std::process::id()) << 32) ^ 0x9E37_79B9_7F4A_7C15;
    // Incarnations are persisted as SQLite INTEGER; keep them in i64 range.
    (value & (i64::MAX as u64)).max(1)
}

/// Reapply an emergency snapshot only when it belongs to this exact workspace
/// incarnation. A sidecar from a replaced workspace is retained for inspection
/// under a `.foreign` name; a future base generation is treated as invalid.
fn apply_recovery(
    path: &Path,
    incarnation: u64,
    durable_generation: u64,
    database: &crate::sqlite_v4::SqliteV4,
    store: &mut crate::domain::Store,
) -> Result<Option<String>, String> {
    let Some(plan) = read_recovery(path)? else {
        return Ok(None);
    };
    if !plan.snapshot_id.is_empty()
        && store.recovery_snapshot() == &plan.snapshot_id
    {
        // A crash after COMMIT but before sidecar deletion: the marker is
        // durable, so nothing is replayed.
        let _ = clear_recovery(path);
        return Ok(None);
    }
    if plan.incarnation != incarnation {
        let target = recovery_sidecar(path).with_extension(format!(
            "foreign-{}.json",
            plan.incarnation
        ));
        let _ = std::fs::rename(recovery_sidecar(path), &target);
        return Ok(Some(format!(
            "Live recovery belongs to workspace incarnation {} but this workspace is {}; the file was retained at {} and was not replayed.",
            plan.incarnation,
            incarnation,
            target.display()
        )));
    }
    // Validate the whole plan before mutating any loaded state. A later gap or
    // mismatched receipt must not leave earlier groups partially applied.
    let mut expected = durable_generation.saturating_add(1);
    for group in &plan.groups {
        if group.generation == 0 {
            continue;
        }
        if group.generation <= durable_generation {
            if group.intent_hash.is_empty()
                || !database.commit_matches(group.generation, &group.intent_hash)?
            {
                let target = recovery_sidecar(path).with_extension("stale.json");
                let _ = std::fs::rename(recovery_sidecar(path), &target);
                return Ok(Some(format!(
                    "Live recovery generation {} does not match the durable receipt; newer durable state was preserved and the snapshot is retained at {}.",
                    group.generation,
                    target.display()
                )));
            }
            continue;
        }
        if group.generation != expected {
            let target = recovery_sidecar(path).with_extension("stale.json");
            let _ = std::fs::rename(recovery_sidecar(path), &target);
            return Ok(Some(format!(
                "Live recovery has a generation gap at {} (expected {}); the snapshot is retained at {}.",
                group.generation,
                expected,
                target.display()
            )));
        }
        expected = expected.saturating_add(1);
    }
    // Only now apply: uncommitted accepted generations in order, then the final
    // unsubmitted tail (generation zero) last.
    let mut applied = false;
    for group in &plan.groups {
        if group.generation > durable_generation || group.generation == 0 {
            if !group.changes.is_empty() || !group.metadata.is_empty() {
                store.apply_live_journal_group(&group.changes, &group.metadata);
                applied = true;
            }
        }
    }
    if applied {
        // Persist the retirement marker in the same atomic group as the
        // recovered writes. A later crash after COMMIT can then retire the
        // sidecar without replaying over newer durable state.
        if !plan.snapshot_id.is_empty() {
            store.set_recovery_snapshot(plan.snapshot_id.clone());
        }
    } else {
        let _ = clear_recovery(path);
    }
    Ok(None)
}

/// Build an emergency plan from the exact accepted groups and the final dirty
/// snapshot. A dirty payload that exactly duplicates the latest accepted value
/// for the same key/field is dropped from the generation-zero tail, so a crash
/// after the accepted commit cannot replay an older duplicate over a later edit.
pub(crate) fn build_recovery_plan(
    incarnation: u64,
    durable_generation: u64,
    snapshot_id: String,
    accepted: Vec<crate::persistence_journal::StagedGroup>,
    tail: crate::domain::StoreDelta,
) -> RecoveryPlan {
    let mut groups = Vec::new();
    let mut latest_changes = std::collections::HashMap::new();
    let mut latest_metadata = std::collections::HashMap::new();
    for group in accepted {
        let intent_hash = crate::sqlite_v4::SqliteV4::group_intent_hash(&group)
            .unwrap_or_default();
        for change in group.changes() {
            latest_changes.insert(change.key(), change.clone());
        }
        for metadata in group.metadata() {
            latest_metadata.insert(metadata.field(), metadata.clone());
        }
        groups.push(RecoveryGroup {
            generation: group.generation(),
            intent_hash,
            changes: group.changes().to_vec(),
            metadata: group.metadata().to_vec(),
            effects: group.effects().to_vec(),
        });
    }
    let changes: Vec<_> = tail
        .changes
        .into_iter()
        .filter(|change| latest_changes.get(&change.key()) != Some(change))
        .collect();
    let metadata: Vec<_> = tail
        .metadata
        .into_iter()
        .filter(|metadata| latest_metadata.get(metadata.field()) != Some(metadata))
        .collect();
    if !changes.is_empty() || !metadata.is_empty() {
        groups.push(RecoveryGroup {
            generation: 0,
            intent_hash: String::new(),
            changes,
            metadata,
            effects: Vec::new(),
        });
    }
    RecoveryPlan {
        snapshot_id,
        incarnation,
        durable_generation,
        groups,
    }
}

/// Create the incremental backend for a brand-new workspace. The caller holds
/// the workspace lock. An empty store is seeded so the first worker can resume
/// at generation zero with the same incarnation sidecar.
pub(crate) fn create_new(storage: &Storage) -> Result<Option<LiveStartup>, String> {
    let path = storage.path();
    if !path.extension().is_some_and(|extension| extension == "sqlite3") || path.exists() {
        return Ok(None);
    }
    let root = storage.workspace_root().to_path_buf();
    let incarnation = fresh_incarnation();
    write_sidecar(&sidecar(path), incarnation)?;
    let mut store = crate::domain::Store::empty();
    normalize_loaded(&mut store)?;
    crate::sqlite_v4::SqliteV4::create_seeded(path, &root, incarnation, &store)?;
    Ok(Some(LiveStartup {
        path: path.to_path_buf(),
        root,
        incarnation,
        migrated_store: Some(store),
        recovery_warning: None,
    }))
}

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
            let mut store = database.load_store_lazy()?;
            let recovery_warning = apply_recovery(
                path,
                incarnation,
                database.durable_generation()?,
                &database,
                &mut store,
            )?;
            normalize_loaded(&mut store)?;
            Ok(Some(LiveStartup {
                path: path.to_path_buf(),
                root,
                incarnation,
                migrated_store: Some(store),
                recovery_warning,
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
            let mut store = database.load_store_lazy()?;
            let recovery_warning = apply_recovery(
                path,
                incarnation,
                database.durable_generation()?,
                &database,
                &mut store,
            )?;
            normalize_loaded(&mut store)?;
            Ok(Some(LiveStartup {
                path: path.to_path_buf(),
                root,
                incarnation,
                migrated_store: Some(store),
                recovery_warning,
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
        let loaded = selected
            .migrated_store
            .as_ref()
            .expect("v4 startup loads exactly one verified store");
        assert_eq!(loaded, &store);
        assert!(selected.recovery_warning.is_none());
        assert_eq!(read_sidecar(&sidecar(&selected.path)).unwrap(), Some(incarnation));
    }

    fn temp_workspace(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "t14-live-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        (root.clone(), root.join("workspace.sqlite3"))
    }

    fn changed_task(store: &Store, title: &str) -> crate::persistence_journal::EntityChange {
        let mut task = store.task(1).expect("mock task 1").clone();
        task.title = title.into();
        crate::persistence_journal::EntityChange::Upsert(
            crate::persistence_journal::EntityPayload::Task(task),
        )
    }

    fn unsent(store: &Store, title: &str) -> RecoveryGroup {
        RecoveryGroup {
            generation: 0,
            intent_hash: String::new(),
            changes: vec![changed_task(store, title)],
            metadata: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn accepted(
        generation: u64,
        hash: String,
        store: &Store,
        title: &str,
    ) -> RecoveryGroup {
        RecoveryGroup {
            generation,
            intent_hash: hash,
            changes: vec![changed_task(store, title)],
            metadata: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(incarnation: u64, durable_generation: u64, groups: Vec<RecoveryGroup>) -> RecoveryPlan {
        RecoveryPlan {
            snapshot_id: String::new(),
            incarnation,
            durable_generation,
            groups,
        }
    }

    fn staged(
        journal: &mut crate::persistence_journal::ChangeJournal,
        store: &Store,
        title: &str,
    ) -> (crate::persistence_journal::StagedGroup, String) {
        let group = journal
            .stage(vec![changed_task(store, title)])
            .expect("stage recovery group");
        let hash = crate::sqlite_v4::SqliteV4::group_intent_hash(&group).unwrap();
        (group, hash)
    }

    #[test]
    fn same_incarnation_recovery_replays_and_preserves_untouched_state() {
        let (root, path) = temp_workspace("recovery-apply");
        let store = Store::mock();
        let incarnation = 8_001;
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        write_recovery(
            &path,
            &plan(incarnation, 0, vec![unsent(&store, "Recovered title")]),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Recovered title");
        assert_eq!(loaded.tasks().len(), store.tasks().len());
        assert_eq!(loaded.default_model(), store.default_model());
        assert!(loaded.has_tracked_changes(), "recovery remains to be committed");
    }

    #[test]
    fn foreign_incarnation_recovery_is_retained_but_not_replayed() {
        let (root, path) = temp_workspace("recovery-foreign");
        let store = Store::mock();
        let incarnation = 8_002;
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        write_recovery(
            &path,
            &plan(incarnation + 1, 0, vec![unsent(&store, "Foreign title")]),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_some());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, store.task(1).unwrap().title);
        assert!(!recovery_sidecar(&path).exists());
        assert!(std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("foreign-")));
    }

    #[test]
    fn recovery_generation_gap_is_rejected_without_replay() {
        let (root, path) = temp_workspace("recovery-gap");
        let store = Store::mock();
        let incarnation = 8_003;
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        journal.stage(vec![changed_task(&store, "Ignored")]).unwrap();
        journal.stage(vec![changed_task(&store, "Ignored")]).unwrap();
        // Generation 3 cannot follow durable 0 without generations 1 and 2.
        write_recovery(
            &path,
            &plan(
                incarnation,
                0,
                vec![accepted(3, "hash".into(), &store, "Gap title")],
            ),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_some());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, store.task(1).unwrap().title);
    }

    #[test]
    fn committed_accepted_group_is_retired_without_replay() {
        let (root, path) = temp_workspace("recovery-committed");
        let store = Store::mock();
        let incarnation = 8_004;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (group, hash) = staged(&mut journal, &store, "Committed title");
        database.commit_group(&group).unwrap();
        drop(database);
        write_recovery(
            &path,
            &plan(
                incarnation,
                1,
                vec![accepted(1, hash, &store, "Committed title")],
            ),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Committed title");
        assert_eq!(loaded.tasks().len(), store.tasks().len());
        assert!(!loaded.has_tracked_changes(), "committed group is already durable");
    }

    #[test]
    fn stale_recovery_cannot_roll_back_a_later_durable_edit() {
        let (root, path) = temp_workspace("recovery-stale-later");
        let store = Store::mock();
        let incarnation = 8_005;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (first, first_hash) = staged(&mut journal, &store, "Committed title");
        database.commit_group(&first).unwrap();
        let (second, _) = staged(&mut journal, &store, "Later durable title");
        database.commit_group(&second).unwrap();
        drop(database);
        // A stale snapshot that only remembers generation 1. It matches the
        // durable receipt at that generation, so it is retired; generation 2
        // must survive untouched.
        write_recovery(
            &path,
            &plan(
                incarnation,
                1,
                vec![accepted(1, first_hash, &store, "Committed title")],
            ),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Later durable title");
    }

    #[test]
    fn partially_committed_plan_keeps_the_uncommitted_tail() {
        let (root, path) = temp_workspace("recovery-partial");
        let store = Store::mock();
        let incarnation = 8_006;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (accepted_group, accepted_hash) = staged(&mut journal, &store, "Committed A");
        database.commit_group(&accepted_group).unwrap();
        drop(database);
        // A committed, B still unsubmitted in the same emergency plan.
        write_recovery(
            &path,
            &plan(
                incarnation,
                1,
                vec![
                    accepted(1, accepted_hash, &store, "Committed A"),
                    unsent(&store, "Uncommitted B"),
                ],
            ),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Uncommitted B");
        assert!(loaded.has_tracked_changes());
    }

    #[test]
    fn actual_plan_drops_duplicate_accepted_tail() {
        let store = Store::mock();
        let incarnation = 8_009;
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (group, _) = staged(&mut journal, &store, "Accepted A");
        let duplicate = crate::domain::StoreDelta {
            changes: vec![changed_task(&store, "Accepted A")],
            metadata: Vec::new(),
            epoch: 1,
        };
        let plan = build_recovery_plan(
            incarnation,
            1,
            "snap".into(),
            vec![group],
            duplicate,
        );
        assert_eq!(plan.groups.len(), 1);
        assert!(plan.groups.iter().all(|group| group.generation > 0));
    }

    #[test]
    fn actual_plan_keeps_distinct_unsubmitted_tail() {
        let store = Store::mock();
        let incarnation = 8_010;
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (group, _) = staged(&mut journal, &store, "Accepted A");
        let tail = crate::domain::StoreDelta {
            changes: vec![changed_task(&store, "Unsubmitted B")],
            metadata: Vec::new(),
            epoch: 2,
        };
        let plan = build_recovery_plan(
            incarnation,
            1,
            "snap".into(),
            vec![group],
            tail,
        );
        assert_eq!(plan.groups.len(), 2);
        assert_eq!(plan.groups[1].generation, 0);
        assert_eq!(plan.groups[1].changes.len(), 1);
    }

    #[test]
    fn actual_plan_partial_drain_recovers_only_uncommitted_tail() {
        let (root, path) = temp_workspace("recovery-actual-partial");
        let store = Store::mock();
        let incarnation = 8_011;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (group, _) = staged(&mut journal, &store, "Accepted A");
        database.commit_group(&group).unwrap();
        drop(database);
        let plan = build_recovery_plan(
            incarnation,
            1,
            "snap-partial".into(),
            vec![group],
            crate::domain::StoreDelta {
                changes: vec![changed_task(&store, "Uncommitted B")],
                metadata: Vec::new(),
                epoch: 2,
            },
        );
        write_recovery(&path, &plan).unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Uncommitted B");
        assert_eq!(
            loaded.recovery_snapshot(),
            "snap-partial",
            "retirement marker travels with the recovered group"
        );
    }

    #[test]
    fn duplicate_tail_cannot_roll_back_a_later_durable_edit_after_marker() {
        let (root, path) = temp_workspace("recovery-tail-marker");
        let store = Store::mock();
        let incarnation = 8_007;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (accepted_group, accepted_hash) = staged(&mut journal, &store, "Accepted A");
        database.commit_group(&accepted_group).unwrap();
        let snapshot_id = "snap-A".to_string();
        let plan = plan(
            incarnation,
            1,
            vec![
                accepted(1, accepted_hash, &store, "Accepted A"),
                unsent(&store, "Accepted A"),
            ],
        );
        write_recovery(&path, &plan).unwrap();
        // The recovery tail and its retirement marker commit together.
        let recovery = journal
            .stage_with_metadata(
                vec![changed_task(&store, "Accepted A")],
                vec![crate::persistence_journal::MetadataDelta::RecoverySnapshot(
                    snapshot_id.clone(),
                )],
                Vec::new(),
            )
            .unwrap();
        database.commit_group(&recovery).unwrap();
        // A later, newer durable edit on the same entity.
        let (later, _) = staged(&mut journal, &store, "Later durable B");
        database.commit_group(&later).unwrap();
        drop(database);
        // Crash after COMMIT but before the sidecar was deleted. The inherited
        // zero tail is now stale and must not reapply A over B.
        let mut stale_plan = plan;
        stale_plan.snapshot_id = snapshot_id;
        write_recovery(&path, &stale_plan).unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_none());
        let loaded = selected.migrated_store.as_ref().unwrap();
        assert_eq!(loaded.task(1).unwrap().title, "Later durable B");
        assert!(!recovery_sidecar(&path).exists());
    }

    #[test]
    fn invalid_later_group_prevents_all_partial_application() {
        let (root, path) = temp_workspace("recovery-validate-first");
        let store = Store::mock();
        let incarnation = 8_008;
        let mut database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, incarnation, &store).unwrap();
        let mut journal = crate::persistence_journal::ChangeJournal::new(incarnation);
        let (first, first_hash) = staged(&mut journal, &store, "Applied first");
        database.commit_group(&first).unwrap();
        drop(database);
        write_recovery(
            &path,
            &plan(
                incarnation,
                1,
                vec![
                    accepted(1, first_hash, &store, "Applied first"),
                    // Generation 3 is a gap after durable 1.
                    accepted(3, "missing-hash".into(), &store, "Should not apply"),
                    unsent(&store, "Tail should not apply"),
                ],
            ),
        )
        .unwrap();
        let selected = select_existing(&Storage::new(path.clone()))
            .unwrap()
            .expect("v4 selected");
        assert!(selected.recovery_warning.is_some());
        let loaded = selected.migrated_store.as_ref().unwrap();
        // The durable first group is present; no recovered tail was applied.
        assert_eq!(loaded.task(1).unwrap().title, "Applied first");
        assert!(!loaded.has_tracked_changes());
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
