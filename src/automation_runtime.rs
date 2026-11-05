//! Due scanning, atomic occurrence accounting and recovery coordination.
//!
//! Storage transactions live in the app layer: callers clone the store, apply a
//! `TickPlan`, save it through the single Storage writer, and only then enqueue work.
use crate::{
    automation::{
        manual_trigger_key, scheduled_trigger_key, AutomationDefinition, AutomationOccurrence,
        AutomationOccurrenceSnapshot, DefinitionStatus, OccurrenceReason,
        OccurrenceStatus, SkipCode, TriggerSource,
    },
    automation_schedule::{decide, CatchUp, DueDecision},
    domain::Store,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedRange {
    pub count: u64,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TickAction {
    /// Create and queue the newest due occurrence.
    Run {
        decision: DueDecision,
        supersede: Option<u64>,
    },
    /// Record one skipped occurrence and advance the cursor.
    Skip {
        trigger: i64,
        next_cursor: i64,
        skipped: SkippedRange,
        code: SkipCode,
        detail: String,
    },
    /// Stop automatic dispatch until the user resolves a problem.
    Attention { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickPlan {
    pub automation_id: u64,
    pub definition_version: u32,
    pub action: TickAction,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickCommit {
    pub queued: Vec<u64>,
    pub skipped: Vec<u64>,
    pub superseded: Vec<u64>,
    pub attention: Vec<(u64, String)>,
}

impl Store {
    /// Read-only due scan. No queue, process or storage side effects.
    pub fn plan_automation_ticks(&self, now: i64) -> Vec<TickPlan> {
        let mut plans = vec![];
        for definition in &self.automations {
            if !definition.status.is_active() {
                continue;
            }
            if let Some(reason) = self.automation_blocker(definition) {
                plans.push(TickPlan {
                    automation_id: definition.id,
                    definition_version: definition.version,
                    action: TickAction::Attention { reason },
                });
                continue;
            }
            let Some(cursor) = definition.cursor else { continue };
            if cursor > now {
                continue;
            }
            let anchor = definition.anchor.unwrap_or(cursor);
            let decision = match decide(
                &definition.schedule,
                anchor,
                cursor,
                now,
                definition.policy.allowed_delay_seconds,
                definition.policy.catch_up,
            ) {
                Ok(Some(decision)) => decision,
                Ok(None) => continue,
                Err(reason) => {
                    plans.push(TickPlan {
                        automation_id: definition.id,
                        definition_version: definition.version,
                        action: TickAction::Attention { reason },
                    });
                    continue;
                }
            };
            let pending = self
                .automation_occurrences
                .iter()
                .find(|o| {
                    o.automation_id == definition.id && o.status == OccurrenceStatus::Queued
                });
            // An already queued manual request is never replaced by a schedule.
            if let Some(pending) = pending
                && pending.source == TriggerSource::Manual
                && decision.trigger != pending.scheduled_for.unwrap_or(i64::MIN)
            {
                plans.push(TickPlan {
                    automation_id: definition.id,
                    definition_version: definition.version,
                    action: TickAction::Skip {
                        trigger: decision.trigger,
                        next_cursor: decision.next_cursor,
                        skipped: SkippedRange {
                            count: 1 + decision.skipped_count,
                            from: Some(cursor),
                            to: Some(decision.trigger),
                        },
                        code: SkipCode::ManualPending,
                        detail:
                            "A queued manual run keeps its place; the scheduled occurrence was skipped."
                                .into(),
                    },
                });
                continue;
            }
            // Supersede only an older unstarted scheduled occurrence.
            let supersede = pending
                .filter(|o| o.source == TriggerSource::Scheduled)
                .filter(|o| o.scheduled_for.is_some_and(|value| value < decision.trigger))
                .map(|o| o.id);
            if decision.expired {
                plans.push(TickPlan {
                    automation_id: definition.id,
                    definition_version: definition.version,
                    action: TickAction::Skip {
                        trigger: decision.trigger,
                        next_cursor: decision.next_cursor,
                        skipped: SkippedRange {
                            count: 1 + decision.skipped_count,
                            from: Some(cursor),
                            to: Some(decision.trigger),
                        },
                        code: SkipCode::Expired,
                        detail: format!(
                            "The newest trigger was more than {} minutes old when it became runnable.",
                            definition.policy.allowed_delay_seconds / 60
                        ),
                    },
                });
                continue;
            }
            if definition.policy.catch_up == CatchUp::SkipAll && decision.skipped_count > 0 {
                plans.push(TickPlan {
                    automation_id: definition.id,
                    definition_version: definition.version,
                    action: TickAction::Skip {
                        trigger: decision.trigger,
                        next_cursor: decision.next_cursor,
                        skipped: SkippedRange {
                            count: decision.skipped_count,
                            from: decision.skipped_from,
                            to: decision.skipped_to,
                        },
                        code: SkipCode::CatchUpSkipped,
                        detail: "Catch-up is set to skip missed periods.".into(),
                    },
                });
                continue;
            }
            plans.push(TickPlan {
                automation_id: definition.id,
                definition_version: definition.version,
                action: TickAction::Run {
                    decision,
                    supersede,
                },
            });
        }
        plans
    }

    /// Blockers that must stop unattended dispatch rather than retrying forever.
    pub fn automation_blocker(&self, definition: &AutomationDefinition) -> Option<String> {
        if !self.projects.iter().any(|p| p.id == definition.project_id) {
            return Some(
                "The automation's project was removed or is unavailable. Choose another project."
                    .into(),
            );
        }
        if !definition.approval_current() {
            return Some(
                "This version is not approved, or its plan, schedule, models or materials changed. Review and enable it again."
                    .into(),
            );
        }
        if definition.consecutive_failures >= definition.policy.max_consecutive_failures {
            return Some(format!(
                "Paused after {} consecutive failed runs. Resolve the cause, then resume.",
                definition.consecutive_failures
            ));
        }
        None
    }

    /// Apply one due plan inside the caller's transaction. The definition version,
    /// cursor and occurrence are updated together.
    pub fn commit_automation_tick(
        &mut self,
        plan: &TickPlan,
        now: i64,
    ) -> Result<TickCommit, String> {
        let mut commit = TickCommit::default();
        let definition = self
            .automation(plan.automation_id)
            .ok_or("Automation not found.")?
            .clone();
        if definition.version != plan.definition_version {
            return Err("The automation changed while its schedule was being evaluated.".into());
        }
        if definition.status != DefinitionStatus::Enabled {
            return Err("The automation is no longer enabled.".into());
        }
        match &plan.action {
            TickAction::Attention { reason } => {
                self.mark_automation_attention(definition.id, reason.clone());
                commit
                    .attention
                    .push((definition.id, reason.clone()));
            }
            TickAction::Skip {
                trigger,
                next_cursor,
                skipped,
                code,
                detail,
            } => {
                let mut occurrence = build_occurrence(&definition, TriggerSource::Scheduled, *trigger, now)?;
                occurrence.status = OccurrenceStatus::Skipped;
                occurrence.reason = Some(OccurrenceReason::new(*code, detail.clone()));
                occurrence.skipped_count = skipped.count;
                occurrence.skipped_from = skipped.from;
                occurrence.skipped_to = skipped.to;
                let id = self.record_occurrence_without_research(occurrence)?;
                self.automation_mut(definition.id).unwrap().cursor = Some(*next_cursor);
                self.automation_mut(definition.id).unwrap().updated_at = now;
                commit.skipped.push(id);
            }
            TickAction::Run {
                decision,
                supersede,
            } => {
                if let Some(id) = supersede {
                    if let Some(pending) = self.occurrence_mut(*id) {
                        if pending.status == OccurrenceStatus::Queued {
                            pending.transition(
                                OccurrenceStatus::Skipped,
                                Some(OccurrenceReason::new(
                                    SkipCode::SupersededByNewer,
                                    "A newer scheduled occurrence replaced this unstarted request.",
                                )),
                            )?;
                            commit.superseded.push(*id);
                        }
                    }
                }
                let mut occurrence =
                    build_occurrence(&definition, TriggerSource::Scheduled, decision.trigger, now)?;
                occurrence.skipped_count = decision.skipped_count;
                occurrence.skipped_from = decision.skipped_from;
                occurrence.skipped_to = decision.skipped_to;
                let occurrence = self.materialize_occurrence(occurrence)?;
                let definition = self.automation_mut(definition.id).unwrap();
                definition.cursor = Some(decision.next_cursor);
                definition.updated_at = now;
                commit.queued.push(occurrence.id);
            }
        }
        Ok(commit)
    }

    /// Manual runs use a separate request key and never move the schedule cursor.
    pub fn request_manual_occurrence(
        &mut self,
        automation: u64,
        request_id: &str,
        now: i64,
    ) -> Result<AutomationOccurrence, String> {
        let key = manual_trigger_key(automation, request_id)?;
        if let Some(existing) = self.automation_occurrence_by_key(&key) {
            return Ok(existing.clone());
        }
        let definition = self
            .automation(automation)
            .ok_or("Automation not found.")?
            .clone();
        if definition.status == DefinitionStatus::Archived {
            return Err("Restore the archived automation before running it.".into());
        }
        if !definition.approval_current() {
            return Err("Review and enable this automation version before running it.".into());
        }
        if let Some(reason) = self.automation_blocker(&definition) {
            return Err(reason);
        }
        if self
            .automation_occurrences
            .iter()
            .any(|o| o.automation_id == automation && o.status == OccurrenceStatus::Queued)
        {
            return Err(
                "This automation already has an unstarted request. Wait for it to start or stop it first."
                    .into(),
            );
        }
        let mut occurrence = build_occurrence(&definition, TriggerSource::Manual, now, now)?;
        occurrence.scheduled_for = None;
        occurrence.requested_at = Some(now);
        occurrence.reference_time = now;
        occurrence.trigger_key = key;
        occurrence.data_window = crate::automation_execution::window_for(
            &occurrence.snapshot.template,
            now,
            &occurrence.timezone,
        )?;
        self.materialize_occurrence(occurrence)
    }

    pub fn record_occurrence_without_research(
        &mut self,
        mut occurrence: AutomationOccurrence,
    ) -> Result<u64, String> {
        if occurrence.id == 0 {
            occurrence.id = self.next_occurrence_id()?;
        }
        if self
            .automation_occurrence_by_key(&occurrence.trigger_key)
            .is_some()
        {
            return Err("This trigger already has an occurrence.".into());
        }
        let id = occurrence.id;
        self.automation_occurrences.push(occurrence);
        Ok(id)
    }

    /// Cancel every unstarted occurrence, e.g. on pause/edit/archive. Returns the
    /// occurrence ids so the app can remove their queue entries.
    pub fn cancel_unstarted_occurrences(
        &mut self,
        automation: u64,
        code: SkipCode,
        detail: &str,
    ) -> Vec<u64> {
        let mut cancelled = vec![];
        for occurrence in self
            .automation_occurrences
            .iter_mut()
            .filter(|o| o.automation_id == automation && o.status == OccurrenceStatus::Queued)
        {
            if occurrence
                .transition(
                    OccurrenceStatus::Skipped,
                    Some(OccurrenceReason::new(code, detail)),
                )
                .is_ok()
            {
                cancelled.push(occurrence.id);
            }
        }
        cancelled
    }

    /// Stop one occurrence and its retries. The app cancels the active process first.
    pub fn stop_occurrence(
        &mut self,
        id: u64,
        now: i64,
    ) -> Result<Vec<u64>, String> {
        let occurrence = self.occurrence(id).ok_or("Automation run not found.")?;
        let automation = occurrence.automation_id;
        let mut runs = vec![];
        if occurrence.status.is_active() {
            let occurrence = self.occurrence_mut(id).unwrap();
            if occurrence.status == OccurrenceStatus::Queued {
                occurrence.transition(
                    OccurrenceStatus::Cancelled,
                    Some(OccurrenceReason::new(
                        SkipCode::Paused,
                        "Stopped before execution started.",
                    )),
                )?;
            } else {
                occurrence.transition(
                    OccurrenceStatus::Cancelled,
                    Some(OccurrenceReason::new(
                        SkipCode::Paused,
                        "Stopped by the user.",
                    )),
                )?;
            }
            runs = occurrence.run_ids.clone();
        }
        if let Some(definition) = self.automation_mut(automation) {
            definition.updated_at = now;
        }
        Ok(runs)
    }

    /// Final success requires every goal, every step and the final report, never
    /// exit code alone.
    pub fn complete_occurrence(
        &mut self,
        id: u64,
        final_run_id: u64,
        now: i64,
    ) -> Result<bool, String> {
        let occurrence = self.occurrence(id).ok_or("Automation run not found.")?.clone();
        let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else {
            occurrence.research_goal_ids.clone()
        };
        let Some(first_goal) = goals.first().copied() else {
            return Err("The occurrence has no research goal.".into());
        };
        let _ = first_goal;
        if goals.iter().any(|goal| !self.goal_tasks_complete(*goal)) {
            return Ok(false);
        }
        let mut latest_report: Option<(u64, String, String)> = None;
        for goal in &goals {
            let report = self
                .research_children(*goal)
                .into_iter()
                .filter(|task| {
                    self.task(*task)
                        .and_then(|t| t.research.as_ref())
                        .is_some_and(|r| r.kind == crate::research::ResearchKind::Advise)
                })
                .filter_map(|task| self.research_result(task))
                .max_by_key(|run| run.id)
                .map(|run| (run.id, run.status.clone(), run.output.clone()));
            // Every approved goal must produce its own final report.
            let Some(report) = report else { return Ok(false) };
            if report.1 != "Succeeded" || report.2.trim().is_empty() {
                return Ok(false);
            }
            if latest_report
                .as_ref()
                .is_none_or(|current| report.0 > current.0)
            {
                latest_report = Some(report);
            }
        }
        let report_id = latest_report
            .ok_or("The period has no final report.")?
            .0;
        let occurrence = self.occurrence_mut(id).unwrap();
        occurrence.final_run_id = Some(report_id.max(final_run_id));
        occurrence.unread = true;
        occurrence.transition(OccurrenceStatus::Succeeded, None)?;
        let automation = occurrence.automation_id;
        self.note_automation_success(automation);
        let definition = self.automation_mut(automation).unwrap();
        definition.updated_at = now;
        Ok(true)
    }

    pub fn fail_occurrence(
        &mut self,
        id: u64,
        reason: impl Into<String>,
        now: i64,
    ) -> Result<Option<String>, String> {
        let occurrence = self.occurrence(id).ok_or("Automation run not found.")?.clone();
        let automation = occurrence.automation_id;
        if occurrence.status.is_terminal() {
            return Ok(None);
        }
        let reason = reason.into();
        self.occurrence_mut(id).unwrap().transition(
            OccurrenceStatus::Failed,
            Some(OccurrenceReason::new(SkipCode::ExecutionFailed, reason.clone())),
        )?;
        Ok(self.note_automation_failure(automation, now))
    }

    pub fn wait_for_retry(&mut self, id: u64, detail: impl Into<String>) -> Result<(), String> {
        let occurrence = self.occurrence_mut(id).ok_or("Automation run not found.")?;
        occurrence.transition(
            OccurrenceStatus::WaitingRetry,
            Some(OccurrenceReason::new(SkipCode::RetryScheduled, detail)),
        )
    }

    pub fn mark_occurrence_read(&mut self, id: u64) {
        if let Some(occurrence) = self.occurrence_mut(id) {
            occurrence.unread = false;
        }
    }

    pub fn next_automation_due(&self) -> Option<i64> {
        self.automations
            .iter()
            .filter(|a| a.status == DefinitionStatus::Enabled)
            .filter_map(|a| a.cursor)
            .min()
    }
}

fn build_occurrence(
    definition: &AutomationDefinition,
    source: TriggerSource,
    reference: i64,
    now: i64,
) -> Result<AutomationOccurrence, String> {
    let approval = definition
        .approval
        .clone()
        .ok_or("The automation has no approved snapshot.")?;
    if !definition.approval_current() {
        return Err("The automation approval is stale. Review and enable it again.".into());
    }
    let template = definition
        .template
        .clone()
        .ok_or("The automation has no approved plan.")?;
    let key = match source {
        TriggerSource::Scheduled => scheduled_trigger_key(definition.id, reference),
        TriggerSource::Manual => "manual:pending".into(),
    };
    let data_window = crate::automation_execution::window_for(
        &template,
        reference,
        &definition.schedule.timezone,
    )?;
    Ok(AutomationOccurrence {
        id: 0, // assigned by materialize/record helpers below
        automation_id: definition.id,
        definition_version: definition.version,
        source,
        trigger_key: key,
        scheduled_for: Some(reference),
        requested_at: None,
        actual_started_at: None,
        timezone: definition.schedule.timezone.clone(),
        reference_time: reference,
        data_window,
        snapshot: AutomationOccurrenceSnapshot {
            objective: definition.objective.clone(),
            expected_result: definition.expected_result.clone(),
            template,
            models: approval.models.clone(),
            agents: approval.agents.clone(),
            materials: approval.materials.clone(),
            permissions: approval.permissions.clone(),
            policy: definition.policy.clone(),
            approval_version: approval.version,
            approval_signature: crate::automation::hash_json(
                &serde_json::to_value(&approval).unwrap_or_default(),
            ),
        },
        status: OccurrenceStatus::Queued,
        reason: None,
        research_goal_id: None,
        research_goal_ids: vec![],
        research_topic_id: None,
        run_ids: vec![],
        final_run_id: None,
        unread: false,
        skipped_count: 0,
        skipped_from: None,
        skipped_to: None,
        created_at: now,
        deadline_at: None,
    })
}

/// Recovery classification supplied by an explicit session check.
#[allow(dead_code)] // `Running` is exercised by recovery tests and the verified-running path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Completed,
    Stopped,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RecoveryCommit {
    pub running: Vec<u64>,
    pub completed: Vec<u64>,
    pub retry: Vec<u64>,
    pub blocked: Vec<u64>,
    pub attention: Vec<(u64, String)>,
}

/// Reconcile active occurrences after a restart. No external process is started
/// here; the app imports results and enqueues only confirmed safe retries.
pub fn reconcile_occurrence(
    store: &mut Store,
    id: u64,
    state: SessionState,
    now: i64,
) -> Result<RecoveryCommit, String> {
    let mut commit = RecoveryCommit::default();
    let occurrence = store.occurrence(id).ok_or("Automation run not found.")?.clone();
    if !occurrence.status.is_active() {
        return Ok(commit);
    }
    let automation = occurrence.automation_id;
    let approval_current = store
        .automation(automation)
        .is_some_and(|definition| definition.approval_current());
    match state {
        SessionState::Running => {
            store.occurrence_mut(id).unwrap().status = OccurrenceStatus::Running;
            commit.running.push(id);
        }
        SessionState::Completed => {
            if occurrence.status == OccurrenceStatus::Queued {
                commit.retry.push(id);
                return Ok(commit);
            }
            // An intermediate step may be complete while later steps remain.
            if !store.complete_occurrence(id, 0, now)? {
                store.occurrence_mut(id).unwrap().transition(
                    OccurrenceStatus::Queued,
                    Some(OccurrenceReason::new(
                        SkipCode::DefinitionChanged,
                        "A completed step will continue with the next step of the same period.",
                    )),
                )?;
                commit.retry.push(id);
            } else {
                commit.completed.push(id);
            }
        }
        SessionState::Stopped => {
            if approval_current {
                let automation = occurrence.automation_id;
                if occurrence.status == OccurrenceStatus::Queued {
                    commit.retry.push(id);
                    return Ok(commit);
                }
                store.occurrence_mut(id).unwrap().transition(
                    OccurrenceStatus::Queued,
                    Some(OccurrenceReason::new(
                        SkipCode::DefinitionChanged,
                        "The previous attempt stopped. It will resume with the same approved period and inputs.",
                    )),
                )?;
                store.automation_mut(automation).unwrap().updated_at = now;
                commit.retry.push(id);
            } else {
                store.occurrence_mut(id).unwrap().status = OccurrenceStatus::Verifying;
                store.mark_automation_attention(
                    automation,
                    "The automation's approval changed while an occurrence was interrupted. Review it before continuing.",
                );
                commit.blocked.push(id);
                commit.attention.push((automation, "Approval changed during recovery".into()));
            }
        }
        SessionState::Unknown => {
            if store
                .occurrence(id)
                .is_some_and(|occurrence| occurrence.status != OccurrenceStatus::Verifying)
            {
                store.occurrence_mut(id).unwrap().transition(
                    OccurrenceStatus::Verifying,
                    Some(OccurrenceReason::new(
                        SkipCode::DefinitionChanged,
                        "The previous session's state cannot be confirmed; no work was replayed.",
                    )),
                )?;
            }
            // A user pause already blocks future triggers. Do not overwrite it with
            // NeedsAttention, or a later verified recovery would lose the pause intent.
            if !store.automation(automation).is_some_and(|definition| {
                matches!(
                    definition.status,
                    DefinitionStatus::Paused
                        | DefinitionStatus::Archived
                        | DefinitionStatus::Draft
                )
            }) {
                store.mark_automation_attention(
                    automation,
                    "The previous session's state cannot be confirmed. Check the result before continuing; no work was replayed.",
                );
            }
            commit.blocked.push(id);
            commit
                .attention
                .push((automation, "Session state unknown".into()));
        }
    }
    Ok(commit)
}

/// Observable fake runner. Never starts a process; used by `--qa-automation`.
#[derive(Clone, Debug, Default)]
pub struct FakeAutomationRunner {
    pub starts: Vec<FakeStart>,
    pub cancels: Vec<u64>,
    /// occurrence id → successful or failed
    pub outcomes: std::collections::HashMap<u64, bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeStart {
    pub occurrence_id: u64,
    pub automation_id: u64,
    pub trigger_key: String,
    pub model: String,
    pub reference_time: i64,
    pub window_resolved: bool,
    pub snapshot_signature: String,
}
impl FakeAutomationRunner {
    pub fn start(&mut self, occurrence: &AutomationOccurrence) -> u64 {
        self.starts.push(FakeStart {
            occurrence_id: occurrence.id,
            automation_id: occurrence.automation_id,
            trigger_key: occurrence.trigger_key.clone(),
            model: occurrence.snapshot.models[1].clone(),
            reference_time: occurrence.reference_time,
            window_resolved: occurrence.data_window.is_resolved(),
            snapshot_signature: occurrence.snapshot.approval_signature.clone(),
        });
        occurrence.id
    }
    pub fn cancel(&mut self, occurrence: u64) {
        self.cancels.push(occurrence);
    }
    pub fn complete(&mut self, store: &mut Store, occurrence: u64, now: i64) -> Result<(), String> {
        let success = self.outcomes.get(&occurrence).copied().unwrap_or(true);
        if store
            .occurrence(occurrence)
            .is_some_and(|o| o.status == OccurrenceStatus::Queued)
        {
            store.occurrence_mut(occurrence).unwrap().mark_running(now)?;
        }
        if self.outcomes.remove(&occurrence).is_none() && success {
            // Default: synthesize complete research results so the state machine advances.
            if let Some(goal) = store.occurrence(occurrence).and_then(|o| o.research_goal_id) {
                for task in store.tasks.iter().filter(|t| t.research.as_ref().is_some_and(|r| r.goal_id == goal && r.kind != crate::research::ResearchKind::Goal)).map(|t| t.id).collect::<Vec<_>>() {
                    if store.research_result(task).is_none() {
                        let mut run = store.prepare_run(task)?;
                        run.automation_occurrence_id = Some(occurrence);
                        run.status = "Succeeded".into();
                        run.output = "Fake automation report".into();
                        run.sources = vec![crate::research::Source {
                            title: "Fake source".into(),
                            url: "https://example.com/fake-automation".into(),
                            published_at: "unknown".into(),
                        }];
                        store.runs.push(run);
                    }
                    let _ = store.set_stage(task, crate::domain::Stage::Done);
                }
                store.sync_goal_progress(goal);
            }
        }
        if success {
            store.complete_occurrence(occurrence, 0, now)?;
        } else {
            store.fail_occurrence(occurrence, "Fake runner failure", now)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationApproval, AutomationPermissions, AutomationPolicy, TimeWindowState,
    };
    use crate::automation_execution::build_template;
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Stage, Store};
    use crate::research::ResearchKind;

    const DAY: i64 = 86_400;

    fn schedule(minutes: u32) -> Schedule {
        Schedule {
            spec: ScheduleSpec::Interval { minutes },
            timezone: "UTC".into(),
        }
    }
    fn store_with(automation: bool) -> (Store, u64) {
        let mut store = Store::empty();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into();
        store.default_model = "test/model".into();
        let id = 1;
        if automation {
            let template = build_template("Track yesterday's topic", "A note").unwrap();
            store.automations.push(AutomationDefinition {
                id,
                version: 1,
                project_id: 1,
                shape: crate::automation::AutomationShape::Research,
                name: "Five-minute check".into(),
                objective: "Track yesterday's topic".into(),
                expected_result: "A note".into(),
                schedule: schedule(5),
                status: DefinitionStatus::Draft,
                template: Some(template),
                approval: None,
                policy: AutomationPolicy::default(),
                cursor: None,
                anchor: None,
                consecutive_failures: 0,
                attention_reason: None,
                created_at: 0,
                updated_at: 0,
            });
            store
                .approve_and_enable(
                    id,
                    vec![],
                    AutomationPermissions::restricted("OpenCode"),
                    0,
                )
                .unwrap();
            // Freeze the approval to interval minutes for predictable ids.
            store.automation_mut(id).unwrap().anchor = Some(0);
            store.automation_mut(id).unwrap().cursor = Some(300);
        }
        (store, id)
    }

    #[test]
    fn due_tick_queues_once_and_advances_cursor() {
        let (mut store, id) = store_with(true);
        let plans = store.plan_automation_ticks(300);
        assert_eq!(plans.len(), 1);
        let commit = store.commit_automation_tick(&plans[0], 300).unwrap();
        assert_eq!(commit.queued.len(), 1);
        assert_eq!(store.automation(id).unwrap().cursor, Some(600));
        let occurrence_id = commit.queued[0];
        {
            let occurrence = store.occurrence(occurrence_id).unwrap();
            assert_eq!(occurrence.status, OccurrenceStatus::Queued);
            assert_eq!(occurrence.reference_time, 300);
            assert!(occurrence.research_goal_id.is_some());
            assert!(occurrence.data_window.is_resolved());
        }
        // Repeating the tick does not start another occurrence.
        assert!(store.plan_automation_ticks(300).is_empty());
        // A second tick after the period creates a pending replacement candidate only
        // when a slot is free; here the first is still unstarted and is superseded.
        let plans = store.plan_automation_ticks(600);
        assert_eq!(plans.len(), 1);
        let commit = store.commit_automation_tick(&plans[0], 600).unwrap();
        assert_eq!(commit.superseded.len(), 1);
        assert_eq!(store.occurrence(occurrence_id).unwrap().status, OccurrenceStatus::Skipped);
        assert_eq!(store.occurrence(occurrence_id).unwrap().reason.as_ref().unwrap().code, SkipCode::SupersededByNewer);
        let live: Vec<_> = store.automation_occurrences.iter().filter(|o| o.status == OccurrenceStatus::Queued).collect();
        assert_eq!(live.len(), 1);
    }

    #[test]
    fn two_automations_due_at_the_same_instant_do_not_collide() {
        let (mut store, first) = store_with(true);
        let mut second = store.automation(first).unwrap().clone();
        second.id = 2;
        second.name = "Second five-minute check".into();
        store.automations.push(second);
        let plans = store.plan_automation_ticks(300);
        assert_eq!(plans.len(), 2);
        let mut queued = vec![];
        for plan in &plans {
            let commit = store.commit_automation_tick(plan, 300).unwrap();
            queued.extend(commit.queued);
        }
        assert_eq!(queued.len(), 2);
        let keys: std::collections::HashSet<_> = store
            .automation_occurrences
            .iter()
            .map(|occurrence| occurrence.trigger_key.clone())
            .collect();
        assert_eq!(keys.len(), 2);
        assert_eq!(store.automation_occurrences.len(), 2);
    }

    #[test]
    fn missed_periods_are_summarized_and_expiry_skips_without_entities_per_minute() {
        let (mut store, id) = store_with(true);
        store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 3600;
        // 10 missed minutes; only the latest is represented.
        let plans = store.plan_automation_ticks(600 + 300);
        assert_eq!(plans.len(), 1);
        let commit = store.commit_automation_tick(&plans[0], 600 + 300).unwrap();
        let occurrence = store.occurrence(commit.queued[0]).unwrap();
        assert_eq!(occurrence.skipped_count, 2);
        assert_eq!(occurrence.reference_time, 900);
        assert_eq!(store.automation_occurrences.len(), 1);
        // A long gap is one skipped occurrence, not thousands.
        store.automation_mut(id).unwrap().cursor = Some(1200);
        store.automation_mut(id).unwrap().policy.allowed_delay_seconds = 60;
        let late = 1200 + 30 * DAY + 120;
        let plans = store.plan_automation_ticks(late);
        let commit = store.commit_automation_tick(&plans[0], late).unwrap();
        assert!(commit.queued.is_empty());
        let skipped = store.occurrence(commit.skipped[0]).unwrap();
        assert_eq!(skipped.status, OccurrenceStatus::Skipped);
        assert_eq!(skipped.reason.as_ref().unwrap().code, SkipCode::Expired);
        assert!(skipped.skipped_count > 8_000);
        assert_eq!(store.automation_occurrences.len(), 2);
    }

    #[test]
    fn catch_up_skip_all_records_one_skipped_occurrence() {
        let (mut store, id) = store_with(true);
        store.automation_mut(id).unwrap().policy.catch_up = CatchUp::SkipAll;
        let plans = store.plan_automation_ticks(1800);
        let commit = store.commit_automation_tick(&plans[0], 1800).unwrap();
        assert!(commit.queued.is_empty());
        let skipped = store.occurrence(commit.skipped[0]).unwrap();
        assert_eq!(skipped.reason.as_ref().unwrap().code, SkipCode::CatchUpSkipped);
        assert_eq!(store.automation(id).unwrap().cursor, Some(2100));
    }

    #[test]
    fn manual_run_deduplicates_requests_and_never_moves_the_cursor() {
        let (mut store, id) = store_with(true);
        let cursor = store.automation(id).unwrap().cursor;
        let first = store.request_manual_occurrence(id, "request-1", 1000).unwrap();
        assert_eq!(first.source, TriggerSource::Manual);
        assert_eq!(first.scheduled_for, None);
        assert_eq!(first.requested_at, Some(1000));
        assert_eq!(store.automation(id).unwrap().cursor, cursor);
        // The same request is idempotent.
        let again = store.request_manual_occurrence(id, "request-1", 2000).unwrap();
        assert_eq!(again.id, first.id);
        // A different manual request is refused while one is unstarted.
        assert!(store
            .request_manual_occurrence(id, "request-2", 1000)
            .is_err());
        // A scheduled occurrence does not replace a pending manual request.
        let plans = store.plan_automation_ticks(1800);
        let commit = store.commit_automation_tick(&plans[0], 1800).unwrap();
        assert!(commit.queued.is_empty());
        assert_eq!(store.occurrence(first.id).unwrap().status, OccurrenceStatus::Queued);
        let skipped = store.occurrence(commit.skipped[0]).unwrap();
        assert_eq!(skipped.reason.as_ref().unwrap().code, SkipCode::ManualPending);
    }

    #[test]
    fn pause_edit_archive_and_project_removal_cancel_unstarted_work() {
        let (mut store, id) = store_with(true);
        let plans = store.plan_automation_ticks(300);
        store.commit_automation_tick(&plans[0], 300).unwrap();
        let occurrence = store.automation_occurrences.iter().find(|occurrence| occurrence.status == OccurrenceStatus::Queued).unwrap().id;
        let cancelled = store.cancel_unstarted_occurrences(
            id,
            SkipCode::Paused,
            "paused",
        );
        assert_eq!(cancelled, vec![occurrence]);
        assert_eq!(store.occurrence(occurrence).unwrap().status, OccurrenceStatus::Skipped);
        store.pause_automation(id, 400).unwrap();
        assert_eq!(store.automation(id).unwrap().cursor, None);
        assert!(store.plan_automation_ticks(10_000).is_empty());
        // Resuming re-anchors and does not backfill.
        store.resume_automation(id, 10_000).unwrap();
        let cursor = store.automation(id).unwrap().cursor.unwrap();
        assert!(cursor > 10_000);
        store.projects.clear();
        let plans = store.plan_automation_ticks(cursor);
        assert!(matches!(plans[0].action, TickAction::Attention { .. }));
        let commit = store.commit_automation_tick(&plans[0], cursor).unwrap();
        assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::NeedsAttention);
        assert_eq!(commit.attention.len(), 1);
    }

    #[test]
    fn failure_threshold_pauses_and_success_clears_counter() {
        let (mut store, id) = store_with(true);
        store.automation_mut(id).unwrap().policy.max_consecutive_failures = 2;
        let occurrence = store.request_manual_occurrence(id, "r1", 1000).unwrap();
        assert!(store.fail_occurrence(occurrence.id, "network", 1000).unwrap().is_none());
        assert_eq!(store.automation(id).unwrap().consecutive_failures, 1);
        // Skipped and cancelled runs do not count.
        let skipped = store.request_manual_occurrence(id, "r2", 1100).unwrap();
        store.stop_occurrence(skipped.id, 1100).unwrap();
        assert_eq!(store.automation(id).unwrap().consecutive_failures, 1);
        let second = store.request_manual_occurrence(id, "r3", 1200).unwrap();
        let reason = store.fail_occurrence(second.id, "network", 1200).unwrap().unwrap();
        assert!(reason.contains("consecutive"));
        assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::NeedsAttention);
        store.note_automation_success(id);
        store.resume_automation(id, 2000).unwrap();
        assert_eq!(store.automation(id).unwrap().consecutive_failures, 0);
    }

    #[test]
    fn success_requires_final_report_not_process_exit() {
        let (mut store, id) = store_with(true);
        let occurrence = store
            .materialize_occurrence(build_occurrence(store.automation(id).unwrap(), TriggerSource::Scheduled, 300, 300).unwrap())
            .unwrap();
        let goal = occurrence.research_goal_id.unwrap();
        store.occurrence_mut(occurrence.id).unwrap().mark_running(300).unwrap();
        // Mark every task done but provide no result: not success.
        for task in store.tasks.iter_mut().filter(|t| t.research.as_ref().is_some_and(|r| r.goal_id == goal)) {
            task.stage = Stage::Done;
        }
        assert!(!store.complete_occurrence(occurrence.id, 1, 300).unwrap());
        // A validated final report is required.
        for task in store.research_children(goal) {
            if store.research_result(task).is_none() {
                let mut run = store.prepare_run(task).unwrap();
                run.status = "Succeeded".into();
                run.output = "Report body".into();
                run.sources = vec![crate::research::Source{title:"s".into(),url:"https://example.com".into(),published_at:"unknown".into()}];
                run.research_kind = Some(ResearchKind::Advise);
                store.runs.push(run);
            }
            let _ = store.set_stage(task, Stage::Done);
        }
        // All result signatures are now valid and the advise exists.
        assert!(store.complete_occurrence(occurrence.id, 9, 300).unwrap());
        assert_eq!(store.occurrence(occurrence.id).unwrap().status, OccurrenceStatus::Succeeded);
        assert!(store.occurrence(occurrence.id).unwrap().unread);
    }

    #[test]
    fn recovery_covers_running_completed_stopped_and_unknown() {
        for (state, expected) in [
            (SessionState::Running, OccurrenceStatus::Running),
            (SessionState::Unknown, OccurrenceStatus::Verifying),
        ] {
            let (mut store, id) = store_with(true);
            let occurrence = store.request_manual_occurrence(id, "r", 1000).unwrap();
            let commit = reconcile_occurrence(&mut store, occurrence.id, state, 2000).unwrap();
            assert_eq!(store.occurrence(occurrence.id).unwrap().status, expected);
            if state == SessionState::Unknown {
                assert_eq!(commit.blocked, vec![occurrence.id]);
                assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::NeedsAttention);
            }
        }
        let (mut store, id) = store_with(true);
        let occurrence = store.request_manual_occurrence(id, "r", 1000).unwrap();
        let commit = reconcile_occurrence(&mut store, occurrence.id, SessionState::Stopped, 2000).unwrap();
        assert_eq!(commit.retry, vec![occurrence.id]);
        assert_eq!(store.occurrence(occurrence.id).unwrap().status, OccurrenceStatus::Queued);
        // Completed without final entities remains queued for the next step.
        let (mut store, id) = store_with(true);
        let occurrence = store.request_manual_occurrence(id, "r", 1000).unwrap();
        let commit = reconcile_occurrence(&mut store, occurrence.id, SessionState::Completed, 2000).unwrap();
        assert_eq!(commit.retry.len(), 1);
    }

    #[test]
    fn fake_runner_records_observable_start_and_cancel_events() {
        let (mut store, id) = store_with(true);
        let occurrence = store.request_manual_occurrence(id, "r", 1000).unwrap();
        let mut runner = FakeAutomationRunner::default();
        runner.start(&occurrence);
        assert_eq!(runner.starts.len(), 1);
        assert_eq!(runner.starts[0].reference_time, 1000);
        assert!(runner.starts[0].window_resolved);
        runner.cancel(occurrence.id);
        assert_eq!(runner.cancels, vec![occurrence.id]);
        runner.outcomes.insert(occurrence.id, false);
        runner.complete(&mut store, occurrence.id, 2000).unwrap();
        assert_eq!(store.occurrence(occurrence.id).unwrap().status, OccurrenceStatus::Failed);
    }

    #[test]
    fn time_window_stays_unresolved_when_business_calendar_is_not_verified() {
        let (mut store, id) = store_with(false);
        store.automations.push(AutomationDefinition {
            id,
            version: 1,
            project_id: 1,
            shape: crate::automation::AutomationShape::Research,
            name: "Manual".into(),
            objective: "Track".into(),
            expected_result: "Note".into(),
            schedule: schedule(60),
            status: DefinitionStatus::Draft,
            template: Some(build_template("Track", "Note").unwrap()),
            approval: Some(AutomationApproval {
                version: 1,
                approved_at: 0,
                template_signature: build_template("Track", "Note").unwrap().signature(),
                schedule_signature: schedule(60).signature(),
                models: std::array::from_fn(|_| "test/model".into()),
                agents: Default::default(),
                materials: vec![],
                permissions: AutomationPermissions::restricted("OpenCode"),
            }),
            policy: AutomationPolicy::default(),
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
        });
        let occurrence = store.request_manual_occurrence(id, "r", 1000).unwrap();
        match occurrence.data_window.state {
            TimeWindowState::Resolved { .. } => {
                panic!("a task without an explicit previous-day range must stay unresolved")
            }
            TimeWindowState::Unresolved { .. } => {}
        }
    }
}

/// A post-period report regeneration whose own deadline has passed. Its late result
/// must never replace the period's previous successful report.
pub(crate) fn expired_regeneration(
    store: &Store,
    run: &crate::domain::AgentRun,
    now: i64,
) -> bool {
    run.deadline_at.is_some_and(|deadline| now >= deadline)
        && run.research_kind == Some(crate::research::ResearchKind::Advise)
        && run.automation_occurrence_id.is_some_and(|id| {
            store
                .occurrence(id)
                .is_some_and(|occurrence| occurrence.status.is_terminal())
        })
}

/// Application glue: due scan, transactional cursor advances, queue hand-off and
/// per-occurrence timeout enforcement. No other module writes automation state.
use crate::app::TaskApp;

impl TaskApp {
    pub fn automation_now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    /// Wake at the nearest due time, bounded to 30 seconds so wall-clock changes
    /// and system wake are noticed without an execution-pump poll loop.
    pub(crate) fn arm_automation_timer(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if self.automation_timer_armed {
            return;
        }
        if self.store.next_automation_due().is_none() {
            self.automation_timer_armed = false;
            return;
        }
        self.automation_timer_armed = true;
        self.automation_generation = self.automation_generation.wrapping_add(1);
        let generation = self.automation_generation;
        let now = Self::automation_now();
        let mut delay = self
            .store
            .next_automation_due()
            .map(|due| (due - now).max(0) as u64)
            .unwrap_or(30)
            .clamp(0, 30);
        if let Some(until) = self.automation_backoff_until {
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            delay = delay.max(remaining.as_secs() + (remaining.subsec_nanos() > 0) as u64);
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay * 1000 + 10))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.automation_generation == generation {
                    this.automation_timer_armed = false;
                    this.poll_automations(cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn cancel_automation_timer(&mut self) {
        self.automation_generation = self.automation_generation.wrapping_add(1);
        self.automation_timer_armed = false;
        self.automation_backoff_until = None;
        self.automation_started.clear();
    }

    pub fn poll_automations(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if self.workspace_required
            || self.storage.is_none()
            || !self.storage_error.is_empty()
        {
            return;
        }
        let now = Self::automation_now();
        let plans = self.store.plan_automation_ticks(now);
        if !plans.is_empty() {
            let rollback = self.store.clone();
            let mut commits = vec![];
            let mut failure = None;
            for plan in &plans {
                let automation = plan.automation_id;
                match self.store.commit_automation_tick(plan, now) {
                    Ok(commit) => {
                        if self
                            .store
                            .automation(automation)
                            .is_some_and(|a| a.status == DefinitionStatus::NeedsAttention)
                            && self.store.automation(automation).and_then(|a| a.attention_reason.clone())
                                .is_some_and(|r| r.starts_with("Paused after"))
                        { /* expected threshold state */ }
                        commits.push(commit)
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            if failure.is_none() {
                self.persist_if_changed(true);
                if !self.storage_error.is_empty() {
                    failure = Some(self.storage_error.clone());
                }
            }
            if let Some(error) = failure {
                // Nothing may be dispatched and no in-memory cursor may survive.
                self.store = rollback;
                self.error = format!("Automation schedule was not saved: {error}");
                self.automation_backoff_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
                self.automation_timer_armed = false;
                self.arm_automation_timer(cx);
                cx.notify();
                return;
            }
            self.automation_backoff_until = None;
            for commit in commits {
                for id in commit.superseded {
                    self.cancel_unstarted_automation_work(&[id]);
                }
                for id in commit.queued {
                    self.enqueue_automation_occurrence(id, cx);
                }
            }
        }
        self.enforce_automation_timeouts(now, cx);
        self.enforce_regeneration_deadlines(now, cx);
        if self.persist_if_changed(true) {
            cx.notify();
        }
        self.automation_timer_armed = false;
        self.arm_automation_timer(cx);
    }

    fn active_automation_occurrence_for_goal(&self, goal: u64) -> Option<u64> {
        self.store
            .automation_occurrences
            .iter()
            .rev()
            .find(|occurrence| {
                occurrence.research_goal_id == Some(goal) && occurrence.status.is_active()
            })
            .map(|occurrence| occurrence.id)
    }

    /// All normal start entry points route automation-owned steps back to their
    /// occurrence. Returns true when the caller must not continue the normal path.
    pub(crate) fn route_automation_task(&mut self, task_id: u64, cx: &mut gpui_kit::Context<Self>) -> bool {
        let Some(task) = self.store.task(task_id) else {
            return false;
        };
        if task.automation_id.is_none() {
            return false;
        }
        let Some(goal) = self.store.owning_research_goal(task_id) else {
            return true;
        };
        let Some(occurrence) = self.active_automation_occurrence_for_goal(goal) else {
            let error = "This step belongs to an automation period that is not active. Use Run now on the automation or open its history to retry the period.";
            self.error = error.into();
            self.task_start_feedback = Some((task_id, error.into()));
            cx.notify();
            return true;
        };
        if !self
            .store
            .ready_research_tasks(goal)
            .unwrap_or_default()
            .contains(&task_id)
        {
            let error = "This automation step is not ready to run. Use the automation history to review the current period.";
            self.error = error.into();
            self.task_start_feedback = Some((task_id, error.into()));
            cx.notify();
            return true;
        }
        self.enqueue_automation_occurrence(occurrence, cx);
        true
    }

    pub(crate) fn route_automation_goal(&mut self, goal: u64, cx: &mut gpui_kit::Context<Self>) -> bool {
        if self.store.task(goal).is_none_or(|task| task.automation_id.is_none()) {
            return false;
        }
        let Some(occurrence) = self.active_automation_occurrence_for_goal(goal) else {
            let error = "This automation period is not active. Use Run now on the automation or open its history to retry the period.";
            self.error = error.into();
            self.task_start_feedback = Some((goal, error.into()));
            cx.notify();
            return true;
        };
        self.enqueue_automation_occurrence(occurrence, cx);
        true
    }

    /// Load a queued occurrence into the shared run queue. The occurrence stays
    /// Queued until the queue actually starts the process.
    pub(crate) fn enqueue_automation_occurrence(
        &mut self,
        occurrence_id: u64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if self.storage.is_none() || !self.storage_error.is_empty() {
            return;
        }
        let now = Self::automation_now();
        let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
            return;
        };
        if !matches!(
            occurrence.status,
            OccurrenceStatus::Queued
                | OccurrenceStatus::Running
                | OccurrenceStatus::WaitingRetry
                | OccurrenceStatus::Verifying
        ) {
            return;
        }
        if self
            .store
            .runs
            .iter()
            .any(|run| {
                run.automation_occurrence_id == Some(occurrence_id)
                    && matches!(
                        run.status.as_str(),
                        "Queued" | "Running" | "Cancelling" | "Recovering"
                    )
            })
        {
            return;
        }
        if occurrence.first_start_expired(now) {
            self.expire_occurrence(occurrence_id, now, cx);
            return;
        }
        if let Some(deadline) = occurrence.deadline_at
            && now >= deadline
        {
            self.timeout_occurrence(occurrence_id, now, cx);
            return;
        }
        if !self.automation_unattended_enabled {
            let reason = "Unattended automation execution is disabled in this build until execution-layer tool denial is verified. Nothing was started.";
            if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                let _ = value.transition(
                    OccurrenceStatus::Verifying,
                    Some(OccurrenceReason::new(SkipCode::VerificationRequired, reason)),
                );
            }
            self.store
                .mark_automation_attention(occurrence.automation_id, reason);
            self.error = reason.into();
            self.persist_if_changed(true);
            cx.notify();
            return;
        }
        if self.automation_fake_mode {
            self.run_fake_automation(occurrence, now, cx);
            return;
        }
        let run = match self.store.prepare_occurrence_run(&occurrence, now) {
            Ok(run) => run,
            Err(error) => {
                let terminal = error.contains("no research goal")
                    || error.contains("No runnable step");
                if terminal {
                    let _ = self.store.fail_occurrence(occurrence_id, error.clone(), now);
                } else {
                    if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                        let _ = value.transition(
                            OccurrenceStatus::Verifying,
                            Some(OccurrenceReason::new(SkipCode::DefinitionChanged, error.clone())),
                        );
                    }
                    self.store
                        .mark_automation_attention(occurrence.automation_id, error.clone());
                }
                self.error = format!("Automation did not start: {error}");
                self.persist_if_changed(true);
                cx.notify();
                return;
            }
        };
        let run_id = run.id;
        let automation = occurrence.automation_id;
        let rollback = self.store.clone();
        self.launch_run(run, vec![], cx);
        let queued = self
            .store
            .runs
            .iter()
            .any(|r| r.id == run_id && r.status == "Queued");
        if queued {
            if let Some(occurrence) = self.store.occurrence_mut(occurrence_id)
                && !occurrence.run_ids.contains(&run_id)
            {
                occurrence.run_ids.push(run_id);
            }
            self.persist_if_changed(true);
            if !self.storage_error.is_empty() {
                self.store = rollback;
                self.queued_runs.retain(|(id, _)| *id != run_id);
                self.automation_backoff_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
                self.error = "Automation run was not saved and will not start.".into();
            }
        } else {
            let reason = if self.error.is_empty() {
                "The automation step could not be queued.".to_string()
            } else {
                self.error.clone()
            };
            if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                let _ = value.transition(
                    OccurrenceStatus::Verifying,
                    Some(OccurrenceReason::new(SkipCode::DefinitionChanged, reason.clone())),
                );
            }
            self.store.mark_automation_attention(automation, reason);
            self.persist_if_changed(true);
        }
        self.start_pump(cx);
        cx.notify();
    }

    fn run_fake_automation(
        &mut self,
        occurrence: AutomationOccurrence,
        now: i64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let runner = self
            .automation_fake
            .get_or_insert_with(FakeAutomationRunner::default);
        let _ = runner.start(&occurrence);
        if let Some(value) = self.store.occurrence_mut(occurrence.id) {
            let _ = value.mark_running(now);
        }
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() {
            self.error = "Fake automation state was not saved.".into();
            return;
        }
        if self.automation_fake_hold {
            cx.notify();
            return;
        }
        let result = self
            .automation_fake
            .as_mut()
            .unwrap()
            .complete(&mut self.store, occurrence.id, now);
        if let Err(error) = result {
            self.error = format!("Fake automation failed: {error}");
        }
        self.persist_if_changed(true);
        cx.notify();
    }

    fn expire_occurrence(
        &mut self,
        occurrence_id: u64,
        now: i64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
            return;
        };
        if occurrence.status == OccurrenceStatus::Queued {
            if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                let _ = value.transition(
                    OccurrenceStatus::Skipped,
                    Some(OccurrenceReason::new(
                        SkipCode::Expired,
                        "The request was still queued after its allowed delay.",
                    )),
                );
            }
            self.persist_if_changed(true);
        }
        let _ = now;
        cx.notify();
    }

    fn timeout_occurrence(
        &mut self,
        occurrence_id: u64,
        now: i64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
            return;
        };
        if occurrence.status.is_terminal() {
            self.automation_started.remove(&occurrence_id);
            return;
        }
        for run in &occurrence.run_ids {
            if self
                .store
                .runs
                .iter()
                .any(|r| r.id == *run && matches!(r.status.as_str(), "Queued" | "Running"))
            {
                self.cancel_run_id(*run, cx);
            }
        }
        for run in self.store.runs.iter_mut().filter(|r| {
            r.automation_occurrence_id == Some(occurrence_id)
                && r.status == "Queued"
        }) {
            run.status = "Cancelled".into();
            run.ended_at = Some(crate::opencode::timestamp());
            run.logs
                .push("[automation] Cancelled because the occurrence time limit was reached.".into());
        }
        if !self.store.occurrence(occurrence_id).is_some_and(|o| o.status.is_terminal()) {
            let _ = self.store.occurrence_mut(occurrence_id).map(|value| {
                value.transition(
                    OccurrenceStatus::Failed,
                    Some(OccurrenceReason::new(
                        SkipCode::TimedOut,
                        "The automation's total time limit was reached.",
                    )),
                )
            });
            if let Some(reason) = self.store.note_automation_failure(occurrence.automation_id, now) {
                self.toast = reason;
            }
            self.error = "Automation stopped at its time limit.".into();
        }
        self.persist_if_changed(true);
        cx.notify();
    }

    fn enforce_automation_timeouts(&mut self, now: i64, cx: &mut gpui_kit::Context<Self>) {
        let timed_out: Vec<u64> = self
            .store
            .automation_occurrences
            .iter()
            .filter(|o| o.status == OccurrenceStatus::Running)
            .filter(|o| {
                o.deadline_at.is_some_and(|deadline| now >= deadline)
                    || self.automation_started.get(&o.id).is_some_and(|start| {
                        start.elapsed().as_secs() as i64
                            >= o.snapshot.policy.timeout_seconds
                    })
            })
            .map(|o| o.id)
            .collect();
        for id in timed_out {
            self.timeout_occurrence(id, now, cx);
        }
    }

    /// Runs from the normal UI pump so a hung report regeneration is stopped even
    /// when the automation clock is waiting for a distant next trigger.
    pub(crate) fn enforce_active_run_deadlines(
        &mut self,
        now: i64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let mut ids: Vec<u64> = self.active_runs.iter().map(|run| run.id).collect();
        ids.extend(self.queued_runs.iter().map(|(id, _)| *id));
        ids.extend(
            self.recovery
                .checks
                .keys()
                .copied()
                .filter(|id| {
                    self.store
                        .runs
                        .iter()
                        .find(|run| run.id == *id)
                        .is_some_and(|run| run.status == "Recovering")
                }),
        );
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let run = self.store.runs.iter().find(|run| run.id == id).cloned();
            let Some(run) = run else { continue };
            if !expired_regeneration(&self.store, &run, now) {
                continue;
            }
            if run.status == "Recovering" || run.status == "Interrupted" {
                if let Some(value) = self.store.runs.iter_mut().find(|run| run.id == id) {
                    value.status = "Cancelled".into();
                    value.ended_at = Some(crate::opencode::timestamp());
                    if !value.logs.iter().any(|line| line.contains("time limit")) {
                        value.logs.push(
                            "[automation] Recovered report regeneration exceeded its own time limit; the previous successful report is preserved."
                                .into(),
                        );
                    }
                }
                self.recovery.checks.remove(&id);
                self.persist_if_changed(true);
            } else {
                if let Some(value) = self.store.runs.iter_mut().find(|run| run.id == id)
                    && !value.logs.iter().any(|line| line.contains("time limit"))
                {
                    value.logs.push(
                        "[automation] Report regeneration exceeded its own time limit; the previous successful report is preserved."
                            .into(),
                    );
                }
                self.cancel_run_id(id, cx);
            }
        }
    }

    fn enforce_regeneration_deadlines(
        &mut self,
        now: i64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let expired: Vec<u64> = self
            .store
            .runs
            .iter()
            .filter(|run| {
                matches!(run.status.as_str(), "Queued" | "Running" | "Recovering")
                    && expired_regeneration(&self.store, run, now)
            })
            .map(|run| run.id)
            .collect();
        for id in expired {
            if let Some(run) = self.store.runs.iter_mut().find(|run| run.id == id) {
                if !run.logs.iter().any(|line| line.contains("time limit")) {
                    run.logs.push(
                        "[automation] Report regeneration exceeded its own time limit; the previous successful report is preserved."
                            .into(),
                    );
                }
            }
            if self
                .store
                .runs
                .iter()
                .find(|run| run.id == id)
                .is_some_and(|run| run.status == "Recovering")
            {
                if let Some(value) = self.store.runs.iter_mut().find(|run| run.id == id) {
                    value.status = "Cancelled".into();
                    value.ended_at = Some(crate::opencode::timestamp());
                }
                self.recovery.checks.remove(&id);
                self.persist_if_changed(true);
            } else {
                self.cancel_run_id(id, cx);
            }
        }
    }

    /// Called after a run belonging to an occurrence is finalized.
    pub(crate) fn automation_after_run(
        &mut self,
        occurrence_id: u64,
        run_id: u64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let now = Self::automation_now();
        let Some(occurrence) = self.store.occurrence(occurrence_id).cloned() else {
            return;
        };
        let Some(run) = self.store.runs.iter().find(|r| r.id == run_id).cloned() else {
            return;
        };
        if occurrence.status.is_terminal() {
            // A report regeneration is a new attempt for the same finished period.
            if occurrence.status == OccurrenceStatus::Succeeded
                && run.status == "Succeeded"
                && run.research_kind == Some(crate::research::ResearchKind::Advise)
                && !run.output.trim().is_empty()
                && !expired_regeneration(&self.store, &run, now)
                && occurrence.final_run_id != Some(run_id)
            {
                if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                    value.final_run_id = Some(run_id);
                    value.unread = true;
                }
                self.persist_if_changed(true);
            }
            self.automation_started.remove(&occurrence_id);
            return;
        }
        let definition = occurrence.automation_id;
        // Timeout wins even if a late success arrives.
        if occurrence.deadline_at.is_some_and(|deadline| now >= deadline) {
            self.timeout_occurrence(occurrence_id, now, cx);
            return;
        }
        match run.status.as_str() {
            "Succeeded" => {
                let goals_done = self.store.occurrence_goals_complete(&occurrence);
                if goals_done
                    && self
                        .store
                        .complete_occurrence(occurrence_id, run_id, now)
                        .unwrap_or(false)
                {
                    let resumed = self
                        .store
                        .resume_after_verified_recovery(definition, now);
                    self.toast = if resumed {
                        "Automation recovered · future triggers resumed".into()
                    } else {
                        "Automation run completed · result is unread".into()
                    };
                    self.persist_if_changed(true);
                    cx.notify();
                    return;
                }
                if goals_done {
                    let _ = self.store.fail_occurrence(
                        occurrence_id,
                        "The final report did not pass validation.",
                        now,
                    );
                    self.store
                        .mark_automation_attention(definition, "The final report did not pass validation. Review the run.");
                } else {
                    // Continue the same period with the next dependency-ready step,
                    // including the next approved goal.
                    self.enqueue_automation_occurrence(occurrence_id, cx);
                }
            }
            "Failed" | "Blocked" | "Interrupted" | "Cancelled" | "Stale" => {
                let scheduled_retry = self.store.runs.iter().any(|other| {
                    other.id != run_id
                        && other.automation_occurrence_id == Some(occurrence_id)
                        && other.status == "Queued"
                });
                if scheduled_retry {
                    let _ = self.store.wait_for_retry(
                        occurrence_id,
                        "Waiting for the approved step retry; the period, inputs and time window are unchanged.",
                    );
                    self.persist_if_changed(true);
                } else if run.status == "Failed" {
                    let threshold = self
                        .store
                        .fail_occurrence(
                            occurrence_id,
                            run.logs
                                .iter()
                                .rev()
                                .find_map(|line| line.strip_prefix("[error] "))
                                .unwrap_or("The automation step failed."),
                            now,
                        )
                        .unwrap_or(None);
                    if let Some(reason) = threshold {
                        self.toast = reason;
                    }
                } else if matches!(run.status.as_str(), "Blocked" | "Stale") {
                    if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                        let _ = value.transition(
                            OccurrenceStatus::Verifying,
                            Some(OccurrenceReason::new(
                                SkipCode::DefinitionChanged,
                                "This run needs attention. Review its message before continuing.",
                            )),
                        );
                    }
                    self.store.mark_automation_attention(
                        definition,
                        run.logs
                            .iter()
                            .rev()
                            .find(|line| line.starts_with("[browser]") || line.starts_with("[workspace]") || line.starts_with("[error]"))
                            .cloned()
                            .unwrap_or_else(|| "A run needs attention. Open it to see the reason.".into()),
                    );
                } else {
                    if let Some(value) = self.store.occurrence_mut(occurrence_id) {
                        let _ = value.transition(
                            OccurrenceStatus::Cancelled,
                            Some(OccurrenceReason::new(
                                SkipCode::Paused,
                                "The run was cancelled.",
                            )),
                        );
                    }
                }
            }
            _ => {}
        }
        if self
            .store
            .occurrence(occurrence_id)
            .is_some_and(|occurrence| occurrence.status.is_terminal())
        {
            self.automation_started.remove(&occurrence_id);
        }
        self.persist_if_changed(true);
        cx.notify();
    }

    /// Startup reconciliation. Unknown session state never triggers a replay.
    pub(crate) fn recover_automations(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let now = Self::automation_now();
        let active: Vec<u64> = self
            .store
            .automation_occurrences
            .iter()
            .filter(|o| o.status.is_active())
            .map(|o| o.id)
            .collect();
        let mut retry = vec![];
        let mut attention = false;
        for id in active {
            let occurrence = self.store.occurrence(id).cloned().unwrap();
            let runs: Vec<_> = self
                .store
                .runs
                .iter()
                .filter(|r| r.automation_occurrence_id == Some(id))
                .collect();
            let latest = runs.iter().max_by_key(|r| r.id);
            let state = match latest {
                None if occurrence.status == OccurrenceStatus::Queued => {
                    // The commit happened before dispatch: safe to queue the saved intent.
                    retry.push(id);
                    continue;
                }
                None => SessionState::Unknown,
                Some(run) if run.status == "Succeeded" => SessionState::Completed,
                Some(run)
                    if run.status == "Interrupted"
                        && (run.started_at.is_empty()
                            || run.logs.iter().any(|line| {
                                line.contains("Application closed before a final result")
                            })) =>
                {
                    SessionState::Stopped
                }
                Some(_) => SessionState::Unknown,
            };
            match reconcile_occurrence(&mut self.store, id, state, now) {
                Ok(commit) => {
                    retry.extend(commit.retry);
                    attention |= !commit.attention.is_empty();
                }
                Err(error) => {
                    self.error = format!("Automation recovery needs attention: {error}");
                    attention = true;
                }
            }
        }
        if let Some(reason) = self
            .store
            .automations
            .iter()
            .filter(|definition| definition.status == DefinitionStatus::Enabled)
            .find_map(|definition| self.store.automation_blocker(definition))
        {
            self.toast = format!("An automation needs attention: {reason}");
        }
        if attention {
            self.publishing_scan_requested = true;
        }
        self.persist_if_changed(true);
        if self.storage_error.is_empty() {
            for id in retry {
                self.enqueue_automation_occurrence(id, cx);
            }
        }
        self.arm_automation_timer(cx);
        cx.notify();
    }

    /// Stop the active run and every scheduled retry for one occurrence.
    pub(crate) fn stop_automation_occurrence(
        &mut self,
        occurrence_id: u64,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let now = Self::automation_now();
        self.automation_started.remove(&occurrence_id);
        if self.automation_fake_mode
            && let Some(runner) = self.automation_fake.as_mut()
        {
            runner.cancel(occurrence_id);
        }
        let Ok(run_ids) = self.store.stop_occurrence(occurrence_id, now) else {
            return;
        };
        for run in &run_ids {
            if self.active_runs.iter().any(|a| a.id == *run)
                || self.queued_runs.iter().any(|(id, _)| id == run)
            {
                self.cancel_run_id(*run, cx);
            }
        }
        self.queued_runs.retain(|(id, _)| {
            !self
                .store
                .runs
                .iter()
                .any(|run| run.id == *id && run.automation_occurrence_id == Some(occurrence_id))
        });
        self.persist_if_changed(true);
        cx.notify();
    }

}
