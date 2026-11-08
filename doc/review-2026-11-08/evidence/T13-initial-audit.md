# T13 audit: domain commands and scoped undo

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Task: T13 / F03 and mutation architecture, with T14/T15 enablement
Mode: read-only code audit plus this artifact. No product code, maintained documentation, workspace data, or user-visible state was changed. No test suite, app instance, real workspace, commit, or push was touched.

**Status:** audit complete for a bounded first pass. T13 implementation has **not** started and is **not** accepted. T03 and T04 are accepted only in the reviewed working-tree candidate (`doc/review-2026-11-08/IMPLEMENTATION.md:143-145`, `:180-184`); their tests and semantics are constraints on this work, not proof of T13.

Working-tree line numbers below are from this audit and may drift while other agents edit.

---

## 1. Executive finding

The application has good local failure handling in a few critical paths, but it has **no single command boundary**. A user action is usually a hand-written sequence:

1. clone the whole `Store` for rollback,
2. call one or more `Store` mutation methods,
3. call `persist_if_changed(true)` one or more times,
4. separately mutate `runs`, `queued_runs`, timers, selection, and feedback,
5. show success based on the first write only.

That sequence is repeated in `automation_views.rs`, `automation_runtime.rs`, `research_app.rs`, `archive_app.rs`, and the ordinary edit surfaces. The result is exactly the T13 failure class:

- an automation edit/enable/pause/archive can commit the definition, then fail the second write that cancels queued runs, while still showing success (`automation_views.rs:1330-1382`, `:1384-1460`, `:1495-1517`, `:1671-1692`);
- retry/manual-run can commit the occurrence, then fail while adding the run or the occurrence's `run_ids`, leaving two durable phases (`automation_views.rs:1546-1609`, `automation_runtime.rs:1764-1876`);
- stop can cancel a process and mutate state before checking durability (`automation_runtime.rs:2327-2358`, `automation_views.rs:1611-1625`);
- Undo is still a whole-`Store` snapshot; it replaces the store and says “Last change undone” before any save, and only the automation graph is guarded (`app.rs:633-654`, `:1017-1075`, `:1152-1189`);
- several lifecycle completions (after-run, recovery, read/unread, log cleanup) mutate and persist without a rollback or a success gate (`automation_runtime.rs:2109-2251`, `:2254-2324`, `automation_views.rs:1694-1735`).

The minimal fix is **not** a wrapper around `persist_if_changed`. T13 needs a typed prepared-command boundary that validates and stages on a clone, writes once, returns a receipt only after the write succeeds, and only then executes explicit post-commit effects. Undo then becomes a scoped inverse command through the same gate, not `self.store = self.undo.pop()`.

Three corrections are mandatory and are added in §5.2, §5.5, and §5.6:

1. **No stale replay.** A failed prepared command may not be committed later by assigning its old `staged` store. Background completions, timer ticks, recovery, and auto-retry continue while the save is failing. The pending intent carries a base generation/state stamp and a retained idempotency key. On retry it is revalidated and **reprepared from the current store**; if the command no longer applies to the same target, T13 returns an explicit conflict instead of replacing newer history, cursor, or approval. This is especially concrete for dispatch: `research::prepare_run` allocates the next run id from the current store (`research.rs:798-940`), so replaying a stale run would collide with a newer completion. T14 must inherit the same generation/ack rule.
2. **Stop is an emergency exception.** “No new execution” commands fail closed, but a user Stop must still stop the active external process/session when durable storage is unavailable. It reports `StopRequestedNotDurable`, keeps a retry intent, never claims Saved/Cancelled, suppresses autorestart, and has an explicit recovery policy. This is an intentional asymmetry, not a hole in the fail-closed rule.
3. **Dispatch is its own commit boundary.** Routing `EnqueueOccurrence` through today's `launch_run` keeps the run record and `occurrence.run_ids` in two writes (`automation_runtime.rs:1813-1873`). T13 must extract the existing launch preparation, commit run + occurrence link in one staged write, and only then enqueue the existing execution engine. A wrapper around the two-write path is not acceptance.

No `Store` schema migration is required. Two narrow additions are outside the schema: a transient command base generation, and a best-effort emergency stop journal for crash-safe suppression of autorestart. The existing durable-intent mechanism remains: a saved `Queued` occurrence/run pair is reconciled once by startup recovery (`automation_runtime.rs:2275-2278`). T14/T15 own asynchronous persistence and indexed storage; they must consume the same prepared-commit receipt, base generation, and stop journal.

---

## 2. Current persistence and commit APIs

| API | Location | Behavior | T13 relevance |
| --- | --- | --- | --- |
| `Storage::save(&Store)` | `persistence.rs:222-294` | Validates the file stamp, serializes the whole store, writes SQLite in one transaction (`sqlite_storage.rs:69-148`) or JSON temp-file+rename, then marks the file validated. | One durable write per call. Correct transaction rollback exists, but callers decide how many writes a command uses. |
| `TaskApp::persist_if_changed(force)` | `app.rs:1152-1189` | Reconciles goal approvals, invalidates whole-store undo on automation diff, skips if `store == last_saved`, optionally throttles during active runs, saves whole store, updates `last_saved` or `storage_error`. Returns “recovered from error”, not “committed”. | Not a command gate. Callers must inspect `storage_error`; many do not. |
| `TaskApp::retry_save` | `app.rs:1197-1217` | Saves the current in-memory store. | If a command refused to apply its staged store, there is nothing to retry. A pending prepared commit is needed. |
| `TaskApp::confirm_save` | `app.rs:666-681` | Persists, then shows success only if storage exists and `storage_error` is empty. | The one existing correct “Saved” gate; it is not used by all mutations. |
| `TaskApp::finish` | `app.rs:682-700` | Pops a duplicate undo entry, sets toast/error, notifies. Does **not** persist or inspect `storage_error`. | Many ordinary edits report success before durability. |
| `TaskApp::snapshot` | `app.rs:633-639` | Pushes a full `Store` clone, clears invalidation, caps at 30. | The current undo history is whole-store. |
| `TaskApp::invalidate_undo_for_automation` | `app.rs:645-654` | Clears all undo entries and sets the footer reason. | T04’s conservative guard. Must remain while legacy snapshots exist. |
| `automation_state_differs` | `app.rs:57-92` | Compares automations, occurrences, automation project, automation tasks/topics/requests. | Reusable as a protected-state assertion for scoped undo. It is not scoped itself. |
| Autosave observer | `app.rs:415-419` | Calls `persist_if_changed(false)` on every self-notification. | A pending prepared commit must not be bypassed by this observer. |
| Whole-store equality / `last_saved` | `app.rs:1152-1189` | Used only to skip a save. There is no per-command base revision. | Retrying a stale staged store can overwrite newer runs, cursors, approvals, or completions. |
| SQLite failure injection pattern | `persistence.rs:476-488` | A `BEFORE INSERT` trigger makes the transaction fail and verifies the old state remains. | No new production failure hook is needed for T13 tests. |

The current write is a **whole-store snapshot** on every call. That is acceptable for T13 correctness but is the measured T14/T15 cost. T13 must not add a second persistence format or a second store.

**Missing base stamp:** nothing records “the state this command was prepared from”. `last_saved` is only the last confirmed write, not the command base. Background completions keep mutating `store.runs` and definitions while a save is failing. Therefore a `pending_commit` that later assigns `prepared.staged` wholesale can erase those newer events. The base stamp and reprepare rule in §5.2 are mandatory, not optional.

---

## 3. Automation lifecycle inventory

### 3.1 User-visible command wrappers

| Command | Location | Current sequence | Failure behavior / gap |
| --- | --- | --- | --- |
| Save draft | `automation_views.rs:1263-1328`, `:1330-1382` | Clone rollback; `create_automation_draft` or `revise_automation_with_shape`/`rename_automation`; save; check; `cancel_unstarted_automation_work`; save again; success feedback. | Second save result is never checked (`:1357-1358`). A durable definition can be saved while the run-cancellation change is not, and the UI still says “Saved…”. |
| Enable | `automation_views.rs:1384-1460` | Validate plan binding; clone rollback; save/revise form; `approve_and_enable`; save; check; cancel unstarted work; save again; “Enabled…”. | Same double-write gap (`:1444-1445`). Approval, cursor, and definition are durable, but queue cleanup may not be. |
| Pause | `automation_views.rs:1495-1517` | Clone rollback; `pause_automation` (skips queued occurrences, clears cursor); save; check; cancel unstarted work; save again; “Paused…”. | Same double-write gap (`:1507-1508`). |
| Resume schedule | `automation_views.rs:1519-1540` | Clone rollback; `resume_schedule` (approval check, new cursor); save; check; arm timer; success. | Single write and gate. The effect (timer) runs only after success. This is the model to follow. |
| Retry occurrence | `automation_views.rs:1546-1584` | Blocker check; if active, enqueue without persistence; else clone rollback; `retry_occurrence` (reopen same occurrence); save; check; `enqueue_automation_occurrence`. | Occurrence commit and run dispatch are separate durable phases. The success toast is emitted after enqueue, but enqueue can fail internally and set `error`; feedback is still the success message. |
| Run now | `automation_views.rs:1586-1609` | New request id; clone rollback; `request_manual_occurrence`; save; check; success feedback; enqueue. | Feedback is set to “Manual run queued” before enqueue's possible failure. |
| Stop now | `automation_views.rs:1611-1625` | Find active occurrence; `stop_automation_occurrence`; unconditional “Stopping…” feedback. | `stop_automation_occurrence` mutates, cancels the process, then persists without checking (`automation_runtime.rs:2339-2356`). |
| Cancel period | `automation_views.rs:1629-1645` | Stop occurrence; toast includes `storage_error` if present. | Better than Stop now, but the state is still mutated before durability. |
| Copy | `automation_views.rs:1650-1668` | Clone rollback; `copy_automation`; save; check; open copy. | Single write and gate; okay, but no command type and no undo decision. |
| Archive | `automation_views.rs:1671-1692` | Clone rollback; `archive_automation`; save; check; cancel unstarted work; save again; toast. | Same double-write gap (`:1683-1684`). |
| Toggle read/unread | `automation_views.rs:1694-1706` | Mutate occurrence; save; no check. | Persistence failure leaves in-memory state different and can still show the new state. |
| Cleanup logs | `automation_views.rs:1722-1735` | Mutate runs; save; no check; feedback “completed”. | Destructive effect reported without a durability gate. |
| Plan ready | `automation_views.rs:1204-1259` | UI-state update from an already-completed planner run; save; feedback. | The plan text belongs to completion persistence (T23/T25). Do not let T13 claim the plan is saved based on this UI call. |
| Start planning | `automation_views.rs:1145-1198` | Create request, prepare run, set `planning_run_id`, `launch_run`, repair pointer on failure. | Two-step durable intent/run; coordinate with T23/T25. |

### 3.2 Domain command methods (already reasonably staged)

| Domain method | Location | Current precedence | Notes |
| --- | --- | --- | --- |
| `create_automation_draft` | `automation_execution.rs:339-379` | Validate project, allocate id, build, `validate_draft`, push. | No I/O and no rollback needed if called on a staging clone. |
| `revise_automation_with_shape` | `automation_execution.rs:454-532` | Clone definition to `staged`, validate project, decide plan-binding retention, bump version, clear approval/cursor/anchor, validate, replace, skip queued occurrences. | Good atomic domain shape. This is the method T13 should call, not duplicate. |
| `approve_and_enable` | `automation_execution.rs:573-597` | Validate archived/project, build approval, compute first trigger, then mutate approval/status/anchor/cursor. | Approval and cursor are one unit. Good. |
| `pause_automation` | `automation_execution.rs:599-628` | Validate status, skip queued occurrences, set paused/clear cursor. | Occurrence transitions ignore errors (`let _`), inherited from current behavior. |
| `resume_schedule` | `automation_execution.rs:631-654` | Validate status/approval/project, compute first trigger, set enabled/anchor/cursor. | Must stay separate from retry (T03). |
| `archive_automation` | `automation_execution.rs:659-685` | Idempotent archived check, skip queued occurrences, set archived/clear cursor. | Idempotence is already modeled. |
| `retry_occurrence` | `automation_runtime.rs:555-581` | Validate terminal status, `occurrence_retry_blocker`, then set `Queued`, clear reason, set retry time/deadline. | Preserves identity/reference/snapshot. Do not change. |
| `request_manual_occurrence` | `automation_runtime.rs:282-326` | Deduplicate by trigger key, validate approval/blockers/queued, build and materialize. | Idempotent for a repeated `request_id`. |
| `commit_automation_tick` | `automation_runtime.rs:207-279` | Version/status check, then attention/skip/run mutation; may mutate superseded occurrence and cursor. | Caller `poll_automations` clones/rolls back the whole store (`:1630-1670`). Good local behavior, wrong ownership boundary. |
| `stop_occurrence` | `automation_runtime.rs:374-407` | Transition active occurrence, update definition timestamp, return run ids. | Process cancellation is the caller's effect. |
| `complete_occurrence` / `fail_occurrence` | `automation_runtime.rs:411-486` | Validate final report/goals, set terminal state, update failures. | Called from after-run; caller persists without rollback. |

The domain methods are mostly the right primitives. T13 should **not rewrite them**; it should make them execute against a staged `Store` and commit once.

### 3.3 Runtime and recovery effects

| Path | Location | Current ordering | Consequence |
| --- | --- | --- | --- |
| Scheduled tick | `automation_runtime.rs:1622-1688` | Plan → commit each tick on live store → save once → on failure restore whole-store clone and return → on success enqueue queued occurrences. | Correct fail-closed for the tick. Effects are outside the durable commit, which is the desired shape. |
| Enqueue occurrence | `automation_runtime.rs:1764-1876` | Storage gate; status/active-run checks; expiry/deadline; `prepare_occurrence_run`; `launch_run` (pushes run and saves); on success push `run_id` to occurrence and save again. | Two writes. If the second save fails, the in-memory rollback is to a snapshot taken before `launch_run`, while disk may already contain the queued run. Recovery may reconcile, but the command's success feedback is not tied to the final write. |
| After run | `automation_runtime.rs:2109-2251` | Mutate occurrence/definition; sometimes enqueue next step before the final save; toast “completed” then save with no rollback. | Can report completion before durability. If save fails, in-memory and disk diverge until another save. |
| Recovery | `automation_runtime.rs:2254-2324` | Mutate active occurrences via `reconcile_occurrence`; save once; enqueue only if `storage_error` is empty. | Enqueue gate is correct. No rollback of mutation on save failure. |
| Stop occurrence | `automation_runtime.rs:2327-2358` | Mutate occurrence, cancel active/queued runs, remove queued entries, save. | Process side effect precedes durability. |
| Timeout/expiry | `automation_runtime.rs:1912-1986` | Cancel runs, transition, note failure, save. | Same shape; no rollback. |
| Auto-retry | `research_app.rs:624-680` | Finalize, save, prepare retry, push run, save, check, then enqueue; on failure pop and rely on error. | Two writes and a special-case pop. |
| Run finalization | `research_app.rs:519-623`, `:624-680` | Mutate run/result/tasks, save, then automation continuation. | Result persistence precedes external side effects, which is correct, but it is not a typed command. |

### 3.4 Undo

Current undo is a `Vec<Store>` (`app.rs:186`) with:

- `snapshot()` before edits (`app.rs:633-639`; call sites listed in §6);
- `invalidate_undo_for_automation()` at every durable automation diff (`app.rs:645-654`, invoked from `persist_if_changed` at `:1157-1159`);
- `undo_change()` rejects while runs/queued/batch work exists, pops a snapshot, rejects a whole snapshot whose automation graph differs, preserves `runs`, `publications`, `report_publishing`, and monotonic `next_id`, then assigns the store and says “Last change undone” (`app.rs:1017-1075`);
- footer reason from `undo_invalidation` (`views.rs:563-570`).

This passes the accepted T04 cases (`automation_qa.rs:3056-3314`), including pause, enable, scheduled cursor/occurrence, and direct Undo before persistence. It is still a whole-store restore. It cannot satisfy T13's “use inverse user edits or explicit non-undoable commands rather than whole-Store restoration”, and `undo_change` does not persist before claiming the change was applied.

`reconcile_goal_approvals` is called at the start of every persist (`research.rs:407-415`; `app.rs:1153`). Any scoped inverse that touches a research goal's stage/approval must run this reconciliation on the staged store so the inverse is not immediately reverted or allowed to bypass the approval rule.

### 3.5 Fail-closed versus fail-open today

**Already fail-closed (preserve):**

- `poll_automations` restores the whole store and suppresses dispatch on tick/save failure (`automation_runtime.rs:1654-1670`).
- `launch_run` removes the just-pushed run if the save fails (`app.rs:1542-1547`).
- `save_automation_draft`, `enable_automation`, `pause_automation_ui`, `resume_automation_schedule`, `retry_automation_occurrence`, and `run_automation_now` check the first save and roll back.
- `enqueue_automation_occurrence` returns when storage is absent or already failed (`automation_runtime.rs:1769`) and rolls back the run push on save failure (`:1851-1858`).
- `recover_automations` enqueues only when `storage_error` is empty (`:2316-2321`).
- `confirm_save` and `retry_save` check durability before success.

**Fail-open or partial (must be removed by T13):**

- second saves after queue cleanup are unchecked (`automation_views.rs:1357-1358`, `:1444-1445`, `:1507-1508`, `:1683-1684`);
- stop now reports success before durability (`automation_views.rs:1620-1624`);
- read/unread and log cleanup save without checking (`:1704`, `:1725`);
- after-run can toast completion and save without rollback (`automation_runtime.rs:2158-2163`);
- recovery mutates before save and does not restore (`:2293-2316`);
- Undo applies and toasts before persistence (`app.rs:1039-1073`);
- `finish` reports an action outcome without saving (`app.rs:682-700`).

---

## 4. Concrete gaps against T13 acceptance

| ID | Gap | Evidence | Acceptance impact |
| --- | --- | --- | --- |
| G1 | A single user command performs two durable writes; the second is unchecked and success is shown. | `automation_views.rs:1330-1382`, `:1384-1460`, `:1495-1517`, `:1671-1692` | “Persistence failure after preparation cannot claim Saved or partially enable scheduling.” Violated for queue cleanup. |
| G2 | Occurrence commit and run dispatch are separate durable phases with different rollback points. | `automation_views.rs:1546-1609`; `automation_runtime.rs:1764-1876` | “Cannot launch work after failure / on retry.” The process does not start after a failed save, but durable state can be split and success feedback can precede the second write. |
| G3 | Stop mutates and cancels before checking durability; Stop now has unconditional success feedback. | `automation_runtime.rs:2339-2356`; `automation_views.rs:1611-1625` | “Cannot claim Saved” and “failure cannot launch work” are not enforced for stop. |
| G4 | Undo restores a whole `Store` and reports success before persistence; only the automation graph is guarded. | `app.rs:1017-1075` | “Undo cannot alter history/approval/cursor” is protected for automation today but not structurally; “use inverse user edits” is not met. |
| G5 | Lifecycle completions mutate and persist without rollback or success gating. | `automation_runtime.rs:2109-2251`, `:2254-2324`; `automation_views.rs:1694-1735` | “Persistence failure cannot claim Saved” is violated for completion/read/cleanup/recovery. |
| G6 | Idempotence is scattered in runtime helpers, not owned by a command. | `automation_runtime.rs:282-326`, `:555-581`, `:1764-1798`, `:606-672` | Commands are not idempotent by construction; retries depend on incidental status checks. |
| G7 | Post-commit effects are duplicated in UI, scheduler, recovery, after-run, and auto-retry. | `automation_views.rs:1575`, `:1604`; `automation_runtime.rs:1672-1679`, `:2178`, `:2318-2320` | No single place to guarantee “persist before enqueue/timer/stop”. |
| G8 | Ordinary edits use whole-store snapshots, and `finish`/`confirm_save` disagree on durability. | `app.rs:633-639`, `:682-700`; call sites in §6 | Ordinary edits can be reported as applied before save; undo can roll back state that is not an inverse of the named edit. |
| G9 | Design currently promises whole-store Undo protection. | `DESIGN.md:27`, `:43`; `PRODUCT.md:36-38` | T13's scoped undo changes the published contract; T17 must reconcile it in the same delivery wave. Do not silently reinterpret. |
| G10 | No typed owner/effect receipt exists for T14/T15. | `app.rs:1152-1189`; `persistence.rs:222-294` | T14 cannot order acknowledgements or distinguish pending/durable/failed without reworking every caller. |

**Not a finding:** the existing scheduler tick already demonstrates the right pattern (stage/commit, then effects) and should be generalized, not replaced.

---

## 5. Proposed minimal command boundary

Keep the existing `Store` methods and the existing execution engine. Add a thin, non-GPUI command layer and a thin GPUI executor.

### 5.1 New domain-only module: `src/domain_command.rs`

```rust
// No gpui types. Pure Store preparation, effect descriptions, and scoped undo.
pub(crate) struct AutomationDraftInput {
    pub project: u64,
    pub shape: crate::automation::AutomationShape,
    pub name: String,
    pub objective: String,
    pub expected_result: String,
    pub schedule: crate::automation_schedule::Schedule,
    pub policy: crate::automation::AutomationPolicy,
    pub template: Option<crate::automation::AutomationTemplate>,
}

pub(crate) enum AutomationCommand {
    SaveDraft { id: Option<u64>, input: AutomationDraftInput, now: i64 },
    Enable { id: Option<u64>, input: AutomationDraftInput,
             materials: Vec<crate::automation::AutomationMaterial>, now: i64 },
    Pause { id: u64, now: i64 },
    ResumeSchedule { id: u64, now: i64 },
    RetryOccurrence { occurrence: u64, now: i64 },
    RunNow { automation: u64, request_id: String, now: i64 },
    StopOccurrence { occurrence: u64, now: i64 },
    Archive { id: u64, now: i64 },
    Copy { id: u64, now: i64 },
    SetRead { occurrence: u64, unread: bool },
    CleanupLogs { id: u64, now: i64, apply: bool },
    Rename { id: u64, name: String, now: i64 },
}

pub(crate) enum EditCommand {
    TaskDetails { id: u64, title: String, description: String,
                  labels: Vec<String>, due_date: String },
    TaskStage { id: u64, stage: crate::domain::Stage },
    Comment { id: u64, text: String },
    ProjectName { id: u64, name: String },
    ProjectDirectory { id: u64, directory: String },
    TopicCreate { project: u64, name: String },
    TopicRename { id: u64, name: String },
    TopicArchived { id: u64, archived: bool },
    TopicMove { id: u64, project: u64 },
    GoalTopic { id: u64, topic: Option<u64> },
    TaskArchived { id: u64, archived: bool },
    ReopenStep { id: u64 },
    AddMaterials { id: u64, attachments: Vec<crate::attachments::Attachment> },
    RemoveMaterial { id: u64, index: usize },
    Settings(SettingsEdit),
}

pub(crate) enum Command {
    Automation(AutomationCommand),
    Edit(EditCommand),
    Undo(UndoEntry),
}

/// Stable identity retained across a failed save and a reprepare. It is the
/// idempotency key, not a store revision.
pub(crate) struct CommandId {
    pub kind: &'static str,
    pub target: u64,
    pub operation_key: String, // request id, occurrence id, definition version, undo target
}

/// State the command was prepared from. T13 baseline: exact Store equality.
/// T14 may replace `exact` with a monotonic generation + digest, but must keep
/// the same conflict rule.
pub(crate) struct CommandBase {
    pub generation: u64,
    pub exact: Box<crate::domain::Store>,
}

pub(crate) struct PreparedCommand {
    pub id: CommandId,
    pub class: CommandClass,
    pub command: Command,           // retained for reprepare
    pub base: CommandBase,
    pub staged: crate::domain::Store,
    pub outcome: CommandOutcome,
}

pub(crate) struct CommandOutcome {
    pub effects: Vec<CommandEffect>,
    pub undo: UndoDecision,
    pub feedback: Option<CommandFeedback>,
}

pub(crate) enum CommandClass {
    StartOrExtend,        // fail closed: no commit means no effect
    OrdinaryEdit,         // fail closed; inverse is recorded only after commit
    StopExternalWork,     // emergency exception in §5.5
}

pub(crate) enum CommandEffect {
    EnqueueOccurrence(u64),            // existing enqueue_automation_occurrence
    QueueRun { run_id: u64 },          // existing scheduler queue entry
    RemoveQueuedRuns(Vec<u64>),        // in-memory queue only; run status already staged
    StopRuns(Vec<u64>),                // existing cancel_run_id
    ArmAutomationTimer,
    Notice(CommandFeedback),
}

pub(crate) enum UndoDecision {
    NotUndoable(&'static str),
    Record(UndoEntry),
}

impl Store {
    /// Clone self, apply exactly one command, validate, reconcile approvals,
    /// compute effects and the scoped undo entry. Never writes and never starts work.
    pub(crate) fn prepare_command(
        &self,
        id: CommandId,
        class: CommandClass,
        command: Command,
    ) -> Result<PreparedCommand, String> { /* dispatch to existing methods */ }
}
```

`SettingsEdit`, `SettingsSnapshot`, and `CommandFeedback` are small supporting types: `SettingsEdit` names the validated configuration change, `SettingsSnapshot` captures only the configuration fields that edit can touch, and `CommandFeedback` is a neutral `{tone, message}` value produced on commit. They must not contain runs, executions, or navigation state.

`prepare_command` must delegate to the existing domain methods in §3.2. It must not reimplement schedule math, approval hashing, retry blockers, or template validation. `SaveDraft`/`Enable` keep the current “signature unchanged → rename only” rule; move the pure `definition_form_signature` helper out of `automation_views.rs` only if needed, otherwise pass the already-computed decision from the UI as a `Rename` command after a successful `SaveDraft`.

### 5.2 Commit gate and receipt

```rust
pub(crate) trait CommitSink {
    fn save(&self, store: &Store) -> Result<(), String>;
}

pub(crate) struct CommandReceipt {
    pub effects: Vec<CommandEffect>,
    pub undo: UndoDecision,
    pub feedback: Option<CommandFeedback>,
}

pub(crate) enum CommitError {
    Rejected(String),             // validation/prepare failed; live store untouched
    Stale { message: String, pending: PreparedCommand },      // base changed; no write
    NotDurable { message: String, pending: PreparedCommand }, // write failed; no live assignment
}

pub(crate) fn commit_prepared(
    sink: &dyn CommitSink,
    live: &mut Store,
    last_saved: &mut Store,
    prepared: PreparedCommand,
) -> Result<CommandReceipt, CommitError> {
    if !prepared.base.matches(live) {
        return Err(CommitError::Stale {
            message: "The workspace changed after this command was prepared.".into(),
            pending: prepared,
        });
    }
    sink.save(&prepared.staged)
        .map_err(|message| CommitError::NotDurable { message, pending: prepared })?;
    // Re-check before assignment so a concurrent/pipelined change cannot be
    // overwritten between the save and the in-memory swap. T14 must enforce the
    // same rule across the worker acknowledgement.
    if !prepared.base.matches(live) {
        return Err(CommitError::Stale {
            message: "The workspace changed while this command was being saved.".into(),
            pending: prepared,
        });
    }
    *live = prepared.staged;
    *last_saved = live.clone();
    Ok(CommandReceipt { /* move effects/undo/feedback out */ })
}
```

Rules:

1. **One write per command.** The current whole-store `Storage::save` is the sink for T13. T14 replaces the sink implementation with the ordered worker; it does not change callers. A command that uses two writes is not a T13 command (§5.6).
2. **No live mutation before success.** `prepare_command` works on a clone. `commit_prepared` assigns only after `save` returns `Ok` and the base still matches.
3. **No effect before success.** Effects are returned in the receipt. `TaskApp` executes them only after the receipt.
4. **No “Saved”/“Enabled”/“Paused”/“Retried”/“Cancelled” before the receipt.** Feedback is part of the receipt or is produced by the effect executor only on success.
5. **Pending retry never replays a stale store.**
   - On `NotDurable`, keep `PreparedCommand` in `TaskApp::pending_commit`. Do **not** assign `staged` and do **not** run effects.
   - On `Stale`, keep the typed command/idempotency key and the pending record; do not silently discard the user's intent.
   - `retry_save` first checks `pending.base.matches(&self.store)`.
     - If it matches, retry the same prepared commit once.
     - If it does not match, call `prepare_command(pending.id.clone(), pending.class, pending.command.clone())` against the **current** store. If that succeeds and still describes the same target/intent, commit the newly staged result. If it fails, returns a different `CommandId`, or the target was replaced/removed, return an explicit `Conflict` (“The workspace changed; review this change before applying it again”).
   - Never assign the old `pending.staged` after a base mismatch. That is the wholesale stale replacement this rule forbids.
   - A new command while pending either retries the pending command first or is rejected with “Save or discard the previous change first”; it must not merge two prepared bases.
6. **Reprepare policy by command.**
   - `RetryOccurrence`: reprepare only if the occurrence is still Failed/Cancelled/Interrupted; if it has since succeeded or is already queued by a newer action, return `Noop`/`Conflict`, never reopen it.
   - `RunNow`: reuse the retained `request_id`; if the same request already produced an occurrence, return `Noop`. A new request id is a different command.
   - `DispatchOccurrence`/`QueueRun`: reprepare the run from the current store because run ids are allocated from current `runs` (`research.rs:798-940`). If the occurrence is no longer queued or already has an active/queued run, return `Noop`; if it is terminal, `Conflict`.
   - `SaveDraft`/`Enable`: reprepare only against the definition version the user reviewed. If the version moved, `Conflict`; never approve a newer version implicitly.
   - `Pause`/`Archive`: reprepare idempotently from the current definition; if already paused/archived, `Noop`.
   - `Undo`: reprepare the inverse only if the current target still equals the recorded `after` state. Otherwise `Conflict`; never undo a newer edit or a newer runtime event.
7. **Effects are idempotent.** Use existing keys and checks:
   - occurrence dispatch: `occurrence.id` + active/queued-run check in `enqueue_automation_occurrence` (`automation_runtime.rs:1785-1798`);
   - manual run: `manual_trigger_key` (`automation.rs:658-664`) and `request_manual_occurrence` dedupe (`automation_runtime.rs:288-291`);
   - scheduled materialization: `scheduled_trigger_key` uniqueness (`automation_runtime.rs:606-672`, `materialize_occurrence` at `automation_execution.rs:830-838`);
   - run dispatch: `run.id` plus occurrence link; `QueueRun` only adds a run that is already in the committed store;
   - stop/cancel: filter by terminal/active status; `cancel_run_id` only for live runs;
   - timer arm: `arm_automation_timer` is safe to repeat (`automation_runtime.rs:1579-1614`).
8. **One execution engine.** `CommandEffect::QueueRun` uses the existing scheduler queue and pump; `CommandEffect::EnqueueOccurrence` uses the extracted launch preparation from §5.6. No new `AgentRun` construction, process spawn, or second scheduler.
9. **No navigation in effects.** Effects never change `Scope`, selection, dialogs, drafts, or scroll. Explicit UI handlers may navigate after a successful receipt, per T02.
10. **T14 inherits the invariant.** Every asynchronous save carries `CommandBase.generation`. An acknowledgement whose base/generation is older than the current store must not replace in-memory state; the app either reprepares the retained command or reports conflict. The worker may coalesce and reorder only when it preserves this rule.

### 5.3 Scoped undo model

```rust
pub(crate) enum UndoEntry {
    TaskFields { id: u64, before: TaskFields, after: TaskFields },
    TaskStage { id: u64, before: Stage, after: Stage,
                before_approved: Option<String>, after_approved: Option<String> },
    Comment { id: u64, index: usize, body: String },
    ProjectName { id: u64, before: String, after: String },
    ProjectDirectory { id: u64, before: String, after: String },
    TopicCreated { id: u64, name: String, project: u64 },
    TopicName { id: u64, before: String, after: String },
    TopicArchived { id: u64, before: bool, after: bool },
    TopicProject { id: u64, before: u64, after: u64 },
    GoalTopic { id: u64, before: Option<u64>, after: Option<u64> },
    TaskArchived { id: u64, before: bool, after: bool },
    Materials { id: u64, before: Vec<Attachment>, after: Vec<Attachment> },
    Settings { before: Box<SettingsSnapshot>, after: Box<SettingsSnapshot> },
}

pub(crate) struct TaskFields {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub cycle: Option<String>,
    pub due_date: String,
}
```

Inverse semantics:

- `UndoEntry` is itself converted to the corresponding `EditCommand` and passed through `prepare_command`/`commit_prepared`; there is no second mutation path.
- On inverse application, append an `activity_event("Undo: …")` instead of truncating activity history. Do not remove unrelated activity entries.
- `TaskStage` stores both stage and `approved_plan` before/after so the `set_stage` side effect (`domain.rs:1021-1047`) is inverted consistently. Then run `reconcile_goal_approvals` on the staged store.
- `TopicCreated` inverse may remove the topic only if it is unchanged, has no tasks/requests, and is not automation-owned; otherwise return a non-undoable error and keep the entry.
- `Materials` inverses adjust metadata only. Files copied to the data directory are not deleted by Undo; that is an explicit non-undoable external side effect (existing whole-store undo also left files behind).
- `Settings` inverses restore only the configuration fields that the edit changed. Never restore `runs`, `automations`, `automation_occurrences`, `research_requests`, `publications`, `report_publishing`, or `next_id`.
- After applying an inverse, assert the protected graph is unchanged:
  - `automations`, `automation_occurrences`, `runs`, `research_requests`, `publications`, `report_publishing`;
  - automation-owned `projects`/`tasks`/`topics`.
  - Reuse `automation_state_differs` and add the run/request/publication comparisons. If the assertion fails, return a command error and do not commit. This is a structural guarantee, not review discipline.
- Push the `UndoEntry` only after the original command's commit succeeds. Undo of an undo is not required; redo is out of scope.
- Keep T04's conservative invalidation for any **legacy** `Vec<Store>` entry during migration. Once all snapshot call sites are migrated, delete `snapshot`, `undo: Vec<Store>`, `automation_state_differs` as an undo-rejection trigger (retain it as the protected-state assertion), and the invalidation reason. Until then, do not remove T04 tests.

### 5.4 How this preserves T03/T04

- `RetryOccurrence` calls the existing `retry_occurrence`; it never calls `resume_schedule`. The occurrence keeps its identity, reference time, deadline semantics, approved snapshot, finished steps, and paused/`NeedsAttention` definition. Existing T03 tests (`automation_runtime.rs:1140-1331`) remain unchanged.
- `ResumeSchedule` remains a separate command with its own approval/project/cursor validation. It never reopens history.
- `Enable` remains one staged revision + approval + cursor mutation followed by one commit; it preserves T09's shape/template/version atomicity and T03's “new definition edit does not rewrite an old occurrence”.
- Scoped undo never writes the automation graph, so T04's accepted cases continue to hold. The stale-snapshot guard remains during migration and becomes defense in depth afterward.
- Scheduler tick, recovery, and after-run continue to use the same domain methods and the same concurrency pools; T13 changes ownership/order, not execution semantics.

---

## 6. Ordinary edit undo classification

Current snapshot call sites (whole-store) and the required decision:

| Edit type | Current site | Decision | Scoped inverse / reason |
| --- | --- | --- | --- |
| Task title/description/labels/cycle/due | `app.rs:992-1015` | Undoable | `TaskFields` before/after. |
| Manual task stage | `app.rs:909-911` | Undoable with approval capture | `TaskStage` including `approved_plan`; revalidate transition on inverse. |
| Comment | `views.rs:2795-2805` | Undoable if unchanged | Remove the exact appended comment (index + body guard); if it was edited/deleted, non-undoable. |
| Project rename | `project_views.rs:30-37` | Undoable | `ProjectName`. |
| Project folder | `app.rs:1271-1282` | Undoable | `ProjectDirectory`; revalidate containment. |
| Topic create | `topic_views.rs:149` | Undoable with guard | `TopicCreated`; refuse if referenced/automation-owned. |
| Topic rename | `topic_views.rs:149` | Undoable | `TopicName`. |
| Topic archive/restore | `archive_app.rs:70-100`, `topic_views.rs:550` | Undoable if no work was stopped | `TopicArchived`; archive that cancels active work is non-undoable because process/history effects cannot be reversed. |
| Topic move | `topic_views.rs:50-65` | Undoable | `TopicProject`; revalidate move blockers. |
| Goal/topic reassignment | `topic_views.rs:244-254` | Undoable | `GoalTopic`; revalidate dependencies. |
| Task archive | `archive_app.rs:70-100` | Undoable if clean; non-undoable with active work | `TaskArchived`; clean archive only. |
| Step reopen | `goal_steps.rs:184-190` | Undoable | Inverse stage to the previous stage; revalidate. |
| Add materials | `research_app.rs:364-383` | Metadata undoable; file copy non-undoable | `Materials` before/after; do not delete copied bytes. |
| Remove material | `research_app.rs:316-325` | Undoable | `Materials`; file still exists. |
| Model agent/model | `research_app.rs:41-123` | Undoable | `Settings` config snapshot only. |
| Use global / all stages | `research_app.rs:70-79`, `research_views.rs:221-235` | Undoable | `Settings`. |
| Profiles create/apply/update/delete/restore | `settings_views.rs:56-142` | Undoable | `Settings` (profiles + active profile). |
| Research/automation concurrency | `settings_views.rs:152-168` | Undoable | `Settings` limits. |
| Appearance/theme | settings/appearance paths | Undoable | `Settings.appearance`; reapply theme after commit. |
| Automation save/enable/pause/resume/retry/run now/stop/archive/copy | `automation_views.rs` | **Non-undoable** | Durable lifecycle, approvals, cursors, occurrence identity, and queue effects. Show “Automation changes cannot be undone; they are saved as a separate operation.” |
| Read/unread | `automation_views.rs:1694-1706` | Non-undoable | Runtime presentation state. |
| Log cleanup | `automation_views.rs:1722-1735` | Non-undoable | Destructive and not fully reversible. |
| Research request create/replan/plan apply | `project_app.rs:60-101`, `request_planning.rs` | Non-undoable | Creates durable intent/planning identity and runs; current code already clears undo. |
| Run submission/completion/recovery | `app.rs:1549`, `research_app.rs` | Non-undoable | Clears undo today; runtime history must not roll back. |
| Publication/export | publication paths | Non-undoable | External side effect. |
| Automation-owned task/topic edits | any path | **Forbidden/redirected** | They belong to the automation lifecycle; do not create ordinary undo entries. |

Rule: an edit that cannot produce a complete, validated inverse is `NotUndoable` with a user-visible reason. It is never approximated by a partial field restore.

---

## 7. Implementation slices

Each slice is independently executable and leaves the app buildable. Do not start slice N+1 until slice N's tests pass.

### S0 — Types and pure preparation (no UI adoption)

- Add `src/domain_command.rs` with `AutomationCommand`, `EditCommand`, `Command`, `PreparedCommand`, `CommandOutcome`, `CommandEffect`, `UndoDecision`, `UndoEntry`, and `Store::prepare_command`.
- Implement only delegation to existing `Store` methods. No persistence, no GPUI.
- Add unit tests for prepare-only behavior and inverse round trips.
- Add module to `src/main.rs`.

Exit: new tests pass; no existing behavior changes.

### S1 — Automation lifecycle through the gate

- Add `src/command_app.rs` with `TaskApp::execute_command`, `commit_prepared` using `Storage::save`, `pending_commit`, and effect execution.
- Migrate: save draft, enable, pause, resume, retry, run now, stop, archive, copy, read/unread, cleanup.
- Delete the double `persist_if_changed` sequence and the unconditional success feedback.
- Move queued-run cancellation for edit/pause/archive/stop into the staged command (`Store` mutation) and emit only `RemoveQueuedRuns`/`StopRuns` as effects.
- Keep `enqueue_automation_occurrence` as the only dispatch effect.

Exit: every automation user command is one prepare + one commit + effects; no direct `persist_if_changed` remains in these wrappers.

### S2 — Scheduler, recovery, after-run, auto-retry

- Route `poll_automations`, `enqueue_automation_occurrence`, `automation_after_run`, `recover_automations`, `stop_automation_occurrence`, timeout/expiry, and auto-retry through the same prepared/commit/effect shape.
- Preserve the tick's all-or-nothing behavior and the recovery “queued intent re-enqueues once” rule.
- Do not add a second durable journal; a committed `Queued` occurrence is the intent.

Exit: all lifecycle writes use one gate; no toast/feedback precedes durability; crash-after-save-before-effect test passes.

### S3 — Scoped undo core

- Replace `undo: Vec<Store>` with `undo: Vec<UndoEntry>` and `pending_undo` if needed.
- Implement inverse commands and the protected-state assertion.
- Migrate task details, stage, comment, project rename/folder, topic create/rename/archive/move, goal topic, step reopen first.
- Keep T04 invalidation only for any remaining legacy snapshot call sites.

Exit: these edits round-trip through inverse commands; no whole-store assignment.

### S4 — Remaining ordinary edits and settings

- Migrate archive, materials, model/profile/settings/concurrency/appearance.
- Decide clean-vs-active archive undoability and implement the guard.
- Remove legacy `snapshot()` call sites and `self.store = self.undo.pop()`.

Exit: zero `Vec<Store>` undo entries; every ordinary edit is either a scoped inverse or explicitly non-undoable.

### S5 — Remove legacy guard and reconcile documentation

- Delete `snapshot`, whole-store undo rejection, and the old invalidation message path.
- Retain `automation_state_differs` only as the protected-state assertion.
- Remove dead call sites and stale comments.
- Hand the DESIGN/PRODUCT/comment updates to T17; do not edit them in T13.

Exit: T13 acceptance matrix passes; T04 regression tests still pass with the new guard.

---

## 8. Failure-injection and regression matrix

Use a real SQLite workspace and the existing trigger technique (`persistence.rs:476-488`). No production failure flag is required. For “after preparation”, call `Store::prepare_command` directly and assert the live store and effects are untouched. For “on retry”, drop the trigger and retry the same command/pending commit.

| # | Level | Scenario | Injection | Required assertions |
| --- | --- | --- | --- | --- |
| 1 | Domain unit | Prepare an enable command | None | Returns a staged store with approval/status/cursor; live store unchanged; no effects executed. |
| 2 | Domain unit | Prepare fails validation | Invalid project/archived/template | `Rejected`; live store byte-for-byte unchanged; no effects. |
| 3 | App + SQLite | Enable save fails before commit | `BEFORE INSERT ON automations` trigger | Definition remains Draft; no approval; cursor `None`; no occurrence; no enqueue; no “Enabled” feedback; `pending_commit` holds the prepared command. |
| 4 | App + SQLite | Pause save fails | Trigger | Status/cursor/queued occurrences unchanged; no success toast; queued runs not removed from the live queue. |
| 5 | App + SQLite | Save draft save fails | Trigger | No new definition/version; no “Saved”/“Draft saved”; existing draft unchanged. |
| 6 | App + SQLite | Archive save fails | Trigger | Status/history/cursor unchanged; no success toast; no process cancellation effect. |
| 7 | App + SQLite | Retry save fails | Trigger | Occurrence stays Failed/Cancelled; definition untouched; no run/queue entry; no “Retrying” toast. |
| 8 | App + SQLite | Manual run save fails | Trigger | No occurrence, no run, cursor unchanged; no “Manual run queued”. |
| 9 | App + SQLite | Retry after storage recovery | Drop trigger, call `retry_save` | Exactly one commit; one occurrence reopens; one run is eventually queued; no duplicate occurrence. |
| 10 | App + SQLite | Save succeeds, effect fails validation | Valid save, then force `prepare_occurrence_run` failure | Occurrence remains durable in a non-running state; attention/error recorded; no duplicate run; retry effect is idempotent. |
| 11 | App + SQLite | Crash after save before effect | Commit a `Queued` occurrence, drop the app, reload | Startup recovery enqueues once; second poll produces no second run; trigger key/dedupe intact. |
| 12 | App + SQLite | Double activation | Call pause/enable/retry/run-now twice | Second call is a no-op or explicit error; one approval/cursor/occurrence/run; no double cancellation. |
| 13 | App + SQLite | Scheduler tick save fails | Trigger during `poll_automations` | No occurrence materialized, cursor unchanged, no enqueue, timer re-armed with backoff, error shown. |
| 14 | App + SQLite | After-run save fails | Trigger at completion | No “completed/unread” success claim; in-memory state not silently treated as durable; retry/save path remains available. |
| 15 | App + SQLite | Recovery save fails | Trigger during `recover_automations` | No enqueue; no success state claim; after recovery, queued intent dispatches once. |
| 16 | App + SQLite | Undo commit fails | Trigger during inverse | Live store unchanged; undo entry retained; “Undo not applied · not saved”; retry succeeds once. |
| 17 | App + SQLite | Undo after automation mutation | Scoped ordinary edit → pause/resume/retry → Undo | Protected graph identical before/after; history, approval, cursor, dedupe keys, occurrence identity unchanged. |
| 18 | App + SQLite | Direct Undo before persistence | Edit → mutate automation in memory → Undo | Rejected with reason; automation mutation survives; no partial merge. |
| 19 | Unit | Protected-state assertion | Craft an inverse that touches `runs`/`automations` | `prepare_command` returns an error; commit never happens. |
| 20 | Unit | Ordinary inverse round trip | Each `UndoEntry` variant | Exact before values restored; unrelated fields/activity preserved; activity gets an undo event. |

### 8.1 Ordinary edit tests

- Task details: title, description, labels, cycle, due date; save, undo, verify only fields change.
- Task stage: Todo → Planned → Done/Cancelled with approval side effects; undo restores stage and `approved_plan`; `reconcile_goal_approvals` remains consistent.
- Comment: append then undo removes exactly that comment; a later edited/deleted comment makes undo non-destructive.
- Project rename/folder: undo restores name/directory; invalid inverse fails without mutation.
- Topic create/rename/archive/restore/move: undo round trips; created-topic inverse refuses when referenced; archive with running work is non-undoable.
- Goal topic reassignment: undo restores the previous topic and dependency validity.
- Step reopen: undo restores the previous stage without starting work.
- Materials add/remove: metadata returns to before; copied files are not deleted.
- Models/profiles/concurrency/appearance: undo restores only settings; no run/history change.
- Automation commands: every one is `NotUndoable` with a clear reason and no entry added.
- Run submission/completion: undo stack is cleared, not rolled back.

### 8.2 Existing tests that must keep passing

- `automation_runtime.rs:1024-1139` (manual dedupe, pause/edit/archive cleanup).
- `automation_runtime.rs:1140-1331` (T03 retry scope, snapshot, archived/ownership, resume does not retry).
- `automation_execution.rs:2174` (verified recovery resumes schedule but never user pause).
- `automation_qa.rs:1881` (retry scope) and `:3056-3314` (T04 Undo protection).
- T09 shape/template/version tests and the current `automation_qa` draft-default cases.
- `persistence.rs:476-488` (transaction rollback) and `sqlite_storage.rs` upgrade/integrity tests.

---

## 9. Risks, dependencies, and ownership conflicts

| Risk / dependency | Impact | Mitigation |
| --- | --- | --- |
| T14/T15 own persistence. | The commit gate must not bake in synchronous whole-store writes or block an ordered worker. | Keep `CommitSink` and `PreparedCommand` as the seam. T14 replaces the sink/ack mechanism; T15 changes the write/query implementation. Do not change `Storage`/`sqlite_storage` in T13. |
| T05 owns `automation_runtime.rs` and eligibility. | S2 touches the same tick/enqueue/recovery functions. | Ship S0/S1 first; schedule S2 after T05's current edits settle or limit changes to wrapping the existing functions. Do not change eligibility logic. |
| T06 owns result presentation. | Completion feedback and result dialogs may change concurrently. | T13 only gates feedback; do not restyle or restructure result presentation. |
| T23/T25 own planning ownership, history, and schema. | `SaveDraft`/`Enable`/`PlanReady` interact with request revisions and plan bindings. | Keep `PlanReady` out of T13's first slices or make it a thin wrapper. Do not change `research_requests`, `PlanningOwner`, or `AUTOMATION_SCHEMA_VERSION`. |
| T01 owns QA entry points and `automation_qa.rs`. | Adding failure-injection cases to the shared QA suite can conflict. | Put new unit tests in `domain_command.rs`/`command_app.rs`; if a QA entry is required, add a separate `command_qa` module and coordinate the dispatch line with T01. |
| T02 owns background dispatch/navigation. | Effects must never navigate; command success handlers may navigate only for explicit user actions. | Effects contain no `Scope`/selection/dialog writes. Verify with the settings/report/dirty-form/period-dialog boundary tests. |
| T12 owns navigation controller. | Do not add route authority to commands. | Commands return IDs/receipts; UI decides navigation. |
| T17 owns documentation. | DESIGN/PRODUCT currently promise whole-store Undo protection and “Saved only after persistence”. | Do not edit docs in T13; file the exact wording change as a T17 prerequisite. The code must satisfy the design line “Saved appears only after local persistence succeeds” (`DESIGN.md:43`) and “On storage failure… never show success” (`DESIGN.md:57`). |
| Staging a full `Store` per command. | Memory/CPU cost on the 43,200-occurrence fixture. | T13 measures correctness only; T14/T15 measure and remove the cost. Do not add a second partial-store format in T13. |
| `reconcile_goal_approvals` inside prepare. | It can mutate unrelated research goals and interact with undo. | Preserve current behavior (it runs on every persist), but run it on the staged clone and include it in the protected-state review. T21 owns the approval/input contract. |
| Pending commit vs autosave observer. | The observer could save the live store and bypass a failed prepared command. | `persist_if_changed` must not write while `pending_commit` exists; it should retry the pending command or return without touching the store. |
| Two-phase dispatch remains (queue intent then run record). | A crash between them can leave a `Queued` occurrence with no run. | This is already recoverable by `recover_automations` (`automation_runtime.rs:2275-2278`). Keep the intent durable, make the effect idempotent, and add test #11. Do not pretend it is one SQLite transaction unless T14/T15 explicitly provide one. |
| Error strings and disabled reasons. | UX consistency and English-only rule. | Use `CommandFeedback` for user text; keep strings in `command_app.rs`/UI, not scattered. |
| Removing T04 invalidation too early. | Legacy snapshots could still roll back automation. | Remove only after all `snapshot()` call sites are migrated. Keep `automation_state_differs` as an assertion. |

---

## 10. Exact files to change

### New

- `src/domain_command.rs` — command types, `Store::prepare_command`, inverse types, pure unit tests.
- `src/command_app.rs` — `TaskApp::execute_command`, commit sink adapter, `pending_commit`, effect execution, GPUI-level tests.
- `src/command_qa.rs` (only if the shared QA gate needs a new entry) — end-to-end SQLite failure-injection checks. Coordinate with T01.

### Modify

- `src/main.rs` — register the new modules.
- `src/app.rs` — replace `undo: Vec<Store>` with scoped entries; add `pending_commit`; route `persist_if_changed`, `retry_save`, `finish`, `confirm_save`, and `undo_change` through the gate; remove `snapshot`/whole-store undo after migration. Do not change `launch_run`'s T02 navigation contract.
- `src/automation_views.rs` — wrappers at `:1330-1382`, `:1384-1460`, `:1495-1517`, `:1519-1540`, `:1546-1584`, `:1586-1609`, `:1611-1625`, `:1629-1645`, `:1650-1668`, `:1671-1692`, `:1694-1706`, `:1722-1735`; remove `cancel_unstarted_automation_work` from the UI path or make it a staged domain helper.
- `src/automation_runtime.rs` — S2 only: `poll_automations` (`:1622-1688`), `enqueue_automation_occurrence` (`:1764-1876`), `automation_after_run` (`:2109-2251`), `recover_automations` (`:2254-2324`), `stop_automation_occurrence` (`:2327-2358`), timeout/expiry. Do not change retry/eligibility semantics.
- `src/research_app.rs` — auto-retry (`:624-680`) and finalization ordering; scoped undo for materials (`:316-325`, `:364-383`).
- `src/research_views.rs` — model/settings snapshot (`:221-235`); comment path is in `views.rs`.
- `src/settings_views.rs` — model/profile/concurrency snapshots (`:56-142`, `:152-168`).
- `src/goal_steps.rs` — step reopen inverse (`:184-190`).
- `src/project_views.rs` — project rename inverse (`:30-37`).
- `src/topic_views.rs` — move/create/rename/reassign/restore inverses (`:50-65`, `:149`, `:244-254`, `:550`).
- `src/archive_app.rs` — archive inverse/non-undoable decision and stop-work ordering (`:67-100`).
- `src/views.rs` — comment inverse (`:2795-2805`) and Undo footer reason/tooltips (`:563-570`), only after scoped undo lands.

### Do not modify in T13

- `src/persistence.rs`, `src/sqlite_storage.rs` — T14/T15.
- `src/automation_execution.rs` domain algorithms — wrap only; T03/T09/T25 own their contracts.
- `src/execution_eligibility.rs`, `src/execution_status.rs` — T05.
- `src/result_presentation*.rs` — T06.
- `src/navigation.rs` — T12.
- `src/request_planning.rs`, `src/project_planning.rs` — T23/T25.
- `src/automation_qa.rs` — T01, unless a narrowly coordinated addition is approved.
- `PRODUCT.md`, `DESIGN.md`, `doc/RESEARCH_WORKFLOW_STRATEGY.md` — T17.

---

## 11. Handoff

**Next executable step:** S0 only. Add `src/domain_command.rs` with the types above and pure `prepare_command` delegation plus unit tests; register the module. Do not touch UI or persistence in S0.

**Do not start S1 until:**
- S0 compiles and its tests pass;
- T05's current `automation_runtime.rs` edits are merged or frozen;
- T01's QA file ownership is coordinated.

**Acceptance is not claimed.** This audit does not prove any T13 criterion. T13 remains Open until:
- every automation command is one prepared commit followed by effects;
- every ordinary edit is a scoped inverse or explicitly non-undoable;
- the failure-injection matrix and existing T03/T04/T09 tests pass;
- no whole-store undo assignment remains;
- a reviewer independently verifies the result.

No commit, push, workspace change, app restart, or full-suite run was performed.
