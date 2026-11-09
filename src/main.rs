#![recursion_limit = "256"]
mod startup;
#[cfg(feature = "qa")]
mod render_qa;
#[cfg(feature = "qa")]
mod affordance_qa;
mod app;
mod automation;
mod automation_activity;
mod automation_execution;
mod automation_planning_state;
mod automation_policy;
mod automation_runtime;
mod automation_schedule;
mod automation_views;
mod archive_presentation;
#[cfg(feature = "qa")]
mod archive_qa;
mod archive_app;
mod appearance;
mod attachments;
mod batch;
mod batch_app;
#[cfg(feature = "qa")]
mod automation_qa;
#[cfg(feature = "qa")]
mod command_qa;
#[cfg(feature = "qa")]
mod task_edit_qa;
#[cfg(feature = "qa")]
mod batch_qa;
#[cfg(feature = "qa")]
mod concurrency_qa;
#[cfg(feature = "qa")]
mod execution_eligibility_qa;
mod controls;
mod domain;
mod dispatch_admission;
mod dispatch_intent;
#[cfg(feature = "qa")]
mod dispatch_admission_qa;
mod domain_command;
mod history_reader;
mod live_save_app;
mod scoped_domain;
mod scoped_edit;
mod task_edit_app;
mod material_app;
mod scoped_material;
mod scoped_settings;
mod material_verify;
mod command_app;
mod draft_exit;
mod display_order;
mod stop_intent;
mod ego;
mod ego_mcp;
mod execution_status;
mod execution_eligibility;
mod global_search;
#[cfg(feature = "qa")]
mod global_search_qa;
mod goal_steps;
#[cfg(feature = "qa")]
mod intake_qa;
mod intake_views;
#[cfg(feature = "qa")]
mod material_qa;
mod navigation;
mod navigation_context;
mod navigation_context_app;
mod navigation_route;
#[cfg(feature = "qa")]
mod navigation_context_qa;
#[cfg(feature = "qa")]
mod navigation_qa;
mod opencode;
mod codex;
mod persistence;
mod persistence_journal;
mod persistence_live;
mod persistence_metrics;
mod persistence_worker;
#[cfg(feature = "qa")]
mod live_performance_qa;
#[cfg(feature = "qa")]
mod live_persistence_qa;
#[cfg(feature = "qa")]
mod set_read_qa;
#[cfg(feature = "qa")]
mod persistence_qa;
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
mod result_presentation;
mod research_app;
mod research_export;
mod research_input;
mod research_input_app;
mod research_input_views;
#[cfg(feature = "qa")]
mod research_input_qa;
#[cfg(feature = "qa")]
mod t19_qa;
mod report_html;
mod report_git;
mod report_publish;
#[cfg(feature = "qa")]
mod publishing_qa;
#[cfg(feature = "qa")]
mod research_qa;
mod research_views;
#[cfg(feature = "qa")]
mod result_presentation_qa;
mod rich_markdown;
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
mod shell_surfaces;
mod settings_views;
mod model_profiles;
#[cfg(feature = "qa")]
mod simplify_qa;
#[cfg(feature = "qa")]
mod t11_qa;
mod sqlite_storage;
mod sqlite_v4;
#[cfg(feature = "qa")]
mod topic_qa;
mod topic_views;
mod ui_assets;
mod brand;
mod views;
mod workspace;
mod workflow;
mod workflow_views;
#[cfg(feature = "qa")]
mod workflow_qa;
#[cfg(feature = "qa")]
mod workspace_qa;

use gpui_kit::*;

actions!(task_manager, [Quit]);

/// Return the first argument that is not a supported macOS launch argument.
///
/// Every supported QA command returns before the guard that calls this, so the
/// only inputs that reach it are macOS/Finder/Xcode launch arguments and
/// genuine mistakes. Only two documented forms are preserved:
///
/// - Finder's process serial number, `-psn_<digits>_<digits>`.
/// - Xcode's `-NSDocumentRevisionsDebugMode YES` (or `NO`).
///
/// Everything else, including unknown single-dash switches, a bare `-`, `--`
/// and positional tokens, fails closed instead of bootstrapping a workspace.
fn unsupported_argument(args: &[String]) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        if supported_macos_launch_argument(args, &mut index) {
            continue;
        }
        return Some(args[index].clone());
    }
    None
}

fn supported_macos_launch_argument(args: &[String], index: &mut usize) -> bool {
    let argument = args[*index].as_str();
    if let Some(serial) = argument.strip_prefix("-psn_") {
        let mut parts = serial.split('_');
        let high = parts.next().unwrap_or_default();
        let low = parts.next().unwrap_or_default();
        let well_formed = !high.is_empty()
            && !low.is_empty()
            && parts.next().is_none()
            && high.bytes().all(|byte| byte.is_ascii_digit())
            && low.bytes().all(|byte| byte.is_ascii_digit());
        if well_formed {
            *index += 1;
            return true;
        }
        return false;
    }
    if argument == "-NSDocumentRevisionsDebugMode"
        && let Some(value) = args.get(*index + 1)
        && (value == "YES" || value == "NO")
    {
        *index += 2;
        return true;
    }
    false
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--ego-browser-mcp") {
        let result = std::env::args().nth(2).ok_or_else(|| "Missing ego-browser path".to_string())
            .and_then(|path| ego_mcp::serve(std::path::Path::new(&path)));
        if let Err(error) = result { eprintln!("{error}"); std::process::exit(1); }
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-board-topics") { topic_qa::board_directory(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-display-order") { workflow_qa::display_order(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-board-scroll") { workflow_qa::scroll_benchmark(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-workflow") { workflow_qa::run(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-rich-markdown") { rich_markdown::verify_frames(); return; }
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
    if std::env::args().any(|arg| arg == "--qa-result-presentation") {
        result_presentation_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-research-inputs") {
        research_input_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-keyboard-visual") {
        t19_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-recovery-scroll") { recovery_qa::scroll(); return; }
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
    if std::env::args().any(|arg| arg == "--qa-navigation-context") {
        navigation_context_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-sidebar") {
        topic_qa::sidebar();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-topic-migration") {
        topic_qa::run_migration(); return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-topic-actions") {
        topic_qa::goal_actions();
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
    if std::env::args().any(|arg| arg == "--qa-archived-research") {
        archive_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-dispatch-admission") {
        dispatch_admission_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-dispatch-crash") {
        dispatch_admission_qa::crash_child();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-concurrency") {
        concurrency_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-execution-eligibility") {
        execution_eligibility_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-simplify") {
        simplify_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-header-layout") {
        simplify_qa::header_layout();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-automation") {
        automation_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-command") {
        command_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-task-edit") {
        task_edit_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-planning") {
        automation_qa::run_planning();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-automation-perf") {
        automation_qa::performance_fixture();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-v4-crash") {
        sqlite_v4::crash_child();
        return;
    }
    #[cfg(feature = "qa")]
    if let Some(mode) = persistence_qa::requested_mode() {
        persistence_qa::run(&mode);
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-batch") {
        batch_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-publishing") { publishing_qa::run(); return; }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-settings") {
        settings_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-global-search") {
        global_search_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-affordance") {
        affordance_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-t11") {
        t11_qa::run();
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
    if std::env::args().any(|arg| arg == "--qa-handoff-progress") {
        research_qa::handoff_progress();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-research") {
        research_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-live-persistence") {
        live_persistence_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-live-performance") {
        live_performance_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa-set-read") {
        set_read_qa::run();
        return;
    }
    #[cfg(feature = "qa")]
    if std::env::args().any(|arg| arg == "--qa") {
        qa::run();
        return;
    }
    // A missing QA feature or misspelled flag must never open a real workspace.
    // Every supported route returns above, so only genuine mistakes and macOS
    // launch arguments reach this guard.
    if let Some(argument) = unsupported_argument(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!(
            "Unsupported argument {argument}. Build with --features qa and use a supported QA command. No workspace was opened."
        );
        std::process::exit(2);
    }
    application().with_assets(ui_assets::AppAssets).run(|cx| {
        init(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        appearance::apply(appearance::Appearance::default(), None, cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(920.)), cx);
        let handle = open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1000.), px(680.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("What the hell are you planning?".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: cfg!(target_os = "macos")
                        .then(|| point(px(20.), px(16.))),
                    ..Default::default()
                }),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| app::TaskApp::new(window, cx)),
        )
        .expect("Could not open the application window");
        let (quit_tx, quit_rx) = std::sync::mpsc::channel::<bool>();
        let app_entity = handle.1.clone();
        cx.on_action(move |_: &Quit, cx| {
            let _ = app_entity.update(cx, |app, cx| {
                app.begin_graceful_quit(quit_tx.clone(), cx)
            });
        });
        // Poll the completion channel on the foreground executor so the quit
        // request never blocks the UI thread. The app quits only after the
        // persistence owner reports success.
        let quit_task = cx.spawn(async move |cx| {
            let mut receiver = quit_rx;
            loop {
                match receiver.try_recv() {
                    Ok(true) => {
                        cx.update(|cx| cx.quit());
                        return;
                    }
                    Ok(false) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
        });
        quit_task.detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod argument_guard_tests {
    use super::unsupported_argument;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn rejects_unknown_application_flags() {
        for case in [
            "--live-persistence-qa",
            "--qa-typo",
            "-live-persistence-qa",
            "-bogus",
            "-bogus path",
            "-",
            "--",
        ] {
            assert!(unsupported_argument(&args(&[case])).is_some(), "{case}");
        }
        assert_eq!(
            unsupported_argument(&args(&["--live-persistence-qa", "--qa-live-persistence"])),
            Some("--live-persistence-qa".into())
        );
    }

    #[test]
    fn rejects_malformed_process_serial_numbers() {
        for case in ["-psn_", "-psn_abc", "-psn_0", "-psn_0_", "-psn__1", "-psn_0_1_2", "-psn_0_x"] {
            assert_eq!(
                unsupported_argument(&args(&[case])),
                Some(case.to_string()),
                "{case}"
            );
        }
        assert_eq!(
            unsupported_argument(&args(&["-bogus", "path"])),
            Some("-bogus".into())
        );
    }

    #[test]
    fn rejects_incomplete_document_revision_switches() {
        assert_eq!(
            unsupported_argument(&args(&["-NSDocumentRevisionsDebugMode"])),
            Some("-NSDocumentRevisionsDebugMode".into())
        );
        assert_eq!(
            unsupported_argument(&args(&["-NSDocumentRevisionsDebugMode", "MAYBE"])),
            Some("-NSDocumentRevisionsDebugMode".into())
        );
        assert_eq!(
            unsupported_argument(&args(&["-NSDocumentRevisionsDebugMode", "--qa-live-persistence"])),
            Some("-NSDocumentRevisionsDebugMode".into())
        );
    }

    #[test]
    fn preserves_documented_macos_launch_arguments() {
        assert_eq!(unsupported_argument(&args(&[])), None);
        assert_eq!(unsupported_argument(&args(&["-psn_0_12345"])), None);
        assert_eq!(
            unsupported_argument(&args(&["-NSDocumentRevisionsDebugMode", "YES"])),
            None
        );
        assert_eq!(
            unsupported_argument(&args(&["-NSDocumentRevisionsDebugMode", "NO"])),
            None
        );
        assert_eq!(
            unsupported_argument(&args(&["-psn_0_1", "-NSDocumentRevisionsDebugMode", "YES"])),
            None
        );
    }

    #[test]
    fn rejects_positional_arguments() {
        assert_eq!(
            unsupported_argument(&args(&["workspace.sqlite3"])),
            Some("workspace.sqlite3".into())
        );
    }
}
