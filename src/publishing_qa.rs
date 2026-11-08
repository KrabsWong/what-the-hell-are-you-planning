//! Native publishing settings and status acceptance. Never pushes or requests a network resource.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
    persistence::Storage,
    report_publish::{Publication, PublishEvent},
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    path::Path,
    sync::{Arc, mpsc},
};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/publishing-qa/{}", std::process::id()));
    for (width, height) in [(1440., 920.), (1000., 720.)] {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let directory = root.join(format!("{appearance:?}-{width}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("workspace.sqlite3");
            let mut initial = Store::empty();
            initial.set_appearance(appearance);
            initial.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
            let topic = initial.create_topic(1, "Gold research and investment observations").unwrap();
            let goal = initial
                .create_research_goal_in_topic(
                    "Research gold trends".into(),
                    "Incomplete isolated acceptance research".into(),
                    1,
                    Some(topic),
                )
                .unwrap();
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
                        a.open_model_settings(w, cx);
                    });
                    app = Some(view.clone());
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            let app = app.unwrap();
            let handle = window.into();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.within("settings-tabs").click(2usize, cx);
                w.render_frame(cx);
                assert_eq!(app.read(cx).scope(), Scope::Settings);
                assert!(!app.read(cx).store.report_publishing().automatic);
                for (id, value) in [
                    (
                        "publishing-repository",
                        "git@github.com:fixture-owner/private-reports.git",
                    ),
                    ("publishing-branch", "reports/live"),
                    ("publishing-url", "https://reports.example.com"),
                ] {
                    click(w, cx, id);
                    w.press("cmd-a", cx);
                    w.input(value, cx);
                }
                click(w, cx, "save-publishing");
                assert!(app.read(cx).storage_error.is_empty());
                let saved = Storage::new(path.clone()).load().unwrap().unwrap();
                assert_eq!(
                    saved.report_publishing().repository,
                    "git@github.com:fixture-owner/private-reports.git"
                );
                assert_eq!(saved.report_publishing().branch, "reports/live");
                assert_eq!(
                    saved.report_publishing().base_url,
                    "https://reports.example.com"
                );
                click(w, cx, "automatic-publishing");
                assert!(
                    Storage::new(path.clone())
                        .load()
                        .unwrap()
                        .unwrap()
                        .report_publishing()
                        .automatic
                );
                assert!(
                    app.read(cx).publishing_job.is_none(),
                    "Incomplete topics must not start publishing"
                );
            })
            .unwrap();
            screenshot(
                &mut cx,
                handle,
                &directory,
                "01-publishing-settings-toggle.png",
            );
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    assert!(a.settings_tab == crate::settings_views::SettingsTab::Publishing);
                    a.settings_scroll.set_offset(point(px(0.), px(0.)));
                    cx.notify();
                });
                w.render_frame(cx);
            })
            .unwrap();
            screenshot(
                &mut cx,
                handle,
                &directory,
                "01-publishing-settings-top.png",
            );
            cx.update_window(handle, |_, w, cx| {
                click(w, cx, "automatic-publishing");
                assert!(!Storage::new(path.clone()).load().unwrap().unwrap().report_publishing().automatic);
                app.update(cx, |a, cx| a.navigate(Scope::Topic(topic), w, cx));
                w.render_frame(cx);
                assert_eq!(w.find("publish-topic-report").disabled(), Some(true));
                assert_eq!(w.find("publish-topic-report").label(), Some("Publish report"));
                assert!(app.read(cx).publishing_job.is_none());
                let publish_bounds = w.find("publish-topic-report").bounds();
                assert!(publish_bounds.size.width < px(220.), "Publish must remain a compact action");
                if width >= 1440. {
                    assert_eq!(publish_bounds.top(), w.find("topic-new-goal").bounds().top());
                    assert!(publish_bounds.right() <= w.find("topic-new-goal").bounds().left());
                }
                // Exercise the real enabled click handler, but reject configuration before any IO.
                let repository = app.read(cx).store.report_publishing().repository.clone();
                app.update(cx, |a, cx| {
                    a.store.task_mut(goal).unwrap().stage = Stage::Done;
                    a.store.report_publishing_mut().repository = "invalid-fixture-repository".into();
                    cx.notify();
                });
                w.render_frame(cx);
                assert_ne!(w.find("publish-topic-report").disabled(), Some(true));
                w.click("publish-topic-report", cx);
                assert!(!app.read(cx).error.is_empty(), "Enabled click must reach configuration validation");
                assert!(app.read(cx).publishing_job.is_none());
                assert!(app.read(cx).store.publications().is_empty());
                app.update(cx, |a, cx| {
                    a.store.task_mut(goal).unwrap().stage = Stage::Planned;
                    a.store.report_publishing_mut().repository = repository;
                    a.error.clear();
                    a.save_feedback = None;
                    for (id, status, message) in [
                        ("00000000000000000000000000000001", "Online", ""),
                        ("00000000000000000000000000000002", "Pushed · awaiting deployment", "GitHub received this snapshot. EdgeOne deployment is still pending."),
                        ("00000000000000000000000000000003", "Failed", "Fixture authentication failure. Configure local Git authentication and retry."),
                    ] {
                        a.store.push_publication(Publication {
                            id: id.into(), topic, fingerprint: id.into(), config: a.store.report_publishing().clone(),
                            title: "Gold research and investment observations".into(), markdown: "Fixture report".into(),
                            status: status.into(), commit: None, message: message.into(),
                        });
                    }
                    cx.notify();
                });
                w.render_frame(cx);
                for id in ["00000000000000000000000000000001", "00000000000000000000000000000002", "00000000000000000000000000000003"] {
                    assert!(w.find(format!("copy-report-{id}")).visible());
                    assert!(w.find(format!("open-report-{id}")).visible());
                }
                for id in ["00000000000000000000000000000001", "00000000000000000000000000000002", "00000000000000000000000000000003"] {
                    assert!(w.try_find(format!("retry-report-{id}")).is_none(), "Rows must not duplicate the header action");
                }
                // The product keeps one permanent publication per topic. The
                // extra records above exist only to exercise legacy history
                // rows; the header action and later publishing flow use the
                // current record, so isolate that record before changing state.
                let current_id = "00000000000000000000000000000003";
                app.update(cx, |a, cx| {
                    a.store.retain_publications(|p| p.topic != topic || p.id == current_id);
                    cx.notify();
                });
                w.render_frame(cx);
                for (status, commit, expected_label) in [
                    ("Online", None, "View report"),
                    ("Pushed · awaiting deployment", Some("fixture-commit-sha"), "Check status"),
                    ("Pushed · awaiting deployment", None, "Retry publishing"),
                    ("Failed", None, "Retry publishing"),
                ] {
                    app.update(cx, |a, cx| {
                        let index = crate::report_publish::current_publication_index(
                            &a.store.publications(),
                            topic,
                        )
                        .expect("current publication");
                        let latest = &mut a.store.publications()[index];
                        assert_eq!(latest.id, current_id, "The header must read the publication under test");
                        latest.status = status.into();
                        latest.commit = commit.map(str::to_string);
                        cx.notify();
                    });
                    w.render_frame(cx);
                    assert_eq!(w.find("publish-topic-report").label(), Some(expected_label));
                    assert_ne!(w.find("publish-topic-report").disabled(), Some(true));
                }
                let clipboard = cx.read_from_clipboard();
                w.click("copy-report-00000000000000000000000000000003", cx);
                assert_eq!(cx.read_from_clipboard().unwrap().text().as_deref(), Some("https://reports.example.com/00000000000000000000000000000003/"));
                assert_eq!(app.read(cx).toast, "Report link copied");
                cx.write_to_clipboard(clipboard.unwrap_or_else(|| ClipboardItem::new_string(String::new())));
            }).unwrap();
            screenshot(
                &mut cx,
                handle,
                &directory,
                "02-topic-publication-status.png",
            );
            let (progress_tx, progress_rx) = mpsc::channel();
            let publication_id = "00000000000000000000000000000003";
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.publishing_job = Some((publication_id.into(), progress_rx));
                    cx.notify();
                });
                progress_tx
                    .send(PublishEvent::Progress("Generating HTML".into(), None))
                    .unwrap();
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                assert_eq!(
                    app.read(cx).store.publications().last().unwrap().status,
                    "Generating HTML"
                );
                w.render_frame(cx);
                assert_eq!(w.find("publish-topic-report").disabled(), Some(true));
                assert_eq!(w.find("publish-topic-report").label(), Some("Publishing…"));
                assert!(
                    w.try_find(format!("retry-report-{publication_id}"))
                        .is_none()
                );
                progress_tx
                    .send(PublishEvent::Progress(
                        "Pushed · verifying live page".into(),
                        Some("fixture-commit-sha".into()),
                    ))
                    .unwrap();
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                w.render_frame(cx);
                assert_eq!(w.find("publish-topic-report").label(), Some("Checking…"));
                let publication = app.read(cx).store.publications().last().unwrap();
                assert_eq!(publication.status, "Pushed · verifying live page");
                assert_eq!(publication.commit.as_deref(), Some("fixture-commit-sha"));
                assert!(publication.message.contains("GitHub has received"));
                let saved = Storage::new(path.clone()).load().unwrap().unwrap();
                assert_eq!(
                    saved.publications().last().unwrap().status,
                    "Pushed · verifying live page"
                );
                assert_eq!(
                    saved.publications().last().unwrap().commit.as_deref(),
                    Some("fixture-commit-sha")
                );
                let description_id = format!("report-description-{publication_id}");
                let checking_bounds = w.find(description_id.clone()).bounds();
                app.update(cx, |a, cx| {
                    let p = a.store.publications().last_mut().unwrap();
                    p.status = "Pushed · awaiting deployment".into();
                    p.message = "Check finished; deployment pending".into();
                    cx.notify();
                });
                w.render_frame(cx);
                assert_eq!(w.find(description_id.clone()).bounds().size.width, checking_bounds.size.width);
                app.update(cx, |a, cx| {
                    let p = a.store.publications().last_mut().unwrap();
                    p.status = "Pushed · verifying live page".into();
                    p.message.clear();
                    cx.notify();
                });
                w.render_frame(cx);
                assert_eq!(w.find(description_id.clone()).bounds().size.width, checking_bounds.size.width);
                app.update(cx, |a, cx| {
                    let p = a.store.publications().last_mut().unwrap();
                    p.status = "Pushed · site access error".into();
                    p.message = "Report pushed to GitHub successfully. Public page returned HTTP 545. EdgeOne reports an edge function execution error. Check the hosting service, then use Check status; no new push is needed.".into();
                    a.publishing_job = None;
                    cx.notify();
                });
                w.render_frame(cx);
                assert_eq!(w.find("publish-topic-report").label(), Some("Check status"));
                let description = w.find(description_id).bounds();
                assert!(description.size.height >= checking_bounds.size.height);
                assert_eq!(description.size.width, checking_bounds.size.width);
                assert_eq!(app.read(cx).store.publications().last().unwrap().commit.as_deref(), Some("fixture-commit-sha"));

            })
            .unwrap();
            screenshot(
                &mut cx,
                handle,
                &directory,
                "03-topic-publishing-progress.png",
            );
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, _| {
                    a.publishing_job = None;
                });
                w.remove_window();
            })
            .unwrap();
            drop(progress_tx);
            drop(app);
            drop(cx);
            let mut cx = context();
            let window = cx
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                    view.update(cx, |a, cx| {
                        a.attach_storage(Storage::new(path.clone()), w, cx);
                        a.open_model_settings(w, cx);
                        a.refresh_publishing_inputs(w, cx);
                        assert_eq!(
                            a.publishing_inputs[0].read(cx).value().as_ref(),
                            "git@github.com:fixture-owner/private-reports.git"
                        );
                        assert_eq!(
                            a.publishing_inputs[1].read(cx).value().as_ref(),
                            "reports/live"
                        );
                        assert_eq!(
                            a.publishing_inputs[2].read(cx).value().as_ref(),
                            "https://reports.example.com"
                        );
                        assert!(!a.store.report_publishing().automatic);
                        assert!(a.publishing_job.is_none());
                    });
                    cx.new(|cx| base::Root::new(view, w, cx))
                })
                .unwrap();
            cx.update_window(window.into(), |_, w, _| w.remove_window())
                .unwrap();
        }
    }
    println!(
        "PUBLISHING QA PASSED: repository/branch/site save and restart, automatic toggle persistence, incomplete topic gating, compact stateful header, copy feedback, live progress + commit persistence, Online/Pushed/Failed controls; light/dark at 1440 and 1000 pixels. No network operations. {}",
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
fn click(w: &mut Window, cx: &mut App, id: &str) {
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
fn screenshot(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, directory: &Path, name: &str) {
    // Headless animation time is deterministic; immediate frames leave the tab spring frozen.
    for _ in 0..10 {
        cx.advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(directory.join(name))
        .unwrap();
}
