//! Native scheduler acceptance using local processes only; no model or network calls.
use crate::{
    app::{Scope, TaskApp},
    batch::BatchState,
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect fixture evidence","instructions":"Return EVIDENCE_SENTINEL and a source.","kind":"collect","depends_on":[]},{"title":"Analyze fixture evidence","instructions":"Read evidence and return ANALYSIS_SENTINEL.","kind":"analyze","depends_on":[0]},{"title":"Write fixture report","instructions":"Read analysis and preserve sources.","kind":"advise","depends_on":[1]}]}"#;
const CLI: &str = r#"#!/usr/bin/python3
import json,os,pathlib,sys,time
args=sys.argv[1:]
model=args[args.index('--model')+1]
session=args[args.index('--session')+1]
prompt=args[-1]
root=pathlib.Path.cwd()
def event(kind):
 fd=os.open(str(pathlib.Path(__file__).parent/'events.jsonl'),os.O_WRONLY|os.O_CREAT|os.O_APPEND,0o600)
 os.write(fd,(json.dumps({'event':kind,'time':time.monotonic(),'model':model,'session':session,'directory':str(root)})+'\n').encode())
 os.close(fd)
event('start')
print(json.dumps({'type':'reasoning','part':{'text':'CONCURRENCY_LIVE_MARKER'}}),flush=True)
if 'QA_CANCEL' in prompt: time.sleep(30)
time.sleep(1.0 if model=='fixture/collector' else 0.2)
if 'QA_FAIL' in prompt and not (root/'recovered-fail').exists():
 event('failed')
 print(json.dumps({'type':'error','error':{'message':'Intentional concurrency QA failure'}}),flush=True)
 sys.exit(1)
source={'title':'Isolated local fixture','url':'https://example.com/'+root.name,'published_at':'2026-10-29 fixture'}
if model=='fixture/collector':
 (root/'evidence-ready').write_text(session)
 report='EVIDENCE_SENTINEL '+root.name
elif model=='fixture/analyst':
 assert (root/'evidence-ready').exists()
 assert 'EVIDENCE_SENTINEL '+root.name in prompt and source['url'] in prompt
 (root/'analysis-ready').write_text(session)
 report='ANALYSIS_SENTINEL '+root.name
elif model=='fixture/adviser':
 assert (root/'analysis-ready').exists()
 assert 'ANALYSIS_SENTINEL '+root.name in prompt and source['url'] in prompt
 report='FINAL_REPORT '+root.name
else: raise AssertionError(model)
event('complete')
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':report,'sources':[source]})}}),flush=True)
"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/concurrency-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    executable(&cli, CLI);
    let ego = root.join("fake-ego");
    executable(
        &ego,
        "#!/bin/sh\nprintf '%s\\n' 'Local QA guide' '[ego-browser:skill] end of guide'\n",
    );
    let mut store = Store::empty();
    store.projects[0].directory = root.to_string_lossy().into_owned();
    store.opencode_path = cli.to_string_lossy().into_owned();
    store.research_config.ego_path = ego.to_string_lossy().into_owned();
    store.research_config.models = [
        "fixture/planner",
        "fixture/collector",
        "fixture/analyst",
        "fixture/adviser",
    ]
    .map(String::from);
    let goals: Vec<_> = (0..4)
        .map(|i| goal(&mut store, &format!("Independent goal {i}")))
        .collect();
    let path = root.join("workspace.json");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path), w, cx);
                a.navigate(Scope::Project(1), w, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.start_todo_batch(1, cx))
    })
    .unwrap();
    wait(
        &mut cx,
        handle,
        &app,
        |a| {
            a.active_runs.len() == 3
                && !a.queued_runs.is_empty()
                && a.store
                    .runs
                    .iter()
                    .filter(|r| r.logs.iter().any(|l| l.contains("CONCURRENCY_LIVE_MARKER")))
                    .count()
                    == 3
        },
        "three simultaneous CLI sessions",
    );
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("three-running.png"))
        .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert!(
            matches!(a.todo_batch.as_ref().unwrap().state, BatchState::Finished),
            "{}",
            a.error
        );
        assert_eq!(a.store.runs.len(), 12);
        assert!(
            a.store.runs.iter().all(|r| r.status == "Succeeded"),
            "{:?}",
            a.store
                .runs
                .iter()
                .map(|r| (&r.status, &r.logs))
                .collect::<Vec<_>>()
        );
        let dirs: std::collections::BTreeSet<_> =
            a.store.runs.iter().map(|r| r.directory.clone()).collect();
        assert_eq!(dirs.len(), 4, "Each goal needs its own work directory");
        for goal in &goals {
            assert!(a.store.next_research_task(*goal).unwrap().is_none());
        }
    })
    .unwrap();
    // Verify process overlap from the fixtures themselves, independent of UI counters.
    let events = std::fs::read_to_string(root.join("events.jsonl")).unwrap();
    let mut live = std::collections::BTreeSet::new();
    let mut peak = 0;
    for line in events.lines() {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        let session = event["session"].as_str().unwrap().to_owned();
        if event["event"] == "start" {
            assert!(
                live.insert(session),
                "A session executed concurrently with itself"
            );
        } else {
            assert!(live.remove(&session));
        }
        peak = peak.max(live.len());
        assert!(
            live.len() <= 3,
            "CLI process overlap exceeded concurrency limit"
        );
    }
    assert_eq!(peak, 3);
    assert!(live.is_empty());
    // Cancelling one goal must not cancel or stall another independently running goal.
    let (cancel, survivor) = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.cancel_todo_batch(cx);
                let cancel = goal(&mut a.store, "QA_CANCEL");
                let survivor = goal(&mut a.store, "Cancel survivor");
                a.run_research_next(cancel, true, cx);
                a.run_research_next(survivor, true, cx);
                (cancel, survivor)
            })
        })
        .unwrap();
    wait(
        &mut cx,
        handle,
        &app,
        |a| {
            a.active_runs.len() == 2
                && a.active_runs.iter().all(|active| {
                    a.store
                        .runs
                        .iter()
                        .find(|r| r.id == active.id)
                        .unwrap()
                        .logs
                        .iter()
                        .any(|l| l.contains("CONCURRENCY_LIVE_MARKER"))
                })
        },
        "two running goals before cancellation",
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let task = a.store.research_children(cancel)[0];
            let id = a.store.runs.iter().find(|r| r.task_id == task).unwrap().id;
            a.cancel_run_id(id, cx);
        })
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        let children = a.store.research_children(cancel);
        let runs: Vec<_> = a
            .store
            .runs
            .iter()
            .filter(|r| children.contains(&r.task_id))
            .collect();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "Cancelled");
        assert!(
            a.store.next_research_task(survivor).unwrap().is_none(),
            "Unrelated goal must finish"
        );
    })
    .unwrap();
    // A failed step retries in the original persisted session, then continues its dependents.
    let failed = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                let id = goal(&mut a.store, "QA_FAIL");
                a.run_research_next(id, true, cx);
                id
            })
        })
        .unwrap();
    idle(&mut cx, handle, &app);
    let (session, task) = cx
        .update_window(handle, |_, _, cx| {
            let a = app.read(cx);
            let r = a.store.runs.last().unwrap();
            assert_eq!(r.status, "Failed");
            std::fs::write(Path::new(&r.directory).join("recovered-fail"), "recover").unwrap();
            (
                r.session_id
                    .clone()
                    .expect("Session must be persisted before execution"),
                r.task_id,
            )
        })
        .unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.run_research_next(failed, true, cx))
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        let runs: Vec<_> = a.store.runs.iter().filter(|r| r.task_id == task).collect();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].status, "Succeeded");
        assert_eq!(runs[1].session_id.as_deref(), Some(session.as_str()));
        assert!(a.store.next_research_task(failed).unwrap().is_none());
    })
    .unwrap();
    // A rejected dependency must not prevent a different goal from being queued.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let blocked_goal = goal(&mut a.store, "Blocked dependency feedback");
            let other_goal = goal(&mut a.store, "Independent manual start");
            a.store
                .set_stage(blocked_goal, crate::domain::Stage::Planned)
                .unwrap();
            a.store
                .set_stage(other_goal, crate::domain::Stage::Planned)
                .unwrap();
            let blocked = a.store.research_children(blocked_goal)[1];
            let ready = a.store.research_children(other_goal)[0];
            a.open_task(blocked, w, cx);
            a.start_run(blocked, cx);
            assert!(!a.task_running(blocked));
            assert!(
                a.task_start_feedback
                    .as_ref()
                    .unwrap()
                    .1
                    .contains("prerequisite")
            );
            a.open_task(ready, w, cx);
            a.start_run(ready, cx);
            assert!(a.task_running(ready));
            assert!(a.error.is_empty());
            assert!(a.task_start_feedback.is_none());
            a.start_run(ready, cx);
            assert!(
                a.task_start_feedback
                    .as_ref()
                    .unwrap()
                    .1
                    .contains("already")
            );
        });
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    println!(
        "Concurrency QA passed: 3 CLI sessions, 4 isolated dependency chains, independent cancellation, same-session retry. Artifacts: {}",
        root.display()
    );
}
fn goal(store: &mut Store, title: &str) -> u64 {
    let id = store
        .create_research_goal(title.into(), format!("Isolated concurrency QA. {title}"), 1)
        .unwrap();
    store
        .task_mut(id)
        .unwrap()
        .research
        .as_mut()
        .unwrap()
        .requires_approval = true;
    store.apply_research_plan(id, PLAN).unwrap();
    id
}
fn idle(cx: &mut HeadlessAppContext, h: AnyWindowHandle, a: &Entity<TaskApp>) {
    wait(
        cx,
        h,
        a,
        |a| a.active_runs.is_empty() && a.queued_runs.is_empty(),
        "scheduler idle",
    );
}
fn wait(
    cx: &mut HeadlessAppContext,
    h: AnyWindowHandle,
    a: &Entity<TaskApp>,
    predicate: impl Fn(&TaskApp) -> bool,
    label: &str,
) {
    let start = Instant::now();
    loop {
        let done = cx
            .update_window(h, |_, _, cx| {
                a.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                    assert!(a.active_runs.len() <= 3, "Concurrency limit exceeded");
                });
                predicate(a.read(cx))
            })
            .unwrap();
        if done {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Timed out: {label}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
