//! Recovery acceptance uses isolated storage and a local CLI fixture, never a live model.
use crate::{
    app::{Scope, TaskApp},
    domain::{Project, Stage, Store},
    persistence::Storage,
    recovery::SessionCheck,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Collect evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze evidence","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write report","kind":"advise","depends_on":[1]}]}"#;
fn executable(path: &Path, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, source).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-recovery-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("opencode-fixture");
    executable(
        &cli,
        r#"#!/usr/bin/python3
import json,pathlib,sys,time
root=pathlib.Path(__file__).parent
args=sys.argv[1:]
if args[:2]==['api','session.active']:
 print(json.dumps({'data':{'ses_running':{'type':'running'}}}));sys.exit(0)
if args[:2]==['session','export']:
 if args[2]=='ses_unknown': print('not JSON');sys.exit(0)
 print((root/(args[2]+'.json')).read_text());sys.exit(0)
with open(root/'executed','a') as f:f.write('run\n')
time.sleep(.1)
result={'status':'complete','report':'Verified fixture output','sources':[{'title':'Fixture evidence','url':'https://example.com/recovery','published_at':'2026-10-30'}]}
print(json.dumps({'type':'text','part':{'text':json.dumps(result)}}),flush=True)
"#,
    );
    let ego = root.join("ego-fixture");
    executable(
        &ego,
        "#!/bin/sh\nprintf '%s\n' 'QA browser guide' '[ego-browser:skill] end of guide'\n",
    );
    let mut store = Store::empty();
    store.opencode_path = cli.to_string_lossy().into_owned();
    store.default_model = "fixture/model".into();
    store.research_config.ego_path = ego.to_string_lossy().into_owned();
    for id in 1..=2 {
        let dir = root.join(format!("project-{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        if id == 1 {
            store.projects[0].directory = dir.to_string_lossy().into_owned();
            store.projects[0].name = "财报研究 · 恢复验收".into();
        } else {
            store.projects.push(Project {
                id,
                name: "行业研究 · 恢复验收".into(),
                key: "OTHER".into(),
                directory: dir.to_string_lossy().into_owned(),
                research_brief: String::new(),
            });
        }
    }
    let mut goals = vec![];
    for (index, session) in ["ses_completed", "ses_stopped", "ses_running", "ses_unknown"]
        .iter()
        .enumerate()
    {
        let project = if index == 1 { 2 } else { 1 };
        let goal = store
            .create_research_goal(if index == 2 { "还原最近一次财报事件，产出可溯源、可复核的财报基本面、市场反应与机构解读综合分析及投资观察框架".into() } else { format!("Recovery goal {index}") }, "Research".into(), project)
            .unwrap();
        let children = store.apply_research_plan(goal, PLAN).unwrap();
        store.set_stage(goal, Stage::InProgress).unwrap();
        let mut run = store.prepare_run(children[0]).unwrap();
        run.id = (index + 2) as u64;
        run.status = "Running".into();
        run.directory = store
            .execution_directory(children[0])
            .unwrap()
            .to_string_lossy()
            .into_owned();
        run.started_at = crate::opencode::timestamp();
        run.session_id = Some(session.to_string());
        std::fs::create_dir_all(&run.directory).unwrap();
        let output=serde_json::json!({"status":"complete","report":"Recovered evidence","sources":[{"title":"Saved fixture","url":"https://example.com/recovery","published_at":"2026-10-30"}]}).to_string();
        let messages = serde_json::json!({"messages":[{"type":"user","text":run.prompt},{"type":"assistant","content":[{"type":"text","text":output}]},{"type":"idle","outcome":if index==0 {"succeeded"} else {"interrupted"}}]});
        std::fs::write(root.join(format!("{session}.json")), messages.to_string()).unwrap();
        store.runs.push(run);
        goals.push((goal, children));
    }
    // A superseded interrupted attempt used to hold the newer attempt's
    // session and directory forever, even after the latest check said Stopped.
    let mut historical = store.runs[1].clone();
    historical.id = 1;
    historical.status = "Interrupted".into();
    store.runs.insert(0, historical);
    let untouched = store
        .create_research_goal("Never started Todo".into(), "Research".into(), 1)
        .unwrap();
    store.apply_research_plan(untouched, PLAN).unwrap();
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        component::Theme::change(component::ThemeMode::Light, None, cx);
    });
    let mut entity = None;
    let window = cx
        .open_window(size(px(1200.), px(900.)), |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.navigate(Scope::Recovery, w, cx);
            });
            entity = Some(app.clone());
            cx.new(|cx| base::Root::new(app, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let handle: AnyWindowHandle = window.into();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                app.read(cx).recovery.checks.len() == 4
                    && app
                        .read(cx)
                        .recovery
                        .checks
                        .values()
                        .all(|v| *v != SessionCheck::Checking)
            })
            .unwrap();
        if done {
            break;
        }
        assert!(Instant::now() < deadline, "startup checks timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::Runs, w, cx);
            a.selected_run = Some(3);
            a.task_result_context = None;
            a.reveal_selected_run();
            a.show_run_details = false;
        });
        w.render_frame(cx);
        assert!(w.try_find("run-project-picker").is_some());
        assert!(w.try_find("run-topic-picker").is_some());
        assert!(w.try_find("run-resolve-task").is_some());
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("interrupted-result-navigation.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("run-resolve-task", cx);
        assert_eq!(app.read(cx).selected, Some(goals[1].1[0]));
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
    }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::Runs, w, cx);
            a.selected_run = Some(2);
            a.task_result_context = None;
        });
        w.render_frame(cx);
        assert!(w.try_find("run-topic-filter").is_some());
        w.click("run-topic-filter", cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.click(1usize, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        let a = app.read(cx);
        let run = a.store.runs.iter().find(|r| Some(r.id) == a.selected_run).unwrap();
        assert_eq!(a.store.task(run.task_id).unwrap().project_id, 1);
        assert_ne!(a.store.task_topic(run.task_id).unwrap().id, a.store.task_topic(goals[0].0).unwrap().id);
        w.render_frame(cx);
        w.click("run-topic-picker", cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        assert!(w.try_find(0usize).is_some());
        assert!(w.try_find(1usize).is_none(), "Goal menu must contain only the selected topic's goal");
        w.click(0usize, cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
    }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let a = app.read(cx);
        assert!(
            a.store.research_result(goals[0].1[0]).is_some(),
            "completed background output must be imported"
        );
        assert_eq!(a.recovery.checks[&4], SessionCheck::Running);
        assert_eq!(
            a.store.step_state(goals[2].1[0]),
            crate::goal_steps::StepState::Running
        );
        assert!(matches!(a.recovery.checks[&5], SessionCheck::Unknown(_)));
        assert_eq!(a.store.runs.len(), 5, "startup must never launch research");
        assert_eq!(a.goal_run_state(goals[1].0).0, "Resume goal");
        assert!(!a.goal_run_state(goals[1].0).1);
        assert_eq!(a.goal_run_state(goals[2].0).0, "Running…");
        assert!(a.goal_run_state(goals[2].0).1);
        assert_eq!(a.goal_run_state(untouched).0, "Run goal");

        assert!(!root.join("executed").exists());
        app.update(cx, |a, cx| a.open_goal_steps(goals[1].0, w, cx));
        w.render_frame(cx);
        assert!(w.try_find("goal-resume").is_some());
        assert!(
            app.read(cx)
                .store
                .step_block_reason(goals[1].1[0])
                .unwrap()
                .contains("interrupted")
        );
        assert!(
            app.read(cx)
                .store
                .step_block_reason(goals[1].1[1])
                .unwrap()
                .contains("Waiting for")
        );
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        let runs_before_navigation = app.read(cx).store.runs.len();
        let topic = app.read(cx).store.task_topic(goals[0].0).unwrap().id;
        w.click(("recovery-project", 1u64), cx);
        assert_eq!(app.read(cx).scope, Scope::Project(1));
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        w.click(format!("recovery-topic-{topic}"), cx);
        assert_eq!(app.read(cx).scope, Scope::Topic(topic));
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        w.click(("recovery-goal", goals[0].0), cx);
        assert_eq!(app.read(cx).selected, Some(goals[0].0));
        assert_ne!(app.read(cx).goal_steps_tab, Some(goals[0].0));
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        w.click(("recovery-step", goals[0].1[0]), cx);
        assert_eq!(app.read(cx).selected, Some(goals[0].1[0]));
        assert_eq!(app.read(cx).store.runs.len(), runs_before_navigation);
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        for layout in [crate::app::Layout::List, crate::app::Layout::Board] {
            app.update(cx, |a, cx| {
                a.open_goal_steps(goals[1].0, w, cx);
                a.steps_layout = layout;
            });
            w.render_frame(cx);
            w.click(format!("goal-step-{}", goals[1].1[0]), cx);
            w.render_frame(cx);
            assert!(w.try_find("step-execution-reason").is_some());
            assert!(w.try_find("step-run").is_some());
        }
        app.update(cx, |a, cx| a.open_task(goals[3].1[0], w, cx));
        w.render_frame(cx);
        assert!(w.try_find("step-check-session").is_some());
        w.click("step-run", cx);
        assert_eq!(app.read(cx).store.runs.len(), runs_before_navigation);
        // All entry points share the same detail panel. Dependency blockers cannot launch.
        app.update(cx, |a, cx| a.open_task(goals[1].1[1], w, cx));
        w.render_frame(cx);
        assert!(w.try_find("step-execution-panel").is_some());
        assert!(w.try_find("step-execution-reason").is_some());
        w.click("step-run", cx);
        assert_eq!(app.read(cx).store.runs.len(), runs_before_navigation);
        w.click(format!("step-prerequisite-{}", goals[1].1[0]), cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected, Some(goals[1].1[0]));
        assert!(w.try_find("step-attempt-details").is_some());
        let execution = w.try_find("step-execution-panel").unwrap().bounds();
        let properties = w.try_find("task-properties-heading").unwrap().bounds();
        assert!(execution.origin.x >= properties.origin.x);
        assert!(execution.origin.y + execution.size.height <= properties.origin.y);

    })
    .unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("blocked-step-actions.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("step-run", cx);
        assert_eq!(app.read(cx).store.runs.len(), 6, "one step retry must launch exactly one attempt");
        app.update(cx, |a, cx| a.open_task(goals[1].1[0], w, cx));
        w.render_frame(cx);
        w.click("step-run", cx);
        assert_eq!(app.read(cx).store.runs.len(), 6, "running step cannot launch twice");
    }).unwrap();
    let retry_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let done = cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.poll_runtime(cx));
            w.render_frame(cx);
            app.read(cx).store.research_result(goals[1].1[0]).is_some()
        }).unwrap();
        if done { break; }
        assert!(Instant::now() < retry_deadline, "step retry did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Recovery, w, cx));
        w.render_frame(cx);
        w.click("resume-all-projects", cx);
    }).unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("paused-projects.png"))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                let a = app.read(cx);
                a.store.goal_tasks_complete(goals[0].0) && a.store.goal_tasks_complete(goals[1].0)
            })
            .unwrap();
        if done {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "resume timed out: {}",
            cx.update(|cx| app.read(cx).error.clone())
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert_eq!(
            a.store.runs.len(),
            10,
            "two projects should run only five unfinished steps"
        );
        assert!(
            !a.store
                .runs
                .iter()
                .any(|r| a.store.owning_research_goal(r.task_id) == Some(untouched))
        );
        assert_eq!(
            a.store
                .runs
                .iter()
                .filter(|r| r.task_id == goals[0].1[0])
                .count(),
            1,
            "recovered evidence must not repeat"
        );
        assert_eq!(a.goal_run_state(goals[0].0).0, "Completed");
        assert!(a.goal_run_state(goals[0].0).1);

        assert_eq!(
            a.store
                .runs
                .iter()
                .filter(|r| r.task_id == goals[2].1[0] || r.task_id == goals[3].1[0])
                .count(),
            2,
            "busy and unknown sessions must not repeat"
        );
    })
    .unwrap();
    let planning_run = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                let request = a
                    .store
                    .create_research_request(
                        2,
                        None,
                        "INTC 刚刚发布财报后暴跌：分析财报数据、市场预期与投资观察框架",
                    )
                    .unwrap();
                let mut run = a.store.prepare_request_plan(request).unwrap();
                run.status = "Interrupted".into();
                let id = run.id;
                a.store
                    .research_requests
                    .iter_mut()
                    .find(|r| r.id == request)
                    .unwrap()
                    .planning_run_id = Some(id);
                a.store.runs.push(run);
                a.navigate(Scope::Recovery, w, cx);
                id
            })
        })
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            w.try_find(("recovery-request", planning_run)).is_some(),
            "planning-only projects must show their specific request"
        );
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("recovery-hierarchy.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click(("recovery-request", planning_run), cx);
        assert_eq!(app.read(cx).selected_run, Some(planning_run));
        assert_eq!(app.read(cx).scope, Scope::Runs);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.task_result_context = None;
            a.show_run_details = false;
            cx.notify();
        });
        w.render_frame(cx);
        assert!(w.try_find("run-planning-toggle").is_none());
        w.click("run-project-planning", cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.click(0usize, cx);
        assert_eq!(app.read(cx).selected_run, Some(planning_run));
    }).unwrap();
    // Archiving a finished batch item must not prematurely finish the remaining goal.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let finished = a.store.create_research_goal("Finished batch item".into(), "Brief".into(), 1).unwrap();
            let remaining = a.store.create_research_goal("Remaining batch item".into(), "Brief".into(), 1).unwrap();
            a.todo_batch = Some(crate::batch::TodoBatch {
                project_id: 1, items: vec![finished, remaining], completed: 1,
                runs_completed: 1, state: crate::batch::BatchState::Running,
                run_ids: Default::default(), done_items: vec![finished],
                failed_items: vec![], failed_tasks: vec![finished],
            });
            a.confirm_archive(crate::archive_app::ArchiveTarget::Task(finished), cx);
            let batch = a.todo_batch.as_ref().unwrap();
            assert_eq!(batch.items, vec![remaining]);
            assert!(batch.done_items.is_empty());
            assert!(batch.failed_tasks.is_empty());
            assert_eq!(batch.completed, 0);
            assert_eq!(batch.state, crate::batch::BatchState::Running);
            a.todo_batch = None;
        });
    }).unwrap();
    // Archive cancellation uses only this fixture; never a live OpenCode session.
    executable(&cli, r#"#!/usr/bin/python3
import json,pathlib,sys,time
args=sys.argv[1:]
if args[:2] == ['api','session.interrupt']:
    assert 'resume=false' in args
    pathlib.Path(__file__).with_suffix('.interrupt').write_text(' '.join(args))
    print(json.dumps({'data':{'interrupted':True}}))
else:
    time.sleep(20)
"#);
    let archived_goal = cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let goal = a.store.create_research_goal("Archive cancellation".into(), "Check cancellation".into(), 1).unwrap();
            a.store.apply_research_plan(goal, PLAN).unwrap();
            let topic = a.store.task_topic(goal).unwrap().id;
            let step = a.store.research_children(goal)[0];
            a.start_run(step, cx);
            a.poll_runtime(cx);
            assert!(a.task_running(step));
            assert!(!a.active_runs.is_empty(), "Fixture must exercise a dispatched process");
            let queued_goal = a.store.create_research_goal_in_topic("Queued related goal".into(), "Wait".into(), 1, Some(topic)).unwrap();
            let mut queued = a.store.prepare_run(queued_goal).unwrap();
            queued.status = "Queued".into();
            queued.started_at.clear();
            let queued_id = queued.id;
            a.store.runs.push(queued);
            a.queued_runs.push((queued_id, vec![]));
            let other = a.store.create_research_goal("Unrelated topic".into(), "Must retain queue".into(), 1).unwrap();
            let mut unrelated = a.store.prepare_run(other).unwrap();
            unrelated.status = "Queued".into();
            unrelated.started_at.clear();
            let other_run = unrelated.id;
            a.store.runs.push(unrelated);
            a.queued_runs.push((other_run, vec![]));
            a.confirm_archive(crate::archive_app::ArchiveTarget::Topic(topic), cx);
            assert!(a.store.task_is_archived(step));
            assert!(a.store.prepare_run(step).is_err());
            assert!(a.store.ready_research_tasks(goal).is_err());
            assert!(!a.queued_runs.iter().any(|(id, _)| *id == queued_id));
            assert!(a.queued_runs.iter().any(|(id, _)| *id == other_run));
            assert_eq!(a.store.runs.iter().find(|r| r.id == queued_id).unwrap().status, "Cancelled");
            let before = a.store.runs.len();
            a.start_run(step, cx);
            a.start_goal_batch(goal, cx);
            assert_eq!(before, a.store.runs.len());
            // Keep the unrelated fixture queued without executing it in this acceptance test.
            a.queued_runs.retain(|(id, _)| *id != other_run);
            goal
        })
    }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let stopped = cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| a.poll_runtime(cx));
            !app.read(cx).goal_busy(archived_goal)
        }).unwrap();
        if stopped && cli.with_extension("interrupt").exists() { break; }
        assert!(Instant::now() < deadline, "Archive did not stop its session");
        std::thread::sleep(Duration::from_millis(20));
    }
    println!(
        "Recovery QA passed: startup import, busy/unknown exclusion, explicit multi-project resume, no unstarted Todo execution; screenshots: {}",
        root.display()
    );
}
