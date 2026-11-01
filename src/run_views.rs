use crate::app::Scope;
use crate::{
    app::*,
    appearance::palette,
    domain::{AgentRun, Store},
    research::ResearchKind,
    views::*,
};
use gpui_kit::base::{h_resizable, resizable_panel};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::{
    component::{Disableable, Selectable, checkbox::Checkbox, menu::DropdownMenu},
    prelude::FluentBuilder,
    *,
};

// Fixed bounds keep status labels still while the ring expands and fades.
fn running_indicator(id: &'static str, color: u32) -> AnyElement {
    div()
        .id(id)
        .relative()
        .size(px(16.))
        .flex_shrink_0()
        .child(
            div()
                .absolute()
                .rounded_full()
                .border_1()
                .border_color(rgb(color))
                .with_animation(
                    "ripple",
                    Animation::new(std::time::Duration::from_millis(1800)).repeat(),
                    |ring, phase| {
                        let size = 6. + 10. * phase;
                        ring.size(px(size))
                            .left(px((16. - size) / 2.))
                            .top(px((16. - size) / 2.))
                            .opacity(0.65 * (1. - phase))
                    },
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(5.))
                .top(px(5.))
                .size(px(6.))
                .rounded_full()
                .bg(rgb(color))
                .with_animation(
                    "breath",
                    Animation::new(std::time::Duration::from_millis(1800)).repeat(),
                    |dot, phase| dot.opacity(0.75 + 0.25 * (phase * std::f32::consts::TAU).cos()),
                ),
        )
        .into_any_element()
}

fn status_label(status: &str) -> &str {
    match status {
        "Succeeded" => "Done",
        "Blocked" => "Needs your attention",
        "Stale" => "Needs updating",
        "Cancelling" => "Stopping…",
        "Recovering" => "Running in background",
        "Cancelled" => "Stopped",
        other => other,
    }
}

fn attempt_indicator(run: &AgentRun, cx: &App) -> gpui_kit::component::button::Button {
    use crate::execution_status::ExecutionStatus as Status;
    let status = match run.status.as_str() {
        "Succeeded" => Status::Done,
        "Running" | "Recovering" | "Cancelling" => Status::Running,
        "Queued" if run.auto_retry_attempt > 0 => Status::Retrying,
        "Queued" => Status::Queued,
        _ => Status::Blocked,
    };
    quiet_button(format!("attempt-status-{}", run.id), "")
        .w(px(20.)).h(px(20.)).p_0().flex_shrink_0()
        .child(status.icon_view(cx))
        .accessibility_label(format!("Attempt: {}", status_label(&run.status)))
        .tooltip(format!("This attempt: {}{}", status_label(&run.status),
            if run.auto_retry_attempt > 0 { format!(" · Retry {}/3", run.auto_retry_attempt) } else { String::new() }))
}

fn activity_label(run: &AgentRun) -> &'static str {
    if run.project_plan.is_some() {
        return "Planning research topics";
    }
    match run.research_kind {
        Some(ResearchKind::Goal) => "Planning research steps",
        Some(ResearchKind::Collect) => "Collecting evidence",
        Some(ResearchKind::Analyze) => "Analyzing findings",
        Some(ResearchKind::Advise) => "Writing the report",
        None => "Working on the task",
    }
}

// Group by stable identities. A moved task follows its current project; missing tasks
// keep their history in an unlinked section instead of disappearing.
struct RunTopic<'a> {
    id: Option<u64>,
    runs: Vec<&'a AgentRun>,
}
struct RunProject<'a> {
    id: Option<u64>,
    topics: Vec<RunTopic<'a>>,
}
fn run_projects(store: &Store) -> Vec<RunProject<'_>> {
    let mut groups: Vec<RunProject<'_>> = vec![];
    for run in store.runs.iter().rev().filter(|r| !store.run_is_archived(r)) {
        let task = store.task(run.task_id);
        let project = run.project_plan.or_else(|| task.map(|t| t.project_id));
        let goal = if run.project_plan.is_some() {
            None
        } else {
            Some(
                task.and_then(|t| t.research.as_ref())
                    .map(|r| r.goal_id)
                    .unwrap_or(run.task_id),
            )
        };
        let i = groups
            .iter()
            .position(|g| g.id == project)
            .unwrap_or_else(|| {
                groups.push(RunProject {
                    id: project,
                    topics: vec![],
                });
                groups.len() - 1
            });
        let topics = &mut groups[i].topics;
        let j = topics.iter().position(|g| g.id == goal).unwrap_or_else(|| {
            topics.push(RunTopic {
                id: goal,
                runs: vec![],
            });
            topics.len() - 1
        });
        topics[j].runs.push(run);
    }
    groups.sort_by_key(|group| {
        store
            .projects
            .iter()
            .position(|p| Some(p.id) == group.id)
            .unwrap_or(usize::MAX)
    });
    for group in &mut groups {
        group.topics.sort_by_key(|topic| {
            topic
                .id
                .and_then(|id| store.tasks.iter().position(|t| t.id == id))
                .unwrap_or(usize::MAX)
        });
    }
    groups
}

fn run_location(store: &Store, run: &AgentRun) -> (Option<u64>, Option<u64>) {
    let task = store.task(run.task_id);
    let project = run.project_plan.or_else(|| task.map(|t| t.project_id));
    let topic = if run.project_plan.is_some() {
        None
    } else {
        Some(
            task.and_then(|t| t.research.as_ref())
                .map(|r| r.goal_id)
                .unwrap_or(run.task_id),
        )
    };
    (project, topic)
}

impl TaskApp {
    pub fn reveal_selected_run(&mut self) {
        self.result_scroll.set_offset(point(px(0.), px(0.)));
        self.expanded_run_sources = None;
        self.show_planning_activity = self
            .selected_run
            .and_then(|id| self.store.runs.iter().find(|r| r.id == id))
            .or_else(|| self.store.runs.last())
            .is_some_and(|r| {
                r.project_plan.is_some() || r.research_kind == Some(ResearchKind::Goal)
            });
    }

    fn select_run(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected_run = Some(id);
        self.reveal_selected_run();
        self.show_run_details = false;
        self.show_run_result = true;
        self.log_scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(crate) fn runs(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let selected = self
            .selected_run
            .and_then(|id| self.store.runs.iter().find(|r| r.id == id))
            .filter(|r| self.task_result_context.is_some() || !self.store.run_is_archived(r))
            .or_else(|| self.store.runs.iter().rev().find(|r| !self.store.run_is_archived(r)));
        if self.task_result_context.is_some()
            && let Some(run) = selected
        {
            return self.run_content(run, cx);
        }
        let groups = run_projects(&self.store);
        let location = selected.map(|run| run_location(&self.store, run));
        let project = groups.iter().find(|p| Some(p.id) == location.map(|l| l.0));
        let topic = project.and_then(|p| {
            p.topics
                .iter()
                .find(|t| Some(t.id) == location.map(|l| l.1))
        });
        let mut history = col()
            .id("run-navigation")
            .w_full()
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .border_r_1()
            .border_color(rgb(colors.line));
        if let (Some(project), Some(topic)) = (project, topic) {
            let name = project
                .id
                .and_then(|id| self.store.projects.iter().find(|p| p.id == id))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unlinked history".into());
            let topic_task = topic.id.and_then(|id| self.store.task(id));
            let research = topic_task.is_some_and(|t| t.research.is_some());
            let topic_title = topic_task.map(|t| t.title.clone()).unwrap_or_else(|| {
                if topic.id.is_none() {
                    "Project planning".into()
                } else {
                    topic.runs[0].task_title.clone()
                }
            });
            let project_options: Vec<_> = groups
                .iter()
                .map(|p| {
                    let name =
                        p.id.and_then(|id| self.store.projects.iter().find(|p| p.id == id))
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Unlinked history".into());
                    let run = p
                        .topics
                        .iter()
                        .flat_map(|t| t.runs.iter())
                        .max_by_key(|r| r.id)
                        .unwrap();
                    (p.id, name, run.id)
                })
                .collect();
            let actual_topic = topic_task.and_then(|t| self.store.task_topic(t.id));
            let selected_topic = actual_topic.map(|t| t.id);
            let mut topic_filters = Vec::new();
            for group in &project.topics {
                if let Some(t) = group.id.and_then(|id| self.store.task_topic(id)) {
                    if !topic_filters.iter().any(|(id, _, _)| *id == t.id) {
                        topic_filters.push((t.id, t.display_name(), group.runs[0].id));
                    }
                }
            }
            let planning_options: Vec<_> = project.topics.iter().flat_map(|group| group.runs.iter())
                .filter(|run| run.project_plan.is_some()
                    || (topic.id.is_some() && run.task_id == topic.id.unwrap()
                        && run.research_kind == Some(ResearchKind::Goal)))
                .map(|run| (run.id, format!("{} · {} · {}", if run.project_plan.is_some() { "Project plan" } else { "Goal plan" },
                    if run.started_at.is_empty() { &run.queued_at } else { &run.started_at }, status_label(&run.status))))
                .collect();
            let planning_entity = cx.entity().downgrade();
            let topic_options: Vec<_> = project
                .topics
                .iter()
                .filter(|t| t.id.is_some() && t.id.and_then(|id| self.store.task_topic(id)).map(|t| t.id) == selected_topic)
                .map(|t| {
                    let name =
                        t.id.and_then(|id| self.store.task(id))
                            .map(|task| format!("{} · {}", self.store.identifier(task), task.title))
                            .unwrap_or_else(|| {
                                if t.id.is_none() {
                                    "Project planning".into()
                                } else {
                                    t.runs[0].task_title.clone()
                                }
                            });
                    (
                        t.id,
                        name,
                        t.runs
                            .iter()
                            .find(|r| {
                                r.research_kind == Some(ResearchKind::Advise)
                                    && self
                                        .store
                                        .research_result(r.task_id)
                                        .is_some_and(|valid| valid.id == r.id)
                            })
                            .unwrap_or(&t.runs[0])
                            .id,
                    )
                })
                .collect();
            let pid = project.id;
            let tid = topic.id;
            let entity = cx.entity().downgrade();
            let topic_entity = entity.clone();
            let filter_entity = entity.clone();
            let goal_title = topic_task.map(|t| format!("{} · {}", self.store.identifier(t), t.title))
                .unwrap_or(topic_title);
            history = history.child(
                col().flex_shrink_0().px_3().py_3().gap_2()
                    .border_b_1().border_color(rgb(colors.line))
                    .child(label("Project", cx).text_xs())
                    .child(button("run-project-picker", name.clone()).w_full()
                        .dropdown_caret(true).tooltip("Choose project")
                        .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                            for (id, name, rid) in &project_options {
                                let entity = entity.clone(); let rid = *rid;
                                menu = menu.item(task_menu_item(name.clone()).checked(*id == pid)
                                    .on_click(move |_, _, cx| { let _ = entity.update(cx, |a, cx| a.select_run(rid, cx)); }));
                            }
                            menu
                        }))
                    .child(label("Topic", cx).text_xs())
                    .child(gpui_kit::component::button::Button::new("run-topic-filter")
                        .w_full().h_auto().py_2().px_2().dropdown_caret(true)
                        .disabled(topic_filters.is_empty())
                        .child(div().flex_1().min_w_0().text_left().whitespace_normal()
                            .child(actual_topic.map(|t| t.display_name()).unwrap_or_else(|| "Choose topic".into())))
                        .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                            for (id, name, rid) in &topic_filters {
                                let entity = filter_entity.clone(); let rid = *rid;
                                menu = menu.item(task_menu_item(name.clone()).checked(Some(*id) == selected_topic)
                                    .on_click(move |_, _, cx| { let _ = entity.update(cx, |a, cx| a.select_run(rid, cx)); }));
                            }
                            menu
                        }))
                    .when(topic_task.is_some(), |d| d
                    .child(label(if research { "Goal" } else { "Activity" }, cx).text_xs())
                    .child(gpui_kit::component::button::Button::new("run-topic-picker")
                        .w_full().h_auto().py_2().px_2().dropdown_caret(true)
                        .child(div().flex_1().min_w_0().text_left().whitespace_normal()
                            .text_sm().line_height(relative(1.4)).child(goal_title))
                        .tooltip("Choose goal or planning activity")
                        .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                            for (id, name, rid) in &topic_options {
                                let entity = topic_entity.clone(); let rid = *rid;
                                menu = menu.item(task_menu_item(name.clone()).checked(*id == tid)
                                    .on_click(move |_, _, cx| { let _ = entity.update(cx, |a, cx| a.select_run(rid, cx)); }));
                            }
                            menu
                        }))
                    )
                    .child(row().flex_wrap().gap_3()
                        .when_some(topic_task, |d, task| {
                            let id = task.id;
                            d.child(quiet_button("open-run-goal", "Goal details →")
                                .h_6().px_0().text_xs().text_color(rgb(colors.accent))
                                .on_click(cx.listener(move |a, _, w, cx| a.open_task(id, w, cx))))
                        })
                        .when(!planning_options.is_empty(), |d| d.child(
                            quiet_button("run-project-planning", "Planning history")
                                .h_6().px_0().text_xs().text_color(rgb(colors.accent)).dropdown_caret(true)
                                .tooltip("Project and current goal planning records")
                                .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                                    for (rid, title) in &planning_options {
                                        let entity = planning_entity.clone(); let rid = *rid;
                                        menu = menu.item(task_menu_item(title.clone()).on_click(move |_, _, cx| {
                                            let _ = entity.update(cx, |a, cx| a.select_run(rid, cx));
                                        }));
                                    }
                                    menu
                                })
                        ))),
            );

            let mut latest: Vec<&AgentRun> = vec![];
            for run in &topic.runs {
                if !latest.iter().any(|r| r.task_id == run.task_id) {
                    latest.push(run);
                }
            }
            latest.sort_by_key(|r| {
                self.store
                    .tasks
                    .iter()
                    .position(|t| t.id == r.task_id)
                    .unwrap_or(usize::MAX)
            });
            let (planning, tasks): (Vec<_>, Vec<_>) = latest.into_iter().partition(|r| {
                r.project_plan.is_some() || r.research_kind == Some(ResearchKind::Goal)
            });
            let (reports, tasks): (Vec<_>, Vec<_>) = tasks
                .into_iter()
                .partition(|r| r.research_kind == Some(ResearchKind::Advise));
            let mut entries = col()
                .id("run-history")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_2()
                .gap_1();
            if !reports.is_empty() {
                entries = entries.child(
                    div()
                        .px_2()
                        .pt_1()
                        .pb_1()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Final report"),
                );
                for run in reports {
                    entries = entries.child(self.run_history_entry(run, selected, cx));
                }
            }
            if !tasks.is_empty() {
                entries = entries.child(
                    label(format!("Steps with activity · {}", tasks.len()), cx)
                        .px_2()
                        .pt_2()
                        .pb_1(),
                );
            }
            for run in tasks {
                entries = entries.child(self.run_history_entry(run, selected, cx));
            }
            if topic
                .runs
                .iter()
                .all(|r| r.project_plan.is_some() || r.research_kind == Some(ResearchKind::Goal))
            {
                entries = entries.child(
                    label(
                        "No task activity yet. Review the proposed tasks before starting research.",
                        cx,
                    )
                    .px_2()
                    .pb_3(),
                );
            }
            if self.show_planning_activity && !planning.is_empty() {
                entries = entries.child(label("Planning records", cx).px_2().py_2())
                    .children(planning.iter().map(|r| self.run_history_entry(r, selected, cx)));
            }
            history = history.child(entries.map(observed));
        }
        let pane = if let Some(run) = selected {
            self.run_content(run, cx)
        } else {
            col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_4()
                .child(div().text_2xl().child("Your results will appear here"))
                .child(muted("Choose a task in your project to get started.", cx))
                .child(
                    primary_button("runs-to-tasks", "Go to tasks")
                        .on_click(cx.listener(|a, _, w, cx| a.navigate(Scope::All, w, cx))),
                )
                .into_any_element()
        };
        col()
            .size_full()
            .child(self.todo_batch_bar(None, cx))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("results-split")
                        .with_state(&self.results_split)
                        .child(
                            resizable_panel()
                                .size(px(260.))
                                .size_range(px(200.)..px(520.))
                                .fixed()
                                .visible(selected.is_some())
                                .child(history.map(observed)),
                        )
                        .child(
                            resizable_panel()
                                .size_range(px(240.)..Pixels::MAX)
                                .child(pane),
                        ),
                ),
            )
            .into_any_element()
    }

    fn run_history_entry(
        &self,
        run: &AgentRun,
        selected: Option<&AgentRun>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let rid = run.id;
        let active = selected
            .is_some_and(|s| s.task_id == run.task_id && s.project_plan == run.project_plan);
        let planning = run.project_plan.is_some() || run.research_kind == Some(ResearchKind::Goal);
        let title = if run.project_plan.is_some() {
            "Generate research topics".into()
        } else if planning {
            "Break down research goal".into()
        } else {
            run.task_title.clone()
        };
        let kind = match run.research_kind {
            Some(ResearchKind::Collect) => "Evidence",
            Some(ResearchKind::Analyze) => "Analysis",
            Some(ResearchKind::Advise) => "Final report",
            _ => "Task",
        };
        let identity = if planning {
            "Planning record".into()
        } else {
            self.store
                .task(run.task_id)
                .map(|t| format!("{} · {kind}", self.store.identifier(t)))
                .unwrap_or_else(|| format!("Task {} · saved activity", run.task_id))
        };
        quiet_button(format!("run-{rid}"), "")
            .custom(
                gpui_kit::component::button::ButtonCustomVariant::new(cx)
                    .foreground(rgb(colors.text).into())
                    .hover(rgb(colors.hover).into())
                    .active(rgb(colors.hover).into()),
            )
            .w_full()
            .h_auto()
            .px_3()
            .py_2()
            .rounded(px(5.))
            .selected(active)
            .tooltip(title.clone())
            .accessibility_label(format!("{identity}: {title}"))
            .when(run.research_kind == Some(ResearchKind::Advise), |b| {
                b.border_1()
                    .border_color(rgb(colors.line))
                    .bg(rgb(colors.panel))
            })
            .when(active, |b| b.bg(rgb(colors.accent_surface)))
            .child(
                col()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .id(format!("run-title-{rid}"))
                            .role(Role::Label)
                            .aria_label(title.clone())
                            .w_full()
                            .whitespace_normal()
                            .text_sm()
                            .line_height(relative(1.4))
                            .child(title)
                            .map(observed),
                    )
                    .child(
                        row()
                            .flex_wrap()
                            .gap_1()
                            .child(
                                label(identity, cx)
                                    .px_2()
                                    .py(px(2.))
                                    .rounded_sm()
                                    .text_color(rgb(colors.tag_text))
                                    .bg(rgb(colors.tag_surface)),
                            )
                            .child(attempt_indicator(run, cx)),
                    ),
            )
            .on_click(cx.listener(move |a, _, _, cx| a.select_run(rid, cx)))
            .into_any_element()
    }

    fn run_version_picker(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let attempts: Vec<_> = self
            .store
            .runs
            .iter()
            .rev()
            .filter(|r| r.task_id == run.task_id && r.project_plan == run.project_plan)
            .collect();
        let latest = attempts.first().is_some_and(|r| r.id == run.id);
        let caption = format!(
            "{} · {}",
            if latest { "Latest" } else { "Earlier version" },
            (if run.started_at.is_empty() {
                &run.queued_at
            } else {
                &run.started_at
            })
            .chars()
            .take(16)
            .collect::<String>()
        );
        if attempts.len() < 2 {
            return label(caption, cx).into_any_element();
        }
        let selected = run.id;
        let options: Vec<_> = attempts
            .iter()
            .enumerate()
            .map(|(i, r)| {
                (
                    r.id,
                    format!(
                        "{}{} · {}",
                        if i == 0 { "Latest · " } else { "" },
                        (if r.started_at.is_empty() {
                            &r.queued_at
                        } else {
                            &r.started_at
                        })
                        .chars()
                        .take(19)
                        .collect::<String>(),
                        status_label(&r.status)
                    ),
                )
            })
            .collect();
        let entity = cx.entity().downgrade();
        quiet_button("run-version-picker", caption)
            .px_0()
            .text_xs()
            .text_color(rgb(palette(cx).muted))
            .dropdown_caret(true)
            .tooltip("View earlier attempts for this task")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for (id, name) in &options {
                    let entity = entity.clone();
                    let id = *id;
                    menu = menu.item(
                        task_menu_item(name.clone())
                            .checked(id == selected)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |a, cx| a.select_run(id, cx));
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn run_content(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let active = self.active_runs.iter().any(|a| a.id == run.id)
            || self.queued_runs.iter().any(|(id, _)| *id == run.id);
        let run_goal = self.run_goal(run.id);
        let id = run.task_id;
        let project_plan = run.project_plan;
        let project_id = project_plan.or_else(|| self.store.task(id).map(|t| t.project_id));
        let can_open =
            project_id.is_some() && (project_plan.is_some() || self.store.task(id).is_some());
        let browser_blocked = run.logs.iter().any(|l| l.starts_with("[blocked]"));
        let planning = project_plan.is_some() || run.research_kind == Some(ResearchKind::Goal);
        // Research streams contain a JSON protocol until the lifecycle validates the result.
        let readable = !planning
            && (run.status == "Succeeded" || (run.status == "Blocked" && !browser_blocked));
        let (headline, explanation) = match run.status.as_str() {
            "Queued" => (
                "Waiting to start",
                "This task will start when a worker and its working folder/browser are available.",
            ),
            "Running" => (
                activity_label(run),
                "You can leave this page. The result will appear here when this step finishes.",
            ),
            "Recovering" => (
                "Previous session is still running",
                "OpenCode is continuing this task in the background. The app checks its status and imports the result when it finishes; no duplicate run is started.",
            ),
            "Interrupted" => (
                "No completed result for this attempt",
                "The previous attempt was interrupted. Open the task to check its session and retry; completed prerequisite results will be reused.",
            ),
            "Cancelling" => (
                "Stopping this task",
                "Waiting for the current process to stop.",
            ),
            "Succeeded" if planning => (
                "Your research tasks are ready",
                "Open the project or task to review the proposed work and choose what to start.",
            ),
            "Succeeded" => (
                "Result ready",
                "Read the result below and check its sources before using the conclusions.",
            ),
            "Blocked" if browser_blocked => (
                "The browser needs your attention",
                "Open the research browser to handle its prompt. Return to the task when you are ready to continue.",
            ),
            "Blocked" => (
                "More information is needed",
                "Review the explanation below, then open the task to add the missing information.",
            ),
            "Stale" => (
                "The task has changed",
                "Materials or earlier results changed during this run. Open the task and run it again to update the result.",
            ),
            "Cancelled" => (
                "This task was stopped",
                "Open the task when you want to start it again. Previous completed results are kept.",
            ),
            _ => (
                "This attempt did not finish",
                "Open the task to check its setup and try again. Technical details contain the error and saved output.",
            ),
        };
        let headline = if run.status == "Queued" && run.auto_retry_attempt > 0 {
            "Waiting to retry"
        } else { headline };
        let queue_explanation = if run.status == "Queued" && run.auto_retry_attempt > 0 {
            if self.todo_batch.as_ref().is_some_and(|batch|
                batch.run_ids.contains_key(&run.id) && batch.state != crate::batch::BatchState::Running) {
                format!("Retry {}/3 is paused. Continue the batch to resume, or Stop task to cancel.", run.auto_retry_attempt)
            } else {
                let seconds = run.retry_delay_remaining(chrono::Utc::now().timestamp());
                if seconds > 0 {
                    format!("This attempt failed. Automatic retry {}/3 starts in {seconds} seconds. Stop task cancels the retry.", run.auto_retry_attempt)
                } else {
                    format!("Automatic retry {}/3 is waiting for an execution slot or shared resource. Stop task cancels the retry.", run.auto_retry_attempt)
                }
            }
        } else if run.status == "Queued" {
            let conflicts: Vec<_> = self
                .active_runs
                .iter()
                .filter_map(|active| {
                    self.store
                        .runs
                        .iter()
                        .find(|r| r.id == active.id)
                        .filter(|r| self.store.run_conflicts(run, r))
                        .map(|r| {
                            format!(
                                "{} · {} ({})",
                                self.store
                                    .task(r.task_id)
                                    .map(|t| self.store.identifier(t))
                                    .unwrap_or_default(),
                                r.task_title,
                                self.store
                                    .run_conflict_reason(run, r)
                                    .unwrap_or("resource busy")
                            )
                        })
                })
                .collect();
            if !self.storage_error.is_empty() {
                format!("Waiting for local storage: {}", self.storage_error)
            } else if !conflicts.is_empty() {
                format!(
                    "Queued behind: {}. Starts automatically when this resource is free.",
                    conflicts.join("; ")
                )
            } else if self.active_runs.len() >= crate::scheduler::MAX_CONCURRENT_RUNS {
                format!(
                    "All {} execution slots are busy. This task is queued and will start automatically when a slot is free.",
                    crate::scheduler::MAX_CONCURRENT_RUNS
                )
            } else {
                "Queued · preparing to start automatically.".into()
            }
        } else if run.status == "Failed" && run.auto_retry_attempt >= crate::retries::MAX_AUTO_RETRIES {
            "This task still failed after 3 automatic retries. Review the attempt details and fix any setup issues before retrying manually.".into()
        } else if let Some(crate::recovery::SessionCheck::Unknown(error)) =
            self.recovery.checks.get(&run.id)
        {
            format!(
                "Session check failed: {error}. Use Check progress in Paused projects to retry the check."
            )
        } else {
            explanation.to_string()
        };
        let context_task = self.task_result_context;
        let mut actions = row().flex_wrap().gap_2().child(
            quiet_button(
                "open-run-task",
                if project_plan.is_some() {
                    "Project details"
                } else if context_task.is_some() {
                    "Back to task"
                } else {
                    "Task details"
                },
            )
            .disabled(!can_open)
            .on_click(cx.listener(move |this, _, w, cx| {
                if let Some(pid) = project_plan {
                    this.navigate(Scope::Project(pid), w, cx);
                    this.layout = Layout::Board;
                } else {
                    this.open_task(context_task.unwrap_or(id), w, cx);
                }
            })),
        );
        actions = actions.child(
            quiet_button(
                if self.show_run_details {
                    "show-result"
                } else {
                    "run-technical-details"
                },
                if self.show_run_details {
                    "Back to result"
                } else {
                    "Technical details"
                },
            )
            .icon(if self.show_run_details {
                assets::IconName::ArrowLeft
            } else {
                assets::IconName::Code
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.show_run_details = !this.show_run_details;
                this.show_run_result = !this.show_run_details;
                cx.notify();
            })),
        );
        if browser_blocked || (active && run.research_kind == Some(ResearchKind::Collect)) {
            actions = actions.child(
                quiet_button("run-open-browser", "Open browser")
                    .icon(assets::IconName::ExternalLink)
                    .on_click(cx.listener(|this, _, _, cx| this.open_ego(cx))),
            );
        }
        if active {
            if run_goal.is_some_and(|goal| self.research_sequences.contains(&goal)) {
                actions = actions.child(
                    quiet_button("pause-research", "Pause after this step").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if let Some(goal) = run_goal {
                                this.research_sequences.remove(&goal);
                            }
                            this.toast = "Research will pause after this step".into();
                            cx.notify();
                        },
                    )),
                );
            }
            actions = actions.child(
                quiet_button("cancel-run", "Stop task")
                    .text_color(rgb(colors.danger))
                    .disabled(run.status == "Cancelling")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_run(cx))),
            );
        }
        let progress = col()
            .id("run-progress")
            .gap_2()
            .flex_shrink_0()
            .child(
                row()
                    .flex_wrap()
                    .justify_between()
                    .gap_3()
                    .child(
                        row()
                            .flex_wrap()
                            .gap_3()
                            .child(attempt_indicator(run, cx))
                            .child(self.run_version_picker(run, cx)),
                    )
                    .child(actions),
            )
            .when((run.status != "Succeeded" || planning) && (readable || self.show_run_details), |d| {
                d.child(label(queue_explanation.clone(), cx))
            })
            .map(observed);
        let goal = self
            .store
            .task(id)
            .and_then(|t| t.research.as_ref())
            .map(|r| r.goal_id);
        let children = goal
            .map(|goal| self.store.research_children(goal))
            .unwrap_or_default();
        let mut stages = col().gap_2();
        for (kind, name) in [
            (ResearchKind::Collect, "Gather evidence"),
            (ResearchKind::Analyze, "Analyze findings"),
            (ResearchKind::Advise, "Write report"),
        ] {
            let tasks: Vec<_> = children
                .iter()
                .filter(|id| {
                    self.store
                        .task(**id)
                        .is_some_and(|t| t.research.as_ref().is_some_and(|r| r.kind == kind))
                })
                .collect();
            if tasks.is_empty() {
                continue;
            }
            let completed = tasks
                .iter()
                .filter(|id| self.store.research_result(***id).is_some())
                .count();
            stages = stages.child(
                row()
                    .justify_between()
                    .child(
                        row()
                            .gap_1()
                            .text_sm()
                            .child(
                                if completed != tasks.len()
                                    && active
                                    && run.status == "Running"
                                    && run.research_kind == Some(kind)
                                {
                                    running_indicator(
                                        "research-stage-pulse",
                                        run_color(&run.status, cx),
                                    )
                                } else {
                                    div()
                                        .w(px(16.))
                                        .flex_shrink_0()
                                        .child(if completed == tasks.len() {
                                            "✓"
                                        } else {
                                            "○"
                                        })
                                        .into_any_element()
                                },
                            )
                            .child(name),
                    )
                    .child(label(format!("{completed} / {} complete", tasks.len()), cx)),
            );
            // Only mounted in the active, non-result view below. Completed attempts
            // continue to open their report without an extra progress outline.
            let mut entries = col().id(format!("stage-tasks-{}", kind.label()))
                .ml_2().pl_3().gap_0p5().border_l_1().border_color(rgb(colors.line));
            for task_id in tasks {
                let task_id = *task_id;
                let Some(task) = self.store.task(task_id) else { continue; };
                use crate::execution_status::ExecutionStatus;
                let status = self.store.execution_status(task_id, self.todo_batch.as_ref());
                let status = if self.store.research_result(task_id).is_some()
                    && !matches!(status, ExecutionStatus::Running | ExecutionStatus::Queued | ExecutionStatus::Retrying) {
                    ExecutionStatus::Done
                } else { status };
                let current = task_id == run.task_id;
                let title = task.title.clone();
                entries = entries.child(row().id(format!("progress-task-{task_id}"))
                    .min_w_0().gap_2().px_2().h(px(32.)).rounded_md()
                    .when(current, |d| d.bg(rgb(colors.accent_surface)))
                    .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(title.clone()).build(w, cx))
                    .child(status.icon_view(cx))
                    .child(div().flex_1().min_w_0().text_sm().truncate()
                        .text_color(rgb(colors.text)).child(task.title.clone()))
                    .child(label(status.label(), cx).flex_shrink_0()
                        .text_color(rgb(status.color(cx))))
                    .map(observed));
            }
            stages = stages.child(entries.map(observed));
        }
        col()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_1()
            .px_4()
            .py_2()
            .child(
                col().flex_shrink_0().gap_2().child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .line_height(relative(1.3))
                        .line_clamp(2)
                        .child(run.task_title.clone()),
                ),
            )
            .child(progress)
            .when(self.show_run_details, |d| {
                d.child(self.run_technical_panel(run, cx))
            })
            .when(!self.show_run_details && readable, |d| {
                d.child(self.run_result_panel(run, cx))
            })
            .when(!self.show_run_details && !readable, |d| {
                d.child(
                    col()
                        .id("run-waiting")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pt_4()
                        .gap_3()
                        .border_t_1()
                        .border_color(rgb(colors.line))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::MEDIUM)
                                .child(headline),
                        )
                        .child(label(queue_explanation.clone(), cx))
                        .when(!active && can_open && project_plan.is_none(), |d| {
                            d.child(primary_button("run-resolve-task", "Open task & resolve").self_start()
                                .on_click(cx.listener(move |a, _, w, cx| a.open_task(id, w, cx))))
                        })
                        .when(active, |d| d.child(stages))
                        .when(active, |d| {
                            d.child(label(
                                "You can keep working elsewhere while this task runs.",
                                cx,
                            ))
                        })
                        .map(observed),
                )
            })
            .into_any_element()
    }

    fn run_technical_panel(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let logs = run.logs.join("\n\n");
        let output = run.output.clone();
        col()
            .id("run-technical-panel")
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                col()
                    .id("run-metadata")
                    .max_h(px(140.))
                    .overflow_y_scroll()
                    .flex_shrink_0()
                    .gap_1()
                    .text_xs()
                    .child(label(
                        format!(
                            "Run {} · {} · {} · {}",
                            run.id,
                            run.agent,
                            run.stage.label(),
                            run.model
                        ),
                        cx,
                    ))
                    .child(label(format!("Folder: {}", run.directory), cx))
                    .child(label(
                        format!(
                            "Queued: {} · Started: {} · Finished: {}",
                            if run.queued_at.is_empty() {
                                "Not recorded"
                            } else {
                                &run.queued_at
                            },
                            if run.status == "Queued" || run.started_at.is_empty() {
                                "Not started"
                            } else {
                                &run.started_at
                            },
                            run.ended_at
                                .as_deref()
                                .unwrap_or(if run.status == "Queued" {
                                    "—"
                                } else if run.status == "Running" {
                                    "In progress"
                                } else {
                                    "Not recorded"
                                })
                        ),
                        cx,
                    ))
                    .child(label(
                        format!(
                            "Session: {} · Exit: {:?}",
                            run.session_id.as_deref().unwrap_or("Not available"),
                            run.exit_code
                        ),
                        cx,
                    ))
                    .map(observed),
            )
            .child(
                row()
                    .flex_wrap()
                    .child(
                        select_button("show-logs", "Live logs", !self.show_run_result).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.show_run_result = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        select_button("show-raw-output", "Saved output", self.show_run_result)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_run_result = true;
                                cx.notify();
                            })),
                    )
                    .child(button("copy-logs", "Copy logs").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(logs.clone()))
                    }))
                    .child(
                        button("copy-raw-output", "Copy output")
                            .disabled(output.is_empty())
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(output.clone()))
                            }),
                    ),
            )
            .when(!self.show_run_result, |d| {
                d.child(
                    Checkbox::new("follow-logs")
                        .label("Follow output")
                        .checked(self.follow_logs)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.follow_logs = *value;
                            if *value {
                                this.log_scroll.scroll_to_bottom();
                            }
                            cx.notify();
                        })),
                )
            })
            .when(self.show_run_result, |d| {
                d.child(
                    col()
                        .id("run-original-output")
                        .overflow_y_scroll()
                        .flex_1()
                        .min_h_0()
                        .p_4()
                        .bg(rgb(colors.side))
                        .font_family("Menlo")
                        .text_xs()
                        .child(run.output.clone())
                        .map(observed),
                )
            })
            .when(!self.show_run_result, |d| {
                d.child(
                    col()
                        .id("run-log-panel")
                        .track_scroll(&self.log_scroll)
                        .overflow_y_scroll()
                        .flex_1()
                        .min_h_0()
                        .p_4()
                        .gap_3()
                        .bg(rgb(colors.side))
                        .rounded_md()
                        .font_family("Menlo")
                        .text_xs()
                        .children(run.logs.iter().map(|line| {
                            div()
                                .text_color(rgb(if line.starts_with("[error]") {
                                    colors.danger
                                } else {
                                    colors.secondary
                                }))
                                .child(line.clone())
                        }))
                        .map(observed),
                )
            })
            .map(observed)
    }
}
