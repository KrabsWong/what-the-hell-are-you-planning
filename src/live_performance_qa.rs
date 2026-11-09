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
        let small_result = run_fixture(&root.join("small"), small)?;
        let large_result = run_fixture(&root.join("large-43200"), large)?;
        let report = serde_json::json!({
            "scope": "Actual live TaskApp via Some(LiveStartup) + ordered worker; editor Save command and indexed first-page path.",
            "hardware": hardware(),
            "profile": profile(),
            "samples": SAMPLES,
            "warmup": WARMUP,
            "target_p95_ui_us": TARGET_P95_UI_US,
            "body_read_instrumentation": null,
            "body_read_note": "No live run-body byte counter exists; do not invent one.",
            "fixtures": [small_result, large_result],
            "peak_rss_bytes": peak_rss_bytes(),
        });
        let path = root.join("report.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        let large_p95 = report["fixtures"][1]["editor"]["main_thread_total"]["p95_us"]
            .as_u64()
            .unwrap_or(u64::MAX);
        if large_p95 >= TARGET_P95_UI_US {
            return Err(format!(
                "live editor save p95 UI {large_p95}us exceeds {TARGET_P95_UI_US}us target"
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
    let history = measure_history(&mut cx, handle, &app, automation, &path, root)?;
    let peak = peak_rss_bytes();
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(serde_json::json!({
        "occurrences": spec.occurrences,
        "materialized_runs": spec.materialized,
        "body_bytes": spec.body_bytes,
        "db_bytes": db_bytes,
        "editor": editor,
        "history_page": history,
        "peak_rss_bytes": peak,
    }))
}

fn build_store(root: &Path, spec: Spec) -> Result<(Store, u64), String> {
    let template = crate::automation_execution::build_template("Live perf", "Short report")
        .map_err(|error| error.to_string())?;
    let mut temp = Store::empty();
    temp.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
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
