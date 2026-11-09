//! T13 S0: domain-only prepared commands and scoped undo.
//!
//! This module deliberately contains no GPUI and no persistence I/O. It prepares
//! one command on a cloned `Store`, validates before publishing the clone, and
//! records only the inverse of the named edit. S1 adds the app executor; S2 adds
//! the single-commit dispatch boundary and lifecycle effects.
//!
//! S0 is a working subset, not the whole T13 surface: task-detail and task-stage
//! edits, their inverses, a pause preparation that delegates to the existing
//! automation command, and dispatch staging that writes a run and its occurrence
//! link into one staged store. The remaining commands are listed in the audit.

#![allow(dead_code)] // Boundary types are consumed by S1/S2; S0 tests exercise the core.

use crate::automation::{
    AutomationDefinition, AutomationMaterial, AutomationPolicy, AutomationShape,
    AutomationTemplate, DefinitionStatus, OccurrenceStatus,
};
use crate::automation_schedule::Schedule;
use crate::domain::{AgentRun, Stage, Store, Task, activity_event};
use crate::execution_eligibility::{
    ActiveAttempt, EligibilityDecision, EligibilityOwner, StepOutcome,
};
use crate::research::ResearchKind;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandId {
    pub kind: &'static str,
    pub target: u64,
    pub operation_key: String,
}

impl CommandId {
    pub(crate) fn new(kind: &'static str, target: u64, operation_key: impl Into<String>) -> Self {
        Self {
            kind,
            target,
            operation_key: operation_key.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandClass {
    /// Enabling, resuming, retrying, dispatching, or another future-extending
    /// action. A storage failure must not run its effects.
    StartOrExtend,
    /// Ordinary user edit. A failure leaves the live store untouched and records
    /// no undo entry.
    OrdinaryEdit,
    /// Stop/cancel existing external work. This is the explicit safety exception:
    /// stopping may proceed without a durable cancellation, but it never claims
    /// durability. See the audit §5.5.
    StopExternalWork,
}

/// The state a command was prepared from. The revision is the store's
/// monotonic tracked-mutation stamp: every tracked entity/scalar write advances
/// it, so a stale base never replays pipelined state. It deliberately does not
/// clone or hash the whole store on the UI thread.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandBase {
    pub revision: u64,
    pub next_id: u64,
    pub run_id_floor: u64,
}

impl CommandBase {
    pub(crate) fn capture(store: &Store) -> Self {
        Self {
            revision: store.dirty_epoch(),
            next_id: store.next_id(),
            run_id_floor: store.run_id_floor(),
        }
    }

    pub(crate) fn matches(&self, store: &Store) -> bool {
        self.revision == store.dirty_epoch()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskFields {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub cycle: Option<String>,
    pub due_date: String,
}

impl TaskFields {
    pub(crate) fn from_task(task: &Task) -> Self {
        Self {
            title: task.title.clone(),
            description: task.description.clone(),
            labels: task.labels.clone(),
            cycle: task.cycle.clone(),
            due_date: task.due_date.clone(),
        }
    }

    fn apply_to(&self, task: &mut Task) {
        task.title = self.title.clone();
        task.description = self.description.clone();
        task.labels = self.labels.clone();
        task.cycle = self.cycle.clone();
        task.due_date = self.due_date.clone();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UndoEntry {
    TaskFields {
        id: u64,
        before: TaskFields,
        after: TaskFields,
    },
    TaskStage {
        id: u64,
        before: Stage,
        before_approved: Option<String>,
        after: Stage,
        after_approved: Option<String>,
    },
}

impl UndoEntry {
    pub(crate) fn target(&self) -> u64 {
        match self {
            Self::TaskFields { id, .. } | Self::TaskStage { id, .. } => *id,
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::TaskFields { .. } => "task_fields",
            Self::TaskStage { .. } => "task_stage",
        }
    }

    fn matches_after(&self, store: &Store) -> bool {
        let Some(task) = store.task(self.target()) else {
            return false;
        };
        match self {
            Self::TaskFields { after, .. } => TaskFields::from_task(task) == *after,
            Self::TaskStage {
                after,
                after_approved,
                ..
            } => task.stage == *after && task.approved_plan == *after_approved,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EditCommand {
    TaskDetails {
        id: u64,
        title: String,
        description: String,
        labels: Vec<String>,
        due_date: String,
    },
    TaskStage {
        id: u64,
        stage: Stage,
    },
}

impl EditCommand {
    fn target(&self) -> u64 {
        match self {
            Self::TaskDetails { id, .. } | Self::TaskStage { id, .. } => *id,
        }
    }

    fn id(&self) -> CommandId {
        match self {
            Self::TaskDetails { id, .. } => {
                CommandId::new("task_details", *id, format!("task:{id}:details"))
            }
            Self::TaskStage { id, stage } => CommandId::new(
                "task_stage",
                *id,
                format!("task:{id}:stage:{}", stage.key()),
            ),
        }
    }

    fn guard_reprepare(&self, base: &CommandBase, live: &Store) -> Result<(), CommandError> {
        match self {
            Self::TaskDetails { id, .. } => {
                let before = base
                    .store()
                    .task(*id)
                    .ok_or_else(|| CommandError::Conflict("The task was removed.".into()))?;
                let now = live
                    .task(*id)
                    .ok_or_else(|| CommandError::Conflict("The task was removed.".into()))?;
                if TaskFields::from_task(before) != TaskFields::from_task(now) {
                    return Err(CommandError::Conflict(
                        "The task changed after this edit was prepared.".into(),
                    ));
                }
            }
            Self::TaskStage { id, .. } => {
                let before = base
                    .store()
                    .task(*id)
                    .ok_or_else(|| CommandError::Conflict("The task was removed.".into()))?;
                let now = live
                    .task(*id)
                    .ok_or_else(|| CommandError::Conflict("The task was removed.".into()))?;
                if before.stage != now.stage || before.approved_plan != now.approved_plan {
                    return Err(CommandError::Conflict(
                        "The task stage changed after this command was prepared.".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomationDraftInput {
    pub project: u64,
    pub shape: AutomationShape,
    pub name: String,
    pub objective: String,
    pub expected_result: String,
    pub schedule: Schedule,
    pub policy: AutomationPolicy,
    pub template: Option<AutomationTemplate>,
}

impl AutomationDraftInput {
    fn signature(&self) -> Option<String> {
        serde_json::to_string(&serde_json::json!({
            "objective": self.objective,
            "expected_result": self.expected_result,
            "schedule": self.schedule,
            "policy": self.policy,
            "template": self.template,
            "shape": self.shape,
        }))
        .ok()
    }
}

fn definition_form_signature(definition: &AutomationDefinition) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "objective": definition.objective,
        "expected_result": definition.expected_result,
        "schedule": definition.schedule,
        "policy": definition.policy,
        "template": definition.template,
        "shape": definition.shape,
    }))
    .ok()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Edit(EditCommand),
    /// Create or revise one automation draft. Validation and versioning stay in
    /// the existing Store methods.
    SaveDraft {
        id: Option<u64>,
        input: AutomationDraftInput,
        now: i64,
    },
    /// Save/revise the reviewed draft and approve it in one staged store.
    Enable {
        id: Option<u64>,
        input: AutomationDraftInput,
        materials: Vec<AutomationMaterial>,
        now: i64,
    },
    /// Delegates to the existing `Store::pause_automation`.
    AutomationPause {
        id: u64,
        now: i64,
    },
    ResumeSchedule {
        id: u64,
        now: i64,
    },
    RetryOccurrence {
        occurrence: u64,
        now: i64,
    },
    RunNow {
        automation: u64,
        request_id: String,
        now: i64,
    },
    StopOccurrence {
        occurrence: u64,
        now: i64,
    },
    Archive {
        id: u64,
        now: i64,
    },
    Copy {
        id: u64,
        now: i64,
    },
    SetRead {
        occurrence: u64,
        unread: bool,
    },
    CleanupLogs {
        id: u64,
        now: i64,
        apply: bool,
    },
    /// Stage one run and its occurrence link in the same cloned store. This is
    /// the S0 shape; S2 extracts the remaining launch preparation and removes
    /// the current two-write enqueue path.
    DispatchRun {
        occurrence: Option<u64>,
        run: AgentRun,
    },
    /// Requeue a run that was committed before its queue effect (for example a
    /// crash after the single dispatch commit). It never creates a second run.
    RequeueRun {
        run_id: u64,
        occurrence: u64,
        now: i64,
    },
    /// Commit a planning run and the request's planning-run mapping atomically.
    DispatchRequestPlan {
        request_id: u64,
        run: AgentRun,
    },
    /// Commit a report-regeneration attempt for an ordinary or automation source.
    DispatchRegeneration {
        occurrence: Option<u64>,
        source_id: u64,
        style: String,
        run: AgentRun,
    },
    Undo(UndoEntry),
}

impl Command {
    pub(crate) fn class(&self) -> CommandClass {
        match self {
            Self::Edit(_) | Self::Undo(_) => CommandClass::OrdinaryEdit,
            Self::StopOccurrence { .. } => CommandClass::StopExternalWork,
            Self::SaveDraft { .. }
            | Self::Enable { .. }
            | Self::AutomationPause { .. }
            | Self::ResumeSchedule { .. }
            | Self::RetryOccurrence { .. }
            | Self::RunNow { .. }
            | Self::Archive { .. }
            | Self::Copy { .. }
            | Self::SetRead { .. }
            | Self::CleanupLogs { .. }
            | Self::DispatchRun { .. }
            | Self::RequeueRun { .. }
            | Self::DispatchRequestPlan { .. }
            | Self::DispatchRegeneration { .. } => CommandClass::StartOrExtend,
        }
    }

    pub(crate) fn id(&self) -> CommandId {
        match self {
            Self::Edit(edit) => edit.id(),
            Self::SaveDraft { id, .. } => CommandId::new(
                "automation_save_draft",
                id.unwrap_or(0),
                format!("automation:{:?}:save_draft", id),
            ),
            Self::Enable { id, .. } => CommandId::new(
                "automation_enable",
                id.unwrap_or(0),
                format!("automation:{:?}:enable", id),
            ),
            Self::AutomationPause { id, .. } => {
                CommandId::new("automation_pause", *id, format!("automation:{id}:pause"))
            }
            Self::ResumeSchedule { id, .. } => {
                CommandId::new("automation_resume", *id, format!("automation:{id}:resume"))
            }
            Self::RetryOccurrence { occurrence, .. } => CommandId::new(
                "automation_retry",
                *occurrence,
                format!("occurrence:{occurrence}:retry"),
            ),
            Self::RunNow {
                automation,
                request_id,
                ..
            } => CommandId::new(
                "automation_run_now",
                *automation,
                format!("automation:{automation}:manual:{request_id}"),
            ),
            Self::StopOccurrence { occurrence, .. } => CommandId::new(
                "automation_stop",
                *occurrence,
                format!("occurrence:{occurrence}:stop"),
            ),
            Self::Archive { id, .. } => CommandId::new(
                "automation_archive",
                *id,
                format!("automation:{id}:archive"),
            ),
            Self::Copy { id, .. } => {
                CommandId::new("automation_copy", *id, format!("automation:{id}:copy"))
            }
            Self::SetRead { occurrence, .. } => CommandId::new(
                "automation_read",
                *occurrence,
                format!("occurrence:{occurrence}:read"),
            ),
            Self::CleanupLogs { id, .. } => CommandId::new(
                "automation_cleanup",
                *id,
                format!("automation:{id}:cleanup"),
            ),
            Self::DispatchRun { occurrence, run } => CommandId::new(
                "dispatch_run",
                occurrence.unwrap_or(run.task_id),
                format!("dispatch:{occurrence:?}:run:{}", run.id),
            ),
            Self::RequeueRun {
                run_id,
                occurrence,
                ..
            } => CommandId::new(
                "requeue_run",
                *occurrence,
                format!("occurrence:{occurrence}:requeue:{run_id}"),
            ),
            Self::DispatchRequestPlan { request_id, run } => CommandId::new(
                "dispatch_request_plan",
                *request_id,
                format!("request:{request_id}:plan-run:{}", run.id),
            ),
            Self::DispatchRegeneration {
                occurrence, run, ..
            } => CommandId::new(
                "dispatch_regeneration",
                occurrence.unwrap_or(run.task_id),
                format!("regeneration:{occurrence:?}:run:{}", run.id),
            ),
            Self::Undo(entry) => CommandId::new(
                "undo",
                entry.target(),
                format!("undo:{}:{}", entry.kind(), entry.target()),
            ),
        }
    }

    fn guard_reprepare(&self, base: &CommandBase, live: &Store) -> Result<(), CommandError> {
        match self {
            Self::Edit(edit) => edit.guard_reprepare(base, live),
            Self::SaveDraft { id: Some(id), .. } => {
                let before = base
                    .store()
                    .automation(*id)
                    .ok_or_else(|| CommandError::Conflict("The automation was removed.".into()))?;
                let now = live
                    .automation(*id)
                    .ok_or_else(|| CommandError::Conflict("The automation was removed.".into()))?;
                if before.version != now.version || before.name != now.name {
                    return Err(CommandError::Conflict(
                        "The automation changed after this draft was prepared.".into(),
                    ));
                }
                Ok(())
            }
            Self::Enable { id: Some(id), .. } => {
                let before = base
                    .store()
                    .automation(*id)
                    .ok_or_else(|| CommandError::Conflict("The automation was removed.".into()))?;
                let now = live
                    .automation(*id)
                    .ok_or_else(|| CommandError::Conflict("The automation was removed.".into()))?;
                if before.version != now.version
                    || before.status != now.status
                    || before.approval != now.approval
                    || before.cursor != now.cursor
                    || before.anchor != now.anchor
                {
                    return Err(CommandError::Conflict(
                        "The automation lifecycle changed after this enable was prepared.".into(),
                    ));
                }
                Ok(())
            }
            Self::SaveDraft { id: None, .. } | Self::Enable { id: None, .. } => Ok(()),
            Self::AutomationPause { id, .. } => {
                if live
                    .automation(*id)
                    .is_some_and(|definition| definition.status == DefinitionStatus::Paused)
                {
                    return Err(CommandError::Conflict(
                        "The automation is already paused.".into(),
                    ));
                }
                Ok(())
            }
            Self::ResumeSchedule { id, .. } => {
                if live
                    .automation(*id)
                    .is_some_and(|definition| definition.status == DefinitionStatus::Enabled)
                {
                    return Err(CommandError::Conflict(
                        "The automation is already enabled.".into(),
                    ));
                }
                Ok(())
            }
            Self::Archive { id, .. } => {
                if live
                    .automation(*id)
                    .is_some_and(|definition| definition.status == DefinitionStatus::Archived)
                {
                    return Err(CommandError::Conflict(
                        "The automation is already archived.".into(),
                    ));
                }
                Ok(())
            }
            Self::RetryOccurrence { occurrence, .. } => {
                let before = base
                    .store()
                    .occurrence(*occurrence)
                    .map(|value| value.status);
                let now = live.occurrence(*occurrence).map(|value| value.status);
                if before != now {
                    return Err(CommandError::Conflict(
                        "The automation period changed after this retry was prepared.".into(),
                    ));
                }
                Ok(())
            }
            Self::RunNow { .. }
            | Self::SetRead { .. }
            | Self::CleanupLogs { .. }
            | Self::StopOccurrence { .. } => Ok(()),
            Self::Copy { .. } => Err(CommandError::Conflict(
                "The automation changed while waiting to save; copy it again from the current state."
                    .into(),
            )),
            Self::DispatchRun { .. } => Err(CommandError::Conflict(
                "The dispatch changed while waiting to save; prepare it again from the current period."
                    .into(),
            )),
            Self::RequeueRun { run_id, .. } => {
                if live
                    .runs()
                    .iter()
                    .find(|run| run.id == *run_id)
                    .is_none_or(|run| !matches!(run.status.as_str(), "Queued" | "Interrupted"))
                {
                    return Err(CommandError::Conflict(
                        "The committed run changed before it could be queued.".into(),
                    ));
                }
                Ok(())
            }
            Self::DispatchRequestPlan { .. } | Self::DispatchRegeneration { .. } => {
                Err(CommandError::Conflict(
                    "The dispatch source changed while waiting to save; rebuild it from the current source."
                        .into(),
                ))
            }
            Self::Undo(entry) => {
                if !entry.matches_after(live) {
                    return Err(CommandError::Conflict(
                        "The target changed after this undo was prepared.".into(),
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandFeedback {
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandEffect {
    EnqueueOccurrence(u64),
    QueueRun { run_id: u64 },
    ArmAutomationTimer,
    RemoveQueuedRuns(Vec<u64>),
    StopRuns(Vec<u64>),
    Notice(CommandFeedback),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UndoDecision {
    Record(UndoEntry),
    NoOp,
    NotUndoable(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandOutcome {
    /// Definition, occurrence or run id produced/updated by this command.
    pub subject: Option<u64>,
    pub effects: Vec<CommandEffect>,
    pub undo: UndoDecision,
    pub feedback: Option<CommandFeedback>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedCommand {
    pub id: CommandId,
    pub class: CommandClass,
    pub command: Command,
    pub base: CommandBase,
    pub staged: Store,
    pub outcome: CommandOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandError {
    Rejected(String),
    Conflict(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StopIntent {
    pub occurrence: u64,
    pub run_ids: Vec<u64>,
    pub requested_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StopDurability {
    Saved,
    NotSaved { message: String },
    NotRecorded { message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StopOutcome {
    Stopped {
        durability: StopDurability,
    },
    StopRequestedNotDurable {
        intent: StopIntent,
        durability: StopDurability,
    },
    StopRequestFailed {
        intent: StopIntent,
        error: String,
    },
    /// The period reached a terminal state independently. `durable` is true only
    /// when the terminal state is already in `last_saved`.
    AlreadyEnded {
        durable: bool,
    },
}

impl StopOutcome {
    /// Only this outcome may be presented as a durable stop. The other outcomes
    /// are truthful “requested, not saved/recorded/failed” states.
    pub(crate) fn is_durably_saved(&self) -> bool {
        matches!(
            self,
            Self::Stopped {
                durability: StopDurability::Saved
            }
        )
    }
}

impl Store {
    /// Prepare exactly one command against a clone. The original store is never
    /// mutated. Validation, existing `Store` methods, and scoped inverse
    /// construction happen here; no I/O or execution is performed.
    pub(crate) fn prepare_command(
        &self,
        id: CommandId,
        class: CommandClass,
        command: Command,
    ) -> Result<PreparedCommand, CommandError> {
        if id != command.id() {
            return Err(CommandError::Rejected(
                "The command identity does not match its target.".into(),
            ));
        }
        if class != command.class() {
            return Err(CommandError::Rejected(
                "The command safety class does not match its operation.".into(),
            ));
        }
        let base = CommandBase::capture(self);
        let mut staged = self.clone();
        let outcome = match &command {
            Command::Edit(edit) => apply_edit(&mut staged, edit)?,
            Command::SaveDraft { id, input, now } => {
                let pending = id
                    .map(|value| queued_occurrences_for(&staged, value))
                    .unwrap_or_default();
                let definition = save_draft_on(&mut staged, *id, input, *now)?;
                let runs = cancel_queued_runs(&mut staged, &pending);
                CommandOutcome {
                    subject: Some(definition),
                    effects: (!runs.is_empty())
                        .then_some(CommandEffect::RemoveQueuedRuns(runs))
                        .into_iter()
                        .collect(),
                    undo: UndoDecision::NotUndoable(
                        "Automation lifecycle changes are saved as a separate operation.",
                    ),
                    feedback: None,
                }
            }
            Command::Enable {
                id,
                input,
                materials,
                now,
            } => {
                let pending = id
                    .map(|value| queued_occurrences_for(&staged, value))
                    .unwrap_or_default();
                let definition = save_draft_on(&mut staged, *id, input, *now)?;
                staged
                    .approve_and_enable(definition, materials.clone(), *now)
                    .map_err(CommandError::Rejected)?;
                let runs = cancel_queued_runs(&mut staged, &pending);
                let mut effects = vec![CommandEffect::ArmAutomationTimer];
                if !runs.is_empty() {
                    effects.push(CommandEffect::RemoveQueuedRuns(runs));
                }
                CommandOutcome {
                    subject: Some(definition),
                    effects,
                    undo: UndoDecision::NotUndoable(
                        "Automation lifecycle changes are saved as a separate operation.",
                    ),
                    feedback: None,
                }
            }
            Command::AutomationPause { id, now } => {
                let pending = queued_occurrences_for(&staged, *id);
                staged
                    .pause_automation(*id, *now)
                    .map_err(CommandError::Rejected)?;
                let runs = cancel_queued_runs(&mut staged, &pending);
                CommandOutcome {
                    subject: Some(*id),
                    effects: vec![
                        CommandEffect::ArmAutomationTimer,
                        CommandEffect::RemoveQueuedRuns(runs),
                    ],
                    undo: UndoDecision::NotUndoable(
                        "Automation lifecycle changes are saved as a separate operation.",
                    ),
                    feedback: Some(CommandFeedback {
                        title: "Paused".into(),
                        detail: "Future triggers stopped; a running period can finish.".into(),
                    }),
                }
            }
            Command::ResumeSchedule { id, now } => {
                staged
                    .resume_schedule(*id, *now)
                    .map_err(CommandError::Rejected)?;
                CommandOutcome {
                    subject: Some(*id),
                    effects: vec![CommandEffect::ArmAutomationTimer],
                    undo: UndoDecision::NotUndoable(
                        "Automation lifecycle changes are saved as a separate operation.",
                    ),
                    feedback: None,
                }
            }
            Command::RetryOccurrence { occurrence, now } => {
                staged
                    .retry_occurrence(*occurrence, *now)
                    .map_err(CommandError::Rejected)?;
                CommandOutcome {
                    subject: Some(*occurrence),
                    effects: vec![CommandEffect::EnqueueOccurrence(*occurrence)],
                    undo: UndoDecision::NotUndoable(
                        "Automation period changes are saved as a separate operation.",
                    ),
                    feedback: None,
                }
            }
            Command::RunNow {
                automation,
                request_id,
                now,
            } => {
                let occurrence = staged
                    .request_manual_occurrence(*automation, request_id, *now)
                    .map_err(CommandError::Rejected)?;
                CommandOutcome {
                    subject: Some(occurrence.id),
                    effects: vec![CommandEffect::EnqueueOccurrence(occurrence.id)],
                    undo: UndoDecision::NotUndoable(
                        "Manual automation runs are not an ordinary edit.",
                    ),
                    feedback: None,
                }
            }
            Command::StopOccurrence { occurrence, now } => {
                let _run_ids = staged
                    .stop_occurrence(*occurrence, *now)
                    .map_err(CommandError::Rejected)?;
                let queued = cancel_queued_runs(&mut staged, &[*occurrence]);
                let mut effects = vec![];
                if !queued.is_empty() {
                    effects.push(CommandEffect::RemoveQueuedRuns(queued));
                }
                CommandOutcome {
                    subject: Some(*occurrence),
                    effects,
                    undo: UndoDecision::NotUndoable("Stopping a run is not an ordinary edit."),
                    feedback: None,
                }
            }
            Command::Archive { id, now } => {
                let pending = queued_occurrences_for(&staged, *id);
                staged
                    .archive_automation(*id, *now)
                    .map_err(CommandError::Rejected)?;
                let runs = cancel_queued_runs(&mut staged, &pending);
                CommandOutcome {
                    subject: Some(*id),
                    effects: vec![CommandEffect::RemoveQueuedRuns(runs)],
                    undo: UndoDecision::NotUndoable(
                        "Automation lifecycle changes are saved as a separate operation.",
                    ),
                    feedback: None,
                }
            }
            Command::Copy { id, now } => {
                let copy = staged
                    .copy_automation(*id, *now)
                    .map_err(CommandError::Rejected)?;
                CommandOutcome {
                    subject: Some(copy),
                    effects: vec![],
                    undo: UndoDecision::NotUndoable(
                        "Copying an automation creates a new identity and is not undoable.",
                    ),
                    feedback: None,
                }
            }
            Command::SetRead { occurrence, unread } => {
                let mut value = staged
                    .occurrence_mut(*occurrence)
                    .ok_or_else(|| CommandError::Rejected("Automation period not found.".into()))?;
                value.unread = *unread;
                CommandOutcome {
                    subject: Some(*occurrence),
                    effects: vec![],
                    undo: UndoDecision::NotUndoable("Read state is runtime presentation state."),
                    feedback: None,
                }
            }
            Command::CleanupLogs { id, now, apply } => {
                let (occurrences, bytes, _) = staged
                    .automation_log_cleanup(*id, *now, *apply)
                    .map_err(CommandError::Rejected)?;
                CommandOutcome {
                    subject: Some(*id),
                    effects: vec![],
                    undo: UndoDecision::NotUndoable("Log cleanup is destructive."),
                    feedback: Some(CommandFeedback {
                        title: if *apply {
                            "Log cleanup completed"
                        } else {
                            "Log cleanup preview"
                        }
                        .into(),
                        detail: format!(
                            "{occurrences} period(s), {:.1} KB of detailed logs.",
                            bytes as f64 / 1024.
                        ),
                    }),
                }
            }
            Command::DispatchRun { occurrence, run } => {
                apply_dispatch_run(&mut staged, *occurrence, run.clone())?
            }
            Command::RequeueRun {
                run_id,
                occurrence,
                now,
            } => apply_requeue_run(&mut staged, *run_id, *occurrence, *now)?,
            Command::DispatchRequestPlan { request_id, run } => {
                apply_dispatch_request_plan(&mut staged, *request_id, run.clone())?
            }
            Command::DispatchRegeneration {
                occurrence,
                source_id,
                style,
                run,
            } => apply_dispatch_regeneration(
                &mut staged,
                *occurrence,
                *source_id,
                style,
                run.clone(),
            )?,
            Command::Undo(entry) => apply_inverse(&mut staged, entry)?,
        };

        // This mirrors the current persist-time reconciliation so an inverse
        // cannot bypass the research approval rule.
        staged.reconcile_goal_approvals();

        if class == CommandClass::OrdinaryEdit {
            assert_ordinary_edit_preserved(self, &staged)?;
        }

        Ok(PreparedCommand {
            id,
            class,
            command,
            base,
            staged,
            outcome,
        })
    }

    /// Prepare the scoped inverse through the same validation path.
    pub(crate) fn prepare_undo(&self, entry: UndoEntry) -> Result<PreparedCommand, CommandError> {
        let command = Command::Undo(entry);
        let id = command.id();
        self.prepare_command(id, CommandClass::OrdinaryEdit, command)
    }
}

/// Revalidate a failed prepared command against the current store.
///
/// If the base still matches, the command may be retried as-is. Otherwise the
/// original typed command and idempotency key are reused for a fresh preparation;
/// a conflict returns without a write, effect, or stale assignment. The old
/// `PreparedCommand.staged` is never replayed.
pub(crate) fn reprepare(
    pending: PreparedCommand,
    live: &Store,
) -> Result<PreparedCommand, CommandError> {
    if pending.base.matches(live) {
        return Ok(pending);
    }
    pending.command.guard_reprepare(&pending.base, live)?;
    live.prepare_command(pending.id.clone(), pending.class, pending.command.clone())
}

fn queued_occurrences_for(store: &Store, automation: u64) -> Vec<u64> {
    store
        .all_occurrences()
        .iter()
        .filter(|occurrence| {
            occurrence.automation_id == automation && occurrence.status == OccurrenceStatus::Queued
        })
        .map(|occurrence| occurrence.id)
        .collect()
}

fn cancel_queued_runs(store: &mut Store, occurrences: &[u64]) -> Vec<u64> {
    let runs: Vec<u64> = store
        .runs()
        .iter()
        .filter(|run| {
            run.status == "Queued"
                && run
                    .automation_occurrence_id
                    .is_some_and(|id| occurrences.contains(&id))
        })
        .map(|run| run.id)
        .collect();
    let ids: Vec<u64> = store.runs().iter().filter(|run| runs.contains(&run.id)).map(|run| run.id).collect();
    for run_id in ids {
        let mut run = store.run_mut(run_id).unwrap();
        run.status = "Cancelled".into();
        run.ended_at = Some(crate::opencode::timestamp());
        run.logs.push(
            "[automation] Cancelled before starting because its request was replaced, paused or archived."
                .into(),
        );
    }
    runs
}

fn save_draft_on(
    staged: &mut Store,
    id: Option<u64>,
    input: &AutomationDraftInput,
    now: i64,
) -> Result<u64, CommandError> {
    if let Some(id) = id {
        let existing = staged
            .automation(id)
            .ok_or_else(|| CommandError::Rejected("Automation not found.".into()))?
            .clone();
        let desired = input.signature();
        let current = definition_form_signature(&existing);
        if desired != current {
            staged
                .revise_automation_with_shape(
                    id,
                    input.project,
                    input.name.clone(),
                    input.objective.clone(),
                    input.expected_result.clone(),
                    input.schedule.clone(),
                    input.policy.clone(),
                    input.template.clone(),
                    now,
                    Some(input.shape),
                )
                .map_err(CommandError::Rejected)?;
        } else if input.name.trim() != existing.name {
            staged
                .rename_automation(id, &input.name, now)
                .map_err(CommandError::Rejected)?;
        }
        Ok(id)
    } else {
        staged
            .create_automation_draft(
                input.project,
                input.shape,
                input.name.clone(),
                input.objective.clone(),
                input.expected_result.clone(),
                input.schedule.clone(),
                input.policy.clone(),
                input.template.clone(),
                now,
            )
            .map_err(CommandError::Rejected)
    }
}

fn apply_edit(staged: &mut Store, edit: &EditCommand) -> Result<CommandOutcome, CommandError> {
    match edit {
        EditCommand::TaskDetails {
            id,
            title,
            description,
            labels,
            due_date,
        } => {
            let task = staged
                .task(*id)
                .ok_or_else(|| CommandError::Rejected("Task not found.".into()))?;
            if task.archived {
                return Err(CommandError::Rejected(
                    "Restore the archived task before editing it.".into(),
                ));
            }
            if title.trim().is_empty() {
                return Err(CommandError::Rejected(
                    "Add a task title before saving.".into(),
                ));
            }
            let before = TaskFields::from_task(task);
            let after = TaskFields {
                title: title.trim().to_string(),
                description: description.clone(),
                labels: labels.clone(),
                cycle: before.cycle.clone(),
                due_date: due_date.trim().to_string(),
            };
            if before == after {
                return Ok(CommandOutcome {
                    subject: None,
                    effects: Vec::new(),
                    undo: UndoDecision::NoOp,
                    feedback: Some(CommandFeedback {
                        title: "No changes".into(),
                        detail: "The task details already match this edit.".into(),
                    }),
                });
            }
            after.apply_to(staged.task_mut(*id).expect("validated task"));
            staged
                .task_mut(*id)
                .expect("validated task")
                .activity
                .push(activity_event("Task details updated"));
            Ok(CommandOutcome {
                subject: None,
                effects: Vec::new(),
                undo: UndoDecision::Record(UndoEntry::TaskFields {
                    id: *id,
                    before,
                    after,
                }),
                feedback: None,
            })
        }
        EditCommand::TaskStage { id, stage } => {
            let task = staged
                .task(*id)
                .ok_or_else(|| CommandError::Rejected("Task not found.".into()))?;
            let before = task.stage;
            let before_approved = task.approved_plan.clone();
            staged
                .set_stage(*id, *stage)
                .map_err(CommandError::Rejected)?;
            let task = staged.task(*id).expect("validated task");
            let after = task.stage;
            let after_approved = task.approved_plan.clone();
            if before == after && before_approved == after_approved {
                return Ok(CommandOutcome {
                    subject: None,
                    effects: Vec::new(),
                    undo: UndoDecision::NoOp,
                    feedback: None,
                });
            }
            Ok(CommandOutcome {
                subject: None,
                effects: Vec::new(),
                undo: UndoDecision::Record(UndoEntry::TaskStage {
                    id: *id,
                    before,
                    before_approved,
                    after,
                    after_approved,
                }),
                feedback: None,
            })
        }
    }
}

fn apply_inverse(staged: &mut Store, entry: &UndoEntry) -> Result<CommandOutcome, CommandError> {
    if !entry.matches_after(staged) {
        return Err(CommandError::Conflict(
            "The target changed after this undo was prepared.".into(),
        ));
    }
    match entry {
        UndoEntry::TaskFields { id, before, .. } => {
            let task = staged
                .task_mut(*id)
                .ok_or_else(|| CommandError::Rejected("Task not found.".into()))?;
            before.apply_to(task);
            task.activity
                .push(activity_event("Undo: task details restored"));
        }
        UndoEntry::TaskStage {
            id,
            before,
            before_approved,
            ..
        } => {
            staged
                .set_stage(*id, *before)
                .map_err(CommandError::Rejected)?;
            let task = staged
                .task_mut(*id)
                .ok_or_else(|| CommandError::Rejected("Task not found.".into()))?;
            task.approved_plan = before_approved.clone();
            task.activity
                .push(activity_event("Undo: task stage restored"));
        }
    }
    Ok(CommandOutcome {
        subject: None,
        effects: Vec::new(),
        undo: UndoDecision::NoOp,
        feedback: Some(CommandFeedback {
            title: "Undone".into(),
            detail: "The named edit was reversed; runtime history was not changed.".into(),
        }),
    })
}

fn apply_dispatch_run(
    staged: &mut Store,
    occurrence: Option<u64>,
    run: AgentRun,
) -> Result<CommandOutcome, CommandError> {
    validate_dispatch_lifecycle(&run)?;
    if staged.runs().iter().any(|existing| existing.id == run.id) {
        return Err(CommandError::Rejected(
            "This run identifier is already in use.".into(),
        ));
    }
    if run.project_plan.is_some() || run.planning_owner.is_some() {
        if occurrence.is_some() {
            return Err(CommandError::Rejected(
                "A planning run cannot carry an automation period link.".into(),
            ));
        }
        let run_id = run.id;
        staged.push_run(run);
        return Ok(CommandOutcome {
            subject: Some(run_id),
            effects: vec![CommandEffect::QueueRun { run_id }],
            undo: UndoDecision::NotUndoable("Submitting a run is not an ordinary edit."),
            feedback: None,
        });
    }
    let task = staged
        .task(run.task_id)
        .ok_or_else(|| CommandError::Rejected("The run's task is not available.".into()))?;
    let research = task.research.as_ref().ok_or_else(|| {
        CommandError::Rejected("A dispatch run must target a research step.".into())
    })?;
    if research.kind == ResearchKind::Goal {
        return Err(CommandError::Rejected(
            "A research goal is not an executable step.".into(),
        ));
    }
    if run.research_kind != Some(research.kind) {
        return Err(CommandError::Rejected(
            "The run's research kind does not match its task.".into(),
        ));
    }

    // The shared eligibility result is the dispatch contract. Do not recreate a
    // second status/approval/dependency gate here.
    let eligibility = staged.execution_eligibility(run.task_id);

    match occurrence {
        Some(id) => {
            let link = staged
                .all_occurrences()
                .iter()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| CommandError::Rejected("Automation period not found.".into()))?;
            // Verifying is a fail-closed recovery state. The shared eligibility
            // already returns Waiting for it; this check gives the caller the
            // precise reason instead of silently treating it as dispatchable.
            if link.status == OccurrenceStatus::Verifying {
                return Err(CommandError::Rejected(
                    "This automation period is checking a previous session; confirm recovery before dispatching."
                        .into(),
                ));
            }
            if run.automation_occurrence_id != Some(id) {
                return Err(CommandError::Rejected(
                    "The dispatch run does not belong to the selected automation period.".into(),
                ));
            }
            if task.automation_id != Some(link.automation_id) {
                return Err(CommandError::Rejected(
                    "The dispatch task belongs to another automation.".into(),
                ));
            }
            let goal = staged.owning_research_goal(run.task_id).ok_or_else(|| {
                CommandError::Rejected("The dispatch task has no research owner.".into())
            })?;
            if !(link.research_goal_id == Some(goal) || link.research_goal_ids.contains(&goal)) {
                return Err(CommandError::Rejected(
                    "The dispatch task belongs to another automation period.".into(),
                ));
            }
            if !eligibility.is_executable_for_preparation() {
                return Err(CommandError::Rejected(
                    "The shared execution eligibility does not allow preparing this step.".into(),
                ));
            }
            match &eligibility.owner {
                EligibilityOwner::AutomationPeriod(owner) if owner.occurrence == Some(id) => {}
                _ => {
                    return Err(CommandError::Rejected(
                        "The automation period ownership is missing or ambiguous.".into(),
                    ));
                }
            }
            // This delegates phase order, dependency readiness and the
            // one-active-attempt rule to the shared selector instead of copying
            // a divergent subset.
            match staged
                .next_occurrence_step(link)
                .map_err(CommandError::Rejected)?
            {
                Some(next) if next == run.task_id => {}
                Some(_) => {
                    return Err(CommandError::Rejected(
                        "This step is not the next eligible step of its automation period.".into(),
                    ));
                }
                None => {
                    return Err(CommandError::Rejected(
                        "No runnable step remains for this automation period.".into(),
                    ));
                }
            }
        }
        None => {
            if run.automation_occurrence_id.is_some() {
                return Err(CommandError::Rejected(
                    "An automation-owned run must name its period.".into(),
                ));
            }
            if task.automation_id.is_some() {
                return Err(CommandError::Rejected(
                    "An automation-owned step cannot use ordinary dispatch.".into(),
                ));
            }
            if !matches!(eligibility.owner, EligibilityOwner::OrdinaryResearch { .. }) {
                return Err(CommandError::Rejected(
                    "The ordinary dispatch target has ambiguous automation ownership.".into(),
                ));
            }
            if eligibility.active_attempt != ActiveAttempt::None
                || eligibility.approval_pending
                || eligibility.goal_barrier.is_some()
                || eligibility.phase_barrier.is_some()
                || !eligibility.unmet_dependencies.is_empty()
            {
                return Err(CommandError::Rejected(
                    "The shared execution eligibility does not allow preparing this step.".into(),
                ));
            }
            match eligibility.outcome {
                StepOutcome::Open => {}
                // An explicit ordinary rerun of a failed step remains valid.
                StepOutcome::Failed => {}
                _ => {
                    return Err(CommandError::Rejected(
                        "This ordinary research step is not open for another attempt.".into(),
                    ));
                }
            }
            if eligibility.decision() != EligibilityDecision::Ready
                && eligibility.outcome != StepOutcome::Failed
            {
                return Err(CommandError::Rejected(
                    "The ordinary research step is waiting or blocked.".into(),
                ));
            }
        }
    }

    let run_id = run.id;
    staged.push_run(run);
    if let Some(id) = occurrence {
        let mut link = staged.occurrence_mut(id).expect("validated occurrence");
        if !link.run_ids.contains(&run_id) {
            link.run_ids.push(run_id);
        }
    }
    Ok(CommandOutcome {
        subject: None,
        effects: vec![CommandEffect::QueueRun { run_id }],
        undo: UndoDecision::NotUndoable("Submitting a run is not an ordinary edit."),
        feedback: None,
    })
}

fn apply_dispatch_request_plan(
    staged: &mut Store,
    request_id: u64,
    run: AgentRun,
) -> Result<CommandOutcome, CommandError> {
    validate_dispatch_lifecycle(&run)?;
    let request = staged
        .research_requests()
        .iter()
        .find(|request| request.id == request_id)
        .ok_or_else(|| CommandError::Rejected("The planning request is no longer available.".into()))?;
    let owner = run.planning_owner.as_ref().ok_or_else(|| {
        CommandError::Rejected("The planning run has no immutable request owner.".into())
    })?;
    if owner.request_id != request_id || owner.revision != request.revision {
        return Err(CommandError::Rejected(
            "The planning run belongs to an older request revision.".into(),
        ));
    }
    if staged.request_is_archived(request) {
        return Err(CommandError::Rejected(
            "Restore the archived topic before planning.".into(),
        ));
    }
    if !staged
        .projects()
        .iter()
        .any(|project| project.id == request.project_id && !project.directory.is_empty())
    {
        return Err(CommandError::Rejected(
            "The planning project is no longer available.".into(),
        ));
    }
    if staged.request_attempts(request_id).iter().any(|attempt| {
        matches!(attempt.status.as_str(), "Queued" | "Running" | "Cancelling")
    }) {
        return Err(CommandError::Rejected(
            "This request already has an active planning attempt.".into(),
        ));
    }
    let signature = staged
        .request_plan_signature(request_id)
        .map_err(CommandError::Rejected)?;
    if run.input_signature != signature {
        return Err(CommandError::Rejected(
            "The planning request inputs changed before dispatch; generate the plan again."
                .into(),
        ));
    }
    if let Some(automation) = request.automation_id
        && staged
            .latest_planning_request(automation)
            .is_none_or(|latest| latest.id != request_id)
    {
        return Err(CommandError::Rejected(
            "This planning request is no longer the automation's current revision.".into(),
        ));
    }
    if staged.runs().iter().any(|existing| existing.id == run.id) {
        return Err(CommandError::Rejected(
            "This run identifier is already in use.".into(),
        ));
    }
    let run_id = run.id;
    staged.push_run(run);
    if let Some(request) = staged.request_mut(request_id)
    {
        request.planning_run_id = Some(run_id);
    }
    Ok(CommandOutcome {
        subject: Some(run_id),
        effects: vec![CommandEffect::QueueRun { run_id }],
        undo: UndoDecision::NotUndoable("Submitting a run is not an ordinary edit."),
        feedback: None,
    })
}

fn apply_dispatch_regeneration(
    staged: &mut Store,
    occurrence: Option<u64>,
    source_id: u64,
    _style: &str,
    run: AgentRun,
) -> Result<CommandOutcome, CommandError> {
    validate_dispatch_lifecycle(&run)?;
    if staged.runs().iter().any(|existing| existing.id == run.id) {
        return Err(CommandError::Rejected(
            "This run identifier is already in use.".into(),
        ));
    }
    let source = staged
        .runs()
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| CommandError::Rejected("The report source is no longer available.".into()))?;
    if source.status != "Succeeded"
        || source.research_kind != Some(ResearchKind::Advise)
        || source.output.trim().is_empty()
    {
        return Err(CommandError::Rejected(
            "Choose a completed final report to regenerate.".into(),
        ));
    }
    if source.task_id != run.task_id {
        return Err(CommandError::Rejected(
            "The regeneration run does not match its report source.".into(),
        ));
    }
    if staged.research_signature(source.task_id).as_ref() != Some(&source.input_signature) {
        return Err(CommandError::Rejected(
            "The report inputs changed; complete the updated research before regenerating."
                .into(),
        ));
    }
    if staged.research_pending_approval(source.task_id) {
        return Err(CommandError::Rejected(
            "Approve the current research plan before regenerating its report.".into(),
        ));
    }
    let task = staged
        .task(run.task_id)
        .ok_or_else(|| CommandError::Rejected("The regeneration task is unavailable.".into()))?;
    if staged.task_is_archived(run.task_id) || task.archived {
        return Err(CommandError::Rejected(
            "Restore the archived task or topic before regenerating its report.".into(),
        ));
    }
    if staged.runs().iter().any(|existing| {
        existing.task_id == run.task_id
            && existing.project_plan == run.project_plan
            && matches!(
                existing.status.as_str(),
                "Queued" | "Running" | "Cancelling" | "Recovering"
            )
    }) {
        return Err(CommandError::Rejected(
            "This task already has an active run.".into(),
        ));
    }
    if let Some(occurrence_id) = occurrence {
        let occurrence = staged
            .occurrence(occurrence_id)
            .ok_or_else(|| CommandError::Rejected("Automation period not found.".into()))?;
        if run.automation_occurrence_id != Some(occurrence_id)
            || task.automation_id != Some(occurrence.automation_id)
            || occurrence.snapshot.approval_signature.trim().is_empty()
        {
            return Err(CommandError::Rejected(
                "The regeneration run does not belong to this automation period.".into(),
            ));
        }
        let goal = staged
            .owning_research_goal(run.task_id)
            .ok_or_else(|| CommandError::Rejected("The regeneration task has no goal.".into()))?;
        if !(occurrence.research_goal_id == Some(goal)
            || occurrence.research_goal_ids.contains(&goal))
        {
            return Err(CommandError::Rejected(
                "The regeneration task belongs to another automation period.".into(),
            ));
        }
    } else if run.automation_occurrence_id.is_some() {
        return Err(CommandError::Rejected(
            "An ordinary regeneration cannot carry an automation period link.".into(),
        ));
    }
    let run_id = run.id;
    staged.push_run(run);
    if let Some(occurrence_id) = occurrence
        && let Some(mut value) = staged.occurrence_mut(occurrence_id)
        && !value.run_ids.contains(&run_id)
    {
        value.run_ids.push(run_id);
    }
    Ok(CommandOutcome {
        subject: Some(run_id),
        effects: vec![CommandEffect::QueueRun { run_id }],
        undo: UndoDecision::NotUndoable("Submitting a run is not an ordinary edit."),
        feedback: None,
    })
}

fn apply_requeue_run(
    staged: &mut Store,
    run_id: u64,
    occurrence_id: u64,
    now: i64,
) -> Result<CommandOutcome, CommandError> {
    let occurrence = staged
        .occurrence(occurrence_id)
        .ok_or_else(|| CommandError::Rejected("Automation period not found.".into()))?
        .clone();
    if !matches!(
        occurrence.status,
        OccurrenceStatus::Queued | OccurrenceStatus::Running | OccurrenceStatus::WaitingRetry
    ) {
        return Err(CommandError::Rejected(
            "This automation period is not active for the committed run.".into(),
        ));
    }
    if occurrence.first_start_expired(now) {
        return Err(CommandError::Rejected(
            "The committed run is older than the allowed start delay.".into(),
        ));
    }
    if occurrence.deadline_at.is_some_and(|deadline| now >= deadline) {
        return Err(CommandError::Rejected(
            "The committed run has reached its execution time limit.".into(),
        ));
    }
    if !occurrence.run_ids.contains(&run_id) {
        return Err(CommandError::Rejected(
            "The committed run is not linked to this automation period.".into(),
        ));
    }
    let run = staged
        .runs()
        .iter()
        .find(|run| run.id == run_id)
        .cloned()
        .ok_or_else(|| CommandError::Rejected("The committed run is missing.".into()))?;
    if run.automation_occurrence_id != Some(occurrence_id)
        || !matches!(run.status.as_str(), "Queued" | "Interrupted")
        || !run.started_at.is_empty()
    {
        return Err(CommandError::Rejected(
            "The committed run is not an unstarted queued intent.".into(),
        ));
    }
    let task = staged
        .task(run.task_id)
        .ok_or_else(|| CommandError::Rejected("The committed run's task is missing.".into()))?;
    if task.automation_id != Some(occurrence.automation_id) {
        return Err(CommandError::Rejected(
            "The committed run's task belongs to another automation.".into(),
        ));
    }
    let goal = staged
        .owning_research_goal(run.task_id)
        .ok_or_else(|| CommandError::Rejected("The committed run's task has no goal.".into()))?;
    if !(occurrence.research_goal_id == Some(goal)
        || occurrence.research_goal_ids.contains(&goal))
    {
        return Err(CommandError::Rejected(
            "The committed run's task belongs to another automation period.".into(),
        ));
    }
    // Re-evaluate the shared T05 contract with the committed run removed. The
    // run itself must not make its own step look Failed/blocked.
    let mut probe = staged.clone();
    probe.retain_runs(|candidate| candidate.id != run_id);
    let probe_occurrence = probe
        .occurrence(occurrence_id)
        .ok_or_else(|| CommandError::Rejected("Automation period not found.".into()))?;
    if !probe
        .execution_eligibility(run.task_id)
        .is_executable_for_preparation()
    {
        return Err(CommandError::Rejected(
            "The shared execution eligibility no longer allows this committed step.".into(),
        ));
    }
    match probe
        .next_occurrence_step(probe_occurrence)
        .map_err(CommandError::Rejected)?
    {
        Some(next) if next == run.task_id => {}
        Some(_) => {
            return Err(CommandError::Rejected(
                "The committed run is not the next eligible step of its period.".into(),
            ));
        }
        None => {
            return Err(CommandError::Rejected(
                "No runnable step remains for this automation period.".into(),
            ));
        }
    }
    let mut run = staged.run_mut(run_id).expect("validated committed run");
    run.status = "Queued".into();
    run.queued_at = crate::opencode::timestamp();
    run.ended_at = None;
    run.exit_code = None;
    Ok(CommandOutcome {
        subject: Some(run_id),
        effects: vec![CommandEffect::QueueRun { run_id }],
        undo: UndoDecision::NotUndoable("Queueing a committed run is not an ordinary edit."),
        feedback: None,
    })
}

fn validate_dispatch_lifecycle(run: &AgentRun) -> Result<(), CommandError> {
    if run.id == 0 {
        return Err(CommandError::Rejected(
            "A dispatch run must already have an identifier.".into(),
        ));
    }
    if run.status != "Queued" {
        return Err(CommandError::Rejected(
            "A dispatch run must be prepared into the queued lifecycle state.".into(),
        ));
    }
    if run.queued_at.trim().is_empty()
        || !run.started_at.is_empty()
        || run.ended_at.is_some()
        || run.exit_code.is_some()
    {
        return Err(CommandError::Rejected(
            "The dispatch run has an inconsistent queued lifecycle.".into(),
        ));
    }
    Ok(())
}

/// An ordinary edit or its inverse may change only the named task fields. Runs,
/// automation approvals/cursors/occurrences, planning requests, publications and
/// automation-owned entities are protected.
pub(crate) fn assert_ordinary_edit_preserved(
    before: &Store,
    after: &Store,
) -> Result<(), CommandError> {
    if before.automations() != after.automations()
        || before.all_occurrences() != after.all_occurrences()
        || before.runs() != after.runs()
        || before.research_requests() != after.research_requests()
        || before.publications() != after.publications()
        || before.report_publishing() != after.report_publishing()
        || automation_projects(before) != automation_projects(after)
        || automation_tasks(before) != automation_tasks(after)
        || automation_topics(before) != automation_topics(after)
    {
        return Err(CommandError::Conflict(
            "This edit would change protected automation, run, approval or history state.".into(),
        ));
    }
    if after.next_id() < before.next_id() {
        return Err(CommandError::Conflict(
            "An ordinary edit may not decrease the workspace identifier sequence.".into(),
        ));
    }
    Ok(())
}

fn automation_projects(store: &Store) -> Vec<&crate::domain::Project> {
    store
        .projects()
        .iter()
        .filter(|project| project.kind == crate::domain::ProjectKind::Automation)
        .collect()
}

fn automation_tasks(store: &Store) -> Vec<&Task> {
    store
        .tasks()
        .iter()
        .filter(|task| task.automation_id.is_some())
        .collect()
}

fn automation_topics(store: &Store) -> Vec<&crate::domain::Topic> {
    store
        .topics()
        .iter()
        .filter(|topic| topic.automation_id.is_some())
        .collect()
}

pub(crate) trait CommitSink {
    fn save(&self, store: &Store) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandReceipt {
    pub subject: Option<u64>,
    pub effects: Vec<CommandEffect>,
    pub undo: UndoDecision,
    pub feedback: Option<CommandFeedback>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommitError {
    Stale {
        message: String,
        pending: PreparedCommand,
    },
    NotDurable {
        message: String,
        pending: PreparedCommand,
    },
}

impl CommitError {
    pub(crate) fn pending(&self) -> &PreparedCommand {
        match self {
            Self::Stale { pending, .. } | Self::NotDurable { pending, .. } => pending,
        }
    }
}

/// Pure commit guard. It never assigns a stale staged store and never returns a
/// receipt before the sink reports success. The caller executes effects only from
/// a successful receipt.
///
/// S0 is synchronous: the caller holds `&mut live` for the whole prepare/save/
/// assign sequence, and `CommitSink::save` cannot mutate `live`. That exclusive
/// borrow is the actual race protection here. A second check after the write is
/// only defense against a reentrant or buggy sink; it is **not** a guarantee for
/// a pipelined/background writer, because a stale disk write cannot be undone
/// after the fact. T14 must validate the generation and reject a stale commit
/// **before** writing to disk, then order acknowledgements.
pub(crate) fn commit_prepared(
    sink: &dyn CommitSink,
    live: &mut Store,
    last_saved: &mut Store,
    prepared: PreparedCommand,
) -> Result<CommandReceipt, CommitError> {
    if !prepared.base.matches(live) {
        return Err(CommitError::Stale {
            message: "The workspace changed after this command was prepared.".into(),
            pending: prepared,
        });
    }
    if let Err(message) = sink.save(&prepared.staged) {
        return Err(CommitError::NotDurable {
            message,
            pending: prepared,
        });
    }
    // S0 only: the exclusive `&mut` borrow means no background mutation can
    // interleave with the synchronous save. This re-check guards reentrancy, not
    // a concurrent writer. T14 must reject stale generations before disk commit;
    // post-write rejection cannot repair a durable lost update.
    if !prepared.base.matches(live) {
        return Err(CommitError::Stale {
            message: "The workspace changed while this command was being saved.".into(),
            pending: prepared,
        });
    }
    let PreparedCommand {
        staged, outcome, ..
    } = prepared;
    *live = staged;
    *last_saved = live.clone();
    Ok(CommandReceipt {
        subject: outcome.subject,
        effects: outcome.effects,
        undo: outcome.undo,
        feedback: outcome.feedback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationOccurrence, TimeWindow, TriggerSource, WindowRule};
    use crate::automation_schedule::ScheduleSpec;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemorySink {
        writes: RefCell<Vec<Store>>,
        fail: bool,
    }

    impl CommitSink for MemorySink {
        fn save(&self, store: &Store) -> Result<(), String> {
            if self.fail {
                return Err("injected write failure".into());
            }
            self.writes.borrow_mut().push(store.clone());
            Ok(())
        }
    }

    fn task_details_command(id: u64, title: &str) -> Command {
        Command::Edit(EditCommand::TaskDetails {
            id,
            title: title.into(),
            description: "updated description".into(),
            labels: vec!["Updated".into()],
            due_date: "2026-11-09".into(),
        })
    }

    fn stage_command(id: u64, stage: Stage) -> Command {
        Command::Edit(EditCommand::TaskStage { id, stage })
    }

    fn occurrence(id: u64, automation: u64) -> AutomationOccurrence {
        let now = 1_000;
        AutomationOccurrence {
            id,
            automation_id: automation,
            definition_version: 1,
            source: TriggerSource::Manual,
            trigger_key: format!("manual:{id}"),
            scheduled_for: None,
            requested_at: Some(now),
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: now,
            data_window: TimeWindow::unresolved("test"),
            snapshot: crate::automation::AutomationOccurrenceSnapshot {
                objective: "test".into(),
                expected_result: "test".into(),
                shape: Some(AutomationShape::Quick),
                template: AutomationTemplate {
                    goals: Vec::new(),
                    window_rule: WindowRule::Unresolved,
                },
                models: [
                    "test/model".into(),
                    "test/model".into(),
                    "test/model".into(),
                    "test/model".into(),
                ],
                agents: [None, None, None, None],
                materials: Vec::new(),
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "test-signature".into(),
            },
            status: OccurrenceStatus::Queued,
            reason: None,
            research_goal_id: None,
            research_goal_ids: Vec::new(),
            research_topic_id: None,
            run_ids: Vec::new(),
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: now,
            deadline_at: None,
            retry_requested_at: None,
        }
    }

    fn automation_definition(id: u64, now: i64) -> crate::automation::AutomationDefinition {
        crate::automation::AutomationDefinition {
            id,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Quick,
            name: "Paused by test".into(),
            objective: "Check a value".into(),
            expected_result: "One answer".into(),
            schedule: Schedule {
                spec: ScheduleSpec::Interval { minutes: 60 },
                timezone: "UTC".into(),
            },
            status: DefinitionStatus::Enabled,
            template: None,
            plan_binding: None,
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: Some(now + 60),
            anchor: Some(now),
            consecutive_failures: 0,
            attention_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    const PLAN: &str = r#"{"tasks":[
        {"title":"Collect one","instructions":"Collect one.","kind":"collect","depends_on":[]},
        {"title":"Analyze","instructions":"Analyze.","kind":"analyze","depends_on":[0]},
        {"title":"Report","instructions":"Report.","kind":"advise","depends_on":[1]}
    ]}"#;

    fn fixture_store() -> Store {
        let mut store = Store::empty();
        store.set_default_model("test/model".into());
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.research_config_mut().models = [
            "test/model".into(),
            "test/model".into(),
            "test/model".into(),
            "test/model".into(),
        ];
        store
    }

    fn mark_queued(run: &mut AgentRun) {
        run.status = "Queued".into();
        run.queued_at = "2026-11-08T00:00:00Z".into();
        run.started_at.clear();
        run.ended_at = None;
        run.exit_code = None;
    }

    fn automation_fixture() -> (Store, AutomationOccurrence, AgentRun) {
        let mut store = fixture_store();
        let topic = store.create_topic(1, "Automation topic").unwrap();
        let goal = store
            .create_research_goal_in_topic(
                "Automation goal".into(),
                "Check the fixture.".into(),
                1,
                Some(topic),
            )
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        store.task_mut(goal).unwrap().automation_id = Some(1);
        for step in &steps {
            store.task_mut(*step).unwrap().automation_id = Some(1);
        }
        let mut occ = occurrence(10, 1);
        occ.research_goal_id = Some(goal);
        occ.research_goal_ids = vec![goal];
        occ.research_topic_id = Some(topic);
        store.push_occurrence(occ.clone());
        let mut run = store.prepare_occurrence_run(&occ, 1_000).unwrap();
        mark_queued(&mut run);
        (store, occ, run)
    }

    fn ordinary_fixture() -> (Store, AgentRun) {
        let mut store = fixture_store();
        let topic = store.create_topic(1, "Ordinary topic").unwrap();
        let goal = store
            .create_research_goal_in_topic(
                "Ordinary goal".into(),
                "Check the ordinary fixture.".into(),
                1,
                Some(topic),
            )
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        let mut run = store.prepare_run(steps[0]).unwrap();
        mark_queued(&mut run);
        (store, run)
    }

    fn dispatch_command(occurrence: Option<u64>, run: AgentRun) -> Command {
        Command::DispatchRun { occurrence, run }
    }

    fn prepare_dispatch(
        store: &Store,
        occurrence: Option<u64>,
        run: AgentRun,
    ) -> Result<PreparedCommand, CommandError> {
        let command = dispatch_command(occurrence, run);
        store.prepare_command(command.id(), CommandClass::StartOrExtend, command)
    }

    #[test]
    fn prepared_edit_leaves_the_original_store_untouched() {
        let store = Store::mock();
        let before = store.clone();
        let prepared = store
            .prepare_command(
                task_details_command(1, "Edited title").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Edited title"),
            )
            .unwrap();

        assert_eq!(store, before, "prepare must not mutate the original");
        assert_eq!(prepared.staged.task(1).unwrap().title, "Edited title");
        match prepared.outcome.undo {
            UndoDecision::Record(UndoEntry::TaskFields { before, after, .. }) => {
                assert_ne!(before.title, after.title);
            }
            other => panic!("expected a task-fields inverse, got {other:?}"),
        }
    }

    #[test]
    fn task_details_undo_round_trips_and_preserves_history() {
        let mut live = Store::mock();
        live.simulate_run(1).unwrap();
        let history = live.runs().to_vec();
        let sink = MemorySink::default();
        let mut last_saved = live.clone();

        let prepared = live
            .prepare_command(
                task_details_command(1, "Edited title").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Edited title"),
            )
            .unwrap();
        let receipt = commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap();
        let UndoDecision::Record(entry) = receipt.undo else {
            panic!("expected a recorded inverse");
        };
        assert_eq!(live.runs(), history, "ordinary edit must not change runs");
        assert_eq!(live.task(1).unwrap().title, "Edited title");

        let prepared_undo = live.prepare_undo(entry).unwrap();
        let receipt = commit_prepared(&sink, &mut live, &mut last_saved, prepared_undo).unwrap();
        assert_eq!(receipt.undo, UndoDecision::NoOp);
        assert_eq!(
            live.task(1).unwrap().title,
            "Polish task details and navigation"
        );
        assert_eq!(live.runs(), history, "undo must not change run history");
        assert!(
            live.task(1)
                .unwrap()
                .activity
                .last()
                .is_some_and(|event| event.contains("Undo: task details restored")),
            "undo appends an event rather than truncating history"
        );
        assert_eq!(sink.writes.borrow().len(), 2);
    }

    #[test]
    fn task_stage_inverse_restores_approval_without_touching_runs() {
        let mut live = Store::mock();
        let goal = live
            .create_research_goal("Goal".into(), "Brief".into(), 1)
            .unwrap();
        live.set_stage(goal, Stage::Planned).unwrap();
        live.simulate_run(1).unwrap();
        let history = live.runs().to_vec();
        let before_stage = live.task(goal).unwrap().stage;
        let before_approved = live.task(goal).unwrap().approved_plan.clone();
        assert!(before_approved.is_some(), "planned research is approved");

        let sink = MemorySink::default();
        let mut last_saved = live.clone();
        let prepared = live
            .prepare_command(
                stage_command(goal, Stage::Backlog).id(),
                CommandClass::OrdinaryEdit,
                stage_command(goal, Stage::Backlog),
            )
            .unwrap();
        let receipt = commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap();
        let UndoDecision::Record(entry) = receipt.undo else {
            panic!("expected a recorded inverse");
        };
        assert_eq!(live.task(goal).unwrap().approved_plan, None);

        let prepared_undo = live.prepare_undo(entry).unwrap();
        commit_prepared(&sink, &mut live, &mut last_saved, prepared_undo).unwrap();
        assert_eq!(live.task(goal).unwrap().stage, before_stage);
        assert_eq!(live.task(goal).unwrap().approved_plan, before_approved);
        assert_eq!(live.runs(), history, "stage undo must not change runs");
    }

    #[test]
    fn noop_stage_returns_noop_and_does_not_rewrite_fields() {
        let store = Store::mock();
        let stage = store.task(1).unwrap().stage;
        let prepared = store
            .prepare_command(
                stage_command(1, stage).id(),
                CommandClass::OrdinaryEdit,
                stage_command(1, stage),
            )
            .unwrap();
        assert_eq!(prepared.outcome.undo, UndoDecision::NoOp);
        assert_eq!(
            TaskFields::from_task(prepared.staged.task(1).unwrap()),
            TaskFields::from_task(store.task(1).unwrap())
        );
    }

    #[test]
    fn undo_conflicts_when_the_target_changed_after_preparation() {
        let mut live = Store::mock();
        let sink = MemorySink::default();
        let mut last_saved = live.clone();
        let prepared = live
            .prepare_command(
                task_details_command(1, "First edit").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "First edit"),
            )
            .unwrap();
        let receipt = commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap();
        let UndoDecision::Record(entry) = receipt.undo else {
            panic!("expected a recorded inverse");
        };
        let pending = live.prepare_undo(entry).unwrap();

        live.task_mut(1).unwrap().title = "A newer edit".into();
        let before = live.clone();
        let error = reprepare(pending, &live).unwrap_err();
        assert!(matches!(error, CommandError::Conflict(_)));
        assert_eq!(live, before, "conflict must not assign or mutate");
        assert_eq!(live.task(1).unwrap().title, "A newer edit");
    }

    #[test]
    fn stale_prepared_store_is_rejected_after_a_background_completion() {
        let mut live = Store::mock();
        let mut last_saved = live.clone();
        let sink = MemorySink::default();
        let prepared = live
            .prepare_command(
                task_details_command(1, "Queued edit").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Queued edit"),
            )
            .unwrap();
        // Simulate a completion/history change between prepare and commit.
        live.simulate_run(1).unwrap();
        let after_completion = live.clone();

        let error = commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap_err();
        assert!(matches!(error, CommitError::Stale { .. }));
        assert_eq!(
            live, after_completion,
            "stale commit must not overwrite newer state"
        );
        assert_eq!(
            live.task(1).unwrap().title,
            "Polish task details and navigation",
            "the stale edit was not applied"
        );
        assert!(
            sink.writes.borrow().is_empty(),
            "no write may happen on stale base"
        );
        assert_eq!(live.runs().len(), 1, "the new completion survives");
    }

    #[test]
    fn reprepare_keeps_newer_history_and_applies_the_same_typed_edit() {
        let mut live = Store::mock();
        let mut last_saved = live.clone();
        let sink = MemorySink::default();
        let pending = live
            .prepare_command(
                task_details_command(1, "Queued edit").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Queued edit"),
            )
            .unwrap();
        live.simulate_run(1).unwrap();
        let history = live.runs().to_vec();

        let fresh = reprepare(pending, &live).unwrap();
        commit_prepared(&sink, &mut live, &mut last_saved, fresh).unwrap();
        assert_eq!(live.task(1).unwrap().title, "Queued edit");
        assert_eq!(live.runs(), history, "reprepare must not drop the newer run");
    }

    #[test]
    fn failing_sink_returns_pending_without_changing_live_state() {
        let mut live = Store::mock();
        let mut last_saved = live.clone();
        let sink = MemorySink {
            fail: true,
            ..MemorySink::default()
        };
        let prepared = live
            .prepare_command(
                task_details_command(1, "Must not apply").id(),
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Must not apply"),
            )
            .unwrap();
        let before = live.clone();
        let error = commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap_err();
        assert!(matches!(error, CommitError::NotDurable { .. }));
        assert_eq!(live, before);
        assert_eq!(last_saved, before);
        assert_eq!(error.pending().id.kind, "task_details");
    }

    #[test]
    fn protected_history_guard_rejects_a_changed_run() {
        let mut store = Store::mock();
        store.simulate_run(1).unwrap();
        let before = store.clone();
        store.run_at_mut(0).unwrap().status = "Succeeded".into();
        assert!(matches!(
            assert_ordinary_edit_preserved(&before, &store),
            Err(CommandError::Conflict(_))
        ));
    }

    #[test]
    fn dispatch_stages_run_and_occurrence_link_in_one_store() {
        let (store, occ, run) = automation_fixture();
        let history = store.runs().to_vec();
        let prepared = prepare_dispatch(&store, Some(occ.id), run.clone()).unwrap();

        assert_eq!(store.runs(), history, "prepare must not mutate the original");
        assert_eq!(prepared.staged.runs().len(), history.len() + 1);
        assert_eq!(
            prepared.staged.occurrence(occ.id).unwrap().run_ids,
            vec![run.id],
            "the run link is staged with the run"
        );
        assert_eq!(
            prepared.outcome.effects,
            vec![CommandEffect::QueueRun { run_id: run.id }]
        );

        let sink = MemorySink::default();
        let mut live = store.clone();
        let mut last_saved = live.clone();
        commit_prepared(&sink, &mut live, &mut last_saved, prepared).unwrap();
        assert_eq!(live.occurrence(occ.id).unwrap().run_ids, vec![run.id]);
        assert_eq!(sink.writes.borrow().len(), 1, "one dispatch write");
    }

    #[test]
    fn dispatch_rejects_verifying_periods() {
        let (mut store, occ, run) = automation_fixture();
        store.occurrence_mut(occ.id).unwrap().status = OccurrenceStatus::Verifying;
        let error = prepare_dispatch(&store, Some(occ.id), run).unwrap_err();
        assert!(
            matches!(error, CommandError::Rejected(ref message) if message.contains("checking a previous session")),
            "{error:?}"
        );
    }

    #[test]
    fn dispatch_rejects_none_occurrence_with_automation_owned_run() {
        let (store, _occ, run) = automation_fixture();
        let error = prepare_dispatch(&store, None, run).unwrap_err();
        assert!(
            matches!(error, CommandError::Rejected(ref message) if message.contains("must name its period")),
            "{error:?}"
        );
    }

    #[test]
    fn dispatch_rejects_missing_occurrence_link() {
        let (store, occ, mut run) = automation_fixture();
        run.automation_occurrence_id = None;
        let error = prepare_dispatch(&store, Some(occ.id), run).unwrap_err();
        assert!(
            matches!(error, CommandError::Rejected(ref message) if message.contains("does not belong")),
            "{error:?}"
        );
    }

    #[test]
    fn dispatch_rejects_a_task_from_another_period_or_automation() {
        let (mut store, occ, mut run) = automation_fixture();
        let other_topic = store.create_topic(1, "Other automation topic").unwrap();
        let other_goal = store
            .create_research_goal_in_topic(
                "Other automation goal".into(),
                "A foreign period.".into(),
                1,
                Some(other_topic),
            )
            .unwrap();
        let other_steps = store.apply_research_plan(other_goal, PLAN).unwrap();
        store.task_mut(other_goal).unwrap().automation_id = Some(1);
        for step in &other_steps {
            store.task_mut(*step).unwrap().automation_id = Some(1);
        }
        run.task_id = other_steps[0];
        let error = prepare_dispatch(&store, Some(occ.id), run).unwrap_err();
        assert!(
            matches!(error, CommandError::Rejected(ref message) if message.contains("another automation period")),
            "{error:?}"
        );
    }

    #[test]
    fn dispatch_rejects_an_incoherent_queued_lifecycle() {
        let (store, occ, mut run) = automation_fixture();
        run.status = "Running".into();
        let error = prepare_dispatch(&store, Some(occ.id), run).unwrap_err();
        assert!(
            matches!(error, CommandError::Rejected(ref message) if message.contains("queued lifecycle") || message.contains("queued")),
            "{error:?}"
        );
    }

    #[test]
    fn dispatch_accepts_ordinary_research_without_an_occurrence_link() {
        let (store, run) = ordinary_fixture();
        let before = store.runs().len();
        let prepared = prepare_dispatch(&store, None, run.clone()).unwrap();
        assert_eq!(prepared.staged.runs().len(), before + 1);
        assert_eq!(
            prepared
                .staged
                .runs()
                .last()
                .unwrap()
                .automation_occurrence_id,
            None
        );
        assert_eq!(
            prepared.outcome.effects,
            vec![CommandEffect::QueueRun { run_id: run.id }]
        );
        assert!(prepared.staged.all_occurrences().is_empty());
    }

    #[test]
    fn dispatch_accepts_a_reopened_retry_period() {
        let (mut store, occ, mut run) = automation_fixture();
        let mut failed = run.clone();
        failed.status = "Failed".into();
        failed.started_at = "2026-11-08T00:00:00Z".into();
        failed.ended_at = Some("2026-11-08T00:01:00Z".into());
        store.push_run(failed);
        store.occurrence_mut(occ.id).unwrap().retry_requested_at = Some(1_000);
        run = store.prepare_occurrence_run(&occ, 1_000).unwrap();
        mark_queued(&mut run);

        let prepared = prepare_dispatch(&store, Some(occ.id), run.clone()).unwrap();
        assert_eq!(
            prepared.staged.occurrence(occ.id).unwrap().run_ids,
            vec![run.id]
        );
        assert_eq!(prepared.staged.runs().last().unwrap().status, "Queued");
    }

    #[test]
    fn dispatch_reprepare_conflicts_instead_of_reusing_a_stale_run_id() {
        let (mut store, occ, run) = automation_fixture();
        let pending = prepare_dispatch(&store, Some(occ.id), run.clone()).unwrap();
        // A newer dispatch took the same identifier.
        let mut newer = run;
        newer.status = "Queued".into();
        store.push_run(newer);

        let error = reprepare(pending, &store).unwrap_err();
        assert!(matches!(error, CommandError::Conflict(_)));
    }

    #[test]
    fn automation_pause_prepares_through_the_existing_store_command() {
        let mut store = Store::mock();
        let now = 1_000;
        store.push_automation(automation_definition(1, now));
        let before = store.clone();
        let prepared = store
            .prepare_command(
                Command::AutomationPause { id: 1, now }.id(),
                CommandClass::StartOrExtend,
                Command::AutomationPause { id: 1, now },
            )
            .unwrap();
        assert_eq!(store, before, "prepare must not mutate the original");
        assert_eq!(
            prepared.staged.automation(1).unwrap().status,
            DefinitionStatus::Paused
        );
        assert_eq!(prepared.staged.automation(1).unwrap().cursor, None);
        assert_eq!(
            prepared.outcome.undo,
            UndoDecision::NotUndoable(
                "Automation lifecycle changes are saved as a separate operation."
            )
        );
    }

    #[test]
    fn stop_outcome_never_claims_durability_without_a_saved_record() {
        let intent = StopIntent {
            occurrence: 10,
            run_ids: vec![2],
            requested_at: 1_000,
            reason: "user stop".into(),
        };
        let requested = StopOutcome::StopRequestedNotDurable {
            intent: intent.clone(),
            durability: StopDurability::NotSaved {
                message: "workspace busy".into(),
            },
        };
        assert!(!requested.is_durably_saved());
        let stopped = StopOutcome::Stopped {
            durability: StopDurability::Saved,
        };
        assert!(stopped.is_durably_saved());
        let failed = StopOutcome::StopRequestFailed {
            intent,
            error: "session unknown".into(),
        };
        assert!(!failed.is_durably_saved());
    }

    #[test]
    fn command_identity_must_match_its_target() {
        let store = Store::mock();
        let wrong = CommandId::new("wrong", 99, "wrong");
        let err = store
            .prepare_command(
                wrong,
                CommandClass::OrdinaryEdit,
                task_details_command(1, "Edit"),
            )
            .unwrap_err();
        assert!(matches!(err, CommandError::Rejected(_)));
    }
}
