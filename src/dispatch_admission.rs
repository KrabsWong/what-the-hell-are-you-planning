//! T14 L1b: durable dispatch admission.
//!
//! The adapter is the real bridge between a prepared `DispatchRun` command and
//! the bounded ordered `PersistenceWorker`. It stages exactly the run, its
//! occurrence link and the next-id metadata into one atomic group. The caller
//! must not queue or launch the run until `AdmissionUpdate::Durable` returns the
//! matching receipt. Pending intents are persisted by the worker thread (atomic
//! temp write + fsync + rename + directory fsync) before submission, so a crash
//! or shutdown keeps the run identity without blocking the UI.

use crate::dispatch_intent::AdmissionParts;
use crate::domain::{AgentRun, Store};
use crate::domain_command::{Command, PreparedCommand};
use crate::persistence_journal::{
    ChangeJournal, DurableReceipt, EntityChange, EntityPayload, JournalError, MetadataDelta,
    StagedGroup,
};
use crate::persistence_worker::{PersistenceWorker, SubmitOutcome, WorkerEvent, WorkerOptions};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

const RESOLUTION_STATUS_TAG: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdmissionId {
    pub operation_key: String,
    pub run_id: u64,
    pub occurrence: Option<u64>,
    /// Zero until the group is staged against a known durable generation.
    pub generation: u64,
}

fn admission_id(parts: &AdmissionParts, generation: u64) -> AdmissionId {
    AdmissionId {
        operation_key: parts.operation_key.clone(),
        run_id: parts.run.id,
        occurrence: parts.occurrence.as_ref().map(|value| value.id),
        generation,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AdmissionState {
    PersistingIntent,
    WaitingForStartup,
    Submitted,
    Blocked { reason: String },
    Retained { reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DurableAdmission {
    pub id: AdmissionId,
    pub receipt: DurableReceipt,
    pub parts: AdmissionParts,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AdmissionUpdate {
    Durable(DurableAdmission),
    LaunchClaimPersisted { operation_key: String },
    LaunchClaimFailed { operation_key: String, reason: String },
    LaunchClaimReleased { operation_key: String },
    ResolutionPersisted { operation_key: String, rerun: bool, replacement: Option<u64> },
    ResolutionRestored { operation_key: String, rerun: bool, replacement: Option<u64> },
    ResolutionReleased { operation_key: String },
    ResolutionFailed { operation_key: String, reason: String },
    ResolutionStatusDurable {
        operation_key: String,
        run: AgentRun,
        receipt: DurableReceipt,
    },
    ResolutionStatusFailed {
        operation_key: String,
        reason: String,
    },
    Acknowledged { operation_key: String },
    AckFailed { operation_key: String, reason: String },
    Blocked { id: AdmissionId, reason: String },
    Retained { id: AdmissionId, reason: String },
    StaleAck { id: AdmissionId, reason: String },
    /// An ordinary live save group became durable. `epoch` is the dirty epoch
    /// captured when the group was staged.
    LiveSaveDurable { generation: u64, epoch: u64 },
    /// A live save group was blocked, rejected, returned or failed. The UI
    /// keeps the dirty state and retries later.
    LiveSaveError { generation: u64, epoch: u64, reason: String },
}

#[derive(Debug)]
pub(crate) struct AdmissionRejection {
    pub prepared: PreparedCommand,
    pub reason: String,
}

struct StatusPending {
    operation_key: String,
    run: AgentRun,
    group: StagedGroup,
    submitted: bool,
}

struct PendingAdmission {
    parts: std::sync::Arc<AdmissionParts>,
    id: AdmissionId,
    group: Option<StagedGroup>,
    intent_persisted: bool,
    state: AdmissionState,
}

#[derive(Debug, PartialEq, Eq)]
enum LiveReceiptMatch {
    None,
    Durable(u64),
    Mismatch(u64),
}

/// A live receipt is validated before its mapping is removed. A foreign
/// workspace or non-empty effect list can never retire a live group.
fn classify_live_receipt(
    live: &BTreeMap<u64, u64>,
    workspace: u64,
    receipt: &DurableReceipt,
) -> LiveReceiptMatch {
    let Some(epoch) = live.get(&receipt.generation).copied() else {
        return LiveReceiptMatch::None;
    };
    if receipt.workspace != workspace || !receipt.effects.is_empty() {
        return LiveReceiptMatch::Mismatch(epoch);
    }
    LiveReceiptMatch::Durable(epoch)
}

pub(crate) struct DispatchAdmissionAdapter {
    worker: PersistenceWorker,
    journal: Option<ChangeJournal>,
    workspace: u64,
    unstaged: VecDeque<PendingAdmission>,
    active: BTreeMap<u64, PendingAdmission>,
    awaiting_ack: BTreeMap<String, AdmissionParts>,
    launch_claims: BTreeSet<String>,
    resolutions: BTreeMap<String, (bool, Option<u64>)>,
    status_pending: BTreeMap<u64, StatusPending>,
    live_saves: BTreeMap<u64, u64>,
    live_retained: Option<(StagedGroup, u64)>,
    /// Exact accepted live groups retained for crash recovery until their
    /// receipt arrives. Cleared on the matching durable acknowledgement.
    live_copies: BTreeMap<u64, StagedGroup>,
    resolution_keys: BTreeMap<String, String>,
    known_keys: BTreeSet<String>,
    max_pending: usize,
    staged: usize,
    durable: usize,
    retained: usize,
    last_error: Option<String>,
}

impl DispatchAdmissionAdapter {
    pub(crate) fn open(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        intent_path: std::path::PathBuf,
    ) -> Result<Self, String> {
        Self::open_internal(path, root, workspace, options, intent_path, None)
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn open_with_test_options(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        intent_path: std::path::PathBuf,
        fault: Option<crate::sqlite_v4::Fault>,
    ) -> Result<Self, String> {
        Self::open_internal(path, root, workspace, options, intent_path, fault)
    }

    fn open_internal(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        intent_path: std::path::PathBuf,
        fault: Option<crate::sqlite_v4::Fault>,
    ) -> Result<Self, String> {
        let worker = match fault {
            #[cfg(any(test, feature = "qa"))]
            Some(fault) => PersistenceWorker::start_with_test_options_and_intents(
                path,
                root,
                workspace,
                options,
                std::time::Duration::ZERO,
                Some(fault),
                intent_path,
            )?,
            #[cfg(not(any(test, feature = "qa")))]
            Some(_) => unreachable!("fault injection is unavailable"),
            None => {
                PersistenceWorker::start_with_intents(path, root, workspace, options, intent_path)?
            }
        };
        Ok(Self {
            worker,
            journal: None,
            workspace,
            unstaged: VecDeque::new(),
            active: BTreeMap::new(),
            awaiting_ack: BTreeMap::new(),
            launch_claims: BTreeSet::new(),
            resolutions: BTreeMap::new(),
            status_pending: BTreeMap::new(),
            live_saves: BTreeMap::new(),
            live_retained: None,
            live_copies: BTreeMap::new(),
            resolution_keys: BTreeMap::new(),
            known_keys: BTreeSet::new(),
            max_pending: options.max_retained_groups.max(1),
            staged: 0,
            durable: 0,
            retained: 0,
            last_error: None,
        })
    }

    pub(crate) fn workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.worker.is_started() && self.journal.is_some()
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref().or_else(|| self.worker.last_error())
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.unstaged.len() + self.active.len()
    }

    pub(crate) fn retained_len(&self) -> usize {
        self.retained
    }

    pub(crate) fn staged_count(&self) -> usize {
        self.staged
    }

    pub(crate) fn durable_count(&self) -> usize {
        self.durable
    }

    pub(crate) fn pending_run_ids(&self) -> Vec<u64> {
        self.unstaged
            .iter()
            .chain(self.active.values())
            .map(|pending| pending.parts.run.id)
            .collect()
    }

    pub(crate) fn awaiting_acknowledgement(&self) -> Vec<AdmissionId> {
        self.awaiting_ack
            .values()
            .map(|parts| admission_id(parts, 0))
            .collect()
    }

    /// Admit one prepared DispatchRun. The worker persists the intent before the
    /// adapter stages a group; no SQLite write or launch happens before the
    /// durable receipt.
    pub(crate) fn admit(
        &mut self,
        prepared: PreparedCommand,
    ) -> Result<AdmissionId, AdmissionRejection> {
        let parts = match extract_parts(&prepared) {
            Ok(parts) => parts,
            Err(reason) => return Err(AdmissionRejection { prepared, reason }),
        };
        if let Some(existing) = self.find_id(&parts.operation_key) {
            return Ok(existing);
        }
        if self.pending_len() >= self.max_pending {
            return Err(AdmissionRejection {
                prepared,
                reason: "Dispatch admission budget is full; the intent was retained and no run was queued."
                    .into(),
            });
        }
        let id = admission_id(&parts, 0);
        let mut pending = PendingAdmission {
            parts: std::sync::Arc::new(parts),
            id: id.clone(),
            group: None,
            intent_persisted: false,
            state: AdmissionState::PersistingIntent,
        };
        self.known_keys.insert(pending.parts.operation_key.clone());
        if let Err(reason) = self.request_intent_persistence(&mut pending) {
            pending.state = AdmissionState::Retained { reason };
            self.retained += 1;
        }
        self.unstaged.push_back(pending);
        Ok(id)
    }

    /// Bounded, non-blocking poll. It never joins and never sleeps.
    pub(crate) fn poll(&mut self) -> Vec<AdmissionUpdate> {
        self.retry_live_group();
        self.retry_status_groups();
        let mut updates = Vec::new();
        for event in self.worker.poll() {
            match event {
                WorkerEvent::Started {
                    durable_generation,
                    intents,
                    ..
                } => {
                    self.journal = Some(ChangeJournal::resume(self.workspace, durable_generation));
                    self.restore_intents(intents, &mut updates);
                    self.stage_unstaged(&mut updates);
                }
                WorkerEvent::Durable { receipt, .. } => self.on_durable(receipt, &mut updates),
                WorkerEvent::IntentPersisted { key, result } => match result {
                    Ok(()) => {
                        if let Some(operation_key) = key.strip_prefix("launch:") {
                            self.launch_claims.insert(operation_key.to_string());
                            updates.push(AdmissionUpdate::LaunchClaimPersisted {
                                operation_key: operation_key.to_string(),
                            });
                        } else if let Some(rest) = key.strip_prefix("resolve:") {
                            if let Some((operation_key, rerun, replacement)) = parse_resolution(rest) {
                                self.resolutions
                                    .insert(operation_key.to_string(), (rerun, replacement));
                                self.resolution_keys.insert(operation_key.to_string(), key.clone());
                                updates.push(AdmissionUpdate::ResolutionPersisted {
                                    operation_key: operation_key.to_string(),
                                    rerun,
                                    replacement,
                                });
                            }
                        } else {
                            if let Some(pending) = self
                                .unstaged
                                .iter_mut()
                                .find(|pending| pending.parts.operation_key == key)
                            {
                                pending.intent_persisted = true;
                                pending.state = AdmissionState::WaitingForStartup;
                            }
                            self.stage_unstaged(&mut updates);
                        }
                    }
                    Err(reason) => {
                        if let Some(operation_key) = key.strip_prefix("launch:") {
                            updates.push(AdmissionUpdate::LaunchClaimFailed {
                                operation_key: operation_key.to_string(),
                                reason,
                            });
                        } else if let Some(rest) = key.strip_prefix("resolve:") {
                            let operation_key = rest
                                .rsplit_once(':')
                                .map(|(key, _)| key.to_string())
                                .unwrap_or_default();
                            updates.push(AdmissionUpdate::ResolutionFailed {
                                operation_key,
                                reason,
                            });
                        } else {
                            self.retain_unstaged(&key, reason, &mut updates);
                        }
                    }
                },
                WorkerEvent::IntentRemoved { key, result } => match result {
                    Ok(()) => {
                        if let Some(operation_key) = key.strip_prefix("launch:") {
                            self.launch_claims.remove(operation_key);
                            updates.push(AdmissionUpdate::LaunchClaimReleased {
                                operation_key: operation_key.to_string(),
                            });
                        } else if let Some(rest) = key.strip_prefix("resolve:") {
                            let Some((operation_key, _, _)) = parse_resolution(rest) else {
                                continue;
                            };
                            self.resolutions.remove(operation_key);
                            self.resolution_keys.remove(operation_key);
                            updates.push(AdmissionUpdate::ResolutionReleased {
                                operation_key: operation_key.to_string(),
                            });
                        } else {
                            self.awaiting_ack.remove(&key);
                            self.known_keys.remove(&key);
                            updates.push(AdmissionUpdate::Acknowledged {
                                operation_key: key,
                            });
                        }
                    }
                    Err(reason) => updates.push(AdmissionUpdate::AckFailed {
                        operation_key: key,
                        reason,
                    }),
                },
                WorkerEvent::Blocked {
                    generation,
                    error,
                    ..
                } => {
                    if let Some(epoch) = self.live_saves.get(&generation).copied() {
                        self.last_error = Some(format!("live save blocked: {error}"));
                        updates.push(AdmissionUpdate::LiveSaveError {
                            generation,
                            epoch,
                            reason: format!("live save blocked: {error}"),
                        });
                        continue;
                    }
                    if let Some(pending) = self.status_pending.get_mut(&generation) {
                        updates.push(AdmissionUpdate::ResolutionStatusFailed {
                            operation_key: pending.operation_key.clone(),
                            reason: format!("status write blocked: {error}"),
                        });
                        continue;
                    }
                    if let Some(pending) = self.active.get_mut(&generation) {
                        pending.state = AdmissionState::Blocked {
                            reason: error.clone(),
                        };
                        self.retained += 1;
                        updates.push(AdmissionUpdate::Blocked {
                            id: pending.id.clone(),
                            reason: error,
                        });
                    }
                }
                WorkerEvent::Returned { group, .. } => {
                    let generation = group.generation();
                    if let Some(epoch) = self.live_saves.get(&generation).copied() {
                        self.live_copies.remove(&generation);
                        self.live_retained = Some((group, epoch));
                        updates.push(AdmissionUpdate::LiveSaveError {
                            generation,
                            epoch,
                            reason: "worker returned the live save group before durability; it is retained"
                                .into(),
                        });
                        continue;
                    }
                    if let Some(pending) = self.status_pending.get_mut(&generation) {
                        pending.group = group;
                        pending.submitted = false;
                        updates.push(AdmissionUpdate::ResolutionStatusFailed {
                            operation_key: pending.operation_key.clone(),
                            reason: "worker returned the status group before durability; it is retained"
                                .into(),
                        });
                        continue;
                    }
                    if let Some(pending) = self.active.get_mut(&generation) {
                        pending.group = Some(group);
                        let reason = "worker returned the group before durability".to_string();
                        pending.state = AdmissionState::Retained {
                            reason: reason.clone(),
                        };
                        self.retained += 1;
                        updates.push(AdmissionUpdate::Retained {
                            id: pending.id.clone(),
                            reason,
                        });
                    }
                }
                WorkerEvent::Rejected { reason, group, .. } => {
                    let generation = group.generation();
                    if let Some(epoch) = self.live_saves.get(&generation).copied() {
                        self.live_copies.remove(&generation);
                        self.live_retained = Some((group, epoch));
                        updates.push(AdmissionUpdate::LiveSaveError {
                            generation,
                            epoch,
                            reason: format!("live save group rejected and retained: {reason:?}"),
                        });
                        continue;
                    }
                    if let Some(pending) = self.status_pending.get_mut(&generation) {
                        pending.group = group;
                        pending.submitted = false;
                        updates.push(AdmissionUpdate::ResolutionStatusFailed {
                            operation_key: pending.operation_key.clone(),
                            reason: format!("status group rejected and retained: {reason:?}"),
                        });
                        continue;
                    }
                    if let Some(pending) = self.active.get_mut(&generation) {
                        pending.group = Some(group);
                        let reason = format!("persistence rejected the dispatch group: {reason:?}");
                        pending.state = AdmissionState::Retained {
                            reason: reason.clone(),
                        };
                        self.retained += 1;
                        updates.push(AdmissionUpdate::Retained {
                            id: pending.id.clone(),
                            reason,
                        });
                    }
                }
                WorkerEvent::Failed { message } => {
                    self.last_error = Some(message.clone());
                    self.live_retained = None;
                    for (generation, epoch) in std::mem::take(&mut self.live_saves) {
                        updates.push(AdmissionUpdate::LiveSaveError {
                            generation,
                            epoch,
                            reason: message.clone(),
                        });
                    }
                    for pending in self.status_pending.values_mut() {
                        pending.submitted = false;
                        updates.push(AdmissionUpdate::ResolutionStatusFailed {
                            operation_key: pending.operation_key.clone(),
                            reason: message.clone(),
                        });
                    }
                    for pending in self.unstaged.iter_mut().chain(self.active.values_mut()) {
                        pending.state = AdmissionState::Retained {
                            reason: message.clone(),
                        };
                        self.retained += 1;
                        updates.push(AdmissionUpdate::Retained {
                            id: pending.id.clone(),
                            reason: message.clone(),
                        });
                    }
                }
                WorkerEvent::Holdings { .. } | WorkerEvent::Shutdown { .. } => {}
            }
        }
        updates
    }

    /// Retry retained/blocked groups and submit any staged waiting intents.
    pub(crate) fn retry(&mut self) -> Result<(), String> {
        self.worker.retry()?;
        self.retry_status_groups();
        let keys: Vec<String> = self
            .unstaged
            .iter()
            .filter(|pending| !pending.intent_persisted)
            .map(|pending| pending.parts.operation_key.clone())
            .collect();
        for key in keys {
            if let Some(index) = self
                .unstaged
                .iter()
                .position(|pending| pending.parts.operation_key == key)
            {
                let mut pending = self.unstaged.remove(index).unwrap();
                if let Err(reason) = self.request_intent_persistence(&mut pending) {
                    pending.state = AdmissionState::Retained { reason };
                    self.retained += 1;
                }
                self.unstaged.push_back(pending);
            }
        }
        let generations: Vec<u64> = self.active.keys().copied().collect();
        for generation in generations {
            let resubmit = self
                .active
                .get(&generation)
                .is_some_and(|pending| pending.group.is_some());
            if resubmit {
                self.submit_generation(generation);
            }
        }
        self.stage_unstaged(&mut Vec::new());
        Ok(())
    }

    /// Ask the worker to remove the durable intent. Completion is reported later
    /// as `AdmissionUpdate::Acknowledged`; callers must keep their surfaced
    /// pending state until then.
    pub(crate) fn acknowledge(&mut self, id: &AdmissionId) -> Result<(), String> {
        if !self.awaiting_ack.contains_key(&id.operation_key) {
            return Err(format!(
                "dispatch intent {} is not awaiting acknowledgement",
                id.operation_key
            ));
        }
        self.worker.remove_intent(id.operation_key.clone())
    }

    /// Persist an at-most-once launch claim before spawning. The claim is small
    /// relative to the run and lives in the same worker-owned budgeted store.
    pub(crate) fn claim_launch(&mut self, id: &AdmissionId) -> Result<(), String> {
        self.worker
            .persist_marker(format!("launch:{}", id.operation_key))
    }

    pub(crate) fn persist_resolution(
        &mut self,
        operation_key: &str,
        rerun: bool,
        replacement: Option<u64>,
    ) -> Result<(), String> {
        let decision = if rerun { "rerun" } else { "failed" };
        let suffix = replacement
            .map(|id| format!(":{id}"))
            .unwrap_or_default();
        self.worker
            .persist_marker(format!("resolve:{operation_key}:{decision}{suffix}"))
    }

    /// Persist the resolved old-run status as an ordered journal group. The
    /// caller must not release the claim or decision marker until the matching
    /// durable receipt arrives.
    pub(crate) fn persist_resolution_status(
        &mut self,
        operation_key: &str,
        run: AgentRun,
    ) -> Result<(), String> {
        let Some(journal) = self.journal.as_mut() else {
            return Err("persistence worker is not ready for resolution status".into());
        };
        let group = journal
            .stage_with_effects(
                vec![EntityChange::Upsert(EntityPayload::Run(run.clone()))],
                vec![run.id, RESOLUTION_STATUS_TAG],
            )
            .map_err(|error| format!("cannot stage resolution status: {error:?}"))?;
        let generation = group.generation();
        self.status_pending.insert(
            generation,
            StatusPending {
                operation_key: operation_key.to_string(),
                run,
                group,
                submitted: false,
            },
        );
        self.submit_status_generation(generation);
        Ok(())
    }

    fn submit_status_generation(&mut self, generation: u64) {
        let Some(pending) = self.status_pending.get_mut(&generation) else {
            return;
        };
        if pending.submitted {
            return;
        }
        let outcome = self.worker.submit(pending.group.clone());
        match outcome {
            SubmitOutcome::Accepted => pending.submitted = true,
            // Retain the exact staged group/generation for a later retry.
            SubmitOutcome::NotReady(_)
            | SubmitOutcome::Backpressure(_)
            | SubmitOutcome::Closed(_) => {}
        }
    }

    fn retry_status_groups(&mut self) {
        let generations: Vec<u64> = self.status_pending.keys().copied().collect();
        for generation in generations {
            self.submit_status_generation(generation);
        }
    }

    pub(crate) fn release_dispatch_intent(&mut self, operation_key: &str) -> Result<(), String> {
        self.worker.remove_intent(operation_key.to_string())
    }

    pub(crate) fn release_resolution(&mut self, operation_key: &str) -> Result<(), String> {
        let Some(full_key) = self.resolution_keys.get(operation_key).cloned() else {
            return Ok(());
        };
        self.worker.remove_intent(full_key)
    }

    pub(crate) fn resolution(&self, operation_key: &str) -> Option<(bool, Option<u64>)> {
        self.resolutions.get(operation_key).copied()
    }

    pub(crate) fn launch_claim_exists(&self, operation_key: &str) -> bool {
        self.launch_claims.contains(operation_key)
    }

    /// Remove a claim only after the caller explicitly resolved the ambiguity or
    /// confirmed a terminal run state.
    pub(crate) fn release_launch_claim(&mut self, operation_key: &str) -> Result<(), String> {
        self.worker
            .remove_intent(format!("launch:{operation_key}"))
    }

    pub(crate) fn launch_claims(&self) -> Vec<String> {
        self.launch_claims.iter().cloned().collect()
    }

    pub(crate) fn launch_claim_for_run(&self, run_id: u64) -> Option<String> {
        self.launch_claims
            .iter()
            .find(|key| key.ends_with(&format!(":run:{run_id}")))
            .cloned()
    }

    pub(crate) fn request_shutdown(&self) {
        self.worker.request_shutdown();
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }

    pub(crate) fn join(&mut self) -> bool {
        self.worker.join()
    }

    /// Test-only bounded shutdown. Production UI never blocks: it requests
    /// shutdown and polls `is_finished` before joining.
    #[cfg(test)]
    pub(crate) fn shutdown_for_test(&mut self) {
        self.request_shutdown();
        for _ in 0..400 {
            if self.is_finished() {
                assert!(self.join());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("worker did not finish within the test shutdown budget");
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn set_fault(&self, fault: crate::sqlite_v4::Fault) -> Result<(), String> {
        self.worker.set_fault(fault)
    }

    fn request_intent_persistence(
        &self,
        pending: &mut PendingAdmission,
    ) -> Result<(), String> {
        self.worker.persist_intent(
            pending.parts.operation_key.clone(),
            pending.parts.clone(),
        )?;
        pending.state = AdmissionState::PersistingIntent;
        Ok(())
    }

    fn restore_intents(
        &mut self,
        intents: Vec<(String, Option<std::sync::Arc<AdmissionParts>>)>,
        updates: &mut Vec<AdmissionUpdate>,
    ) {
        // Resolve markers and launch claims prove the old dispatch intent is
        // already durable. Re-admitting it on restart would occupy the pending
        // slot and could create an unrelated replacement id.
        let claimed_or_resolved: std::collections::BTreeSet<String> = intents
            .iter()
            .filter_map(|(key, _)| {
                key.strip_prefix("launch:")
                    .or_else(|| key.strip_prefix("resolve:"))
                    .map(|rest| {
                        rest.split_once(":rerun")
                            .map(|(value, _)| value.to_string())
                            .unwrap_or_else(|| rest.trim_end_matches(":failed").to_string())
                    })
            })
            .collect();
        for (stored_key, parts) in intents {
            if let Some(operation_key) = stored_key.strip_prefix("launch:") {
                self.launch_claims.insert(operation_key.to_string());
                continue;
            }
            if let Some(rest) = stored_key.strip_prefix("resolve:") {
                if let Some((operation_key, rerun, replacement)) = parse_resolution(rest) {
                    self.resolutions
                        .insert(operation_key.to_string(), (rerun, replacement));
                    self.resolution_keys.insert(operation_key.to_string(), stored_key.clone());
                    updates.push(AdmissionUpdate::ResolutionRestored {
                        operation_key: operation_key.to_string(),
                        rerun,
                        replacement,
                    });
                }
                continue;
            }
            let Some(parts) = parts else {
                continue;
            };
            if claimed_or_resolved.contains(&stored_key) {
                continue;
            }
            if !self.known_keys.insert(stored_key) {
                continue;
            }
            let id = admission_id(&parts, 0);
            self.unstaged.push_back(PendingAdmission {
                parts,
                id,
                group: None,
                intent_persisted: true,
                state: AdmissionState::WaitingForStartup,
            });
        }
    }

    fn stage_unstaged(&mut self, updates: &mut Vec<AdmissionUpdate>) {
        let keys: Vec<String> = self
            .unstaged
            .iter()
            .filter(|pending| pending.intent_persisted)
            .map(|pending| pending.parts.operation_key.clone())
            .collect();
        for key in keys {
            let Some(index) = self
                .unstaged
                .iter()
                .position(|pending| pending.parts.operation_key == key)
            else {
                continue;
            };
            let mut pending = self.unstaged.remove(index).unwrap();
            let result = match self.journal.as_mut() {
                Some(journal) => stage(journal, &pending.parts),
                None => {
                    self.unstaged.push_back(pending);
                    return;
                }
            };
            match result {
                Ok(group) => {
                    pending.id.generation = group.generation();
                    pending.group = Some(group);
                    let generation = pending.id.generation;
                    self.active.insert(generation, pending);
                    self.submit_generation(generation);
                }
                Err(reason) => {
                    pending.state = AdmissionState::Retained {
                        reason: reason.clone(),
                    };
                    self.retained += 1;
                    let id = pending.id.clone();
                    self.unstaged.push_back(pending);
                    updates.push(AdmissionUpdate::Retained { id, reason });
                }
            }
        }
    }

    fn submit_generation(&mut self, generation: u64) {
        let Some(pending) = self.active.get_mut(&generation) else {
            return;
        };
        let Some(group) = pending.group.clone() else {
            return;
        };
        match self.worker.submit(group) {
            SubmitOutcome::Accepted => {
                pending.state = AdmissionState::Submitted;
                self.staged += 1;
            }
            SubmitOutcome::NotReady(group) => {
                pending.group = Some(group);
                pending.state = AdmissionState::WaitingForStartup;
            }
            SubmitOutcome::Backpressure(group) => {
                pending.group = Some(group);
                pending.state = AdmissionState::Retained {
                    reason: "persistence budget is full; the dispatch is retained".into(),
                };
                self.retained += 1;
            }
            SubmitOutcome::Closed(group) => {
                pending.group = Some(group);
                pending.state = AdmissionState::Retained {
                    reason: "persistence worker is closed; the dispatch is retained".into(),
                };
                self.retained += 1;
            }
        }
    }

    fn on_durable(&mut self, receipt: DurableReceipt, updates: &mut Vec<AdmissionUpdate>) {
        match classify_live_receipt(&self.live_saves, self.workspace, &receipt) {
            LiveReceiptMatch::Durable(epoch) => {
                self.live_saves.remove(&receipt.generation);
                self.live_copies.remove(&receipt.generation);
                updates.push(AdmissionUpdate::LiveSaveDurable {
                    generation: receipt.generation,
                    epoch,
                });
                return;
            }
            LiveReceiptMatch::Mismatch(epoch) => {
                // Keep the mapping: the exact retained intent may still become
                // durable with a correctly addressed receipt.
                updates.push(AdmissionUpdate::LiveSaveError {
                    generation: receipt.generation,
                    epoch,
                    reason: "stale or mismatched live save receipt; group retained".into(),
                });
                return;
            }
            LiveReceiptMatch::None => {}
        }
        if let Some(pending) = self.status_pending.get(&receipt.generation) {
            if receipt.workspace == self.workspace
                && receipt.effects == vec![pending.run.id, RESOLUTION_STATUS_TAG]
            {
                let pending = self
                    .status_pending
                    .remove(&receipt.generation)
                    .expect("validated status pending");
                updates.push(AdmissionUpdate::ResolutionStatusDurable {
                    operation_key: pending.operation_key,
                    run: pending.run,
                    receipt,
                });
            } else {
                let pending = self
                    .status_pending
                    .get(&receipt.generation)
                    .expect("retained status pending");
                updates.push(AdmissionUpdate::StaleAck {
                    id: AdmissionId {
                        operation_key: pending.operation_key.clone(),
                        run_id: pending.run.id,
                        occurrence: None,
                        generation: receipt.generation,
                    },
                    reason: "mismatched resolution-status receipt; status group retained".into(),
                });
            }
            return;
        }
        let Some(pending) = self.active.remove(&receipt.generation) else {
            updates.push(AdmissionUpdate::StaleAck {
                id: AdmissionId {
                    operation_key: String::new(),
                    run_id: 0,
                    occurrence: None,
                    generation: receipt.generation,
                },
                reason: format!(
                    "durable receipt for unknown generation {}",
                    receipt.generation
                ),
            });
            return;
        };
        if receipt.workspace != self.workspace
            || receipt.generation != pending.id.generation
            || receipt.effects != pending.parts.effects()
        {
            let reason = format!(
                "stale or mismatched durable receipt for {:?}",
                pending.id.operation_key
            );
            self.active.insert(receipt.generation, pending);
            self.retained += 1;
            updates.push(AdmissionUpdate::StaleAck {
                id: AdmissionId {
                    operation_key: String::new(),
                    run_id: 0,
                    occurrence: None,
                    generation: receipt.generation,
                },
                reason,
            });
            return;
        }
        self.durable += 1;
        let parts = (*pending.parts).clone();
        self.awaiting_ack
            .insert(parts.operation_key.clone(), parts.clone());
        updates.push(AdmissionUpdate::Durable(DurableAdmission {
            id: pending.id,
            receipt,
            parts,
        }));
    }

    /// Stage and submit one ordinary live change group. This shares the same
    /// journal and worker as dispatch admission, so generations stay ordered and
    /// a dispatch can never overtake an earlier edit.
    pub(crate) fn submit_live_group(
        &mut self,
        changes: Vec<crate::persistence_journal::EntityChange>,
        metadata: Vec<crate::persistence_journal::MetadataDelta>,
        epoch: u64,
    ) -> Result<u64, String> {
        if changes.is_empty() && metadata.is_empty() {
            return Err("live save group is empty".into());
        }
        if let Some((_, pending_epoch)) = &self.live_retained {
            return Err(format!(
                "a previous live save group is retained (epoch {pending_epoch})"
            ));
        }
        let Some(journal) = self.journal.as_mut() else {
            return Err("live persistence is not ready".into());
        };
        let group = journal
            .stage_with_metadata(changes, metadata, Vec::new())
            .map_err(|error| format!("cannot stage live save group: {error:?}"))?;
        let generation = group.generation();
        self.live_saves.insert(generation, epoch);
        let copy = group.clone();
        match self.worker.submit(group) {
            SubmitOutcome::Accepted => {
                self.live_copies.insert(generation, copy);
                Ok(generation)
            }
            SubmitOutcome::NotReady(group) | SubmitOutcome::Backpressure(group) => {
                self.live_retained = Some((group, epoch));
                Ok(generation)
            }
            SubmitOutcome::Closed(_) => {
                self.live_saves.remove(&generation);
                Err("live persistence worker is closed".into())
            }
        }
    }

    /// Accepted live groups still awaiting a receipt, in generation order, plus
    /// the retained group. Recovery writes these exact boundaries.
    pub(crate) fn live_snapshot(&self) -> Vec<StagedGroup> {
        let mut groups: Vec<StagedGroup> = self.live_copies.values().cloned().collect();
        if let Some((group, _)) = &self.live_retained
            && !groups.iter().any(|existing| existing.generation() == group.generation())
        {
            groups.push(group.clone());
        }
        groups.sort_by_key(StagedGroup::generation);
        groups
    }

    pub(crate) fn live_pending(&self) -> bool {
        !self.live_saves.is_empty() || self.live_retained.is_some()
    }

    /// True while this exact generation still belongs to an unacknowledged live
    /// save. A duplicate or stale receipt for another generation must never
    /// complete a UI continuation.
    pub(crate) fn live_tracks(&self, generation: u64) -> bool {
        self.live_saves.contains_key(&generation)
    }

    pub(crate) fn live_workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn durable_generation(&self) -> u64 {
        self.worker.durable_generation()
    }

    pub(crate) fn is_started(&self) -> bool {
        self.worker.is_started()
    }

    pub(crate) fn startup_error(&self) -> Option<&str> {
        self.worker.startup_error()
    }

    /// Explicit footer retry: re-drain a retained live group and ask the worker
    /// to retry a parked generation. Effects remain gated on the next exact
    /// durable receipt.
    pub(crate) fn retry_live(&mut self) -> Result<(), String> {
        self.retry_live_group();
        if self.live_retained.is_some() {
            return Err("The previous live save is still retained by the persistence worker.".into());
        }
        self.worker.retry()
    }

    fn retry_live_group(&mut self) {
        let Some((group, epoch)) = self.live_retained.take() else {
            return;
        };
        let generation = group.generation();
        let copy = group.clone();
        match self.worker.submit(group) {
            SubmitOutcome::Accepted => {
                self.live_copies.insert(generation, copy);
            }
            SubmitOutcome::NotReady(group) | SubmitOutcome::Backpressure(group) => {
                self.live_retained = Some((group, epoch));
            }
            SubmitOutcome::Closed(_) => {
                self.live_saves.remove(&generation);
                self.last_error = Some("live persistence worker is closed".into());
            }
        }
    }

    fn retain_unstaged(&mut self, key: &str, reason: String, updates: &mut Vec<AdmissionUpdate>) {
        if let Some(pending) = self
            .unstaged
            .iter_mut()
            .find(|pending| pending.parts.operation_key == key)
        {
            pending.state = AdmissionState::Retained {
                reason: reason.clone(),
            };
            self.retained += 1;
            updates.push(AdmissionUpdate::Retained {
                id: pending.id.clone(),
                reason,
            });
        }
    }

    fn find_id(&self, operation_key: &str) -> Option<AdmissionId> {
        self.unstaged
            .iter()
            .chain(self.active.values())
            .find(|pending| pending.parts.operation_key == operation_key)
            .map(|pending| pending.id.clone())
            .or_else(|| {
                self.awaiting_ack
                    .get(operation_key)
                    .map(|parts| admission_id(parts, 0))
            })
    }
}

fn parse_resolution(rest: &str) -> Option<(&str, bool, Option<u64>)> {
    if let Some(operation_key) = rest.strip_suffix(":failed") {
        return Some((operation_key, false, None));
    }
    if let Some(operation_key) = rest.strip_suffix(":rerun") {
        return Some((operation_key, true, None));
    }
    let (base, replacement) = rest.rsplit_once(':')?;
    let replacement = replacement.parse::<u64>().ok()?;
    let operation_key = base.strip_suffix(":rerun")?;
    Some((operation_key, true, Some(replacement)))
}

fn extract_parts(prepared: &PreparedCommand) -> Result<AdmissionParts, String> {
    let Command::DispatchRun { occurrence, run } = &prepared.command else {
        return Err("durable admission currently accepts only DispatchRun commands".into());
    };
    let staged_run = prepared
        .staged
        .runs()
        .iter()
        .find(|candidate| candidate.id == run.id)
        .cloned()
        .ok_or_else(|| "the staged command does not contain its dispatch run".to_string())?;
    let occurrence = match occurrence {
        Some(id) => {
            let link = prepared.staged.occurrence(*id).cloned().ok_or_else(|| {
                "the staged command does not contain its occurrence link".to_string()
            })?;
            if !link.run_ids.contains(&run.id) {
                return Err("the staged occurrence is missing the dispatch run link".into());
            }
            Some(link)
        }
        None => None,
    };
    let next_id = (prepared.staged.next_id() != prepared.base.next_id)
        .then_some(prepared.staged.next_id());
    let run_id_floor = (prepared.staged.run_id_floor() != prepared.base.run_id_floor)
        .then_some(prepared.staged.run_id_floor());
    Ok(AdmissionParts {
        operation_key: prepared.id.operation_key.clone(),
        run: staged_run,
        occurrence,
        next_id,
        run_id_floor,
    })
}

fn stage(journal: &mut ChangeJournal, parts: &AdmissionParts) -> Result<StagedGroup, String> {
    let mut changes = vec![EntityChange::Upsert(EntityPayload::Run(parts.run.clone()))];
    if let Some(occurrence) = &parts.occurrence {
        changes.push(EntityChange::Upsert(EntityPayload::Occurrence(
            occurrence.clone(),
        )));
    }
    let mut metadata: Vec<MetadataDelta> =
        parts.next_id.map(MetadataDelta::NextId).into_iter().collect();
    if let Some(floor) = parts.run_id_floor {
        metadata.push(MetadataDelta::RunIdFloor(floor));
    }
    journal
        .stage_with_metadata(changes, metadata, parts.effects())
        .map_err(|error| match error {
            JournalError::EmptyGroup => "dispatch group is empty".to_string(),
            JournalError::GenerationOverflow => "dispatch generation overflow".to_string(),
        })
}

/// Apply only the exact durable payloads to a live store. It never assigns the
/// old staged snapshot over newer state. Returns whether the run was newly
/// inserted (the caller still owns launch idempotency).
pub(crate) fn apply_durable_parts(
    store: &mut Store,
    parts: &AdmissionParts,
) -> Result<bool, String> {
    let existing = store
        .runs()
        .iter()
        .find(|run| run.id == parts.run.id)
        .cloned();
    let inserted = match existing {
        None => {
            store.push_run(parts.run.clone());
            true
        }
        Some(_) => false,
    };
    if let Some(occurrence) = &parts.occurrence {
        match store.occurrence(occurrence.id) {
            None => store.push_occurrence(occurrence.clone()),
            Some(live) => {
                if !live.run_ids.contains(&parts.run.id) {
                    if let Some(mut link) = store.occurrence_mut(occurrence.id) {
                        link.run_ids.push(parts.run.id);
                    }
                }
            }
        }
    }
    if let Some(next_id) = parts.next_id {
        store.set_next_id(store.next_id().max(next_id));
    }
    if let Some(floor) = parts.run_id_floor {
        store.set_run_id_floor(store.run_id_floor().max(floor));
    }
    Ok(inserted)
}

/// Test-only launch recorder. It appends every actual call; tests assert
/// uniqueness instead of hiding duplicate launches inside the recorder.
#[cfg(feature = "qa")]
#[derive(Debug, Default)]
pub(crate) struct FakeDispatchRunner {
    pub starts: Vec<u64>,
    pub completions: Vec<u64>,
    pub launch_log: Option<std::path::PathBuf>,
}

#[cfg(feature = "qa")]
impl FakeDispatchRunner {
    pub(crate) fn start(&mut self, run_id: u64) {
        self.starts.push(run_id);
        if let Some(path) = &self.launch_log {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{run_id}");
                let _ = file.sync_all();
            }
        }
    }

    pub(crate) fn complete(&mut self, run_id: u64) {
        self.completions.push(run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentRun, Store};
    use crate::persistence_worker::WorkerOptions;
    use std::sync::atomic::{AtomicU64, Ordering};

    const WORKSPACE: u64 = 77;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Find evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "t14-l1b-{name}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn prepared_dispatch() -> (Store, PreparedCommand) {
        prepared_dispatch_with_run(1)
    }

    fn prepared_dispatch_with_run(run_id: u64) -> (Store, PreparedCommand) {
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("fixture/model".into());
        store.research_config_mut().models = std::array::from_fn(|_| "fixture/model".to_string());
        let goal = store
            .create_research_goal("Goal".into(), "Scope".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        let mut run = store.prepare_run(steps[0]).unwrap();
        run.id = run_id;
        run.status = "Queued".into();
        run.queued_at = "2026-11-09T00:00:00Z".into();
        run.started_at.clear();
        run.ended_at = None;
        run.exit_code = None;
        let command = Command::DispatchRun {
            occurrence: None,
            run,
        };
        let prepared = store
            .prepare_command(
                command.id(),
                crate::domain_command::CommandClass::StartOrExtend,
                command,
            )
            .unwrap();
        (store, prepared)
    }

    fn seeded(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = temp_root(name);
        let path = root.join("workspace.sqlite3");
        crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, WORKSPACE, &Store::mock())
            .unwrap();
        (root, path)
    }

    fn wait_ready(adapter: &mut DispatchAdmissionAdapter) {
        for _ in 0..400 {
            adapter.poll();
            if adapter.is_ready() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("adapter did not start: {:?}", adapter.last_error());
    }

    fn wait_durable(adapter: &mut DispatchAdmissionAdapter) -> DurableAdmission {
        for _ in 0..800 {
            for update in adapter.poll() {
                if let AdmissionUpdate::Durable(durable) = update {
                    return durable;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("dispatch did not become durable: {:?}", adapter.last_error());
    }

    fn wait_ack(adapter: &mut DispatchAdmissionAdapter, key: &str) {
        for _ in 0..400 {
            for update in adapter.poll() {
                if let AdmissionUpdate::Acknowledged { operation_key } = update
                    && operation_key == key
                {
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("acknowledgement was not confirmed: {:?}", adapter.last_error());
    }

    #[test]
    fn admission_persists_intent_before_group_and_ack_removes_it() {
        let (root, path) = seeded("admission");
        let intent = root.join("intent.json");
        let (_, prepared) = prepared_dispatch();
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            intent.clone(),
        )
        .unwrap();
        wait_ready(&mut adapter);
        let id = adapter.admit(prepared).unwrap();
        for _ in 0..200 {
            if intent.exists() {
                break;
            }
            adapter.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(intent.exists(), "worker persisted the intent");
        let durable = wait_durable(&mut adapter);
        assert_eq!(durable.id.run_id, id.run_id);
        assert_eq!(durable.parts.effects(), vec![id.run_id, 0]);
        adapter.acknowledge(&durable.id).unwrap();
        wait_ack(&mut adapter, &durable.id.operation_key);
        assert!(!intent.exists(), "acknowledgement removes the intent");
        adapter.shutdown_for_test();

        let reopened = crate::sqlite_v4::SqliteV4::open(&path, &root, WORKSPACE)
            .unwrap()
            .load_store()
            .unwrap();
        assert_eq!(
            reopened.runs().iter().filter(|run| run.id == id.run_id).count(),
            1
        );
    }

    #[test]
    fn stale_receipt_is_rejected_without_durable_launch() {
        let (root, path) = seeded("stale");
        let (_, prepared) = prepared_dispatch();
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            root.join("intent.json"),
        )
        .unwrap();
        wait_ready(&mut adapter);
        let id = adapter.admit(prepared).unwrap();
        let update = adapter.on_durable_for_test(DurableReceipt {
            workspace: WORKSPACE,
            generation: id.generation + 99,
            effects: vec![id.run_id, 0],
        });
        assert!(matches!(update, AdmissionUpdate::StaleAck { .. }));
        assert_eq!(adapter.durable_count(), 0);
        adapter.request_shutdown();
    }

    #[test]
    fn write_failure_blocks_then_retry_commits_once() {
        let (root, path) = seeded("retry");
        let (_, prepared) = prepared_dispatch();
        let intent = root.join("intent.json");
        let mut adapter = DispatchAdmissionAdapter::open_with_test_options(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            intent.clone(),
            Some(crate::sqlite_v4::Fault::BeforeCommit),
        )
        .unwrap();
        wait_ready(&mut adapter);
        let id = adapter.admit(prepared).unwrap();
        let mut blocked = false;
        for _ in 0..400 {
            for update in adapter.poll() {
                if matches!(update, AdmissionUpdate::Blocked { .. }) {
                    blocked = true;
                }
            }
            if blocked {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(blocked, "write failure must be reported as blocked");
        assert_eq!(adapter.durable_count(), 0);
        assert!(intent.exists());
        adapter.set_fault(crate::sqlite_v4::Fault::None).unwrap();
        adapter.retry().unwrap();
        let durable = wait_durable(&mut adapter);
        assert_eq!(durable.id.run_id, id.run_id);
        adapter.acknowledge(&durable.id).unwrap();
        wait_ack(&mut adapter, &durable.id.operation_key);
        let reopened = crate::sqlite_v4::SqliteV4::open(&path, &root, WORKSPACE)
            .unwrap()
            .load_store()
            .unwrap();
        assert_eq!(
            reopened.runs().iter().filter(|run| run.id == id.run_id).count(),
            1
        );
        adapter.request_shutdown();
    }

    #[test]
    fn crash_before_acknowledgement_restores_the_same_intent_without_duplicate_run() {
        let (root, path) = seeded("reopen");
        let (_, prepared) = prepared_dispatch();
        let intent = root.join("intent.json");
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            intent.clone(),
        )
        .unwrap();
        wait_ready(&mut adapter);
        let id = adapter.admit(prepared).unwrap();
        let durable = wait_durable(&mut adapter);
        assert_eq!(durable.id.run_id, id.run_id);
        // Simulate a crash after the durable commit but before acknowledgement.
        // Close the worker deterministically so the WAL is checkpointed before
        // the read-only preflight of the reopened database.
        adapter.shutdown_for_test();

        let mut reopened = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            intent.clone(),
        )
        .unwrap();
        wait_ready(&mut reopened);
        let restored = wait_durable(&mut reopened);
        assert_eq!(restored.id.run_id, id.run_id);
        assert_ne!(restored.id.generation, durable.id.generation);
        reopened.acknowledge(&restored.id).unwrap();
        wait_ack(&mut reopened, &restored.id.operation_key);
        reopened.request_shutdown();
        while !reopened.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        reopened.join();
        let store = crate::sqlite_v4::SqliteV4::open(&path, &root, WORKSPACE)
            .unwrap()
            .load_store()
            .unwrap();
        assert_eq!(
            store.runs().iter().filter(|run| run.id == id.run_id).count(),
            1,
            "crash recovery must not duplicate the run"
        );
    }

    #[test]
    fn full_admission_budget_rejects_without_persisting_a_second_intent() {
        let (root, path) = seeded("budget");
        let (_, first) = prepared_dispatch_with_run(1);
        let (_, second) = prepared_dispatch_with_run(2);
        let intent = root.join("intent.json");
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions {
                max_retained_groups: 1,
                channel_capacity: 1,
                ..WorkerOptions::default()
            },
            intent.clone(),
        )
        .unwrap();
        wait_ready(&mut adapter);
        adapter.admit(first).unwrap();
        let rejection = adapter.admit(second).unwrap_err();
        assert!(rejection.reason.contains("budget"));
        assert_eq!(rejection.prepared.command.id().kind, "dispatch_run");
        adapter.request_shutdown();
    }

    #[test]
    fn oversize_intent_is_retained_without_a_durable_group() {
        let (root, path) = seeded("oversize");
        let (_, prepared) = prepared_dispatch();
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions {
                max_retained_bytes: 64,
                ..WorkerOptions::default()
            },
            root.join("intent.json"),
        )
        .unwrap();
        wait_ready(&mut adapter);
        adapter.admit(prepared).unwrap();
        let mut retained = false;
        for _ in 0..200 {
            for update in adapter.poll() {
                if matches!(update, AdmissionUpdate::Retained { .. }) {
                    retained = true;
                }
            }
            if retained {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(retained, "oversize intent must be surfaced as retained");
        assert_eq!(adapter.pending_len(), 1, "caller obligation is retained");
        assert_eq!(adapter.durable_count(), 0);
        assert!(!root.join("intent.json").exists());
        adapter.request_shutdown();
    }

    #[test]
    fn startup_over_budget_intent_fails_and_leaves_the_file_intact() {
        let (root, path) = seeded("startup-budget");
        let (_, prepared) = prepared_dispatch();
        let parts = extract_parts(&prepared).unwrap();
        let intent = root.join("intent.json");
        let payload = serde_json::json!({"items":[{"key": parts.operation_key, "parts": parts}]});
        std::fs::write(&intent, serde_json::to_vec(&payload).unwrap()).unwrap();
        let before = std::fs::read(&intent).unwrap();
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions {
                max_retained_bytes: 64,
                ..WorkerOptions::default()
            },
            intent.clone(),
        )
        .unwrap();
        for _ in 0..200 {
            adapter.poll();
            if adapter.last_error().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            adapter
                .last_error()
                .is_some_and(|error| error.contains("budget exceeded at startup")),
            "{:?}",
            adapter.last_error()
        );
        assert_eq!(std::fs::read(&intent).unwrap(), before, "file must be preserved");
        adapter.request_shutdown();
    }

    #[test]
    fn intent_insert_failure_retains_obligation_and_retry_completes() {
        let (root, path) = seeded("insert-failure");
        let (_, prepared) = prepared_dispatch();
        let blocker = root.join("blocked");
        let intent = blocker.join("intent.json");
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            intent.clone(),
        )
        .unwrap();
        wait_ready(&mut adapter);
        std::fs::write(&blocker, b"not a directory").unwrap();
        let id = adapter.admit(prepared).unwrap();
        let mut retained = false;
        for _ in 0..200 {
            for update in adapter.poll() {
                if matches!(update, AdmissionUpdate::Retained { .. }) {
                    retained = true;
                }
            }
            if retained {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(retained);
        assert_eq!(adapter.pending_len(), 1);
        assert_eq!(adapter.durable_count(), 0);
        // Repair the filesystem and retry the same obligation.
        std::fs::remove_file(&blocker).unwrap();
        std::fs::create_dir_all(root.join("blocked")).unwrap();
        adapter.retry().unwrap();
        let durable = wait_durable(&mut adapter);
        assert_eq!(durable.id.run_id, id.run_id);
        adapter.acknowledge(&durable.id).unwrap();
        wait_ack(&mut adapter, &durable.id.operation_key);
        adapter.request_shutdown();
    }

    #[test]
    fn status_group_retains_exact_generation_under_backpressure() {
        let (root, path) = seeded("status-backpressure");
        let mut adapter = DispatchAdmissionAdapter::open(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions {
                max_retained_groups: 1,
                channel_capacity: 1,
                ..WorkerOptions::default()
            },
            root.join("intent.json"),
        )
        .unwrap();
        wait_ready(&mut adapter);
        adapter.worker.set_hold(true);
        // Fill the only retained-group slot with an ordinary journal group.
        let filler = adapter
            .journal
            .as_mut()
            .unwrap()
            .stage(vec![EntityChange::Upsert(EntityPayload::Run(
                extract_parts(&adapter_prepared_for_test(&root, 998))
                    .unwrap()
                    .run,
            ))])
            .unwrap();
        let filler_generation = filler.generation();
        assert!(matches!(
            adapter.worker.submit(filler),
            SubmitOutcome::Accepted
        ));
        let parts = extract_parts(&adapter_prepared_for_test(&root, 999)).unwrap();
        let mut blocked = parts.run.clone();
        blocked.status = "Blocked".into();
        adapter
            .persist_resolution_status("dispatch:None:run:999", blocked)
            .unwrap();
        let retained = filler_generation + 1;
        assert_eq!(*adapter.status_pending.keys().next().unwrap(), retained);
        assert!(
            !adapter.status_pending.get(&retained).unwrap().submitted,
            "status group must be retained under backpressure, not submitted"
        );
        adapter.worker.set_hold(false);
        let mut status_gen = None;
        for _ in 0..800 {
            for update in adapter.poll() {
                if let AdmissionUpdate::ResolutionStatusDurable { receipt, .. } = update {
                    status_gen = Some(receipt.generation);
                }
            }
            if status_gen.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(status_gen, Some(retained), "exact retained generation must commit");
        // A subsequent ordinary admission must commit at the next generation.
        adapter
            .admit(adapter_prepared_for_test(&root, 1_001))
            .unwrap();
        let mut last_gen = None;
        for _ in 0..800 {
            for update in adapter.poll() {
                if let AdmissionUpdate::Durable(value) = update
                    && value.parts.run.id == 1_001
                {
                    last_gen = Some(value.receipt.generation);
                }
            }
            if last_gen.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(last_gen, Some(retained + 1), "ordinary admission must follow contiguously");
        adapter.request_shutdown();
    }

    fn adapter_prepared_for_test(root: &std::path::Path, id: u64) -> PreparedCommand {
        let (mut store, _) = prepared_dispatch();
        store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
        let goal = store
            .create_research_goal("Second".into(), "Second".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        let mut run = store.prepare_run(steps[0]).unwrap();
        run.id = id;
        run.status = "Queued".into();
        run.queued_at = "2026-11-09T00:00:00Z".into();
        run.started_at.clear();
        let command = Command::DispatchRun {
            occurrence: None,
            run,
        };
        store
            .prepare_command(
                command.id(),
                crate::domain_command::CommandClass::StartOrExtend,
                command,
            )
            .unwrap()
    }

    #[test]
    fn apply_durable_parts_is_exact_and_does_not_overwrite_newer_state() {
        let mut store = Store::mock();
        let (_, prepared) = prepared_dispatch();
        let parts = extract_parts(&prepared).unwrap();
        assert!(apply_durable_parts(&mut store, &parts).unwrap());
        assert_eq!(
            store.runs().iter().filter(|run| run.id == parts.run.id).count(),
            1
        );
        // Identical replay is not a new insertion and must not request a launch.
        assert!(!apply_durable_parts(&mut store, &parts).unwrap());
        store.run_mut(parts.run.id).unwrap().status = "Succeeded".into();
        assert!(!apply_durable_parts(&mut store, &parts).unwrap());
        // Older content never overwrites newer state.
        let mut conflicting = parts.clone();
        conflicting.run.status = "Failed".into();
        assert!(!apply_durable_parts(&mut store, &conflicting).unwrap());
        assert_eq!(
            store.runs()[0].status,
            "Succeeded",
            "newer state must not be overwritten by a replay"
        );
    }

    #[test]
    fn live_receipt_validation_rejects_foreign_workspace_same_generation() {
        let mut live = BTreeMap::new();
        live.insert(7u64, 41u64);
        let receipt = DurableReceipt {
            workspace: 2,
            generation: 7,
            effects: Vec::new(),
        };
        assert_eq!(
            classify_live_receipt(&live, 1, &receipt),
            LiveReceiptMatch::Mismatch(41)
        );
        assert_eq!(live.get(&7), Some(&41), "mapping must not be retired");
    }

    #[test]
    fn live_receipt_validation_requires_exact_epoch_and_empty_effects() {
        let mut live = BTreeMap::new();
        live.insert(9u64, 55u64);
        let foreign_effects = DurableReceipt {
            workspace: 1,
            generation: 9,
            effects: vec![1],
        };
        assert_eq!(
            classify_live_receipt(&live, 1, &foreign_effects),
            LiveReceiptMatch::Mismatch(55)
        );
        let exact = DurableReceipt {
            workspace: 1,
            generation: 9,
            effects: Vec::new(),
        };
        assert_eq!(
            classify_live_receipt(&live, 1, &exact),
            LiveReceiptMatch::Durable(55)
        );
        assert_eq!(
            classify_live_receipt(&live, 1, &DurableReceipt {
                workspace: 1,
                generation: 10,
                effects: Vec::new(),
            }),
            LiveReceiptMatch::None
        );
    }
}

#[cfg(test)]
impl DispatchAdmissionAdapter {
    fn on_durable_for_test(&mut self, receipt: DurableReceipt) -> AdmissionUpdate {
        let mut updates = Vec::new();
        self.on_durable(receipt, &mut updates);
        updates.into_iter().next().expect("one admission update")
    }
}
