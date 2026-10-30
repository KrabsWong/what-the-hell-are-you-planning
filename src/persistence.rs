//! Versioned local workspace storage. Writes replace the old file only after a complete flush.
use crate::domain::Store;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const VERSION: u32 = 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct Storage {
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct WorkspaceFile {
    version: u32,
    workspace: Store,
}

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    #[cfg(feature = "qa")]
    pub fn default_path() -> Result<PathBuf, String> {
        for variable in ["WTHAYP_DATA_DIR", "TASK_MANAGER_DATA_DIR"] {
            if let Some(directory) = std::env::var_os(variable) {
                if directory.is_empty() { return Err(format!("{variable} must name a directory.")); }
                return Ok(PathBuf::from(directory).join("workspace.sqlite3"));
            }
        }
        crate::workspace::configured()?.map(|root| root.join("workspace/workspace.sqlite3"))
            .ok_or_else(|| "Choose a workspace in the app before continuing.".into())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep the returned file alive for the application's lifetime. Never unlink the lock file:
    /// other instances must lock the same inode even after the current holder exits.
    pub fn acquire_lock(&self) -> Result<fs::File, String> {
        let path = self.ensure_parent()?.join("workspace.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|error| format!("Cannot open workspace lock {}: {error}", path.display()))?;
        file.try_lock().map_err(|error| {
            format!(
                "Cannot lock {}. Another app instance may already have this workspace open: {error}",
                path.display()
            )
        })?;
        Ok(file)
    }

    /// A missing file is a new workspace; invalid files are never treated as empty data.
    pub fn load(&self) -> Result<Option<Store>, String> {
        if self
            .path
            .extension()
            .is_some_and(|extension| extension == "sqlite3")
        {
            if self.path.exists() {
                let mut loaded = crate::sqlite_storage::load(&self.path)?;
                if let Some(store) = &mut loaded { store.normalize_topics(); }
                return Ok(loaded);
            }
            let legacy = Storage::new(self.path.with_file_name("workspace.json"));
            let Some(store) = legacy.load()? else {
                return Ok(None);
            };
            let parent = self.ensure_parent()?;
            let temporary = parent.join(format!(
                ".migration-{}-{}.sqlite3",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let migrated = (|| {
                crate::sqlite_storage::save(&temporary, &store)?;
                if crate::sqlite_storage::load(&temporary)? != Some(store.clone()) {
                    return Err("SQLite migration verification failed".into());
                }
                fs::rename(&temporary, &self.path)
                    .map_err(|e| format!("Cannot install migrated workspace: {e}"))?;
                Ok(Some(store))
            })();
            if migrated.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            return migrated;
        }
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!("Cannot read {}: {error}", self.path.display()));
            }
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid workspace file {}: {error}", self.path.display()))?;
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        if !version.is_some_and(|v| (1..=u64::from(VERSION)).contains(&v)) {
            return Err(format!(
                "Unsupported workspace version in {} (expected {VERSION}, found {}). The file was left unchanged.",
                self.path.display(),
                version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "missing or invalid".into())
            ));
        }
        let mut file: WorkspaceFile = serde_json::from_value(value).map_err(|error| {
            format!("Invalid workspace data in {}: {error}", self.path.display())
        })?;
        if file.version < 4 && file.workspace.default_model.is_empty() {
            let common = &file.workspace.research_config.models[0];
            if crate::opencode::valid_model(common)
                && file
                    .workspace
                    .research_config
                    .models
                    .iter()
                    .all(|m| m == common)
            {
                let common = common.clone();
                file.workspace.default_model = common.clone();
                file.workspace.research_config.models = Default::default();
                for policy in &mut file.workspace.policies {
                    if policy.agent == "OpenCode" && policy.model == common {
                        policy.model.clear();
                    }
                }
            }
        }
        file.workspace.normalize_topics();
        Ok(Some(file.workspace))
    }

    pub fn save(&self, workspace: &Store) -> Result<(), String> {
        // Refuse to overwrite damaged data or a file written by a newer application.
        self.load()?;
        if self
            .path
            .extension()
            .is_some_and(|extension| extension == "sqlite3")
        {
            self.ensure_parent()?;
            return crate::sqlite_storage::save(&self.path, workspace);
        }
        let mut bytes = serde_json::to_vec_pretty(&WorkspaceFile {
            version: VERSION,
            workspace: workspace.clone(),
        })
        .map_err(|error| format!("Cannot serialize workspace: {error}"))?;
        bytes.push(b'\n');
        let parent = self.ensure_parent()?;
        let file_name = self
            .path
            .file_name()
            .ok_or("Workspace path must name a file.")?;
        let temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("Cannot create workspace temporary file: {error}"))?;
        let result = (|| -> std::io::Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &self.path)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "Cannot save workspace to {}: {error}",
                self.path.display()
            ));
        }
        Ok(())
    }

    /// One-time, explicitly reviewed cleanup. The caller must own the workspace lock.
    /// Never infer demonstration ownership from an ID or title in normal startup.
    #[cfg(any(test, feature = "qa"))]
    pub fn remove_reviewed_demo(
        &self,
        workspace: &mut Store,
        reviewed_ids: &[u64],
    ) -> Result<Option<PathBuf>, String> {
        let seed = Store::mock();
        let ids: Vec<_> = reviewed_ids
            .iter()
            .copied()
            .filter(|id| workspace.task(*id).is_some())
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        for id in &ids {
            let task = workspace.task(*id).unwrap();
            if seed.task(*id).is_none_or(|original| {
                original.title != task.title
                    || original.description != task.description
                    || original.comments != task.comments
            }) || task.research.is_some()
                || !task.attachments.is_empty()
                || workspace.runs.iter().any(|run| run.task_id == *id)
            {
                return Err(format!(
                    "Task {id} contains real or unrecognized data; it was not removed."
                ));
            }
        }
        if workspace.tasks.iter().any(|task| {
            !ids.contains(&task.id)
                && (task.parent_id.is_some_and(|parent| ids.contains(&parent))
                    || task.research.as_ref().is_some_and(|research| {
                        ids.contains(&research.goal_id)
                            || research.depends_on.iter().any(|id| ids.contains(id))
                    }))
        }) {
            return Err(
                "A retained task references the reviewed sample tasks; nothing was removed.".into(),
            );
        }
        let bytes = fs::read(&self.path).map_err(|e| format!("Cannot back up workspace: {e}"))?;
        let parent = self.ensure_parent()?;
        let backup = parent.join(format!(
            "workspace.before-demo-removal.{}-{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&backup)
            .map_err(|e| format!("Cannot create backup: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot save backup: {e}"))?;
        let mut cleaned = workspace.clone();
        cleaned.tasks.retain(|task| !ids.contains(&task.id));
        cleaned.projects.retain(|project| {
            !seed.projects.contains(project)
                || cleaned
                    .tasks
                    .iter()
                    .any(|task| task.project_id == project.id)
        });
        if cleaned.projects.is_empty() {
            cleaned.projects = Store::empty().projects;
        }
        let defaults = Store::empty();
        for policy in &mut cleaned.policies {
            if seed.policies.contains(policy)
                && let Some(default) = defaults.policies.iter().find(|p| p.stage == policy.stage)
            {
                *policy = default.clone();
            }
        }
        // Keep next_id monotonic and preserve configured policies, real runs and settings.
        self.save(&cleaned)?;
        *workspace = cleaned;
        Ok(Some(backup))
    }

    fn ensure_parent(&self) -> Result<&Path, String> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(parent).map_err(|error| {
            format!(
                "Cannot create workspace directory {}: {error}",
                parent.display()
            )
        })?;
        Ok(parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Stage;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "task-manager-storage-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self(directory)
        }
        fn storage(&self) -> Storage {
            Storage::new(self.0.join("data/workspace.json"))
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sqlite_migrates_legacy_without_modifying_backup_and_reopens() {
        let directory = TestDirectory::new();
        let legacy = directory.storage();
        let mut original = Store::mock();
        original.simulate_run(1).unwrap();
        original.add_comment(1, "migration survives").unwrap();
        legacy.save(&original).unwrap();
        let backup = fs::read(legacy.path()).unwrap();
        let database = Storage::new(legacy.path().with_extension("sqlite3"));
        assert_eq!(database.load().unwrap(), Some(original.clone()));
        assert_eq!(fs::read(legacy.path()).unwrap(), backup);
        original.set_stage(1, Stage::Done).unwrap();
        database.save(&original).unwrap();
        assert_eq!(
            Storage::new(database.path().into()).load().unwrap(),
            Some(original)
        );
        assert_eq!(fs::read(legacy.path()).unwrap(), backup);
        let db = rusqlite::Connection::open(database.path()).unwrap();
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }

    #[test]
    fn sqlite_topics_open_legacy_schema_and_roundtrip_membership() {
        let directory = TestDirectory::new();
        let database = Storage::new(directory.0.join("workspace.sqlite3"));
        let mut original = Store::empty();
        let goal = original.create_research_goal("Quarterly review".into(), "Analyze earnings".into(), 1).unwrap();
        original.normalize_topics();
        database.save(&original).unwrap();
        let db = rusqlite::Connection::open(database.path()).unwrap();
        db.execute("UPDATE workspace SET payload=json_remove(payload, '$.topics')", []).unwrap();
        db.execute("UPDATE tasks SET payload=json_remove(payload, '$.topic_id')", []).unwrap();
        drop(db);
        let mut loaded = database.load().unwrap().unwrap();
        assert_eq!(loaded, original);
        let topic = loaded.create_topic(1, "Company research").unwrap();
        loaded.set_goal_topic(goal, Some(topic)).unwrap();
        loaded.set_topic_archived(topic, true).unwrap();
        database.save(&loaded).unwrap();
        assert_eq!(database.load().unwrap(), Some(loaded));
    }

    #[test]
    fn sqlite_failed_write_rolls_back_all_tables() {
        let directory = TestDirectory::new();
        let database = Storage::new(directory.0.join("workspace.sqlite3"));
        let original = Store::mock();
        database.save(&original).unwrap();
        let db = rusqlite::Connection::open(database.path()).unwrap();
        db.execute_batch("CREATE TRIGGER reject_projects BEFORE INSERT ON projects BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        drop(db);
        let mut changed = original.clone();
        changed.task_mut(1).unwrap().title = "must not persist".into();
        assert!(database.save(&changed).is_err());
        assert_eq!(database.load().unwrap(), Some(original));
    }

    #[test]
    fn sqlite_rejects_corrupt_legacy_and_future_database_without_overwriting() {
        let directory = TestDirectory::new();
        let legacy = directory.storage();
        fs::create_dir_all(legacy.path().parent().unwrap()).unwrap();
        fs::write(legacy.path(), "broken json").unwrap();
        let database = Storage::new(legacy.path().with_extension("sqlite3"));
        assert!(database.load().is_err());
        assert!(!database.path().exists());
        fs::remove_file(legacy.path()).unwrap();
        database.save(&Store::empty()).unwrap();
        let db = rusqlite::Connection::open(database.path()).unwrap();
        db.pragma_update(None, "user_version", 999).unwrap();
        drop(db);
        let before = fs::read(database.path()).unwrap();
        assert!(database.load().is_err());
        assert!(database.save(&Store::empty()).is_err());
        assert_eq!(fs::read(database.path()).unwrap(), before);
    }

    #[test]
    fn round_trip_preserves_tasks_stage_policies_and_run_history() {
        let directory = TestDirectory::new();
        let storage = directory.storage();
        let mut workspace = Store::mock();
        workspace.default_model = "test/global".into();
        workspace.task_mut(1).unwrap().title = "持久化任务".into();
        let policy = workspace
            .policies
            .iter_mut()
            .find(|p| p.stage == Stage::Review)
            .unwrap();
        policy.agent = "OpenCode".into();
        policy.model = "provider/review-model".into();
        workspace.set_stage(1, Stage::Review).unwrap();
        workspace.simulate_run(1).unwrap();
        storage.save(&workspace).unwrap();
        assert_eq!(
            Storage::new(storage.path().into()).load().unwrap(),
            Some(workspace.clone())
        );
        workspace.add_comment(1, "Saved after reopening").unwrap();
        storage.save(&workspace).unwrap();
        assert_eq!(storage.load().unwrap(), Some(workspace));
        assert_eq!(
            fs::read_dir(storage.path().parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }

    fn write_legacy_workspace(storage: &Storage, store: &Store, version: u32) {
        let mut data = serde_json::json!({"version": version, "workspace": store});
        data["workspace"]
            .as_object_mut()
            .unwrap()
            .remove("default_model");
        fs::create_dir_all(storage.path().parent().unwrap()).unwrap();
        fs::write(storage.path(), serde_json::to_vec(&data).unwrap()).unwrap();
    }

    #[test]
    fn uniform_legacy_models_migrate_without_changing_requests_or_manual_overrides() {
        let directory = TestDirectory::new();
        let storage = directory.storage();
        let mut old = Store::mock();
        let common = "tencent-token-plan/deepseek/deepseek-flash";
        old.research_config.models = std::array::from_fn(|_| common.into());
        old.policies[0].agent = "OpenCode".into();
        old.policies[0].model = common.into();
        old.policies[1].agent = "OpenCode".into();
        old.policies[1].model = "test/intentional-override".into();
        old.policies[2].agent = "Manual".into();
        old.policies[2].model = common.into();
        old.projects[0].directory = "/tmp".into();
        old.set_stage(1, old.policies[0].stage).unwrap();
        let run = old.prepare_run(1).unwrap();
        old.runs.push(run);
        write_legacy_workspace(&storage, &old, 3);
        let before_bytes = fs::read(storage.path()).unwrap();
        let restored = storage.load().unwrap().unwrap();
        let mut expected = old.clone();
        expected.default_model = common.into();
        expected.research_config.models = Default::default();
        expected.policies[0].model.clear();
        assert_eq!(restored, expected);
        assert_eq!(restored.prepare_run(1).unwrap().model, common);
        assert_eq!(restored.tasks, old.tasks);
        assert_eq!(restored.runs, old.runs);
        assert_eq!(fs::read(storage.path()).unwrap(), before_bytes);
        assert_eq!(storage.load().unwrap(), Some(expected.clone()));
        storage.save(&restored).unwrap();
        assert_eq!(storage.load().unwrap(), Some(expected));
        let data: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.path()).unwrap()).unwrap();
        assert_eq!(data["version"], 4);
    }

    #[test]
    fn mixed_invalid_and_current_model_settings_remain_explicit() {
        let directory = TestDirectory::new();
        let storage = directory.storage();
        for models in [
            ["test/a", "test/b", "test/a", "test/a"],
            ["test/a", "", "test/a", "test/a"],
            ["invalid"; 4],
            [""; 4],
        ] {
            let mut old = Store::mock();
            old.research_config.models = models.map(String::from);
            write_legacy_workspace(&storage, &old, 3);
            assert_eq!(storage.load().unwrap(), Some(old));
        }
        let mut current = Store::mock();
        current.research_config.models = std::array::from_fn(|_| "test/explicit".into());
        storage.save(&current).unwrap();
        assert_eq!(storage.load().unwrap(), Some(current));
    }

    #[test]
    fn missing_file_is_new_workspace_without_creating_a_file() {
        let directory = TestDirectory::new();
        let storage = directory.storage();
        assert_eq!(storage.load().unwrap(), None);
        assert!(!storage.path().exists());
    }

    #[test]
    fn reviewed_demo_cleanup_backs_up_exact_bytes_and_keeps_real_data() {
        let dir = TestDirectory::new();
        let storage = dir.storage();
        let _lock = storage.acquire_lock().unwrap();
        let mut store = Store::mock();
        store.projects.push(crate::domain::Project {
            id: 3,
            name: "Real research".into(),
            key: "RES".into(),
            directory: "/tmp/research".into(),
            research_brief: String::new(),
        });
        let real_id = store
            .create_research_goal("Research".into(), "Real goal".into(), 3)
            .unwrap();
        store.policies[0].model = "user/configured-model".into();
        storage.save(&store).unwrap();
        let original_bytes = fs::read(storage.path()).unwrap();
        let real_task = store.task(real_id).unwrap().clone();
        let next_id = store.next_id;
        let backup = storage
            .remove_reviewed_demo(&mut store, &(1..=22).collect::<Vec<_>>())
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), original_bytes);
        assert_eq!(store.tasks, vec![real_task]);
        assert_eq!(store.projects.len(), 1);
        assert_eq!(store.projects[0].id, 3);
        assert_eq!(store.next_id, next_id);
        assert_eq!(store.policies[0].model, "user/configured-model");
        // Loading upgrades approval metadata, while cleanup itself preserves the real task exactly.
        let mut expected_loaded = store.clone();
        expected_loaded.normalize_topics();
        let loaded = storage.load().unwrap().unwrap();
        assert_eq!(loaded, expected_loaded);
        assert_eq!(loaded.task(real_id).unwrap().stage, store.task(real_id).unwrap().stage);
        assert!(loaded.goal_execution_approved(real_id));
        assert_eq!(
            storage
                .remove_reviewed_demo(&mut store, &(1..=22).collect::<Vec<_>>())
                .unwrap(),
            None
        );
    }

    #[test]
    fn reviewed_demo_cleanup_rejects_real_runs_and_retained_dependencies_atomically() {
        let dir = TestDirectory::new();
        let storage = dir.storage();
        let _lock = storage.acquire_lock().unwrap();
        let mut store = Store::mock();
        storage.save(&store).unwrap();
        let original = store.clone();
        assert!(storage.remove_reviewed_demo(&mut store, &[1]).is_err());
        assert_eq!(store, original);
        store.simulate_run(10).unwrap();
        let before = store.clone();
        assert!(storage.remove_reviewed_demo(&mut store, &[10]).is_err());
        assert_eq!(store, before);
        store.task_mut(12).unwrap().title = "User edited title".into();
        let before = store.clone();
        assert!(storage.remove_reviewed_demo(&mut store, &[12]).is_err());
        assert_eq!(store, before);
        assert_eq!(storage.load().unwrap(), Some(original));
    }

    #[test]
    fn older_workspaces_default_to_light_without_losing_configuration() {
        let dir = TestDirectory::new();
        let storage = dir.storage();
        let store = Store::empty();
        storage.save(&store).unwrap();
        let mut data: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.path()).unwrap()).unwrap();
        data["workspace"]
            .as_object_mut()
            .unwrap()
            .remove("appearance");
        fs::write(storage.path(), serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(storage.load().unwrap(), Some(store));
    }

    #[test]
    fn workspace_lock_excludes_a_second_instance_until_released() {
        let directory = TestDirectory::new();
        let first = directory.storage();
        let second = directory.storage();
        let lock = first.acquire_lock().unwrap();
        let error = second.acquire_lock().unwrap_err();
        assert!(error.contains("Another app instance"));
        drop(lock);
        // Concurrent process tests may briefly inherit descriptors between fork and exec.
        // The close-on-exec descriptor is released before the spawned program runs.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let _next_lock = loop {
            match second.acquire_lock() {
                Ok(lock) => break lock,
                Err(error) => {
                    assert!(std::time::Instant::now() < deadline, "{error}");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        };
        assert!(
            first
                .path()
                .parent()
                .unwrap()
                .join("workspace.lock")
                .is_file()
        );
        assert!(!first.path().exists());
    }

    #[test]
    fn corrupted_and_unknown_version_files_are_preserved() {
        let directory = TestDirectory::new();
        let storage = directory.storage();
        fs::create_dir_all(storage.path().parent().unwrap()).unwrap();
        for bytes in [
            b"not valid JSON".as_slice(),
            br#"{"version":99,"workspace":{}}"#,
            br#"{"version":1,"workspace":{}}"#,
            br#"{"workspace":{}}"#,
        ] {
            fs::write(storage.path(), bytes).unwrap();
            assert!(storage.load().is_err());
            assert!(storage.save(&Store::mock()).is_err());
            assert_eq!(fs::read(storage.path()).unwrap(), bytes);
        }
    }

    #[cfg(unix)]
    #[test]
    fn workspace_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = TestDirectory::new();
        let storage = directory.storage();
        let _lock = storage.acquire_lock().unwrap();
        storage.save(&Store::mock()).unwrap();
        assert_eq!(
            fs::metadata(storage.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(storage.path().parent().unwrap().join("workspace.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    #[test]
    fn upgrades_v1_without_losing_tasks_and_defaults_new_research_fields() {
        let dir = TestDirectory::new();
        let storage = dir.storage();
        let original = Store::mock();
        storage.save(&original).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.path()).unwrap()).unwrap();
        file["version"] = serde_json::json!(1);
        file["workspace"]
            .as_object_mut()
            .unwrap()
            .remove("research_config");
        for task in file["workspace"]["tasks"].as_array_mut().unwrap() {
            task.as_object_mut().unwrap().remove("research");
            task.as_object_mut().unwrap().remove("attachments");
        }
        fs::write(storage.path(), serde_json::to_vec(&file).unwrap()).unwrap();
        let loaded = storage.load().unwrap().unwrap();
        assert_eq!(loaded, original);
        storage.save(&loaded).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.path()).unwrap()).unwrap();
        assert_eq!(saved["version"], 4);
    }
    #[test]
    fn upgrades_v2_with_existing_research_approved_and_project_fields_defaulted() {
        let dir = TestDirectory::new();
        let storage = dir.storage();
        let mut original = Store::empty();
        let goal = original
            .create_research_goal("Existing research".into(), "Scope".into(), 1)
            .unwrap();
        let mut file = serde_json::json!({"version":2,"workspace":original});
        file["workspace"]["projects"][0]
            .as_object_mut()
            .unwrap()
            .remove("research_brief");
        file["workspace"]["tasks"][0]["research"]
            .as_object_mut()
            .unwrap()
            .remove("requires_approval");
        file["workspace"]["tasks"][0].as_object_mut().unwrap().remove("approved_plan");
        fs::create_dir_all(storage.path().parent().unwrap()).unwrap();
        let original_bytes = serde_json::to_vec(&file).unwrap();
        fs::write(storage.path(), &original_bytes).unwrap();
        let loaded = storage.load().unwrap().unwrap();
        original.normalize_topics();
        assert_eq!(loaded, original);
        assert_eq!(fs::read(storage.path()).unwrap(), original_bytes, "Loading must not rewrite the legacy source");
        assert!(loaded.task(goal).unwrap().research.as_ref().unwrap().requires_approval);
        assert!(loaded.task(goal).unwrap().approved_plan.is_some());
        assert!(!loaded.research_pending_approval(goal));
        storage.save(&loaded).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(storage.path()).unwrap()).unwrap();
        assert_eq!(
            saved["version"], 4,
            "Older binaries must refuse to drop approval gates and the global model"
        );
    }
}
