//! Headless native rendering and interaction smoke tests. Run on macOS with --features qa -- --qa.
use crate::{app::TaskApp, domain::Stage};
use gpui_kit::{
    component::{Theme, ThemeMode},
    test::TestWindowExt,
    *,
};
use std::sync::Arc;

pub fn run() {
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
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    std::fs::create_dir_all(".local/qa").unwrap();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/01-list.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("task-row-1", cx);
        assert_eq!(app.read(cx).selected, Some(1));
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/02-detail.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("navigation-back", cx);
        w.click("board-view", cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/03-board.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.drag_to("card-9", "column-todo", cx);
        assert_eq!(app.read(cx).store.task(9).unwrap().stage, Stage::Planned);
        w.click("undo", cx);
        assert_eq!(app.read(cx).store.task(9).unwrap().stage, Stage::Backlog);
        w.right_click("card-9", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.try_find("popup-menu").is_some());
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/06-context.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("model-settings", cx);
        click_scrolled(w, cx, "model-settings-scroll", "global-task-overrides");
        w.click("policy-review", cx);
        w.click("agent-OpenCode", cx);
        click_scrolled(w, cx, "workflow-scroll", "policy-model");
        w.press("cmd-a", cx);
        w.input("local-review-model", cx);
        click_scrolled(w, cx, "workflow-scroll", "save-policy");
        let p = app
            .read(cx)
            .store
            .policies
            .iter()
            .find(|p| p.stage == Stage::Review)
            .unwrap();
        assert_eq!(p.agent, "OpenCode");
        assert_eq!(p.model, "local-review-model");
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/04-workflow.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("project-1", cx);
        w.click("new-task", cx);
        w.input("QA — 原生输入测试", cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/05-create.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("save-task", cx);
        let state = app.read(cx);
        let task = state.store.task(state.selected.unwrap()).unwrap();
        assert_eq!(task.title, "QA — 原生输入测试");
        w.click("undo", cx);
        assert!(app.read(cx).selected.is_none());
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("model-settings", cx);
        click_scrolled(w, cx, "model-settings-scroll", "global-task-overrides");
        click_scrolled(w, cx, "workflow-scroll", "policy-model");
        w.press("cmd-k", cx);
        assert!(
            w.try_find("task-search").is_some(),
            "Search shortcut should reveal the collection"
        );
        w.input("status:review", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            app.read(cx)
                .visible_tasks()
                .iter()
                .all(|t| t.stage == Stage::Review)
        );
        assert!(!app.read(cx).visible_tasks().is_empty());
        w.press("cmd-a", cx);
        w.press("backspace", cx);
        w.click("cycle-0", cx);
        w.click("new-task", cx);
        w.input("Upcoming work", cx);
        w.click("save-task", cx);
        let id = app.read(cx).selected.unwrap();
        assert_eq!(
            app.read(cx).store.task(id).unwrap().cycle.as_deref(),
            Some("Cycle 1")
        );
        w.click("task-actions-menu", cx);
        w.click(0usize, cx);
        w.click("draft-stage-review", cx);
        w.click("save-task", cx);
        assert_eq!(app.read(cx).store.task(id).unwrap().stage, Stage::Review);
        assert!(w.try_find("simulate-run").is_none());
        w.click("runs", cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(".local/qa/07-runs.png")
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("new-project", cx);
        w.input("QA Project", cx);
        w.click("create-project", cx);
        w.click("close-project-plan", cx);
        assert_eq!(
            app.read(cx).store.projects.last().unwrap().name,
            "QA Project"
        );
        w.click("undo", cx);
        assert_eq!(app.read(cx).scope, crate::app::Scope::All);
        w.click("project-2", cx);
        w.click("list-view", cx);
        w.click("task-row-16", cx);
        w.click("add-subtask", cx);
        w.input("Adapter child", cx);
        assert!(w.try_find("draft-due").is_none());
        w.click("save-task", cx);
        let id = app.read(cx).selected.unwrap();
        assert_eq!(app.read(cx).store.task(id).unwrap().project_id, 2);
        assert_eq!(app.read(cx).store.task(id).unwrap().parent_id, Some(16));
        // This preview has no storage. Dismiss its explicit not-saved notice before continuing.
        w.click("dismiss-save-feedback", cx);
        w.click("task-activity-toggle", cx);
        w.click("comment-input", cx);
        w.input("Checked the handoff", cx);
        w.click("post-comment", cx);
        assert_eq!(
            app.read(cx)
                .store
                .task(id)
                .unwrap()
                .comments
                .last()
                .unwrap()
                .body,
            "Checked the handoff"
        );
        w.click("task-actions-menu", cx);
        w.click(6usize, cx);
        assert!(app.read(cx).store.task(id).unwrap().archived);
        w.click("archive", cx);
        w.click(format!("task-row-{id}"), cx);
        w.click("task-actions-menu", cx);
        w.click(6usize, cx);
        assert!(!app.read(cx).store.task(id).unwrap().archived);
    })
    .unwrap();
    let small = cx
        .open_window(size(px(1000.), px(680.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    cx.update_window(small.into(), |_, w, cx| w.render_frame(cx))
        .unwrap();
    cx.capture_screenshot(small.into())
        .unwrap()
        .save(".local/qa/08-small.png")
        .unwrap();
    cx.update_window(small.into(), |_, w, cx| {
        w.click("model-settings", cx);
        click_scrolled(w, cx, "model-settings-scroll", "global-task-overrides");
    })
    .unwrap();
    cx.capture_screenshot(small.into())
        .unwrap()
        .save(".local/qa/09-small-workflow.png")
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::current_dir()
            .unwrap()
            .join(format!(".local/qa/runtime-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("opencode-fixture");
        std::fs::write(&script,"#!/bin/sh\necho '{\"type\":\"text\",\"part\":{\"text\":\"streaming before cancel\"}}'\nsleep 20\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        crate::persistence::Storage::new(root.join("workspace.json"))
            .save(&crate::domain::Store::mock())
            .unwrap();
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.attach_storage(
                    crate::persistence::Storage::new(root.join("workspace.json")),
                    w,
                    cx,
                );
                a.store.projects[0].directory = root.to_string_lossy().into_owned();
                a.store.opencode_path = script.to_string_lossy().into_owned();
                let policy = a
                    .store
                    .policies
                    .iter_mut()
                    .find(|p| p.stage == Stage::InProgress)
                    .unwrap();
                policy.agent = "OpenCode".into();
                policy.model = "fixture/model".into();
                a.open_task(1, w, cx);
            });
            w.render_frame(cx);
            w.click("execute-task", cx);
            assert!((!app.read(cx).active_runs.is_empty() || !app.read(cx).queued_runs.is_empty()));
        })
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.poll_runtime(cx);
            });
            w.render_frame(cx);
            w.click("cancel-run", cx);
        })
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(30));
            let active = cx
                .update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.poll_runtime(cx);
                    });
                    w.render_frame(cx);
                    !app.read(cx).active_runs.is_empty() || !app.read(cx).queued_runs.is_empty()
                })
                .unwrap();
            if !active {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
        }
        cx.update_window(handle, |_, _, cx| {
            assert_eq!(app.read(cx).store.runs.last().unwrap().status, "Cancelled");
            let saved = crate::persistence::Storage::new(root.join("workspace.json"))
                .load()
                .unwrap()
                .unwrap();
            assert_eq!(saved.runs.last().unwrap().status, "Cancelled");
        })
        .unwrap();
    }
    println!(
        "QA passed: native rendering at two sizes; navigation, drag/drop, context menu, global search, policy save, Unicode input, create/edit/undo, cycle inheritance, no simulation action, project creation, subtask ownership, date validation, comments, archive/restore, persisted native Run/Cancel flow."
    );
}

fn click_scrolled(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    for _ in 0..6 {
        let bounds = w.find(id.to_string()).bounds();
        if bounds.top() >= px(105.) && bounds.bottom() <= w.viewport_size().height - px(35.) {
            break;
        }
        let delta = if bounds.top() < px(105.) { 450. } else { -450. };
        w.scroll(
            scroll.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
    }
    w.click(id.to_string(), cx);
}

fn live_context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
    });
    cx
}

/// Opt-in network test: uses the user's local OpenCode credentials and two configured models.
pub fn run_live() {
    use crate::persistence::Storage;
    let root = std::env::current_dir().unwrap().join(".local/live-qa");
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let data = root.join(format!("workspace-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        root.join("last-data-path"),
        data.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let mut cx = live_context();
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(1000.)), |w, cx| {
            let view = cx.new(|cx| {
                let mut view = TaskApp::new_mock(w, cx);
                view.attach_storage(Storage::new(data.join("workspace.json")), w, cx);
                view
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("project-actions", cx);
        w.click(1usize, cx);
        w.input(work.to_str().unwrap(), cx);
        w.click("save-directory", cx);
        w.click("model-settings", cx);
        click_scrolled(w, cx, "model-settings-scroll", "global-task-overrides");
        w.click("refresh-models", cx);
    })
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(40);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let ready = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                !app.read(cx).available_models.is_empty()
            })
            .unwrap();
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "OpenCode model discovery timed out"
        );
    }
    for (stage, model) in [
        (Stage::InProgress, "deepseek/deepseek-flash"),
        (Stage::Review, "deepseek/deepseek-v4-pro"),
    ] {
        cx.update_window(handle, |_, w, cx| {
            click_scrolled(w, cx, "workflow-scroll", &format!("policy-{}", stage.key()));
            click_scrolled(w, cx, "workflow-scroll", "agent-OpenCode");
            click_scrolled(w, cx, "workflow-scroll", "policy-model");
            w.press("cmd-a", cx);
            w.press("backspace", cx);
            click_scrolled(w, cx, "workflow-scroll", "choose-model");
            // The native list uses the exact provider/model identifier from the discovery response.
            w.click(format!("model-{model}"), cx);
            click_scrolled(w, cx, "workflow-scroll", "save-policy");
            assert_eq!(
                app.read(cx)
                    .store
                    .policies
                    .iter()
                    .find(|p| p.stage == stage)
                    .unwrap()
                    .model,
                model
            );
        })
        .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("01-connected-workflow.png"))
        .unwrap();
    let task=cx.update_window(handle,|_,w,cx|{
        w.click("project-1",cx);w.click("new-task",cx);w.input("OpenCode integration acceptance",cx);w.click("draft-stage-in-progress",cx);
        app.update(cx,|a,cx|{let d=a.draft.as_ref().unwrap();d.description.update(cx,|s,cx|s.set_value("Create or overwrite only acceptance.txt in the current folder with exactly TASK_MANAGER_EXECUTION_OK followed by a newline. Do not modify any other file. If the stage is In review, read acceptance.txt and report REVIEW_OK if the exact content matches, without modifying it.",w,cx));});
        w.click("save-task",cx);let id=app.read(cx).selected.unwrap();w.click("execute-task",cx);assert!((!app.read(cx).active_runs.is_empty() || !app.read(cx).queued_runs.is_empty()),"{}",app.read(cx).error);id
    }).unwrap();
    wait_live(&mut cx, handle, &app, &root, "02-implementation-live.png");
    assert_eq!(
        std::fs::read_to_string(work.join("acceptance.txt")).unwrap(),
        "TASK_MANAGER_EXECUTION_OK\n"
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("open-run-task", cx);
        w.click("property-Stage", cx);
        w.click("set-review", cx);
        w.click("execute-task", cx);
        assert_eq!(
            app.read(cx).store.runs.last().unwrap().model,
            "deepseek/deepseek-v4-pro"
        );
    })
    .unwrap();
    wait_live(&mut cx, handle, &app, &root, "03-review-live.png");
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("04-completed-runs.png"))
        .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let state = app.read(cx);
        assert_eq!(state.store.runs.len(), 2);
        assert!(state.store.runs.iter().all(|r| r.status == "Succeeded"));
        assert_eq!(state.store.task(task).unwrap().stage, Stage::Review);
        assert!(
            state
                .store
                .runs
                .last()
                .unwrap()
                .logs
                .iter()
                .any(|line| line.contains("REVIEW_OK"))
        );
        let saved = Storage::new(data.join("workspace.json"))
            .load()
            .unwrap()
            .unwrap();
        assert_eq!(saved, state.store);
    })
    .unwrap();
    println!(
        "LIVE QA passed: model discovery, two stage/model routes, native Run buttons, streamed logs, actual file creation/review, and persisted task/config/history. Data: {}",
        data.display()
    );
}
fn wait_live(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    root: &std::path::Path,
    screenshot: &str,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    let mut captured = false;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(80));
        let (active, has_output) = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                let state = app.read(cx);
                (
                    !state.active_runs.is_empty(),
                    state.store.runs.last().unwrap().logs.len() > 1,
                )
            })
            .unwrap();
        if active && has_output && !captured {
            cx.capture_screenshot(handle)
                .unwrap()
                .save(root.join(screenshot))
                .unwrap();
            captured = true;
            println!("Observed real output while the run was still active.");
        }
        if !active {
            break;
        }
        if std::time::Instant::now() >= deadline {
            cx.update_window(handle, |_, _, cx| app.update(cx, |a, cx| a.cancel_run(cx)))
                .unwrap();
            panic!("OpenCode acceptance run timed out; cancellation requested");
        }
    }
    cx.update_window(handle, |_, _, cx| {
        let run = app.read(cx).store.runs.last().unwrap();
        assert_eq!(run.status, "Succeeded", "Run logs: {}", run.logs.join("\n"));
    })
    .unwrap();
    assert!(captured, "No output was observed before exit");
}
pub fn reopen_live() {
    use crate::persistence::Storage;
    let root = std::env::current_dir().unwrap().join(".local/live-qa");
    let data =
        std::path::PathBuf::from(std::fs::read_to_string(root.join("last-data-path")).unwrap());
    let mut cx = live_context();
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| {
                let mut view = TaskApp::new_mock(w, cx);
                view.attach_storage(Storage::new(data.join("workspace.json")), w, cx);
                view
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    cx.update_window(window.into(), |_, w, cx| {
        w.render_frame(cx);
        let state = app.read(cx);
        assert!(state.storage_error.is_empty(), "{}", state.storage_error);
        assert_eq!(state.store.runs.len(), 2);
        assert!(state.store.runs.iter().all(|r| r.status == "Succeeded"));
        assert!(
            state
                .store
                .tasks
                .iter()
                .any(|t| t.title == "OpenCode integration acceptance")
        );
        assert_eq!(
            state
                .store
                .policies
                .iter()
                .find(|p| p.stage == Stage::Review)
                .unwrap()
                .model,
            "deepseek/deepseek-v4-pro"
        );
        w.click("runs", cx);
    })
    .unwrap();
    cx.capture_screenshot(window.into())
        .unwrap()
        .save(root.join("05-reopened.png"))
        .unwrap();
    println!(
        "REOPEN QA passed in a fresh process: tasks, folders, stage models and completed live logs restored."
    );
}

/// Explicit real-browser acceptance. Uses the normal app controller and default persistent workspace.
pub fn run_research_live() {
    use std::time::{Duration, Instant};
    let work = std::env::current_dir()
        .unwrap()
        .join(".local/storage-research-2026");
    std::fs::create_dir_all(&work).unwrap();
    let brief=std::fs::read_to_string(work.join("research-goal.md")).expect("Place the approved research brief in .local/storage-research-2026/research-goal.md before this opt-in live run");
    let mut cx = live_context();
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new(w, cx));
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    let goal=cx.update_window(handle,|_,_,cx|app.update(cx,|a,cx|{
        assert!(a.storage_error.is_empty(),"{}",a.storage_error);
        if let Some(t)=a.store.tasks.iter().find(|t|t.title=="2026 年剩余时间：存储行业走势与投资机会"&&t.research.is_some()&&!t.archived){return t.id;}
        let project=a.store.projects.iter().map(|p|p.id).max().unwrap_or(0)+1;
        a.store.projects.push(crate::domain::Project{research_brief:String::new(),id:project,name:"存储行业研究".into(),key:"MEM".into(),directory:work.to_string_lossy().into()});
        for (i,model) in ["deepseek/deepseek-flash","deepseek/deepseek-flash","deepseek/deepseek-v4-pro","deepseek/deepseek-v4-pro"].into_iter().enumerate(){if a.store.default_model.is_empty() && a.store.research_config.models[i].is_empty(){a.store.research_config.models[i]=model.into();}}
        a.store.research_config.ego_path="/Users/krabswang/.local/bin/ego-browser".into();
        let goal=a.store.create_research_goal("2026 年剩余时间：存储行业走势与投资机会".into(),format!("{brief}\n\n首次端到端研究请控制为 3 个子任务：一项搜集（覆盖多个来源）、一项分析、一项投资简报。搜集至少 5 个可访问原始来源；优先公开资料，不需要登录，无法获取的估值数据明确列为缺口。"),project).unwrap();
        a.persist_if_changed(true);cx.notify();goal
    })).unwrap();
    println!("Research goal {goal}; real OpenCode and ego(lite); workspace persisted");
    for planning in [true, false] {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                if planning {
                    if !a.store.has_research_plan(goal) {
                        a.start_run(goal, cx);
                    }
                } else {
                    a.run_research_next(goal, true, cx);
                }
            })
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1800);
        let mut previous = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let active = cx
                .update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.poll_runtime(cx);
                    });
                    let a = app.read(cx);
                    let summary = a
                        .store
                        .runs
                        .last()
                        .map(|r| {
                            format!(
                                "Run {} / task {} / {} / {} events",
                                r.id,
                                r.task_id,
                                r.status,
                                r.logs.len()
                            )
                        })
                        .unwrap_or_default();
                    if summary != previous {
                        println!("{summary}");
                        previous = summary;
                    }
                    if a.active_runs.is_empty() && a.queued_runs.is_empty() {
                        println!("State: {} | {}", a.toast, a.error);
                    }
                    let active = !a.active_runs.is_empty();
                    w.render_frame(cx);
                    active
                })
                .unwrap();
            if !active {
                break;
            }
            if Instant::now() > deadline {
                cx.update_window(handle, |_, _, cx| app.update(cx, |a, cx| a.cancel_run(cx)))
                    .unwrap();
                panic!("Research timed out; cancellation requested");
            }
        }
        let okay = cx
            .update_window(handle, |_, _, cx| {
                let a = app.read(cx);
                (if planning {
                    a.store.has_research_plan(goal)
                } else {
                    a.store.next_research_task(goal) == Ok(None)
                }) && a.error.is_empty()
            })
            .unwrap();
        if !okay {
            println!("Research stopped; inspect Agent runs before retrying");
            break;
        }
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.scope = crate::app::Scope::Runs;
            a.selected = None;
            a.selected_run = a.store.runs.last().map(|r| r.id);
            a.show_run_result = true;
            cx.notify();
        });
        let a = app.read(cx);
        for id in a.store.research_children(goal) {
            if let Some(run) = a.store.research_result(id) {
                std::fs::write(work.join(format!("task-{id}-result.md")), &run.output).unwrap();
            }
        }
        println!("Remaining: {:?}", a.store.next_research_task(goal));
        w.render_frame(cx);
    })
    .unwrap();
    // Markdown parsing completes asynchronously after its first rendered frame.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(work.join("latest-research.png"))
        .unwrap();
}
