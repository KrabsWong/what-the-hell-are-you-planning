//! T13 S3d: scoped material add/remove with content identity and owned-copy
//! cleanup. T21 preview remains the execution-input authority.

use crate::attachments::{self, Attachment};
use crate::domain::Store;
use crate::research_input::{self, ResearchInputImpact};
use crate::scoped_edit::{
    approval_context, approval_state, reconcile_goal_scope, restore_scoped_inverse, runtime_stamp,
    validate_companions, GoalApprovalCompanion, GoalApprovalState, GoalRuntimeStamp,
    ScopedEditError,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentIdentity {
    pub name: String,
    pub relative_path: String,
    pub media_type: String,
    pub size: u64,
    pub content_sha256: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CleanupObligation {
    pub root: std::path::PathBuf,
    pub copies: Vec<ImportedCopy>,
}

pub(crate) fn take_cleanup_obligation(
    queue: &mut Vec<CleanupObligation>,
    current: &std::path::Path,
) -> Option<CleanupObligation> {
    let index = queue.iter().position(|obligation| {
        !obligation.root.as_os_str().is_empty() && obligation.root == current
    })?;
    Some(queue.remove(index))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportedCopy {
    pub relative_path: String,
    pub size: u64,
    pub content_sha256: String,
    pub owner_task: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialAddPlan {
    id: u64,
    before: Vec<Attachment>,
    attachments: Vec<Attachment>,
    imported: Vec<ImportedCopy>,
    approval_before: Vec<GoalApprovalState>,
    runtime_before: Vec<GoalRuntimeStamp>,
    impact: Option<ResearchInputImpact>,
}

impl MaterialAddPlan {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    pub(crate) fn impact(&self) -> Option<&ResearchInputImpact> {
        self.impact.as_ref()
    }
    pub(crate) fn imported(&self) -> &[ImportedCopy] {
        &self.imported
    }
    pub(crate) fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialRemovePlan {
    id: u64,
    index: usize,
    before: Vec<Attachment>,
    after: Vec<Attachment>,
    removed: Attachment,
    target_identity: AttachmentIdentity,
    approval_before: Vec<GoalApprovalState>,
    runtime_before: Vec<GoalRuntimeStamp>,
    impact: Option<ResearchInputImpact>,
}

impl MaterialRemovePlan {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    pub(crate) fn impact(&self) -> Option<&ResearchInputImpact> {
        self.impact.as_ref()
    }
    pub(crate) fn index(&self) -> usize {
        self.index
    }
    pub(crate) fn target_identity(&self) -> &AttachmentIdentity {
        &self.target_identity
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialAddUndo {
    id: u64,
    before: Vec<Attachment>,
    after: Vec<Attachment>,
    imported: Vec<ImportedCopy>,
    approval: Vec<GoalApprovalCompanion>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialRemoveUndo {
    id: u64,
    index: usize,
    before: Vec<Attachment>,
    after: Vec<Attachment>,
    before_identity: AttachmentIdentity,
    approval: Vec<GoalApprovalCompanion>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MaterialUndo {
    Add(MaterialAddUndo),
    Remove(MaterialRemoveUndo),
}

impl MaterialUndo {
    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Add(undo) => undo.id,
            Self::Remove(undo) => undo.id,
        }
    }

    pub(crate) fn target_identity(&self) -> Option<&AttachmentIdentity> {
        match self {
            Self::Remove(undo) => Some(&undo.before_identity),
            Self::Add(_) => None,
        }
    }

    pub(crate) fn reference_paths(&self) -> Vec<String> {
        match self {
            Self::Add(undo) => undo
                .before
                .iter()
                .chain(undo.after.iter())
                .map(|attachment| attachment.relative_path.clone())
                .collect(),
            Self::Remove(undo) => undo
                .before
                .iter()
                .chain(undo.after.iter())
                .map(|attachment| attachment.relative_path.clone())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MaterialChangeSet {
    pub task: u64,
    pub before: Vec<AttachmentIdentity>,
    pub after: Vec<AttachmentIdentity>,
    pub imported: Vec<ImportedCopy>,
    pub approvals: Vec<GoalApprovalCompanion>,
    pub activity: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppliedMaterialAdd {
    pub undo: MaterialAddUndo,
    pub impact: Option<ResearchInputImpact>,
    pub change_set: MaterialChangeSet,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppliedMaterialRemove {
    pub undo: MaterialRemoveUndo,
    pub impact: Option<ResearchInputImpact>,
    pub change_set: MaterialChangeSet,
}

pub(crate) fn attachment_sha256(_data_dir: &Path, attachment: &Attachment) -> Result<String, String> {
    if attachment.content_sha256.is_empty() {
        return Err("This material needs background verification before it can be changed.".into());
    }
    Ok(attachment.content_sha256.clone())
}

pub(crate) fn imported_copy(
    _data_dir: &Path,
    owner_task: u64,
    attachment: &Attachment,
) -> Result<ImportedCopy, String> {
    if attachment.content_sha256.is_empty() {
        return Err("The imported material has no content identity.".into());
    }
    Ok(ImportedCopy {
        relative_path: attachment.relative_path.clone(),
        size: attachment.size,
        content_sha256: attachment.content_sha256.clone(),
        owner_task,
    })
}

fn identity_list(_data_dir: &Path, attachments: &[Attachment]) -> Result<Vec<AttachmentIdentity>, String> {
    Ok(attachments
        .iter()
        .map(|attachment| identity(attachment, attachment.content_sha256.clone()))
        .collect())
}

fn identity(attachment: &Attachment, sha: String) -> AttachmentIdentity {
    AttachmentIdentity {
        name: attachment.name.clone(),
        relative_path: attachment.relative_path.clone(),
        media_type: attachment.media_type.clone(),
        size: attachment.size,
        content_sha256: sha,
    }
}


fn validate_imported_copy(data_dir: &Path, copy: &ImportedCopy) -> Result<(), ScopedEditError> {
    let _ = data_dir;
    if copy.content_sha256.is_empty() {
        return Err(ScopedEditError::Rejected(
            "The imported material has no content identity. Add it again.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare_material_add(
    store: &Store,
    data_dir: &Path,
    id: u64,
    attachments: Vec<Attachment>,
    imported: Vec<ImportedCopy>,
) -> Result<MaterialAddPlan, ScopedEditError> {
    if attachments.is_empty() {
        return Err(ScopedEditError::Rejected("Choose a material to add.".into()));
    }
    let task = store
        .task(id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before changing materials.".into(),
        ));
    }
    if task.attachments.len() + attachments.len() > 12 {
        return Err(ScopedEditError::Rejected(
            "A task can hold up to 12 attachments.".into(),
        ));
    }
    for attachment in &attachments {
        let Some(copy) = imported.iter().find(|copy| {
            copy.relative_path == attachment.relative_path
                && copy.size == attachment.size
                && copy.owner_task == id
        }) else {
            return Err(ScopedEditError::Rejected(
                "The imported material identity is missing or owned by another task.".into(),
            ));
        };
        validate_imported_copy(data_dir, copy)?;
        if attachment.content_sha256 != copy.content_sha256 {
            return Err(ScopedEditError::Rejected(
                "The imported material identity does not match the attachment.".into(),
            ));
        }
    }
    let impact = research_input::preview_material_additions(store, id, &attachments)
        .map_err(ScopedEditError::Rejected)?;
    let needs_approval = impact
        .as_ref()
        .is_some_and(|impact| !impact.presentation_only);
    let (approval_before, runtime_before) = if needs_approval {
        approval_context(store, id)?
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(MaterialAddPlan {
        id,
        before: task.attachments.clone(),
        attachments,
        imported,
        approval_before,
        runtime_before,
        impact,
    })
}

pub(crate) fn apply_material_add(
    store: &mut Store,
    data_dir: &Path,
    plan: &MaterialAddPlan,
) -> Result<AppliedMaterialAdd, ScopedEditError> {
    if plan.approval_before.len() != plan.runtime_before.len() {
        return Err(ScopedEditError::Rejected(
            "The material plan has mismatched approval and runtime evidence.".into(),
        ));
    }
    let task = store
        .task(plan.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(plan.id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before changing materials.".into(),
        ));
    }
    if task.attachments != plan.before {
        return Err(ScopedEditError::Conflict(
            "The material list changed after this add was prepared.".into(),
        ));
    }
    let current_impact = research_input::preview_material_additions(store, plan.id, &plan.attachments)
        .map_err(ScopedEditError::Conflict)?;
    if current_impact != plan.impact {
        return Err(ScopedEditError::Conflict(
            "The research context changed after this material was prepared. Review the new impact before adding."
                .into(),
        ));
    }
    for before in &plan.approval_before {
        let now = approval_state(store, before.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if &now != before {
            return Err(ScopedEditError::Conflict(
                "The goal's approval or stage changed after this material was prepared.".into(),
            ));
        }
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .ok_or_else(|| {
                ScopedEditError::Rejected("The material plan is missing runtime evidence.".into())
            })?;
        if &runtime_stamp(store, before.goal) != runtime {
            return Err(ScopedEditError::Conflict(
                "A run or automation period changed for this goal after the material was prepared."
                    .into(),
            ));
        }
    }

    for copy in &plan.imported {
        validate_imported_copy(data_dir, copy)?;
    }
    let before_identities = identity_list(data_dir, &plan.before)
        .map_err(ScopedEditError::Rejected)?;
    let mut after_identities = before_identities.clone();
    for attachment in &plan.attachments {
        let copy = plan
            .imported
            .iter()
            .find(|copy| {
                copy.relative_path == attachment.relative_path && copy.size == attachment.size
            })
            .ok_or_else(|| {
                ScopedEditError::Rejected("The imported material identity is missing.".into())
            })?;
        after_identities.push(identity(attachment, copy.content_sha256.clone()));
    }

    let task = store.task_mut(plan.id).expect("validated task");
    task.attachments.extend(plan.attachments.iter().cloned());
    task.activity.push(crate::domain::activity_event("Materials added"));
    let after = store.task(plan.id).expect("validated task").attachments.clone();
    let mut activity = vec![plan.id];
    let mut approvals = Vec::new();
    for before in &plan.approval_before {
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .expect("prevalidated runtime evidence")
            .clone();
        if reconcile_goal_scope(store, before.goal) {
            activity.push(before.goal);
        }
        let after = approval_state(store, before.goal).expect("approval goal validated");
        approvals.push(GoalApprovalCompanion {
            goal: before.goal,
            before: before.clone(),
            after,
            runtime,
        });
    }
    Ok(AppliedMaterialAdd {
        undo: MaterialAddUndo {
            id: plan.id,
            before: plan.before.clone(),
            after: after.clone(),
            imported: plan.imported.clone(),
            approval: approvals.clone(),
        },
        impact: plan.impact.clone(),
        change_set: MaterialChangeSet {
            task: plan.id,
            before: before_identities,
            after: after_identities,
            imported: plan.imported.clone(),
            approvals,
            activity,
        },
    })
}

pub(crate) fn prepare_material_remove(
    store: &Store,
    data_dir: &Path,
    id: u64,
    index: usize,
) -> Result<MaterialRemovePlan, ScopedEditError> {
    prepare_material_remove_inner(store, data_dir, id, index, None)
}

pub(crate) fn prepare_material_remove_verified(
    store: &Store,
    data_dir: &Path,
    id: u64,
    index: usize,
    content_sha256: &str,
) -> Result<MaterialRemovePlan, ScopedEditError> {
    prepare_material_remove_inner(store, data_dir, id, index, Some(content_sha256))
}

fn prepare_material_remove_inner(
    store: &Store,
    data_dir: &Path,
    id: u64,
    index: usize,
    verified_sha256: Option<&str>,
) -> Result<MaterialRemovePlan, ScopedEditError> {
    let task = store
        .task(id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if store.task_is_archived(id) {
        return Err(ScopedEditError::Rejected(
            "Restore the archived task before changing materials.".into(),
        ));
    }
    let _ = data_dir;
    let removed = task.attachments.get(index).cloned().ok_or_else(|| {
        ScopedEditError::Rejected("Choose a material that is still attached.".into())
    })?;
    let impact = research_input::preview_material_change(
        store,
        id,
        research_input::MaterialChange::Remove(index),
    )
    .map_err(ScopedEditError::Rejected)?;
    let needs_approval = impact
        .as_ref()
        .is_some_and(|impact| !impact.presentation_only);
    let (approval_before, runtime_before) = if needs_approval {
        approval_context(store, id)?
    } else {
        (Vec::new(), Vec::new())
    };
    let mut after = task.attachments.clone();
    after.remove(index);
    let sha = match verified_sha256 {
        Some(sha) => sha.to_string(),
        None => attachment_sha256(data_dir, &removed).map_err(ScopedEditError::Rejected)?,
    };
    let target_identity = identity(&removed, sha);
    Ok(MaterialRemovePlan {
        id,
        index,
        before: task.attachments.clone(),
        after,
        removed,
        target_identity,
        approval_before,
        runtime_before,
        impact,
    })
}

pub(crate) fn prepare_material_remove_by_identity(
    store: &Store,
    data_dir: &Path,
    id: u64,
    target: &AttachmentIdentity,
) -> Result<MaterialRemovePlan, ScopedEditError> {
    let task = store
        .task(id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    let Some(index) = task
        .attachments
        .iter()
        .position(|attachment| attachment.relative_path == target.relative_path)
    else {
        return Err(ScopedEditError::Conflict(
            "The material selected for removal is no longer attached.".into(),
        ));
    };
    let current = &task.attachments[index];
    let sha = attachment_sha256(data_dir, current).map_err(ScopedEditError::Conflict)?;
    if current.size != target.size || sha != target.content_sha256 {
        return Err(ScopedEditError::Conflict(
            "The selected material changed after removal was prepared. Retry was not retargeted."
                .into(),
        ));
    }
    let plan = prepare_material_remove_inner(
        store,
        data_dir,
        id,
        index,
        Some(&target.content_sha256),
    )?;
    if plan.target_identity != *target {
        return Err(ScopedEditError::Conflict(
            "The selected material identity changed after removal was prepared.".into(),
        ));
    }
    Ok(plan)
}

pub(crate) fn apply_material_remove(
    store: &mut Store,
    data_dir: &Path,
    plan: &MaterialRemovePlan,
) -> Result<AppliedMaterialRemove, ScopedEditError> {
    let task = store
        .task(plan.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if task.attachments != plan.before {
        return Err(ScopedEditError::Conflict(
            "The material list changed after this removal was prepared.".into(),
        ));
    }
    let current_impact = research_input::preview_material_change(
        store,
        plan.id,
        research_input::MaterialChange::Remove(plan.index),
    )
    .map_err(ScopedEditError::Conflict)?;
    if current_impact != plan.impact {
        return Err(ScopedEditError::Conflict(
            "The research context changed after this material was prepared. Review the new impact before removing."
                .into(),
        ));
    }
    for before in &plan.approval_before {
        let now = approval_state(store, before.goal).ok_or_else(|| {
            ScopedEditError::Conflict("The owning goal is no longer available.".into())
        })?;
        if &now != before {
            return Err(ScopedEditError::Conflict(
                "The goal's approval or stage changed after this material was prepared.".into(),
            ));
        }
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .ok_or_else(|| {
                ScopedEditError::Rejected("The material plan is missing runtime evidence.".into())
            })?;
        if &runtime_stamp(store, before.goal) != runtime {
            return Err(ScopedEditError::Conflict(
                "A run or automation period changed for this goal after the material was prepared."
                    .into(),
            ));
        }
    }
    let before_identities = identity_list(data_dir, &plan.before)
        .map_err(ScopedEditError::Rejected)?;
    let after_identities = identity_list(data_dir, &plan.after)
        .map_err(ScopedEditError::Rejected)?;
    let before_identity = identity(
        &plan.removed,
        attachment_sha256(data_dir, &plan.removed).map_err(ScopedEditError::Rejected)?,
    );

    let task = store.task_mut(plan.id).expect("validated task");
    task.attachments.remove(plan.index);
    task.activity
        .push(crate::domain::activity_event("Material removed"));
    let mut activity = vec![plan.id];
    let mut approvals = Vec::new();
    for before in &plan.approval_before {
        let runtime = plan
            .runtime_before
            .iter()
            .find(|stamp| stamp.goal == before.goal)
            .expect("prevalidated runtime evidence")
            .clone();
        if reconcile_goal_scope(store, before.goal) {
            activity.push(before.goal);
        }
        let after = approval_state(store, before.goal).expect("approval goal validated");
        approvals.push(GoalApprovalCompanion {
            goal: before.goal,
            before: before.clone(),
            after,
            runtime,
        });
    }
    Ok(AppliedMaterialRemove {
        undo: MaterialRemoveUndo {
            id: plan.id,
            index: plan.index,
            before: plan.before.clone(),
            after: plan.after.clone(),
            before_identity,
            approval: approvals.clone(),
        },
        impact: plan.impact.clone(),
        change_set: MaterialChangeSet {
            task: plan.id,
            before: before_identities,
            after: after_identities,
            imported: Vec::new(),
            approvals,
            activity,
        },
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MaterialUndoEffect {
    /// App-owned copies whose deletion is deferred until the metadata write is
    /// durable. The live caller rechecks references before deleting.
    pub cleanup: Vec<ImportedCopy>,
}

pub(crate) fn undo_material_edit(
    store: &mut Store,
    data_dir: &Path,
    undo: &MaterialUndo,
) -> Result<MaterialUndoEffect, ScopedEditError> {
    match undo {
        MaterialUndo::Add(undo) => undo_material_add(store, data_dir, undo),
        MaterialUndo::Remove(undo) => {
            undo_material_remove(store, data_dir, undo)?;
            Ok(MaterialUndoEffect::default())
        }
    }
}

fn undo_material_add(
    store: &mut Store,
    data_dir: &Path,
    undo: &MaterialAddUndo,
) -> Result<MaterialUndoEffect, ScopedEditError> {
    let task = store
        .task(undo.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if task.attachments != undo.after {
        return Err(ScopedEditError::Conflict(
            "The material list changed after this add. Undo would remove newer materials.".into(),
        ));
    }
    validate_companions(store, &undo.approval)?;
    let original = task.attachments.clone();
    store.task_mut(undo.id).expect("validated task").attachments = undo.before.clone();
    if let Err(error) = restore_scoped_inverse(
        store,
        undo.id,
        &[],
        None,
        &undo.approval,
        Some("Undo: materials removed"),
        true,
    ) {
        store.task_mut(undo.id).expect("validated task").attachments = original;
        return Err(error);
    }
    Ok(MaterialUndoEffect {
        cleanup: undo.imported.clone(),
    })
}

fn undo_material_remove(
    store: &mut Store,
    data_dir: &Path,
    undo: &MaterialRemoveUndo,
) -> Result<(), ScopedEditError> {
    let restored = undo
        .before
        .get(undo.index)
        .cloned()
        .ok_or_else(|| ScopedEditError::Conflict("The removed material is missing.".into()))?;
    let path = attachments::resolve(data_dir, &restored)
        .map_err(|error| ScopedEditError::Conflict(error.to_string()))?;
    let bytes = fs::read(&path)
        .map_err(|error| ScopedEditError::Conflict(format!("Cannot read material: {error}")))?;
    let sha = format!("{:x}", Sha256::digest(bytes));
    if restored.size != undo.before_identity.size || sha != undo.before_identity.content_sha256 {
        return Err(ScopedEditError::Conflict(
            "The material file changed or is missing. Undo will not fabricate or overwrite material bytes."
                .into(),
        ));
    }
    restore_remove_metadata(store, undo)
}

pub(crate) fn undo_material_edit_verified(
    store: &mut Store,
    undo: &MaterialUndo,
) -> Result<MaterialUndoEffect, ScopedEditError> {
    match undo {
        MaterialUndo::Add(undo) => undo_material_add(store, Path::new(""), undo),
        MaterialUndo::Remove(undo) => {
            restore_remove_metadata(store, undo)?;
            Ok(MaterialUndoEffect::default())
        }
    }
}

fn restore_remove_metadata(
    store: &mut Store,
    undo: &MaterialRemoveUndo,
) -> Result<(), ScopedEditError> {
    let task = store
        .task(undo.id)
        .ok_or_else(|| ScopedEditError::Rejected("Task not found.".into()))?;
    if task.attachments != undo.after {
        return Err(ScopedEditError::Conflict(
            "The material list changed after this removal. Undo would overwrite newer materials."
                .into(),
        ));
    }
    let restored = undo
        .before
        .get(undo.index)
        .cloned()
        .ok_or_else(|| ScopedEditError::Conflict("The removed material is missing.".into()))?;
    validate_companions(store, &undo.approval)?;
    let original = task.attachments.clone();
    let mut restored_list = undo.after.clone();
    restored_list.insert(undo.index.min(restored_list.len()), restored);
    store.task_mut(undo.id).expect("validated task").attachments = restored_list;
    if let Err(error) = restore_scoped_inverse(
        store,
        undo.id,
        &[],
        None,
        &undo.approval,
        Some("Undo: material restored"),
        true,
    ) {
        store.task_mut(undo.id).expect("validated task").attachments = original;
        return Err(error);
    }
    Ok(())
}

pub(crate) fn cleanup_imported_copies(
    data_dir: &Path,
    store: &Store,
    copies: &[ImportedCopy],
    extra_references: &[String],
) {
    for copy in copies {
        if referenced(store, &copy.relative_path, extra_references) {
            continue;
        }
        let Ok(path) = attachments::resolve(data_dir, &attachment_ref(&copy.relative_path)) else {
            continue;
        };
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let sha = format!("{:x}", Sha256::digest(bytes));
        if sha == copy.content_sha256 && fs::remove_file(path).is_err() {
            // Best-effort cleanup; metadata undo must not be reported as failed.
        }
    }
}

pub(crate) fn referenced_by_store(store: &Store, relative_path: &str) -> bool {
    referenced(store, relative_path, &[])
}

fn referenced(store: &Store, relative_path: &str, extra_references: &[String]) -> bool {
    if extra_references.iter().any(|path| path == relative_path) {
        return true;
    }
    store.tasks.iter().any(|task| {
        task.attachments
            .iter()
            .any(|attachment| attachment.relative_path == relative_path)
    }) || store.runs().iter().any(|run| {
        run.attachments
            .iter()
            .any(|attachment| attachment.relative_path == relative_path)
    })
}

fn attachment_ref(relative_path: &str) -> Attachment {
    Attachment {
        name: String::new(),
        relative_path: relative_path.to_string(),
        media_type: String::new(),
        size: 0,
        content_sha256: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Stage, Store};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "t13-material-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fixture() -> (Store, std::path::PathBuf, u64, Attachment, ImportedCopy) {
        let directory = temp_dir();
        let mut store = Store::empty();
        store.projects[0].directory = directory.to_string_lossy().into_owned();
        let task = store
            .create_task("Material task", 1, Stage::Planned, None)
            .unwrap();
        let attachment = crate::attachments::import_bytes(
            &directory,
            "material.pdf",
            b"%PDF-1.4\nmaterial fixture\n",
        )
        .unwrap();
        let imported = imported_copy(&directory, task, &attachment).unwrap();
        (store, directory, task, attachment, imported)
    }

    #[test]
    fn cleanup_drain_removes_only_one_matching_obligation_and_keeps_the_rest() {
        let copy = |path: &str| ImportedCopy {
            relative_path: path.into(),
            size: 1,
            content_sha256: "abc".into(),
            owner_task: 1,
        };
        let mut queue = vec![
            CleanupObligation {
                root: std::path::PathBuf::from("/foreign"),
                copies: vec![copy("attachments/foreign.pdf")],
            },
            CleanupObligation {
                root: std::path::PathBuf::from("/current"),
                copies: vec![copy("attachments/a.pdf")],
            },
            CleanupObligation {
                root: std::path::PathBuf::from("/current"),
                copies: vec![copy("attachments/b.pdf")],
            },
        ];
        let first =
            take_cleanup_obligation(&mut queue, std::path::Path::new("/current")).unwrap();
        assert_eq!(first.copies[0].relative_path, "attachments/a.pdf");
        assert_eq!(queue.len(), 2);
        let second =
            take_cleanup_obligation(&mut queue, std::path::Path::new("/current")).unwrap();
        assert_eq!(second.copies[0].relative_path, "attachments/b.pdf");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].root, std::path::PathBuf::from("/foreign"));
        assert!(take_cleanup_obligation(&mut queue, std::path::Path::new("/current")).is_none());
        queue.push(CleanupObligation {
            root: std::path::PathBuf::new(),
            copies: vec![copy("attachments/unbound.pdf")],
        });
        assert!(take_cleanup_obligation(&mut queue, std::path::Path::new("/current")).is_none());
        assert_eq!(queue.len(), 2, "foreign and unknown ownership remain unresolved");
        assert!(queue[1].root.as_os_str().is_empty());
        assert_eq!(queue[1].copies[0].relative_path, "attachments/unbound.pdf");
    }

    #[test]
    fn frozen_period_material_add_is_rejected_before_attachment() {
        let (mut store, directory, task, attachment, imported) = fixture();
        store.task_mut(task).unwrap().automation_id = Some(1);
        let before = store.clone();
        assert!(matches!(
            prepare_material_add(
                &store,
                &directory,
                task,
                vec![attachment.clone()],
                vec![imported]
            ),
            Err(ScopedEditError::Rejected(_))
        ));
        assert_eq!(store, before);
        assert!(crate::attachments::resolve(&directory, &attachment).unwrap().exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn add_inverse_restores_goal_approval_and_stage_for_research_material() {
        let directory = temp_dir();
        let mut store = Store::empty();
        store.projects[0].directory = directory.to_string_lossy().into_owned();
        let goal = store
            .create_research_goal("Material goal".into(), "Brief".into(), 1)
            .unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
        store.set_stage(goal, Stage::Planned).unwrap();
        let approval = store.task(goal).unwrap().approved_plan.clone();
        let attachment = crate::attachments::import_bytes(
            &directory,
            "goal-material.pdf",
            b"%PDF-1.4\ngoal material\n",
        )
        .unwrap();
        let imported = imported_copy(&directory, goal, &attachment).unwrap();
        let plan = prepare_material_add(
            &store,
            &directory,
            goal,
            vec![attachment],
            vec![imported],
        )
        .unwrap();
        let applied = apply_material_add(&mut store, &directory, &plan).unwrap();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);
        assert!(!applied.undo.approval.is_empty());

        undo_material_edit(&mut store, &directory, &MaterialUndo::Add(applied.undo)).unwrap();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Planned);
        assert_eq!(store.task(goal).unwrap().approved_plan, approval);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn verifier_detects_changed_content_after_import_identity() {
        let (store, directory, task, attachment, imported) = fixture();
        let _ = (store, task);
        let path = crate::attachments::resolve(&directory, &attachment).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        fs::write(&path, bytes).unwrap();
        let mut verifier = crate::material_verify::MaterialVerifier::new();
        verifier
            .submit(
                directory.clone(),
                vec![crate::material_verify::VerifyFile {
                    relative_path: attachment.relative_path.clone(),
                    expected_size: attachment.size,
                    expected_sha256: None,
                }],
            )
            .unwrap();
        let response = loop {
            if let Some(response) = verifier.try_recv() {
                break response;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let verified = response.files[0].as_ref().unwrap();
        assert!(!verified.matches(
            &attachment.relative_path,
            attachment.size,
            &imported.content_sha256
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn add_inverse_defers_file_cleanup_until_called_after_durability() {
        let (mut store, directory, task, attachment, imported) = fixture();
        let plan = prepare_material_add(
            &store,
            &directory,
            task,
            vec![attachment.clone()],
            vec![imported],
        )
        .unwrap();
        let applied = apply_material_add(&mut store, &directory, &plan).unwrap();
        let path = crate::attachments::resolve(&directory, &attachment).unwrap();
        let effect =
            undo_material_edit(&mut store, &directory, &MaterialUndo::Add(applied.undo)).unwrap();
        assert!(path.exists(), "domain inverse must not delete before durable save");
        assert_eq!(effect.cleanup.len(), 1);
        cleanup_imported_copies(&directory, &store, &effect.cleanup, &[]);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn cleanup_keeps_a_copy_referenced_by_retained_run_history() {
        let (mut store, directory, task, attachment, imported) = fixture();
        let plan = prepare_material_add(
            &store,
            &directory,
            task,
            vec![attachment.clone()],
            vec![imported],
        )
        .unwrap();
        let applied = apply_material_add(&mut store, &directory, &plan).unwrap();
        let mut run = crate::research::prepare_run(&store, task).unwrap_or_else(|_| {
            let run = crate::domain::AgentRun {
                id: 500,
                task_id: task,
                task_title: "Material task".into(),
                directory: String::new(),
                prompt: String::new(),
                started_at: String::new(),
                queued_at: String::new(),
                workspace_source: None,
                ended_at: None,
                session_id: None,
                exit_code: None,
                agent: "test".into(),
                model: "test".into(),
                stage: Stage::Planned,
                status: "Succeeded".into(),
                logs: vec![],
                output: "history".into(),
                attachments: vec![],
                research_kind: None,
                sources: vec![],
                input_signature: String::new(),
                automation_occurrence_id: None,
                planning_owner: None,
                deadline_at: None,
                auto_retry_attempt: 0,
                retry_not_before: None,
                project_plan: None,
            };
            run
        });
        run.attachments = vec![attachment.clone()];
        store.push_run(run);
        let effect =
            undo_material_edit(&mut store, &directory, &MaterialUndo::Add(applied.undo)).unwrap();
        cleanup_imported_copies(&directory, &store, &effect.cleanup, &[]);
        assert!(crate::attachments::resolve(&directory, &attachment).unwrap().exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn remove_retry_locates_the_original_identity_after_reordering() {
        let (mut store, directory, task, a, imported_a) = fixture();
        let b = crate::attachments::import_bytes(
            &directory,
            "other.pdf",
            b"%PDF-1.4\nother material\n",
        )
        .unwrap();
        let imported_b = imported_copy(&directory, task, &b).unwrap();
        let plan = prepare_material_add(
            &store,
            &directory,
            task,
            vec![a.clone(), b.clone()],
            vec![imported_a, imported_b],
        )
        .unwrap();
        apply_material_add(&mut store, &directory, &plan).unwrap();
        let remove = prepare_material_remove(&store, &directory, task, 0).unwrap();
        let target = remove.target_identity().clone();
        store.task_mut(task).unwrap().attachments.swap(0, 1);
        let retry = prepare_material_remove_by_identity(&store, &directory, task, &target).unwrap();
        assert_eq!(retry.index(), 1);
        apply_material_remove(&mut store, &directory, &retry).unwrap();
        assert_eq!(store.task(task).unwrap().attachments, vec![b]);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn remove_inverse_rejects_missing_or_changed_bytes_without_mutation() {
        let (mut store, directory, task, attachment, imported) = fixture();
        let plan = prepare_material_add(
            &store,
            &directory,
            task,
            vec![attachment.clone()],
            vec![imported],
        )
        .unwrap();
        apply_material_add(&mut store, &directory, &plan).unwrap();
        let remove = prepare_material_remove(&store, &directory, task, 0).unwrap();
        let applied = apply_material_remove(&mut store, &directory, &remove).unwrap();
        let path = crate::attachments::resolve(&directory, &attachment).unwrap();
        let before = store.clone();
        fs::remove_file(&path).unwrap();
        assert!(matches!(
            undo_material_edit(
                &mut store,
                &directory,
                &MaterialUndo::Remove(applied.undo.clone())
            ),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);

        // Restore a same-size changed file and confirm it is still rejected.
        fs::write(&path, b"%PDF-1.4\nchanged material\n").unwrap();
        assert!(matches!(
            undo_material_edit(
                &mut store,
                &directory,
                &MaterialUndo::Remove(applied.undo)
            ),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before);
        let _ = fs::remove_dir_all(directory);
    }
}
