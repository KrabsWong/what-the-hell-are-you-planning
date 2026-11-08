//! Native global-model acceptance in isolated workspaces; never calls a model.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc};

const MODEL: &str = "tencent-token-plan/deepseek/deepseek-flash";

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/settings-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{mode:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.json");
            let mut initial = Store::empty();
            initial.set_appearance(mode);
            initial.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
            initial.research_config_mut().models[0] = "qa/planner".into();
            Storage::new(path.clone()).save(&initial).unwrap();
            let mut cx = context();
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty());
                        a.workspace_root = Some(directory.clone());
                        a.available_models = vec!["qa/other".into(), MODEL.into()];
                        a.codex_models = vec!["qa-codex-model".into(), "qa-codex-other".into()];
                        a.codex_version = Some("codex-cli QA".into());
                        a.codex_status = "2 fixture models available".into();
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle = window.into();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                let mut expected = mode;
                for _ in 0..3 {
                    expected = expected.next();
                    w.click("toggle-appearance", cx);
                    assert_eq!(app.read(cx).store.appearance(), expected);
                    assert_eq!(
                        Storage::new(path.clone())
                            .load()
                            .unwrap()
                            .unwrap()
                            .appearance(),
                        expected
                    );
                }
                assert_eq!(app.read(cx).store.appearance(), mode);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "00-appearance-cycle.png");
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("workspace-menu", cx);
                w.render_frame(cx);
                w.within("popup-menu").click(3usize, cx);
                assert_eq!(app.read(cx).scope(), Scope::Settings);
                for _ in 0..2 {
                    if app.read(cx).store.appearance() == Appearance::System {
                        break;
                    }
                    w.click("toggle-appearance", cx);
                }
                assert_eq!(app.read(cx).store.appearance(), Appearance::System);
                assert_eq!(w.find("appearance-system").label(), Some("Follow system"));
                assert_eq!(
                    Storage::new(path.clone())
                        .load()
                        .unwrap()
                        .unwrap()
                        .appearance(),
                    Appearance::System
                );
                let expected: gpui_kit::component::ThemeMode = w.appearance().into();
                assert_eq!(
                    gpui_kit::component::Theme::global(cx).is_dark(),
                    expected.is_dark()
                );
                w.click(
                    if mode == Appearance::Light {
                        "appearance-light"
                    } else {
                        "appearance-dark"
                    },
                    cx,
                );
                assert_eq!(app.read(cx).store.appearance(), mode);
                assert_eq!(
                    w.find(if mode == Appearance::Light {
                        "appearance-light"
                    } else {
                        "appearance-dark"
                    })
                    .label(),
                    Some(if mode == Appearance::Light {
                        "Light"
                    } else {
                        "Dark"
                    })
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "00-settings-appearance.png");
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).store.concurrency_limit(), 3);
                for limit in 4..=10 {
                    w.click("concurrency-increase", cx);
                    assert_eq!(app.read(cx).store.concurrency_limit(), limit);
                    assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap().concurrency_limit(), limit);
                }
                w.click("concurrency-increase", cx);
                assert_eq!(app.read(cx).store.concurrency_limit(), 10);
                for limit in (1..10).rev() {
                    w.click("concurrency-decrease", cx);
                    assert_eq!(app.read(cx).store.concurrency_limit(), limit);
                }
                w.click("concurrency-decrease", cx);
                assert_eq!(app.read(cx).store.concurrency_limit(), 1);
                w.click("concurrency-increase", cx);
                w.click("concurrency-increase", cx);
                assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap().concurrency_limit(), 3);
                // Automation periods have their own persisted pool.
                w.scroll(
                    "model-settings-scroll",
                    ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
                    cx,
                );
                assert_eq!(app.read(cx).store.automation_concurrency_limit(), 2);
                for limit in 3..=10 {
                    w.click("automation-concurrency-increase", cx);
                    assert_eq!(app.read(cx).store.automation_concurrency_limit(), limit);
                    assert_eq!(
                        Storage::new(path.clone())
                            .load()
                            .unwrap()
                            .unwrap()
                            .automation_concurrency_limit(),
                        limit
                    );
                }
                w.click("automation-concurrency-increase", cx);
                assert_eq!(app.read(cx).store.automation_concurrency_limit(), 10);
                for limit in (1..10).rev() {
                    w.click("automation-concurrency-decrease", cx);
                    assert_eq!(app.read(cx).store.automation_concurrency_limit(), limit);
                }
                w.click("automation-concurrency-decrease", cx);
                assert_eq!(app.read(cx).store.automation_concurrency_limit(), 1);
                w.click("automation-concurrency-increase", cx);
                assert_eq!(
                    Storage::new(path.clone())
                        .load()
                        .unwrap()
                        .unwrap()
                        .automation_concurrency_limit(),
                    2
                );
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                w.within("settings-tabs").click(1usize, cx);
                w.render_frame(cx);
                assert!(w.try_find("appearance-system").is_none());
                click(w, cx, "model-settings-scroll", "research-global-model");
                assert!(app.read(cx).research_model_picker.is_some());
                // Non-focusable page whitespace must dismiss without relying on InputEvent::Blur.
                let width = w.find("model-settings-scroll").bounds().size.width;
                w.click_at("model-settings-scroll", point(width - px(4.), px(8.)), cx);
                assert!(app.read(cx).research_model_picker.is_none());
                click(w, cx, "model-settings-scroll", "research-global-model");
                assert!(app.read(cx).research_model_picker.is_some());
                w.click("research-global-model", cx);
                assert!(app.read(cx).research_model_picker.is_some(), "Input clicks keep its menu open");
                w.input("missing-provider", cx);
                w.press("escape", cx);
                assert!(w.try_find("save-research-settings").is_none());
                assert!(w.try_find("reset-model-settings").is_none());
                assert!(app.read(cx).store.default_model().is_empty());
                click(w, cx, "model-settings-scroll", "research-global-model");
                w.press("cmd-a", cx);
                w.input("qa/original", cx);
                w.press("enter", cx);
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(Storage::new(path.clone()).load().unwrap().unwrap().default_model(), "qa/original");
                app.update(cx, |a, _| {
                    a.available_models
                        .extend((0..30).map(|i| format!("qa/scroll-{i:02}")));
                });
                click(w, cx, "model-settings-scroll", "research-global-model");
                w.render_frame(cx);
                let search_top = w.find("research-global-model").bounds().top();
                let option_top = w.find("research-model-option-qa/scroll-00").bounds().top();
                w.scroll(
                    "research-global-model-options",
                    ScrollDelta::Pixels(point(px(0.), px(-90.))),
                    cx,
                );
                w.render_frame(cx);
                assert_eq!(
                    w.find("research-global-model").bounds().top(),
                    search_top,
                    "Scrolling models must not scroll the settings page"
                );
                assert!(
                    w.find("research-model-option-qa/scroll-00").bounds().top() < option_top,
                    "The model list itself must still scroll"
                );
                w.press("cmd-a", cx);
                w.input("tencent", cx);
                w.render_frame(cx);
                let search_top = w.find("research-global-model").bounds().top();
                for delta in [-300., 300.] {
                    w.scroll(
                        "research-global-model-options",
                        ScrollDelta::Pixels(point(px(0.), px(delta))),
                        cx,
                    );
                    w.render_frame(cx);
                    assert_eq!(
                        w.find("research-global-model").bounds().top(),
                        search_top,
                        "A short filtered list must not pass scrolling to the settings page"
                    );
                }
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "01-model-picker.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("research-model-option-qa/other").is_none());
                click(
                    w,
                    cx,
                    "model-settings-scroll",
                    &format!("research-model-option-{MODEL}"),
                );
                assert_eq!(app.read(cx).global_model.read(cx).value().as_ref(), MODEL);
                assert_eq!(app.read(cx).store.default_model(), MODEL);
                assert_eq!(
                    app.read(cx).store.prepare_project_plan(1).unwrap().model,
                    "qa/planner"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-saved-with-overrides.png");
            cx.update_window(handle, |_, w, cx| {
                w.within("settings-tabs").click(1usize, cx);
                w.render_frame(cx);
                assert!(w.try_find("research-global-model").is_some());
                assert_eq!(app.read(cx).scope(), Scope::Settings);
                assert!(w.try_find("ego-path").is_some());
                assert!(w.try_find("research-model-0").is_some());
            }).unwrap();
            screenshot(&mut cx, handle, &directory, "02-research-model-fields.png");
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-agent-0");
                w.within("popup-menu").click(1usize, cx);
                w.render_frame(cx);
                assert_eq!(app.read(cx).store.research_agent(crate::research::ResearchKind::Goal), "Codex");
                assert_eq!(w.find("research-model-0").placeholder(), Some("Choose a model"));
                assert!(app.read(cx).store.research_model(crate::research::ResearchKind::Goal).is_empty());
                click(w, cx, "model-settings-scroll", "research-model-0");
                assert!(w.try_find("research-model-option-qa/other").is_none());
                assert!(w.try_find("research-model-option-qa-codex-model").is_some());
                w.click("research-model-option-qa-codex-model", cx);
                let persisted = Storage::new(path.clone()).load().unwrap().unwrap();
                assert_eq!(persisted.research_config().agents[0].as_deref(), Some("Codex"));
                assert_eq!(persisted.research_config().models[0], "qa-codex-model");
                assert_eq!(persisted.prepare_project_plan(1).unwrap().agent, "Codex");
                assert_eq!(persisted.prepare_project_plan(1).unwrap().model, "qa-codex-model");
                assert!(app.read(cx).codex_discovery.is_none(), "Fixture choices never start live discovery");
            }).unwrap();
            screenshot(&mut cx, handle, &directory, "02-codex-stage.png");
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "research-use-global-0");
                let persisted = Storage::new(path.clone()).load().unwrap().unwrap();
                assert!(persisted.research_config().agents[0].is_none());
                assert!(persisted.research_config().models[0].is_empty());
                assert_eq!(persisted.research_agent(crate::research::ResearchKind::Goal), "OpenCode");
                assert_eq!(persisted.research_model(crate::research::ResearchKind::Goal), MODEL);
                w.render_frame(cx);
                assert_eq!(w.find("research-model-0").placeholder(), Some("Global default"));
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "research-global-model");
                w.press("cmd-a", cx);
                w.input("OTH", cx);
                assert_eq!(app.read(cx).store.default_model(), MODEL);
                w.click("research-model-option-qa/other", cx);
                assert_eq!(app.read(cx).global_model.read(cx).value().as_ref(), "qa/other");
                assert_eq!(app.read(cx).store.default_model(), "qa/other");
                click(w, cx, "model-settings-scroll", "research-model-0");
                assert!(w.try_find("research-model-option-qa/other").is_some());
                w.press("cmd-a", cx);
                w.input("OTH", cx);
                w.render_frame(cx);
                assert!(w.try_find("research-model-option-qa/scroll-00").is_none());
                w.click("research-model-option-qa/other", cx);
                assert_eq!(app.read(cx).research_models[0].read(cx).value().as_ref(), "qa/other");
                assert!(app.read(cx).research_model_picker.is_none());
                click(w, cx, "model-settings-scroll", "research-use-global-0");
                click(w, cx, "model-settings-scroll", "ego-path");
                w.press("cmd-a", cx);
                w.input("/tmp/qa-ego", cx);
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).store.research_config().ego_path, "/tmp/qa-ego");
                assert!(app.read(cx).store.research_config().models[0].is_empty());
                assert_eq!(
                    app.read(cx).store.prepare_project_plan(1).unwrap().model,
                    "qa/other"
                );

                // Set an override to exercise the bulk reset.
                app.update(cx, |a, cx| {
                    a.store.research_config_mut().models[1] = "qa/collector".into();
                    a.store.research_config_mut().agents[1] = Some("Codex".into());
                    a.research_models[1].update(cx, |s, cx| s.set_value("qa/collector", w, cx));
                    a.ego_input
                        .update(cx, |s, cx| s.set_value("/tmp/preserve-ego", w, cx));
                    cx.notify();
                });
                w.click("workspace-menu", cx);
                w.render_frame(cx);
                w.within("popup-menu").click(3usize, cx);
                click(w, cx, "model-settings-scroll", "global-model-all-stages");
                assert!(app.read(cx).research_models.iter().all(|m| m.read(cx).value().is_empty()));
                let tab_top = w.within("settings-tabs").find(0usize).bounds().top();
                w.scroll(
                    "model-settings-scroll",
                    ScrollDelta::Pixels(point(px(0.), px(-200.))),
                    cx,
                );
                w.render_frame(cx);
                assert_eq!(w.within("settings-tabs").find(0usize).bounds().top(), tab_top);
                w.within("settings-tabs").click(0usize, cx);
                w.within("settings-tabs").click(1usize, cx);
                assert!(app.read(cx).research_models.iter().all(|m| m.read(cx).value().is_empty()));
                assert_eq!(
                    app.read(cx).store.research_config().models[1],
                    "",
                    "Bulk reset saves immediately"
                );
                assert_eq!(
                    app.read(cx).ego_input.read(cx).value().as_ref(),
                    "/tmp/preserve-ego"
                );
                let store = app.read(cx).store.clone();
                assert!(store.research_config().models.iter().all(String::is_empty));
                assert!(store.research_config().agents.iter().all(Option::is_none));
                w.render_frame(cx);
                assert_eq!(w.find("global-model-all-stages").disabled(), Some(true));
                assert_eq!(store.tasks(), initial.tasks());
                assert_eq!(store.runs(), initial.runs(), "Saving never starts a run");
                app.update(cx, |a, cx| a.open_project_plan(1, w, cx));
                w.render_frame(cx);
                assert_ne!(w.find("generate-project-plan").disabled(), Some(true));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-planning-global.png");
            cx.update_window(handle, |_, w, cx| {
                w.click("close-project-plan", cx);
                app.update(cx, |a, cx| a.open_model_settings(w, cx));
            }).unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("model-profile-new", cx);
            }).unwrap();
            screenshot(&mut cx, handle, &directory, "05-new-profile.png");
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-profile-cancel");
                w.render_frame(cx);
                assert!(w.try_find("model-profile-name").is_none());
            }).unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("model-profile-new", cx);
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-profile-create");
                assert!(app.read(cx).store.model_profiles().is_empty());
                app.update(cx, |a, cx| {
                    a.model_profile_name.update(cx, |s, cx| s.set_value("Common Research", w, cx));
                });
                click(w, cx, "model-settings-scroll", "model-profile-create");
                assert_eq!(app.read(cx).store.model_profiles().len(), 1);
                w.render_frame(cx);
                assert!(w.try_find("model-profile-name").is_none());
                assert_eq!(w.find("model-profile-update").disabled(), Some(true));
                assert!(app.read(cx).save_feedback.is_none());
            }).unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("model-profile-new", cx);
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.set_model_agent(Some(1), "Codex", w, cx);
                    let input = a.research_models[1].clone();
                    input.update(cx, |s, cx| s.set_value("qa-codex-model", w, cx));
                    a.finish_model_edit(&input, true, w, cx);
                    a.model_profile_name.update(cx, |s, cx| s.set_value("深度研究 · 多 Agent 协作方案", w, cx));
                });
                click(w, cx, "model-settings-scroll", "model-profile-create");
            }).unwrap();
            cx.run_until_parked();
            profile_menu(&mut cx, handle, "model-profile-picker", 0);
            cx.update_window(handle, |_, _, cx| {
                assert!(app.read(cx).store.research_config().agents.iter().all(Option::is_none));
                assert!(app.read(cx).research_models[1].read(cx).value().is_empty());
            }).unwrap();
            cx.run_until_parked();
            profile_menu(&mut cx, handle, "model-profile-picker", 1);
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).research_models[1].read(cx).value().as_ref(), "qa-codex-model");
                assert_eq!(app.read(cx).store.research_config().agents[1].as_deref(), Some("Codex"));
                w.render_frame(cx);
                let save_bounds = w.find("model-profile-update").bounds();
                assert_eq!(w.find("model-profile-update").disabled(), Some(true));
                app.update(cx, |a, cx| a.use_global_for_stage(1, w, cx));
                w.render_frame(cx);
                assert_eq!(w.find("model-profile-update").bounds(), save_bounds);
                assert_ne!(w.find("model-profile-update").disabled(), Some(true));
                w.scroll("model-settings-scroll", ScrollDelta::Pixels(point(px(0.), px(-400.))), cx);
                w.render_frame(cx);
                assert_eq!(w.find("model-profile-update").bounds(), save_bounds, "Profile actions stay fixed while scrolling");
                app.update(cx, |a, cx| {
                    a.settings_scroll.set_offset(point(px(0.), px(0.)));
                    cx.notify();
                });
            }).unwrap();
            screenshot(&mut cx, handle, &directory, "06-modified-profile.png");
            profile_menu(&mut cx, handle, "model-profile-menu", 0);
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).store.research_config().models[1], "qa-codex-model");
                assert_eq!(app.read(cx).research_models[1].read(cx).value().as_ref(), "qa-codex-model");
                w.render_frame(cx);
                assert_eq!(w.find("model-profile-update").disabled(), Some(true));
                app.update(cx, |a, cx| a.use_global_for_stage(1, w, cx));
            }).unwrap();
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-profile-update");
                assert!(app.read(cx).store.model_profiles()[1].models[1].is_empty());
            }).unwrap();
            cx.run_until_parked();
            profile_menu(&mut cx, handle, "model-profile-menu", 2);
            cx.update_window(handle, |_, w, cx| {
                assert_eq!(app.read(cx).store.model_profiles().len(), 1);
                assert_eq!(app.read(cx).store.default_model(), "qa/other");
                app.update(cx, |a, cx| a.undo_change(w, cx));
                assert_eq!(app.read(cx).store.model_profiles().len(), 2);
            }).unwrap();
            cx.run_until_parked();
            profile_menu(&mut cx, handle, "model-profile-picker", 0);
            let expected = cx.update_window(handle, |_, _, cx| app.read(cx).store.clone()).unwrap();
            assert_eq!(
                Storage::new(path.clone()).load().unwrap().unwrap(),
                expected
            );
            cx.update_window(handle, |_, w, _| w.remove_window())
                .unwrap();
            drop(app);
            drop(cx);
            let mut cx = context();
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert_eq!(a.store, expected);
                        a.open_model_settings(w, cx);
                        assert_eq!(a.global_model.read(cx).value().as_ref(), "qa/other");
                    });
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            screenshot(&mut cx, window.into(), &directory, "04-reopened-global.png");
            cx.update_window(window.into(), |_, w, _| {
                assert_eq!(
                    w.find("global-model-all-stages").disabled(),
                    Some(true),
                    "Reopen must show saved inheritance"
                );
            })
            .unwrap();
            cx.update_window(window.into(), |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-profile-picker");
            }).unwrap();
            screenshot(&mut cx, window.into(), &directory, "07-profile-picker.png");
            cx.update_window(window.into(), |_, w, cx| {
                w.within("popup-menu").click(0usize, cx);
            }).unwrap();
            cx.run_until_parked();
            cx.update_window(window.into(), |_, w, cx| {
                click(w, cx, "model-settings-scroll", "model-profile-menu");
            }).unwrap();
            screenshot(&mut cx, window.into(), &directory, "08-profile-management.png");
            cx.update_window(window.into(), |_, w, _| w.remove_window())
                .unwrap();
        }
    }
    println!(
        "MODEL SETTINGS QA PASSED: validation, picker filtering, save, inheritance, individual/all-stage reset, profiles create/apply/update/delete/undo, planning eligibility and restart; light/dark at both sizes. {}",
        root.display()
    );
}

fn context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    cx
}
fn profile_menu(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, id: &str, index: usize) {
    cx.update_window(handle, |_, w, cx| click(w, cx, "model-settings-scroll", id)).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.within("popup-menu").click(index, cx);
    }).unwrap();
    cx.run_until_parked();
}
fn click(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    if scroll == "model-settings-scroll" {
        if w.try_find(id.to_string()).is_none() {
            w.within("settings-tabs").click(1usize, cx);
            w.render_frame(cx);
        }
    }
    if id.starts_with("model-profile-") {
        w.click(id.to_string(), cx);
        return;
    }
    for _ in 0..30 {
        let bounds = w.find(id.to_string()).bounds();
        let top = w.find(scroll.to_string()).bounds().top();
        if bounds.top() >= top && bounds.bottom() <= w.viewport_size().height - px(40.) {
            break;
        }
        let delta = if bounds.top() < top { 200. } else { -200. };
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
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join(name))
        .unwrap();
}
