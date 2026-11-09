# T10 audit — origin, dirty drafts and creation destination

Status: read-only audit complete. No product, source, test, documentation, snapshot or workspace file was changed. This file is the only artifact.

Task: T10 (P2/M), dependencies T02 (accepted behavior), coordination T08, T12, T23/T25, T13, T22, T11, T07, T01.
Accepted contract references: TASKS.md target contracts 1, 3, 6, 7; T10 acceptance; DESIGN.md lines about automation detail Back, dialog Escape, and period-boundary Back; PRODUCT.md creation/planning paragraphs.

## 1. Evidence base and limits

- Repository HEAD: `611a4586eedb56600a83c8b272a3af43ba95e4a4`, working tree heavily modified by the active program (T05/T06/T08/T23/T25/T01 work is in flight).
- Source was inspected directly; findings are labeled:
  - **[code]** confirmed by reading the current source path named.
  - **[test]** confirmed by an existing QA/unit assertion that encodes (or contradicts) the behavior.
  - **[proposal]** target behavior decision required by T10, not a claim about today's code.
- **Volatility warning:** `src/app.rs`, `src/views.rs`, `src/automation_views.rs`, `src/global_search.rs` were rewritten by other sessions during this audit (mtimes moved between 21:14 and 21:29 CST). Line numbers below were captured at 2026-11-08 21:30 CST and will drift. Every reference also names the function/symbol; implementation must re-anchor by symbol, not by line number.
- `src/global_search.rs` is untracked (new, T08-owned). `src/project_views.rs` and `src/navigation.rs` were stable during the audit.
- No native app was launched. No user workspace was opened. Existing QA suites were read, not executed. No frozen baseline snapshot source was touched.
- No VoiceOver/screen-reader work is in T10. Keyboard/focus and visual behavior are in scope; `doc/ICON_GUIDELINES.md` rules apply only if a new icon is introduced (none is planned).

## 2. Executive conclusions

1. There is **no return context and no navigation history**. `TaskApp` stores one route (`scope`), one task selection (`selected`), one result selection (`task_result_context`), one automation sub-route (`automation_ui.selected/creating/result_dialog`), and filters. Back is derived exclusively from structural breadcrumbs (`go_back` → `navigation_path().last()`), so "origin" is not represented anywhere. [code] `src/navigation.rs:474`, `src/views.rs:1214`.
2. **Every ordinary navigation clears list filters and all overlays.** `navigate` unconditionally clears `query`, the search input, `selected`, `task_result_context`, `draft`, `editing`, `property`, planning fields, automation dialogs and the automation wizard (`creating`) when the target is Automations. [code] `src/app.rs:735-784`, `src/navigation.rs:447-464`.
3. **Global New research navigates before it asks.** `open_project_plan` calls `navigate(Scope::Project)` and forces `layout = Board` **before** opening the overlay; `open_research_intake` then picks a destination without showing a control. Cancel/Escape therefore cannot return to the caller; from All research a filter is already gone. [code] `src/project_app.rs:8-22`, `:39-58`, `src/project_views.rs:92-126`.
4. **The creation destination is implicit and can be invalid.** Global New research falls back to `self.research_project` (a process default set at workspace load and only otherwise changed on the Research page). If that id no longer exists the overlay silently renders nothing and the button appears dead. [code] `src/project_app.rs:44-55`, `src/project_views.rs:94-97`, `src/app.rs:258` (field), `:1150` (load assignment).
5. **The automation wizard is page content, not a modal**, so sidebar, breadcrumbs, header Back, ⌘[ and ⌘N are all reachable while it holds the only copy of a new/edit form. Header Back and Cancel discard it with no decision; navigating away to another scope leaves `creating=true`, so an abandoned wizard can silently reappear later with stale values. [code] `src/automation_views.rs:1815-1822` (`automations`), `:950-980` (`start_automation_create`), `:1018-1046` (`edit_automation`), `:3339-3341` (`automation_wizard`), Cancel at `:3339` block; `src/app.rs:766-770`; `src/views.rs:1214-1261`.
6. **Escape is a global overlay wipe**, not a dismiss operation: it clears the task editor, planning composer, all small editors and both automation dialogs, focuses the window root, does not restore any opener, and leaves `planning_topic` behind. It also does nothing about the wizard. [code] `src/views.rs:3725-3742`.
7. **Filters are lost on Back, scroll is lost almost everywhere.** `navigate` always clears the query text [code] `src/app.rs:776-777`; the research list/board has no tracked scroll handle [code] `src/views.rs:1674-1700`, `:2080-2170`; automation list/detail/results/wizard all share one handle that each entry resets [code] `src/automation_views.rs:301`, `:820-825`, `:825-840`, `:2013`, `:2224`, `:3317`, `:2135-2140` (filter reset).
8. **Period → step loses its page origin by construction.** Opening a period from Automation results keeps the results page, but opening a step clears the dialog and Back always rebuilds the period over the automation **detail**, so a filtered results list and its scroll cannot be returned to. The current QA asserts this loss as expected. [code] `src/automation_views.rs:3067` (`automation-step-{id}`), `src/navigation.rs:460` and `:482-487`, `src/automation_views.rs:825-840`; [test] `src/automation_qa.rs:5136-5233`.
9. **Modal Close does not restore the opener.** Period result and settings handlers focus the window root; only the T23 planning-attempt dialog restores an exact opener. The generic Escape path restores none. [code] `src/automation_views.rs:854-857`, `:910-915`, `:931-948` (T23 pattern), `src/views.rs:3725-3742`.
10. **"Planning…" discoverability is volatile.** The list chip and detail notice depend on the in-memory `automation_ui.planning_automation`, not on durable request/attempt state; after leaving for good, a UI reset, or a restart, a saved draft with an in-flight or unreviewed attempt can show no planning state and no Review action. [code] `src/automation_views.rs:1868`, `:2248-2266`; durable data exists in `src/request_planning.rs:411-442`.
11. **The search blocker is a second, narrower guard.** T08 added `search_navigation_blocker` + `automation_draft_dirty` inside its own module, covering only `draft`, `planning_project` and five automation fields; it is not the T10 policy and will drift. [code] `src/global_search.rs:761-792`, activation at `:688-741`.
12. **Accepted T02/T07 behavior is intact and must stay so.** Dispatch no longer routes (`launch_run` returns `RunDispatch`; completion paths only mutate state). Back from a period step returns to the same period via the period crumb; `task_navigation`/neighbors are period-scoped. T10 must add origin only in click handlers and must not touch `task_navigation`, `TaskNeighbor`, `detail_task_neighbors`, `automation_period_steps`, or completion/retry callbacks. [code] `src/app.rs:1454-1466` (`open_run_progress`), `src/navigation.rs:210-306`, `src/project_app.rs:196-306` (`finalize_project_plan`).

## 3. Current state model (as observed)

`TaskApp` route/state fields relevant to T10 [code] `src/app.rs`:

| Field | Meaning | Cleared by |
| --- | --- | --- |
| `scope: Scope` | research/automation/settings page | set by `navigate`; direct writes by automation handlers |
| `selected: Option<u64>` | task detail | `navigate`, `show_task_result` |
| `task_result_context: Option<u64>` | which task a result page belongs to | `navigate`, `open_task` |
| `selected_run` | result selection | `navigate(Runs)` partly, `open_run_progress`, `reveal_selected_run` |
| `query` + `search` InputState | list-local text filter | always cleared by `navigate` |
| `all_status`, `layout`, `show_done`, `compact` | list/board filters | persist across navigation |
| `draft`, `editing`, `property` | task detail editor | `navigate`, `open_task`, `clear_navigation_overlays`, Escape |
| `planning_project/topic/replan_source` + `project_brief` | research request composer | `navigate`, `clear_navigation_overlays`, Escape; brief also cleared each open |
| `automation_ui.creating/editing/step/planning_*` | automation wizard + planning token | `navigate(Automations)` sets `creating=false`; start/edit/plan resets |
| `automation_ui.selected` | automation detail | `navigate(Automations)`, `open_automations` |
| `automation_ui.settings_dialog/result_dialog/planning_attempt_dialog` | automation modals | many handlers; Escape wipes |
| `automation_ui.result_filter`, `history_limit` | automation results filter | persist in memory; filter has an existence fallback |
| `automation_ui.scroll` | one shared scroll for list/detail/results/wizard | reset by each entry |
| `detail_scroll`, `result_scroll`, `sidebar_scroll` | scroll handles | detail resets on task switch; result resets on run select |
| `focus`, `modal_focus` | window/modals | dialogs focus `modal_focus`; closes focus `focus` |

Existing navigation helpers:

- `navigation_path()` builds breadcrumb links and a "current" label; it is also the only source for Back. It special-cases automation list/create/edit/detail/results and automation task ownership. [code] `src/views.rs:1214-1420`.
- `go_back()` applies the last crumb. [code] `src/navigation.rs:474-490`.
- `clear_navigation_overlays()` wipes overlays without any dirty check. [code] `src/navigation.rs:447-464`.
- `justfile`-style dirty tracking does not exist; `has_unsaved_changes()` compares the whole `Store` with `last_saved`, not form content. [code] `src/app.rs:1235`.

### What persists today (summary)

| Context | route | list filter | scroll | selection | focus | draft/content |
| --- | --- | --- | --- | --- | --- | --- |
| All research / Project / Topic list+board → task | changes | **query cleared**; status/layout kept | **lost** (no handle) | detail selection kept only while staying | root/first focus | n/a |
| Task → View result | Runs | query irrelevant (cleared only if navigate used) | result scroll reset | run selected | root | detail scroll preserved |
| Task detail → Edit task (Escape/Cancel/Save) | stays | n/a | detail preserved | stays | root | **lost on Escape/navigate; Save works** |
| Topic → Report | Runs | n/a | reset | task_result_context | root | n/a |
| Automation list → detail | scope stays Automations | n/a | **list scroll lost** (shared handle reset) | detail selection | root | n/a |
| Automation detail → period dialog | stays | n/a | body has no handle | dialog id | modal focus; **opener not restored** | n/a |
| Period → step | stays on the underlying scope | automation filter not shown | **dialog body scroll lost** | step page | root | n/a |
| Step → Back | rebuilt as detail+period | automation filter kept in memory only | reset | period | modal focus | n/a |
| Automation detail → Settings → Edit | detail replaced by wizard | n/a | settings scroll kept | editing id | root/form | **unsaved wizard lost on Back/Cancel** |
| Automation list → New automation → navigate away | wizard replaced | n/a | reset | n/a | root | **lost / sometimes resurrected** |
| Global New research → Close/Escape | **already changed to a project board** | **query cleared** | reset | none | root | brief kept until next open, which clears it |
| New research → Save/Generate | Runs/planning | n/a | reset | planning run | root | request persisted on submit only |

## 4. Transition inventory

Notation: "origin" = state T10 proposes to capture; "today" describes the current code.

### 4.1 Research surfaces

| # | Entry / control | Code path | Today: state change | Today: Back/Close | Loss / unexpected destination |
| --- | --- | --- | --- | --- | --- |
| R1 | Sidebar All research / Cycles / Archive / Paused projects / Research results | `nav` `src/views.rs:1080-1170` → `navigate` `src/app.rs:735` | scope set; query cleared; overlays cleared | Back = structural parent of the page | structural, intended; clears query (documented current behavior) |
| R2 | Sidebar project row / chevron | `src/views.rs:774-841` | scope Project; focus/expand | Back to All | structural |
| R3 | Sidebar topic row / directory card | `src/topic_views.rs:302-356`, `:358-414` | scope Topic | Back to project | structural |
| R4 | Sidebar task row | `src/views.rs:928-961` → `open_sidebar_task` `src/navigation.rs:361-366` | `navigate(Project)` then detail; opens completed result if any | Back = structural parent | opening a completed goal **skips detail**; origin not captured; query cleared first |
| R5 | List row | `task_row` `src/views.rs:1935-1959` | `open_task` | Back = structural parent (project/topic/goal) | **filter/scroll lost**; Back is project, not the filtered All research list |
| R6 | Board card | `goal_card` `src/views.rs:2172-2200` | `open_task` | structural | same as R5; board column scroll not preserved |
| R7 | Card “Open N steps” | `src/views.rs:2290-2308` → `open_goal_steps` `src/goal_steps.rs:224-229` | opens goal, Goal/Steps tab, `steps_layout=Board` | structural | returns to project, not the board/list it was opened from |
| R8 | Goal Steps tab step row / card | `src/goal_steps.rs` step actions (Run/Open goal/Technical details at `:192-214`) | `open_task` / `view_task_result` | structural | step → Back returns the goal (same page) today; origin tab state is global and happens to persist |
| R9 | Detail “View latest result / View final report” | `src/views.rs:2502-2520` → `view_task_result` `src/research_app.rs:704` → `show_task_result` `:716-725` | scope Runs, `task_result_context=task`, `selected=None` | Back = task crumb | detail scroll preserved; origin not recorded (same result as structural) |
| R10 | Result page “More result actions → Open task/project” | `src/run_views.rs:896-927` | `open_task` or `navigate(Project)` + Board | structural | **returns to goal/topic/project, losing the result page** (deliberate boundary in this audit; see 7.3) |
| R11 | Result page project/topic/planning pickers | `src/run_views.rs:318-430` | direct `selected_run` writes | Back unaffected | selection is not a route; Back loses it |
| R12 | Topic header “New research” | `src/topic_views.rs:532-534` | `open_project_plan(project)` then `planning_topic=Some(topic)` | X only clears planning fields | **route already changed to Project; Cancel leaves the project board, not the topic** |
| R13 | Topic “Replan” | `src/topic_views.rs:270-275` → `open_topic_replan` `src/project_app.rs:24-37` | same navigate-first open | same | same as R12; replan text prefilled |
| R14 | Topic “Report” | `src/topic_views.rs:495-507` | `open_task(report)` then `view_task_result` | Back = report task crumb (skips topic) | topic origin not captured |
| R15 | Topic “Refresh/Regenerate plan” | `src/topic_views.rs:276-299` | dispatches; explicit navigation only via request progress | n/a | out of T10 (T23/T25) |
| R16 | Project toolbar “New research” | `src/project_views.rs:61-69` → `open_project_plan` | navigate-first; layout Board | X only clears | navigation changed before the modal; Cancel does not restore prior layout |
| R17 | All research toolbar “New research” | `src/views.rs:1629-1640` → `open_research_intake` | navigate to `research_project` + Board | X only clears | **origin lost; destination implicit** |
| R18 | `task-search` list filter | `src/views.rs:1684-1700` | `query` changes only | Back after leaving clears it | filter not restored on origin |
| R19 | Goal/board drag submission | `src/app.rs` `change_stage`/batch | explicit submit | no route change (T02) | preserve |
| R20 | Task tab / “Close current task” | `src/views.rs:1521-1595` → `close_current_task` `src/navigation.rs:491-497` | removes tab, `go_back` | structural | if the tab was a report, Back falls to task/goal |

### 4.2 Automation surfaces

| # | Entry / control | Code path | Today: state change | Today: Back/Close | Loss / unexpected destination |
| --- | --- | --- | --- | --- | --- |
| A1 | Sidebar All automations | `automation_child_nav` `src/views.rs:1152-1212` | scope Automations; list; `selected=None`; `creating=false` | Back hidden at list root | structural |
| A2 | Sidebar Automation results | same | scope AutomationRuns | Back to Automations | structural |
| A3 | Automation list row | `src/automation_views.rs:1789-1990` (row click ~`:1889`) | `open_automation`; `scope=Automations`, `selected=Some`, `creating=false`, **scroll reset** | Back to list | **list scroll lost**; no origin object |
| A4 | “New automation” | `src/automation_views.rs:950-980` | `creating=true`, fresh defaults, clears planning tokens | header Back → list; crumb “Automations” → list; Cancel → list/detail | **no dirty check; unsaved form silently lost**; no origin (list implied) |
| A5 | Detail actions (Pause/Resume/Run/Stop) | `src/automation_views.rs:2200-2250` | runtime only | n/a | out of T10 (T13/T03) |
| A6 | Detail Settings | `open_automation_settings` `:843-852` | `settings_dialog=true` | Close → root focus | **opener focus not captured**; settings scroll persists via handle |
| A7 | Settings → Edit | `edit-automation` `:2529-2535` → `edit_automation` `:1018-1046` | `creating=true`, `editing=Some`, form from definition, settings dialog closed | header Back → `CrumbTarget::Automation` → `reopen_automation_settings` `src/navigation.rs:468-472` | **works for clean state** [test `automation_qa.rs:5036-5053`]; **dirty edits silently discarded**; no explicit “back to Settings” origin |
| A8 | Settings → Replan | `replan-automation` `:2509-2515` → `replan_automation` `:980-998` | edit form, `plan=None` | same as A7 | same as A7; Back from a replan with no typing currently clean |
| A9 | Settings → Plan versions → Review | `planning-attempt-*` `:2470-2489`, legacy `:1970-2010` → `open_planning_attempt` `:917-929` | records settings origin + opener focus; closes settings | `close_planning_attempt` `:931-948` restores | **T23 owns; correct — do not touch** |
| A10 | Wizard step Back/Continue/Generate plan/Save draft/Enable | `automation_wizard_back` `:1209-1215`; `automation_wizard_next` `:1105-1207`; `save_automation_draft` `:1380-1432`; `enable_automation` `:1434-...`; footer `:3360-3440` | step/save/enable | in-draft Back keeps values (T09) | **no dirty indicator; leaving the page is the only way out at step 1** |
| A11 | Wizard Cancel | `cancel-automation-wizard` `:3339-3347` | `creating=false`; edit → detail; new → list | immediate | **discards without decision** |
| A12 | Detail → period row | `open-occurrence-*` `:2687-2698` → `open_occurrence_result` `:860-878` | `result_dialog`; page unchanged; marks read | Close `:910-915` → root focus; Escape wipes | **opener focus not restored**; Escape also wipes unrelated overlays |
| A13 | Period → step Open/row | `automation-step-*` `:3067-3072`, row `:3035-3046` → `open_task` | **dialog cleared**, page scope unchanged, step selected | Back uses automation crumb → `open_automation` + `open_occurrence_result` (`src/navigation.rs:482-487`) | **loses results origin**; [test] `automation_qa.rs:5136-5233` currently asserts `Scope::Automations`, i.e. the unexpected destination |
| A14 | Period footer Close / X | `:3175` (X), `:3188` (footer) | closes dialog | root focus | opener not restored |
| A15 | Period “Continue/Retry period” | `:3080-3120` | runtime/period mutation | keeps dialog | T03/T05; preserve |
| A16 | Automation results filter | `:2100-2143` | `result_filter`, `history_limit=20`, **scroll reset** | stays | filter persists in memory; existence fallback at `:2005-2008` |
| A17 | Automation results row | `:2687` | dialog only | Close stays on results | page/scroll preserved except dialog close focus |
| A18 | Automation list/detail “Planning…” | `:1868`, `:2248-2266` | presentation from volatile `planning_automation` | Review → wizard step 3 | **not durable-derived** (see B10) |
| A19 | Archive/lifecycle actions | `archive_automation_ui`, cleanup | status changes | stays | T13/T22/T03; out of T10 |

### 4.3 Creation, planning and small editors

| # | Entry | Code path | Destination today | Exit today | Loss |
| --- | --- | --- | --- | --- | --- |
| C1 | Footer “New research” | `src/views.rs:601-608` | implicit `research_project`; navigate to Project + Board | X/Escape clears overlay only | origin + filters + layout lost; destination not choosable |
| C2 | ⌘N | `src/views.rs:3718-3719` | same as C1; **not guarded by any modal/dirty state** | same | can stack over a dialog/wizard and wipe drafts via navigate |
| C3 | Project toolbar New research | `src/project_views.rs:61-69` | that project (navigate first) | X | previous layout lost |
| C4 | Topic New research | `src/topic_views.rs:532-534` | project + topic (navigate first) | X | topic context lost |
| C5 | Topic Replan | `src/project_app.rs:24-37` | project + source (navigate first) | X | topic context lost; prefilled text cleared on reopen |
| C6 | Planning overlay destination line | `src/project_views.rs:98-108` | static text only | n/a | **no way to change project before planning** |
| C7 | “Save request for later” | `:120-123` → `capture_research_request(start=false)` | persists request, closes overlay, toast | clean | explicit save |
| C8 | “Generate plan” | `:124` → `start_project_plan` → `submit_research_request` → `open_run_progress` | navigates to Runs/planning run | Back structural | **origin not recorded** |
| C9 | Overlay X | `:107` | clears planning fields | focus root | no exact-origin restore |
| C10 | Escape | `src/views.rs:3725-3742` | clears planning fields (not `planning_topic`) | focus root | **no dirty decision; text lost** |
| C11 | Task edit (menu) | `src/views.rs:2396`, `:2567` → `begin_draft` `src/app.rs:947-985` | modal overlay | Cancel `:3550-3560`; Save `save_draft` `:987`; Escape wipe | Escape/navigate lose edits; no decision |
| C12 | Project rename | `src/project_views.rs:13-59` | modal | Cancel/Save | Escape wipes; single-field |
| C13 | Topic name editor | `src/topic_views.rs:122-164` | modal; save navigates to Topic | Cancel/Save | Escape wipes; single-field |
| C14 | Topic move | `src/topic_views.rs:34-120` | modal | Cancel/Confirm | Escape wipes selection; single-purpose |
| C15 | Working folder | `src/app.rs:1283-1300`, overlay `src/views.rs:3379-3442` | modal | Cancel/Save | Escape wipes; single-purpose |
| C16 | New project | `src/views.rs:3795-3877` | modal; creates then navigates | Cancel/Create | Escape wipes typed name |
| C17 | Research page composer (Market research) | `src/research_views.rs:84-99` | page form, submits to Runs | no modal; route switch | page form state is global and not dirty-tracked (out of T10 unless root expands) |

### 4.4 Chrome, keyboard and modal exit

| # | Control | Code | Today |
| --- | --- | --- | --- |
| H1 | Header Back | `src/views.rs:1489-1503` | `go_back` → last crumb; hidden when no crumb |
| H2 | Breadcrumb links | `:1456-1487` | structural targets; explicit navigation |
| H3 | Recently opened | `:1521-1595` | `open_task` per entry; `checked` only for `selected`; “Close current task” → `close_current_task` |
| H4 | Footer Search / ⌘K | `src/views.rs:592-600`, `:615-617` → T08 | opens global search overlay; T08 owns |
| H5 | Footer New research / ⌘N | `:601-608`, `:3718` | see C1/C2 |
| H6 | ⌘[ | `:3704-3717` | guarded by individual overlays but **not** by wizard/planning-attempt; calls `go_back` |
| H7 | Escape | `:3700` (search), `:3720-3724` (planning attempt), `:3725-3742` (global wipe) | inconsistent; no dirty decision; no opener restore |
| H8 | Workspace menu → Settings | `:441-452` | navigate to Settings; clears overlays |
| H9 | Window/appearance | `:1076-1096` | not navigation |
| H10 | Focus | dialogs focus `modal_focus`; closes focus `focus`; T23 restores per-run opener | close-focus contract only met by T23 |

### 4.5 Global search handoff (T08-owned)

| # | Entry / behavior | Code | Today |
| --- | --- | --- | --- |
| S1 | Open from footer/⌘K | `global_search.rs:592-606` | overlay; preserves route/focus/selection; resets domain to All and query to empty |
| S2 | Escape | `:612-620` via overlay capture; `src/views.rs:3700-3703` | closes, restores `previous_focus` |
| S3 | Activate Research hit | `:700-712` | `close_global_search` then `open_task`; **origin not recorded**; `open_task` clears overlays |
| S4 | Activate Automation hit | `:713-721` | `open_automation`; resets list scroll |
| S5 | Activate Period hit | `:722-738` | `open_automation` + `open_occurrence_result` |
| S6 | Dirty guard | `:761-792` | ad-hoc: task editor, planning overlay, five automation fields; blocks with a message |
| S7 | Missing target | `:743-759` | message + refresh; no route change |
| S8 | Keyboard | overlay capture (up/down/enter/escape) | works; background shortcuts suppressed while open |

**T10/T08 boundary:** T10 defines the one dirty computation and one origin mechanism. T08 keeps its overlay/keyboard/footer wiring; it replaces `search_navigation_blocker`'s local predicates with the T10 accessor (one-line message reuse) and does not push origins (search remains an explicit jump for T10; see 7.3 and decision D5). No competing stacks: `global_search.rs` must not add its own history/stack.

## 5. Persistence matrix for target state

| Surface | Must survive Back | Must survive Cancel | Must survive Close | Must survive background events | Notes |
| --- | --- | --- | --- | --- | --- |
| Research list/board | scope, query, all_status, layout, scroll | (Cancel not applicable) | n/a | yes | query is list-local; structural sidebar navigation may still start fresh |
| Topic | topic id, layout, list scroll | (Cancel of dialogs) | n/a | yes | |
| Task detail | selected task, expanded properties, goal tab/steps layout, detail scroll, comment text? | editor only | n/a | yes | comment composer: see U3 |
| Result page | run/task context, result scroll, planning-activity toggle | n/a | n/a | yes | result run selection is chosen by T06/T23 policy |
| Automation list | scroll | n/a | n/a | yes | |
| Automation detail | selected id, scroll | n/a | n/a | yes | |
| Automation settings | open state, scroll, opener | Close returns to detail | opener focus | yes | |
| Period dialog | occurrence id, body scroll, underlying page/filter/scroll, opener | Close returns to underlying page | opener focus | yes | |
| Automation results | filter, history_limit, scroll | n/a | n/a | yes | invalid filter falls back unfiltered |
| Wizard (new/edit/replan) | all fields, step, plan, generated step | Cancel = explicit decision when dirty | n/a (page) | yes (plan may attach; not navigation) | |
| Planning composer | text, destination, mode (new/topic/replan) | Cancel = decision when dirty | n/a | yes | |
| Search overlay | caller route/focus/draft; query only while open | Escape closes | n/a | yes | T08 |

## 6. Defect register (concrete)

Severity follows the program convention: P2 for navigation/presentation that materially affects ordinary use, P3 for bounded edge cases.

**B1 (P2) Planning opens by navigating, so Cancel cannot return to the origin.**
`open_project_plan` runs `self.navigate(Scope::Project(project))` and `self.layout = Layout::Board` before setting `planning_project` [code `src/project_app.rs:12-20`]. `navigate` clears `query`, `selected`, `task_result_context` and every overlay [code `src/app.rs:771-784`]. X/Escape only clear the planning fields [code `src/project_views.rs:107`, `src/views.rs:3728-3735`]. Effect: from filtered All research, New research changes the page and layout; Cancel leaves the user in a project board with the filter gone. Violates contract 3 and T10 “On cancel restore the original route”.

**B2 (P2) Global New research has a hidden and sometimes invalid destination.**
`open_research_intake` uses `research_project` unless the current scope is Project/Topic [code `src/project_app.rs:44-55`]. `research_project` is a process default assigned at load [code `src/app.rs:1150`, field `:258`] and otherwise changed only by the Research page picker/topic move. The overlay prints a static line [code `src/project_views.rs:98-108`]. If the project is gone, `open_project_plan` returns early and the overlay renders nothing [code `src/project_app.rs:9-11`, `src/project_views.rs:94-97`] — a dead click. Violates contract 6 and T10 “show the target project … and allow changing it”.

**B3 (P2) Task-edit and planning drafts are cleared without a decision.**
`clear_navigation_overlays` sets `draft=None`, `editing=false`, `planning_*=None` [code `src/navigation.rs:447-464`]; `navigate` repeats several of them [code `src/app.rs:773-780`]; `open_task` clears them again [code `src/app.rs:845-855`]; Escape clears them unconditionally [code `src/views.rs:3725-3742`]. There is no form-level dirty predicate (`has_unsaved_changes` is store-level [code `src/app.rs:1235`]). Violates contract 6 and “dirty navigation never silently clears the only copy”.

**B4 (P2) Automation wizard is reachable and destructible while it is the only copy.**
`automations()` renders the wizard as page content [code `src/automation_views.rs:1769-1776`], so sidebar/header/⌘[ remain usable. `navigate(Automations)` forces `creating=false` [code `src/app.rs:766-770`]; Cancel discards unconditionally [code `src/automation_views.rs:3339-3347`]; header Back resolves to the list/detail crumb [code `src/views.rs:1227-1261`]. Navigating to a **non-Automations** scope leaves `creating=true`, so the abandoned form can reappear later with stale values. Violates contract 6 and T10 “Settings → Edit → Back … without losing unrelated input”.

**B5 (P2) Two dirty policies will drift / search has a narrower copy.**
T08 currently computes dirtiness in `search_navigation_blocker`/`automation_draft_dirty` [code `src/global_search.rs:761-792`]. It misses schedule fields, timezone/policy values, a generated plan for a fresh form, the planning composer text (it blocks on the overlay being open, not on content), and any suspended state; and it returns a message instead of offering Save/Discard/Stay. Root already flagged the draft-loss risk. Violates single-policy intent and duplicates T10 ownership.

**B6 (P2) Back never restores the filtered research list.**
`go_back` → `navigate(Scope)` [code `src/navigation.rs:474-490`]; `navigate` clears `query` and the search input [code `src/app.rs:776-777`]. `all_status`, `layout`, `show_done`, `compact` persist, so the UI returns to an unfiltered list/section. Violates contract 3 and T10 “same filtered results/scroll”.

**B7 (P2) Scroll positions are not restorable.**
Research list/board has no scroll handle: `collection` [code `src/views.rs:1674-1700`], board horizontal `board-scroll` and per-column `cards-*` [code `:2080-2170`], topic goals list [code `src/topic_views.rs:575`]. Automation uses one handle [code `src/automation_views.rs:301`] applied to list `:2013`, detail `:2224`, results `:2013/2135`, wizard `:3317`, and each entry resets it (`open_automations` `:820-825`, `open_automation` `:825-840`, filter change `:2135-2140`). Period dialog body has no handle [code `:3194`]. Violates contract 3.

**B8 (P2) Period → step loses the page/filter origin and Back lands on the automation detail.**
Step activation calls `open_task` [code `src/automation_views.rs:3067-3072`, `:3035-3046`], which clears `result_dialog` [code `src/navigation.rs:460`] but leaves `scope` at the current page. Back then uses the step's automation crumbs and calls `open_automation` + `open_occurrence_result` [code `src/navigation.rs:482-487`], and `open_automation` forces `scope=Automations` [code `src/automation_views.rs:825-840`]. From a filtered Automation results page, both the page and the filter context are replaced. The current QA asserts the wrong destination: [test `src/automation_qa.rs:5227-5233`] expects `Scope::Automations` after Back from a step opened from results. Violates T10 acceptance and DESIGN “the breadcrumb returns to that period” (breadcrumb is fine; Back is not).

**B9 (P2) Close and Escape do not restore focus to the opener.**
`close_automation_settings` and `close_occurrence_result` focus the root [code `src/automation_views.rs:854-857`, `:910-915`]; the generic Escape wipes all dialogs and focuses root [code `src/views.rs:3725-3742`]. Only the T23 planning-attempt dialog restores an exact opener [code `src/automation_views.rs:931-948`]. Violates contract 3 (“Close … restores focus to its opener”).

**B10 (P2) “Planning…” is not derived from durable state.**
List chip and detail notice are gated by `automation_ui.planning_automation` [code `src/automation_views.rs:1868`, `:2248-2266`]; the Review action only exists there. Durable request/attempt data exists [code `src/request_planning.rs:411-442`] and is not consulted for presentation. After an in-memory reset/restart, a saved draft with a running or unreviewed attempt loses its planning affordance, and “Continue” can start a new attempt instead of reviewing the existing one [code `src/automation_views.rs:1105-1207`]. Violates PRODUCT/T10 “planning-in-progress drafts remain discoverable”.

**B11 (P2) Escape and shortcuts are inconsistent.**
Escape clears `planning_project`/`planning_replan_source` but not `planning_topic` [code `src/views.rs:3728-3734`], bypasses dialog-specific close handlers, and has no dirty decision. ⌘N [code `:3718-3719`] is not guarded by any modal or dirty state, so it can open the planning overlay over a running dialog/wizard and (via today's navigate) wipe drafts. ⌘[ [code `:3704-3717`] is not guarded by the planning-attempt dialog or the wizard. Violates contract 3/6 consistency and T19 keyboard readiness.

**B12 (P3) “Recently opened” cannot restore a report surface.**
Entries always call `open_task` [code `src/views.rs:1521-1595`], and `checked` only compares `selected` `:1576`; choosing the entry of the report currently open drops the user to the task detail. Archived entries are filtered out `:1551`. Contract “selection preserved”; bounded and arguably T12 work.

**B13 (P3) Removed owners produce silent dead ends.**
If an occurrence disappears while a step is open, Back drops to the automation detail (safe) but with no message; if the automation disappears, `open_automation` returns early [code `src/automation_views.rs:825-829`] and `open_period_overview` returns silently [code `src/navigation.rs:407-424`]; if the selected task disappears, `detail` renders “Task not found” while selection/breadcrumb state remains [code `src/views.rs:2430-2432`]. Requirement in T10: graceful invalid/deleted-parent handling; today it is partly graceful but not explained.

**B14 (P2) Planning-in-progress can mutate a draft after the user left.**
`automation_plan_ready` writes `plan`, feedback and `step=3` when `creating` [code `src/automation_views.rs:1278-1334`]. If the user navigated to a non-Automations scope (leaving `creating=true`), the completion mutates the hidden form and may later show a wizard on step 3 with a plan. This is data (acceptable for the same draft) but must never navigate or silently replace dirty text, and must not mark the draft dirty merely because the plan arrived. Coordinate with T02; do not remove the late-completion guards (T09/T23/T25).

## 7. Target policy (normative)

### 7.1 Return context

**RC1. Origin beats breadcrumbs; breadcrumbs never use origin.**
Header Back and ⌘[ return to the recorded origin when one exists and is valid; otherwise they use the current structural parent; otherwise they are hidden/disabled. Breadcrumb clicks always navigate to their structural target and do not consult the origin. This preserves DESIGN’s explicit breadcrumb navigation and T07’s period semantics.

**RC2. Only user drill-down handlers record origins.**
Record in: list/board row → task, “Open N steps” → goal steps, topic goal/report → task/report, step inside goal steps → step, detail View result → result, automation list row → detail, detail → Settings, Settings → Edit/Replan, period row → dialog (modal origin), period step → step, planning submit → planning progress. Do **not** record in: sidebar items, breadcrumbs, footer/⌘ shortcuts, Recently opened, result-actions “Open task/project”, global search activation (T08 decision D5), or any background callback (`finalize_*`, `automation_plan_ready`, timers, recovery, retry). This is the T02 preservation rule.

**RC3. Structural navigation starts the section fresh.**
Calling a sidebar item, breadcrumb, footer target or global search result clears the origin stack and keeps today’s behavior of clearing the list query for a fresh section. Origin restoration is the only path that restores a previous query. (This keeps existing assertions and intent.)

**RC4. Origin is a small bounded stack, not a route controller.**
Depth ≤ 8 with consecutive-duplicate dedupe. Each entry is a typed descriptor with exactly the fields needed (see 7.6), not a whole `Store`/`TaskApp` snapshot. Entries are invalidated on restore if their owners are gone (7.5). T12 will later replace the storage with typed routes without changing semantics.

**RC5. Restore is silent except when an owner disappeared.**
Restoring route/filter/scroll/selection/focus is not a user-visible action. If a target no longer exists, use the fallback and show one neutral notice (e.g. “This period is no longer available. Returned to the automation.”). Never land on a blank page or a generic “All research” page by accident.

**RC6. Close and Cancel are modal-local.**
Close (X/footer/Escape) dismisses only the topmost modal, restores the page exactly (route/filter/scroll/selection) and returns focus to the opener. Cancel (a labeled control on a creation/edit surface) returns to the exact calling page; it never inherits a navigate-first side effect. Clean Cancel is immediate with no prompt.

**RC7. Focus.**
Every modal records `window.focused(cx)` at open (with a keyed `FocusHandle` fallback pattern like T23 where the opener re-renders). Close/Escape restores it if it still resolves; otherwise it focuses the restored page’s primary control or root. Page Back does not need an exact opener (there is no opener element), but it must focus a predictable landmark, never leave focus inside a removed element.

### 7.2 Draft exit policy (contract 6)

**DP1. One dirty computation.**
`dirty_draft()` is the only definition, used by T10 navigation/Exit and by T08 search. It classifies:

| Surface | Dirty when | Baseline |
| --- | --- | --- |
| Task edit | any of title/description/labels/due differs from the stored task | task values at `begin_draft` |
| Automation wizard | the UI form converted to `AutomationDraft` differs from the session baseline, or a plan/planning token was produced for this session and not in the baseline | fresh `AutomationDraft::new` for New; `AutomationDraft::from_definition` for Edit; for Replan, the same with `plan=None` |
| Planning request | composer text differs from its baseline, or the destination/mode differs from the opening destination while text is non-empty | empty for a new request; `topic_replan_input` for replan |

`step` alone, focus movement and background plan arrival are **not** user dirt. A plan accepted for the current baseline updates the baseline (so a later exit does not claim unsaved changes solely because planning finished).

**DP2. Two exit classes.**

- **Restorable/clean**: clean forms close immediately. No confirmation on clean navigation (explicit contract requirement).
- **Decision (Save / Discard / Stay)** for dirty task edit, dirty automation wizard, and dirty planning composer whenever the user attempts to leave or explicitly cancels/escapes. Clean Cancel stays immediate.

**DP3. Decision semantics (one dialog, one mechanism).**

- *Save and continue*: Task → existing `save_draft`; Automation → existing `save_automation_draft` (creates/revises a draft; never enables, never runs); Planning → existing `capture_research_request(start=false)` (“Save request for later”). Validation failures keep the dialog open with the form’s error and do **not** navigate. Save is disabled with a reason when the current content cannot be saved.
- *Discard and continue*: drop the form state and perform the pending exit. The label and tone make the loss explicit (T11 owns final danger styling; do not block on it).
- *Stay*: dismiss the decision, keep the form and its text, restore focus to the first field or the control that initiated the exit.
- Escape while the decision is open = Stay (never discards).
- The decision dialog is modal, focus-trapped, fits 1000×680 (action row wraps; body scrolls), and English-only. No new icon is required; if one is added it must be a registered Lucide asset.

**DP4. Never silently clear the sole copy.**
No path (Back, sidebar, breadcrumb, ⌘[, ⌘N, Escape, search, Close/Cancel) may drop a dirty form without either the Decision or an explicit “Save request for later”/Save flow. Exception, documented and deliberate: single-purpose modal field editors (project rename, topic name, topic move, folder path, new project) keep their existing Cancel/Escape = cancel behavior; they are not research/work drafts, their Cancel is explicit and adjacent, and the stored value remains available. Root may override this exception (D4).

**DP5. Planning-in-progress is data, not navigation.**
Leaving a creation surface while a plan generates is allowed and must not cancel or duplicate the attempt. The saved draft/detail always exposes the derived planning state and a Review action that reopens the **same** attempt (T23 data; T10 presentation). Background completion may attach the plan to the same draft; it must not navigate, must not overwrite dirty objective/expected text, and must not fake a route change.

### 7.3 Origin boundaries (deliberate, testable)

Recorded origins are only those required by contract 3/T10. These are **structural-only** and will not record:

- sidebar rows (they explicitly name a destination in a persistent navigator);
- breadcrumbs (explicit structural parents);
- footer/⌘ shortcuts;
- Recently opened (names the task; B12 remains P3 unless root expands);
- result-actions “Open task/project” (the current QA encodes structural Back; changing it is a separate decision D6);
- global search activation (explicit jump; T08 preserves the calling page until selection, not after).

This boundary keeps the accepted T07/T01 navigation matrices stable and confines behavior change to the T10 acceptance chains.

### 7.4 Creation destination

**CD1. No navigation while composing.**
Opening New research/Plan/Replan opens the overlay in place; destination, layout, filters and scroll stay untouched. Cancel is therefore an exact-origin operation by construction. Submit (“Generate plan”) is an explicit command and may still go to planning progress, but it records the pre-submit origin so Back returns to it.

**CD2. Destination is visible and changeable.**
The overlay always shows the target project as a labeled control (not body text) with the list of user projects; changing it updates the request destination and clears an incompatible topic (switching to “new topic in the selected project”). Replan shows its source project read-only with the reason (“Replan creates a new topic in the original project”). The selector is available before “Save request for later” and “Generate plan”.

**CD3. Default precedence (explicit, no hidden inheritance).**
1) project of the selected task/result context; 2) current `Scope::Project`; 3) current `Scope::Topic`’s project (plus that topic when invoked from the topic’s New research); 4) persisted `last_active_project`; 5) first visible user project. The chosen value is visible at all times and updates `last_active_project`/`research_project` only when the user genuinely picks it.

**CD4. Invalid destination is explained, never silent.**
If no valid project exists, keep the overlay open, disable Generate with a reason and a pointer to create a project; if the chosen project disappears while the overlay is open (archive/cleanup/migration), keep the typed text, show a notice and require a new choice. Never retarget silently and never render an empty overlay.

### 7.5 Removed / invalid owners

Validation and fallback ladder (each step silent unless a notice is needed):

| Origin | Valid when | Fallback |
| --- | --- | --- |
| Research list | scope resolves (Project/Topic exist; cycles are strings; All/Archive/Runs/Recovery always valid) | All research |
| Topic | topic exists (archived topics remain viewable; T22 presentation) | its project, else All research |
| Task | task exists (archived tasks remain viewable; T22) | its topic, else its project, else All research |
| Automation list | always | Automations |
| Automation detail/settings | automation exists (archived still exists) | Automations list |
| Period dialog / step origin | automation and occurrence exist and match | detail, else list; notice if the period is gone |
| Results page | run/task exist per T06/T23 selection policy | results page default selection; notice only if the request/period no longer exists |

Never route to a page that renders “not found” as its terminal state when a parent exists.

### 7.6 Minimum typed origin descriptors (implementation shape)

Not code, but the required data (T12 can replace the storage later):

```
Origin::ResearchList { scope, query, all_status, layout, scroll }
Origin::ResearchTopic { topic, layout, scroll }
Origin::ResearchTask { task, steps_tab, steps_layout, detail_scroll }
Origin::AutomationList { scroll }
Origin::AutomationDetail { automation, scroll }
Origin::AutomationSettings { automation, settings_scroll }
Origin::AutomationPeriod { automation, occurrence,
                            under: Detail | Results { filter, scroll },
                            dialog_scroll, opener }
Origin::ResearchResults { run, task }        // only if D6 authorizes result-origin for "Open task"
```

Rule: descriptors hold filters/scroll/selection/focus only, never domain data or pending commands. Restoration never re-executes anything.

### 7.7 Background and acceptance preservation

- **T02**: origin/decision code is only invoked from user handlers. Add a regression: while a dirty task edit, dirty planning composer, dirty wizard and an open period dialog exist, a scheduled period start plus two step advances and a planning completion leave route/selection/filters/scroll/focus/form text unchanged (no decision dialog appears). Existing T02 evidence must be rerun unchanged.
- **T07**: do not change `task_navigation`, `TaskNeighbor`, `detail_task_neighbors`, `automation_period_steps`, tooltips, or boundary disabling. Back from a period step must still restore the same period (now via origin, with the same visible result). Existing T07 tests (`navigation_qa.rs:268-283`, sidebar suite) must pass unchanged; the only intended difference is the underlying page for periods opened from Automation results (see B8).
- **T09/T23/T25**: do not alter draft defaults, shape-switch invalidation, request revisions, immutable ownership, attempt selection, or opener focus for planning attempts.
- **T11/T22/T13**: no shared-control redesign, no archive semantics, no command/executor refactor.

## 8. Ownership boundaries and proposed files

### 8.1 Explicitly not T10-owned

| Area | Owner | T10 interaction |
| --- | --- | --- |
| `src/global_search.rs`, global search input/overlay, footer Search wiring, ⌘K | T08 (active) | T10 exposes `dirty_draft()` + message; T08 deletes its local `automation_draft_dirty`/predicate logic and uses the accessor. T08 adds no origin stack. One follow-up edit in T08's file, not a competing guard. |
| `open_planning_attempt` / `close_planning_attempt`, `planning_opener_focus`, request/attempt revision data | T23/T25 (frozen) | Reuse for planning history; do not modify. T10's planning discoverability only **reads** `latest_planning_request`/`request_attempts`/`latest_request_attempt`. |
| Lifecycle handlers, save/enable/retry/pause/stop commands, `domain_command.rs` | T13 | Decision “Save” calls existing draft-save entry points only. No new command/executor, no post-commit effects. |
| Archived presentation, restore, archive decision rows | T22 | Origin validation treats archived tasks/topics as viewable history; no new archive logic or CTAs. |
| Button/icon visual system, danger treatment | T11 | Use existing `button`/`primary_button`/`quiet_button`/`tool_icon`; new dialog has no new visual language. Discard label is explicit even before T11 tones it. |
| Navigation header geometry, baseline QA, simplify QA | T01 | T10 changes only Back semantics and handlers; coordinate any assertion updates listed in §12. |
| Typed route controller | T12 | Origin stack is deliberately a small typed stand-in; T12 may absorb it and must keep these semantics. |
| Research execution, eligibility, result presentation | T05/T06 | Untouched. |

### 8.2 Proposed T10-owned files (implementation stage)

New files (no conflict):

1. `src/navigation_context.rs` — pure types + validation + dirty classification helpers; unit tests. No `TaskApp`, no rendering.
2. `src/navigation_context_app.rs` — `impl TaskApp`: capture/restore origin, request navigation/back, pending decision execution, destination helpers. Keeps `app.rs`/`views.rs` diffs small.
3. `src/automation_planning_status.rs` — pure derived planning state for a saved draft (Queued/Running → Planning…; terminal attempt with unreviewed plan → Plan ready); unit tests. Reads request/attempt data, owns no data.
4. `src/navigation_context_qa.rs` — end-to-end QA through real controls at 1440×920 and 1000×680, light/dark where dialogs are shown.

Narrow edits (re-anchor by symbol; sequence after hot sessions settle):

5. `src/app.rs` — one new `navigation_state` field block (origins, decision, baselines, planning destination), one collection scroll handle, small return-value changes for draft save (Save-and-continue), no route logic.
6. `src/navigation.rs` — `go_back` tries origin first; add `apply_origin`/`dismiss_top_modal`; keep `clear_navigation_overlays` behavior for structural navigation; no changes to T07 helpers.
7. `src/project_app.rs` — intake/plan/replan: no navigate-first; destination resolution; planning origin; submit records origin; Cancel helper.
8. `src/project_views.rs` — destination selector, Cancel button, restored-draft/notice states; uses existing controls.
9. `src/views.rs` — header Back/⌘[/⌘N/Escape route through `request_back`/`request_exit`; breadcrumb/sidebar/recent clear origins; render the decision dialog; collection scroll handle. No layout redesign.
10. `src/automation_views.rs` — wizard baseline + dirty wiring, Cancel/Back through the decision, derived planning presentation (list/detail), period origin capture, opener focus + dialog body scroll handle. Only functions named here; coordinate with T05/T06/T23 active edits.
11. `src/main.rs` — register `--qa-navigation-context` module/flag (small; coordinate with T01's dispatch ownership).
12. `src/intake_qa.rs` / `src/affordance_qa.rs` / `src/navigation_qa.rs` / `src/topic_qa.rs` — **additive** assertions or coordinated updates only; see §12. `src/automation_qa.rs` is owned by T05/T06/T23; T10 supplies the exact new expectation for `:5227-5233` and asks the owner to land it (or root assigns the one-line change).

Files explicitly avoided: `src/global_search.rs` (T08), `src/request_planning.rs` (T23/T25), `src/execution_eligibility.rs`, `src/result_presentation.rs` (T05/T06), `src/domain_command.rs`, `src/controls.rs`, `src/appearance.rs`, `.local/opencode-dispatch/baseline-snapshot-*`.

## 9. Incremental implementation plan

Each step compiles and is independently testable. No broad rewrite; T12 remains the later structural extraction.

**S0 — Freeze and coordinate (no product edits).**
Re-read hot files by symbol; record current source hashes; confirm T08/T23/T05/T06 sessions are not mid-write in the files S1–S5 touch; confirm T01 accepts the assertion changes in §12; confirm decisions D1–D6 with root. Record the plan's file ownership in IMPLEMENTATION.md by root (not T10). No app restart, no workspace access.

**S1 — Pure navigation context (S/M).**
Add `navigation_context.rs` (types, stack, validation ladder, dirty classification inputs, destination precedence) + `automation_planning_status.rs` + unit tests: stack push/pop/dedupe/cap; validation fallbacks for deleted task/topic/project/automation/occurrence; invalid filter; derived planning states; destination precedence incl. missing default. No UI wiring yet. This de-risks the policy before touching hot files.

**S2 — Creation destination and exact-origin Cancel (M).**
Remove navigate-first from `open_project_plan`; add `planning_destination` + origin capture; make `open_research_intake` resolve via CD3 and always render a valid destination; add the project selector + Cancel button + no-project state; replan project read-only with reason; keep `planning_topic` semantics for topic New research. On Generate plan record the origin so Back from progress returns. Tests: intake/affordance additions + new QA cases 1–4 (below). Verify existing `intake_qa`/`affordance_qa`/`simplify_qa` planning assertions still pass.

**S3 — Planning discoverability (S).**
Use `automation_planning_status` in automation list chip and detail notice/Review action; keep the in-memory token as an additional live hint, never the only one; ensure `review_automation_planning` still binds the same attempt (existing). Test: leave a draft mid-planning, navigate away, return; simulate restart by rebuilding the app from the store and assert the derived state/Review path; assert completion does not navigate.

**S4 — Origin stack and period/page restoration (M/L).**
Wire capture in the RC2 list; restore in `go_back`; add the collection scroll handle and period-body scroll; capture opener focus for period/settings; restore filters/selection/scroll. Preserve structural navigation clearing. Tests: required chain (results→period→step→Back→period→Close→filtered results/scroll), list→task→Back, goal steps→step→Back→goal steps, automation list→detail→Back with scroll. Update `automation_qa.rs:5227-5233` through its owner.

**S5 — Dirty computation, decision dialog and wizard/task/planning wiring (M/L).**
Add baselines + `dirty_draft()`; intercept Back/⌘[/⌘N/Escape/Cancel/sidebar/breadcrumb/recent for the three surfaces; implement decisions with existing save entry points; remove silent clears for those surfaces; delete the T08 duplicate predicates (T08 owner, one follow-up); keep clean navigation prompt-free. Tests: all three dirty surfaces × Save/Discard/Stay, Escape=Stay, invalid save stays, clean Cancel immediate, “only copy” invariant, plus T02 background no-op regression.

**S6 — Reconciliation and evidence (S).**
T08 adopts `dirty_draft()`; T01/T07/T05 owners reconcile the listed assertions; run the targeted suites and the new QA in both windows/themes; capture screenshots and logs; update the policy text in the maintained docs **through T17 ownership** (do not add contradictory prose): one definition of Back vs breadcrumbs, the two exit classes, destination rules, and the planning-discoverability rule. Record commands/evidence and the proposed IMPLEMENTATION.md note for root. No commit/push.

## 10. Test plan (meaningful, through actual controls)

### 10.1 Unit tests (new modules)

- Origin stack: push/pop order, dedupe, depth cap, clear-on-structural, no mutation from background-shaped calls (call the restoration API only; the “background” test asserts the stack is untouched by completion helpers).
- Validation ladder for each origin variant (present/archived/deleted/topic-project mismatch/occurrence-automation mismatch/filter deleted).
- Dirty classification: task fields individually; automation semantic fields (kind, unit, weekdays, catch-up, limits, timezone, shape, plan, name/objective/expected) and “clean step change”; replan baseline excludes the cleared plan; planning text baseline empty vs prefilled; background plan arrival does not dirty.
- Destination precedence and invalid/missing project.
- Derived planning status: queued/running/failed/unreviewed-success/current-reviewed, legacy request without owner.

### 10.2 New QA suite `--qa-navigation-context`

Fixture: two user projects, one topic, one completed 3-step goal with a final report, one in-progress goal with a period, two automations with two periods each (one filtered, one not), a saved automation draft with an in-flight planning attempt, and one archived task. Use `HeadlessAppContext` + `TestWindowExt`; run the whole scenario at **1440×920 and 1000×680**, light and dark for dialog screenshots. Scroll to controls; never enlarge the window.

| Case | Actual controls | Assertions |
| --- | --- | --- |
| N1 | All research: type a query, set a status filter, scroll the list, click a visible row, View final report, Back, Back | Back 1 = same task detail; Back 2 = All research with the exact query text, status filter and list offset; visible ids match the pre-open set |
| N2 | Project board: scroll columns, open a card, Back | Board layout and horizontal/column offset restored |
| N3 | Topic: open goal steps, open a step, Back, Back | Steps tab and steps layout preserved; second Back restores the topic and its scroll |
| N4 | Automation results: set filter A, scroll, open the newest period, open a step, Back | Period dialog id restored, underlying scope is Automation results, filter A still active, list offset unchanged; Close leaves the same filtered results; focus returns to the period opener |
| N5 | Automation detail: scroll, open period, Close/Escape | Detail scroll unchanged; focus returns to the period row; unrelated overlays (e.g. a background-opened state) unaffected |
| N6 | Automation detail → Settings → Edit → Back (clean) | Settings dialog reopens on the same automation, settings offset preserved, opener focus restored; [test anchor `automation_qa.rs:5036-5053`] |
| N7 | Same, but type in the objective; Header Back | Decision dialog appears: Save draft / Discard / Stay; Stay keeps text and wizard; Save creates/revises a draft, does not enable, and then restores Settings; Discard discards and restores Settings |
| N8 | New automation with typed instruction; sidebar All research; then All automations; then New automation | Dirty exit decision on the sidebar click; Discard leaves no stale wizard; Save persists a Draft; no stale form reappears on revisit |
| N9 | New automation, no typing; Back; New automation | Clean: no dialog, immediate list, fresh defaults (T09 preserved) |
| N10 | Footer New research from All research (query + scroll set); confirm destination; Cancel | Overlay opened without route change; destination visible; Cancel returns to the exact query/scroll/layout; no project switch |
| N11 | Change the destination project in the overlay, then Generate plan, then Back | Request belongs to the chosen project; Back returns to the pre-planning origin with filters/scroll; no implicit project switch |
| N12 | Topic New research; Cancel; Topic Replan; Cancel | Both return to the same topic (id) with scroll; replan text prefilled and not cleared on cancel until an explicit decision |
| N13 | Global New research with the default project archived/removed (fixture edit) | Overlay stays open with a valid selectable destination or a disabled Generate + reason; never a dead click |
| N14 | Planning composer with text; Escape; reopen New research | Decision (Save request / Discard / Stay); Stay keeps text; Escape on the decision = Stay; Save persists exactly one request; Discard clears |
| N15 | Task editor with changed title; Escape / ⌘N / sidebar (via programmatic key dispatch) | Decision semantics; Save persists and continues; Discard reverts display to stored title; Stay keeps text; no silent clear |
| N16 | Period/step with a deleted occurrence and a deleted automation (fixture mutation) | Back falls back detail→list with a notice; no blank page; no foreign period |
| N17 | T02 regression: dirty editor + dirty composer + period dialog open; simulate scheduled start and two advances + planning completion | Route, selection, filters, scroll, focus and text unchanged; no decision dialog; explicit “View progress” still opens the correct period |
| N18 | T07 regression: first/middle/last step Previous/Next, Overview, Back | Boundaries disabled; Overview opens the same period; Back restores the same period; no cross-period movement |
| N19 | T08 boundary: open search over each dirty surface, Escape | Caller route/focus/draft unchanged; activation over a dirty form shows the shared blocker message (T08-owned assertion) |
| N20 | 1000×680 dialogs: destination selector, decision dialog | All buttons visible without enlarging; body scroll reaches controls; labels not clipped; screenshots light/dark |

Each case records Passed/Failed/Blocked/Not run with expected and actual destination, matching T20's ledger style.

### 10.3 Commands and evidence

```bash
export PATH="/opt/homebrew/opt/node@24/bin:$PATH"
cargo test --quiet
cargo build --features qa
target/debug/what-the-hell-are-you-planning --qa-navigation-context
target/debug/what-the-hell-are-you-planning --qa-intake
target/debug/what-the-hell-are-you-planning --qa-affordance
target/debug/what-the-hell-are-you-planning --qa-sidebar
target/debug/what-the-hell-are-you-planning --qa-automation
cargo run --features qa --quiet -- --qa-navigation-context
python3 script/check_icons.py   # only if any icon is added
```

Run sequentially, isolated PID-scoped fixtures, no live workspace, no restart of the user's app. Capture exit codes, logs and screenshots under the task evidence folder; record source/binary hashes as in prior batches.

## 11. Acceptance mapping (TASKS T10)

| Acceptance clause | Covered by |
| --- | --- |
| filtered results → period → step → Back restores same period and ultimately same filtered results/scroll | S4, N4, N5; assertion reconciliation §12 |
| Settings → Edit → Back restores Settings without losing unrelated input | S5, N6, N7 (origin + decision); existing `automation_qa.rs:5036-5053` |
| Clean Cancel immediate | DP2, S2/S5, N9, N10 |
| Dirty navigation never silently clears the only copy | DP3/DP4, S5, N7/N8/N14/N15 |
| Planning-in-progress drafts remain discoverable | S3 + existing T23 data, N8/N13 planning case |
| Return context survives invalid/deleted parent gracefully | S1 ladder, N16 |
| Return context is not confused with breadcrumbs | RC1/RC3, N1/N2/N3 (breadcrumb clicks stay structural) |
| Target project shown and changeable before planning; Cancel exact origin | S2, N10/N11/N12/N13 |
| T02 preserved | S5 regression, N17 |
| T07 period neighbors preserved | N18 |

## 12. Existing assertions that encode current behavior (reconcile before S4/S5)

| Location | Current expectation | Impact | Owner |
| --- | --- | --- | --- |
| `src/automation_qa.rs:5227-5233` | Back from a step opened in Automation results ⇒ `Scope::Automations` | **Must change** to Automation results + restored period/filter (T10 acceptance) | T05/T06/T23-owned file; T10 supplies the exact expectation, owner lands it |
| `src/automation_qa.rs:5036-5053` | Back from Edit ⇒ detail + Settings open | Keep; becomes origin-driven | T23/T05 file; assert unchanged |
| `src/navigation_qa.rs:268-283` | Back from step ⇒ same period over the automation | Keep (structural fallback path, no origin recorded in that helper-driven test) | T07 |
| `src/simplify_qa.rs:1026-1031` | Sequential Back step→goal→topic→project via “Open task” menu | Keep under D6 (result-actions stays structural); if D6 reverses, update | T01 |
| `src/simplify_qa.rs:1129-1146` | Back from task ⇒ Topic ⇒ Project ⇒ All and `query.is_empty()` | Keep (programmatic `open_task` records no origin; structural navigation still clears query) | T01 |
| `src/topic_qa.rs:160-165` | Step → Back ⇒ goal steps list | Keep (origin = goal detail, same visible page) | T07/T01 |
| `src/material_qa.rs:416` | Back from report | Keep (origin task) | T01 |
| `src/intake_qa.rs:87-104` | `open_research_intake` from a topic yields `planning_project=Some(1)`, `planning_topic=None` | Keep; now without route change | T01 |
| `src/affordance_qa.rs:129-148` | Close planning clears `planning_project`; reopen from project works | Keep; Cancel additionally restores origin | T01 |
| `src/global_search.rs:761-792` | Local dirty predicates | Replace with T10 accessor (behavior-compatible message) | T08 |

## 13. Open decisions for root (resolve before S2/S4/S5)

- **D1 Back semantics for list → task.** Confirm origin-first Back for real list/board rows (returns to the filtered list) while sidebar/breadcrumbs/Recently opened stay structural. This is the intended contract-3 change and the main behavioral delta.
- **D2 Planning composer exit.** Confirm Decision (Save request / Discard / Stay) for dirty Escape/Cancel rather than hiding a restorable draft. Recommendation: Decision (one mechanism, no hidden state, no restart ambiguity).
- **D3 Automation “Save and continue”.** Confirm a decision Save may create/revise a Draft definition as a side effect of leaving (explicitly labeled “Save draft and continue”, never enables or runs). Recommendation: yes.
- **D4 Single-field modal exception.** Confirm project rename/topic name/move/folder/new-project keep Cancel/Escape = discard (not part of the draft Decision).
- **D5 Search and origin.** Confirm global search results remain explicit jumps (no origin record) for this iteration; only the dirty blocker is shared with T08. T12/T08 may revisit.
- **D6 Result-actions “Open task/project”.** Confirm it stays structural (keeps `simplify_qa` and current result navigation) rather than recording a result-page origin. Recommendation: keep structural; record as a T12 follow-up.
- **D7 Recently opened report entry (B12).** Confirm defer to T12 or include a small tab-surface memory in T10. Recommendation: defer.
- **D8 Planning submit destination.** Confirm “Generate plan” may continue to open planning progress (explicit label), with origin recorded so Back returns. Recommendation: yes.

## 14. Known gaps / uncertainty (explicit)

- The tree is changing concurrently; symbol anchors are reliable, line numbers are not. S0 must re-read and record hashes.
- `gpui` focus-handle behavior for a re-rendered opener is assumed to work when captured with `window.focused(cx)`; if it proves stale in tests, use T23's keyed `FocusHandle` map pattern (already proven in this codebase). This affects only focus restoration, not routing.
- Board scroll is composed of a horizontal outer scroll plus per-column vertical scrolls; restoring “the board scroll” may require multiple offsets. The audit records “restore the visible viewport” as the acceptance intent; the exact handle set is an implementation detail to settle in S4 with tests. If a single restore is insufficient, record `(board_x, column, column_y)` and restore the visible card's column offset.
- Automation list/detail/results share one scroll handle today. S4 may either capture/restore offsets on the single handle or split it into three; tests decide. Splitting is preferred if a single handle cannot restore after Back without visual jump.
- The planning-in-progress “restart” case depends on how the recovery poll resumes/closes planning runs; S3 tests should first observe the real post-restart run status and only then assert the derived label. No new scheduler/recovery behavior is implied.
- Escape/⌘N dispatch in the headless QA harness uses the existing `press` helpers; if `press` cannot synthesize platform-⌘ combos in a specific suite, use the concrete control (Cancel button / footer New research) for behavior coverage and keep one direct unit test for the guard. Do not skip the coverage.
- The comment composer is not treated as a draft in T10 (see U3 below); if root considers a typed comment a “sole draft”, add it to DP1 as a per-task preserved value (cheap) rather than blocking navigation.

Unknown-but-not-blocking (U):

- **U1** Whether any non-test code still calls `navigate` during a background completion. The audit found only user handlers in the paths read; S5 must add the T02 regression and grep the completion paths again after concurrent merges.
- **U2** Whether `end`-of-planning bootstrap creates a definition before the wizard (it does for planning); the S3 “restart” test will confirm the derived state against real data.
- **U3** Comment composer text is cleared by `open_task` (`src/app.rs` comment reset in `open_task`). Not in the acceptance text; listed as a residual risk for T19/T12, with an optional one-line per-task preserve in S5 if root wants it.
- **U4** Research page (Market research) inline composer (`src/research_views.rs:84-99`) is a page form, not a modal; it shares the global inputs and is out of the T10 draft Decision unless root expands scope. It is included in the inventory (C17) so it is not silently ignored.

## 15. Constraints confirmation

- No product/source/test/docs edits were made; only this audit file was created.
- No commit, no push, no branch/rebase, no real workspace access, no app restart, no frozen baseline snapshot edits, no QA execution that could touch the user's environment.
- No VoiceOver/screen-reader requirement is introduced. Keyboard/focus and visual checks remain in T10.
- Both 1440×920 and 1000×680 are explicit test targets; no enlarged-window workarounds are permitted (scroll to controls, wrap action rows, assert bounds).
- T10 does not reduce scope: origin/dirty/destination/planning-discoverability and the T02/T07 preservation regressions are all included in S1–S6.

## 16. Handoff summary for root

- Approve D1–D8; the two that change user-visible behavior most are D1 (origin Back) and D2/D3 (dirty Decision semantics).
- Authorize S0–S1 immediately (new pure files, no hot-file edits), then S2 (project_app/project_views only), then coordinate S3–S5 with the owners of `automation_views.rs`, `app.rs`, `views.rs`, `automation_qa.rs` and `global_search.rs`.
- T10 will not touch T08's overlay/stack, T23's planning history/focus, T13's commands, T22's archive presentation or T11's controls.
- The single mechanism T08 consumes is `dirty_draft()` + one message; T10 owns the origin stack and decision dialog, and T08 must delete its local predicates rather than extend them.
