//! Native acceptance of the result-first UI. Fixtures and child processes stay local.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
    persistence::Storage,
    research::{ResearchKind, Source},
};
use gpui_kit::{
    test::{TestQueryExt, TestWindowExt},
    *,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect demand-side evidence: capital expenditure and data-center plans across hyperscalers, second-tier clouds, enterprise builds and sovereign AI","instructions":"Isolated UI acceptance only; record original evidence.","kind":"collect","depends_on":[]},{"title":"Analyze supply and demand changes and risks","instructions":"Verify the prior materials.","kind":"analyze","depends_on":[0]},{"title":"Write the final research report","instructions":"Separate facts from assumptions.","kind":"advise","depends_on":[1]}]}"#;
const REPORT: &str = "## Semiconductor industry research\n\nThis is an isolated UI acceptance report, not actual market analysis.\n\n### Key conclusion\n\nThe result body should be the main reading surface. Raw logs, session IDs, model identifiers and run directories remain under technical details.\n\n### Items to verify\n\n- Compare the data definitions used by different companies.\n- Retain original sources and supplemental materials.\n- Let the user decide whether to continue the research.";
const CLI: &str = r#"#!/usr/bin/python3
import json,time
print(json.dumps({'type':'text','part':{'text':'{"status":"complete","report":"RAW_PROTOCOL_SENTINEL"}'}}),flush=True)
print(json.dumps({'type':'reasoning','part':{'text':'UI_QA_PROCESS_READY'}}),flush=True)
time.sleep(30)
"#;

const HEADER_PLAN: &str = r#"{"tasks":[{"title":"Collect long-form evidence for the header layout acceptance fixture","instructions":"Collect evidence.","kind":"collect","depends_on":[]},{"title":"Analyze the long-form evidence for the header layout acceptance fixture","instructions":"Analyze evidence.","kind":"analyze","depends_on":[0]},{"title":"Write the final report for the header layout acceptance fixture","instructions":"Write report.","kind":"advise","depends_on":[1]}]}"#;

/// Focused header containment acceptance for a deep report route.
pub fn header_layout() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/header-layout-qa/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name =
        "Semiconductor industry research: cross-market long-term trends, supply bottlenecks and supply-chain opportunities".into();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
    store.set_default_model("fixture/header".into());
    store.set_opencode_path("/bin/true".into());
    store.research_config_mut().default_agent = "OpenCode".into();
    store.research_config_mut().agents = std::array::from_fn(|_| Some("OpenCode".into()));
    store.research_config_mut().models = ["fixture/header"; 4].map(str::to_string);
    let topic = store
        .create_topic(
            1,
            "Long topic name for header layout: memory, packaging, equipment and materials",
        )
        .unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Long goal title for header layout: cross-market semiconductor demand, supply and valuation".into(),
            "Header layout fixture.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, HEADER_PLAN).unwrap();
    for task in &steps {
        let mut run = store.prepare_run(*task).unwrap();
        run.status = "Succeeded".into();
        run.output = REPORT.into();
        run.ended_at = Some(crate::opencode::timestamp());
        run.exit_code = Some(0);
        store.push_run(run);
        store.set_stage(*task, Stage::Done).unwrap();
    }
    let report_id = store.runs().last().unwrap().id;
    let path = root.join("workspace.json");
    Storage::new(path.clone()).save(&store).unwrap();
    for (width, height) in [(1000., 680.), (1440., 920.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
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
                        a.set_appearance(mode, w, cx);
                        a.open_task(goal, w, cx);
                        a.open_task(steps[2], w, cx);
                        a.view_task_result(steps[2], cx);
                        a.note_selected_run(Some(report_id));
                        cx.notify();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle = window.into();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                .unwrap();
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            screenshot(&mut cx, handle, &directory, "deep-report-header.png");
            cx.update_window(handle, |_, w, _| {
                let viewport = w.viewport_size().width;
                for id in [
                    "navigation-back",
                    "location-trail",
                    "location-current",
                    "open-tasks-menu",
                ] {
                    let bounds = w.find(id).bounds();
                    assert!(
                        bounds.right() <= viewport,
                        "{id} overflowed: {bounds:?} viewport={viewport:?}"
                    );
                }
                let menu = w.find("open-tasks-menu").bounds();
                let current = w.find("location-current").bounds();
                assert!(
                    current.right() <= menu.left(),
                    "current overlaps menu: {current:?} {menu:?}"
                );
            })
            .unwrap();
            for (label, sidebar_width) in [("sidebar-220", 220.), ("sidebar-480", 480.)] {
                cx.update_window(handle, |_, w, cx| {
                    let split = app.read(cx).sidebar_split.clone();
                    split.update(cx, |split, cx| {
                        split.resize_panel(0, px(sidebar_width), w, cx)
                    });
                    w.render_frame(cx);
                })
                .unwrap();
                cx.run_until_parked();
                cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                    .unwrap();
                screenshot(
                    &mut cx,
                    handle,
                    &directory,
                    &format!("deep-report-header-{label}.png"),
                );
                cx.update_window(handle, |_, w, _| {
                    for id in [
                        "navigation-back",
                        "location-trail",
                        "location-current",
                        "open-tasks-menu",
                    ] {
                        let bounds = w.find(id).bounds();
                        assert!(
                            bounds.right() <= w.viewport_size().width,
                            "{label}: {id} overflowed: {bounds:?}"
                        );
                    }
                })
                .unwrap();
            }
            if width > 1000. {
                let before = cx
                    .update_window(handle, |_, w, _| w.find("navigation-back").bounds())
                    .unwrap();
                cx.update_window(handle, |_, w, cx| {
                    w.resize(size(px(1000.), px(height)));
                    w.render_frame(cx);
                })
                .unwrap();
                cx.run_until_parked();
                cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                    .unwrap();
                screenshot(
                    &mut cx,
                    handle,
                    &directory,
                    "deep-report-header-resized-1000.png",
                );
                cx.update_window(handle, |_, w, _| {
                let viewport = w.viewport_size().width;
                let after = w.find("navigation-back").bounds();
                eprintln!("resize-down header: before={before:?} after={after:?} viewport={viewport:?}");
                for id in ["navigation-back", "location-trail", "location-current", "open-tasks-menu"] {
                    let bounds = w.find(id).bounds();
                    assert!(bounds.right() <= viewport, "resized: {id} overflowed: {bounds:?} viewport={viewport:?}");
                }
            }).unwrap();
            }
            cx.update_window(handle, |_, w, cx| w.click("open-tasks-menu", cx))
                .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                .unwrap();
            cx.update_window(handle, |_, w, _| {
                let menu = w.find("open-tasks-menu").bounds();
                assert!(menu.right() <= w.viewport_size().width);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "deep-report-header-menu.png");
            cx.update_window(handle, |_, w, cx| {
                let target = format!("recent-task-{goal}");
                assert!(
                    w.try_find(target.clone()).is_some(),
                    "recent menu must expose {target}"
                );
                w.click(target, cx);
            })
            .unwrap();
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(app.read(cx).selected(), Some(goal));
            })
            .unwrap();
            cx.update_window(handle, |_, w, _| w.remove_window())
                .unwrap();
        }
    }
    println!(
        "Header layout QA passed: deep report route at 1000/1440 px in light/dark; {}",
        root.display()
    );
}

pub fn run() {
    run_checks(false);
}
pub fn result_layout() {
    run_checks(true);
}
fn run_checks(layout_only: bool) {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/simplify-qa/{}", std::process::id()));
    let sizes = if layout_only { vec![(1440., 920.), (1000., 680.), (2672., 920.)] } else { vec![(1440., 920.), (1000., 680.)] };
    for (width, height) in sizes {
        for mode in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let cli = directory.join("fake-opencode");
            executable(&cli, CLI);
            let mut store = Store::empty();
            store.set_appearance(mode);
            store.project_at_mut(0).unwrap().name = "Semiconductor industry research".into();
            store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into_owned();
            store.set_default_model("qa/isolated-model".into());
            store.set_opencode_path(cli.to_string_lossy().into_owned());
            let goal = store
                .create_research_goal(
                    "AI compute chips and data-center semiconductor demand: demand structure, supply bottlenecks and value-chain changes over the next 12 months"
                        .into(),
                    "Study supply and demand, the value chain, risks and material evidence. Isolated UI acceptance only.".into(),
                    1,
                )
                .unwrap();
            let steps = store.apply_research_plan(goal, PLAN).unwrap();
            for task in &steps {
                let mut run = store.prepare_run(*task).unwrap();
                run.status = "Succeeded".into();
                run.output = if layout_only {
                    format!("{}\n\n| Field | Value |\n| --- | --- |\n| File | `/Users/fixture/workspace/research/attachments/2026-11-09-evidence-source-document-with-a-long-filename-and-original-metadata.pdf` |\n\n{}", REPORT, REPORT.repeat(12))
                } else {
                    REPORT.into()
                };
                run.session_id = Some("ses_UI_QA_TECHNICAL_METADATA".into());
                run.logs = vec!["[fixture] RAW_LOG_SENTINEL, local UI acceptance only".into()];
                run.sources = vec![Source {
                    title: "Isolated acceptance source, not real market evidence".into(),
                    url: "https://example.com/ui-qa".into(),
                    published_at: "2026-10-29 QA".into(),
                }];
                run.ended_at = Some(crate::opencode::timestamp());
                run.exit_code = Some(0);
                store.push_run(run);
                store.set_stage(*task, Stage::Done).unwrap();
            }
            let report_id = store.runs().last().unwrap().id;
            let ordinary = store
                .create_task(
                    "Real child-process UI acceptance: starts only a local waiting script and calls no model or browser",
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
                        a.note_selected_run(Some(report_id));
                        a.navigate(Scope::Runs, w, cx);
                        if layout_only {
                            a.set_result_selection(Some(*steps.last().unwrap()), Some(report_id));
                        }
                    });
                    entity = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap()
                .into();
            let app = entity.unwrap();
            if layout_only {
                screenshot(&mut cx, handle, &directory, "compact-result.png");
                for sidebar_width in [480., 260.] {
                    cx.update_window(handle, |_, w, cx| {
                        let sidebar = app.read(cx).sidebar_split.clone();
                        sidebar.update(cx, |s, cx| s.resize_panel(0, px(sidebar_width), w, cx));
                        w.render_frame(cx);
                    }).unwrap();
                    screenshot(&mut cx, handle, &directory, &format!("report-sidebar-{sidebar_width}.png"));
                    cx.update_window(handle, |_, w, _| {
                        let viewport = w.find("research-result-viewport").bounds();
                        let scroll = w.find("research-result-scroll").bounds();
                        let regenerate = w.find("regenerate-report").bounds();
                        let previous = w.find("previous-task").bounds();
                        let next = w.find("next-task").bounds();
                        assert!(regenerate.right() <= previous.left() && previous.right() <= next.left(),
                            "Regenerate must precede the Previous/Next arrows at window width {width}, sidebar {sidebar_width}");
                        assert!((viewport.right() - (w.viewport_size().width - px(16.))).abs() <= px(1.),
                            "Report must fill the available right pane at window width {width}, sidebar {sidebar_width}: {viewport:?}");
                        assert!((scroll.size.width - viewport.size.width).abs() <= px(1.),
                            "Report scroll surface must match its viewport: {scroll:?} in {viewport:?}");
                    }).unwrap();
                }
                cx.update_window(handle, |_, w, cx| {
                    w.click("regenerate-report", cx);
                    w.render_frame(cx);
                    assert!(w.find("confirm-regenerate-report").visible());
                    assert!(app.read(cx).report_regeneration.is_some());
                })
                .unwrap();
                screenshot(&mut cx, handle, &directory, "regenerate-report.png");
                cx.update_window(handle, |_, w, cx| {
                    w.click("cancel-regenerate-report", cx);
                    w.render_frame(cx);
                    assert!(app.read(cx).report_regeneration.is_none());
                    assert_eq!(app.read(cx).store.runs().len(), store.runs().len());
                })
                .unwrap();

                cx.update_window(handle, |_, w, cx| {
                    assert!(w.find("research-result-scroll").visible());
                    assert!(w.try_find("run-technical-panel").is_none());
                    assert_eq!(app.read(cx).store.runs().len(), store.runs().len());
                    let viewport = w.find("research-result-viewport").bounds();
                    let source_top = w.find("toggle-result-sources").bounds().top();
                    w.scroll(
                        "research-result-scroll",
                        ScrollDelta::Pixels(point(px(0.), px(-400.))),
                        cx,
                    );
                    w.render_frame(cx);
                    assert!(app.read(cx).result_scroll.offset().y < px(-300.));
                    assert!(w.find("toggle-result-sources").bounds().top() < source_top);
                    assert_eq!(w.find("research-result-viewport").bounds(), viewport);
                    w.scroll(
                        "research-result-scroll",
                        ScrollDelta::Pixels(point(px(0.), px(-100000.))),
                        cx,
                    );
                    w.render_frame(cx);
                    assert!(w.find("toggle-result-sources").visible());
                    assert_eq!(w.find("research-result-viewport").bounds(), viewport);
                })
                .unwrap();
                screenshot(&mut cx, handle, &directory, "result-scrolled-bottom.png");
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| a.open_task(steps[1], w, cx));
                    w.render_frame(cx);
                    assert!(w.find("step-run").visible());
                    assert!(w.find("step-open-goal").visible());
                    assert!(w.find("step-attempt-details").visible());
                })
                .unwrap();
                screenshot(&mut cx, handle, &directory, "completed-step-actions.png");
                for step in &steps {
                    cx.update_window(handle, |_, w, cx| {
                        app.update(cx, |a, cx| a.open_task(*step, w, cx));
                        w.render_frame(cx);
                        assert!(w.try_find("task-children-heading").is_none());
                        assert!(w.find("add-attachments").visible());
                    })
                    .unwrap();
                }
                screenshot(&mut cx, handle, &directory, "report-materials.png");
                cx.update(|cx| cx.quit());
                continue;
            }
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
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-actions-menu", 0);
            cx.update_window(handle, |_, w, cx| {
                let before = app.read(cx).store.clone();
                assert_eq!(w.find_all("show-result").len(), 1);
                let show_result = w.find("show-result").bounds();
                let actions_menu = w.find("run-actions-menu").bounds();
                assert_eq!(
                    show_result.origin.y, actions_menu.origin.y,
                    "same-row toolbar controls share a top edge"
                );
                assert_eq!(
                    show_result.size.height, actions_menu.size.height,
                    "same-row toolbar controls share a height"
                );
                assert!(
                    show_result.right() <= w.viewport_size().width
                        && actions_menu.right() <= w.viewport_size().width,
                    "toolbar actions stay inside the viewport"
                );
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
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-actions-menu", 0);
            cx.update_window(handle, |_, w, cx| {
                let state = app.read(cx);
                assert_eq!(
                    state.selected_run(),
                    Some(report_id),
                    "selected run changed before history selection"
                );
                let other = state
                    .store
                    .runs()
                    .iter()
                    .rev()
                    .find(|run| run.id != report_id)
                    .expect("another execution in the history")
                    .id;
                click_in(w, cx, "run-history", &format!("run-{other}"));
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
                    "Complete the website permission prompt in the browser, then return to continue the task.",
                    "05-blocked.png",
                ),
                ("Stale", REPORT, "06-stale.png"),
            ] {
                cx.update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        let mut r = a.store.run_at_mut(0).unwrap();
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
                    assert!(w.find("run-actions-menu").visible());
                })
                .unwrap();
                screenshot(&mut cx, handle, &directory, name);
            }

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.store.run_at_mut(0).unwrap().status = "Succeeded".into();
                    a.store.run_at_mut(0).unwrap().output = REPORT.into();
                    let planner = a
                        .store
                        .create_research_goal(
                            "Local cancellation fixture".into(),
                            "Wait for cancellation.".into(),
                            1,
                        )
                        .unwrap();
                    let cancel_steps = a.store.apply_research_plan(planner, PLAN).unwrap();
                    a.store.set_stage(planner, Stage::Planned).unwrap();
                    a.start_run(cancel_steps[0], cx);
                    a.poll_runtime(cx);
                    assert!(!a.active_runs.is_empty(), "{}", a.error);
                });
                w.render_frame(cx);
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.store
                    .runs()
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
            choose_menu(&mut cx, handle, "run-topic-filter", 0); // Research goal
            cx.update_window(handle, |_, w, cx| {
                click_in(w, cx, "run-history", "run-1");
                assert!(
                    w.try_find("cancel-run").is_none(),
                    "A history record must not stop another run"
                );
                assert_eq!(app.read(cx).active_runs.first().unwrap().id, active_id);
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-topic-filter", 1); // Standalone task
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).selected_run(), Some(active_id));
                assert_eq!(w.find_all("cancel-run").len(), 1);
                w.click("cancel-run", cx);
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.active_runs.is_empty() && a.queued_runs.is_empty()
            });
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(
                    app.read(cx).store.runs().last().unwrap().status,
                    "Cancelled"
                )
            })
            .unwrap();

            // A completed goal must open its final child report, not its old planning JSON.
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let mut planning = a.store.runs()[0].clone();
                    planning.id = a.store.runs().iter().map(|r| r.id).max().unwrap() + 1;
                    planning.task_id = goal;
                    planning.task_title = a.store.task(goal).unwrap().title.clone();
                    planning.research_kind = Some(ResearchKind::Goal);
                    planning.output = PLAN.into();
                    planning.sources.clear();
                    a.store.push_run(planning);
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
                assert_eq!(app.read(cx).selected_run(), Some(report_id));
                assert_eq!(app.read(cx).scope(), Scope::Runs);
                assert!(w.find("research-result-scroll").visible());
                assert_eq!(app.read(cx).store, before);
                assert_eq!(app.read(cx).task_result_context(), Some(goal));
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(w.try_find("run-navigation").is_none());
                w.click("navigation-back", cx);
                let state = app.read(cx);
                assert_eq!(
                    state.selected(),
                    Some(goal),
                    "scope={:?} context={:?} tabs={:?}",
                    state.scope(),
                    state.task_result_context(),
                    state.tabs
                );
                assert!(state.task_result_context().is_none());
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
                assert_eq!(app.read(cx).task_result_context(), Some(steps[0]));
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(w.try_find("run-navigation").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-inline-task-result.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("run-actions-menu", cx);
                w.render_frame(cx);
                w.click(1usize, cx);
                assert_eq!(app.read(cx).selected(), Some(steps[0]));
                assert_eq!(app.read(cx).detail_scroll.offset(), task_scroll);
                app.update(cx, |a, cx| a.navigate(Scope::Runs, w, cx));
                w.render_frame(cx);
                assert!(app.read(cx).task_result_context().is_none());
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
                assert_eq!(app.read(cx).scope(), Scope::All);
                assert_eq!(app.read(cx).sidebar_scope(), Scope::Project(1));
                assert!(app.read(cx).expanded_sidebar_projects.contains(&1));
                assert!(w.try_find(format!("sidebar-task-{}", steps[0])).is_some());
                let scope = app.read(cx).scope();
                let selected = app.read(cx).selected();
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert!(w.try_find(format!("sidebar-task-{goal}")).is_none());
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert_eq!(app.read(cx).scope(), scope);
                assert_eq!(app.read(cx).selected(), selected);
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
                let hover_id = format!("sidebar-task-{}", steps[1]);
                scroll_to(w, cx, "sidebar-scroll", &hover_id);
                w.hover(hover_id, cx);
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
                assert_eq!(app.read(cx).selected(), None);
                assert_eq!(app.read(cx).task_result_context(), Some(steps[0]));
                assert_eq!(app.read(cx).scope(), Scope::Runs);
                click_in(w, cx, "sidebar-scroll", "project-1");
                assert_eq!(app.read(cx).selected(), None);
                assert_eq!(app.read(cx).scope(), Scope::Project(1));
                assert!(app.read(cx).expanded_sidebar_projects.contains(&1));
                click_in(w, cx, "sidebar-scroll", "expand-project-1");
                assert!(w.try_find(format!("sidebar-task-{goal}")).is_none());
                app.update(cx, |a, cx| a.open_task(steps[0], w, cx));
            })
            .unwrap();

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| a.open_task(goal, w, cx));
                w.render_frame(cx);
                assert!(
                    w.try_find("research-all").is_some(),
                    "goal detail keeps its explicit run control"
                );
                assert!(w.try_find("research-next").is_none());
                scroll_to(w, cx, "detail-scroll", &format!("subtask-{}", steps[0]));
                assert!(w.find(format!("subtask-{}", steps[0])).bounds().size.height <= px(36.));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "08-compact-subtasks.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("subtask-{}", steps[0]), cx);
                assert_eq!(app.read(cx).selected(), Some(steps[0]));
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
                assert!(
                    w.find_by_label("Edit task").visible(),
                    "Edit task menu item"
                );
                assert!(
                    w.find_by_label("Archive task").visible(),
                    "Archive task menu item"
                );
                w.click(0usize, cx);
                w.render_frame(cx);
                assert!(app.read(cx).editing(), "menu opens the real Edit task form");
                assert!(app.read(cx).draft.is_some());
                scroll_to(w, cx, "draft-scroll", "discard-draft");
                w.click("discard-draft", cx);
                w.render_frame(cx);
                assert!(!app.read(cx).editing());
                assert!(app.read(cx).draft.is_none());
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
                assert_eq!(w.find("step-run").label(), Some("Run again"));
                assert!(w.try_find("configure-research").is_none());
                assert!(w.try_find("property-Cycle").is_none());
                assert!(
                    w.try_find("property-Stage").is_none(),
                    "research steps do not expose manual stage controls"
                );
                assert!(w.try_find("property-Priority").is_some());
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
                app.update(cx, |a, cx| a.navigate(Scope::Research, w, cx));
                w.render_frame(cx);
                assert!(w.try_find("save-research-settings").is_none());
                assert!(w.try_find("create-research").is_some());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "10-research.png");
            cx.update_window(handle, |_, w, cx| {
                let before = app.read(cx).store.clone();
                w.click("research-settings-toggle", cx);
                assert_eq!(app.read(cx).scope(), Scope::Settings);
                assert!(w.try_find("research-settings-toggle").is_none());
                assert!(
                    w.try_find("create-research").is_none(),
                    "Advanced settings replace the composer in this view"
                );
                assert!(w.try_find("ego-path").is_some());
                assert_eq!(app.read(cx).store, before);
                scroll_to(w, cx, "model-settings-scroll", "research-model-0");
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "11-research-settings.png");
            cx.update_window(handle, |_, w, cx| {
                scroll_to(w, cx, "model-settings-scroll", "ego-path")
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "11b-browser-settings.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("navigation-back", cx);
                app.update(cx, |a, cx| a.navigate(Scope::Research, w, cx));
                w.render_frame(cx);
                assert!(w.try_find("create-research").is_some());
                assert!(w.try_find("research-model-0").is_none());
                scroll_to(w, cx, "research-scroll", &format!("research-goal-{goal}"));
                assert!(w.find(format!("research-goal-{goal}")).bounds().size.height <= px(36.));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "12-compact-research-goals.png");
            cx.update_window(handle, |_, w, cx| {
                w.click(format!("research-goal-{goal}"), cx);
                assert_eq!(app.read(cx).selected(), Some(goal));
                w.click("project-1", cx);
                assert_eq!(app.read(cx).scope(), Scope::Project(1));
                assert!(w.try_find("run-project-todo").is_some());
                app.update(cx, |a, _| {
                    a.store.normalize_topics();
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
                    a.store.research_config_mut().ego_path = ego.to_string_lossy().into_owned();
                    // Explicit fixture reopen: mark the completed attempt as a
                    // failed prior attempt so the ordinary rerun is eligible
                    // without weakening the eligibility guard.
                    let prior: Vec<u64> = a
                        .store
                        .runs()
                        .iter()
                        .filter(|run| run.task_id == steps[0] && run.project_plan.is_none())
                        .map(|run| run.id)
                        .collect();
                    for run in prior {
                        if let Some(mut run) = a.store.run_mut(run) {
                            run.status = "Failed".into();
                        }
                    }
                    a.start_run(steps[0], cx);
                    a.poll_runtime(cx);
                    assert!(
                        !a.active_runs.is_empty() || !a.queued_runs.is_empty(),
                        "{}",
                        a.error
                    );
                    assert_eq!(a.store.task(steps[0]).unwrap().stage, Stage::InProgress);
                });
            })
            .unwrap();
            wait(&mut cx, handle, &app, |a| {
                a.store
                    .runs()
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
                    let index = a.store.runs().len() - 1;
                    let mut run = a.store.run_at_mut(index).unwrap();
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
                    a.store.normalize_topics();
                    a.persist_if_changed(true);
                    assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap(), a.store);
                });
            })
            .unwrap();
            // Project identity, attempt history and compact toolbar are exercised together.
            let (latest_id, other_run_id) = cx
                .update_window(handle, |_, w, cx| {
                    let ids = app.update(cx, |a, cx| {
                        let mut second = a.store.projects()[0].clone();
                        second.id = 2;
                        second.name = "Memory industry research".into();
                        second.key = "MEM".into();
                        a.store.push_project(second);
                        let duplicate = a
                            .store
                            .create_research_goal(
                                "Write the final research report".into(),
                                "Isolated history fixture".into(),
                                2,
                            )
                            .unwrap();
                        let mut other = a.store.prepare_run(duplicate).unwrap();
                        other.status = "Succeeded".into();
                        other.output = REPORT.into();
                        let other_id = other.id;
                        a.store.push_run(other);
                        let another_goal = a
                            .store
                            .create_research_goal(
                                "Another research goal: waiting to review scope and evidence"
                                    .into(),
                                "Isolated hierarchy acceptance only".into(),
                                1,
                            )
                            .unwrap();
                        let mut untouched = a.store.prepare_run(another_goal).unwrap();
                        untouched.status = "Failed".into();
                        a.store.push_run(untouched);
                        let mut latest = a
                            .store
                            .runs()
                            .iter()
                            .find(|r| r.id == report_id)
                            .unwrap()
                            .clone();
                        latest.id = a.store.runs().iter().map(|r| r.id).max().unwrap() + 1;
                        latest.status = "Blocked".into();
                        latest.output =
                            "Continue after handling the browser permission prompt.".into();
                        let latest_id = latest.id;
                        a.store.push_run(latest);
                        a.note_selected_run(Some(latest_id));
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
                assert_eq!(app.read(cx).selected_run(), Some(report_id));
                assert!(w.find("research-result-scroll").visible());
                assert!(w.try_find(format!("run-{latest_id}")).is_some());
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-version-picker", 0);
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(app.read(cx).selected_run(), Some(latest_id));
                assert_eq!(
                    app.read(cx).store,
                    before,
                    "Version browsing must not modify results"
                );
            })
            .unwrap();
            choose_menu(&mut cx, handle, "run-project-planning", 0);
            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).show_planning_activity);
                let selected = app.read(cx).selected_run().expect("planning record selected");
                let selected_run = app
                    .read(cx)
                    .store
                    .runs()
                    .iter()
                    .find(|r| r.id == selected)
                    .unwrap();
                assert!(
                    selected_run.project_plan.is_some()
                        || selected_run.research_kind == Some(ResearchKind::Goal),
                    "planning history must select a planning record"
                );
                app.update(cx, |a, cx| a.view_task_result(steps[2], cx));
                w.render_frame(cx);
                assert!(w.try_find(format!("run-{latest_id}")).is_some());
                assert!(!app.read(cx).show_planning_activity);
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
                    if control == "open-tasks-menu" {
                        let bounds = w.find(control).bounds();
                        assert!(
                            bounds.right() <= w.viewport_size().width,
                            "Recently opened clipped before open: {bounds:?} viewport={:?}",
                            w.viewport_size()
                        );
                    }
                    w.click(control, cx);
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
                cx.run_until_parked();
                cx.update_window(handle, |_, w, cx| w.render_frame(cx))
                    .unwrap();
            }

            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.note_selected_run(Some(report_id));
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
                    app.read(cx).selected_run(),
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
                assert_eq!(app.read(cx).scope(), Scope::Project(2));
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
                w.click("plan-project", cx);
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
                assert!(app.read(cx).store.runs().iter().any(|r| r.id == latest_id));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "19-project-list.png");
            // Establish the report task as the explicit caller. The result's
            // original-task action records a return to the same result; only
            // after caller history is exhausted does Back follow breadcrumbs.
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.structural_navigation();
                    a.open_task(steps[2], w, cx);
                    a.record_origin();
                    a.view_task_result(steps[2], cx);
                });
                w.render_frame(cx);
                assert!(w.find("location-all").visible());
                assert_eq!(app.read(cx).task_result_context(), Some(steps[2]));
            })
            .unwrap();
            let original_result = cx.update(|cx| app.read(cx).selected_run());
            assert!(original_result.is_some());
            choose_menu(&mut cx, handle, "run-actions-menu", 1);
            cx.update_window(handle, |_, w, cx| {
                println!("Result origin before Back: {:?}; history: {:?}", app.read(cx).capture_origin(), app.read(cx).return_frames());
                assert_eq!(app.read(cx).selected(), Some(steps[2]));
                w.render_frame(cx);
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).selected().is_none(), "Back must restore result; selected={:?}, origin={:?}, history={:?}, notice={}", app.read(cx).selected(), app.read(cx).capture_origin(), app.read(cx).return_frames(), app.read(cx).toast);
                assert_eq!(app.read(cx).selected_run(), original_result);
                assert_eq!(app.read(cx).task_result_context(), Some(steps[2]));
                assert!(app.read(cx).show_run_result);
                w.render_frame(cx);
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).selected(), Some(steps[2]));
                w.render_frame(cx);
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).selected(), Some(goal));
                w.render_frame(cx);
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).scope(), Scope::Topic(1));
                w.render_frame(cx);
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, _, cx| {
                assert_eq!(app.read(cx).scope(), Scope::Project(1));
                assert!(app.read(cx).selected().is_none());
                app.update(cx, |a, cx| a.view_task_result(steps[2], cx));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "24-focused-progress.png");
            // Construct explicit root fixtures for the status/search filters.
            let mut done_ids = (0, 0, 0, 0);
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let done_a = a
                        .store
                        .create_task("Completed root alpha", 1, Stage::Done, None)
                        .unwrap();
                    let done_search = a
                        .store
                        .create_task("Analysis completed root", 1, Stage::Done, None)
                        .unwrap();
                    let cancelled = a
                        .store
                        .create_task("Cancelled filter fixture", 1, Stage::Cancelled, None)
                        .unwrap();
                    let archived = a
                        .store
                        .create_task("Archived filter fixture", 1, Stage::Done, None)
                        .unwrap();
                    a.store.task_mut(archived).unwrap().archived = true;
                    done_ids = (done_a, done_search, cancelled, archived);
                    a.navigate(Scope::All, w, cx);
                    let mut visible: Vec<u64> = a.visible_tasks().iter().map(|t| t.id).collect();
                    visible.sort_unstable();
                    assert!(visible.contains(&done_a));
                    assert!(visible.contains(&done_search));
                    assert!(visible.contains(&cancelled));
                    assert!(!visible.contains(&archived));
                    assert!(a.visible_tasks().iter().all(|t| !t.archived));
                });
                w.render_frame(cx);
                assert_eq!(w.find("collection-title").label(), Some("All research"));
                w.click("all-status-filter", cx);
                w.click(7usize, cx); // Done
                assert_eq!(
                    app.read(cx).all_status,
                    crate::app::AllStatus::Stage(Stage::Done)
                );
                let mut done: Vec<u64> =
                    app.read(cx).visible_tasks().iter().map(|t| t.id).collect();
                done.sort_unstable();
                assert_eq!(done, vec![done_ids.0, done_ids.1]);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.click("task-search", cx);
                w.input("Analysis", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert_eq!(app.read(cx).query(), "Analysis");
                let ids: Vec<u64> = app.read(cx).visible_tasks().iter().map(|t| t.id).collect();
                assert_eq!(ids, vec![done_ids.1]);
                assert!(w.try_find("show-done").is_none());
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "20-all-status.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("collection-project-1", cx);
                assert_eq!(app.read(cx).scope(), Scope::Project(1));
                assert!(app.read(cx).query().is_empty());
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
                assert_eq!(app.read(cx).scope(), Scope::Topic(1));
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope(), Scope::Project(1));
                assert!(app.read(cx).selected().is_none());
                assert!(app.read(cx).layout == crate::app::Layout::Board);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "21-project-return.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("navigation-back", cx);
                assert_eq!(app.read(cx).scope(), Scope::All);
                assert!(app.read(cx).query().is_empty());
                assert!(w.try_find("navigation-back").is_none());
                assert_eq!(
                    app.read(cx).all_status,
                    crate::app::AllStatus::Stage(Stage::Done)
                );
                let mut done: Vec<u64> =
                    app.read(cx).visible_tasks().iter().map(|t| t.id).collect();
                done.sort_unstable();
                assert_eq!(done, vec![done_ids.0, done_ids.1]);
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
                    a.clear_query();
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
                w.click(6usize, cx); // Cancelled
                let ids: Vec<u64> = app.read(cx).visible_tasks().iter().map(|t| t.id).collect();
                assert_eq!(ids, vec![done_ids.2]);
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
                click_in(w, cx, "detail-scroll", "set-backlog");
                assert_eq!(
                    app.read(cx).store.task(ordinary).unwrap().stage,
                    Stage::Backlog
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
                    a.store.project_at_mut(0).unwrap().name =
                        "Semiconductor industry research: cross-market long-term trends, supply bottlenecks and supply-chain opportunities".into();
                    cx.notify();
                });
                w.render_frame(cx);
                assert!(w.find("property-Topic project").bounds().size.width <= px(164.));
                assert!(w.find("property-Topic project").bounds().right() <= w.viewport_size().width);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "23-long-property.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    let header_task = a
                        .store
                        .create_task(
                            "Long header layout fixture with a readable but extended title",
                            1,
                            Stage::Backlog,
                            None,
                        )
                        .unwrap();
                    a.navigate(Scope::Project(1), w, cx);
                    a.open_task(header_task, w, cx);
                });
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "23b-long-header.png");
            cx.update_window(handle, |_, w, cx| {
                for id in ["navigation-back", "location-trail"] {
                    let bounds = w.find(id).bounds();
                    assert!(
                        bounds.right() <= w.viewport_size().width,
                        "Header action {id} overflowed"
                    );
                    assert!(bounds.size.height <= px(44.));
                }
                let menu = w.try_find("open-tasks-menu").unwrap_or_else(|| {
                    panic!("missing open-tasks-menu; viewport={:?}", w.viewport_size())
                });
                let menu_bounds = menu.bounds();
                assert!(
                    menu.visible(),
                    "Recently opened menu clipped: menu={menu_bounds:?} viewport={:?}",
                    w.viewport_size()
                );
                assert!(menu_bounds.right() <= w.viewport_size().width);
                assert!(menu_bounds.size.height <= px(44.));
                let title = w.find("location-current").bounds();
                assert!(title.size.height <= px(42.));
                assert!(title.right() <= w.viewport_size().width);
                assert!(
                    title.right() <= menu_bounds.left(),
                    "title={title:?} overlaps menu={menu_bounds:?}"
                );
                w.click("navigation-back", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                let title = w.find("location-current").bounds();
                assert!(title.size.height <= px(42.));
                assert!(title.right() <= w.viewport_size().width);
                app.update(cx, |a, cx| {
                    let mut planning = a
                        .store
                        .runs()
                        .iter()
                        .find(|r| r.task_id == goal)
                        .unwrap()
                        .clone();
                    planning.id = a.store.runs().iter().map(|r| r.id).max().unwrap() + 1;
                    planning.status = "Failed".into();
                    a.store.push_run(planning);
                    a.note_selected_run(None);
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
                    a.store.push_run(run);
                    a.research_sequences.clear();
                    a.complete_research_run(run_id, cx);
                    assert_eq!(a.scope(), Scope::Settings, "Background planning must not change the user's page");
                    let mut project = a.store.projects()[0].clone();
                    project.id = 900; project.name = "Background project planning".into();
                    a.store.push_project(project);
                    let mut run = a.store.prepare_project_plan(900).unwrap();
                    run.status = "Succeeded".into();
                    run.output = r#"{"tasks":[{"title":"Local proposal","brief":"Only a navigation fixture."}]}"#.into();
                    a.store.push_run(run);
                    a.finalize_project_plan(a.store.runs().len()-1, Some(cx));
                    assert_eq!(a.scope(), Scope::Settings);
                    a.navigate(Scope::All, w, cx);
                    let removed = a.store.create_task("Removed navigation target", 1, Stage::Backlog, None).unwrap();
                    a.open_task(removed, w, cx);
                    a.open_task(ordinary, w, cx);
                    a.store.retain_tasks(|t| t.id != removed);
                    a.go_back(w, cx);
                    assert_eq!(
                        a.scope(),
                        Scope::Topic(a.store.task_topic(ordinary).unwrap().id),
                        "Back falls back to the task's structural topic"
                    );
                    assert!(a.selected().is_none(), "Back uses ownership, not previously visited tasks");
                });
                w.press("cmd-k", cx);
            }).unwrap();
            cx.update(|cx| cx.quit());
        }
    }
    println!(
        "{}; screenshots: {}",
        if layout_only {
            "Result layout QA passed (wide/narrow, light/dark)"
        } else {
            "Simplified UI acceptance passed"
        },
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
        let report_task = before
            .runs()
            .iter()
            .find(|r| r.id == report)
            .unwrap()
            .task_id;
        let project = before.task(goal).unwrap().project_id;
        let topic = before.task_topic(goal).unwrap().id;
        assert!(w.find("location-all").visible());
        assert!(w.find("location-task").visible());
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected(), Some(report_task));
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected(), Some(goal));
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope(), Scope::Topic(topic));
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope(), Scope::Project(project));
        assert!(app.read(cx).selected().is_none());
        w.press("cmd-[", cx);
        assert_eq!(app.read(cx).scope(), Scope::All);
        assert!(w.try_find("navigation-back").is_none());
        // Opening a goal from an unrelated page must still return to its project.
        app.update(cx, |a, cx| {
            a.navigate(Scope::Settings, w, cx);
            a.open_task(goal, w, cx);
        });
        w.render_frame(cx);
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope(), Scope::Topic(topic));
        app.update(cx, |a, cx| a.open_task(child, w, cx));
        w.render_frame(cx);
        assert!(w.try_find("location-parent").is_some());
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).selected(), Some(goal));
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
                5,
                "All, project, topic, goal and parent must all be present"
            );
            a.go_back(w, cx);
            assert_eq!(a.selected(), Some(child));
            a.store = before.clone();
            a.tabs.retain(|id| *id != nested);
            a.open_task(goal, w, cx);
        });
    })
    .unwrap();
    screenshot(cx, handle, directory, "00-goal-details.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.note_selected_run(Some(report));
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
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
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
        if bounds.top() >= top && bounds.bottom() <= bottom && w.find(id.to_string()).visible() {
            break;
        }
        let delta = if bounds.top() < top { 120. } else { -120. };
        w.scroll(
            scroll.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
        w.render_frame(cx);
    }
    w.render_frame(cx);
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
