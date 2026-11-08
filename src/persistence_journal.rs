//! T14-S1a: pure changed-entity journal and ordered commit coordinator.
//!
//! Foundation only. It owns:
//! - typed changed-entity and typed metadata payloads for the real persisted shape;
//! - deterministic per-journal generation/base sequencing;
//! - chronology-preserving coalescing of one atomic change group;
//! - an ordered coordinator with an injectable sink, bounded group/byte retention,
//!   blocked-generation parking, retry, acknowledgement deduplication, workspace
//!   incarnation rejection and durable effect receipts.
//!
//! It never touches `TaskApp`, `Storage`, SQLite or the T13 command seam, and it
//! never clones, serializes or compares a whole `Store`: the caller supplies the
//! entities and metadata fields it changed. Live mutation coverage must be
//! compile-enforced by the future `Store` accessor design (not manual touch calls
//! and not corrective snapshots).
//!
//! Scope note: the six SQL entity tables plus the real workspace-metadata fields
//! are represented. `topics`, `research_requests`, `publications`, configuration
//! and counters are typed metadata deltas because that is how they are persisted
//! today. A future live writer must map every variant to the actual storage
//! layout; this module does not claim SQL row coverage on its own.

#![allow(dead_code)]

use crate::appearance::Appearance;
use crate::attachments::Attachment;
use crate::automation::{
    AutomationApproval, AutomationDefinition, AutomationOccurrence,
    AutomationOccurrenceSnapshot, AutomationPlanBinding, AutomationTemplate, TimeWindow,
};
use crate::domain::{AgentRun, Project, Stage, StagePolicy, Task, Topic};
use crate::model_profiles::ModelProfile;
use crate::report_publish::{Publication, PublishingConfig};
use crate::request_planning::ResearchRequest;
use crate::research::{ResearchConfig, ResearchTask};
use serde::Serialize;
use std::collections::VecDeque;

/// Persisted table identity used by the future incremental storage writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkTable {
    Tasks,
    Projects,
    Runs,
    Policies,
    Automations,
    Occurrences,
}

impl WorkTable {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Tasks => "tasks",
            Self::Projects => "projects",
            Self::Runs => "runs",
            Self::Policies => "policies",
            Self::Automations => "automations",
            Self::Occurrences => "automation_occurrences",
        }
    }
}

/// Stable identity of one changed entity. Policy rows have no numeric id, so the
/// stage ordinal is the row identity, matching the real `policies` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(crate) enum EntityKey {
    Task(u64),
    Project(u64),
    Run(u64),
    Policy(u8),
    Automation(u64),
    Occurrence(u64),
}

impl EntityKey {
    pub(crate) fn policy(stage: Stage) -> Self {
        Self::Policy(stage as u8)
    }

    pub(crate) fn table(self) -> WorkTable {
        match self {
            Self::Task(_) => WorkTable::Tasks,
            Self::Project(_) => WorkTable::Projects,
            Self::Run(_) => WorkTable::Runs,
            Self::Policy(_) => WorkTable::Policies,
            Self::Automation(_) => WorkTable::Automations,
            Self::Occurrence(_) => WorkTable::Occurrences,
        }
    }
}

/// One owned entity payload. These are real domain types, not a generic value
/// container, so a future writer can map each variant to its table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) enum EntityPayload {
    Task(Task),
    Project(Project),
    Run(AgentRun),
    Policy(StagePolicy),
    Automation(AutomationDefinition),
    Occurrence(AutomationOccurrence),
}

impl EntityPayload {
    pub(crate) fn key(&self) -> EntityKey {
        match self {
            Self::Task(task) => EntityKey::Task(task.id),
            Self::Project(project) => EntityKey::Project(project.id),
            Self::Run(run) => EntityKey::Run(run.id),
            Self::Policy(policy) => EntityKey::policy(policy.stage),
            Self::Automation(definition) => EntityKey::Automation(definition.id),
            Self::Occurrence(occurrence) => EntityKey::Occurrence(occurrence.id),
        }
    }

    pub(crate) fn table(&self) -> WorkTable {
        self.key().table()
    }

    /// Allocation-aware estimate of this payload's owned allocations (inline
    /// struct slot plus heap capacity). It never serializes the entity.
    pub(crate) fn estimated_bytes(&self) -> usize {
        match self {
            Self::Task(task) => task_heap(task),
            Self::Project(project) => project_heap(project),
            Self::Run(run) => run_heap(run),
            Self::Policy(policy) => policy_heap(policy),
            Self::Automation(definition) => automation_heap(definition),
            Self::Occurrence(occurrence) => occurrence_heap(occurrence),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    Upsert,
    Delete,
}

/// One explicit entity change supplied by the caller after a validated domain
/// delta. The payload is the full owned entity, so approval/activity companions
/// are included by construction.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) enum EntityChange {
    Upsert(EntityPayload),
    Delete(EntityKey),
}

impl EntityChange {
    pub(crate) fn key(&self) -> EntityKey {
        match self {
            Self::Upsert(payload) => payload.key(),
            Self::Delete(key) => *key,
        }
    }

    pub(crate) fn table(&self) -> WorkTable {
        self.key().table()
    }

    pub(crate) fn kind(&self) -> ChangeKind {
        match self {
            Self::Upsert(_) => ChangeKind::Upsert,
            Self::Delete(_) => ChangeKind::Delete,
        }
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        match self {
            // The group separately accounts the change slot; this returns only
            // the payload's owned allocations.
            Self::Upsert(payload) => payload.estimated_bytes(),
            Self::Delete(_) => 0,
        }
    }
}

/// Typed delta for fields persisted in the current single metadata row. This is
/// a real seam, not a generic map: each variant names the actual field/type.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) enum MetadataDelta {
    NextId(u64),
    DefaultModel(String),
    OpenCodePath(String),
    Appearance(Appearance),
    LastActiveProject(Option<u64>),
    AutomationSchemaVersion(u32),
    MaxConcurrentRuns(usize),
    MaxConcurrentAutomations(usize),
    ResearchConfig(ResearchConfig),
    ModelProfiles(Vec<ModelProfile>),
    ActiveModelProfile(Option<String>),
    Topics(Vec<Topic>),
    ResearchRequests(Vec<ResearchRequest>),
    Publications(Vec<Publication>),
    PublishingConfig(PublishingConfig),
}

impl MetadataDelta {
    pub(crate) fn field(&self) -> &'static str {
        match self {
            Self::NextId(_) => "next_id",
            Self::DefaultModel(_) => "default_model",
            Self::OpenCodePath(_) => "opencode_path",
            Self::Appearance(_) => "appearance",
            Self::LastActiveProject(_) => "last_active_project",
            Self::AutomationSchemaVersion(_) => "automation_schema_version",
            Self::MaxConcurrentRuns(_) => "max_concurrent_runs",
            Self::MaxConcurrentAutomations(_) => "max_concurrent_automations",
            Self::ResearchConfig(_) => "research_config",
            Self::ModelProfiles(_) => "model_profiles",
            Self::ActiveModelProfile(_) => "active_model_profile",
            Self::Topics(_) => "topics",
            Self::ResearchRequests(_) => "research_requests",
            Self::Publications(_) => "publications",
            Self::PublishingConfig(_) => "report_publishing",
        }
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        metadata_heap(self)
    }
}

/// One atomic group. Fields are private: only the journal can construct a
/// group, so a caller cannot bypass sequencing or coalescing with a malformed
/// value. `enqueue` also revalidates the non-empty invariant defensively.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StagedGroup {
    workspace: u64,
    generation: u64,
    base: u64,
    changes: Vec<EntityChange>,
    metadata: Vec<MetadataDelta>,
    effects: Vec<u64>,
    estimated_bytes: usize,
}

impl StagedGroup {
    pub(crate) fn workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn base(&self) -> u64 {
        self.base
    }

    pub(crate) fn changes(&self) -> &[EntityChange] {
        &self.changes
    }

    pub(crate) fn metadata(&self) -> &[MetadataDelta] {
        &self.metadata
    }

    pub(crate) fn effects(&self) -> &[u64] {
        &self.effects
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }

    pub(crate) fn keys(&self) -> Vec<EntityKey> {
        self.changes.iter().map(EntityChange::key).collect()
    }

    fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.metadata.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JournalError {
    EmptyGroup,
    GenerationOverflow,
}

/// Monotonic generation source for one workspace incarnation.
#[derive(Clone, Debug)]
pub(crate) struct ChangeJournal {
    workspace: u64,
    next_generation: u64,
}

impl ChangeJournal {
    pub(crate) fn new(workspace: u64) -> Self {
        Self {
            workspace,
            next_generation: 1,
        }
    }

    /// Resume after reopening a durable database. Generations must continue from
    /// the persisted durable generation, never restart at 1.
    pub(crate) fn resume(workspace: u64, durable_generation: u64) -> Self {
        Self {
            workspace,
            next_generation: durable_generation.saturating_add(1).max(1),
        }
    }

    pub(crate) fn workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn next_generation(&self) -> u64 {
        self.next_generation
    }

    pub(crate) fn stage(
        &mut self,
        changes: Vec<EntityChange>,
    ) -> Result<StagedGroup, JournalError> {
        self.stage_with_metadata(changes, Vec::new(), Vec::new())
    }

    pub(crate) fn stage_with_effects(
        &mut self,
        changes: Vec<EntityChange>,
        effects: Vec<u64>,
    ) -> Result<StagedGroup, JournalError> {
        self.stage_with_metadata(changes, Vec::new(), effects)
    }

    pub(crate) fn stage_with_metadata(
        &mut self,
        changes: Vec<EntityChange>,
        metadata: Vec<MetadataDelta>,
        effects: Vec<u64>,
    ) -> Result<StagedGroup, JournalError> {
        if changes.is_empty() && metadata.is_empty() {
            return Err(JournalError::EmptyGroup);
        }
        let changes = coalesce(changes);
        // Payload only: the group header and coordinator bookkeeping are not
        // charged to the payload budget.
        let mut estimated_bytes = 0usize;
        estimated_bytes = slots::<EntityChange>(estimated_bytes, changes.capacity());
        for change in &changes {
            estimated_bytes = sat(estimated_bytes, change.estimated_bytes());
        }
        estimated_bytes = slots::<MetadataDelta>(estimated_bytes, metadata.capacity());
        for delta in &metadata {
            estimated_bytes = sat(estimated_bytes, delta.estimated_bytes());
        }
        estimated_bytes = slots::<u64>(estimated_bytes, effects.capacity());
        let generation = self.next_generation;
        let base = generation - 1;
        self.next_generation = generation
            .checked_add(1)
            .ok_or(JournalError::GenerationOverflow)?;
        Ok(StagedGroup {
            workspace: self.workspace,
            generation,
            base,
            changes,
            metadata,
            effects,
            estimated_bytes,
        })
    }
}

/// Chronological last-operation semantics inside one atomic group.
///
/// A later operation on the same key removes the earlier one and is appended at
/// the later position. Delete-then-recreate therefore ends as an upsert, and the
/// final operation for every key is preserved exactly once. Unrelated keys are
/// never dropped. Groups are not merged with each other because each group is one
/// atomic commit.
fn coalesce(changes: Vec<EntityChange>) -> Vec<EntityChange> {
    let mut result: Vec<EntityChange> = Vec::with_capacity(changes.len());
    for change in changes {
        let key = change.key();
        if let Some(index) = result.iter().position(|existing| existing.key() == key) {
            result.remove(index);
        }
        result.push(change);
    }
    result
}

pub(crate) trait CommitSink {
    /// Commit one atomic group. Either every change/metadata delta is durable or
    /// none is. Proving SQLite transaction crash behaviour is a later live-sink
    /// concern; this trait only defines the contract.
    fn commit(&mut self, group: &StagedGroup) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommittedBatch {
    pub generation: u64,
    pub base: u64,
    pub keys: Vec<EntityKey>,
    pub kinds: Vec<ChangeKind>,
    pub metadata: Vec<&'static str>,
    pub effects: Vec<u64>,
    pub estimated_bytes: usize,
}

/// Deterministic test sink for routing only: records keys/kinds, not payload
/// application. Each listed generation fails exactly once.
#[derive(Default)]
pub(crate) struct RecordingSink {
    pub fail_once: Vec<u64>,
    pub attempts: Vec<u64>,
    pub commits: Vec<CommittedBatch>,
}

impl CommitSink for RecordingSink {
    fn commit(&mut self, group: &StagedGroup) -> Result<(), String> {
        self.attempts.push(group.generation());
        if let Some(index) = self
            .fail_once
            .iter()
            .position(|generation| *generation == group.generation())
        {
            self.fail_once.remove(index);
            return Err(format!(
                "injected failure at generation {}",
                group.generation()
            ));
        }
        self.commits.push(CommittedBatch {
            generation: group.generation(),
            base: group.base(),
            keys: group.keys(),
            kinds: group
                .changes()
                .iter()
                .map(EntityChange::kind)
                .collect(),
            metadata: group.metadata().iter().map(MetadataDelta::field).collect(),
            effects: group.effects().to_vec(),
            estimated_bytes: group.estimated_bytes(),
        });
        Ok(())
    }
}

/// Applying fake sink for exact-state tests. It applies upserts/deletes to a
/// working copy and swaps it in only after the whole group succeeds, so a failed
/// group leaves the state untouched. This proves the coordinator/payload
/// contract, not SQLite durability or crash safety.
#[derive(Default)]
pub(crate) struct ApplyingSink {
    pub fail_once: Vec<u64>,
    pub attempts: Vec<u64>,
    pub commits: Vec<u64>,
    pub state: std::collections::HashMap<EntityKey, EntityPayload>,
    pub metadata: Vec<Vec<MetadataDelta>>,
}

impl CommitSink for ApplyingSink {
    fn commit(&mut self, group: &StagedGroup) -> Result<(), String> {
        self.attempts.push(group.generation());
        if let Some(index) = self
            .fail_once
            .iter()
            .position(|generation| *generation == group.generation())
        {
            self.fail_once.remove(index);
            return Err(format!(
                "injected failure at generation {}",
                group.generation()
            ));
        }
        let mut next = self.state.clone();
        for change in group.changes() {
            match change {
                EntityChange::Upsert(payload) => {
                    next.insert(payload.key(), payload.clone());
                }
                EntityChange::Delete(key) => {
                    next.remove(key);
                }
            }
        }
        self.state = next;
        self.metadata.push(group.metadata().to_vec());
        self.commits.push(group.generation());
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnqueueError {
    WrongWorkspace {
        expected: u64,
        found: u64,
    },
    GenerationGap {
        expected: u64,
        found: u64,
    },
    BaseMismatch {
        expected: u64,
        found: u64,
    },
    StaleGeneration {
        durable: u64,
        found: u64,
    },
    InvalidGroup(&'static str),
    /// Group-count or byte budget exceeded. The group is returned intact.
    Overflow,
}

/// A recoverable enqueue rejection always returns the owned group so a caller
/// can retain the intent and retry it later. Nothing disappears.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnqueueRejection {
    pub reason: EnqueueError,
    pub group: StagedGroup,
}

impl EnqueueRejection {
    pub(crate) fn into_group(self) -> StagedGroup {
        self.group
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockedGroup {
    pub group: StagedGroup,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AwaitingAck {
    pub generation: u64,
    pub effects: Vec<u64>,
    pub estimated_bytes: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StepOutcome {
    Committed {
        generation: u64,
    },
    Blocked {
        generation: u64,
        error: String,
    },
    AwaitingAck {
        generation: u64,
    },
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ack {
    pub workspace: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DurableReceipt {
    pub workspace: u64,
    pub generation: u64,
    pub effects: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AckOutcome {
    Durable(DurableReceipt),
    Duplicate {
        generation: u64,
    },
    OutOfOrder {
        generation: u64,
        awaiting: Option<u64>,
    },
    WrongIncarnation {
        expected: u64,
        found: u64,
    },
}

/// Ordered commit coordinator for one workspace incarnation.
///
/// Retention is bounded by both group count and **payload bytes**, and includes
/// the group waiting for acknowledgement and any blocked group — not only queued
/// groups. The byte budget covers owned journal payload allocations (entity and
/// metadata heaps plus the changes/metadata/effects containers and effects). It
/// deliberately excludes the coordinator's own queue/header allocations, which
/// are fixed bookkeeping and not charged to the payload budget. An accepted
/// enqueue therefore never pushes `retained_bytes()` above the budget, and a
/// fully drained queue reports zero retained payload. Pipeline depth is one
/// committed-but-unacknowledged group.
pub(crate) struct CommitCoordinator<S: CommitSink> {
    sink: S,
    workspace: u64,
    durable_generation: u64,
    pending_generation: u64,
    queue: VecDeque<StagedGroup>,
    blocked: Option<BlockedGroup>,
    awaiting: Option<AwaitingAck>,
    max_pending_groups: usize,
    max_pending_bytes: usize,
}

impl<S: CommitSink> CommitCoordinator<S> {
    pub(crate) fn new(
        sink: S,
        workspace: u64,
        max_pending_groups: usize,
        max_pending_bytes: usize,
    ) -> Self {
        Self::with_durable_generation(
            sink,
            workspace,
            max_pending_groups,
            max_pending_bytes,
            0,
        )
    }

    /// Bootstrap a reopened workspace at its persisted durable generation.
    /// Generations must never reset to 1 on a nonempty commits database.
    pub(crate) fn with_durable_generation(
        sink: S,
        workspace: u64,
        max_pending_groups: usize,
        max_pending_bytes: usize,
        durable_generation: u64,
    ) -> Self {
        Self {
            sink,
            workspace,
            durable_generation,
            pending_generation: durable_generation,
            queue: VecDeque::new(),
            blocked: None,
            awaiting: None,
            max_pending_groups: max_pending_groups.max(1),
            max_pending_bytes: max_pending_bytes.max(1),
        }
    }

    pub(crate) fn workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn durable_generation(&self) -> u64 {
        self.durable_generation
    }

    /// Last enqueued generation. Enqueue never advances durability and never
    /// replaces caller state.
    pub(crate) fn pending_generation(&self) -> u64 {
        self.pending_generation
    }

    pub(crate) fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Total groups retained: queued + awaiting acknowledgement + blocked.
    pub(crate) fn retained_groups(&self) -> usize {
        self.queue.len()
            + usize::from(self.awaiting.is_some())
            + usize::from(self.blocked.is_some())
    }

    /// Total estimated bytes retained across queued, awaiting and blocked groups.
    pub(crate) fn retained_bytes(&self) -> usize {
        let mut retained = 0usize;
        for group in &self.queue {
            retained = sat(retained, group.estimated_bytes());
        }
        retained = sat(
            retained,
            self.awaiting
                .as_ref()
                .map_or(0, |awaiting| awaiting.estimated_bytes),
        );
        retained = sat(
            retained,
            self.blocked
                .as_ref()
                .map_or(0, |blocked| blocked.group.estimated_bytes()),
        );
        retained
    }

    pub(crate) fn blocked(&self) -> Option<&BlockedGroup> {
        self.blocked.as_ref()
    }

    /// Drain groups that were accepted but never committed. Used by shutdown to
    /// return exact owned intents instead of silently dropping them.
    pub(crate) fn take_uncommitted(&mut self) -> Vec<StagedGroup> {
        let mut groups: Vec<StagedGroup> = self.queue.drain(..).collect();
        if let Some(blocked) = self.blocked.take() {
            groups.insert(0, blocked.group);
        }
        groups
    }

    pub(crate) fn awaiting_generation(&self) -> Option<u64> {
        self.awaiting.as_ref().map(|awaiting| awaiting.generation)
    }

    pub(crate) fn sink(&self) -> &S {
        &self.sink
    }

    pub(crate) fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    pub(crate) fn enqueue(&mut self, group: StagedGroup) -> Result<(), EnqueueRejection> {
        let reject = |reason: EnqueueError, group: StagedGroup| EnqueueRejection { reason, group };
        if group.workspace() != self.workspace {
            return Err(reject(
                EnqueueError::WrongWorkspace {
                    expected: self.workspace,
                    found: group.workspace(),
                },
                group,
            ));
        }
        if group.is_empty() {
            return Err(reject(
                EnqueueError::InvalidGroup("a group must contain a change or metadata delta"),
                group,
            ));
        }
        if group.generation() <= self.durable_generation {
            return Err(reject(
                EnqueueError::StaleGeneration {
                    durable: self.durable_generation,
                    found: group.generation(),
                },
                group,
            ));
        }
        let expected_generation = self.pending_generation + 1;
        if group.generation() != expected_generation {
            return Err(reject(
                EnqueueError::GenerationGap {
                    expected: expected_generation,
                    found: group.generation(),
                },
                group,
            ));
        }
        if group.base() != self.pending_generation {
            return Err(reject(
                EnqueueError::BaseMismatch {
                    expected: self.pending_generation,
                    found: group.base(),
                },
                group,
            ));
        }
        if self.retained_groups() >= self.max_pending_groups
            || self.retained_bytes().saturating_add(group.estimated_bytes())
                > self.max_pending_bytes
        {
            return Err(reject(EnqueueError::Overflow, group));
        }
        self.pending_generation = group.generation();
        self.queue.push_back(group);
        debug_assert!(
            self.retained_bytes() <= self.max_pending_bytes,
            "accepted enqueue exceeded the payload budget"
        );
        Ok(())
    }

    /// Commit the next queued group. Refuses while a group awaits its
    /// acknowledgement or a failed group is parked.
    pub(crate) fn step(&mut self) -> StepOutcome {
        if let Some(awaiting) = &self.awaiting {
            return StepOutcome::AwaitingAck {
                generation: awaiting.generation,
            };
        }
        if let Some(blocked) = &self.blocked {
            return StepOutcome::Blocked {
                generation: blocked.group.generation(),
                error: blocked.error.clone(),
            };
        }
        let Some(group) = self.queue.pop_front() else {
            return StepOutcome::Idle;
        };
        let generation = group.generation();
        match self.sink.commit(&group) {
            Ok(()) => {
                let effects = group.effects().to_vec();
                let estimated_bytes = effects.capacity().saturating_mul(size_of::<u64>());
                self.awaiting = Some(AwaitingAck {
                    generation,
                    effects,
                    estimated_bytes,
                });
                StepOutcome::Committed { generation }
            }
            Err(error) => {
                self.blocked = Some(BlockedGroup {
                    group,
                    error: error.clone(),
                });
                StepOutcome::Blocked { generation, error }
            }
        }
    }

    /// Retry the parked generation. Successors stay parked until this succeeds
    /// and is acknowledged.
    pub(crate) fn retry(&mut self) -> StepOutcome {
        if self.awaiting.is_some() {
            return self.step();
        }
        let Some(blocked) = self.blocked.take() else {
            return self.step();
        };
        let generation = blocked.group.generation();
        match self.sink.commit(&blocked.group) {
            Ok(()) => {
                let effects = blocked.group.effects().to_vec();
                let estimated_bytes = effects.capacity().saturating_mul(size_of::<u64>());
                self.awaiting = Some(AwaitingAck {
                    generation,
                    effects,
                    estimated_bytes,
                });
                StepOutcome::Committed { generation }
            }
            Err(error) => {
                self.blocked = Some(BlockedGroup {
                    group: blocked.group,
                    error: error.clone(),
                });
                StepOutcome::Blocked { generation, error }
            }
        }
    }

    /// Release a committed group's effects exactly once. Duplicate, stale,
    /// out-of-order and wrong-incarnation acknowledgements never release effects
    /// and never advance durability. No unbounded history is retained:
    /// `durable_generation` already proves which generations were acknowledged.
    pub(crate) fn ack(&mut self, ack: Ack) -> AckOutcome {
        if ack.workspace != self.workspace {
            return AckOutcome::WrongIncarnation {
                expected: self.workspace,
                found: ack.workspace,
            };
        }
        if ack.generation <= self.durable_generation {
            return AckOutcome::Duplicate {
                generation: ack.generation,
            };
        }
        let awaiting = match self.awaiting.take() {
            Some(awaiting) => awaiting,
            None => {
                return AckOutcome::OutOfOrder {
                    generation: ack.generation,
                    awaiting: None,
                };
            }
        };
        if awaiting.generation != ack.generation {
            let awaiting_generation = awaiting.generation;
            self.awaiting = Some(awaiting);
            return AckOutcome::OutOfOrder {
                generation: ack.generation,
                awaiting: Some(awaiting_generation),
            };
        }
        self.durable_generation = ack.generation;
        AckOutcome::Durable(DurableReceipt {
            workspace: self.workspace,
            generation: ack.generation,
            effects: awaiting.effects,
        })
    }
}

// ---------------------------------------------------------------------------
// Allocation-aware size estimates (no serialization)
// ---------------------------------------------------------------------------
//
// The bound covers the owned payload's allocations: inline struct/enum slots,
// `Vec` capacity × element slot size, and `String` capacity bytes. It does not
// include allocator headers, allocator fragmentation, the process/runtime, or the
// coordinator's own bookkeeping outside the accounted containers. It is
// conservative for the payload allocations it covers, but it is **not** a total
// RSS bound. All sums saturate.

use std::mem::size_of;

fn sat(value: usize, add: usize) -> usize {
    value.saturating_add(add)
}

fn slots<T>(value: usize, capacity: usize) -> usize {
    sat(value, capacity.saturating_mul(size_of::<T>()))
}

fn text(value: usize, text: &String) -> usize {
    sat(value, text.capacity())
}

fn text_str(value: usize, text: &str) -> usize {
    sat(value, text.len())
}

fn opt_text(value: usize, text: &Option<String>) -> usize {
    text.as_ref().map_or(value, |text| self::text(value, text))
}

fn texts(value: usize, values: &Vec<String>) -> usize {
    let mut value = slots::<String>(value, values.capacity());
    for text in values {
        value = self::text(value, text);
    }
    value
}

fn opt_texts<const N: usize>(value: usize, values: &[Option<String>; N]) -> usize {
    let mut value = value;
    for text in values {
        value = opt_text(value, text);
    }
    value
}

fn attachments(value: usize, values: &Vec<Attachment>) -> usize {
    let mut value = slots::<Attachment>(value, values.capacity());
    for attachment in values {
        let Attachment {
            name,
            relative_path,
            media_type,
            size: _,
            content_sha256: _,
        } = attachment;
        value = text(value, name);
        value = text(value, relative_path);
        value = text(value, media_type);
    }
    value
}

fn sources(value: usize, values: &Vec<crate::research::Source>) -> usize {
    let mut value = slots::<crate::research::Source>(value, values.capacity());
    for source in values {
        let crate::research::Source {
            title,
            url,
            published_at,
        } = source;
        value = text(value, title);
        value = text(value, url);
        value = text(value, published_at);
    }
    value
}

fn comments(value: usize, values: &Vec<crate::domain::Comment>) -> usize {
    let mut value = slots::<crate::domain::Comment>(value, values.capacity());
    for comment in values {
        let crate::domain::Comment {
            author,
            body,
            created_at,
        } = comment;
        value = text(value, author);
        value = text(value, body);
        value = text(value, created_at);
    }
    value
}

fn research(value: usize, research: &Option<ResearchTask>) -> usize {
    match research {
        None => value,
        Some(research) => {
            let ResearchTask {
                requires_approval: _,
                kind: _,
                goal_id: _,
                depends_on,
                space_name,
            } = research;
            let value = slots::<u64>(value, depends_on.capacity());
            text(value, space_name)
        }
    }
}

fn planning_owner(value: usize, owner: &Option<crate::domain::PlanningOwner>) -> usize {
    match owner {
        None => value,
        Some(owner) => {
            let crate::domain::PlanningOwner {
                request_id: _,
                revision: _,
                automation_id: _,
            } = owner;
            value
        }
    }
}

fn schedule(value: usize, schedule: &crate::automation_schedule::Schedule) -> usize {
    let crate::automation_schedule::Schedule { spec: _, timezone } = schedule;
    text(value, timezone)
}

fn template(value: usize, template: &AutomationTemplate) -> usize {
    let AutomationTemplate { goals, window_rule: _ } = template;
    let mut value = slots::<crate::automation::AutomationGoal>(value, goals.capacity());
    for goal in goals {
        let crate::automation::AutomationGoal {
            title,
            brief,
            steps,
        } = goal;
        value = text(value, title);
        value = text(value, brief);
        value = slots::<crate::automation::AutomationTemplateStep>(value, steps.capacity());
        for step in steps {
            let crate::automation::AutomationTemplateStep {
                title,
                instructions,
                kind: _,
                depends_on,
            } = step;
            value = text(value, title);
            value = text(value, instructions);
            value = slots::<usize>(value, depends_on.capacity());
        }
    }
    value
}

fn approval(value: usize, approval: &Option<AutomationApproval>) -> usize {
    match approval {
        None => value,
        Some(approval) => {
            let AutomationApproval {
                version: _,
                approved_at: _,
                template_signature,
                schedule_signature,
                plan_request_id: _,
                plan_revision: _,
                models,
                agents,
                materials,
            } = approval;
            let mut value = text(value, template_signature);
            value = text(value, schedule_signature);
            for model in models {
                value = text(value, model);
            }
            value = opt_texts(value, agents);
            self::materials(value, materials)
        }
    }
}

fn plan_binding(value: usize, binding: &Option<AutomationPlanBinding>) -> usize {
    match binding {
        None => value,
        Some(binding) => {
            let AutomationPlanBinding {
                request_id: _,
                revision: _,
                project_id: _,
                input_signature,
            } = binding;
            text(value, input_signature)
        }
    }
}

fn materials(value: usize, materials: &Vec<crate::automation::AutomationMaterial>) -> usize {
    let mut value = slots::<crate::automation::AutomationMaterial>(value, materials.capacity());
    for material in materials {
        let crate::automation::AutomationMaterial {
            task_id: _,
            name,
            signature,
        } = material;
        value = text(value, name);
        value = text(value, signature);
    }
    value
}

fn time_window(value: usize, window: &TimeWindow) -> usize {
    let TimeWindow { state } = window;
    match state {
        crate::automation::TimeWindowState::Unresolved { reason } => text_str(value, reason),
        crate::automation::TimeWindowState::Resolved { start_utc, end_utc, label } => {
            let _ = (start_utc, end_utc);
            text_str(value, label)
        }
    }
}

fn snapshot(value: usize, snapshot: &AutomationOccurrenceSnapshot) -> usize {
    let AutomationOccurrenceSnapshot {
        objective,
        expected_result,
        shape: _,
        template,
        models,
        agents,
        materials,
        policy: _,
        approval_version: _,
        approval_signature,
    } = snapshot;
    let mut value = text(value, objective);
    value = text(value, expected_result);
    value = self::template(value, template);
    for model in models {
        value = text(value, model);
    }
    value = opt_texts(value, agents);
    value = self::materials(value, materials);
    text(value, approval_signature)
}

fn task_heap(task: &Task) -> usize {
    // Exhaustive destructuring: adding a field to `Task` must fail compilation
    // until this estimate is reviewed.
    let Task {
        id: _,
        created_at,
        automation_id: _,
        title,
        description,
        project_id: _,
        topic_id: _,
        approved_plan,
        stage: _,
        priority: _,
        labels,
        cycle,
        bucket,
        due_date,
        parent_id: _,
        comments,
        activity,
        archived: _,
        attachments,
        research,
    } = task;
    let mut value = size_of::<Task>();
    value = text(value, created_at);
    value = text(value, title);
    value = text(value, description);
    value = opt_text(value, approved_plan);
    value = texts(value, labels);
    value = opt_text(value, cycle);
    value = text(value, bucket);
    value = text(value, due_date);
    value = self::comments(value, comments);
    value = texts(value, activity);
    value = self::attachments(value, attachments);
    self::research(value, research)
}

fn run_heap(run: &AgentRun) -> usize {
    let AgentRun {
        automation_occurrence_id: _,
        planning_owner,
        deadline_at: _,
        auto_retry_attempt: _,
        retry_not_before: _,
        project_plan: _,
        id: _,
        task_title,
        directory,
        prompt,
        started_at,
        queued_at,
        workspace_source,
        ended_at,
        session_id,
        exit_code: _,
        task_id: _,
        agent,
        model,
        stage: _,
        status,
        logs,
        output,
        attachments,
        research_kind: _,
        sources,
        input_signature,
    } = run;
    let mut value = size_of::<AgentRun>();
    value = self::planning_owner(value, planning_owner);
    value = text(value, task_title);
    value = text(value, directory);
    value = text(value, prompt);
    value = text(value, started_at);
    value = text(value, queued_at);
    value = opt_text(value, workspace_source);
    value = opt_text(value, ended_at);
    value = opt_text(value, session_id);
    value = text(value, agent);
    value = text(value, model);
    value = text(value, status);
    value = texts(value, logs);
    value = text(value, output);
    value = self::attachments(value, attachments);
    value = self::sources(value, sources);
    text(value, input_signature)
}

fn automation_heap(definition: &AutomationDefinition) -> usize {
    let AutomationDefinition {
        id: _,
        version: _,
        project_id: _,
        shape: _,
        name,
        objective,
        expected_result,
        schedule,
        status: _,
        template,
        plan_binding,
        approval,
        policy: _,
        cursor: _,
        anchor: _,
        consecutive_failures: _,
        attention_reason,
        created_at: _,
        updated_at: _,
    } = definition;
    let mut value = size_of::<AutomationDefinition>();
    value = text(value, name);
    value = text(value, objective);
    value = text(value, expected_result);
    value = self::schedule(value, schedule);
    if let Some(template) = template {
        value = self::template(value, template);
    }
    value = self::plan_binding(value, plan_binding);
    value = self::approval(value, approval);
    opt_text(value, attention_reason)
}

fn occurrence_heap(occurrence: &AutomationOccurrence) -> usize {
    let AutomationOccurrence {
        id: _,
        automation_id: _,
        definition_version: _,
        source: _,
        trigger_key,
        scheduled_for: _,
        requested_at: _,
        actual_started_at: _,
        timezone,
        reference_time: _,
        data_window,
        snapshot,
        status: _,
        reason,
        research_goal_id: _,
        research_goal_ids,
        research_topic_id: _,
        run_ids,
        final_run_id: _,
        unread: _,
        skipped_count: _,
        skipped_from: _,
        skipped_to: _,
        created_at: _,
        deadline_at: _,
        retry_requested_at: _,
    } = occurrence;
    let mut value = size_of::<AutomationOccurrence>();
    value = text(value, trigger_key);
    value = text(value, timezone);
    value = time_window(value, data_window);
    value = self::snapshot(value, snapshot);
    if let Some(reason) = reason {
        let crate::automation::OccurrenceReason { code: _, detail } = reason;
        value = text(value, detail);
    }
    value = slots::<u64>(value, research_goal_ids.capacity());
    value = slots::<u64>(value, run_ids.capacity());
    value
}

fn policy_heap(policy: &StagePolicy) -> usize {
    let StagePolicy {
        stage: _,
        agent,
        model,
        prompt,
        auto_start: _,
    } = policy;
    let mut value = size_of::<StagePolicy>();
    value = text(value, agent);
    value = text(value, model);
    text(value, prompt)
}

fn project_heap(project: &Project) -> usize {
    let Project {
        id: _,
        name,
        key,
        directory,
        research_brief,
        kind: _,
    } = project;
    let mut value = size_of::<Project>();
    value = text(value, name);
    value = text(value, key);
    value = text(value, directory);
    text(value, research_brief)
}

fn topic_heap(topic: &Topic) -> usize {
    let Topic {
        replanned_from: _,
        automation_id: _,
        id: _,
        project_id: _,
        name,
        archived: _,
    } = topic;
    text(size_of::<Topic>(), name)
}

fn research_config_heap(config: &ResearchConfig) -> usize {
    let ResearchConfig {
        default_agent,
        codex_path,
        agents,
        models,
        ego_path,
    } = config;
    let mut value = size_of::<ResearchConfig>();
    value = text(value, default_agent);
    value = text(value, codex_path);
    value = opt_texts(value, agents);
    for model in models {
        value = text(value, model);
    }
    text(value, ego_path)
}

fn model_profile_heap(profile: &ModelProfile) -> usize {
    let ModelProfile {
        name,
        default_agent,
        default_model,
        agents,
        models,
    } = profile;
    let mut value = size_of::<ModelProfile>();
    value = text(value, name);
    value = text(value, default_agent);
    value = text(value, default_model);
    value = opt_texts(value, agents);
    for model in models {
        value = text(value, model);
    }
    value
}

fn request_heap(request: &ResearchRequest) -> usize {
    let ResearchRequest {
        id: _,
        project_id: _,
        topic_id: _,
        automation_id: _,
        expected_result,
        revision: _,
        provisional_topic_name,
        input,
        submitted_at,
        planning_run_id: _,
        goal_ids,
        brief,
    } = request;
    let mut value = size_of::<ResearchRequest>();
    value = opt_text(value, expected_result);
    value = opt_text(value, provisional_topic_name);
    value = text(value, input);
    value = text(value, submitted_at);
    value = slots::<u64>(value, goal_ids.capacity());
    if let Some(brief) = brief {
        let crate::request_planning::StructuredBrief {
            objective,
            deliverable,
            scope,
            assumptions,
            facts_to_verify,
        } = brief;
        value = text(value, objective);
        value = text(value, deliverable);
        value = text(value, scope);
        value = texts(value, assumptions);
        value = texts(value, facts_to_verify);
    }
    value
}

fn publication_heap(publication: &Publication) -> usize {
    let Publication {
        id,
        topic: _,
        fingerprint,
        config,
        title,
        markdown,
        status,
        commit,
        message,
    } = publication;
    let mut value = size_of::<Publication>();
    value = text(value, id);
    value = text(value, fingerprint);
    value = publishing_config_heap(value, config);
    value = text(value, title);
    value = text(value, markdown);
    value = text(value, status);
    value = opt_text(value, commit);
    text(value, message)
}

fn publishing_config_heap(value: usize, config: &PublishingConfig) -> usize {
    let PublishingConfig {
        repository,
        branch,
        base_url,
        automatic: _,
    } = config;
    let mut value = text(value, repository);
    value = text(value, branch);
    text(value, base_url)
}

fn metadata_heap(delta: &MetadataDelta) -> usize {
    match delta {
        MetadataDelta::NextId(_)
        | MetadataDelta::Appearance(_)
        | MetadataDelta::LastActiveProject(_)
        | MetadataDelta::AutomationSchemaVersion(_)
        | MetadataDelta::MaxConcurrentRuns(_)
        | MetadataDelta::MaxConcurrentAutomations(_) => size_of::<MetadataDelta>(),
        MetadataDelta::DefaultModel(text) | MetadataDelta::OpenCodePath(text) => {
            let value = size_of::<MetadataDelta>();
            self::text(value, text)
        }
        MetadataDelta::ActiveModelProfile(profile) => {
            let size = size_of::<MetadataDelta>();
            opt_text(size, profile)
        }
        MetadataDelta::ResearchConfig(config) => research_config_heap(config),
        MetadataDelta::ModelProfiles(profiles) => {
            let mut value = size_of::<MetadataDelta>();
            value = slots::<ModelProfile>(value, profiles.capacity());
            for profile in profiles {
                value = sat(value, model_profile_heap(profile));
            }
            value
        }
        MetadataDelta::Topics(topics) => {
            let mut value = size_of::<MetadataDelta>();
            value = slots::<Topic>(value, topics.capacity());
            for topic in topics {
                value = sat(value, topic_heap(topic));
            }
            value
        }
        MetadataDelta::ResearchRequests(requests) => {
            let mut value = size_of::<MetadataDelta>();
            value = slots::<ResearchRequest>(value, requests.capacity());
            for request in requests {
                value = sat(value, request_heap(request));
            }
            value
        }
        MetadataDelta::Publications(publications) => {
            let mut value = size_of::<MetadataDelta>();
            value = slots::<Publication>(value, publications.capacity());
            for publication in publications {
                value = sat(value, publication_heap(publication));
            }
            value
        }
        MetadataDelta::PublishingConfig(config) => {
            publishing_config_heap(size_of::<MetadataDelta>(), config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationPolicy, AutomationShape};
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Priority, Store};
    use std::collections::HashMap;

    const WORKSPACE: u64 = 7;
    const BYTES: usize = 8 * 1024 * 1024;

    fn task(id: u64, title: &str) -> Task {
        Task {
            id,
            created_at: "2026-11-08T00:00:00Z".into(),
            automation_id: None,
            title: title.into(),
            description: String::new(),
            project_id: 1,
            topic_id: None,
            approved_plan: None,
            stage: Stage::Backlog,
            priority: Priority::None,
            labels: Vec::new(),
            cycle: None,
            bucket: String::new(),
            due_date: String::new(),
            parent_id: None,
            comments: Vec::new(),
            activity: Vec::new(),
            archived: false,
            attachments: Vec::new(),
            research: None,
        }
    }

    fn task_upsert(id: u64, title: &str) -> EntityChange {
        EntityChange::Upsert(EntityPayload::Task(task(id, title)))
    }

    fn coordinator(max_groups: usize) -> CommitCoordinator<RecordingSink> {
        CommitCoordinator::new(RecordingSink::default(), WORKSPACE, max_groups, BYTES)
    }

    fn applying_coordinator(max_groups: usize) -> CommitCoordinator<ApplyingSink> {
        CommitCoordinator::new(ApplyingSink::default(), WORKSPACE, max_groups, BYTES)
    }

    fn stage(journal: &mut ChangeJournal, changes: Vec<EntityChange>) -> StagedGroup {
        journal.stage(changes).expect("group should stage")
    }

    fn durable<S: CommitSink>(
        coordinator: &mut CommitCoordinator<S>,
    ) -> DurableReceipt {
        let generation = coordinator
            .awaiting_generation()
            .expect("a group should be awaiting acknowledgement");
        match coordinator.ack(Ack {
            workspace: WORKSPACE,
            generation,
        }) {
            AckOutcome::Durable(receipt) => receipt,
            other => panic!("expected durable receipt, got {other:?}"),
        }
    }

    #[test]
    fn chronological_operations_on_one_key_are_all_preserved_across_groups() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(8);
        for change in [
            task_upsert(1, "created"),
            task_upsert(1, "updated"),
            EntityChange::Delete(EntityKey::Task(1)),
            task_upsert(1, "recreated"),
        ] {
            let group = stage(&mut journal, vec![change]);
            coordinator.enqueue(group).unwrap();
            assert!(matches!(
                coordinator.step(),
                StepOutcome::Committed { .. }
            ));
            durable(&mut coordinator);
        }
        let batches = &coordinator.sink().commits;
        assert_eq!(batches.len(), 4);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.kinds[0])
                .collect::<Vec<_>>(),
            vec![
                ChangeKind::Upsert,
                ChangeKind::Upsert,
                ChangeKind::Delete,
                ChangeKind::Upsert
            ]
        );
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.generation)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(coordinator.durable_generation(), 4);
    }

    #[test]
    fn applying_sink_proves_exact_state_chronology() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = applying_coordinator(8);
        let mut titles = Vec::new();
        for change in [
            task_upsert(1, "created"),
            task_upsert(1, "updated"),
            EntityChange::Delete(EntityKey::Task(1)),
            task_upsert(1, "recreated"),
        ] {
            coordinator
                .enqueue(stage(&mut journal, vec![change]))
                .unwrap();
            coordinator.step();
            durable(&mut coordinator);
            titles.push(match coordinator.sink().state.get(&EntityKey::Task(1)) {
                Some(EntityPayload::Task(task)) => Some(task.title.clone()),
                None => None,
                other => panic!("unexpected state {other:?}"),
            });
        }
        assert_eq!(
            titles,
            vec![
                Some("created".into()),
                Some("updated".into()),
                None,
                Some("recreated".into())
            ]
        );
    }

    #[test]
    fn within_group_coalescing_keeps_last_operation_and_unrelated_keys() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = stage(
            &mut journal,
            vec![
                task_upsert(1, "first"),
                task_upsert(2, "unrelated"),
                EntityChange::Delete(EntityKey::Task(1)),
                task_upsert(1, "recreated"),
            ],
        );
        assert_eq!(
            group.keys(),
            vec![EntityKey::Task(2), EntityKey::Task(1)]
        );
        assert_eq!(group.changes().len(), 2);
        match &group.changes()[1] {
            EntityChange::Upsert(EntityPayload::Task(task)) => {
                assert_eq!(task.title, "recreated")
            }
            other => panic!("expected the recreated task, got {other:?}"),
        }

        let delete_then_recreate = stage(
            &mut journal,
            vec![
                EntityChange::Delete(EntityKey::Task(3)),
                task_upsert(3, "back"),
            ],
        );
        assert_eq!(delete_then_recreate.changes().len(), 1);
        assert_eq!(
            delete_then_recreate.changes()[0].kind(),
            ChangeKind::Upsert
        );
    }

    #[test]
    fn journal_payloads_exactly_match_the_supplied_changed_entities() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let expected = task(9, "exact payload");
        let group = stage(
            &mut journal,
            vec![EntityChange::Upsert(EntityPayload::Task(expected.clone()))],
        );
        assert_eq!(group.changes().len(), 1);
        assert_eq!(
            group.changes()[0],
            EntityChange::Upsert(EntityPayload::Task(expected))
        );
        assert_eq!(group.changes()[0].table(), WorkTable::Tasks);
        assert!(matches!(
            journal.stage(Vec::<EntityChange>::new()),
            Err(JournalError::EmptyGroup)
        ));
    }

    #[test]
    fn task_payload_includes_approval_and_activity_companions() {
        let mut changed = task(4, "Approved goal");
        changed.approved_plan = Some("approved-signature".into());
        changed.activity = vec!["Reviewed · 2026-11-08T00:00:00Z".into()];
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = applying_coordinator(4);
        coordinator
            .enqueue(stage(
                &mut journal,
                vec![EntityChange::Upsert(EntityPayload::Task(changed))],
            ))
            .unwrap();
        coordinator.step();
        durable(&mut coordinator);
        match coordinator.sink().state.get(&EntityKey::Task(4)) {
            Some(EntityPayload::Task(task)) => {
                assert_eq!(task.approved_plan.as_deref(), Some("approved-signature"));
                assert_eq!(task.activity.len(), 1);
            }
            other => panic!("task companions missing: {other:?}"),
        }
    }

    #[test]
    fn typed_metadata_delta_is_staged_and_bounded() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = journal
            .stage_with_metadata(
                Vec::new(),
                vec![
                    MetadataDelta::NextId(42),
                    MetadataDelta::Topics(vec![Topic {
                        id: 1,
                        project_id: 1,
                        name: "Research".into(),
                        archived: false,
                        automation_id: None,
                        replanned_from: None,
                    }]),
                ],
                vec![3],
            )
            .unwrap();
        assert_eq!(group.metadata().len(), 2);
        assert_eq!(group.metadata()[0].field(), "next_id");
        assert_eq!(group.metadata()[1].field(), "topics");
        assert!(group.estimated_bytes() > 0);
        assert!(matches!(
            journal.stage_with_metadata(Vec::new(), Vec::new(), Vec::new()),
            Err(JournalError::EmptyGroup)
        ));
    }

    #[test]
    fn atomic_cursor_and_occurrence_group_is_one_indivisible_commit() {
        let (automation, occurrence_id, definition, period) = atomic_automation_case();
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = stage(
            &mut journal,
            vec![
                EntityChange::Upsert(EntityPayload::Automation(definition)),
                EntityChange::Upsert(EntityPayload::Occurrence(period)),
            ],
        );
        let mut coordinator = applying_coordinator(4);
        coordinator.enqueue(group).unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { .. }
        ));
        assert_eq!(coordinator.sink().commits.len(), 1);
        assert_eq!(coordinator.sink().state.len(), 2);
        assert!(coordinator
            .sink()
            .state
            .contains_key(&EntityKey::Automation(automation)));
        assert!(coordinator
            .sink()
            .state
            .contains_key(&EntityKey::Occurrence(occurrence_id)));
    }

    fn atomic_automation_case() -> (u64, u64, AutomationDefinition, AutomationOccurrence) {
        let mut store = Store::empty();
        store.projects[0].directory = "/tmp/t14-s1a".into();
        store.default_model = "qa/automation".into();
        store.research_config.models = std::array::from_fn(|_| "qa/automation".into());
        let schedule = Schedule {
            spec: ScheduleSpec::Interval { minutes: 10 },
            timezone: "UTC".into(),
        };
        let template = crate::automation_execution::build_quick_template(
            "Check one value and cite it.",
            "One short answer.",
        )
        .unwrap();
        let automation = store
            .create_automation_draft(
                1,
                AutomationShape::Quick,
                "S1a".into(),
                "Check one value and cite it.".into(),
                "One short answer.".into(),
                schedule,
                AutomationPolicy::default(),
                Some(template),
                1_000,
            )
            .unwrap();
        store.approve_and_enable(automation, vec![], 1_000).unwrap();
        store.automation_mut(automation).unwrap().cursor = Some(2_000);
        let occurrence = store
            .request_manual_occurrence(automation, "s1a-atomic", 1_500)
            .unwrap();
        let occurrence_id = occurrence.id;
        let definition = store.automation(automation).unwrap().clone();
        let period = store.occurrence(occurrence_id).unwrap().clone();
        (automation, occurrence_id, definition, period)
    }

    #[test]
    fn failed_generation_parks_successors_and_retry_commits_in_order_once() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(4);
        coordinator.sink_mut().fail_once.push(1);
        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(1, "one")]))
            .unwrap();
        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(2, "two")]))
            .unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Blocked { generation: 1, .. }
        ));
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Blocked { generation: 1, .. }
        ));
        assert!(coordinator.sink().commits.is_empty());
        assert!(matches!(
            coordinator.retry(),
            StepOutcome::Committed { generation: 1 }
        ));
        durable(&mut coordinator);
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 2 }
        ));
        durable(&mut coordinator);
        assert_eq!(
            coordinator
                .sink()
                .commits
                .iter()
                .map(|batch| batch.generation)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(coordinator.sink().attempts, vec![1, 1, 2]);
    }

    #[test]
    fn applying_sink_leaves_state_unchanged_when_the_atomic_group_fails() {
        let (_, occurrence_id, definition, period) = atomic_automation_case();
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = applying_coordinator(4);
        coordinator.sink_mut().fail_once.push(1);
        coordinator
            .enqueue(stage(
                &mut journal,
                vec![
                    EntityChange::Upsert(EntityPayload::Automation(definition)),
                    EntityChange::Upsert(EntityPayload::Occurrence(period)),
                ],
            ))
            .unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Blocked { generation: 1, .. }
        ));
        assert!(coordinator.sink().state.is_empty());
        assert!(matches!(
            coordinator.retry(),
            StepOutcome::Committed { generation: 1 }
        ));
        durable(&mut coordinator);
        assert_eq!(coordinator.sink().state.len(), 2);
        assert!(coordinator
            .sink()
            .state
            .contains_key(&EntityKey::Occurrence(occurrence_id)));
    }

    #[test]
    fn stale_duplicate_and_out_of_order_acks_release_nothing() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(4);
        coordinator
            .enqueue(
                journal
                    .stage_with_effects(vec![task_upsert(1, "one")], vec![41, 42])
                    .unwrap(),
            )
            .unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 1 }
        ));
        assert_eq!(
            coordinator.ack(Ack {
                workspace: WORKSPACE,
                generation: 2
            }),
            AckOutcome::OutOfOrder {
                generation: 2,
                awaiting: Some(1)
            }
        );
        assert_eq!(coordinator.durable_generation(), 0);
        assert_eq!(
            coordinator.ack(Ack {
                workspace: WORKSPACE,
                generation: 0
            }),
            AckOutcome::Duplicate { generation: 0 }
        );
        let receipt = durable(&mut coordinator);
        assert_eq!(receipt.effects, vec![41, 42]);
        assert_eq!(
            coordinator.ack(Ack {
                workspace: WORKSPACE,
                generation: 1
            }),
            AckOutcome::Duplicate { generation: 1 }
        );
        assert_eq!(coordinator.durable_generation(), 1);
        assert_eq!(coordinator.sink().commits.len(), 1);
    }

    #[test]
    fn wrong_workspace_intents_and_receipts_are_rejected() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = stage(&mut journal, vec![task_upsert(1, "one")]);
        let mut other = CommitCoordinator::new(RecordingSink::default(), 8, 4, BYTES);
        let rejection = other.enqueue(group.clone()).unwrap_err();
        assert_eq!(
            rejection.reason,
            EnqueueError::WrongWorkspace {
                expected: 8,
                found: WORKSPACE
            }
        );
        assert_eq!(rejection.into_group(), group);

        let mut coordinator = coordinator(4);
        coordinator.enqueue(group).unwrap();
        coordinator.step();
        assert_eq!(
            coordinator.ack(Ack {
                workspace: 8,
                generation: 1
            }),
            AckOutcome::WrongIncarnation {
                expected: WORKSPACE,
                found: 8
            }
        );
        assert_eq!(coordinator.durable_generation(), 0);
    }

    #[test]
    fn retention_bound_counts_awaiting_and_blocked_groups() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(2);
        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(1, "one")]))
            .unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 1 }
        ));
        assert_eq!(coordinator.retained_groups(), 1);
        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(2, "two")]))
            .unwrap();
        assert_eq!(coordinator.retained_groups(), 2);
        let third = stage(&mut journal, vec![task_upsert(3, "three")]);
        let rejection = coordinator.enqueue(third.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, third);
        durable(&mut coordinator);
        assert_eq!(coordinator.retained_groups(), 1);
        coordinator.enqueue(third).unwrap();
    }

    #[test]
    fn oversized_group_is_rejected_whole_before_acceptance() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 1_024);
        let mut large = task(1, "large");
        large.description = "x".repeat(4_096);
        let group = stage(
            &mut journal,
            vec![EntityChange::Upsert(EntityPayload::Task(large.clone()))],
        );
        assert!(group.estimated_bytes() > 1_024);
        let rejection = coordinator.enqueue(group.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, group);
        assert_eq!(coordinator.pending_generation(), 0);
        assert!(coordinator.sink().commits.is_empty());

        let atomic = {
            let mut journal = ChangeJournal::new(WORKSPACE);
            let (_, _, definition, period) = atomic_automation_case();
            journal.stage(vec![
                EntityChange::Upsert(EntityPayload::Automation(definition)),
                EntityChange::Upsert(EntityPayload::Occurrence(period)),
            ])
        }
        .unwrap();
        let mut budget =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, atomic.estimated_bytes());
        // The atomic group is accepted as one unit only when the budget fits it.
        budget.enqueue(atomic.clone()).unwrap();
        assert!(matches!(
            budget.step(),
            StepOutcome::Committed { generation: 1 }
        ));
        assert_eq!(budget.sink().commits[0].keys.len(), 2);
        assert_eq!(atomic.changes().len(), 2);
    }

    #[test]
    fn bounded_overflow_returns_the_retained_intent_without_dropping_it() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(1);
        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(1, "one")]))
            .unwrap();
        let retained = stage(&mut journal, vec![task_upsert(2, "two")]);
        let rejection = coordinator.enqueue(retained.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, retained);
        assert_eq!(rejection.group.generation(), 2);
        assert_eq!(rejection.group.changes().len(), 1);
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 1 }
        ));
        durable(&mut coordinator);
        coordinator.enqueue(retained).unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 2 }
        ));
        durable(&mut coordinator);
        assert_eq!(
            coordinator
                .sink()
                .commits
                .iter()
                .map(|batch| batch.generation)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn a_new_in_memory_update_after_ack_is_not_replaced_by_the_receipt() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(4);
        let mut in_memory: HashMap<EntityKey, EntityPayload> = HashMap::new();

        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(1, "first")]))
            .unwrap();
        coordinator.step();
        let receipt = durable(&mut coordinator);
        assert_eq!(receipt.generation, 1);
        in_memory.insert(
            EntityKey::Task(1),
            EntityPayload::Task(task(1, "newer in-memory")),
        );

        assert_eq!(
            coordinator.ack(Ack {
                workspace: WORKSPACE,
                generation: 1
            }),
            AckOutcome::Duplicate { generation: 1 }
        );
        match in_memory.get(&EntityKey::Task(1)) {
            Some(EntityPayload::Task(task)) => assert_eq!(task.title, "newer in-memory"),
            other => panic!("in-memory update was replaced: {other:?}"),
        }

        coordinator
            .enqueue(stage(&mut journal, vec![task_upsert(1, "second")]))
            .unwrap();
        coordinator.step();
        durable(&mut coordinator);
        assert_eq!(coordinator.durable_generation(), 2);
    }

    #[test]
    fn many_empty_strings_and_spare_string_capacity_are_accounted() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut many = task(1, "many empty labels");
        many.labels = vec![String::new(); 100_000];
        let label_slots = 100_000usize.saturating_mul(size_of::<String>());
        let payload = EntityPayload::Task(many.clone());
        assert!(
            payload.estimated_bytes() >= label_slots,
            "empty String slots must be accounted: {} < {label_slots}",
            payload.estimated_bytes()
        );

        let mut spare = task(2, "spare capacity");
        spare.description = String::with_capacity(1_000_000);
        let group = stage(
            &mut journal,
            vec![EntityChange::Upsert(EntityPayload::Task(spare))],
        );
        assert!(
            group.estimated_bytes() >= 1_000_000,
            "spare String capacity must be accounted: {}",
            group.estimated_bytes()
        );

        let mut coordinator =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 200_000);
        let mut coordinator_journal = ChangeJournal::new(WORKSPACE);
        let many_group = stage(
            &mut coordinator_journal,
            vec![EntityChange::Upsert(EntityPayload::Task(many))],
        );
        let rejection = coordinator.enqueue(many_group.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, many_group);
        assert_eq!(rejection.group.changes().len(), 1);
    }

    #[test]
    fn nested_template_and_metadata_spare_capacity_are_accounted() {
        let (_, _, mut definition, _) = atomic_automation_case();
        let mut goals = Vec::with_capacity(10_000);
        goals.push(definition.template.as_ref().unwrap().goals[0].clone());
        definition.template.as_mut().unwrap().goals = goals;
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = stage(
            &mut journal,
            vec![EntityChange::Upsert(EntityPayload::Automation(definition))],
        );
        let goal_slots = 10_000usize.saturating_mul(size_of::<crate::automation::AutomationGoal>());
        assert!(
            group.estimated_bytes() >= goal_slots,
            "template goal capacity must be accounted: {} < {goal_slots}",
            group.estimated_bytes()
        );

        let mut topics = Vec::with_capacity(10_000);
        let mut large_name = String::with_capacity(1_000_000);
        large_name.push_str("large first topic");
        topics.push(Topic {
            id: 1,
            project_id: 1,
            name: large_name,
            archived: false,
            automation_id: None,
            replanned_from: None,
        });
        topics.push(Topic {
            id: 2,
            project_id: 1,
            name: "small last".into(),
            archived: false,
            automation_id: None,
            replanned_from: None,
        });
        let mut metadata_journal = ChangeJournal::new(WORKSPACE);
        let metadata_group = metadata_journal
            .stage_with_metadata(
                Vec::new(),
                vec![MetadataDelta::Topics(topics)],
                Vec::new(),
            )
            .unwrap();
        let topic_slots = 10_000usize.saturating_mul(size_of::<Topic>());
        assert!(
            metadata_group.estimated_bytes() >= topic_slots.saturating_add(1_000_000),
            "metadata topic capacity and the first large item must both be accounted: {} < {}",
            metadata_group.estimated_bytes(),
            topic_slots.saturating_add(1_000_000)
        );

        let mut profiles = Vec::with_capacity(4);
        profiles.push(ModelProfile {
            name: "first large profile".into(),
            default_agent: "x".repeat(400_000),
            default_model: "m".into(),
            agents: Default::default(),
            models: Default::default(),
        });
        profiles.push(ModelProfile {
            name: "last small profile".into(),
            default_agent: "small".into(),
            default_model: "m".into(),
            agents: Default::default(),
            models: Default::default(),
        });
        let mut profile_journal = ChangeJournal::new(WORKSPACE);
        let profile_group = profile_journal
            .stage_with_metadata(
                Vec::new(),
                vec![MetadataDelta::ModelProfiles(profiles)],
                Vec::new(),
            )
            .unwrap();
        assert!(
            profile_group.estimated_bytes() >= 400_000,
            "the first large profile must not be replaced by the last small one: {}",
            profile_group.estimated_bytes()
        );

        let mut requests = Vec::with_capacity(4);
        requests.push(ResearchRequest {
            id: 1,
            project_id: 1,
            topic_id: None,
            automation_id: None,
            expected_result: None,
            revision: 1,
            provisional_topic_name: None,
            input: "r".repeat(300_000),
            submitted_at: "2026-11-08T00:00:00Z".into(),
            planning_run_id: None,
            goal_ids: Vec::new(),
            brief: None,
        });
        requests.push(ResearchRequest {
            id: 2,
            project_id: 1,
            topic_id: None,
            automation_id: None,
            expected_result: None,
            revision: 1,
            provisional_topic_name: None,
            input: "small last request".into(),
            submitted_at: "2026-11-08T00:00:00Z".into(),
            planning_run_id: None,
            goal_ids: Vec::new(),
            brief: None,
        });
        let request_group = profile_journal
            .stage_with_metadata(
                Vec::new(),
                vec![MetadataDelta::ResearchRequests(requests)],
                Vec::new(),
            )
            .unwrap();
        assert!(
            request_group.estimated_bytes() >= 300_000,
            "the first large request must not be replaced: {}",
            request_group.estimated_bytes()
        );

        let mut publications = Vec::with_capacity(4);
        publications.push(Publication {
            id: "p1".into(),
            topic: 1,
            fingerprint: "f".into(),
            config: PublishingConfig::default(),
            title: "First".into(),
            markdown: "m".repeat(300_000),
            status: "Online".into(),
            commit: None,
            message: String::new(),
        });
        publications.push(Publication {
            id: "p2".into(),
            topic: 1,
            fingerprint: "f".into(),
            config: PublishingConfig::default(),
            title: "Last".into(),
            markdown: "small".into(),
            status: "Online".into(),
            commit: None,
            message: String::new(),
        });
        let publication_group = profile_journal
            .stage_with_metadata(
                Vec::new(),
                vec![MetadataDelta::Publications(publications)],
                Vec::new(),
            )
            .unwrap();
        assert!(
            publication_group.estimated_bytes() >= 300_000,
            "the first large publication must not be replaced: {}",
            publication_group.estimated_bytes()
        );

        // Independent overflow case: fresh journal + fresh coordinator, so the
        // rejection can only be the byte budget, not a generation gap.
        let mut overflow_journal = ChangeJournal::new(WORKSPACE);
        let overflow_group = overflow_journal
            .stage_with_metadata(
                Vec::new(),
                vec![MetadataDelta::DefaultModel("x".repeat(2_000))],
                Vec::new(),
            )
            .unwrap();
        let mut coordinator =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 1_000);
        let rejection = coordinator.enqueue(overflow_group.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group.metadata().len(), 1);
        assert_eq!(rejection.group, overflow_group);
    }

    #[test]
    fn retention_bytes_count_awaiting_blocked_and_reject_atomically() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 4_000);
        let large_task = |id: u64, title: &str| {
            let mut task = task(id, title);
            task.description = "x".repeat(2_000);
            EntityChange::Upsert(EntityPayload::Task(task))
        };
        coordinator
            .enqueue(stage(&mut journal, vec![large_task(1, "one")]))
            .unwrap();
        assert!(matches!(
            coordinator.step(),
            StepOutcome::Committed { generation: 1 }
        ));
        coordinator
            .enqueue(stage(&mut journal, vec![large_task(2, "two")]))
            .unwrap();
        assert_eq!(coordinator.retained_groups(), 2);
        assert!(coordinator.retained_bytes() > 2_000);
        assert!(coordinator.retained_bytes() <= 4_000);
        let third = stage(&mut journal, vec![large_task(3, "three")]);
        let rejection = coordinator.enqueue(third.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, third);
        durable(&mut coordinator);

        let mut blocked_journal = ChangeJournal::new(WORKSPACE);
        let mut blocked = CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 4_000);
        blocked.sink_mut().fail_once.push(1);
        blocked
            .enqueue(stage(
                &mut blocked_journal,
                vec![large_task(4, "blocked")],
            ))
            .unwrap();
        assert!(matches!(
            blocked.step(),
            StepOutcome::Blocked { generation: 1, .. }
        ));
        assert_eq!(blocked.retained_groups(), 1);
        assert!(blocked.retained_bytes() > 2_000);
        assert!(blocked.retained_bytes() <= 4_000);
        let after_block = stage(&mut blocked_journal, vec![large_task(5, "after")]);
        let rejection = blocked.enqueue(after_block.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, after_block);
        assert!(matches!(
            blocked.retry(),
            StepOutcome::Committed { generation: 1 }
        ));
        durable(&mut blocked);
        blocked.enqueue(after_block).unwrap();
    }

    #[test]
    fn payload_budget_exact_boundary_growth_and_drained_queue() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let first = stage(&mut journal, vec![task_upsert(1, "one")]);
        let budget = first.estimated_bytes();
        let mut coordinator =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 64, budget);
        coordinator.enqueue(first.clone()).unwrap();
        assert_eq!(coordinator.retained_bytes(), budget);
        let second = stage(&mut journal, vec![task_upsert(2, "two")]);
        let rejection = coordinator.enqueue(second.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group, second);
        coordinator.step();
        durable(&mut coordinator);
        assert_eq!(coordinator.retained_bytes(), 0);
        assert_eq!(coordinator.retained_groups(), 0);

        // Growth from a nonzero queue capacity: the payload bound is independent
        // of the deque's allocation growth and still rejects the next item.
        let mut growth_journal = ChangeJournal::new(WORKSPACE);
        let groups: Vec<_> = (0..16)
            .map(|id| stage(&mut growth_journal, vec![task_upsert(id + 10, "group")]))
            .collect();
        let growth_budget: usize = groups
            .iter()
            .fold(0usize, |total, group| sat(total, group.estimated_bytes()));
        let mut growth =
            CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 64, growth_budget);
        for group in groups {
            growth.enqueue(group).unwrap();
            assert!(growth.retained_bytes() <= growth_budget);
        }
        assert_eq!(growth.retained_bytes(), growth_budget);
        let extra = stage(&mut growth_journal, vec![task_upsert(99, "extra")]);
        assert_eq!(
            growth.enqueue(extra.clone()).unwrap_err().reason,
            EnqueueError::Overflow
        );
        while growth.retained_groups() > 0 {
            growth.step();
            durable(&mut growth);
        }
        assert_eq!(growth.retained_bytes(), 0);
    }

    #[test]
    fn atomic_overflow_rejection_preserves_both_payloads() {
        let (automation, occurrence_id, definition, period) = atomic_automation_case();
        let mut journal = ChangeJournal::new(WORKSPACE);
        let group = stage(
            &mut journal,
            vec![
                EntityChange::Upsert(EntityPayload::Automation(definition)),
                EntityChange::Upsert(EntityPayload::Occurrence(period)),
            ],
        );
        let mut coordinator = CommitCoordinator::new(RecordingSink::default(), WORKSPACE, 8, 64);
        let rejection = coordinator.enqueue(group.clone()).unwrap_err();
        assert_eq!(rejection.reason, EnqueueError::Overflow);
        assert_eq!(rejection.group.changes().len(), 2);
        assert_eq!(
            rejection.group.keys(),
            vec![
                EntityKey::Automation(automation),
                EntityKey::Occurrence(occurrence_id)
            ]
        );
        assert_eq!(rejection.group, group);
        assert_eq!(coordinator.pending_generation(), 0);
    }

    #[test]
    fn generation_and_base_validation_rejects_gaps_and_wrong_bases() {
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut coordinator = coordinator(4);
        let first = stage(&mut journal, vec![task_upsert(1, "one")]);
        let second = stage(&mut journal, vec![task_upsert(2, "two")]);
        assert_eq!((first.generation(), first.base()), (1, 0));
        assert_eq!((second.generation(), second.base()), (2, 1));
        let gap = coordinator.enqueue(second.clone()).unwrap_err();
        assert_eq!(
            gap.reason,
            EnqueueError::GenerationGap {
                expected: 1,
                found: 2
            }
        );
        assert_eq!(gap.group, second);
        // Inside this module a test can construct a malformed group to prove the
        // coordinator rejects a wrong base instead of applying it.
        let wrong_base = StagedGroup {
            workspace: WORKSPACE,
            generation: 2,
            base: 0,
            changes: vec![task_upsert(2, "two")],
            metadata: Vec::new(),
            effects: Vec::new(),
            estimated_bytes: 64,
        };
        coordinator.enqueue(first).unwrap();
        let rejection = coordinator.enqueue(wrong_base).unwrap_err();
        assert_eq!(
            rejection.reason,
            EnqueueError::BaseMismatch {
                expected: 1,
                found: 0
            }
        );
        coordinator.step();
        durable(&mut coordinator);
        coordinator.enqueue(second).unwrap();
        let third = stage(&mut journal, vec![task_upsert(3, "three")]);
        assert_eq!(coordinator.enqueue(third), Ok(()));
    }
}
