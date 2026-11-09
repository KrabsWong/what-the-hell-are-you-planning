//! T13 S3b: the real task-editor save adapter.
//!
//! The accepted pure [`crate::scoped_edit`] module owns planning, atomic typed
//! application and the conditional inverse. This module only connects that
//! domain primitive to the live editor, synchronous storage receipt, pending
//! review/retry UI and the scoped Undo stack.
//!
//! One commit-time Store clone replaces the previous `save_draft` clone: the
//! candidate is built from the current live store, never from a plan-time base.
//! Synchronous legacy persistence remains until T14; no second clone or
//! background worker is introduced here.

use crate::app::TaskApp;
use crate::draft_exit::TaskEditValues;
use crate::scoped_edit::{
    apply_comment_edit, apply_task_edit, prepare_comment_edit, prepare_task_edit,
    rebase_task_edit, CommentEditPlan, ScopedEditError, StageIntent, TaskEditField, TaskEditInput,
    TaskEditPlan, TaskEditValue,
};
use gpui_kit::*;

/// The fully prepared save. `input` is retained for retry; `plan` carries the
/// reviewed T21 impact and typed deltas.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedTaskSave {
    pub input: TaskEditInput,
    pub plan: TaskEditPlan,
    /// Raw editor values at review time. A later draft edit is a fresh intent;
    /// unrelated Store changes are not.
    pub reviewed: TaskEditValues,
}

/// A failed task-edit write retains the exact prepared intent. Retry re-prepares
/// against the current store instead of replaying a stale staged Store.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingTaskEdit {
    pub prepared: PreparedTaskSave,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedCommentSave {
    pub task: u64,
    pub body: String,
    pub plan: CommentEditPlan,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingCommentEdit {
    pub prepared: PreparedCommentSave,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TaskSaveOutcome {
    Saved,
    /// Accepted and queued in memory, not yet durable. Never treated as Saved.
    Saving,
    NoChanges,
    PendingImpact,
    PendingDurability(String),
    RetryableFailure(String),
    Rejected(String),
}

impl TaskSaveOutcome {
    pub(crate) fn is_saved(&self) -> bool {
        matches!(self, Self::Saved | Self::NoChanges)
    }
}

impl TaskApp {
    /// Build the real editor payload from the live draft. Priority is not edited
    /// in this form and therefore preserves the current task value.
    pub(crate) fn draft_editor_values(&self, cx: &Context<Self>) -> Option<TaskEditValues> {
        let draft = self.draft.as_ref()?;
        let labels: Vec<String> = draft
            .labels
            .read(cx)
            .value()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        Some(TaskEditValues {
            title: draft.title.read(cx).value().to_string(),
            description: draft.description.read(cx).value().to_string(),
            labels,
            due_date: draft.due.read(cx).value().to_string(),
            project: draft.project,
            stage: draft.stage,
            cycle: draft.cycle.clone(),
        })
    }

    pub(crate) fn task_edit_input(&self, cx: &Context<Self>) -> Option<TaskEditInput> {
        let current_values = self.draft_editor_values(cx)?;
        let task = self.store.task(self.draft.as_ref()?.id)?;
        let baseline = self
            .navigation
            .task_edit_baseline
            .as_ref()
            .filter(|(id, _)| *id == task.id)
            .map(|(_, values)| values)
            .cloned()
            .unwrap_or_else(|| TaskEditValues {
                title: task.title.clone(),
                description: task.description.clone(),
                labels: task.labels.clone(),
                due_date: task.due_date.clone(),
                project: task.project_id,
                stage: task.stage,
                cycle: task.cycle.clone(),
            });
        Some(TaskEditInput {
            id: task.id,
            title: if current_values.title != baseline.title {
                current_values.title
            } else {
                task.title.clone()
            },
            description: if current_values.description != baseline.description {
                current_values.description
            } else {
                task.description.clone()
            },
            labels: if current_values.labels != baseline.labels {
                current_values.labels
            } else {
                task.labels.clone()
            },
            due_date: if current_values.due_date != baseline.due_date {
                current_values.due_date
            } else {
                task.due_date.clone()
            },
            cycle: if current_values.cycle != baseline.cycle {
                current_values.cycle
            } else {
                task.cycle.clone()
            },
            priority: task.priority,
            bucket: task.bucket.clone(),
            project: Some(if current_values.project != baseline.project {
                current_values.project
            } else {
                task.project_id
            }),
            stage: Some(if current_values.stage != baseline.stage {
                current_values.stage
            } else {
                task.stage
            }),
            stage_intent: StageIntent::Editor,
        })
    }

    /// Validate the draft and produce the accepted scoped plan against the
    /// current live Store.
    pub(crate) fn prepare_task_editor_save(
        &self,
        cx: &Context<Self>,
    ) -> Result<PreparedTaskSave, String> {
        let reviewed = self
            .draft_editor_values(cx)
            .ok_or_else(|| "The task editor is no longer open.".to_string())?;
        let input = self
            .task_edit_input(cx)
            .ok_or_else(|| "The task editor is no longer open.".to_string())?;
        if input.title.trim().is_empty() {
            return Err("Add a task title before saving.".into());
        }
        if !input.due_date.trim().is_empty() && !crate::app::valid_date(&input.due_date) {
            return Err("Use a valid date in YYYY-MM-DD format.".into());
        }
        if self
            .store
            .task(input.id)
            .is_some_and(|task| task.project_id != input.project.unwrap_or(task.project_id))
        {
            return Err(crate::research_input::MOVE_PROJECT_MESSAGE.into());
        }
        let plan = prepare_task_edit(&self.store, &input).map_err(scoped_error_text)?;
        self.validate_live_task_edit_eligibility(&plan)?;
        Ok(PreparedTaskSave {
            input,
            plan,
            reviewed,
        })
    }

    /// Commit a prepared task edit once. The candidate is cloned from the current
    /// store at commit time; a failed write leaves the live store, draft, modal
    /// and navigation intent untouched.
    pub(crate) fn commit_prepared_task_edit(
        &mut self,
        prepared: PreparedTaskSave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        self.commit_prepared_task_change(prepared, Some(window), cx)
    }

    fn commit_prepared_task_change(
        &mut self,
        prepared: PreparedTaskSave,
        navigate_editor: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        if let Err(message) = self.validate_live_task_edit_eligibility(&prepared.plan) {
            self.show_save_error(&message, cx);
            return TaskSaveOutcome::Rejected(message);
        }
        let id = prepared.plan.id();
        let mut staged = self.store.clone();
        let applied = match apply_task_edit(&mut staged, &prepared.plan) {
            Ok(applied) => applied,
            Err(error) => {
                let message = scoped_error_text(error);
                self.show_save_error(&message, cx);
                return TaskSaveOutcome::Rejected(message);
            }
        };
        let instance = self.task_editor_instance();
        let reviewed = prepared.reviewed.clone();
        let completion = crate::live_save_app::LiveCompletion::TaskEdit {
            task: id,
            expected: reviewed,
            instance,
            close_editor: navigate_editor.is_some(),
            undo: applied.undo.clone(),
        };
        if let Err(error) = self.live_can_accept_primary() {
            self.storage_error = error.clone();
            self.show_save_error(&error, cx);
            return TaskSaveOutcome::Rejected(error);
        }
        let acceptance = match self.save_staged_store(&staged, completion, cx) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                self.storage_error = error.clone();
                self.pending_task_edit = Some(PendingTaskEdit { prepared });
                self.show_save_error(&error, cx);
                return if self.storage.is_none() {
                    TaskSaveOutcome::PendingDurability(error)
                } else {
                    TaskSaveOutcome::RetryableFailure(error)
                };
            }
        };
        if acceptance.is_durable() {
            // The synchronous path completed the undo/toast continuation.
            self.adopt_saved_store(staged);
        } else {
            self.pending_task_edit = None;
            self.toast = "Saving task…".into();
        }
        if let Some(window) = navigate_editor {
            // Closing the editor is safe only when this exact form is still
            // unchanged; otherwise the acknowledgement leaves the newer draft
            // and its editor open.
            if acceptance.is_durable() {
                let _ = self.close_task_editor_overlay();
                self.draft = None;
                self.navigation.task_edit_baseline = None;
                self.open_task(id, window, cx);
                self.finish_saved_exit(window, cx);
            }
        }
        if acceptance.is_durable() {
            self.error.clear();
        }
        cx.notify();
        if acceptance.is_durable() {
            TaskSaveOutcome::Saved
        } else {
            TaskSaveOutcome::Saving
        }
    }

    /// Direct property/board change. Reuses the shared task-edit preparation
    /// and does not enter the task-editor draft/navigation path.
    pub(crate) fn apply_task_metadata(
        &mut self,
        id: u64,
        field: TaskEditField,
        value: TaskEditValue,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(
            field,
            TaskEditField::Labels
                | TaskEditField::DueDate
                | TaskEditField::Cycle
                | TaskEditField::Priority
                | TaskEditField::Bucket
        ) {
            self.show_save_error("This property requires the task editor.", cx);
            return false;
        }
        let Some(mut input) = self.task_edit_input_from_task(id, StageIntent::Manual) else {
            self.show_save_error("Task not found.", cx);
            return false;
        };
        match field {
            TaskEditField::Labels => {
                let TaskEditValue::Labels(value) = value else { return false };
                input.labels = value;
            }
            TaskEditField::DueDate => {
                let TaskEditValue::Text(value) = value else { return false };
                input.due_date = value;
            }
            TaskEditField::Cycle => {
                let TaskEditValue::OptionalText(value) = value else { return false };
                input.cycle = value;
            }
            TaskEditField::Priority => {
                let TaskEditValue::Priority(value) = value else { return false };
                input.priority = value;
            }
            TaskEditField::Bucket => {
                let TaskEditValue::Text(value) = value else { return false };
                input.bucket = value;
            }
            _ => unreachable!(),
        }
        let plan = match prepare_task_edit(&self.store, &input) {
            Ok(plan) => plan,
            Err(error) => {
                self.show_save_error(&scoped_error_text(error), cx);
                return false;
            }
        };
        if let Err(message) = self.validate_live_task_edit_eligibility(&plan) {
            self.show_save_error(&message, cx);
            return false;
        }
        let reviewed = reviewed_from_input(&input);
        self.commit_prepared_task_change(
            PreparedTaskSave {
                input,
                plan,
                reviewed,
            },
            None,
            cx,
        )
        .is_saved()
    }

    /// Direct board/property stage change and scoped step reopen.
    pub(crate) fn apply_task_stage(
        &mut self,
        id: u64,
        stage: crate::domain::Stage,
        intent: StageIntent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut input) = self.task_edit_input_from_task(id, intent) else {
            self.show_save_error("Task not found.", cx);
            return false;
        };
        input.stage = Some(stage);
        let plan = match prepare_task_edit(&self.store, &input) {
            Ok(plan) => plan,
            Err(error) => {
                self.show_save_error(&scoped_error_text(error), cx);
                return false;
            }
        };
        if let Err(message) = self.validate_live_task_edit_eligibility(&plan) {
            self.show_save_error(&message, cx);
            return false;
        }
        let reviewed = reviewed_from_input(&input);
        self.commit_prepared_task_change(
            PreparedTaskSave {
                input,
                plan,
                reviewed,
            },
            None,
            cx,
        )
        .is_saved()
    }

    fn task_edit_input_from_task(&self, id: u64, intent: StageIntent) -> Option<TaskEditInput> {
        let task = self.store.task(id)?;
        Some(TaskEditInput {
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
            stage_intent: intent,
        })
    }

    pub(crate) fn prepare_comment_save(
        &self,
        id: u64,
        body: &str,
    ) -> Result<PreparedCommentSave, String> {
        let plan = prepare_comment_edit(&self.store, id, body).map_err(scoped_error_text)?;
        Ok(PreparedCommentSave {
            task: id,
            body: body.trim().to_string(),
            plan,
        })
    }

    pub(crate) fn commit_prepared_comment(
        &mut self,
        prepared: PreparedCommentSave,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        let mut staged = self.store.clone();
        let applied = match apply_comment_edit(&mut staged, &prepared.plan) {
            Ok(applied) => applied,
            Err(error) => {
                let message = scoped_error_text(error);
                self.show_save_error(&message, cx);
                return TaskSaveOutcome::Rejected(message);
            }
        };
        let completion = crate::live_save_app::LiveCompletion::Comment {
            task: prepared.task,
            body: prepared.body.clone(),
            undo: applied.undo.clone(),
        };
        if let Err(error) = self.live_can_accept_primary() {
            self.storage_error = error.clone();
            self.show_save_error(&error, cx);
            return TaskSaveOutcome::Rejected(error);
        }
        let acceptance = match self.save_staged_store(&staged, completion, cx) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                self.storage_error = error.clone();
                self.pending_comment_edit = Some(PendingCommentEdit { prepared });
                self.show_save_error(&error, cx);
                return if self.storage.is_none() {
                    TaskSaveOutcome::PendingDurability(error)
                } else {
                    TaskSaveOutcome::RetryableFailure(error)
                };
            }
        };
        if acceptance.is_durable() {
            self.adopt_saved_store(staged);
            self.pending_comment_edit = None;
            if let Some(window) = window {
                // Clear only the exact draft this save owned. A newer draft typed
                // after a failed retry, or another task's cached comment, survives.
                let current = self.comment.read(cx).value().trim().to_string();
                if self.selected() == Some(prepared.task) && current == prepared.body {
                    self.comment
                        .update(cx, |state, cx| state.set_value("", window, cx));
                }
                if self
                    .navigation
                    .comments
                    .get(prepared.task)
                    .is_some_and(|text| text.trim() == prepared.body)
                {
                    self.navigation.comments.take(prepared.task);
                }
                self.refresh_research_inputs(window, cx);
            }
            self.error.clear();
            self.toast = "Comment added".into();
            self.save_feedback = Some(Ok("Comment added".into()));
            cx.notify();
            TaskSaveOutcome::Saved
        } else {
            self.pending_comment_edit = None;
            self.toast = "Saving comment…".into();
            cx.notify();
            TaskSaveOutcome::Saving
        }
    }

    pub(crate) fn retry_comment_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        let Some(pending) = self.pending_comment_edit.clone() else {
            return TaskSaveOutcome::Rejected("There is no pending comment to retry.".into());
        };
        let plan = match prepare_comment_edit(&self.store, pending.prepared.task, &pending.prepared.body)
        {
            Ok(plan) => plan,
            Err(error) => {
                let message = scoped_error_text(error);
                self.pending_comment_edit = None;
                self.show_save_error(&message, cx);
                return TaskSaveOutcome::Rejected(message);
            }
        };
        if let Some(impact) = plan.impact().cloned()
            && Some(&impact) != pending.prepared.plan.impact()
        {
            let prepared = PreparedCommentSave {
                task: pending.prepared.task,
                body: pending.prepared.body,
                plan,
            };
            self.research_save_impact = Some(crate::research_input_app::PendingResearchSave {
                task: prepared.task,
                impact,
                prepared: crate::research_input_app::PendingPreparedSave::Comment(
                    prepared.clone(),
                ),
            });
            self.pending_comment_edit = Some(PendingCommentEdit { prepared });
            self.toast = "The impact changed · review the updated effect before posting".into();
            window.focus(&self.modal_focus, cx);
            cx.notify();
            return TaskSaveOutcome::PendingImpact;
        }
        let outcome = self.commit_prepared_comment(
            PreparedCommentSave {
                task: pending.prepared.task,
                body: pending.prepared.body,
                plan,
            },
            Some(window),
            cx,
        );
        if outcome.is_saved() {
            self.research_save_impact = None;
        }
        outcome
    }

    /// Retry the retained task edit after a storage failure. The original typed
    /// delta is revalidated against the current store; it is never rebased from
    /// the old full form. Unrelated metadata/runtime therefore stays current,
    /// while a newer owned-field change conflicts.
    pub(crate) fn retry_task_editor_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        let Some(pending) = self.pending_task_edit.clone() else {
            return TaskSaveOutcome::Rejected("There is no pending task edit to retry.".into());
        };
        // Retry the original typed delta, not a full old-form rebase. Owned
        // fields keep their original before/after guard; unrelated metadata and
        // runtime are refreshed from current state. A newer owned-field change
        // conflicts in the rebase.
        let plan = match rebase_task_edit(&self.store, &pending.prepared.plan) {
            Ok(plan) => plan,
            Err(error) => {
                let message = scoped_error_text(error);
                self.pending_task_edit = None;
                self.show_save_error(&message, cx);
                return TaskSaveOutcome::Rejected(message);
            }
        };
        if let Some(impact) = plan.impact().cloned()
            && Some(&impact) != pending.prepared.plan.impact()
        {
            let task = pending.prepared.input.id;
            self.research_save_impact = Some(crate::research_input_app::PendingResearchSave {
                task,
                impact,
                prepared: crate::research_input_app::PendingPreparedSave::Task(
                    PreparedTaskSave {
                        input: pending.prepared.input.clone(),
                        plan: plan.clone(),
                        reviewed: pending.prepared.reviewed.clone(),
                    },
                ),
            });
            self.pending_task_edit = Some(PendingTaskEdit {
                prepared: PreparedTaskSave {
                    input: pending.prepared.input,
                    plan,
                    reviewed: pending.prepared.reviewed,
                },
            });
            self.toast = "The impact changed · review the updated effect before saving".into();
            window.focus(&self.modal_focus, cx);
            cx.notify();
            return TaskSaveOutcome::PendingImpact;
        }
        let outcome = self.commit_prepared_task_edit(
            PreparedTaskSave {
                input: pending.prepared.input,
                plan,
                reviewed: pending.prepared.reviewed,
            },
            window,
            cx,
        );
        match &outcome {
            TaskSaveOutcome::Saved | TaskSaveOutcome::NoChanges => {
                self.pending_task_edit = None;
                self.research_save_impact = None;
            }
            TaskSaveOutcome::Rejected(_) => {
                // The draft remains for correction, but the stale intent is no
                // longer retryable on its own.
                self.pending_task_edit = None;
                self.research_save_impact = None;
            }
            TaskSaveOutcome::Saving
            | TaskSaveOutcome::PendingImpact
            | TaskSaveOutcome::PendingDurability(_)
            | TaskSaveOutcome::RetryableFailure(_) => {}
        }
        outcome
    }

    /// Pending TodoBatch eligibility is a live-boundary input the pure plan
    /// cannot see. Check it before preparation and again at commit/retry.
    fn validate_live_task_edit_eligibility(
        &self,
        plan: &TaskEditPlan,
    ) -> Result<(), String> {
        let Some(stage) = plan.stage() else {
            return Ok(());
        };
        if stage.after == crate::domain::Stage::InProgress {
            return Err(
                "Save task details first, then submit execution from Board or the status menu."
                    .into(),
            );
        }
        if plan.stage_intent() == StageIntent::Reopen {
            return Ok(());
        }
        self.store
            .validate_manual_transition(plan.id(), stage.after, self.todo_batch.as_ref())
    }
}

fn reviewed_from_input(input: &TaskEditInput) -> TaskEditValues {
    TaskEditValues {
        title: input.title.clone(),
        description: input.description.clone(),
        labels: input.labels.clone(),
        due_date: input.due_date.clone(),
        project: input.project.unwrap_or_default(),
        stage: input.stage.unwrap_or(crate::domain::Stage::Backlog),
        cycle: input.cycle.clone(),
    }
}

fn scoped_error_text(error: ScopedEditError) -> String {
    match error {
        ScopedEditError::Rejected(message) | ScopedEditError::Conflict(message) => message,
    }
}
