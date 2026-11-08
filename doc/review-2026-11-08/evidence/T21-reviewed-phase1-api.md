# T21 audit — research-input invalidation, impact preview, historical results, Undo boundary

Date: 2026-11-08
Task: [T21](../../doc/review-2026-11-08/TASKS.md#t21--explain-and-control-research-input-invalidation)
Finding: F19
Mode: read-only bounded audit. No source, product doc, workspace, fixture, snapshot or application state was modified. This document is the only artifact written.
Related owners: T06 (typed result presentation), T22 (archive selector/UI), T13 (command/scoped undo), T10 (navigation/dirty guard), T11 (shared styles), T17 (documentation, handed back).

Line references are for the current working tree and may drift; function names are authoritative.

## 1. Outcome in one paragraph

The invalidation itself is deliberate and mostly graph-correct: `goal_plan_signature` covers the goal's title, description, comments, materials and project plus every active child's title, description, comments, materials, parent, archived flag, kind and dependencies, while `research_signature` additionally covers the ancestor context chain, project directory and dependency run IDs. Labels, priority, due date and cycle are already outside both signatures, so presentation-only edits already preserve completion. The defect is that Save/comment/material actions commit first and let `reconcile_goal_approvals` silently move the owning goal from Done/Planned to Backlog, clear `approved_plan`, and make every step result non-current. There is no pre-commit explanation or affected-step preview, no "previous version" entry on the task after invalidation, no historical export decision, and no ordinary-versus-automation edit boundary in the generic editor. T13's emerging scoped Undo currently records only task fields, so once the editor migrates to it, Undo will no longer restore approval the way today's whole-store undo does.

## 1a. Root decisions incorporated

The Phase 1 implementation follows these reviewed decisions. Later phases still apply.

1. **Historical export.** Single-report viewing/copy/export through T06 is sufficient. The current-goal ZIP keeps its "all steps current" contract; no historical ZIP revision-grouping requirement.
2. **Automation period inputs are frozen.** Generic edit/comment/material mutations must not change a period snapshot. Phase 1 exposes `automation_edit_decision` so UI and a later domain-command boundary share one policy. Only labels, due date and cycle are metadata; title, description, project, stage, comment and material are refused with an instruction to edit the automation definition and replan future periods. There is no period-settings editor route.
3. **Working directory is execution context.** Phase 1 adds `preview_directory_change` and reports affected goals/results without changing the signature format or existing approvals on unchanged load. A later explicit directory-change command records/revokes affected approval; scoped Undo restores it only without conflicting edits or runtime changes.
4. **Two axes.** Goal review is separate from actual input delta. `review_required` blocks all execution; `graph_delta` reports only the changed step and its downstream. Unchanged siblings are `Unchanged` with matching retained inputs, never labeled changed solely because approval was withdrawn. No separate scheduler and no weakening of execution approval.
5. **Revision display.** Use a stable SHA-256 digest of the persisted `input_signature` plus run ID/time. Do not display a raw JSON prefix and do not rewrite stored runs.
6. **Pre-notification.** Compute-on-Save blocking preview plus persistent explanation is accepted. Comments/materials need the same pre-commit gate. Cancelled imported materials are a later cleanup concern; Phase 1 only reports impact.
7. **Undo.** The approval/stage restoration contract is sent to T13. `domain_command.rs` is not edited here.
8. **Temporal history order.** Result history uses the recorded attempt time with a deterministic persisted-ID tie-break, matching the accepted T16 presentation contract. A higher ID never means newer after clock rollback; legacy and out-of-order timestamps must be tested.
9. **Approval-impact confirmation is result-independent.** A pre-commit explanation is required whenever approval is withdrawn or the actual input graph changes, even with no successful result or only pre-existing stale history. Result counts qualify the message, not visibility.
10. **Cancelled material import cleanup.** Cancelling a held import removes only newly created, unreferenced copies owned by the app. It never deletes originals, user-managed files or shared/referenced copies.

## 2. Authoritative input map

`Store::prepare_run` is the prompt truth (`src/research.rs`, `prepare_run`, ~903-928):

| Prompt section | Source |
| --- | --- |
| Research goal | `goal.title` |
| Goal context | `goal.description` |
| Current task | `task.title` |
| Instructions | `task.description`, or a fixed planning instruction for a Goal |
| Project directory | owning project `directory` |
| User comments | `goal.comments` + `task.comments` |
| Materials | ancestor + dependency attachments added by `prepare_run_materials` (`src/research_app.rs` ~439-517) |
| Dependency context | validated prior run IDs and outputs |

`goal_plan_signature` (`src/research.rs` ~393-400) hashes the goal `(title, description, comments, attachments, project_id)` and each non-goal descendant `(id, title, description, comments, attachments, parent_id, archived, (kind, depends_on))`. `research_signature` (`src/research.rs` ~632-693) hashes each ancestor's `(id, title, description, comments, attachments, project_id)`, the project directory, the step kind/goal, and dependency result IDs.

Consequences:

- **Execution input:** goal/step title, description, comments, attachments, project/directory, parent, dependency graph, archived state, a Goal's own plan content.
- **Presentation only (already):** labels, priority, due date, cycle. They are absent from both signatures and from the prompt. No approval or completion change occurs today.
- **Approval scope:** one research goal and its whole active step graph. A change to any step's input changes `goal_plan_signature` and therefore revokes approval for every step in that goal. That is the current approved-graph contract, not a bug by itself.
- **Other goals:** their signatures do not include another goal's content, so a correctly scoped preview must report zero affected results in other goals. Tests must assert this rather than assume it.

## 3. Current invalidation flow (what actually happens)

1. `TaskApp::begin_draft` copies the task into the editor (`src/app.rs` ~958-997). The editor exposes Title, Description, Stage, Project and Labels (`src/views.rs`, `editor_overlay`, ~3481). Priority, due date and cycle are edited outside this overlay.
2. `TaskApp::save_draft` (`src/app.rs` ~998-1060) snapshots, calls `move_project` and `set_stage`, mutates title/description/labels/due directly on the live store, appends "Task details updated", closes the editor, then calls `confirm_save`.
3. `confirm_save` → `persist_if_changed` (`src/app.rs` ~711-726, ~1204-1244). `persist_if_changed` first calls `self.store.reconcile_goal_approvals()`.
4. `reconcile_goal_approvals` (`src/research.rs` ~407-415) finds every approved Goal whose `approved_plan` no longer equals `goal_plan_signature`, sets `approved_plan = None`, sets `stage = Backlog`, and appends "Research plan changed · review before execution". The reverted state is what is saved.
5. `goal_execution_approved` now returns false, so every `research_result` in the goal returns None. `goal_tasks_complete` becomes false, Export ZIP disables, each step's `StepState` falls back to Waiting (`src/goal_steps.rs` ~25-62), and the goal reads Backlog / Review plan.
6. Comments post directly (`src/views.rs`, `post-comment`, ~2802-2813); material removal mutates directly (`src/research_app.rs`, `remove_attachment`, ~308-320); material addition attaches immediately after import (`poll_research_io`, ~333-384). All rely on the same autosave reconciliation, and all show a success toast before the user learns about the consequence.

The stored runs are not deleted. `AgentRun.input_signature` and `AgentRun.prompt` survive, so historical viewing is technically possible; the missing parts are presentation, route and explanation.

## 4. Concrete defects

### D1 — No Save-impact preview before commit (P2, F19 direct)

`save_draft` mutates the live store and only `persist_if_changed` invalidates. The overlay shows a neutral "Save changes" button and no statement that approval/current-result eligibility will change. This is the exact native reproduction in F19.

### D2 — Invalidation cause and affected steps are never presented (P2)

`reconcile_goal_approvals` records one generic activity line and does not carry which fields changed, which goal was affected, or how many current results became previous. Because the mutation happens during autosave, a background save can invalidate without any edit-specific surface. Any fix must preview from the draft, not from this post-save hook.

### D3 — After invalidation the task loses its route to the previous report (P2)

`detail` computes `result_target` from `research_result` (`src/views.rs` ~2463-2471). Once approval is revoked, no step qualifies, so "View latest result" / "View final report" disappears. The runs remain reachable only by navigating to Research results and finding the task history, or by an existing version picker (`src/run_views.rs`, `run_version_picker`, ~586-650) after a result is open. The goal detail never says "Previous version" or shows the input revision, so acceptance "old reports remain viewable under their original revision" is not met from the place where the invalidation happened.

### D4 — Comments and materials have no preview and no pre-commit gate (P2)

Comment, remove-material and add-material each trigger the same silent invalidation on autosave. Material addition is the worst case because files are copied first and attached immediately. A cancelled held import must remove only the newly created, unreferenced copies owned by the app; originals, user-managed files and shared/referenced copies are never deleted.

### D5 — Automation-owned research tasks are editable through the generic path (P1 boundary, code-confirmed)

The detail menu disables editing only for archived or running tasks (`src/views.rs` ~2495, ~2599). Automation period tasks have `automation_id = Some`, but `begin_draft`/`save_draft` do not check ownership. Editing one can change `goal_plan_signature` of an occurrence goal whose approval was recorded at period creation (`src/automation_execution.rs` ~936-944) and make the period's plan invalid. T13's `assert_ordinary_edit_preserved` would reject such an edit (`src/domain_command.rs` ~1313-1337), but the current editor bypasses that path. T21 classifies this scope in Phase 1 (`AutomationEditDecision`) and must enforce it at the UI and command boundary with T13. Frozen period inputs are refused with the definition/replan explanation; metadata-only edits remain allowed. No period-settings route is introduced.

### D6 — Working-folder changes invalidate results without revoking approval (P2, cross-cutting input path)

`save_directory` (`src/app.rs` ~1326-1351) writes `project.directory` directly and persists. The directory is in `research_signature` but not in `goal_plan_signature`, so every step result in that project becomes non-current while each goal keeps `approved_plan` and its Done/Planned stage. There is no review prompt, no affected-goal count, and the goal can read Done while `goal_tasks_complete` is false. This is a real invalidation path outside the edit screen and must be either previewed or explicitly declared out of T21 with a tracked follow-up.

### D7 — T13 scoped `TaskFields` Undo will not restore approval (P1 cross-owner regression risk)

`UndoEntry::TaskFields` records only the five task fields (`src/domain_command.rs` ~123-137). Forward preparation applies fields and then `reconcile_goal_approvals` (~852-858), which clears the goal's `approved_plan` and moves it to Backlog. The inverse restores only fields (`apply_inverse`, ~1074-1104); it does not carry the goal's prior stage/approval, so the goal stays Backlog and the completed results stay non-current. Today's whole-store undo (`src/app.rs` ~1062-1119) does restore them because the snapshot contains the goal. Migrating the editor without extending the undo entry would regress the F19 native "Undo restores completion" behavior.

### D8 — Whole-store Undo can revert runtime task events while preserving run records (P2, T13-owned)

`snapshot` stores the whole `Store` before the edit (`src/app.rs` ~678). `undo_change` preserves `runs`, `publications`, `report_publishing` and `next_id`, but restores all task fields/stages/activity from the snapshot (~1086-1091). Run completion writes task stage/activity through `set_stage` (`src/research_app.rs` ~607-609; `src/domain.rs` ~1148-1161). Undo is blocked while runs are active, but not after they finish. The semantic hazard is code-confirmed: a snapshot that predates a completed run can restore an older stage/activity while keeping the completed run. This audit did not execute a native reproduction; T13 must prove or disprove it in its scoped-undo tests.

### D9 — Documentation contradicts the actual affected graph (P3)

`USER_GUIDE.md` ~207 says changing materials marks the old results of "that task and of everything downstream" as needing refresh and that finished upstream collection is unaffected. The implementation revokes the whole goal approval, so sibling results also stop counting as current (confirmed by `research.rs` tests `modified_materials_and_new_dependency_runs_invalidate_downstream_results` and `comments_and_materials_are_part_of_the_approved_plan`). The guide is materially wrong and must be corrected by T21's doc handoff.

### D10 — No regression guard for presentation-only edits or graph scope (P3)

There is no test that labels/priority/due/cycle leave approval and results unchanged, no test that an edit affects only the owning goal, and no test that the preview's classification agrees with the actual prompt. Those are the highest-value additions.

## 5. Stale / current / previous version semantics

Use existing contracts; do not add a second truth store.

- **Current result:** `Store::research_result(task)` — newest succeeded, non-empty, quality-valid run whose `input_signature` equals `research_signature(task)` and whose goal is approved/unarchived. This is the only result that may feed a new run, export, publish or completion count.
- **Previous version:** a succeeded, non-empty, historically valid run for the same task whose signature is not the current one, or any retained successful run when the current signature is unavailable because approval was revoked. Viewing is allowed; it must never be used as a dependency, export source for a current ZIP, or completion evidence.
- **Stale:** `result_presentation::present_run` already derives `Stale input` when the task is live and the current signature is None or differs (`src/result_presentation.rs` ~315-347). T21 should consume that label, not redefine it.
- **Input revision:** the persisted `AgentRun.input_signature`. Display a stable SHA-256 digest of the persisted signature plus run ID and timestamp. The goal-level `approved_plan` hash is cleared on invalidation and is not a durable revision history; do not invent one or add a schema field in this task. The run prompt itself is the retained record of what the old input looked like.
- **Original approved version / historical result:** T06/T22 labels. T21 must not reinterpret archive or typed presentation; it only supplies selection and impact metadata.
- **Undo:** restores the named edit and its approval consequence; it must not restore or remove runtime runs, logs, ended-at, or unrelated task activity. Scoped undo is T13-owned; T21 supplies the required approval-state contract and tests.

## 6. Which draft changes need a preview or gate

| Draft change | Signature/prompt effect | Preview | Gate before commit |
| --- | --- | --- | --- |
| Title | Yes (goal/step prompt + signature) | Show owning goal, changed/affected graph and renewed review | Always when approval is lost or inputs change, including before the first run |
| Description | Yes | Same as title | Same as title |
| Labels | No | Neutral "presentation only" statement | None |
| Due date / cycle | No | Same | None |
| Priority (property menu) | No | Same, if surfaced at all | None |
| Stage → Backlog/Cancelled | Clears approval by explicit lifecycle action | Explain approval is withdrawn | Existing stage confirmation; do not hide it inside Save |
| Stage → Todo/Planned | Renews approval for the edited graph | Explain review approval | Existing approval action |
| Project change | Yes (project ID + directory for every descendant) | List affected steps/project | Always when approval is lost or inputs change |
| Comment | Yes | List affected steps | Always when approval is lost or inputs change |
| Material add/remove | Yes | List affected steps, name the file and inheritance path | Always when the input graph changes; on cancel remove only newly imported unreferenced owned copies |
| Archived task | Editing already disabled | None | None |
| Automation-owned task: title/description/project/stage | Changes an occurrence's approved plan | Explain frozen period input; direct to editing the automation definition and replanning future periods | Refuse generic save (Phase 1 decision helper; Phase 3 UI/domain enforcement) |
| Automation-owned task: labels/due/cycle | No frozen input change | Metadata-only explanation | Allowed; frozen snapshot untouched |

A preview computed only after Save is pressed is acceptable only if it blocks the commit; a hover-only or transient toast is not acceptable (PRODUCT anti-references). The at-rest editor should still say that research inputs require review while labels/priority do not.

## 7. Proposed bounded implementation phases

### Phase 1 — Implemented pure classification/impact/history slice

Files changed: `src/research_input.rs` (new) and one registration line in `src/main.rs`. No other source or product doc was touched; `domain_command.rs`, T06/T22 presentation, navigation and styles are untouched.

Implemented API (`pub(crate)`):

```rust
enum ResearchScope { OrdinaryGoal(u64), OrdinaryStep(u64), OrdinaryTask, AutomationOwned, Archived }
enum StepInputState { Unchanged, Changed, DownstreamAffected }
struct StepImpact {
    id, title, kind, input_state,
    current_run_before: Option<u64>,
    inputs_match_retained_run: bool,
    retained_run: Option<u64>,
}
struct ResearchInputImpact {
    scope, changed, goal, goal_title,
    signature_changed, approval_before, approval_after,
    review_required, approval_already_pending,
    current_results, review_blocks_results,
    input_changed_results, retained_matching_results, pre_existing_stale_results,
    changed_input_steps, affected_input_steps, steps,
    other_goals_affected,
    presentation_only, confirmation_required,
}
struct TaskEditIntent { title, description, labels, due_date, cycle, stage, project }
enum FrozenChange<'a> { Edit(&'a TaskEditIntent), Comment, Material }
enum AutomationEditDecision { NotAutomationOwned, MetadataOnly, FrozenInput { fields, message } }
enum MaterialChange<'a> { Add(&'a Attachment), Remove(usize) }
enum RetainedQuality { NotEvaluated }
struct PreviousVersion { run_id, started_at, revision_digest, execution_admissible, inputs_match_current, quality }

preview_task_edit(store, id, &TaskEditIntent) -> Result<Option<ResearchInputImpact>, String>
preview_comment(store, id, body) -> Result<Option<ResearchInputImpact>, String>
preview_material_change(store, id, MaterialChange) -> Result<Option<ResearchInputImpact>, String>
preview_directory_change(store, project, new_directory) -> Result<DirectoryChangeImpact, String>
automation_edit_decision(store, id, FrozenChange) -> AutomationEditDecision
previous_versions(store, id) -> Vec<PreviousVersion>
revision_digest(persisted_signature) -> Option<String>
```

Implementation rules actually used:

- Candidate edits are staged on a clone; the live store is never mutated by any preview. `preview_task_edit` mirrors the editor: project changes are rejected with the existing topic-move message; stage and fields are applied on the clone; `reconcile_goal_approvals` runs on the clone so `approval_after` matches the real save.
- Input delta and result availability are separate. `graph_delta` compares field-level goal/step input (title, description, comments, attachments, parent, project, archived, kind, dependencies) and propagates through `depends_on`. It does not duplicate the signature formula. Shared signatures remain the only mechanism for matching retained runs, forced onto a preview clone so approval loss does not masquerade as input change.
- `review_required` means the owning goal loses approval and all execution waits. `changed_input_steps` is the direct change; `affected_input_steps` adds downstream. Unchanged siblings are `Unchanged`; siblings with retained runs are counted in `retained_matching_results` only when their persisted input still matches. Pre-existing stale results are counted separately and are not attributed to the edit.
- Directory preview uses `graph_delta(..., context_changed = true)` and a `BTreeSet` of step IDs, so each step contributes exactly once. It reports changed/affected steps and review even before any run exists.
- Automation policy: `FrozenInput` for title, description, project, stage, comment and material; `MetadataOnly` for labels, due date and cycle. The constant message tells the user to edit the automation definition and replan future periods. No nonexistent period editor is referenced.
- Result-history order uses `run_recorded_at`/`compare_runs_newest_first`/`run_activity_order_key`: recorded `started_at` (then `queued_at`) in the two supported persisted formats, known times first, deterministic run-ID tie-break only. Legacy missing/invalid timestamps sort last; no ID=newest inference. `latest_successful_run` uses the same key.
- `confirmation_required = review_required || !affected_input_steps.is_empty()`. Result counts no longer gate visibility, so an approved never-run input edit and an all-preexisting-stale history edit still show the impact explanation.
- `previous_versions` is read-only, newest first, and exposes `revision_digest` (SHA-256 over `input_signature`, 16 hex characters), `execution_admissible` (the existing `research_result` admission contract), `inputs_match_current` (input equality independent of approval) and `quality: NotEvaluated`. T06 still owns reader validation and freshness labels; no successful non-empty run is claimed as a validated report here.
- No new signature format, schema, scheduler, approval weakening or historical ZIP grouping.

Phase 1 tests (all in `src/research_input.rs`):

1. `title_edit_previews_approval_withdrawal_and_the_actual_input_graph` — branched DAG plus a separate goal; only the changed step is direct, its downstream is affected, the sibling branch is retained, the other goal is untouched, store unchanged.
2. `goal_title_edit_changes_every_descendant_but_not_another_goal` — ancestor edit marks all descendants, separate goal keeps approval and results.
3. `labels_due_date_cycle_and_priority_are_presentation_only` — approval, stage and current results survive; preview is presentation-only.
4. `metadata_and_noop_edits_leave_prompts_byte_identical` — `prepare_run` prompt is unchanged for metadata; title, description and comment changes alter it. No title is removed from the prompt.
5. `comment_preview_is_scoped_to_the_changed_branch` — only changed step plus downstream; sibling retained.
6. `material_add_and_remove_preview_match_the_committed_invalidation` — add and remove both covered; live attachments untouched.
7. `stage_withdrawal_blocks_execution_without_claiming_input_changes` — approval withdrawal with empty input delta.
8. `automation_period_inputs_are_frozen_and_metadata_is_explicit` — frozen fields and message; metadata-only path; no live mutation.
9. `archived_edits_and_history_are_not_ordinary_previews` — ancestor archive blocks preview and history selection.
10. `previous_versions_keep_old_revision_and_matching_siblings` — digest stability and length, old run not execution-admissible and inputs not matching; unchanged sibling retains matching inputs; runs not rewritten.
11. `directory_change_preview_is_scoped_to_the_project_and_does_not_mutate` — one goal per project, exact counts, unchanged directory no-op, automation project refused.
12. `unchanged_load_and_metadata_reconciliation_keep_saved_approvals` — repeated reconciliation does not revoke approval.
13. `directory_preview_without_any_run_still_reports_changed_inputs_and_review` — changed/affected steps and review before the first run.
14. `pre_existing_missing_upstream_is_not_attributed_to_a_metadata_edit` — metadata edit reports pre-existing stale results separately and no affected graph; later upstream edit reports the actual changed/affected graph.
15. `retained_versions_expose_input_match_and_quality_as_separate_axes` — execution admission, input match and `NotEvaluated` quality are distinct.
16. `input_edit_without_any_retained_result_still_requires_confirmation` — approved never-run input edit loses approval and still gets the blocking preview.
17. `pending_review_with_no_history_still_requires_the_input_graph_preview` — already-pending goal with no history still previews the changed graph.
18. `run_history_uses_recorded_time_then_id_with_legacy_and_clock_rollback` — recorded time primary, deterministic ID tie-break, legacy missing timestamps last, clock rollback not reinterpreted as newest by ID; `retained_run` follows the same order.

Evidence (2026-11-08, current working tree):

- `cargo test --quiet research_input::` → 18 passed, 0 failed.
- `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet` → 486 passed, 0 failed, 2 ignored. Without Node 24 on PATH the pre-existing `report_git` publishing test fails for the missing runtime, not for this module.
- `cargo test --quiet --no-run` → full default compilation succeeds; warnings only, no errors. QA-feature compilation is currently blocked by unrelated in-progress `archive_qa.rs`/`command_qa.rs` edits from other owners (`label`/`row`/`typed_intent_checks` missing), not by `research_input.rs`.
- The earlier root gate failure (`root-visual-result-acceptance-unit.log`, 11 errors) was captured mid-migration while callers/tests still used the removed `Matches`/`AffectedByUpstream`/`NoResult`/`is_current` names. Those errors are gone.
- Current status note (22:18): focused `research_input::` passes 18/18 and the full default gate passes 486/486 (2 ignored) after the unrelated `command_app.rs`/`navigation_context_app.rs` edits settled. QA-feature compilation remains temporarily blocked by other owners' `archive_qa.rs`/`command_qa.rs` work; re-run the QA compile after they finish. Do not treat this as T21 acceptance.

Phase 1 is complete and ready for review. Phase 2 UI and the later directory/automation command boundaries remain required and coordinated.

### Phase 2a — Edit overlay preview and Save gate

Files: `src/views.rs`, `src/app.rs` (state + Save orchestration). Do not change `draft_exit` semantics without T10 review.

- Add persistent editor copy near the actions: research inputs (title, description, project) require review; labels/due/cycle do not.
- On `save-task`, call `research_input::preview_task_edit` with the draft intent. If `confirmation_required`, open a modal "Review save impact" listing goal, affected steps, current→previous count, and the renewed-review requirement. Primary "Save and request review", secondary "Cancel" (focused first for destructive confirmation). Escape cancels. If not destructive but approval will be revoked, change the button label to "Save and review plan" and save once with the inline explanation.
- Never auto-approve. After Save, the goal is Backlog/Review plan; the existing approval action remains the only renewal path.
- Clear the preview state on navigation/cancel; T10 must treat the modal as dirty-guard-protected.
- Exclude automation-owned tasks from this flow with an explicit ownership explanation, pending the phase 3 policy.

### Phase 2b — Comment and material gates

Files: `src/views.rs` (comment), `src/research_app.rs` (materials), `src/app.rs` (pending state), possibly `src/views.rs` overlay rendering only.

- Comment: compute `preview_comment` on click; destructive impact opens the same impact dialog; confirm calls `add_comment`.
- Remove material: compute `preview_material_change(Remove)`; destructive impact gates the existing danger action.
- Add material: after import completes, compute `preview_material_change(Add)` for the batch before attaching. Hold imported attachments in `pending_materials`; confirm attaches and persists once. Cancel must delete only the newly imported copies created by this pending batch when they are unreferenced and app-owned; never delete originals, user-managed files or copies referenced by another task/run. State this in the confirmation. Reuse the existing attachment error/status area; do not add a new file format.
- Keep all actions keyboard reachable; no hover-only reveal; warning tone from the shared notice style (T11).

### Phase 2c — Previous-version entry and historical export decision

Files: `src/views.rs` (task header/secondary action), `src/research_views.rs` only if a small context line is needed, `src/research_export.rs` only after T06 coordination.

- When `result_target` is None but `previous_versions` is non-empty, show a secondary "View previous version" action using the newest previous run and `show_task_result(run_id)`; label the panel with run ID, revision digest and "Previous version — produced before the latest input change". Reuse T06's freshness/quality output; do not modify `present_run` or `research_history_result`.
- Export recommendation: keep the goal ZIP current-only (its “all steps current” contract is sound), and add a single-report previous-version export/copy path through T06's reader transformation. Whole-goal historical ZIPs require a coherent revision per step and are out of the bounded scope; if root wants them, T22/T06 must define the revision grouping first. Explicit unknown below.

### Phase 3 — Ownership and cross-cutting input consistency (requires owner coordination)

- Automation-owned inputs: enforce the Phase 1 `AutomationEditDecision` at both the editor and the command boundary. Frozen edits name the definition/replan path for future periods; metadata-only edits remain allowed. Do not add a period-settings route.
- Working folder: use `preview_directory_change` before commit, then an explicit directory-change command revokes only affected goal approvals and records the before/after consequence for scoped Undo. Do not add directory to `goal_plan_signature`; unchanged load keeps every saved approval.
- T13 Undo: extend `UndoEntry::TaskFields` (and future comment/material entries) with the owning goal's `(stage, approved_plan)` before/after, include it in `matches_after`, restore it in `apply_inverse`, and let the existing reconcile verify rather than overwrite. Add conflict handling if approval changed for another reason. This is T13-owned code; T21 supplies the contract and tests.
- Runtime-event preservation: T13 scoped undo must touch only the named fields plus the recorded approval snapshot; runs, logs, ended-at, unrelated activity and other tasks must remain byte-identical. Add a test that completes a run between edit and undo.

### Phase 4 — Documentation and QA handoff

- `doc/RESEARCH_WORKFLOW_STRATEGY.md`: add a short "Research input revisions and edit impact" section: signature/prompt inputs, presentation-only fields, whole-goal approval scope, preview/confirmation rules, current/previous/stale semantics, historical viewing/export limits, automation-owned boundary, Undo contract.
- `USER_GUIDE.md`: correct the "only that task and downstream" material wording to the actual whole-goal approval scope; add the edit preview/previous-version behavior.
- `PRODUCT.md`: one workflow sentence that input edits preview affected completed results and require renewed review, while labels/priority preserve completion.
- `DESIGN.md`: at-rest impact notice placement, destructive confirmation, previous-version entry, and the rule that the preview is never hover-only.
- `doc/QA_COMMANDS.md` and a new isolated QA entry (for example `--qa-research-inputs`) covering title-only, description, labels, due/priority, comment, material add/remove, graph scoping, previous version, Undo, automation-owned and archived boundaries at 1440×920 and 1000×680 in both themes. No VoiceOver/screen-reader scope; keyboard and focus remain required.

## 8. Verification plan

- Phase 1: `cargo test --quiet research_input::`, plus `cargo test --quiet research::` and `cargo test --quiet domain_command::` to prove no cross-module regression.
- Phase 2: focused unit/QA plus `cargo run --features qa -- --qa-research` and the new `--qa-research-inputs` in an isolated PID-scoped workspace. Do not touch the user's real workspace.
- Icons: if a History/previous-version icon is added, run `python3 script/check_icons.py` and confirm the asset is embedded in `src/ui_assets.rs`.
- Final: `cargo test --quiet`, the relevant QA suites, light/dark screenshots, and a native keyboard pass. No app restart of the user's running instance unless requested.

## 9. Explicit unknowns / decisions for root

1. **Historical export shape.** Resolved: single-report previous-version viewing/copy/export through T06; current-goal ZIP stays current-only.
2. **Automation-owned edit policy.** Resolved: Phase 1 policy helper; UI and domain enforcement follow in Phase 3 with T13, without a period-settings route.
3. **Working-folder invalidation.** Resolved: preview now, explicit command later; no signature-format change.
4. **Undo runtime-event reproduction.** D8 is code-derived, not executed. T13 must reproduce with its candidate and prove the scoped inverse preserves runtime events.
5. **Preview interaction.** This audit recommends compute-on-Save with a blocking impact dialog for destructive edits, plus at-rest explanatory copy, to avoid per-keystroke store cloning. Root may prefer an always-live preview; that would need a cheap draft-only signature diff.
6. **No native screenshots or QA runs** were produced in this read-only audit; visual and keyboard acceptance remain part of implementation.

## 10. Files intentionally not changed

`src/research.rs`, `src/domain_command.rs`, `src/result_presentation.rs`, `src/archive_presentation.rs`, `src/app.rs`, `src/views.rs`, `src/research_app.rs`, `src/research_export.rs`, `src/run_views.rs`, `PRODUCT.md`, `DESIGN.md`, `USER_GUIDE.md`, `doc/RESEARCH_WORKFLOW_STRATEGY.md`, all fixtures and screenshots.

No commits, pushes, workspace writes, frozen snapshots or production app access were performed.
