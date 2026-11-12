//! Native research acceptance with isolated files and local process fixtures only.
use crate::{
    app::{Scope, TaskApp},
    attachments,
    persistence::Storage,
};
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
const PLAN: &str = r#"{"tasks":[{"title":"Collect original news and announcements","instructions":"Save sources, times and facts; use ego(lite) retrieval.","kind":"collect","depends_on":[]},{"title":"Analyze news and supplementary materials","instructions":"Check the PDF and screenshots; separate facts from inference.","kind":"analyze","depends_on":[0]},{"title":"Produce a conditional investment brief","instructions":"State risks, catalysts and invalidation conditions.","kind":"advise","depends_on":[1]}]}"#;
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
assert 'BEGIN LOCAL MATERIAL TEXT' not in prompt
session = args[args.index('--session') + 1] if '--session' in args else 'none'
material_marker = pathlib.Path('cli-material-' + session + '.checked')
if 'Attached material:' in prompt and not material_marker.exists():
    assert files, 'Original files must reach the first attempt for this session'
    assert all(pathlib.Path(f).is_file() for f in files)
    assert any(pathlib.Path(f).read_bytes().startswith(b'%PDF-') for f in files)
    assert any(pathlib.Path(f).read_bytes().startswith(b'\x89PNG') for f in files)
    material_marker.write_text('original files delivered')
with open('calls.jsonl', 'a', encoding='utf-8') as log:
    log.write(json.dumps({'model': model, 'prompt': prompt, 'files': files}, ensure_ascii=False) + '\n')
print(json.dumps({'type': 'reasoning', 'sessionID': 'fixture-' + model, 'part': {'text': 'LIVE_MARKER ' + model}}), flush=True)
if 'QA_BROWSER_HANDOFF' in prompt and model == 'fixture/collector':
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
    steps = [
        {'title': 'Collect original news and announcements', 'instructions': 'Save sources, times and facts; use ego(lite) retrieval.', 'kind': 'collect', 'depends_on': []},
        {'title': 'Analyze news and supplementary materials', 'instructions': 'Check the PDF and screenshots; separate facts from inference.', 'kind': 'analyze', 'depends_on': [0]},
        {'title': 'Produce a conditional investment brief', 'instructions': 'State risks, catalysts and invalidation conditions.', 'kind': 'advise', 'depends_on': [1]}]
    if 'expanding ONE saved research request' in prompt:
        result = {'topic_name': 'US technology news research', 'brief': {
            'objective': 'Explain recent US technology news from primary evidence.',
            'deliverable': 'A sourced research brief with facts, uncertainty and risks.',
            'scope': 'Isolated QA fixture for the current request-planning flow.',
            'assumptions': ['The fixture uses a local fake agent.'],
            'facts_to_verify': ['No external facts are required for this acceptance check.']},
            'goals': [{'title': 'US technology news research · native acceptance', 'brief': 'Review the fixture evidence and produce a conditional brief.', 'tasks': steps}]}
    else:
        result = {'tasks': steps}
elif 'QA_REPORTED_HANDOFF' in prompt and model == 'fixture/collector':
    result = {'status':'blocked', 'blocker':'browser_handoff', 'report':'Browser access was handed to the user. Existing evidence incomplete.', 'sources':[]}
elif 'QA_BROWSER_HANDOFF' in prompt or 'QA_REPORTED_HANDOFF' in prompt:
    assert 'BROWSER CONTROL:' in prompt and 'Do not use browser tools' in prompt
    assert 'Browser access was handed to the user' in prompt or 'OFFLINE_GAP_SENTINEL' in prompt
    result = {'status': 'partial', 'report': 'OFFLINE_GAP_SENTINEL: Browser access was handed to the user. No verified prices; conclusions remain unavailable.', 'sources': []}
elif 'QA_BLOCK' in prompt:
    result = {'status': 'blocked', 'blocker': 'browser_unavailable', 'report': 'QA fixture: browser runtime unavailable; chain must stop.', 'sources': []}
else:
    if model == 'fixture/collector':
        assert 'BROWSER INTEGRATION' in prompt and 'What the hell are you planning - research' in prompt
        report = '## Collected evidence\nEVIDENCE_SENTINEL\nThis content is an acceptance fixture, not real news.\n[Original source](https://example.com/qa-market-release)'
    elif model == 'fixture/analyst':
        assert 'EVIDENCE_SENTINEL' in prompt and source['url'] in prompt and 'Source records:' in prompt
        report = '## Analysis\nANALYSIS_SENTINEL\nPrior evidence and user-supplied materials were received. This is not a real market judgement.\n[Evidence source](https://example.com/qa-market-release)'
    elif model == 'fixture/adviser':
        assert 'ANALYSIS_SENTINEL' in prompt and source['url'] in prompt
        report = '## Investment research brief · QA fixture\n\n### Conditions and risks\nAcceptance content only; no real investment advice.\n\n- Retain sources and times.\n- Separate facts from inference.\n- Give a general conditional analysis when holdings and risk preferences are absent.\n\n[View evidence source](https://example.com/qa-market-release)'
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
    let pdf = root.join("research-material.pdf");
    let png = root.join("market-screenshot.png");
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
                a.store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
                a.store.set_opencode_path(cli.to_string_lossy().into_owned());
                a.executable_input.update(cx, |input, cx| {
                    input.set_value(cli.to_string_lossy().into_owned(), w, cx);
                });
                // The one-sentence composer is reached from a project's
                // "Open requests" entry. Seed one pending request so this
                // suite exercises that current navigation path instead of the
                // removed sidebar route.
                a.store
                    .create_research_request(
                        a.store.projects()[0].id,
                        None,
                        "Saved request for the research composer",
                    )
                    .unwrap();
                a.persist_if_changed(true);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    let composer_project = cx.update(|cx| app.read(cx).store.projects()[0].id);

    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("workspace-menu", cx);
        w.render_frame(cx);
        w.within("popup-menu").click(3usize, cx);
        w.within("settings-tabs").click(1usize, cx);
        for (i, model) in MODELS.iter().enumerate() {
            click_in(w, cx, "model-settings-scroll", &format!("research-model-{i}"));
            w.press("cmd-a", cx);
            w.input(model, cx);
            w.press("enter", cx);
        }
        click_in(w, cx, "model-settings-scroll", "ego-path");
        w.press("cmd-a", cx);
        w.input(ego.to_str().unwrap(), cx);
    }).unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(
            app.read(cx).store.research_config().models,
            MODELS.map(String::from)
        );
        click_in(w, cx, "model-settings-scroll", "check-ego");
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
        app.update(cx, |a, cx| a.navigate(Scope::Project(composer_project), w, cx));
        w.render_frame(cx);
        w.click("saved-requests", cx);
        click_in(w, cx, "research-scroll", "research-title");
        w.input("US technology news research · native acceptance", cx);
        click_in(w, cx, "research-scroll", "research-brief");
        w.input(
            "Research the last three days of technology announcements; separate facts from inference and state sources, times, risks and invalidation conditions.",
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
    let request_id = cx
        .update_window(handle, |_, w, cx| {
            click_in(w, cx, "research-scroll", "create-research");
            app.read(cx)
                .store
                .research_requests()
                .last()
                .expect("submitted research request")
                .id
        })
        .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| a.store.research_requests().iter().any(|request| request.id == request_id && request.goal_ids.len() == 1),
        "request plan goal",
    );
    let goal = cx
        .update_window(handle, |_, w, cx| {
            let id = app
                .read(cx)
                .store
                .research_requests()
                .iter()
                .find(|request| request.id == request_id)
                .expect("request")
                .goal_ids[0];
            app.update(cx, |a, cx| a.open_task(id, w, cx));
            assert_eq!(
                app.read(cx).store.task(id).unwrap().title,
                "US technology news research · native acceptance"
            );
            app.update(cx, |a, cx| {
                a.import_attachments(id, vec![pdf.clone()], cx)
            });
            id
        })
        .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| a.attachment_import.is_none() && a.pending_material_job.is_none(),
        "material import",
    );
    cx.update_window(handle, |_, w, cx| {
        if app.read(cx).research_save_is_pending() {
            w.render_frame(cx);
            w.click("research-save-impact-confirm", cx);
            w.render_frame(cx);
        }
        cx.write_to_clipboard(ClipboardItem::new_string("ordinary text".into()));
        app.update(cx, |a, cx| assert!(!a.paste_attachments(goal, cx)));
        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
            ImageFormat::Png, std::fs::read(&png).unwrap(),
        )));
        click_in(w, cx, "detail-scroll", "material-drop-hint");
        w.press("cmd-v", cx);
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.attachment_import.is_none() && a.pending_material_job.is_none(), "clipboard image import");
    cx.update_window(handle, |_, w, cx| {
        if app.read(cx).research_save_is_pending() {
            w.render_frame(cx);
            w.click("research-save-impact-confirm", cx);
            w.render_frame(cx);
        }
    })
    .unwrap();
    assert!(!root.join("material-cache").exists(), "Import must not compile or extract materials");
    std::fs::remove_file(&pdf).unwrap();
    std::fs::remove_file(&png).unwrap();
    cx.update_window(handle, |_, w, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.task(goal).unwrap().attachments.len(), 2);
        for a in &state.store.task(goal).unwrap().attachments {
            assert!(attachments::resolve(&root, a).unwrap().is_file());
        }
        // Materials are part of the approved plan. Approve after importing
        // them, then let the user start the chain explicitly.
        app.update(cx, |a, cx| {
            a.change_stage(goal, crate::domain::Stage::Planned, cx);
            assert!(a.store.goal_execution_approved(goal), "{}", a.error);
            cx.notify();
        });
        w.render_frame(cx);
        assert!(w.try_find("research-all").is_some());
    })
    .unwrap();
    let children = cx
        .update_window(handle, |_, w, cx| {
            let state = app.read(cx);
            let ids = state.store.research_children(goal);
            assert_eq!(ids.len(), 3);
            assert_eq!(state.store.runs().len(), 1, "only planning has run before explicit execution");
            assert_eq!(state.store.runs().last().unwrap().status, "Succeeded");
            assert_eq!(state.selected(), Some(goal));
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
        let state = app.read(cx);
        assert!(
            !state.active_runs.is_empty() || !state.queued_runs.is_empty(),
            "research-all did not start the goal: error={:?} toast={:?} stage={:?} approved={} label={:?}",
            state.error,
            state.toast,
            state.store.task(goal).map(|task| task.stage),
            state.store.goal_execution_approved(goal),
            w.find("research-all").label()
        );
    })
    .unwrap();
    wait_until(
        &mut cx,
        handle,
        &app,
        |a| {
            !a.active_runs.is_empty()
                && a.store
                    .runs()
                    .last()
                    .is_some_and(|r| r.logs.iter().any(|l| l.contains("LIVE_MARKER")))
        },
        "streamed collection output",
    );
    cx.update_window(handle, |_, w, cx| {
        // Background execution no longer navigates. Open the active step and
        // its attempt details the way a user does, which selects the run.
        click_in(w, cx, "detail-scroll", &format!("subtask-{}", children[0]));
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "step-attempt-details");
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "03-live-logs.png");
    assert!(
        wait_idle(&mut cx, handle, &app),
        "The task chain must stream before completion"
    );
    cx.update_window(handle, |_, w, cx| {
        let state = app.read(cx);
        assert!(state.research_sequences.is_empty());
        assert_eq!(state.store.runs().len(), 4);
        for (run, model) in state.store.runs().iter().zip(MODELS) {
            assert_eq!(run.model, model);
            assert_eq!(run.status, "Succeeded", "{:?}", run.logs);
            if run.project_plan.is_some() {
                assert!(run.attachments.is_empty(), "planning does not receive step materials");
            } else {
                assert_eq!(run.attachments.len(), 2);
            }
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
        w.input(
            "Note: focus on the difference between event dates and publication dates.",
            cx,
        );
        click_in(w, cx, "detail-scroll", "post-comment");
        w.render_frame(cx);
        {
            let state = app.read(cx);
            assert!(
                state.research_save_is_pending(),
                "a comment on retained results must open T21 review"
            );
            assert!(
                state.store.task(goal).unwrap().comments.is_empty(),
                "no comment may be written before review"
            );
            for id in &children {
                assert!(
                    state.store.research_result(*id).is_some(),
                    "no result may be invalidated before review"
                );
            }
        }
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.task(goal).unwrap().comments.len(), 1);
        for id in &children {
            assert!(app.read(cx).store.research_result(*id).is_none());
        }
        // Editing the goal invalidates its approved snapshot. Re-approve the
        // edited plan before the next explicit run, as T21 requires.
        app.update(cx, |a, cx| {
            a.change_stage(goal, crate::domain::Stage::Planned, cx);
            assert!(a.store.goal_execution_approved(goal), "{}", a.error);
            cx.notify();
        });
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(app.read(cx).store.runs().len(), 7);
        assert!(app.read(cx).store.runs()[4].prompt.contains("publication dates"));
        assert!(
            children
                .iter()
                .all(|id| app.read(cx).store.research_result(*id).is_some())
        );
    })
    .unwrap();

    // Material on the analysis task invalidates only that task and its descendants.
    let extra = root.join("supplementary-image.png");
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
        |a| a.attachment_import.is_none() && a.pending_material_job.is_none(),
        "subtask material import",
    );
    cx.update_window(handle, |_, w, cx| {
        let before = app.read(cx).store.task(children[1]).unwrap().attachments.len();
        {
            let state = app.read(cx);
            assert!(
                state.research_save_is_pending(),
                "a material on retained results must open T21 review"
            );
            assert_eq!(
                state.store.task(children[1]).unwrap().attachments.len(),
                before,
                "no material may be attached before review"
            );
        }
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).store.task(children[1]).unwrap().attachments.len(),
            before + 1
        );
    })
    .unwrap();
    std::fs::remove_file(extra).unwrap();
    cx.update_window(handle, |_, w, cx| {
        // Adding material to a step invalidates the goal approval. Re-approve
        // first; the unchanged collection result is then reusable, while the
        // edited analysis and its dependent report are not.
        app.update(cx, |a, cx| {
            a.open_task(goal, w, cx);
            a.change_stage(goal, crate::domain::Stage::Planned, cx);
            assert!(a.store.goal_execution_approved(goal), "{}", a.error);
            cx.notify();
        });
        w.render_frame(cx);
        let store = &app.read(cx).store;
        assert!(store.research_result(children[0]).is_some());
        assert!(store.research_result(children[1]).is_none());
        assert!(store.research_result(children[2]).is_none());
        assert_eq!(store.next_research_task(goal).unwrap(), Some(children[1]));
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.runs().len(), 9);
        assert_eq!(state.store.runs()[7].attachments.len(), 3);
        assert!(
            children
                .iter()
                .all(|id| state.store.research_result(*id).is_some())
        );
    })
    .unwrap();
    // Steps use isolated working folders, so each step writes its own call log.
    let mut calls = Vec::new();
    read_calls(&root, &mut calls);
    assert_eq!(calls.len(), 9);
    assert!(calls.iter().any(|call| call["model"] == MODELS[0]));
    let analysis_with_material = calls
        .iter()
        .find(|call| {
            call["model"] == MODELS[2]
                && call["files"].as_array().is_some_and(|files| files.len() == 3)
        })
        .expect("analysis call with three materials");
    assert!(
        analysis_with_material["prompt"]
            .as_str()
            .unwrap()
            .contains("Read the attached file directly")
    );
    assert!(
        analysis_with_material["prompt"]
            .as_str()
            .unwrap()
            .contains("Source records:")
    );
    assert!(
        calls
            .iter()
            .filter(|call| call["model"] == MODELS[3])
            .any(|call| call["prompt"].as_str().unwrap().contains("ANALYSIS_SENTINEL"))
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
        assert_eq!(state.store.runs().len(), 10);
        assert_eq!(state.store.runs().last().unwrap().status, "Blocked");
        assert!(state.research_sequences.is_empty());
        assert_eq!(
            state
                .store
                .runs()
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
                    .runs()
                    .last()
                    .is_some_and(|r| r.logs.iter().any(|l| l.contains("LIVE_MARKER")))
        },
        "cancellable streamed output",
    );
    cx.update_window(handle, |_, w, cx| {
        // Open the active step attempt explicitly; launch no longer navigates.
        let active = app
            .read(cx)
            .active_runs
            .first()
            .expect("active cancellable run")
            .id;
        let task_id = app
            .read(cx)
            .store
            .runs()
            .iter()
            .find(|run| run.id == active)
            .expect("active run")
            .task_id;
        click_in(w, cx, "detail-scroll", &format!("subtask-{task_id}"));
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "step-attempt-details");
        w.render_frame(cx);
        w.click("cancel-run", cx);
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(app.read(cx).store.runs().len(), 11);
        assert_eq!(app.read(cx).store.runs().last().unwrap().status, "Cancelled");
        create_fixture_goal(&app, w, cx, "QA_BROWSER_HANDOFF");
        click_in(w, cx, "detail-scroll", "research-all");
    })
    .unwrap();
    wait_idle(&mut cx, handle, &app);
    let expected = cx
        .update_window(handle, |_, _, cx| {
            let state = app.read(cx);
            assert_eq!(state.store.runs().len(), 14);
            assert_eq!(state.store.runs().last().unwrap().status, "Succeeded");
            assert!(state.research_sequences.is_empty());
            let handoff = &state.store.runs()[11];
            let goal = state.store.owning_research_goal(handoff.task_id).unwrap();
            assert!(state.store.research_browser_handed_off(goal));
            assert!(state.store.research_children(goal).iter().all(|id| state.store.research_result(*id).is_some()));
            assert!(state.store.runs()[12].prompt.contains("BROWSER CONTROL:"));
            assert!(state.store.runs()[13].output.contains("OFFLINE_GAP_SENTINEL"));
            assert!(
                !Path::new(&handoff.directory)
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
                a.store.push_run(run);
                a.store
                    .add_comment(stale_goal, "New scope supplied while the plan was running.")
                    .unwrap();
                a.finalize_research_run(run_id, cx);
                assert_eq!(a.store.runs().last().unwrap().status, "Stale");
                assert!(a.store.research_children(stale_goal).is_empty());
                assert!(a.research_sequences.is_empty());
                // Loading normalizes legacy unmanaged goals into the current
                // approval model. Apply the same normalization before the
                // persisted-store equality check.
                a.store.normalize_topics();
                a.persist_if_changed(true);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
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
                for task in a.store.tasks() {
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
        reopened.update(cx, |a, cx| a.navigate(Scope::Project(composer_project), w, cx));
        w.render_frame(cx);
        w.click("saved-requests", cx);
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
        "Research QA passed: native input, four routed models, managed PDF/image copies, generated persisted plan, streamed dependency chain, source/file handoff, comment/material invalidation and reruns, stale plan rejection, blocked/cancel cleanup, exact workspace reopen and 1440/1000 screenshots. Fixtures only: {}",
        root.display()
    );
}

// Focused lifecycle acceptance independent of legacy settings navigation.
pub fn handoff_progress() {
    let root = std::env::current_dir().unwrap().join(format!(".local/handoff-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cli = root.join("fake-opencode");
    let ego = root.join("ego-browser");
    write_executable(&cli, FAKE_CLI);
    write_executable(&ego, "#!/bin/sh\nexit 0\n");
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
    cx.update(init);
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.attach_storage(Storage::new(root.join("workspace.json")), w, cx);
            a.store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
            a.store.set_opencode_path(cli.to_string_lossy().into_owned());
            a.store.research_config_mut().ego_path = ego.to_string_lossy().into_owned();
            a.store.research_config_mut().models = MODELS.map(String::from);
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle = window.into();
    for title in ["QA_BROWSER_HANDOFF", "QA_REPORTED_HANDOFF", "QA_BLOCK"] {
        let goal = cx.update_window(handle, |_, w, cx| {
            let goal = create_fixture_goal(&app, w, cx, title);
            app.update(cx, |a, cx| a.run_research_next(goal, true, cx));
            goal
        }).unwrap();
        wait_idle(&mut cx, handle, &app);
        cx.update_window(handle, |_, _, cx| {
            let state = app.read(cx);
            let runs: Vec<_> = state.store.runs().iter().filter(|r| state.store.owning_research_goal(r.task_id) == Some(goal)).collect();
            assert!(state.storage_error.is_empty(), "{}", state.storage_error);
            if title == "QA_BLOCK" {
                assert_eq!(runs.len(), 1);
                assert_eq!(runs[0].status, "Blocked");
                assert!(!state.store.research_browser_handed_off(goal));
            } else {
                assert_eq!(runs.len(), 3, "{title}: {:?}", runs.iter().map(|r| (&r.status, &r.logs)).collect::<Vec<_>>());
                assert!(runs.iter().all(|r| r.status == "Succeeded"));
                assert!(state.store.research_browser_handed_off(goal));
                assert!(runs[1].prompt.contains("BROWSER CONTROL:"));
                assert!(runs[2].output.contains("OFFLINE_GAP_SENTINEL"));
                assert!(!Path::new(&runs[0].directory).join("handoff-continued").exists());
                let mut next = state.store.prepare_run(runs[0].task_id).unwrap();
                state.prepare_run_materials(runs[0].task_id, &mut next).unwrap();
                assert!(next.prompt.contains("BROWSER CONTROL:"));
                assert!(!next.prompt.contains("CODEX BROWSER TRANSPORT"));
                assert!(!next.prompt.contains("\nBROWSER INTEGRATION"));
            }
        }).unwrap();
    }
    println!("HANDOFF QA PASSED: tool handoff and reported handoff continue offline through analysis/report; browser runtime failure stops; no browser takeover. {}", root.display());
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
                        .runs()
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

fn read_calls(directory: &Path, calls: &mut Vec<serde_json::Value>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            read_calls(&path, calls);
        } else if path.file_name().is_some_and(|name| name == "calls.jsonl") {
            let text = std::fs::read_to_string(&path).unwrap();
            calls.extend(
                text.lines()
                    .map(|line| serde_json::from_str(line).unwrap()),
            );
        }
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
    let mut reported_pending_state = false;
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
        if !reported_pending_state && started.elapsed() >= Duration::from_secs(2) {
            cx.update_window(handle, |_, _, cx| {
                let state = app.read(cx);
                let materials: Vec<_> = state.store.tasks().iter()
                    .filter(|task| !task.attachments.is_empty())
                    .map(|task| (task.id, task.attachments.len()))
                    .collect();
                eprintln!(
                    "Waiting for {label}: import={}, verification={}, material_edit={}, impact_confirmation={}, cleanup_obligations={}, attached_counts={materials:?}, error={:?}, storage_error={:?}",
                    state.attachment_import.is_some(),
                    state.pending_material_job.is_some(),
                    state.pending_material_edit.is_some(),
                    state.research_save_impact.is_some(),
                    state.pending_cleanup_retry.len(),
                    state.error,
                    state.storage_error,
                );
            }).unwrap();
            reported_pending_state = true;
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


/// Focused acceptance independent of the retired research sidebar navigation.
pub fn attachments_only() {
    let root = std::env::current_dir().unwrap().join(format!(".local/attachment-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(), Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let mut goal = 0;
    let window = cx.open_window(size(px(1440.), px(1000.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.attach_storage(Storage::new(root.join("workspace.json")), w, cx);
            a.store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
            a.store.research_config_mut().models = MODELS.map(String::from);
            goal = a.store.apply_project_plan(1, r#"{"tasks":[{"title":"Attachment QA","brief":"Read supplied materials"}]}"#).unwrap()[0];
            a.store.set_stage(goal, crate::domain::Stage::Planned).unwrap();
            a.open_task(goal, w, cx);
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let handle = window.into();
    let app = app.unwrap();
    cx.update_window(handle, |_, w, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("ordinary text".into()));
        app.update(cx, |a, cx| assert!(!a.paste_attachments(goal, cx)));
        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png,
            include_bytes!("test-fixtures/supplement.png").to_vec())));
        click_in(w, cx, "detail-scroll", "material-drop-hint");
        w.press("cmd-v", cx);
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.attachment_import.is_none() && a.pending_material_job.is_none(), "paste shortcut import");
    cx.update_window(handle, |_, w, cx| {
        if app.read(cx).research_save_is_pending() {
            w.render_frame(cx);
            w.click("research-save-impact-confirm", cx);
            w.render_frame(cx);
        }
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.store.task(goal).unwrap().attachments.len() == 1, "paste shortcut saved");
    let pdf = root.join("original.pdf");
    std::fs::write(&pdf, include_bytes!("test-fixtures/blank.pdf")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        assert!(w.try_find("paste-attachments").is_none());
        click_in(w, cx, "detail-scroll", "material-drop-hint");
        w.press("cmd-v", cx);
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.attachment_import.is_none() && a.pending_material_job.is_none(), "repeat paste import");
    cx.update_window(handle, |_, w, cx| {
        if app.read(cx).research_save_is_pending() {
            w.render_frame(cx);
            w.click("research-save-impact-confirm", cx);
            w.render_frame(cx);
        }
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.store.task(goal).unwrap().attachments.len() == 2, "repeat paste saved");
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "detail-scroll", "material-drop-hint");
        let position = w.find("material-drop-hint").bounds().center();
        w.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Entered {
            position, paths: ExternalPaths(vec![pdf.clone()].into()),
        }), cx);
        w.render_frame(cx);
        w.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Submit { position }), cx);
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.attachment_import.is_none() && a.pending_material_job.is_none(), "file import");
    cx.update_window(handle, |_, w, cx| {
        if app.read(cx).research_save_is_pending() {
            w.render_frame(cx);
            w.click("research-save-impact-confirm", cx);
            w.render_frame(cx);
        }
    }).unwrap();
    wait_until(&mut cx, handle, &app, |a| a.store.task(goal).unwrap().attachments.len() == 3, "file import saved");
    let (run, files) = cx.update_window(handle, |_, _, cx| app.update(cx, |a, _| {
        assert_eq!(a.store.task(goal).unwrap().attachments.len(), 3);
        a.store.set_stage(goal, crate::domain::Stage::Planned).unwrap();
        let mut run = a.store.prepare_run(goal).unwrap();
        let children = a.store.apply_research_plan(goal, PLAN).unwrap();
        a.store.set_stage(goal, crate::domain::Stage::Planned).unwrap();
        let files = a.prepare_run_materials(children[1], &mut run).unwrap();
        assert_eq!(files.len(), 3);
        assert!(!run.prompt.contains("BEGIN LOCAL MATERIAL TEXT"));
        assert!(files.iter().any(|p| std::fs::read(p).unwrap() == include_bytes!("test-fixtures/blank.pdf")));
        assert!(!root.join("material-cache").exists());
        (run, files)
    })).unwrap();
    let cli = root.join("files-cli.py");
    write_executable(&cli, r#"#!/usr/bin/python3
import pathlib,sys
args=sys.argv[1:]
files=[args[i+1] for i,a in enumerate(args) if a=='--file']
assert len(files)==3 and all(pathlib.Path(f).is_file() for f in files)
assert 'Read the attached file directly' in args[-1]
"#);
    let running = crate::opencode::start(crate::opencode::Request { agent: None, browser_executable: None,
        executable: cli, directory: root.clone(), model: MODELS[2].into(), prompt: run.prompt,
        title: "attachment QA".into(), files, session_id: None,
    }).unwrap();
    loop {
        if let crate::opencode::Event::Finished { code, error, .. } = running.events.recv_timeout(Duration::from_secs(10)).unwrap() {
            assert_eq!(code, Some(0), "{error:?}"); break;
        }
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.store.task_mut(goal).unwrap().attachments[0].name = "4c9e204e-a566-4569-9d45-88f28b5c403e.png".into();
            a.store.task_mut(goal).unwrap().attachments[1].name = "a-very-long-screenshot-filename-used-to-verify-that-no-line-is-cut-in-half.png".into();
            cx.notify();
        });
        w.render_frame(cx);
    }).unwrap();
    cx.run_until_parked();
    screenshot(&mut cx, handle, &root, "materials.png");
    cx.update_window(handle, |_, w, cx| {
        w.hover("material-card-0", cx);
        w.render_frame(cx);
        w.click("material-menu-0", cx);
    }).unwrap();
    screenshot(&mut cx, handle, &root, "material-actions.png");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        crate::appearance::apply(crate::appearance::Appearance::Dark, Some(w), cx);
    }).unwrap();
    screenshot(&mut cx, handle, &root, "materials-dark.png");
    // In-flight results show each stage's tasks; completed attempts remain report-first.
    let steps = cx.update_window(handle, |_, w, cx| app.update(cx, |a, cx| {
        let steps = a.store.research_children(goal);
        a.store.set_stage(goal, crate::domain::Stage::Planned).unwrap();
        let mut completed = a.store.prepare_run(steps[0]).unwrap();
        completed.status = "Succeeded".into();
        completed.output = "## Completed evidence\nReadable report fixture.".into();
        a.store.push_run(completed);
        a.store.set_stage(steps[0], crate::domain::Stage::Done).unwrap();
        let mut running = a.store.prepare_run(steps[1]).unwrap();
        running.status = "Running".into();
        let run_id = running.id;
        a.store.push_run(running);
        a.queued_runs.push((run_id, vec![]));
        a.note_selected_run(Some(run_id));
        a.show_run_details = false;
        a.show_run_result = true;
        a.navigate(crate::app::Scope::Runs, w, cx);
        steps
    })).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        for id in &steps { assert!(w.find(format!("progress-task-{id}")).visible()); }
        assert!(w.try_find("research-result-scroll").is_none());
    }).unwrap();
    screenshot(&mut cx, handle, &root, "running-stage-tasks.png");
    // Restart recovery must retain the outline and per-task actions without a local worker.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.queued_runs.clear();
            let run_id = a.selected_run().unwrap();
            a.store.run_mut(run_id).unwrap().status = "Recovering".into();
            cx.notify();
        });
        w.render_frame(cx);
        for id in &steps {
            assert!(w.find(format!("progress-task-{id}")).visible());
            assert!(w.find(format!("progress-task-title-{id}")).visible());
            assert!(w.try_find(format!("progress-task-details-{id}")).is_none());
        }
        assert!(w.try_find("open-run-task").is_none());
        assert!(w.try_find("run-technical-details").is_none());
        assert!(w.try_find("run-resolve-task").is_none());
        assert!(w.try_find(format!("progress-technical-details-{}", steps[2])).is_none());
    }).unwrap();
    screenshot(&mut cx, handle, &root, "recovering-stage-tasks.png");
    cx.update_window(handle, |_, w, cx| {
        let running = app.read(cx).selected_run().unwrap();
        w.click(format!("progress-task-title-{}", steps[0]), cx);
        assert!(app.read(cx).show_run_result);
        assert!(!app.read(cx).show_run_details);
        assert_eq!(app.read(cx).store.runs().iter().find(|r| r.id == app.read(cx).selected_run().unwrap()).unwrap().task_id, steps[0]);
        app.update(cx, |a, cx| { a.show_task_result(running, cx); });
        w.render_frame(cx);
        w.click(format!("progress-task-title-{}", steps[1]), cx);
        assert_eq!(app.read(cx).selected(), Some(steps[1]));
        assert!(app.read(cx).has_return_frames());
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).displayed_run_id(), Some(running),
            "Back from a progress task must restore the originating run");
        assert!(app.read(cx).show_run_result);
        assert!(!app.read(cx).show_run_details);
        app.update(cx, |a, cx| { a.show_task_result(running, cx); });
        w.render_frame(cx);
        assert_eq!(app.read(cx).displayed_run_id(), Some(running),
            "Opening a run must resolve that run after visiting a task");
        w.click(format!("progress-task-title-{}", steps[2]), cx);
        assert_eq!(app.read(cx).selected(), Some(steps[2]));
        app.update(cx, |a, cx| { a.show_task_result(running, cx); });
        w.render_frame(cx);
        assert_eq!(app.read(cx).displayed_run_id(), Some(running),
            "A sibling task selection must not hide the requested run");
    }).unwrap();

    cx.update_window(handle, |_, w, cx| {
        w.click(format!("progress-technical-details-{}", steps[0]), cx);
        assert!(app.read(cx).show_run_details);
        let selected = app.read(cx).selected_run().unwrap();
        assert_eq!(app.read(cx).displayed_run_id(), Some(selected),
            "Technical details must display the run selected by the row");
        assert_eq!(app.read(cx).store.runs().iter().find(|r| r.id == selected).unwrap().task_id, steps[0]);
    }).unwrap();

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.queued_runs.clear();
            a.show_task_result(a.store.runs()[0].id, cx);
            cx.notify();
        });
        w.render_frame(cx);
        assert!(w.find("research-result-scroll").visible());
        for id in &steps { assert!(w.try_find(format!("progress-task-{id}")).is_none()); }
    }).unwrap();
    screenshot(&mut cx, handle, &root, "completed-report.png");
    cx.update_window(handle, |_, w, cx| {
        assert!(w.try_find("copy-result").is_none());
        assert!(w.try_find("open-run-task").is_none());
        assert!(w.try_find("run-technical-details").is_none());
        w.click("run-actions-menu", cx);
    }).unwrap();
    screenshot(&mut cx, handle, &root, "result-actions-menu.png");
    cx.update_window(handle, |_, w, cx| {
        w.within("popup-menu").click(0usize, cx);
        assert!(app.read(cx).show_run_details);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("show-result", cx); }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("run-actions-menu", cx); }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.within("popup-menu").click(3usize, cx);
        assert!(cx.read_from_clipboard().unwrap().text().unwrap().contains("Readable report fixture"));
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| { w.render_frame(cx); w.click("run-actions-menu", cx); }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.within("popup-menu").click(1usize, cx);
        assert!(app.read(cx).selected().is_some());
    }).unwrap();
    println!("ATTACHMENT QA PASSED: paste shortcut, file drop, original bytes, blank PDF, inheritance, CLI --file; no OCR/cache. {}", root.display());
}
