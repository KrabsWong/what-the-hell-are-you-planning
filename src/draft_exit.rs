//! Pure dirty-draft and exit-decision policy for T10.
//!
//! One computation decides whether a form holds unsaved user input, and one
//! small state machine decides what happens when the user tries to leave. The
//! application, the global-search overlay and the decision dialog all consume
//! these functions so no second guard can drift.
//!
//! Nothing here touches the window, the store or persistence. `SaveOutcome`
//! must come from a real save entry point: a decision proceeds only after a
//! confirmed save, never on "no error observed".

use std::collections::BTreeMap;

use crate::automation_views::AutomationDraft;
use crate::navigation_context::{DismissibleSurface, NavigationTarget};

/// A surface that can hold user input the user did not commit yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftSurface {
    TaskEdit { task: u64 },
    /// `editing` is `Some(id)` for an existing definition, `None` for a new one.
    AutomationWizard { editing: Option<u64> },
    /// The project/topic research request composer (New research, topic New
    /// research, topic Replan).
    PlanningRequest,
    /// The inline comment composer of one task. Preserved per task.
    Comment { task: u64 },
    /// The inline composer on the Market research page. Preserved as a page
    /// draft (destination + text).
    ResearchComposer,
}

/// How an exit attempt must treat a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftClass {
    /// Leaving offers Save / Discard / Stay.
    Decision,
    /// Leaving preserves the text; the surface offers a visible discard later.
    Preserved,
}

/// Surfaces that may raise the decision dialog. Kept separate from
/// `DraftSurface` so a preserved surface cannot accidentally prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecisionSurface {
    TaskEdit { task: u64 },
    AutomationWizard { editing: Option<u64> },
    PlanningRequest,
}

pub(crate) fn draft_class(surface: DraftSurface) -> DraftClass {
    match surface {
        DraftSurface::TaskEdit { .. }
        | DraftSurface::AutomationWizard { .. }
        | DraftSurface::PlanningRequest => DraftClass::Decision,
        DraftSurface::Comment { .. } | DraftSurface::ResearchComposer => DraftClass::Preserved,
    }
}

pub(crate) fn decision_surface(surface: DraftSurface) -> Option<DecisionSurface> {
    match surface {
        DraftSurface::TaskEdit { task } => Some(DecisionSurface::TaskEdit { task }),
        DraftSurface::AutomationWizard { editing } => {
            Some(DecisionSurface::AutomationWizard { editing })
        }
        DraftSurface::PlanningRequest => Some(DecisionSurface::PlanningRequest),
        DraftSurface::Comment { .. } | DraftSurface::ResearchComposer => None,
    }
}

/// Values of the task detail editor, in the form the form holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskEditValues {
    pub title: String,
    pub description: String,
    /// Normalized: trimmed, empty entries removed, order as entered.
    pub labels: Vec<String>,
    pub due_date: String,
    pub project: u64,
    pub stage: crate::domain::Stage,
    pub cycle: Option<String>,
}

/// Any difference in a task-editor field is unsaved user input.
pub(crate) fn task_edit_dirty(stored: &TaskEditValues, current: &TaskEditValues) -> bool {
    stored != current
}

/// A whole automation form, including the reviewed plan. `AutomationDraft`
/// excludes the wizard step, so moving between steps is not dirt. The caller
/// must rebase the baseline when the wizard accepts a plan for its own attempt.
pub(crate) fn automation_draft_dirty(baseline: &AutomationDraft, current: &AutomationDraft) -> bool {
    baseline != current
}

/// Text draft comparison used by the planning composer and the Market research
/// composer. Whitespace-only differences are not content; deleting prefilled
/// replan text is a change.
pub(crate) fn text_draft_dirty(baseline: &str, current: &str) -> bool {
    baseline.trim() != current.trim()
}

/// An inline comment has no baseline: any non-empty text is unsaved input.
pub(crate) fn comment_draft_dirty(current: &str) -> bool {
    !current.trim().is_empty()
}

/// Per-task text slots so switching tasks never drops a typed comment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerTaskText {
    values: BTreeMap<u64, String>,
}

impl PerTaskText {
    pub(crate) fn set(&mut self, task: u64, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            self.values.remove(&task);
        } else {
            self.values.insert(task, text);
        }
    }

    pub(crate) fn get(&self, task: u64) -> Option<&str> {
        self.values.get(&task).map(String::as_str)
    }

    pub(crate) fn take(&mut self, task: u64) -> Option<String> {
        self.values.remove(&task)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
}

/// A preserved page draft for the Market research composer. Destination and
/// text travel together so a restored draft never lands in a different project
/// without saying so.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComposerDraft {
    pub project: u64,
    pub topic: Option<u64>,
    pub title: String,
    pub brief: String,
}

/// Where a resolved exit goes after Save/Discard.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExitTarget {
    Back,
    /// Record the caller and go to the target (user drill-down / search).
    Navigate(NavigationTarget),
    /// Clear return history and go to the structural target.
    Structural(NavigationTarget),
    NewResearch,
    Dismiss(DismissibleSurface),
    SidebarTask { project: u64, task: u64 },
    Recent { task: u64, surface: crate::navigation_context::RecentSurface },
    CancelWizard { editing: Option<u64> },
    CancelPlanning,
}

/// One pending exit decision. `error` is shown inside the dialog and never
/// silently dismisses it. `from_search` closes the search overlay only after the
/// user proceeds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingExit {
    pub surface: DecisionSurface,
    pub target: ExitTarget,
    pub error: Option<String>,
    /// Neutral progress text while a save receipt is genuinely pending.
    pub notice: Option<String>,
    pub from_search: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExitDecision {
    Save,
    Discard,
    Stay,
}

/// The confirmed result of the surface's own save entry point. `Rejected` is a
/// validation/business failure; `Unavailable` means storage/persistence could
/// not confirm the save. Neither may proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    Saved,
    /// A real durable write is in flight. Not Saved, but not a failure either:
    /// keep the decision and complete the original exit after acknowledgement.
    Pending(String),
    Rejected(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DecisionOutcome {
    Proceed(ExitTarget),
    Pending { message: String },
    Stay { error: Option<String> },
}

/// The one decision state machine. Save proceeds only on a confirmed save.
pub(crate) fn resolve_decision(
    pending: &PendingExit,
    decision: ExitDecision,
    save: SaveOutcome,
) -> DecisionOutcome {
    match decision {
        ExitDecision::Stay => DecisionOutcome::Stay { error: None },
        ExitDecision::Discard => DecisionOutcome::Proceed(pending.target.clone()),
        ExitDecision::Save => match save {
            SaveOutcome::Saved => DecisionOutcome::Proceed(pending.target.clone()),
            SaveOutcome::Pending(message) => DecisionOutcome::Pending { message },
            SaveOutcome::Rejected(error) | SaveOutcome::Unavailable(error) => {
                DecisionOutcome::Stay {
                    error: Some(error),
                }
            }
        },
    }
}

pub(crate) fn decision_title(surface: DecisionSurface) -> &'static str {
    match surface {
        DecisionSurface::TaskEdit { .. } => "Save this task edit?",
        DecisionSurface::AutomationWizard { .. } => "Save this automation draft?",
        DecisionSurface::PlanningRequest => "Save this research request?",
    }
}

/// The primary label states exactly what Save does. The automation wording is
/// intentionally "Save draft and continue": saving never enables or runs.
pub(crate) fn save_action_label(surface: DecisionSurface) -> &'static str {
    match surface {
        DecisionSurface::TaskEdit { .. } => "Save changes and continue",
        DecisionSurface::AutomationWizard { .. } => "Save draft and continue",
        DecisionSurface::PlanningRequest => "Save request and continue",
    }
}

pub(crate) fn discard_action_label() -> &'static str {
    "Discard and continue"
}

pub(crate) fn stay_action_label() -> &'static str {
    "Stay"
}

/// The single message used when a decision surface blocks a navigation the
/// caller cannot present a dialog for (for example the global-search overlay).
/// T08 must use this text instead of its own predicate-specific copies.
pub(crate) fn blocking_message(surface: DecisionSurface) -> &'static str {
    match surface {
        DecisionSurface::TaskEdit { .. } => {
            "Finish, save or discard the open task edit before opening a search result."
        }
        DecisionSurface::AutomationWizard { .. } => {
            "Finish, save or discard the automation draft before opening a search result."
        }
        DecisionSurface::PlanningRequest => {
            "Finish, save or discard the research request before opening a search result."
        }
    }
}

/// Neutral explanation shown under the title. It names the loss and confirms
/// that saving a draft does not start work.
pub(crate) fn decision_explanation(surface: DecisionSurface) -> &'static str {
    match surface {
        DecisionSurface::TaskEdit { .. } => {
            "These edits exist only in this form. Save them, discard them, or stay here."
        }
        DecisionSurface::AutomationWizard { .. } => {
            "These fields exist only in this form. Saving stores a draft and never enables or runs the automation."
        }
        DecisionSurface::PlanningRequest => {
            "This request exists only in this form. Save it for later, discard it, or stay here. Generating a plan is a separate action."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationShape;
    use crate::automation_views::AutomationDraftDefaults;
    use crate::automation_schedule::CatchUp;

    fn baseline() -> AutomationDraft {
        AutomationDraft::new(&AutomationDraftDefaults {
            timezone: "Asia/Shanghai".into(),
        })
    }

    fn task_values() -> TaskEditValues {
        TaskEditValues {
            title: "Title".into(),
            description: "Description".into(),
            labels: vec!["one".into(), "two".into()],
            due_date: "2026-11-08".into(),
            project: 1,
            stage: crate::domain::Stage::Planned,
            cycle: None,
        }
    }

    #[test]
    fn task_edit_dirty_detects_every_field() {
        let stored = task_values();
        assert!(!task_edit_dirty(&stored, &stored.clone()));
        let mut cases = vec![];
        let mut value = stored.clone();
        value.title = "Other".into();
        cases.push(value);
        let mut value = stored.clone();
        value.description = "Other".into();
        cases.push(value);
        let mut value = stored.clone();
        value.labels = vec!["one".into()];
        cases.push(value);
        let mut value = stored.clone();
        value.due_date = "2026-12-01".into();
        cases.push(value);
        let mut value = stored.clone();
        value.project = 2;
        cases.push(value);
        let mut value = stored.clone();
        value.stage = crate::domain::Stage::Done;
        cases.push(value);
        let mut value = stored.clone();
        value.cycle = Some("Sprint".into());
        cases.push(value);
        for value in cases {
            assert!(task_edit_dirty(&stored, &value));
        }
    }

    #[test]
    fn automation_dirty_covers_shape_schedule_policy_and_plan() {
        let base = baseline();
        assert!(!automation_draft_dirty(&base, &base.clone()));

        let mut changed = base.clone();
        changed.shape = AutomationShape::Research;
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.kind = crate::automation_views::AutomationKind::Interval;
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.interval_in_hours = true;
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.catch_up = CatchUp::SkipAll;
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.weekdays = [false; 7];
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.timezone = "UTC".into();
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.timeout = "45".into();
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.objective = "New objective".into();
        assert!(automation_draft_dirty(&base, &changed));

        let mut changed = base.clone();
        changed.plan = Some(
            crate::automation_execution::build_template("Plan", "Output").unwrap(),
        );
        assert!(automation_draft_dirty(&base, &changed));
    }

    #[test]
    fn a_rebased_plan_is_not_dirt() {
        let mut base = baseline();
        let plan = crate::automation_execution::build_template("Plan", "Output").unwrap();
        base.plan = Some(plan.clone());
        let mut current = base.clone();
        current.name = "Named".into();
        assert!(automation_draft_dirty(&base, &current));
        current.name.clear();
        assert!(!automation_draft_dirty(&base, &current));
    }

    #[test]
    fn text_drafts_ignore_whitespace_but_detect_real_edits() {
        assert!(!text_draft_dirty("", "  \n"));
        assert!(!text_draft_dirty("Prefilled", "Prefilled  "));
        assert!(text_draft_dirty("", "Request"));
        assert!(text_draft_dirty("Prefilled", ""));
    }

    #[test]
    fn comments_are_preserved_per_task() {
        let mut comments = PerTaskText::default();
        assert!(!comment_draft_dirty("  "));
        assert!(comment_draft_dirty("A note"));
        comments.set(1, "Task one note");
        comments.set(2, "Task two note");
        assert_eq!(comments.get(1), Some("Task one note"));
        assert_eq!(comments.get(2), Some("Task two note"));
        comments.set(1, "   ");
        assert_eq!(comments.get(1), None);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments.take(2).as_deref(), Some("Task two note"));
        assert!(comments.is_empty());
    }

    #[test]
    fn decision_classes_match_the_approved_policy() {
        assert_eq!(
            draft_class(DraftSurface::TaskEdit { task: 1 }),
            DraftClass::Decision
        );
        assert_eq!(
            draft_class(DraftSurface::AutomationWizard { editing: None }),
            DraftClass::Decision
        );
        assert_eq!(
            draft_class(DraftSurface::PlanningRequest),
            DraftClass::Decision
        );
        assert_eq!(
            draft_class(DraftSurface::Comment { task: 1 }),
            DraftClass::Preserved
        );
        assert_eq!(
            draft_class(DraftSurface::ResearchComposer),
            DraftClass::Preserved
        );
        assert_eq!(
            decision_surface(DraftSurface::Comment { task: 1 }),
            None
        );
        assert_eq!(
            decision_surface(DraftSurface::ResearchComposer),
            None
        );
    }

    #[test]
    fn save_never_proceeds_without_a_confirmed_result() {
        let pending = PendingExit {
            surface: DecisionSurface::AutomationWizard { editing: None },
            target: ExitTarget::Navigate(NavigationTarget::AutomationList),
            error: None,
            notice: None,
            from_search: false,
        };
        assert_eq!(
            resolve_decision(&pending, ExitDecision::Save, SaveOutcome::Saved),
            DecisionOutcome::Proceed(ExitTarget::Navigate(NavigationTarget::AutomationList))
        );
        assert_eq!(
            resolve_decision(
                &pending,
                ExitDecision::Save,
                SaveOutcome::Pending("Saving…".into())
            ),
            DecisionOutcome::Pending {
                message: "Saving…".into()
            },
            "a genuine in-flight save is neither rejected nor advanced"
        );
        for save in [
            SaveOutcome::Rejected("Add an instruction first.".into()),
            SaveOutcome::Unavailable("Workspace storage is unavailable.".into()),
        ] {
            match resolve_decision(&pending, ExitDecision::Save, save) {
                DecisionOutcome::Stay { error: Some(_) } => {}
                other => panic!("save without confirmation must stay, got {other:?}"),
            }
        }
        assert_eq!(
            resolve_decision(&pending, ExitDecision::Discard, SaveOutcome::Saved),
            DecisionOutcome::Proceed(ExitTarget::Navigate(NavigationTarget::AutomationList))
        );
        assert_eq!(
            resolve_decision(&pending, ExitDecision::Stay, SaveOutcome::Saved),
            DecisionOutcome::Stay { error: None }
        );
    }

    #[test]
    fn action_labels_are_explicit_and_english() {
        assert_eq!(
            save_action_label(DecisionSurface::AutomationWizard { editing: Some(1) }),
            "Save draft and continue"
        );
        assert_eq!(
            save_action_label(DecisionSurface::TaskEdit { task: 1 }),
            "Save changes and continue"
        );
        assert_eq!(
            save_action_label(DecisionSurface::PlanningRequest),
            "Save request and continue"
        );
        assert_eq!(discard_action_label(), "Discard and continue");
        assert_eq!(stay_action_label(), "Stay");
        for surface in [
            DecisionSurface::TaskEdit { task: 1 },
            DecisionSurface::AutomationWizard { editing: None },
            DecisionSurface::PlanningRequest,
        ] {
            assert!(!decision_title(surface).is_empty());
            assert!(!decision_explanation(surface).is_empty());
            assert!(!blocking_message(surface).is_empty());
        }
        assert!(
            blocking_message(DecisionSurface::AutomationWizard { editing: None })
                .contains("automation draft")
        );
    }
}
