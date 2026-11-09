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
        instance: Option<TaskEditorInstance>,
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
    MaterialUndo {
        cleanup: Vec<crate::scoped_material::ImportedCopy>,
    },
    Settings {
        undo: crate::scoped_settings::SettingsUndo,
        message: String,
    },
    Domain {
        undo: Option<crate::scoped_domain::DomainUndo>,
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

/// One submitted ordinary/runtime group and every continuation that becomes
/// eligible when that exact epoch is durable. Distinct occurrence/run IDs are
/// never merged across entries: each completion is retained verbatim.
#[derive(Clone, Debug)]
pub(crate) struct LiveGroup {
    pub generation: u64,
    pub epoch: u64,
    pub workspace: u64,
    pub completions: Vec<LiveCompletion>,
}

#[derive(Clone, Debug)]
pub(crate) enum DeferredUiAction {
    ClearComment { task: u64, body: String },
}

/// Groups retained outside the worker's ordered acknowledgement window. Kept
/// below `WorkerOptions::max_retained_groups` so a reservation always leaves
/// room for the primary group and the adapter's one retained group.
pub(crate) const MAX_LIVE_FOLLOWUP_GROUPS: usize = 16;
/// Hard bound on retained post-commit continuations across every group. The
/// UI reserves capacity before mutating; a stalled worker therefore rejects
/// new actions instead of growing an unbounded effect queue.
pub(crate) const MAX_RETAINED_COMPLETIONS: usize = 256;
pub(crate) const MAX_RETAINED_COMPLETION_BYTES: usize = 256 * 1024;

impl LiveCompletion {
    fn is_effect(&self) -> bool {
        matches!(
            self,
            Self::Command { .. }
                | Self::AutomationTicks { .. }
                | Self::EnqueueNext { .. }
                | Self::ClearStopIntent { .. }
                | Self::RetryQueued { .. }
        )
    }

    /// Conservative size of the retained effect payload. Distinct IDs and
    /// notices are counted; nothing is merged or dropped to fit.
    pub(crate) fn estimated_bytes(&self) -> usize {
        let base = std::mem::size_of::<Self>();
        match self {
            Self::None => 0,
            Self::Status(message) => base.saturating_add(message.capacity()),
            Self::AutomationTicks {
                queued,
                superseded,
                notice,
            } => base
                .saturating_add(queued.capacity().saturating_mul(8))
                .saturating_add(superseded.capacity().saturating_mul(8))
                .saturating_add(notice.as_ref().map_or(0, |text| text.capacity())),
            Self::EnqueueNext { .. } => base.saturating_add(8),
            Self::RetryQueued { notice, .. } => base
                .saturating_add(8)
                .saturating_add(notice.as_ref().map_or(0, |text| text.capacity())),
            Self::ClearStopIntent { .. } => base.saturating_add(8),
            // Form/command completions are primary-only; count a conservative
            // slot if one is ever retained as an auxiliary effect.
            Self::Command { effects, .. } => base
                .saturating_add(effects.capacity().saturating_mul(std::mem::size_of::<crate::domain_command::CommandEffect>())),
            Self::TaskEdit { expected, .. } => base
                .saturating_add(expected.title.capacity())
                .saturating_add(expected.description.capacity()),
            Self::Comment { body, .. } => base.saturating_add(body.capacity()),
            Self::Material { message, .. } => base.saturating_add(message.capacity()),
            Self::MaterialUndo { cleanup } => base
                .saturating_add(cleanup.capacity().saturating_mul(std::mem::size_of::<crate::scoped_material::ImportedCopy>())),
            Self::Settings { message, .. } => base.saturating_add(message.capacity()),
            Self::Domain { message, .. } => base.saturating_add(message.capacity()),
            Self::Request { input, .. } => base.saturating_add(input.capacity()),
            Self::UndoApplied => base,
        }
    }
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
            || !self.live_followups.is_empty()
            || self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.live_pending())
    }

    /// Preflight before any live mutation. A second form/command save is
    /// rejected while a primary continuation is in flight, so no validated
    /// change can be merged and then lose its completion.
    pub(crate) fn live_can_accept_primary(&self) -> Result<(), String> {
        if !self.live_persistence_active() {
            return Ok(());
        }
        if self.live_primary.is_some() {
            return Err(
                "A previous save is still waiting for its durable receipt. Its changes stay in the workspace and retry automatically."
                    .into(),
            );
        }
        Ok(())
    }

    /// Keep the bounded poll loop alive until every accepted group and every
    /// dirty follow-up has been acknowledged.
    pub(crate) fn live_needs_pump(&self) -> bool {
        self.live_persistence_active()
            && (self.live_has_pending_receipts()
                || self.live_has_deferred_runtime()
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

    /// Submit a follow-up group from the autosave/pump path.
    pub(crate) fn submit_followups_in_persist(&mut self) -> Result<(), String> {
        self.submit_pending_followups(None)
    }

    /// Submit the current dirty snapshot as the form/command primary group. The
    /// caller must have merged validated staged values into the live store.
    pub(crate) fn submit_live_snapshot(
        &mut self,
        completion: LiveCompletion,
        cx: &mut Context<Self>,
    ) -> Result<SaveAcceptance, String> {
        self.live_can_accept_primary()?;
        if !self.live_persistence_active() {
            return Err("Live persistence is unavailable.".into());
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

    /// Whether one more ordinary/runtime completion can be retained. A pending
    /// (generation 0) group accepts additional completions without consuming a
    /// new slot.
    fn retained_completion_usage(&self) -> (usize, usize) {
        let mut count = 0usize;
        let mut bytes = 0usize;
        for group in &self.live_followups {
            for completion in &group.completions {
                count = count.saturating_add(1);
                bytes = bytes.saturating_add(completion.estimated_bytes());
            }
        }
        (count, bytes)
    }

    /// Reserve capacity for one more auxiliary continuation before any Store
    /// mutation. `estimate_bytes` covers the completion payload the caller is
    /// about to create.
    pub(crate) fn live_reserve_auxiliary(&self, estimate_bytes: usize) -> Result<(), String> {
        if !self.live_persistence_active() {
            return Ok(());
        }
        let (count, bytes) = self.retained_completion_usage();
        let needs_group = !self.live_followups.iter().any(|group| group.generation == 0);
        if count >= MAX_RETAINED_COMPLETIONS
            || bytes.saturating_add(estimate_bytes) > MAX_RETAINED_COMPLETION_BYTES
            || (needs_group && self.live_followups.len() >= MAX_LIVE_FOLLOWUP_GROUPS)
        {
            return Err(
                "Persistence is still confirming earlier changes. This action was not applied; retry when the workspace is saved."
                    .into(),
            );
        }
        Ok(())
    }

    /// Conservative default reservation for small runtime effects.
    pub(crate) fn live_can_accept_auxiliary(&self) -> Result<(), String> {
        self.live_reserve_auxiliary(4096)
    }

    /// Submit an ordinary/runtime group that may coexist with a form/command
    /// primary. Its effects are retained by epoch in a bounded queue and run
    /// only when an acknowledgement covers that epoch.
    pub(crate) fn submit_live_auxiliary(
        &mut self,
        completion: LiveCompletion,
        cx: &mut Context<Self>,
    ) -> Result<SaveAcceptance, String> {
        if !self.live_persistence_active() {
            return Err("Live persistence is unavailable.".into());
        }
        let delta = self.store.drain_store_delta()?;
        if delta.changes.is_empty() && delta.metadata.is_empty() {
            self.complete_live_now(completion, cx);
            return Ok(SaveAcceptance::Durable);
        }
        self.retain_completion(completion);
        self.last_save = std::time::Instant::now();
        self.submit_pending_followups(Some(cx))?;
        self.start_pump(cx);
        Ok(SaveAcceptance::Pending)
    }

    /// Append a completion to the pending (not yet submitted) group, or open a
    /// new reserved group. Callers must run `live_can_accept_auxiliary` before
    /// mutating; this function never drops an accepted effect.
    fn retain_completion(
        &mut self,
        completion: LiveCompletion,
    ) {
        let workspace = self.live_workspace.unwrap_or(0);
        let epoch = self.store.dirty_epoch();
        if let Some(group) = self
            .live_followups
            .iter_mut()
            .find(|group| group.generation == 0)
        {
            group.completions.push(completion);
            group.epoch = group.epoch.max(epoch);
            return;
        }
        // Capacity was reserved by the caller. If an invariant breach still
        // reaches here, preserve the effect rather than silently dropping it;
        // the admission preflight is the actual memory bound.
        debug_assert!(self.live_followups.len() < MAX_LIVE_FOLLOWUP_GROUPS);
        self.live_followups.push_back(LiveGroup {
            generation: 0,
            epoch,
            workspace,
            completions: vec![completion],
        });
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

    fn submit_pending_followups(
        &mut self,
        cx: Option<&mut Context<Self>>,
    ) -> Result<(), String> {
        if !self.live_followups.iter().any(|group| group.generation == 0) {
            return Ok(());
        }
        let delta = self.store.drain_store_delta()?;
        if delta.changes.is_empty() && delta.metadata.is_empty() {
            return Ok(());
        }
        let workspace = self.live_workspace.unwrap_or(0);
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return Err("Live persistence is unavailable.".into());
        };
        let generation = match adapter.submit_live_group(delta.changes, delta.metadata, delta.epoch) {
            Ok(generation) => generation,
            Err(error) if error.contains("not ready") => {
                self.storage_error = "Saving will start when persistence is ready.".into();
                return Ok(());
            }
            Err(error) => {
                self.live_failure = Some(error.clone());
                self.storage_error = error.clone();
                return Err(error);
            }
        };
        for group in self.live_followups.iter_mut() {
            if group.generation == 0 && group.epoch <= delta.epoch {
                // The submitted snapshot covers this group's mutations.
                group.generation = generation;
                group.epoch = delta.epoch;
                group.workspace = workspace;
            }
        }
        self.live_failure = None;
        self.last_save = std::time::Instant::now();
        if let Some(cx) = cx {
            self.start_pump(cx);
        }
        Ok(())
    }

    /// Autosave/pump submission. Coalesces one dirty snapshot into the ordered
    /// journal, records a plain follow-up group, and returns whether work was
    /// submitted. A newer edit during an in-flight primary becomes its own group
    /// rather than being lost or clearing the form early.
    pub(crate) fn persist_live_followup(&mut self) -> bool {
        if !self.live_persistence_active() {
            return false;
        }
        if !self.store.has_tracked_changes() {
            if self.live_followups.iter().any(|group| group.generation == 0) {
                let _ = self.submit_pending_followups(None);
            }
            if self
                .live_primary
                .as_ref()
                .is_some_and(|pending| pending.generation == 0)
            {
                let _ = self.submit_primary_group(None);
            }
            return false;
        }
        let epoch = self.store.dirty_epoch();
        if let Some(pending) = self.live_primary.as_ref() {
            if pending.generation == 0 {
                if let Err(error) = self.submit_primary_group(None) {
                    self.storage_error = error;
                    return false;
                }
                return true;
            }
            if !pending.durable && pending.epoch == epoch {
                // This snapshot is already the primary group.
                return false;
            }
        }
        if self
            .live_followups
            .iter()
            .any(|group| group.generation == 0 || group.epoch == epoch)
        {
            let _ = self.submit_pending_followups(None);
            return false;
        }
        let delta = match self.store.drain_store_delta() {
            Ok(delta) => delta,
            Err(error) => {
                self.storage_error = error;
                return false;
            }
        };
        if delta.changes.is_empty() && delta.metadata.is_empty() {
            return false;
        }
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            self.storage_error = "Live persistence is unavailable.".into();
            return false;
        };
        match adapter.submit_live_group(delta.changes, delta.metadata, delta.epoch) {
            Ok(generation) => {
                self.live_followups.push_back(LiveGroup {
                    generation,
                    epoch: delta.epoch,
                    workspace: self.live_workspace.unwrap_or(0),
                    completions: Vec::new(),
                });
                self.live_failure = None;
                self.last_save = std::time::Instant::now();
                true
            }
            Err(error) => {
                self.storage_error = error;
                false
            }
        }
    }

    /// Retry deferred primary/follow-up submissions and a blocked/retained
    /// worker group. Effects remain gated on the next matching receipt.
    pub(crate) fn retry_live_persistence(&mut self, cx: &mut Context<Self>) {
        if !self.live_persistence_active() {
            return;
        }
        self.live_failure = None;
        self.live_retry_at = None;
        let _ = self.submit_primary_group(Some(cx));
        let _ = self.submit_pending_followups(Some(cx));
        if let Some(adapter) = self.dispatch_admission.as_mut()
            && let Err(error) = adapter.retry_live()
        {
            self.live_failure = Some(error.clone());
            self.storage_error = error;
        }
        if self.store.has_tracked_changes()
            && self.live_primary.as_ref().is_none_or(|pending| pending.durable)
            && !self.live_followups.iter().any(|group| group.generation == 0)
        {
            self.persist_if_changed(true);
        }
        self.start_pump(cx);
    }

    /// Called for every live receipt after the adapter has validated its
    /// workspace, generation and empty effect list. Runs only continuations
    /// whose captured epoch is covered by this acknowledgement.
    pub(crate) fn on_live_durable(
        &mut self,
        generation: u64,
        epoch: u64,
        cx: &mut Context<Self>,
    ) {
        if generation == 0 {
            return;
        }
        let workspace = self.live_workspace.unwrap_or(0);
        if let Some(pending) = self.live_primary.as_mut()
            && pending.generation == generation
            && pending.epoch == epoch
            && pending.workspace == workspace
        {
            pending.durable = true;
            self.store.retire_epoch(epoch);
            self.live_failure = None;
            self.storage_error.clear();
        }
        // Remove and run every covered follow-up in epoch order. A deferred
        // group (generation 0) is also covered by this snapshot's epoch.
        let mut covered = Vec::new();
        loop {
            let Some(index) = self
                .live_followups
                .iter()
                .position(|group| group.epoch <= epoch && group.workspace == workspace)
            else {
                break;
            };
            let group = self.live_followups.remove(index).expect("checked index");
            covered.push(group);
        }
        covered.sort_by_key(|group| group.epoch);
        for group in covered {
            self.store.retire_epoch(group.epoch);
            // Each completion is run exactly once, in submission order. IDs are
            // distinct entries/content, never merged.
            for completion in group.completions {
                self.complete_live_now(completion, cx);
            }
        }
        self.finish_primary_if_ready(cx);
        if self.store.has_tracked_changes()
            && self.live_primary.as_ref().is_none_or(|pending| pending.durable)
        {
            // A newer edit landed while groups were in flight. Write it next;
            // the form still does not close until every current edit is durable.
            self.persist_if_changed(true);
        }
        if self
            .live_primary
            .as_ref()
            .is_some_and(|pending| pending.durable)
            || self.live_followups.iter().any(|group| group.generation == 0)
        {
            let _ = self.submit_pending_followups(Some(cx));
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
        let workspace = self.live_workspace.unwrap_or(0);
        let matches_primary = self.live_primary.as_ref().is_some_and(|pending| {
            generation != 0
                && pending.generation == generation
                && pending.epoch == epoch
                && pending.workspace == workspace
        });
        let matches_followup = self.live_followups.iter().any(|group| {
            generation != 0
                && group.generation == generation
                && group.epoch == epoch
                && group.workspace == workspace
        });
        if !matches_primary && !matches_followup {
            // Never let an unrelated receipt overwrite the current error.
            return;
        }
        self.live_failure = Some(reason.clone());
        self.storage_error = reason;
        self.live_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
        cx.notify();
    }

    /// Replay one-shot runtime callbacks once continuation capacity frees.
    /// Identities only; the underlying run/occurrence state stays in the live
    /// store, so replaying performs the transition exactly once.
    pub(crate) fn drain_live_deferred_runtime(&mut self, cx: &mut Context<Self>) {
        if !self.live_persistence_active() {
            return;
        }
        for _ in 0..16 {
            if self.live_can_accept_auxiliary().is_err() {
                break;
            }
            if let Some(run_id) = self.live_deferred_runs.pop_front() {
                self.complete_research_run(run_id, cx);
                continue;
            }
            if let Some((occurrence, run_id)) = self.live_deferred_after_run.pop_front() {
                self.automation_after_run(occurrence, run_id, cx);
                continue;
            }
            if self.live_deferred_recovery {
                self.live_deferred_recovery = false;
                self.recover_automations(cx);
                continue;
            }
            break;
        }
    }

    pub(crate) fn live_has_deferred_runtime(&self) -> bool {
        !self.live_deferred_runs.is_empty()
            || !self.live_deferred_after_run.is_empty()
            || self.live_deferred_recovery
    }

    pub(crate) fn defer_run_completion(&mut self, run_id: u64) {
        if !self.live_deferred_runs.contains(&run_id) {
            self.live_deferred_runs.push_back(run_id);
        }
    }

    pub(crate) fn defer_after_run(&mut self, occurrence: u64, run_id: u64) {
        if !self
            .live_deferred_after_run
            .contains(&(occurrence, run_id))
        {
            self.live_deferred_after_run.push_back((occurrence, run_id));
        }
    }

    /// A workspace switch owns a new incarnation. Drop every old continuation
    /// and in-flight correlation before the new worker is attached; an old
    /// adapter/ACK must never complete a form in the new workspace.
    pub(crate) fn reset_live_state(&mut self) {
        self.live_primary = None;
        self.live_followups.clear();
        self.live_deferred_runs.clear();
        self.live_deferred_after_run.clear();
        self.live_deferred_recovery = false;
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
        // Forms and status changes close only once every current edit is
        // durable. Post-commit effects in the follow-up queue are tied to their
        // own group and may run even while a later edit is still being written.
        if !pending.completion.is_effect() && self.store.has_tracked_changes() {
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
                    && instance == self.task_editor_instance()
                    && instance.is_some()
                    && self.task_editor_overlay() == Some(task)
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
            LiveCompletion::MaterialUndo { cleanup } => {
                self.undo_invalidation = None;
                crate::appearance::apply(self.store.appearance(), None, cx);
                self.toast = "Last change undone".into();
                self.error.clear();
                if !cleanup.is_empty() {
                    self.submit_material_cleanup(cleanup, cx);
                }
            }
            LiveCompletion::Settings { undo, message } => {
                self.undo_invalidation = None;
                self.undo.push_settings(undo);
                self.toast = message;
                self.error.clear();
            }
            LiveCompletion::Domain { undo, message } => {
                self.undo_invalidation = None;
                if let Some(undo) = undo {
                    self.undo.push_domain(undo);
                }
                self.toast = message;
                self.error.clear();
            }
            LiveCompletion::Request {
                id,
                start,
                destination,
                input,
                replan_source,
                form_epoch,
            } => {
                let unchanged = self.planning_form_epoch == form_epoch
                    && self.planning_destination == destination
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
                    self.error.clear();
                    self.toast = "Request saved · generate a plan when ready".into();
                } else if self.planning_destination.is_none() {
                    // The original form is gone and nothing newer owns the
                    // surface; a passive acknowledgement is safe.
                    self.error.clear();
                    self.toast = "Request saved · generate a plan when ready".into();
                }
                if start {
                    // Starting a saved request never moves the user's route.
                    self.retry_research_request_without_navigation(id, cx);
                }
            }
            LiveCompletion::Command {
                class,
                subject,
                effects,
            } => {
                self.storage_error.clear();
                if class != crate::domain_command::CommandClass::OrdinaryEdit {
                    self.invalidate_undo_for_automation();
                }
                if class == crate::domain_command::CommandClass::StopExternalWork
                    && let Some(subject) = subject
                {
                    self.clear_stop_intent(subject);
                    self.automation_ui.feedback = Some(Ok(
                        "Stopped. The cancellation is saved and this period will not restart."
                            .into(),
                    ));
                }
                self.apply_effects(effects, cx);
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

    /// Stable entity identity of the currently open editor. Reopening the same
    /// task replaces all of these entities.
    pub(crate) fn task_editor_instance(&self) -> Option<TaskEditorInstance> {
        self.draft.as_ref().map(|draft| TaskEditorInstance {
            title: draft.title.entity_id(),
            description: draft.description.entity_id(),
            labels: draft.labels.entity_id(),
            due: draft.due.entity_id(),
        })
    }

    /// Advance the composer instance when a planning form is opened, retargeted
    /// or closed. A close+reopen of the same destination is therefore a distinct
    /// form even if the text and destination happen to be equal.
    pub(crate) fn bump_planning_form_epoch(&mut self) {
        self.planning_form_epoch = self.planning_form_epoch.wrapping_add(1).max(1);
    }
}
