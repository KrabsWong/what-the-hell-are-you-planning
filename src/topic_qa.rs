//! Isolated Topic navigation, editing and persistence acceptance.
use crate::{
    app::{Scope, TaskApp},
    domain::Store,
    persistence::Storage,
};
use gpui_kit::{
    component::{Theme, ThemeMode},
    test::TestWindowExt,
    *,
};
use std::sync::Arc;

pub fn run() {
    let root = std::env::temp_dir().join(format!("planning-topic-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut store = Store::empty();
    store.projects[0].name = "投资研究".into();
    let goal = store
        .create_research_goal(
            "英伟达季度财报与基本面研究".into(),
            "分析增长与风险".into(),
            1,
        )
        .unwrap();
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Light, None, cx);
    });
    let mut app = None;
    let window = cx
        .open_window(size(px(1000.), px(720.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.attach_storage(Storage::new(path.clone()), w, cx);
                a.navigate(Scope::Project(1), w, cx);
                crate::appearance::apply(crate::appearance::Appearance::Light, Some(w), cx);
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    cx.update_window(handle, |_, w, cx| {
        w.render_frame(cx);
        w.click("new-project", cx);
        w.render_frame(cx);
        w.click("project-name", cx);
        w.input("财报", cx);
        w.click("create-project", cx);
        let project = app.read(cx).store.projects.last().unwrap().id;
        assert_eq!(app.read(cx).scope, Scope::Project(project));
        assert!(
            app.read(cx).planning_project.is_none(),
            "Creating a project must not launch research intake"
        );
        assert!(
            !app.read(cx)
                .store
                .tasks
                .iter()
                .any(|t| t.project_id == project)
        );
        assert!(app.read(cx).store.research_requests.is_empty());
        app.update(cx, |a, cx| a.navigate(Scope::Project(1), w, cx));
    })
    .unwrap();
    let topic = cx
        .update_window(handle, |_, w, cx| {
            // Seed the grouping fixture; new topics are created through research intake.
            let id = app.update(cx, |a, _| {
                a.store.create_topic(1, "英伟达 NVIDIA").unwrap()
            });
            app.update(cx, |a, cx| {
                a.open_task(goal, w, cx);
                a.show_task_details = true;
                cx.notify();
            });
            w.render_frame(cx);
            w.click("goal-topic", cx);
            w.click(1usize, cx);
            assert_eq!(app.read(cx).store.task(goal).unwrap().topic_id, Some(id));
            app.update(cx, |a, cx| {
                a.store.apply_research_plan(goal, r#"{"tasks":[{"title":"收集财报","instructions":"收集原始财报","kind":"collect","depends_on":[]},{"title":"分析基本面","instructions":"分析财报","kind":"analyze","depends_on":[0]},{"title":"研究报告","instructions":"汇总判断","kind":"advise","depends_on":[1]}]}"#).unwrap();
                let child = a.store.create_task("补充核查", 1, crate::domain::Stage::Planned, Some(goal)).unwrap();
                assert_eq!(a.store.task_topic(child).unwrap().id, id);
                a.change_stage(child, crate::domain::Stage::Done, cx);
                assert_eq!(a.store.task(child).unwrap().stage, crate::domain::Stage::Done, "Ordinary subtasks retain manual completion");
                for title in [
                    "存储芯片周期：DRAM、NAND 与 HBM 的供需、库存与价格走势",
                    "先进制程代工与先进封装：产能扩张、资本开支与定价能力",
                    "半导体设备、零部件与材料：资本开支周期、出口管制与国产替代",
                    "成熟制程、模拟、功率与汽车/工业半导体：需求分化与中国产能影响",
                    "跨 A 股、港股、美股的半导体估值、周期定位与投资机会地图",
                ] {
                    let extra = a.store.create_research_goal(title.into(), "研究行业趋势".into(), 1).unwrap();
                    a.store.set_goal_topic(extra, Some(id)).unwrap();
                }
                a.navigate(Scope::Topic(id), w, cx);
            });
            w.render_frame(cx);
            w.click(format!("topic-toggle-{id}"), cx);
            assert!(app.read(cx).collapsed_sidebar_topics.contains(&id));
            w.render_frame(cx);
            w.click(format!("topic-toggle-{id}"), cx);
            assert!(!app.read(cx).collapsed_sidebar_topics.contains(&id));
            id
        })
        .unwrap();
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.update_window(handle, |_, w, cx| {
        w.click("board-view", cx);
        assert!(app.read(cx).layout == crate::app::Layout::Board);
        w.render_frame(cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("topic-board.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("list-view", cx);
        assert!(app.read(cx).layout == crate::app::Layout::List);
        w.render_frame(cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("topic-light.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("topic-goal-{goal}"), cx);
        assert_eq!(app.read(cx).selected, Some(goal));
        let (crumbs, _) = app.read(cx).navigation_path();
        assert!(crumbs.iter().any(|(_, name, _)| name == "#英伟达 NVIDIA"));
        w.render_frame(cx);
    })
    .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("goal-steps-tab", cx);
        w.render_frame(cx);
        assert!(w.try_find("goal-steps-board").is_some());
        w.click("steps-list-view", cx);
        w.render_frame(cx);
        assert!(w.try_find("goal-steps-list").is_some());
        let child = app.read(cx).store.research_children(goal)[0];
        let before = app.read(cx).store.runs.len();
        w.click(format!("goal-step-{child}"), cx);
        assert_eq!(app.read(cx).selected, Some(child));
        w.render_frame(cx);
        w.click("navigation-back", cx);
        w.render_frame(cx);
        assert!(w.try_find("goal-steps-list").is_some());
        app.update(cx, |a, cx| {
            a.navigate(Scope::All, w, cx);
            a.layout = crate::app::Layout::Board;
        });
        w.render_frame(cx);
        w.click(format!("goal-progress-{goal}"), cx);
        w.render_frame(cx);
        assert_eq!(app.read(cx).selected, Some(goal));
        assert!(w.try_find("goal-steps-board").is_some());
        assert_eq!(
            app.read(cx).store.runs.len(),
            before,
            "Viewing steps never executes work"
        );
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("goal-steps-board.png"))
        .unwrap();
    // Isolated display fixtures, no scheduler or external processes are started.
    let mut fixture = cx.update(|cx| app.read(cx).store.clone());
    fixture.default_model = "fixture/model".into();
    fixture.projects[0].directory = root.to_string_lossy().into_owned();
    fixture
        .set_stage(goal, crate::domain::Stage::Planned)
        .unwrap();
    let children = fixture.research_children(goal);
    let mut running = fixture.prepare_run(children[0]).unwrap();
    running.id = 1;
    running.status = "Running".into();
    let mut failed = running.clone();
    failed.id = 2;
    failed.task_id = children[1];
    failed.status = "Failed".into();
    fixture.runs.extend([running, failed]);
    fixture.task_mut(children[0]).unwrap().title =
        "收集官方财报与投资者电话会：核实收入、毛利率、现金流及未来季度指引，保留完整来源".into();
    fixture
        .create_task(
            "等待人工补充材料",
            1,
            crate::domain::Stage::Planned,
            Some(goal),
        )
        .unwrap();
    steps_screenshot(
        &fixture,
        goal,
        1720.,
        false,
        false,
        &root.join("steps-all-colors-light.png"),
    );
    steps_screenshot(
        &fixture,
        goal,
        1720.,
        true,
        false,
        &root.join("steps-all-colors-dark.png"),
    );
    steps_screenshot(
        &fixture,
        goal,
        900.,
        false,
        true,
        &root.join("steps-list-narrow.png"),
    );
    cx.update_window(handle, |_, w, cx| {
        w.click("goal-overview-tab", cx);
        w.render_frame(cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("topic-property.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.show_task_details = true;
            a.store.default_model = "tencent-token-plan/deepseek/deepseek-flash".into();
            a.detail_scroll.set_offset(point(px(0.), px(-560.)));
            cx.notify();
        });
        w.render_frame(cx);
    })
    .unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("task-execution-settings.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("navigation-back", cx);
        assert_eq!(app.read(cx).scope, Scope::Topic(topic));
        w.render_frame(cx);
        w.click("rename-topic", cx);
        w.render_frame(cx);
        app.read(cx)
            .topic_editor
            .as_ref()
            .unwrap()
            .2
            .clone()
            .update(cx, |s, cx| s.set_value("英伟达长期研究", w, cx));
        w.click("save-topic", cx);
        assert_eq!(
            app.read(cx)
                .store
                .topics
                .iter()
                .find(|t| t.id == topic)
                .unwrap()
                .name,
            "英伟达长期研究"
        );
        w.render_frame(cx);
        w.click("archive-topic", cx);
        assert!(
            app.read(cx)
                .store
                .topics
                .iter()
                .find(|t| t.id == topic)
                .unwrap()
                .archived
        );
        assert!(!app.read(cx).store.task(goal).unwrap().archived);
        app.update(cx, |a, cx| {
            a.scope = Scope::All;
            assert!(!a.visible_tasks().iter().any(|t| t.id == goal));
            a.scope = Scope::Archive;
            assert!(a.visible_tasks().iter().any(|t| t.id == goal));
            cx.notify();
        });
        w.render_frame(cx);
        w.click(SharedString::from(format!("topic-link-{topic}")), cx);
        assert_eq!(app.read(cx).scope, Scope::Topic(topic));

        w.render_frame(cx);
        w.click("archive-topic", cx);
        assert!(
            !app.read(cx)
                .store
                .topics
                .iter()
                .find(|t| t.id == topic)
                .unwrap()
                .archived
        );
        w.render_frame(cx);
        w.click("topic-new-goal", cx);
        assert_eq!(app.read(cx).planning_topic, Some(topic));
        app.update(cx, |a, cx| {
            a.project_brief
                .update(cx, |s, cx| s.set_value("下一季度财报，检查业绩", w, cx));
        });
        w.render_frame(cx);
        w.click("save-project-context", cx);
        assert_eq!(
            app.read(cx)
                .store
                .research_requests
                .last()
                .unwrap()
                .topic_id,
            Some(topic)
        );
        app.update(cx, |a, cx| {
            a.persist_if_changed(true);
            assert!(a.storage_error.is_empty());
            a.navigate(Scope::Topic(topic), w, cx);
        });
        w.render_frame(cx);
        w.press("cmd-k", cx);
        assert_eq!(app.read(cx).scope, Scope::All);
        app.update(cx, |a, cx| a.navigate(Scope::Topic(topic), w, cx));
        Theme::change(ThemeMode::Dark, Some(w), cx);
    })
    .unwrap();
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("topic-dark.png"))
        .unwrap();
    let saved = Storage::new(path).load().unwrap().unwrap();
    assert_eq!(
        saved.topics.iter().find(|t| t.id == topic).unwrap().name,
        "英伟达长期研究"
    );
    assert_eq!(saved.task(goal).unwrap().topic_id, Some(topic));
    println!("Topic QA passed; screenshots: {}", root.display());
}

fn steps_screenshot(
    store: &Store,
    goal: u64,
    width: f32,
    dark: bool,
    list: bool,
    path: &std::path::Path,
) {
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
    });
    let mut app = None;
    let window = cx
        .open_window(size(px(width), px(800.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new_mock(w, cx));
            view.update(cx, |a, cx| {
                a.store = store.clone();
                a.open_goal_steps(goal, w, cx);
                if list {
                    a.steps_layout = crate::app::Layout::List;
                }
                crate::appearance::apply(
                    if dark {
                        crate::appearance::Appearance::Dark
                    } else {
                        crate::appearance::Appearance::Light
                    },
                    Some(w),
                    cx,
                );
            });
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let handle = window.into();
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    if list {
        let app = app.unwrap();
        for step in store.research_children(goal) {
            cx.update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.store.runs[0].task_id = step;
                    a.store.runs[0].status = "Running".into();
                    a.open_task(step, w, cx);
                });
                w.render_frame(cx);
                w.click("task-progress", cx);
                assert_eq!(app.read(cx).selected, Some(goal));
                assert_eq!(app.read(cx).goal_steps_tab, Some(goal));
                assert!(app.read(cx).steps_layout == crate::app::Layout::List);
                w.render_frame(cx);
                assert!(w.try_find("goal-steps-list").is_some());
            }).unwrap();
        }
        app.update(&mut cx, |a, cx| { a.store = store.clone(); cx.notify(); });
        cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
    }
    cx.capture_screenshot(handle).unwrap().save(path).unwrap();
}

/// Startup expansion and last-project persistence, independent of execution workflows.
pub fn sidebar() {
    let root = std::env::temp_dir().join(format!("planning-sidebar-qa-{}", std::process::id()));
    let mut store = Store::mock();
    let topic = store.create_topic(1, "Startup topic").unwrap();
    let goal = store.create_research_goal_in_topic(
        "Startup goal".into(), "Check navigation".into(), 1, Some(topic),
    ).unwrap();
    let path = root.join("workspace.sqlite3");
    Storage::new(path.clone()).save(&store).unwrap();
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(crate::ui_assets::AppAssets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| init(cx));
    let mut app = None;
    let window = cx.open_window(size(px(1000.), px(720.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| {
            a.attach_storage(Storage::new(path.clone()), w, cx);
            assert!(a.expanded_sidebar_projects.is_empty());
        });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle = window.into();
    // Navigation survives a storage reload, while expansion is reset to one project.
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            let other = a.store.projects.last().unwrap().id;
            a.navigate(Scope::Project(other), w, cx);
            assert_eq!(a.store.last_active_project, Some(other));
            a.navigate(Scope::Topic(topic), w, cx);
            assert_eq!(a.store.last_active_project, Some(1));
            a.navigate(Scope::All, w, cx);
            assert_eq!(a.store.last_active_project, Some(1));
            a.persist_if_changed(true);
            let loaded = Storage::new(path.clone()).load().unwrap().unwrap();
            assert_eq!(loaded.last_active_project, Some(1));
            a.expanded_sidebar_projects = a.store.projects.iter().map(|p| p.id).collect();
            a.collapsed_sidebar_topics.clear();
            a.collapsed_sidebar_goals.clear();
            // Open a persisted copy to exercise startup without sharing the live lock.
            let reopened = Storage::new(root.join("reopened/workspace.sqlite3"));
            reopened.save(&loaded).unwrap();
            a.attach_storage(reopened, w, cx);
            assert_eq!(a.expanded_sidebar_projects, vec![1]);
            assert!(a.store.topics.iter().all(|t| a.collapsed_sidebar_topics.contains(&t.id)));
            assert!(a.collapsed_sidebar_goals.contains(&goal));
            a.open_task(goal, w, cx);
            assert!(!a.collapsed_sidebar_topics.contains(&topic));
            a.store.last_active_project = Some(u64::MAX);
            a.restore_sidebar();
            assert!(a.expanded_sidebar_projects.is_empty());
            a.store.last_active_project = None;
            a.restore_sidebar();
            assert!(a.expanded_sidebar_projects.is_empty());
            a.open_task(goal, w, cx);
            assert_eq!(a.store.last_active_project, Some(1));
        });
    }).unwrap();
    println!("Sidebar startup QA passed");
}

/// Isolated migration checks do not depend on unrelated workflow fixtures.
pub fn run_migration() {
    let root = std::env::temp_dir().join(format!("planning-migration-qa-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut store = Store::empty();
    let goal = store.create_task("Semiconductor research", 1, crate::domain::Stage::Planned, None).unwrap();
    store.create_task("Review earnings sources", 1, crate::domain::Stage::Planned, Some(goal)).unwrap();
    let topic = store.task_topic(goal).unwrap().id;
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(), Arc::new(crate::ui_assets::AppAssets), platform::current_headless_renderer);
    cx.update(|cx| { init(cx); Theme::change(ThemeMode::Light, None, cx); });
    let mut app = None;
    let window = cx.open_window(size(px(1440.), px(900.)), |w, cx| {
        let view = cx.new(|cx| TaskApp::new_mock(w, cx));
        view.update(cx, |a, cx| { a.store = store; a.navigate(Scope::Topic(topic), w, cx); });
        app = Some(view.clone());
        cx.new(|cx| base::Root::new(view, w, cx))
    }).unwrap();
    let app = app.unwrap();
    let handle = window.into();
    // Rename uses the same application modal surface and never mutates on cancel/error.
    cx.update_window(handle, |_, w, cx| {
        let before = app.read(cx).store.clone();
        app.update(cx, |a, cx| { a.navigate(Scope::Project(1), w, cx); });
        w.render_frame(cx);
        w.click("project-actions", cx);
        w.render_frame(cx);
        w.click(0usize, cx);
        assert!(app.read(cx).project_rename.is_some());
        assert!(!w.has_active_prompt());
        w.render_frame(cx);
        w.click("cancel-project-rename", cx);
        assert_eq!(app.read(cx).store, before);
        app.update(cx, |a, cx| {
            a.open_project_rename(1, w, cx);
            let input = a.project_rename.as_ref().unwrap().1.clone();
            input.update(cx, |s, cx| s.set_value("  ", w, cx));
        });
        w.render_frame(cx);
        w.click("save-project-rename", cx);
        assert!(app.read(cx).project_rename.is_some());
        assert_eq!(app.read(cx).store, before);
        app.update(cx, |a, cx| {
            let input = a.project_rename.as_ref().unwrap().1.clone();
            input.update(cx, |s, cx| s.set_value("Renamed research project", w, cx));
        });
        w.render_frame(cx);
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("project-rename.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("save-project-rename", cx);
        assert!(app.read(cx).project_rename.is_none());
        assert_eq!(app.read(cx).store.projects[0].name, "Renamed research project");
        app.update(cx, |a, cx| a.navigate(Scope::Topic(topic), w, cx));
    }).unwrap();
    // Exercise the in-app migration modal with enough projects to require search.
    let before_move = cx.update_window(handle, |_, w, cx| {
        let before = app.read(cx).store.clone();
        app.update(cx, |a, cx| {
            for id in 100..200 {
                a.store.projects.push(crate::domain::Project {
                    id, name: format!("Destination {id}"), key: format!("P{id}"),
                    directory: String::new(), research_brief: String::new(),
                });
            }
            a.request_topic_move(topic, w, cx);
        });
        w.render_frame(cx);
        assert!(app.read(cx).topic_move.is_some());
        assert!(!w.has_active_prompt());
        before
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("topic-move-dialog.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("topic-move-project", cx);
        w.render_frame(cx);
    }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    cx.update_window(handle, |_, w, cx| w.render_frame(cx)).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("topic-move-project-list.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        // Close the popup before clicking the footer, which it overlays.
        w.press("escape", cx);
        w.render_frame(cx);
        if app.read(cx).topic_move.is_some() { w.click("cancel-topic-move", cx); }
        assert!(app.read(cx).topic_move.is_none());
        assert_eq!(app.read(cx).store.topics, before_move.topics);
        app.update(cx, |a, cx| {
            a.request_topic_move(topic, w, cx);
            let select = a.topic_move.as_ref().unwrap().projects.clone();
            select.update(cx, |s, cx| s.set_selected_value(&199, w, cx));
            cx.notify();
        });
        w.render_frame(cx);
        w.click("confirm-topic-move", cx);
        assert!(app.read(cx).topic_move.is_none());
        assert_eq!(app.read(cx).store.task(goal).unwrap().project_id, 199);
        app.update(cx, |a, cx| {
            a.store = before_move.clone();
            a.todo_batch = Some(crate::batch::TodoBatch {
                project_id: 1, items: vec![goal], completed: 0, runs_completed: 0,
                state: crate::batch::BatchState::Paused("Paused for QA".into()),
                run_ids: Default::default(), done_items: vec![], failed_items: vec![], failed_tasks: vec![],
            });
            a.request_topic_move(topic, w, cx);
        });
        w.render_frame(cx);
        assert!(!w.has_active_prompt());
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("topic-move-blocked.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click("cancel-topic-move", cx);
        app.update(cx, |a, cx| { a.store = before_move; a.navigate(Scope::Topic(topic), w, cx); });
    }).unwrap();
    let (moved_topic, moved_goal) = cx.update_window(handle, |_, w, cx| {
        let ids = app.update(cx, |a, cx| {
            a.todo_batch = None;
            let mut project = a.store.projects[0].clone();
            project.id = 2; project.name = "Source project".into();
            a.store.projects.push(project);
            let moved_topic = a.store.create_topic(2, "Moved market research topic with a long descriptive title").unwrap();
            let moved_goal = a.store.create_task_in_topic("Moved goal one", 2, crate::domain::Stage::Planned, None, Some(moved_topic)).unwrap();
            a.store.create_task_in_topic("Moved goal two", 2, crate::domain::Stage::Planned, None, Some(moved_topic)).unwrap();
            // A newer goal in the older topic must not split the moved topic's group.
            a.store.create_task_in_topic("New goal in original topic", 1, crate::domain::Stage::Planned, None, Some(topic)).unwrap();
            a.store.move_topic_project(moved_topic, 1).unwrap();
            a.layout = crate::app::Layout::List;
            a.navigate(Scope::Project(1), w, cx);
            (moved_topic, moved_goal)
        });
        w.render_frame(cx);
        let moved_header = w.find(format!("collection-topic-{}", ids.0));
        let original_header = w.find(format!("collection-topic-{topic}"));
        assert!(moved_header.label().unwrap().contains("2 goals"));
        assert!(original_header.label().unwrap().contains("2 goals"));
        assert!(moved_header.bounds().top() < w.find(format!("task-row-{}", ids.1)).bounds().top());
        assert!(w.find(format!("task-row-{}", ids.1)).bounds().top() < original_header.bounds().top());
        assert!(original_header.bounds().top() < w.find(format!("task-row-{goal}")).bounds().top());
        assert!(w.try_find(format!("topic-link-{topic}")).is_none());
        ids
    }).unwrap();
    cx.capture_screenshot(handle).unwrap().save(root.join("project-topic-groups.png")).unwrap();
    cx.update_window(handle, |_, w, cx| {
        w.click(format!("collection-topic-{moved_topic}"), cx);
        assert_eq!(app.read(cx).scope, Scope::Topic(moved_topic));
        app.update(cx, |a, cx| {
            a.navigate(Scope::Project(1), w, cx);
            a.query = "Moved goal one".into(); cx.notify();
        });
        w.render_frame(cx);
        assert!(w.find(format!("collection-topic-{moved_topic}")).label().unwrap().contains("1 goals"));
        assert!(w.find(format!("task-row-{moved_goal}")).visible());
        assert!(w.try_find(format!("collection-topic-{topic}")).is_none());
    }).unwrap();
    println!("Migration modal QA passed: {}", root.display());
}
