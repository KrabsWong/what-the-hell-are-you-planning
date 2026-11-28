//! Goal-local execution views; step cards never approve or launch work.
use crate::{
    app::TaskApp,
    domain::{AgentRun, Stage, Store},
    execution_status::ExecutionStatus,
    views::*,
};
use gpui_kit::{
    component::{Disableable, Selectable},
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

/// Extract the most useful diagnostic from a saved attempt without exposing
/// the whole log. Agent logs do not consistently use an `[error]` prefix, so a
/// generic failure used to hide the real cause from step status surfaces.
pub(crate) fn run_failure_detail(run: &AgentRun) -> String {
    const LIMIT: usize = 240;

    fn clean(line: &str) -> Option<String> {
        let mut value = line.trim();
        if value.is_empty() {
            return None;
        }
        for prefix in [
            "[error] ",
            "[workspace] ",
            "[browser] ",
            "[automation] ",
            "[recovery] ",
            "[research] ",
            "[process] ",
        ] {
            if let Some(rest) = value.strip_prefix(prefix) {
                value = rest.trim();
                break;
            }
        }
        if value.is_empty() {
            return None;
        }
        let mut chars = value.chars();
        let kept: String = chars.by_ref().take(LIMIT).collect();
        if chars.next().is_some() {
            Some(format!("{kept}..."))
        } else {
            Some(kept)
        }
    }

    if let Some(detail) = run
        .logs
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("[error] ").and_then(|_| clean(line)))
    {
        return detail;
    }

    const DIAGNOSTIC_MARKERS: [&str; 14] = [
        "error",
        "fail",
        "timeout",
        "timed out",
        "exceeded",
        "denied",
        "unavailable",
        "could not",
        "cannot",
        "invalid",
        "blocked",
        "rejected",
        "quota",
        "rate limit",
    ];
    if let Some(detail) = run.logs.iter().rev().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        DIAGNOSTIC_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
            .then(|| clean(line))
            .flatten()
    }) {
        return detail;
    }

    run.output
        .lines()
        .rev()
        .find_map(clean)
        .unwrap_or_else(|| "No detailed error was saved for this attempt.".into())
}

impl Store {
    /// Leaf-first chain of unfinished prerequisites, including transitive
    /// dependencies. Used only to offer an explicit rerun path; execution still
    /// revalidates every dependency before dispatch.
    pub(crate) fn missing_prerequisite_chain(&self, id: u64) -> Vec<u64> {
        fn collect(
            store: &Store,
            id: u64,
            seen: &mut std::collections::BTreeSet<u64>,
            out: &mut Vec<u64>,
        ) {
            let Some(task) = store.task(id) else { return };
            let Some(research) = task.research.as_ref() else {
                return;
            };
            for dep in &research.depends_on {
                if !seen.insert(*dep) {
                    continue;
                }
                collect(store, *dep, seen, out);
                if store.research_result(*dep).is_none() {
                    out.push(*dep);
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        collect(self, id, &mut seen, &mut out);
        out
    }

    pub(crate) fn step_state(&self, id: u64) -> StepState {
        let Some(task) = self.task(id) else {
            return StepState::Blocked;
        };
        let latest = self
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == id && r.project_plan.is_none());
        if latest
            .is_some_and(|r| matches!(r.status.as_str(), "Running" | "Cancelling" | "Recovering"))
        {
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
        if let Some(run) = self
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == id && r.project_plan.is_none())
        {
            let detail = run_failure_detail(run);
            if run.status == "Failed" && run.auto_retry_attempt >= crate::retries::MAX_AUTO_RETRIES
            {
                return Some(format!(
                    "This step still failed after 3 automatic retries: {detail} Review the attempt details before retrying manually."
                ));
            }
            let reason = match run.status.as_str() {
                "Interrupted" => Some(format!("Run interrupted: {detail}")),
                "Failed" => Some(format!(
                    "Last attempt failed: {detail} Review the attempt details and retry this step."
                )),
                "Blocked" => Some(format!(
                    "Execution blocked: {detail} Review the attempt details before retrying."
                )),
                "Cancelled" => Some(format!(
                    "Run cancelled: {detail} Retry this step to continue."
                )),
                _ => None,
            };
            if let Some(reason) = reason {
                return Some(reason);
            }
        }
        Some("This step needs attention. Open it to review the attempt.".into())
    }
}
impl TaskApp {
    pub(crate) fn goal_run_state(&self, goal: u64) -> (&'static str, bool, &'static str) {
        if self.store.task_is_archived(goal) {
            return (
                "Archived",
                true,
                "Restore the topic or goal before continuing.",
            );
        }
        if let Some(batch) = self
            .todo_batch
            .as_ref()
            .filter(|b| b.contains_pending(goal))
        {
            if matches!(batch.state, crate::batch::BatchState::Paused(_)) {
                return ("Resume queue", false, "Continue the paused topic queue.");
            }
            if !batch.current_items(&self.store).contains(&goal) {
                return (
                    "Queued",
                    true,
                    "This topic will start automatically after the current topic finishes.",
                );
            }
        }
        if self.goal_busy(goal) {
            return (
                "Running…",
                true,
                "Steps are running or queued. Review the progress below or retry a blocked step individually.",
            );
        }
        if self.store.goal_tasks_complete(goal) || self.store.goal_tasks_present(goal) {
            return (
                "Completed",
                true,
                "All steps have current, completed results.",
            );
        }
        if self.todo_batch.as_ref().is_some_and(|b| {
            b.state != crate::batch::BatchState::Finished && !b.contains_pending(goal)
        }) {
            return (
                "Add to queue",
                false,
                "Add this approved goal to the topic queue. No execution slot is needed to queue it.",
            );
        }
        let started = self.store.runs().iter().any(|r| {
            r.project_plan.is_none() && self.store.owning_research_goal(r.task_id) == Some(goal)
        });
        if !self.store.goal_execution_approved(goal) {
            return if started {
                (
                    "Resume goal",
                    false,
                    "Approve the current plan and continue unfinished steps in dependency order.",
                )
            } else {
                (
                    "Run goal",
                    false,
                    "Review the current plan, approve it and start the research steps.",
                )
            };
        }
        if started {
            (
                "Resume goal",
                false,
                "Continue unfinished steps in dependency order; reuse completed results.",
            )
        } else {
            (
                "Run goal",
                false,
                "Start approved steps in dependency order.",
            )
        }
    }
    pub(crate) fn goal_run_button(
        &self,
        goal: u64,
        element: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::button::Button {
        let (title, disabled, hint) = self.goal_run_state(goal);
        button(element, title)
            .disabled(disabled)
            .tooltip(crate::i18n::tr(hint))
            .on_click(cx.listener(move |a, _, _, cx| {
                a.approve_and_run_research_topic(goal, cx)
            }))
    }
    pub(crate) fn step_execution_panel(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.store.task(id) else {
            return div().into_any_element();
        };
        let Some(research) = task
            .research
            .as_ref()
            .filter(|r| r.kind != crate::research::ResearchKind::Goal)
        else {
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
        let dependencies: Vec<_> = research
            .depends_on
            .iter()
            .copied()
            .filter(|dep| {
                if archived {
                    crate::archive_presentation::validated_history_result(&self.store, *dep)
                        .is_none()
                } else {
                    self.store.research_result(*dep).is_none()
                }
            })
            .collect();
        let latest = self
            .store
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == id && r.project_plan.is_none());
        let reason = if archived {
            "This step belongs to archived work. Restore its topic or goal before running.".into()
        } else if pending {
            "Run goal to approve and start.".into()
        } else if checking {
            "The previous session has not been confirmed stopped. Check its progress before retrying to avoid a duplicate run.".into()
        } else {
            status.reason.clone()
        };
        let colors = crate::appearance::palette(cx);
        let mut panel = col()
            .id("step-execution-panel")
            .w_full()
            .min_w_0()
            .gap_2()
            .py_2()
            .pl_3()
            .when(state != ExecutionStatus::Done, |panel| {
                panel.border_l_2().border_color(rgb(state.color(cx)))
            });
        if state != ExecutionStatus::Done {
            panel = panel
                .child(
                    div()
                        .id("step-execution-status")
                        .aria_label(crate::i18n::tr(self.display_task_status(id).label()))
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(state.color(cx)))
                        .child(crate::i18n::tr(self.display_task_status(id).label()))
                        .map(observed),
                )
                .child(
                    div()
                        .id("step-execution-reason")
                        .text_sm()
                        .text_color(rgb(colors.secondary))
                        .child(selectable(reason))
                        .map(observed),
                );
        }
        if state == ExecutionStatus::Blocked {
            if let Some(run) = latest {
                let detail = run
                    .logs
                    .iter()
                    .rev()
                    .find(|line| {
                        crate::run_errors::is_browser_handoff(line)
                            || ["[error]", "[interrupted]", "[workspace]", "[stream error]"]
                                .iter()
                                .any(|prefix| line.starts_with(prefix))
                    })
                    .map(|line| crate::run_errors::user_message(line).to_owned());
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
            let title = self
                .store
                .task(dep)
                .map(|t| format!("Open {}", self.store.identifier(t)))
                .unwrap_or_else(|| format!("Missing prerequisite {dep}"));
            panel = panel.child(
                button(format!("step-prerequisite-{dep}"), title)
                    .w_full()
                    .tooltip(crate::i18n::tr(
                        self.store
                            .task(dep)
                            .map(|t| t.title.clone())
                            .unwrap_or_default(),
                    ))
                    .disabled(self.store.task(dep).is_none())
                    .on_click(cx.listener(move |a, _, w, cx| a.open_task(dep, w, cx))),
            );
        }
        let mut actions = col().w_full().gap_1();
        if checking {
            actions = actions.child(
                button("step-check-session", "Check previous session")
                    .on_click(cx.listener(|a, _, _, cx| a.check_interrupted_sessions(cx))),
            );
        }
        if cancelled && !archived {
            actions = actions.child(button("step-reopen", "Reopen step").on_click(cx.listener(
                move |a, _, _, cx| {
                    a.apply_task_stage(
                        id,
                        Stage::InProgress,
                        crate::scoped_edit::StageIntent::Reopen,
                        cx,
                    );
                },
            )));
        }
        let run_action = if state == ExecutionStatus::Done {
            quiet_button("step-run", "Run again")
                .self_start()
                .h_7()
                .px_1()
                .text_sm()
                .text_color(rgb(colors.secondary))
        } else {
            primary_button(
                "step-run",
                if latest.is_some() {
                    "Retry step"
                } else {
                    "Run step"
                },
            )
        };
        let action_tooltip = if !dependencies.is_empty() {
            let names: Vec<String> = dependencies
                .iter()
                .map(|dep| {
                    self.store
                        .task(*dep)
                        .map(|task| self.store.identifier(task))
                        .unwrap_or_else(|| format!("Missing prerequisite {dep}"))
                })
                .collect();
            format!(
                "Only this step stays disabled until prerequisites finish: {}. Use Rerun prerequisite to re-run the upstream work.",
                names.join(", ")
            )
        } else if cancelled {
            "Reopen this step before running it again.".into()
        } else if checking {
            "Confirm the previous session before starting a duplicate run.".into()
        } else if pending {
            "Approve this goal's plan before running this step.".into()
        } else if active {
            "This step is already running or queued.".into()
        } else if archived {
            "Restore the archived topic or goal before running this step.".into()
        } else if !self.storage_error.is_empty() {
            format!(
                "Local storage must recover before dispatch: {}",
                self.storage_error
            )
        } else {
            "Run only this step; completed prerequisite results are reused.".into()
        };
        let prerequisite_replay = self.store.missing_prerequisite_chain(id).into_iter().next();
        if !archived {
            actions = actions.child(
                run_action
                    .disabled(
                        pending || checking || active || cancelled || !dependencies.is_empty(),
                    )
                    .tooltip(crate::i18n::tr(action_tooltip))
                    .on_click(cx.listener(move |a, _, _, cx| a.start_run(id, cx))),
            );
        }
        if let Some(prerequisite) = prerequisite_replay {
            let title = self
                .store
                .task(prerequisite)
                .map(|task| self.store.identifier(task))
                .unwrap_or_else(|| format!("prerequisite {prerequisite}"));
            actions = actions.child(
                button("step-rerun-prerequisite", format!("Rerun prerequisite · {title}"))
                    .tooltip(crate::i18n::tr(format!(
                        "Run {title} and its own unfinished prerequisites first. This step stays disabled until its results are current."
                    )))
                    .on_click(cx.listener(move |a, _, _, cx| {
                        a.start_run_background(prerequisite, cx)
                    })),
            );
        }
        let mut links = col().gap_1().child(
            quiet_button(
                "step-open-goal",
                if pending { "Review goal" } else { "Open goal" },
            )
            .self_start()
            .h_7()
            .px_1()
            .text_sm()
            .text_color(rgb(colors.secondary))
            .on_click(cx.listener(move |a, _, w, cx| a.open_task(goal, w, cx))),
        );
        if latest.is_some() {
            links = links.child(
                quiet_button("step-attempt-details", "Technical details")
                    .self_start()
                    .h_7()
                    .px_1()
                    .text_sm()
                    .text_color(rgb(colors.secondary))
                    .on_click(cx.listener(move |a, _, _, cx| {
                        a.view_task_result(id, cx);
                        a.show_run_details = true;
                        a.show_run_result = false;
                        cx.notify();
                    })),
            );
        }
        panel
            .child(actions.child(links))
            .when_some(
                self.task_start_feedback
                    .as_ref()
                    .filter(|(task, _)| *task == id),
                |d, (_, message)| {
                    d.child(crate::views::notice(
                        crate::views::NoticeTone::Danger,
                        message.clone(),
                        cx,
                    ))
                },
            )
            .map(observed)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{StepState, run_failure_detail};
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

    #[test]
    fn blocked_step_reason_uses_the_saved_diagnostic() {
        let mut s = Store::empty();
        s.set_default_model("fixture/model".into());
        s.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into();
        let goal = s
            .create_research_goal("Review".into(), "Analyze earnings".into(), 1)
            .unwrap();
        let ids = s
            .apply_research_plan(
                goal,
                r#"{"tasks":[{"title":"Collect","instructions":"Find filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare filings","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Summarize","kind":"advise","depends_on":[1]}]}"#,
            )
            .unwrap();
        let mut run = s.prepare_run(ids[0]).unwrap();
        run.status = "Failed".into();
        run.logs = vec![
            "[step] start".into(),
            "[workspace] Provider returned 429 rate limit".into(),
        ];
        s.push_run(run);

        assert!(run_failure_detail(s.runs().last().unwrap()).contains("429 rate limit"));
        let reason = s.step_block_reason(ids[0]).unwrap();
        assert!(
            reason.contains("Provider returned 429 rate limit"),
            "the step status must expose the actual failure, not a generic hint"
        );
    }
}
