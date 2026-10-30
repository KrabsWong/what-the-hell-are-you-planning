//! Read-only frame benchmark against a caller-supplied workspace.
use crate::{
    app::{Scope, TaskApp},
    persistence::Storage,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{sync::Arc, time::Instant};
pub fn run() {
    let path = std::env::var("PLANNING_PERF_WORKSPACE").expect("PLANNING_PERF_WORKSPACE");
    let store = Storage::new(path.into()).load().unwrap().unwrap();
    let start = Instant::now();
    for _ in 0..10 {
        for task in &store.tasks {
            std::hint::black_box(store.step_state(task.id));
        }
    }
    println!(
        "all step states: {:.2} ms/pass",
        start.elapsed().as_secs_f64() * 100.
    );
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| init(cx));
    let mut entity = None;
    let window = cx
        .open_window(size(px(1440.), px(900.)), |w, cx| {
            let app = cx.new(|cx| TaskApp::new_mock(w, cx));
            app.update(cx, |a, _| {
                a.store = store.clone();
                a.expanded_sidebar_projects = store.projects.iter().map(|p| p.id).collect();
            });
            entity = Some(app.clone());
            cx.new(|cx| base::Root::new(app, w, cx))
        })
        .unwrap();
    let app = entity.unwrap();
    let handle: AnyWindowHandle = window.into();
    let mut panels = vec![("All", Scope::All), ("Recovery", Scope::Recovery)];
    if let Some(project) = store.projects.last() {
        panels.push(("Project", Scope::Project(project.id)));
    }
    if let Some(topic) = store.topics.last() {
        panels.push(("Topic", Scope::Topic(topic.id)));
    }
    let result = store
        .runs
        .iter()
        .rev()
        .find(|r| r.project_plan.is_none() && r.status == "Succeeded" && !r.output.is_empty());
    let goal = result.and_then(|r| store.owning_research_goal(r.task_id));
    if goal.is_some() {
        panels.push(("Steps", Scope::All));
    }
    if result.is_some() {
        panels.push(("Report", Scope::Runs));
    }
    for (name, scope) in panels {
        cx.update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                a.navigate(scope, w, cx);
                if name == "Steps" {
                    a.open_goal_steps(goal.unwrap(), w, cx);
                }
                if name == "Report" {
                    a.selected_run = result.map(|r| r.id);
                    a.show_run_result = true;
                    a.show_run_details = false;
                    a.reveal_selected_run();
                }
            });
            w.render_frame(cx);
        })
        .unwrap();
        let start = Instant::now();
        for _ in 0..20 {
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |_, cx| cx.notify());
                w.render_frame(cx);
            })
            .unwrap();
        }
        println!(
            "{name}: {:.2} ms/frame",
            start.elapsed().as_secs_f64() * 50.
        );
    }
}
