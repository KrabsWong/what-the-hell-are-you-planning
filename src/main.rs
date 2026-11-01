#[cfg(feature = "qa")]
mod render_qa;
#[cfg(feature = "qa")]
mod affordance_qa;
mod app;
mod archive_app;
mod appearance;
mod attachments;
mod batch;
mod batch_app;
#[cfg(feature = "qa")]
mod batch_qa;
#[cfg(feature = "qa")]
mod concurrency_qa;
mod controls;
mod domain;
mod ego;
mod execution_status;
mod goal_steps;
#[cfg(feature = "qa")]
mod intake_qa;
mod intake_views;
#[cfg(feature = "qa")]
mod material_qa;
mod navigation;
mod opencode;
mod persistence;
mod project_app;
mod project_planning;
#[cfg(feature = "qa")]
mod project_qa;
mod project_views;
#[cfg(feature = "qa")]
mod qa;
mod recovery;
#[cfg(feature = "qa")]
mod recovery_qa;
mod request_planning;
mod research;
mod research_app;
mod research_export;
#[cfg(feature = "qa")]
mod research_qa;
mod research_views;
mod run_views;
mod run_errors;
mod scheduler;
mod retries;
#[cfg(feature = "qa")]
mod retry_qa;
#[cfg(feature = "qa")]
mod topic_queue_qa;
#[cfg(feature = "qa")]
mod settings_qa;
mod settings_views;
#[cfg(feature = "qa")]
mod simplify_qa;
mod sqlite_storage;
#[cfg(feature = "qa")]
mod topic_qa;
mod topic_views;
mod ui_assets;
mod brand;
mod views;
mod workspace;
#[cfg(feature = "qa")]
mod workspace_qa;

use gpui_kit::*;

actions!(task_manager, [Quit]);

fn main() {
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-brand") { brand::verify_frames(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-attachments") { research_qa::attachments_only(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-topic-queue") { topic_queue_qa::run(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-retries") { retry_qa::run(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-render-perf") { render_qa::run(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-result-layout") {
        simplify_qa::result_layout();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-recovery") {
        recovery_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-intake") {
        intake_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-topics") {
        topic_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-calendar") {
        workspace_qa::calendar();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-onboarding") {
        workspace_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-concurrency") {
        concurrency_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-simplify") {
        simplify_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-batch") {
        batch_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-settings") {
        settings_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-affordance") {
        affordance_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-projects" || arg == "--projects-live") {
        project_qa::run(std::env::args().any(|arg| arg == "--projects-live"));
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-workspace") {
        material_qa::workspace();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--materials-live") {
        material_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--research-live") {
        qa::run_research_live();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-research") {
        research_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-live") {
        qa::run_live();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-reopen") {
        qa::reopen_live();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa") {
        qa::run();
        return;
    }
    application().with_assets(ui_assets::AppAssets).run(|cx| {
        init(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        appearance::apply(appearance::Appearance::default(), None, cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(920.)), cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1000.), px(680.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("What the hell are you planning?".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| app::TaskApp::new(window, cx)),
        )
        .expect("Could not open the application window");
        cx.activate(true);
    });
}
