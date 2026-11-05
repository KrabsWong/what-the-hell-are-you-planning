//! Native Todo batch acceptance. Uses isolated workspaces and local process fixtures only.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    batch::{BatchState, TodoBatch},
    domain::{Project, Stage, Store},
    persistence::Storage,
    research::Source,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
const PLAN: &str = r#"{"tasks":[{"title":"Collect fixture evidence","instructions":"Return EVIDENCE_SENTINEL and a source.","kind":"collect","depends_on":[]},{"title":"Analyze fixture evidence","instructions":"Read evidence and return ANALYSIS_SENTINEL.","kind":"analyze","depends_on":[0]},{"title":"Write fixture report","instructions":"Read analysis and preserve sources.","kind":"advise","depends_on":[1]}]}"#;
const CLI: &str = r#"#!/usr/bin/python3
import json,pathlib,sys,time
args=sys.argv[1:]
model=args[args.index('--model')+1]
prompt=args[-1]
root=pathlib.Path.cwd()
with open('calls.jsonl','a') as out: out.write(json.dumps({'model':model,'prompt':prompt})+'\n')
print(json.dumps({'type':'reasoning','part':{'text':'BATCH_LIVE_MARKER'}}),flush=True)
if 'QA_CANCEL' in prompt: time.sleep(30)
time.sleep(0.3)
source={'title':'Isolated QA source, not market research','url':'https://example.com/batch-evidence','published_at':'2026-10-29 QA fixture'}
if model=='fixture/planner':
 result={'tasks':[{'title':'Collect fixture evidence','instructions':'Return EVIDENCE_SENTINEL and a source.','kind':'collect','depends_on':[]},{'title':'Analyze fixture evidence','instructions':'Read evidence and return ANALYSIS_SENTINEL.','kind':'analyze','depends_on':[0]},{'title':'Write fixture report','instructions':'Read analysis and preserve sources.','kind':'advise','depends_on':[1]}]}
elif 'QA_BLOCK' in prompt and not (root/'recovered-block').exists():
 result={'status':'blocked','report':'Fixture browser needs user input.','sources':[]}
else:
 if model=='fixture/collector': report='## Fixture evidence\nEVIDENCE_SENTINEL'
 elif model=='fixture/analyst':
  assert 'EVIDENCE_SENTINEL' in prompt and source['url'] in prompt
  report='## Fixture analysis\nANALYSIS_SENTINEL'
 elif model=='fixture/adviser':
  assert 'ANALYSIS_SENTINEL' in prompt and source['url'] in prompt
  report='## Fixture final report\nDependency context and sources preserved.'
 else: raise AssertionError('Unexpected model: '+model)
 result={'status':'complete','report':report,'sources':[source]}
print(json.dumps({'type':'text','part':{'text':json.dumps(result)}}),flush=True)
"#;
pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/batch-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    executable(&cli, CLI);
    let ego = root.join("fake-ego-browser");
    executable(
        &ego,
        "#!/bin/sh\nprintf '%s\\n' 'Isolated QA browser guide' '[ego-browser:skill] end of guide'\n",
    );
    let path = root.join("workspace.json");
    let mut initial = Store::empty();
    initial.projects[0].name = "半导体行业研究 · 批量验收".into();
    initial.projects[0].directory = root.to_string_lossy().into_owned();
    initial.projects.push(Project {
        id: 2,
        name: "Other project".into(),
        key: "OTHER".into(),
        directory: root.to_string_lossy().into_owned(),
        research_brief: String::new(),
        kind: crate::domain::ProjectKind::User,
    });
    initial.default_model = "fixture/default".into();
    initial.research_config.models = [
        "fixture/planner",
        "fixture/collector",
        "fixture/analyst",
        "fixture/adviser",
    ]
    .map(String::from);
    initial.research_config.ego_path = ego.to_string_lossy().into_owned();
    initial.opencode_path = cli.to_string_lossy().into_owned();
    let first = goal(&mut initial, "Existing planned research", 1);
    let steps = initial.apply_research_plan(first, PLAN).unwrap();
    let mut prior = initial.prepare_run(steps[0]).unwrap();
    prior.status = "Succeeded".into();
    prior.exit_code = Some(0);
    prior.ended_at = Some(crate::opencode::timestamp());
    prior.output = "## Saved fixture evidence\nEVIDENCE_SENTINEL".into();
    prior.sources = vec![Source {
        title: "Existing QA evidence".into(),
        url: "https://example.com/batch-evidence".into(),
        published_at: "2026-10-29 QA fixture".into(),
    }];
    initial.runs.push(prior);
    initial.set_stage(steps[0], Stage::Done).unwrap();
    let second = goal(&mut initial, "Second planned research", 1);
    initial.apply_research_plan(second, PLAN).unwrap();
    let backlog = goal(&mut initial, "Backlog stays untouched", 1);
    initial.set_stage(backlog, Stage::Backlog).unwrap();
    let other = goal(&mut initial, "Other project stays untouched", 2);
    let ordinary = initial
        .create_task("Ordinary Todo fixture", 1, Stage::Planned, None)
        .unwrap();
    Storage::new(path.clone()).save(&initial).unwrap();
    let mut cx = context();
    let mut entity = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.navigate(Scope::Project(1), w, cx);
            });
            entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let handle: AnyWindowHandle = window.into();
    screenshot(&mut cx, handle, &root, "01-idle-project.png");
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            app.read(cx).store.todo_batch_items(1),
            vec![first, second]
        );
        w.render_frame(cx);
        w.click("run-project-todo", cx);
        app.update(cx, |a, cx| {
            assert!(!a.queued_runs.is_empty(), "{}", a.error);
            assert_eq!(
                a.todo_batch.as_ref().unwrap().items,
                vec![first, second]
            );
            a.start_todo_batch(1, cx);
            assert_eq!(
                a.store.runs.len(),
                2,
                "Duplicate click must not submit duplicate jobs"
            );
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app, streamed, "initial streamed output");
    let late = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, _| {
                goal(&mut a.store, "Added after batch started", 1)
            })
        })
        .unwrap();
    screenshot(&mut cx, handle, &root, "02-running-batch.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("pause-todo-batch", cx);
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert!(matches!(
            a.todo_batch.as_ref().unwrap().state,
            BatchState::Paused(_)
        ));
        assert_eq!(
            a.store.runs.len(),
            2,
            "Pause must finish submitted jobs without submitting subsequent steps"
        );
        assert!(a.store.runs.iter().all(|r| r.status == "Succeeded"));
        assert_eq!(a.store.research_children(second).len(), 3);
        assert!(
            !a.store
                .runs
                .iter()
                .any(|r| a.store.research_children(second).contains(&r.task_id))
        );
        assert!(a.store.research_result(steps[1]).is_some());
        assert!(a.store.research_result(steps[2]).is_none());
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "03-paused-batch.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("resume-todo-batch", cx);
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    let finished = cx
        .update_window(handle, |_, _, cx| {
            let a = app.read(cx);
            let b = a.todo_batch.as_ref().unwrap();
            assert!(matches!(b.state, BatchState::Finished), "{}", a.error);
            assert_eq!(b.completed, 2);
            assert_eq!(b.runs_completed, 5);
            assert_eq!(a.store.runs.len(), 6);
            assert!(a.store.runs.iter().all(|r| r.status == "Succeeded"));
            assert_eq!(
                a.store
                    .runs
                    .iter()
                    .filter(|r| r.task_id == steps[0])
                    .count(),
                1
            );
            assert_eq!(a.store.research_children(second).len(), 3);
            assert!(a.store.next_research_task(first).unwrap().is_none());
            assert!(a.store.next_research_task(second).unwrap().is_none());
            assert!(
                !a.store
                    .runs
                    .iter()
                    .any(|r| r.task_id == backlog || r.task_id == other)
            );
            assert_eq!(a.store.task(ordinary).unwrap().stage, Stage::Planned);
            assert_eq!(a.store.todo_batch_items(1), vec![late]);
            assert!(
                !a.store.runs.iter().any(|r| r.task_id == late),
                "A frozen batch must not absorb newly created Todo tasks"
            );
            a.store.clone()
        })
        .unwrap();
    screenshot(&mut cx, handle, &root, "04-finished-batch.png");
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(!a.store.runs.iter().any(|r| r.task_id == ordinary));
            a.store.set_stage(late, Stage::Backlog).unwrap();
            a.cancel_todo_batch(cx);
        })
    })
    .unwrap();
    let blocked = start_goal(&mut cx, handle, &app, "QA_BLOCK");
    idle(&mut cx, handle, &app);
    assert_eq!(cx.update(|cx| app.read(cx).todo_batch.as_ref().unwrap().failed_items.clone()), vec![blocked.0]);
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(
            app.read(cx)
                .store
                .runs
                .iter()
                .filter(|r| r.task_id == blocked.1[0])
                .count(),
            1
        )
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "05-blocked-batch.png");
    let blocked_directory = cx
        .update_window(handle, |_, _, cx| {
            app.read(cx)
                .store
                .execution_directory(blocked.1[0])
                .unwrap()
        })
        .unwrap();
    std::fs::write(
        blocked_directory.join("recovered-block"),
        "browser fixture ready",
    )
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.start_goal_batch(blocked.0, cx))
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        assert!(matches!(
            app.read(cx).todo_batch.as_ref().unwrap().state,
            BatchState::Finished
        ))
    })
    .unwrap();
    let cancelled = start_goal(&mut cx, handle, &app, "QA_CANCEL");
    wait(
        &mut cx,
        handle,
        &app,
        streamed,
        "cancellable streamed output",
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.cancel_todo_batch(cx))
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(a.todo_batch.is_none());
            assert_eq!(a.store.runs.last().unwrap().status, "Cancelled");
            assert!(!a.store.runs.iter().any(|r| r.task_id == cancelled.1[1]));
            archive_goal(a, cancelled.0);
            cx.notify();
        })
    })
    .unwrap();
    let withdrawn = start_goal(&mut cx, handle, &app, "Withdraw approval while running");
    wait(
        &mut cx,
        handle,
        &app,
        streamed,
        "withdrawal streamed output",
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.change_stage(withdrawn.0, Stage::Backlog, cx);
            assert!(!a.error.is_empty());
            assert_eq!(a.store.task(withdrawn.0).unwrap().stage, Stage::InProgress);
        })
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    // Persisting results must succeed before another external task starts.
    let storage_goal = start_goal(&mut cx, handle, &app, "Storage completion failure");
    wait(
        &mut cx,
        handle,
        &app,
        streamed,
        "storage failure streamed output",
    );
    let bad_parent = root.join("not-a-directory");
    std::fs::write(&bad_parent, "fixture").unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| {
            a.storage = Some(Storage::new(bad_parent.join("workspace.json")))
        })
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert!(!a.storage_error.is_empty());
        assert!(matches!(
            a.todo_batch.as_ref().unwrap().state,
            BatchState::Paused(_)
        ));
        assert!(!a.store.runs.iter().any(|r| r.task_id == storage_goal.1[1]));
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.storage = Some(Storage::new(path.clone()));
            a.persist_if_changed(true);
            assert!(a.storage_error.is_empty());
            a.resume_todo_batch(cx);
        })
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let store = &app.read(cx).store;
        assert!(store.goal_tasks_complete(storage_goal.0));
        assert_eq!(store.runs.iter().filter(|r| r.task_id == storage_goal.1[0]).count(), 1,
            "Restoring storage must not repeat successful external work");
    }).unwrap();
    // Pausing during planning leaves the controls on the Runs surface.
    let paused_plan = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.cancel_todo_batch(cx);
                let id = goal(&mut a.store, "Pause during planning", 1);
                a.start_todo_batch(1, cx);
                a.pause_todo_batch(cx);
                id
            })
        })
        .unwrap();
    idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert_eq!(a.scope, Scope::Runs);
        assert_eq!(a.selected, None);
        assert_eq!(a.store.research_children(paused_plan).len(), 3);
        assert!(matches!(
            a.todo_batch.as_ref().unwrap().state,
            BatchState::Paused(_)
        ));
        assert!(
            !a.store
                .runs
                .iter()
                .any(|r| a.store.research_children(paused_plan).contains(&r.task_id))
        );
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.store.set_stage(paused_plan, Stage::Planned).unwrap();
            a.resume_todo_batch(cx);
        })
    })
    .unwrap();
    idle(&mut cx, handle, &app);
    pending_result_checks(&mut cx, handle, &app, &path, &bad_parent);
    let mut expected = cx
        .update_window(handle, |_, _, cx| {
            let a = app.read(cx);
            assert!(matches!(
                a.todo_batch.as_ref().unwrap().state,
                BatchState::Finished
            ));
            assert!(
                a.store
                    .next_research_task(storage_goal.0)
                    .unwrap()
                    .is_none()
            );
            a.store.clone()
        })
        .unwrap();
    expected.normalize_topics();
    assert_eq!(Storage::new(path).load().unwrap().unwrap(), expected);
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    drop(app);
    drop(cx);
    render_variants(&root, &finished, first, second);
    println!(
        "Todo batch QA passed: project scope, frozen Todo snapshot, parent/child deduplication, result reuse, automatic planning, dependency/source handoff, ordinary stage preservation, duplicate start prevention, pause/resume, blocked/withdrawn/cancel and storage-stop recovery, exact reload, Light/Dark × 1440/1000. Fixtures only: {}",
        root.display()
    );
}
fn pending_result_checks(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    path: &Path,
    bad_parent: &Path,
) {
    let (goal_id, steps) = start_goal(cx, handle, app, "Context changes while saving");
    let item = steps[0];
    wait(cx, handle, app, streamed, "pending-result streamed output");
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| {
            a.storage = Some(Storage::new(bad_parent.join("workspace.json")));
        })
    }).unwrap();
    idle(cx, handle, app);
    let manual = cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(!a.storage_error.is_empty());
            assert!(a.store.research_result(item).is_some());
            let batch = a.todo_batch.as_ref().unwrap();
            assert!(matches!(batch.state, BatchState::Paused(_)));
            assert_eq!(batch.run_ids.get(&a.store.runs.last().unwrap().id), Some(&goal_id));
            a.store.add_comment(item, "Include the newly supplied source clarification.").unwrap();
            assert!(a.store.research_result(item).is_none());
            a.storage = Some(Storage::new(path.to_path_buf()));
            a.resume_todo_batch(cx);
            let batch = a.todo_batch.as_ref().unwrap();
            assert!(matches!(&batch.state, BatchState::Paused(reason) if reason.contains("inputs changed")));
            assert_eq!((batch.completed, batch.runs_completed), (0, 0));
            assert!(batch.run_ids.is_empty());
            assert!(a.active_runs.is_empty() && a.queued_runs.is_empty());
            let separate = goal(&mut a.store, "Independent run while queue paused", 2);
            let manual = a.store.apply_research_plan(separate, PLAN).unwrap()[0];
            a.start_run(manual, cx);
            manual
        })
    }).unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.cancel_todo_batch(cx);
            assert!(a.task_running(manual));
        })
    }).unwrap();
    idle(cx, handle, app);
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(a.store.research_result(manual).is_some());
            a.start_goal_batch(goal_id, cx);
        })
    }).unwrap();
    idle(cx, handle, app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert!(a.store.goal_tasks_complete(goal_id));
        assert_eq!(a.store.runs.iter().filter(|r| r.task_id == item).count(), 2);
        assert!(a.store.research_result(item).unwrap().prompt.contains("newly supplied source clarification"));
    }).unwrap();
}

fn goal(store: &mut Store, title: &str, project: u64) -> u64 {
    let id = store
        .create_research_goal(
            title.into(),
            format!("Isolated native batch QA. {title}"),
            project,
        )
        .unwrap();
    id
}
fn start_goal(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    title: &str,
) -> (u64, Vec<u64>) {
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.cancel_todo_batch(cx);
            let id = goal(&mut a.store, title, 1);
            let steps = a.store.apply_research_plan(id, PLAN).unwrap();
            a.start_todo_batch(1, cx);
            assert!(!a.queued_runs.is_empty(), "{}", a.error);
            (id, steps)
        })
    })
    .unwrap()
}
fn archive_goal(app: &mut TaskApp, id: u64) {
    for child in app.store.research_children(id) {
        app.store.archive(child).unwrap();
    }
    app.store.archive(id).unwrap();
}
fn context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    cx
}
fn streamed(app: &TaskApp) -> bool {
    app.active_runs.first().is_some_and(|a| {
        app.store
            .runs
            .iter()
            .find(|r| r.id == a.id)
            .is_some_and(|r| r.logs.iter().any(|l| l.contains("BATCH_LIVE_MARKER")))
    })
}
fn idle(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, app: &Entity<TaskApp>) {
    wait(
        cx,
        handle,
        app,
        |a| a.active_runs.is_empty() && a.queued_runs.is_empty(),
        "batch idle",
    );
}
fn wait(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    predicate: impl Fn(&TaskApp) -> bool,
    label: &str,
) {
    let start = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                predicate(app.read(cx))
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
fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
fn executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}
fn render_variants(root: &Path, store: &Store, first: u64, second: u64) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let dir = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&dir).unwrap();
            let mut saved = store.clone();
            saved.appearance = mode;
            let path = dir.join("workspace.json");
            Storage::new(path.clone()).save(&saved).unwrap();
            let mut cx = context();
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let v = cx.new(|cx| TaskApp::new_mock(w, cx));
                    v.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(
                            a.todo_batch.is_none()
                                && a.active_runs.is_empty()
                                && a.queued_runs.is_empty()
                        );
                        a.navigate(Scope::Project(1), w, cx);
                    });
                    app = Some(v.clone());
                    cx.new(|cx| base::Root::new(v, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle = window.into();
            screenshot(&mut cx, handle, &dir, "idle.png");
            cx.update_window(handle,|_,w,cx|app.update(cx,|a,cx|{a.todo_batch=Some(TodoBatch{failed_tasks:vec![],project_id:1,items:vec![first,second],completed:1,runs_completed:2,state:BatchState::Paused("Paused after the current task. Resume to run the remaining Todo items.".into()),run_ids:Default::default(),done_items:vec![first],failed_items:vec![]});a.navigate(Scope::Runs,w,cx);a.selected_run=a.store.runs.last().map(|r|r.id);a.show_run_result=true;})).unwrap();
            screenshot(&mut cx, handle, &dir, "paused.png");
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    let b = a.todo_batch.as_mut().unwrap();
                    b.completed = 2;
                    b.runs_completed = 5;
                    b.state = BatchState::Finished;
                    cx.notify();
                })
            })
            .unwrap();
            screenshot(&mut cx, handle, &dir, "finished.png");
            cx.update_window(handle, |_, w, _| w.remove_window())
                .unwrap();
        }
    }
}
