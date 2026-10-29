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

/// Isolated calendar navigation and due-date filtering, without live agent calls.
pub fn calendar() {
    use crate::{app::Scope, appearance::Appearance};
    use chrono::Datelike;
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
        w.click("calendar-prev", cx);
        assert_eq!(app.read(cx).calendar_week_offset, -1);
        w.click("calendar-next", cx);
        assert_eq!(app.read(cx).calendar_week_offset, 0);
        w.click("calendar-next", cx);
        let today = chrono::Local::now().date_naive();
        let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday().into()) + chrono::Duration::weeks(1);
        app.update(cx, |a, _| {
            for task in &mut a.store.tasks { task.due_date.clear(); }
            a.store.task_mut(1).unwrap().due_date = monday.to_string();
        });
        w.click("calendar-day-0", cx);
        assert_eq!(app.read(cx).scope, Scope::DueDate(monday.to_string()));
        assert_eq!(app.read(cx).visible_tasks().iter().map(|t| t.id).collect::<Vec<_>>(), vec![1]);
        w.click("calendar-today", cx);
        assert_eq!(app.read(cx).calendar_week_offset, 0);
        assert_eq!(app.read(cx).scope, Scope::DueDate(today.to_string()));
    }).unwrap();
    std::fs::create_dir_all(".local/calendar-qa").unwrap();
    for appearance in [Appearance::Light, Appearance::Dark] {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| a.set_appearance(appearance, w, cx));
            w.render_frame(cx);
        }).unwrap();
        cx.capture_screenshot(handle).unwrap().save(format!(".local/calendar-qa/{appearance:?}.png")).unwrap();
    }
    println!("Calendar navigation and filtering passed");
}
