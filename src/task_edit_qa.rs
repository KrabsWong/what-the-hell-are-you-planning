//! T13 S3b isolated task-editor QA: real save/confirm/retry and scoped Undo.
//! Headless, offline, no model or external process.

use crate::app::{Scope, TaskApp};
use crate::appearance::Appearance;
use crate::batch::{BatchState, TodoBatch};
use crate::domain::{Priority, Stage, Store};
use crate::persistence::Storage;
use crate::research::Source;
use gpui_kit::{test::TestWindowExt, *};
use sha2::Digest;
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
        .join(format!(".local/task-edit-qa/{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    completed_metadata_and_title_undo(&root);
    combined_save_and_stage_undo(&root);
    dirty_save_waits_for_confirmation(&root);
    comment_save_and_undo(&root);
    comment_failed_save_retry(&root);
    comment_footer_retry_new_impact(&root);
    comment_retry_cross_task_cache(&root);
    stage_property_and_undo(&root);
    reopen_step_and_undo(&root);
    material_add_remove_undo_ui(&root);
    material_failure_cancel_and_retry(&root);
    material_completed_confirm_failed_undo_and_retry(&root);
    material_stale_remove_verification(&root);
    material_background_tamper_boundary(&root);
    material_workspace_switch_during_verify(&root);
    confirm_retry_preserves_unrelated_fields(&root);
    confirm_retry_conflicts_on_owned_field(&root);
    confirm_reviews_new_impact(&root);
    pending_batch_rejects_stage(&root);
    scoped_undo_with_unrelated_work(&root);
    scoped_undo_blocks_on_relevant_run(&root);
    automation_barrier_keeps_scoped_undo(&root);
    mixed_legacy_scoped_order(&root);
    println!("Task edit QA passed · {}", root.display());
}

fn workspace(directory: &Path) -> (Store, u64, Vec<u64>) {
    let mut store = Store::empty();
    store.projects[0].directory = directory.to_string_lossy().into();
    store.research_config.models =
        ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
    let goal = store
        .create_research_goal(
            "Branched research".into(),
            "Prove scoped save and undo.".into(),
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

fn with_app<F>(
    store: Store,
    path: &Path,
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
        .open_window(size(px(1100.), px(760.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |app, cx| {
                app.attach_storage(Storage::new(path.to_path_buf()), w, cx);
                assert!(app.storage_error.is_empty(), "{}", app.storage_error);
                app.store = store;
                app.set_appearance(Appearance::Light, w, cx);
                app.scope = Scope::All;
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

fn begin_editor(app: &mut TaskApp, task: u64, w: &mut Window, cx: &mut Context<TaskApp>) {
    app.open_task(task, w, cx);
    app.begin_draft(task, w, cx);
}

fn set_title(app: &mut TaskApp, value: &str, w: &mut Window, cx: &mut Context<TaskApp>) {
    let title = app.draft.as_ref().unwrap().title.clone();
    title.update(cx, |state, cx| state.set_value(value, w, cx));
}

fn set_labels(app: &mut TaskApp, value: &str, w: &mut Window, cx: &mut Context<TaskApp>) {
    let labels = app.draft.as_ref().unwrap().labels.clone();
    labels.update(cx, |state, cx| state.set_value(value, w, cx));
}

fn completed_metadata_and_title_undo(root: &Path) {
    let directory = root.join("completed-und");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let runs: Vec<_> = store.runs().to_vec();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.begin_draft(goal, w, cx);
        set_labels(app, "Metadata only", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().stage, Stage::Done);
        assert_eq!(app.store.runs().to_vec(), runs);
        assert!(app.store.research_result(steps[0]).is_some());
        assert!(app.draft.is_none());
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().labels, vec!["Metadata only".to_string()]);
        assert_eq!(loaded.task(goal).unwrap().stage, Stage::Done);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "metadata-save.png");

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            begin_editor(app, goal, w, cx);
            set_title(app, "Completed title edit", w, cx);
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let pending = app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
            app.research_save_is_pending()
        });
        assert!(pending);
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().stage, Stage::Backlog);
        assert!(app.draft.is_none());
        assert_eq!(
            app.undo.len(),
            2,
            "metadata and title edits are two chronological scoped entries"
        );
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().title, "Completed title edit");
        assert_eq!(loaded.task(goal).unwrap().stage, Stage::Backlog);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "completed-title-saved.png");
    // Exercise the real footer Undo button, not only the direct seam.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (stage, title, labels, results, runs_after, undo_len) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.store.task(goal).unwrap().stage,
            app.store.task(goal).unwrap().title.clone(),
            app.store.task(goal).unwrap().labels.clone(),
            steps
                .iter()
                .filter(|id| app.store.research_result(**id).is_some())
                .count(),
            app.store.runs().to_vec(),
            app.undo.len(),
        )
    });
    assert_eq!(stage, Stage::Done);
    assert_eq!(title, "Branched research");
    assert_eq!(
        labels,
        vec!["Metadata only".to_string()],
        "title undo must not consume the older metadata entry"
    );
    assert_eq!(results, steps.len());
    assert_eq!(runs_after, runs);
    assert_eq!(undo_len, 1, "the metadata entry remains");
    let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
    assert_eq!(loaded.task(goal).unwrap().stage, Stage::Done);
    assert_eq!(loaded.task(goal).unwrap().title, "Branched research");
    assert_eq!(loaded.task(goal).unwrap().labels, vec!["Metadata only".to_string()]);
    assert_eq!(loaded.runs(), runs);

    // Second Undo restores only the earlier metadata edit.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let (labels, title, stage) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.store.task(goal).unwrap().labels.clone(),
            app.store.task(goal).unwrap().title.clone(),
            app.store.task(goal).unwrap().stage,
        )
    });
    assert_eq!(labels, vec!["Research".to_string()]);
    assert_eq!(title, "Branched research");
    assert_eq!(stage, Stage::Done);
    let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
    assert_eq!(loaded.task(goal).unwrap().labels, vec!["Research".to_string()]);
    screenshot(&mut cx, handle, &directory, "completed-title-undone.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn combined_save_and_stage_undo(root: &Path) {
    let directory = root.join("combined-save");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store
        .create_task("Normal task", 1, Stage::Planned, None)
        .unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, task, w, cx);
        set_title(app, "Combined task", w, cx);
        set_labels(app, "One, Two", w, cx);
        app.draft.as_mut().unwrap().stage = Stage::Backlog;
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "draft-scroll", "save-task");
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().title, "Combined task");
        assert_eq!(
            app.store.task(task).unwrap().labels,
            vec!["One".to_string(), "Two".to_string()]
        );
        assert_eq!(app.store.task(task).unwrap().stage, Stage::Backlog);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().title, "Combined task");
        assert_eq!(loaded.task(task).unwrap().stage, Stage::Backlog);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "combined-saved.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().title, "Normal task");
        assert!(app.store.task(task).unwrap().labels.is_empty());
        assert_eq!(app.store.task(task).unwrap().stage, Stage::Planned);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().title, "Normal task");
        assert_eq!(loaded.task(task).unwrap().stage, Stage::Planned);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn dirty_save_waits_for_confirmation(root: &Path) {
    let directory = root.join("dirty-save");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.scope = Scope::All;
        begin_editor(app, goal, w, cx);
        set_title(app, "Dirty exit title", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.press("escape", cx);
        w.render_frame(cx);
        assert!(w.try_find("dirty-exit-save").is_some());
        w.click("dirty-exit-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.research_save_is_pending(), "review opens before exit");
        assert!(app.draft.is_some(), "the draft is retained during review");
        assert!(
            app.navigation.after_save_target.is_some(),
            "the intended destination is pending, not executed"
        );
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "dirty-exit-review.png");
    cx.update_window(handle, |_, w, cx| {
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.draft.is_none());
        assert!(app.navigation.after_save_target.is_none());
        assert_eq!(app.store.task(goal).unwrap().title, "Dirty exit title");
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().title, "Dirty exit title");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn comment_save_and_undo(root: &Path) {
    let directory = root.join("comment-und");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let runs: Vec<_> = store.runs().to_vec();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(goal, w, cx);
        app.show_task_activity = true;
        app.comment
            .update(cx, |state, cx| state.set_value("Add this evidence", w, cx));
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "post-comment");
        w.render_frame(cx);
        assert!(
            app.read(cx).research_save_is_pending(),
            "completed-goal comment opens T21 review"
        );
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().comments.len(), 1);
        assert_eq!(app.store.task(goal).unwrap().stage, Stage::Backlog);
        assert_eq!(app.store.runs().to_vec(), runs);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().comments.len(), 1);
        assert_eq!(loaded.task(goal).unwrap().stage, Stage::Backlog);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "comment-saved.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.store.task(goal).unwrap().comments.is_empty());
        assert_eq!(app.store.task(goal).unwrap().stage, Stage::Done);
        assert_eq!(app.store.runs().to_vec(), runs);
        assert!(steps.iter().all(|id| app.store.research_result(*id).is_some()));
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert!(loaded.task(goal).unwrap().comments.is_empty());
        assert_eq!(loaded.task(goal).unwrap().stage, Stage::Done);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "comment-undone.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn comment_failed_save_retry(root: &Path) {
    let directory = root.join("comment-retry");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let activity_before = store.task(goal).unwrap().activity.len();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(goal, w, cx);
        app.show_task_activity = true;
        app.comment
            .update(cx, |state, cx| state.set_value("Retry this comment", w, cx));
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = None);
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "post-comment");
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.pending_comment_edit.is_some());
        assert!(app.research_save_is_pending());
        assert_eq!(app.store.task(goal).unwrap().comments.len(), 0);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().comments.len(), 0);
        app.qa_storage_writes.set(0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.store.task_mut(steps[1]).unwrap().title = "Sibling changed after failure".into();
            app.store.reconcile_goal_approvals();
            app.comment
                .update(cx, |state, cx| state.set_value("Newer local draft", w, cx));
            app.storage = Some(Storage::new(path.clone()));
            app.persist_if_changed(true);
        });
        let before = app.read(cx).research_save_impact.as_ref().unwrap().impact.clone();
        // The visible confirmation button re-reviews the changed impact first.
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.research_save_is_pending());
        assert_eq!(app.store.task(goal).unwrap().comments.len(), 0);
        let reviewed = app.research_save_impact.as_ref().unwrap().impact.clone();
        assert_ne!(reviewed, before, "changed impact must be shown before writing");
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().comments.len(), 0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().comments.len(), 1);
        assert_eq!(
            app.store.task(goal).unwrap().comments[0].body,
            "Retry this comment"
        );
        assert_eq!(app.qa_storage_writes.get(), 1);
        assert_eq!(app.undo.len(), 1);
        assert_eq!(
            app.store.task(goal).unwrap().activity.len(),
            activity_before + 2
        );
        assert_eq!(
            app.comment.read(cx).value(),
            "Newer local draft",
            "a newer unsaved comment draft survives the old-body retry"
        );
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().comments.len(), 1);
        assert_eq!(loaded.task(goal).unwrap().comments[0].body, "Retry this comment");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn comment_footer_retry_new_impact(root: &Path) {
    let directory = root.join("comment-footer-retry");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store
        .create_task("Ordinary comment task", 1, Stage::Planned, None)
        .unwrap();
    let activity_before = store.task(task).unwrap().activity.len();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task, w, cx);
        app.show_task_activity = true;
        app.comment
            .update(cx, |state, cx| state.set_value("Footer retry comment", w, cx));
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = None);
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "post-comment");
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(
            !app.research_save_is_pending(),
            "an ordinary-task comment needs no T21 review"
        );
        assert!(app.pending_comment_edit.is_some());
        assert_eq!(app.store.task(task).unwrap().comments.len(), 0);
        app.qa_storage_writes.set(0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            let goal = app
                .store
                .create_research_goal("Converted goal".into(), "Brief".into(), 1)
                .unwrap();
            {
                let task_state = app.store.task_mut(task).unwrap();
                task_state.parent_id = Some(goal);
                task_state.research = Some(crate::research::ResearchTask {
                    requires_approval: false,
                    kind: crate::research::ResearchKind::Collect,
                    goal_id: goal,
                    depends_on: vec![],
                    space_name: String::new(),
                });
            }
            app.store
                .task_mut(goal)
                .unwrap()
                .research
                .as_mut()
                .unwrap()
                .requires_approval = true;
            app.store.task_mut(goal).unwrap().approved_plan =
                app.store.goal_plan_signature(goal);
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending comment save".into();
        });
        w.render_frame(cx);
        // Footer Retry is legitimately visible: the failed save had no modal.
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(
            app.research_save_is_pending(),
            "converted research context now requires review"
        );
        assert_eq!(app.store.task(task).unwrap().comments.len(), 0);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().comments.len(), 0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().comments.len(), 1);
        assert_eq!(app.qa_storage_writes.get(), 1);
        assert_eq!(app.undo.len(), 1);
        assert_eq!(
            app.store.task(task).unwrap().activity.len(),
            activity_before + 1,
            "one comment-added activity line"
        );
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().comments.len(), 1);
        assert_eq!(loaded.task(task).unwrap().comments[0].body, "Footer retry comment");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn comment_retry_cross_task_cache(root: &Path) {
    let directory = root.join("comment-cross-task");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task_a = store.create_task("Comment task A", 1, Stage::Planned, None).unwrap();
    let task_b = store.create_task("Comment task B", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task_a, w, cx);
        app.show_task_activity = true;
        app.comment
            .update(cx, |state, cx| state.set_value("Cache this comment", w, cx));
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = None);
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "post-comment");
        w.render_frame(cx);
        assert!(app.read(cx).pending_comment_edit.is_some());
        assert!(!app.read(cx).research_save_is_pending());
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.open_task(task_b, w, cx);
            app.show_task_activity = true;
            app.comment
                .update(cx, |state, cx| state.set_value("B draft", w, cx));
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending comment save".into();
        });
        w.render_frame(cx);
        assert_eq!(
            app.read(cx).navigation.comments.get(task_a),
            Some("Cache this comment")
        );
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task_a).unwrap().comments.len(), 1);
        assert_eq!(app.store.task(task_b).unwrap().comments.len(), 0);
        assert!(app.navigation.comments.get(task_a).is_none());
        assert_eq!(
            app.comment.read(cx).value(),
            "B draft",
            "another task's live editor is preserved"
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| app.open_task(task_a, w, cx));
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(
            app.comment.read(cx).value(),
            "",
            "the saved origin draft does not reappear"
        );
        assert_eq!(app.store.task(task_a).unwrap().comments.len(), 1);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn stage_property_and_undo(root: &Path) {
    let directory = root.join("stage-property");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store.create_task("Stage task", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task, w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "property-Stage");
        w.render_frame(cx);
        w.click("set-backlog", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().stage, Stage::Backlog);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().stage, Stage::Backlog);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "stage-property-backlog.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().stage, Stage::Planned);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().stage, Stage::Planned);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn reopen_step_and_undo(root: &Path) {
    let directory = root.join("reopen-step");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, steps) = workspace(&directory);
    let child = steps[0];
    store.set_stage(child, Stage::Cancelled).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(child, w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        click_in(w, cx, "detail-scroll", "step-reopen");
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(child).unwrap().stage, Stage::Planned);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(child).unwrap().stage, Stage::Planned);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "reopen-step.png");
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(child).unwrap().stage, Stage::Cancelled);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(child).unwrap().stage, Stage::Cancelled);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn settle_material_jobs(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
) {
    for _ in 0..300 {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                app.read(cx).pending_material_job.is_none()
            })
            .unwrap();
        if done {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("material verification/cleanup did not settle");
}

fn material_add_remove_undo_ui(root: &Path) {
    let directory = root.join("material-ui");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store.create_task("Material task", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let source_a = directory.join("material-a.pdf");
    let source_b = directory.join("material-b.pdf");
    std::fs::write(&source_a, b"%PDF-1.4\nmaterial A\n").unwrap();
    std::fs::write(&source_b, b"%PDF-1.4\nmaterial B\n").unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task, w, cx);
    });
    for source in [&source_a, &source_b] {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |app, cx| {
                app.import_attachments(task, vec![source.to_path_buf()], cx);
            });
        })
        .unwrap();
        let mut waited = 0;
        loop {
            let done = cx
                .update_window(handle, |_, _, cx| {
                    app.update(cx, |app, cx| app.poll_runtime(cx));
                    let app = app.read(cx);
                    app.attachment_import.is_none() && app.pending_material_job.is_none()
                })
                .unwrap();
            if done {
                break;
            }
            waited += 1;
            assert!(waited < 300, "material import did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 2);
        assert_eq!(app.undo.len(), 2);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 2);
    })
    .unwrap();
    // Real material menu -> Remove from goal.
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("material-menu-0", cx);
        w.render_frame(cx);
        assert_eq!(
            w.find("material-remove-0").label(),
            Some("Remove from goal")
        );
        w.click("material-remove-0", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert_eq!(app.undo.len(), 3);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 1);
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "material-removed.png");
    // Undo remove restores metadata and does not fabricate bytes.
    cx.update_window(handle, |_, w, cx| {
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        assert_eq!(app.read(cx).store.task(task).unwrap().attachments.len(), 2);
    })
    .unwrap();
    // A retained run references B, so undo of the B add must keep its bytes.
    let (attachment_b, data_dir) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.store.task(task).unwrap().attachments[1].clone(),
            app.data_dir().unwrap(),
        )
    });
    let resolved_b = crate::attachments::resolve(&data_dir, &attachment_b).unwrap();
    app.update(&mut cx, |app, _| {
        let mut run = crate::domain::AgentRun {
            id: 990,
            task_id: task,
            task_title: "Material task".into(),
            directory: data_dir.to_string_lossy().into_owned(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            agent: "test".into(),
            model: "test".into(),
            stage: Stage::Planned,
            status: "Succeeded".into(),
            logs: vec![],
            output: String::new(),
            attachments: vec![attachment_b.clone()],
            research_kind: None,
            sources: vec![],
            input_signature: String::new(),
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
        };
        app.store.push_run(run.clone());
        app.last_saved.push_run(run);
    });
    cx.update_window(handle, |_, w, cx| {
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert!(
            resolved_b.exists(),
            "retained run history keeps the material copy"
        );
    })
    .unwrap();
    screenshot(&mut cx, handle, &directory, "material-undone-history.png");
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn wait_material_import(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
) {
    let mut waited = 0;
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                let app = app.read(cx);
                app.attachment_import.is_none() && app.pending_material_job.is_none()
            })
            .unwrap();
        if done {
            return;
        }
        waited += 1;
        assert!(waited < 300, "material import did not finish");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn material_failure_cancel_and_retry(root: &Path) {
    let directory = root.join("material-failure");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, goal, _) = workspace(&directory);
    let task = store.create_task("Material retry task", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let pdf = directory.join("retry-material.pdf");
    std::fs::write(&pdf, b"%PDF-1.4\nmaterial retry\n").unwrap();
    let blocked_path = directory.join("blocked.sqlite3");
    std::fs::create_dir_all(&blocked_path).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(goal, w, cx);
    });
    // Initial Add on a completed goal: Cancel must delete only the unreferenced copy.
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| {
            app.import_attachments(goal, vec![pdf.clone()], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    let relative = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.has_material_review());
        let crate::research_input_app::PendingPreparedSave::Materials(
            crate::material_app::PreparedMaterialSave::Add(plan),
        ) = &app.research_save_impact.as_ref().unwrap().prepared
        else {
            panic!("material add review");
        };
        plan.imported()[0].relative_path.clone()
    });
    let data_dir = app.update(&mut cx, |app, _| app.data_dir().unwrap());
    let copied = data_dir.join(&relative);
    assert!(copied.exists());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("research-save-impact-cancel", cx);
        w.render_frame(cx);
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert!(app.store.task(goal).unwrap().attachments.is_empty());
        assert!(app.pending_material_edit.is_none());
        assert!(!copied.exists(), "cancel removes the unreferenced imported copy");
    })
    .unwrap();

    // Failed durable Add with no modal must retain copy/pending/error and Retry once.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.open_task(task, w, cx);
            app.storage = Some(Storage::new(blocked_path.clone()));
            app.import_attachments(task, vec![pdf.clone()], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    let pending_add = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.pending_material_edit.is_some());
        assert!(!app.research_save_is_pending());
        assert!(!app.error.is_empty());
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 0);
        let crate::material_app::PendingMaterialEdit {
            prepared: crate::material_app::PreparedMaterialSave::Add(plan),
            ..
        } = app.pending_material_edit.as_ref().unwrap()
        else {
            panic!("pending add");
        };
        plan.imported()[0].relative_path.clone()
    });
    let retry_copy = data_dir.join(&pending_add);
    assert!(retry_copy.exists());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending material save".into();
            app.qa_storage_writes.set(0);
        });
        w.render_frame(cx);
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert!(app.pending_material_edit.is_none());
        assert_eq!(app.qa_storage_writes.get(), 1);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 1);
    })
    .unwrap();

    // Failed Remove -> reorder -> Retry removes the original identity, not the index.
    let source_b = directory.join("second-material.pdf");
    std::fs::write(&source_b, b"%PDF-1.4\nsecond material\n").unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| {
            app.import_attachments(task, vec![source_b.clone()], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(blocked_path.clone()))
        });
        w.render_frame(cx);
        w.click("material-menu-0", cx);
        w.render_frame(cx);
        w.click("material-remove-0", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.pending_material_edit.is_some());
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 2);
    })
    .unwrap();
    let original_identity = cx.update(|cx| {
        let app = app.read(cx);
        let crate::material_app::PendingMaterialEdit {
            prepared: crate::material_app::PreparedMaterialSave::Remove(plan),
            ..
        } = app.pending_material_edit.as_ref().unwrap()
        else {
            panic!("pending remove");
        };
        plan.target_identity().clone()
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.store.task_mut(task).unwrap().attachments.swap(0, 1);
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending material save".into();
            app.qa_storage_writes.set(0);
        });
        w.render_frame(cx);
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.pending_material_edit.is_none());
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert_ne!(
            app.store.task(task).unwrap().attachments[0].relative_path,
            original_identity.relative_path
        );
        assert_eq!(app.qa_storage_writes.get(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 1);
    })
    .unwrap();

    // Importer in flight blocks a second material command; a pending intent
    // blocks a new import admission without clearing the pending operation.
    let third = directory.join("third-material.pdf");
    std::fs::write(&third, b"%PDF-1.4\nthird material\n").unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.import_attachments(task, vec![third], cx);
            assert!(!app.request_material_remove(task, 0, cx));
            assert!(app.error.contains("Wait for the current material import"));
            assert!(app.pending_material_edit.is_none());
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(blocked_path.clone()))
        });
        w.render_frame(cx);
        w.click("material-menu-0", cx);
        w.render_frame(cx);
        w.click("material-remove-0", cx);
        w.render_frame(cx);
        assert!(app.read(cx).pending_material_edit.is_some());
    })
    .unwrap();
    let fourth = directory.join("fourth-material.pdf");
    std::fs::write(&fourth, b"%PDF-1.4\nfourth material\n").unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| {
            app.import_attachments(task, vec![fourth], cx);
            assert!(app.error.contains("Resolve the pending material change"));
            assert!(app.pending_material_edit.is_some());
            assert!(app.attachment_import.is_none());
            app.storage = Some(Storage::new(path.clone()));
        });
    })
    .unwrap();

    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage_error = "Retry the pending remove".into();
            app.qa_storage_writes.set(0);
        });
        w.render_frame(cx);
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.pending_material_edit.is_none());
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert_eq!(app.qa_storage_writes.get(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 1);
    })
    .unwrap();

    settle_material_jobs(&mut cx, handle, &app);
    // A failed Add B survives an unrelated Undo of Add A, including its copy.
    let independent = app.update(&mut cx, |app, _| {
        app.store.create_task("Independent material", 1, Stage::Planned, None).unwrap()
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.open_task(independent, w, cx);
            app.import_attachments(independent, vec![pdf.clone()], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(blocked_path.clone()))
        });
        w.render_frame(cx);
        app.update(cx, |app, cx| {
            app.import_attachments(independent, vec![source_b.clone()], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    let pending_b = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.pending_material_edit.is_some());
        let crate::material_app::PendingMaterialEdit {
            prepared: crate::material_app::PreparedMaterialSave::Add(plan),
            ..
        } = app.pending_material_edit.as_ref().unwrap()
        else {
            panic!("pending B");
        };
        plan.imported()[0].relative_path.clone()
    });
    let pending_b_path = data_dir.join(&pending_b);
    assert!(pending_b_path.exists());
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = Some(Storage::new(path.clone())));
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(independent).unwrap().attachments.len(), 0);
        assert!(app.pending_material_edit.is_some());
        assert!(pending_b_path.exists());
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);

    // Explicit discard owns the failed B intent and its copy before the next command.
    app.update(&mut cx, |app, cx| app.cancel_pending_material(cx));
    assert!(cx.update(|cx| app.read(cx).pending_material_edit.is_none()));
    settle_material_jobs(&mut cx, handle, &app);
    assert!(!pending_b_path.exists());

    // Archived target after import: drop attachment/clean copy, never report success.
    let archived_task = app.update(&mut cx, |app, _| {
        app.store.create_task("Archive import", 1, Stage::Planned, None).unwrap()
    });
    let archive_source = directory.join("archived-material.pdf");
    std::fs::write(&archive_source, b"%PDF-1.4\narchived material\n").unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.open_task(archived_task, w, cx);
            app.import_attachments(archived_task, vec![archive_source.clone()], cx);
            app.store.task_mut(archived_task).unwrap().archived = true;
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert!(app.store.task(archived_task).unwrap().attachments.is_empty());
        assert!(app.pending_material_edit.is_none());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn material_completed_confirm_failed_undo_and_retry(root: &Path) {
    let directory = root.join("material-completed");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let pdf = directory.join("completed-material.pdf");
    std::fs::write(&pdf, b"%PDF-1.4\ncompleted material\n").unwrap();
    let blocked_path = directory.join("blocked.sqlite3");
    std::fs::create_dir_all(&blocked_path).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(goal, w, cx);
    });
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| app.import_attachments(goal, vec![pdf.clone()], cx));
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        assert!(app.read(cx).has_material_review());
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().attachments.len(), 1);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().attachments.len(), 1);
    })
    .unwrap();
    // Failed material Undo leaves both metadata and bytes durable.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(blocked_path.clone()))
        });
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().attachments.len(), 1);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().attachments.len(), 1);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending material undo".into();
        });
        w.render_frame(cx);
        w.click("undo", cx);
        w.render_frame(cx);
    })
    .unwrap();
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().attachments.len(), 0);
        assert!(app.undo.is_empty());
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().attachments.len(), 0);
    })
    .unwrap();

    // Footer Retry after a no-modal failed Add must re-review a newly research
    // impact before writing, then save exactly once.
    let task = app.update(&mut cx, |app, _| {
        app.store.create_task("Retry material task", 1, Stage::Planned, None).unwrap()
    });
    let retry_pdf = directory.join("retry-impact-material.pdf");
    std::fs::write(&retry_pdf, b"%PDF-1.4\nretry impact material\n").unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.open_task(task, w, cx);
            app.storage = Some(Storage::new(path.clone()));
            app.persist_if_changed(true);
            app.storage = Some(Storage::new(blocked_path.clone()));
            app.import_attachments(task, vec![retry_pdf], cx);
        });
    })
    .unwrap();
    wait_material_import(&mut cx, handle, &app);
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            let goal = app
                .store
                .create_research_goal("Material retry goal".into(), "Brief".into(), 1)
                .unwrap();
            {
                let task_state = app.store.task_mut(task).unwrap();
                task_state.parent_id = Some(goal);
                task_state.research = Some(crate::research::ResearchTask {
                    requires_approval: false,
                    kind: crate::research::ResearchKind::Collect,
                    goal_id: goal,
                    depends_on: vec![],
                    space_name: String::new(),
                });
            }
            app.store
                .task_mut(goal)
                .unwrap()
                .research
                .as_mut()
                .unwrap()
                .requires_approval = true;
            app.store.task_mut(goal).unwrap().approved_plan =
                app.store.goal_plan_signature(goal);
            app.storage = Some(Storage::new(path.clone()));
            app.storage_error = "Retry the pending material save".into();
            app.qa_storage_writes.set(0);
        });
        w.render_frame(cx);
        w.click("retry-save", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.has_material_review(), "changed material impact needs review");
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 0);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(task).unwrap().attachments.len(), 1);
        assert_eq!(app.qa_storage_writes.get(), 1);
        assert_eq!(app.undo.len(), 1);
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task).unwrap().attachments.len(), 1);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn material_stale_remove_verification(root: &Path) {
    let directory = root.join("material-stale-remove");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store.create_task("Stale remove task", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let a = directory.join("stale-a.pdf");
    let b = directory.join("stale-b.pdf");
    std::fs::write(&a, b"%PDF-1.4\nstale A\n").unwrap();
    std::fs::write(&b, b"%PDF-1.4\nstale B\n").unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task, w, cx);
    });
    for source in [&a, &b] {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |app, cx| app.import_attachments(task, vec![source.clone()], cx));
        })
        .unwrap();
        wait_material_import(&mut cx, handle, &app);
    }
    app.update(&mut cx, |app, _| {
        app.store.task_mut(task).unwrap().attachments[0].content_sha256.clear();
    });
    let undo_before = cx.update(|cx| app.read(cx).undo.len());
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("material-menu-0", cx);
        w.render_frame(cx);
        w.click("material-remove-0", cx);
        // Reorder immediately after verification was submitted but before the
        // worker response can be applied.
        app.update(cx, |app, _| {
            app.store.task_mut(task).unwrap().attachments.swap(0, 1);
        });
        w.render_frame(cx);
    })
    .unwrap();
    let stable = cx.update(|cx| {
        app.read(cx).store.task(task).unwrap().attachments.clone()
    });
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert_eq!(
            app.store.task(task).unwrap().attachments,
            stable,
            "stale remove completion must not retarget or remove metadata"
        );
        assert_eq!(app.undo.len(), undo_before, "stale completion must not add undo");
        assert!(app.error.contains("changed while it was being verified"));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn material_background_tamper_boundary(root: &Path) {
    let directory = root.join("material-tamper");
    std::fs::create_dir_all(&directory).unwrap();
    let (mut store, _, _) = workspace(&directory);
    let task = store.create_task("Tamper task", 1, Stage::Planned, None).unwrap();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let source = directory.join("tamper.pdf");
    std::fs::write(&source, b"%PDF-1.4\ntamper fixture\n").unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.open_task(task, w, cx);
        app.material_verifier.set_test_delay_ms(250);
    });
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| {
            app.import_attachments(task, vec![source], cx);
        });
    })
    .unwrap();
    let mut waited = 0;
    loop {
        let submitted = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                let app = app.read(cx);
                app.attachment_import.is_none() && app.pending_material_job.is_some()
            })
            .unwrap();
        if submitted {
            break;
        }
        waited += 1;
        assert!(waited < 300, "material verification was not submitted");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (relative, data_dir) = cx.update(|cx| {
        let app = app.read(cx);
        let crate::material_app::PendingMaterialJob::PrepareAdd { imported, .. } =
            app.pending_material_job.as_ref().unwrap()
        else {
            panic!("prepare add verification");
        };
        (imported[0].relative_path.clone(), app.data_dir().unwrap())
    });
    let copied = data_dir.join(&relative);
    let mut bytes = std::fs::read(&copied).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    std::fs::write(&copied, bytes).unwrap();
    let mut waited = 0;
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                app.read(cx).pending_material_job.is_none()
            })
            .unwrap();
        if done {
            break;
        }
        waited += 1;
        assert!(waited < 300, "tampered verification did not complete");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    settle_material_jobs(&mut cx, handle, &app);
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert!(app.store.task(task).unwrap().attachments.is_empty());
        assert!(app.undo.is_empty());
        assert!(!app.error.is_empty());
        assert!(copied.exists(), "changed bytes are retained, not deleted");
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert!(loaded.task(task).unwrap().attachments.is_empty());
        app.material_verifier.set_test_delay_ms(0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn material_workspace_switch_during_verify(root: &Path) {
    let directory_a = root.join("material-switch-a");
    let directory_b = root.join("material-switch-b");
    std::fs::create_dir_all(&directory_a).unwrap();
    std::fs::create_dir_all(&directory_b).unwrap();
    let (mut store_a, _, _) = workspace(&directory_a);
    let task_a = store_a.create_task("Switch task", 1, Stage::Planned, None).unwrap();
    let (mut store_b, _, _) = workspace(&directory_b);
    let task_b = store_b.create_task("Switch task", 1, Stage::Planned, None).unwrap();
    assert_eq!(task_a, task_b);
    let path_a = directory_a.join("workspace.sqlite3");
    let path_b = directory_b.join("workspace.sqlite3");
    let mut store_a_reconnect = store_a.clone();
    Storage::new(path_a.clone()).save(&store_a).unwrap();
    Storage::new(path_b.clone()).save(&store_b).unwrap();
    let source = directory_a.join("switch.pdf");
    let bytes = b"%PDF-1.4\nworkspace switch\n";
    std::fs::write(&source, bytes).unwrap();
    let (mut cx, handle, app) = with_app(store_a, &path_a, |app, w, cx| {
        app.open_task(task_a, w, cx);
        app.material_verifier.set_test_delay_ms(250);
    });
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| app.import_attachments(task_a, vec![source], cx));
    })
    .unwrap();
    let mut waited = 0;
    loop {
        let submitted = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                let app = app.read(cx);
                app.attachment_import.is_none() && app.pending_material_job.is_some()
            })
            .unwrap();
        if submitted {
            break;
        }
        waited += 1;
        assert!(waited < 300, "switch verification was not submitted");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (relative, sha, data_a) = cx.update(|cx| {
        let app = app.read(cx);
        let crate::material_app::PendingMaterialJob::PrepareAdd { imported, .. } =
            app.pending_material_job.as_ref().unwrap()
        else {
            panic!("prepare add switch");
        };
        (
            imported[0].relative_path.clone(),
            imported[0].content_sha256.clone(),
            app.data_dir().unwrap(),
        )
    });
    let copied_a = data_a.join(&relative);
    let copied_b = directory_b.join(&relative);
    std::fs::create_dir_all(copied_b.parent().unwrap()).unwrap();
    std::fs::write(&copied_b, bytes).unwrap();
    store_b
        .task_mut(task_b)
        .unwrap()
        .attachments
        .push(crate::attachments::Attachment {
            name: "switch.pdf".into(),
            relative_path: relative.clone(),
            media_type: "application/pdf".into(),
            size: bytes.len() as u64,
            content_sha256: sha.clone(),
        });
    Storage::new(path_b.clone()).save(&store_b).unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, cx| {
            app.store = store_b.clone();
            app.last_saved = store_b.clone();
            app.storage = Some(Storage::new(path_b.clone()));
            cx.notify();
        });
    })
    .unwrap();
    let mut waited = 0;
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                let app = app.read(cx);
                app.pending_material_job.is_none()
            })
            .unwrap();
        if done {
            break;
        }
        waited += 1;
        assert!(waited < 300, "workspace switch verification did not settle");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert_eq!(app.store.task(task_b).unwrap().attachments.len(), 1);
        assert!(copied_b.exists(), "other workspace copy is untouched");
        assert!(
            copied_a.exists(),
            "original-root cleanup ownership is retained while another workspace is active"
        );
        assert!(app.pending_cleanup_retry.iter().any(|obligation| {
            obligation.root == data_a
                && obligation
                    .copies
                    .iter()
                    .any(|copy| copy.relative_path == relative)
        }));
        assert!(app.undo.is_empty());
        assert!(!app.error.is_empty());
        let loaded = Storage::new(path_b.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task_b).unwrap().attachments.len(), 1);
        app.material_verifier.set_test_delay_ms(0);
    })
    .unwrap();

    // Reconnect authoritative workspace A. One copy is now referenced and must
    // be retained; another unreferenced owned copy must be deleted.
    let unreferenced_relative = "attachments/unreferenced-foreign.pdf";
    let unreferenced_path = directory_a.join(unreferenced_relative);
    std::fs::create_dir_all(unreferenced_path.parent().unwrap()).unwrap();
    let unreferenced_bytes = b"%PDF-1.4\nunreferenced foreign\n";
    std::fs::write(&unreferenced_path, unreferenced_bytes).unwrap();
    let unreferenced = crate::scoped_material::ImportedCopy {
        relative_path: unreferenced_relative.into(),
        size: unreferenced_bytes.len() as u64,
        content_sha256: format!(
            "{:x}",
            sha2::Sha256::digest(unreferenced_bytes)
        ),
        owner_task: task_a,
    };
    store_a_reconnect
        .task_mut(task_a)
        .unwrap()
        .attachments
        .push(crate::attachments::Attachment {
            name: "switch.pdf".into(),
            relative_path: relative.clone(),
            media_type: "application/pdf".into(),
            size: bytes.len() as u64,
            content_sha256: sha.clone(),
        });
    Storage::new(path_a.clone()).save(&store_a_reconnect).unwrap();
    cx.update_window(handle, |_, _, cx| {
        app.update(cx, |app, _| {
            app.pending_cleanup_retry
                .push(crate::scoped_material::CleanupObligation {
                    root: directory_a.clone(),
                    copies: vec![unreferenced.clone()],
                });
            app.store = store_a_reconnect.clone();
            app.last_saved = store_a_reconnect.clone();
            app.storage = Some(Storage::new(path_a.clone()));
        });
    })
    .unwrap();
    let mut waited = 0;
    loop {
        let done = cx
            .update_window(handle, |_, _, cx| {
                app.update(cx, |app, cx| app.poll_runtime(cx));
                let app = app.read(cx);
                app.pending_material_job.is_none() && app.pending_cleanup_retry.is_empty()
            })
            .unwrap();
        if done {
            break;
        }
        waited += 1;
        assert!(waited < 300, "reconnected cleanup did not drain");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.update_window(handle, |_, _, cx| {
        let app = app.read(cx);
        assert!(copied_a.exists(), "referenced original copy is retained");
        assert!(!unreferenced_path.exists(), "unreferenced owned copy is deleted");
        assert!(copied_b.exists(), "workspace B copy remains untouched");
        assert_eq!(app.store.task(task_a).unwrap().attachments.len(), 1);
        let loaded = Storage::new(path_a.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(task_a).unwrap().attachments.len(), 1);
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn confirm_retry_preserves_unrelated_fields(root: &Path) {
    let directory = root.join("retry-labels");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let activity_before = store.task(goal).unwrap().activity.len();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Title only retry", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = None);
        let pending = app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
            assert!(app.research_save_is_pending());
            true
        });
        assert!(pending);
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert!(app.pending_task_edit.is_some());
        assert!(app.draft.is_some());
        assert!(app.research_save_is_pending());
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
        let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
        assert_eq!(loaded.task(goal).unwrap().title, "Branched research");
        assert_eq!(loaded.task(goal).unwrap().labels, vec!["Research".to_string()]);
        assert_eq!(loaded.task(goal).unwrap().activity.len(), activity_before);
        app.qa_storage_writes.set(0);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.store.task_mut(goal).unwrap().labels = vec!["Later label".into()];
            app.store.task_mut(goal).unwrap().priority = Priority::Urgent;
            app.storage = Some(Storage::new(path.clone()));
        });
        // The same visible confirmation button is used again after the failed write.
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Title only retry");
        assert_eq!(
            app.store.task(goal).unwrap().labels,
            vec!["Later label".to_string()]
        );
        assert_eq!(app.store.task(goal).unwrap().priority, Priority::Urgent);
        assert_eq!(
            app.qa_storage_writes.get(),
            1,
            "retry commits exactly one durable write"
        );
        assert_eq!(
            app.store.task(goal).unwrap().activity.len(),
            activity_before + 2,
            "one task update and one approval-withdrawal activity line"
        );
        assert_eq!(app.undo.len(), 1, "retry records exactly one undo entry");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn confirm_retry_conflicts_on_owned_field(root: &Path) {
    let directory = root.join("retry-title");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Pending title", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| app.storage = None);
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        app.update(cx, |app, _| {
            app.store.task_mut(goal).unwrap().title = "Newer external title".into();
            app.storage = Some(Storage::new(path.clone()));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Newer external title");
        assert!(app.draft.is_some(), "draft retained after conflict");
        assert!(!app.error.is_empty());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn confirm_reviews_new_impact(root: &Path) {
    let directory = root.join("new-impact");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Impact title", w, cx);
    });
    let first_impact = cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
            app.research_save_impact.as_ref().unwrap().impact.clone()
        })
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            app.store.task_mut(steps[1]).unwrap().title = "Sibling changed".into();
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        let app = app.read(cx);
        let current = app.research_save_impact.as_ref().unwrap().impact.clone();
        assert_ne!(current, first_impact, "new impact must be re-reviewed");
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
        assert!(app.draft.is_some());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn pending_batch_rejects_stage(root: &Path) {
    let directory = root.join("pending-batch");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.store.set_stage(goal, Stage::Planned).unwrap();
        app.todo_batch = Some(TodoBatch {
            project_id: 1,
            items: vec![goal],
            completed: 0,
            runs_completed: 0,
            state: BatchState::Running,
            run_ids: Default::default(),
            done_items: vec![],
            failed_items: vec![],
            failed_tasks: vec![],
        });
        begin_editor(app, goal, w, cx);
        app.draft.as_mut().unwrap().stage = Stage::Backlog;
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
            assert!(app.error.contains("submitted") || app.error.contains("running"));
            assert_eq!(app.store.task(goal).unwrap().stage, Stage::Planned);
            assert!(app.draft.is_some());
        });
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn scoped_undo_with_unrelated_work(root: &Path) {
    let directory = root.join("unrelated-work");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Saved before run", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
    })
    .unwrap();
    let other = app.update(&mut cx, |app, _| {
        let other = app
            .store
            .create_research_goal("Other".into(), "Other brief".into(), 1)
            .unwrap();
        app.store.apply_research_plan(other, PLAN).unwrap();
        other
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, _| {
            let child = app.store.research_children(other)[0];
            let mut run = app.store.prepare_run(child).unwrap();
            run.status = "Queued".into();
            run.id = 900;
            app.store.push_run(run);
            app.queued_runs.push((900, vec![]));
        });
        app.update(cx, |app, cx| app.undo_change(w, cx));
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
        assert!(app.undo.is_empty());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn scoped_undo_blocks_on_relevant_run(root: &Path) {
    let directory = root.join("relevant-work");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, steps) = workspace(&directory);
    let mut prepared_run = store.prepare_run(steps[0]).unwrap();
    prepared_run.id = 901;
    prepared_run.status = "Queued".into();
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Before relevant run", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        app.update(cx, |app, _| {
            app.store.push_run(prepared_run.clone());
            app.queued_runs.push((901, vec![]));
        });
        app.update(cx, |app, cx| app.undo_change(w, cx));
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Before relevant run");
        assert_eq!(app.undo.len(), 1, "conflicting scoped entry is retained");
        assert!(app.error.contains("Stop or finish work"));
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn automation_barrier_keeps_scoped_undo(root: &Path) {
    let directory = root.join("automation-barrier");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        begin_editor(app, goal, w, cx);
        set_title(app, "Survives automation", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            app.invalidate_undo_for_automation();
            assert_eq!(app.undo.len(), 1, "scoped entry survives legacy barrier");
            app.undo_change(w, cx);
        });
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
}

fn mixed_legacy_scoped_order(root: &Path) {
    let directory = root.join("mixed-order");
    std::fs::create_dir_all(&directory).unwrap();
    let (store, goal, _) = workspace(&directory);
    let path = directory.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let (mut cx, handle, app) = with_app(store, &path, |app, w, cx| {
        app.snapshot();
        app.store.task_mut(goal).unwrap().labels = vec!["Legacy change".into()];
        begin_editor(app, goal, w, cx);
        set_title(app, "Scoped change", w, cx);
    });
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.request_save_draft(w, cx));
        });
        w.render_frame(cx);
        w.click("research-save-impact-confirm", cx);
        w.render_frame(cx);
        app.update(cx, |app, cx| {
            assert_eq!(app.undo.len(), 2);
            app.undo_change(w, cx);
        });
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().title, "Branched research");
        assert_eq!(
            app.store.task(goal).unwrap().labels,
            vec!["Legacy change".to_string()]
        );
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |app, cx| app.undo_change(w, cx));
        let app = app.read(cx);
        assert_eq!(app.store.task(goal).unwrap().labels, vec!["Research".to_string()]);
        assert!(app.undo.is_empty());
    })
    .unwrap();
    cx.update_window(handle, |_, w, _| w.remove_window())
        .unwrap();
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
