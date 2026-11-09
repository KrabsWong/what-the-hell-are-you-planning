# T23/T25 audit and bounded implementation map

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Scope: T23 (persist run ownership across planning attempts) and T25 (bind replanning to the edited request revision)
Mode: read-only audit. No app source, maintained documentation, workspace data, or real research was changed. The only file written is this report. No commit, push, app restart, VoiceOver work, or real workspace access.

**Status:** audit complete; implementation has **not** started. T23 and T25 remain Open. This report is not an acceptance claim.

---

## 1. Executive finding

The current design has one mutable relation between a planning run and its request:

- `ResearchRequest.planning_run_id` is a single `Option<u64>` (`src/request_planning.rs:23`).
- `AgentRun` has no planning-request owner. Its only ownership-like field is `automation_occurrence_id`, which is for period steps, not planning (`src/domain.rs:215-218`).
- `request_for_run` resolves ownership by searching for the request whose **current** `planning_run_id` equals the run ID (`src/request_planning.rs:329-333`).
- `create_automation_request` returns the existing request for the automation and ignores the new input (`src/request_planning.rs:285-291`).

Consequences:

1. A replan reuses the original request and therefore plans the original objective. The edited objective is saved on the automation definition, but the planner prompt still uses `request.input`.
2. Expected-result text is never part of the request or planner context. The Current helper copy correctly admits this (`src/automation_views.rs:3017`), so T25 must finish the contract rather than only change copy.
3. Only the newest attempt is identifiable. Older attempts lose their request relation when `planning_run_id` moves, become "ordinary" runs under the selection heuristics, and can appear in Research results. This is exactly F21/T23.
4. Completion/apply uses the same mutable pointer (`src/project_app.rs:205-218`), so an old in-flight response can be applied after a newer revision starts unless a new stale guard is added.
5. Enable checks only `request.input` vs the live objective, not the expected result, revision, or the run's captured signature (`src/automation_views.rs:1284-1302`). This cannot prove the template was generated for the current contract.

T09's late-completion token, T02's passive completion, T18's validation/truthful copy, and T03's occurrence-snapshot retry are structurally sound and must be preserved. T25 needs to add revision/expected binding underneath them, not replace them.

---

## 2. Current concrete behavior

Working-tree line numbers; re-read before editing because T16/T01 may move them.

| Surface | Current code | Defect |
| --- | --- | --- |
| Request schema | `src/request_planning.rs:10-26`: `automation_id`, `input`, `planning_run_id`, no expected result, no revision | No immutable revision identity; expected output absent |
| Create automation request | `src/request_planning.rs:266-313`; reuse at `285-291` | Replan silently plans old `input`; expected result cannot participate |
| Plan signature | `src/request_planning.rs:335-373`; 7-tuple at `359-367` | Objective changes are detected only if the request is edited; expected changes are invisible |
| Plan prompt | `src/request_planning.rs:462-480` | Context contains `raw_request` only; expected result and revision absent |
| Repair lookup | `src/request_planning.rs:484-488` | Previous failed output found through the mutable `planning_run_id` |
| Prepare run | `src/request_planning.rs:497-534` | Run stores no planning request/revision/automation owner |
| Apply plan | `src/request_planning.rs:537-590` | Applies by request ID, not by the run's captured revision; no run/revision argument |
| Finalize | `src/project_app.rs:196-278`; lookup `205-207`, signature `210-212`, template apply `218-222`, UI callback `247-250` | Old attempt can be applied when it is still the pointer target; no latest-revision/stale check |
| Start planning | `src/automation_views.rs:1080-1130`; request `1088`, pointer `1091-1098` | Objective only; no expected; pointer written before launch persistence |
| Replan | `src/automation_views.rs:874-890` | Clears the UI plan but does not supersede/version the request |
| Review in-flight | `src/automation_views.rs:894-912` | Restores the run token from the latest request's pointer |
| Late completion | `src/automation_views.rs:1136-1156` | T09 token works, but there is no request/revision/content check |
| Enable | `src/automation_views.rs:1280-1351`; check `1284-1302` | Objective-only; expected and captured signature ignored |
| Template apply | `src/automation_execution.rs:236-295` | No request/revision binding; name/template/version replaced from output |
| Approval | `src/automation_execution.rs:184-230`, `516-540` | Approval records template/schedule only; no plan request/revision |
| Ordinary selection | `src/navigation.rs:426-445` | Excludes only `automation_occurrence_id`; automation planning runs pass |
| Results list | `src/run_views.rs:86-91`, `93-145`, `190-208` | Automation detection depends on the mutable request pointer |
| Results fallback | `src/run_views.rs:172-179` | Uses raw `store.runs.last()` for the planning-activity flag |
| Navigate to results | `src/app.rs:701-718` | Clears only occurrence-owned selections, not automation planning runs |
| Sidebar count | `src/domain.rs:623-643` | Already excludes project plans, but ownership is not stated by one policy |
| Dispatch validity | `src/app.rs:1706-1727`; request lookup `1713-1716` | Signature uses pointer lookup; okay only while ownership survives |
| Recovery planning list | `src/recovery.rs:762-815` | Shows planning requests and routes to `Scope::Runs`; automation planning would land in ordinary results |
| Breadcrumbs | `src/views.rs:1318-1368` | Uses `displayed_run_id`; ordinary result text can therefore contradict the visible list |
| Persistence | `src/persistence.rs:9`, `42-53`, `139-141`, `149-150`, `213-215`; `src/sqlite_storage.rs:10-11`, `104-139` | Whole-Store payload/entity tables; new fields need serde defaults plus load normalization |
| Signature path rewrite | `src/sqlite_storage.rs:239-254` | Knows request-plan tuple length 7; an 8-tuple with expected result is not handled |

---

## 3. Required invariants

1. A planning attempt's request/revision owner is immutable after dispatch. It must never be inferred solely from a request's latest pointer.
2. The planning input is immutable per request revision. Editing objective or expected result creates a new revision (or an equivalent new request); it never mutates an in-flight input.
3. A run can be applied only if:
   - its immutable request owner still exists;
   - its captured signature equals the request revision signature;
   - its revision is the automation's current planning candidate;
   - the stored automation objective and expected result equal that revision's content.
4. Any mismatch is terminal `Stale` (or `Failed` with a stale log), preserves the run output, leaves the previous template/approval/periods untouched, and keeps the edited draft.
5. Research results, its fallback selection, breadcrumbs, counts and recovery navigation use one ownership predicate. Automation planning attempts appear only under their automation.
6. Previous occurrences keep their original snapshot objective/expected/template (already true via `AutomationOccurrenceSnapshot`, `src/automation.rs:399-414`; materialization at `src/automation_runtime.rs:644-657`). Do not alter retry semantics.
7. Legacy requests without expected result remain readable and keep their old signature. Do not invent expected text from the current definition.
8. Ambiguous legacy histories are neither deleted nor silently assigned. They remain visible as legacy/unowned history.

---

## 4. Minimal persisted model

Add fields with serde defaults; do not change SQLite tables (requests still live in the `workspace` metadata payload, `src/sqlite_storage.rs:104-138`).

### 4.1 `ResearchRequest` (`src/request_planning.rs`)

```rust
pub struct ResearchRequest {
    // existing fields...
    /// None means a pre-T25 request: expected result was never captured.
    #[serde(default)]
    pub expected_result: Option<String>,
    /// Per-automation planning revision. Legacy requests normalize to 1.
    #[serde(default = "default_planning_revision")]
    pub revision: u32,
    // planning_run_id remains the latest-attempt display cache, not ownership.
}
```

Semantics:

- `input` and `expected_result` are immutable after creation.
- New automation requests always set `expected_result: Some(trimmed)` even when empty, so "absent (legacy)" and "captured empty" are distinguishable.
- `revision` increments only when the `(project, input, expected)` contract changes for the same automation.
- Repeated unchanged replan reuses the same request ID/revision.
- Ordinary research requests keep `revision = 1` and `expected_result = None`; no behavior change.

### 4.2 `AgentRun` (`src/domain.rs`)

```rust
#[serde(default)]
pub planning_request_id: Option<u64>,
#[serde(default)]
pub planning_revision: Option<u32>,
/// Immutable automation owner for automation planning attempts.
#[serde(default)]
pub automation_id: Option<u64>,
```

- `planning_request_id` is authoritative ownership for every planning attempt.
- `planning_revision` is a redundant integrity check/display value.
- `automation_id` lets selection work without a request lookup and survives pointer changes.
- Period steps keep `automation_occurrence_id`; do not set `automation_id` on them unless the unified predicate needs it (derive automation from the occurrence instead).

### 4.3 `AutomationDefinition` / approval (`src/automation.rs`)

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPlanBinding {
    pub request_id: u64,
    pub revision: u32,
    /// Signature captured by the planner run that produced the template.
    pub input_signature: String,
}

// AutomationDefinition
#[serde(default)]
pub plan_binding: Option<AutomationPlanBinding>,

// AutomationApproval
#[serde(default)]
pub plan_request_id: Option<u64>,
#[serde(default)]
pub plan_revision: Option<u32>,
```

Why both: the binding proves which revision produced the template; the approval records what the user approved. Legacy definitions/approvals have `None` and remain valid under their old behavior.

Do **not** include `plan_binding` in `definition_form_signature` (`src/automation_views.rs:732-744`). It is derived state, not a form field; including it would force a version bump on an unchanged save.

---

## 5. Ownership and selection policy

Put the predicate in `Store` so T12 can migrate it later:

```rust
impl Store {
    pub fn run_is_automation_owned(&self, run: &AgentRun) -> bool;
    pub fn run_is_ordinary_result(&self, run: &AgentRun) -> bool; // !owned && !archived
    pub fn latest_ordinary_result_run(&self) -> Option<&AgentRun>;
    pub fn request_for_run(&self, run_id: u64) -> Option<&ResearchRequest>; // owner-aware
}
```

`run_is_automation_owned`:

1. `automation_occurrence_id.is_some()`; or
2. `automation_id.is_some()`; or
3. the owner-aware `request_for_run` returns a request with `automation_id.is_some()`; or
4. legacy fallback only: a non-planning run whose task has `automation_id`.

Do not treat an automation-owned task guard in `launch_run` as ownership for planning, because planning runs have `task_id = 0` and a project-plan marker.

`request_for_run(run_id)`:

1. Find the run and use `run.planning_request_id` when present.
2. Otherwise use the legacy pointer (`request.planning_run_id == run_id`).
3. Otherwise, only for reads/migration, match an unambiguous `request_plan_signature(request.id) == run.input_signature`. Never write this inference at call time; do it in load normalization so it is persisted once and testable.

Apply this policy to every result-selection path:

- `src/navigation.rs::displayed_run_id`: selected/fallback must be `run_is_ordinary_result` unless `task_result_context` explicitly allows the automation occurrence. Also apply the archived filter consistently with `runs()`.
- `src/run_views.rs`: delete the local pointer-based `run_is_automation`; call the Store predicate in `run_projects`, the `runs` selector and fallback. Replace `store.runs.last()` in `reveal_selected_run` with `displayed_run_id()`/the shared policy.
- `src/views.rs`: breadcrumbs already use `displayed_run_id`; after the policy change content/title/crumb agree. Do not add a second fallback.
- `src/app.rs::navigate(Scope::Runs)`: clear a selected run when `run_is_automation_owned`, not only when an occurrence ID is present.
- `src/domain.rs::progress_result_task_count` and any sidebar count: use the same policy (it already produces the right count; centralize the reason).
- `src/recovery.rs` planning-request section: exclude automation-owned planning runs from the ordinary project section. Route them to the automation attempt dialog, or omit them there because the automation settings/detail lists them. Do not send them to `Scope::Runs`.
- `src/run_views.rs::run_content` and `src/views.rs` automation task branch remain occurrence/task based; planning attempts need a separate read-only review surface (section 8).

`displayed_run_id` contract:

1. A valid selected run that the context allows.
2. Otherwise the latest ordinary result run (excluding automation-owned and archived).
3. An automation step is allowed only when `task_result_context` is `Some`.
4. Content, title, breadcrumb and history all call this one result; no local `last()` fallback.

---

## 6. Planner context and signature

### 6.1 Request creation/revision

Replace the reuse-by-automation behavior with revision-aware reuse:

```rust
pub fn create_automation_request(
    &mut self,
    automation: u64,
    project: u64,
    input: impl Into<String>,
    expected_result: impl Into<String>,
) -> Result<u64, String>;
```

Algorithm:

1. Validate trimmed input/expected lengths and automation/project.
2. Find requests with `automation_id == Some(automation)`.
3. Let `latest = max_by_key(revision, id)`.
4. If `latest` has the same `project_id`, `input`, and `Some(expected)` → return its ID (repeated unchanged replan).
5. Else allocate a new request ID, `revision = latest.revision + 1`, and store immutable content.
6. Never mutate an existing request's input/expected/revision.

Keep `planning_run_id` updated only as a display/legacy cache after a run is accepted. It must not be the source of ownership.

### 6.2 Signature

In `request_plan_signature`, preserve the existing 7-tuple for legacy requests (`expected_result == None`) so old runs stay valid. For new requests append the expected result as the 8th element:

```text
(id, project_id, input, submitted_at, project.name, project.directory, topic, expected_result)
```

Rules:

- `expected_result: None` → exactly the old 7-tuple.
- `expected_result: Some("")` → 8-tuple with an empty string. This distinguishes a captured empty answer from legacy.
- Do not include `revision` in the signature: the request ID already identifies the revision, and adding it would churn old fixtures. Compare `run.planning_revision` separately.
- `src/sqlite_storage.rs::transform_paths`: add `Some(8) => Some(5)` for request-plan signatures. The project directory remains index 5; do not shift indices. Add a relocation test with an 8-tuple.
- `replace_signature_path` already handles arbitrary indices; only the tuple-length mapping changes.

### 6.3 Prompt

Extend the context at `src/request_planning.rs:462-466`:

```json
{
  "request_id": ...,
  "revision": ...,
  "submitted_at": ...,
  "raw_request": ...,
  "expected_result": ...,
  "project": ...,
  "selected_topic": ...
}
```

Add one instruction sentence near the deliverable guidance:

> Treat expected_result as the acceptance contract for the requested report. Preserve it; do not broaden the task or invent deliverables outside it. If it is empty or absent, use the concise sourced conclusion described above.

Then update the Research helper at `src/automation_views.rs:3017` to state that both the research description and expected result are captured in the planning revision. Do not claim the plan is bound until the implementation sets `plan_binding`.

### 6.4 Prepare

`prepare_request_plan` must:

1. Capture the request revision content at preparation time.
2. Set `planning_request_id`, `planning_revision`, and `automation_id` on the returned run.
3. Use the immutable request content for the prompt/signature.
4. Reject if a new revision for the same automation already exists and differs (the caller should start the new revision instead).
5. Find a repair candidate by `planning_request_id`, not by the mutable pointer.

`request_plan_auto_repair` must resolve the request through the owner-aware lookup and compare the retired run's signature to that request revision.

---

## 7. Atomic transitions

### 7.1 Start (create or replan)

`src/automation_views.rs::start_automation_planning`:

1. `save_automation_form` revises the definition to the current objective/expected and clears `plan_binding`/approval if the form changed. This must remain atomic (T09/T13 pattern).
2. `create_automation_request(automation, project, objective, expected)` creates/reuses the revision.
3. `prepare_request_plan(request)` returns a run already carrying the immutable owner/revision/automation.
4. Set `request.planning_run_id = Some(run.id)` only after the run is known; launch persists request+run together.
5. If launch fails, clear the pointer only if it still equals this run; leave the request and draft intact; do not apply a template.
6. Set the UI token (`planning_automation`, `planning_run`, and a new `planning_request`/`planning_revision` pair) from the same values.
7. No navigation. Preserve T02.

Replan without edits: the form signature comparison clears the template/approval, creating a new definition version, but `create_automation_request` reuses the same request because content is unchanged. Applying the new template must still bind to that request/revision and increment the definition version.

### 7.2 Continue while a plan is in flight

`automation_wizard_next` step 3 currently preserves any in-flight attempt (`src/automation_views.rs:1048-1051`). Add:

1. Read the current objective/expected.
2. Compare them with the in-flight request revision content.
3. If identical → keep the attempt and go to step 3 (existing behavior).
4. If different → create a new revision and dispatch a new run immediately; replace the UI token. The old run remains stored and becomes stale when it completes.
5. Never mutate the in-flight request.

This is the objective A→B UI path.

### 7.3 Completion (success)

`src/project_app.rs::finalize_project_plan`:

1. Resolve the request with the owner-aware lookup. If `run.planning_request_id` is set but the request is missing, mark `Stale`; do not fall through to project planning.
2. If automation-owned:
   - require `request.id == latest_planning_request(automation).id`;
   - require `request.automation_id == Some(automation)`;
   - require `request_plan_signature(request.id) == run.input_signature`;
   - require `run.planning_revision == Some(request.revision)`;
   - require the stored definition objective/expected (trimmed) equal the request's input/expected (`None` legacy handled separately).
3. Apply the template with the request ID/revision: `apply_automation_template(automation, request.id, request.revision, &run.output, now)`. On success set `plan_binding` and clear approval.
4. If any check fails: set run `Stale`, append one explanatory log line, preserve output/request, leave template/version/approval/periods unchanged, and do not call `automation_plan_ready` with a success state.
5. Success calls `automation_plan_ready(automation, run_id, cx)` once, with the UI still passive (T02).

### 7.4 Completion (failure)

- Run becomes `Failed`; output/logs retained.
- Request/revision/draft remain editable.
- No definition mutation and no UI wizard jump.
- If the T09 token matches, clear only the in-flight token and show "Planning failed. Retry against this request."; preserve the objective/expected text.
- Retry calls `prepare_request_plan` on the same request revision, so the captured prompt remains B.

### 7.5 Concurrent edit while planning

Two cases:

1. **Unsaved text differs from the stored definition.** The domain may validly apply the plan for the stored revision, but `automation_plan_ready` must compare the live form text to the applied binding. If they differ, keep the plan out of the review UI (or visibly label it stale), do not advance, and show an error that a new replan is required. The draft text survives.
2. **Saved text creates a new revision before completion.** The latest-revision guard marks the older run `Stale`; the newer run is the only applicable one. UI tokens prevent the older callback from repopulating the wizard.

`automation_plan_ready` must therefore receive `cx` (or a precomputed `form_matches` flag) and check:

- T09 token equality;
- Research shape;
- applied binding/request ID equals the run's owner;
- current form objective/expected equal the binding's request content.

### 7.6 Enable

`src/automation_views.rs::enable_automation` and `src/automation_execution.rs::approval_for`:

1. For Research with a template and at least one automation planning request:
   - require `definition.plan_binding` is `Some`;
   - require binding request == latest request revision;
   - require binding signature == current request signature;
   - require definition objective/expected == request content;
   - require the approval's plan request/revision equals the binding.
2. If any check fails, return the existing actionable wording: use Replan to generate and review a plan for the current request. Do not silently approve an old template.
3. Expected-only edits must fail the same check, not just objective edits.
4. Legacy automation with a template/approval but no planning request keeps its old eligibility. This preserves old workspaces; do not invent a revision for them.
5. `approval_current` must compare the approval's plan binding when present, and remain true for legacy approvals with `None`.
6. `revise_automation_with_shape` and `apply_automation_template` clear `plan_binding`; only a successful apply sets it. Name-only rename does not clear it.
7. Preserve T03: occurrence retry still uses the frozen occurrence snapshot and never re-enables the schedule or re-plans.

### 7.7 Restart

- New fields are serde defaults; the whole-store snapshot persists them.
- Load normalization fills only missing ownership and never rewrites immutable content.
- A queued/running planning run remains owned after restart; its attempt is accessible even if no worker continues it.
- After three attempts, Research results still excludes all automation planning runs, and the automation attempt list shows all three in revision/attempt order.

---

## 8. Attempt access UI

T23 requires all attempts to remain accessible from the automation. Add the smallest read-only surface, not a second results system:

- Add `automation_ui.attempt_dialog: Option<u64>` (run ID).
- Add `Store::planning_attempts(automation) -> Vec<PlanningAttemptView>` returning request revision + run status/time/output availability, ordered by request revision ascending or newest-first consistent with the settings section. Do not touch occurrence history ordering (T16).
- Render a compact "Plan versions" section in the existing Automation Settings dialog (`automation_settings_dialog`, near the approved/summary block). Each row shows revision, submitted time, run status and an Open/Review action. A stale/unavailable run still lists its stored status and says no applicable template was installed.
- `Open` sets `attempt_dialog`; a small modal shows read-only request input, expected result, run status/logs/output (or a bounded output preview) and close. Reuse the existing modal scrim/focus/Escape pattern.
- Breadcrumb for an attempt is Automations / <automation>; never All research.
- Empty state: "No planning attempts yet."
- Ambiguous legacy attempt with no owner cannot appear under an automation; keep it visible in Research results rather than assigning it. If root wants it discoverable from a specific automation, that requires an explicit migration mapping, not a heuristic.

Do not reuse `result_dialog`, which is occurrence-scoped, and do not route planning attempts into `Scope::Runs`.

---

## 9. Conservative legacy inference

Add `Store::normalize_planning_ownership()` in `src/request_planning.rs`; call it from both load paths in `src/persistence.rs` after `normalize_automation_schema` and before/with `normalize_topics` (`139-141`, `149-150`, `213-215`). It must be idempotent.

Algorithm:

1. Normalize each request: `revision = max(1, revision)`; leave `expected_result = None` for legacy; never synthesize it from the definition.
2. For each run with `planning_request_id == None` and `project_plan != None`:
   a. **Current pointer:** a request with `planning_run_id == run.id` is authoritative; assign it.
   b. **Unique signature:** find automation requests whose `request_plan_signature(request.id) == run.input_signature`. If exactly one, assign it. (Request IDs are in the signature, so duplicates should be impossible.)
   c. **Prompt fallback:** parse the first `"request_id":<number>` from `run.prompt`; accept only if that request's `project_id == run.project_plan` and its signature matches. This recovers attempts whose pointer moved but whose prompt still records the request.
   d. Otherwise leave unowned. Do not guess.
3. On assignment set `planning_request_id`, `planning_revision = request.revision`, and `automation_id = request.automation_id` (only when the request is automation-owned).
4. Never overwrite a non-`None` owner, never delete a request or run, never rewrite input/expected/submitted time/output/logs.
5. Never infer ownership for an ordinary request; `automation_id` stays `None`.
6. Ambiguous runs remain normal visible history. The implementation should record in the test that "visible" means Research results for unowned legacy planning, while all *inferable* automation attempts are excluded and listed under the automation. This is the conservative interpretation of T23's "retain ambiguous history visibly rather than deleting it."

Migration does not require a SQLite schema/table change. Optional: bump `AUTOMATION_SCHEMA_VERSION` only if root wants a semantic marker; it is not required for serde-default fields, and it does not create a backup by itself. Recommendation: keep version 1 and rely on idempotent normalization, or coordinate a version bump with T15's migration work.

---

## 10. Exact implementation map

| File / function | Change |
| --- | --- |
| `src/domain.rs` `AgentRun` | Add `planning_request_id`, `planning_revision`, `automation_id` with serde defaults. Update all struct literals (request planning, project planning, research, fixtures, QA). |
| `src/domain.rs` `Store` | Add `run_is_automation_owned`, `run_is_ordinary_result`, `latest_ordinary_result_run`; use them in `progress_result_task_count`. |
| `src/request_planning.rs` `ResearchRequest` | Add `expected_result: Option<String>`, `revision: u32`. |
| `src/request_planning.rs` `create_automation_request` | Accept expected result; revision-aware reuse/new request; no mutation of existing content. |
| `src/request_planning.rs` `request_for_run` | Owner-aware with legacy pointer fallback. |
| `src/request_planning.rs` `request_plan_signature` | Conditional 7/8-tuple with expected result. |
| `src/request_planning.rs` `prepare_request_plan` | Set immutable owner fields; include revision/expected in context; owner-aware repair lookup. |
| `src/request_planning.rs` `apply_request_plan` | Keep ordinary request behavior; do not apply automation templates here. |
| `src/request_planning.rs` | Add `latest_planning_request(automation)`, `planning_attempts(automation)`, `normalize_planning_ownership()`. |
| `src/automation.rs` | Add `AutomationPlanBinding`, definition `plan_binding`, approval plan request/revision fields. |
| `src/automation_execution.rs` `apply_automation_template` | Accept request ID/revision; verify current revision/signature; set binding; clear approval. |
| `src/automation_execution.rs` `revise_automation_with_shape` | Clear `plan_binding` when plan/objective/expected/shape changes; preserve on name-only rename. |
| `src/automation_execution.rs` `approval_for` / `approval_current` | Validate current binding for new plans; keep legacy approvals valid. |
| `src/project_app.rs` `finalize_project_plan` | Owner-aware lookup; latest-revision/content/signature guard; stale preservation; pass binding to apply; success-only UI callback. |
| `src/automation_views.rs` `start_automation_planning` | Pass objective + expected; store new owner token; atomic pointer update; preserve draft on failure. |
| `src/automation_views.rs` `automation_wizard_next` | Detect changed objective/expected while in flight and start a new revision. |
| `src/automation_views.rs` `review_automation_planning` | Restore the in-flight attempt by immutable owner/latest request, not only the pointer. |
| `src/automation_views.rs` `automation_plan_ready` | Extend T09 token with revision/content/form-match checks; stay passive. |
| `src/automation_views.rs` `enable_automation` | Validate binding, expected, signature and approval; legacy fallback. |
| `src/automation_views.rs` `automation_settings_dialog` | Add "Plan versions" list; add `attempt_dialog` modal. |
| `src/navigation.rs` `displayed_run_id` | Use shared ordinary-result predicate and archived filter. |
| `src/run_views.rs` `run_is_automation`, `run_projects`, `runs`, `reveal_selected_run` | Delegate to Store; one fallback. |
| `src/app.rs` `navigate`, dispatch validity, duplicate guard | Use owner-aware ownership; no route change on completion. |
| `src/views.rs` | No new policy; verify breadcrumbs/count still derive from it. |
| `src/recovery.rs` planning section | Exclude/route automation planning; no ordinary result route. |
| `src/persistence.rs` | Call `normalize_planning_ownership()` on SQLite and JSON load paths. |
| `src/sqlite_storage.rs` | Map request signature tuple length 8 to directory index 5; add migration/relocation test. |
| `src/intake_views.rs` `request_history` | Keep excluding `automation_id`; optionally show "Latest attempt" from the owner-aware pointer. Do not include automation requests in ordinary history. |

No changes to `automation_schedule.rs`, `automation_activity.rs`, occurrence ordering, occurrence snapshots, retry commands, or `automation_policy.rs` beyond the T25 helper copy if root assigns it.

---

## 11. Exact tests

### 11.1 Domain / request planning (`src/request_planning.rs` tests)

1. `automation_replan_binds_objective_and_expected_revision`
   - Create automation with objective A, expected E_A.
   - Prepare/capture prompt; assert it contains A and E_A.
   - Create revision B with expected E_B; assert new request ID/revision, prompt contains B and E_B, does not contain A as the raw request.
   - Apply B; assert binding request/revision, definition version advanced, approval cleared.
2. `expected_only_edit_creates_new_revision_and_signature`
   - Same objective, E_A → E_B.
   - Assert new request ID/revision, signatures differ, prompt contains E_B, old run signature no longer matches.
3. `unchanged_automation_replan_reuses_request`
   - Same objective/expected twice → same request ID/revision; after a terminal run, prepare is allowed again and prompt is identical.
4. `run_owner_survives_pointer_move`
   - Prepare A, set pointer, prepare B, move pointer to B; A's `request_for_run` still resolves A via `planning_request_id`.
5. `legacy_request_without_expected_keeps_old_signature_shape`
   - Deserialize a request missing both new fields; assert `expected_result == None`, `revision == 1`, signature is the 7-tuple; `normalize_planning_ownership` does not invent expected text.
6. `latest_revision_guard_rejects_old_attempt`
   - Create A and B; verify only B is latest; apply A must fail/mark stale at the project finalize boundary (covered again in QA).

### 11.2 Automation domain (`src/automation_execution.rs` tests)

1. `apply_template_records_request_binding`
   - Apply a valid plan with request ID/revision; assert `plan_binding`, approval cleared, version behavior unchanged.
2. `apply_template_rejects_stale_or_foreign_request`
   - Wrong request ID, older revision, mismatched signature → error; definition unchanged.
3. `approval_requires_current_plan_binding`
   - New research template with binding → enable succeeds and approval records ID/revision.
   - Changed objective or expected-only without a new apply → enable fails with Replan guidance.
   - Legacy definition/template/approval with no request/binding still enables (compat guard).
4. `revise_clears_binding_but_rename_keeps_it`
   - Preserve T09/T04 lifecycle behavior.
5. Existing `legacy_single_goal_templates_still_load`, T03 retry tests, T18 expected-action tests must remain green.

### 11.3 Storage / migration (`src/sqlite_storage.rs`, `src/persistence.rs` tests)

1. `legacy_planning_ownership_is_inferred_without_rewriting_content`
   - Build a store, serialize it, remove `expected_result`, `revision`, `planning_request_id`, `planning_revision`, `automation_id`, `plan_binding`.
   - Save/load SQLite; assert request content/timestamps/output unchanged, latest pointer attempt assigned, older attempt assigned only when signature-unique/prompt-parseable, unrelated runs untouched.
2. `ambiguous_legacy_planning_remains_visible_and_unowned`
   - Construct a run with no pointer, no signature match, no parseable request ID; assert no owner is invented and the run remains in ordinary history.
3. `plan_signature_with_expected_result_survives_relocation`
   - Create an 8-tuple signature, relocate the workspace root, assert the project-directory index is rewritten and the expected-result field is preserved; compare stale behavior.
4. `planning_ownership_roundtrips_after_restart`
   - Three revisions/runs, save, load; assert all owners/revisions/statuses/outputs persist and normalization is idempotent on a second load.

### 11.4 Offline automation QA (`src/automation_qa.rs`)

Add to `run()` after the existing planning/late-completion checks, using isolated roots only.

1. `planning_revision_check(root)`
   - Fake planner CLI writes each received prompt to `prompt-<n>.txt`, then prints a valid `RequestPlan` whose topic name reflects the prompt.
   - Create Research automation A, run the wizard, wait for completion; read prompt file and assert A + expected A.
   - Open Replan, change to B and expected B, continue, wait; assert prompt contains B/E_B, not the old request text.
   - Assert Enable succeeds without another Replan and records the binding.
   - Enable, materialize/complete one period with a fake runner, then replan to B and assert the old occurrence snapshot still has A/E_A.
2. `planning_stale_completion_check(root)`
   - Prepare A with a saved successful `output`, then create revision B before completing A.
   - Call `complete_research_run(A)`; assert A becomes `Stale`, template/binding/version unchanged from B's state, B's draft remains.
   - Complete B; assert it applies and enables.
   - Reuse the T09 `late_planning_completion_check` pattern (`automation_qa.rs:3440`) and add the revision assertion. Ensure the old T09 assertions (no UI repopulation, no wizard jump, saved plan retained) still pass.
3. `planning_concurrent_edit_check(root)`
   - Start A, set the live form to B, call `automation_plan_ready`/completion, assert the plan is not presented as approved for B and the draft text is preserved.
   - Press Continue with B; assert a new revision/run and that A cannot become the binding.
4. `planning_failure_retry_check(root)`
   - Fake CLI exits nonzero on the first invocation, valid plan on the second.
   - Assert failure preserves B text/request, no template/binding/approval, and Retry captures B/E_B on the second prompt.
5. `planning_attempts_restart_check(root)`
   - Three attempts A/B/C, save storage, drop/reopen the app on the same SQLite file.
   - Click Research results: assert no `run-navigation` row for any automation planning run and the displayed run is ordinary.
   - Open Automation → Settings: assert "Plan versions" lists all three with correct revision/status; open one read-only attempt; assert breadcrumb is Automations/<name>, not All research.
6. `legacy_planning_migration_check(root)`
   - Use a fixture without new fields, load through Storage/attach, assert the app opens, inferable attempts are excluded from Research results and listed under the automation, ambiguous attempts remain visible in ordinary history, and no content was rewritten.
7. Preserve existing acceptance cases:
   - `late_planning_completion_check` (T09);
   - `completion_boundary_check` / `ordinary_dispatch_boundary_check` (T02);
   - `retry_scope_check` (T03);
   - `instruction_validation_check` (T18);
   - `activity_order_check` (T16).
   Add unique helper names and avoid editing T16's ordering assertions.

### 11.5 UI assertions (within QA above)

- Research results count/tree excludes all automation planning runs after two replans and restart.
- `displayed_run_id` content/title/breadcrumb match the same ordinary run after fallback.
- An automation attempt opens with Automation breadcrumb; no All research crumb.
- Enable after objective-only and expected-only edits is rejected until a matching plan is applied.
- Previous period result still shows A/E_A after B is approved.
- Ordinary request retry attempts remain visible in Saved research requests / Research results (no regression).

### 11.6 Commands

- `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet`
- `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo run --features qa --quiet -- --qa-automation`
- Relevant focused unit filters while iterating (e.g. `cargo test --quiet request_planning::`, `automation_execution::`, `sqlite_storage::`, `persistence::`).
- If any UI layout/copy changed, the relevant isolated screenshots at 1440×920 and 1000×680, light/dark. No icon work is expected unless the attempt dialog adds a status icon; if it does, follow `doc/ICON_GUIDELINES.md` and `python3 script/check_icons.py`.
- Do not run against the user's real workspace; use the QA root.
- Record exact exit codes and pre-existing failures (T01 gate still open).

---

## 12. Preservation and coordination

### Must preserve

- **T09:** `automation_ui.planning_run` + `planning_automation` + Research-shape late-completion guard; switching shape clears the token. Extend, do not replace.
- **T02:** completion/finalize never changes scope, selection, dialog, scroll or draft. Any new attempt dialog opens only from an explicit click.
- **T18:** all instruction fields validated; expected-result validation remains input validation, not a sandbox; helper copy must be truthful. T25 now makes expected result a planning input, so update the copy in the same change.
- **T03:** period retry uses the occurrence's original approved snapshot; new binding fields do not affect retry.
- **T04:** durable automation mutations invalidate whole-store Undo; adding binding/approval fields must not let Undo orphan them. Keep `definition_form_signature` derived-only and clear binding through domain commands.
- **T07:** frozen navigation behavior. Only change `displayed_run_id`/selection filtering; do not alter neighbor/period ownership logic.
- **T16:** do not touch `automation_activity.rs` or occurrence ordering. Put the attempt list in the settings dialog, not in period history.

### Ownership conflicts to expect

1. `src/automation_views.rs` and `src/automation_qa.rs` are shared with T16 (activity/list/history) and T01 (QA baseline). T23/T25 edits must be localized to planning functions and new checks, with unique helper names.
2. `src/navigation.rs` is T07-frozen but contains `displayed_run_id`. Coordinate a narrowly scoped selection-policy change; do not reopen T07 navigation semantics.
3. `src/automation_execution.rs` and `src/automation.rs` overlap T03/T18/T04 accepted behavior and T16's runtime files. Re-run their focused tests before and after.
4. `src/persistence.rs` / `src/sqlite_storage.rs` may overlap T15/T14 future work. Keep this change to serde defaults, load normalization, and the signature tuple mapping; do not introduce a new table or background writer.
5. `src/recovery.rs` planning section is an overlooked result-selection path; T01 may also touch recovery QA. Coordinate the assertion change.
6. `src/views.rs` breadcrumbs and counts are shared with T07/T12; change only the data source, not navigation structure.
7. `src/automation_policy.rs` is T18-owned; T25 should not modify it. Only the helper text in `automation_views.rs` changes.

---

## 13. Unknowns left to implementation verification

These are verification items, not reasons for more broad reading:

- Exact legacy fixtures in the current workspace: whether any real stored automation request has a pointer that no longer matches the run. The migration test must reproduce both pointer-present and pointer-lost histories from synthetic fixtures; do not inspect the user's live workspace.
- Whether `recovery.rs` currently imports a planning run's ownership fields on restart; verify after adding the fields and add a focused recovery assertion if it reconstructs runs.
- Whether any test constructors clone/store requests with `revision = 0`; normalize and assert `max(1, revision)`.
- Whether the T09 UI token needs a separate `planning_request` field or can derive it from `planning_run` + owner; either is acceptable if the late-completion test proves stale rejection.
- Whether root wants a schema-version bump for semantic traceability. The safe default is no bump plus idempotent normalization; T15 may later add a versioned migration/backup.
- Exact visual placement of "Plan versions" in the settings dialog; follow DESIGN's settings/approval hierarchy and keep the page's execution actions unchanged.

---

## 14. Acceptance checklist for the later implementation

- [ ] Every planning run persists an immutable request/revision/automation owner.
- [ ] Objective A→B and expected-only edits create a new revision; planner prompt and signature capture both.
- [ ] Repeated unchanged replan reuses the revision and still enables after review.
- [ ] Completion applies only the latest matching revision; old/in-flight responses become Stale without changing template, approval or periods.
- [ ] Failed planning preserves the editable draft and retries against the same revision.
- [ ] Enable requires a matching reviewed plan binding; legacy definitions without a request remain readable/enableable under their old contract.
- [ ] Previous periods retain their original objective/expected/template.
- [ ] Research results, displayed run, breadcrumbs, counts and history agree and exclude inferable automation planning attempts.
- [ ] Automation attempt list exposes all revisions/attempts, including after restart; ambiguous legacy runs stay visible but unassigned.
- [ ] T09 late-completion, T02 passive completion, T18 validation, T03 retry, T04 undo, T07 navigation and T16 ordering regressions remain green.
- [ ] Storage/save/reopen and old-schema fixtures pass; no user workspace or real research is touched.
- [ ] Root assigns coding scope and file ownership before any source edit; this audit remains the only artifact written by this session.

---

## 15. Stop point

Audit complete. No implementation was performed. Root should review this map, resolve the T16/T01/T18/T07 file coordination, and assign the T23/T25 coding scope before any source or maintained-document edit.
