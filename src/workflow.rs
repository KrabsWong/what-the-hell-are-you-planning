//! User-facing workflow projection and manual transition policy.
//! Stored stages and run records remain authoritative; views never invent a running state.
use crate::{
    batch::TodoBatch,
    domain::{Stage, Store},
    execution_status::ExecutionStatus,
    research::ResearchKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionGroup {
    Waiting,
    Queued,
    Running,
    Attention,
}
impl ExecutionGroup {
    pub const ALL: [Self; 4] = [Self::Waiting, Self::Queued, Self::Running, Self::Attention];
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting for dependencies",
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Attention => "Needs attention",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Attention => "attention",
        }
    }
}

pub fn phase_label(stage: Stage) -> &'static str {
    match stage {
        Stage::InProgress => "Execution",
        _ => stage.label(),
    }
}

#[derive(Clone, Debug)]
pub struct TaskStatus {
    pub phase: Stage,
    pub execution: ExecutionStatus,
    pub group: Option<ExecutionGroup>,
    pub reason: String,
}
impl TaskStatus {
    pub fn label(&self) -> String {
        let phase = phase_label(self.phase);
        if self.execution.label() == self.phase.label() || self.execution == ExecutionStatus::Done {
            phase.into()
        } else {
            format!("{phase} · {}", self.execution.label())
        }
    }
}

impl Store {
    pub fn task_status(&self, id: u64, batch: Option<&TodoBatch>) -> TaskStatus {
        use ExecutionStatus::*;
        let mut execution = self.execution_status(id, batch);
        let task = self.task(id);
        let goal = self.owning_research_goal(id).unwrap_or(id);
        let submitted = batch.is_some_and(|b| b.contains_pending(goal))
            || self.runs.iter().any(|r| {
                r.project_plan.is_none()
                    && (r.task_id == id
                        || (goal == id && self.owning_research_goal(r.task_id) == Some(id)))
                    && matches!(
                        r.status.as_str(),
                        "Queued" | "Running" | "Cancelling" | "Recovering"
                    )
            });
        if task.is_some_and(|t| t.stage == crate::domain::Stage::Cancelled)
            && !matches!(execution, Running | Queued | Retrying)
        {
            execution = Stage(crate::domain::Stage::Cancelled);
        }
        let stopped = !submitted
            && task.is_some_and(|t| t.stage == crate::domain::Stage::InProgress)
            && matches!(execution, Ready | Waiting | Stage(crate::domain::Stage::InProgress));
        if stopped {
            execution = Paused;
        }
        let phase = match execution {
            _ if task.is_some_and(|t| t.stage == crate::domain::Stage::Cancelled)
                && !matches!(execution, Running | Queued | Retrying) =>
            {
                crate::domain::Stage::Cancelled
            }
            Running | Queued | Retrying | Paused => crate::domain::Stage::InProgress,
            Blocked if task.is_some_and(|t| t.stage != crate::domain::Stage::Cancelled) => {
                crate::domain::Stage::InProgress
            }
            Done => crate::domain::Stage::Done,
            Approval => crate::domain::Stage::Backlog,
            Waiting | Ready => {
                if submitted {
                    crate::domain::Stage::InProgress
                } else {
                    crate::domain::Stage::Planned
                }
            }
            Stage(stage) => stage,
            _ => task
                .map(|t| t.stage)
                .unwrap_or(crate::domain::Stage::Backlog),
        };
        let group = (phase == crate::domain::Stage::InProgress).then_some(match execution {
            Waiting => ExecutionGroup::Waiting,
            Queued | Retrying | Ready => ExecutionGroup::Queued,
            Running => ExecutionGroup::Running,
            _ => ExecutionGroup::Attention,
        });
        let mut reason = if stopped {
            "Execution is not queued. Submit again to continue; completed results are kept."
                .into()
        } else if execution == Waiting {
            let missing = task
                .and_then(|t| t.research.as_ref())
                .map(|r| {
                    r.depends_on
                        .iter()
                        .filter(|dep| {
                            self.task(**dep)
                                .is_none_or(|t| t.stage != crate::domain::Stage::Done)
                                || self.research_result(**dep).is_none()
                        })
                        .map(|dep| {
                            self.task(*dep)
                                .map(|t| t.title.clone())
                                .unwrap_or_else(|| format!("Missing task {dep}"))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            format!(
                "Waiting for {}. {}",
                if missing.is_empty() {
                    "prerequisite results".into()
                } else {
                    missing.join(", ")
                },
                if submitted {
                    "Will queue automatically when dependencies are complete."
                } else {
                    "Submit the goal to execute its dependency chain."
                }
            )
        } else if execution == Retrying {
            let retry = self.runs.iter().rev().find(|r| {
                r.status == "Queued"
                    && r.auto_retry_attempt > 0
                    && (r.task_id == id
                        || (goal == id && self.owning_research_goal(r.task_id) == Some(id)))
            });
            retry
                .map(|r| {
                    format!(
                        "Retry {}/{} · eligible in {}s, then waits for an execution slot.",
                        r.auto_retry_attempt,
                        crate::retries::MAX_AUTO_RETRIES,
                        r.retry_delay_remaining(chrono::Utc::now().timestamp())
                    )
                })
                .unwrap_or_else(|| execution.hint().into())
        } else if execution == Blocked {
            self.step_block_reason(id)
                .unwrap_or_else(|| execution.hint().into())
        } else if execution == Stage(crate::domain::Stage::InProgress) {
            "No active run. Open the task to submit execution or review its result.".into()
        } else if execution == Stage(crate::domain::Stage::Review) {
            "Review the completed work, then mark it Done.".into()
        } else {
            execution.hint().into()
        };
        if task.is_some_and(|t| {
            t.research
                .as_ref()
                .is_some_and(|r| r.kind == ResearchKind::Goal)
        }) {
            let states: Vec<_> = self
                .goal_step_ids(id)
                .into_iter()
                .map(|step| self.execution_status(step, batch))
                .collect();
            let summary = ExecutionStatus::ALL
                .into_iter()
                .filter_map(|state| {
                    let count = states.iter().filter(|value| **value == state).count();
                    (count > 0).then(|| format!("{count} {}", state.label().to_lowercase()))
                })
                .collect::<Vec<_>>()
                .join(" · ");
            if !summary.is_empty() {
                reason = format!("{summary}. {reason}");
            }
        }
        TaskStatus {
            phase,
            execution,
            group,
            reason,
        }
    }

    /// User changes only. Runtime completion, retry and interruption recovery use
    /// set_stage separately; they must not be mistaken for a manual backward move.
    pub fn validate_manual_transition(
        &self,
        id: u64,
        target: Stage,
        batch: Option<&TodoBatch>,
    ) -> Result<(), String> {
        let task = self.task(id).ok_or("Task not found.")?;
        if self.task_is_archived(id) {
            return Err("Restore this task before changing its status.".into());
        }
        if task.parent_id.is_some() && task.research.is_some() {
            return Err("Research steps follow their dependencies automatically. Submit or review the complete goal.".into());
        }
        let status = self.task_status(id, batch);
        let goal = self.owning_research_goal(id).unwrap_or(id);
        let busy = batch.is_some_and(|b| b.contains_pending(goal))
            || self.runs.iter().any(|r| {
                r.project_plan.is_none()
                    && (r.task_id == id || self.owning_research_goal(r.task_id) == Some(id))
                    && matches!(
                        r.status.as_str(),
                        "Queued" | "Running" | "Cancelling" | "Recovering"
                    )
            });
        if busy {
            return Err("This task is submitted or running. Use the queue controls to pause, continue or stop it.".into());
        }
        if target == status.phase && target != Stage::InProgress {
            return Ok(());
        }
        if target == Stage::InProgress {
            if !matches!(status.phase, Stage::Planned | Stage::InProgress) {
                return Err(
                    "Move the task to Todo and approve its plan before submitting execution."
                        .into(),
                );
            }
            if task.research.is_some() && !self.goal_execution_approved(id) {
                return Err(
                    "Review and approve the complete research plan before executing.".into(),
                );
            }
            if task.research.is_some() {
                self.validate_goal_dependencies(id)?;
            }
            return Ok(());
        }
        let allowed = matches!(
            (status.phase, target),
            (Stage::Backlog, Stage::Planned)
                | (Stage::Planned, Stage::Backlog)
                | (Stage::InProgress, Stage::Review)
                | (Stage::Review, Stage::Done)
                | (Stage::Cancelled, Stage::Planned)
        ) || (target == Stage::Cancelled
            && !matches!(status.phase, Stage::Done | Stage::Cancelled));
        if !allowed {
            return Err(format!(
                "Cannot move {} → {}. Follow the workflow order; running work cannot move back to Todo.",
                phase_label(status.phase),
                phase_label(target)
            ));
        }
        if task
            .research
            .as_ref()
            .is_some_and(|r| r.kind == ResearchKind::Goal)
        {
            if matches!(target, Stage::Review | Stage::Done) && !self.goal_tasks_complete(id) {
                return Err("Complete all research dependencies and obtain current results before finishing this goal.".into());
            }
            if target == Stage::Planned {
                self.validate_goal_dependencies(id)?;
            }
        }
        if target == Stage::Review
            && task.research.is_none()
            && self
                .runs
                .iter()
                .rev()
                .find(|r| r.task_id == id && r.project_plan.is_none())
                .is_some_and(|r| r.status != "Succeeded")
        {
            return Err(
                "Resolve the failed or interrupted execution before moving this task to review."
                    .into(),
            );
        }
        // Invalidating a prerequisite is not a status-edit operation.
        if target != Stage::Done
            && self.tasks.iter().any(|t| {
                t.research
                    .as_ref()
                    .is_some_and(|r| r.depends_on.contains(&id))
                    && (t.stage == Stage::Done
                        || self.runs.iter().any(|r| {
                            r.task_id == t.id
                                && matches!(r.status.as_str(), "Queued" | "Running" | "Recovering")
                        }))
            })
        {
            return Err("A submitted or completed task depends on this result. Stop dependent work before changing it.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::AllStatus, batch::BatchState};
    fn fixture() -> (Store, u64, Vec<u64>, TodoBatch) {
        let mut store = Store::empty();
        store.default_model = "fixture/model".into();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into();
        let goal = store
            .create_research_goal("Goal".into(), "Scope".into(), 1)
            .unwrap();
        let ids = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        let batch = TodoBatch {
            project_id: 1,
            items: vec![goal],
            completed: 0,
            runs_completed: 0,
            state: BatchState::Running,
            run_ids: Default::default(),
            done_items: vec![],
            failed_items: vec![],
            failed_tasks: vec![],
        };
        (store, goal, ids, batch)
    }
    #[test]
    fn submission_changes_phase_without_faking_running_and_keeps_dependencies_waiting() {
        let (store, goal, ids, batch) = fixture();
        assert_eq!(store.task_status(goal, None).phase, Stage::Planned);
        assert_eq!(store.task_status(ids[0], None).group, None);
        assert_eq!(
            store.task_status(goal, Some(&batch)).group,
            Some(ExecutionGroup::Queued)
        );
        let waiting = store.task_status(ids[1], Some(&batch));
        assert_eq!(waiting.phase, Stage::InProgress);
        assert_eq!(waiting.group, Some(ExecutionGroup::Waiting));
        assert!(waiting.reason.contains("Collect"));
        assert!(waiting.reason.contains("automatically"));
        assert!(AllStatus::Stage(Stage::InProgress).matches(&waiting));
        assert!(AllStatus::Execution(ExecutionGroup::Waiting).matches(&waiting));
        assert!(!AllStatus::Stage(Stage::Planned).matches(&waiting));
        assert!(store.runs.is_empty());
    }
    #[test]
    fn aggregate_running_wins_over_waiting_and_pause_and_retry_have_separate_groups() {
        let (mut store, goal, ids, mut batch) = fixture();
        let mut run = store.prepare_run(ids[0]).unwrap();
        run.status = "Running".into();
        store.runs.push(run);
        batch.state = BatchState::Paused("Paused".into());
        let state = store.task_status(goal, Some(&batch));
        assert_eq!(state.group, Some(ExecutionGroup::Running));
        assert!(state.reason.contains("1 running"));
        assert_eq!(
            store.task_status(ids[1], Some(&batch)).group,
            Some(ExecutionGroup::Attention)
        );
        batch.state = BatchState::Running;
        store.runs[0].status = "Queued".into();
        store.runs[0].auto_retry_attempt = 1;
        assert_eq!(
            store.task_status(goal, Some(&batch)).group,
            Some(ExecutionGroup::Queued)
        );
        store.runs[0].status = "Failed".into();
        assert_eq!(
            store.task_status(goal, Some(&batch)).group,
            Some(ExecutionGroup::Attention)
        );
    }
    #[test]
    fn manual_transitions_reject_skips_backwards_moves_and_busy_ancestors() {
        let (mut store, goal, ids, batch) = fixture();
        let id = store
            .create_task("Ordinary", 1, Stage::Backlog, None)
            .unwrap();
        let before = store.clone();
        assert!(
            store
                .validate_manual_transition(id, Stage::InProgress, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(id, Stage::Done, None)
                .is_err()
        );
        assert_eq!(store, before);
        assert!(
            store
                .validate_manual_transition(id, Stage::Planned, None)
                .is_ok()
        );
        store.set_stage(id, Stage::Planned).unwrap();
        assert!(
            store
                .validate_manual_transition(id, Stage::InProgress, None)
                .is_ok()
        );
        store.set_stage(id, Stage::InProgress).unwrap();
        assert!(
            store
                .validate_manual_transition(id, Stage::Planned, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(id, Stage::Backlog, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(goal, Stage::Backlog, Some(&batch))
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(goal, Stage::Done, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(ids[1], Stage::InProgress, None)
                .is_err()
        );
        let mut run = store.prepare_run(ids[0]).unwrap();
        run.status = "Running".into();
        store.runs.push(run);
        assert!(
            store
                .validate_manual_transition(goal, Stage::Cancelled, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(goal, Stage::Planned, None)
                .is_err()
        );
    }
    #[test]
    fn ordinary_success_enters_review_and_internal_recovery_remains_possible() {
        let (mut store, _, _, _) = fixture();
        let id = store
            .create_task("Ordinary", 1, Stage::Planned, None)
            .unwrap();
        let mut run = store.prepare_run(id).unwrap();
        run.status = "Succeeded".into();
        store.runs.push(run);
        assert_eq!(store.task_status(id, None).phase, Stage::Review);
        assert!(
            store
                .validate_manual_transition(id, Stage::Planned, None)
                .is_err()
        );
        assert!(
            store
                .validate_manual_transition(id, Stage::Done, None)
                .is_ok()
        );
        store.set_stage(id, Stage::Done).unwrap();
        assert_eq!(store.task_status(id, None).phase, Stage::Done);
        // Runtime rollback is deliberately separate from manual actions.
        store.set_stage(id, Stage::InProgress).unwrap();
        store.set_stage(id, Stage::Planned).unwrap();
    }
    #[test]
    fn approval_archiving_and_dependency_validation_cannot_be_bypassed() {
        let (mut store, goal, ids, _) = fixture();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.task_mut(goal).unwrap().approved_plan = None;
        assert!(
            store
                .validate_manual_transition(goal, Stage::InProgress, None)
                .is_err()
        );
        store
            .task_mut(ids[0])
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .depends_on = vec![ids[1]];
        assert!(
            store
                .validate_manual_transition(goal, Stage::Planned, None)
                .is_err()
        );
        store.task_mut(goal).unwrap().archived = true;
        assert!(
            store
                .validate_manual_transition(goal, Stage::Planned, None)
                .is_err()
        );
    }
}
