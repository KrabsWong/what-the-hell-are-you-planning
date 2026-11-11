//! Isolated end-to-end acceptance for sentence -> understanding -> confirmation -> Backlog -> approved Goal -> report.
use crate::{
    app::{Layout, Scope, TaskApp},
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
from pathlib import Path
prompt = sys.argv[-1]
root = Path(__file__).parent
if 'clarifying ONE saved research request' in prompt:
    marker = root / 'planning-retried'
    if not marker.exists():
        marker.write_text('first attempt failed')
        print('Temporary fixture connection failure', file=sys.stderr)
        sys.exit(1)
    revised = 'Focus on operating results' in prompt
    result = {'summary':'Explain Intel earnings using official data and accessible analyst views. Deliver a sourced report and an independent peer comparison and disclose evidence gaps.' + (' Focus on operating results.' if revised else ''), 'questions':[] if revised or 'Research guidance for the next quarter.' in prompt else ['Should the research focus on operating results or trading decisions?']}
elif 'expanding ONE saved research request' in prompt:
    assert 'Focus on operating results' in prompt or 'Research guidance for the next quarter.' in prompt
    steps = [
        {'title':'Collect official earnings', 'instructions':'Collect official releases and verify period, units and accounting basis.', 'kind':'collect', 'depends_on':[]},
        {'title':'Collect accessible analyst views', 'instructions':'Distinguish original research from media summaries.', 'kind':'collect', 'depends_on':[]},
        {'title':'Analyze fundamentals and expectations', 'instructions':'Compare both evidence sets and distinguish facts from inferences.', 'kind':'analyze', 'depends_on':[0,1]},
        {'title':'Write the final report', 'instructions':'Provide a summary, key data and sources, disclosing evidence gaps.', 'kind':'advise', 'depends_on':[2]}]
    result = {'topic_name':'Intel earnings research','brief':{'objective':'Explain the market reaction after earnings','deliverable':'English report and key data table','scope':'Official earnings and accessible analyst views','assumptions':['Use English by default'],'facts_to_verify':['Verify the claimed earnings date and price decline']}, 'goals':[{'title':'Intel earnings and market reaction','brief':'Explain operating changes and the market reaction.','tasks':steps}, {'title':'Independent peer comparison','brief':'Compare peers independently.','tasks':steps}]}
    if not (root / 'plan-schema-repaired').exists():
        (root / 'plan-schema-repaired').write_text('missing fields fixture')
        for goal in result['goals']:
            for task in goal['tasks']:
                task.pop('kind', None)
                task.pop('depends_on', None)
    elif 'REPAIR THE PREVIOUS PLAN' in prompt:
        assert 'missing field' in prompt and 'previous_output' in prompt
else:
    time.sleep(.65)
    result = {'status':'complete','report':'# Research findings\n\nThis is isolated acceptance data, not real earnings analysis.\n\n[Original source](https://example.com/fixture)','sources':[{'title':'QA fixture source','url':'https://example.com/fixture','published_at':'unknown'}]}
    if 'blocked-once-proof' in prompt and not (root / 'branch-resumed').exists():
        (root / 'branch-resumed').write_text('blocked once')
        result = {'status':'blocked','report':'Fixture source temporarily unavailable; retry after access is restored.','sources':[]}

print(json.dumps({'type':'text','part':{'text':json.dumps(result,ensure_ascii=False)}}),flush=True)
"#;

pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-intake-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    executable(&root.join("opencode-fixture"), CLI);
    executable(
        &root.join("ego-fixture"),
        "#!/bin/sh\nprintf '%s\\n' 'QA browser instructions' '[ego-browser:skill] end of guide'\n",
    );
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name = "Investment research".into();
    store.project_at_mut(0).unwrap().directory = root.join("project").to_string_lossy().into();
    std::fs::create_dir_all(&store.projects()[0].directory).unwrap();
    store.set_opencode_path(root.join("opencode-fixture").to_string_lossy().into());
    store.set_default_model("fixture/research".into());
    store.research_config_mut().ego_path = root.join("ego-fixture").to_string_lossy().into();
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
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.navigate(Scope::Project(1), w, cx);
                a.layout = Layout::Board;
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_,w,cx| {
        app.update(cx, |a, cx| {
            let existing = a.store.create_topic(1, "Existing unrelated research").unwrap();
            a.navigate(Scope::Topic(existing), w, cx);
            a.open_research_intake(w, cx);
            assert_eq!(a.planning_project, Some(1));
            assert_eq!(a.planning_topic, None);
        });
        w.render_frame(cx);
        w.click("project-plan-brief",cx); w.input("Explain the decline after Intel earnings yesterday using official data and analyst views. Deliver an English report and an independent peer comparison.",cx);
        w.click("generate-project-plan",cx);
        assert_eq!(app.read(cx).store.research_requests().len(),1);
        let state = app.read(cx);
        let topic = state.store.research_requests()[0].topic_id.unwrap();
        assert_ne!(state.store.topics().iter().find(|t| t.id == topic).unwrap().name, "Existing unrelated research");
        w.render_frame(cx);
        assert!(w.find(format!("topic-{topic}")).visible());
    }).unwrap();
    wait(&mut cx, handle, &app);
    let timestamp = snapshot(&mut cx, &app).research_requests()[0]
        .submitted_at
        .clone();
    assert!(
        snapshot(&mut cx, &app).research_requests()[0]
            .goal_ids
            .is_empty()
    );
    assert_eq!(snapshot(&mut cx, &app).runs()[0].status, "Failed");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Research, w, cx));
        w.render_frame(cx);
        // Retry saved request without re-entering or duplicating its original input.
        app.update(cx, |a, cx| a.retry_research_request(1, cx));
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    // Understanding is a durable stopping point: no goal plan is generated yet.
    let pending = snapshot(&mut cx, &app);
    assert!(pending.research_requests()[0].goal_ids.is_empty());
    assert!(!root.join("plan-schema-repaired").exists());
    let mut restored = Storage::new(path.clone()).load().unwrap().unwrap();
    assert!(restored.research_requests()[0].goal_ids.is_empty());
    assert!(restored.request_understanding_ready(1));
    assert_eq!(restored.research_requests()[0].understanding.as_ref().unwrap().questions.len(), 1);
    assert!(!restored.request_understanding_confirmed(1));
    assert!(restored.confirm_request_understanding(1).is_err(),
        "An unanswered direction question must prevent confirmation after reload");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("understanding-confirm-1").visible());
    }).unwrap();
    screenshot(&mut cx, handle, &root, "00-understanding-question-light.png");
    cx.update(|cx| component::Theme::change(component::ThemeMode::Dark, None, cx));
    screenshot(&mut cx, handle, &root, "00-understanding-question-dark.png");
    cx.update(|cx| component::Theme::change(component::ThemeMode::Light, None, cx));
    cx.update_window(handle, |_, w, cx| {
        w.click("understanding-edit-1", cx);
        w.render_frame(cx);
        w.click("input-state", cx);
        w.input("Focus on operating results, not trading decisions.", cx);
        w.click("understanding-refine-1", cx);
    }).unwrap();
    wait(&mut cx, handle, &app);
    assert!(snapshot(&mut cx, &app).research_requests()[0].goal_ids.is_empty());
    assert!(!root.join("plan-schema-repaired").exists());
    assert!(snapshot(&mut cx, &app).request_understanding_ready(1));
    assert!(!snapshot(&mut cx, &app).request_understanding_confirmed(1));
    screenshot(&mut cx, handle, &root, "00-understanding-review-light.png");
    cx.update(|cx| component::Theme::change(component::ThemeMode::Dark, None, cx));
    screenshot(&mut cx, handle, &root, "00-understanding-review-dark.png");
    let compact_store = snapshot(&mut cx, &app);
    compact_understanding_screenshots(&mut cx, &compact_store, &root);
    cx.update(|cx| component::Theme::change(component::ThemeMode::Light, None, cx));
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("understanding-confirm-1", cx);
    }).unwrap();
    wait(&mut cx, handle, &app);
    let repaired = snapshot(&mut cx, &app);
    assert_eq!(repaired.runs().iter().filter(|r| r.logs.iter().any(|l| l.starts_with("[plan repair]"))).count(), 1);
    assert!(repaired.runs().iter().any(|r| r.status == "Failed" && r.logs.iter().any(|l| l.contains("missing field `kind`"))));
    let goals = snapshot(&mut cx, &app).research_requests()[0]
        .goal_ids
        .clone();
    assert_eq!(
        goals.len(),
        2,
        "{}",
        cx.update(|cx| app.read(cx).error.clone())
    );
    assert_eq!(
        snapshot(&mut cx, &app).research_requests()[0].submitted_at,
        timestamp
    );
    let goal = goals[0];
    let other = goals[1];
    let children = snapshot(&mut cx, &app).research_children(goal);
    let topic = snapshot(&mut cx, &app).task_topic(goal).unwrap().id;
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::Project(1), w, cx);
            a.layout = Layout::Board;
        });
        w.render_frame(cx);
        for child in &children {
            assert!(
                w.try_find(format!("card-{child}")).is_none(),
                "Internal steps must not be approval cards"
            );
            assert!(app.read(cx).store.prepare_run(*child).is_err());
        }
        assert_eq!(app.read(cx).store.task(goal).unwrap().stage, Stage::Backlog);
        let runs_before_drag = app.read(cx).store.runs().len();
        w.drag_to(format!("card-{goal}"), "column-todo", cx);
        assert!(app.read(cx).store.goal_execution_approved(goal));
        assert!(app.read(cx).store.research_pending_approval(other));
        assert_eq!(
            app.read(cx).store.runs().len(),
            runs_before_drag,
            "Dragging must not execute"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "01-approved-goal-board.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("run-project-todo", cx);
    })
    .unwrap();
    let peak = wait(&mut cx, handle, &app);
    assert!(
        peak >= 2,
        "Independent collection branches should run concurrently; peak={peak}"
    );
    assert_eq!(
        snapshot(&mut cx, &app).task(goal).unwrap().stage,
        Stage::Done,
        "{}",
        cx.update(|cx| app.read(cx).error.clone())
    );
    assert!(
        snapshot(&mut cx, &app)
            .runs()
            .iter()
            .filter(|r| r.project_plan.is_none())
            .all(|r| snapshot(&mut cx, &app).owning_research_goal(r.task_id) == Some(goal))
    );
    let report = *children.last().unwrap();
    assert!(snapshot(&mut cx, &app).research_result(report).is_some());
    let before = snapshot(&mut cx, &app).runs().len();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(goal, w, cx);
        });
        w.render_frame(cx);
        w.click("research-task-result", cx);
        assert_eq!(app.read(cx).scope(), Scope::Runs);
    })
    .unwrap();
    screenshot(&mut cx, handle, &root, "02-final-report.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.store
                .task_mut(children[2])
                .unwrap()
                .description
                .push_str(" Add guidance sensitivity analysis.");
            a.persist_if_changed(true);
            assert_eq!(a.store.task(goal).unwrap().stage, Stage::Backlog);
            assert!(a.store.prepare_run(children[2]).is_err());
            assert_eq!(a.store.runs().len(), before);
            a.change_stage(goal, Stage::Planned, cx);
            a.start_goal_batch(goal, cx);
            let _ = w;
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    assert_eq!(
        snapshot(&mut cx, &app).task(goal).unwrap().stage,
        Stage::Done
    );
    assert_eq!(
        snapshot(&mut cx, &app).runs().len(),
        before + 2,
        "Only changed analysis and its report should rerun"
    );
    // A second request in the same Topic gets a separate durable plan, no project one-shot gate.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            assert!(a.submit_research_request(1, Some(topic), "Research guidance for the next quarter.".into(), cx));
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    assert!(snapshot(&mut cx, &app).research_requests()[1].goal_ids.is_empty());
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| a.save_research_understanding(2, None, cx));
    }).unwrap();
    wait(&mut cx, handle, &app);
    assert_eq!(snapshot(&mut cx, &app).research_requests().len(), 2);
    assert!(
        snapshot(&mut cx, &app).research_requests()[1]
            .goal_ids
            .iter()
            .all(|id| snapshot(&mut cx, &app).task_topic(*id).unwrap().id == topic)
    );
    // A blocked prerequisite must not stop a separate collection branch or release analysis.
    let retry_goal = snapshot(&mut cx, &app).research_requests()[1].goal_ids[0];
    let retry_steps = snapshot(&mut cx, &app).research_children(retry_goal);
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            a.store
                .task_mut(retry_steps[0])
                .unwrap()
                .description
                .push_str(" blocked-once-proof");
            a.change_stage(retry_goal, Stage::Planned, cx);
            a.start_goal_batch(retry_goal, cx);
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    let blocked = snapshot(&mut cx, &app);
    assert!(blocked.research_result(retry_steps[0]).is_none());
    assert!(blocked.research_result(retry_steps[1]).is_some());
    assert!(!blocked.runs().iter().any(|r| r.task_id == retry_steps[2]));
    assert_eq!(blocked.task(retry_goal).unwrap().stage, Stage::InProgress);
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, cx| {
            let batch = a.todo_batch.as_ref().unwrap();
            assert_eq!(batch.state, crate::batch::BatchState::Finished);
            assert!(batch.failed_items.contains(&retry_goal));
            a.start_goal_batch(retry_goal, cx);
        });
    })
    .unwrap();
    wait(&mut cx, handle, &app);
    let resumed = snapshot(&mut cx, &app);
    assert_eq!(resumed.task(retry_goal).unwrap().stage, Stage::Done);
    assert_eq!(
        resumed
            .runs()
            .iter()
            .filter(|r| r.task_id == retry_steps[1])
            .count(),
        1
    );
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |a, _| {
            a.persist_if_changed(true);
            assert!(a.storage_error.is_empty());
        });
    })
    .unwrap();
    let loaded = Storage::new(path).load().unwrap().unwrap();
    assert_eq!(loaded.research_requests().len(), 2);
    assert_eq!(loaded.task(goal).unwrap().stage, Stage::Done);
    // Compact display fixture: check navigation order and disclosure interaction.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.store.task_mut(goal).unwrap().description = "Explain the market reaction after earnings, delivering a sourced research report.".into();
            a.open_task(goal, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("request-brief-content").is_none());
        assert!(w.try_find("request-brief-toggle").is_some());
        let tabs = w.try_find("goal-steps-tab").unwrap().bounds();
        let badge = w.try_find("task-kind-badge").unwrap().bounds();
        assert!(tabs.origin.x < badge.origin.x, "page tabs must precede the type badge");
    }).unwrap();
    screenshot(&mut cx, handle, &root, "03-overview-collapsed.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("request-brief-toggle", cx);
        w.render_frame(cx);
        assert!(w.try_find("request-brief-content").is_some());
    }).unwrap();
    screenshot(&mut cx, handle, &root, "04-overview-expanded.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("request-brief-toggle", cx);
        w.render_frame(cx);
        assert!(w.try_find("request-brief-content").is_none());
        app.update(cx, |a, cx| a.open_task(children[0], w, cx));
        w.render_frame(cx);
        assert!(w.try_find("task-kind-badge").is_some());
        assert!(w.try_find("goal-overview-tab").is_none());
    }).unwrap();
    screenshot(&mut cx, handle, &root, "05-evidence-header.png");
    println!(
        "INTAKE QA PASSED: understanding failure/retry, pending understanding persistence, clarification feedback, explicit confirmation, light/dark understanding screenshots, missing-field auto repair, complete plan, required Topic, Goal-only drag approval, peak concurrency {peak}, final report, reapproval and selective rerun, repeated request, blocked branch/resume, persistence. {}",
        root.display()
    );
}
fn compact_understanding_screenshots(cx: &mut HeadlessAppContext, store: &Store, root: &Path) {
    // Fresh windows avoid retained pane dimensions from the headless resize harness.
    let run = store.research_requests()[0].planning_run_id;
    let window = cx.open_window(size(px(1000.), px(680.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.store = store.clone();
            a.set_result_selection(None, run);
            a.show_run_result = true;
            cx.notify();
        });
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let handle = window.into();
    for (theme, name) in [
        (component::ThemeMode::Light, "00-understanding-review-compact-light.png"),
        (component::ThemeMode::Dark, "00-understanding-review-compact-dark.png"),
    ] {
        cx.update(|cx| component::Theme::change(theme, None, cx));
        screenshot(cx, handle, root, name);
        cx.update_window(handle, |_, w, _| {
            let confirm = w.find("understanding-confirm-1").bounds();
            assert!(confirm.right() <= w.viewport_size().width,
                "Understanding confirmation must remain inside the compact window");
        }).unwrap();
    }
}

fn wait(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, app: &Entity<TaskApp>) -> usize {
    let start = Instant::now();
    let mut peak = 0;
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.poll_runtime(cx));
                let a = app.read(cx);
                peak = peak.max(a.active_runs.len());
                a.active_runs.is_empty() && a.queued_runs.is_empty()
            })
            .unwrap();
        if done {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(45),
            "Fixture run timed out"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    peak
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
fn executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

fn snapshot(cx: &mut HeadlessAppContext, app: &Entity<TaskApp>) -> Store {
    cx.update(|cx| app.read(cx).store.clone())
}
