//! First-launch layout check; no user config or live model calls.
use crate::app::TaskApp;
use gpui_kit::{test::TestWindowExt, *};
use std::sync::Arc;
pub fn run() {
    let root = std::env::current_dir().unwrap().join(".local/workspace-onboarding-qa");
    std::fs::create_dir_all(&root).unwrap();
    for (width, height) in [(1000., 680.), (1440., 920.)] {
        let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
        cx.update(init);
        let mut app = None;
        let window = cx.open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            app = Some(view.clone());
            view.update(cx, |a, _| { a.workspace_required = true; });
            cx.new(|cx| base::Root::new(view, w, cx))
        }).unwrap();
        let handle = window.into();
        cx.update_window(handle, |_, w, cx| { w.render_frame(cx); }).unwrap();
        cx.update_window(handle, |_, w, cx| { w.render_frame(cx); }).unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w.try_find("startup-opencode").is_some());
            assert!(w.try_find("startup-codex").is_some());
            assert_eq!(w.find("startup-continue").disabled(), Some(true));
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(root.join(format!("setup-{width}.png"))).unwrap();
        let app = app.unwrap();
        for phase in ["Backing up existing data…", "Converting workspace paths…", "Verifying upgraded data…"] {
            cx.update_window(handle, |_, _w, cx| {
                app.update(cx, |a, cx| {
                    a.startup.workspace_phase = Some(phase);
                    cx.notify();
                    a.check_startup(cx);
                    assert!(a.startup.pending.is_none(), "dependency checks must wait for workspace preparation");
                });
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert_eq!(app.read(cx).startup.workspace_phase, Some(phase));
                assert!(w.try_find("startup-opencode").is_none());
            }).unwrap();
        }
        cx.capture_screenshot(handle).unwrap().save(root.join(format!("upgrade-{width}.png"))).unwrap();
        cx.update_window(handle, |_, _w, cx| {
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(crate::startup::WorkspaceOpenEvent::Complete(Err("Upgrade verification failed. Original data was preserved.".into()))).unwrap();
            app.update(cx, |a, cx| {
                a.startup.workspace_pending = Some(rx);
                a.workspace_root = Some(root.clone());
                cx.notify();
            });
        }).unwrap();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(app.read(cx).startup.workspace_phase.is_none());
            assert!(w.try_find("retry-workspace-upgrade").is_some());
            assert!(app.read(cx).startup.pending.is_none());
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(root.join(format!("upgrade-error-{width}.png"))).unwrap();
    }
    upgrade_retry_preserves_lock_ownership();
    println!("Workspace onboarding screenshots: {}", root.display());
}

/// Exercise failure and retry with isolated data and deliberately unavailable dependencies.
fn upgrade_retry_preserves_lock_ownership() {
    use crate::{domain::Store, persistence::Storage};
    use std::time::{Duration, Instant};
    let root = std::env::temp_dir().join(format!("workspace-upgrade-retry-{}", crate::opencode::new_session_id()));
    std::fs::create_dir_all(root.join("projects/project-1")).unwrap();
    let path = root.join("workspace.sqlite3");
    let mut original = Store::empty();
    original.projects[0].directory = root.join("projects/project-1").to_string_lossy().into();
    original.opencode_path = root.join("missing-opencode").to_string_lossy().into();
    original.research_config.ego_path = root.join("missing-ego-browser").to_string_lossy().into();
    assert!(!std::path::Path::new(&original.opencode_path).exists());
    assert!(!std::path::Path::new(&original.research_config.ego_path).exists());
    Storage::new(path.clone()).save(&original).unwrap();
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TRIGGER reject_projects BEFORE INSERT ON projects BEGIN SELECT RAISE(ABORT, 'fixture save failure'); END;").unwrap();
        let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
        cx.update(init);
        let mut app = None;
        let window = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        }).unwrap();
        let handle = window.into();
        let app = app.unwrap();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.workspace_required = true;
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert!(a.storage.is_none(), "failed attachment must not install storage");
                assert!(!a.storage_error.is_empty());
                a.check_startup(cx);
                assert!(a.startup.pending.is_none(), "failed attachment must block dependency checks");
            });
        }).unwrap();
        let probe = Storage::new(path.clone()).acquire_lock().expect("failed attachment must release its workspace lock");
        drop(probe);
        db.execute_batch("DROP TRIGGER reject_projects").unwrap();
        // Retry an actual v1 upgrade so the asynchronous completion also carries its backup.
        crate::sqlite_storage::save(&path, std::path::Path::new("/unused-fixture-root"), &original).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.begin_workspace_open(Storage::new(path.clone()), cx);
                assert!(a.startup.workspace_pending.is_some());
                a.check_startup(cx);
                assert!(a.startup.pending.is_none(), "dependencies must wait until upgrade completion is attached");
            });
        }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let finished = cx.update_window(handle, |_, w, cx| app.update(cx, |a, cx| {
                if a.startup.workspace_pending.is_some() {
                    assert!(a.startup.pending.is_none());
                    a.poll_workspace_open(w, cx);
                }
                a.startup.workspace_pending.is_none()
            })).unwrap();
            if finished { break; }
            assert!(Instant::now() < deadline, "asynchronous upgrade retry did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
        cx.update_window(handle, |_, _, cx| {
            let a = app.read(cx);
            assert!(a.storage.is_some(), "retry must install the successfully prepared workspace");
            assert!(a.storage_error.is_empty(), "retry must clear the previous attachment error");
            assert!(!a.workspace_required);
            assert!(a.startup.upgrade_backup.as_ref().is_some_and(|backup| backup.is_file()));
            assert_eq!(a.store, original);
        }).unwrap();
        assert!(Storage::new(path.clone()).acquire_lock().is_err(), "the UI must retain the worker's workspace lock after attachment");
    }
    std::fs::remove_dir_all(root).unwrap();
    println!("Workspace upgrade failure/retry, dependency gating and lock transfer passed");
}

/// Verify the deferred date features have no visible entry points.
pub fn calendar() {
    use crate::appearance::Appearance;
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
    cx.update(init);
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(720.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        for id in ["due", "overdue", "research", "calendar-prev", "calendar-today", "calendar-day-0"] {
            assert!(w.try_find(id).is_none(), "Hidden navigation still visible: {id}");
        }
        app.update(cx, |a, cx| {
            a.store.task_mut(1).unwrap().due_date = "2026-12-31".into();
            a.open_task(1, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("edit-date").is_none());
        app.update(cx, |a, cx| a.begin_draft(1, w, cx));
        w.render_frame(cx);
        assert!(w.try_find("draft-due").is_none());
        w.click("save-task", cx);
        assert_eq!(app.read(cx).store.task(1).unwrap().due_date, "2026-12-31");
    }).unwrap();
    std::fs::create_dir_all(".local/calendar-qa").unwrap();
    for appearance in [Appearance::Light, Appearance::Dark] {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.set_appearance(appearance, w, cx));
            w.render_frame(cx);
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(format!(".local/calendar-qa/{appearance:?}.png")).unwrap();
    }
    println!("Hidden date controls and preserved existing dates passed");
}
