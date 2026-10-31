//! Local-only acceptance for delayed queueing, cancellation and batch continuation.
use crate::{
    app::TaskApp,
    batch::{BatchState, TodoBatch},
    domain::{Stage, Store},
    persistence::Storage,
};
use gpui_kit::*;
use std::{
    os::unix::fs::PermissionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-retry-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    std::fs::write(&cli, "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"text\",\"part\":{\"text\":\"Recovered result\"}}'\n").unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut store = Store::empty();
    store.projects[0].directory = root.to_string_lossy().into();
    store.opencode_path = cli.to_string_lossy().into();
    store.default_model = "fixture/model".into();
    for policy in &mut store.policies {
        policy.agent = "OpenCode".into();
    }
    let task = store
        .create_task("Retry acceptance", 1, Stage::Planned, None)
        .unwrap();
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
        app.update(cx, |a, cx| {
            let mut failed = a.store.prepare_run(task).unwrap();
            failed.status = "Failed".into();
            let failed_id = failed.id;
            a.store.runs.push(failed);
            a.complete_research_run(failed_id, cx);
            assert_eq!(a.queued_runs.len(), 1);
            let retry_id = a.queued_runs[0].0;
            a.poll_runtime(cx);
            assert!(
                a.active_runs.is_empty(),
                "Retry must not start before its deadline"
            );
            a.cancel_run_id(retry_id, cx);
            assert!(a.queued_runs.is_empty());
            assert_eq!(a.store.runs.last().unwrap().status, "Cancelled");

            let mut failed = a.store.prepare_run(task).unwrap();
            failed.status = "Failed".into();
            let failed_id = failed.id;
            a.store.runs.push(failed);
            a.todo_batch = Some(TodoBatch {
                project_id: 1,
                items: vec![task],
                completed: 0,
                runs_completed: 0,
                state: BatchState::Running,
                run_ids: [(failed_id, task)].into_iter().collect(),
                done_items: vec![],
                failed_items: vec![],
                failed_tasks: vec![],
            });
            a.complete_research_run(failed_id, cx);
            let retry_id = a.queued_runs[0].0;
            assert!(
                a.todo_batch
                    .as_ref()
                    .unwrap()
                    .run_ids
                    .contains_key(&retry_id)
            );
            assert!(
                !a.todo_batch
                    .as_ref()
                    .unwrap()
                    .run_ids
                    .contains_key(&failed_id)
            );
            a.pause_todo_batch(cx);
            a.store.runs.last_mut().unwrap().retry_not_before = Some(0);
            a.poll_runtime(cx);
            assert!(
                a.active_runs.is_empty(),
                "Paused batches must not dispatch retries"
            );
            a.resume_todo_batch(cx);
            a.poll_runtime(cx);
            assert_eq!(a.active_runs.len(), 1);
        })
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                    a.active_runs.is_empty() && a.queued_runs.is_empty()
                })
            })
            .unwrap();
        if done {
            break;
        }
        assert!(Instant::now() < deadline, "Retry process did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| {
            assert_eq!(a.store.runs.last().unwrap().status, "Succeeded");
            assert_eq!(a.store.runs.last().unwrap().auto_retry_attempt, 1);
            let batch = a.todo_batch.as_ref().unwrap();
            assert_eq!(batch.state, BatchState::Finished);
            assert_eq!(batch.completed, 1);
            assert!(batch.failed_tasks.is_empty());
        })
    })
    .unwrap();
    println!(
        "Retry QA passed: delay, cancellation, paused batch, dispatch, successful continuation."
    );
}
