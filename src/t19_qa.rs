//! T19 ordinary keyboard/visual matrix. Offline only; native acceptance is root-owned.

use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/t19-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    visual_matrix(&root);
    settings_retained_invalidation(&root);
    footer_retention_invalidation(&root);
    sidebar_retained_surface(&root);
    startup_completion_scheduled(&root);
    automation_wizard_quick_review(&root);
    automation_results_compact(&root);
    settings_scroll_probe(&root);
    keyboard_matrix(&root);
    automation_result_keyboard(&root);
    global_search_keyboard(&root);
    task_form_dirty_keyboard(&root);
    println!("T19 QA passed · {}", root.display());
}

fn fixture(directory: &Path) -> Store {
    let mut store = Store::mock();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.project_at_mut(1).unwrap().directory = directory.to_string_lossy().into();
    store
}

fn search_fixture(directory: &Path) -> Store {
    let mut store = fixture(directory);
    store
        .create_research_goal(
            "Polish market research".into(),
            "Track the Polish market signal.".into(),
            1,
        )
        .unwrap();
    store
}

fn automation_results_fixture(
    directory: &Path,
) -> (Store, Vec<u64>, Vec<String>) {
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationShape, OccurrenceStatus,
        TimeWindow, TriggerSource,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    let mut store = fixture(directory);
    let schedule = Schedule {
        spec: ScheduleSpec::Daily { hour: 10, minute: 0 },
        timezone: "Asia/Shanghai".into(),
    };
    let shapes = [
        AutomationShape::Quick,
        AutomationShape::Research,
        AutomationShape::Quick,
        AutomationShape::Research,
        AutomationShape::Quick,
    ];
    let statuses = [
        OccurrenceStatus::Succeeded,
        OccurrenceStatus::Failed,
        OccurrenceStatus::Queued,
        OccurrenceStatus::Succeeded,
        OccurrenceStatus::WaitingRetry,
    ];
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for (index, (shape, status)) in shapes.into_iter().zip(statuses).enumerate() {
        let name = if index % 2 == 0 {
            format!("Shenzhen fuel price and refinery outage watch number {index}")
        } else {
            format!("Long research automation for regional refinery maintenance and price impact {index}")
        };
        let objective = "Check today's fuel prices and summarize the change with sources.";
        let expected = "Current price, change and source.";
        let template = if shape == AutomationShape::Quick {
            crate::automation_execution::build_quick_template(objective, expected)
        } else {
            crate::automation_execution::build_template(objective, expected)
        }
        .unwrap();
        let automation = store
            .create_automation_draft(
                1,
                shape,
                name.clone(),
                objective.into(),
                expected.into(),
                schedule.clone(),
                Default::default(),
                Some(template),
                1_764_500_000 + index as i64 * 600,
            )
            .unwrap();
        let definition = store.automation(automation).unwrap().clone();
        let start = 1_764_600_000 + index as i64 * 3_600;
        let occurrence_id = store.next_occurrence_id().unwrap();
        let occurrence = AutomationOccurrence {
            id: occurrence_id,
            automation_id: automation,
            definition_version: definition.version,
            source: if index % 2 == 0 {
                TriggerSource::Scheduled
            } else {
                TriggerSource::Manual
            },
            trigger_key: crate::automation::scheduled_trigger_key(automation, start),
            scheduled_for: Some(start),
            requested_at: None,
            actual_started_at: Some(start + 60),
            timezone: definition.schedule.timezone.clone(),
            reference_time: start,
            data_window: TimeWindow::unresolved("T19 compact results"),
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
                approval_signature: "T19 compact results".into(),
            },
            status,
            reason: (status == OccurrenceStatus::Failed)
                .then(|| {
                    crate::automation::OccurrenceReason::new(
                        crate::automation::SkipCode::ExecutionFailed,
                        "T19 fixture failure",
                    )
                }),
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: index % 2 == 0,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: start,
            deadline_at: None,
            retry_requested_at: None,
        };
        store.materialize_occurrence(occurrence).unwrap();
        ids.push(occurrence_id);
        names.push(name);
    }
    (store, ids, names)
}

fn visual_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            for surface in ["project-rename", "topic-move", "directory", "new-project"] {
                visual_surface(root, mode, width, height, surface);
            }
            visual_search(root, mode, width, height);
            visual_dirty_exit(root, mode, width, height);
            visual_automation_settings(root, mode, width, height);
            visual_automation_result(root, mode, width, height);
            visual_settings(root, mode, width, height);
        }
    }
}

fn visual_surface(root: &Path, mode: Appearance, width: f32, height: f32, surface: &str) {
    let directory = root.join(format!("{surface}-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = fixture(&directory);
    store.set_appearance(mode);
    if surface == "project-rename" {
        store.project_at_mut(0).unwrap().name =
            "A deliberately long English and 中文 project name used to verify readable disclosure"
                .into();
    }
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        match surface {
            "project-rename" => app.open_project_rename(1, w, cx),
            "topic-move" => app.request_topic_move(1, w, cx),
            "directory" => app.open_directory(1, w, cx),
            "new-project" => {
                app.remember_dialog_opener(w, cx);
                let input = cx.new(|cx| {
                    gpui_kit::component::input::InputState::new(w, cx).placeholder("Project name")
                });
                input.update(cx, |state, cx| state.focus(w, cx));
                app.new_project = Some(input);
            }
            _ => unreachable!(),
        }
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        if surface == "directory" || surface == "new-project" {
            app.update(cx, |app, cx| {
                app.error = if surface == "directory" {
                    "Enter an existing absolute folder path.".into()
                } else {
                    "A project name is required".into()
                };
                cx.notify();
            });
            w.render_frame(cx);
        }
        match surface {
            "project-rename" => {
                w.find("rename-project-name");
                assert!(w.find("cancel-project-rename").visible());
            }
            "topic-move" => {
                assert!(w.find("cancel-topic-move").visible());
            }
            "directory" => {
                w.find("directory-path");
                assert!(w.find("cancel-directory").visible());
            }
            "new-project" => {
                w.find("project-name");
                assert!(w.find("cancel-project").visible());
            }
            _ => unreachable!(),
        }
        let _ = app;
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, &format!("{surface}.png"));
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn visual_search(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("search-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = fixture(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.focus_search(w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.find("global-search-input").focused() == Some(true));
        w.click("global-search-input", cx);
        w.input("Polish", cx);
    })
    .unwrap();
    // Input subscriptions update search results after the window event returns.
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).global_search.read(cx).value().as_ref(),
            "Polish"
        );
        // Children also match their parent's title in the search context.
        let targets: std::collections::BTreeSet<_> = app
            .read(cx)
            .global_search_ui
            .hits
            .iter()
            .map(|hit| hit.target.id())
            .collect();
        assert_eq!(targets, [1, 2, 3, 4, 5].into_iter().collect());
        assert!(
            w.within("global-search-results")
                .find("global-search-result-research-1")
                .visible()
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "search.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn visual_dirty_exit(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("dirty-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = fixture(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(1, w, cx);
        app.begin_draft(1, w, cx);
        let title = app.draft.as_ref().unwrap().title.clone();
        title.update(cx, |state, cx| state.set_value("Dirty visual task", w, cx));
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);
        w.find("dirty-exit-stay");
        let _ = app;
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "dirty-exit.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn visual_automation_settings(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("automation-settings-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = fixture(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::Automations);
        app.start_automation_create(w, cx);
        app.automation_ui.objective.update(cx, |state, cx| {
            state.set_value("Visual automation settings keyboard check", w, cx)
        });
        assert!(app.save_automation_draft(cx));
        let id = app.store.automations().last().unwrap().id;
        app.open_automation(id, w, cx);
        app.open_automation_settings(w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("automation-settings-body");
        assert!(w.find("close-automation-settings").visible());
        let _ = app;
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "automation-settings.png");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).settings_overlay());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn now_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

fn automation_result_fixture(directory: &Path) -> (Store, u64, u64) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().name = "T19 automation".into();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.set_default_model("qa/automation".into());
    let template = crate::automation_execution::build_template(
        "Track a public signal and cite the source.",
        "A short sourced report.",
    )
    .unwrap();
    let schedule = crate::automation_schedule::Schedule {
        spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 10 },
        timezone: "Asia/Shanghai".into(),
    };
    let automation = store
        .create_automation_draft(
            1,
            crate::automation::AutomationShape::Research,
            "T19 result reader".into(),
            "Track a public signal and cite the source.".into(),
            "A short sourced report.".into(),
            schedule,
            Default::default(),
            Some(template),
            now_seconds(),
        )
        .unwrap();
    store
        .approve_and_enable(automation, vec![], now_seconds())
        .unwrap();
    let occurrence = store
        .request_manual_occurrence(automation, "t19-result", now_seconds())
        .unwrap();
    let mut report = String::from("# T19 automation report\n\n");
    for index in 0..40 {
        report.push_str(&format!(
            "Paragraph {index} of the retained automation result.\n\n"
        ));
    }
    let output = serde_json::json!({
        "status": "complete",
        "report": report,
        "sources": [{"title": "T19 source", "url": "https://example.com/t19", "published_at": "unknown"}]
    })
    .to_string();
    let run_id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
    let run = crate::domain::AgentRun {
        automation_occurrence_id: Some(occurrence.id),
        planning_owner: None,
        deadline_at: None,
        auto_retry_attempt: 0,
        retry_not_before: None,
        project_plan: None,
        id: run_id,
        task_id: 0,
        task_title: "T19 automation report".into(),
        directory: directory.to_string_lossy().into(),
        prompt: "T19".into(),
        started_at: crate::opencode::timestamp(),
        queued_at: String::new(),
        workspace_source: None,
        ended_at: Some(crate::opencode::timestamp()),
        session_id: None,
        exit_code: Some(0),
        agent: "OpenCode".into(),
        model: "qa/automation".into(),
        stage: crate::domain::Stage::Done,
        status: "Succeeded".into(),
        logs: vec![],
        output,
        attachments: vec![],
        research_kind: Some(crate::research::ResearchKind::Advise),
        sources: vec![],
        input_signature: String::new(),
    };
    store.push_run(run);
    let occurrence_id = occurrence.id;
    {
        let mut stored = store.occurrence_mut(occurrence_id).unwrap();
        stored.status = crate::automation::OccurrenceStatus::Succeeded;
        stored.run_ids = vec![run_id];
        stored.final_run_id = Some(run_id);
    }
    (store, automation, occurrence_id)
}

fn visual_automation_result(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("automation-result-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, automation, occurrence) = automation_result_fixture(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::Automations);
        app.open_automation(automation, w, cx);
        app.open_occurrence_result(occurrence, w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("automation-result-body");
        assert!(w.find("close-automation-result-footer").visible());
        let _ = app;
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "automation-result.png");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(app.read(cx).period_overlay().is_none());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn automation_result_keyboard(root: &Path) {
    let directory = root.join("keyboard-automation-result");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, automation, occurrence) = automation_result_fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::Automations);
        app.open_automation(automation, w, cx);
        cx.notify();
    });
    for activation in ["enter", "space", "escape"] {
        let (opener, background) = cx
            .update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                let background = (app.read(cx).scope(), app.read(cx).selected());
                w.click(format!("open-occurrence-{occurrence}"), cx);
                let opener = app
                    .read(cx)
                    .dialog_opener_focus
                    .as_ref()
                    .and_then(|focus| focus.upgrade())
                    .expect("live result opener");
                (opener, background)
            })
            .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(app.read(cx).period_overlay(), Some(occurrence));
            assert_eq!(
                w.focused(cx),
                Some(app.read(cx).dialog_first_focus.clone()),
                "result opens on Close"
            );
            assert_eq!(
                w.find("close-automation-result-footer").focused(),
                Some(true)
            );
        })
        .unwrap();
        if activation == "escape" {
            for key in ["tab", "shift-tab"] {
                for _ in 0..24 {
                    cx.update_window(handle, |_, w, cx| {
                        w.press(key, cx);
                    })
                    .unwrap();
                    cx.run_until_parked();
                    cx.update_window(handle, |_, w, cx| {
                        w.render_frame(cx);
                        assert!(
                            focus_inside(w, "automation-result-focus"),
                            "{key}: focus escaped result dialog"
                        );
                    })
                    .unwrap();
                }
            }
        }
        cx.update_window(handle, |_, w, cx| w.press(activation, cx))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            assert!(
                app.read(cx).period_overlay().is_none(),
                "{activation}: result did not close"
            );
            assert_eq!(
                w.focused(cx),
                Some(opener),
                "{activation}: opener not restored"
            );
            assert_eq!(
                (app.read(cx).scope(), app.read(cx).selected()),
                background,
                "{activation}: background changed"
            );
        })
        .unwrap();
    }
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn visual_settings(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("settings-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = fixture(&directory);
    store.set_appearance(mode);
    store.report_publishing_mut().repository = "owner/visual-repo".into();
    store.report_publishing_mut().branch = "main".into();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::Settings);
        app.set_settings_tab(crate::settings_views::SettingsTab::Models);
        app.available_models = vec!["local/plan-model".into(), "local/collect-model".into()];
        app.ego_status = "Runtime ready".into();
        cx.notify();
        let _ = w;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("research-global-model");
        click_in_settings(w, cx, "research-global-model");
        w.render_frame(cx);
        w.find("research-global-model-options");
        w.find("research-model-option-local/plan-model");
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "settings-models-picker.png");
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("research-global-model-options").is_none(),
            "Escape dismisses the settings model picker"
        );
        w.within("settings-tabs").click(0usize, cx);
        w.render_frame(cx);
        w.find("appearance-light");
        w.find("concurrency-increase");
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "settings-general.png");
    cx.update_window(handle, |_, w, cx| {
        w.within("settings-tabs").click(2usize, cx);
        w.render_frame(cx);
        w.find("publishing-repository");
        assert_eq!(
            app.read(cx).publishing_inputs[0].read(cx).value().as_ref(),
            "owner/visual-repo"
        );
        w.find("save-publishing");
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "settings-publishing.png");
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.show_save_error("Could not save settings. Check the workspace folder.", cx)
        });
        w.render_frame(cx);
        assert!(w.find("save-feedback").visible(), "error banner must render");
        assert!(
            w.find("dismiss-save-feedback").visible(),
            "error banner keeps its dismiss action"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "settings-save-error.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn settings_retained_invalidation(root: &Path) {
    let directory = root.join("settings-retained-invalidation");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::Settings);
        app.set_settings_tab(crate::settings_views::SettingsTab::Models);
        cx.notify();
        let _ = w;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("research-global-model");

        // Theme change reaches the retained page and selects the matching control.
        app.update(cx, |app, cx| app.set_appearance(Appearance::Dark, w, cx));
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.appearance(), Appearance::Dark);
        w.within("settings-tabs").click(0usize, cx);
        w.render_frame(cx);
        assert!(
            gpui_kit::component::Theme::global(cx).is_dark(),
            "dark theme must reach the retained page theme"
        );

        // External store update reaches the retained page.
        w.within("settings-tabs").click(1usize, cx);
        w.render_frame(cx);
        app.update(cx, |app, cx| {
            app.store.create_model_profile("T19 Profile").unwrap();
            app.store.apply_model_profile("T19 Profile");
            cx.notify();
        });
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).store.active_model_profile().as_deref(),
            Some("T19 Profile")
        );
        let picker = w
            .find("model-profile-picker")
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(picker.contains("T19 Profile"), "{picker}");

        // Nested input state survives tab changes.
        let nested = app.read(cx).global_model.clone();
        nested.update(cx, |state, cx| state.set_value("test/nested-model", w, cx));
        w.within("settings-tabs").click(0usize, cx);
        w.render_frame(cx);
        w.find("appearance-light");
        w.within("settings-tabs").click(1usize, cx);
        w.render_frame(cx);
        assert_eq!(nested.read(cx).value().as_ref(), "test/nested-model");

        // Async provider completion through the real poll path updates the
        // rendered model catalog without a manual forced rebuild.
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send((
            std::path::PathBuf::from("/t19/opencode"),
            Ok((
                "t19-version".to_string(),
                vec!["local/one".to_string(), "local/two".to_string()],
            )),
        ))
        .unwrap();
        app.update(cx, |app, cx| {
            app.complete_discovery_for_qa(rx, cx);
        });
        w.render_frame(cx);
        assert_eq!(app.read(cx).connection_status, "2 local models available");
        assert_eq!(app.read(cx).available_models.len(), 2);
        click_in_settings(w, cx, "research-model-0");
        w.find("research-model-0-options");
        w.find("research-model-option-local/one");
        w.find("research-model-option-local/two");

        // Escape dismisses suggestions without moving focus, so the next
        // Shift+Tab returns to the preceding provider picker instead of the
        // window footer.
        let model_focus = w.focused(cx);
        assert!(model_focus.is_some(), "model input must own keyboard focus");
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("research-model-0-options").is_none(),
            "Escape must dismiss the model suggestions"
        );
        assert_eq!(
            w.focused(cx),
            model_focus,
            "Escape must keep focus in the model input"
        );
        // Headless frames do not resolve focus traversal out of a focused
        // Input (native Tab order is verified separately), so exercise the
        // app's TabPrev action and require either the restored provider focus
        // or retained input focus. Both prove Escape did not focus the root.
        let tab_prev = cx
            .build_action("root::TabPrev", None)
            .expect("base TabPrev action registered");
        w.dispatch_action(tab_prev, cx);
        w.render_frame(cx);
        let retained = w.focused(cx) == model_focus
            || w.find("model-agent-0").focused() == Some(true);
        assert!(
            retained,
            "TabPrev after dismissing suggestions must keep settings focus"
        );
        assert_ne!(
            w.focused(cx),
            Some(app.read(cx).focus.clone()),
            "dismissing suggestions must not focus the window root"
        );
        assert_ne!(
            w.find("footer-new-research").focused(),
            Some(true),
            "dismissing suggestions must not send focus to the footer"
        );

        // Publishing tab synchronizes its nested inputs from the store on a
        // real tab activation, and its controls stay live in the retained page.
        app.update(cx, |app, cx| {
            app.store.report_publishing_mut().repository = "owner/t19-repo".into();
            app.store.report_publishing_mut().branch = "t19-branch".into();
            cx.notify();
        });
        w.within("settings-tabs").click(2usize, cx);
        w.render_frame(cx);
        w.find("publishing-repository");
        w.find("publishing-branch");
        w.find("publishing-url");
        assert!(w.find("save-publishing").visible());
        let repository = app.read(cx).publishing_inputs[0].clone();
        let branch = app.read(cx).publishing_inputs[1].clone();
        assert_eq!(repository.read(cx).value().as_ref(), "owner/t19-repo");
        assert_eq!(branch.read(cx).value().as_ref(), "t19-branch");

        // Runtime probe completion through the real polling path reaches the
        // rendered Models page (channel result, not a direct field assignment).
        w.within("settings-tabs").click(1usize, cx);
        w.render_frame(cx);
        let (probe_tx, probe_rx) = std::sync::mpsc::channel();
        probe_tx
            .send(Ok("Runtime ready · T19 probe".to_string()))
            .unwrap();
        app.update(cx, |app, cx| {
            app.ego_probe = Some(probe_rx);
            app.poll_research_io(cx);
        });
        w.render_frame(cx);
        assert!(app.read(cx).ego_probe.is_none());
        assert_eq!(app.read(cx).ego_status, "Runtime ready · T19 probe");
        assert_eq!(
            w.find("ego-status").label().map(str::to_string).unwrap_or_default(),
            "Runtime ready · T19 probe"
        );

        // Save errors surface through the existing feedback banner.
        app.update(cx, |app, cx| {
            app.show_save_error("T19 settings error probe", cx);
        });
        w.render_frame(cx);
        assert!(app.read(cx).error.contains("T19 settings error probe"));
        assert!(
            w.find("save-feedback").visible(),
            "save errors must render the feedback banner"
        );

        // Concurrency stays semantic and visible on the General page.
        w.within("settings-tabs").click(0usize, cx);
        w.render_frame(cx);
        app.update(cx, |app, cx| app.set_execution_concurrency(2, cx));
        w.render_frame(cx);
        assert_eq!(app.read(cx).store.concurrency_limit(), 2);
        assert!(
            gpui_kit::component::Theme::global(cx).is_dark(),
            "theme survives the remaining semantic transitions"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn footer_retention_invalidation(root: &Path) {
    let directory = root.join("footer-retention");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        cx.notify();
        let _ = w;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("footer-new-research");
        w.find("footer-search");
        assert_eq!(
            w.find("undo").disabled(),
            Some(true),
            "empty undo stack disables the cached footer command"
        );

        // Cached commands keep working.
        w.click("footer-search", cx);
        w.render_frame(cx);
        assert!(app.read(cx).global_search_ui.open);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        w.click("footer-new-research", cx);
        w.render_frame(cx);
        assert!(app.read(cx).planning_project.is_some());
        w.press("escape", cx);
        w.render_frame(cx);

        // A real domain edit invalidates the footer; clicking the enabled Undo
        // must execute the scoped inverse and disable the command again. This
        // proves interactivity instead of inferring from an absent flag.
        let topic_name = "Footer cache check";
        app.update(cx, |app, cx| {
            app.apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TopicCreate {
                    project: 1,
                    name: topic_name.into(),
                },
                "Topic created",
                cx,
            );
            cx.notify();
        });
        w.render_frame(cx);
        assert!(app.read(cx).store.topics().iter().any(|t| t.name == topic_name));
        assert!(w.find("undo").visible());
        w.click("undo", cx);
        w.render_frame(cx);
        assert!(
            !app.read(cx).store.topics().iter().any(|t| t.name == topic_name),
            "cached Undo must remove the created topic"
        );
        assert_eq!(
            w.find("undo").disabled(),
            Some(true),
            "consumed Undo disables the cached footer command"
        );

        // Persistence errors surface through the cached footer and clear again.
        app.update(cx, |app, cx| {
            app.storage_error = "T19 footer storage failure".into();
            cx.notify();
        });
        w.render_frame(cx);
        w.find("retry-save");
        app.update(cx, |app, cx| {
            app.storage_error.clear();
            cx.notify();
        });
        w.render_frame(cx);
        assert!(
            w.try_find("retry-save").is_none(),
            "clearing the storage error invalidates the cached footer"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn sidebar_retained_surface(root: &Path) {
    let directory = root.join("sidebar-retained");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        cx.notify();
        let _ = w;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let brand = w.find("sidebar-brand-overlay").bounds();
        assert!(brand.size.width > px(20.), "brand mark must stay visible");

        // Pointer routing through the overlay area: a project row at the top of
        // the retained sidebar still navigates.
        w.click("project-1", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Project(1));

        // Expanding a project and opening the bottom workspace menu prove the
        // overlay interior does not intercept sidebar controls.
        w.click("expand-project-1", cx);
        w.render_frame(cx);
        assert!(app.read(cx).expanded_sidebar_projects.contains(&1));
        w.click("workspace-menu", cx);
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);

        // Wheel scrolling moves only navigation; the identity stays fixed.
        let before_scroll = app.read(cx).sidebar_scroll.offset().y;
        w.scroll(
            "sidebar-scroll",
            ScrollDelta::Pixels(point(px(0.), px(-200.))),
            cx,
        );
        w.render_frame(cx);
        let after_scroll = app.read(cx).sidebar_scroll.offset().y;
        assert!(after_scroll < before_scroll - px(10.));
        let clip = w.find("sidebar-logo-clip").bounds();
        let slot = w.find("sidebar-brand-slot").bounds();
        let moved = w.find("sidebar-brand-overlay").bounds();
        assert!(
            (moved.top() - brand.top()).abs() <= px(0.5),
            "brand must stay fixed above navigation scroll"
        );
        assert!(
            (moved.top() - slot.top()).abs() <= px(0.5)
                && (moved.left() - slot.left()).abs() <= px(0.5)
                && (moved.size.width - slot.size.width).abs() <= px(0.5),
            "brand overlay must match the identity-row slot at nonzero scroll"
        );
        // Scroll to the content end without moving the identity row.
        for _ in 0..20 {
            let before = app.read(cx).sidebar_scroll.offset().y;
            w.scroll(
                "sidebar-scroll",
                ScrollDelta::Pixels(point(px(0.), px(-400.))),
                cx,
            );
            w.render_frame(cx);
            if app.read(cx).sidebar_scroll.offset().y == before {
                break;
            }
        }
        let mark = w.find("sidebar-brand-overlay");
        let end_slot = w.find("sidebar-brand-slot").bounds();
        assert!(mark.visible(), "brand must remain visible after scrolling");
        assert!((mark.bounds().top() - brand.top()).abs() <= px(0.5));
        assert!((end_slot.top() - slot.top()).abs() <= px(0.5));
        assert!(mark.bounds().bottom() <= clip.bottom());

        // Dragging the resize divider still resizes the sidebar panel.
        let width_before = w.find("sidebar-scroll").bounds().size.width;
        w.drag(point(px(256.), px(400.)), point(px(356.), px(400.)), cx);
        w.render_frame(cx);
        assert!(
            w.find("sidebar-scroll").bounds().size.width > width_before,
            "sidebar divider must resize the retained panel"
        );

        // Dark theme keeps the overlay mark present at the same anchor.
        app.update(cx, |app, cx| app.set_appearance(Appearance::Dark, w, cx));
        w.render_frame(cx);
        assert!(w.find("sidebar-brand-overlay").visible());
        // The overlay stays pointer-transparent in the dark theme as well.
        w.click("project-2", cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).scope(), Scope::Project(2));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn startup_completion_scheduled(root: &Path) {
    use std::sync::atomic::Ordering;
    struct NotifyProbe {
        count: Arc<std::sync::atomic::AtomicUsize>,
        _subscription: Option<gpui_kit::Subscription>,
    }
    let directory = root.join("startup-scheduled");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        cx.notify();
        let _ = w;
    });
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let probe = cx.update(|cx| cx.new(|_| NotifyProbe { count: count.clone(), _subscription: None }));
    probe.update(&mut cx, |this, cx| {
        this._subscription = Some(cx.observe(&app, |this, _, _| {
            this.count.fetch_add(1, Ordering::SeqCst);
        }));
    });
    let before = cx.update_window(handle, |_, w, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        app.update(cx, |app, cx| {
            app.startup.pending = Some(rx);
            app.startup.required = true;
            app.startup.silent = true;
            app.start_pump(cx);
        });
        w.render_frame(cx);
        w.find("startup-checking");
        let before = count.load(Ordering::SeqCst);
        tx.send([
            Ok("1.0".to_string()),
            Err("Codex not configured".to_string()),
            Err("ego not configured".to_string()),
        ])
        .unwrap();
        drop(tx);
        before
    })
    .unwrap();
    // Drive the headless executor outside the App/window update borrow.
    // No manual pump tick, click, resize, or render happens here.
    for _ in 0..8 {
        cx.advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        if cx.update(|cx| app.read(cx).startup.pending.is_none()) {
            break;
        }
    }
    cx.update_window(handle, |_, w, cx| {
        assert!(
            app.read(cx).startup.pending.is_none(),
            "the scheduled pump must consume startup completion without user input"
        );
        assert!(
            count.load(Ordering::SeqCst) > before,
            "startup completion must notify the root view for repaint"
        );
        w.render_frame(cx);
        assert!(
            w.try_find("startup-checking").is_none(),
            "Starting… must disappear after scheduled startup completion"
        );
        w.find("startup-recheck");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn automation_wizard_quick_review(root: &Path) {
    let directory = root.join("automation-wizard-review");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::Automations);
        cx.notify();
        let _ = w;
    });
    let mut cancel_step1 = Bounds::default();
    let mut action_step1 = Bounds::default();

    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("new-automation", cx);
        w.render_frame(cx);
        w.click("automation-shape-quick", cx);
        w.render_frame(cx);
        w.click("automation-objective", cx);
        w.input("Check today's fuel price", cx);
        w.click("automation-expected", cx);
        w.input("Current price and source", cx);
        w.render_frame(cx);
        cancel_step1 = w.find("cancel-automation-wizard").bounds();
        action_step1 = w.find("automation-next-step").bounds();

        w.click("automation-next-step", cx);
        w.render_frame(cx);
        let cancel_step2 = w.find("cancel-automation-wizard").bounds();
        assert!(
            (cancel_step2.left() - cancel_step1.left()).abs() <= px(0.5)
                && (cancel_step2.right() - cancel_step1.right()).abs() <= px(0.5),
            "step 2 must keep the wizard Cancel anchor"
        );
        assert_eq!(
            w.find("automation-schedule-note").label(),
            Some("Times use the schedule and time zone above.")
        );
        let action_step2 = w.find("automation-next-step").bounds();
        assert!(
            (action_step2.right() - action_step1.right()).abs() <= px(0.5),
            "step 2 must keep the primary action anchored"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "wizard-step2.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("automation-next-step", cx);
        w.render_frame(cx);
        assert_eq!(
            w.find("automation-review-heading").label(),
            Some("Review and authorization"),
            "Quick's final step is authorization, not an approved plan"
        );
        assert!(
            w.try_find("automation-plan-goal-0").is_none(),
            "Quick must not expose synthetic goal-tree internals"
        );
        let card = w.find("automation-quick-authorization");
        let card_text = card.label().unwrap_or_default();
        assert!(card_text.contains("Check today's fuel price"));
        assert!(card_text.contains("Current price and source"));
        let cancel_step3 = w.find("cancel-automation-wizard").bounds();
        let action_step3 = w.find("enable-automation").bounds();
        assert!(
            (cancel_step3.left() - cancel_step1.left()).abs() <= px(0.5)
                && (cancel_step3.right() - cancel_step1.right()).abs() <= px(0.5),
            "step 3 must keep the wizard Cancel anchor"
        );
        assert!(
            (action_step3.right() - action_step1.right()).abs() <= px(0.5),
            "step 3 must keep the primary action anchored"
        );
        assert!(
            w.find("enable-automation").visible(),
            "authorization keeps the enabling action"
        );
        let _ = app;
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "wizard-step3.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn automation_results_compact(root: &Path) {
    let (store, ids, names) = automation_results_fixture(root);
    for (width, height, mode) in [
        (1000., 680., Appearance::Dark),
        (1000., 680., Appearance::Light),
        (1440., 920., Appearance::Dark),
        (1440., 920., Appearance::Light),
    ] {
        let directory = root.join(format!("automation-results-{width}-{mode:?}"));
        std::fs::create_dir_all(&directory).unwrap();
        let mut scenario_store = store.clone();
        scenario_store.set_appearance(mode);
        let path = directory.join("workspace.sqlite3");
        Storage::new(path.clone()).save(&scenario_store).unwrap();
        let (mut cx, handle, app) = with_app(scenario_store, &path, width, height, |app, w, cx| {
            app.set_scope_identity(Scope::AutomationRuns);
            cx.notify();
            let _ = w;
        });
        cx.update_window(handle, |_, w, cx| {
            w.render_frame(cx);
            w.find("automation-results-subtitle");
            let viewport = w.viewport_size();
            let subtitle = w.find("automation-results-subtitle").bounds();
            assert!(
                subtitle.right() <= px(width) + px(1.),
                "{width}x{height} {mode:?}: subtitle must stay inside the viewport"
            );
            let hint_snapshot = w.find("automation-results-unread-hint");
            let hint = hint_snapshot.bounds();
            assert!(
                hint_snapshot.visible() && hint.right() <= px(width) + px(1.),
                "{width}x{height} {mode:?}: unread hint must be visible and bounded"
            );
            for (id, name) in ids.iter().zip(names.iter()) {
                let row = w.find(format!("automation-occurrence-{id}"));
                assert!(
                    row.visible() && row.bounds().right() <= viewport.width + px(1.),
                    "{width}x{height} {mode:?}: result row must be inside the viewport"
                );
                let label = row.label().unwrap_or_default();
                assert!(label.contains(name), "row keeps the automation identity");
                assert!(
                    label.contains("Succeeded")
                        || label.contains("Failed")
                        || label.contains("Queued")
                        || label.contains("Waiting to retry"),
                    "row keeps its status"
                );
                let action = w.find(format!("open-occurrence-{id}"));
                assert!(
                    action.visible() && action.bounds().right() <= viewport.width + px(1.),
                    "{width}x{height} {mode:?}: Open action must be reachable"
                );
                let name_bounds = w.find(format!("occurrence-name-{id}")).bounds();
                assert!(
                    name_bounds.size.width > px(60.)
                        && name_bounds.right() <= action.bounds().left(),
                    "{width}x{height} {mode:?}: visible name retains space before Open"
                );
                for field in ["activity", "trigger", "status"] {
                    let field = w.find(format!("occurrence-{field}-{id}"));
                    assert!(
                        field.visible() && field.bounds().right() <= action.bounds().left(),
                        "{width}x{height} {mode:?}: metadata stays inside the text column"
                    );
                }
            }
        })
        .unwrap();
        screenshot(
            &mut cx,
            handle,
            &directory,
            &format!("automation-results-{width}-{mode:?}.png"),
        );
        cx.update_window(handle, |_, w, cx| {
            let first = ids[0];
            w.click(format!("open-occurrence-{first}"), cx);
            w.render_frame(cx);
            assert_eq!(app.read(cx).period_overlay(), Some(first));
            w.press("escape", cx);
            w.render_frame(cx);
            assert!(app.read(cx).period_overlay().is_none());
            assert_eq!(app.read(cx).scope(), Scope::AutomationRuns);
        })
        .unwrap();
        cx.update_window(handle, |_, w, _| w.remove_window())
            .unwrap();
    }
}

fn click_in_settings(w: &mut Window, cx: &mut App, id: &str) {
    w.render_frame(cx);
    for _ in 0..30 {
        let bounds = w.find(id.to_string()).bounds();
        let top = w.find("model-settings-scroll").bounds().top();
        if bounds.top() >= top && bounds.bottom() <= w.viewport_size().height - px(40.) {
            break;
        }
        let delta = if bounds.top() < top { 200. } else { -200. };
        w.scroll(
            "model-settings-scroll",
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
    }
    w.click(id.to_string(), cx);
}

fn settings_scroll_probe(root: &Path) {
    let directory = root.join("settings-scroll-probe");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::Settings);
        app.set_settings_tab(crate::settings_views::SettingsTab::Models);
        cx.notify();
        let _ = w;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let start = crate::brand::scroll_render_probe();
        for _ in 0..60 {
            w.render_frame(cx);
        }
        let idle = crate::brand::scroll_render_probe();
        for _gesture in 0..6 {
            for _frame in 0..10 {
                w.scroll(
                    "model-settings-scroll",
                    ScrollDelta::Pixels(point(px(0.), px(-320.))),
                    cx,
                );
                w.render_frame(cx);
            }
        }
        let gesture = crate::brand::scroll_render_probe();
        let delta = |end: crate::brand::ScrollRenderProbe,
                     start: crate::brand::ScrollRenderProbe| {
            (
                end.app - start.app,
                end.sidebar - start.sidebar,
                end.logo - start.logo,
                end.settings - start.settings,
                end.general - start.general,
            )
        };
        let idle_delta = delta(idle, start);
        let gesture_delta = delta(gesture, idle);
        println!(
            "scroll probe 60 idle frames: app={} sidebar={} logo={} settings={} general={}",
            idle_delta.0, idle_delta.1, idle_delta.2, idle_delta.3, idle_delta.4
        );
        println!(
            "scroll probe 6 gesture x10 frames: app={} sidebar={} logo={} settings={} general={}",
            gesture_delta.0, gesture_delta.1, gesture_delta.2, gesture_delta.3, gesture_delta.4
        );
        let before_general = crate::brand::scroll_render_probe().general;
        app.update(cx, |app, cx| {
            app.set_settings_tab(crate::settings_views::SettingsTab::General);
            cx.notify();
        });
        w.render_frame(cx);
        let after_general = crate::brand::scroll_render_probe().general;
        assert!(
            after_general > before_general,
            "General tab must still construct its settings subtree"
        );
        let _ = app;
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn keyboard_matrix(root: &Path) {
    let scenarios: &[(&str, fn(&mut TaskApp, &mut Window, &mut Context<TaskApp>))] = &[
        ("project-rename", |app, w, cx| {
            app.open_project_rename(1, w, cx)
        }),
        ("topic-move", |app, w, cx| app.request_topic_move(1, w, cx)),
        ("directory", |app, w, cx| app.open_directory(1, w, cx)),
    ];
    for (surface, open) in scenarios {
        keyboard_surface(root, surface, *open);
    }
}

fn keyboard_surface(
    root: &Path,
    surface: &str,
    open: fn(&mut TaskApp, &mut Window, &mut Context<TaskApp>),
) {
    let directory = root.join(format!("keyboard-{surface}"));
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.fixture_select_task(Some(1));
        cx.notify();
    });
    let mut opener: Option<FocusHandle> = None;
    let mut background: (Scope, Option<u64>) = (Scope::All, None);
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("project-1", cx);
        w.render_frame(cx);
        opener = w.focused(cx);
        background = (app.read(cx).scope(), app.read(cx).selected());
        assert!(opener.is_some(), "{surface}: real opener focus required");
        app.update(cx, |app, cx| open(app, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    if surface == "topic-move" {
        cx.update_window(handle, |_, w, cx| {
            w.click("topic-move-project", cx);
            w.render_frame(cx);
            w.press("escape", cx);
            w.render_frame(cx);
            assert!(
                dialog_open(&app.read(cx), surface),
                "first Escape must close only the Select popup"
            );
        })
        .unwrap();
        cx.run_until_parked();
    }
    for key in ["tab", "tab", "tab", "tab", "tab", "tab", "tab", "tab"] {
        cx.update_window(handle, |_, w, cx| {
            w.press(key, cx);
            w.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, _| {
            assert!(focus_inside_dialog(w, surface), "{surface}: Tab escaped");
        })
        .unwrap();
    }
    for key in [
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
        "shift-tab",
    ] {
        cx.update_window(handle, |_, w, cx| {
            w.press(key, cx);
            w.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, w, _| {
            assert!(
                focus_inside_dialog(w, surface),
                "{surface}: Shift-Tab escaped"
            );
        })
        .unwrap();
    }
    // Close the cycle pass, then reopen from the page for activation.
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            !dialog_open(&app.read(cx), surface),
            "{surface}: Escape did not close"
        );
        assert_eq!(
            w.focused(cx),
            opener,
            "{surface}: opener not restored after Escape"
        );
        app.update(cx, |app, cx| open(app, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    // Real Enter/Space activation from the visible Cancel control.
    let cancel_id = match surface {
        "project-rename" => "cancel-project-rename",
        "topic-move" => "cancel-topic-move",
        "directory" => "cancel-directory",
        _ => unreachable!(),
    };
    let mut activated = false;
    for _ in 0..10 {
        let hit = cx
            .update_window(handle, |_, w, _| w.find(cancel_id).focused() == Some(true))
            .unwrap();
        if hit {
            activated = true;
            break;
        }
        cx.update_window(handle, |_, w, cx| {
            w.press("tab", cx);
            w.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    assert!(activated, "{surface}: Cancel never received focus");
    cx.update_window(handle, |_, w, cx| {
        w.press("enter", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            !dialog_open(&app.read(cx), surface),
            "{surface}: Enter did not activate Cancel"
        );
        assert_eq!(
            w.focused(cx),
            opener,
            "{surface}: opener not restored after Cancel"
        );
        assert_eq!(
            (app.read(cx).scope(), app.read(cx).selected()),
            background.clone(),
            "{surface}: background moved after Cancel"
        );
        app.update(cx, |app, cx| open(app, w, cx));
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            !dialog_open(&app.read(cx), surface),
            "{surface}: Escape did not close"
        );
        assert_eq!(
            w.focused(cx),
            opener,
            "{surface}: opener not restored after Escape"
        );
        assert_eq!(
            (app.read(cx).scope(), app.read(cx).selected()),
            background.clone(),
            "{surface}: background moved after Escape"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn global_search_keyboard(root: &Path) {
    let directory = root.join("keyboard-global-search");
    std::fs::create_dir_all(&directory).unwrap();
    let store = search_fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        cx.notify();
    });
    let mut opener: Option<FocusHandle> = None;
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("footer-search", cx);
        w.render_frame(cx);
        opener = app.read(cx).global_search_ui.previous_focus.clone();
        assert!(opener.is_some(), "Search must remember its live opener");
        assert_eq!(w.find("global-search-input").focused(), Some(true));
        w.input("Polish", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        assert!(
            !app.read(cx).global_search_ui.hits.is_empty(),
            "Polish fixture must produce search hits"
        );
        for key in ["tab", "tab", "tab", "tab", "tab", "tab"] {
            w.press(key, cx);
            w.render_frame(cx);
            assert!(search_focus_inside(w), "Tab left visible search controls");
        }
        for key in [
            "shift-tab",
            "shift-tab",
            "shift-tab",
            "shift-tab",
            "shift-tab",
            "shift-tab",
        ] {
            w.press(key, cx);
            w.render_frame(cx);
            assert!(
                search_focus_inside(w),
                "Shift-Tab left visible search controls"
            );
        }
        w.click("global-search-input", cx);
        w.press("down", cx);
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(!app.read(cx).global_search_ui.open);
        assert_eq!(w.focused(cx), opener, "search opener restored");
        assert_eq!(app.read(cx).scope(), Scope::All);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn search_focus_inside(w: &mut Window) -> bool {
    w.find("global-search-input").focused() == Some(true)
        || w.find("global-search-close").focused() == Some(true)
        || ["all", "research", "automations"]
            .iter()
            .any(|key| w.find(format!("global-search-domain-{key}")).focused() == Some(true))
}

fn task_form_dirty_keyboard(root: &Path) {
    let directory = root.join("keyboard-task-dirty");
    std::fs::create_dir_all(&directory).unwrap();
    let store = fixture(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(1, w, cx);
        app.begin_draft(1, w, cx);
        let title = app.draft.as_ref().unwrap().title.clone();
        title.update(cx, |state, cx| {
            state.set_value("Dirty keyboard task", w, cx)
        });
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(app.read(cx).navigation.pending_exit.is_some());
        w.find("dirty-exit-stay");
        for _ in 0..6 {
            w.press("tab", cx);
            w.render_frame(cx);
            let inside = ["dirty-exit-stay", "dirty-exit-discard", "dirty-exit-save"]
                .iter()
                .any(|id| w.find(*id).focused() == Some(true));
            assert!(inside, "Tab left dirty-exit controls");
        }
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(app.read(cx).navigation.pending_exit.is_none());
        assert!(app.read(cx).draft.is_some());
        assert_eq!(app.read(cx).selected(), Some(1));
        w.press("escape", cx);
        w.render_frame(cx);
        if app.read(cx).draft.is_some() {
            w.click("dirty-exit-stay", cx);
            w.render_frame(cx);
        }
        assert!(app.read(cx).draft.is_some(), "Stay keeps the draft");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn dialog_open(app: &TaskApp, surface: &str) -> bool {
    match surface {
        "project-rename" => app.project_rename.is_some(),
        "topic-move" => app.topic_move.is_some(),
        "directory" => app.directory_project.is_some(),
        _ => false,
    }
}

fn focus_inside(w: &mut Window, scope: &'static str) -> bool {
    w.within(scope)
        .elements()
        .iter()
        .any(|element| element.focused() == Some(true))
}

fn focus_inside_dialog(w: &mut Window, surface: &str) -> bool {
    let ids: &[&str] = match surface {
        "project-rename" => &[
            "rename-project-name",
            "cancel-project-rename",
            "save-project-rename",
        ],
        "topic-move" => &[
            "topic-move-project",
            "cancel-topic-move",
            "confirm-topic-move",
        ],
        "directory" => &["directory-path", "cancel-directory", "save-directory"],
        _ => &[],
    };
    ids.iter().any(|id| {
        w.try_find(*id)
            .is_some_and(|snapshot| snapshot.focused() == Some(true))
    })
}

fn with_app<F>(
    store: Store,
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
    let mut entity = None;
    let handle = cx
        .open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |app, cx| {
                app.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                assert!(app.storage_error.is_empty(), "{}", app.storage_error);
                let appearance = store.appearance();
                app.store = store;
                app.set_appearance(appearance, w, cx);
                configure(app, w, cx);
                cx.notify();
            });
            entity = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap()
        .into();
    (cx, handle, entity.unwrap())
}

fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, directory: &Path, name: &str) {
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
