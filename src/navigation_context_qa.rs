//! T10 S6: real-control return context, scroll and dirty exits.
//!
//! Runs at 1440x920 and 1000x680. It uses the actual list/board/topic rows,
//! period steps, footer/header commands and the shared Save/Discard/Stay dialog.

use crate::automation::{
    AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    OccurrenceStatus, TimeWindow, TriggerSource,
};
use crate::automation_schedule::{Schedule, ScheduleSpec};
use crate::domain::{Stage, Store};
use crate::app::{Layout, Scope, TaskApp};
use crate::persistence::Storage;
use gpui_kit::{component::{Theme, ThemeMode}, prelude::*, test::TestWindowExt, *};
use std::{path::Path, sync::Arc};

const PLAN: &str = r#"{"tasks":[{"title":"Evidence","instructions":"Collect evidence","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze evidence","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write report","kind":"advise","depends_on":[1]}]}"#;

struct Fixture {
    topic: u64,
    goal: u64,
    tasks: Vec<u64>,
    automation: u64,
    period: u64,
    period_steps: Vec<u64>,
}

fn store(root: &Path) -> Store {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into();
    store.set_default_model("fixture/navigation".into());
    store.research_config_mut().models =
        ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from);
    store
}

fn finish_steps(store: &mut Store, occurrence: u64) -> Vec<u64> {
    let steps = store.automation_period_steps(occurrence);
    for step in &steps {
        let mut run = store.prepare_run(*step).unwrap();
        run.automation_occurrence_id = Some(occurrence);
        run.status = "Succeeded".into();
        run.output = format!("Completed report for step {step}");
        store.push_run(run);
        store.set_stage(*step, Stage::Done).unwrap();
    }
    steps
}

fn add_extra_period(store: &mut Store, automation: u64, scheduled: i64) -> u64 {
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
        data_window: TimeWindow::unresolved("navigation context QA"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: definition.objective.clone(),
            expected_result: definition.expected_result.clone(),
            shape: Some(definition.shape),
            template: definition.template.clone().unwrap(),
            models: ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from),
            agents: Default::default(),
            materials: vec![],
            policy: definition.policy.clone(),
            approval_version: definition.version,
            approval_signature: "navigation context QA".into(),
        },
        status: OccurrenceStatus::Queued,
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

fn fixture(root: &Path) -> (Store, Fixture) {
    let mut store = store(root);
    store.push_project(crate::domain::Project {
        id: 2,
        name: "Second".into(),
        key: "P2".into(),
        directory: root.to_string_lossy().into(),
        research_brief: String::new(),
        kind: crate::domain::ProjectKind::User,
    });
    let topic = store.create_topic(1, "Topic").unwrap();
    let goal = store
        .create_research_goal_in_topic("Research goal".into(), "Scope".into(), 1, Some(topic))
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    for step in &steps {
        let mut run = store.prepare_run(*step).unwrap();
        run.status = "Succeeded".into();
        run.output = "Completed research report".into();
        store.push_run(run);
        store.set_stage(*step, Stage::Done).unwrap();
    }
    for index in 0..30 {
        store
            .create_task_in_topic(
                format!("Topic row {index:02}"),
                1,
                Stage::Backlog,
                None,
                Some(topic),
            )
            .unwrap();
    }
    for index in 0..30 {
        store
            .create_task(format!("Row {index:02}"), 1, Stage::Backlog, None)
            .unwrap();
    }
    let tasks = store
        .tasks()
        .iter()
        .filter(|task| task.parent_id.is_none() && task.id != goal)
        .map(|task| task.id)
        .collect::<Vec<_>>();
    let automation = store
        .create_automation_draft(
            1,
            AutomationShape::Research,
            "Automation".into(),
            "Track public filings".into(),
            "A cited note".into(),
            Schedule {
                spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                timezone: "Asia/Shanghai".into(),
            },
            AutomationPolicy::default(),
            Some(crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()),
            1_000,
        )
        .unwrap();
    let definition = store.automation(automation).unwrap().clone();
    let occurrence = AutomationOccurrence {
        id: store.next_occurrence_id().unwrap(),
        automation_id: automation,
        definition_version: definition.version,
        source: TriggerSource::Scheduled,
        trigger_key: crate::automation::scheduled_trigger_key(automation, 1_000),
        scheduled_for: Some(1_000),
        requested_at: None,
        actual_started_at: Some(1_060),
        timezone: definition.schedule.timezone.clone(),
        reference_time: 1_000,
        data_window: TimeWindow::unresolved("navigation context QA"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: definition.objective.clone(),
            expected_result: definition.expected_result.clone(),
            shape: Some(definition.shape),
            template: definition.template.clone().unwrap(),
            models: ["fixture/plan", "fixture/collect", "fixture/analyze", "fixture/advise"].map(String::from),
            agents: Default::default(),
            materials: vec![],
            policy: definition.policy.clone(),
            approval_version: definition.version,
            approval_signature: "navigation context QA".into(),
        },
        status: OccurrenceStatus::Queued,
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
        created_at: 1_000,
        deadline_at: None,
        retry_requested_at: None,
    };
    let period = store.materialize_occurrence(occurrence).unwrap().id;
    let _ = finish_steps(&mut store, period);
    let mut newest_period = period;
    for index in 0..40 {
        newest_period = add_extra_period(&mut store, automation, 10_000 + index * 100);
    }
    let period = newest_period;
    let period_steps = finish_steps(&mut store, period);
    let _top = add_extra_period(&mut store, automation, 20_000);
    (
        store,
        Fixture {
            topic,
            goal,
            tasks,
            automation,
            period,
            period_steps,
        },
    )
}

fn list_offset(app: &crate::app::TaskApp) -> f32 {
    app.navigation.task_list_scroll.0.borrow().base_handle.offset().y.as_f32()
}

fn board_offset(app: &crate::app::TaskApp) -> f32 {
    app.navigation.board_scroll.offset().x.as_f32()
}

fn scroll_over(w: &mut Window, id: ElementId, dy: f32, cx: &mut App) {
    scroll_xy_over(w, id, 0.0, dy, cx)
}

fn scroll_xy_over(w: &mut Window, id: ElementId, dx: f32, dy: f32, cx: &mut App) {
    w.scroll(id, ScrollDelta::Pixels(point(px(dx), px(dy))), cx);
}

/// Bring `target` inside the window using the real scroll container, with
/// bounded increments. Returns false when the row is not reachable.
fn scroll_into_view(w: &mut Window, cx: &mut App, container: &str, target: &str, dy: f32) -> bool {
    let target = ElementId::Name(target.into());
    for _ in 0..60 {
        w.render_frame(cx);
        let visible = w.try_find(target.clone()).is_some_and(|snapshot| {
            let bounds = snapshot.bounds();
            let viewport = w.viewport_size();
            snapshot.visible()
                && bounds.top() >= px(0.)
                && bounds.bottom() <= viewport.height
                && bounds.left() >= px(0.)
                && bounds.right() <= viewport.width
        });
        if visible {
            return true;
        }
        w.scroll(
            container.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(dy))),
            cx,
        );
    }
    false
}

fn open_task_editor(w: &mut Window, cx: &mut App, id: u64) {
    if w.try_find(format!("task-row-{id}")).is_none() {
        // The previous editor may have closed on top of the task detail page.
        w.click("navigation-back", cx);
        w.render_frame(cx);
    }
    assert!(
        w.try_find(format!("task-row-{id}")).is_some(),
        "the task row {id} must exist before opening its editor"
    );
    w.click(format!("task-row-{id}"), cx);
    w.render_frame(cx);
    w.click("task-actions-menu", cx);
    w.render_frame(cx);
    w.click(0usize, cx);
    w.render_frame(cx);
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

pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-navigation-context-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (store, fixture) = fixture(&root);
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| { init(cx); Theme::change(ThemeMode::Light, None, cx); });
    let mut app = None;
    let window = cx.open_window(size(px(1440.), px(920.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.attach_storage(Storage::new(path.clone()), w, cx);
            a.store = store.clone();
            a.navigate(Scope::All, w, cx);
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // 1. List filter + nonzero scroll survives a row drill-down.
    cx.update_window(handle, |_, w, cx| {
        w.click("task-search", cx);
        w.input("Row", cx);
        w.render_frame(cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        assert_eq!(app.read(cx).query, "Row", "typing must settle into the filter");
        let first = app.read(cx).visible_tasks()[0].id;
        scroll_over(w, ElementId::Name(format!("task-row-{first}").into()), -20.0, cx);
        w.render_frame(cx);
        assert!(list_offset(app.read(cx)) < -10.0, "precondition: list is scrolled");
        let id = app.read(cx).visible_tasks()[1].id;
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).selected(),
            Some(id),
            "row click did not open the task; pending={:?} origins={}",
            app.read(cx).navigation.pending_exit,
            app.read(cx).navigation.origins.len()
        );
        assert_eq!(
            app.read(cx).navigation.origins.len(),
            1,
            "clicking a row must record one origin"
        );
        if let Some(crate::navigation_context::Origin::ResearchList { query, .. }) =
            app.read(cx).navigation.origins.peek()
        {
            assert_eq!(query, "Row", "captured origin must include the filter");
        } else {
            panic!("row drill-down captured the wrong origin");
        }
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert!(
            app.read(cx).navigation.pending_exit.is_none(),
            "Back unexpectedly asked to save: {:?}",
            app.read(cx).navigation.pending_exit
        );
        assert_eq!(app.read(cx).query, "Row", "list query must survive Back");
        assert!(list_offset(app.read(cx)) < -10.0, "list scroll must be restored");
    }).unwrap();

    // 1b. Board horizontal + column scroll survives a card drill-down.
    cx.update_window(handle, |_, w, cx| {
        w.click("board-view", cx);
        w.render_frame(cx);
        let id = app.read(cx).visible_tasks().first().unwrap().id;
        let card = ElementId::Name(format!("card-{id}").into());
        scroll_xy_over(w, card.clone(), -120.0, 0.0, cx);
        scroll_xy_over(w, card, 0.0, -60.0, cx);
        w.render_frame(cx);
        assert!(board_offset(app.read(cx)) < -10.0, "precondition: board scrolled horizontally");
        assert!(
            app.read(cx).navigation.board_column(Stage::Backlog).offset().y < px(-10.),
            "precondition: board column scrolled"
        );
        w.click(format!("card-{id}"), cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).layout, Layout::Board);
        assert!(board_offset(app.read(cx)) < -10.0, "board horizontal offset restored");
        assert!(
            app.read(cx)
                .navigation
                .board_column(Stage::Backlog)
                .offset()
                .y
                < px(-10.),
            "board column offset restored"
        );
        assert_eq!(
            app.read(cx).navigation.board_column(Stage::Planned).offset().y,
            px(0.),
            "unscrolled columns return to zero"
        );
    }).unwrap();

    // 2. Topic list scroll survives opening a goal.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.layout = Layout::List;
            a.navigate(Scope::Topic(fixture.topic), w, cx);
        });
        w.render_frame(cx);
        let goal_row = ElementId::Name(format!("topic-goal-{}", fixture.goal).into());
        scroll_over(w, goal_row, -40.0, cx);
        w.render_frame(cx);
        assert!(
            app.read(cx).navigation.topic_scroll.offset().y < px(-10.),
            "precondition: topic list scrolled"
        );
        let row_id = app
            .read(cx)
            .store
            .tasks()
            .iter()
            .filter(|task| task.topic_id == Some(fixture.topic))
            .nth(4)
            .unwrap()
            .id;
        w.click(format!("topic-goal-{row_id}"), cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected(), Some(row_id));
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Topic(fixture.topic));
        assert!(app.read(cx).navigation.topic_scroll.offset().y < px(-10.));
    }).unwrap();

    // 3. Dirty task edit: Stay keeps text; Discard reverts; Save persists + leaves.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::All, w, cx));
        w.render_frame(cx);
        let id = app.read(cx).visible_tasks()[0].id;
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        w.click("task-actions-menu", cx);
        w.render_frame(cx);
        w.click(0usize, cx);
        w.render_frame(cx);
        w.click("draft-title", cx);
        w.input(" changed", cx);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-stay").is_some(), "dirty navigation must ask");
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_some(), "Stay keeps the draft");
        assert!(
            app.read(cx)
                .draft
                .as_ref()
                .is_some_and(|draft| draft.title.read(cx).value().contains(" changed")),
            "Stay keeps the typed input"
        );
        w.press("escape", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_none());
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::All);
        // Save path on the same cleanly reopened task.
        let id = app.read(cx).visible_tasks()[0].id;
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        w.click("task-actions-menu", cx);
        w.render_frame(cx);
        w.click(0usize, cx);
        w.render_frame(cx);
        w.click("draft-title", cx);
        w.input(" saved", cx);
        w.press("escape", cx);
        w.render_frame(cx);
        w.click("dirty-exit-save", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_none());
        assert!(
            app.read(cx)
                .store
                .task(id)
                .is_some_and(|task| task.title.contains(" saved")),
            "Save persists the edited title"
        );
    }).unwrap();

    // 4. Dirty automation wizard: Stay keeps the form, Discard leaves none.
    cx.update_window(handle, |_, w, cx| {
        w.click("all-automations", cx);
        w.render_frame(cx);
        w.click("new-automation", cx);
        w.render_frame(cx);
        w.click("automation-objective", cx);
        w.input("Check prices", cx);
        w.click("all", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-stay").is_some());
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating, "Stay keeps the wizard");
        assert!(
            app.read(cx)
                .automation_ui
                .form_draft(cx)
                .objective
                .contains("Check prices"),
            "Stay keeps the typed objective"
        );
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_some(),
            "Back must ask before dropping a dirty automation draft"
        );
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating, "Back + Stay keeps the wizard");
        w.click("location-automations", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_some(),
            "the breadcrumb must ask before dropping a dirty automation draft"
        );
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating);
        w.click("all", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_some(),
            "the sidebar must ask before dropping a dirty automation draft"
        );
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).automation_ui.creating);
        w.click("all", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-discard").is_some());
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).automation_ui.creating);
        assert_eq!(app.read(cx).scope(), Scope::All);
    }).unwrap();

    // 5. The planning composer is a modal surface: background mouse routes are
    // unavailable, while Cancel/Close and platform shortcuts raise the shared
    // decision. Stay keeps the text; Discard restores the origin project.
    cx.update_window(handle, |_, w, cx| {
        w.click("project-1", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Project(1));
        w.click("footer-new-research", cx);
        w.render_frame(cx);
        let before = app.read(cx).scope().clone();
        assert!(w.try_find("planning-destination").is_some());
        app.update(cx, |a, cx| {
            a.retarget_planning_destination(2, w, cx);
        });
        w.render_frame(cx);
        assert_eq!(app.read(cx).planning_project, Some(2), "destination changed");
        assert_eq!(app.read(cx).planning_destination.unwrap().project(), 2);
        w.click("project-plan-brief", cx);
        w.input("Dirty planning text", cx);
        let location = app.read(cx).scope().clone();
        for blocked in ["navigation-back", "all", "location-all", "open-tasks-menu"] {
            if w.try_find(blocked).is_none() {
                continue;
            }
            w.click(blocked, cx);
            w.render_frame(cx);
            assert!(
                app.read(cx).planning_destination.is_some(),
                "{blocked} behind the planning dialog must not drop the request"
            );
            assert!(
                app.read(cx)
                    .project_brief
                    .read(cx)
                    .value()
                    .contains("Dirty planning text"),
                "{blocked} behind the planning dialog must not clear the text"
            );
            assert!(
                app.read(cx).navigation.pending_exit.is_none(),
                "{blocked} behind the planning dialog must not dispatch a decision"
            );
            assert_eq!(
                app.read(cx).scope(),
                location,
                "{blocked} behind the planning dialog must not activate the page underneath"
            );
        }
        let mut check = |w: &mut Window, cx: &mut App, label: &str| {
            assert!(
                w.try_find("dirty-exit-stay").is_some(),
                "{label} must ask before dropping a dirty planning request"
            );
            w.click("dirty-exit-stay", cx);
            w.render_frame(cx);
            assert!(
                app.read(cx).planning_destination.is_some(),
                "{label} Stay must keep planning open"
            );
            assert!(
                app.read(cx)
                    .project_brief
                    .read(cx)
                    .value()
                    .contains("Dirty planning text"),
                "{label} Stay must keep the planning text"
            );
            assert!(app.read(cx).navigation.pending_exit.is_none());
        };
        w.press("cmd-[", cx);
        w.render_frame(cx);
        check(w, cx, "Command-[");
        w.press("cmd-n", cx);
        w.render_frame(cx);
        check(w, cx, "Command-N");
        w.click("planning-cancel", cx);
        w.render_frame(cx);
        check(w, cx, "Cancel");
        w.click("close-project-plan", cx);
        w.render_frame(cx);
        check(w, cx, "Close");
        // Escape on the open decision is Stay: dismiss and keep editing.
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-discard").is_some());
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(app.read(cx).planning_destination.is_some());
        assert!(app.read(cx).navigation.pending_exit.is_none());
        w.click("planning-cancel", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-discard").is_some());
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), before);
        assert!(app.read(cx).planning_destination.is_none());
        app.update(cx, |a, cx| {
            a.search.update(cx, |s, cx| s.set_value("", w, cx));
            cx.notify();
        });
        w.render_frame(cx);
    }).unwrap();

    // 6. Automation results filter + period -> step -> Back.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-results", cx);
        w.render_frame(cx);
        app.update(cx, |a, cx| {
            a.automation_ui.result_filter = Some(fixture.automation);
            a.automation_ui.history_limit = 60;
            cx.notify();
        });
        w.render_frame(cx);
        let target = ElementId::Name(format!("open-occurrence-{}", fixture.period).into());
        scroll_over(w, target.clone(), -40.0, cx);
        w.render_frame(cx);
        for _ in 0..30 {
            if w.try_find(target.clone())
                .is_some_and(|snapshot| snapshot.visible())
            {
                break;
            }
            let anchor = app
                .read(cx)
                .store
                .all_occurrences()
                .iter()
                .map(|occurrence| occurrence.id)
                .find(|id| {
                    w.try_find(format!("open-occurrence-{id}"))
                        .is_some_and(|snapshot| snapshot.visible())
                });
            let Some(anchor) = anchor else { break };
            scroll_over(
                w,
                ElementId::Name(format!("open-occurrence-{anchor}").into()),
                -120.0,
                cx,
            );
            w.render_frame(cx);
        }
        assert!(
            w.try_find(target).is_some_and(|snapshot| snapshot.visible()),
            "the target period must be scrolled into view"
        );
        let before_scroll = app.read(cx).automation_ui.scroll.offset().y;
        assert!(
            before_scroll < px(-10.),
            "precondition: automation results scrolled"
        );
        w.click(format!("open-occurrence-{}", fixture.period), cx);
        w.render_frame(cx);
        w.click(format!("automation-step-{}", fixture.period_steps[0]), cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        assert_eq!(app.read(cx).automation_ui.result_dialog, Some(fixture.period));
        assert_eq!(
            app.read(cx).automation_ui.scroll.offset().y,
            before_scroll,
            "automation results scroll restored exactly"
        );
        assert_eq!(app.read(cx).automation_ui.result_filter, Some(fixture.automation));
    }).unwrap();

    // 7. Save failure leaves the composer and retry saves exactly once.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::All, w, cx));
        w.render_frame(cx);
        w.click("footer-new-research", cx);
        w.render_frame(cx);
        w.click("project-plan-brief", cx);
        w.input("One saved request", cx);
        let before = app.read(cx).store.research_requests().len();
        app.update(cx, |a, _| a.storage = None);
        w.click("save-project-context", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.research_requests().len(), before);
        assert!(app.read(cx).planning_destination.is_some());
        app.update(cx, |a, _| a.storage = Some(Storage::new(path.clone())));
        w.click("save-project-context", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.research_requests().len(), before + 1);
    }).unwrap();

    // 8. Search from empty results returns to the same empty results page.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::Runs, w, cx));
        w.render_frame(cx);
        w.click("footer-search", cx);
        w.render_frame(cx);
        w.click("global-search-input", cx);
        w.input("Research goal", cx);
        w.render_frame(cx);
    }).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        let id = format!("global-search-result-research-{}", fixture.goal);
        assert!(
            w.try_find(id.clone()).is_some(),
            "the searched goal must be visible before activation"
        );
        w.click(id, cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Runs);
        assert!(app.read(cx).selected().is_none());
        assert!(app.read(cx).task_result_context().is_none());
    }).unwrap();

    // 9. Every task-edit field participates in the same dirty decision, and
    // Discard never persists it.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::All, w, cx));
        w.render_frame(cx);
        let id = app.read(cx).visible_tasks()[0].id;

        // TITLE is covered in section 3. DESCRIPTION/DUE/CYCLE are part of the
        // same pure baseline (draft_exit unit test); here the remaining exposed
        // controls get a real edit/Stay/Discard pass.

        // LABELS
        open_task_editor(w, cx, id);
        w.click("draft-labels", cx);
        w.input("qa-label", cx);
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-stay").is_some());
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(
            app.read(cx)
                .draft
                .as_ref()
                .is_some_and(|draft| draft.labels.read(cx).value().contains("qa-label")),
            "a label edit marks the draft dirty"
        );
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(app.read(cx).store.task(id).is_some_and(|task| task.labels.is_empty()));

        // STAGE
        open_task_editor(w, cx, id);
        w.click("draft-stage-todo", cx);
        w.render_frame(cx);
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_some(),
            "a stage edit marks the draft dirty"
        );
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).draft.as_ref().map(|draft| draft.stage),
            Some(Stage::Planned)
        );
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.task(id).unwrap().stage, Stage::Backlog);

        // PROJECT
        open_task_editor(w, cx, id);
        w.click("draft-project-2", cx);
        w.render_frame(cx);
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_some(),
            "a project edit marks the draft dirty"
        );
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).draft.as_ref().map(|draft| draft.project),
            Some(2)
        );
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.task(id).unwrap().project_id, 1);
    }).unwrap();

    // 10. The modal task editor blocks background mouse routes by design; they
    // must do nothing rather than drop or activate anything underneath. Visible
    // Cancel and the supported platform shortcuts raise the shared decision.
    cx.update_window(handle, |_, w, cx| {
        let id = app.read(cx).visible_tasks()[0].id;
        open_task_editor(w, cx, id);
        w.click("draft-title", cx);
        w.input(" routed", cx);
        w.render_frame(cx);
        let location = app.read(cx).scope().clone();
        let selected = app.read(cx).selected();
        for blocked in ["navigation-back", "all", "open-tasks-menu"] {
            w.click(blocked, cx);
            w.render_frame(cx);
            assert!(
                app.read(cx).draft.is_some(),
                "{blocked} behind the editor must not drop the draft"
            );
            assert!(
                app.read(cx).navigation.pending_exit.is_none(),
                "{blocked} behind the editor must not dispatch a decision"
            );
            assert_eq!(
                app.read(cx).scope(),
                location,
                "{blocked} behind the editor must not activate the page underneath"
            );
            assert_eq!(
                app.read(cx).selected(),
                selected,
                "{blocked} behind the editor must not change the selection"
            );
        }
        let mut check = |w: &mut Window, cx: &mut App, label: &str| {
            assert!(
                w.try_find("dirty-exit-stay").is_some(),
                "{label} must ask before dropping a dirty task edit"
            );
            w.click("dirty-exit-stay", cx);
            w.render_frame(cx);
            assert!(
                app.read(cx).draft.is_some(),
                "{label} Stay must keep the editor open"
            );
            assert!(
                app.read(cx).navigation.pending_exit.is_none(),
                "{label} Stay must dismiss the decision"
            );
        };
        w.press("cmd-[", cx);
        w.render_frame(cx);
        check(w, cx, "Command-[");
        w.press("cmd-n", cx);
        w.render_frame(cx);
        check(w, cx, "Command-N");
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        check(w, cx, "Cancel");
        // Escape on the open decision means Stay: it dismisses the dialog and
        // keeps the dirty editor and its text.
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-discard").is_some());
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_none(),
            "Escape on the decision must dismiss it"
        );
        assert!(app.read(cx).draft.is_some());
        assert!(app.read(cx).navigation.pending_exit.is_none());
        // The final visible Cancel + Discard is the only way this editor closes.
        w.click("cancel-draft", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_none());
    }).unwrap();

    // 11. A per-task comment and the Market research composer are preserved,
    // not treated as unsaved structural edits.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| a.navigate(Scope::All, w, cx));
        w.render_frame(cx);
        let id = app.read(cx).visible_tasks()[0].id;
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "task-activity-toggle");
        click_in(w, cx, "detail-scroll", "comment-input");
        w.input("preserved note", cx);
        w.click("all", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_none(),
            "a comment is preserved without a decision"
        );
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "task-activity-toggle");
        click_in(w, cx, "detail-scroll", "comment-input");
        assert!(
            app.read(cx).comment.read(cx).value().contains("preserved note"),
            "the per-task comment survives leaving and reopening the task"
        );

        app.update(cx, |a, cx| a.navigate(Scope::Research, w, cx));
        w.render_frame(cx);
        w.click("research-title", cx);
        w.input("Parked title", cx);
        w.click("research-brief", cx);
        w.input("Parked brief", cx);
        w.click("all", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("dirty-exit-stay").is_none(),
            "the Market composer parks without a decision"
        );
        app.update(cx, |a, cx| a.navigate(Scope::Research, w, cx));
        w.render_frame(cx);
        assert!(
            app.read(cx).research_title.read(cx).value().contains("Parked title")
                && app.read(cx).research_brief.read(cx).value().contains("Parked brief"),
            "the Market composer draft returns with its destination"
        );
        app.update(cx, |a, cx| a.navigate(Scope::All, w, cx));
        w.render_frame(cx);
    }).unwrap();

    screenshot(&mut cx, handle, &root, "navigation-context.png");
    for (width, height, theme) in [
        (1000.0, 680.0, ThemeMode::Dark),
        (1000.0, 680.0, ThemeMode::Light),
        (1440.0, 920.0, ThemeMode::Dark),
        (1440.0, 920.0, ThemeMode::Light),
    ] {
        size_theme_pass(
            &mut cx,
            &store,
            &path,
            &root,
            &fixture,
            width,
            height,
            theme,
        );
    }
    println!("Navigation context QA passed · {}", root.display());
}

fn size_theme_pass(
    cx: &mut HeadlessAppContext,
    store: &Store,
    path: &Path,
    root: &Path,
    fixture: &Fixture,
    width: f32,
    height: f32,
    theme: ThemeMode,
) {
    let mut app = None;
    let window = cx
        .open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                a.store = store.clone();
                Theme::change(theme, Some(w), cx);
                a.navigate(Scope::All, w, cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle: AnyWindowHandle = window.into();

    // Dirty dialog fits and keeps the draft; capture it for visual review.
    cx.update_window(handle, |_, w, cx| {
        let id = app.read(cx).visible_tasks()[0].id;
        w.click(format!("task-row-{id}"), cx);
        w.render_frame(cx);
        w.click("task-actions-menu", cx);
        w.render_frame(cx);
        w.click(0usize, cx);
        w.render_frame(cx);
        w.click("draft-title", cx);
        w.input(" narrow", cx);
        w.press("escape", cx);
        w.render_frame(cx);
        for id in ["dirty-exit-save", "dirty-exit-discard", "dirty-exit-stay"] {
            let bounds = w.find(id).bounds();
            assert!(bounds.right() <= px(width) && bounds.bottom() <= px(height));
            assert!(bounds.size.width > px(20.));
        }
    })
    .unwrap();
    screenshot(
        cx,
        handle,
        root,
        &format!("dirty-dialog-{width}x{height}-{theme:?}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("dirty-exit-stay", cx);
        w.render_frame(cx);
        assert!(app.read(cx).draft.is_some());
        w.press("escape", cx);
        w.render_frame(cx);
        w.click("dirty-exit-discard", cx);
        w.render_frame(cx);
    })
    .unwrap();

    // Period result from Automation results keeps its page and period.
    cx.update_window(handle, |_, w, cx| {
        w.click("automation-results", cx);
        w.render_frame(cx);
        assert!(
            scroll_into_view(
                w,
                cx,
                "automation-results-scroll",
                &format!("open-occurrence-{}", fixture.period),
                -160.0,
            ),
            "the period row must be reachable at {width}x{height}"
        );
        w.click(format!("open-occurrence-{}", fixture.period), cx);
        w.render_frame(cx);
        w.click(format!("automation-step-{}", fixture.period_steps[0]), cx);
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        assert_eq!(app.read(cx).automation_ui.result_dialog, Some(fixture.period));
    })
    .unwrap();

    // Creation target selector exists at this size and Cancel is exact.
    cx.update_window(handle, |_, w, cx| {
        w.click("close-automation-result", cx);
        w.render_frame(cx);
        w.click("footer-new-research", cx);
        w.render_frame(cx);
        let before = app.read(cx).scope().clone();
        let destination = w.find("planning-destination");
        assert!(destination.visible());
        assert!(destination.bounds().right() <= px(width));
        assert!(destination.bounds().bottom() <= px(height));
        app.update(cx, |a, cx| a.retarget_planning_destination(2, w, cx));
        w.render_frame(cx);
        assert_eq!(app.read(cx).planning_project, Some(2));
        w.click("planning-cancel", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), before);
        assert!(app.read(cx).planning_destination.is_none());
    })
    .unwrap();
    screenshot(
        cx,
        handle,
        root,
        &format!("navigation-context-{width}x{height}-{theme:?}.png"),
    );
    let _ = cx.update_window(handle, |_, w, _| w.remove_window());
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, root: &Path, name: &str) {
    cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
    if let Ok(image) = cx.capture_screenshot(handle) {
        let _ = image.save(root.join(name));
    }
}
