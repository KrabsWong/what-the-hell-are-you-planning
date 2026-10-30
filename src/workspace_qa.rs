//! First-launch layout check; no user config or live model calls.
use crate::app::TaskApp;
use gpui_kit::{test::TestWindowExt, *};
use std::sync::Arc;
pub fn run() {
    let root = std::env::current_dir().unwrap().join(".local/workspace-onboarding-qa");
    std::fs::create_dir_all(&root).unwrap();
    for (width, height) in [(1000., 680.), (1440., 920.)] {
        let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
        cx.update(init);
        let window = cx.open_window(size(px(width), px(height)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, _| { a.workspace_required = true; });
            cx.new(|cx| base::Root::new(view, w, cx))
        }).unwrap();
        let handle = window.into();
        cx.update_window(handle, |_, w, cx| { w.render_frame(cx); }).unwrap();
        cx.update_window(handle, |_, w, cx| { w.render_frame(cx); }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(root.join(format!("setup-{width}.png"))).unwrap();
    }
    println!("Workspace onboarding screenshots: {}", root.display());
}

/// Verify the deferred date features have no visible entry points.
pub fn calendar() {
    use crate::{domain::Stage, appearance::Appearance};
    let mut cx = HeadlessAppContext::with_platform(platform::current_platform(true).text_system(), Arc::new(assets::Assets), platform::current_headless_renderer);
    cx.update(init);
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(720.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        for id in ["due", "overdue", "research", "calendar-prev", "calendar-today", "calendar-day-0"] {
            assert!(w.try_find(id).is_none(), "Hidden navigation still visible: {id}");
        }
        app.update(cx, |a, cx| {
            a.store.task_mut(1).unwrap().due_date = "2026-12-31".into();
            a.open_task(1, w, cx);
        });
        w.render_frame(cx);
        assert!(w.try_find("edit-date").is_none());
        app.update(cx, |a, cx| a.begin_draft(Some(1), None, Stage::Backlog, w, cx));
        w.render_frame(cx);
        assert!(w.try_find("draft-due").is_none());
        w.click("save-task", cx);
        assert_eq!(app.read(cx).store.task(1).unwrap().due_date, "2026-12-31");
    }).unwrap();
    std::fs::create_dir_all(".local/calendar-qa").unwrap();
    for appearance in [Appearance::Light, Appearance::Dark] {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.set_appearance(appearance, w, cx));
            w.render_frame(cx);
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(format!(".local/calendar-qa/{appearance:?}.png")).unwrap();
    }
    println!("Hidden date controls and preserved existing dates passed");
}
