//! Offline automation acceptance: fake clock/runner, isolated SQLite workspace, no model calls.
use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::Appearance,
    automation::{AutomationShape, DefinitionStatus, OccurrenceStatus},
    domain::{Stage, Store},
    persistence::Storage,
};
use gpui_kit::{component::{Theme, ThemeMode}, test::TestWindowExt, *};
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
    if std::env::var_os("PLANNING_QA_SETTINGS_ONLY").is_some() {
        settings_hierarchy_fixture(&root);
        println!("Automation settings QA passed · {}", root.display());
        return;
    }
    if std::env::var_os("PLANNING_QA_LIST_ONLY").is_some() {
        list_hierarchy_fixture(&root);
        println!("Automation list QA passed · {}", root.display());
        return;
    }
    pure_checks(&root);
    instruction_validation_check(&root);
    planning_chain(&root);
    wizard_actions_check(&root);
    execution_chain(&root);
    recovery_chain(&root);
    pause_recovery_chain(&root);
    regeneration_recovery_chain(&root);
    archive_recovery_chain(&root);
    entry_route_check(&root);
    dispatch_boundary_check(&root);
    retry_scope_check(&root);
    activity_order_check(&root);
    ordinary_dispatch_boundary_check(&root);
    completion_boundary_check(&root);
    queue_chain(&root);
    independent_pools_check(&root);
    undo_protection_check(&root);
    draft_defaults_check(&root);
    shape_switch_check(&root);
    late_planning_completion_check(&root);
    planning_revision_completion_check(&root);
    planning_cli_flow_check(&root);
    shape_revision_atomic_check(&root);
    let snapshot = ui_interaction(&root);
    render_matrix(&root, &snapshot);
    println!("Automation QA passed · {}", root.display());
}

/// Focused T23/T25 planning ownership checks. Keeps the full automation matrix
/// available while the unrelated period/result work is being integrated.
pub fn run_planning() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/planning-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if std::env::var_os("PLANNING_QA_UNDERSTANDING_ONLY").is_some() {
        planning_cli_flow_check(&root);
        println!("Research understanding QA passed · {}", root.display());
        return;
    }
    pure_checks(&root);
    instruction_validation_check(&root);
    planning_chain(&root);
    late_planning_completion_check(&root);
    planning_revision_completion_check(&root);
    planning_cli_flow_check(&root);
    println!("Planning QA passed · {}", root.display());
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
                a.set_scope_identity(Scope::Automations);
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
    std::fs::create_dir_all(directory.join("automations")).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    store.research_config_mut().models =
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
        .create_automation_request(id, automation_project, "Track the semiconductor supply chain", "A sourced note")
        .unwrap();
    confirm_fixture_understanding(&mut store, request);
    let mut run = store.prepare_request_plan(request).unwrap();
    run.status = "Succeeded".into();
    run.ended_at = Some(crate::opencode::timestamp());
    run.output = r#"{"topic_name":"Semiconductor supply","brief":{"objective":"Track supply","deliverable":"Sourced note","scope":"Latest quarter","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Foundry capacity","brief":"Track announced capacity","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare capacity","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]},{"title":"Equipment orders","brief":"Track equipment bookings","tasks":[{"title":"Collect","instructions":"Collect orders","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare orders","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]}]}"#.into();
    let run_id = run.id;
    store.push_run(run);
    store.request_mut(request).unwrap()
        .planning_run_id = Some(run_id);
    let topics_before = store.topics().len();
    let tasks_before = store.tasks().len();
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
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(id);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.complete_research_run(run_id, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = _window.into();
    let (goals, name, status, topics, tasks, auto_dirs) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(id).unwrap();
        (
            definition.template.as_ref().map(|template| template.goals.len()),
            definition.name.clone(),
            definition.status,
            a.store.topics().len(),
            a.store.tasks().len(),
            a.store
                .automation(id)
                .map(|definition| definition.project_id)
                .and_then(|project| a.store.projects().iter().find(|p| p.id == project))
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
        !read_state(&mut cx, &app, |a| a.store.runs().iter().any(|candidate| {
            candidate.research_kind == Some(crate::research::ResearchKind::Goal)
                && candidate.task_id == 0
        })),
        "freezing the plan must not launch another planner"
    );
    // The attempt stays under its automation, never in ordinary Research results.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_settings_overlay();
            cx.notify();
        });
        w.render_frame(cx);
        let review = format!("planning-attempt-{run_id}");
        assert!(
            w.try_find(review.clone()).is_some(),
            "the owning automation lists its planning attempt"
        );
        w.click(review, cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
        Some(run_id)
    );
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.close_planning_attempt_overlay();
            a.navigate(Scope::Runs, w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    assert_ne!(
        read_state(&mut cx, &app, |a| a.displayed_run_id()),
        Some(run_id),
        "automation planning must not become the ordinary Research result"
    );
    println!("automation planning chain passed (reviewed plan frozen, no topic or replanning)");
}


fn confirm_fixture_understanding(store: &mut Store, request: u64) {
    store.apply_request_understanding(request,
        r#"{"summary":"Review the requested subject and deliver a sourced report within the stated scope.","questions":[]}"#,
    ).unwrap();
    store.confirm_request_understanding(request).unwrap();
}

fn model_store(directory: &Path) -> Store {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name = "Automation QA project".into();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/automation".into());
    store
}

fn draft(store: &mut Store, name: &str) -> u64 {
    let template = crate::automation_execution::build_template(
        "Track notable changes in public information, preserving sources and dates.",
        "A concise report with sources and evidence gaps.",
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
            "Track notable changes in public information, preserving sources and dates.".into(),
            "A concise report with sources and evidence gaps.".into(),
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
    let id = draft(&mut store, "Ten-minute check");
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
    assert_eq!(store.all_occurrences().len(), 2);

    // A fake runner records starts and data windows without a process.
    let mut runner = crate::automation_runtime::FakeAutomationRunner::default();
    runner.start(&store.occurrence(commit.skipped[0]).unwrap().clone());
    assert_eq!(runner.starts.len(), 1);
    assert!(!runner.starts[0].window_resolved, "a task without an explicit range must stay unresolved");

    // Large history remains bounded and readable.
    let mut history_store = model_store(&directory);
    let history_id = draft(&mut history_store, "Historical capacity check");
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
                    shape: Some(definition.shape),
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
            retry_requested_at: None,
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
            .all_occurrences()
            .iter()
            .take(20)
            .filter(|o| o.unread)
            .count(),
        0
    );
    println!("pure automation checks passed");
}

/// T18: creation-time input validation is intentionally not a runtime
/// capability boundary. The marker proves rejected input never dispatches, and
/// the benign execution chain proves allowed work still finishes.
fn instruction_validation_check(root: &Path) {
    let directory = root.join("instruction-validation");
    std::fs::create_dir_all(&directory).unwrap();
    let cli = fake_marker_cli(&directory);
    let marker = directory.join("invoked.log");
    let path = directory.join("workspace.sqlite3");

    // Save a legacy-shaped draft with unsupported stored text, reopen it from
    // SQLite, and prove structural readability plus atomic enable rejection.
    let mut store = model_store(&directory);
    store.set_opencode_path(cli.to_string_lossy().into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let template = crate::automation_execution::build_template(
        "Track public announcements",
        "A sourced note",
    )
    .unwrap();
    let id = store
        .create_automation_draft(
            1,
            AutomationShape::Research,
            "Legacy check".into(),
            "Track public announcements".into(),
            "A sourced note".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Daily { hour: 9, minute: 0 },
                timezone: "Asia/Shanghai".into(),
            },
            Default::default(),
            Some(template),
            now(),
        )
        .unwrap();
    {
        let mut definition = store.automation_mut(id).unwrap();
        definition.expected_result = "Send email the answer".into();
        definition.approval = None;
        definition.status = DefinitionStatus::Draft;
    }
    Storage::new(path.clone()).save(&store).unwrap();
    let mut reopened = Storage::new(path.clone()).load().unwrap().unwrap();
    assert_eq!(
        reopened.automation(id).unwrap().expected_result,
        "Send email the answer",
        "legacy contents must remain readable"
    );
    assert!(reopened.validate_automation_integrity().is_ok());
    let loaded = reopened.automation(id).unwrap().clone();
    let error = reopened
        .approve_and_enable(id, vec![], now())
        .expect_err("legacy unsupported text must not be enabled");
    assert!(error.starts_with("Expected result:"), "{error}");
    assert_eq!(reopened.automation(id).unwrap(), &loaded);
    assert!(reopened.automation(id).unwrap().approval.is_none());
    assert!(reopened.all_occurrences().is_empty());
    assert!(reopened.runs().is_empty());
    assert!(!marker.exists(), "rejected input must not invoke the agent");
    let after = Storage::new(path).load().unwrap().unwrap();
    assert_eq!(
        after.automation(id),
        Some(&loaded),
        "rejected enable must not rewrite or delete legacy contents"
    );

    expected_only_ui_check(
        &root.join("instruction-validation-quick"),
        AutomationShape::Quick,
    );
    expected_only_ui_check(
        &root.join("instruction-validation-research"),
        AutomationShape::Research,
    );
    println!("instruction validation passed (form, domain, storage, no dispatch)");
}

fn expected_only_ui_check(directory: &Path, shape: AutomationShape) {
    std::fs::create_dir_all(directory).unwrap();
    let cli = fake_marker_cli(directory);
    let marker = directory.join("invoked.log");
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(directory);
    store.set_opencode_path(cli.to_string_lossy().into());
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
                a.set_scope_identity(Scope::Automations);
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
        w.click("new-automation", cx);
        if shape == AutomationShape::Research {
            w.click("automation-shape-research", cx);
        }
        w.click("automation-objective", cx);
        w.input("Check public fuel prices in Shenzhen", cx);
        w.render_frame(cx);
        w.click_at("automation-expected", point(px(12.), px(12.)), cx);
        w.render_frame(cx);
        w.input("Send email the answer", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (objective_value, expected_value) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.automation_ui.objective.read(cx).value().to_string(),
            app.automation_ui.expected.read(cx).value().to_string(),
        )
    });
    assert_eq!(
        objective_value, "Check public fuel prices in Shenzhen",
        "{shape:?} objective field must receive the UI input"
    );
    assert_eq!(
        expected_value, "Send email the answer",
        "{shape:?} expected-result field must receive the UI input"
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (step, feedback, automations, runs) = read_state(&mut cx, &app, |a| {
        (
            a.automation_ui.step,
            a.automation_ui
                .feedback
                .as_ref()
                .and_then(|result| result.as_ref().err().cloned()),
            a.store.automations().len(),
            a.store.runs().len(),
        )
    });
    assert_eq!(step, 1, "{shape:?} must stay on the instruction step");
    let feedback = feedback.unwrap_or_else(|| panic!("{shape:?} must show an error"));
    assert!(feedback.starts_with("Expected result:"), "{feedback}");
    assert!(feedback.contains("input validation"), "{feedback}");
    assert_eq!(automations, 0, "{shape:?} rejection must not save a draft");
    assert_eq!(runs, 0, "{shape:?} rejection must not dispatch");
    assert!(!marker.exists(), "{shape:?} rejection must not invoke the agent");
    screenshot(&mut cx, handle, directory, "expected-only-rejected.png");
}

fn fake_marker_cli(directory: &Path) -> PathBuf {
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json, os
marker = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'invoked.log')
with open(marker, 'a') as handle:
    handle.write('run\n')
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Fixture report','sources':[]})}}), flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    cli
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
import json,sys,time,os
args=sys.argv[1:]
marker=os.path.join(os.path.dirname(os.path.abspath(__file__)),'invoked.log')
with open(marker,'a') as handle:
    handle.write('run\n')
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
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
                a.set_scope_identity(Scope::Automations);
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
                let runs = a.store.runs().iter().filter(|r| r.automation_occurrence_id == Some(occurrence)).map(|r| (r.id, r.task_id, r.status.clone(), r.logs.iter().rev().take(3).cloned().collect::<Vec<_>>())).collect::<Vec<_>>();
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
    let invoked = std::fs::read_to_string(directory.join("invoked.log"))
        .expect("the benign fake CLI should run the approved steps");
    assert!(
        invoked.lines().count() >= 3,
        "benign control should finish every approved step: {invoked:?}"
    );
    assert_eq!(run_count.len(), 3, "{run_count:?}");
    assert!(final_run.is_some());
    let sessions: std::collections::HashSet<_> = read_state(&mut cx, &app, |a| {
        a.store
            .runs()
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
            let task = a
                .store
                .runs()
                .iter()
                .find(|run| run.id == source)
                .map(|run| run.task_id)
                .unwrap();
            a.store
                .task_mut(task)
                .unwrap()
                .description
                .push_str("\nQA input changed after the saved report finished.");
            a.error.clear();
            a.regenerate_report(source, "Make the explanation clearer", cx);
        });
    })
    .unwrap();
    let regeneration_deadline = std::time::Instant::now() + Duration::from_secs(20);
    let regenerated = loop {
        let found = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx).store.runs().iter()
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
                    a.store.runs().iter().filter(|run| run.automation_occurrence_id == Some(occurrence)).map(|run| (run.id, run.task_id, run.status.clone(), run.output.len(), run.logs.iter().rev().take(3).cloned().collect::<Vec<_>>())).collect::<Vec<_>>(),
                    a.queued_runs.len(),
                    a.active_runs.len(),
                )
            });
            panic!("automation report regeneration did not finish: {debug:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (final_run_id, regenerated_model, regenerated_dir, historical_regeneration) =
        read_state(&mut cx, &app, |a| {
        let run = a.store.runs().iter().find(|run| run.id == regenerated).unwrap();
        (
            a.store.occurrence(occurrence).unwrap().final_run_id,
            run.model.clone(),
            run.directory.clone(),
            crate::research::is_historical_report_regeneration(run),
        )
    });
    assert_eq!(final_run_id, Some(regenerated));
    assert!(historical_regeneration);
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
            .runs()
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
                    .runs()
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
            a.store.runs().iter().find(|run| run.id == timeout_run).map(|run| run.status.clone()),
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
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
    store.push_run(run);
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
                a.set_scope_identity(Scope::Automations);
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
                .runs()
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
                {
                    let mut definition = a.store.automation_mut(id).unwrap();
                    definition.anchor = Some(next_now - 60);
                    definition.cursor = Some(next_now);
                }
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
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
    store.push_run(run);
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
                a.set_scope_identity(Scope::Automations);
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
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
    store.push_run(run);
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
                a.set_scope_identity(Scope::Automations);
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
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
        .runs()
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
    store.push_run(regen);
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
                    .runs()
                    .iter()
                    .find(|run| run.id == regen_id)
                    .is_some_and(|run| run.status == "Cancelled")
                    && !a.active_runs.iter().any(|run| run.id == regen_id)
                    && !a.queued_runs.iter().any(|(id, _)| *id == regen_id)
                    && !a.store.runs().iter().any(|run| run.status == "Recovering")
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
            a.store.runs().iter().find(|run| run.id == source).map(|run| run.output.clone()),
            a.store.runs().iter().find(|run| run.id == regen_id).map(|run| run.output.clone()),
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
    store.set_default_model("qa/automation".into());
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
    assert!(read_state(&mut cx, &app, |a| a.store.runs().iter().all(|run| {
        run.task_id != task || run.automation_occurrence_id == Some(occurrence)
    })));
    println!("step entry routing passed (ordinary entry routes through the occurrence)");
}

#[derive(Clone, Debug, PartialEq)]
struct DispatchUiState {
    scope: Scope,
    layout: Layout,
    selected: Option<u64>,
    selected_run: Option<u64>,
    automation_selected: Option<u64>,
    creating: bool,
    step: u8,
    result_dialog: Option<u64>,
    settings_dialog: bool,
    objective: String,
    follow_logs: bool,
    show_run_result: bool,
    show_run_details: bool,
    detail_scroll: gpui_kit::Point<gpui_kit::Pixels>,
    result_scroll: gpui_kit::Point<gpui_kit::Pixels>,
    automation_scroll: gpui_kit::Point<gpui_kit::Pixels>,
}

fn dispatch_ui_state(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
) -> DispatchUiState {
    cx.update(|cx| {
        let a = app.read(cx);
        DispatchUiState {
            scope: a.scope(),
            layout: a.layout,
            selected: a.selected(),
            selected_run: a.selected_run(),
            automation_selected: a.automation_ui.selected,
            creating: a.automation_ui.creating,
            step: a.automation_ui.step,
            result_dialog: a.period_overlay(),
            settings_dialog: a.settings_overlay(),
            objective: a.automation_ui.objective.read(cx).value().to_string(),
            follow_logs: a.follow_logs,
            show_run_result: a.show_run_result,
            show_run_details: a.show_run_details,
            detail_scroll: a.detail_scroll.offset(),
            result_scroll: a.result_scroll.offset(),
            automation_scroll: a.automation_ui.scroll.offset(),
        }
    })
}

/// Background dispatch never takes over the page. Cover settings, an existing
/// report, a dirty creation draft and an open period dialog while a scheduled
/// three-step period starts and advances twice.
fn dispatch_boundary_check(root: &Path) {
    for surface in ["settings", "report", "dirty-form", "period-dialog"] {
        let directory = root.join(format!("dispatch-{surface}"));
        let project = directory.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let cli = directory.join("fake-opencode");
        std::fs::write(
            &cli,
            r#"#!/usr/bin/python3
import json
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Dispatch boundary report','sources':[{'title':'Fixture source','url':'https://example.com/dispatch-boundary','published_at':'unknown'}]})}}),flush=True)
"#,
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut store = model_store(&project);
        store.set_opencode_path(cli.to_string_lossy().into());
        store.set_default_model("fixture/automation".into());
        store.research_config_mut().models =
            ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"]
                .map(String::from);
        let id = draft(&mut store, &format!("Dispatch boundary {surface}"));
        enable(&mut store, id);

        let report_goal = store
            .create_research_goal(
                "Existing report".into(),
                "Show an existing completed result".into(),
                1,
            )
            .unwrap();
        let report_steps = store
            .apply_research_plan(
                report_goal,
                r#"{"tasks":[{"title":"Existing collect","instructions":"Gather","kind":"collect","depends_on":[]},{"title":"Existing analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Existing report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#,
            )
            .unwrap();
        let mut report_run = store.prepare_run(report_steps[0]).unwrap();
        report_run.status = "Succeeded".into();
        report_run.output = "Existing report body".into();
        let report_run_id = report_run.id;
        store.push_run(report_run);
        store.set_stage(report_steps[0], Stage::Done).unwrap();

        let history = store
            .request_manual_occurrence(id, &format!("previous-{surface}"), now() - 120)
            .unwrap();
        store.stop_occurrence(history.id, now() - 110).unwrap();

        let stamp = now();
        store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 1;
        store.automation_mut(id).unwrap().policy.timeout_seconds = 120;
        store.automation_mut(id).unwrap().anchor = Some(stamp - 600);
        store.automation_mut(id).unwrap().cursor = Some(stamp);
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
                    a.set_scope_identity(Scope::Automations);
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
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                match surface {
                    "settings" => a.set_scope_identity(Scope::Settings),
                    "report" => {
                        a.set_scope_identity(Scope::Runs);
                        a.fixture_select_task(None);
                        a.note_selected_run(Some(report_run_id));
                        a.show_run_result = true;
                        a.show_run_details = false;
                    }
                    "dirty-form" => {
                        a.set_scope_identity(Scope::Automations);
                        a.fixture_select_task(None);
                        a.automation_ui.selected = None;
                        a.automation_ui.creating = true;
                        a.automation_ui.step = 2;
                        a.automation_ui.objective.update(cx, |state, cx| {
                            state.set_value("Unsaved dispatch draft", w, cx);
                            state.focus(w, cx);
                        });
                    }
                    "period-dialog" => {
                        a.set_scope_identity(Scope::Automations);
                        a.fixture_select_task(None);
                        a.automation_ui.selected = Some(id);
                        a.push_period_overlay(history.id);
                    }
                    _ => {}
                }
                match surface {
                    "dirty-form" => {}
                    "period-dialog" => w.focus(&a.modal_focus, cx),
                    _ => w.focus(&a.focus, cx),
                }
                cx.notify();
            });
            w.render_frame(cx);
        })
        .unwrap();
        let before = dispatch_ui_state(&mut cx, &app);
        let focus_before = cx
            .update_window(handle, |_, w, cx| w.focused(cx))
            .unwrap();

        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| a.poll_automations(cx));
        })
        .unwrap();
        let occurrence = read_state(&mut cx, &app, |a| {
            a.store
                .all_occurrences()
                .iter()
                .rev()
                .find(|occurrence| {
                    occurrence.automation_id == id
                        && occurrence.trigger_key.starts_with("scheduled:")
                })
                .map(|occurrence| occurrence.id)
                .expect("scheduled occurrence")
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let terminal = cx
                .update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| a.poll_runtime(cx));
                    w.render_frame(cx);
                    app.read(cx)
                        .store
                        .occurrence(occurrence)
                        .is_some_and(|occurrence| occurrence.status.is_terminal())
                })
                .unwrap();
            assert_eq!(
                dispatch_ui_state(&mut cx, &app),
                before,
                "background dispatch changed the visible state from {surface}"
            );
            assert_eq!(
                cx.update_window(handle, |_, w, cx| w.focused(cx))
                    .unwrap(),
                focus_before,
                "background dispatch moved focus from {surface}"
            );
            if terminal {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "scheduled dispatch did not finish from {surface}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let (status, run_ids, goal) = read_state(&mut cx, &app, |a| {
            let occurrence = a.store.occurrence(occurrence).unwrap();
            (occurrence.status, occurrence.run_ids.clone(), occurrence.research_goal_id)
        });
        assert_eq!(status, OccurrenceStatus::Succeeded);
        assert_eq!(run_ids.len(), 3, "two subsequent steps must run: {run_ids:?}");
        // Explicit View progress opens the period that owns the step.
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                assert!(a.open_automation_progress_for_goal(goal.unwrap(), w, cx));
                assert_eq!(a.scope(), Scope::Automations);
                assert_eq!(a.period_overlay(), Some(occurrence));
            });
        })
        .unwrap();

        if surface == "settings" {
            // Ordinary manual submission still reaches its progress surface.
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.set_scope_identity(Scope::Settings);
                    a.fixture_select_task(None);
                    a.note_selected_run(None);
                    a.start_run(report_steps[1], cx);
                });
            })
            .unwrap();
            let manual_run = read_state(&mut cx, &app, |a| {
                assert_eq!(a.scope(), Scope::Runs);
                a.selected_run().expect("manual submission opens its run")
            });
            assert!(
                read_state(&mut cx, &app, |a| a
                    .store
                    .runs()
                    .iter()
                    .any(|run| run.id == manual_run && run.task_id == report_steps[1])),
                "manual submission queues the requested step"
            );
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.cancel_run_id(manual_run, cx));
            })
            .unwrap();
        }
        println!("dispatch boundary passed from {surface}");
    }
}

/// Retrying a failed period keeps the paused definition and its schedule state
/// untouched; this exercises the app handler that previously resumed the schedule.
fn retry_scope_check(root: &Path) {
    let directory = root.join("retry-scope");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = model_store(&directory);
    let id = draft(&mut store, "Retry scope");
    enable(&mut store, id);
    let stamp = now();
    let occurrence = store
        .request_manual_occurrence(id, "retry-scope", stamp)
        .unwrap();
    let goal = occurrence.research_goal_id.unwrap();
    let mut first = store.prepare_occurrence_run(&occurrence, stamp).unwrap();
    first.status = "Succeeded".into();
    first.output = "Collected evidence".into();
    let collect = first.task_id;
    store.push_run(first);
    store.set_stage(collect, Stage::Done).unwrap();
    let mut second = store.prepare_occurrence_run(&occurrence, stamp + 10).unwrap();
    second.status = "Failed".into();
    store.push_run(second);
    store
        .fail_occurrence(occurrence.id, "step failed", stamp + 20)
        .unwrap();
    store.pause_automation(id, stamp + 30).unwrap();
    // A later edit moves the definition to a new Draft version. The historical
    // period still retries with its own approved snapshot.
    let revised = crate::automation_execution::build_template(
        "Revised objective",
        "A revised result",
    )
    .unwrap();
    store
        .revise_automation(
            id,
            1,
            "Retry scope revised".into(),
            "Revised objective".into(),
            "A revised result".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 10 },
                timezone: "Asia/Shanghai".into(),
            },
            Default::default(),
            Some(revised),
            stamp + 40,
        )
        .unwrap();
    assert_eq!(store.automation(id).unwrap().version, 2);
    assert_eq!(
        store.automation(id).unwrap().status,
        DefinitionStatus::Draft
    );
    let definition_before = store.automation(id).unwrap().clone();
    let reference = store.occurrence(occurrence.id).unwrap().reference_time;
    let snapshot_version = store
        .occurrence(occurrence.id)
        .unwrap()
        .snapshot
        .approval_version;
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
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(id);
                a.push_period_overlay(occurrence.id);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    // Click the actual period action after the definition edit. The button is
    // enabled because retry eligibility comes from the historical occurrence,
    // not the current Draft definition.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        for _ in 0..24 {
            if w.find("resume-automation-steps").visible() {
                break;
            }
            w.scroll(
                "automation-result-body",
                ScrollDelta::Pixels(point(px(0.), px(-120.))),
                cx,
            );
            w.render_frame(cx);
        }
        let retry = w.find("resume-automation-steps");
        assert_ne!(
            retry.disabled(),
            Some(true),
            "a historical approved period remains retryable after an edit"
        );
        w.click("resume-automation-steps", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Automations, "retry stays in its period");
        assert_eq!(
            app.read(cx).period_overlay(),
            Some(occurrence.id)
        );
    })
    .unwrap();
    let (definition, reopened, collect_stage, runs) = read_state(&mut cx, &app, |a| {
        (
            a.store.automation(id).unwrap().clone(),
            a.store.occurrence(occurrence.id).unwrap().clone(),
            a.store.task(collect).unwrap().stage,
            a.store
                .runs()
                .iter()
                .filter(|run| run.automation_occurrence_id == Some(occurrence.id))
                .count(),
        )
    });
    assert_eq!(
        definition.status, definition_before.status,
        "retrying a period must not enable or resume the edited definition"
    );
    assert_eq!(definition.status, DefinitionStatus::Draft);
    assert_eq!(definition.version, 2);
    assert_eq!(definition.cursor, definition_before.cursor);
    assert_eq!(definition.cursor, None);
    assert_eq!(definition.approval, definition_before.approval);
    assert_eq!(definition.schedule, definition_before.schedule);
    assert_eq!(reopened.id, occurrence.id);
    assert_eq!(reopened.reference_time, reference);
    assert_eq!(
        reopened.snapshot.approval_version, snapshot_version,
        "the retry keeps the original approved version"
    );
    assert_eq!(collect_stage, Stage::Done, "finished steps stay complete");
    assert_eq!(reopened.research_goal_id, Some(goal));
    assert_eq!(reopened.status, OccurrenceStatus::Succeeded);
    assert_eq!(
        runs,
        4,
        "the retry continues the same period instead of duplicating it"
    );
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .all_occurrences()
        .iter()
        .filter(|candidate| candidate.id == occurrence.id)
        .count()
        == 1));
    screenshot(&mut cx, handle, &directory, "retry-scope.png");
    println!("period retry scope passed (edited definition unchanged, same period and original snapshot)");
}

fn activity_order_check(root: &Path) {
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, TimeWindow, TriggerSource,
    };
    use crate::automation_activity::{compare_newest_first, occurrence_activity, ActivityKind};

    let directory = root.join("activity-order");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    let id = draft(&mut store, "Activity order");
    enable(&mut store, id);

    // Two failed manual periods. The older ID records the earlier start.
    let first = now() - 3_000;
    let old = store
        .request_manual_occurrence(id, "activity-old", first)
        .unwrap();
    let old_reference = old.reference_time;
    let old_window = old.data_window.clone();
    store
        .occurrence_mut(old.id)
        .unwrap()
        .mark_running(first + 10)
        .unwrap();
    store
        .fail_occurrence(old.id, "activity QA failure", first + 20)
        .unwrap();

    let second = first + 1_000;
    let newer = store
        .request_manual_occurrence(id, "activity-newer", second)
        .unwrap();
    store
        .occurrence_mut(newer.id)
        .unwrap()
        .mark_running(second + 10)
        .unwrap();
    store
        .fail_occurrence(newer.id, "activity QA failure", second + 20)
        .unwrap();

    // The explicit retry is the newest activity even though its occurrence ID is older.
    let retry_at = second + 1_000;
    store.retry_occurrence(old.id, retry_at).unwrap();
    let reopened = store.occurrence(old.id).unwrap();
    assert_eq!(reopened.status, OccurrenceStatus::Queued);
    assert_eq!(reopened.retry_requested_at, Some(retry_at));
    assert_eq!(reopened.reference_time, old_reference, "retry keeps the reference");
    assert_eq!(reopened.data_window, old_window, "retry keeps the data window");
    // Freeze the render fixture with no active periods. Recovery enqueues every
    // active Queued occurrence on attach; cancelling after the retry assertions
    // keeps both records historical while preserving the retry timestamp.
    store.stop_occurrence(old.id, retry_at + 1).unwrap();

    // A scheduled period that has not started falls back to its trigger time.
    let definition = store.automation(id).unwrap().clone();
    let scheduled_at = second + 5;
    let scheduled_id = store
        .record_occurrence_without_research(AutomationOccurrence {
            id: 0,
            automation_id: id,
            definition_version: definition.version,
            source: TriggerSource::Scheduled,
            trigger_key: crate::automation::scheduled_trigger_key(id, scheduled_at),
            scheduled_for: Some(scheduled_at),
            requested_at: None,
            actual_started_at: None,
            timezone: definition.schedule.timezone.clone(),
            reference_time: scheduled_at,
            data_window: TimeWindow::unresolved("QA activity order"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: definition.objective.clone(),
                expected_result: definition.expected_result.clone(),
                shape: Some(definition.shape),
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
                "Inert QA fixture: an unstarted scheduled period.",
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
            created_at: scheduled_at,
            deadline_at: None,
            retry_requested_at: None,
        })
        .unwrap();

    let latest = crate::automation_views::latest_occurrence(&store, id).unwrap();
    assert_eq!(latest.id, old.id, "the older-ID retry is the latest activity");
    let activity = occurrence_activity(latest);
    assert_eq!(activity.kind, ActivityKind::RetryRequested);
    assert_eq!(activity.utc, retry_at);
    let mut ordered: Vec<_> = store
        .all_occurrences()
        .iter()
        .filter(|occurrence| occurrence.automation_id == id)
        .collect();
    ordered.sort_by(|a, b| compare_newest_first(a, b));
    assert_eq!(
        ordered.iter().map(|occurrence| occurrence.id).collect::<Vec<_>>(),
        vec![old.id, newer.id, scheduled_id],
        "retry, then the later start, then the unstarted scheduled trigger"
    );
    assert_eq!(
        occurrence_activity(store.occurrence(scheduled_id).unwrap()).kind,
        ActivityKind::Scheduled
    );
    let activity_text = format!(
        "{} {}",
        activity.kind.label(),
        crate::automation_views::format_in_zone_precise(activity.utc, &latest.timezone)
    );
    assert!(
        activity_text.starts_with("Retried "),
        "the shared activity key formats as Retried: {activity_text}"
    );
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
                // Set fake mode before attachment: attachment recovers and may
                // enqueue active periods immediately.
                a.automation_fake_mode = true;
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = None;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                assert!(a.error.is_empty(), "{}", a.error);
                assert!(
                    a.store
                        .all_occurrences()
                        .iter()
                        .all(|occurrence| !occurrence.status.is_active()),
                    "the activity fixture must stay inert after attachment"
                );
                assert_eq!(
                    a.store.occurrence(old.id).unwrap().status,
                    OccurrenceStatus::Cancelled
                );
                assert_eq!(
                    a.store.occurrence(old.id).unwrap().retry_requested_at,
                    Some(retry_at)
                );
                assert_eq!(
                    a.store.occurrence(newer.id).unwrap().status,
                    OccurrenceStatus::Failed
                );
                assert_eq!(
                    a.store.occurrence(scheduled_id).unwrap().status,
                    OccurrenceStatus::Skipped
                );
                assert_eq!(
                    a.store.automation(id).unwrap().status,
                    DefinitionStatus::Enabled
                );
                assert_eq!(
                    crate::automation_views::latest_occurrence(&a.store, id)
                        .map(|occurrence| occurrence.id),
                    Some(old.id)
                );
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // The list summary names the latest status and the activity timestamp.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            w.try_find(format!("automation-latest-{id}")).is_some(),
            "the list exposes the latest activity summary"
        );
        // The screenshot records the visible timestamp text for root review; the
        // expected key is asserted above with the shared formatter.
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "activity-order-list.png");

    // Detail history and global results use the same order: retried old period first.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.set_scope_identity(Scope::Automations);
            a.automation_ui.selected = Some(id);
            a.automation_ui.result_filter = None;
            a.automation_ui.history_limit = 20;
            cx.notify();
        });
        w.render_frame(cx);
        let old_y = w.find(format!("open-occurrence-{}", old.id)).bounds().origin.y;
        let newer_y = w
            .find(format!("open-occurrence-{}", newer.id))
            .bounds()
            .origin
            .y;
        let scheduled_y = w
            .find(format!("open-occurrence-{scheduled_id}"))
            .bounds()
            .origin
            .y;
        assert!(old_y < newer_y, "the retried period leads detail history");
        assert!(newer_y < scheduled_y, "started periods lead unstarted ones");
        assert!(
            w.try_find(format!("occurrence-activity-{}", old.id)).is_some(),
            "each row renders its activity timestamp"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "activity-order-history.png");

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.set_scope_identity(Scope::AutomationRuns);
            a.automation_ui.selected = None;
            a.automation_ui.result_filter = None;
            cx.notify();
        });
        w.render_frame(cx);
        let old_y = w.find(format!("open-occurrence-{}", old.id)).bounds().origin.y;
        let newer_y = w
            .find(format!("open-occurrence-{}", newer.id))
            .bounds()
            .origin
            .y;
        let scheduled_y = w
            .find(format!("open-occurrence-{scheduled_id}"))
            .bounds()
            .origin
            .y;
        assert!(old_y < newer_y, "the retried period leads global results");
        assert!(newer_y < scheduled_y);
        assert!(
            w.try_find(format!("occurrence-activity-{}", old.id)).is_some(),
            "global results render the same activity timestamp"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "activity-order-results.png");
    assert_eq!(
        read_state(&mut cx, &app, |a| crate::automation_views::latest_occurrence(&a.store, id)
            .map(|occurrence| occurrence.id)),
        Some(old.id)
    );
    println!("activity order passed (older-ID retry is latest in list, history and results walls)");
}

/// Ordinary Research background continuation and failure auto-retry must not
/// move the route, selection, dirty draft, scroll or focus either.
fn ordinary_dispatch_boundary_check(root: &Path) {
    let directory = root.join("ordinary-dispatch");
    std::fs::create_dir_all(&directory).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Ordinary continuation report','sources':[{'title':'Fixture source','url':'https://example.com/ordinary-continuation','published_at':'unknown'}]})}}),flush=True)
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/ordinary".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"]
            .map(String::from);
    let goal = store
        .create_research_goal(
            "Ordinary continuation".into(),
            "Verify background continuation".into(),
            1,
        )
        .unwrap();
    let steps = store
        .apply_research_plan(
            goal,
            r#"{"tasks":[{"title":"Collect","instructions":"Gather","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#,
        )
        .unwrap();
    let fail_goal = store
        .create_research_goal(
            "Ordinary auto retry".into(),
            "Verify delayed retry dispatch".into(),
            1,
        )
        .unwrap();
    let fail_steps = store
        .apply_research_plan(
            fail_goal,
            r#"{"tasks":[{"title":"Fail collect","instructions":"Gather","kind":"collect","depends_on":[]},{"title":"Fail analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Fail report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#,
        )
        .unwrap();
    let mut failed = store.prepare_run(fail_steps[0]).unwrap();
    failed.status = "Failed".into();
    let failed_id = failed.id;
    store.push_run(failed);
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
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.creating = true;
                a.automation_ui.step = 2;
                a.automation_ui.objective.update(cx, |state, cx| {
                    state.set_value("Unsaved ordinary background draft", w, cx);
                    state.focus(w, cx);
                });
                a.detail_scroll.set_offset(point(px(0.), px(-120.)));
                a.result_scroll.set_offset(point(px(0.), px(-120.)));
                a.automation_ui.scroll.set_offset(point(px(0.), px(-120.)));
                a.fixture_select_task(None);
                a.note_selected_run(None);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let before = dispatch_ui_state(&mut cx, &app);
    let focus_before = cx
        .update_window(handle, |_, w, cx| w.focused(cx))
        .unwrap();
    assert!(
        before.detail_scroll.y < px(0.) && before.result_scroll.y < px(0.),
        "fixture must start with nonzero scroll offsets: {:?}",
        before
    );

    // The user starts one goal; the batch/failure paths submit the remaining
    // steps and the retry in the background.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.run_research_next(goal, true, cx));
    })
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let finished = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                let a = app.read(cx);
                a.store.goal_tasks_complete(goal)
                    && a.todo_batch
                        .as_ref()
                        .is_some_and(|batch| batch.state == crate::batch::BatchState::Finished)
            })
            .unwrap();
        assert_eq!(
            dispatch_ui_state(&mut cx, &app),
            before,
            "ordinary batch continuation changed the visible state"
        );
        assert_eq!(
            cx.update_window(handle, |_, w, cx| w.focused(cx))
                .unwrap(),
            focus_before,
            "ordinary batch continuation moved focus"
        );
        if finished {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "ordinary background continuation did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        read_state(&mut cx, &app, |a| a
            .store
            .runs()
            .iter()
            .filter(|run| run.task_id == steps[0] || run.task_id == steps[1] || run.task_id == steps[2])
            .filter(|run| run.status == "Succeeded")
            .count()),
        3,
        "the background batch must finish all three steps"
    );

    // Failure auto-retry is also background work: schedule the retry without
    // moving away from the dirty draft or changing the selected attempt.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, _| a.note_selected_run(Some(failed_id)));
        w.render_frame(cx);
    })
    .unwrap();
    let before_retry = dispatch_ui_state(&mut cx, &app);
    let focus_before_retry = cx
        .update_window(handle, |_, w, cx| w.focused(cx))
        .unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.complete_research_run(failed_id, cx));
    })
    .unwrap();
    let retry_id = read_state(&mut cx, &app, |a| {
        assert_eq!(a.scope(), Scope::Automations);
        assert_eq!(a.selected_run(), Some(failed_id));
        a.queued_runs
            .iter()
            .find(|(id, _)| {
                a.store
                    .runs()
                    .iter()
                    .any(|run| run.id == *id && run.auto_retry_attempt == 1)
            })
            .map(|(id, _)| *id)
            .expect("failure schedules a delayed retry in the background")
    });
    assert_eq!(
        dispatch_ui_state(&mut cx, &app),
        before_retry,
        "failure auto-retry changed the visible state"
    );
    assert_eq!(
        cx.update_window(handle, |_, w, cx| w.focused(cx))
            .unwrap(),
        focus_before_retry,
        "failure auto-retry moved focus"
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.cancel_run_id(retry_id, cx));
    })
    .unwrap();
    println!("ordinary background dispatch passed (batch continuation and failure retry stay passive)");
}

/// A completed plan run stores its generated tasks without navigating. The user
/// opens the goal or project through the existing review affordance.
fn completion_boundary_check(root: &Path) {
    for kind in ["goal-plan", "project-plan"] {
        for route in ["progress", "other"] {
            let directory = root.join(format!("completion-{kind}-{route}"));
            std::fs::create_dir_all(&directory).unwrap();
            let mut store = model_store(&directory);
            store.set_default_model("fixture/plan".into());
            store.research_config_mut().models =
                ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"]
                    .map(String::from);
            store.project_at_mut(0).unwrap().research_brief =
                "Cover the global market and the next twelve months.".into();
            let target = if kind == "goal-plan" {
                let goal = store
                    .create_research_goal(
                        "Plan completion".into(),
                        "Plan three research steps".into(),
                        1,
                    )
                    .unwrap();
                let mut run = store.prepare_run(goal).unwrap();
                run.status = "Succeeded".into();
                run.output = r#"{"tasks":[{"title":"Completion collect","instructions":"Gather","kind":"collect","depends_on":[]},{"title":"Completion analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Completion report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#.into();
                store.push_run(run);
                goal
            } else {
                let mut run = store.prepare_project_plan(1).unwrap();
                run.status = "Succeeded".into();
                run.output = r#"{"tasks":[{"title":"Completion topic one","brief":"Study the first approved topic with explicit evidence."},{"title":"Completion topic two","brief":"Study the second approved topic with explicit evidence."}]}"#.into();
                store.push_run(run);
                1
            };
            let run_id = store.runs().last().unwrap().id;
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
                        a.fixture_select_task(None);
                        a.note_selected_run(None);
                        a.layout = Layout::List;
                        if route == "progress" {
                            a.set_scope_identity(Scope::Runs);
                            a.note_selected_run(Some(run_id));
                            a.show_run_result = true;
                            a.show_run_details = false;
                            a.layout = Layout::Board;
                            a.detail_scroll.set_offset(point(px(0.), px(-110.)));
                            a.result_scroll.set_offset(point(px(0.), px(-130.)));
                        } else {
                            a.set_scope_identity(Scope::Settings);
                            a.show_run_result = false;
                            a.show_run_details = true;
                        }
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
                let focus = app.read(cx).focus.clone();
                w.focus(&focus, cx);
                w.render_frame(cx);
            })
            .unwrap();
            let before = dispatch_ui_state(&mut cx, &app);
            let focus_before = cx
                .update_window(handle, |_, w, cx| w.focused(cx))
                .unwrap();
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.complete_research_run(run_id, cx));
            })
            .unwrap();
            assert_eq!(
                dispatch_ui_state(&mut cx, &app),
                before,
                "{kind} completion changed the visible state from {route}"
            );
            assert_eq!(
                cx.update_window(handle, |_, w, cx| w.focused(cx))
                    .unwrap(),
                focus_before,
                "{kind} completion moved focus"
            );
            let generated = read_state(&mut cx, &app, |a| {
                if kind == "goal-plan" {
                    a.store.research_children(target).len()
                } else {
                    a.store
                        .tasks()
                        .iter()
                        .filter(|task| task.project_id == 1 && task.parent_id.is_none())
                        .count()
                }
            });
            assert!(generated >= 2, "{kind} must save generated tasks");
            // Existing review affordances open the saved target without a
            // completion-time redirect.
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    if kind == "goal-plan" {
                        a.open_task(target, w, cx);
                        assert_eq!(a.selected(), Some(target));
                    } else {
                        a.open_collection_project(1, w, cx);
                        assert_eq!(a.scope(), Scope::Project(1));
                    }
                });
            })
            .unwrap();
            println!("completion boundary passed ({kind} from {route})");
        }
    }
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
if 'Queue A1' in prompt and 'Collect current evidence' in prompt:
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
        timezone: "Asia/Shanghai".into(),
    };
    let mut make = |name: &str, objective: &str| {
        let template = crate::automation_execution::build_template(
            &format!("{objective} yesterday’s public information"),
            "Brief",
        )
        .unwrap();
        let id = store
            .create_automation_draft(
                1,
                crate::automation::AutomationShape::Research,
                name.into(),
                format!("{objective} yesterday’s public information"),
                "Brief".into(),
                schedule.clone(),
                Default::default(),
                Some(template),
                now(),
            )
            .unwrap();
        enable(&mut store, id);
        id
    };
    let a1 = make("Queue A1", "Queue A1");
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
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::All,
        "background period dispatch must not replace the visible page"
    );
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
            .and_then(|id| a.store.runs().iter().find(|run| run.id == id))
            .and_then(|run| run.ended_at.clone())
            .unwrap_or_default();
        let second_start = second_occurrence
            .run_ids
            .first()
            .and_then(|id| a.store.runs().iter().find(|run| run.id == *id))
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

const POOL_PLAN: &str = r#"{"tasks":[{"title":"Collect pool evidence","instructions":"Collect the pool fixture.","kind":"collect","depends_on":[]},{"title":"Analyze pool evidence","instructions":"Analyze the pool fixture.","kind":"analyze","depends_on":[0]},{"title":"Write pool report","instructions":"Write the pool fixture report.","kind":"advise","depends_on":[1]}]}"#;

fn pool_research_goal(store: &mut Store, topic: u64, title: &str) -> u64 {
    let id = store
        .create_research_goal_in_topic(
            title.into(),
            format!("Independent pool fixture. {title}"),
            1,
            Some(topic),
        )
        .unwrap();
    store.apply_research_plan(id, POOL_PLAN).unwrap();
    store
        .task_mut(id)
        .unwrap()
        .research
        .as_mut()
        .unwrap()
        .requires_approval = true;
    store.set_stage(id, Stage::Planned).unwrap();
    id
}

/// Research and automation occupancy are independent pools. A full research
/// queue must not block automation periods and vice versa.
fn independent_pools_check(root: &Path) {
    let directory = root.join("independent-pools");
    std::fs::create_dir_all(&directory).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json,os,time
root=os.getcwd()
open(os.path.join(root,'pool-started'),'w').write('1')
while not os.path.exists(os.path.join(root,'pool-release')):
    time.sleep(0.02)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Pool fixture report','sources':[{'title':'Pool fixture','url':'https://example.com/pool','published_at':'unknown'}]})}}),flush=True)
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
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/automation".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    store.set_max_concurrent_runs(1);
    store.set_max_concurrent_automations(1);
    let topic = store.create_topic(1, "Pool research").unwrap();
    pool_research_goal(&mut store, topic, "Research A");
    pool_research_goal(&mut store, topic, "Research B");
    let automations: Vec<u64> = ["Automation A", "Automation B"]
        .into_iter()
        .map(|name| {
            let id = draft(&mut store, name);
            enable(&mut store, id);
            id
        })
        .collect();
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
        .open_window(size(px(1000.), px(720.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                // Create the manual requests after startup recovery, otherwise
                // recovery would enqueue them before the test controls the queue.
                let now = chrono::Utc::now().timestamp();
                for id in &automations {
                    a.store
                        .request_manual_occurrence(*id, &format!("pool-{id}"), now)
                        .unwrap();
                }
                a.start_todo_batch(1, cx);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let occurrences: Vec<u64> = automations
        .iter()
        .map(|id| {
            let key = format!("manual:{id}:pool-{id}");
            read_state(&mut cx, &app, |a| {
                a.store
                    .automation_occurrence_by_key(&key)
                    .unwrap()
                    .id
            })
        })
        .collect();
    let wait_for = |cx: &mut HeadlessAppContext, label: &str, done: &dyn Fn(&TaskApp) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let finished = cx
                .update_window(handle, |_, _, cx| {
                    app.update(cx, |a, cx| a.poll_runtime(cx));
                    done(app.read(cx))
                })
                .unwrap();
            if finished {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "pool check timed out: {label}");
            std::thread::sleep(Duration::from_millis(20));
        }
    };

    // Research limit 1: one research step runs, the second goal waits.
    wait_for(&mut cx, "one research task running", &|a| {
        a.research_slots_used() == 1 && !a.queued_runs.is_empty()
    });
    let pool_debug = read_state(&mut cx, &app, |a| {
        (
            a.active_runs
                .iter()
                .map(|active| {
                    a.store
                        .runs()
                        .iter()
                        .find(|run| run.id == active.id)
                        .map(|run| (run.id, run.task_id, run.status.clone(), run.automation_occurrence_id))
                        .unwrap_or((active.id, 0, "missing".into(), None))
                })
                .collect::<Vec<_>>(),
            a.store
                .all_occurrences()
                .iter()
                .map(|occurrence| (occurrence.id, occurrence.status, occurrence.trigger_key.clone()))
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_slots_used()),
        0,
        "{pool_debug:?}"
    );

    // A full research pool must not prevent an automation period from starting.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.enqueue_automation_occurrence(occurrences[0], cx));
    })
    .unwrap();
    wait_for(&mut cx, "automation A running alongside research", &|a| {
        a.automation_slots_used() == 1
    });
    assert_eq!(read_state(&mut cx, &app, |a| a.research_slots_used()), 1);

    // The automation pool is full: a second automation period waits without
    // touching the queued research goal.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.enqueue_automation_occurrence(occurrences[1], cx));
    })
    .unwrap();
    wait_for(&mut cx, "automation B queued", &|a| {
        a.queued_runs.len() == 2
            && a.store.occurrence(occurrences[1]).unwrap().status == OccurrenceStatus::Queued
    });
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_slots_used()), 1);
    assert_eq!(read_state(&mut cx, &app, |a| a.research_slots_used()), 1);

    // Raising only the automation limit admits B while research stays capped.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.set_automation_concurrency(2, cx);
            a.poll_runtime(cx);
        });
    })
    .unwrap();
    wait_for(&mut cx, "two automations running", &|a| {
        a.automation_slots_used() == 2
    });
    assert_eq!(read_state(&mut cx, &app, |a| a.research_slots_used()), 1);

    // Raising only the research limit admits the waiting research goal while
    // both automation periods keep running.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.set_execution_concurrency(2, cx);
            a.poll_runtime(cx);
        });
    })
    .unwrap();
    wait_for(&mut cx, "two research tasks running", &|a| {
        a.research_slots_used() == 2
    });
    assert_eq!(read_state(&mut cx, &app, |a| a.automation_slots_used()), 2);

    // Release the fixture and stop cleanly.
    for run in read_state(&mut cx, &app, |a| {
        a.store
            .runs()
            .iter()
            .filter(|run| !run.directory.is_empty())
            .map(|run| run.directory.clone())
            .collect::<Vec<_>>()
    }) {
        let _ = std::fs::write(std::path::Path::new(&run).join("pool-release"), "");
    }
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.todo_batch = None;
            a.queued_runs.clear();
            let running: Vec<u64> = a.active_runs.iter().map(|active| active.id).collect();
            for id in running {
                a.cancel_run_id(id, cx);
            }
        });
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(120));
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("independent pools passed (research and automation limits do not overlap)");
}

fn undo_protection_check(root: &Path) {
    let directory = root.join("undo-protection");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    let task = store
        .create_task("Ordinary task", 1, Stage::Planned, None)
        .unwrap();
    let paused = draft(&mut store, "Undo pause guard");
    enable(&mut store, paused);
    let draft_id = draft(&mut store, "Undo enable guard");
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
                a.set_scope_identity(Scope::Automations);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // Automation mutations never create Undo entries. An unrelated scoped edit
    // stays undoable and cannot roll back schedule state.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a
                .apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Ordinary edit one".into(),
                    },
                    "Task renamed",
                    cx,
                )
                .is_some());
            assert_eq!(a.undo.len(), 1, "a scoped edit is undoable");
        });
        app.update(cx, |a, cx| a.pause_automation_ui(paused, cx));
        w.render_frame(cx);
    })
    .unwrap();
    let (undo_len, reason, status) = read_state(&mut cx, &app, |a| {
        (
            a.undo.len(),
            a.undo_invalidation.clone(),
            a.store.automation(paused).unwrap().status,
        )
    });
    assert_eq!(
        undo_len, 1,
        "pause records no undo entry and keeps the scoped edit"
    );
    assert!(reason.is_none());
    assert_eq!(status, DefinitionStatus::Paused);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let definitions = a.store.automations().to_vec();
            let occurrences = a.store.all_occurrences().to_vec();
            let runs = a.store.runs().to_vec();
            let ids = (a.store.next_id(), a.store.run_id_floor());
            a.undo_change(w, cx);
            assert_eq!(a.store.automations(), definitions.as_slice(), "Undo preserves full definitions");
            assert_eq!(a.store.all_occurrences(), occurrences.as_slice(), "Undo preserves full periods and keys");
            assert_eq!(a.store.runs(), runs.as_slice(), "Undo preserves full runtime history");
            assert_eq!((a.store.next_id(), a.store.run_id_floor()), ids, "Undo preserves identity floors");
            let disk = a.storage.as_ref().unwrap().load().unwrap().unwrap();
            assert_eq!(disk.automations(), definitions.as_slice(), "Durable definitions survive Undo");
            assert_eq!(disk.all_occurrences(), occurrences.as_slice(), "Durable periods and keys survive Undo");
            assert_eq!(disk.runs(), runs.as_slice(), "Durable runtime history survives Undo");
            assert_eq!((disk.next_id(), disk.run_id_floor()), ids, "Durable identity floors survive Undo");
        });
    })
    .unwrap();
    let (title, status) = read_state(&mut cx, &app, |a| {
        (
            a.store.task(task).unwrap().title.clone(),
            a.store.automation(paused).unwrap().status,
        )
    });
    assert_eq!(title, "Ordinary task", "the scoped edit was undone");
    assert_eq!(status, DefinitionStatus::Paused);

    // A scoped edit after the automation mutation still works and does not
    // disturb the paused definition.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a
                .apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Ordinary edit two".into(),
                    },
                    "Task renamed",
                    cx,
                )
                .is_some());
            assert!(a.undo_invalidation.is_none(), "no legacy barrier remains");
{
            let definitions = a.store.automations().to_vec();
            let occurrences = a.store.all_occurrences().to_vec();
            let runs = a.store.runs().to_vec();
            let ids = (a.store.next_id(), a.store.run_id_floor());
            a.undo_change(w, cx);
            assert_eq!(a.store.automations(), definitions.as_slice(), "Undo preserves full definitions");
            assert_eq!(a.store.all_occurrences(), occurrences.as_slice(), "Undo preserves full periods and keys");
            assert_eq!(a.store.runs(), runs.as_slice(), "Undo preserves full runtime history");
            assert_eq!((a.store.next_id(), a.store.run_id_floor()), ids, "Undo preserves identity floors");
            let disk = a.storage.as_ref().unwrap().load().unwrap().unwrap();
            assert_eq!(disk.automations(), definitions.as_slice(), "Durable definitions survive Undo");
            assert_eq!(disk.all_occurrences(), occurrences.as_slice(), "Durable periods and keys survive Undo");
            assert_eq!(disk.runs(), runs.as_slice(), "Durable runtime history survives Undo");
            assert_eq!((disk.next_id(), disk.run_id_floor()), ids, "Durable identity floors survive Undo");
        };
        });
    })
    .unwrap();
    let (title, status, undo_len) = read_state(&mut cx, &app, |a| {
        (
            a.store.task(task).unwrap().title.clone(),
            a.store.automation(paused).unwrap().status,
            a.undo.len(),
        )
    });
    assert_eq!(title, "Ordinary task");
    assert_eq!(status, DefinitionStatus::Paused);
    assert_eq!(undo_len, 0);

    // Scheduled cursor + occurrence change while no run is active. The tick
    // creates no Undo entry; undoing the scoped edit leaves it intact.
    let now = TaskApp::automation_now();
    let occurrence_id = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                assert!(a
                    .apply_domain_command_undoable(
                        crate::scoped_domain::DomainCommand::TaskTitle {
                            task,
                            title: "Ordinary edit four".into(),
                        },
                        "Task renamed",
                        cx,
                    )
                    .is_some());
                // Resume the paused schedule before planning its next tick.
                a.store.resume_schedule(paused, now).unwrap();
                {
                    let mut definition = a.store.automation_mut(paused).unwrap();
                    definition.anchor = Some(now - 600);
                    definition.cursor = Some(now);
                }
                let plans = a.store.plan_automation_ticks(now);
                assert!(!plans.is_empty(), "the due schedule produces a tick");
                let commit = a.store.commit_automation_tick(&plans[0], now).unwrap();
                let occurrence = commit.queued.first().copied().or_else(|| commit.skipped.first().copied());
                assert!(occurrence.is_some(), "the tick creates or skips an occurrence");
                a.persist_if_changed(true);
                assert_eq!(a.undo.len(), 1, "the tick records no undo entry");
                assert!(a.undo_invalidation.is_none());
                occurrence.unwrap()
            })
        })
        .unwrap();
    let (occurrences, cursor, title) = read_state(&mut cx, &app, |a| {
        (
            a.store
                .all_occurrences()
                .iter()
                .filter(|o| o.automation_id == paused)
                .count(),
            a.store.automation(paused).unwrap().cursor,
            a.store.task(task).unwrap().title.clone(),
        )
    });
    assert_eq!(occurrences, 1);
    assert!(
        read_state(&mut cx, &app, |a| a.store.occurrence(occurrence_id).is_some()),
        "the committed occurrence is retained"
    );
    assert!(cursor.is_some_and(|cursor| cursor > now), "cursor advanced");
    assert_eq!(title, "Ordinary edit four");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let definitions = a.store.automations().to_vec();
            let occurrences = a.store.all_occurrences().to_vec();
            let runs = a.store.runs().to_vec();
            let ids = (a.store.next_id(), a.store.run_id_floor());
            a.undo_change(w, cx);
            assert_eq!(a.store.automations(), definitions.as_slice(), "Undo preserves full definitions");
            assert_eq!(a.store.all_occurrences(), occurrences.as_slice(), "Undo preserves full periods and keys");
            assert_eq!(a.store.runs(), runs.as_slice(), "Undo preserves full runtime history");
            assert_eq!((a.store.next_id(), a.store.run_id_floor()), ids, "Undo preserves identity floors");
            let disk = a.storage.as_ref().unwrap().load().unwrap().unwrap();
            assert_eq!(disk.automations(), definitions.as_slice(), "Durable definitions survive Undo");
            assert_eq!(disk.all_occurrences(), occurrences.as_slice(), "Durable periods and keys survive Undo");
            assert_eq!(disk.runs(), runs.as_slice(), "Durable runtime history survives Undo");
            assert_eq!((disk.next_id(), disk.run_id_floor()), ids, "Durable identity floors survive Undo");
        });
    })
    .unwrap();
    let (title, occurrences, cursor) = read_state(&mut cx, &app, |a| {
        (
            a.store.task(task).unwrap().title.clone(),
            a.store
                .all_occurrences()
                .iter()
                .filter(|o| o.automation_id == paused)
                .count(),
            a.store.automation(paused).unwrap().cursor,
        )
    });
    assert_eq!(title, "Ordinary task");
    assert_eq!(occurrences, 1, "Undo cannot delete the new occurrence");
    assert!(cursor.is_some_and(|cursor| cursor > now), "Undo cannot restore the old cursor");

    // A scoped edit followed by saving a new Quick draft: Undo restores only
    // the edit and cannot delete the draft.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a
                .apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Ordinary edit five".into(),
                    },
                    "Task renamed",
                    cx,
                )
                .is_some());
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Check fuel prices every day and cite a source.", w, cx);
            });
            a.save_automation_draft(cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    let (automations, undo_len, reason) = read_state(&mut cx, &app, |a| {
        (
            a.store.automations().len(),
            a.undo.len(),
            a.undo_invalidation.clone(),
        )
    });
    assert_eq!(automations, 3, "the Quick draft was saved");
    assert_eq!(undo_len, 1, "saving a draft records no undo entry");
    assert!(reason.is_none());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let definitions = a.store.automations().to_vec();
            let occurrences = a.store.all_occurrences().to_vec();
            let runs = a.store.runs().to_vec();
            let ids = (a.store.next_id(), a.store.run_id_floor());
            a.undo_change(w, cx);
            assert_eq!(a.store.automations(), definitions.as_slice(), "Undo preserves full definitions");
            assert_eq!(a.store.all_occurrences(), occurrences.as_slice(), "Undo preserves full periods and keys");
            assert_eq!(a.store.runs(), runs.as_slice(), "Undo preserves full runtime history");
            assert_eq!((a.store.next_id(), a.store.run_id_floor()), ids, "Undo preserves identity floors");
            let disk = a.storage.as_ref().unwrap().load().unwrap().unwrap();
            assert_eq!(disk.automations(), definitions.as_slice(), "Durable definitions survive Undo");
            assert_eq!(disk.all_occurrences(), occurrences.as_slice(), "Durable periods and keys survive Undo");
            assert_eq!(disk.runs(), runs.as_slice(), "Durable runtime history survives Undo");
            assert_eq!((disk.next_id(), disk.run_id_floor()), ids, "Durable identity floors survive Undo");
        });
    })
    .unwrap();
    let (automations, title) = read_state(&mut cx, &app, |a| {
        (
            a.store.automations().len(),
            a.store.task(task).unwrap().title.clone(),
        )
    });
    assert_eq!(automations, 3, "Undo cannot delete the saved draft");
    assert_eq!(title, "Ordinary task");

    // A scoped edit followed by enabling a draft: Undo restores only the edit
    // and cannot disable the automation again.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a
                .apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Ordinary edit six".into(),
                    },
                    "Task renamed",
                    cx,
                )
                .is_some());
            a.edit_automation(draft_id, w, cx);
            a.enable_automation(cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    let (status, approval, cursor, undo_len) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(draft_id).unwrap();
        (
            definition.status,
            definition.approval.is_some(),
            definition.cursor,
            a.undo.len(),
        )
    });
    assert_eq!(status, DefinitionStatus::Enabled);
    assert!(approval);
    assert!(cursor.is_some());
    assert_eq!(undo_len, 1, "enabling records no undo entry");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let definitions = a.store.automations().to_vec();
            let occurrences = a.store.all_occurrences().to_vec();
            let runs = a.store.runs().to_vec();
            let ids = (a.store.next_id(), a.store.run_id_floor());
            a.undo_change(w, cx);
            assert_eq!(a.store.automations(), definitions.as_slice(), "Undo preserves full definitions");
            assert_eq!(a.store.all_occurrences(), occurrences.as_slice(), "Undo preserves full periods and keys");
            assert_eq!(a.store.runs(), runs.as_slice(), "Undo preserves full runtime history");
            assert_eq!((a.store.next_id(), a.store.run_id_floor()), ids, "Undo preserves identity floors");
            let disk = a.storage.as_ref().unwrap().load().unwrap().unwrap();
            assert_eq!(disk.automations(), definitions.as_slice(), "Durable definitions survive Undo");
            assert_eq!(disk.all_occurrences(), occurrences.as_slice(), "Durable periods and keys survive Undo");
            assert_eq!(disk.runs(), runs.as_slice(), "Durable runtime history survives Undo");
            assert_eq!((disk.next_id(), disk.run_id_floor()), ids, "Durable identity floors survive Undo");
        });
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.automation(draft_id).unwrap().status),
        DefinitionStatus::Enabled,
        "Undo cannot disable the newly enabled automation"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.task(task).unwrap().title.clone()),
        "Ordinary task"
    );
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("undo protection passed");
}

fn draft_defaults_check(root: &Path) {
    let directory = root.join("draft-defaults");
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
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // First session: weekly, SkipAll, a different time zone and non-default
    // limits. Continue to the review step and go Back; the same draft must keep
    // every value.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("new-automation", cx);
        w.render_frame(cx);
        w.click("automation-objective", cx);
        w.input("Check fuel prices and cite a source.", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        w.click("automation-kind-Weekly", cx);
        w.click("automation-catchup-skip", cx);
        w.click("automation-day-6", cx);
        app.update(cx, |a, cx| {
            a.automation_ui.hour.update(cx, |state, cx| state.set_value("07", w, cx));
            a.automation_ui.minute.update(cx, |state, cx| state.set_value("45", w, cx));
            a.automation_ui.timezone.update(cx, |state, cx| state.set_value("Europe/London", w, cx));
            a.automation_ui.delay.update(cx, |state, cx| state.set_value("120", w, cx));
            a.automation_ui.timeout.update(cx, |state, cx| state.set_value("45", w, cx));
            a.automation_ui.failures.update(cx, |state, cx| state.set_value("5", w, cx));
            a.automation_ui.retention.update(cx, |state, cx| state.set_value("60", w, cx));
            cx.notify();
        });
        w.render_frame(cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        w.click("automation-back-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let a = app.read(cx);
        let ui = &a.automation_ui;
        let schedule = ui.schedule(cx).expect("retained schedule");
        assert_eq!(
            schedule.spec,
            crate::automation_schedule::ScheduleSpec::Weekly {
                days: vec![0, 1, 2, 3, 4, 6],
                hour: 7,
                minute: 45,
            }
        );
        assert_eq!(schedule.timezone, "Europe/London");
        let policy = ui.policy(cx).expect("retained policy");
        assert_eq!(policy.catch_up, crate::automation_schedule::CatchUp::SkipAll);
        assert_eq!(policy.allowed_delay_seconds, 120 * 60);
        assert_eq!(policy.timeout_seconds, 45 * 60);
        assert_eq!(policy.max_consecutive_failures, 5);
        assert_eq!(policy.log_retention_days, 60);
    });

    // Second session: hourly interval. Cancel both sessions, then start New.
    cx.update_window(handle, |_, w, cx| {
        w.click("cancel-automation-wizard", cx);
        w.render_frame(cx);
        if w.try_find("dirty-exit-discard").is_some() {
            w.click("dirty-exit-discard", cx);
            w.render_frame(cx);
        }
        w.click("new-automation", cx);
        w.render_frame(cx);
        w.click("automation-objective", cx);
        w.input("Check fuel prices every two hours.", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        w.click("automation-kind-Interval", cx);
        w.click("automation-unit-hours", cx);
        w.click("automation-catchup-skip", cx);
        app.update(cx, |a, cx| {
            a.automation_ui.interval.update(cx, |state, cx| state.set_value("2", w, cx));
            cx.notify();
        });
        w.render_frame(cx);
        w.click("cancel-automation-wizard", cx);
        w.render_frame(cx);
        if w.try_find("dirty-exit-discard").is_some() {
            w.click("dirty-exit-discard", cx);
            w.render_frame(cx);
        }
    })
    .unwrap();

    // New must restore the explicit defaults, not a mixture of the two sessions.
    cx.update_window(handle, |_, w, cx| {
        w.click("new-automation", cx);
        w.render_frame(cx);
        w.click("automation-objective", cx);
        w.input("Check fuel prices and cite a source.", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let a = app.read(cx);
        let ui = &a.automation_ui;
        assert_eq!(ui.shape, crate::automation::AutomationShape::Quick);
        assert_eq!(ui.kind, crate::automation_views::AutomationKind::Daily);
        assert!(!ui.interval_in_hours, "interval input defaults to minutes");
        assert_eq!(ui.catch_up, crate::automation_schedule::CatchUp::LatestOnly);
        assert_eq!(ui.weekdays, crate::automation_views::DEFAULT_WEEKDAYS);
        let schedule = ui.schedule(cx).expect("default schedule");
        assert_eq!(
            schedule.spec,
            crate::automation_schedule::ScheduleSpec::Daily { hour: 10, minute: 0 }
        );
        assert_eq!(schedule.timezone, crate::automation_views::system_timezone());
        let policy = ui.policy(cx).expect("default policy");
        assert_eq!(policy.catch_up, crate::automation_schedule::CatchUp::LatestOnly);
        assert_eq!(policy.allowed_delay_seconds, 24 * 60 * 60);
        assert_eq!(policy.timeout_seconds, 30 * 60);
        assert_eq!(policy.max_consecutive_failures, 3);
        assert_eq!(policy.log_retention_days, 30);
        assert_eq!(ui.interval.read(cx).value().to_string(), "10");
        assert_eq!(ui.hour.read(cx).value().to_string(), "10");
        assert_eq!(ui.minute.read(cx).value().to_string(), "00");
        assert_eq!(ui.delay.read(cx).value().to_string(), "");
        assert_eq!(ui.timeout.read(cx).value().to_string(), "30");
        assert_eq!(ui.failures.read(cx).value().to_string(), "3");
        assert_eq!(ui.retention.read(cx).value().to_string(), "30");
        assert_eq!(ui.timezone.read(cx).value().to_string(), crate::automation_views::system_timezone());
    });
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("draft defaults passed");
}

fn shape_switch_check(root: &Path) {
    let directory = root.join("shape-switch");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    let id = draft(&mut store, "Shape switch guard");
    enable(&mut store, id);
    let before = store.automation(id).unwrap().clone();
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.edit_automation(id, w, cx);
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
        assert_eq!(
            app.read(cx).automation_ui.plan.as_ref().map(|plan| plan.goals.len()),
            Some(before.template.as_ref().unwrap().goals.len()),
            "editing starts from the approved plan"
        );
        w.click("automation-shape-quick", cx);
        w.render_frame(cx);
        {
            let a = app.read(cx);
            assert_eq!(a.automation_ui.shape, crate::automation::AutomationShape::Quick);
            assert!(a.automation_ui.plan.is_none(), "the research plan is discarded");
            assert!(a.automation_ui.planning_automation.is_none());
            assert_eq!(
                a.store.automation(id).unwrap().approval,
                before.approval,
                "switching in the form does not silently rewrite the stored approval"
            );
        }
        // Switching back must not resurrect the incompatible plan.
        w.click("automation-shape-research", cx);
        w.click("automation-shape-quick", cx);
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.plan.is_none());
        w.click("save-automation-draft", cx);
        w.render_frame(cx);
        if let Some(Err(error)) = app.read(cx).automation_ui.feedback.as_ref() {
            panic!("shape switch save failed: {error}");
        }
    })
    .unwrap();
    let after = read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().clone());
    assert_eq!(after.version, before.version + 1);
    assert_eq!(after.status, DefinitionStatus::Draft);
    assert!(after.approval.is_none(), "the incompatible approval is cleared");
    assert_eq!(after.shape, crate::automation::AutomationShape::Quick);
    assert!(
        after
            .template
            .as_ref()
            .is_some_and(|template| template.goals.iter().map(|goal| goal.steps.len()).sum::<usize>() == 1),
        "a Quick draft carries its single answer step"
    );
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("shape switch invalidation passed");
}

fn planning_revision_completion_check(root: &Path) {
    let directory = root.join("planning-revision");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::create_dir_all(directory.join("automations")).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let automation_project = store
        .ensure_automation_project(directory.join("automations").to_string_lossy().into())
        .unwrap();
    let id = store
        .create_automation_draft(
            automation_project,
            crate::automation::AutomationShape::Research,
            "Revision completion".into(),
            "Objective A".into(),
            "Expected A".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Daily { hour: 9, minute: 0 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            None,
            now(),
        )
        .unwrap();
    let request = store
        .create_automation_request(id, automation_project, "Objective A", "Expected A")
        .unwrap();
    let plan = r#"{"topic_name":"Revision plan","brief":{"objective":"Objective A","deliverable":"Expected A","scope":"Scope","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Goal","brief":"Brief","tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}]}"#;
    confirm_fixture_understanding(&mut store, request);
    let mut first = store.prepare_request_plan(request).unwrap();
    first.status = "Succeeded".into();
    first.output = plan.into();
    let first_id = first.id;
    store.push_run(first);
    let mut second = store.prepare_request_plan(request).unwrap();
    second.status = "Succeeded".into();
    second.output = plan.into();
    let second_id = second.id;
    store.push_run(second);
    store.request_at_mut(0).unwrap().planning_run_id = Some(second_id);
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(id);
                a.edit_automation(id, w, cx);
                a.automation_ui.planning_automation = Some(id);
                a.automation_ui.planning_run = Some(second_id);
                a.complete_research_run(first_id, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.runs().iter().find(|run| run.id == first_id).unwrap().status.clone()),
        "Stale",
        "an older same-input attempt cannot overwrite the newer one"
    );
    assert!(read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().template.is_none()));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value("Edited B", w, cx);
            });
            a.complete_research_run(second_id, cx);
        });
    })
    .unwrap();
    assert!(read_state(&mut cx, &app, |a| a.automation_ui.plan.is_none()));
    let (objective_after, expected_after) = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                (
                    a.automation_ui.objective.read(cx).value().to_string(),
                    a.automation_ui.expected.read(cx).value().to_string(),
                )
            })
        })
        .unwrap();
    assert!(objective_after.contains("Objective A"));
    assert!(
        expected_after.contains("Edited B"),
        "unsaved edited expected result must survive the completing run"
    );
    let (expected_only, binding_revision) = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.automation_ui.expected.update(cx, |state, cx| {
                    state.set_value("Expected B", w, cx);
                });
                a.save_automation_draft(cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                let request = a
                    .store
                    .create_automation_request(id, automation_project, "Objective A", "Expected B")
                    .unwrap();
                let plan = r#"{"topic_name":"Expected B plan","brief":{"objective":"Objective A","deliverable":"Expected B","scope":"Scope","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Goal","brief":"Brief","tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}]}"#;
                confirm_fixture_understanding(&mut a.store, request);
                let mut run = a.store.prepare_request_plan(request).unwrap();
                run.status = "Succeeded".into();
                run.output = plan.into();
                let run_id = run.id;
                a.store.push_run(run);
                a.automation_ui.planning_automation = Some(id);
                a.automation_ui.planning_run = Some(run_id);
                a.complete_research_run(run_id, cx);
                (request, a.store.research_requests().iter().find(|value| value.id == request).unwrap().revision)
            })
        })
        .unwrap();
    assert_eq!(binding_revision, 2);
    let binding = read_state(&mut cx, &app, |a| a.store.automation(id).unwrap().plan_binding.clone()).unwrap();
    assert_eq!(binding.request_id, expected_only);
    assert_eq!(binding.revision, 2);
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("planning revision completion passed (same-revision order, dirty draft, expected-only rebind)");
}

fn wait_for_planning_run(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    handle: AnyWindowHandle,
    run_id: u64,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let terminal = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                w.render_frame(cx);
                app.read(cx)
                    .store
                    .runs()
                    .iter()
                    .find(|run| run.id == run_id)
                    .is_some_and(|run| {
                        matches!(
                            run.status.as_str(),
                            "Succeeded" | "Failed" | "Stale" | "Cancelled" | "Blocked"
                        )
                    })
            })
            .unwrap();
        if terminal {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "[timeout] planning run {run_id} did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn confirm_wizard_understanding(cx: &mut HeadlessAppContext, app: &Entity<TaskApp>, handle: AnyWindowHandle) {
    let request = read_state(cx, app, |a| {
        a.store.latest_planning_request(a.automation_ui.editing.unwrap()).unwrap().id
    });
    assert!(read_state(cx, app, |a| a.store.request_understanding_ready(request)));
    assert!(!read_state(cx, app, |a| a.store.request_understanding_confirmed(request)));
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("understanding-confirm-{request}"), cx);
        w.render_frame(cx);
    }).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let run = loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.poll_runtime(cx));
            w.render_frame(cx);
        }).unwrap();
        if let Some(run) = read_state(cx, app, |a| a.store.latest_request_attempt(request)
            .filter(|run| !crate::request_planning::is_understanding_run(run)).map(|run| run.id)) {
            break run;
        }
        assert!(std::time::Instant::now() < deadline, "confirmation did not start goal planning");
        std::thread::sleep(Duration::from_millis(10));
    };
    wait_for_planning_run(cx, app, handle, run);
    assert!(read_state(cx, app, |a| a.store.request_understanding_confirmed(request)));
}

fn latest_prompt(directory: &Path) -> String {
    let mut files: Vec<_> = std::fs::read_dir(directory.join("prompts"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    files.sort();
    std::fs::read_to_string(files.last().expect("fake planner captured no prompt")).unwrap()
}

fn planning_cli_flow_check(root: &Path) {
    let directory = root.join("planning-cli-flow");
    std::fs::create_dir_all(directory.join("prompts")).unwrap();
    let cli = directory.join("fake-opencode");
    std::fs::write(
        &cli,
        r#"#!/usr/bin/python3
import json, os, sys, time
prompt = sys.argv[-1]
base = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(base, 'prompts', f'prompt-{time.time_ns()}.txt'), 'w') as handle:
    handle.write(prompt)
state = os.path.join(base, 'failed-once')
if 'FAIL_ONCE' in prompt and not os.path.exists(state):
    open(state, 'w').write('1')
    sys.exit(7)
plan = {"topic_name":"Captured plan","brief":{"objective":"Captured","deliverable":"Expected","scope":"Latest","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Captured goal","brief":"Brief","tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}]}
if 'You are clarifying ONE saved research request' in prompt:
    plan = {'summary':'Review the requested subject, focus on the stated scope, and deliver a concise sourced report. Do not expand into unrelated recommendations.', 'questions':[]}
print(json.dumps({'type':'text','part':{'text':json.dumps(plan)}}), flush=True)
"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    store.set_opencode_path(cli.to_string_lossy().into());
    store.set_default_model("fixture/plan".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
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
        w.click("new-automation", cx);
        w.click("automation-shape-research", cx);
        w.click("automation-objective", cx);
        w.input("Objective A", cx);
        w.click("automation-expected", cx);
        w.input("Expected A", cx);
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (automation, run_a) = read_state(&mut cx, &app, |a| {
        (
            a.automation_ui.editing.expect("wizard saved its draft"),
            a.automation_ui.planning_run.expect("wizard started a planner"),
        )
    });
    wait_for_planning_run(&mut cx, &app, handle, run_a);
    assert!(read_state(&mut cx, &app, |a| a.store.automation(automation).unwrap().template.is_none()));
    screenshot(&mut cx, handle, &directory, "understanding-light.png");
    cx.update_window(handle, |_, w, cx| { w.click("toggle-appearance", cx); w.render_frame(cx); }).unwrap();
    screenshot(&mut cx, handle, &directory, "understanding-dark.png");
    cx.update_window(handle, |_, w, cx| { w.click("toggle-appearance", cx); w.render_frame(cx); }).unwrap();
    confirm_wizard_understanding(&mut cx, &app, handle);
    let prompt = latest_prompt(&directory);
    assert!(prompt.contains("Objective A"));
    assert!(prompt.contains("Expected A"));
    assert!(prompt.contains("\"revision\":1"));
    assert!(read_state(&mut cx, &app, |a| a.store.automation(automation).unwrap().plan_binding.is_some()));
    let request_a = read_state(&mut cx, &app, |a| {
        a.store.latest_planning_request(automation).unwrap().id
    });
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (status_a, approval_a) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(automation).unwrap();
        (
            definition.status,
            definition.approval.as_ref().map(|approval| (approval.plan_request_id, approval.plan_revision)),
        )
    });
    assert_eq!(status_a, DefinitionStatus::Enabled, "generated A must actually enable");
    assert_eq!(approval_a, Some((Some(request_a), Some(1))));
    let period_a = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let occurrence = a
                    .store
                    .request_manual_occurrence(automation, "journey-a", now())
                    .unwrap();
                a.persist_if_changed(true);
                cx.notify();
                (occurrence.id, occurrence.snapshot.objective.clone(), occurrence.snapshot.expected_result.clone())
            })
        })
        .unwrap();
    assert_eq!(period_a.1, "Objective A");
    assert_eq!(period_a.2, "Expected A");

    // Objective B and expected B through the real wizard and planner.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.replan_automation(automation, w, cx));
        w.click("automation-objective", cx);
        w.press("cmd-a", cx);
        w.input("Objective B", cx);
        w.click("automation-expected", cx);
        w.press("cmd-a", cx);
        w.input("Expected B", cx);
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    let run_b = read_state(&mut cx, &app, |a| a.automation_ui.planning_run.expect("replan started"));
    wait_for_planning_run(&mut cx, &app, handle, run_b);
    confirm_wizard_understanding(&mut cx, &app, handle);
    let prompt = latest_prompt(&directory);
    assert!(prompt.contains("Objective B"));
    assert!(prompt.contains("Expected B"));
    assert!(prompt.contains("\"revision\":2"));
    let revision_b = read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().revision);
    let request_b = read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id);
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (status_b, binding_b, approval_b) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(automation).unwrap();
        (
            definition.status,
            definition.plan_binding.clone(),
            definition.approval.as_ref().map(|approval| (approval.plan_request_id, approval.plan_revision)),
        )
    });
    assert_eq!(status_b, DefinitionStatus::Enabled, "replanned B must enable without another Replan");
    assert_eq!(binding_b.as_ref().map(|binding| binding.request_id), Some(request_b));
    assert_eq!(approval_b, Some((Some(request_b), Some(revision_b))));
    let unchanged_period = read_state(&mut cx, &app, |a| {
        let occurrence = a.store.occurrence(period_a.0).unwrap();
        (
            occurrence.snapshot.objective.clone(),
            occurrence.snapshot.expected_result.clone(),
        )
    });
    assert_eq!(unchanged_period, ("Objective A".into(), "Expected A".into()));

    // Expected-only edit is another immutable revision.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.replan_automation(automation, w, cx));
        w.click("automation-expected", cx);
        w.press("cmd-a", cx);
        w.input("Expected C", cx);
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    let run_expected = read_state(&mut cx, &app, |a| {
        a.automation_ui.planning_run.expect("expected-only replan started")
    });
    wait_for_planning_run(&mut cx, &app, handle, run_expected);
    confirm_wizard_understanding(&mut cx, &app, handle);
    let prompt = latest_prompt(&directory);
    assert!(prompt.contains("Objective B"));
    assert!(prompt.contains("Expected C"));
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .latest_planning_request(automation)
        .unwrap()
        .revision)
        > revision_b);
    let request_c = read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id);
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (binding_c, approval_c) = read_state(&mut cx, &app, |a| {
        let definition = a.store.automation(automation).unwrap();
        (
            definition.plan_binding.as_ref().map(|binding| binding.request_id),
            definition.approval.as_ref().and_then(|approval| approval.plan_request_id),
        )
    });
    assert_eq!(binding_c, Some(request_c), "expected-only revision needs its own reviewed plan");
    assert_eq!(approval_c, Some(request_c));

    // Repeated unchanged replan reuses the same request revision.
    let request_before = read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.replan_automation(automation, w, cx));
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    let run_unchanged = read_state(&mut cx, &app, |a| a.automation_ui.planning_run.expect("unchanged replan started"));
    wait_for_planning_run(&mut cx, &app, handle, run_unchanged);
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id),
        request_before
    );

    // Failed planning keeps the captured edited text and retries the same request.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.replan_automation(automation, w, cx));
        w.click("automation-objective", cx);
        w.press("cmd-a", cx);
        w.input("FAIL_ONCE", cx);
        w.click("automation-expected", cx);
        w.press("cmd-a", cx);
        w.input("Failure contract", cx);
        w.click("automation-next-step", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    let run_failed = read_state(&mut cx, &app, |a| a.automation_ui.planning_run.expect("failed planner started"));
    wait_for_planning_run(&mut cx, &app, handle, run_failed);
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.runs().iter().find(|run| run.id == run_failed).unwrap().status.clone()),
        "Failed"
    );
    let failed_request = read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id);
    let (objective_after, expected_after) = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                (
                    a.automation_ui.objective.read(cx).value().to_string(),
                    a.automation_ui.expected.read(cx).value().to_string(),
                )
            })
        })
        .unwrap();
    assert!(objective_after.contains("FAIL_ONCE"));
    assert!(expected_after.contains("Failure contract"));
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-back-step", cx);
        w.click("automation-next-step", cx);
    })
    .unwrap();
    let (retry_id, retry_step, retry_error) = read_state(&mut cx, &app, |a| {
        (
            a.automation_ui.planning_run,
            a.automation_ui.step,
            a.automation_ui.preview_error.clone(),
        )
    });
    assert_ne!(
        retry_id,
        Some(run_failed),
        "retry did not start (step {retry_step}, error {retry_error})"
    );
    let retry_id = retry_id.expect("retry started");
    wait_for_planning_run(&mut cx, &app, handle, retry_id);
    confirm_wizard_understanding(&mut cx, &app, handle);
    let prompt = latest_prompt(&directory);
    assert!(prompt.contains("FAIL_ONCE"));
    assert!(prompt.contains("Failure contract"));
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.latest_planning_request(automation).unwrap().id),
        failed_request
    );

    if std::env::var_os("PLANNING_QA_UNDERSTANDING_ONLY").is_some() {
        println!("Understanding confirmation, revised scope and retry checks passed");
        return;
    }

    let attempt_ids = read_state(&mut cx, &app, |a| {
        let mut ids: Vec<_> = a
            .store
            .runs()
            .iter()
            .filter(|run| {
                run.planning_owner
                    .as_ref()
                    .is_some_and(|owner| owner.automation_id == Some(automation))
            })
            .map(|run| run.id)
            .collect();
        ids.sort_unstable();
        ids
    });
    assert!(
        attempt_ids.len() >= 5,
        "every revision and retry attempt stays in owner history"
    );
    assert!(read_state(&mut cx, &app, |a| a
        .store
        .runs()
        .iter()
        .filter(|run| run.planning_owner.is_some())
        .all(|run| a.store.run_is_automation_owned(run))));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_automation(automation, w, cx);
            a.navigation_path();
        });
    })
    .unwrap();
    let (_, current) = read_state(&mut cx, &app, |a| a.navigation_path());
    assert_eq!(
        current,
        read_state(&mut cx, &app, |a| a.store.automation(automation).unwrap().name.clone())
    );
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Runs, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    assert!(read_state(&mut cx, &app, |a| {
        attempt_ids
            .iter()
            .all(|run_id| a.displayed_run_id() != Some(*run_id))
    }));
    assert_eq!(read_state(&mut cx, &app, |a| a.store.progress_result_task_count()), 0);
    let legacy_id = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let mut legacy = a.store.runs().last().unwrap().clone();
                legacy.id = a.store.runs().iter().map(|run| run.id).max().unwrap() + 1;
                legacy.planning_owner = None;
                legacy.prompt.clear();
                legacy.input_signature = "[\"legacy-ambiguous\"]".into();
                legacy.status = "Succeeded".into();
                legacy.task_title = "Legacy automation planning attempt".into();
                a.store.push_run(legacy.clone());
                a.persist_if_changed(true);
                legacy.id
            })
        })
        .unwrap();

    // Restart from the durable snapshot written by the attempts above.
    let restart_directory = directory.join("restart");
    std::fs::create_dir_all(&restart_directory).unwrap();
    let restart_path = restart_directory.join("workspace.sqlite3");
    std::fs::copy(&path, &restart_path).unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(restart_path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(automation);
                a.automation_ui.creating = false;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let samples = [attempt_ids[0], run_expected, run_failed];
    assert_eq!(
        samples.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3,
        "the history sample needs three distinct attempts"
    );
    let expected_content = read_state(&mut cx, &app, |a| {
        samples
            .iter()
            .map(|run_id| {
                let request = a.store.request_for_run(*run_id).expect("owned attempt has a request");
                (*run_id, request.input.clone(), request.expected_result.clone().unwrap_or_default())
            })
            .collect::<Vec<_>>()
    });
    let before = read_state(&mut cx, &app, |a| serde_json::to_string(&a.store).unwrap());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("automation-settings", cx);
        w.render_frame(cx);
        w.within("automation-settings-focus").scroll(
            "automation-settings-body",
            ScrollDelta::Pixels(point(px(0.), px(-80.))),
            cx,
        );
        w.render_frame(cx);
        for run_id in &attempt_ids {
            assert!(
                w.try_find(format!("planning-attempt-{run_id}")).is_some(),
                "restart keeps attempt {run_id} listed under the automation"
            );
        }
    })
    .unwrap();
    for (run_id, objective, expected) in &expected_content {
        cx.update_window(handle, |_, w, cx| {
            w.within("automation-settings-focus").scroll("automation-settings-body",
                ScrollDelta::Pixels(point(px(0.), px(10_000.))), cx);
            for _ in 0..20 {
                w.render_frame(cx);
                if w.try_find(format!("planning-attempt-{run_id}")).is_some_and(|item| item.visible()) {
                    break;
                }
                w.within("automation-settings-focus").scroll("automation-settings-body",
                    ScrollDelta::Pixels(point(px(0.), px(-120.))), cx);
            }
        }).unwrap();
        let scroll_before = read_state(&mut cx, &app, |a| a.automation_ui.settings_scroll.offset().y);
        cx.update_window(handle, |_, w, cx| {
            w.click(format!("planning-attempt-{run_id}"), cx);
            w.render_frame(cx);
            w.render_frame(cx);
        })
        .unwrap();
        assert_eq!(
            read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
            Some(*run_id)
        );
        let (actual, status, revision) = read_state(&mut cx, &app, |a| {
            let request = a.store.request_for_run(*run_id).unwrap();
            let run = a.store.runs().iter().find(|run| run.id == *run_id).unwrap();
            (
                (request.input.clone(), request.expected_result.clone()),
                run.status.clone(),
                request.revision,
            )
        });
        assert_eq!(&actual.0, objective);
        assert_eq!(actual.1.clone().unwrap_or_default(), *expected);
        let id = |prefix: &str, value: &str| {
            match prefix {
                "planning-attempt-status" => format!("planning-attempt-status-{run_id}-{}", value.replace(' ', "-")),
                "planning-attempt-revision" => format!("planning-attempt-revision-{run_id}-{value}"),
                _ => format!("{prefix}-{run_id}"),
            }
        };
        cx.update_window(handle, |_, w, _| {
            assert!(
                w.try_find("planning-attempt-focus").is_some(),
                "observed attempt dialog wrapper missing"
            );
            assert_eq!(
                w.find(id("planning-attempt-objective", objective)).label(),
                Some(objective.as_str())
            );
            assert_eq!(
                w.find(id("planning-attempt-expected", expected)).label(),
                Some(expected.as_str())
            );
            assert_eq!(
                w.find(id("planning-attempt-status", &status)).label(),
                Some(status.as_str())
            );
            let revision_label = format!("Revision {revision}");
            assert_eq!(
                w.find(id("planning-attempt-revision", &revision.to_string())).label(),
                Some(revision_label.as_str())
            );
        })
        .unwrap();
        let (_, current) = read_state(&mut cx, &app, |a| a.navigation_path());
        assert_eq!(
            current,
            read_state(&mut cx, &app, |a| a.store.automation(automation).unwrap().name.clone())
        );
        screenshot(&mut cx, handle, &directory, &format!("planning-attempt-{run_id}.png"));
        if *run_id == run_expected {
            cx.update(|cx| Theme::change(ThemeMode::Dark, None, cx));
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, &format!("planning-attempt-{run_id}-dark.png"));
            cx.update(|cx| Theme::change(ThemeMode::Light, None, cx));
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
            })
            .unwrap();
        }
        cx.update_window(handle, |_, w, cx| {
            w.click("close-planning-attempt-footer", cx);
            w.render_frame(cx);
        })
        .unwrap();
        assert_eq!(
            read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
            None
        );
        assert_eq!(
            read_state(&mut cx, &app, |a| a.automation_ui.settings_scroll.offset().y),
            scroll_before,
            "closing an attempt restores the settings history scroll"
        );
        cx.update_window(handle, |_, w, _| {
            assert_eq!(
                w.try_find(format!("planning-attempt-{run_id}"))
                    .and_then(|item| item.focused()),
                Some(true),
                "closing an attempt restores focus to its own Review opener"
            );
        })
        .unwrap();
    }
    assert_eq!(
        read_state(&mut cx, &app, |a| serde_json::to_string(&a.store).unwrap()),
        before,
        "reviewing attempts must not mutate saved history"
    );
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.selected = None;
            a.close_settings_overlay();
            a.close_planning_attempt(w, cx);
            a.navigate(Scope::Automations, w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(
            w.try_find(format!("legacy-planning-attempt-{legacy_id}")).is_some(),
            "ambiguous automation-project history is visible under Automations"
        );
        w.click(format!("legacy-planning-attempt-{legacy_id}"), cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
        Some(legacy_id)
    );
    cx.update(|cx| Theme::change(ThemeMode::Dark, None, cx));
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "planning-history-dark.png");
    cx.update(|cx| Theme::change(ThemeMode::Light, None, cx));
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "planning-history-light.png");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
        None
    );
    cx.update_window(handle, |_, w, _| {
        assert_eq!(
            w.try_find(format!("legacy-planning-attempt-{legacy_id}"))
                .and_then(|item| item.focused()),
            Some(true),
            "Escape restores focus to the legacy Review opener"
        );
    })
    .unwrap();
    let narrow_directory = directory.join("narrow");
    std::fs::create_dir_all(&narrow_directory).unwrap();
    let narrow_path = narrow_directory.join("workspace.sqlite3");
    std::fs::copy(&restart_path, &narrow_path).unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    let mut app = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(narrow_path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(automation);
                a.automation_ui.creating = false;
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    let narrow_target = format!("planning-attempt-{}", attempt_ids[0]);
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("automation-settings", cx);
        w.render_frame(cx);
        let target = narrow_target.clone();
        if !w.find(target.clone()).visible() {
            w.within("automation-settings-focus").scroll(
                "automation-settings-body",
                ScrollDelta::Pixels(point(px(0.), px(-600.))),
                cx,
            );
            w.render_frame(cx);
        }
        w.click(target, cx);
        w.render_frame(cx);
        assert!(w.try_find("planning-attempt-focus").is_some());
        w.press("escape", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.planning_attempt_overlay()),
        None
    );
    assert!(
        read_state(&mut cx, &app, |a| a.settings_overlay()),
        "Escape restores the initiating settings dialog"
    );
    cx.update_window(handle, |_, w, _| {
        assert_eq!(
            w.try_find(narrow_target.clone()).and_then(|item| item.focused()),
            Some(true),
            "narrow Escape restores the exact Review opener"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "planning-history-narrow.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Runs, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    assert!(read_state(&mut cx, &app, |a| a.displayed_run_id() != Some(legacy_id)));
    assert_eq!(read_state(&mut cx, &app, |a| a.store.progress_result_task_count()), 0);
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("planning CLI flow passed (wizard A/B, expected-only, unchanged, failure retry, restart attempts)");
}

fn late_planning_completion_check(root: &Path) {
    let directory = root.join("late-planning");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::create_dir_all(directory.join("automations")).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = model_store(&directory);
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    let automation_project = store
        .ensure_automation_project(directory.join("automations").to_string_lossy().into())
        .unwrap();
    let id = store
        .create_automation_draft(
            automation_project,
            crate::automation::AutomationShape::Research,
            "Late planning completion".into(),
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
        .create_automation_request(id, automation_project, "Track the semiconductor supply chain", "A sourced note")
        .unwrap();
    confirm_fixture_understanding(&mut store, request);
    let mut run = store.prepare_request_plan(request).unwrap();
    run.status = "Succeeded".into();
    run.ended_at = Some(crate::opencode::timestamp());
    run.output = r#"{"topic_name":"Semiconductor supply","brief":{"objective":"Track supply","deliverable":"Sourced note","scope":"Latest quarter","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Foundry capacity","brief":"Track announced capacity","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare capacity","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]},{"title":"Equipment orders","brief":"Track equipment bookings","tasks":[{"title":"Collect","instructions":"Collect orders","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare orders","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]}]}"#.into();
    let run_id = run.id;
    store.push_run(run);
    store.request_mut(request).unwrap()
        .planning_run_id = Some(run_id);
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(id);
                a.edit_automation(id, w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // The wizard starts this exact planning run, then the user switches to Quick
    // and back to Research before the planner completes.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.step = 3;
            a.automation_ui.planning_automation = Some(id);
            a.automation_ui.planning_run = Some(run_id);
            a.select_automation_shape(crate::automation::AutomationShape::Quick, cx);
            assert!(a.automation_ui.plan.is_none());
            assert!(a.automation_ui.planning_run.is_none());
            a.select_automation_shape(crate::automation::AutomationShape::Research, cx);
            assert!(a.automation_ui.plan.is_none());
            assert!(a.automation_ui.planning_run.is_none());
            // The user is back on the schedule step when the old planner finishes.
            a.automation_ui.step = 2;
            a.complete_research_run(run_id, cx);
        });
    })
    .unwrap();
    let (plan, step, planning_automation, planning_run, template_goals, run_status, request_run) =
        read_state(&mut cx, &app, |a| {
            (
                a.automation_ui.plan.clone(),
                a.automation_ui.step,
                a.automation_ui.planning_automation,
                a.automation_ui.planning_run,
                a.store
                    .automation(id)
                    .and_then(|definition| definition.template.as_ref())
                    .map(|template| template.goals.len()),
                a.store
                    .runs()
                    .iter()
                    .find(|candidate| candidate.id == run_id)
                    .map(|run| run.status.clone()),
                a.store
                    .research_requests()
                    .iter()
                    .find(|candidate| candidate.id == request)
                    .and_then(|request| request.planning_run_id),
            )
        });
    assert!(plan.is_none(), "the discarded plan must not repopulate the draft");
    assert_eq!(step, 2, "a stale completion must not jump the wizard");
    assert_eq!(planning_automation, None);
    assert_eq!(planning_run, None);
    assert_eq!(template_goals, Some(2), "the valid saved plan result is retained");
    assert_eq!(run_status.as_deref(), Some("Succeeded"), "the planning run is not cancelled");
    assert_eq!(request_run, Some(run_id), "request history keeps its planning run");
    // Switching back after completion still must not resurrect the old plan.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.select_automation_shape(crate::automation::AutomationShape::Quick, cx);
            a.select_automation_shape(crate::automation::AutomationShape::Research, cx);
            assert!(a.automation_ui.plan.is_none());
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("late planning completion passed");
}

fn shape_revision_atomic_check(root: &Path) {
    let directory = root.join("shape-revision-atomic");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = model_store(&directory);
    let id = draft(&mut store, "Atomic shape revision");
    enable(&mut store, id);
    let before = store.automation(id).unwrap().clone();

    let quick = crate::automation_execution::build_quick_template(
        "Check fuel prices",
        "A sourced price",
    )
    .unwrap();
    let error = store
        .revise_automation_with_shape(
            id,
            before.project_id,
            "Atomic shape revision".into(),
            before.objective.clone(),
            before.expected_result.clone(),
            before.schedule.clone(),
            before.policy.clone(),
            Some(quick),
            now() + 1,
            Some(crate::automation::AutomationShape::Research),
        )
        .unwrap_err();
    assert!(
        error.contains("3–12") || error.to_lowercase().contains("research plan"),
        "unexpected rejection: {error}"
    );
    assert_eq!(
        store.automation(id),
        Some(&before),
        "a rejected Research/Quick shape-template pair leaves the whole definition unchanged"
    );

    let research = before.template.clone().unwrap();
    let error = store
        .revise_automation_with_shape(
            id,
            before.project_id,
            "Atomic shape revision".into(),
            before.objective.clone(),
            before.expected_result.clone(),
            before.schedule.clone(),
            before.policy.clone(),
            Some(research),
            now() + 2,
            Some(crate::automation::AutomationShape::Quick),
        )
        .unwrap_err();
    assert!(
        error.to_lowercase().contains("quick task"),
        "unexpected rejection: {error}"
    );
    assert_eq!(
        store.automation(id),
        Some(&before),
        "the second rejected shape-template pair leaves the whole definition unchanged"
    );
    println!("atomic shape revision rejection passed");
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
                a.set_scope_identity(Scope::Automations);
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
    // Automations is a sibling of Research's All, so its list has no All crumb
    // and no Back action to a page it does not belong to.
    cx.update_window(handle, |_, w, cx| {
        let (crumbs, current) = app.read(cx).navigation_path();
        assert!(
            crumbs.is_empty(),
            "automation list has no parent crumbs, found {}",
            crumbs.len()
        );
        assert_eq!(current, "Automations");
        assert!(w.try_find("navigation-back").is_none());
    })
    .unwrap();

    let long_name = "Daily industry brief and unusual changes for nontechnical readers with sources, time range and evidence gaps";
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
        w.input("Track notable public information changes with sources, time range and uncertainty.", cx);
        w.click("automation-expected", cx);
        w.input("A three-minute brief with sources and evidence gaps.", cx);
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
            {
                let mut definition = a.store.automation_mut(id).unwrap();
                definition.anchor = Some(now - 660);
                definition.cursor = Some(now - 60);
            }
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
        read_state(&mut cx, &app, |a| a.period_overlay()),
        Some(occurrence)
    );
    assert!(!read_state(&mut cx, &app, |a| a.store.occurrence(occurrence).unwrap().unread));
    screenshot(&mut cx, handle, &directory, "03-result.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("close-automation-result", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.period_overlay()), None);

    // Editing creates a new version and requires a new approval. Edit is opened
    // from the settings dialog, next to the settings it changes.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-settings", cx);
        w.render_frame(cx);
        w.click("edit-automation", cx);
        w.render_frame(cx);
        w.click("automation-name", cx);
        w.input(" (second version)", cx);
        w.click("automation-objective", cx);
        w.input("Also record evidence gaps.", cx);
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
    assert!(name.ends_with(" (second version)"), "{name}");

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

    // Automation-owned steps are archived with their automation, never on their own.
    let step = read_state(&mut cx, &app, |a| {
        a.store
            .tasks()
            .iter()
            .find(|t| t.automation_id == Some(id))
            .map(|t| t.id)
    })
    .expect("automation-owned step");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.request_archive(crate::archive_app::ArchiveTarget::Task(step), w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    assert!(
        !read_state(&mut cx, &app, |a| a.store.task_is_archived(step)),
        "an automation step is not a Research archive target"
    );
    assert!(read_state(&mut cx, &app, |a| {
        a.toast.contains("archived with their automation")
    }));

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
        w.input("Verify that failed storage does not show a saved status.", cx);
        w.click("automation-expected", cx);
        w.input("Keep the draft and show the error.", cx);
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
        let content = w.find("automation-detail-scroll").bounds();
        let run = w.find("run-automation").bounds();
        let settings = w.find("automation-settings").bounds();
        let expected_right = content.right() - px(24.);
        assert!(
            (settings.right() - expected_right).abs() <= px(1.),
            "detail actions should end at the content's right inset: settings={settings:?}, content={content:?}"
        );
        assert!(
            run.top() == settings.top(),
            "standard-width actions should stay on one line"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "05-detail-enabled.png");
    // Leaving the wizard while the planner runs must keep the in-flight state
    // visible on the saved draft, in the list and on its detail page.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.planning_automation = Some(id);
            a.navigate(Scope::Automations, w, cx);
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "05a-list-planning.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_automation(id, w, cx));
        w.render_frame(cx);
        assert!(
            w.try_find("review-automation-planning").is_some(),
            "the automation detail keeps the in-flight plan visible"
        );
        w.click("review-automation-planning", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).automation_ui.step, 3);
        assert!(app.read(cx).automation_ui.creating);
        app.update(cx, |a, cx| {
            a.automation_ui.planning_automation = None;
            a.open_automation(id, w, cx);
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
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
            selected.contains("Track"),
            "the objective should be selectable, got {selected:?}"
        );
        w.press("cmd-c", cx);
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        assert!(
            copied.contains("Track"),
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
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::Automations
    );
    assert!(!read_state(&mut cx, &app, |a| a.automation_ui.creating));
    assert!(read_state(&mut cx, &app, |a| a.settings_overlay()));
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.selected),
        Some(id)
    );

    let (automation_run, automation_task) = read_state(&mut cx, &app, |a| {
        a.store
            .runs()
            .iter()
            .rev()
            .find(|run| run.automation_occurrence_id.is_some())
            .map(|run| (run.id, run.task_id))
            .expect("the automation has a run to open")
    });
    // Switching to Research results must not keep a period step selected; the
    // ordinary results view falls back to its own latest run.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.note_selected_run(Some(automation_run));
            a.set_result_selection(Some(automation_task), a.selected_run());
            a.navigate(Scope::Runs, w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(read_state(&mut cx, &app, |a| a.selected_run()), None);
    assert_eq!(read_state(&mut cx, &app, |a| a.task_result_context()), None);

    // Opening one automation step from its period still shows only that step:
    // automation periods have no project/topic history to fill the column.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.set_result_selection(Some(automation_task), a.selected_run());
            a.set_scope_identity(Scope::Runs);
            a.note_selected_run(Some(automation_run));
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
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::AutomationRuns
    );
    assert_eq!(read_state(&mut cx, &app, |a| a.title()), "Automation results");
    let (crumbs, current) = read_state(&mut cx, &app, |a| a.navigation_path());
    assert_eq!(current, "Automation results");
    assert_eq!(
        crumbs.iter().map(|(_, title, _)| title.as_str()).collect::<Vec<_>>(),
        vec!["Automations"],
        "automation results must not borrow Research's All crumb"
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let now = chrono::Utc::now().timestamp();
            for index in 0..40 {
                if let Ok(occurrence) = a.store.request_manual_occurrence(
                    id,
                    &format!("scroll-fixture-{index}"),
                    now + index,
                ) {
                    if let Some(mut value) = a.store.occurrence_mut(occurrence.id) {
                        value.status = crate::automation::OccurrenceStatus::Cancelled;
                    }
                }
            }
            cx.notify();
        });
    })
    .unwrap();
    let (latest, before_scroll) = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.automation_ui.result_filter = Some(id);
                a.automation_ui.history_limit = 60;
                cx.notify();
            });
            w.render_frame(cx);
            let before_list_scroll = app
                .read(cx)
                .automation_ui
                .result_list
                .scroll_px_offset_for_scrollbar()
                .y;
            w.scroll(
                "automation-results-scroll",
                ScrollDelta::Pixels(point(px(0.), px(-40.))),
                cx,
            );
            w.render_frame(cx);
            w.render_frame(cx);
            let after_list_scroll = app
                .read(cx)
                .automation_ui
                .result_list
                .scroll_px_offset_for_scrollbar()
                .y;
            assert!(
                after_list_scroll <= before_list_scroll - px(20.),
                "wheel must move the virtual results list; before={before_list_scroll:?} after={after_list_scroll:?}"
            );
            let candidates: Vec<u64> = app
                .read(cx)
                .store
                .all_occurrences()
                .iter()
                .filter(|occurrence| {
                    occurrence.automation_id == id
                        && (occurrence.research_goal_id.is_some()
                            || !occurrence.research_goal_ids.is_empty())
                })
                .map(|occurrence| occurrence.id)
                .collect();
            assert!(
                !candidates.is_empty(),
                "fixture must keep goal-bearing periods for the filtered automation"
            );
            let mut chosen = None;
            for _ in 0..60 {
                chosen = candidates.iter().copied().find(|candidate| {
                    w.try_find(format!("open-occurrence-{candidate}"))
                        .is_some_and(|snapshot| {
                            let bounds = snapshot.bounds();
                            snapshot.visible()
                                && bounds.top() > px(60.)
                                && bounds.bottom() < px(880.)
                                && bounds.right() < px(1420.)
                        })
                });
                if chosen.is_some() {
                    break;
                }
                w.scroll(
                    "automation-results-scroll",
                    ScrollDelta::Pixels(point(px(0.), px(-100.))),
                    cx,
                );
                w.render_frame(cx);
            }
            let latest = chosen.expect(
                "no visible period with research steps after scrolling the results list",
            );
            w.render_frame(cx);
            let before_scroll = app.read(cx).automation_ui.scroll.offset().y;
            let list_scroll = app
                .read(cx)
                .automation_ui
                .result_list
                .scroll_px_offset_for_scrollbar()
                .y;
            assert!(
                before_scroll < px(-10.),
                "automation results must actually scroll before the drill-down; got {before_scroll:?}"
            );
            assert!(
                (before_scroll - list_scroll).abs() <= px(1.),
                "ScrollHandle must mirror the virtual list offset; handle={before_scroll:?} list={list_scroll:?}"
            );
            w.render_frame(cx);
            let target = ElementId::Name(format!("open-occurrence-{latest}").into());
            let row_in_view = |w: &Window| {
                w.try_find(target.clone()).is_some_and(|snapshot| {
                    let bounds = snapshot.bounds();
                    let viewport = w.viewport_size();
                    snapshot.visible()
                        && bounds.top() > px(0.)
                        && bounds.bottom() < viewport.height
                        && bounds.left() > px(0.)
                        && bounds.right() < viewport.width
                })
            };
            assert!(
                row_in_view(w),
                "the scrolled period row must be fully visible before activation"
            );
            // The same helper render and pointer move must not change visibility.
            w.render_frame(cx);
            assert!(
                row_in_view(w),
                "the scrolled period row must stay visible across frames"
            );
            w.click(format!("open-occurrence-{latest}"), cx);
            w.render_frame(cx);
            assert_eq!(
                app.read(cx).period_overlay(),
                Some(latest),
                "clicking the visible period row opens its result"
            );
            (latest, before_scroll)
        })
        .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.period_overlay()),
        Some(latest)
    );
    screenshot(&mut cx, handle, &directory, "07-automation-results.png");
    // A research period lists its split steps with full titles. Opening one and
    // using Back restores the period result, not the automation list.
    let step = read_state(&mut cx, &app, |a| {
        let occurrence = a.store.occurrence(latest).unwrap();
        occurrence
            .research_goal_id
            .into_iter()
            .chain(occurrence.research_goal_ids.iter().copied())
            .flat_map(|goal| a.store.research_children(goal))
            .next()
            .expect("the period has a research step")
    });
    cx.update_window(handle, |_, w, cx| {
        let open = format!("automation-step-{step}");
        if w.try_find(open.clone()).is_none() && w.try_find("automation-completed-steps").is_some() {
            w.click("automation-completed-steps", cx);
            w.render_frame(cx);
        }
        assert!(
            w.try_find(ElementId::Name(open.clone().into())).is_some(),
            "a research period lists its split steps"
        );
        w.click(ElementId::Name(open.into()), cx);
        w.render_frame(cx);
        let (crumbs, _) = app.read(cx).navigation_path();
        assert!(
            crumbs
                .iter()
                .any(|(_, title, _)| title == &format!("Period {latest}")),
            "a step breadcrumb points at its period"
        );
        assert!(
            !crumbs.iter().any(|(_, title, _)| title == "All research"),
            "automation steps must not claim a Research crumb"
        );
        w.click("navigation-back", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.period_overlay()),
        Some(latest),
        "Back from a step returns to its period result"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::AutomationRuns,
        "a period opened from Automation results must return to that results page"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.result_filter),
        Some(id),
        "Back must preserve the results filter"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.automation_ui.scroll.offset().y),
        before_scroll,
        "Back must preserve the exact results scroll"
    );
    cx.update_window(handle, |_, w, cx| {
        assert!(
            w.try_find(format!("open-occurrence-{latest}")).is_some(),
            "the original filtered result row remains visible"
        );
        w.click("close-automation-result", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        assert_eq!(app.read(cx).automation_ui.result_filter, Some(id));
        assert!(
            w.try_find(format!("open-occurrence-{latest}")).is_some(),
            "closing the period returns to the same filtered results list"
        );
        assert_eq!(
            app.read(cx).automation_ui.scroll.offset().y,
            before_scroll,
            "Close must preserve the exact results scroll"
        );
    })
    .unwrap();
    // Separate detail-origin case: a period opened from the automation detail
    // returns to that detail, not to Automation results.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_automation(id, w, cx));
        w.render_frame(cx);
        let target = ElementId::Name(format!("open-occurrence-{latest}").into());
        let target_in_view = |w: &Window| {
            w.try_find(target.clone()).is_some_and(|snapshot| {
                let bounds = snapshot.bounds();
                let viewport = w.viewport_size();
                snapshot.visible()
                    && bounds.top() > px(0.)
                    && bounds.bottom() < viewport.height
                    && bounds.left() > px(0.)
                    && bounds.right() < viewport.width
            })
        };
        // The automation detail is a real scrolling page; bring the period row
        // into view with bounded increments instead of trusting an off-screen row.
        for _ in 0..40 {
            if target_in_view(w) {
                break;
            }
            w.scroll(
                "automation-detail-scroll",
                ScrollDelta::Pixels(point(px(0.), px(-400.))),
                cx,
            );
            w.render_frame(cx);
        }
        assert!(
            target_in_view(w),
            "the detail period row must be scrolled into view before activation"
        );
        w.click(format!("open-occurrence-{latest}"), cx);
        w.render_frame(cx);
        if w.try_find(format!("automation-step-{step}")).is_none() && w.try_find("automation-completed-steps").is_some() {
            w.click("automation-completed-steps", cx);
            w.render_frame(cx);
        }
        w.click(format!("automation-step-{step}"), cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).period_overlay(), Some(latest));
        w.click("close-automation-result", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Automations);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-results", cx);
        w.render_frame(cx);
    })
    .unwrap();
    // The results header narrows the list to one automation without leaving the page.
    cx.update_window(handle, |_, w, cx| {
        assert!(
            w.try_find("automation-results-filter").is_some(),
            "Automation results expose an automation filter"
        );
        app.update(cx, |a, cx| {
            a.automation_ui.result_filter = Some(id);
            cx.notify();
        });
        w.render_frame(cx);
        let open = format!("open-occurrence-{latest}");
        assert!(
            w.try_find(ElementId::Name(open.into())).is_some(),
            "the selected automation keeps its periods visible"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "07b-automation-results-filtered.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.result_filter = Some(copy);
            cx.notify();
        });
        w.render_frame(cx);
        let open = format!("open-occurrence-{latest}");
        assert!(
            w.try_find(ElementId::Name(open.into())).is_none(),
            "another automation hides periods that do not belong to it"
        );
        app.update(cx, |a, cx| {
            a.automation_ui.result_filter = None;
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("navigation-back", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::Automations
    );

    // A period waiting for its scheduled retry is cancellable directly from
    // Research results, without pausing the automation or stopping a process.
    let waiting = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let now = chrono::Utc::now().timestamp();
                let occurrence = a
                    .store
                    .request_manual_occurrence(id, "waiting-retry", now)
                    .unwrap();
                a.store.occurrence_mut(occurrence.id).unwrap().mark_running(now).unwrap();
                a.store
                    .wait_for_retry(occurrence.id, "Waiting for the approved step retry.")
                    .unwrap();
                a.persist_if_changed(true);
                cx.notify();
                occurrence.id
            })
        })
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-results", cx);
        w.render_frame(cx);
    })
    .unwrap();
    // Active rows stay read-only: the period dialog owns the cancel action so a
    // running list cannot stop work with a single stray click.
    screenshot(&mut cx, handle, &directory, "07c-automation-results-active.png");
    cx.update_window(handle, |_, w, cx| {
        // The newest period is appended at the end of the results list; scroll
        // the list to it instead of inheriting the restored offset.
        w.scroll(
            "automation-results-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
            cx,
        );
        w.render_frame(cx);
        // The period exposes its unfinished steps with a resume control above them.
        let open = format!("open-occurrence-{waiting}");
        assert!(
            w.try_find(ElementId::Name(open.clone().into()))
                .is_some_and(|snapshot| snapshot.visible()),
            "the newest active period is reachable in the results list"
        );
        w.click(ElementId::Name(open.into()), cx);
        w.render_frame(cx);
        assert!(
            w.try_find("resume-automation-steps").is_some(),
            "a period with unfinished steps offers Resume above the step list"
        );
        assert!(
            w.try_find("cancel-automation-result").is_some(),
            "the period dialog offers the cancel action for an active period"
        );
        w.click("cancel-automation-result", cx);
        w.render_frame(cx);
        w.click("close-automation-result", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.occurrence(waiting).unwrap().status),
        OccurrenceStatus::Cancelled
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.scope()),
        Scope::AutomationRuns,
        "cancelling from the period dialog keeps the user on Automation results"
    );
    assert_eq!(
        read_state(&mut cx, &app, |a| a.period_overlay()),
        None,
        "closing the dialog leaves no modal behind"
    );
    // Resume retries the same period in place instead of creating a new one.
    let periods_before = read_state(&mut cx, &app, |a| a.store.all_occurrences().len());
    cx.update_window(handle, |_, w, cx| {
        w.scroll(
            "automation-results-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
            cx,
        );
        w.render_frame(cx);
        let open = format!("open-occurrence-{waiting}");
        w.click(ElementId::Name(open.into()), cx);
        w.render_frame(cx);
        w.click("resume-automation-steps", cx);
        w.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        read_state(&mut cx, &app, |a| a.store.all_occurrences().len()),
        periods_before,
        "Resume must reopen the same period, not queue a new manual occurrence"
    );
    assert_ne!(
        read_state(&mut cx, &app, |a| a.store.occurrence(waiting).unwrap().status),
        OccurrenceStatus::Cancelled,
        "the cancelled period is queued again in place"
    );
    path
}

/// Settings metadata remains readable without crowding the task or footer actions.
fn settings_hierarchy_fixture(root: &Path) {
    use gpui_kit::test::TestEventExt;
    for shape in [AutomationShape::Quick, AutomationShape::Research] {
        for (width, height) in [(1440., 920.), (900., 620.)] {
            for mode in [Appearance::Light, Appearance::Dark] {
                let name = format!("settings-{}-{}-{}",
                    if shape == AutomationShape::Quick { "quick" } else { "research" },
                    if mode == Appearance::Light { "light" } else { "dark" }, width as u32);
                let directory = root.join(name);
                std::fs::create_dir_all(&directory).unwrap();
                let path = directory.join("workspace.sqlite3");
                let mut store = model_store(&directory);
                store.set_appearance(mode);
                let objective = "Compare fuel prices across two cities and summarize recent changes with dated sources.";
                let expected = "A concise comparison table with dates, sources and evidence gaps.";
                let template = if shape == AutomationShape::Quick {
                    crate::automation_execution::build_quick_template(objective, expected).unwrap()
                } else {
                    crate::automation_execution::build_template(objective, expected).unwrap()
                };
                let id = store.create_automation_draft(1, shape, "Fuel price review".into(),
                    objective.into(), expected.into(), crate::automation_schedule::Schedule {
                        spec: crate::automation_schedule::ScheduleSpec::Daily { hour: 10, minute: 0 },
                        timezone: "Asia/Shanghai".into(),
                    }, Default::default(), Some(template), now()).unwrap();
                enable(&mut store, id);
                let mut definition = store.automation_mut(id).unwrap();
                let approval = definition.approval.as_mut().unwrap();
                approval.models = std::array::from_fn(|_| "provider-with-a-long-name/research-model-with-an-extended-version-identifier".into());
                approval.agents = std::array::from_fn(|_| Some("OpenCode".into()));
                drop(definition);
                Storage::new(path.clone()).save(&store).unwrap();
                let mut cx = HeadlessAppContext::with_platform(
                    platform::current_platform(true).text_system(),
                    Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer);
                cx.update(init);
                let mut app = None;
                let handle: AnyWindowHandle = cx.open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        a.automation_fake_mode = true;
                        a.open_automation(id, w, cx);
                        cx.notify();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                }).unwrap().into();
                let app = app.unwrap();
                cx.update_window(handle, |_, w, cx| {
                    w.render_frame(cx);
                    if width >= 1440. {
                        let content = w.find("automation-detail-scroll").bounds();
                        let run = w.find("run-automation").bounds();
                        let settings = w.find("automation-settings").bounds();
                        let expected_right = content.right() - px(24.);
                        assert!(
                            (settings.right() - expected_right).abs() <= px(1.),
                            "detail actions should end at the content's right inset: settings={settings:?}, content={content:?}"
                        );
                        assert!(
                            run.top() == settings.top(),
                            "standard-width actions should stay on one line"
                        );
                    }
                }).unwrap();
                screenshot(&mut cx, handle, &directory, "detail-header.png");
                cx.update_window(handle, |_, w, cx| {
                    w.click("automation-settings", cx);
                    w.render_frame(cx);
                    assert!(w.find("edit-automation").visible());
                    let schedule = w.find("automation-settings-schedule");
                    assert!(schedule.visible());
                    assert!(schedule.bounds().right() < px(width));
                    if shape == AutomationShape::Quick {
                        assert!(w.try_find("planning-attempt-history").is_none(),
                            "quick tasks should not show irrelevant empty planning history");
                    }
                }).unwrap();
                screenshot(&mut cx, handle, &directory, "settings-top.png");
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.automation_ui.settings_scroll.set_offset(point(px(0.), px(-4000.)));
                        cx.notify();
                    });
                    w.render_frame(cx);
                }).unwrap();
                screenshot(&mut cx, handle, &directory, "settings-execution.png");
                cx.update_window(handle, |_, w, cx| {
                    assert!(w.find("automation-execution-help").visible());
                    assert!(w.find("edit-automation").visible());
                    let model = w.find("automation-execution-model-3");
                    let agent = w.find("automation-execution-agent-3");
                    assert!(model.visible() && agent.visible());
                    assert!(model.bounds().right() <= px(width) && agent.bounds().right() <= px(width),
                        "long model names must remain within the dialog viewport");
                    let help = w.find("automation-execution-help").bounds();
                    w.pointer_move(help.center(), None, cx);
                }).unwrap();
                cx.run_until_parked();
                cx.advance_clock(Duration::from_millis(600));
                cx.run_until_parked();
                cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
                screenshot(&mut cx, handle, &directory, "settings-help.png");
                cx.update_window(handle, |_, w, cx| {
                    let model = w.find("automation-execution-model-3").bounds();
                    w.pointer_move(model.center(), None, cx);
                }).unwrap();
                cx.run_until_parked();
                cx.advance_clock(Duration::from_millis(600));
                cx.run_until_parked();
                cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
                screenshot(&mut cx, handle, &directory, "settings-model-help.png");
                cx.update_window(handle, |_, w, cx| {
                    w.click("close-automation-settings", cx);
                    w.render_frame(cx);
                    assert!(!app.read(cx).settings_overlay());
                }).unwrap();
                cx.update(|cx| cx.quit());
            }
        }
    }
}

/// A compact list matrix isolates schedule metadata from current run state.
fn list_hierarchy_fixture(root: &Path) {
    let directory = root.join("list-fixture");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = model_store(&directory);
    for (name, status, paused) in [
        ("Daily market summary", Some(OccurrenceStatus::Succeeded), false),
        ("A long automation name that must leave room for its type and enabled status", Some(OccurrenceStatus::Failed), false),
        ("New automation", None, false),
        ("Paused weekly review", Some(OccurrenceStatus::Succeeded), true),
    ] {
        let id = draft(&mut store, name);
        enable(&mut store, id);
        if let Some(status) = status {
            let occurrence = store.request_manual_occurrence(id, &format!("list-{id}"), now() - 600).unwrap();
            let mut occurrence = store.occurrence_mut(occurrence.id).unwrap();
            occurrence.status = status;
            occurrence.actual_started_at = Some(now() - 590);
        }
        if paused {
            store.pause_automation(id, now()).unwrap();
        }
    }
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    render_matrix(root, &path);
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
            store.set_appearance(mode);
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
                        a.set_scope_identity(Scope::Automations);
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
            cx.update_window(handle, |_, w, cx| {
                for definition in app.read(cx).store.automations() {
                    let schedule = w.find(format!("automation-schedule-{}", definition.id));
                    let latest = w.find(format!("automation-latest-{}", definition.id));
                    assert!(schedule.bounds().origin.y < latest.bounds().origin.y,
                        "schedule and runtime information occupy separate rows");
                    assert!(latest.bounds().right() <= px(width),
                        "runtime summary stays inside the compact window");
                }
            }).unwrap();
            if width == 1000. && std::env::var_os("PLANNING_QA_LIST_ONLY").is_some() {
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.automation_ui.scroll.set_offset(point(px(0.), px(-1000.)));
                        cx.notify();
                    });
                    w.render_frame(cx);
                }).unwrap();
                screenshot(&mut cx, handle, &directory, "list-scrolled.png");
            }
            let id = read_state(&mut cx, &app, |a| a.store.automations()[0].id);
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.open_automation(id, w, cx);
                    assert_eq!(a.automation_detail_id(), Some(id));
                    cx.notify();
                });
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "detail.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.navigate(Scope::AutomationRuns, w, cx));
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "results.png");
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
    let template = crate::automation_execution::build_template("Capacity check", "Concise report").unwrap();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Thirty-day minute-level capacity check".into(),
            "Capacity check".into(),
            "Concise report".into(),
            schedule.clone(),
            Default::default(),
            Some(template.clone()),
            now(),
        )
        .unwrap();
    enable(&mut store, id);
    let start = now() - 30 * 86_400;
    let report = "## Capacity sample\n\n".to_string()
        + &"Representative report paragraph for list, detail and save cost. ".repeat(120);
    let periods = std::env::var("PLANNING_QA_PERIODS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(43_200)
        .clamp(1, 86_400);
    for index in 0..periods {
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
                objective: "Capacity check".into(),
                expected_result: "Concise report".into(),
                shape: Some(crate::automation::AutomationShape::Research),
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
            retry_requested_at: None,
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
                    planning_owner: None,
                    deadline_at: None,
                    auto_retry_attempt: 0,
                    retry_not_before: None,
                    project_plan: None,
                    id: store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1,
                    task_id: task,
                    task_title: "Capacity sample".into(),
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
                store.push_run(run);
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
    assert_eq!(loaded.all_occurrences().len(), periods as usize);
    let list_started = std::time::Instant::now();
    let mut latest: Vec<_> = loaded.all_occurrences().iter().collect();
    latest.sort_by_key(|occurrence| std::cmp::Reverse(occurrence.id));
    let first_page: Vec<_> = latest.into_iter().take(20).collect();
    let list_ms = list_started.elapsed().as_millis();
    assert_eq!(first_page.len(), 20);
    let db_bytes = std::fs::metadata(&path).unwrap().len();
    let materialized_occurrences = loaded
        .all_occurrences()
        .iter()
        .filter(|occurrence| !occurrence.run_ids.is_empty())
        .count();
    println!(
        "performance fixture: {periods} occurrences ({materialized_occurrences} with full research entities) + representative reports; cold save {cold_save_ms} ms; warm saves {:?} ms; load {load_ms} ms; history page query {list_ms} ms; db {:.1} MB; directory {}",
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
                a.set_scope_identity(crate::app::Scope::Automations);
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
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_automation(id, w, cx);
            assert_eq!(a.automation_detail_id(), Some(id));
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
            if let Some(mut definition) = a.store.automation_mut(id) {
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
