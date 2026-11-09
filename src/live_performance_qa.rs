//! T14/T15 live performance gate.
//!
//! Opens a real v4 workspace through `Some(LiveStartup)` and the ordered worker,
//! then measures the actual task-editor save command and the real indexed
//! first-page path. UI synchronous work is timed separately from the wait for
//! durable acknowledgement. No sleeps are inserted inside a measured action.

use crate::app::TaskApp;
use crate::automation::{
    AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    OccurrenceStatus, TimeWindow, TriggerSource,
};
use crate::automation_schedule::{DataWindow, Schedule, ScheduleSpec};
use crate::domain::{AgentRun, Stage, Store};
use crate::persistence::Storage;
use crate::persistence_live;
use gpui_kit::{test::TestWindowExt, *};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INCARNATION: u64 = 0x4C_1F_3A_90;
const WARMUP: usize = 3;
const SAMPLES: usize = 30;
const TARGET_P95_UI_US: u64 = 50_000;

#[derive(Clone, Copy)]
struct Spec {
    occurrences: usize,
    materialized: usize,
    body_bytes: usize,
}

pub fn run() {
    let root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".local/live-performance-qa")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&root);
    let result = (|| {
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let small = Spec {
            occurrences: 500,
            materialized: 20,
            body_bytes: 8192,
        };
        let large = Spec {
            occurrences: 43_200,
            materialized: 240,
            body_bytes: 8192,
        };
        let huge = Spec {
            occurrences: 86_400,
            materialized: 480,
            body_bytes: 8192,
        };
        let mut fixture_results = Vec::new();
        for (label, spec) in [
            ("small", small),
            ("large-43200", large),
            ("huge-86400", huge),
        ] {
            match run_fixture(&root.join(label), spec) {
                Ok(value) => fixture_results.push(value),
                Err(error) => {
                    let partial = serde_json::json!({
                        "scope": "PARTIAL: live performance run failed before all fixtures completed.",
                        "hardware": hardware(),
                        "profile": profile(),
                        "samples": SAMPLES,
                        "warmup": WARMUP,
                        "target_p95_ui_us": TARGET_P95_UI_US,
                        "status": "failed",
                        "failed_fixture": label,
                        "error": error,
                        "completed_fixtures": fixture_results,
                    });
                    let path = root.join("report.json");
                    std::fs::write(&path, serde_json::to_vec_pretty(&partial).unwrap()).unwrap();
                    return Err(format!("{label}: {error}; partial report at {}", path.display()));
                }
            }
        }
        let small_result = fixture_results.remove(0);
        let large_result = fixture_results.remove(0);
        let huge_result = fixture_results.remove(0);
        let touched = measure_touched_collections(&root)?;
        let mut report = serde_json::json!({
            "scope": "Actual live TaskApp via Some(LiveStartup) + ordered worker; editor save, command-level automation SaveDraft, automation Enable/Pause, settings save, scoped task Undo and indexed first-page path.",
            "gate_scope": "Per-operation p95 gates use the 43,200-occurrence fixture (fixtures[1]). The 86,400-occurrence fixture is measured for RSS/DB/latency but is not gated by this run.",
            "hardware": hardware(),
            "profile": profile(),
            "samples": SAMPLES,
            "warmup": WARMUP,
            "target_p95_ui_us": TARGET_P95_UI_US,
            "body_read_instrumentation": null,
            "body_read_note": "No live run-body byte counter exists; do not invent one.",
            "fixtures": [small_result, large_result, huge_result],
            "touched_collection_cases": touched,
            "peak_rss_bytes": peak_rss_bytes(),
        });
        let path = root.join("report.json");
        let operations = [
            ("editor", &report["fixtures"][1]["editor"]["main_thread_total"]),
            ("automation_save_draft", &report["fixtures"][1]["automation_save_draft"]["main_thread_total"]),
            ("automation_enable", &report["fixtures"][1]["automation_lifecycle"]["enable"]["main_thread_total"]),
            ("automation_pause", &report["fixtures"][1]["automation_lifecycle"]["pause"]["main_thread_total"]),
            ("settings", &report["fixtures"][1]["settings"]["main_thread_total"]),
            ("scoped_task_undo", &report["fixtures"][1]["scoped_task_undo"]["main_thread_total"]),
            ("history_page", &report["fixtures"][1]["history_page"]["main_thread_total"]),
        ];
        let mut gates = serde_json::Map::new();
        let mut failures = Vec::new();
        for (name, stats) in operations {
            let p95 = stats["p95_us"].as_u64().unwrap_or(u64::MAX);
            let passed = p95 < TARGET_P95_UI_US;
            gates.insert(name.to_string(), serde_json::json!({
                "p95_us": p95,
                "target_us": TARGET_P95_UI_US,
                "passed": passed,
            }));
            if !passed {
                failures.push(format!("{name} p95 {p95}us"));
            }
        }
        report["gates"] = serde_json::Value::Object(gates);
        std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        if !failures.is_empty() {
            return Err(format!(
                "live p95 gates exceeded {TARGET_P95_UI_US}us: {}",
                failures.join(", ")
            ));
        }
        Ok(())
    })();
    match result {
        Ok(()) => println!("LIVE_PERFORMANCE_QA status=ok root={}", root.display()),
        Err(error) => {
            eprintln!("LIVE_PERFORMANCE_QA status=failed error={error}");
            std::process::exit(1);
        }
    }
}

fn seed_store(root: &Path, store: &Store) -> Result<std::path::PathBuf, String> {
    let path = root.join("workspace.sqlite3");
    crate::sqlite_v4::SqliteV4::create_seeded(&path, root, INCARNATION, store)
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn run_fixture(root: &Path, spec: Spec) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let (store, automation) = build_store(root, spec)?;
    let path = root.join("workspace.sqlite3");
    crate::sqlite_v4::SqliteV4::create_seeded(&path, root, INCARNATION, &store)
        .map_err(|error| error.to_string())?;
    let db_bytes = std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len();
    let (mut cx, handle, app) = open_live(root, &path)?;
    let task = cx
        .update(|cx| app.read(cx).store.tasks().iter().map(|task| task.id).max())
        .ok_or("fixture task is missing")?;
    let editor = measure_editor(&mut cx, handle, &app, task)?;
    let automation_save = measure_automation_save_draft(&mut cx, handle, &app, automation, &path, root)?;
    let automation_lifecycle =
        measure_automation_lifecycle(&mut cx, handle, &app, automation, &path, root)?;
    let settings = measure_settings(&mut cx, handle, &app, &path, root)?;
    let scoped_undo = measure_scoped_task_undo(&mut cx, handle, &app, task, &path, root)?;
    let append_ack = measure_append_ack_shared_snapshot(&mut cx, handle, &app, task, &path, root)?;
    let history = measure_history(&mut cx, handle, &app, automation, &path, root)?;
    let peak = peak_rss_bytes();
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(serde_json::json!({
        "occurrences": spec.occurrences,
        "materialized_runs": spec.materialized,
        "body_bytes": spec.body_bytes,
        "db_bytes": db_bytes,
        "editor": editor,
        "automation_save_draft": automation_save,
        "automation_lifecycle": automation_lifecycle,
        "settings": settings,
        "scoped_task_undo": scoped_undo,
        "append_ack_shared_snapshot": append_ack,
        "history_page": history,
        "peak_rss_bytes": peak,
    }))
}

fn build_store(root: &Path, spec: Spec) -> Result<(Store, u64), String> {
    let template = crate::automation_execution::build_template("Live perf", "Short report")
        .map_err(|error| error.to_string())?;
    let mut temp = Store::empty();
    temp.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    temp.set_default_model("qa/model".into());
    temp.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
    let automation = temp
        .create_automation_draft(
            1,
            AutomationShape::Research,
            "Live performance fixture".into(),
            "Live perf".into(),
            "Short report".into(),
            Schedule {
                spec: ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            AutomationPolicy::default(),
            Some(template.clone()),
            chrono::Utc::now().timestamp(),
        )
        .map_err(|error| error.to_string())?;
    let definition = temp
        .automation(automation)
        .ok_or("automation fixture missing")?
        .clone();
    let mut occurrences = Vec::with_capacity(spec.occurrences);
    let mut runs = Vec::new();
    let body = "L".repeat(spec.body_bytes.max(1));
    let start = chrono::Utc::now().timestamp() - spec.occurrences as i64;
    for index in 0..spec.occurrences {
        let at = start + index as i64;
        let status = match index % 10 {
            0 => OccurrenceStatus::Failed,
            1..=7 => OccurrenceStatus::Succeeded,
            _ => OccurrenceStatus::Skipped,
        };
        let mut occurrence = AutomationOccurrence {
            id: index as u64 + 1,
            automation_id: automation,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: format!("live-perf-{index}"),
            scheduled_for: Some(at),
            requested_at: None,
            actual_started_at: Some(at + 1),
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
                objective: "Live perf".into(),
                expected_result: "Short report".into(),
                shape: Some(AutomationShape::Research),
                template: template.clone(),
                models: std::array::from_fn(|_| "qa/model".to_string()),
                agents: Default::default(),
                materials: Vec::new(),
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "live-perf".into(),
            },
            status,
            reason: None,
            research_goal_id: None,
            research_goal_ids: Vec::new(),
            research_topic_id: None,
            run_ids: Vec::new(),
            final_run_id: None,
            unread: status == OccurrenceStatus::Succeeded && index % 3 == 0,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: at,
            deadline_at: None,
            retry_requested_at: None,
        };
        if index < spec.materialized && status != OccurrenceStatus::Skipped {
            let run_id = runs.len() as u64 + 1;
            runs.push(AgentRun {
                id: run_id,
                task_id: 1,
                task_title: "Live perf".into(),
                directory: root.to_string_lossy().into_owned(),
                prompt: "Live performance fixture".into(),
                started_at: String::new(),
                queued_at: String::new(),
                workspace_source: None,
                ended_at: None,
                session_id: None,
                exit_code: None,
                agent: "test".into(),
                model: "test".into(),
                stage: Stage::Done,
                status: "Succeeded".into(),
                logs: Vec::new(),
                output: body.clone(),
                attachments: Vec::new(),
                research_kind: None,
                sources: Vec::new(),
                input_signature: String::new(),
                automation_occurrence_id: Some(occurrence.id),
                planning_owner: None,
                deadline_at: None,
                auto_retry_attempt: 0,
                retry_not_before: None,
                project_plan: None,
            });
            occurrence.run_ids = vec![run_id];
            occurrence.final_run_id = Some(run_id);
        }
        occurrences.push(occurrence);
    }
    let mut store = Store::empty()
        .with_loaded_tracked(runs, vec![definition], occurrences)
        .map_err(|(_, error)| error)?;
    // with_loaded_tracked replaces collections only; restore the fixture's
    // model configuration on the final seeded store before Enable validation.
    store.set_default_model("qa/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    store
        .create_task("Live perf save", 1, Stage::Planned, None)
        .map_err(|error| error.to_string())?;
    Ok((store, automation))
}

fn open_live(root: &Path, path: &Path) -> Result<(HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>), String> {
    let storage = Storage::new(path.to_path_buf());
    let _ = storage.upgrade_paths(|_| {});
    let lock = storage.acquire_lock()?;
    let mut live = persistence_live::select_existing(&storage)?
        .ok_or("live startup did not select the fixture")?;
    let store = live.migrated_store.take();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1400.), px(900.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_loaded_storage_live(storage, Ok((lock, store)), Some(live.clone()), w, cx);
                assert!(a.live_required, "live performance fixture must require live");
                assert!(
                    a.live_persistence_active(),
                    "live performance fixture must attach the ordered worker"
                );
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .map_err(|error| format!("open live window failed: {error:?}"))?;
    let app = app.ok_or("live app missing")?;
    let handle: AnyWindowHandle = window.into();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let ready = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).live_required
                    && app.read(cx).live_persistence_active()
                    && !app.read(cx).live_has_pending_receipts()
            })
            .map_err(|error| format!("live pump failed: {error:?}"))?;
        if ready {
            return Ok((cx, handle, app));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = root;
    Err("live fixture did not become ready".into())
}

#[derive(Clone, Copy)]
enum PumpKind {
    Save,
    History,
}

fn pump_measured(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    kind: PumpKind,
    label: &str,
    predicate: impl Fn(&TaskApp) -> bool,
) -> Result<(u64, Result<(), String>), String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut main_us = 0u64;
    loop {
        let started = Instant::now();
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| match kind {
                    PumpKind::Save => {
                        let _ = a.poll_dispatch_admissions(cx);
                    }
                    PumpKind::History => a.poll_history_io(cx),
                });
                predicate(app.read(cx))
            })
            .map_err(|error| format!("pump failed: {error:?}"))?;
        main_us = main_us.saturating_add(started.elapsed().as_micros() as u64);
        if done {
            return Ok((main_us, Ok(())));
        }
        if Instant::now() > deadline {
            return Ok((main_us, Err(format!("timed out waiting for {label}"))));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn measure_editor(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    task: u64,
) -> Result<serde_json::Value, String> {
    let mut ui_sync = Vec::new();
    let mut ack_pump = Vec::new();
    let mut render = Vec::new();
    let mut main_total = Vec::new();
    let mut wall = Vec::new();
    let mut last_title = String::new();
    for index in 0..(WARMUP + SAMPLES) {
        let expected = format!("Live perf {index}");
        last_title = expected.clone();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.open_task(task, w, cx);
                a.begin_draft(task, w, cx);
                let title = a.draft.as_ref().unwrap().title.clone();
                title.update(cx, |state, cx| state.set_value(&expected, w, cx));
            });
            w.render_frame(cx);
        })
        .map_err(|error| format!("editor open failed: {error:?}"))?;
        let started = Instant::now();
        let mut inner = 0u128;
        let mut admitted = false;
        let mut blocked_impact = false;
        cx.update_window(handle, |_, w, cx| {
            let ui_started = Instant::now();
            let saved = app.update(cx, |a, cx| a.request_save_draft(w, cx));
            inner = ui_started.elapsed().as_micros();
            let a = app.read(cx);
            admitted = saved || a.live_primary.is_some();
            blocked_impact = a.research_save_impact.is_some();
        })
        .map_err(|error| format!("editor save failed: {error:?}"))?;
        if !admitted || blocked_impact {
            return Err(format!(
                "editor sample {index} was not admitted: admitted={admitted} impact={blocked_impact}"
            ));
        }
        let (pump_us, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "editor durable", |a| {
                !a.live_has_pending_receipts()
                    && a.live_primary.is_none()
                    && a.store.task(task).is_some_and(|task| task.title == expected)
            })?;
        if let Err(error) = pump_err {
            return Err(error);
        }
        let render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("render failed: {error:?}"))?;
        let render_us = render_started.elapsed().as_micros() as u64;
        let elapsed = started.elapsed().as_micros() as u64;
        let (error, storage_error, final_title) = cx.update(|cx| {
            let a = app.read(cx);
            (
                a.error.clone(),
                a.storage_error.clone(),
                a.store.task(task).map(|task| task.title.clone()),
            )
        });
        if !error.is_empty() || !storage_error.is_empty() {
            return Err(format!(
                "editor sample {index} failed after ACK: error={error:?} storage_error={storage_error:?}"
            ));
        }
        if final_title.as_deref() != Some(expected.as_str()) {
            return Err(format!(
                "editor sample {index} did not apply exact target: {final_title:?}"
            ));
        }
        if index >= WARMUP {
            let main = inner as u64 + pump_us + render_us;
            ui_sync.push(inner as u64);
            ack_pump.push(pump_us);
            render.push(render_us);
            main_total.push(main);
            wall.push(elapsed);
        }
    }
    let disk = read_v4_store_for(cx, app)?;
    let disk_title = disk.task(task).map(|task| task.title.clone());
    if disk_title.as_deref() != Some(last_title.as_str()) {
        return Err(format!(
            "final durable title mismatch: expected {last_title:?}, disk {disk_title:?}"
        ));
    }
    Ok(serde_json::json!({
        "samples": main_total.len(),
        "ui_sync": stats(&ui_sync),
        "ack_pump": stats(&ack_pump),
        "render": stats(&render),
        "main_thread_total": stats(&main_total),
        "durable_wall": stats(&wall),
        "final_title": last_title,
        "disk_title": disk_title,
    }))
}

fn read_v4_store_for(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
) -> Result<Store, String> {
    let (path, root, incarnation) = cx
        .update(|cx| {
            app.read(cx)
                .live_attach
                .clone()
                .map(|(path, root, incarnation)| (path, root, incarnation))
        })
        .ok_or("live attach target is missing")?;
    crate::sqlite_v4::SqliteV4::open(&path, &root, incarnation)?.load_store()
}


fn definition_input(
    definition: &crate::automation::AutomationDefinition,
    name: String,
) -> crate::domain_command::AutomationDraftInput {
    crate::domain_command::AutomationDraftInput {
        project: definition.project_id,
        shape: definition.shape,
        name,
        objective: definition.objective.clone(),
        expected_result: definition.expected_result.clone(),
        schedule: definition.schedule.clone(),
        policy: definition.policy.clone(),
        template: definition.template.clone(),
    }
}

fn open_automation_measure_surface(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    automation: u64,
) -> Result<(), String> {
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_automation(automation, w, cx));
        w.render_frame(cx);
    })
    .map_err(|error| format!("open automation failed: {error:?}"))?;
    let (scope, selected, creating) = cx.update(|cx| {
        let a = app.read(cx);
        (a.scope(), a.automation_ui.selected, a.automation_ui.creating)
    });
    if scope != crate::app::Scope::Automations || selected != Some(automation) || creating {
        return Err(format!(
            "automation measurement surface is wrong: scope={scope:?} selected={selected:?} creating={creating}"
        ));
    }
    Ok(())
}

fn open_settings_measure_surface(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
) -> Result<(), String> {
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.set_scope_identity(crate::app::Scope::Settings);
            a.set_settings_tab(crate::settings_views::SettingsTab::General);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("open settings failed: {error:?}"))?;
    let scope = cx.update(|cx| app.read(cx).scope());
    if scope != crate::app::Scope::Settings {
        return Err(format!("settings measurement surface is wrong: {scope:?}"));
    }
    Ok(())
}

fn measure_automation_save_draft(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    automation: u64,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let mut ui_sync = Vec::new();
    let mut ack_pump = Vec::new();
    let mut render = Vec::new();
    let mut main_total = Vec::new();
    let mut wall = Vec::new();
    let mut expected_name = String::new();
    open_automation_measure_surface(cx, handle, app, automation)?;
    let occurrences_before = cx.update(|cx| app.read(cx).store.all_occurrences().len());
    let runs_before = cx.update(|cx| app.read(cx).store.runs().len());
    for index in 0..(WARMUP + SAMPLES) {
        let expected = format!("Live automation {index}");
        expected_name = expected.clone();
        let input = cx
            .update(|cx| {
                app.read(cx)
                    .store
                    .automation(automation)
                    .map(|definition| definition_input(definition, expected.clone()))
            })
            .ok_or("automation save fixture missing")?;
        let started = Instant::now();
        let mut inner = 0u128;
        let mut admitted = false;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui_started = Instant::now();
                let run = a.execute_command(
                    crate::domain_command::Command::SaveDraft {
                        id: Some(automation),
                        input: input.clone(),
                        now: chrono::Utc::now().timestamp(),
                    },
                    cx,
                );
                inner = ui_started.elapsed().as_micros();
                admitted = matches!(
                    run,
                    Ok(crate::command_app::CommandRun::Saving { .. })
                        | Ok(crate::command_app::CommandRun::Durable(_))
                );
            });
        })
        .map_err(|error| format!("automation save failed: {error:?}"))?;
        if !admitted {
            return Err(format!("automation save sample {index} was not admitted"));
        }
        let (pump_us, pump_err) = pump_measured(cx, handle, app, PumpKind::Save, "automation save durable", |a| {
            !a.live_has_pending_receipts()
                && a.live_primary.is_none()
                && a.store
                    .automation(automation)
                    .is_some_and(|definition| definition.name == expected)
        })?;
        if let Err(error) = pump_err {
            return Err(error);
        }
        let render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("automation render failed: {error:?}"))?;
        let render_us = render_started.elapsed().as_micros() as u64;
        let elapsed = started.elapsed().as_micros() as u64;
        let (error, storage_error, name) = cx.update(|cx| {
            let a = app.read(cx);
            (
                a.error.clone(),
                a.storage_error.clone(),
                a.store.automation(automation).map(|definition| definition.name.clone()),
            )
        });
        if !error.is_empty() || !storage_error.is_empty() || name.as_deref() != Some(expected.as_str()) {
            return Err(format!(
                "automation save sample {index} failed after ACK: error={error:?} storage={storage_error:?} name={name:?}"
            ));
        }
        if index >= WARMUP {
            ui_sync.push(inner as u64);
            ack_pump.push(pump_us);
            render.push(render_us);
            main_total.push(inner as u64 + pump_us + render_us);
            wall.push(elapsed);
        }
    }
    let disk = read_v4_store_for(cx, app)?;
    let disk_name = disk.automation(automation).map(|definition| definition.name.clone());
    if disk_name.as_deref() != Some(expected_name.as_str()) {
        return Err(format!("automation disk name mismatch: {disk_name:?}"));
    }
    if disk.all_occurrences().len() != occurrences_before || disk.runs().len() != runs_before {
        return Err("automation rename created or removed occurrences/runs".into());
    }
    let _ = (path, root);
    Ok(serde_json::json!({
        "samples": main_total.len(),
        "ui_sync": stats(&ui_sync),
        "ack_pump": stats(&ack_pump),
        "render": stats(&render),
        "main_thread_total": stats(&main_total),
        "durable_wall": stats(&wall),
        "disk_name": disk_name,
        "occurrences_unchanged": occurrences_before,
        "runs_unchanged": runs_before,
    }))
}

fn measure_automation_lifecycle(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    automation: u64,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let mut enable_ui = Vec::new();
    let mut enable_pump = Vec::new();
    let mut enable_render = Vec::new();
    let mut enable_main = Vec::new();
    let mut enable_wall = Vec::new();
    let mut pause_ui = Vec::new();
    let mut pause_pump = Vec::new();
    let mut pause_render = Vec::new();
    let mut pause_main = Vec::new();
    let mut pause_wall = Vec::new();
    open_automation_measure_surface(cx, handle, app, automation)?;
    let occurrences_before = cx.update(|cx| app.read(cx).store.all_occurrences().len());
    for index in 0..(WARMUP + SAMPLES) {
        let input = cx
            .update(|cx| {
                app.read(cx)
                    .store
                    .automation(automation)
                    .map(|definition| definition_input(definition, format!("Lifecycle {index}")))
            })
            .ok_or("automation lifecycle fixture missing")?;
        let started = Instant::now();
        let mut enable_inner = 0u128;
        let mut admitted = false;
        let mut rejection = None;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui_started = Instant::now();
                let run = a.execute_command(
                    crate::domain_command::Command::Enable {
                        id: Some(automation),
                        input: input.clone(),
                        materials: Vec::new(),
                        now: chrono::Utc::now().timestamp(),
                    },
                    cx,
                );
                enable_inner = ui_started.elapsed().as_micros();
                admitted = matches!(
                    run,
                    Ok(crate::command_app::CommandRun::Saving { .. })
                        | Ok(crate::command_app::CommandRun::Durable(_))
                );
                if !admitted {
                    rejection = Some(match run {
                        Err(error) => format!("CommandRun::Err({error})"),
                        Ok(other) => format!("{other:?}"),
                    });
                }
            });
        })
        .map_err(|error| format!("automation enable failed: {error:?}"))?;
        if !admitted {
            let (error, storage_error) = cx.update(|cx| {
                let a = app.read(cx);
                (a.error.clone(), a.storage_error.clone())
            });
            return Err(format!(
                "automation enable sample {index} was not admitted: rejection={rejection:?} app_error={error:?} storage_error={storage_error:?}"
            ));
        }
        let (enable_pump_us, enable_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "enable durable", |a| {
                !a.live_has_pending_receipts()
                    && a.live_primary.is_none()
                    && a.store
                        .automation(automation)
                        .is_some_and(|definition| definition.status == crate::automation::DefinitionStatus::Enabled)
            })?;
        if let Err(error) = enable_err {
            return Err(error);
        }
        let enable_render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("enable render failed: {error:?}"))?;
        let enable_render_us = enable_render_started.elapsed().as_micros() as u64;
        let enable_elapsed = started.elapsed().as_micros() as u64;
        if cx.update(|cx| app.read(cx).store.all_occurrences().len()) != occurrences_before {
            return Err("enabling created an occurrence; fixture is not future-safe".into());
        }

        let pause_started = Instant::now();
        let mut pause_inner = 0u128;
        let mut pause_admitted = false;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui_started = Instant::now();
                let run = a.execute_command(
                    crate::domain_command::Command::AutomationPause {
                        id: automation,
                        now: chrono::Utc::now().timestamp(),
                    },
                    cx,
                );
                pause_inner = ui_started.elapsed().as_micros();
                pause_admitted = matches!(
                    run,
                    Ok(crate::command_app::CommandRun::Saving { .. })
                        | Ok(crate::command_app::CommandRun::Durable(_))
                );
            });
        })
        .map_err(|error| format!("automation pause failed: {error:?}"))?;
        if !pause_admitted {
            return Err(format!("automation pause sample {index} was not admitted"));
        }
        let (pause_pump_us, pause_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "pause durable", |a| {
                !a.live_has_pending_receipts()
                    && a.live_primary.is_none()
                    && a.store
                        .automation(automation)
                        .is_some_and(|definition| definition.status == crate::automation::DefinitionStatus::Paused)
            })?;
        if let Err(error) = pause_err {
            return Err(error);
        }
        let pause_render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("pause render failed: {error:?}"))?;
        let pause_render_us = pause_render_started.elapsed().as_micros() as u64;
        let pause_elapsed = pause_started.elapsed().as_micros() as u64;
        if index >= WARMUP {
            enable_ui.push(enable_inner as u64);
            enable_pump.push(enable_pump_us);
            enable_render.push(enable_render_us);
            enable_main.push(enable_inner as u64 + enable_pump_us + enable_render_us);
            enable_wall.push(enable_elapsed);
            pause_ui.push(pause_inner as u64);
            pause_pump.push(pause_pump_us);
            pause_render.push(pause_render_us);
            pause_main.push(pause_inner as u64 + pause_pump_us + pause_render_us);
            pause_wall.push(pause_elapsed);
        }
    }
    let disk = read_v4_store_for(cx, app)?;
    let disk_status = disk.automation(automation).map(|definition| definition.status);
    if disk_status != Some(crate::automation::DefinitionStatus::Paused) {
        return Err(format!("lifecycle final disk status is not Paused: {disk_status:?}"));
    }
    if disk.all_occurrences().len() != occurrences_before {
        return Err("lifecycle created occurrences; fixture was not future-safe".into());
    }
    let _ = (path, root);
    Ok(serde_json::json!({
        "enable": {
            "samples": enable_main.len(),
            "ui_sync": stats(&enable_ui),
            "ack_pump": stats(&enable_pump),
            "render": stats(&enable_render),
            "main_thread_total": stats(&enable_main),
            "durable_wall": stats(&enable_wall),
        },
        "pause": {
            "samples": pause_main.len(),
            "ui_sync": stats(&pause_ui),
            "ack_pump": stats(&pause_pump),
            "render": stats(&pause_render),
            "main_thread_total": stats(&pause_main),
            "durable_wall": stats(&pause_wall),
        },
        "disk_status": format!("{disk_status:?}"),
        "occurrences_unchanged": occurrences_before,
    }))
}

fn measure_settings(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let mut ui_sync = Vec::new();
    let mut ack_pump = Vec::new();
    let mut render = Vec::new();
    let mut main_total = Vec::new();
    let mut wall = Vec::new();
    open_settings_measure_surface(cx, handle, app)?;
    let mut last_target = 0usize;
    for index in 0..(WARMUP + SAMPLES) {
        let base = cx.update(|cx| app.read(cx).store.max_concurrent_runs());
        // Always change the value, including warmup and the base=1 case.
        let target = if base == 1 { 2 } else { 1 };
        assert_ne!(target, base, "settings sample must change the current value");
        last_target = target;
        let started = Instant::now();
        let mut inner = 0u128;
        let accepted = cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui_started = Instant::now();
                let accepted = a.apply_scoped_settings(
                    crate::scoped_settings::SettingsChange::ExecutionConcurrency(target),
                    cx,
                );
                inner = ui_started.elapsed().as_micros();
                accepted
            })
        })
        .map_err(|error| format!("settings save failed: {error:?}"))?;
        if !accepted {
            return Err(format!("settings sample {index} was rejected"));
        }
        let (pump_us, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "settings durable", |a| {
                !a.live_has_pending_receipts() && a.live_primary.is_none()
            })?;
        if let Err(error) = pump_err {
            return Err(error);
        }
        let render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("settings render failed: {error:?}"))?;
        let render_us = render_started.elapsed().as_micros() as u64;
        let elapsed = started.elapsed().as_micros() as u64;
        let (applied, error, storage_error) = cx.update(|cx| {
            let a = app.read(cx);
            (
                a.store.max_concurrent_runs(),
                a.error.clone(),
                a.storage_error.clone(),
            )
        });
        if applied != target || !error.is_empty() || !storage_error.is_empty() {
            return Err(format!(
                "settings sample {index} did not apply exactly: applied={applied} target={target} error={error:?} storage={storage_error:?}"
            ));
        }
        if index >= WARMUP {
            ui_sync.push(inner as u64);
            ack_pump.push(pump_us);
            render.push(render_us);
            main_total.push(inner as u64 + pump_us + render_us);
            wall.push(elapsed);
        }
    }
    let disk = read_v4_store_for(cx, app)?;
    if disk.max_concurrent_runs() != last_target {
        return Err(format!(
            "settings disk mismatch: expected {last_target}, got {}",
            disk.max_concurrent_runs()
        ));
    }
    let _ = (path, root);
    Ok(serde_json::json!({
        "samples": main_total.len(),
        "ui_sync": stats(&ui_sync),
        "ack_pump": stats(&ack_pump),
        "render": stats(&render),
        "main_thread_total": stats(&main_total),
        "durable_wall": stats(&wall),
        "disk_max_concurrent_runs": last_target,
    }))
}

fn measure_scoped_task_undo(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    task: u64,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let mut ui_sync = Vec::new();
    let mut ack_pump = Vec::new();
    let mut render = Vec::new();
    let mut main_total = Vec::new();
    let mut wall = Vec::new();
    let baseline = cx
        .update(|cx| app.read(cx).store.task(task).map(|task| task.title.clone()))
        .ok_or("undo fixture task missing")?;
    for index in 0..(WARMUP + SAMPLES) {
        let changed = format!("Undo sample {index}");
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.open_task(task, w, cx);
                a.begin_draft(task, w, cx);
                let title = a.draft.as_ref().unwrap().title.clone();
                title.update(cx, |state, cx| state.set_value(&changed, w, cx));
            });
            w.render_frame(cx);
        })
        .map_err(|error| format!("undo edit open failed: {error:?}"))?;
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                assert!(a.request_save_draft(w, cx) || a.live_primary.is_some());
            });
        })
        .map_err(|error| format!("undo edit save failed: {error:?}"))?;
        let (_, saved_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "undo edit durable", |a| {
                !a.live_has_pending_receipts()
                    && a.live_primary.is_none()
                    && a.store.task(task).is_some_and(|task| task.title == changed)
            })?;
        if let Err(error) = saved_err {
            return Err(error);
        }
        let started = Instant::now();
        let mut inner = 0u128;
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                let ui_started = Instant::now();
                a.undo_change(w, cx);
                inner = ui_started.elapsed().as_micros();
            });
        })
        .map_err(|error| format!("scoped undo failed: {error:?}"))?;
        let (pump_us, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "undo durable", |a| {
                !a.live_has_pending_receipts()
                    && a.live_primary.is_none()
                    && a.store.task(task).is_some_and(|task| task.title == baseline)
            })?;
        if let Err(error) = pump_err {
            return Err(error);
        }
        let (restored, error, storage_error) = cx.update(|cx| {
            let a = app.read(cx);
            (
                a.store.task(task).map(|task| task.title.clone()),
                a.error.clone(),
                a.storage_error.clone(),
            )
        });
        if restored.as_deref() != Some(baseline.as_str())
            || !error.is_empty()
            || !storage_error.is_empty()
        {
            return Err(format!(
                "undo sample {index} did not restore exactly: restored={restored:?} baseline={baseline:?} error={error:?} storage={storage_error:?}"
            ));
        }
        let render_started = Instant::now();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .map_err(|error| format!("undo render failed: {error:?}"))?;
        let render_us = render_started.elapsed().as_micros() as u64;
        let elapsed = started.elapsed().as_micros() as u64;
        if index >= WARMUP {
            ui_sync.push(inner as u64);
            ack_pump.push(pump_us);
            render.push(render_us);
            main_total.push(inner as u64 + pump_us + render_us);
            wall.push(elapsed);
        }
    }
    let disk = read_v4_store_for(cx, app)?;
    let disk_title = disk.task(task).map(|task| task.title.clone());
    if disk_title.as_deref() != Some(baseline.as_str()) {
        return Err(format!("scoped undo disk title mismatch: {disk_title:?} != {baseline:?}"));
    }
    let _ = (path, root);
    Ok(serde_json::json!({
        "samples": main_total.len(),
        "ui_sync": stats(&ui_sync),
        "ack_pump": stats(&ack_pump),
        "render": stats(&render),
        "main_thread_total": stats(&main_total),
        "durable_wall": stats(&wall),
        "disk_title": disk_title,
    }))
}

fn measure_append_ack_shared_snapshot(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    task: u64,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let next_id = cx
        .update(|cx| {
            app.read(cx)
                .store
                .runs()
                .iter()
                .map(|run| run.id)
                .max()
                .unwrap_or(0)
                + 1
        });
    let title = cx
        .update(|cx| app.read(cx).store.task(task).map(|task| task.title.clone()))
        .ok_or("append fixture task missing")?;
    let run = AgentRun {
        automation_occurrence_id: None,
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
        id: next_id,
        task_title: title,
        directory: root.to_string_lossy().into_owned(),
        prompt: "append/ack fixture".into(),
        started_at: String::new(),
        queued_at: String::new(),
        workspace_source: None,
        ended_at: None,
        session_id: None,
        exit_code: None,
        task_id: task,
        agent: "test".into(),
        model: "qa/model".into(),
        stage: Stage::Done,
        status: "Queued".into(),
        logs: Vec::new(),
        output: String::new(),
        attachments: Vec::new(),
        research_kind: None,
        sources: Vec::new(),
        input_signature: String::new(),
    };
    // Hold a snapshot clone across both writes to exercise the worst case where
    // retained/last-saved allocations share the run Vec with live state.
    let retained = cx.update(|cx| app.read(cx).store.clone());
    let wall_started = Instant::now();
    let mut append_ui = 0u128;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let ui_started = Instant::now();
            a.store.push_run(run);
            a.persist_if_changed(true);
            append_ui = ui_started.elapsed().as_micros();
            cx.notify();
        });
    })
    .map_err(|error| format!("append failed: {error:?}"))?;
    let (append_pump, append_err) =
        pump_measured(cx, handle, app, PumpKind::Save, "append durable", |a| {
            !a.live_has_pending_receipts()
                && a.live_primary.is_none()
                && a.store.runs().iter().any(|value| value.id == next_id)
        })?;
    if let Err(error) = append_err {
        return Err(error);
    }
    let mut update_ui = 0u128;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let ui_started = Instant::now();
            if let Some(mut value) = a.store.run_mut(next_id) {
                value.status = "Succeeded".into();
                value.output = "append-ack output".into();
            }
            a.persist_if_changed(true);
            update_ui = ui_started.elapsed().as_micros();
            cx.notify();
        });
    })
    .map_err(|error| format!("update failed: {error:?}"))?;
    let (update_pump, update_err) =
        pump_measured(cx, handle, app, PumpKind::Save, "append update durable", |a| {
            !a.live_has_pending_receipts()
                && a.live_primary.is_none()
                && a.store
                    .runs()
                    .iter()
                    .find(|value| value.id == next_id)
                    .is_some_and(|value| {
                        value.status == "Succeeded" && value.output == "append-ack output"
                    })
        })?;
    if let Err(error) = update_err {
        return Err(error);
    }
    let wall_us = wall_started.elapsed().as_micros() as u64;
    drop(retained);
    let disk = read_v4_store_for(cx, app)?;
    let disk_run = disk.runs().iter().find(|value| value.id == next_id);
    if !disk_run.is_some_and(|value| {
        value.status == "Succeeded" && value.output == "append-ack output"
    }) {
        return Err("append/ack run did not reach disk exactly".into());
    }
    let _ = path;
    Ok(serde_json::json!({
        "run_id": next_id,
        "condition": "snapshot clone retained across append and update",
        "append_ui_us": append_ui as u64,
        "append_pump_us": append_pump,
        "update_ui_us": update_ui as u64,
        "update_pump_us": update_pump,
        "wall_us": wall_us,
        "disk_status": "Succeeded",
        "disk_output": "append-ack output",
    }))
}

fn minimal_run(id: u64, task: u64, root: &Path, output: &str) -> AgentRun {
    AgentRun {
        automation_occurrence_id: None,
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
        id,
        task_title: "touched run".into(),
        directory: root.to_string_lossy().into_owned(),
        prompt: "touched".into(),
        started_at: String::new(),
        queued_at: String::new(),
        workspace_source: None,
        ended_at: None,
        session_id: None,
        exit_code: None,
        task_id: task,
        agent: "test".into(),
        model: "qa/model".into(),
        stage: Stage::Done,
        status: "Succeeded".into(),
        logs: Vec::new(),
        output: output.into(),
        attachments: Vec::new(),
        research_kind: None,
        sources: Vec::new(),
        input_signature: String::new(),
    }
}

fn measure_touched_runs(root: &Path, label: &str, count: usize) -> Result<serde_json::Value, String> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut base = Store::mock();
    base.project_mut(1)
        .ok_or("run fixture project missing")?
        .directory = dir.to_string_lossy().into_owned();
    let runs: Vec<AgentRun> = (1..=count as u64)
        .map(|id| minimal_run(id, 1, &dir, ""))
        .collect();
    let store = base
        .with_loaded_tracked(runs, Vec::new(), Vec::new())
        .map_err(|(_, error)| error)?;
    let path = seed_store(&dir, &store)?;
    let (mut cx, handle, app) = open_live(&dir, &path)?;
    let actual = cx.update(|cx| app.read(cx).store.runs().len());
    if actual != count {
        return Err(format!("run fixture expected {count} runs, got {actual}"));
    }
    let mut samples = Vec::new();
    let mut disk_expectations: Vec<(u64, String)> = Vec::new();
    for index in 0..5u64 {
        let runs_len_before = cx.update(|cx| app.read(cx).store.runs().len());
        if runs_len_before != count + index as usize {
            return Err(format!(
                "run sample {index} started with {runs_len_before} runs, expected {}",
                count + index as usize
            ));
        }
        let retained = cx.update(|cx| app.read(cx).store.clone());
        let new_id = count as u64 + index + 1;
        let started = Instant::now();
        let mut append_ui = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui = Instant::now();
                a.store.push_run(minimal_run(new_id, 1, &dir, ""));
                a.persist_if_changed(true);
                append_ui = ui.elapsed().as_micros();
                cx.notify();
            });
        })
        .map_err(|error| format!("run append failed: {error:?}"))?;
        let (append_pump, append_err) =
            pump_measured(&mut cx, handle, &app, PumpKind::Save, "run append durable", |a| {
                !a.live_has_pending_receipts()
                    && a.store.runs().iter().any(|run| run.id == new_id)
            })?;
        if let Err(error) = append_err {
            return Err(error);
        }
        let runs_len_after = cx.update(|cx| app.read(cx).store.runs().len());
        if runs_len_after != runs_len_before + 1 {
            return Err(format!(
                "run sample {index} appended to {runs_len_after} runs, expected {}",
                runs_len_before + 1
            ));
        }
        let mut update_ui = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui = Instant::now();
                if let Some(mut run) = a.store.run_mut(new_id) {
                    run.status = "Succeeded".into();
                    run.output = format!("touched-{index}");
                }
                a.persist_if_changed(true);
                update_ui = ui.elapsed().as_micros();
                cx.notify();
            });
        })
        .map_err(|error| format!("run update failed: {error:?}"))?;
        let (update_pump, update_err) =
            pump_measured(&mut cx, handle, &app, PumpKind::Save, "run update durable", |a| {
                !a.live_has_pending_receipts()
                    && a.store
                        .runs()
                        .iter()
                        .find(|run| run.id == new_id)
                        .is_some_and(|run| run.output == format!("touched-{index}"))
            })?;
        if let Err(error) = update_err {
            return Err(error);
        }
        disk_expectations.push((new_id, format!("touched-{index}")));
        samples.push(serde_json::json!({
            "run_collection_len_before": runs_len_before,
            "run_collection_len_after_append": runs_len_after,
            "append_ui_us": append_ui as u64,
            "append_ack_pump_us": append_pump,
            "update_ui_us": update_ui as u64,
            "update_ack_pump_us": update_pump,
            "wall_us": started.elapsed().as_micros() as u64,
        }));
        drop(retained);
    }
    let disk = read_v4_store_for(&mut cx, &app)?;
    if disk.runs().len() != count + 5 {
        return Err(format!(
            "touched run disk has {} runs, expected {}",
            disk.runs().len(),
            count + 5
        ));
    }
    for (run_id, output) in &disk_expectations {
        let persisted = disk.runs().iter().find(|run| run.id == *run_id);
        if !persisted.is_some_and(|run| run.status == "Succeeded" && &run.output == output) {
            return Err(format!(
                "touched run {run_id} did not reach disk as Succeeded/{output}"
            ));
        }
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(serde_json::json!({
        "actual_runs_before": actual,
        "samples": samples,
        "sample_note": "five bounded one-shot append+update samples with a retained Store clone; not a p95 claim",
        "disk_run_count": disk.runs().len(),
        "condition": "snapshot clone retained across each append/update",
    }))
}

fn occurrence_collection_len(cx: &mut HeadlessAppContext, app: &Entity<TaskApp>) -> usize {
    cx.update(|cx| app.read(cx).store.all_occurrences().len())
}

fn sample_position(index: usize) -> &'static str {
    match index {
        0 => "head",
        1 => "quarter",
        2 => "middle",
        3 => "three_quarter",
        _ => "tail",
    }
}

/// Pick a deterministic occurrence near head, quarter, middle, three-quarter
/// and tail. Identity lookup is linear, so the recorded index makes the
/// position cost explicit instead of always measuring near id 1.
fn choose_occurrence_near(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    used: &[u64],
    require_unread: bool,
    sample: usize,
) -> Option<(usize, u64)> {
    cx.update(|cx| {
        let items = app.read(cx).store.all_occurrences();
        let len = items.len();
        if len == 0 {
            return None;
        }
        let target = (len - 1).saturating_mul(sample.min(4)) / 4;
        // Search outward from the requested position, so the tail stays near
        // the tail instead of wrapping back to the head when the last item is
        // not eligible.
        for offset in 0..len {
            for index in [target.checked_add(offset), target.checked_sub(offset)]
                .into_iter()
                .flatten()
                .filter(|index| *index < len)
            {
                let occurrence = &items[index];
                if !used.contains(&occurrence.id) && (!require_unread || occurrence.unread) {
                    return Some((index, occurrence.id));
                }
            }
        }
        None
    })
}

fn occurrence_unread(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    id: u64,
) -> Option<bool> {
    cx.update(|cx| app.read(cx).store.occurrence(id).map(|occurrence| occurrence.unread))
}

fn measure_direct_mark_read_case(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    count: usize,
    used: &mut Vec<u64>,
    expectations: &mut Vec<(u64, bool)>,
    hold_retained: bool,
    retained_state: &str,
    case: &str,
) -> Result<serde_json::Value, String> {
    let mut samples = Vec::new();
    for index in 0..5u64 {
        let (occurrence_index, id) = choose_occurrence_near(cx, app, used, true, index as usize)
            .ok_or_else(|| format!("{case}: no unread occurrence available for sample {index}"))?;
        used.push(id);
        let occurrence_len = occurrence_collection_len(cx, app);
        if occurrence_len != count {
            return Err(format!(
                "{case} sample {index} started with {occurrence_len} occurrences, expected {count}"
            ));
        }
        let retained = if hold_retained {
            Some(cx.update(|cx| app.read(cx).store.clone()))
        } else {
            None
        };
        let started = Instant::now();
        let mut ui_us = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui = Instant::now();
                a.store.mark_occurrence_read(id);
                a.persist_if_changed(true);
                ui_us = ui.elapsed().as_micros();
                cx.notify();
            });
        })
        .map_err(|error| format!("{case} mark-read failed: {error:?}"))?;
        let (pump, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "direct mark-read durable", |a| {
                !a.live_has_pending_receipts()
                    && a.store
                        .occurrence(id)
                        .is_some_and(|occurrence| !occurrence.unread)
            })?;
        if let Err(error) = pump_err {
            return Err(format!("{case}: {error}"));
        }
        expectations.push((id, false));
        samples.push(serde_json::json!({
            "occurrence_id": id,
            "occurrence_index": occurrence_index,
            "sample_position": sample_position(index as usize),
            "occurrence_collection_len": occurrence_len,
            "retained_state": retained_state,
            "ui_us": ui_us as u64,
            "ack_pump_us": pump,
            "wall_us": started.elapsed().as_micros() as u64,
        }));
        drop(retained);
    }
    Ok(serde_json::json!({
        "case": case,
        "retained_state": retained_state,
        "samples": samples,
        "sample_note": "five bounded one-shot direct mark-read + live snapshot ACK samples; not a p95 claim",
    }))
}

fn measure_toggle_command_case(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    count: usize,
    used: &mut Vec<u64>,
    expectations: &mut Vec<(u64, bool)>,
) -> Result<serde_json::Value, String> {
    let mut samples = Vec::new();
    for index in 0..5u64 {
        let (occurrence_index, id) = choose_occurrence_near(cx, app, used, false, index as usize)
            .ok_or_else(|| format!("toggle command: no occurrence available for sample {index}"))?;
        used.push(id);
        let before = occurrence_unread(cx, app, id)
            .ok_or_else(|| format!("toggle command: occurrence {id} disappeared"))?;
        let expected = !before;
        let occurrence_len = occurrence_collection_len(cx, app);
        if occurrence_len != count {
            return Err(format!(
                "toggle command sample {index} started with {occurrence_len} occurrences, expected {count}"
            ));
        }
        let started = Instant::now();
        let mut ui_us = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui = Instant::now();
                a.toggle_automation_read(id, cx);
                ui_us = ui.elapsed().as_micros();
            });
        })
        .map_err(|error| format!("toggle command failed: {error:?}"))?;
        let actual = occurrence_unread(cx, app, id);
        if actual != Some(expected) {
            return Err(format!(
                "toggle command did not apply occurrence {id}: expected unread={expected}, got {actual:?}"
            ));
        }
        let (pump, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "toggle command durable", |a| {
                !a.live_has_pending_receipts()
                    && a.store
                        .occurrence(id)
                        .is_some_and(|occurrence| occurrence.unread == expected)
            })?;
        if let Err(error) = pump_err {
            return Err(format!("toggle command: {error}"));
        }
        expectations.push((id, expected));
        samples.push(serde_json::json!({
            "occurrence_id": id,
            "occurrence_index": occurrence_index,
            "sample_position": sample_position(index as usize),
            "occurrence_collection_len": occurrence_len,
            "retained_state": "none: live direct SetRead path; no whole Store staging; position-dependent identity lookup only",
            "ui_us": ui_us as u64,
            "ack_pump_us": pump,
            "wall_us": started.elapsed().as_micros() as u64,
        }));
    }
    Ok(serde_json::json!({
        "case": "toggle_command",
        "retained_state": "none: live direct SetRead path; no whole Store staging; position-dependent identity lookup only",
        "samples": samples,
        "sample_note": "five bounded one-shot real toggle_automation_read command + live snapshot ACK samples; not a p95 claim",
    }))
}

fn measure_noop_form_then_direct_case(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    count: usize,
    used: &mut Vec<u64>,
    expectations: &mut Vec<(u64, bool)>,
) -> Result<serde_json::Value, String> {
    let task = cx
        .update(|cx| app.read(cx).store.tasks().iter().map(|task| task.id).max())
        .ok_or("no-op form fixture task missing")?;
    let mut warmup_outcome = String::new();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, window, cx);
            a.begin_draft(task, window, cx);
            warmup_outcome = format!("{:?}", a.request_save_draft_outcome(window, cx));
        });
    })
    .map_err(|error| format!("no-op form warmup failed: {error:?}"))?;
    if warmup_outcome.contains("Saving") {
        let (_pump, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "no-op form warmup durable", |a| {
                !a.live_has_pending_receipts()
            })?;
        if let Err(error) = pump_err {
            return Err(format!("no-op form warmup: {error}"));
        }
    }
    let mut samples = Vec::new();
    let mut outcomes = Vec::new();
    for index in 0..5u64 {
        let mut outcome = String::new();
        let mut save_ui_us = 0u128;
        cx.update_window(handle, |_, window, cx| {
            app.update(cx, |a, cx| {
                let started = Instant::now();
                a.open_task(task, window, cx);
                a.begin_draft(task, window, cx);
                let result = a.request_save_draft_outcome(window, cx);
                outcome = format!("{result:?}");
                save_ui_us = started.elapsed().as_micros();
            });
        })
        .map_err(|error| format!("no-op form save failed: {error:?}"))?;
        if outcome != "Saved" {
            return Err(format!(
                "no-op form sample {index} was not a durable no-op: {outcome}"
            ));
        }
        outcomes.push(outcome.clone());
        let (occurrence_index, id) = choose_occurrence_near(cx, app, used, true, index as usize)
            .ok_or_else(|| format!("no-op form then direct: no unread occurrence for sample {index}"))?;
        used.push(id);
        let occurrence_len = occurrence_collection_len(cx, app);
        if occurrence_len != count {
            return Err(format!(
                "no-op form sample {index} started with {occurrence_len} occurrences, expected {count}"
            ));
        }
        let started = Instant::now();
        let mut ui_us = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let ui = Instant::now();
                a.store.mark_occurrence_read(id);
                a.persist_if_changed(true);
                ui_us = ui.elapsed().as_micros();
                cx.notify();
            });
        })
        .map_err(|error| format!("no-op form then direct mark-read failed: {error:?}"))?;
        let (pump, pump_err) =
            pump_measured(cx, handle, app, PumpKind::Save, "no-op form then direct durable", |a| {
                !a.live_has_pending_receipts()
                    && a.store
                        .occurrence(id)
                        .is_some_and(|occurrence| !occurrence.unread)
            })?;
        if let Err(error) = pump_err {
            return Err(format!("no-op form then direct: {error}"));
        }
        expectations.push((id, false));
        samples.push(serde_json::json!({
            "occurrence_id": id,
            "occurrence_index": occurrence_index,
            "sample_position": sample_position(index as usize),
            "noop_form_save_outcome": outcome,
            "noop_form_save_ui_us": save_ui_us as u64,
            "occurrence_collection_len": occurrence_len,
            "retained_state": "after a real no-op task form save; if live last_saved was re-pinned, this mark-read copies",
            "ui_us": ui_us as u64,
            "ack_pump_us": pump,
            "wall_us": started.elapsed().as_micros() as u64,
        }));
    }
    Ok(serde_json::json!({
        "case": "noop_form_then_direct",
        "retained_state": "after a real no-op task form save; if live last_saved was re-pinned, this mark-read copies",
        "noop_form_warmup_outcome": warmup_outcome,
        "noop_form_save_outcomes": outcomes,
        "samples": samples,
        "sample_note": "five bounded one-shot no-op form save followed by direct mark-read + live snapshot ACK; not a p95 claim",
    }))
}

fn measure_touched_occurrences(root: &Path, label: &str, count: usize) -> Result<serde_json::Value, String> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let (store, _automation) = build_store(
        &dir,
        Spec {
            occurrences: count,
            materialized: 0,
            body_bytes: 0,
        },
    )?;
    let path = seed_store(&dir, &store)?;
    let (mut cx, handle, app) = open_live(&dir, &path)?;
    let actual = cx.update(|cx| app.read(cx).store.all_occurrences().len());
    if actual != count {
        return Err(format!("occurrence fixture expected {count}, got {actual}"));
    }
    let mut used = Vec::new();
    let mut disk_expectations: Vec<(u64, bool)> = Vec::new();
    let noop = measure_noop_form_then_direct_case(
        &mut cx,
        handle,
        &app,
        count,
        &mut used,
        &mut disk_expectations,
    )?;
    let direct = measure_direct_mark_read_case(
        &mut cx,
        handle,
        &app,
        count,
        &mut used,
        &mut disk_expectations,
        false,
        "none: live store is the only owner; identity lookup scans to the recorded occurrence index",
        "direct_mark_read",
    )?;
    let toggle = measure_toggle_command_case(
        &mut cx,
        handle,
        &app,
        count,
        &mut used,
        &mut disk_expectations,
    )?;
    let retained = measure_direct_mark_read_case(
        &mut cx,
        handle,
        &app,
        count,
        &mut used,
        &mut disk_expectations,
        true,
        "one full Store clone retained across the mutation",
        "retained_snapshot_diagnostic",
    )?;
    // Diagnostic only: force the same full baseline that adopt_saved_store can
    // install if a live no-op save adopts its staged clone. This is labelled so
    // the result is never mistaken for the real no-op form path above.
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.last_saved = a.store.clone();
        });
    });
    let forced = measure_direct_mark_read_case(
        &mut cx,
        handle,
        &app,
        count,
        &mut used,
        &mut disk_expectations,
        false,
        "diagnostic: last_saved replaced with a full Store clone before direct mark-read",
        "forced_last_saved_then_direct",
    )?;

    let disk = read_v4_store_for(&mut cx, &app)?;
    if disk.all_occurrences().len() != count {
        return Err(format!(
            "touched occurrence disk has {} occurrences, expected {count}",
            disk.all_occurrences().len()
        ));
    }
    for (id, expected) in &disk_expectations {
        if !disk
            .occurrence(*id)
            .is_some_and(|occurrence| occurrence.unread == *expected)
        {
            return Err(format!(
                "touched occurrence {id} did not reach disk with unread={expected}"
            ));
        }
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(serde_json::json!({
        "actual_occurrences_before": actual,
        "disk_occurrence_count": disk.all_occurrences().len(),
        "cases": {
            "direct_mark_read": direct,
            "noop_form_then_direct": noop,
            "toggle_command": toggle,
            "retained_snapshot_diagnostic": retained,
            "forced_last_saved_then_direct": forced,
        },
        "sample_note": "five bounded one-shot samples per case; not a p95 claim",
    }))
}

fn measure_touched_collections(root: &Path) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "runs_43200": measure_touched_runs(root, "touched-runs-43200", 43_200)?,
        "runs_86400": measure_touched_runs(root, "touched-runs-86400", 86_400)?,
        "occurrences_43200": measure_touched_occurrences(root, "touched-occ-43200", 43_200)?,
        "occurrences_86400": measure_touched_occurrences(root, "touched-occ-86400", 86_400)?,
    }))
}

fn measure_history(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    automation: u64,
    path: &Path,
    root: &Path,
) -> Result<serde_json::Value, String> {
    let mut ui_sync = Vec::new();
    let mut pump = Vec::new();
    let mut render = Vec::new();
    let mut main_total = Vec::new();
    let mut wall = Vec::new();
    let mut rows_seen = 0usize;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_automation(automation, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("automation detail open failed: {error:?}"))?;
    let (scope, selected, creating) = cx.update(|cx| {
        let a = app.read(cx);
        (a.scope(), a.automation_ui.selected, a.automation_ui.creating)
    });
    if scope != crate::app::Scope::Automations
        || selected != Some(automation)
        || creating
    {
        return Err(format!(
            "history measurement is not on the visible automation detail: scope={scope:?} selected={selected:?} creating={creating}"
        ));
    }
    for index in 0..(WARMUP + SAMPLES) {
        let started = Instant::now();
        let mut inner = 0u128;
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.history_cache.clear();
                let ui_started = Instant::now();
                a.request_history_page(automation, cx);
                inner = ui_started.elapsed().as_micros();
            });
        })
        .map_err(|error| format!("history request failed: {error:?}"))?;
        let (pump_us, pump_err) =
            pump_measured(cx, handle, app, PumpKind::History, "history page", |a| {
            a.history_cache.get(&automation).is_some_and(|cache| {
                matches!(cache.state, crate::history_reader::HistoryLoad::Loaded)
                    && cache.rows.len() >= 20
            })
            })?;
        if let Err(error) = pump_err {
            return Err(error);
        }
        let (state, row_ids) = cx.update(|cx| {
            app.read(cx)
                .history_cache
                .get(&automation)
                .map(|cache| {
                    (
                        format!("{:?}", cache.state),
                        cache.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
                    )
                })
                .unwrap_or_default()
        });
        if state != "Loaded" {
            return Err(format!("history sample {index} did not load: {state}"));
        }
        rows_seen = row_ids.len();
        if !row_ids.windows(2).all(|pair| pair[0] > pair[1]) {
            return Err(format!("history sample {index} is not newest-first by id"));
        }
        let first_row = *row_ids.first().ok_or("history page returned no rows")?;
        let render_started = Instant::now();
        let visible = cx
            .update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.find(format!("automation-occurrence-{first_row}")).visible()
                    && w.try_find(format!("occurrence-status-{first_row}")).is_some()
            })
            .map_err(|error| format!("history render failed: {error:?}"))?;
        if !visible {
            return Err(format!(
                "history sample {index} did not render the visible first occurrence row {first_row}"
            ));
        }
        let render_us = render_started.elapsed().as_micros() as u64;
        let elapsed = started.elapsed().as_micros() as u64;
        if index >= WARMUP {
            ui_sync.push(inner as u64);
            pump.push(pump_us);
            render.push(render_us);
            main_total.push(inner as u64 + pump_us + render_us);
            wall.push(elapsed);
        }
    }
    let incarnation = crate::sqlite_v4::read_incarnation(path)?;
    let mut reader = crate::history_reader::HistoryReader::start(path, root, incarnation)?;
    reader
        .request_page(99, automation, None, None, 20)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut metrics = None;
    while Instant::now() < deadline {
        for response in reader.poll(8) {
            if let crate::history_reader::ReaderResponse::Page { result, .. } = response
                && let Ok(page) = result
            {
                metrics = Some(page.metrics);
                break;
            }
        }
        if metrics.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let metrics = metrics.ok_or("history page metrics unavailable")?;
    Ok(serde_json::json!({
        "samples": main_total.len(),
        "ui_sync": stats(&ui_sync),
        "ack_pump": stats(&pump),
        "render": stats(&render),
        "main_thread_total": stats(&main_total),
        "durable_wall": stats(&wall),
        "first_page_rows": rows_seen,
        "order": "newest-first by descending occurrence id",
        "page_metrics_source": "separate read-only HistoryReader validation request on the same database; NOT the 30 UI page queries",
        "page_metrics_note": "body_reads is the backend handle's cumulative body-table read count for this validation request; it is not attributed to the timed UI requests.",
        "page_metrics": {
            "occurrence_rows": metrics.occurrence_rows,
            "run_rows": metrics.run_rows,
            "fullscan_steps": metrics.fullscan_steps,
            "page_vm_steps": metrics.page_vm_steps,
            "total_count_vm_steps": metrics.total_count_vm_steps,
            "unread_count_vm_steps": metrics.unread_count_vm_steps,
            "body_reads": metrics.body_reads,
        },
    }))
}

fn stats(values: &[u64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let pick = |q: f64| -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let index = ((sorted.len() as f64 - 1.0) * q).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    };
    serde_json::json!({
        "count": sorted.len(),
        "p50_us": pick(0.50),
        "p95_us": pick(0.95),
        "p99_us": pick(0.99),
        "max_us": sorted.last().copied().unwrap_or(0),
    })
}

fn profile() -> serde_json::Value {
    serde_json::json!({
        "qa_feature": cfg!(feature = "qa"),
        "debug_assertions": cfg!(debug_assertions),
        "executable": std::env::current_exe().map(|path| path.display().to_string()).unwrap_or_default(),
        "argv": std::env::args().collect::<Vec<_>>(),
        "source_sha256": source_sha(),
    })
}

fn source_sha() -> String {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/live_performance_qa.rs"
    ));
    format!("{:x}", Sha256::digest(bytes))
}

fn hardware() -> serde_json::Value {
    serde_json::json!({
        "cpu": sysctl("machdep.cpu.brand_string"),
        "memory_bytes": sysctl("hw.memsize"),
        "os_build": sysctl("kern.osversion"),
        "arch": std::env::consts::ARCH,
    })
}

fn sysctl(name: &str) -> String {
    std::process::Command::new("/usr/sbin/sysctl")
        .arg("-n")
        .arg(name)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn peak_rss_bytes() -> u64 {
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
    }
    0
}
