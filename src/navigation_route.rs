//! T12 S1: pure typed navigation core.
//!
//! This module is deliberately free of `TaskApp`, `Window`, `FocusHandle`,
//! `InputState`, `Storage` and runtime state. It owns route identity, the
//! legal overlay stack, bounded return frames and owner validation. The app
//! adapter will capture scroll/focus, bind editor payloads and call these
//! mutators only from explicit user commands.
//!
//! The type system removes mutually exclusive destinations. Referential
//! validity (a task/automation/occurrence/run still exists) is validated
//! atomically before any mutation, so a rejected command leaves the state
//! byte-for-byte unchanged.

// S1 is intentionally pure and not wired into `TaskApp` yet. Remove this
// allowance in S2 when the adapter starts consuming the API.
#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::app::{AllStatus, Layout, Scope};
use crate::domain::Store;

pub(crate) type ProjectId = u64;
pub(crate) type TopicId = u64;
pub(crate) type TaskId = u64;
pub(crate) type RunId = u64;
pub(crate) type AutomationId = u64;
pub(crate) type OccurrenceId = u64;
pub(crate) type RequestId = u64;

/// Hard upper bound. The grammar below only admits shorter stacks; the bound
/// is a safety net, not the authorization rule.
pub(crate) const MAX_OVERLAY_DEPTH: usize = 6;
pub(crate) const MAX_RETURN_DEPTH: usize = 8;

/// The data scope a route projects into for sidebar/collection rendering.
/// `ResearchList` never carries a `Runs` alias: ordinary results are
/// `ResearchResult`, and Market research/Paused are their own routes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DataScope {
    All,
    Project(ProjectId),
    Topic(TopicId),
    Cycle(String),
    Archive,
    Results,
    Paused,
    MarketResearch,
    Automations,
    AutomationResults,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResearchScope {
    All,
    Project(ProjectId),
    Topic(TopicId),
    Cycle(String),
    Archive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskPanel {
    Overview,
    Steps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Models,
    Publishing,
}

/// What owns an ordinary/task result page. Automation planning attempts are
/// never admitted through `OrdinaryPage`; they stay T23-owned history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultOwner {
    /// Global Research results. Only ordinary, non-automation results.
    OrdinaryPage,
    /// An ordinary research task's report. The task must not be automation-owned.
    Task(TaskId),
    /// An automation step's report. All three identities are explicit.
    AutomationTask {
        automation: AutomationId,
        occurrence: OccurrenceId,
        task: TaskId,
    },
    /// A pending/completed ordinary research-request plan. Automation requests
    /// are excluded; T23 owns automation planning-attempt history.
    PlanningProgress { request: RequestId },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResearchListPage {
    scope: ResearchScope,
    all_status: AllStatus,
    layout: Layout,
}

impl ResearchListPage {
    pub(crate) fn new(scope: ResearchScope, all_status: AllStatus, layout: Layout) -> Self {
        Self {
            scope,
            all_status,
            layout,
        }
    }
    pub(crate) fn scope(&self) -> &ResearchScope {
        &self.scope
    }
    pub(crate) fn all_status(&self) -> AllStatus {
        self.all_status
    }
    pub(crate) fn layout(&self) -> Layout {
        self.layout
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResearchTaskPage {
    task: TaskId,
    panel: TaskPanel,
    steps_layout: Layout,
}

impl ResearchTaskPage {
    pub(crate) fn new(task: TaskId, panel: TaskPanel, steps_layout: Layout) -> Self {
        Self {
            task,
            panel,
            steps_layout,
        }
    }
    pub(crate) fn task(&self) -> TaskId {
        self.task
    }
    pub(crate) fn panel(&self) -> TaskPanel {
        self.panel
    }
    pub(crate) fn steps_layout(&self) -> Layout {
        self.steps_layout
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResultSelection {
    run: Option<RunId>,
}

impl ResultSelection {
    pub(crate) fn empty() -> Self {
        Self { run: None }
    }
    pub(crate) fn run(&self) -> Option<RunId> {
        self.run
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResearchResultPage {
    owner: ResultOwner,
    selection: ResultSelection,
}

impl ResearchResultPage {
    pub(crate) fn new(owner: ResultOwner, selection: ResultSelection) -> Self {
        Self { owner, selection }
    }
    pub(crate) fn owner(&self) -> ResultOwner {
        self.owner
    }
    pub(crate) fn selection(&self) -> ResultSelection {
        self.selection
    }
    pub(crate) fn run(&self) -> Option<RunId> {
        self.selection.run
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsPage {
    tab: SettingsSection,
}

impl SettingsPage {
    pub(crate) fn new(tab: SettingsSection) -> Self {
        Self { tab }
    }
    pub(crate) fn tab(&self) -> SettingsSection {
        self.tab
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AutomationResultsPage {
    filter: Option<AutomationId>,
    history_limit: usize,
}

impl AutomationResultsPage {
    pub(crate) fn new(filter: Option<AutomationId>, history_limit: usize) -> Self {
        Self {
            filter,
            history_limit,
        }
    }
    pub(crate) fn filter(&self) -> Option<AutomationId> {
        self.filter
    }
    pub(crate) fn history_limit(&self) -> usize {
        self.history_limit
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AutomationDetailPage {
    automation: AutomationId,
}

impl AutomationDetailPage {
    pub(crate) fn new(automation: AutomationId) -> Self {
        Self { automation }
    }
    pub(crate) fn automation(&self) -> AutomationId {
        self.automation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AutomationEditorPage {
    editing: Option<AutomationId>,
}

impl AutomationEditorPage {
    pub(crate) fn new(editing: Option<AutomationId>) -> Self {
        Self { editing }
    }
    pub(crate) fn editing(&self) -> Option<AutomationId> {
        self.editing
    }
}

/// The persistent page. Exactly one variant is the destination; a period is a
/// typed overlay over an explicit Detail/Results base, not a fourth page flag.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Route {
    ResearchList(ResearchListPage),
    ResearchTask(ResearchTaskPage),
    ResearchResult(ResearchResultPage),
    PausedProjects,
    MarketResearch,
    Settings(SettingsPage),
    AutomationList,
    AutomationResults(AutomationResultsPage),
    AutomationDetail(AutomationDetailPage),
    AutomationEditor(AutomationEditorPage),
}

impl Route {
    pub(crate) fn research_list(scope: ResearchScope, all_status: AllStatus, layout: Layout) -> Self {
        Self::ResearchList(ResearchListPage::new(scope, all_status, layout))
    }
    pub(crate) fn research_task(task: TaskId) -> Self {
        Self::ResearchTask(ResearchTaskPage::new(task, TaskPanel::Overview, Layout::List))
    }
    pub(crate) fn research_task_steps(task: TaskId, layout: Layout) -> Self {
        Self::ResearchTask(ResearchTaskPage::new(task, TaskPanel::Steps, layout))
    }
    pub(crate) fn research_result(owner: ResultOwner, run: Option<RunId>) -> Self {
        Self::ResearchResult(ResearchResultPage::new(
            owner,
            ResultSelection { run },
        ))
    }
    pub(crate) fn automation_detail(automation: AutomationId) -> Self {
        Self::AutomationDetail(AutomationDetailPage::new(automation))
    }
    pub(crate) fn automation_editor(editing: Option<AutomationId>) -> Self {
        Self::AutomationEditor(AutomationEditorPage::new(editing))
    }
    pub(crate) fn automation_results(filter: Option<AutomationId>, limit: usize) -> Self {
        Self::AutomationResults(AutomationResultsPage::new(filter, limit))
    }

    /// Projection used by sidebar/collection code during migration. It is
    /// derived, read-only and never stored beside the route.
    pub(crate) fn data_scope(&self, store: &Store) -> DataScope {
        match self {
            Self::ResearchList(page) => match page.scope() {
                ResearchScope::All => DataScope::All,
                ResearchScope::Project(id) => DataScope::Project(*id),
                ResearchScope::Topic(id) => DataScope::Topic(*id),
                ResearchScope::Cycle(name) => DataScope::Cycle(name.clone()),
                ResearchScope::Archive => DataScope::Archive,
            },
            Self::ResearchTask(page) => store
                .task(page.task())
                .map(|task| DataScope::Project(task.project_id))
                .unwrap_or(DataScope::All),
            Self::ResearchResult(page) => match page.owner() {
                ResultOwner::Task(task) | ResultOwner::AutomationTask { task, .. } => store
                    .task(task)
                    .map(|task| DataScope::Project(task.project_id))
                    .unwrap_or(DataScope::Results),
                ResultOwner::OrdinaryPage | ResultOwner::PlanningProgress { .. } => DataScope::Results,
            },
            Self::PausedProjects => DataScope::Paused,
            Self::MarketResearch => DataScope::MarketResearch,
            Self::Settings(_) => DataScope::Settings,
            Self::AutomationList => DataScope::Automations,
            Self::AutomationResults(_) => DataScope::AutomationResults,
            Self::AutomationDetail(_) | Self::AutomationEditor(_) => DataScope::Automations,
        }
    }

    pub(crate) fn task(&self) -> Option<TaskId> {
        match self {
            Self::ResearchTask(page) => Some(page.task()),
            Self::ResearchResult(page) => match page.owner() {
                ResultOwner::Task(task) | ResultOwner::AutomationTask { task, .. } => Some(task),
                _ => None,
            },
            _ => None,
        }
    }
}

/// A modal/editor surface. Page identity is not stored here; the overlay stack
/// is ordered bottom -> top and only the top is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorTarget {
    Task(TaskId),
    Automation { editing: Option<AutomationId> },
    PlanningRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overlay {
    Search,
    PlanningComposer,
    TaskEditor { task: TaskId },
    AutomationSettings { automation: AutomationId },
    AutomationPeriod {
        automation: AutomationId,
        occurrence: OccurrenceId,
    },
    PlanningAttempt { run: RunId, from_settings: bool },
    ResearchImpactReview { task: TaskId },
    PreviousVersions { task: TaskId, run: Option<RunId> },
    ProjectRename { project: ProjectId },
    TopicEditor { project: ProjectId, topic: Option<TopicId> },
    TopicMove { topic: TopicId },
    FolderDialog { project: ProjectId },
    NewProject,
    DraftDecision { editor: EditorTarget },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverlayKind {
    Search,
    PlanningComposer,
    TaskEditor,
    AutomationSettings,
    AutomationPeriod,
    PlanningAttempt,
    ResearchImpactReview,
    PreviousVersions,
    ProjectRename,
    TopicEditor,
    TopicMove,
    FolderDialog,
    NewProject,
    DraftDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverlayClass {
    /// A task editor or planning composer (a dirty-capable editor).
    Editor,
    /// A view/config/small editor root. Cannot coexist with another root.
    Content,
    /// T21 save impact confirmation, a child of the task editor or decision.
    Impact,
    /// The T10 save/discard/stay decision.
    Decision,
    Search,
}

impl Overlay {
    pub(crate) fn kind(&self) -> OverlayKind {
        match self {
            Self::Search => OverlayKind::Search,
            Self::PlanningComposer => OverlayKind::PlanningComposer,
            Self::TaskEditor { .. } => OverlayKind::TaskEditor,
            Self::AutomationSettings { .. } => OverlayKind::AutomationSettings,
            Self::AutomationPeriod { .. } => OverlayKind::AutomationPeriod,
            Self::PlanningAttempt { .. } => OverlayKind::PlanningAttempt,
            Self::ResearchImpactReview { .. } => OverlayKind::ResearchImpactReview,
            Self::PreviousVersions { .. } => OverlayKind::PreviousVersions,
            Self::ProjectRename { .. } => OverlayKind::ProjectRename,
            Self::TopicEditor { .. } => OverlayKind::TopicEditor,
            Self::TopicMove { .. } => OverlayKind::TopicMove,
            Self::FolderDialog { .. } => OverlayKind::FolderDialog,
            Self::NewProject => OverlayKind::NewProject,
            Self::DraftDecision { .. } => OverlayKind::DraftDecision,
        }
    }

    pub(crate) fn class(&self) -> OverlayClass {
        match self {
            Self::Search => OverlayClass::Search,
            Self::TaskEditor { .. } | Self::PlanningComposer => OverlayClass::Editor,
            Self::ResearchImpactReview { .. } => OverlayClass::Impact,
            Self::DraftDecision { .. } => OverlayClass::Decision,
            _ => OverlayClass::Content,
        }
    }

    pub(crate) fn task(&self) -> Option<TaskId> {
        match self {
            Self::TaskEditor { task } | Self::ResearchImpactReview { task } => Some(*task),
            Self::PreviousVersions { task, .. } => Some(*task),
            _ => None,
        }
    }
}

/// Where a period dialog sits. This is explicit so a period can never be
/// reconstructed later from a stale `scope`/`selected_run` combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PeriodBase {
    Detail { automation: AutomationId },
    Results {
        filter: Option<AutomationId>,
        history_limit: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskSurface {
    Detail,
    Result { run: Option<RunId> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecentSurface {
    Task,
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnterMode {
    /// Persistent navigator entries: clear returns and all overlays.
    Structural,
    /// A user drill-down: capture the caller and close overlays it replaces.
    DrillDown,
    /// Same-section update (tab/filter/panel): no return and no close.
    Replace,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Destination {
    Route(Route),
    Overlay(Overlay),
    TaskOutcome { task: TaskId, surface: TaskSurface },
    Period {
        base: PeriodBase,
        automation: AutomationId,
        occurrence: OccurrenceId,
    },
    Recent { task: TaskId, surface: RecentSurface },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScrollKey {
    TaskList,
    BoardX,
    BoardColumn(u8),
    Topic,
    PeriodBody,
    Settings,
    Result,
    Dialog,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ScrollBookmark {
    entries: BTreeMap<ScrollKey, (f32, f32)>,
}

impl ScrollBookmark {
    pub(crate) fn set(&mut self, key: ScrollKey, x: f32, y: f32) {
        self.entries.insert(key, (x, y));
    }
    pub(crate) fn get(&self, key: ScrollKey) -> Option<(f32, f32)> {
        self.entries.get(&key).copied()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResultFallback {
    pub project: Option<ProjectId>,
    pub topic: Option<TopicId>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReturnFrame {
    pub route: Route,
    pub overlays: Vec<Overlay>,
    pub scroll: ScrollBookmark,
    /// Captured at push time for a deleted selected run. It never lives on the
    /// live result page and cannot become a second navigation authority.
    pub result_fallback: Option<ResultFallback>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ReturnStack {
    frames: Vec<ReturnFrame>,
}

impl ReturnStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub(crate) fn last(&self) -> Option<&ReturnFrame> {
        self.frames.last()
    }
    pub(crate) fn clear(&mut self) {
        self.frames.clear();
    }
    pub(crate) fn record(&mut self, frame: ReturnFrame) {
        if self.frames.last() == Some(&frame) {
            return;
        }
        self.frames.push(frame);
        if self.frames.len() > MAX_RETURN_DEPTH {
            let excess = self.frames.len() - MAX_RETURN_DEPTH;
            self.frames.drain(0..excess);
        }
    }
    fn pop(&mut self) -> Option<ReturnFrame> {
        self.frames.pop()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingKind {
    /// A T21 impact review must be confirmed before the save may proceed.
    ImpactReview { task: TaskId },
    /// The write is queued/awaiting a durable acknowledgement. Not Saved.
    Durability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Confirmation {
    Confirmed,
    Pending(PendingKind),
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NavEffect {
    Entered { captured_return: bool },
    Back { restored: Restored },
    Closed { overlay: Overlay },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Restored {
    pub frame: ReturnFrame,
    pub notice: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ConfirmationStep {
    Advanced(NavEffect),
    Pending(PendingKind),
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NavCommand {
    Enter {
        dest: Destination,
        mode: EnterMode,
    },
    Back,
    CloseTop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackAvailability {
    Enabled,
    NoDestination,
    BlockedByOverlay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NavReject {
    NoDestination,
    BlockedByOverlay,
    InvalidStack,
    DuplicateOverlay,
    MissingOwner { kind: &'static str, id: u64 },
    InvalidOwnership { reason: &'static str },
    InvalidRoute,
}

impl NavReject {
    fn missing(kind: &'static str, id: u64) -> Self {
        Self::MissingOwner { kind, id }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NavState {
    route: Route,
    overlays: Vec<Overlay>,
    returns: ReturnStack,
    recent_surfaces: BTreeMap<TaskId, RecentSurface>,
}

impl NavState {
    /// Checked construction. A route whose owner no longer exists is rejected;
    /// callers cannot create an invalid initial state through the public API.
    pub(crate) fn new(store: &Store, route: Route) -> Result<Self, NavReject> {
        validate_route(&route, store)?;
        Ok(Self {
            route,
            overlays: Vec::new(),
            returns: ReturnStack::new(),
            recent_surfaces: BTreeMap::new(),
        })
    }

    pub(crate) fn route(&self) -> &Route {
        &self.route
    }
    pub(crate) fn overlays(&self) -> &[Overlay] {
        &self.overlays
    }
    pub(crate) fn top_overlay(&self) -> Option<Overlay> {
        self.overlays.last().copied()
    }
    pub(crate) fn returns(&self) -> &ReturnStack {
        &self.returns
    }
    pub(crate) fn remember_recent_surface(&mut self, task: TaskId, surface: RecentSurface) {
        self.recent_surfaces.insert(task, surface);
    }
    pub(crate) fn recent_surface(&self, task: TaskId) -> RecentSurface {
        self.recent_surfaces
            .get(&task)
            .copied()
            .unwrap_or(RecentSurface::Task)
    }
    pub(crate) fn data_scope(&self, store: &Store) -> DataScope {
        self.route.data_scope(store)
    }
    pub(crate) fn task(&self) -> Option<TaskId> {
        self.route.task()
    }

    /// The only destination mutator. A rejected command leaves `self`
    /// unchanged; tests assert equality against the pre-command snapshot.
    pub(crate) fn enter(
        &mut self,
        store: &Store,
        dest: Destination,
        mode: EnterMode,
        scroll: ScrollBookmark,
    ) -> Result<NavEffect, NavReject> {
        let mut candidate = self.clone();
        let captured = candidate.apply_destination(store, dest, mode, scroll)?;
        candidate.validate(store)?;
        *self = candidate;
        Ok(NavEffect::Entered {
            captured_return: captured,
        })
    }

    fn apply_destination(
        &mut self,
        store: &Store,
        dest: Destination,
        mode: EnterMode,
        scroll: ScrollBookmark,
    ) -> Result<bool, NavReject> {
        match dest {
            Destination::Route(route) => {
                validate_route(&route, store)?;
                let captured = self.begin_destination(mode, scroll, None);
                self.route = route;
                Ok(captured)
            }
            Destination::TaskOutcome { task, surface } => {
                let route = route_for_task_surface(store, task, surface)?;
                let captured = self.begin_destination(mode, scroll, None);
                self.route = route;
                Ok(captured)
            }
            Destination::Recent { task, surface } => {
                let route = route_for_recent(store, task, surface)?;
                let captured = self.begin_destination(mode, scroll, None);
                self.route = route;
                Ok(captured)
            }
            Destination::Overlay(overlay) => {
                self.push_overlay_internal(store, overlay)?;
                Ok(false)
            }
            Destination::Period {
                base,
                automation,
                occurrence,
            } => {
                let route = route_for_period(store, base, automation, occurrence)?;
                let captured = self.begin_destination(mode, scroll, None);
                self.route = route;
                self.push_overlay_internal(
                    store,
                    Overlay::AutomationPeriod {
                        automation,
                        occurrence,
                    },
                )?;
                Ok(captured)
            }
        }
    }

    fn begin_destination(
        &mut self,
        mode: EnterMode,
        scroll: ScrollBookmark,
        result_fallback: Option<ResultFallback>,
    ) -> bool {
        match mode {
            EnterMode::Structural => {
                self.overlays.clear();
                self.returns.clear();
                false
            }
            EnterMode::DrillDown => {
                let frame = ReturnFrame {
                    route: self.route.clone(),
                    overlays: self.overlays.clone(),
                    scroll,
                    result_fallback,
                };
                self.returns.record(frame);
                self.overlays.clear();
                true
            }
            EnterMode::Replace => false,
        }
    }

    pub(crate) fn push_overlay(
        &mut self,
        store: &Store,
        overlay: Overlay,
    ) -> Result<(), NavReject> {
        let mut candidate = self.clone();
        candidate.push_overlay_internal(store, overlay)?;
        candidate.validate(store)?;
        *self = candidate;
        Ok(())
    }

    fn push_overlay_internal(&mut self, store: &Store, overlay: Overlay) -> Result<(), NavReject> {
        if self
            .overlays
            .iter()
            .any(|existing| existing.kind() == overlay.kind())
        {
            return Err(NavReject::DuplicateOverlay);
        }
        let mut next = self.overlays.clone();
        next.push(overlay);
        validate_overlay_stack(&self.route, &next)?;
        validate_overlay_owners(&next, store)?;
        self.overlays = next;
        Ok(())
    }

    pub(crate) fn close_top(
        &mut self,
        store: &Store,
    ) -> Result<NavEffect, NavReject> {
        let mut candidate = self.clone();
        let overlay = candidate
            .overlays
            .pop()
            .ok_or(NavReject::NoDestination)?;
        candidate.validate(store)?;
        *self = candidate;
        Ok(NavEffect::Closed { overlay })
    }

    pub(crate) fn back_availability(&self, store: &Store) -> BackAvailability {
        if !self.overlays.is_empty() {
            return BackAvailability::BlockedByOverlay;
        }
        if !self.returns.is_empty() || structural_parent(&self.route, store).is_some() {
            BackAvailability::Enabled
        } else {
            BackAvailability::NoDestination
        }
    }

    /// Pop one return frame, or use the structural parent when no frame
    /// exists. A frame whose owner disappeared is normalized, never replayed as
    /// a blank/not-found page.
    pub(crate) fn back(&mut self, store: &Store) -> Result<NavEffect, NavReject> {
        if !self.overlays.is_empty() {
            return Err(NavReject::BlockedByOverlay);
        }
        let mut candidate = self.clone();
        if let Some(frame) = candidate.returns.pop() {
            let restored = validate_frame(frame, store);
            candidate.route = restored.frame.route.clone();
            candidate.overlays = restored.frame.overlays.clone();
            candidate.validate(store)?;
            let frame = restored.frame;
            *self = candidate;
            return Ok(NavEffect::Back {
                restored: Restored {
                    frame,
                    notice: restored.notice,
                },
            });
        }
        let Some(parent) = structural_parent(&candidate.route, store) else {
            return Err(NavReject::NoDestination);
        };
        candidate.route = parent;
        candidate.overlays.clear();
        candidate.returns.clear();
        candidate.validate(store)?;
        let frame = ReturnFrame {
            route: candidate.route.clone(),
            overlays: vec![],
            scroll: ScrollBookmark::default(),
            result_fallback: None,
        };
        *self = candidate;
        Ok(NavEffect::Back {
            restored: Restored {
                frame,
                notice: None,
            },
        })
    }

    /// Apply a save/discard/impact outcome. Only `Confirmed` advances the
    /// route/overlay state. `PendingImpact` opens the T21 child above the
    /// decision/editor; `PendingDurability` is explicitly not Saved.
    pub(crate) fn apply_confirmation(
        &mut self,
        confirmation: Confirmation,
        command: NavCommand,
        store: &Store,
        scroll: ScrollBookmark,
    ) -> ConfirmationStep {
        match confirmation {
            Confirmation::Rejected(message) => ConfirmationStep::Rejected(message),
            Confirmation::Pending(PendingKind::Durability) => {
                ConfirmationStep::Pending(PendingKind::Durability)
            }
            Confirmation::Pending(PendingKind::ImpactReview { task }) => {
                let mut candidate = self.clone();
                let pushed = candidate
                    .push_overlay_internal(store, Overlay::ResearchImpactReview { task })
                    .and_then(|_| candidate.validate(store));
                match pushed {
                    Ok(()) => {
                        *self = candidate;
                        ConfirmationStep::Pending(PendingKind::ImpactReview { task })
                    }
                    Err(reject) => ConfirmationStep::Rejected(format!("{reject:?}")),
                }
            }
            Confirmation::Confirmed => {
                let mut candidate = self.clone();
                if matches!(command, NavCommand::CloseTop) {
                    // Save-and-close is a completed flow, not an empty-stack
                    // error. Remove exactly the editor/decision/impact flow;
                    // leave an unrelated Search (and any future approved
                    // sibling surface) alone.
                    if let Some(overlay) = candidate.take_completed_flow() {
                        return match candidate.validate(store) {
                            Ok(()) => {
                                *self = candidate;
                                ConfirmationStep::Advanced(NavEffect::Closed { overlay })
                            }
                            Err(reject) => {
                                ConfirmationStep::Rejected(format!("{reject:?}"))
                            }
                        };
                    }
                } else {
                    candidate.take_completed_flow();
                }
                match candidate.execute(command, store, scroll) {
                    Ok(effect) => {
                        *self = candidate;
                        ConfirmationStep::Advanced(effect)
                    }
                    Err(reject) => ConfirmationStep::Rejected(format!("{reject:?}")),
                }
            }
        }
    }

    /// Remove exactly the completed editor/decision/impact flow. Search and
    /// other unrelated overlays are preserved; no parent is popped. Returns the
    /// topmost removed overlay so a confirmed Close/Escape can report success.
    fn take_completed_flow(&mut self) -> Option<Overlay> {
        let target = self
            .overlays
            .iter()
            .rev()
            .find_map(|overlay| match overlay {
                Overlay::DraftDecision { editor } => Some(*editor),
                _ => None,
            })
            .or_else(|| {
                self.overlays.last().and_then(|overlay| match overlay {
                    Overlay::TaskEditor { task } => Some(EditorTarget::Task(*task)),
                    Overlay::PlanningComposer => Some(EditorTarget::PlanningRequest),
                    _ => None,
                })
            })?;
        let mut removed = None;
        let mut index = 0;
        while index < self.overlays.len() {
            let overlay = self.overlays[index];
            let is_flow = matches!(
                overlay.class(),
                OverlayClass::Decision | OverlayClass::Impact
            ) || match (target, overlay) {
                (EditorTarget::Task(task), Overlay::TaskEditor { task: editor }) => {
                    task == editor
                }
                (EditorTarget::PlanningRequest, Overlay::PlanningComposer) => true,
                _ => false,
            };
            if is_flow {
                removed = Some(self.overlays.remove(index));
            } else {
                index += 1;
            }
        }
        removed
    }

    pub(crate) fn execute(
        &mut self,
        command: NavCommand,
        store: &Store,
        scroll: ScrollBookmark,
    ) -> Result<NavEffect, NavReject> {
        match command {
            NavCommand::Enter { dest, mode } => self.enter(store, dest, mode, scroll),
            NavCommand::Back => self.back(store),
            NavCommand::CloseTop => self.close_top(store),
        }
    }

    pub(crate) fn set_research_filters(
        &mut self,
        all_status: AllStatus,
        layout: Layout,
    ) -> Result<(), NavReject> {
        match &mut self.route {
            Route::ResearchList(page) => {
                page.all_status = all_status;
                page.layout = layout;
                Ok(())
            }
            _ => Err(NavReject::InvalidRoute),
        }
    }

    pub(crate) fn set_task_panel(&mut self, panel: TaskPanel) -> Result<(), NavReject> {
        match &mut self.route {
            Route::ResearchTask(page) => {
                page.panel = panel;
                Ok(())
            }
            _ => Err(NavReject::InvalidRoute),
        }
    }

    pub(crate) fn select_run(&mut self, store: &Store, run: RunId) -> Result<(), NavReject> {
        let mut candidate = self.clone();
        let Route::ResearchResult(page) = &mut candidate.route else {
            return Err(NavReject::InvalidRoute);
        };
        let next = ResearchResultPage::new(
            page.owner,
            ResultSelection { run: Some(run) },
        );
        validate_result_page(&next, store)?;
        candidate.route = Route::ResearchResult(next);
        candidate.validate(store)?;
        *self = candidate;
        Ok(())
    }

    pub(crate) fn set_results_filter(
        &mut self,
        store: &Store,
        filter: Option<AutomationId>,
        history_limit: usize,
    ) -> Result<(), NavReject> {
        let mut candidate = self.clone();
        match &mut candidate.route {
            Route::AutomationResults(page) => {
                page.filter = filter;
                page.history_limit = history_limit;
            }
            _ => return Err(NavReject::InvalidRoute),
        }
        candidate.validate(store)?;
        *self = candidate;
        Ok(())
    }

    pub(crate) fn set_settings_tab(&mut self, tab: SettingsSection) -> Result<(), NavReject> {
        match &mut self.route {
            Route::Settings(page) => {
                page.tab = tab;
                Ok(())
            }
            _ => Err(NavReject::InvalidRoute),
        }
    }

    fn validate(&self, store: &Store) -> Result<(), NavReject> {
        validate_route(&self.route, store)?;
        validate_overlay_stack(&self.route, &self.overlays)?;
        validate_overlay_owners(&self.overlays, store)
    }
}

fn route_for_task_surface(
    store: &Store,
    task: TaskId,
    surface: TaskSurface,
) -> Result<Route, NavReject> {
    let definition = store.task(task).ok_or_else(|| NavReject::missing("task", task))?;
    match surface {
        TaskSurface::Detail => Ok(Route::research_task(task)),
        TaskSurface::Result { run } => {
            let owner = result_owner_for_task(store, task)?;
            let route = Route::research_result(owner, run);
            validate_route(&route, store)?;
            let _ = definition;
            Ok(route)
        }
    }
}

fn route_for_recent(
    store: &Store,
    task: TaskId,
    surface: RecentSurface,
) -> Result<Route, NavReject> {
    match surface {
        RecentSurface::Task => route_for_task_surface(store, task, TaskSurface::Detail),
        RecentSurface::Result => {
            let run = store.research_result(task).map(|run| run.id).or_else(|| {
                store
                    .runs()
                    .iter()
                    .rev()
                    .find(|run| run.task_id == task && run.project_plan.is_none())
                    .map(|run| run.id)
            });
            route_for_task_surface(store, task, TaskSurface::Result { run })
        }
    }
}

fn result_owner_for_task(store: &Store, task: TaskId) -> Result<ResultOwner, NavReject> {
    let value = store.task(task).ok_or_else(|| NavReject::missing("task", task))?;
    if let Some(automation) = value.automation_id {
        let (_, occurrence) = store
            .automation_period_for_task(task)
            .ok_or(NavReject::InvalidOwnership {
                reason: "automation step has no period",
            })?;
        Ok(ResultOwner::AutomationTask {
            automation,
            occurrence,
            task,
        })
    } else {
        Ok(ResultOwner::Task(task))
    }
}

fn route_for_period(
    store: &Store,
    base: PeriodBase,
    automation: AutomationId,
    occurrence: OccurrenceId,
) -> Result<Route, NavReject> {
    let definition = store
        .automation(automation)
        .ok_or_else(|| NavReject::missing("automation", automation))?;
    let value = store
        .occurrence(occurrence)
        .ok_or_else(|| NavReject::missing("occurrence", occurrence))?;
    if value.automation_id != automation {
        return Err(NavReject::InvalidOwnership {
            reason: "period does not belong to automation",
        });
    }
    match base {
        PeriodBase::Detail { automation: base } if base == automation => {
            Ok(Route::automation_detail(definition.id))
        }
        PeriodBase::Detail { .. } => Err(NavReject::InvalidOwnership {
            reason: "period base detail names another automation",
        }),
        PeriodBase::Results {
            filter,
            history_limit,
        } => Ok(Route::automation_results(filter, history_limit)),
    }
}

fn validate_route(route: &Route, store: &Store) -> Result<(), NavReject> {
    match route {
        Route::ResearchList(page) => match page.scope() {
            ResearchScope::Project(id) if !store.projects.iter().any(|p| p.id == *id) => {
                Err(NavReject::missing("project", *id))
            }
            ResearchScope::Topic(id) if !store.topics.iter().any(|t| t.id == *id) => {
                Err(NavReject::missing("topic", *id))
            }
            _ => Ok(()),
        },
        Route::ResearchTask(page) => {
            if store.task(page.task()).is_none() {
                Err(NavReject::missing("task", page.task()))
            } else {
                Ok(())
            }
        }
        Route::ResearchResult(page) => validate_result_page(page, store),
        Route::PausedProjects | Route::MarketResearch => Ok(()),
        Route::Settings(_) => Ok(()),
        Route::AutomationList => Ok(()),
        Route::AutomationResults(page) => {
            if let Some(filter) = page.filter()
                && store.automation(filter).is_none()
            {
                return Err(NavReject::missing("automation", filter));
            }
            if page.history_limit() == 0 {
                return Err(NavReject::InvalidOwnership {
                    reason: "results history limit must be positive",
                });
            }
            Ok(())
        }
        Route::AutomationDetail(page) => {
            if store.automation(page.automation()).is_none() {
                Err(NavReject::missing("automation", page.automation()))
            } else {
                Ok(())
            }
        }
        Route::AutomationEditor(page) => {
            if let Some(automation) = page.editing() {
                if store.automation(automation).is_none() {
                    return Err(NavReject::missing("automation", automation));
                }
            }
            Ok(())
        }
    }
}

fn validate_result_page(page: &ResearchResultPage, store: &Store) -> Result<(), NavReject> {
    validate_result_owner(page.owner(), store)?;
    if let Some(run) = page.run() {
        validate_result_run(page.owner(), run, store)?;
    }
    Ok(())
}

fn validate_result_owner(owner: ResultOwner, store: &Store) -> Result<(), NavReject> {
    match owner {
        ResultOwner::OrdinaryPage => Ok(()),
        ResultOwner::Task(task) => {
            let value = store.task(task).ok_or_else(|| NavReject::missing("task", task))?;
            if value.automation_id.is_some() {
                Err(NavReject::InvalidOwnership {
                    reason: "an automation task needs an explicit period owner",
                })
            } else {
                Ok(())
            }
        }
        ResultOwner::AutomationTask {
            automation,
            occurrence,
            task,
        } => {
            if store.automation(automation).is_none() {
                return Err(NavReject::missing("automation", automation));
            }
            let value = store
                .occurrence(occurrence)
                .ok_or_else(|| NavReject::missing("occurrence", occurrence))?;
            if value.automation_id != automation {
                return Err(NavReject::InvalidOwnership {
                    reason: "period does not belong to automation",
                });
            }
            let task_value = store.task(task).ok_or_else(|| NavReject::missing("task", task))?;
            if task_value.automation_id != Some(automation) {
                return Err(NavReject::InvalidOwnership {
                    reason: "task is not owned by the automation",
                });
            }
            if store.automation_period_for_task(task) != Some((automation, occurrence)) {
                return Err(NavReject::InvalidOwnership {
                    reason: "task is not in the named period",
                });
            }
            Ok(())
        }
        ResultOwner::PlanningProgress { request } => {
            let value = store
                .research_requests
                .iter()
                .find(|value| value.id == request)
                .ok_or_else(|| NavReject::missing("request", request))?;
            if value.automation_id.is_some() {
                Err(NavReject::InvalidOwnership {
                    reason: "automation planning is T23-owned history",
                })
            } else {
                Ok(())
            }
        }
    }
}

fn validate_result_run(owner: ResultOwner, run: RunId, store: &Store) -> Result<(), NavReject> {
    let value = store
        .runs()
        .iter()
        .find(|value| value.id == run)
        .ok_or_else(|| NavReject::missing("run", run))?;
    match owner {
        ResultOwner::OrdinaryPage => {
            if store.run_is_automation_owned(value) {
                Err(NavReject::InvalidOwnership {
                    reason: "automation run cannot use the ordinary results page",
                })
            } else {
                Ok(())
            }
        }
        ResultOwner::Task(task) => {
            if value.task_id != task {
                return Err(NavReject::InvalidOwnership {
                    reason: "run belongs to another task",
                });
            }
            if store.run_is_automation_owned(value) {
                return Err(NavReject::InvalidOwnership {
                    reason: "automation run cannot use a task-only result owner",
                });
            }
            Ok(())
        }
        ResultOwner::AutomationTask {
            automation,
            occurrence,
            task,
        } => {
            if value.task_id != task {
                return Err(NavReject::InvalidOwnership {
                    reason: "run belongs to another task",
                });
            }
            if value.automation_occurrence_id != Some(occurrence) {
                return Err(NavReject::InvalidOwnership {
                    reason: "run does not belong to the named period",
                });
            }
            if store.automation_period_for_task(task) != Some((automation, occurrence)) {
                return Err(NavReject::InvalidOwnership {
                    reason: "task/period identity no longer matches",
                });
            }
            if !store.run_is_automation_owned(value) {
                return Err(NavReject::InvalidOwnership {
                    reason: "named period run is not automation-owned",
                });
            }
            Ok(())
        }
        ResultOwner::PlanningProgress { request } => {
            if store.run_is_automation_owned(value) {
                return Err(NavReject::InvalidOwnership {
                    reason: "automation planning is T23-owned history",
                });
            }
            if store
                .request_for_run(value.id)
                .map(|value| value.id)
                != Some(request)
            {
                return Err(NavReject::InvalidOwnership {
                    reason: "run is not the named request planning attempt",
                });
            }
            Ok(())
        }
    }
}

fn validate_overlay_stack(base: &Route, overlays: &[Overlay]) -> Result<(), NavReject> {
    if overlays.len() > MAX_OVERLAY_DEPTH {
        return Err(NavReject::InvalidStack);
    }
    let mut search = 0usize;
    let mut roots = 0usize;
    let mut decisions = 0usize;
    let mut impacts = 0usize;
    let mut decision: Option<EditorTarget> = None;
    let mut root_task: Option<TaskId> = None;
    let mut root_kind: Option<OverlayKind> = None;
    let mut first_root_index = None;

    for (index, overlay) in overlays.iter().enumerate() {
        match overlay.class() {
            OverlayClass::Search => search += 1,
            OverlayClass::Editor | OverlayClass::Content => {
                roots += 1;
                first_root_index.get_or_insert(index);
                root_kind = Some(overlay.kind());
                root_task = overlay.task();
            }
            OverlayClass::Decision => {
                decisions += 1;
                decision = match overlay {
                    Overlay::DraftDecision { editor } => Some(*editor),
                    _ => unreachable!(),
                };
            }
            OverlayClass::Impact => impacts += 1,
        }
    }
    if search > 1 || roots > 1 || decisions > 1 || impacts > 1 {
        return Err(NavReject::InvalidStack);
    }
    if let Some(index) = first_root_index {
        if index != 0 {
            // A root cannot open above Search; Search is a child of a page or
            // an existing modal, not a base for an editor.
            return Err(NavReject::InvalidStack);
        }
    }

    // Decision targets must match the underlying editor.
    if let Some(target) = decision {
        match target {
            EditorTarget::Task(task) => {
                if root_kind != Some(OverlayKind::TaskEditor) || root_task != Some(task) {
                    return Err(NavReject::InvalidStack);
                }
            }
            EditorTarget::PlanningRequest => {
                if root_kind != Some(OverlayKind::PlanningComposer) {
                    return Err(NavReject::InvalidStack);
                }
            }
            EditorTarget::Automation { .. } => {
                if !matches!(base, Route::AutomationEditor(_)) || roots != 0 {
                    return Err(NavReject::InvalidStack);
                }
            }
        }
    }

    // T21 impact review is a save child of the task editor (optionally through
    // the decision that initiated Save), never a standalone modal.
    if impacts > 0 {
        if root_kind != Some(OverlayKind::TaskEditor) {
            return Err(NavReject::InvalidStack);
        }
        let impact_task = overlays.iter().find_map(|overlay| match overlay {
            Overlay::ResearchImpactReview { task } => Some(*task),
            _ => None,
        });
        if impact_task != root_task {
            return Err(NavReject::InvalidStack);
        }
        if let Some(target) = decision
            && !matches!(target, EditorTarget::Task(_))
        {
            return Err(NavReject::InvalidStack);
        }
        if let Some(target) = decision
            && root_task != target.task()
        {
            return Err(NavReject::InvalidStack);
        }
        if let Some(target) = decision
            && impact_task != target.task()
        {
            return Err(NavReject::InvalidStack);
        }
    }

    // Decision and impact ordering: decision, if present, is below impact.
    if decisions > 0 && impacts > 0 {
        let decision_index = overlays
            .iter()
            .position(|overlay| overlay.class() == OverlayClass::Decision)
            .unwrap();
        let impact_index = overlays
            .iter()
            .position(|overlay| overlay.class() == OverlayClass::Impact)
            .unwrap();
        if decision_index > impact_index {
            return Err(NavReject::InvalidStack);
        }
    }

    // Content roots are view/config surfaces and cannot carry a decision or
    // impact child. Previous versions is view-only and cannot coexist with an
    // editor/content root beyond Search.
    if let Some(kind) = root_kind {
        let view_only = matches!(
            kind,
            OverlayKind::AutomationSettings
                | OverlayKind::AutomationPeriod
                | OverlayKind::PlanningAttempt
                | OverlayKind::PreviousVersions
                | OverlayKind::ProjectRename
                | OverlayKind::TopicEditor
                | OverlayKind::TopicMove
                | OverlayKind::FolderDialog
                | OverlayKind::NewProject
        );
        if view_only && (decisions > 0 || impacts > 0) {
            return Err(NavReject::InvalidStack);
        }
    }

    // Base compatibility for roots that name an owner.
    for overlay in overlays {
        validate_overlay_base(base, *overlay)?;
    }
    Ok(())
}

impl EditorTarget {
    fn task(self) -> Option<TaskId> {
        match self {
            Self::Task(task) => Some(task),
            _ => None,
        }
    }
}

fn validate_overlay_base(base: &Route, overlay: Overlay) -> Result<(), NavReject> {
    match overlay {
        Overlay::TaskEditor { task } => match base {
            Route::ResearchTask(page) if page.task() == task => Ok(()),
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::AutomationSettings { automation } => match base {
            Route::AutomationDetail(page) if page.automation() == automation => Ok(()),
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::AutomationPeriod {
            automation,
            occurrence: _,
        } => match base {
            Route::AutomationDetail(page) if page.automation() == automation => Ok(()),
            Route::AutomationResults(_) => Ok(()),
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::PlanningAttempt { .. } => match base {
            Route::AutomationDetail(_) | Route::AutomationList | Route::AutomationResults(_) => {
                Ok(())
            }
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::PreviousVersions { task, .. } => match base {
            Route::ResearchTask(page) if page.task() == task => Ok(()),
            Route::ResearchResult(page) => match page.owner() {
                ResultOwner::Task(owner) if owner == task => Ok(()),
                ResultOwner::AutomationTask { task: owner, .. } if owner == task => Ok(()),
                _ => Err(NavReject::InvalidStack),
            },
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::ResearchImpactReview { .. } => match base {
            Route::ResearchTask(_) => Ok(()),
            _ => Err(NavReject::InvalidStack),
        },
        Overlay::DraftDecision { .. } => match base {
            Route::ResearchTask(_)
            | Route::AutomationEditor(_)
            | Route::AutomationDetail(_)
            | Route::AutomationList => Ok(()),
            _ => Ok(()),
        },
        Overlay::Search
        | Overlay::PlanningComposer
        | Overlay::ProjectRename { .. }
        | Overlay::TopicEditor { .. }
        | Overlay::TopicMove { .. }
        | Overlay::FolderDialog { .. }
        | Overlay::NewProject => Ok(()),
    }
}

fn validate_overlay_owners(overlays: &[Overlay], store: &Store) -> Result<(), NavReject> {
    for overlay in overlays {
        match overlay {
            Overlay::TaskEditor { task } | Overlay::ResearchImpactReview { task } => {
                if store.task(*task).is_none() {
                    return Err(NavReject::missing("task", *task));
                }
            }
            Overlay::AutomationSettings { automation } => {
                if store.automation(*automation).is_none() {
                    return Err(NavReject::missing("automation", *automation));
                }
            }
            Overlay::AutomationPeriod {
                automation,
                occurrence,
            } => {
                if store.automation(*automation).is_none() {
                    return Err(NavReject::missing("automation", *automation));
                }
                let value = store
                    .occurrence(*occurrence)
                    .ok_or_else(|| NavReject::missing("occurrence", *occurrence))?;
                if value.automation_id != *automation {
                    return Err(NavReject::InvalidOwnership {
                        reason: "period does not belong to automation",
                    });
                }
            }
            Overlay::PlanningAttempt { run, .. } => {
                if !store.runs().iter().any(|value| value.id == *run) {
                    return Err(NavReject::missing("run", *run));
                }
            }
            Overlay::PreviousVersions { task, run } => {
                if store.task(*task).is_none() {
                    return Err(NavReject::missing("task", *task));
                }
                if let Some(run) = run {
                    let value = store
                        .runs()
                        .iter()
                        .find(|value| value.id == *run)
                        .ok_or_else(|| NavReject::missing("run", *run))?;
                    if value.task_id != *task {
                        return Err(NavReject::InvalidOwnership {
                            reason: "previous version belongs to another task",
                        });
                    }
                }
            }
            Overlay::ProjectRename { project } | Overlay::FolderDialog { project } => {
                if !store.projects.iter().any(|value| value.id == *project) {
                    return Err(NavReject::missing("project", *project));
                }
            }
            Overlay::TopicEditor { project, topic } => {
                if !store.projects.iter().any(|value| value.id == *project) {
                    return Err(NavReject::missing("project", *project));
                }
                if let Some(topic) = topic
                    && !store.topics.iter().any(|value| value.id == *topic)
                {
                    return Err(NavReject::missing("topic", *topic));
                }
            }
            Overlay::TopicMove { topic } => {
                if !store.topics.iter().any(|value| value.id == *topic) {
                    return Err(NavReject::missing("topic", *topic));
                }
            }
            Overlay::DraftDecision { editor } => match editor {
                EditorTarget::Task(task) => {
                    if store.task(*task).is_none() {
                        return Err(NavReject::missing("task", *task));
                    }
                }
                EditorTarget::Automation { editing } => {
                    if let Some(automation) = editing
                        && store.automation(*automation).is_none()
                    {
                        return Err(NavReject::missing("automation", *automation));
                    }
                }
                EditorTarget::PlanningRequest => {}
            },
            Overlay::Search | Overlay::PlanningComposer | Overlay::NewProject => {}
        }
    }
    Ok(())
}

fn structural_parent(route: &Route, store: &Store) -> Option<Route> {
    match route {
        Route::ResearchList(page) => match page.scope() {
            ResearchScope::All => None,
            ResearchScope::Project(_) | ResearchScope::Cycle(_) | ResearchScope::Archive => {
                Some(Route::research_list(
                    ResearchScope::All,
                    AllStatus::All,
                    Layout::List,
                ))
            }
            ResearchScope::Topic(topic) => store
                .topics
                .iter()
                .find(|value| value.id == *topic)
                .and_then(|value| {
                    store
                        .projects
                        .iter()
                        .find(|project| project.id == value.project_id)
                })
                .map(|project| {
                    Route::research_list(
                        ResearchScope::Project(project.id),
                        AllStatus::All,
                        Layout::List,
                    )
                })
                .or_else(|| {
                    Some(Route::research_list(
                        ResearchScope::All,
                        AllStatus::All,
                        Layout::List,
                    ))
                }),
        },
        Route::ResearchTask(page) => {
            let task = store.task(page.task())?;
            if let Some(parent) = task.parent_id
                && store.task(parent).is_some()
            {
                return Some(Route::research_task(parent));
            }
            if let Some(topic) = store.task_topic(task.id) {
                return Some(Route::research_list(
                    ResearchScope::Topic(topic.id),
                    AllStatus::All,
                    Layout::List,
                ));
            }
            if store.projects.iter().any(|project| project.id == task.project_id) {
                return Some(Route::research_list(
                    ResearchScope::Project(task.project_id),
                    AllStatus::All,
                    Layout::List,
                ));
            }
            Some(Route::research_list(
                ResearchScope::All,
                AllStatus::All,
                Layout::List,
            ))
        }
        Route::ResearchResult(page) => match page.owner() {
            ResultOwner::Task(task) | ResultOwner::AutomationTask { task, .. } => {
                Some(Route::research_task(task))
            }
            ResultOwner::OrdinaryPage | ResultOwner::PlanningProgress { .. } => None,
        },
        Route::PausedProjects | Route::MarketResearch | Route::Settings(_) => None,
        Route::AutomationList => None,
        Route::AutomationResults(_) => Some(Route::AutomationList),
        Route::AutomationDetail(_page) => Some(Route::AutomationList),
        Route::AutomationEditor(page) => page
            .editing()
            .map(Route::automation_detail)
            .or(Some(Route::AutomationList)),
    }
}

fn validate_frame(mut frame: ReturnFrame, store: &Store) -> Restored {
    let mut notice = None;
    if let Route::AutomationResults(page) = &mut frame.route {
        if let Some(filter) = page.filter
            && store.automation(filter).is_none()
        {
            page.filter = None;
            notice = Some("That automation filter is no longer available.".into());
        }
        if page.history_limit == 0 {
            page.history_limit = 20;
            notice = Some("The saved history limit was invalid; the default was restored.".into());
        }
    }
    if validate_route(&frame.route, store).is_err() {
        let fallback = match &frame.route {
            Route::ResearchList(ResearchListPage {
                scope: ResearchScope::Project(_) | ResearchScope::Topic(_) | ResearchScope::Cycle(_),
                ..
            }) => Route::research_list(ResearchScope::All, AllStatus::All, Layout::List),
            Route::ResearchTask(_) => {
                Route::research_list(ResearchScope::All, AllStatus::All, Layout::List)
            }
            Route::ResearchResult(page) => match page.owner() {
                ResultOwner::Task(_) | ResultOwner::AutomationTask { .. } => {
                    Route::research_list(ResearchScope::All, AllStatus::All, Layout::List)
                }
                ResultOwner::OrdinaryPage => {
                    Route::research_result(ResultOwner::OrdinaryPage, None)
                }
                ResultOwner::PlanningProgress { .. } => {
                    Route::research_result(ResultOwner::OrdinaryPage, None)
                }
            },
            Route::AutomationDetail(_) | Route::AutomationEditor(_) => Route::AutomationList,
            _ => Route::research_list(ResearchScope::All, AllStatus::All, Layout::List),
        };
        frame.route = fallback;
        frame.overlays.clear();
        notice = Some("That page is no longer available. Returned to a safe parent.".into());
        return Restored { frame, notice };
    }

    if let Route::ResearchResult(page) = &mut frame.route {
        if let Some(run) = page.run()
            && validate_result_run(page.owner(), run, store).is_err()
        {
            page.selection = ResultSelection::empty();
            notice = Some("That result is no longer available. Returned to Research results.".into());
        }
    }

    while !frame.overlays.is_empty()
        && (validate_overlay_stack(&frame.route, &frame.overlays).is_err()
            || validate_overlay_owners(&frame.overlays, store).is_err())
    {
        frame.overlays.pop();
        notice = Some("Part of the previous page is no longer available.".into());
    }
    Restored { frame, notice }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::{Project, ProjectKind, Stage};

    fn base_store() -> Store {
        let mut store = Store::empty();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.default_model = "test/model".into();
        store.research_config.models =
            ["test/plan", "test/collect", "test/analyze", "test/advise"].map(String::from);
        store.policies = Store::mock().policies;
        store
    }

    fn task(store: &mut Store, title: &str) -> TaskId {
        store
            .create_task(title, 1, Stage::Planned, None)
            .expect("task")
    }

    fn run(store: &mut Store, task: TaskId, status: &str, output: &str) -> RunId {
        push_run(store, task, None, status, output)
    }

    fn push_run(
        store: &mut Store,
        task: TaskId,
        occurrence: Option<OccurrenceId>,
        status: &str,
        output: &str,
    ) -> RunId {
        let id = store.runs().iter().map(|value| value.id).max().unwrap_or(0) + 1;
        let task_title = store
            .task(task)
            .map(|value| value.title.clone())
            .unwrap_or_default();
        store.push_run(crate::domain::AgentRun {
            automation_occurrence_id: occurrence,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id,
            task_title,
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
            output: output.into(),
            attachments: vec![],
            research_kind: None,
            sources: vec![],
            input_signature: String::new(),
        });
        id
    }

    fn template() -> crate::automation::AutomationTemplate {
        crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()
    }

    fn automation(store: &mut Store, name: &str) -> AutomationId {
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
            .expect("automation")
    }

    fn period(store: &mut Store, automation: AutomationId, scheduled: i64) -> OccurrenceId {
        let definition = store.automation(automation).expect("definition").clone();
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
        store.materialize_occurrence(occurrence).expect("period").id
    }

    fn list_route() -> Route {
        Route::research_list(
            ResearchScope::All,
            AllStatus::All,
            Layout::List,
        )
    }

    fn result_route(owner: ResultOwner, run: Option<RunId>) -> Route {
        Route::research_result(owner, run)
    }

    fn scroll(y: f32) -> ScrollBookmark {
        let mut scroll = ScrollBookmark::default();
        scroll.set(ScrollKey::TaskList, 0.0, y);
        scroll
    }

    #[test]
    fn structural_enter_clears_returns_and_overlays() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, list_route()).unwrap();
        state
            .enter(
                &store,
                Destination::TaskOutcome {
                    task,
                    surface: TaskSurface::Detail,
                },
                EnterMode::DrillDown,
                scroll(10.0),
            )
            .unwrap();
        assert_eq!(state.returns().len(), 1);
        state.push_overlay(&store, Overlay::TaskEditor { task }).unwrap();
        assert_eq!(state.overlays().len(), 1);
        state
            .enter(
                &store,
                Destination::Route(list_route()),
                EnterMode::Structural,
                scroll(0.0),
            )
            .unwrap();
        assert!(state.returns().is_empty());
        assert!(state.overlays().is_empty());
        assert!(matches!(state.route(), Route::ResearchList(_)));
    }

    #[test]
    fn back_restores_route_and_nonzero_scroll() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, list_route()).unwrap();
        state
            .enter(
                &store,
                Destination::TaskOutcome {
                    task,
                    surface: TaskSurface::Detail,
                },
                EnterMode::DrillDown,
                scroll(120.0),
            )
            .unwrap();
        match state.back(&store).unwrap() {
            NavEffect::Back { restored } => {
                assert!(matches!(restored.frame.route, Route::ResearchList(_)));
                assert_eq!(restored.frame.scroll.get(ScrollKey::TaskList), Some((0.0, 120.0)));
            }
            other => panic!("expected Back, got {other:?}"),
        }
    }

    #[test]
    fn back_restores_explicit_zero_scroll_bookmark() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, list_route()).unwrap();
        state
            .enter(
                &store,
                Destination::TaskOutcome {
                    task,
                    surface: TaskSurface::Detail,
                },
                EnterMode::DrillDown,
                scroll(0.0),
            )
            .unwrap();
        match state.back(&store).unwrap() {
            NavEffect::Back { restored } => {
                assert_eq!(
                    restored.frame.scroll.get(ScrollKey::TaskList),
                    Some((0.0, 0.0)),
                    "zero is a real scroll position, not absence"
                );
                assert!(!restored.frame.scroll.is_empty());
            }
            other => panic!("expected Back, got {other:?}"),
        }
    }

    #[test]
    fn deleted_owner_falls_back_instead_of_restoring_a_not_found_page() {
        let mut store = base_store();
        let first = task(&mut store, "First");
        let second = task(&mut store, "Second");
        let mut state = NavState::new(&store, Route::research_task(first)).unwrap();
        state
            .enter(
                &store,
                Destination::TaskOutcome {
                    task: second,
                    surface: TaskSurface::Detail,
                },
                EnterMode::DrillDown,
                scroll(0.0),
            )
            .unwrap();
        store.tasks.retain(|value| value.id != first);
        match state.back(&store).unwrap() {
            NavEffect::Back { restored } => {
                assert!(matches!(
                    restored.frame.route,
                    Route::ResearchList(ResearchListPage {
                        scope: ResearchScope::All,
                        ..
                    })
                ));
                assert!(restored.notice.is_some());
            }
            other => panic!("expected Back fallback, got {other:?}"),
        }
    }

    #[test]
    fn task_a_report_b_mismatch_is_rejected_atomically() {
        let mut store = base_store();
        let task_a = task(&mut store, "A");
        let task_b = task(&mut store, "B");
        let run_a = run(&mut store, task_a, "Succeeded", "A report");
        let run_b = run(&mut store, task_b, "Succeeded", "B report");
        let mut state = NavState::new(&store, result_route(ResultOwner::Task(task_a), Some(run_a))).unwrap();
        let before = state.clone();
        let err = state
            .enter(
                &store,
                Destination::Route(result_route(ResultOwner::Task(task_a), Some(run_b))),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::InvalidOwnership { .. }));
        assert_eq!(state, before, "rejected command must not mutate state");
    }

    #[test]
    fn ordinary_results_never_admit_automation_runs() {
        let mut store = base_store();
        let task = task(&mut store, "Automation task");
        store.task_mut(task).unwrap().automation_id = Some(77);
        let run = run(&mut store, task, "Succeeded", "automation output");
        let mut state = NavState::new(&store, list_route()).unwrap();
        let before = state.clone();
        let err = state
            .enter(
                &store,
                Destination::Route(result_route(ResultOwner::OrdinaryPage, Some(run))),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::InvalidOwnership { .. }));
        assert_eq!(state, before);
    }

    #[test]
    fn automation_task_result_requires_the_named_period_identity() {
        let mut store = base_store();
        let automation = automation(&mut store, "Period owner");
        let occurrence = period(&mut store, automation, 1_000);
        let steps = store.automation_period_steps(occurrence);
        let task = steps[0];
        let run = push_run(&mut store, task, Some(occurrence), "Succeeded", "period report");
        let owner = ResultOwner::AutomationTask {
            automation,
            occurrence,
            task,
        };
        let mut state = NavState::new(&store, list_route()).unwrap();
        state
            .enter(
                &store,
                Destination::Route(result_route(owner, Some(run))),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap();
        assert_eq!(state.route().task(), Some(task));

        let before = state.clone();
        let wrong = ResultOwner::AutomationTask {
            automation,
            occurrence,
            task,
        };
        let mut other_owner = wrong;
        if let ResultOwner::AutomationTask { task: wrong_task, .. } = &mut other_owner {
            *wrong_task = steps[1];
        }
        let err = state
            .enter(
                &store,
                Destination::Route(result_route(other_owner, Some(run))),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::InvalidOwnership { .. }));
        assert_eq!(state, before);
    }

    #[test]
    fn same_period_step_return_restores_period_over_results() {
        let mut store = base_store();
        let automation = automation(&mut store, "Same period");
        let occurrence = period(&mut store, automation, 1_000);
        let task = store.automation_period_steps(occurrence)[0];
        let mut state = NavState::new(&store, Route::automation_results(None, 20)).unwrap();
        state
            .enter(
                &store,
                Destination::Period {
                    base: PeriodBase::Results {
                        filter: None,
                        history_limit: 20,
                    },
                    automation,
                    occurrence,
                },
                EnterMode::DrillDown,
                scroll(0.0),
            )
            .unwrap();
        assert_eq!(
            state.top_overlay(),
            Some(Overlay::AutomationPeriod {
                automation,
                occurrence
            })
        );
        state
            .enter(
                &store,
                Destination::TaskOutcome {
                    task,
                    surface: TaskSurface::Detail,
                },
                EnterMode::DrillDown,
                scroll(0.0),
            )
            .unwrap();
        assert!(matches!(state.route(), Route::ResearchTask(_)));
        match state.back(&store).unwrap() {
            NavEffect::Back { restored } => {
                assert!(matches!(restored.frame.route, Route::AutomationResults(_)));
                assert_eq!(
                    restored.frame.overlays,
                    vec![Overlay::AutomationPeriod {
                        automation,
                        occurrence
                    }]
                );
            }
            other => panic!("expected period restore, got {other:?}"),
        }
    }

    #[test]
    fn close_top_dismisses_only_the_top_overlay() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state.push_overlay(&store, Overlay::TaskEditor { task }).unwrap();
        state.push_overlay(&store, Overlay::Search).unwrap();
        match state.close_top(&store).unwrap() {
            NavEffect::Closed { overlay } => assert_eq!(overlay, Overlay::Search),
            other => panic!("expected Search close, got {other:?}"),
        }
        assert_eq!(state.overlays(), &[Overlay::TaskEditor { task }]);
        assert!(matches!(state.route(), Route::ResearchTask(_)));
        match state.close_top(&store).unwrap() {
            NavEffect::Closed { overlay } => assert_eq!(overlay, Overlay::TaskEditor { task }),
            other => panic!("expected editor close, got {other:?}"),
        }
        assert!(state.overlays().is_empty());
    }

    #[test]
    fn pending_confirmation_never_advances_until_confirmed() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let run = run(&mut store, task, "Succeeded", "report");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state.push_overlay(&store, Overlay::TaskEditor { task }).unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            )
            .unwrap();
        let before = state.clone();
        let step = state.apply_confirmation(
            Confirmation::Pending(PendingKind::Durability),
            NavCommand::Enter {
                dest: Destination::Route(result_route(ResultOwner::Task(task), Some(run))),
                mode: EnterMode::Replace,
            },
            &store,
            scroll(0.0),
        );
        assert_eq!(step, ConfirmationStep::Pending(PendingKind::Durability));
        assert_eq!(state, before, "durability pending is not Saved and must not navigate");

        let step = state.apply_confirmation(
            Confirmation::Pending(PendingKind::ImpactReview { task }),
            NavCommand::Enter {
                dest: Destination::Route(result_route(ResultOwner::Task(task), Some(run))),
                mode: EnterMode::Replace,
            },
            &store,
            scroll(0.0),
        );
        assert_eq!(
            step,
            ConfirmationStep::Pending(PendingKind::ImpactReview { task })
        );
        assert_eq!(state.route(), before.route());
        assert_eq!(
            state.top_overlay(),
            Some(Overlay::ResearchImpactReview { task })
        );
        assert_eq!(state.returns(), before.returns());

        let step = state.apply_confirmation(
            Confirmation::Confirmed,
            NavCommand::Enter {
                dest: Destination::Route(result_route(ResultOwner::Task(task), Some(run))),
                mode: EnterMode::Replace,
            },
            &store,
            scroll(0.0),
        );
        assert!(matches!(step, ConfirmationStep::Advanced(_)));
        assert!(matches!(state.route(), Route::ResearchResult(_)));
        assert!(
            state.overlays().is_empty(),
            "completed editor/decision/impact flow must not survive over the result"
        );
    }

    #[test]
    fn confirmed_close_completes_the_editor_flow_and_preserves_nested_search() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task })
            .unwrap();
        state.push_overlay(&store, Overlay::Search).unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            )
            .unwrap();
        state
            .push_overlay(&store, Overlay::ResearchImpactReview { task })
            .unwrap();
        let step = state.apply_confirmation(
            Confirmation::Confirmed,
            NavCommand::CloseTop,
            &store,
            scroll(0.0),
        );
        assert_eq!(
            step,
            ConfirmationStep::Advanced(NavEffect::Closed {
                overlay: Overlay::ResearchImpactReview { task }
            }),
            "Save/Discard-confirmed Escape must succeed instead of hitting an empty stack"
        );
        assert_eq!(state.overlays(), &[Overlay::Search]);
        assert_eq!(state.route(), &Route::research_task(task));
        match state.close_top(&store).unwrap() {
            NavEffect::Closed { overlay } => assert_eq!(overlay, Overlay::Search),
            other => panic!("expected Search close, got {other:?}"),
        }
        assert!(state.overlays().is_empty());
        assert_eq!(state.route(), &Route::research_task(task));
    }

    #[test]
    fn confirmed_close_without_unrelated_overlays_leaves_the_page() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task })
            .unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            )
            .unwrap();
        state
            .push_overlay(&store, Overlay::ResearchImpactReview { task })
            .unwrap();
        let step = state.apply_confirmation(
            Confirmation::Confirmed,
            NavCommand::CloseTop,
            &store,
            scroll(0.0),
        );
        assert!(matches!(
            step,
            ConfirmationStep::Advanced(NavEffect::Closed {
                overlay: Overlay::ResearchImpactReview { .. }
            })
        ));
        assert!(state.overlays().is_empty());
        assert_eq!(state.route(), &Route::research_task(task));
    }

    #[test]
    fn rejected_confirmation_keeps_the_flow_open_and_route_unchanged() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task })
            .unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            )
            .unwrap();
        let before = state.clone();
        let step = state.apply_confirmation(
            Confirmation::Rejected("retry".into()),
            NavCommand::CloseTop,
            &store,
            scroll(0.0),
        );
        assert_eq!(step, ConfirmationStep::Rejected("retry".into()));
        assert_eq!(state, before);
    }

    #[test]
    fn checked_construction_rejects_a_missing_owner() {
        let store = base_store();
        let err = NavState::new(&store, Route::research_task(404)).unwrap_err();
        assert!(matches!(err, NavReject::MissingOwner { kind: "task", .. }));
        assert!(NavState::new(&store, list_route()).is_ok());
    }

    #[test]
    fn impact_review_must_match_the_editor_task() {
        let mut store = base_store();
        let task_a = task(&mut store, "A");
        let task_b = task(&mut store, "B");
        let mut state = NavState::new(&store, Route::research_task(task_a)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task: task_a })
            .unwrap();
        let before = state.clone();
        let err = state
            .push_overlay(&store, Overlay::ResearchImpactReview { task: task_b })
            .unwrap_err();
        assert_eq!(err, NavReject::InvalidStack);
        assert_eq!(state, before, "mismatched impact owner must not mutate state");

        // With an intervening decision, both owners must still match.
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task_a),
                },
            )
            .unwrap();
        let before = state.clone();
        let err = state
            .push_overlay(&store, Overlay::ResearchImpactReview { task: task_b })
            .unwrap_err();
        assert_eq!(err, NavReject::InvalidStack);
        assert_eq!(state, before);
    }

    #[test]
    fn previous_versions_owner_must_match_the_base_page() {
        let mut store = base_store();
        let task_a = task(&mut store, "A");
        let task_b = task(&mut store, "B");
        let mut state = NavState::new(&store, Route::research_task(task_a)).unwrap();
        let before = state.clone();
        let err = state
            .push_overlay(
                &store,
                Overlay::PreviousVersions {
                    task: task_b,
                    run: None,
                },
            )
            .unwrap_err();
        assert_eq!(err, NavReject::InvalidStack);
        assert_eq!(state, before);

        let run_b = run(&mut store, task_b, "Succeeded", "B report");
        let before = state.clone();
        let err = state
            .push_overlay(
                &store,
                Overlay::PreviousVersions {
                    task: task_a,
                    run: Some(run_b),
                },
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::InvalidOwnership { .. }));
        assert_eq!(state, before);
    }

    #[test]
    fn results_filter_mutation_validates_owner_and_limit_atomically() {
        let mut store = base_store();
        let automation = automation(&mut store, "Filter owner");
        let mut state = NavState::new(&store, Route::automation_results(Some(automation), 20)).unwrap();
        let before = state.clone();
        let err = state
            .set_results_filter(&store, Some(404), 20)
            .unwrap_err();
        assert!(matches!(err, NavReject::MissingOwner { .. }));
        assert_eq!(state, before);

        let before = state.clone();
        let err = state.set_results_filter(&store, None, 0).unwrap_err();
        assert_eq!(
            err,
            NavReject::InvalidOwnership {
                reason: "results history limit must be positive"
            }
        );
        assert_eq!(state, before);

        state.set_results_filter(&store, None, 20).unwrap();
        match state.route() {
            Route::AutomationResults(page) => {
                assert_eq!(page.filter(), None);
                assert_eq!(page.history_limit(), 20);
            }
            other => panic!("expected automation results, got {other:?}"),
        }
    }

    #[test]
    fn confirmed_back_restores_an_unrelated_parent_from_settings_edit() {
        let mut store = base_store();
        let automation = automation(&mut store, "Settings parent");
        let mut state = NavState::new(&store, Route::automation_detail(automation)).unwrap();
        state
            .push_overlay(&store, Overlay::AutomationSettings { automation })
            .unwrap();
        state
            .enter(
                &store,
                Destination::Route(Route::automation_editor(Some(automation))),
                EnterMode::DrillDown,
                scroll(0.0),
            )
            .unwrap();
        assert_eq!(state.returns().len(), 1);
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Automation {
                        editing: Some(automation),
                    },
                },
            )
            .unwrap();
        let step = state.apply_confirmation(
            Confirmation::Confirmed,
            NavCommand::Back,
            &store,
            scroll(0.0),
        );
        assert!(matches!(step, ConfirmationStep::Advanced(_)));
        assert_eq!(state.route(), &Route::automation_detail(automation));
        assert_eq!(
            state.overlays(),
            &[Overlay::AutomationSettings { automation }]
        );
    }

    #[test]
    fn failed_confirmed_commit_preserves_the_completed_flow() {
        let mut store = base_store();
        let task_a = task(&mut store, "A");
        let task_b = task(&mut store, "B");
        let run_b = run(&mut store, task_b, "Succeeded", "B report");
        let mut state = NavState::new(&store, Route::research_task(task_a)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task: task_a })
            .unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task_a),
                },
            )
            .unwrap();
        state
            .push_overlay(
                &store,
                Overlay::ResearchImpactReview { task: task_a },
            )
            .unwrap();
        let before = state.clone();
        let step = state.apply_confirmation(
            Confirmation::Confirmed,
            NavCommand::Enter {
                dest: Destination::Route(result_route(ResultOwner::Task(task_a), Some(run_b))),
                mode: EnterMode::Replace,
            },
            &store,
            scroll(0.0),
        );
        assert!(matches!(step, ConfirmationStep::Rejected(_)));
        assert_eq!(state, before, "failed commit must preserve the whole flow");
    }

    #[test]
    fn nested_search_and_impact_return_to_the_preceding_surface_on_close() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state
            .push_overlay(&store, Overlay::TaskEditor { task })
            .unwrap();
        state.push_overlay(&store, Overlay::Search).unwrap();
        state
            .push_overlay(
                &store,
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            )
            .unwrap();
        state
            .push_overlay(&store, Overlay::ResearchImpactReview { task })
            .unwrap();

        assert_eq!(
            state.close_top(&store).unwrap(),
            NavEffect::Closed {
                overlay: Overlay::ResearchImpactReview { task }
            }
        );
        assert_eq!(
            state.top_overlay(),
            Some(Overlay::DraftDecision {
                editor: EditorTarget::Task(task)
            })
        );
        assert_eq!(
            state.close_top(&store).unwrap(),
            NavEffect::Closed {
                overlay: Overlay::DraftDecision {
                    editor: EditorTarget::Task(task)
                }
            }
        );
        assert_eq!(state.top_overlay(), Some(Overlay::Search));
        assert_eq!(
            state.close_top(&store).unwrap(),
            NavEffect::Closed {
                overlay: Overlay::Search
            }
        );
        assert_eq!(state.top_overlay(), Some(Overlay::TaskEditor { task }));
        assert_eq!(state.route(), &Route::research_task(task));
    }

    #[test]
    fn overlay_grammar_allows_the_full_editor_decision_impact_search_flow() {
        let task = 1;
        let base = Route::research_task(task);
        let legal = vec![
            vec![],
            vec![Overlay::TaskEditor { task }],
            vec![Overlay::TaskEditor { task }, Overlay::Search],
            vec![
                Overlay::TaskEditor { task },
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            ],
            vec![
                Overlay::TaskEditor { task },
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
                Overlay::ResearchImpactReview { task },
            ],
            vec![
                Overlay::TaskEditor { task },
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
                Overlay::ResearchImpactReview { task },
                Overlay::Search,
            ],
            vec![Overlay::PreviousVersions { task, run: None }],
            vec![
                Overlay::PreviousVersions { task, run: None },
                Overlay::Search,
            ],
            vec![Overlay::Search],
        ];
        for stack in &legal {
            assert!(
                validate_overlay_stack(&base, stack).is_ok(),
                "expected legal stack {stack:?}"
            );
        }
        let illegal = vec![
            vec![Overlay::ResearchImpactReview { task }],
            vec![
                Overlay::TaskEditor { task },
                Overlay::ResearchImpactReview { task: task + 1 },
            ],
            vec![Overlay::Search, Overlay::TaskEditor { task }],
            vec![Overlay::PreviousVersions {
                task: task + 1,
                run: None,
            }],
            vec![
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
                Overlay::ResearchImpactReview { task },
            ],
            vec![
                Overlay::TaskEditor { task },
                Overlay::ResearchImpactReview { task },
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            ],
            vec![
                Overlay::PreviousVersions { task, run: None },
                Overlay::TaskEditor { task },
            ],
            vec![
                Overlay::PreviousVersions { task, run: None },
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            ],
        ];
        for stack in &illegal {
            assert!(
                validate_overlay_stack(&base, stack).is_err(),
                "expected illegal stack {stack:?}"
            );
        }
        assert!(MAX_OVERLAY_DEPTH >= 4);
    }

    #[test]
    fn overlay_grammar_is_exhaustive_over_representative_layers() {
        let task = 1;
        let base = Route::research_task(task);
        let candidates = [
            ("editor", Overlay::TaskEditor { task }),
            (
                "decision",
                Overlay::DraftDecision {
                    editor: EditorTarget::Task(task),
                },
            ),
            ("impact", Overlay::ResearchImpactReview { task }),
            ("search", Overlay::Search),
            ("previous", Overlay::PreviousVersions { task, run: None }),
        ];
        fn sequences(
            prefix: &mut Vec<usize>,
            used: &mut [bool; 5],
            out: &mut Vec<Vec<usize>>,
        ) {
            out.push(prefix.clone());
            if prefix.len() == 4 {
                return;
            }
            for index in 0..5 {
                if used[index] {
                    continue;
                }
                used[index] = true;
                prefix.push(index);
                sequences(prefix, used, out);
                prefix.pop();
                used[index] = false;
            }
        }
        let mut all = vec![];
        sequences(&mut Vec::new(), &mut [false; 5], &mut all);
        let mut observed = std::collections::BTreeSet::new();
        for sequence in all {
            let overlays: Vec<Overlay> = sequence
                .iter()
                .map(|index| candidates[*index].1)
                .collect();
            if validate_overlay_stack(&base, &overlays).is_ok() {
                observed.insert(
                    sequence
                        .iter()
                        .map(|index| candidates[*index].0)
                        .collect::<Vec<_>>(),
                );
            }
        }
        let expected: std::collections::BTreeSet<Vec<&str>> = [
            vec![],
            vec!["search"],
            vec!["editor"],
            vec!["editor", "search"],
            vec!["previous"],
            vec!["previous", "search"],
            vec!["editor", "decision"],
            vec!["editor", "search", "decision"],
            vec!["editor", "decision", "search"],
            vec!["editor", "impact"],
            vec!["editor", "search", "impact"],
            vec!["editor", "impact", "search"],
            vec!["editor", "decision", "impact"],
            vec!["editor", "search", "decision", "impact"],
            vec!["editor", "decision", "search", "impact"],
            vec!["editor", "decision", "impact", "search"],
        ]
        .into_iter()
        .collect();
        assert_eq!(observed, expected);
    }

    #[test]
    fn data_scope_has_no_runs_alias_inside_research_list() {
        let store = base_store();
        let list = Route::research_list(
            ResearchScope::All,
            AllStatus::All,
            Layout::List,
        );
        assert_eq!(list.data_scope(&store), DataScope::All);
        let results = Route::research_result(ResultOwner::OrdinaryPage, None);
        assert_eq!(results.data_scope(&store), DataScope::Results);
    }

    #[test]
    fn result_page_rejects_deleted_owner_without_mutation() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let owner = ResultOwner::Task(task);
        let mut state = NavState::new(&store, result_route(owner, None)).unwrap();
        store.tasks.retain(|value| value.id != task);
        let before = state.clone();
        let err = state
            .enter(
                &store,
                Destination::Route(result_route(owner, None)),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::MissingOwner { .. }));
        assert_eq!(state, before);
    }

    #[test]
    fn planning_progress_requires_the_named_request() {
        let mut store = base_store();
        let task = task(&mut store, "Planner");
        let run = run(&mut store, task, "Succeeded", "plan");
        let owner = ResultOwner::PlanningProgress { request: 404 };
        let mut state = NavState::new(&store, list_route()).unwrap();
        let before = state.clone();
        let err = state
            .enter(
                &store,
                Destination::Route(result_route(owner, Some(run))),
                EnterMode::Replace,
                scroll(0.0),
            )
            .unwrap_err();
        assert!(matches!(err, NavReject::MissingOwner { .. }));
        assert_eq!(state, before);
    }

    #[test]
    fn back_is_blocked_while_a_modal_is_active() {
        let mut store = base_store();
        let task = task(&mut store, "Task");
        let mut state = NavState::new(&store, Route::research_task(task)).unwrap();
        state.push_overlay(&store, Overlay::TaskEditor { task }).unwrap();
        assert_eq!(state.back_availability(&store), BackAvailability::BlockedByOverlay);
        assert_eq!(state.back(&store).unwrap_err(), NavReject::BlockedByOverlay);
    }

    #[test]
    fn return_stack_is_bounded_and_deduplicates_consecutive_frames() {
        let mut stack = ReturnStack::new();
        let frame = ReturnFrame {
            route: list_route(),
            overlays: vec![],
            scroll: ScrollBookmark::default(),
            result_fallback: None,
        };
        stack.record(frame.clone());
        stack.record(frame.clone());
        assert_eq!(stack.len(), 1, "consecutive duplicates collapse");
        for index in 0..(MAX_RETURN_DEPTH + 3) {
            stack.record(ReturnFrame {
                route: Route::research_list(
                    ResearchScope::Cycle(format!("cycle-{index}")),
                    AllStatus::All,
                    Layout::List,
                ),
                overlays: vec![],
                scroll: ScrollBookmark::default(),
                result_fallback: None,
            });
        }
        assert_eq!(stack.len(), MAX_RETURN_DEPTH);
        assert_eq!(
            stack.last().map(|frame| frame.route.clone()),
            Some(Route::research_list(
                ResearchScope::Cycle(format!("cycle-{}", MAX_RETURN_DEPTH + 2)),
                AllStatus::All,
                Layout::List,
            )),
            "the bound retains the newest frames in order"
        );
    }

    #[test]
    fn visible_projects_are_not_required_for_navigation_validation() {
        let mut store = base_store();
        store.projects.push(Project {
            id: 2,
            name: "Second".into(),
            key: "P2".into(),
            directory: std::env::temp_dir().to_string_lossy().into_owned(),
            research_brief: String::new(),
            kind: ProjectKind::User,
        });
        let mut state = NavState::new(&store, list_route()).unwrap();
        state
            .enter(
                &store,
                Destination::Route(Route::research_list(
                    ResearchScope::Project(2),
                    AllStatus::All,
                    Layout::Board,
                )),
                EnterMode::Structural,
                scroll(0.0),
            )
            .unwrap();
        assert_eq!(state.data_scope(&store), DataScope::Project(2));
    }
}
