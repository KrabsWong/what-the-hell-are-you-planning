//! T14 isolated QA for the live `Command::SetRead` direct path.
//!
//! Verifies that the narrow live path preserves pre-existing runtime dirt,
//! rejects missing ids and a busy primary slot without mutation, keeps no Undo
//! entry, survives an injected write failure followed by retry, and reloads the
//! exact per-id read state from disk.

use crate::app::TaskApp;
use crate::automation::{
    AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    OccurrenceStatus, TimeWindow, TriggerSource,
};
use crate::automation_schedule::{DataWindow, Schedule, ScheduleSpec};
use crate::domain::Store;
use crate::command_app::CommandRun;
use crate::domain_command::Command;
use crate::persistence::Storage;
use crate::persistence_live;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INCARNATION: u64 = 0x5E_7A_11_20;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".local/set-read-qa")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&root);
    let result: Result<(), String> = (|| {
        guards_and_prior_dirt(&root.join("guards"))?;
        sync_enqueue_retry(&root.join("sync-enqueue"))?;
        failure_retry_restart(&root.join("failure"))?;
        Ok(())
    })();
    match result {
        Ok(()) => println!("SET_READ_QA status=ok root={}", root.display()),
        Err(error) => {
            eprintln!("SET_READ_QA status=failed error={error}");
            std::process::exit(1);
        }
    }
}

fn occurrence(id: u64, unread: bool, automation: u64) -> AutomationOccurrence {
    let at = 1_700_000_000 + id as i64;
    AutomationOccurrence {
        id,
        automation_id: automation,
        definition_version: 1,
        source: TriggerSource::Scheduled,
        trigger_key: format!("set-read-{id}"),
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
            objective: "Set-read QA".into(),
            expected_result: "Exact read state".into(),
            shape: Some(AutomationShape::Research),
            template: crate::automation::AutomationTemplate {
                goals: Vec::new(),
                window_rule: Default::default(),
            },
            models: std::array::from_fn(|_| "qa/model".to_string()),
            agents: Default::default(),
            materials: Vec::new(),
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "set-read-qa".into(),
        },
        status: OccurrenceStatus::Succeeded,
        reason: None,
        research_goal_id: None,
        research_goal_ids: Vec::new(),
        research_topic_id: None,
        run_ids: Vec::new(),
        final_run_id: None,
        unread,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: at,
        deadline_at: None,
        retry_requested_at: None,
    }
}

fn fixture(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let template = crate::automation_execution::build_template("Set read QA", "Exact read state")
        .map_err(|error| error.to_string())?;
    let mut temp = Store::empty();
    temp.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    temp.set_default_model("qa/model".into());
    temp.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
    let automation = temp
        .create_automation_draft(
            1,
            AutomationShape::Research,
            "Set read QA".into(),
            "Set read QA".into(),
            "Exact read state".into(),
            Schedule {
                spec: ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            AutomationPolicy::default(),
            Some(template),
            chrono::Utc::now().timestamp(),
        )
        .map_err(|error| error.to_string())?;
    let definition = temp
        .automation(automation)
        .ok_or("set-read automation fixture missing")?
        .clone();
    let mut store = Store::empty()
        .with_loaded_tracked(
            Vec::new(),
            vec![definition],
            vec![
                occurrence(1, false, automation),
                occurrence(2, true, automation),
                occurrence(3, false, automation),
            ],
        )
        .map_err(|(_, error)| error)?;
    store.project_mut(1).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("qa/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "qa/model".into());
    let path = root.join("workspace.sqlite3");
    crate::sqlite_v4::SqliteV4::create_seeded(&path, root, INCARNATION, &store)
        .map_err(|error| error.to_string())?;
    Ok((root.to_path_buf(), path))
}

fn open_live(
    root: &Path,
    path: &Path,
) -> Result<(HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>), String> {
    let storage = Storage::new(path.to_path_buf());
    let _ = storage.upgrade_paths(|_| {});
    let lock = storage.acquire_lock()?;
    let mut live = persistence_live::select_existing(&storage)?
        .ok_or("set-read QA did not select the seeded workspace")?;
    let store = live.migrated_store.take();
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
                a.attach_loaded_storage_live(storage, Ok((lock, store)), Some(live.clone()), w, cx);
                assert!(a.live_required, "set-read QA must require live persistence");
                assert!(
                    a.live_persistence_active(),
                    "set-read QA must attach the ordered worker"
                );
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .map_err(|error| format!("set-read open window failed: {error:?}"))?;
    let app = app.ok_or("set-read app missing")?;
    let handle: AnyWindowHandle = window.into();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let ready = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).live_persistence_active()
                    && !app.read(cx).live_has_pending_receipts()
            })
            .map_err(|error| format!("set-read live pump failed: {error:?}"))?;
        if ready {
            return Ok((cx, handle, app));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("set-read live fixture did not become ready".into())
}

fn read_disk(root: &Path, path: &Path) -> Result<Store, String> {
    crate::sqlite_v4::SqliteV4::open(path, root, INCARNATION)?.load_store()
}

fn pump_clean(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    label: &str,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                !app.read(cx).live_has_pending_receipts() && app.read(cx).live_failure.is_none()
            })
            .map_err(|error| format!("{label} pump failed: {error:?}"))?;
        if done {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!("timed out waiting for {label}"))
}

fn execute_set_read(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    occurrence: u64,
    unread: bool,
) -> Result<CommandRun, String> {
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.execute_command(Command::SetRead { occurrence, unread }, cx)
        })
    })
    .map_err(|error| format!("set-read command failed: {error:?}"))?
}

fn occurrence_value(store: &Store, id: u64) -> Result<bool, String> {
    store
        .occurrence(id)
        .map(|value| value.unread)
        .ok_or_else(|| format!("occurrence {id} missing"))
}

fn guards_and_prior_dirt(root: &Path) -> Result<(), String> {
    let (root, path) = fixture(root)?;
    let (mut cx, handle, app) = open_live(&root, &path)?;
    let undo_before = cx.update(|cx| app.read(cx).undo.len());
    let occurrence_count = cx.update(|cx| app.read(cx).store.all_occurrences().len());
    if occurrence_count != 3 {
        return Err(format!("expected 3 occurrences, got {occurrence_count}"));
    }

    let missing = execute_set_read(&mut cx, handle, &app, 999, true);
    if missing.is_ok() {
        return Err("missing occurrence id was accepted".into());
    }
    let epoch_before_missing = cx.update(|cx| app.read(cx).store.dirty_epoch());
    let missing_again = execute_set_read(&mut cx, handle, &app, 999, true);
    if missing_again.is_ok() {
        return Err("missing occurrence id was accepted on retry".into());
    }
    let epoch_after_missing = cx.update(|cx| app.read(cx).store.dirty_epoch());
    if epoch_after_missing != epoch_before_missing {
        return Err("missing occurrence id advanced the dirty epoch".into());
    }

    let busy = cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.live_primary = Some(crate::live_save_app::LivePending {
                generation: 0,
                epoch: a.store.dirty_epoch(),
                workspace: a.live_workspace.unwrap_or(0),
                durable: false,
                completion: crate::live_save_app::LiveCompletion::None,
            });
            a.execute_command(
                Command::SetRead {
                    occurrence: 2,
                    unread: false,
                },
                cx,
            )
        })
    })
    .map_err(|error| format!("busy set-read failed: {error:?}"))?;
    if busy.is_ok() {
        return Err("busy primary slot accepted a read-state command".into());
    }
    let busy_value = cx.update(|cx| app.read(cx).store.occurrence(2).map(|v| v.unread));
    if busy_value != Some(true) {
        return Err(format!(
            "busy rejection mutated occurrence 2: {busy_value:?}"
        ));
    }
    cx.update(|cx| {
        app.update(cx, |a, _| a.live_primary = None);
    });

    // Pre-existing runtime dirt: a direct runtime read-state change that has not
    // been submitted yet must survive the SetRead delta instead of being
    // overwritten by a stale staged snapshot.
    cx.update(|cx| {
        app.update(cx, |a, _| {
            if let Some(mut value) = a.store.occurrence_mut(3) {
                value.unread = true;
            }
        });
    });
    let direct = execute_set_read(&mut cx, handle, &app, 1, true)?;
    if !matches!(direct, CommandRun::Saving { .. } | CommandRun::Durable(_)) {
        return Err("set-read command did not report a durable or pending run".into());
    }
    pump_clean(&mut cx, handle, &app, "guards set-read durable")?;

    let undo_after = cx.update(|cx| app.read(cx).undo.len());
    if undo_after != undo_before {
        return Err(format!(
            "read-state command pushed Undo: before={undo_before} after={undo_after}"
        ));
    }
    let disk = read_disk(&root, &path)?;
    if disk.all_occurrences().len() != 3 {
        return Err("guards disk occurrence count changed".into());
    }
    for (id, expected) in [(1, true), (2, true), (3, true)] {
        if occurrence_value(&disk, id)? != expected {
            return Err(format!(
                "guards disk occurrence {id} has wrong unread state"
            ));
        }
    }
    // Equal in-memory value is still a tracked write: it must submit existing
    // dirty runtime data rather than claim immediate durability.
    cx.update(|cx| {
        app.update(cx, |a, _| {
            if let Some(mut value) = a.store.occurrence_mut(3) {
                value.skipped_count = 7;
            }
        });
    });
    let equal = execute_set_read(&mut cx, handle, &app, 2, true)?;
    if !matches!(equal, CommandRun::Saving { .. }) {
        return Err("equal-value set-read did not submit its pending delta".into());
    }
    pump_clean(&mut cx, handle, &app, "equal-value set-read durable")?;
    let equal_disk = read_disk(&root, &path)?;
    if occurrence_value(&equal_disk, 2)? != true {
        return Err("equal-value set-read lost occurrence 2 read state".into());
    }
    if equal_disk.occurrence(3).map(|value| value.skipped_count) != Some(7) {
        return Err("equal-value set-read skipped prior dirty runtime data".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    println!("SET_READ_QA case=guards-and-prior-dirt status=ok");
    Ok(())
}


fn sync_enqueue_retry(root: &Path) -> Result<(), String> {
    let (root, path) = fixture(root)?;
    let (mut cx, handle, app) = open_live(&root, &path)?;
    // Hold the worker and fill its bounded submission channel. The next
    // auxiliary group is retained by the adapter, so a following SetRead is
    // rejected synchronously even though no primary receipt exists yet.
    cx.update(|cx| {
        let _ = app.update(cx, |a, _| {
            if let Some(adapter) = a.dispatch_admission.as_ref() {
                adapter.set_hold(true);
            }
        });
    });
    for index in 0..6u64 {
        cx.update(|cx| {
            app.update(cx, |a, _| {
                a.store.set_last_active_project(Some(index + 1));
            });
        });
        let submitted = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.submit_live_auxiliary(crate::live_save_app::LiveCompletion::None, cx)
                })
            })
            .map_err(|error| format!("sync enqueue fill failed: {error:?}"))?;
        if let Err(error) = submitted {
            if error.contains("retained") {
                break;
            }
            return Err(format!("sync enqueue fill rejected: {error}"));
        }
    }
    let epoch_before = cx.update(|cx| app.read(cx).store.dirty_epoch());
    let rejected = execute_set_read(&mut cx, handle, &app, 1, true);
    let (value, primary, epoch_after) = cx.update(|cx| {
        let a = app.read(cx);
        (
            a.store.occurrence(1).map(|value| value.unread),
            a.live_primary.is_some(),
            a.store.dirty_epoch(),
        )
    });
    if rejected.is_ok() {
        return Err("sync enqueue rejection was not surfaced".into());
    }
    if value != Some(true) {
        return Err("sync enqueue rejection rolled back the requested read state".into());
    }
    if !primary {
        return Err("sync enqueue rejection dropped the retained primary obligation".into());
    }
    if epoch_after <= epoch_before {
        return Err("sync enqueue rejection lost the tracked read mutation".into());
    }
    cx.update(|cx| {
        let _ = app.update(cx, |a, _| {
            if let Some(adapter) = a.dispatch_admission.as_ref() {
                adapter.set_hold(false);
            }
        });
    });
    // Let the retained/backpressured groups drain, then retry the primary.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let pending = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx)
                    .dispatch_admission
                    .as_ref()
                    .is_some_and(|adapter| adapter.live_pending())
            })
            .map_err(|error| format!("sync enqueue drain poll failed: {error:?}"))?;
        if !pending {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.update(|cx| {
        app.update(cx, |a, cx| a.retry_live_persistence(cx));
    });
    pump_clean(&mut cx, handle, &app, "sync enqueue retry durable")?;
    let disk = read_disk(&root, &path)?;
    if occurrence_value(&disk, 1)? != true {
        return Err("sync enqueue retry did not persist the requested read state".into());
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    println!("SET_READ_QA case=sync-enqueue-retry status=ok");
    Ok(())
}

fn failure_retry_restart(root: &Path) -> Result<(), String> {
    let (root, path) = fixture(root)?;
    let (mut cx, handle, app) = open_live(&root, &path)?;
    let undo_before = cx.update(|cx| app.read(cx).undo.len());
    let fault = cx.update(|cx| {
        app.update(cx, |a, _| {
            a.dispatch_admission
                .as_mut()
                .ok_or("missing dispatch admission")?
                .set_fault(crate::sqlite_v4::Fault::BeforeCommit)
        })
    });
    if let Err(error) = fault {
        return Err(format!("set write fault failed: {error}"));
    }
    let run = execute_set_read(&mut cx, handle, &app, 1, true)?;
    if !matches!(run, CommandRun::Saving { .. } | CommandRun::Durable(_)) {
        return Err("faulted set-read was rejected before submitting".into());
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut failed = false;
    while Instant::now() < deadline {
        failed = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                app.read(cx).live_failure.is_some()
            })
            .map_err(|error| format!("set-read failure poll failed: {error:?}"))?;
        if failed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !failed {
        return Err("injected set-read write failure was not reported".into());
    }
    let disk_failed = read_disk(&root, &path)?;
    if occurrence_value(&disk_failed, 1)? {
        return Err("failed set-read write still reached disk".into());
    }
    let undo_failed = cx.update(|cx| app.read(cx).undo.len());
    if undo_failed != undo_before {
        return Err("failed set-read pushed Undo".into());
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
        return Err(format!("clear set-read fault failed: {error}"));
    }
    pump_clean(&mut cx, handle, &app, "set-read retry durable")?;

    let disk = read_disk(&root, &path)?;
    if occurrence_value(&disk, 1)? != true {
        return Err("retry did not persist occurrence 1".into());
    }
    if occurrence_value(&disk, 2)? != true || occurrence_value(&disk, 3)? != false {
        return Err("retry changed an unrelated occurrence".into());
    }
    let undo_after = cx.update(|cx| app.read(cx).undo.len());
    if undo_after != undo_before {
        return Err("successful set-read pushed Undo".into());
    }

    // A fresh v4 load is the restart boundary: no UI snapshot is involved.
    let restarted = read_disk(&root, &path)?;
    for (id, expected) in [(1, true), (2, true), (3, false)] {
        if occurrence_value(&restarted, id)? != expected {
            return Err(format!("restart lost occurrence {id} read state"));
        }
    }
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
    println!("SET_READ_QA case=failure-retry-restart status=ok");
    Ok(())
}
