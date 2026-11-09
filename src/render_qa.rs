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
pub(crate) fn load_workspace(path: &Path) -> Result<Store, String> {
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

/// Timed native wheel input on the same isolated snapshot used by the frame audit.
/// Semantic notifications deliberately exercise background-update costs.
pub fn scroll_native() {
    use crate::{app::Layout, appearance::Appearance};
    use std::time::Duration;
    let store = load_workspace(Path::new(&std::env::var("PLANNING_PERF_WORKSPACE").unwrap())).unwrap();
    let result = store.runs().iter().rev().find(|r| r.project_plan.is_none()
        && r.status == "Succeeded" && !r.output.is_empty()
        && !store.run_is_automation_owned(r)).map(|r| r.id);
    let dark = std::env::var("PLANNING_SCROLL_DARK").as_deref() == Ok("1");
    let mode = if dark { Appearance::Dark } else { Appearance::Light };
    application().with_assets(crate::ui_assets::AppAssets).run(move |cx| {
        init(cx);
        let (handle, app) = open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1000.), px(680.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Application scroll QA — isolated snapshot".into()), ..Default::default() }),
            ..Default::default()
        }, cx, |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, cx| {
                a.store = store;
                a.expanded_sidebar_projects = a.store.projects().iter().map(|p| p.id).collect();
                a.set_appearance(mode, w, cx);
            });
            app
        }).unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            for (name, scope, target, horizontal) in [
                ("Sidebar", Scope::All, "sidebar-scroll", false),
                ("Research list", Scope::All, "collection-table-scroll", false),
                ("Research board", Scope::All, "board-scroll", true),
                ("Model settings", Scope::Settings, "model-settings-scroll", false),
                ("Automation results", Scope::AutomationRuns, "automation-results-scroll", false),
                ("Research report", Scope::Runs, "research-result-scroll", false),
            ] {
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| a.navigate(scope, w, cx));
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(400)).await;
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.layout = if horizontal { Layout::Board } else { Layout::List };
                        if matches!(name, "Sidebar" | "Research list" | "Research board") {
                            a.set_scope_identity(Scope::All);
                        }
                        if name == "Model settings" { a.set_settings_tab(crate::settings_views::SettingsTab::Models); }
                        if name == "Research report" {
                            assert!(result.is_some(), "Snapshot must contain a readable ordinary result");
                            a.note_selected_run(result);
                            a.show_run_result = true;
                            a.show_run_details = false;
                            a.reveal_selected_run();
                        }
                        cx.notify();
                    });
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(400)).await;
                for updates in [false, true] {
                    let mut intervals = Vec::new();
                    let mut work = Vec::new();
                    let mut previous = Instant::now();
                    for frame in 0..120 {
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                        let started = Instant::now();
                        intervals.push(started.duration_since(previous).as_secs_f64() * 1000.);
                        previous = started;
                        cx.update_window(handle, |_, w, cx| {
                            if updates && frame % 5 == 0 { app.update(cx, |_, cx| cx.notify()); }
                            let position = w.find(target).bounds().center();
                            w.dispatch_event(MouseMoveEvent { position, ..Default::default() }.to_platform_input(), cx);
                            let amount = px(if frame < 60 { -12. } else { 12. });
                            let delta = if horizontal { point(amount, px(0.)) } else { point(px(0.), amount) };
                            w.dispatch_event(ScrollEvent { position, delta: ScrollDelta::Pixels(delta), ..Default::default() }.to_platform_input(), cx);
                            // Native input schedules its own display callback. A forced
                            // draw here would render twice and distort scroll latency.
                            if frame == 59 {
                                let a = app.read(cx);
                                match name {
                                    "Research list" => assert!(a.navigation.task_list_scroll.0.borrow().base_handle.offset().y < px(-100.)),
                                    "Research board" => assert!(a.navigation.board_scroll.offset().x < px(-100.)),
                                    "Automation results" => assert!(a.automation_ui.result_list.scroll_px_offset_for_scrollbar().y.abs() > px(100.)),
                                    "Sidebar" => assert!(a.sidebar_scroll.offset().y < px(-100.)),
                                    "Model settings" => assert!(a.settings_scroll.offset().y < px(-100.)),
                                    "Research report" => assert!(a.result_scroll.offset().y < px(-100.)),
                                    _ => {}
                                }
                            }
                        }).unwrap();
                        work.push(started.elapsed().as_secs_f64() * 1000.);
                    }
                    intervals.sort_by(f64::total_cmp);
                    work.sort_by(f64::total_cmp);
                    println!("{mode:?} {name} background_updates={updates}: work p95={:.2} max={:.2} ms; input interval p95={:.2} max={:.2} ms; intervals>50ms={}", work[114], work[119], intervals[114], intervals[119], intervals.iter().filter(|ms| **ms > 50.).count());
                }
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
}
