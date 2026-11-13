//! T21 Phase 2a/2c application seams: impact confirmation and previous versions.
//!
//! This module never owns a second mutation engine. A confirmed save commits the
//! accepted `scoped_edit` plan once through `task_edit_app`; the impact review
//! only decides whether that typed path may run. Reader text and quality come
//! from T06.

use crate::app::TaskApp;
use crate::attachments::Attachment;
use crate::domain::Store;
use crate::research::ResearchKind;
use crate::research_input::{self, PreviousVersion, ResearchInputImpact, StepInputState};
use crate::result_presentation::{ResultPresentation, present_run};
use gpui_kit::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PendingPreparedSave {
    Task(crate::task_edit_app::PreparedTaskSave),
    Comment(crate::task_edit_app::PreparedCommentSave),
    Materials(crate::material_app::PreparedMaterialSave),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingResearchSave {
    pub task: u64,
    pub impact: ResearchInputImpact,
    pub prepared: PendingPreparedSave,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreviousVersionEntry {
    pub task: u64,
    pub task_title: String,
    pub kind: ResearchKind,
    pub version: PreviousVersion,
}

impl TaskApp {
    /// True while a Save is waiting for the impact review. T10 treats this as
    /// pending user input, never as a saved state.
    pub(crate) fn research_save_is_pending(&self) -> bool {
        self.research_save_impact.is_some()
    }

    /// Save entry point used by the editor button. It prepares the accepted
    /// scoped plan against the current store and either opens the blocking
    /// review or commits the typed delta once.
    pub(crate) fn request_save_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        matches!(
            self.request_save_draft_outcome(window, cx),
            crate::task_edit_app::TaskSaveOutcome::Saved
        )
    }

    /// Typed save result used by the dirty-exit decision so a genuine pending
    /// receipt is not reported as a rejection.
    pub(crate) fn request_save_draft_outcome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> crate::task_edit_app::TaskSaveOutcome {
        use crate::task_edit_app::TaskSaveOutcome;
        if self.research_save_impact.is_some() {
            return TaskSaveOutcome::Rejected(
                "Review the pending save impact first.".into(),
            );
        }
        self.pending_task_edit = None;
        #[cfg(feature = "qa")]
        let prepare_started = std::time::Instant::now();
        let prepared = match self.prepare_task_editor_save(cx) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.show_save_error(error.clone(), cx);
                return TaskSaveOutcome::Rejected(error);
            }
        };
        if let Some(impact) = prepared.plan.impact().cloned()
            && impact.confirmation_required
        {
            let task = prepared.plan.id();
            self.research_save_opener = window.focused(cx);
            self.research_save_impact = Some(PendingResearchSave {
                task,
                impact,
                prepared: PendingPreparedSave::Task(prepared),
            });
            window.focus(&self.dialog_first_focus, cx);
            cx.notify();
            return TaskSaveOutcome::PendingImpact;
        }
        #[cfg(feature = "qa")]
        eprintln!(
            "LIVE_DIAG prepare_us={}",
            prepare_started.elapsed().as_micros()
        );
        #[cfg(feature = "qa")]
        let commit_started = std::time::Instant::now();
        let outcome = self.commit_prepared_task_edit(prepared, window, cx);
        #[cfg(feature = "qa")]
        eprintln!(
            "LIVE_DIAG commit_us={}",
            commit_started.elapsed().as_micros()
        );
        outcome
    }

    /// Confirm the reviewed impact. A changed graph, approval, field or runtime
    /// condition is re-reviewed against the current store instead of saving
    /// under the old confirmation.
    pub(crate) fn confirm_research_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.research_save_impact.clone() else {
            return false;
        };
        if let PendingPreparedSave::Materials(prepared) = pending.prepared.clone() {
            let fresh = match self.refresh_prepared_material(prepared) {
                Ok(fresh) => fresh,
                Err(error) => {
                    self.show_save_error(error, cx);
                    return false;
                }
            };
            if let Some(impact) = fresh.impact().cloned()
                && Some(&impact) != Some(&pending.impact)
            {
                self.pending_material_edit = Some(
                    crate::material_app::PendingMaterialEdit {
                        prepared: fresh.clone(),
                        root: self
                            .pending_material_edit
                            .as_ref()
                            .map(|pending| pending.root.clone())
                            .unwrap_or_default(),
                    },
                );
                self.research_save_impact = Some(PendingResearchSave {
                    task: fresh.id(),
                    impact,
                    prepared: PendingPreparedSave::Materials(fresh),
                });
                self.toast = "The impact changed · review the updated effect before saving".into();
                cx.notify();
                return false;
            }
            let outcome = self.commit_prepared_material(fresh, cx);
            match outcome {
                crate::task_edit_app::TaskSaveOutcome::Saved
                | crate::task_edit_app::TaskSaveOutcome::NoChanges => {
                    self.research_save_impact = None;
                    self.pending_material_edit = None;
                    true
                }
                crate::task_edit_app::TaskSaveOutcome::Saving
                | crate::task_edit_app::TaskSaveOutcome::PendingDurability(_)
                | crate::task_edit_app::TaskSaveOutcome::RetryableFailure(_) => {
                    cx.notify();
                    false
                }
                crate::task_edit_app::TaskSaveOutcome::PendingImpact
                | crate::task_edit_app::TaskSaveOutcome::Rejected(_) => {
                    self.research_save_impact = None;
                    cx.notify();
                    false
                }
            }
        } else if let PendingPreparedSave::Comment(prepared) = pending.prepared {
            let plan = match crate::scoped_edit::prepare_comment_edit(
                &self.store,
                prepared.task,
                &prepared.body,
            ) {
                Ok(plan) => plan,
                Err(error) => {
                    let message = match error {
                        crate::scoped_edit::ScopedEditError::Rejected(message)
                        | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
                    };
                    self.show_save_error(message, cx);
                    return false;
                }
            };
            if let Some(fresh) = plan.impact().cloned()
                && fresh != pending.impact
            {
                self.research_save_impact = Some(PendingResearchSave {
                    task: pending.task,
                    impact: fresh,
                    prepared: PendingPreparedSave::Comment(
                        crate::task_edit_app::PreparedCommentSave {
                            task: prepared.task,
                            body: prepared.body,
                            plan,
                        },
                    ),
                });
                self.toast = "The impact changed · review the updated effect before posting".into();
                cx.notify();
                return false;
            }
            let outcome = self.commit_prepared_comment(
                crate::task_edit_app::PreparedCommentSave {
                    task: prepared.task,
                    body: prepared.body,
                    plan,
                },
                Some(window),
                cx,
            );
            match outcome {
                crate::task_edit_app::TaskSaveOutcome::Saved
                | crate::task_edit_app::TaskSaveOutcome::NoChanges => {
                    self.research_save_impact = None;
                    self.pending_comment_edit = None;
                    true
                }
                crate::task_edit_app::TaskSaveOutcome::Saving
                | crate::task_edit_app::TaskSaveOutcome::PendingDurability(_)
                | crate::task_edit_app::TaskSaveOutcome::RetryableFailure(_) => {
                    window.focus(&self.dialog_first_focus, cx);
                    cx.notify();
                    false
                }
                crate::task_edit_app::TaskSaveOutcome::PendingImpact
                | crate::task_edit_app::TaskSaveOutcome::Rejected(_) => {
                    self.research_save_impact = None;
                    cx.notify();
                    false
                }
            }
        } else {
            let PendingPreparedSave::Task(pending_prepared) = pending.prepared else {
                unreachable!()
            };
            let draft_values = match self.draft_editor_values(cx) {
                Some(values) => values,
                None => {
                    self.research_save_impact = None;
                    self.pending_task_edit = None;
                    cx.notify();
                    return false;
                }
            };
            let prepared = if draft_values == pending_prepared.reviewed {
                match crate::scoped_edit::rebase_task_edit(&self.store, &pending_prepared.plan) {
                    Ok(plan) => crate::task_edit_app::PreparedTaskSave {
                        input: pending_prepared.input.clone(),
                        plan,
                        reviewed: pending_prepared.reviewed.clone(),
                    },
                    Err(error) => {
                        let message = match error {
                            crate::scoped_edit::ScopedEditError::Rejected(message)
                            | crate::scoped_edit::ScopedEditError::Conflict(message) => message,
                        };
                        self.show_save_error(message, cx);
                        return false;
                    }
                }
            } else {
                match self.prepare_task_editor_save(cx) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        self.research_save_impact = None;
                        self.pending_task_edit = None;
                        self.show_save_error(error, cx);
                        return false;
                    }
                }
            };
            if prepared.plan.id() != pending.task {
                self.research_save_impact = None;
                self.pending_task_edit = None;
                self.show_save_error(
                    "The edited task changed. Reopen its editor before saving.",
                    cx,
                );
                return false;
            }
            if let Some(fresh) = prepared.plan.impact().cloned()
                && fresh.confirmation_required
                && fresh != pending.impact
            {
                let task = prepared.plan.id();
                self.research_save_impact = Some(PendingResearchSave {
                    task,
                    impact: fresh,
                    prepared: PendingPreparedSave::Task(prepared),
                });
                self.toast = "The impact changed · review the updated effect before saving".into();
                cx.notify();
                return false;
            }
            let exit_task = prepared.plan.id();
            let exit_instance = self.task_editor_instance();
            let exit_values = self.draft_editor_values(cx);
            let outcome = self.commit_prepared_task_edit(prepared, window, cx);
            match outcome {
                crate::task_edit_app::TaskSaveOutcome::Saved
                | crate::task_edit_app::TaskSaveOutcome::NoChanges => {
                    self.research_save_impact = None;
                    self.pending_task_edit = None;
                    if let Some(values) = exit_values.as_ref() {
                        let _ = self.complete_task_saved_exit(
                            exit_task,
                            exit_instance,
                            values,
                            window,
                            cx,
                        );
                    }
                    true
                }
                crate::task_edit_app::TaskSaveOutcome::Saving
                | crate::task_edit_app::TaskSaveOutcome::PendingDurability(_)
                | crate::task_edit_app::TaskSaveOutcome::RetryableFailure(_) => {
                    window.focus(&self.dialog_first_focus, cx);
                    cx.notify();
                    false
                }
                crate::task_edit_app::TaskSaveOutcome::PendingImpact
                | crate::task_edit_app::TaskSaveOutcome::Rejected(_) => {
                    self.research_save_impact = None;
                    self.pending_task_edit = None;
                    cx.notify();
                    false
                }
            }
        }
    }

    /// Post a comment through the scoped comment mutation. A confirmation-
    /// required impact opens the same reviewed modal used by task saves.
    pub(crate) fn request_task_comment(
        &mut self,
        id: u64,
        body: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.research_save_impact.is_some() {
            return false;
        }
        let prepared = match self.prepare_comment_save(id, body) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.show_save_error(error, cx);
                return false;
            }
        };
        if let Some(impact) = prepared.plan.impact().cloned()
            && impact.confirmation_required
        {
            self.research_save_opener = window.focused(cx);
            self.research_save_impact = Some(PendingResearchSave {
                task: id,
                impact,
                prepared: PendingPreparedSave::Comment(prepared),
            });
            window.focus(&self.dialog_first_focus, cx);
            cx.notify();
            return false;
        }
        self.commit_prepared_comment(prepared, Some(window), cx)
            .is_saved()
    }

    /// Cancel only the review. The source draft, selection and route stay intact.
    pub(crate) fn cancel_research_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let material_review = matches!(
            self.research_save_impact
                .as_ref()
                .map(|pending| &pending.prepared),
            Some(PendingPreparedSave::Materials(_))
        );
        if self.research_save_impact.take().is_some() {
            self.pending_task_edit = None;
            self.pending_comment_edit = None;
            self.navigation.pending_saved_exit = None;
            self.navigation.pending_exit_token = None;
            if let Some(pending) = &mut self.navigation.pending_exit {
                pending.notice = None;
            }
            if material_review {
                self.cancel_pending_material(cx);
            }
            let opener = self.research_save_opener.take();
            if let Some(opener) = opener {
                window.focus(&opener, cx);
            } else {
                window.focus(&self.focus, cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn open_previous_versions(
        &mut self,
        task: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.previous_version_entries(task);
        self.previous_versions_opener = window.focused(cx);
        self.previous_versions_task = Some(task);
        self.previous_versions_run = entries.first().map(|entry| entry.version.run_id);
        self.previous_versions_scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(run) = self.previous_versions_run {
            self.refresh_previous_version_text(run, cx);
        }
        window.focus(&self.dialog_first_focus, cx);
        cx.notify();
    }

    pub(crate) fn select_previous_version(&mut self, run: u64, cx: &mut Context<Self>) {
        self.previous_versions_run = Some(run);
        self.previous_versions_scroll
            .set_offset(point(px(0.), px(0.)));
        self.refresh_previous_version_text(run, cx);
        self.expanded_run_sources = None;
        self.expanded_run_appendix = None;
        self.result_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn refresh_previous_version_text(&mut self, run: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.previous_version_entry(run) else {
            self.previous_version_text = None;
            return;
        };
        let Some(run_ref) = self.store.runs().iter().find(|candidate| candidate.id == run) else {
            self.previous_version_text = None;
            return;
        };
        let history = history_presentation(&self.store, &entry, run_ref);
        let text = history.presentation.reader_markdown;
        self.previous_version_text =
            Some(cx.new(|cx| {
                gpui_kit::base::text::TextViewState::markdown(&text, cx).scrollable(true)
            }));
    }

    pub(crate) fn close_previous_versions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_versions_task = None;
        self.previous_versions_run = None;
        self.previous_version_text = None;
        let opener = self.previous_versions_opener.take();
        if let Some(opener) = opener {
            window.focus(&opener, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn previous_version_entries(&self, task: u64) -> Vec<PreviousVersionEntry> {
        let Some(current) = self.store.task(task) else {
            return Vec::new();
        };
        let tasks: Vec<u64> = if current
            .research
            .as_ref()
            .is_some_and(|research| research.kind == ResearchKind::Goal)
        {
            self.store.research_children(task)
        } else {
            vec![task]
        };
        let mut entries = Vec::new();
        for id in tasks {
            let Some(task) = self.store.task(id) else {
                continue;
            };
            let Some(kind) = task.research.as_ref().map(|research| research.kind) else {
                continue;
            };
            for version in research_input::previous_versions(&self.store, id) {
                entries.push(PreviousVersionEntry {
                    task: id,
                    task_title: task.title.clone(),
                    kind,
                    version,
                });
            }
        }
        entries.sort_by(|a, b| {
            let a = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == a.version.run_id)
                .map(research_input::run_activity_order_key)
                .unwrap_or((None, a.version.run_id));
            let b = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == b.version.run_id)
                .map(research_input::run_activity_order_key)
                .unwrap_or((None, b.version.run_id));
            b.cmp(&a)
        });
        entries
    }

    pub(crate) fn previous_version_entry(&self, run: u64) -> Option<PreviousVersionEntry> {
        let task = self.previous_versions_task?;
        self.previous_version_entries(task)
            .into_iter()
            .find(|entry| entry.version.run_id == run)
    }

    pub(crate) fn previous_version_reader(&self, run: u64) -> Option<String> {
        let entry = self.previous_version_entry(run)?;
        self.store
            .runs()
            .iter()
            .find(|candidate| candidate.id == entry.version.run_id)
            .map(|run| history_presentation(&self.store, &entry, run).text())
    }

    pub(crate) fn copy_previous_version(&mut self, run: u64, cx: &mut Context<Self>) {
        let Some(text) = self.previous_version_reader(run) else {
            return;
        };
        if text.trim().is_empty() {
            self.error = "This retained attempt has no reader text to copy.".into();
            cx.notify();
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast = format!("Previous report run #{run} copied");
        cx.notify();
    }

    pub(crate) fn export_previous_version(&mut self, run: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.previous_version_entry(run) else {
            return;
        };
        let Some(run_ref) = self
            .store
            .runs()
            .iter()
            .find(|candidate| candidate.id == entry.version.run_id)
        else {
            return;
        };
        let document = previous_version_document(&self.store, &entry, run_ref);
        let directory = self
            .workspace_root
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        let name = format!(
            "{}-previous-run-{}.md",
            filename(&entry.task_title),
            entry.version.run_id
        );
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |this, cx| {
            let destination = match path.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => return,
                _ => {
                    let _ = this.update(cx, |app, cx| {
                        app.error = "Could not open the report export dialog.".into();
                        cx.notify();
                    });
                    return;
                }
            };
            let saved = destination.clone();
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(&destination, document) })
                .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => {
                        app.error.clear();
                        app.toast = format!("Previous report exported to {}", saved.display());
                    }
                    Err(error) => {
                        app.error = format!("Could not export the previous report: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn previous_version_document(
    store: &Store,
    entry: &PreviousVersionEntry,
    run: &crate::domain::AgentRun,
) -> String {
    let history = history_presentation(store, entry, run);
    let mut document = format!(
        "# {} — Previous version\n\nRun #{} · {} · Input revision {}\n\n",
        entry.task_title,
        run.id,
        crate::time_display::timestamp(if entry.version.started_at.is_empty() {
            "recorded time unavailable"
        } else {
            entry.version.started_at.as_str()
        }),
        entry
            .version
            .revision_digest
            .as_deref()
            .unwrap_or("not recorded")
    );
    document.push_str(&history.text());
    document.push('\n');
    document.push_str("\n---\n\n## References\n\n");
    if history.presentation.sources.is_empty() {
        document
            .push_str("No separately recorded web sources. See citations in the document above.\n");
    } else {
        for source in &history.presentation.sources {
            document.push_str(&format!(
                "- {}\n  - URL: <{}>\n  - Published: {}\n",
                source.title.replace(['\n', '\r'], " "),
                source.url,
                crate::time_display::timestamp(&source.published_at).replace(['\n', '\r'], " ")
            ));
        }
    }
    document
}

pub(crate) fn step_kind_label(kind: ResearchKind) -> &'static str {
    match kind {
        ResearchKind::Goal => "Goal",
        ResearchKind::Collect => "Evidence",
        ResearchKind::Analyze => "Analysis",
        ResearchKind::Advise => "Final report",
    }
}

pub(crate) fn retained_label(impact: &ResearchInputImpact) -> String {
    let mut changed = Vec::new();
    let mut downstream = Vec::new();
    for step in &impact.steps {
        match step.input_state {
            StepInputState::Changed => changed.push(step.title.clone()),
            StepInputState::DownstreamAffected => downstream.push(step.title.clone()),
            StepInputState::Unchanged => {}
        }
    }
    let mut text = String::new();
    if !changed.is_empty() {
        text.push_str(&format!("Changed input: {}.", changed.join(", ")));
    }
    if !downstream.is_empty() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&format!(
            "Downstream input changed: {}.",
            downstream.join(", ")
        ));
    }
    if impact.retained_matching_results > 0 {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&format!(
            "{} unchanged result(s) keep matching inputs.",
            impact.retained_matching_results
        ));
    }
    text
}

fn filename(text: &str) -> String {
    let mut name: String = text
        .chars()
        .take(60)
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    name = name.trim_matches('_').to_string();
    if name.is_empty() {
        "research".into()
    } else {
        name
    }
}

/// Kept for Phase 2b callers so the material gate can use the same pending state
/// shape without inventing a second review model.
pub(crate) struct PendingMaterialBatch {
    pub task: u64,
    pub attachments: Vec<Attachment>,
    pub impact: ResearchInputImpact,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryPresentation {
    pub presentation: ResultPresentation,
    pub context: String,
}

impl HistoryPresentation {
    /// Quality and freshness facts shown on screen and carried into Copy/Export.
    pub(crate) fn facts(&self) -> Vec<&'static str> {
        let mut facts = Vec::new();
        if let Some(quality) = self.presentation.quality.label() {
            facts.push(quality);
        }
        facts.extend(self.presentation.freshness.labels());
        facts
    }

    /// One reader text for render context, Copy and Export. The context and facts
    /// are part of the same adapted object, so no path can copy a contradictory
    /// notice.
    pub(crate) fn text(&self) -> String {
        let facts = self.facts();
        let mut header = format!("> {}", self.context);
        if !facts.is_empty() {
            header.push_str(&format!("\n>\n> {}", facts.join(" · ")));
        }
        let body = self.presentation.reader_text();
        if body.trim().is_empty() {
            header
        } else {
            format!("{header}\n\n{body}")
        }
    }
}

/// Narrow T21 adapter over T06's presentation. It may suppress a false stale
/// badge when the persisted input still matches and execution is merely waiting
/// for approval. It never makes an ineligible run current and never changes T06
/// evidence quality or automation snapshot semantics.
pub(crate) fn history_presentation(
    store: &Store,
    entry: &PreviousVersionEntry,
    run: &crate::domain::AgentRun,
) -> HistoryPresentation {
    let mut presentation = present_run(store, run);
    let goal = store.owning_research_goal(entry.task).unwrap_or(entry.task);
    let goal_approved = store.goal_execution_approved(goal);
    let current_run = store.research_result(entry.task).map(|result| result.id);
    let superseded_by_newer = current_run.is_some_and(|current| current != run.id);
    if !entry.version.execution_admissible && entry.version.inputs_match_current {
        presentation.freshness.stale_input = false;
        presentation.freshness.historical = true;
        presentation.freshness.previous_version = false;
    }
    let context = if store.task_is_archived(entry.task) {
        "Archived history · retained report, available read-only. Restore the goal to resume work."
            .to_string()
    } else if entry.version.execution_admissible {
        "Current execution result.".to_string()
    } else if entry.version.inputs_match_current {
        // Approval availability, not the historical label, decides whether the
        // user must review. A superseded attempt under an approved goal must not
        // claim that review is required.
        if goal_approved {
            if superseded_by_newer {
                "Earlier attempt · inputs unchanged · superseded by a newer current result."
                    .to_string()
            } else {
                "Earlier attempt · inputs unchanged · the goal is currently approved.".to_string()
            }
        } else {
            "Inputs unchanged · retained while execution waits for plan review.".to_string()
        }
    } else if goal_approved {
        "Previous input revision · superseded; the goal is approved for the newer inputs."
            .to_string()
    } else {
        "Previous input revision · execution waits for the updated plan to be approved.".to_string()
    };
    HistoryPresentation {
        presentation,
        context,
    }
}

#[cfg(test)]
mod tests {
    use super::{PreviousVersionEntry, history_presentation, previous_version_document};
    use crate::domain::{Stage, Store};

    const PLAN: &str = r#"{"tasks":[
        {"title":"Collect A","instructions":"Collect A","kind":"collect","depends_on":[]},
        {"title":"Collect B","instructions":"Collect B","kind":"collect","depends_on":[]},
        {"title":"Analyze A","instructions":"Analyze A","kind":"analyze","depends_on":[0]},
        {"title":"Report","instructions":"Report","kind":"advise","depends_on":[2]}
    ]}"#;

    fn workspace() -> (Store, u64, Vec<u64>) {
        let mut store = Store::mock();
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.research_config_mut().models =
            ["test/plan", "test/search", "test/analysis", "test/brief"].map(String::from);
        let goal = store
            .create_research_goal("History".into(), "Scope".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
        for id in &steps {
            let mut run = store.prepare_run(*id).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Report {id}");
            store.push_run(run);
            store.set_stage(*id, Stage::Done).unwrap();
        }
        store.sync_goal_progress(goal);
        (store, goal, steps)
    }

    fn version_entry(
        store: &Store,
        task: u64,
        run: u64,
    ) -> (PreviousVersionEntry, crate::domain::AgentRun) {
        let version = crate::research_input::previous_versions(store, task)
            .into_iter()
            .find(|version| version.run_id == run)
            .unwrap();
        let task_ref = store.task(task).unwrap();
        (
            PreviousVersionEntry {
                task,
                task_title: task_ref.title.clone(),
                kind: task_ref.research.as_ref().unwrap().kind,
                version,
            },
            store
                .runs()
                .iter()
                .find(|candidate| candidate.id == run)
                .unwrap()
                .clone(),
        )
    }

    #[test]
    fn approved_goal_superseded_attempt_is_not_waiting_for_review() {
        let (mut store, goal, steps) = workspace();
        let original = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .clone();
        let mut newer = original.clone();
        newer.id = 10_000;
        newer.started_at = "2026-11-08 18:00:00 +08:00".into();
        store.push_run(newer.clone());
        assert!(store.goal_execution_approved(goal));
        assert_eq!(store.research_result(steps[0]).unwrap().id, newer.id);

        let (entry, run) = version_entry(&store, steps[0], original.id);
        let history = history_presentation(&store, &entry, &run);
        assert!(!history.presentation.freshness.stale_input);
        assert!(
            history
                .context
                .contains("superseded by a newer current result"),
            "{}",
            history.context
        );
        assert!(
            !history.context.contains("plan review"),
            "{}",
            history.context
        );

        let (current_entry, current_run) = version_entry(&store, steps[0], newer.id);
        let current = history_presentation(&store, &current_entry, &current_run);
        assert_eq!(current.context, "Current execution result.");
        assert!(current.presentation.freshness.is_current());
    }

    #[test]
    fn archived_matching_history_is_read_only_not_stale() {
        let (mut store, goal, steps) = workspace();
        let run_id = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .id;
        store.archive(goal).unwrap();
        let (entry, run) = version_entry(&store, steps[0], run_id);
        let history = history_presentation(&store, &entry, &run);
        assert!(entry.version.inputs_match_current);
        assert!(!history.presentation.freshness.stale_input);
        assert!(history.context.contains("Archived history"));
        assert!(history.context.contains("read-only"));
        let document = previous_version_document(&store, &entry, &run);
        assert!(document.contains(&history.text()));
        assert!(!history.text().contains("Stale input"));
        assert!(!document.contains("Stale input"));
        assert!(history.facts().contains(&"Historical result"));
    }

    #[test]
    fn archived_genuine_changed_input_keeps_stale_facts_in_text_and_export() {
        let (mut store, goal, steps) = workspace();
        let run_id = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .id;
        store.task_mut(steps[0]).unwrap().title = "Collect A revised".into();
        store.reconcile_goal_approvals();
        store.archive(goal).unwrap();
        let (entry, run) = version_entry(&store, steps[0], run_id);
        assert!(!entry.version.inputs_match_current);
        let history = history_presentation(&store, &entry, &run);
        assert!(history.facts().contains(&"Stale input"));
        assert!(history.text().contains("Stale input"));
        assert!(history.context.contains("Archived history"));
        let document = previous_version_document(&store, &entry, &run);
        assert!(document.contains("Stale input"));
        assert!(document.contains(&history.text()));
    }

    #[test]
    fn reapproved_newer_inputs_make_old_revision_superseded_not_review_pending() {
        let (mut store, goal, steps) = workspace();
        let old_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[0])
            .unwrap()
            .clone();
        store.task_mut(steps[0]).unwrap().title = "Collect A revised".into();
        store.reconcile_goal_approvals();
        assert!(!store.goal_execution_approved(goal));
        store.set_stage(goal, Stage::Planned).unwrap();
        assert!(store.goal_execution_approved(goal));

        let (entry, run) = version_entry(&store, steps[0], old_run.id);
        assert!(!entry.version.inputs_match_current);
        let history = history_presentation(&store, &entry, &run);
        assert!(
            history.context.contains("superseded; the goal is approved"),
            "{}",
            history.context
        );
        assert!(!history.context.contains("waits"), "{}", history.context);
        assert!(history.presentation.freshness.stale_input);
        let document = previous_version_document(&store, &entry, &run);
        assert!(document.contains(&history.text()));
    }

    #[test]
    fn revoked_approval_keeps_review_waiting_context() {
        let (mut store, _, steps) = workspace();
        let old_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[1])
            .unwrap()
            .clone();
        store.task_mut(steps[0]).unwrap().title = "Collect A revised".into();
        store.reconcile_goal_approvals();
        let (entry, run) = version_entry(&store, steps[1], old_run.id);
        let history = history_presentation(&store, &entry, &run);
        assert!(entry.version.inputs_match_current);
        assert!(
            history.context.contains("waits for plan review"),
            "{}",
            history.context
        );
        assert!(!history.presentation.freshness.stale_input);
    }
}
