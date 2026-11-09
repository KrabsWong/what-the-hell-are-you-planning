//! Focused offline acceptance for the T08 global Search overlay.
//!
//! No model, browser or network call. Every case uses an isolated store under
//! `.local/global-search-qa/<pid>` and a real headless window at 1440x920 and
//! 1000x680 in both themes.

use crate::{
    app::{AllStatus, Layout, Scope, TaskApp},
    appearance::Appearance,
    automation::{
        AutomationDefinition, AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy,
        AutomationShape, AutomationTemplate, DefinitionStatus, OccurrenceStatus, TimeWindow,
        TriggerSource,
    },
    automation_schedule::{Schedule, ScheduleSpec},
    domain::{Stage, Store},
    persistence::Storage,
    research::{ResearchKind, ResearchTask},
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};

const PLAN: &str = r#"{"tasks":[{"title":"Collect original announcements","instructions":"Save sources.","kind":"collect","depends_on":[]},{"title":"Analyze the evidence","instructions":"Compare the evidence.","kind":"analyze","depends_on":[0]},{"title":"Write the final report","instructions":"Write a cited report.","kind":"advise","depends_on":[1]}]}"#;

struct Fixture {
    store: Store,
    topic: u64,
    goal: u64,
    step: u64,
    run: u64,
    automation: u64,
    period: u64,
}

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/global-search-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let label = format!(
                "{}-{}",
                match mode {
                    Appearance::Light => "light",
                    Appearance::Dark => "dark",
                    Appearance::System => "system",
                },
                width as u32
            );
            let directory = root.join(&label);
            std::fs::create_dir_all(&directory).unwrap();
            let fixture = fixture(&directory, mode);
            let path = directory.join("workspace.sqlite3");
            Storage::new(path.clone()).save(&fixture.store).unwrap();

            research_list_origin(&fixture, &path, &directory, &label, width, height);
            research_scope_origin(
                &fixture,
                &path,
                &directory,
                &label,
                width,
                height,
                Scope::Project(1),
                "project",
            );
            research_scope_origin(
                &fixture,
                &path,
                &directory,
                &label,
                width,
                height,
                Scope::Topic(fixture.topic),
                "topic",
            );
            research_detail_origin(&fixture, &path, &directory, &label, width, height);
            research_result_origin(&fixture, &path, &directory, &label, width, height);
            automation_list_origin(&fixture, &path, &directory, &label, width, height);
            automation_results_origin(&fixture, &path, &directory, &label, width, height);
            automation_modal_and_period(&fixture, &path, &directory, &label, width, height);
            settings_origin(&fixture, &path, &directory, &label, width, height);
            dirty_task_form(&fixture, &path, &directory, &label, width, height);
            dirty_automation_form(&fixture, &path, &directory, &label, width, height);
            clean_automation_form(&fixture, &path, &directory, &label, width, height);
            empty_and_deleted_targets(&fixture, &path, &directory, &label, width, height);
        }
    }
    println!("Global search QA passed · {}", root.display());
}

fn fixture(directory: &Path, mode: Appearance) -> Fixture {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name = "Semiconductor desk".into();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/global-search".into());
    store.research_config_mut().models =
        ["qa/plan", "qa/collect", "qa/analyze", "qa/advise"].map(String::from);
    store.set_appearance(mode);
    let topic = store.create_topic(1, "Memory supply").unwrap();
    let goal = store
        .create_research_goal_in_topic(
            "Semiconductor supply outlook".into(),
            "Compare DRAM, NAND and HBM demand.".into(),
            1,
            Some(topic),
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    let step = steps[0];
    let mut run = store.prepare_run(step).unwrap();
    run.status = "Succeeded".into();
    run.output = "## Saved report\n\nThe isolated fixture saved a report.".into();
    run.ended_at = Some(crate::opencode::timestamp());
    run.exit_code = Some(0);
    let run_id = run.id;
    store.push_run(run);

    let automation = automation_definition(&mut store, directory);
    let (period, _automation_goal) = automation_period(&mut store, automation);
    Fixture {
        store,
        topic,
        goal,
        step,
        run: run_id,
        automation,
        period,
    }
}

fn automation_definition(store: &mut Store, _directory: &Path) -> u64 {
    let id = 7;
    store.push_automation(AutomationDefinition {
        id,
        version: 1,
        project_id: 1,
        shape: AutomationShape::Quick,
        name: "Fuel price watch".into(),
        objective: "Track the public fuel price each period.".into(),
        expected_result: "A short cited answer with the as-of time.".into(),
        schedule: Schedule {
            spec: ScheduleSpec::Daily { hour: 9, minute: 15 },
            timezone: "UTC".into(),
        },
        status: DefinitionStatus::Enabled,
        template: Some(AutomationTemplate {
            goals: vec![],
            window_rule: Default::default(),
        }),
        plan_binding: None,
        approval: None,
        policy: AutomationPolicy::default(),
        cursor: None,
        anchor: None,
        consecutive_failures: 0,
        attention_reason: None,
        created_at: 0,
        updated_at: 0,
    });
    id
}

fn automation_period(store: &mut Store, automation: u64) -> (u64, u64) {
    let goal = store
        .create_research_goal(
            "Quick fuel price answer".into(),
            "Answer with the current value.".into(),
            1,
        )
        .unwrap();
    store.task_mut(goal).unwrap().automation_id = Some(automation);
    let task = store
        .create_task("Answer the recurring fuel question", 1, Stage::Planned, Some(goal))
        .unwrap();
    let task_ref = store.task_mut(task).unwrap();
    task_ref.automation_id = Some(automation);
    task_ref.research = Some(ResearchTask {
        requires_approval: false,
        kind: ResearchKind::Advise,
        goal_id: goal,
        depends_on: vec![],
        space_name: "qa".into(),
    });
    let mut run = store.prepare_run(task).unwrap();
    run.automation_occurrence_id = Some(41);
    run.status = "Succeeded".into();
    run.output = "The isolated saved answer.".into();
    run.ended_at = Some(crate::opencode::timestamp());
    run.exit_code = Some(0);
    let run_id = run.id;
    store.push_run(run);
    store.set_stage(task, Stage::Done).unwrap();
    store.push_occurrence(AutomationOccurrence {
        id: 41,
        automation_id: automation,
        definition_version: 1,
        source: TriggerSource::Scheduled,
        trigger_key: "scheduled:41".into(),
        scheduled_for: Some(1_000),
        requested_at: None,
        actual_started_at: Some(1_001),
        timezone: "UTC".into(),
        reference_time: 1_000,
        data_window: TimeWindow::unresolved("QA fixture"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: "Track the public fuel price each period.".into(),
            expected_result: "A short cited answer with the as-of time.".into(),
            shape: Some(AutomationShape::Quick),
            template: AutomationTemplate {
                goals: vec![],
                window_rule: Default::default(),
            },
            models: Default::default(),
            agents: Default::default(),
            materials: vec![],
            policy: AutomationPolicy::default(),
            approval_version: 1,
            approval_signature: "qa".into(),
        },
        status: OccurrenceStatus::Succeeded,
        reason: None,
        research_goal_id: Some(goal),
        research_goal_ids: vec![goal],
        research_topic_id: None,
        run_ids: vec![run_id],
        final_run_id: Some(run_id),
        unread: true,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: 999,
        deadline_at: None,
        retry_requested_at: None,
    });
    (41, goal)
}

fn with_app<F>(
    store: &Store,
    path: &Path,
    width: f32,
    height: f32,
    configure: F,
) -> (HeadlessAppContext, AnyWindowHandle, Entity<TaskApp>)
where
    F: FnOnce(&mut TaskApp, &mut Window, &mut Context<TaskApp>),
{
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let appearance = store.appearance();
    let mut app = None;
    let handle = cx
        .open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                a.store = store.clone();
                a.set_appearance(appearance, w, cx);
                configure(a, w, cx);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap()
        .into();
    (cx, handle, app.unwrap())
}

/// The overlay focuses its query field on open. A pointer click on the field
/// makes the following text input deterministic even when the opener was a
/// footer button that the platform may keep as the pointer target.
fn focus_search_input(w: &mut Window, cx: &mut App) {
    w.render_frame(cx);
    w.click("global-search-input", cx);
    w.render_frame(cx);
    assert_eq!(w.find("global-search-input").focused(), Some(true));
}

fn screenshot(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    directory: &Path,
    name: &str,
) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}

fn research_list_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::All);
        a.fixture_select_task(None);
        a.layout = Layout::Board;
        a.all_status = AllStatus::Open;
    });
    let mut opener = None;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.try_find("global-search-panel").is_none());
        opener = w.focused(cx);
        w.click("footer-search", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::All);
        assert_eq!(app.read(cx).layout, Layout::Board);
        assert_eq!(app.read(cx).all_status, AllStatus::Open);
        assert_eq!(w.find("global-search-input").focused(), Some(true));
        assert_eq!(
            w.find("global-search-empty-title").label(),
            Some("Search across Research and Automations")
        );
        w.input("semiconductor", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("list-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let expected = format!("Research · {}", fixture.store.task(fixture.goal).unwrap().title);
        let found = w.try_find(format!("global-search-result-research-{}", fixture.goal));
        assert_eq!(
            found.as_ref().and_then(|row| row.label()),
            Some(expected.as_str())
        );
        let before = app.read(cx).global_search_ui.selected;
        w.press("down", cx);
        w.render_frame(cx);
        let next = app.read(cx).global_search_ui.selected;
        assert_ne!(next, before, "Down must move the global-search selection");
        assert_eq!(
            w.find(format!("global-search-result-research-{}", fixture.goal))
                .selected(),
            Some(false)
        );
        w.press("up", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).global_search_ui.selected, before);
        assert_eq!(
            w.find(format!("global-search-result-research-{}", fixture.goal))
                .selected(),
            Some(true)
        );
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::All);
        assert_eq!(app.read(cx).layout, Layout::Board);
        assert_eq!(app.read(cx).all_status, AllStatus::Open);
        assert_eq!(w.focused(cx), opener);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

/// Table-driven Project/Topic caller coverage: both required non-All Research
/// routes must keep their scope, list filter, layout and opener focus through a
/// typed global query for the footer command and ⌘K alike.
fn research_scope_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
    scope: Scope,
    route_label: &str,
) {
    for (entry_label, use_footer) in [("footer", true), ("cmdk", false)] {
        let initial_scope = scope.clone();
        let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
            a.set_scope_identity(initial_scope.clone());
            a.fixture_select_task(None);
            a.layout = Layout::Board;
            a.all_status = AllStatus::Open;
            a.search
                .update(cx, |input, cx| input.set_value("supply", w, cx));
            a.set_query("supply".into());
            cx.notify();
        });
        let mut opener = None;
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            opener = w.focused(cx);
            assert_eq!(app.read(cx).scope(), scope);
            if use_footer {
                w.click("footer-search", cx);
            } else {
                w.press("cmd-k", cx);
            }
            w.render_frame(cx);
            focus_search_input(w, cx);
            assert!(app.read(cx).global_search_ui.open);
            assert_eq!(app.read(cx).scope(), scope);
            assert_eq!(app.read(cx).layout, Layout::Board);
            assert_eq!(app.read(cx).all_status, AllStatus::Open);
            assert_eq!(app.read(cx).query(), "supply");
            assert_eq!(
                app.read(cx).search.read(cx).value().to_string(),
                "supply"
            );
            w.input("Semiconductor", cx);
        })
        .unwrap();
        cx.run_until_parked();
        screenshot(
            &mut cx,
            handle,
            directory,
            &format!("{route_label}-{entry_label}-{label}.png"),
        );
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(w
                .try_find(format!(
                    "global-search-result-research-{}",
                    fixture.goal
                ))
                .is_some());
            w.press("escape", cx);
            w.render_frame(cx);
            assert!(!app.read(cx).global_search_ui.open);
            assert_eq!(app.read(cx).scope(), scope);
            assert_eq!(app.read(cx).layout, Layout::Board);
            assert_eq!(app.read(cx).all_status, AllStatus::Open);
            assert_eq!(app.read(cx).query(), "supply");
            assert_eq!(
                app.read(cx).search.read(cx).value().to_string(),
                "supply"
            );
            assert_eq!(w.focused(cx), opener);
        })
        .unwrap();
        cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    }
}

fn research_detail_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.open_task(fixture.goal, w, cx);
        a.detail_scroll
            .set_offset(point(px(0.), px(-48.)));
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let scroll = app.read(cx).detail_scroll.offset();
        let opener = w.focused(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).selected(), Some(fixture.goal));
        assert_eq!(w.find("global-search-input").focused(), Some(true));
        // Closing restores the opener route, selection, scroll and focus.
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).selected(), Some(fixture.goal));
        assert_eq!(app.read(cx).detail_scroll.offset(), scroll);
        assert_eq!(w.focused(cx), opener);
        // A chosen research title opens its owning project/topic path.
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Collect original announcements", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("detail-selection-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).selected(), Some(fixture.step));
        let (crumbs, _) = app.read(cx).navigation_path();
        assert!(crumbs
            .iter()
            .any(|(_, title, _)| title.as_str() == "Semiconductor desk"));
        let topic_name = fixture
            .store
            .topics()
            .iter()
            .find(|topic| topic.id == fixture.topic)
            .map(|topic| topic.display_name())
            .unwrap();
        assert!(crumbs
            .iter()
            .any(|(_, title, _)| title.as_str() == topic_name.as_str()));
        assert_eq!(app.read(cx).detail_scroll.offset(), point(px(0.), px(0.)));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn research_result_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, _, _| {
        a.set_scope_identity(Scope::Runs);
        a.set_result_selection(Some(fixture.step), a.selected_run());
        a.note_selected_run(Some(fixture.run));
        a.show_run_result = true;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let opener = w.focused(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::Runs);
        assert_eq!(app.read(cx).selected_run(), Some(fixture.run));
        w.press("escape", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Runs);
        assert_eq!(app.read(cx).task_result_context(), Some(fixture.step));
        assert_eq!(app.read(cx).selected_run(), Some(fixture.run));
        assert_eq!(w.focused(cx), opener);
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("result-origin-{label}.png"),
    );
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn automation_list_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.navigate(Scope::Automations, w, cx);
        a.automation_ui.result_filter = Some(fixture.automation);
        a.automation_ui.history_limit = 20;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Answer the recurring fuel question", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("global-search-domain-research", cx);
        w.render_frame(cx);
        assert_eq!(
            w.find("global-search-empty-title").label(),
            Some("No matches"),
            "automation-owned steps must not appear as ordinary Research results"
        );
        w.click("global-search-domain-all", cx);
        w.press("cmd-a", cx);
        w.input("Fuel price watch", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w
            .try_find(format!(
                "global-search-result-automation-{}",
                fixture.automation
            ))
            .is_some());
        w.click("global-search-domain-automations", cx);
        w.render_frame(cx);
        assert!(w
            .try_find(format!(
                "global-search-result-research-{}",
                fixture.goal
            ))
            .is_none());
        let expected = format!("Automation · Fuel price watch");
        assert_eq!(
            w.find(format!(
                "global-search-result-automation-{}",
                fixture.automation
            ))
            .label(),
            Some(expected.as_str())
        );
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("automation-list-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).automation_ui.selected, Some(fixture.automation));
        let (crumbs, current) = app.read(cx).navigation_path();
        assert_eq!(current, "Fuel price watch");
        assert_eq!(
            crumbs.first().map(|(_, title, _)| title.as_str()),
            Some("Automations")
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.go_back(w, cx));
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).automation_ui.selected, None);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn automation_results_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.navigate(Scope::AutomationRuns, w, cx);
        a.automation_ui.result_filter = Some(fixture.automation);
        a.automation_ui.history_limit = 20;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let opener = w.focused(cx);
        // F08: this route used to keep its content while focusing an invisible
        // Research input. Footer Search must open the visible global overlay.
        w.click("footer-search", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert_eq!(w.find("global-search-input").focused(), Some(true));
        assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        assert_eq!(w.focused(cx), opener);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Fuel price watch", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        // Pointer selection is the same command as Enter.
        w.click(
            format!(
                "global-search-result-automation-{}",
                fixture.automation
            ),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("automation-results-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).automation_ui.selected, Some(fixture.automation));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn automation_modal_and_period(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.open_automation(fixture.automation, w, cx);
        a.open_automation_settings(w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).settings_overlay());
        let opener = w.focused(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert!(app.read(cx).settings_overlay());
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert!(app.read(cx).settings_overlay());
        assert_eq!(w.focused(cx), opener);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("period 41", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("period-selection-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert!(!app.read(cx).settings_overlay());
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).automation_ui.selected, Some(fixture.automation));
        assert_eq!(app.read(cx).period_overlay(), Some(fixture.period));
        let (crumbs, current) = app.read(cx).navigation_path();
        assert_eq!(current, "Fuel price watch");
        assert_eq!(
            crumbs.first().map(|(_, title, _)| title.as_str()),
            Some("Automations")
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).period_overlay(), None);
        assert_eq!(app.read(cx).scope(), Scope::Automations);
        assert_eq!(app.read(cx).automation_ui.selected, Some(fixture.automation));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn settings_origin(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.navigate(Scope::Settings, w, cx);
        a.set_settings_tab(crate::settings_views::SettingsTab::Models);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let opener = w.focused(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::Settings);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(app.read(cx).scope(), Scope::Settings);
        assert!(
            app.read(cx).settings_tab() == crate::settings_views::SettingsTab::Models,
            "Settings origin must keep the selected tab"
        );
        assert_eq!(w.focused(cx), opener);
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("settings-origin-{label}.png"),
    );
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn dirty_task_form(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.begin_draft(fixture.goal, w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.input(" revised", cx);
        w.render_frame(cx);
        let before = app
            .read(cx)
            .draft
            .as_ref()
            .unwrap()
            .title
            .read(cx)
            .value()
            .to_string();
        assert!(before.contains("revised"));
        let opener = w.focused(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        assert!(app.read(cx).draft.is_some());
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_some());
        assert_eq!(
            app.read(cx)
                .draft
                .as_ref()
                .unwrap()
                .title
                .read(cx)
                .value()
                .to_string(),
            before
        );
        assert_eq!(w.find("draft-title").focused(), Some(true));
        assert_eq!(w.focused(cx), opener);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Collect original announcements", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w
            .try_find(format!(
                "global-search-result-research-{}",
                fixture.step
            ))
            .is_some());
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("dirty-task-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        assert!(app.read(cx).draft.is_some());
        assert!(app.read(cx).navigation.pending_exit.is_some());
        assert!(w.try_find("dirty-exit-stay").is_some());
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open, "Stay keeps Search");
        assert!(app.read(cx).draft.is_some(), "Stay keeps the task draft");
        assert!(app.read(cx).navigation.pending_exit.is_none());
    })
    .unwrap();
    // Focus/blur subscriptions from closing the decision settle in their own
    // turn, so the next Enter reaches the Search field exactly as it does in the
    // native app.
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(30));
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        // The native flow presses Return again with no extra pointer action.
        w.press("enter", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-discard").is_some());
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open, "Discard activates the target");
        assert!(app.read(cx).draft.is_none());
        assert_eq!(app.read(cx).selected(), Some(fixture.step));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn dirty_automation_form(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.start_automation_create(w, cx);
        a.automation_ui
            .name
            .update(cx, |input, cx| input.set_value("My queued automation", w, cx));
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Fuel price watch", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("dirty-automation-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        assert!(app.read(cx).automation_ui.creating);
        assert_eq!(
            app.read(cx)
                .automation_ui
                .name
                .read(cx)
                .value()
                .to_string(),
            "My queued automation"
        );
        assert!(app.read(cx).navigation.pending_exit.is_some());
        assert!(w.try_find("dirty-exit-stay").is_some());
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        assert!(app.read(cx).automation_ui.creating);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("enter", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert!(!app.read(cx).automation_ui.creating);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn clean_automation_form(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.start_automation_create(w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Fuel price watch", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("clean-form-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert!(!app.read(cx).automation_ui.creating);
        assert_eq!(app.read(cx).automation_ui.selected, Some(fixture.automation));
        assert_eq!(app.read(cx).scope(), Scope::Automations);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}

fn empty_and_deleted_targets(
    fixture: &Fixture,
    path: &Path,
    directory: &Path,
    label: &str,
    width: f32,
    height: f32,
) {
    let (mut cx, handle, app) = with_app(&fixture.store, path, width, height, |a, w, cx| {
        a.open_task(fixture.goal, w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("definitely-no-such-result", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(w.find("global-search-empty-title").label(), Some("No matches"));
        assert!(app.read(cx).global_search_ui.hits.is_empty());
        w.press("escape", cx);
        w.render_frame(cx);

        // Deleted target: compute the hit, remove the only matching task while
        // the overlay stays open, then confirm it refuses to navigate.
        w.press("cmd-k", cx);
        w.render_frame(cx);
        focus_search_input(w, cx);
        w.input("Collect original announcements", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let doomed = fixture.step;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w
            .try_find(format!("global-search-result-research-{doomed}"))
            .is_some());
        app.update(cx, |a, _| {
            a.store.retain_tasks(|task| task.id != doomed);
        });
        w.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    screenshot(
        &mut cx,
        handle,
        directory,
        &format!("empty-deleted-{label}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        assert!(app
            .read(cx)
            .global_search_ui
            .message
            .as_deref()
            .is_some_and(|message| message.contains("no longer exists")));
        assert_eq!(app.read(cx).selected(), Some(fixture.goal));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
}
