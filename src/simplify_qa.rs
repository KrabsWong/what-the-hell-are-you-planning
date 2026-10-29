//! Native acceptance of the result-first UI. Fixtures and child processes stay local.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
    persistence::Storage,
    research::{ResearchKind, Source},
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const PLAN: &str = r#"{"tasks":[{"title":"收集需求端证据：超大规模云厂商、二线云、企业自建与主权 AI 的资本开支与数据中心规划","instructions":"仅用于界面验收，记录原始证据。","kind":"collect","depends_on":[]},{"title":"分析供需变化与风险","instructions":"核对前序材料。","kind":"analyze","depends_on":[0]},{"title":"生成最终研究报告","instructions":"说明事实与假设的边界。","kind":"advise","depends_on":[1]}]}"#;
const REPORT: &str = "## 半导体行业研究\n\n这是一份隔离界面验收报告，不是实际市场分析。\n\n### 核心结论\n\n结果正文应是阅读的主体。原始日志、会话编号、模型标识和运行目录通过技术详情查看。\n\n### 需要继续核对\n\n- 比较不同公司的数据口径。\n- 保留原始来源与补充材料。\n- 由用户决定是否进入后续研究。";
const CLI: &str = r#"#!/usr/bin/python3
import json,time
print(json.dumps({'type':'text','part':{'text':'{"status":"complete","report":"RAW_PROTOCOL_SENTINEL"}'}}),flush=True)
print(json.dumps({'type':'reasoning','part':{'text':'UI_QA_PROCESS_READY'}}),flush=True)
time.sleep(30)
"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/simplify-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let cli = directory.join("fake-opencode");
            executable(&cli, CLI);
            let mut store = Store::empty();
            store.appearance = mode;
            store.projects[0].name = "半导体行业研究".into();
            store.projects[0].directory = directory.to_string_lossy().into_owned();
            store.default_model = "qa/isolated-model".into();
            store.opencode_path = cli.to_string_lossy().into_owned();
            let goal = store
                .create_research_goal(
                    "AI 算力芯片与数据中心半导体需求：未来 12 个月的需求结构、供给瓶颈与价值链变化"
                        .into(),
                    "研究供需、产业链、风险及材料证据。此内容仅用于界面验收。".into(),
                    1,
                )
                .unwrap();
            let steps = store.apply_research_plan(goal, PLAN).unwrap();
            for task in &steps {
                let mut run = store.prepare_run(*task).unwrap();
                run.status = "Succeeded".into();
                run.output = REPORT.into();
                run.session_id = Some("ses_UI_QA_TECHNICAL_METADATA".into());
                run.logs = vec!["[fixture] RAW_LOG_SENTINEL, local UI acceptance only".into()];
                run.sources = vec![Source {
                    title: "隔离验收来源，非真实市场证据".into(),
                    url: "https://example.com/ui-qa".into(),
                    published_at: "2026-10-29 QA".into(),
                }];
                run.ended_at = Some(crate::opencode::timestamp());
                run.exit_code = Some(0);
                store.runs.push(run);
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let report_id = store.runs.last().unwrap().id;
            let ordinary = store
                .create_task(
                    "真实子进程界面验收：仅启动本地等待脚本，不调用模型或浏览器",
                    1,
                    Stage::Planned,
                    None,
                )
                .unwrap();
            let path = directory.join("workspace.json");
            Storage::new(path.clone()).save(&store).unwrap();
            let mut cx = HeadlessAppContext::with_platform(
                platform::current_platform(true).text_system(),
                Arc::new(crate::ui_assets::AppAssets),
                platform::current_headless_renderer,
            );
            cx.update(init);
            let mut entity = None;
            let handle: AnyWindowHandle = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                        a.selected_run = Some(report_id);
                        a.navigate(Scope::Runs, w, cx);
                    });
                    entity = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap()
                .into();
            let app = entity.unwrap();
            navigation_checks(&mut cx, handle, &app, goal, steps[0], report_id, &directory);
            screenshot(&mut cx, handle, &directory, "01-result.png");
            cx.update_window(handle, |_, w, cx| {
                let sidebar = app.read(cx).sidebar_split.clone();
                let results = app.read(cx).results_split.clone();
                sidebar.update(cx, |s, cx| s.resize_panel(0, px(280.), w, cx));
                results.update(cx, |s, cx| s.resize_panel(0, px(270.), w, cx));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "01-resized-columns.png");
            cx.update_window(handle, |_, w, cx| {
                let sidebar = app.read(cx).sidebar_split.clone();
                let results = app.read(cx).results_split.clone();
                assert!((sidebar.read(cx).sizes()[0] - px(280.)).abs() < px(2.));
                assert!((results.read(cx).sizes()[0] - px(270.)).abs() < px(2.));
                sidebar.update(cx, |s, cx| s.resize_panel(0, px(236.), w, cx));
                results.update(cx, |s, cx| s.resize_panel(0, px(320.), w, cx));
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).show_run_result);
                assert!(!app.read(cx).show_run_details);
                assert!(w.try_find("run-technical-panel").is_none());
                assert!(w.try_find("run-log-panel").is_none());
                assert!(w.try_find("workflow").is_none());
                assert!(w.find("research-result-scroll").visible());
                assert!(
                    w.try_find("result-source-0").is_none(),
                    "Source links are collapsed by default"
                );
                click_in(w, cx, "research-result-scroll", "toggle-result-sources");
                assert!(w.try_find("result-source-0").is_some());
                click_in(w, cx, "research-result-scroll", "toggle-result-sources");
                assert!(w.try_find("result-source-0").is_none());
                assert_eq!(w.find_all("cancel-run").len(), 0);
                let before = app.read(cx).store.clone();
                let toggle_y = w.find("run-technical-details").bounds().origin.y;
                w.click("run-technical-details", cx);
                assert_eq!(w.find_all("show-result").len(), 1);
                assert_eq!(w.find("show-result").bounds().origin.y, toggle_y);
                assert!(app.read(cx).show_run_details);
                assert!(w.find("run-technical-panel").visible());
                assert!(w.find("run-log-panel").visible());
                w.click("show-raw-output", cx);
                assert!(w.find("run-original-output").visible());
                assert!(w.try_find("run-log-panel").is_none());
                w.click("show-logs", cx);
                assert!(w.find("run-log-panel").visible());
                assert_eq!(app.read(cx).store, before);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-technical.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("show-result", cx);
                assert!(!app.read(cx).show_run_details);
                assert!(w.try_find("run-technical-panel").is_none());
                w.click("run-technical-details", cx);
                click_in(w, cx, "run-history", "run-1");
                assert!(
                    !app.read(cx).show_run_details,
                    "Selecting another execution must reset technical view"
                );
                assert!(app.read(cx).show_run_result);
                assert!(w.try_find("run-log-panel").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-long-title.png");

            // Status-specific notices remain in the ordinary view. No raw errors required.
            for (status, output, name) in [
                (
                    "Failed",
                    "{\"invalid\": \"RAW_PROTOCOL_SENTINEL\"}",
                    "04-failed.png",
                ),
                (
                    "Blocked",
                    "请在浏览器中完成网站权限选择，再返回任务继续。",
                    "05-blocked.png",
                ),
                ("Stale", REPORT, "06-stale.png"),
            ] {
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        let r = &mut a.store.runs[0];
                        r.status = status.into();
                        r.output = output.into();
                        a.show_run_details = false;
                        a.show_run_result = true;
                        cx.notify();
                    });
                    w.render_frame(cx);
                    assert!(w.find("run-progress").visible());
                    assert!(w.try_find("run-technical-panel").is_none());
                    assert!(w.try_find("run-log-panel").is_none());
                    assert!(w.find("open-run-task").visible());
                })
                .unwrap();
                screenshot(&mut cx, handle, &directory, name);
            }

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.store.runs[0].status = "Succeeded".into();
                    a.store.runs[0].output = REPORT.into();
                    a.start_run(ordinary, cx);
                    assert!(!a.active_runs.is_empty(), "{}", a.error);
                });
                w.render_frame(cx);
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.store
                    .runs
                    .last()
                    .unwrap()
                    .logs
                    .iter()
                    .any(|l| l.contains("UI_QA_PROCESS_READY"))
            });
            cx.update_window(handle, |_, w, cx| {
                assert!(!app.read(cx).show_run_details);
                assert!(w.find("run-progress").visible());
                assert_eq!(w.find_all("cancel-run").len(), 1);
                assert!(w.try_find("result-cancel-run").is_none());
                assert!(w.try_find("run-log-panel").is_none());
                assert!(
                    w.try_find("research-result-scroll").is_none(),
                    "Raw in-flight protocol must not be shown as a report"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "07-running.png");
            let active_id = cx
                .update_window(handle, |_, _, cx| {
                    app.read(cx).active_runs.first().unwrap().id
                })
                .unwrap();
            choose_menu(&mut cx, handle, "run-topic-picker", 0); // Research goal
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "run-history", "run-1");
                assert!(
                    w.try_find("cancel-run").is_none(),
                    "A history record must not stop another run"
                );
                assert_eq!(app.read(cx).active_runs.first().unwrap().id, active_id);
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-topic-picker", 1); // Standalone task
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).selected_run, Some(active_id));
                w.click("run-technical-details", cx);
                assert_eq!(w.find_all("cancel-run").len(), 1);
                w.click("cancel-run", cx);
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.active_runs.is_empty() && a.queued_runs.is_empty()
            });
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(app.read(cx).store.runs.last().unwrap().status, "Cancelled")
            })
            .unwrap();

            // A completed goal must open its final child report, not its old planning JSON.
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let mut planning = a.store.runs[0].clone();
                    planning.id = a.store.runs.iter().map(|r| r.id).max().unwrap() + 1;
                    planning.task_id = goal;
                    planning.task_title = a.store.task(goal).unwrap().title.clone();
                    planning.research_kind = Some(ResearchKind::Goal);
                    planning.output = PLAN.into();
                    planning.sources.clear();
                    a.store.runs.push(planning);
                    a.open_task(goal, w, cx);
                });
                w.render_frame(cx);
                assert!(
                    w.try_find("task-view-result").is_none(),
                    "A goal must not offer its planning JSON as the result"
                );
                assert_eq!(
                    w.find("research-task-result").label(),
                    Some("View final report")
                );
                let before = app.read(cx).store.clone();
                w.click("research-task-result", cx);
                assert_eq!(app.read(cx).selected_run, Some(report_id));
                assert_eq!(app.read(cx).scope, Scope::Runs);
                assert!(w.find("research-result-scroll").visible());
                assert_eq!(app.read(cx).store, before);
                assert_eq!(app.read(cx).task_result_context, Some(goal));
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(w.try_find("run-navigation").is_none());
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).selected, Some(goal));
                assert!(app.read(cx).task_result_context.is_none());
                app.update(cx, |a, cx| a.open_task(steps[2], w, cx));
                w.render_frame(cx);
                assert_eq!(
                    w.find("task-view-result").label(),
                    Some("View final report")
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-upstream-research.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.open_task(steps[0], w, cx));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-task.png");
            let task_scroll = cx
                .update_window(handle, |_, _, cx| app.read(cx).detail_scroll.offset())
                .unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.click("task-view-result", cx);
                assert_eq!(app.read(cx).task_result_context, Some(steps[0]));
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(w.try_find("run-navigation").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-inline-task-result.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("open-run-task", cx);
                assert_eq!(app.read(cx).selected, Some(steps[0]));
                assert_eq!(app.read(cx).detail_scroll.offset(), task_scroll);
                app.update(cx, |a, cx| a.navigate(Scope::Runs, w, cx));
                w.render_frame(cx);
                assert!(app.read(cx).task_result_context.is_none());
                assert!(w.try_find("run-navigation").is_some());
                app.update(cx, |a, cx| a.open_task(steps[0], w, cx));
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.navigate(Scope::All, w, cx);
                    a.expanded_sidebar_projects.clear();
                    a.open_task(steps[0], w, cx);
                });
                w.render_frame(cx);
                assert_eq!(app.read(cx).scope, Scope::All);
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(app.read(cx).expanded_sidebar_projects.contains(&1));
                assert!(w.try_find(format!("sidebar-task-{}", steps[0])).is_some());
                let scope = app.read(cx).scope.clone();
                let selected = app.read(cx).selected;
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert!(w.try_find(format!("sidebar-task-{goal}")).is_none());
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert_eq!(app.read(cx).scope, scope);
                assert_eq!(app.read(cx).selected, selected);
                assert!(w.try_find(format!("sidebar-task-{goal}")).is_some());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-project-expanded.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("sidebar-goal-toggle-{goal}"), cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-goal-collapsed.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find(format!("sidebar-task-{}", steps[0])).is_none());
                app.update(cx, |a, cx| a.open_task(steps[0], w, cx));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-goal-revealed.png");
            cx.update_window(handle, |_, w, _| {
                assert!(w.find(format!("sidebar-task-{}", steps[0])).visible());
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.hover(format!("sidebar-task-{}", steps[1]), cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-project-task-hover.png");

            cx.update_window(handle, |_, w, cx| {
                click_in(
                    w,
                    cx,
                    "sidebar-scroll",
                    &format!("sidebar-task-{}", steps[0]),
                );
                assert_eq!(app.read(cx).selected, Some(steps[0]));
                assert_eq!(app.read(cx).scope, Scope::Project(1));
                click_in(w, cx, "sidebar-scroll", "project-1");
                assert_eq!(app.read(cx).selected, None);
                assert!(app.read(cx).expanded_sidebar_projects.contains(&1));
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert!(w.try_find(format!("sidebar-task-{goal}")).is_none());
                app.update(cx, |a, cx| a.open_task(steps[0], w, cx));
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.open_task(goal, w, cx));
                w.render_frame(cx);
                assert!(w.try_find("research-all").is_none());
                assert!(w.try_find("research-next").is_none());
                scroll_to(w, cx, "detail-scroll", &format!("subtask-{}", steps[0]));
                assert!(w.find(format!("subtask-{}", steps[0])).bounds().size.height <= px(36.));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-compact-subtasks.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("subtask-{}", steps[0]), cx);
                assert_eq!(app.read(cx).selected, Some(steps[0]));
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("edit-task").is_none());
                assert!(w.try_find("archive-task").is_none());
                w.click("task-actions-menu", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-task-menu.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(1usize, cx);
                assert_eq!(app.read(cx).toast, "Task link copied");
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                assert!(!app.read(cx).show_task_activity);
                assert!(w.try_find("comment-input").is_none());
                click_in(w, cx, "detail-scroll", "task-activity-toggle");
                assert!(app.read(cx).show_task_activity);
                assert!(w.try_find("comment-input").is_some());
                click_in(w, cx, "detail-scroll", "task-activity-toggle");
                assert!(w.try_find("comment-input").is_none());
                assert!(!app.read(cx).show_task_details);
                assert_eq!(
                    w.find("task-view-result").label(),
                    Some("View latest result")
                );
                assert_eq!(w.find("execute-task").label(), Some("Run again"));
                assert!(w.try_find("configure-research").is_none());
                assert!(w.try_find("property-Cycle").is_none());
                assert!(w.try_find("property-Stage").is_some());
                let before = app.read(cx).store.clone();
                click_in(w, cx, "detail-scroll", "task-details-toggle");
                assert!(app.read(cx).show_task_details);
                assert!(w.try_find("configure-research").is_some());
                assert!(w.try_find("property-Cycle").is_some());
                assert_eq!(app.read(cx).store, before);
                scroll_to(w, cx, "detail-scroll", "configure-research");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "09-task-details.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("research", cx);
                assert!(!app.read(cx).show_research_settings);
                assert!(w.try_find("save-research-settings").is_none());
                assert!(w.try_find("create-research").is_some());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "10-research.png");
            cx.update_window(handle, |_, w, cx| {
                let before = app.read(cx).store.clone();
                w.click("research-settings-toggle", cx);
                assert!(app.read(cx).show_research_settings);
                assert!(w.try_find("research-settings-toggle").is_none());
                assert!(
                    w.try_find("create-research").is_none(),
                    "Advanced settings replace the composer in this view"
                );
                assert!(w.try_find("save-research-settings").is_some());
                assert_eq!(app.read(cx).store, before);
                scroll_to(w, cx, "research-scroll", "research-model-0");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "11-research-settings.png");
            cx.update_window(handle, |_, w, cx| {
                scroll_to(w, cx, "research-scroll", "save-research-settings")
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "11b-browser-settings.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope, Scope::Settings);
                click_in(w, cx, "model-settings-scroll", "global-task-overrides");
                assert_eq!(app.read(cx).scope, Scope::Workflow);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "12-workflow-access.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.store.research_config.models[0] = "qa/planning-override".into();
                    a.open_project_plan(1, w, cx);
                });
                click_in(w, cx, "project-plan-scroll", "project-plan-model");
                assert_eq!(app.read(cx).scope, Scope::Research);
                assert!(
                    app.read(cx).show_research_settings,
                    "Project planning override link must reveal its settings"
                );
                assert!(app.read(cx).planning_project.is_none());
                assert!(w.try_find("research-model-0").is_some());
                assert!(w.try_find("create-research").is_none());
                w.click("research", cx);
                assert!(
                    !app.read(cx).show_research_settings,
                    "Normal navigation returns to the research composer"
                );
                assert!(w.try_find("create-research").is_some());
                assert!(w.try_find("research-model-0").is_none());
                scroll_to(w, cx, "research-scroll", &format!("research-goal-{goal}"));
                assert!(w.find(format!("research-goal-{goal}")).bounds().size.height <= px(36.));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "12-compact-research-goals.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("research-goal-{goal}"), cx);
                assert_eq!(app.read(cx).selected, Some(goal));
                w.click("project-1", cx);
                assert_eq!(app.read(cx).scope, Scope::Project(1));
                assert!(w.try_find("run-project-todo").is_some());
                app.update(cx, |a, _| {
                    a.persist_if_changed(true);
                    assert!(a.storage_error.is_empty());
                    assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap(), a.store);
                });
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "13-project.png");
            // Exercise the real research launch route using only a local guide and waiting CLI.
            let ego = directory.join("fake-ego-browser");
            executable(
                &ego,
                "#!/bin/sh\nprintf '%s\\n' 'QA browser guide only' '[ego-browser:skill] end of guide'\n",
            );
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    a.store.research_config.ego_path = ego.to_string_lossy().into_owned();
                    a.start_run(steps[0], cx);
                    assert!(!a.active_runs.is_empty(), "{}", a.error);
                    assert_eq!(a.store.task(steps[0]).unwrap().stage, Stage::InProgress);
                });
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.store
                    .runs
                    .last()
                    .unwrap()
                    .logs
                    .iter()
                    .any(|l| l.contains("UI_QA_PROCESS_READY"))
            });
            cx.update_window(handle, |_, w, _| {
                assert!(w.find("run-waiting").visible());
                assert!(w.find("run-open-browser").visible());
                assert!(w.try_find("research-result-scroll").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "14-research-running.png");
            cx.update_window(handle, |_, w, cx| w.click("cancel-run", cx))
                .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.active_runs.is_empty() && a.queued_runs.is_empty()
            });
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    let run = a.store.runs.last_mut().unwrap();
                    run.status = "Blocked".into();
                    run.logs.push(
                        "[blocked] Local UI fixture: website needs the user's attention".into(),
                    );
                    cx.notify();
                });
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "15-browser-attention.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.find("run-open-browser").visible());
                assert!(w.try_find("research-result-scroll").is_none());
                assert!(w.try_find("cancel-run").is_none());
                app.update(cx, |a, _| {
                    a.persist_if_changed(true);
                    assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap(), a.store);
                });
            })
            .unwrap();
            // Project identity, attempt history and compact toolbar are exercised together.
            let (latest_id, other_run_id) = cx
                .update_window(handle, |_, w, cx| {
                    let ids = app.update(cx, |a, cx| {
                        let mut second = a.store.projects[0].clone();
                        second.id = 2;
                        second.name = "存储行业研究".into();
                        second.key = "MEM".into();
                        a.store.projects.push(second);
                        let duplicate = a
                            .store
                            .create_task("生成最终研究报告", 2, Stage::Planned, None)
                            .unwrap();
                        let mut other = a.store.prepare_run(duplicate).unwrap();
                        other.status = "Succeeded".into();
                        other.output = REPORT.into();
                        let other_id = other.id;
                        a.store.runs.push(other);
                        let another_goal = a
                            .store
                            .create_research_goal(
                                "另一个研究目标：等待检查范围与证据".into(),
                                "仅用于层级验收".into(),
                                1,
                            )
                            .unwrap();
                        let mut untouched = a.store.prepare_run(another_goal).unwrap();
                        untouched.status = "Failed".into();
                        a.store.runs.push(untouched);
                        let mut latest = a
                            .store
                            .runs
                            .iter()
                            .find(|r| r.id == report_id)
                            .unwrap()
                            .clone();
                        latest.id = a.store.runs.iter().map(|r| r.id).max().unwrap() + 1;
                        latest.status = "Blocked".into();
                        latest.output = "请处理浏览器权限后继续。".into();
                        let latest_id = latest.id;
                        a.store.runs.push(latest);
                        a.selected_run = Some(latest_id);
                        a.navigate(Scope::Runs, w, cx);
                        (latest_id, other_id)
                    });
                    w.render_frame(cx);
                    assert!(w.find("run-project-picker").visible());
                    assert!(w.find("run-topic-picker").visible());
                    assert!(w.find(format!("run-title-{}", ids.0)).bounds().size.height >= px(16.));
                    assert!(
                        w.try_find(format!("run-{report_id}")).is_none(),
                        "Old attempts do not clutter the task list"
                    );
                    ids
                })
                .unwrap();
            let before = cx
                .update_window(handle, |_, _, cx| app.read(cx).store.clone())
                .unwrap();
            choose_menu(&mut cx, handle, "run-version-picker", 1);
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).selected_run, Some(report_id));
                assert!(w.find("research-result-scroll").visible());
                assert!(w.try_find(format!("run-{latest_id}")).is_some());
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-version-picker", 0);
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(app.read(cx).selected_run, Some(latest_id));
                assert_eq!(
                    app.read(cx).store,
                    before,
                    "Version browsing must not modify results"
                );
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-topic-picker", 2);
            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).show_planning_activity);
                assert!(w.try_find(format!("run-{latest_id}")).is_none());
                app.update(cx, |a, cx| a.view_task_result(steps[2], cx));
                w.render_frame(cx);
                assert!(w.try_find(format!("run-{latest_id}")).is_some());
                assert!(!app.read(cx).show_planning_activity);
                click_in(w, cx, "run-history", "run-planning-toggle");
                assert!(app.read(cx).show_planning_activity);
                click_in(w, cx, "run-history", "run-planning-toggle");
                assert!(w.find("run-project-picker").visible());
                assert!(w.find("run-topic-picker").visible());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "16-grouped-history.png");
            for (control, file) in [
                ("run-topic-picker", "26-topic-menu.png"),
                ("open-tasks-menu", "27-open-menu.png"),
            ] {
                cx.update_window(handle, |_, w, cx| {
                    w.click(control, cx);
                })
                .unwrap();
                cx.update_window(handle, |_, w, cx| {
                    w.hover(0usize, cx);
                })
                .unwrap();
                for _ in 0..12 {
                    cx.advance_clock(Duration::from_millis(100));
                    cx.run_until_parked();
                    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                        .unwrap();
                }
                screenshot(&mut cx, handle, &directory, file);
                cx.update_window(handle, |_, w, cx| {
                    w.press("escape", cx);
                })
                .unwrap();
            }

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.selected_run = Some(report_id);
                    a.reveal_selected_run();
                    cx.notify();
                });
                w.render_frame(cx);
                assert!(
                    w.try_find("run-version-picker").is_some(),
                    "Historical report can switch to latest"
                );
                w.click("run-project-picker", cx);
                w.click(1usize, cx); // Other project
                assert!(w.try_find(format!("run-{other_run_id}")).is_some());
                assert_eq!(
                    app.read(cx).selected_run,
                    Some(other_run_id),
                    "Same titles in different projects stay independent"
                );
                app.update(cx, |a, cx| {
                    a.layout = crate::app::Layout::List;
                    a.navigate(Scope::All, w, cx);
                });
                w.render_frame(cx);
                assert!(w.find("collection-project-1").visible());
                assert!(w.find("collection-project-2").visible());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "17-grouped-tasks.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("collection-project-2", cx);
                assert_eq!(app.read(cx).scope, Scope::Project(2));
                w.click("project-1", cx);
                let title = w.find("collection-title").bounds();
                let action = w.find("run-project-todo").bounds();
                assert!(title.size.height >= px(24.));
                assert!(
                    (action.top() - title.top()).abs() < px(40.),
                    "Batch action belongs in the title toolbar"
                );
                assert!(
                    w.try_find("todo-batch-status").is_none(),
                    "Idle projects have no batch banner"
                );
                w.click("project-actions", cx);
                assert_eq!(w.find(1usize).label(), Some("Working folder…"));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "18-project-menu.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(1usize, cx);
                assert_eq!(app.read(cx).directory_project, Some(1));
                w.click("cancel-directory", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.click("project-actions", cx);
                w.click(0usize, cx);
                assert_eq!(app.read(cx).planning_project, Some(1));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "18b-project-planning.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("close-project-plan", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.click("list-view", cx);
                assert!(w.find("run-project-todo").visible());
                assert!(app.read(cx).directory_project.is_none());
                assert!(app.read(cx).planning_project.is_none());
                assert!(app.read(cx).layout == crate::app::Layout::List);
                assert!(app.read(cx).store.runs.iter().any(|r| r.id == latest_id));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "19-project-list.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.view_task_result(steps[2], cx));
                w.render_frame(cx);
                assert!(w.find("location-all").visible());
                w.click("open-run-task", cx);
                assert_eq!(app.read(cx).selected, Some(steps[2]));
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).selected, Some(goal));
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope, Scope::Project(1));
                assert!(app.read(cx).selected.is_none());
                app.update(cx, |a, cx| a.view_task_result(steps[2], cx));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "24-focused-progress.png");
            // All includes closed tasks, excludes archives, and preserves the drill-down origin.
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let cancelled = a
                        .store
                        .create_task("Cancelled filter fixture", 1, Stage::Cancelled, None)
                        .unwrap();
                    let archived = a
                        .store
                        .create_task("Archived filter fixture", 1, Stage::Done, None)
                        .unwrap();
                    a.store.task_mut(archived).unwrap().archived = true;
                    a.show_done = false;
                    a.navigate(Scope::All, w, cx);
                    assert!(a.visible_tasks().iter().any(|t| t.id == cancelled));
                    assert!(a.visible_tasks().iter().any(|t| t.stage == Stage::Done));
                    assert!(a.visible_tasks().iter().all(|t| !t.archived));
                });
                w.render_frame(cx);
                assert_eq!(w.find("collection-title").label(), Some("All"));
                w.click("all-status-filter", cx);
                w.click(6usize, cx); // Done
                assert_eq!(
                    app.read(cx).all_status,
                    crate::app::AllStatus::Stage(Stage::Done)
                );
                assert_eq!(app.read(cx).visible_tasks().len(), 3);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.click("task-search", cx);
                w.input("分析", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert_eq!(app.read(cx).query, "分析");
                assert_eq!(app.read(cx).visible_tasks().len(), 1);
                assert!(w.try_find("show-done").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "20-all-status.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("collection-project-1", cx);
                assert_eq!(app.read(cx).scope, Scope::Project(1));
                assert!(app.read(cx).query.is_empty());
                assert!(w.find("navigation-back").visible());
                w.click("board-view", cx);
                w.render_frame(cx);
                let column = format!("column-{}", Stage::Planned.key());
                let before = w.find(column.clone()).bounds().left();
                w.scroll(
                    column.clone(),
                    ScrollDelta::Pixels(point(px(0.), px(-240.))),
                    cx,
                );
                w.render_frame(cx);
                assert_eq!(
                    w.find(column.clone()).bounds().left(),
                    before,
                    "Vertical scroll must not move the board horizontally"
                );
                w.scroll(
                    column.clone(),
                    ScrollDelta::Pixels(point(px(-120.), px(0.))),
                    cx,
                );
                w.render_frame(cx);
                assert!(
                    w.find(column.clone()).bounds().left() < before,
                    "Explicit horizontal scrolling remains available"
                );
                app.update(cx, |a, cx| a.open_task(goal, w, cx));
                w.render_frame(cx);
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope, Scope::Project(1));
                assert!(app.read(cx).selected.is_none());
                assert!(app.read(cx).layout == crate::app::Layout::Board);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "21-project-return.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope, Scope::All);
                assert!(app.read(cx).query.is_empty());
                assert!(w.try_find("navigation-back").is_none());
                assert_eq!(
                    app.read(cx).all_status,
                    crate::app::AllStatus::Stage(Stage::Done)
                );
                assert_eq!(app.read(cx).visible_tasks().len(), 3);
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.scroll(
                    "column-backlog",
                    ScrollDelta::Pixels(point(px(-1200.), px(0.))),
                    cx,
                );
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "21-all-board-tags.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.query.clear();
                    a.search.update(cx, |s, cx| s.set_value("", w, cx));
                });
                w.click("all-status-filter", cx);
                w.click(1usize, cx); // Open
                assert!(
                    app.read(cx)
                        .visible_tasks()
                        .iter()
                        .all(|t| !t.stage.is_closed())
                );
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.click("all-status-filter", cx);
                w.click(7usize, cx); // Cancelled
                assert_eq!(app.read(cx).visible_tasks().len(), 1);
                assert_eq!(app.read(cx).visible_tasks()[0].stage, Stage::Cancelled);
                app.update(cx, |a, cx| a.open_task(ordinary, w, cx));
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "detail-scroll", "property-Priority");
                click_in(w, cx, "detail-scroll", "priority-high");
                assert_eq!(
                    app.read(cx).store.task(ordinary).unwrap().priority,
                    crate::domain::Priority::High
                );
                click_in(w, cx, "detail-scroll", "property-Stage");
                click_in(w, cx, "detail-scroll", "set-review");
                assert_eq!(
                    app.read(cx).store.task(ordinary).unwrap().stage,
                    Stage::Review
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "22-text-properties.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("property-Stage", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "28-stage-options.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("property-Stage", cx);
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.store.projects[0].name =
                        "半导体行业研究：跨市场长期趋势、供给瓶颈与产业链机会".into();
                    cx.notify();
                });
                w.render_frame(cx);
                assert!(w.find("property-Project").bounds().size.width <= px(164.));
                assert!(w.find("property-Project").bounds().right() <= w.viewport_size().width);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "23-long-property.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.navigate(Scope::Project(1), w, cx);
                    a.open_task(ordinary, w, cx);
                });
                w.render_frame(cx);
                for id in ["navigation-back", "open-tasks-menu", "location-trail"] {
                    let bounds = w.find(id).bounds();
                    assert!(
                        bounds.right() <= w.viewport_size().width,
                        "Header action {id} overflowed"
                    );
                    assert!(bounds.size.height <= px(44.));
                }
                w.click("navigation-back", cx);
                let title = w.find("location-current").bounds();
                assert!(title.size.height <= px(42.));
                assert!(title.right() <= w.find("open-tasks-menu").bounds().left());
                app.update(cx, |a, cx| {
                    let mut planning = a
                        .store
                        .runs
                        .iter()
                        .find(|r| r.task_id == goal)
                        .unwrap()
                        .clone();
                    planning.id = a.store.runs.iter().map(|r| r.id).max().unwrap() + 1;
                    planning.status = "Failed".into();
                    a.store.runs.push(planning);
                    a.selected_run = None;
                    a.navigate(Scope::Runs, w, cx);
                });
                w.render_frame(cx);
                assert!(
                    app.read(cx).show_planning_activity,
                    "Latest planning record is revealed after reopening Progress"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "25-planning-activity.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.navigate(Scope::Settings, w, cx);
                    let goal = a.store.create_research_goal("Background planning QA".into(), "Local fixture".into(), 1).unwrap();
                    let mut run = a.store.prepare_run(goal).unwrap();
                    let run_id = run.id;
                    run.status = "Succeeded".into(); run.output = PLAN.into();
                    a.store.runs.push(run);
                    a.research_sequences.clear();
                    a.complete_research_run(run_id, cx);
                    assert_eq!(a.scope, Scope::Settings, "Background planning must not change the user's page");
                    let mut project = a.store.projects[0].clone();
                    project.id = 900; project.name = "Background project planning".into();
                    a.store.projects.push(project);
                    let mut run = a.store.prepare_project_plan(900).unwrap();
                    run.status = "Succeeded".into();
                    run.output = r#"{"tasks":[{"title":"Local proposal","brief":"Only a navigation fixture."}]}"#.into();
                    a.store.runs.push(run);
                    a.finalize_project_plan(a.store.runs.len()-1);
                    assert_eq!(a.scope, Scope::Settings);
                    a.navigate(Scope::All, w, cx);
                    let removed = a.store.create_task("Removed navigation target", 1, Stage::Backlog, None).unwrap();
                    a.open_task(removed, w, cx);
                    a.open_task(ordinary, w, cx);
                    a.store.tasks.retain(|t| t.id != removed);
                    a.go_back(w, cx);
                    assert_eq!(a.scope, Scope::Project(a.store.task(ordinary).unwrap().project_id));
                    assert!(a.selected.is_none(), "Back uses ownership, not previously visited tasks");
                });
                w.press("cmd-k", cx);
            }).unwrap();
            cx.update(|cx| cx.quit());
        }
    }
    println!(
        "Simplified UI acceptance passed; screenshots: {}",
        root.display()
    );
}

fn navigation_checks(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    goal: u64,
    child: u64,
    report: u64,
    directory: &Path,
) {
    cx.update_window(handle, |_, w, cx| {
        let before = app.read(cx).store.clone();
        let report_task = before.runs.iter().find(|r| r.id == report).unwrap().task_id;
        let project = before.task(goal).unwrap().project_id;
        assert!(w.find("location-all").visible());
        assert!(w.find("location-task").visible());
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected, Some(report_task));
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected, Some(goal));
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope, Scope::Project(project));
        assert!(app.read(cx).selected.is_none());
        w.press("cmd-[", cx);
        assert_eq!(app.read(cx).scope, Scope::All);
        assert!(w.try_find("navigation-back").is_none());
        // Opening a goal from an unrelated page must still return to its project.
        app.update(cx, |a, cx| {
            a.navigate(Scope::Settings, w, cx);
            a.open_task(goal, w, cx);
        });
        w.render_frame(cx);
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope, Scope::Project(project));
        app.update(cx, |a, cx| a.open_task(child, w, cx));
        w.render_frame(cx);
        assert!(w.try_find("location-parent").is_some());
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected, Some(goal));
        assert_eq!(app.read(cx).store, before);
        app.update(cx, |a, cx| {
            let nested = a
                .store
                .create_task(
                    "Nested navigation check",
                    project,
                    Stage::Planned,
                    Some(child),
                )
                .unwrap();
            a.open_task(nested, w, cx);
            let (path, _) = a.navigation_path();
            assert_eq!(
                path.len(),
                4,
                "All, project, goal and parent must all be present"
            );
            a.go_back(w, cx);
            assert_eq!(a.selected, Some(child));
            a.store = before.clone();
            a.tabs.retain(|id| *id != nested);
            a.open_task(goal, w, cx);
        });
    })
    .unwrap();
    screenshot(cx, handle, directory, "00-goal-details.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.selected_run = Some(report);
            a.navigate(Scope::Runs, w, cx);
            a.expanded_run_sources = None;
            a.result_scroll.set_offset(point(px(0.), px(0.)));
        });
    })
    .unwrap();
}

fn choose_menu(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, trigger: &str, item: usize) {
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.click(trigger.to_string(), cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.click(item, cx))
        .unwrap();
    cx.run_until_parked();
}

fn scroll_to(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    for _ in 0..30 {
        let bounds = w.find(id.to_string()).bounds();
        let area = w.find(scroll.to_string()).bounds();
        let top = area.top().max(px(110.));
        let bottom = area.bottom().min(w.viewport_size().height - px(40.)) - px(8.);
        if bounds.top() >= top && bounds.bottom() <= bottom {
            break;
        }
        let delta = if bounds.top() < top { 120. } else { -120. };
        w.scroll(
            scroll.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
    }
    assert!(w.find(id.to_string()).visible(), "Could not scroll to {id}");
}
fn click_in(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    scroll_to(w, cx, scroll, id);
    w.click(id.to_string(), cx);
}
fn wait(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    ready: impl Fn(&TaskApp) -> bool,
) {
    let start = Instant::now();
    loop {
        let done = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                ready(app.read(cx))
            })
            .unwrap();
        if done {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "Timed out waiting for local QA process"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
fn executable(path: &Path, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, source).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
