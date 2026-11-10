//! Goal-local execution views; step cards never approve or launch work.
use crate::{
    app::{Layout, TaskApp},
    domain::{Stage, Store},
    execution_status::ExecutionStatus,
    views::*,
};
use gpui_kit::{
    component::{
        Selectable, Disableable,
        button::{ButtonCustomVariant, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepState {
    Waiting,
    Running,
    Blocked,
    Done,
}
impl Store {
    pub(crate) fn step_state(&self, id: u64) -> StepState {
        let Some(task) = self.task(id) else {
            return StepState::Blocked;
        };
        let latest = self
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == id && r.project_plan.is_none());
        if latest.is_some_and(|r| {
            matches!(r.status.as_str(), "Running" | "Cancelling" | "Recovering")
        }) {
            return StepState::Running;
        }
        if task.stage == Stage::Done
            && (task.research.is_none() || self.research_result_available_or_pending(id))
        {
            return StepState::Done;
        }
        if latest.is_some_and(|r| r.status == "Queued") {
            return StepState::Waiting;
        }
        if task.stage == Stage::Cancelled
            || latest.is_some_and(|r| {
                matches!(
                    r.status.as_str(),
                    "Failed" | "Blocked" | "Cancelled" | "Interrupted"
                )
            })
        {
            return StepState::Blocked;
        }
        // Only a step that actually failed (or was cancelled) is Blocked. A step
        // whose prerequisite failed has not run yet, so it stays Waiting: this
        // keeps the evidence → analysis → report order legible instead of marking
        // every downstream step as another failure.
        StepState::Waiting
    }
    pub(crate) fn step_block_reason(&self, id: u64) -> Option<String> {
        if self.step_state(id) != StepState::Blocked {
            return None;
        }
        let task = self.task(id)?;
        if task.stage == Stage::Cancelled {
            return Some("Cancelled · restore this step before continuing".into());
        }
        if let Some(run) = self.runs().iter().rev().find(|r| r.task_id == id && r.project_plan.is_none()) {
            if run.status == "Failed" && run.auto_retry_attempt >= crate::retries::MAX_AUTO_RETRIES {
                return Some("This step still failed after 3 automatic retries. Review the attempt details before retrying manually.".into());
            }
            let reason = match run.status.as_str() {
                "Interrupted" => Some("Run interrupted · the previous attempt did not finish"),
                "Failed" => Some("Last attempt failed · review the attempt details and retry this step"),
                "Blocked" => Some("Execution blocked · review the attempt details before retrying"),
                "Cancelled" => Some("Run cancelled · retry this step to continue"),
                _ => None,
            };
            if let Some(reason) = reason { return Some(reason.into()); }
        }
        Some("This step needs attention. Open it to review the attempt.".into())
    }

}
impl TaskApp {
    pub(crate) fn goal_run_state(&self, goal: u64) -> (&'static str, bool, &'static str) {
        if self.store.task_is_archived(goal) {
            return ("Archived", true, "Restore the topic or goal before continuing.");
        }
        if let Some(batch) = self.todo_batch.as_ref().filter(|b| b.contains_pending(goal)) {
            if matches!(batch.state, crate::batch::BatchState::Paused(_)) {
                return ("Resume queue", false, "Continue the paused topic queue.");
            }
            if !batch.current_items(&self.store).contains(&goal) {
                return ("Queued", true, "This topic will start automatically after the current topic finishes.");
            }
        }
        if self.goal_busy(goal) {
            return ("Running…", true, "Steps are running or queued. Open Steps to inspect progress or retry a blocked step individually.");
        }
        if self.store.goal_tasks_complete(goal) || self.store.goal_tasks_present(goal) {
            return ("Completed", true, "All steps have current, completed results.");
        }
        if !self.store.goal_execution_approved(goal) {
            return ("Awaiting approval", true, "Review the plan and move this goal to Todo first.");
        }
        if self.todo_batch.as_ref().is_some_and(|b| b.state != crate::batch::BatchState::Finished && !b.contains_pending(goal)) {
            return ("Add to queue", false, "Add this approved goal to the topic queue. No execution slot is needed to queue it.");
        }
        let started = self.store.runs().iter().any(|r| r.project_plan.is_none()
            && self.store.owning_research_goal(r.task_id) == Some(goal));
        if started {
            ("Resume goal", false, "Continue unfinished steps in dependency order; reuse completed results.")
        } else {
            ("Run goal", false, "Start approved steps in dependency order.")
        }
    }
    pub(crate) fn goal_run_button(&self, goal: u64, element: &'static str, cx: &mut Context<Self>) -> gpui_kit::component::button::Button {
        let (title, disabled, hint) = self.goal_run_state(goal);
        button(element, title).disabled(disabled).tooltip(hint)
            .on_click(cx.listener(move |a, _, _, cx| a.run_research_next(goal, true, cx)))
    }
    pub(crate) fn step_execution_panel(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.store.task(id) else { return div().into_any_element(); };
        let Some(research) = task.research.as_ref().filter(|r| r.kind != crate::research::ResearchKind::Goal) else {
            return div().into_any_element();
        };
        let goal = research.goal_id;
        let status = self.display_task_status(id);
        let state = status.execution;
        let archived = self.store.task_is_archived(id);
        let pending = self.store.research_pending_approval(id);
        let checking = self.recovery_blocks_task(id);
        let active = self.task_running(id);
        let cancelled = task.stage == Stage::Cancelled;
        let dependencies: Vec<_> = research.depends_on.iter().copied()
            .filter(|dep| {
                if archived {
                    crate::archive_presentation::validated_history_result(&self.store, *dep).is_none()
                } else {
                    self.store.research_result(*dep).is_none()
                }
            }).collect();
        let latest = self.store.runs().iter().rev().find(|r| r.task_id == id && r.project_plan.is_none());
        let reason = if archived {
            "This step belongs to archived work. Restore its topic or goal before running.".into()
        } else if pending {
            "This goal's plan needs approval. Review the goal and move it to Todo first.".into()
        } else if checking {
            "The previous session has not been confirmed stopped. Check its progress before retrying to avoid a duplicate run.".into()
        } else { status.reason.clone() };
        let colors = crate::appearance::palette(cx);
        let mut panel = col().id("step-execution-panel").w_full().min_w_0().gap_2().py_2().pl_3()
            .border_l_2().border_color(rgb(state.color(cx)))
            .child(div().id("step-execution-status").aria_label(self.display_task_status(id).label())
                .text_sm().font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(state.color(cx))).child(self.display_task_status(id).label()).map(observed))
            .child(div().id("step-execution-reason").text_sm().text_color(rgb(colors.secondary)).child(selectable(reason)).map(observed));
        if state == ExecutionStatus::Blocked {
            if let Some(run) = latest {
                let detail = run.logs.iter().rev().find(|line| {
                    ["[error]", "[blocked]", "[interrupted]", "[workspace]", "[stream error]"].iter()
                        .any(|prefix| line.starts_with(prefix))
                }).map(|line| crate::run_errors::user_message(line).to_owned());
                if let Some(detail) = detail {
                    panel = panel.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        detail,
                        cx,
                    ));
                }
            }
        }
        for dep in dependencies.iter().copied() {
            let title = self.store.task(dep).map(|t| format!("Open {}", self.store.identifier(t)))
                .unwrap_or_else(|| format!("Missing prerequisite {dep}"));
            panel = panel.child(button(format!("step-prerequisite-{dep}"), title).w_full()
                .tooltip(self.store.task(dep).map(|t| t.title.clone()).unwrap_or_default())
                .disabled(self.store.task(dep).is_none())
                .on_click(cx.listener(move |a, _, w, cx| a.open_task(dep, w, cx))));
        }
        let mut actions = col().w_full().gap_1();
        if checking {
            actions = actions.child(button("step-check-session", "Check previous session")
                .on_click(cx.listener(|a, _, _, cx| a.check_interrupted_sessions(cx))));
        }
        if cancelled && !archived {
            actions = actions.child(button("step-reopen", "Reopen step")
                .on_click(cx.listener(move |a, _, _, cx| {
                    a.apply_task_stage(
                        id,
                        Stage::Planned,
                        crate::scoped_edit::StageIntent::Reopen,
                        cx,
                    );
                })));
        }
        let run_action = if state == ExecutionStatus::Done {
            quiet_button("step-run", "Run again").self_start().h_7().px_1().text_sm()
                .text_color(rgb(colors.secondary))
        } else {
            primary_button("step-run", if latest.is_some() { "Retry step" } else { "Run step" })
        };
        if !archived {
            actions = actions.child(run_action
                .disabled(pending || checking || active || cancelled || !dependencies.is_empty())
                .on_click(cx.listener(move |a, _, _, cx| a.start_run(id, cx))));
        }
        let mut links = col().gap_1()
            .child(quiet_button("step-open-goal", if pending { "Review goal" } else { "Open goal" })
                .self_start().h_7().px_1().text_sm().text_color(rgb(colors.secondary))
                .on_click(cx.listener(move |a, _, w, cx| a.open_task(goal, w, cx))));
        if latest.is_some() {
            links = links.child(quiet_button("step-attempt-details", "Technical details")
                .self_start().h_7().px_1().text_sm().text_color(rgb(colors.secondary))
                .on_click(cx.listener(move |a, _, _, cx| {
                    a.view_task_result(id, cx);
                    a.show_run_details = true;
                    a.show_run_result = false;
                    cx.notify();
                })));
        }
        panel.child(actions.child(links))
            .when_some(self.task_start_feedback.as_ref().filter(|(task, _)| *task == id), |d, (_, message)| {
                d.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    message.clone(),
                    cx,
                ))
            }).map(observed).into_any_element()
    }
    pub(crate) fn open_goal_steps(&mut self, goal: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.open_task(goal, w, cx);
        self.set_goal_steps_tab(Some(goal));
        self.set_steps_layout(Layout::Board);
        cx.notify();
    }
    pub(crate) fn goal_tabs(&self, goal: u64, cx: &mut Context<Self>) -> AnyElement {
        row()
            .gap_1()
            .child(
                row_button("goal-overview-tab", "Overview").h(px(28.)).px_2().text_sm()
                    .selected(self.goal_steps_tab() != Some(goal))
                    .on_click(cx.listener(|a, _, _, cx| {
                        a.set_goal_steps_tab(None);
                        cx.notify();
                    })),
            )
            .child(
                row_button("goal-steps-tab", "Steps").h(px(28.)).px_2().text_sm()
                    .selected(self.goal_steps_tab() == Some(goal))
                    .on_click(cx.listener(move |a, _, _, cx| {
                        a.set_goal_steps_tab(Some(goal));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    pub(crate) fn goal_steps_view(&self, goal: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = crate::appearance::palette(cx);
        let archived = self.store.task_is_archived(goal);
        let mut ids = self.store.research_children(goal);
        ids.sort_by(|a, b| crate::display_order::siblings_in_display_order(self.store.task(*a).unwrap(), self.store.task(*b).unwrap()));
        let mut content = col().flex_1().min_h_0().child(
            col()
                .px_6()
                .pt_3()
                .pb_3()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.store.task(goal).unwrap().title.clone()),
                )
                .child(
                    row()
                        .gap_3()
                        .justify_between()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(label(self.execution_summary(goal), cx)),
                        )
                        .child(self.layout_switch_for(true, cx)),
                ),
        );
        if ids.is_empty() {
            return content
                .child(muted("No steps planned yet.", cx).px_6())
                .into_any_element();
        }
        let step = |id: u64, board: bool, cx: &mut Context<Self>| {
            let task = self.store.task(id).unwrap();
            let title = format!("{} · {}", self.store.identifier(task), task.title);
            let execution = self.display_task_status(id).execution;
            let status = self.display_task_status(id);
            let (surface, hover, border) = board_card_colors(status.phase, cx);
            let dark = gpui_kit::component::Theme::global(cx).is_dark();
            row_button(format!("goal-step-{id}"), "")
                .w_full()
                .min_w_0()
                .when(board, |b| {
                    b.h_auto()
                        .p_3()
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .border_1()
                        .border_color(rgb(border))
                        .bg(rgb(surface))
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .color(rgb(surface).into())
                                .foreground(rgb(colors.text).into())
                                .hover(rgb(hover).into())
                                .active(rgb(hover).into()),
                        )
                })
                .when(!board, |b| b.h_8().px_2())
                .accessibility_label(format!("{} · {}", title, status.label()))
                .when(!board, |b| b.tooltip(format!("{} · {}", status.label(), status.reason)))
                .on_click(cx.listener(move |a, _, w, cx| {
                    a.request_task(id, false, w, cx)
                }))
                .child(if board {
                    col()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .child(
                            row()
                                .w_full()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .rounded(px(4.))
                                        .px_1()
                                        .py(px(2.))
                                        .bg(rgba(if dark { 0xffffff14 } else { 0xffffff99 }))
                                        .text_xs()
                                        .child(self.store.identifier(task)),
                                )
                                .child(
                                    execution.icon_view(format!("goal-step-icon-{id}"), cx),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .whitespace_normal()
                                .text_left()
                                .line_height(relative(1.4))
                                .child(selectable(task.title.clone())),
                        )
                        .child(div().id(format!("goal-step-status-{id}"))
                            .aria_label(status.label())
                            .text_xs().text_left().whitespace_normal()
                            .text_color(rgb(execution.color(cx))).child(selectable(status.label())).map(observed))
                        .child(div().text_xs().text_left().whitespace_normal()
                            .text_color(rgb(colors.secondary)).child(selectable(status.reason.clone())))
                        .child(
                            row()
                                .w_full()
                                .justify_between()
                                .child(label(
                                    task.research
                                        .as_ref()
                                        .map(|r| match r.kind {
                                            crate::research::ResearchKind::Collect => "Evidence",
                                            crate::research::ResearchKind::Analyze => "Analysis",
                                            crate::research::ResearchKind::Advise => "Final report",
                                            crate::research::ResearchKind::Goal => "Goal",
                                        })
                                        .unwrap_or("Task"),
                                    cx,
                                ))
                                .child(row().gap_1().child(label("Open", cx)).child(gpui_kit::component::Icon::new(assets::IconName::ChevronRight).size_4())),
                        )
                        .into_any_element()
                } else {
                    row()
                        .w_full()
                        .min_w_0()
                        .child(
                            execution.icon_view(format!("goal-step-row-icon-{id}"), cx),
                        )
                        .child(single_line_text(format!("goal-step-title-{id}"), title))
                        .child(label(status.label(), cx).id(format!("goal-step-status-{id}"))
                            .aria_label(status.label()).map(observed))
                        .child(row().gap_1().child(label("Open", cx)).child(gpui_kit::component::Icon::new(assets::IconName::ChevronRight).size_4()))
                        .into_any_element()
                })
        };
        if self.steps_layout() == Layout::List {
            content = content.child(
                col()
                    .id("goal-steps-list")
                    .px_6()
                    .pb_5()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .children(ids.iter().map(|id| step(*id, false, cx)))
                    .map(observed),
            );
        } else {
            content = content.child(
                row()
                    .id("goal-steps-board")
                    .items_start()
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .px_6()
                    .pb_5()
                    .gap_4()
                    .children(Stage::BOARD_ORDER.into_iter().filter(|phase|
                        if archived {
                            ids.iter().any(|id| self.display_task_status(*id).phase == *phase)
                        } else {
                            matches!(phase, Stage::Planned | Stage::InProgress | Stage::Done) || ids.iter().any(|id| self.display_task_status(*id).phase == *phase)
                        }
                    ).map(|phase| {
                        let matching: Vec<_> = ids.iter().copied().filter(|id|
                            self.display_task_status(*id).phase == phase).collect();
                        let tasks: Vec<_> = matching.iter().filter_map(|id| self.store.task(*id).cloned()).collect();
                        let mut cards = vec![];
                        if phase == Stage::InProgress {
                            for group in crate::workflow::ExecutionGroup::ALL {
                                if self.board_execution_filter.is_some_and(|filter| filter != group) { continue; }
                                let group_ids: Vec<_> = matching.iter().copied().filter(|id|
                                    self.display_task_status(*id).group == Some(group)).collect();
                                if group_ids.is_empty() { continue; }
                                cards.push(label(format!("{} · {}", group.label(), group_ids.len()), cx).into_any_element());
                                cards.extend(group_ids.into_iter().map(|id| step(id, true, cx).into_any_element()));
                            }
                        } else { cards.extend(matching.iter().map(|id| step(*id, true, cx).into_any_element())); }
                        col().id(format!("steps-column-{}", crate::workflow::phase_label(phase)))
                            .w(px(if phase == Stage::InProgress { 350. } else { 292. })).flex_shrink_0().h_full().gap_3()
                            .child(row().h_9().gap_2()
                                .child(div().text_color(rgb(stage_color(phase, cx))).child(selectable(crate::workflow::phase_label(phase))))
                                .child(label(matching.len().to_string(), cx))
                                .when(phase == Stage::InProgress && !archived, |d| d.child(self.execution_status_filter(&tasks, cx))))
                            .child(col().id(format!("steps-cards-{}", crate::workflow::phase_label(phase)))
                                .flex_1().min_h_0().overflow_y_scroll().gap_3().restrict_scroll_to_axis()
                                .when(cards.is_empty(), |d| d.child(label("No steps in this category", cx)))
                                .children(cards)
                                .map(observed))
                    }))
                    .map(observed),
            );
        }
        content.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::StepState;
    use crate::domain::{Stage, Store};
    #[test]
    fn states_follow_runs_dependencies_and_valid_results() {
        let mut s = Store::empty();
        s.set_default_model("fixture/model".into());
        s.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into();
        let goal = s
            .create_research_goal("Review".into(), "Analyze earnings".into(), 1)
            .unwrap();
        let ids = s.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect","instructions":"Find filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare evidence","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Summarize","kind":"advise","depends_on":[1]}]}"#).unwrap();
        assert_eq!(s.step_state(ids[0]), StepState::Waiting);
        let mut run = s.prepare_run(ids[0]).unwrap();
        run.status = "Running".into();
        s.push_run(run);
        assert_eq!(s.step_state(ids[0]), StepState::Running);
        assert_eq!(s.step_state(ids[1]), StepState::Waiting);
        s.last_run_mut().unwrap().status = "Failed".into();
        assert_eq!(s.step_state(ids[0]), StepState::Blocked);
        assert_eq!(
            s.step_state(ids[1]),
            StepState::Waiting,
            "a dependent step has not run yet and must not be marked as failed"
        );
        assert_eq!(s.step_state(ids[2]), StepState::Waiting);
        s.last_run_mut().unwrap().status = "Queued".into();
        assert_eq!(s.step_state(ids[2]), StepState::Waiting);
        s.last_run_mut().unwrap().status = "Succeeded".into();
        s.last_run_mut().unwrap().output = "Verified evidence".into();
        s.task_mut(ids[0]).unwrap().stage = Stage::Done;
        assert_eq!(s.step_state(ids[0]), StepState::Done);
        s.task_mut(ids[0])
            .unwrap()
            .description
            .push_str(" changed inputs");
        assert_eq!(s.step_state(ids[0]), StepState::Waiting);
        assert_eq!(s.research_children(goal), ids);
    }
}
