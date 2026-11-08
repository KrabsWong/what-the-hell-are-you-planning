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
    reconnect_policy();
    println!("Workspace onboarding screenshots: {}", root.display());
}

/// Store-level regression for reconnecting and relocating a workspace holding an
/// ordinary project plus real Quick and Research automations with a saved period
/// and report. It calls `workspace::initialize` directly (the same function the
/// Set workspace action uses); it does not click a UI button. Uses only disposable
/// folders and never reads or writes the real bootstrap location.
fn reconnect_policy() {
    use crate::automation::{AutomationPolicy, AutomationShape, OccurrenceStatus};
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{AgentRun, ProjectKind, Stage, Store};
    use crate::persistence::Storage;
    use crate::research::ResearchKind;
    use crate::workspace;
    let base = std::env::temp_dir().join(format!("workspace-reconnect-qa-{}", crate::opencode::new_session_id()));
    let root = base.join("root");
    let moved = base.join("moved");
    std::fs::create_dir_all(&root).unwrap();
    let now = 1_700_000_000;

    let data = workspace::initialize(&root, None).unwrap();
    let user_dir = workspace::project_directory(&data, 1).unwrap();
    std::fs::write(std::path::Path::new(&user_dir).join("goal.md"), "user data").unwrap();
    let automation_dir = workspace::automation_directory(&data).unwrap();

    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = user_dir.clone();
    store.set_default_model("qa/automation".into());
    let automation_project = store.ensure_automation_project(automation_dir.clone()).unwrap();
    let schedule = Schedule {
        spec: ScheduleSpec::Interval { minutes: 10 },
        timezone: "Asia/Shanghai".into(),
    };
    let quick = store
        .create_automation_draft(
            automation_project,
            AutomationShape::Quick,
            "Quick index check".into(),
            "Check the closing index level.".into(),
            "One number with its as-of time and 2-5 sources.".into(),
            schedule.clone(),
            AutomationPolicy::default(),
            Some(crate::automation_execution::build_quick_template(
                "Check the closing index level.",
                "One number with its as-of time and 2-5 sources.",
            ).unwrap()),
            now,
        )
        .unwrap();
    let research = store
        .create_automation_draft(
            automation_project,
            AutomationShape::Research,
            "Research supply chain".into(),
            "Track supply-chain changes.".into(),
            "A short sourced report.".into(),
            schedule,
            AutomationPolicy::default(),
            Some(crate::automation_execution::build_template(
                "Track supply-chain changes.",
                "A short sourced report.",
            ).unwrap()),
            now,
        )
        .unwrap();
    store.approve_and_enable(quick, vec![], now).unwrap();
    store.approve_and_enable(research, vec![], now).unwrap();

    // Save one real period for each automation.
    for id in [quick, research] {
        let mut definition = store.automation_mut(id).unwrap();
        definition.anchor = Some(now - 600);
        definition.cursor = Some(now);
    }
    let plans = store.plan_automation_ticks(now);
    assert_eq!(plans.len(), 2, "both automations are due");
    for plan in &plans {
        store.commit_automation_tick(plan, now).unwrap();
    }

    // Attach a completed report run to the Research period.
    let occurrence = store
        .all_occurrences()
        .iter()
        .find(|occurrence| occurrence.automation_id == research)
        .cloned()
        .unwrap();
    let report = "# Supply chain report\n\nRepresentative saved report.";
    let run = AgentRun {
        automation_occurrence_id: Some(occurrence.id),
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
        id: store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1,
        task_id: occurrence.research_goal_id.unwrap_or(0),
        task_title: "Research supply chain".into(),
        directory: automation_dir.clone(),
        prompt: "QA saved report".into(),
        started_at: chrono::DateTime::from_timestamp(now, 0).unwrap().to_rfc3339(),
        queued_at: String::new(),
        workspace_source: None,
        ended_at: Some(chrono::DateTime::from_timestamp(now + 60, 0).unwrap().to_rfc3339()),
        session_id: Some("qa-reconnect-report".into()),
        exit_code: Some(0),
        agent: "OpenCode".into(),
        model: "qa/automation".into(),
        stage: Stage::Done,
        status: "Succeeded".into(),
        logs: vec!["saved report log".into()],
        output: report.into(),
        attachments: vec![],
        research_kind: Some(ResearchKind::Advise),
        sources: vec![],
        input_signature: String::new(),
    };
    let run_id = run.id;
    store.push_run(run);
    {
        let mut occurrence = store.occurrence_mut(occurrence.id).unwrap();
        occurrence.run_ids.push(run_id);
        occurrence.final_run_id = Some(run_id);
        occurrence.status = OccurrenceStatus::Succeeded;
        occurrence.unread = true;
    }

    Storage::new(data.join("workspace.sqlite3")).save(&store).unwrap();

    // Reconnection through the same root.
    assert_eq!(workspace::initialize(&root, None).unwrap(), data);
    let loaded = Storage::new(data.join("workspace.sqlite3")).load().unwrap().unwrap();
    assert_eq!(loaded.projects()[0].directory, user_dir);
    assert_eq!(loaded.automation_project().unwrap().directory, automation_dir);
    assert_eq!(loaded.automation_project().unwrap().kind, ProjectKind::Automation);
    assert_automations_survive(&loaded, report);

    // Relocating the entire portable workspace keeps projects, automations,
    // periods and the saved report loadable.
    std::fs::rename(&root, &moved).unwrap();
    let relocated = workspace::initialize(&moved, None).unwrap();
    let moved_user = relocated.join("projects/project-1");
    let moved_automation = relocated.join("automations");
    let loaded = Storage::new(relocated.join("workspace.sqlite3")).load().unwrap().unwrap();
    assert_eq!(std::path::PathBuf::from(&loaded.projects()[0].directory), moved_user);
    assert_eq!(std::path::PathBuf::from(&loaded.automation_project().unwrap().directory), moved_automation);
    assert_eq!(std::fs::read_to_string(moved_user.join("goal.md")).unwrap(), "user data");
    assert_automations_survive(&loaded, report);

    // A genuinely external project still fails and names the project and root.
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let mut bad = loaded.clone();
    bad.project_at_mut(0).unwrap().directory = outside.to_string_lossy().into_owned();
    Storage::new(relocated.join("workspace.sqlite3")).save(&bad).unwrap();
    let error = workspace::initialize(&moved, None).unwrap_err();
    assert!(error.contains(&bad.projects()[0].name) && error.contains("projects"), "{error}");

    std::fs::remove_dir_all(&base).unwrap();
    println!("Workspace reconnect/relocation with real Quick and Research automations passed");
}

/// Assert the saved shapes, periods and report text survived a reopen.
fn assert_automations_survive(store: &crate::domain::Store, report: &str) {
    use crate::automation::{AutomationShape, OccurrenceStatus};
    assert_eq!(store.automations().len(), 2);
    let quick = store.automations().iter().find(|automation| automation.shape == AutomationShape::Quick).unwrap();
    let research = store.automations().iter().find(|automation| automation.shape == AutomationShape::Research).unwrap();
    assert_eq!(quick.name, "Quick index check");
    assert_eq!(quick.objective, "Check the closing index level.");
    assert_eq!(research.name, "Research supply chain");
    assert_eq!(research.objective, "Track supply-chain changes.");
    assert!(quick.template.is_some() && research.template.is_some());
    assert_eq!(store.all_occurrences().len(), 2);
    let occurrence = store.all_occurrences().iter().find(|occurrence| occurrence.automation_id == research.id).unwrap();
    assert_eq!(occurrence.status, OccurrenceStatus::Succeeded);
    let saved = crate::automation_views::occurrence_report(store, occurrence).expect("saved report survives");
    assert_eq!(saved.output, report);
}

/// Exercise failure and retry with isolated data and deliberately unavailable dependencies.
fn upgrade_retry_preserves_lock_ownership() {
    use crate::{domain::Store, persistence::Storage};
    use std::time::{Duration, Instant};
    let root = std::env::temp_dir().join(format!("workspace-upgrade-retry-{}", crate::opencode::new_session_id()));
    std::fs::create_dir_all(root.join("projects/project-1")).unwrap();
    let path = root.join("workspace.sqlite3");
    let mut original = Store::empty();
    original.project_at_mut(0).unwrap().directory = root.join("projects/project-1").to_string_lossy().into();
    original.set_opencode_path(root.join("missing-opencode").to_string_lossy().into());
    original.research_config_mut().ego_path = root.join("missing-ego-browser").to_string_lossy().into();
    assert!(!std::path::Path::new(&original.opencode_path()).exists());
    assert!(!std::path::Path::new(&original.research_config().ego_path).exists());
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
