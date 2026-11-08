//! T14: single ordered SQLite persistence worker.
//!
//! The worker owns the only `SqliteV4` connection on its own thread. Submission,
//! polling and shutdown request are non-blocking UI surfaces. The accepted
//! journal/coordinator supply generation ordering, atomic groups, blocked
//! parking, exact-intent retry and durable receipts. Live `TaskApp` activation is
//! deliberately absent from this slice.

#![allow(dead_code)]

use crate::persistence_journal::{
    Ack, AckOutcome, CommitCoordinator, CommitSink, DurableReceipt, EnqueueError,
    EnqueueRejection, StagedGroup, StepOutcome,
};
use crate::dispatch_intent::AdmissionParts;
use crate::sqlite_v4::{Fault, SqliteV4};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerOptions {
    /// Total accepted groups retained across channel, in-flight, coordinator,
    /// blocked and rejected-event states.
    pub max_retained_groups: usize,
    /// Total accepted payload bytes across the same states.
    pub max_retained_bytes: usize,
    pub channel_capacity: usize,
    pub event_capacity: usize,
}

impl Default for WorkerOptions {
    fn default() -> Self {
        Self {
            max_retained_groups: 32,
            max_retained_bytes: 8 * 1024 * 1024,
            channel_capacity: 4,
            event_capacity: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SubmitOutcome {
    Accepted,
    /// Startup has not completed; the owned group is returned.
    NotReady(StagedGroup),
    /// A bounded group/byte/admission budget was full; the owned group is returned.
    Backpressure(StagedGroup),
    /// Shutdown or worker failure; the owned group is returned.
    Closed(StagedGroup),
}

#[derive(Debug)]
pub(crate) enum WorkerEvent {
    Started {
        workspace: u64,
        durable_generation: u64,
        intents: Vec<(String, Option<Arc<AdmissionParts>>)>,
    },
    Durable {
        receipt: DurableReceipt,
        reservation: Reservation,
    },
    Blocked {
        workspace: u64,
        generation: u64,
        error: String,
    },
    Holdings {
        generation: u64,
    },
    Rejected {
        reason: EnqueueError,
        group: StagedGroup,
        reservation: Reservation,
    },
    Returned {
        group: StagedGroup,
        reservation: Reservation,
    },
    IntentPersisted {
        key: String,
        result: Result<(), String>,
    },
    IntentRemoved {
        key: String,
        result: Result<(), String>,
    },
    Failed {
        message: String,
    },
    Shutdown {
        workspace: u64,
        durable_generation: u64,
        blocked: Option<u64>,
        returned: usize,
    },
}

impl WorkerEvent {
    pub(crate) fn is_stale(&self, workspace: u64, durable_generation: u64) -> bool {
        match self {
            Self::Durable { receipt, .. } => {
                receipt.workspace != workspace || receipt.generation <= durable_generation
            }
            Self::Started { workspace: event, .. }
            | Self::Blocked { workspace: event, .. }
            | Self::Shutdown { workspace: event, .. } => *event != workspace,
            Self::Holdings { .. }
            | Self::Rejected { .. }
            | Self::Returned { .. }
            | Self::IntentPersisted { .. }
            | Self::IntentRemoved { .. }
            | Self::Failed { .. } => false,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct IntentFile {
    items: Vec<IntentFileItem>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct IntentFileItem {
    key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parts: Option<AdmissionParts>,
}

#[derive(serde::Serialize)]
struct IntentFileRef<'a> {
    items: Vec<IntentFileItemRef<'a>>,
}

#[derive(serde::Serialize)]
struct IntentFileItemRef<'a> {
    key: &'a str,
    parts: Option<&'a AdmissionParts>,
}

struct StoredIntent {
    parts: Option<Arc<AdmissionParts>>,
    bytes: usize,
}

struct IntentStore {
    path: Option<PathBuf>,
    items: BTreeMap<String, StoredIntent>,
    bytes: usize,
    max_bytes: usize,
}

impl IntentStore {
    fn load(path: Option<PathBuf>, max_bytes: usize) -> Result<Self, String> {
        let Some(path) = path else {
            return Ok(Self {
                path: None,
                items: BTreeMap::new(),
                bytes: 0,
                max_bytes,
            });
        };
        let payload = match fs::read(&path) {
            Ok(payload) => payload,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path: Some(path),
                    items: BTreeMap::new(),
                    bytes: 0,
                    max_bytes,
                })
            }
            Err(error) => {
                return Err(format!(
                    "cannot read dispatch intent {}: {error}",
                    path.display()
                ))
            }
        };
        let file: IntentFile = serde_json::from_slice(&payload).map_err(|error| {
            format!(
                "cannot parse dispatch intent {}: {error}",
                path.display()
            )
        })?;
        let mut items = BTreeMap::new();
        let mut bytes = 0usize;
        for item in file.items {
            let size = serde_json::to_vec(&item)
                .map_err(|error| format!("cannot measure dispatch intent: {error}"))?
                .len();
            bytes = bytes.saturating_add(size);
            items.insert(item.key, StoredIntent {
                parts: item.parts.map(Arc::new),
                bytes: size,
            });
        }
        if bytes > max_bytes {
            return Err(format!(
                "dispatch intent budget exceeded at startup: {bytes} > {max_bytes} bytes; the file was left intact"
            ));
        }
        Ok(Self {
            path: Some(path),
            items,
            bytes,
            max_bytes,
        })
    }

    fn records(&self) -> Vec<(String, Option<Arc<AdmissionParts>>)> {
        self.items
            .iter()
            .map(|(key, stored)| (key.clone(), stored.parts.clone()))
            .collect()
    }

    fn insert_marker(&mut self, key: String) -> Result<(), String> {
        let size = serde_json::to_vec(&IntentFileItemRef {
            key: &key,
            parts: None,
        })
        .map_err(|error| format!("cannot encode dispatch marker: {error}"))?
        .len();
        let previous = self.items.get(&key).map_or(0, |stored| stored.bytes);
        let next = self.bytes.saturating_sub(previous).saturating_add(size);
        if next > self.max_bytes {
            return Err(format!(
                "dispatch intent byte budget exceeded: {next} > {} bytes",
                self.max_bytes
            ));
        }
        self.items.insert(key, StoredIntent { parts: None, bytes: size });
        self.bytes = next;
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = self.path.as_ref() else {
            return Err("durable intent storage is not configured".into());
        };
        if self.items.is_empty() {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "cannot remove dispatch intent {}: {error}",
                        path.display()
                    ))
                }
            }
            sync_parent(path)?;
            return Ok(());
        }
        write_intents_atomic(
            path,
            self.items
                .iter()
                .map(|(key, stored)| (key.as_str(), stored.parts.as_deref())),
        )
    }

    fn insert(&mut self, key: String, parts: Arc<AdmissionParts>) -> Result<(), String> {
        let size = serde_json::to_vec(&*parts)
            .map_err(|error| format!("cannot encode dispatch intent: {error}"))?
            .len();
        let previous = self.items.get(&key).map_or(0, |stored| stored.bytes);
        let next = self.bytes.saturating_sub(previous).saturating_add(size);
        if next > self.max_bytes {
            return Err(format!(
                "dispatch intent byte budget exceeded: {next} > {} bytes",
                self.max_bytes
            ));
        }
        self.items.insert(key, StoredIntent { parts: Some(parts), bytes: size });
        self.bytes = next;
        if let Err(error) = self.persist() {
            // Keep the in-memory obligation so a later retry can finish it.
            return Err(error);
        }
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), String> {
        let Some(previous) = self.items.get(key) else {
            return Ok(());
        };
        let bytes = previous.bytes;
        self.items.remove(key);
        self.bytes = self.bytes.saturating_sub(bytes);
        self.persist()
    }
}

fn sync_parent(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("cannot sync dispatch intent directory: {error}"))?;
    }
    Ok(())
}

fn write_intents_atomic<'a>(
    path: &Path,
    records: impl Iterator<Item = (&'a str, Option<&'a AdmissionParts>)>,
) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create dispatch intent directory: {error}"))?;
    }
    let payload = serde_json::to_vec(&IntentFileRef {
        items: records
            .map(|(key, parts)| IntentFileItemRef { key, parts })
            .collect(),
    })
    .map_err(|error| format!("cannot encode dispatch intent: {error}"))?;
    let temp = path.with_extension("json.tmp");
    let mut file =
        fs::File::create(&temp).map_err(|error| format!("cannot create dispatch intent: {error}"))?;
    use std::io::Write;
    file.write_all(&payload)
        .map_err(|error| format!("cannot write dispatch intent: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("cannot sync dispatch intent: {error}"))?;
    drop(file);
    fs::rename(&temp, path)
        .map_err(|error| format!("cannot install dispatch intent: {error}"))?;
    sync_parent(path)
}

#[derive(Debug, Default)]
struct Budget {
    groups: usize,
    bytes: usize,
    max_groups: usize,
    max_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct Reservation {
    budget: Arc<Mutex<Budget>>,
    bytes: usize,
    released: bool,
}

impl Reservation {
    fn try_new(budget: &Arc<Mutex<Budget>>, bytes: usize) -> Option<Self> {
        let budget_handle = budget.clone();
        let mut budget = budget.lock().unwrap_or_else(|error| error.into_inner());
        if bytes > budget.max_bytes
            || budget.groups.saturating_add(1) > budget.max_groups
            || budget.bytes.saturating_add(bytes) > budget.max_bytes
        {
            return None;
        }
        budget.groups += 1;
        budget.bytes = budget.bytes.saturating_add(bytes);
        Some(Self {
            budget: budget_handle,
            bytes,
            released: false,
        })
    }

    fn release(mut self) {
        self.release_inner();
    }

    /// Shrink the reservation after the entity payload has been committed and
    /// only the effect receipt remains retained.
    fn shrink_to(&mut self, bytes: usize) {
        if self.released || bytes >= self.bytes {
            return;
        }
        let mut budget = self.budget.lock().unwrap_or_else(|error| error.into_inner());
        budget.bytes = budget.bytes.saturating_sub(self.bytes - bytes);
        self.bytes = bytes;
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let mut budget = self.budget.lock().unwrap_or_else(|error| error.into_inner());
        budget.groups = budget.groups.saturating_sub(1);
        budget.bytes = budget.bytes.saturating_sub(self.bytes);
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.release_inner();
    }
}

struct SubmittedGroup {
    group: StagedGroup,
    reservation: Reservation,
}

enum WorkerRequest {
    Submit(SubmittedGroup),
    PersistIntent {
        key: String,
        parts: Option<Arc<AdmissionParts>>,
    },
    RemoveIntent(String),
    Retry,
    #[cfg(any(test, feature = "qa"))]
    SetDelay(Duration),
    #[cfg(any(test, feature = "qa"))]
    SetFault(Fault),
}

struct WorkerSink {
    database: SqliteV4,
    #[cfg(any(test, feature = "qa"))]
    fault: Option<Fault>,
}

impl CommitSink for WorkerSink {
    fn commit(&mut self, group: &StagedGroup) -> Result<(), String> {
        #[cfg(any(test, feature = "qa"))]
        if let Some(fault) = self.fault {
            return self
                .database
                .commit_group_with_fault(group, fault)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"));
        }
        self.database
            .commit_group(group)
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }
}

pub(crate) struct PersistenceWorker {
    workspace: u64,
    submissions: SyncSender<WorkerRequest>,
    events: Receiver<WorkerEvent>,
    join: Option<JoinHandle<()>>,
    budget: Arc<Mutex<Budget>>,
    hold: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    durable_generation: u64,
    started: bool,
    startup_error: Option<String>,
    last_error: Option<String>,
}

impl PersistenceWorker {
    /// Non-blocking factory: database opening and bootstrap happen on the worker
    /// thread. Inspect `poll()` for `Started` or `Failed`.
    pub(crate) fn start(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
    ) -> Result<Self, String> {
        Self::start_internal(path, root, workspace, options, Duration::ZERO, None, None)
    }

    /// Start with worker-owned atomic intent storage. The UI never reads or
    /// writes the intent file on this path.
    pub(crate) fn start_with_intents(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        intent_path: PathBuf,
    ) -> Result<Self, String> {
        Self::start_internal(
            path,
            root,
            workspace,
            options,
            Duration::ZERO,
            None,
            Some(intent_path),
        )
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn start_with_test_options(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        delay: Duration,
        fault: Option<Fault>,
    ) -> Result<Self, String> {
        Self::start_internal(path, root, workspace, options, delay, fault, None)
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn start_with_test_options_and_intents(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        delay: Duration,
        fault: Option<Fault>,
        intent_path: PathBuf,
    ) -> Result<Self, String> {
        Self::start_internal(
            path,
            root,
            workspace,
            options,
            delay,
            fault,
            Some(intent_path),
        )
    }

    fn start_internal(
        path: &Path,
        root: &Path,
        workspace: u64,
        options: WorkerOptions,
        delay: Duration,
        fault: Option<Fault>,
        intent_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let (submit_tx, submit_rx) = sync_channel(options.channel_capacity.max(1));
        let (event_tx, event_rx) = sync_channel(options.event_capacity.max(1));
        let budget = Arc::new(Mutex::new(Budget {
            max_groups: options.max_retained_groups.max(1),
            max_bytes: options.max_retained_bytes.max(1),
            ..Budget::default()
        }));
        let hold = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let path = path.to_path_buf();
        let root = root.to_path_buf();
        let worker_budget = budget.clone();
        let worker_hold = hold.clone();
        let worker_shutdown = shutdown.clone();
        let join = thread::Builder::new()
            .name("t14-persistence".into())
            .spawn(move || {
                worker_main(
                    path,
                    root,
                    workspace,
                    options,
                    delay,
                    fault,
                    intent_path,
                    submit_rx,
                    event_tx,
                    worker_budget,
                    worker_hold,
                    worker_shutdown,
                )
            })
            .map_err(|error| format!("cannot spawn persistence worker: {error}"))?;
        Ok(Self {
            workspace,
            submissions: submit_tx,
            events: event_rx,
            join: Some(join),
            budget,
            hold,
            shutdown,
            durable_generation: 0,
            started: false,
            startup_error: None,
            last_error: None,
        })
    }

    pub(crate) fn workspace(&self) -> u64 {
        self.workspace
    }

    pub(crate) fn is_started(&self) -> bool {
        self.started
    }

    pub(crate) fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub(crate) fn durable_generation(&self) -> u64 {
        self.durable_generation
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Non-blocking. A group is accepted only after a shared count/byte
    /// reservation succeeds; otherwise ownership is returned immediately.
    pub(crate) fn submit(&self, group: StagedGroup) -> SubmitOutcome {
        if group.workspace() != self.workspace {
            return SubmitOutcome::Backpressure(group);
        }
        if self.shutdown.load(Ordering::Acquire) {
            return SubmitOutcome::Closed(group);
        }
        if !self.started {
            return SubmitOutcome::NotReady(group);
        }
        let Some(reservation) = Reservation::try_new(&self.budget, group.estimated_bytes()) else {
            return SubmitOutcome::Backpressure(group);
        };
        match self
            .submissions
            .try_send(WorkerRequest::Submit(SubmittedGroup { group, reservation }))
        {
            Ok(()) => SubmitOutcome::Accepted,
            Err(TrySendError::Full(request)) => match request {
                WorkerRequest::Submit(submitted) => SubmitOutcome::Backpressure(submitted.group),
                _ => unreachable!("only submit requests are sent here"),
            },
            Err(TrySendError::Disconnected(request)) => match request {
                WorkerRequest::Submit(submitted) => SubmitOutcome::Closed(submitted.group),
                _ => unreachable!("only submit requests are sent here"),
            },
        }
    }

    pub(crate) fn retry(&self) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::Retry)
            .map_err(|error| format!("persistence worker is unavailable: {error}"))
    }

    /// Non-blocking request to persist one dispatch intent on the worker thread.
    pub(crate) fn persist_intent(
        &self,
        key: String,
        parts: Arc<AdmissionParts>,
    ) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::PersistIntent {
                key,
                parts: Some(parts),
            })
            .map_err(|error| format!("dispatch intent persistence is unavailable: {error}"))
    }

    /// Persist a small marker (launch claim, resolution decision) without
    /// cloning or serializing the run payload on the UI thread.
    pub(crate) fn persist_marker(&self, key: String) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::PersistIntent { key, parts: None })
            .map_err(|error| format!("dispatch marker persistence is unavailable: {error}"))
    }

    /// Non-blocking request to remove one acknowledged intent.
    pub(crate) fn remove_intent(&self, key: String) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::RemoveIntent(key))
            .map_err(|error| format!("dispatch intent removal is unavailable: {error}"))
    }

    /// Bounded per-poll work so a continuously refilled producer cannot starve
    /// the UI frame.
    pub(crate) fn poll(&mut self) -> Vec<WorkerEvent> {
        self.poll_bounded(32)
    }

    fn poll_bounded(&mut self, limit: usize) -> Vec<WorkerEvent> {
        let mut events = Vec::new();
        for _ in 0..limit {
            match self.events.try_recv() {
                Ok(event) => {
                    match &event {
                        WorkerEvent::Started {
                            durable_generation, ..
                        } => {
                            self.started = true;
                            self.durable_generation = *durable_generation;
                        }
                        WorkerEvent::Shutdown {
                            durable_generation, ..
                        } => self.durable_generation = *durable_generation,
                        WorkerEvent::Durable { receipt, .. } => {
                            if receipt.workspace == self.workspace
                                && receipt.generation > self.durable_generation
                            {
                                self.durable_generation = receipt.generation;
                            }
                        }
                        WorkerEvent::Failed { message } => {
                            self.startup_error = Some(message.clone());
                            self.last_error = Some(message.clone());
                        }
                        WorkerEvent::Rejected { .. } | WorkerEvent::Returned { .. } => {}
                        WorkerEvent::Blocked { .. }
                        | WorkerEvent::Holdings { .. }
                        | WorkerEvent::IntentPersisted { .. }
                        | WorkerEvent::IntentRemoved { .. } => {}
                    }
                    events.push(event);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.last_error = Some("persistence worker disconnected".into());
                    break;
                }
            }
        }
        events
    }

    /// Non-blocking shutdown request. No event drain, sleep, spin or join.
    pub(crate) fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    /// True once the worker thread has exited. Only then may `join` be called.
    pub(crate) fn is_finished(&self) -> bool {
        self.join.as_ref().is_some_and(JoinHandle::is_finished)
    }

    /// Join only a finished worker. Returns false without waiting otherwise.
    pub(crate) fn join(&mut self) -> bool {
        if !self.is_finished() {
            return false;
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        true
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn set_delay(&self, delay: Duration) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::SetDelay(delay))
            .map_err(|error| format!("cannot set worker delay: {error}"))
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn set_fault(&self, fault: Fault) -> Result<(), String> {
        self.submissions
            .try_send(WorkerRequest::SetFault(fault))
            .map_err(|error| format!("cannot set worker fault: {error}"))
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn set_hold(&self, hold: bool) {
        self.hold.store(hold, Ordering::Release);
    }
}

impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        self.request_shutdown();
        // Never wait on the caller/UI. Join only an already-finished thread;
        // otherwise detach it; it observes the atomic flag at its next step.
        let _ = self.join();
    }
}

fn worker_main(
    path: PathBuf,
    root: PathBuf,
    workspace: u64,
    options: WorkerOptions,
    mut delay: Duration,
    mut fault: Option<Fault>,
    intent_path: Option<PathBuf>,
    submissions: Receiver<WorkerRequest>,
    events: SyncSender<WorkerEvent>,
    budget: Arc<Mutex<Budget>>,
    hold: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    let database = match SqliteV4::open(&path, &root, workspace) {
        Ok(database) => database,
        Err(message) => {
            let _ = events.send(WorkerEvent::Failed { message });
            return;
        }
    };
    let durable = match database.durable_generation() {
        Ok(value) => value,
        Err(message) => {
            let _ = events.send(WorkerEvent::Failed { message });
            return;
        }
    };
    let mut intents = match IntentStore::load(intent_path, options.max_retained_bytes.max(1)) {
        Ok(intents) => intents,
        Err(message) => {
            let _ = events.send(WorkerEvent::Failed { message });
            return;
        }
    };
    let mut coordinator = CommitCoordinator::with_durable_generation(
        WorkerSink {
            database,
            #[cfg(any(test, feature = "qa"))]
            fault,
        },
        workspace,
        options.max_retained_groups,
        options.max_retained_bytes,
        durable,
    );
    let mut reservations: HashMap<u64, Reservation> = HashMap::new();
    if events
        .send(WorkerEvent::Started {
            workspace,
            durable_generation: durable,
            intents: intents.records(),
        })
        .is_err()
    {
        return;
    }
    let mut last_blocked = None;
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match submissions.recv_timeout(Duration::from_millis(20)) {
            Ok(WorkerRequest::Submit(mut submitted)) => {
                if !delay.is_zero() {
                    thread::sleep(delay);
                }
                if hold.load(Ordering::Acquire) {
                    let generation = submitted.group.generation();
                    if events
                        .send(WorkerEvent::Holdings { generation })
                        .is_err()
                    {
                        return;
                    }
                    while hold.load(Ordering::Acquire) && !shutdown.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(2));
                    }
                    if shutdown.load(Ordering::Acquire) {
                        let _ = events.send(WorkerEvent::Returned {
                            group: submitted.group,
                            reservation: submitted.reservation,
                        });
                        break;
                    }
                }
                match coordinator.enqueue(submitted.group) {
                    Ok(()) => {
                        // The reservation stays in the map until durable or returned.
                        let generation = coordinator.pending_generation();
                        reservations.insert(generation, submitted.reservation);
                    }
                    Err(EnqueueRejection { reason, group }) => {
                        if events
                            .send(WorkerEvent::Rejected {
                                reason,
                                group,
                                reservation: submitted.reservation,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            Ok(WorkerRequest::PersistIntent { key, parts }) => {
                let result = match parts {
                    Some(parts) => intents.insert(key.clone(), parts),
                    None => intents.insert_marker(key.clone()),
                };
                let _ = events.send(WorkerEvent::IntentPersisted { key, result });
            }
            Ok(WorkerRequest::RemoveIntent(key)) => {
                let result = intents.remove(&key);
                let _ = events.send(WorkerEvent::IntentRemoved { key, result });
            }
            Ok(WorkerRequest::Retry) => {
                let _ = coordinator.retry();
            }
            #[cfg(any(test, feature = "qa"))]
            Ok(WorkerRequest::SetDelay(value)) => delay = value,
            #[cfg(any(test, feature = "qa"))]
            Ok(WorkerRequest::SetFault(value)) => {
                fault = if value == Fault::None { None } else { Some(value) };
                coordinator.sink_mut().fault = fault;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        drain(
            &mut coordinator,
            workspace,
            &events,
            &mut last_blocked,
            &mut reservations,
        );
    }
    drain(
        &mut coordinator,
        workspace,
        &events,
        &mut last_blocked,
        &mut reservations,
    );
    let blocked = coordinator.blocked().map(|blocked| blocked.group.generation());
    let mut returned = 0usize;
    while let Ok(request) = submissions.try_recv() {
        match request {
            WorkerRequest::Submit(SubmittedGroup { group, reservation }) => {
                returned += 1;
                if events
                    .send(WorkerEvent::Returned { group, reservation })
                    .is_err()
                {
                    return;
                }
            }
            WorkerRequest::PersistIntent { key, parts } => {
                let result = match parts {
                    Some(parts) => intents.insert(key.clone(), parts),
                    None => intents.insert_marker(key.clone()),
                };
                let _ = events.send(WorkerEvent::IntentPersisted { key, result });
            }
            WorkerRequest::RemoveIntent(key) => {
                let result = intents.remove(&key);
                let _ = events.send(WorkerEvent::IntentRemoved { key, result });
            }
            WorkerRequest::Retry => {}
            #[cfg(any(test, feature = "qa"))]
            WorkerRequest::SetDelay(_) | WorkerRequest::SetFault(_) => {}
        }
    }
    for group in coordinator.take_uncommitted() {
        let reservation = reservations
            .remove(&group.generation())
            .unwrap_or_else(|| Reservation {
                budget: budget.clone(),
                bytes: 0,
                released: true,
            });
        returned += 1;
        if events
            .send(WorkerEvent::Returned { group, reservation })
            .is_err()
        {
            return;
        }
    }
    let _ = events.send(WorkerEvent::Shutdown {
        workspace,
        durable_generation: coordinator.durable_generation(),
        blocked,
        returned,
    });
}

fn drain(
    coordinator: &mut CommitCoordinator<WorkerSink>,
    workspace: u64,
    events: &SyncSender<WorkerEvent>,
    last_blocked: &mut Option<u64>,
    reservations: &mut HashMap<u64, Reservation>,
) {
    loop {
        match coordinator.step() {
            StepOutcome::Committed { generation } => {
                *last_blocked = None;
                let ack = coordinator.ack(Ack {
                    workspace,
                    generation,
                });
                let reservation = reservations.remove(&generation);
                match ack {
                    AckOutcome::Durable(receipt) => {
                        let mut reservation = reservation.unwrap_or_else(|| Reservation {
                            budget: Arc::new(Mutex::new(Budget::default())),
                            bytes: 0,
                            released: true,
                        });
                        reservation.shrink_to(receipt.effects.capacity() * std::mem::size_of::<u64>());
                        if events
                            .send(WorkerEvent::Durable {
                                receipt,
                                reservation,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    other => {
                        let _ = events.send(WorkerEvent::Failed {
                            message: format!("worker acknowledgement failed: {other:?}"),
                        });
                        return;
                    }
                }
            }
            StepOutcome::AwaitingAck { generation } => {
                let ack = coordinator.ack(Ack {
                    workspace,
                    generation,
                });
                let reservation = reservations.remove(&generation);
                match ack {
                    AckOutcome::Durable(receipt) => {
                        let mut reservation = reservation.unwrap_or_else(|| Reservation {
                            budget: Arc::new(Mutex::new(Budget::default())),
                            bytes: 0,
                            released: true,
                        });
                        reservation.shrink_to(receipt.effects.capacity() * std::mem::size_of::<u64>());
                        if events
                            .send(WorkerEvent::Durable {
                                receipt,
                                reservation,
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    other => {
                        let _ = events.send(WorkerEvent::Failed {
                            message: format!("worker acknowledgement failed: {other:?}"),
                        });
                        return;
                    }
                }
            }
            StepOutcome::Blocked { generation, error } => {
                if *last_blocked != Some(generation) {
                    *last_blocked = Some(generation);
                    let _ = events.send(WorkerEvent::Blocked {
                        workspace,
                        generation,
                        error,
                    });
                }
                return;
            }
            StepOutcome::Idle => return,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Store;
    use crate::persistence_journal::{ChangeJournal, EntityChange, EntityPayload};
    use std::fs;

    const WORKSPACE: u64 = 41;
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "t14-worker3-{name}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn seeded(name: &str) -> (PathBuf, PathBuf, Store) {
        let root = temp_root(name);
        let store = Store::mock();
        let path = root.join("workspace.sqlite3");
        SqliteV4::create_seeded(&path, &root, WORKSPACE, &store).unwrap();
        (root, path, store)
    }

    fn group(
        journal: &mut ChangeJournal,
        store: &Store,
        id: u64,
        title: &str,
        effects: Vec<u64>,
    ) -> StagedGroup {
        let mut task = store.task(id).unwrap().clone();
        task.title = title.into();
        journal
            .stage_with_effects(
                vec![EntityChange::Upsert(EntityPayload::Task(task))],
                effects,
            )
            .unwrap()
    }

    fn wait_started(worker: &mut PersistenceWorker) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let _ = worker.poll();
            if worker.is_started() {
                return;
            }
            if let Some(error) = worker.startup_error() {
                panic!("worker startup failed: {error}");
            }
            assert!(Instant::now() < deadline, "worker did not start");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for<F>(worker: &mut PersistenceWorker, mut predicate: F) -> Vec<WorkerEvent>
    where
        F: FnMut(&[WorkerEvent]) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut events = Vec::new();
        loop {
            events.extend(worker.poll());
            if predicate(&events) {
                return events;
            }
            assert!(Instant::now() < deadline, "worker event timeout");
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Test-only caller loop. The production API has no wait; this helper may
    /// sleep because it represents the caller's own polling policy.
    fn collect_until_shutdown(
        worker: &mut PersistenceWorker,
        timeout: Duration,
    ) -> (Vec<WorkerEvent>, bool) {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        loop {
            events.extend(worker.poll());
            let shutdown = events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Shutdown { .. }));
            if shutdown && worker.is_finished() {
                return (events, worker.join());
            }
            if Instant::now() > deadline {
                return (events, worker.join());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn startup_and_submit_are_nonblocking_during_delayed_commit() {
        let (root, path, store) = seeded("delay");
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            Duration::from_millis(250),
            None,
        )
        .unwrap();
        assert!(matches!(
            worker.submit(group(
                &mut ChangeJournal::new(WORKSPACE),
                &store,
                1,
                "early",
                vec![1]
            )),
            SubmitOutcome::NotReady(_)
        ));
        wait_started(&mut worker);
        let started = Instant::now();
        let outcome = worker.submit(group(
            &mut ChangeJournal::new(WORKSPACE),
            &store,
            1,
            "delayed",
            vec![1],
        ));
        let elapsed = started.elapsed();
        assert_eq!(outcome, SubmitOutcome::Accepted);
        assert!(elapsed < Duration::from_millis(50), "submit blocked {elapsed:?}");
        eprintln!("worker_submit_us={}", elapsed.as_micros());
        let commit_started = Instant::now();
        wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Durable { .. }))
        });
        eprintln!("worker_commit_including_delay_us={}", commit_started.elapsed().as_micros());
        assert_eq!(worker.durable_generation(), 1);
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
    }

    #[test]
    fn hold_barrier_makes_channel_backpressure_deterministic() {
        let (root, path, store) = seeded("backpressure");
        let options = WorkerOptions {
            channel_capacity: 1,
            ..WorkerOptions::default()
        };
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            options,
            Duration::ZERO,
            None,
        )
        .unwrap();
        wait_started(&mut worker);
        worker.set_hold(true);
        let mut journal = ChangeJournal::new(WORKSPACE);
        assert_eq!(
            worker.submit(group(&mut journal, &store, 1, "one", vec![1])),
            SubmitOutcome::Accepted
        );
        wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Holdings { .. }))
        });
        assert_eq!(
            worker.submit(group(&mut journal, &store, 2, "two", vec![2])),
            SubmitOutcome::Accepted
        );
        let retained = match worker.submit(group(&mut journal, &store, 3, "three", vec![3])) {
            SubmitOutcome::Backpressure(group) => group,
            other => panic!("expected backpressure, got {other:?}"),
        };
        worker.set_hold(false);
        wait_for(&mut worker, |events| {
            events.iter().filter(|event| matches!(event, WorkerEvent::Durable { .. })).count() >= 2
        });
        assert_eq!(worker.submit(retained), SubmitOutcome::Accepted);
        wait_for(&mut worker, |events| {
            events.iter().any(|event| matches!(event, WorkerEvent::Durable { receipt, .. } if receipt.generation == 3))
        });
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
    }

    #[test]
    fn failed_first_commit_parks_successor_and_retry_releases_once() {
        let (root, path, store) = seeded("park");
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            Duration::ZERO,
            Some(Fault::BeforeCommit),
        )
        .unwrap();
        wait_started(&mut worker);
        let mut journal = ChangeJournal::new(WORKSPACE);
        assert_eq!(
            worker.submit(group(&mut journal, &store, 1, "one", vec![11])),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            worker.submit(group(&mut journal, &store, 2, "two", vec![22])),
            SubmitOutcome::Accepted
        );
        wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Blocked { generation: 1, .. }))
        });
        worker.set_fault(Fault::None).unwrap();
        worker.retry().unwrap();
        let events = wait_for(&mut worker, |events| {
            events.iter().filter(|event| matches!(event, WorkerEvent::Durable { .. })).count() >= 2
        });
        let receipts: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                WorkerEvent::Durable { receipt, .. } => Some(receipt.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(receipts[0].generation, 1);
        assert_eq!(receipts[0].effects, vec![11]);
        assert_eq!(receipts[1].generation, 2);
        assert_eq!(receipts[1].effects, vec![22]);
        worker.request_shutdown();
        let (_, joined) = collect_until_shutdown(&mut worker, Duration::from_secs(5));
        assert!(joined);
    }

    #[test]
    fn uncertain_commit_retry_returns_one_exact_receipt() {
        let (root, path, store) = seeded("uncertain");
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            WorkerOptions::default(),
            Duration::ZERO,
            Some(Fault::AfterCommit),
        )
        .unwrap();
        wait_started(&mut worker);
        let group = group(
            &mut ChangeJournal::new(WORKSPACE),
            &store,
            1,
            "uncertain",
            vec![7],
        );
        assert_eq!(worker.submit(group), SubmitOutcome::Accepted);
        wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Blocked { generation: 1, .. }))
        });
        worker.set_fault(Fault::None).unwrap();
        worker.retry().unwrap();
        let events = wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Durable { receipt, .. } if receipt.generation == 1))
        });
        let receipts: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                WorkerEvent::Durable { receipt, .. } => Some(receipt.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].effects, vec![7]);
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
    }

    #[test]
    fn reopen_bootstraps_nonzero_generation_and_rejects_wrong_workspace() {
        let (root, path, store) = seeded("reopen");
        let mut worker =
            PersistenceWorker::start(&path, &root, WORKSPACE, WorkerOptions::default()).unwrap();
        wait_started(&mut worker);
        let mut journal = ChangeJournal::new(WORKSPACE);
        worker.submit(group(&mut journal, &store, 1, "one", vec![1]));
        worker.submit(group(&mut journal, &store, 2, "two", vec![2]));
        wait_for(&mut worker, |events| {
            events.iter().filter(|event| matches!(event, WorkerEvent::Durable { .. })).count() >= 2
        });
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
        let mut reopened =
            PersistenceWorker::start(&path, &root, WORKSPACE, WorkerOptions::default()).unwrap();
        wait_started(&mut reopened);
        assert_eq!(reopened.durable_generation(), 2);
        let mut wrong =
            PersistenceWorker::start(&path, &root, WORKSPACE + 1, WorkerOptions::default()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while wrong.startup_error().is_none() && Instant::now() < deadline {
            let _ = wrong.poll();
            thread::sleep(Duration::from_millis(2));
        }
        assert!(wrong.startup_error().is_some());
        let stale = WorkerEvent::Durable {
            receipt: DurableReceipt {
                workspace: WORKSPACE,
                generation: 2,
                effects: vec![2],
            },
            reservation: Reservation {
                budget: Arc::new(Mutex::new(Budget::default())),
                bytes: 0,
                released: true,
            },
        };
        assert!(stale.is_stale(WORKSPACE, reopened.durable_generation()));
        let mut resumed = ChangeJournal::resume(WORKSPACE, 2);
        reopened.submit(group(&mut resumed, &store, 3, "three", vec![3]));
        wait_for(&mut reopened, |events| {
            events.iter().any(|event| matches!(event, WorkerEvent::Durable { receipt, .. } if receipt.generation == 3))
        });
        reopened.request_shutdown();
        let _ = collect_until_shutdown(&mut reopened, Duration::from_secs(5));
    }

    #[test]
    fn nonblocking_shutdown_preserves_all_accepted_outcomes_with_full_events() {
        let (root, path, store) = seeded("shutdown");
        let options = WorkerOptions {
            event_capacity: 1,
            channel_capacity: 2,
            ..WorkerOptions::default()
        };
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            options,
            Duration::ZERO,
            Some(Fault::BeforeCommit),
        )
        .unwrap();
        wait_started(&mut worker);
        let mut journal = ChangeJournal::new(WORKSPACE);
        worker.submit(group(&mut journal, &store, 1, "one", vec![1]));
        worker.submit(group(&mut journal, &store, 2, "two", vec![2]));
        wait_for(&mut worker, |events| {
            events
                .iter()
                .any(|event| matches!(event, WorkerEvent::Blocked { generation: 1, .. }))
        });
        let started = Instant::now();
        worker.request_shutdown();
        assert!(
            started.elapsed() < Duration::from_millis(5),
            "request_shutdown blocked"
        );
        let (events, joined) = collect_until_shutdown(&mut worker, Duration::from_secs(5));
        assert!(joined);
        let returned = events
            .iter()
            .filter(|event| matches!(event, WorkerEvent::Returned { .. }))
            .count();
        assert_eq!(returned, 2);
        let shutdown = events
            .iter()
            .find_map(|event| match event {
                WorkerEvent::Shutdown {
                    durable_generation,
                    blocked,
                    ..
                } => Some((*durable_generation, *blocked)),
                _ => None,
            })
            .unwrap();
        assert_eq!(shutdown, (0, Some(1)));
    }

    #[test]
    fn receipt_effects_remain_inside_the_admission_budget_until_consumed() {
        let (root, path, store) = seeded("receipt-budget");
        let options = WorkerOptions {
            max_retained_groups: 2,
            max_retained_bytes: 3_000,
            channel_capacity: 2,
            ..WorkerOptions::default()
        };
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            options,
            Duration::ZERO,
            None,
        )
        .unwrap();
        wait_started(&mut worker);
        let mut journal = ChangeJournal::new(WORKSPACE);
        assert_eq!(
            worker.submit(group(
                &mut journal,
                &store,
                1,
                "large effects",
                (0..100).collect()
            )),
            SubmitOutcome::Accepted
        );
        // The first receipt is durable but not yet consumed. Hold the event in
        // an ordinary vector; its reservation keeps the shared budget charged.
        let mut held = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.durable_generation() == 0 && Instant::now() < deadline {
            held.extend(worker.poll());
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(worker.durable_generation(), 1);
        let second = group(&mut journal, &store, 2, "second", (0..100).collect());
        assert!(
            matches!(worker.submit(second.clone()), SubmitOutcome::Backpressure(_)),
            "unconsumed durable receipt must keep its reservation"
        );
        drop(held);
        assert_eq!(worker.submit(second), SubmitOutcome::Accepted);
        wait_for(&mut worker, |events| {
            events.iter().any(|event| matches!(event, WorkerEvent::Durable { receipt, .. } if receipt.generation == 2))
        });
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
    }

    #[test]
    fn admission_budget_rejects_oversized_groups_immediately() {
        let (root, path, store) = seeded("oversized");
        let options = WorkerOptions {
            max_retained_groups: 4,
            max_retained_bytes: 1_024,
            ..WorkerOptions::default()
        };
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            options,
            Duration::ZERO,
            None,
        )
        .unwrap();
        wait_started(&mut worker);
        let mut huge = store.task(1).unwrap().clone();
        huge.description = "x".repeat(20_000);
        let mut journal = ChangeJournal::new(WORKSPACE);
        let huge_group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(huge))])
            .unwrap();
        assert!(matches!(
            worker.submit(huge_group),
            SubmitOutcome::Backpressure(_)
        ));
        worker.request_shutdown();
        let _ = collect_until_shutdown(&mut worker, Duration::from_secs(5));
    }

    #[test]
    fn shutdown_accounts_each_accepted_generation_exactly_once_with_full_events() {
        let (root, path, store) = seeded("outcomes");
        let options = WorkerOptions {
            event_capacity: 1,
            channel_capacity: 2,
            ..WorkerOptions::default()
        };
        let mut worker = PersistenceWorker::start_with_test_options(
            &path,
            &root,
            WORKSPACE,
            options,
            Duration::from_millis(150),
            None,
        )
        .unwrap();
        wait_started(&mut worker);
        let mut journal = ChangeJournal::new(WORKSPACE);
        let mut accepted = Vec::new();
        for (id, title, effect) in [(1, "one", 1), (2, "two", 2), (3, "three", 3)] {
            let candidate = group(&mut journal, &store, id, title, vec![effect]);
            if matches!(worker.submit(candidate), SubmitOutcome::Accepted) {
                accepted.push((effect as u64, effect));
            }
        }
        worker.request_shutdown();
        let (events, joined) = collect_until_shutdown(&mut worker, Duration::from_secs(10));
        assert!(joined);
        let mut outcomes: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
        for event in &events {
            match event {
                WorkerEvent::Durable { receipt, .. } => {
                    outcomes
                        .entry(receipt.generation)
                        .or_default()
                        .extend(receipt.effects.iter().copied());
                }
                WorkerEvent::Returned { group, .. } => {
                    outcomes
                        .entry(group.generation())
                        .or_default()
                        .extend(group.effects().iter().copied());
                }
                _ => {}
            }
        }
        for (generation, effect) in &accepted {
            let recorded = outcomes
                .get(generation)
                .unwrap_or_else(|| panic!("accepted generation {generation} vanished"));
            assert_eq!(recorded, &vec![*effect], "effects changed or duplicated");
        }
        assert_eq!(outcomes.len(), accepted.len(), "unexpected duplicate outcome");
    }

    #[test]
    fn startup_failure_is_observable_without_false_saved() {
        let root = temp_root("missing");
        let mut missing = PersistenceWorker::start(
            &root.join("absent.sqlite3"),
            &root,
            WORKSPACE,
            WorkerOptions::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while missing.startup_error().is_none() && Instant::now() < deadline {
            let _ = missing.poll();
            thread::sleep(Duration::from_millis(2));
        }
        assert!(missing.startup_error().is_some());
        assert!(!missing.is_started());
        let group = group(
            &mut ChangeJournal::new(WORKSPACE),
            &Store::mock(),
            1,
            "not ready",
            vec![1],
        );
        assert!(matches!(
            missing.submit(group),
            SubmitOutcome::NotReady(_)
        ));
    }
}
