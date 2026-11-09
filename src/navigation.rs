//! Structural navigation helpers. User drill-down Back restores the typed
//! return frame first; breadcrumb structure is the fallback and explicit parent
//! navigation path.
use crate::app::{Scope, TaskApp};
use crate::domain::Store;
use gpui_kit::*;

/// Which ordered collection owns the task currently being viewed.
///
/// Ordinary research keeps the documented project-tree traversal. Automation
/// steps are ordered by their frozen occurrence, so Previous/Next must not use
/// the shared automation project as a navigation pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskNavigationContext {
    ResearchTree,
    AutomationPeriod {
        automation: u64,
        occurrence: u64,
        /// Position in the period's ordered steps. `None` means the task is the
        /// period/goal overview rather than one of the executable steps.
        step_index: Option<usize>,
        step_count: usize,
    },
    /// The task still belongs to an automation, but its period is no longer
    /// available. Navigation stays disabled instead of falling back to another
    /// automation or generic run.
    AutomationUnavailable { automation: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NeighborTarget {
    Task(u64),
    AutomationOverview { automation: u64, occurrence: u64 },
}

/// One explicit Previous/Next destination, including how it should be named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskNeighbor {
    pub target: Option<NeighborTarget>,
    /// Used only when an intended task has disappeared or moved between
    /// rendering and activation. It always stays inside the same automation
    /// period.
    pub fallback: Option<NeighborTarget>,
    /// The automation period this destination was rendered for. A stale task
    /// that now belongs to another period is treated as missing.
    owner: Option<(u64, u64)>,
    pub tooltip: String,
}

impl TaskNeighbor {
    fn disabled(tooltip: impl Into<String>) -> Self {
        Self {
            target: None,
            fallback: None,
            owner: None,
            tooltip: tooltip.into(),
        }
    }
    fn task(
        target: u64,
        fallback: Option<NeighborTarget>,
        owner: Option<(u64, u64)>,
        tooltip: impl Into<String>,
    ) -> Self {
        Self {
            target: Some(NeighborTarget::Task(target)),
            fallback,
            owner,
            tooltip: tooltip.into(),
        }
    }
    /// Resolve the click destination now, so a deleted or moved target cannot
    /// open an unrelated task or a generic result page.
    pub(crate) fn resolved(&self, store: &crate::domain::Store) -> Option<NeighborTarget> {
        match self.target.as_ref() {
            Some(NeighborTarget::Task(id))
                if store.task(*id).is_some()
                    && self
                        .owner
                        .is_none_or(|(automation, occurrence)| {
                            store.automation_period_for_task(*id)
                                == Some((automation, occurrence))
                        }) =>
            {
                self.target.clone()
            }
            Some(NeighborTarget::AutomationOverview {
                automation,
                occurrence,
            }) if store
                .occurrence(*occurrence)
                .is_some_and(|value| value.automation_id == *automation) =>
            {
                self.target.clone()
            }
            _ => self.fallback.clone().filter(|fallback| match fallback {
                NeighborTarget::Task(id) => store.task(*id).is_some(),
                NeighborTarget::AutomationOverview {
                    automation,
                    occurrence,
                } => store
                    .occurrence(*occurrence)
                    .is_some_and(|value| value.automation_id == *automation),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskNavigation {
    pub context: TaskNavigationContext,
    pub previous: TaskNeighbor,
    pub next: TaskNeighbor,
    /// A separate action when the current task is an automation step. It opens
    /// the owning period overview rather than joining the step sequence.
    pub overview: Option<TaskNeighbor>,
}

impl crate::domain::Store {
    /// Sidebar defaults only to a current completed result, never a historical attempt.
    pub(crate) fn completed_sidebar_result(&self, id: u64) -> Option<u64> {
        use crate::{domain::Stage, research::ResearchKind};
        let task = self.task(id)?;
        if task.stage != Stage::Done || self.task_status(id, None).phase != Stage::Done {
            return None;
        }
        match task.research.as_ref().map(|r| r.kind) {
            Some(ResearchKind::Goal) => {
                if !self.goal_tasks_complete(id) { return None; }
                let children = self.research_children(id);
                children.iter().rev()
                    .filter(|child| self.task(**child).and_then(|t| t.research.as_ref()).is_some_and(|r| r.kind == ResearchKind::Advise))
                    .chain(children.iter().rev())
                    .find_map(|child| self.research_result(*child).map(|r| r.id))
            }
            Some(_) => self.research_result(id).map(|r| r.id),
            None => self.runs().iter().rev().find(|r| r.task_id == id && r.project_plan.is_none())
                .filter(|r| r.status == "Succeeded" && !r.output.trim().is_empty()).map(|r| r.id),
        }
    }

    /// Detail navigation follows the project tree, independently of list filters,
    /// task stages and collapsed rows. Wrap at the ends to keep both actions available.
    pub(crate) fn detail_task_neighbors(&self, id: u64) -> (u64, u64) {
        let Some(current) = self.task(id) else { return (id, id); };
        let archived = self.task_is_archived(id);
        let mut tasks: Vec<_> = self.tasks().iter().filter(|task|
            task.project_id == current.project_id && self.task_is_archived(task.id) == archived).collect();
        tasks.sort_by(|a, b| crate::display_order::tree_siblings_in_display_order(a, b));
        let mut roots: Vec<_> = tasks.iter().copied().filter(|task|
            task.parent_id.is_none_or(|parent| !tasks.iter().any(|t| t.id == parent))).collect();
        roots.sort_by(|a, b| self.compare_display_tasks(a, b));
        fn append(tasks: &[&crate::domain::Task], id: u64, ordered: &mut Vec<u64>) {
            if ordered.contains(&id) { return; }
            ordered.push(id);
            for child in tasks.iter().filter(|t| t.parent_id == Some(id)) {
                append(tasks, child.id, ordered);
            }
        }
        let mut ordered = vec![];
        for root in roots { append(&tasks, root.id, &mut ordered); }
        let Some(index) = ordered.iter().position(|task| *task == id) else { return (id, id); };
        (ordered[(index + ordered.len() - 1) % ordered.len()], ordered[(index + 1) % ordered.len()])
    }

    /// The automation occurrence that owns this task, derived from the
    /// occurrence's immutable goal IDs rather than from the shared project.
    pub(crate) fn automation_period_for_task(&self, id: u64) -> Option<(u64, u64)> {
        let task = self.task(id)?;
        let automation = task.automation_id?;
        let goal = task.research.as_ref()?.goal_id;
        self.all_occurrences()
            .iter()
            .rev()
            .find(|occurrence| {
                occurrence.automation_id == automation
                    && (occurrence.research_goal_id == Some(goal)
                        || occurrence.research_goal_ids.contains(&goal))
            })
            .map(|occurrence| (automation, occurrence.id))
    }

    /// Steps of one period in the order the period presents and executes them:
    /// approved goals in order, then the workflow order inside each goal. Goals
    /// themselves are overviews and deliberately do not appear here.
    pub(crate) fn automation_period_steps(&self, occurrence_id: u64) -> Vec<u64> {
        let Some(occurrence) = self.occurrence(occurrence_id) else {
            return vec![];
        };
        let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else {
            occurrence.research_goal_ids.clone()
        };
        let mut ordered = vec![];
        for goal in goals {
            let mut steps: Vec<_> = self
                .research_children(goal)
                .into_iter()
                .filter(|id| self.task(*id).is_some())
                .collect();
            steps.sort_by(|a, b| match (self.task(*a), self.task(*b)) {
                (Some(a), Some(b)) => crate::display_order::siblings_in_display_order(a, b),
                _ => a.cmp(b),
            });
            ordered.extend(steps);
        }
        ordered
    }

    /// Explicit navigation context for Previous/Next and the separate overview
    /// action. Ordinary research still wraps through its project tree.
    pub(crate) fn task_navigation(&self, id: u64) -> TaskNavigation {
        let Some(task) = self.task(id) else {
            return TaskNavigation {
                context: TaskNavigationContext::ResearchTree,
                previous: TaskNeighbor::disabled("Previous task unavailable"),
                next: TaskNeighbor::disabled("Next task unavailable"),
                overview: None,
            };
        };
        if let Some(automation) = task.automation_id {
            let Some((_, occurrence_id)) = self.automation_period_for_task(id) else {
                return TaskNavigation {
                    context: TaskNavigationContext::AutomationUnavailable { automation },
                    previous: TaskNeighbor::disabled(
                        "Automation step · its period is no longer available",
                    ),
                    next: TaskNeighbor::disabled(
                        "Automation step · its period is no longer available",
                    ),
                    overview: None,
                };
            };
            let steps = self.automation_period_steps(occurrence_id);
            let position = steps.iter().position(|step| *step == id);
            let overview = NeighborTarget::AutomationOverview {
                automation,
                occurrence: occurrence_id,
            };
            let automation_name = self
                .automation(automation)
                .map(|definition| definition.name.clone())
                .unwrap_or_else(|| format!("Automation {automation}"));
            let previous = match position {
                Some(0) => TaskNeighbor::disabled(format!(
                    "Start of Period {occurrence_id} · no previous step"
                )),
                Some(index) => TaskNeighbor::task(
                    steps[index - 1],
                    Some(overview.clone()),
                    Some((automation, occurrence_id)),
                    self.automation_step_tooltip("Previous step", steps[index - 1], occurrence_id),
                ),
                None => TaskNeighbor::disabled(format!(
                    "Period {occurrence_id} overview · step navigation starts in the step list"
                )),
            };
            let next = match position {
                Some(index) if index + 1 == steps.len() => TaskNeighbor::disabled(format!(
                    "End of Period {occurrence_id} · no next step"
                )),
                Some(index) => TaskNeighbor::task(
                    steps[index + 1],
                    Some(overview.clone()),
                    Some((automation, occurrence_id)),
                    self.automation_step_tooltip("Next step", steps[index + 1], occurrence_id),
                ),
                None => TaskNeighbor::disabled(format!(
                    "Period {occurrence_id} overview · step navigation starts in the step list"
                )),
            };
            return TaskNavigation {
                context: TaskNavigationContext::AutomationPeriod {
                    automation,
                    occurrence: occurrence_id,
                    step_index: position,
                    step_count: steps.len(),
                },
                previous,
                next,
                overview: Some(TaskNeighbor {
                    target: Some(overview),
                    fallback: None,
                    owner: None,
                    tooltip: format!(
                        "Open Period {occurrence_id} overview · {automation_name}"
                    ),
                }),
            };
        }
        let (previous, next) = self.detail_task_neighbors(id);
        TaskNavigation {
            context: TaskNavigationContext::ResearchTree,
            previous: TaskNeighbor::task(
                previous,
                None,
                None,
                self.ordinary_task_tooltip("Previous task", previous),
            ),
            next: TaskNeighbor::task(
                next,
                None,
                None,
                self.ordinary_task_tooltip("Next task", next),
            ),
            overview: None,
        }
    }

    fn ordinary_task_tooltip(&self, action: &str, id: u64) -> String {
        self.task(id)
            .map(|task| format!("{action} · {}", task.title))
            .unwrap_or_else(|| action.into())
    }

    fn automation_step_tooltip(&self, action: &str, id: u64, occurrence: u64) -> String {
        let Some(task) = self.task(id) else {
            return format!("{action} · Period {occurrence}");
        };
        let kind = match task.research.as_ref().map(|research| research.kind) {
            Some(crate::research::ResearchKind::Collect) => "Evidence",
            Some(crate::research::ResearchKind::Analyze) => "Analysis",
            Some(crate::research::ResearchKind::Advise) => "Final report",
            _ => "Step",
        };
        format!(
            "{action} · {kind} · {} · Period {occurrence}",
            task.title
        )
    }

    /// Labels used by the recently opened menu so automation steps carry their
    /// definition and period identity instead of looking like generic tasks.
    pub(crate) fn task_navigation_label(&self, id: u64) -> String {
        let Some(task) = self.task(id) else {
            return id.to_string();
        };
        let base = format!("{} · {}", self.identifier(task), task.title);
        let Some(automation) = task.automation_id else {
            return base;
        };
        if let Some((_, occurrence)) = self.automation_period_for_task(id) {
            let name = self
                .automation(automation)
                .map(|definition| definition.name.as_str())
                .unwrap_or("Automation");
            format!("{name} · Period {occurrence} · {base}")
        } else {
            format!("Automation {automation} · Period unavailable · {base}")
        }
    }
}

#[derive(Clone)]
pub(crate) enum CrumbTarget {
    View(Scope),
    Task(u64),
    Automation(u64),
    AutomationDetail(u64),
    AutomationResult { automation: u64, occurrence: u64 },
}
impl TaskApp {
    pub(crate) fn open_sidebar_task(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let result = if self.task_running(id) || self.goal_busy(id) { None }
            else { self.store.completed_sidebar_result(id) };
        self.open_task(id, window, cx);
        if let Some(run) = result { self.show_task_result(run, cx); }
    }

    /// Activate a resolved neighbor. Task detail keeps the detail surface; the
    /// result surface preserves the established result-first behavior.
    pub(crate) fn open_navigation_target(
        &mut self,
        target: &NeighborTarget,
        open_result: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            NeighborTarget::Task(id) => {
                if open_result {
                    self.open_sidebar_task(*id, window, cx);
                } else {
                    self.open_task(*id, window, cx);
                }
            }
            NeighborTarget::AutomationOverview {
                automation,
                occurrence,
            } => self.open_period_overview(*automation, *occurrence, window, cx),
        }
    }

    /// Resolve at activation time. A task that disappeared after rendering uses
    /// the same period's overview, never another automation or a generic run.
    pub(crate) fn open_task_neighbor(
        &mut self,
        neighbor: &TaskNeighbor,
        open_result: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = neighbor.resolved(&self.store) else {
            return;
        };
        self.open_navigation_target(&target, open_result, window, cx);
    }

    pub(crate) fn open_period_overview(
        &mut self,
        automation: u64,
        occurrence: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.automation(automation).is_none()
            || self
                .store
                .occurrence(occurrence)
                .is_none_or(|value| value.automation_id != automation)
        {
            return;
        }
        self.open_automation(automation, window, cx);
        self.open_occurrence_result(occurrence, window, cx);
    }

    pub(crate) fn displayed_run_id(&self) -> Option<u64> {
        resolved_result_run_id(
            &self.store,
            self.task_result_context(),
            self.period_overlay(),
            self.selected_run(),
        )
    }

    pub(crate) fn clear_navigation_overlays(&mut self) {
        self.save_feedback = None;
        self.draft = None;
        let _ = self.close_task_editor_overlay();
        self.navigation.task_edit_baseline = None;
        self.property = None;
        self.new_project = None;
        self.topic_editor = None;
        self.project_rename = None;
        self.topic_move = None;
        self.planning_project = None;
        self.planning_replan_source = None;
        self.planning_topic = None;
        if self.planning_destination.is_some() {
            self.bump_planning_form_epoch();
        }
        self.planning_destination = None;
        self.planning_destination_error = None;
        self.planning_opener_focus = None;
        self.directory_project = None;
        self.clear_typed_overlays();
        self.error.clear();
    }

    /// Return to an automation's detail page with the settings dialog that
    /// launched Edit or Replan reopened, so the flow does not fall back to All.
    pub(crate) fn reopen_automation_settings(&mut self, id: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.automation_ui.editing = None;
        self.open_automation(id, w, cx);
        self.open_automation_settings(w, cx);
    }

    pub(crate) fn go_back(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.has_return_frames() {
            match self.back_frame() {
                Ok(crate::navigation_route::NavEffect::Back { restored }) => {
                    self.apply_return_frame(restored.frame, w, cx);
                    if let Some(notice) = restored.notice {
                        self.toast = notice;
                    }
                    cx.notify();
                }
                Ok(_) => {}
                Err(_) => {}
            }
            return;
        }
        let (parents, _) = self.navigation_path();
        if let Some((_, _, target)) = parents.last() {
            match target {
                CrumbTarget::View(scope) => self.navigate(scope.clone(), w, cx),
                CrumbTarget::Task(id) => self.open_task(*id, w, cx),
                CrumbTarget::Automation(id) => self.reopen_automation_settings(*id, w, cx),
                CrumbTarget::AutomationDetail(id) => self.open_automation(*id, w, cx),
                CrumbTarget::AutomationResult { automation, occurrence } => {
                    // A step belongs to one period; Back restores that period's
                    // result over the automation detail instead of the list.
                    self.open_automation(*automation, w, cx);
                    self.open_occurrence_result(*occurrence, w, cx);
                }
            }
        }
    }
    pub(crate) fn close_current_task(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected() else {
            return;
        };
        self.tabs.retain(|task| *task != id);
        self.go_back(w, cx);
    }
}

/// One resolved result selection for the reader, breadcrumbs, history and typed
/// routes. Task/period context may select only runs owned by that context; the
/// global results page may select only ordinary, non-archived runs. A missing or
/// cross-context selection never falls back to an arbitrary global run.
pub(crate) fn resolved_result_run_id(
    store: &Store,
    context_task: Option<u64>,
    period: Option<u64>,
    selected_run: Option<u64>,
) -> Option<u64> {
    if let Some(task_id) = context_task {
        let task = store.task(task_id)?;
        let belongs = |run: &crate::domain::AgentRun| {
            if task.automation_id.is_some() {
                run.task_id == task_id
                    && store.run_is_automation_owned(run)
                    && period
                        .is_none_or(|occurrence| run.automation_occurrence_id == Some(occurrence))
            } else {
                !store.run_is_automation_owned(run)
                    && (run.task_id == task_id
                        || store.owning_research_goal(run.task_id) == Some(task_id))
            }
        };
        if let Some(id) = selected_run
            && let Some(run) = store.runs().iter().find(|run| run.id == id)
            && belongs(run)
        {
            return Some(run.id);
        }
        return store
            .runs()
            .iter()
            .rev()
            .find(|run| belongs(run))
            .map(|run| run.id);
    }
    if let Some(id) = selected_run
        && let Some(run) = store.runs().iter().find(|run| run.id == id)
        && store.run_is_ordinary_result(run)
    {
        return Some(run.id);
    }
    store.latest_ordinary_result_run().map(|run| run.id)
}

#[cfg(test)]
mod tests {
    use super::{NeighborTarget, TaskNavigationContext};
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Stage, Store};

    fn automation_store() -> Store {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        store.research_config_mut().models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        store
    }

    fn automation(store: &mut Store, name: &str, template: crate::automation::AutomationTemplate) -> u64 {
        store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                name.into(),
                "Track public filings".into(),
                "A cited note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                Some(template),
                1_000,
            )
            .unwrap()
    }

    fn period(store: &mut Store, automation: u64, scheduled: i64) -> u64 {
        let definition = store.automation(automation).unwrap().clone();
        let occurrence = AutomationOccurrence {
            id: store.next_occurrence_id().unwrap(),
            automation_id: automation,
            definition_version: definition.version,
            source: crate::automation::TriggerSource::Scheduled,
            trigger_key: crate::automation::scheduled_trigger_key(automation, scheduled),
            scheduled_for: Some(scheduled),
            requested_at: None,
            actual_started_at: Some(scheduled + 60),
            timezone: definition.schedule.timezone.clone(),
            reference_time: scheduled,
            data_window: crate::automation::TimeWindow::unresolved("test window"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: definition.objective.clone(),
                expected_result: definition.expected_result.clone(),
                shape: Some(definition.shape),
                template: definition.template.clone().unwrap(),
                models: ["test/plan", "test/collect", "test/analyze", "test/advise"]
                    .map(String::from),
                agents: Default::default(),
                materials: vec![],
                policy: definition.policy.clone(),
                approval_version: definition.version,
                approval_signature: "test approval".into(),
            },
            status: crate::automation::OccurrenceStatus::Queued,
            reason: None,
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: scheduled,
            deadline_at: None,
            retry_requested_at: None,
        };
        store.materialize_occurrence(occurrence).unwrap().id
    }

    fn template() -> crate::automation::AutomationTemplate {
        crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()
    }

    #[test]
    fn sidebar_requires_completion_and_a_successful_latest_result() {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        let id = store.create_task("Task", 1, Stage::Planned, None).unwrap();
        store.replace_policies(Store::mock().policies().to_vec());
        store.simulate_run(id).unwrap();
        let run_id = {
            let mut run = store.last_run_mut().unwrap();
            run.status = "Succeeded".into();
            run.output = "Final result".into();
            run.id
        };
        for stage in [Stage::Backlog, Stage::Planned, Stage::InProgress, Stage::Review, Stage::Cancelled] {
            store.task_mut(id).unwrap().stage = stage;
            assert_eq!(store.completed_sidebar_result(id), None);
        }
        store.task_mut(id).unwrap().stage = Stage::Done;
        assert_eq!(store.completed_sidebar_result(id), Some(run_id));
        for status in ["Failed", "Blocked", "Interrupted", "Cancelled", "Queued", "Running"] {
            store.last_run_mut().unwrap().status = status.into();
            assert_eq!(store.completed_sidebar_result(id), None);
        }
        store.last_run_mut().unwrap().status = "Succeeded".into();
        store.last_run_mut().unwrap().output.clear();
        assert_eq!(store.completed_sidebar_result(id), None);
    }

    #[test]
    fn detail_navigation_includes_completed_steps_and_wraps_across_topics() {
        let mut store = Store::empty();
        let topic_a = store.create_topic(1, "A").unwrap();
        let topic_b = store.create_topic(1, "B").unwrap();
        let a = store.create_research_goal_in_topic("A goal".into(), "Scope".into(), 1, Some(topic_a)).unwrap();
        // Create B before A's children: navigation must still follow the hierarchy.
        let b = store.create_research_goal_in_topic("B goal".into(), "Scope".into(), 1, Some(topic_b)).unwrap();
        let steps = store.apply_research_plan(a, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        store.task_mut(steps[0]).unwrap().stage = Stage::Done;
        store.task_mut(steps[1]).unwrap().stage = Stage::InProgress;
        store.task_mut(steps[2]).unwrap().stage = Stage::Cancelled;
        assert_eq!(store.detail_task_neighbors(steps[0]), (a, steps[1]));
        assert_eq!(store.detail_task_neighbors(steps[1]), (steps[0], steps[2]));
        assert_eq!(store.detail_task_neighbors(steps[2]), (steps[1], b));
        assert_eq!(store.detail_task_neighbors(b), (steps[2], a));
        assert_eq!(store.detail_task_neighbors(a), (b, steps[0]));
        let navigation = store.task_navigation(steps[2]);
        assert_eq!(navigation.context, TaskNavigationContext::ResearchTree);
        assert_eq!(navigation.previous.target, Some(NeighborTarget::Task(steps[1])));
        assert_eq!(navigation.next.target, Some(NeighborTarget::Task(b)));
        assert!(navigation.overview.is_none());
        assert!(navigation.next.tooltip.contains("Next task"));
    }

    #[test]
    fn single_task_navigation_stays_on_current_task() {
        let mut store = Store::empty();
        let id = store.create_research_goal("Only goal".into(), "Scope".into(), 1).unwrap();
        assert_eq!(store.detail_task_neighbors(id), (id, id));
    }

    #[test]
    fn automation_neighbors_stay_inside_one_period_across_definitions() {
        let mut store = automation_store();
        let first_automation = automation(&mut store, "First automation", template());
        let second_automation = automation(&mut store, "Second automation", template());
        let first_period = period(&mut store, first_automation, 1_000);
        let second_period = period(&mut store, first_automation, 3_000);
        let other_period = period(&mut store, second_automation, 5_000);
        let other_second_period = period(&mut store, second_automation, 7_000);
        let steps = store.automation_period_steps(first_period);
        assert_eq!(steps.len(), 3);
        assert_eq!(store.automation_period_steps(second_period).len(), 3);
        assert_eq!(store.automation_period_steps(other_period).len(), 3);
        assert_eq!(store.automation_period_steps(other_second_period).len(), 3);

        let first = store.task_navigation(steps[0]);
        assert_eq!(
            first.context,
            TaskNavigationContext::AutomationPeriod {
                automation: first_automation,
                occurrence: first_period,
                step_index: Some(0),
                step_count: 3,
            }
        );
        assert!(first.previous.target.is_none());
        assert!(first.previous.resolved(&store).is_none());
        assert_eq!(first.next.target, Some(NeighborTarget::Task(steps[1])));
        assert!(first.next.tooltip.contains("Next step"));
        assert!(first.next.tooltip.contains(&format!("Period {first_period}")));
        assert!(first.overview.is_some());

        let middle = store.task_navigation(steps[1]);
        assert_eq!(middle.previous.target, Some(NeighborTarget::Task(steps[0])));
        assert_eq!(middle.next.target, Some(NeighborTarget::Task(steps[2])));
        assert!(middle.previous.tooltip.contains("Previous step"));

        let last = store.task_navigation(steps[2]);
        assert_eq!(last.previous.target, Some(NeighborTarget::Task(steps[1])));
        assert!(last.next.target.is_none());
        assert!(last.next.resolved(&store).is_none());
        assert!(last.next.tooltip.contains(&format!("End of Period {first_period}")));

        // Two definitions with two periods each: every owner context navigates
        // only its own ordered steps and disables the first/last boundary.
        let owners = [
            (first_automation, first_period),
            (first_automation, second_period),
            (second_automation, other_period),
            (second_automation, other_second_period),
        ];
        for (automation, occurrence) in owners {
            let ordered = store.automation_period_steps(occurrence);
            assert_eq!(ordered.len(), 3);
            for (index, task) in ordered.iter().enumerate() {
                let navigation = store.task_navigation(*task);
                assert_eq!(
                    navigation.context,
                    TaskNavigationContext::AutomationPeriod {
                        automation,
                        occurrence,
                        step_index: Some(index),
                        step_count: ordered.len(),
                    }
                );
                assert_eq!(
                    navigation.previous.target,
                    index
                        .checked_sub(1)
                        .map(|previous| NeighborTarget::Task(ordered[previous]))
                );
                assert_eq!(
                    navigation.next.target,
                    ordered
                        .get(index + 1)
                        .copied()
                        .map(NeighborTarget::Task)
                );
                for neighbor in [&navigation.previous, &navigation.next] {
                    if let Some(NeighborTarget::Task(target)) = neighbor.target {
                        assert!(
                            ordered.contains(&target),
                            "Period {occurrence} reached another period"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn multi_goal_period_orders_all_steps_before_the_next_goal() {
        let mut store = automation_store();
        let mut template = template();
        let second_goal = template.goals[0].clone();
        template.goals.push(second_goal);
        let automation = automation(&mut store, "Two goals", template);
        let occurrence = period(&mut store, automation, 1_000);
        let occurrence_value = store.occurrence(occurrence).unwrap();
        assert_eq!(occurrence_value.research_goal_ids.len(), 2);
        let goals = occurrence_value.research_goal_ids.clone();
        let first_goal_steps = store.research_children(goals[0]);
        let second_goal_steps = store.research_children(goals[1]);
        let mut expected = first_goal_steps.clone();
        expected.sort_by(|a, b| {
            crate::display_order::siblings_in_display_order(
                store.task(*a).unwrap(),
                store.task(*b).unwrap(),
            )
        });
        let mut second = second_goal_steps.clone();
        second.sort_by(|a, b| {
            crate::display_order::siblings_in_display_order(
                store.task(*a).unwrap(),
                store.task(*b).unwrap(),
            )
        });
        expected.extend(second);
        assert_eq!(store.automation_period_steps(occurrence), expected);
        let last_of_first_goal = expected[first_goal_steps.len() - 1];
        let first_of_second_goal = expected[first_goal_steps.len()];
        assert_eq!(
            store.task_navigation(last_of_first_goal).next.target,
            Some(NeighborTarget::Task(first_of_second_goal))
        );
        // The first step of the second goal is still inside the same period.
        assert_eq!(
            store.task_navigation(first_of_second_goal).previous.target,
            Some(NeighborTarget::Task(last_of_first_goal))
        );
        // Goal overviews are not part of the ordered step sequence.
        let overview = store.task_navigation(goals[0]);
        assert!(overview.previous.target.is_none());
        assert!(overview.next.target.is_none());
        assert!(overview.overview.is_some());
    }

    #[test]
    fn missing_step_falls_back_to_the_same_period_overview() {
        let mut store = automation_store();
        let automation = automation(&mut store, "Fallback automation", template());
        let occurrence = period(&mut store, automation, 1_000);
        let steps = store.automation_period_steps(occurrence);
        let navigation = store.task_navigation(steps[0]);
        let next_task = steps[1];
        store.retain_tasks(|task| task.id != next_task);
        assert_eq!(
            navigation.next.resolved(&store),
            Some(NeighborTarget::AutomationOverview {
                automation,
                occurrence,
            }),
            "a deleted step must not open another period or a generic run"
        );
        assert!(store.task_navigation(steps[0]).next.resolved(&store).is_some());
    }

    #[test]
    fn moved_step_does_not_activate_across_periods() {
        let mut store = automation_store();
        let automation = automation(&mut store, "Moved step", template());
        let occurrence = period(&mut store, automation, 1_000);
        let second_period = period(&mut store, automation, 3_000);
        let steps = store.automation_period_steps(occurrence);
        let navigation = store.task_navigation(steps[0]);
        let next_task = steps[1];
        let second_goal = store
            .occurrence(second_period)
            .unwrap()
            .research_goal_id
            .unwrap();
        store
            .task_mut(next_task)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .goal_id = second_goal;
        assert_eq!(
            navigation.next.resolved(&store),
            Some(NeighborTarget::AutomationOverview {
                automation,
                occurrence,
            }),
            "a target moved to another period must not be activated"
        );
    }

    #[test]
    fn removed_period_disables_automation_navigation() {
        let mut store = automation_store();
        let automation = automation(&mut store, "Missing period", template());
        let occurrence = period(&mut store, automation, 1_000);
        let steps = store.automation_period_steps(occurrence);
        store.retain_occurrences(|candidate| candidate.id != occurrence);
        let navigation = store.task_navigation(steps[0]);
        assert_eq!(
            navigation.context,
            TaskNavigationContext::AutomationUnavailable { automation }
        );
        assert!(navigation.previous.resolved(&store).is_none());
        assert!(navigation.next.resolved(&store).is_none());
        assert!(navigation.overview.is_none());
    }

    #[test]
    fn completed_automation_step_keeps_report_and_period_context() {
        let mut store = automation_store();
        let automation = automation(&mut store, "Completed step", template());
        let occurrence = period(&mut store, automation, 1_000);
        let steps = store.automation_period_steps(occurrence);
        let step = steps[0];
        let mut run = store.prepare_run(step).unwrap();
        run.automation_occurrence_id = Some(occurrence);
        run.status = "Succeeded".into();
        run.output = "Completed report".into();
        let run_id = run.id;
        store.push_run(run);
        store.set_stage(step, Stage::Done).unwrap();
        assert_eq!(store.research_result(step).map(|run| run.id), Some(run_id));
        assert_eq!(store.completed_sidebar_result(step), Some(run_id));
        let navigation = store.task_navigation(step);
        assert!(matches!(
            navigation.context,
            TaskNavigationContext::AutomationPeriod {
                automation: owner,
                occurrence: owner_period,
                step_index: Some(0),
                ..
            } if owner == automation && owner_period == occurrence
        ));
        let label = store.task_navigation_label(step);
        assert!(label.contains("Completed step"), "{label}");
        assert!(label.contains(&format!("Period {occurrence}")), "{label}");
    }

    fn run_for(task: u64, id: u64, occurrence: Option<u64>, status: &str) -> crate::domain::AgentRun {
        crate::domain::AgentRun {
            automation_occurrence_id: occurrence,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id,
            task_title: format!("Task {task}"),
            directory: String::new(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            task_id: task,
            agent: "test".into(),
            model: "test/model".into(),
            stage: Stage::Done,
            status: status.into(),
            logs: vec![],
            output: "result".into(),
            attachments: vec![],
            research_kind: None,
            sources: vec![],
            input_signature: String::new(),
        }
    }

    #[test]
    fn result_selection_stays_inside_the_task_context() {
        let mut store = Store::mock();
        store.push_run(run_for(1, 10, None, "Succeeded"));
        store.push_run(run_for(2, 11, None, "Succeeded"));
        // A selected run from another task must not win over the task context.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, Some(11)),
            Some(10)
        );
        // A deleted selection falls back to a run in the same task only.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, Some(999)),
            Some(10)
        );
        // No run for the context is an explicit empty selection, never a global run.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(4), None, None),
            None
        );
    }

    #[test]
    fn global_results_select_only_ordinary_runs() {
        let mut store = Store::mock();
        store.push_run(run_for(1, 10, None, "Succeeded"));
        store.push_run(run_for(2, 11, Some(5), "Succeeded"));
        // An automation-owned selected run is never used globally.
        assert_eq!(
            super::resolved_result_run_id(&store, None, None, Some(11)),
            Some(10)
        );
        assert_eq!(
            super::resolved_result_run_id(&store, None, None, Some(10)),
            Some(10)
        );
        assert_eq!(super::resolved_result_run_id(&store, None, None, Some(999)), Some(10));
    }

    #[test]
    fn archived_task_context_keeps_its_history_readable() {
        let mut store = Store::mock();
        store.push_run(run_for(1, 10, None, "Succeeded"));
        store.task_mut(1).unwrap().archived = true;
        // Global results exclude archived history...
        assert_eq!(super::resolved_result_run_id(&store, None, None, None), None);
        // ...but an explicit archived task context still resolves its own run.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, None),
            Some(10)
        );
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, Some(10)),
            Some(10)
        );
    }

    #[test]
    fn goal_context_includes_its_child_step_report() {
        let mut store = Store::mock();
        let child = 4;
        store.push_run(run_for(child, 30, None, "Succeeded"));
        store.push_run(run_for(2, 31, None, "Succeeded"));
        store.task_mut(child).unwrap().research = Some(crate::research::ResearchTask {
            requires_approval: false,
            kind: crate::research::ResearchKind::Advise,
            goal_id: 1,
            depends_on: vec![],
            space_name: String::new(),
        });
        // The goal context resolves its child step's report.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, Some(30)),
            Some(30)
        );
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, None),
            Some(30)
        );
        // An unrelated goal's run never wins.
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, Some(31)),
            Some(30)
        );
        // Archived goal keeps the same child report readable.
        store.task_mut(1).unwrap().archived = true;
        assert_eq!(
            super::resolved_result_run_id(&store, Some(1), None, None),
            Some(30)
        );
        // Global results exclude the archived goal and its child tree.
        assert_eq!(super::resolved_result_run_id(&store, None, None, None), None);
    }

    #[test]
    fn period_context_selects_only_the_same_occurrence() {
        let mut store = Store::mock();
        let task = 1;
        store.task_mut(task).unwrap().automation_id = Some(7);
        store.push_run(run_for(task, 20, Some(5), "Succeeded"));
        store.push_run(run_for(task, 21, Some(6), "Succeeded"));
        assert_eq!(
            super::resolved_result_run_id(&store, Some(task), Some(5), Some(21)),
            Some(20)
        );
        assert_eq!(
            super::resolved_result_run_id(&store, Some(task), Some(5), None),
            Some(20)
        );
        assert_eq!(
            super::resolved_result_run_id(&store, Some(task), Some(9), None),
            None
        );
    }
}
