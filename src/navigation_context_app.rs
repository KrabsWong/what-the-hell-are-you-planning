//! TaskApp glue for the pure T10 navigation context.
//!
//! User handlers call `record_origin` before a drill-down; Back asks the origin
//! stack first and falls back to the structural crumbs. Structural entry points
//! (sidebar, breadcrumbs, a result's parent "Open project") clear the stack.
//!
//! Background completion paths must never call these methods.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::app::{AllStatus, Layout, Scope, TaskApp};
use crate::automation_views::AutomationDraft;
use crate::domain::{Stage, Store};
use crate::draft_exit::{
    ComposerDraft, DecisionOutcome, DecisionSurface, DraftSurface, ExitDecision, ExitTarget,
    PendingExit, PerTaskText, SaveOutcome, TaskEditValues,
};
use crate::navigation_context::{
    BoardAnchor, CreationMode, NavigationTarget, Origin, OriginStack, PeriodUnder, RecentSurface,
    ResearchScroll, ScrollState,
};
use crate::settings_views::SettingsTab;
use gpui_kit::*;

/// A dirty-exit Save whose durable receipt is still in flight. The decision
/// modal stays open; the exact captured exit runs once after acknowledgement.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingSavedExit {
    pub surface: DecisionSurface,
    pub target: ExitTarget,
    pub from_search: bool,
    pub token: u64,
    pub durable: bool,
    pub closed_by_completion: bool,
    pub form: PendingSavedExitForm,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PendingSavedExitForm {
    Automation {
        input: crate::domain_command::AutomationDraftInput,
        subject: Option<u64>,
        epoch: u64,
    },
    Task {
        task: u64,
        instance: Option<crate::live_save_app::TaskEditorInstance>,
        values: crate::draft_exit::TaskEditValues,
    },
    Planning {
        id: u64,
        input: String,
        epoch: u64,
        destination: Option<crate::navigation_context::CreationMode>,
        start: bool,
        replan_source: Option<u64>,
    },
}

pub(crate) struct NavigationState {    /// Last surface opened for each task tab, so Recently opened can reopen a
    /// report instead of always dropping to the detail page.
    pub recent_surfaces: BTreeMap<u64, RecentSurface>,
    /// Virtualized task list scroll.
    pub task_list_scroll: UniformListScrollHandle,
    /// Topic goal list scroll.
    pub topic_scroll: ScrollHandle,
    /// Board horizontal scroll.
    pub board_scroll: ScrollHandle,
    /// Per-column board vertical scroll, created on first render.
    board_columns: RefCell<Vec<(Stage, ScrollHandle)>>,
    /// Whole-form snapshot captured when the automation wizard opened.
    pub automation_baseline: Option<AutomationDraft>,
    /// Task-editor values captured when the editor opened. Comparing against
    /// this baseline avoids treating unrelated runtime changes as user dirt.
    pub task_edit_baseline: Option<(u64, TaskEditValues)>,
    /// Planning composer baseline (empty for New, prefilled for Replan).
    pub planning_baseline: String,
    /// Per-task inline comments, so switching tasks never drops typed text.
    pub comments: PerTaskText,
    /// Market research composer parked across navigation.
    pub composer: Option<ComposerDraft>,
    /// One pending Save/Discard/Stay decision.
    pub pending_exit: Option<PendingExit>,
    /// Focus owner at the moment the decision opened; Stay returns here.
    pub decision_return_focus: Option<FocusHandle>,
    /// Exit to perform after a T21 impact confirmation and confirmed save.
    pub after_save_target: Option<(ExitTarget, bool)>,
    /// Dirty-exit Save waiting for the exact durable command receipt.
    pub pending_saved_exit: Option<PendingSavedExit>,
    /// Column/save command token handed to the next live command submission.
    pub pending_exit_token: Option<u64>,
    /// Standalone automation Save completion handed to the next command.
    pub pending_automation_save: Option<crate::live_save_app::AutomationSaveReceipt>,
    next_exit_token: u64,
    /// Selected run while the visible page is not the typed result page (for
    /// example a planning attempt reviewed from Settings). S4 replaces this
    /// payload with a typed overlay; it is never route identity.
    pub selected_attempt: Option<u64>,
    /// Last Settings section, used as the default when Settings is entered
    /// again. The active page stores its own typed tab.
    pub settings_section: crate::navigation_route::SettingsSection,
    /// Last goal-steps layout, used when a Task page has no explicit value.
    pub steps_layout: crate::app::Layout,
}

impl NavigationState {
    pub(crate) fn new() -> Self {
        Self {
            recent_surfaces: BTreeMap::new(),
            task_list_scroll: UniformListScrollHandle::new(),
            topic_scroll: ScrollHandle::new(),
            board_scroll: ScrollHandle::new(),
            board_columns: RefCell::new(Vec::new()),
            automation_baseline: None,
            task_edit_baseline: None,
            planning_baseline: String::new(),
            comments: PerTaskText::default(),
            composer: None,
            pending_exit: None,
            decision_return_focus: None,
            after_save_target: None,
            pending_saved_exit: None,
            pending_exit_token: None,
            pending_automation_save: None,
            next_exit_token: 1,
            selected_attempt: None,
            settings_section: crate::navigation_route::SettingsSection::General,
            steps_layout: crate::app::Layout::Board,
        }
    }

    pub(crate) fn board_column(&self, stage: Stage) -> ScrollHandle {
        let mut columns = self.board_columns.borrow_mut();
        if let Some((_, handle)) = columns.iter().find(|(value, _)| *value == stage) {
            return handle.clone();
        }
        let handle = ScrollHandle::new();
        columns.push((stage, handle.clone()));
        handle
    }
}

fn settings_index(tab: SettingsTab) -> usize {
    match tab {
        SettingsTab::General => 0,
        SettingsTab::Models => 1,
        SettingsTab::Publishing => 2,
    }
}

fn settings_tab(index: usize) -> SettingsTab {
    match index {
        1 => SettingsTab::Models,
        2 => SettingsTab::Publishing,
        _ => SettingsTab::General,
    }
}

fn pixels_y(handle: &ScrollHandle) -> f32 {
    handle.offset().y.as_f32()
}

impl TaskApp {
    /// Clear the typed return history for an explicit structural navigation.
    pub(crate) fn structural_navigation(&mut self) {
        self.clear_return_frames();
    }

    pub(crate) fn record_origin(&mut self) {
        let frame = self.capture_return_frame();
        self.record_return_frame(frame);
    }

    /// A card's Open steps is a user drill-down. Back restores the page that
    /// actually showed the card, including All/Project/Topic/Archive, layout,
    /// filter and scroll; breadcrumbs remain structural.
    pub(crate) fn record_goal_origin(&mut self, _task: u64) {
        // A card's Open steps is a drill-down from the page that actually shows
        // the card. Capture that initiating context (All/Project/Topic/Archive,
        // including layout, filter and scroll) rather than rewriting it to the
        // goal's Topic. Breadcrumbs remain structural parents.
        self.record_origin();
    }

    pub(crate) fn remember_recent_surface(&mut self, task: u64, surface: RecentSurface) {
        self.navigation.recent_surfaces.insert(task, surface);
    }

    /// Snapshot the current page for a later Back. Only user drill-down handlers
    /// call this.
    pub(crate) fn capture_origin(&self) -> Origin {
        if let Some(occurrence_id) = self.period_overlay()
            && let Some(occurrence) = self.store.occurrence(occurrence_id)
        {
            let under = if self.scope() == Scope::AutomationRuns {
                PeriodUnder::Results {
                    filter: self.automation_ui.result_filter,
                    history_limit: self.automation_ui.history_limit,
                    scroll: ScrollState {
                        x: self.automation_ui.scroll.offset().x.as_f32(),
                        y: pixels_y(&self.automation_ui.scroll),
                    },
                }
            } else {
                PeriodUnder::Detail {
                    scroll: ScrollState {
                        x: self.automation_ui.scroll.offset().x.as_f32(),
                        y: pixels_y(&self.automation_ui.scroll),
                    },
                }
            };
            return Origin::AutomationPeriod {
                automation: occurrence.automation_id,
                occurrence: occurrence_id,
                under,
                dialog_scroll: ScrollState {
                    x: self.automation_ui.dialog_scroll.offset().x.as_f32(),
                    y: pixels_y(&self.automation_ui.dialog_scroll),
                },
            };
        }
        if let Some(task) = self.task_result_context() {
            // A report opened from a task is its own page. Its caller context is
            // the task/run, not a filtered results list; project/topic stay
            // unset so Back does not silently filter the results page.
            return self.report_origin(Some(task), self.selected_run());
        }
        // A selected task is the visible task page even when the underlying
        // scope is Runs; capture that page rather than the results list.
        if let Some(task) = self.selected() {
            return Origin::ResearchTask {
                task,
                steps_tab: self.goal_steps_tab(),
                steps_layout: self.steps_layout(),
                scroll: ScrollState {
                    x: self.detail_scroll.offset().x.as_f32(),
                    y: pixels_y(&self.detail_scroll),
                },
            };
        }
        if let Some(run) = self.selected_run()
            && self.scope() == Scope::Runs
        {
            return self.results_origin(Some(run));
        }
        if self.scope() == Scope::Automations
            && self.settings_overlay()
            && let Some(id) = self.automation_ui.selected
        {
            return Origin::AutomationSettings {
                automation: id,
                settings_scroll: ScrollState {
                    x: self.automation_ui.settings_scroll.offset().x.as_f32(),
                    y: pixels_y(&self.automation_ui.settings_scroll),
                },
            };
        }
        match &self.scope() {
            Scope::Topic(topic) => Origin::ResearchTopic {
                topic: *topic,
                layout: self.layout,
                scroll: ScrollState {
                    x: self.navigation.topic_scroll.offset().x.as_f32(),
                    y: pixels_y(&self.navigation.topic_scroll),
                },
            },
            Scope::Automations if self.automation_ui.selected.is_some() => {
                Origin::AutomationDetail {
                    automation: self.automation_ui.selected.unwrap(),
                    scroll: ScrollState {
                        x: self.automation_ui.scroll.offset().x.as_f32(),
                        y: pixels_y(&self.automation_ui.scroll),
                    },
                }
            }
            Scope::AutomationRuns => Origin::AutomationResults {
                filter: self.automation_ui.result_filter,
                history_limit: self.automation_ui.history_limit,
                scroll: ScrollState {
                    x: self.automation_ui.scroll.offset().x.as_f32(),
                    y: pixels_y(&self.automation_ui.scroll),
                },
            },
            Scope::Settings => Origin::SettingsPage {
                tab: settings_index(self.settings_tab()),
                scroll: ScrollState {
                    x: self.settings_scroll.offset().x.as_f32(),
                    y: pixels_y(&self.settings_scroll),
                },
            },
            Scope::Automations => Origin::AutomationList {
                scroll: ScrollState {
                    x: self.automation_ui.scroll.offset().x.as_f32(),
                    y: pixels_y(&self.automation_ui.scroll),
                },
            },
            scope => Origin::ResearchList {
                scope: scope.clone(),
                query: self.query(),
                all_status: self.all_status,
                layout: self.layout,
                scroll: ResearchScroll {
                    outer: ScrollState {
                        x: self.navigation.board_scroll.offset().x.as_f32(),
                        y: self
                            .navigation
                            .task_list_scroll
                            .0
                            .borrow()
                            .base_handle
                            .offset()
                            .y
                            .as_f32(),
                    },
                    columns: Stage::BOARD_ORDER
                        .into_iter()
                        .map(|stage| BoardAnchor {
                            stage,
                            y: self
                                .navigation
                                .board_column(stage)
                                .offset()
                                .y
                                .as_f32(),
                        })
                        .collect(),
                },
            },
        }
    }

    fn report_origin(&self, task: Option<u64>, run: Option<u64>) -> Origin {
        Origin::ResearchResult {
            task,
            run,
            project: None,
            topic: None,
            scroll: ScrollState {
                x: self.result_scroll.offset().x.as_f32(),
                y: pixels_y(&self.result_scroll),
            },
        }
    }

    /// The Research results page has no separate project/topic filter state:
    /// its pickers choose a run, and the run's task owns the displayed
    /// project/topic. Capture that run explicitly; do not infer filters from an
    /// unrelated task selection.
    fn results_origin(&self, run: Option<u64>) -> Origin {
        let (project, topic) = run
            .and_then(|id| self.store.runs().iter().find(|value| value.id == id))
            .and_then(|run| self.store.task(run.task_id))
            .map(|task| {
                (
                    Some(task.project_id),
                    self.store.task_topic(task.id).map(|topic| topic.id),
                )
            })
            .unwrap_or((None, None));
        Origin::ResearchResult {
            task: None,
            run,
            project,
            topic,
            scroll: ScrollState {
                x: self.result_scroll.offset().x.as_f32(),
                y: pixels_y(&self.result_scroll),
            },
        }
    }

    fn results_run_for(&self, project: Option<u64>, topic: Option<u64>) -> Option<u64> {
        if project.is_none() && topic.is_none() {
            return None;
        }
        self.store
            .runs()
            .iter()
            .rev()
            .filter(|run| run.project_plan.is_none())
            .find(|run| {
                let task = self.store.task(run.task_id);
                let actual_project = task.map(|task| task.project_id);
                let actual_topic = task.and_then(|task| self.store.task_topic(task.id));
                project.is_none_or(|project| actual_project == Some(project))
                    && topic.is_none_or(|topic| actual_topic.is_some_and(|value| value.id == topic))
            })
            .map(|run| run.id)
    }

    /// Apply one validated origin. This never records another origin.
    pub(crate) fn apply_origin(&mut self, origin: Origin, w: &mut Window, cx: &mut Context<Self>) {
        match origin {
            Origin::ResearchList {
                scope,
                query,
                all_status,
                layout,
                scroll,
            } => {
                self.navigate(scope, w, cx);
                self.all_status = all_status;
                self.layout = layout;
                self.set_query(query.clone());
                self.search.update(cx, |s, cx| s.set_value(query, w, cx));
                self.navigation
                    .task_list_scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(scroll.outer.y)));
                self.navigation
                    .board_scroll
                    .set_offset(point(px(scroll.outer.x), px(0.)));
                for stage in Stage::BOARD_ORDER {
                    self.navigation
                        .board_column(stage)
                        .set_offset(point(px(0.), px(0.)));
                }
                for column in scroll.columns {
                    self.navigation
                        .board_column(column.stage)
                        .set_offset(point(px(0.), px(column.y)));
                }
            }
            Origin::ResearchTopic {
                topic,
                layout,
                scroll,
            } => {
                self.navigate(Scope::Topic(topic), w, cx);
                self.layout = layout;
                self.navigation
                    .topic_scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::ResearchTask {
                task,
                steps_tab,
                steps_layout,
                scroll,
            } => {
                self.open_task(task, w, cx);
                self.set_goal_steps_tab(steps_tab);
                self.set_steps_layout(steps_layout);
                self.detail_scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::ResearchResult {
                task,
                run,
                project,
                topic,
                scroll,
            } => {
                let run = run.or_else(|| self.results_run_for(project, topic));
                match (task, run) {
                    (Some(task), Some(run)) => {
                        self.open_task(task, w, cx);
                        self.set_result_selection(Some(task), Some(run));
                        self.show_run_result = true;
                        self.show_run_details = false;
                    }
                    (Some(task), None) => {
                        self.open_task(task, w, cx);
                        self.set_result_selection(Some(task), None);
                    }
                    (None, Some(run)) => {
                        self.set_result_selection(None, Some(run));
                        self.show_run_result = true;
                        self.show_run_details = false;
                    }
                    (None, None) => {
                        self.navigate(Scope::Runs, w, cx);
                    }
                }
                self.result_scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::SettingsPage { tab, scroll } => {
                self.navigate(Scope::Settings, w, cx);
                self.set_settings_tab(settings_tab(tab));
                self.settings_scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::AutomationList { scroll } => {
                self.open_automations(w, cx);
                self.automation_ui
                    .scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::AutomationResults {
                filter,
                history_limit,
                scroll,
            } => {
                self.navigate(Scope::AutomationRuns, w, cx);
                self.automation_ui.result_filter = filter;
                self.automation_ui.history_limit = history_limit;
                self.automation_ui
                    .scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::AutomationDetail { automation, scroll } => {
                self.open_automation(automation, w, cx);
                self.automation_ui
                    .scroll
                    .set_offset(point(px(scroll.x), px(scroll.y)));
            }
            Origin::AutomationSettings {
                automation,
                settings_scroll,
            } => {
                self.open_automation(automation, w, cx);
                self.open_automation_settings(w, cx);
                self.automation_ui
                    .settings_scroll
                    .set_offset(point(px(settings_scroll.x), px(settings_scroll.y)));
            }
            Origin::AutomationPeriod {
                automation,
                occurrence,
                under,
                dialog_scroll,
            } => {
                match under {
                    PeriodUnder::Detail { scroll } => {
                        self.open_automation(automation, w, cx);
                        self.automation_ui
                            .scroll
                            .set_offset(point(px(scroll.x), px(scroll.y)));
                    }
                    PeriodUnder::Results {
                        filter,
                        history_limit,
                        scroll,
                    } => {
                        self.navigate(Scope::AutomationRuns, w, cx);
                        self.automation_ui.result_filter = filter;
                        self.automation_ui.history_limit = history_limit;
                        self.automation_ui
                            .scroll
                            .set_offset(point(px(scroll.x), px(scroll.y)));
                    }
                }
                self.open_occurrence_result(occurrence, w, cx);
                self.automation_ui
                    .dialog_scroll
                    .set_offset(point(px(dialog_scroll.x), px(dialog_scroll.y)));
            }
        }
        cx.notify();
    }

    /// Open a destination selected by global search. The caller page is recorded
    /// so Back restores its filters and scroll (D5).
    pub(crate) fn attempt_navigation(
        &mut self,
        target: NavigationTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let frame = self.capture_return_frame();
        self.apply_target(target, w, cx);
        self.record_return_frame(frame);
        cx.notify();
    }

    fn apply_target(
        &mut self,
        target: NavigationTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            NavigationTarget::Research { scope } => self.navigate(scope, w, cx),
            NavigationTarget::ResearchTask(task) => self.open_task(task, w, cx),
            NavigationTarget::ResearchSteps(goal) => self.open_goal_steps(goal, w, cx),
            NavigationTarget::ResearchTaskResult(task) => {
                let result = if self.task_running(task) || self.goal_busy(task) {
                    None
                } else {
                    self.store.completed_sidebar_result(task)
                };
                if let Some(run) = result {
                    if self.defer_result_navigation(run, Some(task), true, None, cx) { return; }
                }
                self.open_task(task, w, cx);
                if let Some(run) = result {
                    self.show_task_result(run, cx);
                }
            }
            NavigationTarget::ResearchResult { task, run } => {
                if let Some(run) = run {
                    if self.defer_result_navigation(run, Some(task), false, None, cx) { return; }
                }
                self.set_result_selection(Some(task), run);
            }
            NavigationTarget::SettingsPage => self.navigate(Scope::Settings, w, cx),
            NavigationTarget::AutomationList => self.open_automations(w, cx),
            NavigationTarget::AutomationResults => self.navigate(Scope::AutomationRuns, w, cx),
            NavigationTarget::AutomationDetail(automation) => {
                self.open_automation(automation, w, cx)
            }
            NavigationTarget::AutomationSettings(automation) => {
                self.open_automation(automation, w, cx);
                self.open_automation_settings(w, cx);
            }
            NavigationTarget::AutomationPeriod {
                automation,
                occurrence,
            } => {
                self.open_automation(automation, w, cx);
                self.open_occurrence_result(occurrence, w, cx);
            }
            NavigationTarget::Recent { task, surface } => {
                self.open_recent(task, surface, w, cx)
            }
        }
    }

    pub(crate) fn open_recent(
        &mut self,
        task: u64,
        surface: RecentSurface,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.task(task).is_none() {
            return;
        }
        if surface == RecentSurface::Result {
            let run = self
                .store
                .research_result(task)
                .map(|run| run.id)
                .or_else(|| {
                    self.store
                        .runs()
                        .iter()
                        .rev()
                        .find(|run| run.task_id == task && run.project_plan.is_none())
                        .map(|run| run.id)
                });
            if let Some(run) = run {
                if self.defer_result_navigation(run, Some(task), true, None, cx) { return; }
                self.open_task(task, w, cx);
                self.show_task_result(run, cx);
                return;
            }
        }
        self.open_task(task, w, cx);
    }

    pub(crate) fn recent_target(&self, task: u64) -> NavigationTarget {
        NavigationTarget::Recent {
            task,
            surface: self
                .navigation
                .recent_surfaces
                .get(&task)
                .copied()
                .unwrap_or(RecentSurface::Task),
        }
    }

    pub(crate) fn set_automation_baseline(&mut self, cx: &App) {
        self.navigation.automation_baseline = Some(self.automation_ui.form_draft(cx));
    }

    /// Capture the editor's initial values, including project/stage/cycle, so
    /// only user edits (not unrelated runtime changes) make it dirty.
    pub(crate) fn begin_task_edit_baseline(&mut self, cx: &App) {
        if let Some(draft) = &self.draft {
            self.navigation.task_edit_baseline =
                Some((draft.id, draft_values(draft, cx)));
        }
    }

    /// One dirty computation shared by every exit path and by global search.
    pub(crate) fn dirty_draft(&self, cx: &App) -> Option<DraftSurface> {
        if let Some(draft) = &self.draft {
            let current = draft_values(draft, cx);
            let baseline = self
                .navigation
                .task_edit_baseline
                .as_ref()
                .filter(|(id, _)| *id == draft.id)
                .map(|(_, values)| values)
                .cloned()
                .or_else(|| {
                    self.store.task(draft.id).map(|task| TaskEditValues {
                        title: task.title.clone(),
                        description: task.description.clone(),
                        labels: task.labels.clone(),
                        due_date: task.due_date.clone(),
                        project: task.project_id,
                        stage: task.stage,
                        cycle: task.cycle.clone(),
                    })
                });
            if baseline
                .as_ref()
                .is_none_or(|stored| crate::draft_exit::task_edit_dirty(stored, &current))
            {
                return Some(DraftSurface::TaskEdit { task: draft.id });
            }
        }
        if self.automation_ui.creating {
            let current = self.automation_ui.form_draft(cx);
            let dirty = self
                .navigation
                .automation_baseline
                .as_ref()
                .map_or_else(
                    || {
                        self.automation_ui.editing.is_some()
                            || self.automation_ui.step > 1
                            || self.automation_ui.plan.is_some()
                            || !current.name.trim().is_empty()
                            || !current.objective.trim().is_empty()
                            || !current.expected.trim().is_empty()
                    },
                    |baseline| crate::draft_exit::automation_draft_dirty(baseline, &current),
                );
            if dirty {
                return Some(DraftSurface::AutomationWizard {
                    editing: self.automation_ui.editing,
                });
            }
        }
        if self.planning_destination.is_some() || self.planning_destination_error.is_some() {
            let current = self.project_brief.read(cx).value().to_string();
            if crate::draft_exit::text_draft_dirty(&self.navigation.planning_baseline, &current) {
                return Some(DraftSurface::PlanningRequest);
            }
        }
        if let Some(task) = self.selected().or(self.task_result_context()) {
            let current = self.comment.read(cx).value().to_string();
            if crate::draft_exit::comment_draft_dirty(&current) {
                return Some(DraftSurface::Comment { task });
            }
        }
        if self.scope() == Scope::Research {
            let title = self.research_title.read(cx).value().to_string();
            let brief = self.research_brief.read(cx).value().to_string();
            if !title.trim().is_empty() || !brief.trim().is_empty() {
                let unchanged = self.navigation.composer.as_ref().is_some_and(|draft| {
                    draft.project == self.research_project
                        && draft.topic == self.research_topic
                        && draft.title == title
                        && draft.brief == brief
                });
                if !unchanged {
                    return Some(DraftSurface::ResearchComposer);
                }
            }
        }
        None
    }

    /// Ask to leave a surface. Dirty Decision surfaces open the one dialog;
    /// Preserved surfaces navigate and keep their parked text; clean surfaces
    /// proceed immediately.
    pub(crate) fn request_exit(
        &mut self,
        target: ExitTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.pending_result_navigation = None;
        if self.navigation.pending_exit.is_some() {
            return false;
        }
        if let Some(surface) = self
            .dirty_draft(cx)
            .and_then(crate::draft_exit::decision_surface)
        {
            self.navigation.decision_return_focus = w.focused(cx);
            self.navigation.pending_exit = Some(PendingExit {
                surface,
                target,
                error: None,
                notice: None,
                from_search: false,
            });
            w.focus(&self.modal_focus, cx);
            cx.notify();
            return false;
        }
        self.execute_exit(target, w, cx);
        true
    }

    pub(crate) fn decide_exit(
        &mut self,
        decision: ExitDecision,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.navigation.pending_exit.take() else {
            return;
        };
        if self.navigation.pending_saved_exit.is_some() {
            self.navigation.pending_exit = Some(pending);
            return;
        }
        let exit_token = if decision == ExitDecision::Save {
            let token = self.navigation.next_exit_token;
            self.navigation.next_exit_token = self.navigation.next_exit_token.wrapping_add(1);
            self.navigation.pending_exit_token = Some(token);
            Some(token)
        } else {
            None
        };
        let save = if decision == ExitDecision::Save {
            self.save_decision_surface(&pending, exit_token, w, cx)
        } else {
            SaveOutcome::Saved
        };
        if !matches!(save, SaveOutcome::Pending(_)) {
            self.navigation.pending_exit_token = None;
        }
        if decision != ExitDecision::Save {
            self.navigation.after_save_target = None;
        }
        match crate::draft_exit::resolve_decision(&pending, decision, save) {
            DecisionOutcome::Proceed(target) => {
                self.navigation.decision_return_focus = None;
                if pending.from_search {
                    self.close_global_search(w, cx);
                }
                if decision == ExitDecision::Discard {
                    self.discard_surface(&pending.surface, w, cx);
                }
                self.execute_exit(target, w, cx);
            }
            DecisionOutcome::Pending { message } => {
                let mut pending = pending;
                pending.error = None;
                pending.notice = Some(message);
                self.navigation.pending_exit = Some(pending);
                w.focus(&self.modal_focus, cx);
                cx.notify();
            }
            DecisionOutcome::Stay { error } => {
                let from_search = pending.from_search;
                if let Some(error) = error {
                    let mut pending = pending;
                    pending.error = Some(error);
                    self.navigation.pending_exit = Some(pending);
                    w.focus(&self.modal_focus, cx);
                } else {
                    // Stay dismisses the decision and leaves the dirty surface
                    // exactly as it was; the next exit attempt asks again.
                    self.navigation.after_save_target = None;
                    let restore = self.navigation.decision_return_focus.take();
                    if from_search {
                        // Search keeps its own overlay focus so Enter can open the
                        // selected hit again after Stay.
                        self.open_global_search(w, cx);
                    }
                    if let Some(handle) = restore {
                        w.focus(&handle, cx);
                    } else if !from_search {
                        w.focus(&self.focus, cx);
                    }
                }
                cx.notify();
            }
        }
    }

    fn save_decision_surface(
        &mut self,
        pending: &PendingExit,
        exit_token: Option<u64>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> SaveOutcome {
        self.navigation.pending_saved_exit = None;
        match &pending.surface {
            DecisionSurface::TaskEdit { .. } => {
                use crate::task_edit_app::TaskSaveOutcome;
                match self.request_save_draft_outcome(w, cx) {
                    TaskSaveOutcome::Saved | TaskSaveOutcome::NoChanges => SaveOutcome::Saved,
                    TaskSaveOutcome::Saving => {
                        let instance = self.task_editor_instance();
                        let Some(values) = self.draft_editor_values(cx) else {
                            return SaveOutcome::Rejected(
                                "The task edit closed before it could be acknowledged.".into(),
                            );
                        };
                        self.navigation.pending_saved_exit = Some(PendingSavedExit {
                            surface: pending.surface.clone(),
                            target: pending.target.clone(),
                            from_search: pending.from_search,
                            token: exit_token.unwrap_or(0),
                            durable: false,
                            closed_by_completion: false,
                            form: PendingSavedExitForm::Task {
                                task: match &pending.surface {
                                    DecisionSurface::TaskEdit { task } => *task,
                                    _ => 0,
                                },
                                instance,
                                values,
                            },
                        });
                        SaveOutcome::Pending(
                            "Saving… leaving after the task edit is durable.".into(),
                        )
                    }
                    TaskSaveOutcome::PendingImpact => {
                        let instance = self.task_editor_instance();
                        let Some(values) = self.draft_editor_values(cx) else {
                            return SaveOutcome::Rejected(
                                "The task edit closed before it could be acknowledged.".into(),
                            );
                        };
                        self.navigation.pending_saved_exit = Some(PendingSavedExit {
                            surface: pending.surface.clone(),
                            target: pending.target.clone(),
                            from_search: pending.from_search,
                            token: exit_token.unwrap_or(0),
                            durable: false,
                            closed_by_completion: false,
                            form: PendingSavedExitForm::Task {
                                task: match &pending.surface {
                                    DecisionSurface::TaskEdit { task } => *task,
                                    _ => 0,
                                },
                                instance,
                                values,
                            },
                        });
                        self.navigation.after_save_target = None;
                        SaveOutcome::Pending(
                            "Review the save impact before leaving this task.".into(),
                        )
                    }
                    TaskSaveOutcome::PendingDurability(message)
                    | TaskSaveOutcome::RetryableFailure(message)
                    | TaskSaveOutcome::Rejected(message) => SaveOutcome::Rejected(message),
                }
            }
            DecisionSurface::AutomationWizard { .. } => {
                match self.save_automation_draft_outcome(cx) {
                    crate::automation_views::AutomationSaveOutcome::Saved => SaveOutcome::Saved,
                    crate::automation_views::AutomationSaveOutcome::Saving {
                        input,
                        subject,
                        form_epoch,
                    } => {
                        self.navigation.pending_saved_exit = Some(PendingSavedExit {
                            surface: pending.surface.clone(),
                            target: pending.target.clone(),
                            from_search: pending.from_search,
                            token: exit_token.unwrap_or(0),
                            durable: false,
                            closed_by_completion: false,
                            form: PendingSavedExitForm::Automation {
                                input,
                                subject,
                                epoch: form_epoch,
                            },
                        });
                        SaveOutcome::Pending(
                            "Saving… leaving after the draft is durable.".into(),
                        )
                    }
                    crate::automation_views::AutomationSaveOutcome::Rejected(message) => {
                        SaveOutcome::Rejected(message)
                    }
                }
            }
            DecisionSurface::PlanningRequest => {
                let Some(mode) = self.planning_destination else {
                    return SaveOutcome::Rejected(
                        "Choose a destination before saving this request.".into(),
                    );
                };
                let input = self.project_brief.read(cx).value().to_string();
                let topic = match mode {
                    CreationMode::ExistingTopic { topic, .. } => Some(topic),
                    _ => None,
                };
                match self.capture_research_request_outcome(
                    mode.project(),
                    topic,
                    input,
                    false,
                    cx,
                ) {
                    crate::project_app::RequestCaptureOutcome::Saved => SaveOutcome::Saved,
                    crate::project_app::RequestCaptureOutcome::Pending {
                        id,
                        input,
                        epoch,
                        destination,
                        start,
                        replan_source,
                    } => {
                        self.navigation.pending_saved_exit = Some(PendingSavedExit {
                            surface: pending.surface.clone(),
                            target: pending.target.clone(),
                            from_search: pending.from_search,
                            token: exit_token.unwrap_or(0),
                            durable: false,
                            closed_by_completion: false,
                            form: PendingSavedExitForm::Planning {
                                id,
                                input,
                                epoch,
                                destination,
                                start,
                                replan_source,
                            },
                        });
                        SaveOutcome::Pending(
                            "Saving… leaving after the request is durable.".into(),
                        )
                    }
                    crate::project_app::RequestCaptureOutcome::Rejected(message) => {
                        SaveOutcome::Rejected(message)
                    }
                }
            }
        }
    }

    fn discard_surface(
        &mut self,
        surface: &DecisionSurface,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match surface {
            DecisionSurface::TaskEdit { .. } => {
                self.draft = None;
                let _ = self.close_task_editor_overlay();
                self.navigation.task_edit_baseline = None;
                self.error.clear();
                w.focus(&self.focus, cx);
            }
            DecisionSurface::AutomationWizard { .. } => {
                self.automation_ui.creating = false;
                self.automation_ui.planning_automation = None;
                self.automation_ui.planning_run = None;
                self.navigation.automation_baseline = None;
                self.automation_ui.feedback = None;
            }
            DecisionSurface::PlanningRequest => {
                let opener = self.planning_opener_focus.take();
                if self.planning_destination.is_some() {
                    self.bump_planning_form_epoch();
                }
                self.planning_destination = None;
                self.planning_destination_error = None;
                self.planning_project = None;
                self.planning_topic = None;
                self.planning_replan_source = None;
                self.navigation.planning_baseline.clear();
                self.error.clear();
                if let Some(opener) = opener {
                    w.focus(&opener, cx);
                } else {
                    w.focus(&self.focus, cx);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn execute_exit(
        &mut self,
        target: ExitTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            ExitTarget::Back => self.go_back(w, cx),
            ExitTarget::Navigate(target) => self.attempt_navigation(target, w, cx),
            ExitTarget::Structural(target) => {
                self.structural_navigation();
                self.apply_target(target, w, cx);
            }
            ExitTarget::NewResearch => self.open_research_intake(w, cx),
            ExitTarget::SidebarTask { project, task } => {
                if !self.task_running(task) && !self.goal_busy(task) {
                    if let Some(run) = self.store.completed_sidebar_result(task) {
                        if self.defer_result_navigation(run, Some(task), true, Some(project), cx) { return; }
                    }
                }
                self.structural_navigation();
                self.navigate(Scope::Project(project), w, cx);
                self.open_sidebar_task(task, w, cx);
            }
            ExitTarget::Recent { task, surface } => {
                self.record_origin();
                self.open_recent(task, surface, w, cx);
            }
            ExitTarget::CancelWizard { editing } => {
                self.automation_ui.creating = false;
                self.automation_ui.planning_automation = None;
                self.automation_ui.planning_run = None;
                self.navigation.automation_baseline = None;
                self.automation_ui.feedback = None;
                if let Some(id) = editing {
                    self.open_automation(id, w, cx);
                } else {
                    self.open_automations(w, cx);
                }
            }
            ExitTarget::CancelPlanning => self.cancel_planning(w, cx),
            ExitTarget::Dismiss(surface) => self.dismiss_surface(surface, w, cx),
        }
        cx.notify();
    }

    fn dismiss_surface(
        &mut self,
        surface: crate::navigation_context::DismissibleSurface,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::navigation_context::DismissibleSurface as D;
        match surface {
            D::TaskEditor => {
                self.draft = None;
                let _ = self.close_task_editor_overlay();
                self.navigation.task_edit_baseline = None;
                self.error.clear();
                w.focus(&self.focus, cx);
            }
            D::PlanningComposer => self.cancel_planning(w, cx),
            D::PlanningAttempt => self.close_planning_attempt(w, cx),
            D::TaskResult => self.close_occurrence_result(w, cx),
            D::AutomationSettings => self.close_automation_settings(w, cx),
            D::AutomationPeriod => self.close_occurrence_result(w, cx),
            D::ProjectRename => {
                self.project_rename = None;
                self.restore_dialog_opener(w, cx);
            }
            D::TopicEditor => {
                self.topic_editor = None;
                self.restore_dialog_opener(w, cx);
            }
            D::TopicMove => {
                self.topic_move = None;
                self.restore_dialog_opener(w, cx);
            }
            D::FolderDialog => {
                self.directory_project = None;
                self.restore_dialog_opener(w, cx);
            }
            D::NewProject => {
                self.new_project = None;
                self.restore_dialog_opener(w, cx);
            }
            D::GlobalSearch => self.close_global_search(w, cx),
        }
        cx.notify();
    }

    /// A synchronous task save has already completed the captured exit. Only
    /// the exact task/form identity that produced this save may be marked;
    /// an unrelated or newer pending exit is left untouched.
    pub(crate) fn complete_task_saved_exit(
        &mut self,
        task: u64,
        instance: Option<crate::live_save_app::TaskEditorInstance>,
        values: &crate::draft_exit::TaskEditValues,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let matches = self
            .navigation
            .pending_saved_exit
            .as_ref()
            .is_some_and(|ack| {
                matches!(
                    &ack.form,
                    PendingSavedExitForm::Task {
                        task: expected_task,
                        instance: expected_instance,
                        values: expected_values,
                    } if *expected_task == task
                        && *expected_instance == instance
                        && expected_values == values
                )
            });
        if !matches {
            return false;
        }
        if let Some(ack) = &mut self.navigation.pending_saved_exit {
            ack.durable = true;
            ack.closed_by_completion = true;
        }
        self.poll_pending_saved_exit(w, cx);
        true
    }

    pub(crate) fn dirty_exit_durable(&mut self, token: u64) {
        if let Some(ack) = &mut self.navigation.pending_saved_exit
            && ack.token == token
        {
            ack.durable = true;
        }
    }

    pub(crate) fn dirty_exit_form_closed(
        &mut self,
        instance: Option<crate::live_save_app::TaskEditorInstance>,
    ) {
        if let Some(ack) = &mut self.navigation.pending_saved_exit
            && matches!(
                &ack.form,
                PendingSavedExitForm::Task {
                    instance: expected,
                    ..
                } if expected == &instance
            )
        {
            ack.closed_by_completion = true;
        }
    }

    pub(crate) fn dirty_exit_failed(
        &mut self,
        token: u64,
        reason: String,
        cx: &mut Context<Self>,
    ) {
        let matches = self
            .navigation
            .pending_saved_exit
            .as_ref()
            .is_some_and(|ack| ack.token == token);
        if !matches {
            return;
        }
        self.navigation.pending_saved_exit = None;
        if let Some(pending) = &mut self.navigation.pending_exit {
            pending.notice = None;
            pending.error = Some(format!(
                "The save could not be completed: {reason}. Review and retry."
            ));
        }
        cx.notify();
    }

    /// Poll a dirty-exit Save that is waiting for its exact durable receipt.
    /// A failed write keeps the decision modal and makes Save retryable. A
    /// newer/reopened form is preserved and the late exit is dropped.
    pub(crate) fn poll_pending_saved_exit(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(ack) = self.navigation.pending_saved_exit.take() else {
            return false;
        };
        if !ack.durable {
            self.navigation.pending_saved_exit = Some(ack);
            return false;
        }
        let same_form = match &ack.form {
            PendingSavedExitForm::Automation {
                input,
                subject,
                epoch,
            } => {
                let input_matches = self.automation_ui.creating
                    && self.automation_ui.form_epoch == *epoch
                    && self
                        .automation_draft_input(cx)
                        .ok()
                        .as_ref()
                        .is_some_and(|current| current == input);
                let subject_exists =
                    subject.is_none_or(|id| self.store.automation(id).is_some());
                input_matches && subject_exists
            }
            PendingSavedExitForm::Task {
                task: _,
                instance,
                values,
            } => {
                if self.task_editor_instance() == *instance {
                    self.draft_editor_values(cx).as_ref() == Some(values)
                } else {
                    // Only this exact completion may have closed the captured
                    // editor. A newer/reopened form is never approved by task
                    // existence alone.
                    ack.closed_by_completion && self.draft.is_none()
                }
            }
            PendingSavedExitForm::Planning {
                id,
                input,
                epoch,
                destination,
                replan_source,
                ..
            } => {
                let same_epoch = self.planning_form_epoch == *epoch;
                if self.planning_destination.is_some() {
                    same_epoch
                        && self.planning_destination == *destination
                        && self.planning_replan_source == *replan_source
                        && self.project_brief.read(cx).value().trim() == input.trim()
                } else {
                    same_epoch
                        && self
                            .store
                            .research_requests()
                            .iter()
                            .any(|request| request.id == *id)
                }
            }
        };
        if !same_form {
            // The user reopened or edited a newer form; never close it.
            self.navigation.pending_exit = None;
            cx.notify();
            return true;
        }
        self.navigation.pending_exit = None;
        self.discard_surface(&ack.surface, w, cx);
        if ack.from_search {
            self.close_global_search(w, cx);
        }
        self.execute_exit(ack.target, w, cx);
        true
    }

    /// Resume the exit that was waiting for T21's impact confirmation and a
    /// confirmed durable save. Called only after `save_draft` succeeded.
    pub(crate) fn finish_saved_exit(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some((target, from_search)) = self.navigation.after_save_target.take() else {
            return;
        };
        self.navigation.pending_exit = None;
        if from_search {
            self.close_global_search(w, cx);
        }
        self.execute_exit(target, w, cx);
    }

    pub(crate) fn request_back(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(ExitTarget::Back, w, cx);
    }

    pub(crate) fn request_scope(&mut self, scope: Scope, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(
            ExitTarget::Structural(NavigationTarget::Research { scope }),
            w,
            cx,
        );
    }

    pub(crate) fn request_goal_steps(&mut self, goal: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(
            ExitTarget::Navigate(NavigationTarget::ResearchSteps(goal)),
            w,
            cx,
        );
    }

    pub(crate) fn request_task(
        &mut self,
        task: u64,
        structural: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = if structural {
            ExitTarget::Structural(NavigationTarget::ResearchTask(task))
        } else {
            ExitTarget::Navigate(NavigationTarget::ResearchTask(task))
        };
        self.request_exit(target, w, cx);
    }

    /// Topic/list clicks open a completed task's result first, while keeping the
    /// same dirty decision and origin recording as an ordinary task click.
    pub(crate) fn request_task_result_first(
        &mut self,
        task: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_exit(
            ExitTarget::Navigate(NavigationTarget::ResearchTaskResult(task)),
            w,
            cx,
        );
    }

    pub(crate) fn request_sidebar_task(
        &mut self,
        project: u64,
        task: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_exit(ExitTarget::SidebarTask { project, task }, w, cx);
    }

    pub(crate) fn request_recent(
        &mut self,
        task: u64,
        surface: RecentSurface,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_exit(ExitTarget::Recent { task, surface }, w, cx);
    }

    pub(crate) fn request_new_research(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(ExitTarget::NewResearch, w, cx);
    }

    pub(crate) fn request_cancel_planning(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(ExitTarget::CancelPlanning, w, cx);
    }

    pub(crate) fn request_cancel_wizard(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(
            ExitTarget::CancelWizard {
                editing: self.automation_ui.editing,
            },
            w,
            cx,
        );
    }

    pub(crate) fn request_cancel_task_edit(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.request_exit(
            ExitTarget::Dismiss(crate::navigation_context::DismissibleSurface::TaskEditor),
            w,
            cx,
        );
    }

    /// Escape closes only the topmost surface. A dirty one raises the decision;
    /// a pending decision treats Escape as Stay.
    pub(crate) fn request_escape(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.research_save_impact.is_some() {
            self.cancel_research_save(w, cx);
            return true;
        }
        if self.previous_versions_task.is_some() {
            self.close_previous_versions(w, cx);
            return true;
        }
        if self.navigation.pending_exit.is_some() {
            self.decide_exit(ExitDecision::Stay, w, cx);
            return true;
        }
        use crate::navigation_context::DismissibleSurface as D;
        let target = if self.planning_destination.is_some()
            || self.planning_destination_error.is_some()
        {
            ExitTarget::Dismiss(D::PlanningComposer)
        } else if self.draft.is_some() {
            ExitTarget::Dismiss(D::TaskEditor)
        } else if self.planning_attempt_overlay().is_some() {
            ExitTarget::Dismiss(D::PlanningAttempt)
        } else if self.period_overlay().is_some() {
            ExitTarget::Dismiss(D::TaskResult)
        } else if self.settings_overlay() {
            ExitTarget::Dismiss(D::AutomationSettings)
        } else if self.project_rename.is_some() {
            ExitTarget::Dismiss(D::ProjectRename)
        } else if self.topic_editor.is_some() {
            ExitTarget::Dismiss(D::TopicEditor)
        } else if self.topic_move.is_some() {
            ExitTarget::Dismiss(D::TopicMove)
        } else if self.directory_project.is_some() {
            ExitTarget::Dismiss(D::FolderDialog)
        } else if self.new_project.is_some() {
            ExitTarget::Dismiss(D::NewProject)
        } else {
            return false;
        };
        self.request_exit(target, w, cx);
        true
    }

    /// Global-search activation passes through the same dirty gate. A decision
    /// closes the overlay only after Save/Discard proceeds.
    pub(crate) fn request_search_target(
        &mut self,
        target: NavigationTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.pending_exit.is_some() {
            return;
        }
        if let Some(surface) = self
            .dirty_draft(cx)
            .and_then(crate::draft_exit::decision_surface)
        {
            self.navigation.decision_return_focus = w.focused(cx);
            self.navigation.pending_exit = Some(PendingExit {
                surface,
                target: ExitTarget::Navigate(target),
                error: None,
                notice: None,
                from_search: true,
            });
            w.focus(&self.modal_focus, cx);
            cx.notify();
            return;
        }
        self.close_global_search(w, cx);
        self.attempt_navigation(target, w, cx);
    }

    pub(crate) fn discard_parked_composer(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.navigation.composer = None;
        self.research_title.update(cx, |s, cx| s.set_value("", w, cx));
        self.research_brief.update(cx, |s, cx| s.set_value("", w, cx));
        cx.notify();
    }

    pub(crate) fn park_current_comment(&mut self, cx: &App) {
        if let Some(task) = self.selected() {
            let text = self.comment.read(cx).value().to_string();
            self.navigation.comments.set(task, text);
        }
    }

    pub(crate) fn restore_task_comment(
        &mut self,
        task: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self
            .navigation
            .comments
            .get(task)
            .unwrap_or_default()
            .to_string();
        self.comment.update(cx, |s, cx| s.set_value(text, w, cx));
    }

    pub(crate) fn park_research_composer(&mut self, cx: &App) {
        if self.scope() != Scope::Research {
            return;
        }
        let title = self.research_title.read(cx).value().to_string();
        let brief = self.research_brief.read(cx).value().to_string();
        if title.trim().is_empty() && brief.trim().is_empty() {
            return;
        }
        self.navigation.composer = Some(ComposerDraft {
            project: self.research_project,
            topic: self.research_topic,
            title,
            brief,
        });
    }

    pub(crate) fn restore_research_composer(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.navigation.composer.clone() else {
            return;
        };
        self.research_project = draft.project;
        self.research_topic = draft.topic;
        self.research_title
            .update(cx, |s, cx| s.set_value(draft.title, w, cx));
        self.research_brief
            .update(cx, |s, cx| s.set_value(draft.brief, w, cx));
    }
}

/// A pure helper used by the generated route in tests and by future callers.
#[allow(dead_code)]
pub(crate) fn destination_project(mode: CreationMode) -> u64 {
    mode.project()
}

fn draft_values(draft: &crate::app::Draft, cx: &App) -> TaskEditValues {
    TaskEditValues {
        title: draft.title.read(cx).value().to_string(),
        description: draft.description.read(cx).value().to_string(),
        labels: draft
            .labels
            .read(cx)
            .value()
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .collect(),
        due_date: draft.due.read(cx).value().to_string(),
        project: draft.project,
        stage: draft.stage,
        cycle: draft.cycle.clone(),
    }
}
