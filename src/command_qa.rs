//! T13 S1 isolated executor QA: persistence failure, pending retry, and stop
//! safety through the real `TaskApp` storage path. No model or external process
//! is started; fake/headless execution only.

use crate::app::{Scope, TaskApp};
use crate::automation::{
    AutomationOccurrence, AutomationPolicy, AutomationShape, AutomationTemplate, OccurrenceStatus,
    TriggerSource, WindowRule,
};
use crate::automation_schedule::{Schedule, ScheduleSpec};
use crate::domain::{AgentRun, Stage, Store};
use crate::persistence::Storage;
use crate::research::ResearchKind;
use crate::stop_intent;
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/command-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    executor_checks(&root);
    journal_ordering_check(&root);
    multi_stop_failure_check(&root);
    terminal_failure_retains_journal_check(&root);
    dispatch_checks(&root);
    pending_dispatch_preserves_prior_intent(&root);
    typed_intent_checks(&root);
    automation_command_invalidates_undo_check(&root);
    println!("Command QA passed · {}", root.display());
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn base_store(directory: &Path) -> Store {
    let mut store = Store::empty();
    let project_directory = directory.join("workspace/projects/project-1");
    std::fs::create_dir_all(&project_directory).unwrap();
    store.project_at_mut(0).unwrap().directory = project_directory.to_string_lossy().into_owned();
    store.set_default_model("qa/model".into());
    store.research_config_mut().models = [
        "qa/model".into(),
        "qa/model".into(),
        "qa/model".into(),
        "qa/model".into(),
    ];
    store
}

fn request_plan_fixture(directory: &Path) -> (Store, u64) {
    let mut store = base_store(directory);
    let request = store
        .create_research_request(1, None, "Compare two public sources and cite both.")
        .unwrap();
    (store, request)
}

fn regeneration_fixture(
    directory: &Path,
    automation: bool,
    material: bool,
) -> (Store, u64, Option<u64>, Option<std::path::PathBuf>) {
    let (mut store, occurrence_id_opt) = if automation {
        let (store, occurrence, _) = dispatch_store(directory);
        (store, Some(occurrence.id))
    } else {
        let mut store = base_store(directory);
        let topic = store.create_topic(1, "Regeneration topic").unwrap();
        let goal = store
            .create_research_goal_in_topic(
                "Regeneration goal".into(),
                "Regenerate a report.".into(),
                1,
                Some(topic),
            )
            .unwrap();
        store.apply_research_plan(goal, PLAN).unwrap();
        (store, None)
    };
    let advise = store
        .tasks()
        .iter()
        .find(|task| {
            task.research
                .as_ref()
                .is_some_and(|research| research.kind == ResearchKind::Advise)
        })
        .map(|task| task.id)
        .unwrap();
    let goal_id = store
        .task(advise)
        .and_then(|task| task.research.as_ref())
        .map(|research| research.goal_id)
        .unwrap();
    if let Some(occurrence_id) = occurrence_id_opt {
        let mut value = occurrence(occurrence_id, 0);
        value.run_ids.clear();
        value.status = OccurrenceStatus::Succeeded;
        value.research_goal_id = Some(goal_id);
        value.research_goal_ids = vec![goal_id];
        store.push_occurrence(value);
    }
    // Create genuine completed prerequisite runs in phase/dependency order.
    let mut prerequisites: Vec<u64> = store
        .tasks()
        .iter()
        .filter(|task| {
            task.research.as_ref().is_some_and(|research| {
                research.goal_id == goal_id
                    && research.kind != ResearchKind::Advise
                    && research.kind != ResearchKind::Goal
            })
        })
        .map(|task| task.id)
        .collect();
    prerequisites.sort_by_key(|id| {
        let kind = store
            .task(*id)
            .and_then(|task| task.research.as_ref())
            .map(|research| research.kind)
            .unwrap_or(ResearchKind::Collect);
        ResearchKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or(0)
    });
    for id in prerequisites {
        let mut completed = store.prepare_run(id).unwrap();
        completed.status = "Succeeded".into();
        completed.output = format!(
            "Completed {} evidence with a retained source.",
            completed
                .research_kind
                .map(|kind| kind.label())
                .unwrap_or("research")
        );
        completed.input_signature = store.research_signature(id).unwrap();
        store.push_run(completed);
        store.set_stage(id, Stage::Done).unwrap();
    }
    let material_path = material.then(|| install_material(&mut store, directory, advise));
    let mut source = store.prepare_run(advise).unwrap();
    source.id = 100;
    source.status = "Succeeded".into();
    source.output = serde_json::json!({
        "status": "complete",
        "report": "Prior validated report body with source-linked evidence.",
        "blocker": null,
        "sources": [{
            "title": "Example source",
            "url": "https://example.com/source",
            "published_at": "unknown"
        }]
    })
    .to_string();
    source.input_signature = store.research_signature(advise).unwrap();
    if let Some(occurrence_id) = occurrence_id_opt {
        source.automation_occurrence_id = Some(occurrence_id);
        if let Some(mut value) = store.occurrence_mut(occurrence_id) {
            value.status = OccurrenceStatus::Succeeded;
        }
    }
    store.push_run(source);
    (store, 100, occurrence_id_opt, material_path)
}

fn automation_store(directory: &Path) -> (Store, u64) {
    let mut store = Store::empty();
    let project_directory = directory.join("workspace/projects/project-1");
    std::fs::create_dir_all(&project_directory).unwrap();
    store.project_at_mut(0).unwrap().directory = project_directory.to_string_lossy().into_owned();
    store.set_default_model("qa/automation".into());
    store.research_config_mut().models = [
        "qa/automation".into(),
        "qa/automation".into(),
        "qa/automation".into(),
        "qa/automation".into(),
    ];
    let template = crate::automation_execution::build_quick_template(
        "Check one changed public value and cite its source.",
        "One short answer with a source.",
    )
    .unwrap();
    let id = store
        .create_automation_draft(
            1,
            AutomationShape::Quick,
            "Command QA".into(),
            "Check one changed public value and cite its source.".into(),
            "One short answer with a source.".into(),
            Schedule {
                spec: ScheduleSpec::Interval { minutes: 10 },
                timezone: "UTC".into(),
            },
            AutomationPolicy::default(),
            Some(template),
            now(),
        )
        .unwrap();
    store.approve_and_enable(id, vec![], now()).unwrap();
    (store, id)
}

fn stop_store(directory: &Path) -> Store {
    let mut store = Store::mock();
    let project_directory = directory.join("workspace/projects/project-1");
    std::fs::create_dir_all(&project_directory).unwrap();
    store.project_at_mut(0).unwrap().directory = project_directory.to_string_lossy().into_owned();
    let run_id = 1;
    store.push_run(AgentRun {
        automation_occurrence_id: Some(10),
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
        id: run_id,
        task_title: "QA stop".into(),
        directory: directory.to_string_lossy().into_owned(),
        prompt: String::new(),
        queued_at: String::new(),
        workspace_source: None,
        started_at: crate::opencode::timestamp(),
        ended_at: None,
        session_id: None,
        exit_code: None,
        task_id: 1,
        agent: "OpenCode".into(),
        model: "qa/automation".into(),
        stage: Stage::InProgress,
        status: "Running".into(),
        logs: Vec::new(),
        output: String::new(),
        attachments: Vec::new(),
        research_kind: None,
        sources: Vec::new(),
        input_signature: String::new(),
    });
    store.push_occurrence(occurrence(10, run_id));
    store
        .push_automation(crate::automation::AutomationDefinition {
            id: 1,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Quick,
            name: "QA stop automation".into(),
            objective: "QA".into(),
            expected_result: "QA".into(),
            schedule: Schedule {
                spec: ScheduleSpec::Interval { minutes: 10 },
                timezone: "UTC".into(),
            },
            status: crate::automation::DefinitionStatus::Enabled,
            template: None,
            plan_binding: None,
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: Some(now() + 60),
            anchor: Some(now()),
            consecutive_failures: 0,
            attention_reason: None,
            created_at: now(),
            updated_at: now(),
        });
    store
}

const PLAN: &str = r#"{"tasks":[
    {"title":"Collect one","instructions":"Collect one.","kind":"collect","depends_on":[]},
    {"title":"Analyze","instructions":"Analyze.","kind":"analyze","depends_on":[0]},
    {"title":"Report","instructions":"Report.","kind":"advise","depends_on":[1]}
]}"#;

fn dispatch_store(directory: &Path) -> (Store, AutomationOccurrence, AgentRun) {
    let (mut store, automation) = automation_store(directory);
    let topic = store.create_topic(1, "Dispatch topic").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Dispatch goal".into(),
            "Check the dispatch fixture.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    store.task_mut(goal).unwrap().automation_id = Some(automation);
    for step in &steps {
        store.task_mut(*step).unwrap().automation_id = Some(automation);
    }
    let mut occ = occurrence(20, 0);
    occ.run_ids.clear();
    occ.status = OccurrenceStatus::Queued;
    occ.actual_started_at = None;
    occ.deadline_at = None;
    occ.research_goal_id = Some(goal);
    occ.research_goal_ids = vec![goal];
    occ.research_topic_id = Some(topic);
    // Prepare the run with the occurrence visible, then remove it. The test
    // attaches the occurrence after the app opens so startup recovery cannot
    // pre-queue it before exercising the dispatch boundary.
    store.push_occurrence(occ.clone());
    let run = store.prepare_occurrence_run(&occ, now()).unwrap();
    store.pop_occurrence();
    (store, occ, run)
}

fn occurrence(id: u64, run_id: u64) -> AutomationOccurrence {
    let now = now();
    AutomationOccurrence {
        id,
        automation_id: 1,
        definition_version: 1,
        source: TriggerSource::Manual,
        trigger_key: format!("manual:{id}"),
        scheduled_for: None,
        requested_at: Some(now),
        actual_started_at: Some(now),
        timezone: "UTC".into(),
        reference_time: now,
        data_window: crate::automation::TimeWindow::unresolved("qa"),
        snapshot: crate::automation::AutomationOccurrenceSnapshot {
            objective: "qa".into(),
            expected_result: "qa".into(),
            shape: Some(AutomationShape::Quick),
            template: AutomationTemplate {
                goals: Vec::new(),
                window_rule: WindowRule::Unresolved,
            },
            models: [
                "qa/automation".into(),
                "qa/automation".into(),
                "qa/automation".into(),
                "qa/automation".into(),
            ],
            agents: [None, None, None, None],
            materials: Vec::new(),
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "qa".into(),
        },
        status: OccurrenceStatus::Running,
        reason: None,
        research_goal_id: None,
        research_goal_ids: Vec::new(),
        research_topic_id: None,
        run_ids: vec![run_id],
        final_run_id: None,
        unread: false,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: now,
        deadline_at: None,
        retry_requested_at: None,
    }
}

fn app_for(
    path: &Path,
    workspace_root: &Path,
) -> (HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>) {
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
                a.workspace_root = Some(workspace_root.to_path_buf());
                a.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                a.set_scope_identity(Scope::Automations);
                a.automation_fake_mode = true;
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    (cx, window.into(), app.unwrap())
}

fn fail_automation_writes(path: &Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_automations BEFORE INSERT ON automations BEGIN SELECT RAISE(ABORT, 'qa injected write failure'); END;",
    )
    .unwrap();
}

fn allow_automation_writes(path: &Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("DROP TRIGGER reject_automations;")
        .unwrap();
}

fn executor_checks(root: &Path) {
    // Pending pause: the first save fails, no live definition change, no success.
    let directory = root.join("executor-pause");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, id) = automation_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    fail_automation_writes(&path);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.pause_automation_ui(id, cx));
        w.render_frame(cx);
    })
    .unwrap();
    let (status, pending, feedback) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.automation(id).unwrap().status,
            a.pending_commit.is_some(),
            a.automation_ui.feedback.clone(),
        )
    });
    assert_eq!(status, crate::automation::DefinitionStatus::Enabled);
    assert!(pending, "the failed command must remain pending");
    assert!(matches!(feedback, Some(Err(_))), "{feedback:?}");
    screenshot(&mut cx, handle, &directory, "pause-pending.png");
    allow_automation_writes(&path);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.retry_pending_saves(cx));
        w.render_frame(cx);
    })
    .unwrap();
    let (status, pending, feedback) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.automation(id).unwrap().status,
            a.pending_commit.is_some(),
            a.automation_ui.feedback.clone(),
        )
    });
    assert_eq!(status, crate::automation::DefinitionStatus::Paused);
    assert!(!pending);
    assert!(matches!(feedback, Some(Ok(_))), "{feedback:?}");
    screenshot(&mut cx, handle, &directory, "pause-retried.png");

    // Save a new draft through the app executor under the same failure case.
    let template = crate::automation_execution::build_quick_template(
        "Check a second public value.",
        "One short answer.",
    )
    .unwrap();
    let input = crate::domain_command::AutomationDraftInput {
        project: 1,
        shape: AutomationShape::Quick,
        name: "Command QA draft".into(),
        objective: "Check a second public value.".into(),
        expected_result: "One short answer.".into(),
        schedule: Schedule {
            spec: ScheduleSpec::Interval { minutes: 10 },
            timezone: "UTC".into(),
        },
        policy: AutomationPolicy::default(),
        template: Some(template),
    };
    let before_count = cx.update(|cx| app.read(cx).store.automations().len());
    fail_automation_writes(&path);
    let error = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.execute_command(
                crate::domain_command::Command::SaveDraft {
                    id: None,
                    input: input.clone(),
                    now: now(),
                },
                cx,
            )
        })
    });
    assert!(error.is_err());
    let (count, pending) = cx.update(|cx| {
        let a = app.read(cx);
        (a.store.automations().len(), a.pending_commit.is_some())
    });
    assert_eq!(
        count, before_count,
        "failed draft save must not add a definition"
    );
    assert!(pending);
    allow_automation_writes(&path);
    cx.update(|cx| app.update(cx, |a, cx| a.retry_pending_saves(cx)));
    let count = cx.update(|cx| app.read(cx).store.automations().len());
    assert_eq!(
        count,
        before_count + 1,
        "retry saves the retained typed draft once"
    );

    // Stop: external cancellation is requested, but a failed save is reported
    // truthfully and the journal prevents an automatic restart until retried.
    let directory = root.join("executor-stop");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone())
        .save(&stop_store(&directory))
        .unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.automation_fake = Some(crate::automation_runtime::FakeAutomationRunner::default());
            a.automation_started
                .insert(10, std::time::Instant::now());
        });
    });
    fail_automation_writes(&path);
    let outcome = cx.update(|cx| app.update(cx, |a, cx| a.stop_automation_safely(10, cx)));
    assert!(
        !outcome.is_durably_saved(),
        "a failed save must not claim a durable stop: {outcome:?}"
    );
    let (cancels, started) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_fake
                .as_ref()
                .map(|runner| runner.cancels.clone())
                .unwrap_or_default(),
            a.automation_started.contains_key(&10),
        )
    });
    assert_eq!(cancels, vec![10], "fake backend stop signal exactly once");
    assert!(!started, "automation_started cleanup must run");
    let (pending, intents, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.pending_stop.is_some(),
            a.stop_intents.len(),
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert!(pending, "stop retry intent must survive");
    assert_eq!(intents, 1);
    assert!(journal, "best-effort stop journal must exist");
    screenshot(&mut cx, handle, &directory, "stop-pending.png");
    allow_automation_writes(&path);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.retry_pending_saves(cx));
        w.render_frame(cx);
    })
    .unwrap();
    let (status, pending, intents, journal, feedback) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(10).unwrap().status,
            a.pending_stop.is_some(),
            a.stop_intents.len(),
            stop_intent::journal_path(&directory).exists(),
            a.automation_ui.feedback.clone(),
        )
    });
    assert_eq!(status, OccurrenceStatus::Cancelled);
    assert!(!pending);
    assert_eq!(intents, 0);
    assert!(!journal);
    assert!(
        matches!(feedback, Some(Ok(ref message)) if message.contains("Stopped")),
        "{feedback:?}"
    );
    screenshot(&mut cx, handle, &directory, "stop-retried.png");
}

fn journal_ordering_check(root: &Path) {
    // A valid journal is reconciled before recovery and clears only after an
    // explicit durable save.
    let directory = root.join("journal-valid");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone())
        .save(&stop_store(&directory))
        .unwrap();
    stop_intent::record(
        &directory,
        &stop_intent::StopRecord {
            occurrence: 10,
            run_ids: vec![1],
            requested_at: now(),
            reason: "qa stop".into(),
        },
    )
    .unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (status, journal, error) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(10).unwrap().status,
            stop_intent::journal_path(&directory).exists(),
            a.stop_journal_error.clone(),
        )
    });
    assert_eq!(status, OccurrenceStatus::Cancelled);
    assert!(!journal, "a durable reconciliation clears the journal");
    assert!(error.is_none());

    // A malformed journal keeps the app viewable but fail-closes recovery.
    let directory = root.join("journal-malformed");
    std::fs::create_dir_all(directory.join("workspace")).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone())
        .save(&stop_store(&directory))
        .unwrap();
    std::fs::write(stop_intent::journal_path(&directory), b"{bad json").unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (blocked, status, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.stop_journal_error.is_some(),
            a.store.occurrence(10).unwrap().status,
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert!(blocked, "malformed journal must fail closed");
    assert_eq!(
        status,
        OccurrenceStatus::Running,
        "the malformed journal must not silently cancel or replay the period"
    );
    assert!(journal, "the unresolved journal is retained for the user");
    screenshot(&mut cx, handle, &directory, "journal-blocked.png");
}

/// Two stopped periods, first reconciliation save fails, later storage works.
/// Recovery must stay blocked while any intent remains, and per-occurrence
/// admission must stay blocked even after storage_error is cleared.
fn multi_stop_failure_check(root: &Path) {
    let directory = root.join("multi-stop-failure");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let mut store = stop_store(&directory);
    let mut second_run = store.runs()[0].clone();
    second_run.id = 2;
    second_run.automation_occurrence_id = Some(11);
    store.push_run(second_run);
    store.push_occurrence(occurrence(11, 2));
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();

    // Re-open both periods in memory and add two journal records. The attached
    // app has no journal yet, so this isolates the reconciliation failure path.
    cx.update(|cx| {
        app.update(cx, |a, _| {
            for occurrence in [10, 11] {
                if let Some(mut value) = a.store.occurrence_mut(occurrence) {
                    value.status = OccurrenceStatus::Running;
                }
            }
        });
    });
    for occurrence in [10, 11] {
        stop_intent::record(
            &directory,
            &stop_intent::StopRecord {
                occurrence,
                run_ids: vec![if occurrence == 10 { 1 } else { 2 }],
                requested_at: now(),
                reason: "qa multi stop".into(),
            },
        )
        .unwrap();
    }

    // Fail every store write first. Both periods must remain unresolved and
    // recovery must not be reported ready.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_occurrences BEFORE INSERT ON automation_occurrences BEGIN SELECT RAISE(ABORT, 'qa multi failure'); END;",
    )
    .unwrap();
    drop(db);
    let ready = cx.update(|cx| app.update(cx, |a, cx| a.prepare_stop_recovery(cx)));
    let (intents, queued, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.stop_intents.len(),
            a.queued_runs.len(),
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert!(!ready, "recovery must not be ready with unresolved intents");
    assert_eq!(intents, 2);
    assert_eq!(queued, 0, "neither stopped period may dispatch");
    assert!(journal);

    // Even if a later save clears storage_error, the per-occurrence guard still
    // blocks admission for both stopped periods.
    cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.storage_error.clear();
            a.enqueue_automation_occurrence(10, cx);
            a.enqueue_automation_occurrence(11, cx);
        });
    });
    let queued = cx.update(|cx| app.read(cx).queued_runs.len());
    assert_eq!(
        queued, 0,
        "stop intents must block admission independent of storage_error"
    );

    // Fix storage and retry reconciliation. Now both are durable and the
    // journal is cleared; no period was replayed.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_occurrences;")
        .unwrap();
    drop(db);
    let ready = cx.update(|cx| app.update(cx, |a, cx| a.prepare_stop_recovery(cx)));
    assert!(ready, "reconciliation must succeed once storage works");
    let (statuses, intents, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            (
                a.store.occurrence(10).unwrap().status,
                a.store.occurrence(11).unwrap().status,
            ),
            a.stop_intents.len(),
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert_eq!(
        statuses,
        (OccurrenceStatus::Cancelled, OccurrenceStatus::Cancelled)
    );
    assert_eq!(intents, 0);
    assert!(!journal);
}

/// A failed first reconciliation leaves the occurrence terminal only in memory.
/// A second retry with the same storage failure must retain the journal; the
/// database must still show the replayable period, and only a later successful
/// save may clear it.
fn terminal_failure_retains_journal_check(root: &Path) {
    let directory = root.join("terminal-failure");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone())
        .save(&stop_store(&directory))
        .unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| {
        app.update(cx, |a, _| {
            if let Some(mut value) = a.store.occurrence_mut(10) {
                value.status = OccurrenceStatus::Running;
            }
        });
    });
    stop_intent::record(
        &directory,
        &stop_intent::StopRecord {
            occurrence: 10,
            run_ids: vec![1],
            requested_at: now(),
            reason: "qa terminal retry".into(),
        },
    )
    .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_occurrences BEFORE INSERT ON automation_occurrences BEGIN SELECT RAISE(ABORT, 'qa terminal failure'); END;",
    )
    .unwrap();
    drop(db);

    let first = cx.update(|cx| app.update(cx, |a, cx| a.prepare_stop_recovery(cx)));
    assert!(!first);
    let second = cx.update(|cx| app.update(cx, |a, cx| a.prepare_stop_recovery(cx)));
    assert!(
        !second,
        "second failed reconciliation must not report success"
    );
    let (memory_status, last_saved_status, intents, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(10).unwrap().status,
            a.last_saved.occurrence(10).unwrap().status,
            a.stop_intents.len(),
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert_eq!(memory_status, OccurrenceStatus::Cancelled);
    assert!(
        !last_saved_status.is_terminal(),
        "in-memory terminal state is not durability"
    );
    assert_eq!(intents, 1);
    assert!(journal, "the only durable stop record must be retained");
    let disk = Storage::new(path.clone()).load().unwrap().unwrap();
    let disk_status = disk.occurrence(10).unwrap().status;
    assert!(
        !disk_status.is_terminal() && disk_status != OccurrenceStatus::Cancelled,
        "the database still contains the replayable period, got {disk_status:?}"
    );

    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_occurrences;")
        .unwrap();
    drop(db);
    let ready = cx.update(|cx| app.update(cx, |a, cx| a.prepare_stop_recovery(cx)));
    assert!(ready);
    let (status, journal) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(10).unwrap().status,
            stop_intent::journal_path(&directory).exists(),
        )
    });
    assert_eq!(status, OccurrenceStatus::Cancelled);
    assert!(!journal);
    let disk = Storage::new(path).load().unwrap().unwrap();
    assert_eq!(
        disk.occurrence(10).unwrap().status,
        OccurrenceStatus::Cancelled
    );
}

fn dispatch_checks(root: &Path) {
    // Successful dispatch: exactly one storage commit and one occurrence link.
    let directory = root.join("dispatch-success");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    let (run_id, writes, queued, link) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.automation_fake_mode = false;
            a.qa_storage_writes.set(0);
            let prepared = match a.prepare_run_dispatch(run, vec![], cx) {
                Some(prepared) => prepared,
                None => panic!("dispatch preparation failed: {}", a.error),
            };
            let dispatch = a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).unwrap();
            (
                dispatch.run_id,
                a.qa_storage_writes.get(),
                a.queued_runs.iter().any(|(id, _)| *id == dispatch.run_id),
                a.store.occurrence(occ.id).unwrap().run_ids.clone(),
            )
        })
    });
    assert_eq!(writes, 1, "dispatch must use exactly one storage commit");
    assert_eq!(link, vec![run_id]);
    assert!(queued);
    assert_eq!(cx.update(|cx| app.read(cx).store.runs().len()), 1);
    // Do not let the pump start a real process in this QA fixture.
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Same-process durable queued run with a lost queue effect: requeue the
    // exact run id, not a second run.
    let (same_id, writes, queued) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.qa_storage_writes.set(0);
            a.enqueue_automation_occurrence(occ.id, cx);
            (
                a.store.occurrence(occ.id).unwrap().run_ids.clone(),
                a.qa_storage_writes.get(),
                a.queued_runs.iter().any(|(id, _)| *id == run_id),
            )
        })
    });
    assert_eq!(same_id, vec![run_id]);
    assert_eq!(writes, 1);
    assert!(queued);
    assert_eq!(cx.update(|cx| app.read(cx).store.runs().len()), 1);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Crash after commit, before queue effect: requeue the same run id.
    let (run_id_recovered, writes, queued) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            if let Some(mut run) = a.store.run_mut(run_id) {
                run.status = "Interrupted".into();
                run.started_at.clear();
            }
            a.save_current_store_durably(
                crate::live_save_app::LiveCompletion::None,
                cx,
            )
            .unwrap();
            a.queued_runs.clear();
            a.qa_storage_writes.set(0);
            a.enqueue_automation_occurrence(occ.id, cx);
            (
                a.store.occurrence(occ.id).unwrap().run_ids.clone(),
                a.qa_storage_writes.get(),
                a.queued_runs.iter().any(|(id, _)| *id == run_id),
            )
        })
    });
    assert_eq!(run_id_recovered, vec![run_id], "no duplicate run id");
    assert_eq!(writes, 1, "requeue is one commit");
    assert!(queued);
    assert_eq!(cx.update(|cx| app.read(cx).store.runs().len()), 1);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Failed commit retains dispatch intent; retry reprepares from current state.
    let directory = root.join("dispatch-failure");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_runs BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'qa dispatch failure'); END;",
    )
    .unwrap();
    drop(db);
    let (pending, runs) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.qa_storage_writes.set(0);
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).is_none());
            (a.pending_dispatch.is_some(), a.store.runs().len())
        })
    });
    assert!(pending);
    assert_eq!(runs, 0, "failed dispatch must not stage a run");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_runs;").unwrap();
    drop(db);
    let (writes, link, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.qa_storage_writes.set(0);
            a.retry_pending_saves(cx);
            (
                a.qa_storage_writes.get(),
                a.store.occurrence(occ.id).unwrap().run_ids.clone(),
                a.pending_dispatch.is_some(),
            )
        })
    });
    assert_eq!(writes, 1, "dispatch retry is one commit");
    assert_eq!(link.len(), 1);
    assert!(!pending);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Stale prepared run id: commit is rejected without a stage, then retry
    // allocates a fresh run id from the current store.
    let directory = root.join("dispatch-stale");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    let stale_id = run.id;
    let (pending, runs) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            let mut newer = prepared.run.clone();
            newer.automation_occurrence_id = None;
            newer.task_id = 1;
            newer.status = "Simulated".into();
            a.store.push_run(newer);
            assert!(a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).is_none());
            (a.pending_dispatch.is_some(), a.store.runs().len())
        })
    });
    assert!(pending);
    assert_eq!(runs, 1);
    let (link, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            (
                a.store.occurrence(occ.id).unwrap().run_ids.clone(),
                a.pending_dispatch.is_some(),
            )
        })
    });
    assert!(!pending);
    assert_eq!(link.len(), 1);
    assert_ne!(link[0], stale_id, "stale run id must not be replayed");
    assert!(link[0] > stale_id);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // A committed never-started run recovered after its allowed delay must not
    // be requeued; expiry wins over exact-intent recovery.
    let directory = root.join("dispatch-expired");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    cx.update(|cx| {
        app.update(cx, |a, cx| {
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).unwrap();
            a.queued_runs.clear();
            if let Some(mut value) = a.store.occurrence_mut(occ.id) {
                value.reference_time = now() - value.snapshot.policy.allowed_delay_seconds - 1;
            }
            a.enqueue_automation_occurrence(occ.id, cx);
        });
    });
    let (status, queued) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(occ.id).unwrap().status,
            a.queued_runs.len(),
        )
    });
    assert_eq!(status, OccurrenceStatus::Skipped);
    assert_eq!(queued, 0, "expired committed work must not dispatch");

    // Deadline wins over exact-intent recovery as well.
    let directory = root.join("dispatch-deadline");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    cx.update(|cx| {
        app.update(cx, |a, cx| {
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).unwrap();
            a.queued_runs.clear();
            if let Some(mut value) = a.store.occurrence_mut(occ.id) {
                value.deadline_at = Some(now() - 1);
            }
            a.enqueue_automation_occurrence(occ.id, cx);
        });
    });
    let (status, queued) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(occ.id).unwrap().status,
            a.queued_runs.len(),
        )
    });
    assert!(status.is_terminal(), "a past-deadline committed run must time out");
    assert_eq!(queued, 0, "past-deadline committed work must not dispatch");
}

fn pending_dispatch_preserves_prior_intent(root: &Path) {
    let directory = root.join("dispatch-prior-pending");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, occ, run) = dispatch_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.update(|cx| app.update(cx, |a, _| a.store.push_occurrence(occ.clone())));
    fail_automation_writes(&path);
    let pending_kind = cx.update(|cx| {
        app.update(cx, |a, cx| {
            assert!(
                a.execute_command(
                    crate::domain_command::Command::SetRead {
                        occurrence: occ.id,
                        unread: true,
                    },
                    cx,
                )
                .is_err()
            );
            a.pending_commit.as_ref().map(|value| value.id.kind)
        })
    });
    assert_eq!(pending_kind, Some("automation_read"));
    let (dispatch_pending, run_count, queued) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(a.commit_prepared_dispatch(prepared, crate::app::LaunchIntent::Occurrence(occ.id), cx).is_none());
            (
                a.pending_dispatch.is_some(),
                a.store.runs().len(),
                a.queued_runs.len(),
            )
        })
    });
    assert!(!dispatch_pending, "the earlier user intent must remain authoritative");
    assert_eq!(run_count, 0, "no dispatch run may be staged");
    assert_eq!(queued, 0, "no queue effect may run");
    let pending_kind = cx.update(|cx| app.read(cx).pending_commit.as_ref().map(|value| value.id.kind));
    assert_eq!(pending_kind, Some("automation_read"));
    allow_automation_writes(&path);
    let (pending, read) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            (
                a.pending_commit.is_some(),
                a.store.occurrence(occ.id).unwrap().unread,
            )
        })
    });
    assert!(!pending);
    assert!(read);
    assert_eq!(cx.update(|cx| app.read(cx).store.runs().len()), 0);
}

/// Typed dispatch intents must rebuild the authoritative request/source and
/// reject prepared-but-stale inputs at the command boundary.
fn install_material(store: &mut Store, directory: &Path, task_id: u64) -> std::path::PathBuf {
    // A real 1x1 PNG, not only a magic-byte prefix.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
        0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let attachment = crate::attachments::import_bytes(directory, "evidence.png", PNG).unwrap();
    let path = crate::attachments::resolve(directory, &attachment).unwrap();
    store
        .task_mut(task_id)
        .unwrap()
        .attachments
        .push(attachment);
    path
}

fn typed_intent_checks(root: &Path) {
    // Request-plan failure/retry preserves revision ownership and atomic mapping.
    let directory = root.join("request-plan");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, request) = request_plan_fixture(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_runs BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'qa request failure'); END;",
    )
    .unwrap();
    drop(db);
    let (pending, run_id) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = a.store.prepare_request_plan(request).unwrap();
            let run_id = run.id;
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(
                a.commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::RequestPlan { request_id: request },
                    cx,
                )
                .is_none()
            );
            (a.pending_dispatch.is_some(), run_id)
        })
    });
    assert!(pending);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_runs;").unwrap();
    drop(db);
    let (mapping, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            (
                a.store
                    .research_requests()
                    .iter()
                    .find(|value| value.id == request)
                    .and_then(|value| value.planning_run_id),
                a.pending_dispatch.is_some(),
            )
        })
    });
    assert_eq!(mapping, Some(run_id));
    assert!(!pending);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Stale request input after preparation is rejected before any run/mapping.
    let directory = root.join("request-stale");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, request) = request_plan_fixture(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (rejected, runs, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = a.store.prepare_request_plan(request).unwrap();
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            if let Some(value) = a
                .store.request_mut(request)
            {
                value.input = "A different request after preparation.".into();
            }
            let rejected = a
                .commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::RequestPlan { request_id: request },
                    cx,
                )
                .is_none();
            (rejected, a.store.runs().len(), a.pending_dispatch.is_some())
        })
    });
    assert!(rejected);
    assert_eq!(runs, 0);
    assert!(pending);
    let discarded = cx.update(|cx| app.update(cx, |a, cx| a.discard_pending_dispatch(cx)));
    assert!(discarded);
    assert!(!cx.update(|cx| app.read(cx).pending_dispatch.is_some()));

    // Ordinary report regeneration: failure/retry must stay research-owned.
    let directory = root.join("report-regeneration");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, source_id, _, _) = regeneration_fixture(&directory, false, false);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_runs BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'qa regeneration failure'); END;",
    )
    .unwrap();
    drop(db);
    let pending = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = crate::research::prepare_report_regeneration(&a.store, source_id, "shorter")
                .unwrap();
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(
                a.commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::ReportRegeneration {
                        source_id,
                        style: "shorter".into(),
                        occurrence: None,
                    },
                    cx,
                )
                .is_none()
            );
            a.pending_dispatch.is_some()
        })
    });
    assert!(pending);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_runs;").unwrap();
    drop(db);
    let (owned, toast, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            (
                a.store
                    .runs()
                    .iter()
                    .find(|run| run.research_kind == Some(ResearchKind::Advise) && run.id != source_id)
                    .and_then(|run| run.automation_occurrence_id)
                    .is_some(),
                a.toast.clone(),
                a.pending_dispatch.is_some(),
            )
        })
    });
    assert!(!owned);
    assert!(toast.contains("research execution slot"));
    assert!(!pending);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Retry re-runs the original material/browser preparation on the fresh run.
    let directory = root.join("regeneration-materials");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (mut store, source_id, _, _) =
        regeneration_fixture(&directory, false, true);
    store
        .run_mut(source_id)
        .unwrap()
        .logs
        .push(crate::research::BROWSER_HANDOFF_LOG.into());
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_runs BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'qa material retry'); END;",
    )
    .unwrap();
    drop(db);
    let pending = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = crate::research::prepare_report_regeneration(&a.store, source_id, "shorter")
                .unwrap();
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(
                a.commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::ReportRegeneration {
                        source_id,
                        style: "shorter".into(),
                        occurrence: None,
                    },
                    cx,
                )
                .is_none()
            );
            a.pending_dispatch.is_some()
        })
    });
    assert!(pending);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_runs;").unwrap();
    drop(db);
    let (attachments, prompt, queued_files, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            let run = a
                .store
                .runs()
                .iter()
                .find(|run| run.research_kind == Some(ResearchKind::Advise) && run.id != source_id)
                .unwrap();
            (
                run.attachments.len(),
                (
                    run.prompt.matches("Attached material").count(),
                    run.prompt.matches("BROWSER CONTROL").count(),
                ),
                a.queued_runs
                    .iter()
                    .find(|(id, _)| *id == run.id)
                    .map(|(_, files)| files.len()),
                a.pending_dispatch.is_some(),
            )
        })
    });
    assert_eq!(attachments, 1);
    assert_eq!(prompt, (1, 1), "material and browser context appended once");
    assert_eq!(queued_files, Some(1));
    assert!(!pending);
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Missing/changed material is rejected on retry, not silently omitted.
    let directory = root.join("regeneration-material-missing");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (mut store, source_id, _, material_path) =
        regeneration_fixture(&directory, false, true);
    let baseline_runs = store.runs().len();
    let attachment_path = material_path.expect("material fixture path");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_runs BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT, 'qa material missing'); END;",
    )
    .unwrap();
    drop(db);
    let pending = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = crate::research::prepare_report_regeneration(&a.store, source_id, "shorter")
                .unwrap();
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            assert!(
                a.commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::ReportRegeneration {
                        source_id,
                        style: "shorter".into(),
                        occurrence: None,
                    },
                    cx,
                )
                .is_none()
            );
            a.pending_dispatch.is_some()
        })
    });
    assert!(pending);
    std::fs::write(&attachment_path, b"changed material bytes").unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER reject_runs;").unwrap();
    drop(db);
    let (runs, pending, queued) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            a.retry_pending_saves(cx);
            (
                a.store.runs().len(),
                a.pending_dispatch.is_some(),
                a.queued_runs.len(),
            )
        })
    });
    assert_eq!(
        runs, baseline_runs,
        "changed material must not add a run or drop prerequisite history"
    );
    assert!(!pending, "stale material intent is explicitly resolved");
    assert_eq!(queued, 0);

    // Automation period regeneration keeps automation ownership and links the
    // regeneration run into the period history.
    let directory = root.join("period-regeneration");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, source_id, occurrence, _) = regeneration_fixture(&directory, true, false);
    let occurrence_id = occurrence.unwrap();
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (owned, toast, linked, run_id) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = a
                .prepare_regeneration_dispatch(source_id, "shorter", Some(occurrence_id))
                .unwrap();
            let likely_id = run.id;
            let prepared = match a.prepare_run_dispatch(run, vec![], cx) {
                Some(value) => value,
                None => panic!("period regeneration preparation failed: {}", a.error),
            };
            let dispatch = a
                .commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::ReportRegeneration {
                        source_id,
                        style: "shorter".into(),
                        occurrence: Some(occurrence_id),
                    },
                    cx,
                )
                .unwrap();
            (
                dispatch.automation_owned,
                a.toast.clone(),
                a.store
                    .occurrence(occurrence_id)
                    .is_some_and(|value| value.run_ids.contains(&likely_id)),
                likely_id,
            )
        })
    });
    assert!(owned, "period regeneration must remain automation-owned");
    assert!(toast.contains("automation slot"));
    assert!(linked);
    assert_eq!(run_id, cx.update(|cx| app.read(cx).store.runs().last().unwrap().id));
    cx.update(|cx| app.update(cx, |a, _| a.queued_runs.clear()));

    // Stale regeneration source is rejected, then the pending intent is discardable.
    let directory = root.join("regeneration-stale");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, source_id, _, _) = regeneration_fixture(&directory, false, false);
    let baseline_runs = store.runs().len();
    let source_task = store
        .runs()
        .iter()
        .find(|run| run.id == source_id)
        .unwrap()
        .task_id;
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (rejected, runs, pending) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            let run = crate::research::prepare_report_regeneration(&a.store, source_id, "shorter")
                .unwrap();
            let prepared = a.prepare_run_dispatch(run, vec![], cx).unwrap();
            a.store.task_mut(source_task).unwrap().description =
                "Changed after regeneration was prepared.".into();
            let rejected = a
                .commit_prepared_dispatch(
                    prepared,
                    crate::app::LaunchIntent::ReportRegeneration {
                        source_id,
                        style: "shorter".into(),
                        occurrence: None,
                    },
                    cx,
                )
                .is_none();
            (rejected, a.store.runs().len(), a.pending_dispatch.is_some())
        })
    });
    assert!(rejected);
    assert_eq!(
        runs, baseline_runs,
        "only the original prerequisite/source runs remain"
    );
    assert!(pending);
    assert!(cx.update(|cx| app.update(cx, |a, cx| a.discard_pending_dispatch(cx))));
}

fn automation_command_invalidates_undo_check(root: &Path) {
    let directory = root.join("command-undo-guard");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite3");
    let (store, _) = automation_store(&directory);
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = app_for(&path, &directory);
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let (undo_len, reason, status) = cx.update(|cx| {
        app.update(cx, |a, cx| {
            assert!(a
                .apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TopicCreate {
                        project: 1,
                        name: "Undo guard".into(),
                    },
                    "Topic created",
                    cx,
                )
                .is_some());
            assert_eq!(a.undo.len(), 1, "ordinary scoped edit is undoable");
            a.store.push_automation(crate::automation::AutomationDefinition {
                id: 99,
                version: 1,
                project_id: 1,
                shape: AutomationShape::Quick,
                name: "Undo guard".into(),
                objective: "Check a value.".into(),
                expected_result: "One answer.".into(),
                schedule: Schedule {
                    spec: ScheduleSpec::Interval { minutes: 10 },
                    timezone: "UTC".into(),
                },
                status: crate::automation::DefinitionStatus::Enabled,
                template: None,
                plan_binding: None,
                approval: None,
                policy: AutomationPolicy::default(),
                cursor: Some(now()),
                anchor: Some(now()),
                consecutive_failures: 0,
                attention_reason: None,
                created_at: now(),
                updated_at: now(),
            });
            let command = crate::domain_command::Command::AutomationPause {
                id: 99,
                now: now(),
            };
            a.execute_command(command, cx).unwrap();
            (
                a.undo.len(),
                a.undo_invalidation.clone(),
                a.store.automation(99).unwrap().status,
            )
        })
    });
    assert_eq!(
        undo_len, 1,
        "a durable automation command records no undo entry and keeps scoped edits"
    );
    assert!(reason.is_none());
    assert_eq!(status, crate::automation::DefinitionStatus::Paused);
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, directory: &Path, name: &str) {
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(40));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}
