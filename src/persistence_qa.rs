//! T14-S0 isolated persistence measurements.
//!
//! This harness owns its own fixtures and writes only under
//! `.local/t14-perf/<pid>/<mode>/`. It never opens a configured/real workspace.
//! It does not change persistence behaviour: it times the current code paths.
//!
//! Modes:
//!   inventory    describe the retained 43,200-period fixture (read-only)
//!   occ43200     measure an unchanged copy of that fixture
//!   small        generate and measure an ordinary-size fixture
//!   body-heavy   generate and measure a report-body-heavy fixture
//!   retry-heavy  generate and measure a history with retried older periods
//!   xl           generate and measure a larger representative history (opt-in)
//!
//! Migration, crash and concurrency modes belong to T14-S5/T15 and are not
//! implemented here.

use crate::app::{Scope, TaskApp};
use crate::automation::{
    AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    OccurrenceReason, OccurrenceStatus, SkipCode, TimeWindow, TriggerSource,
};
use crate::automation_schedule::{DataWindow, Schedule, ScheduleSpec};
use crate::domain::{AgentRun, Stage, Store};
use crate::persistence::Storage;
use crate::persistence_metrics as metrics;
use gpui_kit::{test::TestWindowExt, *};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const RETAINED_FIXTURE: &str =
    ".local/automation-perf/51782/workspace.sqlite3";
const RETAINED_FIXTURE_SHA256: &str =
    "f105d46ee44b0eceafeffff443d6b0113c37e7b97591e4dc425ba7b4299c67ec";

pub fn requested_mode() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--qa-persistence-perf" {
            return Some(args.next().unwrap_or_else(|| "occ43200".into()));
        }
        if let Some(value) = arg.strip_prefix("--qa-persistence-perf=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Identity of the executable image captured once at startup. The harness must
/// be launched from a frozen copy; hashing the path later can identify an
/// on-disk replacement instead of the running image.
static EXECUTABLE_SHA256: OnceLock<String> = OnceLock::new();

fn capture_executable_identity() {
    EXECUTABLE_SHA256.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| fs::File::open(path).ok())
            .and_then(|mut file| hash_reader(&mut file).ok())
            .unwrap_or_default()
    });
}

fn executable_sha256() -> String {
    EXECUTABLE_SHA256.get().cloned().unwrap_or_default()
}

pub fn run(mode: &str) {
    capture_executable_identity();
    let root = run_root(mode);
    if root.exists() {
        let _ = fs::remove_dir_all(&root);
    }
    if let Err(error) = fs::create_dir_all(&root) {
        eprintln!("PERSISTENCE_PERF mode={mode} status=failed error={error}");
        std::process::exit(1);
    }
    let result = match mode {
        "inventory" => inventory(&root),
        "occ43200" => occ43200(&root),
        "small" => generated(&root, "small", FixtureSpec::small()),
        "body-heavy" => generated(&root, "body-heavy", FixtureSpec::body_heavy()),
        "retry-heavy" => generated(&root, "retry-heavy", FixtureSpec::retry_heavy()),
        "xl" => generated(&root, "xl", FixtureSpec::xl()),
        "migration" | "crash" | "concurrency" => Err(format!(
            "mode '{mode}' is a T14-S5/T15 mode and is not implemented in T14-S0"
        )),
        other => Err(format!(
            "unknown mode '{other}'; expected inventory|occ43200|small|body-heavy|retry-heavy|xl"
        )),
    };
    match result {
        Ok(()) => println!(
            "PERSISTENCE_PERF mode={mode} status=ok root={}",
            root.display()
        ),
        Err(error) => {
            eprintln!("PERSISTENCE_PERF mode={mode} status=failed error={error}");
            std::process::exit(1);
        }
    }
}

fn run_root(mode: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".local/t14-perf")
        .join(std::process::id().to_string())
        .join(mode)
}

fn retained_fixture_path() -> Result<PathBuf, String> {
    let path = std::env::current_dir()
        .map_err(|error| format!("current directory unavailable: {error}"))?
        .join(RETAINED_FIXTURE);
    if !path.is_file() {
        return Err(format!(
            "retained fixture is missing at {}; T14-S0 never generates or substitutes it",
            path.display()
        ));
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

fn inventory(root: &Path) -> Result<(), String> {
    let source = retained_fixture_path()?;
    let source_sha256 = hash_file(&source)?;
    let source_bytes = fs::metadata(&source)
        .map_err(|error| format!("cannot stat fixture: {error}"))?
        .len();
    if source_sha256 != RETAINED_FIXTURE_SHA256 {
        eprintln!(
            "WARNING: retained fixture hash changed: expected {RETAINED_FIXTURE_SHA256}, found {source_sha256}"
        );
    }

    let store = Storage::new(source.clone())
        .load()?
        .ok_or("retained fixture returned no store")?;

    let mut logical = serde_json::json!({
        "occurrences": store.all_occurrences().len(),
        "automations": store.automations().len(),
        "runs": store.runs().len(),
        "tasks": store.tasks().len(),
        "projects": store.projects().len(),
        "topics": store.topics().len(),
        "research_requests": store.research_requests().len(),
        "unread_occurrences": store.all_occurrences().iter().filter(|occurrence| occurrence.unread).count(),
        "runs_with_body": store.runs().iter().filter(|run| !run.output.trim().is_empty()).count(),
        "run_body_bytes": store.runs().iter().map(|run| run.output.len()).sum::<usize>(),
        "occurrence_status": status_breakdown(&store),
    });

    let table_stats = sqlite_table_stats(&source)?;
    logical["tables"] = table_stats.clone();
    logical["distinct_occurrence_snapshots"] = distinct_snapshot_count(&source)?.into();
    logical["user_version"] = sqlite_user_version(&source)?.into();

    let report = serde_json::json!({
        "mode": "inventory",
        "fixture": {
            "path": source.display().to_string(),
            "sha256": source_sha256,
            "expected_sha256": RETAINED_FIXTURE_SHA256,
            "bytes": source_bytes,
        },
        "logical": logical,
        "hardware": hardware(),
        "profile": profile(),
        "peak_rss_bytes": peak_rss_bytes(),
    });
    write_json(root.join("report.json"), &report)?;
    print_text_report(&report);
    Ok(())
}

fn status_breakdown(store: &Store) -> serde_json::Value {
    let mut counts = std::collections::BTreeMap::new();
    for occurrence in store.all_occurrences() {
        *counts
            .entry(occurrence.status.label().to_string())
            .or_insert(0usize) += 1;
    }
    serde_json::to_value(counts).unwrap_or_default()
}

fn sqlite_user_version(path: &Path) -> Result<i64, String> {
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot open fixture read-only: {error}"))?;
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("cannot read user_version: {error}"))
}

fn distinct_snapshot_count(path: &Path) -> Result<i64, String> {
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot open fixture read-only: {error}"))?;
    connection
        .query_row(
            "SELECT COUNT(DISTINCT json_extract(payload,'$.snapshot')) FROM automation_occurrences",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("cannot count distinct snapshots: {error}"))
}

fn sqlite_table_stats(path: &Path) -> Result<serde_json::Value, String> {
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot open fixture read-only: {error}"))?;
    let mut result = serde_json::Map::new();
    for table in [
        "tasks",
        "projects",
        "runs",
        "policies",
        "automations",
        "automation_occurrences",
        "workspace",
    ] {
        let (rows, bytes): (i64, i64) = connection
            .query_row(
                &format!("SELECT COUNT(*), COALESCE(SUM(LENGTH(payload)),0) FROM {table}"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| format!("cannot read {table} stats: {error}"))?;
        result.insert(
            table.to_string(),
            serde_json::json!({"rows": rows, "payload_bytes": bytes}),
        );
    }
    Ok(serde_json::Value::Object(result))
}

// ---------------------------------------------------------------------------
// 43,200 retained fixture
// ---------------------------------------------------------------------------

fn occ43200(root: &Path) -> Result<(), String> {
    let source = retained_fixture_path()?;
    let source_sha256 = hash_file(&source)?;
    let source_bytes = fs::metadata(&source)
        .map_err(|error| format!("cannot stat fixture: {error}"))?
        .len();
    if source_sha256 != RETAINED_FIXTURE_SHA256 {
        eprintln!(
            "WARNING: retained fixture hash changed: expected {RETAINED_FIXTURE_SHA256}, found {source_sha256}"
        );
    }

    let work = root.join("work");
    fs::create_dir_all(work.join("project"))
        .map_err(|error| format!("cannot create fixture directory: {error}"))?;
    let load_path = work.join("workspace.sqlite3");
    let copy = copy_fixture(&source, &load_path)?;

    // Load: first (cold in this process) and two later reads.
    let storage = Storage::new(load_path.clone());
    let started = Instant::now();
    let store = storage
        .load()?
        .ok_or("copied fixture returned no store")?;
    metrics::record_external("load.wall", started.elapsed());
    for _ in 0..2 {
        let started = Instant::now();
        let reloaded = Storage::new(load_path.clone())
            .load()?
            .ok_or("copied fixture returned no store")?;
        std::hint::black_box(reloaded);
        metrics::record_external("load.wall", started.elapsed());
    }

    let checkpoint_after_load = peak_rss_bytes();

    // Main-thread costs that are not the SQLite call itself.
    let comparison = store.clone();
    for _ in 0..10 {
        metrics::measure("probe.clone", || std::hint::black_box(store.clone()));
    }
    for _ in 0..10 {
        metrics::measure("probe.equal", || {
            std::hint::black_box(store == comparison)
        });
    }
    let mut reconciled = store.clone();
    for _ in 0..10 {
        metrics::measure("probe.reconcile", || {
            reconciled.reconcile_goal_approvals()
        });
    }
    for _ in 0..5 {
        metrics::measure("probe.serialize", || {
            std::hint::black_box(serde_json::to_value(&store).unwrap_or_default())
        });
    }

    let probe_occurrence = store
        .all_occurrences()
        .iter()
        .find(|occurrence| occurrence.unread)
        .map(|occurrence| occurrence.id)
        .or_else(|| {
            store
                .all_occurrences()
                .first()
                .map(|occurrence| occurrence.id)
        })
        .ok_or("fixture has no occurrences")?;
    for index in 0..10 {
        let command = crate::domain_command::Command::SetRead {
            occurrence: probe_occurrence,
            unread: index % 2 == 0,
        };
        let id = command.id();
        let class = command.class();
        if let Ok(prepared) = metrics::measure("probe.command_prepare", || {
            store.prepare_command(id, class, command)
        }) {
            drop(prepared);
        }
    }

    // Cold save on a fresh handle (includes the revalidation read), then warm saves.
    let cold_path = work.join("cold.sqlite3");
    fs::copy(&source, &cold_path).map_err(|error| format!("cannot copy fixture: {error}"))?;
    let cold_storage = Storage::new(cold_path.clone());
    let started = Instant::now();
    cold_storage.save(&store)?;
    metrics::record_external("save.cold_wall", started.elapsed());
    let warm_save_samples = 10;
    for _ in 0..warm_save_samples {
        let started = Instant::now();
        cold_storage.save(&store)?;
        metrics::record_external("save.warm_wall", started.elapsed());
    }
    let db_bytes_after_saves = fs::metadata(&cold_path)
        .map_err(|error| format!("cannot stat saved fixture: {error}"))?
        .len();
    let checkpoint_after_saves = peak_rss_bytes();

    // App path on a fresh copy.
    let app_path = work.join("app.sqlite3");
    fs::copy(&source, &app_path).map_err(|error| format!("cannot copy fixture: {error}"))?;
    let app = app_suite(
        &app_path,
        20,
        20,
        store.all_occurrences().len(),
        store.runs().len(),
    )?;
    let checkpoint_after_app = peak_rss_bytes();

    let extra = serde_json::json!({
        "fixture": {
            "source_path": source.display().to_string(),
            "source_sha256": source_sha256,
            "source_bytes": source_bytes,
            "copy_sha256": copy.sha256,
            "copy_bytes": copy.bytes,
            "app_copy_sha256": hash_file(&app_path)?,
            "db_bytes_after_saves": db_bytes_after_saves,
            "logical": {
                "occurrences": store.all_occurrences().len(),
                "runs": store.runs().len(),
                "tasks": store.tasks().len(),
                "automations": store.automations().len(),
                "unread": store.all_occurrences().iter().filter(|occurrence| occurrence.unread).count(),
                "run_body_bytes": store.runs().iter().map(|run| run.output.len()).sum::<usize>(),
            },
        },
        "app": app,
        "sampling": sampling_note(warm_save_samples),
        "memory": {
            "peak_after_load_bytes": checkpoint_after_load,
            "peak_after_saves_bytes": checkpoint_after_saves,
            "peak_after_app_bytes": checkpoint_after_app,
            "peak_final_bytes": peak_rss_bytes(),
        },
    });
    write_report(root, "occ43200", extra)
}

// ---------------------------------------------------------------------------
// Generated fixtures
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct FixtureSpec {
    occurrences: usize,
    materialized: usize,
    body_bytes: usize,
    retried: usize,
}

impl FixtureSpec {
    fn small() -> Self {
        Self {
            occurrences: 500,
            materialized: 20,
            body_bytes: 8 * 1024,
            retried: 0,
        }
    }
    fn body_heavy() -> Self {
        Self {
            occurrences: 5_000,
            materialized: 400,
            body_bytes: 50 * 1024,
            retried: 0,
        }
    }
    fn retry_heavy() -> Self {
        Self {
            occurrences: 43_200,
            materialized: 200,
            body_bytes: 8 * 1024,
            retried: 200,
        }
    }
    fn xl() -> Self {
        Self {
            occurrences: 129_600,
            materialized: 1_000,
            body_bytes: 20 * 1024,
            retried: 0,
        }
    }
}

fn generated(root: &Path, label: &str, spec: FixtureSpec) -> Result<(), String> {
    let work = root.join("work");
    let project = work.join("project");
    fs::create_dir_all(&project).map_err(|error| format!("cannot create fixture: {error}"))?;
    let path = work.join("workspace.sqlite3");

    let build_started = Instant::now();
    let store = build_fixture(&project, spec)?;
    metrics::record_external("fixture.build_wall", build_started.elapsed());

    let storage = Storage::new(path.clone());
    let save_started = Instant::now();
    storage.save(&store)?;
    metrics::record_external("fixture.save_wall", save_started.elapsed());
    let db_bytes = fs::metadata(&path)
        .map_err(|error| format!("cannot stat fixture: {error}"))?
        .len();
    let db_sha256 = hash_file(&path)?;

    let load_started = Instant::now();
    let loaded = Storage::new(path.clone())
        .load()?
        .ok_or("generated fixture returned no store")?;
    metrics::record_external("load.wall", load_started.elapsed());
    if loaded.tasks().len() != store.tasks().len()
        || loaded.runs().len() != store.runs().len()
        || loaded.all_occurrences().len() != store.all_occurrences().len()
    {
        return Err("generated fixture did not round-trip its logical counts".into());
    }
    let checkpoint_after_load = peak_rss_bytes();

    let comparison = loaded.clone();
    for _ in 0..10 {
        metrics::measure("probe.clone", || {
            std::hint::black_box(loaded.clone())
        });
    }
    for _ in 0..10 {
        metrics::measure("probe.equal", || {
            std::hint::black_box(loaded == comparison)
        });
    }

    let save_samples = match label {
        "small" => 20,
        "body-heavy" => 10,
        "retry-heavy" => 5,
        "xl" => 5,
        _ => 10,
    };
    for _ in 0..save_samples {
        let started = Instant::now();
        Storage::new(path.clone()).save(&loaded)?;
        metrics::record_external("save.warm_wall", started.elapsed());
    }
    let checkpoint_after_saves = peak_rss_bytes();

    let app_path = work.join("app.sqlite3");
    fs::copy(&path, &app_path).map_err(|error| format!("cannot copy fixture: {error}"))?;
    let app = app_suite(
        &app_path,
        if label == "xl" { 5 } else { 20 },
        20,
        store.all_occurrences().len(),
        store.runs().len(),
    )?;
    let checkpoint_after_app = peak_rss_bytes();

    let extra = serde_json::json!({
        "fixture": {
            "label": label,
            "path": path.display().to_string(),
            "generated_at_unix": now(),
            "sha256": db_sha256,
            "bytes": db_bytes,
            "spec": {
                "occurrences": spec.occurrences,
                "materialized": spec.materialized,
                "body_bytes": spec.body_bytes,
                "retried": spec.retried,
            },
            "sampling": sampling_note(save_samples),
            "logical": {
                "occurrences": store.all_occurrences().len(),
                "runs": store.runs().len(),
                "tasks": store.tasks().len(),
                "automations": store.automations().len(),
                "run_body_bytes": store.runs().iter().map(|run| run.output.len()).sum::<usize>(),
            },
            "note": "generated fixture hashes depend on the generation timestamp and are not comparable across runs",
        },
        "app": app,
        "sampling": sampling_note(save_samples),
        "memory": {
            "peak_after_load_bytes": checkpoint_after_load,
            "peak_after_saves_bytes": checkpoint_after_saves,
            "peak_after_app_bytes": checkpoint_after_app,
            "peak_final_bytes": peak_rss_bytes(),
        },
    });
    write_report(root, label, extra)
}

fn build_fixture(project: &Path, spec: FixtureSpec) -> Result<Store, String> {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name = "Persistence fixture".into();
    store.project_at_mut(0).unwrap().directory = project.to_string_lossy().into_owned();
    store.set_default_model("qa/automation".into());

    let objective = "Track one changed public value and cite its source.";
    let expected = "A short report with sources and evidence gaps.";
    let template = crate::automation_execution::build_template(objective, expected)?;
    let schedule = Schedule {
        spec: ScheduleSpec::Interval { minutes: 1 },
        timezone: "UTC".into(),
    };
    let id = store.create_automation_draft(
        1,
        AutomationShape::Research,
        "Persistence fixture".into(),
        objective.into(),
        expected.into(),
        schedule,
        AutomationPolicy::default(),
        Some(template.clone()),
        now(),
    )?;
    store.approve_and_enable(id, vec![], now())?;

    let report = report_body(spec.body_bytes);
    let start = now() - (spec.occurrences as i64) * 60;
    for index in 0..spec.occurrences {
        let at = start + index as i64 * 60;
        let key = crate::automation::scheduled_trigger_key(id, at);
        let status = match index % 10 {
            0 => OccurrenceStatus::Failed,
            1..=7 => OccurrenceStatus::Succeeded,
            _ => OccurrenceStatus::Skipped,
        };
        let mut occurrence = AutomationOccurrence {
            id: 0,
            automation_id: id,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: key,
            scheduled_for: Some(at),
            requested_at: None,
            actual_started_at: Some(at + 5),
            timezone: "UTC".into(),
            reference_time: at,
            data_window: TimeWindow::resolved(
                DataWindow {
                    start_utc: at - 86_400,
                    end_utc: at,
                },
                "previous day",
            ),
            snapshot: AutomationOccurrenceSnapshot {
                objective: objective.into(),
                expected_result: expected.into(),
                shape: Some(AutomationShape::Research),
                template: template.clone(),
                models: std::array::from_fn(|_| "qa/automation".to_string()),
                agents: Default::default(),
                materials: vec![],
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "qa".into(),
            },
            status,
            reason: (status == OccurrenceStatus::Skipped).then(|| {
                OccurrenceReason::new(SkipCode::Expired, "Fixture skipped period")
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
        occurrence.id = store.next_occurrence_id()?;
        if index < spec.materialized && status != OccurrenceStatus::Skipped {
            let mut materialized = store.materialize_occurrence(occurrence)?;
            let goal = materialized
                .research_goal_id
                .ok_or("materialized period has no research goal")?;
            let mut run_ids = vec![];
            for task in store.research_children(goal) {
                let task_status = if status == OccurrenceStatus::Failed {
                    "Failed"
                } else {
                    "Succeeded"
                };
                let run = AgentRun {
                    automation_occurrence_id: Some(materialized.id),
                    planning_owner: None,
                    deadline_at: None,
                    auto_retry_attempt: 0,
                    retry_not_before: None,
                    project_plan: None,
                    id: store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1,
                    task_id: task,
                    task_title: "Fixture step".into(),
                    directory: project.to_string_lossy().into_owned(),
                    prompt: "Persistence fixture prompt".into(),
                    started_at: chrono::DateTime::from_timestamp(at, 0)
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    queued_at: String::new(),
                    workspace_source: None,
                    ended_at: chrono::DateTime::from_timestamp(at + 60, 0)
                        .map(|value| value.to_rfc3339()),
                    session_id: Some(format!("qa-session-{index}-{task}")),
                    exit_code: Some(0),
                    agent: "OpenCode".into(),
                    model: "qa/automation".into(),
                    stage: Stage::Done,
                    status: task_status.into(),
                    logs: (0..12)
                        .map(|line| format!("[log] representative automation line {line}"))
                        .collect(),
                    output: report.clone(),
                    attachments: vec![],
                    research_kind: Some(crate::research::ResearchKind::Advise),
                    sources: vec![crate::research::Source {
                        title: "Fixture source".into(),
                        url: format!("https://example.com/fixture/{index}"),
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
            *store
                .occurrence_mut(materialized_id)
                .ok_or("materialized period disappeared")? = materialized;
        } else {
            store.record_occurrence_without_research(occurrence)?;
        }
    }

    // Retried older periods keep their reference time but move to the top of the
    // shared activity order. This exercises ordering/pagination, not a new run.
    if spec.retried > 0 {
        let mut remaining = spec.retried;
        let ids: Vec<u64> = store
            .all_occurrences()
            .iter()
            .filter(|occurrence| occurrence.status != OccurrenceStatus::Skipped)
            .map(|occurrence| occurrence.id)
            .take(remaining)
            .collect();
        for id in ids {
            if let Some(mut occurrence) = store.occurrence_mut(id) {
                occurrence.retry_requested_at = Some(occurrence.reference_time + 10_000);
            }
        }
    }
    Ok(store)
}

fn report_body(bytes: usize) -> String {
    const UNIT: &str =
        "Representative report paragraph used to measure stored report body cost. ";
    let mut body = String::with_capacity(bytes + UNIT.len());
    while body.len() < bytes {
        body.push_str(UNIT);
    }
    body
}

// ---------------------------------------------------------------------------
// App path
// ---------------------------------------------------------------------------

fn app_suite(
    path: &Path,
    interaction_samples: usize,
    streaming_samples: usize,
    expected_occurrences: usize,
    expected_runs: usize,
) -> Result<serde_json::Value, String> {
    let app_path = path.to_path_buf();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let open_started = Instant::now();
    let mut app_entity = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(app_path.clone()), w, cx);
                a.set_scope_identity(Scope::AutomationRuns);
                a.automation_ui.selected = None;
                a.automation_ui.result_filter = None;
                a.automation_ui.history_limit = 20;
                a.automation_fake_mode = true;
                cx.notify();
            });
            app_entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .map_err(|_| "open_window failed".to_string())?;
    let app = app_entity.ok_or("app entity was not created")?;
    let handle: AnyWindowHandle = window.into();
    metrics::record_external("app.open_wall", open_started.elapsed());

    let (attached, attach_error, attach_occurrences, attach_runs) = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| {
                (
                    a.storage.is_some(),
                    a.storage_error.clone(),
                    a.store.all_occurrences().len(),
                    a.store.runs().len(),
                )
            })
        })
        .map_err(|_| "cannot read app state".to_string())?;
    if !attached {
        return Err("storage did not attach; the measurement is invalid".into());
    }
    if !attach_error.is_empty() {
        return Err(format!(
            "storage reported an error after attach: {attach_error}"
        ));
    }
    if attach_occurrences != expected_occurrences || attach_runs != expected_runs {
        return Err(format!(
            "attached store counts differ: occurrences {attach_occurrences}/{expected_occurrences}, runs {attach_runs}/{expected_runs}"
        ));
    }
    let disk_after_attach = Storage::new(app_path.clone())
        .load()?
        .ok_or("attached database could not be reloaded")?;
    if disk_after_attach.all_occurrences().len() != expected_occurrences
        || disk_after_attach.runs().len() != expected_runs
    {
        return Err("reloaded database counts do not match the fixture; attach save is not durable".into());
    }
    let attach_state = serde_json::json!({
        "storage_attached": attached,
        "storage_error": attach_error,
        "occurrences": attach_occurrences,
        "runs": attach_runs,
        "disk_occurrences": disk_after_attach.all_occurrences().len(),
        "disk_runs": disk_after_attach.runs().len(),
    });

    let first_render = Instant::now();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .map_err(|_| "first render failed".to_string())?;
    metrics::record_external("app.first_render_wall", first_render.elapsed());

    for _ in 0..20 {
        let started = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |_, cx| cx.notify());
            w.render_frame(cx);
        })
        .map_err(|_| "results render failed".to_string())?;
        metrics::record_external("app.results_render_wall", started.elapsed());
    }

    // Per-definition history: select the fixture automation and render.
    let definition = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| a.store.automations().first().map(|value| value.id))
        })
        .map_err(|_| "cannot read automation id".to_string())?;
    if let Some(definition) = definition {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.set_scope_identity(Scope::Automations);
                a.automation_ui.selected = Some(definition);
                cx.notify();
            });
        })
        .map_err(|_| "cannot select automation".to_string())?;
        for _ in 0..20 {
            let started = Instant::now();
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |_, cx| cx.notify());
                w.render_frame(cx);
            })
            .map_err(|_| "history render failed".to_string())?;
            metrics::record_external("app.history_render_wall", started.elapsed());
        }
    }

    // Forced interaction: rename one automation, persist, then verify durability
    // outside the timed span. `update_window` returns unit, so persistence is
    // never inferred from the closure's return value.
    let mut interaction_durable = 0usize;
    let mut interaction_failed = 0usize;
    let mut last_name = String::new();
    let commits_before = metrics::counter("persist.commit.ok");
    for index in 0..interaction_samples {
        let started = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, _| {
                if let Some(mut definition) = a.store.automation_mut(definition.unwrap_or(0)) {
                    definition.name.push(if index % 2 == 0 { 'a' } else { 'b' });
                }
                a.persist_if_changed(true);
            });
            w.render_frame(cx);
        })
        .map_err(|_| "interaction failed".to_string())?;
        metrics::record_external("app.interaction_wall", started.elapsed());
        let (error, unsaved, name) = cx
            .update_window(handle, |_, _, cx| {
                app.read_with(cx, |a, _| {
                    (
                        a.storage_error.clone(),
                        a.has_unsaved_changes(),
                        a.store
                            .automations()
                            .first()
                            .map(|definition| definition.name.clone())
                            .unwrap_or_default(),
                    )
                })
            })
            .map_err(|_| "cannot verify interaction durability".to_string())?;
        if error.is_empty() && !unsaved {
            interaction_durable += 1;
        } else {
            interaction_failed += 1;
        }
        last_name = name;
    }
    if interaction_failed > 0 || interaction_durable != interaction_samples {
        return Err(format!(
            "forced-save baseline failed: {interaction_failed} of {interaction_samples} forced interactions were not durable ({interaction_durable} durable); a later cumulative save must not hide this"
        ));
    }
    let interaction_commits = metrics::counter("persist.commit.ok").saturating_sub(commits_before);
    let disk_after_interactions = Storage::new(app_path.clone())
        .load()?
        .ok_or("database could not be reloaded after interactions")?;
    let disk_interaction_name = disk_after_interactions
        .automations()
        .first()
        .map(|definition| definition.name.clone())
        .unwrap_or_default();
    if disk_interaction_name != last_name {
        return Err(format!(
            "forced interactions were not durable: memory name {last_name:?}, disk name {disk_interaction_name:?}"
        ));
    }
    if interaction_commits != interaction_durable as u64 {
        return Err(format!(
            "commit count {interaction_commits} does not match durable interaction count {interaction_durable}"
        ));
    }

    // Streaming-like updates: small run mutation + non-forced persist. A throttled
    // sample may legitimately not write, so actual commits and the durable end
    // state are recorded instead of assuming one write per sample.
    let mut streaming_commits = 0usize;
    let mut streaming_skipped = 0usize;
    let mut streaming_throttle_skips = 0usize;
    let mut streaming_durable = 0usize;
    let mut streaming_error_samples = 0usize;
    let mut previous_commits = metrics::counter("persist.commit.ok");
    let mut last_marker = String::new();
    for index in 0..streaming_samples {
        let marker = format!("[log] T14-S0 streaming sample {index}");
        let started = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, _| {
                if let Some(first_id) = a.store.runs().first().map(|run| run.id) {
                    if let Some(mut run) = a.store.run_mut(first_id) {
                        run.logs.push(marker.clone());
                    }
                }
                a.persist_if_changed(false);
            });
            w.render_frame(cx);
        })
        .map_err(|_| "streaming sample failed".to_string())?;
        metrics::record_external("app.streaming_wall", started.elapsed());
        let (error, unsaved, last_log) = cx
            .update_window(handle, |_, _, cx| {
                app.read_with(cx, |a, _| {
                    (
                        a.storage_error.clone(),
                        a.has_unsaved_changes(),
                        a.store
                            .runs()
                            .first()
                            .and_then(|run| run.logs.last().cloned())
                            .unwrap_or_default(),
                    )
                })
            })
            .map_err(|_| "cannot verify streaming durability".to_string())?;
        if !error.is_empty() {
            streaming_error_samples += 1;
            return Err(format!(
                "streaming sample {index} reported a storage error: {error}; throttle skips are not storage errors"
            ));
        }
        let commits = metrics::counter("persist.commit.ok");
        if commits == previous_commits {
            streaming_skipped += 1;
            if unsaved {
                streaming_throttle_skips += 1;
            }
        } else {
            streaming_commits += 1;
        }
        previous_commits = commits;
        if !unsaved {
            streaming_durable += 1;
        }
        last_marker = last_log;
    }
    let unsaved_at_end = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| a.has_unsaved_changes())
        })
        .map_err(|_| "cannot read final streaming state".to_string())?;
    let mut flushed_at_end = false;
    if unsaved_at_end {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, _| {
                a.persist_if_changed(true);
            });
            w.render_frame(cx);
        })
        .map_err(|_| "end-of-stream flush failed".to_string())?;
        flushed_at_end = true;
    }
    let unsaved_after_flush = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| a.has_unsaved_changes())
        })
        .map_err(|_| "cannot read flushed streaming state".to_string())?;
    if unsaved_after_flush {
        return Err("streaming end-of-run flush did not make the store durable".into());
    }
    if streaming_commits + streaming_skipped != streaming_samples {
        return Err(format!(
            "streaming classification is incomplete: {streaming_commits} commits + {streaming_skipped} skips != {streaming_samples} samples"
        ));
    }
    let disk_after_streaming = Storage::new(app_path.clone())
        .load()?
        .ok_or("database could not be reloaded after streaming")?;
    let disk_stream_marker = disk_after_streaming
        .runs()
        .first()
        .and_then(|run| run.logs.last().cloned())
        .unwrap_or_default();
    let disk_stream_counts_match = disk_after_streaming.all_occurrences().len()
        == expected_occurrences
        && disk_after_streaming.runs().len() == expected_runs;
    if !disk_stream_counts_match {
        return Err("database counts changed after streaming; measurement is invalid".into());
    }
    if streaming_commits > 0 && disk_stream_marker.is_empty() {
        return Err("streaming committed but the durable log marker is missing".into());
    }
    if streaming_commits > 0 && streaming_skipped == 0 && disk_stream_marker != last_marker {
        return Err(format!(
            "streaming end state is not durable: memory marker {last_marker:?}, disk marker {disk_stream_marker:?}"
        ));
    }

    // Result dialog on a period with a final run plus render.
    let dialog_id = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| {
                let candidates: Vec<_> = a
                    .store
                    .all_occurrences()
                    .iter()
                    .filter(|occurrence| occurrence.final_run_id.is_some())
                    .collect();
                candidates
                    .iter()
                    .find(|occurrence| !occurrence.unread)
                    .or_else(|| candidates.first())
                    .map(|occurrence| occurrence.id)
            })
        })
        .map_err(|_| "cannot read dialog period".to_string())?;
    let mut dialog_opened = false;
    if let Some(dialog_id) = dialog_id {
        let started = Instant::now();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.open_occurrence_result(dialog_id, w, cx));
            w.render_frame(cx);
        })
        .map_err(|_| "dialog open failed".to_string())?;
        dialog_opened = cx
            .update_window(handle, |_, _, cx| {
                app.read_with(cx, |a, _| a.automation_ui.result_dialog == Some(dialog_id))
            })
            .map_err(|_| "cannot read dialog state".to_string())?;
        metrics::record_external("app.dialog_open_wall", started.elapsed());
        for _ in 0..10 {
            let started = Instant::now();
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |_, cx| cx.notify());
                w.render_frame(cx);
            })
            .map_err(|_| "dialog render failed".to_string())?;
            metrics::record_external("app.dialog_render_wall", started.elapsed());
        }
    }

    // Synthetic presentation inference: these are copies the UI would receive
    // through `present_occurrence`, not real body-read counters. Product code has
    // no body-read counter yet; T15 must add one.
    let (page_bytes, page_rows, materialized_bytes, materialized_rows, dialog_bytes, load_body_bytes) = cx
        .update_window(handle, |_, _, cx| {
            app.read_with(cx, |a, _| {
                let mut occurrences: Vec<_> = a.store.all_occurrences().iter().collect();
                occurrences.sort_by(|left, right| {
                    crate::automation_activity::compare_newest_first(left, right)
                });
                let page: Vec<_> = occurrences.iter().copied().take(20).collect();
                let page_bytes = page
                    .iter()
                    .map(|occurrence| {
                        presentation_bytes(
                            &crate::result_presentation::present_occurrence(&a.store, occurrence),
                        )
                    })
                    .sum::<usize>();
                let materialized: Vec<_> = occurrences
                    .iter()
                    .copied()
                    .filter(|occurrence| occurrence.final_run_id.is_some())
                    .take(20)
                    .collect();
                let materialized_bytes = materialized
                    .iter()
                    .map(|occurrence| {
                        presentation_bytes(
                            &crate::result_presentation::present_occurrence(&a.store, occurrence),
                        )
                    })
                    .sum::<usize>();
                let dialog_bytes = dialog_id
                    .and_then(|id| a.store.occurrence(id))
                    .map(|occurrence| {
                        presentation_bytes(
                            &crate::result_presentation::present_occurrence(&a.store, occurrence),
                        )
                    })
                    .unwrap_or(0);
                let load_body_bytes = a.store.runs().iter().map(|run| run.output.len()).sum::<usize>();
                (
                    page_bytes,
                    page.len(),
                    materialized_bytes,
                    materialized.len(),
                    dialog_bytes,
                    load_body_bytes,
                )
            })
        })
        .map_err(|_| "cannot measure presentation bytes".to_string())?;
    metrics::bump("synthetic.presentation.first_page_bytes", page_bytes as u64);
    metrics::bump(
        "synthetic.presentation.materialized_page_bytes",
        materialized_bytes as u64,
    );
    metrics::bump("synthetic.presentation.dialog_bytes", dialog_bytes as u64);
    metrics::bump("observed.load.run_body_bytes", load_body_bytes as u64);

    Ok(serde_json::json!({
        "attach": attach_state,
        "interaction_attempted": interaction_samples,
        "interaction_durable": interaction_durable,
        "interaction_failed": interaction_failed,
        "interaction_commits": interaction_commits,
        "interaction_disk_name_matches": true,
        "streaming_attempted": streaming_samples,
        "streaming_commits": streaming_commits,
        "streaming_skipped_no_write": streaming_skipped,
        "streaming_throttle_skips": streaming_throttle_skips,
        "streaming_error_samples": streaming_error_samples,
        "streaming_durable_samples": streaming_durable,
        "streaming_unsaved_at_end": unsaved_at_end,
        "streaming_unsaved_after_flush": unsaved_after_flush,
        "streaming_flushed_at_end": flushed_at_end,
        "streaming_disk_marker": disk_stream_marker,
        "first_page_rows": page_rows,
        "first_page_presentation_bytes_synthetic": page_bytes,
        "materialized_page_rows": materialized_rows,
        "materialized_page_presentation_bytes_synthetic": materialized_bytes,
        "dialog_opened": dialog_opened,
        "dialog_presentation_bytes_synthetic": dialog_bytes,
        "presentation_bytes_are_synthetic": true,
        "presentation_note": "inferred copies returned by the presentation selector; no product body-read counter exists yet",
    }))
}

fn sampling_note(warm_save_samples: usize) -> serde_json::Value {
    serde_json::json!({
        "warm_save_samples": warm_save_samples,
        "stable_p95_min_samples": 30,
        "note": "A p95 from fewer than 30 warm-save samples is indicative only. The sample count is reported so it is not read as a stable percentile.",
    })
}

fn presentation_bytes(presentation: &crate::result_presentation::ResultPresentation) -> usize {
    presentation.reader_markdown.len()
        + presentation
            .reports
            .iter()
            .map(|report| presentation_bytes(&report.presentation))
            .sum::<usize>()
}

// ---------------------------------------------------------------------------
// Reporting helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CopyDigest {
    sha256: String,
    bytes: u64,
}

fn copy_fixture(source: &Path, destination: &Path) -> Result<CopyDigest, String> {
    fs::copy(source, destination).map_err(|error| {
        format!(
            "cannot copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(CopyDigest {
        sha256: hash_file(destination)?,
        bytes: fs::metadata(destination)
            .map_err(|error| format!("cannot stat copy: {error}"))?
            .len(),
    })
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    hash_reader(&mut file)
}

fn hash_reader(reader: &mut impl Read) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1 << 20];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("cannot read executable: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_json(path: PathBuf, value: &serde_json::Value) -> Result<(), String> {
    fs::write(
        &path,
        serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn write_report(
    root: &Path,
    mode: &str,
    extra: serde_json::Value,
) -> Result<(), String> {
    let report = serde_json::json!({
        "mode": mode,
        "hardware": hardware(),
        "profile": profile(),
        "metrics": metrics::snapshot().iter().map(|span| {
            serde_json::json!({
                "name": span.name,
                "count": span.count,
                "total_us": span.total_us,
                "p50_us": span.p50_us,
                "p95_us": span.p95_us,
                "p99_us": span.p99_us,
                "max_us": span.max_us,
            })
        }).collect::<Vec<_>>(),
        "counters": metrics::counters(),
        "extra": extra,
        "peak_rss_bytes": peak_rss_bytes(),
    });
    write_json(root.join("report.json"), &report)?;
    let text = text_report(&report);
    fs::write(root.join("report.txt"), text)
        .map_err(|error| format!("cannot write text report: {error}"))?;
    print_text_report(&report);
    Ok(())
}

fn text_report(report: &serde_json::Value) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "mode: {}\n",
        report["mode"].as_str().unwrap_or("?")
    ));
    text.push_str(&format!(
        "hardware: {}\nprofile: {}\n",
        report["hardware"], report["profile"]
    ));
    if let Some(metrics) = report["metrics"].as_array() {
        text.push_str("\nname\tcount\ttotal_us\tp50_us\tp95_us\tp99_us\tmax_us\n");
        for span in metrics {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                span["name"].as_str().unwrap_or("?"),
                span["count"],
                span["total_us"],
                span["p50_us"],
                span["p95_us"],
                span["p99_us"],
                span["max_us"],
            ));
        }
    }
    if let Some(counters) = report["counters"].as_object() {
        text.push_str("\ncounters:\n");
        for (name, value) in counters {
            text.push_str(&format!("{name}\t{value}\n"));
        }
    }
    text.push_str(&format!(
        "\nextra:\n{}\n",
        serde_json::to_string_pretty(&report["extra"]).unwrap_or_default()
    ));
    text
}

fn print_text_report(report: &serde_json::Value) {
    print!("{}", text_report(report));
}

fn hardware() -> serde_json::Value {
    serde_json::json!({
        "cpu": sysctl("machdep.cpu.brand_string"),
        "memory_bytes": sysctl("hw.memsize"),
        "os_version": command_output("sw_vers", &["-productVersion"]),
        "os_build": command_output("sw_vers", &["-buildVersion"]),
        "arch": std::env::consts::ARCH,
    })
}

fn profile() -> serde_json::Value {
    let executable = std::env::current_exe().ok();
    serde_json::json!({
        "qa_feature": cfg!(feature = "qa"),
        "debug_assertions": cfg!(debug_assertions),
        "guess": if cfg!(debug_assertions) { "dev" } else { "release" },
        "executable": executable.as_ref().map(|path| path.display().to_string()),
        "executable_sha256": executable_sha256(),
        "argv": std::env::args().collect::<Vec<_>>(),
    })
}

fn sysctl(key: &str) -> String {
    command_output("sysctl", &["-n", key])
}

fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn peak_rss_bytes() -> u64 {
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            #[cfg(target_os = "macos")]
            {
                return usage.ru_maxrss as u64;
            }
            #[cfg(not(target_os = "macos"))]
            {
                return (usage.ru_maxrss as u64) * 1024;
            }
        }
        0
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
