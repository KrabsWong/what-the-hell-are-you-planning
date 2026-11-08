//! T13 S1/S2: TaskApp command executor, pending retry, dispatch intent, and
//! emergency stop handling.
//!
//! The domain command module owns preparation and validation. This module owns
//! the single storage commit, pending/reprepare UX, and post-commit effects.
//! Dispatch commits the run and occurrence link once before the queue effect.

use crate::app::{LaunchIntent, PendingDispatch, RunDispatch, TaskApp};
use crate::domain::Store;
use crate::domain_command::{
    Command, CommandClass, CommandEffect, CommandError, CommandReceipt, CommitError, CommitSink,
    PreparedCommand, StopDurability, StopIntent, StopOutcome,
};
use crate::persistence::Storage;
use crate::stop_intent::{self, StopRecord};
use gpui_kit::*;

struct StorageSink<'a> {
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
    ) -> Result<CommandReceipt, String> {
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
        let prepared = self
            .store
            .prepare_command(id, class, command)
            .map_err(command_error_text)?;
        self.commit_prepared_command(prepared, cx)
    }

    fn commit_prepared_command(
        &mut self,
        prepared: PreparedCommand,
        cx: &mut Context<Self>,
    ) -> Result<CommandReceipt, String> {
        let Some(storage) = self.storage.as_ref() else {
            return Err("Workspace storage is unavailable. Nothing was saved.".into());
        };
        let class = prepared.class;
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
                Ok(receipt)
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

    /// Footer Retry save / next-command retry. It never replays the old staged
    /// store: it reprepares from the current store under the retained identity.
    pub(crate) fn retry_pending_saves(&mut self, cx: &mut Context<Self>) {
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
            if let Command::DispatchRun { run, .. } = &command {
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
        let reserved_run = match &command {
            Command::DispatchRun { run, .. } => Some(run.clone()),
            _ => None,
        };
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
                self.dispatch_awaiting = Some(id);
                self.dispatch_awaiting_files = Some(files);
                self.dispatch_admission_error = None;
                self.toast = "Saving dispatch before launch".into();
                cx.notify();
                None
            }
            Err(rejection) => {
                if let Command::DispatchRun { run, .. } = &rejection.prepared.command {
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
            Some(adapter) => adapter.poll(),
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
                        self.dispatch_awaiting = Some(durable.id.clone());
                        self.dispatch_admission_error = None;
                        self.error.clear();
                    }
                    // Persist the launch claim before any spawn. The claim is the
                    // durable at-most-once marker for the external process.
                    let parts = std::sync::Arc::new(durable.parts.clone());
                    self.dispatch_claim_pending = Some((durable.id.clone(), parts.clone()));
                    if let Some(adapter) = self.dispatch_admission.as_mut() {
                        if let Err(error) = adapter.claim_launch(&durable.id) {
                            self.dispatch_admission_error = Some(format!(
                                "Launch claim could not be queued; no process was started: {error}"
                            ));
                            self.dispatch_claim_pending = None;
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
                    if self
                        .dispatch_status_durable
                        .as_ref()
                        .is_some_and(|(key, _)| key == &operation_key)
                    {
                        self.dispatch_status_durable = None;
                    }
                    if let Some((key, run_id, rerun, replacement)) =
                        self.dispatch_resolution_pending.clone()
                        && key == operation_key
                    {
                        self.finish_resolution(&key, run_id, rerun, replacement, cx);
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
                        self.dispatch_status_durable = None;
                    }
                    #[cfg(feature = "qa")]
                    Self::qa_crash_at("decision");
                    if let Some(launch_key) = self.dispatch_resolution_launch_pending.take() {
                        self.launch_claimed(&launch_key, cx);
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionFailed { reason, .. } => {
                    self.dispatch_admission_error = Some(format!(
                        "The resolution decision was not durable; nothing was changed: {reason}"
                    ));
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LaunchClaimFailed { reason, .. } => {
                    self.dispatch_admission_error = Some(format!(
                        "Launch claim was not durable; no process was started: {reason}"
                    ));
                    self.dispatch_claim_pending = None;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::ResolutionStatusFailed { reason, .. } => {
                    self.dispatch_admission_error = Some(reason);
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
                    self.dispatch_status_durable = Some((key, claimed));
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::Acknowledged { operation_key } => {
                    if self
                        .dispatch_awaiting
                        .as_ref()
                        .is_some_and(|id| id.operation_key == operation_key)
                    {
                        self.dispatch_awaiting = None;
                        self.dispatch_awaiting_files = None;
                        self.dispatch_ack_pending = None;
                        self.dispatch_admission_error = None;
                    }
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::AckFailed { reason, .. } => {
                    self.dispatch_admission_error = Some(reason);
                    self.dispatch_ack_pending = None;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LiveSaveDurable { epoch, .. } => {
                    self.store.retire_epoch(epoch);
                    self.storage_error.clear();
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::LiveSaveError { reason, .. } => {
                    self.storage_error = reason;
                    changed = true;
                }
                crate::dispatch_admission::AdmissionUpdate::StaleAck { reason, .. }
                | crate::dispatch_admission::AdmissionUpdate::Blocked { reason, .. }
                | crate::dispatch_admission::AdmissionUpdate::Retained { reason, .. } => {
                    self.dispatch_admission_error = Some(reason);
                    changed = true;
                }
            }
        }
        // Acknowledgement is retried only after an explicit failure; a pending
        // request is not re-sent every frame.
        if self.dispatch_ack_pending.is_none()
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
        let candidates: Vec<(u64, String)> = self
            .store
            .runs()
            .iter()
            .filter(|run| {
                run.status == "Queued"
                    && !self.queued_runs.iter().any(|(id, _)| *id == run.id)
                    && !self.active_runs.iter().any(|active| active.id == run.id)
                    && !self.dispatch_launched.contains(&format!(
                        "dispatch:{:?}:run:{}",
                        run.automation_occurrence_id, run.id
                    ))
            })
            .map(|run| {
                (
                    run.id,
                    format!("dispatch:{:?}:run:{}", run.automation_occurrence_id, run.id),
                )
            })
            .collect();
        let mut changed = false;
        for (run_id, operation_key) in candidates {
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

    fn finish_resolution(
        &mut self,
        key: &str,
        run_id: u64,
        rerun: bool,
        replacement: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if !rerun {
            if let Some(adapter) = self.dispatch_admission.as_mut() {
                let _ = adapter.release_resolution(key);
            }
            return;
        }
        let Some(new_id) = replacement else {
            // Retain the decision marker: a malformed replacement invariant must
            // stay visible for recovery, not be silently erased.
            self.dispatch_admission_error = Some(
                "The rerun decision has no stable replacement identity; the decision was retained and no new attempt was started."
                    .into(),
            );
            return;
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
                return;
            };
            if existing.task_id != task_id
                || existing.automation_occurrence_id != occurrence
            {
                self.dispatch_admission_error = Some(format!(
                    "Run {new_id} already belongs to unrelated work; the rerun decision was retained."
                ));
                return;
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
                return;
            }
            if let Some(adapter) = self.dispatch_admission.as_mut() {
                let _ = adapter.release_resolution(key);
            }
            return;
        }
        let task = owner.map(|(task, _)| task);
        let Some(task) = task else {
            self.dispatch_admission_error =
                Some("The rerun source task is no longer available.".into());
            return;
        };
        let mut run = match self.store.prepare_run(task) {
            Ok(run) => run,
            Err(error) => {
                self.dispatch_admission_error =
                    Some(format!("The rerun attempt could not be prepared: {error}"));
                return;
            }
        };
        run.id = new_id;
        let files = match self.prepare_run_materials(task, &mut run) {
            Ok(files) => files,
            Err(error) => {
                self.dispatch_admission_error =
                    Some(format!("The rerun materials are unavailable: {error}"));
                return;
            }
        };
        self.dispatch_resolution_new = Some((key.to_string(), new_id));
        let intent = self.launch_intent_for(&run);
        self.launch_run_with_intent(run, files, intent, cx);
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
        let Some((key, had_claim)) = self.dispatch_status_durable.clone() else {
            return false;
        };
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return false;
        };
        let _ = adapter.release_dispatch_intent(&key);
        if had_claim {
            if adapter.release_launch_claim(&key).is_err() {
                return false;
            }
            // `LaunchClaimReleased` finishes the resolution after the removal is
            // actually durable; the obligation stays until that update arrives.
            return false;
        }
        if let Some((run_id, rerun, replacement)) = self
            .dispatch_resolution_pending
            .as_ref()
            .map(|(_, run_id, rerun, replacement)| (*run_id, *rerun, *replacement))
        {
            if rerun {
                self.finish_resolution(&key, run_id, rerun, replacement, cx);
                self.dispatch_status_durable = None;
                return true;
            }
        }
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return false;
        };
        if adapter.release_resolution(&key).is_err() {
            return false;
        }
        self.dispatch_status_durable = None;
        true
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
    pub(crate) fn retry_dispatch_admission(&mut self) -> bool {
        let Some(adapter) = self.dispatch_admission.as_mut() else {
            return false;
        };
        match adapter.retry() {
            Ok(()) => true,
            Err(error) => {
                self.dispatch_admission_error = Some(error);
                false
            }
        }
    }

    #[cfg(feature = "qa")]
    fn qa_crash_at(point: &str) {
        if std::env::var("T14_LB_CRASH").ok().as_deref() == Some(point) {
            std::process::abort();
        }
    }

    fn apply_effects(&mut self, effects: Vec<CommandEffect>, cx: &mut Context<Self>) {
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
                    match self.save_current_store_durably() {
                        Ok(()) => {
                            let _ = stop_intent::clear(root, intent.occurrence);
                            self.stop_intents
                                .retain(|value| value.occurrence != intent.occurrence);
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
            match self.save_current_store_durably() {
                Ok(()) => {
                    let _ = stop_intent::clear(root, intent.occurrence);
                    self.stop_intents
                        .retain(|value| value.occurrence != intent.occurrence);
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

    pub(crate) fn save_current_store_durably(&mut self) -> Result<(), String> {
        let receipt = self.store.drain_store_delta()?;
        let Some(storage) = self.storage.as_ref() else {
            return Err("Workspace storage is unavailable.".into());
        };
        match storage.save(&self.store) {
            Ok(()) => {
                self.store.retire_store_delta(&receipt);
                self.last_saved = self.store.clone();
                self.storage_error.clear();
                Ok(())
            }
            Err(error) => {
                self.storage_error = error.clone();
                Err(error)
            }
        }
    }
}
