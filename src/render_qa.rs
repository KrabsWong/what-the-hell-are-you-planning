//! Read-only frame benchmark against a caller-supplied, closed fixture.
use crate::{
    app::{Scope, TaskApp},
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Read `PRAGMA user_version` through an immutable URI: no locks, no WAL
/// sidecars, and no writes to the caller's fixture.
fn schema_version(path: &Path) -> Result<u32, String> {
    let encoded = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace(' ', "%20");
    let connection = rusqlite::Connection::open_with_flags(
        format!("file:{encoded}?immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("Cannot read {} schema version: {error}", path.display()))?;
    u32::try_from(version).map_err(|_| format!("Invalid SQLite schema version {version}"))
}

/// Load a caller-supplied workspace without writing to it. The caller must
/// pass a closed, frozen, isolated workspace copy, never a live workspace:
/// an uncheckpointed WAL or hot journal is rejected instead of guessed at.
/// Version 4 is only readable through the incremental backend, which enables
/// WAL, so a plain file copy is opened through it and the caller's fixture
/// itself is only read.
fn load_workspace(path: &Path) -> Result<Store, String> {
    let storage = Storage::new(path.to_path_buf());
    if path.extension().is_some_and(|extension| extension == "sqlite3") && path.is_file() {
        for suffix in ["-wal", "-journal"] {
            let sidecar = sidecar(path, suffix);
            if sidecar.is_file() {
                return Err(format!(
                    "{} must be a closed, checkpointed workspace copy; found {}. Checkpoint and close the workspace before benchmarking.",
                    path.display(),
                    sidecar.display()
                ));
            }
        }
        if schema_version(path)? == 4 {
            let private = std::env::temp_dir().join(format!(
                "planning-render-perf-{}-{}",
                std::process::id(),
                crate::opencode::new_session_id()
            ));
            let result = (|| {
                std::fs::create_dir_all(&private).map_err(|error| {
                    format!("Cannot create private workspace snapshot directory: {error}")
                })?;
                let snapshot = private.join("workspace.sqlite3");
                std::fs::copy(path, &snapshot)
                    .map_err(|error| format!("Cannot copy {}: {error}", path.display()))?;
                let incarnation = crate::sqlite_v4::read_incarnation(&snapshot)?;
                crate::sqlite_v4::SqliteV4::open(&snapshot, storage.workspace_root(), incarnation)?
                    .load_store()
            })();
            let _ = std::fs::remove_dir_all(&private);
            return result;
        }
    }
    storage
        .load()?
        .ok_or_else(|| format!("No workspace data in {}", path.display()))
}

pub fn run() {
    let path = PathBuf::from(
        std::env::var("PLANNING_PERF_WORKSPACE").expect("PLANNING_PERF_WORKSPACE"),
    );
    let store = load_workspace(&path).unwrap();
    let start = Instant::now();
    for _ in 0..10 {
        for task in store.tasks() {
            std::hint::black_box(store.step_state(task.id));
        }
    }
    println!(
        "all step states: {:.2} ms/pass",
        start.elapsed().as_secs_f64() * 100.
    );
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| init(cx));
    let mut entity = None;
    let window = cx
        .open_window(size(px(1440.), px(900.)), |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, _| {
                a.store = store.clone();
                a.expanded_sidebar_projects = store.projects().iter().map(|p| p.id).collect();
            });
            entity = Some(app.clone());
            cx.new(|cx| base::Root::new(app, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let handle: AnyWindowHandle = window.into();
    let mut panels = vec![("All research", Scope::All), ("Recovery", Scope::Recovery)];
    if let Some(project) = store.projects().last() {
        panels.push(("Project", Scope::Project(project.id)));
    }
    if let Some(topic) = store.topics().last() {
        panels.push(("Topic", Scope::Topic(topic.id)));
    }
    let result = store
        .runs()
        .iter()
        .rev()
        .find(|r| r.project_plan.is_none() && r.status == "Succeeded" && !r.output.is_empty());
    let goal = result.and_then(|r| store.owning_research_goal(r.task_id));
    if goal.is_some() {
        panels.push(("Steps", Scope::All));
    }
    if result.is_some() {
        panels.push(("Report", Scope::Runs));
    }
    for (name, scope) in panels {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.navigate(scope, w, cx);
                if name == "Steps" {
                    a.open_goal_steps(goal.unwrap(), w, cx);
                }
                if name == "Report" {
                    a.note_selected_run(result.map(|r| r.id));
                    a.show_run_result = true;
                    a.show_run_details = false;
                    a.reveal_selected_run();
                }
            });
            w.render_frame(cx);
        })
        .unwrap();
        let start = Instant::now();
        for _ in 0..20 {
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |_, cx| cx.notify());
                w.render_frame(cx);
            })
            .unwrap();
        }
        println!(
            "{name}: {:.2} ms/frame",
            start.elapsed().as_secs_f64() * 50.
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{load_workspace, sidecar};
    use crate::domain::Store;
    use std::path::{Path, PathBuf};

    fn caller_entries(root: &Path) -> Vec<String> {
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        entries
    }

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "planning-render-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn v4_workspace_loads_without_writing_the_caller_fixture() {
        let root = temp_root("v4");
        let path = root.join("workspace.sqlite3");
        let mut expected = Store::empty();
        expected.set_default_model("fixture/render-model".into());
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, 0x52_31_37_35, &expected).unwrap();
        let before = std::fs::read(&path).unwrap();
        let entries = caller_entries(&root);
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.default_model(), "fixture/render-model");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "loading a v4 workspace must not write the caller fixture"
        );
        assert_eq!(
            caller_entries(&root),
            entries,
            "loading must not create sidecar files next to the caller fixture"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v4_workspace_with_uncheckpointed_sidecars_is_rejected() {
        let root = temp_root("sidecar");
        let path = root.join("workspace.sqlite3");
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, 0x52_31_37_36, &Store::empty())
            .unwrap();
        std::fs::write(sidecar(&path, "-wal"), b"uncheckpointed").unwrap();
        let error = load_workspace(&path).unwrap_err();
        assert!(
            error.contains("closed, checkpointed workspace copy"),
            "unexpected error: {error}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v3_sqlite_workspace_still_loads() {
        let root = temp_root("v3");
        let path = root.join("workspace.sqlite3");
        let mut expected = Store::empty();
        expected.set_default_model("fixture/legacy-model".into());
        crate::sqlite_storage::save(&path, &root, &expected).unwrap();
        let before = std::fs::read(&path).unwrap();
        let loaded = load_workspace(&path).unwrap();
        assert_eq!(loaded.default_model(), "fixture/legacy-model");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }
}
