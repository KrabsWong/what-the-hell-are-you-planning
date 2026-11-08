//! The only bootstrap setting outside the user-selected workspace is its location.
use crate::{domain::{Project, ProjectKind, Store}, persistence::Storage};
use std::{fs, path::{Path, PathBuf}};

fn config_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
    Ok(PathBuf::from(home).join("Library/Application Support/What the hell are you planning/workspace-location.json"))
}

pub fn configured() -> Result<Option<PathBuf>, String> {
    let path = config_path()?;
    if !path.exists() { return Ok(None); }
    let root: PathBuf = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| format!("Invalid workspace location: {e}"))?;
    if !root.is_absolute() || !root.join("workspace/workspace.sqlite3").is_file() {
        return Err(format!("Workspace unavailable: {}. Reconnect this folder before continuing.", root.display()));
    }
    Ok(Some(root))
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "workspace.lock" || name == "workspace-location.json" { continue; }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let destination = target.join(name);
        if kind.is_symlink() {
            // Materialize file links (e.g. a virtualenv Python executable); never follow directory links.
            if !entry.path().is_file() { return Err(format!("Cannot migrate directory or broken symbolic link: {}", entry.path().display())); }
            fs::copy(entry.path(), &destination).map_err(|e| e.to_string())?;
            continue;
        }
        if kind.is_dir() { copy_tree(&entry.path(), &destination)?; }
        else if kind.is_file() { fs::copy(entry.path(), destination).map_err(|e| e.to_string())?; }
        else { return Err(format!("Unsupported file: {}", entry.path().display())); }
    }
    Ok(())
}

/// Canonicalize before checking containment, including symlinked ancestors.
pub fn check_directory(root: &Path, directory: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !directory.is_absolute() { return Err("An absolute project directory is required.".into()); }
    let mut existing = directory;
    while !existing.exists() { existing = existing.parent().ok_or("Invalid project directory")?; }
    let canonical = existing.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root) || directory.components().any(|c| c == std::path::Component::ParentDir) {
        return Err("Project and task folders must be inside the configured workspace.".into());
    }
    Ok(())
}

/// The single subfolder that may contain a project of this kind, relative to the
/// workspace data directory. Ordinary projects get per-id folders under
/// `projects/`; the one generated automation container lives under `automations/`.
fn kind_root_name(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::User => "projects",
        ProjectKind::Automation => "automations",
    }
}

/// Absolute folder that owns every project of `kind`.
pub fn allowed_root(workspace: &Path, kind: ProjectKind) -> PathBuf {
    workspace.join(kind_root_name(kind))
}

fn kind_relative_path(kind: ProjectKind, id: u64) -> PathBuf {
    match kind {
        ProjectKind::User => Path::new(kind_root_name(kind)).join(format!("project-{id}")),
        ProjectKind::Automation => PathBuf::from(kind_root_name(kind)),
    }
}

/// Absolute folder used for one project of `kind`. The automation container is
/// shared, so its id is ignored.
pub fn project_path(workspace: &Path, kind: ProjectKind, id: u64) -> PathBuf {
    workspace.join(kind_relative_path(kind, id))
}

fn create_project_directory(workspace: &Path, kind: ProjectKind, id: u64) -> Result<String, String> {
    let path = project_path(workspace, kind, id);
    check_directory(workspace, &path)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

pub fn project_directory(root: &Path, id: u64) -> Result<String, String> {
    create_project_directory(root, ProjectKind::User, id)
}

/// Automation output lives in its own generated project folder so manual projects
/// and automation results never share a directory tree.
pub fn automation_directory(root: &Path) -> Result<String, String> {
    create_project_directory(root, ProjectKind::Automation, 0)
}

/// Validate one loaded project against the folder its kind allows. The whole
/// workspace is checked first, so symlink escapes and parent traversal fail
/// before the kind-specific root is considered; the error names the project and
/// the required root for genuinely invalid data.
pub fn check_project_directory(workspace: &Path, project: &Project) -> Result<(), String> {
    let required = allowed_root(workspace, project.kind);
    let directory = Path::new(&project.directory);
    let invalid = || {
        format!(
            "Project \"{}\" (id {}) must be inside {}.",
            project.name,
            project.id,
            required.display()
        )
    };
    check_directory(workspace, directory).map_err(|_| invalid())?;
    check_directory(&required, directory).map_err(|_| invalid())?;
    Ok(())
}

/// A runtime directory is valid when it lives under any project-kind root.
pub fn check_project_roots(workspace: &Path, directory: &Path) -> Result<(), String> {
    // Whole-workspace containment first: a kind root that is itself a symlink to
    // an external folder must not make that folder's children look valid.
    check_directory(workspace, directory)?;
    for kind in [ProjectKind::User, ProjectKind::Automation] {
        if check_directory(&allowed_root(workspace, kind), directory).is_ok() {
            return Ok(());
        }
    }
    check_directory(&allowed_root(workspace, ProjectKind::User), directory)
}

/// Import a closed legacy workspace without changing its original files. Installation is atomic.
pub fn initialize(root: &Path, legacy: Option<&Path>) -> Result<PathBuf, String> {
    if !root.is_absolute() || !root.is_dir() { return Err("Choose an existing absolute workspace folder.".into()); }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let destination = root.join("workspace");
    if destination.exists() {
        let storage = Storage::new(destination.join("workspace.sqlite3"));
        let _lock = storage.acquire_lock()?;
        let store = storage.load()?.ok_or("The selected folder contains an incomplete workspace.")?;
        for project in store.projects() {
            check_project_directory(&destination, project)?;
            if !Path::new(&project.directory).is_dir() { return Err(format!("Project folder is missing: {}", project.name)); }
        }
        return Ok(destination);
    }
    let staging = root.join(format!(".workspace-setup-{}", std::process::id()));
    fs::create_dir(&staging).map_err(|e| format!("Cannot prepare workspace: {e}"))?;
    let result = (|| {
        let mut store = Store::empty();
        let mut mappings: Vec<(String, String)> = vec![];
        let mut legacy_lock = None;
        #[cfg(unix)]
        let mut session_locks = vec![];
        if let Some(source) = legacy.filter(|p| p.exists()) {
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            if root.starts_with(&source) || source.starts_with(&root) { return Err("Choose a workspace folder outside the old data folder.".into()); }
            let storage = Storage::new(source.join("workspace.sqlite3"));
            legacy_lock = Some(storage.acquire_lock()?);
            store = storage.load()?.unwrap_or_else(Store::empty);
            #[cfg(unix)] {
                let mut seen = std::collections::HashSet::new();
                for run in store.runs() {
                    if seen.insert((run.directory.clone(), run.session_id.clone())) {
                        session_locks.push(crate::opencode::lock_session(&crate::opencode::Request { agent: None, browser_executable: None,
                            directory: PathBuf::from(&run.directory), session_id: run.session_id.clone(),
                            executable: PathBuf::from(&store.opencode_path()), files: vec![],
                            model: run.model.clone(), prompt: String::new(), title: String::new(),
                        })?);
                    }
                }
            }
            copy_tree(&source, &staging)?;
            mappings.push((source.to_string_lossy().into_owned(), destination.to_string_lossy().into_owned()));
        }
        for project in &mut store.projects() {
            let relative = kind_relative_path(project.kind, project.id);
            let target = staging.join(&relative);
            if !project.directory.is_empty() {
                let source = PathBuf::from(&project.directory).canonicalize().map_err(|e| format!("Cannot read project {}: {e}", project.name))?;
                if root.starts_with(&source) || source.starts_with(&root) { return Err("Choose a workspace outside existing project folders.".into()); }
                copy_tree(&source, &target)?;
                mappings.push((project.directory.clone(), destination.join(&relative).to_string_lossy().into_owned()));
            } else { fs::create_dir_all(&target).map_err(|e| e.to_string())?; }
            project.directory = destination.join(relative).to_string_lossy().into_owned();
        }
        // Remap stored file locations, including run snapshots; session IDs remain unchanged.
        mappings.sort_by_key(|(old, _)| std::cmp::Reverse(old.len()));
        fn remap(value: &mut serde_json::Value, mappings: &[(String, String)]) {
            match value {
                serde_json::Value::String(s) => for (old, new) in mappings {
                    if s == old || s.starts_with(&format!("{old}/")) { *s = format!("{new}{}", &s[old.len()..]); break; }
                },
                serde_json::Value::Array(values) => for v in values { remap(v, mappings); },
                serde_json::Value::Object(values) => for v in values.values_mut() { remap(v, mappings); },
                _ => {}
            }
        }
        let mut value = serde_json::to_value(&store).map_err(|e| e.to_string())?;
        remap(&mut value, &mappings);
        // Signatures contain serialized snapshots: remap only their known directory slot,
        // without refreshing stale inputs or rewriting historical prompt/output text.
        for run in value["runs"].as_array_mut().into_iter().flatten() {
            let Some(text) = run.get("input_signature").and_then(serde_json::Value::as_str) else { continue; };
            let Ok(mut signature) = serde_json::from_str::<serde_json::Value>(text) else { continue; };
            let index = if run.get("project_plan").is_some_and(|v| !v.is_null()) {
                match signature.as_array().map(Vec::len) { Some(7) => Some(5), Some(5) => Some(3), _ => None }
            } else if run.get("research_kind").is_some_and(|v| !v.is_null()) && signature.as_array().is_some_and(|v| v.len() == 5) {
                Some(2)
            } else { None };
            if let Some(index) = index.filter(|index| signature[*index].is_string()) {
                let before = signature[index].clone();
                remap(&mut signature[index], &mappings);
                if signature[index] != before {
                    let updated = crate::sqlite_storage::replace_signature_path(text, index, &signature[index])?;
                    run["input_signature"] = serde_json::Value::String(updated);
                }
            }
        }
        store = serde_json::from_value(value).map_err(|e| e.to_string())?;
        store.recover_interrupted_runs();
        fn remap_markers(directory: &Path, staging: &Path, destination: &Path, mappings: &[(String, String)]) -> Result<(), String> {
            for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() { remap_markers(&entry.path(), staging, destination, mappings)?; }
                else if entry.file_name() == ".workspace-migrated" {
                    let mut value = serde_json::Value::String(fs::read_to_string(entry.path()).map_err(|e| e.to_string())?);
                    remap(&mut value, mappings);
                    let source = Path::new(value.as_str().unwrap());
                    let final_directory = destination.join(directory.strip_prefix(staging).map_err(|e| e.to_string())?);
                    let source = if source.is_absolute() && source.starts_with(destination) {
                        crate::scheduler::relative_migration_source(source, &final_directory)
                    } else { source.to_path_buf() };
                    fs::write(entry.path(), source.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        remap_markers(&staging, &staging, &destination, &mappings)?;
        let storage = Storage::new(staging.join("workspace.sqlite3")).with_workspace_root(destination.clone());
        storage.save(&store)?;
        if storage.load()?.as_ref() != Some(&store) {
            return Err("Workspace verification failed; original files are unchanged.".into());
        }
        fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
        drop(legacy_lock);
        Ok(destination.clone())
    })();
    if result.is_err() { let _ = fs::remove_dir_all(staging); }
    result
}

pub fn legacy_directory() -> Result<Option<PathBuf>, String> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?);
    for name in ["What the hell are you planning", "Task Manager"] {
        let dir = home.join("Library/Application Support").join(name);
        if dir.join("workspace.sqlite3").exists() || dir.join("workspace.json").exists() { return Ok(Some(dir)); }
    }
    Ok(None)
}

pub fn save_location(root: &Path) -> Result<(), String> {
    let path = config_path()?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(&root).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wthayp-workspace-{name}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&p).unwrap(); p
    }
    #[test]
    fn new_workspace_and_project_folders_are_contained() {
        let root = temp("new");
        let data = initialize(&root, None).unwrap();
        let store = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert!(Path::new(&store.project_at_mut(0).unwrap().directory).is_dir());
        assert!(Path::new(&project_directory(&data, 42).unwrap()).starts_with(&data));
        assert!(check_directory(&data, &root.join("outside")).is_err());
        assert!(check_directory(&data, &data.join("../outside")).is_err());
        assert_eq!(initialize(&root, None).unwrap(), data);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn migration_copies_projects_and_materials_without_changing_original() {
        let root = temp("import"); let old = temp("old"); let project = temp("project");
        fs::write(project.join("report.md"), "original report").unwrap();
        fs::create_dir(old.join("attachments")).unwrap();
        fs::write(old.join("attachments/material.pdf"), "material").unwrap();
        let mut store = Store::empty(); store.project_at_mut(0).unwrap().directory = project.to_string_lossy().into_owned();
        let storage = Storage::new(old.join("workspace.sqlite3")); storage.save(&store).unwrap();
        let lock = storage.acquire_lock().unwrap();
        assert!(initialize(&root, Some(&old)).is_err());
        assert!(!root.join("workspace").exists()); drop(lock);
        let data = initialize(&root, Some(&old)).unwrap();
        let migrated = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert_eq!(fs::read_to_string(Path::new(&migrated.project_at_mut(0).unwrap().directory).join("report.md")).unwrap(), "original report");
        assert_eq!(storage.load().unwrap().unwrap(), store);
        assert!(data.join("attachments/material.pdf").is_file());
        for p in [root, old, project] { fs::remove_dir_all(p).unwrap(); }
    }
    #[test]
    fn legacy_import_relocates_signature_paths_and_markers_without_refreshing_inputs() {
        let root = temp("signature-import"); let old = temp("signature-old"); let project = temp("signature-project");
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = project.to_string_lossy().into_owned();
        store.set_default_model("fixture/model".into());
        let mut template = store.prepare_project_plan(1).unwrap();
        template.status = "Succeeded".into();
        template.prompt = format!("Historical path: {}", project.display());
        template.output = template.prompt.clone();
        let signatures = [
            serde_json::json!([1, "old project name", "old brief", project, []]),
            serde_json::json!([1, 1, "old request", "date", "old name", project, null]),
            serde_json::json!([[], [9], project, "analyze", 1]),
        ];
        for (index, signature) in signatures.iter().enumerate() {
            let mut run = template.clone();
            run.id = index as u64 + 1;
            run.input_signature = signature.to_string();
            if index == 2 { run.project_plan = None; run.research_kind = Some(crate::research::ResearchKind::Analyze); }
            store.push_run(run);
        }
        let marker = project.join(".research/goal-1/.workspace-migrated");
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(&marker, project.to_string_lossy().as_bytes()).unwrap();
        let storage = Storage::new(old.join("workspace.sqlite3"));
        storage.save(&store).unwrap();
        let original = fs::read(storage.path()).unwrap();
        let data = initialize(&root, Some(&old)).unwrap();
        let restored = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        for (index, signature) in signatures.into_iter().enumerate() {
            let mut expected = signature;
            expected[[3, 5, 2][index]] = serde_json::Value::String(restored.project_at_mut(0).unwrap().directory.clone());
            assert_eq!(restored.runs()[index].input_signature, expected.to_string());
            assert_eq!(restored.runs()[index].prompt, template.prompt);
            assert_eq!(restored.runs()[index].output, template.output);
        }
        assert_eq!(fs::read(storage.path()).unwrap(), original);
        assert_eq!(fs::read_to_string(marker).unwrap(), project.to_string_lossy());
        let execution = Path::new(&restored.project_at_mut(0).unwrap().directory).join(".research/goal-1");
        assert_eq!(fs::read_to_string(execution.join(".workspace-migrated")).unwrap(), "../..");
        assert_eq!(crate::scheduler::migrated_workspace_source(&execution).unwrap(), restored.project_at_mut(0).unwrap().directory);
        let moved = root.with_extension("moved");
        fs::rename(&root, &moved).unwrap();
        let project_after_move = moved.join("workspace/projects/project-1");
        assert_eq!(crate::scheduler::migrated_workspace_source(&project_after_move.join(".research/goal-1")).unwrap(), project_after_move.canonicalize().unwrap().to_string_lossy());
        for p in [moved, old, project] { fs::remove_dir_all(p).unwrap(); }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_directory_symlink_escape() {
        let root = temp("symlink"); let outside = temp("outside");
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();
        assert!(check_directory(&root, &root.join("linked/new-goal")).is_err());
        fs::remove_dir_all(root).unwrap(); fs::remove_dir_all(outside).unwrap();
    }

    fn project(id: u64, name: &str, directory: impl Into<String>, kind: ProjectKind) -> Project {
        Project {
            id,
            name: name.into(),
            key: format!("P{id}"),
            directory: directory.into(),
            research_brief: String::new(),
            kind,
        }
    }

    /// Explicit reconnection of a workspace holding an ordinary project plus the
    /// generated automation container must accept both kind-specific roots.
    #[test]
    fn mixed_reconnect_accepts_ordinary_and_automation_projects() {
        let root = temp("mixed-reconnect");
        let data = initialize(&root, None).unwrap();
        let user_dir = project_directory(&data, 1).unwrap();
        fs::write(Path::new(&user_dir).join("goal.md"), "user data").unwrap();
        let automation_dir = automation_directory(&data).unwrap();
        fs::write(Path::new(&automation_dir).join("quick.md"), "automation data").unwrap();
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = user_dir.clone();
        let automation_project = store.ensure_automation_project(automation_dir.clone()).unwrap();
        let schedule = crate::automation_schedule::Schedule {
            spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 10 },
            timezone: "Asia/Shanghai".into(),
        };
        store
            .create_automation_draft(
                automation_project,
                crate::automation::AutomationShape::Quick,
                "Quick index check".into(),
                "Check the closing index level.".into(),
                "One number with its as-of time and 2-5 sources.".into(),
                schedule.clone(),
                Default::default(),
                Some(crate::automation_execution::build_quick_template(
                    "Check the closing index level.",
                    "One number with its as-of time and 2-5 sources.",
                ).unwrap()),
                1_700_000_000,
            )
            .unwrap();
        store
            .create_automation_draft(
                automation_project,
                crate::automation::AutomationShape::Research,
                "Research supply chain".into(),
                "Track supply-chain changes.".into(),
                "A short sourced report.".into(),
                schedule,
                Default::default(),
                Some(crate::automation_execution::build_template(
                    "Track supply-chain changes.",
                    "A short sourced report.",
                ).unwrap()),
                1_700_000_000,
            )
            .unwrap();
        Storage::new(data.join("workspace.sqlite3")).save(&store).unwrap();

        // Set workspace again after the bootstrap location was removed.
        assert_eq!(initialize(&root, None).unwrap(), data);
        let loaded = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert_eq!(loaded.project_at_mut(0).unwrap().directory, user_dir);
        assert_eq!(loaded.automation_project().unwrap().directory, automation_dir);
        assert_eq!(loaded.automations().len(), 2);
        assert!(loaded.automations().iter().any(|automation| automation.shape == crate::automation::AutomationShape::Quick && automation.name == "Quick index check"));
        assert!(loaded.automations().iter().any(|automation| automation.shape == crate::automation::AutomationShape::Research && automation.name == "Research supply chain"));
        assert_eq!(fs::read_to_string(Path::new(&user_dir).join("goal.md")).unwrap(), "user data");
        assert_eq!(fs::read_to_string(Path::new(&automation_dir).join("quick.md")).unwrap(), "automation data");
        fs::remove_dir_all(root).unwrap();
    }

    /// An ordinary-only workspace still reconnects without the automation root.
    #[test]
    fn ordinary_only_reconnect_still_passes() {
        let root = temp("ordinary-reconnect");
        let data = initialize(&root, None).unwrap();
        let user_dir = project_directory(&data, 1).unwrap();
        fs::write(Path::new(&user_dir).join("goal.md"), "user data").unwrap();
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = user_dir.clone();
        Storage::new(data.join("workspace.sqlite3")).save(&store).unwrap();

        assert_eq!(initialize(&root, None).unwrap(), data);
        let loaded = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert!(loaded.automation_project().is_none());
        assert_eq!(loaded.project_at_mut(0).unwrap().directory, user_dir);
        fs::remove_dir_all(root).unwrap();
    }

    /// Moving the entire portable workspace keeps both kinds reconnectable because
    /// stored paths are relative to the workspace root.
    #[test]
    fn relocated_workspace_reconnects_with_automations() {
        let root = temp("relocate-source").canonicalize().unwrap();
        let moved = root.with_extension("moved");
        let data = initialize(&root, None).unwrap();
        let user_dir = project_directory(&data, 1).unwrap();
        fs::write(Path::new(&user_dir).join("goal.md"), "user data").unwrap();
        let automation_dir = automation_directory(&data).unwrap();
        fs::write(Path::new(&automation_dir).join("quick.md"), "automation data").unwrap();
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = user_dir;
        store.ensure_automation_project(automation_dir).unwrap();
        Storage::new(data.join("workspace.sqlite3")).save(&store).unwrap();

        fs::rename(&root, &moved).unwrap();
        let moved = moved.canonicalize().unwrap();
        let reopened = initialize(&moved, None).unwrap();
        assert_eq!(reopened, moved.join("workspace"));
        let loaded = Storage::new(reopened.join("workspace.sqlite3")).load().unwrap().unwrap();
        let moved_user = moved.join("workspace/projects/project-1");
        let moved_automation = moved.join("workspace/automations");
        assert_eq!(PathBuf::from(&loaded.project_at_mut(0).unwrap().directory), moved_user);
        assert_eq!(PathBuf::from(&loaded.automation_project().unwrap().directory), moved_automation);
        assert_eq!(fs::read_to_string(moved_user.join("goal.md")).unwrap(), "user data");
        assert_eq!(fs::read_to_string(moved_automation.join("quick.md")).unwrap(), "automation data");
        fs::remove_dir_all(moved).unwrap();
    }

    /// Genuinely invalid data names the project and required root, and the policy
    /// rejects parent traversal, wrong-kind roots and symlink escapes.
    #[test]
    fn directory_policy_rejects_external_traversal_wrong_kind_and_symlinks() {
        let root = temp("policy-reject").canonicalize().unwrap();
        let data = initialize(&root, None).unwrap();
        let user_dir = project_directory(&data, 1).unwrap();
        let automation_dir = automation_directory(&data).unwrap();
        let outside = temp("policy-outside").canonicalize().unwrap();

        let external = project(7, "External", outside.to_string_lossy(), ProjectKind::User);
        let error = check_project_directory(&data, &external).unwrap_err();
        assert!(error.contains("External"), "{error}");
        assert!(error.contains("projects"), "{error}");

        // Parent traversal that leaves the workspace.
        let traversal = project(8, "Traversal", data.join("projects/../escape").to_string_lossy(), ProjectKind::User);
        assert!(check_project_directory(&data, &traversal).is_err());

        // Wrong kind: an ordinary project may not live in the automation root.
        let wrong = project(9, "Wrong kind", automation_dir.clone(), ProjectKind::User);
        assert!(check_project_directory(&data, &wrong).is_err());

        // An automation project is confined to the automation root.
        let escaping = project(10, "Escaping automation", user_dir.clone(), ProjectKind::Automation);
        assert!(check_project_directory(&data, &escaping).is_err());

        #[cfg(unix)]
        {
            let link = data.join("projects/project-escape");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            let linked = project(11, "Symlink escape", link.to_string_lossy(), ProjectKind::User);
            assert!(check_project_directory(&data, &linked).is_err());
        }

        assert!(check_project_directory(&data, &project(1, "User", user_dir, ProjectKind::User)).is_ok());
        assert!(check_project_directory(&data, &project(2, "Automation", automation_dir, ProjectKind::Automation)).is_ok());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    /// A kind root that is a symlink to an external folder must not validate that
    /// folder's children as workspace runtime directories, even when the
    /// descendant does not exist locally.
    #[cfg(unix)]
    #[test]
    fn runtime_directory_rejects_symlinked_kind_roots() {
        for kind in [ProjectKind::User, ProjectKind::Automation] {
            let workspace = temp("root-symlink-workspace");
            let outside = temp("root-symlink-outside").canonicalize().unwrap();
            let kind_root = allowed_root(&workspace, kind);
            std::os::unix::fs::symlink(&outside, &kind_root).unwrap();
            let existing = outside.join("goal");
            fs::create_dir_all(&existing).unwrap();

            assert!(check_project_roots(&workspace, &existing).is_err());
            assert!(check_project_roots(&workspace, &kind_root).is_err());
            assert!(check_project_roots(&workspace, &kind_root.join("missing-goal")).is_err());

            fs::remove_dir_all(workspace).unwrap();
            fs::remove_dir_all(outside).unwrap();
        }
    }

    /// Valid runtime folders under either real kind root still pass.
    #[test]
    fn runtime_directory_accepts_valid_kind_roots() {
        let root = temp("runtime-valid");
        let data = initialize(&root, None).unwrap();
        let user_dir = project_directory(&data, 1).unwrap();
        let automation_dir = automation_directory(&data).unwrap();
        assert!(check_project_roots(&data, Path::new(&user_dir)).is_ok());
        assert!(check_project_roots(&data, &Path::new(&user_dir).join("goal")).is_ok());
        assert!(check_project_roots(&data, Path::new(&automation_dir)).is_ok());
        assert!(check_project_roots(&data, &Path::new(&automation_dir).join("period-1")).is_ok());
        assert!(check_project_roots(&data, &data.join("outside")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Legacy import relocates each project by kind and leaves the source untouched.
    #[test]
    fn migration_places_ordinary_and_automation_projects_by_kind() {
        let root = temp("migration-mixed");
        let old = temp("migration-old");
        let user_source = temp("migration-user");
        let automation_source = temp("migration-automation");
        fs::write(user_source.join("goal.md"), "user report").unwrap();
        fs::write(automation_source.join("quick.md"), "automation report").unwrap();
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = user_source.to_string_lossy().into_owned();
        store.ensure_automation_project(automation_source.to_string_lossy().into_owned()).unwrap();
        let legacy_storage = Storage::new(old.join("workspace.sqlite3"));
        legacy_storage.save(&store).unwrap();
        let original = fs::read(legacy_storage.path()).unwrap();

        let data = initialize(&root, Some(&old)).unwrap();
        let migrated = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert_eq!(PathBuf::from(&migrated.project_at_mut(0).unwrap().directory), data.join("projects/project-1"));
        let automation = migrated.automation_project().unwrap();
        assert_eq!(PathBuf::from(&automation.directory), data.join("automations"));
        assert_eq!(fs::read_to_string(Path::new(&migrated.project_at_mut(0).unwrap().directory).join("goal.md")).unwrap(), "user report");
        assert_eq!(fs::read_to_string(Path::new(&automation.directory).join("quick.md")).unwrap(), "automation report");
        // The original legacy workspace and its project folders are untouched.
        assert_eq!(fs::read(legacy_storage.path()).unwrap(), original);
        assert_eq!(fs::read_to_string(user_source.join("goal.md")).unwrap(), "user report");
        assert_eq!(fs::read_to_string(automation_source.join("quick.md")).unwrap(), "automation report");
        // The migrated workspace is reconnectable.
        assert_eq!(initialize(&root, None).unwrap(), data);
        for p in [root, old, user_source, automation_source] { fs::remove_dir_all(p).unwrap(); }
    }
}
