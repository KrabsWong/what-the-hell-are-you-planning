//! T14/T15 live persistence continuations.
//!
//! The UI mutates the live `Store` optimistically, submits one typed group to
//! the bounded ordered worker, and only completes user-visible work (status,
//! form close, command effects) after the exact durable receipt. A newer edit
//! made while the write is in flight is never cleared by that acknowledgement;
//! the ordered journal writes it next.

use crate::app::TaskApp;
use crate::domain::{Stage, Store, Task};
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomationSaveReceipt {
    pub input: crate::domain_command::AutomationDraftInput,
    pub form_epoch: u64,
}

impl LiveCompletion {
    pub(crate) fn exit_token(&self) -> Option<u64> {
        match self {
            Self::Command { exit_token, .. }
            | Self::Request { exit_token, .. }
            | Self::TaskEdit { exit_token, .. } => *exit_token,
            _ => None,
        }
    }

    pub(crate) fn automation_save(&self) -> Option<&AutomationSaveReceipt> {
        match self {
            Self::Command { automation_save, .. } => automation_save.as_ref(),
            _ => None,
        }
    }
}

/// UI-only work that runs after the corresponding write is durable.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LiveCompletion {
    None,
    Status(String),
    ArchiveDeleted,
    ResearchUnderstanding { request: u64, signature: String },
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
        /// Exact dirty-exit operation waiting for this receipt.
        exit_token: Option<u64>,
    },
    Comment {
        task: u64,
        body: String,
        undo: crate::scoped_edit::CommentEditUndo,
    },
    Material {
        undo: crate::scoped_material::MaterialUndo,
        message: String,
        /// Exact reviewed material change this receipt belongs to.
        identity: crate::material_app::MaterialSaveIdentity,
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
        exit_token: Option<u64>,
    },
    /// Typed command post-commit continuation. Effects run only here, after the
    /// exact group is durable.
    Command {
        class: crate::domain_command::CommandClass,
        subject: Option<u64>,
        effects: Vec<crate::domain_command::CommandEffect>,
        /// Exact dirty-exit operation waiting for this receipt.
        exit_token: Option<u64>,
        /// Explicit completion for a standalone automation Save draft.
        automation_save: Option<AutomationSaveReceipt>,
        /// Exact enable/pause/resume command waiting for this receipt.
        automation_command: Option<crate::automation_views::AutomationCommandReceipt>,
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

/// Result of stopping one agent process off the UI thread.
pub(crate) struct StoppedRun {
    pub id: u64,
    pub stopped: bool,
    pub events: Vec<crate::opencode::Event>,
    pub had_error: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum DeferredUiAction {
    ClearComment { task: u64, body: String },
    FinishSavedExit { open: u64 },
    /// Close the material impact modal only if it still belongs to this exact
    /// durable receipt, then restore the opener/root focus.
    CloseMaterialImpact {
        identity: crate::material_app::MaterialSaveIdentity,
    },
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

/// Pure quit/close safety decision. An unavailable incremental backend with
/// dirty edits is never considered safe, even though no worker is attached.
pub(crate) fn quit_state_is_clean(
    live_required: bool,
    live_active: bool,
    dirty: bool,
    primary: bool,
    followups: bool,
    deferred: bool,
    adapter_pending: bool,
) -> bool {
    if !live_active {
        // Read-only/unattached: safe only when no in-memory change or effect
        // remains. `live_required` alone is not a write loss.
        return !dirty && !primary && !followups && !deferred;
    }
    !dirty && !primary && !followups && !deferred && !adapter_pending
}

fn followup_usage(groups: &std::collections::VecDeque<LiveGroup>) -> (usize, usize) {
    let mut count = 0usize;
    let mut bytes = 0usize;
    for group in groups {
        for completion in &group.completions {
            count = count.saturating_add(1);
            bytes = bytes.saturating_add(completion.estimated_bytes());
        }
    }
    (count, bytes)
}

fn followup_has_capacity(
    groups: &std::collections::VecDeque<LiveGroup>,
    estimate_bytes: usize,
) -> bool {
    let (count, bytes) = followup_usage(groups);
    let needs_group = !groups.iter().any(|group| group.generation == 0);
    count < MAX_RETAINED_COMPLETIONS
        && bytes.saturating_add(estimate_bytes) <= MAX_RETAINED_COMPLETION_BYTES
        && (!needs_group || groups.len() < MAX_LIVE_FOLLOWUP_GROUPS)
}

impl LiveCompletion {
    fn is_effect(&self) -> bool {
        matches!(
            self,
            Self::Command { .. }
                | Self::ResearchUnderstanding { .. }
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
            Self::ArchiveDeleted => base,
            Self::Status(message) => base.saturating_add(message.capacity()),
            Self::ResearchUnderstanding { signature, .. } => base.saturating_add(signature.capacity()),
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

    /// True when the active continuation is a task-editor save. Dirty-exit
    /// navigation uses this to show Pending rather than a false failure.
    pub(crate) fn live_task_edit_pending(&self) -> bool {
        matches!(
            self.live_primary.as_ref().map(|pending| &pending.completion),
            Some(LiveCompletion::TaskEdit { .. })
        )
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
            && (self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| {
                    !adapter.is_started() && adapter.startup_error().is_none()
                })
                || self.live_has_pending_receipts()
                || self.live_has_deferred_runtime()
                || self.history_needs_pump()
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
        followup_usage(&self.live_followups)
    }

    /// Reserve capacity for one more auxiliary continuation before any Store
    /// mutation. `estimate_bytes` covers the completion payload the caller is
    /// about to create.
    pub(crate) fn live_reserve_auxiliary(&self, estimate_bytes: usize) -> Result<(), String> {
        if !self.live_persistence_active() {
            return Ok(());
        }
        if !followup_has_capacity(&self.live_followups, estimate_bytes) {
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
                // Startup is still opening the worker. This is not a workspace
                // failure: keep the dirty set and retry from the pump without
                // gating setup/onboarding on a transient condition.
                self.live_pending_reason =
                    Some("Saving will start when persistence is ready.".into());
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
                self.live_pending_reason =
                    Some("Saving will start when persistence is ready.".into());
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
                self.live_pending_reason = None;
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
        if !self.store.has_tracked_changes()
            && self.live_primary.is_none()
            && self.live_followups.is_empty()
            && let Some(path) = self.live_recovery_path.take()
        {
            // The recovered sidecar is now durable in the database.
            let _ = crate::persistence_live::clear_recovery(&path);
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
        // A failed lifecycle save is retained in live_primary/live_followups
        // for the ordered retry path. Do not release automation_command_inflight
        // here: the same receipt may still become durable, and clearing it would
        // allow a duplicate Enable/Pause/Resume dispatch.
        let failed_token = if matches_primary {
            self.live_primary
                .as_ref()
                .and_then(|pending| pending.completion.exit_token())
        } else {
            self.live_followups
                .iter()
                .filter(|group| {
                    generation != 0
                        && group.generation == generation
                        && group.epoch == epoch
                        && group.workspace == workspace
                })
                .flat_map(|group| group.completions.iter())
                .find_map(LiveCompletion::exit_token)
        };
        if let Some(token) = failed_token {
            self.dirty_exit_failed(token, reason.clone(), cx);
        }
        self.live_failure = Some(reason.clone());
        self.storage_error = reason;
        self.live_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
        cx.notify();
    }

    /// Reattach the ordered worker after an attachment failure. The v4
    /// workspace stays read-only (never a legacy write) until this succeeds.
    pub(crate) fn retry_live_attachment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some((path, root, incarnation)) = self.live_attach.clone() else {
            return Err("This workspace has no incremental persistence target.".into());
        };
        let intent_path = path.with_extension("dispatch-intents.json");
        self.dispatch_admission = None;
        self.attach_dispatch_admission(&path, &root, incarnation, intent_path)?;
        self.live_save_enabled = true;
        self.live_workspace = Some(incarnation);
        self.live_failure = None;
        self.live_retry_at = None;
        self.storage_error.clear();
        let _ = self.persist_live_followup();
        self.start_pump(cx);
        Ok(())
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

    pub(crate) fn start_history_reader(&mut self, path: &std::path::Path, root: &std::path::Path, incarnation: u64) {
        match crate::history_reader::HistoryReader::start(path, root, incarnation) {
            Ok(reader) => self.history_reader = Some(reader),
            Err(error) => {
                self.storage_error = format!(
                    "Indexed history and lazy report loading are unavailable; the workspace remains readable from the loaded copy: {error}"
                );
            }
        }
    }

    pub(crate) fn current_history_automation(&self) -> Option<u64> {
        self.automation_detail_id().or(self.automation_ui.selected)
    }

    fn next_history_request(&mut self) -> u64 {
        self.history_request_seq = self.history_request_seq.wrapping_add(1).max(1);
        self.history_request_seq
    }

    pub(crate) fn request_history_page(&mut self, automation: u64, cx: &mut Context<Self>) {
        let (cursor, watermark, reset) = self
            .history_cache
            .get(&automation)
            .map(|cache| (cache.next.clone(), cache.watermark, cache.rows.is_empty()))
            .unwrap_or((None, None, true));
        let request_id = self.next_history_request();
        let result = match self.history_reader.as_ref() {
            Some(reader) => {
                reader.request_page(request_id, automation, cursor, watermark, 20)
            }
            None => return,
        };
        match result {
            Ok(()) => {
                let cache = self.history_cache.entry(automation).or_default();
                cache.state = crate::history_reader::HistoryLoad::Loading { request_id };
                cache.reset = reset;
                cx.notify();
            }
            Err(error) => {
                let cache = self.history_cache.entry(automation).or_default();
                cache.state = crate::history_reader::HistoryLoad::Failed {
                    error,
                    retry_at: std::time::Instant::now()
                        + std::time::Duration::from_secs(2),
                };
            }
        }
    }

    pub(crate) fn load_more_automation_history(
        &mut self,
        automation: u64,
        cx: &mut Context<Self>,
    ) {
        let can = self
            .history_cache
            .get(&automation)
            .is_some_and(|cache| {
                cache.next.is_some()
                    && !matches!(
                        cache.state,
                        crate::history_reader::HistoryLoad::Loading { .. }
                    )
            });
        if can {
            self.request_history_page(automation, cx);
        }
    }

    /// True while indexed history or a lazy body is still expected. Keeps the
    /// bounded pump alive only until the reader answers, then lets it idle.
    pub(crate) fn history_needs_pump(&self) -> bool {
        if self.history_reader.is_none() {
            return false;
        }
        if self.pending_result_navigation.is_some() || self.automation_ui.pending_period.is_some() { return true; }
        if self.todo_batch.as_ref().is_some_and(|batch| batch.state == crate::batch::BatchState::Running)
            && !self.batch_result_bodies_pending().is_empty()
        {
            return true;
        }
        let state_pending = |run_id: u64| match self.run_body_state.get(&run_id) {
            None | Some(crate::history_reader::BodyLoad::NotRequested) => true,
            Some(crate::history_reader::BodyLoad::Loading { .. }) => true,
            Some(crate::history_reader::BodyLoad::Failed { .. }) => true,
            Some(
                crate::history_reader::BodyLoad::Loaded
                | crate::history_reader::BodyLoad::Absent,
            ) => false,
        };
        if let Some(run_id) = self.displayed_run_id() {
            let has_output = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == run_id)
                .is_some_and(|run| !run.output.is_empty());
            if !has_output && state_pending(run_id) {
                return true;
            }
        }
        if self
            .visible_dependency_bodies()
            .iter()
            .any(|run_id| self.store.run_body_is_lazy(*run_id) && state_pending(*run_id))
        {
            return true;
        }
        if let Some(occurrence) = self
            .period_overlay()
            .and_then(|id| self.store.occurrence(id))
        {
            if let Some(run_id) = occurrence
                .final_run_id
                .or_else(|| occurrence.run_ids.last().copied())
                && self.store.run_body_is_lazy(run_id)
                && state_pending(run_id)
            {
                return true;
            }
            let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
                occurrence.research_goal_id.into_iter().collect()
            } else {
                occurrence.research_goal_ids.clone()
            };
            for goal in goals {
                for task in self.store.research_children(goal) {
                    if let Some(run_id) = self.store.research_result_body_pending(task)
                        && state_pending(run_id)
                    {
                        return true;
                    }
                }
            }
        }
        if let Some(automation) = self.current_history_automation() {
            return match self.history_cache.get(&automation) {
                None => true,
                Some(cache) => matches!(
                    &cache.state,
                    crate::history_reader::HistoryLoad::NotRequested
                        | crate::history_reader::HistoryLoad::Loading { .. }
                        | crate::history_reader::HistoryLoad::Failed { .. }
                ),
            };
        }
        false
    }

    /// A failed read belongs only to this still-unloaded report. Old request
    /// errors must not replace content that has since been hydrated.
    pub(crate) fn report_body_error(&self, run: &crate::domain::AgentRun) -> Option<&str> {
        if !run.output.trim().is_empty() || !self.store.run_body_is_lazy(run.id) {
            return None;
        }
        match self.run_body_state.get(&run.id) {
            Some(crate::history_reader::BodyLoad::Failed { error, .. }) => Some(error.as_str()),
            _ => None,
        }
    }

    /// Explicit retry for a failed body read. This is not the persistence
    /// footer retry and only re-requests the selected report body.
    pub(crate) fn retry_run_body(&mut self, run_id: u64) {
        self.run_body_state.remove(&run_id);
        let _ = self.request_run_body(run_id);
    }

    pub(crate) fn retry_occurrence_body(&mut self, occurrence: u64) -> bool {
        let Some(run_id) = self.store.occurrence(occurrence).and_then(|value| {
            value
                .final_run_id
                .or_else(|| value.run_ids.last().copied())
        }) else {
            return false;
        };
        self.retry_run_body(run_id);
        true
    }

    pub(crate) fn request_run_body(&mut self, run_id: u64) -> bool {
        let has_body = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .is_some_and(|run| !run.output.is_empty());
        if has_body {
            return false;
        }
        let request = match self.run_body_state.get(&run_id) {
            None => true,
            Some(crate::history_reader::BodyLoad::NotRequested) => true,
            // A stale reply can be marked Loaded without hydrating when the run
            // revision changed. Retry while the marker still says lazy.
            Some(crate::history_reader::BodyLoad::Loaded) => self.store.run_body_is_lazy(run_id),
            Some(crate::history_reader::BodyLoad::Failed { retry_at, .. }) => {
                retry_at <= &std::time::Instant::now()
            }
            Some(
                crate::history_reader::BodyLoad::Loading { .. }
                | crate::history_reader::BodyLoad::Absent,
            ) => false,
        };
        if !request {
            return false;
        }
        let request_id = self.next_history_request();
        let expected = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .map(crate::history_reader::RunBodyRevision::capture);
        let sent = self
            .history_reader
            .as_ref()
            .is_some_and(|reader| reader.request_body(request_id, run_id).is_ok());
        self.run_body_state.insert(
            run_id,
            if sent {
                crate::history_reader::BodyLoad::Loading {
                    request_id,
                    expected: expected.unwrap_or(crate::history_reader::RunBodyRevision {
                        status: String::new(),
                        ended_at: None,
                        session_id: None,
                        exit_code: None,
                        output_len: 0,
                        sources: 0,
                        logs: 0,
                    }),
                }
            } else {
                crate::history_reader::BodyLoad::Failed {
                    error: "history reader is busy".into(),
                    retry_at: std::time::Instant::now() + std::time::Duration::from_secs(2),
                }
            },
        );
        sent
    }

    /// Saved prerequisite bodies needed by unfinished steps of the visible
    /// research goal. Presentation-only: these bodies are never reused as a
    /// research result until they are hydrated and validated.
    pub(crate) fn visible_dependency_bodies(&self) -> Vec<u64> {
        let Some(goal) = self
            .selected()
            .and_then(|id| self.store.task(id))
            .and_then(|task| task.research.as_ref())
            .map(|research| research.goal_id)
        else {
            return Vec::new();
        };
        let mut bodies = Vec::new();
        for step in self.store.research_children(goal) {
            let Some(task) = self.store.task(step) else { continue };
            if task.stage == Stage::Done { continue; }
            let Some(research) = task.research.as_ref() else { continue };
            for dep in &research.depends_on {
                if let Some(run_id) = self.store.research_result_body_pending(*dep) {
                    bodies.push(run_id);
                }
            }
        }
        bodies.sort_unstable();
        bodies.dedup();
        bodies
    }

    /// Bounded poll of indexed pages and lazy report bodies. Never blocks.
    pub(crate) fn poll_history_io(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(reader) = self.history_reader.as_mut() {
            for response in reader.poll(8) {
                changed = true;
                match response {
                    crate::history_reader::ReaderResponse::Page {
                        request_id,
                        automation,
                        result,
                    } => {
                        let Some(cache) = self.history_cache.get_mut(&automation) else {
                            continue;
                        };
                        if !matches!(
                            cache.state,
                            crate::history_reader::HistoryLoad::Loading {
                                request_id: active
                            } if active == request_id
                        ) {
                            // A stale response for an older cursor/reset must not
                            // overwrite or clear the newer request.
                            continue;
                        }
                        match result {
                            Ok(page) => {
                                if matches!(
                                    page.contract,
                                    crate::sqlite_v4::PageContract::Invalidated { .. }
                                ) {
                                    // Concurrent insert/retry changed activity after
                                    // the watermark: restart from page one rather
                                    // than silently skipping a row.
                                    self.history_cache.insert(
                                        automation,
                                        crate::history_reader::HistoryCache::default(),
                                    );
                                    self.request_history_page(automation, cx);
                                    continue;
                                }
                                if let crate::sqlite_v4::PageContract::Stable { watermark } =
                                    page.contract
                                {
                                    cache.watermark = Some(watermark);
                                }
                                cache.total = page.total;
                                if cache.reset {
                                    cache.rows.clear();
                                }
                                for row in page.rows {
                                    if let Some(existing) =
                                        cache.rows.iter_mut().find(|existing| existing.id == row.id)
                                    {
                                        *existing = row;
                                    } else {
                                        cache.rows.push(row);
                                    }
                                }
                                cache.rows.sort_by(|a, b| {
                                    b.activity_utc
                                        .cmp(&a.activity_utc)
                                        .then(b.activity_rank.cmp(&a.activity_rank))
                                        .then(b.id.cmp(&a.id))
                                });
                                cache.next = page.next;
                                cache.state = if cache.rows.is_empty() {
                                    crate::history_reader::HistoryLoad::LoadedEmpty
                                } else {
                                    crate::history_reader::HistoryLoad::Loaded
                                };
                            }
                            Err(error) => {
                                cache.state = crate::history_reader::HistoryLoad::Failed {
                                    error,
                                    retry_at: std::time::Instant::now()
                                        + std::time::Duration::from_secs(2),
                                };
                            }
                        }
                    }
                    crate::history_reader::ReaderResponse::Body {
                        request_id,
                        run_id,
                        result,
                    } => {
                        if !self.store.runs().iter().any(|run| run.id == run_id) {
                            self.run_body_state.remove(&run_id);
                            continue;
                        }
                        let expected = match self.run_body_state.get(&run_id) {
                            Some(crate::history_reader::BodyLoad::Loading {
                                request_id: active,
                                expected,
                            }) if *active == request_id => expected.clone(),
                            _ => continue,
                        };
                        match result {
                            Ok(Some(body)) if !body.is_empty() => {
                                // Hydrate only the exact unchanged revision. A
                                // newer streaming/regenerated output (including a
                                // deliberately cleared body) wins over the disk
                                // snapshot and remains part of the next delta.
                                let unchanged = self
                                    .store
                                    .runs()
                                    .iter()
                                    .find(|run| run.id == run_id)
                                    .is_some_and(|run| {
                                        crate::history_reader::body_reply_is_current(run, &expected)
                                    });
                                if unchanged {
                                    self.store.hydrate_run_body(run_id, body);
                                }
                                self.run_body_state
                                    .insert(run_id, crate::history_reader::BodyLoad::Loaded);
                            }
                            Ok(_) => {
                                // No durable body exists. Stop presenting the
                                // run as a pending report forever.
                                self.store.clear_lazy_body(run_id);
                                self.run_body_state
                                    .insert(run_id, crate::history_reader::BodyLoad::Absent);
                            }
                            Err(error) => {
                                self.run_body_state.insert(
                                    run_id,
                                    crate::history_reader::BodyLoad::Failed {
                                        error,
                                        retry_at: std::time::Instant::now()
                                            + std::time::Duration::from_secs(2),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }
        // Start only missing/expired-retry requests; loaded/empty/absent states
        // are terminal and never re-request on every poll.
        if let Some(automation) = self.current_history_automation() {
            let request = match self.history_cache.get(&automation) {
                None => true,
                Some(cache) => matches!(
                    cache.state,
                    crate::history_reader::HistoryLoad::NotRequested
                ) || matches!(
                    cache.state,
                    crate::history_reader::HistoryLoad::Failed { retry_at, .. }
                        if retry_at <= std::time::Instant::now()
                ),
            };
            if request {
                self.request_history_page(automation, cx);
            }
        }
        let mut body_targets: Vec<u64> = Vec::new();
        if let Some(id) = self.automation_ui.pending_period {
            body_targets.extend(self.pending_period_bodies(id));
        }
        if let Some(pending) = &self.pending_result_navigation { body_targets.push(pending.run_id); }
        if let Some(run_id) = self.displayed_run_id() {
            body_targets.push(run_id);
        }
        // A visible research goal needs its prerequisites' saved bodies before
        // dependency gates can admit a retry or resume. Hydrate them on demand
        // instead of leaving a completed upstream step looking missing after
        // restart.
        body_targets.extend(self.visible_dependency_bodies());
        if let Some(occurrence) = self
            .period_overlay()
            .and_then(|id| self.store.occurrence(id))
        {
            if let Some(run_id) = occurrence
                .final_run_id
                .or_else(|| occurrence.run_ids.last().copied())
            {
                body_targets.push(run_id);
            }
            // A reopened multi-step period needs every step's lazy result body,
            // bounded by the approved step count, not the whole history.
            let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
                occurrence.research_goal_id.into_iter().collect()
            } else {
                occurrence.research_goal_ids.clone()
            };
            for goal in goals {
                for task in self.store.research_children(goal) {
                    if let Some(run_id) = self.store.research_result_body_pending(task) {
                        body_targets.push(run_id);
                    }
                }
            }
        }
        body_targets.sort_unstable();
        body_targets.dedup();
        // Loaded/loading/absent targets do not consume the per-poll budget, so
        // an occurrence with more than 32 steps still makes progress.
        let mut sent = 0usize;
        for run_id in body_targets {
            if sent >= 32 {
                break;
            }
            if self.request_run_body(run_id) {
                sent += 1;
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn finish_automation_draft_save(
        &mut self,
        subject: Option<u64>,
        target: AutomationSaveReceipt,
        cx: &mut Context<Self>,
    ) {
        let same_form = self.automation_ui.form_epoch == target.form_epoch
            && self
                .automation_draft_input(cx)
                .ok()
                .as_ref()
                == Some(&target.input);
        let planning_receipt = self
            .automation_ui
            .planning_save_inflight
            .as_ref()
            .is_some_and(|pending| {
                pending.form_epoch == target.form_epoch && pending.input == target.input
            });
        if !same_form {
            // Durable, but a newer form owns the surface: never overwrite it,
            // and never let an older planning receipt consume it.
            if planning_receipt {
                self.clear_planning_save_inflight();
            }
            self.clear_pending_automation_planning();
            return;
        }
        if self.navigation.pending_saved_exit.is_some() {
            // A dirty-exit decision owns the close/route. Keep the wizard
            // identity intact so its exact-form poll can execute the exit.
            return;
        }
        if planning_receipt {
            self.clear_planning_save_inflight();
            if self.automation_ui.creating && self.automation_ui.step == 3 {
                // The reviewed Generate plan intent consumes this exact receipt.
                self.resume_automation_planning_after_save(subject, &target, cx);
            } else {
                // Back/Cancel/newer surface cancelled the wait. Keep the wizard
                // identity; do not run the ordinary draft completion.
                self.clear_pending_automation_planning();
            }
            return;
        }
        let Some(id) = subject.or(self.automation_ui.editing) else {
            return;
        };
        self.automation_ui.editing = Some(id);
        self.automation_ui.selected = Some(id);
        self.automation_ui.creating = false;
        let status = self
            .store
            .automation(id)
            .map(|definition| definition.status);
        let message = match status {
            Some(crate::automation::DefinitionStatus::Enabled) => {
                "Saved. The enabled version remains approved."
            }
            Some(crate::automation::DefinitionStatus::Paused) => {
                "Saved. The automation remains paused."
            }
            Some(crate::automation::DefinitionStatus::NeedsAttention) => {
                "No changes. Resolve the listed problem before it can run."
            }
            _ => "Draft saved. Nothing will run until you enable it.",
        };
        self.automation_ui.feedback = Some(Ok(message.into()));
        self.toast = "Automation saved locally".into();
    }

    /// Everything that could still be lost on an immediate normal quit.
    pub(crate) fn live_clean_for_quit(&self) -> bool {
        quit_state_is_clean(
            self.live_required,
            self.live_persistence_active(),
            self.store.has_tracked_changes(),
            self.live_primary.is_some(),
            !self.live_followups.is_empty(),
            self.live_has_deferred_runtime(),
            self.dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.live_pending()),
        )
    }

    /// Start a nonblocking graceful quit. Process stopping and persistence both
    /// run off the UI thread; the window stays open until every accepted write
    /// is durable or a recoverable failure is reported.
    pub(crate) fn begin_graceful_quit(
        &mut self,
        sender: std::sync::mpsc::Sender<bool>,
        cx: &mut Context<Self>,
    ) {
        if self.quit_requested {
            return;
        }
        self.quit_requested = true;
        self.quit_sender = Some(sender);
        self.begin_quit_shutdown(cx);
        // Idle/clean/queued-only sessions have no stop worker; still run the
        // drain check so the quit completes instead of waiting forever.
        self.finish_live_quit_drain(cx);
        self.start_pump(cx);
        self.toast = "Saving before quitting…".into();
        cx.notify();
    }

    fn begin_quit_shutdown(&mut self, cx: &mut Context<Self>) {
        self.cancel_automation_timer();
        self.research_sequences.clear();
        for (id, _) in std::mem::take(&mut self.queued_runs) {
            if let Some(mut run) = self.store.run_mut(id) {
                run.status = "Interrupted".into();
                run.ended_at = Some(crate::opencode::timestamp());
            }
        }
        if self.quit_stopping.is_some() || self.active_runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.active_runs);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("t14-quit-stop".into())
            .spawn(move || {
                let stopped = runs
                    .into_iter()
                    .map(|mut active| {
                        let (stopped, events) =
                            active.handle.stop_and_wait(std::time::Duration::from_secs(5));
                        StoppedRun {
                            id: active.id,
                            stopped,
                            events,
                            had_error: active.had_error,
                        }
                    })
                    .collect();
                let _ = tx.send(stopped);
            })
            .ok();
        self.quit_stopping = Some(rx);
        self.start_pump(cx);
    }

    fn poll_quit_stopping(&mut self) {
        let Some(rx) = self.quit_stopping.as_ref() else {
            return;
        };
        let Ok(results) = rx.try_recv() else {
            return;
        };
        self.quit_stopping = None;
        for mut result in results {
            if let Some(mut run) = self.store.run_mut(result.id) {
                let finished = crate::app::record_events(
                    &mut *run,
                    &mut result.had_error,
                    std::mem::take(&mut result.events),
                );
                if !result.stopped {
                    run.logs.push(
                        "[warning] Process cleanup timed out after requesting termination.".into(),
                    );
                }
                if (!finished || run.status == "Cancelled") && run.status != "Blocked" {
                    run.status = "Interrupted".into();
                    run.ended_at = Some(crate::opencode::timestamp());
                    run.logs.push("[interrupted] Application closed before a final result was received; process termination was requested.".into());
                }
            }
            self.finalize_research_run_without_ui(result.id);
        }
    }

    /// Veto a platform close until the live writes are durable. Called from the
    /// window's should-close handler on the UI thread.
    pub(crate) fn can_close_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.live_clean_for_quit()
            && self.active_runs.is_empty()
            && self.quit_stopping.is_none()
            && !self.live_has_deferred_runtime()
        {
            return true;
        }
        self.window_close_requested = true;
        self.begin_quit_shutdown(cx);
        let _ = self.persist_live_followup();
        if let Some(pending) = self.live_primary.as_ref()
            && pending.generation == 0
        {
            let _ = self.submit_primary_group(Some(cx));
        } else if self.live_followups.iter().any(|group| group.generation == 0) {
            let _ = self.submit_pending_followups(Some(cx));
        }
        self.start_pump(cx);
        self.toast = "Saving before closing…".into();
        cx.notify();
        false
    }

    /// Called from the pump. Completes the graceful quit/close exactly once.
    pub(crate) fn finish_live_quit_drain(&mut self, cx: &mut Context<Self>) {
        self.poll_quit_stopping();
        if !self.quit_requested && !self.window_close_requested {
            return;
        }
        if self.quit_stopping.is_some() {
            return;
        }
        if self.live_required && !self.live_persistence_active() {
            // Fail closed: attempt one nonblocking reattach, otherwise report a
            // recoverable failure and keep the window open.
            if self.retry_live_attachment(cx).is_err() {
                self.quit_requested = false;
                self.window_close_requested = false;
                self.window_close_ready = false;
                if let Some(sender) = self.quit_sender.take() {
                    let _ = sender.send(false);
                }
                self.error = self.storage_error.clone();
                self.toast = "Not closed · persistence is unavailable".into();
                cx.notify();
                return;
            }
        }
        let _ = self.persist_live_followup();
        if let Some(pending) = self.live_primary.as_ref()
            && pending.generation == 0
        {
            let _ = self.submit_primary_group(Some(cx));
        }
        if self.live_followups.iter().any(|group| group.generation == 0) {
            let _ = self.submit_pending_followups(Some(cx));
        }
        if self.live_clean_for_quit() {
            let close_window = self.window_close_requested && !self.quit_requested;
            self.quit_requested = false;
            self.window_close_requested = false;
            if close_window && self.window_close_registered {
                // Render completes the platform close after the durable drain.
                self.window_close_ready = true;
            }
            if let Some(sender) = self.quit_sender.take() {
                let _ = sender.send(true);
            }
            cx.notify();
            return;
        }
        if let Some(reason) = self.live_failure.clone() {
            self.quit_requested = false;
            self.window_close_requested = false;
            self.window_close_ready = false;
            if let Some(sender) = self.quit_sender.take() {
                let _ = sender.send(false);
            }
            self.error = format!(
                "The workspace was not fully saved; the window stays open so you can retry. {reason}"
            );
            self.toast = "Not closed · a save is still failing".into();
            cx.notify();
        }
    }

    /// Platform/menu termination cannot be cancelled by GPUI (the quit future
    /// is polled for 200 ms). This emergency path requests process cancellation
    /// without waiting and writes a durable typed recovery sidecar so the next
    /// start reapplies anything that was not acknowledged.
    pub(crate) fn emergency_shutdown(&mut self) {
        self.cancel_automation_timer();
        self.research_sequences.clear();
        for active in std::mem::take(&mut self.active_runs) {
            active.handle.cancel();
            if let Some(mut run) = self.store.run_mut(active.id) {
                if !matches!(run.status.as_str(), "Succeeded" | "Failed" | "Blocked") {
                    run.status = "Interrupted".into();
                    run.ended_at = Some(crate::opencode::timestamp());
                }
            }
        }
        for (id, _) in std::mem::take(&mut self.queued_runs) {
            if let Some(mut run) = self.store.run_mut(id) {
                run.status = "Interrupted".into();
                run.ended_at = Some(crate::opencode::timestamp());
            }
        }
        if let Some(storage) = self.storage.as_ref()
            && let Some(incarnation) = self.live_workspace
        {
            let accepted = self
                .dispatch_admission
                .as_ref()
                .map(|adapter| adapter.live_snapshot())
                .unwrap_or_default();
            let durable_generation = self
                .dispatch_admission
                .as_ref()
                .map_or(0, |adapter| adapter.durable_generation());
            let snapshot_id = format!(
                "{}-{}-{}",
                incarnation,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.as_nanos())
                    .unwrap_or_default(),
            );
            let tail = self.store.drain_store_delta().unwrap_or(
                crate::domain::StoreDelta {
                    changes: Vec::new(),
                    metadata: Vec::new(),
                    epoch: 0,
                },
            );
            let plan = crate::persistence_live::build_recovery_plan(
                incarnation,
                durable_generation,
                snapshot_id,
                accepted,
                tail,
            );
            if let Err(error) =
                crate::persistence_live::write_recovery(storage.path(), &plan)
            {
                self.storage_error = format!(
                    "Emergency recovery snapshot could not be written; unacknowledged changes may be lost: {error}"
                );
                eprintln!("{}", self.storage_error);
            }
        }
        let _ = self.persist_live_followup();
    }

    /// A workspace switch owns a new incarnation. Drop every old continuation
    /// and in-flight correlation before the new worker is attached; an old
    /// adapter/ACK must never complete a form in the new workspace.
    pub(crate) fn reset_live_state(&mut self) {
        self.pending_result_navigation = None;
        self.automation_ui.pending_period = None;
        self.live_primary = None;
        self.live_followups.clear();
        self.history_reader = None;
        self.history_cache.clear();
        self.run_body_state.clear();
        self.history_request_seq = 0;
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
        self.batch_pending_runs.clear();
        self.dispatch_cancelled.clear();
        self.dispatch_cancel_retire.clear();
        self.dispatch_parked = false;
        #[cfg(feature = "qa")]
        {
            self.qa_hold_claim = false;
        }
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
            LiveCompletion::ResearchUnderstanding { request, signature } => {
                self.continue_confirmed_understanding(request, &signature, cx);
            }
            LiveCompletion::ArchiveDeleted => {
                self.undo.clear();
                self.undo_invalidation = Some("Permanently deleted research cannot be restored with Undo.".into());
                self.toast = "Archived research permanently deleted".into();
                self.error.clear();
            }
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
                exit_token,
            } => {
                self.pending_task_edit = None;
                self.undo_invalidation = None;
                if !undo.is_empty() {
                    self.undo.push_task_edit(undo);
                }
                // Only the exact unchanged form instance closes. InputState
                // edits are not Store dirt, and a close+reopen of the same task
                // creates new entities, so both are compared explicitly.
                // The durable fact is independent of whether this form is still
                // the current one. Mark the exact exit first; the render pump
                // then discards a stale exit using the captured identity.
                if let Some(token) = exit_token {
                    self.dirty_exit_durable(token);
                }
                let same_form = close_editor
                    && instance == self.task_editor_instance()
                    && instance.is_some()
                    && self.task_editor_overlay() == Some(task)
                    && self.draft_editor_values(cx).as_ref() == Some(&expected);
                if same_form {
                    let _ = self.close_task_editor_overlay();
                    self.draft = None;
                    self.navigation.task_edit_baseline = None;
                    self.dirty_exit_form_closed(instance);
                    if self.navigation.after_save_target.is_some() {
                        // The user explicitly chose to leave after this save.
                        // Complete that exit on the next window frame.
                        self.deferred_ui
                            .push(DeferredUiAction::FinishSavedExit { open: task });
                    }
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
            LiveCompletion::Material {
                undo,
                message,
                identity,
            } => {
                if self
                    .pending_material_edit
                    .as_ref()
                    .is_some_and(|pending| pending.prepared.identity() == identity)
                {
                    self.pending_material_edit = None;
                }
                self.undo_invalidation = None;
                self.undo.push_material(undo);
                self.error.clear();
                self.toast = format!("{message} · Undo is available");
                self.save_feedback = Some(Ok(message));
                // Window-dependent dismissal/focus runs on the next render.
                self.deferred_ui
                    .push(DeferredUiAction::CloseMaterialImpact { identity });
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
                exit_token,
            } => {
                if let Some(token) = exit_token {
                    self.dirty_exit_durable(token);
                }
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
                    self.toast = "Request saved · review the research understanding when ready".into();
                } else if self.planning_destination.is_none() {
                    // The original form is gone and nothing newer owns the
                    // surface; a passive acknowledgement is safe.
                    self.error.clear();
                    self.toast = "Request saved · review the research understanding when ready".into();
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
                exit_token,
                automation_save,
                automation_command,
            } => {
                if let Some(token) = exit_token {
                    self.dirty_exit_durable(token);
                }
                if let Some(receipt) = automation_save {
                    self.finish_automation_draft_save(subject, receipt, cx);
                }
                if let Some(receipt) = automation_command {
                    self.finish_automation_command(subject, receipt, cx);
                }
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
                DeferredUiAction::FinishSavedExit { open } => {
                    self.open_task(open, window, cx);
                    self.finish_saved_exit(window, cx);
                }
                DeferredUiAction::CloseMaterialImpact { identity } => {
                    let matches = self.research_save_impact.as_ref().is_some_and(|pending| {
                        matches!(
                            &pending.prepared,
                            crate::research_input_app::PendingPreparedSave::Materials(prepared)
                                if prepared.identity() == identity
                        )
                    });
                    if matches {
                        self.research_save_impact = None;
                        if self
                            .pending_material_edit
                            .as_ref()
                            .is_some_and(|pending| pending.prepared.identity() == identity)
                        {
                            self.pending_material_edit = None;
                        }
                        let opener = self.research_save_opener.take();
                        if let Some(opener) = opener {
                            window.focus(&opener, cx);
                        } else {
                            window.focus(&self.focus, cx);
                        }
                        cx.notify();
                    }
                }
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


#[cfg(test)]
mod tests {
    use super::*;

    fn group(completions: Vec<LiveCompletion>) -> LiveGroup {
        LiveGroup {
            generation: 0,
            epoch: 1,
            workspace: 7,
            completions,
        }
    }

    #[test]
    fn distinct_enqueue_ids_are_retained_verbatim() {
        let a = LiveCompletion::EnqueueNext { occurrence: 11 };
        let b = LiveCompletion::EnqueueNext { occurrence: 22 };
        let group = group(vec![a.clone(), b.clone()]);
        assert_eq!(group.completions, vec![a, b]);
    }

    #[test]
    fn distinct_clear_stop_intents_are_retained_verbatim() {
        let a = LiveCompletion::ClearStopIntent {
            occurrence: 11,
            resume_recovery: true,
        };
        let b = LiveCompletion::ClearStopIntent {
            occurrence: 22,
            resume_recovery: false,
        };
        let group = group(vec![a.clone(), b.clone()]);
        assert_eq!(group.completions, vec![a, b]);
    }

    #[test]
    fn idle_live_workspace_is_clean_for_quit() {
        assert!(quit_state_is_clean(true, true, false, false, false, false, false));
        assert!(quit_state_is_clean(false, false, false, false, false, false, false));
    }

    #[test]
    fn pending_write_or_stop_worker_is_never_clean() {
        assert!(!quit_state_is_clean(true, true, true, false, false, false, false));
        assert!(!quit_state_is_clean(true, true, false, true, false, false, false));
        assert!(!quit_state_is_clean(true, true, false, false, true, false, false));
        assert!(!quit_state_is_clean(true, true, false, false, false, true, false));
        assert!(!quit_state_is_clean(true, true, false, false, false, false, true));
    }

    #[test]
    fn unavailable_backend_with_dirty_edits_is_not_clean() {
        assert!(!quit_state_is_clean(true, false, true, false, false, false, false));
        assert!(quit_state_is_clean(true, false, false, false, false, false, false));
    }

    #[test]
    fn capacity_bounds_total_retained_completions_not_just_groups() {
        let completions: Vec<_> = (0..MAX_RETAINED_COMPLETIONS)
            .map(|id| LiveCompletion::EnqueueNext { occurrence: id as u64 })
            .collect();
        let mut groups = std::collections::VecDeque::new();
        groups.push_back(group(completions));
        assert!(!followup_has_capacity(&groups, 0));
        assert!(followup_has_capacity(&groups, 0) == false);
    }

    #[test]
    fn capacity_bounds_bytes_and_allows_pending_group_append() {
        let text = "x".repeat(MAX_RETAINED_COMPLETION_BYTES);
        let mut groups = std::collections::VecDeque::new();
        groups.push_back(group(vec![LiveCompletion::Status(text)]));
        assert!(!followup_has_capacity(&groups, 1));
        let mut appended = std::collections::VecDeque::new();
        appended.push_back(group(Vec::new()));
        assert!(followup_has_capacity(&appended, MAX_RETAINED_COMPLETION_BYTES));
    }
}
