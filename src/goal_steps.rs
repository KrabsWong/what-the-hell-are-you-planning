//! Goal-local execution state and actions shared by task surfaces.
use crate::research::ResearchKind;
use crate::{
    app::TaskApp,
    domain::{AgentRun, Stage, Store},
    execution_status::ExecutionStatus,
    views::*,
};
use gpui_kit::{component::Disableable, *};

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
            .tooltip(crate::i18n::tr_string(hint))
            .on_click(cx.listener(move |a, _, _, cx| a.approve_and_run_research_topic(goal, cx)))
    }
    pub(crate) fn step_run_button(
        &self,
        id: u64,
        element: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::button::Button {
        let Some(task) = self.store.task(id) else {
            return button(element, "Run step").disabled(true);
        };
        let Some(research) = task
            .research
            .as_ref()
            .filter(|r| r.kind != ResearchKind::Goal)
        else {
            return button(element, "Run step").disabled(true);
        };
        let state = self.display_task_status(id).execution;
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
        let disabled = archived
            || pending
            || checking
            || active
            || cancelled
            || !dependencies.is_empty()
            || !self.storage_error.is_empty();
        let label = if state == ExecutionStatus::Done {
            "Run again"
        } else if latest.is_some() {
            "Retry step"
        } else {
            "Run step"
        };
        let tooltip = if archived {
            "Restore the archived topic or goal before running this step."
        } else if pending {
            "Approve this goal's plan before running this step."
        } else if checking {
            "Confirm the previous session before starting a duplicate run."
        } else if cancelled {
            "Reopen this step before running it again."
        } else if active {
            "This step is already running or queued."
        } else if !dependencies.is_empty() {
            "Finish the upstream steps before running this one."
        } else if !self.storage_error.is_empty() {
            "Local storage must recover before dispatch."
        } else {
            "Run only this step; completed prerequisite results are reused."
        };
        let action = primary_button(element, label);
        action
            .disabled(disabled)
            .tooltip(crate::i18n::tr(tooltip))
            .on_click(cx.listener(move |a, _, _, cx| a.start_run(id, cx)))
    }

    pub(crate) fn step_recovery_actions(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.store.task(id) else {
            return div().into_any_element();
        };
        let Some(research) = task
            .research
            .as_ref()
            .filter(|r| r.kind != ResearchKind::Goal)
        else {
            return div().into_any_element();
        };
        let archived = self.store.task_is_archived(id);
        let checking = self.recovery_blocks_task(id);
        let cancelled = task.stage == Stage::Cancelled;
        let mut actions = col().w_full().min_w_0().gap_1();
        if checking {
            actions = actions.child(
                button("step-check-session", "Check previous session").on_click(cx.listener(
                    |a, _, _, cx| a.check_interrupted_sessions(cx),
                )),
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
        for dep in research
            .depends_on
            .iter()
            .copied()
            .filter(|dep| self.store.research_result(*dep).is_none())
        {
            let Some(task) = self.store.task(dep) else {
                continue;
            };
            let title = format!("{} · {}", self.store.identifier(task), task.title);
            actions = actions.child(
                link_text(format!("step-prerequisite-{dep}"), title, cx).on_click(
                    cx.listener(move |a, _, w, cx| a.open_task(dep, w, cx)),
                ),
            );
        }
        let blocked_detail = (self.display_task_status(id).execution == ExecutionStatus::Blocked)
            .then(|| {
                self.store
                    .runs()
                    .iter()
                    .rev()
                    .find(|r| r.task_id == id && r.project_plan.is_none())
            })
            .flatten()
            .and_then(|run| {
                run.logs.iter().rev().find(|line| {
                    crate::run_errors::is_browser_handoff(line)
                        || ["[error]", "[interrupted]", "[workspace]", "[stream error]"]
                            .iter()
                            .any(|prefix| line.starts_with(prefix))
                })
            })
            .map(|line| crate::run_errors::user_message(line).to_owned());
        if let Some(detail) = blocked_detail {
            actions = actions.child(crate::views::notice(
                crate::views::NoticeTone::Danger,
                detail,
                cx,
            ));
        }
        actions.into_any_element()
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
