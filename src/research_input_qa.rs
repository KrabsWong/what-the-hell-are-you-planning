//! Isolated T21 Phase 2 QA: edit-impact gate, retained-version reader, durable
//! failure retention. Offline only.

use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    domain::{Stage, Store},
    persistence::Storage,
    research::Source,
    research_input_app::PreviousVersionEntry,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, sync::Arc, time::Duration};

const PLAN: &str = r#"{"tasks":[
    {"title":"Collect A","instructions":"Collect A","kind":"collect","depends_on":[]},
    {"title":"Collect B","instructions":"Collect B","kind":"collect","depends_on":[]},
    {"title":"Analyze A","instructions":"Analyze A","kind":"analyze","depends_on":[0]},
    {"title":"Report","instructions":"Report","kind":"advise","depends_on":[2]}
]}"#;

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/research-input-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    parity_checks();
    impact_dialog_matrix(&root);
    history_dialog_matrix(&root);
    archived_history_dialog(&root);
    archived_changed_history_dialog(&root);
    long_history_matrix(&root);
    keyboard_dialogs(&root);
    failed_durable_save_keeps_pending_and_draft(&root);
    println!("Research input QA passed · {}", root.display());
}

fn workspace(directory: &Path) -> (Store, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.research_config_mut().models =
        ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
    let goal = store
        .create_research_goal(
            "Branched research".into(),
            "Prove impact and history.".into(),
            1,
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    store
        .task_mut(goal)
        .unwrap()
        .research
        .as_mut()
        .unwrap()
        .requires_approval = true;
    store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
    for id in &steps {
        let mut run = store.prepare_run(*id).unwrap();
        run.status = "Succeeded".into();
        run.output = format!("Completed report body for task {id}");
        run.sources = vec![Source {
            title: format!("Source for {id}"),
            url: format!("https://example.com/{id}"),
            published_at: "2026-11-08".into(),
        }];
        run.ended_at = Some(crate::opencode::timestamp());
        run.exit_code = Some(0);
        store.push_run(run);
        store.set_stage(*id, Stage::Done).unwrap();
    }
    store.sync_goal_progress(goal);
    (store, goal, steps)
}

fn invalidated(directory: &Path) -> (Store, u64, Vec<u64>) {
    let (mut store, goal, steps) = workspace(directory);
    store.task_mut(steps[0]).unwrap().title = "Collect A revised".into();
    store.reconcile_goal_approvals();
    assert!(!store.goal_execution_approved(goal));
    (store, goal, steps)
}

fn parity_checks() {
    let directory = std::env::temp_dir().join(format!("t21-parity-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let (store, _, steps) = invalidated(&directory);
    let sibling = steps[1];
    let sibling_run = store
        .runs()
        .iter()
        .find(|run| run.task_id == sibling)
        .unwrap()
        .clone();
    let sibling_version = crate::research_input::previous_versions(&store, sibling)
        .into_iter()
        .next()
        .unwrap();
    let sibling_entry = PreviousVersionEntry {
        task: sibling,
        task_title: store.task(sibling).unwrap().title.clone(),
        kind: crate::research::ResearchKind::Collect,
        version: sibling_version,
    };
    let history =
        crate::research_input_app::history_presentation(&store, &sibling_entry, &sibling_run);
    assert!(!history.presentation.freshness.stale_input);
    assert!(history.text().contains("Inputs unchanged"));
    let document =
        crate::research_input_app::previous_version_document(&store, &sibling_entry, &sibling_run);
    assert!(document.contains(&history.text()));
    assert!(!document.contains("Stale input"));
    assert!(document.contains("## References"));
    let _ = std::fs::remove_dir_all(&directory);
}

fn impact_dialog_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            impact_dialog(root, mode, width, height);
        }
    }
}

fn impact_dialog(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("impact-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = workspace(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(steps[0], w, cx);
        app.begin_draft(steps[0], w, cx);
        let title = app.draft.as_ref().unwrap().title.clone();
        title.update(cx, |state, cx| state.set_value("Collect A revised", w, cx));
        cx.notify();
    });
    let baseline = cx.update(|cx| app.read(cx).store.clone());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.find("research-input-note");
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        assert!(w.try_find("research-save-impact").is_some());
        assert!(app.read(cx).research_save_is_pending());
        assert_eq!(
            app.read(cx).store,
            baseline,
            "open review must not mutate the store"
        );
        assert!(w.find("research-save-impact-cancel").visible());
        assert!(w.find("research-save-impact-confirm").visible());
    })
    .unwrap();
    // Evidence is captured while the reviewed dialog is still open.
    screenshot(
        &mut cx,
        handle,
        &directory,
        &format!("impact-dialog-{mode:?}-{width}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "research-save-impact", "research-save-impact-cancel");
        w.render_frame(cx);
        assert!(w.try_find("research-save-impact").is_none());
        let app = app.read(cx);
        let draft = app.draft.as_ref().expect("Cancel keeps the source draft");
        assert_eq!(
            draft.title.read(cx).value(),
            "Collect A revised",
            "Cancel keeps the edited text"
        );
        assert_eq!(app.selected(), Some(steps[0]), "Cancel keeps the route");
        assert_eq!(app.store, baseline, "Cancel leaves the store unchanged");
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        click_in(
            w,
            cx,
            "research-save-impact",
            "research-save-impact-confirm",
        );
        w.render_frame(cx);
        assert!(!app.read(cx).research_save_is_pending());
        assert!(!app.read(cx).store.goal_execution_approved(goal));
        assert_eq!(
            app.read(cx).selected(),
            Some(steps[0]),
            "confirmed save stays on the edited step"
        );
        assert_eq!(app.read(cx).scope(), Scope::All);
        assert!(app.read(cx).draft.is_none());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn history_dialog_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            history_dialog(root, mode, width, height);
        }
    }
}

fn history_dialog(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("history-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = invalidated(&directory);
    store.set_appearance(mode);
    let sibling_run = store
        .runs()
        .iter()
        .find(|run| run.task_id == steps[1])
        .unwrap()
        .id;
    let changed_run = store
        .runs()
        .iter()
        .find(|run| run.task_id == steps[0])
        .unwrap()
        .id;
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(goal, w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        w.find("previous-versions");
        assert_eq!(app.read(cx).previous_version_entries(goal).len(), 4);
        let sibling = w
            .find(format!("previous-version-row-{sibling_run}"))
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(sibling.contains("Inputs unchanged · retained"), "{sibling}");
        let changed = w
            .find(format!("previous-version-row-{changed_run}"))
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(changed.contains("Previous input revision"), "{changed}");

        // Select the changed-input report through its actual row, then prove the
        // reader, Copy and Export builder all use the same adapted presentation.
        click_in(
            w,
            cx,
            "previous-version-list",
            &format!("previous-version-row-{changed_run}"),
        );
        w.render_frame(cx);
        assert_eq!(app.read(cx).previous_versions_run, Some(changed_run));
        let expected = app.read(cx).previous_version_reader(changed_run).unwrap();
        assert!(
            expected.contains("Previous input revision")
                || expected.contains("earlier input revision"),
            "{expected}"
        );
        click_in(w, cx, "previous-versions", "previous-version-copy");
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        assert_eq!(copied, expected, "Copy must match the rendered reader text");

        let entry = app.read(cx).previous_version_entry(changed_run).unwrap();
        let run = app
            .read(cx)
            .store
            .runs()
            .iter()
            .find(|run| run.id == changed_run)
            .unwrap()
            .clone();
        let document =
            crate::research_input_app::previous_version_document(&app.read(cx).store, &entry, &run);
        assert!(document.contains(&expected));
        assert!(document.contains("## References"));
        w.find("previous-version-more");
    })
    .unwrap();
    // Evidence is captured with the dialog and selected reader still open.
    screenshot(
        &mut cx,
        handle,
        &directory,
        &format!("history-dialog-{mode:?}-{width}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "previous-versions", "previous-version-close");
        w.render_frame(cx);
        assert!(w.try_find("previous-versions").is_none());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn archived_history_dialog(root: &Path) {
    let directory = root.join("history-archived-Light-1440");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = workspace(&directory);
    store.set_appearance(Appearance::Light);
    store.archive(goal).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(goal, w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        w.find("previous-versions");
        let entries = app.read(cx).previous_version_entries(goal);
        assert_eq!(entries.len(), steps.len());
        assert!(
            entries
                .iter()
                .all(|entry| !entry.version.execution_admissible)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.version.revision_digest.is_some()),
            "archived history keeps original revisions"
        );
        assert!(
            w.try_find("research-all").is_none(),
            "no run controls in archived history"
        );
        assert!(w.try_find("previous-version-copy").is_some());
        assert!(w.try_find("previous-version-more").is_some());
        let facts = w
            .find("previous-version-facts")
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(!facts.contains("Stale input"), "{facts}");
        let selected = app.read(cx).previous_versions_run.unwrap();
        let text = app.read(cx).previous_version_reader(selected).unwrap();
        assert!(!text.contains("Stale input"), "{text}");
        click_in(w, cx, "previous-versions", "previous-version-copy");
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(text)
        );
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        &directory,
        "archived-history-Light-1440.png",
    );
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "previous-versions", "previous-version-close");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn archived_changed_history_dialog(root: &Path) {
    let directory = root.join("history-archived-changed-Light-1440");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = workspace(&directory);
    store.set_appearance(Appearance::Light);
    let changed_run = store
        .runs()
        .iter()
        .find(|run| run.task_id == steps[0])
        .unwrap()
        .id;
    store.task_mut(steps[0]).unwrap().title = "Collect A revised after archive check".into();
    store.reconcile_goal_approvals();
    store.archive(goal).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1440., 920., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(goal, w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        click_in(
            w,
            cx,
            "previous-version-list",
            &format!("previous-version-row-{changed_run}"),
        );
        w.render_frame(cx);
        let facts = w
            .find("previous-version-facts")
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert_eq!(app.read(cx).previous_versions_run, Some(changed_run));
        assert!(facts.contains("Stale input"), "{facts}");
        let text = app.read(cx).previous_version_reader(changed_run).unwrap();
        assert!(text.contains("Stale input"), "{text}");
        assert!(text.contains("Archived history"), "{text}");
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        &directory,
        "archived-history-changed-Light-1440.png",
    );
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "previous-versions", "previous-version-close");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn long_title_english() -> &'static str {
    "Collect primary-source regulatory filings, reconcile the audited annual revenue tables, and explain every restatement that affects the investment conclusion"
}

fn long_title_cjk() -> &'static str {
    "核对主要来源的监管申报文件并逐项解释经审计年度收入表中的重述项目及其对投资结论的影响"
}

fn long_report() -> String {
    let mut report = String::from("# Long retained report\n\n");
    for index in 0..80 {
        report.push_str(&format!(
            "Paragraph {index}: the retained report keeps enough evidence detail to force the reader body beyond one viewport while Close, Copy and Export remain reachable.\n\n"
        ));
    }
    report
}

fn long_workspace(directory: &Path) -> (Store, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.project_at_mut(0).unwrap().directory = directory.to_string_lossy().into();
    store.research_config_mut().models =
        ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
    let goal = store
        .create_research_goal(
            "Long-title retained versions".into(),
            "Layout proof.".into(),
            1,
        )
        .unwrap();
    let steps = store.apply_research_plan(goal, PLAN).unwrap();
    store.task_mut(steps[0]).unwrap().title = long_title_english().into();
    store.task_mut(steps[1]).unwrap().title = long_title_cjk().into();
    store
        .task_mut(goal)
        .unwrap()
        .research
        .as_mut()
        .unwrap()
        .requires_approval = true;
    store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
    for id in &steps {
        let mut run = store.prepare_run(*id).unwrap();
        run.status = "Succeeded".into();
        run.output = if *id == steps[0] {
            long_report()
        } else {
            format!("Completed report body for task {id}")
        };
        run.ended_at = Some(crate::opencode::timestamp());
        run.exit_code = Some(0);
        store.push_run(run);
        store.set_stage(*id, Stage::Done).unwrap();
    }
    store.sync_goal_progress(goal);
    let base = store
        .runs()
        .iter()
        .find(|run| run.task_id == steps[0])
        .unwrap()
        .clone();
    for (index, signature) in ["older-revision-a", "older-revision-b"].iter().enumerate() {
        let mut attempt = base.clone();
        attempt.id = 20_000 + index as u64;
        attempt.started_at = format!("2026-11-0{} 09:00:00 +08:00", index + 1);
        attempt.input_signature = (*signature).into();
        store.push_run(attempt);
    }
    (store, goal, steps)
}

fn long_history_matrix(root: &Path) {
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        for mode in [Appearance::Light, Appearance::Dark] {
            long_history_dialog(root, mode, width, height);
        }
    }
}

fn long_history_dialog(root: &Path, mode: Appearance, width: f32, height: f32) {
    let directory = root.join(format!("long-history-{mode:?}-{width}"));
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = long_workspace(&directory);
    store.set_appearance(mode);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let older_a = 20_000u64;
    let older_b = 20_001u64;
    let (mut cx, handle, app) = with_app(store, &path, width, height, |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(goal, w, cx);
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        w.find("previous-versions");
        assert_eq!(app.read(cx).previous_version_entries(goal).len(), 6);

        let row_a = format!("previous-version-row-{older_a}");
        let bounds = w.find(row_a.clone()).bounds();
        let row_height = bounds.bottom() - bounds.top();
        assert!(
            row_height > px(44.),
            "long title must wrap instead of clipping: {row_height}"
        );
        let label = w
            .find(row_a.clone())
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(label.contains(long_title_english()), "{label}");

        let cjk_run = app
            .read(cx)
            .store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[1])
            .unwrap()
            .id;
        let cjk_row = format!("previous-version-row-{cjk_run}");
        click_in(w, cx, "previous-version-list", &cjk_row);
        w.render_frame(cx);
        assert_eq!(app.read(cx).previous_versions_run, Some(cjk_run));
        let cjk_label = w
            .find(cjk_row.clone())
            .label()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(cjk_label.contains(long_title_cjk()), "{cjk_label}");
        assert!(
            w.find(cjk_row.clone()).bounds().bottom() - w.find(cjk_row.clone()).bounds().top()
                > px(44.)
        );

        click_in(w, cx, "previous-version-list", &row_a);
        w.render_frame(cx);
        assert_eq!(app.read(cx).previous_versions_run, Some(older_a));
        let first = app.read(cx).previous_version_reader(older_a).unwrap();
        assert!(first.contains("Run #20000") || first.contains("Long retained report"));
        let entry = app.read(cx).previous_version_entry(older_a).unwrap();
        let run = app
            .read(cx)
            .store
            .runs()
            .iter()
            .find(|run| run.id == older_a)
            .unwrap()
            .clone();
        let first_document =
            crate::research_input_app::previous_version_document(&app.read(cx).store, &entry, &run);
        assert!(
            first_document.contains("Run #20000"),
            "export proves selected run"
        );
        assert_eq!(entry.task, steps[0]);

        click_in(
            w,
            cx,
            "previous-version-list",
            &format!("previous-version-row-{older_b}"),
        );
        w.render_frame(cx);
        assert_eq!(app.read(cx).previous_versions_run, Some(older_b));
        let second_entry = app.read(cx).previous_version_entry(older_b).unwrap();
        let second_run = app
            .read(cx)
            .store
            .runs()
            .iter()
            .find(|run| run.id == older_b)
            .unwrap()
            .clone();
        let second_document = crate::research_input_app::previous_version_document(
            &app.read(cx).store,
            &second_entry,
            &second_run,
        );
        assert!(
            second_document.contains("Run #20001"),
            "distinct attempt provenance"
        );
        assert_ne!(first_document, second_document);

        assert!(w.find("previous-version-close").visible());
        assert!(w.find("previous-version-copy").visible());
        assert!(w.find("previous-version-more").visible());
        let dialog = w.find("previous-versions").bounds();
        let export = w.find("previous-version-more").bounds();
        assert!(
            export.bottom() <= dialog.bottom() - px(1.),
            "export footer stays inside modal: export {} dialog {}",
            export.bottom(),
            dialog.bottom()
        );
        assert!(
            export.bottom() <= w.viewport_size().height,
            "export stays in window"
        );
        w.click(format!("previous-version-report-{older_b}"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let text_state = app
            .read(cx)
            .previous_version_text
            .clone()
            .expect("owned reader state");
        let (before_px, max_px, before_item, item_count) = {
            let list = text_state.read(cx).list_state();
            (
                list.scroll_px_offset_for_scrollbar().y,
                list.max_offset_for_scrollbar().y,
                list.logical_scroll_top().item_ix,
                list.item_count(),
            )
        };
        assert!(item_count > 0, "markdown blocks parsed: {item_count}");
        assert!(max_px > px(0.), "reader must have scrollable extent: {max_px}");
        w.hover(format!("previous-version-report-{older_b}"), cx);
        w.scroll(
            format!("previous-version-report-{older_b}"),
            ScrollDelta::Pixels(point(px(0.), px(-420.))),
            cx,
        );
        w.render_frame(cx);
        w.render_frame(cx);
        let after_px = text_state.read(cx).list_state().scroll_px_offset_for_scrollbar().y;
        assert!(
            after_px < before_px && after_px < px(0.),
            "internal reader offset must move: item {before_item}, px {before_px}->{after_px}, max {max_px}"
        );
        w.render_frame(cx);
        let after_second = text_state.read(cx).list_state().scroll_px_offset_for_scrollbar().y;
        assert_eq!(after_second, after_px, "scroll offset must remain stable");
        assert!(w.find("previous-version-close").visible());
        assert!(w.find("previous-version-copy").visible());
        assert!(w.find("previous-version-more").visible());
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        &directory,
        &format!("long-history-dialog-{mode:?}-{width}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("previous-version-more", cx);
        w.render_frame(cx);
    })
    .unwrap();
    screenshot(
        &mut cx,
        handle,
        &directory,
        &format!("history-more-{mode:?}-{width}.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.press("escape", cx);
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| app.select_previous_version(older_a, cx));
        w.render_frame(cx);
        let state = app.read(cx).previous_version_text.clone().unwrap();
        let offset = state
            .read(cx)
            .list_state()
            .scroll_px_offset_for_scrollbar()
            .y;
        assert_eq!(offset, px(0.), "version change resets reader offset");
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        click_in(w, cx, "previous-versions", "previous-version-close");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn focus_inside(w: &mut Window, scope: &'static str) -> bool {
    w.within(scope)
        .elements()
        .iter()
        .any(|element| element.focused() == Some(true))
}

fn keyboard_dialogs(root: &Path) {
    let directory = root.join("keyboard-dialogs");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = workspace(&directory);
    store.set_appearance(Appearance::Light);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.set_scope_identity(Scope::All);
        app.open_task(steps[0], w, cx);
        app.begin_draft(steps[0], w, cx);
        let title = app.draft.as_ref().unwrap().title.clone();
        title.update(cx, |state, cx| {
            state.set_value("Keyboard impact edit", w, cx)
        });
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let opener = w.focused(cx);
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        let modal = app.read(cx).dialog_first_focus.clone();
        assert_eq!(
            w.focused(cx),
            Some(modal.clone()),
            "focus enters impact Cancel control"
        );
        w.press("space", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("research-save-impact").is_none(),
            "Space activates focused Cancel"
        );
        assert!(app.read(cx).draft.is_some());
        assert_eq!(w.focused(cx), opener, "Space Cancel returns the opener");
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        assert_eq!(
            w.focused(cx),
            Some(modal.clone()),
            "impact reopens on Cancel"
        );
        for _ in 0..10 {
            w.press("tab", cx);
            w.render_frame(cx);
            assert!(
                w.focused(cx) == Some(modal.clone()) || focus_inside(w, "research-save-impact"),
                "Tab cycle leaves the impact controls"
            );
        }
        for _ in 0..10 {
            w.press("shift-tab", cx);
            w.render_frame(cx);
            assert!(
                w.focused(cx) == Some(modal.clone()) || focus_inside(w, "research-save-impact"),
                "Shift-Tab cycle leaves the impact controls"
            );
        }
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("research-save-impact").is_none());
        assert_eq!(w.focused(cx), opener, "Escape returns the opener");
        assert!(app.read(cx).draft.is_some(), "Escape cancels review only");
        assert_eq!(app.read(cx).selected(), Some(steps[0]));
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        // Close the still-open editor before testing the history modal.
        w.click("discard-draft", cx);
        w.render_frame(cx);
        app.update(cx, |app, cx| app.open_task(goal, w, cx));
        w.render_frame(cx);
        let opener = w.focused(cx);
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        let modal = app.read(cx).dialog_first_focus.clone();
        assert_eq!(
            w.focused(cx),
            Some(modal.clone()),
            "focus enters history Close control"
        );
        w.press("enter", cx);
        w.render_frame(cx);
        assert!(
            w.try_find("previous-versions").is_none(),
            "Enter activates focused Close"
        );
        assert_eq!(w.focused(cx), opener, "Enter Close returns the opener");
        assert_eq!(app.read(cx).selected(), Some(goal));
        w.click("previous-versions-open", cx);
        w.render_frame(cx);
        assert_eq!(
            w.focused(cx),
            Some(modal.clone()),
            "history reopens on Close"
        );
        for _ in 0..12 {
            w.press("tab", cx);
            w.render_frame(cx);
            assert!(
                w.focused(cx) == Some(modal.clone()) || focus_inside(w, "previous-versions"),
                "Tab cycle leaves the history controls"
            );
        }
        for _ in 0..12 {
            w.press("shift-tab", cx);
            w.render_frame(cx);
            assert!(
                w.focused(cx) == Some(modal.clone()) || focus_inside(w, "previous-versions"),
                "Shift-Tab cycle leaves the history controls"
            );
        }
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("previous-versions").is_none());
        assert_eq!(w.focused(cx), opener, "Escape returns history opener");
        assert_eq!(app.read(cx).selected(), Some(goal));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn failed_durable_save_keeps_pending_and_draft(root: &Path) {
    let directory = root.join("save-failure");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, steps) = workspace(&directory);
    store.set_appearance(Appearance::Light);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, 1000., 680., |app, w, cx| {
        app.storage = None;
        app.set_scope_identity(Scope::All);
        app.open_task(steps[0], w, cx);
        app.begin_draft(steps[0], w, cx);
        let title = app.draft.as_ref().unwrap().title.clone();
        title.update(cx, |state, cx| {
            state.set_value("Retain this draft on failure", w, cx)
        });
        cx.notify();
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        click_in(
            w,
            cx,
            "research-save-impact",
            "research-save-impact-confirm",
        );
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.storage.is_none());
        assert!(!app.error.is_empty());
        assert!(
            app.research_save_is_pending(),
            "pending review survives failure"
        );
        assert!(app.draft.is_some(), "source draft survives failure");
        assert!(w.try_find("research-save-impact").is_some());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
    let _ = std::fs::remove_dir_all(&directory);
}

fn click_in(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    scroll_to(w, cx, scroll, id);
    w.click(id.to_string(), cx);
}

fn scroll_to(w: &mut Window, cx: &mut App, scroll: &str, id: &str) {
    w.render_frame(cx);
    for _ in 0..40 {
        let target = w.find(id.to_string());
        if target.visible() {
            let bounds = target.bounds();
            let viewport_top = px(110.);
            let viewport_bottom = w.viewport_size().height - px(40.);
            if bounds.top() >= viewport_top && bounds.bottom() <= viewport_bottom {
                return;
            }
        }
        w.scroll(
            scroll.to_string(),
            ScrollDelta::Pixels(point(px(0.), px(-160.))),
            cx,
        );
        w.render_frame(cx);
    }
    assert!(w.find(id.to_string()).visible(), "Could not scroll to {id}");
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
