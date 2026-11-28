use crate::app::Scope;
use crate::{
    app::*,
    appearance::palette,
    domain::{AgentRun, Store},
    research::ResearchKind,
    views::*,
};
use gpui_kit::base::{FocusTrapElement, h_resizable, resizable_panel};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::{
    component::{Disableable, Selectable, menu::DropdownMenu, switch::Switch},
    prelude::FluentBuilder,
    *,
};

pub(crate) struct TechnicalDetailsModal {
    pub(crate) run_id: u64,
    pub(crate) show_raw_output: bool,
    pub(crate) opener_focus: Option<gpui_kit::FocusHandle>,
    pub(crate) log_scroll: gpui_kit::ScrollHandle,
    pub(crate) opener_route: crate::navigation_route::Route,
    pub(crate) opener_result: bool,
    pub(crate) opener_details: bool,
}

// Keep the Lucide status icon in fixed bounds while its opacity pulses.
fn running_indicator(id: &'static str, color: u32) -> AnyElement {
    div()
        .id(id)
        .size(px(16.))
        .flex_shrink_0()
        .child(
            gpui_kit::component::Icon::new(assets::IconName::CircleDot)
                .size_4()
                .text_color(rgb(color)),
        )
        .with_animation(
            "breath",
            Animation::new(std::time::Duration::from_millis(1800)).repeat(),
            |icon, phase| icon.opacity(0.75 + 0.25 * (phase * std::f32::consts::TAU).cos()),
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

fn attempt_indicator(run: &AgentRun, cx: &App) -> AnyElement {
    use crate::execution_status::ExecutionStatus as Status;
    let status = match run.status.as_str() {
        "Succeeded" => Status::Done,
        "Running" | "Recovering" | "Cancelling" => Status::Running,
        "Queued" if run.auto_retry_attempt > 0 => Status::Retrying,
        "Queued" => Status::Queued,
        _ => Status::Blocked,
    };
    let tooltip = format!(
        "This attempt: {}{}",
        crate::i18n::tr_string(status_label(&run.status)),
        if run.auto_retry_attempt > 0 {
            format!(" · Retry {}/3", run.auto_retry_attempt)
        } else {
            String::new()
        }
    );
    // This is a status, not a button. An empty button label still reserves layout
    // space and clips the custom icon inside its narrow content container.
    div()
        .id(format!("attempt-status-{}", run.id))
        .size(px(24.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .aria_label(crate::i18n::tr(format!(
            "Attempt: {}",
            crate::i18n::tr_string(status_label(&run.status))
        )))
        .child(status.icon_view(format!("attempt-status-icon-{}", run.id), cx))
        .tooltip(move |w, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(w, cx)
        })
        .into_any_element()
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
impl TaskApp {
    pub fn reveal_selected_run(&mut self) {
        if let Some(project) = self
            .displayed_run_id()
            .and_then(|id| self.store.runs().iter().find(|run| run.id == id))
            .and_then(|run| self.store.task(run.task_id))
            .map(|task| task.project_id)
        {
            self.remember_active_project(project);
        }
        self.result_scroll.set_offset(point(px(0.), px(0.)));
        self.expanded_run_sources = None;
        self.expanded_run_appendix = None;
    }

    pub(crate) fn runs(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self
            .displayed_run_id()
            .and_then(|id| self.store.runs().iter().find(|run| run.id == id));
        if let Some(run) = selected {
            return self.run_content(run, cx);
        }
        self.empty_result_reader(cx)
    }

    fn empty_result_reader(&self, cx: &mut Context<Self>) -> AnyElement {
        col()
            .id("empty-result-reader")
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                div()
                    .text_2xl()
                    .child(selectable("No result selected")),
            )
            .child(muted(
                "Open a task or goal result from its details page.",
                cx,
            ))
            .child(
                primary_button("empty-result-to-tasks", "Go to tasks")
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.navigate(Scope::All, window, cx)
                    })),
            )
            .into_any_element()
    }

    fn run_version_picker(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let attempts: Vec<_> = self
            .store
            .runs()
            .iter()
            .rev()
            .filter(|r| r.task_id == run.task_id && r.project_plan == run.project_plan)
            .collect();
        let caption = crate::time_display::timestamp(if run.started_at.is_empty() {
            &run.queued_at
        } else {
            &run.started_at
        });
        if attempts.len() < 2 {
            return label(caption, cx).into_any_element();
        }
        let selected = run.id;
        let options: Vec<_> = attempts
            .iter()
            .map(|r| {
                (
                    r.id,
                    format!(
                        "Run #{} · {} · {}",
                        r.id,
                        crate::time_display::timestamp(if r.started_at.is_empty() {
                            &r.queued_at
                        } else {
                            &r.started_at
                        }),
                        status_label(&r.status)
                    ),
                )
            })
            .collect();
        let entity = cx.entity().downgrade();
        quiet_button("run-version-picker", caption)
            .px_1()
            .text_xs()
            .text_color(rgb(palette(cx).muted))
            .dropdown_caret(true)
            .tooltip(crate::i18n::tr("View earlier attempts for this task"))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for (id, name) in &options {
                    let entity = entity.clone();
                    let id = *id;
                    menu = menu.item(
                        task_menu_item(name.clone())
                            .checked(id == selected)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |a, cx| {
                                    a.set_selected_run(Some(id));
                                    a.reveal_selected_run();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn run_content(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let result_presentation = crate::result_presentation::present_run(&self.store, run);
        let automation_owned = run.automation_occurrence_id.is_some();
        let active = self.active_runs.iter().any(|a| a.id == run.id)
            || self.queued_runs.iter().any(|(id, _)| *id == run.id);
        let admission_pending = self
            .dispatch_awaiting
            .as_ref()
            .is_some_and(|id| id.run_id == run.id)
            || self
                .dispatch_claim_pending
                .as_ref()
                .is_some_and(|(id, parts)| id.run_id == run.id && parts.run.id == run.id)
            || self
                .pending_dispatch_reserved
                .as_ref()
                .is_some_and(|(_, reserved, _)| reserved.id == run.id)
            || self
                .batch_pending_runs
                .iter()
                .any(|pending| pending.run_id == run.id);
        let stoppable = active || admission_pending || run.status == "Recovering";
        // Recovered OpenCode sessions have no local worker/queue entry, but still
        // need the same progress outline as runs launched by this app instance.
        let ongoing = active
            || matches!(
                run.status.as_str(),
                "Running" | "Queued" | "Recovering" | "Cancelling"
            );
        let run_goal = self.run_goal(run.id);
        let id = run.task_id;
        let attempt_id = run.id;
        let project_plan = run.project_plan;
        let project_id = project_plan.or_else(|| self.store.task(id).map(|t| t.project_id));
        let can_open =
            project_id.is_some() && (project_plan.is_some() || self.store.task(id).is_some());
        let browser_blocked = run
            .logs
            .iter()
            .any(|line| crate::run_errors::is_browser_handoff(line));
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
            "Checking" => (
                "Launch outcome needs checking",
                "A durable launch claim exists, but the process start was not confirmed. Review the working folder, then explicitly rerun as a new attempt or mark this attempt failed. It will not be relaunched automatically.",
            ),
            "Interrupted" => (
                "No completed result for this attempt",
                "The previous attempt was interrupted. Open the task to check its session and retry; completed prerequisite results will be reused.",
            ),
            "Cancelling" => (
                "Stopping this task",
                "Waiting for the current process to stop.",
            ),
            "Succeeded" if crate::request_planning::is_understanding_run(run) => (
                "Research understanding saved",
                "Review the understanding above before generating goals and tasks.",
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
                "The research browser needs your attention. Switch to it and handle its prompt; return to the task when you are ready to continue.",
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
        } else if run.status == "Queued" && automation_owned {
            "Waiting for an automation slot"
        } else {
            headline
        };
        let queue_explanation = if run.status == "Queued" && run.auto_retry_attempt > 0 {
            if self.todo_batch.as_ref().is_some_and(|batch| {
                batch.run_ids.contains_key(&run.id)
                    && batch.state != crate::batch::BatchState::Running
            }) {
                format!(
                    "Retry {}/3 is paused. Continue the batch to resume, or Stop task to cancel.",
                    run.auto_retry_attempt
                )
            } else {
                let seconds = run.retry_delay_remaining(chrono::Utc::now().timestamp());
                if seconds > 0 {
                    format!(
                        "This attempt failed. Automatic retry {}/3 starts in {seconds} seconds. Stop task cancels the retry.",
                        run.auto_retry_attempt
                    )
                } else {
                    format!(
                        "Automatic retry {}/3 is waiting for an execution slot or shared resource. Stop task cancels the retry.",
                        run.auto_retry_attempt
                    )
                }
            }
        } else if run.status == "Queued" {
            let conflicts: Vec<_> = self
                .active_runs
                .iter()
                .filter_map(|active| {
                    self.store
                        .runs()
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
            } else if automation_owned
                && self.automation_slots_used() >= self.store.automation_concurrency_limit()
            {
                format!(
                    "All {} automation slots are busy. This period is queued and will start automatically when an automation slot is free; research tasks continue independently.",
                    self.store.automation_concurrency_limit()
                )
            } else if automation_owned {
                "Waiting for this automation's previous period to finish. Research tasks are not affected.".into()
            } else if !automation_owned
                && self.research_slots_used() >= self.store.concurrency_limit()
            {
                format!(
                    "All {} research execution slots are busy. This task is queued and will start automatically when a research slot is free.",
                    self.store.concurrency_limit()
                )
            } else {
                "Queued · preparing to start automatically.".into()
            }
        } else if run.status == "Failed"
            && run.auto_retry_attempt >= crate::retries::MAX_AUTO_RETRIES
        {
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
        let context_task = self.task_result_context();
        let inline_task_actions = ongoing
            && !readable
            && !self.show_run_details
            && !planning
            && run_goal.is_some()
            && run.research_kind.is_some();
        let origin_label = if project_plan.is_some() {
            "Project details"
        } else if context_task
            .and_then(|id| self.store.task(id))
            .and_then(|t| t.research.as_ref())
            .is_some_and(|r| r.kind == ResearchKind::Goal)
        {
            "View original goal"
        } else {
            "View original task"
        };
        let mut actions = row().flex_shrink_0().gap_2();
        if self.show_run_details {
            actions = actions.child(
                quiet_button("show-result", "Back to result")
                    .icon(assets::IconName::ArrowLeft)
                    .on_click(cx.listener(|a, _, _, cx| {
                        a.show_run_details = false;
                        a.show_run_result = true;
                        cx.notify();
                    })),
            );
        }
        if run.research_kind == Some(ResearchKind::Advise)
            && run.status == "Succeeded"
            && !self.store.task_is_archived(run.task_id)
        {
            let source_id = run.id;
            let regeneration_notice =
                crate::research::report_regeneration_input_notice(&self.store, source_id);
            let regeneration_tooltip = regeneration_notice.unwrap_or_else(|| {
                "Rewrite this report using existing evidence and analysis".into()
            });
            actions = actions.child(
                primary_button("regenerate-report", "Regenerate")
                    .disabled(self.task_running(id))
                    .tooltip(crate::i18n::tr(regeneration_tooltip))
                    .on_click(cx.listener(move |a, _, w, cx| {
                        a.report_regeneration = Some((
                            source_id,
                            cx.new(|cx| {
                                gpui_kit::component::input::TextareaState::new(w, cx)
                                    .placeholder(crate::i18n::tr(
                                        "Optional: tone, audience or length…",
                                    ))
                                    .rows(3)
                            }),
                        ));
                        cx.notify();
                    })),
            );
        }
        if readable && !planning && self.store.task(context_task.unwrap_or(id)).is_some() {
            let navigation = self.store.task_navigation(context_task.unwrap_or(id));
            let previous_enabled = navigation.previous.resolved(&self.store).is_some();
            let next_enabled = navigation.next.resolved(&self.store).is_some();
            let previous = navigation.previous.clone();
            let next = navigation.next.clone();
            let overview = navigation.overview.clone();
            if let Some(overview) = overview {
                actions = actions.child(
                    quiet_button("period-overview", "Overview")
                        .h(px(32.))
                        .tooltip(crate::i18n::tr(overview.tooltip.clone()))
                        .on_click(cx.listener(move |a, _, w, cx| {
                            a.open_task_neighbor(&overview, true, w, cx);
                        })),
                );
            }
            actions = actions
                .child(
                    crate::controls::tool_icon(
                        "previous-task",
                        assets::IconName::ArrowUp,
                        "Previous task",
                    )
                    .h(px(32.))
                    .w(px(32.))
                    .disabled(!previous_enabled)
                    .tooltip(crate::i18n::tr(previous.tooltip.clone()))
                    .on_click(cx.listener(move |a, _, w, cx| {
                        a.open_task_neighbor(&previous, true, w, cx);
                    })),
                )
                .child(
                    crate::controls::tool_icon(
                        "next-task",
                        assets::IconName::ArrowDown,
                        "Next task",
                    )
                    .h(px(32.))
                    .w(px(32.))
                    .disabled(!next_enabled)
                    .tooltip(crate::i18n::tr(next.tooltip.clone()))
                    .on_click(cx.listener(move |a, _, w, cx| {
                        a.open_task_neighbor(&next, true, w, cx);
                    })),
                );
        }
        if stoppable {
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
                danger_button("cancel-run", "Stop task")
                    .disabled(run.status == "Cancelling")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_run(cx))),
            );
        }
        if !inline_task_actions {
            let entity = cx.entity().downgrade();
            let copy = result_presentation.reader_text();
            let details_entity = entity.clone();
            let details_run = run.id;
            actions = actions.child(
                crate::controls::tool_icon(
                    "run-actions-menu",
                    assets::IconName::Ellipsis,
                    "More result actions",
                )
                .h(px(32.))
                .w(px(32.))
                .dropdown_menu_with_anchor(
                    Anchor::TopRight,
                    move |mut menu, _, _| {
                        let details_entity = details_entity.clone();
                        let entity = entity.clone();
                        menu = menu.item(
                            gpui_kit::component::menu::PopupMenuItem::new(crate::i18n::tr(
                                "Technical details",
                            ))
                            .on_click(move |_, w, cx| {
                                let _ = details_entity.update(cx, |a, cx| {
                                    a.open_technical_details_for_run(details_run, w, cx);
                                    cx.notify();
                                });
                            }),
                        );
                        menu = menu.item(
                            gpui_kit::component::menu::PopupMenuItem::new(crate::i18n::tr(
                                origin_label,
                            ))
                            .disabled(!can_open)
                            .on_click(move |_, w, cx| {
                                let _ = entity.update(cx, |a, cx| {
                                    if let Some(pid) = project_plan {
                                        a.structural_navigation();
                                        a.navigate(Scope::Project(pid), w, cx);
                                    } else {
                                        a.record_origin();
                                        a.open_task(context_task.unwrap_or(id), w, cx);
                                    }
                                });
                            }),
                        );
                        if readable && !copy.is_empty() {
                            let copy = copy.clone();
                            menu = menu.separator().item(
                                gpui_kit::component::menu::PopupMenuItem::new(crate::i18n::tr(
                                    "Copy result",
                                ))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                }),
                            );
                        }
                        menu
                    },
                ),
            );
        }
        let mut progress = col()
            .id("run-progress")
            .gap_2()
            .flex_shrink_0()
            .child(
                row()
                    .id("run-title-bar")
                    .w_full()
                    .min_w_0()
                    .h(px(32.))
                    .child(
                        div()
                            .id("run-title")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .tooltip({
                                let title = run.task_title.clone();
                                move |w, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(title.clone())
                                        .build(w, cx)
                                }
                            })
                            .child(selectable(run.task_title.clone()))
                            .map(observed),
                    )
                    .child(actions)
                    .map(observed),
            )
            .child(
                row()
                    .id("run-metadata")
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .gap_3()
                    .child(attempt_indicator(run, cx))
                    .when(run.research_kind.is_some() && !planning, |d| {
                        let mut kind = if self.report_body_error(run).is_some() {
                            crate::i18n::tr_string("Report unavailable")
                        } else {
                            crate::i18n::tr_string(result_presentation.kind.label())
                        }
                        .to_string();
                        if result_presentation.notice.is_none() {
                            if let Some(suffix) = result_presentation.status_suffix() {
                                kind.push_str(&format!(" · {suffix}"));
                            }
                        }
                        let freshness = result_presentation.freshness.labels();
                        if !freshness.is_empty() {
                            kind.push_str(" · ");
                            kind.push_str(&freshness.join(" · "));
                        }
                        d.child(
                            label(kind.clone(), cx)
                                .id("run-result-kind")
                                .aria_label(crate::i18n::tr(kind))
                                .map(observed),
                        )
                    })
                    .child(execution_metadata(
                        String::new(),
                        run.agent.to_string(),
                        if run.model.is_empty() {
                            "Not recorded"
                        } else {
                            run.model.as_str()
                        },
                        cx,
                    ))
                    .child(self.run_version_picker(run, cx))
                    .when(!self.show_run_details && readable, |d| {
                        d.child(label(
                            format!(
                                "{} sources · {} materials",
                                run.sources.len(),
                                run.attachments.len()
                            ),
                            cx,
                        ))
                    })
                    .map(observed),
            )
            .when(
                (run.status != "Succeeded" || planning) && (readable || self.show_run_details),
                |d| d.child(label(queue_explanation.clone(), cx)),
            )
            .map(observed);
        let understanding_review = self
            .store
            .request_for_run_record(run)
            .is_some_and(|request| {
                request.understanding_required
                    && request.goal_ids.is_empty()
                    && self.store.request_understanding_ready(request.id)
            });
        if let Some(request) = self.store.request_for_run_record(run)
            && request.understanding_required
            && request.goal_ids.is_empty()
        {
            progress = col()
                .gap_3()
                .flex_shrink_0()
                .child(progress)
                .child(self.research_understanding_view(request.id, cx))
                .into_any_element();
        }
        if let Some((source_id, input)) = &self.report_regeneration {
            if *source_id == run.id {
                let source_id = *source_id;
                let input = input.clone();
                let preferences = input.clone();
                let regeneration_notice =
                    crate::research::report_regeneration_input_notice(&self.store, source_id);
                let regeneration_tooltip = regeneration_notice.clone().unwrap_or_else(|| {
                    "Rewrite only the report text; upstream research stays unchanged".into()
                });
                progress = col().gap_2().child(progress).child(col().gap_2()
                    .child(label("Rewrite only the final report. Existing evidence, analysis and earlier report versions are kept.", cx))
                    .when_some(regeneration_notice, |panel, reason| {
                        panel.child(
                            crate::views::notice(crate::views::NoticeTone::Warning, reason, cx)
                                .id("report-regeneration-history-notice"),
                        )
                    })
                    .child(gpui_kit::component::input::Textarea::new(&input))
                    .child(row().gap_2()
                        .child(primary_button("confirm-regenerate-report", "Regenerate report")
                            .disabled(self.task_running(id))
                            .tooltip(crate::i18n::tr(regeneration_tooltip))
                            .on_click(cx.listener(move |a, _, _, cx| {
                                let style = preferences.read(cx).value().to_string();
                                a.regenerate_report(source_id, &style, cx);
                            })))
                        .child(quiet_button("cancel-regenerate-report", "Cancel")
                            .on_click(cx.listener(|a, _, _, cx| {
                                a.report_regeneration = None;
                                cx.notify();
                            }))))).into_any_element();
            }
        }
        let goal = self
            .store
            .task(id)
            .and_then(|t| t.research.as_ref())
            .map(|r| r.goal_id);
        let mut children = goal
            .map(|goal| self.store.research_children(goal))
            .unwrap_or_default();
        children.sort_by(|a, b| {
            crate::display_order::siblings_in_display_order(
                self.store.task(*a).unwrap(),
                self.store.task(*b).unwrap(),
            )
        });
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
                .filter(|id| self.store.research_result_available_or_pending(***id))
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
                                    && ongoing
                                    && matches!(run.status.as_str(), "Running" | "Recovering")
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
                                        .child(
                                            gpui_kit::component::Icon::new(
                                                if completed == tasks.len() {
                                                    assets::IconName::Check
                                                } else {
                                                    assets::IconName::Circle
                                                },
                                            )
                                            .size_4(),
                                        )
                                        .into_any_element()
                                },
                            )
                            .child(name),
                    )
                    .child(label(format!("{completed} / {} complete", tasks.len()), cx)),
            );
            // Only mounted in the active, non-result view below. Completed attempts
            // continue to open their report without an extra progress outline.
            let mut entries = col()
                .id(format!("stage-tasks-{}", kind.label()))
                .ml_2()
                .pl_3()
                .gap_0p5()
                .border_l_1()
                .border_color(rgb(colors.line));
            for task_id in tasks {
                let task_id = *task_id;
                let Some(task) = self.store.task(task_id) else {
                    continue;
                };
                use crate::execution_status::ExecutionStatus;
                let status = self.display_task_status(task_id);
                let result_run = (status.execution == ExecutionStatus::Done)
                    .then(|| self.store.research_result(task_id).map(|r| r.id))
                    .flatten();
                let current = task_id == run.task_id;
                let status_label = crate::i18n::tr_string(status.label());
                let row_label = format!("{} · {status_label}", task.title);
                let row_tooltip = format!("{}\n{}", task.title, status_label);
                let latest_run = self
                    .store
                    .runs()
                    .iter()
                    .rev()
                    .find(|r| r.task_id == task_id && r.project_plan.is_none())
                    .map(|r| r.id);
                entries = entries.child(
                    row()
                        .id(format!("progress-task-{task_id}"))
                        .min_w_0()
                        .gap_2()
                        .px_2()
                        .h(px(32.))
                        .rounded(px(crate::appearance::CONTROL_RADIUS))
                        .when(current, |d| d.bg(rgb(colors.accent_surface)))
                        .aria_label(crate::i18n::tr(row_label))
                        .tooltip(move |w, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(row_tooltip.clone())
                                .build(w, cx)
                        })
                        .child(
                            status
                                .execution
                                .icon_view(format!("progress-task-icon-{task_id}"), cx),
                        )
                        .child(
                            row_button(format!("progress-task-title-{task_id}"), "")
                                .flex_1()
                                .min_w_0()
                                .h(px(28.))
                                .px_1()
                                .accessibility_label(crate::i18n::tr(task.title.clone()))
                                .tooltip(crate::i18n::tr(if result_run.is_some() {
                                    "Open result"
                                } else {
                                    "Open task details"
                                }))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_sm()
                                        .text_left()
                                        .truncate()
                                        .text_color(rgb(colors.text))
                                        .child(selectable(task.title.clone())),
                                )
                                .child(
                                    div().flex_shrink_0().child(
                                        row()
                                            .gap_1()
                                            .child(label(
                                                if result_run.is_some() {
                                                    "View result"
                                                } else {
                                                    "Open task"
                                                },
                                                cx,
                                            ))
                                            .child(
                                                gpui_kit::component::Icon::new(
                                                    assets::IconName::ChevronRight,
                                                )
                                                .size_4(),
                                            ),
                                    ),
                                )
                                .on_click(cx.listener(move |a, _, w, cx| {
                                    a.record_origin();
                                    if let Some(run_id) = result_run {
                                        a.show_task_result(run_id, cx);
                                    } else {
                                        a.open_task(task_id, w, cx);
                                    }
                                })),
                        )
                        .child(row().gap_0p5().flex_shrink_0().when_some(
                            latest_run,
                            |actions, run_id| {
                                actions.child(
                                    crate::controls::technical_details_button(format!(
                                        "progress-technical-details-{task_id}"
                                    ))
                                    .on_click(cx.listener(
                                        move |a, _, w, cx| {
                                            a.open_technical_details_for_run(run_id, w, cx);
                                            cx.notify();
                                        },
                                    )),
                                )
                            },
                        ))
                        .map(observed),
                );
            }
            stages = stages.child(entries.map(observed));
        }
        col()
            .id("run-content")
            .flex_1()
            .w_full()
            .min_w_0()
            .h_full()
            .when(understanding_review && !self.show_run_details, |d| {
                d.min_h_0().overflow_y_scroll()
            })
            .gap_1()
            .px_4()
            .py_2()
            .child(progress)
            .when(self.show_run_details, |d| {
                d.child(self.run_technical_panel(run, cx))
            })
            .when(!self.show_run_details && readable, |d| {
                d.child(self.run_result_panel(run, cx))
            })
            .when(
                !self.show_run_details
                    && !readable
                    && !(understanding_review && run.status == "Succeeded"),
                |d| {
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
                                    .child(selectable(headline)),
                            )
                            .child(label(queue_explanation.clone(), cx))
                            .when(run.status == "Checking", |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            primary_button(
                                                "run-launch-rerun",
                                                "Rerun as new attempt",
                                            )
                                            .on_click(
                                                cx.listener(move |a, _, _, cx| {
                                                    let _ = a.resolve_launch_ambiguity(
                                                        attempt_id, true, cx,
                                                    );
                                                }),
                                            ),
                                        )
                                        .child(
                                            danger_button(
                                                "run-launch-failed",
                                                "Mark attempt failed",
                                            )
                                            .on_click(
                                                cx.listener(move |a, _, _, cx| {
                                                    let _ = a.resolve_launch_ambiguity(
                                                        attempt_id, false, cx,
                                                    );
                                                }),
                                            ),
                                        ),
                                )
                            })
                            .when(!ongoing && can_open && project_plan.is_none(), |d| {
                                d.child(
                                    primary_button("run-resolve-task", "Open task & resolve")
                                        .self_start()
                                        .on_click(cx.listener(move |a, _, w, cx| {
                                            a.record_origin();
                                            a.open_task(id, w, cx);
                                        })),
                                )
                            })
                            .when(ongoing, |d| d.child(stages))
                            .when(ongoing, |d| {
                                d.child(label(
                                    "You can keep working elsewhere while this task runs.",
                                    cx,
                                ))
                            })
                            .map(observed),
                    )
                },
            )
            .map(observed)
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
                    .child(execution_metadata(
                        format!(
                            "Run {} · {}",
                            run.id,
                            crate::i18n::tr_string(run.stage.label())
                        ),
                        run.agent.to_string(),
                        if run.model.is_empty() {
                            "Not recorded"
                        } else {
                            run.model.as_str()
                        },
                        cx,
                    ))
                    .when_some(self.store.task(run.task_id), |details, task| {
                        details.child(label(format!("Task: {}", self.store.identifier(task)), cx))
                    })
                    .child(label(format!("Folder: {}", run.directory), cx))
                    .child(label(
                        format!(
                            "Queued: {} · Started: {} · Finished: {}",
                            crate::time_display::timestamp(if run.queued_at.is_empty() {
                                "Not recorded"
                            } else {
                                &run.queued_at
                            }),
                            crate::time_display::timestamp(
                                if run.status == "Queued" || run.started_at.is_empty() {
                                    "Not started"
                                } else {
                                    &run.started_at
                                }
                            ),
                            crate::time_display::timestamp(run.ended_at.as_deref().unwrap_or(
                                if run.status == "Queued" {
                                    "—"
                                } else if run.status == "Running" {
                                    "In progress"
                                } else {
                                    "Not recorded"
                                }
                            ))
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
                        select_button("show-logs", "Live logs", !self.show_run_result, cx)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_run_result = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        select_button("show-raw-output", "Saved output", self.show_run_result, cx)
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
                    Switch::new("follow-logs")
                        .label(crate::i18n::tr("Follow output"))
                        .checked(self.follow_logs)
                        .on_click(cx.listener(|this, value, _, cx| {
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
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .font_family("Menlo")
                        .text_xs()
                        .child(selectable_raw(run.output.clone()))
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
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .font_family("Menlo")
                        .text_xs()
                        .children(run.logs.iter().map(|line| {
                            div()
                                .text_color(rgb(if line.starts_with("[error]") {
                                    colors.danger
                                } else {
                                    colors.secondary
                                }))
                                .child(selectable(line.clone()))
                        }))
                        .map(observed),
                )
            })
            .map(observed)
    }
}

impl TaskApp {
    pub(crate) fn open_technical_details(
        &mut self,
        task: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self
            .store
            .runs()
            .iter()
            .rev()
            .find(|run| run.task_id == task && run.project_plan.is_none())
        else {
            return;
        };
        self.open_technical_details_for_run(run.id, window, cx);
    }

    pub(crate) fn open_technical_details_for_run(
        &mut self,
        run_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.store.runs().iter().any(|run| run.id == run_id) {
            return;
        }
        self.technical_details_modal = Some(TechnicalDetailsModal {
            run_id,
            show_raw_output: true,
            opener_focus: window.focused(cx),
            log_scroll: gpui_kit::ScrollHandle::new(),
            opener_route: self.nav.route().clone(),
            opener_result: self.show_run_result,
            opener_details: self.show_run_details,
        });
        cx.notify();
    }

    pub(crate) fn open_automation_technical_details(
        &mut self,
        run_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_technical_details_for_run(run_id, window, cx);
    }

    pub(crate) fn close_technical_details(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modal = self.technical_details_modal.take();
        if let Some(modal) = &modal {
            if self.nav.route() != &modal.opener_route {
                let _ = self.nav.replace_route(&self.store, modal.opener_route.clone());
            }
            self.show_run_result = modal.opener_result;
            self.show_run_details = modal.opener_details;
        }
        let opener = modal.and_then(|modal| modal.opener_focus);
        if let Some(opener) = opener {
            window.focus(&opener, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn technical_details_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(modal) = &self.technical_details_modal else {
            return div().into_any_element();
        };
        let Some(run) = self.store.runs().iter().find(|run| run.id == modal.run_id) else {
            return div().into_any_element();
        };
        let entity = cx.entity().downgrade();
        let logs_entity = entity.clone();
        let output_entity = entity.clone();
        let logs = run.logs.join("\n\n");
        let output = run.output.clone();
        let show_raw_output = modal.show_raw_output;
        let log_scroll = modal.log_scroll.clone();
        let copy_output = output.clone();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x000000a0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_technical_details(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                col()
                    .id("technical-details-modal")
                    .w(px(860.))
                    .max_w_full()
                    .h(relative(0.9))
                    .max_h_full()
                    .p_6()
                    .gap_4()
                    .on_mouse_down(
                        MouseButton::Left,
                        |_, _, cx| cx.stop_propagation(),
                    )
                    .map(|d| crate::appearance::floating_surface(d, cx))
                    .child(
                        row()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable(crate::i18n::tr("Technical details"))),
                            )
                            .child(
                                button("technical-details-close", "Close")
                                    .icon(assets::IconName::Close)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_technical_details(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        col()
                            .id("technical-details-metadata")
                            .max_h(px(140.))
                            .overflow_y_scroll()
                            .flex_shrink_0()
                            .gap_1()
                            .text_xs()
                            .child(crate::views::execution_metadata(
                                format!("Run {} · {}", run.id, crate::i18n::tr_string(run.stage.label())),
                                run.agent.to_string(),
                                if run.model.is_empty() {
                                    "Not recorded"
                                } else {
                                    run.model.as_str()
                                },
                                cx,
                            ))
                            .when_some(self.store.task(run.task_id), |details, task| {
                                details.child(label(format!("Task: {}", self.store.identifier(task)), cx))
                            })
                            .child(label(format!("Folder: {}", run.directory), cx))
                            .child(label(
                                format!(
                                    "Queued: {} · Started: {} · Finished: {}",
                                    crate::time_display::timestamp(if run.queued_at.is_empty() {
                                        "Not recorded"
                                    } else {
                                        &run.queued_at
                                    }),
                                    crate::time_display::timestamp(
                                        if run.status == "Queued" || run.started_at.is_empty() {
                                            "Not started"
                                        } else {
                                            &run.started_at
                                        }
                                    ),
                                    crate::time_display::timestamp(run.ended_at.as_deref().unwrap_or(
                                        if run.status == "Queued" {
                                            "—"
                                        } else if run.status == "Running" {
                                            "In progress"
                                        } else {
                                            "Not recorded"
                                        }
                                    ))
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
                            )),
                    )
                    .child(
                        row()
                            .flex_wrap()
                            .child(
                                crate::controls::select_button(
                                    "technical-details-show-logs",
                                    "Live logs",
                                    !show_raw_output,
                                    cx,
                                )
                                .on_click(move |_, _, cx| {
                                    let _ = logs_entity.update(cx, |app, cx| {
                                        if let Some(modal) = app.technical_details_modal.as_mut() {
                                            modal.show_raw_output = false;
                                        }
                                        cx.notify();
                                    });
                                }),
                            )
                            .child(
                                crate::controls::select_button(
                                    "technical-details-show-output",
                                    "Saved output",
                                    show_raw_output,
                                    cx,
                                )
                                .on_click(move |_, _, cx| {
                                    let _ = output_entity.update(cx, |app, cx| {
                                        if let Some(modal) = app.technical_details_modal.as_mut() {
                                            modal.show_raw_output = true;
                                        }
                                        cx.notify();
                                    });
                                }),
                            )
                            .child(
                                button("technical-details-copy-logs", "Copy logs").on_click(
                                    move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            logs.clone(),
                                        ))
                                    },
                                ),
                            )
                            .child(
                                button("technical-details-copy-output", "Copy output")
                                    .disabled(copy_output.is_empty())
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_output.clone(),
                                        ))
                                    }),
                            ),
                    )
                    .when(show_raw_output, |d| {
                        d.child(
                            col()
                                .id("technical-details-output")
                                .overflow_y_scroll()
                                .flex_1()
                                .min_h_0()
                                .p_4()
                                .bg(rgb(palette(cx).side))
                                .rounded(px(crate::appearance::GROUP_RADIUS))
                                .font_family("Menlo")
                                .text_xs()
                                .child(selectable_raw(output.clone()))
                                .map(observed),
                        )
                    })
                    .when(!show_raw_output, |d| {
                        d.child(
                            col()
                                .id("technical-details-logs")
                                .track_scroll(&log_scroll)
                                .overflow_y_scroll()
                                .flex_1()
                                .min_h_0()
                                .p_4()
                                .gap_3()
                                .bg(rgb(palette(cx).side))
                                .rounded(px(crate::appearance::GROUP_RADIUS))
                                .font_family("Menlo")
                                .text_xs()
                                .children(run.logs.iter().map(|line| {
                                    div()
                                        .text_color(rgb(if line.starts_with("[error]") {
                                            palette(cx).danger
                                        } else {
                                            palette(cx).secondary
                                        }))
                                        .child(selectable(line.clone()))
                                }))
                                .map(observed),
                        )
                    }),
            )
            .track_focus(&self.modal_focus)
            .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    app.close_technical_details(window, cx);
                    cx.stop_propagation();
                }
            }))
            .focus_trap("technical-details-focus", &self.modal_focus)
            .into_any_element()
    }
}
