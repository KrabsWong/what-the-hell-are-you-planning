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
    store.set_default_model("fixture/model".into());
    store.set_opencode_path(cli.to_string_lossy().into());
    store.research_config_mut().ego_path = cli.to_string_lossy().into();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into();
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
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.start_run(id, cx));
        assert!(app.read(cx).store.runs().is_empty(), "Legacy tasks cannot start new runs");
    }).unwrap();
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
        assert!(!a.store.runs().iter().any(|r| r.task_id == steps[1]));
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
            .runs()
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
        let mut run = a.store.prepare_run(ids[0]).unwrap(); run.status = "Running".into(); a.store.push_run(run);
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
    let task_ids: Vec<u64> = store.tasks().iter().map(|task| task.id).collect();
    for id in task_ids {
        store.task_mut(id).unwrap().created_at = "2026-11-01 10:00:00 +08:00".into();
    }
    store.task_mut(a).unwrap().priority = crate::domain::Priority::Urgent;
    store.swap_tasks(0, 2); // Rendering must not depend on serialization order.
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
        assert!(app.read(cx).store.runs().is_empty());
        assert!(app.read(cx).todo_batch.is_none());
    }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let (goal, steps) = app.update(cx, |a, cx| {
            let goal = a.store.create_research_goal("Workflow order".into(), "Scope".into(), 1).unwrap();
            let steps = a.store.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
            for (i, id) in steps.iter().enumerate() { a.store.task_mut(*id).unwrap().created_at = format!("2026-11-0{}T10:00:00Z", i + 1); }
            a.store.reverse_tasks();
            a.collapsed_sidebar_goals.retain(|id| *id != goal);
            a.open_task(goal, w, cx);
            a.set_goal_steps_tab(Some(goal));
            a.set_steps_layout(Layout::List);
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
        assert!(app.read(cx).store.runs().is_empty());
    }).unwrap();
    println!("Newest-first sidebar, List, Board and execution-group positions verified.");
    cx.update(|cx| cx.quit());
}

/// Repeatable horizontal-scroll workload with validated research results, no real agents.
pub fn scroll_benchmark() {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into();
    store.set_default_model("fixture/model".into());
    for n in 0..24 {
        let goal = store.create_research_goal(format!("Scroll benchmark {n}"), "Research scope".into(), 1).unwrap();
        let steps = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        for id in steps {
            let mut run = store.prepare_run(id).unwrap();
            run.status = "Succeeded".into();
            run.output = "Completed benchmark result".into();
            store.push_run(run);
            store.task_mut(id).unwrap().stage = Stage::Done;
        }
        store.sync_goal_progress(goal);
    }
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer);
    cx.update(init);
    let benchmark_goal = store.tasks().first().unwrap().id;
    let mut view = None;
    let window = cx.open_window(size(px(1000.), px(720.)), |w, cx| {
        let app = cx.new(|cx| TaskApp::new_mock(w, cx));
        app.update(cx, |a, cx| { a.store = store; a.navigate(Scope::All, w, cx); a.layout = Layout::Board; });
        view = Some(app.clone());
        cx.new(|cx| base::Root::new(app, w, cx))
    }).unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); }).unwrap();
    let initial_left = cx.update_window(handle, |_, w, _| w.find("column-in-progress").bounds().left()).unwrap();
    let mut elapsed = Vec::new();
    for n in 0..30 {
        let start = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            let position = w.find("board-scroll").bounds().center();
            w.dispatch_event(MouseMoveEvent {
                position,
                ..Default::default()
            }.to_platform_input(), cx);
            w.dispatch_event(ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(if n < 15 { -12. } else { 12. }), px(0.))),
                ..Default::default()
            }.to_platform_input(), cx);
            // The normal input path redraws dirty views. render_frame() forces
            // a full window refresh and defeats retained scrolling entirely.
            w.draw(cx).clear(cx);
        }).unwrap();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.);
        if n == 14 {
            cx.update_window(handle, |_, w, _| {
                assert!(w.find("column-in-progress").bounds().left() < initial_left, "Horizontal gesture must move the board");
            }).unwrap();
        }
    }
    let app = view.unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(app.read(cx).display_task_status(benchmark_goal).phase, Stage::Done);
        app.update(cx, |a, cx| { a.store.task_mut(benchmark_goal).unwrap().stage = Stage::Cancelled; cx.notify(); });
        w.render_frame(cx);
        assert_eq!(app.read(cx).display_task_status(benchmark_goal).phase, Stage::Cancelled, "Render cache must not retain stale status across frames");
    }).unwrap();
    elapsed.sort_by(f64::total_cmp);
    println!("Board scroll workload: 24 goals / 72 completed steps; median {:.2} ms, p95 {:.2} ms per scroll+render", elapsed[15], elapsed[28]);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::Settings, w, cx);
            a.set_settings_tab(crate::settings_views::SettingsTab::Models);
            cx.notify();
        });
        w.render_frame(cx);
    }).unwrap();
    let mut elapsed = Vec::new();
    for n in 0..60 {
        let start = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            let position = w.find("model-settings-scroll").bounds().center();
            w.dispatch_event(MouseMoveEvent {
                position,
                ..Default::default()
            }.to_platform_input(), cx);
            w.dispatch_event(ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(if n < 30 { -12. } else { 12. }))),
                ..Default::default()
            }.to_platform_input(), cx);
            w.draw(cx).clear(cx);
            if n == 29 {
                assert!(app.read(cx).settings_scroll.offset().y < px(-100.),
                    "Vertical gesture must move settings");
            }
        }).unwrap();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.);
    }
    elapsed.sort_by(f64::total_cmp);
    println!("Settings vertical scroll: median {:.2} ms, p95 {:.2} ms per scroll+render", elapsed[30], elapsed[56]);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::All, w, cx);
            a.expanded_sidebar_projects = vec![1];
            a.collapsed_sidebar_topics.clear();
            a.collapsed_sidebar_goals.clear();
            cx.notify();
        });
        w.render_frame(cx);
    }).unwrap();
    let brand_top = cx.update_window(handle, |_, w, _| w.find("sidebar-brand-overlay").bounds().top()).unwrap();
    let mut elapsed = Vec::new();
    for n in 0..120 {
        let start = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            let position = w.find("sidebar-scroll").bounds().center();
            w.dispatch_event(MouseMoveEvent { position, ..Default::default() }.to_platform_input(), cx);
            w.dispatch_event(ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(if n < 60 { -24. } else { 24. }))),
                ..Default::default()
            }.to_platform_input(), cx);
            w.draw(cx).clear(cx);
            assert!((w.find("sidebar-brand-overlay").bounds().top() - brand_top).abs() <= px(0.5),
                "Logo must stay fixed during normal scroll dispatch");
            if n == 59 {
                assert!(app.read(cx).sidebar_scroll.offset().y < px(-500.),
                    "Vertical gesture must move the expanded sidebar");
            }
        }).unwrap();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.);
    }
    elapsed.sort_by(f64::total_cmp);
    println!("Expanded sidebar scroll: 24 goals / 72 steps; median {:.2} ms, p95 {:.2} ms per scroll+render", elapsed[60], elapsed[113]);

}
