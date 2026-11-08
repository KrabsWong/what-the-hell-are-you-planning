//! T21 Phase 1: pure research-input impact analysis.
//!
//! This module never mutates the live store, never writes storage and never
//! prepares execution. It stages candidate edits on a clone so callers can show
//! a pre-commit impact preview, and it separates two independent facts:
//!
//! * whether the owning goal loses approval and therefore blocks all execution;
//! * which steps' actual inputs changed, directly or through an upstream step.
//!
//! A lost approval is not evidence that every sibling input changed. Unchanged
//! siblings are reported as retained with matching inputs even though execution
//! waits for renewed approval.

#![allow(dead_code)] // Phase 2 consumes these APIs from UI and command boundaries.

use crate::attachments::Attachment;
use crate::domain::{AgentRun, ProjectKind, Stage, Store, Task};
use crate::research::ResearchKind;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const FROZEN_AUTOMATION_MESSAGE: &str = "This task belongs to an automation period. Period execution inputs are frozen. Edit the automation definition and replan future periods; existing periods keep their saved snapshot.";

pub(crate) const MOVE_PROJECT_MESSAGE: &str =
    "Move the entire topic using its Project property or topic menu, then edit task details.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResearchScope {
    OrdinaryGoal(u64),
    OrdinaryStep(u64),
    OrdinaryTask,
    AutomationOwned,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepInputState {
    /// Neither this step's inputs nor an upstream step's inputs changed.
    Unchanged,
    /// This edit changed the step's own, ancestor or project input.
    Changed,
    /// An upstream step's input changed, so this step's dependency input changed too.
    DownstreamAffected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StepImpact {
    pub id: u64,
    pub title: String,
    pub kind: ResearchKind,
    pub input_state: StepInputState,
    pub current_run_before: Option<u64>,
    /// True when the newest retained successful run still matches the current
    /// input signature, independent of approval state.
    pub inputs_match_retained_run: bool,
    pub retained_run: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResearchInputImpact {
    pub scope: ResearchScope,
    pub changed: bool,
    pub goal: u64,
    pub goal_title: String,
    pub signature_changed: bool,
    pub approval_before: bool,
    pub approval_after: bool,
    /// The goal loses approval and every execution path waits for review.
    pub review_required: bool,
    /// The goal was already waiting for review before this proposed change.
    pub approval_already_pending: bool,
    pub current_results: usize,
    /// Results that were current before and will be execution-ineligible after,
    /// solely because the whole-goal approval was withdrawn.
    pub review_blocks_results: usize,
    /// Results whose actual inputs changed directly or through upstream steps.
    pub input_changed_results: usize,
    /// Results on unaffected steps: retained, with inputs still matching.
    pub retained_matching_results: usize,
    pub pre_existing_stale_results: usize,
    pub changed_input_steps: Vec<u64>,
    pub affected_input_steps: Vec<u64>,
    pub steps: Vec<StepImpact>,
    /// Goals outside this goal whose approval changed. Normal edits return empty.
    pub other_goals_affected: Vec<u64>,
    /// No approval or input change; labels, due date, cycle or no-op only.
    pub presentation_only: bool,
    /// A pre-commit confirmation is warranted because retained results exist and
    /// actual inputs changed.
    pub confirmation_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskEditIntent {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub due_date: String,
    pub cycle: Option<String>,
    pub bucket: String,
    pub stage: Stage,
    pub project: u64,
}

impl TaskEditIntent {
    pub(crate) fn from_task(store: &Store, id: u64) -> Option<Self> {
        let task = store.task(id)?;
        Some(Self {
            title: task.title.clone(),
            description: task.description.clone(),
            labels: task.labels.clone(),
            due_date: task.due_date.clone(),
            cycle: task.cycle.clone(),
            bucket: task.bucket.clone(),
            stage: task.stage,
            project: task.project_id,
        })
    }
}

pub(crate) enum FrozenChange<'a> {
    Edit(&'a TaskEditIntent),
    Comment,
    Material,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AutomationEditDecision {
    NotAutomationOwned,
    /// Labels, due date and cycle only; the frozen execution snapshot is untouched.
    MetadataOnly,
    /// Would change a frozen execution input. The message instructs the user to
    /// edit the automation definition and replan future periods.
    FrozenInput {
        fields: Vec<&'static str>,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedQuality {
    /// T21 does not classify reader/evidence quality. T06 owns validation and
    /// freshness labels; a successful non-empty run is not a validated claim.
    NotEvaluated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviousVersion {
    pub run_id: u64,
    pub started_at: String,
    /// Stable digest of the persisted `input_signature`, not a JSON prefix.
    pub revision_digest: Option<String>,
    /// Execution admission per `Store::research_result`. This is not a reader
    /// quality claim; `quality` remains explicitly unevaluated here.
    pub execution_admissible: bool,
    /// The persisted run input equals the current input signature, independent of
    /// approval and of reader quality.
    pub inputs_match_current: bool,
    /// T06-owned quality is intentionally not inferred in this module.
    pub quality: RetainedQuality,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectoryGoalImpact {
    pub goal: u64,
    pub title: String,
    pub changed_input_steps: Vec<u64>,
    pub affected_input_steps: Vec<u64>,
    pub current_results: usize,
    pub retained_matching_results: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectoryChangeImpact {
    pub project: u64,
    pub changed: bool,
    pub goals: Vec<DirectoryGoalImpact>,
    pub current_results: usize,
    pub retained_matching_results: usize,
    /// At least one approved goal has changed execution context and needs review.
    pub review_required: bool,
}

/// Explicit metadata/execution policy for automation-owned period tasks.
///
/// Metadata may change without touching the frozen snapshot. Any execution
/// input, comment or material change is refused here so a later command boundary
/// can enforce the same decision.
pub(crate) fn automation_edit_decision(
    store: &Store,
    id: u64,
    change: FrozenChange<'_>,
) -> AutomationEditDecision {
    let Some(task) = store.task(id) else {
        return AutomationEditDecision::NotAutomationOwned;
    };
    if !automation_owned(store, task) {
        return AutomationEditDecision::NotAutomationOwned;
    }
    let fields = match change {
        FrozenChange::Edit(intent) => {
            let mut fields = Vec::new();
            if intent.title.trim() != task.title {
                fields.push("title");
            }
            if intent.description != task.description {
                fields.push("description");
            }
            if intent.project != task.project_id {
                fields.push("project");
            }
            if intent.stage != task.stage {
                fields.push("stage");
            }
            fields
        }
        FrozenChange::Comment => vec!["comment"],
        FrozenChange::Material => vec!["material"],
    };
    if fields.is_empty() {
        AutomationEditDecision::MetadataOnly
    } else {
        AutomationEditDecision::FrozenInput {
            fields,
            message: FROZEN_AUTOMATION_MESSAGE.into(),
        }
    }
}

/// Preview a task-detail edit exactly as the editor would apply it.
///
/// Title, description, stage and research context are execution inputs. Labels,
/// due date and cycle are presentation metadata and keep completion intact.
/// Project changes are rejected here because the editor already requires the
/// topic/property move path.
pub(crate) fn preview_task_edit(
    store: &Store,
    id: u64,
    intent: &TaskEditIntent,
) -> Result<Option<ResearchInputImpact>, String> {
    let task = store.task(id).ok_or("Task not found.")?;
    if store.task_is_archived(id) {
        return Err("Restore the archived task before editing it.".into());
    }
    match automation_edit_decision(store, id, FrozenChange::Edit(intent)) {
        AutomationEditDecision::FrozenInput { message, .. } => return Err(message),
        AutomationEditDecision::MetadataOnly => {
            if task.research.is_none() {
                return Ok(None);
            }
            let mut after = store.clone();
            {
                let task = after.task_mut(id).expect("validated task");
                task.labels = intent.labels.clone();
                task.due_date = intent.due_date.trim().into();
                task.cycle = intent.cycle.clone();
                task.bucket = intent.bucket.clone();
            }
            after.reconcile_goal_approvals();
            return Ok(impact(store, &after, id, false));
        }
        AutomationEditDecision::NotAutomationOwned => {}
    }
    if task.research.is_none() {
        return Ok(None);
    }
    if intent.project != task.project_id {
        return Err(MOVE_PROJECT_MESSAGE.into());
    }
    let mut after = store.clone();
    if intent.stage != task.stage {
        after.set_stage(id, intent.stage)?;
    }
    {
        let task = after.task_mut(id).expect("validated task");
        task.title = intent.title.trim().into();
        task.description = intent.description.clone();
        task.labels = intent.labels.clone();
        task.due_date = intent.due_date.trim().into();
        task.cycle = intent.cycle.clone();
    }
    after.reconcile_goal_approvals();
    Ok(impact(store, &after, id, false))
}

pub(crate) fn preview_comment(
    store: &Store,
    id: u64,
    body: &str,
) -> Result<Option<ResearchInputImpact>, String> {
    let task = store.task(id).ok_or("Task not found.")?;
    if store.task_is_archived(id) {
        return Err("Restore the archived task before commenting.".into());
    }
    match automation_edit_decision(store, id, FrozenChange::Comment) {
        AutomationEditDecision::FrozenInput { message, .. } => return Err(message),
        AutomationEditDecision::MetadataOnly | AutomationEditDecision::NotAutomationOwned => {}
    }
    if task.research.is_none() {
        return Ok(None);
    }
    if body.trim().is_empty() {
        return Err("Write a comment before posting.".into());
    }
    let mut after = store.clone();
    after.add_comment(id, body)?;
    after.reconcile_goal_approvals();
    Ok(impact(store, &after, id, false))
}

pub(crate) enum MaterialChange<'a> {
    Add(&'a Attachment),
    Remove(usize),
}

pub(crate) fn preview_material_change(
    store: &Store,
    id: u64,
    change: MaterialChange<'_>,
) -> Result<Option<ResearchInputImpact>, String> {
    let task = store.task(id).ok_or("Task not found.")?;
    if store.task_is_archived(id) {
        return Err("Restore the archived task before changing materials.".into());
    }
    match automation_edit_decision(store, id, FrozenChange::Material) {
        AutomationEditDecision::FrozenInput { message, .. } => return Err(message),
        AutomationEditDecision::MetadataOnly | AutomationEditDecision::NotAutomationOwned => {}
    }
    if task.research.is_none() {
        return Ok(None);
    }
    let mut after = store.clone();
    match change {
        MaterialChange::Add(attachment) => {
            after
                .task_mut(id)
                .expect("validated task")
                .attachments
                .push(attachment.clone());
        }
        MaterialChange::Remove(index) => {
            let task = after.task_mut(id).expect("validated task");
            if index >= task.attachments.len() {
                return Err("Choose a material that is still attached to this task.".into());
            }
            task.attachments.remove(index);
        }
    }
    after.reconcile_goal_approvals();
    Ok(impact(store, &after, id, false))
}

pub(crate) fn preview_material_additions(
    store: &Store,
    id: u64,
    attachments: &[Attachment],
) -> Result<Option<ResearchInputImpact>, String> {
    let task = store.task(id).ok_or("Task not found.")?;
    if store.task_is_archived(id) {
        return Err("Restore the archived task before changing materials.".into());
    }
    match automation_edit_decision(store, id, FrozenChange::Material) {
        AutomationEditDecision::FrozenInput { message, .. } => return Err(message),
        AutomationEditDecision::MetadataOnly | AutomationEditDecision::NotAutomationOwned => {}
    }
    if task.research.is_none() {
        return Ok(None);
    }
    if attachments.is_empty() {
        return Ok(None);
    }
    let mut after = store.clone();
    after
        .task_mut(id)
        .expect("validated task")
        .attachments
        .extend(attachments.iter().cloned());
    after.reconcile_goal_approvals();
    Ok(impact(store, &after, id, false))
}

/// Preview a project working-directory change. The project still owns every
/// research goal in it; this helper reports which goals need review without
/// changing the signature format or the live approval state.
pub(crate) fn preview_directory_change(
    store: &Store,
    project: u64,
    new_directory: &str,
) -> Result<DirectoryChangeImpact, String> {
    let Some(current) = store.projects().iter().find(|p| p.id == project) else {
        return Err("Project not found.".into());
    };
    if current.kind == ProjectKind::Automation {
        return Err(FROZEN_AUTOMATION_MESSAGE.into());
    }
    let new_directory = new_directory.trim();
    if new_directory.is_empty() {
        return Err("Enter an existing absolute folder path.".into());
    }
    if current.directory == new_directory {
        return Ok(DirectoryChangeImpact {
            project,
            changed: false,
            goals: Vec::new(),
            current_results: 0,
            retained_matching_results: 0,
            review_required: false,
        });
    }
    let mut after = store.clone();
    after
        .projects
        .iter_mut()
        .find(|p| p.id == project)
        .expect("validated project")
        .directory = new_directory.into();
    let mut goals = Vec::new();
    let mut current_results = 0;
    let mut retained_matching_results = 0;
    let mut review_required = false;
    let goal_ids: Vec<u64> = store
        .tasks()
        .iter()
        .filter(|task| {
            task.project_id == project
                && !task.archived
                && task.automation_id.is_none()
                && task.stage != Stage::Cancelled
                && task
                    .research
                    .as_ref()
                    .is_some_and(|research| research.kind == ResearchKind::Goal)
        })
        .map(|task| task.id)
        .collect();
    for goal in goal_ids {
        let graph = graph_delta(store, &after, goal, true);
        if graph.affected.is_empty() {
            continue;
        }
        let after_sigs = input_signatures(&after, goal);
        let ids: BTreeSet<u64> = input_signatures(store, goal)
            .keys()
            .chain(after_sigs.keys())
            .copied()
            .collect();
        let mut goal_current = 0;
        let mut goal_retained = 0;
        for id in ids {
            if store.research_result(id).is_some() {
                goal_current += 1;
            }
            if graph.affected.contains(&id) {
                continue;
            }
            let latest = latest_successful_run(store, id);
            let after_sig = after_sigs.get(&id).cloned().flatten();
            if latest.is_some_and(|run| {
                after_sig
                    .as_deref()
                    .is_some_and(|signature| signature == run.input_signature)
            }) {
                goal_retained += 1;
            }
        }
        let changed_input_steps: Vec<u64> = graph.direct.iter().copied().collect();
        let affected_input_steps: Vec<u64> = graph.affected.iter().copied().collect();
        let title = store
            .task(goal)
            .map(|task| task.title.clone())
            .unwrap_or_default();
        if store.goal_execution_approved(goal) {
            review_required = true;
        }
        current_results += goal_current;
        retained_matching_results += goal_retained;
        goals.push(DirectoryGoalImpact {
            goal,
            title,
            changed_input_steps,
            affected_input_steps,
            current_results: goal_current,
            retained_matching_results: goal_retained,
        });
    }
    Ok(DirectoryChangeImpact {
        project,
        changed: true,
        goals,
        current_results,
        retained_matching_results,
        review_required,
    })
}

/// Successful runs for a task, newest first, with a stable digest of the persisted
/// input signature. This is presentation metadata only; it never admits execution.
pub(crate) fn previous_versions(store: &Store, id: u64) -> Vec<PreviousVersion> {
    let Some(task) = store.task(id) else {
        return Vec::new();
    };
    if task.research.is_none() {
        return Vec::new();
    }
    let Some(goal) = store.owning_research_goal(id) else {
        return Vec::new();
    };
    let archived = store.task_is_archived(id);
    // Archived history keeps the accepted T22 signature-matched revision as its
    // reference point. Older attempts with the same persisted input therefore
    // remain identifiable without admitting execution. Unarchived work uses the
    // preview-only approval-forced signature.
    let current_signature = if archived {
        store
            .research_history_result(id)
            .map(|run| run.input_signature.clone())
    } else {
        input_signatures(store, goal).remove(&id).flatten()
    };
    let current_run = store.research_result(id).map(|run| run.id);
    let mut runs: Vec<&AgentRun> = store
        .runs()
        .iter()
        .filter(|run| {
            run.task_id == id
                && run.project_plan.is_none()
                && run.research_kind.is_some()
                && run.status == "Succeeded"
                && !run.output.trim().is_empty()
        })
        .collect();
    // Recorded activity time first, deterministic persisted ID only as the tie
    // break. A higher ID is not evidence of a newer attempt after a clock change.
    runs.sort_by(|a, b| compare_runs_newest_first(a, b));
    runs.into_iter()
        .map(|run| PreviousVersion {
            run_id: run.id,
            started_at: run_started_label(run),
            revision_digest: revision_digest(&run.input_signature),
            execution_admissible: current_run == Some(run.id),
            inputs_match_current: current_signature
                .as_deref()
                .is_some_and(|signature| signature == run.input_signature),
            quality: RetainedQuality::NotEvaluated,
        })
        .collect()
}

/// SHA-256 digest of the persisted serialized signature. The digest is stable
/// across processes and is safe to display; the raw JSON prefix is not.
pub(crate) fn revision_digest(signature: &str) -> Option<String> {
    if signature.is_empty() {
        return None;
    }
    let digest = Sha256::digest(signature.as_bytes());
    Some(
        digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

struct GraphDelta {
    direct: BTreeSet<u64>,
    affected: BTreeSet<u64>,
}

fn impact(
    before: &Store,
    after: &Store,
    id: u64,
    context_changed: bool,
) -> Option<ResearchInputImpact> {
    let after_task = after.task(id)?;
    let goal = after.owning_research_goal(id)?;
    let goal_task = after.task(goal)?;
    if goal_task
        .research
        .as_ref()
        .is_none_or(|research| research.kind != ResearchKind::Goal)
    {
        return None;
    }
    let scope = if automation_owned(after, after_task) {
        ResearchScope::AutomationOwned
    } else if after.task_is_archived(id) {
        ResearchScope::Archived
    } else if id == goal {
        ResearchScope::OrdinaryGoal(goal)
    } else {
        ResearchScope::OrdinaryStep(goal)
    };
    let graph = graph_delta(before, after, goal, context_changed);
    let before_sigs = input_signatures(before, goal);
    let after_sigs = input_signatures(after, goal);
    let ids: BTreeSet<u64> = before_sigs
        .keys()
        .chain(after_sigs.keys())
        .copied()
        .collect();
    let mut steps = Vec::new();
    let mut current_results = 0;
    let mut input_changed_results = 0;
    let mut retained_matching_results = 0;
    let mut pre_existing_stale_results = 0;
    for step in ids {
        let Some(task) = after.task(step) else {
            continue;
        };
        let Some(kind) = task.research.as_ref().map(|research| research.kind) else {
            continue;
        };
        let latest = latest_successful_run(before, step);
        let after_sig = after_sigs.get(&step).cloned().flatten();
        let input_state = if graph.direct.contains(&step) {
            StepInputState::Changed
        } else if graph.affected.contains(&step) {
            StepInputState::DownstreamAffected
        } else {
            StepInputState::Unchanged
        };
        let inputs_match = latest.is_some_and(|run| {
            after_sig
                .as_deref()
                .is_some_and(|signature| signature == run.input_signature)
        });
        let current_run_before = before.research_result(step).map(|run| run.id);
        if current_run_before.is_some() {
            current_results += 1;
        }
        if input_state != StepInputState::Unchanged && latest.is_some() {
            input_changed_results += 1;
        }
        if !graph.affected.contains(&step) {
            if latest.is_some() && inputs_match {
                retained_matching_results += 1;
            } else if latest.is_some() {
                pre_existing_stale_results += 1;
            }
        }
        steps.push(StepImpact {
            id: step,
            title: task.title.clone(),
            kind,
            input_state,
            current_run_before,
            inputs_match_retained_run: inputs_match,
            retained_run: latest.map(|run| run.id),
        });
    }
    let changed_input_steps: Vec<u64> = graph.direct.iter().copied().collect();
    let affected_input_steps: Vec<u64> = graph.affected.iter().copied().collect();
    let approval_before = before.goal_execution_approved(goal);
    let approval_after = after.goal_execution_approved(goal);
    let review_required = approval_before && !approval_after;
    let approval_already_pending = !approval_before
        && goal_task
            .research
            .as_ref()
            .is_some_and(|research| research.requires_approval);
    let signature_changed = before.goal_plan_signature(goal) != after.goal_plan_signature(goal);
    let changed = before != after;
    let presentation_only = changed
        && !signature_changed
        && changed_input_steps.is_empty()
        && affected_input_steps.is_empty()
        && approval_before == approval_after;
    // A pre-commit explanation is required whenever approval is withdrawn or the
    // actual input graph changed, even when no successful result exists yet or all
    // retained history was already stale. Result counts qualify the message, not
    // whether the user sees it.
    let confirmation_required = review_required || !affected_input_steps.is_empty();
    let mut other_goals_affected = Vec::new();
    for task in after.tasks() {
        if task.id == goal || task.archived {
            continue;
        }
        if task
            .research
            .as_ref()
            .is_some_and(|research| research.kind == ResearchKind::Goal)
            && before.goal_execution_approved(task.id) != after.goal_execution_approved(task.id)
        {
            other_goals_affected.push(task.id);
        }
    }
    other_goals_affected.sort_unstable();
    Some(ResearchInputImpact {
        scope,
        changed,
        goal,
        goal_title: goal_task.title.clone(),
        signature_changed,
        approval_before,
        approval_after,
        review_required,
        approval_already_pending,
        current_results,
        review_blocks_results: if approval_after { 0 } else { current_results },
        input_changed_results,
        retained_matching_results,
        pre_existing_stale_results,
        changed_input_steps,
        affected_input_steps,
        steps,
        other_goals_affected,
        presentation_only,
        confirmation_required,
    })
}

/// Field-level input delta plus dependency propagation. This is intentionally not
/// a second signature: it answers only "what input changed for this edit", while
/// the shared signatures remain the source of truth for result matching.
fn graph_delta(before: &Store, after: &Store, goal: u64, context_changed: bool) -> GraphDelta {
    let ids: BTreeSet<u64> = before
        .research_children(goal)
        .into_iter()
        .chain(after.research_children(goal))
        .collect();
    let goal_changed = context_changed
        || match (before.task(goal), after.task(goal)) {
            (Some(before), Some(after)) => goal_own_input(before) != goal_own_input(after),
            _ => true,
        };
    let mut direct = BTreeSet::new();
    for id in &ids {
        if goal_changed {
            direct.insert(*id);
            continue;
        }
        let changed = match (before.task(*id), after.task(*id)) {
            (Some(before), Some(after)) => step_own_input(before) != step_own_input(after),
            _ => true,
        };
        if changed {
            direct.insert(*id);
        }
    }
    let mut dependencies: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for store in [before, after] {
        for id in &ids {
            if let Some(research) = store.task(*id).and_then(|task| task.research.as_ref()) {
                dependencies
                    .entry(*id)
                    .or_default()
                    .extend(research.depends_on.iter().copied());
            }
        }
    }
    let mut affected = direct.clone();
    loop {
        let mut grew = false;
        for (id, dependencies) in &dependencies {
            if !affected.contains(id)
                && dependencies
                    .iter()
                    .any(|dependency| affected.contains(dependency))
            {
                affected.insert(*id);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    GraphDelta { direct, affected }
}

fn goal_own_input(task: &Task) -> serde_json::Value {
    serde_json::json!([
        task.title,
        task.description,
        task.comments,
        task.attachments,
        task.project_id,
    ])
}

fn step_own_input(task: &Task) -> serde_json::Value {
    serde_json::json!([
        task.title,
        task.description,
        task.comments,
        task.attachments,
        task.parent_id,
        task.project_id,
        task.archived,
        task.research.as_ref().map(|research| research.kind),
        task.research
            .as_ref()
            .map(|research| research.depends_on.clone()),
    ])
}

fn input_signatures(store: &Store, goal: u64) -> BTreeMap<u64, Option<String>> {
    let forced = force_goal_approval(store, goal);
    forced
        .research_children(goal)
        .into_iter()
        .map(|id| (id, forced.research_signature(id)))
        .collect()
}

fn force_goal_approval(store: &Store, goal: u64) -> Store {
    let mut forced = store.clone();
    let Some(task) = forced.task(goal) else {
        return forced;
    };
    let Some(research) = task.research.as_ref() else {
        return forced;
    };
    if research.kind != ResearchKind::Goal {
        return forced;
    }
    {
        let task = forced.task_mut(goal).expect("validated goal");
        if matches!(task.stage, Stage::Backlog | Stage::Cancelled) {
            task.stage = Stage::Planned;
        }
        if let Some(research) = task.research.as_mut() {
            research.requires_approval = true;
        }
    }
    let signature = forced.goal_plan_signature(goal);
    forced.task_mut(goal).expect("validated goal").approved_plan = signature;
    forced
}

/// Recorded attempt time used by result-history ordering. This mirrors the T16
/// presentation contract: read the stored timestamp, tolerate legacy/invalid
/// values, and never infer recency from the run ID.
pub(crate) fn run_recorded_at(run: &AgentRun) -> Option<i64> {
    [&run.started_at, &run.queued_at]
        .into_iter()
        .find_map(|value| {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            chrono::DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S %:z")
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(value))
                .ok()
                .map(|date| date.timestamp_millis())
        })
}

fn run_started_label(run: &AgentRun) -> String {
    if !run.started_at.trim().is_empty() {
        run.started_at.clone()
    } else {
        run.queued_at.clone()
    }
}

/// Ordering key where a greater value means a later recorded attempt. Missing or
/// unparseable legacy timestamps sort after known times; the persisted ID is the
/// deterministic tie-break only.
pub(crate) fn run_activity_order_key(run: &AgentRun) -> (Option<i64>, u64) {
    (run_recorded_at(run), run.id)
}

/// Newest-first comparator for result history. Clock rollback remains visible as
/// the recorded order; equal times fall back to the deterministic ID tie-break.
pub(crate) fn compare_runs_newest_first(a: &AgentRun, b: &AgentRun) -> Ordering {
    run_activity_order_key(b).cmp(&run_activity_order_key(a))
}

fn latest_successful_run(store: &Store, id: u64) -> Option<&AgentRun> {
    store
        .runs()
        .iter()
        .filter(|run| {
            run.task_id == id
                && run.project_plan.is_none()
                && run.research_kind.is_some()
                && run.status == "Succeeded"
                && !run.output.trim().is_empty()
        })
        .max_by_key(|run| run_activity_order_key(run))
}

/// Public read-only ownership check for UI copy. It never authorizes a mutation.
pub(crate) fn automation_owned_task(store: &Store, id: u64) -> bool {
    store
        .task(id)
        .is_some_and(|task| automation_owned(store, task))
}

fn automation_owned(store: &Store, task: &Task) -> bool {
    task.automation_id.is_some()
        || store
            .projects()
            .iter()
            .find(|project| project.id == task.project_id)
            .is_some_and(|project| project.kind == ProjectKind::Automation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Priority;

    const BRANCHED: &str = r#"{"tasks":[
        {"title":"Collect A","instructions":"Collect from source A","kind":"collect","depends_on":[]},
        {"title":"Collect B","instructions":"Collect from source B","kind":"collect","depends_on":[]},
        {"title":"Analyze A","instructions":"Analyze source A","kind":"analyze","depends_on":[0]},
        {"title":"Report","instructions":"Write the final report","kind":"advise","depends_on":[2]}
    ]}"#;

    fn approve(store: &mut Store, goal: u64) {
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
    }

    fn complete(store: &mut Store, steps: &[u64]) {
        for id in steps {
            let mut run = store.prepare_run(*id).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Completed result for {id}");
            store.push_run(run);
        }
    }

    /// Goal A (project 1) with a branched DAG: 0 -> 2 -> 3 and independent 1.
    /// Goal B (project 2) is a separate three-step goal with completed results.
    fn fixture() -> (Store, u64, Vec<u64>, u64, Vec<u64>) {
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = "/tmp/t21-one".into();
        store.project_at_mut(1).unwrap().directory = "/tmp/t21-two".into();
        store.research_config_mut().models =
            ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
        let goal = store
            .create_research_goal("Goal A".into(), "Scope A".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, BRANCHED).unwrap();
        approve(&mut store, goal);
        complete(&mut store, &steps);
        let other = store
            .create_research_goal("Goal B".into(), "Scope B".into(), 2)
            .unwrap();
        let other_steps = store.apply_research_plan(other, BRANCHED).unwrap();
        approve(&mut store, other);
        complete(&mut store, &other_steps);
        (store, goal, steps, other, other_steps)
    }

    fn step(impact: &ResearchInputImpact, id: u64) -> &StepImpact {
        impact.steps.iter().find(|step| step.id == id).unwrap()
    }

    #[test]
    fn title_edit_previews_approval_withdrawal_and_the_actual_input_graph() {
        let (store, goal, steps, other, _) = fixture();
        let before = store.clone();
        let mut intent = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        intent.title = "Collect A with a revised scope".into();

        let impact = preview_task_edit(&store, steps[0], &intent)
            .unwrap()
            .unwrap();
        assert_eq!(store, before, "preview must not mutate the live store");
        assert_eq!(impact.scope, ResearchScope::OrdinaryStep(goal));
        assert!(impact.changed);
        assert!(impact.signature_changed);
        assert!(impact.review_required);
        assert!(!impact.approval_already_pending);
        assert_eq!(impact.current_results, 4);
        assert_eq!(impact.review_blocks_results, 4);
        assert_eq!(impact.input_changed_results, 3);
        assert_eq!(impact.retained_matching_results, 1);
        assert_eq!(impact.changed_input_steps, vec![steps[0]]);
        assert_eq!(
            impact.affected_input_steps,
            vec![steps[0], steps[2], steps[3]]
        );
        assert!(impact.confirmation_required);
        assert!(impact.other_goals_affected.is_empty());
        assert!(other > 0);

        assert_eq!(step(&impact, steps[0]).input_state, StepInputState::Changed);
        assert_eq!(
            step(&impact, steps[2]).input_state,
            StepInputState::DownstreamAffected
        );
        assert_eq!(
            step(&impact, steps[3]).input_state,
            StepInputState::DownstreamAffected
        );
        assert_eq!(
            step(&impact, steps[1]).input_state,
            StepInputState::Unchanged
        );
        assert!(step(&impact, steps[1]).inputs_match_retained_run);
        assert!(!step(&impact, steps[0]).inputs_match_retained_run);
        assert_eq!(
            step(&impact, steps[1]).current_run_before,
            Some(step(&impact, steps[1]).retained_run.unwrap())
        );
    }

    #[test]
    fn goal_title_edit_changes_every_descendant_but_not_another_goal() {
        let (store, goal, steps, other, other_steps) = fixture();
        let mut intent = TaskEditIntent::from_task(&store, goal).unwrap();
        intent.title = "Goal A revised".into();
        let impact = preview_task_edit(&store, goal, &intent).unwrap().unwrap();
        assert_eq!(impact.scope, ResearchScope::OrdinaryGoal(goal));
        assert_eq!(impact.changed_input_steps.len(), steps.len());
        assert_eq!(impact.affected_input_steps.len(), steps.len());
        assert_eq!(impact.retained_matching_results, 0);
        assert_eq!(impact.input_changed_results, 4);
        assert!(impact.other_goals_affected.is_empty());
        assert!(store.goal_execution_approved(other));
        for id in &other_steps {
            assert!(store.research_result(*id).is_some());
        }
    }

    #[test]
    fn labels_due_date_cycle_and_priority_are_presentation_only() {
        let (mut store, goal, steps, _, _) = fixture();
        let mut intent = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        intent.labels = vec!["Presentation".into()];
        intent.due_date = "2026-12-01".into();
        intent.cycle = Some("Q4".into());
        let impact = preview_task_edit(&store, steps[0], &intent)
            .unwrap()
            .unwrap();
        assert!(impact.changed);
        assert!(impact.presentation_only);
        assert!(!impact.signature_changed);
        assert!(!impact.review_required);
        assert!(!impact.confirmation_required);
        assert_eq!(impact.input_changed_results, 0);
        assert_eq!(impact.retained_matching_results, 4);
        assert!(store.goal_execution_approved(goal));

        store.task_mut(steps[0]).unwrap().priority = Priority::Urgent;
        store.reconcile_goal_approvals();
        assert!(store.goal_execution_approved(goal));
        assert!(store.research_result(steps[0]).is_some());
        assert_eq!(store.task(goal).unwrap().stage, Stage::Planned);
    }

    #[test]
    fn metadata_and_noop_edits_leave_prompts_byte_identical() {
        let (store, _, steps, _, _) = fixture();
        let base = store.prepare_run(steps[0]).unwrap().prompt;

        let mut metadata = store.clone();
        {
            let task = metadata.task_mut(steps[0]).unwrap();
            task.labels = vec!["Metadata".into()];
            task.due_date = "2026-12-31".into();
            task.cycle = Some("Q4".into());
            task.priority = Priority::Low;
        }
        assert_eq!(metadata.prepare_run(steps[0]).unwrap().prompt, base);

        let mut title = store.clone();
        title.task_mut(steps[0]).unwrap().title = "Changed title".into();
        approve(&mut title, store.owning_research_goal(steps[0]).unwrap());
        assert_ne!(title.prepare_run(steps[0]).unwrap().prompt, base);

        let mut description = store.clone();
        description.task_mut(steps[0]).unwrap().description = "Changed instructions".into();
        approve(
            &mut description,
            store.owning_research_goal(steps[0]).unwrap(),
        );
        assert_ne!(description.prepare_run(steps[0]).unwrap().prompt, base);

        let mut comments = store.clone();
        comments
            .add_comment(steps[0], "Clarify the source")
            .unwrap();
        approve(&mut comments, store.owning_research_goal(steps[0]).unwrap());
        assert_ne!(comments.prepare_run(steps[0]).unwrap().prompt, base);

        let no_op = preview_task_edit(
            &store,
            steps[0],
            &TaskEditIntent::from_task(&store, steps[0]).unwrap(),
        )
        .unwrap()
        .unwrap();
        assert!(!no_op.changed);
        assert!(!no_op.review_required);
        assert!(!no_op.confirmation_required);
    }

    #[test]
    fn comment_preview_is_scoped_to_the_changed_branch() {
        let (store, goal, steps, _, _) = fixture();
        let impact = preview_comment(&store, steps[0], "Clarify source A")
            .unwrap()
            .unwrap();
        assert!(impact.review_required);
        assert_eq!(impact.changed_input_steps, vec![steps[0]]);
        assert_eq!(
            impact.affected_input_steps,
            vec![steps[0], steps[2], steps[3]]
        );
        assert_eq!(
            step(&impact, steps[1]).input_state,
            StepInputState::Unchanged
        );
        assert_eq!(store.task(goal).unwrap().stage, Stage::Planned);
        assert!(store.goal_execution_approved(goal));
        assert!(store.research_result(steps[1]).is_some());
    }

    #[test]
    fn material_add_and_remove_preview_match_the_committed_invalidation() {
        let (mut store, goal, steps, _, _) = fixture();
        let material = Attachment {
            name: "Scope A.pdf".into(),
            relative_path: "attachments/scope-a.pdf".into(),
            media_type: "application/pdf".into(),
            size: 128,
            content_sha256: String::new(),
        };
        let added = preview_material_change(&store, steps[0], MaterialChange::Add(&material))
            .unwrap()
            .unwrap();
        assert!(added.review_required);
        assert!(added.changed_input_steps.contains(&steps[0]));
        assert!(added.affected_input_steps.contains(&steps[2]));
        assert!(!added.affected_input_steps.contains(&steps[1]));
        assert!(store.task(steps[0]).unwrap().attachments.is_empty());

        store
            .task_mut(steps[0])
            .unwrap()
            .attachments
            .push(material.clone());
        approve(&mut store, goal);
        let removed = preview_material_change(&store, steps[0], MaterialChange::Remove(0))
            .unwrap()
            .unwrap();
        assert!(removed.review_required);
        assert_eq!(removed.changed_input_steps, vec![steps[0]]);
        assert_eq!(
            removed.affected_input_steps,
            vec![steps[0], steps[2], steps[3]]
        );
        assert_eq!(store.task(steps[0]).unwrap().attachments.len(), 1);
    }

    #[test]
    fn stage_withdrawal_blocks_execution_without_claiming_input_changes() {
        let (store, goal, _, _, _) = fixture();
        let mut intent = TaskEditIntent::from_task(&store, goal).unwrap();
        intent.stage = Stage::Backlog;
        let impact = preview_task_edit(&store, goal, &intent).unwrap().unwrap();
        assert!(impact.review_required);
        assert!(!impact.signature_changed);
        assert!(impact.changed_input_steps.is_empty());
        assert!(impact.affected_input_steps.is_empty());
        assert_eq!(impact.review_blocks_results, 4);
        assert_eq!(impact.input_changed_results, 0);
        assert!(
            impact.confirmation_required,
            "approval withdrawal always gets a preview"
        );
        assert!(!impact.presentation_only);
        assert!(store.goal_execution_approved(goal));
    }

    #[test]
    fn automation_period_inputs_are_frozen_and_metadata_is_explicit() {
        let (mut store, goal, _, _, _) = fixture();
        store.task_mut(goal).unwrap().automation_id = Some(7);

        let mut title = TaskEditIntent::from_task(&store, goal).unwrap();
        title.title = "Frozen title".into();
        let decision = automation_edit_decision(&store, goal, FrozenChange::Edit(&title));
        assert!(matches!(
            decision,
            AutomationEditDecision::FrozenInput { ref fields, .. } if fields == &vec!["title"]
        ));
        let error = preview_task_edit(&store, goal, &title).unwrap_err();
        assert!(error.contains("automation period"));
        assert!(preview_comment(&store, goal, "Frozen comment").is_err());
        assert!(
            preview_material_change(
                &store,
                goal,
                MaterialChange::Add(&Attachment {
                    name: "frozen.pdf".into(),
                    relative_path: "attachments/frozen.pdf".into(),
                    media_type: "application/pdf".into(),
                    size: 1,
                    content_sha256: String::new(),
                })
            )
            .is_err()
        );

        let mut stage = TaskEditIntent::from_task(&store, goal).unwrap();
        stage.stage = Stage::Backlog;
        assert!(matches!(
            automation_edit_decision(&store, goal, FrozenChange::Edit(&stage)),
            AutomationEditDecision::FrozenInput { ref fields, .. } if fields.contains(&"stage")
        ));

        let mut metadata = TaskEditIntent::from_task(&store, goal).unwrap();
        metadata.labels = vec!["Automation metadata".into()];
        assert_eq!(
            automation_edit_decision(&store, goal, FrozenChange::Edit(&metadata)),
            AutomationEditDecision::MetadataOnly
        );
        let impact = preview_task_edit(&store, goal, &metadata).unwrap().unwrap();
        assert_eq!(impact.scope, ResearchScope::AutomationOwned);
        assert!(impact.presentation_only);
        assert!(!impact.review_required);
        assert_eq!(
            store.task(goal).unwrap().labels,
            vec!["Research".to_string()]
        );
    }

    #[test]
    fn archived_edits_are_blocked_but_original_history_is_retained() {
        let (mut store, goal, steps, _, _) = fixture();
        let original_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .clone();
        store.task_mut(goal).unwrap().archived = true;
        store.reconcile_goal_approvals();
        let intent = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        assert!(preview_task_edit(&store, steps[0], &intent).is_err());
        assert!(preview_comment(&store, steps[0], "archived").is_err());

        let versions = previous_versions(&store, steps[0]);
        assert_eq!(versions.len(), 1, "archive must not drop retained versions");
        assert_eq!(versions[0].run_id, original_run.id);
        assert!(!versions[0].execution_admissible);
        assert!(versions[0].revision_digest.is_some());
        assert_eq!(
            store.runs().iter().find(|run| run.id == original_run.id).unwrap().output,
            original_run.output
        );
    }

    #[test]
    fn previous_versions_keep_old_revision_and_matching_siblings() {
        let (mut store, _, steps, _, _) = fixture();
        store.task_mut(steps[0]).unwrap().title = "Collect A revised".into();
        store.reconcile_goal_approvals();
        assert!(!store.goal_execution_approved(store.owning_research_goal(steps[0]).unwrap()));

        let versions = previous_versions(&store, steps[0]);
        assert_eq!(versions.len(), 1);
        let old = &versions[0];
        assert!(!old.execution_admissible);
        assert!(!old.inputs_match_current);
        assert!(!old.revision_digest.as_ref().unwrap().contains('{'));
        assert_eq!(old.revision_digest.as_ref().unwrap().len(), 16);

        let sibling = previous_versions(&store, steps[1]);
        assert_eq!(sibling.len(), 1);
        assert!(
            !sibling[0].execution_admissible,
            "approval withdrawal blocks execution"
        );
        assert!(
            sibling[0].inputs_match_current,
            "an unchanged sibling is retained with matching inputs"
        );

        let digest = revision_digest("{\"stable\":true}").unwrap();
        assert_eq!(digest, revision_digest("{\"stable\":true}").unwrap());
        assert_ne!(digest, revision_digest("{\"stable\":false}").unwrap());
        assert_eq!(revision_digest(""), None);
        assert_eq!(store.runs().len(), 8, "history reading never rewrites runs");
    }

    #[test]
    fn directory_change_preview_is_scoped_to_the_project_and_does_not_mutate() {
        let (mut store, goal, steps, other, _) = fixture();
        let before = store.clone();
        let impact = preview_directory_change(&store, 1, "/tmp/t21-one-renamed").unwrap();
        assert!(impact.changed);
        assert!(impact.review_required);
        assert_eq!(impact.goals.len(), 1);
        assert_eq!(impact.goals[0].goal, goal);
        assert_eq!(impact.goals[0].changed_input_steps.len(), steps.len());
        assert_eq!(impact.goals[0].affected_input_steps.len(), steps.len());
        assert_eq!(impact.goals[0].retained_matching_results, 0);
        assert_eq!(impact.current_results, 4);
        assert!(store.goal_execution_approved(other));
        assert_eq!(store, before);

        let unchanged = preview_directory_change(&store, 1, "/tmp/t21-one").unwrap();
        assert!(!unchanged.changed);
        assert!(unchanged.goals.is_empty());
        assert!(!unchanged.review_required);

        let second = preview_directory_change(&store, 2, "/tmp/t21-two-renamed").unwrap();
        assert_eq!(second.goals.len(), 1);
        assert_eq!(second.goals[0].goal, other);

        let automation_project = store
            .ensure_automation_project("/tmp/t21-auto".into())
            .unwrap();
        assert!(preview_directory_change(&store, automation_project, "/tmp/t21-auto-new").is_err());
    }

    #[test]
    fn unchanged_load_and_metadata_reconciliation_keep_saved_approvals() {
        let (mut store, goal, _, _, _) = fixture();
        store.reconcile_goal_approvals();
        assert!(store.goal_execution_approved(goal));
        assert_eq!(store.task(goal).unwrap().stage, Stage::Planned);
        let before = store.clone();
        store.reconcile_goal_approvals();
        assert_eq!(store, before);
    }

    #[test]
    fn directory_preview_without_any_run_still_reports_changed_inputs_and_review() {
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = "/tmp/t21-empty".into();
        store.research_config_mut().models =
            ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
        let goal = store
            .create_research_goal("Empty goal".into(), "No runs yet".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, BRANCHED).unwrap();
        approve(&mut store, goal);
        let impact = preview_directory_change(&store, 1, "/tmp/t21-empty-moved").unwrap();
        assert_eq!(impact.goals.len(), 1);
        assert_eq!(impact.goals[0].goal, goal);
        assert_eq!(impact.goals[0].changed_input_steps.len(), steps.len());
        assert_eq!(impact.goals[0].affected_input_steps.len(), steps.len());
        assert_eq!(impact.current_results, 0);
        assert_eq!(impact.retained_matching_results, 0);
        assert!(
            impact.review_required,
            "directory context is execution input"
        );
    }

    #[test]
    fn pre_existing_missing_upstream_is_not_attributed_to_a_metadata_edit() {
        let (mut store, goal, steps, _, _) = fixture();
        // Keep a successful analysis run but remove its upstream evidence run.
        // The analysis is now pre-existing invalid, not invalidated by a new edit.
        let evidence_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .id;
        store.retain_runs(|run| run.id != evidence_run);
        assert!(store.research_result(steps[2]).is_none());
        assert!(store.goal_execution_approved(goal));

        let mut metadata = TaskEditIntent::from_task(&store, steps[1]).unwrap();
        metadata.labels = vec!["Metadata only".into()];
        let no_op = preview_task_edit(&store, steps[1], &metadata)
            .unwrap()
            .unwrap();
        assert!(no_op.affected_input_steps.is_empty());
        assert_eq!(no_op.input_changed_results, 0);
        assert_eq!(no_op.pre_existing_stale_results, 2);
        assert!(no_op.presentation_only);
        assert!(no_op.retained_matching_results >= 1);

        let mut upstream = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        upstream.title = "Collect A revised".into();
        let changed = preview_task_edit(&store, steps[0], &upstream)
            .unwrap()
            .unwrap();
        assert_eq!(changed.changed_input_steps, vec![steps[0]]);
        assert_eq!(
            changed.affected_input_steps,
            vec![steps[0], steps[2], steps[3]]
        );
        assert!(!changed.affected_input_steps.contains(&steps[1]));
        assert_eq!(
            changed.input_changed_results, 2,
            "only retained downstream results count"
        );
        assert!(changed.confirmation_required);
        assert!(changed.retained_matching_results >= 1);
    }

    #[test]
    fn retained_versions_expose_input_match_and_quality_as_separate_axes() {
        let (store, _, steps, _, _) = fixture();
        let versions = previous_versions(&store, steps[0]);
        assert_eq!(versions.len(), 1);
        assert!(versions[0].execution_admissible);
        assert!(versions[0].inputs_match_current);
        assert_eq!(versions[0].quality, RetainedQuality::NotEvaluated);
    }

    #[test]
    fn input_edit_without_any_retained_result_still_requires_confirmation() {
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = "/tmp/t21-empty-confirm".into();
        store.research_config_mut().models =
            ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
        let goal = store
            .create_research_goal("Never run".into(), "No results".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, BRANCHED).unwrap();
        approve(&mut store, goal);

        let mut intent = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        intent.title = "Change before the first run".into();
        let impact = preview_task_edit(&store, steps[0], &intent)
            .unwrap()
            .unwrap();
        assert_eq!(impact.current_results, 0);
        assert_eq!(impact.retained_matching_results, 0);
        assert!(impact.input_changed_results == 0);
        assert!(impact.review_required);
        assert!(
            impact.confirmation_required,
            "approval loss must be previewed without results"
        );
        assert_eq!(impact.changed_input_steps, vec![steps[0]]);
        assert!(impact.affected_input_steps.contains(&steps[3]));
    }

    #[test]
    fn pending_review_with_no_history_still_requires_the_input_graph_preview() {
        let (mut store, goal, steps, _, _) = fixture();
        store.task_mut(goal).unwrap().stage = Stage::Backlog;
        store.task_mut(goal).unwrap().approved_plan = None;
        assert!(!store.goal_execution_approved(goal));

        let mut intent = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        intent.description = "Change while already awaiting review".into();
        let impact = preview_task_edit(&store, steps[0], &intent)
            .unwrap()
            .unwrap();
        assert!(!impact.review_required);
        assert!(impact.approval_already_pending);
        assert_eq!(impact.current_results, 0);
        assert!(
            impact.confirmation_required,
            "the changed graph still needs explanation"
        );
        assert_eq!(impact.changed_input_steps, vec![steps[0]]);
        assert_eq!(
            impact.affected_input_steps,
            vec![steps[0], steps[2], steps[3]]
        );
    }

    #[test]
    fn run_history_uses_recorded_time_then_id_with_legacy_and_clock_rollback() {
        let (mut store, _, steps, _, _) = fixture();
        let base_id = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .id;
        store
            .run_mut(base_id)
            .unwrap()
            .started_at = "2026-11-08 10:00:00 +08:00".into();
        let base = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .clone();
        let mut later = base.clone();
        later.id = 10_000;
        later.started_at = "2026-11-08 12:00:00 +08:00".into();
        store.push_run(later.clone());
        let mut rolled_back = base.clone();
        rolled_back.id = 10_001;
        rolled_back.started_at = "2026-11-08 11:00:00 +08:00".into();
        store.push_run(rolled_back.clone());
        let mut legacy = base.clone();
        legacy.id = 10_002;
        legacy.started_at.clear();
        legacy.queued_at.clear();
        store.push_run(legacy.clone());
        let mut tied = base.clone();
        tied.id = 10_003;
        tied.started_at = later.started_at.clone();
        store.push_run(tied.clone());
        let mut queued_fallback = base.clone();
        queued_fallback.id = 10_004;
        queued_fallback.started_at = "invalid legacy start".into();
        queued_fallback.queued_at = "2026-11-08 09:00:00 +08:00".into();
        store.push_run(queued_fallback.clone());

        let versions = previous_versions(&store, steps[0]);
        let ids: Vec<u64> = versions.iter().map(|version| version.run_id).collect();
        assert_eq!(
            ids,
            vec![
                tied.id,
                later.id,
                rolled_back.id,
                base.id,
                queued_fallback.id,
                legacy.id
            ],
            "timestamp first, ID only for equal times; legacy records sort last"
        );
        assert_eq!(versions.last().unwrap().started_at, "");
        assert_eq!(
            compare_runs_newest_first(&later, &rolled_back),
            Ordering::Less,
            "a higher run ID does not make a clock-rolled-back attempt newer"
        );
        assert_eq!(run_recorded_at(&later), Some(later_time_ms()));
        assert_eq!(run_recorded_at(&legacy), None);
        assert_eq!(
            run_recorded_at(&queued_fallback),
            Some(
                chrono::DateTime::parse_from_str(
                    "2026-11-08 09:00:00 +08:00",
                    "%Y-%m-%d %H:%M:%S %:z"
                )
                .unwrap()
                .timestamp_millis()
            )
        );

        let mut metadata = TaskEditIntent::from_task(&store, steps[0]).unwrap();
        metadata.labels = vec!["Timing".into()];
        let impact = preview_task_edit(&store, steps[0], &metadata)
            .unwrap()
            .unwrap();
        assert_eq!(step(&impact, steps[0]).retained_run, Some(tied.id));
    }

    fn later_time_ms() -> i64 {
        chrono::DateTime::parse_from_str("2026-11-08 12:00:00 +08:00", "%Y-%m-%d %H:%M:%S %:z")
            .unwrap()
            .timestamp_millis()
    }
}
