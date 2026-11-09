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

const CASE_ENV: &str = "LIVE_PERSISTENCE_QA_CASE";
const ROOT_ENV: &str = "LIVE_PERSISTENCE_QA_ROOT";
const SEQUENTIAL_ENV: &str = "LIVE_PERSISTENCE_QA_SEQUENTIAL";
const CASES: [&str; 23] = [
    "core",
    "reopen",
    "pending-quit",
    "startup-retry",
    "write-failure-retry",
    "reopened-identical-form",
    "dirty-exit-task",
    "dirty-exit-task-reopen-identical",
    "dirty-exit-automation",
    "dirty-exit-automation-reopen-identical",
    "dirty-exit-planning",
    "dirty-exit-planning-cancel",
    "automation-save-step1",
    "automation-save-finish-step3",
    "automation-save-newer-form",
    "automation-rejected-unrelated-command",
    "settings-save",
    "editor-small",
    "editor-43200",
    "history-43200",
    "lazy-period-hydration",
    "lazy-body-missing-retry",
    "lazy-latest-failed",
];

fn run_case(case: &str) -> Result<(), String> {
    let root = std::env::var_os(ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(run_root);
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    match case {
        "core" => core_save_and_ack(&root.join("core")),
        "reopen" => reopen_after_quit(&root.join("core")),
        "pending-quit" => pending_quit_drain(&root.join("quit")),
        "startup-retry" => startup_self_lock_retry(&root.join("startup-retry")),
        "write-failure-retry" => write_failure_retry(&root.join("write-failure-retry")),
        "reopened-identical-form" => {
            reopened_identical_form(&root.join("reopened-identical-form"))
        }
        "dirty-exit-task" => dirty_exit_task_editor(&root.join("dirty-exit-task")),
        "dirty-exit-task-reopen-identical" => {
            dirty_exit_task_reopen_identical(&root.join("dirty-exit-task-reopen"))
        }
        "dirty-exit-automation" => dirty_exit_automation(&root.join("dirty-exit-automation")),
        "dirty-exit-automation-reopen-identical" => dirty_exit_automation_reopen_identical(
            &root.join("dirty-exit-automation-reopen"),
        ),
        "dirty-exit-planning" => dirty_exit_planning(&root.join("dirty-exit-planning"), false),
        "dirty-exit-planning-cancel" => {
            dirty_exit_planning(&root.join("dirty-exit-planning-cancel"), true)
        }
        "automation-save-step1" => automation_save_case(&root.join("automation-save-step1"), false, false),
        "automation-save-finish-step3" => {
            automation_save_case(&root.join("automation-save-step3"), true, false)
        }
        "automation-save-newer-form" => {
            automation_save_case(&root.join("automation-save-newer"), false, true)
        }
        "automation-rejected-unrelated-command" => automation_rejected_unrelated(
            &root.join("automation-rejected-unrelated"),
        ),
        "settings-save" => {
            let (ui, total) = measure_settings_save(&root, "settings-small")?;
            println!("LIVE_PERF settings_save_ui_us={ui} settings_save_total_us={total}");
            Ok(())
        }
        "editor-small" => {
            let (ui, total) = measure_editor_save(&root, "small", 0)?;
            println!("LIVE_PERF editor_save_small_ui_us={ui} editor_save_small_total_us={total}");
            Ok(())
        }
        "editor-43200" => {
            let (ui, total) = measure_editor_save(&root, "large-43200-empty-runs", 43_200)?;
            println!(
                "LIVE_PERF editor_save_43200_empty_runs_ui_us={ui} editor_save_total_us={total}"
            );
            if ui >= 50_000 {
                return Err(format!(
                    "editor save UI blocking {ui}us at 43,200 empty-run history (>=50ms)"
                ));
            }
            Ok(())
        }
        "lazy-period-hydration" => lazy_period_hydrates_on_open(&root.join("lazy-period")),
        "lazy-body-missing-retry" => lazy_body_missing_and_retry(&root.join("lazy-missing")),
        "lazy-latest-failed" => lazy_latest_attempt_invalidates(&root.join("lazy-failed")),
        "history-43200" => {
            let elapsed = measure_history(&root, "history-43200", 43_200)?;
            println!("LIVE_PERF history_page_43200_us={elapsed}");
            Ok(())
        }
        other => Err(format!("unknown live persistence QA case {other}")),
    }
}

/// Default: one child process per case so QA lifecycle teardown cannot overlap
/// the next fixture's worker/reader start-up. This is QA isolation only; the
/// r108 production hang is a T19 SQLite/lifecycle defect and is NOT proved
/// fixed by process isolation. Set LIVE_PERSISTENCE_QA_SEQUENTIAL=1 to run the
/// same cases sequentially in one process (external bounded timeout required)
/// to reproduce that lifecycle after the dependency upgrade.
pub fn run() {
    if let Ok(case) = std::env::var(CASE_ENV) {
        match run_case(&case) {
            Ok(()) => println!("LIVE_PERSISTENCE_QA case={case} status=ok"),
            Err(error) => {
                eprintln!("LIVE_PERSISTENCE_QA case={case} status=failed error={error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let root = run_root();
    let _ = std::fs::remove_dir_all(&root);
    if let Err(error) = std::fs::create_dir_all(&root) {
        eprintln!("LIVE_PERSISTENCE_QA status=failed root error={error}");
        std::process::exit(1);
    }
    if std::env::var_os(SEQUENTIAL_ENV).is_some() {
        let mut failures = Vec::new();
        for case in CASES {
            if let Err(error) = run_case(case) {
                eprintln!("LIVE_SCENARIO_FAILED {case}: {error}");
                failures.push(format!("{case}: {error}"));
            }
        }
        if failures.is_empty() {
            println!("LIVE_PERSISTENCE_QA status=ok sequential=1 root={}", root.display());
        } else {
            eprintln!(
                "LIVE_PERSISTENCE_QA status=failed sequential=1 error={}",
                failures.join(" | ")
            );
            std::process::exit(1);
        }
        return;
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("LIVE_PERSISTENCE_QA status=failed current_exe error={error}");
            std::process::exit(1);
        }
    };
    let mut failures = Vec::new();
    for case in CASES {
        let status = std::process::Command::new(&executable)
            .arg("--qa-live-persistence")
            .env(CASE_ENV, case)
            .env(ROOT_ENV, &root)
            .status();
        match status {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("LIVE_SCENARIO_FAILED {case}: exit {status}");
                failures.push(format!("{case}: exit {status}"));
            }
            Err(error) => failures.push(format!("{case}: spawn {error}")),
        }
    }
    if failures.is_empty() {
        println!("LIVE_PERSISTENCE_QA status=ok root={}", root.display());
    } else {
        eprintln!(
            "LIVE_PERSISTENCE_QA status=failed error={}",
            failures.join(" | ")
        );
        std::process::exit(1);
    }
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


fn advance_until(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    label: &str,
    predicate: impl Fn(&TaskApp) -> bool,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if cx.update(|cx| predicate(app.read(cx))) {
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(30) {
            return Err(format!("timed out advancing async pump for {label}"));
        }
        cx.advance_clock(Duration::from_millis(100));
    }
}

fn startup_gate_clear(app: &TaskApp) -> bool {
    app.startup.workspace_pending.is_none()
        && !app.workspace_required
        && app.storage_error.is_empty()
        && app.live_required
        && app.live_persistence_active()
        && !app.live_has_pending_receipts()
        && app.live_clean_for_quit()
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
                startup_gate_clear(app.read(cx))
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
    if !cx.update(|cx| startup_gate_clear(app.read(cx))) {
        return Err("initial startup gate did not clear".into());
    }
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
                startup_gate_clear(app.read(cx))
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
    // Editor and comment saves must be rejected before they mutate owned fields
    // while a primary receipt is still in flight.
    let (title_before, approval_before, comments_before) = cx.update(|cx| {
        let task = app.read(cx).store.task(task).unwrap();
        (
            task.title.clone(),
            task.approved_plan.clone(),
            task.comments.len(),
        )
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let input = a.draft.as_ref().unwrap().title.clone();
            input.update(cx, |state, cx| state.set_value("Overlap editor", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("overlap editor open failed: {error:?}"))?;
    let editor_accepted = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.request_save_draft(w, cx))
        })
        .map_err(|error| format!("overlap editor save failed: {error:?}"))?;
    let comment_accepted = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.request_task_comment(task, "Overlap comment", w, cx))
        })
        .map_err(|error| format!("overlap comment failed: {error:?}"))?;
    let (title_after, approval_after, comments_after) = cx.update(|cx| {
        let task = app.read(cx).store.task(task).unwrap();
        (
            task.title.clone(),
            task.approved_plan.clone(),
            task.comments.len(),
        )
    });
    if editor_accepted || comment_accepted {
        return Err("editor/comment save was accepted during a pending primary".into());
    }
    if title_after != title_before
        || approval_after != approval_before
        || comments_after != comments_before
    {
        return Err(format!(
            "rejected editor/comment overlap mutated owned state: title {title_before:?}->{title_after:?} approval_changed={} comments {comments_before}->{comments_after}",
            approval_after != approval_before
        ));
    }
    cx.update(|cx| app.update(cx, |a, _| a.draft = None));

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

fn write_failure_retry(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let undo_before = cx.update(|cx| app.read(cx).undo.len());
    let fault_set = cx.update(|cx| {
        app.update(cx, |a, _| {
            a.dispatch_admission
                .as_mut()
                .ok_or("missing dispatch admission")?
                .set_fault(crate::sqlite_v4::Fault::BeforeCommit)
        })
    });
    if let Err(error) = fault_set {
        return Err(format!("set write fault failed: {error:?}"));
    }
    let outcome = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Retry value".into(),
                    },
                    "Task renamed",
                    cx,
                )
            })
        })
        .map_err(|error| format!("faulted save failed: {error:?}"))?;
    if outcome.is_none() {
        return Err("faulted save was rejected before submitting".into());
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut failed = false;
    while Instant::now() < deadline {
        failed = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).live_failure.is_some()
            })
            .map_err(|error| format!("failure poll failed: {error:?}"))?;
        if failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !failed {
        return Err("injected write failure was not reported".into());
    }
    let (undo_failed, disk_failed, error_failed) = cx.update(|cx| {
        (
            app.read(cx).undo.len(),
            read_v4_store(root, &path).ok(),
            app.read(cx).storage_error.clone(),
        )
    });
    if undo_failed != undo_before {
        return Err("failed write pushed Undo before a durable receipt".into());
    }
    if disk_failed
        .as_ref()
        .and_then(|store| store.task(task).map(|task| task.title.as_str()))
        == Some("Retry value")
    {
        return Err("injected failure still produced a durable write".into());
    }
    if error_failed.is_empty() {
        return Err("failed write did not surface a retryable error".into());
    }
    let cleared = cx.update(|cx| {
        app.update(cx, |a, cx| {
            if let Some(adapter) = a.dispatch_admission.as_mut()
                && let Err(error) = adapter.set_fault(crate::sqlite_v4::Fault::None)
            {
                return Err(error);
            }
            a.retry_live_persistence(cx);
            Ok(())
        })
    });
    if let Err(error) = cleared {
        return Err(format!("clear write fault failed: {error:?}"));
    }
    pump_until(&mut cx, handle, &app, "retry durable receipt", |a| {
        !a.live_has_pending_receipts() && a.live_failure.is_none()
    })?;
    let (undo_after, disk_after) = cx.update(|cx| {
        (
            app.read(cx).undo.len(),
            read_v4_store(root, &path).ok(),
        )
    });
    if undo_after != undo_before + 1 {
        return Err(format!(
            "retry did not apply exactly one Undo: before={undo_before} after={undo_after}"
        ));
    }
    if disk_after
        .and_then(|store| store.task(task).map(|task| task.title.clone()))
        .as_deref()
        != Some("Retry value")
    {
        return Err("retry did not persist the accepted value".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn reopened_identical_form(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let title = a.draft.as_ref().unwrap().title.clone();
            title.update(cx, |state, cx| state.set_value("Same value", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("identical form open failed: {error:?}"))?;
    let saved = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.request_save_draft(w, cx))
        })
        .map_err(|error| format!("identical form save failed: {error:?}"))?;
    if saved {
        return Err("identical form save unexpectedly reported durable".into());
    }
    // Reopen the same task with the exact same value before the first ACK.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.draft = None;
            a.navigation.task_edit_baseline = None;
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let title = a.draft.as_ref().unwrap().title.clone();
            title.update(cx, |state, cx| state.set_value("Same value", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("identical form reopen failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "identical form ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (open, value, disk) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.draft.is_some(),
            a.draft
                .as_ref()
                .map(|draft| draft.title.read(cx).value().to_string()),
            read_v4_store(root, &path).ok(),
        )
    });
    if !open || value.as_deref() != Some("Same value") {
        return Err(format!(
            "stale ACK closed or changed a newer identical form: open={open} value={value:?}"
        ));
    }
    if disk
        .and_then(|store| store.task(task).map(|task| task.title.clone()))
        .as_deref()
        != Some("Same value")
    {
        return Err("identical form ACK did not persist the saved value".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn exit_target() -> crate::draft_exit::ExitTarget {
    crate::draft_exit::ExitTarget::Navigate(
        crate::navigation_context::NavigationTarget::Research {
            scope: crate::app::Scope::All,
        },
    )
}

fn dirty_exit_task_editor(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let input = a.draft.as_ref().unwrap().title.clone();
            input.update(cx, |state, cx| state.set_value("Exit saved", w, cx));
            assert!(!a.request_exit(exit_target(), w, cx));
            assert!(a.navigation.pending_exit.is_some());
            a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("dirty task exit failed: {error:?}"))?;
    let (pending, error, after) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.navigation.pending_exit.is_some(),
            a.navigation
                .pending_exit
                .as_ref()
                .and_then(|pending| pending.error.clone()),
            a.navigation.after_save_target.is_some(),
        )
    });
    if error.is_some() {
        return Err(format!(
            "live pending task exit was reported as failure before its receipt: {error:?}"
        ));
    }
    if !pending && !after {
        return Err("live pending task exit lost its decision".into());
    }
    pump_until(&mut cx, handle, &app, "dirty task exit receipt", |a| {
        !a.live_has_pending_receipts() && a.navigation.pending_exit.is_none()
    })?;
    let (scope, draft, saved) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.scope(),
            a.draft.is_some(),
            a.store.task(task).map(|task| task.title.clone()),
        )
    });
    if scope != crate::app::Scope::All {
        return Err(format!("task exit did not proceed after ACK: scope={scope:?}"));
    }
    if draft {
        return Err("task exit left the saved editor open after ACK".into());
    }
    if saved.as_deref() != Some("Exit saved") {
        return Err(format!("task exit did not persist: {saved:?}"));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn dirty_exit_task_reopen_identical(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let title = a.draft.as_ref().unwrap().title.clone();
            title.update(cx, |state, cx| state.set_value("Identical exit", w, cx));
            assert!(!a.request_exit(exit_target(), w, cx));
            a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx);
            // A different surface and a fresh generation with the same values
            // exist before the first ACK. The stale ACK must not navigate or
            // close this editor.
            a.navigate(crate::app::Scope::Project(1), w, cx);
            a.open_task(task, w, cx);
            a.begin_draft(task, w, cx);
            let title = a.draft.as_ref().unwrap().title.clone();
            title.update(cx, |state, cx| state.set_value("Identical exit", w, cx));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("reopen task exit failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "reopen task ACK", |a| {
        !a.live_has_pending_receipts() && a.navigation.pending_saved_exit.is_none()
    })?;
    let (scope, draft, value, count) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.scope(),
            a.draft.is_some(),
            a.draft
                .as_ref()
                .map(|draft| draft.title.read(cx).value().to_string()),
            read_v4_store(root, &path)
                .ok()
                .map(|store| store.research_requests().len())
                .unwrap_or(0),
        )
    });
    if scope != crate::app::Scope::Project(1) || !draft || value.as_deref() != Some("Identical exit")
    {
        return Err(format!(
            "stale task ACK navigated/closed a newer identical editor: scope={scope:?} open={draft} value={value:?}"
        ));
    }
    let _ = count;
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn dirty_exit_automation_reopen_identical(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let before = cx.update(|cx| app.read(cx).store.automations().len());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Identical automation", w, cx);
            });
            assert!(!a.request_exit(
                crate::draft_exit::ExitTarget::CancelWizard { editing: None },
                w,
                cx,
            ));
            a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx);
            // Reopen with the same values, new form epoch, before ACK.
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Identical automation", w, cx);
            });
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("reopen automation exit failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "reopen automation ACK", |a| {
        !a.live_has_pending_receipts() && a.navigation.pending_saved_exit.is_none()
    })?;
    let (creating, value, count) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.objective.read(cx).value().to_string(),
            read_v4_store(root, &path)
                .ok()
                .map(|store| store.automations().len())
                .unwrap_or(0),
        )
    });
    if !creating || value != "Identical automation" {
        return Err(format!(
            "stale automation ACK closed/changed a newer identical wizard: creating={creating} value={value:?}"
        ));
    }
    if count != before + 1 {
        return Err(format!(
            "automation save-and-exit did not persist exactly one draft: before={before} disk={count}"
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn dirty_exit_automation(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let before = cx.update(|cx| app.read(cx).store.automations().len());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Check prices every day and cite a source.", w, cx);
            });
            assert!(!a.request_exit(
                crate::draft_exit::ExitTarget::CancelWizard { editing: None },
                w,
                cx,
            ));
            assert!(a.navigation.pending_exit.is_some());
            a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("dirty automation exit failed: {error:?}"))?;
    let error = cx.update(|cx| {
        app.read(cx)
            .navigation
            .pending_exit
            .as_ref()
            .and_then(|pending| pending.error.clone())
    });
    if error.is_some() {
        return Err(format!(
            "live pending automation exit was reported as failure before its receipt: {error:?}"
        ));
    }
    pump_until(&mut cx, handle, &app, "dirty automation exit receipt", |a| {
        !a.live_has_pending_receipts() && a.navigation.pending_exit.is_none()
    })?;
    let (count, wizard, error) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.automations().len(),
            a.automation_ui.creating,
            a.error.clone(),
        )
    });
    if count != before + 1 {
        return Err(format!(
            "automation save-and-exit did not commit exactly one draft: before={before} after={count} error={error:?}"
        ));
    }
    if wizard {
        return Err("automation save-and-exit left the wizard open after ACK".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn dirty_exit_planning(root: &Path, cancel: bool) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let before = cx.update(|cx| app.read(cx).store.research_requests().len());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            // Start from an explicit route different from the requested exit
            // target, so remaining on it proves the pending receipt withheld
            // navigation.
            a.navigate(crate::app::Scope::Project(1), w, cx);
            a.open_project_plan(1, w, cx);
            a.project_brief.update(cx, |state, cx| {
                state.set_value("Live planning request", w, cx);
            });
            let target = if cancel {
                crate::draft_exit::ExitTarget::CancelPlanning
            } else {
                exit_target()
            };
            assert!(!a.request_exit(target, w, cx));
            assert!(a.navigation.pending_exit.is_some());
            a.decide_exit(crate::draft_exit::ExitDecision::Save, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("dirty planning exit failed: {error:?}"))?;
    let (scope, composer, composer_text, pending, after, error) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.scope(),
            a.planning_destination.is_some(),
            a.project_brief.read(cx).value().to_string(),
            a.navigation.pending_exit.is_some(),
            a.navigation.after_save_target.is_some(),
            a.navigation
                .pending_exit
                .as_ref()
                .and_then(|pending| pending.error.clone()),
        )
    });
    if scope != crate::app::Scope::Project(1) {
        return Err(format!(
            "planning request exit proceeded before its durable receipt: scope={scope:?}"
        ));
    }
    if !composer || composer_text != "Live planning request" {
        return Err(format!(
            "planning composer was not retained pending ACK: open={composer} text={composer_text:?}"
        ));
    }
    if error.is_some() {
        return Err(format!(
            "live pending planning request was reported as failure: {error:?}"
        ));
    }
    if !pending && !after {
        return Err("planning request exit lost its intended destination".into());
    }
    pump_until(&mut cx, handle, &app, "planning request ACK", |a| {
        !a.live_has_pending_receipts() && a.navigation.pending_exit.is_none()
    })?;
    let (scope, composer, count, disk) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.scope(),
            a.planning_destination.is_some(),
            a.store.research_requests().len(),
            read_v4_store(root, &path).ok(),
        )
    });
    // CancelPlanning is an in-place close that preserves the current route;
    // the structural exit target must reach All exactly once.
    let expected_scope = if cancel {
        crate::app::Scope::Project(1)
    } else {
        crate::app::Scope::All
    };
    if scope != expected_scope {
        return Err(format!(
            "planning request exit reached the wrong destination after ACK: expected {expected_scope:?} got {scope:?}"
        ));
    }
    if composer {
        return Err("planning composer stayed open after its ACK".into());
    }
    if count != before + 1 {
        return Err(format!(
            "planning request was not committed exactly once: before={before} after={count}"
        ));
    }
    let disk_count = disk
        .map(|store| store.research_requests().len())
        .unwrap_or(0);
    if disk_count != before + 1 {
        return Err(format!(
            "planning request was not durable after ACK: disk={disk_count}"
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_rejected_unrelated(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let task = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let base = cx.update(|cx| app.read(cx).store.max_concurrent_runs());
    let target = if base < 10 { base + 1 } else { base - 1 };
    let mut rejected = false;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Rejected automation", w, cx);
            });
            // Make the primary slot busy; the automation command must reject
            // and must not leave a handoff for a later unrelated command.
            assert!(a.apply_scoped_settings(
                crate::scoped_settings::SettingsChange::ExecutionConcurrency(target),
                cx,
            ));
            assert!(a.live_primary.is_some());
            let outcome = a.save_automation_draft_outcome(cx);
            rejected =
                matches!(outcome, crate::automation_views::AutomationSaveOutcome::Rejected(_));
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("rejected automation save failed: {error:?}"))?;
    if !rejected {
        return Err("busy automation save was not rejected".into());
    }
    let marker_after_reject = cx.update(|cx| {
        app.read(cx)
            .navigation
            .pending_automation_save
            .is_some()
    });
    pump_until(&mut cx, handle, &app, "settings ACK before unrelated", |a| {
        !a.live_has_pending_receipts()
    })?;
    let unrelated = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.apply_domain_command_undoable(
                    crate::scoped_domain::DomainCommand::TaskTitle {
                        task,
                        title: "Unrelated edit".into(),
                    },
                    "Task renamed",
                    cx,
                )
            })
        })
        .map_err(|error| format!("unrelated command failed: {error:?}"))?;
    if unrelated.is_none() {
        return Err("unrelated command was rejected unexpectedly".into());
    }
    pump_until(&mut cx, handle, &app, "unrelated ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (creating, selected, value, marker_left, count, disk) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.selected.is_some(),
            a.automation_ui.objective.read(cx).value().to_string(),
            a.navigation.pending_automation_save.is_some(),
            a.store.automations().len(),
            read_v4_store(root, &path)
                .ok()
                .map(|store| store.automations().len())
                .unwrap_or(0),
        )
    });
    if marker_after_reject || marker_left {
        return Err(format!(
            "rejected automation save left a stale handoff: after_reject={marker_after_reject} left={marker_left}"
        ));
    }
    if !creating || selected || value != "Rejected automation" {
        return Err(format!(
            "unrelated command consumed/closed the rejected form: creating={creating} selected={selected} value={value:?}"
        ));
    }
    if count != 0 || disk != 0 {
        return Err(format!(
            "rejected automation save was committed: memory={count} disk={disk}"
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_save_case(root: &Path, finish_step3: bool, newer_form: bool) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let _ = updated_task(&mut store);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let before = cx.update(|cx| app.read(cx).store.automations().len());
    let mut step_seen = 0u8;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Check prices every day and cite a source.", w, cx);
            });
            if finish_step3 {
                a.automation_wizard_next(w, cx);
                a.automation_wizard_next(w, cx);
                step_seen = a.automation_ui.step;
            }
            let saved = a.save_automation_draft(cx);
            assert!(!saved || step_seen == 3);
            if newer_form {
                // A newer wizard with different input opens before the ACK.
                a.start_automation_create(w, cx);
                a.automation_ui.objective.update(cx, |state, cx| {
                    state.set_value("Newer automation input", w, cx);
                });
            }
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("automation save failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "automation save ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (creating, selected, value, feedback, count) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.selected.is_some(),
            a.automation_ui.objective.read(cx).value().to_string(),
            a.automation_ui.feedback.clone(),
            read_v4_store(root, &path)
                .ok()
                .map(|store| store.automations().len())
                .unwrap_or(0),
        )
    });
    if count != before + 1 {
        return Err(format!(
            "automation save did not persist exactly one draft: before={before} disk={count}"
        ));
    }
    if newer_form {
        if !creating || value != "Newer automation input" {
            return Err(format!(
                "stale automation save ACK closed/changed a newer wizard: creating={creating} value={value:?}"
            ));
        }
    } else {
        if creating {
            return Err("standalone automation save never cleared the wizard after ACK".into());
        }
        if !selected && !finish_step3 {
            return Err("standalone automation save did not select the saved draft".into());
        }
        if feedback
            .as_ref()
            .and_then(|value| value.as_ref().err())
            .is_some()
        {
            return Err(format!(
                "standalone automation save reported an error after ACK: {feedback:?}"
            ));
        }
        if finish_step3 && step_seen != 3 {
            return Err(format!("Save draft & finish was not on step 3: step={step_seen}"));
        }
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
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
    let mut inner_us = 0u128;
    let saved = cx
        .update_window(handle, |_, w, cx| {
            let inner = Instant::now();
            let saved = app.update(cx, |a, cx| a.request_save_draft(w, cx));
            inner_us = inner.elapsed().as_micros();
            saved
        })
        .map_err(|error| format!("perf editor save failed: {error:?}"))?;
    let ui = started.elapsed().as_micros();
    eprintln!("LIVE_EDITOR_SAVE label={label} inner_us={inner_us} update_window_us={ui}");
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

fn lazy_period_store(root: &Path) -> Result<(Store, u64, u64, Vec<u64>), String> {
    let template = crate::automation_execution::build_template(
        "Collect evidence and write a short report",
        "A short report with sources",
    )
    .map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("qa/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
    let automation = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Lazy period".into(),
            "Summarize the offline fixture evidence.".into(),
            "A short report with sources.".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            temp_automation_now(),
        )
        .map_err(|error| error.to_string())?;
    store.approve_and_enable(automation, Vec::new(), temp_automation_now()).map_err(|error| error.to_string())?;
    let mut occurrence = store
        .request_manual_occurrence(automation, "lazy-period", temp_automation_now())
        .map_err(|error| error.to_string())?;
    let goal = occurrence.research_goal_id.ok_or("lazy fixture goal")?;
    let steps = store.research_children(goal);
    if steps.len() < 2 {
        return Err("lazy fixture must have multiple steps".into());
    }
    let mut next_id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
    let mut run_ids = Vec::new();
    for task in &steps {
        store.set_stage(*task, Stage::Done).map_err(|error| error.to_string())?;
        let signature = store
            .research_signature(*task)
            .ok_or_else(|| format!("lazy fixture signature missing for task {task}"))?;
        let kind = store
            .task(*task)
            .and_then(|task| task.research.as_ref())
            .map(|research| research.kind)
            .ok_or("lazy fixture research kind")?;
        let body = format!(
            "{{\"status\":\"complete\",\"report\":\"Saved report for task {task}\",\"blocker\":null,\"sources\":[{{\"title\":\"Fixture\",\"url\":\"https://example.com/{task}\",\"published_at\":\"unknown\"}}]}}"
        );
        let run = AgentRun {
            automation_occurrence_id: Some(occurrence.id),
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: next_id,
            task_title: format!("Step {task}"),
            directory: String::new(),
            prompt: "fixture".into(),
            started_at: "2026-11-08T00:00:00Z".into(),
            queued_at: "2026-11-08T00:00:00Z".into(),
            workspace_source: None,
            ended_at: Some("2026-11-08T00:01:00Z".into()),
            session_id: None,
            exit_code: Some(0),
            task_id: *task,
            agent: "OpenCode".into(),
            model: "qa/model".into(),
            stage: Stage::Done,
            status: "Succeeded".into(),
            logs: vec!["[research] Complete · result validated".into()],
            output: body,
            attachments: Vec::new(),
            research_kind: Some(kind),
            sources: vec![crate::research::Source {
                title: "Fixture".into(),
                url: format!("https://example.com/{task}"),
                published_at: "unknown".into(),
            }],
            input_signature: signature,
        };
        run_ids.push(next_id);
        store.push_run(run);
        next_id += 1;
    }
    if let Some(mut value) = store.occurrence_mut(occurrence.id) {
        value.run_ids = run_ids.clone();
        value.final_run_id = run_ids.last().copied();
        value.status = crate::automation::OccurrenceStatus::Succeeded;
        occurrence = value.clone();
    }
    Ok((store, automation, occurrence.id, steps))
}

fn lazy_period_hydrates_on_open(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let (store, _automation, occurrence, steps) = lazy_period_store(root)?;
    let final_task = *steps.last().ok_or("lazy final task")?;
    let final_run = store
        .runs()
        .iter()
        .rev()
        .find(|run| run.task_id == final_task)
        .map(|run| run.id)
        .ok_or("lazy final run")?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let before = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.runs().iter().find(|run| run.id == final_run).is_some_and(|run| run.output.is_empty()),
            a.store.run_body_is_lazy(final_run),
            a.history_needs_pump(),
        )
    });
    if !before.0 || !before.1 {
        return Err(format!("fixture did not open lazy: {:?}", before));
    }
    if before.2 {
        return Err("fixture was not idle before opening the period".into());
    }
    // No manual pump: production render must schedule the pending body work.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_occurrence_result(occurrence, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("open lazy period: {error:?}"))?;
    advance_until(&mut cx, &app, "lazy period bodies", |a| {
        steps.iter().all(|task| {
            a.store
                .runs()
                .iter()
                .find(|run| run.task_id == *task && run.status == "Succeeded")
                .is_some_and(|run| !run.output.is_empty())
        }) && !a.store.run_body_is_lazy(final_run)
    })?;
    let (final_report, results) = cx.update(|cx| {
        let a = app.read(cx);
        let occurrence = a.store.occurrence(occurrence).unwrap();
        let presentation = crate::result_presentation::present_occurrence(&a.store, occurrence);
        (
            presentation.has_final_report,
            steps
                .iter()
                .filter(|task| a.store.research_result(**task).is_some())
                .count(),
        )
    });
    if !final_report || results != steps.len() {
        return Err(format!(
            "lazy period did not become truthful: final={final_report} results={results}/{}",
            steps.len()
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn lazy_body_missing_and_retry(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let (store, _automation, occurrence, steps) = lazy_period_store(root)?;
    let final_task = *steps.last().ok_or("lazy final task")?;
    let final_run = store
        .runs()
        .iter()
        .rev()
        .find(|run| run.task_id == final_task)
        .map(|run| run.id)
        .ok_or("lazy final run")?;
    let path = seed_store(root, &store)?;
    // A known-body run whose body row is gone must show missing, not complete.
    let connection = rusqlite::Connection::open(&path).map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM run_bodies WHERE run_id=?1",
            rusqlite::params![final_run as i64],
        )
        .map_err(|error| error.to_string())?;
    drop(connection);
    let (mut cx, handle, app) = open_live(root, &path)?;
    let idle = cx.update(|cx| !app.read(cx).history_needs_pump());
    if !idle {
        return Err("missing-body fixture was not idle before opening the period".into());
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_occurrence_result(occurrence, w, cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("open missing body period: {error:?}"))?;
    advance_until(&mut cx, &app, "missing body state", |a| {
        matches!(
            a.run_body_state.get(&final_run),
            Some(crate::history_reader::BodyLoad::Absent)
        )
    })?;
    let (report, result) = cx.update(|cx| {
        let a = app.read(cx);
        (
            crate::result_presentation::present_occurrence(
                &a.store,
                a.store.occurrence(occurrence).unwrap(),
            )
            .has_final_report,
            a.store.research_result(final_task).is_some(),
        )
    });
    if report || result {
        return Err("missing lazy body was presented as a current report".into());
    }
    // Explicit body retry must re-request this run, not use the persistence footer.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.run_body_state.insert(
                final_run,
                crate::history_reader::BodyLoad::Failed {
                    error: "fixture failure".into(),
                    retry_at: std::time::Instant::now(),
                },
            );
        });
        w.render_frame(cx);
        assert!(
            w.try_find("automation-report-retry").is_some(),
            "failed body read must expose an explicit retry action"
        );
        w.click("automation-report-retry", cx);
        w.render_frame(cx);
        app.update(cx, |a, _| {
            assert!(
                !matches!(
                    a.run_body_state.get(&final_run),
                    Some(crate::history_reader::BodyLoad::Failed { .. })
                ),
                "body retry must re-request the selected body"
            );
        });
    })
    .map_err(|error| format!("body retry: {error:?}"))?;
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn lazy_latest_attempt_invalidates(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let (store, _automation, _occurrence, steps) = lazy_period_store(root)?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let task = steps[0];
    let (pending_before, result_before) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.research_result_body_pending(task).is_some(),
            a.store.research_result(task).is_some(),
        )
    });
    if !pending_before || result_before {
        return Err(format!(
            "lazy pending before retry was wrong: pending={pending_before} result={result_before}"
        ));
    }
    cx.update(|cx| {
        let a = app.read(cx);
        let mut store = a.store.clone();
        let next_id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
        let mut failed = crate::domain::AgentRun {
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: next_id,
            task_title: "Retry".into(),
            directory: String::new(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            task_id: task,
            agent: "OpenCode".into(),
            model: "qa/model".into(),
            stage: Stage::Done,
            status: "Failed".into(),
            logs: Vec::new(),
            output: String::new(),
            attachments: Vec::new(),
            research_kind: None,
            sources: Vec::new(),
            input_signature: String::new(),
        };
        store.push_run(failed);
        if store.research_result_body_pending(task).is_some() || store.research_result(task).is_some() {
            return Err("latest failed attempt did not invalidate older lazy result".to_string());
        }
        Ok::<(), String>(())
    })?;
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}
