//! Isolated T05 presentation check: automation eligibility labels are visible at
//! the supported window sizes and both themes. No model or network is invoked.
use crate::{
    app::TaskApp,
    appearance::Appearance,
    automation::{AutomationPolicy, AutomationTemplate, TimeWindow, TriggerSource},
    domain::{Stage, Store},
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

const PLAN: &str = r#"{"tasks":[{"title":"Collect one","instructions":"Collect one.","kind":"collect","depends_on":[]},{"title":"Collect two","instructions":"Collect two.","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze.","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report.","kind":"advise","depends_on":[2]}]}"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/execution-eligibility-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let name = if mode == Appearance::Light { "light" } else { "dark" };
            let directory = root.join(format!("{name}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.sqlite3");
            let (store, goal, steps, _failed_run) = fixture(mode, &directory);
            Storage::new(path.clone()).save(&store).unwrap();

            let mut cx = HeadlessAppContext::with_platform(
                platform::current_platform(true).text_system(),
                Arc::new(crate::ui_assets::AppAssets),
                platform::current_headless_renderer,
            );
            cx.update(init);
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                        a.store.occurrence_mut(1).unwrap().status =
                            crate::automation::OccurrenceStatus::Running;
                        a.store.occurrence_mut(1).unwrap().reason = None;
                        a.store.mark_occurrence_read(1);
                        a.push_period_overlay(1);
                        cx.notify();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle: AnyWindowHandle = window.into();
            let failed = steps[1];
            let analysis = steps[2];
            let report = steps[3];
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                scroll_to(w, cx, &format!("automation-step-{failed}"));
                let failed_label = w
                    .find(format!("automation-step-status-{failed}"))
                    .label()
                    .unwrap_or_default()
                    .to_string();
                assert!(
                    failed_label.contains("Blocked"),
                    "the failed step alone must be Blocked, got {failed_label}"
                );
                assert_eq!(
                    w.find(format!("automation-step-status-{analysis}")).label(),
                    Some("Waiting for collection"),
                    "the downstream analysis must name the phase barrier"
                );
                assert!(
                    w.find(format!("automation-step-status-{analysis}")).visible(),
                    "the barrier phrase must be visibly rendered, not tooltip-only"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "01-period-step-labels.png");

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.close_occurrence_result(w, cx);
                    a.open_task(analysis, w, cx);
                });
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert_eq!(
                    w.find("step-execution-status").label(),
                    Some("Waiting for collection"),
                    "the task detail surface must expose the same barrier"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-task-status-label.png");

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.open_goal_steps(goal, w, cx));
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                let failed_progress = w
                    .try_find(format!("goal-step-status-{failed}"))
                    .unwrap_or_else(|| panic!("missing progress card for failed step {failed}"));
                assert!(
                    failed_progress.label().unwrap_or_default().contains("Blocked"),
                    "progress must show the failed step as Blocked, got {:?}",
                    failed_progress.label()
                );
                let analysis_progress = w
                    .try_find(format!("goal-step-status-{analysis}"))
                    .unwrap_or_else(|| panic!("missing progress card for analysis {analysis}"));
                assert_eq!(
                    analysis_progress.label(),
                    Some("Waiting for collection"),
                    "progress must expose the same phase barrier as the period and task surfaces"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-goal-steps-status-label.png");

            let waiting = cx.update(|cx| {
                app.read(cx)
                    .store
                    .task_status(analysis, None)
                    .label()
            });
            let report_label = cx.update(|cx| {
                app.read(cx)
                    .store
                    .task_status(report, None)
                    .label()
            });
            assert_eq!(waiting, "Waiting for collection");
            assert_eq!(
                report_label, "Waiting for collection",
                "the final report waits for the first unfinished earlier phase"
            );
            assert_eq!(
                cx.update(|cx| app.read(cx).store.task(goal).map(|task| task.title.clone())),
                Some("Automation goal".into())
            );
        }
    }
    ordinary_progress_matrix(&root);
    println!(
        "T05 execution-eligibility QA passed: visible period/task/goal-step barrier labels and ordinary progress labels at 1440x920 and 1000x680, light and dark. Evidence: {}",
        root.display()
    );
}

fn ordinary_progress_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let name = if mode == Appearance::Light { "light" } else { "dark" };
            let directory = root.join(format!("ordinary-{name}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.sqlite3");
            let mut store = Store::empty();
            store.set_appearance(mode);
            store.set_default_model("fixture/model".into());
            store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
            store.research_config_mut().models = ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/report"]
                .map(String::from);
            let goal = store
                .create_research_goal("Ordinary goal".into(), "Check ordinary progress.".into(), 1)
                .unwrap();
            let steps = store
                .apply_research_plan(
                    goal,
                    r#"{"tasks":[{"title":"Collect","instructions":"Collect.","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze.","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report.","kind":"advise","depends_on":[1]}]}"#,
                )
                .unwrap();
            let mut active = store.prepare_run(steps[0]).unwrap();
            active.status = "Running".into();
            active.logs.push("[fixture] Ordinary running step".into());
            let failed_run = active.id;
            store.push_run(active);
            Storage::new(path.clone()).save(&store).unwrap();

            let mut cx = HeadlessAppContext::with_platform(
                platform::current_platform(true).text_system(),
                Arc::new(crate::ui_assets::AppAssets),
                platform::current_headless_renderer,
            );
            cx.update(init);
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                        if let Some(mut run) =
                            a.store.run_mut(failed_run)
                        {
                            run.status = "Running".into();
                            run.ended_at = None;
                        }
                        a.open_run_progress(crate::app::RunDispatch {
                            run_id: failed_run,
                            automation_owned: false,
                        });
                        cx.notify();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle: AnyWindowHandle = window.into();
            let analysis = steps[1];
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                let collect_row = w
                    .try_find(format!("progress-task-status-{}", steps[0]))
                    .unwrap_or_else(|| panic!("missing ordinary progress row for the running step"));
                assert!(
                    collect_row.label().unwrap_or_default().contains("Running"),
                    "ordinary progress must show the active step, got {:?}",
                    collect_row.label()
                );
                let analysis_row = w
                    .try_find(format!("progress-task-status-{analysis}"))
                    .unwrap_or_else(|| panic!("missing ordinary progress row for analysis"));
                assert!(
                    analysis_row.label().unwrap_or_default().contains("Waiting"),
                    "ordinary progress must keep dependency waiting, got {:?}",
                    analysis_row.label()
                );
            })
            .unwrap();
            assert_eq!(
                cx.update(|cx| app.read(cx).store.task_status(analysis, None).label())
                    .contains("Waiting"),
                true,
                "ordinary eligibility remains dependency-only"
            );
            screenshot(&mut cx, handle, &directory, "ordinary-progress-status-label.png");
        }
    }
}

fn scroll_to(w: &mut Window, cx: &mut App, id: &str) {
    w.render_frame(cx);
    for _ in 0..24 {
        if w.find(id.to_string()).visible() {
            return;
        }
        w.scroll(
            "automation-result-body".to_string(),
            ScrollDelta::Pixels(point(px(0.), px(-120.))),
            cx,
        );
        w.render_frame(cx);
    }
    assert!(w.find(id.to_string()).visible(), "Could not scroll to {id}");
}

fn fixture(mode: Appearance, directory: &Path) -> (Store, u64, Vec<u64>, u64) {
    let mut store = Store::empty();
    store.set_appearance(mode);
    store.set_default_model("fixture/model".into());
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.research_config_mut().models = ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/report"]
        .map(String::from);
    let topic = store.create_topic(1, "Automation topic").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Automation goal".into(),
            "Check the fixture.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    store.task_mut(goal).unwrap().automation_id = Some(7);
    for id in &steps {
        store.task_mut(*id).unwrap().automation_id = Some(7);
    }
    let mut first = store.prepare_run(steps[0]).unwrap();
    first.status = "Succeeded".into();
    first.output = "Collected first evidence".into();
    store.push_run(first);
    store.set_stage(steps[0], Stage::Done).unwrap();
    let mut failed = store.prepare_run(steps[1]).unwrap();
    failed.status = "Failed".into();
    failed.logs.push("[error] Fixture failure".into());
    let failed_run = failed.id;
    store.push_run(failed);
    store.push_automation(crate::automation::AutomationDefinition {
        id: 7,
        version: 1,
        project_id: 1,
        shape: crate::automation::AutomationShape::Research,
        name: "Paused definition, running period".into(),
        objective: "Check the fixture.".into(),
        expected_result: "A result.".into(),
        schedule: crate::automation_schedule::Schedule {
            spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 5 },
            timezone: "UTC".into(),
        },
        status: crate::automation::DefinitionStatus::Paused,
        template: None,
        plan_binding: None,
        approval: None,
        policy: AutomationPolicy::default(),
        cursor: None,
        anchor: None,
        consecutive_failures: 0,
        attention_reason: None,
        created_at: 0,
        updated_at: 0,
    });
    store.push_occurrence(crate::automation::AutomationOccurrence {
        id: 1,
        automation_id: 7,
        definition_version: 1,
        source: TriggerSource::Scheduled,
        trigger_key: "scheduled:1000".into(),
        scheduled_for: Some(1_000),
        requested_at: None,
        actual_started_at: None,
        timezone: "UTC".into(),
        reference_time: 1_000,
        data_window: TimeWindow::unresolved("fixture"),
        snapshot: crate::automation::AutomationOccurrenceSnapshot {
            objective: "Check the fixture.".into(),
            expected_result: "A result.".into(),
            shape: None,
            template: AutomationTemplate { goals: vec![], window_rule: Default::default() },
            models: Default::default(),
            agents: Default::default(),
            materials: vec![],
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "fixture".into(),
        },
        status: crate::automation::OccurrenceStatus::Running,
        reason: None,
        research_goal_id: Some(goal),
        research_goal_ids: vec![goal],
        research_topic_id: Some(topic),
        run_ids: vec![],
        final_run_id: None,
        unread: true,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: 1_000,
        deadline_at: None,
        retry_requested_at: None,
    });
    (store, goal, steps, failed_run)
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    for _ in 0..6 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
