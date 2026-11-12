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
const CASES: [&str; 40] = [
    "core",
    "reopen",
    "pending-quit",
    "window-reopen",
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
    "lazy-research-queue",
    "lazy-research-queue-retry",
    "lazy-research-queue-pause",
    "lazy-body-missing-retry",
    "lazy-latest-failed",
    "t04-automation-lifecycle-undo",
    "t15-incremental-sql-audit",
    "automation-generate-plan-async",
    "automation-generate-plan-unchanged",
    "automation-generate-plan-retry",
    "automation-generate-plan-cancel",
    "automation-enable-after-plan",
    "automation-enable-new-quick",
    "automation-pause-navigate-away",
    "automation-enable-duplicate",
    "automation-enable-same-form-edit",
    "automation-enable-new-quick-edit",
    "automation-enable-new-quick-leave",
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
        "window-reopen" => window_reopen(&root.join("window-reopen")),
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
        "automation-generate-plan-async" => {
            automation_generate_plan_async(&root.join("automation-generate-plan-async"), false)
        }
        "automation-generate-plan-unchanged" => {
            automation_generate_plan_async(&root.join("automation-generate-plan-unchanged"), true)
        }
        "automation-generate-plan-retry" => {
            automation_generate_plan_retry(&root.join("automation-generate-plan-retry"))
        }
        "automation-generate-plan-cancel" => {
            automation_generate_plan_cancel(&root.join("automation-generate-plan-cancel"))
        }
        "automation-enable-after-plan" => {
            automation_enable_after_plan(&root.join("automation-enable-after-plan"))
        }
        "automation-enable-new-quick" => {
            automation_enable_new_quick(&root.join("automation-enable-new-quick"))
        }
        "automation-pause-navigate-away" => {
            automation_pause_navigate_away(&root.join("automation-pause-navigate-away"))
        }
        "automation-enable-duplicate" => {
            automation_enable_duplicate(&root.join("automation-enable-duplicate"))
        }
        "automation-enable-same-form-edit" => {
            automation_enable_same_form_edit(&root.join("automation-enable-same-form-edit"))
        }
        "automation-enable-new-quick-edit" => {
            automation_enable_new_quick_edit(&root.join("automation-enable-new-quick-edit"))
        }
        "automation-enable-new-quick-leave" => {
            automation_enable_new_quick_leave(&root.join("automation-enable-new-quick-leave"))
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
        "lazy-research-queue" => lazy_research_queue(&root.join("lazy-research-queue"), "normal"),
        "lazy-research-queue-retry" => lazy_research_queue(&root.join("lazy-research-queue-retry"), "retry"),
        "lazy-research-queue-pause" => lazy_research_queue(&root.join("lazy-research-queue-pause"), "pause"),
        "lazy-period-hydration" => lazy_period_hydrates_on_open(&root.join("lazy-period")),
        "lazy-body-missing-retry" => lazy_body_missing_and_retry(&root.join("lazy-missing")),
        "lazy-latest-failed" => lazy_latest_attempt_invalidates(&root.join("lazy-failed")),
        "t04-automation-lifecycle-undo" => {
            t04_automation_lifecycle_undo(&root.join("t04-lifecycle-undo"))
        }
        "t15-incremental-sql-audit" => {
            t15_incremental_sql_audit(&root.join("t15-sql-audit"))
        }
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

fn automation_generate_plan_async(root: &Path, unchanged: bool) -> Result<(), String> {
    let expected = if unchanged { "Expected old" } else { "Expected new revision" };
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template("Plan async", "Expected old")?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Automation 2".into(),
            "Objective old".into(),
            "Expected old".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            if unchanged { None } else { Some(template) },
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| app.update(cx, |a, _| {
        a.qa_dispatch_fake = Some(crate::dispatch_admission::FakeDispatchRunner::default());
    }));
    let (version_before, requests_before, runs_before) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.automation(id).map(|definition| definition.version).unwrap_or(0),
            a.store.research_requests().len(),
            a.store.runs().len(),
        )
    });
    let mut step = 0u8;
    let mut pending_after_click = false;
    let mut planning_run_after_click = false;
    let mut feedback_after_click = String::new();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.replan_automation(id, w, cx);
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value(expected, w, cx);
            });
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            step = a.automation_ui.step;
            planning_run_after_click = a.automation_ui.planning_run.is_some();
            feedback_after_click = a
                .automation_ui
                .feedback
                .clone()
                .map(|value| match value {
                    Ok(text) => text,
                    Err(text) => format!("ERR:{text}"),
                })
                .unwrap_or_default();
            pending_after_click = a.automation_planning_pending()
                && a.automation_ui.planning_run.is_none()
                && a.automation_ui.feedback.is_some()
                && !feedback_after_click.starts_with("ERR:");
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("generate-plan click failed: {error:?}"))?;
    if step != 3 || (!unchanged && (planning_run_after_click || !pending_after_click)) {
        return Err(format!(
            "Generate plan did not wait truthfully on step 3: step={step} run={planning_run_after_click} pending={pending_after_click} feedback={feedback_after_click:?}"
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut started = false;
    while Instant::now() < deadline {
        started = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                let a = app.read(cx);
                !a.live_has_pending_receipts()
                    && a.store.automations().iter().any(|definition| {
                        definition.id == id
                            && definition.version >= version_before
                            && definition.expected_result == expected
                    })
                    && a.store.research_requests().len() == requests_before + 1
                    && a.store.runs().len() == runs_before + 1
                    && a.automation_ui.planning_run.is_some()
                    && a.qa_dispatch_fake.as_ref().is_some_and(|fake| {
                        fake.starts.as_slice() == a.automation_ui.planning_run.as_slice()
                    })
                    && a.active_runs.is_empty()
            })
            .map_err(|error| format!("generate-plan ACK pump failed: {error:?}"))?;
        if started {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !started {
        let state = cx.update(|cx| {
            let a = app.read(cx);
            format!(
                "version={:?} requests={} runs={} planning_run={:?} pending={} step={} feedback={:?} failure={:?} receipts={}",
                a.store.automation(id).map(|d| d.version),
                a.store.research_requests().len(),
                a.store.runs().len(),
                a.automation_ui.planning_run,
                a.automation_planning_pending(),
                a.automation_ui.step,
                a.automation_ui.feedback,
                a.live_failure,
                a.live_has_pending_receipts(),
            )
        });
        return Err(format!("durable draft never started exactly one planner run: {state}"));
    }
    let (version_after, request, run_id, planning_run, step_after, feedback_after) =
        cx.update(|cx| {
            let a = app.read(cx);
            let request = a
                .store
                .research_requests()
                .iter()
                .find(|request| request.automation_id == Some(id))
                .cloned();
            (
                a.store.automation(id).map(|definition| definition.version).unwrap_or(0),
                request,
                a.store
                    .runs()
                    .iter()
                    .find(|run| {
                        run.planning_owner
                            .as_ref()
                            .is_some_and(|owner| owner.automation_id == Some(id))
                    })
                    .map(|run| run.id),
                a.automation_ui.planning_run,
                a.automation_ui.step,
                a.automation_ui.feedback.clone(),
            )
        });
    let Some(request) = request else {
        return Err("no immutable planning request was created".into());
    };
    if request.input.trim() != "Objective old"
        || request.expected_result.as_deref() != Some(expected)
        || request.automation_id != Some(id)
    {
        return Err(format!("planning request revision is wrong: {request:?}"));
    }
    if run_id.is_none() || planning_run != run_id {
        return Err(format!(
            "planner token does not match the one run: run={run_id:?} planning={planning_run:?}"
        ));
    }
    if version_after != version_before + u32::from(!unchanged) {
        return Err(format!(
            "definition version did not advance once: before={version_before} after={version_after}"
        ));
    }
    if step_after != 3
        || feedback_after
            .as_ref()
            .and_then(|value| value.as_ref().err())
            .is_some()
    {
        return Err(format!(
            "planning continuation did not settle: step={step_after} feedback={feedback_after:?}"
        ));
    }
    if !cx.update(|cx| app.read(cx).automation_ui.creating) {
        return Err("Generate plan closed the review wizard".into());
    }
    // No second click and no duplicate request/run after settling.
    std::thread::sleep(Duration::from_millis(50));
    let (requests_final, runs_final) = cx.update(|cx| {
        let a = app.read(cx);
        (a.store.research_requests().len(), a.store.runs().len())
    });
    if requests_final != requests_before + 1 || runs_final != runs_before + 1 {
        return Err(format!(
            "planning continuation duplicated work: requests={requests_before}->{requests_final} runs={runs_before}->{runs_final}"
        ));
    }
    let disk = read_v4_store(root, &path)?;
    let persisted = disk
        .automation(id)
        .ok_or("persisted automation missing after replan")?;
    if persisted.expected_result != expected || persisted.version != version_after {
        return Err("durable draft revision does not match memory".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}


fn seed_planned_research_draft(root: &Path) -> Result<(u64, PathBuf), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template("Plan async", "Expected old")?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Automation 2".into(),
            "Objective old".into(),
            "Expected old".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    let path = seed_store(root, &store)?;
    Ok((id, path))
}

fn start_generate_plan_click(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    id: u64,
    expected: &str,
) -> Result<(u8, bool, bool, String), String> {
    let mut step = 0u8;
    let mut pending = false;
    let mut planning_run = false;
    let mut feedback = String::new();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.replan_automation(id, w, cx);
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value(expected, w, cx);
            });
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            step = a.automation_ui.step;
            pending = a.automation_planning_pending();
            planning_run = a.automation_ui.planning_run.is_some();
            feedback = a
                .automation_ui
                .feedback
                .clone()
                .map(|value| match value {
                    Ok(text) => text,
                    Err(text) => format!("ERR:{text}"),
                })
                .unwrap_or_default();
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("generate-plan click failed: {error:?}"))?;
    Ok((step, pending, planning_run, feedback))
}

fn automation_generate_plan_retry(root: &Path) -> Result<(), String> {
    let (id, path) = seed_planned_research_draft(root)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| app.update(cx, |a, _| {
        a.qa_dispatch_fake = Some(crate::dispatch_admission::FakeDispatchRunner::default());
    }));
    let (version_before, requests_before, runs_before) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.automation(id).map(|d| d.version).unwrap_or(0),
            a.store.research_requests().len(),
            a.store.runs().len(),
        )
    });
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.dispatch_admission
                .as_mut()
                .ok_or("missing dispatch admission")?
                .set_fault(crate::sqlite_v4::Fault::BeforeCommit)
        })
    })
    .map_err(|error| format!("set plan write fault failed: {error}"))?;
    let (step, pending, planning_run, feedback) =
        start_generate_plan_click(&mut cx, handle, &app, id, "Expected new revision")?;
    if step != 3 || !pending || planning_run || feedback.starts_with("ERR:") {
        return Err(format!(
            "retry case did not wait truthfully: step={step} pending={pending} run={planning_run} feedback={feedback:?}"
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut failed = false;
    while Instant::now() < deadline {
        failed = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).live_failure.is_some()
            })
            .map_err(|error| format!("plan failure poll failed: {error:?}"))?;
        if failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !failed {
        return Err("injected draft save failure was not reported".into());
    }
    let (pending_failed, planning_failed, requests_failed, runs_failed) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_planning_pending(),
            a.automation_ui.planning_run.is_some(),
            a.store.research_requests().len(),
            a.store.runs().len(),
        )
    });
    if !pending_failed || planning_failed || requests_failed != requests_before || runs_failed != runs_before {
        return Err(format!(
            "failed save lost/consumed planning intent: pending={pending_failed} run={planning_failed} requests={requests_failed} runs={runs_failed}"
        ));
    }
    let disk_failed = read_v4_store(root, &path)?;
    if disk_failed.automation(id).map(|d| d.version) != Some(version_before) {
        return Err("failed draft save still advanced the durable version".into());
    }
    cx.update(|cx| {
        app.update(cx, |a, cx| {
            if let Some(adapter) = a.dispatch_admission.as_mut()
                && let Err(error) = adapter.set_fault(crate::sqlite_v4::Fault::None)
            {
                return Err(error);
            }
            a.retry_live_persistence(cx);
            Ok(())
        })
    })
    .map_err(|error| format!("clear plan write fault failed: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut started = false;
    while Instant::now() < deadline {
        started = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                let a = app.read(cx);
                !a.live_has_pending_receipts()
                    && a.store.automations().iter().any(|d| {
                        d.id == id
                            && d.version > version_before
                            && d.expected_result == "Expected new revision"
                    })
                    && a.store.research_requests().len() == requests_before + 1
                    && a.store.runs().len() == runs_before + 1
                    && a.automation_ui.planning_run.is_some()
                    && a.qa_dispatch_fake.as_ref().is_some_and(|fake| {
                        fake.starts.as_slice() == a.automation_ui.planning_run.as_slice()
                    })
                    && a.active_runs.is_empty()
            })
            .map_err(|error| format!("plan retry pump failed: {error:?}"))?;
        if started {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !started {
        return Err("real retry did not save the draft and start one planner run".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_generate_plan_cancel(root: &Path) -> Result<(), String> {
    let (id, path) = seed_planned_research_draft(root)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| app.update(cx, |a, _| {
        a.qa_dispatch_fake = Some(crate::dispatch_admission::FakeDispatchRunner::default());
    }));
    let (requests_before, runs_before) = cx.update(|cx| {
        let a = app.read(cx);
        (a.store.research_requests().len(), a.store.runs().len())
    });
    let (step, pending, planning_run, feedback) =
        start_generate_plan_click(&mut cx, handle, &app, id, "Expected new revision")?;
    if step != 3 || !pending || planning_run || feedback.starts_with("ERR:") {
        return Err("cancel case did not enter the waiting state".into());
    }
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.automation_wizard_back(cx));
    })
    .map_err(|error| format!("wizard back failed: {error:?}"))?;
    let (step_after, pending_after, planning_after) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.step,
            a.automation_planning_pending(),
            a.automation_ui.planning_run.is_some(),
        )
    });
    if step_after != 2 || pending_after || planning_after {
        return Err(format!(
            "Back did not clear waiting intent: step={step_after} pending={pending_after} run={planning_after}"
        ));
    }
    pump_until(&mut cx, handle, &app, "Back draft ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (requests_after, runs_after, creating_after, route_after) = cx.update(|cx| {
        let a = app.read(cx);
        (a.store.research_requests().len(), a.store.runs().len(), a.automation_ui.creating, a.automation_ui.selected)
    });
    if requests_after != requests_before || runs_after != runs_before {
        return Err(format!(
            "Back launched planning after ACK: requests={requests_before}->{requests_after} runs={runs_before}->{runs_after}"
        ));
    }
    if !creating_after || route_after != Some(id) {
        return Err(format!(
            "Back stole the wizard/route: creating={creating_after} selected={route_after:?}"
        ));
    }
    // Edit/new form while waiting must also clear intent and avoid launch.
    let (_, pending, _, _) = start_generate_plan_click(
        &mut cx, handle, &app, id, "Expected third revision",
    )?;
    if !pending {
        return Err("new-form cancellation fixture did not enter an async wait".into());
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.start_automation_create(w, cx));
    })
    .map_err(|error| format!("new form while waiting failed: {error:?}"))?;
    let pending_new = cx.update(|cx| app.read(cx).automation_planning_pending());
    if pending_new {
        return Err("new form while waiting retained planning intent".into());
    }
    pump_until(&mut cx, handle, &app, "new-form ACK", |a| {
        !a.live_has_pending_receipts()
    })?;
    let (requests_new, runs_new) = cx.update(|cx| {
        let a = app.read(cx);
        (a.store.research_requests().len(), a.store.runs().len())
    });
    if requests_new != requests_before || runs_new != runs_before {
        return Err(format!(
            "new form while waiting launched planning: requests={requests_before}->{requests_new} runs={runs_before}->{runs_new}"
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_after_plan(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template(
        "Enable plan",
        "Expected result",
    )?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Automation 2".into(),
            "Objective old".into(),
            "Expected result".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.edit_automation(id, w, cx);
            // This fixture represents the reviewed-plan boundary: the frozen
            // template is already present, as after a completed Generate plan.
            a.automation_ui.plan = a.store.automation(id).and_then(|d| d.template.clone());
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            assert_eq!(a.automation_ui.step, 3, "Enable fixture must reach review");
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("enable fixture review failed: {error:?}"))?;
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .map_err(|error| format!("enable click failed: {error:?}"))?;
    // Drive only the real pump timer; no manual poll can mask a wake gap.
    advance_until(&mut cx, &app, "enable durable receipt", |a| {
        !a.live_has_pending_receipts()
            && !a.automation_ui.creating
            && a.automation_ui.selected == Some(id)
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Enabled"))
    })?;
    let clean = cx.update(|cx| {
        let a = app.read(cx);
        a.dirty_draft(cx).is_none()
            && a.automation_ui.pending_command.is_none()
            && a.navigation.automation_baseline.is_none()
    });
    if !clean {
        return Err("enable ACK left a dirty wizard, baseline or pending receipt".into());
    }
    let disk = read_v4_store(root, &path)?;
    if disk.automation(id).map(|definition| definition.status)
        != Some(crate::automation::DefinitionStatus::Enabled)
    {
        return Err("enable was not durable after its ACK".into());
    }

    cx.update(|cx| app.update(cx, |a, cx| a.pause_automation_ui(id, cx)));
    advance_until(&mut cx, &app, "pause durable receipt", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Paused"))
    })?;
    let disk = read_v4_store(root, &path)?;
    if disk.automation(id).map(|definition| definition.status)
        != Some(crate::automation::DefinitionStatus::Paused)
    {
        return Err("pause was not durable after its ACK".into());
    }

    cx.update(|cx| app.update(cx, |a, cx| a.resume_automation_schedule(id, cx)));
    advance_until(&mut cx, &app, "resume durable receipt", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Enabled again"))
    })?;
    let disk = read_v4_store(root, &path)?;
    if disk.automation(id).map(|definition| definition.status)
        != Some(crate::automation::DefinitionStatus::Enabled)
    {
        return Err("resume was not durable after its ACK".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_new_quick(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.name.update(cx, |state, cx| {
                state.set_value("New quick", w, cx);
            });
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Check one value", w, cx);
            });
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value("One answer", w, cx);
            });
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            assert_eq!(a.automation_ui.step, 3);
            assert!(
                a.automation_ui.editing.is_none(),
                "new Quick creation has no saved editor id yet"
            );
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("new Quick review failed: {error:?}"))?;
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .map_err(|error| format!("new Quick enable click failed: {error:?}"))?;
    advance_until(&mut cx, &app, "new Quick enable ACK", |a| {
        !a.live_has_pending_receipts()
            && !a.automation_ui.creating
            && a.automation_ui.selected.is_some()
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Enabled"))
    })?;
    let (clean, id) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.dirty_draft(cx).is_none()
                && a.automation_ui.automation_command_inflight.is_none()
                && a.automation_ui.pending_command.is_none(),
            a.automation_ui.selected,
        )
    });
    let Some(id) = id else {
        return Err("new Quick enable did not select the created automation".into());
    };
    if !clean {
        return Err("new Quick enable left a dirty or pending wizard".into());
    }
    let disk = read_v4_store(root, &path)?;
    if disk.automations().len() != 1
        || disk.automation(id).map(|definition| definition.status)
            != Some(crate::automation::DefinitionStatus::Enabled)
    {
        return Err("new Quick enable was not durable exactly once".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn enabled_quick_store(root: &Path) -> Result<(Store, u64), String> {
    let template = crate::automation_execution::build_quick_template(
        "Check one value",
        "One answer",
    )?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Quick,
            "Quick detail".into(),
            "Check one value".into(),
            "One answer".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    store
        .approve_and_enable(id, vec![], 1_700_000_000)
        .map_err(|error| error.to_string())?;
    Ok((store, id))
}

fn automation_pause_navigate_away(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let (store, id) = enabled_quick_store(root)?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.automation_ui.selected = Some(id);
            a.automation_ui.creating = false;
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(true);
            a.pause_automation_ui(id, cx);
            assert!(
                a.automation_ui.automation_command_inflight.is_some(),
                "pause must keep its exact in-flight receipt"
            );
            // A new draft opens while the pause receipt is still pending.
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Newer draft", w, cx);
            });
            // An unrelated retained follow-up failure must not release the
            // lifecycle in-flight guard for the still-pending pause receipt.
            let workspace = a.live_workspace.unwrap_or(0);
            a.live_followups.push_back(crate::live_save_app::LiveGroup {
                generation: 9_999,
                epoch: 1,
                workspace,
                completions: vec![crate::live_save_app::LiveCompletion::None],
            });
            a.on_live_failure(
                9_999,
                1,
                "injected unrelated followup failure".into(),
                cx,
            );
            assert!(
                a.automation_ui.automation_command_inflight.is_some(),
                "unrelated followup failure released the pause in-flight guard"
            );
            a.live_followups.clear();
            a.live_failure = None;
            a.storage_error.clear();
            a.live_retry_at = None;
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(false);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("pause/navigate fixture failed: {error:?}"))?;
    advance_until(&mut cx, &app, "pause delayed ACK", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui.automation_command_inflight.is_none()
    })?;
    let (creating, objective, feedback, selected) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.objective.read(cx).value().to_string(),
            a.automation_ui.feedback.clone(),
            a.automation_ui.selected,
        )
    });
    if !creating || objective != "Newer draft" || selected == Some(id) {
        return Err(format!(
            "delayed pause ACK overwrote the newer draft: creating={creating} objective={objective:?} selected={selected:?}"
        ));
    }
    if feedback
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .is_some_and(|message| message.starts_with("Paused"))
    {
        return Err("delayed pause ACK wrote page feedback onto a newer draft".into());
    }
    let disk = read_v4_store(root, &path)?;
    if disk.automation(id).map(|definition| definition.status)
        != Some(crate::automation::DefinitionStatus::Paused)
    {
        return Err("pause was not durable after the delayed ACK".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_duplicate(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template(
        "Enable plan",
        "Expected result",
    )?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Automation 2".into(),
            "Objective old".into(),
            "Expected result".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    let before = store.automation(id).map(|definition| definition.version).unwrap_or(0);
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.edit_automation(id, w, cx);
            a.automation_ui.plan = a.store.automation(id).and_then(|d| d.template.clone());
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(true);
            a.enable_automation(cx);
            assert!(
                a.automation_ui.automation_command_inflight.is_some(),
                "first enable must occupy the in-flight slot"
            );
            a.enable_automation(cx);
            assert!(
                a.automation_ui
                    .feedback
                    .as_ref()
                    .and_then(|value| value.as_ref().err())
                    .is_some_and(|message| message.contains("Wait for the pending")),
                "duplicate enable must be rejected while the receipt is pending"
            );
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(false);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("duplicate enable fixture failed: {error:?}"))?;
    advance_until(&mut cx, &app, "duplicate enable ACK", |a| {
        !a.live_has_pending_receipts()
            && !a.automation_ui.creating
            && a.automation_ui.automation_command_inflight.is_none()
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Enabled"))
    })?;
    let disk = read_v4_store(root, &path)?;
    let definition = disk.automation(id).ok_or("duplicate enable lost the automation")?;
    if definition.version != before
        || definition.status != crate::automation::DefinitionStatus::Enabled
    {
        return Err(format!(
            "duplicate enable changed the definition more than once: before={before} after={} status={:?}",
            definition.version, definition.status
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_same_form_edit(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template(
        "Enable plan",
        "Expected result",
    )?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let id = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Automation 2".into(),
            "Objective old".into(),
            "Expected result".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template),
            1_700_000_000,
        )
        .map_err(|error| error.to_string())?;
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.edit_automation(id, w, cx);
            a.automation_ui.plan = a.store.automation(id).and_then(|d| d.template.clone());
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(true);
            a.enable_automation(cx);
            assert!(a.automation_ui.automation_command_inflight.is_some());
            // Same form, newer field edit while the receipt is pending.
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Edited while pending", w, cx);
            });
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(false);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("same-form enable fixture failed: {error:?}"))?;
    advance_until(&mut cx, &app, "same-form enable ACK", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui.automation_command_inflight.is_none()
    })?;
    let (creating, objective, feedback, dirty) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.objective.read(cx).value().to_string(),
            a.automation_ui
                .feedback
                .clone()
                .and_then(|value| value.ok())
                .unwrap_or_default(),
            a.dirty_draft(cx).is_some(),
        )
    });
    if !creating || objective != "Edited while pending" || !dirty {
        return Err(format!(
            "same-form enable ACK closed or cleaned newer edits: creating={creating} objective={objective:?} dirty={dirty}"
        ));
    }
    if !feedback.contains("newer edits remain unsaved") {
        return Err(format!(
            "same-form enable ACK did not disclose unsaved edits: {feedback:?}"
        ));
    }
    let disk = read_v4_store(root, &path)?;
    let definition = disk.automation(id).ok_or("same-form enable lost the automation")?;
    if definition.status != crate::automation::DefinitionStatus::Enabled
        || definition.objective != "Objective old"
    {
        return Err(format!(
            "same-form enable persisted the wrong version: status={:?} objective={:?}",
            definition.status, definition.objective
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_new_quick_edit(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.name.update(cx, |state, cx| {
                state.set_value("New quick", w, cx);
            });
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Original quick", w, cx);
            });
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value("One answer", w, cx);
            });
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            assert!(a.automation_ui.editing.is_none());
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(true);
            a.enable_automation(cx);
            assert!(a.automation_ui.automation_command_inflight.is_some());
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Edited while pending", w, cx);
            });
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(false);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("new Quick edit fixture failed: {error:?}"))?;
    advance_until(&mut cx, &app, "new Quick edit ACK", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui.automation_command_inflight.is_none()
    })?;
    let (creating, editing, selected, objective, dirty) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.editing,
            a.automation_ui.selected,
            a.automation_ui.objective.read(cx).value().to_string(),
            a.dirty_draft(cx).is_some(),
        )
    });
    let Some(first_id) = editing else {
        return Err("new Quick edit ACK did not adopt the durable definition id".into());
    };
    if !creating || selected != Some(first_id) || objective != "Edited while pending" || !dirty {
        return Err(format!(
            "new Quick edit ACK lost the form identity/dirty state: creating={creating} editing={editing:?} selected={selected:?} objective={objective:?} dirty={dirty}"
        ));
    }
    // The next Enable must update the same definition, not create a second one.
    cx.update_window(handle, |_, w, cx| {
        w.click("enable-automation", cx);
        w.render_frame(cx);
    })
    .map_err(|error| format!("new Quick second enable click failed: {error:?}"))?;
    advance_until(&mut cx, &app, "new Quick second enable ACK", |a| {
        !a.live_has_pending_receipts()
            && !a.automation_ui.creating
            && a.automation_ui
                .feedback
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .is_some_and(|message| message.starts_with("Enabled"))
    })?;
    let disk = read_v4_store(root, &path)?;
    if disk.automations().len() != 1 {
        return Err(format!(
            "second Enable created a duplicate definition: {} automations",
            disk.automations().len()
        ));
    }
    let definition = disk
        .automation(first_id)
        .ok_or("second Enable lost the adopted definition")?;
    if definition.status != crate::automation::DefinitionStatus::Enabled
        || definition.objective != "Edited while pending"
    {
        return Err(format!(
            "second Enable did not update the same definition: status={:?} objective={:?}",
            definition.status, definition.objective
        ));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn automation_enable_new_quick_leave(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| {
        app.update(cx, |a, _| {
            a.qa_dispatch_fake =
                Some(crate::dispatch_admission::FakeDispatchRunner::default());
        })
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("First quick", w, cx);
            });
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value("One answer", w, cx);
            });
            a.automation_wizard_next(w, cx);
            a.automation_wizard_next(w, cx);
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(true);
            a.enable_automation(cx);
            // Leave the originating wizard and open a different new form
            // before the durable ACK returns.
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value("Second draft", w, cx);
            });
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .set_hold(false);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("new Quick leave fixture failed: {error:?}"))?;
    advance_until(&mut cx, &app, "new Quick leave ACK", |a| {
        !a.live_has_pending_receipts()
            && a.automation_ui.automation_command_inflight.is_none()
    })?;
    let (creating, editing, selected, objective, feedback) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.automation_ui.creating,
            a.automation_ui.editing,
            a.automation_ui.selected,
            a.automation_ui.objective.read(cx).value().to_string(),
            a.automation_ui.feedback.clone(),
        )
    });
    if !creating || editing.is_some() || selected.is_some() || objective != "Second draft" {
        return Err(format!(
            "late new Quick ACK stole the newer form/route: creating={creating} editing={editing:?} selected={selected:?} objective={objective:?}"
        ));
    }
    if feedback
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .is_some_and(|message| message.starts_with("Enabled"))
    {
        return Err("late new Quick ACK wrote page feedback onto the newer form".into());
    }
    let disk = read_v4_store(root, &path)?;
    if disk.automations().len() != 1 {
        return Err(format!(
            "late new Quick ACK duplicated the definition: {} automations",
            disk.automations().len()
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

fn automation_snapshot(store: &Store) -> (Vec<crate::automation::AutomationDefinition>, Vec<crate::automation::AutomationOccurrence>, Vec<AgentRun>, u64, u64) {
    (
        store.automations().to_vec(),
        store.all_occurrences().to_vec(),
        store.runs().to_vec(),
        store.next_id(),
        store.run_id_floor(),
    )
}

fn finish_pending(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, app: &Entity<TaskApp>, label: &str) -> Result<(), String> {
    pump_until(cx, handle, app, label, |a| !a.live_has_pending_receipts())
}

fn prepare_paused_automation(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    objective: &str,
) -> Result<u64, String> {
    let existing = cx.update(|cx| app.read(cx).store.automations().len());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.start_automation_create(w, cx);
            a.automation_ui.objective.update(cx, |state, cx| {
                state.set_value(objective, w, cx);
            });
            a.automation_ui.expected.update(cx, |state, cx| {
                state.set_value("One answer", w, cx);
            });
            let _ = a.save_automation_draft_outcome(cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("automation create failed: {error:?}"))?;
    pump_until(cx, handle, app, "automation draft durable", |a| {
        !a.live_has_pending_receipts() && a.store.automations().len() == existing + 1
    })?;
    let id = cx
        .update(|cx| {
            app.read(cx)
                .store
                .automations()
                .iter()
                .map(|definition| definition.id)
                .max()
        })
        .ok_or("automation draft id missing")?;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.enable_automation(cx));
    })
    .map_err(|error| format!("automation enable failed: {error:?}"))?;
    pump_until(cx, handle, app, "automation enabled", |a| {
        !a.live_has_pending_receipts()
            && a.store
                .automation(id)
                .is_some_and(|definition| definition.status == crate::automation::DefinitionStatus::Enabled)
    })?;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.pause_automation_ui(id, cx));
    })
    .map_err(|error| format!("automation pause failed: {error:?}"))?;
    pump_until(cx, handle, app, "automation paused", |a| {
        !a.live_has_pending_receipts()
            && a.store
                .automation(id)
                .is_some_and(|definition| definition.status == crate::automation::DefinitionStatus::Paused)
    })?;
    Ok(id)
}

fn semantic_automation_view(store: &Store) -> Result<serde_json::Value, String> {
    let definitions = store
        .automations()
        .iter()
        .map(|definition| serde_json::to_value(definition).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, String>>()?;
    let occurrences = store
        .all_occurrences()
        .iter()
        .map(|occurrence| {
            serde_json::json!({
                "id": occurrence.id,
                "automation_id": occurrence.automation_id,
                "status": occurrence.status,
                "trigger_key": occurrence.trigger_key,
                "reference_time": occurrence.reference_time,
                "scheduled_for": occurrence.scheduled_for,
                "run_ids": occurrence.run_ids,
                "final_run_id": occurrence.final_run_id,
                "unread": occurrence.unread,
            })
        })
        .collect::<Vec<_>>();
    let runs = store
        .runs()
        .iter()
        .map(|run| {
            serde_json::json!({
                "id": run.id,
                "task_id": run.task_id,
                "occurrence_id": run.automation_occurrence_id,
                "status": run.status,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "definitions": definitions,
        "occurrences": occurrences,
        "runs": runs,
        "next_id": store.next_id(),
        "run_id_floor": store.run_id_floor(),
    }))
}

fn body_bytes_audit(path: &Path) -> Result<(usize, String), String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| error.to_string())?;
    sql_hash(&conn, "SELECT run_id,body FROM run_bodies ORDER BY run_id")
}

fn t04_automation_lifecycle_undo(root: &Path) -> Result<(), String> {
    let (path, task) = seed_live_workspace_with_history(root)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let original = cx.update(|cx| {
        let task = app.read(cx).store.task(task).unwrap();
        (
            task.title.clone(),
            task.stage,
            task.labels.clone(),
            task.approved_plan.clone(),
        )
    });
    let seeded = cx.update(|cx| automation_snapshot(&app.read(cx).store));
    if seeded.1.is_empty() || seeded.2.is_empty() {
        return Err("T04 fixture did not seed occurrence/run history".into());
    }

    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TaskTitle {
                    task,
                    title: "Scoped edit before automations".into(),
                },
                "Task renamed",
                cx,
            );
        });
    })
    .map_err(|error| format!("scoped edit failed: {error:?}"))?;
    finish_pending(&mut cx, handle, &app, "scoped edit durable")?;

    let new_id = prepare_paused_automation(&mut cx, handle, &app, "T04 new quick automation")?;
    if new_id == 1 {
        return Err("T04 new automation reused the seeded historic id".into());
    }
    let memory_before = cx.update(|cx| automation_snapshot(&app.read(cx).store));
    if memory_before.1.len() != seeded.1.len() || memory_before.2.len() != seeded.2.len() {
        return Err("enabling a paused automation changed seeded occurrence/run history".into());
    }
    let memory_semantics =
        cx.update(|cx| semantic_automation_view(&app.read(cx).store))?;
    let disk_before_store = read_v4_store(root, &path)?;
    let disk_before = automation_snapshot(&disk_before_store);
    let disk_semantics = semantic_automation_view(&disk_before_store)?;
    if memory_semantics != disk_semantics {
        return Err("memory and reopened disk disagree before Undo".into());
    }
    let (bodies_before, body_hash_before) = body_bytes_audit(&path)?;
    if bodies_before == 0 || body_hash_before.is_empty() {
        return Err("T04 fixture seeded no run body bytes".into());
    }

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.undo_change(w, cx));
    })
    .map_err(|error| format!("ordinary undo failed: {error:?}"))?;
    finish_pending(&mut cx, handle, &app, "ordinary undo durable")?;

    let (title, owned, memory_after) = cx.update(|cx| {
        let a = app.read(cx);
        let task = a.store.task(task).unwrap();
        (
            task.title.clone(),
            (
                task.stage,
                task.labels.clone(),
                task.approved_plan.clone(),
            ),
            automation_snapshot(&a.store),
        )
    });
    if title != original.0 {
        return Err(format!("ordinary edit was not restored: {title:?}"));
    }
    if owned != (original.1, original.2.clone(), original.3.clone()) {
        return Err("ordinary Undo changed the task's owned fields".into());
    }
    if memory_after != memory_before {
        return Err(
            "ordinary Undo changed automation definitions, occurrences, runs or ID floors in memory"
                .into(),
        );
    }
    let disk_after_store = read_v4_store(root, &path)?;
    let disk_after = automation_snapshot(&disk_after_store);
    if disk_after != disk_before {
        return Err("ordinary Undo changed automation/history/runtime state on disk".into());
    }
    let disk_task = disk_after_store
        .task(task)
        .ok_or("task missing from reopened disk")?;
    if disk_task.title != original.0
        || disk_task.stage != original.1
        || disk_task.labels != original.2
        || disk_task.approved_plan != original.3
    {
        return Err("reopened disk task fields were not restored by ordinary Undo".into());
    }
    let disk_semantics_after = semantic_automation_view(&disk_after_store)?;
    if disk_semantics_after != disk_semantics {
        return Err("reopened disk semantics changed across ordinary Undo".into());
    }
    let (bodies_after, body_hash_after) = body_bytes_audit(&path)?;
    if bodies_after != bodies_before || body_hash_after != body_hash_before {
        return Err(format!(
            "ordinary Undo rewrote run body bytes: {bodies_before}/{body_hash_before} -> {bodies_after}/{body_hash_after}"
        ));
    }
    let paused = disk_after_store
        .automation(new_id)
        .ok_or("new automation missing from reopened disk")?;
    if paused.status != crate::automation::DefinitionStatus::Paused {
        return Err(format!("new automation status changed: {:?}", paused.status));
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}

fn assert_history_seeded(path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| error.to_string())?;
    for (table, sql) in [
        (
            "automation_occurrences",
            "SELECT COUNT(*) FROM automation_occurrences",
        ),
        ("runs", "SELECT COUNT(*) FROM runs"),
        ("run_bodies", "SELECT COUNT(*) FROM run_bodies WHERE length(body)>0"),
        (
            "occurrence_snapshots",
            "SELECT COUNT(*) FROM occurrence_snapshots",
        ),
        ("tasks", "SELECT COUNT(*) FROM tasks"),
    ] {
        let count: i64 = conn
            .query_row(sql, [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err(format!("T15 fixture seeded no {table} rows to audit"));
        }
    }
    Ok(())
}

fn sql_hash(conn: &rusqlite::Connection, sql: &str) -> Result<(usize, String), String> {
    use sha2::{Digest, Sha256};
    let mut statement = conn.prepare(sql).map_err(|error| error.to_string())?;
    let column_count = statement.column_count();
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        count += 1;
        for index in 0..column_count {
            let value: rusqlite::types::Value = row.get(index).map_err(|error| error.to_string())?;
            hash.update(format!("{value:?}\x1f"));
        }
        hash.update(b"\n");
    }
    Ok((count, format!("{:x}", hash.finalize())))
}

fn sql_audit(path: &Path) -> Result<serde_json::Value, String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| error.to_string())?;
    let tables = [
        ("automations", "SELECT id,status,updated_at,payload FROM automations ORDER BY id"),
        ("occurrences", "SELECT id,automation_id,status,unread,trigger_key,snapshot_hash,payload FROM automation_occurrences ORDER BY id"),
        ("runs", "SELECT id,task_id,occurrence_id,status,has_body,body_bytes,payload FROM runs ORDER BY id"),
        ("run_bodies", "SELECT run_id,body FROM run_bodies ORDER BY run_id"),
        ("occurrence_snapshots", "SELECT hash,payload,refs FROM occurrence_snapshots ORDER BY hash"),
        ("tasks", "SELECT id,stage,payload FROM tasks ORDER BY id"),
    ];
    let mut out = serde_json::Map::new();
    for (name, sql) in tables {
        let (count, hash) = sql_hash(&conn, sql)?;
        out.insert(name.into(), serde_json::json!({"rows": count, "sha256": hash}));
    }
    Ok(serde_json::Value::Object(out))
}

fn seed_live_workspace_with_history(root: &Path) -> Result<(PathBuf, u64), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    store.research_config_mut().models = [
        "fixture/planner",
        "fixture/collector",
        "fixture/analyst",
        "fixture/adviser",
    ]
    .map(String::from);
    let task = updated_task(&mut store);
    let template = crate::automation_execution::build_template("Historic automation", "One report")
        .map_err(|error| error.to_string())?;
    let automation = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "Historic seeded automation".into(),
            "Historic automation".into(),
            "One report".into(),
            crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            crate::automation::AutomationPolicy::default(),
            Some(template.clone()),
            chrono::Utc::now().timestamp(),
        )
        .map_err(|error| error.to_string())?;
    let occurrence = crate::automation::AutomationOccurrence {
        id: 1,
        automation_id: automation,
        definition_version: 1,
        source: crate::automation::TriggerSource::Scheduled,
        trigger_key: "historic-seeded-key".into(),
        scheduled_for: Some(1),
        requested_at: None,
        actual_started_at: Some(2),
        timezone: "UTC".into(),
        reference_time: 1,
        data_window: crate::automation::TimeWindow::unresolved("seeded"),
        snapshot: crate::automation::AutomationOccurrenceSnapshot {
            objective: "Historic automation".into(),
            expected_result: "One report".into(),
            shape: Some(crate::automation::AutomationShape::Research),
            template,
            models: std::array::from_fn(|_| "fixture/model".to_string()),
            agents: Default::default(),
            materials: Vec::new(),
            policy: crate::automation::AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "seeded".into(),
        },
        status: crate::automation::OccurrenceStatus::Succeeded,
        reason: None,
        research_goal_id: None,
        research_goal_ids: Vec::new(),
        research_topic_id: None,
        run_ids: vec![1],
        final_run_id: Some(1),
        unread: false,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: 1,
        deadline_at: None,
        retry_requested_at: None,
    };
    store.push_occurrence(occurrence);
    store.push_run(AgentRun {
        id: 1,
        task_id: task,
        task_title: "Historic run".into(),
        directory: root.to_string_lossy().into_owned(),
        prompt: "Historic seeded body".into(),
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
        output: "Historic report body".into(),
        attachments: Vec::new(),
        research_kind: None,
        sources: Vec::new(),
        input_signature: String::new(),
        automation_occurrence_id: Some(1),
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
    });
    let path = seed_store(root, &store)?;
    Ok((path, task))
}

fn install_write_audit(path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(path).map_err(|error| error.to_string())?;
    let mut sql = String::from(
        "CREATE TABLE qa_write_audit (seq INTEGER PRIMARY KEY AUTOINCREMENT, table_name TEXT NOT NULL, op TEXT NOT NULL, row_id TEXT);",
    );
    let specs = [
        ("automations", "id", "NEW.id", "OLD.id"),
        ("automation_occurrences", "id", "NEW.id", "OLD.id"),
        ("runs", "id", "NEW.id", "OLD.id"),
        ("run_bodies", "run_id", "NEW.run_id", "OLD.run_id"),
        ("occurrence_snapshots", "hash", "NEW.hash", "OLD.hash"),
        ("tasks", "id", "NEW.id", "OLD.id"),
        ("meta", "key", "NEW.key", "OLD.key"),
    ];
    for (table, _column, new_id, old_id) in specs {
        sql.push_str(&format!(
            "CREATE TRIGGER qa_{table}_ins AFTER INSERT ON {table} BEGIN INSERT INTO qa_write_audit(table_name,op,row_id) VALUES('{table}','INSERT',{new_id}); END;\n"
        ));
        sql.push_str(&format!(
            "CREATE TRIGGER qa_{table}_upd AFTER UPDATE ON {table} BEGIN INSERT INTO qa_write_audit(table_name,op,row_id) VALUES('{table}','UPDATE',{new_id}); END;\n"
        ));
        sql.push_str(&format!(
            "CREATE TRIGGER qa_{table}_del AFTER DELETE ON {table} BEGIN INSERT INTO qa_write_audit(table_name,op,row_id) VALUES('{table}','DELETE',{old_id}); END;\n"
        ));
    }
    conn.execute_batch(&sql).map_err(|error| error.to_string())
}

fn audit_marker(path: &Path) -> Result<i64, String> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    conn.query_row("SELECT COALESCE(MAX(seq),0) FROM qa_write_audit", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn audit_since(path: &Path, marker: i64) -> Result<Vec<(String, String, String)>, String> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let mut statement = conn
        .prepare("SELECT table_name,op,row_id FROM qa_write_audit WHERE seq>?1 ORDER BY seq")
        .map_err(|error| error.to_string())?;
    let mut rows = statement.query([marker]).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        out.push((row.get(0).map_err(|e| e.to_string())?, row.get(1).map_err(|e| e.to_string())?, row.get(2).map_err(|e| e.to_string())?));
    }
    Ok(out)
}

fn assert_exact_writes(
    writes: &[(String, String, String)],
    expected: &[(&str, &str, u64)],
    label: &str,
) -> Result<(), String> {
    if writes.len() != expected.len() {
        return Err(format!(
            "{label} wrote {} rows, expected {}: {:?}",
            writes.len(),
            expected.len(),
            writes
        ));
    }
    for ((table, op, row), (want_table, want_op, want_row)) in writes.iter().zip(expected) {
        if table != want_table || op != want_op || row != &want_row.to_string() {
            return Err(format!(
                "{label} wrote {table}/{op}/{row}, expected {want_table}/{want_op}/{want_row}"
            ));
        }
    }
    Ok(())
}

fn audit_summary(writes: &[(String, String, String)]) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (table, _, _) in writes {
        *counts.entry(table.as_str()).or_default() += 1;
    }
    if counts.is_empty() {
        return "none".into();
    }
    counts
        .iter()
        .map(|(table, count)| format!("{table}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn assert_no_history_writes(writes: &[(String, String, String)], label: &str) -> Result<(), String> {
    for (table, op, row) in writes {
        if matches!(
            table.as_str(),
            "automation_occurrences" | "runs" | "run_bodies" | "occurrence_snapshots"
        ) {
            return Err(format!("{label} rewrote historical {table} row {row} via {op}"));
        }
    }
    Ok(())
}

fn t15_incremental_sql_audit(root: &Path) -> Result<(), String> {
    let (path, task) = seed_live_workspace_with_history(root)?;
    install_write_audit(&path)?;
    assert_history_seeded(&path)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    let id = prepare_paused_automation(&mut cx, handle, &app, "T15 audit check")?;
    let original = cx.update(|cx| {
        let task = app.read(cx).store.task(task).unwrap();
        (task.title.clone(), task.stage, task.labels.clone(), task.approved_plan.clone())
    });

    let marker = audit_marker(&path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.edit_automation(id, w, cx);
            a.automation_ui.name.update(cx, |state, cx| {
                state.set_value("Renamed automation audit", w, cx);
            });
            let _ = a.save_automation_draft_outcome(cx);
        });
        w.render_frame(cx);
    })
    .map_err(|error| format!("automation rename failed: {error:?}"))?;
    pump_until(&mut cx, handle, &app, "rename durable", |a| {
        !a.live_has_pending_receipts()
            && a.store
                .automation(id)
                .is_some_and(|definition| definition.name == "Renamed automation audit")
    })?;
    let rename_writes = audit_since(&path, marker)?;
    println!("LIVE_AUDIT t15 rename writes={}", audit_summary(&rename_writes));
    assert_no_history_writes(&rename_writes, "automation rename")?;
    assert_exact_writes(
        &rename_writes,
        &[("automations", "UPDATE", id)],
        "automation rename",
    )?;

    let marker = audit_marker(&path)?;
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TaskTitle {
                    task,
                    title: "T15 undo audit".into(),
                },
                "Task renamed",
                cx,
            );
        });
    })
    .map_err(|error| format!("scoped edit for audit failed: {error:?}"))?;
    finish_pending(&mut cx, handle, &app, "audit edit durable")?;
    let edit_writes = audit_since(&path, marker)?;
    println!("LIVE_AUDIT t15 edit writes={}", audit_summary(&edit_writes));
    assert_no_history_writes(&edit_writes, "scoped task edit")?;
    assert_exact_writes(&edit_writes, &[("tasks", "UPDATE", task)], "scoped task edit")?;

    let marker = audit_marker(&path)?;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.undo_change(w, cx));
    })
    .map_err(|error| format!("scoped undo for audit failed: {error:?}"))?;
    finish_pending(&mut cx, handle, &app, "audit undo durable")?;
    let undo_writes = audit_since(&path, marker)?;
    println!("LIVE_AUDIT t15 undo writes={}", audit_summary(&undo_writes));
    assert_no_history_writes(&undo_writes, "scoped Undo")?;
    assert_exact_writes(&undo_writes, &[("tasks", "UPDATE", task)], "scoped Undo")?;
    let after = cx.update(|cx| {
        let task = app.read(cx).store.task(task).unwrap();
        (task.title.clone(), task.stage, task.labels.clone(), task.approved_plan.clone())
    });
    if after != original {
        return Err(format!(
            "scoped Undo did not restore owned task fields: before={original:?} after={after:?}"
        ));
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

/// Exercise real macOS hide/activate APIs; the headless platform cannot hide.
fn window_reopen(_root: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    // NSApplication termination exits the process, so this native case cannot
    // share the sequential suite's process with the remaining headless cases.
    if std::env::var_os(SEQUENTIAL_ENV).is_some() {
        let status = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--qa-live-persistence")
            .env(CASE_ENV, "window-reopen")
            .env_remove(SEQUENTIAL_ENV)
            .status()
            .map_err(|e| e.to_string())?;
        return if status.success() { Ok(()) } else { Err(format!("native window QA failed: {status}")) };
    }
    application().with_assets(crate::ui_assets::AppAssets).run(|cx| {
        init(cx);
        let (handle, app) = open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None, size(px(1000.), px(680.)), cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Window reopen QA — isolated fixture".into()),
                ..Default::default()
            }),
            ..Default::default()
        }, cx, |w, cx| cx.new(|cx| TaskApp::new_mock(w, cx))).unwrap();
        cx.spawn(async move |cx| {
            for deferred in [false, true, false] {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                cx.update_window(handle.into(), |_, w, cx| {
                    app.update(cx, |a, cx| {
                        assert!(a.can_close_window(cx));
                        if deferred {
                            a.window_close_ready = true;
                            cx.notify();
                        }
                    });
                    if !deferred {
                        crate::close_workspace_window(w, cx);
                    }
                }).expect("close must retain the window");
                cx.background_executor().timer(Duration::from_millis(150)).await;
                cx.update(|cx| {
                    crate::reopen_application(cx);
                    assert_eq!(cx.windows().len(), 1, "reopen must not duplicate windows");
                });
                cx.update_window(handle.into(), |_, _, cx| {
                    assert!(!app.read(cx).window_close_ready, "deferred close must finish");
                }).expect("reopen must retain the original workspace");
            }
            println!("WINDOW_REOPEN_QA native close/reopen cycles passed");
            cx.update(|cx| cx.quit());
        }).detach();
    });
    Ok(())
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

fn lazy_research_queue(root: &Path, mode: &str) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let mut store = Store::empty();
    store.set_default_model("fixture/model".into());
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    let goal = store.create_research_goal("Lazy queue".into(), "Reuse saved evidence".into(), 1)?;
    let plan = r#"{"tasks":[{"title":"Collect","instructions":"Find evidence","kind":"collect","depends_on":[]},{"title":"Collect more","instructions":"Extend evidence","kind":"collect","depends_on":[0]},{"title":"Analyze","instructions":"Use both results","kind":"analyze","depends_on":[0,1]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[2]}]}"#;
    let steps = store.apply_research_plan(goal, plan)?;
    let mut saved = Vec::new();
    for step in &steps[..2] {
        let mut run = store.prepare_run(*step)?;
        run.status = "Succeeded".into();
        run.output = format!("Saved evidence for step {step}");
        saved.push((run.id, run.output.clone()));
        store.push_run(run);
        store.set_stage(*step, Stage::Done)?;
    }
    let path = seed_store(root, &store)?;
    let (mut cx, handle, app) = open_live(root, &path)?;
    cx.update(|cx| app.update(cx, |a, cx| {
        a.qa_dispatch_fake = Some(crate::dispatch_admission::FakeDispatchRunner::default());
        assert!(saved.iter().all(|(id, _)| a.store.run_body_is_lazy(*id)));
        assert!(a.store.ready_research_tasks(goal).unwrap().is_empty());
        if mode == "retry" {
            a.run_body_state.insert(saved[0].0, crate::history_reader::BodyLoad::Failed {
                error: "Fixture read failure".into(),
                retry_at: Instant::now() + Duration::from_secs(60),
            });
        }
        a.start_goal_batch(goal, cx);
        if mode == "retry" {
            assert!(matches!(&a.todo_batch.as_ref().unwrap().state,
                crate::batch::BatchState::Paused(reason) if reason.contains("Fixture read failure")));
            assert_eq!(a.store.runs().len(), saved.len());
            a.resume_todo_batch(cx);
        }
        assert_eq!(a.todo_batch.as_ref().unwrap().state, crate::batch::BatchState::Running);
        assert_eq!(a.store.runs().len(), saved.len(), "no launch before hydration");
        assert!(a.history_needs_pump(), "body loading must keep the runtime alive");
    }));
    if mode == "pause" {
        cx.update(|cx| app.update(cx, |a, cx| a.pause_todo_batch(cx)));
        pump_until(&mut cx, handle, &app, "paused body reads", |a| {
            saved.iter().all(|(id, _)| !a.store.run_body_is_lazy(*id))
        })?;
        cx.update(|cx| app.update(cx, |a, cx| {
            assert!(matches!(a.todo_batch.as_ref().unwrap().state, crate::batch::BatchState::Paused(_)));
            assert_eq!(a.store.runs().len(), saved.len(), "hydration must respect pause");
            a.resume_todo_batch(cx);
        }));
    }
    advance_until(&mut cx, &app, "lazy research admission", |a| {
        a.qa_dispatch_fake.as_ref().is_some_and(|fake| !fake.starts.is_empty())
    })?;
    cx.update(|cx| {
        let a = app.read(cx);
        assert_eq!(a.qa_dispatch_fake.as_ref().unwrap().starts.len(), 1);
        assert_eq!(a.store.runs().len(), saved.len() + 1, "completed steps must not rerun");
        let run = a.store.runs().last().unwrap();
        assert_eq!(run.task_id, steps[2]);
        for (id, body) in &saved {
            assert!(!a.store.run_body_is_lazy(*id));
            assert!(run.prompt.contains(body), "dependent prompt must include saved evidence");
        }
        assert_eq!(a.todo_batch.as_ref().unwrap().state, crate::batch::BatchState::Running);
    });
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    Ok(())
}
