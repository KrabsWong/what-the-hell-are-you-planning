//! Native research acceptance with isolated files and local process fixtures only.
use crate::{app::TaskApp, attachments, persistence::Storage};
use gpui_kit::{
    component::{Theme, ThemeMode},
    test::TestWindowExt,
    *,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const MODELS: [&str; 4] = [
    "fixture/planner",
    "fixture/collector",
    "fixture/analyst",
    "fixture/adviser",
];
const PLAN: &str = r#"{"tasks":[{"title":"收集原始新闻与公告","instructions":"保存来源、时间和事实，使用 ego(lite) 检索。","kind":"collect","depends_on":[]},{"title":"分析新闻与补充材料","instructions":"核对 PDF 和截图，区分事实与推断。","kind":"analyze","depends_on":[0]},{"title":"形成有条件的投资简报","instructions":"注明风险、催化因素和失效条件。","kind":"advise","depends_on":[1]}]}"#;
const FAKE_CLI: &str = r#"#!/usr/bin/python3
import json, os, pathlib, sys, time
args = sys.argv[1:]
if '--version' in args:
    print('OpenCode QA fixture'); sys.exit(0)
if 'models' in args:
    print('fixture/planner\nfixture/collector\nfixture/analyst\nfixture/adviser'); sys.exit(0)
model = args[args.index('--model') + 1]
prompt = args[-1]
files = [args[i + 1] for i, arg in enumerate(args) if arg == '--file']
assert '--standalone' in args and args[args.index('--format') + 1] == 'json'
assert pathlib.Path(os.getcwd()).resolve() == pathlib.Path(os.environ['PWD']).resolve()
assert not files, 'Binary materials must not be sent to a text-only provider'
if 'Attached material:' in prompt:
    assert 'MEMORY-PDF-7341' in prompt and 'MEMORY-IMAGE-9265' in prompt
    assert 'PDFKit text' in prompt and 'Vision OCR' in prompt
with open('calls.jsonl', 'a', encoding='utf-8') as log:
    log.write(json.dumps({'model': model, 'prompt': prompt, 'files': files}, ensure_ascii=False) + '\n')
print(json.dumps({'type': 'reasoning', 'sessionID': 'fixture-' + model, 'part': {'text': 'LIVE_MARKER ' + model}}), flush=True)
if 'QA_BROWSER_HANDOFF' in prompt:
    output = 'Error: task.listTabs: A browser permission prompt for location access has appeared. The user now controls this task space. Resume only after the user confirms.'
    print(json.dumps({'type': 'tool_use', 'part': {'tool': 'shell', 'state': {'status': 'completed', 'input': {'command': 'ego-browser nodejs < fixture.mjs'}, 'output': output}}}), flush=True)
    time.sleep(2)
    pathlib.Path('handoff-continued').write_text('This must never run after the browser handoff')
    sys.exit(1)
if 'QA_CANCEL' in prompt:
    time.sleep(30)
time.sleep(0.35)
source = {'title': 'QA fixture source — not actual market news', 'url': 'https://example.com/qa-market-release', 'published_at': '2026-10-28T09:00:00+08:00'}
if model == 'fixture/planner':
    result = {'tasks': [
        {'title': '收集原始新闻与公告', 'instructions': '保存来源、时间和事实，使用 ego(lite) 检索。', 'kind': 'collect', 'depends_on': []},
        {'title': '分析新闻与补充材料', 'instructions': '核对 PDF 和截图，区分事实与推断。', 'kind': 'analyze', 'depends_on': [0]},
        {'title': '形成有条件的投资简报', 'instructions': '注明风险、催化因素和失效条件。', 'kind': 'advise', 'depends_on': [1]}]}
elif 'QA_BLOCK' in prompt:
    result = {'status': 'blocked', 'report': 'QA fixture: browser requires user input; chain must stop.', 'sources': []}
else:
    if model == 'fixture/collector':
        assert 'BROWSER INTEGRATION' in prompt and 'Task Manager research' in prompt
        report = '## 已收集证据\nEVIDENCE_SENTINEL\n本内容仅为验收夹具，不是真实新闻。\n[原始来源](https://example.com/qa-market-release)'
    elif model == 'fixture/analyst':
        assert 'EVIDENCE_SENTINEL' in prompt and source['url'] in prompt and 'Source records:' in prompt
        report = '## 分析结果\nANALYSIS_SENTINEL\n已接收前序证据及用户补充材料。此处不构成真实市场判断。\n[证据来源](https://example.com/qa-market-release)'
    elif model == 'fixture/adviser':
        assert 'ANALYSIS_SENTINEL' in prompt and source['url'] in prompt
        report = '## 投资研究简报 · QA fixture\n\n### 条件与风险\n验收内容，不提供实际投资建议。\n\n- 保留来源和时间。\n- 区分事实与推断。\n- 缺少持仓和风险偏好时给出一般性条件分析。\n\n[查看证据来源](https://example.com/qa-market-release)'
    else:
        raise AssertionError('Unexpected routed model: ' + model)
    import re
    for name in re.findall(r'Attached material: (.*?) \(', prompt):
        report += '\nSupplemental material: ' + name + ', page 1: fixture text verified.'
    result = {'status': 'complete', 'report': report, 'sources': [source]}
print(json.dumps({'type': 'text', 'part': {'text': json.dumps(result, ensure_ascii=False)}}), flush=True)
"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/research-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    let ego = root.join("fake-ego-browser");
    write_executable(&cli, FAKE_CLI);
    write_executable(
        &ego,
        "#!/bin/sh\nprintf '%s\\n' 'QA browser guide only' '[ego-browser:skill] end of guide'\n",
    );
    let pdf = root.join("研究材料.pdf");
    let png = root.join("市场截图.png");
    std::fs::write(&pdf, include_bytes!("test-fixtures/supplement.pdf")).unwrap();
    write_png(&png);

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
    });
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(root.join("workspace.json")), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.store.projects[0].directory = root.to_string_lossy().into_owned();
                a.store.opencode_path = cli.to_string_lossy().into_owned();
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();

    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("research", cx);
        w.click("research-settings-toggle", cx);
        for (i, model) in MODELS.iter().enumerate() {
            click_in(w, cx, "research-scroll", &format!("research-model-{i}"));
            w.press("cmd-a", cx);
            w.input(model, cx);
        }
        click_in(w, cx, "research-scroll", "ego-path");
        w.press("cmd-a", cx);
        w.input(ego.to_str().unwrap(), cx);
        click_in(w, cx, "research-scroll", "save-research-settings");
        assert_eq!(
            app.read(cx).store.research_config.models,
            MODELS.map(String::from)
        );
        click_in(w, cx, "research-scroll", "check-ego");
    })
    .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| a.ego_probe.is_none(),
        "local browser guide probe",
    );
    cx.update_window(handle, |_, w, cx| {
        assert!(app.read(cx).ego_status.contains("ready"));
        w.click("research", cx);
        click_in(w, cx, "research-scroll", "research-title");
        w.input("美股科技新闻研究 · 原生验收", cx);
        click_in(w, cx, "research-scroll", "research-brief");
        w.input(
            "研究最近三天科技行业公告；区分事实与推断，注明来源、时间、风险和证伪条件。",
            cx,
        );
        w.scroll(
            "research-scroll",
            ScrollDelta::Pixels(point(px(0.), px(2000.))),
            cx,
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "01-research.png");
    let goal = cx
        .update_window(handle, |_, w, cx| {
            click_in(w, cx, "research-scroll", "create-research");
            let id = app.read(cx).selected.unwrap();
            assert_eq!(
                app.read(cx).store.task(id).unwrap().title,
                "美股科技新闻研究 · 原生验收"
            );
            app.update(cx, |a, cx| {
                a.import_attachments(id, vec![pdf.clone(), png.clone()], cx)
            });
            id
        })
        .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| a.attachment_import.is_none(),
        "material import",
    );
    std::fs::remove_file(&pdf).unwrap();
    std::fs::remove_file(&png).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.task(goal).unwrap().attachments.len(), 2);
        for a in &state.store.task(goal).unwrap().attachments {
            assert!(attachments::resolve(&root, a).unwrap().is_file());
        }
        click_in(w, cx, "detail-scroll", "execute-task");
        assert!(
            (!app.read(cx).active_runs.is_empty() || !app.read(cx).queued_runs.is_empty()),
            "{}",
            app.read(cx).error
        );
    })
    .unwrap();
    assert!(
        wait_idle(&mut cx, handle, &app),
        "Planning must stream before exit"
    );
    let children = cx
        .update_window(handle, |_, w, cx| {
            let state = app.read(cx);
            let ids = state.store.research_children(goal);
            assert_eq!(ids.len(), 3);
            assert_eq!(state.store.runs.last().unwrap().status, "Succeeded");
            assert_eq!(state.selected, Some(goal));
            let saved = Storage::new(root.join("workspace.json"))
                .load()
                .unwrap()
                .unwrap();
            assert_eq!(saved.research_children(goal), ids);
            w.scroll(
                "detail-scroll",
                ScrollDelta::Pixels(point(px(0.), px(2000.))),
                cx,
            );
            ids
        })
        .unwrap();
    screenshot(&mut cx, handle, &root, "02-goal-materials-and-plan.png");

    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "detail-scroll", "research-all");
        assert!(app.read(cx).research_sequences.contains(&goal));
    })
    .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| {
            !a.active_runs.is_empty()
                && a.store
                    .runs
                    .last()
                    .is_some_and(|r| r.logs.iter().any(|l| l.contains("LIVE_MARKER")))
        },
        "streamed collection output",
    );
    cx.update_window(handle, |_, w, cx| w.click("run-technical-details", cx))
        .unwrap();
    screenshot(&mut cx, handle, &root, "03-live-logs.png");
    assert!(
        wait_idle(&mut cx, handle, &app),
        "The task chain must stream before completion"
    );
    cx.update_window(handle, |_, w, cx| {
        let state = app.read(cx);
        assert!(state.research_sequences.is_empty());
        assert_eq!(state.store.runs.len(), 4);
        for (run, model) in state.store.runs.iter().zip(MODELS) {
            assert_eq!(run.model, model);
            assert_eq!(run.status, "Succeeded", "{:?}", run.logs);
            assert_eq!(run.attachments.len(), 2);
        }
        for id in &children {
            assert!(state.store.research_result(*id).is_some());
        }
        if w.try_find("show-result").is_some() {
            w.click("show-result", cx);
        }
        assert!(w.try_find("research-result-scroll").is_some());
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "04-completed-result.png");

    // A user's later comment invalidates the goal context and every dependent result.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.open_task(goal, w, cx));
        click_in(w, cx, "detail-scroll", "task-activity-toggle");
        click_in(w, cx, "detail-scroll", "comment-input");
        w.input("补充：请重点关注事件日期与报道日期的差异。", cx);
        click_in(w, cx, "detail-scroll", "post-comment");
        for id in &children {
            assert!(app.read(cx).store.research_result(*id).is_none());
        }
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(app.read(cx).store.runs.len(), 7);
        assert!(app.read(cx).store.runs[4].prompt.contains("报道日期"));
        assert!(
            children
                .iter()
                .all(|id| app.read(cx).store.research_result(*id).is_some())
        );
    })
    .unwrap();

    // Material on the analysis task invalidates only that task and its descendants.
    let extra = root.join("补充图.png");
    write_png(&extra);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(children[1], w, cx);
            a.import_attachments(children[1], vec![extra.clone()], cx);
        });
    })
    .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| a.attachment_import.is_none(),
        "subtask material import",
    );
    std::fs::remove_file(extra).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let store = &app.read(cx).store;
        assert!(store.research_result(children[0]).is_some());
        assert!(store.research_result(children[1]).is_none());
        assert!(store.research_result(children[2]).is_none());
        assert_eq!(store.next_research_task(goal).unwrap(), Some(children[1]));
        app.update(cx, |a, cx| a.open_task(goal, w, cx));
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.runs.len(), 9);
        assert_eq!(state.store.runs[7].attachments.len(), 3);
        assert!(
            children
                .iter()
                .all(|id| state.store.research_result(*id).is_some())
        );
    })
    .unwrap();
    let calls: Vec<serde_json::Value> = std::fs::read_to_string(
        root.join(".research")
            .join(format!("goal-{goal}"))
            .join("calls.jsonl"),
    )
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
    assert_eq!(calls.len(), 9);
    assert_eq!(calls[0]["model"], MODELS[0]);
    assert_eq!(calls[7]["files"].as_array().unwrap().len(), 0);
    assert!(calls[7]["prompt"].as_str().unwrap().contains("Vision OCR"));
    assert!(
        calls[7]["prompt"]
            .as_str()
            .unwrap()
            .contains("Source records:")
    );
    assert!(
        calls[8]["prompt"]
            .as_str()
            .unwrap()
            .contains("ANALYSIS_SENTINEL")
    );

    // Blockers and cancellation both stop the remainder of a continuous chain.
    let blocked = cx
        .update_window(handle, |_, w, cx| {
            let id = create_fixture_goal(&app, w, cx, "QA_BLOCK");
            click_in(w, cx, "detail-scroll", "research-all");
            id
        })
        .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.runs.len(), 10);
        assert_eq!(state.store.runs.last().unwrap().status, "Blocked");
        assert!(state.research_sequences.is_empty());
        assert_eq!(
            state
                .store
                .runs
                .iter()
                .filter(|r| state.store.research_children(blocked).contains(&r.task_id))
                .count(),
            1
        );
        create_fixture_goal(&app, w, cx, "QA_CANCEL");
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| {
            !a.active_runs.is_empty()
                && a.store
                    .runs
                    .last()
                    .is_some_and(|r| r.logs.iter().any(|l| l.contains("LIVE_MARKER")))
        },
        "cancellable streamed output",
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("cancel-run", cx);
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(app.read(cx).store.runs.len(), 11);
        assert_eq!(app.read(cx).store.runs.last().unwrap().status, "Cancelled");
        create_fixture_goal(&app, w, cx, "QA_BROWSER_HANDOFF");
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    let expected = cx
        .update_window(handle, |_, _, cx| {
            let state = app.read(cx);
            assert_eq!(state.store.runs.len(), 12);
            assert_eq!(state.store.runs.last().unwrap().status, "Blocked");
            assert!(state.research_sequences.is_empty());
            assert!(state.error.contains("browser permission prompt"));
            assert!(
                !Path::new(&state.store.runs.last().unwrap().directory)
                    .join("handoff-continued")
                    .exists()
            );
            assert!(state.storage_error.is_empty());
            // A plan arriving after the user changes its inputs must never create outdated tasks.
            // This is a completion fixture, not another spawned CLI invocation.
            app.update(cx, |a, cx| {
                let stale_goal = a
                    .store
                    .create_research_goal(
                        "QA_STALE_PLAN".into(),
                        "Initial planning context.".into(),
                        1,
                    )
                    .unwrap();
                let mut run = a.store.prepare_run(stale_goal).unwrap();
                run.status = "Succeeded".into();
                run.ended_at = Some(crate::opencode::timestamp());
                run.exit_code = Some(0);
                run.output = PLAN.into();
                let run_id = run.id;
                a.store.runs.push(run);
                a.store
                    .add_comment(stale_goal, "New scope supplied while the plan was running.")
                    .unwrap();
                a.finalize_research_run(run_id);
                assert_eq!(a.store.runs.last().unwrap().status, "Stale");
                assert!(a.store.research_children(stale_goal).is_empty());
                assert!(a.research_sequences.is_empty());
                cx.notify();
                a.store.clone()
            })
        })
        .unwrap();
    assert_eq!(
        Storage::new(root.join("workspace.json"))
            .load()
            .unwrap()
            .unwrap(),
        expected
    );

    // Drop the entire headless application so retained Window/Root entities release the lock.
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    drop(app);
    drop(cx);
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
    });
    let mut reopened = None;
    let small = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(root.join("workspace.json")), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                assert_eq!(a.store, expected);
                for id in &children {
                    assert!(a.store.research_result(*id).is_some());
                }
                for task in &a.store.tasks {
                    for material in &task.attachments {
                        assert!(attachments::resolve(&root, material).unwrap().is_file());
                    }
                }
            });
            reopened = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let reopened = reopened.unwrap();
    cx.update_window(small.into(), |_, w, cx| {
        w.render_frame(cx);
        w.click("research", cx);
        assert_eq!(
            reopened.read(cx).research_models[3]
                .read(cx)
                .value()
                .to_string(),
            MODELS[3]
        );
    })
    .unwrap();
    screenshot(
        &mut cx,
        small.into(),
        &root,
        "05-reopened-small-research.png",
    );
    println!(
        "Research QA passed: Chinese native input, four routed models, managed PDF/image copies, generated persisted plan, streamed dependency chain, source/file handoff, comment/material invalidation and reruns, stale plan rejection, blocked/cancel cleanup, exact workspace reopen and 1440/1000 screenshots. Fixtures only: {}",
        root.display()
    );
}

fn create_fixture_goal(app: &Entity<TaskApp>, w: &mut Window, cx: &mut App, title: &str) -> u64 {
    app.update(cx, |a, cx| {
        let id = a
            .store
            .create_research_goal(title.into(), "Isolated failure-path fixture.".into(), 1)
            .unwrap();
        a.store.apply_research_plan(id, PLAN).unwrap();
        a.open_task(id, w, cx);
        id
    })
}

fn wait_idle(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, app: &Entity<TaskApp>) -> bool {
    let mut streamed = false;
    let started = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                let state = app.read(cx);
                streamed |= state.active_runs.first().is_some_and(|active| {
                    state
                        .store
                        .runs
                        .iter()
                        .find(|r| r.id == active.id)
                        .is_some_and(|r| r.logs.iter().any(|l| l.contains("LIVE_MARKER")))
                });
                state.active_runs.is_empty() && state.queued_runs.is_empty()
            })
            .unwrap();
        if done {
            return streamed;
        }
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "Fixture execution timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    predicate: impl Fn(&TaskApp) -> bool,
    label: &str,
) {
    let started = Instant::now();
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
            started.elapsed() < Duration::from_secs(300),
            "Timed out: {label}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn click_in(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    for _ in 0..24 {
        let bounds = w.find(id.to_string()).bounds();
        if bounds.top() >= px(105.) && bounds.bottom() <= w.viewport_size().height - px(35.) {
            break;
        }
        let delta = if bounds.top() < px(105.) { 220. } else { -220. };
        w.scroll(
            scroll.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
    }
    w.click(id.to_string(), cx);
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}

fn write_executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}

fn write_png(path: &Path) {
    std::fs::write(path, include_bytes!("test-fixtures/supplement.png")).unwrap();
}
