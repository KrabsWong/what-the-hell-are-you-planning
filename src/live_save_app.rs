//! T14/T15 live persistence continuations.
//!
//! The UI mutates the live `Store` optimistically, submits one typed group to
//! the bounded ordered worker, and only completes user-visible work (status,
//! form close, command effects) after the exact durable receipt. A newer edit
//! made while the write is in flight is never cleared by that acknowledgement;
//! the ordered journal writes it next.

use crate::app::TaskApp;
use crate::domain::{Store, Task};
use gpui_kit::{Context, Window};

/// Stable instance identity for an open task editor. Reopening the same task
/// creates new `InputState` entities, so equal field values are not enough to
/// prove this is the same form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskEditorInstance {
    pub title: gpui_kit::EntityId,
    pub description: gpui_kit::EntityId,
    pub labels: gpui_kit::EntityId,
    pub due: gpui_kit::EntityId,
}

/// How a save was accepted. `Pending` means the typed group is queued but not
/// yet durable: callers must keep the form open and must not claim "Saved".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveAcceptance {
    Durable,
    Pending,
}

impl SaveAcceptance {
    pub(crate) fn is_durable(self) -> bool {
        matches!(self, Self::Durable)
    }
}

/// UI-only work that runs after the corresponding write is durable.
#[derive(Clone, Debug)]
pub(crate) enum LiveCompletion {
    None,
    Status(String),
    TaskEdit {
        task: u64,
        /// Form identity captured at submit. The editor closes only if the same
        /// editor instance is still open with exactly these values; a newer
        /// draft, a reopened editor or a different editor is never cleared and
        /// the late ACK never navigates.
        expected: crate::draft_exit::TaskEditValues,
        instance: TaskEditorInstance,
        close_editor: bool,
        undo: crate::scoped_edit::TaskEditUndo,
    },
    Comment {
        task: u64,
        body: String,
        undo: crate::scoped_edit::CommentEditUndo,
    },
    Material {
        undo: crate::scoped_material::MaterialUndo,
        message: String,
    },
    Settings {
        undo: crate::scoped_settings::SettingsUndo,
        message: String,
    },
    Request {
        id: u64,
        start: bool,
        /// Composer identity captured at submit. A late receipt closes/resets
        /// only this unchanged form instance; a reopened form, new draft or new
        /// destination is preserved.
        destination: Option<crate::navigation_context::CreationMode>,
        input: String,
        replan_source: Option<u64>,
        form_epoch: u64,
    },
    /// Typed command post-commit continuation. Effects run only here, after the
    /// exact group is durable.
    Command {
        class: crate::domain_command::CommandClass,
        subject: Option<u64>,
        effects: Vec<crate::domain_command::CommandEffect>,
    },
    /// Deferred post-commit effects for automation/runtime transitions. These
    /// must never run when the write failed.
    AutomationTicks {
        queued: Vec<u64>,
        superseded: Vec<u64>,
        notice: Option<String>,
    },
    EnqueueNext {
        occurrence: u64,
    },
    RetryQueued {
        run_id: u64,
        occurrence: Option<u64>,
        goal: Option<u64>,
        notice: Option<String>,
    },
    ClearStopIntent {
        occurrence: u64,
        resume_recovery: bool,
    },
    UndoApplied,
}

#[derive(Clone, Debug)]
pub(crate) struct LivePending {
    /// Zero while the worker has not accepted the group yet (startup window).
    pub generation: u64,
    pub epoch: u64,
    pub workspace: u64,
    pub durable: bool,
    pub completion: LiveCompletion,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LiveInflight {
    pub generation: u64,
    pub epoch: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum DeferredUiAction {
    ClearComment { task: u64, body: String },
}

impl TaskApp {
    /// The worker is attached and the live path owns ordinary writes.
    pub(crate) fn live_persistence_active(&self) -> bool {
        self.live_save_enabled && self.dispatch_admission.is_some()
    }

    pub(crate) fn live_primary_busy(&self) -> bool {
        self.live_primary.is_some()
    }

    pub(crate) fn live_has_pending_receipts(&self) -> bool {
        self.live_primary.is_some()
            || self.live_inflight.is_some()
            || self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.live_pending())
    }

    /// Keep the bounded poll loop alive until every accepted group and every
    /// dirty follow-up has been acknowledged.
    pub(crate) fn live_needs_pump(&self) -> bool {
        self.live_persistence_active()
            && (self.live_has_pending_receipts()
                || (self.store.has_tracked_changes() && self.live_failure.is_none()))
    }

    pub(crate) fn drain_live_delta(&self) -> Result<crate::domain::StoreDelta, String> {
        self.store.drain_store_delta()
    }

    /// Submit a deferred primary group from the autosave/pump path, which has
    /// no GPUI context. The caller wakes the pump separately.
    pub(crate) fn submit_primary_group_in_persist(&mut self) -> Result<(), String> {
        self.submit_primary_group(None)
    }

    /// Submit the current dirty snapshot. The caller must have already merged
    /// any validated staged values into the live store, so the drained group is
    /// the exact durable write set, including concurrent later edits.
    pub(crate) fn submit_live_snapshot(
        &mut self,
        completion: LiveCompletion,
        cx: &mut Context<Self>,
    ) -> Result<SaveAcceptance, String> {
        if !self.live_persistence_active() {
            return Err("Live persistence is unavailable.".into());
        }
        if self.live_primary.is_some() {
            return Err(
                "A previous save is still waiting for its durable receipt. Its changes stay in the workspace and retry automatically."
                    .into(),
            );
        }
        let delta = self.store.drain_store_delta()?;
        if delta.changes.is_empty() && delta.metadata.is_empty() {
            self.complete_live_now(completion, cx);
            return Ok(SaveAcceptance::Durable);
        }
        let workspace = self.live_workspace.unwrap_or(0);
        self.live_primary = Some(LivePending {
            generation: 0,
            epoch: delta.epoch,
            workspace,
            durable: false,
            completion,
        });
        self.last_save = std::time::Instant::now();
        self.submit_primary_group(Some(cx))?;
        self.start_pump(cx);
        Ok(SaveAcceptance::Pending)
    }

    fn submit_primary_group(&mut self, cx: Option<&mut Context<Self>>) -> Result<(), String> {
        let Some(pending) = self.live_primary.as_ref() else {
            return Ok(());
        };
        if pending.generation != 0 {
            return Ok(());
        }
        let delta = self.store.drain_store_delta()?;
        if delta.changes.is_empty() && delta.metadata.is_empty() {
            self.live_primary = None;
            return Ok(());
        }
        let workspace = self.live_workspace.unwrap_or(0);
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return Err("Live persistence is unavailable.".into());
        };
        match adapter.submit_live_group(delta.changes, delta.metadata, delta.epoch) {
            Ok(generation) => {
                if let Some(pending) = self.live_primary.as_mut() {
                    pending.generation = generation;
                    pending.epoch = delta.epoch;
                    pending.workspace = workspace;
                }
                self.live_failure = None;
                self.last_save = std::time::Instant::now();
                if let Some(cx) = cx {
                    self.start_pump(cx);
                }
                Ok(())
            }
            Err(error) if error.contains("not ready") => {
                // Startup is still opening the worker. The dirty set stays
                // intact and the pump retries without losing the continuation.
                self.storage_error = "Saving will start when persistence is ready.".into();
                Ok(())
            }
            Err(error) => {
                self.live_failure = Some(error.clone());
                self.storage_error = error.clone();
                Err(error)
            }
        }
    }

    /// Retry a deferred primary submission or a blocked/retained worker group.
    /// Effects remain gated on the next matching receipt.
    pub(crate) fn retry_live_persistence(&mut self, cx: &mut Context<Self>) {
        if !self.live_persistence_active() {
            return;
        }
        self.live_failure = None;
        self.live_retry_at = None;
        if self
            .live_primary
            .as_ref()
            .is_some_and(|pending| pending.generation == 0)
        {
            if let Err(error) = self.submit_primary_group(Some(cx)) {
                self.live_failure = Some(error.clone());
                self.storage_error = error;
            }
            return;
        }
        if let Some(adapter) = self.dispatch_admission.as_mut()
            && let Err(error) = adapter.retry_live()
        {
            self.live_failure = Some(error.clone());
            self.storage_error = error;
        }
        if self.live_inflight.is_none() && self.store.has_tracked_changes() {
            self.persist_if_changed(true);
        }
        self.start_pump(cx);
    }

    /// Called for every live receipt after the adapter has validated its
    /// workspace, generation and empty effect list.
    pub(crate) fn on_live_durable(
        &mut self,
        generation: u64,
        epoch: u64,
        cx: &mut Context<Self>,
    ) {
        if generation == 0 {
            return;
        }
        if let Some(inflight) = self.live_inflight
            && inflight.generation == generation
            && inflight.epoch == epoch
        {
            self.live_inflight = None;
            self.store.retire_epoch(inflight.epoch);
        }
        // Exact correlation: captured workspace, generation and epoch must all
        // match. A generation-0 deferred submission is only completed after the
        // worker returns its real generation.
        if let Some(pending) = self.live_primary.as_mut()
            && pending.generation == generation
            && pending.epoch == epoch
        {
            if pending.workspace != self.live_workspace.unwrap_or(0) {
                // A receipt from an older workspace incarnation must never
                // complete or clear the current form.
                return;
            }
            pending.durable = true;
            self.store.retire_epoch(epoch);
            self.live_failure = None;
            self.storage_error.clear();
        }
        self.finish_primary_if_ready(cx);
        if self.store.has_tracked_changes()
            && self.live_primary.as_ref().is_some_and(|pending| pending.durable)
        {
            // A newer edit landed while the primary group was in flight. Write
            // it next; the form/panel still does not close until it is durable.
            self.persist_if_changed(true);
        }
        cx.notify();
    }

    pub(crate) fn on_live_failure(
        &mut self,
        generation: u64,
        epoch: u64,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        let matches_primary = self.live_primary.as_ref().is_some_and(|pending| {
            generation != 0 && pending.generation == generation && pending.epoch == epoch
        });
        let matches_inflight = self
            .live_inflight
            .is_some_and(|inflight| inflight.generation == generation && inflight.epoch == epoch);
        if !matches_primary && !matches_inflight {
            // Never let an unrelated receipt overwrite the current error.
            return;
        }
        self.live_failure = Some(reason.clone());
        self.storage_error = reason;
        self.live_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
        cx.notify();
    }

    /// A workspace switch owns a new incarnation. Drop every old continuation
    /// and in-flight correlation before the new worker is attached; an old
    /// adapter/ACK must never complete a form in the new workspace.
    pub(crate) fn reset_live_state(&mut self) {
        self.live_primary = None;
        self.live_inflight = None;
        self.live_workspace = None;
        self.live_failure = None;
        self.live_retry_at = None;
        self.live_save_inflight_epoch = None;
        self.live_command_generation = None;
        self.live_command_applying = false;
        self.deferred_ui.clear();
        self.dispatch_admission = None;
        self.dispatch_awaiting = None;
        self.dispatch_awaiting_files = None;
        self.dispatch_ack_pending = None;
        self.dispatch_applied.clear();
        self.dispatch_launched.clear();
        self.live_save_enabled = false;
    }

    /// Retry a retained/blocked group when its short backoff has elapsed. This
    /// is called from the bounded pump, never from a render-triggered loop.
    pub(crate) fn poll_live_retry(&mut self) -> bool {
        let Some(at) = self.live_retry_at else {
            return false;
        };
        if std::time::Instant::now() < at {
            return false;
        }
        self.live_retry_at = None;
        if let Some(adapter) = self.dispatch_admission.as_mut() {
            let _ = adapter.retry_live();
        }
        true
    }

    fn finish_primary_if_ready(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.live_primary.as_ref() else {
            return;
        };
        if !pending.durable {
            return;
        }
        let is_effect = matches!(
            pending.completion,
            LiveCompletion::AutomationTicks { .. }
                | LiveCompletion::EnqueueNext { .. }
                | LiveCompletion::ClearStopIntent { .. }
                | LiveCompletion::RetryQueued { .. }
        );
        // Forms and status changes close only once every current edit is
        // durable. Post-commit effects are tied to their own group and may run
        // even while a later edit is still being written.
        if !is_effect && self.store.has_tracked_changes() {
            return;
        }
        let pending = self.live_primary.take().expect("checked above");
        self.complete_live_now(pending.completion, cx);
    }

    pub(crate) fn complete_live_now(
        &mut self,
        completion: LiveCompletion,
        cx: &mut Context<Self>,
    ) {
        match completion {
            LiveCompletion::None => {}
            LiveCompletion::Status(message) => {
                self.toast = message;
                self.error.clear();
            }
            LiveCompletion::TaskEdit {
                task,
                expected,
                instance,
                close_editor,
                undo,
            } => {
                self.pending_task_edit = None;
                self.undo_invalidation = None;
                if !undo.is_empty() {
                    self.undo.push_task_edit(undo);
                }
                // Only the exact unchanged form instance closes. InputState
                // edits are not Store dirt, and a close+reopen of the same task
                // creates new entities, so both are compared explicitly.
                let same_form = close_editor
                    && self.task_editor_overlay() == Some(task)
                    && self.task_editor_instance() == Some(instance)
                    && self.draft_editor_values(cx).as_ref() == Some(&expected);
                if same_form {
                    let _ = self.close_task_editor_overlay();
                    self.draft = None;
                    self.navigation.task_edit_baseline = None;
                } else if self.task_editor_overlay().is_some() {
                    // A different/newer form owns the surface (or the same form
                    // has newer edits): do not claim the current draft is saved
                    // or clear that form's validation feedback.
                    return;
                }
                self.error.clear();
                self.toast = "Task saved to local workspace".into();
                self.save_feedback = Some(Ok("Task saved to local workspace".into()));
            }
            LiveCompletion::Comment { task, body, undo } => {
                self.pending_comment_edit = None;
                self.undo_invalidation = None;
                self.undo.push_comment(undo);
                self.deferred_ui.push(DeferredUiAction::ClearComment { task, body });
                self.error.clear();
                self.toast = "Comment added".into();
                self.save_feedback = Some(Ok("Comment added".into()));
            }
            LiveCompletion::Material { undo, message } => {
                self.pending_material_edit = None;
                self.undo_invalidation = None;
                self.undo.push_material(undo);
                self.error.clear();
                self.toast = format!("{message} · Undo is available");
                self.save_feedback = Some(Ok(message));
            }
            LiveCompletion::Settings { undo, message } => {
                self.undo_invalidation = None;
                self.undo.push_settings(undo);
                self.toast = message;
                self.error.clear();
            }
            LiveCompletion::Request {
                id,
                start,
                destination,
                input,
                replan_source,
            } => {
                let unchanged = self.planning_destination == destination
                    && self.planning_replan_source == replan_source
                    && self.project_brief.read(cx).value().trim() == input.trim();
                if unchanged {
                    self.planning_project = None;
                    self.planning_topic = None;
                    self.planning_replan_source = None;
                    self.planning_destination = None;
                    self.planning_destination_error = None;
                    self.planning_opener_focus = None;
                    self.navigation.planning_baseline.clear();
                }
                self.error.clear();
                self.toast = "Request saved · generate a plan when ready".into();
                if start {
                    // Starting a saved request never moves the user's route.
                    self.retry_research_request_without_navigation(id, cx);
                }
            }
            LiveCompletion::AutomationTicks {
                queued,
                superseded,
                notice,
            } => {
                for id in superseded {
                    self.cancel_unstarted_automation_work(&[id]);
                }
                for id in queued {
                    self.enqueue_automation_occurrence(id, cx);
                }
                if let Some(notice) = notice {
                    self.toast = notice;
                }
            }
            LiveCompletion::EnqueueNext { occurrence } => {
                self.enqueue_automation_occurrence(occurrence, cx);
            }
            LiveCompletion::RetryQueued {
                run_id,
                occurrence,
                goal,
                notice,
            } => {
                self.queued_runs.push((run_id, vec![]));
                if let Some(occurrence) = occurrence
                    && let Some(mut value) = self.store.occurrence_mut(occurrence)
                    && !value.run_ids.contains(&run_id)
                {
                    value.run_ids.push(run_id);
                }
                if let Some(goal) = goal {
                    self.research_sequences.insert(goal);
                }
                if let Some(notice) = notice {
                    self.toast = notice;
                }
            }
            LiveCompletion::ClearStopIntent {
                occurrence,
                resume_recovery,
            } => {
                if let Some(root) = self.workspace_root.clone()
                    && crate::stop_intent::clear(&root, occurrence).is_ok()
                {
                    self.stop_intents
                        .retain(|value| value.occurrence != occurrence);
                }
                if resume_recovery && (self.stop_intents.is_empty() && self.stop_journal_error.is_none())
                {
                    self.recover_automations(cx);
                }
            }
            LiveCompletion::UndoApplied => {
                self.undo_invalidation = None;
                self.toast = "Last change undone".into();
                self.error.clear();
            }
        }
        cx.notify();
    }

    /// Apply window-dependent follow-ups collected by an acknowledgement. The
    /// pump has no `Window`, so these run on the next render instead of dropping
    /// focus/navigation work or blocking the worker thread.
    pub(crate) fn apply_deferred_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.deferred_ui.is_empty() {
            return;
        }
        for action in std::mem::take(&mut self.deferred_ui) {
            match action {
                DeferredUiAction::ClearComment { task, body } => {
                    let current = self.comment.read(cx).value().trim().to_string();
                    if self.selected() == Some(task) && current == body {
                        self.comment
                            .update(cx, |state, cx| state.set_value("", window, cx));
                    }
                    if self
                        .navigation
                        .comments
                        .get(task)
                        .is_some_and(|text| text.trim() == body)
                    {
                        self.navigation.comments.take(task);
                    }
                    self.refresh_research_inputs(window, cx);
                }
            }
        }
    }

    /// Merge a validated staged store into live state without a whole-store
    /// assignment. Returns the typed delta that must be submitted. Used by forms
    /// that still prepare on a clone while their command seam is migrated.
    pub(crate) fn merge_staged_store(&mut self, staged: &Store) -> Result<(), String> {
        let delta = staged.drain_store_delta()?;
        self.store
            .apply_live_journal_group(&delta.changes, &delta.metadata);
        Ok(())
    }

    /// Apply a typed entity upsert through the tracked live path.
    pub(crate) fn upsert_task_live(&mut self, task: Task) {
        if let Some(mut slot) = self.store.task_mut(task.id) {
            *slot = task;
        } else {
            self.store.push_task(task);
        }
    }
}
