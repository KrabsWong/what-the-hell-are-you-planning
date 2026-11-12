//! Focused offline acceptance for archived research history. No model calls.
use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::Appearance,
    controls::button,
    domain::{Stage, Store},
    persistence::Storage,
    views::{label, row},
};
use gpui_kit::{test::{TestQueryExt, TestWindowExt}, *};
use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze findings","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/archive-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    pure_checks();
    if std::env::var("ARCHIVE_QA_CASE").as_deref() == Ok("permanent-delete") {
        for (width, height) in [(1440., 920.), (1000., 680.)] {
            for mode in [Appearance::Light, Appearance::Dark] {
                let directory = root.join(format!("{mode:?}-{width}"));
                std::fs::create_dir_all(&directory).unwrap();
                list_and_board(&directory, mode, width, height);
            }
        }
        permanent_delete_confirmation(&root);
        goal_menu_labels(&root);
        topic_directory_visibility(&root);
        println!("Archive deletion QA passed · {}", root.display());
        return;
    }
    goal_menu_labels(&root);
    topic_directory_visibility(&root);
    render_matrix(&root);
    archived_in_progress_board(&root);
    ordinary_goal_steps_back(&root);
    ancestor_archive_material_check(&root);
    active_archive_confirmation(&root);
    permanent_delete_confirmation(&root);
    println!("Archive QA passed · {}", root.display());
}

fn archived_in_progress_board(root: &Path) {
    let directory = root.join("archived-in-progress-board");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/archive".into());
    let topic = store.create_topic(1, "Archived board topic").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "TASK-G1 Review research goal".into(),
            "Archived incomplete goal must remain visible on the board.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let _steps = store.apply_research_plan(goal, PLAN).unwrap();
    store.set_stage(goal, Stage::InProgress).unwrap();
    store.archive(goal).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::All);
        a.layout = Layout::Board;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("archive", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).visible_tasks().len(), 1);
        assert!(
            w.find(format!("card-{goal}")).visible(),
            "archived InProgress goal must keep its board card"
        );
        assert!(w.find(format!("restore-archived-{goal}")).visible());
        assert!(
            w.try_find("execution-status-filter").is_none(),
            "archive must not show execution filtering"
        );
        let status = w
            .find(format!("card-status-{goal}"))
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(status.contains("Archived"), "{status}");

        // Open steps from the Archive board must return to the same Archive
        // board, not the goal's ordinary Topic board.
        w.click(format!("goal-progress-{goal}"), cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected(), Some(goal));
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Archive);
        assert_eq!(app.read(cx).layout, Layout::Board);
        assert!(w.find(format!("card-{goal}")).visible());

        // The same entry with the keyboard shortcut.
        w.click(format!("goal-progress-{goal}"), cx);
        w.render_frame(cx);
        w.press("cmd-[", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Archive);
        assert_eq!(app.read(cx).layout, Layout::Board);
        assert!(w.find(format!("restore-archived-{goal}")).visible());
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        &directory,
        "archived-in-progress-board.png",
    );
}

fn ordinary_goal_steps_back(root: &Path) {
    let directory = root.join("ordinary-goal-steps-back");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/archive".into());
    let topic = store.create_topic(1, "Ordinary steps topic").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Ordinary Open steps goal".into(),
            "Board caller must survive Open steps Back.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let _steps = store.apply_research_plan(goal, PLAN).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    for scope in [Scope::All, Scope::Project(1)] {
        let (mut cx, handle, app) = with_app(store.clone(), &path, 1200., 800., |a, _, _| {
            a.set_scope_identity(scope.clone());
            a.layout = Layout::Board;
        });
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(app.read(cx).visible_tasks().len(), 1);
            let caller = app.read(cx).scope();
            w.click(format!("goal-progress-{goal}"), cx);
            w.render_frame(cx);
            assert_eq!(app.read(cx).selected(), Some(goal));
            w.click("navigation-back", cx);
            w.render_frame(cx);
            assert_eq!(app.read(cx).scope(), caller, "Open steps returns its board caller");
            assert_eq!(app.read(cx).layout, Layout::Board);
            assert!(w.find(format!("card-{goal}")).visible());
        })
        .unwrap();
    }

    // List caller: row -> goal detail -> Steps -> Back restores All/List and its
    // non-empty filter.
    {
        let (mut cx, handle, app) = with_app(store.clone(), &path, 1200., 800., |a, _, _| {
            a.set_scope_identity(Scope::All);
            a.layout = Layout::List;
            a.set_query("Ordinary".into());
        });
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            app.update(cx, |a, cx| a.request_task(goal, false, w, cx));
            w.render_frame(cx);
            assert_eq!(app.read(cx).selected(), Some(goal));
            w.click("navigation-back", cx);
            w.render_frame(cx);
            assert_eq!(app.read(cx).scope(), Scope::All);
            assert_eq!(app.read(cx).layout, Layout::List);
            assert_eq!(app.read(cx).query(), "Ordinary");
        })
        .unwrap();
    }

    // Topic caller: topic goal row -> Steps -> Back restores the Topic board.
    {
        let topic = store.task_topic(goal).unwrap().id;
        let (mut cx, handle, app) = with_app(store.clone(), &path, 1200., 800., |a, _, _| {
            a.set_scope_identity(Scope::Topic(topic));
            a.layout = Layout::Board;
        });
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            app.update(cx, |a, cx| a.request_task(goal, false, w, cx));
            w.render_frame(cx);
            assert_eq!(app.read(cx).selected(), Some(goal));
            w.click("navigation-back", cx);
            w.render_frame(cx);
            assert_eq!(app.read(cx).scope(), Scope::Topic(topic));
            assert_eq!(app.read(cx).layout, Layout::Board);
        })
        .unwrap();
    }
}

fn completed_goal(directory: &Path) -> (Store, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/archive".into());
    let topic = store.create_topic(1, "Archive QA topic").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Archived three-step goal".into(),
            "Completed history used to prove archived presentation.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    for task in &steps {
        let mut run = store.prepare_run(*task).unwrap();
        run.status = "Succeeded".into();
        run.output = format!("Retained archived report for {task}");
        run.ended_at = Some(crate::opencode::timestamp());
        run.exit_code = Some(0);
        store.push_run(run);
        store.set_stage(*task, Stage::Done).unwrap();
    }
    store.sync_goal_progress(goal);
    (store, goal, steps)
}

fn pure_checks() {
    let directory = std::env::temp_dir().join(format!("archive-qa-pure-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = completed_goal(&directory);
    assert!(store.goal_tasks_complete(goal));
    let report = store.research_result(steps[2]).unwrap().output.clone();
    store.archive(goal).unwrap();
    let status = store.task_status(goal, None);
    assert!(status.label().contains("Archived · 3/3 complete"), "{}", status.label());
    assert_eq!(status.phase, Stage::Done);
    let target = crate::archive_presentation::result_target(&store, goal, true);
    assert_eq!(target, Some(steps[2]));
    assert_eq!(
        crate::archive_presentation::retained_result(&store, steps[2])
            .map(|run| run.output.as_str()),
        Some(report.as_str())
    );
    assert!(!store.goal_tasks_complete(goal));
    store.restore(goal).unwrap();
    assert!(store.goal_tasks_complete(goal));
    let (mut inherited, inherited_goal, inherited_steps) = completed_goal(&directory);
    let topic = inherited.task_topic(inherited_goal).unwrap().id;
    inherited.set_topic_archived(topic, true).unwrap();
    assert!(!inherited.task(inherited_steps[0]).unwrap().archived);
    assert!(inherited.task_is_archived(inherited_goal));
    assert!(inherited.task_is_archived(inherited_steps[0]));
    inherited.set_topic_archived(topic, false).unwrap();
    let _ = std::fs::remove_dir_all(&directory);
}

fn render_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            list_and_board(&directory, mode, width, height);
            detail_and_restore(&directory, mode, width, height);
            ordinary_status_check(&directory, mode, width, height);
        }
    }
}

fn assert_status_fits(w: &Window, expected: &str) {
    let status = w.find("task-execution-status");
    assert_eq!(status.label(), Some(expected));
    let bounds = status.bounds();
    assert!(bounds.size.width > px(0.), "status has no width: {bounds:?}");
    assert!(
        bounds.right() <= w.viewport_size().width,
        "status overflows right edge: {bounds:?}, viewport {:?}",
        w.viewport_size()
    );
}

fn ordinary_status_check(directory: &Path, mode: Appearance, width: f32, height: f32) {
    let scenario = directory.join(format!("ordinary-status-{mode:?}-{width}"));
    std::fs::create_dir_all(&scenario).unwrap();
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = scenario.to_string_lossy().into();
    store.set_appearance(mode);
    let task = store
        .create_task("Ordinary status layout", 1, Stage::InProgress, None)
        .unwrap();
    let path = scenario.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, _app) = with_app(store, &path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::Project(1));
        a.fixture_select_task(Some(task));
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_status_fits(w, "Execution · Paused");
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("ordinary-status-{mode:?}-{width}.png"));
}

fn list_and_board(directory: &Path, mode: Appearance, width: f32, height: f32) {
    for layout in [Layout::List, Layout::Board] {
        let scenario = directory.join(format!("archive-{layout:?}"));
        std::fs::create_dir_all(&scenario).unwrap();
        let (mut store, goal, _) = completed_goal(&scenario);
        store.set_appearance(mode);
        store.archive(goal).unwrap();
        let path = scenario.join("workspace.sqlite3");
        Storage::new(path.clone()).save(&store).unwrap();
        let name = format!("archive-{}.png", match layout {
            Layout::List => "list",
            Layout::Board => "board",
        });
        let (mut cx, handle, app) = with_app(store, &path, width, height, |a, _, _| {
            a.set_scope_identity(Scope::All);
            a.layout = layout;
            a.show_done = false;
        });
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.click("archive", cx);
            w.render_frame(cx);
            let nav_count = w
                .find("nav-count-Archive")
                .label()
                .map(str::to_string)
                .unwrap_or_default();
            assert_eq!(nav_count, "1");
            let summary = w
                .find("collection-summary")
                .label()
                .map(str::to_string)
                .unwrap_or_default();
            assert!(summary.starts_with("1 goals"), "{summary}");
            assert_eq!(app.read(cx).visible_tasks().len(), 1);
            assert_eq!(app.read(cx).scope(), Scope::Archive);
            let status = if layout == Layout::List {
                w.find(format!("task-status-{goal}"))
                    .label()
                    .map(str::to_string)
            } else {
                w.find(format!("card-status-{goal}"))
                    .label()
                    .map(str::to_string)
            };
            let status = status.unwrap_or_default();
            assert!(status.contains("Archived · 3/3 complete"), "{status}");
            assert!(w.find(format!("delete-archived-{goal}")).visible());
            if layout == Layout::List {
                assert!(w.find(format!("task-row-{goal}")).visible());
                assert!(w.find(format!("task-title-container-{goal}")).bounds().size.width > px(60.), "archive actions must leave room for the title");
                assert!(w.find(format!("restore-archived-{goal}")).visible());
            } else {
                assert!(w.find(format!("card-{goal}")).visible());
                assert!(w.find(format!("restore-archived-{goal}")).visible());
                assert!(w.try_find("board-guidance").is_none());
                assert!(w.try_find("empty-drop-todo").is_none());
                assert!(w.try_find("empty-drop-in-progress").is_none());
                assert!(w.try_find("execution-status-filter").is_none());
            }
        })
        .unwrap();
        screenshot(&mut cx, handle, directory, &name);
        drop(cx);
        if layout == Layout::List {
            default_archive_visibility(directory, mode, width, height);
        }
    }
}

fn default_archive_visibility(directory: &Path, mode: Appearance, width: f32, height: f32) {
    let scenario = directory.join("archive-default");
    std::fs::create_dir_all(&scenario).unwrap();
    let (mut store, goal, _) = completed_goal(&scenario);
    store.set_appearance(mode);
    store.archive(goal).unwrap();
    let path = scenario.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::All);
        a.layout = Layout::List;
        // Default application state: completed items are shown.
        a.show_done = true;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("archive", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).visible_tasks().len(), 1);
        let status = w
            .find(format!("task-status-{goal}"))
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(status.contains("Archived · 3/3 complete"), "{status}");
    })
    .unwrap();
}

fn detail_and_restore(directory: &Path, mode: Appearance, width: f32, height: f32) {
    let scenario = directory.join(format!("archive-detail-{mode:?}-{width}"));
    std::fs::create_dir_all(&scenario).unwrap();
    let (mut store, goal, steps) = completed_goal(&scenario);
    store.set_appearance(mode);
    store.archive(goal).unwrap();
    let path = scenario.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::Archive);
        a.fixture_select_task(Some(goal));
        a.layout = Layout::List;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("archived-research-badge").visible());
        assert!(w.find(format!("restore-archived-{goal}")).visible());
        assert!(w.try_find("goal-resume").is_none());
        assert!(w.try_find("goal-export").is_none());
        assert!(w.try_find("approve-research-topic").is_none());
        assert!(w.try_find("regenerate-report").is_none());
        assert_eq!(w.find("property-Stage").disabled(), Some(true));
        assert_status_fits(w, "Archived · 3/3 complete");
        let materials = w
            .find("material-drop-hint")
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(materials.contains("read-only"), "{materials}");
        assert!(!materials.contains("Drop PDFs"), "{materials}");
        assert_eq!(w.find("add-attachments").disabled(), Some(true));
        for step in &steps {
            assert!(w.find(format!("subtask-{step}")).visible());
            assert!(
                w.try_find(format!("subtask-refresh-{step}")).is_none(),
                "validated archived step must not say Needs refresh"
            );
        }
        w.find("research-task-result");
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("archive-detail-{mode:?}-{width}.png"));

    cx.update_window(handle, |_, w, cx| {
        w.click("goal-steps-tab", cx);
        w.render_frame(cx);
        for (index, step) in steps.iter().enumerate() {
            if index > 0 {
                w.scroll(
                    "steps-cards-Done",
                    ScrollDelta::Pixels(point(px(0.), px(-160.))),
                    cx,
                );
                w.render_frame(cx);
            }
            let status = w
                .find(format!("goal-step-status-{step}"))
                .label()
                .map(str::to_string)
                .unwrap_or_default();
            assert!(status.contains("Archived"), "{status}");
            assert!(w.find(format!("goal-step-{step}")).visible());
        }
        assert!(w.try_find("steps-column-Todo").is_none());
        assert!(w.try_find("steps-column-Execution").is_none());
        assert!(w.try_find("execution-status-filter").is_none());
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("archive-detail-steps-{mode:?}-{width}.png"));
    cx.update_window(handle, |_, w, cx| {
        w.click("goal-overview-tab", cx);
        w.render_frame(cx);
    })
    .unwrap();

    let step = steps[0];
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("subtask-{step}"), cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("archived-research-badge").visible());
        assert!(w.try_find("completed-update-needed").is_none());
        assert!(w.try_find("archive-retained-attempt-notice").is_none());
        assert_status_fits(w, "Archived · result retained");
        assert!(w.try_find("step-run").is_none());
        w.find("task-execution-status");
        assert_status_fits(w, "Archived · result retained");
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("archive-step-{mode:?}-{width}.png"));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_task(goal, w, cx));
        w.render_frame(cx);
    })
    .unwrap();

    cx.update_window(handle, |_, w, cx| {
        w.click("research-task-result", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("research-result-scroll").visible());
        assert_eq!(app.read(cx).task_result_context(), Some(goal));
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("archive-retained-result-{mode:?}-{width}.png"));

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_task(goal, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click(format!("restore-archived-{goal}"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).store.task(goal).unwrap().archived);
        assert!(app.read(cx).store.goal_tasks_complete(goal));
        assert!(w.try_find("archived-research-badge").is_none());
        assert!(w.try_find(format!("restore-archived-{goal}")).is_none());
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, &format!("archive-restored-{mode:?}-{width}.png"));
}

fn qa_prompt_builder(
    _level: PromptLevel,
    message: &str,
    _detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let focus = cx.focus_handle();
    let actions = actions.iter().map(|action| action.label().to_string()).collect::<Vec<_>>();
    let message = message.to_string();
    let view = cx.new(|_| QaPrompt { focus, message, actions });
    handle.with_view(view, window, cx)
}

struct QaPrompt {
    focus: FocusHandle,
    message: String,
    actions: Vec<String>,
}
impl EventEmitter<PromptResponse> for QaPrompt {}
impl Focusable for QaPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for QaPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("qa-prompt")
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgba(0x00000080))
            .child(label(self.message.clone(), cx))
            .child(row().gap_2().children(self.actions.iter().enumerate().map(|(index, action)| {
                button(format!("qa-prompt-{index}"), action.clone())
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(index))))
            })))
    }
}

fn ancestor_archive_material_check(root: &Path) {
    let scenario = root.join("ancestor-material");
    std::fs::create_dir_all(&scenario).unwrap();
    let (mut store, goal, steps) = completed_goal(&scenario);
    let topic = store.task_topic(goal).unwrap().id;
    store.set_topic_archived(topic, true).unwrap();
    store.set_appearance(Appearance::Light);
    let path = scenario.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let step = steps[0];
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::Archive);
        a.fixture_select_task(Some(step));
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).store.task(step).unwrap().archived);
        assert!(app.read(cx).store.task_is_archived(step));
        assert_eq!(w.find("add-attachments").disabled(), Some(true));
        let hint = w
            .find("material-drop-hint")
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(hint.contains("read-only"), "{hint}");
        assert!(w.try_find("step-run").is_none());
        assert!(w.try_find("completed-update-needed").is_none());
        assert!(w.try_find("archive-retained-attempt-notice").is_none());
        assert_status_fits(w, "Archived · result retained");
    })
    .unwrap();
    screenshot(&mut cx, handle, &scenario, "ancestor-archived-step.png");
}

fn active_archive_confirmation(root: &Path) {
    let directory = root.join("active-confirmation");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = completed_goal(&directory);
    let mut active = store.prepare_run(steps[0]).unwrap();
    active.status = "Running".into();
    store.push_run(active);
    let path = directory.join("active.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::Archive);
        a.fixture_select_task(Some(goal));
    });
    // Headless platform prompts are queued but not rendered by `has_active_prompt`.
    // Use GPUI's supported fallback prompt renderer so Cancel/Confirm are real clicks.
    cx.update(|cx| cx.set_prompt_builder(qa_prompt_builder));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.archive_task(goal, w, cx));
    })
    .unwrap();
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.has_active_prompt(), "active archive must ask for confirmation");
        w.click("qa-prompt-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!w.has_active_prompt());
        assert!(
            !app.read(cx).store.task(goal).unwrap().archived,
            "Cancel must not archive"
        );
        assert!(app
            .read(cx)
            .store
            .runs()
            .iter()
            .any(|run| run.task_id == steps[0] && run.status == "Running"));
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "active-archive-cancel.png");

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.archive_task(goal, w, cx));
    })
    .unwrap();
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.has_active_prompt());
        w.click("qa-prompt-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).store.task(goal).unwrap().archived);
        assert!(app
            .read(cx)
            .store
            .runs()
            .iter()
            .filter(|run| run.task_id == steps[0])
            .all(|run| run.status != "Running"));
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "active-archive-confirm.png");
}

fn with_app<F>(
    store: Store,
    path: &Path,
    width: f32,
    height: f32,
    configure: F,
) -> (HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>)
where
    F: FnOnce(&mut TaskApp, &mut Window, &mut Context<TaskApp>),
{
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut entity = None;
    let handle = cx
        .open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                let appearance = store.appearance();
                a.store = store;
                a.set_appearance(appearance, w, cx);
                configure(a, w, cx);
                cx.notify();
            });
            entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap()
        .into();
    (cx, handle, entity.unwrap())
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, directory: &Path, name: &str) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}

fn goal_menu_labels(root: &Path) {
    for archived in [false, true] {
        let directory = root.join(format!("goal-menu-{archived}"));
        std::fs::create_dir_all(&directory).unwrap();
        let (mut store, goal, _) = completed_goal(&directory);
        if archived {
            store.archive(goal).unwrap();
        }
        let path = directory.join("workspace.sqlite3");
        Storage::new(path.clone()).save(&store).unwrap();
        let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
            a.set_scope_identity(if archived { Scope::Archive } else { Scope::All });
            a.fixture_select_task(Some(goal));
        });
        cx.update_window(handle, |_, w, cx| {
            assert_eq!(app.read(cx).selected(), Some(goal));
            w.render_frame(cx);
            w.click("task-actions-menu", cx);
            w.render_frame(cx);
            let expected = if archived { "Restore goal" } else { "Archive goal" };
            assert!(w.find_by_label(expected).visible(), "goal menu must say {expected}");
            w.press("escape", cx);
            w.render_frame(cx);
        })
        .unwrap();
        drop(cx);
    }
}

fn topic_directory_visibility(root: &Path) {
    let directory = root.join("topic-directory");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, _) = completed_goal(&directory);
    let listed = store.task_topic(goal).map(|topic| topic.id).unwrap();
    store.set_topic_archived(listed, true).unwrap();
    let empty = store.create_topic(1, "Empty archived topic").unwrap();
    store.set_topic_archived(empty, true).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::Archive);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find(format!("topic-link-{listed}")).visible());
        assert!(
            w.try_find(format!("topic-link-{empty}")).is_none(),
            "an archived topic without goals must leave the Archive topic strip"
        );
        assert!(app.read(cx).store.topic(empty).is_some(), "hiding must not delete the topic");
        assert!(app.read(cx).store.topic(listed).is_some());
    })
    .unwrap();
    drop(cx);
}

fn permanent_delete_confirmation(root: &Path) {
    let directory = root.join("delete-confirmation");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = completed_goal(&directory);
    store.archive(goal).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::Archive);
        a.layout = Layout::List;
    });
    cx.update(|cx| cx.set_prompt_builder(qa_prompt_builder));
    // An unavailable save must leave the entire deletion scope intact.
    cx.update(|cx| app.update(cx, |a, cx| {
        let target = crate::archive_app::ArchiveTarget::Task(goal);
        let reviewed = a.store.archive_deletion(target).unwrap();
        let storage = a.storage.take();
        a.confirm_delete_archived(target, reviewed, cx);
        assert!(a.store.task(goal).is_some());
        assert!(steps.iter().all(|id| a.store.task(*id).is_some()));
        assert!(!a.error.is_empty());
        a.storage = storage;
    }));
    for action in ["cancel", "switch-workspace", "delete"] {
        let confirm = action == "delete";
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, _| a.set_scope_identity(Scope::Archive));
            assert!(app.read(cx).store.task_is_archived(goal), "{action}: target must stay archived");
            w.render_frame(cx);
            w.click(format!("delete-archived-{goal}"), cx);
        }).unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w.has_active_prompt());
            assert!(app.read(cx).store.task(goal).is_some(), "confirmation must precede deletion");
            if action == "switch-workspace" {
                app.update(cx, |a, _| a.storage = Some(Storage::new(directory.join("different-workspace.sqlite3"))));
            }
            w.click(if action == "cancel" { "qa-prompt-0" } else { "qa-prompt-1" }, cx);
        }).unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            let a = app.read(cx);
            assert_eq!(a.store.task(goal).is_none(), confirm);
            assert!(steps.iter().all(|id| a.store.task(*id).is_none() == confirm));
            if confirm { assert!(a.undo.is_empty()); }
            if action == "switch-workspace" {
                assert!(a.error.contains("workspace changed"));
                app.update(cx, |a, _| a.storage = Some(Storage::new(path.clone())));
            }
        }).unwrap();
        let disk = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(disk.task(goal).is_none(), confirm);
    }
}
