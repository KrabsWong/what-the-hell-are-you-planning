//! Isolated native project-planning acceptance; --projects-live uses the saved local model.
use crate::{
    app::{Scope, TaskApp},
    domain::{Stage, Store},
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const CLI: &str = r#"#!/usr/bin/python3
import json, sys, time
prompt = sys.argv[-1]
model = sys.argv[sys.argv.index('--model') + 1]
print(json.dumps({'type':'reasoning','part':{'text':'PROJECT_STREAM ' + model}}), flush=True)
time.sleep(.3)
if 'planning research topics for a project' in prompt:
    result = {'tasks':[
        {'title':'存储芯片周期与机会', 'brief':'研究供需、价格及竞争，列出证据需求和风险。'},
        {'title':'先进封装产能与需求', 'brief':'研究供需结构和资本支出，全球市场，未来一年。'},
        {'title':'半导体设备国产替代', 'brief':'研究技术能力与订单变化，需公告和财报佐证。'}]}
elif model == 'qa/planner':
    result = {'tasks':[
        {'title':'采集原始证据', 'instructions':'实际资料来源', 'kind':'collect', 'depends_on':[]},
        {'title':'分析证据', 'instructions':'区分事实与推断', 'kind':'analyze', 'depends_on':[0]},
        {'title':'生成研究报告', 'instructions':'风险与结论', 'kind':'advise', 'depends_on':[1]}]}
else:
    result = {'status':'complete', 'report':'QA flow evidence, not financial research.', 'sources':[{'title':'QA source','url':'https://example.com/qa','published_at':'unknown'}]}
print(json.dumps({'type':'text','part':{'text':json.dumps(result, ensure_ascii=False)}}), flush=True)
"#;

pub fn run(live: bool) {
    let root = std::env::current_dir().unwrap().join(format!(
        ".local/project-{}-qa/{}",
        if live { "live" } else { "native" },
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut initial = Store::empty();
    if live {
        let saved = Storage::new(Storage::default_path().unwrap())
            .load()
            .unwrap()
            .unwrap();
        initial.opencode_path = saved.opencode_path;
        initial.default_model = saved.default_model;
        initial.research_config = saved.research_config;
    } else {
        let cli = root.join("fixture-opencode");
        executable(&cli, CLI);
        let ego = root.join("fixture-ego");
        executable(
            &ego,
            "#!/bin/sh\nprintf '%s\\n' 'QA browser instructions' '[ego-browser:skill] end of guide'\n",
        );
        initial.opencode_path = cli.to_string_lossy().into();
        initial.research_config.models =
            ["qa/planner", "qa/collector", "qa/analyst", "qa/adviser"].map(String::from);
        initial.research_config.ego_path = ego.to_string_lossy().into();
    }
    let path = root.join("workspace.json");
    Storage::new(path.clone()).save(&initial).unwrap();
    let mut cx = context();
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx)
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    let project = cx
        .update_window(handle, |_, w, cx| {
            assert!(app.read(cx).storage_error.is_empty());
            w.render_frame(cx);
            w.click("new-project", cx);
            w.input("半导体行业研究", cx);
            w.click("create-project", cx);
            let id = app.read(cx).store.projects.last().unwrap().id;
            assert_eq!(app.read(cx).planning_project, Some(id));
            w.click("project-plan-brief", cx);
            w.input(
                "聚焦未来一年的全球半导体机会，拆成 3 个可独立研究的方向。不要给出现成投资结论。",
                cx,
            );
            w.click("save-project-context", cx);
            w.click("project-plan-folder", cx);
            w.click("directory-path", cx);
            w.input(root.to_str().unwrap(), cx);
            w.click("save-directory", cx);
            assert_eq!(
                app.read(cx).store.projects.last().unwrap().directory,
                root.to_str().unwrap()
            );
            id
        })
        .unwrap();
    screenshot(&mut cx, handle, &root, "01-planning-light.png");
    cx.update_window(handle, |_, w, cx| w.click("generate-project-plan", cx))
        .unwrap();
    wait(&mut cx, handle, &app);
    let ids = cx
        .update_window(handle, |_, _, cx| {
            let a = app.read(cx);
            assert!(a.error.is_empty(), "{}", a.error);
            assert_eq!(a.store.runs.len(), 1);
            assert_eq!(
                a.store.runs[0].status, "Succeeded",
                "{:?}",
                a.store.runs[0].logs
            );
            assert_eq!(a.scope, Scope::Project(project));
            let ids: Vec<_> = a
                .store
                .tasks
                .iter()
                .filter(|t| t.project_id == project)
                .map(|t| t.id)
                .collect();
            assert!(ids.len() >= 2);
            for id in &ids {
                assert!(a.store.research_pending_approval(*id));
                assert!(a.store.prepare_run(*id).is_err());
            }
            ids
        })
        .unwrap();
    screenshot(&mut cx, handle, &root, "02-pending-kanban.png");
    cx.update_window(handle, |_, w, cx| {
        // Approval is an actual Kanban interaction; no second run is implicitly launched.
        w.click(format!("approve-topic-{}", ids[0]), cx);
        assert_eq!(
            app.read(cx).selected,
            None,
            "Card action must not also open the card"
        );
        assert_eq!(
            app.read(cx).store.task(ids[0]).unwrap().stage,
            Stage::Planned
        );
        assert_eq!(app.read(cx).store.runs.len(), 1);
        w.drag_to(format!("card-{}", ids[1]), "column-todo", cx);
        assert_eq!(
            app.read(cx).store.task(ids[1]).unwrap().stage,
            Stage::Planned
        );
        w.drag_to(format!("card-{}", ids[1]), "column-backlog", cx);
        assert!(app.read(cx).store.research_pending_approval(ids[1]));
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "03-selected-kanban.png");
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("card-{}", ids[0]), cx);
        click(w, cx, "execute-task");
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert!(a.error.is_empty(), "{}", a.error);
        assert_eq!(a.store.runs.len(), 2);
        assert_eq!(a.store.runs.last().unwrap().status, "Succeeded");
        assert!(a.store.research_children(ids[0]).len() >= 3);
        for id in &ids[1..] {
            assert!(a.store.research_children(*id).is_empty());
            assert!(a.store.research_pending_approval(*id));
        }
        assert!(a.store.next_research_task(ids[0]).unwrap().is_some());
    })
    .unwrap();
    if !live {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| a.run_research_next(ids[0], true, cx))
        })
        .unwrap();
        wait(&mut cx, handle, &app);
        cx.update_window(handle, |_, _, cx| {
            assert_eq!(app.read(cx).store.next_research_task(ids[0]).unwrap(), None);
            assert_eq!(app.read(cx).store.runs.len(), 5);
            // Removing approval invalidates results and gates direct child execution too.
            app.update(cx, |a, cx| {
                a.change_stage(ids[0], Stage::Backlog, cx);
                let child = a.store.research_children(ids[0])[0];
                assert!(a.store.prepare_run(child).is_err());
                assert!(a.store.research_result(child).is_none());
                a.approve_research_topic(child, cx);
                assert_eq!(a.store.task(ids[0]).unwrap().stage, Stage::Planned);
                // Completion fixtures exercise business failure paths without another process.
                let baseline = a.store.clone();
                a.store.projects[0].directory = root.to_string_lossy().into();
                let mut stale = a.store.prepare_project_plan(1).unwrap();
                stale.status = "Succeeded".into();
                stale.output =
                    r#"{"tasks":[{"title":"Fresh proposal","brief":"Specific scope"}]}"#.into();
                let rid = stale.id;
                a.store.runs.push(stale);
                a.store.projects[0].research_brief = "Changed while running".into();
                a.finalize_research_run(rid);
                assert_eq!(a.store.runs.last().unwrap().status, "Stale");
                assert_eq!(a.store.tasks.len(), baseline.tasks.len());
                assert!(!a.store.has_project_plan(1));
                for (status, expected) in [
                    ("Succeeded", "Failed"),
                    ("Cancelled", "Cancelled"),
                    ("Failed", "Failed"),
                ] {
                    let mut run = a.store.prepare_project_plan(1).unwrap();
                    run.status = status.into();
                    run.output = "malformed output".into();
                    let rid = run.id;
                    a.store.runs.push(run);
                    a.finalize_research_run(rid);
                    assert_eq!(a.store.runs.last().unwrap().status, expected);
                    assert_eq!(a.store.tasks.len(), baseline.tasks.len());
                }
                a.store = baseline;
                a.error.clear();
            });
        })
        .unwrap();
    }
    let expected = cx
        .update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                a.persist_if_changed(true);
                cx.notify();
            });
            app.read(cx).store.clone()
        })
        .unwrap();
    assert_eq!(
        Storage::new(path.clone()).load().unwrap().unwrap(),
        expected
    );
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    drop(app);
    drop(cx);
    let mut cx = context();
    let mut reopened = None;
    let window = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                assert_eq!(a.store, expected);
                a.open_project_plan(project, w, cx);
            });
            reopened = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let handle = window.into();
    screenshot(&mut cx, handle, &root, "04-reopened-planning-1000.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("close-project-plan", cx);
        w.click("toggle-appearance", cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "05-dark-kanban-1000.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    drop(reopened);
    drop(cx);
    println!(
        "PROJECT {} QA PASSED: project creation, streamed proposals, Kanban approval/drag, unselected gating, approved goal planning, persistence/reopen and light/dark. {}",
        if live { "LIVE" } else { "NATIVE" },
        root.display()
    );
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
fn wait(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, app: &Entity<TaskApp>) {
    let started = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                app.read(cx).active_runs.is_empty() && app.read(cx).queued_runs.is_empty()
            })
            .unwrap();
        if done {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(360),
            "Project run timed out"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn click(w: &mut Window, cx: &mut App, id: &str) {
    w.render_frame(cx);
    for _ in 0..20 {
        let b = w.find(id.to_string()).bounds();
        if b.bottom() < w.viewport_size().height - px(40.) {
            break;
        }
        w.scroll(
            "detail-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-240.))),
            cx,
        );
    }
    w.click(id.to_string(), cx);
}
fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    for _ in 0..5 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
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
