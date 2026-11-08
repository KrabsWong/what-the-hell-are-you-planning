# T13 S1 handoff

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Scope: T13 S1 — app command executor, automation lifecycle durability, emergency stop/journal, scoped edit/undo foundation.
Status: **S1 implemented in the working tree; awaiting root review. T13 is not accepted. S2 is not started.**

---

## 1. Completed boundaries

### 1.1 App command executor

`src/command_app.rs` adds:

- `TaskApp::execute_command(command, cx)`:
  - resolves any pending command first;
  - blocks automation admission while `stop_journal_error` is set;
  - calls `Store::prepare_command`;
  - commits through the existing `Storage::save` via `CommitSink`;
  - executes effects only after a successful receipt.
- `commit_prepared_command`:
  - success → clear storage error, apply effects, return receipt;
  - `NotDurable` → keep the typed command in `pending_commit`/`pending_stop`, set truthful storage error, no effects;
  - `Stale` → keep the typed command pending, no stale-store assignment.
- `retry_pending_saves`:
  - takes the retained typed command and idempotency key;
  - calls `reprepare(pending, &self.store)` against the **current** store;
  - on success commits once and applies effects;
  - on conflict reports it and does not replay the old staged store.
- `persist_if_changed` no longer globally suppresses background saves while a command is pending. Background completion state may be saved; retry reprepares from current state.
- `retry_save` resolves pending commands/stop intents before legacy save.
- `pending_commit`, `pending_stop`, `stop_intents`, `stop_journal_error` are narrow `TaskApp` fields.

### 1.2 Lifecycle handlers adopted

These handler bodies in `automation_views.rs` now build a typed command and call the executor:

- save draft
- enable
- pause
- resume schedule
- retry period
- run manually
- stop / cancel period (emergency path)
- archive
- copy
- read/unread
- log cleanup apply

Removed from these paths:

- double `persist_if_changed` writes;
- success feedback before a durable receipt;
- rollback-by-store-clone for these commands;
- UI-side cancellation of queued runs.

Persistent queued-run cancellation (run status/ended time/log) is now part of the staged store in `domain_command.rs`; only `queued_runs` removal/process effects run after commit.

### 1.3 Domain command extensions

`src/domain_command.rs` now includes:

- `AutomationDraftInput` and form-signature comparison.
- `Command::SaveDraft`, `Enable`, `AutomationPause`, `ResumeSchedule`, `RetryOccurrence`, `RunNow`, `StopOccurrence`, `Archive`, `Copy`, `SetRead`, `CleanupLogs`.
- `CommandOutcome.subject` for the created/updated definition, occurrence or copy.
- `CommandEffect::EnqueueOccurrence`, `ArmAutomationTimer`, `RemoveQueuedRuns`, `StopRuns`, `QueueRun`, `Notice`.
- `save_draft_on` delegates to existing `create_automation_draft` / `revise_automation_with_shape` / `rename_automation`; `enable` delegates to `approve_and_enable`.
- queued-run cancellation helper marks run status/ended/log in the staged store.
- `guard_reprepare` checks definition version/name/status/approval/cursor/anchor and occurrence status as applicable.
- S0 dispatch validation still uses shared `ExecutionEligibility` and rejects `Verifying`, owner/link mismatches, foreign periods, and incoherent queued lifecycles.

### 1.4 Emergency stop and journal

`src/stop_intent.rs`:

- Workspace-local `workspace/stop-intents.json`, version 1, portable occurrence/run ids only.
- Missing file = no records; malformed/unreadable/newer version = diagnostic + fail-closed admission.
- `record`/`clear` use a temp file, `sync_all`, rename, best-effort parent directory sync, and remove the temp on failure.
- Mutation (`read_file`) also rejects newer versions before writing, preserving the original bytes and unknown fields.
- Tests cover round trip, portable ids, malformed/newer files, no temp leak, and future-version mutation rejection.

`src/command_app.rs` stop path:

- `stop_automation_safely`:
  - records an in-memory intent and best-effort journal **before** external stop;
  - calls the existing `cancel_run_id` path even during storage failure;
  - commits the stop command; only a receipt or a matching `last_saved` `Cancelled` state reports `Stopped { Saved }`;
  - in-memory terminal state is never treated as durability;
  - failed save returns `StopRequestedNotDurable`/`NotRecorded` and keeps `pending_stop`/intent;
  - failed external session returns `StopRequestFailed`;
  - an independently terminal durable period returns `AlreadyEnded`.
- `prepare_stop_recovery`:
  - loads the journal **before** `recover_automations` in `attach_loaded_storage`;
  - malformed/newer journal sets `stop_journal_error`, leaves the app viewable, cancels the automation timer, and skips recovery;
  - reconciles every record; a failed save does not hide later records;
  - returns true only when all retained intents are durably cleared.
- `reconcile_loaded_stop_intents` terminal branch:
  - clears only when `last_saved` already contains a terminal state, or after an explicit `save_current_store_durably` success;
  - otherwise retains the journal and intent. A failed first reconciliation followed by another failed retry cannot erase the durable record.
- Per-occurrence admission guard:
  - `enqueue_automation_occurrence` rejects any occurrence listed in `stop_intents`, independent of transient `storage_error`/pending state;
  - `poll_automations`, `enqueue`, and `recover_automations` reject while `stop_journal_error` is set.

### 1.5 Scoped edit/undo foundation

- S0 scoped task-field and task-stage inverses remain.
- Ordinary lifecycle handlers no longer use parallel whole-store mutation paths.
- T04 protection remains in place: `undo: Vec<Store>` and automation-diff invalidation are untouched.
- S3/S4 still need to migrate comments, project/topic, archive, materials, settings and remove the remaining whole-store undo paths.

---

## 2. Verification evidence

### Full unit suite

```text
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet
running 463 tests
test result: ok. 461 passed; 0 failed; 2 ignored
EXIT_STATUS=0
```

### Focused S0/S1 unit tests

```text
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet domain_command
21 passed; 0 failed
EXIT_STATUS=0

PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet stop_intent
4 passed; 0 failed
EXIT_STATUS=0
```

### Isolated executor QA

```text
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-command
Command QA passed
EXIT_STATUS=0
```

Artifacts under `.local/command-qa/<pid>/` include:

- `pause-pending.png` — failed save, pending command, truthful error.
- `pause-retried.png` — retry succeeds once.
- `stop-pending.png` — external stop requested, non-durable cancellation, journal present.
- `stop-retried.png` — retry saves cancellation once, journal cleared.
- `journal-blocked.png` — malformed journal keeps the app viewable and fails closed.

Additional executor QA coverage:

- draft save failure → no definition added, pending retained, retry adds exactly one;
- two journal records with a failed first reconciliation → recovery not ready, neither period in `queued_runs`, storage-error clearing does not admit them;
- terminal-in-memory + failed save + second failed retry → journal retained, DB still contains the replayable period; later successful save clears.

`stop_intent` also covers future-version mutation: `record`/`clear` reject version 99, preserve the original bytes/unknown fields, and never down-convert the journal.

Root independent `--qa-command` evidence is recorded in `.local/opencode-dispatch/root-T13-S1-current.log` (passed).

Warnings in the QA build are pre-existing/unrelated dead-code warnings from other modules. No assertions were weakened.

### Current external compile note

After the last T13 pass, unrelated active T12 `src/navigation_context.rs` test edits introduced compile errors (`Origin::ResearchResult` missing fields, shadowed `store()`). They are outside T13 and were not edited. The recorded full-suite/QA results above are from the last coherent T13 tree; a fresh focused rerun is blocked until that owner finishes. Root should rerun focused checks after the T12 change settles.

---

## 3. Exact call sites still on the old path

### S2 dispatch boundary (required before T13 acceptance)

- `automation_runtime.rs::enqueue_automation_occurrence` still:
  - calls `prepare_occurrence_run`;
  - calls `launch_run`, which pushes the run and saves;
  - then appends `run_id` to `occurrence.run_ids` and saves again.
- `CommandEffect::EnqueueOccurrence` intentionally calls this existing path in S1. This is **not** atomic dispatch and must not be called T13-complete.
- `launch_run` remains shared with ordinary research; S2 must extract its preparation and commit run + occurrence link in one staged write, then use `QueueRun`.
- `automation_after_run` still enqueues the next step through the same effect; its own persistence ordering is not yet fully command-gated.
- `recover_automations` still has its own save path after reconciliation; S2 should route it through the same gate while preserving T03/T05 semantics.
- Auto-retry in `research_app.rs` still has a special-case push/save/pop path; S2 should preserve its semantics while unifying the commit shape.
- Planning submission/auto-repair remains with T23/T25 ownership; S1 did not rewrite it.

### Scoped undo (S3/S4)

- `TaskApp::undo: Vec<Store>`, `snapshot`, `undo_change`, `automation_state_differs` invalidation remain.
- No whole-store undo path was removed; T04 protection is intact.
- Comments, project/topic, archive, materials, model/profile/settings, appearance and concurrency still use whole-store snapshots.

### Other lifecycle gaps intentionally left

- `automation_after_run` completion feedback still uses its existing save path; S1 did not make result completion a typed command.
- `plan_ready` remains presentation/planning-owned and is not treated as a durability receipt.
- `poll_automations` still owns its own tick commit/rollback; only the stop-journal guard was added.

---

## 4. Ownership notes

| File | Change |
| --- | --- |
| `src/domain_command.rs` | T13-owned extensions. |
| `src/command_app.rs` | New T13 executor/stop glue. |
| `src/stop_intent.rs` | New T13 journal helper. |
| `src/command_qa.rs` | New T13 isolated QA. |
| `src/main.rs` | Module registration and `--qa-command` dispatch. |
| `src/app.rs` | Narrow fields/init, `last_saved` visibility, pending-aware `persist_if_changed`/`retry_save`, stop-recovery ordering in `attach_loaded_storage`. |
| `src/automation_views.rs` | Lifecycle handler bodies listed in §1.2 and the stop feedback helper. Did not edit T06 selectors/readers or T23 planning ownership/history UI. |
| `src/automation_runtime.rs` | Only fail-closed guards: `stop_journal_error` in `poll_automations`/`recover_automations`, and per-occurrence `stop_intents` guard in `enqueue_automation_occurrence`. T05 eligibility logic unchanged. |

No T08 search, T06 result-presentation, T23 planning ownership/history UI, or frozen snapshot files were edited.

---

## 5. Remaining T13 scope

### Exact next authorized slice — S2 only

1. **S2.1 — extract launch preparation.**
   - In `src/app.rs`, split `launch_run` into:
     - `prepare_run_dispatch(&self, run, files) -> Result<PreparedRunDispatch, String>` for the existing validation, workspace/session/prompt preparation, `queued_at`/`status=Queued`/`started_at.clear()`;
     - the existing queue effect (`queued_runs`, undo clear, toast, `start_pump`).
   - No second scheduler or agent configuration.

2. **S2.2 — one dispatch commit.**
   - In `src/domain_command.rs`, add the app-facing `DispatchOccurrence`/`DispatchRun` preparation that:
     - calls existing `prepare_occurrence_run` for the next step;
     - stages the prepared `AgentRun` and `occurrence.run_ids` link in one cloned store;
     - validates the S0 queued lifecycle and shared eligibility again.
   - Replace `CommandEffect::EnqueueOccurrence` in `command_app.rs` with the one-commit `DispatchOccurrence` + `QueueRun { run_id }`.
   - Keep `QueueRun` as the existing scheduler effect only.

3. **S2.3 — runtime ordering.**
   - Route `automation_after_run` continuation, `recover_automations`, timeout/expiry, and `research_app.rs` auto-retry through the same gate.
   - Preserve T03 retry authority/identity, T05 eligibility, and T09 shape/version semantics.
   - Leave T23/T25 planning submission and request/history ownership untouched.

4. **S2 tests.**
   - dispatch commit failure → no run, no link, no queue effect;
   - save succeeds, app drops before `QueueRun` → startup queues once, no duplicate;
   - stale prepared run id → conflict/reprepare, never reuse;
   - one SQLite write for automation dispatch;
   - after-run/recovery/auto-retry still preserve completed steps, cursors, and paused/NeedsAttention definitions.

### Later waves

- **S3:** migrate ordinary edit inverses for comments, project/topic, stage/dependencies, archive, materials.
- **S4:** settings/profiles/concurrency/appearance inverses; remove `Vec<Store>` undo and `snapshot`.
- **S5:** remove T04 legacy invalidation after all whole-store paths are gone; T17 docs.
- **T14:** consume `CommandBase` generation for ordered async writes; reject stale commits **before** disk write; integrate stop journal with acknowledgements.
- **T15:** incremental persistence/indexed history; do not silently fold the stop journal into the main schema without review.

**Full T13 remains incomplete until S2 atomic run+period link dispatch and scoped Undo adoption are done.**

---

## 6. Handoff state

- S1 implemented and tested; full suite exit 0.
- S2 not started.
- No commit, push, real workspace access, or app restart.
- T13 remains Open; S1 is not acceptance.
