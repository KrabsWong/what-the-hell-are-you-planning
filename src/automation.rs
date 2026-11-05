//! Automation definitions, versions, approvals, occurrences and their state rules.
//!
//! This module is pure domain state. Trigger arithmetic lives in
//! `automation_schedule`, dispatch/accounting in `automation_runtime`, and research
//! adaption in `automation_execution`.
use crate::automation_schedule::{CatchUp, DataWindow, Schedule};
use crate::research::ResearchKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const AUTOMATION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_EXECUTION_TIMEOUT_SECONDS: i64 = 30 * 60;
pub const DEFAULT_ALLOWED_DELAY_SECONDS: i64 = 24 * 60 * 60;
pub const DEFAULT_MAX_CONSECUTIVE_FAILURES: u32 = 3;
pub const DEFAULT_LOG_RETENTION_DAYS: u32 = 30;
pub const MIN_LOG_RETENTION_DAYS: u32 = 1;
pub const MAX_LOG_RETENTION_DAYS: u32 = 3650;
pub const MAX_ALLOWED_DELAY_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const MIN_TIMEOUT_SECONDS: i64 = 60;
pub const MAX_TIMEOUT_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionStatus {
    Draft,
    Enabled,
    Paused,
    NeedsAttention,
    Archived,
}
impl DefinitionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Enabled => "Enabled",
            Self::Paused => "Paused",
            Self::NeedsAttention => "Needs attention",
            Self::Archived => "Archived",
        }
    }
    pub fn is_active(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    Scheduled,
    Manual,
}
impl TriggerSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scheduled => "Schedule",
            Self::Manual => "Run now",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceStatus {
    Queued,
    Running,
    WaitingRetry,
    Verifying,
    Succeeded,
    Failed,
    Interrupted,
    Cancelled,
    Skipped,
}
impl OccurrenceStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::WaitingRetry => "Waiting to retry",
            Self::Verifying => "Checking after restart",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
        }
    }
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }
    /// An occurrence still owns this automation's execution resource.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Running | Self::WaitingRetry | Self::Verifying
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCode {
    SupersededByNewer,
    ManualPending,
    Expired,
    Paused,
    Archived,
    ProjectUnavailable,
    DefinitionChanged,
    CatchUpSkipped,
    DuplicateRequest,
    RetryScheduled,
    ExecutionFailed,
    TimedOut,
    VerificationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceReason {
    pub code: SkipCode,
    pub detail: String,
}
impl OccurrenceReason {
    pub fn new(code: SkipCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPolicy {
    pub allowed_delay_seconds: i64,
    pub timeout_seconds: i64,
    pub max_consecutive_failures: u32,
    pub catch_up: CatchUp,
    pub log_retention_days: u32,
}
impl Default for AutomationPolicy {
    fn default() -> Self {
        Self {
            allowed_delay_seconds: DEFAULT_ALLOWED_DELAY_SECONDS,
            timeout_seconds: DEFAULT_EXECUTION_TIMEOUT_SECONDS,
            max_consecutive_failures: DEFAULT_MAX_CONSECUTIVE_FAILURES,
            catch_up: CatchUp::LatestOnly,
            log_retention_days: DEFAULT_LOG_RETENTION_DAYS,
        }
    }
}
impl AutomationPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_ALLOWED_DELAY_SECONDS).contains(&self.allowed_delay_seconds) {
            return Err(format!(
                "Allowed delay must be between 1 second and {} days.",
                MAX_ALLOWED_DELAY_SECONDS / 86_400
            ));
        }
        if !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&self.timeout_seconds) {
            return Err("Execution time limit must be between 1 minute and 24 hours.".into());
        }
        if !(1..=10).contains(&self.max_consecutive_failures) {
            return Err("Pause after consecutive failures must be between 1 and 10.".into());
        }
        if !(MIN_LOG_RETENTION_DAYS..=MAX_LOG_RETENTION_DAYS).contains(&self.log_retention_days) {
            return Err("Log retention must be between 1 and 3,650 days.".into());
        }
        Ok(())
    }
}

/// The capability boundary recorded in an approval. It is copied into every
/// occurrence; changing it requires a new approved version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPermissions {
    pub agent: String,
    pub profile: String,
    pub allowed_actions: Vec<String>,
    pub denied_actions: Vec<String>,
    pub unattended: bool,
    pub notes: String,
}
impl AutomationPermissions {
    pub fn restricted(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            profile: "automation-research".into(),
            allowed_actions: vec!["read".into(), "webfetch".into(), "websearch".into()],
            denied_actions: vec![
                "write".into(),
                "edit".into(),
                "shell".into(),
                "bash".into(),
                "task".into(),
                "browser".into(),
                "question".into(),
            ],
            unattended: true,
            notes: "Restrictions are applied through a generated project-local OpenCode agent; see doc/AUTOMATION_PLAN.md §7.".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationMaterial {
    pub task_id: u64,
    pub name: String,
    /// SHA-256 of the bytes approved for this material.
    pub signature: String,
}

/// How much structure an automation carries. Both shapes reuse the same execution
/// pipeline; only the approved template and the output container differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationShape {
    /// One instruction and one read-only step, created without any planning.
    Quick,
    /// A reviewed plan produced once by the research planning chain.
    #[default]
    Research,
}
impl AutomationShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "Quick task",
            Self::Research => "Research automation",
        }
    }
    pub fn is_quick(self) -> bool {
        self == Self::Quick
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationTemplateStep {
    pub title: String,
    pub instructions: String,
    pub kind: ResearchKind,
    pub depends_on: Vec<usize>,
}

/// One reviewed research goal. Quick tasks have exactly one goal with one step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationGoal {
    pub title: String,
    /// Scope, questions, evidence and deliverable for this goal.
    pub brief: String,
    pub steps: Vec<AutomationTemplateStep>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowRule {
    /// The scheduler does not derive a range; collection verifies the requested one.
    #[default]
    Unresolved,
    /// The request explicitly means the previous natural day in the task time zone.
    PreviousLocalDay,
}

/// The approved, frozen plan. Occurrences instantiate it without replanning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AutomationTemplate {
    pub goals: Vec<AutomationGoal>,
    #[serde(default)]
    pub window_rule: WindowRule,
}

/// Serialized form that keeps workspaces written before multi-goal templates loading.
#[derive(Deserialize)]
struct AutomationTemplateWire {
    #[serde(default)]
    goals: Vec<AutomationGoal>,
    #[serde(default)]
    window_rule: WindowRule,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    steps: Vec<AutomationTemplateStep>,
}

impl<'de> Deserialize<'de> for AutomationTemplate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AutomationTemplateWire::deserialize(deserializer)?;
        let goals = if wire.goals.is_empty() && !wire.steps.is_empty() {
            vec![AutomationGoal {
                title: "Recurring task".into(),
                brief: wire.instructions.clone(),
                steps: wire.steps,
            }]
        } else {
            wire.goals
        };
        Ok(Self {
            goals,
            window_rule: wire.window_rule,
        })
    }
}

impl AutomationTemplate {
    pub fn single_goal(goal: AutomationGoal, window_rule: WindowRule) -> Self {
        Self {
            goals: vec![goal],
            window_rule,
        }
    }
    /// Research plans may contain one goal; Quick templates always contain exactly one.
    pub fn validate(&self, shape: AutomationShape) -> Result<(), String> {
        if self.goals.is_empty() {
            return Err("Generate or edit the recurring plan before enabling.".into());
        }
        if self.goals.len() > 8 {
            return Err("An automation can carry at most eight research goals.".into());
        }
        if shape.is_quick() {
            let goal = &self.goals[0];
            if self.goals.len() != 1 || goal.steps.len() != 1 {
                return Err("A quick task must have exactly one goal and one step.".into());
            }
            let step = &goal.steps[0];
            if goal.brief.trim().is_empty() || goal.brief.len() > 12_000 {
                return Err("Describe the recurring task in 1–12,000 bytes.".into());
            }
            if step.title.trim().is_empty() || step.instructions.trim().is_empty() {
                return Err("A quick task needs a step title and instructions.".into());
            }
            if !step.depends_on.is_empty() {
                return Err("A quick task's only step cannot depend on another step.".into());
            }
            return Ok(());
        }
        for goal in &self.goals {
            if goal.brief.trim().is_empty() || goal.brief.len() > 12_000 {
                return Err("Describe each recurring goal in 1–12,000 bytes.".into());
            }
            crate::research::parse_plan(&goal.plan_json()?)?;
        }
        Ok(())
    }
    /// Steps a period must execute, used to require only the models it really needs.
    pub fn kinds(&self) -> Vec<ResearchKind> {
        let mut kinds = vec![];
        for step in self.goals.iter().flat_map(|goal| &goal.steps) {
            if !kinds.contains(&step.kind) {
                kinds.push(step.kind);
            }
        }
        if kinds.is_empty() {
            kinds.push(ResearchKind::Advise);
        }
        kinds
    }
    pub fn signature(&self) -> String {
        hash_json(&serde_json::to_value(self).unwrap_or_default())
    }
    /// The label shown in the review step and in the period history.
    pub fn summary(&self) -> String {
        self.goals
            .iter()
            .map(|goal| goal.title.clone())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl AutomationGoal {
    /// The research plan payload accepted by `Store::apply_research_plan`.
    pub fn plan_json(&self) -> Result<String, String> {
        serde_json::to_string(&serde_json::json!({ "tasks": self.steps }))
            .map_err(|e| e.to_string())
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationApproval {
    /// Definition version this approval authorizes. A newer version invalidates it.
    pub version: u32,
    pub approved_at: i64,
    pub template_signature: String,
    pub schedule_signature: String,
    /// Exact effective model and agent for each research kind.
    pub models: [String; 4],
    pub agents: [Option<String>; 4],
    pub materials: Vec<AutomationMaterial>,
    pub permissions: AutomationPermissions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationOccurrenceSnapshot {
    pub objective: String,
    pub expected_result: String,
    pub template: AutomationTemplate,
    pub models: [String; 4],
    pub agents: [Option<String>; 4],
    pub materials: Vec<AutomationMaterial>,
    pub permissions: AutomationPermissions,
    pub policy: AutomationPolicy,
    pub approval_version: u32,
    pub approval_signature: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationDefinition {
    pub id: u64,
    pub version: u32,
    /// The generated automation project. New definitions always use it; older
    /// workspaces may still reference the project they were created in.
    pub project_id: u64,
    /// Missing in workspaces written before task shapes existed; those were research.
    #[serde(default)]
    pub shape: AutomationShape,
    pub name: String,
    /// What the automation should do each period.
    pub objective: String,
    /// What a useful result looks like.
    pub expected_result: String,
    pub schedule: Schedule,
    pub status: DefinitionStatus,
    pub template: Option<AutomationTemplate>,
    pub approval: Option<AutomationApproval>,
    pub policy: AutomationPolicy,
    /// Next unprocessed trigger (UTC seconds). None means no future trigger.
    pub cursor: Option<i64>,
    /// Fixed interval anchor / enable time (UTC seconds).
    pub anchor: Option<i64>,
    pub consecutive_failures: u32,
    pub attention_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl AutomationDefinition {
    pub fn approval_current(&self) -> bool {
        let Some(approval) = &self.approval else { return false };
        let Some(template) = &self.template else { return false };
        approval.version == self.version
            && approval.template_signature == template.signature()
            && approval.schedule_signature == self.schedule.signature()
            && self.schedule.validate().is_ok()
    }
    pub fn validate_draft(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 160 {
            return Err("Automation name must be 1–160 characters.".into());
        }
        if self.objective.trim().is_empty() || self.objective.len() > 12_000 {
            return Err("Describe what to do in 1–12,000 bytes.".into());
        }
        if self.expected_result.trim().is_empty() || self.expected_result.len() > 12_000 {
            return Err("Describe the expected result in 1–12,000 bytes.".into());
        }
        self.schedule.validate()?;
        self.policy.validate()?;
        if let Some(template) = &self.template {
            template.validate(self.shape)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub state: TimeWindowState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TimeWindowState {
    /// Business calendar or source verification has not resolved this period yet.
    Unresolved { reason: String },
    Resolved {
        start_utc: i64,
        end_utc: i64,
        label: String,
    },
}
impl TimeWindow {
    pub fn unresolved(reason: impl Into<String>) -> Self {
        Self {
            state: TimeWindowState::Unresolved {
                reason: reason.into(),
            },
        }
    }
    pub fn resolved(window: DataWindow, label: impl Into<String>) -> Self {
        Self {
            state: TimeWindowState::Resolved {
                start_utc: window.start_utc,
                end_utc: window.end_utc,
                label: label.into(),
            },
        }
    }
    pub fn is_resolved(&self) -> bool {
        matches!(self.state, TimeWindowState::Resolved { .. })
    }
}


#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationOccurrence {
    pub id: u64,
    pub automation_id: u64,
    pub definition_version: u32,
    pub source: TriggerSource,
    /// Unique key: `scheduled:<utc>` or `manual:<request-id>`.
    pub trigger_key: String,
    pub scheduled_for: Option<i64>,
    pub requested_at: Option<i64>,
    pub actual_started_at: Option<i64>,
    pub timezone: String,
    pub reference_time: i64,
    pub data_window: TimeWindow,
    pub snapshot: AutomationOccurrenceSnapshot,
    pub status: OccurrenceStatus,
    pub reason: Option<OccurrenceReason>,
    /// First goal of the period; kept for older workspaces and single-goal links.
    pub research_goal_id: Option<u64>,
    /// Every goal instantiated for this period. A period succeeds only when all
    /// of them are complete.
    #[serde(default)]
    pub research_goal_ids: Vec<u64>,
    pub research_topic_id: Option<u64>,
    pub run_ids: Vec<u64>,
    pub final_run_id: Option<u64>,
    pub unread: bool,
    pub skipped_count: u64,
    pub skipped_from: Option<i64>,
    pub skipped_to: Option<i64>,
    pub created_at: i64,
    pub deadline_at: Option<i64>,
}
impl AutomationOccurrence {
    pub fn transition(
        &mut self,
        target: OccurrenceStatus,
        reason: Option<OccurrenceReason>,
    ) -> Result<(), String> {
        if !can_transition(self.status, target) {
            return Err(format!(
                "An occurrence cannot move from {} to {}.",
                self.status.label(),
                target.label()
            ));
        }
        self.status = target;
        self.reason = reason;
        Ok(())
    }
    pub fn mark_running(&mut self, started_at: i64) -> Result<(), String> {
        // The total time limit starts at the first actual start and survives restarts,
        // retries and wake-from-sleep. Re-dispatch never resets it.
        if self.deadline_at.is_none() {
            if self.actual_started_at.is_none() {
                self.actual_started_at = Some(started_at);
            }
            let first = self.actual_started_at.unwrap_or(started_at);
            self.deadline_at = Some(
                first
                    .checked_add(self.snapshot.policy.timeout_seconds)
                    .ok_or("The execution deadline is out of range.")?,
            );
        }
        self.transition(OccurrenceStatus::Running, None)
    }
    pub fn is_expired(&self, now: i64) -> bool {
        now.saturating_sub(self.reference_time) > self.snapshot.policy.allowed_delay_seconds
    }
    /// The allowed start delay applies only before the first actual start. Later
    /// steps and retries are governed by the occurrence's total deadline instead.
    pub fn first_start_expired(&self, now: i64) -> bool {
        self.actual_started_at.is_none() && self.is_expired(now)
    }
}

pub fn can_transition(from: OccurrenceStatus, to: OccurrenceStatus) -> bool {
    use OccurrenceStatus::*;
    match from {
        Queued => matches!(to, Running | Cancelled | Skipped | Interrupted | Failed | Verifying),
        Running => matches!(
            to,
            Queued | WaitingRetry | Verifying | Succeeded | Failed | Interrupted | Cancelled
        ),
        WaitingRetry => matches!(
            to,
            Queued | Running | Failed | Interrupted | Cancelled | Verifying
        ),
        Verifying => matches!(
            to,
            Queued | Running | Succeeded | Failed | Interrupted | Cancelled | WaitingRetry
        ),
        Interrupted => matches!(to, WaitingRetry | Verifying | Cancelled | Failed),
        Succeeded | Failed | Cancelled | Skipped => false,
    }
}

/// Trigger identity is scoped to one automation; two schedules at the same instant
/// must not collide.
pub fn scheduled_trigger_key(automation_id: u64, utc: i64) -> String {
    format!("scheduled:{automation_id}:{utc}")
}
pub fn manual_trigger_key(automation_id: u64, request_id: &str) -> Result<String, String> {
    let request_id = request_id.trim();
    if request_id.is_empty()
        || request_id.len() > 128
        || !request_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("A manual run needs a valid request identifier.".into());
    }
    Ok(format!("manual:{automation_id}:{request_id}"))
}

pub fn hash_json(value: &serde_json::Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap_or_default()))
}

/// Read helpers kept on the store so views never scan unrelated entities by hand.
impl crate::domain::Store {
    pub fn automation(&self, id: u64) -> Option<&AutomationDefinition> {
        self.automations.iter().find(|a| a.id == id)
    }
    pub fn automation_mut(&mut self, id: u64) -> Option<&mut AutomationDefinition> {
        self.automations.iter_mut().find(|a| a.id == id)
    }
    pub fn occurrence(&self, id: u64) -> Option<&AutomationOccurrence> {
        self.automation_occurrences.iter().find(|o| o.id == id)
    }
    pub fn occurrence_mut(&mut self, id: u64) -> Option<&mut AutomationOccurrence> {
        self.automation_occurrences
            .iter_mut()
            .find(|o| o.id == id)
    }
    pub fn automation_occurrences(&self, automation: u64) -> Vec<&AutomationOccurrence> {
        self.automation_occurrences
            .iter()
            .filter(|o| o.automation_id == automation)
            .collect()
    }
    pub fn automation_occurrence_by_key(&self, key: &str) -> Option<&AutomationOccurrence> {
        self.automation_occurrences
            .iter()
            .find(|o| o.trigger_key == key)
    }
    pub fn automation_active(&self, automation: u64) -> Option<&AutomationOccurrence> {
        self.automation_occurrences
            .iter()
            .find(|o| o.automation_id == automation && o.status.is_active())
    }
    pub fn automation_unread_count(&self, automation: u64) -> usize {
        self.automation_occurrences
            .iter()
            .filter(|o| o.automation_id == automation && o.unread)
            .count()
    }
    /// Every goal of a period must finish before the period can succeed.
    pub fn occurrence_goals_complete(&self, occurrence: &AutomationOccurrence) -> bool {
        let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else {
            occurrence.research_goal_ids.clone()
        };
        !goals.is_empty() && goals.iter().all(|goal| self.goal_tasks_complete(*goal))
    }
    /// Reject duplicate persisted identities or trigger keys after loading a workspace.
    pub fn validate_automation_integrity(&self) -> Result<(), String> {
        let mut automation_ids = std::collections::HashSet::new();
        for definition in &self.automations {
            if definition.id == 0 || !automation_ids.insert(definition.id) {
                return Err("Automation definitions contain a duplicate or zero identifier.".into());
            }
        }
        let mut occurrence_ids = std::collections::HashSet::new();
        let mut trigger_keys = std::collections::HashSet::new();
        for occurrence in &self.automation_occurrences {
            if occurrence.id == 0 || !occurrence_ids.insert(occurrence.id) {
                return Err("Automation runs contain a duplicate or zero identifier.".into());
            }
            if occurrence.trigger_key.trim().is_empty()
                || !trigger_keys.insert(occurrence.trigger_key.clone())
            {
                return Err("Automation runs contain a duplicate or empty trigger key.".into());
            }
            if !self
                .automations
                .iter()
                .any(|definition| definition.id == occurrence.automation_id)
            {
                return Err(format!(
                    "Automation run {} refers to a missing automation.",
                    occurrence.id
                ));
            }
        }
        Ok(())
    }

    pub fn next_automation_id(&self) -> Result<u64, String> {
        self.automations
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "No automation identifiers remain.".into())
    }
    pub fn next_occurrence_id(&self) -> Result<u64, String> {
        self.automation_occurrences
            .iter()
            .map(|o| o.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "No automation run identifiers remain.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation_schedule::{ScheduleSpec, CatchUp};
    use crate::domain::Store;

    fn schedule() -> Schedule {
        Schedule {
            spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
            timezone: "Asia/Shanghai".into(),
        }
    }
    fn template() -> AutomationTemplate {
        AutomationTemplate::single_goal(
            AutomationGoal {
                title: "Daily check".into(),
                brief: "Summarize notable changes and cite sources.".into(),
                steps: vec![
                    AutomationTemplateStep {
                        title: "Collect".into(),
                        instructions: "Collect dated primary sources.".into(),
                        kind: ResearchKind::Collect,
                        depends_on: vec![],
                    },
                    AutomationTemplateStep {
                        title: "Analyze".into(),
                        instructions: "Compare the collected evidence.".into(),
                        kind: ResearchKind::Analyze,
                        depends_on: vec![0],
                    },
                    AutomationTemplateStep {
                        title: "Report".into(),
                        instructions: "Write the requested report with gaps.".into(),
                        kind: ResearchKind::Advise,
                        depends_on: vec![1],
                    },
                ],
            },
            WindowRule::Unresolved,
        )
    }
    fn definition() -> AutomationDefinition {
        AutomationDefinition {
            id: 1,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Research,
            name: "Daily check".into(),
            objective: "Track changes".into(),
            expected_result: "A short sourced note".into(),
            schedule: schedule(),
            status: DefinitionStatus::Draft,
            template: Some(template()),
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn draft_validation_rejects_empty_or_dynamic_templates() {
        let mut value = definition();
        assert!(value.validate_draft().is_ok());
        value.name.clear();
        assert!(value.validate_draft().is_err());
        value.name = "Valid".into();
        let goal = &mut value.template.as_mut().unwrap().goals[0];
        goal.steps[1].depends_on = vec![9];
        assert!(value.validate_draft().is_err());
        let goal = &mut value.template.as_mut().unwrap().goals[0];
        goal.steps[1].depends_on = vec![0];
        goal.steps.pop();
        assert!(value.validate_draft().is_err());
        value.template = None;
        assert!(value.validate_draft().is_ok());
    }

    #[test]
    fn approval_is_bound_to_version_template_and_schedule() {
        let mut value = definition();
        let signature = value.template.as_ref().unwrap().signature();
        value.approval = Some(AutomationApproval {
            version: 1,
            approved_at: 0,
            template_signature: signature,
            schedule_signature: value.schedule.signature(),
            models: Default::default(),
            agents: Default::default(),
            materials: vec![],
            permissions: AutomationPermissions::restricted("OpenCode"),
        });
        assert!(value.approval_current());
        value.version = 2;
        assert!(!value.approval_current());
        value.version = 1;
        value.schedule.timezone = "UTC".into();
        assert!(!value.approval_current());
        value = definition();
        value.template.as_mut().unwrap().goals[0].steps[0].instructions.push_str(" changed");
        assert!(!value.approval_current());
    }

    #[test]
    fn occurrence_transitions_reject_completed_reentry_and_skip_preserves_reason() {
        let mut occurrence = AutomationOccurrence {
            id: 1,
            automation_id: 1,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: scheduled_trigger_key(1, 100),
            scheduled_for: Some(100),
            requested_at: None,
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: 100,
            data_window: TimeWindow::unresolved("not collected yet"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Track".into(),
                expected_result: "Note".into(),
                template: template(),
                models: Default::default(),
                agents: Default::default(),
                materials: vec![],
                permissions: AutomationPermissions::restricted("OpenCode"),
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "sig".into(),
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
            created_at: 0,
            deadline_at: None,
        };
        occurrence.mark_running(110).unwrap();
        assert_eq!(occurrence.status, OccurrenceStatus::Running);
        assert_eq!(occurrence.deadline_at, Some(110 + DEFAULT_EXECUTION_TIMEOUT_SECONDS));
        occurrence
            .transition(
                OccurrenceStatus::Cancelled,
                Some(OccurrenceReason::new(SkipCode::Expired, "late")),
            )
            .unwrap();
        assert_eq!(occurrence.reason.as_ref().unwrap().code, SkipCode::Expired);
        assert!(occurrence
            .transition(OccurrenceStatus::Running, None)
            .is_err());
        assert!(!can_transition(
            OccurrenceStatus::Succeeded,
            OccurrenceStatus::Running
        ));
        assert!(can_transition(
            OccurrenceStatus::Interrupted,
            OccurrenceStatus::WaitingRetry
        ));
    }

    #[test]
    fn trigger_keys_differ_and_survive_version_changes() {
        assert_eq!(scheduled_trigger_key(7, 42), "scheduled:7:42");
        assert_ne!(scheduled_trigger_key(7, 42), manual_trigger_key(7, "abc").unwrap());
        assert!(manual_trigger_key(7, "").is_err());
        assert!(manual_trigger_key(7, "bad key").is_err());
        assert_eq!(manual_trigger_key(7, "req-1").unwrap(), "manual:7:req-1");
        assert_ne!(scheduled_trigger_key(7, 42), scheduled_trigger_key(8, 42));
    }

    #[test]
    fn policy_validates_positive_bounded_values() {
        let policy = AutomationPolicy::default();
        assert!(policy.validate().is_ok());
        for invalid in [
            AutomationPolicy {
                allowed_delay_seconds: 0,
                ..AutomationPolicy::default()
            },
            AutomationPolicy {
                allowed_delay_seconds: MAX_ALLOWED_DELAY_SECONDS + 1,
                ..AutomationPolicy::default()
            },
            AutomationPolicy {
                timeout_seconds: 0,
                ..AutomationPolicy::default()
            },
            AutomationPolicy {
                max_consecutive_failures: 0,
                ..AutomationPolicy::default()
            },
            AutomationPolicy {
                log_retention_days: 0,
                ..AutomationPolicy::default()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        assert_eq!(AutomationPolicy::default().catch_up, CatchUp::LatestOnly);
    }

    #[test]
    fn loaded_occurrences_reject_duplicate_trigger_keys_and_missing_definitions() {
        let mut store = Store::empty();
        store.automations.push(definition());
        let mut occurrence = AutomationOccurrence {
            id: 1,
            automation_id: 1,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: scheduled_trigger_key(1, 100),
            scheduled_for: Some(100),
            requested_at: None,
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: 100,
            data_window: TimeWindow::unresolved("test"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Track".into(),
                expected_result: "Note".into(),
                template: template(),
                models: std::array::from_fn(|_| "test/model".to_string()),
                agents: Default::default(),
                materials: vec![],
                permissions: AutomationPermissions::restricted("OpenCode"),
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "sig".into(),
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
            created_at: 0,
            deadline_at: None,
        };
        assert!(store.validate_automation_integrity().is_ok());
        store.automation_occurrences.push(occurrence.clone());
        let mut duplicate = occurrence.clone();
        duplicate.id = 2;
        store.automation_occurrences.push(duplicate);
        assert!(store.validate_automation_integrity().is_err());
        store.automation_occurrences.pop();
        store.automation_occurrences[0].automation_id = 99;
        assert!(store.validate_automation_integrity().is_err());
    }

    #[test]
    fn serialization_round_trip_and_legacy_defaults() {
        let store = Store::empty();
        let json = serde_json::to_value(&store).unwrap();
        assert_eq!(json["automations"], serde_json::json!([]));
        let restored: Store = serde_json::from_value(json).unwrap();
        assert!(restored.automations.is_empty());
        assert!(restored.automation_occurrences.is_empty());
        let value = definition();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<AutomationDefinition>(&json).unwrap(),
            value
        );
    }
}
