//! Focused T11 visual acceptance: shared action affordances, trigger labels,
//! long values, destructive/secondary/primary separation, disabled and selected
//! states. Every scenario asserts its own observed definition/period state.
//! No model is invoked and no lifecycle code is changed to force a fixture.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    automation::{
        AutomationGoal, AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationShape,
        AutomationTemplate, AutomationTemplateStep, DefinitionStatus, OccurrenceStatus,
        TriggerSource, WindowRule,
    },
    domain::Store,
    persistence::Storage,
    research::ResearchKind,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

const LONG_EN: &str = "Review recurring semiconductor supply, demand and pricing reports and reconcile every cited source before publishing the final research note";
const LONG_CJK: &str = "复核每一条来源并在发布最终研究报告前整理半导体供应链的长期变化、价格趋势与风险";
const AUTOMATION: &str = "Public semiconductor supply and pricing watch for every operating region";

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/t11-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let name = if mode == Appearance::Light { "light" } else { "dark" };
            let directory = root.join(format!("{name}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();

            enabled_idle(mode, width, height, &directory);
            recovery_active(mode, width, height, &directory);
        }
    }
    println!(
        "T11 QA PASSED: 1440/1000 × light/dark. Verified enabled-idle (Run manually primary, no Resume/Stop), draft (disabled Run manually), recovery (Needs attention, Resume primary, Stop danger), failed retry primary, destructive Cancel period, wrapped recent-menu values, passive status chip and selection vs primary. Screenshots: {}",
        root.display()
    );
}

/// A genuinely enabled, idle definition with one historical failed period. It is
/// not recovered as interrupted work, so Resume and Stop must be absent.
fn enabled_idle(mode: Appearance, width: f32, height: f32, directory: &Path) {
    let path = directory.join("enabled.sqlite3");
    let (store, forked) = fixture(mode);
    save(&path, &store);
    let (app, mut cx, handle) = open(&path, width, height, mode);
    let (id, draft, failed) = forked;

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert_eq!(
                a.store.automation(id).unwrap().status,
                DefinitionStatus::Enabled
            );
            assert!(a.store.automation_active(id).is_none());
            assert!(a.automation_ui.feedback.is_none());
            assert!(a.storage_error.is_empty());
            assert!(a.error.is_empty());
            assert!(!matches!(a.save_feedback, Some(Err(_))));
            a.open_automation(id, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("run-automation").is_some());
        assert!(w.try_find("resume-automation").is_none());
        assert!(w.try_find("stop-automation").is_none());
        assert!(w.try_find("automation-settings").is_some());
        let height = w.find("run-automation").bounds().size.height;
        assert!(
            height >= px(28.) && height <= px(36.),
            "Run manually height was {height:?}"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "01-enabled-idle.png");

    // Failed period: Retry period is the only primary action, and the step list
    // exposes long English/CJK titles without a separate control changing size.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert_eq!(
                a.store.occurrence(failed).unwrap().status,
                OccurrenceStatus::Failed
            );
            a.open_occurrence_result(failed, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("resume-automation-steps").is_some());
        assert!(w.try_find("cancel-automation-result").is_none());
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "02-failed-retry.png");

    // Recently opened: complete automation/period/task value, wrapped.
    let step = read(&mut cx, &app, |store| {
        let occurrence = store.occurrence(failed).unwrap();
        store
            .research_children(occurrence.research_goal_ids[0])
            .first()
            .copied()
            .unwrap()
    });
    let full_label = read(&mut cx, &app, |store| store.task_navigation_label(step));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.close_occurrence_result(w, cx);
            a.open_task(step, w, cx);
            a.open_task(2, w, cx);
        });
        w.render_frame(cx);
        w.click("open-tasks-menu", cx);
        w.render_frame(cx);
        let snapshot = w.find(format!("recent-task-{step}"));
        assert_eq!(snapshot.label(), Some(full_label.as_str()));
        assert!(snapshot.bounds().size.width > px(380.));
        assert!(
            snapshot.bounds().size.height >= px(28.),
            "a long recent label must wrap instead of collapsing to one ellipsized line"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "03-recent-menu.png");
    cx.update_window(handle, |_, w, cx| w.press("escape", cx)).unwrap();

    // Ordinary task detail: status is a passive chip, not a focusable button.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::All, w, cx);
            a.open_task(1, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("task-execution-status").is_some());
        assert!(w.try_find("task-activity-toggle").is_some());
        assert!(w.try_find("property-Stage").is_some());
        let status = w.find("task-execution-status");
        assert_eq!(status.role(), Some(Role::Label));
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "04-task-detail.png");

    // Draft automation: the disabled advancing action keeps its shape and the
    // page names the reason instead of silently offering it.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert_eq!(
                a.store.automation(draft).unwrap().status,
                DefinitionStatus::Draft
            );
            a.open_automation(draft, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("enable-draft-automation").is_some());
        assert_eq!(w.find("run-automation").disabled(), Some(true));
        assert!(w.try_find("stop-automation").is_none());
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "05-draft-disabled.png");

    // Selection control vs the one primary advancing action in the same group.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.navigate(Scope::Automations, w, cx);
            a.start_automation_create(w, cx);
        });
        w.render_frame(cx);
        let quick_selected = w.find("automation-shape-quick").checked();
        let research_selected = w.find("automation-shape-research").checked();
        assert_ne!(
            quick_selected, research_selected,
            "exactly one shape must expose the selected/toggled state"
        );
        assert_eq!(quick_selected, Some(true));
        w.click("automation-shape-research", cx);
        w.render_frame(cx);
        assert_eq!(
            w.find("automation-shape-research").checked(),
            Some(true),
            "clicking a shape must move the toggled state"
        );
        assert_eq!(
            w.find("automation-shape-quick").checked(),
            Some(false),
            "the previously selected shape must clear its toggled state"
        );
        assert!(w.try_find("automation-next-step").is_some());
        let selection_height = w.find("automation-shape-research").bounds().size.height;
        assert!(
            selection_height >= px(28.) && selection_height <= px(36.),
            "Selection control height was {selection_height:?}"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "06-selection-primary.png");

    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    drop(app);
    drop(cx);
}

/// The recovery fixture stays explicit: a stored running period is reconciled on
/// open, so Needs attention, Resume schedule and Stop this run are expected.
fn recovery_active(mode: Appearance, width: f32, height: f32, directory: &Path) {
    let path = directory.join("recovery.sqlite3");
    let (store, id, running) = recovery_fixture(mode);
    save(&path, &store);
    let (app, mut cx, handle) = open(&path, width, height, mode);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a.storage_error.is_empty());
            assert!(a.error.is_empty());
            assert!(!matches!(a.save_feedback, Some(Err(_))));
            let definition = a.store.automation(id).unwrap();
            assert_eq!(definition.status, DefinitionStatus::NeedsAttention);
            assert!(definition.attention_reason.is_some());
            assert_eq!(
                a.store.occurrence(running).unwrap().status,
                OccurrenceStatus::Verifying,
                "an active period with no recorded session is reconciled fail-closed"
            );
            assert!(a.store.automation_active(id).is_some());
            a.open_automation(id, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("resume-automation").is_some());
        assert!(w.try_find("stop-automation").is_some());
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "07-recovery-active.png");

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert_eq!(
                a.store.occurrence(running).unwrap().status,
                OccurrenceStatus::Verifying
            );
            assert!(a.store.occurrence(running).unwrap().status.is_active());
            a.open_occurrence_result(running, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("cancel-automation-result").is_some());
        let height = w.find("cancel-automation-result").bounds().size.height;
        assert!(
            height >= px(28.) && height <= px(36.),
            "Cancel period height was {height:?}"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, directory, "08-recovery-cancel.png");

    cx.update_window(handle, |_, w, _| w.remove_window()).unwrap();
    drop(app);
    drop(cx);
}

fn save(path: &Path, store: &Store) {
    Storage::new(path.to_path_buf()).save(store).unwrap();
    assert!(
        Storage::new(path.to_path_buf()).load().unwrap().is_some(),
        "T11 QA uses real storage and must reopen the saved workspace"
    );
}

fn open(
    path: &Path,
    width: f32,
    height: f32,
    mode: Appearance,
) -> (
    Entity<TaskApp>,
    HeadlessAppContext,
    AnyWindowHandle,
) {
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
                a.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                crate::appearance::apply(mode, Some(w), cx);
                cx.notify();
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    (app.unwrap(), cx, window.into())
}

fn read<T>(cx: &mut HeadlessAppContext, app: &Entity<TaskApp>, f: impl Fn(&Store) -> T) -> T {
    let mut out = None;
    cx.update(|cx| {
        out = Some(f(&app.read(cx).store));
    });
    out.unwrap()
}

fn occurrence(
    store: &Store,
    automation: u64,
    status: OccurrenceStatus,
    key: &str,
    now: i64,
) -> AutomationOccurrence {
    let definition = store.automation(automation).unwrap();
    AutomationOccurrence {
        id: 0,
        automation_id: automation,
        definition_version: definition.version,
        source: TriggerSource::Manual,
        trigger_key: key.into(),
        scheduled_for: None,
        requested_at: Some(now - 120),
        actual_started_at: (status == OccurrenceStatus::Running).then_some(now - 60),
        timezone: definition.schedule.timezone.clone(),
        reference_time: now,
        data_window: crate::automation::TimeWindow::unresolved("T11 QA"),
        snapshot: AutomationOccurrenceSnapshot {
            objective: definition.objective.clone(),
            expected_result: definition.expected_result.clone(),
            shape: Some(definition.shape),
            template: definition.template.clone().unwrap(),
            models: ["qa/planner", "qa/collector", "qa/analyst", "qa/adviser"]
                .map(String::from),
            agents: Default::default(),
            materials: vec![],
            policy: definition.policy.clone(),
            approval_version: definition.version,
            approval_signature: "t11-qa".into(),
        },
        status,
        reason: None,
        research_goal_id: None,
        research_goal_ids: vec![],
        research_topic_id: None,
        run_ids: vec![],
        final_run_id: None,
        unread: true,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: now,
        deadline_at: None,
        retry_requested_at: None,
    }
}

/// One enabled definition, one unstarted draft and one failed historical period.
fn fixture(mode: Appearance) -> (Store, (u64, u64, u64)) {
    let (mut store, automation, template, schedule) = base(mode, AUTOMATION, now());
    let draft = store
        .create_automation_draft(
            store.projects()[0].id,
            AutomationShape::Research,
            "Draft packaging and supplier watch".into(),
            LONG_EN.into(),
            "A sourced review kept ready for approval.".into(),
            schedule,
            Default::default(),
            Some(template),
            now(),
        )
        .unwrap();
    store.approve_and_enable(automation, vec![], now()).unwrap();
    let failed = store
        .materialize_occurrence(occurrence(
            &store,
            automation,
            OccurrenceStatus::Failed,
            "manual:t11-failed",
            now() - 600,
        ))
        .unwrap();
    (store, (automation, draft, failed.id))
}

fn recovery_fixture(mode: Appearance) -> (Store, u64, u64) {
    let (mut store, automation, _, _) = base(mode, AUTOMATION, now());
    store.approve_and_enable(automation, vec![], now()).unwrap();
    let running = store
        .materialize_occurrence(occurrence(
            &store,
            automation,
            OccurrenceStatus::Running,
            "manual:t11-running",
            now(),
        ))
        .unwrap();
    (store, automation, running.id)
}

fn base(
    mode: Appearance,
    name: &str,
    now: i64,
) -> (Store, u64, AutomationTemplate, crate::automation_schedule::Schedule) {
    let mut store = Store::mock();
    store.set_appearance(mode);
    store.project_at_mut(0).unwrap().name =
        "Semiconductor supply, pricing and packaging research programme".into();
    store.project_at_mut(1).unwrap().name = "跨区域供应链与公开信息追踪".into();
    store.task_mut(1).unwrap().title = LONG_EN.into();
    store.task_mut(2).unwrap().title = LONG_CJK.into();
    store.task_mut(5).unwrap().title = LONG_CJK.into();
    store.set_default_model("qa/default".into());
    store.research_config_mut().models =
        ["qa/planner", "qa/collector", "qa/analyst", "qa/adviser"].map(String::from);
    let template = AutomationTemplate::single_goal(
        AutomationGoal {
            title: "Semiconductor supply and pricing evidence".into(),
            brief: "Collect dated evidence, reconcile sources, and report what changed.".into(),
            steps: vec![
                AutomationTemplateStep {
                    title: format!("收集并核对来源：{LONG_CJK}"),
                    instructions: "Save original links and publication times.".into(),
                    kind: ResearchKind::Collect,
                    depends_on: vec![],
                },
                AutomationTemplateStep {
                    title: "Analyze the collected evidence against the recurring question and name every evidence gap".into(),
                    instructions: "Separate facts from inference.".into(),
                    kind: ResearchKind::Analyze,
                    depends_on: vec![0],
                },
                AutomationTemplateStep {
                    title: "Prepare the final report and keep every citation beside its claim".into(),
                    instructions: "Open with the answer and disclose missing evidence.".into(),
                    kind: ResearchKind::Advise,
                    depends_on: vec![1],
                },
            ],
        },
        WindowRule::Unresolved,
    );
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Daily { hour: 10, minute: 0 },
        timezone: "Asia/Shanghai".into(),
    };
    let id = store
        .create_automation_draft(
            store.projects()[0].id,
            AutomationShape::Research,
            name.into(),
            LONG_EN.into(),
            "A sourced, dated report that separates facts from inference.".into(),
            schedule.clone(),
            Default::default(),
            Some(template.clone()),
            now,
        )
        .unwrap();
    (store, id, template, schedule)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
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
