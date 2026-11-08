//! Isolated schema-v4 SQLite backend for T14/T15.
//!
//! This module is intentionally not wired into `TaskApp` or `Storage`. It
//! implements the real transaction, migration, indexed page and lazy-body
//! contracts on disposable test databases only. Live activation, worker
//! attachment and `Store` mutation tracking remain later slices.
//!
//! Ownership: entity payloads and metadata deltas come from
//! `persistence_journal`; this module only maps them to rows.

#![allow(dead_code)]

use crate::automation::{
    AutomationApproval, AutomationDefinition, AutomationMaterial, AutomationOccurrence,
    AutomationOccurrenceSnapshot, AutomationPlanBinding, AutomationTemplate, TimeWindow,
    TimeWindowState,
};
use crate::automation_activity::occurrence_activity;
use crate::domain::{
    AgentRun, Comment, PlanningOwner, Project, ProjectKind, Stage, StagePolicy, Store, Task, Topic,
};
use crate::model_profiles::ModelProfile;
use crate::persistence_journal::{
    CommitSink, EntityChange, EntityKey, EntityPayload, MetadataDelta, StagedGroup, WorkTable,
};
use crate::report_publish::{Publication, PublishingConfig};
use crate::request_planning::{ResearchRequest, StructuredBrief};
use crate::research::{ResearchConfig, ResearchKind, Source};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const SCHEMA_SQL: &str = r#"
CREATE TABLE database_identity (
    id INTEGER PRIMARY KEY CHECK(id=1),
    incarnation INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    payload TEXT NOT NULL
);
CREATE TABLE commits (
    generation INTEGER PRIMARY KEY,
    workspace INTEGER NOT NULL,
    base INTEGER NOT NULL,
    intent_hash TEXT NOT NULL,
    effects TEXT NOT NULL,
    committed_at INTEGER NOT NULL
);
CREATE TABLE tasks (
    position INTEGER PRIMARY KEY,
    id INTEGER NOT NULL UNIQUE,
    project_id INTEGER NOT NULL,
    topic_id INTEGER,
    stage TEXT NOT NULL,
    archived INTEGER NOT NULL,
    automation_id INTEGER,
    payload TEXT NOT NULL
);
CREATE INDEX idx_tasks_project ON tasks(project_id, stage, archived);
CREATE TABLE projects (
    position INTEGER PRIMARY KEY,
    id INTEGER NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE policies (
    position INTEGER PRIMARY KEY,
    stage INTEGER NOT NULL UNIQUE,
    payload TEXT NOT NULL
);
CREATE TABLE automations (
    position INTEGER PRIMARY KEY,
    id INTEGER NOT NULL UNIQUE,
    project_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE runs (
    position INTEGER PRIMARY KEY,
    id INTEGER NOT NULL UNIQUE,
    task_id INTEGER NOT NULL,
    occurrence_id INTEGER,
    status TEXT NOT NULL,
    research_kind TEXT,
    has_body INTEGER NOT NULL,
    body_bytes INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX idx_runs_task ON runs(task_id, status);
CREATE INDEX idx_runs_occurrence ON runs(occurrence_id);
CREATE TABLE run_bodies (
    run_id INTEGER PRIMARY KEY,
    body TEXT NOT NULL
);
CREATE TABLE occurrence_snapshots (
    hash TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    refs INTEGER NOT NULL
);
CREATE TABLE automation_occurrences (
    position INTEGER PRIMARY KEY,
    id INTEGER NOT NULL UNIQUE,
    automation_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    unread INTEGER NOT NULL,
    activity_utc INTEGER NOT NULL,
    activity_rank INTEGER NOT NULL,
    activity_seq INTEGER NOT NULL,
    reference_time INTEGER NOT NULL,
    trigger_key TEXT NOT NULL,
    snapshot_hash TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_occ_trigger ON automation_occurrences(automation_id, trigger_key);
CREATE INDEX idx_occ_activity ON automation_occurrences(activity_utc DESC, activity_rank DESC, id DESC);
CREATE INDEX idx_occ_auto_activity ON automation_occurrences(automation_id, activity_utc DESC, activity_rank DESC, id DESC);
CREATE INDEX idx_occ_auto_unread ON automation_occurrences(automation_id) WHERE unread = 1;
CREATE INDEX idx_occ_unread ON automation_occurrences(unread) WHERE unread = 1;
"#;

/// One isolated schema-v4 database. All writes are explicit API calls.
pub(crate) struct SqliteV4 {
    connection: Connection,
    path: PathBuf,
    root: PathBuf,
    incarnation: u64,
    body_reads: Cell<u64>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum CommitOutcome {
    Committed { generation: u64 },
    AlreadyDurable { generation: u64, effects: Vec<u64> },
}

#[derive(Debug, PartialEq)]
pub(crate) enum CommitError {
    WrongIncarnation { expected: u64, found: u64 },
    GenerationGap { expected: u64, found: u64 },
    BaseMismatch { expected: u64, found: u64 },
    IntentConflict { generation: u64 },
    OutOfRange { field: &'static str, value: u64 },
    Storage(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    None,
    MidGroup,
    BeforeCommit,
    AfterCommit,
    /// Test-only hard abort after applying the first change, before commit.
    CrashMidGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MigrationFault {
    None,
    AfterBackup,
    AfterTemp,
    AfterCheckpoint,
    CrashAfterCheckpoint,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActivityCursor {
    pub activity_utc: i64,
    pub activity_rank: u8,
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageQuery {
    pub automation: Option<u64>,
    pub cursor: Option<ActivityCursor>,
    /// Commit generation captured by the first page. Rows whose `activity_seq`
    /// is newer are excluded until the caller refreshes.
    pub watermark: Option<u64>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryRow {
    pub id: u64,
    pub automation_id: u64,
    pub status: String,
    pub unread: bool,
    pub activity_utc: i64,
    pub activity_rank: u8,
    pub activity_seq: u64,
    pub reference_time: i64,
    pub run_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageMetrics {
    pub occurrence_rows: usize,
    /// The page query never joins `runs`; this stays zero.
    pub run_rows: usize,
    /// SQLite fullscan-step counter for this page statement. A deep keyset page
    /// on an indexed variant must stay near the limit, not walk the history.
    pub fullscan_steps: usize,
    /// SQLite VM-step counter for the page select itself.
    pub page_vm_steps: usize,
    /// VM steps spent in the total-count query (separate from the page select).
    pub total_count_vm_steps: usize,
    /// VM steps spent in the unread-count query.
    pub unread_count_vm_steps: usize,
    /// Cumulative body-table reads observed by this backend handle.
    pub body_reads: u64,
}

/// Explicit pagination contract. A page sequence is pinned to a commit
/// generation. If any commit lands after that watermark, a later page is
/// invalidated rather than silently filtering out a changed/unseen row; the
/// caller must restart from the first page. This is truthful invalidation, not
/// a claim of a stable snapshot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PageContract {
    Stable { watermark: u64 },
    Invalidated { watermark: u64, latest: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryPage {
    pub contract: PageContract,
    pub rows: Vec<HistoryRow>,
    pub next: Option<ActivityCursor>,
    pub total: usize,
    pub unread: usize,
    pub metrics: PageMetrics,
}

impl SqliteV4 {
    /// Create a new v4 database. The caller owns the path and must use a
    /// disposable/test location until root authorizes production activation.
    pub(crate) fn create_new(path: &Path, root: &Path, incarnation: u64) -> Result<Self, String> {
        let incarnation_sql = sql_i64(incarnation, "incarnation")?;
        if path.exists() {
            return Err(format!("refusing to overwrite existing {}", path.display()));
        }
        ensure_absolute(root)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        drop(
            options
                .open(path)
                .map_err(|error| format!("cannot create {}: {error}", path.display()))?,
        );
        let connection = Connection::open(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        configure_connection(&connection)?;
        connection
            .execute_batch(SCHEMA_SQL)
            .map_err(|error| format!("cannot create schema: {error}"))?;
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| format!("cannot set schema version: {error}"))?;
        connection
            .execute(
                "INSERT INTO database_identity(id,incarnation,created_at) VALUES(1,?1,?2)",
                params![incarnation_sql, now()],
            )
            .map_err(|error| format!("cannot write identity: {error}"))?;
        // Keep schema and identity in the main file so a read-only immutable
        // preflight can validate a WAL database without creating sidecars.
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("cannot checkpoint new database: {error}"))?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
            root: root.to_path_buf(),
            incarnation,
            body_reads: Cell::new(0),
        })
    }

    /// Create and seed a v4 database from an in-memory store. This is a
    /// migration/test helper, not the live commit path.
    pub(crate) fn create_seeded(
        path: &Path,
        root: &Path,
        incarnation: u64,
        store: &Store,
    ) -> Result<Self, String> {
        let mut database = Self::create_new(path, root, incarnation)?;
        database.seed_store(store)?;
        if database.load_store()? != *store {
            return Err("seeded v4 database did not round-trip its store".into());
        }
        Ok(database)
    }

    pub(crate) fn open(path: &Path, root: &Path, incarnation: u64) -> Result<Self, String> {
        ensure_absolute(root)?;
        if !path.is_file() {
            return Err(format!("workspace database does not exist: {}", path.display()));
        }
        // Preflight read-only and immutable: no database is created, no WAL/shm
        // sidecar is written, and unsupported/foreign databases are rejected
        // before any journal-mode change. Version and identity are checkpointed
        // into the main file when the schema is created.
        let probe = Connection::open_with_flags(
            immutable_uri(path),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_URI
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        probe
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        let version: i64 = probe
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| format!("cannot read schema version: {error}"))?;
        if version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported SQLite schema version {version} (expected {SCHEMA_VERSION})"
            ));
        }
        let stored: i64 = probe
            .query_row(
                "SELECT incarnation FROM database_identity WHERE id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("cannot read database identity: {error}"))?;
        if stored as u64 != incarnation {
            return Err(format!(
                "workspace incarnation mismatch: database {stored}, expected {incarnation}"
            ));
        }
        drop(probe);
        let connection = Connection::open(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        configure_connection(&connection)?;
        Ok(Self {
            connection,
            path: path.to_path_buf(),
            root: root.to_path_buf(),
            incarnation,
            body_reads: Cell::new(0),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn incarnation(&self) -> u64 {
        self.incarnation
    }

    pub(crate) fn durable_generation(&self) -> Result<u64, String> {
        let value: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(generation),0) FROM commits",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("cannot read durable generation: {error}"))?;
        u64::try_from(value).map_err(|_| format!("negative durable generation {value}"))
    }

    pub(crate) fn receipt(&self, generation: u64) -> Result<Option<Vec<u64>>, String> {
        let payload: Option<String> = self
            .connection
            .query_row(
                "SELECT effects FROM commits WHERE generation=?1",
                [sql_i64(generation, "receipt generation")?],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("cannot read receipt: {error}"))?;
        match payload {
            None => Ok(None),
            Some(payload) => serde_json::from_str(&payload)
                .map(Some)
                .map_err(|error| format!("invalid stored effects: {error}")),
        }
    }

    pub(crate) fn commit_group(
        &mut self,
        group: &StagedGroup,
    ) -> Result<CommitOutcome, CommitError> {
        self.commit_inner(group, Fault::None)
    }

    #[cfg(any(test, feature = "qa"))]
    pub(crate) fn commit_group_with_fault(
        &mut self,
        group: &StagedGroup,
        fault: Fault,
    ) -> Result<CommitOutcome, CommitError> {
        self.commit_inner(group, fault)
    }

    fn commit_inner(
        &mut self,
        group: &StagedGroup,
        fault: Fault,
    ) -> Result<CommitOutcome, CommitError> {
        if group.workspace() != self.incarnation {
            return Err(CommitError::WrongIncarnation {
                expected: self.incarnation,
                found: group.workspace(),
            });
        }
        let intent_hash = intent_hash(group)?;
        let generation_sql = sql_i64(group.generation(), "generation")
            .map_err(|_| CommitError::OutOfRange { field: "generation", value: group.generation() })?;
        let base_sql = sql_i64(group.base(), "base")
            .map_err(|_| CommitError::OutOfRange { field: "base", value: group.base() })?;
        let incarnation_sql = sql_i64(self.incarnation, "incarnation")
            .map_err(|_| CommitError::OutOfRange { field: "incarnation", value: self.incarnation })?;
        // All generation/base/receipt checks are inside the write transaction so
        // two handles cannot both pass a pre-transaction check.
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| CommitError::Storage(error.to_string()))?;
        let existing: Option<(i64, String, String)> = transaction
            .query_row(
                "SELECT workspace,intent_hash,effects FROM commits WHERE generation=?1",
                [generation_sql],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| CommitError::Storage(error.to_string()))?;
        if let Some((workspace, stored_hash, effects)) = existing {
            if workspace as u64 != self.incarnation {
                drop(transaction);
                return Err(CommitError::WrongIncarnation {
                    expected: self.incarnation,
                    found: workspace as u64,
                });
            }
            if stored_hash != intent_hash {
                drop(transaction);
                return Err(CommitError::IntentConflict {
                    generation: group.generation(),
                });
            }
            let effects: Vec<u64> = serde_json::from_str(&effects)
                .map_err(|error| CommitError::Storage(error.to_string()))?;
            drop(transaction);
            return Ok(CommitOutcome::AlreadyDurable {
                generation: group.generation(),
                effects,
            });
        }
        let durable: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(generation),0) FROM commits",
                [],
                |row| row.get(0),
            )
            .map_err(|error| CommitError::Storage(error.to_string()))?;
        let durable = durable as u64;
        if group.generation() != durable + 1 {
            drop(transaction);
            return Err(CommitError::GenerationGap {
                expected: durable + 1,
                found: group.generation(),
            });
        }
        if group.base() != durable {
            drop(transaction);
            return Err(CommitError::BaseMismatch {
                expected: durable,
                found: group.base(),
            });
        }
        for (index, change) in group.changes().iter().enumerate() {
            apply_change(&transaction, change, &self.root, generation_sql)
                .map_err(CommitError::Storage)?;
            if index == 0 && fault == Fault::CrashMidGroup {
                if let Ok(marker) = std::env::var("T14_CRASH_MARKER") {
                    let _ = fs::write(marker, b"mid-group-reached");
                }
                std::process::abort();
            }
            if fault == Fault::MidGroup && index == 0 {
                return Err(CommitError::Storage("injected mid-group failure".into()));
            }
        }
        for delta in group.metadata() {
            apply_metadata(&transaction, delta).map_err(CommitError::Storage)?;
        }
        transaction
            .execute(
                "INSERT INTO commits(generation,workspace,base,intent_hash,effects,committed_at) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    generation_sql,
                    incarnation_sql,
                    base_sql,
                    intent_hash,
                    serde_json::to_string(group.effects()).map_err(|error| CommitError::Storage(error.to_string()))?,
                    now(),
                ],
            )
            .map_err(|error| CommitError::Storage(error.to_string()))?;
        if fault == Fault::BeforeCommit {
            return Err(CommitError::Storage("injected before-commit failure".into()));
        }
        transaction
            .commit()
            .map_err(|error| CommitError::Storage(error.to_string()))?;
        if fault == Fault::AfterCommit {
            return Err(CommitError::Storage(
                "injected after-commit uncertainty".into(),
            ));
        }
        Ok(CommitOutcome::Committed {
            generation: group.generation(),
        })
    }

    fn seed_store(&mut self, store: &Store) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| format!("cannot begin seed transaction: {error}"))?;
        for task in &store.tasks {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Task(task.clone())),
                &self.root,
                0,
            )?;
        }
        for project in &store.projects {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Project(project.clone())),
                &self.root,
                0,
            )?;
        }
        for policy in &store.policies {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Policy(policy.clone())),
                &self.root,
                0,
            )?;
        }
        for run in store.runs() {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Run(run.clone())),
                &self.root,
                0,
            )?;
        }
        for automation in store.automations() {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Automation(automation.clone())),
                &self.root,
                0,
            )?;
        }
        for occurrence in store.all_occurrences() {
            apply_change(
                &transaction,
                &EntityChange::Upsert(EntityPayload::Occurrence(occurrence.clone())),
                &self.root,
                0,
            )?;
        }
        seed_metadata(&transaction, store)?;
        transaction
            .commit()
            .map_err(|error| format!("cannot commit seed: {error}"))?;
        Ok(())
    }

    pub(crate) fn load_store(&self) -> Result<Store, String> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("cannot start read transaction: {error}"))?;
        let mut store = Store::empty();
        let mut meta = BTreeMap::new();
        {
            let mut statement = transaction
                .prepare("SELECT key,payload FROM meta")
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (key, payload) = row.map_err(|error| error.to_string())?;
                let value: Value =
                    serde_json::from_str(&payload).map_err(|error| error.to_string())?;
                meta.insert(key, value);
            }
        }
        store.next_id = parse_meta(&meta, "next_id")?;
        store.opencode_path = parse_meta(&meta, "opencode_path")?;
        store.default_model = parse_meta(&meta, "default_model")?;
        store.model_profiles = parse_meta(&meta, "model_profiles")?;
        store.active_model_profile = parse_meta(&meta, "active_model_profile")?;
        store.max_concurrent_runs = parse_meta(&meta, "max_concurrent_runs")?;
        store.max_concurrent_automations = parse_meta(&meta, "max_concurrent_automations")?;
        store.research_config = parse_meta(&meta, "research_config")?;
        store.appearance = parse_meta(&meta, "appearance")?;
        store.last_active_project = parse_meta(&meta, "last_active_project")?;
        store.automation_schema_version = parse_meta(&meta, "automation_schema_version")?;
        store.report_publishing = parse_meta(&meta, "report_publishing")?;
        store.publications = parse_meta(&meta, "publications")?;
        store.topics = parse_meta(&meta, "topics")?;
        store.research_requests = parse_meta(&meta, "research_requests")?;

        store.tasks = load_rows(&transaction, "tasks", &self.root, WorkTable::Tasks)?;
        store.projects = load_rows(&transaction, "projects", &self.root, WorkTable::Projects)?;
        let runs = load_runs(&transaction, &self.root)?;
        store.policies = load_policies(&transaction)?;
        let automations = load_rows(
            &transaction,
            "automations",
            &self.root,
            WorkTable::Automations,
        )?;
        let occurrences = load_occurrences(&transaction)?;
        let store = store
            .with_loaded_tracked(runs, automations, occurrences)
            .map_err(|(_, error)| error)?;
        transaction
            .commit()
            .map_err(|error| format!("cannot finish read transaction: {error}"))?;
        Ok(store)
    }

    pub(crate) fn page(&self, query: &PageQuery) -> Result<HistoryPage, String> {
        let limit = query.limit.clamp(1, 1000);
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| format!("cannot start page transaction: {error}"))?;
        let latest: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(generation),0) FROM commits",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let latest = latest as u64;
        let watermark = query.watermark.unwrap_or(latest);
        let (total, total_count_vm_steps) = count_on(&transaction, query.automation, false)?;
        let (unread, unread_count_vm_steps) = count_on(&transaction, query.automation, true)?;
        if latest > watermark {
            transaction
                .commit()
                .map_err(|error| error.to_string())?;
            return Ok(HistoryPage {
                contract: PageContract::Invalidated {
                    watermark,
                    latest,
                },
                rows: Vec::new(),
                next: None,
                total,
                unread,
                metrics: PageMetrics {
                    occurrence_rows: 0,
                    run_rows: 0,
                    fullscan_steps: 0,
                    page_vm_steps: 0,
                    total_count_vm_steps,
                    unread_count_vm_steps,
                    body_reads: self.body_reads.get(),
                },
            });
        }
        let values = page_values(query, watermark, limit)?;
        let mut statement = transaction
            .prepare(&page_sql(query))
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params_from_iter(values.iter().map(|value| value.as_ref())),
                |row| {
                    let payload: String = row.get(8)?;
                    let value: Value = serde_json::from_str(&payload).unwrap_or(Value::Null);
                    let run_ids = value
                        .get("run_ids")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_u64)
                                .collect::<Vec<u64>>()
                        })
                        .unwrap_or_default();
                    Ok(HistoryRow {
                        id: row.get::<_, i64>(0)? as u64,
                        automation_id: row.get::<_, i64>(1)? as u64,
                        status: row.get(2)?,
                        unread: row.get::<_, i64>(3)? != 0,
                        activity_utc: row.get(4)?,
                        activity_rank: row.get::<_, i64>(5)? as u8,
                        activity_seq: row.get::<_, i64>(6)? as u64,
                        reference_time: row.get(7)?,
                        run_ids,
                    })
                },
            )
            .map_err(|error| error.to_string())?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let fullscan_steps = statement.get_status(rusqlite::StatementStatus::FullscanStep) as usize;
        let page_vm_steps = statement.get_status(rusqlite::StatementStatus::VmStep) as usize;
        drop(statement);
        let next = if rows.len() == limit {
            rows.last().map(|row| ActivityCursor {
                activity_utc: row.activity_utc,
                activity_rank: row.activity_rank,
                id: row.id,
            })
        } else {
            None
        };
        transaction
            .commit()
            .map_err(|error| error.to_string())?;
        Ok(HistoryPage {
            contract: PageContract::Stable { watermark },
            metrics: PageMetrics {
                occurrence_rows: rows.len(),
                run_rows: 0,
                fullscan_steps,
                page_vm_steps,
                total_count_vm_steps,
                unread_count_vm_steps,
                body_reads: self.body_reads.get(),
            },
            rows,
            next,
            total,
            unread,
        })
    }


    /// Fetch one report body on demand. This is the only API that reads
    /// `run_bodies`; page queries never join it.
    pub(crate) fn body(&self, run_id: u64) -> Result<Option<String>, String> {
        self.body_reads.set(self.body_reads.get() + 1);
        self.connection
            .query_row(
                "SELECT body FROM run_bodies WHERE run_id=?1",
                [sql_i64(run_id, "run id")?],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn occurrence(
        &self,
        id: u64,
    ) -> Result<Option<AutomationOccurrence>, String> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT o.payload,s.payload FROM automation_occurrences o JOIN occurrence_snapshots s ON s.hash=o.snapshot_hash WHERE o.id=?1",
                [sql_i64(id, "occurrence id")?],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((payload, snapshot)) = row else {
            return Ok(None);
        };
        let mut value: Value = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
        value["snapshot"] = serde_json::from_str(&snapshot).map_err(|error| error.to_string())?;
        serde_json::from_value(value)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn checkpoint(&self) -> Result<(), String> {
        self.connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|_| ())
            .map_err(|error| format!("cannot checkpoint WAL: {error}"))
    }

    #[cfg(test)]
    fn page_plan(&self, query: &PageQuery) -> Result<String, String> {
        let watermark = query.watermark.unwrap_or(0);
        let sql = format!("EXPLAIN QUERY PLAN {}", page_sql(query));
        let values = page_values(query, watermark, query.limit.clamp(1, 1000))?;
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params_from_iter(values.iter().map(|value| value.as_ref())),
                |row| row.get::<_, String>(3),
            )
            .map_err(|error| error.to_string())?;
        let mut plan = String::new();
        for row in rows {
            plan.push_str(&row.map_err(|error| error.to_string())?);
            plan.push('\n');
        }
        Ok(plan)
    }
}

impl CommitSink for SqliteV4 {
    fn commit(&mut self, group: &StagedGroup) -> Result<(), String> {
        match self.commit_group(group) {
            Ok(_) => Ok(()),
            Err(error) => Err(format!("{error:?}")),
        }
    }
}

fn page_sql(query: &PageQuery) -> String {
    let select = "SELECT id,automation_id,status,unread,activity_utc,activity_rank,activity_seq,reference_time,payload FROM automation_occurrences";
    let order = "ORDER BY activity_utc DESC, activity_rank DESC, id DESC";
    match (query.automation.is_some(), query.cursor.is_some()) {
        (false, false) => format!("{select} WHERE activity_seq<=?1 {order} LIMIT ?2"),
        (false, true) => format!(
            "{select} WHERE activity_seq<=?1 AND (activity_utc,activity_rank,id) < (?2,?3,?4) {order} LIMIT ?5"
        ),
        (true, false) => format!(
            "{select} WHERE automation_id=?1 AND activity_seq<=?2 {order} LIMIT ?3"
        ),
        (true, true) => format!(
            "{select} WHERE automation_id=?1 AND activity_seq<=?2 AND (activity_utc,activity_rank,id) < (?3,?4,?5) {order} LIMIT ?6"
        ),
    }
}

fn page_values(
    query: &PageQuery,
    watermark: u64,
    limit: usize,
) -> Result<Vec<Box<dyn ToSql>>, String> {
    let cursor = query.cursor.as_ref();
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(automation) = query.automation {
        values.push(Box::new(sql_i64(automation, "automation filter")?));
    }
    values.push(Box::new(sql_i64(watermark, "watermark")?));
    if let Some(cursor) = cursor {
        values.push(Box::new(cursor.activity_utc));
        values.push(Box::new(cursor.activity_rank as i64));
        values.push(Box::new(sql_i64(cursor.id, "cursor id")?));
    }
    values.push(Box::new(sql_i64(limit as u64, "limit")?));
    Ok(values)
}

fn configure_connection(connection: &Connection) -> Result<(), String> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| format!("cannot enable WAL: {error}"))?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| format!("cannot set synchronous=FULL: {error}"))?;
    Ok(())
}

fn ensure_absolute(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!(
            "workspace root must be absolute, got {}",
            path.display()
        ));
    }
    Ok(())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Entity and metadata row application
// ---------------------------------------------------------------------------

fn apply_change(
    transaction: &rusqlite::Transaction<'_>,
    change: &EntityChange,
    root: &Path,
    activity_seq: i64,
) -> Result<(), String> {
    match change {
        EntityChange::Delete(key) => delete_entity(transaction, key),
        EntityChange::Upsert(payload) => upsert_payload(transaction, payload, root, activity_seq),
    }
}

fn upsert_payload(
    transaction: &rusqlite::Transaction<'_>,
    payload: &EntityPayload,
    root: &Path,
    activity_seq: i64,
) -> Result<(), String> {
    match payload {
        EntityPayload::Task(task) => upsert_task(transaction, task),
        EntityPayload::Project(project) => upsert_project(transaction, project, root),
        EntityPayload::Policy(policy) => upsert_policy(transaction, policy),
        EntityPayload::Run(run) => upsert_run(transaction, run, root),
        EntityPayload::Automation(automation) => upsert_automation(transaction, automation, root),
        EntityPayload::Occurrence(occurrence) => {
            upsert_occurrence(transaction, occurrence, activity_seq)
        }
    }
}

fn upsert_task(transaction: &rusqlite::Transaction<'_>, task: &Task) -> Result<(), String> {
    let value = serde_json::to_value(task).map_err(|error| error.to_string())?;
    let id = sql_i64(task.id, "task id")?;
    let position = position_for(transaction, "tasks", "id", id)?;
    transaction
        .execute(
            "INSERT INTO tasks(position,id,project_id,topic_id,stage,archived,automation_id,payload) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8) \
             ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,topic_id=excluded.topic_id,stage=excluded.stage,archived=excluded.archived,automation_id=excluded.automation_id,payload=excluded.payload",
            params![
                position,
                id,
                sql_i64(task.project_id, "task project id")?,
                sql_opt_i64(task.topic_id, "task topic id")?,
                task.stage.key(),
                task.archived as i64,
                sql_opt_i64(task.automation_id, "task automation id")?,
                value.to_string(),
            ],
        )
        .map_err(|error| format!("cannot write task {}: {error}", task.id))?;
    Ok(())
}

fn upsert_project(
    transaction: &rusqlite::Transaction<'_>,
    project: &Project,
    root: &Path,
) -> Result<(), String> {
    let mut value = serde_json::to_value(project).map_err(|error| error.to_string())?;
    transform_entity_paths(&mut value, WorkTable::Projects, root, true)?;
    let id = sql_i64(project.id, "project id")?;
    let position = position_for(transaction, "projects", "id", id)?;
    let kind = match project.kind {
        ProjectKind::User => "user",
        ProjectKind::Automation => "automation",
    };
    transaction
        .execute(
            "INSERT INTO projects(position,id,kind,payload) VALUES(?1,?2,?3,?4) \
             ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,payload=excluded.payload",
            params![position, id, kind, value.to_string()],
        )
        .map_err(|error| format!("cannot write project {}: {error}", project.id))?;
    Ok(())
}

fn upsert_policy(
    transaction: &rusqlite::Transaction<'_>,
    policy: &StagePolicy,
) -> Result<(), String> {
    let value = serde_json::to_value(policy).map_err(|error| error.to_string())?;
    let stage = policy.stage as i64;
    let position = position_for(transaction, "policies", "stage", stage)?;
    transaction
        .execute(
            "INSERT INTO policies(position,stage,payload) VALUES(?1,?2,?3) \
             ON CONFLICT(stage) DO UPDATE SET payload=excluded.payload",
            params![position, stage, value.to_string()],
        )
        .map_err(|error| format!("cannot write policy {stage}: {error}"))?;
    Ok(())
}

fn upsert_automation(
    transaction: &rusqlite::Transaction<'_>,
    automation: &AutomationDefinition,
    root: &Path,
) -> Result<(), String> {
    let mut value = serde_json::to_value(automation).map_err(|error| error.to_string())?;
    transform_entity_paths(&mut value, WorkTable::Automations, root, true)?;
    let id = sql_i64(automation.id, "automation id")?;
    let position = position_for(transaction, "automations", "id", id)?;
    transaction
        .execute(
            "INSERT INTO automations(position,id,project_id,status,updated_at,payload) VALUES(?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,status=excluded.status,updated_at=excluded.updated_at,payload=excluded.payload",
            params![
                position,
                id,
                sql_i64(automation.project_id, "automation project id")?,
                automation.status.label(),
                automation.updated_at,
                value.to_string(),
            ],
        )
        .map_err(|error| format!("cannot write automation {}: {error}", automation.id))?;
    Ok(())
}

fn upsert_run(
    transaction: &rusqlite::Transaction<'_>,
    run: &AgentRun,
    root: &Path,
) -> Result<(), String> {
    let mut value = serde_json::to_value(run).map_err(|error| error.to_string())?;
    let output = value
        .as_object_mut()
        .and_then(|object| object.remove("output"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    transform_entity_paths(&mut value, WorkTable::Runs, root, true)?;
    let id = sql_i64(run.id, "run id")?;
    let position = position_for(transaction, "runs", "id", id)?;
    let kind = run.research_kind.map(|kind| kind.label());
    transaction
        .execute(
            "INSERT INTO runs(position,id,task_id,occurrence_id,status,research_kind,has_body,body_bytes,payload) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) \
             ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id,occurrence_id=excluded.occurrence_id,status=excluded.status,research_kind=excluded.research_kind,has_body=excluded.has_body,body_bytes=excluded.body_bytes,payload=excluded.payload",
            params![
                position,
                id,
                sql_i64(run.task_id, "run task id")?,
                sql_opt_i64(run.automation_occurrence_id, "run occurrence id")?,
                run.status.as_str(),
                kind,
                (!output.is_empty()) as i64,
                i64::try_from(output.len()).map_err(|_| "run body length exceeds SQLite INTEGER range".to_string())?,
                value.to_string(),
            ],
        )
        .map_err(|error| format!("cannot write run {}: {error}", run.id))?;
    if output.is_empty() {
        transaction
            .execute("DELETE FROM run_bodies WHERE run_id=?1", [id])
            .map_err(|error| format!("cannot clear run body {}: {error}", run.id))?;
    } else {
        transaction
            .execute(
                "INSERT INTO run_bodies(run_id,body) VALUES(?1,?2) \
                 ON CONFLICT(run_id) DO UPDATE SET body=excluded.body",
                params![id, output],
            )
            .map_err(|error| format!("cannot write run body {}: {error}", run.id))?;
    }
    Ok(())
}

fn upsert_occurrence(
    transaction: &rusqlite::Transaction<'_>,
    occurrence: &AutomationOccurrence,
    activity_seq: i64,
) -> Result<(), String> {
    let mut value = serde_json::to_value(occurrence).map_err(|error| error.to_string())?;
    let snapshot = value
        .as_object_mut()
        .and_then(|object| object.remove("snapshot"))
        .ok_or_else(|| format!("occurrence {} has no snapshot", occurrence.id))?;
    let snapshot_json = snapshot.to_string();
    let snapshot_hash = canonical_hash(&snapshot)?;
    let occurrence_id = sql_i64(occurrence.id, "occurrence id")?;
    let old_hash: Option<String> = transaction
        .query_row(
            "SELECT snapshot_hash FROM automation_occurrences WHERE id=?1",
            [occurrence_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if old_hash.as_deref() != Some(snapshot_hash.as_str()) {
        if let Some(old_hash) = old_hash.as_deref() {
            decrement_snapshot(transaction, old_hash)?;
        }
        transaction
            .execute(
                "INSERT INTO occurrence_snapshots(hash,payload,refs) VALUES(?1,?2,1) \
                 ON CONFLICT(hash) DO UPDATE SET refs=refs+1",
                params![snapshot_hash, snapshot_json],
            )
            .map_err(|error| format!("cannot write snapshot: {error}"))?;
    }
    let activity = occurrence_activity(occurrence);
    let position = position_for(transaction, "automation_occurrences", "id", occurrence_id)?;
    transaction
        .execute(
            "INSERT INTO automation_occurrences(position,id,automation_id,status,unread,activity_utc,activity_rank,activity_seq,reference_time,trigger_key,snapshot_hash,payload) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
             ON CONFLICT(id) DO UPDATE SET automation_id=excluded.automation_id,status=excluded.status,unread=excluded.unread,activity_utc=excluded.activity_utc,activity_rank=excluded.activity_rank,activity_seq=excluded.activity_seq,reference_time=excluded.reference_time,trigger_key=excluded.trigger_key,snapshot_hash=excluded.snapshot_hash,payload=excluded.payload",
            params![
                position,
                occurrence_id,
                sql_i64(occurrence.automation_id, "occurrence automation id")?,
                occurrence.status.label(),
                occurrence.unread as i64,
                activity.utc,
                activity.kind as i64,
                activity_seq,
                occurrence.reference_time,
                occurrence.trigger_key.as_str(),
                snapshot_hash,
                value.to_string(),
            ],
        )
        .map_err(|error| format!("cannot write occurrence {}: {error}", occurrence.id))?;
    Ok(())
}

fn decrement_snapshot(
    transaction: &rusqlite::Transaction<'_>,
    hash: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "UPDATE occurrence_snapshots SET refs=refs-1 WHERE hash=?1",
            [hash],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM occurrence_snapshots WHERE hash=?1 AND refs<=0", [hash])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn delete_entity(
    transaction: &rusqlite::Transaction<'_>,
    key: &EntityKey,
) -> Result<(), String> {
    match key {
        EntityKey::Task(id) => {
            transaction
                .execute("DELETE FROM tasks WHERE id=?1", [sql_i64(*id, "task id")?])
                .map_err(|error| error.to_string())?;
        }
        EntityKey::Project(id) => {
            transaction
                .execute("DELETE FROM projects WHERE id=?1", [sql_i64(*id, "project id")?])
                .map_err(|error| error.to_string())?;
        }
        EntityKey::Run(id) => {
            transaction
                .execute("DELETE FROM run_bodies WHERE run_id=?1", [sql_i64(*id, "run id")?])
                .map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM runs WHERE id=?1", [sql_i64(*id, "run id")?])
                .map_err(|error| error.to_string())?;
        }
        EntityKey::Policy(stage) => {
            transaction
                .execute("DELETE FROM policies WHERE stage=?1", [*stage as i64])
                .map_err(|error| error.to_string())?;
        }
        EntityKey::Automation(id) => {
            transaction
                .execute("DELETE FROM automations WHERE id=?1", [sql_i64(*id, "automation id")?])
                .map_err(|error| error.to_string())?;
        }
        EntityKey::Occurrence(id) => {
            let occurrence_id = sql_i64(*id, "occurrence id")?;
            let hash: Option<String> = transaction
                .query_row(
                    "SELECT snapshot_hash FROM automation_occurrences WHERE id=?1",
                    [occurrence_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(hash) = hash.as_deref() {
                decrement_snapshot(transaction, hash)?;
            }
            transaction
                .execute("DELETE FROM automation_occurrences WHERE id=?1", [occurrence_id])
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn apply_metadata(
    transaction: &rusqlite::Transaction<'_>,
    delta: &MetadataDelta,
) -> Result<(), String> {
    let value = metadata_value(delta)?;
    transaction
        .execute(
            "INSERT INTO meta(key,payload) VALUES(?1,?2) \
             ON CONFLICT(key) DO UPDATE SET payload=excluded.payload",
            params![delta.field(), value.to_string()],
        )
        .map_err(|error| format!("cannot write metadata {}: {error}", delta.field()))?;
    Ok(())
}

fn metadata_value(delta: &MetadataDelta) -> Result<Value, String> {
    match delta {
        MetadataDelta::NextId(value) => serde_json::to_value(value),
        MetadataDelta::DefaultModel(value) => serde_json::to_value(value),
        MetadataDelta::OpenCodePath(value) => serde_json::to_value(value),
        MetadataDelta::Appearance(value) => serde_json::to_value(value),
        MetadataDelta::LastActiveProject(value) => serde_json::to_value(value),
        MetadataDelta::AutomationSchemaVersion(value) => serde_json::to_value(value),
        MetadataDelta::MaxConcurrentRuns(value) => serde_json::to_value(value),
        MetadataDelta::MaxConcurrentAutomations(value) => serde_json::to_value(value),
        MetadataDelta::ResearchConfig(value) => serde_json::to_value(value),
        MetadataDelta::ModelProfiles(value) => serde_json::to_value(value),
        MetadataDelta::ActiveModelProfile(value) => serde_json::to_value(value),
        MetadataDelta::Topics(value) => serde_json::to_value(value),
        MetadataDelta::ResearchRequests(value) => serde_json::to_value(value),
        MetadataDelta::Publications(value) => serde_json::to_value(value),
        MetadataDelta::PublishingConfig(value) => serde_json::to_value(value),
    }
    .map_err(|error| error.to_string())
}

fn seed_metadata(
    transaction: &rusqlite::Transaction<'_>,
    store: &Store,
) -> Result<(), String> {
    let values = [
        ("next_id", serde_json::to_value(store.next_id)),
        ("opencode_path", serde_json::to_value(&store.opencode_path)),
        ("default_model", serde_json::to_value(&store.default_model)),
        ("model_profiles", serde_json::to_value(&store.model_profiles)),
        (
            "active_model_profile",
            serde_json::to_value(&store.active_model_profile),
        ),
        (
            "max_concurrent_runs",
            serde_json::to_value(store.max_concurrent_runs),
        ),
        (
            "max_concurrent_automations",
            serde_json::to_value(store.max_concurrent_automations),
        ),
        ("research_config", serde_json::to_value(&store.research_config)),
        ("appearance", serde_json::to_value(store.appearance)),
        (
            "last_active_project",
            serde_json::to_value(store.last_active_project),
        ),
        (
            "automation_schema_version",
            serde_json::to_value(store.automation_schema_version),
        ),
        (
            "report_publishing",
            serde_json::to_value(&store.report_publishing),
        ),
        ("publications", serde_json::to_value(&store.publications)),
        ("topics", serde_json::to_value(&store.topics)),
        (
            "research_requests",
            serde_json::to_value(&store.research_requests),
        ),
    ];
    for (key, value) in values {
        let value = value.map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO meta(key,payload) VALUES(?1,?2) \
                 ON CONFLICT(key) DO UPDATE SET payload=excluded.payload",
                params![key, value.to_string()],
            )
            .map_err(|error| format!("cannot write metadata {key}: {error}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

fn parse_meta<T: DeserializeOwned>(meta: &BTreeMap<String, Value>, key: &str) -> Result<T, String> {
    let value = meta
        .get(key)
        .ok_or_else(|| format!("missing metadata field {key}"))?;
    serde_json::from_value(value.clone()).map_err(|error| format!("invalid metadata {key}: {error}"))
}

fn load_rows<T: DeserializeOwned>(
    connection: &Connection,
    table: &str,
    root: &Path,
    work_table: WorkTable,
) -> Result<Vec<T>, String> {
    let mut statement = connection
        .prepare(&format!("SELECT payload FROM {table} ORDER BY position"))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| error.to_string())?;
        let mut value: Value = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
        transform_entity_paths(&mut value, work_table, root, false)?;
        values.push(serde_json::from_value(value).map_err(|error| error.to_string())?);
    }
    Ok(values)
}

fn load_policies(connection: &Connection) -> Result<Vec<StagePolicy>, String> {
    let mut statement = connection
        .prepare("SELECT payload FROM policies ORDER BY position")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| error.to_string())?;
        values.push(serde_json::from_str(&payload).map_err(|error| error.to_string())?);
    }
    Ok(values)
}

fn load_runs(connection: &Connection, root: &Path) -> Result<Vec<AgentRun>, String> {
    let mut statement = connection
        .prepare(
            "SELECT r.payload,b.body FROM runs r LEFT JOIN run_bodies b ON b.run_id=r.id ORDER BY r.position",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for row in rows {
        let (payload, body) = row.map_err(|error| error.to_string())?;
        let mut value: Value = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
        value["output"] = Value::String(body.unwrap_or_default());
        transform_entity_paths(&mut value, WorkTable::Runs, root, false)?;
        values.push(serde_json::from_value(value).map_err(|error| error.to_string())?);
    }
    Ok(values)
}

fn load_occurrences(connection: &Connection) -> Result<Vec<AutomationOccurrence>, String> {
    let mut statement = connection
        .prepare(
            "SELECT o.payload,s.payload FROM automation_occurrences o JOIN occurrence_snapshots s ON s.hash=o.snapshot_hash ORDER BY o.position",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for row in rows {
        let (payload, snapshot) = row.map_err(|error| error.to_string())?;
        let mut value: Value = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
        value["snapshot"] = serde_json::from_str(&snapshot).map_err(|error| error.to_string())?;
        values.push(serde_json::from_value(value).map_err(|error| error.to_string())?);
    }
    Ok(values)
}

fn transform_entity_paths(
    value: &mut Value,
    table: WorkTable,
    root: &Path,
    encode: bool,
) -> Result<(), String> {
    let key = match table {
        WorkTable::Projects => "projects",
        WorkTable::Runs => "runs",
        WorkTable::Automations => "automations",
        _ => return Ok(()),
    };
    let mut wrapper = json!({ key: [value.clone()] });
    crate::sqlite_storage::transform_paths(&mut wrapper, root, encode)?;
    *value = wrapper[key]
        .as_array_mut()
        .and_then(|items| items.pop())
        .ok_or_else(|| format!("path codec lost the {key} payload"))?;
    Ok(())
}

fn position_for(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    id_column: &str,
    id: i64,
) -> Result<i64, String> {
    let existing: Option<i64> = transaction
        .query_row(
            &format!("SELECT position FROM {table} WHERE {id_column}=?1"),
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(position) = existing {
        return Ok(position);
    }
    transaction
        .query_row(
            &format!("SELECT COALESCE(MAX(position),0)+1 FROM {table}"),
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn canonical_hash(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Serialize)]
struct IntentView<'a> {
    workspace: u64,
    generation: u64,
    base: u64,
    changes: &'a [EntityChange],
    metadata: &'a [MetadataDelta],
    effects: &'a [u64],
}

fn intent_hash(group: &StagedGroup) -> Result<String, CommitError> {
    let view = IntentView {
        workspace: group.workspace(),
        generation: group.generation(),
        base: group.base(),
        changes: group.changes(),
        metadata: group.metadata(),
        effects: group.effects(),
    };
    let bytes = serde_json::to_vec(&view)
        .map_err(|error| CommitError::Storage(format!("cannot hash intent: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn immutable_uri(path: &Path) -> String {
    let encoded = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace(' ', "%20");
    format!("file:{encoded}?immutable=1")
}

fn sql_i64(value: u64, label: &str) -> Result<i64, String> {
    i64::try_from(value)
        .map_err(|_| format!("{label} {value} exceeds SQLite INTEGER range"))
}

fn sql_opt_i64(value: Option<u64>, label: &str) -> Result<Option<i64>, String> {
    value.map(|value| sql_i64(value, label)).transpose()
}

fn row_u64(value: i64, label: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("negative {label} {value} in database"))
}

fn count_on(
    connection: &Connection,
    automation: Option<u64>,
    unread_only: bool,
) -> Result<(usize, usize), String> {
    let (sql, values): (&str, Vec<Box<dyn ToSql>>) = match (automation, unread_only) {
        (Some(automation), false) => (
            "SELECT COUNT(*) FROM automation_occurrences WHERE automation_id=?1",
            vec![Box::new(sql_i64(automation, "automation filter")?)],
        ),
        (Some(automation), true) => (
            "SELECT COUNT(*) FROM automation_occurrences WHERE automation_id=?1 AND unread=1",
            vec![Box::new(sql_i64(automation, "automation filter")?)],
        ),
        (None, false) => ("SELECT COUNT(*) FROM automation_occurrences", Vec::new()),
        (None, true) => (
            "SELECT COUNT(*) FROM automation_occurrences WHERE unread=1",
            Vec::new(),
        ),
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| error.to_string())?;
    let value = statement
        .query_row(
            params_from_iter(values.iter().map(|value| value.as_ref())),
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    let vm_steps = statement.get_status(rusqlite::StatementStatus::VmStep) as usize;
    Ok((value as usize, vm_steps))
}

// ---------------------------------------------------------------------------
// Explicit v3 -> v4 migration
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MigrationOutcome {
    Migrated { backup: PathBuf },
    AlreadyCurrent,
}

pub(crate) fn migrate_v3_to_v4(
    path: &Path,
    root: &Path,
    incarnation: u64,
) -> Result<MigrationOutcome, String> {
    migrate_inner(path, root, incarnation, MigrationFault::None)
}

fn migrate_inner(
    path: &Path,
    root: &Path,
    incarnation: u64,
    fault: MigrationFault,
) -> Result<MigrationOutcome, String> {
    ensure_absolute(root)?;
    if !path.is_file() {
        return Err(format!("workspace database does not exist: {}", path.display()));
    }
    // Exclusive ownership for the whole read/backup/replace sequence. The lock
    // is the same workspace lock the live app uses, so a running app or another
    // cooperative migrator prevents replacement instead of racing it.
    let _lock = crate::persistence::Storage::new(path.to_path_buf()).acquire_lock()?;
    let version = read_user_version(path)?;
    if version == SCHEMA_VERSION {
        let current = SqliteV4::open(path, root, incarnation)?;
        current.load_store()?;
        return Ok(MigrationOutcome::AlreadyCurrent);
    }
    if version > SCHEMA_VERSION {
        return Err(format!(
            "unsupported future SQLite schema version {version}; the file was left unchanged"
        ));
    }
    if version < 3 {
        return Err(format!(
            "schema version {version} must first use the existing v1/v2 path-upgrade route; no file was changed"
        ));
    }
    let store = crate::sqlite_storage::load(path, root)?
        .ok_or("v3 workspace returned no store")?;
    validate_store(&store)?;
    let backup = unique_sibling(path, "before-schema-v4", "sqlite3");
    vacuum_backup(path, &backup)?;
    let backup_store = crate::sqlite_storage::load(&backup, root)?
        .ok_or("migration backup could not be reloaded")?;
    if backup_store != store {
        return Err("migration backup does not match the original logical state".into());
    }
    if fault == MigrationFault::AfterBackup {
        return Err("injected failure after verified backup".into());
    }
    // The verified backup already includes committed WAL data. Under the
    // exclusive workspace lock, checkpoint the original so its main file is
    // self-contained before sidecar cleanup or replacement. A busy checkpoint
    // means no replacement is attempted; a partial checkpoint may already have
    // occurred, but the original logical state remains readable and the verified
    // backup remains valid.
    checkpoint_original(path)?;
    let temporary = unique_sibling(path, "v4-migrating", "sqlite3");
    let result = (|| -> Result<(), String> {
        let database = SqliteV4::create_seeded(&temporary, root, incarnation, &store)?;
        database.checkpoint()?;
        drop(database);
        let _ = fs::remove_file(temporary.with_extension("sqlite3-wal"));
        let _ = fs::remove_file(temporary.with_extension("sqlite3-shm"));
        fsync_file(&temporary)?;
        if let Some(parent) = temporary.parent() {
            fsync_dir(parent)?;
        }
        if fault == MigrationFault::AfterTemp {
            return Err("injected failure after verified v4 temporary write".into());
        }
        // Old v3 WAL/SHM sidecars must never be replayed against the new main
        // file. Move them aside only after the v4 temporary is verified, and
        // restore them if the main-file rename fails.
        let mut moved_sidecars: Vec<(PathBuf, PathBuf)> = Vec::new();
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
            if sidecar.exists() {
                let preserved = unique_sibling(path, &format!("old{suffix}"), "sidecar");
                if let Err(error) = fs::rename(&sidecar, &preserved) {
                    for (original, preserved) in moved_sidecars.drain(..).rev() {
                        let _ = fs::rename(preserved, original);
                    }
                    return Err(format!("cannot preserve old database sidecar: {error}"));
                }
                moved_sidecars.push((sidecar, preserved));
            }
        }
        if fault == MigrationFault::CrashAfterCheckpoint {
            if let Ok(marker) = std::env::var("T14_CRASH_MARKER") {
                let _ = fs::write(marker, b"migration-checkpoint-reached");
            }
            std::process::abort();
        }
        if fault == MigrationFault::AfterCheckpoint {
            return Err("injected failure after checkpoint/sidecar handling".into());
        }
        if let Err(error) = fs::rename(&temporary, path) {
            for (original, preserved) in moved_sidecars.drain(..).rev() {
                let _ = fs::rename(preserved, original);
            }
            return Err(format!("cannot install v4 database: {error}"));
        }
        if let Some(parent) = path.parent() {
            fsync_dir(parent)?;
        }
        let reopened = SqliteV4::open(path, root, incarnation)?;
        if reopened.load_store()? != store {
            return Err("installed v4 database did not round-trip the original state".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(MigrationOutcome::Migrated { backup })
}

fn checkpoint_original(path: &Path) -> Result<(), String> {
    let connection = Connection::open(path)
        .map_err(|error| format!("cannot open source for checkpoint: {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let busy: i64 = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
        .map_err(|error| format!("cannot checkpoint source WAL: {error}"))?;
    if busy != 0 {
        return Err(format!(
            "source WAL checkpoint is busy (busy={busy}); no replacement was attempted"
        ));
    }
    Ok(())
}

fn read_user_version(path: &Path) -> Result<i64, String> {
    let connection = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("cannot read schema version: {error}"))
}

fn validate_store(store: &Store) -> Result<(), String> {
    let mut ids = HashSet::new();
    for task in &store.tasks {
        if !ids.insert(("task", task.id)) {
            return Err(format!("duplicate task id {} before migration", task.id));
        }
    }
    let mut ids = HashSet::new();
    for project in &store.projects {
        if !ids.insert(project.id) {
            return Err(format!("duplicate project id {} before migration", project.id));
        }
    }
    let mut ids = HashSet::new();
    for run in store.runs() {
        if !ids.insert(run.id) {
            return Err(format!("duplicate run id {} before migration", run.id));
        }
    }
    let mut ids = HashSet::new();
    for automation in store.automations() {
        if !ids.insert(automation.id) {
            return Err(format!("duplicate automation id {} before migration", automation.id));
        }
    }
    let mut ids = HashSet::new();
    let mut triggers = HashSet::new();
    for occurrence in store.all_occurrences() {
        if !ids.insert(occurrence.id) {
            return Err(format!(
                "duplicate occurrence id {} before migration",
                occurrence.id
            ));
        }
        if !triggers.insert((occurrence.automation_id, occurrence.trigger_key.clone())) {
            return Err(format!(
                "duplicate trigger key {} for automation {} before migration",
                occurrence.trigger_key, occurrence.automation_id
            ));
        }
    }
    Ok(())
}

fn vacuum_backup(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!("backup target already exists: {}", destination.display()));
    }
    // Read-only source: SQLite reads committed WAL content for VACUUM INTO
    // without checkpointing or otherwise mutating the source database.
    let connection = Connection::open_with_flags(
        source,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot open source for backup: {error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(
        options
            .open(destination)
            .map_err(|error| format!("cannot create backup: {error}"))?,
    );
    connection
        .execute(
            "VACUUM INTO ?1",
            [destination
                .to_str()
                .ok_or("backup path must be UTF-8")?],
        )
        .map_err(|error| format!("cannot write backup: {error}"))?;
    let backup = Connection::open_with_flags(
        destination,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|error| format!("cannot verify backup: {error}"))?;
    let checks: Vec<String> = {
        let mut statement = backup
            .prepare("PRAGMA integrity_check")
            .map_err(|error| error.to_string())?;
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?
    };
    if checks != ["ok"] {
        return Err("migration backup failed its integrity check".into());
    }
    fsync_file(destination)?;
    if let Some(parent) = destination.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

pub(crate) fn backup_v3_verified(
    source: &Path,
    destination: &Path,
    root: &Path,
    expected: &Store,
) -> Result<(), String> {
    vacuum_backup(source, destination)?;
    let restored = crate::sqlite_storage::load(destination, root)?
        .ok_or("v3 backup returned no store")?;
    if restored != *expected {
        return Err("v3 backup logical state differs from the original".into());
    }
    Ok(())
}

pub(crate) fn backup_v4_verified(
    source: &Path,
    destination: &Path,
    root: &Path,
    incarnation: u64,
) -> Result<(), String> {
    let source_db = SqliteV4::open(source, root, incarnation)?;
    let expected = source_db.load_store()?;
    vacuum_backup(source, destination)?;
    let backup_db = SqliteV4::open(destination, root, incarnation)?;
    if backup_db.load_store()? != expected {
        return Err("v4 backup logical state differs from the source".into());
    }
    Ok(())
}

fn unique_sibling(path: &Path, tag: &str, extension: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    path.with_file_name(format!(
        "{tag}-{nanos}-{}-{}.{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        extension
    ))
}

fn fsync_file(path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("cannot open {} for fsync: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("cannot fsync {}: {error}", path.display()))
}

fn fsync_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let directory = fs::File::open(path)
            .map_err(|error| format!("cannot open {} for fsync: {error}", path.display()))?;
        directory
            .sync_all()
            .map_err(|error| format!("cannot fsync directory {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationMaterial, AutomationShape};
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::persistence_journal::ChangeJournal;
    use crate::research::Source;
    use std::sync::atomic::{AtomicU64, Ordering};

    const INCARNATION: u64 = 11;
    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "t14-v4-{name}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn make_run(id: u64, task: u64, occurrence: Option<u64>, body: &str) -> AgentRun {
        AgentRun {
            automation_occurrence_id: occurrence,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id,
            task_title: format!("Step {id}"),
            directory: String::new(),
            prompt: "prompt".into(),
            started_at: "2026-11-08T00:00:00Z".into(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: Some("2026-11-08T00:01:00Z".into()),
            session_id: Some(format!("session-{id}")),
            exit_code: Some(0),
            task_id: task,
            agent: "OpenCode".into(),
            model: "qa/model".into(),
            stage: Stage::Done,
            status: "Succeeded".into(),
            logs: vec!["[log] representative line".into()],
            output: body.into(),
            attachments: Vec::new(),
            research_kind: Some(ResearchKind::Advise),
            sources: vec![Source {
                title: "Source".into(),
                url: format!("https://example.com/{id}"),
                published_at: "unknown".into(),
            }],
            input_signature: "[\"signature\"]".into(),
        }
    }

    fn fixture_store(root: &Path) -> Store {
        let mut store = Store::mock();
        let project_directory = root.join("project");
        fs::create_dir_all(&project_directory).unwrap();
        store.projects[0].directory = project_directory.to_string_lossy().into_owned();
        store.default_model = "qa/model".into();
        store.research_config.models = std::array::from_fn(|_| "qa/model".into());
        let schedule = Schedule {
            spec: ScheduleSpec::Interval { minutes: 10 },
            timezone: "UTC".into(),
        };
        let body = "Representative report body. ".repeat(80);

        let quick_template = crate::automation_execution::build_quick_template(
            "Check one changed public value and cite its source.",
            "One short answer with a source.",
        )
        .unwrap();
        let quick = store
            .create_automation_draft(
                1,
                AutomationShape::Quick,
                "Quick fixture".into(),
                "Check one changed public value and cite its source.".into(),
                "One short answer with a source.".into(),
                schedule.clone(),
                Default::default(),
                Some(quick_template),
                1_000,
            )
            .unwrap();
        store.approve_and_enable(quick, Vec::new(), 1_000).unwrap();
        let mut materialized = store
            .request_manual_occurrence(quick, "quick-manual-1", 1_100)
            .unwrap();
        let goal = materialized.research_goal_id.unwrap();
        let mut run_ids = Vec::new();
        for task in store.research_children(goal) {
            let run = make_run(
                store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1,
                task,
                Some(materialized.id),
                &body,
            );
            run_ids.push(run.id);
            store.push_run(run);
        }
        materialized.run_ids = run_ids.clone();
        materialized.final_run_id = run_ids.last().copied();
        materialized.retry_requested_at = Some(1_200);
        materialized.unread = true;
        materialized.snapshot.materials.push(AutomationMaterial {
            task_id: goal,
            name: "brief.pdf".into(),
            signature: "a".repeat(64),
        });
        let materialized_id = materialized.id;
        *store.occurrence_mut(materialized_id).unwrap() = materialized;

        let research_template = crate::automation_execution::build_template(
            "Track one recurring public change and cite sources.",
            "A report with sources and evidence gaps.",
        )
        .unwrap();
        let research = store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Research fixture".into(),
                "Track one recurring public change and cite sources.".into(),
                "A report with sources and evidence gaps.".into(),
                schedule,
                Default::default(),
                Some(research_template),
                1_300,
            )
            .unwrap();
        store.approve_and_enable(research, Vec::new(), 1_300).unwrap();
        store.automation_mut(research).unwrap().cursor = Some(1_900);
        let mut materialized = store
            .request_manual_occurrence(research, "research-manual-1", 1_400)
            .unwrap();
        let goal = materialized.research_goal_id.unwrap();
        let mut run_ids = Vec::new();
        for task in store.research_children(goal) {
            let run = make_run(
                store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1,
                task,
                Some(materialized.id),
                &body,
            );
            run_ids.push(run.id);
            store.push_run(run);
        }
        materialized.run_ids = run_ids.clone();
        materialized.final_run_id = run_ids.last().copied();
        materialized.unread = false;
        let materialized_id = materialized.id;
        *store.occurrence_mut(materialized_id).unwrap() = materialized;

        let planning_id = store.runs().iter().map(|run| run.id).max().unwrap_or(0) + 1;
        let mut planning = make_run(planning_id, goal, None, "planning output");
        planning.status = "Succeeded".into();
        planning.planning_owner = Some(PlanningOwner {
            request_id: 1,
            revision: 2,
            automation_id: Some(research),
        });
        planning.input_signature = "[\"plan-signature\"]".into();
        store.push_run(planning);

        let mut stale = store.runs().last().unwrap().clone();
        stale.id += 1_000;
        stale.input_signature = "[\"stale-before-move\"]".into();
        stale.output = "stale report".into();
        store.push_run(stale);

        store.research_requests.push(ResearchRequest {
            id: 1,
            project_id: 1,
            topic_id: None,
            automation_id: Some(research),
            expected_result: Some("A report with sources and evidence gaps.".into()),
            revision: 2,
            provisional_topic_name: None,
            input: "Track one recurring public change and cite sources.".into(),
            submitted_at: "2026-11-08T00:00:00Z".into(),
            planning_run_id: Some(planning_id),
            goal_ids: vec![goal],
            brief: None,
        });
        store.next_id = store
            .runs()
            .iter()
            .map(|run| run.id)
            .max()
            .unwrap_or(store.next_id)
            .max(store.next_id)
            + 1;
        store
    }

    fn add_plain_occurrences(store: &mut Store, count: usize) {
        let template = store.all_occurrences()[0].clone();
        for index in 0..count {
            let id = store.next_occurrence_id().unwrap();
            let mut occurrence = template.clone();
            occurrence.id = id;
            occurrence.automation_id = template.automation_id;
            occurrence.trigger_key = format!("manual:plain-{id}");
            occurrence.status = crate::automation::OccurrenceStatus::Succeeded;
            occurrence.unread = false;
            occurrence.reference_time = 2_000 + index as i64;
            occurrence.scheduled_for = Some(2_000 + index as i64);
            occurrence.requested_at = Some(2_000 + index as i64);
            occurrence.actual_started_at = Some(2_000 + index as i64);
            occurrence.retry_requested_at = None;
            occurrence.research_goal_id = None;
            occurrence.research_goal_ids.clear();
            occurrence.run_ids.clear();
            occurrence.final_run_id = None;
            store.record_occurrence_without_research(occurrence).unwrap();
        }
    }

    #[test]
    fn seeded_v4_round_trips_and_uses_indexes() {
        let root = temp_root("roundtrip");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        let database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        assert_eq!(database.load_store().unwrap(), store);
        assert_eq!(
            database
                .connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap()
                .to_lowercase(),
            "wal"
        );
        assert_eq!(
            database
                .connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
        let snapshot_count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM occurrence_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(snapshot_count >= 1);
        let body_count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM run_bodies", [], |row| row.get(0))
            .unwrap();
        assert!(body_count > 0);
    }

    #[test]
    fn commit_writes_only_changed_rows_and_persists_receipt() {
        let root = temp_root("commit");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        let mut database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let mut changed = store.task(1).unwrap().clone();
        changed.title = "changed through v4".into();
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage_with_effects(
                vec![EntityChange::Upsert(EntityPayload::Task(changed.clone()))],
                vec![9],
            )
            .unwrap();
        let before = database.connection.total_changes();
        let outcome = database.commit_group(&group).unwrap();
        let after = database.connection.total_changes();
        assert_eq!(outcome, CommitOutcome::Committed { generation: 1 });
        assert!(
            after - before <= 4,
            "only the task row and receipt may change, got {} changes",
            after - before
        );
        assert_eq!(database.durable_generation().unwrap(), 1);
        assert_eq!(database.receipt(1).unwrap(), Some(vec![9]));
        drop(database);
        let reopened = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        assert_eq!(reopened.load_store().unwrap().task(1).unwrap().title, changed.title);
    }

    #[test]
    fn commit_faults_are_atomic_and_uncertain_commit_is_idempotent() {
        let root = temp_root("fault");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        let mut journal = ChangeJournal::new(INCARNATION);
        let mut changed = store.task(1).unwrap().clone();
        changed.title = "faulted title".into();
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(changed))])
            .unwrap();

        for fault in [Fault::MidGroup, Fault::BeforeCommit] {
            let fault_path = root.join(format!("{fault:?}.sqlite3"));
            let mut database =
                SqliteV4::create_seeded(&fault_path, &root, INCARNATION, &store).unwrap();
            assert!(database
                .commit_group_with_fault(&group, fault)
                .is_err());
            assert_eq!(database.load_store().unwrap(), store);
            assert_eq!(database.durable_generation().unwrap(), 0);
            let commits: i64 = database
                .connection
                .query_row("SELECT COUNT(*) FROM commits", [], |row| row.get(0))
                .unwrap();
            assert_eq!(commits, 0);
        }

        let after_path = root.join("after.sqlite3");
        let mut database =
            SqliteV4::create_seeded(&after_path, &root, INCARNATION, &store).unwrap();
        assert!(database
            .commit_group_with_fault(&group, Fault::AfterCommit)
            .is_err());
        assert_eq!(database.durable_generation().unwrap(), 1);
        let applied = database.load_store().unwrap();
        assert_eq!(applied.task(1).unwrap().title, "faulted title");
        assert_eq!(
            database.commit_group(&group).unwrap(),
            CommitOutcome::AlreadyDurable {
                generation: 1,
                effects: Vec::new()
            }
        );
        let mut different = applied.task(1).unwrap().clone();
        different.title = "different payload".into();
        let mut other_journal = ChangeJournal::new(INCARNATION);
        let conflicting = other_journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(different))])
            .unwrap();
        assert_eq!(
            database.commit_group(&conflicting).unwrap_err(),
            CommitError::IntentConflict { generation: 1 }
        );
        assert_eq!(database.load_store().unwrap().task(1).unwrap().title, "faulted title");
    }

    #[test]
    fn two_handles_cannot_both_commit_the_same_generation() {
        let root = temp_root("two-handles");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let mut first = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        let mut second = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        let mut journal = ChangeJournal::new(INCARNATION);
        let mut changed = store.task(1).unwrap().clone();
        changed.title = "first writer".into();
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(changed))])
            .unwrap();
        first.commit_group(&group).unwrap();
        assert_eq!(
            second.commit_group(&group).unwrap(),
            CommitOutcome::AlreadyDurable {
                generation: 1,
                effects: Vec::new()
            }
        );
        let mut different = store.task(1).unwrap().clone();
        different.title = "second writer".into();
        let mut other = ChangeJournal::new(INCARNATION);
        let conflict = other
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(different))])
            .unwrap();
        assert_eq!(
            second.commit_group(&conflict).unwrap_err(),
            CommitError::IntentConflict { generation: 1 }
        );
        assert_eq!(first.load_store().unwrap().task(1).unwrap().title, "first writer");
    }

    #[test]
    fn open_preflight_creates_nothing_and_rejects_foreign_databases() {
        let root = temp_root("preflight");
        let path = root.join("missing.sqlite3");
        let before = file_names(&root);
        assert!(SqliteV4::open(&path, &root, INCARNATION).is_err());
        assert!(!path.exists());
        assert_eq!(file_names(&root), before);

        let store = fixture_store(&root);
        let good = root.join("good.sqlite3");
        let db = SqliteV4::create_seeded(&good, &root, INCARNATION, &store).unwrap();
        db.checkpoint().unwrap();
        drop(db);
        assert_eq!(
            SqliteV4::open(&good, &root, INCARNATION)
                .unwrap()
                .connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap()
                .to_lowercase(),
            "wal"
        );

        let future = root.join("future.sqlite3");
        fs::copy(&good, &future).unwrap();
        set_user_version(&future, 99);
        let hash = file_hash(&future);
        let files = file_names(&root);
        assert!(SqliteV4::open(&future, &root, INCARNATION).is_err());
        assert_eq!(file_hash(&future), hash);
        assert_eq!(file_names(&root), files);

        let wrong = root.join("wrong.sqlite3");
        fs::copy(&good, &wrong).unwrap();
        let hash = file_hash(&wrong);
        let files = file_names(&root);
        assert!(SqliteV4::open(&wrong, &root, INCARNATION + 1).is_err());
        assert_eq!(file_hash(&wrong), hash);
        assert_eq!(file_names(&root), files);
    }

    #[test]
    fn page_uses_activity_index_cursor_and_lazy_bodies() {
        let root = temp_root("page");
        let mut store = fixture_store(&root);
        add_plain_occurrences(&mut store, 30);
        let path = root.join("workspace.sqlite3");
        let database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let page = database
            .page(&PageQuery {
                automation: None,
                cursor: None,
                watermark: None,
                limit: 10,
            })
            .unwrap();
        assert!(matches!(page.contract, PageContract::Stable { .. }));
        let mut expected: Vec<_> = store.all_occurrences().iter().collect();
        expected.sort_by(|left, right| {
            crate::automation_activity::compare_newest_first(left, right)
        });
        let expected_ids: Vec<u64> = expected.iter().take(10).map(|value| value.id).collect();
        assert_eq!(page.rows.iter().map(|row| row.id).collect::<Vec<_>>(), expected_ids);
        assert_eq!(page.metrics.run_rows, 0);
        assert_eq!(page.metrics.body_reads, 0);
        let plan = database
            .page_plan(&PageQuery {
                automation: None,
                cursor: None,
                watermark: None,
                limit: 10,
            })
            .unwrap();
        assert!(plan.contains("idx_occ"), "unexpected plan: {plan}");
        let run = store
            .runs()
            .iter()
            .find(|run| !run.output.is_empty())
            .unwrap()
            .clone();
        assert!(database.body(run.id).unwrap().unwrap().contains("Representative"));
        assert_eq!(database.body_reads.get(), 1);
    }

    #[test]
    fn pagination_invalidates_on_retry_or_insert_and_restarts() {
        let root = temp_root("pagination");
        let mut store = fixture_store(&root);
        add_plain_occurrences(&mut store, 20);
        let path = root.join("workspace.sqlite3");
        let mut database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let first = database
            .page(&PageQuery {
                automation: None,
                cursor: None,
                watermark: None,
                limit: 5,
            })
            .unwrap();
        let watermark = match first.contract {
            PageContract::Stable { watermark } => watermark,
            other => panic!("expected stable first page, got {other:?}"),
        };
        assert!(first.next.is_some());
        let unseen = store
            .all_occurrences()
            .iter()
            .find(|occurrence| !first.rows.iter().any(|row| row.id == occurrence.id))
            .unwrap()
            .clone();
        let mut retried = unseen.clone();
        retried.retry_requested_at = Some(9_000);
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Occurrence(retried))])
            .unwrap();
        database.commit_group(&group).unwrap();
        let second = database
            .page(&PageQuery {
                automation: None,
                cursor: first.next.clone(),
                watermark: Some(watermark),
                limit: 5,
            })
            .unwrap();
        assert_eq!(
            second.contract,
            PageContract::Invalidated {
                watermark,
                latest: 1
            }
        );
        assert!(second.rows.is_empty());
        let restarted = database
            .page(&PageQuery {
                automation: None,
                cursor: None,
                watermark: None,
                limit: 5,
            })
            .unwrap();
        assert_eq!(restarted.rows[0].id, unseen.id);
        assert_eq!(
            restarted.rows.iter().filter(|row| row.id == unseen.id).count(),
            1
        );

        let mut inserted = unseen.clone();
        inserted.id = store.next_occurrence_id().unwrap();
        inserted.trigger_key = format!("manual:inserted-{}", inserted.id);
        inserted.retry_requested_at = None;
        inserted.reference_time = 9_500;
        inserted.requested_at = Some(9_500);
        inserted.actual_started_at = Some(9_500);
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Occurrence(inserted))])
            .unwrap();
        database.commit_group(&group).unwrap();
        let third = database
            .page(&PageQuery {
                automation: None,
                cursor: restarted.next.clone(),
                watermark: match restarted.contract {
                    PageContract::Stable { watermark } => Some(watermark),
                    _ => None,
                },
                limit: 5,
            })
            .unwrap();
        assert!(matches!(third.contract, PageContract::Invalidated { .. }));
    }

    #[test]
    fn cursor_occurrence_and_run_link_commit_atomically() {
        let root = temp_root("atomic");
        let mut store = fixture_store(&root);
        let automation = store.automations()[0].id;
        let occurrence = store.all_occurrences()[0].clone();
        let new_run_id = store.runs().iter().map(|run| run.id).max().unwrap() + 500;
        let mut run = store.runs()[0].clone();
        run.id = new_run_id;
        run.automation_occurrence_id = Some(occurrence.id);
        run.output = "new linked body".into();
        let mut definition = store.automation(automation).unwrap().clone();
        definition.cursor = Some(8_888);
        let mut period = occurrence.clone();
        period.run_ids.push(new_run_id);
        period.final_run_id = Some(new_run_id);
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage(vec![
                EntityChange::Upsert(EntityPayload::Automation(definition)),
                EntityChange::Upsert(EntityPayload::Occurrence(period)),
                EntityChange::Upsert(EntityPayload::Run(run)),
            ])
            .unwrap();
        let fault_path = root.join("atomic-fault.sqlite3");
        let mut faulted =
            SqliteV4::create_seeded(&fault_path, &root, INCARNATION, &store).unwrap();
        assert!(faulted
            .commit_group_with_fault(&group, Fault::BeforeCommit)
            .is_err());
        assert_eq!(faulted.load_store().unwrap(), store);
        assert_eq!(faulted.durable_generation().unwrap(), 0);

        let path = root.join("atomic.sqlite3");
        let mut database =
            SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        database.commit_group(&group).unwrap();
        let loaded = database.load_store().unwrap();
        assert_eq!(loaded.automation(automation).unwrap().cursor, Some(8_888));
        let linked = loaded.occurrence(occurrence.id).unwrap();
        assert!(linked.run_ids.contains(&new_run_id));
        assert_eq!(linked.final_run_id, Some(new_run_id));
        assert_eq!(loaded.runs().iter().find(|run| run.id == new_run_id).unwrap().output, "new linked body");
    }

    #[test]
    fn wal_safe_backup_includes_committed_wal_content() {
        let root = temp_root("wal-backup");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        let mut database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let mut changed = store.task(1).unwrap().clone();
        changed.title = "wal-backed title".into();
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(changed))])
            .unwrap();
        database.commit_group(&group).unwrap();
        let expected = database.load_store().unwrap();
        drop(database);
        let backup = root.join("wal-backup.sqlite3");
        backup_v4_verified(&path, &backup, &root, INCARNATION).unwrap();
        let restored = SqliteV4::open(&backup, &root, INCARNATION)
            .unwrap()
            .load_store()
            .unwrap();
        assert_eq!(restored, expected);
    }

    fn add_plain_occurrences_for(
        store: &mut Store,
        count: usize,
        automation: u64,
        prefix: &str,
    ) {
        let template = store.all_occurrences()[0].clone();
        let start = store
            .all_occurrences()
            .iter()
            .map(|occurrence| occurrence.id)
            .max()
            .unwrap_or(0)
            + 1;
        for index in 0..count {
            let id = start + index as u64;
            let mut occurrence = template.clone();
            occurrence.id = id;
            occurrence.automation_id = automation;
            occurrence.trigger_key = format!("{prefix}-{id}");
            occurrence.status = crate::automation::OccurrenceStatus::Succeeded;
            occurrence.unread = false;
            occurrence.reference_time = 20_000 + index as i64;
            occurrence.scheduled_for = Some(20_000 + index as i64);
            occurrence.requested_at = Some(20_000 + index as i64);
            occurrence.actual_started_at = Some(20_000 + index as i64);
            occurrence.retry_requested_at = None;
            occurrence.research_goal_id = None;
            occurrence.research_goal_ids.clear();
            occurrence.run_ids.clear();
            occurrence.final_run_id = None;
            store.push_occurrence(occurrence);
        }
        store.next_id = store.next_id.max(start + count as u64);
    }

    #[test]
    fn deep_page_at_43200_periods_is_index_bounded() {
        let root = temp_root("deep-page");
        let mut store = fixture_store(&root);
        let other = store.automations()[0].id;
        let target = store.automations()[1].id;
        add_plain_occurrences_for(&mut store, 43_188, other, "manual:other");
        add_plain_occurrences_for(&mut store, 10, target, "manual:target");
        assert_eq!(store.all_occurrences().len(), 43_200);
        let path = root.join("workspace.sqlite3");
        let database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM automation_occurrences", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 43_200);
        let (utc, rank, id): (i64, i64, i64) = database
            .connection
            .query_row(
                "SELECT activity_utc,activity_rank,id FROM automation_occurrences ORDER BY activity_utc DESC,activity_rank DESC,id DESC LIMIT 1 OFFSET 30000",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let deep = PageQuery {
            automation: None,
            cursor: Some(ActivityCursor {
                activity_utc: utc,
                activity_rank: rank as u8,
                id: id as u64,
            }),
            watermark: Some(0),
            limit: 20,
        };
        let page = database.page(&deep).unwrap();
        assert_eq!(page.rows.len(), 20);
        assert!(
            page.metrics.fullscan_steps <= 100,
            "deep page walked {} rows; plan {}",
            page.metrics.fullscan_steps,
            database.page_plan(&deep).unwrap()
        );
        let plan = database.page_plan(&deep).unwrap();
        assert!(plan.contains("idx_occ_activity"), "plan: {plan}");
        assert!(plan.contains("SEARCH"), "deep cursor must seek, not scan: {plan}");

        let target_count: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM automation_occurrences WHERE automation_id=?1",
                [target as i64],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(target_count, 11, "fixture occurrence plus ten target rows");
        let (utc, rank, id): (i64, i64, i64) = database
            .connection
            .query_row(
                "SELECT activity_utc,activity_rank,id FROM automation_occurrences WHERE automation_id=?1 ORDER BY activity_utc DESC,activity_rank DESC,id DESC LIMIT 1 OFFSET 5",
                [target as i64],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let rare = PageQuery {
            automation: Some(target),
            cursor: Some(ActivityCursor {
                activity_utc: utc,
                activity_rank: rank as u8,
                id: id as u64,
            }),
            watermark: Some(0),
            limit: 5,
        };
        let rare_page = database.page(&rare).unwrap();
        assert_eq!(rare_page.rows.len(), 5);
        assert!(rare_page.rows.iter().all(|row| row.automation_id == target));
        assert!(rare_page.metrics.fullscan_steps <= 50);
        assert!(rare_page.metrics.page_vm_steps <= 200);
        let rare_plan = database.page_plan(&rare).unwrap();
        assert!(rare_plan.contains("idx_occ_auto_activity"), "plan: {rare_plan}");
        assert!(rare_plan.contains("SEARCH"), "plan: {rare_plan}");
        // Counts are reported separately in the metrics; SQLite may optimize a
        // COUNT(*) to fewer VM steps than the select, so equality is not assumed.
        assert_eq!(page.metrics.occurrence_rows, 20);
    }

    #[test]
    fn equal_time_ties_paginate_without_loss_or_duplicates() {
        let root = temp_root("ties");
        let mut store = fixture_store(&root);
        add_plain_occurrences(&mut store, 8);
        let ids: Vec<u64> = store.all_occurrences().iter().map(|occurrence| occurrence.id).collect();
        for id in ids {
            let mut occurrence = store.occurrence_mut(id).unwrap();
            occurrence.reference_time = 7_000;
            occurrence.scheduled_for = Some(7_000);
            occurrence.requested_at = Some(7_000);
            occurrence.actual_started_at = Some(7_000);
            occurrence.retry_requested_at = None;
        }
        let path = root.join("workspace.sqlite3");
        let database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        let mut expected: Vec<_> = store.all_occurrences().iter().collect();
        expected.sort_by(|left, right| {
            crate::automation_activity::compare_newest_first(left, right)
        });
        let expected_ids: Vec<u64> = expected.iter().map(|value| value.id).collect();
        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let page = database
                .page(&PageQuery {
                    automation: None,
                    cursor,
                    watermark: Some(0),
                    limit: 3,
                })
                .unwrap();
            assert!(matches!(page.contract, PageContract::Stable { .. }));
            seen.extend(page.rows.iter().map(|row| row.id));
            cursor = page.next.clone();
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen, expected_ids);
        let unique: HashSet<_> = seen.iter().copied().collect();
        assert_eq!(unique.len(), seen.len());
    }

    #[test]
    fn migration_preserves_state_and_backup_and_is_idempotent() {
        let root = temp_root("migration");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        crate::sqlite_storage::save(&path, &root, &store).unwrap();
        let before = fs::read(&path).unwrap();
        let outcome = migrate_v3_to_v4(&path, &root, INCARNATION).unwrap();
        let backup = match outcome {
            MigrationOutcome::Migrated { backup } => backup,
            other => panic!("expected migration, got {other:?}"),
        };
        assert_ne!(fs::read(&path).unwrap(), before);
        let migrated = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        assert_eq!(migrated.load_store().unwrap(), store);
        assert_eq!(
            crate::sqlite_storage::load(&backup, &root).unwrap().unwrap(),
            store
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let integrity: String = Connection::open(&backup)
            .unwrap()
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        assert_eq!(
            migrate_v3_to_v4(&path, &root, INCARNATION).unwrap(),
            MigrationOutcome::AlreadyCurrent
        );
    }

    #[test]
    fn migration_requires_exclusive_lock_and_valid_current_identity() {
        let root = temp_root("migration-lock");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        crate::sqlite_storage::save(&path, &root, &store).unwrap();
        let before = fs::read(&path).unwrap();
        {
            let _lock = crate::persistence::Storage::new(path.clone())
                .acquire_lock()
                .unwrap();
            assert!(migrate_v3_to_v4(&path, &root, INCARNATION).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), before);

        let current = root.join("current.sqlite3");
        SqliteV4::create_seeded(&current, &root, INCARNATION, &store).unwrap();
        let hash = file_hash(&current);
        assert!(migrate_v3_to_v4(&current, &root, INCARNATION + 1).is_err());
        assert_eq!(file_hash(&current), hash);
    }

    #[test]
    fn verified_backup_does_not_mutate_a_wal_source() {
        let root = temp_root("wal-preserve");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        let mut database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        // Establish a checkpointed baseline, then make the new title WAL-only.
        database.checkpoint().unwrap();
        database
            .connection
            .pragma_update(None, "wal_autocheckpoint", 0)
            .unwrap();
        let mut changed = store.task(1).unwrap().clone();
        changed.title = "wal source latest".into();
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(changed))])
            .unwrap();
        database.commit_group(&group).unwrap();
        // Keep the source connection open: closing the last connection may
        // checkpoint and remove the WAL, which would make the central claim
        // vacuous.
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        assert!(wal.exists(), "source WAL was not created");
        assert!(fs::metadata(&wal).unwrap().len() > 0, "source WAL is empty");
        let main_before = file_hash(&path);
        let wal_before = file_hash(&wal);

        // The latest commit is only in WAL: an immutable main-file read must
        // still see the old title.
        let main_only = Connection::open_with_flags(
            immutable_uri(&path),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .unwrap();
        let payload: String = main_only
            .query_row("SELECT payload FROM tasks WHERE id=1", [], |row| row.get(0))
            .unwrap();
        let main_task: Value = serde_json::from_str(&payload).unwrap();
        assert_ne!(main_task["title"], "wal source latest");

        let backup = root.join("preserve-backup.sqlite3");
        backup_v4_verified(&path, &backup, &root, INCARNATION).unwrap();
        assert_eq!(file_hash(&path), main_before, "backup mutated the main file");
        assert!(wal.exists(), "backup removed the source WAL");
        assert_eq!(file_hash(&wal), wal_before, "backup checkpointed the source WAL");
        let restored = SqliteV4::open(&backup, &root, INCARNATION)
            .unwrap()
            .load_store()
            .unwrap();
        assert_eq!(restored.task(1).unwrap().title, "wal source latest");
    }

    #[test]
    fn wal_migration_faults_preserve_source_and_latest_committed_state() {
        let root = temp_root("wal-migration");
        let store = fixture_store(&root);
        let base = root.join("base.sqlite3");
        crate::sqlite_storage::save(&base, &root, &store).unwrap();
        for fault in [
            MigrationFault::AfterBackup,
            MigrationFault::AfterTemp,
            MigrationFault::AfterCheckpoint,
        ] {
            let path = root.join(format!("wal-{fault:?}.sqlite3"));
            fs::copy(&base, &path).unwrap();
            let writer = Connection::open(&path).unwrap();
            writer.pragma_update(None, "journal_mode", "WAL").unwrap();
            writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
            writer
                .execute(
                    "UPDATE tasks SET payload=json_set(payload,'$.title','wal latest title') WHERE json_extract(payload,'$.id')=1",
                    [],
                )
                .unwrap();
            let wal = PathBuf::from(format!("{}-wal", path.display()));
            assert!(wal.exists() && fs::metadata(&wal).unwrap().len() > 0);
            let main_before = file_hash(&path);
            let wal_before = file_hash(&wal);
            assert!(migrate_inner(&path, &root, INCARNATION, fault).is_err());
            // Only the pre-backup failure is required to leave the source bytes
            // untouched. Later stages may checkpoint the source after the
            // verified backup; logical latest state remains authoritative.
            if fault == MigrationFault::AfterBackup {
                assert_eq!(file_hash(&path), main_before);
                assert_eq!(file_hash(&wal), wal_before);
            }
            let latest = crate::sqlite_storage::load(&path, &root).unwrap().unwrap();
            assert_eq!(latest.task(1).unwrap().title, "wal latest title");
            let backups: Vec<_> = fs::read_dir(&root)
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    path.file_name()
                        .unwrap()
                        .to_str()
                        .is_some_and(|name| name.starts_with("before-schema-v4-"))
                        .then_some(path)
                })
                .collect();
            assert!(!backups.is_empty());
            assert_eq!(
                crate::sqlite_storage::load(&backups[0], &root)
                    .unwrap()
                    .unwrap()
                    .task(1)
                    .unwrap()
                    .title,
                "wal latest title"
            );
            drop(writer);
        }
    }

    #[test]
    fn successful_migration_moves_old_sidecars_without_replay() {
        let root = temp_root("wal-sidecars");
        let store = fixture_store(&root);
        let path = root.join("workspace.sqlite3");
        crate::sqlite_storage::save(&path, &root, &store).unwrap();
        let writer = Connection::open(&path).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        writer
            .execute(
                "UPDATE tasks SET payload=json_set(payload,'$.title','v3 wal title') WHERE json_extract(payload,'$.id')=1",
                [],
            )
            .unwrap();
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        assert!(wal.exists());
        migrate_v3_to_v4(&path, &root, INCARNATION).unwrap();
        drop(writer);
        // The checkpoint under the exclusive lock makes the old main file
        // self-contained; any old sidecar is moved aside or truncated and must
        // not be replayed into the new v4 database.
        let mut database = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        assert_eq!(
            database.load_store().unwrap().task(1).unwrap().title,
            "v3 wal title"
        );
        let mut changed = database.load_store().unwrap().task(1).unwrap().clone();
        changed.title = "v4 after migration".into();
        let mut journal = ChangeJournal::new(INCARNATION);
        let group = journal
            .stage(vec![EntityChange::Upsert(EntityPayload::Task(changed))])
            .unwrap();
        database.commit_group(&group).unwrap();
        drop(database);
        let reopened = SqliteV4::open(&path, &root, INCARNATION).unwrap();
        assert_eq!(
            reopened.load_store().unwrap().task(1).unwrap().title,
            "v4 after migration"
        );
    }

    #[test]
    fn migration_faults_leave_original_and_reject_future_or_duplicates() {
        let root = temp_root("migration-fault");
        let store = fixture_store(&root);
        let base = root.join("base.sqlite3");
        crate::sqlite_storage::save(&base, &root, &store).unwrap();
        for fault in [
            MigrationFault::AfterBackup,
            MigrationFault::AfterTemp,
            MigrationFault::AfterCheckpoint,
        ] {
            let path = root.join(format!("{fault:?}.sqlite3"));
            fs::copy(&base, &path).unwrap();
            let before = fs::read(&path).unwrap();
            assert!(migrate_inner(&path, &root, INCARNATION, fault).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(
                crate::sqlite_storage::load(&path, &root).unwrap().unwrap(),
                store
            );
        }
        let future = root.join("future.sqlite3");
        fs::copy(&base, &future).unwrap();
        set_user_version(&future, 99);
        let hash = file_hash(&future);
        assert!(migrate_v3_to_v4(&future, &root, INCARNATION).is_err());
        assert_eq!(file_hash(&future), hash);

        let duplicate = root.join("duplicate.sqlite3");
        let mut duplicate_store = store.clone();
        let mut extra = duplicate_store.all_occurrences()[0].clone();
        extra.id += 1_000;
        extra.trigger_key = duplicate_store.all_occurrences()[0]
            .trigger_key
            .clone();
        duplicate_store.push_occurrence(extra);
        crate::sqlite_storage::save(&duplicate, &root, &duplicate_store).unwrap();
        let duplicate_hash = file_hash(&duplicate);
        assert!(migrate_v3_to_v4(&duplicate, &root, INCARNATION).is_err());
        assert_eq!(file_hash(&duplicate), duplicate_hash);
    }

    #[test]
    fn relocation_codec_preserves_paths_and_stale_signatures() {
        let root = temp_root("relocate");
        let mut store = fixture_store(&root);
        let project_directory = store.projects[0].directory.clone();
        let automation = store.automations()[0].id;
        let signature = format!(
            "[1,\"name\",\"brief\",\"scope\",null,\"{}\",null,null]",
            project_directory
        );
        store.automation_mut(automation).unwrap().plan_binding = Some(AutomationPlanBinding {
            request_id: 1,
            revision: 1,
            project_id: 1,
            input_signature: signature,
        });
        let path = root.join("workspace.sqlite3");
        let database = SqliteV4::create_seeded(&path, &root, INCARNATION, &store).unwrap();
        assert_eq!(database.load_store().unwrap(), store);
        drop(database);
        let moved = root.with_extension("moved");
        fs::rename(&root, &moved).unwrap();
        let relocated = SqliteV4::open(&moved.join("workspace.sqlite3"), &moved, INCARNATION)
            .unwrap()
            .load_store()
            .unwrap();
        assert!(Path::new(&relocated.projects[0].directory).starts_with(&moved));
        let binding = relocated.automation(automation).unwrap().plan_binding.clone().unwrap();
        assert!(binding.input_signature.contains(moved.to_string_lossy().as_ref()));
        let stale = relocated
            .runs()
            .iter()
            .find(|run| run.input_signature == "[\"stale-before-move\"]");
        assert!(stale.is_some());
    }

    fn set_user_version(path: &Path, version: i64) {
        let connection = Connection::open(path).unwrap();
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        let _ = connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        });
    }

    fn file_hash(path: &Path) -> String {
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
    }

    fn file_names(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}


#[cfg(feature = "qa")]
pub fn crash_child() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("missing");
    let path = PathBuf::from(args.get(2).cloned().unwrap_or_default());
    let root = PathBuf::from(args.get(3).cloned().unwrap_or_default());
    let incarnation: u64 = args.get(4).and_then(|value| value.parse().ok()).unwrap_or(51);
    let marker = path.with_extension("marker");
    let fail = |message: String| -> ! {
        eprintln!("v4 crash child {mode}: {message}");
        std::process::exit(2);
    };
    match mode {
        "seed" => {
            let store = crash_fixture(&root);
            SqliteV4::create_seeded(&path, &root, incarnation, &store)
                .unwrap_or_else(|error| fail(error));
        }
        "seed-v3" => {
            let mut store = Store::mock();
            let directory = root.join("project");
            std::fs::create_dir_all(&directory).unwrap();
            store.projects[0].directory = directory.to_string_lossy().into_owned();
            crate::sqlite_storage::save(&path, &root, &store).unwrap_or_else(|error| fail(error));
        }
        "after-commit" => {
            let mut database = SqliteV4::open(&path, &root, incarnation)
                .unwrap_or_else(|error| fail(error));
            let store = database.load_store().unwrap_or_else(|error| fail(error));
            let group = crash_group(&store, database.durable_generation().unwrap_or(0));
            database
                .commit_group(&group)
                .unwrap_or_else(|error| fail(format!("{error:?}")));
            std::fs::write(&marker, b"after-commit").unwrap();
            // Abrupt death immediately after COMMIT, before ack/effects/drop.
            std::process::abort();
        }
        "retry-same" => {
            let mut database = SqliteV4::open(&path, &root, incarnation)
                .unwrap_or_else(|error| fail(error));
            let store = database.load_store().unwrap_or_else(|error| fail(error));
            let group = crash_group(&store, 0);
            match database.commit_group(&group) {
                Ok(CommitOutcome::AlreadyDurable { generation: 1, .. }) => {
                    std::fs::write(&marker, b"retry-ok").unwrap();
                }
                other => fail(format!("retry was not idempotent: {other:?}")),
            }
        }
        "mid-group" => {
            let mut database = SqliteV4::open(&path, &root, incarnation)
                .unwrap_or_else(|error| fail(error));
            let store = database.load_store().unwrap_or_else(|error| fail(error));
            let group = crash_group(&store, database.durable_generation().unwrap_or(0));
            let _ = database.commit_group_with_fault(&group, Fault::CrashMidGroup);
            fail("mid-group fault returned instead of aborting".into());
        }
        "migration-crash" => {
            let _ = migrate_inner(&path, &root, incarnation, MigrationFault::CrashAfterCheckpoint);
            fail("migration crash fault returned instead of aborting".into());
        }
        other => fail(format!("unknown mode {other}")),
    }
}

#[cfg(feature = "qa")]
fn crash_fixture(root: &Path) -> Store {
    use crate::automation::AutomationShape;
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    let mut store = Store::mock();
    let directory = root.join("project");
    std::fs::create_dir_all(&directory).unwrap();
    store.projects[0].directory = directory.to_string_lossy().into_owned();
    store.default_model = "qa/model".into();
    store.research_config.models = std::array::from_fn(|_| "qa/model".into());
    let template = crate::automation_execution::build_quick_template(
        "Check one value.",
        "One answer.",
    )
    .unwrap();
    let automation = store
        .create_automation_draft(
            1,
            AutomationShape::Quick,
            "Crash fixture".into(),
            "Check one value.".into(),
            "One answer.".into(),
            Schedule {
                spec: ScheduleSpec::Interval { minutes: 10 },
                timezone: "UTC".into(),
            },
            Default::default(),
            Some(template),
            1_000,
        )
        .unwrap();
    store
        .approve_and_enable(automation, Vec::new(), 1_000)
        .unwrap();
    let _ = store
        .request_manual_occurrence(automation, "crash-manual", 1_100)
        .unwrap();
    store
}

#[cfg(feature = "qa")]
fn crash_group(store: &Store, _durable: u64) -> StagedGroup {
    use crate::persistence_journal::ChangeJournal;
    let automation = store.automations()[0].id;
    let occurrence = store.all_occurrences()[0].clone();
    let mut definition = store.automation(automation).unwrap().clone();
    definition.cursor = Some(9_999);
    let mut period = occurrence.clone();
    period.unread = true;
    let mut journal = ChangeJournal::new(51);
    journal
        .stage(vec![
            EntityChange::Upsert(EntityPayload::Automation(definition)),
            EntityChange::Upsert(EntityPayload::Occurrence(period)),
        ])
        .unwrap()
}
