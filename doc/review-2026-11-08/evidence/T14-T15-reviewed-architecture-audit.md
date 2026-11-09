# T14/T15 persistence and query audit

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Scope: T14 (ordered background persistence with acknowledgements) and T15 (incremental
dirty-entity writes, indexed paginated history, lazy report bodies, versioned migration),
read-only architecture audit and staged implementation/measurement plan.
Mode: **audit only.** No product code, maintained documentation, fixture, snapshot, or
user-visible state was changed. No benchmark, no test suite, no app instance, no
production workspace, no commit, no push, no app restart. The existing dirty working tree
was only read.

**Status: not accepted, not implemented.** This report is input for root review. It does
not satisfy any T14/T15 acceptance criterion.

---

## 1. Tree state this audit describes

`HEAD = 611a4586eedb56600a83c8b272a3af43ba95e4a4`; the working tree is heavily modified by
concurrent T10/T13/T21/T22/T11 work (60+ modified files). Line numbers below were captured
from this snapshot and **will drift**; re-locate by function name.

| File | SHA-256 at audit time |
| --- | --- |
| `src/app.rs` | `5f31c18ea81da08a675b1e3157d6019f4b658f9cb9a981a169c628bea7d0fe65` |
| `src/persistence.rs` | `aa45c097dbbdd481f2d3d84abcf4d469e0fe37d25f1ee1f8d2e8cb045519700c` |
| `src/sqlite_storage.rs` | `7ed294ee38f55886e85c00a46cec38b7fc3aca4f4c3f47917a7117f82aca86a4` |
| `src/domain_command.rs` | `4362075f280d2d910782e828b7f08e22981e580f2be1d2c935a60c7e912f9091` |
| `src/command_app.rs` | `e8e838ffa91b32f86ab06cbaa9530ba3accf7636a510357fd404e52f5ec7179b` |
| `src/automation_qa.rs` | `2df992702dcdf2c7ea71971e6898751b8bd2aaac9079ffad9a49cf1c4788dbf2` |
| `src/automation_views.rs` | `3e6ec044af1fb828dd2e421f3e6171cee63501a0501b3c1e2b8332b445a869c1` |
| `src/domain.rs` | `c500aa5faf7d90017bc80e9e1737c2edab14b9505c2b60064249e34cd54e6a8b` |

Constraint inherited from T13: `src/persistence.rs` / `src/sqlite_storage.rs` were
declared off-limits to T13 and **belong to T14/T15**. `src/domain_command.rs`,
`src/command_app.rs`, `src/stop_intent.rs` belong to T13 and are still changing; T14 must
consume a frozen interface from them (section 14).

---

## 2. Executive summary

1. The measured 1,350 ms UI block is only one of three main-thread costs. The other two are
   (a) whole-`Store` deep equality/`automation_state_differs` on **every** `cx.notify()`,
   and (b) whole-`Store` clone/hash staging in the T13 command gate
   (`CommandBase::capture` clones the store and `state_revision` serializes it to JSON *three
   times per command*). Moving only SQLite IO off-thread cannot meet "<50 ms p95
   main-thread save-related work". T14 must remove all three.
2. The 43,200-occurrence fixture is **not** a report-body fixture. Measured table sizes in
   the retained artifact: occurrences 91.1 MB of the 172.4 MiB database, runs **1.0 MB**
   total, tasks 0.35 MB, automations 2 KB. 70% of each occurrence payload
   (1,474 of 2,089 bytes) is the **byte-identical frozen snapshot**. The dominant scale cost
   is duplicated occurrence snapshots, not report bodies. T15 must therefore (a) dedupe
   frozen snapshots losslessly and (b) add a *separate* body-heavy fixture, or lazy bodies
   will be "proved" on a dataset where bodies are 0.6% of the data.
3. The existing "history page query 0 ms" number is a synthetic vector sort on an
   already-loaded copy — exactly what TASKS.md forbids. The real UI paths are
   `automation_results` (clone refs of all 43,200 → full sort with `occurrence_activity`
   recomputed per comparison → take 20), per-definition history (same), and
   `automation_list` (per definition: full scan for `latest_occurrence`, full scan for
   unread); each rendered row also calls `present_occurrence`, which scans `runs` several
   times. With runs on most periods this is O(page × runs).
4. Incremental writes are required for correctness, not only speed: the current write
   rewrites six tables and the metadata payload in one transaction, so any save failure is
   all-or-nothing; a patch pipeline must preserve that atomicity for *cursor + occurrence*
   and for `occurrence.run_ids` + `runs`.
5. The command gate currently stages a full `Store` clone and keeps it on failure
   (`pending_commit`). On this fixture that is a multi-hundred-millisecond main-thread cost
   *per user command*, independent of save. T14 must replace clone-staging with an
   entity-scoped change set. That is a contract change to T13 and needs root/T13 agreement
   before implementation.
6. `TaskApp::undo: Vec<Store>` keeps up to **30 whole-store clones**. On this fixture that
   is potentially several GB and makes any memory benchmark meaningless. T13 S3/S4 scoped
   undo is a hard prerequisite for T14's memory evidence; T14 must not own undo.

Decisions requested from root are listed in section 15.

---

## 3. Measured baseline vs assumption

### 3.1 Measured (retained evidence)

Source: `doc/review-2026-11-08/evidence/perf.txt`, produced by
`src/automation_qa.rs::performance_fixture`, retained artifact
`.local/automation-perf/51782/workspace.sqlite3`.

| Metric | Measured | Notes |
| --- | --- | --- |
| Fixture | 43,200 occurrences, 240 runs, 320 tasks, 1 automation | SQLite `user_version=3` |
| Database size | 180,748,288 bytes = 172.4 MiB | same artifact as the review |
| Occurrence payload total | 91,118,398 bytes (43,200 rows) | 98.5% of all table-payload bytes |
| Run payload total | 1,008,290 bytes (240 rows) | reports are tiny in this fixture |
| Task payload total | 349,623 bytes | |
| Occurrence payload split | 2,089 B: snapshot 1,474 B (template 1,107), rest 615 B | snapshot is 70.6% |
| Cold save | 1,964 ms | one sample |
| Warm saves | 1,292 / 1,316 / 1,912 ms | 3 samples; **no p95**, hardware not recorded then |
| Load | 634 ms | full deserialize of every run and occurrence |
| UI-triggered synchronous save block | 1,350 ms | one sample |
| Cold open incl. attach | 2,087 ms | |
| List frame / detail frame | 12 ms / 55 ms | small-run dataset |
| "History page query" | 0 ms | **synthetic vector sort, not the UI path; not acceptable evidence** |

Machine used for this audit and available for re-measurement: Apple M1 Pro, 32 GiB RAM,
macOS 27.2. The 2026-11-08 perf run did not record hardware or build profile; it was the
dev profile (`opt-level = 1`, `debug = 0`) with `--features qa`. T14 must record both, and
must not compare dev-profile results with release numbers.

### 3.2 Assumptions that are **not** measured (must be instrumented, not asserted)

- The split of the 1.3–1.9 s save between JSON serialization, path transform, SQLite
  DELETE+INSERT, and fsync.
- `last_saved = store.clone()` cost after each successful save.
- `store == last_saved`, `automation_state_differs`, and `reconcile_goal_approvals` cost per
  notify/pump tick (they run before the "is it changed" fast path).
- `CommandBase::capture` clone + `state_revision` `serde_json::to_vec` cost (called in
  `capture` and in **both** `matches` checks of `commit_prepared`).
- `PreparedCommand.staged` clone/retain cost, including retained `pending_commit`.
- Peak RSS of the in-memory `Store`; effect of 30 deep undo snapshots.
- `present_occurrence` / `Store::research_result` scan cost per rendered row as runs scale.
- Behaviour of all the above at 2× and 4× the fixture, and with 20–50 KB bodies.

---

## 4. Current write path and cost decomposition

### 4.1 Write chain

`TaskApp::persist_if_changed(force)` (`src/app.rs:1274`) →

1. `store.reconcile_goal_approvals()` — mutates research goals; per goal it computes
   `goal_plan_signature` (serializes goal + all child tasks) → O(goals × tasks).
2. `automation_state_differs(&self.store, &self.last_saved)` — deep comparison of
   automations, occurrences, automation project, automation tasks/topics/requests.
3. `store == last_saved` — full `Store` deep equality.
4. Throttle: during active runs, at most one write per 500 ms.
5. `storage.save(&self.store)` then `self.last_saved = self.store.clone()` (a second full
   clone) on success.

`Storage::save` (`src/persistence.rs:222`) →

6. `file_stamp` + possible full `self.load()` (re-read of the entire workspace) if the file
   stamp changed.
7. `sqlite_storage::save` → `serde_json::to_value(store)` (full serialize; `Send` + large
   `Value` tree), `transform_paths` (walks all runs, parses `input_signature` JSON),
   `upgrade_paths` (opens DB read-only, reads `user_version`), `write`.
8. `write` → one transaction: `CREATE TABLE IF NOT EXISTS` for 6 tables, **`DELETE FROM`
   every table**, chunked re-INSERT of every row, `INSERT OR REPLACE` the single
   `workspace` metadata row, `PRAGMA user_version`, commit. `journal_mode=DELETE`,
   `synchronous=FULL` (fsync per commit).
9. `file_stamp` update.

The command path is a second write chain: `execute_command` → `Store::prepare_command`
(clone + validate) → `commit_prepared` → `StorageSink::save` → same `Storage::save`, then
`last_saved = live.clone()`.

### 4.2 Why "move SQLite off-thread" is insufficient

Main-thread work that would remain if only `Storage::save` moved to a worker:

| Remaining main-thread work | Where | Amplifier |
| --- | --- | --- |
| `store == last_saved` | `persist_if_changed`, `has_unsaved_changes`, `retry` paths | every `cx.notify()` through `cx.observe_self` |
| `automation_state_differs` | `persist_if_changed` | deep compare of occurrences each notify |
| `reconcile_goal_approvals` | `persist_if_changed` | per-goal signature serialization |
| `CommandBase::capture` clone | every command | whole store |
| `state_revision` serialization | `capture` + 2× `matches` | whole store → JSON, 3× per command |
| `PreparedCommand.staged` clone | every command + retained on failure | whole store; `pending_commit` retains **two** full copies (`base.exact` + `staged`) until retry |
| `last_saved = live.clone()` | every command commit | whole store |
| rollback clones | `poll_automations`, `automation_after_run`, `recover_automations`, `stop`, auto-retry | whole store per tick / per completion |
| `undo.push(store.clone())` | `snapshot()` | up to 30 whole stores retained |

Conclusion: T14's core deliverable is a **generation + entity-change-set** architecture that
removes all full-store clone/compare/serialize work from the main thread. The worker is the
ordering mechanism, not the whole fix.

---

## 5. Complete synchronous save/store-call inventory (audit snapshot)

Classes: **A** = autosave/runtime (must become patch + ack), **B** = user command (already
gated, must become patch + ack), **C** = "Saved" UI wrapper (must become ack-driven),
**D** = startup/attach/relocation (may stay a full write), **E** = test/QA only.

| # | Site (audit snapshot) | Class | Synchronous work | Notes / required change |
| --- | --- | --- | --- | --- |
| 1 | `app.rs:491` `cx.observe_self` → `persist_if_changed(false)` | A | compare + copy + serialize + DB | Fires on every notify; must become `queue_persist()` (no compare, no clone). |
| 2 | `app.rs:1274-1314` `persist_if_changed` | A | all of §4.1 | Replace body with patch drain + generation bump. Keep `force` = "bypass 500 ms coalescing", **not** "full snapshot". |
| 3 | `app.rs:1342` `retry_save` | A/C | full save of `self.store` | Must re-enqueue retained unacked patches in order; never write a stale full store. |
| 4 | `app.rs:1226` `attach_loaded_storage` initial save | D | full save | Keep full write; must complete (ack) before automation recovery/dispatch is enabled. |
| 5 | `app.rs:1263` `save_staged_store` (`project_app.rs:209` project creation) | C | full save of a staged store | Convert to a command/creation patch or keep full write at creation (rare, bounded); measure. |
| 6 | `app.rs:757` `confirm_save` → `persist_if_changed(true)`; used by `app.rs:1110` task save, `app.rs:1423` project folder, `report_publish.rs:255/360`, `settings_views.rs:32/157/166`, `views.rs:3970` | C | full save | Currently the only path that honours "Saved means written". Must show "Saving…" then "Saved" on ack. |
| 7 | `command_app.rs:30` `StorageSink::save` via `command_app.rs:89` | B | full save | Becomes worker enqueue; `qa_storage_writes` counts committed transactions. |
| 8 | `command_app.rs:581` `save_current_store_durably` (used by after-run, recovery, stop reconciliation, auto-retry) | A | full save + clone to `last_saved` | Becomes `commit_now()`: flush patch, await ack, bounded timeout. Must remain durable-before-effect. |
| 9 | `automation_runtime.rs:1716` scheduler tick save + `1693` rollback clone | A | full save, full clone | Patch + entity rollback only; keep all-or-nothing tick. |
| 10 | `automation_runtime.rs:1744` post-tick persist | A | full save | Same patch as #9; one transaction. |
| 11 | `automation_runtime.rs:1954` dispatch-failure attention save | A | full save | Patch (definition + occurrence). |
| 12 | `automation_runtime.rs:1993`, `:2010` fake-runner transitions | A/E | full save | Patch; fake mode only. |
| 13 | `automation_runtime.rs:2167`, `:2218` deadline/regeneration enforcement | A | full save | Patch; usually 1 run + 1 occurrence. |
| 14 | `automation_runtime.rs:2498` stop completion save | A | full save | Patch; stop durability must be proven by ack (see §9.5). |
| 15 | `research_app.rs:66,77,114` model/agent settings, `:382` materials | C | full save | Patch (task/meta). |
| 16 | `research_app.rs:639` run completion (`complete_research_run`) | A | full save before auto-retry/continuation | Patch; must stay durable-before-effect. |
| 17 | `research_app.rs:655` auto-retry rollback clone | A | full clone | Entity-scoped rollback. |
| 18 | `recovery.rs:351,363,457,479,494,504,520` recovery imports | A | up to 7 full saves per poll | Patch + coalesce; today each import can block the UI. |
| 19 | `batch_app.rs:138,260` batch queue changes | A | full save | Patch. |
| 20 | `archive_app.rs:160` archive/restore | A/C | full save | Patch; T22 owns semantics. |
| 21 | `project_app.rs:324` plan apply/repair | A | full save | Patch; T23/T25 own semantics. |
| 22 | `report_publish.rs:283` publication claim, `:329` publication state | A/C | full save | Patch (publication entry). |
| 23 | `startup.rs:360` post-open normalization save | A | full save | Once per open; may stay full but must not run before the worker shadow is initialized. |
| 24 | `research_views.rs:229` model settings | C | full save | Patch. |
| 25 | `views.rs:350` task field save | C | full save | Patch. |
| 26 | `automation_views.rs:863` mark-read on opening a result | A | full save | Not command-gated today; convert to a `SetRead` command via the gate (T13 already has `SetRead`). |
| 27 | `automation_views.rs:1311,1384,1400` planning plan-ready / rejection | A | full save | T23/T25 own ownership; T14 only makes the write patched. |
| 28 | `automation_views.rs` lifecycle commands (`1441,1465,1550,1595,1612,1649,1667,1727,1747,1764,1793`) | B | via `execute_command` | Already gated; must become patch + ack. |
| 29 | `workspace.rs:241` relocation staging save + verify | D | full save + full reload compare | Keep full write; must remain v4-aware and preserve legacy files. |
| 30 | `persistence.rs:161` SQLite migration verify | D | full save + load | Keep; extend for v4. |
| 31 | `sqlite_storage.rs:575,631,726`, `persistence.rs` tests, `*_qa.rs` | E | full save | QA helpers may keep full saves; add an isolated perf entry instead of editing shared fixtures. |

Additional whole-store clone sites that T14 must address (not saves, but the same cost
class): `app.rs:567` init, `app.rs:724` `snapshot()`, `app.rs:1228/1270/1300/1345`,
`command_app.rs:583`, `automation_runtime.rs:1693/2024/2075/2239/2391`,
`research_app.rs:655`, `domain_command.rs:71/718`, `project_app.rs:193`,
`project_views.rs:31`, `request_planning.rs:222/731/882`, `project_planning.rs:296/325`,
`domain.rs:923/1410/1566/1758`. Domain-internal staging clones are acceptable only if they
are entity-scoped or happen off the main thread.

---

## 6. Field, drop and load semantics (must be preserved by T15)

### 6.1 Three independent versions

| Version | Location | Meaning |
| --- | --- | --- |
| JSON `VERSION = 5` | `persistence.rs:9` | legacy `workspace.json` envelope |
| SQLite `SCHEMA_VERSION = 3` | `sqlite_storage.rs:11` (`user_version`) | table layout; T15 introduces 4 |
| `Store.automation_schema_version` | `automation::AUTOMATION_SCHEMA_VERSION` | domain automation schema |

T15 changes only SQLite `user_version` (3→4). It must not conflate the three, and must keep
refusing newer/unknown versions without writing (`persistence.rs:183`,
`sqlite_storage.rs:29,100,171`).

### 6.2 Table split and the metadata blob

`sqlite_storage::TABLES = [tasks, projects, runs, policies, automations,
automation_occurrences]`. Every other `Store` field (topics, research_requests,
publications, report_publishing, next_id, settings, profiles, appearance,
`last_active_project`, `automation_schema_version`, …) lives in the **single**
`workspace` row (`id=1, payload TEXT`). `write` removes the six table keys from the
serialized `Value` and stores the remainder; `read_connection` puts them back from the
tables. Consequences:

- A rename of one automation currently rewrites 172 MB of JSON because the metadata row is
  rewritten wholesale. Incremental writes must split metadata into fields or a per-field
  table.
- Any new non-table `Store` field is persisted automatically — except transient fields,
  which **must** be `#[serde(skip)]` (dirty set, generations, hydrated bodies, caches).
  There is no `#[serde(deny_unknown_fields)]`: unknown JSON keys are silently dropped on
  load and therefore permanently lost on the next save. Do not introduce a field in a newer
  binary without a `VERSION`/`user_version` bump, and do not write transient state into JSON.

### 6.3 Load contract

`Storage::load` → for SQLite: `read` (read-only connection, one transaction) →
`read_connection` (version check, metadata payload, per-table `SELECT payload ORDER BY
position`) → `transform_paths(decode)` → `serde_json::from_value::<Store>` →
`normalize_automation_schema` → `validate_automation_integrity` → `normalize_topics` →
`normalize_planning_ownership`. A missing file is `Ok(None)` and the app substitutes
`Store::empty()`. An unreadable/invalid/future file is an error and is never treated as
empty (`persistence.rs:121`, `sqlite_storage.rs:29`).

Ordering contract: `position` is the vector index; `read` restores exact vector order.
No production code uses `Vec::insert`, `remove(index)` or `sort` on the six entity vectors
(verified by grep), so monotonic positions written incrementally are safe **only if** every
incremental insert uses `MAX(position)+1`. Vector order is part of
`Store::PartialEq`, so any reordering breaks reopen-equality tests.

Path contract: only `projects[].directory`, `runs[].directory`,
`runs[].workspace_source`, and known positions inside `runs[].input_signature` /
`automations[].plan_binding.input_signature` are rewritten. `prompt`, `output`, `logs`,
sources, titles and free text are byte-preserved. `replace_signature_path` preserves exact
JSON bytes so signature string equality survives relocation.

### 6.4 Failure and durability contract

- One `Storage::save` = one transaction; any error rolls back all tables and
  `user_version` (tested by trigger injection, `persistence.rs:476-488`).
- `Storage.validated` stamp avoids re-reading before each save; a changed stamp triggers a
  re-read, and afterwards the in-memory store still **overwrites** the file. With
  incremental writes, "reload then overwrite" is unsafe (it would discard rows written by
  the other writer). T15 must fail closed instead (see §9.7 and section 15, Q5).
- `workspace.lock` is held for the process lifetime and must never be unlinked. The worker
  must not open a second lock.
- The DB file is `0600`, directories `0700`.
- `next_id` must remain monotonic across migration and patched writes.

### 6.5 Backup / migration semantics that already exist (reuse, do not reinvent)

`sqlite_storage::upgrade` (called from `Storage::upgrade_paths` and from every `save`):

1. read-only open, accept only `1 | 2`, else refuse;
2. read the full logical store;
3. preflight `serde_json::to_value` + `transform_paths(encode)`;
4. `VACUUM INTO workspace.before-path-upgrade-<nanos>-<pid>-<seq>.sqlite3` (0600), read it
   back, `PRAGMA integrity_check`, full `Store` equality against the original;
5. fsync backup file and parent directory;
6. rewrite the original inside one transaction and verify;
7. return the backup path for the UI; never delete old backups.

T15 extends the accepted versions to `1 | 2 | 3` and adds a v4-specific backup name. The
migration must keep the same "verify before commit, verify after write, keep backup on
failure" shape.

---

## 7. Report body and source consumers (lazy-body contract)

`AgentRun.output` is a `String` (serde default `""`). Sources are `Vec<Source>` on the run;
logs are `Vec<String>` on the run. Consumers found in production code:

| Consumer | Uses | If body is not hydrated |
| --- | --- | --- |
| `research.rs:567` `research_result` / `research_result_checked` (validity for steps, lists, publication) | `!output.trim().is_empty()` plus signature/dependency checks | A run with a stored body would look result-less → step re-runs, publish/export refuses. **Correctness-critical.** |
| `domain.rs:689` `progress_result_task_count` | `run.status == "Succeeded" && !output.trim().is_empty()` | Counts drop silently |
| `navigation.rs:135` result selection | same emptiness test | Missing result / wrong destination |
| `archive_presentation.rs:44` retained outcome | same | Archived goal would lose its outcome |
| `automation_runtime.rs:2247` regeneration/final selection | same | Wrong final report; regeneration loop |
| `result_presentation.rs` `present_occurrence` | scans runs, reads bodies | Empty result previews |
| `automation_views.rs:3643` result dialog | truncates `output` to 20,000 chars, calls `.chars().count()` over the whole body | Empty dialog; O(body) per render |
| `report_publish.rs:133` `topic_share_content` | concatenates full bodies + sources | Publishes empty pages |
| `research_export.rs` | full bodies | Exports empty documents |
| `research.rs` prompt handoff / `.research-inputs` | upstream bodies | Downstream steps get no evidence |
| `project_app.rs:381-405` planning parse | planner `output` | Plan cannot be parsed/applied |
| `recovery.rs:357` recovered output | last output | Recovery loses the answer |
| `qa.rs:137` writes `task-<id>-result.md` | bodies | QA evidence gaps |
| `global_search.rs` | titles/metadata only (bodies not searched today) | No change |

**Required contract (compile-enforced, not review-enforced).** Remove the public
`output: String` and replace it with:

```rust
pub struct AgentRun {
    // ... unchanged ...
    /// Number of bytes of the stored body. Persisted. Zero means no body.
    #[serde(default)]
    pub body_bytes: usize,
    /// Hydrated body; never serialized as part of the run row.
    #[serde(skip)]
    pub body: Option<SharedText>,   // SharedText = Arc<str> with a manual serde impl
}

impl AgentRun {
    pub fn has_report(&self) -> bool { self.body_bytes > 0 }
    pub fn body(&self) -> Result<&str, BodyNotLoaded>;
}
```

Every consumer above must then explicitly hydrate via
`TaskApp::hydrate_body(run_id)` / `Store::body_or_load(run_id, &BodySource)`; the compiler
enumerates all of them. Do not keep an empty `output` field "for compatibility": that is
exactly how a silent "no report" regression enters. Migration derives `body_bytes` from
`length(output)` and moves the text to `run_bodies`; a run with `status = "Succeeded"` and
`body_bytes = 0` is flagged by integrity validation (it already is a no-result run today).

`logs` stay in the run row for this iteration (they are small and are read by the technical
details dialog); note the 2,000-line cap in `record_events`.

---

## 8. Actual history/query paths and ordering contract

| UI path | Current behaviour | Cost class |
| --- | --- | --- |
| `automation_results` (`automation_views.rs:2138-2245`) | collect all occurrences → `sort_by(compare_newest_first)` over all → optional filter → `take(history_limit)`; counts queued/waiting over all | O(n log n) with activity recomputation per comparison; **0 ms figure is not this path** |
| Per-definition history (`automation_views.rs:2494-2545`) | filter all occurrences by definition → sort all → take limit; "Load more" raises `history_limit` by 20 and re-sorts everything | Same, repeated per click |
| `automation_list` (`automation_views.rs:1959-1990`) | per definition: `latest_occurrence` (full scan) + `automation_unread_count` (full scan) + `present_occurrence` (several run scans) | O(defs × (n + runs)) |
| `automation_occurrence_row` (`automation_views.rs:2824`) | `present_occurrence` per row → multiple full `runs` scans + `research_result` linear scans | O(page × runs) |
| `Store::occurrence(id)` (`automation.rs:683`) | linear scan | Called from hot paths (dialogs, gates) |
| `Store::research_result(task)` (`research.rs:567`, via `research_result_checked`) | linear scan of `runs`, fresh `Validation` cache per call | Called per step in lists |
| `global_search::search` | builds a candidate with formatted strings for every occurrence | O(n) per keystroke; T08 owns the UI |

Ordering authority is `automation_activity::compare_newest_first` (T16):
descending `(activity_utc, ActivityKind rank, occurrence_id)` where the UTC source is
`retry_requested_at` → `actual_started_at` → `requested_at` → `scheduled_for` →
`reference_time`. SQL index columns **must** be derived from `occurrence_activity`, never
from a second formula. Activity kind ranks: `Reference=0 < Scheduled=1 < ManualRequest=2 <
ActualStart=3 < RetryRequested=4` (declaration order), compared **descending**.

Required T15 query API (order = SQL `ORDER BY activity_utc DESC, activity_rank DESC,
id DESC`):

```rust
pub struct HistoryCursor { pub activity_utc: i64, pub activity_rank: u8, pub id: u64 }

pub struct HistoryQuery {
    pub automation: Option<u64>,      // None = all automations
    pub cursor: Option<HistoryCursor>,
    pub limit: usize,
    pub watermark: Option<u64>,       // activity_seq snapshot at first page
}

pub struct HistoryPage {
    pub rows: Vec<HistoryRow>,        // occurrence summary + presentation inputs
    pub next: Option<HistoryCursor>,
    pub total: usize,
    pub unread: usize,
}
```

Pagination under concurrent insert/retry: a retry can move an old row to the top by
changing its sort key. Recommended rule (must be written in DESIGN by T17):
each page sequence is bound to a `activity_seq` watermark taken when the first page loads;
rows whose `activity_seq > watermark` are excluded from later pages (they appear on
refresh at the top). This guarantees **no duplicates and no reordering within a page
sequence**; a moved row is deliberately absent until refresh. The alternative (keyset only)
can duplicate or skip rows, so it must not be used. Test both an insert and a retry between
pages.

---

## 9. Target architecture (T14)

### 9.1 Data model additions (transient, serde-skipped)

```rust
pub struct Store {
    // existing fields unchanged
    #[serde(skip)] pub(crate) dirty: DirtySet,
    #[serde(skip)] pub(crate) generation: u64,
    #[serde(skip)] pub(crate) activity_seq: u64,
}

#[derive(Default)]
pub struct DirtySet {
    pub tasks: BTreeSet<u64>,
    pub projects: BTreeSet<u64>,
    pub topics: BTreeSet<u64>,
    pub policies: BTreeSet<Stage>,          // or "all policies"
    pub automations: BTreeSet<u64>,
    pub occurrences: BTreeSet<u64>,
    pub runs: BTreeSet<u64>,
    pub requests: BTreeSet<u64>,
    pub publications: BTreeSet<String>,
    pub meta: BTreeSet<MetaField>,
    pub removed: Vec<(Table, u64)>,
}

pub struct StorePatch {
    pub generation: u64,
    pub base: u64,               // worker shadow generation this patch applies to
    pub upserts: Vec<EntityPayload>,   // owned, serialized once, off the UI thread
    pub deletes: Vec<(Table, u64)>,
    pub meta: Vec<(MetaField, MetaValue)>,
    pub activity: Vec<(u64, i64, u8, u64)>, // id, activity_utc, rank, activity_seq
}
```

`Store::touch_*` records ids. `Store::drain_patch()` clones **only** the dirty entities
(typically 1–3), clears the set, and bumps `generation`. Serialization to JSON happens on
the worker, from the owned values (no UI-thread `to_value` of the store).

### 9.2 Where tracking is inserted

Mutation sites that must call `touch_*` (grouped; ~46 `iter_mut` sites plus mutators):

- `domain.rs`: `task_mut`, `set_stage`, `add_comment`, `create_*`, `apply_*`, `retain`
  paths, `remove_reviewed_demo` (QA), `recover_interrupted_runs`, `normalize_*`.
- `research.rs`: `reconcile_goal_approvals`, `sync_goal_progress`,
  `apply_proposed_research_plan`.
- `automation*.rs`: every `occurrence_mut`/`automation_mut` write, tick commit, retry,
  stop, mark-read, snapshot materialization, run linking.
- `app.rs`: streaming `record_events` loops, `dispatch_runs` status writes, `shutdown`
  interrupted writes.
- `recovery.rs`, `batch_app.rs`, `archive_app.rs`, `project_app.rs`, `report_publish.rs`,
  `research_app.rs`: the specific `iter_mut().find(id)` sites listed in §5.
- `command_app.rs`/`domain_command.rs`: command change sets (see §9.4).

Safety net (mandatory, because missed tracking silently drops data):
1. `#[cfg(debug_assertions)]` audit after every ack: full-compare worker shadow vs live
   store; panic with the differing table/ids on mismatch. Runs in all tests/QA.
2. Release-mode audit every N acks (N≈64) and on shutdown; on mismatch, log, force a full
   corrective write, and surface a diagnostic — never silently continue.
3. `touch_*` inside existing mutators where possible, so callers cannot forget the common
   paths.

### 9.3 Worker protocol and acknowledgement state machine

```rust
enum PersistRequest {
    Init { store: Box<Store> },            // attach only; one full write
    Patch(StorePatch),
    Query(QueryRequest),                   // T15 page/count requests
    Flush { generation: u64 },             // bounded wait for durability
    Shutdown,
}

enum PersistEvent {
    Ready { generation: u64 },
    Committed { generation: u64, rows: RowsWritten, elapsed: Duration },
    Failed { generation: u64, message: String, retryable: bool },
    QueryResult(QueryResponse),
    Flushed { generation: u64, result: Result<(), String> },
}
```

Main-thread state: `store_generation`, `durable_generation`, `unacked: VecDeque<StorePatch>`,
`pending_effects: BTreeMap<u64, Vec<QueuedEffect>>`, `last_persist_error`,
`query_inflight`. Drain `PersistEvent` in the existing 80 ms pump **and** in a dedicated
20 ms poll while `unacked` is non-empty (shortens durable-before-dispatch latency without a
new async runtime; no new dependency).

Rules:

1. **Ordering.** One worker thread, FIFO. A patch may be applied only if
   `patch.base == shadow.generation`; otherwise it is rejected as `Stale` **before any disk
   write**. The main thread coalesces only by merging patches in order (union of upserts,
   deletes win, `base` unchanged) — never by dropping a patch that carries a different
   entity.
2. **No stale assignment.** An ack updates `durable_generation = max(durable_generation,
   generation)` and the compact durable status index. It never assigns a store value. A
   `Committed` older than `durable_generation` is ignored (defensive; single worker makes it
   impossible in practice).
3. **Durable before dispatch.** `StartOrExtend` commands register their effects under the
   generation that contains their change. Effects run only on `Committed(g)` with
   `g >= effect.generation`, and each effect is re-validated against the current store at
   execution time (e.g., `QueueRun` only if the run is still `Queued`, not stopped, and the
   occurrence still active). If the effect is no longer valid, drop it and report a truthful
   notice — never start work that a later command cancelled.
4. **Coalescing.** Streaming logs produce at most one patch per flush window (reuse the
   existing 500 ms throttle, but measure; 100–200 ms is likely fine once writes are cheap).
   `force = true` bypasses the window; it does not change patch size.
5. **Failure.** On `Failed`, keep `unacked` and the failed generation; show
   "Not saved · <reason>" plus "Retry save". Retry re-enqueues the **same patches** in order
   (they are still valid against the shadow because nothing was committed). Do not
   drop/reprepare patches arbitrarily; commands keep their existing reprepare/conflict rule
   for their *intent* (T13 §5.2), while the persistence layer retries only durable writes.
6. **Retained intents.** `pending_commit`, `pending_stop`, `pending_dispatch` remain T13
   concepts, but must not retain whole `Store` values. `PreparedCommand` must carry an
   entity change set, not `staged: Store`.
7. **Shutdown.** `shutdown()` sends `Flush` and blocks (bounded, e.g. 5 s, matching the
   process-stop bound) before the window closes; on failure, report "Not saved" in the
   terminating state. `on_app_quit` currently returns immediately; it must await the flush.
8. **Crash.** Ack is sent only after commit. A crash between commit and ack leaves a durable
   state the UI never saw; on reopen the state is loaded normally and startup recovery
   reconciles any durable `Queued` run exactly once (existing rule). A crash before commit
   rolls back the transaction.

### 9.4 Command-gate change (T13 dependency)

Recommended (requires T13/root approval):

- `CommandBase { generation: u64 }`; delete `exact: Box<Store>` and `state_revision`
  (removes 3 full serializations per command).
- `PreparedCommand { id, class, command, base, change: StoreChange, outcome }` where
  `StoreChange` = `Vec<EntityBefore>` + `Vec<EntityAfter>` + meta changes.
- Prepare mutates the live store optimistically and records the before-images of exactly
  the touched entities; validation still runs on the current store. This deliberately
  replaces T13 S0's "no live mutation before success" with "no durable effect, feedback or
  effect execution before ack, and exact entity rollback on rejection/failure". Every
  entity type must have a rollback test (value equality after rollback, including vector
  order, `next_id`, and `activity`).
- `commit_prepared` splits into `enqueue_prepared(...) -> PendingCommit` and
  `on_ack(pending, &mut live) -> Result<CommandReceipt, CommitError>`.
- `last_saved: Store` is replaced by `durable_generation` plus a compact
  `durable_occurrence_status: HashMap<u64, OccurrenceStatus>` (needed by the stop-durability
  rule and by "no saved claim"). `automation_state_changed` is replaced by "the drained
  change set touched automation entities", so undo invalidation stays exact and cheap.

Fallback if T13 cannot adopt change sets yet: keep the command path on a full clone
**temporarily**, but acceptance cannot be claimed, because a single rename would exceed the
50 ms budget. Root must either (a) authorize the change-set contract, or (b) accept a larger
budget for commands and re-scope T14 acceptance accordingly. Do not silently redefine the
acceptance metric.

### 9.5 Stop exception (already T13-owned)

Keep the existing asymmetry: external stop proceeds without durability, never claims it,
and retains the journal. T14 only changes how durability is observed:
`durable_occurrence_status(id)` replaces `last_saved.occurrence(id)`; `StopOutcome` values
and journal ordering are unchanged. The journal stays a separate file (T13 note: T15 must
not fold it into the schema).

### 9.6 "Saved" semantics

`has_unsaved_changes()` becomes `store_generation > durable_generation`. The footer already
renders "Saving changes…" from that predicate (`views.rs:529`), so the truthful three-state
display needs no visual redesign. `confirm_save` becomes: enqueue patch → show "Saving…" →
on ack show the caller's success text; on failure show "Not saved · reason" + Retry.
No "Saved" before ack (TASK T14 acceptance).

### 9.7 External-change policy (decision needed)

Today a changed file stamp triggers a full re-read and then an overwrite. With patches the
worker must either (a) fail closed ("Workspace changed on disk; reopen"), or (b) re-read and
rebase. Rebasing patches onto externally changed rows is complex and can silently overwrite.
Recommend **fail closed**, stop accepting new patches, keep the live store editable but
visibly "Not saved", and require reopen. Root must approve; it changes current behaviour.

---

## 10. T14 staged slices

Each slice must leave the app buildable and all existing accepted behaviour passing. Do not
start slice N+1 before slice N exits. All slices are T14-owned files only (section 14).

### T14-S0 — Instrumentation and fixtures (no behaviour change)

Files: new `src/persistence_metrics.rs`, new `src/persistence_qa.rs`, one registration line
in `src/main.rs`, `#[cfg(feature = "qa")]` span calls only.

- Span recorder: `persist.reconcile`, `persist.automation_compare`, `persist.store_compare`,
  `persist.clone_last_saved`, `persist.serialize`, `persist.path_transform`,
  `persist.sqlite_write`, `persist.fsync`, `load.read`, `load.deserialize`,
  `load.normalize`, `command.capture`, `command.hash`, `command.stage`,
  `command.commit`, `history.filter`, `history.sort`, `history.present_row`,
  `body.read`, `ack.apply`.
- `--qa-persistence-perf` with modes `small`, `occ43200`, `xl`, `body-heavy`, `migration`,
  `crash`, `concurrency` (section 12). Isolated roots only.
- Record hardware, OS, build profile, sample counts, p50/p95/p99, DB size, peak RSS.
- Exit: a baseline table per span; no source behaviour changes; existing tests unchanged.
  **No expensive benchmark is run before root approves the fixture recipe in section 12.**

### T14-S1 — Dirty journal (pure domain)

Files: `src/domain.rs` (DirtySet/StorePatch/touch), `src/automation*.rs` mutators,
`src/research.rs`; tests in-file.

- `Store::touch_*`, `drain_patch`, `patch.apply_preview` (test-only), debug full audit.
- Insert tracking at every mutation site from §9.2; collect ids before `iter_mut` loops.
- Exit: unit tests prove each mutator marks exactly the intended entity; patch application
  is byte-identical to whole-store staging for randomized sequences; debug audit clean in
  `cargo test --quiet` and the automation/command QA suites.

### T14-S2 — Ordered worker + ack state machine (behaviour-compatible)

Files: new `src/persistence_worker.rs`; `src/persistence.rs` (worker-facing API only);
`src/app.rs` (fields, `persist_if_changed`, `retry_save`, `attach_loaded_storage`,
`shutdown`); `src/command_app.rs` (executor integration).

- Move `Storage` into the worker; `TaskApp` keeps `PersistenceHandle` + `_storage_lock`.
- Replace `storage.is_some()` gates with `persistence_available()`.
- Replace `last_saved` with generations + durable status index; update the 39 `last_saved`
  references (20 are tests; T13 owns `command_app.rs`, coordinate).
- `save_current_store_durably` → `commit_now(timeout)`.
- Footer/error/retry paths driven by ack.
- Exit: no `Storage::save` call on the UI thread (grep gate + a QA assertion); save
  failure/retry, stale patch rejection, out-of-order ack rejection, shutdown flush, and
  durable-before-dispatch tests pass; automation/command QA pass with truthful feedback.

### T14-S3 — Remove command/rollback whole-store clones (T13 contract change)

Files: `src/domain_command.rs`, `src/command_app.rs` (both T13-owned; requires handover),
`src/automation_runtime.rs`, `src/research_app.rs` rollback sites.

- Change-set staging, delete `state_revision`/`exact`, split commit into enqueue/ack.
- Entity-scoped rollback for tick/after-run/recovery/stop/auto-retry.
- Exit: command prepare p95 budget met on the 43,200 fixture; every T13 S0/S1/S2 test and
  T03/T04/T05/T09 regressions still pass; retained `pending_*` memory is entity-sized.

### T14-S4 — Coalescing, streaming, and truthful pending UX polish

- Streaming coalescing window; footer states; "Retry save" semantics; backlog of patches.
- Exit: during a 60 s simulated streaming run with an injected keystroke every 200 ms, p95
  persistence-attributable main-thread time < 50 ms; no lost patch (debug audit clean);
  durable-before-dispatch preserved.

### T14-S5 — Crash/concurrency evidence and handoff

- Section 13 matrix, evidence logs, and the exact benchmark table; no acceptance claim.

---

## 11. T15 storage v4 design

### 11.1 Schema v4 (proposal)

```sql
-- keep: tasks, projects, policies, automations
-- extend:
ALTER TABLE automations ADD COLUMN status TEXT;          -- denormalized from payload
ALTER TABLE automations ADD COLUMN project_id INTEGER;
ALTER TABLE automations ADD COLUMN updated_at INTEGER;

-- occurrences: split from snapshot, add index columns
CREATE TABLE automation_occurrences (
  position INTEGER PRIMARY KEY,
  id INTEGER NOT NULL UNIQUE,
  payload TEXT NOT NULL,               -- occurrence WITHOUT snapshot
  automation_id INTEGER NOT NULL,
  status TEXT NOT NULL,
  unread INTEGER NOT NULL,
  activity_utc INTEGER NOT NULL,
  activity_rank INTEGER NOT NULL,      -- ActivityKind declaration order
  activity_seq INTEGER NOT NULL,       -- bumped on every activity change
  reference_time INTEGER NOT NULL,
  trigger_key TEXT NOT NULL,
  has_snapshot INTEGER NOT NULL
);
CREATE INDEX idx_occ_activity ON automation_occurrences(activity_utc DESC, activity_rank DESC, id DESC);
CREATE INDEX idx_occ_auto_activity ON automation_occurrences(automation_id, activity_utc DESC, activity_rank DESC, id DESC);
CREATE INDEX idx_occ_auto_unread ON automation_occurrences(automation_id) WHERE unread = 1;
CREATE UNIQUE INDEX idx_occ_trigger ON automation_occurrences(automation_id, trigger_key);

-- deduplicated immutable snapshots
CREATE TABLE occurrence_snapshots (
  hash TEXT PRIMARY KEY,               -- sha256 of canonical snapshot JSON
  payload TEXT NOT NULL
);

-- runs: index columns + body split
CREATE TABLE runs (
  position INTEGER PRIMARY KEY,
  id INTEGER NOT NULL UNIQUE,
  payload TEXT NOT NULL,               -- run WITHOUT output
  task_id INTEGER NOT NULL,
  occurrence_id INTEGER,
  status TEXT NOT NULL,
  research_kind TEXT,
  has_body INTEGER NOT NULL,
  body_bytes INTEGER NOT NULL,
  started_at INTEGER,
  ended_at INTEGER
);
CREATE INDEX idx_runs_task ON runs(task_id);
CREATE INDEX idx_runs_occ ON runs(occurrence_id);

CREATE TABLE run_bodies (
  run_id INTEGER PRIMARY KEY,
  body TEXT NOT NULL
);

-- metadata as per-field rows so a rename never rewrites the 10 KB blob
CREATE TABLE meta (key TEXT PRIMARY KEY, payload TEXT NOT NULL);
```

Notes:
- `payload` stays authoritative for round-trip equality; index columns are derived and
  verified against `occurrence_activity()` on every write (debug assertion + QA check).
- `occurrence_snapshots.hash`: use SHA-256 of the canonical serialized snapshot
  (`serde_json::to_vec` of the `Value` after key-order normalization by serde). Dedupe is
  lossless: identical bytes restore the same `AutomationOccurrenceSnapshot`. Verify by full
  `Store` equality after reopen.
- `activity_seq` is a per-occurrence counter (or a global monotonic `Store.activity_seq`
  value stored per row); it implements the pagination watermark in §8.
- `research_requests`, `topics`, `publications`, `report_publishing` may remain opaque
  `meta` fields for v4 to avoid colliding with T23/T25 schema ownership. Splitting
  `research_requests` requires T23/T25 review.
- `journal_mode`: recommend `WAL` + `synchronous=FULL` so a read query never blocks behind
  the writer; otherwise every page read can wait up to the 5 s `busy_timeout`. This is a
  persistent DB property change and needs approval (Q4). If WAL is rejected, all reads must
  be served by the single worker connection.

### 11.2 Incremental write algorithm

`apply_patch(conn, shadow, patch)`:

1. begin `IMMEDIATE` transaction;
2. for each upsert: serialize the entity once, extract snapshot/body (`remove` from the
   `Value`), upsert its row with `position = MAX(position)+1` on first insert;
   for runs, upsert `run_bodies`; for occurrences, upsert `occurrence_snapshots` by hash;
3. for each delete: `DELETE` the row (and orphan body/snapshot rows only when unreferenced);
4. for each meta field: `INSERT OR REPLACE INTO meta`;
5. commit; then send `Committed { generation, rows }`.

Invariants:
- Cursor + occurrence in the same patch → same transaction (tick atomicity).
- `runs` row + `occurrence.run_ids` update in the same patch (dispatch atomicity).
- Unique `(automation_id, trigger_key)` **and** the existing domain dedupe prevent duplicate
  occurrences under crash/retry.
- `next_id` meta is written before/with the entity rows (same transaction).

### 11.3 Migration v3 → v4

1. read-only open; `user_version == 3`; otherwise refuse (future) or no-op;
2. read the full logical `Store` (`read_connection`) — this is the only full read;
3. preflight: validate `automation_integrity`, scan duplicate `(automation_id, trigger_key)`,
   ensure every run/occurrence id is unique, and compute all index columns from
   `occurrence_activity` / run fields;
4. backup: `VACUUM INTO workspace.before-schema-v4-<nanos>-<pid>-<seq>.sqlite3` (0600),
   read back, `PRAGMA integrity_check`, full `Store` equality against the original, fsync
   file + parent dir;
5. create the new tables/columns inside one transaction; backfill row by row from the
   already-read store (not by re-parsing SQL JSON, so normalizations apply);
6. verify inside the transaction: row counts equal vector lengths; `PRAGMA integrity_check`;
   `read_connection` (after temporarily setting version in the tx) + full `Store` equality
   with the pre-migration store; `next_id > max(id)`; every `run_ids` / `research_goal_ids`
   / `automation_occurrence_id` resolves; approval signature consistency unchanged;
7. `user_version = 4`; commit; re-open and verify again (defence against a buggy verify);
8. keep the backup and return its path for the existing "backup created" UI.

Failure at any point: transaction rollback, original v3 intact, backup retained, error names
the failing phase. Idempotent: re-running on v4 is a no-op. A crash test kills the child at
each phase (pre-backup, mid-backup, post-backup/pre-transaction, mid-transaction,
post-commit) and the parent reopens and verifies.

### 11.4 Preservation matrix (must be asserted by full `Store` equality + explicit field checks)

| Entity | Preserved |
| --- | --- |
| `tasks` | id, title, description, project/topic/parent, stage, priority, labels, cycle, bucket, due date, comments, activity, archived, attachments, `research` (kind, goal_id, depends_on, requires_approval), `approved_plan`, `automation_id`, `created_at` |
| `projects` | id, name, key, directory (relocated losslessly), brief, kind |
| `topics` | id, project, name, archived, automation_id, replanned_from |
| `automations` | id, version, project, shape, name, objective, expected_result, schedule, status, template, `plan_binding.input_signature` (byte-identical on same-root), approval (version, signatures, plan_request/revision), policy, cursor, anchor, failures, attention, timestamps |
| `occurrences` | id, automation, definition_version, source, trigger_key (dedupe), scheduled/requested/started/retry times, timezone, reference_time, data_window, snapshot (byte-identical), status, reason, goal ids, topic, run_ids, final_run_id, unread, skipped_*, created_at, deadline_at |
| `runs` | id, task, title, directory, prompt, times, session, exit code, agent, model, stage, status, logs, body (moved, byte-identical), attachments, research_kind, sources, input_signature, planning_owner, occurrence link, deadline/retry fields |
| reports/publishing | status, url, commit, fingerprint, message, config |
| global | next_id (monotonic), settings, profiles, appearance, opencode path, default model, automation_schema_version, last_active_project |

---

## 12. Measurement protocol (proposed; **not executed**)

Do not run until root approves this exact recipe. The existing artifact
`.local/automation-perf/51782` must be copied, never mutated.

### 12.1 Fixtures (all generated by an isolated QA entry, no user data)

| ID | Shape | Purpose |
| --- | --- | --- |
| `small` | 500 occurrences, 40 runs, 3 automations, 3 projects | ordinary-workspace latency |
| `occ43200` | byte-for-byte the current generator (43,200 occurrences, 240 runs, 320 tasks, 1 automation, 172.4 MiB) | the TASK-mandated fixture; must match the retained hash |
| `xl` | 129,600 occurrences (3×), 1,000 runs, 6 automations, long English + CJK titles, 1,000 materials | "larger representative history" |
| `body-heavy` | 5,000 occurrences, 1,000 runs with 20 KB and 50 KB bodies, 3 sources each | lazy-body proof (current fixture cannot show it) |
| `retry-heavy` | 43,200 occurrences with 200 retried old periods | pagination/activity-index parity |

Record per fixture: generator seed, logical counts, DB size after migration, snapshot-dedupe
savings, body bytes, file hash.

### 12.2 Spans and samples

- Main-thread spans (µs): `persist.*`, `command.*`, `history.*`, `ack.apply`, plus frame
  duration for the visible route.
- Interaction measurement: 200 iterations of (synthetic rename command / mark-read /
  open first history page / load-more ×5 / open a result dialog), each followed by
  `render_frame`, sampling the persistence-attributable spans and the click→frame latency.
- Streaming: 60 s fake run producing 20 events/s while the same interactions occur; sample
  every iteration. Report p50/p95/p99, max, sample count, hardware, profile.
- Load: cold (drop caches not possible portably; report warm and first-run), DB size, peak
  RSS (`/usr/bin/time -l` or `ru_maxrss`), and body bytes NOT read during load.

### 12.3 Proposed gates (must be confirmed by root; not TASKS wording)

1. p95 main-thread persistence-attributable work **< 50 ms** on `occ43200` (TASK target),
   with zero individual DB/serialization call on the UI thread (grep + runtime assertion).
2. First history page p95 < 30 ms via the real query path; 1,000-row paging p95 < 100 ms;
   ordering identical to `compare_newest_first` on randomized data (contract test).
3. Rename one automation: exactly 1 transaction, 1 `automations` row, 0 run/occurrence row
   writes, 0 metadata-blob rewrite (assert via `sqlite3_total_changes` and a worker
   rows-written counter).
4. Load with lazy bodies: no `run_bodies` read; load time and peak RSS improvement recorded
   (expect a large reduction from snapshot dedupe + lazy bodies).
5. DB size: report before/after per fixture; explain every increase (indexes) and decrease
   (snapshot dedupe, body split).
6. Crash/reopen and concurrency matrix (section 13) all pass, no duplicate dispatch, no
   lost cursor/occurrence, no duplicate trigger key.

### 12.4 Command set for evidence

```text
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-command
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-automation
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf small
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf occ43200
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf xl
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf body-heavy
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf migration
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf crash
PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-persistence-perf concurrency
```

Evidence paths: `.local/t14-perf/<pid>/` (raw), plus copied summaries under
`doc/review-2026-11-08/evidence/` only after root review. Exit codes must be preserved (no
pipeline masking).

---

## 13. Crash / reopen / concurrency / retry verification matrix

| # | Scenario | Injection | Required assertions |
| --- | --- | --- | --- |
| 1 | Crash after commit, before ack | child process + `abort()` at the worker hook | reopen shows the committed state; no duplicate run; UI never claims it saw the ack |
| 2 | Crash mid-transaction | `abort()` between statements / SIGKILL | old state intact after reopen; no partial rows |
| 3 | Crash after ack, before `QueueRun` effect | abort after `Committed`, before effect dispatch | reopen queues the durable run exactly once (existing dispatch recovery) |
| 4 | Crash during migration | kill at each migration phase | v3 intact or v4 complete; backup readable; `integrity_check` ok |
| 5 | Save failure then retry | `BEFORE INSERT` trigger | live state unchanged, "Not saved", Retry writes once, no duplicate |
| 6 | Stale patch rejected | enqueue patch with old `base` | worker refuses before writing; no disk change; main reports conflict |
| 7 | Out-of-order ack ignored | inject a synthetic older `Committed` | `durable_generation` never decreases; no in-memory overwrite |
| 8 | Concurrent insert + page read | worker inserts/retries while pages load | no duplicates within a sequence, ordering matches comparator, watermark rule holds |
| 9 | Retry moves an old period to the top between pages | token retry | row absent until refresh; no skip/duplicate |
| 10 | Duplicate trigger key | insert same `(automation_id, trigger_key)` twice | unique index rejects; app-level dedupe prevents the second attempt |
| 11 | Cursor+occurrence atomicity | crash mid-tick transaction | cursor and occurrence change together or not at all |
| 12 | Dispatch atomicity | crash after `DispatchRun` commit | run + `occurrence.run_ids` present together |
| 13 | Migration duplicate trigger keys in legacy data | seed duplicate keys in a v3 copy | migration fails closed with a named diagnostic; original intact; no deletion |
| 14 | Newer-version file | `user_version = 99` | refuse, no write, no backup churn |
| 15 | Relocation after v4 | move workspace root | paths relocate; body/snapshot tables intact; approvals and signatures unchanged |

---

## 14. Ownership boundaries and integration seams

### 14.1 Required from T13 (freeze before T14-S2/S3)

1. `CommandBase` exposes a stable `generation: u64`; T14 needs `capture(generation)` and
   `matches(generation)` and will delete `exact: Box<Store>` and `state_revision`.
2. `PreparedCommand.staged: Store` must become an entity change set (`StoreChange`) before
   T14-S3. If T13 ships more code depending on `staged`, it must be migrated again.
3. `commit_prepared(sink, live, last_saved, prepared)` splits into
   `enqueue_prepared(...) -> PendingCommit` and `on_ack(pending, &mut live) -> Result<Receipt,
   CommitError>`. The existing stale/reprepare rules and error enum stay.
4. `CommandEffect` execution moves to ack time and needs `effect_guard(&Store) -> bool`
   (drop the effect if a later command invalidated it).
5. `PendingDispatch`/`PendingStop` keep their "no stale run id / journal" semantics;
   `retry_pending_saves` must also retry unacked patches in order.
6. `qa_storage_writes` counts worker commits; `--qa-command` assertions must wait for ack
   (T13 owns `command_qa.rs`; T14 proposes the exact test changes).
7. Stop durability must call `durable_occurrence_status(id)` instead of reading
   `last_saved` (T13-owned `command_app.rs`).
8. `save_current_store_durably()` becomes `commit_now(timeout)`.
9. T13 must not add new whole-store clone rollbacks; use entity rollback so T14-S3 stays
   tractable.
10. The stop journal stays a separate file and is ordered before external stop / store
    commit; T15 must not fold it into SQLite (T13 handoff requirement).

### 14.2 Boundaries with other owners

| Owner | Files | T14/T15 rule |
| --- | --- | --- |
| T13 | `domain_command.rs`, `command_app.rs`, `stop_intent.rs`, `command_qa.rs` | T14 edits only after explicit handover of the executor and command-gate interfaces; no parallel edits. |
| T10 | `navigation*.rs`, draft/creation views, `app.rs` navigation fields | No edits. Any dirty tracking needed in creation flows lands as a coordinated one-line `touch_*` patch after T10 settles. |
| T21 | `research_input.rs`, approval/invalidation previews | No semantic change. `reconcile_goal_approvals` cost work must preserve T21's approval contract; coordinate memoization. |
| T22 | `archive_app.rs`, archive presentation | One-line tracking only; no archive semantics change. |
| T11 | `controls.rs`, row/affordance styling | T15 changes the history **data source** only; row visuals stay T11. |
| T01 | `automation_qa.rs`, QA entry points, `main.rs` dispatch list | T14 adds a separate `persistence_qa.rs` + one `main.rs` line. The 43,200 fixture lives in T01's file: request a narrow parameterization patch, or duplicate the generator in `persistence_qa.rs` for this stage. Do not silently rewrite the baseline fixture. |
| T16 | `automation_activity.rs` ordering | T15's SQL order must be derived from T16's key. Any T16 precedence change must update the index columns and the parity test in the same PR. |
| T17 | `DESIGN.md`, `PRODUCT.md`, `USER_GUIDE.md`, `RESEARCH_WORKFLOW_STRATEGY.md` | T14/T15 request doc changes: "Saved" only after ack; pending/durable/failed states; pagination watermark semantics; incremental history; snapshot dedupe is lossless. Do not edit docs here. |
| T23/T25 | `research_requests` ownership/revisions | Keep `research_requests` opaque in v4 meta; do not add ownership indexes without T23/T25 review. |

### 14.3 Export-crate / dependency policy

No new crate is required: manual `serde` impls for `SharedText`/snapshot sharing avoid the
`serde/rc` feature; the worker uses `std::thread` + `std::sync::mpsc` and the existing GPUI
timer. `sha2` is already a dependency (snapshot hashing). Any dependency change needs
explicit approval.

---

## 15. Risks and open questions for root

| # | Question | Recommended answer |
| --- | --- | --- |
| Q1 | Can T14 change the T13 command-gate contract (generation-only base, entity change set, enqueue/ack split, optimistic apply + exact rollback)? | **Yes, required.** Without it, command prepare alone will exceed 50 ms. Schedule T14-S3 after T13 S2 acceptance; T13 must stop deepening `staged`/`exact`. |
| Q2 | Is the acceptance metric "persistence-attributable main-thread p95 < 50 ms" including command prepare/rollback? | **Yes** — that is the user-visible freeze during a save-triggering interaction. Document the span set in the evidence. |
| Q3 | May T15 dedupe immutable occurrence snapshots into a shared table? | **Yes.** It is lossless (content hash), removes ~63 MB here, and is verified by full `Store` equality. It is the single largest db/memory win. |
| Q4 | May the DB switch to WAL + `synchronous=FULL` so page reads do not wait behind writes? | **Yes, recommended.** Otherwise page reads must all go through the worker (acceptable fallback, but adds async page plumbing). Needs explicit approval because it changes the persistent journal mode. |
| Q5 | On an external file change (stamp mismatch), fail closed or rebase? | **Fail closed**, keep edits in memory, show "Not saved · Workspace changed on disk; reopen". Rebasing patches can silently overwrite another writer. |
| Q6 | Where do the fixtures live and who owns them? | T14 owns `persistence_qa.rs`. Leave `automation_qa.rs::performance_fixture` untouched; either T01 adds a parameterized variant or `persistence_qa.rs` duplicates it. Confirm before running. |
| Q7 | Budget for a command whose change set is large (e.g. cleanup logs over hundreds of runs, pause skipping many queued occurrences) | Accept entity-sized copies; measure; if p95 > 50 ms for that command, split it into bounded batches with progress, not a whole-store clone. |
| Q8 | Must T14 ship before T13 S3/S4 scoped undo? | Memory evidence needs it. Minimum: T13 must remove `Vec<Store>`/`snapshot` or T14 must cap undo snapshots. Do not claim memory acceptance with 30 deep clones possible. |
| Q9 | Does "loading the first history page" include building the in-memory store? | Recommend: yes — measure open→first page; and separately prove that no body deserialization occurred. |
| Q10 | Should `AgentRun.output` be removed (compile-enforced hydration) or kept with a `body_loaded` flag? | **Remove/rename it** (`body: Option<SharedText>` + `body_bytes`). A flag leaves silent-empty-result paths. |
| Q11 | Legacy duplicate `(automation_id, trigger_key)` during migration | Fail closed with a diagnostic and keep the backup; never delete or silently choose one. Root may prefer an explicit user-facing repair flow (out of T15 scope). |
| Q12 | Is re-measuring on the same machine acceptable as "same fixture"? | Yes if the generator is unchanged and the DB hash/counts match; record hardware and profile in the evidence. |

---

## 16. Handoff

- This report is a **read-only audit and a proposed plan**; no T14/T15 acceptance is
  claimed, and no implementation was started.
- Working tree unchanged; no production data touched; no commit, push, restart, or
  expensive benchmark run.
- Recommended next step: root reviews this report, answers Q1–Q12, and authorizes
  **T14-S0 only** (instrumentation + `persistence_qa.rs` + the exact fixture recipe in
  section 12) so the missing baseline is measured before any behaviour change.
- T14/T15 remain **Open**. T13 must freeze `domain_command.rs` / `command_app.rs` interfaces
  (section 14.1) before T14-S2/S3; T01 must confirm fixture ownership before S0 runs.
