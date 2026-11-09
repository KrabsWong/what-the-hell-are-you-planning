//! T14 L1b app-level durable dispatch admission QA.
//!
//! Uses the real `DispatchAdmissionAdapter` + `PersistenceWorker` + isolated
//! SQLite v4 database and a fake local launch recorder. No model, network or
//! production workspace is touched.

use crate::app::{LaunchIntent, PendingDispatch, PreparedRunDispatch, TaskApp};
use crate::automation::{
    AutomationDefinition, AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy,
    AutomationShape, AutomationTemplate, DefinitionStatus, OccurrenceStatus, TriggerSource,
    WindowRule,
};
use crate::automation_schedule::{Schedule, ScheduleSpec};
use crate::dispatch_admission::{AdmissionId, AdmissionUpdate, DispatchAdmissionAdapter};
use crate::domain_command::{Command, CommandClass};
use crate::persistence_worker::WorkerOptions;
use crate::dispatch_admission::FakeDispatchRunner;
use crate::domain::{AgentRun, Store};
use crate::persistence::Storage;
use crate::persistence_live;
use gpui_kit::test::TestWindowExt;
use gpui_kit::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WORKSPACE: u64 = 9_001;
const INCARNATION: u64 = 0x54_31_2B_10;
const FAKE_CLI: &str = r#"#!/usr/bin/env python3
import json, sys, time
args = sys.argv[1:]
if 'models' in args:
    print('fixture/model')
    sys.exit(0)
time.sleep(0.15)
print(json.dumps({'type': 'reasoning', 'sessionID': 'dispatch-fixture', 'part': {'text': 'fixture'}}), flush=True)
result = {'status': 'partial', 'report': '## Dispatch fixture\n\nSequential step completed.', 'sources': []}
print(json.dumps({'type': 'text', 'part': {'text': json.dumps(result)}}), flush=True)
"#;

fn write_fake_cli(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, FAKE_CLI).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Find evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;
const INDEPENDENT_PLAN: &str = r#"{"tasks":[{"title":"Collect A","instructions":"Find A","kind":"collect","depends_on":[]},{"title":"Collect B","instructions":"Find B","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze A","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[2]}]}"#;

fn run_root() -> PathBuf {
    std::env::current_dir()
        .unwrap()
        .join(format!(
            ".local/dispatch-admission-qa/{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
}

pub fn run() {
    let root = run_root();
    std::fs::create_dir_all(&root).unwrap();
    durable_before_launch(&root);
    idle_first_dispatch_keeps_runtime_pump(&root);
    executor_driven_three_step_dispatch(&root);
    cancel_before_receipt_never_launches(&root);
    cancel_after_receipt_before_claim_never_launches(&root);
    pending_admission_defers_fanout_and_cancel_cleans_binding(&root);
    parked_worker_quiesces_and_retry_resumes(&root);
    material_verification_wakes_and_settles(&root);
    render_wake_saves_idle_legacy_edit(&root);
    recover_after_ack_before_launch(&root);
    resolve_claimed_ambiguity(&root);
    rerun_resolution_admits_distinct_attempt(&root);
    persisted_status_gates_claim_cleanup(&root);
    reserved_dispatch_survives_prepare_failure(&root);
    backpressure_retains_intent(&root);
    println!("DISPATCH_ADMISSION_QA status=ok root={}", root.display());
}

fn fixture(root: &Path) -> (Store, u64) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("fixture/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    let goal = store
        .create_research_goal("Dispatch goal".into(), "Scope".into(), 1)
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    (store, steps[0])
}

fn queued_run(store: &Store, task: u64, id: u64) -> AgentRun {
    let mut run = store.prepare_run(task).unwrap();
    run.id = id;
    run.status = "Queued".into();
    run.queued_at = "2026-11-09T00:00:00Z".into();
    run.started_at.clear();
    run.ended_at = None;
    run.exit_code = None;
    run
}

struct AppHarness {
    cx: HeadlessAppContext,
    handle: AnyWindowHandle,
    app: Entity<TaskApp>,
    root: PathBuf,
    path: PathBuf,
    intent: PathBuf,
}

impl AppHarness {
    fn new(name: &str, store: Store) -> Self {
        Self::new_in(&run_root(), name, store)
    }

    fn new_in(base: &Path, name: &str, store: Store) -> Self {
        let root = base.join(format!("window-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("workspace.sqlite3");
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, WORKSPACE, &store).unwrap();
        let intent = root.join("dispatch-intent.json");
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
                view.update(cx, |a, _cx| {
                    a.store = store.clone();
                    a.last_saved = store.clone();
                    a.qa_dispatch_fake = Some(FakeDispatchRunner {
                        launch_log: Some(root.join("launches.log")),
                        ..Default::default()
                    });
                    a.attach_dispatch_admission(&path, &root, WORKSPACE, intent.clone())
                        .unwrap();
                });
                app = Some(view.clone());
                cx.new(|cx| base::Root::new(view, w, cx))
            })
            .unwrap();
        Self {
            cx,
            handle: window.into(),
            app: app.unwrap(),
            root,
            path,
            intent,
        }
    }

    fn existing(base: &Path, store: Store) -> Self {
        let root = base.to_path_buf();
        let path = root.join("workspace.sqlite3");
        let intent = root.join("dispatch-intent.json");
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
                view.update(cx, |a, _cx| {
                    a.store = store.clone();
                    a.last_saved = store.clone();
                    a.qa_dispatch_fake = Some(FakeDispatchRunner {
                        launch_log: Some(root.join("launches.log")),
                        ..Default::default()
                    });
                    a.attach_dispatch_admission(&path, &root, WORKSPACE, intent.clone())
                        .unwrap();
                });
                app = Some(view.clone());
                cx.new(|cx| base::Root::new(view, w, cx))
            })
            .unwrap();
        Self {
            cx,
            handle: window.into(),
            app: app.unwrap(),
            root,
            path,
            intent,
        }
    }

    fn poll_until(&mut self, timeout: Duration, mut done: impl FnMut(&TaskApp) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let value = self
                .cx
                .update_window(self.handle, |_, _, cx| {
                    self.app
                        .update(cx, |a, cx| {
                            a.poll_dispatch_admissions(cx);
                            (done(a), a.qa_dispatch_fake.as_ref().map(|f| f.starts.clone()))
                        })
                })
                .unwrap();
            if value.0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn starts(&mut self) -> Vec<u64> {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app
                    .update(cx, |a, _| a.qa_dispatch_fake.as_ref().unwrap().starts.clone())
            })
            .unwrap()
    }

    fn commit(&mut self, run: AgentRun, intent: LaunchIntent) -> bool {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app
                    .update(cx, |a, cx| {
                        a.commit_prepared_dispatch(
                            PreparedRunDispatch {
                                run: run.clone(),
                                files: Vec::new(),
                            },
                            intent,
                            cx,
                        )
                        .is_some()
                    })
            })
            .unwrap()
    }

    fn poll_runtime_once(&mut self) -> bool {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app.update(cx, |a, cx| a.poll_runtime(cx))
            })
            .unwrap()
    }

    fn advance(&mut self, step: Duration) {
        self.cx.advance_clock(step);
    }

    fn render(&mut self) {
        self.cx
            .update_window(self.handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    fn set_hold(&mut self, hold: bool) {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app.update(cx, |a, _| {
                    if let Some(adapter) = a.dispatch_admission.as_ref() {
                        adapter.set_hold(hold);
                    }
                })
            })
            .unwrap();
    }

    fn set_fault(&mut self, fault: crate::sqlite_v4::Fault) {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app.update(cx, |a, _| {
                    if let Some(adapter) = a.dispatch_admission.as_ref() {
                        adapter.set_fault(fault).unwrap();
                    }
                })
            })
            .unwrap();
    }

    fn cancel_pending(&mut self, run_id: u64) -> bool {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app
                    .update(cx, |a, cx| a.cancel_pending_dispatch(run_id, cx))
            })
            .unwrap()
    }

    fn retry_dispatch(&mut self) -> bool {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app
                    .update(cx, |a, cx| a.retry_dispatch_admission(cx))
            })
            .unwrap()
    }

    /// Drive only the real executor timer. No adapter calls or manual polling.
    fn pump_until(
        &mut self,
        timeout: Duration,
        mut done: impl FnMut(&TaskApp) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.cx.advance_clock(Duration::from_millis(30));
            let value = self
                .cx
                .update_window(self.handle, |_, _, cx| {
                    self.app.update(cx, |a, _| done(a))
                })
                .unwrap();
            if value {
                return true;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        false
    }

    fn run_status(&mut self, run_id: u64) -> Option<String> {
        self.cx
            .update_window(self.handle, |_, _, cx| {
                self.app.update(cx, |a, _| {
                    a.store
                        .runs()
                        .iter()
                        .find(|run| run.id == run_id)
                        .map(|run| run.status.clone())
                })
            })
            .unwrap()
    }

    fn wait_recovery(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let done = self
                .cx
                .update_window(self.handle, |_, _, cx| {
                    self.app.update(cx, |a, cx| {
                        a.poll_runtime(cx);
                        a.dispatch_recovery_done
                    })
                })
                .unwrap();
            if done {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("dispatch admission recovery did not become ready");
    }
}

fn three_step_fixture(root: &Path) -> (Store, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("fixture/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    let goal = store
        .create_research_goal("Sequential dispatch goal".into(), "Scope".into(), 1)
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    (store, steps)
}

fn independent_tasks_fixture(root: &Path) -> (Store, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("fixture/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    // One first step per independent goal: each has no prerequisite, so a queued
    // run for it is a genuinely independent first dispatch.
    let tasks = (0..3)
        .map(|index| {
            let goal = store
                .create_research_goal(
                    format!("Independent dispatch {index}"),
                    "Scope".into(),
                    1,
                )
                .unwrap();
            let steps = store.apply_research_plan(goal, PLAN).unwrap();
            steps[0]
        })
        .collect();
    (store, tasks)
}

fn idle_first_dispatch_keeps_runtime_pump(root: &Path) {
    let (store, steps) = independent_tasks_fixture(root);
    let mut harness = AppHarness::new("idle-pump", store.clone());
    harness.wait_recovery();
    harness.cx.update_window(harness.handle, |_, _, cx| {
        harness.app.update(cx, |a, _| {
            // Keep the scheduler from spawning a real agent in this focused
            // polling regression; the admitted run must still queue exactly once.
            a.storage_error = "QA runtime polling fixture: launch suppressed".into();
        });
    });
    for (index, task) in steps.iter().enumerate() {
        let run = queued_run(&store, *task, 2_000 + index as u64);
        assert!(!harness.commit(run.clone(), LaunchIntent::Task(*task)));
        // The idle workspace has no active run yet; the first runtime poll must
        // still be kept alive by the awaiting admission.
        assert!(
            harness.poll_runtime_once(),
            "runtime pump stopped before the first durable dispatch ACK"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let queued = harness
                .cx
                .update_window(harness.handle, |_, _, cx| {
                    harness.app.update(cx, |a, _| {
                        a.queued_runs.iter().filter(|(id, _)| *id == run.id).count()
                    })
                })
                .unwrap();
            if queued == 1 {
                break;
            }
            assert!(Instant::now() < deadline, "dispatch did not become durable");
            assert!(harness.poll_runtime_once());
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.cx.update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert_eq!(
                    a.queued_runs.iter().filter(|(id, _)| *id == run.id).count(),
                    1,
                    "one admission must queue exactly one dispatch"
                );
                assert!(a.active_runs.is_empty(), "no process launched by polling");
            });
        });
        let ack_deadline = Instant::now() + Duration::from_secs(3);
        while harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness.app.update(cx, |a, cx| {
                    let _ = a.poll_runtime(cx);
                    a.dispatch_awaiting.is_some() || a.dispatch_ack_pending.is_some()
                })
            })
            .unwrap()
        {
            assert!(
                Instant::now() < ack_deadline,
                "dispatch acknowledgement did not settle"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn executor_driven_three_step_dispatch(root: &Path) {
    let directory = root.join("executor-live");
    std::fs::create_dir_all(&directory).unwrap();
    let cli = directory.join("fake-opencode");
    write_fake_cli(&cli);
    let (mut store, steps) = three_step_fixture(&directory);
    store.set_opencode_path(cli.to_string_lossy().into_owned());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    let goal = store
        .tasks()
        .iter()
        .find(|task| task.research.is_some())
        .map(|task| task.id)
        .unwrap();
    store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);

    let path = directory.join("workspace.sqlite3");
    crate::sqlite_v4::SqliteV4::create_seeded(&path, &directory, INCARNATION, &store).unwrap();
    let storage = Storage::new(path.clone());
    let _ = storage.upgrade_paths(|_| {});
    let lock = storage.acquire_lock().unwrap();
    let mut live = persistence_live::select_existing(&storage).unwrap().unwrap();
    let loaded = live.migrated_store.take();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut entity = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_loaded_storage_live(
                    storage,
                    Ok((lock, loaded)),
                    Some(live),
                    w,
                    cx,
                );
            });
            entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        cx.advance_clock(Duration::from_millis(100));
        let ready = cx.update(|cx| {
            let a = app.read(cx);
            a.live_persistence_active() && a.dispatch_recovery_done
        });
        if ready {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        cx.update(|cx| {
            let a = app.read(cx);
            a.live_persistence_active() && a.dispatch_recovery_done
        }),
        "live startup did not become ready"
    );

    cx.update(|cx| app.update(cx, |a, cx| a.start_goal_batch(goal, cx)));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut complete = false;
    while Instant::now() < deadline {
        cx.advance_clock(Duration::from_millis(100));
        let (done, settled) = cx.update(|cx| {
            let a = app.read(cx);
            let done = steps.iter().all(|task| {
                a.store
                    .runs()
                    .iter()
                    .filter(|run| run.task_id == *task)
                    .count()
                    == 1
                    && a.store
                        .runs()
                        .iter()
                        .filter(|run| run.task_id == *task)
                        .all(|run| run.status == "Succeeded")
            });
            (done, done && !a.runtime_pump_active())
        });
        if settled {
            complete = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(
        complete,
        "three-step dispatch did not settle through the runtime pump: {}",
        cx.update(|cx| {
            let a = app.read(cx);
            let runs: Vec<_> = a
                .store
                .runs()
                .iter()
                .map(|run| (run.id, run.task_id, run.status.clone(), run.logs.last().cloned()))
                .collect();
            format!(
                "steps={steps:?} runs={runs:?} error={:?} storage_error={:?} admission_error={:?} queued={:?} active={:?} pumping={} batch={:?} approved={:?}",
                a.error,
                a.storage_error,
                a.dispatch_admission_error,
                a.queued_runs.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                a.active_runs.iter().map(|run| run.id).collect::<Vec<_>>(),
                a.runtime_pump_active(),
                a.todo_batch.as_ref().map(|batch| batch.state.clone()),
                a.todo_batch.as_ref().map(|batch| batch.run_ids.clone()),
            )
        })
    );

    cx.update(|cx| {
        let a = app.read(cx);
        assert!(a.storage_error.is_empty(), "{}", a.storage_error);
        for task in &steps {
            let attempts: Vec<_> = a.store.runs().iter().filter(|run| run.task_id == *task).collect();
            assert_eq!(attempts.len(), 1, "each step has exactly one attempt");
            assert_eq!(attempts[0].status, "Succeeded");
        }
        assert!(a.active_runs.is_empty());
        assert!(a.queued_runs.is_empty());
        assert!(!a.runtime_pump_active(), "runtime pump settles after the durable chain");
    });
}

fn automation_occurrence(id: u64, run_id: u64, unread: bool) -> AutomationOccurrence {
    let now = chrono::Utc::now().timestamp();
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
        snapshot: AutomationOccurrenceSnapshot {
            objective: "qa".into(),
            expected_result: "qa".into(),
            shape: Some(AutomationShape::Quick),
            template: AutomationTemplate {
                goals: Vec::new(),
                window_rule: WindowRule::Unresolved,
            },
            models: std::array::from_fn(|_| "fixture/model".to_string()),
            agents: [None, None, None, None],
            materials: Vec::new(),
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "qa".into(),
        },
        status: OccurrenceStatus::Succeeded,
        reason: None,
        research_goal_id: None,
        research_goal_ids: Vec::new(),
        research_topic_id: None,
        run_ids: vec![run_id],
        final_run_id: Some(run_id),
        unread,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: now,
        deadline_at: None,
        retry_requested_at: None,
    }
}

fn approved_goal_with_plan(root: &Path, plan: &str) -> (Store, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("fixture/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    let goal = store
        .create_research_goal("Batch cancellation goal".into(), "Scope".into(), 1)
        .unwrap();
    let steps = store.apply_research_plan(goal, plan).unwrap();
    store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
    (store, goal, steps)
}

fn cancel_before_receipt_never_launches(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 5_100);
    let mut harness = AppHarness::new("cancel-pre-receipt", store);
    harness.wait_recovery();
    // Hold the worker so the durable receipt cannot arrive before cancellation.
    harness.set_hold(true);
    assert!(!harness.commit(run.clone(), LaunchIntent::Task(task)));
    assert!(
        harness.cancel_pending(run.id),
        "a pending admission must be cancellable before its receipt"
    );
    harness.set_hold(false);
    let settled = harness.pump_until(Duration::from_secs(15), |a| {
        a.dispatch_cancelled.is_empty()
            && a.dispatch_awaiting.is_none()
            && a.dispatch_status_durable.is_none()
            && a.store
                .runs()
                .iter()
                .any(|candidate| candidate.id == run.id && candidate.status == "Cancelled")
            && !a.runtime_pump_active()
    });
    assert!(
        settled,
        "pre-receipt cancellation did not settle: status={:?}",
        harness.run_status(run.id)
    );
    assert!(
        harness.starts().is_empty(),
        "a cancelled pre-receipt dispatch must never launch"
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                let saved = a
                    .last_saved
                    .runs()
                    .iter()
                    .find(|candidate| candidate.id == run.id)
                    .expect("the receipt must leave a durable run");
                assert_eq!(
                    saved.status, "Cancelled",
                    "pre-receipt cancellation must be durable"
                );
            })
        })
        .unwrap();
}

fn cancel_after_receipt_before_claim_never_launches(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 5_200);
    let mut harness = AppHarness::new("cancel-before-claim", store);
    harness.wait_recovery();
    // Deterministic window: the claim request is held after the durable receipt.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| a.qa_hold_claim = true)
        })
        .unwrap();
    assert!(!harness.commit(run.clone(), LaunchIntent::Task(task)));
    assert!(
        harness.pump_until(Duration::from_secs(10), |a| {
            a.store.runs().iter().any(|candidate| candidate.id == run.id)
                && a.dispatch_claim_pending.is_some()
        }),
        "the durable receipt was not applied before the held claim"
    );
    assert!(
        harness.starts().is_empty(),
        "the held claim must not have launched yet"
    );
    // Exercise the real public Stop route, not the internal helper.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| a.cancel_run_id(run.id, cx))
        })
        .unwrap();
    let settled = harness.pump_until(Duration::from_secs(15), |a| {
        a.dispatch_cancelled.is_empty()
            && a.dispatch_cancel_retire.is_empty()
            && a.dispatch_awaiting.is_none()
            && a.store
                .runs()
                .iter()
                .any(|candidate| candidate.id == run.id && candidate.status == "Cancelled")
            && !a.runtime_pump_active()
    });
    assert!(
        settled,
        "post-receipt cancellation did not settle: status={:?} diagnostic={}",
        harness.run_status(run.id),
        harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness.app.update(cx, |a, _| format!(
                    "cancelled={:?} retire={:?} awaiting={:?} status_durable={:?} parked={} ack={:?} claim={:?} admission_error={:?}",
                    a.dispatch_cancelled,
                    a.dispatch_cancel_retire,
                    a.dispatch_awaiting.as_ref().map(|id| id.operation_key.clone()),
                    a.dispatch_status_durable.as_ref().map(|r| r.key.clone()),
                    a.dispatch_parked,
                    a.dispatch_ack_pending.as_ref().map(|id| id.operation_key.clone()),
                    a.dispatch_claim_pending.as_ref().map(|(id, _)| id.operation_key.clone()),
                    a.dispatch_admission_error,
                ))
            })
            .unwrap()
    );
    assert!(
        harness.starts().is_empty(),
        "a run cancelled before its claim was acknowledged must never launch"
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                let saved = a
                    .last_saved
                    .runs()
                    .iter()
                    .find(|candidate| candidate.id == run.id)
                    .expect("durable run");
                assert_eq!(saved.status, "Cancelled");
            })
        })
        .unwrap();
}

fn pending_admission_defers_fanout_and_cancel_cleans_binding(root: &Path) {
    let directory = root.join("fanout");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _steps) = approved_goal_with_plan(&directory, INDEPENDENT_PLAN);
    let mut harness = AppHarness::new_in(root, "fanout", store);
    harness.wait_recovery();
    let database = harness.path.clone();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.storage = Some(Storage::new(database));
            })
        })
        .unwrap();
    harness.set_hold(true);
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness
                .app
                .update(cx, |a, cx| a.start_goal_batch(goal, cx))
        })
        .unwrap();
    assert!(
        harness.pump_until(Duration::from_secs(5), |a| a.batch_pending_runs.len() == 1),
        "the first admission must occupy the single pending slot: {}",
        harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness.app.update(cx, |a, _| format!(
                    "error={:?} storage_error={:?} batch={:?} pending={:?} awaiting={:?} queue={:?} stages={:?}",
                    a.error,
                    a.storage_error,
                    a.todo_batch.as_ref().map(|b| (b.state.clone(), b.items.clone(), b.failed_items.clone())),
                    a.batch_pending_runs,
                    a.dispatch_awaiting.as_ref().map(|id| (id.run_id, id.operation_key.clone())),
                    a.queued_runs.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    a.store.tasks().iter().map(|t| (t.id, t.stage, t.research.as_ref().map(|r| r.kind))).collect::<Vec<_>>(),
                ))
            })
            .unwrap()
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert_eq!(
                    a.batch_pending_runs.len(),
                    1,
                    "remaining ready steps must be deferred, not rejected"
                );
                assert!(
                    matches!(
                        a.todo_batch.as_ref().map(|batch| &batch.state),
                        Some(crate::batch::BatchState::Running)
                    ),
                    "a pending admission must not pause the batch"
                );
                assert!(a.queued_runs.is_empty());
            })
        })
        .unwrap();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| a.cancel_todo_batch(cx))
        })
        .unwrap();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert!(a.todo_batch.is_none());
                assert!(
                    a.batch_pending_runs.is_empty(),
                    "cancel must remove pending batch bindings"
                );
                assert!(
                    !a.dispatch_cancelled.is_empty(),
                    "the accepted dispatch must remain tracked for cancellation"
                );
            })
        })
        .unwrap();
    harness.set_hold(false);
    assert!(
        harness.pump_until(Duration::from_secs(15), |a| {
            a.dispatch_cancelled.is_empty()
                && a.dispatch_awaiting.is_none()
                && !a.runtime_pump_active()
        }),
        "cancelled pending batch did not settle"
    );
    assert!(
        harness.starts().is_empty(),
        "no step may launch after the batch was cancelled"
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert!(
                    a.store
                        .runs()
                        .iter()
                        .all(|run| run.status != "Queued" && run.status != "Running"),
                    "no cancelled step may remain scheduled"
                );
            })
        })
        .unwrap();
}

fn parked_worker_quiesces_and_retry_resumes(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 5_300);
    let mut harness = AppHarness::new("parked-worker", store);
    harness.wait_recovery();
    // Force the dispatch group commit to fail once so the worker parks it.
    harness.set_fault(crate::sqlite_v4::Fault::MidGroup);
    assert!(!harness.commit(run.clone(), LaunchIntent::Task(task)));
    assert!(
        harness.pump_until(Duration::from_secs(15), |a| {
            a.dispatch_parked && !a.runtime_pump_active()
        }),
        "a blocked worker group must park and stop the bounded pump"
    );
    assert!(
        harness.starts().is_empty(),
        "no process may launch from a parked group"
    );
    // Explicit retry through the real compact footer control, not a helper.
    harness.set_fault(crate::sqlite_v4::Fault::None);
    harness.render();
    harness
        .cx
        .update_window(harness.handle, |_, window, cx| {
            window.click("retry-dispatch-admission", cx);
        })
        .unwrap();
    assert!(
        harness.pump_until(Duration::from_secs(15), |a| {
            a.qa_dispatch_fake
                .as_ref()
                .is_some_and(|fake| fake.starts.contains(&run.id))
                && !a.runtime_pump_active()
        }),
        "the visible retry control did not resume the exact parked dispatch"
    );
    let starts = harness.starts();
    assert_eq!(
        starts.iter().filter(|id| **id == run.id).count(),
        1,
        "the retried intent must launch exactly once"
    );
    assert!(
        starts.iter().all(|id| *id == run.id),
        "retry must not allocate another run"
    );
}

fn material_verification_wakes_and_settles(root: &Path) {
    let (mut store, task) = fixture(root);
    if let Some(goal) = store.owning_research_goal(task) {
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
    }
    let mut harness = AppHarness::new("material-wake", store);
    harness.wait_recovery();
    let database = harness.path.clone();
    let data = harness.root.clone();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.storage = Some(Storage::new(database));
            })
        })
        .unwrap();
    let relative = "attachments/synthetic.png";
    std::fs::create_dir_all(data.join("attachments")).unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nsynthetic material";
    std::fs::write(data.join(relative), bytes).unwrap();
    let sha = format!("{:x}", Sha256::digest(bytes));
    let attachment = crate::attachments::Attachment {
        name: "synthetic.png".into(),
        relative_path: relative.into(),
        media_type: "image/png".into(),
        size: bytes.len() as u64,
        content_sha256: sha.clone(),
    };
    let imported = vec![crate::scoped_material::ImportedCopy {
        relative_path: relative.into(),
        size: bytes.len() as u64,
        content_sha256: sha,
        owner_task: task,
    }];
    let submitted = harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.submit_material_add_verify_for_root(
                    data.clone(),
                    task,
                    vec![attachment.clone()],
                    imported,
                    cx,
                )
            })
        })
        .unwrap();
    assert!(submitted, "material verification must be accepted");
    // No manual poll: the submission wakepoint and keep predicate must finish it.
    assert!(
        harness.pump_until(Duration::from_secs(15), |a| {
            a.pending_material_job.is_none()
        }),
        "material verification did not settle through the runtime pump"
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                let review =
                    a.pending_material_edit.is_some() || a.research_save_impact.is_some();
                let attached = a.store.task(task).is_some_and(|value| {
                    value
                        .attachments
                        .iter()
                        .any(|candidate| candidate.relative_path == relative)
                });
                assert!(
                    review || attached,
                    "verification must either open the impact review or attach durably"
                );
                assert!(
                    !a.runtime_pump_active(),
                    "human material review must not keep the pump spinning"
                );
            })
        })
        .unwrap();
    harness
        .cx
        .update_window(harness.handle, |_, window, cx| {
            harness
                .app
                .update(cx, |a, cx| a.cancel_research_save(window, cx));
        })
        .unwrap();
    assert!(
        harness.pump_until(Duration::from_secs(15), |a| {
            a.pending_material_job.is_none()
                && a.pending_material_edit.is_none()
                && a.research_save_impact.is_none()
                && a.pending_cleanup_retry.is_empty()
                && !a.runtime_pump_active()
        }),
        "cancelled material verification must clean up and quiesce"
    );
}

fn render_wake_saves_idle_legacy_edit(root: &Path) {
    let directory = root.join("render-wake");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into_owned();
    store.set_default_model("fixture/model".into());
    store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
    let task = store
        .create_research_goal("Render wake goal".into(), "Scope".into(), 1)
        .unwrap();
    store.push_automation(AutomationDefinition {
        id: 1,
        version: 1,
        project_id: 1,
        shape: AutomationShape::Quick,
        name: "Render wake QA".into(),
        objective: "QA".into(),
        expected_result: "QA".into(),
        schedule: Schedule {
            spec: ScheduleSpec::Interval { minutes: 1440 },
            timezone: "UTC".into(),
        },
        status: DefinitionStatus::Draft,
        template: None,
        plan_binding: None,
        approval: None,
        policy: AutomationPolicy::default(),
        cursor: None,
        anchor: None,
        consecutive_failures: 0,
        attention_reason: None,
        created_at: 0,
        updated_at: 0,
    });
    let mut occurrence = automation_occurrence(900, 901, true);
    occurrence.research_goal_id = Some(task);
    occurrence.research_goal_ids = vec![task];
    store.push_occurrence(occurrence);

    let path = directory.join("workspace.sqlite3");
    crate::sqlite_v4::SqliteV4::create_seeded(&path, &directory, INCARNATION, &store).unwrap();
    let storage = Storage::new(path.clone());
    let _ = storage.upgrade_paths(|_| {});
    let lock = storage.acquire_lock().unwrap();
    let mut live = persistence_live::select_existing(&storage).unwrap().unwrap();
    let loaded = live.migrated_store.take();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut entity = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_loaded_storage_live(storage, Ok((lock, loaded)), Some(live), w, cx);
            });
            entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let handle: AnyWindowHandle = window.into();

    // Wait for startup to quiesce so the later edit genuinely begins idle.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut idle = false;
    while Instant::now() < deadline {
        cx.advance_clock(Duration::from_millis(80));
        idle = cx.update(|cx| {
            let a = app.read(cx);
            a.live_persistence_active() && !a.runtime_pump_active() && !a.live_needs_pump()
        });
        if idle {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(idle, "live startup did not become idle");

    // Real idle mark-read mutation and persist, with no notify or start_pump.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _cx| {
            a.store.mark_occurrence_read(900);
            a.persist_if_changed(true);
        });
    })
    .unwrap();
    let pending_without_pump = cx.update(|cx| {
        let a = app.read(cx);
        !a.runtime_pump_active() && a.live_has_pending_receipts()
    });
    assert!(
        pending_without_pump,
        "mark-read must leave a receipt pending while the pump is idle: {}",
        cx.update(|cx| {
            let a = app.read(cx);
            format!(
                "pumping={} pending={} dirty={} unread={:?} occurrence={:?} live_active={}",
                a.runtime_pump_active(),
                a.live_has_pending_receipts(),
                a.store.has_tracked_changes(),
                a.store.occurrence(900).map(|value| value.unread),
                a.store.occurrence(900).map(|value| value.id),
                a.live_persistence_active(),
            )
        })
    );

    // A render is the only wake; no manual poll.
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut read_settled = false;
    while Instant::now() < deadline {
        cx.advance_clock(Duration::from_millis(40));
        read_settled = cx.update(|cx| {
            let a = app.read(cx);
            !a.live_needs_pump()
                && !a.runtime_pump_active()
                && !a.store.has_tracked_changes()
                && a.store.occurrence(900).is_some_and(|value| !value.unread)
        });
        if read_settled {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let read_settled = read_settled
        || cx.update(|cx| {
            let a = app.read(cx);
            !a.live_needs_pump()
                && !a.runtime_pump_active()
                && !a.store.has_tracked_changes()
                && a.store.occurrence(900).is_some_and(|value| !value.unread)
        });
    assert!(
        read_settled,
        "the render wake did not carry the mark-read receipt to a saved idle state: {}",
        cx.update(|cx| {
            let a = app.read(cx);
            format!(
                "pumping={} needs_pump={} pending={} dirty={} unread={:?} saved_unread={:?} primary={} followups={} live_failure={:?} storage_error={:?}",
                a.runtime_pump_active(),
                a.live_needs_pump(),
                a.live_has_pending_receipts(),
                a.store.has_tracked_changes(),
                a.store.occurrence(900).map(|value| value.unread),
                a.last_saved.occurrence(900).map(|value| value.unread),
                a.live_primary.is_some(),
                a.live_followups.len(),
                a.live_failure,
                a.storage_error,
            )
        })
    );

    let durable_read = crate::sqlite_v4::SqliteV4::open(&path, &directory, INCARNATION)
        .unwrap()
        .load_store()
        .unwrap();
    assert!(
        durable_read
            .occurrence(900)
            .is_some_and(|value| !value.unread),
        "the mark-read receipt must be durable in the workspace"
    );

    // Representative other context-free legacy edit: mutate only the store and
    // persist, then rely on the same render wake.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _cx| {
            a.store
                .task_mut(task)
                .unwrap()
                .labels
                .push("legacy-edit".into());
            a.persist_if_changed(true);
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut edit_settled = false;
    while Instant::now() < deadline {
        cx.advance_clock(Duration::from_millis(40));
        edit_settled = cx.update(|cx| {
            let a = app.read(cx);
            !a.live_needs_pump()
                && !a.runtime_pump_active()
                && !a.store.has_tracked_changes()
                && a
                    .store
                    .task(task)
                    .is_some_and(|value| value.labels.iter().any(|label| label == "legacy-edit"))
        });
        if edit_settled {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let edit_settled = edit_settled
        || cx.update(|cx| {
            let a = app.read(cx);
            !a.live_needs_pump()
                && !a.runtime_pump_active()
                && !a.store.has_tracked_changes()
                && a
                    .store
                    .task(task)
                    .is_some_and(|value| value.labels.iter().any(|label| label == "legacy-edit"))
        });
    assert!(
        edit_settled,
        "the render wake did not persist a second context-free legacy edit"
    );
}

fn durable_before_launch(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 1_000);
    let mut harness = AppHarness::new("durable", store.clone());
    assert!(!harness.commit(run.clone(), LaunchIntent::Task(task)));
    // No queue, active run or fake launch before the durable receipt.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert!(a.queued_runs.is_empty(), "queued before durable ack");
                assert!(a.active_runs.is_empty(), "active before durable ack");
                assert!(
                    a.store.runs().iter().all(|value| value.id != run.id),
                    "run entered the store before durable ack"
                );
            })
        })
        .unwrap();
    assert!(harness.starts().is_empty(), "fake runner started before ack");
    assert!(
        harness.poll_until(Duration::from_secs(10), |a| {
            a.dispatch_awaiting.is_none()
                && a.store.runs().iter().any(|value| value.id == run.id)
        }),
        "dispatch never became durable"
    );
    assert_eq!(harness.starts(), vec![run.id], "exactly one launch after ack");
    let keys: Vec<String> = std::fs::read(&harness.intent)
        .ok()
        .and_then(|payload| serde_json::from_slice::<serde_json::Value>(&payload).ok())
        .and_then(|value| {
            value["items"].as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["key"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default();
    assert!(
        !keys.iter().any(|key| key == "dispatch:None:run:1000"),
        "confirmed dispatch intent was not acknowledged: {keys:?}"
    );
    assert!(
        keys.iter().any(|key| key == "launch:dispatch:None:run:1000"),
        "launch claim must remain until the run reaches a confirmed state: {keys:?}"
    );
    // The isolated database contains exactly one run and no duplicate link.
    let stored = crate::sqlite_v4::SqliteV4::open(&harness.path, &harness.root, WORKSPACE)
        .unwrap()
        .load_store()
        .unwrap();
    assert_eq!(
        stored.runs().iter().filter(|value| value.id == run.id).count(),
        1
    );
}

fn recover_after_ack_before_launch(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 2_000);
    // Simulate a crash after the durable commit/ack but before scheduler launch:
    // persisted SQLite has the Queued run and no intent remains.
    let mut stored = store.clone();
    stored.push_run(run.clone());
    let mut harness = AppHarness::new("recover", stored);
    assert!(
        harness.poll_until(Duration::from_secs(5), |a| a.dispatch_recovery_done),
        "recovery did not run"
    );
    assert_eq!(
        harness.starts(),
        vec![run.id],
        "durable but unlaunched run is recovered once"
    );
    // A second poll must not duplicate the launch.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness
                .app
                .update(cx, |a, cx| a.poll_dispatch_admissions(cx));
        })
        .unwrap();
    assert_eq!(harness.starts(), vec![run.id], "recovery is idempotent");
}

fn resolve_claimed_ambiguity(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 5_000);
    let mut stored = store.clone();
    stored.push_run(run.clone());
    let mut harness = AppHarness::new("ambiguity", stored);
    let key = "dispatch:None:run:5000".to_string();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.dispatch_recovery_done = true;
                let adapter = a.dispatch_admission.as_mut().unwrap();
                adapter
                    .claim_launch(&AdmissionId {
                        operation_key: key.clone(),
                        run_id: run.id,
                        occurrence: None,
                        generation: 1,
                    })
                    .unwrap();
            })
        })
        .unwrap();
    assert!(
        harness.poll_until(Duration::from_secs(5), |a| {
            a.dispatch_admission
                .as_ref()
                .unwrap()
                .launch_claim_exists(&key)
        }),
        "claim was not restored"
    );
    // Force the recovery pass to reconcile the ambiguous claim visibly.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.dispatch_recovery_done = false;
                a.queued_runs.clear();
                if let Some(mut run) = a.store.run_mut(run.id) {
                    run.status = "Queued".into();
                }
            })
        })
        .unwrap();
    assert!(
        harness.poll_until(Duration::from_secs(5), |a| {
            a.store.runs().iter().any(|value| {
                value.id == run.id && value.status == "Checking"
            })
        }),
        "ambiguous claim did not become Checking"
    );
    assert_eq!(harness.starts(), Vec::<u64>::new(), "no blind relaunch");
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.resolve_launch_ambiguity(run.id, false, cx).unwrap();
            })
        })
        .unwrap();
    for _ in 0..100 {
        harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness
                    .app
                    .update(cx, |a, cx| a.poll_dispatch_admissions(cx));
            })
            .unwrap();
        let released = harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness.app.update(cx, |a, cx| {
                    a.poll_dispatch_admissions(cx);
                    !a.dispatch_admission
                        .as_ref()
                        .unwrap()
                        .launch_claim_exists(&key)
                })
            })
            .unwrap();
        if released {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert_eq!(
                    a.store.runs().iter().find(|value| value.id == run.id).unwrap().status,
                    "Blocked"
                );
                assert!(
                    !a.dispatch_admission
                        .as_ref()
                        .unwrap()
                        .launch_claim_exists(&key),
                    "explicit resolution must release the claim"
                );
            })
        })
        .unwrap();
}

fn rerun_resolution_admits_distinct_attempt(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 6_000);
    let mut stored = store.clone();
    stored.push_run(run.clone());
    let mut harness = AppHarness::new("rerun", stored);
    let key = "dispatch:None:run:6000".to_string();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.dispatch_recovery_done = true;
                a.dispatch_admission
                    .as_mut()
                    .unwrap()
                    .claim_launch(&AdmissionId {
                        operation_key: key.clone(),
                        run_id: run.id,
                        occurrence: None,
                        generation: 1,
                    })
                    .unwrap();
            })
        })
        .unwrap();
    assert!(harness.poll_until(Duration::from_secs(5), |a| {
        a.dispatch_admission
            .as_ref()
            .unwrap()
            .launch_claim_exists(&key)
    }));
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.dispatch_recovery_done = false;
                a.queued_runs.clear();
                if let Some(mut run) = a.store.run_mut(run.id) {
                    run.status = "Queued".into();
                }
            })
        })
        .unwrap();
    assert!(harness.poll_until(Duration::from_secs(5), |a| {
        a.store
            .runs()
            .iter()
            .any(|value| value.id == run.id && value.status == "Checking")
    }));
    let (new_id, interloper) = harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.resolve_launch_ambiguity(run.id, true, cx).unwrap();
                let replacement = a
                    .dispatch_resolution_pending
                    .as_ref()
                    .and_then(|(_, _, _, replacement)| *replacement)
                    .unwrap();
                // A concurrent unrelated admission must not receive the reserved id.
                let interloper = a.store.allocate_run_id().unwrap();
                (replacement, interloper)
            })
        })
        .unwrap();
    assert_ne!(new_id, interloper, "reserved replacement id was reissued");
    assert!(
        harness.poll_until(Duration::from_secs(15), |a| {
            a.store.runs().iter().any(|value| value.id == new_id)
                && a.qa_dispatch_fake
                    .as_ref()
                    .is_some_and(|fake| fake.starts.contains(&new_id))
        }),
        "rerun was not durably admitted and launched as a distinct attempt"
    );
    assert!(
        !harness.starts().contains(&run.id),
        "the old attempt identifier must never be requeued"
    );
    assert_eq!(
        harness
            .starts()
            .iter()
            .filter(|id| **id == new_id)
            .count(),
        1,
        "replacement attempt launched exactly once"
    );
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                assert_eq!(
                    a.store.runs().iter().find(|run| run.id == run.id).map(|r| r.status.clone()),
                    Some("Blocked".into())
                );
            })
        })
        .unwrap();
}

fn persisted_status_gates_claim_cleanup(root: &Path) {
    let (store, task) = fixture(root);
    let run = queued_run(&store, task, 7_000);
    let mut stored = store.clone();
    stored.push_run(run.clone());
    let mut harness = AppHarness::new("claim-cleanup", stored);
    let key = "dispatch:None:run:7000".to_string();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                a.dispatch_recovery_done = true;
                a.dispatch_admission
                    .as_mut()
                    .unwrap()
                    .claim_launch(&AdmissionId {
                        operation_key: key.clone(),
                        run_id: run.id,
                        occurrence: None,
                        generation: 1,
                    })
                    .unwrap();
            })
        })
        .unwrap();
    assert!(harness.poll_until(Duration::from_secs(5), |a| {
        a.dispatch_admission
            .as_ref()
            .unwrap()
            .launch_claim_exists(&key)
    }));
    // In-memory terminal status alone must not release the claim.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.store.run_mut(run.id).unwrap().status = "Succeeded".into();
                a.poll_dispatch_admissions(cx);
                assert!(
                    a.dispatch_admission
                        .as_ref()
                        .unwrap()
                        .launch_claim_exists(&key),
                    "claim was released before the terminal status was persisted"
                );
            })
        })
        .unwrap();
    // Once the persisted baseline carries a confirmed Running status, cleanup is
    // safe: the launch identity is durably tied to this run.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.last_saved.run_mut(run.id).unwrap().status = "Running".into();
                a.poll_dispatch_admissions(cx);
            })
        })
        .unwrap();
    for _ in 0..100 {
        let released = harness
            .cx
            .update_window(harness.handle, |_, _, cx| {
                harness.app.update(cx, |a, cx| {
                    a.poll_dispatch_admissions(cx);
                    !a.dispatch_admission
                        .as_ref()
                        .unwrap()
                        .launch_claim_exists(&key)
                })
            })
            .unwrap();
        if released {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("claim was not released after the persisted Running status");
}

fn reserved_dispatch_survives_prepare_failure(root: &Path) {
    let (store, task) = fixture(root);
    let mut run = queued_run(&store, task, 8_000);
    run.directory = root.join("missing-directory").to_string_lossy().into_owned();
    let mut harness = AppHarness::new("reserved-prepare", store);
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                a.dispatch_recovery_done = true;
                a.pending_dispatch = Some(PendingDispatch {
                    intent: LaunchIntent::Task(task),
                    files: Vec::new(),
                });
                a.pending_dispatch_reserved =
                    Some((LaunchIntent::Task(task), run.clone(), Vec::new()));
                a.qa_retry_pending(cx);
            })
        })
        .unwrap();
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                let reserved = a
                    .pending_dispatch_reserved
                    .as_ref()
                    .expect("failed preparation must retain the exact reserved run");
                assert_eq!(reserved.1.id, 8_000);
            })
        })
        .unwrap();
    // Repair the working folder and retry the same reserved identity.
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, cx| {
                if let Some((intent, mut run, files)) = a.pending_dispatch_reserved.take() {
                    run.directory = root.to_string_lossy().into_owned();
                    a.pending_dispatch_reserved = Some((intent, run, files));
                }
                a.qa_retry_pending(cx);
            })
        })
        .unwrap();
    assert!(
        harness.poll_until(Duration::from_secs(15), |a| {
            a.qa_dispatch_fake
                .as_ref()
                .is_some_and(|fake| fake.starts.contains(&8_000))
        }),
        "reserved dispatch did not launch after repair"
    );
    assert!(
        !harness.starts().iter().any(|id| *id != 8_000),
        "preparation retry allocated a different run id: {:?}",
        harness.starts()
    );
}

fn backpressure_retains_intent(root: &Path) {
    let (store, task) = fixture(root);
    let first = queued_run(&store, task, 3_000);
    let second = queued_run(&store, task, 3_001);
    let mut harness = AppHarness::new("backpressure", store);
    assert!(!harness.commit(first, LaunchIntent::Task(task)));
    // A second dispatch while the first awaits durability must preserve its
    // caller intent and files instead of dropping them.
    assert!(!harness.commit(second.clone(), LaunchIntent::Task(task)));
    harness
        .cx
        .update_window(harness.handle, |_, _, cx| {
            harness.app.update(cx, |a, _| {
                let pending = a
                    .pending_dispatch
                    .as_ref()
                    .expect("second intent retained under backpressure");
                assert_eq!(pending.intent, LaunchIntent::Task(task));
                assert!(a.queued_runs.is_empty());
            })
        })
        .unwrap();
}


/// Subprocess crash harness for the L1b admission seams. The phase aborts the
/// process after a real durable state transition; the parent test verifies the
/// database, claim and launch record after restart.
pub fn crash_child() {
    let args: Vec<String> = std::env::args().collect();
    let phase = args.get(2).cloned().unwrap_or_default();
    let root = PathBuf::from(args.get(3).cloned().unwrap_or_default());
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("workspace.sqlite3");
    let intent = root.join("dispatch-intent.json");
    let launches = root.join("launches.log");
    if !path.exists() {
        let (store, _) = fixture(&root);
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, WORKSPACE, &store).unwrap();
    }
    if phase == "recover-check" {
        recover_app_check(&root);
        return;
    }
    if phase == "restart-failed" || phase == "restart-rerun" {
        restart_execute(&root, phase == "restart-rerun");
        return;
    }
    if phase.starts_with("restart-crash-") {
        restart_execute(&root, phase.contains("rerun"));
        panic!("crash point was not reached");
    }
    let (store, task) = fixture(&root);
    let run = queued_run(&store, task, 4_000);
    let command = Command::DispatchRun {
        occurrence: None,
        run,
    };
    let prepared = store
        .prepare_command(command.id(), CommandClass::StartOrExtend, command)
        .unwrap();
    let mut adapter = DispatchAdmissionAdapter::open(
        &path,
        &root,
        WORKSPACE,
        WorkerOptions::default(),
        intent,
    )
    .unwrap();
    for _ in 0..800 {
        adapter.poll();
        if adapter.is_ready() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(adapter.is_ready(), "child adapter did not start");
    let id = adapter.admit(prepared).unwrap();
    let durable = loop {
        let mut found = None;
        for update in adapter.poll() {
            if let AdmissionUpdate::Durable(value) = update {
                found = Some(value);
            }
        }
        if let Some(value) = found {
            break value;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(durable.id.run_id, id.run_id);
    if phase == "abort-after-durable" {
        std::process::abort();
    }
    if phase == "abort-after-ack" {
        adapter.acknowledge(&durable.id).unwrap();
        loop {
            let mut acked = false;
            for update in adapter.poll() {
                if matches!(update, AdmissionUpdate::Acknowledged { .. }) {
                    acked = true;
                }
            }
            if acked {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        std::process::abort();
    }
    adapter
        .claim_launch(&durable.id)
        .unwrap();
    loop {
        let mut claimed = false;
        for update in adapter.poll() {
            if matches!(update, AdmissionUpdate::LaunchClaimPersisted { .. }) {
                claimed = true;
            }
        }
        if claimed {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if phase == "resolution-failed" || phase == "resolution-rerun" {
        let rerun = phase == "resolution-rerun";
        adapter
            .persist_resolution(
                "dispatch:None:run:4000",
                rerun,
                rerun.then_some(5_000),
            )
            .unwrap();
        loop {
            let mut persisted = false;
            for update in adapter.poll() {
                if matches!(update, AdmissionUpdate::ResolutionPersisted { .. }) {
                    persisted = true;
                }
            }
            if persisted {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        std::process::abort();
    }
    match phase.as_str() {
        "claim-before-spawn" => std::process::abort(),
        "spawn-before-status" => {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&launches)
                .unwrap();
            writeln!(file, "{}", id.run_id).unwrap();
            file.sync_all().unwrap();
            std::process::abort();
        }
        other => panic!("unknown crash phase {other}"),
    }
}


fn recover_app_check(root: &Path) {
    let path = root.join("workspace.sqlite3");
    let launches = root.join("launches.log");
    let before = std::fs::read_to_string(&launches)
        .map(|value| value.lines().count())
        .unwrap_or(0);
    let store = crate::sqlite_v4::SqliteV4::open(&path, root, WORKSPACE)
        .unwrap()
        .load_store()
        .unwrap();
    assert!(
        store
            .runs()
            .iter()
            .any(|run| run.status == "Queued"),
        "restart fixture must contain the durable queued run"
    );
    let mut harness = AppHarness::existing(root, store);
    assert!(
        harness.poll_until(Duration::from_secs(10), |a| {
            a.store
                .runs()
                .iter()
                .any(|run| run.status == "Checking")
        }),
        "app recovery did not reconcile the ambiguous claim"
    );
    assert!(
        harness.starts().is_empty(),
        "app scheduler relaunched an ambiguously claimed run"
    );
    let after = std::fs::read_to_string(&launches)
        .map(|value| value.lines().count())
        .unwrap_or(0);
    assert_eq!(before, after, "app restart recorded another launch");
}


fn restart_execute(root: &Path, rerun: bool) {
    let path = root.join("workspace.sqlite3");
    let launches = root.join("launches.log");
    let store = crate::sqlite_v4::SqliteV4::open(&path, root, WORKSPACE)
        .unwrap()
        .load_store()
        .unwrap();
    let mut harness = AppHarness::existing(root, store);
    let old_key = "dispatch:None:run:4000";
    let new_id = 5_000;
    let mut last_diagnostic = std::time::Instant::now();
    let done = harness.poll_until(Duration::from_secs(20), |a| {
        if last_diagnostic.elapsed() >= Duration::from_secs(5) {
            last_diagnostic = std::time::Instant::now();
            let (pending, durable, worker_error) = a
                .dispatch_admission
                .as_ref()
                .map(|adapter| {
                    (
                        adapter.pending_len(),
                        adapter.durable_count(),
                        adapter.last_error().unwrap_or("").to_string(),
                    )
                })
                .unwrap_or((0, 0, "no-adapter".into()));
            eprintln!(
                "DIAG retirement={:?} pending={pending} durable={durable} worker_error={worker_error} claim={} resolution={:?}",
                a.dispatch_status_durable,
                a.dispatch_admission
                    .as_ref()
                    .is_some_and(|adapter| adapter.launch_claim_exists(old_key)),
                a.dispatch_admission
                    .as_ref()
                    .and_then(|adapter| adapter.resolution(old_key)),
            );
        }
        let old = a.store.runs().iter().find(|run| run.id == 4_000);
        let old_blocked = old.is_some_and(|run| run.status == "Blocked");
        let claim_gone = !a
            .dispatch_admission
            .as_ref()
            .unwrap()
            .launch_claim_exists(old_key);
        let resolution_gone = a
            .dispatch_admission
            .as_ref()
            .unwrap()
            .resolution(old_key)
            .is_none();
        if rerun {
            old_blocked
                && claim_gone
                && resolution_gone
                && a.store.runs().iter().any(|run| run.id == new_id)
                && a.qa_dispatch_fake
                    .as_ref()
                    .is_some_and(|fake| fake.starts.contains(&new_id))
        } else {
            old_blocked && claim_gone && resolution_gone
        }
    });
    if !done {
        eprintln!(
            "RESTART statuses={:?} claim={} resolution={:?} starts={:?}",
            harness
                .cx
                .update_window(harness.handle, |_, _, cx| {
                    harness
                        .app
                        .update(cx, |a, _| a.store.runs().iter().map(|run| (run.id, run.status.clone())).collect::<Vec<_>>())
                })
                .unwrap(),
            harness
                .cx
                .update_window(harness.handle, |_, _, cx| {
                    harness.app.update(cx, |a, _| {
                        a.dispatch_admission
                            .as_ref()
                            .unwrap()
                            .launch_claim_exists(old_key)
                    })
                })
                .unwrap(),
            harness
                .cx
                .update_window(harness.handle, |_, _, cx| {
                    harness.app.update(cx, |a, _| {
                        a.dispatch_admission
                            .as_ref()
                            .unwrap()
                            .resolution(old_key)
                    })
                })
                .unwrap(),
            harness.starts(),
        );
    }
    assert!(done, "restart execution did not settle");
    let starts = harness.starts();
    assert!(!starts.contains(&4_000), "old attempt was relaunched");
    if rerun {
        assert_eq!(
            starts.iter().filter(|id| **id == new_id).count(),
            1,
            "replacement attempt must launch exactly once"
        );
    } else {
        assert!(starts.is_empty(), "failed decision launched a process");
    }
    if launches.exists() {
        let lines: Vec<_> = std::fs::read_to_string(&launches)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert!(!lines.iter().any(|line| line == "4000"));
        if rerun {
            assert_eq!(lines.iter().filter(|line| *line == "5000").count(), 1);
        }
    }
    let keys: Vec<String> = std::fs::read(root.join("dispatch-intent.json"))
        .ok()
        .and_then(|payload| serde_json::from_slice::<serde_json::Value>(&payload).ok())
        .and_then(|value| {
            value["items"].as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["key"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default();
    assert!(!keys.iter().any(|key| key == old_key));
    assert!(!keys.iter().any(|key| key.starts_with("resolve:")));
    assert!(!keys.iter().any(|key| key == "launch:dispatch:None:run:4000"));
}
