# T21 audit — research-input invalidation, impact preview, historical results, Undo boundary

Date: 2026-11-08
Task: [T21](../../doc/review-2026-11-08/TASKS.md#t21--explain-and-control-research-input-invalidation)
Finding: F19
Mode: read-only bounded audit. No source, product doc, workspace, fixture, snapshot or application state was modified. This document is the only artifact written.
Related owners: T06 (typed result presentation), T22 (archive selector/UI), T13 (command/scoped undo), T10 (navigation/dirty guard), T11 (shared styles), T17 (documentation, handed back).

Line references are for the current working tree and may drift; function names are authoritative.

## 1. Outcome in one paragraph

The invalidation itself is deliberate and mostly graph-correct: `goal_plan_signature` covers the goal's title, description, comments, materials and project plus every active child's title, description, comments, materials, parent, archived flag, kind and dependencies, while `research_signature` additionally covers the ancestor context chain, project directory and dependency run IDs. Labels, priority, due date and cycle are already outside both signatures, so presentation-only edits already preserve completion. The defect is that Save/comment/material actions commit first and let `reconcile_goal_approvals` silently move the owning goal from Done/Planned to Backlog, clear `approved_plan`, and make every step result non-current. There is no pre-commit explanation or affected-step preview, no "previous version" entry on the task after invalidation, no historical export decision, and no ordinary-versus-automation edit boundary in the generic editor. T13's emerging scoped Undo currently records only task fields, so once the editor migrates to it, Undo will no longer restore approval the way today's whole-store undo does.

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

Comment, remove-material and add-material each trigger the same silent invalidation on autosave. Material addition is the worst case because files are copied first and attached immediately; a cancel/confirmation design must state what happens to copied files (today Undo leaves them on disk).

### D5 — Automation-owned research tasks are editable through the generic path (P1 boundary, code-confirmed)

The detail menu disables editing only for archived or running tasks (`src/views.rs` ~2495, ~2599). Automation period tasks have `automation_id = Some`, but `begin_draft`/`save_draft` do not check ownership. Editing one can change `goal_plan_signature` of an occurrence goal whose approval was recorded at period creation (`src/automation_execution.rs` ~936-944) and make the period's plan invalid. T13's `assert_ordinary_edit_preserved` would reject such an edit (`src/domain_command.rs` ~1313-1337), but the current editor bypasses that path. T21 must classify this scope and present it explicitly; the final enforcement policy belongs to T12/T23/T25, not a silent T21 behavior change.

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
- **Input revision:** the persisted `AgentRun.input_signature`. Display a short prefix plus run ID and timestamp. The goal-level `approved_plan` hash is cleared on invalidation and is not a durable revision history; do not invent one or add a schema field in this task. The run prompt itself is the retained record of what the old input looked like.
- **Original approved version / historical result:** T06/T22 labels. T21 must not reinterpret archive or typed presentation; it only supplies selection and impact metadata.
- **Undo:** restores the named edit and its approval consequence; it must not restore or remove runtime runs, logs, ended-at, or unrelated task activity. Scoped undo is T13-owned; T21 supplies the required approval-state contract and tests.

## 6. Which draft changes need a preview or gate

| Draft change | Signature/prompt effect | Preview | Gate before commit |
| --- | --- | --- | --- |
| Title | Yes (goal/step prompt + signature) | Show owning goal, current results becoming previous, renewed review | Required when at least one current result exists; otherwise label Save as "Save and review plan" |
| Description | Yes | Same as title | Same as title |
| Labels | No | Neutral "presentation only" statement | None |
| Due date / cycle | No | Same | None |
| Priority (property menu) | No | Same, if surfaced at all | None |
| Stage → Backlog/Cancelled | Clears approval by explicit lifecycle action | Explain approval is withdrawn | Existing stage confirmation; do not hide it inside Save |
| Stage → Todo/Planned | Renews approval for the edited graph | Explain review approval | Existing approval action |
| Project change | Yes (project ID + directory for every descendant) | List affected steps/project | Required when current results exist |
| Comment | Yes | List affected steps | Required when current results exist |
| Material add/remove | Yes | List affected steps, name the file and inheritance path | Required when current results exist; state that copied files are not deleted by cancel/Undo |
| Archived task | Editing already disabled | None | None |
| Automation-owned task | Changes an occurrence's approved plan | Explain ownership; no ordinary "review plan" flow | No generic save until T12/T23/T25 decides the policy |

A preview computed only after Save is pressed is acceptable only if it blocks the commit; a hover-only or transient toast is not acceptable (PRODUCT anti-references). The at-rest editor should still say that research inputs require review while labels/priority do not.

## 7. Proposed bounded implementation phases

### Phase 1 — Pure classification/impact/history slice (no owner races; implement first)

New file `src/research_input.rs`; add one module registration line in `src/main.rs`. Do not modify `result_presentation.rs`, `archive_presentation.rs`, `research_history_result`, `domain_command.rs`, navigation, or styles.

Suggested API:

```rust
pub(crate) enum ResearchScope { OrdinaryGoal(u64), OrdinaryStep(u64), OrdinaryTask, AutomationOwned, Archived }
pub(crate) enum StepDisposition { KeptCurrent, BecomesPrevious, AlreadyPrevious, GainsCurrent, NoResult }
pub(crate) struct StepImpact { pub id: u64, pub title: String, pub kind: ResearchKind, pub disposition: StepDisposition, pub current_run: Option<u64> }
pub(crate) struct ResearchInputImpact {
    pub scope: ResearchScope,
    pub goal: u64,
    pub goal_title: String,
    pub signature_changed: bool,
    pub approval_before: bool,
    pub approval_after: bool,
    pub reapproval_required: bool,
    pub becomes_historical: usize,
    pub steps: Vec<StepImpact>,
    pub other_goals_affected: Vec<u64>,
    pub presentation_only: bool,
    pub confirmation_required: bool,
}
pub(crate) struct TaskEditIntent { pub title: String, pub description: String, pub labels: Vec<String>, pub due_date: String, pub stage: Stage, pub project: u64 }
impl TaskEditIntent { pub(crate) fn from_task(store: &Store, id: u64) -> Option<Self>; }
pub(crate) enum MaterialChange<'a> { Add(&'a Attachment), Remove(usize) }
pub(crate) fn preview_task_edit(store: &Store, id: u64, intent: &TaskEditIntent) -> Result<Option<ResearchInputImpact>, String>;
pub(crate) fn preview_comment(store: &Store, id: u64, body: &str) -> Result<Option<ResearchInputImpact>, String>;
pub(crate) fn preview_material_change(store: &Store, id: u64, change: MaterialChange<'_>) -> Result<Option<ResearchInputImpact>, String>;
pub(crate) struct PreviousVersion { pub run_id: u64, pub started_at: String, pub input_revision: String, pub is_current: bool }
pub(crate) fn previous_versions(store: &Store, id: u64) -> Vec<PreviousVersion>;
```

Implementation rules:

- Return `None` for non-research tasks. Return `Archived`/`AutomationOwned` scopes without staging a generic edit.
- For ordinary research, compose the candidate store on a clone using existing domain APIs: `move_project`, `set_stage`, and T13's `Store::prepare_command` with `Command::Edit(EditCommand::TaskDetails { .. })`. That reuses T13's field validation, protected-state guard and reconcile instead of duplicating signature logic. Do not modify `domain_command.rs`.
- Derive `approval_before/after` with `goal_execution_approved`, `signature_changed` with `goal_plan_signature`, and per-step disposition with `research_result` before/after. Do not recompute or copy signature fields.
- Scan every research goal and assert/report `other_goals_affected`; normal edits must return empty. This is the graph-scope check, not an invalidate-all mechanism.
- `confirmation_required = becomes_historical > 0` (add comments/materials on an approved goal with current results). `presentation_only = !signature_changed && approval unchanged`.
- `previous_versions` reads runs directly (`task_id`, succeeded, non-empty, non-planning) newest first, marks the current run, and exposes only the persisted signature prefix. Do not use or change `research_history_result`.
- No UI strings in this module beyond stable reason identifiers; English UI copy belongs to phase 2/views.

Required Phase 1 tests (`src/research_input.rs` `#[cfg(test)]`):

1. `title_edit_previews_reapproval_and_every_goal_step`: completed three-step goal → title intent yields `reapproval_required`, 3 steps `BecomesPrevious`, `confirmation_required`, `other_goals_affected` empty, store unchanged.
2. `description_edit_matches_prompt_input`: same, and the preview agrees with `prepare_run` differences.
3. `labels_due_and_priority_are_presentation_only`: mutate labels/due/priority; run `reconcile_goal_approvals`; goal stays Done/approved, all results current, preview `presentation_only`.
4. `labels_and_due_change_does_not_change_prompt`: `prepare_run` prompt byte-identical before/after; title/description/comment/material variants differ. This directly prevents "remove titles from prompts to pass tests".
5. `comment_preview_is_scoped_to_one_goal`: two goals in one project; comment on goal A step → only A steps affected, B results stay current; commit simulation matches preview exactly.
6. `material_add_and_remove_preview_match_commit`: inherited goal material and step material both covered; old runs retained; reconcile moves goal to Backlog.
7. `stage_backlog_preview_clears_approval_without_fake_reapproval`: stage-only intent reports withdrawal, not renewed approval.
8. `no_op_edit_and_archived_and_automation_scopes`: no-op returns empty impact; archived returns `Archived`; automation-owned returns `AutomationOwned` and never a generic save preview.
9. `previous_versions_retain_old_revision`: after invalidation, old succeeded run is listed with its old signature prefix and `is_current == false`; after rerun, the new run is current and the old remains previous; no runs are removed.
10. `loaded_goal_approval_is_not_spuriously_revoked`: loading/unchanged store reconciliation does not alter approval (guards against signature drift).

Phase 1 is the only slice that should be dispatched before root authorization of UI work.

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
- Add material: after import completes, compute `preview_material_change(Add)` for the batch before attaching. Hold imported attachments in `pending_materials`; confirm attaches and persists once; cancel leaves imported files unreferenced and states that. Reuse the existing attachment error/status area; do not add a new file format.
- Keep all actions keyboard reachable; no hover-only reveal; warning tone from the shared notice style (T11).

### Phase 2c — Previous-version entry and historical export decision

Files: `src/views.rs` (task header/secondary action), `src/research_views.rs` only if a small context line is needed, `src/research_export.rs` only after T06 coordination.

- When `result_target` is None but `previous_versions` is non-empty, show a secondary "View previous version" action using the newest previous run and `show_task_result(run_id)`; label the panel with run ID, short input revision and "Previous version — produced before the latest input change". Reuse T06's freshness/quality output; do not modify `present_run` or `research_history_result`.
- Export recommendation: keep the goal ZIP current-only (its “all steps current” contract is sound), and add a single-report previous-version export/copy path through T06's reader transformation. Whole-goal historical ZIPs require a coherent revision per step and are out of the bounded scope; if root wants them, T22/T06 must define the revision grouping first. Explicit unknown below.

### Phase 3 — Ownership and cross-cutting input consistency (requires owner coordination)

- Automation-owned edit policy: either disable generic Edit task for `automation_id.is_some()` with a reason, or route to automation-period settings. Decide with T12/T23/T25; do not silently invalidate an occurrence plan.
- Working folder: before commit, compute affected goals/current results and preview; either revoke approval proactively for those goals or explicitly document that directory changes leave signatures stale without review. Do **not** add directory to `goal_plan_signature` casually: it would invalidate every stored approval on upgrade and needs a migration decision.
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

1. **Historical export shape.** Recommendation: single-report previous-version copy/export via T06; whole-goal historical ZIPs need a revision-grouping contract owned by T22/T06. Not decided here.
2. **Automation-owned edit policy.** Disable generic editing, or give it automation-revision semantics? T12/T23/T25 ownership; T21 should only classify and explain until then.
3. **Working-folder invalidation.** Proactively revoke approval for affected goals, or document signature-only staleness? Adding directory to `goal_plan_signature` is migration-sensitive; no decision made.
4. **Undo runtime-event reproduction.** D8 is code-derived, not executed. T13 must reproduce with its candidate and prove the scoped inverse preserves runtime events.
5. **Preview interaction.** This audit recommends compute-on-Save with a blocking impact dialog for destructive edits, plus at-rest explanatory copy, to avoid per-keystroke store cloning. Root may prefer an always-live preview; that would need a cheap draft-only signature diff.
6. **No native screenshots or QA runs** were produced in this read-only audit; visual and keyboard acceptance remain part of implementation.

## 10. Files intentionally not changed

`src/research.rs`, `src/domain_command.rs`, `src/result_presentation.rs`, `src/archive_presentation.rs`, `src/app.rs`, `src/views.rs`, `src/research_app.rs`, `src/research_export.rs`, `src/run_views.rs`, `PRODUCT.md`, `DESIGN.md`, `USER_GUIDE.md`, `doc/RESEARCH_WORKFLOW_STRATEGY.md`, all fixtures and screenshots.

No commits, pushes, workspace writes, frozen snapshots or production app access were performed.
