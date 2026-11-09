//! Sidebar navigation opens completed research results and preserves automation periods.
//!
//! The existing `--qa-sidebar` entry calls this after its startup checks, so the
//! suite exercises real Previous/Next/Overview/Back controls without adding a
//! new dispatch flag.
use crate::{
    app::{Scope, TaskApp},
    automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
        OccurrenceStatus, TimeWindow, TriggerSource,
    },
    automation_schedule::{Schedule, ScheduleSpec},
    domain::{Stage, Store},
    persistence::Storage,
};
use gpui_kit::{
    component::{Theme, ThemeMode},
    prelude::*,
    test::TestWindowExt,
    *,
};
use std::{path::Path, sync::Arc, time::Duration};

struct Fixture {
    research_goal: u64,
    research_steps: Vec<u64>,
    research_runs: Vec<u64>,
    first_automation: u64,
    first_period: u64,
    first_steps: Vec<u64>,
    second_period: u64,
    second_step: u64,
    other_period: u64,
    other_second_period: u64,
}

fn store() -> Store {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
    store.set_default_model("fixture/navigation".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"]
            .map(String::from);
    store
}

fn template() -> crate::automation::AutomationTemplate {
    crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()
}

fn add_automation(store: &mut Store, name: &str) -> u64 {
    store
        .create_automation_draft(
            1,
            AutomationShape::Research,
            name.into(),
            "Track public filings".into(),
            "A cited note".into(),
            Schedule {
                spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                timezone: "Asia/Shanghai".into(),
            },
            AutomationPolicy::default(),
            Some(template()),
            1_000,
        )
        .unwrap()
}

fn add_period(store: &mut Store, automation: u64, scheduled: i64) -> u64 {
    let definition = store.automation(automation).unwrap().clone();
    let occurrence = AutomationOccurrence {
        id: store.next_occurrence_id().unwrap(),
        automation_id: automation,
        definition_version: definition.version,
        source: TriggerSource::Scheduled,
        trigger_key: crate::automation::scheduled_trigger_key(automation, scheduled),
        scheduled_for: Some(scheduled),
        requested_at: None,
        actual_started_at: Some(scheduled + 60),
        timezone: definition.schedule.timezone.clone(),
        reference_time: scheduled,
        data_window: TimeWindow::unresolved("navigation QA window"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: definition.objective.clone(),
            expected_result: definition.expected_result.clone(),
            shape: Some(definition.shape),
            template: definition.template.clone().unwrap(),
            models: ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"]
                .map(String::from),
            agents: Default::default(),
            materials: vec![],
            policy: definition.policy.clone(),
            approval_version: definition.version,
            approval_signature: "navigation QA approval".into(),
        },
        status: OccurrenceStatus::Running,
        reason: None,
        research_goal_id: None,
        research_goal_ids: vec![],
        research_topic_id: None,
        run_ids: vec![],
        final_run_id: None,
        unread: false,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: scheduled,
        deadline_at: None,
        retry_requested_at: None,
    };
    store.materialize_occurrence(occurrence).unwrap().id
}

fn finish_steps(store: &mut Store, occurrence: u64) -> Vec<u64> {
    let steps = store.automation_period_steps(occurrence);
    for step in &steps {
        let mut run = store.prepare_run(*step).unwrap();
        run.automation_occurrence_id = Some(occurrence);
        run.status = "Succeeded".into();
        run.output = format!("Completed navigation report for step {step}");
        store.push_run(run);
        store.set_stage(*step, Stage::Done).unwrap();
    }
    steps
}

fn fixture() -> (Store, Fixture) {
    let mut store = store();
    let first_automation = add_automation(&mut store, "First automation");
    let second_automation = add_automation(&mut store, "Second automation");
    let first_period = add_period(&mut store, first_automation, 1_000);
    let second_period = add_period(&mut store, first_automation, 3_000);
    let other_period = add_period(&mut store, second_automation, 5_000);
    let other_second_period = add_period(&mut store, second_automation, 7_000);
    let second_steps = finish_steps(&mut store, second_period);
    let _ = finish_steps(&mut store, other_period);
    let _ = finish_steps(&mut store, other_second_period);
    let first_steps = finish_steps(&mut store, first_period);
    let research_goal = store.create_research_goal(
        "Completed ordinary research".into(), "Verify result-first navigation.".into(), 1,
    ).unwrap();
    let research_steps = store.apply_research_plan(research_goal,
        r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect sources.","kind":"collect","depends_on":[]},{"title":"Analyze evidence","instructions":"Analyze sources.","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Write the report.","kind":"advise","depends_on":[1]}]}"#,
    ).unwrap();
    let mut research_runs = Vec::new();
    for step in &research_steps {
        let mut run = store.prepare_run(*step).unwrap();
        run.status = "Succeeded".into();
        run.output = format!("# Ordinary research report {step}\n\nCompleted navigation fixture.");
        research_runs.push(run.id);
        store.push_run(run);
        store.set_stage(*step, Stage::Done).unwrap();
    }
    store.task_mut(research_goal).unwrap().stage = Stage::Done;
    (
        store,
        Fixture {
            research_goal,
            research_steps,
            research_runs,
            first_automation,
            first_period,
            first_steps,
            second_period,
            second_step: second_steps[0],
            other_period,
            other_second_period,
        },
    )
}

fn verify_structural_paths(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    handle: AnyWindowHandle,
    fixture: &Fixture,
) {
    // Real workspace-menu Settings action captures the caller page.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            // Make the Automations list genuinely scrollable so the Settings
            // return frame proves scroll restoration rather than a clamped 0.
            for index in 0..40u64 {
                let _ = a.store.create_automation_draft(
                    1,
                    AutomationShape::Research,
                    format!("Scroll fixture {index}"),
                    "Track the scroll fixture".into(),
                    "A short note".into(),
                    Schedule {
                        spec: ScheduleSpec::Daily {
                            hour: 8,
                            minute: 30,
                        },
                        timezone: "Asia/Shanghai".into(),
                    },
                    AutomationPolicy::default(),
                    Some(template()),
                    2_000 + index as i64,
                );
            }
            a.structural_navigation();
            a.navigate(crate::app::Scope::Automations, w, cx);
            cx.notify();
        });
    })
    .unwrap();
    let mut captured_settings_scroll = 0.0f32;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        assert!(
            app.read(cx).store.automations().len() >= 40,
            "scroll fixture must be visible"
        );
        let max = app.read(cx).automation_ui.scroll.max_offset();
        assert!(
            max.y > px(70.),
            "automation list rows must overflow the viewport; max_offset={max:?}"
        );
        app.update(cx, |a, _| {
            a.automation_ui.scroll.scroll_to_item(40);
        });
        w.render_frame(cx);
        assert!(
            app.read(cx).automation_ui.scroll.offset().y <= px(-70.),
            "automation list scroll offset must be set before Settings"
        );
        assert!(
            app.read(cx).automation_ui.scroll.top_item() >= 30,
            "last automation rows are reachable with real scrolling"
        );
        println!(
            "automation scroll fixture: max_offset={:?} top_item={}",
            max,
            app.read(cx).automation_ui.scroll.top_item()
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("workspace-menu", cx);
        w.render_frame(cx);
        assert!(
            app.read(cx).automation_ui.scroll.offset().y <= px(-70.),
            "opening the workspace menu must not reset caller scroll"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.within("popup-menu").click(3usize, cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let a = app.read(cx);
        assert!(a.settings_page(), "workspace menu opens Settings");
        let (links, current) = a.navigation_path();
        assert!(links.is_empty(), "Settings is not a Research child");
        assert_eq!(current, "Settings");
        assert!(
            w.try_find("navigation-back").is_some(),
            "workspace-menu Settings captures its caller"
        );
        assert_eq!(
            w.focused(cx),
            Some(app.read(cx).focus.clone()),
            "closing the workspace menu restores page focus for keyboard Back"
        );
        let captured_scroll = a
            .return_frames()
            .last()
            .and_then(|frame| {
                frame
                    .scroll
                    .get(crate::navigation_route::ScrollKey::PeriodBody)
            })
            .map(|(_, y)| y)
            .unwrap_or(0.0);
        assert!(
            captured_scroll <= -70.0,
            "Settings return frame captures the caller scroll"
        );
        captured_settings_scroll = captured_scroll;
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("cmd-[", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert_eq!(a.scope(), crate::app::Scope::Automations);
        let restored = a.automation_ui.scroll.offset().y.as_f32();
        assert!(
            (restored - captured_settings_scroll).abs() <= 1.0,
            "Cmd+[ restores the exact captured caller scroll: captured={captured_settings_scroll} restored={restored}"
        );
    })
    .unwrap();

    // The same action preserves a list filter.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.structural_navigation();
            a.navigate(crate::app::Scope::All, w, cx);
            a.set_query("Row".into());
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("workspace-menu", cx);
        w.render_frame(cx);
        w.within("popup-menu").click(3usize, cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert_eq!(a.scope(), crate::app::Scope::All);
        assert_eq!(a.query(), "Row", "Settings Back preserves the list filter");
    })
    .unwrap();

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.structural_navigation();
            a.navigate(crate::app::Scope::AutomationRuns, w, cx);
            let (links, current) = a.navigation_path();
            assert_eq!(
                links.first().map(|(_, title, _)| title.as_str()),
                Some("Automations"),
                "Automation results belong to Automations"
            );
            assert_eq!(current, "Automation results");

            a.open_occurrence_result(fixture.first_period, w, cx);
            assert_eq!(a.period_overlay(), Some(fixture.first_period));
            a.close_occurrence_result(w, cx);
            assert_eq!(a.scope(), crate::app::Scope::AutomationRuns);
            a.go_back(w, cx);
            assert_eq!(
                a.scope(),
                crate::app::Scope::Automations,
                "Automation results fall back to Automations, never Research"
            );
            a.open_task(fixture.first_steps[0], w, cx);
            cx.notify();
        });
    })
    .unwrap();
}

pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-navigation-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (store, fixture) = fixture();
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Light, None, cx);
    });
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.store = store.clone();
                a.open_task(fixture.first_steps[0], w, cx);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    exercise_completed_research(&mut cx, &app, handle, &fixture, "light");
    exercise(&mut cx, &app, handle, &fixture, "light");
    verify_structural_paths(&mut cx, &app, handle, &fixture);
    screenshot(&mut cx, handle, &root, "navigation-light.png");
    cx.update_window(handle, |_, w, cx| {
        Theme::change(ThemeMode::Dark, Some(w), cx);
        w.render_frame(cx);
    })
    .unwrap();
    exercise_completed_research(&mut cx, &app, handle, &fixture, "dark");
    exercise(&mut cx, &app, handle, &fixture, "dark");
    verify_structural_paths(&mut cx, &app, handle, &fixture);
    screenshot(&mut cx, handle, &root, "navigation-dark.png");
    println!("Sidebar navigation QA passed · {}", root.display());
}

fn exercise_completed_research(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    handle: AnyWindowHandle,
    fixture: &Fixture,
    theme: &str,
) {
    let goal = fixture.research_goal;
    let steps = &fixture.research_steps;
    let runs = &fixture.research_runs;
    let final_run = *runs.last().unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.structural_navigation();
            a.navigate(Scope::Project(1), w, cx);
            a.expanded_sidebar_projects = vec![1];
            a.collapsed_sidebar_goals.retain(|id| *id != goal);
            a.collapsed_sidebar_topics.clear();
            a.sidebar_scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        });
        w.render_frame(cx);
    }).unwrap();
    cx.run_until_parked();
    for (task, run) in std::iter::once((goal, final_run))
        .chain(steps.iter().copied().zip(runs.iter().copied()))
    {
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.click(format!("sidebar-task-{task}"), cx);
            w.render_frame(cx);
            let a = app.read(cx);
            assert_eq!(a.task_result_context(), Some(task),
                "{theme}: sidebar opens the completed task's result context");
            assert_eq!(a.selected_run(), Some(run),
                "{theme}: a goal opens its final report and a step opens its own report");
            assert!(w.find("research-result-scroll").visible(),
                "{theme}: completed sidebar targets show the report reader");
        }).unwrap();
    }
    for from_result in [false, true] {
        for (origin, control, target, run) in [
            (steps[0], "previous-task", goal, final_run),
            (steps[0], "next-task", steps[1], runs[1]),
            (steps[1], "previous-task", steps[0], runs[0]),
            (steps[1], "next-task", steps[2], runs[2]),
        ] {
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.open_task(origin, w, cx);
                    if from_result {
                        a.view_task_result(origin, cx);
                    }
                });
                w.render_frame(cx);
                if !from_result {
                    assert_eq!(app.read(cx).selected(), Some(origin),
                        "{theme}: regression starts on explicit task detail");
                }
                w.click(control, cx);
                w.render_frame(cx);
                let a = app.read(cx);
                assert_eq!(a.task_result_context(), Some(target),
                    "{theme}: {control} prefers completed results from detail and reader");
                assert_eq!(a.selected_run(), Some(run),
                    "{theme}: {control} opens the target's report, not the prior report");
                assert!(w.find("research-result-scroll").visible());
            }).unwrap();
        }
    }
}

fn exercise(
    cx: &mut HeadlessAppContext,
    app: &Entity<TaskApp>,
    handle: AnyWindowHandle,
    fixture: &Fixture,
    theme: &str,
) {
    let first = fixture.first_steps[0];
    let middle = fixture.first_steps[1];
    let last = fixture.first_steps[2];
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            // This case tests the structural-fallback API on purpose: no
            // user drill-down origin is recorded before opening the step.
            a.structural_navigation();
            a.open_task(first, w, cx)
        });
        w.render_frame(cx);
        assert!(
            w.find("period-overview").visible(),
            "{theme}: automation step detail needs a separate overview action"
        );
        assert_eq!(
            w.find("previous-task").disabled(),
            Some(true),
            "{theme}: the first step must not wrap into another period"
        );
        assert_ne!(
            w.find("next-task").disabled(),
            Some(true),
            "{theme}: the middle step must offer the next step"
        );

        w.click("next-task", cx);
        assert_eq!(
            app.read(cx).selected(),
            Some(middle),
            "{theme}: next stays inside the period"
        );
        w.render_frame(cx);
        w.click("next-task", cx);
        assert_eq!(
            app.read(cx).selected(),
            Some(last),
            "{theme}: the last step remains inside the period"
        );
        w.render_frame(cx);
        assert_eq!(
            w.find("next-task").disabled(),
            Some(true),
            "{theme}: the last step must not wrap into another period"
        );
        w.click("previous-task", cx);
        assert_eq!(
            app.read(cx).selected(),
            Some(middle),
            "{theme}: previous returns to the preceding period step"
        );

        // The separate Overview action opens this exact period, not a step.
        w.click("period-overview", cx);
        let (overview_owner, overview_dialog) = {
            let a = app.read(cx);
            (a.automation_ui.selected, a.period_overlay())
        };
        assert_eq!(
            overview_owner,
            Some(fixture.first_automation),
            "{theme}: Overview keeps the owning automation"
        );
        assert_eq!(
            overview_dialog,
            Some(fixture.first_period),
            "{theme}: Overview opens the current period"
        );
        app.update(cx, |a, cx| a.close_occurrence_result(w, cx));

        // Back from a step restores the same period dialog.
        app.update(cx, |a, cx| {
            a.structural_navigation();
            a.open_task(last, w, cx);
        });
        w.render_frame(cx);
        w.click("navigation-back", cx);
        let (scope, automation, dialog) = {
            let a = app.read(cx);
            (a.scope(), a.automation_ui.selected, a.period_overlay())
        };
        assert_eq!(scope, Scope::Automations, "{theme}: Back opens the owning automation");
        assert_eq!(automation, Some(fixture.first_automation));
        assert_eq!(
            dialog,
            Some(fixture.first_period),
            "{theme}: Back must restore the same period"
        );
        app.update(cx, |a, cx| a.close_occurrence_result(w, cx));

        // Reports keep the same period-scoped neighbor controls.
        app.update(cx, |a, cx| a.open_sidebar_task(middle, w, cx));
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).task_result_context(),
            Some(middle),
            "{theme}: completed steps open their report"
        );
        assert!(w.find("previous-task").visible());
        assert!(w.find("next-task").visible());
        assert!(w.find("period-overview").visible());
        w.click("next-task", cx);
        assert_eq!(
            app.read(cx).task_result_context(),
            Some(last),
            "{theme}: report navigation stays inside the period"
        );
        assert_eq!(app.read(cx).scope(), Scope::Runs);

        // Recently opened entries name the automation and period.
        let labels: Vec<String> = {
            let a = app.read(cx);
            a.tabs
                .iter()
                .map(|id| a.store.task_navigation_label(*id))
                .collect()
        };
        assert!(
            labels.iter().any(|label| {
                label.contains("First automation")
                    && label.contains(&format!("Period {}", fixture.first_period))
            }),
            "{theme}: recently opened labels lose automation ownership: {labels:?}"
        );
        let current_period_steps: std::collections::HashSet<_> = app
            .read(cx)
            .store
            .automation_period_steps(fixture.first_period)
            .into_iter()
            .collect();
        let foreign_steps: std::collections::HashSet<_> = app
            .read(cx)
            .store
            .automation_period_steps(fixture.second_period)
            .into_iter()
            .chain(
                app.read(cx)
                    .store
                    .automation_period_steps(fixture.other_period),
            )
            .chain(
                app.read(cx)
                    .store
                    .automation_period_steps(fixture.other_second_period),
            )
            .collect();
        assert!(
            current_period_steps.contains(&last),
            "{theme}: the viewed step belongs to the expected period"
        );
        assert!(
            !foreign_steps.contains(&last),
            "{theme}: navigation reached another automation period"
        );

    })
    .unwrap();

    // Activate a real recent entry from another period: the destination is
    // the selected task, and its owner remains the second period.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.open_task(middle, w, cx);
            a.open_task(fixture.second_step, w, cx);
            a.open_task(first, w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        cx.update_window(handle, |_, _, cx| {
            app.read(cx).tabs.iter().any(|id| *id == fixture.second_step)
        })
        .unwrap(),
        "{theme}: the foreign period step remains in recently opened"
    );
    open_recent_menu(cx, handle);
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("recent-task-{}", fixture.second_step), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (selected, owner) = cx
        .update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            let a = app.read(cx);
            (
                a.selected(),
                a.selected().and_then(|id| a.store.automation_period_for_task(id)),
            )
        })
        .unwrap();
    assert_eq!(
        selected,
        Some(fixture.second_step),
        "{theme}: the recent entry opens its named task"
    );
    assert_eq!(
        owner,
        Some((fixture.first_automation, fixture.second_period)),
        "{theme}: the recent entry keeps its automation and period ownership"
    );

    // Activate a current-period entry and verify the same context.
    assert!(
        cx.update_window(handle, |_, _, cx| {
            app.read(cx).tabs.iter().any(|id| *id == first)
        })
        .unwrap(),
        "{theme}: the current period step remains in recently opened"
    );
    open_recent_menu(cx, handle);
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("recent-task-{first}"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let (selected, owner) = cx
        .update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            let a = app.read(cx);
            (
                a.selected(),
                a.selected().and_then(|id| a.store.automation_period_for_task(id)),
            )
        })
        .unwrap();
    assert_eq!(
        selected,
        Some(first),
        "{theme}: the current recent entry opens its named task"
    );
    assert_eq!(
        owner,
        Some((fixture.first_automation, fixture.first_period)),
        "{theme}: the current entry keeps its automation and period ownership"
    );

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            // This case tests the structural-fallback API on purpose: no
            // user drill-down origin is recorded before opening the step.
            a.structural_navigation();
            a.open_task(first, w, cx)
        });
        w.render_frame(cx);
    })
    .unwrap();
}

fn open_recent_menu(cx: &mut HeadlessAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, w, cx| w.click("open-tasks-menu", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx))
        .unwrap();
}

fn screenshot(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    directory: &Path,
    name: &str,
) {
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(30));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}
