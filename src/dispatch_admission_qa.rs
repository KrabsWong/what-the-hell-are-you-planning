//! T14 L1b app-level durable dispatch admission QA.
//!
//! Uses the real `DispatchAdmissionAdapter` + `PersistenceWorker` + isolated
//! SQLite v4 database and a fake local launch recorder. No model, network or
//! production workspace is touched.

use crate::app::{LaunchIntent, PreparedRunDispatch, TaskApp};
use crate::dispatch_admission::{AdmissionId, AdmissionUpdate, DispatchAdmissionAdapter};
use crate::domain_command::{Command, CommandClass};
use crate::persistence_worker::WorkerOptions;
use crate::dispatch_admission::FakeDispatchRunner;
use crate::domain::{AgentRun, Store};
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WORKSPACE: u64 = 9_001;
const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Find evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

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
    recover_after_ack_before_launch(&root);
    resolve_claimed_ambiguity(&root);
    rerun_resolution_admits_distinct_attempt(&root);
    persisted_status_gates_claim_cleanup(&root);
    backpressure_retains_intent(&root);
    println!("DISPATCH_ADMISSION_QA status=ok root={}", root.display());
}

fn fixture(root: &Path) -> (Store, u64) {
    let mut store = Store::empty();
    store.projects[0].directory = root.to_string_lossy().into_owned();
    store.default_model = "fixture/model".into();
    store.research_config.models = std::array::from_fn(|_| "fixture/model".to_string());
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
    let done = harness.poll_until(Duration::from_secs(20), |a| {
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
