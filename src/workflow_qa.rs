//! Isolated native drag/drop acceptance. Never invokes a configured real agent.
use crate::{
    app::{AllStatus, Layout, Scope, TaskApp},
    batch::{BatchState, TodoBatch},
    domain::{Stage, Store},
    persistence::Storage,
    workflow::ExecutionGroup,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    os::unix::fs::PermissionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/workflow-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    std::fs::write(&cli, r#"#!/usr/bin/python3
import json, sys, time
prompt = sys.argv[-1]
time.sleep(0.15)
text = 'Local acceptance result'
if 'FINAL RESPONSE CONTRACT' in prompt:
    if 'Current task: Analyze drag evidence' in prompt or 'Current task: Write drag report' in prompt:
        assert 'EVIDENCE_SENTINEL' in prompt, 'Dependencies must be passed before execution'
    text = json.dumps({'status': 'complete', 'report': 'EVIDENCE_SENTINEL Local fixture evidence. [Source](https://example.com/qa)', 'sources': [{'title': 'Local fixture', 'url': 'https://example.com/qa', 'published_at': 'unknown'}]})
print(json.dumps({'type': 'text', 'part': {'text': text}}), flush=True)
"#).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut store = Store::empty();
    store.default_model = "fixture/model".into();
    store.opencode_path = cli.to_string_lossy().into();
    store.research_config.ego_path = cli.to_string_lossy().into();
    store.projects[0].directory = root.to_string_lossy().into();
    let id = store
        .create_task("Drag to submit · isolated task", 1, Stage::Backlog, None)
        .unwrap();
    let path = root.join("workspace.json");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut entity = None;
    let handle: AnyWindowHandle = cx
        .open_window(size(px(1900.), px(1040.)), |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.navigate(Scope::All, w, cx);
                a.layout = Layout::Board;
            });
            entity = Some(app.clone());
            cx.new(|cx| base::Root::new(app, w, cx))
        })
        .unwrap()
        .into();
    let app = entity.unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.drag_to(format!("card-{id}"), "column-in-progress", cx);
        assert_eq!(app.read(cx).store.task(id).unwrap().stage, Stage::Backlog);
        assert!(app.read(cx).store.runs.is_empty());
        w.drag_to(format!("card-{id}"), "column-todo", cx);
        assert_eq!(app.read(cx).store.task(id).unwrap().stage, Stage::Planned);
        w.drag_to(format!("card-{id}"), "column-in-progress", cx);
        assert_eq!(app.read(cx).store.runs.len(), 1, "{}", app.read(cx).error);
        assert_eq!(app.read(cx).store.runs[0].stage, Stage::InProgress);
        assert!(
            app.read(cx).store.runs[0]
                .prompt
                .contains("Implement the agreed task")
        );
        assert_eq!(app.read(cx).scope, Scope::All);
        assert_eq!(
            app.read(cx).store.task_status(id, None).group,
            Some(ExecutionGroup::Queued)
        );
        w.render_frame(cx);
        w.drag_to(format!("card-{id}"), "column-todo", cx);
        assert_eq!(app.read(cx).store.runs.len(), 1);
        assert!(app.read(cx).error.contains("submitted"));
        w.click("execution-filter-running", cx);
        w.render_frame(cx);
        assert!(w.try_find(format!("card-{id}")).is_none());
        w.click("execution-filter-all", cx);
        app.update(cx, |a, cx| a.change_stage(id, Stage::InProgress, cx));
        assert_eq!(
            app.read(cx).store.runs.len(),
            1,
            "Duplicate submission must not start another run"
        );
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.poll_runtime(cx);
            });
            w.render_frame(cx);
        })
        .unwrap();
        if cx
            .update(|cx| app.read(cx).active_runs.is_empty() && app.read(cx).queued_runs.is_empty())
        {
            break;
        }
        assert!(Instant::now() < deadline, "Local fixture timed out");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(30));
    }
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            app.read(cx).store.task_status(id, None).phase,
            Stage::Review
        );
        app.update(cx, |a, cx| {
            a.change_stage(id, Stage::Planned, cx);
            assert!(a.error.contains("Cannot move"));
            a.change_stage(id, Stage::Done, cx);
            assert_eq!(a.store.task_status(id, None).phase, Stage::Done);
            // A failed submission must not create a fake Running task.
            let invalid = a
                .store
                .create_task("Missing model", 1, Stage::Planned, None)
                .unwrap();
            a.store.default_model.clear();
            let count = a.store.runs.len();
            a.change_stage(invalid, Stage::InProgress, cx);
            assert_eq!(a.store.runs.len(), count);
            assert_eq!(a.store.task_status(invalid, None).phase, Stage::Planned);
            a.store.default_model = "fixture/model".into();
            a.error.clear();
            a.store.tasks.retain(|task| task.id != invalid);
            a.show_done = true;
        });
        w.render_frame(cx);
    })
    .unwrap();
    // A single goal drop must execute the complete dependency chain in order.
    let (chain, steps) = app.update(&mut cx, |a, _| {
        let goal = a.store.create_research_goal("Drag dependency acceptance".into(), "Local evidence".into(), 1).unwrap();
        let steps = a.store.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect drag evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze drag evidence","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write drag report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        (goal, steps)
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.drag_to(format!("card-{chain}"), "column-in-progress", cx);
        let a = app.read(cx);
        assert_eq!(
            a.store.task_status(steps[1], a.todo_batch.as_ref()).group,
            Some(ExecutionGroup::Waiting)
        );
        assert!(!a.store.runs.iter().any(|r| r.task_id == steps[1]));
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.poll_runtime(cx);
            });
            w.render_frame(cx);
        })
        .unwrap();
        if cx.update(|cx| app.read(cx).store.goal_tasks_complete(chain)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Research chain did not complete: {}",
            cx.update(|cx| app.read(cx).error.clone())
        );
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(30));
    }
    cx.update(|cx| {
        let a = app.read(cx);
        let runs: Vec<_> = a
            .store
            .runs
            .iter()
            .filter(|r| steps.contains(&r.task_id))
            .collect();
        assert_eq!(runs.len(), 3);
        assert_eq!(runs.iter().map(|r| r.task_id).collect::<Vec<_>>(), steps);
        assert!(runs.iter().all(|r| r.status == "Succeeded"));
        assert_eq!(
            a.store.task_status(chain, a.todo_batch.as_ref()).phase,
            Stage::Done
        );
    });
    // Render a queued research chain; verify the same projection in list, tree and details.
    let goal = app.update(&mut cx, |a, cx| {
        let goal = a.store.create_research_goal("Research dependency chain".into(), "Collect, analyze and report".into(), 1).unwrap();
        let ids = a.store.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze evidence","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        a.todo_batch = Some(TodoBatch { project_id: 1, items: vec![goal], completed: 0, runs_completed: 0,
            state: BatchState::Paused("QA paused queue".into()), run_ids: Default::default(), done_items: vec![], failed_items: vec![], failed_tasks: vec![] });
        a.store.set_stage(goal, Stage::InProgress).unwrap();
        let mut run = a.store.prepare_run(ids[0]).unwrap(); run.status = "Running".into(); a.store.runs.push(run);
        a.expanded_sidebar_projects.push(1);
        cx.notify(); goal
    });
    for (name, mode) in [
        ("light", crate::appearance::Appearance::Light),
        ("dark", crate::appearance::Appearance::Dark),
    ] {
        cx.update_window(handle, |_, w, cx| {
            crate::appearance::apply(mode, Some(w), cx);
            app.update(cx, |a, cx| {
                a.navigate(Scope::All, w, cx);
                a.layout = Layout::Board;
                a.all_status = AllStatus::All;
            });
            w.render_frame(cx);
        })
        .unwrap();
        cx.capture_screenshot(handle)
            .unwrap()
            .save(root.join(format!("board-{name}.png")))
            .unwrap();
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.layout = Layout::List;
            a.all_status = AllStatus::Stage(Stage::InProgress);
            cx.notify();
        });
        w.render_frame(cx);
        assert!(w.find(format!("task-row-{goal}")).visible());
        assert!(w.try_find(format!("task-row-{id}")).is_none());
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("list.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_task(goal, w, cx));
        w.render_frame(cx);
        assert!(w.find("task-execution-status").visible());
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("details.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_goal_steps(goal, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("steps.png"))
        .unwrap();
    println!(
        "Workflow drag, queue, dependency protection and views passed: {}",
        root.display()
    );
    cx.update(|cx| cx.quit());
}

/// Verify actual element positions in all three views, including execution groups.
pub fn display_order() {
    let mut store = Store::empty();
    let old = store.create_topic(1, "Old topic").unwrap();
    let new = store.create_topic(1, "New topic").unwrap();
    let a = store.create_task_in_topic("Old topic old task", 1, Stage::Planned, None, Some(old)).unwrap();
    let b = store.create_task_in_topic("New topic old task", 1, Stage::Planned, None, Some(new)).unwrap();
    let c = store.create_task_in_topic("New topic latest task", 1, Stage::Planned, None, Some(new)).unwrap();
    let d = store.create_task_in_topic("Old topic latest task", 1, Stage::Planned, None, Some(old)).unwrap();
    for task in &mut store.tasks { task.created_at = "2026-11-01 10:00:00 +08:00".into(); }
    store.task_mut(a).unwrap().priority = crate::domain::Priority::Urgent;
    store.tasks.swap(0, 2); // Rendering must not depend on serialization order.
    let expected = [c, b, d, a];
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer);
    cx.update(init);
    let mut entity = None;
    let handle: AnyWindowHandle = cx.open_window(size(px(1440.), px(1000.)), |w, cx| {
        let app = cx.new(|cx| TaskApp::new_mock(w, cx));
        app.update(cx, |app, cx| {
            app.store = store.clone();
            app.expanded_sidebar_projects = vec![1];
            app.navigate(Scope::Project(1), w, cx);
            app.layout = Layout::List;
        });
        entity = Some(app.clone());
        cx.new(|cx| base::Root::new(app, w, cx))
    }).unwrap().into();
    let app = entity.unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(app.read(cx).ordered_visible_tasks().iter().map(|t| t.id).collect::<Vec<_>>(), expected);
        for pair in expected.windows(2) {
            assert!(w.find(format!("task-row-{}", pair[0])).bounds().top() < w.find(format!("task-row-{}", pair[1])).bounds().top());
            assert!(w.find(format!("sidebar-task-{}", pair[0])).bounds().top() < w.find(format!("sidebar-task-{}", pair[1])).bounds().top());
        }
        app.update(cx, |a, cx| { a.layout = Layout::Board; cx.notify(); });
        w.render_frame(cx);
        for pair in expected.windows(2) {
            assert!(w.find(format!("card-{}", pair[0])).bounds().top() < w.find(format!("card-{}", pair[1])).bounds().top());
        }
        app.update(cx, |a, cx| {
            for id in expected { a.store.set_stage(id, Stage::InProgress).unwrap(); }
            cx.notify();
        });
        w.render_frame(cx);
        for pair in expected.windows(2) {
            assert!(w.find(format!("card-{}", pair[0])).bounds().top() < w.find(format!("card-{}", pair[1])).bounds().top());
        }
        assert!(app.read(cx).store.runs.is_empty());
        assert!(app.read(cx).todo_batch.is_none());
    }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let (goal, steps) = app.update(cx, |a, cx| {
            let goal = a.store.create_research_goal("Workflow order".into(), "Scope".into(), 1).unwrap();
            let steps = a.store.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
            for (i, id) in steps.iter().enumerate() { a.store.task_mut(*id).unwrap().created_at = format!("2026-11-0{}T10:00:00Z", i + 1); }
            a.store.tasks.reverse();
            a.collapsed_sidebar_goals.retain(|id| *id != goal);
            a.open_task(goal, w, cx);
            a.goal_steps_tab = Some(goal);
            a.steps_layout = Layout::List;
            cx.notify();
            (goal, steps)
        });
        w.render_frame(cx);
        for pair in steps.windows(2) {
            assert!(w.find(format!("goal-step-{}", pair[0])).bounds().top() < w.find(format!("goal-step-{}", pair[1])).bounds().top());
            assert!(w.find(format!("sidebar-task-{}", pair[0])).bounds().top() < w.find(format!("sidebar-task-{}", pair[1])).bounds().top());
        }
        assert_eq!(app.read(cx).store.detail_task_neighbors(goal).1, steps[0]);
        assert_eq!(app.read(cx).store.detail_task_neighbors(steps[0]).1, steps[1]);
        assert_eq!(app.read(cx).store.detail_task_neighbors(steps[1]).1, steps[2]);
        assert!(app.read(cx).store.runs.is_empty());
    }).unwrap();
    println!("Newest-first sidebar, List, Board and execution-group positions verified.");
    cx.update(|cx| cx.quit());
}
