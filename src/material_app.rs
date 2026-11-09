//! T13 S3d live material add/remove adapter: content identity, approval
//! companions, durable failure retention, scoped Undo and owned-copy cleanup.

use crate::app::TaskApp;
use crate::attachments::Attachment;
use crate::scoped_material::{
    apply_material_add, apply_material_remove, prepare_material_add,
    prepare_material_remove, ImportedCopy, MaterialAddPlan, MaterialRemovePlan, MaterialUndo,
};
use crate::material_verify::{VerifiedFile, VerifyFile};
use crate::scoped_material::{take_cleanup_obligation, CleanupObligation};
use crate::task_edit_app::TaskSaveOutcome;
use std::path::PathBuf;
use gpui_kit::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PreparedMaterialSave {
    Add(MaterialAddPlan),
    Remove(MaterialRemovePlan),
}

impl PreparedMaterialSave {
    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Add(plan) => plan.id(),
            Self::Remove(plan) => plan.id(),
        }
    }

    pub(crate) fn impact(&self) -> Option<&crate::research_input::ResearchInputImpact> {
        match self {
            Self::Add(plan) => plan.impact(),
            Self::Remove(plan) => plan.impact(),
        }
    }

    pub(crate) fn reference_paths(&self) -> Vec<String> {
        match self {
            Self::Add(plan) => plan
                .attachments()
                .iter()
                .map(|attachment| attachment.relative_path.clone())
                .collect(),
            Self::Remove(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingMaterialEdit {
    pub prepared: PreparedMaterialSave,
    pub root: PathBuf,
}

pub(crate) enum PendingMaterialJob {
    PrepareRemove {
        job: u64,
        task: u64,
        index: usize,
        before: Vec<Attachment>,
    },
    PrepareAdd {
        job: u64,
        task: u64,
        attachments: Vec<Attachment>,
        imported: Vec<crate::scoped_material::ImportedCopy>,
        root: PathBuf,
    },
    UndoRemove {
        job: u64,
        undo: MaterialUndo,
    },
    Cleanup {
        job: u64,
        copies: Vec<crate::scoped_material::ImportedCopy>,
        root: PathBuf,
    },
    Delete {
        job: u64,
        copies: Vec<crate::scoped_material::ImportedCopy>,
        root: PathBuf,
    },
}



impl TaskApp {
    fn material_data_dir(&self) -> Result<PathBuf, String> {
        self.data_dir()
    }

    pub(crate) fn prepare_material_add_save(
        &self,
        id: u64,
        attachments: Vec<Attachment>,
        imported: Vec<ImportedCopy>,
    ) -> Result<PreparedMaterialSave, String> {
        let data_dir = self.material_data_dir()?;
        prepare_material_add(&self.store, &data_dir, id, attachments, imported)
            .map(PreparedMaterialSave::Add)
            .map_err(material_error_text)
    }

    pub(crate) fn request_material_remove(
        &mut self,
        id: u64,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.attachment_import.is_some() {
            self.show_save_error("Wait for the current material import to finish.", cx);
            return false;
        }
        if self.pending_material_edit.is_some()
            || self.has_material_review()
            || self.pending_material_job.is_some()
        {
            self.show_save_error(
                "Resolve the pending material change before removing another material.",
                cx,
            );
            return false;
        }
        let data_dir = match self.material_data_dir() {
            Ok(dir) => dir,
            Err(error) => {
                self.show_save_error(error, cx);
                return false;
            }
        };
        if self
            .store
            .task(id)
            .and_then(|task| task.attachments.get(index))
            .is_none_or(|attachment| attachment.content_sha256.is_empty())
        {
            let Some(size) = self
                .store
                .task(id)
                .and_then(|task| task.attachments.get(index))
                .map(|attachment| attachment.size)
            else {
                self.show_save_error("Choose a material that is still attached.", cx);
                return false;
            };
            let Some(relative_path) = self
                .store
                .task(id)
                .and_then(|task| task.attachments.get(index))
                .map(|attachment| attachment.relative_path.clone())
            else {
                self.show_save_error("Choose a material that is still attached.", cx);
                return false;
            };
            let before = self
                .store
                .task(id)
                .map(|task| task.attachments.clone())
                .unwrap_or_default();
            match self.material_verifier.submit(
                data_dir,
                vec![VerifyFile {
                    relative_path,
                    expected_size: size,
                    expected_sha256: None,
                }],
            ) {
                Ok(job) => {
                    self.pending_material_job = Some(PendingMaterialJob::PrepareRemove {
                        job,
                        task: id,
                        index,
                        before,
                    });
                    self.toast = "Verifying material before removal…".into();
                    self.start_pump(cx);
                    cx.notify();
                    return false;
                }
                Err(error) => {
                    self.show_save_error(error, cx);
                    return false;
                }
            }
        }
        let prepared = match prepare_material_remove(&self.store, &data_dir, id, index) {
            Ok(plan) => PreparedMaterialSave::Remove(plan),
            Err(error) => {
                self.show_save_error(&material_error_text(error), cx);
                return false;
            }
        };
        self.dispatch_material_save(prepared, cx).is_saved()
    }

    pub(crate) fn dispatch_material_save(
        &mut self,
        prepared: PreparedMaterialSave,
        cx: &mut Context<Self>,
    ) -> crate::task_edit_app::TaskSaveOutcome {
        if self.pending_material_edit.is_some()
            || self.has_material_review()
            || self.pending_material_job.is_some()
        {
            let message =
                "Resolve the pending material change before starting another one.".to_string();
            self.show_save_error(&message, cx);
            return crate::task_edit_app::TaskSaveOutcome::Rejected(message);
        }
        if let Some(impact) = prepared.impact().cloned()
            && impact.confirmation_required
        {
            self.pending_material_edit = Some(PendingMaterialEdit {
                prepared: prepared.clone(),
                root: self.material_data_dir().unwrap_or_default(),
            });
            self.research_save_impact = Some(crate::research_input_app::PendingResearchSave {
                task: prepared.id(),
                impact,
                prepared: crate::research_input_app::PendingPreparedSave::Materials(prepared),
            });
            cx.notify();
            return crate::task_edit_app::TaskSaveOutcome::PendingImpact;
        }
        self.commit_prepared_material(prepared, cx)
    }

    pub(crate) fn has_material_review(&self) -> bool {
        self.research_save_impact.as_ref().is_some_and(|pending| {
            matches!(
                pending.prepared,
                crate::research_input_app::PendingPreparedSave::Materials(_)
            )
        })
    }

    pub(crate) fn commit_prepared_material(
        &mut self,
        prepared: PreparedMaterialSave,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        if prepared
            .reference_paths()
            .iter()
            .any(|path| self.material_cleanup_reserved.contains(path))
        {
            let message = String::from(
                "Material cleanup is still in progress for this file. Try again shortly.",
            );
            self.show_save_error(&message, cx);
            return TaskSaveOutcome::Rejected(message);
        }
        let data_dir = match self.material_data_dir() {
            Ok(path) => path,
            Err(error) => {
                self.pending_material_edit = Some(PendingMaterialEdit {
                    prepared: prepared.clone(),
                    root: PathBuf::new(),
                });
                self.storage_error = error.clone();
                self.show_save_error(error, cx);
                return TaskSaveOutcome::PendingDurability(
                    "Workspace storage is unavailable. Nothing was saved.".into(),
                );
            }
        };
        let mut staged = self.store.clone();
        let (undo, impact, change_set) = match &prepared {
            PreparedMaterialSave::Add(plan) => match apply_material_add(&mut staged, &data_dir, plan)
            {
                Ok(applied) => (
                    MaterialUndo::Add(applied.undo),
                    applied.impact,
                    applied.change_set,
                ),
                Err(error) => {
                    let message = material_error_text(error);
                    self.pending_material_edit = Some(PendingMaterialEdit { prepared, root: data_dir.clone() });
                    self.show_save_error(&message, cx);
                    return TaskSaveOutcome::Rejected(message);
                }
            },
            PreparedMaterialSave::Remove(plan) => {
                match apply_material_remove(&mut staged, &data_dir, plan) {
                    Ok(applied) => (
                        MaterialUndo::Remove(applied.undo),
                        applied.impact,
                        applied.change_set,
                    ),
                    Err(error) => {
                        let message = material_error_text(error);
                        self.pending_material_edit = Some(PendingMaterialEdit { prepared, root: data_dir.clone() });
                        self.show_save_error(&message, cx);
                        return TaskSaveOutcome::Rejected(message);
                    }
                }
            }
        };
        let message = match &prepared {
            PreparedMaterialSave::Add(_) => "Materials added",
            PreparedMaterialSave::Remove(_) => "Material removed",
        };
        let completion = crate::live_save_app::LiveCompletion::Material {
            undo: undo.clone(),
            message: message.into(),
        };
        if let Err(error) = self.live_can_accept_primary() {
            self.storage_error = error.clone();
            self.show_save_error(&error, cx);
            return TaskSaveOutcome::Rejected(error);
        }
        let acceptance = match self.save_staged_store(&staged, completion, cx) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                self.storage_error = error.clone();
                self.pending_material_edit = Some(PendingMaterialEdit { prepared, root: data_dir.clone() });
                self.show_save_error(&error, cx);
                return if self.storage.is_none() {
                    TaskSaveOutcome::PendingDurability(error)
                } else {
                    TaskSaveOutcome::RetryableFailure(error)
                };
            }
        };
        if acceptance.is_durable() {
            // The Material completion already pushed the inverse, cleared the
            // pending owner and set the durable toast; only a non-live save
            // still needs the staged store adopted into the live tree.
            if !self.live_persistence_active() {
                self.adopt_saved_store(staged);
            }
        } else {
            self.pending_material_edit = None;
            self.toast = format!("Saving {message}…");
        }
        let _ = change_set;
        cx.notify();
        if acceptance.is_durable() {
            TaskSaveOutcome::Saved
        } else {
            TaskSaveOutcome::Saving
        }
    }

    pub(crate) fn refresh_prepared_material(
        &self,
        prepared: PreparedMaterialSave,
    ) -> Result<PreparedMaterialSave, String> {
        let data_dir = self.material_data_dir()?;
        // The imported copies belong to the root captured when the change was
        // prepared. A workspace switch keeps the pending intent until the user
        // returns; it must never re-prepare against another workspace's store.
        if let Some(pending) = &self.pending_material_edit
            && !pending.root.as_os_str().is_empty()
            && pending.root != data_dir
        {
            return Err(
                "The workspace changed while this material change was pending. Return to the original workspace to finish it."
                    .into(),
            );
        }
        match prepared {
            PreparedMaterialSave::Add(plan) => prepare_material_add(
                &self.store,
                &data_dir,
                plan.id(),
                plan.attachments().to_vec(),
                plan.imported().to_vec(),
            )
            .map(PreparedMaterialSave::Add)
            .map_err(material_error_text),
            PreparedMaterialSave::Remove(plan) => {
                crate::scoped_material::prepare_material_remove_by_identity(
                    &self.store,
                    &data_dir,
                    plan.id(),
                    plan.target_identity(),
                )
                .map(PreparedMaterialSave::Remove)
                .map_err(material_error_text)
            }
        }
    }

    pub(crate) fn retry_material_save(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TaskSaveOutcome {
        let Some(pending) = self.pending_material_edit.clone() else {
            return TaskSaveOutcome::Rejected("There is no pending material change to retry.".into());
        };
        let prepared = match self.refresh_prepared_material(pending.prepared.clone()) {
            Ok(prepared) => prepared,
            Err(error) => {
                // Keep the original intent and imported-copy ownership until the
                // user explicitly discards it. A transient/conflicting refresh
                // must not erase the retryable pending edit.
                self.show_save_error(&error, cx);
                return TaskSaveOutcome::Rejected(error);
            }
        };
        if let Some(fresh) = prepared.impact().cloned()
            && Some(&fresh) != pending.prepared.impact()
        {
            self.pending_material_edit = Some(PendingMaterialEdit {
                prepared: prepared.clone(),
                root: pending.root.clone(),
            });
            self.research_save_impact = Some(crate::research_input_app::PendingResearchSave {
                task: prepared.id(),
                impact: fresh,
                prepared: crate::research_input_app::PendingPreparedSave::Materials(prepared),
            });
            self.toast = "The impact changed · review the updated effect before saving".into();
            cx.notify();
            return TaskSaveOutcome::PendingImpact;
        }
        // Keep the stable pending slot aligned with the refreshed identity so a
        // successful retry clears exactly this operation, not a different one.
        self.pending_material_edit = Some(PendingMaterialEdit {
            prepared: prepared.clone(),
            root: pending.root.clone(),
        });
        self.commit_prepared_material(prepared, cx)
    }

    pub(crate) fn apply_material_undo(
        &mut self,
        undo: MaterialUndo,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(data_dir) = self.material_data_dir() else {
            self.error = "Workspace storage is unavailable. Undo did not run.".into();
            return false;
        };
        let mut staged = self.store.clone();
        let effect =
            match crate::scoped_material::undo_material_edit_verified(&mut staged, &undo) {
                Ok(effect) => effect,
                Err(error) => {
                    self.error = material_error_text(error);
                    return false;
                }
            };
        let completion = crate::live_save_app::LiveCompletion::MaterialUndo {
            cleanup: effect.cleanup.clone(),
        };
        let acceptance = match self.save_staged_store(&staged, completion, cx) {
            Ok(acceptance) => acceptance,
            Err(error) => {
                self.storage_error = error.clone();
                self.show_save_error(error, cx);
                return false;
            }
        };
        if acceptance.is_durable() {
            self.adopt_saved_store(staged);
            self.undo_invalidation = None;
            crate::appearance::apply(self.store.appearance(), None, cx);
            self.toast = "Last change undone".into();
            self.error.clear();
            if !effect.cleanup.is_empty() {
                self.submit_material_cleanup(effect.cleanup, cx);
            }
        } else {
            self.toast = "Saving undo…".into();
        }
        cx.notify();
        true
    }

    pub(crate) fn submit_material_undo_verify(
        &mut self,
        undo: MaterialUndo,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_material_job.is_some() {
            self.undo.push_material(undo);
            self.error = "Material verification is busy. Try again shortly.".into();
            return false;
        }
        let Some(target) = undo.target_identity().cloned() else {
            return self.apply_material_undo(undo, cx);
        };
        let Ok(root) = self.material_data_dir() else {
            self.undo.push_material(undo);
            self.error = "Workspace storage is unavailable. Undo did not run.".into();
            return false;
        };
        let files = vec![VerifyFile {
            relative_path: target.relative_path.clone(),
            expected_size: target.size,
            expected_sha256: None,
        }];
        match self.material_verifier.submit(root, files) {
            Ok(job) => {
                self.pending_material_job = Some(PendingMaterialJob::UndoRemove { job, undo });
                self.toast = "Checking material before undo…".into();
                self.start_pump(cx);
                cx.notify();
                true
            }
            Err(error) => {
                self.undo.push_material(undo);
                self.error = error;
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn submit_material_cleanup(
        &mut self,
        copies: Vec<crate::scoped_material::ImportedCopy>,
        cx: &mut Context<Self>,
    ) -> bool {
        if copies.is_empty() {
            return true;
        }
        let Ok(root) = self.material_data_dir() else {
            self.error = "Workspace storage is unavailable; material cleanup was deferred.".into();
            self.pending_cleanup_retry.push(CleanupObligation {
                root: PathBuf::new(),
                copies,
            });
            return false;
        };
        self.submit_material_cleanup_for_root(root, copies, cx)
    }

    pub(crate) fn submit_material_cleanup_for_root(
        &mut self,
        root: PathBuf,
        copies: Vec<crate::scoped_material::ImportedCopy>,
        cx: &mut Context<Self>,
    ) -> bool {
        if copies.is_empty() {
            return true;
        }
        if self.pending_material_job.is_some() {
            self.error = "Material verification is busy. Try again shortly.".into();
            self.pending_cleanup_retry.push(CleanupObligation { root, copies });
            return false;
        }
        let files = copies
            .iter()
            .map(|copy| VerifyFile {
                relative_path: copy.relative_path.clone(),
                expected_size: copy.size,
                expected_sha256: None,
            })
            .collect();
        match self.material_verifier.submit(root.clone(), files) {
            Ok(job) => {
                self.pending_material_job = Some(PendingMaterialJob::Cleanup {
                    job,
                    copies,
                    root,
                });
                self.start_pump(cx);
                true
            }
            Err(error) => {
                self.error = error;
                self.pending_cleanup_retry.push(CleanupObligation { root, copies });
                cx.notify();
                false
            }
        }
    }

    /// Submit an add verification bound to the root captured when the import
    /// started. A workspace switch during copy must not rebind the copies.
    pub(crate) fn submit_material_add_verify_for_root(
        &mut self,
        root: PathBuf,
        task: u64,
        attachments: Vec<Attachment>,
        imported: Vec<crate::scoped_material::ImportedCopy>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_material_job.is_some() {
            self.error = "Material verification is busy. Try again shortly.".into();
            cx.notify();
            return false;
        }
        if self.material_data_dir().ok().as_ref() != Some(&root) {
            self.error =
                "The workspace changed before materials were verified. Nothing was attached.".into();
            cx.notify();
            return false;
        }
        let files = imported
            .iter()
            .map(|copy| VerifyFile {
                relative_path: copy.relative_path.clone(),
                expected_size: copy.size,
                expected_sha256: None,
            })
            .collect();
        match self.material_verifier.submit(root.clone(), files) {
            Ok(job) => {
                self.pending_material_job = Some(PendingMaterialJob::PrepareAdd {
                    job,
                    task,
                    attachments,
                    imported,
                    root,
                });
                self.toast = "Verifying materials…".into();
                self.start_pump(cx);
                cx.notify();
                true
            }
            Err(error) => {
                self.error = error;
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn poll_material_jobs(&mut self, cx: &mut Context<Self>) {
        while let Some(response) = self.material_verifier.try_recv() {
            let Some(job) = self.pending_material_job.take() else {
                continue;
            };
            match job {
                PendingMaterialJob::PrepareRemove {
                    job,
                    task,
                    index,
                    before,
                } => {
                    if job != response.id {
                        self.pending_material_job = Some(PendingMaterialJob::PrepareRemove {
                            job,
                            task,
                            index,
                            before,
                        });
                        continue;
                    }
                    let root_matches = self
                        .material_data_dir()
                        .is_ok_and(|root| root == response.root);
                    let current = self
                        .store
                        .task(task)
                        .map(|task| task.attachments.clone());
                    let same_list = current.as_ref() == Some(&before);
                    let selected = before.get(index);
                    let Some(Ok(file)) = response.files.first() else {
                        self.error = "The material could not be verified.".into();
                        continue;
                    };
                    let identity_matches = selected.is_some_and(|attachment| {
                        attachment.relative_path == file.relative_path()
                            && attachment.size == file.size()
                    });
                    if !root_matches || !same_list || !identity_matches {
                        self.error =
                            "The material list changed while it was being verified. Nothing was removed."
                                .into();
                        cx.notify();
                        continue;
                    }
                    if self
                        .store
                        .task(task)
                        .and_then(|task| task.attachments.get(index))
                        .is_none_or(|attachment| {
                            attachment.relative_path != file.relative_path()
                                || attachment.size != file.size()
                        })
                    {
                        self.error =
                            "The selected material changed while it was being verified.".into();
                        cx.notify();
                        continue;
                    }
                    match crate::scoped_material::prepare_material_remove_verified(
                        &self.store,
                        &response.root,
                        task,
                        index,
                        file.content_sha256(),
                    ) {
                        Ok(plan) => {
                            let _ = self.dispatch_material_save(
                                PreparedMaterialSave::Remove(plan),
                                cx,
                            );
                        }
                        Err(error) => self.error = material_error_text(error),
                    }
                }
                PendingMaterialJob::PrepareAdd {
                    job,
                    task,
                    attachments,
                    imported,
                    root,
                } => {
                    if job != response.id {
                        self.pending_material_job = Some(PendingMaterialJob::PrepareAdd {
                            job,
                            task,
                            attachments,
                            imported,
                            root,
                        });
                        continue;
                    }
                    let current_root = self.material_data_dir().ok();
                    if response.root != root || current_root.as_ref() != Some(&root) {
                        self.submit_material_cleanup_for_root(
                            root,
                            imported,
                            cx,
                        );
                        self.error =
                            "The workspace changed while materials were being verified. Nothing was attached."
                                .into();
                        cx.notify();
                        continue;
                    }
                    let verified = imported.iter().all(|copy| {
                        response.files.iter().any(|result| {
                            result.as_ref().is_ok_and(|file| {
                                file.matches(&copy.relative_path, copy.size, &copy.content_sha256)
                            })
                        })
                    });
                    if verified {
                        let cleanup = imported.clone();
                        match self.prepare_material_add_save(
                            task,
                            attachments,
                            imported,
                        ) {
                            Ok(prepared) => {
                                let outcome = self.dispatch_material_save(prepared, cx);
                                if matches!(
                                    outcome,
                                    TaskSaveOutcome::Rejected(_)
                                ) && self.pending_material_edit.is_none()
                                {
                                    self.submit_material_cleanup_for_root(
                                        root.clone(),
                                        cleanup,
                                        cx,
                                    );
                                }
                            }
                            Err(error) => {
                                self.submit_material_cleanup_for_root(root.clone(), cleanup, cx);
                                self.error = error;
                            }
                        }
                    } else {
                        self.submit_material_cleanup_for_root(root, imported, cx);
                        self.error =
                            "A material changed while it was being verified. Nothing was attached."
                                .into();
                    }
                }
                PendingMaterialJob::UndoRemove { job, undo } => {
                    if job != response.id {
                        self.pending_material_job =
                            Some(PendingMaterialJob::UndoRemove { job, undo });
                        continue;
                    }
                    let Some(target) = undo.target_identity().cloned() else {
                        if !self.apply_material_undo(undo.clone(), cx) {
                            self.undo.push_material(undo);
                        }
                        continue;
                    };
                    let matches = response
                        .files
                        .first()
                        .and_then(|result| result.as_ref().ok())
                        .is_some_and(|file: &VerifiedFile| {
                            file.relative_path() == target.relative_path
                                && file.size() == target.size
                                && file.content_sha256() == target.content_sha256
                        });
                    if matches {
                        if !self.apply_material_undo(undo.clone(), cx) {
                            self.undo.push_material(undo);
                        }
                    } else {
                        self.undo.push_material(undo);
                        self.error =
                            "The material file changed or is missing. Undo did not run."
                                .into();
                        cx.notify();
                    }
                }
                PendingMaterialJob::Cleanup { job, copies, root } => {
                    if job != response.id {
                        self.pending_material_job = Some(PendingMaterialJob::Cleanup {
                            job,
                            copies,
                            root,
                        });
                        continue;
                    }
                    let current_root = self.material_data_dir().ok();
                    if response.root != root || current_root.as_ref() != Some(&root) {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root,
                            copies,
                        });
                        self.error =
                            "Material cleanup was paused because the workspace changed; copies were retained."
                                .into();
                        cx.notify();
                        continue;
                    }
                    if response.files.len() != copies.len() {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root,
                            copies,
                        });
                        self.error =
                            "Material verification returned an incomplete result; copies were retained."
                                .into();
                        cx.notify();
                        continue;
                    }
                    let extra = self.material_reference_paths();
                    let mut delete_copies = Vec::new();
                    let mut retry_copies = Vec::new();
                    for (copy, result) in copies.into_iter().zip(response.files.into_iter()) {
                        match result {
                            Ok(file) if file.matches(
                                &copy.relative_path,
                                copy.size,
                                &copy.content_sha256,
                            ) => {
                                if !extra.iter().any(|path| path == &copy.relative_path)
                                    && !crate::scoped_material::referenced_by_store(
                                        &self.store,
                                        &copy.relative_path,
                                    )
                                {
                                    delete_copies.push(copy);
                                }
                            }
                            Ok(_) => {
                                // Bytes changed: keep the file, do not delete it.
                            }
                            Err(error) if transient_material_io(&error) => {
                                retry_copies.push(copy);
                            }
                            Err(_) => {
                                // Missing or changed file: safe retention.
                            }
                        }
                    }
                    if !retry_copies.is_empty() {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root: root.clone(),
                            copies: retry_copies,
                        });
                        self.error =
                            "Some material copies could not be verified and were retained for retry."
                                .into();
                    }
                    let files = delete_copies
                        .iter()
                        .map(|copy| VerifyFile {
                            relative_path: copy.relative_path.clone(),
                            expected_size: copy.size,
                            expected_sha256: Some(copy.content_sha256.clone()),
                        })
                        .collect::<Vec<_>>();
                    if files.is_empty() {
                        continue;
                    }
                    let reserved = delete_copies
                        .iter()
                        .map(|copy| copy.relative_path.clone())
                        .collect::<Vec<_>>();
                    match self.material_verifier.submit_delete(root.clone(), files) {
                        Ok(job) => {
                            self.material_cleanup_reserved = reserved;
                            self.pending_material_job = Some(PendingMaterialJob::Delete {
                                job,
                                copies: delete_copies,
                                root,
                            });
                            self.start_pump(cx);
                        }
                        Err(error) => {
                            self.pending_cleanup_retry.push(CleanupObligation {
                                root,
                                copies: delete_copies,
                            });
                            self.error =
                                format!("{error} Material copies were retained.");
                        }
                    }
                }
                PendingMaterialJob::Delete { job, copies, root } => {
                    if job != response.id {
                        self.pending_material_job = Some(PendingMaterialJob::Delete {
                            job,
                            copies,
                            root,
                        });
                        continue;
                    }
                    self.material_cleanup_reserved.clear();
                    let current_root = self.material_data_dir().ok();
                    if response.root != root || current_root.as_ref() != Some(&root) {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root,
                            copies,
                        });
                        self.error =
                            "Material cleanup finished after the workspace changed; copies remain tracked for their original workspace."
                                .into();
                        continue;
                    }
                    if response.files.len() != copies.len() {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root,
                            copies,
                        });
                        self.error =
                            "Material cleanup returned an incomplete result; copies were retained."
                                .into();
                        continue;
                    }
                    let mut failed = Vec::new();
                    for (copy, result) in copies.into_iter().zip(response.files.into_iter()) {
                        match result {
                            Ok(file)
                                if file.matches(
                                    &copy.relative_path,
                                    copy.size,
                                    &copy.content_sha256,
                                ) => {}
                            _ => failed.push(copy),
                        }
                    }
                    if failed.is_empty() {
                        // All submitted copies were verified and unlinked.
                    } else {
                        self.pending_cleanup_retry.push(CleanupObligation {
                            root,
                            copies: failed,
                        });
                        self.error =
                            "Some material copies could not be cleaned up and were retained."
                                .into();
                    }
                }
            }
            cx.notify();
        }
        if self.pending_material_job.is_none()
            && let Some(current) = self.material_data_dir().ok()
            && let Some(obligation) =
                take_cleanup_obligation(&mut self.pending_cleanup_retry, &current)
        {
            self.submit_material_cleanup_for_root(
                obligation.root,
                obligation.copies,
                cx,
            );
        }
    }

    pub(crate) fn cancel_pending_material(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_material_edit.take() else {
            return;
        };
        if let PreparedMaterialSave::Add(plan) = pending.prepared {
            let copies = plan.imported().to_vec();
            if pending.root.as_os_str().is_empty() {
                self.pending_cleanup_retry.push(CleanupObligation {
                    root: pending.root,
                    copies,
                });
                self.error =
                    "Material cleanup ownership is unknown; copies were retained.".into();
            } else {
                self.submit_material_cleanup_for_root(pending.root, copies, cx);
            }
        }
    }

    pub(crate) fn material_reference_paths(&self) -> Vec<String> {
        self.material_reference_paths_excluding(None)
    }

    pub(crate) fn material_reference_paths_excluding(
        &self,
        exclude: Option<&PreparedMaterialSave>,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        if let Some(pending) = &self.pending_material_edit
            && exclude != Some(&pending.prepared)
        {
            paths.extend(pending.prepared.reference_paths());
        }
        if let Some(review) = &self.research_save_impact
            && let crate::research_input_app::PendingPreparedSave::Materials(prepared) =
                &review.prepared
            && exclude != Some(prepared)
        {
            paths.extend(prepared.reference_paths());
        }
        paths.extend(self.undo.material_reference_paths());
        if let Some(dispatch) = &self.pending_dispatch {
            for file in &dispatch.files {
                paths.push(file.to_string_lossy().into_owned());
                if let Some(name) = file.file_name().and_then(|name| name.to_str()) {
                    paths.push(format!("attachments/{name}"));
                }
            }
        }
        paths
    }
}

fn transient_material_io(error: &str) -> bool {
    error.contains("Cannot read material")
        || error.contains("Cannot inspect material")
        || error.contains("Cannot remove material copy")
}

fn material_error_text(error: crate::scoped_edit::ScopedEditError) -> String {
    match error {
        crate::scoped_edit::ScopedEditError::Rejected(message)
        | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
    }
}
