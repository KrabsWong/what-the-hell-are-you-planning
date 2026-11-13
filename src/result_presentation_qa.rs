//! Focused offline acceptance for typed result presentation. No model or browser runs.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    automation::{
        AutomationApproval, AutomationDefinition, AutomationOccurrence,
        AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape, AutomationTemplate,
        DefinitionStatus, OccurrenceStatus, TimeWindow, TriggerSource,
    },
    automation_schedule::{Schedule, ScheduleSpec},
    domain::{Stage, Store},
    persistence::Storage,
    research::{self, ResearchKind, ResearchTask, Source},
    result_presentation::{present_occurrence, present_run, EvidenceQuality, ResultKind},
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze findings","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

#[derive(Clone, Copy, Debug)]
enum AutomationState {
    QuickComplete,
    QuickPartial,
    QuickRunning,
    QuickFailed,
    QuickCancelled,
    QuickStale,
    QuickRegenerating,
    ResearchComplete,
    ResearchPartial,
    ResearchRunning,
    ResearchFailed,
    ResearchCancelled,
    ResearchRegenerating,
    ResearchStale,
    ResearchMultiGoalStale,
}

#[derive(Clone, Copy, Debug)]
enum OrdinaryState {
    Complete,
    Partial,
    Running,
    Stale,
    Failed,
    Cancelled,
    Regenerating,
}

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/result-presentation-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if std::env::var("PLANNING_REFRESH_PREVIEW").as_deref() == Ok("1") {
        refresh_preview(&root);
        return;
    }
    if std::env::var("PLANNING_TECHNICAL_QA").as_deref() == Ok("1") {
        atomic_period_navigation_check(&root);
        technical_scroll_check(&root);
        return;
    }
    pure_checks();
    lazy_reader_check(&root);
    atomic_reader_navigation_check(&root);
    atomic_period_navigation_check(&root);
    render_matrix(&root);
    copy_result_check(&root);
    export_parity_check(&root).unwrap();
    println!("Result presentation QA passed · {}", root.display());
}

#[derive(Clone, Debug)]
struct ExpectedRender {
    state: String,
    report_runs: Vec<u64>,
    no_final: bool,
    progress: bool,
}

fn expected_render(store: &Store, occurrence: &AutomationOccurrence) -> ExpectedRender {
    let presentation = present_occurrence(store, occurrence);
    let active = presentation.attempt.is_active();
    ExpectedRender {
        state: if active {
            String::new()
        } else {
            presentation.state_label()
        },
        report_runs: presentation
            .reports
            .iter()
            .filter(|report| report.presentation.has_reader_body())
            .filter_map(|report| report.presentation.run_id)
            .collect(),
        no_final: !presentation.has_final_report && !active,
        progress: !presentation.progress.is_empty(),
    }
}

fn workspace(directory: &Path) -> Store {
    let mut store = Store::mock();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/result-presentation".into());
    store.research_config_mut().models =
        ["qa/plan", "qa/collect", "qa/analyze", "qa/advise"].map(String::from);
    store
}

fn add_run(
    store: &mut Store,
    task: u64,
    occurrence: Option<u64>,
    status: &str,
    output: &str,
) -> u64 {
    let mut run = store.prepare_run(task).unwrap();
    run.automation_occurrence_id = occurrence;
    run.status = status.into();
    run.output = output.into();
    run.sources = vec![Source {
        title: "Isolated QA source".into(),
        url: "https://example.com/result-presentation".into(),
        published_at: "2026-11-08 QA".into(),
    }];
    run.ended_at = Some(crate::opencode::timestamp());
    run.exit_code = Some(0);
    let id = run.id;
    store.push_run(run);
    id
}

fn add_definition(store: &mut Store, id: u64, shape: AutomationShape, version: u32) {
    let schedule = Schedule {
        spec: ScheduleSpec::Interval { minutes: 10 },
        timezone: "UTC".into(),
    };
    let template = AutomationTemplate {
        goals: vec![],
        window_rule: Default::default(),
    };
    let approval = AutomationApproval {
        version,
        approved_at: 0,
        template_signature: template.signature(),
        schedule_signature: schedule.signature(),
        plan_request_id: None,
        plan_revision: None,
        models: Default::default(),
        agents: [None, None, None, None],
        materials: vec![],
    };
    store.push_automation(AutomationDefinition {
        id,
        version,
        project_id: 1,
        shape,
        name: "Result presentation QA".into(),
        objective: "Exercise result quality states.".into(),
        expected_result: "A readable report or an explicit no-report state.".into(),
        schedule,
        status: DefinitionStatus::Enabled,
        template: Some(template),
        plan_binding: None,
        approval: Some(approval),
        policy: AutomationPolicy::default(),
        cursor: None,
        anchor: None,
        consecutive_failures: 0,
        attention_reason: None,
        created_at: 0,
        updated_at: 0,
    });
}

fn occurrence(
    id: u64,
    goals: Vec<u64>,
    shape: AutomationShape,
    status: OccurrenceStatus,
    final_run_id: Option<u64>,
    definition_version: u32,
) -> AutomationOccurrence {
    AutomationOccurrence {
        id,
        automation_id: 99,
        definition_version,
        source: TriggerSource::Manual,
        trigger_key: format!("manual:{id}"),
        scheduled_for: None,
        requested_at: Some(100),
        actual_started_at: Some(101),
        timezone: "UTC".into(),
        reference_time: 100,
        data_window: TimeWindow::unresolved("QA fixture"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: "Exercise result presentation.".into(),
            expected_result: "A readable report or an explicit no-report state.".into(),
            shape: Some(shape),
            template: AutomationTemplate {
                goals: vec![],
                window_rule: Default::default(),
            },
            models: Default::default(),
            agents: Default::default(),
            materials: vec![],
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "qa".into(),
        },
        status,
        reason: None,
        research_goal_id: goals.first().copied(),
        research_goal_ids: goals,
        research_topic_id: None,
        run_ids: vec![],
        final_run_id,
        unread: true,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: 99,
        deadline_at: None,
        retry_requested_at: None,
    }
}

fn quick_goal(store: &mut Store, occurrence_id: u64, status: &str, output: &str) -> (u64, u64, u64) {
    let goal = store
        .create_research_goal(
            "Quick result QA".into(),
            "A single direct answer step.".into(),
            1,
        )
        .unwrap();
    let task = store
        .create_task("Answer the recurring question", 1, Stage::Planned, Some(goal))
        .unwrap();
    let task_ref = store.task_mut(task).unwrap();
    task_ref.description = "Give the smallest complete answer.".into();
    task_ref.research = Some(ResearchTask {
        requires_approval: false,
        kind: ResearchKind::Advise,
        goal_id: goal,
        depends_on: vec![],
        space_name: "qa".into(),
    });
    let run = add_run(store, task, Some(occurrence_id), status, output);
    if status == "Succeeded" {
        store.set_stage(task, Stage::Done).unwrap();
    }
    (goal, task, run)
}

fn research_goal(store: &mut Store, _occurrence_id: u64) -> (u64, Vec<u64>) {
    let goal = store
        .create_research_goal(
            "Research result QA".into(),
            "Three ordered steps used to exercise result states.".into(),
            1,
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    (goal, steps)
}

fn automation_fixture(directory: &Path, state: AutomationState) -> (Store, u64) {
    let mut store = workspace(directory);
    let shape = match state {
        AutomationState::QuickComplete
        | AutomationState::QuickPartial
        | AutomationState::QuickRunning
        | AutomationState::QuickFailed
        | AutomationState::QuickCancelled
        | AutomationState::QuickStale
        | AutomationState::QuickRegenerating => AutomationShape::Quick,
        _ => AutomationShape::Research,
    };
    let version = if matches!(
        state,
        AutomationState::ResearchRegenerating | AutomationState::QuickRegenerating
    ) {
        2
    } else {
        1
    };
    add_definition(&mut store, 99, shape, version);

    match state {
        AutomationState::QuickComplete => {
            let (goal, _task, run) = quick_goal(&mut store, 31, "Succeeded", "Quick complete answer");
            store.push_occurrence(occurrence(
                31,
                vec![goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(run),
                1,
            ));
            (store, 31)
        }
        AutomationState::QuickPartial => {
            let (goal, _task, run) = quick_goal(
                &mut store,
                32,
                "Succeeded",
                &format!("{}\n\nQuick partial answer", research::EVIDENCE_GAP_NOTE),
            );
            store.push_occurrence(occurrence(
                32,
                vec![goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(run),
                1,
            ));
            (store, 32)
        }
        AutomationState::QuickRunning => {
            let (goal, _task, run) = quick_goal(
                &mut store,
                33,
                "Running",
                "Checking the current value before answering",
            );
            store.push_occurrence(occurrence(
                33,
                vec![goal],
                shape,
                OccurrenceStatus::Running,
                None,
                1,
            ));
            let _ = run;
            (store, 33)
        }
        AutomationState::QuickFailed => {
            let (goal, _task, run) = quick_goal(
                &mut store,
                40,
                "Failed",
                r#"{"status":"complete","report":"unvalidated quick"#,
            );
            store.push_occurrence(occurrence(
                40,
                vec![goal],
                shape,
                OccurrenceStatus::Failed,
                None,
                1,
            ));
            let _ = run;
            (store, 40)
        }
        AutomationState::QuickCancelled => {
            let (goal, _task, run) = quick_goal(
                &mut store,
                41,
                "Cancelled",
                &format!("{}\n\nCancelled quick answer", research::EVIDENCE_GAP_NOTE),
            );
            store.push_occurrence(occurrence(
                41,
                vec![goal],
                shape,
                OccurrenceStatus::Cancelled,
                None,
                1,
            ));
            let _ = run;
            (store, 41)
        }
        AutomationState::QuickStale => {
            let (goal, task, run) = quick_goal(&mut store, 42, "Succeeded", "Quick old answer");
            store
                .task_mut(task)
                .unwrap()
                .description
                .push_str(" changed after completion");
            store.push_occurrence(occurrence(
                42,
                vec![goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(run),
                1,
            ));
            (store, 42)
        }
        AutomationState::QuickRegenerating => {
            let (goal, task, run) = quick_goal(
                &mut store,
                43,
                "Succeeded",
                &format!("{}\n\nQuick previous answer", research::EVIDENCE_GAP_NOTE),
            );
            let mut regenerating = store
                .runs()
                .iter()
                .find(|value| value.id == run)
                .unwrap()
                .clone();
            regenerating.id = store.runs().iter().map(|value| value.id).max().unwrap_or(0) + 1;
            regenerating.status = "Running".into();
            regenerating.output = "Regenerating the quick answer".into();
            regenerating.session_id = Some("ses_qa_quick_regeneration".into());
            store.push_run(regenerating);
            let _ = task;
            store.push_occurrence(occurrence(
                43,
                vec![goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(run),
                1,
            ));
            (store, 43)
        }
        AutomationState::ResearchComplete => {
            let (goal, steps) = research_goal(&mut store, 34);
            add_run(&mut store, steps[0], Some(34), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(34), "Succeeded", "First goal analysis");
            let report = add_run(&mut store, steps[2], Some(34), "Succeeded", "First goal final report");
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let second_goal = store
                .create_research_goal(
                    "Second complete goal".into(),
                    "A second required final report.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(34), "Succeeded", "Second goal evidence");
            add_run(&mut store, second_steps[1], Some(34), "Succeeded", "Second goal analysis");
            add_run(&mut store, second_steps[2], Some(34), "Succeeded", "Second goal final report");
            for task in &second_steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            store.push_occurrence(occurrence(
                34,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(report),
                1,
            ));
            (store, 34)
        }
        AutomationState::ResearchPartial => {
            let (goal, steps) = research_goal(&mut store, 35);
            add_run(&mut store, steps[0], Some(35), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(35), "Succeeded", "First goal analysis");
            let report = add_run(
                &mut store,
                steps[2],
                Some(35),
                "Succeeded",
                &format!("{}\n\nFirst goal partial final report", research::EVIDENCE_GAP_NOTE),
            );
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let second_goal = store
                .create_research_goal(
                    "Second goal without final report".into(),
                    "One completed evidence step but no final report.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(
                &mut store,
                second_steps[0],
                Some(35),
                "Succeeded",
                "Second goal completed evidence",
            );
            store.set_stage(second_steps[0], Stage::Done).unwrap();
            store.push_occurrence(occurrence(
                35,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(report),
                1,
            ));
            (store, 35)
        }
        AutomationState::ResearchRunning => {
            let (goal, steps) = research_goal(&mut store, 36);
            add_run(&mut store, steps[0], Some(36), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(36), "Succeeded", "First goal analysis");
            add_run(&mut store, steps[2], Some(36), "Succeeded", "First goal final report");
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let second_goal = store
                .create_research_goal(
                    "Second running goal".into(),
                    "A second goal is still analyzing.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(36), "Succeeded", "Second goal evidence");
            add_run(
                &mut store,
                second_steps[1],
                Some(36),
                "Running",
                "Second goal analysis is running",
            );
            store.push_occurrence(occurrence(
                36,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Running,
                None,
                1,
            ));
            (store, 36)
        }
        AutomationState::ResearchFailed => {
            let (goal, steps) = research_goal(&mut store, 37);
            add_run(&mut store, steps[0], Some(37), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(37), "Succeeded", "First goal analysis");
            add_run(&mut store, steps[2], Some(37), "Succeeded", "First goal retained final report");
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let second_goal = store
                .create_research_goal(
                    "Second failed goal".into(),
                    "A second goal failed during analysis.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(37), "Succeeded", "Second goal evidence remains");
            add_run(&mut store, second_steps[1], Some(37), "Failed", "Second goal analysis failed");
            store.push_occurrence(occurrence(
                37,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Failed,
                None,
                1,
            ));
            (store, 37)
        }
        AutomationState::ResearchCancelled => {
            let (goal, steps) = research_goal(&mut store, 38);
            add_run(&mut store, steps[0], Some(38), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(38), "Succeeded", "First goal analysis");
            add_run(&mut store, steps[2], Some(38), "Succeeded", "First goal retained final report");
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let second_goal = store
                .create_research_goal(
                    "Second cancelled goal".into(),
                    "A second goal was stopped during analysis.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(38), "Succeeded", "Second goal evidence remains");
            add_run(&mut store, second_steps[1], Some(38), "Cancelled", "Stopped during second goal analysis");
            store.push_occurrence(occurrence(
                38,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Cancelled,
                None,
                1,
            ));
            (store, 38)
        }
        AutomationState::ResearchStale => {
            let (goal, steps) = research_goal(&mut store, 45);
            add_run(&mut store, steps[0], Some(45), "Succeeded", "Stale goal evidence");
            add_run(&mut store, steps[1], Some(45), "Succeeded", "Stale goal analysis");
            let report = add_run(
                &mut store,
                steps[2],
                Some(45),
                "Succeeded",
                "Stale goal previous report",
            );
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            store
                .task_mut(steps[2])
                .unwrap()
                .description
                .push_str(" changed after completion");
            store.push_occurrence(occurrence(
                45,
                vec![goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(report),
                1,
            ));
            (store, 45)
        }
        AutomationState::ResearchMultiGoalStale => {
            let (first_goal, first_steps) = research_goal(&mut store, 46);
            add_run(&mut store, first_steps[0], Some(46), "Succeeded", "Stale first evidence");
            add_run(&mut store, first_steps[1], Some(46), "Succeeded", "Stale first analysis");
            let stale_report = add_run(
                &mut store,
                first_steps[2],
                Some(46),
                "Succeeded",
                "Stale first goal report",
            );
            for task in &first_steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            store
                .task_mut(first_steps[2])
                .unwrap()
                .description
                .push_str(" changed after completion");

            let second_goal = store
                .create_research_goal(
                    "Current second goal".into(),
                    "A current final report remains inspectable beside stale history.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(46), "Succeeded", "Second goal evidence");
            add_run(&mut store, second_steps[1], Some(46), "Succeeded", "Second goal analysis");
            add_run(
                &mut store,
                second_steps[2],
                Some(46),
                "Succeeded",
                "Current second goal report",
            );

            store.push_occurrence(occurrence(
                46,
                vec![first_goal, second_goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(stale_report),
                1,
            ));
            (store, 46)
        }
        AutomationState::ResearchRegenerating => {
            let (goal, steps) = research_goal(&mut store, 39);
            add_run(&mut store, steps[0], Some(39), "Succeeded", "First goal evidence");
            add_run(&mut store, steps[1], Some(39), "Succeeded", "First goal analysis");
            let report = add_run(
                &mut store,
                steps[2],
                Some(39),
                "Succeeded",
                &format!("{}\n\nPrevious partial first goal report", research::EVIDENCE_GAP_NOTE),
            );
            for task in &steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let mut regenerating = store
                .runs()
                .iter()
                .find(|run| run.id == report)
                .unwrap()
                .clone();
            regenerating.id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
            regenerating.status = "Running".into();
            regenerating.output = "Regenerating the first goal report from existing evidence".into();
            regenerating.session_id = Some("ses_qa_regeneration".into());
            store.push_run(regenerating);

            let second_goal = store
                .create_research_goal(
                    "Second complete goal during regeneration".into(),
                    "A second goal remains complete while the first is regenerated.".into(),
                    1,
                )
                .unwrap();
            let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
            add_run(&mut store, second_steps[0], Some(39), "Succeeded", "Second goal evidence");
            add_run(&mut store, second_steps[1], Some(39), "Succeeded", "Second goal analysis");
            add_run(&mut store, second_steps[2], Some(39), "Succeeded", "Second goal complete report");
            for task in &second_steps {
                store.set_stage(*task, Stage::Done).unwrap();
            }
            // Definition is v2 while the occurrence keeps its frozen v1 snapshot.
            store.push_occurrence(occurrence(
                39,
                vec![goal, second_goal],
                shape,
                OccurrenceStatus::Succeeded,
                Some(report),
                1,
            ));
            (store, 39)
        }
    }
}

fn ordinary_fixture(directory: &Path, state: OrdinaryState) -> (Store, u64, u64) {
    let mut store = workspace(directory);
    let goal = store
        .create_research_goal(
            "Ordinary research result QA".into(),
            "Three steps used to exercise ordinary result presentation.".into(),
            1,
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence body");
    add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings body");
    let report_status = match state {
        OrdinaryState::Running => "Running",
        OrdinaryState::Failed => "Failed",
        OrdinaryState::Cancelled => "Cancelled",
        _ => "Succeeded",
    };
    let output = match state {
        OrdinaryState::Partial => {
            format!("{}\n\nOrdinary partial report body", research::EVIDENCE_GAP_NOTE)
        }
        OrdinaryState::Running => "Writing the report from collected evidence".into(),
        OrdinaryState::Failed => {
            r#"{"status":"complete","report":"unvalidated raw envelope"#.into()
        }
        OrdinaryState::Cancelled => {
            r#"{"status":"partial","report":"cancelled raw envelope"#.into()
        }
        _ => "Ordinary complete report body".into(),
    };
    let report = add_run(&mut store, steps[2], None, report_status, &output);
    if matches!(
        state,
        OrdinaryState::Complete
            | OrdinaryState::Partial
            | OrdinaryState::Stale
            | OrdinaryState::Regenerating
    ) {
        for task in &steps {
            store.set_stage(*task, Stage::Done).unwrap();
        }
    }
    if matches!(state, OrdinaryState::Regenerating) {
        let mut active = store
            .runs()
            .iter()
            .find(|run| run.id == report)
            .unwrap()
            .clone();
        active.id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
        active.status = "Running".into();
        active.output = "Regenerating the ordinary report".into();
        active.session_id = Some("ses_qa_ordinary_regeneration".into());
        store.push_run(active);
    }
    if matches!(state, OrdinaryState::Stale) {
        store
            .task_mut(steps[2])
            .unwrap()
            .description
            .push_str(" changed after completion");
    }
    (store, steps[2], report)
}

fn pure_checks() {
    let directory = std::env::temp_dir().join(format!(
        "result-presentation-pure-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();

    for state in [
        AutomationState::QuickComplete,
        AutomationState::QuickPartial,
        AutomationState::QuickRunning,
        AutomationState::QuickFailed,
        AutomationState::QuickCancelled,
        AutomationState::QuickStale,
        AutomationState::QuickRegenerating,
        AutomationState::ResearchComplete,
        AutomationState::ResearchPartial,
        AutomationState::ResearchRunning,
        AutomationState::ResearchFailed,
        AutomationState::ResearchCancelled,
        AutomationState::ResearchRegenerating,
        AutomationState::ResearchStale,
        AutomationState::ResearchMultiGoalStale,
    ] {
        let (store, occurrence_id) = automation_fixture(&directory, state);
        let occurrence = store.occurrence(occurrence_id).unwrap();
        let presentation = present_occurrence(&store, occurrence);
        match state {
            AutomationState::QuickComplete | AutomationState::ResearchComplete => {
                assert_eq!(presentation.kind, ResultKind::FinalReport);
                assert_eq!(presentation.quality, EvidenceQuality::Complete);
                assert!(presentation.has_final_report);
                assert!(presentation.notice.is_none());
            }
            AutomationState::QuickPartial => {
                assert_eq!(presentation.kind, ResultKind::FinalReport);
                assert_eq!(presentation.quality, EvidenceQuality::Partial);
                assert!(presentation.has_final_report);
                assert!(presentation.notice.is_some());
                assert!(!presentation.reader_markdown.contains(research::EVIDENCE_GAP_NOTE));
            }
            AutomationState::ResearchPartial => {
                assert_eq!(presentation.kind, ResultKind::PartialPeriod);
                assert!(!presentation.has_final_report);
                assert_eq!(presentation.quality, EvidenceQuality::Incomplete);
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.quality == EvidenceQuality::Partial
                        && report.presentation.notice.is_some()
                }));
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.reader_markdown == "Second goal completed evidence"
                }));
            }
            AutomationState::QuickRunning => {
                assert_eq!(presentation.kind, ResultKind::LiveProgress);
                assert!(!presentation.has_final_report);
                assert!(presentation.reader_markdown.is_empty());
            }
            AutomationState::ResearchRunning => {
                assert_eq!(presentation.kind, ResultKind::PartialPeriod);
                assert!(!presentation.has_final_report);
                assert!(!presentation.progress.is_empty());
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.reader_markdown == "First goal final report"
                }));
            }
            AutomationState::QuickFailed | AutomationState::QuickCancelled => {
                assert!(!presentation.has_final_report);
                assert_ne!(presentation.kind, ResultKind::FinalReport);
                assert!(presentation.reader_text().is_empty());
            }
            AutomationState::QuickStale => {
                assert!(!presentation.has_final_report);
                assert_eq!(presentation.quality, EvidenceQuality::UnverifiedLegacy);
                assert!(presentation.freshness.stale_input);
            }
            AutomationState::ResearchStale => {
                assert!(!presentation.has_final_report);
                assert_eq!(presentation.quality, EvidenceQuality::UnverifiedLegacy);
                assert!(presentation.freshness.stale_input);
                assert!(!presentation.freshness.previous_version);
                assert!(presentation
                    .reports
                    .iter()
                    .any(|report| report.presentation.reader_markdown == "Stale goal previous report"));
                assert!(!presentation.reader_text().contains("{\"status\""));
            }
            AutomationState::ResearchMultiGoalStale => {
                assert_eq!(presentation.kind, ResultKind::PartialPeriod);
                assert!(!presentation.has_final_report);
                assert!(presentation.freshness.stale_input);
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.reader_markdown == "Stale first goal report"
                }));
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.has_final_report
                        && report.presentation.reader_markdown == "Current second goal report"
                }));
            }
            AutomationState::QuickRegenerating | AutomationState::ResearchRegenerating => {
                assert_eq!(presentation.quality, EvidenceQuality::Partial);
                assert!(presentation.has_final_report);
                assert!(presentation.freshness.previous_version);
                assert!(presentation.freshness.regenerating);
                assert!(presentation.notice.is_some());
            }
            AutomationState::ResearchFailed | AutomationState::ResearchCancelled => {
                assert!(!presentation.has_final_report);
                assert_ne!(presentation.kind, ResultKind::FinalReport);
                assert!(presentation.reports.iter().any(|report| {
                    report.presentation.reader_markdown.contains("evidence")
                        || report.presentation.reader_markdown.contains("Evidence")
                }));
            }
        }
    }

    for state in [
        OrdinaryState::Complete,
        OrdinaryState::Partial,
        OrdinaryState::Running,
        OrdinaryState::Stale,
        OrdinaryState::Failed,
        OrdinaryState::Cancelled,
        OrdinaryState::Regenerating,
    ] {
        let (store, _task, run_id) = ordinary_fixture(&directory, state);
        let run = store.runs().iter().find(|run| run.id == run_id).unwrap();
        let presentation = present_run(&store, run);
        match state {
            OrdinaryState::Complete => {
                assert_eq!(presentation.kind, ResultKind::FinalReport);
                assert_eq!(presentation.quality, EvidenceQuality::Complete);
                assert!(presentation.has_final_report);
            }
            OrdinaryState::Partial => {
                assert_eq!(presentation.quality, EvidenceQuality::Partial);
                assert!(presentation.notice.is_some());
                assert!(presentation.reader_text().contains("Evidence gaps:"));
            }
            OrdinaryState::Running => {
                assert_eq!(presentation.kind, ResultKind::LiveProgress);
                assert!(presentation.reader_markdown.is_empty());
            }
            OrdinaryState::Stale => {
                assert!(presentation.freshness.stale_input);
                assert!(!presentation.has_final_report);
                assert!(!presentation.reader_markdown.is_empty());
            }
            OrdinaryState::Failed | OrdinaryState::Cancelled => {
                assert_eq!(presentation.quality, EvidenceQuality::NotValidated);
                assert!(presentation.reader_markdown.is_empty());
                assert!(!presentation.reader_text().contains("raw envelope"));
            }
            OrdinaryState::Regenerating => {
                assert!(presentation.freshness.regenerating);
                assert!(presentation.has_final_report);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
}

fn render_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            for (state, name) in [
                (AutomationState::QuickComplete, "quick-complete"),
                (AutomationState::QuickPartial, "quick-partial"),
                (AutomationState::QuickRunning, "quick-running"),
                (AutomationState::QuickFailed, "quick-failed"),
                (AutomationState::QuickCancelled, "quick-cancelled"),
                (AutomationState::QuickStale, "quick-stale"),
                (AutomationState::QuickRegenerating, "quick-regenerating"),
                (AutomationState::ResearchComplete, "research-complete"),
                (AutomationState::ResearchPartial, "research-partial"),
                (AutomationState::ResearchRunning, "research-running"),
                (AutomationState::ResearchFailed, "research-failed"),
                (AutomationState::ResearchCancelled, "research-cancelled"),
                (AutomationState::ResearchRegenerating, "research-regenerating"),
                (AutomationState::ResearchStale, "research-stale"),
                (AutomationState::ResearchMultiGoalStale, "research-multigoal-stale"),
            ] {
                let (mut store, occurrence_id) = automation_fixture(&directory, state);
                store.set_appearance(mode);
                let occurrence = store.occurrence(occurrence_id).unwrap();
                let expected = expected_render(&store, occurrence);
                let path = directory.join(format!("{name}.sqlite3"));
                Storage::new(path.clone()).save(&store).unwrap();
                screenshot_automation(
                    store,
                    &path,
                    occurrence_id,
                    &directory,
                    &format!("{name}.png"),
                    width,
                    height,
                    match state {
                        AutomationState::QuickRegenerating | AutomationState::ResearchRegenerating => {
                            &[(-420., "body")][..]
                        }
                        AutomationState::ResearchComplete
                        | AutomationState::ResearchRunning
                        | AutomationState::ResearchFailed
                        | AutomationState::ResearchCancelled
                        | AutomationState::ResearchPartial => &[(-350., "second-goal")][..],
                        AutomationState::ResearchMultiGoalStale => {
                            &[(-420., "reports"), (-1200., "current")][..]
                        }
                        _ => &[][..],
                    },
                    expected,
                );
            }
            for (state, name) in [
                (OrdinaryState::Complete, "ordinary-complete"),
                (OrdinaryState::Partial, "ordinary-partial"),
                (OrdinaryState::Running, "ordinary-running"),
                (OrdinaryState::Stale, "ordinary-stale"),
                (OrdinaryState::Failed, "ordinary-failed"),
                (OrdinaryState::Cancelled, "ordinary-cancelled"),
                (OrdinaryState::Regenerating, "ordinary-regenerating"),
            ] {
                let (mut store, task, run) = ordinary_fixture(&directory, state);
                store.set_appearance(mode);
                let path = directory.join(format!("{name}.sqlite3"));
                Storage::new(path.clone()).save(&store).unwrap();
                screenshot_ordinary(
                    store,
                    &path,
                    task,
                    run,
                    &directory,
                    &format!("{name}.png"),
                    width,
                    height,
                );
            }
        }
    }
}

fn with_app<F>(
    store: Store,
    path: &Path,
    width: f32,
    height: f32,
    configure: F,
) -> (HeadlessAppContext, AnyWindowHandle)
where
    F: FnOnce(&mut TaskApp, &mut Window, &mut Context<TaskApp>),
{
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
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
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap()
        .into();
    (cx, handle)
}

fn screenshot_automation(
    store: Store,
    path: &Path,
    occurrence: u64,
    directory: &Path,
    name: &str,
    width: f32,
    height: f32,
    scrolls: &[(f32, &str)],
    expected: ExpectedRender,
) {
    let (mut cx, handle) = with_app(store, path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::AutomationRuns);
        a.automation_ui.selected = Some(99);
        a.push_period_overlay(occurrence);
    });
    screenshot(&mut cx, handle, directory, name);
    cx.update_window(handle, |_, w, _| {
        let state_element = w.try_find("automation-result-state");
        let state = state_element.as_ref().and_then(|element| element.label())
            .map(str::to_string).unwrap_or_default();
        if expected.state.is_empty() {
            assert!(
                state_element.is_none(),
                "Active results keep state in progress rows and notices instead of repeating it"
            );
        } else if expected.report_runs.len() == 1 && !expected.no_final
            && matches!(name, "quick-complete.png" | "quick-partial.png" | "research-complete.png" | "research-partial.png")
        {
            assert!(state_element.is_none(), "Single final reports must not repeat quality/type metadata above the reader");
        } else {
            assert_eq!(state, expected.state);
        }
        for run in &expected.report_runs {
            w.find(format!("automation-report-body-{occurrence}-{run}"));
        }
        if expected.no_final {
            w.find("automation-no-final-notice");
            assert!(!state.starts_with("Final report"), "{state}");
        }
        if expected.progress {
            w.find("automation-live-progress");
        }
        if name.contains("stale") {
            assert!(state.contains("Stale input"), "{state}");
            assert!(!state.contains("Original approved version"), "{state}");
        }
    })
    .unwrap();
    for (delta, suffix) in scrolls {
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.scroll(
                "automation-result-body",
                ScrollDelta::Pixels(point(px(0.), px(*delta))),
                cx,
            );
        })
        .unwrap();
        let body_name = name.replace(".png", &format!("-{suffix}.png"));
        screenshot(&mut cx, handle, directory, &body_name);
    }
}

fn technical_scroll_check(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let (mut store, occurrence) = automation_fixture(root, AutomationState::ResearchComplete);
            store.set_appearance(mode);
            let ids: Vec<_> = store.runs().iter()
                .filter(|run| run.automation_occurrence_id == Some(occurrence))
                .map(|run| run.id).collect();
            assert!(ids.len() > 1);
            for index in 0..store.runs().len() {
                let run = store.run_at_mut(index).unwrap();
                if ids.contains(&run.id) {
                    run.logs = (0..40).map(|line| format!("Run #{} diagnostic line {line}: evidence collection and report processing.", run.id)).collect();
                }
            }
            let first_step = store.runs().iter().find(|run| run.id == ids[0]).unwrap().task_id;
            let latest = *ids.last().unwrap();
            let first = ids[0];
            let presentation = present_occurrence(&store, store.occurrence(occurrence).unwrap());
            let source_run = presentation.reports.iter()
                .find(|report| !report.presentation.sources.is_empty()).unwrap()
                .presentation.run_id.unwrap();
            let sources_toggle = format!("automation-sources-{occurrence}-report-{source_run}");
            let source_link = format!("automation-source-{occurrence}-report-{source_run}-0");
            let path = root.join(format!("technical-{mode:?}-{width}.sqlite3"));
            Storage::new(path.clone()).save(&store).unwrap();
            let (mut cx, handle) = with_app(store, &path, width, height, |a, _, _| {
                a.set_scope_identity(Scope::AutomationRuns);
                a.push_period_overlay(occurrence);
                a.automation_ui.show_technical = false;
            });
            screenshot(&mut cx, handle, root, &format!("result-default-{mode:?}-{width}.png"));
            cx.update_window(handle, |root, w, cx| {
                w.render_frame(cx);
                let app = root.downcast::<base::Root>().unwrap().read(cx).view().clone().downcast::<TaskApp>().unwrap();
                let step_row = format!("automation-step-{first_step}");
                assert!(w.try_find(step_row.clone()).is_none());
                w.click("automation-completed-steps", cx);
                w.render_frame(cx);
                w.find(step_row.clone());
                w.click("automation-completed-steps", cx);
                w.render_frame(cx);
                assert!(w.try_find(step_row).is_none());
                assert!(w.try_find(source_link.clone()).is_none());
                w.click(sources_toggle.clone(), cx);
                w.render_frame(cx);
                w.find(source_link.clone());
                w.click(sources_toggle.clone(), cx);
                w.render_frame(cx);
                assert!(w.try_find(source_link.clone()).is_none());
                assert!(w.try_find("automation-period-diagnostics").is_none());
                app.update(cx, |a, cx| {
                    a.automation_ui.show_technical = true;
                    cx.notify();
                });
                w.render_frame(cx);
                w.find("automation-period-diagnostics");
                app.update(cx, |a, _| {
                    a.automation_ui.dialog_scroll.set_offset(point(px(0.), -a.automation_ui.dialog_scroll.max_offset().y));
                });
                w.render_frame(cx);
                let log_id = format!("automation-run-logs-{latest}");
                assert!(w.find(log_id.clone()).visible());
                assert!(w.try_find(format!("automation-run-logs-{first}")).is_none());
                let before = app.read(cx).automation_ui.dialog_scroll.offset();
                w.scroll(log_id.clone(), ScrollDelta::Pixels(point(px(0.), px(-90.))), cx);
                w.render_frame(cx);
                assert!(app.read(cx).automation_ui.technical_scroll.offset().y < px(0.));
                assert_eq!(app.read(cx).automation_ui.dialog_scroll.offset(), before);
                w.scroll(log_id.clone(), ScrollDelta::Pixels(point(px(0.), px(40.))), cx);
                w.render_frame(cx);
                assert_eq!(app.read(cx).automation_ui.dialog_scroll.offset(), before);
                w.scroll(log_id.clone(), ScrollDelta::Pixels(point(px(0.), px(1000.))), cx);
                w.render_frame(cx);
                assert_eq!(app.read(cx).automation_ui.dialog_scroll.offset(), before);
                w.scroll(log_id.clone(), ScrollDelta::Pixels(point(px(0.), px(100.))), cx);
                w.render_frame(cx);
                assert!(app.read(cx).automation_ui.dialog_scroll.offset().y > before.y);
                w.click(format!("automation-run-tab-{first}"), cx);
                w.render_frame(cx);
                assert_eq!(app.read(cx).automation_ui.technical_run, Some(first));
                assert_eq!(app.read(cx).automation_ui.technical_scroll.offset().y, px(0.));
                assert!(w.try_find(log_id.clone()).is_none());
                w.find(format!("automation-run-logs-{first}"));
            }).unwrap();
            screenshot(&mut cx, handle, root, &format!("technical-{mode:?}-{width}.png"));
        }
    }
    println!("Technical run selection and scroll QA passed · {}", root.display());
}

fn screenshot_ordinary(
    store: Store,
    path: &Path,
    task: u64,
    run: u64,
    directory: &Path,
    name: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle) = with_app(store, path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::Runs);
        a.set_result_selection(Some(task), a.selected_run());
        a.note_selected_run(Some(run));
        a.show_run_details = false;
        a.show_run_result = true;
    });
    screenshot(&mut cx, handle, directory, name);
}

fn screenshot(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    directory: &Path,
    name: &str,
) {
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

fn refresh_preview(root: &Path) {
    let (store, report_task, _) = ordinary_fixture(root, OrdinaryState::Complete);
    let goal = store.owning_research_goal(report_task).unwrap();
    let path = root.join("workspace.sqlite3");
    let database = crate::sqlite_v4::SqliteV4::create_seeded(&path, root, 0x52_45_46, &store).unwrap();
    let lazy = database.load_store_lazy().unwrap();
    let reader = crate::history_reader::HistoryReader::start(&path, root, 0x52_45_46).unwrap();
    application().with_assets(crate::ui_assets::AppAssets).run(move |cx| {
        init(cx);
        open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1440.), px(920.)), cx))),
            titlebar: Some(TitlebarOptions {
                title: Some("Needs refresh fix — isolated test".into()),
                ..Default::default()
            }),
            ..Default::default()
        }, cx, |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, cx| {
                a.store = lazy;
                a.history_reader = Some(reader);
                a.set_appearance(Appearance::Dark, w, cx);
                a.open_task(goal, w, cx);
            });
            app
        }).unwrap();
        cx.activate(true);
    });
}

fn lazy_reader_check(root: &Path) {
    let directory = root.join("lazy-reader");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, report_task, report_run) = ordinary_fixture(&directory, OrdinaryState::Complete);
    let goal = store.owning_research_goal(report_task).unwrap();
    let steps = store.research_children(goal);
    let first = store.runs().iter().find(|run| run.research_kind == Some(ResearchKind::Collect)
        && store.owning_research_goal(run.task_id) == store.owning_research_goal(report_task)).unwrap();
    let (first_task, first_run, first_body) = (first.task_id, first.id, first.output.clone());
    let report_body = store.runs().iter().find(|run| run.id == report_run).unwrap().output.clone();
    for id in [first_run, report_run] {
        store.run_mut(id).unwrap().output.clear();
        store.mark_lazy_body(id);
    }
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            // No reader worker: hold each loading transition deterministically.
            a.store = store;
            a.open_task(goal, w, cx);
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, w, cx| {
        for mode in [Appearance::Light, Appearance::Dark] {
            app.update(cx, |a, cx| a.set_appearance(mode, w, cx));
            w.render_frame(cx);
            for step in &steps {
                w.find(format!("subtask-{step}"));
                assert!(w.try_find(format!("subtask-refresh-{step}")).is_none(),
                    "current lazy results and dependencies must not need refresh");
            }
        }
        app.update(cx, |a, _| a.store.clear_lazy_body(first_run));
        w.render_frame(cx);
        for step in &steps {
            w.find(format!("subtask-refresh-{step}"));
        }
        app.update(cx, |a, _| {
            a.store.mark_lazy_body(first_run);
            a.store.run_mut(first_run).unwrap().input_signature.push_str(" changed");
        });
        w.render_frame(cx);
        for step in &steps {
            w.find(format!("subtask-refresh-{step}"));
        }
        app.update(cx, |a, cx| {
            let signature = &mut a.store.run_mut(first_run).unwrap().input_signature;
            signature.truncate(signature.len() - " changed".len());
            a.open_task(first_task, w, cx);
            a.show_task_result(first_run, cx);
        });
        w.render_frame(cx);
        assert_eq!(w.find("run-result-kind").label(), Some("Loading saved report…"));
        assert!(w.find("research-result-scroll").visible());
        app.update(cx, |a, cx| {
            a.open_task(report_task, w, cx);
            a.show_task_result(report_run, cx);
            a.store.hydrate_run_body(first_run, first_body);
            cx.notify();
        });
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected_run(), Some(report_run), "an older body's arrival must not change selection");
        assert_eq!(w.find("run-result-kind").label(), Some("Loading saved report…"));
        app.update(cx, |a, cx| {
            a.run_body_state.insert(report_run, crate::history_reader::BodyLoad::Failed {
                error: "Isolated read failure".into(),
                retry_at: std::time::Instant::now() + Duration::from_secs(60),
            });
            cx.notify();
        });
        w.render_frame(cx);
        assert_eq!(w.find("run-result-kind").label(), Some("Report unavailable"));
        assert!(w.find("retry-result-body").visible());
        app.update(cx, |a, cx| {
            a.run_body_state.remove(&report_run);
            cx.notify();
        });
        w.render_frame(cx);
        assert_eq!(w.find("run-result-kind").label(), Some("Loading saved report…"));
        app.update(cx, |a, cx| {
            a.store.hydrate_run_body(report_run, report_body);
            cx.notify();
        });
        w.render_frame(cx);
        assert_eq!(w.find("run-result-kind").label(), Some("Final report"));
        assert!(w.try_find("retry-result-body").is_none());
        assert_eq!(app.read(cx).selected_run(), Some(report_run));
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(directory.join("hydrated-report.png")).unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("Lazy reader distinguishes loading, read failure and hydrated content without changing selection");
}

fn atomic_period_navigation_check(root: &Path) {
    let directory = root.join("atomic-period");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, first) = automation_fixture(&directory, AutomationState::ResearchComplete);
    let mut next = store.occurrence(first).unwrap().clone();
    next.id = 999;
    next.trigger_key = "manual:999".into();
    store.push_occurrence(next);
    let final_run = store.occurrence(first).unwrap().final_run_id.unwrap();
    let path = directory.join("workspace.sqlite3");
    let database = crate::sqlite_v4::SqliteV4::create_seeded(&path, &directory, 732, &store).unwrap();
    let lazy = database.load_store_lazy().unwrap();
    let reader = crate::history_reader::HistoryReader::start(&path, &directory, 732).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(), Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let handle: AnyWindowHandle = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, _| {
            a.store = lazy;
            a.history_reader = Some(reader);
            a.set_scope_identity(Scope::AutomationRuns);
        });
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap().into();
    let mut app = None;
    cx.update_window(handle, |root, w, cx| {
        let view = root.downcast::<base::Root>().unwrap().read(cx).view().clone().downcast::<TaskApp>().unwrap();
        view.update(cx, |a, cx| {
            a.open_occurrence_result(999, w, cx);
            assert_eq!(a.automation_ui.pending_period, Some(999));
            assert_eq!(a.period_overlay(), None, "Do not reveal an incomplete dialog");
            a.open_occurrence_result(first, w, cx);
            assert_eq!(a.automation_ui.pending_period, Some(first));
            assert_eq!(a.period_overlay(), None);
        });
        app = Some(view);
    }).unwrap();
    let app = app.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.poll_history_io(cx));
            w.render_frame(cx);
            let a = app.read(cx);
            assert_ne!(a.period_overlay(), Some(999), "Latest selection wins");
            if a.period_overlay().is_some() {
                assert!(a.pending_period_bodies(first).is_empty());
                assert!(w.try_find("automation-report-loading").is_none());
                let result = present_occurrence(&a.store, a.store.occurrence(first).unwrap());
                assert_eq!(result.reports.len(), 2);
                for report in &result.reports {
                    assert!(report.presentation.has_reader_body());
                    w.find(format!("automation-report-body-{first}-{}", report.presentation.run_id.unwrap()));
                }
            }
        }).unwrap();
        if cx.update(|cx| app.read(cx).period_overlay() == Some(first)) { break; }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.close_occurrence_result(w, cx);
            a.open_occurrence_result(first, w, cx);
            assert_eq!(a.period_overlay(), Some(first), "Cached result opens immediately");
            a.close_occurrence_result(w, cx);
            a.store.run_mut(final_run).unwrap().output.clear();
            a.store.mark_lazy_body(final_run);
            a.run_body_state.remove(&final_run);
            a.open_occurrence_result(first, w, cx);
            assert_eq!(a.automation_ui.pending_period, Some(first));
            a.navigate(Scope::Settings, w, cx);
            assert_eq!(a.automation_ui.pending_period, None);
            a.run_body_state.insert(final_run, crate::history_reader::BodyLoad::Failed {
                error: "Isolated read failure".into(),
                retry_at: std::time::Instant::now() + Duration::from_secs(60),
            });
            a.open_occurrence_result(first, w, cx);
            assert_eq!(a.period_overlay(), Some(first), "Read failures must remain reachable");
        });
        w.render_frame(cx);
        w.find("automation-report-unavailable");
    }).unwrap();
    println!("Period dialogs open after hydration; cached, superseded, cancelled and failed reads verified");
}

fn atomic_reader_navigation_check(root: &Path) {
    let directory = root.join("atomic-reader-navigation");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, report_task, report_run) = ordinary_fixture(&directory, OrdinaryState::Complete);
    let goal = store.owning_research_goal(report_task).unwrap();
    let steps = store.research_children(goal);
    let first_task = steps[0];
    let analysis_task = steps[1];
    let run_for = |task| store.runs().iter().rev().find(|run| run.task_id == task).unwrap().id;
    let first_run = run_for(first_task);
    let analysis_run = run_for(analysis_task);
    let path = directory.join("workspace.sqlite3");
    let incarnation = 0x41_54_4f_4d;
    let database = crate::sqlite_v4::SqliteV4::create_seeded(&path, &directory, incarnation, &store).unwrap();
    let mut lazy = database.load_store_lazy().unwrap();
    lazy.hydrate_run_body(first_run, database.body(first_run).unwrap().unwrap());
    let reader = crate::history_reader::HistoryReader::start(&path, &directory, incarnation).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(), Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.store = lazy;
            a.history_reader = Some(reader);
            a.open_sidebar_task(first_task, w, cx);
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected_run(), Some(first_run));
        app.update(cx, |a, cx| {
            a.request_sidebar_task(1, report_task, w, cx);
            assert_eq!(a.pending_result_navigation.as_ref().map(|pending| pending.run_id), Some(report_run));
            assert_eq!(a.selected_run(), Some(first_run), "retain the old reader while preparing the destination");
            a.request_sidebar_task(1, analysis_task, w, cx);
            assert_eq!(a.pending_result_navigation.as_ref().map(|pending| pending.run_id), Some(analysis_run));
            assert_eq!(a.selected_run(), Some(first_run), "a newer click replaces only the pending destination");
        });
        w.render_frame(cx);
        assert_eq!(w.find("run-result-kind").label(), Some("Intermediate evidence"));
    }).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.poll_history_io(cx));
            w.render_frame(cx);
            assert_ne!(app.read(cx).selected_run(), Some(report_run), "the obsolete destination must never take over");
            assert_eq!(w.find("run-result-kind").label(), Some("Intermediate evidence"));
        }).unwrap();
        if cx.update(|cx| app.read(cx).selected_run() == Some(analysis_run)) { break; }
        assert!(std::time::Instant::now() < deadline, "prepared result did not open");
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.store.run_mut(report_run).unwrap().output.clear();
            a.store.mark_lazy_body(report_run);
            a.run_body_state.remove(&report_run);
            a.request_sidebar_task(1, report_task, w, cx);
            assert!(a.pending_result_navigation.is_some());
            a.navigate(Scope::Settings, w, cx);
            assert!(a.pending_result_navigation.is_none());
        });
        w.render_frame(cx);
    }).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.poll_history_io(cx));
            w.render_frame(cx);
            assert_eq!(app.read(cx).scope(), Scope::Settings, "late body replies must not reopen a cancelled destination");
        }).unwrap();
        if cx.update(|cx| !app.read(cx).store.run_body_is_lazy(report_run)) { break; }
        assert!(std::time::Instant::now() < deadline, "cancelled read did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    println!("Prepared result navigation retains the reader, chooses the latest click and cancels on navigation");
}

fn copy_result_check(root: &Path) {
    let directory = root.join("copy-result");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, task, run) = ordinary_fixture(&directory, OrdinaryState::Partial);
    store.set_appearance(Appearance::Light);
    let expected = present_run(&store, store.runs().iter().find(|value| value.id == run).unwrap())
        .reader_text();
    let path = directory.join("copy.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle) = with_app(store, &path, 1000., 680., |a, _, _| {
        a.set_scope_identity(Scope::Runs);
        a.set_result_selection(Some(task), a.selected_run());
        a.note_selected_run(Some(run));
        a.show_run_details = false;
        a.show_run_result = true;
    });
    let previous = cx.update(|cx| cx.read_from_clipboard());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("run-actions-menu", cx);
        w.render_frame(cx);
        w.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    let copied = cx
        .update(|cx| cx.read_from_clipboard())
        .and_then(|item| item.text())
        .map(|text| text.to_string())
        .unwrap_or_default();
    cx.update(|cx| {
        cx.write_to_clipboard(previous.unwrap_or_else(|| ClipboardItem::new_string(String::new())))
    });
    assert_eq!(copied, expected);
    assert!(copied.contains("Ordinary partial report body"));
    assert!(copied.contains("Evidence gaps:"));
    assert!(!copied.contains(research::EVIDENCE_GAP_NOTE));
    println!("Copy result matches reader body and notice");
}

fn export_parity_check(root: &Path) -> Result<(), String> {
    let directory = root.join("export-parity");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, task, run) = ordinary_fixture(&directory, OrdinaryState::Partial);
    let expected = present_run(&store, store.runs().iter().find(|value| value.id == run).unwrap())
        .reader_text();
    let goal = store
        .task(task)
        .and_then(|task| task.parent_id)
        .ok_or("The export fixture lost its goal.")?;
    let export = store.research_export(goal)?;
    let (_, bytes) = export
        .files
        .iter()
        .find(|(name, _)| name.starts_with("report/"))
        .ok_or("The export fixture lost its report file.")?;
    let markdown = String::from_utf8(bytes.clone()).map_err(|error| error.to_string())?;
    if !markdown.starts_with(&expected) {
        return Err("Exported report does not start with the shared reader body and notice.".into());
    }
    if markdown.matches(research::EVIDENCE_GAP_NOTICE).count() != 1 {
        return Err("Exported report duplicated the evidence notice.".into());
    }
    if markdown.contains(research::EVIDENCE_GAP_NOTE) {
        return Err("Exported report leaked the internal evidence marker.".into());
    }
    println!("Export report matches reader body and notice");
    Ok(())
}
