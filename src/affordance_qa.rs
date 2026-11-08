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

const TOPICS: &str = r#"{"tasks":[{"title":"Semiconductor memory supply, demand and industry opportunities for the next year","brief":"Compare DRAM, NAND and HBM demand, record dates and sources, and identify risks and evidence gaps."},{"title":"Advanced packaging and equipment supply-chain research","brief":"Check changes in capacity, capital expenditure and demand."}]}"#;
const STEPS: &str = r#"{"tasks":[{"title":"Collect original announcements and news","instructions":"Save actual sources and dates.","kind":"collect","depends_on":[]},{"title":"Verify materials and analyze supply and demand","instructions":"Cite supplemental materials and earlier sources.","kind":"analyze","depends_on":[0]},{"title":"Prepare the final research report","instructions":"Separate facts, inferences and risks.","kind":"advise","depends_on":[1]}]}"#;

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
            store.projects[0].name = "Semiconductor industry research".into();
            store.research_config.models =
                ["qa/planner", "qa/collector", "qa/analyst", "qa/adviser"].map(String::from);
            store.appearance = mode;
            Storage::new(path.clone()).save(&store).unwrap();
            let mut cx = HeadlessAppContext::with_platform(
                platform::current_platform(true).text_system(),
                Arc::new(crate::ui_assets::AppAssets),
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

            screenshot(&mut cx, handle, &directory, "01-planning-empty.png");
            cx.update_window(handle, |_, w, cx| {
                let before = app.read(cx).store.clone();
                w.click("generate-project-plan", cx);
                assert_eq!(
                    app.read(cx).store,
                    before,
                    "Empty research request must have no side effect"
                );
                assert!(!app.read(cx).error.is_empty());
                w.hover("generate-project-plan", cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-planning-hover.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("project-plan-brief", cx);
                w.input("Global semiconductor market for the next year; require primary evidence and risks.", cx);
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
                    "Global semiconductor market for the next year; require primary evidence and risks.",
                    "Tab must leave the plain-text form without inserting indentation"
                );
            })
            .unwrap();
            press(&mut cx, handle, "shift-tab");
            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).modal_focus.contains_focused(w, cx));
            })
            .unwrap();
            press(&mut cx, handle, "tab");
            cx.update_window(handle, |_, w, _| {
                assert_eq!(w.find("save-project-context").focused(), Some(true));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-keyboard-focus.png");
            for key in ["tab", "shift-tab"] {
                for _ in 0..8 {
                    press(&mut cx, handle, key);
                    cx.update_window(handle, |_, w, cx| {
                        assert!(
                            app.read(cx).modal_focus.contains_focused(w, cx),
                            "Planning dialog lost its focus trap while navigating with {key}"
                        );
                    })
                    .unwrap();
                }
            }
            cx.update_window(handle, |_, w, cx| {
                w.click("save-project-context", cx);
                assert!(app.read(cx).planning_project.is_none());
                assert!(app.read(cx).store.research_requests.iter().any(|request| request.input.contains("Global semiconductor market")));
                // The overlay no longer changes the route; return to the project
                // through its real sidebar row before reopening from its toolbar.
                w.click("project-1", cx);
                w.render_frame(cx);
                w.click("plan-project", cx);
            }).unwrap();
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
                assert!(app.read(cx).store.runs().is_empty());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "04-planning-ready.png");

            let goal = cx
                .update_window(handle, |_, w, cx| {
                    w.click("close-project-plan", cx);
                    app.update(cx, |a, cx| {
                        let ids = a.store.apply_project_plan(1, TOPICS).unwrap();
                        a.navigate(Scope::All, w, cx);
                        a.layout = Layout::Board;
                        cx.notify();
                        ids[0]
                    })
                })
                .unwrap();
            // A reviewable plan is generated before approval. Approval then
            // covers the complete step graph; generating steps after approval
            // would invalidate the saved approval snapshot.
            let (children, material) = cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| {
                    let children = a.store.apply_proposed_research_plan(goal, STEPS).unwrap();
                    let source = directory.join("supplemental-materials-industry-announcements.pdf");
                    std::fs::write(&source, include_bytes!("test-fixtures/supplement.pdf")).unwrap();
                    let material = crate::attachments::import_file(&directory, &source).unwrap();
                    a.store.task_mut(goal).unwrap().attachments.push(material.clone());
                    cx.notify();
                    (children, material)
                })
            }).unwrap();
            cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
            screenshot(&mut cx, handle, &directory, "05-kanban.png");
            cx.update_window(handle, |_, w, cx| {
                let topics: Vec<u64> = app.read(cx).store.topics.iter().map(|topic| topic.id).collect();
                scroll_card_into_view(w, cx, &format!("approve-topic-{goal}"), &topics);
                w.click(format!("approve-topic-{goal}"), cx);
                assert_eq!(app.read(cx).store.task(goal).unwrap().stage, Stage::Planned);
                assert!(app.read(cx).store.runs().is_empty());
                app.update(cx, |a, cx| {
                    let mut run = a.store.prepare_run(children[0]).unwrap();
                    run.status = "Succeeded".into();
                    run.output = "## Research evidence (UI acceptance)\n\nThis is isolated QA data, not an actual market conclusion.\n\n[View the original announcement](https://example.com/announcement)\n\nSource links must remain visually distinct from the body.".into();
                    run.sources = vec![Source { title: "Original company announcement and filings: supply and demand data".into(), url: "https://example.com/announcement".into(), published_at: "2026-10-29 (QA)".into() }];
                    run.attachments = vec![material];
                    run.logs = vec!["[fixture] Native UI evidence only, no external process was launched.".into()];
                    run.ended_at = Some(crate::opencode::timestamp());
                    run.exit_code = Some(0);
                    a.store.push_run(run);
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
                w.click("workspace-menu", cx);
                w.render_frame(cx);
                w.within("popup-menu").click(3usize, cx);
                w.within("settings-tabs").click(1usize, cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-research.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("save-research-settings").is_none());
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "09-research-settings.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("workspace-menu", cx);
                w.render_frame(cx);
                w.within("popup-menu").click(3usize, cx);
                w.within("settings-tabs").click(1usize, cx);
                w.render_frame(cx);
                assert!(w.try_find("research-model-0").is_some());
                assert!(w.try_find("global-task-overrides").is_none());
            })
            .unwrap();
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "sidebar-scroll", "runs");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "12-result-links.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("result-source-0").is_none());
                click_in(w, cx, "research-result-scroll", "toggle-result-sources");
                assert!(w.try_find("result-source-0").is_some());
                let before = app.read(cx).store.clone();
                w.click("run-actions-menu", cx);
                w.render_frame(cx);
                w.click(0usize, cx);
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
                        extra.name = format!("QA industry research {id}");
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
        "AFFORDANCE QA PASSED: 1440/1000 × light/dark, empty request validation, hover, keyboard focus, save, close/reopen, approval, property menus, materials, settings and result links. Screenshots: {}",
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
    assert!(w.find(id.to_string()).visible(), "Could not scroll to {id}");
    w.click(id.to_string(), cx);
}

/// Scroll the Backlog cards column by dispatching at a visible topic header
/// inside it. The column itself is a layout node rather than an observed test
/// target, so use whichever topic header is currently visible and switch to a
/// later group after the column moves.
fn scroll_card_into_view(w: &mut Window, cx: &mut App, id: &str, topics: &[u64]) {
    w.render_frame(cx);
    for _ in 0..30 {
        let bounds = w.find(id.to_string()).bounds();
        let viewport = w.viewport_size();
        let top = px(110.);
        let bottom = viewport.height - px(40.);
        if bounds.top() >= top && bounds.bottom() <= bottom {
            break;
        }
        let anchor = topics.iter().find_map(|topic| {
            let anchor = format!("board-topic-backlog-{topic}");
            w.try_find(anchor.clone())
                .filter(|snapshot| snapshot.visible())
                .map(|_| anchor)
        });
        let Some(anchor) = anchor else { break; };
        let delta = if bounds.top() < top { 120. } else { -120. };
        w.scroll(
            anchor,
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
        w.render_frame(cx);
    }
    assert!(w.find(id.to_string()).visible(), "Could not scroll to {id}");
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
