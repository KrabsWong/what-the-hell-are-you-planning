# T05 read-only audit: shared execution eligibility and status presentation

Date: 2026-11-08. Working tree, not committed. Line references are to the current uncommitted candidate; T23/T25 is actively changing schema/ownership code, so re-check call-site line numbers before editing.

Scope: T05 / F04. Read-only. No implementation, no commit, no production workspace access, no app restart, no broad QA. No cargo command was run because the shared schema migration is compiling.

## Executive result

The dispatcher already enforces a **per-goal phase barrier** and **ordered-goal barrier**. The presentation path does not.

- Dispatcher: `automation_execution.rs:936` `phase_ordered_ready_step` walks `Collect -> Analyze -> Advise` inside one goal; `prepare_occurrence_run` at `automation_execution.rs:967` additionally serializes goals in `research_goal_ids` order (`996-1012`).
- Display: `execution_status.rs:168` inspects only declared `depends_on`, active runs, and the ordinary in-memory `TodoBatch`. It has no occurrence, phase, goal-index, or period-lifecycle context.
- Between automation steps, `workflow.rs:63` can infer `Paused` from stored task stage alone (`83-88`), and ordinary hint text (`execution_status.rs:140-145`) tells the user to use the Research topic queue.

The smallest safe fix is one pure `ExecutionEligibility` result owned by a new module, consumed by both `prepare_occurrence_run` and `task_status`. Ordinary Research must keep its current dependency-only behavior; phase/goal/period rules apply only to automation-owned steps.

## Concrete mismatches

### M1 — Analysis shows Ready while collection is still running

Reproducible fixture state:

- Approved automation period with one goal and four steps: `C1` collect (done, current result), `C2` collect (running), `A` analyze (`depends_on = [C1]`), `R` advise (`depends_on = [A]`).
- `C1` has a current result. `C2` has no result. `A`'s declared dependency is already satisfied.

Current behavior:

- `Store::ready_research_tasks` (`batch.rs:82-99`) filters only declared dependencies, so `A` is in its ready list.
- `phase_ordered_ready_step` still returns `C2`, because the first unfinished phase is Collect (`automation_execution.rs:936-963`). Dispatcher decision: `C2`.
- `Store::execution_status(A)` reaches the research-step branch (`execution_status.rs:243-265`); `step_state(A)` is Waiting, `research_pending_approval` is false, `A.depends_on` is satisfied, so it returns `Ready`.
- `workflow::TaskStatus` therefore renders `Todo · Ready` (phase from `workflow.rs:101-107`, label from `52-59`) and reason from `execution_status.rs:140-142`: “Ready to run; not currently queued. Run the goal or add its topic to the queue.”
- The automation period step row (`automation_views.rs:2767-2932`) uses the same local `execution_status` call at `2819` but renders only an icon at `2856`, so it cannot even state the misleading label.

Expected: `A` displays “Waiting for collection” with the unfinished collection steps named; only `C2` is eligible.

This is a display/guidance defect, not observed out-of-order execution.

### M2 — Later-goal work shows Ready while the dispatcher stays in the first goal

Reproducible fixture state:

- One period with `research_goal_ids = [G1, G2]`.
- `G1` still has a running collection step. `G2`'s collection step is planned, has no dependencies, and its goal is approved.

Current behavior:

- `prepare_occurrence_run` iterates goals in approved order (`automation_execution.rs:996-1012`) and selects work from `G1`; it never considers `G2` until `G1` is complete.
- `Store::execution_status(G2_step)` has no occurrence/goal-index input. It sees an approved goal and satisfied dependencies, so it returns `Ready`.
- Period step presentation repeats that value at `automation_views.rs:2819`.

Expected: `G2_step` displays “Waiting for previous goal” naming `G1`; only `G1` work is eligible.

### M3 — Between-step and period-owned wait context can become ordinary “Paused”

`workflow.rs:63` computes `submitted` from the ordinary `TodoBatch` or active runs only (`68-77`). It then applies the ordinary “stopped” heuristic at `83-88`:

- stored task stage `InProgress`
- no ordinary batch membership and no `Queued/Running/Cancelling/Recovering` run
- execution `Ready` / `Waiting` / stage `InProgress`

→ `execution = Paused`, phase `InProgress`, reason from `119-121`, or the Paused hint at `execution_status.rs:143-145` (“The topic queue is paused…”).

Automation steps never join `TodoBatch`. Their submit/wait state lives on `AutomationOccurrence` (`Queued`, `Running`, `WaitingRetry`, `Verifying`, terminal). So the ordinary heuristic is the wrong authority.

Exact structural trigger (reachable whenever the persisted step stage is `InProgress` without an active attempt, for example after a stage snapshot/restore or an imported/recovered store): the user sees `Paused` even when the occurrence is `Running` or `WaitingRetry`. The definition being `Paused` is unrelated and must not produce this state.

Honest unknown: the current working tree has no single obvious normal-path sequence that leaves an automation step at `InProgress` with no run; the predicate is reachable through stored state and stage snapshots, and T23/T25 may change stage ownership. The regression must construct the state directly, not rely on timing.

Expected for an active occurrence: never `Paused`; show the period-owned state (`Ready` between steps, `Queued` for pool wait, `Retrying` for a scheduled retry, `Waiting` for a barrier). A `Paused` definition with a running manual/scheduled period must not pause that period; future triggers are a separate definition concern.

### M4 — Automation guidance leaks ordinary Research queue advice

`TaskStatus.reason` falls back to `ExecutionStatus::hint` (`workflow.rs:178-179`). The affected strings:

- Ready: “Run the goal or add its topic to the queue.” (`execution_status.rs:140-142`)
- Queued: “Queued to start automatically when this topic or an execution slot becomes available.” (`130-132`)
- Retrying: “… waiting for the retry delay or an execution slot.” (`133-135`)
- Paused: “The topic queue is paused…” (`143-145`)

These are correct for ordinary Research but never for automation-owned steps. The period-level copy already names the automation pool correctly (`automation_views.rs:2660-2676`, `2015-2032`); step-level copy contradicts it.

### M5 — Period/progress rows collapse eligibility to an icon with no explanation

- Period step row: local `execution_status` at `automation_views.rs:2819`; local presentation overrides at `2820-2834` (`Done` for any current result, `Stopping` for a terminal period); only `status.icon_view` at `2856`. No label, reason, tooltip, or accessible text.
- Run progress step row: local `execution_status` and a second local `Done` override at `run_views.rs:1039-1047`; only an icon at `1058`; tooltip at `1057` is the task title, not the status reason.

These local overrides are not the dispatcher rule. They must consume the shared result or be reduced to explicit presentation-only transforms (terminal-period `Stopping`, current-result `Done`). Otherwise “every surface says Waiting for collection” cannot be verified.

## Current call paths

### Dispatch path

1. `automation_runtime.rs:1618` `poll_automations` commits scheduled ticks and calls `enqueue_automation_occurrence` for queued periods (`1673`).
2. `automation_runtime.rs:1750` `enqueue_automation_occurrence` validates active occurrence status (`1762-1770`), existing active run (`1771-1784`), expiry/deadline (`1785-1794`), then calls `prepare_occurrence_run` (`1799`).
3. `automation_execution.rs:967` `prepare_occurrence_run`:
   - active status/deadline guards (`972-985`)
   - ordered goals (`996-1012`)
   - `Store::ready_research_tasks` (`batch.rs:82`) for declared dependencies only
   - `phase_ordered_ready_step` (`automation_execution.rs:936`) for per-goal phase order
4. Manual step start: `app.rs:1354` `start_run_with_navigation` → `automation_runtime.rs:1699` `route_automation_task`.
   - `active_automation_occurrence_for_goal` at `1686` finds the active occurrence.
   - `1716-1727` accepts the step using `ready_research_tasks(goal)` only. This bypasses the phase check: clicking “Run step” on the ready `A` above passes this guard, enqueues the period, and the dispatcher then starts `C2`. The user sees an action on `A` start a different step.
5. Manual goal start: `research_app.rs:181` `run_research_next` → `automation_runtime.rs:1732` `route_automation_goal` → same enqueue path.

### Presentation path

1. `workflow_views.rs:5` `display_task_status` → `Store::task_status` (`workflow.rs:63`) → `Store::execution_status` (`execution_status.rs:168`), with a per-frame cache cleared at `views.rs:3592`.
2. Research-step branch: `execution_status.rs:243-265` (blocked, approval, declared dependencies, pending batch).
3. Goal aggregate: `execution_status.rs:222-241` and summary at `278-287`; no phase/goal-index awareness.
4. Ordinary list: `views.rs:1942` `task_row`, status label at `2025-2026`.
5. Ordinary board: `views.rs:2179` `goal_card`, chip/reason at `2260-2262`.
6. Task detail: `views.rs:2952-2962`.
7. Goal steps list/board: `goal_steps.rs:250-437`; label at `310`/`345`/`375`; reason at `347`.
8. Step execution panel: `goal_steps.rs:126-221`; label/reason at `152-154`.
9. Period steps: `automation_views.rs:2767-2932`; local status at `2819`, icon at `2856`.
10. Run progress steps: `run_views.rs:1039-1058`.

## Smallest shared pure design

New module: `src/execution_eligibility.rs`. Pure `Store` queries only; no GPUI types, no clocks except an explicitly passed `now`, no mutation.

```rust
pub(crate) enum OwnerDomain {
    OrdinaryResearch { goal: u64 },
    AutomationPeriod(AutomationOwnership),
}

pub(crate) struct AutomationOwnership {
    pub automation: u64,
    pub occurrence: Option<u64>, // None = historical/orphaned automation entity
    pub goal: u64,
    pub goal_index: usize,       // position in research_goal_ids
    pub goal_count: usize,
    pub period: PeriodState,     // Active, Queued, WaitingRetry, Verifying, Ended, Missing
}

pub(crate) enum ActiveAttempt {
    None,
    Queued,
    Running,
    Stopping,
    Retrying { attempt: u32 },
}

pub(crate) enum StepOutcome {
    Open,
    Done,
    Failed,     // only this step's own attempt failed/cancelled
    Cancelled,
}

pub(crate) struct PhaseBarrier {
    pub phase: ResearchKind,       // earlier phase that must finish
    pub unfinished: Vec<u64>,      // steps with no current result
}

pub(crate) struct GoalBarrier {
    pub earlier_goal: u64,
    pub earlier_goal_title: String,
}

pub(crate) struct ExecutionEligibility {
    pub owner: OwnerDomain,
    pub active_attempt: ActiveAttempt,
    pub approval_pending: bool,
    pub outcome: StepOutcome,
    pub goal_barrier: Option<GoalBarrier>, // automation only
    pub phase_barrier: Option<PhaseBarrier>, // automation only
    pub unmet_dependencies: Vec<u64>,
    pub pool_wait: Option<PoolWait>,       // Automation / Research
}

pub(crate) enum PoolWait {
    AutomationSlot,
    ResearchQueue,
}

pub(crate) enum EligibilityDecision {
    Done, Blocked, Approval, Running, Stopping, Retrying,
    Queued, Waiting, Ready, Ended, Missing,
}
```

Derived precedence (single function, used by display and dispatch):

1. Missing task / archived / cancelled → `Missing` or `Blocked`.
2. Current result → `Done`.
3. Own failed or cancelled attempt → `Blocked` (only the step that actually failed).
4. Active attempt: Running/Recovering/Cancelling → `Running`; queued auto-retry → `Retrying`; queued → `Queued`.
5. Approval pending → `Approval`.
6. Period `Verifying` → `Waiting` (“checking after restart”).
7. Period terminal with unfinished step → `Ended` (presentation: Waiting; never Ready).
8. Automation goal barrier → `Waiting`.
9. Automation phase barrier → `Waiting`.
10. Unmet declared dependencies → `Waiting`.
11. Period `Queued` for automation pool → `Queued` (PoolWait::AutomationSlot).
12. Otherwise `Ready`.

Notes:

- Evaluate barriers **before** pool wait. A later-phase step in a queued period is “Waiting for collection”, not merely “Queued”; an eligible collection step is “Queued for an automation slot”.
- Do not read `DefinitionStatus` in this function. A Paused definition does not stop an already-active period. The automation list continues to show definition state separately.
- Phase and goal barriers are computed only for `OwnerDomain::AutomationPeriod`. Ordinary Research keeps its existing dependency-only eligibility and hints; this preserves T02/T07/T09 behavior.
- `unmet_dependencies` should use the same predicate as `ready_research_tasks` (missing/archived/non-Done/no current result). Do not duplicate a weaker check.
- Phase unfinished predicate should match `phase_ordered_ready_step`: `stage != Cancelled && research_result(id).is_none()` and `research.kind == earlier_phase`.
- Goal barrier: any goal earlier in `research_goal_ids` for which `goal_tasks_complete` is false.
- Period lookup: match the step goal against occurrence `research_goal_ids` / `research_goal_id`; prefer the active occurrence, otherwise the latest containing occurrence. Do not use `occurrence_activity`/T16 ordering helpers.

Dispatch API shape:

```rust
impl Store {
    pub(crate) fn execution_eligibility(&self, task: u64) -> ExecutionEligibility;
    pub(crate) fn next_eligible_goal_step(&self, goal: u64) -> Option<u64>;
    pub(crate) fn next_eligible_occurrence_step(&self, occurrence: &AutomationOccurrence) -> Option<u64>;
}
```

`next_eligible_goal_step` must reproduce current selection exactly:

- walk `Collect -> Analyze -> Advise`;
- inside the first phase with unfinished work, return the first step whose eligibility is `Ready` and whose phase is that phase;
- preserve the current `ready.first()` fallback;
- for `next_eligible_occurrence_step`, walk goals in order; return the first goal step; if a goal has no eligible step and is incomplete, stop (preserve the current “next goal not runnable yet” error).

## Exact integration points

1. `src/main.rs`: add `mod execution_eligibility;` (one line). Coordinate if T23/T25 is also touching module registration.
2. `src/execution_status.rs`:
   - keep `ExecutionStatus` as drawing/presentation vocabulary;
   - map eligibility decision to `ExecutionStatus` for research steps;
   - replace the research-step dependency-only branch (`243-265`) with the shared result;
   - keep ordinary goal aggregate behavior unless it reads a step status through the shared path;
   - leave the ordinary hint strings intact for ordinary Research.
3. `src/workflow.rs`:
   - add `barrier`/owner context to `TaskStatus` (or derive labels/reasons from eligibility stored alongside);
   - `label()` returns the exact phrase for automation barriers: `Waiting for collection`, `Waiting for analysis`, `Waiting for previous goal`, `Queued for automation slot`;
   - `reason` prefers eligibility reason for automation owners and barriers, otherwise preserves existing ordinary branches;
   - replace the automation `stopped` path: automation `submitted` comes from occurrence state, not `TodoBatch`; never synthesize `Paused` from an automation step's stored stage.
4. `src/automation_execution.rs` — **T23/T25-owned; do not edit under T05**:
   - after ownership settles, replace `ready_research_tasks` + `phase_ordered_ready_step` in `prepare_occurrence_run` with `next_eligible_occurrence_step`;
   - delete or reduce `phase_ordered_ready_step` to a wrapper so there is one rule;
   - preserve goal-order error semantics and deadline/status guards.
5. `src/automation_runtime.rs` — narrow guard only:
   - `route_automation_task` (`1716-1727`) should compare against `next_eligible_goal_step(goal)`, not `ready_research_tasks`, so an action on `A` cannot enqueue `C2`. Preserve T02/T03 passive/navigation behavior; this changes only the accept/reject check and feedback text.
6. `src/automation_views.rs` — presentation-only window, `automation_occurrence_steps` (`2767-2932`):
   - replace local `execution_status` at `2819` with `display_task_status(task_id)`;
   - keep the terminal-period `Stopping` and current-result `Done` transformations as explicit presentation overrides, but derive the base state from shared eligibility;
   - add a compact status label or a status tooltip/accessible label to the row so the barrier phrase is actually present. Do not touch creation/planning/settings/navigation/result-policy areas.
7. `src/goal_steps.rs`: likely no code change; `TaskStatus.label`/`reason` feed the existing label and panel. Add tests only if a rendering selector is needed.
8. `src/views.rs`: no change needed; list/board/task detail already render `TaskStatus.label()` and `reason.clone()`.
9. `src/run_views.rs`: same local `execution_status` + `Done` override at `1039-1047`; only the status cell. Coordinate with T06, which owns result/progress presentation. Prefer T05 exposing the shared label and T06 applying it if T06 lands first; otherwise a minimal label/tooltip change is in T05 scope.
10. `src/execution_status.rs` `hint()` remains ordinary-only. Do not add automation wording there unless it takes an explicit owner; keep ownership in the shared result.

## Regression matrix (dispatcher + display together)

Each automation test must assert both the selected step and the displayed status/reason. Prefer pure `Store` assertions in the new module plus a thin `workflow` assertion; one focused QA observation can verify the period row text.

| Case | Fixture | Assert dispatcher | Assert display |
| --- | --- | --- | --- |
| T05-A1 phase barrier | Plan `C1`, `C2`, `A(dep C1)`, `R(dep A)`; `C1` done; `C2` running | `prepare_occurrence_run` selects `C2` | `A` is Waiting; label contains `Waiting for collection`; reason names `C2`; reason never contains `Ready` or `add its topic to the queue` |
| T05-A2 phase clears | Same, then `C2` done with current result | `prepare_occurrence_run` selects `A` | `A` no longer has a phase barrier; displays `Ready` (or `Queued` only if the occurrence is queued), with automation-specific ready copy |
| T05-B goal barrier | Occurrence goals `[G1, G2]`; `G1` collection running; `G2` collection planned, no deps | selects `G1` step; `G2` is not selected | `G2` step label `Waiting for previous goal`; reason names `G1`; never `Ready` |
| T05-C failed vs descendants | `C2` latest run Failed with retry exhausted; `A` deps satisfied; occurrence terminal/WaitingRetry | terminal period: `prepare_occurrence_run` errors; no automatic selection of `A` | exactly `C2` is Blocked; `A` is Waiting with phase reason, not Blocked, not Ready |
| T05-D between steps | Active Running occurrence; next step stage `InProgress`; no run, no ordinary batch; deps and phase clear | `prepare_occurrence_run` selects the step | not `Paused`; not topic-queue copy; Ready between steps with automation dispatch copy |
| T05-E paused definition, running period | Definition `Paused` (or `NeedsAttention`); occurrence `Running`; selected step eligible | dispatcher still selects/runs the period step | step and period show Running/Ready, never `Paused` from definition status; definition status stays separate in the automation list |
| T05-F queued period | Occurrence `Queued`; eligible collection step; no run | `prepare_occurrence_run` can prepare the step; pool admission remains app-owned | display `Queued for automation slot`; reason mentions automation slot and explicitly not Research queue admission |
| T05-G terminal period, unfinished step | Occurrence `Failed`/`Cancelled`; downstream step has no result | `prepare_occurrence_run` errors | downstream Waiting with “period ended; retry the period”, never Ready; failed step only is Blocked |
| T05-H orphan automation entity | `task.automation_id` set; no matching occurrence; no result | not dispatchable | Waiting with “period not active”; no Ready and no ordinary Topic advice |
| T05-I approval | Automation goal approval invalid | dispatcher refuses via approval | display Approval with review-plan reason |
| T05-J ordinary preservation | Ordinary Research with same `C1/C2/A` shape and `A` deps satisfied | ordinary path unchanged | ordinary status keeps current behavior and hints; no automation phase barrier is introduced |
| T05-K period row copy | A1 fixture rendered in period dialog | — | period step row exposes the same barrier text/tooltip as `TaskStatus`, not only an icon |

UI verification for T05-K should be a focused isolated observation, not the full T20 matrix. No screenshots are required for the pure cases.

## Proposed ownership

**T05 owns (exclusive):**

- new `src/execution_eligibility.rs` and its pure tests;
- `src/execution_status.rs` eligibility mapping;
- `src/workflow.rs` `TaskStatus`/reason/label and automation submitted-state correction;
- `src/main.rs` module-registration line;
- `src/automation_views.rs`, only the `automation_occurrence_steps` presentation block;
- `src/automation_runtime.rs`, only the `route_automation_task` eligibility guard block;
- focused tests in `execution_status.rs`, `workflow.rs`, new module, and the automation period-row observation.

**Handoff to T23/T25 (do not edit):**

- `src/automation_execution.rs` integration of `next_eligible_occurrence_step` into `prepare_occurrence_run`; removing/reducing `phase_ordered_ready_step`;
- `src/domain.rs`, `src/app.rs`, planning/form areas of `automation_views.rs`, navigation result policy, persistence/schema.

T05 can ship the pure module and presentation path first with `automation_execution.rs` still on its old rule; that would make display correct but not prove one shared source. T05 acceptance must wait for the small T23/T25-owned dispatcher call-site change and the joint tests.

**Excluded / frozen:**

- T01: baseline QA, `navigation_header` geometry.
- T16: `automation_activity.rs` and ordering helpers; eligibility must not depend on activity ordering.
- T02: passive background navigation; no new route writes.
- T03: retry snapshot semantics; eligibility may describe a retry but must not mutate it.
- T04: undo invalidation/snapshot protection.
- T07: navigation neighbors/period ownership.
- T09: draft defaults/form construction.
- T18: automation policy/validation/capability copy.
- T06: result kind/report/reader selection; coordinate only on the run-progress status cell.
- T11/T19: control affordances/visual keyboard work. No VoiceOver work, no new icons, no emoji.

## Preserved contracts and non-goals

- Definition status (`Enabled/Paused/NeedsAttention/Draft/Archived`) stays separate from period and step state.
- A Paused definition does not pause an already-running occurrence.
- Only the step whose own attempt failed is Blocked; downstream steps are Waiting with a phase/dependency/goal reason.
- The dispatcher keeps ordered goals and per-goal phase order; no concurrency increase and no schedule/cursor changes.
- Ordinary Research keeps dependency-only ordering and current queue guidance.
- No ordinary Research FIFO advice is shown on automation-owned steps.
- No broad rewrite of `execution_status`, `workflow`, or automation views; no result presentation redesign.
- No commit, push, production workspace access, or app restart.

## Unknowns and bounded follow-up checks

1. Exact real-world frequency of the between-step `Paused` predicate: the stored-state trigger is confirmed; the normal-path sequence depends on stage snapshot/restore and may change under T23/T25. The regression must build the state directly.
2. Whether an existing test helper can construct a two-goal occurrence without private/QA-only setup. Inspect only the relevant fixture at implementation time; do not add broad QA.
3. Same-phase sibling ordering. The dispatcher picks the first ready step in the active phase; T05 does not require distinguishing sibling order in presentation. Do not expand scope unless a test shows contradictory guidance.
4. T23/T25 may rename or relocate ownership/stage fields. Re-run the pure tests and the dispatcher joint tests after that migration settles.
5. `run_views.rs` progress-row ownership with T06. If T06 changes the progress presentation first, T05 supplies the label/reason and T06 consumes it; do not duplicate a second status model.
6. No full build/test evidence is claimed by this audit. The shared tree was compiling during schema migration, so no cargo/QA command was run.
