//! Native visual/interaction acceptance for action affordances. No model is invoked.
use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
    persistence::Storage,
    research::Source,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

const TOPICS: &str = r#"{"tasks":[{"title":"未来一年半导体存储供需与行业机会","brief":"比较 DRAM、NAND 与 HBM 需求，注明日期与来源，检查风险和证据缺口。"},{"title":"先进封装与设备产业链研究","brief":"检查产能、资本开支与需求的变化。"}]}"#;
const STEPS: &str = r#"{"tasks":[{"title":"采集原始公告与新闻","instructions":"保存实际来源和日期。","kind":"collect","depends_on":[]},{"title":"核对材料并分析供需","instructions":"引用补充材料与前序来源。","kind":"analyze","depends_on":[0]},{"title":"整理最终研究报告","instructions":"区分事实、推断和风险。","kind":"advise","depends_on":[1]}]}"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/affordance-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let name = if mode == Appearance::Light {
                "light"
            } else {
                "dark"
            };
            let directory = root.join(format!("{name}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.json");
            let mut store = Store::empty();
            store.projects[0].name = "半导体行业研究".into();
            store.research_config.models =
                ["qa/planner", "qa/collector", "qa/analyst", "qa/adviser"].map(String::from);
            store.appearance = mode;
            Storage::new(path.clone()).save(&store).unwrap();
            let mut cx = HeadlessAppContext::with_platform(
                platform::current_platform(true).text_system(),
                Arc::new(assets::Assets),
                platform::current_headless_renderer,
            );
            cx.update(init);
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                        a.available_models = vec![
                            "qa/planner".into(),
                            "qa/collector".into(),
                            "qa/analyst".into(),
                        ];
                        a.open_project_plan(1, w, cx);
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle = window.into();

            screenshot(&mut cx, handle, &directory, "01-planning-disabled.png");
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(w.find("generate-project-plan").disabled(), Some(true));
                let before = app.read(cx).store.clone();
                w.click("generate-project-plan", cx);
                assert_eq!(
                    app.read(cx).store,
                    before,
                    "Disabled generation must have no side effect"
                );
                w.hover("project-plan-folder", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-planning-hover.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("project-plan-brief", cx);
                w.input("全球半导体市场，未来一年；需要原始证据与风险。", cx);
            })
            .unwrap();
            // Dispatching a Root action is deferred; each key gets an event-loop turn.
            let mut reached = false;
            for _ in 0..24 {
                press(&mut cx, handle, "tab");
                reached = cx
                    .update_window(handle, |_, w, _| {
                        w.find("save-project-context").focused() == Some(true)
                    })
                    .unwrap();
                if reached {
                    break;
                }
            }
            assert!(
                reached,
                "Tab could not reach Save scope inside the planning dialog"
            );
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(
                    app.read(cx).project_brief.read(cx).value().as_ref(),
                    "全球半导体市场，未来一年；需要原始证据与风险。",
                    "Tab must leave the plain-text form without inserting indentation"
                );
            })
            .unwrap();
            press(&mut cx, handle, "shift-tab");
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find("project-plan-model").focused(), Some(true));
            })
            .unwrap();
            press(&mut cx, handle, "tab");
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find("save-project-context").focused(), Some(true));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-keyboard-focus.png");
            press(&mut cx, handle, "enter");
            cx.update_window(handle, |_, _, cx| {
                assert!(
                    app.read(cx).store.projects[0]
                        .research_brief
                        .contains("全球半导体市场")
                );
            })
            .unwrap();
            for key in ["tab", "shift-tab"] {
                for _ in 0..8 {
                    press(&mut cx, handle, key);
                    cx.update_window(handle, |_, w, cx| {
                        assert!(
                            app.read(cx).modal_focus.contains_focused(w, cx),
                            "Planning dialog lost its focus trap while navigating with {key}"
                        );
                        assert_ne!(w.find("generate-project-plan").focused(), Some(true));
                    })
                    .unwrap();
                }
            }
            cx.update_window(handle, |_, w, cx| {
                // Close is tested as an actual icon hit target, then reopen from the project.
                w.click("close-project-plan", cx);
                assert!(app.read(cx).planning_project.is_none());
                w.click("plan-project", cx);
                app.update(cx, |a, cx| {
                    a.store.projects[0].directory = directory.to_string_lossy().into();
                    cx.notify();
                });
                w.render_frame(cx);
                assert_ne!(w.find("generate-project-plan").disabled(), Some(true));
                assert!(app.read(cx).store.runs.is_empty());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "04-planning-ready.png");

            let goal = cx
                .update_window(handle, |_, w, cx| {
                    w.click("close-project-plan", cx);
                    app.update(cx, |a, cx| {
                        let ids = a.store.apply_project_plan(1, TOPICS).unwrap();
                        a.layout = Layout::Board;
                        cx.notify();
                        ids[0]
                    })
                })
                .unwrap();
            screenshot(&mut cx, handle, &directory, "05-kanban.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("approve-topic-{goal}"), cx);
                assert_eq!(app.read(cx).store.task(goal).unwrap().stage, Stage::Planned);
                assert!(app.read(cx).store.runs.is_empty());
                app.update(cx, |a, cx| {
                    let children = a.store.apply_research_plan(goal, STEPS).unwrap();
                    let source = directory.join("补充材料：行业公告与数据.pdf");
                    std::fs::write(&source, include_bytes!("test-fixtures/supplement.pdf")).unwrap();
                    let material = crate::attachments::import_file(&directory, &source).unwrap();
                    a.store.task_mut(goal).unwrap().attachments.push(material.clone());
                    let mut run = a.store.prepare_run(children[0]).unwrap();
                    run.status = "Succeeded".into();
                    run.output = "## 研究证据（界面验收）\n\n这是独立 QA 数据，不是实际市场结论。\n\n[查看原始公告](https://example.com/announcement)\n\n来源链接应与正文清楚区分。".into();
                    run.sources = vec![Source { title: "原始公司公告与财报：供需数据来源".into(), url: "https://example.com/announcement".into(), published_at: "2026-10-29 (QA)".into() }];
                    run.attachments = vec![material];
                    run.logs = vec!["[fixture] Native UI evidence only, no external process was launched.".into()];
                    run.ended_at = Some(crate::opencode::timestamp());
                    run.exit_code = Some(0);
                    a.store.runs.push(run);
                    a.store.set_stage(children[0], Stage::Done).unwrap();
                    a.open_task(goal, w, cx);
                });
            }).unwrap();
            screenshot(&mut cx, handle, &directory, "06-task-materials.png");
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "detail-scroll", "property-Stage");
                assert!(w.try_find("set-todo").is_some());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "07-task-property-menu.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("research", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-research.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("research-settings-toggle", cx);
                click_in(w, cx, "research-scroll", "save-research-settings");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "09-research-settings.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("model-settings", cx);
                click_in(w, cx, "model-settings-scroll", "global-task-overrides");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "10-workflow.png");
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(w.find("agent-Codex").disabled(), Some(true));
                let before = app.read(cx).policy_agent.clone();
                click_in(w, cx, "workflow-scroll", "agent-Codex");
                assert_eq!(app.read(cx).policy_agent, before);
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "workflow-scroll", "save-policy");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "11-workflow-settings.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("runs", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "12-result-links.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("result-source-0").is_none());
                click_in(w, cx, "research-result-scroll", "toggle-result-sources");
                assert!(w.try_find("result-source-0").is_some());
                let before = app.read(cx).store.clone();
                w.click("run-technical-details", cx);
                assert!(w.try_find("run-log-panel").is_some());
                assert!(
                    w.try_find("cancel-run").is_none(),
                    "Completed history has no stop action"
                );
                assert_eq!(app.read(cx).store, before);
                assert!(
                    (app.read(cx).active_runs.is_empty() && app.read(cx).queued_runs.is_empty())
                );
                app.update(cx, |a, cx| {
                    a.navigate(Scope::Project(1), w, cx);
                    a.layout = Layout::List;
                    cx.notify();
                });
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "13-task-list.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let project = a.store.projects[0].clone();
                    a.store.projects.extend((2..=9).map(|id| {
                        let mut extra = project.clone();
                        extra.id = id;
                        extra.name = format!("QA 行业研究 {id}");
                        extra.key = format!("QA{id}");
                        extra
                    }));
                    cx.notify();
                });
                w.render_frame(cx);
                let bounds = w.find("toggle-appearance").bounds();
                assert!(bounds.top() >= px(0.));
                assert!(
                    bounds.bottom() <= w.viewport_size().height - px(41.),
                    "Theme action must stay fully above the footer and sidebar padding"
                );
                click_in(w, cx, "sidebar-scroll", "project-9");
                assert_eq!(app.read(cx).scope, Scope::Project(9));
                let next = if app.read(cx).store.appearance == Appearance::Light {
                    2usize
                } else {
                    1usize
                };
                w.click("toggle-appearance", cx);
                if next == 1 && app.read(cx).store.appearance == Appearance::System {
                    w.click("toggle-appearance", cx);
                }
                assert_eq!(app.read(cx).store.appearance, mode.toggled());
                let next = if app.read(cx).store.appearance == Appearance::Light {
                    2usize
                } else {
                    1usize
                };
                w.click("toggle-appearance", cx);
                if next == 1 && app.read(cx).store.appearance == Appearance::System {
                    w.click("toggle-appearance", cx);
                }
                assert_eq!(app.read(cx).store.appearance, mode);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "14-sidebar-scroll.png");
            cx.update_window(handle, |_, w, _| w.remove_window())
                .unwrap();
            drop(app);
            drop(cx);
        }
    }
    println!(
        "AFFORDANCE QA PASSED: 1440/1000 × light/dark, disabled no-op, hover, Tab/Enter save, close/reopen, approval, property menus, materials, settings and result links. Screenshots: {}",
        root.display()
    );
}

fn press(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, w, cx| w.press(key, cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}

fn click_in(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
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
    w.click(id.to_string(), cx);
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    for _ in 0..6 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
