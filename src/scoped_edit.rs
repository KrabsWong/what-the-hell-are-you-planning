//! T13 S3a: pure scoped task-edit preparation, atomic application, and
//! field-conditional undo.
//!
//! This module has no GPUI and performs no persistence. It creates a typed plan
//! against the current Store, reapplies only after revalidating the exact fields,
//! approval context, runtime evidence and T21 impact it owns, and records a
//! conditional inverse. The live command executor/UI will consume these
//! primitives in S3b; this slice does not replace them or remove the accepted
//! T04 guard.
//!
//! Scope: this is the task-detail editor payload. It carries the requested
//! project so UI intent is never silently ignored, but a differing project is
//! rejected through the existing topic/project-move policy. A dedicated scoped
//! project/topic context command remains a later S3 slice.
//!
//! Performance seam: `research_input::preview_task_edit` still clones the whole
//! Store internally when it builds its candidate. S3a does not add another
//! whole-Store snapshot, but prepare/apply pay that transitive cost. Replace it
//! with a scoped T21 preview before T14 performance acceptance; do not duplicate
//! the signatures here.

use crate::automation::{DefinitionStatus, OccurrenceReason, OccurrenceStatus};
use crate::domain::{activity_event, AgentRun, Priority, Stage, Store, Task};
use crate::research::ResearchKind;
use crate::research_input::{self, ResearchInputImpact, TaskEditIntent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StageIntent {
    Editor,
    Manual,
    Reopen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TaskEditField {
    Title,
    Description,
    Labels,
    DueDate,
    Cycle,
    Priority,
    Bucket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TaskEditValue {
    Text(String),
    Labels(Vec<String>),
    OptionalText(Option<String>),
    Priority(Priority),
}

/// One exact changed field. Construction and values are private; only checked
/// field/value pairs can exist, so apply/undo never silently ignore a malformed
/// payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskFieldChange {
    field: TaskEditField,
    before: TaskEditValue,
    after: TaskEditValue,
}

impl TaskFieldChange {
    fn checked(field: TaskEditField, before: TaskEditValue, after: TaskEditValue) -> Option<Self> {
        field.accepts(&before).then_some(())?;
        field.accepts(&after).then_some(())?;
        Some(Self {
            field,
            before,
            after,
        })
    }

    pub(crate) fn field(&self) -> &TaskEditField {
        &self.field
    }

    pub(crate) fn before(&self) -> &TaskEditValue {
        &self.before
    }

    pub(crate) fn after(&self) -> &TaskEditValue {
        &self.after
    }

    fn matches_before(&self, task: &Task) -> bool {
        value_for(task, &self.field) == self.before
    }

    fn matches_after(&self, task: &Task) -> bool {
        value_for(task, &self.field) == self.after
    }

    fn apply(&self, task: &mut Task, value: &TaskEditValue) {
        match (&self.field, value) {
            (TaskEditField::Title, TaskEditValue::Text(value)) => task.title = value.clone(),
            (TaskEditField::Description, TaskEditValue::Text(value)) => {
                task.description = value.clone()
            }
            (TaskEditField::Labels, TaskEditValue::Labels(value)) => {
                task.labels = value.clone()
            }
            (TaskEditField::DueDate, TaskEditValue::Text(value)) => {
                task.due_date = value.clone()
            }
            (TaskEditField::Cycle, TaskEditValue::OptionalText(value)) => {
                task.cycle = value.clone()
            }
            (TaskEditField::Priority, TaskEditValue::Priority(value)) => {
                task.priority = *value
            }
            (TaskEditField::Bucket, TaskEditValue::Text(value)) => {
                task.bucket = value.clone()
            }
            _ => unreachable!("checked field/value pair"),
        }
    }
}

impl TaskEditField {
    fn accepts(&self, value: &TaskEditValue) -> bool {
        matches!(
            (self, value),
            (Self::Title | Self::Description | Self::DueDate | Self::Bucket, TaskEditValue::Text(_))
                | (Self::Labels, TaskEditValue::Labels(_))
                | (Self::Cycle, TaskEditValue::OptionalText(_))
                | (Self::Priority, TaskEditValue::Priority(_))
        )
    }
}

fn value_for(task: &Task, field: &TaskEditField) -> TaskEditValue {
    match field {
        TaskEditField::Title => TaskEditValue::Text(task.title.clone()),
        TaskEditField::Description => TaskEditValue::Text(task.description.clone()),
        TaskEditField::Labels => TaskEditValue::Labels(task.labels.clone()),
        TaskEditField::DueDate => TaskEditValue::Text(task.due_date.clone()),
        TaskEditField::Cycle => TaskEditValue::OptionalText(task.cycle.clone()),
        TaskEditField::Priority => TaskEditValue::Priority(task.priority),
        TaskEditField::Bucket => TaskEditValue::Text(task.bucket.clone()),
    }
}

/// Task-detail editor payload. Only fields whose value differs are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskEditInput {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub due_date: String,
    pub cycle: Option<String>,
    pub priority: Priority,
    pub bucket: String,
    /// Requested project context. `None` preserves the current project. A
    /// differing value is rejected by the existing topic/project-move policy.
    pub project: Option<u64>,
    /// `None` means preserve the current stage.
    pub stage: Option<Stage>,
    pub stage_intent: StageIntent,
}

/// The future dirty-entity seam for T14/T15. It names every entity this edit
/// actually mutates: the target task, its approval companions and every task
/// whose activity line was appended. Later entity migrations add their real
/// payloads rather than placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskEditChangeSet {
    pub task: u64,
    pub fields: Vec<TaskEditField>,
    pub stage: Option<TaskStageChange>,
    pub approvals: Vec<GoalApprovalCompanion>,
    pub activity: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalApprovalState {
    pub goal: u64,
    pub stage: Stage,
    pub approved_plan: Option<String>,
    pub signature: Option<String>,
}

/// Semantic run evidence. Status, timestamps and source count are recorded for
/// every relevant run, so an existing terminal/recovery transition is detected.
/// Only the newest run of each task records an output digest; older report
/// bodies are not re-hashed on every edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunSemanticStamp {
    pub id: u64,
    pub task_id: u64,
    pub status: String,
    pub started_at: String,
    pub queued_at: String,
    pub ended_at: Option<String>,
    pub source_count: usize,
    pub output_digest: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OccurrenceSemanticStamp {
    pub id: u64,
    pub automation_id: u64,
    pub status: OccurrenceStatus,
    pub reason: Option<OccurrenceReason>,
    pub reference_time: i64,
    pub requested_at: Option<i64>,
    pub scheduled_for: Option<i64>,
    pub actual_started_at: Option<i64>,
    pub deadline_at: Option<i64>,
    pub retry_requested_at: Option<i64>,
    pub final_run_id: Option<u64>,
    pub run_ids: Vec<u64>,
    pub skipped_count: u64,
    pub skipped_from: Option<i64>,
    pub skipped_to: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutomationSemanticStamp {
    pub id: u64,
    pub version: u32,
    pub status: DefinitionStatus,
    pub cursor: Option<i64>,
    pub anchor: Option<i64>,
    pub approval_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalRuntimeStamp {
    pub goal: u64,
    pub runs: Vec<RunSemanticStamp>,
    pub occurrences: Vec<OccurrenceSemanticStamp>,
    pub automations: Vec<AutomationSemanticStamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalApprovalCompanion {
    pub goal: u64,
    pub before: GoalApprovalState,
    pub after: GoalApprovalState,
    pub runtime: GoalRuntimeStamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskStageChange {
    pub before: Stage,
    pub after: Stage,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TaskEditPlan {
    id: u64,
    project: u64,
    stage_intent: StageIntent,
    intent: TaskEditIntent,
    changes: Vec<TaskFieldChange>,
    stage: Option<TaskStageChange>,
    approval_before: Vec<GoalApprovalState>,
    runtime_before: Vec<GoalRuntimeStamp>,
    impact: Option<ResearchInputImpact>,
}

impl TaskEditPlan {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn project(&self) -> u64 {
        self.project
    }

    pub(crate) fn intent(&self) -> &TaskEditIntent {
        &self.intent
    }

    pub(crate) fn changes(&self) -> &[TaskFieldChange] {
        &self.changes
    }

    pub(crate) fn stage(&self) -> Option<&TaskStageChange> {
        self.stage.as_ref()
    }

    pub(crate) fn stage_intent(&self) -> StageIntent {
        self.stage_intent
    }

    pub(crate) fn approval_before(&self) -> &[GoalApprovalState] {
        &self.approval_before
    }

    pub(crate) fn runtime_before(&self) -> &[GoalRuntimeStamp] {
        &self.runtime_before
    }

    pub(crate) fn impact(&self) -> Option<&ResearchInputImpact> {
        self.impact.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TaskEditUndo {
    id: u64,
    changes: Vec<TaskFieldChange>,
    stage: Option<TaskStageChange>,
    approval: Vec<GoalApprovalCompanion>,
}

impl TaskEditUndo {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn changes(&self) -> &[TaskFieldChange] {
        &self.changes
    }

    pub(crate) fn stage(&self) -> Option<&TaskStageChange> {
        self.stage.as_ref()
    }

    pub(crate) fn approval(&self) -> &[GoalApprovalCompanion] {
        &self.approval
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.stage.is_none() && self.approval.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScopedEditError {
    Rejected(String),
    Conflict(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppliedTaskEdit {
    pub undo: TaskEditUndo,
    pub impact: Option<ResearchInputImpact>,
    pub change_set: TaskEditChangeSet,
}

/// Validate and plan a task edit without mutating the store.
///
/// T21's `preview_task_edit` remains the authority for execution-input,
/// approval, frozen-period and project-move classification.
pub(crate) fn prepare_task_edit(
    store: &Store,
    input: &TaskEditInput,
) -> Result<TaskEditPlan, ScopedEditError> {
    let task = store
        .task(input.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(input.id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before editing it.".into(),
        ));
    }
    let target_project = input.project.unwrap_or(task.project_id);
    if target_project != task.project_id {
        return Err(ScopedEditError::Rejected(research_input::MOVE_PROJECT_MESSAGE.into()));
    }
    let target_stage = input.stage.unwrap_or(task.stage);
    if target_stage != task.stage {
        validate_stage_intent(store, input.id, target_stage, input.stage_intent)?;
    }
    let intent = research_input::TaskEditIntent {
        title: input.title.clone(),
        description: input.description.clone(),
        labels: input.labels.clone(),
        due_date: input.due_date.clone(),
        cycle: input.cycle.clone(),
        bucket: task.bucket.clone(),
        stage: target_stage,
        project: target_project,
    };
    let impact =
        research_input::preview_task_edit(store, input.id, &intent).map_err(ScopedEditError::Rejected)?;

    let mut changes = Vec::new();
    push_change(
        &mut changes,
        task,
        TaskEditField::Title,
        TaskEditValue::Text(input.title.trim().to_string()),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::Description,
        TaskEditValue::Text(input.description.clone()),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::Labels,
        TaskEditValue::Labels(input.labels.clone()),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::DueDate,
        TaskEditValue::Text(input.due_date.trim().to_string()),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::Cycle,
        TaskEditValue::OptionalText(input.cycle.clone()),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::Priority,
        TaskEditValue::Priority(input.priority),
    );
    push_change(
        &mut changes,
        task,
        TaskEditField::Bucket,
        TaskEditValue::Text(input.bucket.clone()),
    );
    let stage = (target_stage != task.stage).then_some(TaskStageChange {
        before: task.stage,
        after: target_stage,
    });

    let execution_field_changed = stage.is_some()
        || changes.iter().any(|change| {
            matches!(
                change.field,
                TaskEditField::Title | TaskEditField::Description
            )
        });
    let needs_approval = execution_field_changed
        && impact
            .as_ref()
            .is_some_and(|impact| !impact.presentation_only);
    let (approval_before, runtime_before) = if needs_approval {
        approval_context(store, input.id)?
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(TaskEditPlan {
        id: input.id,
        project: task.project_id,
        stage_intent: input.stage_intent,
        intent,
        changes,
        stage,
        approval_before,
        runtime_before,
        impact,
    })
}

/// Build the T21 intent that represents the actual owned delta on the current
/// task. Unchanged form fields are taken from current state, never from the
/// stale prepared form.
fn effective_intent(store: &Store, plan: &TaskEditPlan) -> Option<TaskEditIntent> {
    let task = store.task(plan.id)?;
    let mut intent = TaskEditIntent {
        title: task.title.clone(),
        description: task.description.clone(),
        labels: task.labels.clone(),
        due_date: task.due_date.clone(),
        cycle: task.cycle.clone(),
        bucket: task.bucket.clone(),
        stage: task.stage,
        project: task.project_id,
    };
    for change in &plan.changes {
        match (change.field(), change.after()) {
            (TaskEditField::Title, TaskEditValue::Text(value)) => intent.title = value.clone(),
            (TaskEditField::Description, TaskEditValue::Text(value)) => {
                intent.description = value.clone()
            }
            (TaskEditField::Labels, TaskEditValue::Labels(value)) => intent.labels = value.clone(),
            (TaskEditField::DueDate, TaskEditValue::Text(value)) => intent.due_date = value.clone(),
            (TaskEditField::Cycle, TaskEditValue::OptionalText(value)) => {
                intent.cycle = value.clone()
            }
            (TaskEditField::Priority | TaskEditField::Bucket, _) => {}
            _ => unreachable!("checked field/value pair"),
        }
    }
    if let Some(stage) = &plan.stage {
        intent.stage = stage.after;
    }
    Some(intent)
}

/// Rebase a prepared plan onto the current store without rebasing its full
/// editor payload. Only the original owned fields/stage keep their before/after
/// read set; T21 impact and the affected approval/runtime evidence are refreshed
/// from current state. Unrelated metadata and runtime therefore stay current,
/// while a newer owned-field change or project context change conflicts.
pub(crate) fn rebase_task_edit(
    store: &Store,
    plan: &TaskEditPlan,
) -> Result<TaskEditPlan, ScopedEditError> {
    validate_plan(plan)?;
    let current = store
        .task(plan.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(plan.id) {
        return Err(ScopedEditError::Conflict(
            "Restore the archived task before retrying this edit.".into(),
        ));
    }
    if current.project_id != plan.project {
        return Err(ScopedEditError::Conflict(
            "The task's project context changed after this edit was prepared.".into(),
        ));
    }
    for change in &plan.changes {
        if !change.matches_before(current) {
            return Err(ScopedEditError::Conflict(
                "This field changed after the edit was prepared. Review the current value before retrying."
                    .into(),
            ));
        }
    }
    if let Some(stage) = &plan.stage
        && current.stage != stage.before
    {
        return Err(ScopedEditError::Conflict(
            "The task stage changed after the edit was prepared.".into(),
        ));
    }
    let intent = effective_intent(store, plan).ok_or_else(|| {
        ScopedEditError::Rejected("Task not found.".into())
    })?;
    let impact = research_input::preview_task_edit(store, plan.id, &intent)
        .map_err(ScopedEditError::Conflict)?;
    let mut approval_before = Vec::new();
    let mut runtime_before = Vec::new();
    for before in &plan.approval_before {
        approval_before.push(approval_state(store, before.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?);
        runtime_before.push(runtime_stamp(store, before.goal));
    }
    Ok(TaskEditPlan {
        id: plan.id,
        project: plan.project,
        stage_intent: plan.stage_intent,
        intent,
        changes: plan.changes.clone(),
        stage: plan.stage.clone(),
        approval_before,
        runtime_before,
        impact,
    })
}

/// Apply the plan atomically on the current store.
///
/// Preflight verifies immutable plan invariants, project context, owned fields
/// and stage, T21 impact, approval state and goal-scoped runtime evidence before
/// any write. The only fallible domain write is the stage transition, and it is
/// performed first so an error leaves the store untouched. Everything after it
/// uses prevalidated values and cannot fail.
pub(crate) fn apply_task_edit(
    store: &mut Store,
    plan: &TaskEditPlan,
) -> Result<AppliedTaskEdit, ScopedEditError> {
    validate_plan(plan)?;
    let current = store
        .task(plan.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(plan.id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before editing it.".into(),
        ));
    }
    if current.project_id != plan.project {
        return Err(ScopedEditError::Conflict(
            "The task's project context changed after this edit was prepared.".into(),
        ));
    }
    for change in &plan.changes {
        if !change.matches_before(current) {
            return Err(ScopedEditError::Conflict(
                "This field changed after the edit was prepared. Review the current value before applying."
                    .into(),
            ));
        }
    }
    if let Some(stage) = &plan.stage
        && current.stage != stage.before
    {
        return Err(ScopedEditError::Conflict(
            "The task stage changed after the edit was prepared.".into(),
        ));
    }

    // T21 remains the authority. A sibling, dependency, approval, project or
    // frozen-period change after preparation must not apply a stale preview.
    let intent = effective_intent(store, plan).ok_or_else(|| {
        ScopedEditError::Rejected("Task not found.".into())
    })?;
    let current_impact = research_input::preview_task_edit(store, plan.id, &intent)
        .map_err(ScopedEditError::Conflict)?;
    if current_impact != plan.impact {
        return Err(ScopedEditError::Conflict(
            "The research context changed after this edit was prepared. Review the new impact before applying."
                .into(),
        ));
    }

    for before in &plan.approval_before {
        let now = approval_state(store, before.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if &now != before {
            return Err(ScopedEditError::Conflict(
                "The goal's approval or stage changed after this edit was prepared.".into(),
            ));
        }
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .ok_or_else(|| ScopedEditError::Rejected("The edit plan is missing runtime evidence.".into()))?;
        if &runtime_stamp(store, before.goal) != runtime {
            return Err(ScopedEditError::Conflict(
                "A run or automation period changed for this goal after the edit was prepared."
                    .into(),
            ));
        }
    }

    if let Some(stage) = &plan.stage {
        validate_stage_intent(store, plan.id, stage.after, plan.stage_intent)?;
        store
            .validate_stage_transition(plan.id, stage.after)
            .map_err(ScopedEditError::Rejected)?;
        // The only fallible write; preflighted above, and no other write happened.
        store
            .set_stage(plan.id, stage.after)
            .expect("stage transition preflighted");
    }
    if !plan.changes.is_empty() {
        let task = store.task_mut(plan.id).expect("validated task");
        for change in &plan.changes {
            change.apply(task, &change.after);
        }
        task.activity
            .push(activity_event("Task details updated"));
    }

    let mut activity = Vec::new();
    if !plan.changes.is_empty() || plan.stage.is_some() {
        activity.push(plan.id);
    }
    let mut approvals = Vec::new();
    for before in &plan.approval_before {
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .expect("prevalidated runtime evidence")
            .clone();
        if reconcile_goal_scope(store, before.goal) {
            activity.push(before.goal);
        }
        let after = approval_state(store, before.goal)
            .expect("approval goal validated before writes");
        approvals.push(GoalApprovalCompanion {
            goal: before.goal,
            before: before.clone(),
            after,
            runtime,
        });
    }

    // The requested stage may be overridden by scoped approval reconciliation
    // (for example title+InProgress on a goal that loses approval ends in
    // Backlog). Record the effective transition, not the request.
    let effective_stage = store.task(plan.id).expect("validated task").stage;
    let effective_change = plan.stage.as_ref().and_then(|stage| {
        (effective_stage != stage.before).then_some(TaskStageChange {
            before: stage.before,
            after: effective_stage,
        })
    });
    let fields = plan
        .changes
        .iter()
        .map(|change| change.field.clone())
        .collect();
    let change_set = TaskEditChangeSet {
        task: plan.id,
        fields,
        stage: effective_change.clone(),
        approvals: approvals.clone(),
        activity: dedupe(activity),
    };
    Ok(AppliedTaskEdit {
        undo: TaskEditUndo {
            id: plan.id,
            changes: plan.changes.clone(),
            stage: effective_change,
            approval: approvals,
        },
        impact: plan.impact.clone(),
        change_set,
    })
}

/// Apply a conditional inverse. Only owned fields/stage/approval are touched.
///
/// Field and approval restoration is first validated against the recorded
/// `after` state and runtime stamp. The restored fields and approved plan are then
/// staged locally, stage transitions are preflighted against that candidate
/// state, and only then committed. An error rolls the candidate back exactly; no
/// whole-Store clone is used.
pub(crate) fn undo_task_edit(
    store: &mut Store,
    undo: &TaskEditUndo,
) -> Result<(), ScopedEditError> {
    validate_undo(undo)?;
    restore_scoped_inverse(
        store,
        undo.id,
        &undo.changes,
        undo.stage.as_ref(),
        &undo.approval,
        (!undo.changes.is_empty()).then_some("Undo: task edit restored"),
        false,
    )
}

/// Shared validated inverse for task fields/stage and comment entries. It
/// preflights every owned value/companion, restores a local candidate, travels
/// validated stage paths, and rolls everything back on failure.
pub(crate) fn validate_companions(
    store: &Store,
    companions: &[GoalApprovalCompanion],
) -> Result<(), ScopedEditError> {
    for companion in companions {
        let now = approval_state(store, companion.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if now != companion.after {
            return Err(ScopedEditError::Conflict(
                "The goal's approval or stage changed after the edit. Undo would overwrite a newer review."
                    .into(),
            ));
        }
        if runtime_stamp(store, companion.goal) != companion.runtime {
            return Err(ScopedEditError::Conflict(
                "A run or automation period changed for this goal after the edit. Undo would overwrite runtime history."
                    .into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn restore_scoped_inverse(
    store: &mut Store,
    id: u64,
    changes: &[TaskFieldChange],
    stage: Option<&TaskStageChange>,
    companions: &[GoalApprovalCompanion],
    activity_message: Option<&'static str>,
    companions_prevalidated: bool,
) -> Result<(), ScopedEditError> {
    let current = store
        .task(id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(id) {
        return Err(ScopedEditError::Conflict(
            "Restore the archived task before undoing this edit.".into(),
        ));
    }
    for change in changes {
        if !change.matches_after(current) {
            return Err(ScopedEditError::Conflict(
                "This field changed after the edit. Undo would overwrite a newer value."
                    .into(),
            ));
        }
    }
    if let Some(stage) = stage
        && current.stage != stage.after
    {
        return Err(ScopedEditError::Conflict(
            "The task stage changed after the edit. Undo would overwrite a newer stage."
                .into(),
        ));
    }
    if !companions_prevalidated {
        validate_companions(store, companions)?;
    }

    restore_changes(store, id, changes, true);
    restore_companions(store, companions, true);

    let stage_targets = inverse_stage_targets(store, id, stage, companions)?;
    let stage_paths = match undo_stage_paths(store, &stage_targets) {
        Ok(paths) => paths,
        Err(error) => {
            restore_changes(store, id, changes, false);
            restore_companions(store, companions, false);
            return Err(error);
        }
    };

    let snapshots: Vec<StageSnapshot> = stage_targets
        .iter()
        .map(|(target, _)| {
            let task = store.task(*target).expect("prevalidated stage target");
            StageSnapshot {
                id: *target,
                stage: task.stage,
                approved_plan: task.approved_plan.clone(),
                activity_len: task.activity.len(),
            }
        })
        .collect();
    for (target, stages) in &stage_paths {
        for target_stage in stages {
            if let Err(message) = store.set_stage(*target, *target_stage) {
                restore_stage_snapshots(store, &snapshots);
                restore_changes(store, id, changes, false);
                restore_companions(store, companions, false);
                return Err(ScopedEditError::Conflict(message));
            }
        }
    }
    restore_companions(store, companions, true);

    if let Some(message) = activity_message
        && let Some(task) = store.task_mut(id)
    {
        task.activity.push(activity_event(message));
    }
    Ok(())
}

fn restore_changes(store: &mut Store, id: u64, changes: &[TaskFieldChange], before: bool) {
    let Some(task) = store.task_mut(id) else {
        return;
    };
    for change in changes {
        let value = if before { change.before() } else { change.after() };
        change.apply(task, value);
    }
}

fn restore_companions(store: &mut Store, companions: &[GoalApprovalCompanion], before: bool) {
    for companion in companions {
        let Some(task) = store.task_mut(companion.goal) else {
            continue;
        };
        task.approved_plan = if before {
            companion.before.approved_plan.clone()
        } else {
            companion.after.approved_plan.clone()
        };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommentEditPlan {
    id: u64,
    body: String,
    approval_before: Vec<GoalApprovalState>,
    runtime_before: Vec<GoalRuntimeStamp>,
    impact: Option<ResearchInputImpact>,
}

impl CommentEditPlan {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn impact(&self) -> Option<&ResearchInputImpact> {
        self.impact.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommentEditUndo {
    id: u64,
    index: usize,
    after_len: usize,
    author: String,
    body: String,
    created_at: String,
    approval: Vec<GoalApprovalCompanion>,
}

impl CommentEditUndo {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn approval(&self) -> &[GoalApprovalCompanion] {
        &self.approval
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentEditChangeSet {
    pub task: u64,
    pub index: usize,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub approvals: Vec<GoalApprovalCompanion>,
    pub activity: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppliedCommentEdit {
    pub undo: CommentEditUndo,
    pub impact: Option<ResearchInputImpact>,
    pub change_set: CommentEditChangeSet,
}

pub(crate) fn prepare_comment_edit(
    store: &Store,
    id: u64,
    body: &str,
) -> Result<CommentEditPlan, ScopedEditError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(ScopedEditError::Rejected(
            "Write a comment before posting.".into(),
        ));
    }
    let impact = research_input::preview_comment(store, id, body)
        .map_err(ScopedEditError::Rejected)?;
    let needs_approval = impact
        .as_ref()
        .is_some_and(|impact| !impact.presentation_only);
    let (approval_before, runtime_before) = if needs_approval {
        approval_context(store, id)?
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(CommentEditPlan {
        id,
        body: body.to_string(),
        approval_before,
        runtime_before,
        impact,
    })
}

pub(crate) fn apply_comment_edit(
    store: &mut Store,
    plan: &CommentEditPlan,
) -> Result<AppliedCommentEdit, ScopedEditError> {
    if plan.body.trim().is_empty() {
        return Err(ScopedEditError::Rejected(
            "Write a comment before posting.".into(),
        ));
    }
    if plan.approval_before.len() != plan.runtime_before.len() {
        return Err(ScopedEditError::Rejected(
            "The comment plan has mismatched approval and runtime evidence.".into(),
        ));
    }
    let current = store
        .task(plan.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(plan.id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before commenting.".into(),
        ));
    }
    let current_impact = research_input::preview_comment(store, plan.id, &plan.body)
        .map_err(ScopedEditError::Conflict)?;
    if current_impact != plan.impact {
        return Err(ScopedEditError::Conflict(
            "The research context changed after this comment was prepared. Review the new impact before posting."
                .into(),
        ));
    }
    for before in &plan.approval_before {
        let now = approval_state(store, before.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if &now != before {
            return Err(ScopedEditError::Conflict(
                "The goal's approval or stage changed after this comment was prepared.".into(),
            ));
        }
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .ok_or_else(|| {
                ScopedEditError::Rejected(
                    "The comment plan is missing runtime evidence.".into(),
                )
            })?;
        if &runtime_stamp(store, before.goal) != runtime {
            return Err(ScopedEditError::Conflict(
                "A run or automation period changed for this goal after the comment was prepared."
                    .into(),
            ));
        }
    }

    let index = current.comments.len();
    store
        .add_comment(plan.id, &plan.body)
        .map_err(ScopedEditError::Rejected)?;
    let comment = store
        .task(plan.id)
        .and_then(|task| task.comments.last())
        .cloned()
        .ok_or_else(|| ScopedEditError::Rejected("The comment was not stored.".into()))?;

    let mut activity = vec![plan.id];
    let mut approvals = Vec::new();
    for before in &plan.approval_before {
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .expect("prevalidated runtime evidence")
            .clone();
        if reconcile_goal_scope(store, before.goal) {
            activity.push(before.goal);
        }
        let after = approval_state(store, before.goal)
            .expect("approval goal validated before writes");
        approvals.push(GoalApprovalCompanion {
            goal: before.goal,
            before: before.clone(),
            after,
            runtime,
        });
    }

    let undo_author = comment.author.clone();
    let undo_created_at = comment.created_at.clone();
    Ok(AppliedCommentEdit {
        undo: CommentEditUndo {
            id: plan.id,
            index,
            after_len: index + 1,
            author: comment.author,
            body: comment.body,
            created_at: comment.created_at,
            approval: approvals.clone(),
        },
        impact: plan.impact.clone(),
        change_set: CommentEditChangeSet {
            task: plan.id,
            index,
            author: undo_author.clone(),
            body: plan.body.clone(),
            created_at: undo_created_at.clone(),
            approvals,
            activity: dedupe(activity),
        },
    })
}

pub(crate) fn undo_comment_edit(
    store: &mut Store,
    undo: &CommentEditUndo,
) -> Result<(), ScopedEditError> {
    let task = store
        .task(undo.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(undo.id) {
        return Err(ScopedEditError::Conflict(
            "Restore the archived task before undoing this comment.".into(),
        ));
    }
    if task.comments.len() != undo.after_len {
        return Err(ScopedEditError::Conflict(
            "A newer comment changed this conversation. Undo would remove the wrong comment."
                .into(),
        ));
    }
    let comment = task.comments.get(undo.index).ok_or_else(|| {
        ScopedEditError::Conflict(
            "The comment changed after it was posted. Undo would remove a newer comment."
                .into(),
        )
    })?;
    if comment.author != undo.author
        || comment.body != undo.body
        || comment.created_at != undo.created_at
    {
        return Err(ScopedEditError::Conflict(
            "The comment changed after it was posted. Undo would remove a newer comment."
                .into(),
        ));
    }

    validate_companions(store, &undo.approval)?;
    let comment = comment.clone();
    if let Some(task) = store.task_mut(undo.id) {
        task.comments.remove(undo.index);
    }
    let restored = restore_scoped_inverse(
        store,
        undo.id,
        &[],
        None,
        &undo.approval,
        Some("Undo: comment removed"),
        true,
    );
    if let Err(error) = restored {
        if let Some(task) = store.task_mut(undo.id) {
            let index = undo.index.min(task.comments.len());
            task.comments.insert(index, comment);
        }
        return Err(error);
    }
    Ok(())
}

fn validate_plan(plan: &TaskEditPlan) -> Result<(), ScopedEditError> {
    validate_changes(&plan.changes)?;
    if let Some(stage) = &plan.stage
        && stage.before == stage.after
    {
        return Err(ScopedEditError::Rejected(
            "The edit plan contains a no-op stage change.".into(),
        ));
    }
    if plan.approval_before.len() != plan.runtime_before.len() {
        return Err(ScopedEditError::Rejected(
            "The edit plan has mismatched approval and runtime evidence.".into(),
        ));
    }
    for approval in &plan.approval_before {
        if !plan
            .runtime_before
            .iter()
            .any(|runtime| runtime.goal == approval.goal)
        {
            return Err(ScopedEditError::Rejected(
                "The edit plan is missing runtime evidence for an approval goal.".into(),
            ));
        }
    }
    Ok(())
}

fn validate_undo(undo: &TaskEditUndo) -> Result<(), ScopedEditError> {
    validate_changes(&undo.changes)?;
    if let Some(stage) = &undo.stage
        && stage.before == stage.after
    {
        return Err(ScopedEditError::Rejected(
            "The undo record contains a no-op stage change.".into(),
        ));
    }
    let mut approvals = Vec::new();
    for companion in &undo.approval {
        if approvals.contains(&companion.goal) {
            return Err(ScopedEditError::Rejected(
                "The undo record contains duplicate approval goals.".into(),
            ));
        }
        approvals.push(companion.goal);
    }
    if let Some(stage) = &undo.stage
        && let Some(companion) = undo
            .approval
            .iter()
            .find(|companion| companion.goal == undo.id)
        && companion.before.stage != stage.before
    {
        return Err(ScopedEditError::Rejected(
            "The undo record has inconsistent stage and approval evidence.".into(),
        ));
    }
    Ok(())
}

fn validate_changes(changes: &[TaskFieldChange]) -> Result<(), ScopedEditError> {
    let mut seen = Vec::new();
    for change in changes {
        if !change.field().accepts(change.before()) || !change.field().accepts(change.after()) {
            return Err(ScopedEditError::Rejected(
                "The edit plan has a malformed field payload.".into(),
            ));
        }
        if change.before == change.after {
            return Err(ScopedEditError::Rejected(
                "The edit plan contains a no-op field change.".into(),
            ));
        }
        if seen.contains(change.field()) {
            return Err(ScopedEditError::Rejected(
                "The edit plan contains duplicate field changes.".into(),
            ));
        }
        seen.push(change.field().clone());
    }
    Ok(())
}

fn push_change(
    changes: &mut Vec<TaskFieldChange>,
    task: &Task,
    field: TaskEditField,
    after: TaskEditValue,
) {
    let before = value_for(task, &field);
    if before != after {
        if let Some(change) = TaskFieldChange::checked(field, before, after) {
            changes.push(change);
        }
    }
}

pub(crate) fn approval_context(
    store: &Store,
    task: u64,
) -> Result<(Vec<GoalApprovalState>, Vec<GoalRuntimeStamp>), ScopedEditError> {
    let Some(goal) = store.owning_research_goal(task) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let state = approval_state(store, goal)
        .ok_or_else(|| ScopedEditError::Rejected("The owning goal is missing.".into()))?;
    Ok((vec![state], vec![runtime_stamp(store, goal)]))
}

pub(crate) fn approval_state(store: &Store, goal: u64) -> Option<GoalApprovalState> {
    let task = store.task(goal)?;
    Some(GoalApprovalState {
        goal,
        stage: task.stage,
        approved_plan: task.approved_plan.clone(),
        signature: store.goal_plan_signature(goal),
    })
}

pub(crate) fn runtime_stamp(store: &Store, goal: u64) -> GoalRuntimeStamp {
    let mut relevant: Vec<&AgentRun> = store
        .runs()
        .iter()
        .filter(|run| {
            run.task_id == goal || store.owning_research_goal(run.task_id) == Some(goal)
        })
        .collect();
    relevant.sort_by_key(|run| (run.task_id, run.id));
    let mut latest_by_task: std::collections::BTreeMap<u64, u64> = Default::default();
    for run in &relevant {
        latest_by_task
            .entry(run.task_id)
            .and_modify(|latest| *latest = (*latest).max(run.id))
            .or_insert(run.id);
    }
    let runs = relevant
        .into_iter()
        .map(|run| RunSemanticStamp {
            id: run.id,
            task_id: run.task_id,
            status: run.status.clone(),
            started_at: run.started_at.clone(),
            queued_at: run.queued_at.clone(),
            ended_at: run.ended_at.clone(),
            source_count: run.sources.len(),
            output_digest: (latest_by_task.get(&run.task_id) == Some(&run.id))
                .then(|| output_digest(&run.output)),
        })
        .collect();

    let mut occurrences: Vec<_> = store
        .all_occurrences()
        .iter()
        .filter(|occurrence| {
            occurrence.research_goal_id == Some(goal) || occurrence.research_goal_ids.contains(&goal)
        })
        .collect();
    occurrences.sort_by_key(|occurrence| occurrence.id);
    let occurrence_stamps = occurrences
        .iter()
        .map(|occurrence| OccurrenceSemanticStamp {
            id: occurrence.id,
            automation_id: occurrence.automation_id,
            status: occurrence.status,
            reason: occurrence.reason.clone(),
            reference_time: occurrence.reference_time,
            requested_at: occurrence.requested_at,
            scheduled_for: occurrence.scheduled_for,
            actual_started_at: occurrence.actual_started_at,
            deadline_at: occurrence.deadline_at,
            retry_requested_at: occurrence.retry_requested_at,
            final_run_id: occurrence.final_run_id,
            run_ids: occurrence.run_ids.clone(),
            skipped_count: occurrence.skipped_count,
            skipped_from: occurrence.skipped_from,
            skipped_to: occurrence.skipped_to,
        })
        .collect();

    let mut automation_ids: Vec<u64> = occurrences
        .iter()
        .map(|occurrence| occurrence.automation_id)
        .collect();
    automation_ids.sort_unstable();
    automation_ids.dedup();
    let automations = automation_ids
        .into_iter()
        .filter_map(|id| store.automation(id))
        .map(|definition| AutomationSemanticStamp {
            id: definition.id,
            version: definition.version,
            status: definition.status,
            cursor: definition.cursor,
            anchor: definition.anchor,
            approval_version: definition
                .approval
                .as_ref()
                .map(|approval| approval.version)
                .unwrap_or(0),
        })
        .collect();

    GoalRuntimeStamp {
        goal,
        runs,
        occurrences: occurrence_stamps,
        automations,
    }
}

fn output_digest(output: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    output.hash(&mut hasher);
    hasher.finish()
}

/// Reconcile only the affected goal. The global `reconcile_goal_approvals` must
/// not be used here: an unrelated already-stale goal is not part of this edit's
/// change set and must remain untouched.
pub(crate) fn reconcile_goal_scope(store: &mut Store, goal: u64) -> bool {
    let Some(task) = store.task(goal) else {
        return false;
    };
    let should_withdraw = task
        .research
        .as_ref()
        .is_some_and(|research| research.kind == ResearchKind::Goal && research.requires_approval)
        && !task.archived
        && !matches!(task.stage, Stage::Backlog | Stage::Cancelled)
        && task.approved_plan != store.goal_plan_signature(goal);
    if !should_withdraw {
        return false;
    }
    let task = store.task_mut(goal).expect("validated goal");
    task.approved_plan = None;
    task.stage = Stage::Backlog;
    task.activity
        .push(activity_event("Research plan changed · review before execution"));
    true
}

/// The task-detail editor is not the execution submission path. Reject a
/// forged InProgress intent and every transition the live manual workflow
/// rejects. The live boundary must additionally pass its pending batch context.
fn validate_stage_intent(
    store: &Store,
    id: u64,
    stage: Stage,
    intent: StageIntent,
) -> Result<(), ScopedEditError> {
    if stage == Stage::InProgress {
        return Err(ScopedEditError::Rejected(
            "Save task details first, then submit execution from Board or the status menu."
                .into(),
        ));
    }
    if intent == StageIntent::Reopen {
        let task = store.task(id).ok_or_else(|| {
            ScopedEditError::Rejected("Task not found.".into())
        })?;
        if stage != Stage::Planned {
            return Err(ScopedEditError::Rejected(
                "Reopen only restores a cancelled research step to Planned.".into(),
            ));
        }
        let Some(research) = task.research.as_ref() else {
            return Err(ScopedEditError::Rejected(
                "Only research steps can be reopened.".into(),
            ));
        };
        if research.kind == crate::research::ResearchKind::Goal {
            return Err(ScopedEditError::Rejected(
                "Only research steps can be reopened.".into(),
            ));
        }
        if task.parent_id.is_none() {
            return Err(ScopedEditError::Rejected(
                "Only research steps can be reopened.".into(),
            ));
        }
        if task.stage != Stage::Cancelled {
            return Err(ScopedEditError::Rejected(
                "Only a cancelled research step can be reopened.".into(),
            ));
        }
        if store.task_is_archived(id) {
            return Err(ScopedEditError::Rejected(
                "Restore the archived task before reopening it.".into(),
            ));
        }
        return Ok(());
    }
    store
        .validate_manual_transition(id, stage, None)
        .map_err(ScopedEditError::Rejected)
}

/// Stage paths for an inverse. A completed goal reopened by an execution-input
/// edit must go through the approved Planned state before Done; a direct
/// Backlog -> Done write would violate the shared lifecycle policy.
fn undo_stage_paths(
    store: &Store,
    targets: &[(u64, Stage)],
) -> Result<Vec<(u64, Vec<Stage>)>, ScopedEditError> {
    let mut paths = Vec::new();
    for (id, target) in targets {
        let current = store
            .task(*id)
            .ok_or_else(|| ScopedEditError::Conflict("A stage target is missing.".into()))?
            .stage;
        if current == *target {
            continue;
        }
        match store.validate_stage_transition(*id, *target) {
            Ok(()) => paths.push((*id, vec![*target])),
            Err(message) => {
                let needs_approval_path = *target == Stage::Done
                    && matches!(current, Stage::Backlog | Stage::Cancelled)
                    && store.validate_stage_transition(*id, Stage::Planned).is_ok();
                if needs_approval_path {
                    paths.push((*id, vec![Stage::Planned, *target]));
                } else {
                    return Err(ScopedEditError::Conflict(message));
                }
            }
        }
    }
    Ok(paths)
}

fn inverse_stage_targets(
    store: &Store,
    id: u64,
    stage: Option<&TaskStageChange>,
    companions: &[GoalApprovalCompanion],
) -> Result<Vec<(u64, Stage)>, ScopedEditError> {
    let mut targets: std::collections::BTreeMap<u64, Stage> = Default::default();
    if let Some(stage) = stage {
        targets.insert(id, stage.before);
    }
    for companion in companions {
        let task = store.task(companion.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if task.stage != companion.before.stage {
            if let Some(existing) = targets.insert(companion.goal, companion.before.stage)
                && existing != companion.before.stage
            {
                return Err(ScopedEditError::Rejected(
                    "The undo record has conflicting stage targets.".into(),
                ));
            }
        }
    }
    Ok(targets.into_iter().collect())
}


struct StageSnapshot {
    id: u64,
    stage: Stage,
    approved_plan: Option<String>,
    activity_len: usize,
}

fn restore_stage_snapshots(store: &mut Store, snapshots: &[StageSnapshot]) {
    for snapshot in snapshots {
        let Some(task) = store.task_mut(snapshot.id) else {
            continue;
        };
        task.stage = snapshot.stage;
        task.approved_plan = snapshot.approved_plan.clone();
        task.activity.truncate(snapshot.activity_len);
    }
}

fn dedupe(mut values: Vec<u64>) -> Vec<u64> {
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationDefinition, AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy,
        AutomationShape, AutomationTemplate, OccurrenceReason, SkipCode, TimeWindow, TriggerSource,
        WindowRule,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Priority, ProjectKind};

    const BRANCHED: &str = r#"{"tasks":[
        {"title":"Collect A","instructions":"Collect from source A","kind":"collect","depends_on":[]},
        {"title":"Collect B","instructions":"Collect from source B","kind":"collect","depends_on":[]},
        {"title":"Analyze A","instructions":"Analyze source A","kind":"analyze","depends_on":[0]},
        {"title":"Report","instructions":"Write the final report","kind":"advise","depends_on":[2]}
    ]}"#;

    fn store_with_goal() -> (Store, u64, Vec<u64>) {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.research_config_mut().models = [
            "test/model".into(),
            "test/model".into(),
            "test/model".into(),
            "test/model".into(),
        ];
        let topic = store.create_topic(1, "Topic").unwrap();
        let goal = store
            .create_research_goal_in_topic("Goal".into(), "Brief".into(), 1, Some(topic))
            .unwrap();
        let steps = store.apply_research_plan(goal, BRANCHED).unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.set_stage(goal, Stage::Planned).unwrap();
        (store, goal, steps)
    }

    fn input_for(store: &Store, id: u64) -> TaskEditInput {
        let task = store.task(id).unwrap();
        TaskEditInput {
            id,
            title: task.title.clone(),
            description: task.description.clone(),
            labels: task.labels.clone(),
            due_date: task.due_date.clone(),
            cycle: task.cycle.clone(),
            priority: task.priority,
            bucket: task.bucket.clone(),
            project: Some(task.project_id),
            stage: Some(task.stage),
            stage_intent: StageIntent::Editor,
        }
    }

    fn complete_run(store: &mut Store, id: u64) {
        let mut run = store.prepare_run(id).unwrap();
        run.status = "Succeeded".into();
        run.output = format!("Completed result for {id}");
        store.push_run(run);
    }

    fn complete_goal(store: &mut Store, goal: u64, steps: &[u64]) {
        for id in steps {
            complete_run(store, *id);
            store.set_stage(*id, Stage::Done).unwrap();
        }
        store.set_stage(goal, Stage::Done).unwrap();
    }

    fn occurrence(id: u64, automation: u64, goal: u64, now: i64) -> AutomationOccurrence {
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
            snapshot: AutomationOccurrenceSnapshot {
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
                approval_signature: "test".into(),
            },
            status: OccurrenceStatus::Succeeded,
            reason: Some(OccurrenceReason::new(SkipCode::Expired, "test")),
            research_goal_id: Some(goal),
            research_goal_ids: vec![goal],
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

    fn definition(id: u64) -> AutomationDefinition {
        AutomationDefinition {
            id,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Research,
            name: "Daily check".into(),
            objective: "Track changes".into(),
            expected_result: "A sourced note".into(),
            schedule: Schedule {
                spec: ScheduleSpec::Daily {
                    hour: 8,
                    minute: 30,
                },
                timezone: "UTC".into(),
            },
            status: DefinitionStatus::Enabled,
            template: None,
            plan_binding: None,
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: Some(1_000),
            anchor: Some(1_000),
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    #[test]
    fn title_only_edit_and_inverse_restore_approval_without_touching_labels() {
        let (mut store, goal, _) = store_with_goal();
        let before = store.task(goal).unwrap().clone();
        let approval_before = store.task(goal).unwrap().approved_plan.clone();
        let mut input = input_for(&store, goal);
        input.title = "Edited title".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Edited title");
        assert!(!applied.undo.approval().is_empty());
        assert_eq!(applied.undo.id(), goal);
        assert_eq!(applied.undo.changes().len(), 1);
        assert!(applied.undo.stage().is_none());
        assert_eq!(store.task(goal).unwrap().approved_plan, None);

        store.task_mut(goal).unwrap().labels = vec!["New metadata".to_string()];
        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(goal).unwrap().title, before.title);
        assert_eq!(
            store.task(goal).unwrap().labels,
            vec!["New metadata".to_string()]
        );
        assert_eq!(store.task(goal).unwrap().approved_plan, approval_before);
    }

    #[test]
    fn metadata_edit_preserves_real_completed_goal_result_and_runs() {
        let (mut store, goal, steps) = store_with_goal();
        complete_goal(&mut store, goal, &steps);
        let before = store.task(goal).unwrap().clone();
        let runs = store.runs().to_vec();
        let result_before = store.research_result(steps[0]).map(|run| run.id);
        assert!(result_before.is_some(), "fixture must have a current result");

        let mut input = input_for(&store, goal);
        input.labels = vec!["Metadata".into()];
        input.due_date = "2026-12-01".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        assert!(plan.approval_before().is_empty());
        apply_task_edit(&mut store, &plan).unwrap();

        assert_eq!(store.task(goal).unwrap().stage, Stage::Done);
        assert_eq!(store.task(goal).unwrap().stage, before.stage);
        assert_eq!(
            store.task(goal).unwrap().approved_plan,
            before.approved_plan
        );
        assert_eq!(store.runs(), runs);
        assert_eq!(store.research_result(steps[0]).map(|run| run.id), result_before);
        assert!(store.goal_execution_approved(goal));
    }

    #[test]
    fn combined_title_and_stage_effective_result_agrees_with_t21_preview() {
        let (mut store, goal, _) = store_with_goal();
        let original_stage = store.task(goal).unwrap().stage;
        let mut input = input_for(&store, goal);
        input.title = "Combined".into();
        input.stage = Some(Stage::Backlog);
        let plan = prepare_task_edit(&store, &input).unwrap();
        let expected = plan.impact().cloned().expect("research impact");
        let applied = apply_task_edit(&mut store, &plan).unwrap();

        // Preview withdraws approval; the effective stage is the reconciled
        // Backlog state recorded for undo.
        assert!(!expected.approval_after);
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);
        assert_eq!(store.goal_execution_approved(goal), expected.approval_after);
        assert_eq!(store.task(goal).unwrap().title, "Combined");
        assert_eq!(applied.change_set.fields, vec![TaskEditField::Title]);
        assert_eq!(
            applied.change_set.stage,
            Some(TaskStageChange {
                before: original_stage,
                after: Stage::Backlog,
            })
        );

        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Goal");
        assert_eq!(store.task(goal).unwrap().stage, original_stage);
        assert!(store.goal_execution_approved(goal));
    }

    #[test]
    fn description_edit_withdraws_branched_graph_approval_and_inverse_restores_it() {
        let (mut store, goal, steps) = store_with_goal();
        let child = steps[0];
        let before_description = store.task(child).unwrap().description.clone();
        let mut input = input_for(&store, child);
        input.description = "Changed evidence scope".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let impact = plan.impact().cloned().expect("research impact");
        assert_eq!(impact.changed_input_steps, vec![child]);
        assert_eq!(impact.affected_input_steps, vec![child, steps[2], steps[3]]);
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(
            store.task(child).unwrap().description,
            "Changed evidence scope"
        );
        assert!(!store.goal_execution_approved(goal));
        assert_eq!(applied.undo.approval().len(), 1);

        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(child).unwrap().description, before_description);
        assert!(store.goal_execution_approved(goal));
    }

    #[test]
    fn stale_same_field_change_conflicts_without_partial_mutation() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        input.stage = Some(Stage::Backlog);
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(goal).unwrap().title = "Newer title".into();
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn stale_sibling_change_conflicts_even_when_target_field_is_unchanged() {
        let (mut store, goal, steps) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Prepared goal title".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        // Target title is untouched; a sibling step input changed instead.
        store.task_mut(steps[1]).unwrap().title = "Sibling changed".into();
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn stale_approval_or_context_conflicts_without_changes() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(goal).unwrap().approved_plan = Some("different".into());
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Prepared project context".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(goal).unwrap().project_id = 2;
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn frozen_membership_change_between_prepare_and_apply_is_rejected() {
        let mut store = Store::empty();
        let task = store.create_task("Normal", 1, Stage::Planned, None).unwrap();
        let mut input = input_for(&store, task);
        input.title = "Changed".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(task).unwrap().automation_id = Some(1);
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn frozen_period_rejects_execution_input_but_allows_metadata() {
        let mut store = Store::empty();
        let task = store.create_task("Frozen", 1, Stage::Planned, None).unwrap();
        store.task_mut(task).unwrap().automation_id = Some(1);
        let mut input = input_for(&store, task);
        input.title = "Changed".into();
        assert!(matches!(
            prepare_task_edit(&store, &input),
            Err(ScopedEditError::Rejected(_))
        ));
        let mut metadata = input_for(&store, task);
        metadata.labels = vec!["Allowed".to_string()];
        let plan = prepare_task_edit(&store, &metadata).unwrap();
        apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(task).unwrap().labels, vec!["Allowed".to_string()]);
    }

    #[test]
    fn project_context_change_is_rejected_and_not_silently_ignored() {
        let (store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.project = Some(2);
        let before = store.clone();
        assert!(matches!(
            prepare_task_edit(&store, &input),
            Err(ScopedEditError::Rejected(message)) if message.contains("Move the entire topic")
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn unrelated_runtime_between_prepare_and_apply_is_preserved() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        let plan = prepare_task_edit(&store, &input).unwrap();

        let other = store
            .create_research_goal("Other".into(), "Other brief".into(), 1)
            .unwrap();
        store.apply_research_plan(other, BRANCHED).unwrap();
        let other_child = store.research_children(other)[0];
        let mut other_run = store.prepare_run(other_child).unwrap();
        other_run.id = 500;
        other_run.status = "Succeeded".into();
        other_run.output = "other history".into();
        other_run.input_signature = store
            .research_signature(other_child)
            .unwrap_or_default();
        store.push_run(other_run.clone());
        store.push_occurrence(occurrence(7, 99, other, 2_000));

        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.runs(), vec![other_run]);
        assert_eq!(store.all_occurrences().len(), 1);

        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Goal");
        assert_eq!(store.runs().len(), 1);
        assert_eq!(store.all_occurrences().len(), 1);
    }

    #[test]
    fn existing_terminal_run_timestamp_or_output_change_between_prepare_and_apply_conflicts() {
        let (mut store, goal, steps) = store_with_goal();
        complete_run(&mut store, steps[0]);
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();

        store.run_at_mut(0).unwrap().ended_at = Some("2026-01-01 00:00:00 +00:00".into());
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, steps) = store_with_goal();
        complete_run(&mut store, steps[0]);
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.run_at_mut(0).unwrap().output = "Changed historical output".into();
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, steps) = store_with_goal();
        complete_run(&mut store, steps[0]);
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.run_at_mut(0).unwrap().status = "Interrupted".into();
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn existing_occurrence_or_cursor_change_between_prepare_and_apply_conflicts() {
        let (mut store, goal, _) = store_with_goal();
        store.push_automation(definition(9));
        store.push_occurrence(occurrence(4, 9, goal, 1_000));
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();

        store.occurrence_at_mut(0).unwrap().retry_requested_at = Some(2_000);
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, _) = store_with_goal();
        store.push_automation(definition(9));
        store.push_occurrence(occurrence(4, 9, goal, 1_000));
        let mut input = input_for(&store, goal);
        input.title = "Prepared".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.automation_at_mut(0).unwrap().cursor = Some(3_000);
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn related_runtime_between_apply_and_undo_conflicts_and_preserves_history() {
        let (mut store, goal, steps) = store_with_goal();
        // Prepare the completed run while the goal is still approved; the edit
        // itself withdraws approval, so this run is only inserted after apply.
        let mut run = store.prepare_run(steps[0]).unwrap();
        run.status = "Succeeded".into();
        run.output = "completed after apply".into();
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();

        store.push_run(run.clone());
        assert!(matches!(
            undo_task_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store.runs(), vec![run]);
    }

    #[test]
    fn related_occurrence_change_between_apply_and_undo_conflicts_and_preserves_history() {
        let (mut store, goal, _) = store_with_goal();
        store.push_automation(definition(9));
        store.push_occurrence(occurrence(4, 9, goal, 1_000));
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();

        store.occurrence_at_mut(0).unwrap().status = OccurrenceStatus::Failed;
        let before = store.clone();
        assert!(matches!(
            undo_task_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn changed_same_field_or_stage_between_apply_and_undo_conflicts() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        input.stage = Some(Stage::Backlog);
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        store.task_mut(goal).unwrap().title = "Newer".into();
        store.task_mut(goal).unwrap().labels = vec!["Later".into()];
        let before = store.clone();
        assert!(matches!(
            undo_task_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        input.stage = Some(Stage::Backlog);
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        store.set_stage(goal, Stage::Planned).unwrap();
        let before = store.clone();
        assert!(matches!(
            undo_task_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn combined_invalid_stage_leaves_full_store_unchanged() {
        let (mut store, goal, steps) = store_with_goal();
        for id in &steps {
            complete_run(&mut store, *id);
            store.set_stage(*id, Stage::Done).unwrap();
        }
        // Review -> Done is a valid editor transition while the goal is complete.
        store.set_stage(goal, Stage::Review).unwrap();
        let mut input = input_for(&store, goal);
        input.title = "Combined".into();
        input.stage = Some(Stage::Done);
        let plan = prepare_task_edit(&store, &input).unwrap();
        // Stage is not part of the signatures/impact, so the title must not be
        // written when the completion precondition later becomes invalid.
        store.set_stage(steps[0], Stage::Backlog).unwrap();
        let before = store.clone();
        assert!(matches!(
            apply_task_edit(&mut store, &plan),
            Err(ScopedEditError::Rejected(_) | ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before, "invalid stage must not apply the title");
    }

    #[test]
    fn editor_rejects_forged_inprogress_and_research_child_stage_intents() {
        let (store, goal, steps) = store_with_goal();
        let mut in_progress = input_for(&store, goal);
        in_progress.stage = Some(Stage::InProgress);
        assert!(matches!(
            prepare_task_edit(&store, &in_progress),
            Err(ScopedEditError::Rejected(message)) if message.contains("submit execution")
        ));
        let mut child_stage = input_for(&store, steps[0]);
        child_stage.stage = Some(Stage::Review);
        assert!(matches!(
            prepare_task_edit(&store, &child_stage),
            Err(ScopedEditError::Rejected(_))
        ));
    }

    #[test]
    fn completed_goal_title_edit_undo_restores_done_results_and_approval() {
        let (mut store, goal, steps) = store_with_goal();
        complete_goal(&mut store, goal, &steps);
        let runs = store.runs().to_vec();
        let results: Vec<_> = steps
            .iter()
            .map(|id| store.research_result(*id).map(|run| run.id))
            .collect();
        assert!(results.iter().all(Option::is_some));
        let approval = store.task(goal).unwrap().approved_plan.clone();
        assert!(approval.is_some());

        let mut input = input_for(&store, goal);
        input.title = "Completed title edit".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);

        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Goal");
        assert_eq!(store.task(goal).unwrap().stage, Stage::Done);
        assert_eq!(store.task(goal).unwrap().approved_plan, approval);
        assert!(store.goal_execution_approved(goal));
        assert_eq!(store.runs(), runs);
        let after: Vec<_> = steps
            .iter()
            .map(|id| store.research_result(*id).map(|run| run.id))
            .collect();
        assert_eq!(after, results);
    }

    #[test]
    fn undo_rejected_stage_restore_leaves_full_store_unchanged() {
        let (mut store, goal, steps) = store_with_goal();
        complete_goal(&mut store, goal, &steps);
        assert_eq!(store.task(goal).unwrap().stage, Stage::Done);
        let mut input = input_for(&store, goal);
        input.title = "Temporary".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);

        // Child stage is not part of approval/runtime evidence, so the undo
        // preflight reaches the stage lifecycle check and must roll back the
        // restored title/approved plan.
        store.set_stage(steps[0], Stage::Backlog).unwrap();
        let before = store.clone();
        assert!(matches!(
            undo_task_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn goal_scoped_reconcile_does_not_touch_unrelated_stale_goal() {
        let (mut store, goal, _) = store_with_goal();
        let other = store
            .create_research_goal("Other".into(), "Other brief".into(), 1)
            .unwrap();
        store.apply_research_plan(other, BRANCHED).unwrap();
        store
            .task_mut(other)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.task_mut(other).unwrap().approved_plan = Some("stale".into());
        store.task_mut(other).unwrap().stage = Stage::Planned;
        let other_before = store.task(other).unwrap().clone();

        let mut input = input_for(&store, goal);
        input.title = "Scoped".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        apply_task_edit(&mut store, &plan).unwrap();

        assert_eq!(store.task(other).unwrap(), &other_before);
    }

    #[test]
    fn rebased_metadata_edit_preserves_a_newer_title_and_stays_presentation_only() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.labels = vec!["Only metadata".into()];
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(goal).unwrap().title = "Newer title".into();
        let rebased = rebase_task_edit(&store, &plan).unwrap();
        let impact = rebased.impact().unwrap();
        assert!(impact.presentation_only);
        assert!(!impact.signature_changed);
        assert!(!impact.review_required);
        assert!(rebased.approval_before().is_empty());

        apply_task_edit(&mut store, &rebased).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Newer title");
        assert_eq!(store.task(goal).unwrap().labels, vec!["Only metadata".to_string()]);
    }

    #[test]
    fn rebased_title_edit_previews_the_current_description_not_the_stale_form() {
        let (mut store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.title = "Edited title".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        store.task_mut(goal).unwrap().description = "Newer description".into();
        let rebased = rebase_task_edit(&store, &plan).unwrap();
        let impact = rebased.impact().unwrap();
        assert!(impact.signature_changed, "the current description is an input");

        apply_task_edit(&mut store, &rebased).unwrap();
        assert_eq!(store.task(goal).unwrap().title, "Edited title");
        assert_eq!(store.task(goal).unwrap().description, "Newer description");
    }

    #[test]
    fn comment_on_completed_goal_undo_restores_done_signature_and_exact_comment() {
        let (mut store, goal, steps) = store_with_goal();
        complete_goal(&mut store, goal, &steps);
        let runs = store.runs().to_vec();
        let approval = store.task(goal).unwrap().approved_plan.clone();
        let comments_before = store.task(goal).unwrap().comments.clone();

        let plan = prepare_comment_edit(&store, goal, "  Add this evidence  ").unwrap();
        assert_eq!(plan.body(), "Add this evidence");
        assert!(plan.impact().is_some());
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);
        assert_eq!(store.task(goal).unwrap().comments.len(), comments_before.len() + 1);
        assert_eq!(store.runs(), runs);

        let undo = applied.undo.clone();
        undo_comment_edit(&mut store, &undo).unwrap();
        assert_eq!(store.task(goal).unwrap().comments, comments_before);
        assert_eq!(store.task(goal).unwrap().stage, Stage::Done);
        assert_eq!(store.task(goal).unwrap().approved_plan, approval);
        assert!(store.goal_execution_approved(goal));
        assert_eq!(store.runs(), runs);
        assert!(steps.iter().all(|id| store.research_result(*id).is_some()));
    }

    #[test]
    fn comment_undo_conflicts_on_newer_comment_runtime_or_input_and_leaves_store_equal() {
        let (mut store, goal, _) = store_with_goal();
        let plan = prepare_comment_edit(&store, goal, "First comment").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.task_mut(goal).unwrap().comments[0].body = "Changed comment".into();
        let before = store.clone();
        assert!(matches!(
            undo_comment_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, _) = store_with_goal();
        let plan = prepare_comment_edit(&store, goal, "First comment").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.task_mut(goal).unwrap().title = "Newer title".into();
        let before = store.clone();
        assert!(matches!(
            undo_comment_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, steps) = store_with_goal();
        let mut run = store.prepare_run(steps[0]).unwrap();
        run.status = "Running".into();
        store.push_run(run);
        let plan = prepare_comment_edit(&store, goal, "Runtime comment").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.run_at_mut(0).unwrap().status = "Succeeded".into();
        store.run_at_mut(0).unwrap().output = "completed body".into();
        store.run_at_mut(0).unwrap().ended_at = Some("2026-11-09 00:00:00 +00:00".into());
        let before = store.clone();
        assert!(matches!(
            undo_comment_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        let (mut store, goal, _) = store_with_goal();
        let plan = prepare_comment_edit(&store, goal, "First comment").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.add_comment(goal, "Later comment").unwrap();
        let before = store.clone();
        assert!(matches!(
            undo_comment_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
    }

    #[test]
    fn comment_undo_stage_restore_failure_rolls_back_the_removed_comment_exactly() {
        let (mut store, goal, steps) = store_with_goal();
        complete_goal(&mut store, goal, &steps);
        let plan = prepare_comment_edit(&store, goal, "Comment on completed goal").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.set_stage(steps[0], Stage::Backlog).unwrap();
        let before = store.clone();
        assert!(matches!(
            undo_comment_edit(&mut store, &applied.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
        assert_eq!(
            store.task(goal).unwrap().comments.len(),
            1,
            "the candidate removal was rolled back"
        );
    }

    #[test]
    fn reopen_requires_a_cancelled_research_step_and_planned_target() {
        let (mut store, goal, steps) = store_with_goal();
        let child = steps[0];
        store.set_stage(child, Stage::Cancelled).unwrap();
        let mut input = input_for(&store, child);
        input.stage = Some(Stage::Planned);
        input.stage_intent = StageIntent::Reopen;
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(store.task(child).unwrap().stage, Stage::Planned);
        undo_task_edit(&mut store, &applied.undo).unwrap();
        assert_eq!(store.task(child).unwrap().stage, Stage::Cancelled);

        let mut forged_done = input_for(&store, child);
        forged_done.stage = Some(Stage::Done);
        forged_done.stage_intent = StageIntent::Reopen;
        assert!(matches!(
            prepare_task_edit(&store, &forged_done),
            Err(ScopedEditError::Rejected(_))
        ));

        store.set_stage(child, Stage::Review).unwrap();
        let mut not_cancelled = input_for(&store, child);
        not_cancelled.stage = Some(Stage::Planned);
        not_cancelled.stage_intent = StageIntent::Reopen;
        assert!(matches!(
            prepare_task_edit(&store, &not_cancelled),
            Err(ScopedEditError::Rejected(_))
        ));

        let ordinary = store.create_task("Ordinary", 1, Stage::Cancelled, None).unwrap();
        let mut ordinary_reopen = input_for(&store, ordinary);
        ordinary_reopen.stage = Some(Stage::Planned);
        ordinary_reopen.stage_intent = StageIntent::Reopen;
        assert!(matches!(
            prepare_task_edit(&store, &ordinary_reopen),
            Err(ScopedEditError::Rejected(_))
        ));
        assert_eq!(store.task(goal).unwrap().stage, Stage::Planned);
    }

    #[test]
    fn comment_undo_preserves_unrelated_metadata() {
        let (mut store, goal, _) = store_with_goal();
        let plan = prepare_comment_edit(&store, goal, "Comment to remove").unwrap();
        let applied = apply_comment_edit(&mut store, &plan).unwrap();
        store.task_mut(goal).unwrap().labels = vec!["Later label".into()];
        undo_comment_edit(&mut store, &applied.undo).unwrap();
        assert!(store.task(goal).unwrap().comments.is_empty());
        assert_eq!(store.task(goal).unwrap().labels, vec!["Later label".to_string()]);
    }

    #[test]
    fn frozen_automation_comment_is_rejected() {
        let mut store = Store::empty();
        let task = store.create_task("Frozen", 1, Stage::Planned, None).unwrap();
        store.task_mut(task).unwrap().automation_id = Some(1);
        assert!(matches!(
            prepare_comment_edit(&store, task, "Frozen comment"),
            Err(ScopedEditError::Rejected(_))
        ));
    }

    #[test]
    fn no_op_plan_does_not_create_an_undo_change() {
        let (store, goal, _) = store_with_goal();
        let input = input_for(&store, goal);
        let plan = prepare_task_edit(&store, &input).unwrap();
        assert_eq!(plan.id(), goal);
        assert_eq!(plan.project(), store.task(goal).unwrap().project_id);
        assert_eq!(plan.intent().project, plan.project());
        assert!(plan.changes().is_empty());
        assert!(plan.stage().is_none());
        assert!(plan.approval_before().is_empty());
        assert!(plan.runtime_before().is_empty());
    }

    #[test]
    fn metadata_change_does_not_emit_approval_companion() {
        let (store, goal, _) = store_with_goal();
        let mut input = input_for(&store, goal);
        input.labels = vec!["Only metadata".into()];
        let plan = prepare_task_edit(&store, &input).unwrap();
        assert!(plan.approval_before().is_empty());
        assert!(plan.runtime_before().is_empty());
    }

    #[test]
    fn runtime_stamp_is_goal_scoped_and_ignores_other_goal_automation() {
        let (mut store, goal, _) = store_with_goal();
        let other = store
            .create_research_goal("Other".into(), "Other brief".into(), 1)
            .unwrap();
        store.apply_research_plan(other, BRANCHED).unwrap();
        let other_child = store.research_children(other)[0];
        let mut run = store.prepare_run(other_child).unwrap();
        run.id = 500;
        run.status = "Succeeded".into();
        run.input_signature = store.research_signature(other_child).unwrap_or_default();
        store.push_run(run);
        store.push_automation(definition(99));
        store.push_occurrence(occurrence(7, 99, other, 2_000));

        let mut input = input_for(&store, goal);
        input.title = "Scoped runtime".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        assert_eq!(plan.runtime_before().len(), 1);
        let stamp = &plan.runtime_before()[0];
        assert!(stamp.runs.is_empty());
        assert!(stamp.occurrences.is_empty());
        assert!(stamp.automations.is_empty());
    }

    #[test]
    fn change_set_names_exact_touched_task_approval_and_activity_entities() {
        let (mut store, goal, steps) = store_with_goal();
        let child = steps[0];
        let mut input = input_for(&store, child);
        input.description = "Touched child".into();
        let plan = prepare_task_edit(&store, &input).unwrap();
        let applied = apply_task_edit(&mut store, &plan).unwrap();
        assert_eq!(applied.change_set.task, child);
        assert_eq!(applied.change_set.fields, vec![TaskEditField::Description]);
        assert_eq!(applied.change_set.approvals.len(), 1);
        assert_eq!(applied.change_set.approvals[0].goal, goal);
        assert_eq!(applied.change_set.activity, dedupe(vec![child, goal]));
    }

    #[test]
    fn editor_project_field_is_carried_and_non_user_project_is_not_special_cased() {
        let (store, goal, _) = store_with_goal();
        let input = input_for(&store, goal);
        assert_eq!(input.project, Some(store.task(goal).unwrap().project_id));
        assert_eq!(
            store.project_at_mut(0).unwrap().kind,
            ProjectKind::User,
            "fixture uses a user project"
        );
        let _ = Priority::Low;
    }
}
