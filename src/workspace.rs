//! The only bootstrap setting outside the user-selected workspace is its location.
use crate::{domain::Store, persistence::Storage};
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

pub fn project_directory(root: &Path, id: u64) -> Result<String, String> {
    let path = root.join("projects").join(format!("project-{id}"));
    check_directory(root, &path)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
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
        for project in &store.projects {
            check_directory(&destination.join("projects"), Path::new(&project.directory))?;
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
                for run in &store.runs {
                    if seen.insert((run.directory.clone(), run.session_id.clone())) {
                        session_locks.push(crate::opencode::lock_session(&crate::opencode::Request {
                            directory: PathBuf::from(&run.directory), session_id: run.session_id.clone(),
                            executable: PathBuf::from(&store.opencode_path), files: vec![],
                            model: run.model.clone(), prompt: String::new(), title: String::new(),
                        })?);
                    }
                }
            }
            copy_tree(&source, &staging)?;
            mappings.push((source.to_string_lossy().into_owned(), destination.to_string_lossy().into_owned()));
        }
        for project in &mut store.projects {
            let relative = PathBuf::from("projects").join(format!("project-{}", project.id));
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
        store = serde_json::from_value(value).map_err(|e| e.to_string())?;
        store.recover_interrupted_runs();
        fn remap_markers(directory: &Path, mappings: &[(String, String)]) -> Result<(), String> {
            for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() { remap_markers(&entry.path(), mappings)?; }
                else if entry.file_name() == ".workspace-migrated" {
                    let mut value = serde_json::Value::String(fs::read_to_string(entry.path()).map_err(|e| e.to_string())?);
                    remap(&mut value, mappings);
                    fs::write(entry.path(), value.as_str().unwrap()).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        remap_markers(&staging, &mappings)?;
        let storage = Storage::new(staging.join("workspace.sqlite3"));
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
        assert!(Path::new(&store.projects[0].directory).is_dir());
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
        let mut store = Store::empty(); store.projects[0].directory = project.to_string_lossy().into_owned();
        let storage = Storage::new(old.join("workspace.sqlite3")); storage.save(&store).unwrap();
        let lock = storage.acquire_lock().unwrap();
        assert!(initialize(&root, Some(&old)).is_err());
        assert!(!root.join("workspace").exists()); drop(lock);
        let data = initialize(&root, Some(&old)).unwrap();
        let migrated = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
        assert_eq!(fs::read_to_string(Path::new(&migrated.projects[0].directory).join("report.md")).unwrap(), "original report");
        assert_eq!(storage.load().unwrap().unwrap(), store);
        assert!(data.join("attachments/material.pdf").is_file());
        for p in [root, old, project] { fs::remove_dir_all(p).unwrap(); }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_directory_symlink_escape() {
        let root = temp("symlink"); let outside = temp("outside");
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();
        assert!(check_directory(&root, &root.join("linked/new-goal")).is_err());
        fs::remove_dir_all(root).unwrap(); fs::remove_dir_all(outside).unwrap();
    }
}
