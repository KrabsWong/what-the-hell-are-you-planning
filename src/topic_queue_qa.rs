//! Local-only acceptance for FIFO topic execution and adding work at full capacity.
use crate::{app::TaskApp, batch::BatchState, domain::Store, persistence::Storage};
use gpui_kit::*;
use std::{
    os::unix::fs::PermissionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

const PLAN: &str = r#"{"tasks":[{"title":"Evidence 1","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Evidence 2","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Evidence 3","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0,1,2]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[3]}]}"#;

fn goal(store: &mut Store, topic: u64, title: &str) -> u64 {
    let id = store
        .create_research_goal_in_topic(title.into(), title.into(), 1, Some(topic))
        .unwrap();
    store.apply_research_plan(id, PLAN).unwrap();
    let request = store
        .create_research_request(1, Some(topic), title)
        .unwrap();
    store
        .research_requests
        .iter_mut()
        .find(|r| r.id == request)
        .unwrap()
        .goal_ids
        .push(id);
    id
}

pub fn run() {
    let root = std::env::temp_dir().join(format!("topic-queue-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    std::fs::write(&cli, r#"#!/usr/bin/python3
import json,sys,time
time.sleep(0.7 if 'A2' in sys.argv[-1] else 0.2)
if 'FAIL_TOPIC' in sys.argv[-1]:
    sys.exit(1)
print(json.dumps({'type':'text','part':{'text':json.dumps({'status':'complete','report':'Verified fixture result','sources':[{'title':'Fixture','url':'https://example.com','published_at':'unknown'}]})}}),flush=True)
"#).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    let ego = root.join("fake-ego");
    std::fs::write(
        &ego,
        "#!/bin/sh\nprintf '%s\\n' 'Local QA guide' '[ego-browser:skill] end of guide'\n",
    )
    .unwrap();
    std::fs::set_permissions(&ego, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut store = Store::empty();
    store.projects[0].directory = root.to_string_lossy().into();
    store.opencode_path = cli.to_string_lossy().into();
    store.research_config.ego_path = ego.to_string_lossy().into();
    store.default_model = "fixture/model".into();
    let a = store.create_topic(1, "A").unwrap();
    let b = store.create_topic(1, "B").unwrap();
    let c = store.create_topic(1, "C").unwrap();
    let a1 = goal(&mut store, a, "A1");
    let a2 = goal(&mut store, a, "A2");
    let b1 = goal(&mut store, b, "FAIL_TOPIC");
    let c1 = goal(&mut store, c, "C1");
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1000.), px(720.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |state, cx| {
            state.store.research_config.models[2] = "invalid-model".into();
            state.start_goal_batch(a1, cx);
            state.poll_runtime(cx);
            assert_eq!(state.active_runs.len(), 3);
            state.start_goal_batch(b1, cx);
            state.start_goal_batch(c1, cx);
            state.start_goal_batch(a2, cx); // A2 joins A before already queued B and C.
            state.start_goal_batch(b1, cx); // Duplicate submission is idempotent.
            let batch = state.todo_batch.as_ref().unwrap();
            assert_eq!(batch.items.len(), 4);
            assert_eq!(batch.current_items(&state.store), vec![a1, a2]);
            assert_eq!(state.goal_run_state(b1).0, "Queued");
            assert!(state.error.is_empty(), "{}", state.error);
        })
    })
    .unwrap();
    let mut admission_pause_seen = false;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.run_until_parked();
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |state, cx| {
                    // Keep the production 30-second policy; advance its clock only in this fixture.
                    for run in &mut state.store.runs {
                        if run.status == "Queued" && run.auto_retry_attempt > 0 {
                            run.retry_not_before = Some(0);
                        }
                    }
                    state.poll_runtime(cx);
                    if matches!(state.todo_batch.as_ref().unwrap().state, BatchState::Paused(_)) {
                        assert!(!admission_pause_seen);
                        admission_pause_seen = true;
                        assert!(state.store.runs.iter().all(|r| ![b1, c1].contains(&state.store.owning_research_goal(r.task_id).unwrap())));
                        assert!(state.todo_batch.as_ref().unwrap().failed_items.is_empty());
                        assert!(state.task_start_feedback.as_ref().is_some_and(|(_, error)| error.contains("model")));
                        assert!(state.store.task(a1).unwrap().activity.iter().any(|event| event.contains("queue paused")));
                        state.store.research_config.models[2].clear();
                        state.resume_todo_batch(cx);
                    }
                    assert!(state.active_runs.len() <= 3);
                    let active = state
                        .active_runs
                        .iter()
                        .map(|active| {
                            let run = state.store.runs.iter().find(|r| r.id == active.id).unwrap();
                            state.store.execution_topic(run.task_id)
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        active.iter().all(|topic| Some(topic) == active.first()),
                        "Different topics overlapped"
                    );
                    if state
                        .store
                        .runs
                        .iter()
                        .any(|r| state.store.owning_research_goal(r.task_id) == Some(b1))
                    {
                        assert!(
                            state.store.goal_tasks_complete(a1)
                                && state.store.goal_tasks_complete(a2)
                        );
                    }
                    if state
                        .store
                        .runs
                        .iter()
                        .any(|r| state.store.owning_research_goal(r.task_id) == Some(c1))
                    {
                        assert!(
                            state
                                .todo_batch
                                .as_ref()
                                .unwrap()
                                .failed_items
                                .contains(&b1)
                        );
                    }
                    state.todo_batch.as_ref().unwrap().state == BatchState::Finished
                })
            })
            .unwrap();
        if done {
            break;
        }
        assert!(Instant::now() < deadline, "Topic queue did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |state, cx| {
            assert!(admission_pause_seen, "Analysis submission failure must pause the current topic");
            assert!(state.store.goal_tasks_complete(c1));
            assert_eq!(state.todo_batch.as_ref().unwrap().completed, 3);
            assert_eq!(state.todo_batch.as_ref().unwrap().failed_items, vec![b1]);
            for step in state.store.research_children(b1).into_iter().take(3) {
                assert_eq!(
                    state
                        .store
                        .runs
                        .iter()
                        .filter(|r| r.task_id == step)
                        .count(),
                    4
                );
            }
            // A paused queue may accept more work, but it must not start that work.
            let d = state.store.create_topic(1, "D").unwrap();
            let d1 = goal(&mut state.store, d, "D1");
            state.start_goal_batch(b1, cx);
            state.pause_todo_batch(cx);
            state.start_goal_batch(d1, cx);
            assert!(matches!(
                state.todo_batch.as_ref().unwrap().state,
                BatchState::Paused(_)
            ));
            assert!(
                !state
                    .store
                    .runs
                    .iter()
                    .any(|r| state.store.owning_research_goal(r.task_id) == Some(d1))
            );
            state.cancel_todo_batch(cx);
            state.poll_runtime(cx);
            assert!(state.todo_batch.is_none());
            assert!(state.queued_runs.is_empty());
            assert!(state.active_runs.is_empty());
        })
    })
    .unwrap();
    println!(
        "Topic queue QA passed: append at capacity, grouped FIFO, three slots, retry exhaustion, continuation, admission failure pause/resume, pause and stop."
    );
}
