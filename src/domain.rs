//! Persisted workspace model; subprocess execution lives in the OpenCode adapter.
use serde::{Deserialize, Serialize};

pub fn activity_event(message: &str) -> String {
    format!("{message} · {}", crate::opencode::timestamp())
}

/// Old versions stored a relative label without recording an event time.
/// Only creation events can recover their time from the task itself.
pub fn activity_display(event: &str, created_at: &str) -> String {
    if let Some(message) = event
        .strip_suffix(" · Just now")
        .or_else(|| event.strip_suffix(" · just now"))
    {
        let time = if message == "Task created" && !created_at.is_empty() {
            created_at
        } else {
            "Time not recorded"
        };
        return format!("{message} · {time}");
    }
    event.to_string()
}

#[cfg(any(test, feature = "qa"))]
#[path = "fixtures.rs"]
mod fixtures;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Backlog,
    Planned,
    InProgress,
    Review,
    Done,
    Cancelled,
}
impl Stage {
    /// Board columns follow workflow progression, with completion at the far right.
    pub const BOARD_ORDER: [Self; 6] = [
        Self::Backlog, Self::Planned, Self::InProgress,
        Self::Review, Self::Cancelled, Self::Done,
    ];
    pub const ALL: [Self; 6] = [
        Self::Backlog,
        Self::Planned,
        Self::InProgress,
        Self::Review,
        Self::Done,
        Self::Cancelled,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::Planned => "Todo",
            Self::InProgress => "In progress",
            Self::Review => "In review",
            Self::Done => "Done",
            Self::Cancelled => "Cancelled",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Planned => "todo",
            Self::InProgress => "in-progress",
            Self::Review => "review",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
    fn matches(self, value: &str) -> bool {
        let value = value.to_lowercase().replace([' ', '_'], "-");
        self.key() == value
            || self.label().to_lowercase().replace(' ', "-") == value
            || matches!(
                (self, value.as_str()),
                (Self::Planned, "planned")
                    | (Self::InProgress, "doing")
                    | (Self::InProgress, "inprogress")
                    | (Self::Review, "in-review")
                    | (Self::Cancelled, "canceled")
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    None,
    Low,
    Medium,
    High,
    Urgent,
}
impl Priority {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Urgent,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No priority",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Urgent => "Urgent",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}
const LEGACY_APPROVAL_MARKER: &str = "legacy-approval-needs-migration";
fn legacy_approved_plan() -> Option<String> {
    Some(LEGACY_APPROVAL_MARKER.into())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    #[serde(default)]
    pub created_at: String,
    /// Internal research entity owned by an automation occurrence, never a normal task.
    #[serde(default)]
    pub automation_id: Option<u64>,
    pub title: String,
    pub description: String,
    pub project_id: u64,
    #[serde(default)]
    pub topic_id: Option<u64>,
    #[serde(default = "legacy_approved_plan")]
    pub approved_plan: Option<String>,
    pub stage: Stage,
    pub priority: Priority,
    pub labels: Vec<String>,
    pub cycle: Option<String>,
    pub bucket: String,
    pub due_date: String,
    pub parent_id: Option<u64>,
    pub comments: Vec<Comment>,
    pub activity: Vec<String>,
    pub archived: bool,
    #[serde(default)]
    pub attachments: Vec<crate::attachments::Attachment>,
    #[serde(default)]
    pub research: Option<crate::research::ResearchTask>,
}
/// User projects are the only ones shown in navigation. The automation project is
/// a generated container for automation output and is never offered as a choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    #[default]
    User,
    Automation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub research_brief: String,
    #[serde(default)]
    pub kind: ProjectKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    #[serde(default)]
    pub replanned_from: Option<u64>,
    /// Result topics created for automations are excluded from normal research views.
    #[serde(default)]
    pub automation_id: Option<u64>,
    pub id: u64,
    pub project_id: u64,
    pub name: String,
    pub archived: bool,
}
impl Topic {
    pub fn display_name(&self) -> String {
        format!("#{}", self.name.trim_start_matches('#'))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StagePolicy {
    pub stage: Stage,
    pub agent: String,
    pub model: String,
    pub prompt: String,
    pub auto_start: bool,
}
/// Immutable owner of a planning attempt. The request revision is fixed when the
/// run is prepared; a request's latest `planning_run_id` is only a display cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningOwner {
    pub request_id: u64,
    pub revision: u32,
    #[serde(default)]
    pub automation_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRun {
    /// Automation occurrence that owns this run, when it is not a normal research run.
    #[serde(default)]
    pub automation_occurrence_id: Option<u64>,
    /// Immutable request/revision owner for planning attempts.
    #[serde(default)]
    pub planning_owner: Option<PlanningOwner>,
    /// Independent deadline for a post-period report regeneration attempt.
    #[serde(default)]
    pub deadline_at: Option<i64>,
    /// Automatic retries in this user-started attempt chain (initial run is zero).
    #[serde(default)]
    pub auto_retry_attempt: u8,
    #[serde(default)]
    pub retry_not_before: Option<i64>,
    /// Project planning is not a task run; legacy task_id is unused (zero) in this case.
    #[serde(default)]
    pub project_plan: Option<u64>,
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub task_title: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub queued_at: String,
    /// Legacy workspace to snapshot before dispatch; cleared only after successful migration.
    #[serde(default)]
    pub workspace_source: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub task_id: u64,
    pub agent: String,
    pub model: String,
    pub stage: Stage,
    pub status: String,
    pub logs: Vec<String>,
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub attachments: Vec<crate::attachments::Attachment>,
    #[serde(default)]
    pub research_kind: Option<crate::research::ResearchKind>,
    #[serde(default)]
    pub sources: Vec<crate::research::Source>,
    #[serde(default)]
    pub input_signature: String,
}
#[path = "domain_tracked.rs"]
pub mod tracked;
pub use tracked::{TrackedList, TrackedMut, TrackedScalar, TrackedScalarMut, TrackedVec};

fn automation_tracked() -> TrackedVec<crate::automation::AutomationDefinition> {
    tracked::automation_new()
}

fn occurrence_tracked() -> TrackedVec<crate::automation::AutomationOccurrence> {
    tracked::occurrence_new()
}

fn run_tracked() -> TrackedVec<AgentRun> {
    tracked::run_new()
}

fn task_tracked() -> TrackedVec<Task> {
    tracked::task_new()
}

fn project_tracked() -> TrackedVec<Project> {
    tracked::project_new()
}

fn topic_tracked() -> TrackedVec<Topic> {
    tracked::topic_new()
}

fn request_tracked() -> TrackedVec<crate::request_planning::ResearchRequest> {
    tracked::request_new()
}

fn policy_tracked() -> TrackedVec<StagePolicy> {
    tracked::policy_new()
}

fn publication_tracked() -> TrackedList<crate::report_publish::Publication> {
    TrackedList::new()
}

fn model_profile_tracked() -> TrackedList<crate::model_profiles::ModelProfile> {
    TrackedList::new()
}

fn default_automation_schema_version() -> TrackedScalar<u32> {
    TrackedScalar::new(crate::automation::AUTOMATION_SCHEMA_VERSION)
}

fn default_concurrent_runs() -> TrackedScalar<usize> {
    TrackedScalar::new(crate::scheduler::default_concurrent_runs())
}

fn default_concurrent_automations() -> TrackedScalar<usize> {
    TrackedScalar::new(crate::scheduler::default_concurrent_automations())
}

fn deserialize_tasks<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<Task>, D::Error> {
    let items = Vec::<Task>::deserialize(deserializer)?;
    tracked::task_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_projects<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<Project>, D::Error> {
    let items = Vec::<Project>::deserialize(deserializer)?;
    tracked::project_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_topics<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<Topic>, D::Error> {
    let items = Vec::<Topic>::deserialize(deserializer)?;
    tracked::topic_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_requests<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<crate::request_planning::ResearchRequest>, D::Error> {
    let items = Vec::<crate::request_planning::ResearchRequest>::deserialize(deserializer)?;
    tracked::request_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_policies<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<StagePolicy>, D::Error> {
    let items = Vec::<StagePolicy>::deserialize(deserializer)?;
    tracked::policy_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_publications<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedList<crate::report_publish::Publication>, D::Error> {
    let items = Vec::<crate::report_publish::Publication>::deserialize(deserializer)?;
    Ok(TrackedList::from_vec(items))
}

fn deserialize_model_profiles<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedList<crate::model_profiles::ModelProfile>, D::Error> {
    let items = Vec::<crate::model_profiles::ModelProfile>::deserialize(deserializer)?;
    Ok(TrackedList::from_vec(items))
}

fn deserialize_automations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<crate::automation::AutomationDefinition>, D::Error> {
    let items = Vec::<crate::automation::AutomationDefinition>::deserialize(deserializer)?;
    tracked::automation_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_occurrences<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<crate::automation::AutomationOccurrence>, D::Error> {
    let items = Vec::<crate::automation::AutomationOccurrence>::deserialize(deserializer)?;
    tracked::occurrence_from(items).map_err(serde::de::Error::custom)
}

fn deserialize_runs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TrackedVec<AgentRun>, D::Error> {
    let items = Vec::<AgentRun>::deserialize(deserializer)?;
    tracked::run_from(items).map_err(serde::de::Error::custom)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "default_automation_schema_version")]
    automation_schema_version: TrackedScalar<u32>,
    #[serde(default = "automation_tracked", deserialize_with = "deserialize_automations")]
    automations: TrackedVec<crate::automation::AutomationDefinition>,
    #[serde(default = "occurrence_tracked", deserialize_with = "deserialize_occurrences")]
    automation_occurrences: TrackedVec<crate::automation::AutomationOccurrence>,
    #[serde(default)]
    report_publishing: TrackedScalar<crate::report_publish::PublishingConfig>,
    #[serde(default)]
    recovery_snapshot: TrackedScalar<String>,
    /// Runs whose report body exists on disk but was not hydrated at startup.
    /// This is process-local read state, never a persisted domain change.
    #[serde(skip)]
    lazy_bodies: std::collections::BTreeSet<u64>,
    #[serde(default = "publication_tracked", deserialize_with = "deserialize_publications")]
    publications: TrackedList<crate::report_publish::Publication>,
    #[serde(default = "task_tracked", deserialize_with = "deserialize_tasks")]
    tasks: TrackedVec<Task>,
    #[serde(default = "project_tracked", deserialize_with = "deserialize_projects")]
    projects: TrackedVec<Project>,
    #[serde(default = "topic_tracked", deserialize_with = "deserialize_topics")]
    topics: TrackedVec<Topic>,
    #[serde(default = "request_tracked", deserialize_with = "deserialize_requests")]
    research_requests: TrackedVec<crate::request_planning::ResearchRequest>,
    #[serde(default = "policy_tracked", deserialize_with = "deserialize_policies")]
    policies: TrackedVec<StagePolicy>,
    #[serde(default = "run_tracked", deserialize_with = "deserialize_runs")]
    runs: TrackedVec<AgentRun>,
    next_id: TrackedScalar<u64>,
    /// Next unreserved run id. Resolution may reserve an id asynchronously.
    #[serde(default)]
    run_id_floor: TrackedScalar<u64>,
    #[serde(default)]
    opencode_path: TrackedScalar<String>,
    #[serde(default)]
    default_model: TrackedScalar<String>,
    #[serde(default = "model_profile_tracked", deserialize_with = "deserialize_model_profiles")]
    model_profiles: TrackedList<crate::model_profiles::ModelProfile>,
    #[serde(default)]
    active_model_profile: TrackedScalar<Option<String>>,
    #[serde(default = "default_concurrent_runs")]
    max_concurrent_runs: TrackedScalar<usize>,
    /// Independent pool for running automation periods. Research tasks never
    /// consume these slots and automation periods never consume research slots.
    #[serde(default = "default_concurrent_automations")]
    max_concurrent_automations: TrackedScalar<usize>,
    #[serde(default)]
    research_config: TrackedScalar<crate::research::ResearchConfig>,
    #[serde(default)]
    appearance: TrackedScalar<crate::appearance::Appearance>,
    #[serde(default)]
    liquid_glass: TrackedScalar<bool>,
    #[serde(default)]
    last_active_project: TrackedScalar<Option<u64>>,
}

pub use crate::request_planning::ResearchRequest;

/// One staged save group and the exact dirty epoch it was drained at.
/// Retiring the group only clears tracking when no newer write arrived.
pub(crate) struct StoreDelta {
    pub changes: Vec<crate::persistence_journal::EntityChange>,
    pub metadata: Vec<crate::persistence_journal::MetadataDelta>,
    pub epoch: u64,
}

impl Store {
    // ---- Read-only access. Persistent fields are private: mutations must go
    // through the tracked methods below or a typed setter. Clean hydration is
    // restricted to `load_*`, serde and the explicit empty constructor.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }
    pub fn topics(&self) -> &[Topic] {
        &self.topics
    }
    pub fn research_requests(&self) -> &[crate::request_planning::ResearchRequest] {
        &self.research_requests
    }
    pub fn policies(&self) -> &[StagePolicy] {
        &self.policies
    }
    pub fn publications(&self) -> &[crate::report_publish::Publication] {
        &self.publications
    }
    pub fn model_profiles(&self) -> &[crate::model_profiles::ModelProfile] {
        &self.model_profiles
    }
    pub fn next_id(&self) -> u64 {
        *self.next_id.get()
    }
    pub fn run_id_floor(&self) -> u64 {
        *self.run_id_floor.get()
    }
    pub fn opencode_path(&self) -> &String {
        self.opencode_path.get()
    }
    pub fn default_model(&self) -> &String {
        self.default_model.get()
    }
    pub fn active_model_profile(&self) -> &Option<String> {
        self.active_model_profile.get()
    }
    pub fn max_concurrent_runs(&self) -> usize {
        *self.max_concurrent_runs.get()
    }
    pub fn max_concurrent_automations(&self) -> usize {
        *self.max_concurrent_automations.get()
    }
    pub fn research_config(&self) -> &crate::research::ResearchConfig {
        self.research_config.get()
    }
    pub fn liquid_glass(&self) -> bool {
        *self.liquid_glass.get()
    }

    pub fn appearance(&self) -> crate::appearance::Appearance {
        *self.appearance.get()
    }
    pub fn last_active_project(&self) -> Option<u64> {
        *self.last_active_project.get()
    }
    pub fn automation_schema_version(&self) -> u32 {
        *self.automation_schema_version.get()
    }
    pub fn report_publishing(&self) -> &crate::report_publish::PublishingConfig {
        self.report_publishing.get()
    }
    pub fn recovery_snapshot(&self) -> &String {
        self.recovery_snapshot.get()
    }
    pub fn set_recovery_snapshot(&mut self, value: String) {
        self.recovery_snapshot.set(value);
    }

    /// Every human-facing project picker and navigation list uses this view.
    pub fn visible_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects
            .iter()
            .filter(|project| project.kind == ProjectKind::User)
    }
    pub fn automation_project(&self) -> Option<&Project> {
        self.projects
            .iter()
            .find(|project| project.kind == ProjectKind::Automation)
    }
    /// Automation output is stored under one generated project so it never mixes
    /// with manually created projects and topics.
    pub fn ensure_automation_project(&mut self, directory: String) -> Result<u64, String> {
        if let Some(project) = self.automation_project() {
            return Ok(project.id);
        }
        let id = self
            .projects
            .iter()
            .map(|project| project.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("No project identifiers remain.")?;
        self.push_project(Project {
            id,
            name: "Automations".into(),
            key: format!("AUTO{id}"),
            directory,
            research_brief: String::new(),
            kind: ProjectKind::Automation,
        });
        Ok(id)
    }
    /// A non-empty stage override takes precedence over the workspace default.
    pub fn effective_model<'a>(&'a self, stage_override: &'a str) -> &'a str {
        if stage_override.is_empty() {
            self.default_model()
        } else {
            stage_override
        }
    }
    pub fn research_model(&self, kind: crate::research::ResearchKind) -> &str {
        let model = &self.research_config().models[kind.index()];
        if model.is_empty() && self.research_agent(kind) != self.research_config().default_agent {
            ""
        } else {
            self.effective_model(model)
        }
    }
    pub fn research_agent(&self, kind: crate::research::ResearchKind) -> &str {
        self.research_config().agents[kind.index()].as_deref()
            .unwrap_or(&self.research_config().default_agent)
    }
    pub fn agent_path(&self, agent: &str) -> &str {
        if agent == "Codex" { &self.research_config().codex_path } else { self.opencode_path() }
    }
    pub fn valid_research_model(&self, kind: crate::research::ResearchKind) -> bool {
        match self.research_agent(kind) {
            "Codex" => crate::codex::valid_model(self.research_model(kind)),
            "OpenCode" => crate::opencode::valid_model(self.research_model(kind)),
            _ => false,
        }
    }
    /// A new workspace contains configuration, never demonstration tasks or runs.
    pub fn empty() -> Self {
        let mut store = Self {
            automation_schema_version: TrackedScalar::new(
                crate::automation::AUTOMATION_SCHEMA_VERSION,
            ),
            automations: automation_tracked(),
            automation_occurrences: occurrence_tracked(),
            report_publishing: TrackedScalar::new(Default::default()),
            recovery_snapshot: TrackedScalar::new(String::new()),
            lazy_bodies: std::collections::BTreeSet::new(),
            publications: publication_tracked(),
            tasks: task_tracked(),
            projects: project_tracked(),
            topics: topic_tracked(),
            research_requests: request_tracked(),
            policies: policy_tracked(),
            runs: run_tracked(),
            next_id: TrackedScalar::new(1),
            run_id_floor: TrackedScalar::new(0),
            opencode_path: TrackedScalar::new(String::new()),
            default_model: TrackedScalar::new(String::new()),
            model_profiles: model_profile_tracked(),
            active_model_profile: TrackedScalar::new(None),
            max_concurrent_runs: TrackedScalar::new(crate::scheduler::DEFAULT_CONCURRENT_RUNS),
            max_concurrent_automations: TrackedScalar::new(
                crate::scheduler::DEFAULT_CONCURRENT_AUTOMATIONS,
            ),
            research_config: TrackedScalar::new(Default::default()),
            appearance: TrackedScalar::new(Default::default()),
            liquid_glass: TrackedScalar::new(false),
            last_active_project: TrackedScalar::new(None),
        };
        store.projects.push_tracked(Project {
            id: 1,
            name: "Workspace".into(),
            key: "TASK".into(),
            directory: String::new(),
            research_brief: String::new(),
            kind: ProjectKind::User,
        });
        store.projects.clear_dirty();
        store
    }
    /// Older workspaces acquire a topic without rewriting research content or results.
    pub fn normalize_topics(&mut self) {
        let roots: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.parent_id.is_none())
            .map(|t| (t.id, t.project_id, t.topic_id))
            .collect();
        for (id, project, topic) in roots {
            if !self
                .topics
                .iter()
                .any(|t| Some(t.id) == topic && t.project_id == project)
            {
                if let Ok(topic) = self.create_topic(project, self.task(id).unwrap().title.clone()) {
                    self.task_mut(id).unwrap().topic_id = Some(topic);
                }
            }
        }
        // Remove only unreferenced legacy placeholders. Never drop a populated topic.
        self.topics.retain_tracked(|topic| {
            !matches!(topic.name.as_str(), "待归类" | "未归类")
                || self.tasks.iter().any(|t| t.topic_id == Some(topic.id))
                || self.research_requests.iter().any(|r| r.topic_id == Some(topic.id))
        });
        // Missing fields in old files differ from explicit None in a current, unapproved plan.
        // Only migration from the old format or an unmanaged legacy Goal can establish approval.
        let goals: Vec<_> = self
            .tasks
            .iter()
            .filter_map(|t| {
                t.research
                    .as_ref()
                    .filter(|r| r.kind == crate::research::ResearchKind::Goal)
                    .map(|r| {
                        (
                            t.id,
                            !r.requires_approval,
                            t.approved_plan.as_deref() == Some(LEGACY_APPROVAL_MARKER),
                        )
                    })
            })
            .collect();
        for (id, unmanaged, legacy) in goals {
            self.task_mut(id)
                .unwrap()
                .research
                .as_mut()
                .unwrap()
                .requires_approval = true;
            if (unmanaged || legacy)
                && !matches!(
                    self.task(id).unwrap().stage,
                    Stage::Backlog | Stage::Cancelled
                )
            {
                let signature = self.goal_plan_signature(id);
                self.task_mut(id).unwrap().approved_plan = signature;
            } else if legacy {
                self.task_mut(id).unwrap().approved_plan = None;
            }
        }
        let legacy_tasks: Vec<u64> = self
            .tasks
            .iter()
            .filter(|task| task.approved_plan.as_deref() == Some(LEGACY_APPROVAL_MARKER))
            .map(|task| task.id)
            .collect();
        for id in legacy_tasks {
            if let Some(mut task) = self.task_mut(id) {
                task.approved_plan = None;
            }
        }
    }
    pub fn create_topic(&mut self, project: u64, name: impl Into<String>) -> Result<u64, String> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err("Enter a topic name.".into());
        }
        if !self.projects.iter().any(|p| p.id == project) {
            return Err("Choose an existing project.".into());
        }
        let id = self.topics.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.push_topic(Topic {
            replanned_from: None,
            automation_id: None,
            id,
            project_id: project,
            name,
            archived: false,
        });
        Ok(id)
    }
    pub fn rename_topic(&mut self, id: u64, name: impl Into<String>) -> Result<(), String> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err("Enter a topic name.".into());
        }
        let mut topic = self.topic_mut(id).ok_or("Topic not found.")?;
        topic.name = name;
        Ok(())
    }
    pub fn set_topic_archived(&mut self, id: u64, archived: bool) -> Result<(), String> {
        let mut topic = self.topic_mut(id).ok_or("Topic not found.")?;
        topic.archived = archived;
        Ok(())
    }
    pub fn set_goal_topic(&mut self, goal: u64, topic_id: Option<u64>) -> Result<(), String> {
        let task = self.task(goal).ok_or("Research goal not found.")?;
        if task
            .research
            .as_ref()
            .is_none_or(|r| r.kind != crate::research::ResearchKind::Goal)
        {
            return Err("Only research goals can belong to a topic.".into());
        }
        if task.archived {
            return Err("Restore the archived goal before changing its topic.".into());
        }
        if topic_id.is_none() {
            return Err("Choose a topic for this goal.".into());
        }
        if task.topic_id == topic_id {
            return Ok(());
        }
        if let Some(id) = topic_id {
            let topic = self
                .topics
                .iter()
                .find(|t| t.id == id)
                .ok_or("Topic not found.")?;
            if topic.archived || topic.project_id != task.project_id {
                return Err("Choose an active topic in the same project.".into());
            }
        }
        let task = self.task_mut(goal).unwrap();
        task.topic_id = topic_id;
        task.activity.push(activity_event("Topic changed"));
        Ok(())
    }
    pub fn set_root_topic(&mut self, id: u64, topic: u64) -> Result<(), String> {
        let task = self.task(id).ok_or("Task not found.")?;
        if task.parent_id.is_some() || task.archived {
            return Err("Choose an active root goal or task.".into());
        }
        if !self
            .topics
            .iter()
            .any(|t| t.id == topic && t.project_id == task.project_id && !t.archived)
        {
            return Err("Choose an active topic in the same project.".into());
        }
        self.task_mut(id).unwrap().topic_id = Some(topic);
        Ok(())
    }
    /// Research steps and ordinary descendants inherit their goal's topic.
    pub fn task_topic(&self, id: u64) -> Option<&Topic> {
        let mut task = self.task(id)?;
        let project = task.project_id;
        let mut visited = Vec::new();
        loop {
            if task.project_id != project || visited.contains(&task.id) {
                return None;
            }
            visited.push(task.id);
            if let Some(research) = &task.research {
                if research.kind == crate::research::ResearchKind::Goal {
                    return self.topics.iter().find(|topic| {
                        Some(topic.id) == task.topic_id && topic.project_id == project
                    });
                }
                task = self.task(research.goal_id)?;
            } else if let Some(parent) = task.parent_id {
                task = self.task(parent)?;
            } else {
                return self
                    .topics
                    .iter()
                    .find(|t| Some(t.id) == task.topic_id && t.project_id == project);
            }
        }
    }
    /// Archive visibility is inherited without changing descendants' saved flags.
    pub fn task_is_archived(&self, id: u64) -> bool {
        if self.task_topic(id).is_some_and(|topic| topic.archived) {
            return true;
        }
        let mut current = Some(id);
        let mut visited = Vec::new();
        while let Some(id) = current {
            if visited.contains(&id) { break; }
            visited.push(id);
            let Some(task) = self.task(id) else { break; };
            if task.archived { return true; }
            current = task.parent_id.or_else(|| task.research.as_ref()
                .filter(|r| r.goal_id != id).map(|r| r.goal_id));
        }
        false
    }
    pub fn request_is_archived(&self, request: &crate::request_planning::ResearchRequest) -> bool {
        self.topics.iter().any(|t| Some(t.id) == request.topic_id && t.archived)
            || (!request.goal_ids.is_empty()
                && request.goal_ids.iter().all(|id| self.task_is_archived(*id)))
    }
    pub fn run_is_archived(&self, run: &AgentRun) -> bool {
        if run.project_plan.is_some() {
            self.request_for_run(run.id).is_some_and(|r| self.request_is_archived(r))
        } else {
            self.task_is_archived(run.task_id)
        }
    }
    /// One ownership predicate for every result-selection surface. Automation
    /// planning attempts belong to their automation; ambiguous legacy attempts
    /// never become ordinary Research results by accident.
    pub fn run_is_automation_owned(&self, run: &AgentRun) -> bool {
        if run.automation_occurrence_id.is_some() {
            return true;
        }
        if run
            .planning_owner
            .as_ref()
            .is_some_and(|owner| owner.automation_id.is_some())
        {
            return true;
        }
        if self
            .request_for_run_record(run)
            .is_some_and(|request| request.automation_id.is_some())
        {
            return true;
        }
        if run.project_plan.is_some_and(|id| {
            self.projects
                .iter()
                .find(|project| project.id == id)
                .is_some_and(|project| project.kind == ProjectKind::Automation)
        }) {
            return true;
        }
        run.project_plan.is_none()
            && self
                .task(run.task_id)
                .is_some_and(|task| task.automation_id.is_some())
    }
    pub fn run_is_ordinary_result(&self, run: &AgentRun) -> bool {
        !self.run_is_automation_owned(run) && !self.run_is_archived(run)
    }
    pub fn latest_ordinary_result_run(&self) -> Option<&AgentRun> {
        self.runs.iter().rev().find(|run| self.run_is_ordinary_result(run))
    }
    /// Count tasks, not attempts: current work and saved completed results.
    pub fn progress_result_task_count(&self) -> usize {
        // Preserve the original semantics exactly: a task counts when its
        // newest qualifying run is in progress OR any qualifying run has a
        // validated non-empty output. Ownership/archive lookups no longer rescan
        // the whole run history, so the same 43,200-run task stays cheap.
        let mut latest = std::collections::HashSet::new();
        let mut included = std::collections::HashSet::new();
        let mut archived_by_task = std::collections::HashMap::new();
        for run in self.runs.iter().rev().filter(|run| {
            run.project_plan.is_none()
                && !self.run_is_automation_owned(run)
                && run.research_kind != Some(crate::research::ResearchKind::Goal)
                && self.task(run.task_id).is_some()
                && !*archived_by_task
                    .entry(run.task_id)
                    .or_insert_with(|| self.task_is_archived(run.task_id))
        }) {
            let is_latest = latest.insert(run.task_id);
            let in_progress = is_latest
                && matches!(
                    run.status.as_str(),
                    "Queued" | "Running" | "Recovering" | "Cancelling"
                );
            let has_result = run.status == "Succeeded" && !run.output.trim().is_empty();
            if in_progress || has_result {
                included.insert(run.task_id);
            }
        }
        included.len()
    }
    pub fn task(&self, id: u64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }
    pub fn task_mut(&mut self, id: u64) -> Option<&mut Task> {
        self.tasks
            .tracked_mut(crate::persistence_journal::EntityKey::Task(id))
    }

    /// O(1) task mutation for bounded commands that already built an id→index
    /// map in one pass. Used by scoped Undo so K deltas never rescan T tasks.
    pub(crate) fn task_at_index_mut(&mut self, index: usize) -> Option<&mut Task> {
        let id = self.tasks.get(index)?.id;
        self.tasks
            .tracked_mut_index(index, crate::persistence_journal::EntityKey::Task(id))
    }
    pub fn task_at_mut(&mut self, index: usize) -> Option<&mut Task> {
        let id = self.tasks.get(index)?.id;
        self.task_mut(id)
    }
    pub fn project_at_mut(&mut self, index: usize) -> Option<&mut Project> {
        let id = self.projects.get(index)?.id;
        self.project_mut(id)
    }
    pub fn topic_at_mut(&mut self, index: usize) -> Option<&mut Topic> {
        let id = self.topics.get(index)?.id;
        self.topic_mut(id)
    }
    pub fn request_at_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut crate::request_planning::ResearchRequest> {
        let id = self.research_requests.get(index)?.id;
        self.request_mut(id)
    }
    pub fn clear_tasks(&mut self) {
        self.tasks.clear_tracked();
    }
    pub fn clear_projects(&mut self) {
        self.projects.clear_tracked();
    }
    pub fn clear_topics(&mut self) {
        self.topics.clear_tracked();
    }
    pub fn clear_requests(&mut self) {
        self.research_requests.clear_tracked();
    }
    pub fn retain_tasks(&mut self, keep: impl FnMut(&Task) -> bool) {
        self.tasks.retain_tracked(keep);
    }
    pub fn reverse_tasks(&mut self) {
        self.tasks.reverse_tracked();
    }
    pub fn swap_tasks(&mut self, first: usize, second: usize) -> bool {
        self.tasks.swap_tracked(first, second)
    }
    pub fn push_task(&mut self, task: Task) {
        self.tasks.push_tracked(task);
    }
    pub fn remove_task(&mut self, id: u64) -> Option<Task> {
        self.tasks
            .remove_tracked(crate::persistence_journal::EntityKey::Task(id))
    }
    pub fn replace_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks.replace_tracked(tasks);
    }
    pub(in crate::domain) fn load_tasks(&mut self, tasks: Vec<Task>) -> Result<(), String> {
        self.tasks = tracked::task_from(tasks)?;
        Ok(())
    }

    pub fn project(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }
    pub fn project_mut(&mut self, id: u64) -> Option<&mut Project> {
        self.projects
            .tracked_mut(crate::persistence_journal::EntityKey::Project(id))
    }

    pub(crate) fn project_at_index_mut(&mut self, index: usize) -> Option<&mut Project> {
        let id = self.projects.get(index)?.id;
        self.projects
            .tracked_mut_index(index, crate::persistence_journal::EntityKey::Project(id))
    }
    pub fn push_project(&mut self, project: Project) {
        self.projects.push_tracked(project);
    }
    pub fn remove_project(&mut self, id: u64) -> Option<Project> {
        self.projects
            .remove_tracked(crate::persistence_journal::EntityKey::Project(id))
    }
    pub fn retain_projects(&mut self, keep: impl FnMut(&Project) -> bool) {
        self.projects.retain_tracked(keep);
    }
    pub fn replace_projects(&mut self, projects: Vec<Project>) {
        self.projects.replace_tracked(projects);
    }
    pub(in crate::domain) fn load_projects(&mut self, projects: Vec<Project>) -> Result<(), String> {
        self.projects = tracked::project_from(projects)?;
        Ok(())
    }

    pub fn topic(&self, id: u64) -> Option<&Topic> {
        self.topics.iter().find(|topic| topic.id == id)
    }
    pub fn topic_mut(&mut self, id: u64) -> Option<&mut Topic> {
        self.topics
            .tracked_mut(crate::persistence_journal::EntityKey::Topic(id))
    }

    pub(crate) fn topic_at_index_mut(&mut self, index: usize) -> Option<&mut Topic> {
        let id = self.topics.get(index)?.id;
        self.topics
            .tracked_mut_index(index, crate::persistence_journal::EntityKey::Topic(id))
    }
    pub fn push_topic(&mut self, topic: Topic) {
        self.topics.push_tracked(topic);
    }
    pub fn remove_topic(&mut self, id: u64) -> Option<Topic> {
        self.topics
            .remove_tracked(crate::persistence_journal::EntityKey::Topic(id))
    }
    pub fn retain_topics(&mut self, keep: impl FnMut(&Topic) -> bool) {
        self.topics.retain_tracked(keep);
    }
    pub fn replace_topics(&mut self, topics: Vec<Topic>) {
        self.topics.replace_tracked(topics);
    }
    pub(in crate::domain) fn load_topics(&mut self, topics: Vec<Topic>) -> Result<(), String> {
        self.topics = tracked::topic_from(topics)?;
        Ok(())
    }

    pub fn request_mut(
        &mut self,
        id: u64,
    ) -> Option<&mut crate::request_planning::ResearchRequest> {
        self.research_requests
            .tracked_mut(crate::persistence_journal::EntityKey::ResearchRequest(id))
    }

    pub(crate) fn request_at_index_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut crate::request_planning::ResearchRequest> {
        let id = self.research_requests.get(index)?.id;
        self.research_requests.tracked_mut_index(
            index,
            crate::persistence_journal::EntityKey::ResearchRequest(id),
        )
    }
    pub fn push_request(&mut self, request: crate::request_planning::ResearchRequest) {
        self.research_requests.push_tracked(request);
    }
    pub fn remove_request(&mut self, id: u64) -> Option<crate::request_planning::ResearchRequest> {
        self.research_requests
            .remove_tracked(crate::persistence_journal::EntityKey::ResearchRequest(id))
    }
    pub fn retain_requests(
        &mut self,
        keep: impl FnMut(&crate::request_planning::ResearchRequest) -> bool,
    ) {
        self.research_requests.retain_tracked(keep);
    }
    pub fn replace_requests(&mut self, requests: Vec<crate::request_planning::ResearchRequest>) {
        self.research_requests.replace_tracked(requests);
    }
    pub(in crate::domain) fn load_requests(
        &mut self,
        requests: Vec<crate::request_planning::ResearchRequest>,
    ) -> Result<(), String> {
        self.research_requests = tracked::request_from(requests)?;
        Ok(())
    }

    pub fn policy(&self, stage: Stage) -> Option<&StagePolicy> {
        self.policies.iter().find(|policy| policy.stage == stage)
    }
    pub fn remove_policy(&mut self, stage: Stage) -> Option<StagePolicy> {
        self.policies
            .remove_tracked(crate::persistence_journal::EntityKey::policy(stage))
    }
    pub fn replace_policy(&mut self, policy: StagePolicy) {
        if let Some(slot) = self.policy_mut(policy.stage) {
            *slot = policy;
        } else {
            self.push_policy(policy);
        }
    }
    pub fn policy_mut(&mut self, stage: Stage) -> Option<&mut StagePolicy> {
        self.policies
            .tracked_mut(crate::persistence_journal::EntityKey::policy(stage))
    }
    pub fn push_policy(&mut self, policy: StagePolicy) {
        self.policies.push_tracked(policy);
    }
    pub fn retain_policies(&mut self, keep: impl FnMut(&StagePolicy) -> bool) {
        self.policies.retain_tracked(keep);
    }
    pub fn replace_policies(&mut self, policies: Vec<StagePolicy>) {
        self.policies.replace_tracked(policies);
    }
    pub(in crate::domain) fn load_policies(&mut self, policies: Vec<StagePolicy>) -> Result<(), String> {
        self.policies = tracked::policy_from(policies)?;
        Ok(())
    }

    pub fn occurrence(&self, id: u64) -> Option<&crate::automation::AutomationOccurrence> {
        self.automation_occurrences
            .iter()
            .find(|occurrence| occurrence.id == id)
    }

    pub fn automation_mut(
        &mut self,
        id: u64,
    ) -> Option<&mut crate::automation::AutomationDefinition> {
        self.automations
            .tracked_mut(crate::persistence_journal::EntityKey::Automation(id))
    }

    pub fn occurrence_mut(
        &mut self,
        id: u64,
    ) -> Option<&mut crate::automation::AutomationOccurrence> {
        self.automation_occurrences
            .tracked_mut(crate::persistence_journal::EntityKey::Occurrence(id))
    }

    pub fn push_automation(&mut self, definition: crate::automation::AutomationDefinition) {
        self.automations.push_tracked(definition);
    }

    pub fn push_occurrence(&mut self, occurrence: crate::automation::AutomationOccurrence) {
        self.automation_occurrences.push_tracked(occurrence);
    }

    pub fn pop_automation(&mut self) -> Option<crate::automation::AutomationDefinition> {
        self.automations.pop_tracked()
    }

    pub fn pop_occurrence(&mut self) -> Option<crate::automation::AutomationOccurrence> {
        self.automation_occurrences.pop_tracked()
    }

    pub fn run_at_mut(&mut self, index: usize) -> Option<&mut AgentRun> {
        let id = self.runs.get(index)?.id;
        self.runs.tracked_mut(crate::persistence_journal::EntityKey::Run(id))
    }

    /// O(1) run mutation when the caller already holds an id→index map built in
    /// a single history pass. Used by bounded topic/project commands so the
    /// main thread does not rescan a large run history per affected run.
    pub(crate) fn run_at_index_mut(&mut self, index: usize) -> Option<&mut AgentRun> {
        let id = self.runs.get(index)?.id;
        self.runs
            .tracked_mut_index(index, crate::persistence_journal::EntityKey::Run(id))
    }

    pub fn automation_at_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut crate::automation::AutomationDefinition> {
        let id = self.automations.get(index)?.id;
        self.automations
            .tracked_mut(crate::persistence_journal::EntityKey::Automation(id))
    }

    pub fn occurrence_at_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut crate::automation::AutomationOccurrence> {
        let id = self.automation_occurrences.get(index)?.id;
        self.automation_occurrences
            .tracked_mut(crate::persistence_journal::EntityKey::Occurrence(id))
    }

    pub fn extend_runs(&mut self, runs: impl IntoIterator<Item = AgentRun>) {
        for run in runs {
            self.push_run(run);
        }
    }

    pub fn insert_run(&mut self, index: usize, run: AgentRun) {
        self.runs.insert_tracked(index, run);
    }

    pub fn push_run(&mut self, run: AgentRun) {
        self.runs.push_tracked(run);
    }

    pub fn pop_run(&mut self) -> Option<AgentRun> {
        self.runs.pop_tracked()
    }

    pub fn last_run_mut(&mut self) -> Option<&mut AgentRun> {
        let index = self.runs.len().checked_sub(1)?;
        self.run_at_mut(index)
    }

    pub fn last_occurrence_mut(
        &mut self,
    ) -> Option<&mut crate::automation::AutomationOccurrence> {
        let index = self.automation_occurrences.len().checked_sub(1)?;
        self.occurrence_at_mut(index)
    }

    pub fn last_automation_mut(
        &mut self,
    ) -> Option<&mut crate::automation::AutomationDefinition> {
        let index = self.automations.len().checked_sub(1)?;
        self.automation_at_mut(index)
    }

    pub fn run_mut(&mut self, id: u64) -> Option<&mut AgentRun> {
        self.runs
            .tracked_mut(crate::persistence_journal::EntityKey::Run(id))
    }

    /// Reserve one run id for an asynchronous admission. The reservation is
    /// persisted with the workspace so a concurrent create/run cannot reuse it.
    pub fn allocate_run_id(&mut self) -> Option<u64> {
        let max = self.runs.iter().map(|run| run.id).max().unwrap_or(0);
        let next = max.checked_add(1)?.max(*self.run_id_floor.get());
        self.set_run_id_floor(next.checked_add(1)?);
        Some(next)
    }

    pub fn remove_automation(&mut self, id: u64) -> Option<crate::automation::AutomationDefinition> {
        self.automations
            .remove_tracked(crate::persistence_journal::EntityKey::Automation(id))
    }

    pub fn remove_occurrence(&mut self, id: u64) -> Option<crate::automation::AutomationOccurrence> {
        self.automation_occurrences
            .remove_tracked(crate::persistence_journal::EntityKey::Occurrence(id))
    }

    pub fn remove_run(&mut self, id: u64) -> Option<AgentRun> {
        self.lazy_bodies.remove(&id);
        self.runs
            .remove_tracked(crate::persistence_journal::EntityKey::Run(id))
    }

    pub fn retain_automations(&mut self, keep: impl FnMut(&crate::automation::AutomationDefinition) -> bool) {
        self.automations.retain_tracked(keep);
    }

    pub fn retain_occurrences(&mut self, keep: impl FnMut(&crate::automation::AutomationOccurrence) -> bool) {
        self.automation_occurrences.retain_tracked(keep);
    }

    pub fn retain_runs(&mut self, keep: impl FnMut(&AgentRun) -> bool) {
        self.runs.retain_tracked(keep);
    }

    pub fn clear_automations(&mut self) {
        self.automations.clear_tracked();
    }

    pub fn clear_occurrences(&mut self) {
        self.automation_occurrences.clear_tracked();
    }

    pub fn clear_runs(&mut self) {
        self.runs.clear_tracked();
    }

    pub fn has_tracked_changes(&self) -> bool {
        self.automations.is_dirty()
            || self.automation_occurrences.is_dirty()
            || self.runs.is_dirty()
            || self.tasks.is_dirty()
            || self.projects.is_dirty()
            || self.topics.is_dirty()
            || self.research_requests.is_dirty()
            || self.policies.is_dirty()
            || self.publications.is_dirty()
            || self.model_profiles.is_dirty()
            || self.has_metadata_changes()
    }

    /// Read-only per-collection snapshot for diagnostics and tests. It never
    /// clears pending intent, so a failed later collection cannot hide or drop
    /// earlier payloads.
    pub(crate) fn pending_tracked_snapshot(
        &self,
    ) -> [Result<Vec<crate::persistence_journal::EntityChange>, String>; 3] {
        [
            self.automations.pending_changes(),
            self.automation_occurrences.pending_changes(),
            self.runs.pending_changes(),
        ]
    }

    /// Every pending entity change, in stable collection order. Building this
    /// never clears intent and never clones a whole collection.
    pub(crate) fn pending_entity_changes(
        &self,
    ) -> Result<Vec<crate::persistence_journal::EntityChange>, String> {
        let mut changes = Vec::new();
        changes.extend(self.tasks.pending_changes()?);
        changes.extend(self.projects.pending_changes()?);
        changes.extend(self.topics.pending_changes()?);
        changes.extend(self.research_requests.pending_changes()?);
        changes.extend(self.policies.pending_changes()?);
        changes.extend(self.runs.pending_changes()?);
        changes.extend(self.automations.pending_changes()?);
        changes.extend(self.automation_occurrences.pending_changes()?);
        Ok(changes)
    }

    /// Test-only convenience drain. Production uses `drain_store_delta` plus
    /// epoch retirement so a newer write is never cleared by an older ack.
    #[cfg(test)]
    pub fn drain_tracked_changes(
        &mut self,
    ) -> Result<Vec<crate::persistence_journal::EntityChange>, String> {
        let changes = self.pending_entity_changes()?;
        self.tasks.clear_dirty();
        self.projects.clear_dirty();
        self.topics.clear_dirty();
        self.research_requests.clear_dirty();
        self.policies.clear_dirty();
        self.runs.clear_dirty();
        self.automations.clear_dirty();
        self.automation_occurrences.clear_dirty();
        Ok(changes)
    }

    /// Pending typed metadata deltas for every tracked scalar/config field.
    /// The caller stages them in the same ordered group as entity changes.
    pub(crate) fn pending_metadata_deltas(&self) -> Vec<crate::persistence_journal::MetadataDelta> {
        use crate::persistence_journal::MetadataDelta;
        let mut deltas = Vec::new();
        if self.next_id.is_dirty() {
            deltas.push(MetadataDelta::NextId(*self.next_id.get()));
        }
        if self.run_id_floor.is_dirty() {
            deltas.push(MetadataDelta::RunIdFloor(*self.run_id_floor.get()));
        }
        if self.default_model.is_dirty() {
            deltas.push(MetadataDelta::DefaultModel(self.default_model.get().clone()));
        }
        if self.opencode_path.is_dirty() {
            deltas.push(MetadataDelta::OpenCodePath(self.opencode_path.get().clone()));
        }
        if self.liquid_glass.is_dirty() {
            deltas.push(MetadataDelta::LiquidGlass(*self.liquid_glass.get()));
        }
        if self.appearance.is_dirty() {
            deltas.push(MetadataDelta::Appearance(*self.appearance.get()));
        }
        if self.last_active_project.is_dirty() {
            deltas.push(MetadataDelta::LastActiveProject(*self.last_active_project.get()));
        }
        if self.automation_schema_version.is_dirty() {
            deltas.push(MetadataDelta::AutomationSchemaVersion(
                *self.automation_schema_version.get(),
            ));
        }
        if self.max_concurrent_runs.is_dirty() {
            deltas.push(MetadataDelta::MaxConcurrentRuns(*self.max_concurrent_runs.get()));
        }
        if self.max_concurrent_automations.is_dirty() {
            deltas.push(MetadataDelta::MaxConcurrentAutomations(
                *self.max_concurrent_automations.get(),
            ));
        }
        if self.research_config.is_dirty() {
            deltas.push(MetadataDelta::ResearchConfig(self.research_config.get().clone()));
        }
        if self.model_profiles.is_dirty() {
            deltas.push(MetadataDelta::ModelProfiles(self.model_profiles.to_vec()));
        }
        if self.active_model_profile.is_dirty() {
            deltas.push(MetadataDelta::ActiveModelProfile(
                self.active_model_profile.get().clone(),
            ));
        }
        if self.publications.is_dirty() {
            deltas.push(MetadataDelta::Publications(self.publications.to_vec()));
        }
        if self.report_publishing.is_dirty() {
            deltas.push(MetadataDelta::PublishingConfig(
                self.report_publishing.get().clone(),
            ));
        }
        if self.recovery_snapshot.is_dirty() {
            deltas.push(MetadataDelta::RecoverySnapshot(
                self.recovery_snapshot.get().clone(),
            ));
        }
        deltas
    }

    /// Apply one typed journal change to a durable-base replica. Used by the
    /// persistence worker; never for UI dirty discovery.
    pub(crate) fn apply_entity_change(&mut self, change: &crate::persistence_journal::EntityChange) {
        use crate::persistence_journal::{EntityChange, EntityKey, EntityPayload};
        match change {
            EntityChange::Delete(key) => match key {
                EntityKey::Task(id) => {
                    self.remove_task(*id);
                }
                EntityKey::Project(id) => {
                    self.remove_project(*id);
                }
                EntityKey::Topic(id) => {
                    self.remove_topic(*id);
                }
                EntityKey::ResearchRequest(id) => {
                    self.remove_request(*id);
                }
                EntityKey::Run(id) => {
                    self.remove_run(*id);
                }
                EntityKey::Policy(stage) => {
                    self.policies
                        .remove_tracked(crate::persistence_journal::EntityKey::Policy(*stage));
                }
                EntityKey::Automation(id) => {
                    self.remove_automation(*id);
                }
                EntityKey::Occurrence(id) => {
                    self.remove_occurrence(*id);
                }
            },
            EntityChange::PlanRequestMapping {
                request_id,
                revision,
                run_id,
            } => {
                let owner_valid = self.runs().iter().any(|run| {
                    run.id == *run_id
                        && run.planning_owner.as_ref().is_some_and(|owner| {
                            owner.request_id == *request_id && owner.revision == *revision
                        })
                });
                if owner_valid
                    && let Some(mut request) = self.request_mut(*request_id)
                    && request.revision == *revision
                    && request
                        .planning_run_id
                        .is_none_or(|current| *run_id >= current)
                {
                    request.planning_run_id = Some(*run_id);
                }
            }
            EntityChange::EnsureResearchRequest(request) => {
                match self
                    .research_requests()
                    .iter()
                    .find(|existing| existing.id == request.id)
                {
                    None => self.push_request(request.clone()),
                    Some(existing) if existing.revision < request.revision => {
                        if let Some(mut slot) = self.request_mut(request.id) {
                            *slot = request.clone();
                        }
                    }
                    Some(_) => {}
                }
            }
            EntityChange::Upsert(payload) => match payload {
                EntityPayload::Task(value) => {
                    if let Some(slot) = self.task_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_task(value.clone());
                    }
                }
                EntityPayload::Project(value) => {
                    if let Some(slot) = self.project_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_project(value.clone());
                    }
                }
                EntityPayload::Topic(value) => {
                    if let Some(slot) = self.topic_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_topic(value.clone());
                    }
                }
                EntityPayload::ResearchRequest(value) => {
                    if let Some(slot) = self.request_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_request(value.clone());
                    }
                }
                EntityPayload::Run(value) => {
                    if let Some(slot) = self.run_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_run(value.clone());
                    }
                }
                EntityPayload::Policy(value) => {
                    if self.policy(value.stage).is_some() {
                        self.replace_policy(value.clone());
                    } else {
                        self.push_policy(value.clone());
                    }
                }
                EntityPayload::Automation(value) => {
                    if let Some(slot) = self.automation_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_automation(value.clone());
                    }
                }
                EntityPayload::Occurrence(value) => {
                    if let Some(slot) = self.occurrence_mut(value.id) {
                        *slot = value.clone();
                    } else {
                        self.push_occurrence(value.clone());
                    }
                }
            },
        }
    }

    pub(crate) fn apply_metadata_delta(
        &mut self,
        delta: crate::persistence_journal::MetadataDelta,
    ) {
        self.apply_loaded_metadata(delta);
    }

    /// Merge one journal group into the live store while preserving mutation
    /// tracking. The form/command path applies the exact typed delta it already
    /// validated, then drains the live dirty set and submits that snapshot. An
    /// acknowledgement retires only the drained epoch, so a later edit in the
    /// same field is never cleared by this write.
    pub(crate) fn apply_live_journal_group(
        &mut self,
        changes: &[crate::persistence_journal::EntityChange],
        metadata: &[crate::persistence_journal::MetadataDelta],
    ) {
        for change in changes {
            self.apply_entity_change(change);
        }
        for delta in metadata {
            self.apply_live_metadata_delta(delta.clone());
        }
    }

    /// Tracked metadata write. Unlike `apply_loaded_metadata`, this marks the
    /// field dirty so the merged value is part of the next staged group.
    pub(crate) fn apply_live_metadata_delta(
        &mut self,
        delta: crate::persistence_journal::MetadataDelta,
    ) {
        use crate::persistence_journal::MetadataDelta;
        match delta {
            MetadataDelta::NextId(value) => self.set_next_id(value),
            MetadataDelta::RunIdFloor(value) => self.set_run_id_floor(value),
            MetadataDelta::DefaultModel(value) => self.set_default_model(value),
            MetadataDelta::OpenCodePath(value) => self.set_opencode_path(value),
            MetadataDelta::Appearance(value) => self.set_appearance(value),
            MetadataDelta::LiquidGlass(value) => self.set_liquid_glass(value),
            MetadataDelta::LastActiveProject(value) => self.set_last_active_project(value),
            MetadataDelta::AutomationSchemaVersion(value) => {
                self.set_automation_schema_version(value)
            }
            MetadataDelta::MaxConcurrentRuns(value) => self.set_max_concurrent_runs(value),
            MetadataDelta::MaxConcurrentAutomations(value) => {
                self.set_max_concurrent_automations(value)
            }
            MetadataDelta::ResearchConfig(value) => self.set_research_config(value),
            MetadataDelta::ModelProfiles(value) => self.model_profiles.replace_tracked(value),
            MetadataDelta::ActiveModelProfile(value) => self.set_active_model_profile(value),
            MetadataDelta::Topics(value) => self.replace_topics(value),
            MetadataDelta::ResearchRequests(value) => self.replace_requests(value),
            MetadataDelta::Publications(value) => self.replace_publications(value),
            MetadataDelta::PublishingConfig(value) => self.set_report_publishing(value),
            MetadataDelta::RecoverySnapshot(value) => self.set_recovery_snapshot(value),
        }
    }

    pub(crate) fn dirty_epoch(&self) -> u64 {
        let mut epoch = self.automations.epoch()
            .saturating_add(self.automation_occurrences.epoch())
            .saturating_add(self.runs.epoch())
            .saturating_add(self.tasks.epoch())
            .saturating_add(self.projects.epoch())
            .saturating_add(self.topics.epoch())
            .saturating_add(self.research_requests.epoch())
            .saturating_add(self.policies.epoch())
            .saturating_add(self.publications.epoch())
            .saturating_add(self.model_profiles.epoch());
        for scalar in [
            self.next_id.epoch(),
            self.run_id_floor.epoch(),
            self.default_model.epoch(),
            self.opencode_path.epoch(),
            self.appearance.epoch(),
            self.liquid_glass.epoch(),
            self.last_active_project.epoch(),
            self.automation_schema_version.epoch(),
            self.max_concurrent_runs.epoch(),
            self.max_concurrent_automations.epoch(),
            self.research_config.epoch(),
            self.active_model_profile.epoch(),
            self.report_publishing.epoch(),
            self.recovery_snapshot.epoch(),
        ] {
            epoch = epoch.saturating_add(scalar);
        }
        epoch
    }

    /// Snapshot the next staged group without clearing tracking. The caller
    /// resolves the returned group through the ordered journal; retirement is a
    /// separate step so a newer write is never discarded by an older ack.
    /// Consume a store whose exact snapshot was just written durably. Only the
    /// consumed copy loses its tracking, so a caller cannot clear unrelated
    /// pending changes on a different live store.
    pub(crate) fn into_durably_saved(mut self) -> Store {
        self.clear_all_tracking();
        self
    }

    pub(crate) fn drain_store_delta(&self) -> Result<StoreDelta, String> {
        Ok(StoreDelta {
            changes: self.pending_entity_changes()?,
            metadata: self.pending_metadata_deltas(),
            epoch: self.dirty_epoch(),
        })
    }

    /// Retire exactly the drained generation. If any newer mutation happened,
    /// nothing is cleared and the newer group is saved later.
    pub(crate) fn retire_store_delta(&mut self, delta: &StoreDelta) {
        self.retire_epoch(delta.epoch);
    }

    /// Retire tracking only when no newer write arrived after `epoch`.
    pub(crate) fn retire_epoch(&mut self, epoch: u64) {
        if self.dirty_epoch() == epoch {
            self.clear_all_tracking();
        }
    }

    /// Install a report body fetched from durable storage without creating a
    /// dirty write. The body already exists on disk; only the UI copy was lazy.
    pub(crate) fn mark_lazy_body(&mut self, run_id: u64) {
        self.lazy_bodies.insert(run_id);
    }

    /// True while a Succeeded run is known to have a durable body that has not
    /// been fetched into this UI copy yet.
    pub(crate) fn run_body_is_lazy(&self, run_id: u64) -> bool {
        self.lazy_bodies.contains(&run_id)
    }

    pub(crate) fn clear_lazy_body(&mut self, run_id: u64) {
        self.lazy_bodies.remove(&run_id);
    }

    pub(crate) fn hydrate_run_body(&mut self, run_id: u64, body: String) -> bool {
        let Some(index) = self.runs().iter().position(|run| run.id == run_id) else {
            return false;
        };
        self.lazy_bodies.remove(&run_id);
        self.runs.set_clean(index, |run| run.output = body)
    }

    pub(crate) fn drain_metadata_deltas(
        &mut self,
    ) -> Vec<crate::persistence_journal::MetadataDelta> {
        let deltas = self.pending_metadata_deltas();
        self.clear_metadata_dirty();
        deltas
    }

    pub(crate) fn has_metadata_changes(&self) -> bool {
        self.next_id.is_dirty()
            || self.run_id_floor.is_dirty()
            || self.default_model.is_dirty()
            || self.opencode_path.is_dirty()
            || self.appearance.is_dirty()
            || self.liquid_glass.is_dirty()
            || self.last_active_project.is_dirty()
            || self.automation_schema_version.is_dirty()
            || self.max_concurrent_runs.is_dirty()
            || self.max_concurrent_automations.is_dirty()
            || self.research_config.is_dirty()
            || self.model_profiles.is_dirty()
            || self.active_model_profile.is_dirty()
            || self.publications.is_dirty()
            || self.report_publishing.is_dirty()
            || self.recovery_snapshot.is_dirty()
    }

    pub(crate) fn clear_metadata_dirty(&mut self) {
        self.next_id.clear_dirty();
        self.run_id_floor.clear_dirty();
        self.default_model.clear_dirty();
        self.opencode_path.clear_dirty();
        self.appearance.clear_dirty();
        self.liquid_glass.clear_dirty();
        self.last_active_project.clear_dirty();
        self.automation_schema_version.clear_dirty();
        self.max_concurrent_runs.clear_dirty();
        self.max_concurrent_automations.clear_dirty();
        self.research_config.clear_dirty();
        self.model_profiles.clear_dirty();
        self.active_model_profile.clear_dirty();
        self.publications.clear_dirty();
        self.report_publishing.clear_dirty();
        self.recovery_snapshot.clear_dirty();
    }

    /// Acknowledged durable write: every pending entity and metadata change is
    /// now part of the durable baseline.
    pub(in crate::domain) fn clear_all_tracking(&mut self) {
        self.tasks.clear_dirty();
        self.projects.clear_dirty();
        self.topics.clear_dirty();
        self.research_requests.clear_dirty();
        self.policies.clear_dirty();
        self.runs.clear_dirty();
        self.automations.clear_dirty();
        self.automation_occurrences.clear_dirty();
        self.clear_metadata_dirty();
    }

    // ---- Typed scalar setters. These are the only production write paths. ----

    pub fn set_automation_schema_version(&mut self, value: u32) {
        self.automation_schema_version.set(value);
    }
    pub fn set_next_id(&mut self, value: u64) {
        self.next_id.set(value);
    }
    pub fn set_run_id_floor(&mut self, value: u64) {
        self.run_id_floor.set(value);
    }
    pub fn set_opencode_path(&mut self, value: String) {
        self.opencode_path.set(value);
    }
    pub fn set_default_model(&mut self, value: String) {
        self.default_model.set(value);
    }
    pub fn set_active_model_profile(&mut self, value: Option<String>) {
        self.active_model_profile.set(value);
    }
    pub fn set_max_concurrent_runs(&mut self, value: usize) {
        self.max_concurrent_runs.set(value);
    }
    pub fn set_max_concurrent_automations(&mut self, value: usize) {
        self.max_concurrent_automations.set(value);
    }
    pub fn set_research_config(&mut self, value: crate::research::ResearchConfig) {
        self.research_config.set(value);
    }
    pub fn set_liquid_glass(&mut self, value: bool) {
        self.liquid_glass.set(value);
    }

    pub fn set_appearance(&mut self, value: crate::appearance::Appearance) {
        self.appearance.set(value);
    }
    pub fn set_last_active_project(&mut self, value: Option<u64>) {
        self.last_active_project.set(value);
    }
    pub fn set_report_publishing(&mut self, value: crate::report_publish::PublishingConfig) {
        self.report_publishing.set(value);
    }

    pub fn research_config_mut(&mut self) -> &mut crate::research::ResearchConfig {
        &mut self.research_config
    }
    pub fn appearance_mut(&mut self) -> &mut crate::appearance::Appearance {
        &mut self.appearance
    }
    pub fn report_publishing_mut(
        &mut self,
    ) -> &mut crate::report_publish::PublishingConfig {
        &mut self.report_publishing
    }

    pub fn push_publication(&mut self, publication: crate::report_publish::Publication) {
        self.publications.push_tracked(publication);
    }
    pub fn publication_mut(
        &mut self,
        id: &str,
    ) -> Option<&mut crate::report_publish::Publication> {
        let index = self
            .publications
            .iter()
            .position(|publication| publication.id == id)?;
        self.publications.index_mut(index)
    }
    pub fn remove_publication(&mut self, id: &str) -> Option<crate::report_publish::Publication> {
        let index = self
            .publications
            .iter()
            .position(|publication| publication.id == id)?;
        self.publications.remove_tracked(index)
    }
    pub fn retain_publications(
        &mut self,
        keep: impl FnMut(&crate::report_publish::Publication) -> bool,
    ) {
        self.publications.retain_tracked(keep);
    }
    pub fn replace_publications(
        &mut self,
        publications: Vec<crate::report_publish::Publication>,
    ) {
        self.publications.replace_tracked(publications);
    }
    pub(in crate::domain) fn load_publications(
        &mut self,
        publications: Vec<crate::report_publish::Publication>,
    ) {
        self.publications.load(publications);
    }

    pub fn push_model_profile(&mut self, profile: crate::model_profiles::ModelProfile) {
        self.model_profiles.push_tracked(profile);
    }
    pub fn model_profile_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut crate::model_profiles::ModelProfile> {
        let index = self
            .model_profiles
            .iter()
            .position(|profile| profile.name == name)?;
        self.model_profiles.index_mut(index)
    }
    pub fn retain_model_profiles(
        &mut self,
        keep: impl FnMut(&crate::model_profiles::ModelProfile) -> bool,
    ) {
        self.model_profiles.retain_tracked(keep);
    }
    pub fn replace_model_profiles(
        &mut self,
        profiles: Vec<crate::model_profiles::ModelProfile>,
    ) {
        self.model_profiles.replace_tracked(profiles);
    }
    pub(in crate::domain) fn load_model_profiles(&mut self, profiles: Vec<crate::model_profiles::ModelProfile>) {
        self.model_profiles.load(profiles);
    }

    pub fn automations(&self) -> &[crate::automation::AutomationDefinition] {
        self.automations.as_slice()
    }

    pub fn all_occurrences(&self) -> &[crate::automation::AutomationOccurrence] {
        self.automation_occurrences.as_slice()
    }

    pub fn runs(&self) -> &[AgentRun] {
        self.runs.as_slice()
    }

    /// Finish loading a fresh store. Only a pristine builder is accepted, so a
    /// populated or dirty live store cannot silently reset tracked state. The
    /// store is returned with the error so the caller keeps ownership.
    /// Capability for clean hydration. The loader owns a brand-new empty store
    /// and cannot be pointed at live state, so ordinary command paths cannot use
    /// it to replace populated collections without dirty tracking.
    pub(crate) fn begin_load() -> StoreLoader {
        StoreLoader {
            store: Store::empty(),
        }
    }

    pub(crate) fn apply_loaded_metadata(
        &mut self,
        delta: crate::persistence_journal::MetadataDelta,
    ) {
        use crate::persistence_journal::MetadataDelta;
        match delta {
            MetadataDelta::NextId(value) => self.next_id.load(value),
            MetadataDelta::RunIdFloor(value) => self.run_id_floor.load(value),
            MetadataDelta::DefaultModel(value) => self.default_model.load(value),
            MetadataDelta::OpenCodePath(value) => self.opencode_path.load(value),
            MetadataDelta::Appearance(value) => self.appearance.load(value),
            MetadataDelta::LiquidGlass(value) => self.liquid_glass.load(value),
            MetadataDelta::LastActiveProject(value) => self.last_active_project.load(value),
            MetadataDelta::AutomationSchemaVersion(value) => {
                self.automation_schema_version.load(value)
            }
            MetadataDelta::MaxConcurrentRuns(value) => self.max_concurrent_runs.load(value),
            MetadataDelta::MaxConcurrentAutomations(value) => {
                self.max_concurrent_automations.load(value)
            }
            MetadataDelta::ResearchConfig(value) => self.research_config.load(value),
            MetadataDelta::ModelProfiles(value) => {
                self.model_profiles.load(value);
            }
            MetadataDelta::ActiveModelProfile(value) => self.active_model_profile.load(value),
            MetadataDelta::Topics(value) => {
                self.topics = tracked::topic_from(value).expect("validated loaded topics")
            }
            MetadataDelta::ResearchRequests(value) => {
                self.research_requests =
                    tracked::request_from(value).expect("validated loaded requests")
            }
            MetadataDelta::Publications(value) => self.publications.load(value),
            MetadataDelta::PublishingConfig(value) => self.report_publishing.load(value),
            MetadataDelta::RecoverySnapshot(value) => self.recovery_snapshot.load(value),
        }
    }

    pub(crate) fn with_loaded_tracked(
        mut self,
        runs: Vec<AgentRun>,
        automations: Vec<crate::automation::AutomationDefinition>,
        occurrences: Vec<crate::automation::AutomationOccurrence>,
    ) -> Result<Self, (Self, String)> {
        if !self.runs.is_pristine()
            || !self.automations.is_pristine()
            || !self.automation_occurrences.is_pristine()
        {
            return Err((
                self,
                "clean load requires a fresh empty store with no tracked changes".into(),
            ));
        }
        self.runs = match tracked::run_from(runs) {
            Ok(value) => value,
            Err(error) => return Err((self, error)),
        };
        self.automations = match tracked::automation_from(automations) {
            Ok(value) => value,
            Err(error) => return Err((self, error)),
        };
        self.automation_occurrences = match tracked::occurrence_from(occurrences) {
            Ok(value) => value,
            Err(error) => return Err((self, error)),
        };
        Ok(self)
    }

    pub fn identifier(&self, task: &Task) -> String {
        let kind = task
            .research
            .as_ref()
            .map(|r| match r.kind {
                crate::research::ResearchKind::Goal => "G",
                crate::research::ResearchKind::Collect => "E",
                crate::research::ResearchKind::Analyze => "A",
                crate::research::ResearchKind::Advise => "R",
            })
            .unwrap_or("");
        if let Some(research) = &task.research
            && research.kind != crate::research::ResearchKind::Goal
            && research.goal_id != task.id
        {
            return format!(
                "{}-G{}-{kind}{}",
                self.project_key(task),
                research.goal_id,
                task.id
            );
        }
        format!("{}-{kind}{}", self.project_key(task), task.id)
    }
    fn project_key<'a>(&'a self, task: &Task) -> &'a str {
        self.projects
            .iter()
            .find(|p| p.id == task.project_id)
            .map(|p| p.key.as_str())
            .unwrap_or("TASK")
    }
    pub fn create_task(
        &mut self,
        title: impl Into<String>,
        project_id: u64,
        stage: Stage,
        parent_id: Option<u64>,
    ) -> Result<u64, String> {
        self.create_task_in_topic(title, project_id, stage, parent_id, None)
    }
    pub fn create_task_in_topic(
        &mut self,
        title: impl Into<String>,
        project_id: u64,
        stage: Stage,
        parent_id: Option<u64>,
        topic: Option<u64>,
    ) -> Result<u64, String> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err("Enter a task title.".into());
        }
        if !self.projects.iter().any(|p| p.id == project_id) {
            return Err("Choose an existing project.".into());
        }
        if let Some(parent_id) = parent_id {
            let parent = self
                .task(parent_id)
                .ok_or("The parent task no longer exists.")?;
            if parent.archived || parent.project_id != project_id {
                return Err("Choose an active parent in the same project.".into());
            }
        }
        let topic_id = if parent_id.is_none() {
            Some(match topic {
                Some(id) => {
                    if !self.topics.iter().any(|t| t.id == id && t.project_id == project_id && !t.archived) {
                        return Err("Choose an active topic in the same project.".into());
                    }
                    id
                }
                None => self.create_topic(project_id, title.clone())?,
            })
        } else {
            None
        };
        Ok(self.insert_task(title, project_id, stage, parent_id, topic_id))
    }
    /// Root tasks that belong to no topic. Automation quick tasks use this so a
    /// small recurring check never creates a topic per period.
    pub fn create_root_task_without_topic(
        &mut self,
        title: impl Into<String>,
        project_id: u64,
        stage: Stage,
    ) -> Result<u64, String> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err("Enter a task title.".into());
        }
        if !self.projects.iter().any(|p| p.id == project_id) {
            return Err("Choose an existing project.".into());
        }
        Ok(self.insert_task(title, project_id, stage, None, None))
    }
    fn insert_task(
        &mut self,
        title: String,
        project_id: u64,
        stage: Stage,
        parent_id: Option<u64>,
        topic_id: Option<u64>,
    ) -> u64 {
        let id = *self.next_id.get();
        self.set_next_id(id.saturating_add(1));
        self.push_task(Task {
            id,
            created_at: crate::opencode::timestamp(),
            automation_id: None,
            title,
            description: String::new(),
            project_id,
            topic_id,
            approved_plan: None,
            stage,
            priority: Priority::Medium,
            labels: vec![],
            cycle: None,
            bucket: "Now".into(),
            due_date: String::new(),
            parent_id,
            comments: vec![],
            activity: vec![activity_event("Task created")],
            archived: false,
            attachments: vec![],
            research: None,
        });
        id
    }
    /// A display-name edit preserves planning snapshots that are still valid.
    /// Do not refresh snapshots already invalidated by scope or destination changes.
    pub fn rename_project(&mut self, project: u64, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 160 {
            return Err("Enter a project name between 1 and 160 characters.".into());
        }
        let current = self.projects.iter().find(|p| p.id == project).ok_or("Project not found.")?;
        if current.name == name { return Ok(()); }
        let valid: Vec<_> = self.runs.iter().filter(|r| r.project_plan == Some(project)).filter_map(|r| {
            let request = self.request_for_run(r.id).map(|q| q.id);
            let signature = match request {
                Some(id) => self.request_plan_signature(id),
                None => self.project_plan_signature(project),
            };
            (signature.as_ref().ok() == Some(&r.input_signature)).then_some((r.id, request))
        }).collect();
        self.project_mut(project).unwrap().name = name.into();
        for (run, request) in valid {
            let signature = match request {
                Some(id) => self.request_plan_signature(id),
                None => self.project_plan_signature(project),
            }.expect("Renaming a project cannot invalidate its destination");
            self.run_mut(run).unwrap().input_signature = signature;
        }
        Ok(())
    }

    /// Shared preflight for the confirmation dialog and the final mutation guard.
    pub fn topic_move_blockers(&self, topic: u64, batch: Option<&crate::batch::TodoBatch>) -> Vec<String> {
        let ids: Vec<_> = self.tasks.iter().filter(|t| self.task_topic(t.id).is_some_and(|t| t.id == topic)).map(|t| t.id).collect();
        let id_set: std::collections::HashSet<u64> = ids.iter().copied().collect();
        let requests: Vec<_> = self.research_requests.iter().filter(|r|
            r.topic_id == Some(topic) || r.goal_ids.iter().any(|id| id_set.contains(id))).collect();
        let requests_by_run: std::collections::HashMap<u64, &crate::request_planning::ResearchRequest> =
            requests
                .iter()
                .filter_map(|request| request.planning_run_id.map(|run| (run, *request)))
                .collect();
        let task_name = |id| self.task(id).map(|t| format!("{} · {}", self.identifier(t), t.title)).unwrap_or_else(|| format!("Task #{id}"));
        let mut blockers = vec![];
        for run in &self.runs {
            let request = requests_by_run.get(&run.id).copied();
            if !(run.project_plan.is_none() && id_set.contains(&run.task_id) || request.is_some()) { continue; }
            let action = match run.status.as_str() {
                "Queued" => "Open Progress and use Stop task to cancel this queued attempt or retry. If its goal is in the batch queue, also use Stop all, or wait for the queue to finish.",
                "Running" => "Wait for completion, or open Progress and use Stop task. Wait until stopping finishes before moving.",
                "Recovering" => "Wait for recovery to finish, then finish or stop the task. To abandon recovery, archive the topic and confirm Stop and archive; wait for stopping to finish, then restore it if needed.",
                "Cancelling" => "A stop has already been requested. Wait until the status changes to Stopped; do not submit another stop.",
                "Interrupted" if self.is_recovery_candidate(run) => "Resume from Paused projects and finish the work, or archive the topic and confirm Stop and archive to abandon it. Wait for stopping to finish, then restore the topic if needed.",
                _ => continue,
            };
            let name = request.map(|r| format!("Research request #{} · {} (planning)", r.id, r.input))
                .unwrap_or_else(|| task_name(run.task_id));
            blockers.push(format!("{name} — {} (run #{})\n  {}", run.status, run.id, action));
        }
        if let Some(batch) = batch.filter(|b| b.state != crate::batch::BatchState::Finished) {
            for id in &batch.items {
                if !id_set.contains(id) || !batch.contains_pending(*id) { continue; }
                let state = if matches!(batch.state, crate::batch::BatchState::Paused(_)) { "Paused batch queue" } else { "Pending batch queue" };
                let children: Vec<_> = self.tasks.iter().filter(|t| t.parent_id == Some(*id) && !t.stage.is_closed() && self.research_result(t.id).is_none()).map(|t| task_name(t.id)).collect();
                let children = if children.is_empty() { String::new() } else { format!("\n  Pending child tasks: {}", children.join("; ")) };
                blockers.push(format!("{} — {state}{children}\n  Finish the queue, or use Stop all in the queue bar. Stop all affects the entire batch, including other topics; pausing alone does not release it.", task_name(*id)));
            }
        }
        for request in requests {
            if request.goal_ids.iter().any(|id| !id_set.contains(id)) || request.topic_id.is_some_and(|id| id != topic) {
                let goals: Vec<_> = request.goal_ids.iter().map(|id| task_name(*id)).collect();
                blockers.push(format!("Research request #{} · {} — links multiple topics\n  Linked goals: {}. Reconcile the request's topic and goal membership before moving; stopping tasks does not repair this relationship.", request.id, request.input, goals.join("; ")));
            }
        }
        for task in &self.tasks {
            let inside = id_set.contains(&task.id);
            let links = task.parent_id.into_iter().chain(task.research.iter().flat_map(|r|
                std::iter::once(r.goal_id).chain(r.depends_on.iter().copied())));
            let mut seen = std::collections::HashSet::new();
            for id in links.filter(|id| inside != id_set.contains(id)) {
                if seen.insert(id) {
                    blockers.push(format!("{} → {} — cross-topic relationship\n  Reconcile the parent, goal or dependency relationship so it stays inside the topic. Stopping tasks does not repair this relationship.", task_name(task.id), task_name(id)));
                }
            }
        }
        blockers
    }

    /// Move a topic atomically while preserving stable IDs and existing result provenance.
    /// Goal-plan signature after a topic move, using the destination project for
    /// moved goals. Mirrors `research::goal_plan_signature`.
    fn goal_plan_signature_after(
        &self,
        id: u64,
        moved: &std::collections::HashSet<u64>,
        new_project: u64,
    ) -> Option<String> {
        let goal = self.task(id)?;
        if goal.research.as_ref()?.kind != crate::research::ResearchKind::Goal {
            return None;
        }
        let mut tasks: Vec<_> = self
            .tasks()
            .iter()
            .filter(|task| task.id != id && self.owning_research_goal(task.id) == Some(id))
            .collect();
        tasks.sort_by_key(|task| task.id);
        let content: Vec<_> = tasks
            .iter()
            .map(|task| {
                serde_json::json!([
                    task.id,
                    task.title,
                    task.description,
                    task.comments,
                    crate::attachments::signature_payload(&task.attachments),
                    task.parent_id,
                    task.archived,
                    task.research.as_ref().map(|r| (r.kind, &r.depends_on)),
                ])
            })
            .collect();
        let project = if moved.contains(&id) {
            new_project
        } else {
            goal.project_id
        };
        serde_json::to_string(&(
            &goal.title,
            &goal.description,
            &goal.comments,
            crate::attachments::legacy_signature_payload(&goal.attachments),
            project,
            content,
        ))
        .ok()
    }

    /// Prospective research signature after the topic's tasks move to
    /// `new_project`. Used as a preflight; no store field is mutated.
    fn research_signature_after_move(
        &self,
        id: u64,
        moved: &std::collections::HashSet<u64>,
        new_project: u64,
        approved: &mut std::collections::HashMap<u64, Option<String>>,
        latest_results: &std::collections::HashMap<u64, (u64, String)>,
        memo: &mut std::collections::HashMap<u64, Option<String>>,
        visiting: &mut Vec<u64>,
    ) -> Option<String> {
        if let Some(cached) = memo.get(&id) {
            return cached.clone();
        }
        if visiting.contains(&id) {
            return None;
        }
        if let Some(goal) = self.owning_research_goal(id) {
            let signature = approved.entry(goal).or_insert_with(|| {
                self.goal_plan_signature_after(goal, moved, new_project)
            });
            let approved_now = self.task(goal).is_some_and(|task| {
                let Some(research) = &task.research else {
                    return false;
                };
                if research.kind != crate::research::ResearchKind::Goal
                    || self.task_is_archived(goal)
                    || matches!(task.stage, Stage::Backlog | Stage::Cancelled)
                {
                    return false;
                }
                // The move rewrites an approved moved goal's approved_plan to
                // the prospective signature; compare that projection, not the
                // old persisted value.
                let approved_after = if moved.contains(&goal) {
                    signature.clone()
                } else {
                    task.approved_plan.clone()
                };
                !research.requires_approval || approved_after.as_ref() == signature.as_ref()
            });
            if !approved_now {
                memo.insert(id, None);
                return None;
            }
        }
        let task = self
            .task(id)
            .filter(|task| !task.archived && task.stage != Stage::Cancelled)?;
        let research = task.research.as_ref()?;
        let mut parents = vec![];
        let mut cursor = Some(id);
        let mut ancestors = vec![];
        while let Some(current) = cursor {
            if ancestors.contains(&current) {
                memo.insert(id, None);
                return None;
            }
            ancestors.push(current);
            let ancestor = self
                .task(current)
                .filter(|task| !task.archived && task.stage != Stage::Cancelled)?;
            let project = if moved.contains(&ancestor.id) {
                new_project
            } else {
                ancestor.project_id
            };
            parents.push(serde_json::json!([
                ancestor.id,
                ancestor.title,
                ancestor.description,
                ancestor.comments,
                crate::attachments::signature_payload(&ancestor.attachments),
                project,
            ]));
            cursor = ancestor.parent_id;
        }
        let mut dependencies = vec![];
        for dependency in &research.depends_on {
            let signature = {
                visiting.push(id);
                let value = self.research_signature_after_move(
                    *dependency,
                    moved,
                    new_project,
                    approved,
                    latest_results,
                    memo,
                    visiting,
                );
                visiting.pop();
                value?
            };
            let (run_id, run_signature) = latest_results.get(dependency)?;
            if !moved.contains(dependency) && run_signature != &signature {
                memo.insert(id, None);
                return None;
            }
            dependencies.push(*run_id);
        }
        let project = if moved.contains(&task.id) {
            new_project
        } else {
            task.project_id
        };
        let directory = &self
            .projects()
            .iter()
            .find(|candidate| candidate.id == project)?
            .directory;
        let signature = serde_json::to_string(&(
            parents,
            dependencies,
            directory,
            research.kind,
            research.goal_id,
        ))
        .ok();
        memo.insert(id, signature.clone());
        signature
    }

    /// Cycle check for the dependency subgraph touched by a topic move. Called
    /// before any write so a malformed graph is rejected without mutation.
    fn dependency_cycle(&self, ids: &[u64]) -> bool {
        fn visit(
            store: &Store,
            id: u64,
            ids: &[u64],
            state: &mut std::collections::HashMap<u64, u8>,
        ) -> bool {
            match state.get(&id).copied() {
                Some(1) => return true,
                Some(2) => return false,
                _ => {}
            }
            state.insert(id, 1);
            if let Some(task) = store.task(id)
                && let Some(research) = &task.research
            {
                for dependency in &research.depends_on {
                    if ids.contains(dependency) && visit(store, *dependency, ids, state) {
                        return true;
                    }
                }
            }
            state.insert(id, 2);
            false
        }
        let mut state = std::collections::HashMap::new();
        ids.iter().any(|id| visit(self, *id, ids, &mut state))
    }

    pub fn move_topic_project(&mut self, topic: u64, project: u64) -> Result<(), String> {
        let source = self.topics.iter().find(|t| t.id == topic).ok_or("Topic not found.")?.project_id;
        if !self.projects.iter().any(|p| p.id == project) { return Err("Project not found.".into()); }
        if source == project { return Ok(()); }
        let ids: Vec<_> = self.tasks.iter().filter(|t| self.task_topic(t.id).is_some_and(|t| t.id == topic)).map(|t| t.id).collect();
        let id_set: std::collections::HashSet<u64> = ids.iter().copied().collect();
        let requests: Vec<_> = self.research_requests.iter().filter(|r|
            r.topic_id == Some(topic) || r.goal_ids.iter().any(|id| id_set.contains(id))).map(|r| r.id).collect();
        let request_set: std::collections::HashSet<u64> = requests.iter().copied().collect();
        let blockers = self.topic_move_blockers(topic, None);
        if !blockers.is_empty() { return Err(blockers.join("\n\n")); }
        let planning_runs: std::collections::HashSet<u64> = self.research_requests.iter()
            .filter(|r| request_set.contains(&r.id))
            .filter_map(|r| r.planning_run_id)
            .collect();
        let approved: Vec<_> = ids.iter().copied().filter(|id| self.goal_execution_approved(*id)).collect();
        // One pass over run history builds both the id→index map used for O(1)
        // writes and the latest reusable result per task used by reusable-result
        // validation and the prospective signature walk. No per-id rescans.
        let mut run_index = std::collections::HashMap::with_capacity(self.runs.len());
        let mut latest_results: std::collections::HashMap<u64, (u64, String)> =
            std::collections::HashMap::new();
        let mut seen_tasks = std::collections::HashSet::new();
        for (index, run) in self.runs.iter().enumerate().rev() {
            run_index.insert(run.id, index);
            if run.status == "Simulated" || !seen_tasks.insert(run.task_id) {
                continue;
            }
            if run.status == "Succeeded" && !run.output.trim().is_empty() {
                latest_results.insert(run.task_id, (run.id, run.input_signature.clone()));
            }
        }
        // Reusable set from the same precomputed maps instead of one history
        // scan per task.
        let mut current_memo = std::collections::HashMap::new();
        let mut current_approved = std::collections::HashMap::new();
        let empty_moved = std::collections::HashSet::new();
        let valid: Vec<_> = ids
            .iter()
            .filter_map(|id| {
                let signature = self.research_signature_after_move(
                    *id,
                    &empty_moved,
                    0,
                    &mut current_approved,
                    &latest_results,
                    &mut current_memo,
                    &mut Vec::new(),
                )?;
                let (run, run_signature) = latest_results.get(id)?;
                (run_signature == &signature).then_some((*id, *run))
            })
            .collect();
        // Compute every refreshed signature before writing. A malformed graph
        // is rejected while the store and tracking state are still untouched.
        let moved: std::collections::HashSet<u64> = ids.iter().copied().collect();
        let mut approved_signatures: std::collections::HashMap<u64, Option<String>> =
            std::collections::HashMap::new();
        for goal in &approved {
            approved_signatures.insert(
                *goal,
                self.goal_plan_signature_after(*goal, &moved, project),
            );
        }
        let mut memo = std::collections::HashMap::new();
        let mut signatures = std::collections::HashMap::new();
        for (task, run) in &valid {
            let signature = self
                .research_signature_after_move(
                    *task,
                    &moved,
                    project,
                    &mut approved_signatures,
                    &latest_results,
                    &mut memo,
                    &mut Vec::new(),
                )
                .ok_or("Could not preserve research dependencies during migration.")?;
            let index = *run_index
                .get(run)
                .ok_or("A result run is missing from the run index.")?;
            signatures.insert(*run, (index, signature));
        }
        if self.dependency_cycle(&ids) {
            return Err("Could not preserve research dependencies during migration.".into());
        }

        self.topic_mut(topic).unwrap().project_id = project;
        for id in ids.iter().copied() {
            let mut task = self.task_mut(id).unwrap();
            task.project_id = project;
            task.activity.push(activity_event("Topic moved to another project"));
        }
        for goal in approved {
            self.task_mut(goal).unwrap().approved_plan =
                approved_signatures.get(&goal).cloned().flatten();
        }
        for id in requests.iter().copied() {
            let mut request = self.request_mut(id).unwrap();
            request.project_id = project;
            request.topic_id = Some(topic);
        }
        for run_id in &planning_runs {
            if let Some(index) = run_index.get(run_id)
                && let Some(mut run) = self.run_at_index_mut(*index)
            {
                run.project_plan = Some(project);
            }
        }
        // Refresh only results that were reusable before the move. Historical
        // directories, sessions, reports and attachment paths remain intact.
        for (_, (index, signature)) in signatures {
            if let Some(mut run) = self.run_at_index_mut(index) {
                run.input_signature = signature;
            }
        }
        Ok(())
    }

    /// Move the complete subtree together, detaching a cross-project parent.
    pub fn move_project(&mut self, id: u64, project: u64) -> Result<(), String> {
        if !self.projects.iter().any(|p| p.id == project) {
            return Err("Project not found.".into());
        }
        let task = self.task(id).ok_or("Task not found.")?;
        if task.project_id == project {
            return Ok(());
        }
        if task
            .research
            .as_ref()
            .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal)
        {
            return Err(
                "Move the research goal to move its complete task chain between projects.".into(),
            );
        }
        if task.archived {
            return Err("Restore this task before moving it.".into());
        }
        let mut ids = vec![id];
        let mut cursor = 0;
        while cursor < ids.len() {
            let parent = ids[cursor];
            for child in self.tasks.iter().filter(|t| t.parent_id == Some(parent)) {
                if !ids.contains(&child.id) {
                    ids.push(child.id);
                }
            }
            cursor += 1;
        }
        let topic = self.create_topic(project, self.task(id).unwrap().title.clone())?;
        for task_id in ids.iter().copied() {
            let mut task = self.task_mut(task_id).unwrap();
            task.project_id = project;
            task.topic_id = if task.id == id { Some(topic) } else { None };
            if task.id == id {
                task.parent_id = None;
            }
            task.activity.push(activity_event("Project changed"));
        }
        Ok(())
    }
    /// Side-effect-free lifecycle validation shared by `set_stage` and scoped
    /// command preflight. It never mutates the task and never writes approval.
    pub(crate) fn validate_stage_transition(&self, id: u64, stage: Stage) -> Result<(), String> {
        let task = self.task(id).ok_or("Task not found.")?;
        if stage == Stage::Done
            && task.research.as_ref().is_some_and(|research| {
                research.kind == crate::research::ResearchKind::Goal
                    && research.requires_approval
            })
        {
            let children = self.research_children(id);
            if children.is_empty()
                || self.validate_goal_dependencies(id).is_err()
                || children.iter().any(|child| {
                    self.task(*child)
                        .is_none_or(|task| task.stage != Stage::Done)
                        || self.research_result(*child).is_none()
                })
            {
                return Err(
                    "Complete this goal's research steps and final report before marking it Done."
                        .into(),
                );
            }
        }
        if stage == Stage::Planned
            && task.research.as_ref().is_some_and(|research| {
                research.kind == crate::research::ResearchKind::Goal
            })
        {
            self.validate_goal_dependencies(id)?;
        }
        if task.archived {
            return Err("Restore the archived task before changing its stage.".into());
        }
        if task.research.as_ref().is_some_and(|r| r.requires_approval)
            && matches!(task.stage, Stage::Backlog | Stage::Cancelled)
            && !matches!(stage, Stage::Backlog | Stage::Planned | Stage::Cancelled)
        {
            return Err("Approve this research topic by moving it to Todo first.".into());
        }
        Ok(())
    }
    pub fn set_stage(&mut self, id: u64, stage: Stage) -> Result<(), String> {
        self.validate_stage_transition(id, stage)?;
        let approval = if stage == Stage::Planned
            && self.task(id).is_some_and(|t| {
                t.research
                    .as_ref()
                    .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal)
            }) {
            self.goal_plan_signature(id)
        } else {
            None
        };
        let task = self.task_mut(id).ok_or("Task not found.")?;
        if approval.is_some() {
            task.approved_plan = approval;
        }
        if matches!(stage, Stage::Backlog | Stage::Cancelled) {
            task.approved_plan = None;
        }
        if task.stage != stage {
            task.activity.push(activity_event(&format!(
                "{} → {}",
                task.stage.label(),
                stage.label()
            )));
            task.stage = stage;
        }
        // Routing is configuration only: stage transitions never start a real or simulated run.
        Ok(())
    }
    pub fn set_parent(&mut self, id: u64, parent_id: Option<u64>) -> Result<(), String> {
        let task = self.task(id).ok_or("Task not found.")?;
        let project_id = task.project_id;
        if task.parent_id == parent_id { return Ok(()); }
        if let Some(r) = &task.research {
            let required = if r.kind == crate::research::ResearchKind::Goal {
                None
            } else {
                Some(r.goal_id)
            };
            if parent_id != required {
                return Err("Research tasks keep their goal as parent so materials and dependencies stay together.".into());
            }
        }
        if task.archived {
            return Err("Restore the archived task first.".into());
        }
        let mut cursor = parent_id;
        let mut visited = Vec::new();
        while let Some(candidate) = cursor {
            if candidate == id || visited.contains(&candidate) {
                return Err("A task cannot be its own ancestor.".into());
            }
            visited.push(candidate);
            let parent = self.task(candidate).ok_or("Parent task not found.")?;
            if parent.archived || parent.project_id != project_id {
                return Err("Choose an active parent in the same project.".into());
            }
            cursor = parent.parent_id;
        }
        let topic = if parent_id.is_none() {
            Some(self.create_topic(project_id, self.task(id).unwrap().title.clone())?)
        } else {
            None
        };
        let task = self.task_mut(id).unwrap();
        if task.parent_id != parent_id {
            task.parent_id = parent_id;
            task.topic_id = topic;
            task.activity.push(activity_event("Parent task changed"));
        }
        Ok(())
    }
    pub fn add_comment(&mut self, id: u64, text: impl Into<String>) -> Result<(), String> {
        let body = text.into().trim().to_string();
        if body.is_empty() {
            return Err("Write a comment before posting.".into());
        }
        let task = self.task_mut(id).ok_or("Task not found.")?;
        task.comments.push(Comment {
            author: "You".into(),
            body,
            created_at: crate::opencode::timestamp(),
        });
        task.activity.push(activity_event("Comment added"));
        Ok(())
    }
    pub fn archive(&mut self, id: u64) -> Result<(), String> {
        let task = self.task_mut(id).ok_or("Task not found.")?;
        if !task.archived {
            task.archived = true;
            task.activity.push(activity_event("Task archived"));
        }
        Ok(())
    }
    pub fn restore(&mut self, id: u64) -> Result<(), String> {
        let task = self.task_mut(id).ok_or("Task not found.")?;
        if task.archived {
            task.archived = false;
            task.activity.push(activity_event("Task restored"));
        }
        Ok(())
    }
    pub fn prepare_run(&self, id: u64) -> Result<AgentRun, String> {
        if self.research_pending_approval(id) {
            return Err(
                "Approve the complete research goal in Todo before running its steps.".into(),
            );
        }
        crate::research::prepare_run(self, id)
    }
    pub fn mark_research_running(&mut self, run_id: u64) {
        let Some(run) = self.runs.iter().find(|r| r.id == run_id) else {
            return;
        };
        if run.project_plan.is_none() && run.research_kind.is_some() {
            let _ = self.set_stage(run.task_id, Stage::InProgress);
        }
    }
    pub fn restore_research_stage(&mut self, run_id: u64) {
        let Some(run) = self.runs.iter().find(|r| r.id == run_id) else {
            return;
        };
        if run.project_plan.is_none()
            && run.research_kind.is_some()
            && self
                .task(run.task_id)
                .is_some_and(|t| t.stage == Stage::InProgress)
        {
            let (task_id, stage) = (run.task_id, run.stage);
            let _ = self.set_stage(task_id, stage);
        }
    }
    pub fn recover_interrupted_runs(&mut self) {
        let mut interrupted = vec![];
        let count = self.runs.len();
        for i in 0..count {
            let current_id = self.runs[i].id;
            if current_id == 0 {
                let repaired = (i + 1) as u64;
                self.runs.reassign_identity(
                    i,
                    crate::persistence_journal::EntityKey::Run(repaired),
                    |run| run.id = repaired,
                );
            }
            let status = self.runs[i].status.clone();
            if matches!(
                status.as_str(),
                "Queued" | "Running" | "Cancelling" | "Recovering"
            ) {
                let run_id = self.runs[i].id;
                interrupted.push(run_id);
                let mut run = self.run_mut(run_id).unwrap();
                run.status = "Interrupted".into();
                run.ended_at = Some(crate::opencode::timestamp());
                run.logs.push("[interrupted] The application stopped before this run finished. Inspect the working folder before retrying.".into());
            }
        }
        for id in interrupted {
            self.restore_research_stage(id);
        }
    }
    pub fn matches_query(&self, task: &Task, query: &str) -> bool {
        query_tokens(query).iter().all(|token| {
            let negative = token.starts_with('-');
            let term = token.strip_prefix('-').unwrap_or(token);
            let found = if term.contains(':') {
                matches_term(task, term)
            } else {
                matches_term(task, term)
                    || self
                        .identifier(task)
                        .to_lowercase()
                        .contains(&term.to_lowercase())
                    || format!(
                        "{}-{}",
                        self.project_key(task),
                        self.identifier(task).rsplit('-').next().unwrap_or("")
                    )
                    .eq_ignore_ascii_case(term)
                    || format!("{}-{}", self.project_key(task), task.id).eq_ignore_ascii_case(term)
            };
            if negative { !found } else { found }
        })
    }
}

/// Owned clean-hydration capability. Only construction paths call this; it
/// cannot receive an existing live `Store`.
pub(crate) struct StoreLoader {
    store: Store,
}

impl StoreLoader {
    pub(crate) fn set_metadata(&mut self, delta: crate::persistence_journal::MetadataDelta) {
        self.store.apply_loaded_metadata(delta);
    }

    pub(crate) fn tasks(&mut self, tasks: Vec<Task>) -> Result<(), String> {
        self.store.load_tasks(tasks)
    }

    pub(crate) fn projects(&mut self, projects: Vec<Project>) -> Result<(), String> {
        self.store.load_projects(projects)
    }

    pub(crate) fn topics(&mut self, topics: Vec<Topic>) -> Result<(), String> {
        self.store.load_topics(topics)
    }

    pub(crate) fn requests(
        &mut self,
        requests: Vec<crate::request_planning::ResearchRequest>,
    ) -> Result<(), String> {
        self.store.load_requests(requests)
    }

    pub(crate) fn policies(&mut self, policies: Vec<StagePolicy>) -> Result<(), String> {
        self.store.load_policies(policies)
    }

    pub(crate) fn runs(
        &mut self,
        runs: Vec<AgentRun>,
        automations: Vec<crate::automation::AutomationDefinition>,
        occurrences: Vec<crate::automation::AutomationOccurrence>,
    ) -> Result<(), String> {
        let store = std::mem::replace(&mut self.store, Store::empty());
        match store.with_loaded_tracked(runs, automations, occurrences) {
            Ok(store) => {
                self.store = store;
                Ok(())
            }
            Err((store, error)) => {
                self.store = store;
                Err(error)
            }
        }
    }

    pub(crate) fn finish(mut self) -> Store {
        self.store.clear_all_tracking();
        self.store
    }
}

impl Store {
    /// Resolve the request -> latest planning-run projection after a load.
    /// Only rows with an actual `request_plan` marker are projected: the marker
    /// is validated against the immutable run owner, and the newest matching
    /// owner supersedes an older attempt. Unmarked legacy `planning_run_id`
    /// values are preserved exactly; an invalid/orphan marker never clears an
    /// existing mapping.
    pub(crate) fn resolve_loaded_planning_links(
        &mut self,
        markers: &std::collections::BTreeMap<(u64, u32), u64>,
    ) {
        let requests: Vec<(u64, u32, Option<u64>)> = self
            .research_requests
            .iter()
            .map(|request| (request.id, request.revision, request.planning_run_id))
            .collect();
        for (id, revision, current) in requests {
            let owner_latest = self
                .runs
                .iter()
                .filter(|run| {
                    run.planning_owner.as_ref().is_some_and(|owner| {
                        owner.request_id == id && owner.revision == revision
                    })
                })
                .map(|run| run.id)
                .max();
            let marker = markers.get(&(id, revision)).copied();
            let marker_owner = marker.filter(|run_id| {
                self.runs.iter().any(|run| {
                    run.id == *run_id
                        && run.planning_owner.as_ref().is_some_and(|owner| {
                            owner.request_id == id && owner.revision == revision
                        })
                })
            });
            // Only marker-backed rows are projected. A valid owner supersedes
            // an older marker attempt; no marker means this is legacy/unmarked
            // state and is preserved exactly as loaded.
            let resolved = if marker.is_some() {
                owner_latest.or(marker_owner)
            } else {
                None
            };
            if let Some(resolved) = resolved
                && current != Some(resolved)
                && let Some(mut request) = self.request_mut(id)
                && request.revision == revision
            {
                request.planning_run_id = Some(resolved);
            }
        }
        self.research_requests.clear_dirty();
    }
}

/// Whitespace-delimited terms with double-quoted values; conditions are ANDed.
/// Comma-delimited values within a field are ORed. A leading minus excludes a term.
#[cfg(test)]
pub fn matches_query(task: &Task, query: &str) -> bool {
    query_tokens(query).iter().all(|token| {
        if let Some(term) = token.strip_prefix('-') {
            !matches_term(task, term)
        } else {
            matches_term(task, token)
        }
    })
}
fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.chars() {
        if character == '"' {
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
fn matches_term(task: &Task, term: &str) -> bool {
    let term = term.to_lowercase();
    if let Some((field, values)) = term.split_once(':') {
        return values.split(',').any(|value| match field {
            "status" | "stage" => task.stage.matches(value),
            "priority" => {
                task.priority.key() == value || task.priority.label().to_lowercase() == value
            }
            "label" | "labels" => task
                .labels
                .iter()
                .any(|label| label.to_lowercase() == value),
            "cycle" => task
                .cycle
                .as_ref()
                .is_some_and(|cycle| cycle.to_lowercase() == value),
            "bucket" => task.bucket.to_lowercase() == value,
            _ => false,
        });
    }
    task.title.to_lowercase().contains(&term)
        || task.description.to_lowercase().contains(&term)
        || task
            .labels
            .iter()
            .any(|label| label.to_lowercase().contains(&term))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_project_is_generated_hidden_and_reused() {
        let mut store = Store::empty();
        let user_projects = store.visible_projects().count();
        let id = store
            .ensure_automation_project("/tmp/automations".into())
            .unwrap();
        assert_eq!(store.visible_projects().count(), user_projects);
        assert!(store.visible_projects().all(|project| project.id != id));
        assert_eq!(
            store.automation_project().map(|project| project.name.as_str()),
            Some("Automations")
        );
        // A second call never creates another container.
        assert_eq!(store.ensure_automation_project("/other".into()).unwrap(), id);
        assert_eq!(store.projects.iter().filter(|p| p.kind == ProjectKind::Automation).count(), 1);
        // The container survives serialization without leaking into user projects.
        let reopened: Store = serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(reopened.visible_projects().count(), user_projects);
        assert!(reopened.automation_project().is_some());
    }

    #[test]
    fn agent_configuration_migrates_and_prevents_cross_agent_model_inheritance() {
        use crate::research::ResearchKind;
        let legacy: crate::research::ResearchConfig = serde_json::from_str(r#"{"models":["test/plan","","",""],"ego_path":"ego-browser"}"#).unwrap();
        assert_eq!(legacy.default_agent, "OpenCode");
        assert!(legacy.agents.iter().all(Option::is_none));
        let mut store = Store::empty();
        store.set_default_model("test/global".into());
        store.set_research_config(legacy);
        assert_eq!(store.research_agent(ResearchKind::Goal), "OpenCode");
        store.research_config_mut().agents[2] = Some("Codex".into());
        assert_eq!(store.research_model(ResearchKind::Analyze), "");
        assert!(!store.valid_research_model(ResearchKind::Analyze));
        store.research_config_mut().models[2] = "fixture-codex".into();
        assert!(store.valid_research_model(ResearchKind::Analyze));
        let reopened: Store = serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(reopened, store);
        store.research_config_mut().agents[2] = None;
        store.research_config_mut().models[2].clear();
        assert_eq!(store.research_model(ResearchKind::Analyze), "test/global");
    }

    #[test]
    fn progress_count_tracks_tasks_instead_of_attempts() {
        let mut store = Store::mock();
        store.clear_runs();
        store.project_at_mut(0).unwrap().directory = "/tmp/test-project".into();
        for stage in Stage::ALL {
            if let Some(policy) = store.policy_mut(stage) {
                policy.agent = "OpenCode".into();
                policy.model = "test/model".into();
            }
        }
        store.set_stage(1, Stage::InProgress).unwrap();
        store.simulate_run(1).unwrap();
        let template = store.pop_run().unwrap();
        let mut next_run_id = 1_000;
        for index in 0..8 {
            let id = store.create_task(&format!("Step {index}"), 1, Stage::InProgress, None).unwrap();
            let mut run = template.clone();
            run.id = next_run_id;
            next_run_id += 1;
            run.task_id = id;
            run.status = "Failed".into();
            store.push_run(run.clone());
            run.id = next_run_id;
            next_run_id += 1;
            run.status = if index == 7 { "Running" } else { "Succeeded" }.into();
            if index != 7 { run.output = "Saved result".into(); }
            store.push_run(run);
        }
        assert_eq!(store.runs.len(), 16);
        assert_eq!(store.progress_result_task_count(), 8);
        let mut planning = template.clone();
        planning.id = next_run_id;
        next_run_id += 1;
        planning.research_kind = Some(crate::research::ResearchKind::Goal);
        store.push_run(planning);
        let mut planning = template.clone();
        planning.id = next_run_id;
        next_run_id += 1;
        planning.project_plan = Some(1);
        store.push_run(planning);
        assert_eq!(store.progress_result_task_count(), 8);

        // Retrying a task with a saved result must not count it twice.
        let mut retry = store.runs[1].clone();
        retry.id = next_run_id;
        next_run_id += 1;
        retry.status = "Running".into();
        retry.output.clear();
        store.push_run(retry);
        assert_eq!(store.progress_result_task_count(), 8);
        // A later failure supersedes an old running record, but keeps saved results.
        let mut failed = store.runs[15].clone();
        failed.id = next_run_id;
        next_run_id += 1;
        failed.status = "Failed".into();
        store.push_run(failed);
        assert_eq!(store.progress_result_task_count(), 7);
        let id = store.runs[1].task_id;
        store.task_mut(id).unwrap().archived = true;
        assert_eq!(store.progress_result_task_count(), 6);

        for status in ["Failed", "Blocked", "Interrupted", "Cancelled", "Succeeded"] {
            let mut run = template.clone();
            run.id = next_run_id;
            next_run_id += 1;
            run.status = status.into();
            store.push_run(run);
            assert_eq!(store.progress_result_task_count(), 6);
        }
        for status in ["Queued", "Running", "Recovering", "Cancelling"] {
            store.last_run_mut().unwrap().status = status.into();
            assert_eq!(store.progress_result_task_count(), 7);
        }
    }
    #[test]
    fn explicit_topic_creation_never_leaves_an_intermediate_topic() {
        let mut store = Store::empty();
        let topic = store.create_topic(1, "Earnings").unwrap();
        let goal = store.create_research_goal_in_topic(
            "Intel earnings".into(), "Review financials".into(), 1, Some(topic),
        ).unwrap();
        assert_eq!(store.topics.len(), 1);
        assert_eq!(store.task_topic(goal).unwrap().id, topic);
        store.set_parent(goal, None).unwrap();
        assert_eq!(store.topics.len(), 1);
        assert_eq!(store.task_topic(goal).unwrap().id, topic);
        assert!(store.create_research_goal_in_topic(
            "Invalid".into(), "Scope".into(), 1, Some(999),
        ).is_err());
        assert_eq!(store.tasks.len(), 1);
        assert_eq!(store.topics.len(), 1);
    }

    #[test]
    fn placeholder_cleanup_preserves_referenced_topics_and_is_idempotent() {
        let mut store = Store::empty();
        let populated = store.create_topic(1, "待归类").unwrap();
        store.create_research_goal_in_topic("Legacy".into(), "Scope".into(), 1, Some(populated)).unwrap();
        let requested = store.create_topic(1, "未归类").unwrap();
        store.create_research_request(1, Some(requested), "Review earnings").unwrap();
        let empty = store.create_topic(1, "待归类").unwrap();
        store.set_topic_archived(empty, true).unwrap();
        let named_empty = store.create_topic(1, "My next research").unwrap();
        let tasks = store.tasks.clone();
        store.normalize_topics();
        assert!(!store.topics.iter().any(|t| t.id == empty));
        for id in [populated, requested, named_empty] {
            assert!(store.topics.iter().any(|t| t.id == id));
        }
        assert_eq!(store.task_at_mut(0).unwrap().title, tasks[0].title);
        assert_eq!(store.task_at_mut(0).unwrap().topic_id, tasks[0].topic_id);
        let normalized = store.clone();
        store.normalize_topics();
        assert_eq!(store, normalized);
    }

    #[test]
    fn topic_membership_is_required_and_inherited_by_research_steps() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let topic = store.create_topic(project, "  NVIDIA  ").unwrap();
        let goal = store
            .create_research_goal("Earnings".into(), "Review earnings".into(), project)
            .unwrap();
        let step = store
            .create_task("Evidence", project, Stage::Planned, Some(goal))
            .unwrap();
        store.task_mut(step).unwrap().research = Some(crate::research::ResearchTask {
            requires_approval: false,
            kind: crate::research::ResearchKind::Collect,
            goal_id: goal,
            depends_on: vec![],
            space_name: String::new(),
        });
        assert_eq!(store.task_topic(goal).unwrap().name, "Earnings");
        let signature = store.research_signature(step);
        assert!(signature.is_some());
        store.set_goal_topic(goal, Some(topic)).unwrap();
        assert_eq!(store.task_topic(step).unwrap().name, "NVIDIA");
        assert_eq!(store.task(step).unwrap().topic_id, None);
        assert_eq!(store.research_signature(step), signature);
        assert!(store.set_goal_topic(step, Some(topic)).is_err());
        let ordinary = store
            .create_task("Ordinary", project, Stage::Planned, None)
            .unwrap();
        assert!(store.set_goal_topic(ordinary, Some(topic)).is_err());
        assert!(store.set_goal_topic(goal, None).is_err());
        assert_eq!(store.task_topic(step).unwrap().id, topic);
    }

    #[test]
    fn ordinary_descendants_inherit_topic_and_parent_cycles_terminate() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let topic = store.create_topic(project, "NVIDIA").unwrap();
        let goal = store
            .create_research_goal("Earnings".into(), "Review earnings".into(), project)
            .unwrap();
        store.set_goal_topic(goal, Some(topic)).unwrap();
        let child = store
            .create_task("Follow up", project, Stage::Planned, Some(goal))
            .unwrap();
        let descendant = store
            .create_task("Read appendix", project, Stage::Planned, Some(child))
            .unwrap();
        assert_eq!(store.task_topic(child).unwrap().id, topic);
        assert_eq!(store.task_topic(descendant).unwrap().id, topic);
        assert_eq!(store.task(descendant).unwrap().topic_id, None);
        store.set_topic_archived(topic, true).unwrap();
        assert!(store.task_topic(descendant).unwrap().archived);
        store.set_parent(child, None).unwrap();
        assert_eq!(store.task_topic(descendant).unwrap().name, "Follow up");
        // Older or externally edited workspaces must not hang on invalid ancestry.
        store.task_mut(child).unwrap().parent_id = Some(descendant);
        assert!(store.task_topic(descendant).is_none());
    }

    #[test]
    fn topic_archive_preserves_work_and_allows_restore() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let topic = store.create_topic(project, "NVIDIA").unwrap();
        let goal = store
            .create_research_goal("Earnings".into(), "Review earnings".into(), project)
            .unwrap();
        store.set_goal_topic(goal, Some(topic)).unwrap();
        let tasks = store.tasks.clone();
        store.set_topic_archived(topic, true).unwrap();
        assert_eq!(store.tasks, tasks);
        assert!(store.task_topic(goal).unwrap().archived);
        let second = store
            .create_research_goal("Valuation".into(), "Review valuation".into(), project)
            .unwrap();
        assert!(store.set_goal_topic(second, Some(topic)).is_err());
        store.set_topic_archived(topic, false).unwrap();
        store.set_goal_topic(second, Some(topic)).unwrap();
        store.rename_topic(topic, "  AI chips  ").unwrap();
        assert_eq!(store.task_topic(goal).unwrap().name, "AI chips");
        assert!(store.rename_topic(topic, "  ").is_err());
    }

    #[test]
    fn topic_membership_cannot_cross_projects_and_moves_to_named_destination_topic() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let mut other = store.project_at_mut(0).unwrap().clone();
        other.id = project + 1;
        store.push_project(other);
        let topic = store.create_topic(project, "NVIDIA").unwrap();
        let goal = store
            .create_research_goal("Earnings".into(), "Review earnings".into(), project)
            .unwrap();
        let foreign_topic = store.create_topic(project + 1, "Other").unwrap();
        assert!(store.set_goal_topic(goal, Some(foreign_topic)).is_err());
        assert!(store.set_goal_topic(goal, Some(999)).is_err());
        assert!(store.create_topic(999, "Unknown").is_err());
        assert!(store.create_topic(project, " ").is_err());
        store.set_goal_topic(goal, Some(topic)).unwrap();
        store.move_project(goal, project + 1).unwrap();
        assert_eq!(store.task_topic(goal).unwrap().project_id, project + 1);
        assert_eq!(store.task_topic(goal).unwrap().name, "Earnings");
        store.set_goal_topic(goal, Some(foreign_topic)).unwrap();
    }

    #[test]
    fn legacy_workspaces_load_without_topics_and_membership_roundtrips() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let goal = store
            .create_research_goal("Earnings".into(), "Review earnings".into(), project)
            .unwrap();
        let mut legacy = serde_json::to_value(&store).unwrap();
        legacy.as_object_mut().unwrap().remove("topics");
        for task in legacy["tasks"].as_array_mut().unwrap() {
            task.as_object_mut().unwrap().remove("topic_id");
        }
        let loaded: Store = serde_json::from_value(legacy).unwrap();
        assert!(loaded.topics.is_empty());
        assert_eq!(loaded.task(goal).unwrap().topic_id, None);
        let topic = store.create_topic(project, "NVIDIA").unwrap();
        store.set_goal_topic(goal, Some(topic)).unwrap();
        let loaded: Store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(loaded, store);
    }

    #[test]
    fn legacy_topic_normalization_preserves_content_and_is_idempotent() {
        let mut store = Store::mock();
        let before: Vec<_> = store
            .tasks
            .iter()
            .map(|t| (t.id, t.title.clone(), t.description.clone(), t.stage))
            .collect();
        store.clear_topics();
        let ids: Vec<u64> = store.tasks().iter().map(|task| task.id).collect();
        for id in ids {
            store.task_mut(id).unwrap().topic_id = None;
        }
        store.normalize_topics();
        assert!(store.tasks.iter().all(|t| store.task_topic(t.id).is_some()));
        assert_eq!(
            before,
            store
                .tasks
                .iter()
                .map(|t| (t.id, t.title.clone(), t.description.clone(), t.stage))
                .collect::<Vec<_>>()
        );
        let normalized = store.clone();
        store.normalize_topics();
        assert_eq!(store, normalized);
    }

    #[test]
    fn loaded_legacy_goals_gain_versioned_approval_without_permanent_exemption() {
        let mut store = Store::empty();
        let goal = store
            .create_research_goal("Legacy".into(), "Review earnings".into(), 1)
            .unwrap();
        let stage = store.task(goal).unwrap().stage;
        store.normalize_topics();
        assert!(
            store
                .task(goal)
                .unwrap()
                .research
                .as_ref()
                .unwrap()
                .requires_approval
        );
        assert_eq!(store.task(goal).unwrap().stage, stage);
        assert!(store.goal_execution_approved(goal));
        store.add_comment(goal, "Expand research scope").unwrap();
        store.normalize_topics();
        assert!(
            !store.goal_execution_approved(goal),
            "Repeated migration must not approve changes"
        );
        store.reconcile_goal_approvals();
        assert_eq!(store.task(goal).unwrap().stage, Stage::Backlog);
        store.task_mut(goal).unwrap().stage = Stage::Planned;
        store.normalize_topics();
        assert!(
            !store.goal_execution_approved(goal),
            "Explicit missing current approval cannot be inferred from Todo"
        );
    }

    #[test]
    fn missing_legacy_approval_field_migrates_once_but_explicit_null_does_not() {
        let mut store = Store::empty();
        let goal = store
            .create_research_goal("Legacy".into(), "Review earnings".into(), 1)
            .unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        let mut value = serde_json::to_value(&store).unwrap();
        value["tasks"][0]
            .as_object_mut()
            .unwrap()
            .remove("approved_plan");
        let mut legacy: Store = serde_json::from_value(value).unwrap();
        legacy.normalize_topics();
        assert!(legacy.goal_execution_approved(goal));
        let mut current: Store =
            serde_json::from_value(serde_json::to_value(&store).unwrap()).unwrap();
        current.normalize_topics();
        assert!(!current.goal_execution_approved(goal));
    }

    #[test]
    fn research_step_identifiers_include_goal_and_keep_previous_alias() {
        use crate::research::{ResearchKind, ResearchTask};
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let key = store.project_at_mut(0).unwrap().key.clone();
        let goal = store
            .create_task("Goal", project, Stage::Planned, None)
            .unwrap();
        let step = store
            .create_task("Evidence", project, Stage::Planned, Some(goal))
            .unwrap();
        store.task_mut(step).unwrap().research = Some(ResearchTask {
            requires_approval: false,
            kind: ResearchKind::Collect,
            goal_id: goal,
            depends_on: vec![],
            space_name: String::new(),
        });
        let task = store.task(step).unwrap();
        assert_eq!(store.identifier(task), format!("{key}-G{goal}-E{step}"));
        assert!(store.matches_query(task, &format!("{key}-E{step}")));
        assert!(store.matches_query(task, &format!("{key}-G{goal}")));
        assert!(!store.matches_query(task, &format!("-{key}-E{step}")));
    }
    #[test]
    fn research_identifiers_distinguish_types_and_keep_legacy_search() {
        use crate::research::{ResearchKind, ResearchTask};
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let key = store.project_at_mut(0).unwrap().key.clone();
        let id = store
            .create_task("Example", project, Stage::Backlog, None)
            .unwrap();
        assert_eq!(
            store.identifier(store.task(id).unwrap()),
            format!("{key}-{id}")
        );
        for (kind, code) in [
            (ResearchKind::Goal, "G"),
            (ResearchKind::Collect, "E"),
            (ResearchKind::Analyze, "A"),
            (ResearchKind::Advise, "R"),
        ] {
            store.task_mut(id).unwrap().research = Some(ResearchTask {
                requires_approval: false,
                kind,
                goal_id: id,
                depends_on: vec![],
                space_name: String::new(),
            });
            let expected = format!("{key}-{code}{id}");
            assert_eq!(store.identifier(store.task(id).unwrap()), expected);
            let restored: Store =
                serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
            let task = restored.task(id).unwrap();
            assert_eq!(restored.identifier(task), expected);
            assert!(restored.matches_query(task, &expected.to_lowercase()));
            assert!(restored.matches_query(task, &format!("{key}-{id}")));
            assert!(!restored.matches_query(task, &format!("-{key}-{id}")));
        }
    }
    #[test]
    fn creation_time_is_persisted_and_legacy_tasks_stay_unknown() {
        let mut store = Store::empty();
        let project = store.project_at_mut(0).unwrap().id;
        let id = store
            .create_task("New task", project, Stage::Planned, None)
            .unwrap();
        let task = store.task(id).unwrap();
        assert!(
            chrono::DateTime::parse_from_str(&task.created_at, "%Y-%m-%d %H:%M:%S %:z").is_ok()
        );
        let mut value = serde_json::to_value(task).unwrap();
        let restored: Task = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(restored.created_at, task.created_at);
        value.as_object_mut().unwrap().remove("created_at");
        let legacy: Task = serde_json::from_value(value).unwrap();
        assert!(legacy.created_at.is_empty());
    }
    #[test]
    fn new_workspace_has_no_demo_tasks_or_assumed_models() {
        let mut store = Store::empty();
        assert!(store.tasks.is_empty());
        assert!(store.runs.is_empty());
        assert!(store.default_model.is_empty());
        assert_eq!(store.next_id, 1);
        assert_eq!(store.project_at_mut(0).unwrap().name, "Workspace");
        assert!(store.project_at_mut(0).unwrap().directory.is_empty());
        assert!(store.policies.is_empty());
        assert_eq!(store.appearance, crate::appearance::Appearance::Light);
    }
    #[test]
    fn ordinary_tasks_cannot_start_new_runs() {
        let store = Store::mock();
        assert!(store.prepare_run(1).is_err());
    }
    #[test]
    fn research_execution_updates_stage_and_recovers_without_overriding_completion() {
        let mut store = Store::mock();
        store.simulate_run(1).unwrap();
        let id = store.runs[0].id;
        let original = store.task(1).unwrap().stage;
        store.run_at_mut(0).unwrap().research_kind = Some(crate::research::ResearchKind::Collect);
        store.run_at_mut(0).unwrap().status = "Running".into();
        store.mark_research_running(id);
        assert_eq!(store.task(1).unwrap().stage, Stage::InProgress);
        store.recover_interrupted_runs();
        assert_eq!(store.task(1).unwrap().stage, original);
        store.mark_research_running(id);
        store.set_stage(1, Stage::Done).unwrap();
        store.restore_research_stage(id);
        assert_eq!(store.task(1).unwrap().stage, Stage::Done);
        store.mark_research_running(id);
        store.set_stage(1, Stage::Cancelled).unwrap();
        store.restore_research_stage(id);
        assert_eq!(store.task(1).unwrap().stage, Stage::Cancelled);
    }
    #[test]
    fn restart_marks_unfinished_runs_interrupted_without_restarting_them() {
        let mut store = Store::mock();
        store.simulate_run(1).unwrap();
        store.run_at_mut(0).unwrap().status = "Running".into();
        store.recover_interrupted_runs();
        assert_eq!(store.runs[0].status, "Interrupted");
        assert!(store.runs[0].ended_at.is_some());
        let recovered = store.clone();
        store.recover_interrupted_runs();
        assert_eq!(store, recovered);
    }
    #[test]
    fn moving_project_keeps_descendants_together_and_detaches_root() {
        let mut store = Store::mock();
        store.move_project(1, 2).unwrap();
        for id in [1, 2, 3, 4, 5] {
            assert_eq!(store.task(id).unwrap().project_id, 2);
        }
        assert_eq!(store.task(2).unwrap().parent_id, Some(1));
        store.move_project(2, 1).unwrap();
        assert_eq!(store.task(2).unwrap().parent_id, None);
        assert_eq!(store.task(1).unwrap().project_id, 2);
    }
    #[test]
    fn rejected_project_move_does_not_mutate_workspace() {
        let mut store = Store::mock();
        let before = store.clone();
        assert!(store.move_project(1, 999).is_err());
        assert_eq!(store, before);
        store.archive(1).unwrap();
        let before = store.clone();
        assert!(store.move_project(1, 2).is_err());
        assert_eq!(store, before);
    }
    #[test]
    fn same_stage_transition_has_no_activity_or_run_side_effect() {
        let mut store = Store::mock();
        let before = store.clone();
        store.set_stage(1, store.task(1).unwrap().stage).unwrap();
        assert_eq!(store, before);
    }
    #[test]
    fn create_trims_title_and_rejects_invalid_input_without_consuming_id() {
        let mut store = Store::mock();
        let next = store.next_id;
        assert!(store.create_task("  ", 1, Stage::Backlog, None).is_err());
        assert!(store.create_task("Task", 99, Stage::Backlog, None).is_err());
        assert_eq!(store.next_id, next);
        let id = store
            .create_task("  新任务  ", 1, Stage::Planned, None)
            .unwrap();
        assert_eq!(store.task(id).unwrap().title, "新任务");
    }
    #[test]
    fn parent_changes_reject_cycles_and_cross_project_relationships() {
        let mut store = Store::mock();
        assert!(store.set_parent(1, Some(2)).is_err());
        assert!(store.set_parent(1, Some(1)).is_err());
        assert!(store.set_parent(1, Some(16)).is_err());
        assert_eq!(store.task(1).unwrap().parent_id, None);
        store.set_parent(2, None).unwrap();
        assert_eq!(store.task(2).unwrap().parent_id, None);
    }
    #[test]
    fn changing_stage_records_one_event_and_never_launches_agent() {
        let mut store = Store::mock();
        for stage in Stage::ALL { if let Some(policy) = store.policy_mut(stage) { policy.auto_start = true; } }
        let before = store.task(5).unwrap().activity.len();
        store.set_stage(5, Stage::InProgress).unwrap();
        store.set_stage(5, Stage::InProgress).unwrap();
        assert_eq!(store.task(5).unwrap().activity.len(), before + 1);
        assert!(store.runs.is_empty());
        assert!(matches_query(store.task(5).unwrap(), "status:in-progress"));
        assert!(!matches_query(store.task(5).unwrap(), "status:todo"));
    }
    #[test]
    fn query_combines_fields_quotes_aliases_and_exclusion() {
        let store = Store::mock();
        let task = store.task(5).unwrap();
        assert!(matches_query(
            task,
            "status:planned priority:high label:bug 中文"
        ));
        assert!(!matches_query(task, "status:todo -label:bug"));
        assert!(matches_query(
            store.task(1).unwrap(),
            "status:\"in progress\" priority:high,urgent"
        ));
        assert!(store.matches_query(task, "TM-5"));
        assert!(!store.matches_query(task, "AG-5"));
    }
    #[test]
    fn blank_comment_is_rejected_and_comment_is_trimmed() {
        let mut store = Store::mock();
        assert!(store.add_comment(5, " \n ").is_err());
        store.add_comment(5, "  已验证中文输入  ").unwrap();
        assert_eq!(store.task(5).unwrap().comments[0].body, "已验证中文输入");
    }
    #[test]
    fn archive_is_recoverable_and_blocks_stage_changes() {
        let mut store = Store::mock();
        let original = store.task(1).unwrap().title.clone();
        store.archive(1).unwrap();
        assert_eq!(store.task(1).unwrap().title, original);
        assert!(store.task(1).unwrap().archived);
        assert!(store.set_stage(1, Stage::Done).is_err());
        assert!(store.simulate_run(1).is_err());
    }
    #[test]
    fn simulated_run_snapshots_policy_without_changing_stage() {
        let mut store = Store::mock();
        let stage = store.task(1).unwrap().stage;
        store.simulate_run(1).unwrap();
        let before = store.runs[0].clone();
        store.policy_mut(stage).unwrap().model = "another-model".into();
        assert_eq!(store.runs[0], before);
        assert_eq!(store.task(1).unwrap().stage, stage);
        assert_eq!(store.runs[0].status, "Simulated");
        assert!(
            store.runs[0]
                .logs
                .iter()
                .all(|line| line.starts_with("[simulation]"))
        );
        assert!(store.simulate_run(15).is_err());
    }
    #[test]
    fn workspace_round_trip_preserves_tasks_policies_and_history() {
        let mut store = Store::mock();
        store.add_comment(5, "Checked").unwrap();
        store.set_stage(5, Stage::Review).unwrap();
        store.simulate_run(5).unwrap();
        let json = serde_json::to_string(&store).unwrap();
        let restored: Store = serde_json::from_str(&json).unwrap();
        assert_eq!(store, restored);
        assert_eq!(restored.next_id, 23);
    }

    #[test]
    fn clean_load_requires_a_fresh_store() {
        // A populated store is rejected even after its dirty markers were drained.
        let mut populated = Store::mock();
        populated.simulate_run(1).unwrap();
        populated.drain_tracked_changes().unwrap();
        let (populated, error) = populated
            .with_loaded_tracked(Vec::new(), Vec::new(), Vec::new())
            .expect_err("populated stores must not be reset as a load");
        assert!(error.contains("fresh empty store"), "{error}");
        assert_eq!(populated.runs().len(), 1, "rejected load preserves ownership");

        // Tombstones left by clearing are still tracked intent and must block a load.
        let mut cleared = Store::mock();
        cleared.simulate_run(1).unwrap();
        cleared.drain_tracked_changes().unwrap();
        cleared.clear_runs();
        assert!(
            cleared
                .with_loaded_tracked(Vec::new(), Vec::new(), Vec::new())
                .is_err(),
            "cleared-but-dirty stores must not be reset as a load"
        );

        // A truly fresh builder loads cleanly with no pending tracked changes.
        let loaded = Store::empty()
            .with_loaded_tracked(Vec::new(), Vec::new(), Vec::new())
            .expect("fresh store loads");
        assert!(!loaded.has_tracked_changes());
        assert!(loaded.runs().is_empty());
    }

    #[test]
    fn every_persistent_scalar_and_collection_emits_typed_changes() {
        use crate::persistence_journal::{EntityChange, EntityKey, MetadataDelta};
        let mut store = Store::empty();
        store.clear_all_tracking();
        assert!(!store.has_tracked_changes());

        store.set_next_id(42);
        store.set_run_id_floor(50_001);
        store.set_default_model("provider/model".into());
        store.set_opencode_path("/bin/opencode".into());
        store.set_active_model_profile(Some("review".into()));
        store.set_max_concurrent_runs(7);
        store.set_max_concurrent_automations(8);
        store.set_appearance(crate::appearance::Appearance::Dark);
        store.set_last_active_project(Some(1));
        store.set_automation_schema_version(9);
        store.research_config_mut().ego_path = "ego".into();
        store.report_publishing_mut().branch = "main".into();
        store.push_model_profile(crate::model_profiles::ModelProfile {
            name: "review".into(),
            default_agent: "OpenCode".into(),
            default_model: "provider/model".into(),
            agents: Default::default(),
            models: Default::default(),
        });
        store.push_publication(crate::report_publish::Publication {
            id: "pub".into(),
            topic: 1,
            fingerprint: String::new(),
            config: Default::default(),
            title: String::new(),
            markdown: String::new(),
            status: "Pending".into(),
            commit: None,
            message: String::new(),
        });

        let deltas = store.pending_metadata_deltas();
        let fields: Vec<&str> = deltas.iter().map(MetadataDelta::field).collect();
        for expected in [
            "next_id",
            "run_id_floor",
            "default_model",
            "opencode_path",
            "active_model_profile",
            "max_concurrent_runs",
            "max_concurrent_automations",
            "appearance",
            "last_active_project",
            "automation_schema_version",
            "research_config",
            "report_publishing",
            "model_profiles",
            "publications",
        ] {
            assert!(fields.contains(&expected), "missing {expected}: {fields:?}");
        }

        let topic = store.create_topic(1, "Tracked topic").unwrap();
        let task = store
            .create_task_in_topic("Tracked task", 1, Stage::Planned, None, Some(topic))
            .unwrap();
        store.task_mut(task).unwrap().title = "Changed".into();
        store.push_project(Project {
            id: 2,
            name: "Second".into(),
            key: "P2".into(),
            directory: String::new(),
            research_brief: String::new(),
            kind: ProjectKind::User,
        });
        store.push_request(crate::request_planning::ResearchRequest {
            id: 7,
            project_id: 1,
            topic_id: Some(topic),
            automation_id: None,
            expected_result: None,
            revision: 1,
            provisional_topic_name: None,
            input: "Request".into(),
            submitted_at: String::new(),
            planning_run_id: None,
            goal_ids: Vec::new(),
            brief: None,
        });
        store.remove_project(2);

        let changes = store.pending_entity_changes().unwrap();
        assert!(changes.contains(&EntityChange::Upsert(
            crate::persistence_journal::EntityPayload::Task(store.task(task).unwrap().clone())
        )));
        assert!(changes.contains(&EntityChange::Upsert(
            crate::persistence_journal::EntityPayload::Topic(store.topic(topic).unwrap().clone())
        )));
        assert!(changes.contains(&EntityChange::Upsert(
            crate::persistence_journal::EntityPayload::ResearchRequest(
                store.research_requests()[0].clone()
            )
        )));
        assert!(changes.contains(&EntityChange::Delete(EntityKey::Project(2))));
        assert!(matches!(
            store.pending_entity_changes().unwrap().iter().find(|change| {
                matches!(change, EntityChange::Delete(EntityKey::Project(_)))
            }),
            Some(_)
        ));
    }

    #[test]
    fn acknowledgement_retires_only_the_drained_generation() {
        let mut store = Store::mock();
        store.clear_all_tracking();
        store.set_default_model("first".into());
        let drained = store.drain_store_delta().unwrap();
        assert_eq!(drained.metadata.len(), 1);
        store.retire_store_delta(&drained);
        assert!(!store.has_tracked_changes());

        store.set_default_model("second".into());
        let stale = store.drain_store_delta().unwrap();
        store.set_default_model("third".into());
        store.retire_store_delta(&stale);
        assert!(
            store.has_tracked_changes(),
            "a newer write must survive an older acknowledgement"
        );
        let current = store.drain_store_delta().unwrap();
        store.retire_store_delta(&current);
        assert!(!store.has_tracked_changes());
    }

    #[test]
    fn same_identity_edit_after_drain_survives_an_older_ack() {
        use crate::persistence_journal::{EntityChange, EntityPayload};
        let mut store = Store::empty();
        store.clear_all_tracking();
        let topic = store.create_topic(1, "Topic").unwrap();
        let task = store
            .create_task_in_topic("First title", 1, Stage::Planned, None, Some(topic))
            .unwrap();
        let receipt = store.drain_store_delta().unwrap();
        // Same id, edited after the group was drained but before it is acked.
        store.task_mut(task).unwrap().title = "Second title".into();
        store.retire_store_delta(&receipt);
        assert!(store.has_tracked_changes());
        let changes = store.pending_entity_changes().unwrap();
        assert!(changes.iter().any(|change| matches!(
            change,
            EntityChange::Upsert(EntityPayload::Task(value))
                if value.id == task && value.title == "Second title"
        )));

        let current = store.drain_store_delta().unwrap();
        store.retire_store_delta(&current);
        assert!(!store.has_tracked_changes());
    }

    #[test]
    fn stale_scalar_receipt_cannot_clear_a_new_generation() {
        let mut store = Store::empty();
        store.clear_all_tracking();
        store.set_default_model("first".into());
        let first = store.drain_store_delta().unwrap();
        store.retire_store_delta(&first);
        assert!(!store.has_tracked_changes());

        store.set_default_model("second".into());
        store.retire_store_delta(&first);
        assert!(
            store.has_tracked_changes(),
            "a retired receipt must not match a later generation after ABA"
        );
        store.retire_store_delta(&first);
        assert!(store.has_tracked_changes());
    }

    #[test]
    fn stale_list_receipt_and_out_of_order_ack_are_safe() {
        let mut store = Store::empty();
        store.clear_all_tracking();
        store.push_publication(crate::report_publish::Publication {
            id: "a".into(),
            topic: 1,
            fingerprint: String::new(),
            config: Default::default(),
            title: String::new(),
            markdown: String::new(),
            status: "Pending".into(),
            commit: None,
            message: String::new(),
        });
        let first = store.drain_store_delta().unwrap();
        store.retire_store_delta(&first);
        store.push_publication(crate::report_publish::Publication {
            id: "b".into(),
            topic: 1,
            fingerprint: String::new(),
            config: Default::default(),
            title: String::new(),
            markdown: String::new(),
            status: "Pending".into(),
            commit: None,
            message: String::new(),
        });
        let second = store.drain_store_delta().unwrap();
        // Out-of-order: older ack after newer drain, then newer ack.
        store.retire_store_delta(&first);
        assert!(store.has_tracked_changes());
        store.retire_store_delta(&second);
        assert!(!store.has_tracked_changes());
        // Duplicate ack of a retired generation is harmless.
        store.retire_store_delta(&first);
        assert!(!store.has_tracked_changes());
    }

    #[test]
    fn unchanged_setter_is_not_dirty() {
        let mut store = Store::empty();
        store.clear_all_tracking();
        store.set_max_concurrent_runs(store.max_concurrent_runs());
        store.set_appearance(store.appearance());
        store.set_default_model(store.default_model().clone());
        assert!(!store.has_tracked_changes());
        assert!(store.pending_metadata_deltas().is_empty());
    }

    #[test]
    fn run_floor_survives_json_round_trip() {
        let mut store = Store::empty();
        store.set_run_id_floor(50_001);
        store.clear_all_tracking();
        let json = serde_json::to_string(&store).unwrap();
        let restored: Store = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.run_id_floor(), 50_001);
        assert!(!restored.has_tracked_changes());
    }
}

#[cfg(test)]
mod activity_time_tests {
    use super::*;

    #[test]
    fn events_record_seconds_and_survive_serialization() {
        let event = activity_event("Todo → In progress");
        let (_, time) = event.rsplit_once(" · ").unwrap();
        chrono::DateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S %:z").unwrap();
        let saved = serde_json::to_string(&event).unwrap();
        let restored: String = serde_json::from_str(&saved).unwrap();
        assert_eq!(activity_display(&restored, ""), event);
    }

    #[test]
    fn legacy_events_do_not_invent_times() {
        let time = "2026-10-29 20:07:14 +08:00";
        assert_eq!(
            activity_display("Task created · Just now", time),
            format!("Task created · {time}")
        );
        assert_eq!(
            activity_display("Task created · Just now", ""),
            "Task created · Time not recorded"
        );
        assert_eq!(
            activity_display("Task details updated · just now", time),
            "Task details updated · Time not recorded"
        );
        assert_eq!(
            activity_display("Todo → Done · Just now", time),
            "Todo → Done · Time not recorded"
        );
    }
}
