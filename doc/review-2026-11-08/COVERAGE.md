# Full feature coverage and native review evidence

Baseline: `611a4586eedb56600a83c8b272a3af43ba95e4a4`. Review: 2026-11-08. This ledger supersedes the first review's partial-walkthrough coverage statement. [ISSUES.md](ISSUES.md) records defects; [TASKS.md](TASKS.md) is the implementation authority. Every current feature area is accounted for below. Coverage is not release approval: a reproduced defect is covered but failed.

## Evidence model

- **Native verified:** Computer performed the action and observed its resulting state in the isolated app.
- **Native defect:** Computer reproduced the unexpected result; the finding links to the implementation.
- **Automated:** an offline QA or unit test exercises the case. This does not mean a human-style pointer/keyboard path was tested.
- **Inspected:** the UI or source was inspected; the terminal effect was not exercised.
- **Boundary:** the exact remaining external, environmental or keyboard/layout check is specified. It is not a pass.

Native acceptance used `/Users/fuxingxing/workspaces/what-the-hell-are-you-planning/.local/native-review/Planning Native Review.app`, bundle `com.krabswong.planning.native-review`. Its source copy preserved all UI/domain/runtime logic. Only Cargo package identity, window title, bootstrap configuration path and initial fixture settings differed. Build succeeded, and the local bundle was ad-hoc signed and verified. The original `/Applications/Planning.app` remained running and was not restarted. Its existing automation was not stopped or changed by these tests.

The isolated workspace contains synthetic goals and reports. The fake CLI returns a three-step plan and source-linked Markdown; it can simulate success, slow execution, temporary failure and partial evidence. The isolated delta is retained as [native-harness.patch](evidence/native-harness.patch); [fixture agent](evidence/fixture-agent.txt), [fixture browser](evidence/fixture-browser.txt) and [artifact integrity](evidence/native-artifacts.txt) make the scope reviewable. No external research was performed. A fake browser command supports startup checks, not actual web browsing. All product interactions used Computer; shell operations only prepared fixtures, built the app, ran existing QA, inspected saved artifacts and wrote review documents. Bootstrap-location changes were explicit setup for missing-workspace/reconnect scenarios, not simulated UI successes.

## Feature and action matrix

| ID | Feature / expected behavior | Native result | Other evidence and disposition |
| --- | --- | --- | --- |
| W01 | First launch, empty workspace, readiness UI | Verified setup screen; fresh absolute folder created and opened | onboarding QA passed |
| W02 | Invalid relative folder must not initialize | Remained in setup after Set workspace | Source validation in workspace.rs; onboarding QA |
| W03 | Missing configured workspace must offer recovery | Setup returned after isolated bootstrap pointed to a missing root | No user workspace configuration changed |
| W04 | Reconnect existing ordinary workspace | Verified successful reconnection | Control for W05 |
| W05 | Reconnect workspace with automations | **Defect:** own automation directory rejected | F22 / T24 |
| W06 | Workspace lock, path migration, backup and save errors | No native lock contention or disk-full injection | Persistence/workspace unit tests and source reviewed; onboarding and topic-migration pass; workspace QA fails before completing, T01/T20 |
| P01 | Create and rename project | Verified Review secondary → Review navigation | projects/intake QA passed |
| P02 | Working folder settings and containment | Opened editor; /tmp rejected; cancelled | Canonical containment checked in workspace.rs |
| P03 | Workspace menu, reveal/copy path | Menu inspected | OS reveal/clipboard terminal effects not verified; T20 |
| P04 | Topic rename | Verified Reviewed topic in title and sidebar | topic-actions/topic-migration QA passed |
| P05 | Move whole topic between projects | Verified destination picker and move; identifiers/project breadcrumb updated | No partial child-only move |
| P06 | Expand/collapse project, topic and step hierarchy | Verified hierarchy across create, move and restart | sidebar and board-topics QA passed |
| R01 | New research from global/project/topic entry | Global and project creation opened; topic entry inspected | intake QA covers repeated requests and required topic |
| R02 | Empty request validation | Inline required-input error observed; no plan launched | Required-input validation observed; no execution started |
| R03 | Save request for later | Verified Awaiting planning topic and later Generate plan | intake QA passed |
| R04 | Generate plan, approval gate, no immediate execution | Verified three-step plan in Backlog; disabled run/export until approval | workflow/intake QA passed |
| R05 | Planning failure, retry and schema repair | Native success path only | intake QA passed these failure/repair cases; broad research QA remains failed |
| R06 | Goal approval via status and board | Verified status menu Todo → Run goal | Board drag approval exercised by intake QA, not native drag |
| R07 | List/Board, compact rows, filters, empty/reset | Verified layout switching, Done filter, unmatched search and Reset filters | board-topics/display-order/board-scroll QA passed |
| R08 | Execute goal and dependency chain | Verified ordinary three-step completion | F01 reproduced when navigating away during execution |
| R09 | Project/topic queues, parallelism, independent pools | Native goal execution and concurrency settings; project action inspected | batch, topic-queue, concurrency and automation QA passed |
| R10 | Pause after task and Continue | Pause after task observed; remaining steps became Paused | batch QA covers continuation; native restart intentionally interrupted the fixture |
| R11 | Stop/cancel/retry execution | Automation Cancel period and Resume verified | Ordinary stop/resume covered by batch/retries; recovery QA fails, not waived |
| R12 | Restart and interrupted-session guard | Restart preserved workspace; Paused projects showed session checking and blocked prior step | Fixture has no recoverable provider session, so native successful session recovery is a boundary |
| R13 | Edit completed goal, Undo | **Defect in impact communication:** title edit silently invalidated completion; Undo restored it | F19 / T21; signatures explain behavior |
| R14 | Priority, state and project property menus | State and project relocation used; priority/details controls inspected | workflow and topic-actions QA; exhaustive keyboard interaction remains T19 |
| R15 | Comments/activity | Verified Comment added after submitting local comment | Approval invalidation contract covered by F19/T21 |
| M01 | Add PDF through native picker | Verified repository supplement.pdf import | attachments QA passed |
| M02 | Remove material and invalidation feedback | Verified Remove from goal; material count returned to zero | Undo advertised by UI; attachments QA covers original bytes and inheritance |
| M03 | Paste/drop images, scanned/blank PDF, limits | Not manually dragged/pasted into native window | attachments QA and material source; workspace QA failed, do not claim every material path passed |
| M04 | Open material in default app | Action inspected | OS document handoff not verified in this pass; T20 |
| A01 | New Quick / Research choice and required fields | Verified both types and empty form validation | automation QA passed |
| A02 | Daily / weekdays / fixed interval | Verified all controls, weekday toggle, Hours/Minutes and schedule transition | calendar/automation QA; DST and missed-time behavior use simulated clocks |
| A03 | Timezone and schedule preview | Invalid zone blocked continuation; valid zone proceeded; next three triggers displayed | automation schedule unit tests |
| A04 | Catch-up options and advanced limits | Both policies and advanced fields inspected | automation QA covers policy semantics; F14 copy mismatch remains |
| A05 | Fresh form defaults | **Defect:** Hours/interval state inherited from prior form | F09 / T09 |
| A06 | Quick draft → Review and enable → enable | Verified saving did not run; enable exposed Pause/Run manually | Three-step review is deliberate target contract |
| A07 | Research plan generation and frozen review | Verified plan review and multi-step period completion | automation QA covers frozen snapshots |
| A08 | Settings/Edit/Replan | Verified unchanged-objective replan and saved Draft | Changed-objective replan is source-confirmed defective, F23/T25; do not mark that path passed |
| A09 | Pause/resume future scheduling | Pause verified; manual run available while paused | automation QA checks cursor behavior |
| A10 | Manual run and period identity | Verified separate Quick/Research periods and history | Reference time and Manual run labeling visible |
| A11 | Failure → waiting to retry → cancel → resume | Verified simulated service failure, retry wait, cancellation and successful same-period recovery | Retry button said Running while waiting; include in T05 status contract |
| A12 | Retry one period while schedule remains paused | **Defect:** period Resume enabled scheduling | F02 / T03 |
| A13 | Queued/skipped/expired/overlap/sleep catch-up | No actual OS sleep or wall-clock DST experiment | automation QA and unit tests exercise simulated time, queue and recovery states |
| A14 | Log cleanup | Preview expanded; destructive cleanup was not executed natively | Source/automation QA; release test uses disposable logs and verifies report retention |
| N01 | Every sidebar destination | All automations/results/research/projects/paused/research results/archive opened | Automation definition cards opened with pointer input |
| N02 | Breadcrumbs and Back | Verified project/topic/goal/step drilldown; period step Back reopened its period on definition page | Origin versus structural-parent inconsistency: F10 |
| N03 | Previous/Next | **Defect:** Quick Period 4 → its goal → Research Period 2 of another automation | F07 / T07 |
| N04 | Recently opened | Verified menu targets and return to chosen step | Scope/context restoration remains T12 acceptance |
| N05 | Footer Search | **Defect:** All automations → All research | F08 / T08 |
| N06 | Keyboard entry, Escape and dialog focus | New research shortcut and Escape exercised; native input/selection worked | Complete Tab/Shift-Tab matrix not passed; T19 |
| O01 | Final report, sources and technical details | Verified completed ordinary/automation reports; expanded Sources; opened logs and saved output | rich-markdown/result-layout QA passed |
| O02 | Live/intermediate/partial distinction | Partial Quick result says Succeeded with neutral evidence-gap quote; live period can show intermediate output | F05/F06 / T06 |
| O03 | Copy result/log/output actions | Menus and buttons inspected | Clipboard contents not verified; simplify QA fails before completing all actions |
| O04 | ZIP export | Native Save dialog completed and file created; archive integrity inspected | research_export unit coverage |
| O05 | Regeneration/selective rerun/stale results | Controls inspected; stale state observed after editing/materials | intake/retries/automation QA passed selective rerun; never conflate old report with current evidence |
| O06 | Result filter and read/unread | Filter control inspected; Mark unread/read toggles verified | automation QA covers filtered history |
| O07 | Planning-history domain | **Defect:** replan leaves an older automation planning run visible in Research results | F21 / T23 |
| S01 | Theme and research/automation concurrency | Light/Dark switched; limits raised/lowered; settings persisted on restart | settings/automation QA cover both sizes/themes |
| S02 | Profiles/create/override/restore | Created Review profile, changed stage model and agent, restored saved settings | settings QA covers profile inheritance/deletion/restart; native profile deletion not executed |
| S03 | Model catalog/runtime checks | Refresh invoked; fixture version visible; browser diagnostic control inspected/invoked | Real provider catalog/network/browser session is an external boundary |
| S04 | Publishing configuration | Opened fields and automatic-publishing explanation; invalid empty save exercised | Real GitHub target not configured; no remote push |
| H01 | Archive/restore goal | Verified reversible lifecycle and retained results | **Defect:** archived Done rendered as Backlog, 0/3 and approval CTA; F20/T22 |
| H02 | Archive/restore topic | Verified disabled creation while archived and successful Restore topic | topic-actions and archive unit coverage |
| H03 | Publication/export HTML, retry and exact version | Publish controls inspected; HTML/Git source reviewed | publishing QA failed; Node-dependent unit failed; release remains blocked on T01/T20 |
| V01 | Light/Dark native screens | Inspected settings, boards, wizards, result dialogs and setup | Low-contrast metadata, neutral ghost actions and status separation addressed by T11 |
| V02 | Small-window/long-title layouts | Native resize attempts did not establish a different window size | Headless 1000×680/1440×920 screenshots inspected; native resize remains a boundary |
| V03 | Keyboard focus and compact-row readability | Long titles truncate; trigger type is icon-only; repeated Open actions require nearby context | F17/T19; complete keyboard focus matrix remains unverified |
| X01 | Storage scale and render performance | Native workspace operations exercised | 43,200-period save: 1,350 ms UI block; small fixture render: 2.06–3.17 ms/frame; F12 |

## Native reproduction records

These records specify observed effects rather than claiming that every emitted screenshot was saved as a file. Computer screenshots and reported UI states are retained in the review conversation. The independent headless images and command logs are linked in [evidence/VALIDATION.md](evidence/VALIDATION.md).

| Record | Action sequence | Observed outcome |
| --- | --- | --- |
| NATIVE-01 | Generate ordinary plan → approve Todo → Run goal → All automations | Later dependency launch redirected to Result; F01 |
| NATIVE-02 | Done goal → edit title → Save changes → Undo | Backlog / Review plan / export disabled, then Done restored; F19 |
| NATIVE-03 | Done goal → Archive → Archive List/Board → Restore | 0/3 and approval controls while archived, then Done 3/3 restored; F20 |
| NATIVE-04 | Two-hour Quick → finish → new Research → Schedule | Fixed interval / Hours / 10 inherited; F09 |
| NATIVE-05 | Research Period 3 fails → cancel retry → pause definition → period Resume | Same period runs, definition changes back to enabled; F02 |
| NATIVE-06 | Research automation → Replan → Save draft → Research results | Automation Project planning shown; request mapping now points to new run; F21 |
| NATIVE-07 | Quit fixture app → missing bootstrap root → choose original workspace | Automation project path fails reconnect; ordinary-only workspace reconnects; F22 |
| NATIVE-08 | Quick partial report → open period | Succeeded plus neutral evidence-gap quote; no quality badge in entry; F06 |
| NATIVE-09 | Quick Period 4 final step → Previous → Previous | Different automation's Period 2 final step opens; F07 |
| NATIVE-10 | All automations → footer Search | All research search replaces current domain; F08 |

## External and release boundaries

The review accounts for these boundaries; none is counted as passed:

1. Real provider credentials, model output quality, browser login/handoff and provider session recovery were not exercised with the synthetic CLI. A native interrupted run intentionally remained protected by the session-check guard. Offline handoff-progress, concurrency and retry suites passed; recovery QA did not.
2. No real GitHub publication, deployment or credential flow occurred. Publishing QA and Node-dependent unit validation remain failing; an approved disposable remote is needed for a real publication acceptance test. Local ZIP creation is separate and passed.
3. Actual OS sleep/wake, timezone changes at DST boundaries, disk-full failures and lock contention were not injected through Computer. Existing source/unit/QA coverage is identified; T20 requires controlled failure injection before release sign-off.
4. Native drag-and-drop, full clipboard verification, 1000×680 resizing, all keyboard focus cycles are not exhaustively validated. Headless tests are not relabeled as native checks.
5. Seven offline QA entry points still fail. Full review completion therefore does not authorize a release; T01 and the linked behavior tasks must close first.

## Architecture coverage and extraction plan

| Boundary | Principal code inspected | Required invariant / task |
| --- | --- | --- |
| Bootstrap and persistence | workspace.rs, startup.rs, persistence.rs, sqlite_storage.rs | Project-kind directory policy; ordered durable saves; T14/T15/T24 |
| Domain and approval | domain.rs, workflow.rs, research.rs, goal_steps.rs | Separate lifecycle, approval, input revision and historical results; T05/T21/T22 |
| Research intake | request_planning.rs, project_planning.rs, project_app.rs, intake_views.rs | Request/attempt ownership and immutable revisions; T23/T25 |
| Automation definition/time | automation.rs, automation_schedule.rs, automation_runtime.rs, automation_views.rs | Draft defaults, schedule cursor, explicit retry scope; T03/T09/T16 |
| Dispatch and recovery | app.rs, automation_execution.rs, scheduler.rs, batch_app.rs, recovery.rs, retries.rs, run_errors.rs | Background events never navigate; execution pool and cancellation ownership; T02/T03/T05 |
| Agent/browser integration | opencode.rs, codex.rs, ego.rs, ego_mcp.rs, research.rs | Honest capability claims and external-service boundary; T18 |
| Navigation and presentation | navigation.rs, views.rs, run_views.rs, topic_views.rs, project_views.rs, research_views.rs, execution_status.rs | One selected-context model; domain-scoped history; T07/T08/T10/T12/T23 |
| Materials and output | attachments.rs, research_export.rs, rich_markdown.rs, report_html.rs, report_git.rs | Preserve bytes, evidence limits and publication identity; T06/T21/T01 |
| Settings and visual system | settings_views.rs, appearance.rs, controls.rs, ui_assets.rs, brand.rs | Consistent affordances, keyboard behavior and feedback; T11/T19 |
| Regression infrastructure | main.rs QA dispatch and all 33 offline entry points | Account for every failure/alias/prerequisite; T01/T20 |

The code inventory and feature matrix establish review scope; they are not a formal proof of every line or every interleaving. Keep the existing execution engine and extract typed navigation, domain commands, result presentation and asynchronous persistence around the identified failing invariants.
