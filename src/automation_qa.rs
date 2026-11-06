//! Offline automation acceptance: fake clock/runner, isolated SQLite workspace, no model calls.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    automation::{DefinitionStatus, OccurrenceStatus},
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/automation-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    pure_checks(&root);
    planning_chain(&root);
    wizard_actions_check(&root);
    execution_chain(&root);
    recovery_chain(&root);
    pause_recovery_chain(&root);
    regeneration_recovery_chain(&root);
    archive_recovery_chain(&root);
    entry_route_check(&root);
    queue_chain(&root);
    let snapshot = ui_interaction(&root);
    render_matrix(&root, &snapshot);
    println!("Automation QA passed · {}", root.display());
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The final wizard step must offer a working enable action next to saving a
/// reviewed draft; a saved draft keeps the lifecycle controls visible but disabled.
fn wizard_actions_check(root: &Path) {
    let directory = root.join("wizard-actions");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let store = model_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "wizard-list.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("new-automation", cx);
        w.click("automation-objective", cx);
        w.input("Check today's fuel prices in Shenzhen, Guangdong", cx);
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_ui.step), 3);
    screenshot(&mut cx, handle, &directory, "wizard-step3.png");
    cx.update_window(handle, |_, w, _| {
        let enable = w.find("enable-automation");
        assert_ne!(
            enable.disabled(),
            Some(true),
            "the final step must offer a working enable action"
        );
        let save = w
            .try_find("save-automation-draft")
            .expect("the final step must also allow keeping a draft");
        assert!(
            save.bounds().origin.y < px(680.),
            "the final action row must stay inside the window"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("save-automation-draft", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (saved, creating) = read_state(&mut cx, &app, |a| {
        (
            a.automation_ui
                .selected
                .and_then(|id| a.store.automation(id))
                .map(|definition| (definition.name.clone(), definition.status)),
            a.automation_ui.creating,
        )
    });
    assert_eq!(saved, Some(("Check today's fuel prices in Shenzhen, Guangdong".into(), DefinitionStatus::Draft)));
    assert!(!creating, "saving finishes the wizard and opens the draft");
    screenshot(&mut cx, handle, &directory, "wizard-step3-saved.png");
    // A draft keeps the lifecycle controls visible but disabled, so they do not
    // appear to vanish before the first enable.
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "automation-detail-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
            cx,
        );
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| {
        assert_eq!(w.find("pause-automation").disabled(), Some(true));
        assert_eq!(w.find("run-automation").disabled(), Some(true));
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "wizard-draft-actions.png");
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "automation-detail-scroll",
            ScrollDelta::Pixels(point(px(0.), px(10_000.))),
            cx,
        );
        w.render_frame(cx);
    })
    .unwrap();
    // Dark theme legibility for the same success notice.
    cx.update_window(handle, |_, w, cx| {
        w.click("toggle-appearance", cx);
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "wizard-step3-saved-dark.png");
    // A real validation error uses the danger tone in the same layout.
    cx.update_window(handle, |_, w, cx| {
        assert!(
            w.try_find("back-automations").is_none(),
            "the detail relies on the app-level Back instead of a duplicate action"
        );
        w.click("navigation-back", cx);
        w.render_frame(cx);
        w.click("new-automation", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert!(read_state(&mut cx, &app, |a| matches!(
        a.automation_ui.feedback,
        Some(Err(_))
    )));
    screenshot(&mut cx, handle, &directory, "wizard-error-dark.png");
    println!("wizard actions passed (enable and save draft both reachable)");
}


/// This drives the real planner-finalize path with a saved planner output.
fn planning_chain(root: &Path) {
    let directory = root.join("planning");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let automation_project = store
        .ensure_automation_project(directory.join("automations").to_string_lossy().into())
        .unwrap();
    let id = store
        .create_automation_draft(
            automation_project,
            crate::automation::AutomationShape::Research,
            "Pending research".into(),
            "Track the semiconductor supply chain".into(),
            "A sourced note".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Daily { hour: 9, minute: 0 },
                timezone: "Asia/Shanghai".into(),
            },
            crate::automation::AutomationPolicy::default(),
            None,
            now(),
        )
        .unwrap();
    let request = store
        .create_automation_request(id, 1, "Track the semiconductor supply chain")
        .unwrap();
    let mut run = store.prepare_request_plan(request).unwrap();
    run.status = "Succeeded".into();
    run.ended_at = Some(crate::opencode::timestamp());
    run.output = r#"{"topic_name":"Semiconductor supply","brief":{"objective":"Track supply","deliverable":"Sourced note","scope":"Latest quarter","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Foundry capacity","brief":"Track announced capacity","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare capacity","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]},{"title":"Equipment orders","brief":"Track equipment bookings","tasks":[{"title":"Collect","instructions":"Collect orders","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare orders","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]}]}"#.into();
    let run_id = run.id;
    store.runs.push(run);
    store
        .research_requests
        .iter_mut()
        .find(|candidate| candidate.id == request)
        .unwrap()
        .planning_run_id = Some(run_id);
    let topics_before = store.topics.len();
    let tasks_before = store.tasks.len();
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let _window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                a.automation_ui.selected = Some(id);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.complete_research_run(run_id, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let (goals, name, status, topics, tasks, auto_dirs) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (
            definition.template.as_ref().map(|template| template.goals.len()),
            definition.name.clone(),
            definition.status,
            a.store.topics.len(),
            a.store.tasks.len(),
            a.store
                .automation(id)
                .map(|definition| definition.project_id)
                .and_then(|project| a.store.projects.iter().find(|p| p.id == project))
                .map(|project| project.directory.clone())
                .unwrap_or_default(),
        )
    });
    assert_eq!(goals, Some(2), "reviewed goals are frozen into the template");
    assert_eq!(name, "Semiconductor supply", "the planner names the subject");
    assert_eq!(status, DefinitionStatus::Draft, "the plan still needs approval");
    assert_eq!(topics, topics_before, "planning must not create a topic");
    assert_eq!(tasks, tasks_before, "planning must not create goals");
    assert!(auto_dirs.ends_with("automations"), "output lives under Automations: {auto_dirs}");
    assert!(
        !read_state(&mut cx, &app, |a| a.store.runs.iter().any(|candidate| {
            candidate.research_kind == Some(crate::research::ResearchKind::Goal)
                && candidate.task_id == 0
        })),
        "freezing the plan must not launch another planner"
    );
    println!("automation planning chain passed (reviewed plan frozen, no topic or replanning)");
}


fn model_store(directory: &Path) -> Store {
    let mut store = Store::empty();
    store.projects[0].name = "自动化验收项目".into();
    store.projects[0].directory = directory.to_string_lossy().into();
    store.default_model = "qa/automation".into();
    store
}

fn draft(store: &mut Store, name: &str) -> u64 {
    let template = crate::automation_execution::build_template(
        "追踪公开信息中值得注意的变化，并保留来源与时间。",
        "一份带来源和证据缺口的简短报告。",
    )
    .unwrap();
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 10 },
        timezone: "Asia/Shanghai".into(),
    };
    store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            name.into(),
            "追踪公开信息中值得注意的变化，并保留来源与时间。".into(),
            "一份带来源和证据缺口的简短报告。".into(),
            schedule,
            Default::default(),
            Some(template),
            now(),
        )
        .unwrap()
}

fn enable(store: &mut Store, id: u64) {
    store
        .approve_and_enable(
            id,
            vec![],
            now(),
        )
        .unwrap();
}

fn pure_checks(root: &Path) {
    // Atomic commit + persistence + dedupe across reopen.
    let directory = root.join("pure-store");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let storage = Storage::new(path.clone());
    let mut store = model_store(&directory);
    let id = draft(&mut store, "十分钟检查");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 1;
    store.automation_mut(id).unwrap().policy.timeout_seconds = 120;
    store.automation_mut(id).unwrap().anchor = Some(now - 600);
    store.automation_mut(id).unwrap().cursor = Some(now);
    let plans = store.plan_automation_ticks(now);
    assert_eq!(plans.len(), 1, "due scan");
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    assert_eq!(commit.queued.len(), 1);
    let occurrence = commit.queued[0];
    storage.save(&store).unwrap();
    let restored = storage.load().unwrap().unwrap();
    assert_eq!(
        restored.occurrence(occurrence).unwrap().status,
        OccurrenceStatus::Queued
    );
    assert!(restored.plan_automation_ticks(now).is_empty(), "duplicate tick");
    assert_eq!(
        restored.automation_occurrence_by_key(
            &restored.occurrence(occurrence).unwrap().trigger_key
        ).unwrap().id,
        occurrence
    );

    // Storage failure leaves no cursor or occurrence behind.
    let blocked = directory.join("blocked-parent");
    std::fs::write(&blocked, "not a directory").unwrap();
    let broken = Storage::new(blocked.join("workspace.sqlite3"));
    let before = restored.clone();
    assert!(broken.save(&restored).is_err());
    assert_eq!(restored, before);

    // Pause/edit/archive cancel only unstarted requests.
    let mut store = restored;
    let pending_before = store.occurrence(occurrence).unwrap().id;
    let cancelled = store.cancel_unstarted_occurrences(
        id,
        crate::automation::SkipCode::Paused,
        "paused by QA",
    );
    assert_eq!(cancelled, vec![pending_before]);
    assert_eq!(
        store.occurrence(occurrence).unwrap().status,
        OccurrenceStatus::Skipped
    );

    // Large missed range is summarized, not expanded.
    store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 60;
    store.automation_mut(id).unwrap().anchor = Some(now - 600);
    store.automation_mut(id).unwrap().cursor = Some(now);
    let late = now + 30 * 86_400 + 300;
    let plans = store.plan_automation_ticks(late);
    let commit = store.commit_automation_tick(&plans[0], late).unwrap();
    assert!(commit.queued.is_empty());
    let skipped = store.occurrence(commit.skipped[0]).unwrap();
    assert!(skipped.skipped_count > 4_000);
    assert_eq!(store.automation_occurrences.len(), 2);

    // A fake runner records starts and data windows without a process.
    let mut runner = crate::automation_runtime::FakeAutomationRunner::default();
    runner.start(&store.occurrence(commit.skipped[0]).unwrap().clone());
    assert_eq!(runner.starts.len(), 1);
    assert!(!runner.starts[0].window_resolved, "a task without an explicit range must stay unresolved");

    // Large history remains bounded and readable.
    let mut history_store = model_store(&directory);
    let history_id = draft(&mut history_store, "历史容量检查");
    enable(&mut history_store, history_id);
    for index in 0..500 {
        let mut occurrence = {
            let definition = history_store.automation(history_id).unwrap();
            crate::automation_runtime::FakeAutomationRunner::default();
            // Reuse a direct skipped record so no research entities are fabricated.
            let key = format!("manual:qa-history-{index}");
            crate::automation::AutomationOccurrence {
                id: 0,
                automation_id: history_id,
                definition_version: definition.version,
                source: crate::automation::TriggerSource::Manual,
                trigger_key: key,
                scheduled_for: None,
                requested_at: Some(now - index),
                actual_started_at: None,
                timezone: definition.schedule.timezone.clone(),
                reference_time: now - index,
                data_window: crate::automation::TimeWindow::unresolved("QA history"),
                snapshot: crate::automation::AutomationOccurrenceSnapshot {
                    objective: definition.objective.clone(),
                    expected_result: definition.expected_result.clone(),
                    template: definition.template.clone().unwrap(),
                    models: std::array::from_fn(|_| "qa/automation".to_string()),
                    agents: Default::default(),
                    materials: vec![],
                    policy: definition.policy.clone(),
                    approval_version: definition.version,
                    approval_signature: "qa".into(),
                },
                status: OccurrenceStatus::Skipped,
                reason: Some(crate::automation::OccurrenceReason::new(
                    crate::automation::SkipCode::CatchUpSkipped,
                    "QA history entry",
                )),
                research_goal_id: None,
                research_goal_ids: vec![],
                research_topic_id: None,
                run_ids: vec![],
                final_run_id: None,
                unread: false,
                skipped_count: 0,
                skipped_from: None,
                skipped_to: None,
                created_at: now - index,
                deadline_at: None,
            }
        };
        occurrence.id = history_store.next_occurrence_id().unwrap();
        history_store
            .record_occurrence_without_research(occurrence)
            .unwrap();
    }
    assert_eq!(history_store.automation_occurrences(history_id).len(), 500);
    assert_eq!(
        history_store
            .automation_occurrences
            .iter()
            .take(20)
            .filter(|o| o.unread)
            .count(),
        0
    );
    println!("pure automation checks passed");
}

/// Full queued execution with a fake CLI: real dispatch, completion, step hand-off and
/// final-report validation, without a model or network.
fn execution_chain(root: &Path) {
    let directory = root.join("execution");
    std::fs::create_dir_all(&directory).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys,time
args=sys.argv[1:]
prompt=args[-1]
if 'Collect current evidence' in prompt:
    time.sleep(2.2)
if 'QA_REGENERATION_TIMEOUT' in prompt:
    time.sleep(60)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Automation execution chain report','sources':[{'title':'Fixture source','url':'https://example.com/automation-chain','published_at':'unknown'}]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let id = draft(&mut store, "Execution chain");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 1;
    store.automation_mut(id).unwrap().policy.timeout_seconds = 120;
    store.automation_mut(id).unwrap().anchor = Some(now - 600);
    store.automation_mut(id).unwrap().cursor = Some(now);
    let plans = store.plan_automation_ticks(now);
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    let occurrence = commit.queued[0];
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                a.automation_ui.selected = Some(id);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = cx.update_window(window.into(), |_, w, cx| {
            app.update(cx, |a, cx| a.poll_runtime(cx));
            w.render_frame(cx);
            app.read(cx)
                .store
                .occurrence(occurrence)
                .map(|value| value.status)
                .unwrap()
        })
        .unwrap();
        if status.is_terminal() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let debug = read_state(&mut cx, &app, |a| {
                let occ = a.store.occurrence(occurrence).unwrap();
                let runs = a.store.runs.iter().filter(|r| r.automation_occurrence_id == Some(occurrence)).map(|r| (r.id, r.task_id, r.status.clone(), r.logs.iter().rev().take(3).cloned().collect::<Vec<_>>())).collect::<Vec<_>>();
                (occ.status, occ.reason.clone(), runs, a.queued_runs.len(), a.active_runs.len())
            });
            panic!("automation execution chain timed out in {status:?}: {debug:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let (status, run_count, final_run, goal) = read_state(&mut cx, &app, |a| {
        let value = a.store.occurrence(occurrence).unwrap();
        (
            value.status,
            value.run_ids.clone(),
            value.final_run_id,
            value.research_goal_id.unwrap(),
        )
    });
    assert_eq!(status, OccurrenceStatus::Succeeded);
    assert_eq!(run_count.len(), 3, "{run_count:?}");
    assert!(final_run.is_some());
    let sessions: std::collections::HashSet<_> = read_state(&mut cx, &app, |a| {
        a.store
            .runs
            .iter()
            .filter(|run| run.automation_occurrence_id == Some(occurrence))
            .filter_map(|run| run.session_id.clone())
            .collect()
    });
    assert_eq!(sessions.len(), 3, "each step gets its own session");
    assert_eq!(read_state(&mut cx, &app, |a| a.store.research_children(goal).len()), 3);
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .goal_tasks_complete(goal)));
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .occurrence(occurrence)
        .unwrap()
        .unread));
    // Report regeneration is an ordinary entry point and keeps the occurrence
    // snapshot instead of creating a run without automation ownership.
    let source = read_state(&mut cx, &app, |a| {
        a.store.occurrence(occurrence).unwrap().final_run_id.unwrap()
    });
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.error.clear();
            a.regenerate_report(source, "更清楚一些", cx);
        });
    })
    .unwrap();
    let regeneration_deadline = std::time::Instant::now() + Duration::from_secs(20);
    let regenerated = loop {
        let found = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx).store.runs.iter()
                    .filter(|run| run.automation_occurrence_id == Some(occurrence) && run.id > source)
                    .filter(|run| run.status == "Succeeded" && !run.output.trim().is_empty())
                    .max_by_key(|run| run.id)
                    .map(|run| run.id)
            })
            .unwrap();
        if let Some(id) = found { break id; }
        if std::time::Instant::now() >= regeneration_deadline {
            let debug = read_state(&mut cx, &app, |a| {
                (
                    a.error.clone(),
                    a.store.runs.iter().filter(|run| run.automation_occurrence_id == Some(occurrence)).map(|run| (run.id, run.task_id, run.status.clone(), run.output.len(), run.logs.iter().rev().take(3).cloned().collect::<Vec<_>>())).collect::<Vec<_>>(),
                    a.queued_runs.len(),
                    a.active_runs.len(),
                )
            });
            panic!("automation report regeneration did not finish: {debug:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (final_run_id, regenerated_model, regenerated_dir) = read_state(&mut cx, &app, |a| {
        let run = a.store.runs.iter().find(|run| run.id == regenerated).unwrap();
        (
            a.store.occurrence(occurrence).unwrap().final_run_id,
            run.model.clone(),
            run.directory.clone(),
        )
    });
    assert_eq!(final_run_id, Some(regenerated));
    assert_eq!(regenerated_model, "fixture/advise");
    // Regeneration keeps the occurrence's own working directory and leaves no
    // automation-only agent configuration behind.
    assert!(!PathBuf::from(&regenerated_dir).join("opencode.json").exists());
    // A regeneration gets its own deadline. A hung fake CLI must be cancelled, the
    // slot released, and the previous successful report preserved.
    let timeout_source = regenerated;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.store
                .occurrence_mut(occurrence)
                .unwrap()
                .snapshot
                .policy
                .timeout_seconds = 1;
            a.error.clear();
            a.regenerate_report(timeout_source, "QA_REGENERATION_TIMEOUT", cx);
        });
    })
    .unwrap();
    let timeout_run = read_state(&mut cx, &app, |a| {
        a.store
            .runs
            .iter()
            .filter(|run| run.automation_occurrence_id == Some(occurrence) && run.id > timeout_source)
            .max_by_key(|run| run.id)
            .map(|run| run.id)
            .expect("timed-out regeneration run")
    });
    let timeout_deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                let a = app.read(cx);
                a.store
                    .runs
                    .iter()
                    .find(|run| run.id == timeout_run)
                    .is_some_and(|run| run.status == "Cancelled" || run.status == "Failed")
                    && !a.active_runs.iter().any(|run| run.id == timeout_run)
                    && !a.queued_runs.iter().any(|(id, _)| *id == timeout_run)
            })
            .unwrap();
        if done {
            break;
        }
        assert!(
            std::time::Instant::now() < timeout_deadline,
            "hung regeneration was not cancelled at its own deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let (final_report, status, timeout_status) = read_state(&mut cx, &app, |a| {
        (
            a.store.occurrence(occurrence).unwrap().final_run_id,
            a.store.occurrence(occurrence).unwrap().status,
            a.store.runs.iter().find(|run| run.id == timeout_run).map(|run| run.status.clone()),
        )
    });
    assert_eq!(final_report, Some(regenerated));
    assert_eq!(status, OccurrenceStatus::Succeeded);
    assert_eq!(status, OccurrenceStatus::Succeeded);
    assert!(
        matches!(timeout_status.as_deref(), Some("Cancelled") | Some("Failed")),
        "{timeout_status:?}"
    );
    println!("real queue automation chain passed (fake CLI, 3 steps, isolated sessions, regeneration gated and timed out)");
}

/// Simulate a crash after a session started, reopen, verify the session, import its
/// result and continue the same occurrence.
fn recovery_chain(root: &Path) {
    let directory = root.join("recovery");
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys
args=sys.argv[1:]
if args[:2]==['api','session.active']:
    print(json.dumps({'data':{}}));sys.exit(0)
if args[:2]==['session','export']:
    print(open('session-export.json').read());sys.exit(0)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Recovered automation report','sources':[{'title':'Recovered source','url':'https://example.com/automation-recovery','published_at':'unknown'}]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let id = draft(&mut store, "Recover automation");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().anchor = Some(now - 720);
    store.automation_mut(id).unwrap().cursor = Some(now - 120);
    let plans = store.plan_automation_ticks(now);
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    let occurrence = commit.queued[0];
    let goal = store.occurrence(occurrence).unwrap().research_goal_id.unwrap();
    let first_task = store.research_children(goal)[0];
    store
        .occurrence_mut(occurrence)
        .unwrap()
        .mark_running(now - 20)
        .unwrap();
    let mut run = store.prepare_occurrence_run(&store.occurrence(occurrence).unwrap().clone(), now - 20).unwrap();
    run.task_id = first_task;
    run.status = "Running".into();
    run.session_id = Some("ses_automation_recovery".into());
    run.started_at = crate::opencode::timestamp();
    run.directory = store.execution_directory(first_task).unwrap().to_string_lossy().into();
    std::fs::create_dir_all(&run.directory).unwrap();
    let output = serde_json::json!({"status":"complete","report":"Recovered automation evidence","sources":[{"title":"Recovered source","url":"https://example.com/automation-recovery","published_at":"unknown"}]}).to_string();
    let transcript = serde_json::json!({"messages":[
        {"type":"user","text":run.prompt},
        {"type":"assistant","content":[{"type":"text","text":output}]},
        {"type":"idle","outcome":"succeeded"}
    ]});
    std::fs::write(
        PathBuf::from(&run.directory).join("session-export.json"),
        transcript.to_string(),
    )
    .unwrap();
    run.logs.push("[fixture] Crashed after the collection session started.".into());
    let run_id = run.id;
    store.runs.push(run);
    store.occurrence_mut(occurrence).unwrap().run_ids.push(run_id);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let _ = first_task;

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx)
                    .store
                    .occurrence(occurrence)
                    .map(|value| value.status)
                    .unwrap()
            })
            .unwrap();
        if status.is_terminal() {
            assert_eq!(status, OccurrenceStatus::Succeeded, "recovered occurrence");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "recovery chain timed out in {status:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let (runs, final_report) = read_state(&mut cx, &app, |a| {
        (
            a.store
                .runs
                .iter()
                .filter(|run| run.automation_occurrence_id == Some(occurrence))
                .count(),
            a.store.occurrence(occurrence).unwrap().final_run_id,
        )
    });
    assert!(runs >= 3, "recovered collection plus two following steps: {runs}");
    assert!(final_report.is_some());
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .goal_tasks_complete(goal)));
    let (definition_status, cursor) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (definition.status, definition.cursor)
    });
    assert_eq!(definition_status, DefinitionStatus::Enabled);
    assert!(cursor.is_some());
    // A successfully verified recovery must resume the schedule: force the next
    // period and prove it executes, not just that the recovered instance succeeded.
    let next_now = chrono::Utc::now().timestamp();
    let second = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let definition = a.store.automation_mut(id).unwrap();
                definition.anchor = Some(next_now - 60);
                definition.cursor = Some(next_now);
                a.poll_automations(cx);
                a.store
                    .automation_occurrences(id)
                    .into_iter()
                    .filter(|o| o.id != occurrence)
                    .max_by_key(|o| o.id)
                    .map(|o| o.id)
            })
        })
        .unwrap();
    let second = match second {
        Some(second) => second,
        None => {
            let debug = read_state(&mut cx, &app, |a| {
                let definition = a.store.automation(id).unwrap();
                (
                    definition.status,
                    definition.attention_reason.clone(),
                    definition.cursor,
                    definition.approval_current(),
                    a.error.clone(),
                    a.store.plan_automation_ticks(chrono::Utc::now().timestamp()).len(),
                )
            });
            panic!("next period must be created after recovery: {debug:?}")
        }
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx)
                    .store
                    .occurrence(second)
                    .map(|value| value.status)
                    .unwrap()
            })
            .unwrap();
        if status.is_terminal() {
            assert_eq!(status, OccurrenceStatus::Succeeded, "next period");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "next period after recovery timed out in {status:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("recovery automation chain passed (crash, reopen, session import, continuation, next period)");
}

/// Normal step entry points must route automation-owned steps to their occurrence
/// and must not start anything while unattended execution is gated.
fn pause_recovery_chain(root: &Path) {
    let directory = root.join("pause-recovery");
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys
args=sys.argv[1:]
if args[:2]==['api','session.active']:
    print(json.dumps({'data':{}}));sys.exit(0)
if args[:2]==['session','export']:
    print(open('session-export.json').read());sys.exit(0)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Paused recovery report','sources':[{'title':'Paused source','url':'https://example.com/paused-recovery','published_at':'unknown'}]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let id = draft(&mut store, "Paused recovery");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().anchor = Some(now - 720);
    store.automation_mut(id).unwrap().cursor = Some(now - 120);
    let plans = store.plan_automation_ticks(now);
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    let occurrence = commit.queued[0];
    let goal = store.occurrence(occurrence).unwrap().research_goal_id.unwrap();
    let first_task = store.research_children(goal)[0];
    store.occurrence_mut(occurrence).unwrap().mark_running(now - 20).unwrap();
    store.occurrence_mut(occurrence).unwrap().transition(
        crate::automation::OccurrenceStatus::Running,
        None,
    ).ok();
    let mut run = store.prepare_occurrence_run(&store.occurrence(occurrence).unwrap().clone(), now - 20).unwrap();
    run.task_id = first_task;
    run.status = "Running".into();
    run.session_id = Some("ses_paused_recovery".into());
    run.started_at = crate::opencode::timestamp();
    run.directory = store.execution_directory(first_task).unwrap().to_string_lossy().into();
    std::fs::create_dir_all(&run.directory).unwrap();
    let output = serde_json::json!({"status":"complete","report":"Paused recovered evidence","sources":[{"title":"Paused source","url":"https://example.com/paused-recovery","published_at":"unknown"}]}).to_string();
    let transcript = serde_json::json!({"messages":[
        {"type":"user","text":run.prompt},
        {"type":"assistant","content":[{"type":"text","text":output}]},
        {"type":"idle","outcome":"succeeded"}
    ]});
    std::fs::write(PathBuf::from(&run.directory).join("session-export.json"), transcript.to_string()).unwrap();
    let run_id = run.id;
    store.runs.push(run);
    store.occurrence_mut(occurrence).unwrap().run_ids.push(run_id);
    // User pauses while the current period is running; restart must preserve that.
    store.pause_automation(id, now).unwrap();
    assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::Paused);
    assert_eq!(store.automation(id).unwrap().cursor, None);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx)
                    .store
                    .occurrence(occurrence)
                    .map(|value| value.status)
                    .unwrap()
            })
            .unwrap();
        if status.is_terminal() {
            assert_eq!(status, OccurrenceStatus::Succeeded);
            break;
        }
        assert!(std::time::Instant::now() < deadline, "paused recovery timed out in {status:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (status, cursor, reason) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (definition.status, definition.cursor, definition.attention_reason.clone())
    });
    assert_eq!(status, DefinitionStatus::Paused);
    assert_eq!(cursor, None);
    assert!(reason.is_none());
    println!("paused recovery chain passed (user pause survives restart verification)");
}

fn archive_recovery_chain(root: &Path) {
    let directory = root.join("archived-recovery");
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys
args=sys.argv[1:]
if args[:2]==['api','session.active']:
    print(json.dumps({'data':{}}));sys.exit(0)
if args[:2]==['session','export']:
    print(open('session-export.json').read());sys.exit(0)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Paused recovery report','sources':[{'title':'Paused source','url':'https://example.com/paused-recovery','published_at':'unknown'}]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let id = draft(&mut store, "Archived recovery");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().anchor = Some(now - 720);
    store.automation_mut(id).unwrap().cursor = Some(now - 120);
    let plans = store.plan_automation_ticks(now);
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    let occurrence = commit.queued[0];
    let goal = store.occurrence(occurrence).unwrap().research_goal_id.unwrap();
    let first_task = store.research_children(goal)[0];
    store.occurrence_mut(occurrence).unwrap().mark_running(now - 20).unwrap();
    store.occurrence_mut(occurrence).unwrap().transition(
        crate::automation::OccurrenceStatus::Running,
        None,
    ).ok();
    let mut run = store.prepare_occurrence_run(&store.occurrence(occurrence).unwrap().clone(), now - 20).unwrap();
    run.task_id = first_task;
    run.status = "Running".into();
    run.session_id = Some("ses_archived_recovery".into());
    run.started_at = crate::opencode::timestamp();
    run.directory = store.execution_directory(first_task).unwrap().to_string_lossy().into();
    std::fs::create_dir_all(&run.directory).unwrap();
    let output = serde_json::json!({"status":"complete","report":"Paused recovered evidence","sources":[{"title":"Paused source","url":"https://example.com/paused-recovery","published_at":"unknown"}]}).to_string();
    let transcript = serde_json::json!({"messages":[
        {"type":"user","text":run.prompt},
        {"type":"assistant","content":[{"type":"text","text":output}]},
        {"type":"idle","outcome":"succeeded"}
    ]});
    std::fs::write(PathBuf::from(&run.directory).join("session-export.json"), transcript.to_string()).unwrap();
    let run_id = run.id;
    store.runs.push(run);
    store.occurrence_mut(occurrence).unwrap().run_ids.push(run_id);
    // User pauses while the current period is running; restart must preserve that.
    store.archive_automation(id, now).unwrap();
    assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::Archived);
    assert_eq!(store.automation(id).unwrap().cursor, None);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = Scope::Automations;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx)
                    .store
                    .occurrence(occurrence)
                    .map(|value| value.status)
                    .unwrap()
            })
            .unwrap();
        if status.is_terminal() {
            assert_eq!(status, OccurrenceStatus::Succeeded);
            break;
        }
        assert!(std::time::Instant::now() < deadline, "archived recovery timed out in {status:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    let (status, cursor, reason) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (definition.status, definition.cursor, definition.attention_reason.clone())
    });
    assert_eq!(status, DefinitionStatus::Archived);
    assert_eq!(cursor, None);
    assert!(reason.is_none());
    let before = read_state(&mut cx, &app, |a| a.store.automation_occurrences(id).len());
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let now = chrono::Utc::now().timestamp() + 30 * 86_400;
            a.poll_automations(cx);
            let _ = now;
        });
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation_occurrences(id).len()),
        before,
        "archived automation must not create another period"
    );
    println!("archived recovery chain passed (archive survives restart verification)");
}

/// A regeneration that was running when the app stopped must not import a late
/// completed result after its own deadline, and must release the recovery slot.
fn regeneration_recovery_chain(root: &Path) {
    let directory = root.join("regeneration-recovery");
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys
args=sys.argv[1:]
if args[:2]==['api','session.active']:
    print(json.dumps({'data':{}}));sys.exit(0)
if args[:2]==['session','export']:
    print(open('session-export.json').read());sys.exit(0)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'LATE REGENERATION','sources':[]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let id = draft(&mut store, "Regeneration recovery");
    enable(&mut store, id);
    let now = now();
    store.automation_mut(id).unwrap().anchor = Some(now - 720);
    store.automation_mut(id).unwrap().cursor = Some(now - 120);
    let plans = store.plan_automation_ticks(now);
    let commit = store.commit_automation_tick(&plans[0], now).unwrap();
    let occurrence = commit.queued[0];
    store.occurrence_mut(occurrence).unwrap().mark_running(now - 60).unwrap();
    let mut runner = crate::automation_runtime::FakeAutomationRunner::default();
    runner.complete(&mut store, occurrence, now).unwrap();
    assert_eq!(
        store.occurrence(occurrence).unwrap().status,
        OccurrenceStatus::Succeeded
    );
    let source = store.occurrence(occurrence).unwrap().final_run_id.unwrap();
    let source_output = store
        .runs
        .iter()
        .find(|run| run.id == source)
        .map(|run| run.output.clone())
        .unwrap_or_default();
    let mut regen = crate::research::prepare_report_regeneration(&store, source, "late").unwrap();
    let task = regen.task_id;
    regen.automation_occurrence_id = Some(occurrence);
    regen.deadline_at = Some(now - 1);
    regen.status = "Running".into();
    regen.session_id = Some("ses_late_regeneration".into());
    regen.started_at = crate::opencode::timestamp();
    regen.directory = store.execution_directory(task).unwrap().to_string_lossy().into();
    std::fs::create_dir_all(&regen.directory).unwrap();
    let output = serde_json::json!({"status":"complete","report":"LATE REGENERATION","sources":[]}).to_string();
    let transcript = serde_json::json!({"messages":[
        {"type":"user","text":regen.prompt},
        {"type":"assistant","content":[{"type":"text","text":output}]},
        {"type":"idle","outcome":"succeeded"}
    ]});
    std::fs::write(PathBuf::from(&regen.directory).join("session-export.json"), transcript.to_string()).unwrap();
    let regen_id = regen.id;
    store.runs.push(regen);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                let a = app.read(cx);
                a.store
                    .runs
                    .iter()
                    .find(|run| run.id == regen_id)
                    .is_some_and(|run| run.status == "Cancelled")
                    && !a.active_runs.iter().any(|run| run.id == regen_id)
                    && !a.queued_runs.iter().any(|(id, _)| *id == regen_id)
                    && !a.store.runs.iter().any(|run| run.status == "Recovering")
            })
            .unwrap();
        if done { break; }
        assert!(
            std::time::Instant::now() < deadline,
            "expired recovered regeneration did not terminate"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let (final_run, status, source_after, regen_output) = read_state(&mut cx, &app, |a| {
        (
            a.store.occurrence(occurrence).unwrap().final_run_id,
            a.store.occurrence(occurrence).unwrap().status,
            a.store.runs.iter().find(|run| run.id == source).map(|run| run.output.clone()),
            a.store.runs.iter().find(|run| run.id == regen_id).map(|run| run.output.clone()),
        )
    });
    assert_eq!(final_run, Some(source));
    assert_eq!(status, OccurrenceStatus::Succeeded);
    assert_eq!(source_after.as_deref(), Some(source_output.as_str()));
    assert!(!regen_output.unwrap_or_default().contains("LATE REGENERATION"));
    println!("regeneration recovery chain passed (late result discarded, slot released)");
}

fn entry_route_check(root: &Path) {
    let directory = root.join("entry-route");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = model_store(&directory);
    store.default_model = "qa/automation".into();
    let id = draft(&mut store, "Entry route");
    enable(&mut store, id);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.automation_fake_mode = true;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let (occurrence, task) = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let now = chrono::Utc::now().timestamp();
                let occurrence = a
                    .store
                    .request_manual_occurrence(id, "entry-route", now)
                    .unwrap();
                let goal = occurrence.research_goal_id.unwrap();
                let task = a.store.research_children(goal)[0];
                a.start_run(task, cx);
                (occurrence.id, task)
            })
        })
        .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.occurrence(occurrence).unwrap().status),
        OccurrenceStatus::Succeeded
    );
    assert!(read_state(&mut cx, &app, |a| a
        .automation_fake
        .as_ref()
        .is_some_and(|runner| runner.starts.len() == 1)));
    assert!(read_state(&mut cx, &app, |a| a.store.runs.iter().all(|run| {
        run.task_id != task || run.automation_occurrence_id == Some(occurrence)
    })));
    println!("step entry routing passed (ordinary entry routes through the occurrence)");
}

/// A started instance must finish its full chain even when a newer instance is
/// already waiting at the shared queue front.
fn queue_chain(root: &Path) {
    let directory = root.join("queue-chain");
    let project = directory.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,sys,time
prompt=sys.argv[-1]
if '队列 A1' in prompt and 'Collect current evidence' in prompt:
    time.sleep(2.2)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Queue chain report','sources':[{'title':'Queue source','url':'https://example.com/queue-chain','published_at':'unknown'}]})}}),flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut store = model_store(&project);
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/automation".into();
    store.research_config.models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
        timezone: "Asia/Shanghai".into(),
    };
    let mut make = |name: &str, objective: &str| {
        let template = crate::automation_execution::build_template(
            &format!("{objective} 昨天的公开信息"),
            "简报",
        )
        .unwrap();
        let id = store
            .create_automation_draft(
                1,
                crate::automation::AutomationShape::Research,
                name.into(),
                format!("{objective} 昨天的公开信息"),
                "简报".into(),
                schedule.clone(),
                Default::default(),
                Some(template),
                now(),
            )
            .unwrap();
        enable(&mut store, id);
        id
    };
    let a1 = make("Queue A1", "队列 A1");
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.automation_fake_mode = false;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let first = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let now = chrono::Utc::now().timestamp();
                let occurrence = a
                    .store
                    .request_manual_occurrence(a1, "queue-a1", now)
                    .unwrap();
                a.enqueue_automation_occurrence(occurrence.id, cx);
                occurrence.id
            })
        })
        .unwrap();
    let running_deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let running = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).active_runs.len() == 1
            })
            .unwrap();
        if running {
            break;
        }
        assert!(
            std::time::Instant::now() < running_deadline,
            "A1 collection did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let second = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let now = chrono::Utc::now().timestamp();
                let occurrence = a
                    .store
                    .request_manual_occurrence(a1, "queue-a2", now)
                    .unwrap();
                a.enqueue_automation_occurrence(occurrence.id, cx);
                occurrence.id
            })
        })
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(40);
    loop {
        let(status1, status2) = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                let a = app.read(cx);
                (
                    a.store.occurrence(first).unwrap().status,
                    a.store.occurrence(second).unwrap().status,
                )
            })
            .unwrap();
        if status1.is_terminal() && status2.is_terminal() {
            assert_eq!(status1, OccurrenceStatus::Succeeded, "A1 chain");
            assert_eq!(status2, OccurrenceStatus::Succeeded, "A2 chain");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "queue chain timed out: A1 {status1:?}, A2 {status2:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .goal_tasks_complete(
            a.store.occurrence(first).unwrap().research_goal_id.unwrap()
        )));
    let (first_end, second_start) = read_state(&mut cx, &app, |a| {
        let first_occurrence = a.store.occurrence(first).unwrap();
        let second_occurrence = a.store.occurrence(second).unwrap();
        let first_end = first_occurrence
            .final_run_id
            .and_then(|id| a.store.runs.iter().find(|run| run.id == id))
            .and_then(|run| run.ended_at.clone())
            .unwrap_or_default();
        let second_start = second_occurrence
            .run_ids
            .first()
            .and_then(|id| a.store.runs.iter().find(|run| run.id == *id))
            .map(|run| run.started_at.clone())
            .unwrap_or_default();
        (first_end, second_start)
    });
    assert!(
        !first_end.is_empty() && !second_start.is_empty() && first_end <= second_start,
        "A2 must not start before A1 finishes: A1 end {first_end:?}, A2 start {second_start:?}"
    );
    println!("queue chain passed (same-automation A2 waits, A1 completes, then A2)");
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, directory: &Path, name: &str) {
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(40));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}

fn read_state<T>(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    read: impl FnOnce(&TaskApp) -> T,
) -> T {
    cx.update(|cx| read(app.read(cx)))
}

/// Full interaction pass. Returns the persisted workspace used for theme/size screenshots.
fn ui_interaction(root: &Path) -> PathBuf {
    let directory = root.join("ui");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let store = model_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.available_models = vec!["qa/automation".into()];
                a.automation_fake_mode = true;
                a.scope = Scope::Automations;
                a.automation_ui.selected = None;
                a.automation_ui.creating = false;
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    screenshot(&mut cx, handle, &directory, "00-empty-list.png");

    let long_name = "每日行业简报与异常变化追踪（面向非技术读者，包含来源、时间范围和证据缺口）";
    cx.update_window(handle, |_, w, cx| {
        w.click("new-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "00b-shape-choice.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-name", cx);
        w.input(long_name, cx);
        w.click("automation-objective", cx);
        w.input("追踪公开信息中值得注意的变化，说明来源、时间范围和不确定性。", cx);
        w.click("automation-expected", cx);
        w.input("一份三分钟可读完、带来源和证据缺口的简报。", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_ui.step), 2);
    screenshot(&mut cx, handle, &directory, "01-schedule-preview.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_ui.step), 3);
    screenshot(&mut cx, handle, &directory, "02-plan-review.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (id, version, status, cursor) = read_state(&mut cx, &app, |a| {
        let id = a.automation_ui.selected.expect("enabled automation selected");
        let definition = a.store.automation(id).unwrap();
        (
            id,
            definition.version,
            definition.status,
            definition.cursor,
        )
    });
    assert_eq!(status, DefinitionStatus::Enabled);
    assert!(cursor.is_some());
    assert_eq!(version, 1);
    assert!(read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().approval_current()));

    // A failed storage transaction must not create an occurrence, advance the cursor
    // or start the fake runner; the same due point still runs after recovery.
    let now = now();
    let blocked = directory.join("blocked-tick");
    std::fs::write(&blocked, "not a directory").unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let definition = a.store.automation_mut(id).unwrap();
            definition.anchor = Some(now - 660);
            definition.cursor = Some(now - 60);
            a.storage = Some(Storage::new(blocked.join("workspace.sqlite3")));
            a.storage_error.clear();
            a.poll_automations(cx);
        });
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation_occurrences(id).len()),
        0,
        "failed scheduling transaction must not create an occurrence"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().cursor),
        Some(now - 60),
        "failed scheduling transaction must not advance the cursor"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_fake.as_ref().map_or(0, |runner| runner.starts.len())),
        0,
        "failed scheduling transaction must not dispatch"
    );
    assert!(read_state(&mut cx, &app, |a| !a.storage_error.is_empty()));
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.storage = Some(Storage::new(path.clone()));
            a.storage_error.clear();
            a.poll_automations(cx);
        });
    })
    .unwrap();
    let occurrence = read_state(&mut cx, &app, |a| {
        a.store
            .automation_occurrences(id)
            .into_iter()
            .max_by_key(|o| o.id)
            .map(|o| o.id)
            .unwrap()
    });
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.occurrence(occurrence).unwrap().status),
        OccurrenceStatus::Succeeded
    );
    assert!(read_state(&mut cx, &app, |a| a.store.occurrence(occurrence).unwrap().unread));
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_fake.as_ref().unwrap().starts.len()),
        1
    );
    // Clicking a history row opens the result in a dialog and marks it read.
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "automation-detail-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
            cx,
        );
        w.render_frame(cx);
        let open = format!("open-occurrence-{occurrence}");
        w.click(ElementId::Name(open.into()), cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.result_dialog),
        Some(occurrence)
    );
    assert!(!read_state(&mut cx, &app, |a| a.store.occurrence(occurrence).unwrap().unread));
    screenshot(&mut cx, handle, &directory, "03-result.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("close-automation-result", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_ui.result_dialog), None);

    // Editing creates a new version and requires a new approval. Edit is opened
    // from the settings dialog, next to the settings it changes.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-settings", cx);
        w.render_frame(cx);
        w.click("edit-automation", cx);
        w.render_frame(cx);
        w.click("automation-name", cx);
        w.input("（第二版）", cx);
        w.click("automation-objective", cx);
        w.input("补充：记录证据缺口。", cx);
        w.render_frame(cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (version, status, name) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (definition.version, definition.status, definition.name.clone())
    });
    assert_eq!(version, 2);
    assert_eq!(status, DefinitionStatus::Enabled);
    assert!(name.ends_with("（第二版）"), "{name}");

    // Pause, resume, manual run, stop current period, copy and archive.
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("pause-automation", cx); }).unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().status),
        DefinitionStatus::Paused
    );
    assert_eq!(read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().cursor), None);
    // The next-run area lives in the Settings dialog; open it to check that a
    // paused automation marks it instead of listing triggers.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-settings", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("next-runs-paused").visible());
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "04a-paused.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("close-automation-settings", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("resume-automation", cx); }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.try_find("next-runs-paused").is_none());
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().status),
        DefinitionStatus::Enabled
    );
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("run-automation", cx); }).unwrap();
    assert!(read_state(&mut cx, &app, |a| {
        a.store
            .automation_occurrences(id)
            .iter()
            .any(|o| o.source == crate::automation::TriggerSource::Manual
                && o.status == OccurrenceStatus::Succeeded)
    }));
    // Hold a fake run so the Stop action can be exercised.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| a.automation_fake_hold = true);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("run-automation", cx); }).unwrap();
    let running = read_state(&mut cx, &app, |a| {
        a.store.automation_active(id).map(|o| o.id)
    });
    let running = running.expect("held fake run is active");
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.occurrence(running).unwrap().status),
        OccurrenceStatus::Running
    );
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("stop-automation", cx); }).unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.occurrence(running).unwrap().status),
        OccurrenceStatus::Cancelled
    );
    assert!(read_state(&mut cx, &app, |a| {
        a.automation_fake
            .as_ref()
            .is_some_and(|runner| runner.cancels.contains(&running))
    }));
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| a.automation_fake_hold = false);
    })
    .unwrap();

    // Copy and Archive are not on the detail action row for now; the workflows
    // they will own are still exercised directly.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.copy_automation_ui(id, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    let copy = read_state(&mut cx, &app, |a| a.automation_ui.selected.unwrap());
    assert_ne!(copy, id);
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(copy).unwrap().status),
        DefinitionStatus::Draft
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.archive_automation_ui(copy, cx));
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(copy).unwrap().status),
        DefinitionStatus::Archived
    );

    // Invalid form cannot enable.
    cx.update_window(handle, |_, w, cx| {
        w.click("new-automation", cx);
        w.click("automation-name", cx);
        w.input("x", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_ui.step), 1);
    assert!(read_state(&mut cx, &app, |a| a.automation_ui.feedback.is_some()));

    // Storage failure never reports Saved/Enabled.
    let blocked = directory.join("blocked");
    std::fs::write(&blocked, "file blocks directory creation").unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-objective", cx);
        w.input("测试存储失败时不能显示已保存。", cx);
        w.click("automation-expected", cx);
        w.input("保留草稿并显示错误。", cx);
        app.update(cx, |a, _| {
            a.storage = Some(Storage::new(blocked.join("workspace.sqlite3")));
            a.storage_error.clear();
        });
        w.render_frame(cx);
        w.click("save-automation-draft", cx);
    })
    .unwrap();
    assert!(read_state(&mut cx, &app, |a| !a.storage_error.is_empty()));
    assert!(read_state(&mut cx, &app, |a| matches!(
        a.automation_ui.feedback,
        Some(Err(_))
    )));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.storage = Some(Storage::new(path.clone()));
            a.storage_error.clear();
            a.open_automation(id, w, cx);
            a.automation_ui.feedback = None;
            cx.notify();
        });
    })
    .unwrap();

    // The store must still hold the real enabled version after the failed save.
    let saved = Storage::new(path.clone()).load().unwrap().unwrap();
    assert_eq!(saved.automation(id).unwrap().status, DefinitionStatus::Enabled);
    assert!(saved.automation(id).unwrap().approval_current());
    screenshot(&mut cx, handle, &directory, "04-detail.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.feedback = None;
            a.error.clear();
            a.toast = "Automations".into();
            a.open_automation(id, w, cx);
            cx.notify();
        });
        w.render_frame(cx);
        assert_ne!(
            w.find("run-automation").disabled(),
            Some(true),
            "an approved automation exposes the manual trigger"
        );
        assert!(
            w.try_find("automation-objective").is_none(),
            "settings stay out of the page while periods and results are the primary content"
        );
        assert!(
            w.try_find("automation-occurrence-detail").is_none(),
            "results stay closed until a history row is opened"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "05-detail-enabled.png");
    // Settings open in a modal that also owns Edit and Replan.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-settings", cx);
        w.render_frame(cx);
        assert!(
            w.find("edit-automation").visible(),
            "Edit lives in the settings dialog"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "05b-detail-settings.png");
    // Reading text opts into window text selection, so a drag selects it and
    // Cmd+C copies the selection.
    cx.update_window(handle, |_, w, cx| {
        let bounds = w.find("automation-objective").bounds();
        let start = point(bounds.origin.x + px(2.), bounds.origin.y + px(8.));
        let end = point(bounds.right() - px(2.), bounds.origin.y + px(8.));
        w.drag(start, end, cx);
        let selected = gpui_kit::base::TextSelection::selected_text(w, cx);
        assert!(
            selected.contains('追'),
            "the objective should be selectable, got {selected:?}"
        );
        w.press("cmd-c", cx);
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        assert!(
            copied.contains('追'),
            "Cmd+C should copy the selection, got {copied:?}"
        );
    })
    .unwrap();

    // Back from Edit returns to this automation with Settings open, not to All.
    cx.update_window(handle, |_, w, cx| {
        w.click("edit-automation", cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope.clone()),
        Scope::Automations
    );
    assert!(!read_state(&mut cx, &app, |a| a.automation_ui.creating));
    assert!(read_state(&mut cx, &app, |a| a.automation_ui.settings_dialog));
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.selected),
        Some(id)
    );

    let (automation_run, automation_task) = read_state(&mut cx, &app, |a| {
        a.store
            .runs
            .iter()
            .rev()
            .find(|run| run.automation_occurrence_id.is_some())
            .map(|run| (run.id, run.task_id))
            .expect("the automation has a run to open")
    });
    // Switching to Progress & results must not keep a period step selected; the
    // ordinary results view falls back to its own latest run.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.selected_run = Some(automation_run);
            a.task_result_context = Some(automation_task);
            a.navigate(Scope::Runs, w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.selected_run), None);
    assert_eq!(read_state(&mut cx, &app, |a| a.task_result_context), None);

    // Opening one automation step from its period still shows only that step:
    // automation periods have no project/topic history to fill the column.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.task_result_context = Some(automation_task);
            a.scope = Scope::Runs;
            a.selected_run = Some(automation_run);
            a.show_run_details = false;
            a.show_run_result = false;
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| {
        assert!(
            w.try_find("run-navigation").is_none(),
            "an automation run must not show an empty project history column"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "06-results-blank-column.png");

    // Automation results are a separate sidebar section from research results.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-results", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope.clone()),
        Scope::AutomationRuns
    );
    assert_eq!(read_state(&mut cx, &app, |a| a.title()), "Automation results");
    let latest = read_state(&mut cx, &app, |a| {
        a.store
            .automation_occurrences
            .iter()
            .map(|occurrence| occurrence.id)
            .max()
            .unwrap()
    });
    cx.update_window(handle, |_, w, cx| {
        let open = format!("open-occurrence-{latest}");
        w.click(ElementId::Name(open.into()), cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.result_dialog),
        Some(latest)
    );
    screenshot(&mut cx, handle, &directory, "07-automation-results.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("close-automation-result", cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope.clone()),
        Scope::Automations
    );
    path
}

fn render_matrix(root: &Path, source: &Path) {
    let base = Storage::new(source.to_path_buf()).load().unwrap().unwrap();
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let label = format!(
                "{}-{}",
                if mode == Appearance::Light { "light" } else { "dark" },
                width as u32
            );
            let directory = root.join(&label);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.sqlite3");
            let mut store = base.clone();
            store.appearance = mode;
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
                        a.automation_fake_mode = true;
                        a.scope = Scope::Automations;
                        a.automation_ui.selected = None;
                        cx.notify();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle: AnyWindowHandle = window.into();
            screenshot(&mut cx, handle, &directory, "list.png");
            let id = read_state(&mut cx, &app, |a| a.store.automations[0].id);
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.automation_ui.selected = Some(id);
                    cx.notify();
                });
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "detail.png");
        }
    }
}

/// 30 days of one-minute occurrences. Representative terminal runs carry reports,
/// sources and logs; skipped periods are summarized instead of being silently absent.
pub fn performance_fixture() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/automation-perf/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("workspace.sqlite3");
    let directory = root.join("project");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = model_store(&directory);
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 1 },
        timezone: "UTC".into(),
    };
    let template = crate::automation_execution::build_template("容量检查", "简短报告").unwrap();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "30 天分钟级容量检查".into(),
            "容量检查".into(),
            "简短报告".into(),
            schedule.clone(),
            Default::default(),
            Some(template.clone()),
            now(),
        )
        .unwrap();
    enable(&mut store, id);
    let start = now() - 30 * 86_400;
    let report = "## 容量样本\n\n".to_string() + &"这是用于测量列表、详情和保存开销的代表性报告段落。".repeat(120);
    for index in 0..43_200i64 {
        let at = start + index * 60;
        let key = crate::automation::scheduled_trigger_key(id, at);
        let status = match index % 10 {
            0 => OccurrenceStatus::Failed,
            1..=7 => OccurrenceStatus::Succeeded,
            _ => OccurrenceStatus::Skipped,
        };
        let mut occurrence = crate::automation::AutomationOccurrence {
            id: 0,
            automation_id: id,
            definition_version: 1,
            source: crate::automation::TriggerSource::Scheduled,
            trigger_key: key,
            scheduled_for: Some(at),
            requested_at: None,
            actual_started_at: Some(at + 5),
            timezone: "UTC".into(),
            reference_time: at,
            data_window: crate::automation::TimeWindow::resolved(
                crate::automation_schedule::DataWindow {
                    start_utc: at - 86_400,
                    end_utc: at,
                },
                "QA fixture previous day",
            ),
            snapshot: crate::automation::AutomationOccurrenceSnapshot {
                objective: "容量检查".into(),
                expected_result: "简短报告".into(),
                template: template.clone(),
                models: std::array::from_fn(|_| "qa/automation".to_string()),
                agents: Default::default(),
                materials: vec![],
                policy: Default::default(),
                approval_version: 1,
                approval_signature: "qa".into(),
            },
            status,
            reason: (status == OccurrenceStatus::Skipped).then(|| {
                crate::automation::OccurrenceReason::new(
                    crate::automation::SkipCode::Expired,
                    "Capacity fixture skipped period",
                )
            }),
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: status == OccurrenceStatus::Succeeded && index % 3 == 0,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: at,
            deadline_at: None,
        };
        occurrence.id = store.next_occurrence_id().unwrap();
        // The first 100 non-skipped periods carry complete internal research entities
        // and representative reports/logs; the rest measure scheduling-scale history.
        if index < 100 && status != OccurrenceStatus::Skipped {
            let mut materialized = store.materialize_occurrence(occurrence).unwrap();
            let goal = materialized.research_goal_id.unwrap();
            let mut run_ids = vec![];
            for task in store.research_children(goal) {
                let task_status = if status == OccurrenceStatus::Failed { "Failed" } else { "Succeeded" };
                let run = crate::domain::AgentRun {
                    automation_occurrence_id: Some(materialized.id),
                    deadline_at: None,
                    auto_retry_attempt: 0,
                    retry_not_before: None,
                    project_plan: None,
                    id: store.runs.iter().map(|run| run.id).max().unwrap_or(0) + 1,
                    task_id: task,
                    task_title: "容量样本".into(),
                    directory: directory.to_string_lossy().into(),
                    prompt: "QA performance fixture".into(),
                    started_at: chrono::DateTime::from_timestamp(at, 0).unwrap().to_rfc3339(),
                    queued_at: String::new(),
                    workspace_source: None,
                    ended_at: Some(chrono::DateTime::from_timestamp(at + 60, 0).unwrap().to_rfc3339()),
                    session_id: Some(format!("qa-session-{index}-{task}")),
                    exit_code: Some(0),
                    agent: "OpenCode".into(),
                    model: "qa/automation".into(),
                    stage: crate::domain::Stage::Done,
                    status: task_status.into(),
                    logs: (0..12)
                        .map(|line| format!("[log] representative automation log line {line}"))
                        .collect(),
                    output: report.clone(),
                    attachments: vec![],
                    research_kind: Some(crate::research::ResearchKind::Advise),
                    sources: vec![crate::research::Source {
                        title: "Capacity source".into(),
                        url: format!("https://example.com/capacity/{index}"),
                        published_at: "unknown".into(),
                    }],
                    input_signature: String::new(),
                };
                run_ids.push(run.id);
                store.runs.push(run);
            }
            materialized.status = status;
            materialized.unread = status == OccurrenceStatus::Succeeded;
            materialized.final_run_id = run_ids.last().copied();
            materialized.run_ids = run_ids;
            let materialized_id = materialized.id;
            *store.occurrence_mut(materialized_id).unwrap() = materialized;
        } else {
            store
                .record_occurrence_without_research(occurrence)
                .unwrap();
        }
    }
    let storage = Storage::new(path.clone());
    let cold_started = std::time::Instant::now();
    storage.save(&store).unwrap();
    let cold_save_ms = cold_started.elapsed().as_millis();
    let mut save_times = vec![];
    for _ in 0..3 {
        let started = std::time::Instant::now();
        storage.save(&store).unwrap();
        save_times.push(started.elapsed().as_millis());
    }
    save_times.sort_unstable();
    let load_started = std::time::Instant::now();
    let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
    let load_ms = load_started.elapsed().as_millis();
    assert_eq!(loaded.automation_occurrences.len(), 43_200);
    let list_started = std::time::Instant::now();
    let mut latest: Vec<_> = loaded.automation_occurrences.iter().collect();
    latest.sort_by_key(|occurrence| std::cmp::Reverse(occurrence.id));
    let first_page: Vec<_> = latest.into_iter().take(20).collect();
    let list_ms = list_started.elapsed().as_millis();
    assert_eq!(first_page.len(), 20);
    let db_bytes = std::fs::metadata(&path).unwrap().len();
    println!(
        "performance fixture: 43,200 occurrences (100 with full research entities) + representative reports; cold save {cold_save_ms} ms; warm saves {:?} ms; load {load_ms} ms; history page query {list_ms} ms; db {:.1} MB; directory {}",
        save_times,
        db_bytes as f64 / 1_048_576.0,
        root.display()
    );

    // Main-thread first paint and detail paint with the large history.
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let open_started = std::time::Instant::now();
    let mut app = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.scope = crate::app::Scope::Automations;
                a.automation_ui.selected = None;
                a.automation_fake_mode = true;
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let open_ms = open_started.elapsed().as_millis();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let render_started = std::time::Instant::now();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let list_render_ms = render_started.elapsed().as_millis();
    let detail_started = std::time::Instant::now();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.selected = Some(id);
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let detail_render_ms = detail_started.elapsed().as_millis();
    let save_started = std::time::Instant::now();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| {
            if let Some(definition) = a.store.automation_mut(id) {
                definition.name.push(' ');
            }
            a.persist_if_changed(true);
        });
    })
    .unwrap();
    let ui_save_block_ms = save_started.elapsed().as_millis();
    println!(
        "performance fixture UI: cold open incl. attach {open_ms} ms; automation list frame {list_render_ms} ms; detail frame {detail_render_ms} ms; synchronous UI-triggered save block {ui_save_block_ms} ms"
    );
}
