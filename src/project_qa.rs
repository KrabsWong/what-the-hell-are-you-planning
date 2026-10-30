//! Explicit live acceptance uses saved model settings but writes only an isolated QA workspace.
use crate::{
    app::{Layout, Scope, TaskApp},
    domain::{Stage, Store},
    persistence::Storage,
    research::ResearchKind,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run(live: bool) {
    if !live {
        crate::intake_qa::run();
        return;
    }
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/project-live-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("Create the isolated live QA output directory");
    let saved = Storage::new(Storage::default_path().expect("Locate the configured workspace"))
        .load()
        .expect("Read saved configuration; open the app and complete setup first")
        .expect("No saved workspace; configure OpenCode and a model in the app first");
    let mut initial = Store::empty();
    initial.opencode_path = saved.opencode_path;
    initial.default_model = saved.default_model;
    initial.research_config = saved.research_config;
    initial.projects[0].name = "Live research acceptance".into();
    initial.projects[0].directory = root.join("project").to_string_lossy().into();
    std::fs::create_dir_all(&initial.projects[0].directory).unwrap();
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&initial).unwrap();

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
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.navigate(Scope::Project(1), w, cx);
                a.layout = Layout::Board;
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(a.submit_research_request(1, None,
                "分析 Intel 最近一次已公布季度财报，先核实报告期和公告日期，结合官方披露与可访问的机构观点，输出一份简洁中文报告，说明经营变化、预期差异和资料限制；只规划一个完整研究目标，不做交易。".into(), cx),
                "Request could not be saved: {}", a.error);
        });
    }).unwrap();
    wait(&mut cx, handle, &app, "planning", &root);
    let goals = cx.update(|cx| {
        let a = app.read(cx);
        let request = a
            .store
            .research_requests
            .first()
            .expect("Request was not retained");
        assert!(
            !request.goal_ids.is_empty(),
            "Planning produced no goals: {}. Inspect {}",
            a.error,
            path.display()
        );
        for goal in &request.goal_ids {
            let task = a.store.task(*goal).unwrap();
            assert_eq!(task.stage, Stage::Backlog);
            assert!(
                a.store.task_topic(*goal).is_some(),
                "Every goal needs a Topic"
            );
            assert!(
                a.store.research_children(*goal).len() >= 3,
                "Review must include the full task plan"
            );
            assert!(a.store.research_pending_approval(*goal));
            for child in a.store.research_children(*goal) {
                assert!(
                    a.store.prepare_run(child).is_err(),
                    "Unapproved steps must not run"
                );
            }
        }
        assert!(
            a.store.runs.iter().all(|r| r.project_plan.is_some()),
            "Planning must not execute research"
        );
        request.goal_ids.clone()
    });
    screenshot(&mut cx, handle, &root, "01-complete-backlog-plan.png");
    let goal = goals[0];
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let runs = a.store.runs.len();
            a.change_stage(goal, Stage::Planned, cx);
            assert!(
                a.store.goal_execution_approved(goal),
                "Approval failed: {}",
                a.error
            );
            assert_eq!(
                a.store.runs.len(),
                runs,
                "Approval must not launch research"
            );
            a.start_goal_batch(goal, cx);
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app, "research execution", &root);
    let report = cx.update(|cx| {
        let a = app.read(cx);
        assert_eq!(
            a.store.task(goal).unwrap().stage,
            Stage::Done,
            "Live research did not finish: {}. Check model/browser access and inspect {}",
            a.error,
            path.display()
        );
        assert!(
            a.store
                .runs
                .iter()
                .filter(|r| r.project_plan.is_none())
                .all(|r| a.store.owning_research_goal(r.task_id) == Some(goal)),
            "Only the explicitly approved goal may execute"
        );
        a.store
            .research_children(goal)
            .into_iter()
            .find(|id| {
                a.store
                    .task(*id)
                    .and_then(|t| t.research.as_ref())
                    .is_some_and(|r| r.kind == ResearchKind::Advise)
                    && a.store.research_result(*id).is_some()
            })
            .expect("The completed goal must have a valid final report")
    });
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.view_task_result(report, cx);
            a.persist_if_changed(true);
            assert!(a.storage_error.is_empty(), "{}", a.storage_error);
        });
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "02-final-report.png");
    let expected = cx.update(|cx| app.read(cx).store.clone());
    assert_eq!(Storage::new(path).load().unwrap(), Some(expected));
    println!(
        "PROJECT LIVE QA PASSED: one-sentence request, complete Backlog plan, required Topic, Goal approval, explicit execution, valid report and isolated persistence. {}",
        root.display()
    );
}

fn wait(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    phase: &str,
    root: &Path,
) {
    let started = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                let a = app.read(cx);
                a.active_runs.is_empty() && a.queued_runs.is_empty()
            })
            .unwrap();
        if done {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(360),
            "Live {phase} exceeded 360 seconds. Check OpenCode/browser access and inspect {}",
            root.display()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
