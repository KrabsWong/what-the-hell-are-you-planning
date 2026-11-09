# Architecture, behavior and interaction review

## Current correction notice — user commits through 98b71d4

[POST_DELIVERY_REVIEW.md](POST_DELIVERY_REVIEW.md) supersedes the broad UX acceptance below for ordinary completed-result navigation, asynchronous report loading, visual hierarchy/layout and sidebar scrolling. The user's corrections reveal missed common cases and incorrect review prescriptions; they are not all new requirements or rare edge cases. Current source and added QA assertions were reviewed, but this documentation update did not independently rerun tests or native acceptance. V01–V06 define the remaining targeted verification. Historical passes remain valid only for their recorded source and scope.

Current first-round delivery and follow-up disposition: [DELIVERY.md](DELIVERY.md). Original requirements and findings below remain traceable; historical defects are not automatically current blockers.

Review date: 2026-11-08 (Asia/Shanghai). Source baseline: `611a4586eedb56600a83c8b272a3af43ba95e4a4`. Working tree was clean at the start. This review changes documentation only.

Read [TASKS.md](TASKS.md) for the authoritative remediation backlog, target interaction contracts, dependencies and acceptance criteria. Findings describe the baseline, not completed fixes. Source links identify relevant functions; line numbers in prose refer to this baseline and may move.

## Executive assessment

The product has a credible execution foundation: frozen automation approval, separate concurrency pools, deterministic schedule arithmetic, transactional scheduling, per-period identity, recovery verification and current-result validation. Keep these foundations and the common agent execution path. A rewrite or separate automation execution engine is not warranted.

Explicit workspace reconnection also rejects the app's own automation storage layout (F22), and changed-objective replanning reuses the original request (F23). Both need correctness fixes before further workflow expansion.

The highest-priority problems are boundaries between background execution and navigation, between a period and its schedule, and between reversible user edits and durable runtime state. The same entities are reused successfully for execution but their generic navigation and status presentation leak into automation surfaces. The app therefore sometimes communicates a different action or state from the one the runtime implements.

Large files are a symptom, not the sole reason to refactor. `TaskApp` owns routing, drafts, process queues, persistence and feedback; `Store` is both the persisted aggregate and the undo unit. Extract these responsibilities incrementally around the failing invariants below. In particular, do not merely split an `impl TaskApp` across more files while retaining the same uncontrolled state writes.

## Evidence and limits

Evidence labels:

- **UI + code:** observed through Computer, with a matching implementation path.
- **Code-confirmed:** a concrete control flow or data transformation establishes the issue; not reproduced against the user's live data.
- **Measured:** a command or isolated fixture produced the result.
- **Design proposal:** a target behavior decision, not a claimed defect.
- **Risk / unverified:** needs a dedicated reproduction before being called a product failure.

Computer was used on the already running `/Applications/Planning.app`. There was a real automation in progress. No live run was stopped, no automation was enabled, no publishing was requested, and no real research was submitted. A temporary creation input was entered and cancelled without saving. Opening a period can mark it read through the application's normal behavior. The installed binary's commit was not established; source/QA findings are tied to the commit above, while live observations are explicitly distinguished.

The live walkthrough covered the automation goal board, Back to the period dialog, Escape dismissal, automation settings entry/dismissal, the automation-results entry, automation list, Quick/Research type selection, schedule form, wizard Back with input retention, cancellation, All research and the New research shortcut. The period dialog and creation form were visually inspected. Later, Computer returned stale screenshots inconsistent with its changing reported UI state and coordinate calls failed with `noWindowsAvailable`. Rebinding by path, resetting the session and raising the window did not resolve it. This is a **tooling limitation**, not proof that the application stopped rendering. The original application process remained running.

The follow-up review resolved the Computer ambiguity with a uniquely named **Planning Native Review** app and an independent workspace. It used the baseline UI/domain/runtime code, with only package/window identity, bootstrap location and synthetic agent seeding changed in an ignored source copy. The original app was not restarted. Computer then exercised all major native feature areas, including first-run setup, creation, execution, recovery inspection, settings, materials and archive flows. See [COVERAGE.md](COVERAGE.md) for the complete feature inventory, actual outcomes and remaining acceptance boundaries. The fixture does not establish real-provider research quality or remote-publication correctness.

Full coverage here means every feature area and every offline QA entry point is accounted for with evidence and a disposition. It does **not** mean every state permutation passed native acceptance. Failures, unavailable external-service checks and unverified keyboard and layout behaviors are explicit release gates, not silently counted as passes.

### Validation ledger

See [evidence/VALIDATION.md](evidence/VALIDATION.md) for final command outcomes and retained logs. Initial unit run: 289 passed, 1 failed, 2 ignored. The failure is `report_git::tests::repository_index_preserves_readme_and_is_excluded_from_build`: spawning `node` returned `NotFound` at `src/report_git.rs:398`; `node` was not on this execution environment's PATH. This is an environment prerequisite gap, not evidence that the publishing algorithm is wrong.

Automation, workflow and projects QA passed. Automation includes fake-agent execution, plan freezing, restart/recovery, regeneration deadlines, queue routing, separate pools, and light/dark screenshots at 1440×920 and 1000×680. The icon check passed with 51 explicitly registered Lucide assets. Other QA failures are recorded independently; a passing automation suite does not validate the entire application.

### Coverage map

The authoritative detailed coverage map is [COVERAGE.md](COVERAGE.md). All 33 offline `--qa*` entry points in `src/main.rs` were invoked: **26 passed, 7 failed** after supplying the required workspace for the render benchmark. Counts include aliases and performance entry points; they are not 33 independent functional suites. The unit-test dependency failure is separate. The seven failing entries are affordance, research, workspace, recovery, publishing, topics and simplify.

## Findings

### F01 — Background run submission changes the visible page (P1, native UI + code)

`TaskApp::launch_run` in [app.rs](../../src/app.rs), around lines 1430–1448, sets `selected = None` and changes `scope` to `AutomationRuns` or `Runs`. It is not restricted to a user-initiated navigation intent. `poll_automations → enqueue_automation_occurrence → launch_run` uses this path on a timer; `automation_after_run` uses it again when the next step becomes ready. The Recovery exception is not a general protection for other pages.

Scenario: read a research report or edit settings while a scheduled period begins or advances. A background event can replace the content with results navigation. This directly violates the requested “click A, arrive at A” expectation even without another click. Do not infer that a saved draft was deleted; the confirmed defect is the implicit routing side effect. Native reproduction: start the three-step ordinary fixture goal, click All automations while collection runs, and observe a later step replace the page with Result. Fix: **T02**, then **T12**.

### F02 — Period Resume also resumes future scheduling (P1, native UI + code)

[automation_views.rs](../../src/automation_views.rs), `resume_automation_steps` at line 1283, calls `resume_automation_ui` for Paused/NeedsAttention definitions before continuing/reopening the selected occurrence. That helper enables the definition and arms the timer. A user wanting to retry one failed period can unintentionally restart future runs. If reopening subsequently fails, the earlier schedule resume has already happened.

Native reproduction: cancel Period 3 during retry wait, pause its definition, then Resume inside the period. Returning to the definition shows Pause, proving the schedule is enabled again. The existing doc comment also claims terminal periods create a new manual period, while implementation now reopens in place. Fix: **T03**. Preserve completed steps and the period's original reference time.

### F03 — Generic Undo can roll back unrelated automation changes (P1, code-confirmed)

[app.rs](../../src/app.rs), `snapshot` and `undo_change`, saves/restores the whole `Store`. Undo preserves runs/publications/publishing settings and the task ID high-water mark, but does not preserve automation definitions, occurrences or their cursors. Automation save/enable/pause/resume actions in [automation_views.rs](../../src/automation_views.rs) neither create a matching undo entry nor invalidate an older one.

Concrete sequence for a regression: make an undoable ordinary task edit; create/save a Quick draft or change an existing automation's schedule before any run starts; click Undo. The old Store replaces the newer automation state as well as the intended task edit. Launching a run clears Undo, so do **not** generalize this to every running-task scenario. Even without execution, unrelated draft loss and schedule rollback are unacceptable. Fix: **T04**, then **T13**.

### F04 — Displayed readiness does not include automation phase gating (P1, UI + code)

Live automation Period 10 showed collection still running, but an analysis card said `Todo · Ready` and “Run the goal or add its topic to the queue.” [automation_execution.rs](../../src/automation_execution.rs), `phase_ordered_ready_step`, correctly waits for all earlier phases. [execution_status.rs](../../src/execution_status.rs) and [workflow.rs](../../src/workflow.rs) derive generic readiness and Research queue instructions without the same automation eligibility rule.

The observed problem is misleading status/action guidance, **not observed out-of-order execution**. Waiting for an earlier phase, waiting for dependencies, queued for an automation slot, paused scheduling and a failed step must remain separate concepts. Fix: **T05**.

### F05 — Period result content can actually be a live step message (P1, UI + code)

[automation_views.rs](../../src/automation_views.rs), `occurrence_report` at line 1508, falls back from `final_run_id` to the latest nonempty run, without requiring a completed report. `automation_occurrence_detail` places its raw output in the main report panel. Computer observed an in-progress collection commentary sentence in this panel, above the step list, without a clear “Live progress” classification.

Useful partial progress should remain accessible, but “result” must distinguish validated final output, completed intermediate evidence and streaming messages. The main period summary must not let an unfinished step impersonate the answer. Fix: **T06**.

### F06 — Automation reports bypass the shared reader transformation (P2, code-confirmed)

The period dialog passes `run.output.clone()` directly to Markdown at [automation_views.rs](../../src/automation_views.rs):2450. [research.rs](../../src/research.rs) provides `reader_markdown` to remove the internal evidence-gap handoff marker. Consequently a partial automation result can expose internal downstream instructions rather than the reader notice used by other report surfaces. The global reader notice itself is hardcoded Chinese despite English-only app content; user-authored/generated research language must still be preserved.

Fix: **T06**. Share a report presentation model, not another string-strip implementation.

Native partial-result check: Quick Period 4 returns `partial`; its dialog and list say Succeeded, the step has a success check, and the evidence-gap notice is a neutral Markdown quotation. The caveat exists, but quality is not differentiated at the entry point. Keep execution success and evidence completeness as separate fields in T06.

### F07 — Previous/Next crosses automation periods (P2, native UI + code)

[navigation.rs](../../src/navigation.rs), `detail_task_neighbors`, includes every task in the same project and archive state and wraps at both ends. All automation periods share the generated Automations project. The task toolbar uses these neighbors unchanged. A step opened inside one period can therefore jump into another period or another automation, with the generic label “Next task.”

Fix: **T07**. Default to the period's ordered steps for automation; make a cross-period action explicit. The existing cross-topic Research behavior is intentional in tests and should only change through an explicit product decision.

Native reproduction: from Quick Period 4's final step, Previous opens its goal; Previous again opens the final step of **Research Period 2 in a different automation**. The boundary is broader than adjacent periods of one definition.

### F08 — Search has inconsistent domain behavior (P2, native UI + code)

[views.rs](../../src/views.rs), `focus_search` at line 612, routes `Scope::Automations` to `Scope::All` but omits `Scope::AutomationRuns`. The automation-results page has no matching task search field. The same footer/⌘K action can take an automation user to ordinary Research, or focus a hidden input while leaving automation results visible.

Fix: **T08**. Specify one global-search contract or explicitly name contextual searches; do not silently switch domains. Computer's later search state is not treated as a reliable visual reproduction due to the tooling problem.

Native reproduction: click the footer Search while All automations is visible; it immediately routes to All research and its task search field.

### F09 — A new automation inherits pieces of the previous form (P2, native UI + code)

`start_automation_create` resets text fields, shape and step, but not `kind`, `interval_in_hours`, `catch_up` or `weekdays`. `edit_automation` populates those fields from the previous definition. After editing a weekly/SkipAll or hourly automation, “New automation” starts with a mixture of old policy and reset values. For example the reset interval value `10` can retain hours as its unit.

Fix: **T09**. Construct a fresh form model from explicit defaults; returning to an existing draft is a distinct action.

Native reproduction: edit a Quick task to a two-hour interval, finish it, create a new Research automation, and advance to Schedule. The new form starts with Fixed interval / Hours and the reset numeric value 10, instead of a coherent default schedule.

### F10 — Draft exit and Back lack a consistent return contract (P2, code-confirmed + design proposal)

`navigate`/`clear_navigation_overlays` discard several edit states. New automation creation resets input when reopened. Computer confirmed wizard-step Back retained typed input, but Cancel left immediately. That is acceptable only with a clear unsaved-draft policy. Structural Back is explicitly the current implementation, not browser history. A step opened from filtered Automation results returns to the owning automation detail/period, losing the originating results context by construction.

`open_research_intake` also navigates to a chosen/default project before opening the modal; cancelling does not restore the caller. Computer observed global New research opening under Workspace, without an in-dialog destination selector. Fix: **T10**, supported by **T12**. Treat the difference between structural parent and return-to-origin as a deliberate product contract, not as two meanings of Back.

### F11 — Historical affordance finding; blanket-border remedy withdrawn

[controls.rs](../../src/controls.rs) makes `quiet_button` and `tool_icon` ghost controls. DESIGN requires secondary/icon action boundaries visible at rest. Computer showed unbounded Cancel, Close, Open and toolbar icons. The period cancellation control also lacks the specified danger treatment in the inspected dialog. Existing icons were embedded correctly; this is semantics/discoverability, not missing assets.

**Superseded direction:** the blanket bordered-secondary recommendation was incorrect and is withdrawn by C04. Current controls use visible ghost supporting actions/icons, with separate primary/selection/danger roles. Reserve link styling for navigation and give destructive actions an explicit tone. Avoid blindly adding a border inside compound task tabs, whose documented geometry is an intentional exception. Fix: **T11**.

### F12 — Histories are bounded visually but saves block the UI (P1, measured)

[sqlite_storage.rs](../../src/sqlite_storage.rs) serializes the complete Store and deletes/reinserts entity-table rows per save. [app.rs](../../src/app.rs), `persist_if_changed`, performs the save synchronously; it also compares/clones the aggregate. A bound of 20 visible history rows is not database pagination: the store loads all rows and result views scan/sort in memory.

The existing performance fixture measured 43,200 occurrences, 100 with full research entities, representative report data and a 172.4 MB database: cold save 1,964 ms; warm saves 1,292/1,316/1,912 ms; load 634 ms; UI-triggered save block 1,350 ms. This demonstrates a scale-specific freeze, not a measured delay for every current workspace. Source fixture: [automation_qa.rs](../../src/automation_qa.rs), `performance_fixture`; retained [performance log](evidence/perf.txt).

Fix: **T14** (off-thread ordered durability) and **T15** (incremental persistence/history), without weakening save-before-dispatch.

### F13 — Latest result ordering disagrees across surfaces (P2, code-confirmed)

`latest_occurrence` chooses max ID, while history and Automation results sort by actual execution time (fallback reference time). Retrying an older period in place changes its activity time without changing its ID. The list's “Latest” summary can therefore refer to a different period from the first history row.

Fix: **T16**. Distinguish “latest activity” from “latest scheduled period” if both are needed, and use one named ordering function per meaning.

### F14 — Product copy and maintained specifications have drifted (P2, code + rendered evidence)

Examples: the automation footer says missed periods are not backfilled, although LatestOnly catch-up is implemented; PRODUCT says Quick finishes on one screen while the actual wizard has three steps; DESIGN says history is fully collapsed/no per-row fills while current history renders neutral filled rows; a period Resume comment describes a replaced implementation; strategy prose still describes a deterministic template as the general research approval path although new Research automations use generated reviewed plans.

These contradictions prevent the documents from being a reliable implementation authority. Adopt explicit target decisions in TASKS rather than making code follow every conflicting sentence. Fix: **T17**. Preserve historical change notes as history, not current guarantees.

### F15 — Regression commands no longer form a reliable release gate (P1, measured)

`--qa-affordance` fails on nonvisible `approve-topic-1`; `--qa-research` references missing `research`; `--qa-workspace` references missing `save-task`. Recovery also failed on “step retry did not finish”; that failure is **not yet classified** as timing, environment or runtime regression. Publishing QA also reproduced a View report versus Retry publishing label mismatch at `publishing_qa.rs:186`; its root cause remains unclassified. Unit publishing tests require Node but this prerequisite was absent from the current environment.

Do not “fix” these by removing assertions or skipping the routes. Fix: **T01**; retain the failing scenario, investigate recovery, and make selectors/fixtures track the actual information architecture. Additional suite outcomes belong in the ledger.

### F16 — Unattended read-only claims exceed local enforcement (P1 risk / code-confirmed limitation)

`validated_instruction` checks prohibited substrings in the objective, not all expected-output text; wording variants can evade a substring list. OpenCode uses its configured/default `build` agent; Codex uses workspace-write with network access. The time-context prompt prohibits external writes, but this review found no complete action-level enforcement in this application. Existing docs intentionally retain the shared manual/automation execution path.

This is **not a claim that an unauthorized write occurred**. Do not sell a keyword filter or a prompt as an enforced capability boundary. Fix: **T18**: document the actual guarantee, validate all user instruction fields for obvious unsupported actions, and evaluate a shared capability policy if a strict read-only guarantee is required. Do not reintroduce a separate automation agent configuration merely to mask the issue.

### F17 — Dense rows and truncated titles obscure context (P2, observed gap + proposal)

The narrow dark screenshot keeps results readable, but long names truncate, trigger type is icon-only, and period step titles use `.truncate()` despite the full-title specification. The strong colored task cards compete with content. Users need to identify the task, period and trigger without guessing from color or a shortened title.

Fix: **T11** and **T19**, with visible text trigger labels, predictable long-title disclosure, readable status text and ordinary keyboard focus behavior.

### F18 — Scheduling helper edge cases need explicit bounds (P3, code-confirmed)

[automation_schedule.rs](../../src/automation_schedule.rs), `count_between`, does not safely handle an interval range containing no trigger: period 60, anchor 0, from 61, to 62 gives start_index 1 and end_index 0, then signed saturating subtraction is cast to u64 before adding 1 (debug overflow/panic is possible). Current scheduler call sites generally pass aligned trigger points, so no production catch-up failure was reproduced. Calendar counting also silently caps at 4,096 triggers.

Fix: **T16** with boundary tests and a documented bounded-range policy. Do not call this a demonstrated ordinary schedule-dispatch failure.

### F19 — Editing research context silently invalidates completed work (P2, native UI + code; interaction change)

Native reproduction: finish a three-step goal, use More task actions → Edit task, change only its title, then Save changes. The goal and all steps change from Done to Backlog / Review plan; Export ZIP becomes disabled. Undo restores the previous completed state. Adding materials or a comment also changes the approved context.

This invalidation follows the deliberate signatures in [research.rs](../../src/research.rs), `goal_plan_signature` (391) and `research_signature_checked` (589): title, description, comments and attachments are research input. The defect is the missing **impact explanation before saving**, not proof that stored reports were deleted or that invalidation is always wrong. The editor presents Done and Save changes without explaining that approval/current-result eligibility will change. Fix **T21**: distinguish cosmetic metadata from research inputs where possible; preview affected steps and preserve explicitly labeled historical results.

### F20 — Archived completed goals display as awaiting approval (P2, native UI + code)

Archive a completed goal, open Archive, and compare List and Board. List shows Backlog / Review plan and 0/3; Board offers Approve research. Restoring the goal brings back Done and 3/3, so the results were retained. [research.rs](../../src/research.rs) rejects archived work in `goal_execution_approved` and current-result validation; [execution_status.rs](../../src/execution_status.rs) derives Approval from that result without an archive presentation override. [goal_steps.rs](../../src/goal_steps.rs) therefore also loses the historical completion count.

Archive is an admission/lifecycle state, not a request for approval. Fix **T22**: archived presentation retains the last known outcome, offers Restore, and exposes saved results without making them executable.

### F21 — Automation replanning leaks old planning runs into Research results (P2, native UI + code)

Create and run a Research automation, use Settings → Replan, finish the new draft, then click Research results. The isolated UI opens Project planning under Automations. In the fixture, automation planning run 6 remains in `runs`, while request 2 now points only to planning run 15.

[request_planning.rs](../../src/request_planning.rs), `request_for_run` (329), only matches the current `planning_run_id`. [automation_views.rs](../../src/automation_views.rs), `start_automation_planning` (932), overwrites that field. [run_views.rs](../../src/run_views.rs), `run_is_automation` (85), depends on that reverse lookup for planning runs, so the older run loses its classification. Independently, [navigation.rs](../../src/navigation.rs), `displayed_run_id` (69), excludes period runs but not automation planning runs, allowing breadcrumbs and content to disagree.

Fix **T23**: persist immutable run ownership and use one domain-filtered selection function for content, history, breadcrumbs and counts. Preserve all planning attempts under their automation.

### F22 — Reconnecting a workspace containing automations is rejected (P1, native UI + code)

**Disposition:** T24 fix accepted in the working tree on 2026-11-08; [implementation and verification](IMPLEMENTATION.md#t24-independent-review-and-acceptance). The following describes the original baseline defect.

Create a Quick task or Research automation. Close the isolated app, clear/change only its bootstrap location as a test setup, reopen, and choose the existing workspace root. Set workspace reports “Project and task folders must be inside the configured workspace.” A workspace containing only the ordinary project reconnects successfully. Direct startup with the already configured root succeeds, so this specifically affects the reconnect/setup path.

[workspace.rs](../../src/workspace.rs), `initialize` (71–85), validates every loaded project against `destination/projects`. Automation projects are intentionally created at `destination/automations` by `automation_directory` (63) and [app.rs](../../src/app.rs) (322). The validation is incompatible with the app's own storage layout. Fix **T24**: centralize directory policy by project kind, retain canonical containment checks, and test both configured startup and explicit reconnection after creating automations.

### F23 — Replan reuses the original request text after the objective changes (P1, code-confirmed)

[automation_views.rs](../../src/automation_views.rs), `start_automation_planning`, saves the edited definition, then calls `create_automation_request` with its current objective. [request_planning.rs](../../src/request_planning.rs), `create_automation_request` (270–310), returns an existing request for that automation without updating its `input`. The generated plan therefore uses the old request. `enable_automation` (1127–1150) compares the current objective with that old input and instructs the user to Replan again. Repeating the same action cannot repair the mismatch.

This is a source-confirmed changed-objective path; native Replan with unchanged input completed successfully. Fix **T25**: create a new immutable planning-request revision for a changed objective, bind the plan/approval to that revision, and preserve earlier attempts and periods. Verify the prompt contains the new objective before approving the result.

### F24 — Collection step-count disclosure changes without revealing content (P2, r141 native + code)

In Archive List, clicking the goal's `0/3` chevron changes its direction but shows no steps. The same goal's Steps page contains all three children. [Native evidence](evidence/r141-native-archive-return.json) records the reproduction. `TaskApp::visible_tasks` retains only root goals; `ordered_visible_tasks` then calls `append_children` with that goals-only list, while `task_row` still exposes an expansion action based on children in the full Store. The list therefore promises a disclosure it cannot render.

Follow-up under **T10/T11/T22**, accepted through **T20**: preserve the product's goal-only collection contract and give the count an explicit, useful action to the goal's Steps page, or remove the disclosure affordance if the count is passive. Do not silently change collection filtering/count semantics. Verify real clicks in All, Project, Topic and Archive lists, correct goal/Steps destination, Back restoring the source filter/layout/scroll, and readable Light/Dark controls. This is a navigation-contract repair; no persistence or execution change is needed. It can proceed independently of dispatch repair after the current modal-focus patch is isolated.

### F25 — Material verification can stop progressing while import reports success (P1, r146 native + code)

On the r143 isolated normal build, choose Add Files on a paused research goal and select a valid local PNG. The file dialog closes and the app says “materials included in the next run,” but Materials remains 0 and the required input-impact review is not shown. A later screenshot was blocked by Mac lock, so the final import outcome is not asserted. [Evidence](evidence/r146-focused-checks.json) preserves this observation and its limits.

`poll_runtime` polls material jobs, but its keep-alive predicate omits `pending_material_job`. `poll_research_io` clears the import receiver and submits verification; with no other active work the pump may stop before that response arrives. Material submissions also lack an explicit pump wake. Independently, the success toast is emitted after submitting verification, before impact approval and durable save.

Repair under **T14/T21**, with **T20** acceptance: include active verification/cleanup work in the bounded runtime lifecycle and wake it from idle. Waiting for user approval must remain quiescent. Show truthful pending/review feedback and reserve saved-success feedback for the durable receipt. Add executor-driven tests that start from idle without manually polling: import → verification → impact review; Cancel → cleanup → idle; confirm → exact disk attachment → saved state. Native acceptance must repeat these flows and verify material count, approval/result impact and restart persistence. This depends on the in-progress runtime wake/receipt repair; it does not require changing the attachment file format or collection-navigation work.

## Findings added after user corrections

These are baseline-at-aee91f9 misses, with repairs already present at 98b71d4. Source-reviewed does not mean independently accepted. Detailed code evidence, review responsibility and V01–V06 acceptance are in [POST_DELIVERY_REVIEW](POST_DELIVERY_REVIEW.md).

| Finding | Missed behavior or wrong direction | Original ownership | Current disposition |
| --- | --- | --- | --- |
| F26 | Ordinary completed sidebar/neighbor target opens details or misses lazy persisted results | T06/T07/T15 | C01 implemented; V01 pending |
| F27 | Async result navigation lacks atomic commitment and latest-intent cancellation | T02/T08/T10/T12/T15 | C02 implemented; V02 pending |
| F28 | Lazy or failed body read can be presented as no report | T06/T14/T15 | C03 implemented; V02 pending |
| F29 | Reader width, wrapping header actions, Regenerate emphasis and stacked Status violate expected layout/hierarchy | T11/T19 | C05/C06/C07 implemented; V03/V04 pending |
| F30 | Brand/name scroll with navigation; QA enforces that wrong invariant | T11/T19 | C08 implemented; V05 pending |
| F31 | Wheel redraw repeats status validation and benchmark forces unrelated full-window work | T14/T15/T19 | C09 implemented; V05 pending; native smoothness not proved |
| F32 | English-only audit misses research placeholders | T17 | C10 corrected in source; V06 string check |

## Architecture boundaries to extract

| Boundary | Current coupling | Recommended responsibility |
| --- | --- | --- |
| Navigation | Scope, selected task/run, overlays, result context and launch effects | Typed route and explicit user navigation intents; background events cannot navigate |
| Commands/durability | Mutation + Store clone + save + rollback repeated across UI/runtime | Validated domain command; durable commit acknowledgement; effects after commit |
| Execution eligibility | Generic statuses vs automation phase gate vs period UI overrides | Shared eligibility result with reason and owner/pool identity |
| Results | Raw run output vs current result vs intermediate/final report | Typed result presentation with quality, freshness, phase and sources |
| Undo | Entire Store snapshot | Reversible user edit commands, excluding durable runtime facts |
| Storage | Whole aggregate on every write, all history resident | Ordered async writes first; incremental indexed storage second |
| Agent adapters | OpenCode owns shared process plumbing and starts Codex | Shared process transport + agent-specific adapters; preserve one execution pipeline |

Large-file reference: `views.rs` 3,884 lines; `automation_views.rs` 3,269; `app.rs` 2,122; `automation_runtime.rs` 1,991; `automation_execution.rs` 1,984. Splitting by these boundaries is useful because it prevents the concrete failures above. UI and process code should not be generalized merely to reduce line counts.

## What should be retained

Keep approved-version snapshots, in-place period retry, original reference times, idempotent trigger keys, separate Research/Automation pools, phase-ordered automation dispatch, recovery verification before replay, explicit publish verification, original attachment delivery and source/evidence validation. Automation QA demonstrates substantial value here. Preserve warm neutral themes, the two-domain sidebar and progressive disclosure of diagnostics. No branding redesign is recommended.

## Retained visual references

See the [validation ledger](evidence/VALIDATION.md) for four inspected light/dark screenshots and command logs. These are disposable-fixture evidence, not private live-workspace captures.
