//! T13 S1/S2: TaskApp command executor, pending retry, dispatch intent, and
//! emergency stop handling.
//!
//! The domain command module owns preparation and validation. This module owns
//! the single storage commit, pending/reprepare UX, and post-commit effects.
//! Dispatch commits the run and occurrence link once before the queue effect.

use crate::app::{DispatchStatusRetirement, LaunchIntent, PendingDispatch, RunDispatch, TaskApp};
use crate::domain::Store;
use crate::domain_command::{
    Command, CommandClass, CommandEffect, CommandError, CommandReceipt, CommitError, CommitSink,
    PreparedCommand, StopDurability, StopIntent, StopOutcome,
};
use crate::persistence::Storage;
use crate::stop_intent::{self, StopRecord};
use gpui_kit::*;

pub(crate) struct StorageSink<'a> {
    storage: &'a Storage,
    #[cfg(feature = "qa")]
    writes: Option<&'a std::cell::Cell<usize>>,
}

impl CommitSink for StorageSink<'_> {
    fn save(&self, store: &Store) -> Result<(), String> {
        #[cfg(feature = "qa")]
        if let Some(writes) = self.writes {
            writes.set(writes.get() + 1);
        }
        self.storage.save(store)
    }
}

struct NoopSink;

impl CommitSink for NoopSink {
    fn save(&self, _store: &Store) -> Result<(), String> {
        Ok(())
    }
}

/// Result of one user command.
///
/// `Saving` is deliberately distinct from an error: the validated command is
/// visible in memory and queued to the ordered worker, but its effects and
/// success feedback wait for the exact durable receipt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandRun {
    Durable(CommandReceipt),
    Saving { subject: Option<u64> },
}

impl CommandRun {
    pub(crate) fn receipt(&self) -> Option<&CommandReceipt> {
        match self {
            Self::Durable(receipt) => Some(receipt),
            Self::Saving { .. } => None,
        }
    }

    pub(crate) fn subject(&self) -> Option<u64> {
        match self {
            Self::Durable(receipt) => receipt.subject,
            Self::Saving { subject } => *subject,
        }
    }

    pub(crate) fn is_durable(&self) -> bool {
        matches!(self, Self::Durable(_))
    }
}

fn command_error_text(error: CommandError) -> String {
    match error {
        CommandError::Rejected(message) | CommandError::Conflict(message) => message,
    }
}

fn dispatch_error_is_terminal(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("no longer available")
        || message.contains("already ended")
        || message.contains("expired")
        || message.contains("changed")
        || message.contains("does not belong")
        || message.contains("cannot regenerate")
        || message.contains("completed final report")
        || message.contains("not available")
}

impl TaskApp {
    /// Prepare and commit exactly one user command. Success is only reported
    /// after the storage sink accepts the staged store. Failed fail-closed
    /// commands retain their typed command for reprepare against the current store.
    pub(crate) fn execute_command(
        &mut self,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Result<CommandRun, String> {
        if self.pending_commit.is_some()
            || self.pending_stop.is_some()
            || self.pending_dispatch.is_some()
        {
            self.retry_pending_saves(cx);
            if self.pending_commit.is_some()
                || self.pending_stop.is_some()
                || self.pending_dispatch.is_some()
            {
                return Err("Save or discard the previous change before starting another.".into());
            }
        }
        let id = command.id();
        let class = command.class();
        if self.stop_journal_error.is_some() && class != CommandClass::OrdinaryEdit {
            return Err(
                "Automation recovery is paused until the stop-intent file is resolved.".into(),
            );
        }
        // Read-state is a single presentation bit. In live mode, mutate exactly
        // that occurrence on the live store and submit its typed delta instead
        // of staging a whole Store clone. Legacy persistence keeps the normal
        // prepare/commit path.
        if self.live_persistence_active()
            && let Command::SetRead { occurrence, unread } = &command
        {
            return self.commit_direct_set_read(class, *occurrence, *unread, cx);
        }
        let prepared = self
            .store
            .prepare_command(id, class, command)
            .map_err(command_error_text)?;
        self.commit_prepared_command(prepared, cx)
    }

    /// Live-only narrow command path for `Command::SetRead`.
    ///
    /// The caller has already run the pending-command/stop guards. This path
    /// reserves the primary slot before mutating, writes only the occurrence
    /// read bit and submits the same ordered delta. An enqueue failure retains
    /// the requested value for Retry; only pre-acceptance failures restore it.
    /// It never stages or adopts a whole
    /// Store and never pushes an Undo entry.
    fn commit_direct_set_read(
        &mut self,
        class: CommandClass,
        occurrence: u64,
        unread: bool,
        cx: &mut Context<Self>,
    ) -> Result<CommandRun, String> {
        self.live_can_accept_primary()?;
        let previous = self
            .store
            .occurrence(occurrence)
            .map(|value| value.unread)
            .ok_or_else(|| "Automation period not found.".to_string())?;
        let subject = Some(occurrence);
        {
            let mut value = self
                .store
                .occurrence_mut(occurrence)
                .ok_or_else(|| "Automation period not found.".to_string())?;
            value.unread = unread;
        }
        let completion = crate::live_save_app::LiveCompletion::Command {
            class,
            subject,
            effects: Vec::new(),
            exit_token: self.navigation.pending_exit_token.take(),
            automation_save: self.navigation.pending_automation_save.take(),
            automation_command: None,
        };
        match self.submit_live_snapshot(completion, cx) {
            Ok(crate::live_save_app::SaveAcceptance::Pending) => {
                Ok(CommandRun::Saving { subject })
            }
            Ok(crate::live_save_app::SaveAcceptance::Durable) => {
                Ok(CommandRun::Durable(CommandReceipt {
                    subject,
                    effects: Vec::new(),
                    undo: crate::domain_command::UndoDecision::NotUndoable(
                        "Read state is runtime presentation state.",
                    ),
                    feedback: None,
                }))
            }
            Err(error) => {
                // submit_live_snapshot sets live_primary before enqueue. If the
                // primary obligation was retained for Retry, keep the requested
                // tracked value so Retry cannot persist the old read state. Roll
                // back only when no obligation exists (a true pre-acceptance
                // failure).
                if self.live_primary.is_none()
                    && let Some(mut value) = self.store.occurrence_mut(occurrence)
                {
                    value.unread = previous;
                }
                Err(error)
            }
        }
    }

    fn commit_prepared_command(
        &mut self,
        prepared: PreparedCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandRun, String> {
        let class = prepared.class;
        if self.live_persistence_active() {
            // Reserve the single form/command continuation slot before touching
            // live state. A rejected second command must leave values, dirty
            // tracking and the active continuation untouched.
            self.live_can_accept_primary()?;
            if !prepared.base.matches(&self.store) {
                return Err(
                    "The workspace changed after this command was prepared. Review it again."
                        .into(),
                );
            }
            let delta = prepared
                .staged
                .drain_store_delta()
                .map_err(|error| error.to_string())?;
            let subject = prepared.outcome.subject;
            let exit_token = self.navigation.pending_exit_token.take();
            let automation_save = self.navigation.pending_automation_save.take();
            let automation_command = match &prepared.command {
                Command::Enable { .. }
                | Command::AutomationPause { .. }
                | Command::ResumeSchedule { .. } => self.automation_ui.pending_command.take(),
                _ => None,
            };
            let completion = crate::live_save_app::LiveCompletion::Command {
                class,
                subject,
                effects: prepared.outcome.effects.clone(),
                exit_token,
                automation_save,
                automation_command,
            };
            if delta.changes.is_empty() && delta.metadata.is_empty() {
                self.complete_live_now(completion, cx);
                return Ok(CommandRun::Durable(CommandReceipt {
                    subject,
                    effects: prepared.outcome.effects,
                    undo: prepared.outcome.undo,
                    feedback: prepared.outcome.feedback,
                }));
            }
            // Apply exactly the validated typed delta to live state, then submit
            // the drained snapshot. The prepared whole-store clone is dropped.
            self.store
                .apply_live_journal_group(&delta.changes, &delta.metadata);
            match self.submit_live_snapshot(completion, cx)? {
                crate::live_save_app::SaveAcceptance::Pending => {
                    return Ok(CommandRun::Saving { subject });
                }
                crate::live_save_app::SaveAcceptance::Durable => {
                    return Ok(CommandRun::Durable(CommandReceipt {
                        subject,
                        effects: Vec::new(),
                        undo: prepared.outcome.undo,
                        feedback: prepared.outcome.feedback,
                    }));
                }
            }
        }
        let Some(storage) = self.storage.as_ref() else {
            return Err("Workspace storage is unavailable. Nothing was saved.".into());
        };
        let sink = StorageSink {
            storage,
            #[cfg(feature = "qa")]
            writes: Some(&self.qa_storage_writes),
        };
        let result = crate::domain_command::commit_prepared(
            &sink,
            &mut self.store,
            &mut self.last_saved,
            prepared,
        );
        self.finish_commit_result(result, class, cx)
    }

    fn finish_commit_result(
        &mut self,
        result: Result<CommandReceipt, CommitError>,
        class: CommandClass,
        cx: &mut Context<Self>,
    ) -> Result<CommandRun, String> {
        match result {
            Ok(receipt) => {
                self.storage_error.clear();
                // T04 remains authoritative while legacy Vec<Store> undo exists:
                // every durable non-ordinary command invalidates incompatible
                // whole-store snapshots, including pending dispatches/lifecycle.
                if class != CommandClass::OrdinaryEdit {
                    self.invalidate_undo_for_automation();
                }
                if class == CommandClass::StopExternalWork
                    && let Some(subject) = receipt.subject
                {
                    self.clear_stop_intent(subject);
                    self.automation_ui.feedback = Some(Ok(
                        "Stopped. The cancellation is saved and this period will not restart."
                            .into(),
                    ));
                }
                let effects = receipt.effects.clone();
                self.apply_effects(effects, cx);
                Ok(CommandRun::Durable(receipt))
            }
            Err(CommitError::NotDurable { message, pending }) => {
                self.storage_error = message.clone();
                if pending.class == CommandClass::StopExternalWork {
                    self.pending_stop = Some(pending);
                } else {
                    self.pending_commit = Some(pending);
                }
                Err(message)
            }
            Err(CommitError::Stale { message, pending }) => {
                if pending.class == CommandClass::StopExternalWork {
                    self.pending_stop = Some(pending);
                } else {
                    self.pending_commit = Some(pending);
                }
                Err(message)
            }
        }
    }

    /// Live commands retain only typed post-commit data in `live_primary`; no
    /// whole staged store exists to replay here.
    fn finish_live_command(&mut self, _cx: &mut Context<Self>) {}

    /// Footer Retry save / next-command retry. It never replays the old staged
    /// store: it reprepares from the current store under the retained identity.
    pub(crate) fn retry_pending_saves(&mut self, cx: &mut Context<Self>) {
        if self.live_persistence_active() && self.live_has_pending_receipts() {
            self.retry_live_persistence(cx);
            return;
        }
        if let Some(pending) = self.pending_commit.take() {
            match crate::domain_command::reprepare(pending, &self.store) {
                Ok(prepared) => {
                    let stop = prepared.class == CommandClass::StopExternalWork;
                    if let Err(error) = self.commit_prepared_command(prepared, cx) {
                        self.automation_ui.feedback = Some(Err(error));
                        return;
                    }
                    if !stop {
                        self.automation_ui.feedback = Some(Ok("Saved after retry.".into()));
                    }
                }
                Err(error) => {
                    self.automation_ui.feedback = Some(Err(command_error_text(error)));
                    return;
                }
            }
        }
        if let Some(pending) = self.pending_stop.take() {
            match crate::domain_command::reprepare(pending, &self.store) {
                Ok(prepared) => {
                    if let Err(error) = self.commit_prepared_command(prepared, cx) {
                        self.automation_ui.feedback = Some(Err(error));
                        return;
                    }
                }
                Err(error) => {
                    // Keep the stop intent/journal; only the stale typed retry is
                    // discarded, so the period still cannot be auto-restarted.
                    self.automation_ui.feedback = Some(Err(command_error_text(error)));
                    return;
                }
            }
        }
        if let Some(pending) = self.pending_dispatch.take() {
            let backup = pending.clone();
            if let Err(error) = self.retry_dispatch_intent(pending, cx) {
                if self.pending_dispatch.is_none() && !dispatch_error_is_terminal(&error) {
                    self.pending_dispatch = Some(backup);
                    self.automation_ui.feedback = Some(Err(error));
                } else {
                    self.pending_dispatch = None;
                    self.automation_ui.feedback = Some(Err(format!(
                        "Stale pending dispatch discarded: {error}"
                    )));
                }
            }
        }
    }

    /// Explicit resolution for a pending dispatch whose source is no longer
    /// valid. UI callers can expose this as a Discard action; it never queues work.
    pub(crate) fn discard_pending_dispatch(&mut self, cx: &mut Context<Self>) -> bool {
        let discarded = self.pending_dispatch.take().is_some();
        if discarded {
            self.pending_dispatch_reserved = None;
            self.automation_ui.feedback = Some(Err(
                "Pending dispatch discarded. Review the current research request or report and start again."
                    .into(),
            ));
            cx.notify();
        }
        discarded
    }

    /// Cancel a run whose dispatch is still waiting for its durable receipt.
    /// Records a durable cancellation marker, prevents the later claim/spawn and
    /// retires the run as Cancelled through the ordered status write. Returns
    /// true when this call owned the pending admission.
    pub(crate) fn cancel_pending_dispatch(&mut self, run_id: u64, cx: &mut Context<Self>) -> bool {
        let Some(id) = self
            .dispatch_awaiting
            .clone()
            .or_else(|| {
                self.dispatch_claim_pending
                    .as_ref()
                    .map(|(id, _)| id.clone())
            })
            .filter(|id| id.run_id == run_id)
        else {
            return false;
        };
        self.dispatch_cancelled
            .insert(run_id, id.operation_key.clone());
        // A cancelled run must not proceed to a claim even if one was already
        // requested; the durable status path retires any claim that exists.
        if self
            .dispatch_claim_pending
            .as_ref()
            .is_some_and(|(pending, _)| pending.run_id == run_id)
        {
            self.dispatch_claim_pending = None;
        }
        if let Some(adapter) = self.dispatch_admission.as_ref()
            && let Err(error) = adapter.persist_cancel(&id)
        {
            // The cancellation is not durable yet. Stop polling until an
            // explicit retry re-persists the marker; never claim success.
            self.dispatch_admission_error = Some(format!(
                "The cancellation marker could not be queued; this dispatch stays held and is not cancelled until the marker is durable: {error}"
            ));
            self.dispatch_parked = true;
        }
        // If the receipt already applied the run (durable, possibly between the
        // receipt and its launch claim), retire it now and never queue it.
        let snapshot = self.store.run_mut(run_id).map(|run| {
            run.status = "Cancelled".into();
            run.ended_at = Some(crate::opencode::timestamp());
            run.logs.push(
                "[run] Cancelled before launch; no process was started.".into(),
            );
            run.clone()
        });
        if let Some(run) = snapshot
            && let Some(adapter) = self.dispatch_admission.as_mut()
            && let Err(error) = adapter.persist_resolution_status(&id.operation_key, run)
        {
            self.dispatch_admission_error = Some(format!(
                "The cancellation status could not be queued; it will retry: {error}"
            ));
            self.dispatch_parked = true;
        }
        cx.notify();
        true
    }

    /// Start receipt-ordered retirement for one cancelled dispatch: remove the
    /// durable dispatch intent, wait for its acknowledgement, then remove the
    /// cancellation marker. The in-memory obligation survives until both are
    /// confirmed, so a crash cannot resurrect the intent.
    fn begin_cancel_retirement(
        &mut self,
        run_id: u64,
        operation_key: String,
        cx: &mut Context<Self>,
    ) {
        if self
            .dispatch_cancel_retire
            .iter()
            .any(|pending| pending.operation_key == operation_key)
        {
            return;
        }
        self.dispatch_cancel_retire.push(crate::app::PendingCancelRetire {
            operation_key: operation_key.clone(),
            run_id,
            phase: crate::app::CancelRetirePhase::Intent,
            requested: false,
        });
        self.advance_cancel_retirement(&operation_key, cx);
    }

    fn advance_cancel_retirement(&mut self, operation_key: &str, cx: &mut Context<Self>) {
        let Some((phase, requested)) = self
            .dispatch_cancel_retire
            .iter()
            .find(|pending| pending.operation_key == operation_key)
            .map(|pending| (pending.phase, pending.requested))
        else {
            return;
        };
        if requested {
            return;
        }
        if let Some(pending) = self
            .dispatch_cancel_retire
            .iter_mut()
            .find(|pending| pending.operation_key == operation_key)
        {
            pending.requested = true;
        }
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            if let Some(pending) = self
                .dispatch_cancel_retire
                .iter_mut()
                .find(|pending| pending.operation_key == operation_key)
            {
                pending.requested = false;
            }
            self.dispatch_admission_error =
                Some("Durable dispatch admission is unavailable; cancellation retirement is retained.".into());
            self.dispatch_parked = true;
            return;
        };
        let result = match phase {
            crate::app::CancelRetirePhase::Intent => {
                adapter.release_dispatch_intent(operation_key)
            }
            crate::app::CancelRetirePhase::Marker => adapter.release_cancel(operation_key),
        };
        if let Err(error) = result {
            if let Some(pending) = self
                .dispatch_cancel_retire
                .iter_mut()
                .find(|pending| pending.operation_key == operation_key)
            {
                pending.requested = false;
            }
            self.dispatch_admission_error = Some(format!(
                "Cancellation retirement is still pending and will retry: {error}"
            ));
            self.dispatch_parked = true;
            return;
        }
        self.start_pump(cx);
    }

    fn retry_dispatch_intent(
        &mut self,
        pending: crate::app::PendingDispatch,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        use crate::app::LaunchIntent;
        if let Some((reserved_intent, run, files)) = self.pending_dispatch_reserved.clone() {
            if reserved_intent != pending.intent {
                self.pending_dispatch_reserved = None;
                return Err(
                    "The retained dispatch does not belong to this pending intent.".into(),
                );
            }
            let prepared = match self.prepare_run_dispatch(run.clone(), files.clone(), cx) {
                Some(prepared) => prepared,
                None => {
                    self.pending_dispatch_reserved =
                        Some((reserved_intent, run, files));
                    return Err(self.error.clone());
                }
            };
            match self.commit_prepared_dispatch(prepared, reserved_intent.clone(), cx) {
                Some(_) => {
                    self.pending_dispatch_reserved = None;
                    return Ok(());
                }
                None => {
                    self.pending_dispatch_reserved =
                        Some((reserved_intent, run, files));
                    return Err(self.error.clone());
                }
            }
        }
        let now = Self::automation_now();
        let (run, files) = match &pending.intent {
            LaunchIntent::Occurrence(occurrence_id) => {
                let occurrence = self
                    .store
                    .occurrence(*occurrence_id)
                    .cloned()
                    .ok_or_else(|| "The automation period is no longer available.".to_string())?;
                if occurrence.status.is_terminal() {
                    return Err("The automation period already ended.".into());
                }
                if matches!(
                    occurrence.status,
                    crate::automation::OccurrenceStatus::Verifying
                ) {
                    return Err(
                        "The automation period is still checking a previous session.".into(),
                    );
                }
                if occurrence.first_start_expired(now)
                    || occurrence.deadline_at.is_some_and(|deadline| now >= deadline)
                {
                    return Err("The automation period has expired and will not dispatch.".into());
                }
                (
                    self.store
                        .prepare_occurrence_run(&occurrence, now)
                        .map_err(|error| error.to_string())?,
                    pending.files.clone(),
                )
            }
            LaunchIntent::Task(task_id) => {
                let mut run = self
                    .store
                    .prepare_run(*task_id)
                    .map_err(|error| error.to_string())?;
                let files = self.prepare_run_materials(run.task_id, &mut run)?;
                (run, files)
            }
            LaunchIntent::RequestPlan { request_id } => (
                self.store
                    .prepare_request_plan(*request_id)
                    .map_err(|error| error.to_string())?,
                pending.files.clone(),
            ),
            LaunchIntent::ProjectPlan { project_id } => (
                self.store
                    .prepare_project_plan(*project_id)
                    .map_err(|error| error.to_string())?,
                pending.files.clone(),
            ),
            LaunchIntent::ReportRegeneration {
                source_id,
                style,
                occurrence,
            } => {
                let mut run = self.prepare_regeneration_dispatch(*source_id, style, *occurrence)?;
                let files = self.prepare_run_materials(run.task_id, &mut run)?;
                (run, files)
            }
        };
        let intent = pending.intent.clone();
        let prepared = self
            .prepare_run_dispatch(run, files, cx)
            .ok_or_else(|| self.error.clone())?;
        self.commit_prepared_dispatch(prepared, intent, cx)
            .map(|_| ())
            .ok_or_else(|| self.error.clone())
    }

    fn committed_run_files(&self, run_id: u64) -> Result<Vec<std::path::PathBuf>, String> {
        let run = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .ok_or("The committed run is missing.")?;
        if run.attachments.is_empty() {
            return Ok(Vec::new());
        }
        let root = self.data_dir()?;
        run.attachments
            .iter()
            .map(|attachment| crate::attachments::resolve(&root, attachment))
            .collect()
    }

    pub(crate) fn prepare_regeneration_dispatch(
        &self,
        source_id: u64,
        style: &str,
        occurrence_id: Option<u64>,
    ) -> Result<crate::domain::AgentRun, String> {
        let mut run = crate::research::prepare_report_regeneration(&self.store, source_id, style)?;
        if let Some(occurrence_id) = occurrence_id {
            let occurrence = self
                .store
                .occurrence(occurrence_id)
                .ok_or("The automation period for this report is no longer available.")?;
            let index = crate::research::ResearchKind::Advise.index();
            run.automation_occurrence_id = Some(occurrence_id);
            run.deadline_at = Some(
                Self::automation_now().saturating_add(occurrence.snapshot.policy.timeout_seconds),
            );
            run.model = occurrence.snapshot.models[index].clone();
            run.agent = occurrence
                .snapshot
                .agents
                .get(index)
                .and_then(|agent| agent.clone())
                .unwrap_or_else(|| self.store.research_config().default_agent.clone());
            run.prompt =
                crate::automation_execution::with_time_context(run.prompt, occurrence);
            run.logs.push(format!(
                "[automation] Report regeneration for occurrence {} keeps its approved snapshot and time window",
                occurrence.id
            ));
        }
        if self.task_running(run.task_id) || self.recovery_blocks_task(run.task_id) {
            return Err(
                "Wait for this task to finish or resolve its previous session before regenerating."
                    .into(),
            );
        }
        Ok(run)
    }

    /// Start the real durable admission adapter. Not called by production
    /// startup yet; QA and the future activation switch use this same path.
    pub(crate) fn attach_dispatch_admission(
        &mut self,
        database: &std::path::Path,
        root: &std::path::Path,
        workspace: u64,
        intent_path: std::path::PathBuf,
    ) -> Result<(), String> {
        self.dispatch_admission = Some(crate::dispatch_admission::DispatchAdmissionAdapter::open(
            database,
            root,
            workspace,
            crate::persistence_worker::WorkerOptions::default(),
            intent_path,
        )?);
        Ok(())
    }

    /// True while any dispatch admission, claim, acknowledgement or recovery
    /// obligation can still progress without user input. Parked work (a retained
    /// or failed worker group) needs an explicit retry, so it must not keep the
    /// bounded pump awake forever.
    pub(crate) fn dispatch_pump_pending(&self) -> bool {
        if self.dispatch_parked {
            // Parked work cannot progress without an explicit retry. Polling it
            // forever would spin a bounded timer after a worker failure.
            return false;
        }
        self.dispatch_awaiting.is_some()
            || self.dispatch_ack_pending.is_some()
            || self.dispatch_claim_pending.is_some()
            || self.dispatch_resolution_pending.is_some()
            || self.dispatch_resolution_new.is_some()
            || self.dispatch_resolution_launch_pending.is_some()
            || self.dispatch_status_durable.is_some()
            || !self.dispatch_cancel_retire.is_empty()
            || self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.has_status_pending())
            || (!self.dispatch_recovery_done
                && self.dispatch_admission.is_some()
                && self.storage_error.is_empty())
    }

    /// The one run owned by any admitted dispatch command. Planning and
    /// regeneration use the same reserved-intent retry path as an ordinary run.
    fn dispatched_run(command: &Command) -> Option<&crate::domain::AgentRun> {
        match command {
            Command::DispatchRun { run, .. }
            | Command::DispatchRequestPlan { run, .. }
            | Command::DispatchRegeneration { run, .. } => Some(run),
            _ => None,
        }
    }

    /// Real command-handling path for an admitted dispatch. It prepares the same
    /// typed command the synchronous path uses, then waits for durability before
    /// any queue effect or launch.
    pub(crate) fn admit_prepared_dispatch(
        &mut self,
        command: Command,
        intent: LaunchIntent,
        files: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<RunDispatch> {
        if self.dispatch_awaiting.is_some() {
            if let Some(run) = Self::dispatched_run(&command) {
                self.pending_dispatch_reserved =
                    Some((intent.clone(), run.clone(), files.clone()));
            }
            self.pending_dispatch = Some(PendingDispatch { intent, files });
            self.error =
                "A dispatch is already waiting for durable admission. Its intent is retained; retry after it resolves."
                    .into();
            cx.notify();
            return None;
        }
        let reserved_run = Self::dispatched_run(&command).cloned();
        let prepared = match self.store.prepare_command(command.id(), command.class(), command) {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some(run) = reserved_run {
                    self.pending_dispatch_reserved =
                        Some((intent.clone(), run, files.clone()));
                }
                self.pending_dispatch = Some(PendingDispatch { intent, files });
                self.error = command_error_text(error);
                cx.notify();
                return None;
            }
        };
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            self.error = "Durable dispatch admission is unavailable.".into();
            cx.notify();
            return None;
        };
        match adapter.admit(prepared) {
            Ok(id) => {
                if let Some(run) = reserved_run {
                    self.pending_dispatch_reserved =
                        Some((intent.clone(), run, files.clone()));
                }
                self.dispatch_awaiting = Some(id);
                self.dispatch_awaiting_files = Some(files);
                self.dispatch_admission_error = None;
                self.dispatch_parked = false;
                self.toast = "Saving dispatch before launch".into();
                // The first dispatch can begin from a completely idle runtime;
                // keep the bounded pump alive until its durable receipt, claim
                // and acknowledgement complete.
                self.start_pump(cx);
                cx.notify();
                None
            }
            Err(rejection) => {
                if let Some(run) = Self::dispatched_run(&rejection.prepared.command) {
                    self.pending_dispatch_reserved =
                        Some((intent.clone(), run.clone(), files.clone()));
                }
                self.pending_dispatch = Some(PendingDispatch { intent, files });
                self.dispatch_admission_error = Some(rejection.reason.clone());
                self.error = rejection.reason;
                cx.notify();
                None
            }
        }
    }

    /// Bounded polling of durable receipts. It never blocks, joins or assigns an
    /// old staged store. Only exact payloads are merged into live state.
    pub(crate) fn poll_dispatch_admissions(&mut self, cx: &mut Context<Self>) -> bool {
        let updates = match self.dispatch_admission.as_mut() {
            Some(adapter) => {
                let updates = adapter.poll();
                if let Some(error) = adapter.startup_error().map(str::to_owned) {
                    self.live_failure = Some(error.clone());
                    self.storage_error = error;
                }
                updates
            }
            None => return false,
        };
        let mut changed = false;
        for update in updates {
            match update {
                crate::dispatch_admission::AdmissionUpdate::Durable(durable) => {
                    let matches = self.dispatch_awaiting.as_ref().is_none_or(|id| {
                        id.operation_key == durable.id.operation_key
                            && id.run_id == durable.id.run_id
                    });
                    if !matches {
                        self.dispatch_admission_error =
                            Some("A stale dispatch acknowledgement was ignored.".into());
                        changed = true;
                        continue;
                    }
                    let key = durable.id.operation_key.clone();
                    let first_apply = self.dispatch_applied.insert(key.clone());
                    if first_apply {
                        let inserted = match crate::dispatch_admission::apply_durable_parts(
                            &mut self.store,
                            &durable.parts,
                        ) {
                            Ok(inserted) => inserted,
                            Err(error) => {
                                self.dispatch_admission_error = Some(error);
                                changed = true;
                                continue;
                            }
                        };
                        let _ = crate::dispatch_admission::apply_durable_parts(
                            &mut self.last_saved,
                            &durable.parts,
                        );
                        let _ = inserted;
                        if let Some(index) = self
                            .batch_pending_runs
                            .iter()
                            .position(|pending| pending.run_id == durable.id.run_id)
                        {
                            let pending = self.batch_pending_runs.remove(index);
                            // Bind only into the exact batch that requested this
                            // run and only while that goal/step is still pending.
                            // A receipt after cancel/replacement is dropped instead
                            // of joining an unrelated batch.
                            let binds = self.todo_batch.as_ref().is_some_and(|batch| {
                                self.batch_generation_seq == pending.batch_generation
                                    && batch.contains_pending(pending.goal)
                                    && self
                                        .store
                                        .runs()
                                        .iter()
                                        .any(|run| run.id == pending.run_id && run.task_id == pending.step)
                            });
                            if binds {
                                if let Some(batch) = self.todo_batch.as_mut() {
                                    batch.run_ids.insert(pending.run_id, pending.goal);
                                }
                            }
                        }
                        self.dispatch_awaiting = Some(durable.id.clone());
                        self.dispatch_admission_error = None;
                        self.error.clear();
                        // A cancellation recorded before this receipt must retire
                        // the run now: apply it, mark it Cancelled and acknowledge,
                        // but never claim or spawn.
                        let cancelled = self
                            .dispatch_cancelled
                            .get(&durable.id.run_id)
                            .is_some_and(|cancelled_key| {
                                cancelled_key == &durable.id.operation_key
                            });
                        if cancelled {
                            let snapshot = if let Some(run) =
                                self.store.run_mut(durable.id.run_id)
                            {
                                run.status = "Cancelled".into();
                                run.ended_at = Some(crate::opencode::timestamp());
                                run.logs.push(
                                    "[run] Cancelled before launch; no process was started."
                                        .into(),
                                );
                                Some(run.clone())
                            } else {
                                None
                            };
                            if let Some(run) = snapshot
                                && let Some(adapter) = self.dispatch_admission.as_mut()
                                && let Err(error) = adapter.persist_resolution_status(
                                    &durable.id.operation_key,
                                    run,
                                )
                            {
                                self.dispatch_admission_error = Some(format!(
                                    "The cancellation status could not be queued; it will retry: {error}"
                                ));
                                self.dispatch_parked = true;
                            }
                            changed = true;
                            continue;
                        }
                    }
                    #[cfg(feature = "qa")]
                    if std::mem::take(&mut self.qa_hold_claim) {
                        // Deterministic QA window: receipt applied, claim not yet
                        // requested. Keeping `dispatch_claim_pending` set holds the
                        // real Stop route before any launch without parking.
                        self.dispatch_claim_pending = Some((
                            durable.id.clone(),
                            std::sync::Arc::new(durable.parts.clone()),
                        ));
                        changed = true;
                        continue;
                    }
                    // Persist the launch claim before any spawn. The claim is the
                    // durable at-most-once marker for the external process.
                    let parts = std::sync::Arc::new(durable.parts.clone());
                    self.dispatch_claim_pending = Some((durable.id.clone(), parts.clone()));
                    if let Some(adapter) = self.dispatch_admission.as_mut() {
                        if let Err(error) = adapter.claim_launch(&durable.id) {
                            // Keep the exact id/parts so an explicit retry
                            // re-issues the claim; park to stop the pump.
                            self.dispatch_admission_error = Some(format!(
                                "Launch claim could not be queued; no process was started: {error}. Retry dispatch admission to resume this exact run."
                            ));
                            self.dispatch_parked = true;
                        }
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LaunchClaimPersisted { operation_key } => {
                    if let Some((old_key, new_id)) = self.dispatch_resolution_new.clone()
                        && operation_key.ends_with(&format!(":run:{new_id}"))
                    {
                        if let Some(adapter) = self.dispatch_admission.as_mut() {
                            let _ = adapter.release_resolution(&old_key);
                        }
                        #[cfg(feature = "qa")]
                        Self::qa_crash_at("admission");
                        self.dispatch_resolution_launch_pending = Some(operation_key);
                        changed = true;
                        continue;
                    }
                    self.launch_claimed(&operation_key, cx);
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LaunchClaimReleased { operation_key } => {
                    #[cfg(feature = "qa")]
                    Self::qa_crash_at("claim");
                    if let Some(mut retirement) = self.dispatch_status_durable.clone()
                        && retirement.key == operation_key
                    {
                        retirement.claim_done = true;
                        retirement.claim_enqueued = false;
                        self.dispatch_status_durable = Some(retirement);
                    } else if let Some((key, run_id, rerun, replacement)) =
                        self.dispatch_resolution_pending.clone()
                        && key == operation_key
                    {
                        let _ = self.finish_resolution(&key, run_id, rerun, replacement, cx);
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionPersisted {
                    operation_key,
                    rerun,
                    replacement,
                }
                | crate::dispatch_admission::AdmissionUpdate::ResolutionRestored {
                    operation_key,
                    rerun,
                    replacement,
                } => {
                    self.execute_resolution(&operation_key, rerun, replacement, cx);
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionReleased { operation_key } => {
                    if self
                        .dispatch_resolution_pending
                        .as_ref()
                        .is_some_and(|(key, _, _, _)| key == &operation_key)
                    {
                        self.dispatch_resolution_pending = None;
                        self.dispatch_resolution_new = None;
                    }
                    if self
                        .dispatch_status_durable
                        .as_ref()
                        .is_some_and(|retirement| retirement.key == operation_key)
                    {
                        self.dispatch_status_durable = None;
                    }
                    #[cfg(feature = "qa")]
                    Self::qa_crash_at("decision");
                    if let Some(launch_key) = self.dispatch_resolution_launch_pending.take() {
                        self.launch_claimed(&launch_key, cx);
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionFailed { operation_key, reason } => {
                    self.dispatch_admission_error = Some(format!(
                        "The resolution decision was not durable; nothing was changed: {reason}. Retry dispatch admission to resume this exact intent."
                    ));
                    if self
                        .dispatch_resolution_new
                        .as_ref()
                        .is_some_and(|(key, _)| key == &operation_key)
                    {
                        self.dispatch_resolution_new = None;
                    }
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LaunchClaimFailed { reason, operation_key } => {
                    let matched = self
                        .dispatch_claim_pending
                        .as_ref()
                        .is_some_and(|(id, _)| operation_key.ends_with(&id.operation_key));
                    self.dispatch_admission_error = Some(format!(
                        "Launch claim was not durable; no process was started: {reason}. Retry dispatch admission to resume this exact run."
                    ));
                    if matched {
                        // Keep the exact id/parts so retry re-issues the claim.
                        self.dispatch_parked = true;
                    } else {
                        self.dispatch_claim_pending = None;
                        self.dispatch_parked = true;
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionStatusFailed { reason, .. } => {
                    self.dispatch_admission_error = Some(format!(
                        "{reason}. Retry dispatch admission to resume the status write."
                    ));
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionStatusDurable {
                    operation_key,
                    run,
                    ..
                } => {
                    if let Some(mut live) = self.store.run_mut(run.id) {
                        *live = run.clone();
                    }
                    if let Some(mut saved) = self.last_saved.run_mut(run.id) {
                        *saved = run;
                    }
                    let key = operation_key;
                    #[cfg(feature = "qa")]
                    Self::qa_crash_at("status");
                    let claimed = self
                        .dispatch_admission
                        .as_ref()
                        .is_some_and(|adapter| adapter.launch_claim_exists(&key));
                    let pending = self.dispatch_resolution_pending.clone();
                    let (run_id, rerun, replacement) = pending
                        .map(|(_, run_id, rerun, replacement)| (run_id, rerun, replacement))
                        .unwrap_or_else(|| {
                            (
                                key.rsplit(":run:")
                                    .next()
                                    .and_then(|value| value.parse::<u64>().ok())
                                    .unwrap_or(0),
                                false,
                                None,
                            )
                        });
                    self.dispatch_status_durable = Some(DispatchStatusRetirement {
                        key,
                        run_id,
                        rerun,
                        replacement,
                        had_claim: claimed,
                        claim_done: !claimed,
                        claim_enqueued: false,
                        decision_enqueued: false,
                        decision_started: false,
                    });
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::Acknowledged { operation_key } => {
                    if self
                        .dispatch_awaiting
                        .as_ref()
                        .is_some_and(|id| id.operation_key == operation_key)
                    {
                        let run_id = self.dispatch_awaiting.as_ref().map(|id| id.run_id);
                        self.dispatch_awaiting = None;
                        self.dispatch_awaiting_files = None;
                        self.dispatch_ack_pending = None;
                        self.dispatch_admission_error = None;
                        if run_id.is_some_and(|run_id| {
                            self.pending_dispatch_reserved
                                .as_ref()
                                .is_some_and(|(_, run, _)| run.id == run_id)
                        }) {
                            self.pending_dispatch_reserved = None;
                        }
                    }
                    // The dispatch intent removal is confirmed; only now may
                    // the cancellation marker be removed.
                    let advance = self
                        .dispatch_cancel_retire
                        .iter_mut()
                        .find(|pending| {
                            pending.operation_key == operation_key
                                && pending.phase == crate::app::CancelRetirePhase::Intent
                        })
                        .map(|pending| {
                            pending.phase = crate::app::CancelRetirePhase::Marker;
                            pending.requested = false;
                            true
                        })
                        .unwrap_or(false);
                    if advance {
                        self.advance_cancel_retirement(&operation_key, cx);
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::AckFailed { operation_key, reason } => {
                    if self.dispatch_cancel_retire.iter().any(|pending| {
                        pending.operation_key == operation_key
                            && pending.phase == crate::app::CancelRetirePhase::Intent
                    }) {
                        // The dispatch intent removal failed; keep the exact
                        // retirement obligation and the cancellation marker.
                        self.dispatch_admission_error = Some(format!(
                            "The cancellation intent retirement failed and will retry: {reason}"
                        ));
                        self.dispatch_parked = true;
                        changed = true;
                        continue;
                    }
                    if let Some(mut retirement) = self.dispatch_status_durable.clone() {
                        if operation_key == format!("launch:{}", retirement.key) {
                            retirement.claim_enqueued = false;
                            self.dispatch_status_durable = Some(retirement);
                        } else if operation_key
                            .strip_prefix("resolve:")
                            .is_some_and(|rest| rest.starts_with(&retirement.key))
                        {
                            retirement.decision_enqueued = false;
                            // A failed marker-removal request for an already
                            // created replacement is retried via the same
                            // decision path.
                            if retirement.rerun {
                                retirement.decision_started = false;
                            }
                            self.dispatch_status_durable = Some(retirement);
                        }
                    }
                    self.dispatch_admission_error = Some(reason);
                    self.dispatch_ack_pending = None;
                    // The acknowledgement request failed. Park rather than
                    // re-sending every frame; explicit retry resumes it.
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LiveSaveDurable { generation, epoch } => {
                    self.on_live_durable(generation, epoch, cx);
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LiveSaveError {
                    generation,
                    epoch,
                    reason,
                } => {
                    self.on_live_failure(generation, epoch, reason, cx);
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::StaleAck { reason, .. } => {
                    // The worker retained the mismatched group. Only an explicit
                    // retry can resubmit it; keep the reserved intent.
                    self.dispatch_admission_error = Some(reason);
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::Blocked { id, reason } => {
                    // A blocked group is retryable through the worker. Parking it
                    // stops the pump without discarding the exact run identity.
                    self.dispatch_admission_error = Some(format!(
                        "Dispatch {0} is blocked: {reason}. Retry dispatch admission to resume it.",
                        id.run_id
                    ));
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::CancelRestored {
                    operation_key,
                    run_id,
                } => {
                    // A durable cancellation marker survived a restart. Retire the
                    // run if it was already committed; otherwise release the intent.
                    self.dispatch_cancelled
                        .insert(run_id, operation_key.clone());
                    let snapshot = self.store.run_mut(run_id).map(|run| {
                        if run.status != "Cancelled" {
                            run.status = "Cancelled".into();
                            run.ended_at = Some(crate::opencode::timestamp());
                            run.logs.push(
                                "[run] Cancelled before launch; no process was started."
                                    .into(),
                            );
                        }
                        run.clone()
                    });
                    match snapshot {
                        Some(run) => {
                            if let Some(adapter) = self.dispatch_admission.as_mut()
                                && let Err(error) = adapter
                                    .persist_resolution_status(&operation_key, run)
                            {
                                self.dispatch_admission_error = Some(format!(
                                    "The restored cancellation status could not be queued; it will retry: {error}"
                                ));
                                self.dispatch_parked = true;
                            }
                        }
                        None => {
                            // No committed run: retire the dispatch intent first,
                            // then the cancellation marker, in receipt order.
                            self.begin_cancel_retirement(
                                run_id,
                                operation_key.clone(),
                                cx,
                            );
                        }
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::CancelReleased { operation_key } => {
                    self.dispatch_cancelled
                        .retain(|_, key| key.as_str() != operation_key.as_str());
                    self.dispatch_cancel_retire.retain(|pending| {
                        pending.operation_key.as_str() != operation_key.as_str()
                    });
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::CancelFailed {
                    operation_key: _,
                    reason,
                } => {
                    // The cancellation marker was not durable. Keep the held
                    // dispatch and the exact intent; only an explicit retry may
                    // re-persist the marker and retire the run.
                    self.dispatch_admission_error = Some(format!(
                        "The cancellation marker was not durable; this dispatch is still held and not cancelled: {reason}"
                    ));
                    self.dispatch_parked = true;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::Retained { reason, .. } => {
                    // A retained group is parked, not active backpressure. The
                    // worker failure path also lands here, so polling it forever
                    // would spin. Preserve the exact intent for an explicit retry.
                    self.dispatch_admission_error = Some(format!(
                        "{reason}. Retry dispatch admission to resume this exact intent."
                    ));
                    self.dispatch_parked = true;
                    changed = true;
                }
            }
        }
        // Acknowledgement is retried only after an explicit failure; a pending
        // request is not re-sent every frame, and parked work waits for retry.
        if !self.dispatch_parked
            && self.dispatch_ack_pending.is_none()
            && let Some(id) = self.dispatch_awaiting.clone()
            && self.dispatch_applied.contains(&id.operation_key)
        {
            self.begin_acknowledgement(id);
        }
        // A failed/backpressured dispatch-intent retirement is retried on the
        // next bounded poll. The launch claim and resolution marker are retired
        // only from the matching ResolutionStatusDurable handler: releasing them
        // here races the status write and can leave a crashed restart with no
        // claim/key identity for the pending resolution.
        if let Some((key, _, _, _)) = self.dispatch_resolution_pending.clone()
            && let Some(adapter) = self.dispatch_admission.as_mut()
        {
            let _ = adapter.release_dispatch_intent(&key);
        }
        if self.retry_status_completion(cx) {
            changed = true;
        }
        changed |= self.cleanup_launch_claims();
        if !self.dispatch_recovery_done && self.dispatch_admission.is_some() {
            let ready = self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.is_ready());
            if ready {
                self.dispatch_recovery_done = true;
                changed |= self.recover_queued_dispatches(cx);
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }

    fn begin_acknowledgement(&mut self, id: crate::dispatch_admission::AdmissionId) {
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return;
        };
        match adapter.acknowledge(&id) {
            Ok(()) => self.dispatch_ack_pending = Some(id),
            Err(error) => {
                self.dispatch_admission_error = Some(error);
                self.dispatch_ack_pending = None;
            }
        }
    }

    /// Recover a durable-but-unlaunched run after a crash between the commit and
    /// the scheduler handoff. Only transient `Queued` runs are considered, and a
    /// run already in the queue/active set is never duplicated.
    pub(crate) fn recover_queued_dispatches(&mut self, cx: &mut Context<Self>) -> bool {
        let candidates: Vec<(u64, Option<u64>)> = self
            .store
            .runs()
            .iter()
            .filter(|run| {
                run.status == "Queued"
                    && !self.dispatch_cancelled.contains_key(&run.id)
                    && !self.queued_runs.iter().any(|(id, _)| *id == run.id)
                    && !self.active_runs.iter().any(|active| active.id == run.id)
            })
            .map(|run| (run.id, run.automation_occurrence_id))
            .collect();
        let mut changed = false;
        for (run_id, occurrence) in candidates {
            // The claim key differs for ordinary runs, planning requests and
            // report regeneration. Ask the adapter for the exact claim instead
            // of guessing one operation-key shape.
            let operation_key = self
                .dispatch_admission
                .as_ref()
                .and_then(|adapter| adapter.launch_claim_for_run(run_id))
                .unwrap_or_else(|| {
                    format!("dispatch:{occurrence:?}:run:{run_id}")
                });
            if self.dispatch_launched.contains(&operation_key) {
                continue;
            }
            let claimed = self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.launch_claim_exists(&operation_key));
            if claimed {
                if let Some(mut run) = self.store.run_mut(run_id) {
                    run.status = "Checking".into();
                    run.logs.push(
                        "[dispatch] A durable launch claim exists but the process start could not be confirmed. Review the working folder, then explicitly rerun or mark this attempt failed."
                            .into(),
                    );
                }
                self.dispatch_admission_error = Some(format!(
                    "Run {run_id} needs an explicit launch-outcome decision; it was not relaunched automatically."
                ));
                changed = true;
                continue;
            }
            self.apply_effects(vec![CommandEffect::QueueRun { run_id }], cx);
            changed = true;
        }
        changed
    }

    /// Explicit user resolution for a claimed-but-unconfirmed launch. Claims are
    /// never cleared by automatic rerun.
    pub(crate) fn resolve_launch_ambiguity(
        &mut self,
        run_id: u64,
        rerun: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.dispatch_resolution_pending.is_some() {
            return Err("A launch-outcome resolution is already pending; wait for it to finish.".into());
        }
        let operation_key = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .map(|run| {
                format!(
                    "dispatch:{:?}:run:{}",
                    run.automation_occurrence_id, run.id
                )
            })
            .ok_or_else(|| "The run is no longer available.".to_string())?;
        let claimed = self
            .dispatch_admission
            .as_ref()
            .is_some_and(|adapter| adapter.launch_claim_exists(&operation_key));
        if !claimed {
            return Err("This run has no unresolved launch claim.".into());
        }
        let replacement = if rerun {
            Some(
                self.store
                    .allocate_run_id()
                    .ok_or_else(|| "No run identifier is available.".to_string())?,
            )
        } else {
            None
        };
        let send = self
            .dispatch_admission
            .as_mut()
            .ok_or_else(|| "Durable dispatch admission is unavailable.".to_string())?
            .persist_resolution(&operation_key, rerun, replacement);
        if let Err(error) = send {
            self.dispatch_admission_error = Some(format!(
                "The resolution decision could not be persisted; nothing was changed: {error}"
            ));
            return Err(error);
        }
        self.dispatch_resolution_pending =
            Some((operation_key, run_id, rerun, replacement));
        cx.notify();
        Ok(())
    }

    fn execute_resolution(
        &mut self,
        operation_key: &str,
        rerun: bool,
        replacement: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if self.dispatch_resolution_pending.is_none() {
            let run_id = operation_key
                .rsplit(":run:")
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            self.dispatch_resolution_pending =
                Some((operation_key.to_string(), run_id, rerun, replacement));
        }
        let Some((key, run_id, rerun, replacement)) =
            self.dispatch_resolution_pending.clone()
        else {
            return;
        };
        if key != operation_key {
            return;
        }
        if let Some(new_id) = replacement {
            self.store.set_run_id_floor(
                self.store
                    .run_id_floor()
                    .max(new_id.saturating_add(1)),
            );
        }
        if let Some(mut run) = self.store.run_mut(run_id) {
            if run.status != "Blocked" {
                run.status = "Blocked".into();
                run.ended_at = Some(crate::opencode::timestamp());
                run.logs.push(if rerun {
                    "[dispatch] The user resolved an unconfirmed launch: the old attempt is closed and a distinct attempt will be admitted."
                } else {
                    "[dispatch] The user marked the unconfirmed launch as failed. The old attempt will not be relaunched."
                }
                .into());
            }
        }
        let blocked = self.store.runs().iter().find(|run| run.id == run_id).cloned();
        let Some(blocked) = blocked else {
            return;
        };
        let send = self
            .dispatch_admission
            .as_mut()
            .ok_or_else(|| "Durable dispatch admission is unavailable.".to_string())
            .and_then(|adapter| adapter.persist_resolution_status(&key, blocked));
        match send {
            Ok(()) => {}
            Err(error) => {
                self.dispatch_admission_error = Some(format!(
                    "The resolution status was not durably submitted; the claim and decision were retained: {error}"
                ));
            }
        }
        // Claim/marker retirement happens only in ResolutionStatusDurable.
    }

    /// Returns true when an asynchronous replacement/durability continuation is
    /// in flight or already proven. False means nothing was started and the
    /// status-retirement obligation must retry (never set `decision_started`).
    fn finish_resolution(
        &mut self,
        key: &str,
        run_id: u64,
        rerun: bool,
        replacement: Option<u64>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !rerun {
            if let Some(adapter) = self.dispatch_admission.as_mut() {
                return adapter.release_resolution(key).is_ok();
            }
            return false;
        }
        let Some(new_id) = replacement else {
            // Retain the decision marker: a malformed replacement invariant must
            // stay visible for recovery, not be silently erased.
            self.dispatch_admission_error = Some(
                "The rerun decision has no stable replacement identity; the decision was retained and no new attempt was started."
                    .into(),
            );
            return false;
        };
        let owner = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .map(|run| (run.task_id, run.automation_occurrence_id));
        if let Some(existing) = self.store.runs().iter().find(|run| run.id == new_id) {
            let Some((task_id, occurrence)) = owner else {
                self.dispatch_admission_error =
                    Some("The rerun source task is no longer available.".into());
                return false;
            };
            if existing.task_id != task_id
                || existing.automation_occurrence_id != occurrence
            {
                self.dispatch_admission_error = Some(format!(
                    "Run {new_id} already belongs to unrelated work; the rerun decision was retained."
                ));
                return false;
            }
            let persisted = self
                .last_saved
                .runs()
                .iter()
                .find(|run| run.id == new_id)
                .is_some_and(|run| run == existing);
            let claimed = self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.launch_claim_for_run(new_id).is_some());
            // Live status alone is not durable evidence: a Running/terminal
            // mutation that failed to persist must not retire the decision.
            if !(persisted || claimed) {
                self.dispatch_admission_error = Some(format!(
                    "Run {new_id} exists with matching ownership but has no durable admission/claim proof; the rerun decision was retained."
                ));
                return false;
            }
            if let Some(adapter) = self.dispatch_admission.as_mut() {
                return adapter.release_resolution(key).is_ok();
            }
            return false;
        }
        let task = owner.map(|(task, _)| task);
        let Some(task) = task else {
            self.dispatch_admission_error =
                Some("The rerun source task is no longer available.".into());
            return false;
        };
        let mut run = match self.store.prepare_run(task) {
            Ok(run) => run,
            Err(error) => {
                self.dispatch_admission_error =
                    Some(format!("The rerun attempt could not be prepared: {error}"));
                return false;
            }
        };
        run.id = new_id;
        let files = match self.prepare_run_materials(task, &mut run) {
            Ok(files) => files,
            Err(error) => {
                self.dispatch_admission_error =
                    Some(format!("The rerun materials are unavailable: {error}"));
                return false;
            }
        };
        self.dispatch_resolution_new = Some((key.to_string(), new_id));
        let intent = self.launch_intent_for(&run);
        let dispatch = self.launch_run_with_intent(run, files, intent, cx);
        dispatch.is_some()
            || self.pending_dispatch.is_some()
            || self.dispatch_awaiting.is_some()
            || self.dispatch_resolution_launch_pending.is_some()
    }

    fn launch_claimed(&mut self, operation_key: &str, cx: &mut Context<Self>) {
        let pending = self
            .dispatch_claim_pending
            .take()
            .filter(|(id, _)| id.operation_key == operation_key);
        let Some((id, parts)) = pending else {
            return;
        };
        let run_id = parts.run.id;
        let live_status = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .map(|run| run.status.clone())
            .unwrap_or_default();
        let already_scheduled = self.queued_runs.iter().any(|(queued, _)| *queued == run_id)
            || self.active_runs.iter().any(|run| run.id == run_id)
            || self.dispatch_launched.contains(operation_key);
        if !already_scheduled && matches!(live_status.as_str(), "Queued" | "Interrupted") {
            self.dispatch_launched.insert(operation_key.to_string());
            self.apply_effects(vec![CommandEffect::QueueRun { run_id }], cx);
            if let Some(entry) =
                self.queued_runs.iter_mut().find(|(queued, _)| *queued == run_id)
                && let Some(files) = self.dispatch_awaiting_files.take()
            {
                entry.1 = files;
            }
            self.toast = "Dispatch saved and launched once".into();
        } else {
            self.toast = "Dispatch was already launched".into();
        }
        self.begin_acknowledgement(id);
    }

    /// Claims are removed once the requested replacement attempt is durable
    /// and no longer needs the retired claim as its identity anchor.
    fn claims_removed_marker(&self) {}

    /// Retry the exact claim/marker retirement after a durable status receipt.
    /// A failed enqueue leaves this obligation in place; it never rewrites the
    /// status group or loses the resolution marker.
    fn retry_status_completion(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dispatch_parked {
            return false;
        }
        let Some(mut retirement) = self.dispatch_status_durable.clone() else {
            return false;
        };
        let previous = retirement.clone();
        let cancelling = self
            .dispatch_cancelled
            .get(&retirement.run_id)
            .is_some_and(|key| key == &retirement.key);
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return false;
        };
        if !cancelling {
            let _ = adapter.release_dispatch_intent(&retirement.key);
        }
        let mut error = None;
        if !retirement.claim_done {
            if !adapter.launch_claim_exists(&retirement.key) {
                retirement.claim_done = true;
                retirement.claim_enqueued = false;
            } else if !retirement.claim_enqueued {
                match adapter.release_launch_claim(&retirement.key) {
                    Ok(()) => retirement.claim_enqueued = true,
                    Err(reason) => {
                        error = Some(format!(
                            "Launch claim retirement was not queued; it will retry: {reason}"
                        ));
                    }
                }
            }
        }
        let mut marker_gone = false;
        if retirement.claim_done && !retirement.rerun && !retirement.decision_enqueued {
            if adapter.resolution(&retirement.key).is_none() {
                marker_gone = true;
            } else {
                match adapter.release_resolution(&retirement.key) {
                    Ok(()) => retirement.decision_enqueued = true,
                    Err(reason) => {
                        error = Some(format!(
                            "Resolution marker retirement was not queued; it will retry: {reason}"
                        ));
                    }
                }
            }
        }
        let rerun_now =
            retirement.rerun && retirement.claim_done && !retirement.decision_started;
        let mut rerun_started = false;
        if rerun_now {
            rerun_started = false;
        }
        marker_gone |= retirement.rerun
            && retirement.claim_done
            && adapter.resolution(&retirement.key).is_none();
        drop(adapter);
        if let Some(error) = error {
            self.dispatch_admission_error = Some(error);
        }
        if rerun_now {
            rerun_started = self.finish_resolution(
                &retirement.key,
                retirement.run_id,
                retirement.rerun,
                retirement.replacement,
                cx,
            );
            retirement.decision_started = rerun_started;
            if !rerun_started {
                error = Some(
                    "The rerun replacement could not be prepared or admitted; retirement will retry."
                        .into(),
                );
            }
        }
        if marker_gone {
            // The Cancelled status is durable. Retire the intent and marker in
            // receipt order; the marker stays durable until the intent removal
            // is acknowledged.
            if cancelling {
                let run_id = retirement.run_id;
                let key = retirement.key.clone();
                self.dispatch_status_durable = None;
                self.begin_cancel_retirement(run_id, key, cx);
                return true;
            }
            self.dispatch_status_durable = None;
            return true;
        }
        let changed = previous.claim_done != retirement.claim_done
            || previous.claim_enqueued != retirement.claim_enqueued
            || previous.decision_enqueued != retirement.decision_enqueued
            || previous.decision_started != retirement.decision_started;
        self.dispatch_status_durable = Some(retirement);
        changed
    }

    /// Claims are removed once the run reaches a confirmed state so they cannot
    /// grow without bound.
    pub(crate) fn cleanup_launch_claims(&mut self) -> bool {
        let claims = match self.dispatch_admission.as_ref() {
            Some(adapter) => adapter.launch_claims(),
            None => return false,
        };
        let mut changed = false;
        for operation_key in claims {
            let run_id = operation_key
                .rsplit(":run:")
                .next()
                .and_then(|value| value.parse::<u64>().ok());
            let Some(run_id) = run_id else {
                continue;
            };
            let resolving = self
                .dispatch_admission
                .as_ref()
                .is_some_and(|adapter| adapter.resolution(&operation_key).is_some())
                || self
                    .dispatch_resolution_pending
                    .as_ref()
                    .is_some_and(|(key, ..)| key == &operation_key)
                || self
                    .dispatch_resolution_new
                    .as_ref()
                    .is_some_and(|(key, _)| key == &operation_key);
            if resolving {
                // The claim anchors the still-pending resolution; it may only be
                // retired after that resolution status is durable.
                continue;
            }
            let confirmed = self.last_saved.runs().iter().find(|run| run.id == run_id).is_some_and(|run| {
                matches!(
                    run.status.as_str(),
                    "Running" | "Succeeded" | "Failed" | "Blocked" | "Cancelled" | "Stale"
                )
            });
            if confirmed
                && let Some(adapter) = self.dispatch_admission.as_mut()
            {
                let _ = adapter.release_launch_claim(&operation_key);
                changed = true;
            }
        }
        changed
    }

    /// QA-only bridge to the real pending-save retry path.
    #[cfg(feature = "qa")]
    pub(crate) fn qa_retry_pending(&mut self, cx: &mut Context<Self>) {
        self.retry_pending_saves(cx);
    }

    /// Retry retained/blocked dispatch groups without blocking the UI.
    /// Retry a parked dispatch obligation without allocating a new identity.
    /// The worker resubmits retained/blocked groups and re-persists intents;
    /// then the exact parked step (claim, cancellation status or resolution) is
    /// re-driven. Returns false when no parked obligation exists.
    pub(crate) fn retry_dispatch_admission(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.dispatch_parked {
            return false;
        }
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return false;
        };
        if let Err(error) = adapter.retry() {
            self.dispatch_admission_error = Some(format!(
                "The persistence worker is still unavailable; the exact dispatch intent is retained: {error}"
            ));
            cx.notify();
            return false;
        }
        self.dispatch_parked = false;
        self.dispatch_admission_error = None;

        // Re-persist any cancellation marker whose durability was not
        // confirmed. The key is idempotent, so a duplicate write is harmless.
        if let Some((run_id, key)) = self
            .dispatch_cancelled
            .iter()
            .next()
            .map(|(run_id, key)| (*run_id, key.clone()))
        {
            let id = crate::dispatch_admission::AdmissionId {
                operation_key: key,
                run_id,
                occurrence: None,
                generation: 0,
            };
            if let Some(adapter) = self.dispatch_admission.as_ref()
                && let Err(error) = adapter.persist_cancel(&id)
            {
                self.dispatch_admission_error = Some(format!(
                    "The cancellation marker still could not be queued; the dispatch stays held: {error}"
                ));
                self.dispatch_parked = true;
                cx.notify();
                return false;
            }
        }

        // Receipt-ordered retirement already in progress: re-drive its current
        // phase instead of allocating or claiming anything.
        if let Some(key) = self
            .dispatch_cancel_retire
            .first()
            .map(|pending| pending.operation_key.clone())
        {
            self.advance_cancel_retirement(&key, cx);
            // A re-drive may already have an idempotent request in flight; the
            // pump must still wake to observe its receipt.
            self.start_pump(cx);
            cx.notify();
            return true;
        }

        if let Some((id, _)) = self.dispatch_claim_pending.clone() {
            // The run is durable; only its at-most-once launch claim is missing.
            if let Some(adapter) = self.dispatch_admission.as_mut()
                && let Err(error) = adapter.claim_launch(&id)
            {
                self.dispatch_admission_error = Some(format!(
                    "The launch claim still could not be queued; no process was started: {error}"
                ));
                self.dispatch_parked = true;
                return false;
            }
        } else if self.dispatch_status_durable.is_none() {
            // A cancellation status write may have failed before it became
            // durable. Re-stage only when no status group is already pending;
            // otherwise `adapter.retry()` resubmitted it above.
            let cancelled_entry = self
                .dispatch_cancelled
                .iter()
                .find(|(run_id, _)| {
                    self.dispatch_awaiting
                        .as_ref()
                        .is_some_and(|id| id.run_id == **run_id)
                })
                .map(|(run_id, key)| (*run_id, key.clone()));
            if let Some((run_id, key)) = cancelled_entry {
                let already_pending = self
                    .dispatch_admission
                    .as_ref()
                    .is_some_and(|adapter| adapter.has_status_pending());
                if !already_pending {
                    let snapshot = self
                        .store
                        .runs()
                        .iter()
                        .find(|run| run.id == run_id && run.status == "Cancelled")
                        .cloned();
                    if let Some(run) = snapshot
                        && let Some(adapter) = self.dispatch_admission.as_mut()
                        && let Err(error) = adapter.persist_resolution_status(&key, run)
                    {
                        self.dispatch_admission_error = Some(format!(
                            "The cancellation status still could not be queued; it remains parked: {error}"
                        ));
                        self.dispatch_parked = true;
                        return false;
                    }
                }
            } else if let Some((key, _, rerun, replacement)) =
                self.dispatch_resolution_pending.clone()
            {
                let marker_missing = self
                    .dispatch_admission
                    .as_ref()
                    .is_some_and(|adapter| adapter.resolution(&key).is_none());
                if marker_missing
                    && let Some(adapter) = self.dispatch_admission.as_mut()
                    && let Err(error) =
                        adapter.persist_resolution(&key, rerun, replacement)
                {
                    self.dispatch_admission_error = Some(format!(
                        "The resolution decision still could not be persisted: {error}"
                    ));
                    self.dispatch_parked = true;
                    return false;
                }
            }
        }
        self.start_pump(cx);
        cx.notify();
        true
    }

    #[cfg(feature = "qa")]
    fn qa_crash_at(point: &str) {
        if std::env::var("T14_LB_CRASH").ok().as_deref() == Some(point) {
            std::process::abort();
        }
    }

    pub(crate) fn apply_effects(&mut self, effects: Vec<CommandEffect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                CommandEffect::EnqueueOccurrence(occurrence) => {
                    self.enqueue_automation_occurrence(occurrence, cx);
                }
                CommandEffect::QueueRun { run_id } => {
                    let files = match self.committed_run_files(run_id) {
                        Ok(files) => files,
                        Err(error) => {
                            self.error = format!(
                                "The committed run's materials are unavailable: {error}"
                            );
                            return;
                        }
                    };
                    if !self.queued_runs.iter().any(|(id, _)| *id == run_id) {
                        self.queued_runs.push((run_id, files));
                    }
                    #[cfg(feature = "qa")]
                    if let Some(fake) = self.qa_dispatch_fake.as_mut() {
                        fake.start(run_id);
                        cx.notify();
                        continue;
                    }
                    self.start_pump(cx);
                }
                CommandEffect::ArmAutomationTimer => self.arm_automation_timer(cx),
                CommandEffect::RemoveQueuedRuns(runs) => {
                    self.queued_runs.retain(|(id, _)| !runs.contains(id));
                }
                CommandEffect::StopRuns(runs) => {
                    for run in runs {
                        if self.active_runs.iter().any(|active| active.id == run)
                            || self.queued_runs.iter().any(|(id, _)| *id == run)
                        {
                            self.cancel_run_id(run, cx);
                        }
                    }
                }
                CommandEffect::Notice(feedback) => {
                    self.automation_ui.feedback =
                        Some(Ok(format!("{} · {}", feedback.title, feedback.detail)));
                    self.toast = feedback.title;
                }
            }
        }
    }

    /// Emergency stop: external work is stopped before durability. A failed
    /// cancellation save is reported truthfully and retained for retry; it never
    /// becomes a generic success receipt.
    pub(crate) fn stop_automation_safely(
        &mut self,
        occurrence: u64,
        cx: &mut Context<Self>,
    ) -> StopOutcome {
        let now = Self::automation_now();
        let Some(value) = self.store.occurrence(occurrence).cloned() else {
            return StopOutcome::StopRequestFailed {
                intent: StopIntent {
                    occurrence,
                    run_ids: Vec::new(),
                    requested_at: now,
                    reason: "user stop".into(),
                },
                error: "Automation period not found.".into(),
            };
        };
        let intent = StopIntent {
            occurrence,
            run_ids: value.run_ids.clone(),
            requested_at: now,
            reason: "user stop".into(),
        };
        self.stop_intents
            .retain(|item| item.occurrence != occurrence);
        self.stop_intents.push(intent.clone());
        let journal = self.persist_stop_intent(&intent);

        // Safety exception: do this before the storage commit. Keep the same
        // backend cleanup/signal semantics as the legacy stop path:
        // automation_started, fake runner cancellation, then active/queued runs.
        self.automation_started.remove(&occurrence);
        if self.automation_fake_mode
            && let Some(runner) = self.automation_fake.as_mut()
        {
            runner.cancel(occurrence);
        }
        for run in &intent.run_ids {
            self.cancel_run_id(*run, cx);
        }

        let command = Command::StopOccurrence { occurrence, now };
        let result = self
            .store
            .prepare_command(command.id(), command.class(), command)
            .map_err(command_error_text)
            .and_then(|prepared| self.commit_prepared_command(prepared, cx));
        match result {
            Ok(_) => StopOutcome::Stopped {
                durability: StopDurability::Saved,
            },
            Err(message) => {
                // A terminal in-memory state is not durability. Only a matching
                // last_saved Cancelled occurrence can prove the cancellation was
                // written; otherwise the journal and intent stay in place.
                let durable_cancel = self.pending_stop.is_none()
                    && self.last_saved.occurrence(occurrence).is_some_and(|value| {
                        value.status == crate::automation::OccurrenceStatus::Cancelled
                    })
                    && self.store.occurrence(occurrence).is_some_and(|value| {
                        value.status == crate::automation::OccurrenceStatus::Cancelled
                    });
                if durable_cancel {
                    self.clear_stop_intent(occurrence);
                    return StopOutcome::Stopped {
                        durability: StopDurability::Saved,
                    };
                }
                let terminal = self
                    .store
                    .occurrence(occurrence)
                    .is_some_and(|value| value.status.is_terminal());
                if terminal && self.last_saved == self.store {
                    self.clear_stop_intent(occurrence);
                    return StopOutcome::AlreadyEnded { durable: true };
                }
                let durability = match journal {
                    Ok(()) => StopDurability::NotSaved { message },
                    Err(error) => StopDurability::NotRecorded { message: error },
                };
                if terminal {
                    return StopOutcome::AlreadyEnded { durable: false };
                }
                StopOutcome::StopRequestedNotDurable { intent, durability }
            }
        }
    }

    fn persist_stop_intent(&self, intent: &StopIntent) -> Result<(), String> {
        let Some(root) = &self.workspace_root else {
            return Err(
                "Workspace root is unavailable; the stop intent was kept in memory only.".into(),
            );
        };
        stop_intent::record(
            root,
            &StopRecord {
                occurrence: intent.occurrence,
                run_ids: intent.run_ids.clone(),
                requested_at: intent.requested_at,
                reason: intent.reason.clone(),
            },
        )
    }

    pub(crate) fn clear_stop_intent(&mut self, occurrence: u64) {
        self.stop_intents
            .retain(|item| item.occurrence != occurrence);
        if let Some(root) = &self.workspace_root {
            let _ = stop_intent::clear(root, occurrence);
        }
    }

    /// Load and reconcile the stop journal *before* recovery can enqueue work.
    /// A malformed/unreadable/newer journal is fail-closed: the app stays
    /// viewable, but automation recovery/admission is paused until resolved.
    pub(crate) fn prepare_stop_recovery(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(root) = self.workspace_root.clone() else {
            self.stop_intents.clear();
            self.stop_journal_error = None;
            return true;
        };
        let (records, error) = stop_intent::load(&root);
        if let Some(error) = error {
            self.stop_journal_error = Some(error.clone());
            self.error = error;
            self.toast =
                "Automation recovery paused · resolve the stop-intent file before retrying".into();
            self.cancel_automation_timer();
            cx.notify();
            return false;
        }
        self.stop_journal_error = None;
        self.stop_intents = records
            .into_iter()
            .map(|record| StopIntent {
                occurrence: record.occurrence,
                run_ids: record.run_ids,
                requested_at: record.requested_at,
                reason: record.reason,
            })
            .collect();
        self.reconcile_loaded_stop_intents(&root, cx)
    }

    fn reconcile_loaded_stop_intents(
        &mut self,
        root: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let now = Self::automation_now();
        let mut all_durable = true;
        for intent in self.stop_intents.clone() {
            let Some(occurrence) = self.store.occurrence(intent.occurrence).cloned() else {
                if stop_intent::clear(root, intent.occurrence).is_ok() {
                    self.stop_intents
                        .retain(|value| value.occurrence != intent.occurrence);
                }
                continue;
            };
            if occurrence.status.is_terminal() {
                // Terminal in memory is not proof of durability. Clear only when
                // last_saved already contains the terminal state, or after an
                // explicit save succeeds. Otherwise a failed first reconciliation
                // would erase the only durable stop record while the database
                // still contains a replayable period.
                let durable = self
                    .last_saved
                    .occurrence(intent.occurrence)
                    .is_some_and(|value| value.status.is_terminal());
                if durable {
                    let _ = stop_intent::clear(root, intent.occurrence);
                    self.stop_intents
                        .retain(|value| value.occurrence != intent.occurrence);
                } else {
                    match self.save_current_store_durably(
                        crate::live_save_app::LiveCompletion::ClearStopIntent {
                            occurrence: intent.occurrence,
                            resume_recovery: true,
                        },
                        cx,
                    ) {
                        Ok(crate::live_save_app::SaveAcceptance::Durable) => {
                            let _ = stop_intent::clear(root, intent.occurrence);
                            self.stop_intents
                                .retain(|value| value.occurrence != intent.occurrence);
                        }
                        Ok(crate::live_save_app::SaveAcceptance::Pending) => {
                            // The journal/intent stays until the durable receipt;
                            // its completion clears it and resumes recovery.
                            all_durable = false;
                        }
                        Err(error) => {
                            all_durable = false;
                            self.storage_error = error;
                            self.error =
                                "A previous stop could not be saved; automation recovery stays paused."
                                    .into();
                            // Keep the journal and intent for a later retry.
                        }
                    }
                }
                continue;
            }
            self.queued_runs
                .retain(|(id, _)| !intent.run_ids.contains(id));
            if let Err(error) = self.live_can_accept_auxiliary() {
                all_durable = false;
                self.storage_error = error;
                continue;
            }
            let ids: Vec<u64> = self
                .store
                .runs()
                .iter()
                .filter(|run| {
                    run.automation_occurrence_id == Some(intent.occurrence)
                        && matches!(
                            run.status.as_str(),
                            "Queued" | "Running" | "Cancelling" | "Recovering" | "Interrupted"
                        )
                })
                .map(|run| run.id)
                .collect();
            for run_id in ids {
                let Some(mut run) = self.store.run_mut(run_id) else { continue };
                run.status = "Cancelled".into();
                run.ended_at = Some(crate::opencode::timestamp());
                run.logs.push(
                    "[automation] Stop requested before restart; the period was not replayed."
                        .into(),
                );
            }
            let _ = self.store.stop_occurrence(intent.occurrence, now);
            match self.save_current_store_durably(
                crate::live_save_app::LiveCompletion::ClearStopIntent {
                    occurrence: intent.occurrence,
                    resume_recovery: true,
                },
                cx,
            ) {
                Ok(crate::live_save_app::SaveAcceptance::Durable) => {
                    let _ = stop_intent::clear(root, intent.occurrence);
                    self.stop_intents
                        .retain(|value| value.occurrence != intent.occurrence);
                }
                Ok(crate::live_save_app::SaveAcceptance::Pending) => {
                    // Retain the journal until its matching durable receipt.
                    all_durable = false;
                }
                Err(error) => {
                    all_durable = false;
                    self.storage_error = error;
                    self.error =
                        "A previous stop could not be saved; automation recovery stays paused."
                            .into();
                    // Continue with the remaining records so one bad write does
                    // not hide another stopped period. Recovery stays blocked
                    // until every retained intent is durable.
                }
            }
        }
        if self.pending_stop.is_some() || !self.stop_intents.is_empty() {
            self.toast =
                "A previous stop was requested · retry save to keep it from restarting".into();
        }
        cx.notify();
        all_durable && self.stop_intents.is_empty()
    }

    pub(crate) fn save_current_store_durably(
        &mut self,
        completion: crate::live_save_app::LiveCompletion,
        cx: &mut Context<Self>,
    ) -> Result<crate::live_save_app::SaveAcceptance, String> {
        if self.live_persistence_active() {
            return self.submit_live_auxiliary(completion, cx);
        }
        if self.live_required {
            return Err(
                "Incremental persistence is unavailable. Nothing was written to a legacy file; retry when the worker is attached."
                    .into(),
            );
        }
        let receipt = self.store.drain_store_delta()?;
        let Some(storage) = self.storage.as_ref() else {
            return Err("Workspace storage is unavailable.".into());
        };
        match storage.save(&self.store) {
            Ok(()) => {
                self.store.retire_store_delta(&receipt);
                self.last_saved = self.store.clone();
                self.storage_error.clear();
                self.complete_live_now(completion, cx);
                Ok(crate::live_save_app::SaveAcceptance::Durable)
            }
            Err(error) => {
                self.storage_error = error.clone();
                Err(error)
            }
        }
    }
}
