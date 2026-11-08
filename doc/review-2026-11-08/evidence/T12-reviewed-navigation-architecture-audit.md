# T12 audit — typed navigation authority

Status: read-only audit and implementation/verification plan. **No source, maintained documentation, frozen snapshot, production data or running app was changed.** The only written artifact is this file. No commit or push.

Date: 2026-11-08 22:26 CST. Repository HEAD: `611a458`. Working tree heavily modified by the active program.

Task: T12 (P2/L), dependencies T02, T07, T08, T10. Authority: `doc/review-2026-11-08/TASKS.md` §T12 and target contracts 1, 3, 6, 7. Read together with `AGENTS.md`, `PRODUCT.md`, `DESIGN.md`, `IMPLEMENTATION.md`, `doc/ROUTE_ACTION_MATRIX.md`, and the T10 audit/handoff in `.local/opencode-dispatch/T10-audit.md`.

## 1. Evidence base, volatility and limits

- Source was inspected directly. Findings are labeled **[code]**, **[test]**, **[gap]**, **[proposal]**.
- **Volatility warning.** T10, T21 and T13 were actively editing `src/navigation_context_app.rs`, `src/views.rs`, `src/project_app.rs`, `src/automation_views.rs` and `src/app.rs` during this audit. `src/navigation_context_app.rs` grew from 564 to 1,121 lines while the audit ran; line numbers below are a 22:26 snapshot. **Re-anchor every item by module and symbol before editing; never apply a line-number patch.**
- A read-only `cargo check --all-targets` at 22:25 failed in three concurrent places (none attributable to T12): missing `crate::research_input_app` (`src/app.rs:207`), `save_automation_draft` still returning `()` while T10 expects `bool` (`src/navigation_context_app.rs:756`), and the matching `save_draft` call-site mismatch (`src/views.rs:3669`). T10's own 22:26 handoff (`.local/opencode-dispatch/T10-S5-S6-handoff.md`) reports a different shared blocker: T21's in-flight `research_input_views.rs` (`overflow_y_scroll` on `Div`), with 486 unit tests passing before the S5 scaffolding. Both observations are time-stamped; **the tree is mid-integration and no build, unit suite, headless QA or native walkthrough is claimed as passing in this audit.** Re-run at S0.
- No T10 end-to-end QA module or `--qa-navigation-context` dispatch flag exists yet. The decision dialog is not rendered yet. T10's `request_exit`/`decide_exit` plumbing exists in `navigation_context_app.rs`, but `views.rs` still routes Back, ⌘[, ⌘N and Escape through `go_back`/the legacy global wipe, and `global_search.rs` still uses its own dirty predicates and activation. T10 S4/S5/S6 are therefore incomplete at this snapshot. **T12 must not start the identity flip until T10's S6 is accepted.**
- The handoff is a **checkpoint, not acceptance**: root resumed T10 S5/S6 after it. Its useful, stable contribution is the published scaffold (`request_exit`/`request_back`/`request_escape`/`request_search_target`, dirty baselines, comment/composer parking, scroll handles), not the line state. T12 binds to that scaffold and waits for its final accepted shape.
- **No-line-chasing rule.** Line numbers here are evidence of what was inspected, not a patch index. The mounting surface is the symbol/contract list in §3.3; every implementation instruction below names a module + symbol + required shape. If a line moved, re-locate the symbol; if a symbol was renamed by an owner, use the cross-owner ledger in §7.5.
- No native app was launched, no user workspace opened, no screenshots captured, no test command beyond the read-only check was executed.
- `src/navigation_context.rs` (22 tests), `src/draft_exit.rs` (8 tests), `src/navigation.rs` (9 tests), and `src/global_search.rs` (6 tests) encode the current T10/T07/T08 candidates. They are useful as contracts but do not prove rendering or native behavior.
- T13 remains the owner of command preparation, persistence, dispatch and runtime launch. Nothing in this audit authorizes changing `domain_command.rs`, save semantics, timers or execution.

### 1.1 Binding constraints from the request and the accepted work

| ID | Constraint | Consequence for T12 |
| --- | --- | --- |
| X1 | T02/T07 behavior is accepted and must be preserved. | `task_navigation`, `TaskNeighbor`, `detail_task_neighbors`, `automation_period_steps`, tooltips and boundary disabling are out of scope. Previous/Next/Overview are selection actions, not new origins. |
| X2 | T08 search is implemented but awaits T10's shared origin/dirty gate. | T12 supplies the single navigation command and return frame; T08 keeps overlay content, keyboard and focus ownership. No second origin stack. |
| X3 | T10 origin/draft hooks are in flight and are the behavioral authority for Back, Cancel, Close and dirty exit. | T12 absorbs those hooks into typed state; it must not preserve `Origin` capture heuristics as the final design. |
| X4 | T13 owns commands, persistence and launch. | T12 only consumes confirmed save/dispatch results. No new save path, no runtime edits. |
| X5 | Runtime completion, timers, retries and recovery checks have no navigation authority. | Route/overlay fields become private; only explicit UI commands can call the controller. Background plan arrival may mutate a draft payload, never a route or step. |
| X6 | Behavior changes are out of scope; organization changes are in scope. | Visible destinations, Back/Close/Cancel results, filters, scroll and focus must match the accepted T02/T07/T08/T10 contracts before and after. Where current code contradicts those contracts, the *contract* wins and the expected test is corrected through a real control, not a helper. |
| X7 | A mirrored enum plus live writable legacy fields does not satisfy T12. | Every slice must end with the migrated field family removed or read-only derived. No slice may claim type safety while `pub scope`, `pub selected`, `creating`, etc. remain writable. |

## 2. Executive conclusions

1. **There is still no single navigation authority.** `TaskApp` exposes independent public route/selection/overlay fields: `scope`, `selected`, `task_result_context`, `selected_run`, `layout`, `all_status`, `query`, `goal_steps_tab`, `steps_layout`, `show_run_result`, `show_run_details`, plus `automation_ui.{selected, creating, editing, result_dialog, settings_dialog, planning_attempt_dialog, result_filter, history_limit}` and the planning/editor fields. [code `src/app.rs:180-325`, `src/automation_views.rs:240-315`]
2. **Every module can write them.** The fields are `pub` or `pub(crate)`, so `automation_runtime.rs`, `recovery.rs`, `run_views.rs`, `intake_views.rs`, `settings_views.rs`, `batch_app.rs`, `global_search.rs` and tests write route state directly. [code grep in §4]
3. **`navigate()` does not clear the origin stack.** Callers must remember `structural_navigation()` first. Sidebar and breadcrumb handlers do; topic rows, settings links/models, recovery links, intake links, request-history links, batch progress and several CTAs do not. [code `src/app.rs:801`; `src/views.rs:1152/1216/1454`; missing at `src/topic_views.rs:337/415/585`, `src/settings_views.rs:170`, `src/recovery.rs:646/685/825`, `src/intake_views.rs:140/151`, `src/batch_app.rs:385`, `src/run_views.rs:468`]
4. **`capture_origin()` is a precedence heuristic over contradictory booleans**, not a reading of the rendered destination. It checks `result_dialog`, `task_result_context`, `selected_run + scope == Runs`, `settings_dialog`, `selected`, then `scope`. When `selected` and `selected_run` both survive a navigation, the same rendered task detail can be captured as a results page (defect C3). [code `src/navigation_context_app.rs:116-254`]
5. **`capture_origin()` cannot distinguish a period opened over detail from one opened over results** except through `scope`, and `open_task`/`navigate` clear `result_dialog`; the period under-page is therefore reconstructed from a combination that already lost information. [code `src/app.rs:908-930`, `src/automation_views.rs:873-899`, `src/navigation_context_app.rs:116-151`]
6. **The wizard is page content but its "open" state is a boolean beside `editing` and `selected`.** `creating=true` can survive navigation to a non-Automations scope and later reappear with stale values; background plan completion writes `step=3` into that hidden form. [code `src/automation_views.rs:965-994`, `src/automation_views.rs:1359-1430`, `src/app.rs:846-850`]
7. **Period/settings/attempt dialogs are three independent booleans/slots.** Mutual exclusion is maintained only by handler discipline. `close_planning_attempt` restores `settings_dialog` from a boolean captured earlier; `clear_navigation_overlays` and Escape wipe all of them, so the "topmost only" rule is not represented. [code `src/automation_views.rs:856-963`, `src/navigation.rs:447-467`, `src/views.rs:3840-3865`]
8. **The Back control is derived from breadcrumbs, not from the return stack.** It is hidden when `navigation_path()` has no parent even if a return frame exists, and its tooltip names the structural crumb even when Back would restore an origin. [code `src/views.rs:1490-1520`, `src/views.rs:1221-1420`]
9. **`Research results` has two selection policies.** `displayed_run_id()` filters by ordinary/automation ownership, while `run_views::runs()` repeats a similar filter and `select_run()` only writes `selected_run` without updating the task context. Choosing another project/topic run can therefore show run B under task A's breadcrumb. [code `src/navigation.rs:426-445`, `src/run_views.rs:177-200`, `src/run_views.rs:286-350`]
10. **Planning destination is mirrored.** `planning_destination: CreationMode` is authoritative for validation, but `planning_project`, `planning_topic` and `planning_replan_source` remain independently writable around it. [code `src/project_app.rs:20-61`, `src/app.rs:245-255`]
11. **Search still holds a second dirty policy.** `search_navigation_blocker`, `task_edit_holds_input`, `project_planning_holds_input` and `automation_draft_holds_input` duplicate T10's `dirty_draft()` and miss the comment/composer preserved drafts. Activation also bypasses T10's `request_search_target`. [code `src/global_search.rs:709-880`, `src/navigation_context_app.rs:591-675`]
12. **The current headless suites encode some legacy behavior.** `navigation_qa` calls `structural_navigation()` + `open_task()` directly and asserts `Scope::Automations` after Back from a step; `global_search_qa` asserts the caller's settings dialog is gone after selecting a period. Those expectations must be reconciled by the owning tasks (T07/T08/T10) through real controls before T12 freezes the matrix. [test `src/navigation_qa.rs:268-300`, `src/global_search_qa.rs:793-815`]
13. **The correct target is a small typed state machine, not a second set of fields.** `Route` (page identity) + `Overlay` stack + `ReturnFrame` stack, with read-only projections for breadcrumbs/sidebar and one command gate for dirty exit. Legacy fields are deleted family by family; read-only projections are allowed only until the family flip completes.
14. **The visible behavior should not change.** T12 eliminates contradictory states and duplicated writers; it does not redesign pages, dialogs, automation lifecycle, result presentation or search content.
15. **The remaining unknowns are cross-owner, not gaps in the target model.** T10's final command/dialog shape (U1), T21/T13 pending-save semantics (U2/U3), T23's single result-selection policy (U4), T08 activation adoption (U5), and the period page-vs-overlay decision (U11) must resolve before their slices; §7.5 lists the full ledger and evidence required.

## 3. Current state model

### 3.1 Fields that currently decide what is on screen

| Field | Where | Meaning today | Written by (non-QA) |
| --- | --- | --- | --- |
| `scope: Scope` | `app.rs:182` | page family: All/Project/Topic/Cycle/Archive/Runs/Recovery/Research/Automations/AutomationRuns/Settings | `navigate` (`app.rs:801`), `open_automation` (`automation_views.rs:838`), `start_automation_create` (`:965`), `open_run_progress` (`app.rs:1574`), `show_task_result` (`research_app.rs:735`), startup/undo (`app.rs:1155-1214`) |
| `selected: Option<u64>` | `app.rs:190` | task detail selection | `navigate` clears, `open_task` sets (`app.rs:908`), `open_run_progress` clears, undo/startup, `apply_origin` |
| `task_result_context: Option<u64>` | `app.rs:191` | task that owns the displayed result | `navigate` clears, `open_task` clears, `show_task_result` sets, `apply_origin` sets |
| `selected_run: Option<u64>` | `app.rs:293` | result/run selection | `show_task_result`, `select_run` (`run_views.rs:177`), `open_run_progress`, `apply_origin`, recovery/request links |
| `layout`, `all_status`, `query` | `app.rs:183-198` | list/board filters; `query` is list-local text | `navigate` clears query only; board/list controls; `apply_origin` |
| `goal_steps_tab`, `steps_layout` | `app.rs:320-321` | goal Overview/Steps subtab and step layout | `open_goal_steps` (`goal_steps.rs:232`), tab clicks, `apply_origin` |
| `show_run_result`, `show_run_details`, `show_task_details`, `show_task_activity` | `app.rs:314-324` | presentation toggles, not destinations | many local handlers |
| `automation_ui.selected` | `automation_views.rs` (`AutomationUi`) | automation detail identity | `open_automation`, `edit_automation`, save/enable, archive, `apply_origin` |
| `automation_ui.creating`, `.editing`, `.step` | same | wizard open, editing id and step | `start_automation_create`, `edit_automation`, `replan_automation`, wizard controls, cancel/save/enable |
| `automation_ui.result_dialog` | same | period result dialog | `open_occurrence_result`, close, `navigate`/overlay clear |
| `automation_ui.settings_dialog` | same | automation settings dialog | open/close, edit/replan, attempt close, overlay clear |
| `automation_ui.planning_attempt_dialog` and return fields | same | T23 planning history dialog and focus | `open_planning_attempt`/`close_planning_attempt` |
| `automation_ui.result_filter`, `.history_limit` | same | automation results filter/visible bound | filter control, `apply_origin` |
| `planning_project`, `.planning_topic`, `.planning_replan_source` | `app.rs:245-255` | mirrored composer destination | destination selector, `apply_planning_destination`, `clear_navigation_overlays`, Escape |
| `planning_destination`, `.planning_destination_error` | `app.rs:247-252` | visible validated destination of the composer | `begin_planning`, retarget, cancel, intake error path |
| `draft`, `editing`, `property` | `app.rs:204-207` | task edit overlay payload and small property picker | `begin_draft`, save/cancel, `open_task`, `navigate`, Escape |
| `project_rename`, `topic_editor`, `topic_move`, `directory_project`, `new_project` | `app.rs:236-244` | small single-field dialog payloads | their open/cancel/save handlers |
| `navigation.origins` | `navigation_context_app.rs:22-76` | bounded T10 `Origin` stack | `structural_navigation`, `record_origin`, `attempt_navigation`, `apply_origin` |
| `navigation.pending_exit`, baselines, comments, composer | same | T10 dirty/decision/preserved payload | T10 handlers (decision UI not rendered yet) |

### 3.2 Existing pure/typed building blocks that T12 should absorb, not duplicate

| Existing API | File | T12 treatment |
| --- | --- | --- |
| `Origin` (11 variants) | `navigation_context.rs:65-122` | Replace with `Route` + `ReturnFrame`; keep the same filter/scroll/selection data in typed pages and frames. |
| `NavigationTarget` | `navigation_context.rs:136-158` | Replace with `Destination`/`Route`; `Recent{task,surface}` becomes a destination-building policy. |
| `NavigationIntent` / `NavigationKind` | `:164-216` | Replace with `NavCommand::Enter { dest, mode }`; `Search`/`GeneratePlan` become `DrillDown` commands with a captured frame. |
| `OriginStack` / `validate_origin` | `:272-578` | Replace with `ReturnStack` / `validate_frame`; restore the *whole* snapshot (route + overlays + scroll), not a heuristic. |
| `DismissibleSurface` | `:220-235` | Derive from `OverlayKind`; one Escape path. |
| `DraftSurface`, `DraftClass`, `DecisionSurface`, `PendingExit`, `ExitTarget`, `resolve_decision` | `draft_exit.rs` | Keep as the dirty/exit policy (T10-owned). T12 replaces `ExitTarget` with `NavCommand` and requires every command to pass through it. |
| `navigation_context_app.rs` `capture_origin`/`apply_origin`/`attempt_navigation`/`request_*` | `:101-1119` | Replace with `nav.enter`, `nav.back`, `nav.close_top`, `nav.request`, `nav.apply_frame`, `nav.reconcile`. Delete heuristic capture. |
| `clear_navigation_overlays` | `navigation.rs:447-467` | Delete; `enter(structural)` owns overlay clearing internally, dirty gate first. |

### 3.3 Stable consolidation seams (use these, not moving lines)

These are the interfaces that stayed shape-stable across the T10 handoff and the concurrent edits. T12 should bind its slices to them; control ids and line numbers are only QA anchors.

| Seam | Stable contract today | Current owner / status | T12 action | Why it is stable |
| --- | --- | --- | --- | --- |
| `TaskApp.navigation` + `NavigationState` | one field owns origins, recent surfaces, scroll handles, baselines, comments/composer, pending exit | T10, scaffolding present, not accepted | Replace the struct body with `NavState`; keep the `TaskApp` field name/slot so non-nav modules do not change | The field is already the single app-level attachment point; callers access behavior, not its internals |
| `draft_exit` pure API | `DraftSurface`, `DraftClass`, `DecisionSurface`, `PendingExit`, `ExitDecision`, `SaveOutcome`, `resolve_decision`, labels | T10, stable pure module + tests | Keep verbatim; replace only `ExitTarget` payload with `NavCommand` | No GPUI/Store/window types; asserted by 8 unit tests |
| `navigation_context` data descriptors | `Origin` fields: route scope/filter/scroll/selection, period under-page, settings tab | T10, stable shape + 22 tests | Move to `Route` pages + `ReturnFrame`; preserve field coverage | The data shape, not the enum spelling, is the contract |
| `NavigationTarget` / `ExitTarget` command vocabulary | explicit destination + Structural/DrillDown distinction + side-effect targets (`SidebarTask`, `Recent`, `CancelWizard`, `CancelPlanning`) | T10, published but not UI-wired | Collapse to `Destination` + `NavCommand`; preserve every named side effect | T10's exit targets already encode the UI behaviors T12 must not lose |
| `Store` owner validation | `task`, `automation`, `occurrence`, `project`, `topic` lookups; T10 fallback ladder | domain, stable | `validate_frame` reads the same methods; no new domain query | Owner identity and fallback rules are settled by T10 |
| `displayed_run_id` / result ownership | ordinary vs automation-owned filtering; task-owned result context | T23/T06, **no single published policy yet** | Consume one policy function; store its result in `ResearchResultPage` | This is a known cross-owner unknown (U4), not a T12 design choice |
| `open_run_progress` / `start_run_background` | explicit UI progress vs passive dispatch | T02, accepted; T12 must not alter | `open_run_progress` becomes a UI `request_nav`; background stays passive | T02's accepted split is the contract; only the call shape changes |
| confirmed save entry points | `save_draft -> bool`, `save_automation_draft -> bool`, staged request save; failures keep live form/store | T10/T13/T21, moving | Decision consumes a confirmed `SaveOutcome`; no inference from empty error | The handoff names these as the only Save-for-decision entry points |
| `navigation_path` / `sidebar_scope` / header Back | breadcrumbs/sidebar/Back are derived from route-ish state | T10/T01, to be re-pointed | Replace bodies with `breadcrumbs()`, `sidebar_selection()`, `back_availability()`; keep call sites | Render code already calls one function per derivation; only the body changes |
| `global_search` activation | one hit -> one destination; overlay/focus owned by T08 | T08, terminal but not integrated | One `request_nav` call; delete predicates | T08 intentionally has a single activation seam (`activate_global_search_target`) |
| scroll handles | task list / board / columns / topic / period / settings / result handles already live in `NavigationState` | T10, implemented, needs S6 QA | Capture/restore via `ReturnFrame` bookmarks; keep handles in app glue | Handles are GPUI objects; they must not enter the pure route module |
| QA module/flag registration | `main.rs` module list + dispatch flags; T01 owns baseline | T01, shared | Add one flag/module only in the first authorized slice; coordinate the line | This is the only shared registration point; keep the edit one line |

**Seam rule:** if an owner changes a symbol in this table, T12 pauses that slice and updates the ledger; it does not silently adapt to a new line or a new parallel field.

## 4. Route / selection / overlay write inventory

Legend for "current writes": `R` route/scope, `S` task selection, `Q` result selection, `O` overlay/modal flag, `F` filters/page prefs, `D` editor/draft payload, `H` return history.

Legend for target: `Enter(dest, Structural|DrillDown)`, `Overlay(spec)`, `CloseTop`, `Back`, `Decision(target)`, `Payload` (no navigation).

### 4.1 Research and chrome entries

| ID | Entry / control | Handler (snapshot line) | Current writes | Current Back/Close | Target command | Return frame |
| --- | --- | --- | --- | --- | --- | --- |
| R01 | Sidebar **All research** (`nav`) | `views.rs:1152` → `app.rs:801` | R,H clear; O clear; F query clear | structural parent; hidden at root | `Enter(ResearchList{All}, Structural)` | cleared |
| R02 | Sidebar project row (`project-{id}`) | `views.rs:793` | R,H clear; O clear | to All | same, `Project(id)` | cleared |
| R03 | Project disclosure chevron | `views.rs:~800` | none | n/a | disclosure only | unchanged |
| R04 | Sidebar topic row (`topic-{id}`) | `topic_views.rs:337` | R; **H not cleared** | may return to stale origin | `Enter(ResearchList{Topic}, Structural)` | cleared |
| R05 | Topic disclosure chevron | `topic_views.rs:~345` | none | n/a | disclosure only | unchanged |
| R06 | Sidebar task row (`sidebar-task-{id}`) | `views.rs:962` → `navigation.rs:361` | R project, H clear, S, Q result if completed | structural parent | `Enter(TaskOutcome{task}, Structural)`; route is `ResearchTask` or `ResearchResult` from `completed_sidebar_result` | cleared |
| R07 | Goal/steps disclosure | `views.rs:~1000` | none | n/a | disclosure only | unchanged |
| R08 | Archive / Paused / Research results / Market research | `views.rs:1152` | R,H clear, O clear, F query clear | structural | `Enter(ResearchList{...}, Structural)` / `PausedProjects` / `MarketResearch` | cleared |
| R09 | All automations / Automation results | `views.rs:1216` | R,H clear; automation selection cleared on list | structural | `Enter(Automations|AutomationResults, Structural)` | cleared |
| R10 | Breadcrumb link | `views.rs:1454-1476` | R/S/Q/O,H clear | target crumb | `Enter(crumb.destination, Structural)` | cleared |
| R11 | Header **Back** (`navigation-back`) | `views.rs:1518` → `navigation.rs:477` | restores T10 origin or crumb | origin then structural | `Back` | pops one |
| R12 | Recently opened entry | `views.rs:1560` | H push; S/Q/O | structural task path | `Enter(TaskOutcome{task, surface}, DrillDown)` | current snapshot |
| R13 | Close current task | `views.rs:1571` → `navigation.rs:505` | tab payload + Back | origin/crumb | `CloseTask` (payload) + `Back` | pops |
| R14 | Footer **Search** / ⌘K | `views.rs:594`, `:3805` → `global_search.rs:611` | O search; no route | Escape closes only search | `Overlay(Search)` | current snapshot minus search |
| R15 | Footer **New research** / ⌘N | `views.rs:603`, `:3834` → `project_app.rs:157` | O composer; no route (T10 S2) | cancel only | `Decision` then `Overlay(PlanningComposer)` | page unchanged |
| R16 | Workspace menu **Settings…** | `views.rs:448` | R,H clear; O clear | structural parent hidden | `Enter(Settings{tab}, Structural)` | cleared |
| R17 | Settings tab | `settings_views.rs:320` | F tab | page-local | `Payload` route tab update; no return | unchanged |
| R18 | Create project | `views.rs:~3979` | R, O close | project page | `Enter(Project, Structural)` after confirmed save | cleared |
| R19 | Settings links from task/research | `views.rs:3017/3273`, `research_views.rs:111` → `settings_views.rs:170` | R; **H not cleared** | may return to stale origin | `Enter(Settings{Models}, Structural)` | cleared |
| R20 | Undo | `app.rs:~1150-1214` | R/S/O/F reset | current route can jump | `reconcile_after_store_replace` (validate route, clear invalid selection/returns) | cleared if route changed |
| R21 | List task row (`task-row-{id}`) | `views.rs:1984` | H push, S | structural | `Enter(ResearchTask, DrillDown)` | current list + query/layout/scroll |
| R22 | Board card (`card-{id}`) | `views.rs:2245` | H push, S | structural | same | current board + column scroll |
| R23 | Goal card **Open N steps** | `views.rs:2357` → `goal_steps.rs:232` | H push, S, F tab/layout | goal page | `Enter(ResearchTask{panel:Steps, layout:Board}, DrillDown)` | current list/board |
| R24 | Goal step row (`goal-step-{id}`) | `goal_steps.rs:322` | H push, S | goal detail | `Enter(ResearchTask, DrillDown)` | goal steps page |
| R25 | Task context menu **Open task** | `views.rs:2456` | H push, S | structural | `Enter(ResearchTask, DrillDown)` | current page |
| R26 | Task context menu **Edit task** | `views.rs:~2470` | O editor | cancel/Escape | `Overlay(TaskEditor)` | unchanged |
| R27 | Detail **View final report / latest result** | `views.rs:2594` | H push, Q, S clear, R Runs | task crumb | `Enter(ResearchResult, DrillDown)` | task detail + scroll |
| R28 | Detail **View progress** (automation-owned) | `views.rs:2928` → `automation_views.rs:~910` | R Automations, O period, S clear; **H not pushed** | detail | `Enter(Period{automation,occurrence}, DrillDown)` | task detail |
| R29 | Detail **View progress** (ordinary) | `views.rs:2928` | S/F or Q; H not pushed | task/goal | `Enter(ResearchTask{Steps}|ResearchResult, DrillDown)` | task detail |
| R30 | Topic goal row (`topic-goal-{id}`) | `topic_views.rs:484` | H push, S/Q result | topic | `Enter(TaskOutcome{task}, DrillDown)` | topic + layout/scroll |
| R31 | Topic **Report** (`topic-report-{id}`) | `topic_views.rs:521` | H push, S then Q | topic | one `Enter(ResearchResult{task,run}, DrillDown)` | topic |
| R32 | Topic **New research** | `topic_views.rs:552` → `project_app.rs:109` | O composer | cancel returns page | `Overlay(PlanningComposer{ExistingTopic})` | unchanged |
| R33 | Topic **Replan** | `topic_views.rs:277` → `project_app.rs:129` | O composer | cancel | `Overlay(PlanningComposer{Replan})` | unchanged |
| R34 | Topic **Regenerate** | `topic_views.rs:288` | dispatch; `open_run_progress` when foreground | structural | `Enter(ResearchResult{run}, DrillDown)` only after success | topic |
| R35 | Replanned **Open original topic** | `topic_views.rs:585` | R; **H not cleared** | stale origin | `Enter(ResearchList{Topic}, Structural)` | cleared |
| R36 | Saved requests **Open requests** | `views.rs:1741` | R; **H not cleared** | stale | `Enter(MarketResearch, Structural)` | cleared |
| R37 | Request history **Review goals** | `intake_views.rs:140` | R + F layout Board; **H not cleared** | project board | `Enter(Project, Structural)` with Board pref | cleared |
| R38 | Request **Planning details** | `intake_views.rs:151` | R Runs + Q; **H not cleared** | results page | `Enter(ResearchResult{run}, DrillDown)` | current list |
| R39 | Result **Goal details** | `run_views.rs:365` | S; **H not pushed** | task path | `Enter(ResearchTask, DrillDown)` | results page |
| R40 | Result **More actions → Open task** | `run_views.rs:915` | H push, S | task crumb | `Enter(ResearchTask, DrillDown)` | results page |
| R41 | Result **More actions → Open project** | `run_views.rs:915-920` | R + layout Board + H clear | project board | `Enter(Project, Structural)` | cleared |
| R42 | Result project/topic/goal pickers | `run_views.rs:177-200`, `:286-430` | Q only | results page | `Payload route.select_run(run)`; policy also reconciles task | unchanged |
| R43 | Empty results **Go to tasks** | `run_views.rs:468` | R; **H not cleared** | stale | `Enter(ResearchList{All}, Structural)` | cleared |
| R44 | Batch **Progress** | `batch_app.rs:385` | R; **H not cleared** | stale | `Enter(ResearchResult/BatchProgress, Structural)` | cleared |
| R45 | Paused project/topic title | `recovery.rs:646/685` | R; **H not cleared** | stale | `Enter(Project|Topic, Structural)` | cleared |
| R46 | Paused goal/step row | `recovery.rs:739/764` | S; **H not pushed** | path | `Enter(ResearchTask, DrillDown)` | Paused page |
| R47 | Paused **Planning details** | `recovery.rs:825` | R Runs + Q; **H not cleared** | stale | `Enter(ResearchResult{run}, DrillDown)` | Paused page |

### 4.2 Automation entries

| ID | Entry / control | Handler (snapshot line) | Current writes | Current Back/Close | Target command | Return frame |
| --- | --- | --- | --- | --- | --- | --- |
| A01 | All automations sidebar | `views.rs:1216` | R Automations, H clear; selected/creating cleared | list root | `Enter(Automations, Structural)` | cleared |
| A02 | Automation results sidebar | same | R AutomationRuns, H clear | Automations | `Enter(AutomationResults, Structural)` | cleared |
| A03 | Automation row (`automation-row-{id}`) | `automation_views.rs:2041-2062` | H push, R Automations, O clear, selected, scroll reset | list | `Enter(AutomationDetail, DrillDown)` | list scroll |
| A04 | **New automation** (`new-automation`) | `:1966` → `start_automation_create` `:965` | R Automations, selected None, creating true, editing None, step 1, payload reset | crumb/Back to list | `Enter(AutomationEditor{None}, DrillDown)` | list |
| A05 | Detail **Settings** | `:2389` → `open_automation_settings` `:856` | O settings, focus modal | Close focuses root | `Overlay(AutomationSettings{id})` | base detail; opener focus |
| A06 | Settings **Close/X/Escape** | `:867`, `:2791`, `:2843` | O clear; focus root | page unchanged | `CloseTop` | restore opener |
| A07 | Settings **Edit** (`edit-automation`) | `:2744` → `edit_automation` `:1047` | H push, O settings close, R unchanged, selected id, creating true, editing id, payload from definition | Back to settings (crumb special case) | `Enter(AutomationEditor{Some(id)}, DrillDown)` | snapshot{detail, overlay Settings} |
| A08 | Settings **Replan** | `:2765` → `replan_automation` `:996` | same as A07 + plan/token cleared | same | same with replan payload | same |
| A09 | Draft **Review and enable** | `:2313` | H push, edit | detail | same as A07 | detail |
| A10 | Plan version **Review** (`planning-attempt-{run}`) | `:2117`, `:2631` → `open_planning_attempt` `:928` | O attempt; settings false; origin bool; opener map | T23 restores opener | `Overlay(PlanningAttempt{run, from_settings})` | T23 focus contract preserved |
| A11 | Attempt Close/Escape | `:946` | O restore settings maybe; opener | detail/settings | `CloseTop` | T23 restored opener |
| A12 | Results filter | `:2280-2281` | F filter/limit + scroll reset | results | `Payload route.results.set_filter` | unchanged |
| A13 | Results row / detail period row (`open-occurrence-{id}`) | `:3007`, `:2983` | O period, focus modal, marks read | Close focuses root; Escape wipes | `Overlay(AutomationPeriod{automation,occurrence})` | base page/scroll; opener focus |
| A14 | Period **X/footer Close** | `:3613`, `:3636` | O clear; focus root | base page | `CloseTop` | restore opener |
| A15 | Period step row / **Open** (`automation-step-{id}`) | `:3406-3460` | H push, S; `open_task` clears period | period crumb over detail | `Enter(ResearchTask, DrillDown)` | snapshot{base, overlay Period} |
| A16 | Period **Continue/Retry** | `:3485-3515` | runtime only | dialog stays | `Payload` (T03/T05/T13) | none |
| A17 | Wizard step **Back/Continue** | `:1126-1310` | D step/validation/planning | wizard | `Payload` | unchanged |
| A18 | Wizard **Cancel** | `:3796-3804` | creating false; open detail/list | immediate discard | `Decision(CancelWizard)` then `Enter(AutomationDetail|AutomationList, Structural)` (clears the editor return) | list/detail; Settings is not restored |
| A19 | **Save draft** | `:4217` → `:1480` | creating false, selected id, editing retained | detail | confirmed save (T13) then `Enter(AutomationDetail, Structural)` | cleared |
| A20 | **Enable automation** | `:4248` → `:1526` | creating false, editing None, selected id | detail | confirmed command then `Enter(AutomationDetail, Structural)` | cleared |
| A21 | Retry planning (`automation-retry-planning`) | `:4200` | D token/preview | detail | `Payload` | unchanged |
| A22 | Detail **Replan** | `:2730` | H push, editor | settings | `Enter(AutomationEditor, DrillDown)` | detail |
| A23 | Search **Automation** hit | `global_search.rs:734-742` | R Automations, O clear, selected; **no origin** | structural/automation | `Enter(AutomationDetail, DrillDown)` | caller snapshot below search |
| A24 | Search **Period** hit | `:743-759` | R Automations + O period; **no origin** | detail | `Enter(Period, DrillDown)` | caller snapshot below search |
| A25 | Background plan completion | `automation_views.rs:1359-1430` | D plan/feedback/token; **step=3 when creating** | n/a | `Payload.attach_plan_if_session_matches`; **never route/step** | unchanged |

### 4.3 Search, keyboard and modal exits

| ID | Entry | Handler | Current | Target |
| --- | --- | --- | --- | --- |
| S01 | Footer Search / ⌘K open | `global_search.rs:611-626` | O search; captures focus; resets domain/query | `Overlay(Search)`; no route/selection/filter/draft write |
| S02 | Domain filter | `:651-665` | payload | payload |
| S03 | Up/Down | `:667-682` | payload | payload |
| S04 | Enter / row click → Research | `:709-733` | closes search, `open_task`; blocker predicates | `Decision` (one `dirty_draft`) → `Enter(ResearchTask, DrillDown)` + frame |
| S05 | Enter / row click → Automation | `:734-742` | closes, `open_automation`; no origin | `Enter(AutomationDetail, DrillDown)` |
| S06 | Enter / row click → Period | `:743-759` | closes, automation + period; no origin | `Enter(Period, DrillDown)` |
| S07 | Missing target | `:764-780` | message + refresh; no route | same |
| S08 | Escape | `:612-620` + `views.rs:3811` | closes search, restores opener | `CloseTop` only; never global wipe |
| S09 | Dirty guard | `:789-880` | local predicates + refusal message | delete; use `dirty_draft()` + Decision (T10) |
| K01 | ⌘[ | `views.rs:3818-3832` | guarded by many flags except attempt/wizard; `go_back` | `Back`; blocked if any overlay not represented by its guard; controller decides |
| K02 | ⌘N | `views.rs:3833-3834` | unguarded `open_research_intake` | T10-approved command gate; no navigation behind a modal |
| K03 | Escape | `views.rs:3840-3865` | global wipe: draft, planning, all dialogs; focus root; leaves selected/route | `request_escape`: decision Stay, else close top overlay only; no route write |
| K04 | Task context menu (Open/Edit/Move) | `views.rs:2456+` | Open pushes origin; Edit overlay; Move store only | Open → DrillDown; Edit → Overlay; Move → payload |
| K05 | Workspace menu Settings | `views.rs:448` | structural + route | Structural |

### 4.4 Creation and edit entries

| ID | Entry | Handler | Current | Target |
| --- | --- | --- | --- | --- |
| C01 | Footer/project/topic/R15 open composer | `project_app.rs:20-35`, `:157` | Overlay in place; destination resolved | `Overlay(PlanningComposer{mode})`; no route |
| C02 | Destination selector | `project_views.rs:161` → `retarget_planning_destination` | writes destination **and** `planning_project/topic/replan_source` | write `CreationMode` only; derive project/topic |
| C03 | **Save request for later** | `project_app.rs:188-237` | staged save then clears mirror fields; no route | confirmed save → `CloseTop`; no route |
| C04 | **Generate plan** | `project_app.rs:249-258` | H push; dispatch; `open_run_progress` | confirmed dispatch → `Enter(ResearchResult{run}, DrillDown)`; failure stays with draft |
| C05 | Composer **Cancel/X** | `project_app.rs:65-75`, `project_views.rs` | closes overlay | `CloseTop`; clean immediate; dirty Decision |
| C06 | Composer **Escape** | `views.rs:3840-3865` | clears planning fields, not route | `request_escape` (Decision if dirty) |
| C07 | Task **Edit** | `views.rs:~2470` → `begin_draft` | O editor | `Overlay(TaskEditor)` |
| C08 | Task edit **Save/Cancel** | `views.rs:~3669`, editor overlay | save closes; cancel/Escape wipe | confirmed save → `CloseTop`; cancel dirty Decision |
| C09 | Comment composer | `views.rs` detail | cleared by `open_task` (`app.rs:~932`) | `PerTaskText` parked; restore on task; explicit discard |
| C10 | Market research composer | `research_views.rs:84-99` | page fields; `create_research` → Runs; no origin | page payload; Generate → DrillDown with frame; preserve on structural nav |
| C11 | Small editors (project rename, topic editor/move, folder, new project) | their modules | independent overlay fields; Escape wipes all | `Overlay(kind)`; explicit cancel only; at most one non-search overlay stack |
| C12 | Decision dialog | **not rendered** | T10 state only; `views.rs` has no branch | `Overlay(DraftDecision)` top; Save/Discard/Stay; Escape=Stay |

### 4.5 Background writers that must have no navigation authority

| ID | Path | Current navigation effect | Required invariant |
| --- | --- | --- | --- |
| B01 | `finalize_project_plan` (`project_app.rs:338`) | none; feeds `automation_plan_ready` | none; may attach plan payload only |
| B02 | `automation_plan_ready` (`automation_views.rs:1359`) | writes plan/token/feedback/toast; `step=3` if `creating` | never route, never step, never clear dirty text; attach only to matching editor session |
| B03 | Run completion / continuation (`research_app.rs` pump) | none | none |
| B04 | Scheduler/timer (`automation_runtime.rs`, `scheduler.rs`) | none; `route_automation_*` only changes queue/feedback | none |
| B05 | Recovery checks/import (`recovery.rs`) | none | none |
| B06 | Persistence save/ack (T13/T14) | none today | none; may update save feedback |
| B07 | Undo/store replace (`app.rs`) | resets scope/selection without nav intent | controller `reconcile` only; no invented Back |
| B08 | Startup/workspace load (`app.rs:1155`) | sets Scope::All | initialization only, no origin |

**Enforcement:** route/overlay/return fields become private to `navigation_state`; the only public surface is `TaskApp::nav()` (read) plus command methods. A source gate rejects `navigate(`, `open_task(`, `open_automation(`, `open_occurrence_result(`, `structural_navigation(`, `record_origin(` and legacy field assignments in `automation_runtime.rs`, `scheduler.rs`, `workflow.rs`, `execution_*.rs`, `domain_command.rs`, `command_app.rs` except at explicit UI call sites listed in §4.1–4.4.

## 5. Contradictory constructible states

These are reachable through current normal APIs, not just tests. Each row names the construction and the visible consequence.

| ID | Construction | Current symptom | Eliminated by |
| --- | --- | --- | --- |
| C01 | From a result page with `selected_run=Some`, choose **More actions → Open task** (`run_views.rs:915`): `record_origin` then `open_task`; `open_task` does not change `scope` or clear `selected_run` | Rendered page is task detail, but `scope==Runs`; `sidebar_scope()` derives Project from `selected`; further drill-down `capture_origin()` sees `selected_run + scope==Runs` and records a **results** origin, so Back can return to results instead of the task page | Route is `ResearchTask`; `selected_run` is not a global field; return frame is captured from the typed snapshot |
| C02 | Open task A's result, then use the project/topic/goal picker (`run_views.rs:318-430`) to select run B | `task_result_context` still A, `selected_run` B; breadcrumb/task label A, report content B; `displayed_run_id` permits B because context is present | One `ResearchResultPage { selection }`; selecting a run reconciles owner per T23 policy; breadcrumb derives from the effective run |
| C03 | Open a step from a period (`record_origin` captures `AutomationPeriod`), then call `structural_navigation`/a structural link/tests call `open_task` | Rendered step detail can have an empty origin stack; Back uses the automation crumb and rebuilds the period over **detail**, losing the results page it came from | Return frame is captured by the controller at the click, not reconstructed from flags; structural navigation is an explicit different command |
| C04 | Open a period from Automation results, then open its step | `open_task` clears `result_dialog`; `scope` remains `AutomationRuns`; the step is a task page with `scope==AutomationRuns`. Back origin restores the period; if origin is empty, the cron path rebuilds detail+period | `ResearchTask` route + return frame `{base: AutomationResults, overlay: Period}` |
| C05 | Edit automation (settings → Edit) then navigate to a non-Automations scope with a direct `navigate` | `creating=true` survives; the wizard is hidden but alive; background completion can set `step=3`; returning later shows stale form/step | `AutomationEditor` is a route/overlay identity; leaving it is a decision; background attach never changes step |
| C06 | `open_automation` then a direct `navigate(Project)` (e.g. settings/recovery/intake links) | `automation_ui.selected` remains Some while the page is a project; hidden selection can resurface through a Back frame or be captured inconsistently | `AutomationDetail` is a route variant; no hidden selection field |
| C07 | Open settings from detail, open planning attempt, then close attempt after the automation is removed/archived by undo/reload | `close_planning_attempt` restores `settings_dialog` from `planning_attempt_origin_settings` even if detail no longer valid; render falls through to list with a dangling modal flag | Overlay stack stores `AutomationSettings{automation}`; close validates owner and pops to detail or list with one notice |
| C08 | Two dialogs: open settings, press ⌘N (unguarded), open search over it, press Escape | Multiple focus traps; `modal_focus` shared; Escape wipes **all** dialogs and planning fields, not the top only | One ordered overlay stack; `CloseTop`; Escape never writes route |
| C09 | Delete/rename the selected project/topic/task while its page is open (undo, migration, archive cleanup) | `Scope::Project/Topic` renders a not-found page; `selected` stays set and breadcrumbs/sidebar remain; only Undo has a repair path | `nav.reconcile(store)` after store replacement; typed route validation/fallback with one notice |
| C10 | Remove the occurrence behind an open period dialog (retry/cleanup/reload) | Dialog remains with an empty body/header; focus is trapped in a dead modal | Overlay validation on store replace; close to base detail + notice |
| C11 | Select a results filter for automation X, then X is deleted | `result_filter=Some(X)` remains; render silently unfilters; Back normalization only happens if that origin is popped | Results page filter validated at render/route apply; one policy, notice optional only for user-visible loss |
| C12 | `planning_destination` says `NewTopic{project 2}` while `planning_project/planning_topic` still describe the previous mode after an error/retarget path | Save path uses destination but labels/derived fields can disagree; stale topic can be attached or rejected inconsistently | Only `CreationMode` in the overlay; project/topic derived; no mirrored fields |
| C13 | Open a topic row (`topic_views.rs:337`), then Back | Prior origin stack is not cleared; Back restores a page unrelated to the structural destination | Structural enter clears returns inside the controller |
| C14 | Open Settings from a task via model fix (`views.rs:3017`) or research page (`research_views.rs:111`) | `navigate` does not clear origins; Back can leave Settings for the pre-Settings drill-down rather than the structural parent | Same; structural enter clears |
| C15 | Archived/deleted owner rendered while origin points at it | `validate_origin` handles known variants but `capture_origin` can select the wrong variant first (C01/C02), so validation validates the wrong page | Return frame is a snapshot of the route that was actually displayed |
| C16 | `goal_steps_tab=Some(goalA)` then select task B | Tab state is global but only meaningful for goal A; origin for B carries A's tab; `steps_layout` leaks across tasks | `ResearchTaskPage.panel` per route; `steps_layout` is a typed page pref |
| C17 | `selected_run` points at a deleted run or an automation-owned run while `task_result_context=None` | `displayed_run_id` silently substitutes the latest ordinary result while the field still claims another selection; breadcrumb/result labels can disagree | Route stores the effective run; selection policy resolves at read/apply; no second "effective" field |
| C18 | Undo while a task/period page is open | `undo_change` resets `scope`/`selected` directly; origin stack untouched; a later Back can navigate into rolled-back history | `reconcile_after_store_replace`: validate route/overlay/returns, clear invalid frames |
| C19 | Back button on a page with a valid origin but no structural parent (e.g. Settings reached structurally then a drill-down) | Button is **hidden** because `navigation_path()` has no parent; the origin is unreachable by mouse; tooltip names the wrong target when visible | `BackAvailability` derived from returns + route; label from the actual target |
| C20 | Keep a pending exit open, then let a background event/undo remove the target owner | `decide_exit` executes a stale target; `apply_target` silently returns on missing owner, or lands on a fallback without a notice | Revalidate the command on proceed; `NavReject`/fallback with one notice |
| C21 | `show_run_result`/`show_run_details`/`show_task_details` toggled independently | Presentation can claim result and technical details simultaneously; not a route, but the same "independent booleans" pattern | Presentation substate belongs to the page (`ResultView::Report|Technical|Live`), not global booleans |
| C22 | Open a period over detail, select a search hit, then Back | T10 captures settings/period by precedence; if the search hit is an automation step or period, the captured overlay stack is not restorable because `open_task` cleared it; Back may restore a modal that no longer matches the base | Return frame captures base + full overlay stack under search; restore validates |
| C23 | Run a scheduled start while a dirty task edit, composer, wizard or period is open | T02 removed route writes from dispatch, but `automation_plan_ready` can still change the hidden wizard step; if any completion path calls a UI helper, it can navigate | Private nav fields + background no-navigation invariant test + attach-only plan hook |
| C24 | Two independent `focus_trap`s rendered (e.g., settings + period/search), shared `modal_focus` | Focus can move into the wrong layer; close focuses root rather than opener | One top overlay owns focus; per-overlay opener `FocusSlot`; closing restores it |

## 6. Target typed model (exact API)

### 6.1 Module boundaries

- `src/navigation_route.rs` **[new, pure]**: IDs, `Route`, page structs, `Overlay`, `Destination`, `ReturnFrame`, `ReturnStack`, parent/validation/fallback, breadcrumb derivation data, transition tests. No `TaskApp`, `Window`, `FocusHandle`, `Store` mutations. It may read `&Store` for validation, like T10's pure module.
- `src/navigation_state.rs` **[new, app glue]**: `NavState` field, `TaskApp` command methods, focus-handle storage, scroll capture/restore, overlay payload binding to existing Input entities, projections used during migration.
- `src/draft_exit.rs` **[T10-owned, kept]**: dirty computation and Save/Discard/Stay policy. T12 consumes `dirty_draft()` and `resolve_decision()`; it does not duplicate them.
- `src/navigation_context.rs` / `navigation_context_app.rs` **[deleted at the end of T12]**: absorbed. `validate_origin` logic moves to `validate_frame`; creation destination helpers move to `navigation_route`/`project_app`.
- `src/navigation.rs` **[kept for T07 helpers only]**: `task_navigation`, `TaskNeighbor`, `detail_task_neighbors`, `automation_period_steps`, labels/tooltips. `go_back`, `clear_navigation_overlays`, `reopen_automation_settings`, `displayed_run_id` and `close_current_task` route through the controller or are deleted.

### 6.2 State shape

```rust
// Pure. All fields private outside this module.
pub(crate) struct NavState {
    route: Route,
    overlays: Vec<Overlay>,          // bottom -> top, bounded (<= 3), one per kind
    returns: ReturnStack,            // bounded, deduplicated
    recent_surfaces: BTreeMap<u64, RecentSurface>, // destination memory, not route authority
}

pub(crate) enum Route {
    ResearchList(ResearchListPage),      // All | Project | Topic | Cycle | Archive | Runs | Research
    ResearchTask(ResearchTaskPage),      // task detail
    ResearchResult(ResearchResultPage),  // ordinary or task-owned result page
    PausedProjects,
    MarketResearch,
    Settings(SettingsPage),
    AutomationList,
    AutomationResults(AutomationResultsPage),
    AutomationDetail(AutomationDetailPage),
    AutomationEditor(AutomationEditorPage), // new/edit/replan is a page, not a boolean
}

// Page fields are private to this module. `navigation_state.rs` reads them via
// accessors and changes them only through NavState mutators; no caller can
// assemble a contradictory page by struct literal.
pub(crate) struct ResearchListPage {
    scope: ResearchScope,   // no Settings/Automations/Runs aliases; derives Scope
    query: String,          // list-local
    all_status: AllStatus,
    layout: Layout,
}

pub(crate) struct ResearchTaskPage {
    task: u64,
    panel: TaskPanel,       // Overview | Steps
    steps_layout: Layout,   // meaningful only for Steps
}

pub(crate) struct ResearchResultPage {
    owner: ResultOwner,     // Task(u64) | Any
    selection: ResultSelection, // requested/effective run per T23 policy
}

pub(crate) struct SettingsPage { tab: SettingsTab }
pub(crate) struct AutomationResultsPage { filter: Option<u64>, history_limit: usize }
pub(crate) struct AutomationDetailPage { automation: u64 }
pub(crate) struct AutomationEditorPage { editing: Option<u64> }

pub(crate) struct ResultSelection {
    run: Option<u64>, // effective run per T23 policy; no second "displayed" field
}

pub(crate) struct ReturnFrame {
    route: Route,
    overlays: Vec<Overlay>,
    scroll: ScrollBookmark,
    // A deleted-run fallback hint captured with the frame; never live page authority.
    result_fallback: Option<ResultFallback>, // { project: Option<u64>, topic: Option<u64> }
}
```

`ScrollBookmark` is pure data: a small map from `ScrollKey` (`TaskList`, `BoardX`, `BoardColumn(Stage)`, `Topic`, `PeriodBody`, `Settings`, `Result`, `Dialog`) to `(f32, f32)`. `navigation_state.rs` translates it to/from the real GPUI handles; no handle enters the pure module.

**Why this shape:**
- `Scope::Runs` is not a route with a hidden `selected`; it is `ResearchResult`.
- A task detail and a result page cannot both be selected.
- An automation detail and an editor cannot both be open.
- `creating`/`editing`/`selected` become one `AutomationEditorPage.editing` plus the presence of the editor route.
- `task_result_context` is `ResultOwner::Task`.
- `selected_run` is `ResearchResultPage.selection.run`.
- `goal_steps_tab` is `TaskPanel::Steps`.
- `planning_destination` is `Overlay::PlanningComposer{ mode }`; the mirrored fields are deleted.
- `query/layout/all_status/filter/history_limit/tab` live in their page struct, not in global fields.

### 6.3 Overlays (modal/edit/decision)

```rust
pub(crate) enum Overlay {
    Search,                                           // T08 content
    PlanningComposer { mode: CreationMode },          // T10 payload holds text
    TaskEditor { task: u64 },                         // T10 payload holds fields/baseline
    AutomationSettings { automation: u64 },           // opener focus required
    AutomationPeriod { automation: u64, occurrence: u64 }, // base page is under
    PlanningAttempt { run: u64, from_settings: bool },     // T23 focus/return
    ProjectRename { project: u64 },
    TopicEditor { project: u64, topic: Option<u64> },
    TopicMove { topic: u64 },
    FolderDialog { project: u64 },
    NewProject,
    DraftDecision { pending: PendingExit },           // top only
}
```

Invariants enforced by `push_overlay` / `enter` (not by callers):

| Overlay | Valid base | Rule |
| --- | --- | --- |
| `Search` | any route, any stack | may stack over one modal; only one Search |
| `PlanningComposer` | any route | no route change; replaces an existing composer only after dirty decision |
| `TaskEditor` | `ResearchTask{same task}` | cannot open over another task |
| `AutomationSettings` | `AutomationDetail{same}` | cannot open over list/results |
| `AutomationPeriod` | `AutomationDetail{same}` or `AutomationResults` | occurrence must belong to automation; one period only |
| `PlanningAttempt` | `AutomationDetail` or `AutomationList`/`AutomationResults` per T23 | T23 owns attempt identity/focus |
| `DraftDecision` | top of any stack | exactly one; no other push while present |
| small editors | any route | explicit cancel only; one per kind |

`OverlayKind` (without payload) is the `DismissibleSurface` replacement and drives Escape/order/focus. Focus handles and Input entities stay in `TaskApp` keyed by `OverlayKind`; the pure state never holds GPUI types.

### 6.4 Return context

`ReturnFrame` is defined once in §6.2: route + full overlay stack below the destination (minus Search/Decision) + scroll bookmarks + optional deleted-run fallback hint. `ReturnStack { frames: Vec<ReturnFrame> }` is bounded at 8 with consecutive dedupe.

- Captured by the **controller**, from the typed state, at the moment a `DrillDown` command is accepted. Never reconstructed later from booleans.
- `Back` pops one frame, validates it against `&Store`, restores route + overlays + scroll, and shows at most one notice on fallback.
- Structural commands call `returns.clear()` inside `enter`; a caller cannot forget.
- `Escape`/`Close` never touch returns; they pop only the top overlay.
- The T10 `Origin` variants disappear. Their data is now carried by the typed route/overlay/page state.

### 6.5 Commands and controller surface

```rust
pub(crate) enum EnterMode { Structural, DrillDown, Replace } // Replace = same section, no return/clear
pub(crate) enum Destination {
    Route(Route),
    Overlay(Overlay),
    TaskOutcome { task: u64, surface: TaskOutcomeSurface }, // sidebar computes detail vs result
    Period { automation: u64, occurrence: u64 },            // sets base + overlay atomically
    Recent { task: u64, surface: RecentSurface },
    SearchTarget(SearchTarget),                              // T08 maps to Destination
}

pub(crate) enum NavCommand {
    Enter { dest: Destination, mode: EnterMode },
    Back,
    CloseTop,
    Escape,
    CloseTask,                    // payload: remove tab, then Back
    CancelAutomationWizard { editing: Option<u64> },
    CancelPlanning,
}

pub(crate) enum NavReject {
    BlockedByOverlay,
    BlockedByDecision,
    MissingOwner { kind: &'static str, id: u64 },
    InvalidOverlay { overlay: OverlayKind },
    NotAllowedHere { command: &'static str },
}
```

```rust
impl NavState {
    pub fn route(&self) -> &Route;
    pub fn overlays(&self) -> &[Overlay];
    pub fn top_overlay(&self) -> Option<&Overlay>;
    pub fn returns(&self) -> &ReturnStack;

    // The only mutators. All validate before writing, then write atomically.
    pub fn enter(&mut self, store: &Store, dest: Destination, mode: EnterMode)
        -> Result<CapturedReturn, NavReject>;
    pub fn back(&mut self, store: &Store) -> Result<Restored, NavReject>;
    pub fn close_top(&mut self, store: &Store) -> Result<Option<Restored>, NavReject>;
    pub fn push_overlay(&mut self, store: &Store, overlay: Overlay) -> Result<(), NavReject>;
    pub fn reconcile(&mut self, store: &Store) -> Option<Notice>; // after store replace/undo

    // Page-local mutations; never create returns.
    pub fn set_research_query(&mut self, query: String);
    pub fn set_research_filters(&mut self, all_status: AllStatus, layout: Layout);
    pub fn set_task_panel(&mut self, panel: TaskPanel);
    pub fn select_run(&mut self, store: &Store, run: u64) -> Result<(), NavReject>;
    pub fn set_results_filter(&mut self, filter: Option<u64>, limit: usize);
    pub fn set_settings_tab(&mut self, tab: SettingsTab);
}
```

`TaskApp` exposes exactly one command entry that applies the dirty policy first:

```rust
impl TaskApp {
    fn nav(&self) -> &NavState;                        // read-only
    pub(crate) fn request_nav(&mut self, command: NavCommand, w: &mut Window, cx: &mut Context<Self>) -> bool;
    // request_nav:
    //  1. if decision pending: only valid replies are handled by decide_exit
    //  2. if command would leave a dirty Decision surface: store PendingExit, focus decision, stop
    //  3. execute command through NavState, then apply scroll/focus/output effects
    //  4. never called from background code
}
```

`request_nav` is the only function allowed to call `NavState::enter/back/close_top/push_overlay`. `navigate`, `open_task`, `open_automation`, `open_occurrence_result`, `reopen_automation_settings`, `structural_navigation`, `record_origin`, `attempt_navigation`, `clear_navigation_overlays` are deleted or become thin deprecated wrappers that only build a `Destination` and call `request_nav` **from a UI handler**.

### 6.6 Derived views (one source)

```rust
pub(crate) enum SidebarSelection {
    Scope(Scope), Automations, AutomationRuns, Settings, PausedProjects, MarketResearch,
}

pub(crate) struct Crumb { pub label: String, pub destination: Destination }
pub(crate) struct Breadcrumbs { pub parents: Vec<Crumb>, pub current: String }
pub(crate) struct BackAvailability { pub enabled: bool, pub label: Option<String> }

impl TaskApp {
    pub(crate) fn sidebar_selection(&self) -> SidebarSelection;
    pub(crate) fn breadcrumbs(&self) -> Breadcrumbs;
    pub(crate) fn back_availability(&self) -> BackAvailability; // returns top frame, else structural parent
}
```

Rules:
- Sidebar selection derives from `route` only. A period overlay does not change the highlighted section; automation step pages derive `Project(task.project_id)` exactly as today.
- Breadcrumbs derive from `route` + `overlays`. A period over detail/results adds no crumb (DESIGN: dialog over the page). A `ResearchTask` for an automation step adds the `Period` crumb (T07 contract). A wizard adds `Automations / New automation` or `Automations / name / Edit automation`.
- Back availability uses `returns` first; if empty, the structural parent from `route`; Settings has no parent but may still have a return. Tooltip names the actual destination. Breadcrumb clicks ignore returns.
- `displayed_run_id`, result owner, project/topic labels and `title()` consume the same route/policy used for rendering (T23/T06 own the selection policy, not T12).

### 6.7 Validation / fallback ladder (typed)

| Route/overlay | Valid when | Fallback |
| --- | --- | --- |
| `ResearchList{Project}` | project exists | All research + notice |
| `ResearchList{Topic}` | topic exists | its project, else All + notice |
| `ResearchTask` | task exists (archived task viewable) | topic → project → All + notice |
| `ResearchResult{owner:Task}` | task exists; selected run belongs to it or policy replaces | results page with policy selection; notice only if the selected result vanished |
| `ResearchResult{owner:Any}` | selected run ordinary, or any when the caller was a task result | results page default; notice on vanished selection |
| `Settings` | always | n/a |
| `AutomationDetail/Editor{editing:Some}` | automation exists (archived viewable) | Automations list + notice |
| `AutomationResults` | always; filter normalized if missing | un/filtered results; no notice unless it changes the visible page |
| `Overlay::AutomationPeriod` | automation and occurrence exist and match | base detail, else list + notice |
| `Overlay::AutomationSettings` | base detail exists | detail/list + notice |
| `Overlay::PlanningAttempt` | run exists; T23 ownership | close to base + notice |
| `Overlay::TaskEditor` / `PlanningComposer` | owner exists; otherwise keep draft and show form error | do not silently drop text |

Referential validity is **not** a type invariant; it is validated at enter/restore/read. The type system's job is to make mutually exclusive destinations unconstructible.

## 7. T10 / T08 / T13 seams

### 7.1 T10 absorption (not a parallel stack)

| T10 symbol | T12 replacement | Migration rule |
| --- | --- | --- |
| `Origin` | `Route` + `ReturnFrame` | read-only during S6; delete after frames land |
| `OriginStack` | `ReturnStack` | same max 8/dedupe; stored inside `NavState` |
| `NavigationTarget` | `Destination` | `Recent` keeps `RecentSurface`; `ResearchResult` becomes typed page |
| `NavigationIntent` | `NavCommand::Enter` | `Structural`/`DrillDown` are `EnterMode`; Search/GeneratePlan are DrillDown commands |
| `capture_origin` precedence | `NavState.snapshot()` | deleted; no heuristic over booleans |
| `apply_origin` branch ladder | `restore(ReturnFrame)` | scroll/focus policy stays in app glue |
| `validate_origin` | `validate_frame` | same fallback semantics; notices preserved |
| `DismissibleSurface` | `OverlayKind` | one Escape path |
| `ExitTarget` | `NavCommand` | dirty policy remains `draft_exit`; target is typed |
| `clear_navigation_overlays` | `enter(Structural)` internal | callers cannot forget; dirty gate first |
| `record_origin` | controller capture | no UI call sites; structural pages cannot record by mistake |

T10's approved semantics that T12 **must preserve**: origin-first Back for drill-downs; structural sidebar/breadcrumbs; one dirty computation; Save only after confirmed persistence; one decision dialog; preserved comment/composer; search selection restores caller; Recently opened carries surface kind; planning completion never navigates.

### 7.2 T08 seam

- T08 keeps `GlobalSearchUi` (open/domain/selection/hits/scroll/focus) and the overlay renderer.
- T08 deletes `search_navigation_blocker`, `task_edit_holds_input`, `project_planning_holds_input`, `automation_draft_holds_input`.
- T08 activation calls exactly `self.request_nav(NavCommand::Enter { dest: self.search_destination(hit.target), mode: DrillDown }, w, cx)`.
- T12 `request_nav` performs the dirty decision, captures the frame **below Search**, pops Search only on proceed, and restores the caller (route + overlays + scroll) on Back.
- T08 continues to own `previous_focus`; T12 owns the route frame. Opening search never writes route/selection/filters/drafts; Escape closes only search.

### 7.3 T13 seam

- T13 makes `save_draft`, `save_automation_draft`, `capture_research_request` return a confirmed `SaveOutcome`/receipt.
- T12's `DraftDecision` consumes that result; it never infers success from an empty `error`.
- T12 does not change command preparation, staged-store semantics, dispatch, timers or post-commit effects. `Save draft`/`Enable` navigation happens only after the confirmed command result.

### 7.4 T23/T06 seam

- One result selection policy is required (T23). T12 stores the **effective** run in `ResearchResultPage.selection`; it does not keep `task_result_context` beside it.
- T06's typed presentation remains the renderer of the selected run. T12 only decides which run/page is displayed.
- Planning attempt identity/focus remains T23; T12 stores it as an overlay and never reconstructs `from_settings` heuristically.

### 7.5 Cross-owner change ledger and remaining unknowns

T12 cannot freeze these contracts unilaterally. Each item is a gate, not a T12 design choice. Status is as of the 22:26 handoff; re-check at S0.

| ID | Unknown / moving change | Owner | Why it blocks or changes T12 | Resolution evidence required | T12 consumption point |
| --- | --- | --- | --- | --- | --- |
| U1 | Final `request_*`/`ExitTarget`/decision signatures and when the dialog is rendered | T10 | T12 S6/S7 replace these types; if they still move, the bridge will be rewritten twice | T10 S5/S6 accepted handoff naming final symbols; decision UI reviewed | `NavCommand` mapping table (§7.1) |
| U2 | Decision Save may be **pending** while T21 impact confirmation is required | T21 + T10 + T13 | `SaveOutcome` only has Saved/Rejected/Unavailable. A pending impact is not Saved and must not navigate. T12 needs a "pending, keep decision and form" state or reuses `Stay{error}` without losing the pending impact | T21/T13 seam: confirmed impact -> durable receipt -> `Saved`; cancel keeps draft/route; no early Saved | `PendingExit`/`decide_exit`; do not add a second decision dialog |
| U3 | Staged task save does not yet reconcile approval/stage before the single write (T10 handoff B-1) | T10 interim, T13 `EditCommand` | A decision Save could persist a state different from what the user confirmed; T12 must not route on that receipt as if it were the reviewed state | B-1 fixed: staged edits + `reconcile_goal_approvals` + one confirmed write + read-back; or T13 command owns it | `save_decision_surface` / `SaveOutcome::Saved` |
| U4 | One result selection policy for `runs`, `displayed_run_id`, breadcrumbs, history, sidebar counts | T23 (with T06) | Without it, `ResearchResultPage` cannot atomically store task+run; C02/C17 remain possible even inside a typed route | T23 publishes the policy function and fixtures (replan twice, restart, both schema fixtures) | `ResearchResultPage::selection` |
| U5 | T08 activation integration: call `request_search_target`, delete local predicates, preserve focus | T08 | T12's "one gate" is false if search retains a second refusal path; Back restoration of the caller cannot be asserted | T08 adopts the published hook; QA activates from every caller and presses Back | `search_destination` -> `NavCommand` |
| U6 | Final split of `start_run` / `start_run_background` and `open_run_progress` payload | T02 (accepted) | T12 wants no boolean navigation parameter; a later T02 refactor could recreate it | Source shows two named entry points, no `navigate: bool` on the public path | User Run -> DrillDown result; background -> passive |
| U7 | T07/T08 QA expected routes (Back from period step; caller loss after search) | T07/T08/T10 | These tests currently encode the opposite of the T10 contract; T12's matrix will fail against them | Owners update expectations through real controls; T07 neighbor behavior unchanged | §10.2/§10.3 gates |
| U8 | Build/test baseline and 33-entry QA state | T01 | T12 cannot claim before/after if the baseline is unbuildable or failures are unclassified | T01 environment + failure ledger; green baseline or explicit blockers | S0 evidence |
| U9 | FocusHandle validity after overlay pop, and headless focus-trap behavior with stacked overlays | T10/T23/T19 | `Overlay` opener restoration depends on it; a stale handle silently focuses root | T10 S6 QA at both sizes/themes; T23 attempt pattern reused | `Overlay.opener` in app glue |
| U10 | Plan arrival may currently change the wizard step (`step=3`) | T10 + root decision | T12 wants background attach-only. This is a deliberate behavior correction, not a refactor | Root confirms decision; T10 makes attach-only; regression asserts step unchanged | `automation_plan_ready` payload hook |
| U11 | Period modeled as page route vs typed overlay over base | root (architectural) | Changes breadcrumbs and Back semantics; DESIGN says dialog over page | Root confirms §12.2 item 3 | `Destination::Period` mapping |
| U12 | `RecentSurface` memory bound and archived entries | T07/T10 | Typed `Destination::Recent` needs a destination hint; unbounded history is forbidden | Existing tabs bound retained; archived filtering preserved | `recent_surfaces` map |
| U13 | `query/layout/all_status` clearing semantics when structural vs drill-down | T10 | T12 must preserve "structural clears query, layout/status persist"; a page-state flip can accidentally change it | T10 QA + transition tests prove the current visible result | `ResearchListPage` normalization |
| U14 | Maintained route/action matrix updates | T17 | T12 cannot edit maintained docs during implementation; stale docs contradict behavior | T17 updates after acceptance | Documentation gate |

**Unknown policy:** if any U-item is unresolved at the start of its slice, T12 stops at the previous green slice and records the blocker. It does not invent a temporary second field or a heuristic bridge.

## 8. Complete route transition matrix (command semantics)

This is the behavior contract the typed API must encode. "Current" is the snapshot; "Target" is T12 with T10 behavior preserved. `D` = dirty Decision surface.

| Command | Preconditions | Route effect | Overlay effect | Return effect | Focus |
| --- | --- | --- | --- | --- | --- |
| `Enter(Route, Structural)` | no decision | replace route; validate; clear invalid selection | close all overlays | clear all frames | route landmark; cancel composer opener only if it was open |
| `Enter(Route, DrillDown)` | no decision; D handled first | replace route | close non-Search overlays unless destination is `Period` | push snapshot of current route + overlays (minus Search) | destination landmark |
| `Enter(Overlay, *)` | allowed base; D handled first | unchanged | push overlay (replace same kind) | unchanged | overlay's first control |
| `Enter(Period)` | occurrence/automation valid | set base detail/results if needed | push period | push caller frame when drill-down | modal focus; opener capture |
| `Enter(ResearchResult)` | run/task per policy | result page | close period/settings if drill-down from them | push caller | report landmark |
| `Enter(AutomationEditor)` | definition valid for edit; new allowed | editor page | clear settings/period | push settings/detail frame when opened from them | form first field |
| `Back` | no decision; no blocking overlay | pop one frame; validate/fallback | restore frame's overlays | pop one | frame landmark or top overlay opener |
| `Back` (empty returns) | structural parent exists | enter parent structurally | close overlays | already empty | parent landmark |
| `Back` (no parent, no returns) | n/a | no-op | no change | no change | unchanged |
| `Back` (overlay top) | overlay open | **blocked** (header is behind scrim; shortcut no-op) | unchanged | unchanged | unchanged |
| `CloseTop` | overlay open | unchanged | pop top only | unchanged | opener if valid, else page landmark |
| `Escape` | decision pending | unchanged | decision = Stay | unchanged | decision control |
| `Escape` | overlay open | unchanged | pop top only | unchanged | opener |
| `Escape` | no overlay | unchanged | unchanged | unchanged | unchanged (no wipe) |
| `CloseTask` | task tab selected | remove tab, then `Back` | unchanged | pop | Back target |
| `Cancel wizard` | editor route | enter list/detail structurally (clean) | clear editor | clear editor return (Settings is not restored) | list/detail landmark |
| `Cancel planning` | composer overlay | unchanged | pop composer | unchanged | opener |
| `Save draft` | T13 confirmed | enter detail | close editor | clear editor return | detail landmark |
| `Enable` | T13 confirmed | enter detail | close editor | clear editor return | detail landmark |
| `Generate plan` | T13 confirmed dispatch | enter result | close composer | push caller frame | result landmark |
| `Search activate` | dirty gate | enter destination | pop Search then destination overlays | push caller frame below Search | target |
| `Background plan ready` | matching editor session | **no change** | no change (payload plan attach only) | no change | no change |
| `Reconcile after store replace` | any | validate route; fallback | validate each overlay | drop invalid frames | predictable landmark |

### 8.1 Required behavior examples (before/after)

| Scenario | Before (snapshot) | After (target; behavior must look the same except fixed contradictions) |
| --- | --- | --- |
| Filtered All research → task → Back | query cleared, list scroll lost | same filter and scroll restored; task was a DrillDown |
| Results → period → step → Back | results page lost; period rebuilt over detail | period over results restored, dialog scroll restored; then Close returns filtered results |
| Automation detail → Settings → Edit → Back | settings restored (clean only); dirty edit silently lost | dirty Decision; Back restores detail + Settings dialog; Cancel returns detail |
| Period step → Previous/Next | stays inside period | unchanged (T07; no new returns for step-to-step) |
| Sidebar All research while dirty composer | composer silently cleared | Decision Save/Discard/Stay; sidebar destination applies only after Save/Discard |
| Search from settings → period → Back | period over automation detail; settings lost | period over detail with Settings restored below; Back returns Settings |
| Scheduled period starts while editing | no route change (T02) | unchanged; plan attach never changes route/step |
| Automation results filter → period → Back | filter survives in memory but Back may rebuild detail | filter and results scroll restored |
| Back on Settings page | hidden/no action | Back uses return frame when one exists; otherwise hidden |
| Escape with task edit open | wipes every dialog and draft | closes only task edit after Decision (or clean close) |
| ⌘[ with planning-attempt dialog | may go Back behind the modal | blocked; dialog keeps focus |
| Stop/Retry/Continue period | dialog stays, runtime only | unchanged (T03/T05/T13) |

## 9. Incremental adoption after T10 (safe slices)

**Precondition:** T10 S6 accepted: decision dialog rendered, all entry points routed through `request_exit`, no duplicate dirty predicates, save results confirmed, full suite green. T12 S0 verifies this; it does not start earlier.

Each slice is compile-green, has no new writable mirror, and ends by deleting the family it replaces. Read-only projections are allowed only inside a slice and are removed by the final slice.

### 9.0 Handback: proposed first authorized slice (no behavior change)

The safest first slice is **S1 only**, because it adds a new pure module and tests without touching any active T10/T13/T21/T08 surface:

- New `src/navigation_route.rs` containing only pure types and tests from §6.2–§6.4/§8. No `TaskApp`, `Window`, `FocusHandle`, `Storage`, no call sites.
- One `mod navigation_route;` registration line in `src/main.rs` (coordinate with T01; if T01 owns that line during the window, T12 supplies the one-line patch to root instead of editing).
- Tests encode the transition matrix and §5 recipes; they must compile and pass without a buildable app? They are part of the crate, so the shared tree must still be green. If T21/T13 keeps the crate unbuildable, S1 waits; T12 must not merge a test module it cannot run.
- No changes to `navigation_context*`, `draft_exit`, `app.rs`, `views.rs`, `automation_views.rs`, `global_search.rs`, schedule/runtime/persistence.
- Exit criterion: root reviews the type shape (especially U4/U10/U11) and confirms it is the intended authority before any wiring slice starts.

If root prefers a smaller first step, the only acceptable alternative is the pure type file without tests, but that cannot be accepted as a T12 slice. Do not begin S2 (route identity flip) until T10 S6 is accepted and U1–U4 are resolved; S2 touches the hottest files.

**Handback state:** this audit recommends S0 (evidence/freeze) as the next root action. S1 is the first code slice to authorize; no implementation is authorized by this document.

### S0 — Baseline and ownership freeze (no product edits)

- Re-read all hot files by symbol; record hashes (`shasum`) and the exact T10 handoff commit/state.
- Confirm owners idle or coordinated: T08 (search), T23/T25/T06 (result selection), T13 (save signatures), T01 (navigation QA/header), T07 (neighbor fixtures).
- Run and record the before matrix (§10.1) on a disposable workspace; capture screenshots and native click ledger. Do not restart the user's app.
- Add a source-symbol gate list (§10.4) to the T12 evidence directory; no code changes.

### S1 — Pure route/modal/return module (no wiring)

- Add `navigation_route.rs`: `Route`, page structs, `Overlay`, `Destination`, `ReturnFrame`, `ReturnStack`, `SidebarSelection`, `Breadcrumbs`, `BackAvailability`, validation/fallback, `NavReject`.
- Add pure tests: every transition row in §8; every contradictory state in §5 has a compile/construct failure or a normalized result; 8-frame bound/dedupe; owner deletion fallback; structural clears returns; Back frame restore; period overlay base validity.
- No `TaskApp` changes. This slice is small and independently reviewable.

### S2 — Route identity flip (delete `scope`)

- Add `NavState` with `route` + `Route` only; `scope` becomes a **read-only** projection (`fn scope(&self) -> Scope`) while collection/render code is migrated.
- Replace every `self.scope = ...` with `nav.enter(...)`/`nav.replace(...)`; `navigate(scope)` becomes `Destination::Route` + mode.
- Startup/undo use `reconcile`; no direct assignment.
- Delete `pub scope` from `TaskApp`. Grep gate: no `scope =`.
- Fixes C01/C06/C07/C08 route-level symptoms; leaves selection/overlay families until S3/S4.
- Gate: all unit tests + `--qa-sidebar`, `--qa-topics`, `--qa-settings`, `--qa-recovery`, `--qa-global-search`; transition tests for sidebar/breadcrumb/settings/recovery.

### S3 — Selection flip (delete `selected`, `task_result_context`, `selected_run`)

- `ResearchTask` and `ResearchResult` become the only selection carriers; `displayed_run_id` becomes `route.displayed_run(store)` using the T23 single policy.
- `select_run` updates the typed page selection and owner atomically.
- `open_task`/`open_sidebar_task` become `Destination` builders; `recent_surfaces` remains a bounded destination hint.
- Delete the three fields. Grep gate: no `selected =`, `task_result_context =`, `selected_run =`.
- Fixes C02/C03/C17; removes the `capture_origin` misclassification source.
- Gate: T07 sidebar suite unchanged except the intended Back page; results/picker tests; navigation QA.

### S4 — Overlay stack flip (delete automation booleans, draft flags, small-dialog flags)

- Introduce `overlays: Vec<Overlay>` with base/payload validation; move `automation_ui.selected/creating/editing/settings_dialog/result_dialog/planning_attempt_dialog` identity into `Route`/`Overlay`.
- Keep Input entities/baselines as payload keyed by overlay kind.
- Render the T10 Decision as `Overlay::DraftDecision`; implement `CloseTop` for Settings/Period/Attempt/Search/small dialogs; restore opener focus.
- Delete independent overlay booleans; `Escape`/Back/⌘[ all call `request_nav`.
- Fixes C04/C05/C07/C08/C09/C10/C22/C24.
- Gate: `--qa-automation`, `--qa-planning`, `--qa-global-search`, `--qa-affordance`, `--qa-intake`; new decision/dirty QA; focus restoration tests.

### S5 — Page state flip (delete global filters/tabs/planning mirrors)

- Move `query/all_status/layout`, `goal_steps_tab/steps_layout`, result `filter/history_limit`, settings `tab`, and `planning_project/topic/replan_source` into typed page/overlay state.
- `navigate` clearing semantics become `enter(Structural)` normalization: query clears, layout/status persist where today; Back restores the typed values.
- Delete `planning_project`, `planning_topic`, `planning_replan_source`; derive project/topic from `CreationMode`.
- Fixes C11/C12/C13/C14/C16.
- Gate: intake/affordance/topic QA; planning destination tests; results filter/back tests.

### S6 — Return-frame flip (delete T10 `Origin`/`OriginStack`/heuristic capture)

- Replace T10 `Origin` stack with `ReturnStack`; controller captures snapshots from typed state; `apply_origin` and `capture_origin` are deleted.
- Structural enter clears returns; Back pops/validates/restores; notices preserved.
- T08 activation switches to `request_nav`; duplicate search predicates deleted.
- Fixes C13/C14/C15/C19/C20.
- Gate: complete §10.2 chain, search origin/Back from every caller, T10 QA; T07/T08 expected-route updates.

### S7 — Command gate and dirty authority

- `request_nav` is the only command entry; `draft_exit` remains the single dirty policy; no `clear_navigation_overlays`; no direct overlay writes.
- `request_escape` = Stay / CloseTop / no-op; ⌘N through the gate; ⌘[ blocked by overlays.
- `Generate plan`/manual run/progress callers explicitly request the destination after confirmed dispatch; background callers never do.
- Fixes C23; S7 also adds the background no-navigation regression.
- Gate: T02 acceptance scenarios re-run; decision Save/Discard/Stay for all three surfaces; preserved comment/composer tests; keyboard matrix.

### S8 — Delete legacy and reconcile documentation/evidence

- Delete `navigation_context.rs` origin types (keep creation destination policy in route/project module) and `navigation_context_app.rs` wrappers; keep `navigation.rs` T07 helpers only.
- Remove all compatibility projections (`scope()`, `selected()`, etc.); render/sidebar/breadcrumb consume `Route` directly.
- T17 updates `doc/ROUTE_ACTION_MATRIX.md`; T01/T07/T08 owners update stale expectations through real controls.
- Final gates: full unit, 33-entry offline dispatch (or the documented navigation subset with reasons), T12 rendered QA at 1440×920/1000×680 light/dark, T20 native journeys. No performance run required.
- T12 closes only after root reviews evidence and no forbidden writer remains.

## 10. Before/after regression QA gates

### 10.1 Before (freeze the behavior T12 must preserve)

Run on a disposable workspace and record commands, exit codes, artifacts and source hashes. Do not claim a pass for anything blocked or not run.

1. `cargo test --quiet` (Node 24 on `PATH` for the mandatory report-builder test). Record the exact count; current tree may be mid-integration.
2. `cargo build --features qa`, then sequentially:
   - `--qa-sidebar` (T07 navigation, Back, Overview, recent entries)
   - `--qa-global-search` (caller matrix, domains, activation, Escape/focus, dirty forms)
   - `--qa-topic-actions`, `--qa-topics`, `--qa-simplify`, `--qa-intake`, `--qa-affordance`
   - `--qa-automation`, `--qa-planning`, `--qa-batch`, `--qa-recovery`
   - `--qa-execution-eligibility` only if touched (not planned)
3. Capture the mandatory journeys in §10.5 at 1440×920 and 1000×680, light/dark where a dialog appears.
4. Record the known pre-existing failures separately (T01 baseline). A T12 slice cannot be signed off by hiding them; it must not add failures.

### 10.2 Chain gate (T10 acceptance, must pass before S2)

- Filtered All research/Project/Topic → task/result → Back restores exact filter, layout and scroll.
- Automation results (filtered) → period → step → Back restores the same period over **results**; Close returns to the same filtered results/scroll.
- Automation detail → Settings → Edit → Back restores Settings without losing unrelated input; dirty edit offers Save/Discard/Stay; Cancel returns detail.
- Dirty planning composer × sidebar/breadcrumb/⌘[/⌘N/Cancel/Escape → Save request / Discard / Stay; confirmed save only; failure stays.
- Comment and Market research composer survive navigation and expose discard.
- Search from every caller → activate Research/Automation/Period → Back restores caller page, filter, scroll and open modal; Escape restores focus/draft without route change.
- Recently opened report entry reopens report; task entry reopens detail.
- Settings with a return frame shows Back and returns to the caller; no return → structural/no-op.
- No background start/plan completion/cursor tick changes route, selection, overlay, step, filter, scroll, focus or draft text.

### 10.3 Transition-matrix gate (new)

Encode every row of §4.1–§4.4 and §8 as a test:

- Pure test: `(initial NavState, command) -> expected NavState` including returns, overlays, validation/fallback and notices. Include all contradictory recipes from §5: each must be unconstructible or normalize deterministically.
- Rendered/headless test: for every entry control, click the **real control** and assert the destination, breadcrumb, sidebar selection, Back target, overlay stack, focus restoration and preserved text. Do not add helper-only routes to make an obsolete assertion pass.
- Source gate: grep for forbidden writes/calls (§10.4); a match fails the slice unless it is a listed UI builder.

### 10.4 Source gates

Must be empty after S8 (allow only constructors in `navigation_state.rs`/`navigation_route.rs`):

```
# Route/selection/overlay/return writers outside the controller
grep -rn "\.scope\s*=\|\.selected\s*=\|\.task_result_context\s*=\|\.selected_run\s*=\|\.creating\s*=\|\.editing\s*=\|\.settings_dialog\s*=\|\.result_dialog\s*=\|\.planning_attempt_dialog\s*=\|\.goal_steps_tab\s*=\|\.steps_layout\s*=" src --include='*.rs' | grep -vE 'navigation_(route|state)\.rs|_qa\.rs'
# Old navigation API
grep -rn "structural_navigation(\|record_origin(\|apply_origin(\|capture_origin(\|clear_navigation_overlays(\|attempt_navigation(" src --include='*.rs' | grep -vE 'navigation_(route|state)\.rs|_qa\.rs'
# Background navigation authority
grep -rn "navigate(\|open_task(\|open_automation(\|open_occurrence_result(" src/automation_runtime.rs src/scheduler.rs src/workflow.rs src/execution_*.rs src/domain_command.rs src/command_app.rs
```

Permitted matches must be documented as UI call sites with a `// user-command` annotation and covered by the rendered matrix.

### 10.5 Native journeys (T20-aligned, no claim until run)

On a separately identified disposable test instance/workspace, with fake/offline execution where supported, record Passed/Failed/Blocked/Not run and expected/actual/evidence:

1. Every sidebar item (Research and Automations) at 1440×920 and 1000×680.
2. Breadcrumb vs Back from filtered list/board, topic, task, result, automation detail/results, settings.
3. Results → period → step → Previous/Next/Overview → Back → Close.
4. Automation detail → Settings → Edit/Replan → dirty Cancel/Back/Escape × Save/Discard/Stay.
5. New research from footer/⌘N/project/topic; change destination; Save request; Generate plan; Cancel dirty.
6. Global search from All/Project/Topic/task/result/settings/automation list/detail/results/period/settings dialog; activate Research/Automation/Period; Escape; Back.
7. Recently opened task vs report; Close current task.
8. ⌘K, ⌘N, ⌘[, Escape, Tab/Shift-Tab/Enter/Space; no focus escapes a modal; opener focus restored.
9. Scheduled/manual period start, two step advances, planning completion while a dirty form/period/settings is open: route, selection, filter, scroll, focus and text unchanged.
10. Long English/CJK titles and both themes; no clipped Back/crumb/Recently opened.

### 10.6 Current native/headless gaps (do not claim as passes)

| Gap | Evidence | Owner/action |
| --- | --- | --- |
| No T12 implementation exists | Source at 22:26 | This audit; implementation awaits authorization |
| T10 decision UI missing; `views.rs` still uses legacy Escape/Back/⌘[ | Source inspection; T10 handoff §“Scaffolding present but NOT wired” | T10 S5/S6 must land first |
| T21 impact confirmation can make Save pending; decision cannot yet represent it | T10 handoff gaps 3 / U2 | T21 + T10 + T13 seam must publish the pending/Saved contract |
| T10 handoff B-1: staged task save may bypass approval/stage reconcile | T10 handoff §“Post-capture review backlog” | T10 interim or T13 `EditCommand`; T12 must not route on an unreconciled receipt |
| T08 search bypasses T10 `request_search_target`; duplicates dirty predicates | `global_search.rs:709-880` | T08 follow-up after T10 publishes the seam |
| Tree does not compile at audit snapshot | `cargo check` errors in §1; T10 handoff reports T21 `research_input_views.rs` blocker and 486 pre-S5 tests | Concurrent owners; re-run at S0 |
| `navigation_qa` asserts an obsolete Back page and uses structural helper paths | `navigation_qa.rs:268-300` | T07/T10/T01 reconcile via real controls |
| `global_search_qa` does not test Back after activation; asserts settings is lost after period selection | `global_search_qa.rs:793-815` | T08/T10 update expected caller restoration |
| No rendered dirty-decision QA, no `--qa-navigation-context` | `main.rs` dispatch list, module list | T10 S6 adds it; T12 adds the transition matrix after |
| Result dialog/settings Close focus root, not opener | `automation_views.rs:867/921` | T10/T12 overlay focus contract |
| No native evidence for T12 | No app launched | T20 gate after S8 |
| Scroll capture for list/topic/board was flagged in T10 review | `navigation_context_app.dart` capture uses handles now; theme-independent native check pending | T10 acceptance + T12 restore tests |

## 11. Owner seam changes required (exact)

| Owner | Required change | Why |
| --- | --- | --- |
| T02 | Remove the internal `navigate: bool` from `start_run_with_navigation`; keep two explicit user/passive entry points; make `open_run_progress` a UI command that calls `request_nav`. | No completion path should even have a navigation parameter. |
| T07 | Keep `task_navigation`/neighbors untouched; update `navigation_qa` expected Back page only through a real period-step click and the T10 contract. | Preserve accepted behavior; stop asserting the old page. |
| T08 | Delete local dirty predicates; call `request_nav` on activation; keep overlay/focus; Back restores caller + below-modal stack. | One authority, no second history. |
| T10 | Publish final `dirty_draft`/decision UI/confirmed-save seam; delete `Origin` capture at T12 S6; keep creation destination/dirty semantics. | T12 absorbs T10, not duplicates it. |
| T13 | Return confirmed `SaveOutcome`/receipt from save/enable/request-save; no route effects inside commands. | Decision and Generate plan wait for durability. |
| T23 | Publish one result selection policy for `runs`, `displayed_run_id`, breadcrumbs, history and sidebar counts; T12 stores its result. | Eliminate C02/C17 and the duplicate filters. |
| T06 | Keep typed presentation as the renderer of the selected run. | T12 must not reinterpret result quality. |
| T01 | Reconcile header/navigation QA, 33-entry gate and narrow-window Back visibility. | Back availability now derives from returns + route. |
| T17 | Update `doc/ROUTE_ACTION_MATRIX.md` after T10/T12 acceptance. | Maintained documentation stays consistent. |
| T19 | Extend keyboard matrix to overlay stack/focus/Decision. | Escape/Back/⌘[ semantics change only in consistency, not destinations. |
| T22 | No route change; archived owners remain viewable in validation. | Do not invent archive behavior in nav. |

## 12. Risks, decisions and stop point

### 12.1 Risks

1. **Concurrent edits.** T10/T13/T21 are still writing the exact files. Any T12 edit before S0 will conflict or encode a stale model. The handoff already demonstrates the tree moving from three errors to a different T21 blocker within minutes.
2. **Pending save ambiguity.** T21 can make the decision Save asynchronous (impact confirmation before durable receipt). T12 must model "pending" explicitly or reuse `Stay` without losing the pending exit; otherwise it can navigate or claim Saved early.
3. **Behavioral drift disguised as cleanup.** Moving `scope`/selection must preserve visible destinations, query-clearing semantics, scroll restoration and focus. The transition matrix is the guard.
4. **Policy leakage.** T12 must not decide result quality, eligibility, schedule state or save semantics. It only routes to typed pages and preserves the selected owner.
5. **Test preservation pressure.** Some current QA asserts the wrong page (T07 Back, T08 caller loss). Do not add helper APIs to keep those; update through real controls with the owning task.
6. **Partial mirror temptation.** A half-migrated tree can look type-safe while `pub selected`/`creating` still allow contradictions. S2–S6 therefore end with field deletion and grep gates, not with a “compatibility state” that persists.
7. **Focus vs route confusion.** Focus handles and `modal_focus` are not navigation state. The typed overlay owns order/opener identity; do not route on focus changes.

### 12.2 Decisions requested from root (no implementation until answered)

1. Confirm the S0 gate: T12 starts only after T10 S6 is accepted and the tree is green.
2. Confirm the migration-by-state-family order (route → selection → overlays → page state → returns → gate → delete) rather than surface-by-surface mirroring.
3. Confirm that `AutomationPeriod` is a typed **overlay destination** over a base page (preserving the DESIGN dialog/breadcrumb contract), while `ResearchTask`/`ResearchResult`/`AutomationDetail` are page routes.
4. Confirm that `ResultSelection` stores the effective run and that T23 owns the only selection policy; T12 deletes the duplicate `project/topic` results context from the live route (retaining it only as return-frame fallback while restoring).
5. Confirm that background plan arrival may attach a plan to an open editor payload but **must not change the current step**; this is a deliberate correction of the current hidden `step=3` write (U10).
6. Confirm how a **pending T21 impact save** is represented in the decision: T12 will not treat "pending" as `Saved`, will not navigate, and will keep the dialog/form until a durable receipt (U2). If T13 exposes a distinct pending result, T12 consumes it instead of overloading `Stay`.
7. Confirm that the first authorized slice is S1 (pure module + tests only), with S2 gated on T10 S6 and U1–U4.
8. Confirm the evidence gate set in §10 (unit + navigation subset + new matrix + native journeys) and that T12 cannot close on a unit-only result.

### 12.3 Stop

No source, maintained documentation, frozen snapshot, production data or running app was modified. No commit or push. The next action is root review/authorization of this plan; implementation begins at S0 only after T10 and the other owners confirm the seams.

**Handback summary:** the audit is complete against the evidence available at 22:26 CST. The stable seams are named in §3.3; the moving/cross-owner items are enumerated in §7.5 (U1–U14); the first non-overlapping code slice proposed for authorization is S1 (pure `navigation_route.rs` + tests only), with S2 gated on T10 S6 and U1–U4. No implementation, commit or push was performed.

## 13. Appendix — snapshot line index (evidence only; not a patch plan)

Line numbers were captured at 22:26 CST while T10/T21/T13 were editing. Treat this table as a search aid; the contract is §3.3, §6 and §7.5.

| Symbol | File:line at 22:26 |
| --- | --- |
| `navigate` | `src/app.rs:801` |
| `open_collection_project` | `src/app.rs:854` |
| `sidebar_scope` | `src/app.rs:863` |
| `open_task` | `src/app.rs:908` |
| `open_run_progress` | `src/app.rs:1574` |
| `select_run` | `src/run_views.rs:177` |
| `show_task_result` | `src/research_app.rs:735` |
| `open_sidebar_task` / `go_back` / `clear_navigation_overlays` / `reopen_automation_settings` / `displayed_run_id` | `src/navigation.rs:361/477/447/471/426` |
| `navigation_path` | `src/views.rs:1221` |
| header Back / recently opened | `src/views.rs:1507-1580` |
| footer Search/New research | `src/views.rs:594/603` |
| shortcuts | `src/views.rs:3804-3865` |
| `open_automations` / `open_automation` / settings / period | `src/automation_views.rs:833/838/856/873` |
| close settings / close period / attempt | `src/automation_views.rs:867/921/928-963` |
| create / replan / edit / plan-ready | `src/automation_views.rs:965/996/1047/1359` |
| save draft / enable | `src/automation_views.rs:1480/1526` |
| period step rows | `src/automation_views.rs:3406-3460` |
| wizard cancel | `src/automation_views.rs:3796` |
| save/enable controls | `src/automation_views.rs:4217/4248` |
| `capture_origin` / `apply_origin` / `request_exit` / `request_escape` / `request_search_target` | `src/navigation_context_app.rs:116/314/676/995/1032` |
| `open_project_plan` / `open_topic_creation` / `open_topic_replan` / `open_research_intake` / `start_project_plan` | `src/project_app.rs:9/109/129/157/249` |
| `finalize_project_plan` | `src/project_app.rs:338` |
| `open_model_settings` | `src/settings_views.rs:170` |
| `open_global_search` / activate / blocker | `src/global_search.rs:611/709/789` |
| recovery links | `src/recovery.rs:646/685/739/764/825` |
| intake request links | `src/intake_views.rs:140/151` |
| batch progress | `src/batch_app.rs:385` |
| topic rows/report/new/replan/source | `src/topic_views.rs:337/484/521/552/277/585` |
| runs empty CTA / result actions | `src/run_views.rs:468/898-930` |
| `navigation_qa` obsolete Back assertion | `src/navigation_qa.rs:268-300` |
| `global_search_qa` caller loss assertion | `src/global_search_qa.rs:793-815` |
