//! Pure return-context policy for T10.
//!
//! This module deliberately contains no `TaskApp`, rendering or window code. It
//! defines the difference between an explicit structural navigation (sidebar,
//! breadcrumb, footer target) and a user drill-down, the bounded origin stack
//! used by Back, the owner-validation and fallback ladder, and the visible
//! destination rules for research creation.
//!
//! The application captures an `Origin` from its current UI state, records it
//! through `OriginStack::apply_intent`, and later asks the stack for a validated
//! route. Background completion paths must never call these methods.

use crate::app::{AllStatus, Layout, Scope};
use crate::domain::{Stage, Store};

/// Maximum number of chained drill-downs remembered for Back.
pub(crate) const MAX_ORIGIN_DEPTH: usize = 8;

/// Minimum number of automation periods a restored results page shows before
/// the user asks for more.
pub(crate) const DEFAULT_RESULTS_LIMIT: usize = 20;

/// Scroll offsets captured at the moment of a drill-down. Values are plain
/// floats so this module stays independent of the rendering layer; the caller
/// converts them to and from `Pixels`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ScrollState {
    pub x: f32,
    pub y: f32,
}

/// A visible board column and its vertical offset. The board itself scrolls
/// horizontally; each column scrolls vertically.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoardAnchor {
    pub stage: Stage,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResearchScroll {
    /// Horizontal board offset in `x`; virtualized list offset in `y`.
    pub outer: ScrollState,
    /// Vertical offset of every board column that was scrolled.
    pub columns: Vec<BoardAnchor>,
}

/// Which page a research period dialog was opened over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PeriodUnder {
    Detail {
        scroll: ScrollState,
    },
    Results {
        filter: Option<u64>,
        history_limit: usize,
        scroll: ScrollState,
    },
}

/// A snapshot of the state a user should return to. Origins contain only
/// navigation state (route, filters, scroll, selection); they never contain
/// domain data or pending commands, and restoring one never re-executes work.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Origin {
    ResearchList {
        scope: Scope,
        query: String,
        all_status: AllStatus,
        layout: Layout,
        scroll: ResearchScroll,
    },
    ResearchTopic {
        topic: u64,
        layout: Layout,
        scroll: ScrollState,
    },
    ResearchTask {
        task: u64,
        steps_tab: Option<u64>,
        steps_layout: Layout,
        scroll: ScrollState,
    },
    ResearchResult {
        /// Selected task/goal filter, if the caller had one.
        task: Option<u64>,
        /// Selected report/run. `None` is a legitimate empty or waiting results
        /// page, not a missing owner.
        run: Option<u64>,
        /// Selected project filter of the results page.
        project: Option<u64>,
        /// Selected research-topic filter of the results page.
        topic: Option<u64>,
        scroll: ScrollState,
    },
    SettingsPage {
        tab: usize,
        scroll: ScrollState,
    },
    AutomationList {
        scroll: ScrollState,
    },
    AutomationResults {
        filter: Option<u64>,
        history_limit: usize,
        scroll: ScrollState,
    },
    AutomationDetail {
        automation: u64,
        scroll: ScrollState,
    },
    AutomationSettings {
        automation: u64,
        settings_scroll: ScrollState,
    },
    AutomationPeriod {
        automation: u64,
        occurrence: u64,
        under: PeriodUnder,
        dialog_scroll: ScrollState,
    },
}

/// Which surface a Recently opened entry represents. A report entry must open
/// the report, not the task detail page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecentSurface {
    Task,
    Result,
}

/// An explicit destination. This is intentionally smaller than T12's future
/// typed route: it only describes targets the T10 handlers need while keeping
/// structural and drill-down intents distinct.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NavigationTarget {
    Research {
        scope: Scope,
    },
    ResearchTask(u64),
    /// A task click that opens its completed result when one exists.
    ResearchTaskResult(u64),
    ResearchResult {
        task: u64,
        run: Option<u64>,
    },
    SettingsPage,
    AutomationList,
    AutomationResults,
    AutomationDetail(u64),
    AutomationSettings(u64),
    AutomationPeriod {
        automation: u64,
        occurrence: u64,
    },
    Recent {
        task: u64,
        surface: RecentSurface,
    },
}

/// How a navigation request came to exist. Only drill-down requests (including
/// a global-search selection and "Generate plan") record an origin; structural
/// requests start the destination section fresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationKind {
    Structural,
    DrillDown,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NavigationIntent {
    /// Sidebar item, breadcrumb link, footer command, or a result's explicit
    /// "Open project". Clears the pending origin stack.
    Structural(NavigationTarget),
    /// List/board row, report entry, period step, result "Open task", and other
    /// in-context drill-downs. Records the caller state.
    DrillDown {
        target: NavigationTarget,
        origin: Origin,
    },
    /// Choosing a global-search result. T10 records the caller page so Back can
    /// restore its filters and scroll (target contract 7).
    Search {
        target: NavigationTarget,
        origin: Origin,
    },
    /// Generating a plan opens the planning progress surface; Back returns to
    /// the page that launched the request.
    GeneratePlan { origin: Origin },
}

impl NavigationIntent {
    pub(crate) fn structural(target: NavigationTarget) -> Self {
        Self::Structural(target)
    }

    pub(crate) fn drill_down(target: NavigationTarget, origin: Origin) -> Self {
        Self::DrillDown { target, origin }
    }

    pub(crate) fn search(target: NavigationTarget, origin: Origin) -> Self {
        Self::Search { target, origin }
    }

    pub(crate) fn generate_plan(origin: Origin) -> Self {
        Self::GeneratePlan { origin }
    }

    pub(crate) fn kind(&self) -> NavigationKind {
        match self {
            Self::Structural(_) => NavigationKind::Structural,
            Self::DrillDown { .. } | Self::Search { .. } | Self::GeneratePlan { .. } => {
                NavigationKind::DrillDown
            }
        }
    }
}

/// Modal surfaces that Close/Escape dismiss without leaving the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DismissibleSurface {
    TaskEditor,
    PlanningComposer,
    ProjectRename,
    TopicEditor,
    TopicMove,
    FolderDialog,
    NewProject,
    TaskResult,
    AutomationSettings,
    AutomationPeriod,
    /// T23 owns this dialog's exact opener restoration.
    PlanningAttempt,
    /// T08 owns the overlay and its focus capture.
    GlobalSearch,
}

/// Result of asking the stack for a Back destination.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BackResolution {
    /// `None` means "use the structural breadcrumb parent".
    pub route: Option<Origin>,
    /// One neutral sentence when an owner disappeared; callers show it once.
    pub notice: Option<String>,
}

impl BackResolution {
    fn structural() -> Self {
        Self {
            route: None,
            notice: None,
        }
    }

    fn route(origin: Origin) -> Self {
        Self {
            route: Some(origin),
            notice: None,
        }
    }

    fn fallback(origin: Origin, notice: &str) -> Self {
        Self {
            route: Some(origin),
            notice: Some(notice.to_string()),
        }
    }
}

/// Bounded, deduplicated origin history. A structural navigation clears it; a
/// drill-down pushes onto it; Back pops and validates exactly one entry.
#[derive(Clone, Debug, Default)]
pub(crate) struct OriginStack {
    entries: Vec<Origin>,
}

impl OriginStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn peek(&self) -> Option<&Origin> {
        self.entries.last()
    }

    pub(crate) fn record(&mut self, origin: Origin) {
        if self.entries.last() == Some(&origin) {
            return;
        }
        self.entries.push(origin);
        if self.entries.len() > MAX_ORIGIN_DEPTH {
            let excess = self.entries.len() - MAX_ORIGIN_DEPTH;
            self.entries.drain(0..excess);
        }
    }

    /// Apply the state change that belongs to a navigation intent. Callers must
    /// invoke this only from user handlers, never from completion callbacks.
    pub(crate) fn apply_intent(&mut self, intent: &NavigationIntent) {
        match intent {
            NavigationIntent::Structural(_) => self.clear(),
            NavigationIntent::DrillDown { origin, .. }
            | NavigationIntent::Search { origin, .. }
            | NavigationIntent::GeneratePlan { origin } => self.record(origin.clone()),
        }
    }

    /// Pop one origin and validate it against current durable state. An empty
    /// stack returns the structural-fallback signal.
    pub(crate) fn take_back(&mut self, store: &Store) -> BackResolution {
        match self.entries.pop() {
            Some(origin) => validate_origin(origin, store),
            None => BackResolution::structural(),
        }
    }
}

fn project_exists(store: &Store, id: u64) -> bool {
    store.projects.iter().any(|project| project.id == id)
}

fn topic_exists(store: &Store, id: u64) -> bool {
    store.topics.iter().any(|topic| topic.id == id)
}

fn research_all(notice: &str) -> BackResolution {
    BackResolution::fallback(
        Origin::ResearchList {
            scope: Scope::All,
            query: String::new(),
            all_status: AllStatus::All,
            layout: Layout::List,
            scroll: ResearchScroll::default(),
        },
        notice,
    )
}

fn normalized_results_filter(store: &Store, filter: Option<u64>) -> Option<u64> {
    filter.filter(|id| store.automation(*id).is_some())
}

/// Validate one origin against the current store and normalize values that can
/// be repaired without guessing (for example a removed results filter). When an
/// owner is gone, return the nearest still-valid parent with a short notice so
/// the caller never lands on a blank page.
pub(crate) fn validate_origin(origin: Origin, store: &Store) -> BackResolution {
    match origin {
        Origin::ResearchList {
            scope,
            query,
            all_status,
            layout,
            scroll,
        } => match &scope {
            Scope::Project(id) if !project_exists(store, *id) => {
                research_all("That project is no longer available. Returned to All research.")
            }
            Scope::Topic(id) if !topic_exists(store, *id) => {
                research_all("That topic is no longer available. Returned to All research.")
            }
            Scope::Settings => BackResolution::route(Origin::SettingsPage {
                tab: 0,
                scroll: scroll.outer,
            }),
            Scope::Automations => BackResolution::route(Origin::AutomationList {
                scroll: scroll.outer,
            }),
            Scope::AutomationRuns => BackResolution::route(Origin::AutomationResults {
                filter: None,
                history_limit: DEFAULT_RESULTS_LIMIT,
                scroll: scroll.outer,
            }),
            _ => BackResolution::route(Origin::ResearchList {
                scope,
                query,
                all_status,
                layout,
                scroll,
            }),
        },
        Origin::ResearchTopic {
            topic,
            layout,
            scroll,
        } => {
            if topic_exists(store, topic) {
                BackResolution::route(Origin::ResearchTopic {
                    topic,
                    layout,
                    scroll,
                })
            } else {
                research_all("That topic is no longer available. Returned to All research.")
            }
        }
        Origin::ResearchTask {
            task,
            steps_tab,
            steps_layout,
            scroll,
        } => {
            if store.task(task).is_some() {
                BackResolution::route(Origin::ResearchTask {
                    task,
                    steps_tab,
                    steps_layout,
                    scroll,
                })
            } else {
                research_all("That task is no longer available. Returned to All research.")
            }
        }
        Origin::ResearchResult {
            task,
            run,
            project,
            topic,
            scroll,
        } => {
            // Research results is a valid page with no selected report. Keep the
            // route even when the caller opened it empty or while a result was
            // still loading; only clear owners that disappeared, and explain it.
            let mut cleared = false;
            let project = match project {
                Some(id) if !project_exists(store, id) => {
                    cleared = true;
                    None
                }
                other => other,
            };
            let topic = match topic {
                Some(id)
                    if !store.topics.iter().any(|value| {
                        value.id == id && project.is_none_or(|project| value.project_id == project)
                    }) =>
                {
                    cleared = true;
                    None
                }
                other => other,
            };
            let task = match task {
                Some(id) if store.task(id).is_none() => {
                    cleared = true;
                    None
                }
                other => other,
            };
            let selected_run_deleted = run.is_some_and(|id| {
                !store.runs().iter().any(|candidate| {
                    candidate.id == id && task.is_none_or(|task| candidate.task_id == task)
                })
            });
            let run = if selected_run_deleted { None } else { run };
            let notice = if selected_run_deleted {
                Some("That result is no longer available. Returned to Research results.".to_string())
            } else if cleared {
                Some(
                    "Part of this results view is no longer available. Filters were cleared."
                        .to_string(),
                )
            } else {
                None
            };
            BackResolution {
                route: Some(Origin::ResearchResult {
                    task,
                    run,
                    project,
                    topic,
                    scroll,
                }),
                notice,
            }
        }
        Origin::SettingsPage { tab, scroll } => {
            BackResolution::route(Origin::SettingsPage { tab, scroll })
        }
        Origin::AutomationList { scroll } => {
            BackResolution::route(Origin::AutomationList { scroll })
        }
        Origin::AutomationResults {
            filter,
            history_limit,
            scroll,
        } => BackResolution::route(Origin::AutomationResults {
            filter: normalized_results_filter(store, filter),
            history_limit: history_limit.max(DEFAULT_RESULTS_LIMIT),
            scroll,
        }),
        Origin::AutomationDetail { automation, scroll } => {
            if store.automation(automation).is_some() {
                BackResolution::route(Origin::AutomationDetail { automation, scroll })
            } else {
                BackResolution::fallback(
                    Origin::AutomationList { scroll },
                    "That automation is no longer available. Returned to All automations.",
                )
            }
        }
        Origin::AutomationSettings {
            automation,
            settings_scroll,
        } => {
            if store.automation(automation).is_some() {
                BackResolution::route(Origin::AutomationSettings {
                    automation,
                    settings_scroll,
                })
            } else {
                BackResolution::fallback(
                    Origin::AutomationList {
                        scroll: ScrollState::default(),
                    },
                    "That automation is no longer available. Returned to All automations.",
                )
            }
        }
        Origin::AutomationPeriod {
            automation,
            occurrence,
            under,
            dialog_scroll,
        } => {
            let occurrence_valid = store.occurrence(occurrence).is_some_and(|value| {
                value.automation_id == automation
            });
            if occurrence_valid && store.automation(automation).is_some() {
                let under = match under {
                    PeriodUnder::Detail { scroll } => PeriodUnder::Detail { scroll },
                    PeriodUnder::Results {
                        filter,
                        history_limit,
                        scroll,
                    } => PeriodUnder::Results {
                        filter: normalized_results_filter(store, filter),
                        history_limit: history_limit.max(DEFAULT_RESULTS_LIMIT),
                        scroll,
                    },
                };
                return BackResolution::route(Origin::AutomationPeriod {
                    automation,
                    occurrence,
                    under,
                    dialog_scroll,
                });
            }
            if store.automation(automation).is_some() {
                BackResolution::fallback(
                    Origin::AutomationDetail {
                        automation,
                        scroll: ScrollState::default(),
                    },
                    "That period is no longer available. Returned to the automation.",
                )
            } else {
                BackResolution::fallback(
                    Origin::AutomationList {
                        scroll: ScrollState::default(),
                    },
                    "That automation is no longer available. Returned to All automations.",
                )
            }
        }
    }
}

/// The research-creation flows the UI can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreationIntent {
    /// Footer/⌘N/Market-research "New research". Always starts a new topic.
    Global,
    /// A project toolbar "New research".
    Project { project: u64 },
    /// A topic's "New research" (appends a goal through a new plan).
    TopicNewGoal { topic: u64 },
    /// A topic's "Replan" (creates a separate replanned topic).
    TopicReplan { topic: u64 },
}

/// The visible, validated destination of a creation form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreationMode {
    NewTopic { project: u64 },
    ExistingTopic { project: u64, topic: u64 },
    Replan { project: u64, source_topic: u64 },
}

impl CreationMode {
    pub(crate) fn project(self) -> u64 {
        match self {
            Self::NewTopic { project }
            | Self::ExistingTopic { project, .. }
            | Self::Replan { project, .. } => project,
        }
    }

    pub(crate) fn topic(self) -> Option<u64> {
        match self {
            Self::NewTopic { .. } => None,
            Self::ExistingTopic { topic, .. } => Some(topic),
            Self::Replan { source_topic, .. } => Some(source_topic),
        }
    }

    /// Replanning is bound to the original project; every other mode can change
    /// its destination before the user saves or generates.
    pub(crate) fn can_change_project(self) -> bool {
        !matches!(self, Self::Replan { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreationDestinationError {
    NoProjects,
    ProjectMissing(u64),
    TopicMissing(u64),
    TopicProjectMissing(u64),
    ReplanProjectFixed,
}

impl CreationDestinationError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::NoProjects => {
                "Create a project before starting research. The request text is kept.".into()
            }
            Self::ProjectMissing(id) => {
                format!("Project {id} is no longer available. Choose another destination.")
            }
            Self::TopicMissing(id) => {
                format!("Topic {id} is no longer available. Choose another destination.")
            }
            Self::TopicProjectMissing(id) => format!(
                "The project for topic {id} is no longer available. Choose another destination."
            ),
            Self::ReplanProjectFixed => {
                "Replan keeps the original project. Change the source topic instead.".into()
            }
        }
    }
}

fn first_visible_project(store: &Store) -> Option<u64> {
    store.visible_projects().map(|project| project.id).next()
}

/// Resolve the creation destination with an explicit, visible precedence:
/// the selected task's project, the current project scope, the current topic's
/// project, the persisted last active project, then the first visible project.
/// No step is allowed to silently substitute a different project.
pub(crate) fn resolve_creation_destination(
    store: &Store,
    intent: CreationIntent,
    scope: &Scope,
    context_task: Option<u64>,
) -> Result<CreationMode, CreationDestinationError> {
    match intent {
        CreationIntent::Project { project } => {
            if project_exists(store, project) {
                Ok(CreationMode::NewTopic { project })
            } else {
                Err(CreationDestinationError::ProjectMissing(project))
            }
        }
        CreationIntent::TopicNewGoal { topic } => {
            let Some(value) = store.topics.iter().find(|value| value.id == topic) else {
                return Err(CreationDestinationError::TopicMissing(topic));
            };
            if !project_exists(store, value.project_id) {
                return Err(CreationDestinationError::TopicProjectMissing(topic));
            }
            Ok(CreationMode::ExistingTopic {
                project: value.project_id,
                topic,
            })
        }
        CreationIntent::TopicReplan { topic } => {
            let Some(value) = store.topics.iter().find(|value| value.id == topic) else {
                return Err(CreationDestinationError::TopicMissing(topic));
            };
            if !project_exists(store, value.project_id) {
                return Err(CreationDestinationError::TopicProjectMissing(topic));
            }
            Ok(CreationMode::Replan {
                project: value.project_id,
                source_topic: topic,
            })
        }
        CreationIntent::Global => {
            if let Some(task) = context_task
                && let Some(value) = store.task(task)
            {
                if !project_exists(store, value.project_id) {
                    return Err(CreationDestinationError::ProjectMissing(value.project_id));
                }
                return Ok(CreationMode::NewTopic {
                    project: value.project_id,
                });
            }
            match scope {
                Scope::Project(project) => {
                    if project_exists(store, *project) {
                        return Ok(CreationMode::NewTopic { project: *project });
                    }
                    return Err(CreationDestinationError::ProjectMissing(*project));
                }
                Scope::Topic(topic) => {
                    let Some(value) = store.topics.iter().find(|value| value.id == *topic) else {
                        return Err(CreationDestinationError::TopicMissing(*topic));
                    };
                    if !project_exists(store, value.project_id) {
                        return Err(CreationDestinationError::TopicProjectMissing(*topic));
                    }
                    // Global New research always starts a new topic even when it
                    // was invoked from a topic.
                    return Ok(CreationMode::NewTopic {
                        project: value.project_id,
                    });
                }
                _ => {}
            }
            if let Some(project) = store
                .last_active_project
                .filter(|project| project_exists(store, *project))
            {
                return Ok(CreationMode::NewTopic { project });
            }
            match first_visible_project(store) {
                Some(project) => Ok(CreationMode::NewTopic { project }),
                None => Err(CreationDestinationError::NoProjects),
            }
        }
    }
}

/// Change the destination before planning. A replan keeps its original project.
/// Existing-topic mode falls back to a new topic in the chosen project so a
/// topic can never be attached to a foreign project.
pub(crate) fn retarget_creation(
    store: &Store,
    mode: CreationMode,
    project: u64,
) -> Result<CreationMode, CreationDestinationError> {
    if !mode.can_change_project() {
        return Err(CreationDestinationError::ReplanProjectFixed);
    }
    if !project_exists(store, project) {
        return Err(CreationDestinationError::ProjectMissing(project));
    }
    match mode {
        CreationMode::ExistingTopic { .. } => Ok(CreationMode::NewTopic { project }),
        _ => Ok(CreationMode::NewTopic { project }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Project, ProjectKind};

    fn store() -> Store {
        let mut store = Store::empty();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.default_model = "test/model".into();
        store.research_config.models =
            ["test/plan", "test/collect", "test/analyze", "test/advise"].map(String::from);
        store
    }

    fn add_project(store: &mut Store, id: u64, name: &str) {
        store.projects.push(Project {
            id,
            name: name.into(),
            key: format!("P{id}"),
            directory: std::env::temp_dir().to_string_lossy().into_owned(),
            research_brief: String::new(),
            kind: ProjectKind::User,
        });
    }

    fn template() -> crate::automation::AutomationTemplate {
        crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()
    }

    fn automation(store: &mut Store, name: &str) -> u64 {
        store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                name.into(),
                "Track public filings".into(),
                "A cited note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily {
                        hour: 8,
                        minute: 30,
                    },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                Some(template()),
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

    fn list_origin(scope: Scope) -> Origin {
        Origin::ResearchList {
            scope,
            query: "analysis".into(),
            all_status: AllStatus::All,
            layout: Layout::List,
            scroll: ResearchScroll {
                outer: ScrollState { x: 4.0, y: 120.0 },
                columns: vec![BoardAnchor {
                    stage: Stage::Backlog,
                    y: 36.0,
                }],
            },
        }
    }

    #[test]
    fn structural_intents_clear_and_drilldowns_record_origins() {
        let mut stack = OriginStack::new();
        let target = NavigationTarget::ResearchTask(1);
        stack.apply_intent(&NavigationIntent::drill_down(
            target.clone(),
            list_origin(Scope::All),
        ));
        assert_eq!(stack.len(), 1);
        stack.apply_intent(&NavigationIntent::structural(NavigationTarget::Research {
            scope: Scope::All,
        }));
        assert!(stack.is_empty());

        stack.apply_intent(&NavigationIntent::search(
            target,
            list_origin(Scope::Project(1)),
        ));
        assert_eq!(stack.len(), 1, "search selections record the caller origin");
        assert_eq!(
            NavigationIntent::search(
                NavigationTarget::AutomationDetail(1),
                list_origin(Scope::All)
            )
            .kind(),
            NavigationKind::DrillDown
        );
        assert_eq!(
            NavigationIntent::structural(NavigationTarget::Research {
                scope: Scope::All
            })
            .kind(),
            NavigationKind::Structural
        );
    }

    #[test]
    fn origin_stack_is_bounded_and_deduplicated() {
        let mut stack = OriginStack::new();
        let first = list_origin(Scope::All);
        stack.record(first.clone());
        stack.record(first.clone());
        assert_eq!(stack.len(), 1);
        for index in 0..(MAX_ORIGIN_DEPTH + 3) {
            let mut origin = list_origin(Scope::Project(1));
            if let Origin::ResearchList { query, .. } = &mut origin {
                *query = format!("q{index}");
            }
            stack.record(origin);
        }
        assert_eq!(stack.len(), MAX_ORIGIN_DEPTH);
    }

    #[test]
    fn empty_stack_signals_structural_fallback() {
        let store = store();
        let resolution = OriginStack::new().take_back(&store);
        assert_eq!(resolution.route, None);
        assert_eq!(resolution.notice, None);
    }

    #[test]
    fn missing_project_or_topic_falls_back_to_all_research() {
        let store = store();
        let resolution = validate_origin(list_origin(Scope::Project(99)), &store);
        assert!(matches!(
            resolution.route,
            Some(Origin::ResearchList {
                scope: Scope::All,
                ..
            })
        ));
        assert!(resolution.notice.is_some());

        let resolution = validate_origin(
            Origin::ResearchTopic {
                topic: 99,
                layout: Layout::Board,
                scroll: ScrollState::default(),
            },
            &store,
        );
        assert!(matches!(
            resolution.route,
            Some(Origin::ResearchList {
                scope: Scope::All,
                ..
            })
        ));
        assert!(resolution.notice.is_some());
    }

    #[test]
    fn valid_research_origin_keeps_filters_and_scroll() {
        let store = store();
        let origin = list_origin(Scope::All);
        let resolution = validate_origin(origin.clone(), &store);
        assert_eq!(resolution.route, Some(origin));
        assert_eq!(resolution.notice, None);
    }

    #[test]
    fn deleted_task_falls_back_to_all_research() {
        let mut store = store();
        let task = store.create_task("Task", 1, Stage::Planned, None).unwrap();
        store.tasks.retain(|value| value.id != task);
        let resolution = validate_origin(
            Origin::ResearchTask {
                task,
                steps_tab: None,
                steps_layout: Layout::List,
                scroll: ScrollState::default(),
            },
            &store,
        );
        assert!(matches!(
            resolution.route,
            Some(Origin::ResearchList {
                scope: Scope::All,
                ..
            })
        ));
        assert!(resolution.notice.is_some());
    }

    #[test]
    fn empty_or_waiting_results_origin_is_preserved() {
        let mut store = store();
        let task = store.create_task("Task", 1, Stage::Planned, None).unwrap();
        for (task, run) in [(None, None), (Some(task), None)] {
            let resolution = validate_origin(
                Origin::ResearchResult {
                    task,
                    run,
                    project: None,
                    topic: None,
                    scroll: ScrollState { x: 0.0, y: 90.0 },
                },
                &store,
            );
            match resolution.route {
                Some(Origin::ResearchResult {
                    task: restored_task,
                    run: restored_run,
                    scroll,
                    ..
                }) => {
                    assert_eq!(restored_task, task);
                    assert_eq!(restored_run, run);
                    assert_eq!(scroll.y, 90.0);
                }
                other => panic!("empty results must stay results, got {other:?}"),
            }
            assert!(resolution.notice.is_none());
        }
    }

    #[test]
    fn results_project_topic_and_report_selection_are_preserved() {
        let mut store = store();
        store.policies = Store::mock().policies;
        let topic = store.create_topic(1, "Topic").unwrap();
        let task = store.create_task("Task", 1, Stage::Planned, None).unwrap();
        store.simulate_run(task).unwrap();
        let run = store.runs().last().unwrap().id;

        let origin = Origin::ResearchResult {
            task: Some(task),
            run: Some(run),
            project: Some(1),
            topic: Some(topic),
            scroll: ScrollState { x: 0.0, y: 120.0 },
        };
        let resolution = validate_origin(origin.clone(), &store);
        assert_eq!(resolution.route, Some(origin));
        assert_eq!(resolution.notice, None);
    }

    #[test]
    fn deleted_selected_report_stays_on_results_with_a_notice() {
        let mut store = store();
        store.policies = Store::mock().policies;
        let task = store.create_task("Task", 1, Stage::Planned, None).unwrap();
        store.simulate_run(task).unwrap();
        let run = store.runs().last().unwrap().id;
        store.retain_runs(|value| value.id != run);

        let resolution = validate_origin(
            Origin::ResearchResult {
                task: Some(task),
                run: Some(run),
                project: Some(1),
                topic: None,
                scroll: ScrollState::default(),
            },
            &store,
        );
        match resolution.route {
            Some(Origin::ResearchResult {
                task: Some(restored_task),
                run: None,
                project: Some(1),
                ..
            }) => assert_eq!(restored_task, task),
            other => panic!("a deleted report must not open the task detail, got {other:?}"),
        }
        assert!(
            resolution
                .notice
                .as_deref()
                .is_some_and(|notice| notice.contains("no longer available")),
            "the actual fallback must be explained"
        );
    }

    #[test]
    fn deleted_results_filters_are_cleared_but_the_page_survives() {
        let base_store = store();
        let resolution = validate_origin(
            Origin::ResearchResult {
                task: Some(404),
                run: None,
                project: Some(404),
                topic: Some(404),
                scroll: ScrollState::default(),
            },
            &base_store,
        );
        match resolution.route {
            Some(Origin::ResearchResult {
                task,
                run,
                project,
                topic,
                ..
            }) => {
                assert_eq!(task, None);
                assert_eq!(run, None);
                assert_eq!(project, None);
                assert_eq!(topic, None);
            }
            other => panic!("results route must survive deleted filters, got {other:?}"),
        }
        assert!(resolution.notice.is_some());

        // A topic that belongs to another project cannot stay selected.
        let mut other_store = store();
        let topic = other_store.create_topic(1, "Other").unwrap();
        other_store.projects.push(Project {
            id: 2,
            name: "Second".into(),
            key: "P2".into(),
            directory: std::env::temp_dir().to_string_lossy().into_owned(),
            research_brief: String::new(),
            kind: ProjectKind::User,
        });
        let resolution = validate_origin(
            Origin::ResearchResult {
                task: None,
                run: None,
                project: Some(2),
                topic: Some(topic),
                scroll: ScrollState::default(),
            },
            &other_store,
        );
        match resolution.route {
            Some(Origin::ResearchResult {
                project, topic, ..
            }) => {
                assert_eq!(project, Some(2));
                assert_eq!(topic, None);
            }
            other => panic!("expected a results origin, got {other:?}"),
        }
        assert!(resolution.notice.is_some());
    }

    #[test]
    fn search_from_an_empty_results_page_returns_to_empty_results() {
        let store = store();
        let mut stack = OriginStack::new();
        stack.apply_intent(&NavigationIntent::search(
            NavigationTarget::ResearchTask(1),
            Origin::ResearchResult {
                task: None,
                run: None,
                project: None,
                topic: None,
                scroll: ScrollState { x: 0.0, y: 44.0 },
            },
        ));
        let resolution = stack.take_back(&store);
        assert!(matches!(
            resolution.route,
            Some(Origin::ResearchResult {
                task: None,
                run: None,
                ..
            })
        ));
        assert_eq!(resolution.notice, None);
    }

    #[test]
    fn removed_period_falls_back_to_detail_then_to_the_list() {
        let mut store = store();
        let automation = automation(&mut store, "Period owner");
        let occurrence = period(&mut store, automation, 1_000);
        let origin = Origin::AutomationPeriod {
            automation,
            occurrence,
            under: PeriodUnder::Results {
                filter: Some(automation),
                history_limit: 20,
                scroll: ScrollState::default(),
            },
            dialog_scroll: ScrollState { x: 0.0, y: 40.0 },
        };
        store.retain_occurrences(|value| value.id != occurrence);
        let resolution = validate_origin(origin, &store);
        assert!(matches!(
            resolution.route,
            Some(Origin::AutomationDetail {
                automation: restored,
                ..
            }) if restored == automation
        ));
        assert!(resolution.notice.is_some());

        store.retain_automations(|value| value.id != automation);
        let resolution = validate_origin(
            Origin::AutomationPeriod {
                automation,
                occurrence,
                under: PeriodUnder::Detail {
                    scroll: ScrollState::default(),
                },
                dialog_scroll: ScrollState::default(),
            },
            &store,
        );
        assert!(matches!(
            resolution.route,
            Some(Origin::AutomationList { .. })
        ));
        assert!(resolution.notice.is_some());
    }

    #[test]
    fn removed_results_filter_is_normalized_but_the_page_survives() {
        let mut store = store();
        let automation = automation(&mut store, "Filter owner");
        let resolution = validate_origin(
            Origin::AutomationResults {
                filter: Some(9_999),
                history_limit: 5,
                scroll: ScrollState { x: 0.0, y: 80.0 },
            },
            &store,
        );
        match resolution.route {
            Some(Origin::AutomationResults {
                filter,
                history_limit,
                scroll,
            }) => {
                assert_eq!(filter, None);
                assert_eq!(history_limit, DEFAULT_RESULTS_LIMIT);
                assert_eq!(scroll.y, 80.0);
            }
            other => panic!("expected an automation results origin, got {other:?}"),
        }
        assert!(store.automation(automation).is_some());
    }

    #[test]
    fn missing_automation_detail_falls_back_to_the_list() {
        let store = store();
        let resolution = validate_origin(
            Origin::AutomationDetail {
                automation: 404,
                scroll: ScrollState { x: 0.0, y: 22.0 },
            },
            &store,
        );
        assert!(matches!(
            resolution.route,
            Some(Origin::AutomationList { .. })
        ));
        assert!(resolution.notice.is_some());
    }

    #[test]
    fn destination_prefers_the_context_task_then_the_scope() {
        let mut store = store();
        add_project(&mut store, 2, "Second");
        let task = store.create_task("Task", 2, Stage::Planned, None).unwrap();
        let destination = resolve_creation_destination(
            &store,
            CreationIntent::Global,
            &Scope::Project(1),
            Some(task),
        )
        .unwrap();
        assert_eq!(destination, CreationMode::NewTopic { project: 2 });

        let destination = resolve_creation_destination(
            &store,
            CreationIntent::Global,
            &Scope::Project(2),
            None,
        )
        .unwrap();
        assert_eq!(destination, CreationMode::NewTopic { project: 2 });
    }

    #[test]
    fn global_destination_uses_last_active_then_first_visible() {
        let mut store = store();
        add_project(&mut store, 2, "Second");
        store.last_active_project = Some(2);
        let destination =
            resolve_creation_destination(&store, CreationIntent::Global, &Scope::All, None).unwrap();
        assert_eq!(destination, CreationMode::NewTopic { project: 2 });

        store.last_active_project = Some(9_999);
        let destination =
            resolve_creation_destination(&store, CreationIntent::Global, &Scope::All, None).unwrap();
        assert_eq!(destination, CreationMode::NewTopic { project: 1 });
    }

    #[test]
    fn global_destination_explains_an_invalid_explicit_scope() {
        let store = store();
        let error =
            resolve_creation_destination(&store, CreationIntent::Global, &Scope::Project(9), None)
                .unwrap_err();
        assert_eq!(error, CreationDestinationError::ProjectMissing(9));
        assert!(error.message().contains("9"));

        let mut empty = Store::empty();
        empty.projects.clear();
        let error =
            resolve_creation_destination(&empty, CreationIntent::Global, &Scope::All, None)
                .unwrap_err();
        assert_eq!(error, CreationDestinationError::NoProjects);
        assert!(!error.message().is_empty());
    }

    #[test]
    fn topic_destinations_are_validated_and_replan_is_fixed() {
        let mut store = store();
        let topic = store.create_topic(1, "Topic").unwrap();
        assert_eq!(
            resolve_creation_destination(
                &store,
                CreationIntent::TopicNewGoal { topic },
                &Scope::Topic(topic),
                None
            )
            .unwrap(),
            CreationMode::ExistingTopic {
                project: 1,
                topic
            }
        );
        let replan = resolve_creation_destination(
            &store,
            CreationIntent::TopicReplan { topic },
            &Scope::Topic(topic),
            None,
        )
        .unwrap();
        assert_eq!(
            replan,
            CreationMode::Replan {
                project: 1,
                source_topic: topic
            }
        );
        assert!(!replan.can_change_project());
        assert_eq!(
            retarget_creation(&store, replan, 2),
            Err(CreationDestinationError::ReplanProjectFixed)
        );

        let missing = resolve_creation_destination(
            &store,
            CreationIntent::TopicNewGoal { topic: 77 },
            &Scope::All,
            None,
        )
        .unwrap_err();
        assert_eq!(missing, CreationDestinationError::TopicMissing(77));
    }

    #[test]
    fn retargeting_an_existing_topic_starts_a_new_topic_in_the_new_project() {
        let mut store = store();
        add_project(&mut store, 2, "Second");
        let mode = CreationMode::ExistingTopic {
            project: 1,
            topic: 4,
        };
        assert_eq!(
            retarget_creation(&store, mode, 2).unwrap(),
            CreationMode::NewTopic { project: 2 }
        );
    }

    #[test]
    fn recent_entries_carry_their_surface_kind() {
        assert_ne!(RecentSurface::Task, RecentSurface::Result);
        let result = NavigationTarget::Recent {
            task: 3,
            surface: RecentSurface::Result,
        };
        let task = NavigationTarget::Recent {
            task: 3,
            surface: RecentSurface::Task,
        };
        assert_ne!(result, task);
    }

    #[test]
    fn generate_plan_is_a_drill_down_and_records_the_caller() {
        let mut stack = OriginStack::new();
        stack.apply_intent(&NavigationIntent::generate_plan(list_origin(Scope::All)));
        assert_eq!(stack.len(), 1);
        assert_eq!(
            NavigationIntent::generate_plan(list_origin(Scope::All)).kind(),
            NavigationKind::DrillDown
        );
    }

    #[test]
    fn archived_task_origins_remain_restorable() {
        let mut store = store();
        let task = store.create_task("Archived", 1, Stage::Done, None).unwrap();
        store.task_mut(task).unwrap().archived = true;
        let resolution = validate_origin(
            Origin::ResearchTask {
                task,
                steps_tab: None,
                steps_layout: Layout::List,
                scroll: ScrollState::default(),
            },
            &store,
        );
        assert!(matches!(
            resolution.route,
            Some(Origin::ResearchTask {
                task: restored,
                ..
            }) if restored == task
        ));
        assert_eq!(resolution.notice, None);
    }
}
