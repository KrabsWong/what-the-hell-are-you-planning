//! Explicit acceptance against the user's existing research and real local OpenCode.
use crate::{
    app::{Scope, TaskApp},
    appearance::Appearance,
    research::ResearchKind,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run() {
    let root = std::env::current_dir()
        .unwrap()
        .join(".local/storage-research-2026");
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(init);
    let mut app = None;
    let window = cx
        .open_window(size(px(1440.), px(920.)), |w, cx| {
            let view = cx.new(|cx| TaskApp::new(w, cx));
            app = Some(view.clone());
            cx.new(|cx| base::Root::new(view, w, cx))
        })
        .unwrap();
    let app = app.unwrap();
    let handle = window.into();
    let (goal, analysis, advice, collection_run) = cx
        .update_window(handle, |_, w, cx| {
            app.update(cx, |a, cx| {
                assert!(a.storage_error.is_empty(), "{}", a.storage_error);
                if a.store
                    .tasks
                    .iter()
                    .any(|t| t.id == 1 && t.title == "Polish task details and navigation")
                {
                    let backup = a
                        .storage
                        .as_ref()
                        .unwrap()
                        .remove_reviewed_demo(&mut a.store, &(1..=22).collect::<Vec<_>>())
                        .unwrap();
                    println!("Reviewed demo cleanup backup: {backup:?}");
                    a.persist_if_changed(true);
                }
                let goal = a
                    .store
                    .tasks
                    .iter()
                    .find(|t| {
                        t.title == "2026 年剩余时间：存储行业走势与投资机会"
                            && t.research
                                .as_ref()
                                .is_some_and(|r| r.kind == ResearchKind::Goal)
                    })
                    .unwrap()
                    .id;
                let children = a.store.research_children(goal);
                let find = |kind| {
                    *children
                        .iter()
                        .find(|id| {
                            a.store.task(**id).unwrap().research.as_ref().unwrap().kind == kind
                        })
                        .unwrap()
                };
                let analysis = find(ResearchKind::Analyze);
                let advice = find(ResearchKind::Advise);
                let collect = find(ResearchKind::Collect);
                let collection_run = a.store.research_result(collect).unwrap().id;
                a.research_project = a.store.task(goal).unwrap().project_id;
                a.connection_project = a.research_project;
                a.store.appearance = Appearance::Light;
                crate::appearance::apply(a.store.appearance, Some(w), cx);
                a.scope = Scope::Research;
                a.persist_if_changed(true);
                (goal, analysis, advice, collection_run)
            })
        })
        .unwrap();
    let names = ["Micron-Q4-FY26-Prepared-Remarks.pdf", "24-gfinance-mu.png"];
    let paths = [
        root.join("evidence").join(names[0]),
        root.join("evidence/screenshots").join(names[1]),
    ];
    for (name, path) in names.iter().zip(paths) {
        let exists = cx
            .update_window(handle, |_, _, cx| {
                app.read(cx)
                    .store
                    .task(analysis)
                    .unwrap()
                    .attachments
                    .iter()
                    .any(|a| &a.name == name)
            })
            .unwrap();
        if !exists {
            cx.update_window(handle, |_, _, cx| {
                app.update(cx, |a, cx| a.import_attachments(analysis, vec![path], cx))
            })
            .unwrap();
            wait(&mut cx, handle, &app, Duration::from_secs(360));
            cx.update_window(handle, |_, _, cx| {
                let a = app.read(cx);
                assert!(a.error.is_empty(), "{}", a.error);
                assert!(
                    a.store
                        .task(analysis)
                        .unwrap()
                        .attachments
                        .iter()
                        .any(|a| &a.name == name)
                );
                assert!(a.store.research_result(analysis).is_none());
                assert!(a.store.research_result(advice).is_none());
                assert_eq!(a.store.next_research_task(goal).unwrap(), Some(analysis));
            })
            .unwrap();
            println!("Imported and extracted {name}; analysis + advice invalidated");
        }
    }
    cx.update_window(handle, |_,_,cx| app.update(cx, |a,cx| {
        let comment = "请将这次补充的 Micron-Q4-FY26-Prepared-Remarks.pdf 和 24-gfinance-mu.png 纳入重新分析。单独写‘补充材料核对’：分别引用完整文件名、PDF 页码或截图页、至少一个从材料实际读取的字段，并说明与此前结论一致或不同之处。不要将行情截图当成未来预测。后续投资简报应保留这两份材料的引用和核对结论。使用本次运行提供的本地提取正文，不需要重新浏览网站。";
        if !a.store.task(analysis).unwrap().comments.iter().any(|c| c.body==comment) {
            a.store.add_comment(analysis,comment).unwrap();
        }
        if a.store.research_result(advice).is_some_and(|r| names.iter().any(|name| !r.output.contains(name))) {
            a.store.add_comment(advice, "本次补充材料验收：最终简报必须明确引用 Micron-Q4-FY26-Prepared-Remarks.pdf 与 24-gfinance-mu.png 两个完整文件名，写明页码或截图页、实际核对字段、以及对前一版结论的影响。请使用新的分析结果和随运行提供的材料转录更新简报，不要沿用旧版文件正文。").unwrap();
        }
        a.persist_if_changed(true);
        a.run_research_next(goal,true,cx);
    })).unwrap();
    wait(&mut cx, handle, &app, Duration::from_secs(1800));
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            assert!(a.error.is_empty(), "{}", a.error);
            assert_eq!(a.store.next_research_task(goal).unwrap(), None);
            assert!(
                a.store
                    .runs
                    .iter()
                    .any(|r| r.id == collection_run && r.status == "Succeeded")
            );
            for id in [analysis, advice] {
                let result = a.store.research_result(id).unwrap();
                for name in names {
                    assert!(
                        result.output.contains(name),
                        "Result {} must cite material {}",
                        result.id,
                        name
                    );
                }
                assert!(result.attachments.len() >= 2);
                std::fs::write(
                    root.join(format!("task-{id}-with-materials.md")),
                    &result.output,
                )
                .unwrap();
                println!(
                    "Verified run {} task {}: {} sources, {} material snapshots",
                    result.id,
                    id,
                    result.sources.len(),
                    result.attachments.len()
                );
            }
            a.scope = Scope::Runs;
            a.selected = None;
            a.selected_run = Some(a.store.research_result(advice).unwrap().id);
            a.show_run_result = true;
            a.store.appearance = Appearance::Light;
            crate::appearance::apply(a.store.appearance, Some(w), cx);
            a.persist_if_changed(true);
            cx.notify();
        })
    })
    .unwrap();
    render(&mut cx, handle);
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("materials-result-light.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let next = if app.read(cx).store.appearance == Appearance::Light {
            2usize
        } else {
            1usize
        };
        w.click("toggle-appearance", cx);
        if next == 1 && app.read(cx).store.appearance == Appearance::System {
            w.click("toggle-appearance", cx);
        }
        assert_eq!(app.read(cx).store.appearance, Appearance::Dark);
    })
    .unwrap();
    render(&mut cx, handle);
    cx.capture_screenshot(handle)
        .unwrap()
        .save(root.join("materials-result-dark.png"))
        .unwrap();
    cx.update_window(handle, |_, w, cx| {
        let next = if app.read(cx).store.appearance == Appearance::Light {
            2usize
        } else {
            1usize
        };
        w.click("toggle-appearance", cx);
        if next == 1 && app.read(cx).store.appearance == Appearance::System {
            w.click("toggle-appearance", cx);
        }
    })
    .unwrap();
    cx.update_window(handle, |_, _, cx| {
        let a = app.read(cx);
        assert_eq!(
            a.storage.as_ref().unwrap().load().unwrap().unwrap(),
            a.store
        );
    })
    .unwrap();
    println!(
        "MATERIAL LIVE PASSED: actual PDF/OCR, downstream reruns, preserved upstream, cited final report, themes and persistence."
    );
}
fn wait(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<TaskApp>,
    duration: Duration,
) {
    let until = Instant::now() + duration;
    let mut previous = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let busy = cx
            .update_window(handle, |_, w, cx| {
                app.update(cx, |a, cx| {
                    a.poll_runtime(cx);
                });
                let a = app.read(cx);
                let state = format!(
                    "{:?} import={} {}",
                    a.store
                        .runs
                        .last()
                        .map(|r| (r.id, r.status.as_str(), r.logs.len())),
                    a.attachment_import.is_some(),
                    a.error
                );
                if state != previous {
                    println!("{state}");
                    previous = state;
                }
                let busy = !a.active_runs.is_empty() || a.attachment_import.is_some();
                w.render_frame(cx);
                busy
            })
            .unwrap();
        if !busy {
            break;
        }
        if Instant::now() > until {
            cx.update_window(handle, |_, _, cx| app.update(cx, |a, cx| a.cancel_run(cx)))
                .unwrap();
            panic!("Live acceptance timed out");
        }
    }
}
fn render(cx: &mut HeadlessAppContext, handle: AnyWindowHandle) {
    for _ in 0..10 {
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Isolated native acceptance for empty startup, non-1 projects and theme persistence.
pub fn workspace() {
    use crate::{domain::Store, persistence::Storage};
    let root = std::env::current_dir()
        .unwrap()
        .join(format!(".local/workspace-qa/{}", std::process::id()));
    let storage = Storage::new(root.join("workspace.json"));
    let mut empty = Store::empty();
    empty.projects[0].id = 3;
    storage.save(&empty).unwrap();
    for (width, height) in [(1440., 920.), (1000., 680.)] {
        let mut cx = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(assets::Assets),
            platform::current_headless_renderer,
        );
        cx.update(init);
        let mut app = None;
        let window = cx
            .open_window(size(px(width), px(height)), |w, cx| {
                let view = cx.new(|cx| TaskApp::new_mock(w, cx));
                view.update(cx, |a, cx| {
                    a.attach_storage(Storage::new(root.join("workspace.json")), w, cx)
                });
                app = Some(view.clone());
                cx.new(|cx| base::Root::new(view, w, cx))
            })
            .unwrap();
        let app = app.unwrap();
        let handle = window.into();
        cx.update_window(handle, |_, _, cx| {
            assert!(app.read(cx).storage_error.is_empty());
            assert_eq!(app.read(cx).store.appearance, Appearance::Light);
        })
        .unwrap();
        render(&mut cx, handle);
        cx.capture_screenshot(handle)
            .unwrap()
            .save(root.join(format!("empty-light-{width}.png")))
            .unwrap();
        if width == 1440. {
            cx.update_window(handle, |_, w, cx| {
                assert!(app.read(cx).store.tasks.is_empty());
                assert!(app.read(cx).store.runs.is_empty());
                w.click("new-task", cx);
                w.input("真实工作区：补充材料与研究任务", cx);
                w.click("save-task", cx);
                let a = app.read(cx);
                assert!(a.error.is_empty(), "{}", a.error);
                assert_eq!(a.store.tasks.len(), 1);
                assert_eq!(a.store.tasks[0].project_id, 3);
                w.click("navigation-back", cx);
            })
            .unwrap();
        }
        cx.update_window(handle, |_, w, cx| {
            w.click("research", cx);
        })
        .unwrap();
        render(&mut cx, handle);
        cx.capture_screenshot(handle)
            .unwrap()
            .save(root.join(format!("research-light-{width}.png")))
            .unwrap();
        cx.update_window(handle, |_, w, cx| {
            let next = if app.read(cx).store.appearance == Appearance::Light {
                2usize
            } else {
                1usize
            };
            w.click("toggle-appearance", cx);
            if next == 1 && app.read(cx).store.appearance == Appearance::System {
                w.click("toggle-appearance", cx);
            }
            assert_eq!(app.read(cx).store.appearance, Appearance::Dark);
        })
        .unwrap();
        render(&mut cx, handle);
        cx.capture_screenshot(handle)
            .unwrap()
            .save(root.join(format!("research-dark-{width}.png")))
            .unwrap();
        cx.update_window(handle, |_, w, cx| {
            assert_eq!(
                app.read(cx)
                    .storage
                    .as_ref()
                    .unwrap()
                    .load()
                    .unwrap()
                    .unwrap()
                    .appearance,
                Appearance::Dark
            );
            let next = if app.read(cx).store.appearance == Appearance::Light {
                2usize
            } else {
                1usize
            };
            w.click("toggle-appearance", cx);
            if next == 1 && app.read(cx).store.appearance == Appearance::System {
                w.click("toggle-appearance", cx);
            }
        })
        .unwrap();
        cx.update_window(handle, |_, w, _| w.remove_window())
            .unwrap();
        drop(app);
        drop(cx);
    }
    println!(
        "WORKSPACE QA passed: empty startup, valid project fallback, light/dark toggle, persistence, 1440/1000 rendering. {}",
        root.display()
    );
}
