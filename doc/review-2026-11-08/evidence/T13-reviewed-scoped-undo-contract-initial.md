# T13 S3/S4 scoped Undo command contract

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Mode: design/preparation only. No source files were modified. This is the contract for the already-required scoped Undo migration; it is not a new architecture expansion and is not acceptance.

Current integration context: T10 `save_draft` is a staged `bool` path; T21 provides `request_save_draft` / `confirm_research_save` impact wrappers; T12 live routes are not migrated. T04 remains pending the full gate.

---

## 1. Invariants

1. Entity state, execution input, approval, and runtime history remain independent dimensions.
2. A scoped inverse writes only the fields named by its command. Unrelated fields, comments, activity, logs and run history survive even when the inverse targets the same task/goal.
3. Undo never rewrites `runs`, `logs`, `started_at`, `ended_at`, `automation_occurrences`, schedule cursors, request ownership, `next_id`, publications or report-publishing history.
4. Execution-input undo restores approval only when:
   - the current goal/task still matches the recorded `after` state, and
   - the approved revision/input signature still matches the recorded revision, and
   - no runtime event (new attempt, completion, recovery, cursor movement) occurred after the recorded runtime stamp.
5. Any mismatch returns a conflict that retains an actionable edit intent. It never partially applies and never silently discards the user's draft.
6. Frozen automation-period execution inputs are rejected inside the domain command, not only by UI prechecks. Metadata-only edits remain allowed under T21's existing `MetadataOnly` policy.
7. A durable command reports Saved only after one successful storage receipt. Preview confirmation and domain validation commit atomically; a stale confirmation cannot save.
8. Material cleanup deletes only newly imported, app-owned copies under the workspace attachment root that are unreferenced. It never deletes originals or shared paths.
9. No additional whole-`Store` snapshot may be introduced. `Vec<Store>` is removed only after every snapshot call site is migrated.
10. T14/T15 consume a precise entity change set; S3 does not implement the async worker.

---

## 2. Command and inverse types

### 2.1 Edit commands

```rust
pub(crate) enum EditCommand {
    TaskFields {
        id: u64,
        changes: Vec<TaskFieldChange>,
    },
    TaskStage {
        id: u64,
        stage: Stage,
    },
    TaskComment {
        id: u64,
        body: String,
    },
    TaskMaterials {
        id: u64,
        change: MaterialChange,
    },
    ProjectName {
        id: u64,
        name: String,
    },
    ProjectDirectory {
        id: u64,
        directory: String,
        impact: ConfirmedDirectoryImpact,
    },
    ProjectCreate {
        name: String,
        directory: String,
    },
    TopicCreate {
        project: u64,
        name: String,
    },
    TopicRename {
        id: u64,
        name: String,
    },
    TopicArchive {
        id: u64,
        archived: bool,
    },
    TopicMove {
        id: u64,
        project: u64,
    },
    GoalTopic {
        id: u64,
        topic: Option<u64>,
    },
    TaskArchive {
        id: u64,
        archived: bool,
    },
    StepReopen {
        id: u64,
    },
    RestoreArchived {
        owners: Vec<ArchivedOwner>,
    },
    Settings(SettingsEdit),
}

pub(crate) enum TaskField {
    Title,
    Description,
    Labels,
    DueDate,
    Cycle,
    Priority,
}

pub(crate) struct TaskFieldChange {
    pub field: TaskField,
    pub before: TaskFieldValue,
    pub after: TaskFieldValue,
}
```

`TaskFields` carries only fields actually changed by this command. `title`/`description` are execution inputs; `labels`/`due_date`/`cycle` are metadata under T21. `priority` follows current product semantics and must not affect prompts; if it does, it joins the execution-input set.

### 2.2 Approval companion

```rust
pub(crate) struct GoalApprovalCompanion {
    pub goal: u64,
    pub before: GoalApprovalState,
    pub after: GoalApprovalState,
    pub runtime: GoalRuntimeStamp,
}

pub(crate) struct GoalApprovalState {
    pub stage: Stage,
    pub approved_plan: Option<String>,
    pub signature: Option<String>,
    pub requires_approval: bool,
}

pub(crate) struct GoalRuntimeStamp {
    pub max_run_id: u64,
    pub active_run_ids: Vec<u64>,
    pub occurrence_ids: Vec<u64>,
}
```

Capture the companion for title/description/material/comment/directory/stage edits whenever T21's preview says the owning goal's approval or execution graph changes. `TaskFields`/`TaskStage`/`TaskMaterials`/`ProjectDirectory` carry `Option<GoalApprovalCompanion>` (not duplicated for multiple goals; a command may carry `Vec<GoalApprovalCompanion>` when dependencies span goals).

A field-only edit that T21 classifies `presentation_only` carries no approval companion and cannot alter approval.

### 2.3 Scoped inverse entries

```rust
pub(crate) enum UndoEntry {
    TaskFields {
        id: u64,
        changes: Vec<TaskFieldChange>,
        approval: Vec<GoalApprovalCompanion>,
    },
    TaskStage {
        id: u64,
        before: Stage,
        after: Stage,
        approval: Vec<GoalApprovalCompanion>,
    },
    Comment {
        id: u64,
        index_hint: usize,
        body: String,
        created_at: String,
    },
    Materials {
        id: u64,
        before: Vec<Attachment>,
        after: Vec<Attachment>,
        imported_copies: Vec<ImportedCopy>,
    },
    ProjectName { id: u64, before: String, after: String },
    ProjectDirectory {
        id: u64,
        before: String,
        after: String,
        impact: DirectoryImpactSnapshot,
    },
    ProjectCreated { id: u64, name: String, directory: String },
    TopicCreated { id: u64, project: u64, name: String },
    TopicName { id: u64, before: String, after: String },
    TopicArchived { id: u64, before: bool, after: bool },
    TopicProject { id: u64, before: u64, after: u64 },
    GoalTopic { id: u64, before: Option<u64>, after: Option<u64> },
    TaskArchived { id: u64, before: bool, after: bool },
    RestoredArchived { owners: Vec<ArchivedOwnerState> },
    Settings { changes: Vec<SettingsChange> },
}
```

All inverse entries are in-memory only; no schema migration.

### 2.4 Settings inverse

```rust
pub(crate) enum SettingsKey {
    DefaultModel,
    StageAgent(usize),
    StageModel(usize),
    ModelProfiles,
    ActiveProfile,
    ResearchConcurrency,
    AutomationConcurrency,
    Appearance,
    OpencodePath,
    CodexPath,
    PublishingConfig,
}

pub(crate) struct SettingsChange {
    pub key: SettingsKey,
    pub before: SettingsValue,
    pub after: SettingsValue,
}
```

Apply only changed keys. Reject the inverse if a changed key no longer equals its `after` value. Never restore `next_id`, runs, requests, occurrences, approvals or publications as part of Settings.

---

## 3. Preparation, impact, and durable outcome

### 3.1 Edit context

```rust
pub(crate) struct EditContext {
    pub command: EditCommand,
    pub base: CommandBase,
    pub impact: Option<ConfirmedImpact>,
    pub draft: DraftRetention,
    pub origin: NavigationOrigin,
}

pub(crate) struct ConfirmedImpact {
    pub goal: u64,
    pub signature: String,
    pub approval_before: bool,
    pub approval_after: bool,
    pub changed_steps: Vec<u64>,
    pub affected_steps: Vec<u64>,
    pub base_revision: CommandBase,
}
```

`ConfirmedImpact` is a token of the exact preview the user confirmed. `prepare` revalidates `impact.base_revision` and T21's current preview immediately before commit. A changed preview returns `PendingImpact` with the fresh preview; it never saves under the old confirmation.

### 3.2 Durable outcome

```rust
pub(crate) enum SaveOutcome {
    Saved {
        receipt: CommandReceipt,
        change_set: EntityChangeSet,
    },
    PendingImpact {
        pending: PendingEdit,
        impact: ResearchInputImpact,
    },
    PendingDurability {
        pending: PendingEdit,
        error: String,
    },
    Failed {
        error: String,
        draft: DraftRetention,
    },
    Cancelled {
        draft: DraftRetention,
    },
}

pub(crate) struct PendingEdit {
    pub command: EditCommand,
    pub base: CommandBase,
    pub draft: DraftRetention,
    pub origin: NavigationOrigin,
    pub impact: Option<ConfirmedImpact>,
}

pub(crate) struct DraftRetention {
    pub task: u64,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub due_date: String,
    pub cycle: Option<String>,
    pub stage: Stage,
    pub project: u64,
    pub origin_route: NavigationOrigin,
}
```

Rules:

- `request_save_draft` returns `PendingImpact` when confirmation is required; draft and route stay.
- `confirm_research_save` revalidates the impact token; changed impact → `PendingImpact`, no mutation.
- On `PendingDurability`, retain the same `EditContext`, draft and origin; `retry_pending_edit` reprepares from current state and either commits or conflicts.
- On `Saved`, only then clear the draft and navigate/open the task. Failed persistence must not clear the draft, source route, or T21 pending impact.
- Effects run only after the receipt. A failed save produces no undo entry.
- T10/T21 must consume this outcome; `save_draft` returning a bare `bool` is transitional and must not remain as the final contract.

### 3.3 Entity change set for T14/T15

```rust
pub(crate) struct EntityChangeSet {
    pub changes: Vec<EntityChange>,
}

pub(crate) struct EntityChange {
    pub entity: EntityRef,
    pub fields: Vec<&'static str>,
    pub kind: ChangeKind,
}

pub(crate) enum EntityRef {
    Task(u64),
    Project(u64),
    Topic(u64),
    ResearchRequest(u64),
    AutomationDefinition(u64),
    AutomationOccurrence(u64),
    Run(u64),
    Settings,
}

pub(crate) enum ChangeKind {
    Added,
    Updated,
    Removed,
}
```

This is the future dirty-entity seam for T14/T15; S3 records it, S4 can consume it. It is not a persistence rewrite.

---

## 4. Conflict and frozen-period policy

### 4.1 Conflict behavior

An inverse is prepared against a staged clone and revalidated:

- every `TaskFieldChange` must still equal `after`; unrelated fields may differ and remain untouched;
- comment must still exist exactly by `(index_hint, body, created_at)`;
- material list must still equal `after` by stable identity `(relative_path, name, size)`;
- project/topic names and archive/move states must equal `after`;
- goal topic must equal `after`;
- settings keys must equal `after`;
- approval companion requires current goal state == companion.after and runtime stamp unchanged.

On mismatch return:

```rust
EditConflict {
    message: String,
    retained: PendingEdit,
}
```

The editor stays open with the current draft. The user may:
- review the current value and re-apply (reprepare), or
- discard the pending edit explicitly.

No conflict path writes the live store, removes files, or clears the draft.

### 4.2 Frozen automation-period inputs

Use T21's `AutomationEditDecision` contract as the authority:

- `NotAutomationOwned`: normal edit rules.
- `MetadataOnly`: allow labels, due date and cycle only.
- `FrozenInput { fields, message }`: reject the command before staging. Reject title, description, comments, material changes, stage/project/topic changes, dependency changes and directory changes on a frozen automation period.
- A frozen task may still be renamed? `title` is currently an execution input because it participates in prompt/signature; reject it unless T21 explicitly classifies it metadata. Do not infer a broader exemption in S3.
- Metadata edits do not create approval companions.

The domain command returns `EditRejected::FrozenInput`; no run, approval, cursor or draft is changed.

---

## 5. Current call-site inventory and transition

Worktree line numbers may drift. Every remaining `snapshot()`/`confirm_save()`/direct edit must map to one contract action.

| Current entry | Current site | Classification | Contract transition |
| --- | --- | --- | --- |
| Manual task stage | `app.rs:1022-1024` | Undoable with approval companion | `EditCommand::TaskStage`; inverse `UndoEntry::TaskStage`. |
| Task editor save | `app.rs:1066-1148` | Undoable, staged bool today | Migrate to `EditCommand::TaskFields` + `TaskStage`; return `SaveOutcome`; remove `snapshot()` at 1132. T10/T21 call `request_save_draft`/`confirm_research_save`. |
| Project folder | `app.rs:1468-1479` | Undoable with directory impact preview | `ProjectDirectory` + `ConfirmedDirectoryImpact`; replace `confirm_save`. |
| Archive restore loop | `archive_app.rs:39-...` | Undoable re-archive, conditional | `RestoreArchived`; inverse `RestoredArchived`; reject if restored work started. |
| Archive/stop work | `archive_app.rs:109-174` | **Non-undoable when it stops/cancels work** | `Archive` command stops safely; no inverse entry if any run/occurrence was active/queued/recovering. Clean archive may use `TaskArchived`/`TopicArchived` only. Never undo cancel effects into active unscheduled execution. |
| Reopen step | `goal_steps.rs:193-197` | Undoable stage inverse | `StepReopen` → `TaskStage`. |
| Project rename | `project_views.rs:30-37` | Undoable | `ProjectName`. |
| Model agent/model | `research_app.rs:45-66`, `:100-115` | Undoable Settings | `Settings` changes only. |
| Use global stage | `research_app.rs:71-77` | Undoable Settings | `Settings`. |
| Remove material | `research_app.rs:317-325` | Undoable metadata inverse | `TaskMaterials::Remove`; file remains. |
| Add materials | `research_app.rs:365-383` | Undoable metadata; import cleanup | `TaskMaterials::Add`; `ImportedCopy` cleanup only new app-owned unreferenced copies. |
| Use global all stages | `research_views.rs:227-235` | Undoable Settings | `Settings`. |
| Apply/restore/delete/update/create profile | `settings_views.rs:57-142` | Undoable Settings | `Settings` profile keys. |
| Research/automation concurrency | `settings_views.rs:155-166` | Undoable Settings | `Settings` limits; replace `confirm_save`. |
| Topic move | `topic_views.rs:57-68` | Undoable | `TopicMove` inverse `TopicProject`; revalidate blockers. |
| Topic create/rename | `topic_views.rs:154-...` | Undoable create guarded, rename normal | `TopicCreate`/`TopicRename`; create inverse only if unchanged/unreferenced. |
| Goal/root topic | `topic_views.rs:254-...` | Undoable | `GoalTopic`. |
| Restore topic | `topic_views.rs:572` | Undoable archive-state inverse | `TopicArchive`; conditional. |
| Comment | `views.rs:2888-...` | Undoable if unchanged | `TaskComment`; inverse removes exact comment only. |
| Legacy task draft fields | `views.rs:3411`, `3447`, `3477`, `3500`, `3526` | Fold into task editor | Remove direct field writes; use `request_save_draft`/`EditCommand::TaskFields`. |
| Create project | `views.rs:4108-4131` | Undoable only if empty | `ProjectCreate`; inverse `ProjectCreated` only when no topics/tasks/requests were added; else non-undoable/conflict. Replace `confirm_save`. |
| Publishing settings | `report_publish.rs:255`, `:360` | Undoable Settings | `Settings` publishing keys; replace `confirm_save`. |
| Task run submission | `app.rs`/`research_app.rs` run paths | **Non-undoable runtime** | Clears undo; no inverse. |
| Automation lifecycle | `automation_views.rs` / `automation_runtime.rs` | **Non-undoable runtime** | Commands are `NotUndoable`; T04 guard remains until migration. |
| Planning request create/replan/apply, planning run | `project_app.rs`, `automation_views.rs`, `request_planning.rs` | **Non-undoable runtime** | No inverse; T23/T25 ownership. |
| Recovery, retry, timeout, expiry, read/unread, log cleanup, publication | runtime paths | **Non-undoable runtime** | No inverse; preserve history. |

### 5.1 `confirm_save` inventory

Replace with command receipts:

- `app.rs:1479` project folder
- `settings_views.rs:157`, `:166` concurrency
- `report_publish.rs:255`, `:360` publishing settings
- `views.rs:4131` project creation

`confirm_save` may remain only as a thin adapter that maps `SaveOutcome::Saved` and never claims success on `PendingDurability`/`Failed`.

---

## 6. Material ownership and cleanup

```rust
pub(crate) struct ImportedCopy {
    pub relative_path: String,
    pub size: u64,
    pub source: MaterialSource, // ImportedBytes | ImportedFile
}

pub(crate) enum MaterialChange {
    Add { attachments: Vec<Attachment> },
    Remove { index: usize },
}
```

Rules:

- `Add` records every newly created app-owned copy in `imported_copies`.
- If the add command is cancelled, fails validation, or fails durability, remove only those newly created copies if they are under the workspace attachment root and no live/current attachment references them.
- Undo of `Add` removes the attachment metadata and performs the same guarded cleanup.
- Undo of `Remove` restores metadata only if the file still exists at the recorded path and size. If the file is missing/changed, conflict; do not fabricate a new copy.
- Never delete the original user-selected file, a shared dependency attachment, or a path outside the app-owned attachment root.
- Material changes on frozen automation periods are rejected before import.

---

## 7. Directory impact

`ProjectDirectory` requires:

```rust
pub(crate) struct ConfirmedDirectoryImpact {
    pub project: u64,
    pub before: String,
    pub after: String,
    pub affected_steps: Vec<u64>,
    pub approval_before: Vec<GoalApprovalState>,
    pub approval_after: Vec<GoalApprovalState>,
    pub base: CommandBase,
}
```

- Preview must list exact affected steps and approval consequences.
- Commit revalidates `base` and recomputes the impact; changed impact returns `PendingImpact`, no save.
- Inverse restores the old directory only if current still equals `after` and no active run uses the old/new directory.
- Undo must not start or retarget existing runs; it only changes project configuration.

---

## 8. Elimination order for `Vec<Store>`

Do not remove T04 invalidation until the last snapshot path is gone.

1. **S3a — contract types and change set.** Add `EditCommand`, `UndoEntry`, `SaveOutcome`, `PendingEdit`, `EntityChangeSet`; no behavior change. Add pure inverse round-trip tests.
2. **S3b — T10/T21 save path.** Migrate `save_draft` to `PendingImpact`/`Saved` outcome; T21 confirmation revalidates atomically; remove `snapshot()` at `app.rs:1132`. This is mandatory before any broad undo removal.
3. **S3c — simple metadata inverses.** Task fields/stage/comment/step reopen; project/topic names/archives/moves/goal topic.
4. **S3d — materials.** Add/remove with ownership and cleanup; remove attachment snapshots at `research_app.rs:317`, `:365`.
5. **S3e — settings.** Models/profiles/concurrency/appearance/publishing; remove settings snapshots.
6. **S3f — archive/restore and directory.** Safe archive non-undoable when work stopped; clean archive inverse; directory preview/inverse; project creation.
7. **S3g — remaining sites and deletion.** Remove all snapshot call sites, then `TaskApp::undo: Vec<Store>`, `snapshot`, whole-store pop, and `automation_state_differs` invalidation. Keep the protected-state assertion inside the command.
8. **S4/S5 — docs and T14/T15.** T17 documents the final contract; T14 consumes change-set/revision; T15 persists dirty entities.

Exact files to change in S3:

- `src/domain_command.rs` (types, preparation, inverse)
- `src/command_app.rs` (outcome/pending/retry/conflict)
- `src/app.rs` (`save_draft`, change_stage, project directory, undo stack)
- `src/research_input_app.rs` (T21 outcome seam, no second mutation)
- `src/research_input.rs` (impact token/base, no behavior rewrite)
- `src/research_app.rs` (models/materials)
- `src/research_views.rs`, `src/settings_views.rs`, `src/goal_steps.rs`, `src/project_views.rs`, `src/topic_views.rs`, `src/archive_app.rs`, `src/views.rs`
- `src/command_qa.rs` (failure/conflict/undo tests)

Do not edit T06 result presentation, T08 search, T10 navigation except the save-outcome seam, T12 routes, T22 archive results, T23/T25 planning ownership, or frozen snapshots.

---

## 9. Tests and failure injection

### 9.1 Durable save

1. Fail before preparation: invalid/frozen input → no store mutation, draft retained.
2. Fail after preparation before write: injected SQLite trigger → `PendingDurability`, live store unchanged, draft/source route retained.
3. Retry after storage recovery: exactly one commit, one undo entry, no duplicate activity/comment/material copy.
4. Stale confirmation: preview, change goal/input/approval, confirm → `PendingImpact` with fresh impact, no write.
5. Save succeeds but app is captured before UI open: reopen and verify one durable edit, no second write.

### 9.2 Inverse behavior

6. Field-level inverse: edit title, change labels afterward, undo → title restored, labels and unrelated activity preserved.
7. Same goal approval companion: edit description, then complete a run; undo → conflict, approval/runs/logs untouched; retained pending edit.
8. Stage inverse: approval/approved_plan restored only when goal/input/runtime stamp matches after.
9. Comment inverse: exact comment removed; edited/deleted comment → conflict.
10. Material add: cancel before commit and failed save remove only new app-owned unreferenced copy; original source remains.
11. Material remove: undo restores metadata; missing/changed file → conflict; no new file fabricated.
12. Project/topic rename/move/archive/restore: inverse preserves unrelated fields and runs.
13. Archive with active/queued run: command stops safely and is explicitly non-undoable; no inverse can reactivate work.
14. Clean archive: inverse unarchives only; no schedule/run restart.
15. Settings: only changed keys restored; profile/concurrency/appearance inversion does not touch runs.
16. Directory: preview lists affected steps/approval; stale preview/impact conflict; inverse never retargets an active run.
17. Frozen period: title/description/comment/material/stage/directory rejected; labels/due/cycle allowed per T21.
18. Recursive `restore_archived`: inverse restores the exact prior archived owners; partial restore conflict leaves intent.
19. T04: ordinary edit + automation mutation + Undo cannot roll back automation, runs, cursor, approvals; legacy guard stays until S3g.
20. Change set: each command emits exact entity/field changes; no whole-store snapshot is required for persistence.

### 9.3 Test levels

- Pure `domain_command` unit tests for prepare/inverse/conflict.
- App executor tests with real SQLite failure triggers (`command_qa`).
- Isolated UI QA for draft retention, `PendingImpact`, `PendingDurability`, Saved, retry, conflict, material cancel, archive safety.
- Full `--qa-command`, `--qa-automation`, and full unit suite with Node 24 PATH; record actual exit codes.

---

## 10. Ready-for-review handoff

This document is the S3/S4 contract for root review. No source was mutated.

Remaining before implementation authorization:

- T10/T21 must publish the save-outcome seam and confirm the exact `DraftRetention`/impact token ownership.
- T12 origin/return context remains a dependency for archive/project/topic inverse navigation, not for field inverses.
- T04 stays in force until all snapshot paths are migrated.
- T14/T15 consume `EntityChangeSet`; S3 does not start async persistence.

No acceptance is claimed. No commit, push, production workspace access, or app restart.
