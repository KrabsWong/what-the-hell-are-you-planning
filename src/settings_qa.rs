//! Native global-model acceptance in isolated workspaces; never calls a model.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
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
            initial.appearance = mode;
            initial.projects[0].directory = directory.to_string_lossy().into();
            initial.research_config.models[0] = "qa/planner".into();
            initial
                .policies
                .iter_mut()
                .find(|p| p.stage == Stage::Review)
                .unwrap()
                .model = "qa/reviewer".into();
            initial
                .policies
                .iter_mut()
                .find(|p| p.stage == Stage::Done)
                .unwrap()
                .model = "qa/unused-manual".into();
            Storage::new(path.clone()).save(&initial).unwrap();
            let mut cx = context();
            let mut app = None;
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        assert!(a.storage_error.is_empty());
                        a.available_models = vec!["qa/other".into(), MODEL.into()];
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
                    assert_eq!(app.read(cx).store.appearance, expected);
                    assert_eq!(
                        Storage::new(path.clone())
                            .load()
                            .unwrap()
                            .unwrap()
                            .appearance,
                        expected
                    );
                }
                assert_eq!(app.read(cx).store.appearance, mode);
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "00-appearance-cycle.png");
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.click("model-settings", cx);
                assert_eq!(app.read(cx).scope, Scope::Settings);
                for _ in 0..2 {
                    if app.read(cx).store.appearance == Appearance::System {
                        break;
                    }
                    w.click("toggle-appearance", cx);
                }
                assert_eq!(app.read(cx).store.appearance, Appearance::System);
                assert_eq!(w.find("appearance-system").label(), Some("✓ Follow system"));
                assert_eq!(
                    Storage::new(path.clone())
                        .load()
                        .unwrap()
                        .unwrap()
                        .appearance,
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
                assert_eq!(app.read(cx).store.appearance, mode);
                assert_eq!(
                    w.find(if mode == Appearance::Light {
                        "appearance-light"
                    } else {
                        "appearance-dark"
                    })
                    .label(),
                    Some(if mode == Appearance::Light {
                        "✓ Light"
                    } else {
                        "✓ Dark"
                    })
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "00-settings-appearance.png");
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.policy_prompt
                        .update(cx, |s, cx| s.set_value("Unsaved stage instructions", w, cx));
                    a.ego_input
                        .update(cx, |s, cx| s.set_value("/tmp/unsaved-ego", w, cx));
                    a.research_models[1].update(cx, |s, cx| s.set_value("qa/unsaved", w, cx));
                });
                w.click("global-model-input", cx);
                w.input("missing-provider", cx);
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert!(app.read(cx).store.default_model.is_empty());
                assert!(!app.read(cx).error.is_empty());
                assert!(matches!(app.read(cx).save_feedback, Some(Err(_))));
                w.render_frame(cx);
                assert!(w.try_find("save-feedback").is_some());
                click(w, cx, "model-settings-scroll", "reset-global-model");
                click(w, cx, "model-settings-scroll", "global-model-input");
                w.input("qa/original", cx);
                click(w, cx, "model-settings-scroll", "global-task-overrides");
                w.click("model-settings", cx);
                assert_eq!(
                    app.read(cx).global_model.read(cx).value().as_ref(),
                    "qa/original"
                );
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert_eq!(app.read(cx).store.default_model, "qa/original");
                assert!(matches!(app.read(cx).save_feedback, Some(Ok(_))));
                w.render_frame(cx);
                w.click("dismiss-save-feedback", cx);
                assert!(app.read(cx).save_feedback.is_none());
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert!(
                    matches!(app.read(cx).save_feedback, Some(Ok(_))),
                    "Repeated saves still confirm success"
                );
                assert_eq!(
                    app.read(cx).policy_prompt.read(cx).value().as_ref(),
                    "Unsaved stage instructions"
                );
                assert_eq!(
                    app.read(cx).ego_input.read(cx).value().as_ref(),
                    "/tmp/unsaved-ego"
                );
                assert_eq!(
                    app.read(cx).research_models[1].read(cx).value().as_ref(),
                    "qa/unsaved"
                );
                assert_eq!(app.read(cx).store.research_config.models[0], "qa/planner");
                // An unsupported on-disk file must produce a visible failure, never a success.
                let original = std::fs::read(&path).unwrap();
                app.update(cx, |a, cx| {
                    a.global_model
                        .update(cx, |s, cx| s.set_value("qa/write-retry", w, cx))
                });
                std::fs::write(&path, r#"{"version":999}"#).unwrap();
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert!(matches!(app.read(cx).save_feedback, Some(Err(_))));
                assert!(!app.read(cx).storage_error.is_empty());
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    r#"{"version":999}"#
                );
                std::fs::write(&path, original).unwrap();
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert!(matches!(app.read(cx).save_feedback, Some(Ok(_))));
                assert!(app.read(cx).storage_error.is_empty());
                assert_eq!(
                    Storage::new(path.clone())
                        .load()
                        .unwrap()
                        .unwrap()
                        .default_model,
                    "qa/write-retry"
                );
                app.update(cx, |a, _| {
                    a.available_models
                        .extend((0..30).map(|i| format!("qa/scroll-{i:02}")));
                });
                click(w, cx, "model-settings-scroll", "choose-global-model");
                click(w, cx, "model-settings-scroll", "global-model-search");
                w.render_frame(cx);
                let search_top = w.find("global-model-search").bounds().top();
                let option_top = w.find("global-model-option-qa/scroll-00").bounds().top();
                w.scroll(
                    "global-model-list",
                    ScrollDelta::Pixels(point(px(0.), px(-90.))),
                    cx,
                );
                w.render_frame(cx);
                assert_eq!(
                    w.find("global-model-search").bounds().top(),
                    search_top,
                    "Scrolling models must not scroll the settings page"
                );
                assert!(
                    w.find("global-model-option-qa/scroll-00").bounds().top() < option_top,
                    "The model list itself must still scroll"
                );
                w.input("tencent", cx);
                w.render_frame(cx);
                let search_top = w.find("global-model-search").bounds().top();
                for delta in [-300., 300.] {
                    w.scroll(
                        "global-model-list",
                        ScrollDelta::Pixels(point(px(0.), px(delta))),
                        cx,
                    );
                    w.render_frame(cx);
                    assert_eq!(
                        w.find("global-model-search").bounds().top(),
                        search_top,
                        "A short filtered list must not pass scrolling to the settings page"
                    );
                }
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "01-model-picker.png");
            cx.update_window(handle, |_, w, cx| {
                assert!(w.try_find("global-model-option-qa/other").is_none());
                click(
                    w,
                    cx,
                    "model-settings-scroll",
                    &format!("global-model-option-{MODEL}"),
                );
                assert_eq!(app.read(cx).global_model.read(cx).value().as_ref(), MODEL);
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert_eq!(app.read(cx).store.default_model, MODEL);
                assert_eq!(
                    app.read(cx).store.prepare_project_plan(1).unwrap().model,
                    "qa/planner"
                );
                assert_eq!(
                    app.read(cx)
                        .store
                        .policies
                        .iter()
                        .find(|p| p.stage == Stage::Review)
                        .unwrap()
                        .model,
                    "qa/reviewer"
                );
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "02-saved-with-overrides.png");
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "model-settings-scroll", "global-research-overrides");
                assert!(app.read(cx).show_research_settings);
                click(w, cx, "research-scroll", "research-use-global-0");
                click(w, cx, "research-scroll", "save-research-settings");
                assert!(app.read(cx).store.research_config.models[0].is_empty());
                assert_eq!(
                    app.read(cx).store.prepare_project_plan(1).unwrap().model,
                    MODEL
                );
                w.click("model-settings", cx);
                click(w, cx, "model-settings-scroll", "global-task-overrides");
                click(w, cx, "workflow-scroll", "policy-review");
                click(w, cx, "workflow-scroll", "policy-use-global");
                click(w, cx, "workflow-scroll", "save-policy");
                assert!(
                    app.read(cx)
                        .store
                        .policies
                        .iter()
                        .find(|p| p.stage == Stage::Review)
                        .unwrap()
                        .model
                        .is_empty()
                );
                // Reintroduce overrides to verify the explicit all-stages action clears both families.
                app.update(cx, |a, cx| {
                    a.store.research_config.models[1] = "qa/collector".into();
                    a.policy_prompt.update(cx, |s, cx| {
                        s.set_value("Preserve instructions when clearing models", w, cx)
                    });
                    a.ego_input
                        .update(cx, |s, cx| s.set_value("/tmp/preserve-ego", w, cx));
                    a.store
                        .policies
                        .iter_mut()
                        .find(|p| p.stage == Stage::Review)
                        .unwrap()
                        .model = "qa/reviewer".into();
                    cx.notify();
                });
                w.click("model-settings", cx);
                click(w, cx, "model-settings-scroll", "global-model-all-stages");
                assert!(app.read(cx).use_global_for_all);
                assert_eq!(
                    app.read(cx).store.research_config.models[1],
                    "qa/collector",
                    "Checkbox alone must not save"
                );
                click(w, cx, "model-settings-scroll", "save-global-model");
                assert_eq!(
                    app.read(cx).policy_prompt.read(cx).value().as_ref(),
                    "Preserve instructions when clearing models"
                );
                assert_eq!(
                    app.read(cx).ego_input.read(cx).value().as_ref(),
                    "/tmp/preserve-ego"
                );
                let store = app.read(cx).store.clone();
                assert!(store.research_config.models.iter().all(String::is_empty));
                assert!(store.all_stages_use_global_model());
                w.render_frame(cx);
                assert_eq!(w.find("global-model-all-stages").checked(), Some(true));
                assert!(
                    store
                        .policies
                        .iter()
                        .filter(|p| p.agent == "OpenCode")
                        .all(|p| p.model.is_empty())
                );
                assert_eq!(
                    store
                        .policies
                        .iter()
                        .find(|p| p.stage == Stage::Done)
                        .unwrap()
                        .model,
                    "qa/unused-manual"
                );
                assert_eq!(store.tasks, initial.tasks);
                assert_eq!(store.runs, initial.runs, "Saving never starts a run");
                app.update(cx, |a, cx| a.open_project_plan(1, w, cx));
                w.render_frame(cx);
                assert_ne!(w.find("generate-project-plan").disabled(), Some(true));
            })
            .unwrap();
            screenshot(&mut cx, handle, &directory, "03-planning-global.png");
            let expected = cx
                .update_window(handle, |_, w, cx| {
                    w.click("close-project-plan", cx);
                    app.read(cx).store.clone()
                })
                .unwrap();
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
                        a.navigate(Scope::Settings, w, cx);
                        assert_eq!(a.global_model.read(cx).value().as_ref(), MODEL);
                    });
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            screenshot(&mut cx, window.into(), &directory, "04-reopened-global.png");
            cx.update_window(window.into(), |_, w, _| {
                assert_eq!(
                    w.find("global-model-all-stages").checked(),
                    Some(true),
                    "Reopen must show saved inheritance"
                );
            })
            .unwrap();
            cx.update_window(window.into(), |_, w, _| w.remove_window())
                .unwrap();
        }
    }
    println!(
        "MODEL SETTINGS QA PASSED: validation, picker filtering, save, inheritance, individual/all-stage reset, planning eligibility and restart; light/dark at both sizes. {}",
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
fn click(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    for _ in 0..30 {
        let bounds = w.find(id.to_string()).bounds();
        if bounds.top() >= px(110.) && bounds.bottom() <= w.viewport_size().height - px(40.) {
            break;
        }
        let delta = if bounds.top() < px(110.) { 200. } else { -200. };
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
