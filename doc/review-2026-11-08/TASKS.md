# Optimization implementation authority

Date: 2026-11-08. Baseline and evidence: [ISSUES.md](ISSUES.md), [COVERAGE.md](COVERAGE.md). Tasks remain **Open** unless explicitly marked otherwise. Implementation status and verification are recorded in [IMPLEMENTATION.md](IMPLEMENTATION.md).

## How to use this backlog

Use these IDs in implementation PRs and test evidence. Implement narrowly; no broad rewrite. A task closes only when its acceptance criteria are met and relevant regression tests pass. Record the implementation commit, executed commands and screenshot/artifact links beside the task. If a target decision changes, update this document and the applicable PRODUCT/DESIGN/workflow specification in the same PR; do not silently reinterpret the task.

This is the authority for this optimization program. PRODUCT and DESIGN remain the general product specifications; T17 reconciles their conflicts with the target contracts below. RESEARCH_WORKFLOW_STRATEGY remains the authority for research prompts and evidence behavior. Historical AUTOMATION_PLAN material is background, not an overriding acceptance checklist.

P1 means data/scheduling correctness, unexpected consequential behavior or a release-blocking verification gap. P2 means navigation, presentation or maintainability that materially affects ordinary use. P3 means a bounded edge case or follow-on improvement. Effort bands S/M/L are relative implementation size, not delivery promises.

## Target interaction contracts

1. Background work may update status, counts and notifications; it never changes the route, selected item, open dialog, scroll position or draft. A user command may navigate only when its label/purpose implies that destination.
2. “Resume schedule” changes future triggering. “Retry this period” continues the same occurrence, preserves finished steps and reference time, and does not change the schedule's paused state. “Run now” creates one new manual period and does not move the cursor.
3. “Back” returns to the initiating context for detail/drill-down flows, including filters, scroll and selected period. Breadcrumbs explicitly navigate structural parents. If no valid origin exists, use the structural parent fallback. Close dismisses only the current modal/inspector and restores focus to its opener.
4. Definition state, period state, step eligibility and evidence quality are independent. For example: `Schedule paused · Period running`; `Waiting for collection`; `Queued for automation slot`; `Completed with evidence gaps`.
5. A result entry identifies what is available: final report, intermediate evidence or live progress. Streaming commentary is never styled as a validated final answer. A failed period may still expose completed step results with their labels.
6. Creation uses a visible destination and explicit defaults. A fresh form does not inherit hidden values from another item. Navigation preserves dirty drafts or explicitly offers Save draft / Discard / Stay; do not interrupt clean navigation with a confirmation.
7. Search has one predictable scope. Target: a global search surface with explicit Research/Automations filters and visible target types. It preserves the calling page until a result is chosen.
8. Keep the current three-step automation flow for this iteration; make Quick shorter within it. A separate one-screen Quick redesign is not a prerequisite for correctness.
9. Keep compact, expanded history rows for this iteration; the result content and diagnostics remain collapsed into dialogs. Use consistent neutral row surfaces. This resolves the current “fully collapsed history” specification conflict without unnecessary interaction churn.
10. Scheduling while asleep/closed is unavailable. On return, LatestOnly can run the newest eligible missed period; SkipAll and expiry skip as specified. Pausing never backfills its paused interval.
11. Match action emphasis to each page or module: reserve primary buttons for its main action, use secondary buttons or Lucide icon controls for supporting actions, and put suitable low-frequency actions in a More menu. Do not style every action as an equally prominent button. Preserve discoverability, meaningful tooltips and predictable destinations; verify the hierarchy in light and dark native screenshots. This applies across the existing task scope, including T19/T20 acceptance.

## Mandatory scrolling acceptance

Scrolling smoothness is a release requirement across every scrollable surface, not only a benchmark target. T14/T15 must keep persistence, body loading and history queries from blocking interaction; T19/T20 must verify actual scrolling on the isolated native application. Exercise long reports and sources, large research lists/boards, automation history, settings, material lists and scrollable dialogs in both themes and required window sizes, including concurrent run updates and saves. Include small, 43,200-period and larger representative histories where applicable. Record device/display refresh conditions, workload, frame-time distribution and long-frame events, plus before/after traces for any defect. Inspect UI-thread work, layout/rendering, allocation and I/O when a hitch occurs. Every reproducible hitch remains a failed acceptance case until fixed and rerun; static screenshots, synthetic vector slices and average latency alone do not prove smoothness. Keep original latency/memory/database gates as well.

## Delivery order and dependencies

| Wave | Tasks | Outcome |
| --- | --- | --- |
| 0 | T01 | Reproducible test/QA baseline and failure classification |
| 1 | T02, T03, T04, T05, T06, T09, T24, T25 | Prevent surprise navigation/scheduling, undo loss and false result states |
| 2 | T07, T08, T10, T11, T16, T17, T18, T21, T22, T23 | Consistent routes, search, drafts, controls and contracts |
| 3 | T12, T13, T14, T15 | Extract boundaries after behavior is locked by tests; remove measured UI stalls |
| 4 | T19, T20 | Complete keyboard, visual and native acceptance with durable evidence |

Dependencies below are mandatory merge prerequisites unless explicitly called “coordination only.” Independent task investigation can proceed earlier. T01 is a baseline requirement for sign-off, not a reason to postpone reproducing P1 defects. Avoid making every behavior fix wait for a complete architectural extraction.

## T01 — Repair and classify regression coverage

**Priority/size:** P1/M. **Findings:** F15. **Dependencies:** none.

**Do:** make documented offline QA commands a trustworthy release gate.

**How:** declare Node and other test prerequisites, report a clear preflight error when absent, and run publishing tests with a real supported Node installation. Update stale `research`/`save-task` selectors to current user entry points. Investigate the offscreen approval fixture and recovery timeout without deleting assertions or increasing sleeps blindly. Add stable semantic selectors to shared components where appropriate. Classify the additional topics manual-completion mismatch and simplify 2 px control-position mismatch. Document PLANNING_PERF_WORKSPACE for render-perf. Run all 33 offline dispatch entries, identify aliases, and include every failure in the evidence ledger until independently resolved. Preserve those 33 historical entries and include every subsequently added QA entry in final integration coverage. The r63 source audit identifies 48 direct QA flags (46 standalone suites and 2 crash-child modes covered through their parent integration tests), plus 6 persistence performance modes; see [current inventory](evidence/r63-current-qa-inventory.json). Reconcile this inventory against the final source before release; do not invoke crash-child modes without their parent fixtures. Use `python3 script/run_review_gate.py --help` for the frozen-candidate standalone/performance ledger; execute parent crash tests and the native matrix separately.

**Acceptance:** unit tests and documented release suites pass in a documented environment; ignored tests have explicit reasons; failure output distinguishes dependency, fixture, timeout and product failures. Recovery's `step retry did not finish` has a reproducible diagnosis and regression test. QA never touches the user's active workspace or restarts their app.

## T02 — Remove background navigation from run submission

**Priority/size:** P1/M. **Findings:** F01. **Dependencies:** none; T01 for final sign-off.

**Do:** separate queuing/execution from route selection.

**How:** remove unconditional route/selection writes from `launch_run`; return a run/occurrence identifier or dispatch result. Explicit user handlers choose whether to open progress. Timer ticks, dependency continuation, automatic retry and recovery imports only mutate runtime state and emit passive feedback.

**Acceptance:** while viewing Settings, a report, a dirty form and a period dialog, trigger a scheduled start and two subsequent steps with the fake clock/CLI. Route, selection, form content, scroll and focus stay unchanged. Explicit View progress opens the correct period. Ordinary manual Research submission still reaches its intended progress surface. Cover failure/retry and both domains.

## T03 — Separate retrying a period from resuming its schedule

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent review](IMPLEMENTATION.md#t03-independent-review-and-acceptance). T01 and T20 remain release gates.

**Priority/size:** P1/M. **Findings:** F02. **Dependencies:** none.

**Do:** make each lifecycle action affect only its named scope.

**How:** remove the schedule-resume call from the period-retry handler. Use explicit domain commands for `RetryOccurrence` and `ResumeSchedule`. Validate the occurrence's approved snapshot and ownership before mutation; define how an old approved period can be retried after a later definition edit. Default target: retry its original approved snapshot with that version visibly stated; never silently substitute the new template. If policy requires renewed approval, fail without changing schedule state.

**Acceptance:** retry a failed/cancelled period while definition is Paused: definition remains Paused, cursor remains unchanged, same occurrence ID/reference time/completed results are retained. Failure to retry leaves all definition fields unchanged. Resume schedule alone schedules future work and does not retry history. Test NeedsAttention, archived definition, changed version and queued retry/deadline behavior. Update the obsolete comment.

## T04 — Prevent whole-Store Undo from erasing automation changes

**Status:** reaccepted for frozen commit `ed40899` with the topic QA fixture correction and independent strengthened Undo assertions (2026-11-09). Full automation QA passed, including five Undo sites checked against complete definitions, occurrences/dedupe keys, runtime history and ID floors in memory and on disk. See [r81 evidence](evidence/r81-undo.json). Subsequent indexed Undo and asynchronous persistence changes require integrated revalidation; T01 and T20 remain release gates. The earlier legacy-snapshot regression is historical.

**Priority/size:** P1/M. **Findings:** F03. **Dependencies:** none.

**Do:** protect unrelated domain state immediately, ahead of the longer command extraction.

**How:** characterize the ordinary-edit → automation-save/enable/pause → Undo sequence. Either invalidate incompatible undo entries at every durable automation mutation or apply a scoped inverse edit. Prefer a conservative invalidation patch first if scoped inverses cannot be safely delivered independently. Never restore an old schedule cursor/occurrence collection from a generic snapshot. Explain invalidation in Undo's disabled reason.

**Acceptance:** the sequence cannot delete a new Quick draft, change an enabled/paused schedule, roll back trigger dedupe keys or reuse an automation/occurrence identity. Existing ordinary field edits remain undoable where safe. No external work is replayed by Undo. Test changes made while no run is active, because run submission already clears the stack.

## T05 — Share execution eligibility with status presentation

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t05-independent-acceptance). T01 and T20 remain release gates.

**Priority/size:** P1/M. **Findings:** F04. **Dependencies:** none.

**Do:** display the reason a step can or cannot actually run.

**How:** introduce a pure eligibility result containing owner domain, phase barrier, unmet dependencies, approval, queue/pool wait and active-attempt state. Reuse it in automation dispatch, `execution_status`, board/list rows and period steps. Keep definition status separate. Remove ordinary Topic FIFO advice from automation-owned steps.

**Acceptance:** with one collection step running and analysis dependencies otherwise satisfied, every surface says Waiting for collection, not Ready. A completed earlier phase then exposes the correct next eligibility. Only the failed step is Blocked; downstream steps are Waiting with a reason. Automation messages never suggest admission to the ordinary Research queue. Unit tests assert dispatcher eligibility and displayed reason together.

## T06 — Introduce typed result presentation

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t06-independent-acceptance). T21 must preserve this presentation contract; final integration/native release gates remain.

**Priority/size:** P1/M. **Findings:** F05, F06. **Dependencies:** none; coordinate with T05.

**Do:** distinguish final output, intermediate work and live commentary across both domains.

**How:** add a presentation selector returning result kind, attempt status, freshness, evidence quality, reader Markdown and sources. Prefer the validated current final report. Without one, show an explicit “No final report yet” plus progress/step results. Use `reader_markdown` and a shared English UI notice for evidence gaps; preserve user/generated report language. Put streaming diagnostics in a clearly named progress section.

**Acceptance:** Quick, multi-goal Research automation and ordinary Research cover running/complete/partial/failed/cancelled/stale/regenerating states. Raw JSON and internal evidence-gap instructions never appear as report prose. Finished evidence remains inspectable after failure. A successful process exit without a validated report is not presented as a complete answer. Copy/export/render use the same reader transformation where applicable.

## T07 — Keep task neighbors within the active navigation domain

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t07-independent-acceptance).

**Priority/size:** P2/S. **Findings:** F07. **Dependencies:** none.

**Do:** constrain automation Previous/Next to the current occurrence's ordered steps.

**How:** accept an explicit navigation context rather than filtering only by project. Specify whether goal overview participates; target is step-to-step within that period, with a separate Overview action. Disable at boundaries rather than silently wrapping into another period. Keep ordinary Research's documented tree behavior until separately changed.

**Acceptance:** two automations with two periods each cannot be crossed through generic Previous/Next. Visible context and tooltips identify the destination. Missing/deleted target falls back safely. Back returns to the same period; recently opened entries visibly distinguish automation/period ownership.

## T08 — Give Search a stable global contract

**Priority/size:** P2/M. **Findings:** F08. **Dependencies:** none.

**Do:** make footer Search and ⌘K behave identically from all routes.

**How:** implement an explicit search overlay with domain filter, target kind and destination. Search definitions, research titles and period/report metadata using the appropriate query boundary; do not route to All research merely to focus an input. Retain list-local filters as clearly local controls.

**Acceptance:** invoke from All research, project, topic, task, report, settings, automation list/detail/results and a modal. Empty query, no matches, keyboard selection and Escape work. Escape restores caller/focus/draft. Choosing a period opens that period, not a generic last run. AutomationRuns never focuses a nonexistent visible field.

## T09 — Reset new forms from one explicit default model

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent verification](IMPLEMENTATION.md#t09-independent-acceptance). T01 and T20 remain release gates.

**Priority/size:** P2/S. **Findings:** F09. **Dependencies:** none.

**Do:** prevent schedule/unit/policy leakage from previous creation/edit sessions.

**How:** extract `AutomationDraft::new(defaults)` and `from_definition` construction. Reset kind, unit, weekdays, catch-up and all text fields together; use explicit draft IDs for resume-existing-draft. Tests should inspect semantic schedule values, not only field strings.

**Acceptance:** edit weekly/SkipAll and hourly schedules, cancel, start New: Daily 10:00 in system timezone, LatestOnly, default weekdays, minute unit for interval input, and default run limits are restored as specified. Back within the same draft retains its entered data. Switching type does not silently retain an incompatible approved plan.

## T10 — Preserve origin, dirty drafts and creation destination

**Priority/size:** P2/M. **Findings:** F10. **Dependencies:** T02.

**Do:** make Back, breadcrumbs, Cancel and Close predictable.

**How:** add a return context for detail/drill-down flows with source route/filter/scroll. Breadcrumbs remain structural; Back uses origin with parent fallback. Use one dirty-form exit policy: restoreable draft or Save/Discard/Stay. Show the target project in global New research and allow changing it before planning. On cancel restore the original route rather than leaving the user in the default project.

**Acceptance:** filtered results → period → step → Back restores the same period and ultimately same filtered results/scroll. Settings → Edit → Back restores Settings without losing unrelated input. Clean Cancel is immediate; dirty navigation never silently clears the only copy. Planning-in-progress drafts remain discoverable. Return context survives invalid/deleted parent gracefully and is not confused with breadcrumbs.

## T11 — Restore consistent action affordances and visual hierarchy

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t11-independent-acceptance). Final T19/T20 native and release gates remain.

**Priority/size:** P2/M. **Findings:** F11, F17. **Dependencies:** none; T05/T06 for final state screenshots.

**Do:** align shared controls with DESIGN while keeping the current visual identity.

**How:** update secondary/icon factories to show boundaries at rest; reserve ghost treatment for documented compound-control exceptions. Apply danger treatment to cancel/stop; distinguish selection from advancing actions. Give trigger source a visible text label where space permits, with a tooltip in compact layouts. Use neutral row surfaces and semantic state accents rather than large competing fills. Make long step titles wrap or expose their full value predictably.

**Acceptance:** inspect 1440×920 and 1000×680, light and dark, with long English/CJK user titles. Actions are recognizable without hover; passive status chips do not resemble controls; one primary advancing action per group; no clipped action labels. Run icon validation, unit tests and relevant isolated UI QA. Keep Lucide assets and the task-tab exception; do not add a new icon system.

## T12 — Extract a typed navigation controller

**Priority/size:** P2/L. **Findings:** F01, F07–F10 architecture. **Dependencies:** T02, T07, T08, T10.

**Do:** eliminate contradictory combinations of Scope/selection/overlay booleans.

**How:** model routes such as ResearchTask, ResearchResult, AutomationDetail and AutomationPeriod, plus modal/edit substates and return context. Centralize route changes and derive breadcrumbs/sidebar selection from the same state. Migrate one surface at a time; runtime receives no navigation authority.

**Acceptance:** route transition table covers all sidebar/header/row/menu/shortcut entries. Invalid combinations cannot be constructed through normal APIs. No direct route writes remain in process completion/timer handlers. Existing navigation regression matrix passes before and after extraction. This task changes organization, not execution semantics.

## T13 — Extract domain commands and scoped undo

**Priority/size:** P2/L. **Findings:** F03 and mutation architecture. **Dependencies:** T03, T04.

**Do:** consolidate repeated validate/mutate/save/rollback/effect sequences.

**How:** define command inputs and domain results for automation lifecycle and user edits. Separate persistent changes from post-commit effects (queue launch, timer arm, UI feedback). Keep invariant checks inside the command boundary. Use inverse user edits or explicit non-undoable commands rather than whole-Store restoration. Split domain-only automation logic from GPUI glue.

**Acceptance:** a persistence failure cannot launch work, claim Saved or partially enable scheduling. Tests inject failure before commit, after preparation and on retry; commands are idempotent where required. Undo cannot alter runtime history, approval snapshots or schedule cursors. No duplicate manual/automation agent execution engine is introduced.

## T14 — Move persistence off the UI thread with ordered acknowledgements

**Priority/size:** P1/L. **Findings:** F12. **Dependencies:** T13.

**Do:** remove the measured synchronous save stall without weakening durability.

**How:** use a single ordered persistence worker and generation/commit acknowledgements. Coalesce streaming updates, distinguish pending/durable/failed states, and permit dispatch only after the required commit succeeds. Prevent an older save acknowledgement from overwriting newer in-memory state. Account for snapshot-copy/serialization cost, not just SQLite IO.

**Acceptance:** on the same 43,200-occurrence fixture, UI interactions remain responsive during save; target p95 main-thread save-related work under 50 ms, with hardware and sample count recorded. Restart/crash injection verifies cursor+occurrence atomicity and no duplicate dispatch. Saving/failed/retry status accurately reflects durability. No “Saved” is shown before acknowledgement.

## T15 — Add incremental writes and indexed history queries

**Priority/size:** P2/L. **Findings:** F12. **Dependencies:** T14.

**Do:** stop rewriting/loading all historical records for routine edits.

**How:** add dirty-entity persistence and indexed query columns for owner, execution time, status and unread; use cursor pagination and load report bodies on demand. Provide a versioned migration with backup and integrity validation. Measure the actual UI query path rather than a synthetic already-loaded-vector slice.

**Acceptance:** renaming one automation writes only necessary rows/metadata; opening the first history page does not deserialize every report. Query results preserve time ordering and pagination under concurrent insert/retry. Migration/reopen restores all reports, sources, dedupe keys and approvals. Benchmarks include small, 43,200-period and larger representative histories with memory, latency and database-size evidence.

## T16 — Unify temporal ordering and harden schedule arithmetic

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t16-independent-acceptance). T01 and T20 remain release gates.

**Priority/size:** P2/M (arithmetic subtask P3). **Findings:** F13, F18. **Dependencies:** none.

**Do:** make “Latest” and helper boundaries precise.

**How:** extract named occurrence order keys used by list and history. Decide “latest activity” for the list and include the relevant timestamp. Correct empty interval-range counting and explicitly return exact counts or an indicated capped summary for large calendar spans. Keep preview/dispatch on the same scheduling functions.

**Acceptance:** retrying an old ID updates latest activity consistently. Manual request time and scheduled reference time have deterministic fallback rules. Test empty/no-trigger ranges, exact endpoints, DST gap/fold, year boundaries, clock rollback and >4,096 calendar triggers. No overflow and no silent inaccurate count. Existing catch-up behavior remains unchanged for ordinary cases.

## T17 — Reconcile current product/design/workflow documentation

**Priority/size:** P2/S. **Findings:** F14. **Dependencies:** T03, T05, T06; coordinate with all behavior tasks.

**Do:** remove contradictory current guarantees and outdated comments.

**How:** apply the target contracts above to PRODUCT, DESIGN, USER_GUIDE and RESEARCH_WORKFLOW_STRATEGY. Document three-step Quick creation, expanded compact history, LatestOnly catch-up, distinct retry/resume actions, snapshot retry policy and actual report quality semantics. Correct comments when touching implementation. Add a maintained route/action matrix rather than duplicating prose in every file.

**Acceptance:** each user-visible lifecycle term has one definition; no “never backfills” blanket claim contradicts catch-up; new planner-generated Research templates are described accurately. Every behavior PR updates its relevant specification. Repository/app prose is English; quoted/user-authored research content is preserved.

## T18 — Make unattended capability guarantees accurate

**Status:** accepted in the reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t18-independent-acceptance). T25 and release gates remain open.

**Priority/size:** P1/M investigation + scoped implementation. **Findings:** F16. **Dependencies:** none.

**Do:** align the product's read-only promise with the shared agent pipeline's actual controls.

**How:** audit objective, expected answer, generated plan and inherited instruction paths. Validate all supported input fields for obvious unsupported actions, but explicitly classify this as input validation. Document prompt guidance versus enforced capability restrictions. If strict external-write prevention is retained as a product guarantee, design a capability boundary shared by manual and scheduled execution with contextual policy, rather than duplicating agent configuration.

**Acceptance:** a harmless fixture with an unsupported request solely in expected-answer text is rejected or explicitly handled. Tests cover wording variants and negated examples without asserting keyword checks are a sandbox. Documentation names residual limits. Any strict enforcement claim has tool-boundary tests proving denial of external writes and preservation of allowed evidence collection; no real email, trade, publish or delete is executed during tests.

## T19 — Verify keyboard behavior and visual readability

**Priority/size:** P2/M. **Findings:** F17. **Dependencies:** T05, T06, T07, T08, T10, T11.

**Do:** make ordinary keyboard navigation predictable and keep task context readable in compact layouts.

**How:** verify Tab/Shift-Tab order, Enter/Space, Escape, modal focus containment and focus restoration. Keep automation/period identity visible beside repeated Open controls. Use text labels alongside status colors. Check selectable report/body text separately from clickable navigation and provide predictable disclosure for truncated titles.

**Acceptance:** keyboard opens/closes dialogs without moving focus into the background or losing the opener. Visible context identifies the target of repeated actions. Long titles, statuses, empty states, failures and disabled reasons remain readable at minimum window size.

## T20 — Run the full regression matrix for release acceptance

**Priority/size:** P1/M gate. **Findings:** all; closes review coverage gaps. **Dependencies:** T01–T19 and T21–T25 for full program sign-off; can also be run on each delivered wave.

**Do:** rerun the complete feature matrix after remediation, close the explicit external-service, keyboard and layout boundaries, and produce release evidence. The baseline native walkthrough is now recorded in COVERAGE.md; it is not proof that future fixes pass.

**How:** use a separately identified disposable test instance/workspace and verify the binary commit before starting. Avoid simultaneous instances sharing ambiguous bundle identity. Use Computer to exercise every route below, with fake/offline execution where supported; report tool failures separately. Do not restart the user's live app. Save dated screenshots and a click/expected/actual ledger.

**Acceptance matrix:**

| Journey | Required cases |
| --- | --- |
| Workspace | New/open, invalid folder, lock conflict, save failure/retry, backup/restore |
| Research | Global/project/topic creation, saved request, planning failure/retry, approval, List/Board, drag, queue, stop/resume, selective rerun |
| Automation | Quick/Research, all schedule kinds, timezone/preview, draft/edit/replan/enable, pause/resume schedule, manual run, retry period, queued/failed/skipped/recovering |
| Navigation | Every sidebar item, breadcrumbs vs Back, period→step→return, Previous/Next, recently opened, global search and shortcuts |
| Results | Final/intermediate/live/partial/stale, history/filter/unread, sources, copy/export, regeneration, technical details |
| Settings/materials | Agent/models/inheritance/profiles, paths, attachments paste/drop/remove, disabled and missing-config explanations |
| Archive/publication | Archive/restore semantics; offline publish states and exact-version verification; real remote publish only in a separately authorized test destination |
| Visual/keyboard | 1440×920 and 1000×680; both themes; long titles; keyboard/focus; readable status text |

For every case record Passed/Failed/Blocked/Not run, expected destination/effect, actual result and evidence. Release sign-off must not describe a blocked/not-run case as passed. No unresolved P1, no unexplained test failure, and no accidental change to the user's real scheduled work is allowed.

## T21 — Explain and control research-input invalidation

**Priority/size:** P2/M. **Finding:** F19. **Dependencies:** none; coordinate with T06/T13.

**Do:** make edits to approved/completed work predictable without weakening approval checks.

**How:** classify changes as presentation metadata or execution input. If a title is still included in the agent prompt/signature, show the resulting approval/current-result impact before Save; do not silently exempt it without changing the input model. Build an affected-step preview for description, comment and material changes. Keep previous reports in history with input revision and “Previous version” labels. Separate viewing/exporting historical results from permission to rerun.

**Acceptance:** a completed goal's edit screen explains whether Save requires renewed approval and which results become stale. Saving labels/priority that do not affect prompts preserves completion. Input changes invalidate only the appropriate graph; old reports remain viewable under their original revision. Undo restores the edit and approval consistently without rolling back unrelated runtime events. Test title-only, description, material add/remove and comment changes.

## T22 — Present archived research as archived history

**Status:** accepted in the independently reviewed working-tree candidate; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t22-independent-acceptance). Final native and integration release gates remain.

**Priority/size:** P2/M. **Finding:** F20. **Dependencies:** T05; coordinate with T06.

**Do:** prevent Archive from looking like unapproved new work.

**How:** add lifecycle-aware presentation before deriving execution eligibility. Preserve historical completion/outcome independently of whether `research_result` is currently executable. Replace archive Board CTAs with Restore; suppress approval and execution controls. Keep goal and step results accessible as archived history. Choose and document whether sidebar counts mean goals or all entities; use the same unit in the page heading.

**Acceptance:** archiving a completed three-step goal shows Archived with its retained 3/3 outcome in List, Board and detail. No Approve research or Run action is available. Restore returns the original valid completion; archive itself does not require reapproval. Archiving active work still stops admission and preserves the existing stop confirmation. Stored report content is unchanged.

## T23 — Persist run ownership across planning attempts

**Status:** accepted in the independently reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t23-and-t25-independent-acceptance). Integration and release gates remain required.

**Priority/size:** P2/M. **Finding:** F21. **Dependencies:** none; coordinate with T12/T25.

**Do:** keep every automation planning attempt out of ordinary Research results and align all selection surfaces.

**How:** persist immutable request/automation ownership on each run, or an immutable request-attempt relation. Do not infer ownership solely from a request's latest `planning_run_id`. Introduce one selection policy used by `runs`, `displayed_run_id`, breadcrumbs, history and sidebar counts. Migrate old records conservatively using project kind and preserved request history; retain ambiguous history visibly rather than deleting it.

**Acceptance:** generate a Research automation plan, replan twice, restart, then open Research results. No automation planning/run appears there. All attempts remain accessible from the automation. Content, title, breadcrumb and history refer to the same displayed run after fallback selection. Ordinary request retries remain discoverable. Test both older and current schema fixtures.

## T24 — Reconnect automation workspaces using the correct directory policy

**Status:** accepted in the working tree on 2026-11-08; not committed or released. See [independent review and verification](IMPLEMENTATION.md#t24-independent-review-and-acceptance). Baseline T01 failures remain open.

**Priority/size:** P1/S–M. **Finding:** F22. **Dependencies:** none.

**Do:** allow explicit reconnection of workspaces created by the current app.

**How:** centralize allowed directory roots by `ProjectKind`: ordinary projects under the applicable project root; the generated automation project under the automation root. Reuse this policy in creation, reopening, migration and relocation. Keep canonical-path/symlink/parent traversal checks; do not solve this by accepting arbitrary external directories. Return an error naming the invalid project and required root for genuinely invalid data.

**Acceptance:** create an ordinary project plus Quick and Research automations, close, remove only the disposable bootstrap-location file, and reconnect the same root through Set workspace. All data loads without path rejection. Repeat after relocating the entire portable workspace. Ordinary-only reconnect still passes; external/symlink-escape paths still fail; no original files are overwritten. Add an isolated regression to onboarding/workspace QA.

## T25 — Bind replanning to the edited request revision

**Status:** accepted in the independently reviewed working-tree candidate on 2026-11-08; not committed or released. See [independent acceptance](IMPLEMENTATION.md#t23-and-t25-independent-acceptance). Integration and release gates remain required.

**Priority/size:** P1/M. **Finding:** F23. **Dependencies:** none; coordinate schema ownership with T23.

**Do:** make Replan actually plan the current objective and permit its reviewed result to be enabled.

**How:** introduce immutable planning-request revisions or create a new request when the objective changes. Associate the resulting template and approval with that exact revision and the relevant expected-output contract. Keep old runs and frozen periods associated with their original revision. Do not mutate a request underneath an in-flight planner. Handle changed input while planning by preserving the draft and marking the returned plan stale.

**Acceptance:** create a Research automation with objective A, open Replan, change to clearly different objective B, generate and inspect the captured fake-agent prompt. It contains B; the generated plan is bound to B; Enable succeeds without another Replan instruction. Previous periods retain A. Repeated unchanged-input replan remains valid, and concurrent edits cannot approve a stale response. Failed planning preserves the editable B draft and offers retry against B. Expected-result text is part of that immutable planning revision and is included in the captured planner context; changing only the expected result requires a matching reviewed plan and cannot approve a response bound to the previous contract. Legacy requests without this field remain readable and are not silently assigned new historical inputs.

## Definition of done for the program

All findings have a linked disposition: implemented, deliberately accepted with rationale, or superseded by a documented decision. The chosen contracts are reflected in current documentation and tests. Background events do not move the user; actions affect exactly their named scope; statuses match execution; durability and report quality are represented truthfully. Keep benchmark and native acceptance evidence with the implementation rather than relying on this baseline review as proof of future behavior.

## Finding to task traceability

| Finding | Primary implementation task | Related work |
| --- | --- | --- |
| F01 | T02 | T12 |
| F02 | T03 | T05 |
| F03 | T04 | T13 |
| F04 | T05 | T06 |
| F05 | T06 | T05 |
| F06 | T06 | T11 |
| F07 | T07 | T12 |
| F08 | T08 | T12 |
| F09 | T09 | T10 |
| F10 | T10 | T12 |
| F11 | T11 | T19 |
| F12 | T14 | T15 |
| F13 | T16 | T23 |
| F14 | T17 | T11 |
| F15 | T01 | T20 |
| F16 | T18 | T17 |
| F17 | T19 | T11 |
| F18 | T16 | T01 |
| F19 | T21 | T06/T13 |
| F20 | T22 | T05/T06 |
| F21 | T23 | T12/T25 |
| F22 | T24 | T20 |
| F23 | T25 | T23 |
