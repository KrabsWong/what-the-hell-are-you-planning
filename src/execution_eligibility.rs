//! Pure execution eligibility shared by automation dispatch and status presentation.
//!
//! The result contains facts only: owner, current attempt, approval, ordered-goal
//! barrier, phase barrier, unmet dependencies and automation-pool wait. Rendering
//! decisions and dispatch decisions are derived from the same value.
use crate::{
    automation::{AutomationOccurrence, OccurrenceStatus},
    domain::{Stage, Store},
    execution_status::ExecutionStatus,
    research::ResearchKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EligibilityOwner {
    OrdinaryResearch {
        goal: u64,
    },
    AutomationPeriod(AutomationOwnership),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutomationOwnership {
    pub automation: u64,
    /// `None` when no occurrence exists, or when stored ownership is ambiguous.
    pub occurrence: Option<u64>,
    pub goal: u64,
    pub goal_index: usize,
    pub goal_count: usize,
    pub period: PeriodState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PeriodState {
    Missing,
    Ambiguous,
    Queued,
    Running,
    WaitingRetry,
    Verifying,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveAttempt {
    None,
    Queued,
    Running,
    Retrying { attempt: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepOutcome {
    Open,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalBarrier {
    pub earlier_goal: u64,
    pub earlier_goal_title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhaseBarrier {
    /// The earlier phase that must finish before this step can run.
    pub phase: ResearchKind,
    pub unfinished: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PoolWait {
    AutomationSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EligibilityDecision {
    Done,
    Blocked,
    Approval,
    Running,
    Stopping,
    Retrying,
    Queued,
    Waiting,
    Ended,
    Inactive,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionEligibility {
    pub owner: EligibilityOwner,
    pub active_attempt: ActiveAttempt,
    pub approval_pending: bool,
    pub outcome: StepOutcome,
    pub goal_barrier: Option<GoalBarrier>,
    pub phase_barrier: Option<PhaseBarrier>,
    pub unmet_dependencies: Vec<u64>,
    pub pool_wait: Option<PoolWait>,
    /// An explicit period retry reopened this occurrence while its historical
    /// failed attempt remains in the run list. The failure is not erased; it is
    /// simply no longer the current authority for whether the step may run.
    pub retry_authority: bool,
}

impl EligibilityOwner {
    pub(crate) fn is_automation(&self) -> bool {
        matches!(self, Self::AutomationPeriod(_))
    }

    pub(crate) fn period(&self) -> Option<PeriodState> {
        match self {
            Self::AutomationPeriod(owner) => Some(owner.period),
            Self::OrdinaryResearch { .. } => None,
        }
    }
}

impl ExecutionEligibility {
    pub(crate) fn decision(&self) -> EligibilityDecision {
        if self.outcome == StepOutcome::Cancelled {
            return EligibilityDecision::Blocked;
        }
        if self.outcome == StepOutcome::Done {
            return EligibilityDecision::Done;
        }
        let period = self.owner.period();
        match self.active_attempt {
            ActiveAttempt::Running if period == Some(PeriodState::Ended) => {
                return EligibilityDecision::Stopping;
            }
            ActiveAttempt::Running => return EligibilityDecision::Running,
            ActiveAttempt::Retrying { .. } => return EligibilityDecision::Retrying,
            ActiveAttempt::Queued => return EligibilityDecision::Queued,
            ActiveAttempt::None => {}
        }
        if self.outcome == StepOutcome::Failed && !self.retry_authority {
            return EligibilityDecision::Blocked;
        }
        if self.approval_pending {
            return EligibilityDecision::Approval;
        }
        match period {
            Some(PeriodState::Verifying) => return EligibilityDecision::Waiting,
            Some(PeriodState::Ended) => return EligibilityDecision::Ended,
            Some(PeriodState::Missing | PeriodState::Ambiguous) => {
                return EligibilityDecision::Inactive
            }
            _ => {}
        }
        if self.goal_barrier.is_some()
            || self.phase_barrier.is_some()
            || !self.unmet_dependencies.is_empty()
        {
            return EligibilityDecision::Waiting;
        }
        if period == Some(PeriodState::Queued) {
            return EligibilityDecision::Queued;
        }
        EligibilityDecision::Ready
    }

    /// Dispatch inside the period. A queued period is preparable: its run is
    /// created now and the application pool decides when the process starts.
    pub(crate) fn is_executable_for_preparation(&self) -> bool {
        if !self.owner.is_automation()
            || self.active_attempt != ActiveAttempt::None
            || !(self.outcome == StepOutcome::Open
                || (self.outcome == StepOutcome::Failed && self.retry_authority))
            || self.approval_pending
            || self.goal_barrier.is_some()
            || self.phase_barrier.is_some()
            || !self.unmet_dependencies.is_empty()
        {
            return false;
        }
        matches!(
            self.owner.period(),
            Some(PeriodState::Queued | PeriodState::Running | PeriodState::WaitingRetry)
        )
    }

    pub(crate) fn period_is_active(&self) -> bool {
        matches!(
            self.owner.period(),
            Some(
                PeriodState::Queued
                    | PeriodState::Running
                    | PeriodState::WaitingRetry
                    | PeriodState::Verifying
            )
        )
    }

    pub(crate) fn execution_status(&self) -> ExecutionStatus {
        match self.decision() {
            EligibilityDecision::Done => ExecutionStatus::Done,
            EligibilityDecision::Blocked => ExecutionStatus::Blocked,
            EligibilityDecision::Approval => ExecutionStatus::Approval,
            EligibilityDecision::Running => ExecutionStatus::Running,
            EligibilityDecision::Stopping => ExecutionStatus::Stopping,
            EligibilityDecision::Retrying => ExecutionStatus::Retrying,
            EligibilityDecision::Queued => ExecutionStatus::Queued,
            EligibilityDecision::Waiting
            | EligibilityDecision::Ended
            | EligibilityDecision::Inactive => ExecutionStatus::Waiting,
            EligibilityDecision::Ready => ExecutionStatus::Ready,
        }
    }

    /// Exact short phrase for list, board and period/progress rows. `None`
    /// keeps the ordinary phase/execution label.
    pub(crate) fn label_override(&self) -> Option<String> {
        let decision = self.decision();
        if decision != EligibilityDecision::Waiting {
            if decision == EligibilityDecision::Queued
                && self.active_attempt == ActiveAttempt::None
                && self.owner.period() == Some(PeriodState::Queued)
            {
                return Some("Queued for automation slot".into());
            }
            return None;
        }
        if self.goal_barrier.is_some() {
            return Some("Waiting for previous goal".into());
        }
        if let Some(barrier) = &self.phase_barrier {
            return Some(match barrier.phase {
                ResearchKind::Collect => "Waiting for collection",
                ResearchKind::Analyze => "Waiting for analysis",
                ResearchKind::Advise => "Waiting for the final report",
                ResearchKind::Goal => "Waiting for the plan",
            }
            .into());
        }
        None
    }

    pub(crate) fn reason(&self, store: &Store, id: u64) -> String {
        let period = self.owner.period();
        if self.retry_authority
            && matches!(
                self.decision(),
                EligibilityDecision::Queued | EligibilityDecision::Ready
            )
        {
            return "This period was explicitly reopened with its original approved version. Finished steps keep their results; the historical failed attempt stays in run history and only unfinished work runs again."
                .into();
        }
        match self.decision() {
            EligibilityDecision::Done => "Completed result is available.".into(),
            EligibilityDecision::Blocked => {
                if let Some(reason) = store.step_block_reason(id) {
                    if period == Some(PeriodState::Ended) {
                        return format!(
                            "{reason}. The period keeps its original approved version; retrying the period runs only unfinished work."
                        );
                    }
                    return reason;
                }
                "The last attempt needs attention. Open the step for details.".into()
            }
            EligibilityDecision::Approval => {
                "Review and approve the plan before this period can run.".into()
            }
            EligibilityDecision::Running => {
                "This step is running in its automation period.".into()
            }
            EligibilityDecision::Stopping => {
                "The period has ended; a background session from this step is still shutting down and cannot extend the period.".into()
            }
            EligibilityDecision::Retrying => {
                let attempt = match self.active_attempt {
                    ActiveAttempt::Retrying { attempt } => attempt,
                    _ => 0,
                };
                format!(
                    "Retry {attempt}/{} is scheduled for this period. It waits for the automation pool, not the Research queue.",
                    crate::retries::MAX_AUTO_RETRIES
                )
            }
            EligibilityDecision::Queued => {
                if period == Some(PeriodState::Queued) {
                    "Queued for an automation slot; this period starts automatically when a slot is free. Research queue admission is not involved.".into()
                } else {
                    "Queued to start in this automation period as soon as its earlier step finishes.".into()
                }
            }
            EligibilityDecision::Waiting => self.waiting_reason(store),
            EligibilityDecision::Ended => {
                let detail = self.waiting_reason(store);
                format!(
                    "{detail} This period ended before this step ran. Retry the period to continue with its original approved version and saved results."
                )
            }
            EligibilityDecision::Inactive => {
                "This step belongs to an automation period that is not active. Open the automation history to review or retry the period.".into()
            }
            EligibilityDecision::Ready => {
                "Ready in this running period; the automation starts this step after earlier steps finish. No Research queue admission is involved.".into()
            }
        }
    }

    fn waiting_reason(&self, store: &Store) -> String {
        if let Some(barrier) = &self.goal_barrier {
            return format!(
                "Waiting for the previous goal in this period to finish: {}. Later goals never start early.",
                barrier.earlier_goal_title
            );
        }
        if let Some(barrier) = &self.phase_barrier {
            let titles = barrier
                .unfinished
                .iter()
                .map(|id| step_title(store, *id))
                .collect::<Vec<_>>()
                .join(", ");
            let phrase = match barrier.phase {
                ResearchKind::Collect => "collection",
                ResearchKind::Analyze => "analysis",
                ResearchKind::Advise => "the final report",
                ResearchKind::Goal => "the plan",
            };
            let mut reason = format!(
                "Waiting for {phrase}: {titles}. This period finishes every earlier-phase step first."
            );
            if !self.unmet_dependencies.is_empty() {
                reason.push_str(&format!(
                    " Also waiting for prerequisite results: {}.",
                    dependency_titles(store, &self.unmet_dependencies)
                ));
            }
            return reason;
        }
        if !self.unmet_dependencies.is_empty() {
            return format!(
                "Waiting for prerequisite results: {}.",
                dependency_titles(store, &self.unmet_dependencies)
            );
        }
        if self.owner.period() == Some(PeriodState::Verifying) {
            return "Checking this period's previous session after restart. It cannot start a new attempt until the check finishes.".into();
        }
        "Waiting for this automation period to become runnable.".into()
    }
}

fn step_title(store: &Store, id: u64) -> String {
    store
        .task(id)
        .map(|task| task.title.clone())
        .unwrap_or_else(|| format!("Missing step {id}"))
}

fn dependency_titles(store: &Store, ids: &[u64]) -> String {
    ids.iter()
        .map(|id| step_title(store, *id))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Store {
    pub(crate) fn execution_eligibility(&self, id: u64) -> ExecutionEligibility {
        let Some(_task) = self.task(id) else {
            return ExecutionEligibility {
                owner: EligibilityOwner::OrdinaryResearch { goal: id },
                active_attempt: ActiveAttempt::None,
                approval_pending: false,
                outcome: StepOutcome::Cancelled,
                goal_barrier: None,
                phase_barrier: None,
                unmet_dependencies: vec![],
                pool_wait: None,
                retry_authority: false,
            };
        };
        let goal = self.owning_research_goal(id).unwrap_or(id);
        let owner = match self.automation_owner_for_goal(goal) {
            Some(owner) => EligibilityOwner::AutomationPeriod(owner),
            None => EligibilityOwner::OrdinaryResearch { goal },
        };
        self.execution_eligibility_with_owner(id, owner)
    }

    /// The only active occurrence that contains this goal. Ambiguous stored
    /// ownership returns `None` rather than guessing from a latest ID/time.
    pub(crate) fn unique_active_occurrence_for_goal(
        &self,
        goal: u64,
    ) -> Option<&AutomationOccurrence> {
        let mut active = self
            .all_occurrences()
            .iter()
            .filter(|occurrence| {
                occurrence.status.is_active() && occurrence_contains_goal(occurrence, goal)
            });
        let first = active.next()?;
        active.next().is_none().then_some(first)
    }

    pub(crate) fn next_eligible_goal_step(&self, goal: u64) -> Option<u64> {
        let children = self.research_children(goal);
        for phase in [
            ResearchKind::Collect,
            ResearchKind::Analyze,
            ResearchKind::Advise,
        ] {
            let unfinished = children.iter().any(|id| {
                self.task(*id).is_some_and(|task| {
                    task.stage != Stage::Cancelled
                        && self.research_result(*id).is_none()
                        && task
                            .research
                            .as_ref()
                            .is_some_and(|research| research.kind == phase)
                })
            });
            if !unfinished {
                continue;
            }
            return children.iter().copied().find(|id| {
                self.task(*id)
                    .and_then(|task| task.research.as_ref())
                    .is_some_and(|research| research.kind == phase)
                    && self.execution_eligibility(*id).is_executable_for_preparation()
            });
        }
        children
            .into_iter()
            .find(|id| self.execution_eligibility(*id).is_executable_for_preparation())
    }

    /// Select the next step for a whole occurrence. Goals stay in approved
    /// order; a later goal is never selected while an earlier goal is unfinished.
    pub(crate) fn next_occurrence_step(
        &self,
        occurrence: &AutomationOccurrence,
    ) -> Result<Option<u64>, String> {
        if self.occurrence_has_active_attempt(occurrence.id) {
            return Err("This period already has a running or queued step.".into());
        }
        let goals = occurrence_goals(occurrence);
        if goals.is_empty() {
            return Err("The occurrence has no research goal.".into());
        }
        for goal in goals {
            self.ready_research_tasks(goal)?;
            if let Some(step) = self.next_eligible_goal_step(goal) {
                return Ok(Some(step));
            }
            if !self.goal_tasks_complete(goal) {
                return Err("The next goal of this period is not runnable yet.".into());
            }
        }
        Ok(None)
    }

    fn occurrence_has_active_attempt(&self, occurrence: u64) -> bool {
        self.runs().iter().any(|run| {
            run.automation_occurrence_id == Some(occurrence)
                && matches!(
                    run.status.as_str(),
                    "Queued" | "Running" | "Cancelling" | "Recovering"
                )
        })
    }

    fn execution_eligibility_with_owner(
        &self,
        id: u64,
        owner: EligibilityOwner,
    ) -> ExecutionEligibility {
        let task = self.task(id).expect("eligibility requires an existing task");
        let latest_run = self
            .runs()
            .iter()
            .rev()
            .find(|run| run.task_id == id && run.project_plan.is_none());
        let active_attempt = match latest_run.map(|run| run.status.as_str()) {
            Some("Running" | "Cancelling" | "Recovering") => ActiveAttempt::Running,
            Some("Queued") if latest_run.is_some_and(|run| run.auto_retry_attempt > 0) => {
                ActiveAttempt::Retrying {
                    attempt: latest_run.unwrap().auto_retry_attempt,
                }
            }
            Some("Queued") => ActiveAttempt::Queued,
            _ => ActiveAttempt::None,
        };
        let outcome = if task.stage == Stage::Cancelled {
            StepOutcome::Cancelled
        } else if self.task_is_archived(id) {
            StepOutcome::Cancelled
        } else if self.research_result(id).is_some() {
            StepOutcome::Done
        } else if latest_run.is_some_and(|run| {
            matches!(
                run.status.as_str(),
                "Failed" | "Blocked" | "Interrupted" | "Cancelled"
            )
        }) {
            StepOutcome::Failed
        } else {
            StepOutcome::Open
        };
        let approval_pending = self.research_pending_approval(id);
        let goal = self.owning_research_goal(id).unwrap_or(id);
        let (goal_barrier, phase_barrier) = match &owner {
            EligibilityOwner::AutomationPeriod(ownership) => (
                self.goal_barrier_for(ownership),
                self.phase_barrier_for(goal, task.research.as_ref().map(|r| r.kind)),
            ),
            EligibilityOwner::OrdinaryResearch { .. } => (None, None),
        };
        let unmet_dependencies = task
            .research
            .as_ref()
            .map(|research| {
                research
                    .depends_on
                    .iter()
                    .copied()
                    .filter(|dep| {
                        self.task(*dep).is_none_or(|dependency| {
                            dependency.archived
                                || dependency.stage != Stage::Done
                                || dependency
                                    .research
                                    .as_ref()
                                    .is_none_or(|value| value.goal_id != goal)
                                || self.research_result(*dep).is_none()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let pool_wait = (owner.period() == Some(PeriodState::Queued))
            .then_some(PoolWait::AutomationSlot);
        let retry_authority = match &owner {
            EligibilityOwner::AutomationPeriod(ownership)
                if ownership.period == PeriodState::Queued && active_attempt == ActiveAttempt::None =>
            {
                ownership
                    .occurrence
                    .and_then(|occurrence| self.occurrence(occurrence))
                    .is_some_and(|occurrence| occurrence.retry_requested_at.is_some())
            }
            _ => false,
        };
        ExecutionEligibility {
            owner,
            active_attempt,
            approval_pending,
            outcome,
            goal_barrier,
            phase_barrier,
            unmet_dependencies,
            pool_wait,
            retry_authority,
        }
    }

    fn phase_barrier_for(
        &self,
        goal: u64,
        kind: Option<ResearchKind>,
    ) -> Option<PhaseBarrier> {
        let kind = kind?;
        let children = self.research_children(goal);
        for phase in [
            ResearchKind::Collect,
            ResearchKind::Analyze,
            ResearchKind::Advise,
        ] {
            if phase == kind {
                break;
            }
            let unfinished = children
                .iter()
                .copied()
                .filter(|id| {
                    self.task(*id).is_some_and(|task| {
                        task.stage != Stage::Cancelled
                            && self.research_result(*id).is_none()
                            && task
                                .research
                                .as_ref()
                                .is_some_and(|research| research.kind == phase)
                    })
                })
                .collect::<Vec<_>>();
            if !unfinished.is_empty() {
                return Some(PhaseBarrier { phase, unfinished });
            }
        }
        None
    }

    fn goal_barrier_for(&self, owner: &AutomationOwnership) -> Option<GoalBarrier> {
        if owner.goal_index == 0 {
            return None;
        }
        let occurrence = owner.occurrence.and_then(|id| self.occurrence(id))?;
        let goals = occurrence_goals(occurrence);
        for earlier in goals.iter().copied().take(owner.goal_index) {
            if !self.goal_tasks_complete(earlier) {
                return Some(GoalBarrier {
                    earlier_goal: earlier,
                    earlier_goal_title: step_title(self, earlier),
                });
            }
        }
        None
    }

    fn automation_owner_for_goal(&self, goal: u64) -> Option<AutomationOwnership> {
        let task = self.task(goal)?;
        let (occurrence, period) = self.unique_occurrence_for_goal(goal);
        let automation = task
            .automation_id
            .or_else(|| occurrence.map(|value| value.automation_id))?;
        let (goal_index, goal_count) = occurrence
            .map(|value| {
                let goals = occurrence_goals(value);
                let index = goals.iter().position(|id| *id == goal).unwrap_or(0);
                (index, goals.len().max(1))
            })
            .unwrap_or((0, 1));
        Some(AutomationOwnership {
            automation,
            occurrence: occurrence.map(|value| value.id),
            goal,
            goal_index,
            goal_count,
            period,
        })
    }

    fn unique_occurrence_for_goal(
        &self,
        goal: u64,
    ) -> (Option<&AutomationOccurrence>, PeriodState) {
        let matches: Vec<_> = self
            .all_occurrences()
            .iter()
            .filter(|occurrence| occurrence_contains_goal(occurrence, goal))
            .collect();
        let active: Vec<_> = matches
            .iter()
            .copied()
            .filter(|occurrence| occurrence.status.is_active())
            .collect();
        if active.len() == 1 {
            return (Some(active[0]), period_state(active[0].status));
        }
        if active.len() > 1 {
            return (None, PeriodState::Ambiguous);
        }
        if matches.len() == 1 {
            return (Some(matches[0]), period_state(matches[0].status));
        }
        if matches.len() > 1 {
            return (None, PeriodState::Ambiguous);
        }
        (None, PeriodState::Missing)
    }
}

fn occurrence_goals(occurrence: &AutomationOccurrence) -> Vec<u64> {
    if occurrence.research_goal_ids.is_empty() {
        occurrence.research_goal_id.into_iter().collect()
    } else {
        occurrence.research_goal_ids.clone()
    }
}

fn occurrence_contains_goal(occurrence: &AutomationOccurrence, goal: u64) -> bool {
    occurrence.research_goal_id == Some(goal) || occurrence.research_goal_ids.contains(&goal)
}

fn period_state(status: OccurrenceStatus) -> PeriodState {
    match status {
        OccurrenceStatus::Queued => PeriodState::Queued,
        OccurrenceStatus::Running => PeriodState::Running,
        OccurrenceStatus::WaitingRetry => PeriodState::WaitingRetry,
        OccurrenceStatus::Verifying => PeriodState::Verifying,
        OccurrenceStatus::Succeeded
        | OccurrenceStatus::Failed
        | OccurrenceStatus::Interrupted
        | OccurrenceStatus::Cancelled
        | OccurrenceStatus::Skipped => PeriodState::Ended,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationPolicy, AutomationTemplate, TimeWindow, TriggerSource,
    };

    const PLAN: &str = r#"{"tasks":[{"title":"Collect one","instructions":"Collect one.","kind":"collect","depends_on":[]},{"title":"Collect two","instructions":"Collect two.","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze.","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report.","kind":"advise","depends_on":[2]}]}"#;

    fn fixture() -> (Store, u64, Vec<u64>) {
        let mut store = Store::empty();
        store.default_model = "fixture/model".into();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into();
        let topic = store.create_topic(1, "Automation topic").unwrap();
        let goal = store
            .create_research_goal_in_topic(
                "Automation goal".into(),
                "Check the fixture.".into(),
                1,
                Some(topic),
            )
            .unwrap();
        let ids = store.apply_research_plan(goal, PLAN).unwrap();
        let task = store.task_mut(goal).unwrap();
        task.automation_id = Some(7);
        for id in &ids {
            store.task_mut(*id).unwrap().automation_id = Some(7);
        }
        (store, goal, ids)
    }

    fn occurrence(
        id: u64,
        status: OccurrenceStatus,
        goals: Vec<u64>,
    ) -> AutomationOccurrence {
        AutomationOccurrence {
            id,
            automation_id: 7,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: format!("scheduled:{id}"),
            scheduled_for: Some(1_000),
            requested_at: None,
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: 1_000,
            data_window: TimeWindow::unresolved("fixture"),
            snapshot: crate::automation::AutomationOccurrenceSnapshot {
                objective: "Check the fixture.".into(),
                expected_result: "A result.".into(),
                shape: None,
                template: AutomationTemplate {
                    goals: vec![],
                    window_rule: Default::default(),
                },
                models: Default::default(),
                agents: Default::default(),
                materials: vec![],
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "fixture".into(),
            },
            status,
            reason: None,
            research_goal_id: goals.first().copied(),
            research_goal_ids: goals,
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: 1_000,
            deadline_at: None,
            retry_requested_at: None,
        }
    }

    fn complete(store: &mut Store, id: u64) {
        let mut run = store.prepare_run(id).unwrap();
        run.status = "Succeeded".into();
        run.output = format!("Result for {id}");
        store.push_run(run);
        store.set_stage(id, Stage::Done).unwrap();
    }

    fn running(store: &mut Store, id: u64, occurrence: u64) {
        let mut run = store.prepare_run(id).unwrap();
        run.status = "Running".into();
        run.automation_occurrence_id = Some(occurrence);
        store.push_run(run);
    }

    #[test]
    fn phase_barrier_waits_for_every_collection_step_before_analysis() {
        let (mut store, goal, ids) = fixture();
        complete(&mut store, ids[0]);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        // While the second collection step is still idle, it is the next candidate.
        assert_eq!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .unwrap(),
            Some(ids[1])
        );
        // Once it is running, preparation must reject a duplicate attempt.
        running(&mut store, ids[1], 1);
        assert!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .is_err()
        );

        let analysis = store.execution_eligibility(ids[2]);
        assert_eq!(analysis.decision(), EligibilityDecision::Waiting);
        assert_eq!(
            analysis.label_override().as_deref(),
            Some("Waiting for collection")
        );
        let status = store.task_status(ids[2], None);
        assert_eq!(status.execution, ExecutionStatus::Waiting);
        assert_eq!(status.label(), "Waiting for collection");
        assert!(status.reason.contains("Waiting for collection"));
        assert!(!status.reason.contains("Ready"));
        assert!(!status.reason.contains("add its topic to the queue"));
    }

    #[test]
    fn completed_collection_phase_exposes_the_next_analysis_step() {
        let (mut store, goal, ids) = fixture();
        complete(&mut store, ids[0]);
        complete(&mut store, ids[1]);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        let selected = store
            .next_occurrence_step(store.occurrence(1).unwrap())
            .unwrap();
        assert_eq!(selected, Some(ids[2]));
        let analysis = store.execution_eligibility(ids[2]);
        assert!(analysis.phase_barrier.is_none());
        assert!(analysis.is_executable_for_preparation());
        assert_eq!(
            store.task_status(ids[2], None).execution,
            ExecutionStatus::Ready
        );
    }

    #[test]
    fn ordered_goals_wait_for_the_previous_goal_even_without_dependencies() {
        let (mut store, first_goal, first) = fixture();
        let second_goal = store
            .create_research_goal_in_topic(
                "Second goal".into(),
                "Second fixture.".into(),
                1,
                None,
            )
            .unwrap();
        let second = store
            .apply_research_plan(
                second_goal,
                r#"{"tasks":[{"title":"Collect second","instructions":"Collect.","kind":"collect","depends_on":[]},{"title":"Analyze second","instructions":"Analyze.","kind":"analyze","depends_on":[0]},{"title":"Report second","instructions":"Report.","kind":"advise","depends_on":[1]}]}"#,
            )
            .unwrap();
        store.task_mut(second_goal).unwrap().automation_id = Some(7);
        for id in &second {
            store.task_mut(*id).unwrap().automation_id = Some(7);
        }
        running(&mut store, first[0], 1);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![first_goal, second_goal]));

        assert!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .is_err(),
            "an active attempt must not prepare a duplicate"
        );
        // Complete the same attempt (current result) and re-check the ordered
        // goal barrier. The next step stays inside the first goal.
        {
            let mut run = store.last_run_mut().unwrap();
            run.status = "Succeeded".into();
            run.output = "Collected first goal evidence".into();
        }
        store.set_stage(first[0], Stage::Done).unwrap();

        let selected = store
            .next_occurrence_step(store.occurrence(1).unwrap())
            .unwrap();
        assert_eq!(selected, Some(first[1]));
        let later = store.execution_eligibility(second[0]);
        assert_eq!(later.decision(), EligibilityDecision::Waiting);
        assert_eq!(
            later.label_override().as_deref(),
            Some("Waiting for previous goal")
        );
        assert!(later.reason(&store, second[0]).contains("previous goal"));
    }

    #[test]
    fn only_the_failed_step_is_blocked_and_terminal_period_descendants_wait() {
        let (mut store, goal, ids) = fixture();
        complete(&mut store, ids[0]);
        let mut failed = store.prepare_run(ids[1]).unwrap();
        failed.status = "Failed".into();
        failed.auto_retry_attempt = crate::retries::MAX_AUTO_RETRIES;
        store.push_run(failed);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Failed, vec![goal]));

        assert_eq!(
            store.execution_eligibility(ids[1]).decision(),
            EligibilityDecision::Blocked
        );
        let downstream = store.execution_eligibility(ids[2]);
        assert_eq!(downstream.decision(), EligibilityDecision::Ended);
        assert_eq!(downstream.execution_status(), ExecutionStatus::Waiting);
        let reason = downstream.reason(&store, ids[2]);
        assert!(reason.contains("Waiting for collection"));
        assert!(reason.contains("period ended"));
        assert!(!reason.contains("This step is ready"));
        assert_eq!(
            store.task_status(ids[1], None).execution,
            ExecutionStatus::Blocked
        );
        assert_eq!(
            store.task_status(ids[2], None).execution,
            ExecutionStatus::Waiting
        );
    }

    #[test]
    fn newer_active_retry_wins_over_an_older_failed_attempt() {
        let (mut store, goal, ids) = fixture();
        let mut failed = store.prepare_run(ids[1]).unwrap();
        failed.status = "Failed".into();
        store.push_run(failed);
        let mut retry = store.prepare_run(ids[1]).unwrap();
        retry.status = "Queued".into();
        retry.auto_retry_attempt = 1;
        retry.automation_occurrence_id = Some(1);
        store.push_run(retry);
        store.push_occurrence(occurrence(1, OccurrenceStatus::WaitingRetry, vec![goal]));

        let eligibility = store.execution_eligibility(ids[1]);
        assert_eq!(eligibility.decision(), EligibilityDecision::Retrying);
        assert!(!eligibility.is_executable_for_preparation());
        assert!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .is_err()
        );
    }

    #[test]
    fn queued_period_is_preparable_but_displays_pool_wait() {
        let (mut store, goal, ids) = fixture();
        store.push_occurrence(occurrence(1, OccurrenceStatus::Queued, vec![goal]));

        let eligibility = store.execution_eligibility(ids[0]);
        assert_eq!(eligibility.decision(), EligibilityDecision::Queued);
        assert_eq!(
            eligibility.label_override().as_deref(),
            Some("Queued for automation slot")
        );
        assert!(eligibility.is_executable_for_preparation());
        assert_eq!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .unwrap(),
            Some(ids[0])
        );
        let status = store.task_status(ids[0], None);
        assert_eq!(status.label(), "Queued for automation slot");
        assert!(status.reason.contains("automation slot"));
        assert!(!status.reason.contains("Research queue admission is involved"));
    }

    #[test]
    fn running_step_cannot_prepare_a_duplicate_attempt() {
        let (mut store, goal, ids) = fixture();
        running(&mut store, ids[0], 1);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        let eligibility = store.execution_eligibility(ids[0]);
        assert_eq!(eligibility.decision(), EligibilityDecision::Running);
        assert!(!eligibility.is_executable_for_preparation());
        assert!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .is_err()
        );
        assert_eq!(
            store.task_status(ids[0], None).execution,
            ExecutionStatus::Running
        );
    }

    #[test]
    fn between_step_state_is_owned_by_the_period_not_the_ordinary_queue() {
        let (mut store, goal, ids) = fixture();
        store.task_mut(ids[0]).unwrap().stage = Stage::InProgress;
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        let status = store.task_status(ids[0], None);
        assert_ne!(status.execution, ExecutionStatus::Paused);
        assert_eq!(status.execution, ExecutionStatus::Ready);
        assert_eq!(status.phase, Stage::InProgress);
        assert!(status.reason.contains("Ready in this running period"));
        assert!(!status.reason.to_lowercase().contains("topic queue"));
        assert_eq!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .unwrap(),
            Some(ids[0])
        );
    }

    #[test]
    fn paused_definition_does_not_pause_a_running_period() {
        let (mut store, goal, ids) = fixture();
        store.push_automation(crate::automation::AutomationDefinition {
            id: 7,
            version: 1,
            project_id: 1,
            shape: crate::automation::AutomationShape::Research,
            name: "Paused definition".into(),
            objective: "Check.".into(),
            expected_result: "Result.".into(),
            schedule: crate::automation_schedule::Schedule {
                spec: crate::automation_schedule::ScheduleSpec::Interval { minutes: 5 },
                timezone: "UTC".into(),
            },
            status: crate::automation::DefinitionStatus::Paused,
            template: None,
            plan_binding: None,
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
        });
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        let status = store.task_status(ids[0], None);
        assert_ne!(status.execution, ExecutionStatus::Paused);
        assert_eq!(status.execution, ExecutionStatus::Ready);
        assert_eq!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .unwrap(),
            Some(ids[0])
        );
        assert_eq!(
            store.automation(7).unwrap().status,
            crate::automation::DefinitionStatus::Paused
        );
    }

    #[test]
    fn explicit_retry_authority_keeps_history_but_allows_the_failed_step() {
        let (mut store, goal, ids) = fixture();
        complete(&mut store, ids[0]);
        let mut failed = store.prepare_run(ids[1]).unwrap();
        failed.status = "Failed".into();
        store.push_run(failed);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Queued, vec![goal]));
        store.occurrence_mut(1).unwrap().retry_requested_at = Some(2_000);

        let eligibility = store.execution_eligibility(ids[1]);
        assert!(eligibility.retry_authority);
        assert_eq!(eligibility.decision(), EligibilityDecision::Queued);
        assert!(eligibility.is_executable_for_preparation());
        assert_eq!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .unwrap(),
            Some(ids[1])
        );
        assert_eq!(
            store.runs().last().unwrap().status,
            "Failed",
            "explicit retry must not rewrite the historical attempt"
        );
        assert!(store.task_status(ids[1], None).reason.contains("explicitly reopened"));
    }

    #[test]
    fn done_or_running_steps_do_not_take_a_stale_phase_barrier_label() {
        let (mut store, goal, ids) = fixture();
        complete(&mut store, ids[0]);
        complete(&mut store, ids[1]);
        complete(&mut store, ids[2]);
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));
        // A later failed attempt on the second collection step invalidates that
        // phase result without changing the completed analysis result or approval.
        let mut failed = store.prepare_run(ids[1]).unwrap();
        failed.status = "Failed".into();
        store.push_run(failed);

        let analysis = store.execution_eligibility(ids[2]);
        assert_eq!(analysis.outcome, StepOutcome::Done);
        assert!(analysis.phase_barrier.is_some());
        assert_eq!(analysis.decision(), EligibilityDecision::Done);
        assert!(
            analysis.label_override().is_none(),
            "a completed later phase must not render as Waiting"
        );

        let mut running = store.prepare_run(ids[2]).unwrap();
        running.status = "Running".into();
        store.push_run(running);
        let analysis = store.execution_eligibility(ids[2]);
        assert_eq!(analysis.decision(), EligibilityDecision::Running);
        assert!(
            analysis.label_override().is_none(),
            "an active attempt must not render as Waiting because of a stale barrier"
        );
        assert_eq!(
            store.task_status(ids[2], None).execution,
            ExecutionStatus::Running
        );
    }

    #[test]
    fn approval_and_orphaned_automation_entities_do_not_become_ready() {
        let (mut store, goal, ids) = fixture();
        let research = store.task_mut(goal).unwrap().research.as_mut().unwrap();
        research.requires_approval = true;
        store.task_mut(goal).unwrap().approved_plan = None;
        store.push_occurrence(occurrence(1, OccurrenceStatus::Running, vec![goal]));

        let approval = store.execution_eligibility(ids[0]);
        assert_eq!(approval.decision(), EligibilityDecision::Approval);
        assert!(!approval.is_executable_for_preparation());
        assert!(
            store
                .next_occurrence_step(store.occurrence(1).unwrap())
                .is_err()
        );

        store.clear_occurrences();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = false;
        let orphan = store.execution_eligibility(ids[0]);
        assert!(orphan.owner.is_automation());
        assert_eq!(orphan.decision(), EligibilityDecision::Inactive);
        assert!(!orphan.is_executable_for_preparation());
        let status = store.task_status(ids[0], None);
        assert_eq!(status.execution, ExecutionStatus::Waiting);
        assert!(status.reason.contains("not active"));
        assert!(!status.reason.contains("Ready"));
    }

    #[test]
    fn ordinary_research_keeps_dependency_only_status_and_queue_copy() {
        let (store, goal, ids) = fixture();
        let mut store = store;
        store.task_mut(goal).unwrap().automation_id = None;
        for id in &ids {
            store.task_mut(*id).unwrap().automation_id = None;
        }
        complete(&mut store, ids[0]);
        running(&mut store, ids[1], 0);

        let analysis = store.execution_eligibility(ids[2]);
        assert!(!analysis.owner.is_automation());
        assert!(analysis.phase_barrier.is_none());
        assert_eq!(analysis.decision(), EligibilityDecision::Ready);
        assert!(analysis.label_override().is_none());
        let status = store.task_status(ids[2], None);
        assert_eq!(status.execution, ExecutionStatus::Ready);
        assert!(status.reason.contains("Run the goal or add its topic to the queue"));
    }
}
