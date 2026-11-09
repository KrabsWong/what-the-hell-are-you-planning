//! Isolated live-persistence QA.
//!
//! Every fixture lives under `.local/live-persistence-qa/<pid>/` and is opened
//! through a real v4 workspace, the ordered worker and the real `TaskApp` live
//! path. The suite never falls back to legacy `attach_storage`: a fixture that
//! is not `live_required` and `live_persistence_active` fails immediately.

use crate::app::TaskApp;
use crate::domain::{AgentRun, Stage, Store};
use crate::persistence::Storage;
use crate::persistence_live;
use gpui_kit::{test::TestWindowExt, *};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INCARNATION: u64 = 0x51_7E_2A_11;

type Fixture = (HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>);

fn run_root() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".local/live-persistence-qa")
        .join(std::process::id().to_string())
}

pub fn run() {
    let root = run_root();
    let _ = std::fs::remove_dir_all(&root);
    if let Err(error) = std::fs::create_dir_all(&root) {
        eprintln!("LIVE_PERSISTENCE_QA status=failed error={error}");
        std::process::exit(1);
    }
    match run_all(&root) {
        Ok(()) => println!(
            "LIVE_PERSISTENCE_QA status=ok root={}",
            root.display()
        ),
        Err(error) => {
            eprintln!("LIVE_PERSISTENCE_QA status=failed error={error}");
            std::process::exit(1);
        }
    }
}

fn run_all(root: &Path) -> Result<(), String> {
    core_save_and_ack(&root.join("core"))?;
    reopen_after_quit(&root.join("core"))?;
    pending_quit_drain(&root.join("quit"))?;
    startup_self_lock_retry(&root.join("startup-retry"))?;
    let (small_ui, small_total) = measure_editor_save(root, "small", 0)?;
    let (large_ui, large_total) = measure_editor_save(root, "large-43200-empty-runs", 43_200)?;
    let (settings_ui, settings_total) = measure_settings_save(root, "settings-small")?;
    measure_history(root, "history-small", 64)?;
    let history_large = measure_history(root, "history-43200", 43_200)?;
    println!(
        "LIVE_PERF editor_save_small_ui_us={small_ui} editor_save_small_total_us={small_total} editor_save_43200_empty_runs_ui_us={large_ui} editor_save_total_us={large_total} settings_save_ui_us={settings_ui} settings_save_total_us={settings_total} history_page_43200_us={history_large}"
    );
    if large_ui >= 50_000 {
        return Err(format!(
            "reproducible UI blocking: editor save submit took {large_ui}us (>=50ms)"
        ));
    }
    Ok(())
}

fn seed_store(root: &Path, store: &Store) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let path = root.join("workspace.sqlite3");
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    crate::sqlite_v4::SqliteV4::create_seeded(&path, root, INCARNATION, store)
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn read_v4_store(root: &Path, path: &Path) -> Result<Store, String> {
    let incarnation = crate::sqlite_v4::read_incarnation(path)?;
    crate::sqlite_v4::SqliteV4::open(path, root, incarnation)?.load_store()
}

fn open_live(root: &Path, path: &Path) -> Result<Fixture, String> {
    let storage = Storage::new(path.to_path_buf());
    let _ = storage.upgrade_paths(|_| {});
    let lock = storage.acquire_lock()?;
    let mut live = persistence_live::select_existing(&storage)?
        .ok_or("v4 startup did not select the seeded workspace")?;
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
                a.attach_loaded_storage_live(
                    storage,
                    Ok((lock, store)),
                    Some(live.clone()),
                    w,
                    cx,
                );
                assert!(
                    a.live_required,
                    "live QA fixture must require the incremental backend"
                );
                assert!(
                    a.live_persistence_active(),
                    "live QA fixture must attach the ordered worker"
                );
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .map_err(|error| format!("open window failed: {error:?}"))?;
    let app = app.ok_or("live app was not created")?;
    let mut cx = cx;
    cx.update_window(window.into(), |_, w, cx| app.update(cx, |a, cx| a.poll_runtime(cx)))
        .map_err(|error| format!("initial pump failed: {error:?}"))?;
    Ok((cx, window.into(), app))
}

fn pump_until(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    label: &str,
    predicate: impl Fn(&TaskApp) -> bool,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                predicate(app.read(cx))
            })
            .map_err(|error| format!("pump failed: {error:?}"))?;
        if done {
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(60) {
            return Err(format!("timed out waiting for {label}"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}


fn open_via_startup(root: &Path, path: &Path) -> Result<Fixture, String> {
    let storage = Storage::new(path.to_path_buf());
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
            view.update(cx, |a, cx| a.begin_workspace_open(storage, cx));
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .map_err(|error| format!("startup window failed: {error:?}"))?;
    let app = app.ok_or("startup app missing")?;
    let handle: AnyWindowHandle = window.into();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let active = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_workspace_open(w, cx);
                    a.poll_runtime(cx);
                });
                app.read(cx).live_required && app.read(cx).live_persistence_active()
            })
            .map_err(|error| format!("startup poll failed: {error:?}"))?;
        if active {
            return Ok((cx, handle, app));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!(
        "startup open timed out at {}: {}",
        root.display(),
        cx.update(|cx| app.read(cx).storage_error.clone())
    ))
}

/// The same PID already owns the lock when the preparation retry runs. The
/// retry must reuse that lock, not dead-lock against its own lock file.
fn startup_self_lock_retry(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_via_startup(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.workspace_required = true;
            a.storage_error = "Synthetic preparation failure".into();
        })
    });
    let storages = Storage::new(path.clone());
    cx.update(|cx| app.update(cx, |a, cx| a.begin_workspace_open(storages, cx)));
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_workspace_open(w, cx);
                    a.poll_runtime(cx);
                });
                let a = app.read(cx);
                a.live_required && a.live_persistence_active() && a.storage_error.is_empty()
            })
            .map_err(|error| format!("retry poll failed: {error:?}"))?;
        if done {
            let _ = cx.update_window(handle, |_, w, _| w.remove_window());
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!(
        "workspace retry did not attach: {}",
        cx.update(|cx| app.read(cx).storage_error.clone())
    ))
}

fn core_save_and_ack(root: &Path) -> Result<(), String> {
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = store
        .create_task("Live task", 1, Stage::Planned, None)
        .map_err(|error| error.to_string())?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;

    // A pending primary must not claim saved state or push Undo before its
    // durable receipt.
    let undo_before = cx.update(|cx| app.read(cx).undo.len());
    let outcome = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Durable edit".into(),
                    },
                    "Task renamed",
                    cx,
                )
            })
        })
        .map_err(|error| format!("pending save failed: {error:?}"))?;
    if outcome.is_none() {
        return Err("live primary save was rejected unexpectedly".into());
    }
    let (toast, undo_now, primary) = cx.update(|cx| {
        let app = app.read(cx);
        (app.toast.clone(), app.undo.len(), app.live_primary.is_some())
    });
    if primary {
        assert_eq!(undo_now, undo_before, "pending save must not push Undo yet");
        if !toast.contains("Saving") {
            return Err(format!("pending save toast was not truthful: {toast:?}"));
        }
    }
    pump_until(&mut cx, handle, &app, "first durable receipt", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (title, toast, undo_after) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.store.task(task).map(|task| task.title.clone()),
            app.toast.clone(),
            app.undo.len(),
        )
    });
    if title.as_deref() != Some("Durable edit") {
        return Err(format!("durable edit not visible in live store: {title:?}"));
    }
    if undo_after != undo_before + 1 {
        return Err(format!(
            "durable edit must push exactly one Undo: before={undo_before} after={undo_after}"
        ));
    }
    if !toast.contains("Task renamed") {
        return Err(format!("durable toast not truthful: {toast:?}"));
    }

    // A second primary form save is rejected while the first is pending, with
    // no second mutation and no duplicate continuation.
    let (base, first_limit, second_limit) = cx.update(|cx| {
        let base = app.read(cx).store.max_concurrent_runs();
        let first = if base < 10 { base + 1 } else { base - 1 };
        let second = if first < 10 { first + 1 } else { first - 1 };
        (base, first, second)
    });
    let first = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_scoped_settings(
                    crate::scoped_settings::SettingsChange::ExecutionConcurrency(first_limit),
                    cx,
                )
            })
        })
        .map_err(|error| format!("settings save failed: {error:?}"))?;
    if !first {
        return Err("first live settings save was rejected".into());
    }
    if !cx.update(|cx| app.read(cx).live_primary.is_some()) {
        return Err("first live settings save did not hold a primary barrier".into());
    }
    let second = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_scoped_settings(
                    crate::scoped_settings::SettingsChange::ExecutionConcurrency(second_limit),
                    cx,
                )
            })
        })
        .map_err(|error| format!("settings overlap probe failed: {error:?}"))?;
    let during = cx.update(|cx| app.read(cx).store.max_concurrent_runs());
    if second {
        return Err("second primary settings save was accepted".into());
    }
    if during != first_limit {
        return Err(format!(
            "rejected secondary save mutated the limit: expected {first_limit}, got {during}"
        ));
    }
    pump_until(&mut cx, handle, &app, "settings durable receipt", |a| {
        !a.live_has_pending_receipts()
    })?;
    let after = cx.update(|cx| app.read(cx).store.max_concurrent_runs());
    if after != first_limit {
        return Err(format!(
            "durable settings receipt lost the accepted value: expected {first_limit}, got {after}"
        ));
    }
    let _ = base;

    // A newer draft value survives a delayed task-edit ACK.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let input = a.draft.as_ref().unwrap().title.clone();
            input.update(cx, |state, cx| state.set_value("Saved draft", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("open draft failed: {error:?}"))?;
    let saved = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.request_save_draft(w, cx))
        })
        .map_err(|error| format!("draft save failed: {error:?}"))?;
    if saved {
        return Err("live draft save unexpectedly reported durable immediately".into());
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let input = a.draft.as_ref().unwrap().title.clone();
            input.update(cx, |state, cx| state.set_value("Newer draft", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("newer draft failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "delayed task-edit ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (draft_open, newer, disk) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.draft.is_some(),
            app.draft
                .as_ref()
                .map(|draft| draft.title.read(cx).value().to_string()),
            read_v4_store(root, &path).ok(),
        )
    });
    if !draft_open || newer.as_deref() != Some("Newer draft") {
        return Err(format!(
            "delayed ACK clobbered the newer draft: open={draft_open} value={newer:?}"
        ));
    }
    let disk_title = disk.and_then(|store| store.task(task).map(|task| task.title.clone()));
    if disk_title.as_deref() != Some("Saved draft") {
        let (pending, primary, error) = cx.update(|cx| {
            let app = app.read(cx);
            (
                app.pending_task_edit.is_some(),
                app.live_primary.is_some(),
                app.error.clone(),
            )
        });
        return Err(format!(
            "delayed task-edit ACK did not persist the saved value: disk={disk_title:?} draft={newer:?} pending_edit={pending} primary={primary} error={error:?}"
        ));
    }

    if !cx.update(|cx| app.read(cx).live_clean_for_quit()) {
        return Err("live workspace was not clean after durable receipts".into());
    }
    Ok(())
}

fn reopen_after_quit(root: &Path) -> Result<(), String> {
    let path = root.join("workspace.sqlite3");
    let (mut cx, handle, app) = open_live(root, &path)?;
    let title = cx.update(|cx| {
        app.read(cx)
            .store
            .tasks()
            .iter()
            .find(|task| task.title == "Saved draft" || task.title == "Pending A")
            .map(|task| task.title.clone())
    });
    if title.is_none() {
        return Err("reopened live workspace lost the durable task title".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn updated_task(store: &mut Store) -> u64 {
    store
        .create_task("Perf task", 1, Stage::Planned, None)
        .expect("perf task")
}

fn empty_history_runs(task: u64, history: usize) -> Vec<AgentRun> {
    (0..history)
        .map(|index| AgentRun {
            id: index as u64 + 1,
            task_id: task,
            task_title: "History".into(),
            directory: String::new(),
            prompt: String::new(),
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
            output: String::new(),
            attachments: Vec::new(),
            research_kind: None,
            sources: Vec::new(),
            input_signature: String::new(),
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
        })
        .collect()
}

fn measure_editor_save(
    root: &Path,
    label: &str,
    history: usize,
) -> Result<(u128, u128), String> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = dir.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    if history > 0 {
        store = store
            .with_loaded_tracked(empty_history_runs(task, history), Vec::new(), Vec::new())
            .map_err(|(_, error)| error)?;
    }
    let path = seed_store(&dir, &store)?;
    let (mut cx, handle, app) = open_live(&dir, &path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let input = a.draft.as_ref().unwrap().title.clone();
            input.update(cx, |state, cx| state.set_value("Perf edit", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("perf editor open failed: {error:?}"))?;
    let started = Instant::now();
    let saved = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.request_save_draft(w, cx))
        })
        .map_err(|error| format!("perf editor save failed: {error:?}"))?;
    let ui = started.elapsed().as_micros();
    if !saved {
        pump_until(&mut cx, handle, &app, "perf editor durable", |a| {
            !a.live_has_pending_receipts()
        })?;
    }
    let total = started.elapsed().as_micros();
    let disk = read_v4_store(&dir, &path)?;
    if disk
        .task(task)
        .map(|task| task.title.as_str())
        != Some("Perf edit")
    {
        return Err(format!("{label}: editor save was not durable"));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok((ui, total))
}

fn measure_settings_save(root: &Path, label: &str) -> Result<(u128, u128), String> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = dir.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let base = store.max_concurrent_runs();
    let target = if base < 10 { base + 1 } else { base - 1 };
    let path = seed_store(&dir, &store)?;
    let (mut cx, handle, app) = open_live(&dir, &path)?;
    let started = Instant::now();
    let accepted = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_scoped_settings(
                    crate::scoped_settings::SettingsChange::ExecutionConcurrency(target),
                    cx,
                )
            })
        })
        .map_err(|error| format!("settings save failed: {error:?}"))?;
    let ui = started.elapsed().as_micros();
    if accepted {
        pump_until(&mut cx, handle, &app, "settings durable", |a| {
            !a.live_has_pending_receipts()
        })?;
    }
    let total = started.elapsed().as_micros();
    let disk = read_v4_store(&dir, &path)?;
    if disk.max_concurrent_runs() != target {
        return Err(format!("{label}: settings save was not durable"));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok((ui, total))
}

/// A pending accepted write must drain before the window closes. This proves
/// pending shutdown, not ordinary idle quit.
fn pending_quit_drain(root: &Path) -> Result<(), String> {
    let dir = root.to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = dir.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(&dir, &store)?;
    let (mut cx, handle, app) = open_live(&dir, &path)?;
    let outcome = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Quit drain".into(),
                    },
                    "Task renamed",
                    cx,
                )
            })
        })
        .map_err(|error| format!("quit drain save failed: {error:?}"))?;
    if outcome.is_none() {
        return Err("quit drain save was rejected".into());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    cx.update(|cx| app.update(cx, |a, cx| a.begin_graceful_quit(tx, cx)));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut clean = false;
    while Instant::now() < deadline {
        if matches!(rx.try_recv(), Ok(value) if value) {
            clean = true;
            break;
        }
        let _ = cx.update(|cx| app.update(cx, |a, cx| {
            a.poll_runtime(cx);
        }));
        std::thread::sleep(Duration::from_millis(5));
    }
    if !clean {
        return Err("pending shutdown did not drain the accepted write".into());
    }
    let disk = read_v4_store(&dir, &path)?;
    if disk.task(task).map(|task| task.title.as_str()) != Some("Quit drain") {
        return Err("pending shutdown lost the accepted write".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn measure_history(root: &Path, label: &str, count: usize) -> Result<u128, String> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template(
        "Check history",
        "One answer",
    )
    .map_err(|error| error.to_string())?;
    let mut temp = Store::empty();
    let automation = temp
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "History perf".into(),
            "Check history".into(),
            "One answer".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template.clone()),
            temp_automation_now(),
        )
        .map_err(|error| error.to_string())?;
    let definition = temp
        .automation(automation)
        .ok_or("history automation missing")?
        .clone();
    let occurrences: Vec<_> = (0..count)
        .map(|index| crate::automation::AutomationOccurrence {
            id: index as u64 + 1,
            automation_id: automation,
            definition_version: 1,
            source: crate::automation::TriggerSource::Scheduled,
            trigger_key: format!("history-{index}"),
            scheduled_for: Some(index as i64),
            requested_at: None,
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: index as i64,
            data_window: crate::automation::TimeWindow::unresolved("fixture"),
            snapshot: crate::automation::AutomationOccurrenceSnapshot {
                objective: "Check history".into(),
                expected_result: "One answer".into(),
                shape: Some(crate::automation::AutomationShape::Research),
                template: template.clone(),
                models: std::array::from_fn(|_| "test/model".to_string()),
                agents: Default::default(),
                materials: Vec::new(),
                policy: definition.policy.clone(),
                approval_version: 1,
                approval_signature: "fixture".into(),
            },
            status: crate::automation::OccurrenceStatus::Succeeded,
            reason: None,
            research_goal_id: None,
            research_goal_ids: Vec::new(),
            research_topic_id: None,
            run_ids: Vec::new(),
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: index as i64,
            deadline_at: None,
            retry_requested_at: None,
        })
        .collect();
    let store = Store::empty()
        .with_loaded_tracked(Vec::new(), vec![definition], occurrences)
        .map_err(|(_, error)| error)?;
    let path = seed_store(&dir, &store)?;
    let root_path = store
        .projects()
        .first()
        .map(|project| PathBuf::from(&project.directory))
        .unwrap_or_else(|| dir.clone());
    let incarnation = crate::sqlite_v4::read_incarnation(&path)
        .map_err(|error| error.to_string())?;
    let mut reader = crate::history_reader::HistoryReader::start(
        &path,
        &root_path,
        incarnation,
    )?;
    let started = Instant::now();
    reader
        .request_page(1, automation, None, None, 20)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if reader
            .poll(8)
            .iter()
            .any(|response| matches!(response, crate::history_reader::ReaderResponse::Page { request_id: 1, .. }))
        {
            return Ok(started.elapsed().as_micros());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Err(format!("history page {label} timed out"))
}

fn temp_automation_now() -> i64 {
    chrono::Utc::now().timestamp()
}
