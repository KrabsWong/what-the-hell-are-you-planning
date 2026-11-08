# T12 S1 handoff — pure typed navigation core

Status: **S1 implemented and independently unit-tested; no live migration.** The new module is registered but not called by the application. T12 remains open; this is not T12 acceptance.

Scope authorized: pure module + tests + minimal `main.rs` registration. No route/overlay/editor migration, no `TaskApp` wiring, no T10/T13/T21/T08 changes, no commit/push, no production data or app restart.

Date: 2026-11-08. Model: `opencode-go/deepseek-v4.1-flash`.

## 1. Changed files

| File | Change |
| --- | --- |
| `src/navigation_route.rs` | **New.** Pure T12 core: route identity, typed overlays, legal stack grammar, bounded return frames, ownership validation, save/impact/durability outcomes, tests. |
| `src/main.rs` | One module registration line: `mod navigation_route;` (next to the existing navigation modules). |

No other source or maintained-document file was changed. The temporary test-recursion `cfg_attr` added while diagnosing the unrelated T21 macro issue has been **removed**; `main.rs` contains no `recursion_limit` override. The module carries `#![allow(dead_code)]` only because S1 is not wired yet; remove it in the first adapter slice.

## 2. Exact API

### 2.1 Route identity

```rust
pub(crate) enum DataScope {
    All, Project(ProjectId), Topic(TopicId), Cycle(String), Archive,
    Results, Paused, MarketResearch, Automations, AutomationResults, Settings,
}

pub(crate) enum ResearchScope { All, Project(u64), Topic(u64), Cycle(String), Archive }

pub(crate) enum TaskPanel { Overview, Steps }
pub(crate) enum SettingsSection { General, Models, Publishing }

pub(crate) enum Route {
    ResearchList(ResearchListPage),
    ResearchTask(ResearchTaskPage),
    ResearchResult(ResearchResultPage),
    PausedProjects,
    MarketResearch,
    Settings(SettingsPage),
    AutomationList,
    AutomationResults(AutomationResultsPage),
    AutomationDetail(AutomationDetailPage),
    AutomationEditor(AutomationEditorPage),
}
```

Page fields are private. Constructors/accessors are the only public surface:

```rust
Route::research_list(ResearchScope, AllStatus, Layout)
Route::research_task(TaskId)
Route::research_task_steps(TaskId, Layout)
Route::research_result(ResultOwner, Option<RunId>)
Route::automation_detail(AutomationId)
Route::automation_editor(Option<AutomationId>)
Route::automation_results(Option<AutomationId>, usize)
Route::data_scope(&Store) -> DataScope
Route::task() -> Option<TaskId>
```

`ResearchList` deliberately has **no `Scope::Runs` alias**: ordinary results are `ResearchResult`; Market research and Paused are their own routes.

### 2.2 Result ownership

```rust
pub(crate) enum ResultOwner {
    OrdinaryPage, // ordinary, non-automation results only
    Task(TaskId), // ordinary task; rejects an automation-owned task
    AutomationTask { automation, occurrence, task },
    PlanningProgress { request }, // ordinary request only; automation planning stays T23
}

pub(crate) struct ResultSelection { run: Option<RunId> }
```

Validation:
- `OrdinaryPage`: a selected run must exist and `!Store::run_is_automation_owned`.
- `Task(task)`: task exists, is not automation-owned, and a selected run belongs to that task and is not automation-owned.
- `AutomationTask`: automation exists; occurrence exists and belongs to it; task exists and is owned by the automation; `automation_period_for_task(task) == (automation, occurrence)`; a selected run must have the same task, `automation_occurrence_id == occurrence`, the same period identity, and be automation-owned.
- `PlanningProgress`: the request exists and is an ordinary request (no `automation_id`); a selected run must be the request's planning attempt (`request_for_run(run) == request`) and not automation-owned.

This is the T12 ordinary-page ownership consolidation. **T23's immutable planning-attempt ownership is unchanged**; automation planning attempts are rejected from ordinary result pages rather than reinterpreted.

### 2.3 Overlays and editor targets

```rust
pub(crate) enum EditorTarget {
    Task(TaskId),
    Automation { editing: Option<AutomationId> },
    PlanningRequest,
}

pub(crate) enum Overlay {
    Search,
    PlanningComposer,
    TaskEditor { task },
    AutomationSettings { automation },
    AutomationPeriod { automation, occurrence },
    PlanningAttempt { run, from_settings },
    ResearchImpactReview { task },       // T21 child
    PreviousVersions { task, run },      // T21 view-only root
    ProjectRename { project },
    TopicEditor { project, topic },
    TopicMove { topic },
    FolderDialog { project },
    NewProject,
    DraftDecision { editor },
}

pub(crate) enum OverlayClass { Editor, Content, Impact, Decision, Search }
```

`AutomationPeriod` is a **typed overlay over an explicit base**:

```rust
pub(crate) enum PeriodBase {
    Detail { automation },
    Results { filter: Option<AutomationId>, history_limit: usize },
}

Destination::Period { base, automation, occurrence }
```

The base route is set to `AutomationDetail` or `AutomationResults`; the period is pushed as an overlay. A deleted occurrence/automation is rejected atomically; there is no hidden `scope`/`selected_run` reconstruction.

### 2.4 Destinations, commands and effects

```rust
pub(crate) enum EnterMode { Structural, DrillDown, Replace }

pub(crate) enum TaskSurface { Detail, Result { run: Option<RunId> } }
pub(crate) enum RecentSurface { Task, Result }

pub(crate) enum Destination {
    Route(Route),
    Overlay(Overlay),
    TaskOutcome { task, surface: TaskSurface },
    Period { base, automation, occurrence },
    Recent { task, surface: RecentSurface },
}

pub(crate) enum NavCommand { Enter { dest, mode }, Back, CloseTop }

pub(crate) enum NavEffect {
    Entered { captured_return: bool },
    Back { restored: Restored },
    Closed { overlay: Overlay },
}

pub(crate) struct Restored { pub frame: ReturnFrame, pub notice: Option<String> }
```

### 2.5 Return context and scroll

```rust
pub(crate) struct ReturnFrame {
    pub route: Route,
    pub overlays: Vec<Overlay>,
    pub scroll: ScrollBookmark,
    pub result_fallback: Option<ResultFallback>, // captured only; never live route state
}

pub(crate) struct ReturnStack { frames: Vec<ReturnFrame> } // max 8, consecutive dedupe

pub(crate) struct ScrollBookmark { entries: BTreeMap<ScrollKey, (f32, f32)> }
pub(crate) enum ScrollKey { TaskList, BoardX, BoardColumn(u8), Topic, PeriodBody, Settings, Result, Dialog }
```

`ScrollBookmark` stores offsets verbatim, including `(0.0, 0.0)`. It does not clamp or drop zero. The adapter maps `ScrollKey` to real GPUI handles; no handle enters the pure core.

### 2.6 State and mutators

```rust
pub(crate) struct NavState {
    route: Route,
    overlays: Vec<Overlay>,
    returns: ReturnStack,
    recent_surfaces: BTreeMap<TaskId, RecentSurface>,
}

impl NavState {
    pub(crate) fn new(store: &Store, route: Route) -> Result<Self, NavReject>;
    pub(crate) fn route(&self) -> &Route;
    pub(crate) fn overlays(&self) -> &[Overlay];
    pub(crate) fn top_overlay(&self) -> Option<Overlay>;
    pub(crate) fn returns(&self) -> &ReturnStack;
    pub(crate) fn data_scope(&self, store: &Store) -> DataScope;
    pub(crate) fn task(&self) -> Option<TaskId>;
    pub(crate) fn remember_recent_surface(&mut self, task, surface);
    pub(crate) fn recent_surface(&self, task) -> RecentSurface;

    pub(crate) fn enter(&mut self, store, dest, mode, scroll) -> Result<NavEffect, NavReject>;
    pub(crate) fn push_overlay(&mut self, store, overlay) -> Result<(), NavReject>;
    pub(crate) fn close_top(&mut self, store) -> Result<NavEffect, NavReject>;
    pub(crate) fn back_availability(&self, store) -> BackAvailability;
    pub(crate) fn back(&mut self, store) -> Result<NavEffect, NavReject>;
    pub(crate) fn apply_confirmation(&mut self, confirmation, command, store, scroll) -> ConfirmationStep;

    pub(crate) fn set_research_filters(&mut self, all_status, layout) -> Result<(), NavReject>;
    pub(crate) fn set_task_panel(&mut self, panel) -> Result<(), NavReject>;
    pub(crate) fn select_run(&mut self, store, run) -> Result<(), NavReject>;
    pub(crate) fn set_results_filter(&mut self, store, filter, history_limit) -> Result<(), NavReject>;
    pub(crate) fn set_settings_tab(&mut self, tab) -> Result<(), NavReject>;
}
```

`set_research_query` is intentionally **not present**. Query remains an adapter page payload until the S5 page-state flip; S1 does not expose a successful no-op.

### 2.7 Save / impact / durability

```rust
pub(crate) enum PendingKind {
    ImpactReview { task: TaskId },
    Durability,
}

pub(crate) enum Confirmation {
    Confirmed,
    Pending(PendingKind),
    Rejected(String),
}

pub(crate) enum ConfirmationStep {
    Advanced(NavEffect),
    Pending(PendingKind),
    Rejected(String),
}
```

- `PendingDurability` is explicitly not Saved; the route, overlays and returns stay unchanged.
- `PendingImpact` pushes `ResearchImpactReview { task }` above the editor/decision and returns `Pending`; no route/return change.
- `Rejected` leaves the whole state unchanged.
- `Confirmed` removes exactly the completed editor/decision/impact flow, then executes the command. A Back command restores the unrelated parent from the return stack.

## 3. Invariants enforced

1. **Checked construction.** `NavState::new` validates the route against `&Store`; production cannot start from a missing task/project/topic/automation/occurrence or an invalid result owner.
2. **Atomic rejection.** Every mutator clones, validates and commits only on success. A rejected `enter`, `push_overlay`, `select_run`, `set_results_filter`, confirmation or Back leaves `NavState` byte-for-byte equal to its prior value; tests assert state equality.
3. **No mirrored route fields.** The only state is route + overlay stack + return stack + recent-surface hint. There is no second `scope`, `selected`, `task_result_context`, `selected_run`, `creating`, `editing` or dialog boolean.
4. **One route identity.** A route is a page; a period is an overlay over an explicit Detail/Results base; a wizard is `AutomationEditor`; ordinary results are `ResearchResult`.
5. **One result owner.** The page's `ResultOwner` and effective `ResultSelection` are validated together. Task A + report B, ordinary page + automation run, wrong period run, and deleted owner are rejected without mutation.
6. **Overlay grammar.** At most `MAX_OVERLAY_DEPTH = 6`; at most one Search/root/Decision/Impact. The grammar admits at most four stacked overlays today; capacity is not 3.
7. **Only the top overlay is active.** `top_overlay()` is the input/focus target; all lower overlays are inert until the top closes.
8. **Legal nesting.** A root must be the first non-Search overlay. Search may be inserted anywhere; it never precedes a root. A Decision must target its underlying editor; an Impact must match the same task as the editor/Decision and sits above the Decision. A view-only root (Settings, Period, Attempt, PreviousVersions, small editors) cannot carry Decision/Impact. PreviousVersions cannot coexist with an editor/other root.
9. **Structural clears.** `EnterMode::Structural` clears all returns and overlays inside the controller; a caller cannot forget.
10. **DrillDown captures / Replace does not.** DrillDown records exactly one `ReturnFrame` from typed state; Replace keeps returns/overlays. Back pops one frame; empty returns use the structural parent; no frame and no parent is `NoDestination`.
11. **Back is blocked by an open overlay.** The page Back/⌘[ cannot navigate behind a modal; `BackAvailability::BlockedByOverlay`.
12. **Close is top-only.** `close_top` pops exactly one overlay and leaves the route/returns untouched. The T21 impact/previous-version children close to the preceding editor/decision; Search closes to its caller.
13. **Completed flow close.** A confirmed `CloseTop` removes exactly the completed editor/decision/impact flow and returns `Advanced(Closed { topmost_removed })`; it does not hit an empty-stack error, does not pop a parent, and preserves an unrelated nested Search. It is the save/discard-confirmed Escape/close path.
14. **Pending never advances.** `PendingDurability`/`PendingImpact`/`Rejected` never change the route or returns. Only `Confirmed` executes the command.
15. **No runtime/focus types.** The module imports only `AllStatus`, `Layout`, `Store` and `BTreeMap`; no `TaskApp`, `Window`, `FocusHandle`, `InputState`, `Storage`, timer or command engine.

## 4. Overlay grammar (exhaustive representative test)

For a `ResearchTask` base, the exhaustive test over five representative layers (`TaskEditor`, `DraftDecision(Task)`, `ResearchImpactReview`, `Search`, `PreviousVersions`) and all permutations up to length 4 asserts exactly these legal sequences:

```
[]
[search]
[editor]
[editor, search]
[previous]
[previous, search]
[editor, decision]
[editor, search, decision]
[editor, decision, search]
[editor, impact]
[editor, search, impact]
[editor, impact, search]
[editor, decision, impact]
[editor, search, decision, impact]
[editor, decision, search, impact]
[editor, decision, impact, search]
```

For an `AutomationEditor` base, a `DraftDecision(Automation)` may be the only non-Search overlay (Search may sit before or after it). All other permutations are rejected, including impact without an editor, mismatched impact task, Search before a root, PreviousVersions plus editor, two roots, Decision after Impact, and more than the capacity.

## 5. Tests (28, all pass)

`cargo test --quiet navigation_route` -> **28 passed, 0 failed**.

Coverage:
1. `structural_enter_clears_returns_and_overlays`
2. `back_restores_route_and_nonzero_scroll`
3. `back_restores_explicit_zero_scroll_bookmark`
4. `deleted_owner_falls_back_instead_of_restoring_a_not_found_page`
5. `task_a_report_b_mismatch_is_rejected_atomically`
6. `ordinary_results_never_admit_automation_runs`
7. `automation_task_result_requires_the_named_period_identity`
8. `same_period_step_return_restores_period_over_results`
9. `close_top_dismisses_only_the_top_overlay`
10. `pending_confirmation_never_advances_until_confirmed`
11. `confirmed_close_completes_the_editor_flow_and_preserves_nested_search`
12. `confirmed_close_without_unrelated_overlays_leaves_the_page`
13. `rejected_confirmation_keeps_the_flow_open_and_route_unchanged`
14. `checked_construction_rejects_a_missing_owner`
15. `impact_review_must_match_the_editor_task`
16. `previous_versions_owner_must_match_the_base_page`
17. `results_filter_mutation_validates_owner_and_limit_atomically`
18. `confirmed_back_restores_an_unrelated_parent_from_settings_edit`
19. `failed_confirmed_commit_preserves_the_completed_flow`
20. `nested_search_and_impact_return_to_the_preceding_surface_on_close`
21. `overlay_grammar_allows_the_full_editor_decision_impact_search_flow`
22. `overlay_grammar_is_exhaustive_over_representative_layers`
23. `data_scope_has_no_runs_alias_inside_research_list`
24. `result_page_rejects_deleted_owner_without_mutation`
25. `planning_progress_requires_the_named_request`
26. `back_is_blocked_while_a_modal_is_active`
27. `return_stack_is_bounded_and_deduplicates_consecutive_frames`
28. `visible_projects_are_not_required_for_navigation_validation`

Normal transition tests use the checked `NavState::new(&store, route).unwrap()`. The unchecked test helper has been removed; only the checked constructor exists.

## 6. Verification evidence (this candidate)

| Command | Result |
| --- | --- |
| `cargo check --message-format short` | exit 0, no errors; no `navigation_route` warnings |
| `cargo test --quiet navigation_route` | 28 passed, 0 failed |
| `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet` | 521 passed, 0 failed, 2 ignored (latest run after the final grammar test; concurrent owners added tests, so the total is 523. Root's earlier independent durable evidence records 518 passed, 0 failed, 2 ignored) |
| `cargo build --quiet --features qa` | exit 0 |
| `target/debug/what-the-hell-are-you-planning --qa-sidebar` | passed: “Sidebar startup QA passed”; “Sidebar navigation QA passed” |

Attempted but **not** a pass:
- `--qa-global-search` failed at `global_search_qa.rs:934`, expecting a search-activation blocker message containing “task edit”. This is the T10/T21 dirty-exit integration expectation, not S1 code (S1 is not wired into search or UI). T08/T10 own the update.

The root-independent records (`root-post-macro-fix-unit-r3.log`, `integration-independent-518-unit-pass.log`) cover the pure S1 fixes. The later 521-test run also includes tests added by concurrent owners plus the final rejection/grammar tests; no failing assertion changed.

## 7. Remaining integration gaps (not S1 acceptance)

| Gap | Owner | Effect |
| --- | --- | --- |
| No adapter wiring | T12 S2+ | `navigation_route` is not referenced by `TaskApp`; `#![allow(dead_code)]` is temporary. |
| T10 S6 not accepted | T10 | No live command gate/decision UI migration; T12 S2 is gated on T10 S6 plus durable save. |
| Pending save/impact mapping | T21/T13/T10 | `PendingKind`/`Confirmation` are modeled; the adapter must map T21 `PendingResearchSave` and T13 durability, never `Saved` early. |
| Ordinary result selection adapter | T12/T06/T07 | S1 validates ownership; the adapter must fill `ResultSelection` from existing T06/T07 policies. T23 planning ownership is untouched. |
| Focus/opener/scroll adapter | T12 S2 | S1 stores no handles; the app glue must map `ScrollKey`/opener focus and restore top-only. |
| Search QA stale message | T08/T10 | `--qa-global-search` expectation must be reconciled; not S1. |
| Native/headless T12 evidence | T12/T20 | None claimed; S1 is pure. |
| Period page-vs-overlay / plan-attach decisions | root/T10 | S1 encodes period as an overlay; no live step/plan behavior changed. |

## 8. Handoff / next sequencing

- S1 is ready for root review as a pure foundation. It does not close T12, does not change runtime behavior, and must not be described as a navigation migration.
- The next code slice is **not** authorized by this handoff. S2 route-identity flip waits for: T10 S6 accepted, durable save/impact seam settled, and root review of the S1 ownership/overlay choices.
- The first adapter slice should replace only read projections (sidebar/breadcrumb/Back availability) and checked `NavState` construction; it must not introduce a mirrored legacy field.
- No commit, push, production access, app restart or frozen-artifact edit was performed.

**Stop point:** awaiting root review and explicit authorization for the first adapter slice.
