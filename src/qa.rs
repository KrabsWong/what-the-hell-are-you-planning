//! Headless acceptance for the research workflow.
use crate::app::TaskApp;
use gpui_kit::{component::{Theme, ThemeMode}, test::TestWindowExt, *};
use std::sync::Arc;

pub fn run() {
    crate::intake_qa::run();
}

fn live_context() -> HeadlessAppContext {
    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(assets::Assets),
        platform::current_headless_renderer,
    );
    cx.update(|cx| {
        init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
    });
    cx
}

/// Explicit real-browser acceptance. Uses the normal app controller and default persistent workspace.
pub fn run_research_live() {
    use std::time::{Duration, Instant};
    let work = std::env::current_dir()
        .unwrap()
        .join(".local/storage-research-2026");
    std::fs::create_dir_all(&work).unwrap();
    let brief=std::fs::read_to_string(work.join("research-goal.md")).expect("Place the approved research brief in .local/storage-research-2026/research-goal.md before this opt-in live run");
    let mut cx = live_context();
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
    let goal=cx.update_window(handle,|_,_,cx|app.update(cx,|a,cx|{
        assert!(a.storage_error.is_empty(),"{}",a.storage_error);
        if let Some(t)=a.store.tasks().iter().find(|t|t.title=="2026 年剩余时间：存储行业走势与投资机会"&&t.research.is_some()&&!t.archived){return t.id;}
        let project=a.store.projects().iter().map(|p|p.id).max().unwrap_or(0)+1;
        a.store.push_project(crate::domain::Project{research_brief:String::new(),id:project,name:"存储行业研究".into(),key:"MEM".into(),directory:work.to_string_lossy().into(),kind:crate::domain::ProjectKind::User});
        for (i,model) in ["deepseek/deepseek-flash","deepseek/deepseek-flash","deepseek/deepseek-v4-pro","deepseek/deepseek-v4-pro"].into_iter().enumerate(){if a.store.default_model().is_empty() && a.store.research_config().models[i].is_empty(){a.store.research_config_mut().models[i]=model.into();}}
        a.store.research_config_mut().ego_path="/Users/krabswang/.local/bin/ego-browser".into();
        let goal=a.store.create_research_goal("2026 年剩余时间：存储行业走势与投资机会".into(),format!("{brief}\n\n首次端到端研究请控制为 3 个子任务：一项搜集（覆盖多个来源）、一项分析、一项投资简报。搜集至少 5 个可访问原始来源；优先公开资料，不需要登录，无法获取的估值数据明确列为缺口。"),project).unwrap();
        a.persist_if_changed(true);cx.notify();goal
    })).unwrap();
    println!("Research goal {goal}; real OpenCode and ego(lite); workspace persisted");
    for planning in [true, false] {
        cx.update_window(handle, |_, _, cx| {
            app.update(cx, |a, cx| {
                if planning {
                    if !a.store.has_research_plan(goal) {
                        a.start_run(goal, cx);
                    }
                } else {
                    a.run_research_next(goal, true, cx);
                }
            })
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1800);
        let mut previous = String::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let active = cx
                .update_window(handle, |_, w, cx| {
                    app.update(cx, |a, cx| {
                        a.poll_runtime(cx);
                    });
                    let a = app.read(cx);
                    let summary = a
                        .store
                        .runs()
                        .last()
                        .map(|r| {
                            format!(
                                "Run {} / task {} / {} / {} events",
                                r.id,
                                r.task_id,
                                r.status,
                                r.logs.len()
                            )
                        })
                        .unwrap_or_default();
                    if summary != previous {
                        println!("{summary}");
                        previous = summary;
                    }
                    if a.active_runs.is_empty() && a.queued_runs.is_empty() {
                        println!("State: {} | {}", a.toast, a.error);
                    }
                    let active = !a.active_runs.is_empty();
                    w.render_frame(cx);
                    active
                })
                .unwrap();
            if !active {
                break;
            }
            if Instant::now() > deadline {
                cx.update_window(handle, |_, _, cx| app.update(cx, |a, cx| a.cancel_run(cx)))
                    .unwrap();
                panic!("Research timed out; cancellation requested");
            }
        }
        let okay = cx
            .update_window(handle, |_, _, cx| {
                let a = app.read(cx);
                (if planning {
                    a.store.has_research_plan(goal)
                } else {
                    a.store.next_research_task(goal) == Ok(None)
                }) && a.error.is_empty()
            })
            .unwrap();
        if !okay {
            println!("Research stopped; inspect Agent runs before retrying");
            break;
        }
    }
    cx.update_window(handle, |_, w, cx| {
        app.update(cx, |a, cx| {
            a.set_scope_identity(crate::app::Scope::Runs);
            a.selected = None;
            a.selected_run = a.store.runs().last().map(|r| r.id);
            a.show_run_result = true;
            cx.notify();
        });
        let a = app.read(cx);
        for id in a.store.research_children(goal) {
            if let Some(run) = a.store.research_result(id) {
                std::fs::write(work.join(format!("task-{id}-result.md")), &run.output).unwrap();
            }
        }
        println!("Remaining: {:?}", a.store.next_research_task(goal));
        w.render_frame(cx);
    })
    .unwrap();
    // Markdown parsing completes asynchronously after its first rendered frame.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update_window(handle, |_, w, cx| w.render_frame(cx))
            .unwrap();
    }
    cx.capture_screenshot(handle)
        .unwrap()
        .save(work.join("latest-research.png"))
        .unwrap();
}
