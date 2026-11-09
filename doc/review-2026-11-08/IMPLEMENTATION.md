# Implementation and independent review

The authoritative scope and acceptance criteria remain in [TASKS.md](TASKS.md). Implementation is delegated to local OpenCode in bounded batches. A task is accepted only after independent diff review and verification; an agent's completion message alone is not acceptance.

## r143 focused acceptance update

Root independently passed the reviewed modal-focus candidate's navigation-context and keyboard-visual suites; the new rendered-button regression fails before the fix because focus remains on a removed modal control. [Evidence](evidence/r142-focus-offline.json) preserves the exact candidate and scope. Native period-modal keyboard acceptance remains open until the isolated normal build is installed and exercised.

The subsequent [r143 native focus check](evidence/r143-native-focus.json) now passes on the isolated normal build: filtered Period 3 → scrolled report → analysis → Command-[ restores the same period and report position; Command-K opens global Search, Escape returns to analysis, and another Command-[ returns to Period 3. Closing the period preserves the 2-of-5 automation filter. This closes the reproduced period-modal focus defect for this candidate; full integrated keyboard, count-control navigation and scrolling acceptance remain separate. Persistent Saving feedback during this check was forwarded for runtime review and was not counted as a durability pass.

Root independently passed the new live ordinary-edit → automation SaveDraft/Enable/Pause → Undo case with nonempty historical data, and the SQL trigger audit for automation rename, task edit and task Undo. The audit now checks exact row identity, operation and cardinality; the Undo case verifies original task fields on reopened disk as well as memory and preserves complete automation/history state. [Evidence](evidence/r143-live-undo-write-audit.json) identifies the candidate. This strengthens T04/T15 acceptance without waiving outstanding runtime, integrated regression or scrolling gates.

## r145 review update

Native research-input cancellation now passes on the r143 isolated normal build. Saving a changed title first explains approval withdrawal, the three affected steps and retained historical reports. Cancelling the impact dialog preserves the draft; cancelling the editor restores the original visible title and 1/3 progress. [Evidence](evidence/r145-native-impact-cancel.json) is limited to this light-theme cancellation path. Confirmed saving, material changes, historical-version navigation and final integrated acceptance remain open.

The corrected large-history benchmark uses actual collections of 43,200 and 86,400 runs or occurrences, rather than inferring run count from occurrence count. Five bounded samples expose 48.6–93.3 ms first-write occurrence copying at 86,400 entries while a snapshot remains retained. These are diagnostic samples, not quiet p95 acceptance. The seven existing 43,200-entry operation gates pass in the implementer run under load; independent quiet acceptance and native scrolling remain required.

During root QA, an incorrectly ordered command-line flag accidentally launched normal startup and upgraded the configured production database from v3 to v4. After the user's explicit restoration instruction, root compared all original business data and confirmed zero new commits, held the workspace lock, preserved the upgraded database, restored the automatic v3 backup, and verified integrity and exact backup hash equality. Production was not relaunched. Local recovery evidence is retained outside Git. The reviewed CLI guard now rejects unsupported arguments before workspace startup; [evidence](evidence/r144-cli-guard.json) records the checks.

## r146 focused verification and remaining defects

Root independently reran command QA, dispatch-admission QA and all 23 sequential live-persistence cases on the immutable COW candidate `0f336d3f236b4cd0b98dc05431a956fb7e51e06d7908e14bf2c99956c0f26b9f`; all passed. This candidate excludes the pending runtime changes and later strengthened T04/T15 cases, so this is functional evidence rather than final integration or smoothness acceptance. [Evidence](evidence/r146-focused-checks.json) also records native Version history → Escape → Search → Escape → Back on r143.

Native material import exposed an incomplete path: after selecting a valid local PNG, the UI claimed the material would be included in the next run while the count remained zero and no impact review appeared. The runtime keep-alive condition omits pending material verification; the import receiver can finish before the worker response. The runtime implementer must repair wake/keep-alive behavior and premature success feedback, with idle-executor tests covering review, cancellation cleanup and durable confirmation. A subsequent Mac lock prevented further native observation; final import outcome is not claimed.

Large-history diagnosis distinguishes actual command staging from direct runtime writes. `SetRead` prepares a cloned Store and copies the occurrence collection on first mutation; normal live runtime writes generally do not retain that clone. The initial suspicion that a no-op live edit repopulates the sparse durability baseline was disproven in r149: the real live edit branch returns before the fallback adoption code, and measured no-op-save-followed-by-write remains fast. Direct, command and retained-snapshot writes are measured separately to identify the narrow repair. The retained-snapshot diagnostic remains valid but must not be described as the cost of every runtime mutation.

The earlier xctrace surface IDs identify display buffers, not application windows; joining them does not establish app attribution. A private diagnostic build is being prepared to record GPUI per-window draw and input latency through CPU submission. Display scan-out remains a distinct boundary. This instrumentation and the static-logo comparison are diagnostic only; final native scrolling acceptance still requires the complete workload and surface matrix.

## r149 measured command-path bottleneck

The new actual-occurrence benchmark separates direct mark-read, real command toggle, no-op form save followed by mark-read, and explicitly retained snapshots. At 86,400 occurrences, five diagnostic command samples take 56.9–66.1 ms while direct writes take 21–27 microseconds. The real no-op form path does not reproduce the suspected full-history retention; source inspection confirms the live edit branch returns before the fallback Store adoption. That earlier hypothesis is withdrawn. [Evidence](evidence/r149-occurrence-paths.json) records the candidate, source/report identity and per-path ranges; these are bounded diagnostics under concurrent work, not quiet p95 acceptance.

A narrow live `SetRead` path is now assigned to avoid full Store command staging while preserving primary-save admission, exact tracked writes, ordered receipts, failure/retry and non-undoable read-state behavior. The existing legacy command path remains. Independent review and before/after verification are required before acceptance.

## r151 acceptance scope adjustment

The user explicitly authorized completing a full first review round while recording nonblocking wording and rare/extreme combinations for later optimization. [The authority policy](TASKS.md#first-round-acceptance-policy-update) now reflects this. Implementers have been instructed to finish the current narrow repairs and essential regressions, return immutable candidates, and separate nonblocking gaps from failures. Root retains final classification; no current data/execution, material-save, common-navigation or reproduced scrolling blocker is waived by this update.

For the step-count control, the ordinary click/Back path with real filtered, scrolled lists and a typed deferred Steps destination remains required. Exhaustively crossing every dirty-exit decision with every collection scope is a follow-up candidate once existing generic dirty-exit tests and a focused Steps-target replay test pass; it must not become a new combinatorial release gate. The repeated invalid-fixture build was interrupted and corrected before further validation.

## Development checkpoint policy

All work for this optimization goal uses `feature/review-2026-11-08-optimization`. The user authorized periodic checkpoint commits and pushes on 2026-11-09 to prevent code loss. Checkpoints may contain unfinished work and failing acceptance cases; they are not releases or task sign-off. Root owns Git staging, commits and pushes. Implementers continue on this same shared branch. Local `.local/` workspaces, binaries, runtime traces and credentials remain excluded by repository policy. Validation screenshots also remain local and must not be committed or pushed. The initial checkpoint predated this screenshot instruction; cleanup commit `e60ab38` removed all 60 tracked review screenshots from the branch tip and added ignore rules, preserving local copies. Earlier Git history was not rewritten.

## Current full-scope status

All 25 tasks remain in scope. Accepted means the reviewed candidate met its task criteria, not that the entire application is released. Later changes must preserve accepted behavior. The chronological notes below retain earlier intermediate failures.

| Task | Scope | Current disposition |
| --- | --- | --- |
| T01 | Repair and classify regression coverage | r139 full 54-entry offline ledger has passing coverage after retained path-only reruns and a reviewed topics-contract correction; original failures remain recorded. New runtime changes require revalidation. |
| T02 | Remove background navigation from run submission | Offline background-navigation coverage passes; r140 ordinary E24 completion kept the historical period route unchanged. Full final native matrix remains open. |
| T03 | Separate retrying a period from resuming its schedule | Focused lifecycle acceptance retained and r137 automation matrix passes; final native/release gates remain. |
| T04 | Prevent whole-Store Undo from erasing automation changes | Whole-Store Undo variant removed; current offline checks pass. r143 independently passes live edit → automation save/enable/pause → Undo with nonempty historical data and reopened-disk checks. |
| T05 | Share execution eligibility with status presentation | Shared eligibility implemented and offline checks pass. Native r139 exposes a real paused queue caused by async admission integration; runtime repair remains required. |
| T06 | Introduce typed result presentation | Typed results and lazy-body repair pass offline; native saved Quick and Research reports show Complete and correct current/original-version labels. Full state matrix remains open. |
| T07 | Keep task neighbors within the active navigation domain | Period-bounded neighbors pass offline; final native boundary matrix remains required. |
| T08 | Give Search a stable global contract | Global-search and keyboard suites pass in r137 coverage. The reproduced period-modal shortcut failure passes r143 native focus verification; final integrated boundary coverage remains required. |
| T09 | Reset new forms from one explicit default model | Explicit draft defaults implemented and offline creation checks pass; final native integration remains required. |
| T10 | Preserve origin, dirty drafts and creation destination | Typed caller/filter/scroll restoration passes offline. r138 Archive and r140 filtered period pointer-return paths pass; native shortcut/focus and full dirty-save matrix remain open. |
| T11 | Restore consistent action affordances and visual hierarchy | Action hierarchy implemented; r137 compact and normal Light/Dark result rows reviewed with visible Open controls. Full native surface matrix remains open. |
| T12 | Extract a typed navigation controller | Typed route/overlay/return authority implemented with passing offline navigation. Further payload relocation is explicitly deferred in TASKS; behavioral and native gates remain mandatory. |
| T13 | Extract domain commands and scoped undo | Typed commands and scoped Undo implemented; actual first-dispatch/batch async integration is under repair. Whole-history preparation costs await expanded measurement. |
| T14 | Move persistence off the UI thread with ordered acknowledgements | Live ordered receipts and 23-case persistence suite pass in r137. Native first-dispatch wake/batch failure blocks delivery. Expanded benchmark setup failure is retained; final quiet p95 remains open. |
| T15 | Add incremental writes and indexed history queries | Incremental v4 writes, indexed pages and lazy selected bodies implemented. Saved-body native repair verified; r143 exact row-write audits pass; larger-history first-write copying remains under review. Smoothness remains open. |
| T16 | Unify temporal ordering and harden schedule arithmetic | Temporal ordering/arithmetic implementation and offline checks pass; final integrated/native boundaries remain documented. |
| T17 | Reconcile current product/design/workflow documentation | Current status, route, pagination and save documentation is being reconciled against r139/r140 evidence; chronological earlier claims are historical. |
| T18 | Make unattended capability guarantees accurate | Capability wording distinguishes prompt guidance from a runtime sandbox; final behavior/document consistency remains part of release review. |
| T19 | Verify keyboard behavior and visual readability | r137 keyboard/visual QA passes and both result-row sizes/themes reviewed. The period-modal Command-[ recurrence is fixed and natively verified in r143; count-control navigation and app-attributed scrolling remain open. |
| T20 | Run the full regression matrix for release acceptance | 54-entry offline coverage reconciled with original failures and supplements. Full native acceptance, repaired runtime integration and performance gates remain open. |
| T21 | Explain and control research-input invalidation | Impact/history/scoped-Undo checks pass in the r137 matrix. r145 native impact explanation and cancellation pass; confirmed saving, historical-version focus and material paths remain required. |
| T22 | Present archived research as archived history | Archived membership and pointer/shortcut control paths repaired and tested; fresh completed 3/3 archive/restore native flow remains blocked by ordinary execution. Recurrent shortcuts remain open. |
| T23 | Persist run ownership across planning attempts | Immutable planning ownership passes offline planning/history coverage; current native repeated replan/restart/history replay remains required. |
| T24 | Reconnect automation workspaces using the correct directory policy | Directory-policy fix and isolated reconnect/relocation acceptance retained; r137 workspace suite passes. The T24 checks used isolated data. The later root CLI invocation incident and authorized restoration are recorded above. |
| T25 | Bind replanning to the edited request revision | Immutable edited-request revision behavior passes offline planning coverage; current native repeated replan/enable/history replay remains required. |

## Batch 1: T24

- Status: accepted for T24 in the working tree after independent review and native verification; not committed or released.
- Implementer: local OpenCode v2.2.6, `opencode-go/deepseek-v4.1-flash` (switched from `tencent-token-plan/hy4` at the user's request).
- Session: `ses_e457ee573ffeBXRs6rW7w1svNC`.
- Scope: project-kind directory policy, reconnect, migration, portable relocation, and isolated regression coverage.
- Dependencies: none. T01 baseline failures remain separately tracked.
- Boundaries: no commits, pushes, live workspace changes, or restart of the user's app; existing review documents are preserved.
- Dispatch prompt: `.local/opencode-dispatch/T24-prompt.txt` (local working artifact).
- Execution log: `.local/opencode-dispatch/T24-implementation.jsonl` and `.local/opencode-dispatch/T24-deepseek.jsonl` (local working artifacts).
- Initial default-model attempt was rejected by the provider's training-data privacy setting before implementation. That setting was not changed. The OpenCode Go endpoint was independently probed successfully. The initial implementation was interrupted through `session.interrupt` before resuming the same session with the requested Go model.

Other tasks remain pending. Full-program acceptance requires T20 and is not implied by acceptance of an individual batch.

## T24 independent review and acceptance

Changed files: `src/workspace.rs`, `src/app.rs`, `src/workspace_qa.rs`. Directory creation, explicit reconnection and migration use the project-kind directory policy. Runtime paths additionally require whole-workspace containment before either kind root is considered.

The first implementation was returned for revision: the runtime helper could accept an external folder when the kind root itself was a symlink, and the initial QA fixture contained no actual automation definitions. OpenCode addressed both. The final QA uses Quick and Research definitions, saved periods and a report. No UI layout, icon or research execution contract changed.

Independent verification on 2026-11-08:

| Check | Result | Evidence |
| --- | --- | --- |
| `cargo test --quiet` | 296 passed, 1 failed, 2 ignored; exit 101. The sole failure is the existing missing-Node report build test, unchanged from baseline. | [Full log](evidence/T24-tests.log) |
| `cargo run --features qa --quiet -- --qa-onboarding` | Passed, exit 0; includes mixed automation reconnect/relocation and existing upgrade/retry/lock checks. | [QA log](evidence/T24-onboarding.log) |
| `git diff --check` | Passed | Working-tree check |
| Native Set workspace, original isolated data | Passed: removed only the isolated bootstrap, entered the existing root and clicked Set workspace; ordinary research and both automation kinds loaded. | Computer walkthrough in this conversation |
| Native Set workspace, copied portable workspace | Passed: connected `.local/native-review/T24-relocated-data`; four automation results appeared and the historical Period 4 report opened with its saved content and source. | Computer walkthrough in this conversation |

Native verification used the rebuilt `Planning Native Review.app`, with only isolated identity/bootstrap overrides. The production app was not restarted. Unit tests cover ordinary-only reconnection, mixed reconnection, relocation, migration preserving original files, wrong-kind paths, traversal, external folders and root-level symlink escapes, including missing descendants. Store-level QA directly calls initialize and is explicitly distinguished from the separate native button walkthrough.

The pre-existing `--qa-workspace` stale `save-task` selector still fails and remains T01 work. T24 acceptance does not imply a green full suite or release approval. No commit or push was made. The isolated native app remains open on the relocated fixture report for inspection.

Next suggested batch: T02 (background navigation) and T03 (period retry versus schedule resume), each dispatched and reviewed separately before broader navigation abstractions. T25 should be coordinated with T23 request/run ownership.

## Batch 2: T02 and T03

- Status: implementation in progress; not accepted.
- Implementer: local OpenCode, `opencode-go/deepseek-v4.1-flash`.
- Session: `ses_e4562d0a9ffeQ1PjZz1wwplVFq`.
- Prompt/log: `.local/opencode-dispatch/T02-T03-prompt.txt`, `.local/opencode-dispatch/T02-T03-run.jsonl`.
- Scope: passive background dispatch, explicit foreground navigation, and occurrence retry independent of future schedule state.
- Independent review will verify all task acceptance cases and send concrete revision instructions for any failure.

The full 25-task program remains active. Only T24 is accepted so far; no task is implicitly waived. Node 24 was installed through Homebrew to provide a real publishing-test runtime; verification commands use `/opt/homebrew/opt/node@24/bin` in their process-local PATH without changing shell configuration.

## T01 environment and diagnostic progress

- Real Node runtime: Homebrew Node 24.22.2, process-local PATH `/opt/homebrew/opt/node@24/bin:$PATH`.
- Independent node-enabled baseline: `cargo test --quiet` passed with 297 passed, 0 failed, 2 ignored. This supersedes the missing-Node failure as a current environment limitation; the historical logs remain valid records of earlier runs.
- Evidence: [node-enabled unit baseline](evidence/T01-node-enabled-baseline.log).
- OpenCode read-only diagnostic session: `ses_e455e97b5ffeuzzaxHfI7s3RO1`, model `opencode-go/deepseek-v4.1-flash`. It may write only `.local/opencode-dispatch/T01-diagnosis.md`; implementation and full QA verification are still pending.
- T02/T03 was interrupted once after excessive broad exploration and resumed with concrete implementation checkpoints in `.local/opencode-dispatch/T02-T03-focus.txt`. Active execution log is `.local/opencode-dispatch/T02-T03-focused.jsonl`.

## T02/T03 first independent review: changes required

Status remains **not accepted**. The first patch separates dispatch from explicit progress navigation and adds retry state tests, but review found these blockers:

1. The period UI disables historical retry when an edited definition is Draft, even though the domain command accepts its old snapshot. Exercise the actual button after editing.
2. Old template validation uses the current mutable definition shape. Preserve historical shape semantics and add a changed-shape case.
3. Visible Resume labels still conflate period continuation/retry with future schedule resumption.
4. Ordinary background continuations and automatic retry need route/selection/focus preservation evidence comparable to the scheduled automation scenarios.
5. Verification commands must preserve actual exit codes.

Returned to OpenCode in the same session with `.local/opencode-dispatch/T02-T03-review-notes.txt`; active revision log is `.local/opencode-dispatch/T02-T03-revision.jsonl`. No acceptance is inferred from the implementation agent's 18 passing automation runtime tests.

## T01 implementation dispatched

Read-only diagnosis is in `.local/opencode-dispatch/T01-diagnosis.md`. Implementation now runs in the same T01 session, restricted to files not owned by T02/T03. Prompt/log: `.local/opencode-dispatch/T01-implement-prompt.txt`, `.local/opencode-dispatch/T01-implementation.jsonl`.

Supervisor decisions: preserve manual completion for ordinary non-research child tasks with explicit research-step dependency/approval regression guards; align the toolbar to DESIGN's 32 px height rather than weakening its alignment assertion. Stale fixtures must exercise current entry points; recovery must resolve the fake running-session blocker rather than extend timeouts. Full 33-entry sweep is deferred until concurrent implementation settles. T01 remains not accepted.

## T16 arithmetic subtask independently verified

- Implementer session: `ses_e454ab762ffeB87Vx6ISZ8LbAT`, `opencode-go/deepseek-v4.1-flash`.
- Changed module: `src/automation_schedule.rs` only for this subtask.
- Independent command: `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet automation_schedule::` — exit 0, 20 passed.
- Evidence: [independent schedule tests](evidence/T16-arithmetic-tests.log).
- Intervals use overflow-safe i128 index arithmetic and return zero for empty trigger ranges. Calendar counts are exact up to 36,600 local days and explicitly error for larger spans, rather than silently truncating at 4,096 triggers. Preview/dispatch use the same local-time resolution, with DST gap/fold tests.
- Full T16 status remains **Open**: latest-activity ordering across list/history and related UI verification are still pending. No full-task acceptance is implied.

Reviewed schedule module SHA-256: `86a78b01f802b898979823d427bae140ece0a6a395e935b6f5c46e56dbab5d9d`.

## T02/T03 second independent review: completion callbacks

Independent intermediate checks passed: 313 unit tests, 0 failed, 2 ignored; the full automation QA passed including settings/report/dirty-form/period-dialog preservation, edited-definition period retry, and ordinary continuation/auto-retry checks. These results are intermediate and are not acceptance of the final revision. Evidence: [unit log](evidence/T02-T03-intermediate-unit.log), [automation QA log](evidence/T02-T03-intermediate-automation.log).

Code inspection still found background navigation in project/request planning completion and goal-plan completion, plus a global result-panel toggle. This was returned to OpenCode with `.local/opencode-dispatch/T02-completion-review.txt`; active log is `.local/opencode-dispatch/T02-T03-revision2.jsonl`. The review also requires preserving explicit stored snapshot shape while inferring only genuinely absent legacy shape.

T01 received a `session.prompt` steer instruction through the local OpenCode API to update stale QA navigation assumptions using explicit user actions rather than restore automatic completion routing. Session ownership and model remain unchanged. T02/T03 and T01 are not yet accepted.


## T02/T03 final candidate checks and native retry

Independent final candidate checks passed: 314 unit tests, 0 failures, 2 documented opt-in ignores, and full automation QA including completion boundaries, ordinary continuation/retry, and preservation of settings/report/dirty-form/period-dialog state. Logs: [unit](evidence/T02-T03-final-unit.log), [automation](evidence/T02-T03-final-automation.log). Native binary source hashes are recorded in `.local/opencode-dispatch/T02-T03-native-source.json`; later worktree edits are not implicitly covered by these results.

Native isolated app verification: opened paused Quick definition, ran a manual period with a slow synthetic CLI, stopped it, opened the cancelled period, clicked **Retry period**, and observed the same **Period 5** complete successfully while its result dialog remained open. Closing the dialog retained **Resume schedule**, confirming the definition remained paused. Persisted state records occurrence 5 with original reference/start time, run IDs 18 and 19, retry timestamp and final success; definition 1 remains paused. See [state](evidence/T03-native-retry-state.json) and [screenshot](evidence/T03-paused-after-retry.png). Snapshot identity/cursor invariants are additionally covered by runtime tests.

The separate native ordinary planning experiment remains **incomplete**: request run 20 was queued behind earlier fixture work. Staying on Automations while queued does not prove completion preservation. The corresponding screenshot is diagnostic only, not passing completion evidence. Automated completion-boundary tests pass. T02 remains pending final sign-off including T01; T03 awaits final requirement-by-requirement review before status change.

## Batch 3: T04 and T09 dispatched

OpenCode is implementing conservative automation Undo invalidation and complete draft initialization/reset using `opencode-go/deepseek-v4.1-flash`. Prompt/log: `.local/opencode-dispatch/T04-T09-prompt.txt`, `.local/opencode-dispatch/T04-T09-run.jsonl`. T01 owns its baseline QA files concurrently; this batch is restricted to non-overlapping automation/app files and targeted tests. Neither task is accepted.


## T03 independent review and acceptance

**Accepted in the reviewed working-tree candidate**, not committed or released. The final review verified separate retry/resume commands, archived/ownership/approved-snapshot rejection before mutation, original snapshot execution after edited Draft and shape changes, preservation of completed steps/reference identity, retry deadline handling, and future-only schedule resume. Tests compare the full definition before/after retry, and failure cases preserve the occurrence. The UI uses the same blocker, exposes approved version and edited-version context, and no longer resumes the schedule from period retry. Native cancellation/retry confirmed successful same-period execution with the definition still paused. Evidence is linked above. Subsequent integration edits must rerun these regressions; T01 and T20 remain release gates.


## T02 native completion verification resolved

The native fixture CLI lacked `session.active` and transcript export responses, so the real recovery guard correctly held the ordinary queue. The isolated fixture now supplies stopped-session responses tied to its own stored request. Clicking **Check progress** resolved the fixture-only blocker without changing production recovery rules.

A fresh slow planning request (run 21) was observed Running before navigating to Automations. After it completed successfully at 19:01:19, the app remained on Automations; only the sidebar topic/goal appeared. No completion-driven navigation occurred. Evidence: [running](evidence/T02-native-plan-running.png), [completed](evidence/T02-native-plan-completed.png), [persisted run](evidence/T02-native-plan-state.json). This replaces the earlier queued-only experiment as native completion evidence. T02's final sign-off still depends on T01's required full gate.

## T04 early review feedback

Returned a concrete ownership concern while implementation is active: preserving automation/occurrence vectors during whole-store Undo is insufficient if restored tasks/projects/topics orphan those records. Require invalidating/rejecting incompatible snapshots before restoration, including direct Undo before persistence, while retaining safe ordinary edits after automation changes. Review is pending the revised candidate.


## T01 research fixture diagnosis and T18 preparation

Independent inspection of `.local/research-qa/86710/workspace.json` showed the first collector attempt failed the fake CLI's obsolete `BROWSER INTEGRATION` / `Task Manager research` assertion. Three automatic retries then failed the fixture's mandatory `--file` assertion. This is not proof of a successful chain, and changing the expected run count would hide the failure. OpenCode was instructed to update the first assertion against the current browser integration contract and separately classify retry attachment behavior (retained session evidence versus required file retransmission), without weakening evidence preservation or modifying frozen inputs.

T18 read-only audit dispatched with `opencode-go/deepseek-v4.1-flash`, prompt `.local/opencode-dispatch/T18-audit-prompt.txt`, log `.local/opencode-dispatch/T18-audit.jsonl`, planned report `.local/opencode-dispatch/T18-audit.md`. Product implementation remains pending. The audit covers objective/expected answer/generated plan/inherited instructions and real tool restrictions, not merely keyword matching. This session may not edit product or maintained documentation while other tasks own those files.

Prepared independent sequential 33-entry gate runner `.local/opencode-dispatch/run_release_gate.py`. It records source/binary hashes, exact exit codes, per-entry logs and timings, and uses a SQLite backup of an isolated fixture for render performance. It has not been run against the still-changing candidate and is not passing evidence.


## T09 shape-switch review: changes required

The new automation QA passed Undo protection and new-draft defaults, but the shape-switch case failed: the stored version remained 1 rather than advancing to 2. Independent source inspection found that `save_automation_form` compares the desired shape but `revise_automation` does not accept a shape argument; it validates the new template against the old definition shape. Returned to OpenCode with instructions to stage and validate shape/template/version/approval atomically in the domain command, update callers, and preserve T03 historical snapshot semantics. No acceptance is inferred from the two passing new cases; the complete targeted suite must pass after correction.


## T18 audit reviewed; isolated implementation started

[Capability audit](evidence/T18-capability-audit.md) confirms objective-only heuristics, unchecked expected-result input, prompt-only external-action guidance, and unchanged shared OpenCode/Codex runtime permissions. No unauthorized external write was observed. Supervisor decision: make the information-work intent and residual runtime permissions explicit, strengthen shared input validation, and avoid a false sandbox guarantee or a separate automation agent configuration. Preserve local report writing and allowed evidence collection.

The same OpenCode session (`ses_e452c6763ffeClysWYDo3qcE0K`) is implementing only `src/automation_policy.rs` plus module registration and focused unit tests while T04/T09 owns integration files. Prompt/log: `.local/opencode-dispatch/T18-policy-module-prompt.txt`, `.local/opencode-dispatch/T18-policy-module.jsonl`. Generated/inherited instruction paths, UI/domain integration and maintained documentation remain required follow-up work. Expected-result planning context must be coordinated with T25; the current misleading planner implication cannot remain. T18 is not accepted.


## T04 independent acceptance; T09 late completion returned

Independent verification of the reviewed candidate passed: 314 unit tests, 0 failed, 2 documented ignores, and the full automation QA including Undo protection, draft defaults and synchronous shape-switch checks. Evidence: [unit](evidence/T04-T09-independent-unit.log), [automation QA](evidence/T04-T09-independent-automation.log). Source hashes are in `.local/opencode-dispatch/T04-T09-reviewed-source.json`.

**T04 accepted in this working-tree candidate**, not committed or released. Review confirmed centralized invalidation across durable automation definitions/occurrences and their owner graph; direct Undo rejects a mismatched snapshot before any restoration. Tests cover ordinary edits followed by pause/save/enable/scheduled cursor-and-occurrence change, unsafe direct Undo before persistence, and subsequent safe ordinary Undo. The disabled control exposes the invalidation reason. No partial merge can orphan preserved automation records. Later integration changes and T01/T20 release gates remain required.

**T09 remains unaccepted.** Independent code review identified a late-completion gap: `select_automation_shape` clears the planning pointer, but `automation_plan_ready` also accepts the still-matching editing ID, allowing old completion to restore a discarded plan or advance wizard state after a type switch. Returned to OpenCode with `.local/opencode-dispatch/T04-T09-review2.txt` / `.local/opencode-dispatch/T04-T09-review2.jsonl` for request/generation-aware handling and a regression using actual planning completion, including switch-away-and-back. Also requested atomic failed shape/template validation evidence. Passing synchronous tests do not prove this edge.


## Recovery and policy review follow-up

Independent inspection of the isolated recovery fixture `planning-recovery-qa-91970/workspace.sqlite3` explained the old 10-run assertion: after the formerly permanent-running session becomes stopped, Resume all correctly admits its three retained unfinished steps as well, producing 13 total records. New attempts succeeded for tasks 6, 3, 4, 10, 11, 12, 7 and 8. OpenCode was instructed to assert the precise admitted task set and unique attempts, preserve completed-result reuse, and keep unknown-session/never-started exclusions; a total-count edit alone is insufficient evidence. No scheduler behavior change is justified by this fixture mismatch.

The T18 policy prototype initially skipped every quoted action, including direct quoted imperatives. Review returned analytical-description versus instruction-context cases, plus contraction/apostrophe handling, for correction. Passing heuristics will remain input validation only; this does not create an external-write capability boundary. T18 is not accepted.

The independent release runner now copies the candidate executable before the sweep, ensuring all 33 dispatch entries use the same binary even if later builds occur. Full sweep remains pending candidate readiness.


## T18 policy module verified; T07 dispatched

The isolated policy module passed independent `rustc --edition 2024 --test src/automation_policy.rs` execution: 14 passed, 0 failed. [Evidence](evidence/T18-policy-independent.log). It includes field-specific validation, optional expected-result behavior, negation/quoted-description handling, quoted imperative rejection, and Unicode-safe boundaries. These tests prove heuristic behavior only; there is no runtime capability enforcement. The module is not yet connected to application/domain entry points, so **T18 remains open**. Reviewed SHA-256: `5306854f7330c6f1e65510d493a02744c72a8af76c97cfe30336aab0ec5905af`.

T07 dispatched to OpenCode with `.local/opencode-dispatch/T07-prompt.txt` / `.local/opencode-dispatch/T07-run.jsonl`, model `opencode-go/deepseek-v4.1-flash`. Ownership is limited to navigation helpers, neighbor/recent-menu sections and existing sidebar QA; T01/T09 were notified to avoid those sections. Required behavior includes non-wrapping within-period steps, separate overview, contextual recent entries and safe missing-target fallback, while ordinary navigation remains unchanged. No acceptance is implied.


## T09 independent acceptance

**Accepted in the reviewed working-tree candidate**, not committed or released. Explicit new/from-definition draft constructors reset all owned inputs together, while Back within a draft preserves them. Shape revision validates a staged definition with its new shape before replacing any stored state; failed cross-shape template validation leaves the entire definition unchanged. Completion now requires matching automation and run IDs plus Research shape; switching away/back clears that binding, so late completion retains saved history without repopulating the abandoned UI draft or advancing its wizard.

Independent final checks: 328 unit tests passed, 0 failed, 2 documented ignores; full automation QA passed including Undo, defaults, type switching, actual completion callback and atomic-rejection scenarios. [Unit evidence](evidence/T09-final-independent-unit.log), [automation QA](evidence/T09-final-independent-automation.log).

Computer verified the frozen isolated native candidate: set weekly + Sunday + SkipAll, go Back and Continue (values retained), Cancel then New (empty fields, Daily 10:00, Asia/Shanghai system timezone, LatestOnly), select Hours then Cancel/New and revisit Fixed interval (Minutes selected). No test draft was saved. [Back retention](evidence/T09-native-back-preserves.png), [new defaults](evidence/T09-native-new-defaults.png), [minute unit reset](evidence/T09-native-minute-unit-reset.png). The full remaining default limits/weekdays and changed edit-form cases are asserted semantically by QA. Native source hashes/build log: `.local/opencode-dispatch/T09-final-native-source.json`, `.local/opencode-dispatch/T09-final-native-build.log`. Only the uniquely named isolated app was restarted; the user's original app was not touched.


## T18 integration dispatched; T25 expected-output contract clarified

T18 now implements the remaining input-validation integration and truthful capability copy in the same OpenCode session, using `.local/opencode-dispatch/T18-integration-prompt.txt` / `.local/opencode-dispatch/T18-integration.jsonl`. It owns automation policy/domain/form/QA files and capability documentation; T01 and T07 files are excluded. Required checks include expected-only unsupported input, generated template instructions, atomic rejection, legacy readability and a benign fake-CLI control. No runtime sandbox guarantee or second automation agent configuration is introduced.

The audit also confirmed Research expected-result text is currently omitted from planner context. T18 must correct misleading helper copy immediately. T25's existing expected-output revision requirement is now explicit in its acceptance criteria: expected-only edits bind the planner prompt and returned plan to a new immutable input contract, including conservative legacy handling. This behavior is still pending T25; it is not silently counted as completed by a copy correction.


## T01 checkpoint and sidebar QA ownership correction

OpenCode checkpoint reports five original failures now passing: affordance, research, workspace, recovery and publishing. Topics and simplify remain under repair; all seven must be rerun together before the independent 33-entry gate. Supervisor confirmed two stale fixture assumptions: topic menu actionable indices are Move 0 / Rename 1 / Archive-or-Restore 2, and a completed sidebar step intentionally opens its result (`selected=None`, run scope, matching task-result context). Replacement assertions must verify the clicked step's actual displayed result, not merely relax selection expectations.

The T07 dispatch named a nonexistent `sidebar_qa.rs`; the actual existing entry is `topic_qa::sidebar()`. Ownership was corrected in both sessions: T07 may change that function and distinct helpers, while T01 keeps `topic_qa::run()` archive/menu repairs. Narrow edits preserve concurrent work. This correction does not add a dispatch flag or reduce coverage.


## T07 focus correction and T18 instruction-bearing titles

T07 was deliberately interrupted after approximately nine minutes and 57 tool calls with no edits, then resumed in the same session (`ses_e451d95bdffeh9ODrmU7eyl32R`) with concrete implementation checkpoints. Interruption was confirmed by the API and the original process exited; this was not inferred from a timeout. New prompt/log: `.local/opencode-dispatch/T07-focus.txt`, `.local/opencode-dispatch/T07-focused.jsonl`. Scope and acceptance criteria remain unchanged.

T18's in-progress test incorrectly classified generated goal/step titles as display metadata and accepted explicit external-action titles. Independent source review (`research.rs` prompt construction and plan signatures) confirms those titles are instruction-bearing. Returned for shared validation of obvious requests in those fields while preserving benign research vocabulary. Also required a real Storage save/reopen legacy fixture plus atomic enable/reapproval rejection; serde-only round-trip is not sufficient compatibility evidence. Structural workspace integrity checks should remain separate from current action eligibility.

## T07 and T18 integration review checkpoint

Both candidates remain unaccepted. Independent source review found that T07 UI QA opened the recent-task menu without activating an entry, and only checked that the period-overview control existed. OpenCode was asked to verify the actual destinations and assert the exact owning period, including stale target ownership. T18 reported 339 passing unit tests with two ignored tests, but its actual Quick form rejection case advanced to step 2 instead of remaining on step 1. OpenCode must first verify the input values delivered by the UI harness, diagnose the failure, and preserve the expected rejection assertions. The generated goal/step titles are now scanned because they enter agent prompts; a contradictory display-only comment was returned for correction. These observations do not satisfy either task or the full release gate.

### T07 intermediate independent unit verification

`PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet navigation::tests` passed independently: eight passed, none failed, exit 0. [Log](evidence/T07-intermediate-independent-unit.log). This is an intermediate source check, not final candidate acceptance: UI destinations, missing/moved ownership guards and the full release gate remain pending. T07 QA compilation temporarily blocked the other sessions; its owner was asked to prioritize compilation and preserve command exit codes without shell-dependent pipeline masking. The next T16 activity-ordering dispatch has been prepared from the current three inconsistent ordering call sites, but is intentionally not running while T18 owns those files.

## T18 independent acceptance

Status: accepted in the reviewed working-tree candidate, not committed or released. The product now states the actual shared-runtime contract: heuristic checks and prompt guidance do not impose a sandbox; approved agents retain their normal permissions. The audit and maintained documentation identify residual unchecked instruction sources and do not claim exactly-once external operations. Strict external-write prevention is not retained as a product guarantee.

Objective and expected-result checks apply at Continue, create, revise and approve; generated goal/step titles, briefs and instructions are validated before template acceptance because they enter prompts. Domain tests preserve rejected definitions atomically, and real SQLite save/reopen QA proves unsupported legacy drafts remain readable but cannot be enabled. The standalone policy suite covers variants, mixed clauses, quoted instructions/descriptions and negations without claiming exhaustive language recognition.

Independent verification: full unit suite passed (340 passed, zero failed, two explicitly opt-in tests ignored; exit 0), and full automation QA passed (exit 0, fixture `.local/automation-qa/5519`). The first independent build attempt encountered T07's in-progress constructor signature edits and failed compilation; the successful retry followed their repair. [Unit log](evidence/T18-independent-unit.log), [automation QA log](evidence/T18-independent-automation-retry.log), [reviewed source hashes](evidence/T18-reviewed-source.json).

Native verification used a frozen rebuilt isolated app and disposable workspace through Computer. Quick and Research Continue each rejected an unsupported request located only in expected-result text; the input stayed editable on step 1. Changing it to a local summary with the restriction “Do not send email” advanced to schedule configuration. The draft was cancelled without saving, generating a plan or executing research. Light and dark error text was readable: [Quick light](evidence/T18-native-quick-rejection.png), [Research dark](evidence/T18-native-research-rejection-dark.png). The isolated theme was restored to Light; the user's installed app was not restarted.

T25 remains responsible for passing expected-result context into immutable planning revisions. The current Research helper truthfully identifies that unfinished behavior; T18 acceptance does not waive T25 or the full T20 release gate.

## T16 activity-ordering implementation dispatched

The arithmetic subtask remains verified. The remaining list/history activity contract is now assigned to OpenCode session `ses_e454ab762ffeB87Vx6ISZ8LbAT`, using `opencode-go/deepseek-v4.1-flash`. The dispatch is `.local/opencode-dispatch/T16-activity-prompt.txt`; execution log is `.local/opencode-dispatch/T16-activity.jsonl`. Ownership is limited to named ordering helpers, the latest/list/history presentation sections and their tests. The accepted T18 input validation and T03 retry contracts must be preserved. Acceptance requires deterministic timestamps and ordering, including old-ID retries, manual/scheduled fallback and clock rollback. No T16 sign-off is implied by dispatch.

### T01 remaining failures returned; T07 ownership verification

T01 returned with topics and simplify still failing and is not accepted. Root identified the stale `property-Project` selector: the current property label is `Topic project`, and its control ID derives from that name. The topic rename/archive sequence is also being split across settled UI events to diagnose deferred popup teardown while preserving real clicks. T01 was resumed with `.local/opencode-dispatch/T01-finish-prompt.txt`; new log `.local/opencode-dispatch/T01-finish.jsonl`.

T07 now validates a stored neighbor target's period ownership at activation and falls back to the original period overview when the task moved. Its focused independent unit run passed nine tests, exit 0; [log](evidence/T07-owner-independent-unit.log). UI recent-entry/menu verification remains pending, so T07 is not accepted.

### T07 intermediate native navigation check

The frozen T18 native review build includes the then-current T07 navigation implementation (source manifest `.local/opencode-dispatch/T18-native-source.json`). Through Computer, Period 3 opened its Evidence detail, Next moved to Analysis then Final report, and Next was disabled at the final boundary. Recently opened displayed automation name, Period 3 and each step identifier; selecting Evidence returned to the named step, and the menu could reopen afterward. Overview restored the Period 3 dialog. [Recent-menu evidence](evidence/T07-native-recent-period.png). This supports the expected basic destinations and suggests the headless menu-reopen failure needs event-settling diagnosis; it does not replace remaining cross-period/multi-goal/current-candidate acceptance. No research was executed.

### T07 sidebar integration passed; exact matrix still pending

Independent `cargo run --features qa --quiet -- --qa-sidebar` passed, exit 0, including light/dark step detail/report neighbors, Overview/Back and real recent-entry activation. [Log](evidence/T07-intermediate-independent-sidebar.log). Splitting menu interaction phases and settling deferred events resolved the harness failure. Final criterion matching found that fixtures contained two periods for one automation but only one for the second; OpenCode was asked to add the fourth period and verify neighbors against all four owners, as TASKS.md explicitly requires. T07 remains unaccepted until that exact matrix is verified.

### T16 first implementation review and T01 geometry diagnosis

T16's draft activity helper treated `reference_time == 0` as a missing value, although the required epoch field permits zero. Root returned this for correction with explicit zero/negative timestamp coverage and asked that the documented first-present precedence match the actual sorting semantics. Unsupported text-observation API calls in its view changes temporarily blocked the shared test build; the owner was directed to restore compilation using existing observation patterns, without introducing excluded VoiceOver work.

T01's compact-toolbar diagnostic captured current show-result and action-menu tops both at 115.5 px with 32 px heights, while the cached pre-toggle menu top was 147 px. The failure was a stale-layout comparison. Root required comparing current aligned controls (or genuinely nonoverlapping wrapped rows), preserving layout bounds instead of broadly relaxing the test.

## T07 independent acceptance

Status: accepted in the reviewed working-tree candidate, not committed or released. Previous/Next now carries explicit period ownership and follows the current occurrence's ordered steps; the first and last boundaries disable, while Overview is separate. Multi-goal traversal remains within that occurrence. Ordinary research retains its documented wrapping tree order. Missing, moved or unavailable targets cannot activate a foreign period: they resolve to the original overview or remain disabled. Recent entries identify the automation, period and step; Back restores the owning period. DESIGN.md records this contract.

Independent full unit suite passed (345 passed, zero failed, two opt-in tests ignored; exit 0). The exact two-automations-by-two-periods fixture checks all four owner contexts and boundaries; moved/deleted targets and multi-goal transitions are covered. Full sidebar QA passed independently, including actual detail/report navigation, Overview, Back and recent-entry activation in light/dark modes. Lucide embedding verification passed for 51 registered icons. [Unit log](evidence/T07-final-independent-unit.log), [sidebar log](evidence/T07-final-independent-sidebar.log), [icon log](evidence/T07-independent-icons.log), [source hashes](evidence/T07-reviewed-source.json).

Reviewed screenshots show the period breadcrumb and separate Overview action with visible boundary state: [light](evidence/T07-navigation-light.png), [dark](evidence/T07-navigation-dark.png). The earlier native Computer walkthrough verified first/middle/last navigation, actual recent-menu selection/reopen and Overview returning to Period 3. Subsequent changes add stale-owner guards, stable QA text identity and the full four-period fixture; automated checks verify those differences. The menu QA failure was fixed by settling deferred events between real UI actions, not by bypassing those actions.

T01/T20 remain required integration/release gates. This acceptance does not claim the whole application is deliverable.

## T23/T25 bounded schema audit dispatched

OpenCode session `ses_e44fb9f9effeQmDEEHIealhGd2` uses the requested Go DeepSeek model for a read-only audit of immutable run ownership and planning-request revisions. Prompt: `.local/opencode-dispatch/T23-T25-audit-prompt.txt`; log: `.local/opencode-dispatch/T23-T25-audit.jsonl`; expected output: `.local/opencode-dispatch/T23-T25-audit.md`. Source edits are not yet assigned, avoiding overlap with T16. The audit must map every result-selection surface, expected-result prompt/signature binding, conservative old-schema migration and failed/stale/in-flight replan behavior before implementation.

T01 was also instructed not to run a stale executable after a failed build. Its archive fixture must assert the actual menu label and handle the existing stop-and-archive confirmation when seeded running/interrupted attempts require it. The earlier numeric-menu assumptions are provisional until verified against the rendered label.

### T16 intermediate key verification; T01 coverage review

Root independently ran `cargo test --quiet automation_activity::tests`: five passed, exit 0. [Log](evidence/T16-activity-intermediate-unit.log). The corrected helper preserves epoch zero and negative timestamps, defines semantic retry/start/request/trigger/reference precedence, and consistently sorts by the recorded chosen time with explicit tie rules. Full activity UI verification remains pending. Root also identified an assertion that checked only a locally computed expected label; the implementer must distinguish that from actual rendered evidence.

T01's topics runner now passes after settled popup phases, the correct separator-counted Archive/Restore menu item and Archive list layout. Simplify remains under review: root rejected making Recently opened optional in its long-header test without explaining the fixture's missing eligibility, and required a separate nonarchived eligible task case with mandatory bounds/nonoverlap assertions. This preserves the intended layout coverage.

## T23/T25 implementation dispatched with corrected audit decisions

Session `ses_e44fb9f9effeQmDEEHIealhGd2` completed the [read-only audit](evidence/T23-T25-planning-audit.md) and is now implementing from `.local/opencode-dispatch/T23-T25-implement-prompt.txt`; log `.local/opencode-dispatch/T23-T25-implementation.jsonl`. Model remains `opencode-go/deepseek-v4.1-flash`.

Root approved the overall immutable request/attempt approach with these binding corrections to the audit proposal:

- Prefer one typed optional planning owner so partial independent fields cannot contradict each other. Latest-attempt pointers do not establish historical ownership.
- Completion must guard the current applicable attempt as well as revision; an older response from the same unchanged revision must not overwrite a newer reviewed plan.
- History known to belong to an automation project stays outside ordinary Research even if its precise owner is ambiguous. Keep it visibly accessible as unassigned legacy automation planning history; do not guess a specific owner or delete it. Truly unknown history remains visibly unlinked.
- Validate the new plan binding before constructing fresh approval, preserving unchanged historical approvals without fabricating old expected-result inputs.
- Relocation must transform introduced plan-binding signatures as well as run signatures.
- Live unsaved input changes survive completion, and an incompatible response cannot populate the review/approval UI.

T16 retains activity ordering/timestamp ownership; T01 retains its baseline QA behavior. New owner-field initialization in their fixtures is allowed only as a mechanical compile migration. Neither T23 nor T25 is accepted by dispatch.

### T16 green checks returned after visual review

Independent full unit tests and automation QA passed (345 unit tests passed, two opt-in tests ignored, both commands exit 0). However, rendered activity-order screenshots exposed a fixture dispatch error: an unmaterialized scheduled row failed during storage attachment, changing the intended unstarted state and showing an error footer. Root returned this despite green checks. The fixture must prevent unintended dispatch before attachment, use valid queued data and assert exact post-load states/errors before replacement screenshots can support acceptance. T16 remains unaccepted.


## T16 corrected fixture visual review; integration still pending

Root independently inspected the corrected activity list, definition history, and global results screenshots. All three consistently show the older-ID retry first with a `Retried` timestamp; subsequent rows show `Started` and `Scheduled`. The definition remains Enabled and the footer reports All changes saved. The fixture now enables fake mode before attachment and stores inert Cancelled/Failed/Skipped periods, with explicit post-attachment assertions for status, retry timestamp, absence of errors, and latest identity. This fixes the prior unintended recovery dispatch error rather than hiding its footer. Evidence: [list](evidence/T16-reviewed-activity-list.png), [history](evidence/T16-reviewed-activity-history.png), [results](evidence/T16-reviewed-activity-results.png).

T16 is not yet finally accepted: its post-correction full test attempt encountered concurrent T23/T25 schema constructor/signature compile errors. The corrected integration QA must be rerun against a successfully rebuilt candidate after that migration settles. Existing screenshot fixtures also contain a non-English project title; the English-only fixture cleanup remains required before final evidence regeneration.

## T05 audit dispatched

A bounded read-only audit of shared execution eligibility and status presentation has been dispatched to OpenCode using `opencode-go/deepseek-v4.1-flash`. Prompt: `.local/opencode-dispatch/T05-audit-prompt.txt`; log: `.local/opencode-dispatch/T05-audit.jsonl`. Implementation ownership will be assigned after reviewing exact dispatcher/presentation mismatches and coordinating with the active T23/T25 changes. No acceptance is implied by this dispatch.


## T23/T25 in-progress independent review blockers

Root returned three concrete implementation problems to OpenCode before acceptance: (1) ownership normalization indexed `matches[0]` inside eager `then_some`, which panics for an unmatched nonempty legacy signature; use lazy or slice matching and prove reopen safety; (2) binding validation converted signature errors to absence of a problem, permitting invalid modern bindings; distinguish true legacy data and reject orphaned/invalid modern bindings; (3) unconditional binding removal during definition revision would unnecessarily require replanning for schedule-only edits; retain compatible planning bindings while separately invalidating approval. Requested regression cases include unmatched historical signatures, orphaned modern bindings, schedule-only edit/reapproval, and changed planning text. These are review findings against an actively changing candidate, not accepted fixes.

T01 also received a test correction: compare title and menu bounds from the same rendered route. Its current header assertion clicked Back before comparing the new title with old menu geometry. Mandatory narrow-window menu coverage must remain; screenshots should be captured before failing assertions.


## Follow-up source review and T06 dispatch

The active T23/T25 source now replaces eager empty-vector indexing with an explicit length guard, reports missing/invalid modern bindings, validates binding project identity, and preserves compatible bindings through schedule/policy edits. These changes address the reported code paths but are not accepted until regression and reopen evidence passes. Root requested restoring compilation before further UI expansion so T01/T16 can complete integration checks.

Independent T05 inspection also identified the ordinary `task_status` submission/stopped heuristic as a potential automation mismatch between steps: it observes ordinary batch membership or active runs, not the occurrence lifecycle. The audit must cover phase barriers, ordered goals, between-step waiting, and a running manual period whose definition is Paused.

A bounded read-only T06 audit was dispatched using `opencode-go/deepseek-v4.1-flash` to trace final/intermediate/live result selection and reader/copy/export consistency. Prompt and log: `.local/opencode-dispatch/T06-audit-prompt.txt` and `.local/opencode-dispatch/T06-audit.jsonl`. Implementation must coordinate with T23/T25 ownership and T05 eligibility; dispatch is not acceptance.


## Independent intermediate compilation recovery

Root ran `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --features qa --quiet` with its actual process exit code preserved: exit 0, 345 passed, 2 documented opt-in ignores. Evidence: [unit log](evidence/planning-intermediate-independent-unit.log). This verifies the tested intermediate source compiles and passes existing units; the source remains under active modification and the new T23/T25 migration/attempt requirements are not thereby accepted. T01 was notified to rebuild and resume simplify plus the seven focused baseline suites, with sidebar regression coverage required after header geometry changes.


## T01 narrow report header rejected after rebuild

The successfully rebuilt simplify candidate still fails at the earlier `open-tasks-menu` click in the deep report route. Root inspected the 1000 px light screenshot and confirmed the breadcrumb row extends beyond the right viewport, pushing Recently opened offscreen. This is a real layout defect, distinct from the separately diagnosed missing `observed` registration on `navigation-header`. The 1440 px dark ordinary-task header is readable but does not prove the narrow deep route. Evidence: [rejected narrow header](evidence/T01-narrow-report-header-rejected.png). OpenCode was instructed to fix TitleBar/flex width constraints and preserve both the early menu click and the long-title assertions; shortening or skipping the fixture is not accepted.


## Independent automation integration failure returned

Root rebuilt with `cargo build --features qa` successfully, then ran `--qa-automation` with Node 24 on PATH. The QA process exited 101 at `automation_qa.rs:271`: the reviewed two-goal template was absent (`None`, expected `Some(2)`). Logs: `.local/opencode-dispatch/root-integration-build.log`, `.local/opencode-dispatch/root-integration-automation.log`. T23/T25 must diagnose the new attempt/binding guard against this flow without weakening the two-goal assertion or bypassing current-attempt validation. This is a concrete unresolved integration failure; the earlier passing unit suite does not establish automation workflow compatibility.


## Automation integration rerun passed; T05/T06 implementation dispatched

Root rebuilt and reran the entire automation QA suite independently: build exit 0, QA exit 0. Evidence: [build](evidence/root-integration-build-2.log), [automation QA](evidence/root-integration-automation-2.log), screenshots under `.local/automation-qa/25817`. The prior planning-chain failure was corrected by using the automation definition's actual project for the request. The real creation path derives that ID from the saved definition. Planning-specific new migration/UI requirements remain pending; this pass is not T23/T25 acceptance. T16 activity-order QA also passed in this integration run.

Reviewed audits: [T05](evidence/T05-audit.md), [T06](evidence/T06-audit.md). Both implementations have now been dispatched with model `opencode-go/deepseek-v4.1-flash`. Root corrections supersede audit proposals where they conflict: queued display must not prevent dispatch preparation; active retry supersedes historical failure; no duplicate running attempt; manual step eligibility uses the whole period's ordered goals; visible barrier labels are required. Result presentation must aggregate multi-goal completeness/quality, retain inspectable completed reports, distinguish invalid successful envelopes without rejecting legitimate Markdown, preserve simultaneous freshness/regeneration/quality facts, and retain plain-text ordinary-task results. Shared English evidence-gap wording is approved.

Ownership is function-scoped: T05 owns eligibility and step status cells; T06 owns reader presentation/copy/export and quality suffixes; T23/T25 keeps planning ownership/history/forms. T01 retains navigation header and baseline QA. Prompts `.local/opencode-dispatch/T05-implement-prompt.txt` and `T06-implement-prompt.txt` contain exact constraints and regression matrices. No task acceptance follows from dispatch.


## T23/T25 request tests passed; end-to-end capture remains required

Root independently ran `cargo test --quiet request_planning::`: exit 0, 24 passed, no ignored tests in this filtered suite. Evidence: [request tests](evidence/T23-T25-independent-request-unit.log). Source review confirms new tests cover immutable objective/expected revisions, same-input attempt precedence, missing modern binding/project rejection, and the unmatched legacy signature panic.

The new `planning_revision_completion_check` covers stale completion and dirty expected-result protection, but its manually inserted successful runs do not establish what the real wizard sends to the CLI. Root explicitly retained fake-CLI prompt capture through the real wizard (A to B, expected-only change, unchanged replan, failure retry), three-attempt restart/history UI, and Research exclusion as outstanding acceptance evidence. T23/T25 remains open. T01 was asked to localize its remaining popup `Integer(0)` failure with a project stack frame and pre-click evidence before further fixture edits.


## T23/T25 dirty-input assertion removal rejected

Root reviewed an in-progress change removing the assertion that the expected-result field still contains the user's edited text after an older plan completes. The remaining `plan.is_none()` assertion proves only that the obsolete plan is not presented; it does not prove input preservation. OpenCode was instructed to restore exact before/after objective and expected-result comparisons, using the appropriate UI context if necessary, and fix any behavior failure rather than remove the check. This remains an explicit acceptance condition.


## Narrow header partial fix still rejected

Root inspected the latest narrow report screenshot after crumb widths changed from 150 px to 100 px. Recently opened remains partially clipped at the right edge, so this is not a verified width-containment fix. Evidence: [partial menu rejected](evidence/T01-narrow-header-partial-menu-rejected.png). Root required a menu-right-edge assertion in this exact deep report context before interacting. Popup teardown and custom recent-item selectors must also be handled correctly, but neither explanation establishes that the layout is fixed.

The T23/T25 expected-result preservation assertion has been restored in source through an explicit UI-context read. Its execution remains subject to the new planning QA run; restoration alone is not passing evidence.


## T06 first selector review returned for correctness

Root reviewed the first `result_presentation.rs` implementation before UI integration and returned three blockers: marker/envelope classification precedes attempt-state gating and can label Running/Failed/Cancelled data as a final report; stale-input detection relies on task stage instead of the signature/current-result contract, missing input changes while Done; and occurrence selection examines only Advise candidates, omitting completed Collect/Analyze evidence when no final exists. Required regressions include active valid envelopes, failed partial envelopes, cancelled evidence markers, Done tasks with changed input/dependency signatures, and failed multi-goal periods retaining completed reports and intermediate evidence. These are source-level findings against an unfinished candidate; T06 is not accepted.


## T01 investigation reset to isolate the actual layout defect

After repeated selector changes failed to establish narrow header containment, root deliberately interrupted the active T01 turn, confirmed its process ended, and resumed the same OpenCode session/model with a consolidated focused task. The next required evidence is a minimal deep-report header fixture at both widths/themes with full menu bounds, screenshots, and an actual recent-item route check. Native TitleBar behavior and T07 semantics must remain intact. Only after that passes should the full simplify suite resume. Prompt/log: `.local/opencode-dispatch/T01-header-reset-prompt.txt` and `T01-header-reset.jsonl`. This is a workflow correction, not acceptance or a scope reduction.


## T05 eligibility review found explicit-retry regression

Root identified that `retry_occurrence` preserves the old failed run while reopening the same period. The initial shared eligibility requires `StepOutcome::Open` for preparation but derives Failed from that preserved run, so explicit retry cannot prepare its unfinished step. OpenCode must honor reopened-period retry authority without rewriting failure history and preserve T03 snapshot/paused-definition behavior. The existing `failed_period_reopens_in_place_and_keeps_finished_steps` test and cancelled retry cases are required checks.

Root also returned unconditional barrier label overrides: a step whose decision is Done, Running, or Blocked must not be labelled Waiting merely because a phase/goal barrier fact exists. Label, decision, and reason must agree. These source findings block T05 acceptance until tested.


The independent T05 retry test attempt did not execute: compilation exited 101 because the active eligibility integration used `u32` for a stored `u8` retry count and called `icon_view` on `TaskStatus` instead of its execution field. Log: `.local/opencode-dispatch/root-T05-retry-regression.log`. These transient implementation errors were returned; they provide no passing or failing behavioral test evidence.


## T05 explicit-retry regression independently reproduced

After compilation recovered, root reran the existing `failed_period_reopens_in_place_and_keeps_finished_steps` test. It executed and failed (exit 101): preparing the reopened period returned `The next goal of this period is not runnable yet` at `automation_runtime.rs:1114`. Evidence: [independent failure](evidence/T05-independent-retry-failure.log). This confirms the shared eligibility regression against accepted T03 behavior. The original test and historical run state must remain intact; T05 must fix the decision/preparation rules and rerun the complete retry cases.


## T06 first pure selector suite independently passed

Root ran `cargo test --quiet result_presentation::` against the updated intermediate candidate: exit 0, 9 passed. Evidence: [selector tests](evidence/T06-independent-selector-unit.log). The tested cases cover active/failed/cancelled envelope separation, invalid successful envelopes, brace-leading Markdown, changed signatures, legacy text, and multi-goal completeness/evidence retention. This supports the pure selector cases only; UI integration, complete state matrix, regeneration/freshness combinations and copy/export consistency remain open.


## T05 retry restored; new eligibility tests still incomplete

Root independently reran the original explicit-retry regression: 1 passed, exit 0 ([log](evidence/T05-independent-retry-fixed.log)). The new eligibility module suite executed with 10 passed and 2 failed ([log](evidence/T05-independent-eligibility-initial.log)). Root classified the failures: the ordered-goal test must not expect duplicate preparation while another attempt is active; the terminal descendant test must distinguish internal Ended from its Waiting UI projection. OpenCode must preserve no-duplicate dispatch and add explicit label/reason assertions when correcting these expectations. This narrow retry pass does not accept all T05 behavior.


## Independent integrated unit suite: 375 passed

Root ran the complete unit suite after the T05 retry and eligibility corrections: exit 0, 375 passed, 2 documented opt-in ignores. Evidence: [full units](evidence/integration-independent-unit-375.log). This replaces the prior intermediate failing unit result for the tested source; active UI integration continues, so it is not a frozen release gate. Root also reviewed the real planning CLI capture QA and required three actual history selections after reopen, correct per-revision content/status, unchanged underlying navigation/store, and light/dark evidence before planning acceptance.


The independently invoked `cargo run --features qa --quiet -- --qa-planning` completed with exit 0. Evidence: [planning QA](evidence/T23-T25-independent-planning-qa.log). It covers real wizard prompt capture for changed objective/expected result, expected-only edits, unchanged replanning, failed-attempt retry, and persisted attempt listing after reopen, plus dirty/stale completion guards. Root's additional requirement to click and inspect at least three distinct history attempts and review light/dark screenshots is still pending; no T23/T25 acceptance yet.


## Independent intermediate integration: 379 unit tests passed

Root reran the complete unit suite with Node 24 on PATH: exit 0, 379 passed, 2 opt-in ignores. Evidence: [unit log](evidence/integration-independent-unit-379.log). This validates the current intermediate integration, not a frozen release candidate. Root visually inspected the focused Light-1000 deep report header screenshot: the full Recently opened control and popup are now contained. T01 must still verify sidebar resizing and window resizing, retain all original simplify coverage, and complete the release suites. T06 remains open pending rendered state and copy/export acceptance; passing pure selector tests does not prove those requirements.


Root independently built the QA binary (exit 0) and ran `--qa-header-layout` (exit 0): [log](evidence/T01-independent-header-layout.log). This proves the fixed-size deep-report menu containment and recent-item routing at 1000/1440 px in both themes. Resize coverage remains pending. Root returned a T05 QA coverage gap: asserting `Store::task_status().label()` after scrolling an Open button does not prove the rendered status label. Visible semantic label assertions and task/progress surface coverage are required.


## T23 planning-history close loses its origin

Root source review found that `open_planning_attempt` clears `settings_dialog`, while `close_planning_attempt` only clears the history dialog and focuses the app. Reviewing a saved attempt from Plan versions therefore loses the initiating settings/history surface on Close. This violates target interaction contract 3 and blocks T23 acceptance. OpenCode was instructed to preserve/restore explicit modal origin, scroll and opener focus, handle Escape and global legacy origin, and prove three sequential actual history selections without resetting UI flags in the test. Root also rejected request-to-request data comparisons as proof of rendered per-revision content: dialog objective, expected result, revision and status need actual rendered verification.


Root ran the initial `--qa-execution-eligibility` on the independently built intermediate binary: exit 101, missing observed `automation-result-body` scroll anchor. [Failure log](evidence/T05-independent-visible-qa-initial-failure.log). The anchor appears only as a registry ancestor; this is a fixture/observation failure, not proof of product status correctness. Returned to OpenCode along with rendered-label coverage requirements. Root also ran `python3 script/check_icons.py`: exit 0 ([log](evidence/integration-independent-icons-current.log)).


## Next architecture audit and baseline snapshot attempt

Root dispatched a read-only T13 command/scoped-undo audit to OpenCode Go DeepSeek v4.1 Flash, requiring mutation/save/rollback/effect inventory, failure injection matrix and an implementation plan that enables T14/T15. Prompt: `.local/opencode-dispatch/T13-audit-prompt.txt`; output: `.local/opencode-dispatch/T13-audit.md` when complete. No T13 implementation or acceptance yet.

T01 reports all seven focused suites plus sidebar passing. Root copied an isolated source snapshot for the original 33-entry gate; its build failed before QA execution because the new T06 clipboard QA called App-only methods on HeadlessAppContext (E0599). Evidence: `.local/opencode-dispatch/root-baseline-snapshot-build.log`. The separate latest unit attempt also failed compilation on ResearchExport lacking Debug for unwrap_err (E0277), `.local/opencode-dispatch/root-integration-units-5.log`. Both were returned to T06; prior 379-pass evidence remains limited to its earlier source. The full 33-entry gate has not run yet.


## Independent frozen baseline snapshot 2

Root copied src/assets and the dependency manifests into `.local/opencode-dispatch/baseline-snapshot-2`, renamed only the package to isolate its binary, and built it successfully. The original 33-entry sequential QA gate is now executing against the copied immutable binary; source/binary hashes and per-entry exits are recorded in `baseline-snapshot-2/gate/index.json`. Root independently ran the same snapshot unit suite: exit 0, 380 passed, 2 opt-in ignores ([log](evidence/baseline-snapshot2-independent-units.log)). This is a baseline integration checkpoint, not final release approval.

The first 22 completed QA entries include two failures: attachments (`research_qa.rs:1029`, completed progress-step click leaves the selected run on another task) and onboarding (`workspace_qa.rs:239`, explicit saved legacy period report is no longer selected). Root returned the first to T01 for route/fixture diagnosis without weakening the expected destination. Root traced the second to T06 selecting only modern Advise children and required preserving an explicitly owned legacy final_run_id body with unverified labeling; T24 relocation must continue retaining reports. Remaining entries continue running.


## Full original 33-entry gate completed: 29 passed, 4 failed

The independent frozen snapshot 2 gate completed with exit 1: **29 passed, 4 failed, no timeouts**. [Manifest with binary/source hashes and all entry exits](evidence/baseline-snapshot2-33-entry-gate/index.json). Every original entry ran sequentially, including aliases. This is not acceptance.

- Attachments: completed-step click retained the analysis run instead of opening the completed evidence result ([log](evidence/baseline-snapshot2-33-entry-gate/qa-attachments.log)); returned to T01 for root-cause diagnosis with original destination assertion preserved.
- Onboarding: the explicit legacy saved report became unavailable after reopening ([log](evidence/baseline-snapshot2-33-entry-gate/qa-onboarding.log)); returned to T06 to retain inspectable legacy content without claiming validated completeness.
- Automation: recovery remained Verifying until the fixture deadline ([log](evidence/baseline-snapshot2-33-entry-gate/qa-automation.log)); returned to T05 to diagnose dispatch/recovery interaction, coordinating with T06 if necessary. No timeout increase authorized without diagnosis.
- Batch: explicit start/pause left Project(1) rather than the expected Runs surface ([log](evidence/baseline-snapshot2-33-entry-gate/qa-batch.log)); returned to T01 to reconcile the explicit user-command destination with T02 background-navigation isolation, preserving real user control coverage.

All earlier seven failing baseline suites now pass on this snapshot, but these four integration failures prevent T01/T02 release sign-off. Supplemental planning, execution-eligibility and result-presentation suites remain independently required and are still under implementation/review.


## T06 rendered QA executes, but visual evidence rejected

Root independently ran `cargo run --features qa --quiet -- --qa-result-presentation`: exit 0 ([log](evidence/T06-independent-rendered-qa-first-pass.log)). The real Copy result and export parity checks passed. However, visual inspection found [the alleged dark screenshot](evidence/T06-rejected-dark-actually-light.png) uses the light palette and shows unrelated Storage unavailable status. The fixture directly assigns Store without applying appearance or attaching isolated storage. The pass does not establish dark-theme or valid normal-state acceptance. Root returned fixture initialization and actual palette verification, required scroll evidence for the report body in the regeneration view, and identified missing Quick failure/cancellation/staleness/regeneration, ordinary cancellation/regeneration and rendered multi-goal completeness cases. No T06 acceptance.


## Legacy report regression independently repaired; recovery diagnosis narrowed

Root reran `--qa-onboarding` after T06 legacy final-run fallback repair: exit 0 ([log](evidence/T06-independent-legacy-onboarding-fixed.log)). Reconnect/relocation with saved Quick/Research automation reports passes again. This resolves the observed onboarding regression on the current candidate; it does not accept all T06 work.

Root inspected the frozen failed recovery fixture database read-only. Run 1 is Succeeded and contains validated imported evidence; the occurrence remains Verifying with the earlier unknown-session reason and no next run. The CLI import succeeded. Root returned the confirmed-recovery transition/eligibility interaction to T05, explicitly retaining unknown-session fail-closed behavior.

Root reviewed the T13 audit and returned two design hazards before implementation: stale prepared-store replay must not overwrite background completions (require generation/conflict/reprepare semantics), and user Stop must still stop external work during a storage outage while truthfully reporting nondurable cancellation. Merely calling the existing two-write enqueue path from a receipt is not sufficient command-boundary extraction. Audit revision is pending.


## T13 pure command slice dispatched; T05 progress evidence rejected

Root dispatched T13 S0 to the same OpenCode Go DeepSeek v4.1 Flash session after its read-only audit ended. The prompt requires the concurrency/stop-safety design corrections, domain-only command preparation, scoped inverse tests and stale-prepared-state rejection. Ownership is limited to new domain-command files plus module registration; UI/runtime migration remains pending. Prompt: `.local/opencode-dispatch/T13-S0-prompt.txt`. Initial audit retained as [evidence](evidence/T13-initial-audit.md); it is not approved architecture until the corrections are incorporated.

Root visually reviewed the earlier T05 progress screenshot and found an [empty ordinary Research results page](evidence/T05-rejected-empty-progress-screenshot.png). The QA invoked ordinary dispatch on an automation-owned run. Root required actual automation-owned entry-point coverage, with a separate ordinary fixture for ordinary progress; owner-domain filtering must not be weakened to make the test pass.

During T05 recovery diagnosis OpenCode modified two files inside the frozen baseline snapshot source. Root immediately required moving diagnostics to a separate clone and restoring those two source hashes. The gate executable hash is unchanged and the completed 33-entry results remain tied to that binary. Source restoration verification is pending; do not use modified snapshot source as new acceptance evidence.


Root interrupted the T05 turn after further writes to frozen snapshot source, confirmed its process exited, preserved the diagnostic edits separately, and restored both files from snapshot 1 only after verifying they matched snapshot 2 original hashes. **All snapshot 2 source hashes now match the original gate manifest; the gate binary was unchanged.** T05 was resumed in the same model/session with a consolidated recovery/real-owner-route prompt and explicit read-only snapshot constraints: `.local/opencode-dispatch/T05-recovery-resume-prompt.txt`. Diagnostic work must use a separate clone. This restoration changes no product implementation and preserves the original 33-entry evidence.


## Attachments and batch regressions independently passed

Root built the current candidate (exit 0), copied its executable to an independent test path, and ran both previously failing suites sequentially: `--qa-attachments` exit 0 ([log](evidence/T01-independent-attachments-fixed.log)); `--qa-batch` exit 0 ([log](evidence/T01-independent-batch-fixed.log)). The completed evidence fixture now sets the completed step to Done, retaining the original own-result destination assertion. The batch assertion now follows the in-place queue command contract; its real Run/resume control coverage remains. Together with repaired onboarding, three of snapshot2’s four failures have subsequent independent passing evidence. Recovery/automation remains open. A full frozen gate still needs rerunning after remaining integration work.

Root also independently ran expanded planning QA: exit 0 ([log](evidence/T23-independent-history-expanded-qa.log)). Actual A/failed-revision content and dark palette are visible. Root nevertheless returned the 1400px-height workaround for history controls and missing T25 Enable-B/retained-period-A journey. Passing this test does not accept T23/T25.

## T08 global search implementation dispatched

Root dispatched T08 to OpenCode Go DeepSeek v4.1 Flash with narrow search-module, footer/keyboard and route integration ownership. Prompt: `.local/opencode-dispatch/T08-implement-prompt.txt`; log: `.local/opencode-dispatch/T08-implementation.jsonl`. It must satisfy all original T08 entry-point/keyboard/ownership/return-context criteria and preserve dirty drafts; no execution or persistence semantics are in scope. Root review is pending.


## Confirmed recovery independently advances; later retry UI fixture still fails

Root independently ran the new confirmed-complete recovery unit: exit 0, 1 passed ([log](evidence/T05-independent-confirmed-recovery-unit.log)). The full automation QA now passes crash/reopen/import/continuation/next-period, paused recovery, archived recovery, regeneration recovery, step entry and all four passive navigation contexts. It later exits 101 because `resume-automation-steps` exists below the visible scroll region ([log](evidence/T05-independent-recovery-advanced-full-qa.log)). Root returned real scroll-to-button interaction without changing the retry assertion or increasing the window height. Full automation acceptance remains open.

Root refreshed the separate Planning Native Review bundle from a copied current source, preserving its isolated bootstrap path and fixture agent, built successfully and reopened it through Computer Use. The saved workspace loads and Research results is visible. This does not verify window resizing; native resize/header validation remains pending. User production workspace/app was not restarted.

T06 second visual review confirms genuine dark colors and attached storage, but found unchanged-input regeneration still saying task context changed and asking the user to run again. Root required deriving the explanatory notice from typed freshness. Normal automation fixtures also raise an unrelated unapproved-definition footer; they need a valid approval setup before visual sign-off.

## T05 visible eligibility rerun; T13 and T23 review corrections

Root independently reran `cargo run --quiet --features qa -- --qa-execution-eligibility`: exit 0 ([log](evidence/T05-independent-visible-qa-fixed.log)). The generated dark 1000px period screenshot shows the failed collection step as Blocked and downstream analysis as Waiting for collection, with Running period state separate. The matrix includes period/task/goal-step and genuine ordinary progress surfaces in both themes and sizes. Full automation regression is being rerun separately; this result alone does not close T05.

Root independently ran the T13 S0 domain-command unit tests: 14 passed, exit 0 ([log](evidence/T13-independent-S0-unit.log)). Source review nevertheless found dispatch preparation permits Verifying occurrences and an absent occurrence argument can bypass linkage checks on an automation-owned run. Returned to OpenCode for shared eligibility and owner/lifecycle validation tests before integration. The post-save stale check must not be described as protection against future asynchronous stale disk writes; synchronous exclusive access is the current constraint. T13 remains Open.

Root inspected the expanded T25 journey: actual Enable A and Enable B now assert request/revision approval and retain period A objective/expected result. T23 still restores focus to the entire history section instead of the actual Review opener. Returned for exact per-attempt opener restoration on Close and Escape, including global legacy history, with normal-size rendered tests. Neither task is accepted yet.

## Independent full automation regression passes after recovery and scroll fixes

Root's `cargo run --quiet --features qa -- --qa-automation` completed with exit 0 ([full log](evidence/T05-independent-full-automation-fixed.log)), fixture `.local/automation-qa/59794`. Recovery, period retry, passive navigation, completion boundaries, independent pools, undo protection, draft defaults, shape invalidation, late planning, revision completion and the actual planning CLI flow all pass. This resolves the earlier full automation failure for this working-tree candidate. Full frozen 33-entry integration, T23 exact opener focus, remaining task contracts and native final acceptance are still pending; this is not whole-program acceptance.

## T06 independent presentation rerun and visual review

Root reran `cargo run --quiet --features qa -- --qa-result-presentation`: exit 0 ([log](evidence/T06-independent-rendered-qa-fixed.log)). Copy and export parity checks passed. Root visually inspected the [dark regenerating ordinary report](evidence/T06-independent-regenerating-dark.png): the previous valid report remains readable and the notice now describes regeneration without falsely claiming changed inputs. The [multi-goal evidence view](evidence/T06-independent-multigoal-evidence-light.png) retains the second goal's completed evidence and explicitly identifies the missing final report. Storage status is saved and themes render correctly. This resolves those earlier review corrections; final T06 acceptance remains pending complete handoff/integration checks.

A concurrent full-unit invocation hit T23's in-progress removal of `history_focus` before all call sites were updated (exit 101). It is recorded as an intermediate integration build failure, not silently counted as passing or classified as a product regression. Root will rerun after the field migration is coherent.

Root dispatched a read-only T10 origin/draft/creation transition audit to OpenCode Go DeepSeek v4.1 Flash using `.local/opencode-dispatch/T10-audit-prompt.txt`. This covers all entry/exit controls and proposes implementation ownership before edits, coordinating with active global search and planning-history work. T10 remains Open.

## Integration failure classification and T16 refreshed evidence

Root full-unit rerun completed with 396 passed, 3 failed, 2 ignored ([log](evidence/integration-intermediate-unit-failures.log)). Two failures were T13 intermediate fixture lifecycle mismatches while dispatch validation was being tightened; returned to its owner without weakening the queued-state gate. The publishing failure explicitly diagnosed missing Node on PATH. Root reran that publishing test with the documented Node 24 PATH: 1 passed, exit 0 ([log](evidence/integration-node24-publishing-pass.log)).

Root reran schedule arithmetic: 20 passed, exit 0 ([log](evidence/T16-independent-schedule-current.log)). The latest independently passing full automation QA also regenerated activity history with English fixture names, the old-ID retry leading, matching Retried/Started/Scheduled timestamps and All changes saved. The follow-up activity/eligibility test builds hit a concurrent T08 missing `label` import; returned to that owner. These builds are not passing evidence and acceptance remains pending a coherent candidate.

Root dispatched T17 documentation and lifecycle wording reconciliation through `.local/opencode-dispatch/T17-implement-prompt.txt`, including the visibly incorrect blanket 'missed periods are not backfilled' text. The maintained route/action matrix must distinguish current implementation from required pending behavior, preserving TASKS authority.

## T05 independent acceptance

Accepted in the reviewed working-tree candidate; not committed or released. The shared pure eligibility selector is used by automated step selection/preparation, normal automation-owned start routing, execution status and visible reason labels. It covers phase and ordered-goal barriers, dependency/approval waits, active attempts and explicit same-period retry authority without deleting failed history. Confirmed recovery may resume; unknown recovery remains fail-closed. Root independent full automation QA passed ([log](evidence/T05-independent-full-automation-fixed.log)); rendered status QA passed at 1440x920 and 1000x680 in both themes ([log](evidence/T05-independent-visible-qa-fixed.log)); the current complete unit suite passed 406 tests with two documented opt-in ignores ([log](evidence/integration-independent-406-unit-pass.log)). Blocked failed steps and Waiting descendants were visually checked. T01/T20 and future T13 integration remain release/regression gates.

## T16 independent acceptance

Accepted in the reviewed working-tree candidate; not committed or released. Named activity keys are shared across latest definition summary, history and global results, retaining retry/start/manual/scheduled/reference precedence, deterministic tie-breakers and valid zero/negative timestamps. Frozen period reference and data window remain unchanged by retry. Schedule arithmetic passed 20 focused tests ([log](evidence/T16-independent-schedule-current.log)); current full suite passed 406 tests ([log](evidence/integration-independent-406-unit-pass.log)); root full automation QA passed the actual ordering/rendering checks. Root reviewed regenerated [English history evidence](evidence/T16-independent-English-history.png), resolving the earlier language and attachment-dispatch fixture gaps. Existing list/history/results image review and boundary evidence remain linked above. Global light/dark/native regression remains part of T20; no release is claimed.

## T13 S1 and T22 implementation dispatched

Root reviewed T13 S0 rework: unknown Verifying recovery, missing/inconsistent occurrence links, foreign task/period ownership and invalid queued lifecycle are rejected; valid ordinary and automation paths remain covered. The current 406-pass full suite includes its 21 command tests. S0 is suitable to extend but does not accept T13. Root dispatched S1 executor/lifecycle adoption with explicit durable-before-effects, reprepare-on-current-state and nondurable emergency-stop semantics. Prompt: `.local/opencode-dispatch/T13-S1-prompt.txt`. Runtime atomic dispatch and complete scoped undo migration remain later required phases, not waived.

T06's completed owner was assigned T22 archive lifecycle/retained results using `.local/opencode-dispatch/T22-implement-prompt.txt`. It must prove completed three-step outcome across List/Board/detail, no execution controls in archive, unchanged stored reports/approval, exact Restore behavior and active-work stop confirmation. T13 command files, global search and planning history are outside its ownership. Root acceptance remains pending.

T23 exact-opener focus rework has been handed back by OpenCode; root source inspection confirms per-attempt stable focus handles rather than section focus, but independent rendered rerun is pending the in-progress T08 QA module becoming buildable. Agent-reported green checks are not substituted for independent acceptance.

## Additional T08/T06 review gaps and native navigation observation

Root source review returned three T08 gaps: unit tests were disabled behind `cfg(all(test, any()))`; the missing declared global-search QA module blocked feature builds; search activation guarded task/project-planning drafts but not automation create/edit drafts. Search also inferred final-report availability from a pointer alone, contrary to T06 validation. OpenCode was instructed to restore runnable tests, provide real QA, preserve drafts/origin and use truthful result metadata. These are in-progress corrections; T08 remains Open.

T06 acceptance audit found Research automation stale-state coverage missing from the explicit matrix (Quick and ordinary stale existed). Root requested multi-goal Research stale pure/rendered coverage and visible state assertions; screenshot creation alone is not a state assertion. T06 remains Open despite earlier successful matrix execution.

Using Computer on the isolated native-review bundle, root navigated Automation results -> Period 3 -> scrolled to completed report step -> View final report -> Recently opened. The selected period/report ownership remained visible and the recent entry named the automation, period and report step. macOS exposed `zoom the window`; invoking it visibly enlarged the real native window, and the header/menu remained contained in the enlarged report layout. This is native wide-window/zoom evidence, not proof of the still-required 1000px narrow native resize or a current final build. The production app/workspace was not touched.

## T11 dispatched and T25 negative-contract coverage requested

Root dispatched T11 shared-control affordance and visual hierarchy work using `.local/opencode-dispatch/T11-implement-prompt.txt`. Ownership is restricted to control factories/styles and narrow rendering changes; navigation, lifecycle handlers, archive semantics and result validation remain with their existing owners. Required evidence includes both themes/sizes, long English/CJK user titles, danger/disabled/selection distinction, icon checks and relevant regression QA. T11 remains Open.

Root's requirement-level T25 audit confirmed the independent full automation run covers actual A/B enable and frozen period A, expected-C successful replanning, failed retry and dirty-completion handling. However, the existing negative approval test changes objective, while expected-only UI coverage always generates the matching replacement before Enable. Root requested an explicit expected-only stale-plan rejection/atomicity regression followed by matching-plan success from the T23/T25 owner (`.local/opencode-dispatch/T25-negative-contract-prompt.txt`). This closes an evidence gap without silently weakening acceptance; T25 remains Open.

T10's active audit owner received coordination requirements for a shared return-context/dirty-exit mechanism used by T08, avoiding competing guard stacks. Feature QA remains pending T08's real QA module; this does not waive any independent planning-history test.

## T13 S1 interim durability and stop review

Root source inspection found the first S1 emergency-stop implementation could claim Saved after an error solely because the in-memory occurrence was terminal and no pending stop remained. Root returned this: only a confirmed durable receipt or matching confirmed saved state proves persistence. Startup attachment called recovery before stop-journal reconciliation, creating an admission-order hazard; journal suppression must be installed before recovery can enqueue/start. Reconciliation also cleared records on empty storage_error after a save helper that can no-op. Clearing now requires explicit durable evidence.

Unreadable/malformed/newer stop records must not silently permit unknown stopped work to replay: the app can remain usable while affected automation admission is fail-closed with actionable diagnostics. The sidecar's write/rename must have appropriate flush semantics if described as crash-durable. These corrections and app-level failure-injection tests were returned to the active S1 owner; no acceptance is implied. Current command and stop code is under active implementation.

## T22 historical-outcome review; planning gate build feedback

Root reviewed the first archive selector: any Succeeded nonempty run counted as a completed step. This could turn malformed successful output or a result from stale inputs into an Archived 3/3 complete claim. Returned to T22: separate inspectable historical attempts from validated historical completion, reuse the report contract, preserve input revision validity without granting execution to archived tasks, and test invalid success/stale inputs/failed regeneration with prior valid results. Archive must preserve the actual outcome, not synthesize success from process exit.

Once T08 added its QA module, root attempted independent planning QA. Build exited 101 due concurrent missing stop feedback helper (T13), invalid search fixture run conversion and Debug-only assertion issue (T08), and borrowed temporary labels in stale-result QA (T06/T22). [Build log](evidence/integration-planning-intermediate-build-failure.log). Each was returned to its owner. No planning acceptance or successful QA run is claimed for this invocation.

## T17 truthfulness review and T25 test source review

Root returned the route/action matrix claim that report bodies are loaded only when opened: current SQLite load eagerly deserializes entity payloads, so this must distinguish deferred rendering from T15's still-pending indexed/on-demand storage. The authority must not present future performance behavior as current.

Root inspected the new expected-only regression: it enables A, freezes period A, changes only Expected A to Expected B, asserts the rejected enable leaves definition and occurrences unchanged, then applies the matching B plan and enables while retaining the old request/period contract. Independent test execution was attempted, but compilation failed during concurrent shared-control signature and command/status API migrations. This is not passing evidence. Root will rerun against a coherent handoff instead of accepting source inspection alone.

Source recheck confirms T08 unit tests are now enabled and T13 startup loads stop intent protection before recovery; these resolve the specific first-review implementation direction, but runtime and fault-injection acceptance remain pending.

## Follow-up archive validation and multi-stop recovery corrections

T22 now separates retained attempts from signature-matched historical completion. Root found its first implementation reset the visiting set and validation cache inside each historical candidate check, permitting unbounded recursion on a stored dependency cycle and repeated graph traversal. Returned to preserve shared cycle tracking/cache, compute the guarded signature once and add cyclic/shared-dependency regressions.

T13 now loads stop intents before recovery, but root found the initial reconciliation helper still returned true after a failed save. With multiple records, it could break before processing later stopped periods and then run recovery; a subsequent successful save could clear the temporary error and permit replay. Root required successful reconciliation before recovery and per-occurrence intent admission guards independent of storage_error, with a two-record/first-write-fails-once regression. Neither task is accepted.

## T10 audit reviewed; implementation authorized with scope corrections

Root reviewed the [T10 transition inventory](evidence/T10-initial-origin-draft-audit.md). Accepted origin-first drilldown Back, structural sidebar/breadcrumbs, explicit dirty Save/Discard/Stay and confirmed-save-before-navigation. Rejected proposed exceptions that would omit origin restoration for global-search selection or result-to-task drilldown merely to preserve old assertions. Recently opened must preserve target kind (report versus task); comment and inline research composer sole drafts require preservation too. Explicit cancellation of a small single-field editor can keep ordinary discard semantics, but unrelated navigation may not silently lose input.

Root authorized pure context/dirty/derived-planning modules and tests immediately, with an exact API handoff before wiring hot shared UI files. Subsequent S2-S6 remain required; a pure helper slice cannot close T10. T08 will consume the shared guard and origin mechanism, and Save-and-continue must consume T13's confirmed persistence result.

T17 handoff was reviewed and returned for remaining semantic corrections: Quick has no per-period Topic, phases are ordered within each approved-order Goal, retry activity uses recorded timestamps rather than unconditional newest ranking, and system-interrupted recovery differs from explicit user cancellation. Current synchronous pending-command state must not be confused with T14's future asynchronous worker acknowledgements.

## T23 and T25 independent acceptance

T23 and T25 are accepted in the reviewed working-tree candidate, not committed or released. Root independently reran the complete planning QA (exit 0), the 25 request-planning tests (exit 0), the legacy SQLite reopen regression (exit 0), and the expected-only negative approval regression (exit 0). Evidence: [planning QA](evidence/T23-T25-independent-final-planning.log), [request tests](evidence/T23-T25-independent-final-request-tests.log), [legacy reopen](evidence/T23-independent-legacy-reopen.log), [expected-only rejection](evidence/T25-independent-expected-only-negative.log).

The real wizard/fake-CLI flow captures Objective A then B, expected-only C, unchanged replanning and failed-plan retry. It verifies immutable context, stale/dirty completion rejection, prior frozen-period preservation, reviewed-plan enable and restart-owned history. The separate negative test rejects Expected B against an unreviewed A plan without changing definition/occurrences, then accepts the matching B plan. Legacy absent fields remain absent/defaulted without rewriting historical inputs; unmatched ownership remains readable. Earlier independent full automation and unit evidence remains applicable to the reviewed behavior, with later integration gates required.

Root inspected the rendered history and QA assertions: three distinct attempts expose their own objective, expected result, revision and status; ordinary Research selection/counts exclude automation-owned attempts; ambiguous legacy history stays accessible. Close restores the exact Review opener and history scroll at 1200×900; Escape restores the legacy opener and the exact settings Review opener at 1000×680. The QA uses real visible controls and a newly opened narrow window. Root reviewed [narrow history](evidence/T23-independent-planning-history-narrow.png) and [dark attempt detail](evidence/T23-independent-planning-attempt-3-dark.png). Global native keyboard/visual regression remains T19/T20, not silently waived by these task-specific checks.

## T13 stop-retry durability review correction

Root found another nondurable-stop path: `retry_save` re-enters `prepare_stop_recovery` using the current in-memory Store. After reconciliation changes an occurrence to Cancelled but its save fails, the next retry's terminal branch can mistake that memory state for a loaded durable state and clear the stop journal. Returned to OpenCode: require a confirmed saved terminal state or successfully persist before clearing; add repeated-save-failure and reopen/no-replay coverage. T13 remains unaccepted. This is a source-level review finding; correction and runtime verification are pending.

## T06/T22 scope audit and independent rendered checks

Root independently reran the expanded result matrix: exit 0, artifacts `.local/result-presentation-qa/82892`; [log](evidence/T06-independent-stale-matrix.log). Inspected single/multi-goal stale labels, preserved report/evidence bodies and dark narrow presentation. Copy/export parity passed. T06 remains Open: its acceptance explicitly requires multi-goal Research automation across all seven states, while complete/running/failed/cancelled/regenerating fixtures still use one Goal. Returned for true multi-goal cases and visible aggregate assertions; existing partial/stale multi-goal coverage cannot stand in for those states.

T22 pure archive tests independently passed ([log](evidence/T22-independent-archive-unit.log)), including historical cycle/shared-dependency cases. The rendered archive suite exited 101 at missing `task-status-1` ([failure](evidence/T22-independent-rendered-failure.log)); returned for diagnosis without removing the status assertion. Root also returned the archived step selector's `complete = retained_result.is_some()` because that feeds Done even for malformed/stale successful output. Retained readability and validated completion must remain separate for steps as well as Goals. T22 is not accepted.

## T13 independent S1 checkpoint and journal-version correction

The independent command QA exited 0 ([log](evidence/T13-independent-S1-interim-command-qa.log)), including repeated save failure after an in-memory terminal mutation. Root source review confirms that test checks the stop journal remains present while the durable database is still nonterminal, and only a successful later save clears it. S1 remains under review and does not complete T13.

Root returned another concrete journal issue: load rejects future versions, but record/clear used an unchecked parser and then assigned the current version, permitting an emergency stop to overwrite an unsupported future file. Mutation paths must reject unsupported versions without changing bytes, with record/clear regressions. Stale ignored-record wording must reflect paused automation admission.

## T21 audit dispatched

OpenCode Go `opencode-go/deepseek-v4.1-flash` is auditing T21 using `.local/opencode-dispatch/T21-audit-prompt.txt` (active log `T21-audit.jsonl`). This bounded read-only phase traces title, description, comment and material input effects; metadata preservation; affected dependency graphs; retained revision history; and scoped Undo. It must propose coordinated implementation slices around T06/T22 result ownership, T13 commands and T10 drafts. All T21 requirements remain in scope; source implementation is not yet authorized to overlap those owners.

## T10 pure context review and empty-results return correction

Root independently ran the initial navigation-context suite: exit 0, 18 passed ([log](evidence/T10-independent-initial-context-tests.log)). Source inspection found an uncovered origin edge: an empty Research results caller (`task=None, run=None`) falls back to All research, although opening a global-search hit and returning should restore the empty results route. The context also needs the results project/Goal filters. Returned for explicit empty-selection versus deleted-selection handling and tests, with a notice for genuine fallback. Pure tests do not accept T10; destination, dirty-exit, origin/focus/scroll wiring and the complete user-path QA remain required.

Root requested coherent S1 API handoff before shared UI integration. T13 likewise received the independent passing command-QA checkpoint and a request for a bounded S1 handoff after the future-journal guard, followed by the required atomic dispatch/scoped Undo phases. T11 independent visual QA is running under `.local/opencode-dispatch/root-T11-visual-current.log`; no result is claimed until its process exits and rendered evidence is reviewed.

T11 independent visual QA subsequently exited 0 ([log](evidence/T11-independent-visual-checkpoint.log), artifacts `.local/t11-qa/85874`). Root inspected dark/narrow action boundaries and light/wide wrapped recent entries. The claimed enabled fixture actually recovered into Needs attention/Checking after restart with Resume schedule primary; existing assertions checked geometry/presence rather than the claimed state. Returned for an actual enabled-idle fixture, explicit disabled/selected examples and observed state assertions. T11 remains Open until those and the required icon/unit/regression checks pass.

## T13 S1 reviewed; S2 atomic dispatch started

Root reviewed the [S1 handoff](evidence/T13-S1-reviewed-handoff.md), including remaining two-write dispatch and whole-Store Undo call sites. The independent four stop-journal tests now pass, including future-version record/clear byte preservation ([log](evidence/T13-independent-stop-version-tests.log)). The first invocation was blocked by a concurrent navigation test variable shadowing compile error; the owner corrected it before the successful rerun.

Root authorized S2 using `.local/opencode-dispatch/T13-S2-prompt.txt`. After confirming S1 process termination, root resumed the same OpenCode session/model with `T13-S2.jsonl`. Required next boundary: launch validation, run plus occurrence link committed together, then the existing queue effect; tick/completion/recovery/expiry/retry must preserve durable-before-effect ordering. Actual failure/crash/retry and write-count evidence is required. S3-S5 scoped Undo migration remains required afterward; S1 is not full T13 acceptance.

## T08 independent UI checkpoint

Root independently ran global-search QA: exit 0, artifacts `.local/global-search-qa/86721` ([log](evidence/T08-independent-global-search-checkpoint.log)). Reviewed the dark/narrow selected period and its exact period identity. Requested final implementation handoff and removal of debug logging. T08 activation still needs T10 shared origin/dirty integration; no competing local origin stack is authorized. T08 remains acceptance-pending while that contract is completed.

Root also independently ran the full current unit suite with Node 24 on PATH: exit 0, **465 passed, 0 failed, 2 ignored** ([log](evidence/integration-independent-465-unit-pass.log)). This is an integration checkpoint while S2 and navigation/visual work continue, not release approval.

## T10 integration authorized; planning-label review correction

Root reviewed the [S1 API handoff](evidence/T10-reviewed-S1-api.md) and authorized S2-S5 destination/origin/dirty-exit wiring with S6 end-to-end QA still required, using `.local/opencode-dispatch/T10-integration-prompt.txt`. T13 retains persistence/runtime/launch ownership; T08 will consume published origin/dirty hooks instead of creating another guard. T10 remains Open.

Before planning-label wiring, root returned loose derivation in `automation_planning_state`: a successful process plus any old template does not establish a matching reviewed plan; an old approved definition must not hide a failed new replan; an older active attempt must not override the authoritative newer attempt. Requested current request/revision/signature binding checks and regressions. Pure policy tests alone do not establish correct user-visible planning state.

## T22 archive visibility diagnosis

Root traced the earlier missing archived row to a likely product-path filter: `visible_tasks` excludes closed phases outside All research when `show_done` is false. A correctly classified archived completed Goal now has Done phase and can therefore disappear from default Archive List/Board. Returned to fix actual Archive semantics and prove visibility via the sidebar entry with consistent counts, rather than only toggling a fixture filter. A fresh archive invocation was compile-blocked by the active result-fixture expansion, so no new runtime pass is claimed.

T13 was also notified of the T10 owner's observed intermittent stop-journal test failure; a passing rerun alone is insufficient to dismiss a new helper's potential fixture collision. The owner must diagnose the recorded failure while progressing S2.

## T21 audit reviewed and Phase 1 authorized

Root reviewed the [input-invalidation audit](evidence/T21-reviewed-initial-audit.md) and dispatched pure impact/classification/history helpers through `.local/opencode-dispatch/T21-phase1-prompt.txt`. UI, commands and final QA remain required after the helper handoff. Root decisions override preliminary recommendations in the audit:

- Separate whole-goal permission withdrawal from actual changed-input/downstream freshness. A missing executable result due to revoked approval is not proof that every sibling input changed; use a branched-DAG test and preserve unaffected history while execution stays blocked.
- Frozen automation-period execution inputs are read-only through both UI and mutation boundaries. Future changes use definition/replanning, not silent mutation of an old period.
- Working-directory changes are in scope: preview affected work and record an explicit approval consequence, without invalidating all saved approvals through an unplanned signature-format migration.
- Single-report historical copy/export suffices; current-goal ZIP remains current-only. Display a stable digest of the persisted signature with run/time, not a raw JSON prefix.
- A blocking compute-on-Save impact preview plus persistent explanation is acceptable. Comments/materials require equivalent precommit handling; cancel must not silently accumulate newly imported unreferenced copies.
- T13 owns conditional scoped restoration of affected approval/stage. T21 found TaskFields inverses currently omit that consequence; root returned the contract for S3, preserving runtime history/activity and unrelated changes.

## T11 revised state matrix did not pass

Root icon verification passed: all 51 explicit Lucide references are embedded. The independent expanded T11 matrix exited 101: `t11_qa.rs:168` expected the Research shape to be checked but observed false ([log](evidence/T11-independent-selection-state-failure.log)). Returned to diagnose intended default versus actual selected/toggled state and rendering, without weakening the assertion merely to pass. T11 remains Open.

The stop-journal repeat test process is still running under `.local/opencode-dispatch/root-stop-intent-repeat.log`; passing individual iterations are not yet a completed ten-run result. Concurrent compiler work is live, not a terminal blocker.

Follow-up: the repeat process terminated on iteration 3 with a concurrent `project_app.rs:174` E0502 iterator-borrow compile error; the first two iterations passed 4/4. This neither reproduces nor disproves the reported intermittent journal assertion. Returned the compile error to T10. After confirming the T21 audit process had exited, root resumed its same session/model with `T21-phase1.jsonl`; Phase 1 implementation is now running.

## T13 S2 interim dispatch review

Root returned two new S2 hazards before acceptance. First, `commit_prepared_dispatch` cleared `pending_commit` on any executor error, although that error can come from retrying a different pre-existing pending command; this can discard earlier unsaved user intent. Preserve that intent and distinguish dispatch-specific failures. Second, exact-intent requeue ran before expiry/deadline checks and its command ignored `now`; require current admission/expiry validation and tests for crash-before-queue then reopen after expiry. The early any-Queued guard also made the Queued exact-intent branch unreachable; distinguish an actual in-memory queued effect from a committed but unapplied intent. T13 remains Open.

## T08 caller matrix completion dispatched

Root reviewed the [T08 handoff](evidence/T08-initial-reviewed-handoff.md). Its event-loop QA correction is appropriate: input Change subscriptions settle after the update turn; real typing is retained and assertions run after settling. The route inventory still omitted explicit Project and Topic callers required by TASKS. After confirming the first process ended, root resumed the same session/model with `.local/opencode-dispatch/T08-route-completion-prompt.txt` / `T08-route-completion.jsonl` to add these two cases. Shared origin/dirty activation integration with T10 is still required.

## Visual/archive rerun checkpoint

Root verified that new-form default is Quick (the accepted T09 contract), so T11's revised test now checks Quick selected first and then clicks Research to prove selection moves. The subsequent independent T11 and archive invocations both exited 101 during concurrent archive QA/sidebar count instrumentation compile errors ([build log](evidence/integration-archive-visual-intermediate-build.log)). The owner has since corrected the loop-scope and opaque-ID formatting source, but runtime verification remains pending; no pass is inferred from the source correction.

## T10 request-save retry review

Root source review found `capture_research_request` creates the request/placeholder in the live Store before saving; on persistence failure the composer remains open while that unsaved request remains in memory, so pressing Save again can create a duplicate request/topic. Returned for a staged or idempotent pending intent, confirmed durable success before navigation/dispatch, preserved draft on failure, and a real composer failure/retry regression proving exactly one request/topic while preserving background changes. Empty `storage_error` alone is not a durable receipt.

## Stop-journal repeat verification method

The journal test fixture now uses a process-wide atomic sequence in its temporary directory name, removing same-timestamp parallel collisions. Root started a `cargo test --no-run --message-format=json` build, selects only an artifact explicitly marked `profile.test=true`, copies that test executable into `.local/opencode-dispatch/stop-repeat-candidate`, and runs the four journal tests in ten independent processes. This avoids repeatedly rebuilding against concurrent UI edits and never launches an unverified application binary. Completion is pending the actual process exit.

The fixed-binary repeat completed successfully: **10 independent process runs × 4 journal tests**, all actual exit 0 ([log](evidence/T13-independent-stop-repeat-10.log)). The tested fixture includes the atomic sequence. This closes the repeated verification checkpoint for that correction; S2 runtime integration remains unaccepted.

## T21 impact analysis interim review

Root returned three Phase 1 issues: directory-impact iteration chained before/after keys without complete deduplication and could double counts; `NoResult` returned before input comparison, causing an approved but never-run Goal's directory change to omit renewed-review consequences; and `(_, None) -> AffectedByUpstream` attributed pre-existing missing dependency signatures to a metadata-only edit. Required separate input-delta and result-availability axes, union iteration, exact-count tests, approved/no-results directory coverage, and pre-existing failed-dependency branched-DAG tests. No execution approval is weakened.

## Coherent UI verification checkpoint

Root `cargo check --quiet --features qa` exited 0. A build-artifact-selected QA executable is being copied to `.local/opencode-dispatch/review-ui-checkpoint/planning-qa`, with SHA-256 recorded, then used for T11, archived research and result presentation in sequence. The same binary across suites prevents concurrent source changes from changing the candidate midway. Results remain pending actual exits. This is an intermediate verification candidate, not the final frozen full-release gate.

## T06 independent acceptance

Root independently ran the coherent artifact-selected QA executable (SHA-256 in [checkpoint identity](evidence/visual-result-checkpoint.sha256)): the full result-presentation matrix exited 0 ([log](evidence/T06-independent-full-result-matrix.log), artifacts `.local/result-presentation-qa/103`). Coverage includes Quick, ordinary Research and true multi-goal Research automation across complete/running/partial/failed/cancelled/stale/regenerating states; reader/copy/export parity and retained evidence assertions are included. Root inspected partial second-goal and stale/historical report screenshots and reviewed typed lifecycle/validation precedence, malformed successful output, legacy fallback and regeneration handling. A fresh root full unit run exited 0: **483 passed, 0 failed, 2 ignored** ([log](evidence/integration-independent-483-unit-pass.log)). T06 is accepted for this working-tree candidate. T21 input-history integration must preserve these contracts; final T19/T20 native/release gates remain.

## T11 independent visual gates

The same fixed executable passed [T11 state matrix](evidence/T11-independent-final-state-matrix.log), [affordance](evidence/T11-independent-affordance.log) and [header layout](evidence/T11-independent-header-layout.log), all actual exit 0. Root inspected enabled idle, selected Research versus primary Continue, draft disabled actions, and long-name screenshots at both sizes/themes. Icon validation passed all 51 embedded references; the fresh 483-test full unit gate also passed. T11 final sign-off awaits maintained DESIGN documentation of the compound-control exceptions. Headless header geometry is not proof of the still-pending native narrow-window resize gate.

## T13 S2 retry-kind review return

Root found that `PendingDispatch` only stores occurrence/task/files, but `launch_run` also submits request planning and report regeneration. Generic retry via `prepare_run` or `prepare_occurrence_run` loses planning request/revision ownership and regeneration source/style; terminal-period regeneration cannot retry through the ordinary active-period branch. Returned to preserve a typed operation intent and atomically reprepare its correct ownership/mapping, with failure/retry coverage for all three variants and an explicit stale-intent resolution path. T10 was notified of the narrow project planning integration seam. T13 remains Open.

## T22 confirmation evidence return

The fixed checkpoint archive suite failed on workspace lock contention. After its fixture repair, the owner encountered an active-archive confirmation assertion failure and replaced the prompt assertion with only unchanged archive/run state. Root rejected that weaker proof: an inert button also passes it. Required a real confirmation invocation plus Cancel and Confirm outcome checks using supported QA/native mechanisms. T22 remains Open; neither failure is counted as a pass.

## T11 independent acceptance

The owner applied the authorized narrow DESIGN Components/Boundaries update. Root checked the maintained text against the factories: bordered standalone secondary/icon actions; explicitly limited compound row/menu/segmented exceptions; preserved shared-border task tabs; accent/toggled selection distinct from primary; active Stop/Cancel danger distinct from ordinary closing; neutral history surfaces, trigger-source labels and full long-name disclosure. Root reran icon validation: 51 registered references embedded. Together with the independent state/affordance/header matrices, reviewed light/dark wide/narrow screenshots and 483-test unit gate above, T11 meets its task criteria and is accepted in this working-tree candidate. [Reviewed handoff](evidence/T11-reviewed-final-handoff.md). T19/T20 still own comprehensive native keyboard/readability and final integration regression; this is not a release claim.

## T14/T15 architecture preparation dispatched

Root started a new local OpenCode Go DeepSeek v4.1 Flash audit with `.local/opencode-dispatch/T14-T15-audit-prompt.txt`. It is read-only except its handoff document and must inventory actual synchronous saves/report consumers, propose exact T13 integration seams, ordered acknowledgements, incremental writes/indexed pagination/lazy bodies, versioned backup/migration and representative measured gates. No persistence implementation is authorized before the audit is reviewed. The full T14/T15 scope, including main-thread snapshot costs and real UI query paths, remains required.

## T21 additional Phase 1 returns

Root returned run-ID/vector-order history selection in favor of the accepted recorded-time ordering contract, and a confirmation predicate that omitted approval withdrawal for approved never-run work. Requested edge tests and corrected cancelled-import cleanup documentation. The initial Phase 1 process was confirmed terminal before resuming its same session/model for these corrections. UI and domain enforcement remain unimplemented and unaccepted.

## T10 scroll restoration review return

Root found that the current Research List origin records a constant top index and Topic records a default scroll offset. Returning therefore loses the user's position. Returned for actual list/board/topic scroll capture and restoration with nonzero-scroll nested Back tests. The production GPUI uniform-list state exposes its base scroll handle; the test-only logical-index convenience method is not a reason to omit behavior. Research result origin currently derives project/topic ownership and then ignores those fields on restore; required an inventory of actual caller context and preservation without inventing new filters. T10 remains Open.

## T13 typed retry integration returns

The new typed retry intent addresses the earlier lost-operation-kind design, but root found `automation_owned` recognizes only the normal occurrence variant, excluding automation report regeneration. This would label it with Research slot feedback and send its explicit progress action into Research results. Returned for ownership derived from the validated run and route/pool regression coverage. Root also returned request dispatch's missing current revision/signature check at the command boundary, and required consistent period regeneration linkage plus source/approval/archive validation. Preparing an obsolete run must not bypass current invariants merely because its request ID still exists.

An independent QA build exited 101 while six command QA callers still lacked the new LaunchIntent parameter (`.local/opencode-dispatch/review-navigation-command-checkpoint/build.jsonl`). A subsequent independent T21 focused-unit attempt also exited 101 on the new T10 `BTreeMap<Stage, ScrollHandle>` requiring `Stage: Ord` (`.local/opencode-dispatch/root-T21-phase1-tests.log`). Both concrete integration errors were returned to their owners; neither command is a runtime pass. The previous coherent 483-test checkpoint remains historical evidence only.

## T12 consolidation audit dispatched

Root started `.local/opencode-dispatch/T12-audit-prompt.txt` with the required OpenCode Go DeepSeek v4.1 Flash model. The read-only audit must enumerate every actual route/selection/overlay writer and entry handler, propose one typed authority with T10/T08 seams, and provide complete transition/regression matrices. A mirrored enum plus surviving contradictory legacy route fields does not satisfy T12. Implementation remains gated on the reviewed audit and T10 integration, without reducing T12 scope.

## T21 pure phase review and UI authorization

Root independently reran the corrected pure module: actual exit 0, **18 passed**, no filtered-suite failures ([log](evidence/T21-independent-phase1-18-tests.log)). Source review confirms independent input-delta/approval/result axes, deduplicated directory impact including approved never-run work, unchanged missing-dependency handling, recorded-time history ordering, and precommit confirmation independent of result availability. The [reviewed API/audit](evidence/T21-reviewed-phase1-api.md) is suitable for UI integration; this is not full T21 acceptance.

Root authorized narrow Phase 2a/2c impact dialog and previous-version UI via `.local/opencode-dispatch/T21-phase2-authorization.txt`. T21 must publish its exact integration seam and preserve T13 command/inverse ownership. T10/T13 were notified: Save that opens impact confirmation is pending, not a durable receipt; cancellation keeps the original draft/route and navigation waits for confirmed successful persistence. Comment/material/directory/frozen-input command enforcement and scoped Undo remain required.

## T13 retry material preparation return

Root found fresh ordinary/regeneration retry runs bypass `prepare_run_materials` while reusing old pending file paths. That helper supplies attachment snapshot/context/acceptance clauses, browser handoff guidance and validation feedback; rebuilding without it can silently drop user evidence from the run contract. Returned to revalidate/rebuild materials through the same preparation path and cover failed-save retries with attached, missing/changed and handed-off browser cases. Exact committed recovery also needs valid file payloads from the persisted attachment snapshot where applicable, without rewriting or duplicating its saved prompt. T13 remains Open.

## T22 confirmation QA approach reviewed

The owner replaced the inert-state-only check with GPUI's supported fallback prompt renderer and real Cancel/Confirm clicks. Root reviewed this as a valid headless behavioral test approach, while native prompt appearance remains a final native gate. The next independent QA build still exited 101 on missing archive-QA helper imports and a concurrently registered but not yet defined T13 typed-intent test function (`review-navigation-command-checkpoint/build-r2.jsonl`). Root requested compile-ready handoffs before another independent build. Root also returned the arbitrary eight-owner restore limit, which could claim restoration while a deeper archived ancestor still blocks the task; require complete cycle-safe restoration or an explicit unresolved outcome.

## T10 S5/S6 continuation

The S1–S4 owner process was confirmed terminal after its [stage handoff](evidence/T10-reviewed-S1-S4-handoff.md). Root resumed the same required-model session with `.local/opencode-dispatch/T10-S5-S6-prompt.txt` to finish the already-authorized dirty-exit/search integration and real-control QA. The revised source captures production scroll handles; root additionally returned skipped zero-offset board columns, since restoring only nonzero columns leaves another page's scroll behind. All S5/S6 exit routes and nonzero-scroll tests remain required before acceptance.

## T14/T15 architecture review and S0 authorization

Root reviewed the [architecture audit](evidence/T14-T15-reviewed-architecture-audit.md). It correctly identifies whole-store equality/cloning/hashing in addition to SQLite writes, the occurrence-heavy baseline's insufficient report-body load, and the existing synthetic in-memory history benchmark's limited scope. Root authorized only isolated S0 instrumentation/fixtures/baseline measurement (`.local/opencode-dispatch/T14-S0-authorization.txt`), after confirming the audit process was terminal and resuming the same model/session. Worker/schema behavior changes remain gated.

Design decisions preserve full TASKS scope: entity-scoped changes and ordered acknowledgements must remove main-thread whole-store costs; lossless frozen-snapshot dedupe and WAL/FULL are allowed with preservation/backup/crash proofs; external modification fails closed without losing unsaved edits; lazy bodies require compile-enforced hydration, including validation/recovery/export consumers. Scoped Undo remains a T13 prerequisite; capping whole-store snapshots is not a final solution. Atomic cursor+period and run+link changes cannot be split into separately committed batches merely to hit timing.

Root returned protocol hazards before implementation: shadow/live comparison must account for newer unacked generations and cannot reintroduce periodic UI stalls; coalescing must preserve chronological delete/recreate semantics; async rollback cannot overwrite newer same-entity runtime events; failed-generation successor/retry/receipt handling must be explicit; pending approval/enabling cannot admit execution before acknowledgement. Dirty tracking must be complete rather than relying on periodic corrective full writes.

## T21 UI seam review

Root reviewed `T21-UI-integration.md` and returned two integration requirements. First, current `save_draft` closes the editor before persistence, so a wrapper is not enough: T13 must provide a durable outcome and failure retention, consumed by T10/T21. Second, unchanged-sibling history must not hide a stale badge while copy/export retain a contradictory notice; one presentation value must drive all three, keeping T06 report quality separate from execution approval. A narrow verified-input history adapter is authorized; T22 was notified of the ownership seam.

## Coordinated artifact-capture window

The third independent QA build exited 101 on an unregistered T21 module and T10 callback return-type migration (`review-navigation-command-checkpoint/build-r3.jsonl`). Root requested T10/T21/T13/T22/T14 to finish their next small compile-coherent boundary, then pause source edits and report READY FOR ARTIFACT CAPTURE. This is temporary engineering coordination, not task completion or a user approval gate. Root will build/copy/hash one coherent executable and explicitly resume owners after capture; independent QA can then run without rebuilding against concurrent changes. T12 remains read-only. No new acceptance is claimed from these failed builds.

## Stable artifact captured and owners resumed

Root build exited 0 and pre/post Rust/Cargo source fingerprints matched. Copied executable SHA-256: `c217ecf1b5abf6fef14429d2e354f804bef1f9a3bcc5a9d2eab27c30537a1044` ([identity](evidence/navigation-command-checkpoint.sha256), [source fingerprints](evidence/navigation-command-checkpoint-sources.json)). T10/T21/T14 resumed their authorized scopes after capture. QA uses this unchanged executable while implementation continues.

Independent results: [archive behavior](evidence/T22-independent-archive-behavior-pass.log) exit 0, artifacts `.local/archive-qa/20786`; [global search including Project/Topic callers](evidence/T08-independent-project-topic-search-pass.log) exit 0, artifacts `.local/global-search-qa/20816`; [command QA](evidence/T13-independent-typed-retry-fixture-failure.log) exit 101 at `regeneration_fixture`, which attempts the final report before completing its Analyze prerequisite. Root returned the fixture to create genuine dependency-ordered validated results without weakening T05 eligibility. These new regeneration checks have not passed. T08 still awaits shared T10 dirty/origin activation integration. Full automation regression was started on the same fixed executable; result pending.

## T22 visual acceptance failure after behavioral pass

Root reviewed the actual screenshots and rejected T22 despite its automated pass. [Archived detail](evidence/T22-review-failed-archive-detail-light-1440.png) says 3/3 complete while every step says Needs refresh; archive admission is being confused with input freshness in the overview rows. [Narrow archived Board](evidence/T22-review-failed-archive-board-dark-1000.png) still instructs users to drop Todo tasks into Execution, shows active empty dropzones/create controls, and leaves the only archived card outside the initial viewport. This does not meet the intended historical/Restore interaction contract.

Root resumed the confirmed-terminal T22 session with `.local/opencode-dispatch/T22-visual-return-prompt.txt`, requiring consistent historical step labels, removal of misleading active Board actions/guidance, discoverable nonempty archived columns/cards, and visible-control assertions rather than mere element existence. T22 remains Open. The supported fallback confirmation test is preserved; final native platform appearance remains a separate gate.

## T04 reopened by independent T13 integration regression

Root's full automation run on fixed executable `c217ecf1` exited **101**, after routing/retry/background-completion/queue-chain/independent-pool checks passed. `automation_qa.rs:3123` expected Pause to clear the legacy whole-Store Undo snapshot, but observed one remaining snapshot ([log](evidence/T13-independent-automation-undo-regression.log)). Root traced the command commit path bypassing the existing persistence safeguard and returned it to T13. Until all whole-Store Undo paths are replaced, durable automation mutations and their retry/recovery paths must preserve T04 invalidation and its explanation. The regression assertion must not be removed to hide incomplete scoped-Undo migration.

T04 is reopened for the current integration candidate; its earlier pass remains historical evidence. Current fully accepted task count is **11/25**, with T04 reacceptance gated on corrected independent automation regression. T13 S2 remains unaccepted.

## T12 architecture reviewed; pure S1 authorized

Root reviewed the complete [navigation architecture audit](evidence/T12-reviewed-navigation-architecture-audit.md), including direct route writers, contradictory task/result/overlay combinations, transition matrix and migration seams. Authorized only new pure route/controller types and meaningful transition tests through `.local/opencode-dispatch/T12-S1-authorization.txt`; live state-family migration waits for T10 S6 and the durable-save seam. No mirrored legacy route fields may remain as the final authority.

Root clarified that T23 owns planning-attempt identity, not all ordinary result routing; T12 owns ordinary selection consolidation using existing contracts. Automation periods remain overlays over explicit detail/results bases. The model must include T21 impact/history overlays and valid nested confirmation flows rather than imposing an unexplained stack depth of three. Pending impact and pending durability are explicit non-success outcomes; invalid ownership must reject atomically. Full headless/source/native transition verification remains required after wiring.

The audit also found a user-visible background write: plan arrival sets the wizard step to Review even after the user goes Back. Root required T10 to make completion attach-only, retaining explicit Continue/Review progression and matching editor/request checks, with a Back-during-planning regression. This is tracked within T10 before its acceptance.

## T13 interim Undo correction awaiting independent rerun

The owner has restored legacy invalidation at command commits and durable lifecycle saves and corrected the regeneration prerequisite fixture. Root observed the changed call sites, but this is not reacceptance: command and full automation gates must actually pass on the corrected candidate. T04 remains reopened and T13 S2 remains pending.

## Independent retry/Stop checkpoint returned

Root tested fixed executable SHA-256 `4b4d362581f759e494e7d6510b8f479e09270bbeef0d3bad7a6f96bfc06f0116` ([identity](evidence/undo-retry-checkpoint.sha256)); this is an executable checkpoint, not a claim of frozen source. Both gates exited **101**. [Command QA](evidence/T13-independent-material-fixture-failure.log) indexed an empty historical attachment snapshot: the fixture attached material after preparing the source run. Root returned realistic dependency/material preparation and baseline run-count assertions, without weakening production eligibility. [Full automation](evidence/T13-independent-stop-backend-failure.log) passed the exact Undo-protection scenario, then failed the held-backend cancellation assertion: persisted cancellation omitted the fake/backend cancellation signal and runtime tracking cleanup. Root returned equivalent backend cleanup/signal semantics, journal ordering, and failed-storage coverage to T13.

The current source contains a candidate Stop correction; it remains unverified until independent rerun. T04 stays reopened until the required full automation gate passes. T13 S2 remains unaccepted; the fully accepted count remains **11/25**.

## Root review of navigation, history and measurement candidates

Independent `cargo build --features qa --message-format=json` exited **101** (`.local/opencode-dispatch/root-current-build.jsonl`): T12's new pure module derives Copy for owned route/string values and lacks required Eq/Ord traits. Root returned these compile errors and reviewed its actual API: unchecked initial routes, unvalidated result filters, a successful no-op query setter, and confirmation transitions that retain completed edit/decision/impact overlays must be corrected. Impact-review ownership must match the underlying editor, not merely reference an existing task. These are S1 review findings, not permission for live navigation migration.

T21 currently suppresses all previous-version entries for archived tasks. Root returned this to T21/T22 for read-only archived history access or proof of an equivalent complete existing route, preserving the original requirements for historical reports and separate execution permission.

T14 S0 measurement review found that attempted forced saves are counted as persisted without checking storage errors or durable state. Root required attachment/count validation, actual durable save confirmation outside timed spans, explicit throttled-streaming write counts, and honest sample-count/body-byte limitations. The baseline harness must fail on failed setup or failed writes rather than print a misleading success. Worker/schema implementation remains gated on the reviewed S0 evidence and T13 prerequisites.

## Typed command retry passes; period-origin expectation reviewed

Root independently tested executable `15d09dddaad72f2578b4250ea4e19fdc7ce3fa577f4a4f655eb94c14a6dd1bc8` ([identity](evidence/s2-material-checkpoint.sha256)). [Command QA](evidence/T13-independent-material-retry-pass.log) exited **0**, confirming the corrected typed retry/material scenarios on this executable. [Full automation](evidence/T10-independent-period-origin-assertion-failure.log) exited **101** later at `automation_qa.rs:5186`, after Undo protection and the Stop backend checks progressed successfully.

Root inspected the failing scenario: it enters Automation results, opens a period, opens a step and presses Back. The observed `AutomationRuns` base preserves its origin; the old assertion expects `Automations` and therefore encodes the unwanted destination change. Root authorized T10 to update this exact expectation and strengthen original filter/scroll/close assertions, with a separate detail-origin case. Production must not be regressed to satisfy the outdated assertion. This is a reviewed contract correction, not blanket permission to weaken tests. T04 remains pending the corrected full gate; T13 S3/S4 remain required.

## Independent navigation/archive integration checkpoint

The new independent QA build exited **0**. Root copied executable `95b104359e0a3000b7073baf72c123d23339f2df09b6c64cd9a392bd677dd10d` ([identity](evidence/navigation-archive-checkpoint.sha256)) before running four suites. This identifies the executable, not a frozen-source release candidate. All four exited **101**:

- [Automation](evidence/T10-independent-period-scroll-failure.log): the corrected origin destination passes, but the new period-results scroll assertion fails. T10 must prove a nonzero initial scroll and exact restoration with enough scrollable fixture content.
- [Navigation context](evidence/T10-independent-list-query-failure.log): Back loses the list query (`Row` becomes empty). Returned for input/capture/restore diagnosis and real-control regression.
- [Archive](evidence/T22-independent-narrow-step-visibility-failure.log): an archived step card is not visible in the narrow case. Root's [Board screenshot review](evidence/T22-review-improved-archive-board-light-1000.png) confirms the Done card and Restore now appear; [wide Steps](evidence/T22-review-improved-archive-steps-light-1440.png) has consistent archived labels. Narrow vertical content must be reachable by actual scrolling, not bypassed by existence-only assertions. The disabled archived-material area still advertises drag/paste actions and was returned for truthful read-only guidance.
- [Research inputs](evidence/T21-independent-impact-control-failure.log): the scrolling helper cannot find an observed `draft-scroll` target. Returned for real-control repair. Root additionally found screenshots taken after dialog dismissal and then deleted; evidence must capture open dialogs and remain available for review.

Root source review found T10 dirty detection omits stage/cycle/project fields that the editor can modify; a change to only those fields can be silently discarded. Returned to include every editable field, failed-save retention, and focused real-control tests. Existing S6 coverage also silently skips absent Search results and overstates Save/destination/scroll coverage; assertions must exercise the named behavior, both window sizes and themes, rather than only label it.

T12 S1 compile errors were corrected enough for the checkpoint build; semantic review remains open. Root returned confirmed CloseTop after editor-flow dismissal, which can incorrectly reject with NoDestination, and required checked construction in normal transition tests. T13's prior process was confirmed terminal; root resumed it for the S3/S4 command/inverse contract and current call-site inventory only (`T13-S3-contract-design.txt`), without authorizing implementation ahead of the full integration gate. No new task acceptance is claimed.

## Scoped Undo contract review returned before implementation

Root reviewed the [initial S3/S4 contract](evidence/T13-reviewed-scoped-undo-contract-initial.md) and returned concrete correctness gaps through `.local/opencode-dispatch/T13-S3-contract-return.txt`. Comments/material inverses need approval companions; imported copies must survive retryable save failure and remain protected by retained-run references; file size alone is not content identity. Runtime stamps must detect relevant same-ID state/result transitions without rejecting unrelated activity. Final scoped Undo must retain safe user-edit inverses across unrelated runtime work instead of continuing global invalidation.

The final Save outcome must distinguish in-flight durability from failure, retain an appropriate typed draft/intent for each editor, and commit combined fields/stage atomically. Multi-goal impact confirmation and typed entity payloads must support the future persistence worker without introducing more whole-Store snapshot costs. The revision is document-only, running in OpenCode session `ses_e44d01b81ffes04Md17JTpiln` using the required model; the existing T13 implementation session remains `ses_e44d01b81ffes04Md17UJTpiln`. No S3 implementation acceptance or authorization is implied by this review.

## Root diagnosis of recursive test-macro compilation

T12's unit attempt repeatedly reached the recursion limit at `research_input_app.rs:561` and eventually terminated rustc with SIGBUS after increasing the limit. Root inspected the dependency source: gpui-kit 0.8.1 `src/lib.rs:98` explicitly documents that the test-support glob includes GPUI's `test` macro and test modules must import types explicitly to avoid shadowing Rust's built-in `#[test]`. T21's `use super::*` imported the parent's GPUI glob into its pure tests.

Root returned the concrete fix to T21 (named imports in the pure test module) and instructed T12 to remove its temporary recursion-limit override instead of repeatedly compiling the same macro recursion. No tests may be disabled to hide this integration failure. Fresh actual-exit unit verification remains required after the correction.

## Macro correction applied; fresh unit compile failures returned

Root observed T21 replace the glob import with explicit names and T12 remove the temporary recursion-limit override. Before that correction, root terminated only the identified recursively expanding rustc child (PID 30752, parent cargo 30617) to release the Cargo lock; the OpenCode sessions and applications were not stopped. That terminated compiler is not verification success.

The independent node-enabled full unit command now exits **101** on ordinary compilation errors rather than macro recursion ([log](evidence/integration-post-macro-fix-unit-compile-failure.log)): an in-progress archive-material expression, Stage/Default in the expanded dirty-draft model, two navigation test fixture/assertion errors, and a history-test helper shadowed by a local variable. Each error was returned to its owner with its exact location. Subsequent source corrections remain pending a fresh successful full suite; no task acceptance follows from removal of the recursion alone.

## Independent unit suite restored: 518 passed

Root reran the full unit suite with Node 24 available after the concrete corrections: actual exit **0**, **518 passed, 0 failed, 2 ignored** ([log](evidence/integration-independent-518-unit-pass.log)). The normal test build no longer has the recursion-limit workaround. This validates the unit-suite checkpoint, including the new typed-navigation and history cases; it does not substitute for failing/pending UI, native, full automation, persistence or release gates. Task acceptance count remains unchanged.

## Independent 43,200-period persistence baseline

Root ran the S0 harness on a fixed copied executable (`30a6bc8a6508f4f52b154f3aaeedf9bcf85b429bd5ef80d07df4c7e4285b6522`) against an isolated copy of the retained 43,200-period fixture: actual exit **0**, all **20/20 forced interactions durable**, no forced-save failures, 20 streaming commits ([full measurements](evidence/T14-independent-occ43200-baseline.log), [executable identity](evidence/T14-independent-baseline.sha256)). Hardware: Apple M1 Pro, 32 GiB, development QA profile.

Measured interaction p95 was **2,119.980 ms** (20 samples); persistence-span p95 was **2,036.139 ms** (95 total samples, including unchanged calls); command preparation p95 was **297.447 ms** (10 samples). App open was 4,509.631 ms; process peak RSS reached 2,815,885,312 bytes. These are baseline observations under concurrent development/test load, not a controlled release benchmark or a claim of stable percentiles. They confirm that moving SQLite IO alone cannot satisfy the under-50-ms main-thread target.

The source fixture hash matched the retained fixture. The harness explicitly labels presentation-byte estimates as synthetic, rather than actual lazy-body reads. Root additionally returned the remaining harness case where an early failed forced save could be masked by a later successful cumulative save; status must fail whenever any forced interaction fails. T14/T15 remain unimplemented beyond S0; no performance acceptance is claimed.

## T21 independent behavior pass; visual contract returned

Root ran `--qa-research-inputs` on fixed executable `0f732d3f140780278caf90f597a1359545241fe6d26bc77b24900f25d0262cd4`: actual exit **0** ([log](evidence/T21-independent-input-history-behavior-pass.log), [identity](evidence/T21-input-history-checkpoint.sha256)). Evidence now captures actual open dialogs and survives the run. Root inspected narrow light/dark and archived screenshots, then returned these acceptance issues:

- [Impact button](evidence/T21-review-impact-label-light-1000.png) promises “Save and review plan”, but the save path remains on the edited step or follows a pending T10 exit. Use a truthful save label or implement the promised destination without overriding pending navigation. Assert the actual destination after the click.
- [History rows](evidence/T21-review-history-identity-dark-1000.png) omit task titles while showing full timestamps and unlabeled digests, making two Evidence steps hard to distinguish. Show the task identity, with revision metadata in a labeled secondary position; verify long titles at narrow width.
- [Archived history](evidence/T21-review-archive-freshness-conflict-light-1440.png) says inputs are unchanged while the selected report still says Stale input. Correct the common presentation adapter for rendering/copy/export, preserving genuine changed-input and evidence-quality distinctions.
- The revised Cancel test compares Store to itself, and a Save-visibility assertion becomes vacuous after checking pending state. Restore baseline/draft/route comparisons across closures; a behavioral pass containing tautologies is insufficient evidence for those cases.

T21 Phase 2 is not yet accepted. Domain enforcement, comments/material/directory impacts and scoped Undo remain mandatory T13 integration work.

## T12 pure S1 reviewed and independently verified

Root reviewed the [final S1 handoff](evidence/T12-reviewed-S1-handoff.md) and current source, then independently ran its focused tests: actual exit **0**, **28 passed** ([log](evidence/T12-independent-S1-28-unit-pass.log)). Checked construction, owner validation, matching impact/editor identities, atomic rejection, confirmed CloseTop, preserved nested Search, bounded return context and representative overlay grammar are covered. The temporary recursion override is gone; the new module is registered but not wired into live application navigation.

S1 is accepted as the pure foundation only. T12 remains Open: replacing live route/selection/overlay fields, adapting focus/page payloads, removing runtime navigation authority and the complete native/headless matrix remain required. T12's implementation process is terminal after this handoff; do not restart it until the next concrete migration slice is authorized.

Root also reviewed the reported T08 global-search failure at `global_search_qa.rs:934`: it asserts the old interim dirty-edit refusal text. T10 is authorized to replace that exact expectation with shared Save/Discard/Stay behavior and strong source/draft/durability/impact assertions. Search activation must not revert to an obsolete message contract merely to pass the old test.


## T13 S3a pure foundation authorized with a narrow sequencing adjustment

Root authorized the original T13 implementation session to implement domain-only atomic task edits and conditional inverses in focused new modules, with minimal module registration and meaningful tests. This allows architecture work to proceed after the independent S2 command pass while T10 repairs the remaining full-automation origin/scroll regression. It does not accept S2, T04, or full T13.

The existing command executor, CommandBase, TaskApp and live UI must remain unchanged in this slice. Inverse guards cover only changed fields and necessary approval/runtime companions; unrelated metadata and runtime updates must survive. Future integration must reprepare against current state or apply typed deltas, never adopt a stale whole-store candidate. Historical run immutability is approved in principle for future context changes, preserving portable relocation identity and exact impact previews. All remaining command migrations and material retention obligations remain required.


## T21 and T22 returned fixes: independent behavioral checkpoint

Root built and copied executable `1575d772bf92ca53f743cd30ad1398f4762e6cd4deb2b2304daf1b50f4f53ff1`, then independently ran both `--qa-archived-research` and `--qa-research-inputs`: each exited **0**. See [archive evidence](evidence/T22-independent-return-archive-pass.log), [input/history evidence](evidence/T21-independent-return-input-pass.log), and [binary identity](evidence/T21-T22-return-checkpoint.sha256). Visual inspection of the newly captured matrix and remaining integration/native requirements are still pending; these successful behavioral checks alone do not accept either full task.


## Returned checkpoint visual and scoped-edit review

Root inspected the newly passing T21/T22 screenshots. The [impact action](evidence/T21-return-truthful-save-light.png) accurately says Save changes; [history rows](evidence/T21-return-title-first-history-dark.png) now identify the task; unchanged archived inputs no longer show a contradictory stale badge. Two concrete gaps were returned to their terminal OpenCode sessions for implementation:

- T22: the [narrow archived-step properties row](evidence/T22-return-narrow-status-overflow.png) clips its status at the right window edge. Require bounded responsive wrapping/stacking and full light/dark size verification.
- T21: the handoff claims long-title coverage, but neither the fixture nor retained screenshots prove it. Add long English/CJK user titles and multiple attempts for the same task, visible compact attempt identity, actual revision selection and scrollable long content. Synchronize the seam document's stale statements.

Root also reviewed the in-progress T13 pure edit core and returned missing semantic runtime guards, stale impact/read-set validation, preflight atomicity, unrelated global approval reconciliation, unsafe field/value cross-product construction and concurrency tests whose updates occurred before preparation. No pure-foundation or live-integration acceptance is claimed for this candidate.


Root additionally ran the complete automation QA on checkpoint `1575d772`: actual exit **101** at the results-scroll restoration assertion ([log](evidence/T10-independent-return-scroll-still-fails.log)). Earlier checks, including Undo protection, passed. The full scenario still lacks a proven initial nonzero scroll precondition; T10 must distinguish fixture/input-event failure from restoration failure, capture exact offsets, and verify both period Back and Close. T04 and full S2 acceptance remain gated.


## T14 S1a pure journal and acknowledgement foundation authorized

After completion of its running S0 measurements, T14 may implement focused typed journal/coordinator primitives in new modules, with injectable failure tests. This sequencing adjustment permits pure architecture work while T13's live command migration remains pending. No live Store mutation tracking, UI persistence change, worker attachment or schema migration is authorized by this slice. Chronological coalescing, indivisible atomic groups, parked successors after failure, workspace-specific receipts, bounded retention and exactly-once effect eligibility are required. Full compile-enforced mutation coverage, actual database worker integration, T13 seam review and measured performance acceptance remain mandatory later work.


## Expanded return matrix: passing test exposed a real layout failure

Root successfully built fixed checkpoint `d3a19cb8aa6229e7b8692e9c2a35187f3e14b7582625767712bc4c1c4ff57ef2` ([identity](evidence/return-matrix.sha256)) and independently ran four suites:

- [Research inputs](evidence/return-matrix-research-inputs.log): exit **0**, including the new long-title fixtures. However, root's [screenshot inspection](evidence/T21-long-history-selector-collapsed.png) finds the version selector collapsed to a clipped sliver above the long report. Logical child bounds and labels do not prove visible layout. T21 must reserve a usable selector viewport, constrain the body, verify real scrolling and inspect screenshots. Not accepted.
- [Archive](evidence/return-matrix-archived-research.log): exit **101** in the newly added ordinary-status check, expecting In progress · Paused where the existing presentation says Execution · Paused. Returned for authority-aligned fixture correction and complete responsive verification.
- [Navigation context](evidence/return-matrix-navigation-context.log): exit **101**, clicking period 1 after adding enough scrollable rows now targets an off-screen control. Require real scrolling and visible target activation.
- [Full automation](evidence/return-matrix-automation.log): exit **101** at the new precondition: initial results offset is **0**, before the drill-down. This narrows the previous scroll failure to an unproven fixture/input precondition; restoration correctness still needs exact before/after evidence.

All four actual process exits were captured, rather than inferred from a piped command. Failures were returned to the corresponding OpenCode sessions.


## S0 baseline provenance correction

Root inspected the final S0 logs and found different executable hashes across fixture modes. Those runs launched the shared Cargo output while other sessions rebuilt it; hashing that path can describe a replacement executable rather than the running image. Preserve the measurements as mixed-source concurrent observations, not a comparable single-build baseline. T14 must copy one successful build to an immutable path, record its hash before execution and run every baseline mode from that copy. The previously recorded root-owned fixed executable baseline remains valid. This correction does not require stopping other implementation sessions.


## T22 independent acceptance

T22 is accepted in the reviewed working-tree candidate, not committed or released. Root reviewed lifecycle-first history presentation, canonical ancestor archive/material guards, retained result access, counts, restore ownership and stop-confirmation coverage. Root independently built fixed executable `34bd1e9e68c28069bf2ca8d1d0d4613f5e1d91cf92f52247a44cdee176e9d47a` and ran archived-research QA: actual exit **0** ([log](evidence/T22-independent-final-archive-pass.log), [identity](evidence/T21-T22-final-layout-checkpoint.sha256)). Focused archive unit coverage independently passed **15 tests**, exit **0** ([log](evidence/T22-independent-15-unit-pass.log)).

The QA matrix covers light/dark at 1440×920 and 1000×680, List/Board/detail retained completion, restoration without reapproval, retained report content, ancestor archive and active-work confirmation Cancel/Confirm. Root inspected the corrected [narrow step status](evidence/T22-final-step-light-1000.png) and [archive Board](evidence/T22-final-board-dark-1440.png); the prior clipped status is resolved and Restore remains visible. Native system-prompt verification and full integrated release regression remain T19/T20 requirements. T13's later command/durability migration must preserve this accepted behavior.

At the same executable checkpoint, research-input QA also exits **0** ([log](evidence/T21-independent-long-history-pass.log)). The [long-title history selector](evidence/T21-long-history-readable-dark-1000.png) now keeps multiple readable attempts visible with run/time identity. T21 remains open for remaining scrolling/native checks and comment/material/directory/domain/scoped-Undo integration.


## Native archive confirmation and history keyboard review

Root refreshed and launched only the isolated Planning Native Review app, with [recorded binary identity](evidence/native-archive-history-checkpoint.sha256) and [source hashes](evidence/native-archive-history-source.json). Through Computer, root verified native archive Cancel and Stop and archive, retained read-only history, truthful failure when the fixture agent does not acknowledge interruption, and the resulting Archive destination. See [observed steps and limits](evidence/native-archive-history-review.txt).

Native history Escape did not dismiss the dialog; Close worked. This was returned to T21 for explicit modal focus, Escape, Tab/ShiftTab containment and focus restoration. Keyboard support remains in scope despite the excluded screen-reader audit. This native check does not substitute for the complete narrow-window/theme/release matrix.


## Remaining pure architecture and navigation review blockers

At checkpoint `34bd1e9e`, root also independently captured exit **101** for [navigation context](evidence/layout-return2-navigation-context-failure.log) and [full automation](evidence/layout-return2-automation-failure.log), both at initial nonzero-scroll preconditions. Inspection of pinned gpui-kit test support found that its scroll helper moves the pointer and validates a visible target before dispatching a wheel event; the custom T10 helper omitted that pointer move and used raw potentially clipped bounds. Root returned this concrete diagnosis and required visible-row input plus exact restoration checks.

T13's revised pure core adds semantic stamps and stale-preview checks, but root identified a missing positive case: a completed goal's title edit withdraws approval to Backlog; undo must restore the original valid Done state and reports, whereas the ordinary Backlog-to-Done stage guard rejects it. Require a genuine positive regression and atomic validated restoration. A narrow shared domain stage-validation extraction is authorized to eliminate the duplicated lifecycle validator; forged editor execution/child-stage transitions must remain rejected.

T14's initial pure coordinator retains every released generation in an unbounded vector even though its durable watermark can deduplicate receipts. Root returned that growth, queue retention accounting, oversized atomic groups/backpressure, checked payload construction and exact-state fake-sink coverage. Metadata/topics/requests/config coverage remains mandatory before live integration. Pure tests do not establish SQLite atomicity or actual background-worker durability.


## T13 S3a independent pure-foundation acceptance

Root reviewed the final scoped edit and shared stage-validation implementation, including the positive completed-goal inverse, field/approval rollback, current-context/runtime conflicts and rejection of forged editor stage intents. Independent `cargo test --quiet scoped_edit` exited **0**, with **26 passed** ([evidence](evidence/T13-independent-pure-scoped-edit-pass.log)). This accepts only the pure S3a foundation. No live editor migration, command durability integration or complete T13 acceptance is claimed.

The live boundary must still supply pending-batch context, preserve the draft through impact/durability failures, apply against current state and carry exact scoped changes into T14. The existing T21 preview's transitive whole-Store clone and bounded historical output stamp remain explicit later architecture/performance work. T10's full automation gate remains open; root returned silent fixture insertion failures and visible-target scroll assertions for correction.

Root additionally reran the complete unit suite with the documented Node 24 PATH: **564 passed, 0 failed, 2 ignored**, actual exit **0** ([log](evidence/T13-S3a-current-full-unit-pass.log)). The preceding default-PATH run failed only the publishing dependency check because Node was unavailable; that environmental failure was corrected before this pass. Full UI/native gates remain open.


## T14 retention bound and T21 persistent-scroll returns

Root inspected the final S1a journal and returned a concrete retention-bound defect: string-length sums omit vector element allocations and spare String/Vec capacity. For example, 100,000 empty labels retain roughly 2.4 MB of String slots while adding zero text bytes to the estimate. Require allocation-aware conservative accounting, saturating arithmetic and adversarial empty/spare-capacity/nested-payload tests, with whole rejected groups preserved. The same OpenCode session resumed with `T14-S1a-memory-return.txt`; pure S1a is not accepted. The attempted independent journal test exited **101** during a concurrent T21 unsupported scroll-method edit, before tests ran; that compile error was returned to T21.

T21's latest long-history test observed a negative offset immediately after wheel input but zero after subsequent rendering. Root rejected transient offset movement as proof of usable scrolling: require stable nonzero offset over multiple frames, positive scroll extent and a screenshot showing later paragraphs. Native modal keyboard verification remains required.


## T13 live editor integration authorized

Root authorized narrow S3b after independent S3a acceptance, while the separate T10 scroll regression remains open. The original OpenCode session now owns task-editor save, scoped Undo dispatch and impact-confirmation integration; T10/T21 were notified of file-section ownership. Require current-state application, actual pending-batch validation, draft retention on failure, durable-only success/navigation, exact scoped inverses and safe coexistence with legacy Undo. T04 safeguards remain until every whole-Store path is migrated. This sequencing adjustment does not accept full S2/T04/T13 or waive the remaining S3c–g work.

Root also found that the failing automation QA selected a materialized period across all automation definitions, then filtered the visible list to one definition. A target outside that definition can never become visible regardless of scrolling. Returned this specific fixture mismatch and required target membership/count assertions before the unchanged offset-restoration checks.


## Independent scroll checkpoint 3

Root built successfully and copied immutable executable `1b6c2ed4a89782efa28f58b640799e622b4e6f6dcfb3806a8b9c69eeddd28999`. Navigation-context QA exited **0** ([log](evidence/scroll-checkpoint3-navigation.log)). Research-input QA also exited **0**, but all four long-history logs show zero scroll before and after rendering ([log](evidence/scroll-checkpoint3-inputs.log)); this interim passing test does not prove scrolling. Root inspected the [narrow screenshot](evidence/T21-scroll-still-zero-light-1000.png): the reader still shows paragraphs 0–1 after wheel input. Returned for stable offset, positive extent and later-paragraph visual evidence. Full automation QA on the same immutable executable is in progress. This checkpoint does not accept full T10 or T21.

Full automation on checkpoint 3 completed with actual exit **101**: expected an opened period `Some(29)`, observed `None` after activation ([log](evidence/scroll-checkpoint3-automation.log)). Earlier Undo protection and queue/planning regressions passed. Returned the remaining activation failure to T10; full gate remains open.


## T14 allocation accounting follow-up

Root found that the revised metadata loops replaced the cumulative estimate with each item's size, dropping container capacity and all preceding items. Returned saturating accumulation and multi-item regressions. After correction, independent journal tests exited **101**, with **19 passed, 1 failed** ([log](evidence/T14-independent-capacity-intermediate.log)). The remaining nested-capacity test submitted generation 2 to a fresh coordinator expecting generation 1, so it never reached its intended overflow check. Require valid fixture sequencing without weakening production generation validation. Pure S1a remains under review.


## S3b retry review: preserve original edit ownership

Root reviewed the initial live adapter before completion and returned unconditional retry re-preparation from the retained full editor payload. That would turn previously unchanged fields into new edits after another path changed them, or silently overwrite a newer value of an originally edited field. Require original changed-field/read-set guards: preserve unrelated updates, conflict on newer owned-field changes and retain the draft. Pending-batch validation must also run at commit/retry, not only initial preparation. A new independent journal test attempt was interrupted by this live adapter's not-yet-wired fields/methods; no journal acceptance is inferred from a compile failure.


## S3b mixed Undo compatibility review

Root returned the initial mixed-stack implementation's blanket `undo.clear()` on automation changes/run dispatch and its global active-run Undo gate. Those legacy safeguards would erase or block safe scoped task edits after unrelated work. Require selective invalidation of legacy records, preserved chronological ordering and scoped field/goal-runtime conflict checks for typed entries; retain protection against whole-Store restoration. Regression cases must include unrelated active runs, unrelated automation mutation, relevant-goal conflicts and mixed legacy/scoped history.


## T10 incomplete handoff returned

The S5/S6 session ended with the full automation activation failure and the remaining search/dirty-route/theme matrix unfinished. Root resumed the same session with `T10-S6-activation-return.txt`. The handoff's assertion that activation is only a QA targeting issue is not yet proven: require stable clipped bounds/scroll measurements across render and pointer movement, actual control activation and before/after screenshots. No direct state injection or weaker restoration checks may substitute for the user action. All remaining S6 cases remain mandatory.


## T14 enqueue-growth boundary returned

Root found another bounded-retention inconsistency: `retained_bytes` accounts for deque capacity, but admission checks that capacity before `push_back` can grow it. A fresh coordinator whose byte budget equals the first group's estimate accepts that group, then reports retention above its budget. Require consistent accounting for anticipated container growth and post-admission `retained_bytes <= max_pending_bytes` tests at exact boundaries, existing-capacity growth, blocked/awaiting states and drained queues. Atomic groups must remain indivisible and rejected payloads retained. The latest independent test attempt exited **101** before tests because T21's experimental `max_lines(Some(...))` call did not match the pinned API; no journal pass is claimed.


## Independent journal pass and confirmation-path ownership gap

Root reran journal tests on a compilable shared candidate: actual exit **0**, **20 passed** ([log](evidence/T14-independent-capacity-20-pass.log)). Allocation estimates now include empty/spare-capacity collections and preserve preceding metadata contributions. This does not accept S1a yet: the separately returned deque-growth admission boundary still lacks a correction and test.

Root found the S3b confirmation button still re-prepares the entire old draft, ignoring its retained prepared plan. Thus the dedicated retry helper's field-conflict protection can be bypassed by clicking Save changes again after a failed write or intervening update. Require original scoped ownership across the real confirmation/save-again paths, with UI regressions for unrelated-field preservation, owned-field conflict and changed-impact review.

Root also ran the complete current unit suite with Node 24 PATH: actual exit **0**, **568 passed, 0 failed, 2 ignored** ([log](evidence/S3b-intermediate-full-unit-pass.log)). This intermediate unit pass does not cover the newly returned UI confirmation ownership or journal admission-growth cases, and does not accept the live S3b integration.


## T14 consistent payload budget independently verified

Root reviewed the corrected accounting contract: byte admission covers owned pending payload allocations; coordinator bookkeeping is separately bounded by pending-group count and is not represented as a total-RSS budget. Awaiting acknowledgement retains only effect payload after the committed group is released. Independent tests exited **0**, **21 passed**, including exact-budget admission, queue growth/drain and blocked/awaiting cases ([log](evidence/T14-independent-boundary-21-pass.log)). A subsequently added explicit atomic-overflow test is still awaiting independent execution: the rerun failed to compile during an in-progress S3b helper call (`same_editor_payload`). The pure foundation is close to acceptance but no full T14 completion is claimed.


## T14 S1a pure foundation independently accepted

Root accepts the reviewed pure journal/coordinator foundation after code review and independent exit **0**, **21 tests passed** ([final run](evidence/T14-independent-pure-journal-final-pass.log), [test inventory](evidence/T14-independent-pure-journal-test-list.log), [source identity](evidence/T14-pure-journal-source.sha256)). The inventory explicitly includes atomic two-payload rejection and exact-budget/growth/drain cases; earlier notes anticipating 22 tests were a counting estimate, not the actual inventory. Reviewed coverage includes typed payload/metadata, chronological changes, parked failures and ordered retries, wrong-workspace/out-of-order/duplicate receipts, owned-allocation budgeting and preserved rejected intents.

Acceptance is limited to S1a. The payload-byte budget excludes coordinator bookkeeping (bounded by group count), allocator overhead and total process RSS. Fake sinks do not establish SQLite durability. Compile-enforced mutation tracking, the live worker/ack boundary, real transaction/crash checks, incremental writes, indexed/lazy history reads and measured UI performance remain required for full T14/T15.


## Scoped rebase impact must match effective field delta

Root returned `rebase_task_edit` using the retained full form as its impact-preview input while applying only the original changed fields. Example: a label-only edit followed by a newer title would preview restoring the old title, although the actual command changes only labels. Require effective preview intent built from current untouched values plus owned deltas, reusing T21 signature authority. Tests must cover metadata-only edit/newer title and title edit/newer description, with owned-field conflicts preserved.


## T14/T15 isolated SQLite backend authorized

After S1a acceptance, root authorized the same OpenCode owner to implement a real, isolated SQLite incremental-transaction/migration backend using the accepted typed groups. This deliberate sequencing adjustment allows backend work while T13's live command migration proceeds. Scope includes disposable-database tests, atomic generation receipts, verified backups and migration round trips, portable historical path/signature preservation, indexed queries and lazy-body evidence. The v4 snapshot design must include an explicit snapshot-reference hash. No live application storage activation, production data migration or worker attachment is authorized in this slice.

The dispatch is `T14-T15-sqlite-backend-authorization.txt`. Compile-enforced mutation tracking, complete scoped Undo/command migration, actual UI query integration, real worker acknowledgements and final crash/performance gates remain mandatory. Backend unit success alone cannot accept full T14/T15.


## T21 scrollable-reader checkpoint still rejected

Root inspected the latest owner-produced passing QA [narrow screenshot](evidence/T21-scrollable-reader-still-start-footer-overflow.png). After wheel input it still shows paragraphs 0–1, and Copy/Export extend below the dialog border. The revised test dispatches scroll but no longer asserts a stable internal scroll position, so its green exit cannot establish usability. Returned supported TextView/ListState measurement, a later-paragraph screenshot and constrained footer/layout bounds for all four size/theme combinations. Root also required successful immutable-build provenance after observing an owner command run QA despite a failed build.

The S1a owner finished its session before executing the next backend slice. Root confirmed termination and explicitly resumed the same session with `T14-T15-sqlite-backend-authorization.txt`; execution log is `T14-T15-sqlite-backend.jsonl`.


## Effective scoped intent independently verified

The shared candidate is compiling again: root `cargo check --features qa` and QA build both exited **0**. Root reviewed effective preview intent construction from current untouched fields plus owned deltas, and independently ran scoped-edit tests: **28 passed**, exit **0** ([log](evidence/T13-independent-effective-intent-28-pass.log)). This verifies the returned metadata/newer-title and title/newer-description preview cases at the pure boundary.

Root also copied fixed executable `a55ed408f0bafcd9b0610e2a7a65b4c9c9120f688ec7f340fbaa00a0a9d0acb6` and started `--qa-task-edit`. The owner was required to add database reopen/exactly-once checks and actual Save/Undo control coverage, retaining artifacts rather than deleting completed scenarios. No live editor acceptance is claimed yet.

The fixed-binary task-editor QA exited **101** at its first Undo-count assertion ([log](evidence/T13-live-editor-first-undo-count-failure.log)): the scenario saves metadata then title, so two chronological entries are expected, but the assertion expected one. Returned precise increment/order validation and two-step inverse checks; do not clear history merely to satisfy the fixture.


## Isolated SQLite backend early transaction review

Root returned two concrete issues in the initial backend: opening a missing/unsupported/wrong-identity database must not create a file or switch journal mode before read-only validation; and receipt/generation admission must be checked inside the write transaction, not before `BEGIN IMMEDIATE`. Require untouched-file rejection evidence and two-connection generation/retry tests. Backend implementation remains in progress and has no acceptance.

The T21 session ended after only restoring compilation; root confirmed termination and resumed the owned-reader implementation explicitly with `T21-owned-reader-return.txt`. Stable internal scroll evidence and full modal layout/keyboard behavior remain required.


## SQLite read consistency and pagination review

Root returned snapshot consistency for `load_store` and page/count/unread reads: separate queries without a read transaction can mix committed generations. Root also identified that filtering current rows by `activity_seq <= watermark` silently removes an unseen older row if it is retried after page one. Require an explicit stable-snapshot/versioned-row contract or a detected stale-page/restart result, with concurrent retry/insert/tied-time tests; a simple generation filter does not prove stable pagination. Integer parameter conversion must reject out-of-range values before writes instead of silently wrapping `u64` into SQLite integers.

## Independent live editor checkpoint r4

Root observed a successful QA build and copied the executable to an immutable checkpoint before execution. `--qa-task-edit` exited 0; retained fixtures are `.local/task-edit-qa/76616`. Evidence: [QA output](evidence/T13-independent-live-editor-r4.log) and [binary identity](evidence/T13-independent-live-editor-r4.sha256). Coverage includes real footer Undo twice, metadata/title chronological preservation, disk reopen, combined stage edits, retry conflicts, changed impact, pending batch rejection and unrelated work boundaries. The initial goal label is `Research`; restoring it on the second Undo is correct, rather than asserting an empty label list. This is a passing narrow S3b checkpoint, not full T13 or release acceptance. Native verification and the remaining domain command migration are pending.

The owned historical reader still failed its positive scroll-extent assertion in implementation QA; T21 remains unaccepted. SQLite read transactions and explicit invalidated-page responses are now implemented but still require independent backend verification.

## SQLite backend first independent test and migration review

`cargo test --quiet sqlite_v4` exited 101: 8 passed and 2 failed. Pagination retry/insert coverage rejected a reused generation with IntentConflict; relocation coverage rejected an invalid workspace path. [Independent output](evidence/T15-independent-first-backend-tests.log). Both were returned to the implementer for diagnosis without weakening behavioral assertions.

Review also returned the unconditional schema-v4 AlreadyCurrent shortcut (identity is unchecked), original-file WAL checkpointing before a verified backup, and missing enforced exclusive ownership across reading and replacing the migration source. Required evidence includes foreign-v4 rejection, committed-WAL preservation on injected migration failures, and preventing concurrent writes from being silently lost. The isolated backend is not accepted and live migration remains unauthorized for this slice.

## Owned history reader visual progress

Root inspected the implementer screenshot from fixture 77114 at light/1000: paragraphs 7 and 8 are now visible after scrolling. The updated QA checks parsed blocks, positive extent, movement and stability across subsequent frames. [Reviewed screenshot](evidence/T21-owned-reader-scroll-footer-review.png). This resolves the earlier visual evidence of unchanged paragraphs 0/1, but the action buttons touch/slightly exceed the bottom panel boundary. Consistent bottom inset and fully contained action bounds were returned for correction, alongside the remaining keyboard and independent/native gates. No full T21 acceptance is implied.

## Backend r2 and next scoped-command slice

Root independently ran `cargo test --quiet sqlite_v4`: 15 passed, exit 0. [Output](evidence/T15-independent-backend-r2.log). Migration now takes the cooperative workspace lock and validates current-schema identity. Acceptance remains pending: the WAL preservation fixture closes the last database connection before inspecting WAL and conditionally skips WAL assertions. Root requested a guaranteed nonempty committed WAL, unconditional byte-preservation checks, latest-state backup reopen, and v3 migration fault/success coverage for source sidecars. Green tests do not yet establish these requirements.

After the independently passing S3b editor checkpoint, S3c was dispatched to the same OpenCode session: direct metadata/stage changes, comments and step reopen through typed scoped commands and durable conditional Undo. Scope and ownership are recorded in `.local/opencode-dispatch/T13-S3c-authorization.txt`. Materials, settings, context/archive and final snapshot removal remain required later slices; native and full release gates remain open.

## Indexed pagination review: seek is not yet demonstrated

Root reproduced the current optional-filter SQL using SQLite EXPLAIN on the same occurrence schema/index definitions. Both the unfiltered first page and a filtered deep cursor produced `SCAN automation_occurrences USING INDEX idx_occ_activity`. Replacing the optional OR predicates with explicit automation equality and tuple cursor comparison produced `SEARCH automation_occurrences USING INDEX idx_occ_auto_activity`. The existing assertion only checks for an index name and therefore accepts a full index scan; its row metric counts returned rows, not visited rows.

Returned for query variants and pinned-rusqlite query-plan evidence, meaningful deep-page/rare-automation verification at 43,200 periods, and indexed count variants. Ordering, tie handling, invalidated cursors and lazy report bodies must remain intact. This local SQLite reproduction is diagnostic evidence, not acceptance of the pinned backend.

## Integrated r5 independent QA checkpoint

Root observed successful `cargo build --features qa --quiet`, copied an immutable binary (SHA-256 `55961552f7fb90fc8261ed8b57b1f8338213cd8d2a854f8c76f9f4170f0a2360`), and independently ran both suites. `--qa-research-inputs` exited 0 (fixture 79196), and the full `--qa-automation` exited 0 (fixture 79211). Evidence: [inputs](evidence/integrated-r5-inputs.log), [automation](evidence/integrated-r5-automation.log), [binary](evidence/integrated-r5-binary.sha256). This supersedes the earlier automation activation failure for this checkpoint. T10 still needs its full S6 matrix and source review; T04 final review is not silently inferred from one green log.

Root visually inspected the light/1000 and dark/1440 long-report screenshots: paragraphs 7/8 are visible and the footer has bottom inset. [Light](evidence/T21-r5-scroll-Light-1000.png), [dark](evidence/T21-r5-scroll-Dark-1440.png). Positive stable scrolling assertions and a limited Tab/Shift-Tab/Escape scenario passed. Complete modal focus containment and native verification remain pending; the current one-Tab assertions alone do not prove a full focus cycle stays inside the modal.

During concurrent backend review, root found `sqlite_v4.rs` truncated after `count_on`, removing migration code and tests, and immediately returned this regression for restoration. Earlier 15-test evidence does not apply to that truncated source.

## S3c inverse review and native checkpoint preparation

Root reviewed the in-progress comment inverse and returned a candidate-order defect: restoring approval/stage before removing the posted comment evaluates the old approval against a signature that still includes that comment. The requested correction stages comment removal atomically with the inverse, validates recorded after guards first, and rolls back all touched values on failure. Required tests cover completed/approved research restoration and unchanged state on newer-input/runtime conflict. This is review of an unfinished slice, not an accepted implementation.

An isolated native source snapshot was prepared at `.local/native-review/source` with hashes in `.local/native-review/source-checkpoint-r6.json`; only the package identity and isolated workspace bootstrap differ from the copied source. Its build is pending in `.local/opencode-dispatch/root-native-r6-build.log`. The production app and production bootstrap were not changed or restarted.

## Native r6 history modal: keyboard acceptance failed

The isolated native build completed successfully and was launched with Computer. Archive -> archived goal TASK-G1 -> Version history displayed the saved report, sources and contained footer. Escape did not close the modal, including after Tab and after clicking its reader body. The explicit Close button did close it and preserved the archived goal destination. This contradicts the headless keyboard pass as evidence for native Escape behavior; T21/T19 keyboard acceptance remains open. The exact reproduction was returned to OpenCode. [Native record](evidence/native-r6-history-keyboard.txt), [signed binary identity](evidence/native-r6-binary.sha256), [source manifest](evidence/native-r6-source.json). Production data/app were not touched. The isolated app is left on archived goal TASK-G1 with the modal closed.

## S3c first independent domain checkpoint

`cargo test --quiet scoped_edit` passed 32 tests, exit 0. [Output](evidence/T13-independent-s3c-first.log). Root verified that the comment inverse now removes the comment in its candidate after validating recorded companions and reinserts it on failure. The completed-goal restoration test passes. Runtime mutation and post-removal rollback coverage remain requested: the currently runtime-named test changes only comment body/title. Root also returned the unrestricted Reopen stage-intent bypass for explicit domain validation. Live UI/durability tests and full S3c acceptance remain pending.

## Backend test deletion recovery aid

Root observed that the implementer subsequently ran the backend test filter with **0 tests**, matching the missing module. This is not a passing backend gate. Root extracted 960 lines of original migration/test write payloads from the OpenCode execution history into `.local/opencode-dispatch/T14-original-tail-recovery.rs` and delivered that recovery aid to the same active session. It is an original baseline, not the final corrected implementation: the owner must replay identity, locking, WAL, numeric-range and concurrency fixes and preserve the newer query variants. Root did not overwrite the actively edited source. Acceptance requires a nonzero inspected test inventory and independent rerun.

## Restored backend checkpoint and crash-boundary review

Root confirmed restoration of the migration/test tail and independently reran `cargo test --quiet sqlite_v4`: **15 passed**, exit 0. [Output](evidence/T15-independent-restored-tests.log). A source safety copy is retained at `.local/opencode-dispatch/backend-restored-checkpoint/sqlite_v4.rs`. Query variants now separate filtered and cursor cases, but large-history seek evidence remains pending.

Review returned a migration crash window: moving the old WAL/SHM away before replacing its main file can expose the old main without committed WAL if the process exits between operations. Rename-error rollback alone does not cover process failure. Required correction is a durable recovery protocol or a verified-backup-first, exclusive, checked checkpoint that leaves the source self-contained before installation, with nonempty-WAL failure-boundary tests. The two current WAL backup fixtures still close the last connection before backup and do not prove that condition. Isolated backend acceptance remains pending.

The subsequent full node-enabled unit run covered new WAL tests added during review: 591 passed, 3 failed, 2 ignored, exit 101. [Output](evidence/S3c-full-unit-wal-fixture-failures.log). Two fixtures incorrectly assume a v3 `tasks.id` column; another reads an uncheckpointed seed from the immutable main and gets no row. Returned for actual-schema targeting and explicit baseline-versus-WAL setup, preserving the substantive assertions. These failures supersede the earlier 15-test pass for current integration readiness.

## Comment retry review: changed impact and draft ownership

Root returned two S3c retry defects in the in-progress live integration. `retry_comment_save` rebuilds a plan from current state and commits without comparing the retained reviewed impact, so a changed completed/reviewed scope can be invalidated without renewed confirmation. `commit_prepared_comment` unconditionally clears the shared comment input, potentially discarding a newer draft entered after the failed save or a different task's draft.

Required correction: retain the original comment intent, require a fresh impact confirmation when relevant effects change, keep disk unchanged until confirmation, and clear only the matching saved draft for the originating task. Real footer Retry QA must prove exactly one comment/activity/undo entry and preserve newer drafts and navigation. The new integrated QA build is being prepared in `.local/opencode-dispatch/root-integrated-r7-build.log`; no acceptance is inferred from an unfinished build.

## Integrated r7 independent control QA

Build exited 0; root copied binary `d9e6acc5f785dcdc92b0647941dc0074761ac789ea2763c70675ed52d44aec6b` before QA. `--qa-task-edit` exited 101 because the new comment scenario could not find `post-comment`; returned for real route/scroll/control correction without bypassing the UI click. [Failure](evidence/T13-r7-live-comment-failure.log). `--qa-research-inputs` exited 0, fixture 85167. [Output](evidence/T21-r7-inputs-pass.log). Native Escape remains a separate unproven gate.

## WAL r3 independent verification

Root reran the restored backend suite: **18 passed**, exit 0, including nonempty committed WAL backup, migration fault boundaries and the 43,200-period deep-page test. [Output](evidence/T15-independent-wal-r3.log). Source now checkpoints only after a verified backup, checks a busy result before replacement, and verifies logical latest state at later fault boundaries. The WAL fixture explicitly establishes a checkpointed baseline, keeps the source connection open and proves the latest title is WAL-only before backup.

The deep-page SEARCH assertion is meaningful. Root returned one remaining fixture weakness: the supposed rare-automation population clones the target automation for nearly every row, so it does not prove low-selectivity filtering. Required correction uses mostly other-automation rows with an exact small target cohort, filtered deep-cursor evidence, and a statement work counter beyond FullscanStep alone. This is not full T15/live-worker acceptance.

## Comment draft ownership follow-up and native r8 preparation

Root reviewed the changed-impact retry correction and matching live-draft clearing. One cross-task case remains: if the retry succeeds while another task is selected, the originating task's matching cached comment is not cleared and reappears as unsent on return. Returned for conditional clearing of the origin cache independently from the selected task, while preserving newer cache values and the other task's live draft; an actual cross-task retry/return regression is required.

A new isolated native snapshot was prepared with `.local/native-review/source-checkpoint-r8.json` for the modal-local Escape handler. Build is running in `.local/opencode-dispatch/root-native-r8-build.log`; the existing isolated app remains on r6 until a successful build is observed. Production app/data remain untouched.

Both pending commands subsequently exited 0: the isolated native r8 build is ready (immutable executable `.local/native-review/planning-native-r8`), and the focused scoped-edit rerun passed. [Focused output](evidence/T13-independent-s3c-r2.log). Native keyboard acceptance still requires the actual launch/recheck.

## Native r8: Escape correction verified

Root installed the successfully built isolated r8 snapshot and verified with Computer: Archive -> TASK-G1 -> Version history opens; Escape closes the modal, including after Tab, and preserves the archived goal destination. This resolves the r6 native Escape failure for this snapshot. [Native record](evidence/native-r8-escape-review.txt), [signed binary](evidence/native-r8-binary.sha256), [source](evidence/native-r8-source.json).

Full native Tab-cycle/activation remains unproven: the platform tree reports the window as focused, and Tab followed by Return did not close the modal. No assertion is made that Close was focused. T19 and the remaining T21 mutation integration are still open. The isolated app is left at archived TASK-G1 with the modal closed; production app/data were untouched.

## T19 implementation dispatched

The T21 reader session reached a terminal handoff, so root resumed that same OpenCode session with `.local/opencode-dispatch/T19-keyboard-visual-authorization.txt` to implement T19 ordinary keyboard and visual readability across dialogs/forms/actions. Active log: `.local/opencode-dispatch/T19-keyboard-visual.jsonl`. Navigation and pending-save ownership remain coordinated with T10/T13. VoiceOver/screen-reader scope is excluded. This is implementation authorization, not acceptance; root retains final native verification. The isolated backend also reached handoff and is undergoing its final focused independent check before live-worker scope can be dispatched.

## Isolated SQLite foundation accepted; real worker dispatched

Root independently reran the final isolated backend suite: **18 passed**, exit 0. The source hash matches the reviewed handoff (`9a1a61c5c1e8a7293ad067f07373907b43034280d0e987fea45e141c1d9dfde7`). The 43,200-period fixture now has exactly 11 target-automation rows, tests a filtered deep cursor using SEARCH and the owner/activity index, and bounds its page VM work separately from counts. WAL backup and migration corrections reviewed above remain present. [Final isolated output](evidence/T14-T15-independent-isolated-backend-final.log), [source identity](evidence/T14-T15-isolated-backend-source.sha256). This accepts the isolated foundation only, not full T14/T15 or live schema activation.

The prior backend process reached a terminal handoff. Root resumed its same OpenCode session with `.local/opencode-dispatch/T14-worker-authorization.txt`; log `.local/opencode-dispatch/T14-worker.jsonl`. Scope is a real single ordered SQLite worker with nonblocking bounded submission, failure parking/retry, durable-generation reopen, correct acknowledgement/effect sequencing and real subprocess crash/reopen tests. Compile-enforced Store mutation tracking, live attachment/admission, lazy report consumers and controlled end-to-end performance remain explicit future integration gates. No production migration/restart was performed.

## Comment retry QA path diagnosis

Root traced the new comment retry failure to an impossible interaction in its fixture: a failed impact-confirmed save keeps the modal open, so clicking the footer Retry behind its scrim does not dispatch Retry. The next visible confirmation correctly refreshes the changed impact and therefore does not yet write. Returned for two distinct real paths: retry through the retained modal with visible changed-impact review, and a footer retry starting from a save that legitimately has no impact modal before context changes. Modal input blocking must not be weakened merely to pass the test. Existing draft/cache and exactly-once acceptance requirements remain.

The expanded navigation QA now contains additional dirty project/shortcut/comment/composer cases and size/theme passes. Root started a fresh integrated QA build at `.local/opencode-dispatch/root-navigation-r9-build.log` before independent execution; no source-only acceptance is inferred.

## Navigation r9: distinguish blocked background routes

Fresh QA build exited 0. Root ran immutable binary `0c924b59416917ce3f75c8e9c94d8ae290da967d4138499c48574bf01b438498`: navigation-context QA exited 101 at the dirty editor Back assertion. [Output](evidence/T10-r9-modal-background-route.log), [identity](evidence/T10-r9-binary.sha256).

Source diagnosis: the task editor is an absolute full-window occluding modal. Its background Back/sidebar controls cannot receive mouse clicks; unchanged draft and route are correct protection, not lost navigation. Returned for explicit blocked-background assertions and separate reachable Cancel/Escape/shortcut dirty-decision cases. Nonmodal wizard/planning routes still require actual Back/sidebar/breadcrumb/recent coverage. No modal occlusion should be weakened and no route may be silently omitted from the matrix.

## Real worker first code review: changes required

Root inspected the newly implemented real thread/channel worker and returned these blockers before any live activation:

- `shutdown` sends through the bounded submission queue and joins before draining the bounded event queue. A worker blocked sending an event can therefore deadlock shutdown/Drop. Require a nonblocking shutdown state machine and a full-channel regression with bounded test completion.
- The coordinator byte limit excludes request-channel groups, the delayed/inflight group and rejected groups held by the event channel. Require admission reservations covering all owned payload retention and immediate oversized-group rejection with ownership returned.
- Startup blocks up to ten seconds waiting for an event, and polling drains until empty with no per-poll limit. Require UI-safe asynchronous startup and bounded polling work.
- Shutdown reports only a blocked generation while accepted queued groups can be dropped. Require explicit drain/return semantics for every accepted group; no silent loss.

[Reviewed source identity](evidence/T14-worker-first-review.sha256); local source safety copy `.local/opencode-dispatch/worker-first-review/persistence_worker.rs`. These are source-derived defects in an unfinished implementation, not a test-pass or live-app claim. The worker remains unaccepted and is not attached to production startup.

## Worker fixture review: generation and scheduling assumptions

Root inspected the first worker test failures and found two invalid fixture assumptions: `task_group` creates a new journal on every call, producing generation 1 repeatedly while tests wait for generations 2/3; and the capacity-one queue test assumes the worker consumes the first submission before the immediate second submission. Returned for one persistent journal per fixture, durable-generation bootstrap on reopen, and explicit synchronization to establish inflight/queued occupancy. These fixture repairs do not waive the previously returned shutdown deadlock, global admission accounting or accepted-group preservation requirements.

A fresh integrated QA build is running at `.local/opencode-dispatch/root-integrated-r10-build.log` before another independent navigation/comment check.

## Integrated r10 independent QA failures

Build exited 0 and root copied immutable binary `c524e3c0a877ff2b343e916ac0e4a3fa95d05fd5ac9ced7500af5a903556a31e`. Navigation QA exited 101 on missing `navigation-back` in a planning-form state; task-edit QA exited 101 on missing `step-reopen`. [Navigation](evidence/r10-navigation-failure.log), [task edit](evidence/r10-task-edit-failure.log). Returned separately for actual route/status/scroll prerequisite diagnosis without replacing UI controls with direct mutation calls. No new task acceptance.

## Independent r12 task-edit checkpoint and worker return

- Fresh QA build exited 0. Immutable binary SHA-256: `aa7348d0d8f697d4491b3154890a6a468125d3b65570274137aa96dae120f084`. Independent `--qa-task-edit` exited 0, fixture `.local/task-edit-qa/93707`; [log](evidence/T13-r12-task-edit-pass.log). This verifies the focused S3c UI regression checkpoint, not full T13 or native release acceptance. S3d material command/ownership/cleanup implementation is now dispatched.
- T14 worker remains unaccepted: current shutdown still sleeps/spins up to 10 seconds, Drop up to 200 ms, and shutdown drains can discard Durable receipts/effects and rejected ownership. Returned with requirements for nonblocking request/poll/completion and exact observable outcome preservation.
- T19 reader slice leaves ordinary dialogs and empty/error/disabled states uncovered. Same OpenCode session resumed with the full remaining keyboard/visual matrix; native Computer acceptance remains independent and pending.

## Independent r12 navigation checkpoint and retained-receipt review

- The same immutable r12 QA binary independently passed `--qa-navigation-context` with exit 0; [log](evidence/T10-r12-navigation-pass.log). This supersedes r10 missing-control failures for the exercised routes. Full S6 remains open: the inspected matrix substitutes pure checks for description/due/cycle controls and repeats only a subset at other sizes/themes. Remaining real-control coverage was returned to its owner.
- T14 has an additional unaccepted admission gap: releasing a reservation before enqueueing a Durable receipt leaves the receipt effects allocation outside the shared payload budget. Requested reservation transfer or precisely accounted receipt ownership plus full-event-channel tests. Eight implementer worker tests do not establish this invariant.

## Independent r12 command and research-input regressions

- Immutable r12 binary independently passed `--qa-command` (fixture `.local/command-qa/94948`) and `--qa-research-inputs` (fixture `.local/research-input-qa/94946`), both exit 0. Evidence: [command](evidence/r12-command-pass.log), [research inputs](evidence/r12-inputs-pass.log). Reviewed the generated dark 1000×680 impact screenshot: decision text and both footer actions fit, focused Cancel is visible, and the underlying editor is dimmed. This is screenshot/headless evidence, not native keyboard completion.
- Returned new T14 crash tests: `fork_wait` executes allocation/SQLite in a forked multithreaded test process, including inherited WAL connection state. Requested an independently launched child helper with bounded supervision and explicit checkpoint/termination evidence. The after-commit case currently drops the database normally before `_exit(0)`, so it does not prove abrupt post-COMMIT/pre-ack recovery. Mid-group and migration cases must distinguish intended crash checkpoints from unrelated child failures. No crash acceptance is claimed.

## Independent r12 full automation and material review

- Full `--qa-automation` on immutable r12 exited 0, fixture `.local/automation-qa/95212`; [log](evidence/r12-automation-pass.log). Includes Undo protection, independent concurrency pools, draft defaults and planning revision scenarios.
- Started the frozen baseline diagnostic sweep on the same immutable binary, with per-entry exits in `.local/opencode-dispatch/root-r12-baseline-gate/index.json`. Controlled performance gates are explicitly deferred, not marked passed under concurrent builds.
- Returned preliminary S3d implementation defects: fallible file identity reads occur after Store mutation; Add inverse deletes files before durable save; imported path/size alone does not prove the approved content digest. Required prevalidation/exact rollback, deferred post-durability cleanup with current ownership checks, and same-size replacement conflict tests. Material implementation remains unaccepted.

The r12 baseline sweep additionally exposed `--qa-topics` (Back returns All instead of Topic(2), topic_qa.rs:261) and `--qa-topic-actions` (unfinished goal click leaves selection None, line 724), both exit 101. Returned to T10 for route/frame diagnosis while preserving destination assertions. Evidence: [topics](evidence/r12-qa-topics-failure.log), [topic actions](evidence/r12-qa-topic-actions-failure.log). Sweep remains running; no full baseline acceptance.

## T01 baseline fixture return from r12 sweep

- Independently reproduced `--qa-simplify` exit 101 at simplify_qa.rs:441: the fixture submits a research Goal directly as an executable step. Current eligibility correctly refuses this. Requested an approved executable child with the same real-process cancellation and historical-selection assertions.
- Independently reproduced `--qa-batch` exit 101 at batch_qa.rs:371: a newly created unplanned/unapproved Goal is expected to gain three children from ordinary queue submission. Current product requires explicit planning and approval. Requested a current-contract fixture retaining pause/admission and no-background-navigation/no-duplicate-work guarantees; production eligibility must not be relaxed.
- Resumed original T01 OpenCode session `ses_e455e97b5ffeuzzaxHfI7s3RO1` with ownership limited to those two QA files; T10 retains topic/navigation ownership. Evidence: [simplify](evidence/r12-qa-simplify-failure.log), [batch](evidence/r12-qa-batch-failure.log). These are diagnosis and dispatched work, not accepted repairs.

The sweep also reproduced `--qa-research` exit 101 at research_qa.rs:385 after posting a comment. The fixture immediately expects invalidation without completing the new impact review. Returned to T13 to verify this cause and adapt the real-control flow with pre-confirmation nonmutation, durable invalidation and retained-history assertions. [Failure log](evidence/r12-qa-research-failure.log).

The r12 baseline diagnostic sweep completed: 30 executed entries yielded 25 passes and 5 failures; the separately executed full automation passed, bringing functional entries to 26 passed / 5 failed. Two performance entries remain deferred to a controlled run. [Diagnostic index](evidence/r12-baseline-index.json). Original baseline evidence is unchanged; T01 and final release gates remain open.

## Added-entry r12 regression sweep

Started seven additional existing QA entries on immutable r12 beyond the frozen original baseline. `--qa-global-search` independently fails at global_search_qa.rs:944: after Stay preserves Search and task draft, a second Enter fails to reopen the dirty decision. Returned to T10 for keyboard/opener restoration review with T19 coordination. [Failure log](evidence/r12-global-search-failure.log). Previous narrower search passes do not establish current acceptance.

## Completed independent r12 added-entry sweep

Seven additional QA entries completed on the immutable r12 binary: result presentation, execution eligibility, archived research, T11 affordances, header layout and planning passed (6); global search failed (1, already returned). [Index](evidence/r12-added-index.json). Together with the original baseline this gives 32 passing and 6 failing executed entries, with two controlled performance entries still pending. Independently passing task-edit, command, research-inputs and navigation-context checks are additional focused evidence and do not erase these failures. T19 keyboard-visual is newly implemented after r12 and requires a fresh binary plus native verification.

## Independent r13 targeted checkpoint

Fresh QA build exited 0; immutable binary SHA-256 `ad6c30e36443baae17a62fab8f7684e8d4f60957e7e6170dddbde33fdd598d64`. Navigation-context, keyboard-visual and task-edit suites exited 0; global-search still exited 101 on the second Enter after Stay. The trace restores the Search FocusId but shows no second input Enter event. Returned this narrower evidence to T10. [Index](evidence/r13-targeted-index.json).

T19 suite success is not full acceptance: Search Tab checks only the open flag, dirty Escape falls back to a mouse Stay click, and required nested/activation/error-state coverage is incomplete. Returned these test weaknesses and requested actual focus membership, direct Escape outcome, Enter/Space activation and complete surface/state coverage. An isolated native source snapshot r13 is building; production app remains untouched.

## Independent native r13 keyboard checkpoint

Isolated native build exited 0 and was installed only into Planning Native Review.app. Signed binary SHA-256 `5d4fdae59c9ef928495ffd23086ac9edd950594721046a9fe5a71e138c6a0c86`. Computer verified new-project Escape dismissal, project-rename Tab + Space cancellation, and archived TASK-G1 Version history initial Enter closing plus full six-control Tab and Shift-Tab cycles returning to Close and activating it with Enter. The archived target remains selected. Screenshot after first Tab shows the first version row focused. [Evidence](evidence/native-r13-keyboard-review.txt), [source snapshot](evidence/native-r13-source.json). These cases pass; broad T19 and release acceptance remain open. Production app/data were untouched.

## S3d material retry ownership review

Returned additional live adapter defects: failed Remove retries reprepare by list index and can retarget a different material after reorder/replacement; refresh failures discard the pending intent; cleanup reference collection omits other modal/draft/dispatch owners, and cancel reads the pending option after taking it. Required exact original content identity on retry, retained failure ownership until explicit discard, and a shared complete reference collector. Prevalidated reads and deferred post-durability cleanup are present but do not by themselves establish material acceptance.

## Native r13 search/dirty-exit countercheck

Computer independently passed the exact Search → Stay → second Enter sequence that fails in the headless suite. Edited isolated E24 title without saving, opened Search with Cmd+K, searched Analyze, pressed Enter to open dirty decision, clicked Stay, pressed Enter again and observed the decision reopen. Discard opened selected A21, and Back returned original E24 without an editor. The temporary draft was discarded, not saved. [Evidence](evidence/native-r13-search-draft.txt).

This changes diagnosis: a native production failure is not established by the current same-closure headless assertion. Returned to T10 to settle deferred focus/input events between actions while preserving all assertions. Native success does not waive automated regression or full S6 requirements.

## Independent r14 batch checkpoint and worker review

Fresh QA build exited 0; immutable binary SHA-256 `be74bb78e092e72b94561676775be20f0ccc945304df63db51637d5f403256c4`. Independent `--qa-batch` exited 0, fixture `.local/batch-qa/203`; [log](evidence/r14-batch-pass.log). The reviewed fixture now supplies an explicit plan/approval before queue submission and verifies Pause admits no further child beyond the already submitted first step. The legacy success-line phrase "automatic planning" is stale and is not evidence that queue submission plans automatically. Simplify progressed past cancellation but still fails result Back origin; returned to T10.

Worker source now provides request_shutdown and join-only-if-finished, with Durable receipt reservations retained. Independent focused tests are running. Returned remaining test gaps: eliminate mem::forget in budget test and prove release/re-admission; account exact Durable/Returned/Rejected generation outcomes during shutdown; preserve nonempty committed WAL across migration crash; mark exact fault reached before deliberate abort. No live persistence or crash-recovery acceptance yet.

The independent worker test command exited 101 before running tests: new scoped_material test code moves applied.undo at line 925 and reuses it at line 937 (E0382). Returned to T13; this is a shared test compilation blocker, not a proven worker runtime failure. [Log](evidence/r14-worker-compilation-blocked.log).

## Independent focused tests and scoped Undo regression

After material test compilation was fixed, independent worker tests passed (9 tests, exit 0) and scoped material tests passed (5 tests, exit 0). [Worker log](evidence/r15-worker-tests.log), [material log](evidence/r15-material-tests.log). These are focused checkpoints, not full task acceptance.

Review found `UndoHistory::clear_legacy` retains only TaskEdit, silently deleting newly added scoped Comment and Material entries during unrelated runtime invalidation. Returned to T13 with mixed-stack ordering/reference tests and a requirement to remove only Legacy entries. Also returned crash-fixture corrections: distinct pre-WAL baseline and WAL-latest titles, exact after-commit SIGABRT/marker checks, and timeout child reaping.

## Independent subprocess crash checkpoint r16

`cargo test --features qa --test v4_crash --quiet` independently exited 0 with 3 tests passed. Reviewed independent child launch, exact SIGABRT/checkpoint markers, post-COMMIT retry idempotence, mid-group rollback, and live parent WAL writer preserving distinct baseline/latest migration state. [Log](evidence/r16-subprocess-crash-pass.log), [source hashes](evidence/r16-persistence-source.json). This accepts the exercised SQLite subprocess boundaries only; real UI dispatch crash admission, live tracking, lazy loading and performance remain required. Full binary unit regression started after this checkpoint.

## Native report Back and full-unit build check

Computer verified Archive list → TASK-G1 → View final report → Back returns to the same archived goal overview on native r13. [Evidence](evidence/native-r13-report-back.txt). This is a native archived-goal route pass, not proof of the ordinary/programmatically seeded simplify fixture; returned to T10 for a precise route/frame comparison.

Full binary unit regression r16 did not execute: new mixed Undo test lacked UndoStack and UndoRecord imports (E0433). Returned imports and stronger full stack-order assertions to T13. [Build failure](evidence/r16-full-unit-compilation-blocked.log). Production clear_legacy now explicitly preserves TaskEdit, Comment and Material entries, pending successful regression.

## Independent full unit checkpoint and L1a authorization

Full binary unit regression independently exited 0: **612 passed, 0 failed, 2 ignored**. [Log](evidence/r17-full-unit-pass.log). Reviewed isolated worker lifecycle/admission/receipt corrections and strengthened tests together with prior real subprocess crash evidence; accepted this isolated foundation only. [Source hashes](evidence/r17-worker-foundation-source.json).

Authorized L1a implementation of compile-enforced tracking for run, automation and occurrence entities, including internal Store writers and clean bootstrap, exact payload/tombstone drain and no raw mutable collection escape. No whole-Store diff/clone is allowed for tracking. Coordinated mechanical accessor ownership with T13. L1b real isolated DispatchRun durability admission, all remaining entities/scalars, production activation, lazy bodies/UI history queries and controlled benchmarks remain required. Full T14/T15 remain open.

## T19 fixture lifecycle and opener regression review

Independent inspection found the keyboard activation fixture opens the same dialog twice without closing it, overwriting the remembered page opener with the first dialog input that is then replaced. Returned this concrete fixture defect. Rejected the simultaneous production workaround that discards the stored opener and always focuses the app root, and the weakened test that checks only Some focus rather than exact opener restoration. Required one actual open per scenario, a live page opener and exact restoration semantics; native keyboard passes remain historical and do not approve this newer workaround.

S3d current material UI QA covers ordinary-task success/menu Remove/Undo/history references, but pending failure/retry/new-impact/cleanup and real Add Files entry still need evidence. Returned that coverage gap explicitly; an immutable r18 task-edit rerun is in progress.

Independent r18 task-edit QA exited 101 at the new material removal menu: test uses visible label "Remove from goal" as ElementId, but no such ID is registered. Returned real-menu selector/frame repair without direct handler bypass. Immutable SHA-256 `19544043ee1ebf10493b621ecbedc2ed0670ee9076b55acaa14ea630cb6a765c`. [Failure log](evidence/r18-material-menu-failure.log). Earlier task-edit passes predate this added material case.


## Additional material ownership failures returned

Independent review found that initial Add confirmation owns its plan only in research_save_impact; cancellation drops that plan before pending-material cleanup can see it, potentially orphaning imported copies. The import completion path also ignores a failed non-modal save and can overwrite its error with a success toast. Successful import into a target archived or deleted during import drops attachment ownership without cleanup. Returned all three to T13 with initial-confirmation cancellation, durable failure/retry and target-disappearance tests. These remain unaccepted until independently verified.

## Updated visual acceptance contract

The user explicitly requires action hierarchy rather than uniformly prominent buttons. Added target contract 11 to TASKS.md and dispatched it to T13, T10 and T19 using the existing OpenCode Go DeepSeek v4.1 Flash sessions. Primary, secondary, icon-only and More-menu placement must follow page intent and frequency while preserving discoverability and predictable behavior. This supplements existing correctness and full visual acceptance, and does not add VoiceOver scope.


## L1a tracking boundary review returned

The initial tracked collection exposes raw mutable iteration/indexing, allowing identity changes without its mutation guard. Ordinary Clone clears dirty state, and drain consumes a violation before returning an error, permitting a later retry to forget the violation. Public Store collection fields also permit replacement with a clean collection, bypassing deletion tracking. Returned guard-only mutations, restricted bootstrap ownership, preserved pending intent, sticky violations and atomic cross-collection drain with targeted regression tests. L1a is not accepted; live persistence activation remains unauthorized until this boundary and downstream durability admission pass review.

Independent feature-qa compilation during L1a exited 101 with 19 errors, including guarded mutable-borrow lifetimes and a moved background snapshot in T19 QA. Returned each class to its owner; no new binary or runtime success is claimed. [Log](evidence/r19-tracking-compilation-blocked.log).


## L1a source-integrity recovery

An OpenCode substring replacement truncated domain.rs from 2431 to 560 lines, removing Store definitions and blocking all builds. Root independently recovered the exact pre-edit file from OpenCode snapshot tree f05555a5dd5ca0471e92338f4663c8fbc844086e into a local recovery artifact without overwriting shared source. Returned recovery instructions and the failed end-marker diagnosis to T14; other UI owners were told to avoid repeated builds until repair. Recovery must preserve prior shared changes and reapply bounded guarded edits. No acceptance is implied.


## Material pending ownership and More-menu review

Returned another S3d issue: successful Material Undo unconditionally clears the global pending material edit even when it belongs to a different failed Add; this loses retry intent and can orphan its copy. Required identity-aware preservation or explicit conflict resolution and Add A / failed Add B / Undo A coverage.

The revised task More menu invents Copy task link with a wthayp://task/{id} URL, but repository inspection found no registered scheme or incoming handler. Returned removal of this unsupported promise; visual hierarchy work must reorganize supported actions rather than introduce broken destinations. Independent icon embedding check passed with 51 registered icons. This narrow static result does not substitute for compilation, keyboard or native light/dark evidence.


## T17 reconciliation dispatched; L1a follow-up

Started OpenCode session ses_e43e5c93fffe71D24BgQu1KTuv with opencode-go/deepseek-v4.1-flash for T17, scoped to product/user/workflow/route documentation. DESIGN remains owned by T19. The authorization explicitly distinguishes current behavior from unfinished persistence, lazy UI and scoped Undo targets, requires source-backed lifecycle definitions and preserves final integration verification. Local prompt: .local/opencode-dispatch/T17-document-reconciliation.txt.

T14 has restored the domain definitions from the recovered snapshot and extracted the tracked container, but compilation and boundary verification remain incomplete. Returned duplicate-key insertion (first-match drain can select the wrong payload) and crate-visible clean-reset APIs on live Store as additional tracking gaps. The unsupported task-link menu action has been removed in current source; keyboard/native verification of the revised menu remains pending.


## T19 incomplete handoff returned for full coverage

The T19 process reached a terminal handoff. Independent source review found that visual_search still checks an empty query despite the handoff describing populated Search coverage; the keyboard activation comment promises Enter/Space while only Enter is exercised. Automation settings/results, nested Select Escape and required disabled/error/long-title states remain missing. The new-project visual fixture remembers the opener after focusing its new dialog input, so it cannot establish faithful opener restoration. Resumed the same session with T19-remaining-return.txt to complete these cases without weakening assertions. Current source hashes: [r21 checkpoint](evidence/r21-source-review.json).

Corrected the handoff diagnosis: domain.rs is shorter because TrackedVec was extracted after recovery; line count alone does not prove it remains truncated. The actual blocker is incomplete guarded accessor conversion, still owned by T14. No new runtime or native pass is claimed.

Independent r21 `cargo check --features qa --quiet` exited 101 with 52 errors after recovery, primarily guarded borrow lifetimes and remaining immutable-accessor mutation callers, including feature-only QA. Returned exact log and per-file counts to T14. [Log](evidence/r21-compilation-blocked.log). No binary was copied or tested from this failed build.


## Independent isolated tracking reproduction r22

To isolate the container from ongoing shared build failures, root compiled an immutable copy of domain_tracked.rs with minimal entity stubs. Three focused tests ran: ordinary Clone retains pending changes and identity violations remain visible on repeated reads passed; duplicate identity failed. After loading id 1/value 10 and pushing id 1/value 20, pending_changes returned the old value 10 while the live collection contained two entries. Returned this executable reproduction to T14. [Log](evidence/r22-tracking-isolated.log), [source hash and scope](evidence/r22-tracking-source.json). This is actual generic-container evidence only, not Store integration, persistence or application acceptance.

Independent r23 feature-qa check exited 101 with six errors, reduced from r21's 52. Five concern guarded run/occurrence borrow lifetimes; one is an in-progress material save bool-to-typed-outcome caller conversion. Coordinated non-overlapping research_app sections between T14 and T13. [Log](evidence/r23-compilation-blocked.log). Compilation is still not accepted.


## S3d refreshed retry identity returned

The new pending-clear guard compares entire prepared plans. retry_material_save refreshes the plan but only replaces pending when impact changes. For an ordinary-task failed Remove followed by attachment reorder, retry can save the exact target correctly while its new index/before state differs and impact remains None; the old pending plan is then retained after success, blocking future operations. Returned stable operation identity or matching-intent refresh before commit, with failed Remove/reorder/success/pending-cleared/next-operation coverage. This correction must preserve unrelated pending work and retain actual failures.


## Independent isolated container retest r24

The fixed container passes four root tests: duplicate insertion leaves the live collection unchanged and returns a persistent tracking error; loaded duplicates are detected; ordinary cloning retains pending changes; identity violations remain visible on retry. [Log](evidence/r24-tracking-isolated-pass.log), [immutable source and harness scope](evidence/r24-tracking-source.json). The first harness compilation encountered newly embedded app-specific tests; the final harness excludes only module documentation and that embedded test module, leaving production container code unchanged and exercising independent tests. This does not accept Store/bootstrap boundaries or live persistence. Actual full binary unit regression started separately.

Actual r24 full binary unit regression could not execute: 109 compilation errors remain in cfg(test) callers, including unconverted mutations and accidental method conversion of unrelated GoalRuntimeStamp fields. Returned full log and file counts to T14, retaining all assertions. [Log](evidence/r24-unit-compilation-blocked.log). The isolated four-test pass must not be generalized to application test success.

Returned a further S3d race: an import already in flight can complete after a second material command becomes pending; dispatch rejects the imported Add before taking ownership, and the caller continues without retaining or cleaning its copies. Required admission coordination or explicit rejected-copy handling while preserving the existing pending operation and save error.


## Independent immutable QA checkpoint r25

QA build exited 0; copied binary SHA-256 5b052937c552c1ef36c5b23f8406eeb2e11c5971077914dac38636070a497e30. Task-edit exited 101 at the new material-failure pending assertion: the fixture sets storage=None before import, so data_dir rejects before copying/saving and never creates pending state. Returned an actual SQLite write-failure fixture preserving a valid parent data directory and assertions that import/save were attempted. [Log](evidence/r25-task-edit-failure.log).

Global Search still exits 101 on second Enter after Stay despite the new frame settling. Returned current immutable evidence to T10; prior native r13 success remains a diagnosis countercheck, not a waiver. [Log](evidence/r25-search-failure.log). Full unit tests remain a separate gate. T19 terminal handoff again omitted automation results/nested Select implementation; resumed that same session explicitly to complete those missing cases.

The same immutable r25 binary also failed topics (Back scope All vs Topic 2), topic-actions (None vs selected goal 1), keyboard-visual (populated Search asserted before hits appeared) and simplify. Simplify now passes its earlier report Back check and reaches an obsolete test expecting the removed unsupported Copy task link; its blind menu index can now invoke another action. Returned topic routes to T10 and label-checked supported-menu regression plus settled populated Search to T19. Logs: [topics](evidence/r25-topics-failure.log), [topic actions](evidence/r25-topic-actions-failure.log), [simplify](evidence/r25-simplify-failure.log), [keyboard visual](evidence/r25-keyboard-visual-failure.log). None of these suites is accepted.


## L1a bootstrap restriction and unit regression r26

Independent full binary unit compilation exited 101 with 43 errors, reduced from 109; tests still did not execute. Returned remaining per-file counts and [log](evidence/r26-unit-compilation-blocked.log) to T14.

The clean-reset methods became a consuming with_loaded_tracked method, but consuming an existing Store still permits dropping its tracked state. Returned a genuine fresh-builder restriction, or validation that all tracked collections are empty with no dirty keys/tombstones/violations. Required populated-clean and dirty-cleared rejection plus truly fresh clean-load tests. No L1a or live persistence acceptance is implied by the container-only passes.


## Independent immutable QA checkpoint r27

Fresh QA build exited 0; immutable binary SHA-256 3d125bc5590e84856073dd478eee8db26ef8558297298f9f9ded24283463f4d7. Task-edit passed, including new material initial Cancel cleanup, actual failed SQLite write/Retry, identity-based Remove after reorder, import admission and preserving failed Add B while undoing Add A. [Log](evidence/r27-task-edit.log), fixture .local/task-edit-qa/15366. This accepts exercised cases only; material failed Undo, changed-impact footer Retry, native Add Files and remaining T13 migration remain required.

Global Search passed its earlier Stay/second Enter point but failed later at deleted-target explanatory feedback (line 1144). [Log](evidence/r27-global-search.log). Keyboard-visual still failed its immediate populated-Search assertion. [Log](evidence/r27-keyboard-visual.log). Returned each current failure to its owner. T17 interim review also found contradictory Back descriptions and stale T25/T22 status in the route matrix; returned evidence-backed corrections before handoff.


## Root intervention and mandatory scroll acceptance

The user authorizes root to directly fix repeatedly returned issues. Root took exclusive ownership of T19 visual_search and separated input dispatch from its subscription-driven result assertion, adding a visible exact-result assertion. Verification is pending: r28 build was blocked by new automation-result fixture guard lifetimes and a missing focus helper, returned to the T19 owner. No pass is claimed for the direct fix yet.

The user also elevated scrolling smoothness to a hard delivery requirement. Added Mandatory scrolling acceptance to TASKS and dispatched it to T14, T19 and T17: actual native scroll, frame-time/long-frame evidence, large histories and concurrent runtime/persistence workloads, both sizes/themes. Any reproducible hitch remains a failing case; static images, average timings and synthetic collection slicing are insufficient. Existing full-scope acceptance remains intact.


## Root direct Search fixture correction verified r31

Root corrected the repeatedly failing T19 populated-search test: dispatch real input and settle subscriptions before inspecting results, verify the actual query, assert the expected parent-and-four-child match set, and query the result leaf within its container scope rather than trying to observe the container itself. An initial root assumption that only one result should match was corrected after source inspection: descendant context intentionally includes the parent title.

Fresh build exited 0; immutable SHA-256 78bc98cf3e6791f617571ffa0f6b2947878f21b9d4b59d80fa9ad5a4c28ef136. All four visual size/theme iterations progressed past Search; full keyboard-visual then failed the new automation-result initial Close-focus assertion (actual FocusId 31 vs expected 29). Returned that later failure to T19 and handed back the verified visual_search function. [Log](evidence/r31-keyboard-visual.log). This is a narrow fixture correction, not full T19 acceptance.

Native performance tooling is available: xcrun locates Xcode xctrace, whose installed templates include Animation Hitches, Time Profiler and Metal System Trace. This establishes measurement capability only; actual scrolling traces and acceptance are still pending.


## Full unit regression restored r32

Independent full binary unit test command exited 0: 618 passed, 0 failed, 2 ignored. [Log](evidence/r32-full-unit-pass.log). L1a fresh-bootstrap restriction and duplicate validation are present; explicit cross-collection atomic drain evidence remains requested before that phase is accepted. No live backend activation or scrolling acceptance is implied.

Root took over automation_result_keyboard after repeated incomplete T19 handoffs: actual period-result entry, independent Enter/Space/Escape scenarios, exact remembered opener and background restoration, and 24 forward/reverse focus moves inside the full dialog scope. The production opening focus was already changed to the existing Close focus handle when root attempted its patch, so root preserved that current change rather than claiming authorship. Fresh build and verification are underway.


## Independent full Search and current keyboard/visual suites pass r34

Fresh QA build exited 0; immutable SHA-256 c3538c4df0b6d84587f2415585b71fdf258fab01ae62be314ccc0769f709f874. Global Search exited 0, fixture .local/global-search-qa/22367, including the previously failing Stay/second Enter and deleted-target cases. [Log](evidence/r34-global-search-pass.log).

Current T19 keyboard/visual suite exited 0, fixture .local/t19-qa/22366. Root strengthened automation-result scenarios to use the real Open control, exact live opener/context restoration, Enter/Space/Escape and 24 Tab/Shift-Tab moves inside the full dialog. A subsequent Search keyboard failure was a fixture defect: it captured Search-input focus after opening as its expected opener. Root corrected it to the remembered pre-overlay focus, retaining exact equality. [Log](evidence/r34-keyboard-visual-pass.log), [source checkpoint](evidence/r34-keyboard-source.json).

These full-suite passes establish the exercised automated cases only. Fresh Computer native verification, screenshot review, full remaining route coverage and mandatory scrolling traces still remain; T19/T20 and overall delivery are not accepted.


## r35 independent tracking and native checkpoint

- Full node-enabled `cargo test --features qa --quiet` exits 0: 619 passed, 2 ignored, plus 3 real subprocess crash tests passed. Evidence: [r35 full tests](evidence/r35-full-tests.log).
- Root fixed tracked identity reassignment to reject an existing target key before mutation and added a preservation regression. L1a acceptance covers runs, automation definitions, and occurrences only; other entity/scalar tracking and production worker activation remain incomplete.
- L1b is dispatched to the existing OpenCode Go DeepSeek session with `.local/opencode-dispatch/T14-L1b-authorization.txt`. The live process was independently confirmed. Dispatch may launch only after its matching durable acknowledgement; failure/retry, concurrent state, and app-level crash evidence are required.
- Isolated native r35 signed executable SHA-256: `deb9c5cdf9ff677604cd7a655c872a1ce5ad6ed7c8528b049295db75cc92ec7b`. Only the isolated app was restarted; production was untouched.
- Computer verified Automation results opens a period result and Enter and Space close it while retaining the list. Escape closes after a settled frame. A batched open/Tab/Shift-Tab/Escape sequence left the dialog visible until a second Escape; this timing observation is unresolved and is not counted as a passing rapid-input regression. The initial CUA handle also became stale and required rebinding to the same live app.
- Native scrolling frame distributions and the full native keyboard/theme/size matrix remain outstanding. No full-task or release approval is inferred from this checkpoint.


## T17 independent documentation return

Root found a current behavior contradiction in USER_GUIDE step 4: it promises automatic return to the project Kanban after planning, whereas `project_app.rs` completion preserves the current route and prompts the user to open the project. T17 is returned for correction and an audit of related navigation promises. Historical Search/material failures in the handoff also need reconciliation with independent r34/r27 passes, without claiming full integration acceptance. The documented double-primary project toolbar remains an explicit hierarchy follow-up. Revision prompt: `.local/opencode-dispatch/T17-review-return.txt`.


## Focused follow-up: task menu regression

Independent source inspection identified a stale menu selector in `simplify_qa.rs`: the current task menu starts with Edit task, but the fixture clicks index 1 before asserting an editor. The former fake Copy link was removed, so the positional assumption no longer describes the UI. T10 received this precise return instruction: target the real Edit action, preserve product behavior, settle asynchronous events, and verify the current cancel action. This diagnosis is not a passing rerun. The T13 command QA process was independently confirmed absent after an earlier live observation; its owner was instructed to inspect the terminal outcome rather than infer success or failure from elapsed time.


## L1b early independent return: synchronous intent I/O

The in-progress dispatch adapter still performs synchronous filesystem reads/writes and whole-pending-intent serialization through `open_internal`/`restore_intents` and `admit`/`remember_intent`/`write_intents`. Its caller is the app command path, so moving SQLite work to a worker does not yet remove UI-thread persistence stalls. Root returned this concrete blocker to the existing T14 session: move intent persistence, restoration and acknowledgement cleanup to a bounded worker-owned protocol while preserving crash retention and durable admission. The current write/rename implementation also does not establish file/directory durability for a system crash. L1b remains unaccepted; intermediate fixture failures must be fixed without weakening approval or execution eligibility.


## L1b early independent return: observable launch idempotency

The draft QA fake suppresses duplicate `start` calls itself. Root rejected that as evidence of application idempotency: the recorder must retain every invocation and tests must prove the real adapter/scheduler prevents duplicates. The draft `apply_durable_parts` returns queue=true for an identical queued run, so duplicate receipt/replay needs explicit coverage. Root also required recovery evidence around durable commit, acknowledgement and launch; acknowledgement errors were discarded, and a second incoming intent while one dispatch awaited durability was returned without retention. These are in-progress source review findings, sent to T14 for correction before acceptance.

A native Instruments availability recording was started against only isolated app PID 25910 using Animation Hitches. The tool reached its 15-second recording limit and remained live finalizing at the last observation. This is an instrumentation availability check with no scroll workload, not smoothness evidence. Log: `.local/native-review/r35-instrument-availability.log`; pending trace: `.local/native-review/r35-instrument-availability.trace`. The full native scrolling workload matrix remains mandatory.


## Native instrumentation and material responsiveness review

- The isolated PID 25910 Animation Hitches recording completed with exit 0, as did XML table export. Trace metadata identifies the built-in display at 120 Hz. The export exposes frame-lifetime, display and hitch tables. No deliberate scroll workload ran, and frame-lifetime rows alone do not establish application-specific presented-frame timing; therefore this does not pass any scrolling gate. Local evidence: `.local/native-review/r35-instrument-toc.xml`, `r35-frame-tables.xml`, and the availability trace/log.
- Root found synchronous full-file reads/SHA-256 in live material remove/confirm and Undo paths (`material_app.rs`, `scoped_material.rs`, `app.rs`). Some paths hash before and after lists repeatedly. T13 was instructed to preserve identity/conflict checks while moving heavy file work to bounded background jobs, revalidating live state on completion, and covering large files and concurrent edits. Narrow r27 correctness acceptance does not accept this responsiveness issue.
- The project toolbar's two-primary inconsistency was dispatched to T19 for a narrowly scoped secondary Run/resume styling change and DESIGN label reconciliation. Native light/dark validation remains required.


## r35 actual native Settings scroll baseline

Computer exercised six native scroll gestures on Settings → Agent & models in the isolated r35 app, Light, 1440×920: down 2 pages, up 2, down 1, up 1, down 2, up 2. Screenshots confirmed movement between Global default and Browser runtime, with the profile toolbar fixed. Animation Hitches recording targeted only PID 25910 for 30 seconds. Workload manifest: `.local/native-review/r35-settings-scroll-light-workload.json`; trace/log share the `r35-settings-scroll-light` prefix. Recording finalization and frame attribution/analysis remain pending at this checkpoint. There was no deliberate concurrent in-app save/run workload, and other coding/build processes were active, so this is a baseline sample and not a release gate pass.

T19 delivered the narrow project hierarchy change: New research retains primary fill, Run/resume or Add to queue uses the existing secondary button, and Project options remains More. Root inspected the sole callsite and unchanged action/disabled/tooltip semantics. DESIGN now matches the existing result-dialog Cancel placement and Research settings label. Compilation and full theme/size native verification remain pending during active persistence edits.


## r36 independent admission tests and remaining acceptance gaps

Root ran `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --features qa --quiet dispatch_admission::`: actual exit 0, 6 passed. [Evidence](evidence/r36-admission-unit.log). Source inspection confirms the fake launch recorder now records every invocation rather than hiding duplicates. File intent persistence now runs on the worker, but UI encode/decode and the intent path's missing shared byte budget still require correction. Root returned those concrete gaps and required oversize/backpressure/startup-bound tests.

The current app QA recovery case constructs a Store containing a queued run. This is useful recovery-state coverage but does not establish actual app crash/reopen behavior. Root explicitly required subprocess crashes at commit/ack/launch seams, including after launch but before persisted status, with every launch recorded. The earlier journal subprocess tests do not substitute for this adapter-level evidence. L1b remains unaccepted.

T13's prior process was confirmed terminal and its correctness handoff reviewed. It was resumed with `.local/opencode-dispatch/T13-material-background.txt` to implement the material background pipeline and continue the remaining settings/context/archive migrations. Existing compile conflicts are temporary integration conditions, not grounds to omit those tasks.


## Native scroll trace result and QA binary isolation correction

The 30-second r35 Settings scroll recording/export completed successfully. Instruments reports 682 hitch events attributed explicitly to isolated app PID 25910; hitch durations median 8.333333 ms, p95 8.3335 ms, maximum 16.666791 ms, on the reported 120 Hz display. [Parsed summary and limitations](evidence/r35-settings-scroll-summary.json). These are hitch durations, not total frame times. The debug build, concurrent external workloads and absent exact input markers limit interpretation; this sample does not pass the scrolling gate and requires controlled optimized reproduction and attribution.

Root investigated live T10 QA PID 28242 rather than restarting on a timeout. A stack sample showed ordinary Application/NSApplication execution, and symbol inspection found no navigation-context QA or HeadlessAppContext symbols in `/tmp/t10-candidate-final` (SHA-256 `4ce7a171442f8e8219a4b392c6aa9c6aa1058cc7dc55e0bb4f753a71de7e7fe9`). It was a non-QA executable copied from the shared build target. Root stopped only that owned command chain (parent 28239, child 28242) to prevent subsequent QA flags launching ordinary app paths. [Sample evidence](evidence/T10-wrong-binary-sample.txt). No navigation pass or runtime failure is inferred from this invalid invocation. All active implementation owners received exclusive CARGO_TARGET_DIR requirements; T10 must also add a fail-closed guard for unhandled QA flags before any normal workspace initialization. This accidental normal-start boundary still requires read-only investigation; do not assume production data was untouched by that process.

T17's requested correction is independently confirmed: USER_GUIDE now describes passive background planning completion, matching project_app.rs. Current search/material status and project toolbar hierarchy wording now reflect the reviewed checkpoints, with integration/native gates left open. Final T17 reconciliation awaits the completed implementation.


## QA invocation containment and verified CLI protection

The T10 session launched the obsolete non-QA candidate again as PID 31128 (parent 31127). Read-only `lsof` confirmed this process held the normal workspace lock. The workspace SQLite modification time was close to startup, so absence of production impact is not established. Root interrupted the session and its child process immediately; subsequent lock inspection found no holder. No data rollback, replacement or repair was performed.

Root directly added a guard immediately before normal application initialization in `main.rs`: unhandled `--qa`, `--qa-*`, `--projects-live` and `--research-live` arguments print an actionable error and exit 2. Known QA commands still return through their feature-gated handlers first. The exact guard was compiled into the isolated r35 native source snapshot (build exit 0) and exercised as a real non-QA executable: all five cases exit 2 with no normal startup. [Executable hash and results](evidence/r36-cli-guard.json). This validates the guard on that coherent source snapshot; latest full-tree integration remains pending.

T10 may continue only with an exclusive QA target directory and a verified immutable QA binary. The obsolete `/tmp/t10-candidate-final` must never run again. The session received these instructions after guard verification. Root also returned remaining material worker concerns: unbounded response channel/request file list, whole-file buffering and suppressed thread-spawn errors must be resolved while retaining concurrent-file-change checks.


## r37 independent full unit regression

Actual command: `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --features qa --quiet`. Exit 101: 628 passed, 1 failed, 2 ignored. The failing test is `scoped_material::tests::apply_add_preflights_content_before_any_mutation`, at line 857 in the compiled candidate: changed content is no longer rejected by `apply_material_add` as expected. [Full evidence](evidence/r37-full-tests-failure.log).

Root returned the failure to T13 with the requirement to retain content identity and atomic rejection in the background verification design. Tests must cover the actual worker-to-UI proof and stale completion boundary; removing the safety assertion or restoring expensive UI reads is not an acceptable fix. The subprocess crash test target did not run after this failing unit target. No latest full-suite acceptance is claimed.


## r35 native CPU profile analysis and rendering investigation

The real settings-scroll trace CPU export completed. It contains 17,325 samples; the main thread has 15,536 ms of sample weight over the 30-second recording. Leading self samples include memmove (2,236 ms), flexbox layout (503 ms), allocation/free, scene primitive insertion and block layout. TaskApp rendering contributes approximately 1,724 ms inclusively. [Statistical summary](evidence/r35-settings-cpu-summary.json). These weights are not exact frame durations and do not remove the debug/concurrent-workload limitations.

Pinned framework source uses a CVDisplayLink-driven window frame source; no hardcoded 60 Hz cap was established in this inspection. The sidebar mark continuously animates and already has a separate entity. Whether it invalidates unrelated page work remains a hypothesis, not a proven cause. T19 was dispatched to examine invalidation/caching/layout using this evidence, preserve approved brand motion, and provide controlled optimized A/B candidates for root native profiling. Prompt: `.local/opencode-dispatch/T19-scroll-profile.txt`. All long-surface, theme/size, concurrent-save and history-scale acceptance requirements remain open.


## Material identity serialization compatibility return

Root identified an additional migration risk in the draft `Attachment::content_sha256` field: serde defaults absent legacy values to empty but serializes the new empty field, while research approval and input signatures serialize whole attachments (`research.rs`). Unchanged legacy materials can therefore produce changed signature bytes merely on reload. T13 was required to preserve legacy approval/report validity and add old-format attachment plus stored-signature roundtrip/reopen coverage. Filling a verification cache must not silently create a new research input revision. The current cached-hash helper also still reads legacy files synchronously and cannot substitute cached identity for fresh byte verification. These remain explicit acceptance blockers.

Root started a coherent optimized baseline build from the frozen isolated r35 source plus verified CLI guard, using separate package/target `.local/native-review/release-target` and two build jobs. It does not replace or restart the current native app. Log: `.local/opencode-dispatch/root-r35-release-baseline-build.log`. This baseline will separate debug-build overhead from the native rendering diagnosis; it does not represent completion of ongoing T13/T14 changes.


## r38 independent QA from exclusive verified binary

Root copied the completed exclusive T10 target build to an immutable QA executable (SHA-256 `956bfe6c825b34bb78e4a734367ea2403332fdfcc6807ca9137850305d1e832c`), inspected navigation QA/HeadlessAppContext symbols, and verified an unknown QA flag exits 2. [Binary evidence](evidence/r38-binary.json). Actual independent outcomes:

| Suite | Exit | Finding |
| --- | --- | --- |
| Topic actions | 0 | Report navigation and completed-only export shortcuts passed. [Log](evidence/r38-topic-actions.log) |
| Topics | 101 | Line 358 still expects CmdK to replace Topic with All; current global overlay preserves Topic. Return the stale fixture contract, not the product navigation. [Log](evidence/r38-topics.log) |
| Simplify | 101 | Edit-menu case is past its former failure; line 869 now attempts an ineligible ordinary research rerun. Fixture must use explicit reopen/preconditions. [Log](evidence/r38-simplify.log) |
| Task edit | 101 | Line 794 expects two materials immediately after Undo but sees one. Investigate and assert actual asynchronous completion and failure outcomes. [Log](evidence/r38-task-edit.log) |

All three failures were returned to their owners with exact logs and invariant-preserving requirements. These are valid QA executions, unlike the prior wrong-binary attempts. They do not close T01/T10/T13/T20.


The same verified r38 executable independently passed `--qa-navigation-context` with exit 0. [Log](evidence/r38-navigation-context.log). This is a focused navigation-context pass; the separate Topics/Simplify failures and full integration gates remain. The full global-search rerun was still live as PID 44224 at this checkpoint and is not classified by elapsed time.

## r39 independent review: compatibility and crash-ordering returns

All 25 tasks remain active; no release acceptance is implied. The previous model-availability reply was a verified observation of live OpenCode processes, not implementation acceptance.

- The immutable r38 global-search log contains its success marker. The prior shell handle is no longer available, so its exit code was not reconstructed; a fresh independent execution of the same immutable executable is running with an explicit JSON exit record. [Prior log](evidence/r38-global-search.log).
- An independent small Rust reproducer confirms that replacing the legacy attachment struct with a `serde_json::Value` signature payload changes JSON key order. Existing goal approval signatures can therefore become invalid despite unchanged materials. The new compatibility test generated its baseline using new code and missed this regression. Returned to T13 with a required old-format fixture, exact legacy serialization order, reopen/cache-fill checks, and saved run signature coverage. [Reproducer output](evidence/r39-signature-compat.log).
- T13's renamed content-rejection test currently asserts the metadata command succeeds after tampering and discards the original Store. It does not establish its named no-mutation invariant. Returned for an actual asynchronous application boundary test. Also returned file-handle/path identity, regular-file validation, checked byte budgeting and remaining UI-thread cleanup concerns.
- T14 now includes abrupt subprocess crash cases and unique QA roots. Remaining review blockers include recovery tests that only read a claim without exercising scheduling, an explicit-resolution method with no production UI caller, enqueue-before-claim-release ordering, and claim cleanup based on in-memory rather than confirmed durable status. Returned with concrete crash/backpressure/restart acceptance cases.
- T19 profiling follow-up was dispatched in the existing model/session with an exclusive target directory: parent/sidebar/settings/logo counter deltas, followed by a retained-view candidate only if evidence supports it. Headless counter output cannot establish native smoothness.
- The frozen isolated native release baseline build completed successfully. It has not yet been installed or profiled; build success is not performance acceptance. [Binary manifest](evidence/r39-release-baseline-build.json).

### r39 global-search result

Independent rerun completed with exit 0 in 23.19 seconds on the verified immutable r38 executable (`956bfe6c825b34bb78e4a734367ea2403332fdfcc6807ca9137850305d1e832c`). This accepts that focused regression only; later working-tree changes and the full release gate still require validation. [Log](evidence/r39-global-search.log), [exit/binary record](evidence/r39-global-search.json).

## r40 integration checkpoint

The preceding goal turn made concrete progress: independent signature-regression reproduction, recorded global-search success, and implementation returns. This turn independently ran the full unit/integration command against the ongoing candidate; it terminated with exit 101 during compilation, before tests could execute. Three `VerifyFile` initializers omitted the newly added `expected_sha256` field. The exact diagnostics were returned to the T13 owner. [Build log](evidence/r40-build-failure.log), [result](evidence/r40-test-result.json). No full-suite acceptance is claimed.

Source review confirms the signature fix now uses a typed borrowed four-field payload in legacy declaration order and an old-format goal-signature baseline. Saved completed-run compatibility and actual application-level tampering tests still require evidence. Material cleanup review also returned silent submit/delete failures and the reference race between UI preflight and worker deletion; remaining legacy snapshot removal is still required.

The optimized isolated baseline executable independently rejected four unsupported QA/live flags with exit 2 before workspace startup. [Release guard evidence](evidence/r40-release-guard.json). This checks launch isolation, not scrolling performance. Native performance acceptance remains open; concurrent builds must not be presented as a controlled frame-time measurement.

## r41 native optimized baseline installation

The r40 turn made concrete progress through a full compile failure record, implementation returns, and four verified release CLI safety checks. The known initializer changes are now in progress and a fresh independent full test run is running; no pass is assumed.

Root used Computer to quit only the isolated `Planning Native Review` instance, verified that its process exited, replaced only that bundle executable with the frozen optimized release baseline, ad-hoc signed and verified the bundle, and launched it through Computer. The All research screen visibly loaded the isolated Review laboratory fixture. The bootstrap still points to `.local/native-review/T24-relocated-data`; this installation did not launch the production bundle. [Exact source/installed hashes and bootstrap](evidence/r41-native-release-install.json).

The first post-quit AX observation reopened the old isolated build; root detected the new process, quit it again and verified process absence before copying. The final launch above is the installed release executable. This is preparation and a launch smoke check, not a native scroll-performance pass. The release baseline is frozen r35-era source plus the CLI guard; current in-flight T13/T14/T19 changes are not included in it.

### r41 full-test failure and recovery review

Full testing terminated with exit 101: 633 passed, 1 failed, 2 ignored. The failure is `dispatch_admission::tests::crash_before_acknowledgement_restores_the_same_intent_without_duplicate_run`: reopening reports `cannot read schema version: database disk image is malformed`. Integration crash tests were not reached. [Log](evidence/r41-full-tests-failure.log), [result](evidence/r41-test-result.json).

The fixture drops the adapter and immediately reopens. `PersistenceWorker::drop` requests nonblocking shutdown and may detach a still-running worker; `SqliteV4::open` starts with an immutable main-file probe. This suggests a possible reopen/checkpoint race, but causation has not yet been established. Root preserved the recent matching fixture directories and inspected them without repair: immutable integrity checks reported `ok`; ordinary read-only opens failed with `unable to open database file`. These observations do not prove either persistent corruption or production safety. [Inspection records](evidence/r41-reopen-db-inspection.json). Returned to T14 for a lifecycle-based diagnosis and genuine subprocess crash evidence; longer sleeps are not acceptance.

A separate review returned explicit launch resolution ordering: deleting the old launch claim before durably recording the user's chosen resolution still leaves a crash window. The failed/blocked decision must survive restart without permitting automatic launch, and a rerun requires its own durably admitted identity. Overlapping resolution calls must not overwrite the pending decision.

## r42 controlled native workload and independent visual regression

The preceding turn made concrete progress by installing/verifying the optimized isolated baseline, recording the full-suite recovery failure, and returning ordering defects.

Root coordinated a short no-new-build window with all OpenCode owners, waited for existing Cargo/rustc processes to finish, then attached Animation Hitches to isolated release PID 51116 for 30 seconds. No cargo/rustc processes were observed immediately before or after recording. Computer performed eight timed scroll actions in Settings → Agent & models, Light at 1440×920; screenshots confirmed lower Browser/runtime content and return to Global default. The first six actions were rapid reversals; the final down/up pair was observed separately. [Workload and action timestamps](evidence/r42-settings-release-workload.json). The trace `.local/native-review/r42-settings-release-light.trace` has reached its recording limit but is still finalizing; no frame result or performance acceptance is claimed. Build/test work was explicitly released after recording ended.

Independent full keyboard/visual QA passed with exit 0 in 16.14 seconds on immutable candidate `bbca1d50876ea635e977635cce3c0b2ec0394fcd658581d1a11486b847f0d6e2`; unknown QA flags exited 2. [Log](evidence/r42-keyboard-visual.log), [result](evidence/r42-keyboard-visual.json). This preserves the focused visual regression result, not full integration acceptance.

The counter probe printed 60 parent/sidebar/logo/settings renders for 60 forced idle frames and 240 each in its scrolling loop. Because it explicitly calls `render_frame`, those counts do not establish real idle invalidation frequency. The loop also emits 60 same-direction events, reaching scroll limits. Returned to T19 for truthful labeling and opt-in QA-only passive native sampling that never notifies GPUI or requests frames. The owner was resumed after its prior process terminated.

## r43 analysis of the r42 native release trace

The preceding turn made concrete progress by capturing timed native scrolling without overlapping Cargo/rustc, independently passing keyboard/visual QA, and rejecting forced-render counts as evidence of natural invalidation.

The 30-second Animation Hitches recording finished with exit 0. Exported tables attribute **86 hitches** to `planning-native-review (51116)` over 30.803676 seconds on the 120 Hz built-in display. Median hitch interval is 8.333333 ms, p95 16.6665 ms, maximum 16.666875 ms. Twenty-four hitch starts fall within the recorded gesture intervals plus one second; this is approximate wall-clock correlation, not a signposted input measurement. There are no potential-hang rows. Hitch intervals are not full frame durations or a computed FPS. [Summary, events and limitations](evidence/r42-settings-release-summary.json).

This optimized frozen baseline therefore does **not** establish hitch-free acceptance. It covers only Settings, Light, 1440×920, without concurrent app saves/runs or the large history fixture. It cannot establish the full required matrix. Root returned the trace and timing evidence to T19; passive native invalidation logging and CPU analysis remain the next diagnosis steps. The CPU export is running, and no single root cause has been asserted. Other normal desktop services remained active.

Additional T14 review returned a remaining explicit-rerun crash window: clearing the resolution intent and then allocating a new attempt can lose or duplicate that attempt. The persisted resolution must carry a stable replacement identity and retain/replay it idempotently until admission is durable. The synchronous intermediate save path still does not satisfy final worker-acknowledged T14 activation.

CPU export completed: 17,919 samples, with 16,600 ms of main-thread statistical sample weight. Leading self sample weights include memmove 2,818 ms, flexbox 758 ms, allocation 525 ms and bounds insertion 465 ms. Inclusive model-settings and sidebar weights are 838 ms and 763 ms. These are statistical samples, not exact frame durations, and do not establish one invalidation source. [Resolved CPU summary](evidence/r43-release-cpu-summary.json). Returned to T19 alongside the native trace.

## r44 direct integration repairs and stronger navigation check

After repeated returns, root took ownership of the simplify QA result-return block and notified T10 to avoid concurrent edits there. The fixture now establishes an explicit report-task caller, verifies exact run/context restoration through the result's original-task menu, settles frames between Back actions, and separately checks structural parents after origin history is consumed. No production navigation behavior was changed by this test edit.

Root also fixed a shared compilation blocker in `settings_views.rs`: the lazy General-panel closure constructed a `general` value but returned unit. Returning the value restored compatible match-arm types; the shared QA build then passed. The owner was notified and later T13 scope remains authorized.

The first stronger simplify run failed at restoring the result after Back (exit 101), so it is not accepted. A diagnostic version records origin/history to distinguish fixture dirty state, hit targeting and an actual route defect. [Failure](evidence/r44-simplify.log), [binary/exit record](evidence/r44-simplify.json).

Root prepared `.local/native-review/probe-source-r44` from the frozen isolated baseline plus QA-only passive render counters. It preserves interface behavior and bootstrap isolation; a compile-time fallback supplies only the isolated probe-output path. The optimized QA build is running. The uninstrumented baseline was preserved separately as `.local/native-review/planning-native-r35-release`, SHA `c684b245dda84a65838d864cd229b49c81b94a2af0a5bc7e4db1c9e669545ccf`. No passive native measurement or new performance acceptance is claimed yet.

The diagnostic simplify candidate (`5c226fb5f628da1fc57c5ba2a811257e31bc07ddb9e08695958ef6cdb12ea9c4`) passed the stronger result-return block in its first two configurations, then terminated with exit 101 at a later `sidebar-task-3` visibility failure. Concurrent production navigation edits mean the improved result cannot be attributed to diagnostic logging alone. Root released the test block to T10 with instructions to preserve its stronger assertions and finish actual visibility/size coverage. [Diagnostic log](evidence/r44-simplify-diagnostic.log). Full simplify/T10 acceptance remains open.

## r45 passive native invalidation evidence

The preceding turn made concrete progress with a direct compilation fix, stronger navigation tests and a built diagnostic snapshot. Root installed the optimized passive-counter build into only the isolated native bundle after quitting/verifying process exit, then reopened it with Computer. [Build/install hashes](evidence/r45-passive-install.json).

On Settings → Agent & models, Light 1440×920, an idle interval of 14.461 seconds contained **1,707 render invocations each for TaskApp, sidebar, logo and settings**, approximately 118.04 per second. No app input or CUA observations occurred inside that interval, and the sampler neither requests frames nor notifies GPUI. These are natural native render counts, unlike the earlier forced headless-frame probe. Other desktop and coding activity was not suspended, so this is invalidation evidence rather than a controlled frame-time acceptance. Down/up native scrolling was also performed and the page visibly returned to its top. [Markers](evidence/r45-passive-markers.json), [sampled counters](evidence/r45-passive-counts.tsv).

Returned to T19 for production optimization. Local framework inspection identifies `Entity::cached(StyleRefinement)` in gpui-pre 0.3.10; animation requests notify the current view and dirty propagation marks ancestors. A cache boundary containing the animation may therefore invalidate continuously; correct retained sibling content and data/focus/scroll/theme invalidation must be demonstrated. No dependency/registry changes or animation removal were authorized by this evidence. A new independent full test command is running against current implementation; no result is assumed.

### r45 full test result

Independent `cargo test --features qa --quiet` completed with exit 0: **635 unit tests passed, 2 ignored; 6 dispatch crash tests and 3 persistence crash tests passed**. [Full log](evidence/r45-full-tests.log), [result](evidence/r45-full-tests.json). This supersedes r41/r40 failures for this tested candidate, but does not waive uncovered admission/resolution cases, full live migration, UI QA, or native smoothness. Later edits require validation.

## r46 independent app QA and uncovered identity races

The preceding turn made concrete progress through passive native invalidation evidence and a passing full unit/crash checkpoint. Root independently ran `--qa-dispatch-admission` and `--qa-task-edit` on a guarded immutable executable; both exited 0. Material QA completed in 5.46 seconds. [Binary](evidence/r46-binary.json), [admission log](evidence/r46-admission.log), [material log](evidence/r46-task-edit.log), [material result](evidence/r46-task-edit.json). These are scoped app-QA results, not complete T13/T14 acceptance.

T14's terminal handoff was reviewed and the same OpenCode session resumed with a concrete new blocker: the replacement run ID is selected without reserving the allocator across asynchronous resolution persistence. An unrelated creation/admission can consume it, and the completion currently treats any matching run ID as sufficient without exact ownership validation. Requested atomic reservation/admission, overflow handling, ownership checks, and real restart/interleaving tests through resolution execution rather than only marker persistence. Full live tracking, async acknowledgements, migration and indexed/lazy UI remain authorized.

A subsequent T13 edit adds asynchronous removal of legacy uncached materials, but its pending job retains only task/index and applies the returned digest to the attachment currently at that index. No submitted path/size/list-generation comparison occurs at completion in this reviewed version. Reordering/replacing attachments can therefore retarget removal. Returned with a required immutable identity/revision check and deterministic delayed-worker test. This later change is not covered by the older passing immutable material executable. Busy cleanup obligation retention also remains under review.

## r47: native animation invalidation isolation and revision returns

The optimized frozen native baseline was rebuilt with only the diagnostic logo phase held static; production animation was not changed. On Agent & models in Light appearance, a 14.010-second passive interval produced zero additional parent, sidebar, logo, or Settings renders. The earlier animated r45 interval produced 1,707 additional renders in each counter over 14.461 seconds. This supports animation-triggered ancestor invalidation as a cause of idle rebuilds; it does not measure frame times or prove smooth scrolling. Other builds were not suspended. Evidence: [idle interval](evidence/r47-static-idle.json), [counter snapshot](evidence/r47-static-counts.tsv), and [binary identity](evidence/r47-static-install.json). The isolated bundle was restored to the original animated passive baseline after measurement.

Independent source review returned two incomplete changes to OpenCode: retained Settings content lacked semantic invalidations after removing its parent subscription, risking stale tabs and asynchronous results; material cleanup retained retries for busy/root failures but still dropped copy ownership on verifier submission failure. Material removal now captures the original attachment list, root and file identity, but duplicate-path index handling and delayed-worker race tests still require review. Neither task is accepted by this checkpoint. Icon embedding verification reports all 51 referenced Lucide assets embedded. All 25 tasks and the full release matrix remain in scope.

Restoration observation: the animated baseline initially exposed only a Starting screen for about 35 seconds; a later ordinary click produced the loaded research goal page. A one-second process sample showed an idle main event loop, not CPU saturation. This is an unresolved startup/observation discrepancy, not proof of a new production defect; preserve it for startup regression. The restored signed binary SHA256 is `8066b0997ef87bbbc09fe40398ae34d3e88b9297d015ab4b825994d6030dec70`.

## r48: retained Settings native candidate

Independent optimized native measurement copied the frozen animated r44 baseline and added only the retained Settings child, parent observation, initialization and route wiring. Exact source and binary identity are in [source manifest](evidence/r48-retained-source.json) and [installed binary manifest](evidence/r48-retained-install.json). During 13.955 seconds without input, parent/sidebar renders increased by 1,663, logo by 1,664, and Settings by zero. This demonstrates native reuse of the Settings subtree while animation remains enabled; it does not prove frame-time or full-surface scrolling acceptance. See [idle interval](evidence/r48-retained-idle.json) and [counter snapshot](evidence/r48-retained-counts.tsv).

Computer verification on this controlled candidate: Models to General and back updated visibly; Light to Dark and back updated the page; research concurrency increased from 3 to 4 and was restored to 3 with visible saved feedback; scrolling reached Browser runtime and returned to the top; checking the isolated fixture browser changed runtime status from unchecked to ready. Actual provider credentials/models were not probed. Native cached controls intermittently disappear from the tool accessibility snapshot, so screenshot-derived coordinates were used; this does not add VoiceOver scope, but keyboard behavior still needs explicit native verification. The installed isolated candidate remains animated, Light, Agent & models at the top.

The full working-tree retained candidate also independently passed `--qa-keyboard-visual` (exit 0, 12.52 seconds; immutable SHA256 `756c54efe58d4623d3961527284512b4aae5489bdc1ed7b12df97e75a7551433`; invalid flag exits 2). See [QA evidence](evidence/r48-keyboard-visual.json). Forced headless render-counter increases cannot prove semantic invalidation and were returned for stronger asynchronous and visible-outcome checks. Full T19 and release acceptance remain open.

Material review continues: the initial verifier-submit failure now retains copies, but deletion submission/completion and workspace-root transitions also require ownership retention and tests. These precise remaining branches were returned to OpenCode. T14 resumed in its existing session to complete crash-resolution execution seams and full live persistence/history integration rather than stop at L1b foundations.

## r49: native keyboard and cleanup follow-up

The animated r48 native candidate correctly moved keyboard focus from the global model input to the Plan provider with Tab, opened its menu with Enter, closed it with Escape, and continued to the Plan model field. Direct Shift+Tab from the Plan model returned to its provider with a visible focus ring. However, Escape from model suggestions followed by Shift+Tab jumped to the footer New research action. This sequence was returned to T19 for baseline comparison and focus-retention correction; no broad keyboard acceptance is claimed. The isolated model catalog also visibly updated to two fixture models.

Material cleanup review identified an additional multi-file error: Delete completion used any nonmatching result to classify a copy as failed, so multiple successful files could mark each other as failures. Workspace-change completion also discarded retained obligations. OpenCode received the precise branches and required mixed-success multi-file and root-change regressions. Task-edit QA alone does not establish these cases.

Root directly corrected the material retry-drain tail after repeated revision rounds: taking the entire obligations vector and breaking after the first matching workspace silently dropped all remaining entries. The replacement removes only the selected matching entry from the live queue, preserving every other obligation. OpenCode was notified before and after the narrow shared-file edit and asked to add a three-obligation/mixed-workspace regression. A full independent test run is in progress in `evidence/r49-full-tests.log`; the queue fix is not yet accepted without regression evidence.

Independent r49 navigation batch completed: simplify, topics, topic-actions, navigation-context, automation and global-search all exit 0 on immutable `f284d4486779cb8929172b97fe8c9c4502c9ffeb1966cbfed5784911bc0a1e1d`, with invalid-flag guard exit 2. See [batch evidence](evidence/r49-navigation-batch.json). This accepts these regression runs, not full T10/T12 integration or release.

The r49 full test run exits 101: dispatch crash recovery has two failing seams. Restarted failed resolution leaves persisted status Queued instead of Blocked; restarted rerun exceeds the existing 20-second subprocess deadline after repeated preparation of replacement 5000. See [full test results](evidence/r49-full-tests.json) and [log](evidence/r49-full-tests.log). These failures were returned with instructions to preserve real restart coverage and diagnose execution/durability, not weaken assertions or merely extend sleeps.

## r50: live navigation decisions and invalid scrolling workload

Root resumed T12 in its existing OpenCode session with concrete integration decisions: periods remain typed overlays over an explicit base; result selection must remain within the explicit task/period context, and all reader/breadcrumb/history consumers share one resolver; only confirmed saves transition, while pending durability/impact preserves route. Current `displayed_run_id` permits an unrelated latest-run fallback when task context exists, while the reader uses a separate predicate; that mismatch is explicitly assigned for correction with missing-run and cross-domain regressions. Pure controller foundations are insufficient; writable legacy field families must be removed or derived in the final integration.

A native r50 Animation Hitches capture attached to isolated PID 69124 after build processes settled. Computer could still read AX and screenshots, but every attempted scroll/click returned `noWindowsAvailable`, including after rebind and Raise. No gesture succeeded. This is invalid as scrolling evidence, and cannot support a before/after smoothness claim. See [invalid workload record](evidence/r50-workload-invalid.json). Build holds were released; implementation continues. No production app was restarted.

## r51: cleanup ownership correction and crash-recovery diagnosis

The first filtered cleanup-drain test passed, but source review found its new expectation unsafe: an obligation without a root was rebound to whichever workspace happened to be current. Equal path and bytes do not establish workspace ownership. Root directly removed that fallback and changed the test to preserve foreign and unknown-root obligations. The corrected helper test independently passes (exit 0); [evidence](evidence/r51-cleanup-drain-corrected.json). Full worker/cancellation/root-capture coverage remains required and assigned to OpenCode.

Independent inspection located concrete causes behind the r49 crash failures. The restart harness constructs `TaskApp::new_mock`, sets store/last_saved and attaches the admission adapter, but has no synchronous storage. `persist_if_changed` returns false without setting an error when storage is absent; resolution then interprets an empty error as durability and retires markers. The remedy requires an actual worker status write and matching acknowledgement, not a fixture-only save. Admission backpressure also retains only `PendingDispatch { intent, files }`, dropping the reserved replacement run identity; generic retry can allocate a different run. Both mechanisms and mandatory regression cases were returned to T14. Live status alone is not acceptable durable proof.

The r50 capture completed with exit 0, but its workload remains invalid because no scrolling gestures succeeded. Resetting the Computer tool session restored native input; a real downward scroll again reached Browser runtime. This recovered the test connection without restarting the isolated or production app; a fresh measured scrolling run is still needed.

## r52: valid native workload and ordered-resolution review

After resetting the Computer connection, the isolated animated retained-Settings candidate completed eight real scroll gestures. Native screenshots confirmed Browser runtime at the bottom and Global default back at the top. No cargo/rustc processes were observed immediately before recording or after the 30-second limit. The exact gesture timestamps are in [workload evidence](evidence/r52-settings-workload.json). The Animation Hitches recording reached its time limit and is still finalizing; no hitch-count or smoothness conclusion is available yet. The prior r50 workload remains invalid.

Independent review returned two newly introduced persistence problems: a resolution-status group was staged (advancing the journal generation) and then discarded on worker backpressure/not-ready/closed responses, creating a missing generation; mismatched durable receipts removed pending status ownership before validation. OpenCode must retain the exact group and pending identity through retry, validate matching receipts before retirement, and test a subsequent ordinary dispatch to prove the generation sequence remains contiguous.

The live result resolver also needs to preserve explicitly opened archived task history. Global Research results excludes archived work, but this list filter must not blank the explicit archived reader. This regression was returned to T12 with archive-specific tests. The material tampering test still claims application rejection while directly asserting successful metadata application; its scope and a real TaskApp boundary regression were returned again. No full-task or release acceptance is added by this checkpoint.

## r53 independent review: retained Settings and durability retry ownership

The previous turn verified live OpenCode processes and actual tool-call events; no task acceptance was inferred from model availability. All 25 tasks remain in scope.

- Native r52 capture completed successfully with eight confirmed scroll gestures and no Cargo/rustc process observed immediately before or after the recording window. The controlled r48 retained-Settings candidate produced 1,483 app hitch intervals in 30.788 seconds, median 8.333 ms and maximum 25 ms; 171 began within approximate gesture-plus-one-second windows. The previous r42 baseline had 86 intervals, but gesture timing differed, so this is not an exact paired regression measurement. It does prove that lower rebuild counts and lower sampled CPU consumption are insufficient for smoothness acceptance. This candidate remains rejected for performance. [Trace summary](evidence/r52-settings-retained-summary.json), [workload](evidence/r52-settings-workload.json).
- Main-thread statistical CPU weight was 6,548 ms in r52 versus 16,600 ms in r43's baseline analysis. Inclusive layout, paint and sidebar work remain material. These are sampling estimates and overlapping stacks, not frame times. The analysis was sent to the T19 implementer with instructions to preserve functional/focus behavior and provide an immutable optimized candidate for the next native comparison. [CPU summary](evidence/r52-retained-cpu-summary.json).
- T14 now retains rejected status submissions and validates status receipts before removing ownership. Independent source review found another gap: returned/rejected/blocked/failed worker events did not handle status ownership, leaving a submitted status group permanently unretryable. The forced-backpressure test also failed to establish that its initial asynchronous intent had occupied the worker budget. Both findings were returned with required event handling and real ordering assertions; the latest full-suite result remains the failing r49 result until a new independent run supersedes it.
- The local 33-entry gate runner now requires an explicit immutable candidate path and reviewed SHA-256, verifies the copied hash, and requires the fail-closed invalid-QA guard to exit 2 before any suite. Working-tree hashes are labelled as observations, not binary provenance. A complete diagnostic gate was started against the already reviewed r49 candidate (`f284d4486779cb8929172b97fe8c9c4502c9ffeb1966cbfed5784911bc0a1e1d`) using a SQLite backup of the isolated native fixture. Results are pending in `.local/opencode-dispatch/r53-full-gate/index.json`; this older candidate cannot establish acceptance of the evolving current tree.

- Independent navigation unit checkpoint passed: 13 tests, exit 0, including explicit archived-task result ownership. This is focused evidence, not acceptance of the full live typed-route migration. [Result](evidence/r53-navigation-unit.json), [log](evidence/r53-navigation-unit.log).
- T13 independent review found that the delayed PrepareAdd branch verified content identities but used the current workspace for preparation and rejection cleanup without checking the response root. Cancel also rebound an unknown root to the current workspace. Returned requirements: capture and retain original root, reject cross-workspace application, retain unknown-root obligations without deleting anything, and exercise two isolated roots sharing task IDs/relative paths through the actual delayed TaskApp worker boundary.

- A further T14 review found exact rerun identity could still be lost: `retry_dispatch_intent` removed the reserved payload before fallible preparation, and admission preparation failure retained only the generic intent. Returned requirements include preserving the same run/files on every retryable failure, matching reservation ownership to its intent, clearing it only on explicit cancellation/success, and an actual failed-preparation/repair test retaining the reserved identity. The status backpressure test must durably commit a subsequent ordinary admission; checking the journal's next counter alone is insufficient.

- Follow-up material test review rejected immediate cleanup of workspace A while B is active: B's Store cannot establish whether A's file is still referenced. The delayed-switch regression must assert a retained A-root cleanup obligation, then reconnect authoritative A state before safe draining; it must also cover the file becoming referenced in A. The foreign-root guard must not be weakened to satisfy an incorrect test expectation.
- A later navigation edit attempted to include a goal's child reports, but an outer exact-task predicate still excluded all children. This was returned before acceptance, with goal/child, archived goal, unrelated-goal and period ownership cases required. The earlier r53 13-test pass applies only to its compiled checkpoint.

- The newer immutable T19 candidate `48baa703e851f10fe131bb8a13de15b7713f996ebf90fe5755b2c3f5da251f1e` independently passed `--qa-keyboard-visual` (18.012 seconds) and `--qa-settings` (21.418 seconds), after the invalid-QA guard exited 2. This does not supersede the failed native r52 performance result. [Results](evidence/r53-t19-checks.json).
- The r53 diagnostic full gate on the older r49 candidate failed `--qa-attachments` with exit 101: `Timed out: paste shortcut` after the suite's internal 300-second deadline. The material owner received the exact evidence and must rerun the real attachment flow on a current immutable candidate. The full gate continues; later suites are not skipped.

## r54 independent full test checkpoint and development backup

- Created and pushed `feature/review-2026-11-08-optimization`, checkpoint `7504231fdb6b0febf860cbd7407374d91e2ceb8c`; a subsequent remote-head lookup confirmed the exact hash. This is an unfinished-work backup. All subsequent implementation and checkpoint pushes stay on the same branch. Implementers continue after the snapshot, so uncommitted changes are expected.
- Independent `cargo test --features qa --quiet` passed: 646 unit tests, 2 explicitly ignored tests, 6 dispatch crash subprocess tests and 3 persistence crash subprocess tests. Exit 0. This supersedes the r49 full-test failure for the compiled checkpoint, not for later edits or unimplemented live activation. [Summary](evidence/r54-full-tests.json), [log](evidence/r54-full-tests.log).
- The complete r53 33-entry diagnostic gate finished with 32 passed and 1 failed (`--qa-attachments`: paste shortcut timeout). Recovery, simplify, automation, publishing, workspace and all other original entries passed on the fixed r49 binary. Full process exit 1 is preserved; no failing entry was omitted or reclassified as passed. [Full ledger](evidence/r53-full-gate.json); local per-suite logs remain in `.local/opencode-dispatch/r53-full-gate/`.
- Newer T13 candidate `8ad91b082a52aa2108c525f91620cad62499473a8011ecd215af57ab17f8a03b` passed independent `--qa-task-edit` in 7.155 seconds. Its attachment suite was still running at this checkpoint; no attachment acceptance is claimed. [Candidate ledger](evidence/r53-t13-checks.json).
- The material review additionally identified loss of original workspace identity during the earlier import-copy stage: the completion event contains only task ID/results and obtains the current data directory. The implementation must carry the root from import initiation through copy, verification, apply and cleanup, including cancellation, and test switching before copy completes. Optimistic included-material feedback before verified durable apply was also returned.
- T14's prior process completed; the same OpenCode session was resumed for full live tracking, async receipts/startup migration and indexed/lazy history UI. These required tasks remain open despite the corrected passing foundation tests.

## r55 attachment regression diagnosis and screenshot policy

- Screenshot cleanup commit `e60ab38` was pushed on the same development branch. All 60 review screenshots were removed from Git tracking and retained locally; ignore rules prevent future review-image staging. The earlier checkpoint remains in history.
- The newer r53 T13 candidate also failed attachment paste after 300.358 seconds, exit 101. The next implementation added visible impact-confirmation handling to the attachment QA instead of weakening the T21 confirmation contract.
- Root added a one-time pending-state diagnostic after two seconds to the QA wait helper, preserving its predicate and 300-second failure bound. The immutable r55 diagnostic candidate passed all three material imports, then failed in 2.007 seconds at the progress technical-details entry. [Result](evidence/r55-attachments.json).
- Independent diagnosis: `show_task_result` copied the previously selected sibling task into result context even when explicitly opening another task's run. The strict shared resolver then rejected the unrelated context, leaving the requested result absent. Root corrected the destination to use the requested run's owner, preserving an ordinary goal context only when it actually owns the run. Added assertions check the displayed run identity after visiting both active and unstarted sibling tasks. Verification is pending.
- A concurrent import-event signature change interrupted the first root compile; it was not treated as a product-test result. Root captured a byte-consistent source snapshot for the rerun so subsequent agent edits cannot change compilation inputs. [Snapshot manifest](evidence/r55-owner-snapshot.json).
- Previous T12/T13/T14 sessions had terminated with remaining work explicitly open. Fresh OpenCode sessions now have concrete live-route, complete tracking and material/scoped-Undo coding batches. Their smaller context does not reduce the authoritative task scope.

### r55 revised root correction: focused acceptance

- The first frozen correction exposed two additional distinctions: raw `selected_run` QA writes no longer establish a valid task context, and global Results must retain its project/topic/history sidebar instead of becoming a task-only reader. Root corrected the entry semantics and restored QA through the real result command. No existing assertion or timeout was removed.
- `show_task_result` and scoped `select_run` now share requested-run context resolution. With no explicit task context, global browsing remains global. Existing task context is reconciled against the requested run owner, preserving an ordinary goal only for its own reports. Technical-details QA now asserts the displayed run identity, not only the stored selected ID.
- Revised immutable candidate `ace395c87fddbc9e9e711e3b84610348ad812c2b940049e97bf5bb4a24da1ddd`, guard exit 2: `--qa-attachments` passed in 2.106 seconds; `--qa-simplify` passed in 51.803 seconds. The candidate derives from the prior frozen source plus the isolated root fixes, preventing concurrent implementation from changing these test inputs. [Results](evidence/r55-owner-revised-checks.json), [source derivation](evidence/r55-owner-revised-snapshot.json).
- The preceding frozen candidate independently passed archived-research and global-search, but navigation-context failed because the cached footer's New research entry was not visible. That regression was returned to T19, and no full navigation or scrolling acceptance is claimed. The original first-attempt simplify failure was caused by root's unconditional task-context fallback and is superseded by the revised passing simplify result. [First-attempt ledger](evidence/r55-owner-fixed-checks.json).
- `python3 script/check_icons.py` passed with 51 registered icons. No new icon asset was introduced by the root result-entry correction.

## r56 independent persistence completeness regression

Root audited the serialized Store fields against the v4 seed/load and typed metadata mapping. `run_id_floor` exists in Store but is absent from the explicit v4 mappings and `MetadataDelta`. Legacy snapshot storage uses generic serde metadata and is not assumed broken by this finding.

A minimal independent test with `run_id_floor = 50_001` failed at `SqliteV4::create_seeded`: `seeded v4 database did not round-trip its store` (exit 101). This is a migration/seeding blocker for a nonzero reserved floor, not an observed silent successful write. The diagnostic was run on a frozen source with an added test only, preserved separately; original r55 snapshot bytes were restored afterward. [Result](evidence/r56-run-floor-regression.json), [source](evidence/r56-run-floor-source.json).

Returned T14 requirements: persist and restore the scalar, add its ordered metadata delta, retain default compatibility for older files missing the field, test nonzero seeding/migration/restart, and audit every persistent Store field for tracked mutation and complete write/read coverage. This new case does not invalidate the historical r54 passing test execution, but it proves that execution did not cover the newly identified invariant.


## r57 independent scoped-Undo atomicity regression

All four OpenCode implementation sessions were verified running through `session.active`; this is active implementation, not task acceptance. Independent source review found that `scoped_domain::apply_undo` mutates entities before validating settings, and settings restoration validates and writes each field sequentially. A later conflict can therefore return an error after partial rollback.

A frozen diagnostic using the reviewed module reproduced this defect: change default model and OpenCode path, capture Undo, change the path again, then invoke Undo. Undo rejects the conflicting path but has already reset the default model. The independent test exits 101 (0 passed, 1 failed). Two compile-only adaptations correct the module import and mutable guard binding; they do not change the inverse algorithm. [Regression result](evidence/r57-undo-regression.json), [source derivation](evidence/r57-undo-source.json).

Returned requirements: validate every entity and settings precondition before any mutation, test mixed entity/settings conflicts with unchanged state on rejection, and replace whole-Store staging/diff derivation with command-targeted preimages. Current `apply_domain_change_inner` still clones the Store and computes repeated linear searches despite its comment claiming no whole-Store snapshot; this is not bounded scoped-command completion. Existing newly added conflict tests also require corrected action/later-mutation boundaries.

The tracking review returned a separate enforcement gap: public tracked Store fields and crate-visible clean hydration permit replacing tracked values with clean wrappers, bypassing dirty recording. Final mutation APIs must prevent ordinary command paths from bypassing tracking; wrapping public fields alone is insufficient.

Computer rechecked the isolated r48 native baseline: opening an automation period step, Back, and closing the restored result dialog returned to Automation results. This is baseline route evidence only, not current typed-route integration acceptance or scrolling evidence. Codex usage was independently checked at 90% used (10% remaining); no banked reset was consumed.

The same frozen scoped-domain module suite completed with 2 passed and 3 failed: the independent partial-rollback regression, a conflict test missing a mutation after capture, and a topic fixture passing a topic ID as a parent-task ID. All three outcomes were returned without removing assertions. [Suite result](evidence/r57-scoped-domain-tests.json). T14 subsequently made persistent Store fields private in the active source; getter/mutation migration and independent verification are still in progress.


## r58 retained-footer sizing diagnosis and focused verification

Root inspected the pinned gpui-fast implementation: a cached view's style belongs to a separate layout node, while the rendered element is laid out as a root inside the assigned bounds. The footer rendered row lacked an explicit full-width constraint. A frozen r55 revised source with only `w_full()` and `min_w_0()` added to that row independently passed navigation-context (16.356 seconds) and Settings (18.086 seconds). The fail-closed guard exited 2. Candidate SHA-256: `d5542205531053aeb62cf724c000db02e83e6fa82361f038621f911e19c335f4`. [Results](evidence/r58-footer-checks.json), [source](evidence/r58-footer-source.json). OpenCode incorporated these two constraints into the active source.

Keyboard/visual failed at the older footer test's expectation `disabled() == Some(false)`, observing `None`. The toolkit explicitly states that an absent flag does not prove interactivity, so this failure was not silently reclassified as success. Root prepared a stronger frozen follow-up that performs a real scoped concurrency change, clicks Undo, verifies the restored value and verifies the command becomes disabled again. Its build/test result is pending. The implementer received the same behavioral acceptance requirement.

The new scoped-command source now validates all settings before inverse mutation, but independent acceptance awaits a compilable integrated checkpoint. Additional preflight requirements were returned for invalid model-stage indices and archive-owner chains. T14's private fields now expose clean collection loaders, so loading must remain restricted to construction or reject live/pending state; a public clean replacement method would recreate the mutation-tracking bypass. No task or scrolling acceptance is added by these source observations.

The stronger frozen keyboard/visual follow-up passed in 16.572 seconds, exit 0, guard exit 2. It performs a real concurrency edit and clicks the cached Undo, verifies the original value is restored, then verifies Undo is disabled. Candidate SHA-256 `a174942b48adbfdf1fb9fc4d06c4437cca0afaabc3e5809d136dfae1838bc8ac`. This resolves the specific footer test on the frozen candidate without treating missing attributes as proof. [Result](evidence/r58-footer-verified-checks.json), [source](evidence/r58-footer-verified-source.json). Current-tree integration and native scrolling remain open.


## r59 optimized native diagnostic candidate

Checkpoint `092e35e` was pushed successfully to the same development branch. It is an unfinished-work backup; later implementation remains active and validation screenshots remain local.

Root built an optimized frozen diagnostic from the r58 verified source with only the isolated native bootstrap override. The invalid-QA guard exited 2; navigation-context passed in 14.403 seconds and keyboard/visual passed in 11.416 seconds. The signed candidate replaced only the isolated Planning Native Review app; the production app was not restarted. Native Computer confirmed Settings opens, Agent & models renders and scroll inputs operate, with the corrected footer remaining visible. This candidate is not the actively migrating current tree. [Build source](evidence/r59-native-source.json), [checks](evidence/r59-native-checks.json), [installation](evidence/r59-native-install.json).

The first Animation Hitches recording is invalid: build processes remained at preflight, but the shell continued to recording after the failed check. Root stopped it and excluded it from acceptance; zero measured gestures were performed. The recording is still finalizing at this checkpoint. A replacement orchestration script now starts recording inside the same Python control flow only after three clean build-process checks, and records build-process observations throughout capture. The short build hold was released so implementation can continue until clean capture is ready. [Invalid-record ledger](evidence/r59-trace-invalid.json). No new scrolling-performance acceptance is claimed.

For interpretation, Apple defines hitches as missed display deadlines, distinct from long main-thread hangs; a single refresh interval can matter. Reduced rebuild counts or average CPU therefore remain insufficient. [Apple responsiveness documentation](https://developer.apple.com/documentation/xcode/understanding-user-interface-responsiveness), [render-loop explanation](https://developer.apple.com/videos/play/tech-talks/10855/).

T13/T14 integration review also caught broad Store API substitutions applied to non-Store values. File ownership was clarified: T13 owns scoped-domain command adaptations, while T14 maintains the private Store API and its remaining callers. Rejection tests must compare pending typed deltas as well as Store values because value equality omits dirty tracking metadata.


## r60 native workload isolation and command-boundary review

The first reattempt completed eight native gestures but continuous process monitoring detected Cargo/rustc starting after clean preflight, overlapping the recording window. That trace remains contaminated and is excluded from smoothness acceptance. [Contaminated workload](evidence/r60-settings-workload.json). Steer-only build holds were insufficient because already-running tool batches continued launching compilation.

Root deliberately interrupted the three still-running OpenCode sessions through `session.interrupt`; the T19 session was already terminal. `session.active` then returned an empty map, and no Cargo/rustc process remained. This was a short measurement suspension, not a restart caused by a timeout or lack of progress. A separate recording completed eight confirmed native scroll gestures. Monitor observations through the end-of-recording notification contained no build processes. Trace finalization and exact timestamp coverage/hitch analysis remain pending. [Isolated workload](evidence/r60-clean-settings-workload.json). All four original sessions were then resumed with the same requested model and current edits preserved; authoritative status confirmed all four running again.

Source review found remaining whole-Store staging inside `Store::move_topic_project`, even after its outer command stopped cloning. This was returned for bounded affected-entity preparation. Run inverses must hold only the context metadata actually changed, not entire runtime records/report bodies. Current clean collection loaders have been restricted to the domain module, and archive restore now preflights its owner chain; neither source observation is full live persistence acceptance. The icon embedding check passed with 51 registered icons and `git diff --check` passed. Codex usage was verified at 93% used (7% remaining); no reset was consumed.


### r60 independent results

The isolated recording finished successfully. Its actual timestamp range contains seven of the eight executed scroll gestures; the final upward return happened after recording and is excluded from trace attribution. All 29 build-process observations inside the 30.767489-second capture were clean. The app recorded 41 hitch intervals overall; three began within recorded gesture-plus-one-second windows, including a 25.000166 ms hitch. This candidate remains rejected for scrolling smoothness. Different gesture timing prevents an exact paired comparison to r52. [Trace summary](evidence/r60-settings-trace-summary.json).

Main-thread statistical sample weight was 12,189 ms. Inclusive layout (6,245 ms), paint (5,864 ms), flexbox (2,030 ms) and sidebar (1,192 ms) remain material, while Settings frames account for only 3 ms. These groups overlap and are sampling estimates, not measured frame durations. Displayed-surface intervals have median 8.333333 ms, p95 8.333375 ms, maximum 33.333333 ms, with 30 above 12 ms; they are not unique app render frames. The exact hitch window and exports were returned to T19. [CPU analysis](evidence/r60-cpu-summary.json), [surface intervals](evidence/r60-surfaces-summary.json).

A new byte-consistent integrated source snapshot independently passed all seven scoped-domain tests, including mixed settings/entity conflict preflight. Two reserved-run-floor tests passed, covering JSON persistence and v4 nonzero seed/reopen, incremental durable metadata and the missing-field compatibility default. This resolves the specific r56 backend regression for that compiled snapshot. [Snapshot](evidence/r60-integrated-source.json), [scoped-domain result](evidence/r60-scoped-domain.json), [run-floor result](evidence/r60-run-floor.json).

The full unit run failed with 658 passed, 2 failed and 2 ignored. One atomic-drain assertion expects the earlier collection order; the new drain returns the same entities in a different sequence. The all-fields tracking fixture sets automation concurrency and appearance to their existing defaults, then incorrectly expects change deltas. Both failures were returned with instructions to preserve exact membership/atomicity and no-op setter semantics. The full command stopped before integration tests; separate crash-subprocess verification is running and is not yet accepted. [Full result](evidence/r60-full-tests.json).

The explicit frozen integration run passed all 8 dispatch crash subprocess tests and all 3 v4 crash subprocess tests (exit 0). The two unit failures above remain open; this does not convert the full suite to green. [Crash results](evidence/r60-crash-tests.json).


## r61 independent integrated verification and acknowledgement audit

The previous turn verified the requested OpenCode provider/model but did not advance implementation acceptance. Root revalidated all four implementation sessions through `session.active`, then captured a byte-consistent 372-file source snapshot. The full independent `cargo test --features qa --quiet` completed with 663 unit tests passed, 2 ignored, 8 dispatch crash tests passed and 3 v4 crash tests passed (exit 0). The two r60 fixture failures are resolved in this snapshot without removing the exact drain-membership assertion or changing no-op setter semantics. [Source](evidence/r61-integrated-source.json), [full tests](evidence/r61-full-tests.json).

The separate immutable OpenCode candidate `78934b1b19f82f9f975204574129d91ba7215458a7d6ebe3d55ae28a92c1792c` independently passed the invalid-QA guard (exit 2), navigation-context (21.493 seconds), Settings (16.417 seconds) and keyboard/visual (16.591 seconds). These are semantic checks; source-to-binary provenance remains to be reconciled and they do not establish native scrolling smoothness. [Candidate results](evidence/r61-t19-checks.json).

Root returned a main-thread overhead issue: `persist` generated tracked payloads before its 500 ms throttle check. OpenCode moved the throttle check ahead of payload generation in the live source; the r61 frozen test result predates that edit. Synchronous storage writes and whole-Store saved copies still require the planned live asynchronous migration. T12 selection/overlay/page-state/origin integration, T13 full command coverage, T14/T15 live persistence and indexed history, and the complete native performance matrix remain open.

A new boundary audit found scalar/list dirty epochs reset to zero after acknowledgement, unlike entity counters. A later edit can reuse an older receipt's summed epoch. Root added an independent stale scalar receipt regression in a separate diagnostic source. It failed with exit 101: `a stale receipt cleared a newer scalar generation` (0 passed, 1 failed). The sequence acknowledges an earlier scalar edit, makes a new scalar edit, then reapplies the earlier receipt; the newer pending flag is incorrectly cleared. This is a reproduced acknowledgement-boundary defect, not a claim of observed production data loss. Returned requirements include monotonic generations or receipt identities and scalar/list duplicate/out-of-order acknowledgement tests before asynchronous activation. [Diagnostic derivation](evidence/r61-epoch-regression-source.json), [regression result](evidence/r61-epoch-regression.json).

Codex usage was verified at 95% used (5% remaining). No banked reset was consumed. No screenshots or raw traces were added to Git. All four implementation sessions remained running at the latest authoritative status check.


## r62 stale-receipt fix independently verified

Root retained the exact r61 failing regression and replaced only `domain_tracked.rs` with OpenCode's fix in a separate frozen diagnostic source. Scalar and list acknowledgements no longer reset their generation counters. The full test command passed, including the unchanged independent stale-receipt regression, all 8 dispatch crash tests and all 3 v4 crash tests (exit 0). This establishes the specific red-to-green fix without attributing unrelated current-tree edits to the tested candidate. [Derivation](evidence/r62-epoch-fixed-source.json), [test result](evidence/r62-epoch-fixed-tests.json).

Current-tree OpenCode work separately adds scalar/list duplicate and out-of-order acknowledgement fixtures and begins retained-sidebar integration. Neither the diagnostic run nor source inspection establishes final live asynchronous persistence or native smoothness acceptance. Root returned two further review requirements: characterize the topic-move signature-failure rollback's pending tracking state, and verify pointer hit-testing, sidebar scroll/resize and semantic invalidation around the animated-logo overlay. These are source-review concerns, not newly reproduced production failures. All four implementation sessions were independently confirmed running. The icon embedding check passed with 51 registered icons.


## r63 sidebar isolation and release-gate coverage reconciliation

Root captured the current integrated source, but its build failed with two unresolved names: the new sidebar's `col` import and a topic-move cycle helper not yet present during the implementation edit. These were returned to their owners. The cycle helper appeared in the live source afterward; the compiler's unrelated-method suggestion is not an accepted fix. Root also returned the release-mode signature-loop `break` followed by success: cycle detection alone must not silently accept incomplete provenance refresh. [Integrated source](evidence/r63-integrated-source.json), [failed build](evidence/r63-build.json).

A first attempt to isolate the sidebar copied concurrent T12 getter migration in `views.rs`, causing 12 missing-method errors against the older baseline. This is a diagnostic composition failure, not a new product regression. Root preserved that evidence and created a corrected derivation from the r61 baseline with only the sidebar field/initializer, retained sidebar implementation and three sidebar view substitutions. Its build passed and its invalid-QA guard exited 2. Candidate SHA-256: `44afa43487e8537d93327c6ec07f212e7190ddf0f60af606a98d87019d2f02c8`. Sidebar (1.2 seconds), navigation-context (14.205 seconds) and keyboard/visual (16.496 seconds) all passed, exit 0. Native smoothness and complete pointer/resize/overlay verification remain open. [First derivation](evidence/r63-sidebar-source.json), [first build](evidence/r63-sidebar-build.json), [corrected derivation](evidence/r63-sidebar-revised-source.json), [corrected build](evidence/r63-sidebar-revised-build.json), [checks](evidence/r63-sidebar-checks.json).

A release-scope audit found 48 direct QA flags in current `main.rs`, compared with the 33-entry historical gate. Of these, 46 are standalone suites and 2 are crash children requiring their integration-test parents. Six additional persistence measurement modes are parsed separately. T01 now requires the complete current inventory, while retaining the historical 33 entries and requiring final-source reconciliation. The inventory records pending acceptance, not successful execution. Live-network modes remain separate from offline gates; native scrolling remains a separate requirement. [Current inventory](evidence/r63-current-qa-inventory.json).

Root explicitly returned T14 to the missing live integration: worker ownership/startup, nonblocking typed submission, durable/failed/retry UI, generation-safe acknowledgements, dispatch gating and removal of main-thread whole-Store saved copies. Worker foundations alone are not completion. All four OpenCode implementation sessions were confirmed running during this round.


## r64 migration preflight review and executable gate inventory

Root updated the independent local release runner to require the candidate's frozen source root, derive standalone QA flags from that source, retain all historical 33 entries, and include the six separately parsed persistence measurement modes. A syntax/inventory preflight confirms 46 standalone suites and 6 performance modes; the two crash-child flags remain explicitly covered by their parent integration tests. The maintained runner is `script/run_review_gate.py`; its help command and Python syntax were verified. A negative hash-preflight test rejected the candidate before copying or executing it. This is harness preflight only, not execution or final release acceptance. [Preflight](evidence/r64-gate-preflight.json).

The topic-move rewrite now prepares signatures before mutation, but source inspection found its initial visiting list already contains the task that the helper immediately rejects, and its approval check compares the old approved signature with the prospective destination signature. Both were returned as valid-migration regressions requiring completed, approved multi-step coverage. Root also found `capture_runs` still clones complete `AgentRun` records before reducing to narrow Undo deltas; the bounded preimage must be narrow at capture time to avoid copying report bodies on the UI thread.

A first diagnostic composition pulled concurrent persistence API migration and failed compilation: two old baseline calls to the removed staged-save clearing method, plus a new policy-key mismatch (`u8` versus `Stage`). The former is a composition incompatibility; the latter was returned to T14 for checked decoding. Root preserved this result and built a narrower derivation containing only topic-move methods/helpers and scoped commands. Its tests compiled but finished with 8 passed and 2 failed in fixture setup: the plan was changed after approval, so preparing a child run was correctly rejected. Root made a fixture-only follow-up approving the completed plan; that run again had 8 passed and 2 failed, now because News collection lacks agent/model configuration. Neither run reached the new migration assertions. Both fixture failures were returned without relaxing assertions; the source-level migration concerns remain unverified by execution. [Narrow result](evidence/r64-command-revised-tests.json), [fixture-only source](evidence/r64-command-fixture-source.json), [fixture-only result](evidence/r64-command-fixture-tests.json). [First source](evidence/r64-command-source.json), [first result](evidence/r64-command-tests.json), [narrow source](evidence/r64-command-revised-source.json).

All four implementation sessions were verified running. Codex usage was 96% used (4% remaining); no banked reset was consumed.


## r65 topic-move checks passed; live-save alternative rejected

Root froze the corrected topic-move methods/helpers and scoped-command module against the known-good r62 base, excluding concurrent persistence/navigation migration. All 10 scoped-domain tests passed (exit 0), including the completed approved chain's exact prospective signatures and malformed-cycle rejection preserving Store values and pending deltas. The two earlier fixture failures are resolved in this derivation. Crash integration binaries were filtered out by the test selector, so this is not a new crash-test run. Run preimages now capture only identity, input signature and project-plan metadata instead of cloning complete `AgentRun` bodies. Full current-tree migration/Undo integration remains required. [Source](evidence/r65-command-source.json), [tests](evidence/r65-command-tests.json).

Root rejected the new `LiveSaveWorker` as the final T14/T15 direction: it duplicates the existing bounded worker with unbounded channels, clones the full replica and calls legacy `storage.save` per group, waits synchronously for command receipts, joins on shutdown, and resets pending count even when requests may remain in flight. Ordinary `persist_if_changed` still saves synchronously. OpenCode was instructed to wire the existing ordered incremental worker with bounded submissions, asynchronous command continuations, workspace/generation guards and correct pending/durable/failed presentation. A parallel compatibility adapter is not completion of the agreed architecture.

Root prepared an optimized native diagnostic source from the installed r59 baseline with only retained-sidebar changes. The isolated bootstrap path is preserved. The release build is running; no application has been replaced or restarted in this round. This derivation enables a closer sidebar-only comparison while current-tree migration continues. [Native source](evidence/r65-native-source.json).


## r66 optimized sidebar candidate prepared; native interaction awaits unlock

The optimized r65 sidebar-only build completed successfully in 53.898 seconds. Its invalid-QA guard exited 2; independent navigation-context passed in 11.299 seconds and keyboard/visual passed in 12.204 seconds. Unsigned candidate SHA-256: `ef805d6f9d28708875b62e6269f5c3f93cf2479f3dd00b4b9d843cc79c0203ac`. Root verified the exact old isolated executable and process identity before stopping PID 12194, kept its signed backup, replaced only Planning Native Review, signed it, and reran the guard successfully. Signed SHA-256: `3899b2c6efc71ca8faf016f654b6776f88d5db6aa86cb773d0634316267b08bf`. The isolated bootstrap is unchanged. [Build](evidence/r65-native-build.json), [checks](evidence/r66-native-checks.json), [installation](evidence/r66-native-install.json).

Computer reported that the Mac is locked and automatic unlock failed. Root requested manual unlock; no native click, scroll, resize or new hitch capture has been completed for this installed candidate. No native acceptance is claimed. The UI handoff remains pending; source review and offline work can continue.

T14's session became terminal after removing the rejected parallel worker and reporting that live activation remains absent. Root verified its terminal outcome and explicitly resumed the same session for full startup migration, existing-worker activation, asynchronous command continuations and indexed history/body loading. All four sessions were then confirmed running. The reported foundation work is not blanket T14/T15 acceptance.

Typed task/result selection is now projected from routes in the active source. Root returned remaining S4–S6 concerns: `install_route` must not silently discard origin/page/overlay state via a new navigation controller after a failed replacement; result ownership must use actual Goal/child relationships rather than infer Goal solely from mismatched task IDs. Final route and restoration acceptance remains open. Checkpoint `6fe95f1` was successfully pushed to the same development branch; screenshots and traces remain local.


## r67 integrated compile correction and live-receipt boundary audit

Root froze 402 files from the current integrated source. The full test command stopped at compilation: `Stateful<Div>.map(observed)` is not a valid builder call in the pinned toolkit. Root corrected the live sidebar overlay to wrap its complete element in `observed(...)`, preserving its child and test identity. The failed full run remains recorded; no integrated test success is inferred from the correction. [Source](evidence/r67-integrated-source.json), [failed full run](evidence/r67-full-tests.json).

The live-save admission seam handles ordinary-save receipts before the existing dispatch/status workspace checks. Root added an independent same-generation foreign-workspace receipt regression to a frozen diagnostic, with only the sidebar compile correction in addition. It failed with exit 101 (0 passed, 1 failed): `foreign workspace receipt was accepted as a durable live save`. This reproduces the receipt-validation defect through the adapter boundary. [Regression result](evidence/r67-live-receipt-tests.json). Root also returned a source concern: Returned/Rejected live groups remove their generation-to-epoch mapping, but accepted resubmission does not restore it, so a later receipt may lose its ordinary-save identity. Required checks include exact-intent retry, workspace/effect validation before retirement, backpressure and durable acknowledgement. These are acceptance boundaries for the unfinished live integration, not claims of observed user-data loss. [Diagnostic source](evidence/r67-live-receipt-source.json).

The native manual-unlock handoff remains pending. All four OpenCode sessions were revalidated as running; independent source and offline verification continue without reducing the native acceptance requirement.


## r68 acknowledgement fix and current integration verification

Root froze 407 files after the sidebar compile correction and the new live-receipt classifier. Full integrated tests completed with 673 unit tests passed and 2 ignored, then 7 dispatch crash tests passed and 1 failed: `restart_crash_after_resolution_status_and_claim_release_recovers` expected the launch-claim key to remain after the status crash. The command exited 101 before v4 crash tests. [Full result](evidence/r68-full-tests.json). The classifier now checks workspace and empty expected effects before retiring the ordinary-save generation. Root prepared the exact unchanged r67 foreign-workspace regression against the updated adapter in a separate frozen derivation; the unchanged independent regression passed (exit 0, 1 passed), proving the specific foreign-workspace rejection fix for this derivation. [Receipt result](evidence/r68-live-receipt-tests.json). [Integrated source](evidence/r68-integrated-source.json), [receipt derivation](evidence/r68-live-receipt-source.json).

The startup selector remains uncalled outside its own tests at this observation. Both selection paths load and discard the complete Store. Root returned requirements to avoid repeated whole-history/body hydration in normal startup, perform expensive migration/integrity work off the UI thread under the workspace lock, and activate the existing bounded worker with coherent workspace incarnation/data. This remains incomplete T14/T15 integration.

Codex usage was verified at 98% used, exactly 2% remaining. The user's reset threshold is strictly below 2%, so no reset was consumed. Native interaction remains awaiting the manual-unlock handoff; independent code verification continues.


The failed crash test passed when rerun alone with `--exact --test-threads=1` (2.396 seconds), leaving the earlier failure unresolved. Root inspected the retained failed fixture: only the resolution marker remained, with the launch claim absent. Two older leftover failed fixtures had the same marker shape. These observations were returned to T14 for crash-boundary/claim-retirement timing analysis; no assertions were removed and no full-suite success is claimed. [Focused rerun](evidence/r68-crash-focused.json).


## r69 crash-ordering fix and reset threshold

Source review identified the crash-test race: the bounded poll could release the launch claim while resolution status was still pending. The cleanup sweep could also classify the old attempt's terminal-looking saved status without respecting the pending resolution marker. OpenCode now removes the premature release from that poll and skips resolving keys in the generic cleanup sweep. Root froze only `command_app.rs` over r68 and independently passed all 8 dispatch crash subprocess tests (exit 0). Three repeated concurrent-suite runs completed: the first failed with 7 passed and 1 timeout (`resolution_failed_intent_survives_crash_before_execution`, child `restart-failed` exceeded the existing 20-second bound); the next two passed all 8 tests. Stable acceptance remains rejected. The timeout was returned without increasing waits or weakening assertions. [Repeated results](evidence/r69-crash-repeat.json). [Source](evidence/r69-crash-source.json), [first result](evidence/r69-crash-tests.json).

The fix still requires explicit release-failure/backpressure coverage: removing unconditional retry must not leave a durably resolved attempt permanently pending when the claim-release enqueue fails. Root returned this requirement with receipt-gated continuation/retry expectations; a green first crash run is not full T14 acceptance.

Computer again reported the Mac locked. Codex usage is now 99% used (1% remaining), meeting the user's strictly-below-2% reset condition. The exact authorized Dec 1 03:56 GMT+8 credit remains available and unused. No consumption action is exposed by available tools, and native UI access is blocked by the lock; root recorded eligibility without claiming consumption or using the other credit. The OpenAI Docs skill was consulted, but official documentation did not establish an alternative banked-reset execution API. Source/testing work continues while manual unlock remains pending.


## r70 retained timeout state and retirement-phase review

Root read the retained r69 timeout fixture without changing it: run 4000 is durably `Blocked`, while both its launch claim and failed-resolution marker remain. This narrows the stalled state but does not by itself prove which transition failed. [Fixture observation](evidence/r70-timeout-fixture.json).

The live retry addition introduces `dispatch_status_durable`, but source review found remaining obligation gaps: the launch-release handler clears that obligation before `finish_resolution`, which can ignore a failed decision-marker removal enqueue; the no-claim branch clears it after enqueue success rather than durable removal. Root returned explicit retirement phases and in-flight deduplication requirements, with injected enqueue-rejection and asynchronous-removal-failure tests through `ResolutionReleased`. No acceptance was added.

The crash parent and child both use a 20-second bound, allowing the parent to kill the child before its terminal diagnostic is emitted. Root requested one early bounded diagnostic snapshot of the pending resolution/receipt phase and adapter state, preserving the existing timeout and assertions. All four OpenCode sessions remain running; `git diff --check` passed. Native unlock and the eligible but unused reset remain pending.


## r71 integrated navigation gate stopped before launch

Root froze 416 current files for independent navigation-context, global-search and sidebar QA. Compilation failed before any candidate was copied or launched: the new retirement struct was missing an import, the retirement retry method held a mutable adapter borrow across `finish_resolution`, and the new sidebar fixture still accessed `scope` as a field rather than calling `scope()`. These were returned to T14/T19 with precise locations. No stale candidate was substituted and no QA pass is claimed. [Source](evidence/r71-integrated-source.json), [gate result](evidence/r71-navigation-checks.json).

OpenCode's sidebar fixture caught brand/scroll alignment drift. Root required geometry checks against the actual identity row, including titlebar clipping and nonzero scroll offsets, rather than merely sharing duplicated position constants between implementation and tests. The installed r65 native diagnostic predates this newer overlay entity/alignment change and is not a final candidate. All four implementation sessions remain running; native unlock and the authorized unused reset remain pending.


## r72 integrated recovery and navigation verification

Root independently tested the frozen 418-file r72 source: 673 unit tests passed, 2 ignored, all 8 dispatch crash tests and all 3 v4 crash tests passed. The build succeeded; the immutable candidate SHA-256 is `bbb80446cd93638abeb3e199741b9f22e81298779c7ef0f0b3270165767c01e5`. Invalid QA guard exited 2 before navigation-context, global-search and sidebar checks each passed. [Results](evidence/r72-checks.json), [source](evidence/r72-integrated-source.json). These checks do not prove the full native or release matrix.

Root returned an additional rerun recovery concern: `retry_status_completion` marks `decision_started` before `finish_resolution`, although preparation, material preparation or admission can fail. Existing-replacement marker removal also ignores enqueue errors. T14 was verified terminal before resuming the same OpenCode session with retry-phase and injected-failure requirements, plus the still-required full live persistence/startup integration. The full standalone QA and persistence performance ledger is now running against the immutable r72 candidate in an isolated copied fixture. Native interaction remains unverified pending the manual-unlock handoff.


## r74 full-gate progress and command scaling review

The independent r72 candidate passed the base, affordance, archived-research and attachment QA entries; the full ledger remains running. [Progress only](evidence/r74-gate-progress.json). T13 reported completion with scoped field inverses and nine agent-run QA checks; root has not promoted this report to full acceptance. Root found repeated full-run-vector lookups in `capture_runs` and `changed_runs`, giving O(K*N) work for K affected runs among N historical runs. T13 was verified terminal, then resumed with single-pass lookup and actual large-history move/Undo measurement requirements, preserving approved result validity and runtime history.

Computer again confirmed that the Mac is locked. The existing manual-unlock handoff remains pending; native scrolling acceptance and reset consumption have not occurred. No blocked or completed goal status is inferred while implementation and offline verification continue.


## r75 independent live-save latency rejection

The frozen r72 candidate passed the automation functional QA, but its 43,200-occurrence performance fixture measured a **1,336 ms synchronous UI-triggered save block**, with 3,559 ms cold opening, 1,582 ms cold save, 1,243/1,249/1,281 ms warm saves, 657 ms load and 172.4 MB database. The process exits zero without enforcing a responsiveness threshold. Root explicitly rejects performance acceptance; a single measurement is not a p95 distribution. [Measured evidence](evidence/r75-live-save-performance.json). T14 received the actual application-path evidence and requirements for bounded asynchronous saves, durability/error acknowledgement and main-thread distribution checks.

Root also found that the benchmark selects a legacy automation ID instead of entering the typed detail route; its claimed detail frame is not adequate detail-page evidence. T12 received this correction together with an overlay atomicity concern: planning-attempt opening removes existing overlays before validating the new run, while current ownership validation only checks run existence. Required regressions cover missing/foreign runs preserving prior route, overlay and drafts. All four OpenCode implementation sessions were observed running; full offline QA remains in progress.


The full r72 offline ledger finished: 51 of 52 process invocations exited zero; `--qa-topic-migration` exited 101 at `topic_qa.rs:574`, where valid project rename failed to close its modal. The failure was returned to T13. [Full ledger](evidence/r75-full-gate.json). Zero process exits remain insufficient for performance acceptance as documented above.

After the user manually unlocked the Mac, Computer accessed the isolated r65 native candidate. It remained visibly on `Starting…` for over 20 seconds; a two-second sample showed the main thread idle in its event loop. A native window zoom immediately exposed All research, suggesting a repaint/invalidation problem. This is evidence against installed r65, not a claim about the newer source. T19 received the observation and current-candidate regression requirement. Native interaction access is restored; the full native matrix remains outstanding. The updated user goal removes the earlier reset clause; no reset was consumed and no reset action is being pursued.


## r76 continuation beyond partial implementation checkpoints

Root verified T12 and T14 sessions terminal before resuming those same sessions. T12 reports S2/S3 and automation-overlay S4 completed, including atomic planning-attempt ownership checks; editor/search/composer S4, page-state S5 and live return-frame S6 remain required. T14 explicitly confirms that ordinary synchronous saves, startup activation, durable command continuations, full snapshot retirement and indexed/lazy UI reads remain incomplete. Both sessions were resumed with the full remaining scope, rather than accepting the partial checkpoint as task completion.

The current icon embedding checker passed with 51 registered icons, and `git diff --check` passed. A same-branch WIP backup preserves current source and evidence; it is not release acceptance. Screenshots, raw traces and logs remain local.


## r77 independent committed-source verification

Root independently ran `cargo test --features qa --quiet` against an exact archive of pushed commit `ed40899`; the command exited zero in 53.661 seconds. [Result](evidence/r76-full-tests.json). This does not replace native/performance acceptance. The topic-migration failure was traced to a fixture with no attached storage, while the scoped rename command correctly requires durable storage. Root prepared an independent derivation of the same commit with only the updated `topic_qa.rs` fixture, retaining existing cancel, invalid-input and valid-save assertions; its guarded QA run is in progress.

T14 again ended with a dependency explanation rather than implementing the known command/form continuation prerequisite. Root verified its terminal state and resumed the same session with an explicit code-first prerequisite assignment. T19 was asked to provide an exact-source optimized isolated candidate after its repaint/geometry regression work; installation and native acceptance remain root-owned.


## r78 topic fixture accepted narrowly; live continuation rejected

Root's exact `ed40899` derivation with only the real-workspace topic fixture built successfully. Its immutable candidate passed the invalid-QA guard (exit 2), then `--qa-topic-migration` exited zero with existing cancel, invalid-name and valid-save assertions retained. This resolves the specific fixture regression; it does not prove the full current-source release matrix. [Independent result](evidence/r77-topic-fixture.json).

The new live command implementation submits a prepared delta, then reprepares against potentially changed state on its durable receipt and applies through `NoopSink`. Root rejected this design before activation: the recomputed write set and effects may differ from what was committed. Required correction is exact acknowledged-write-set application with field/epoch conflict handling, or a new generation for changed data before effects. Single pending-command correlation must not be overwritten, and callback errors must not be immediately cleared by the enclosing receipt handler. Concurrent edits, second commands and changed preparation require explicit tests.

T19's startup regression manually calls pump ticks and renders frames; root requested real scheduled startup completion without user events in addition. Manual method invocation alone cannot demonstrate the native repaint defect is fixed.


## r79 automation recheck and meaningful performance gating

Root independently ran full automation QA against the frozen `ed40899` plus topic-fixture-only candidate. The SHA-verified binary passed the invalid guard, and automation QA exited zero in 42.412 seconds. [Result](evidence/r79-automation.json). The earlier T13 candidate's legacy planning-overlay failure is not reproduced on this newer integrated derivation; T12 was informed to preserve the working contract.

Root reviewed the command scaling fixture: its 43,200 records are unrelated to the moved topic, leaving the affected write set small. T13 received requirements for representative large affected move/Undo sets and indexed inverse validation/writes, while retaining unrelated runtime preservation.

Root strengthened `script/run_review_gate.py`: a measured synchronous UI save block of at least 50 ms is now `performance_rejected` even if the executable exits zero. Missing/changed timing output requires measurement review. The real 1,336 ms log was replayed through the actual classification code and rejected; script syntax passed. A below-threshold single sample still does not prove p95 responsiveness or native scrolling acceptance.


## r80 persistence ownership handoff after terminal checkpoint

Root verified the previous T14 session terminal. Its code still leaves live activation disabled, form saves reporting success on enqueue, and whole-Store prepared-command comparisons/copies in the prospective live path. These are incomplete requirements, not acceptable performance tradeoffs. Root handed the existing work to a fresh OpenCode session using the same required model and a compact local handoff, covering durable form/command continuations, one bounded writer, startup activation and actual indexed/lazy UI reads. No parallel T14 owner remains active.

T19 received the precise compile issue in its asynchronous startup regression: the `cx` inside a window-update closure is `App`, not the headless context/executor. Scheduled progress must be driven outside that borrow with supported APIs. Current source is not yet accepted as buildable; prior frozen candidates remain separately identified.


## r81 stronger Undo invariants and index-lifetime review

Root prepared an isolated derivation of the previously passing r77 candidate, strengthening every Undo site in the automation regression with full automation/occurrence/run equality and identity-floor checks, both in memory and after reading the saved database. Its build and guarded automation run are in progress; no pass is claimed yet.

The new indexed Undo implementation received another source-review finding: removing a created topic/project shifts vector positions before later deltas use the original index map. Required correction is to defer removals or otherwise preserve stable lookup identities, with a mixed removal/edit regression. Root also required skipping history-index construction for empty run deltas so ordinary title Undo does not scan unrelated history.


## r82 T04 frozen-candidate reacceptance

Root's r81 independent derivation built, passed the invalid-QA guard, and passed the full automation QA with strengthened assertions at five Undo sites. Complete automation definitions, periods/dedupe keys, runtime history and identity floors match both in memory and after reading the database. [Evidence](evidence/r81-undo.json). T04 is reaccepted specifically for this frozen candidate; the ongoing indexed Undo and live persistence work must rerun the same assertions before integrated acceptance. T13 received the exact diagnostic source and instructions to integrate the assertions without overwriting unrelated QA changes.


Root started independent scoped-domain tests against a frozen derivation containing the latest domain/index changes and deferred-removal correction. The asynchronous build/test handle remains active. Startup scheduler review also found the proposed always-restart timer lacks a demonstrated pre-fix race: root required a failing reproduction and burst-scheduling coverage, since repeatedly restarting the timer can delay polling. The current keep-alive predicate omits live persistence states; T14/T19 were asked to coordinate receipt draining and follow-up dirty saves without relying on user input.


## r83 affected-history fixture failure and regression quality

Root's independent r82 scoped-domain run exited 101: 12 tests passed and the new large-affected-history test failed its `research_result` precondition before executing the move. [Result](evidence/r82-scoped-tests.json). This is not evidence of a post-move product failure; the fixture must first prove its completed reports are valid. The unrelated-history case measured move 4.820 ms and Undo 1.004 ms, which does not substitute for the failed affected-history case.

Root also reviewed the structural-removal regression: it removes an item appended after the edited survivor, so the survivor's index never shifts. T13 was required to place the removed entity before the survivor and demonstrate failure against the prior indexed implementation. A green test that does not exercise index movement is insufficient evidence for this fix.


Root directly corrected the persistent startup-test compilation blocker after assigning narrow ownership: clock advancement and executor draining now occur on `HeadlessAppContext` outside the `App`/window update borrow. All completion/notification/render assertions remain. Function ownership was returned to T19 immediately afterward. Root froze the current integrated source and started `cargo check --features qa`; neither compilation success nor native root-cause resolution is assumed before evidence.


## r84 startup observation boundary

The r83 integrated build and invalid guard succeeded, but keyboard/visual QA exited 101 after 13.946 seconds because `startup-checking` was absent from the observation registry. The startup view has an ID but no observation wrapper, so this failure alone does not establish a native repaint failure. [Result](evidence/r83-keyboard-visual.json). T19 received the exact missing-observation finding. Root prepared a controlled r83 derivation changing only the startup element's observation wrapper; unchanged startup assertions are being rerun.


## r85 asynchronous form-identity boundary review

The new live-save continuation module now distinguishes pending acceptance from durability, but root found that task/request completion clears the current editor/composer unconditionally. Waiting for Store dirty state to clear cannot protect edits still held in input controls or a different form opened while the receipt is pending. T14 received requirements for captured form identity and input revision, preserving later drafts and current navigation on late receipts. Root also required comparing the captured workspace/incarnation rather than overwriting it on receipt, and exact accepted-generation correlation rather than matching an unsubmitted generation-zero continuation by epoch alone. Delayed-ack, changed-form and workspace-switch cases are required before activation.


The controlled r84 startup-observation derivation built, passed the invalid-QA guard and passed the full keyboard/visual QA. [Result](evidence/r84-startup-observed.json). This isolates the r83 missing-element failure to test observation. It does not prove the native stale-startup-frame defect is fixed, nor justify the epoch-pump rewrite. T19 was asked to run the same scheduled-completion case with the original boolean pump and retain native verification as a separate requirement.


## r86 corrected affected-history derivation and form-instance identity

Root froze the corrected scoped-domain file over the prior isolated domain baseline and started an independent scoped test run. The updated structural-removal fixture now places removed entities before surviving edited entities, so it exercises actual index movement. The affected-history fixture no longer assigns its planning run to a completed step, avoiding accidental shadowing of that step's reusable result. These corrections remain subject to the running test.

Live completion now compares task and field values, but root required a distinct editor/composer instance identity as well: closing and reopening the same task with identical values while a receipt is pending must not let that receipt close the new form. New validation errors must likewise survive unrelated late receipts.


The r86 independent scoped run passed all 13 tests. The corrected 240-affected-step / 43,200-history case produced 485 deltas and measured move 63.323 ms, Undo 1.146 ms in debug. The unrelated-history case measured move 4.784 ms, Undo 1.137 ms. [Result](evidence/r86-scoped-tests.json). These are domain-level measurements, not native responsiveness acceptance; optimized actual-command scaling and UI evidence remain required. Crash binaries were filtered out by the scoped test selector, so this run does not claim crash-suite coverage.


## r87 same-branch work-in-progress backup

Root is preserving the latest navigation migration, indexed Undo, live-save continuation work and independent r76–r86 evidence in the same development branch. The current live-save integration is still under active implementation and is not claimed buildable or accepted by the earlier frozen-candidate results. Icon embedding passed for 51 registered assets and whitespace validation passed. Screenshots, raw traces, logs and local candidate binaries are excluded from staging.


WIP commit `957d268` was pushed successfully to `feature/review-2026-11-08-optimization`. Root started a paired startup derivation from r84 with the original boolean pump restored. Only the epoch-specific method test is removed; the actual scheduled-completion, notification and rendered-state assertions remain unchanged. This comparison tests whether the proposed scheduler change is necessary for the observed headless behavior; native stale-frame reproduction remains separate.


## r88 pending-capacity preflight review

The new command path applies a typed delta to live state before `submit_live_snapshot` checks whether the primary continuation slot is occupied. The outer guard still checks legacy pending fields. Root returned the requirement to reserve/check capacity before any live mutation, or retain each accepted mutation in a bounded continuation queue. A second command rejected during an in-flight primary must leave values, dirty deltas and pending effects unchanged; otherwise it can be persisted later without its intended continuation.


The paired r87 candidate with the original boolean pump built and passed the guarded full keyboard/visual QA, retaining the actual scheduled-startup assertions. [Result](evidence/r87-original-pump.json). Both original and epoch-based pumps pass this headless case; it does not reproduce the native stale-frame observation and cannot justify claiming that a scheduler rewrite fixes it. T19 was informed to keep native root-cause investigation separate.


Root prepared an optimized isolated native diagnostic from the independently passed r87 original-pump source. Its sole isolation override hardcodes the existing local review bootstrap path. This r88 build is intended to investigate startup/sidebar behavior and is explicitly not the final integrated release candidate. The user's production app and workspace are not restart targets.


## r89 bounded-queue correctness and native diagnostic preparation

Root rejected a live follow-up overflow branch that calls `pop_front` when coalescing fails: it can drop an unrelated accepted completion/effect. T14 must reject before mutation or retain each accepted obligation safely; heterogeneous over-capacity tests must prove no accepted effect disappears.

The optimized r88 isolated diagnostic built in 37.36 seconds and passed the invalid-QA guard. [Build evidence](evidence/r88-native-build.json). Root started navigation-context and keyboard/visual QA before any installation; replacement is gated on both results and exact isolated bundle identity/hash/process checks. This diagnostic is for native startup/sidebar investigation, not final release acceptance.


The r88 optimized diagnostic passed navigation-context and keyboard/visual QA. Root verified the exact isolated bundle/process/hash, stopped only isolated PID 81259, backed up the signed executable and installed/verified the new signed diagnostic (`55f56309ae92f4251be952ed03c4f3284e7d8c86811e9fd9c53956c393320263`). [Installation evidence](evidence/r88-native-install.json). Computer then reported the Mac locked again; native launch/visual behavior was not observed. A new manual-unlock handoff is pending while code work continues.


## r90 continuation identity and resumed native handoff

Root found that follow-up coalescing also replaces distinct `EnqueueNext` occurrence IDs and distinct `ClearStopIntent` obligations merely because their enum kinds match. These are not equivalent effects. T14 received requirements to preserve distinct identities, prove same-identity merge semantics, reserve bounded capacity before mutation, and test heterogeneous capacity overflow with exactly-once effects after durability. The live path remains disabled and is not accepted.

After the user reported unlocking, Computer still returned `Mac is locked` while acquiring the isolated review app. Root requested another unlock retry and continued code review; no native launch or visual acceptance is claimed. A frozen 437-file integration snapshot was captured for an independent compile check.


The r90 frozen integration compile check exited 101 on an extra closing delimiter in `EditCommand::guard_reprepare`, `src/domain_command.rs:291`. The current source retained the same delimiter when checked; T14 received the exact failure. [Evidence](evidence/r90-compile.json). Root also rejected the replacement queue design's unbounded per-group completion vector: a group-count limit alone does not bound retained obligations while the worker is unavailable.


## r91 runtime backpressure must preserve completion events

The live queue now has total retained-completion count and byte checks, and the r90 parser delimiter was removed in the current source. Neither change has independent passing integration evidence yet. Root found a new callback boundary to verify: `automation_after_run` returns when auxiliary capacity is full, although its research/recovery callers have already consumed the run completion. T14 must demonstrate eventual reconciliation after capacity is available without user input or restart, exactly-once next-stage effects, and durability before dispatch. Rejecting a fresh user action and deferring an already-received runtime event require different handling.


The r91 frozen compile check exited 101 with 12 errors: ten QA field-to-getter migration sites (T12), a missing `NotifyProbe` type (T19), and one stale persistence method signature (T14). Root also identified an unconditional recursive `ResearchTaskPage::steps_layout` getter, a runtime failure that appears only as a compiler warning. Each finding was returned to its owner with source locations and unchanged-assertion requirements. [Evidence](evidence/r91-compile.json). This candidate is not buildable or accepted.


## r92 graceful shutdown requires live persistence ownership

Root reviewed the production application lifecycle before live-path activation. `on_app_quit` calls `shutdown` and returns an empty future; `shutdown` only queues the live save. `PersistenceWorker::Drop` requests shutdown and detaches an unfinished thread, while queued groups may be returned to an owner that is being dropped. These paths do not yet prove durability for accepted edits or final runtime output on normal Quit or workspace switching. T14 received the requirement for a nonblocking graceful-close protocol retaining the worker, lock and receipt owner, and delayed-write save/quit/reopen plus failure tests through the actual app lifecycle. Worker-only shutdown accounting is insufficient evidence.


## r93 same-branch integration backup

Root is backing up the current typed page-state migration, scoped Undo improvements, pending/durable continuation work and r87–r92 review evidence on the existing development branch. The recursive layout getter has been corrected in current source, but the latest independently compiled r91 snapshot failed and the live path remains disabled. This commit is a work-in-progress backup, not a release candidate or full acceptance. Lucide embedding passed for 51 registered icons. Screenshots, traces, local logs and candidate binaries remain excluded.


WIP commit `54db910` was pushed to the same development branch. An exact-commit archive compile left one error, the missing startup notification probe. Root added only the four-line local probe type, retained all assertions, and independently compiled that controlled r94 derivation successfully. [Evidence](evidence/r94-compile.json). The first archive attempt failed on the host Python tar API; its accidentally started parent-worktree compile is excluded from acceptance. The successful r94 check uses the verified extracted commit plus the stated sole override. Runtime and release gates remain open.


The r94 frozen derivation passed the full test command: 678 unit tests (2 ignored), 8 dispatch crash tests and 3 SQLite v4 crash tests, exit 0, with Node v24.22.2. [Evidence](evidence/r94-tests.json). This covers only commit `54db910` plus the probe definition, not subsequent graceful-shutdown/live activation edits or native behavior.


## r95 live attachment must not fall back to legacy writes

The current worktree now activates live persistence when an incremental workspace attaches. Root found that adapter attachment failure disables the live flag but still retains `Some(storage)`, allowing generic save methods to select their legacy branch for a migrated workspace. T14 must fail closed with explicit unavailable-backend semantics, prove that no legacy write is attempted, and test attach failure after migration followed by mutation, autosave, retry and successful recovery. The startup full-Store `last_saved` clone also remains subject to the original UI-thread allocation requirement. These current-worktree changes are separate from the passing r94 frozen tests.


The immutable r94 candidate built and passed the invalid-QA isolation guard, then all five targeted UI checks: navigation-context, keyboard-visual, global-search, topics and automation. [Evidence](evidence/r94-ui-qa.json). The binary hash is recorded with individual exits and durations. These headless checks do not replace native scrolling, screenshots or current live-path activation acceptance.


## r96 live lifecycle and recovery identity review

The r96 frozen integration compiled successfully. [Evidence](evidence/r96-compile.json). Root returned four lifecycle gaps: idle quit may fail to start its drain pump, unavailable live persistence is incorrectly considered clean, repeated window close can bypass an in-flight process-stop worker, and close completion needs an explicit wake/close action. Root also required workspace identity and commit correlation in the new recovery sidecar, explicit write-error handling, and validation of platform quit paths. A controlled r97 derivation adds an independent regression that leaves an old recovery sidecar while replacing the SQLite workspace with a distinct incarnation; startup must reject the stale recovery or preserve replacement values. That test is running.


The r97 independent recovery test reproduced cross-workspace overwrite: startup changed the replacement workspace model from `replacement/model` to `old-workspace/model`. The focused run exited 101 with one pass and two failures; the other failure was the existing startup test's outdated `migrated_store` assertion. [Evidence](evidence/r97-recovery.json). T14 received the regression. Root also rejected the new same-incarnation replay test as insufficient: replaying identical values cannot show that a stale recovery preserves a later durable edit to the same entity. That later-edit case is now explicitly required.


The r98 independent focused run passed five cases, including the new foreign-incarnation protection, but failed the strengthened later-commit case: stale recovery replaced `Later durable title` with `Committed title`. [Evidence](evidence/r98-recovery.json). T14 received the exact regression. Its subsequent blanket rejection of unequal generations still needs a partial-drain case: when A commits and B remains pending, startup must preserve the newer committed state and recover B. Retaining a stale file for manual inspection alone does not prove normal-quit durability. Computer was checked again and still reported the Mac locked; the existing unlock handoff remains pending.


## r99–r100 native access restored and clean scrolling trace

After the user unlocked the Mac, root resumed native inspection of the isolated r88 diagnostic. Its stale Starting frame cleared on window Raise without resizing, so a resize-specific explanation is unsupported. Result Escape preserved the underlying list. Two older-diagnostic return-context mismatches were observed and returned to T12 for current-version verification. [Native observations](evidence/r99-native.json).

The first 30-second Instruments recording crashed during finalization and could not export; it is excluded. A second 12.768-second trace completed and exported successfully, with all eight gestures inside the window and zero Cargo/rustc samples during recording. It recorded 18 hitches overall: none overlapping the short input-call intervals, nine within explicitly extended 500 ms settling windows. Accessibility observations occurred between gestures, so this does not by itself prove those nine were caused by scrolling. Main-thread samples point to paint, scene replay and bounds-tree insertion; sidebar work was small in this candidate. [Trace analysis](evidence/r100-native-scroll.json). Root restored all four OpenCode sessions after recording, and T19 received the trace plus a requirement to separate idle repaint, scrolling and observation overhead before choosing a fix. Full native smoothness remains unaccepted.


## r101 recovery tail and reader lifecycle review

Root reviewed the new recovery plan and actual history reader integration. Emergency snapshots append a generation-zero dirty tail even when changes are merely awaiting an accepted group's acknowledgement; the stale-plan test must include that actual tail. Root strengthened the same-entity stale recovery test accordingly and the gap test to include an earlier valid group. The r101 capture caught an in-progress duplicate closing statement/brace; a controlled r101b derivation removes only that syntax fragment and retains both stronger tests. It is running independently.

T15 also received requirements to remove the reader's synchronous Drop join, bound request/response queues, correlate page responses with their request/cache generation, and distinguish not-loaded from valid-empty or failed results. Current empty-history/body conditions otherwise re-request indefinitely. Idle opening must start IO polling; pending reads must keep it alive, and settled idle state must stop it. These are acceptance requirements for the actual UI path, not just the indexed database API.


The syntax-corrected r101b focused run passed seven cases and failed the dirty-tail stale-recovery case: `Committed title` replaced `Later durable title`. Whole-plan validation now passes the stronger late-gap case. [Evidence](evidence/r101-recovery.json). T14 is adding snapshot identity and an atomic recovery marker; root requires testing the actual capture/recover/commit/new-edit/crash-before-retirement sequence, not inferring success from hand-built accepted-only plans.

Root also inspected the pinned GPUI implementation: native `on_app_quit` futures receive only 200 ms, and windows are cleared before those futures are awaited. T14 received these concrete lifecycle constraints; moving a long drain into the callback alone cannot prove normal platform Quit safety.


## r102 live integration and native evidence backup

Root is preserving the current live activation, graceful-exit/recovery rework, indexed history reader, navigation return-frame work and r94–r101 independent evidence on the existing development branch. The current worktree remains in progress and is not accepted by earlier frozen results. The latest controlled recovery test is still red; native scrolling requires further controls and the full matrix. Icon embedding passed for 51 registered assets and whitespace validation passed. Local screenshots, raw traces, logs and binaries are excluded.


WIP commit `23485f3` was pushed successfully to the same branch. An exact archive of that commit passed `cargo check --features qa`; the full test command has started against that fixed source. [Compile evidence](evidence/r102-compile.json). This is not a release acceptance claim.


## r103 full baseline and T13 handoff verification

The exact r102 commit passed ordinary compilation but its full test command exited 101 during test compilation: the recovery-plan helper omitted the new `snapshot_id` field. [Evidence](evidence/r102-tests.json). Current source initializes that helper field; root prepared r103 from the exact commit plus only that initializer and restarted the full test command without changing assertions. Empty-id helper cases do not prove the production nonempty snapshot-marker path.

T13 reported a completed code candidate with scoped-command/Undo scale coverage and ten UI checks. Root is independently checking the fixed source; current live integration and final native acceptance remain separate, so the terminal agent report is not program sign-off.


The r103 full command passed 691 unit tests (2 ignored), 8 dispatch crash tests and 3 SQLite v4 crash tests. [Evidence](evidence/r103-tests.json). Root independently reran the actual scoped command/Undo scale test at 3/240/800 affected steps and 43,200 history records: moves 7.247/7.953/16.156 ms and Undo 1.756/1.387/1.577 ms under the test profile. [Scale evidence](evidence/r103-scoped-scale.json). These single samples do not prove UI p95 or native smoothness. The complete standalone QA/performance gate has started from a guarded immutable binary built from the same frozen source.

Root explicitly identified a UI coverage gap: existing QA fixtures attach legacy storage and do not assert the new live backend is active. T14 must add actual TaskApp startup/worker/pending/durable/error/reader/lifecycle coverage. Root also returned a lazy-body race: request identity alone cannot prevent an old disk reply from overwriting newer in-memory runtime output; a body revision/unloaded-state guard is required.


## T14/T15 live persistence and indexed history checkpoint (r104)

The production startup path now acquires the workspace lock, upgrades pre-v3 databases, migrates v3 to v4 with a verified backup, or creates a new v4 database, all on the background preparation thread. Attachment activates exactly one ordered `PersistenceWorker`; `live_required` prevents any fallback to the legacy v3 writer when that activation fails, so the workspace stays read-only until the worker reattaches.

Ordinary writes use tracked entity/metadata deltas. Form saves merge the validated delta into live state and return `Pending`; saved status, form close, Undo insertion and command effects run only from the matching durable receipt (workspace + generation + epoch). A second form/command save is rejected before mutation while the primary continuation is in flight. Form close also compares a captured editor/composer instance identity and values, so a late ACK cannot close or reset a reopened form. Runtime one-shot callbacks retain only stable identities in a bounded replay queue and are replayed from the pump after capacity frees.

Emergency quit records a typed recovery plan that preserves accepted group generations, intent hashes and the final unsubmitted tail; duplicate tail values already represented by an accepted group are dropped. Startup validates the whole plan before applying anything, rejects foreign incarnations and generation gaps, retires already-committed receipts, and writes a snapshot marker in the same atomic group as recovered writes. Cmd-Q uses a nonblocking drain; platform quit/close that cannot be cancelled writes the recovery plan synchronously and reports failure instead of claiming success.

History pages use indexed keyset queries pinned to a commit watermark; concurrent inserts invalidate the page sequence instead of silently skipping rows. Report bodies are omitted at startup and fetched one at a time; a delayed reply hydrates only the exact unchanged unloaded revision, so newer streaming or regenerated output is never overwritten.

Agent-reported checkpoint: [T14/T15 evidence](evidence/r104-t14-t15-live-activation.json). Open acceptance gates: TaskApp live pending/durable/failure/retry, stale ACK, backpressure/runtime replay, close/switch safety, and native responsiveness. These core checks are pending verification, not authorized deferrals. Material/project/settings clone preparation may be deferred only after measured user-action blocking establishes that core responsiveness is unaffected. Root independently validates the checkpoint claims below.


## r104 independent integration review

Computer access was reconfirmed on the isolated native review app; the Settings window could be raised and observed. The installed r88 diagnostic remains older than the current source and is not current-version acceptance. OpenCode is preparing a frozen optimized candidate with the same isolated bootstrap override for the next native pass.

The ongoing r103 full gate found onboarding retry and simplify/Back failures, plus a 1,421 ms synchronous UI save in the legacy automation performance fixture. These were returned to the responsible OpenCode sessions; the legacy measurement cannot establish current live-path responsiveness. Root also identified that the new live QA initially measured time through durable acknowledgement, rather than separating UI blocking from end-to-end latency, and requested separate measurements.

A separate r104 fixed source snapshot passed 700 unit tests (2 ignored), 8 dispatch crash tests and 3 v4 crash tests. [Independent tests](evidence/r104-root-tests.json). The new live UI fixture is being checked separately. Non-core deferrals are now allowed by the user under the explicit TASKS.md policy; neither data safety nor reproducible interaction stalls are waived.

The independent r104 live fixture exited 1 at its second-primary assertion. The QA author has since changed that probe from a generic title command to an exclusive settings form save; root requested deterministic pending-ACK control and a precise distinction between permitted command queueing and form rejection. This early failure leaves later fixture cases untested and is not yet classified as a production defect. [Live fixture evidence](evidence/r104-root-live-qa.json).


## r105 native live-startup rejection

Root verified the complete isolated source manifest, binary hash, signed unknown-QA startup guard and fixture backup before replacing only the native review bundle. The old process was confirmed exited first. The new optimized candidate opened onboarding for an existing configured workspace with `Saving will start when persistence is ready.` Clicking Retry workspace preparation then failed to lock the workspace; `lsof` showed only the same native process holding that lock. [Native evidence](evidence/r105-native-startup.json). This is a core startup/retry blocker, returned to T14 and the live-QA owner. Direct attachment QA is insufficient: acceptance must exercise background preparation, attachment, worker readiness and startup gating together. Navigation and scrolling were not reached in this candidate.


## r106 checkpoint and live-path findings

Checkpoint `402ff49` was pushed to the existing development branch, excluding local screenshots, traces, logs and fixture data. An exact archive passed 700 unit tests (2 ignored), 8 dispatch crash tests and 3 v4 crash tests. [Independent evidence](evidence/r106-root-tests.json). The next isolated native build uses this exact checkpoint to verify the readiness/lock-handoff changes against r105.

The live-QA implementer measured editor submission at 3,897,869 microseconds with 43,200 empty-body runs (7,953,571 microseconds through durability); the suite rejected that UI stall. This is implementer-reported evidence pending root reproduction, not an accepted benchmark. Root explicitly rejected deferring the measured preparation cost. Code review also found live task-editor/comment fast paths applying changes before primary-capacity preflight, which could mutate state despite rejecting an overlapping save. T14 must move preflight before mutation; the dedicated live-QA owner must verify unchanged fields/approval/comments on rejection. These remain core acceptance blockers.

The complete r103 gate finished all 52 entries: 49 passed its entry checks, onboarding and simplify exited 101, and automation performance was rejected for the 1,421 ms synchronous UI save despite exit zero. [Full gate evidence](evidence/r103-full-gate.json). The six persistence benchmark modes completed, including the larger fixture; their exit codes alone are not native smoothness or latency acceptance. The later live-persistence entrypoint remains a separate gate.


## r107 native startup fix and independent UI results

The exact 402ff49 optimized isolated build opened the existing configured workspace directly, resolving the observed r105 startup gate. Native result opening/Escape kept the list; Settings no longer showed a false All-research parent. The real workspace-menu Settings entry still had no Back control and Cmd+[ stayed on Settings, so T12 received a focused origin-handling follow-up. [Native evidence](evidence/r107-native.json).

Root independently passed sidebar, navigation-context, simplify, global-search and onboarding on the exact checkpoint. The live fixture failed its performance assertion: editor submission blocked 3,298,293 microseconds with 43,200 empty-body runs (6,560,541 microseconds through durability). [UI evidence](evidence/r106-root-ui.json). The current development workload prevents a p95 claim but does not make a multi-second synchronous block acceptable.


## r108 atomicity repair and native dirty-exit finding

Root briefly interrupted only T14, moved live task-editor/comment primary-capacity checks before their first mutation, and supplied the missing diagnostic timer declaration that blocked shared compilation. T14 was resumed on the same session immediately; T13 continues the live overlap regressions. An independent frozen build is in progress.

Native Dark-theme Quick creation preserved fresh defaults and the Stay path. However, Cancel → Save draft and continue treated asynchronous Pending as failure: the modal stayed open with `The automation draft could not be saved.` while the background still said Saving. Read-only SQLite confirmed the named Draft was already durable and no occurrence or run was added. [Native evidence](evidence/r108-native-draft.json). T12 owns typed dirty-exit integration, T14 the durable-completion hook, and T13 live tests. This is a user-visible core contract failure, not optional visual polish.


## r109 concurrent SQLite lifecycle and review follow-up

The frozen r108 live suite timed out after 240 seconds. A one-second native sample showed the main thread opening the next fixture while the former writer and history reader closed SQLite handles, with all three blocked in Unix mutex paths. [Independent timeout evidence](evidence/r108-root-live-timeout.json). The bundled SQLite version is 3.51.1; the [upstream 3.51.2 release](https://sqlite.org/releaselog/3_51_2.html) describes a related Unix lock deadlock fix. This is a suspected upstream match, not a proven diagnosis. T19 owns the minimal compatible dependency update, actual runtime-version verification and a same-process concurrency regression. Per-scenario child-process QA isolation does not close the production lifecycle gate.

The live-QA implementer reports that editor submission fell below 50 ms after sidebar-count optimization, but root review found a behavior regression: counting only the newest run hides an older saved successful result after a later failed or empty attempt. T14 must preserve the existing result-count contract while removing repeated scans. These performance numbers remain implementer-reported until the corrected implementation passes independent review.

Root also rejected a draft dirty-exit completion implementation that inferred success from global persistence idleness. The requested destination must follow its own durable receipt exactly once; unrelated operations, a failed write or a reopened identical-value form must not trigger a stale exit. T12 owns this integration with T14's receipt hook. Native acceptance remains pending: Computer again reported a locked Mac when retried after the user's unlock reply. No native result is inferred from headless tests.


The dirty-exit review also covers ordinary research requests: the former boolean capture API returned true for an accepted but not-yet-durable write. T12 must expose its pending outcome and wait for the matching request receipt, preserving the composer on failure and ignoring stale receipts for a newer form. Root additionally found that an initial optimized ownership lookup omitted the unique legacy input-signature fallback; T14 must preserve that compatibility before its speedup can be accepted.

The dependency candidate changes only rusqlite 0.38.0 → 0.39.0 and libsqlite3-sys 0.36.0 → 0.37.0 in the lockfile. Root verified the downloaded bundled source declares SQLite 3.51.3 and started independent tests from a frozen r108 source with only those dependency changes and the new concurrency regression test. This isolates dependency validation from ongoing navigation edits; it is not final integration acceptance.


Root independently passed both SQLite dependency regressions, including actual runtime version 3.51.3. [Dependency test evidence](evidence/r109-root-sqlite-tests.json). More decisively, the unchanged original r108 same-process application suite, rebuilt with only the dependency update, finished in 30.33 seconds instead of hanging for 240 seconds. It advanced through persistence, reopen, pending quit and startup retry to the final performance gate. [Application-level evidence](evidence/r109-root-live-dependency.json). This supports the upstream lock-fix explanation; it is not a deterministic causality proof. The suite still failed because the unchanged baseline editor submission blocked for 3,843,279 microseconds. The performance fix and dirty-exit integration remain unaccepted pending independent tests of their final implementations.


## r110 dependency acceptance boundary

Root's dependency-only snapshot passed 700 unit tests (2 ignored), 8 dispatch-admission crash tests, 3 v4 crash tests and the final 2 SQLite regression tests. The concurrent database passed integrity checking and contained exactly 214 expected rows. The first unit invocation lacked Node on PATH; the documented rerun used the installed Node v24.22.2 and passed without code changes. [Independent validation](evidence/r110-root-dependency-validation.json).

The SQLite dependency update is accepted within this tested boundary. It does not accept the ongoing dirty-exit integration or sidebar performance change, and it does not satisfy native scrolling or full-program release gates. Root review required an exact TaskEdit operation token in addition to the automation/research-request tokens, plus a regression exercising dirty-exit Save before reopening an identical form; a direct Save-only test would not cover the navigation bug.


## r111 live review and resumed native inspection

Root's frozen working-tree live suite independently measured the 43,200 empty-run editor save at 16,086 microseconds before the UI update returned (28,435 microseconds through durability), versus the prior multi-second stall. This passes the narrow 50 ms save gate for this sample; it does not prove native scroll smoothness or p95. The suite still failed the task-reopen dirty-exit case and an invalid planning route assertion. [Independent live evidence](evidence/r111-root-live.json). The task completion must mark its exact write durable even when a newer form owns the screen, then drop the stale exit by identity. The planning composer opens in place, so its pending test must not assume opening it changes the underlying route.

Computer became accessible again. Native Quick wizard inspection found step3 calling an unapproved draft Approved plan and exposing Goal/Final report/Replan internals despite describing Quick as having no goal tree or planning. Step2 also shifts the Cancel/footer alignment and exposes dispatch implementation prose. T19 owns the render-only correction; T12 retains save handlers and form identity in the shared file. [Native evidence](evidence/r111-native-quick-wizard.json). Final corrected native acceptance remains open.


## r112 QA compilation repair and scope correction

Root briefly interrupted T12, moved `run_until_parked` from the inner GPUI `App` closure to the outer `HeadlessAppContext`, and resumed the same OpenCode session. This fixes the compilation error without deleting or weakening the dirty-exit assertion. A frozen independent `cargo check --locked --features qa` passed after the repair. [Compile evidence](evidence/r112-root-check.json). Outputs from older executables after a failed build are not accepted as evidence for the current implementation.

The planning QA correction must also honor Cancel's contract: `CancelPlanning` closes the composer in place, preserving its underlying project route. A test that expects All research must explicitly request that navigation target; it must not change product behavior to satisfy an incorrect test expectation.


## r113 standalone draft save failure

Root reproduced a second caller of the original pending-save feedback defect. In the isolated native app, changing the saved Quick draft name and clicking standalone Save draft persisted the name in SQLite, but the editor remained open with Saving even after read-only disk confirmation and another native observation. [Native evidence](evidence/r113-native-direct-save.json). The pending `LiveCompletion::Command` path did not perform the same form completion as the synchronous saved path. T12/T14 must carry an exact automation form identity into completion; T13 must cover standalone step1 Save and step3 Save draft & finish, failure/retry, and reopened-form protection. Fixing only the dirty-exit dialog is insufficient.


## r114 completion integration ownership

Root stopped the conflicting parallel edits to standalone automation completion: T12 is intentionally interrupted, while T14 is the sole owner of the save API and receipt integration. T19 continues only wizard rendering. The duplicate temporary receipt structures must be consolidated before the next candidate is built. A further review requires clearing an unconsumed automation-save handoff after command rejection or synchronous completion, so an unrelated later command cannot inherit it. T13 owns the focused regression and the existing live suite. This is an integration repair, not a relaxation of the acceptance criteria.


## T14/T15 live acceptance checkpoint (r114)

The dedicated live fixture now passes all 20 cases on the production live path, including real background v4 startup/retry, pending/durable/write-failure retry, reopen, pending quit, task/automation/planning dirty exits, reopened-identical forms, standalone automation Save draft at step 1 and step 3, newer-form preservation, settings save, 43,200-run editor save and indexed history.

Measured synchronous live UI work on the 43,200 empty-run fixture: editor save 15,102 us, small editor 3,877 us, settings 1,954 us, first indexed history page 2,524 us. The earlier ~3.8 s editor stall was a per-row O(n²) ownership/archive rescan in `progress_result_task_count`; result-count ownership now uses the held run plus the unique legacy signature fallback, and the sidebar no longer rescans the run history per row.

Automation direct Save draft now has one explicit completion mechanism (`navigation.pending_automation_save` -> `LiveCompletion::Command.automation_save`). It finishes only for the same captured form epoch/input; a pending dirty-exit retains ownership of the close/route, and a rejected or legacy sync save clears the handoff so a later command cannot steal it.

Full gate: 701 unit passed (2 ignored), 8 dispatch crash, 3 v4 crash, 2 SQLite regression tests. [Evidence](evidence/r114-t14-t15-final-live.json).


## r115 independent save and navigation acceptance

Root froze the integrated source and independently passed all 20 live persistence cases, 701 unit tests (2 ignored), 8 dispatch-crash tests, 2 SQLite regression tests, and 3 v4 crash tests. The 43,200 empty-run editor save returned from synchronous UI work in 17,122 microseconds; this single sample does not establish native scroll smoothness. [Live evidence](evidence/r115-root-live.json), [test evidence](evidence/r115-root-tests.json).

In the isolated native candidate, direct Save draft now closes the editor to the corresponding definition and shows truthful saved feedback. Editing again, selecting All research and choosing Save draft and continue reaches All research without a false failure or stuck Saving state. A read-only database check confirms the changed draft name, Draft status and unchanged occurrence count of five. [Native evidence](evidence/r115-native.json). The candidate excludes the later Quick tooltip wording; remaining theme, compact-window, keyboard and scrolling acceptance is still open.


## r116 native idle control and scrolling rejection

Root recorded the same optimized r115 candidate on Dark Settings / Agent & models. The 12.834-second idle control recorded zero app hitches. The 20.793-second scroll recording recorded 420 app hitches, including 21 overlapping input-call intervals and 76 overlapping input or an explicit 500 ms settling window. Nine gestures fell inside the trace; the final upward check occurred outside it. Both recordings finalized and exported successfully, and recording-window monitors observed no Cargo/rustc processes. [Evidence and limitations](evidence/r116-native-scroll.json).

This does not pass native smoothness. Rapid reversals were grouped without intermediate AX reads, but a subsequent AX observation and screenshot remain possible overhead; the idle TOC export overlapped the initial capture before gestures. T19 must isolate the remaining rendering/scrolling cost before another candidate is accepted. The recorded intervals are hitch durations, not complete frame durations or calculated FPS. T12 separately owns the real workspace-menu Settings keyboard regression; no speculative key-token normalization is accepted without evidence.


## r117 verified-source checkpoint

The development checkpoint stages the exact r115 source manifest, including its independently passed save and crash regressions. Later Quick tooltip wording and the workspace-menu focus repair remain unstaged for separate acceptance. This checkpoint preserves tested work; it is not release approval. Native scrolling remains rejected by r116 evidence.


### r117 checkpoint publication and review follow-up

Checkpoint `aa22024` was pushed to `feature/review-2026-11-08-optimization`. Only the frozen r115 implementation and compact review evidence were committed; raw captures and screenshots remain local. The independent full offline matrix now targets that immutable binary.

Root rejected a proposed keyboard regression change that replaced the exact captured-scroll assertion with a mere return-frame presence assertion. The focus repair must retain scroll/filter restoration coverage, using a genuinely overflowing fixture if required. T12 must report failures without weakening the contract. T19 is analyzing the r116 idle/scroll contrast before proposing the next performance change.


## r118 native Light Quick wizard verification

On the isolated r115 candidate, root verified full-width schedule/review steps, stable Cancel/primary anchors, and truthful Quick instruction/expected-answer review. Step3 Save draft & finish returns to the matching draft detail without enabling. The old Enable tooltip still says plan in this installed candidate; the known later wording fix remains outside this native acceptance. [Evidence](evidence/r118-native-quick-light.json). Compact layout and full native-matrix completion remain open.


### r118 performance analysis review

Root rejected the first T19 causal interpretation: the first eight input calls occupied only 565 ms, so nine multi-second hitch clusters cannot be asserted to match nine calls one-to-one. Its stated median interval also contradicted its frequency counts. The report must distinguish call timing, potentially queued native events/inertia, and observations, and provide reproducible statistics. A negative static-logo control would exclude one cause only; it would not prove tool cadence or rule out application/framework pacing. Native smoothness remains unaccepted.


## r119 independent keyboard candidate and statistics correction

Root froze the keyboard-focus/tooltip candidate, built it successfully and verified the unknown-QA guard before use. The sidebar suite failed its retained caller-scroll assertion; navigation-context QA passed. T12 received the exact failure and must preserve the assertion while diagnosing the fixture or behavior. [Candidate evidence](evidence/r119-focused.json). The later render-before-offset fixture change was not included in this freeze.

Root independently calculated a 16.666667 ms median hitch-start interval and a duration range of 8.333125–8.333416 ms. The first eight calls occupy offsets 6.881–7.446 seconds; the ninth begins at 14.387 seconds. These are not presented-frame intervals and do not establish a tool-cadence cause. [Corrected statistics](evidence/r119-scroll-statistics.json).

Dragging the native window edge did not visibly resize it; compact acceptance was not performed. A subsequent isolated candidate can start at the exact compact dimensions, recorded as a test-only window-bounds override.


## r120 compact native keyboard repair verification

Root built and installed an optimized isolated r119-source candidate with two explicit test-only overrides: isolated workspace bootstrap and initial 1000×680 logical window bounds. Signed/unsigned guards passed; the prior isolated process exited before replacement, and the fixture was backed up. [Install and native evidence](evidence/r120-native-compact.json).

The actual workspace-menu Settings path now responds to Computer `super+bracketleft` and returns to Automations. The same input had failed before the page-focus repair. This narrowly accepts the native keyboard fix without speculative key aliases. Captured-scroll QA remains a separate unresolved gate. Light compact research and automation lists were readable in the fixture; the rest of the compact matrix remains open.


## r121 compact wizard and full-matrix follow-up

Root verified the compact Quick wizard across steps, reachable fixed footer controls, fully readable scrolled authorization, and preservation of step/scroll during Light-to-Dark switching. Save draft & finish returned to the matching Draft detail. [Evidence](evidence/r121-compact-wizard.json).

The offline r115 matrix also exposed `--qa-task-edit` failing after impact confirmation because `pending_saved_exit` was still present. T14 owns diagnosis and must distinguish a production completion defect from a QA settling gap without removing route/durability assertions. T19 corrected its r116 statistics and withdrew the unsupported gesture-cluster/tool-cadence claims. Exported display-scoped frame tables lack sufficient app-surface attribution, so cause and native smoothness remain unresolved.


## r122 review constraints for remaining regressions

Root rejected setting the automation scroll offset after opening the workspace menu: that bypasses the real journey and can hide a menu-triggered reset. The test must scroll an overflowing list, verify the offset after rendering and menu opening, then verify captured and restored offsets. T12 is also checking whether shrinkable list rows prevent genuine overflow; any layout change needs measured bounds and last-row reachability.

Root also rejected a synchronous completion helper that marked any pending saved exit durable/closed without matching the specific task operation. T14 must preserve token/form ownership, protect unrelated pending exits, and cover the synchronous/no-change paths without weakening the existing impact-confirmation regression. These reviews do not change native keyboard acceptance or waive native smoothness.


## r123 compact Automation results failure

Root observed horizontal overflow on Automation results at 1000×680 logical pixels in Dark. The subtitle and toolbar hint extend offscreen, and each of five result rows places its right-side action outside the viewport. This is an actionable compact-layout failure, not a tool issue. T19 owns responsive results rendering and must assert action visibility/clickability with realistic metadata in both themes/sizes. [Evidence](evidence/r123-compact-results.json).


## r124 independent task-exit verification

Root built a frozen r119 base with only the exact task/form synchronous completion fix. The unknown-QA guard passed; task-edit QA and live-persistence QA both passed independently. This accepts the targeted saved-exit regression, not the subsequent lazy-body or compact-results work. [Evidence](evidence/r124-task-exit.json). Computer access also succeeded again; the installed r120 compact candidate still reproduces the known clipped results actions.


## r125 native Archive view inconsistency

At 1000×680 in Dark, Archive List shows one archived goal and its Restore action. Switching to Board keeps the count at one (Execution 1) but renders “No tasks match this status.” Switching back to List restores the visible row. This is an unresolved view-consistency defect; no restore or deletion was performed. [Evidence](evidence/r125-archive-board.json).


## r126 full frozen-source offline matrix

The r115 candidate completed all 53 inventoried offline entries: 51 passed, two failed, and none timed out. Root cross-checked the terminal ledger. The failures are sidebar caller-scroll capture and task-edit pending exit; the latter has separate passing r124 evidence. All six persistence benchmark modes exited successfully, including the 607.60-second XL case. Exit status alone does not accept latency targets. [Full compact ledger](evidence/r126-offline-matrix.json). Native compact defects and scrolling remain open, and later source changes still require their own verification.


## r127 review boundaries for lazy results and archived cards

Root rejected treating an unloaded body as a content-bearing `research_result`: dependent prompts, report validation and export must never consume an empty placeholder as a valid report. The proposed repair separates pending-body presentation from executable content. Root also rejected rebuilding two full run-ID sets on every history poll; cleanup must follow bounded cache/entity changes rather than add history-sized UI work. These changes remain under review.

For the r125 Archive defect, code inspection identifies the active Execution-group renderer being reused for archived InProgress cards. Archived status intentionally has no execution group, so all such cards disappear. The requested repair preserves archived topic-grouped cards without restoring execution admission or approval controls.


### r127 independent navigation acceptance

The frozen navigation candidate built successfully and passed its unknown-QA guard. Sidebar QA passed in 1.79 seconds and navigation-context QA passed in 21.21 seconds. The sidebar regression now exercises genuine overflow, last-row reachability, the real workspace menu, restored page focus, and exact captured/restored scroll (within one pixel). This separately resolves the remaining r115 offline sidebar failure; it does not accept the unrelated in-progress result/body/Archive changes or native smoothness. [Evidence](evidence/r127-navigation.json).


## r128 performance evidence scope correction

Root rejected interpreting legacy benchmark timings as direct measurements of the current live backend. The persistence harness constructs `new_mock` and calls `attach_storage`, which reaches `attach_loaded_storage_live` with `None`; it measures legacy full load/save. Native startup and live-persistence QA attach the incremental live backend. Legacy stalls remain valid evidence for that path, and offline render risks remain relevant, but the required live-backend p95 is not established by either those legacy numbers or the existing single-sample live timings. T13 must report constructor/backend provenance and the missing live percentile coverage explicitly.


### r128 integrated checkpoint verification

Root combined only the verified navigation and exact task/form saved-exit changes in a frozen candidate, forced recompilation, and passed sidebar, navigation-context, task-edit and all live-persistence cases. The unit/crash suites passed: 701 unit (two ignored), eight dispatch-crash, two SQLite and three v4-crash tests. [Integrated evidence](evidence/r128-integrated.json). This is a development checkpoint, not release approval; later result-loading, compact-results and Archive fixes are excluded.

The corrected [performance scope report](evidence/r128-performance-scope.json) preserves legacy stalls and explicitly leaves the current live p95 unknown. T13 now owns a separate live performance harness using actual LiveStartup, repeated UI actions and ordered durable acknowledgements. Native scrolling remains an independent failed gate.


## r129 lazy-body and compact-evidence review

Root returned the lazy-body candidate for additional work. Its new multi-step test did not actually establish selected-period completion/status after asynchronous hydration. The unavailable-body notice pointed to a footer retry without demonstrating a matching body-retry action, and the loading copy claimed period completion even for active/failed periods with intermediate results. Root also requested coverage of opening a period when the history pump is idle and of scheduled retry wake-up. No native result-state acceptance is claimed.

Root requested that compact-results QA capture the list before opening a result dialog; dialog-only screenshots cannot validate list typography. Container bounds and accessibility labels alone do not prove visible text wraps or remains readable. Archive membership is being tested separately on a frozen r128-based candidate.


### r129 independent Archive regression result

The frozen archived-card repair built and passed the unknown-QA guard. The actual `--qa-archived-research` suite passed in 8.53 seconds. Root inspected its compact Light screenshot: the archived InProgress goal is present with Archived status, step access and Restore, with no execution/approval control. [Evidence](evidence/r129-archive.json). Native Dark reproduction must still be rerun; T22 remains open until then. An initial unsupported `--qa-archive` invocation was safely rejected and is retained as an invocation error, not a product regression.


## r130 native Archive membership and return-path regression

Root installed the guarded optimized r129-based candidate into the isolated test bundle, with the workspace fixture backed up and compact window override recorded. Dark native List and Board both show the archived goal with Restore. Opening its steps preserves archived status and suppresses execution/approval controls. This narrowly verifies the missing-card repair.

A subsequent native journey failed: Archive Board → Open 3 steps → Back navigates to the ordinary Topic board with zero active goals instead of returning to Archive. Both the keyboard shortcut and the top Back button reproduce the wrong destination. T12 owns the origin capture/return repair; T22 remains conditional on this cross-task navigation gate. [Evidence](evidence/r130-native-archive.json).


## r132 native compact-results repair rejected

Root built and installed only the proposed results/header and occurrence-row layout changes over r130. At 1000×680 Dark, subtitle and unread hint now fit, but all five result rows still place Open beyond the right viewport edge. Container-width declarations have not fixed the actual action reachability. T19 must repair the bounded parent/row layout and prove full action bounds before another candidate. [Evidence](evidence/r132-native-results.json).


## r131 repeated live-performance gate failure

The new actual-LiveStartup harness completed 30 measured saves per fixture with exact final disk-title validation. For 43,200 periods, command p95 was 5.352 ms, completion-pump p95 85.591 ms, render p95 80.147 ms and aggregate main-thread-work p95 149.467 ms; the 50 ms target failed. These component percentiles are not additive. Concurrent compilation was active, so root requires a confirmed quiet comparison before assigning cause. [Evidence](evidence/r131-live-performance.json). The failure is retained; it is not waived, and the native scrolling gate remains separate.


## r133 controlled-performance preparation and root layout takeover

Root requested compute holds from all external sessions. The first quiet preflight aborted before running the benchmark because a new Cargo build appeared despite a ready acknowledgement; no quiet result is claimed. Root continues to check actual process state rather than trusting the marker alone.

After repeated feature-QA compile failures and the r132 native rejection, root interrupted T19 and took ownership of its compact-results layout and test slice. The new draft keeps the action outside a flexible text column, separates automation identity from wrapping activity/status metadata, and captures the list before testing Open/Escape. This draft is not yet built or accepted; builds wait until the controlled performance run is complete.


### r133 quiet live-performance comparison

With every OpenCode session inactive and no Cargo/rustc for five seconds before the run, root independently reran the same immutable live benchmark binary. The complete monitor observed zero build-process samples. On 43,200 periods, 30 measured saves gave main-thread-work p95 7.234 ms and durable-wall p95 13.237 ms; the measured 50 ms save gate passes in this condition. First-page main-thread-work p95 was 6.766 ms. Both exact final titles matched disk. [Evidence](evidence/r133-quiet-performance.json). The r131 noisy failure remains recorded; this contrast does not waive native scrolling or establish performance for later source changes.


## r134 root compact-layout QA fixture correction

The root-owned layout candidate compiled and passed its unknown-QA guard. Keyboard/visual QA then rejected the new compact-results fixture before layout checks: a Quick definition incorrectly used the multi-step Research template. Root corrected the fixture to use the existing Quick builder and froze r135 for a new run. No layout acceptance is claimed from this failed invocation. [Evidence](evidence/r134-compact-fixture.json).


## Lazy report-body status and UI regression (r129 private r128 slice)

Read-only inspection of the relocated T24 fixture showed occurrence 5 Succeeded with final run 19, a 192-byte durable body, and tasks 17/18 Done. The UI treated the intentionally unloaded body as missing because result selection required non-empty output. The fix keeps `research_result()` content-strict, records body presence separately for status/presentation, requests the opened period's final and step bodies, and exposes explicit missing/failed/retry feedback. `automation-report-retry` re-requests only the report body; it is not the persistence footer retry.

A private r128 frozen slice overlaid domain/research/sqlite/live-save/history/QA files and spliced only `automation_occurrence_detail` and `automation_occurrence_steps`. Guard binary `3ae392b1ea9b21207911bbfd9d9060f16c5ea3922271387f92e9c04e15b6f385` passed all 23 live cases, including actual result-dialog opening with an idle pump, multi-step body hydration and final/step status, missing-body truthfulness and retry click, and latest-failed-attempt invalidation. [Manifest](evidence/r129-lazy-ui-slice.json).

## r135: root result-row geometry review

The root-owned two-line result layout passes the isolated keyboard/visual suite in the frozen r135 candidate. Root inspected 1000- and 1440-wide screenshots in both themes: names truncate, metadata remains visible, and the separate quiet open action stays inside the row. The fixture now uses the correct Quick template for Quick tasks. Evidence: [r135 results layout](evidence/r135-results-layout.json).

This is scoped offline acceptance of geometry and tested routing, not native or full visual acceptance. Light-theme name colors can still be too bright against white; a separate palette-only correction is dispatched. Computer access has recovered and the isolated native app can be observed again. T12 caller-origin and T14 lazy-body candidates are undergoing independent replay; implementer reports alone do not close these gates.

## r136: independent origin and lazy-body regression replay

Root replayed the immutable T12 candidate: archived-research and navigation-context suites both passed. Root also replayed the immutable T14 candidate: all 23 live persistence cases passed, including selected-period hydration, missing-body/retry and latest-failed invalidation. See [r136 independent evidence](evidence/r136-independent.json) for binary hashes and timings.

The lazy-body tests explicitly start the pump and then manually poll runtime. They establish hydration/retry behavior but do not establish the claimed production wake from idle. That specific test claim was returned for revision: open the real dialog without a test-side start and verify that production rendering wakes the pump. Native and combined-candidate release checks remain open.

## r137: corrected visible-history benchmark and integration candidate

The revised benchmark opens the Automation detail and verifies that the first history row is actually rendered. Root ran its guarded immutable candidate after a five-second quiet preflight; no Cargo/rustc process was sampled throughout the run. At 43,200 history rows, 30 measured saves had aggregate main-thread p95 7,073 us and durable-wall p95 13,159 us. Visible history requests had aggregate main-thread p95 13,529 us and wall p95 20,527 us. Evidence: [r137 visible history performance](evidence/r137-visible-history-performance.json). The prior noisy failure remains retained; this baseline-plus-harness benchmark is not combined-product or native-scrolling acceptance.

Root rejected the first palette revision because it mixed RGB backgrounds with RGBA channel extraction. The revised helper explicitly converts the background and adds independent black/white and primary-channel reference tests. The integrated r137 source is frozen for full Rust tests and native review; neither pending verification is implied by the benchmark result.

## r137: combined tests and native result loading

The frozen combined source passes 706 Rust unit tests (2 ignored), 8 dispatch crash tests, 2 additional persistence tests and 3 v4 crash tests after setting `PLANNING_NODE` to the existing Node 24 executable. The first attempt is retained: its sole failure was Node missing from PATH, not a product assertion. Icon verification passes with 51 registered assets.

The isolated signed native candidate visibly repairs the previously reproduced Period 5 failure: the result opens with its saved final report, Complete/Current status and Done / 1 of 1 steps. Escape returns to the results list. Dark and Light compact result rows retain their Open controls. Archive membership remains visible in List and Board. During the subsequent Open steps/Back batch, Computer reported that the Mac was locked, so no return-path outcome is inferred. Evidence and remaining native scope: [r137 native](evidence/r137-native.json). Native smoothness and the full release matrix remain open.

## r138: native archived origin confirmed; shortcuts remain unaccepted

On the same signed r137 native candidate after unlock, root repeated Archive Board → Open 3 steps → Back button twice. Both returns preserved Archive/Board and its goal/Restore card. Archived steps kept truthful history labels and no run/approval controls. This closes the previously reproduced wrong-Topic destination for the pointer Back path.

The native `super+bracketleft` input did not navigate, and `super+k` did not open Search in that context; pointer Search and Escape worked. This is an unresolved keyboard acceptance case, not proof of a particular application cause. A local-only capture/bubble diagnostic will distinguish event delivery, modifiers and focus before changing product logic. See [r138 native origin](evidence/r138-native-origin.json).

### r138 shortcut diagnostic and unchanged-binary control

The local-only diagnostic logged correct Command modifiers (`platform=true`) and root focus for both `[` and `k`; capture and bubble handlers ran, and navigation/search worked. Root then removed the diagnostic by restoring the byte-identical signed r137 executable and verifying its hash/signature. On that original binary, Archive Board → Open steps → Command-[ visibly returned to the same Archive Board with its card and Restore. No product keyboard patch was made. This supplies the missing native shortcut-origin success; earlier failures after unlock remain in the record as transient and unclassified, not silently reclassified as fixed. The full keyboard and smoothness matrices remain open.

The full r137 regression driver has also exposed a topics Back expectation mismatch and two retained-fixture path setup failures. These are being investigated without removing assertions or replacing the retained fixture. Original failures will remain alongside supplemental reruns.

## r139: ordinary research dispatch blocks native acceptance

At 1440×920, approving synthetic G23 succeeded. Run goal then left all three steps paused, and Resume queue reported an outstanding durable admission for several minutes without starting the local fixture agent. This is a core execution failure; completed-goal archive/restore acceptance was not reached. The running isolated fixture is retained for diagnosis. OpenCode is assigned the runtime/admission investigation and regression; root will repeat the native workflow after reviewing the fix. See [r139 native dispatch](evidence/r139-native-dispatch.json). No smoothness conclusion is drawn while the offline matrix is running.

### r139 full offline matrix and independent corrections

The immutable r137 candidate completed 54 inventoried entries: 48 passed, three carried exact-binary checks passed, and three original failures were retained. Inventory and 43,200-period persistence checks passed after correcting only the working directory for the unchanged retained fixture. The topics failure was a stale structural-parent assertion; a test-only overlay independently passed both topics and navigation-context after asserting the actual All/Board caller and preserving the downstream Topic checks. All inventoried entries now have passing coverage, but this is not one all-green original run and does not override the newly reproduced native dispatch failure. Crash parent tests remain separately recorded in r137 integration evidence. See [full offline ledger](evidence/r139-offline-matrix.json) and [root topics review](evidence/r139-topics-review.json).

## r140: filtered period return and runtime wake evidence

Native filtered Automation results → Period 3 → analysis step → pointer Back restored the same period and report scroll position; Escape restored the same filter and two of five rows. Command-[ again produced no visible navigation, so the earlier successful control does not close native keyboard acceptance. During this unrelated historical-reader flow, previously stalled ordinary E24 finally ran and completed without another Run/Resume action, while the displayed historical result remained stable. Remaining steps stayed paused. Source review identified both an admission pump wake/keep gap and a batch handler that expects a new run synchronously immediately after submission; the async acceptance/queue integration is under repair, not accepted. See [native chain evidence](evidence/r140-native-period-return.json).

The first expanded performance attempt had zero observed compilation/matrix contention but failed before the large fixture: its final seeded Store had lost model configuration and Enable was correctly rejected. This is a harness setup failure, not a latency result. The failure is retained in [performance attempt evidence](evidence/r139-expanded-performance-attempt.json); a full corrected-harness run and quiet measurement remain required.

## r155: bounded native scroll diagnostic and SetRead review

Root installed the isolated frame-probe candidate after closing the old isolated application. Seven native scroll inputs on the small G23 overview fixture produced no late CPU submissions in the recorded bracket; maximum draw time was 6.03 ms and maximum input-to-CPU-submit latency was 6.91 ms. The probe's seven unit tests independently passed. This is a diagnostic baseline, not the full scrolling gate: large history, concurrent saves, other surfaces, and quiet integrated-candidate runs remain outstanding. See [native scroll evidence](evidence/r155-native-scroll-diagnostic.json). Raw traces and screenshots stay local.

Root independently ran the immutable SetRead candidate's focused QA: missing/busy guards, pre-existing dirt and equal-value pending writes, synchronous enqueue failure with retained Retry, and asynchronous failure/restart passed. Source review confirms that Retry preserves the requested read bit. This accepts the focused correction, not the unfinished runtime integration. See [SetRead review](evidence/r155-set-read-review.json).

The T19 count-navigation fixture was invalid: it filtered to one row, cleared its query during navigation, and expected collection rows on the Project topic-directory surface. Root took over after repeated rework, seeded matching completed goals for real scroll displacement, applied the query after navigation, and limited the count-control checks to its actual All research and Archive surfaces. Existing Project and Topic navigation checks remain; corrected QA is still running.

## r156: report scroll finding and navigation fixture isolation

Native Period 3 report scrolling (Markdown table and sources) recorded five input samples without late input frames, but two CPU presents exceeded the 8.33 ms refresh budget; maximum CPU work was 9.19 ms. This bracket is not accepted as smoothness proof. Build contention may have been present; OpenCode is analyzing the offending phases before a quiet repeat. See [retained diagnostic](evidence/r156-period-scroll-diagnostic.json). Root also assigned preparation of a separate 43,200-period synthetic native fixture with disabled schedules and no runnable/recovery obligations; it must be independently verified before use.

The corrected count-navigation QA reached and passed the All/Archive count-to-Steps and Back assertions before failing its new Save replay. Diagnosis showed the size/theme windows attempted to acquire the main harness window's workspace lock, leaving storage unavailable. Each size/theme window now uses its own temporary workspace and explicitly asserts successful storage attachment. The collection scroll surface also receives the existing QA observation adapter so real pointer scroll events target it. Revalidation remains pending; no failed check was converted to a pass.

### r156 count-navigation correction accepted in isolation

The corrected navigation suite passed at both window sizes and both themes. All/Archive counts open Steps and Back restores real filtered scroll state; dirty Stay/Discard/Save preserves the typed Steps destination. Root inspected light/dark screenshots, verified all 51 embedded Lucide references, and ran the private candidate unit/integration tests successfully. The compact count uses a fixed right chevron rather than a false expand toggle; existing T12 render-wake edits were preserved during selective integration. See [focused acceptance](evidence/r156-count-navigation-review.json). Final shared-runtime/native regression remains required.

## r157: production retry entry point still missing

Root source review found that the proposed parked-dispatch retry method is invoked only by its QA harness, while the existing footer and retry handlers do not surface or invoke it. Helper-level recovery success therefore does not establish usable recovery. OpenCode must wire truthful failure feedback and a real retry control, then exercise that control for a representative retained dispatch. This remains a blocker; see [review and required acceptance](evidence/r157-runtime-review.json).

The proposed 43,200-period native fixture was also rejected before installation: it retained only 193 runs/report bodies. OpenCode is replacing it with real per-result history and verifying links and isolation. The current small isolated native fixture remains unchanged. The independently reviewed SetRead correction has been checkpointed separately in `621a1d1`; it does not include unfinished runtime repairs.

## r158: large native history exposes a scrolling blocker

Root independently verified the corrected synthetic fixture: 43,200 periods, 30,240 Succeeded periods each linked to its own 8 KB report, zero missing report links, no nonterminal work, and one paused definition. See [read-only fixture verification](evidence/r158-large-fixture-verification.json). Root copied it into a separate isolated workspace and completed native onboarding with the existing offline fixture tools; the small fixture and production workspace remain untouched.

Native history paging changed the remaining count from 43,180 to 43,160, opened Period 43162 with its own synthetic legacy report, and scrolled through its findings. The combined bracket recorded 20 late CPU presents, maximum CPU work 40.28 ms, and 15 of 15 input frames over budget. No compiler process was present at either bracket edge; continuous contention was not sampled. This is a failed diagnostic, not accepted smoothness. The probe baseline predates current COW/SetRead and runtime changes, so OpenCode must distinguish already-fixed cost from remaining render work before proposing a narrow private repair. See [native failure evidence](evidence/r158-large-native-diagnostic.json). Raw traces and screenshots stay local.

## r159: independent runtime focused regression passes

Root froze the T12 overlay binary by content hash and independently passed `--qa-dispatch-admission`, including the actual footer retry control. Source review covered runtime wake/keep predicates, batch admission ownership, receipt-ordered cancellation retirement, and retry UI wiring. The focused correction is ready for integration review; see [independent evidence](evidence/r159-runtime-focused-review.json). This does not yet accept the combined current-branch candidate or native material confirm/reload, ordinary three-step completion, or completed-goal archive/restore.

The current global Automation results renderer still collects and sorts all occurrences before limiting visible rows, unlike the indexed per-automation history path. Root supplied this concrete source observation to the active scrolling investigation without asserting causality from source alone. A read-only T01–T25 evidence map has been prepared locally for root's final requirement-by-requirement audit; no task closure is inferred from the map.

## r160: combined focused regressions and truthful retry feedback

Root verified the final T12 overlay includes committed count-navigation and SetRead changes, froze its signed executable, and independently passed SetRead, navigation-context and dispatch-admission suites. See [combined focused review](evidence/r160-integrated-focused-review.json). The separately owned performance harness is excluded; this is not the final full release gate.

Further review found the generic parked-dispatch footer claimed “Dispatch not saved” even when the run was already durable and only launch claiming or cancellation retirement had failed. OpenCode is correcting that prefix to neutral attention feedback before preparing the normal isolated native integration candidate. Existing passes predate this wording-only correction. The old isolated diagnostic application was closed after confirming Computer access; production was not opened. Native material confirmation, three-step completion/archive/restore, and large-history smoothness remain outstanding.

The same frozen overlay also passed 718 unit tests (2 ignored), 8 dispatch crash tests, 2 additional integration tests and 3 v4 crash tests under Node 24. All 51 Lucide assets remain embedded. The shared footer wording correction is now present; its native layout and interaction remain part of the pending integrated walkthrough.

## r161: native queue completion and archive restoration

Root installed the normal isolated integration candidate after verifying its source differences, signature, hardcoded isolated bootstrap and synthetic workspace backup. One Resume goal click advanced the previously stalled G23 from 1/3 to 3/3, without another run action or unsolicited navigation. The final report opened and Command-[ returned to Steps. Archive and Restore preserved all three completed results, with explicit archived history labels. Read-only SQLite verification confirms one new successful run for each remaining step and restored Done/unarchived tasks. See [native runtime evidence](evidence/r161-native-runtime.json). A fresh 0/3 submission, material confirmation/reload, and final performance gates remain separate requirements.

The initial private scrolling cache was returned for revision before integration: clean report-body hydration can leave a dirty-epoch-only cache stale, and clearing a 256-entry cache while rendering more rows can cause repeated full rebuilds. Root requested correct invalidation plus cold/after-save and 280-row measurements; warm-frame gains alone do not close the scrolling failure.

## r162: material saves durably but confirmation remains stale

Native Add Files now wakes verification and presents the impact review. After one Add material confirmation, the saved banner and Materials 1 appeared and SQLite contained the attachment with the expected hash; goal approval was withdrawn as explained. However, the impact dialog remained open claiming nothing had been saved and still offered Add material. This is a failed confirmation workflow. Root traced the missing matching-dialog cleanup in asynchronous Material completion and returned it to OpenCode for a focused repair and real confirm/ACK regression. See [retained failure](evidence/r162-material-confirmation.json). The isolated app remains on this state; production was untouched.
