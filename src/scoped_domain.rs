//! T13 S3e/f: bounded, command-targeted change sets for project/topic context,
//! directory, archive/restore and settings.
//!
//! Each command records only the fields it writes. There is no whole-Store
//! snapshot or whole-domain diff. The inverse validates every recorded field
//! before touching anything, then applies a fixed write set.

use crate::appearance::Appearance;
use crate::domain::{AgentRun, Project, ResearchRequest, Store, Task, Topic};
use crate::model_profiles::ModelProfile;
use crate::report_publish::PublishingConfig;
use crate::research::ResearchConfig;
use crate::scoped_edit::ScopedEditError;

#[derive(Clone, Debug, PartialEq)]
enum ProjectDelta {
    Name {
        id: u64,
        before: String,
        after: String,
    },
    Directory {
        id: u64,
        before: String,
        after: String,
    },
    Created(Project),
    Deleted(Project),
}

impl ProjectDelta {
    fn key(&self) -> u64 {
        match self {
            Self::Name { id, .. } | Self::Directory { id, .. } => *id,
            Self::Created(value) | Self::Deleted(value) => value.id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TopicDelta {
    Name {
        id: u64,
        before: String,
        after: String,
    },
    Project {
        id: u64,
        before: u64,
        after: u64,
    },
    Archived {
        id: u64,
        before: bool,
        after: bool,
    },
    Created(Topic),
    Deleted(Topic),
}

impl TopicDelta {
    fn key(&self) -> u64 {
        match self {
            Self::Name { id, .. } | Self::Project { id, .. } | Self::Archived { id, .. } => *id,
            Self::Created(value) | Self::Deleted(value) => value.id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TaskDelta {
    Title {
        id: u64,
        before: String,
        after: String,
    },
    Labels {
        id: u64,
        before: Vec<String>,
        after: Vec<String>,
    },
    Parent {
        id: u64,
        before: Option<u64>,
        after: Option<u64>,
    },
    Topic {
        id: u64,
        before: Option<u64>,
        after: Option<u64>,
    },
    Project {
        id: u64,
        before: u64,
        after: u64,
    },
    ApprovedPlan {
        id: u64,
        before: Option<String>,
        after: Option<String>,
    },
    Archived {
        id: u64,
        before: bool,
        after: bool,
    },
}

impl TaskDelta {
    fn key(&self) -> u64 {
        match self {
            Self::Title { id, .. }
            | Self::Labels { id, .. }
            | Self::Parent { id, .. }
            | Self::Topic { id, .. }
            | Self::Project { id, .. }
            | Self::ApprovedPlan { id, .. }
            | Self::Archived { id, .. } => *id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum RequestDelta {
    Project {
        id: u64,
        before: u64,
        after: u64,
    },
    Topic {
        id: u64,
        before: Option<u64>,
        after: Option<u64>,
    },
}

impl RequestDelta {
    fn key(&self) -> u64 {
        match self {
            Self::Project { id, .. } | Self::Topic { id, .. } => *id,
        }
    }
}

/// Only the user-context fields these commands may change. Runtime status,
/// output, logs, sessions and approvals must never be restored by Undo.
#[derive(Clone, Debug, PartialEq)]
struct RunDelta {
    id: u64,
    before_input_signature: String,
    after_input_signature: String,
    before_project_plan: Option<u64>,
    after_project_plan: Option<u64>,
}

impl RunDelta {
    fn capture(before: &AgentRun, after: &AgentRun) -> Self {
        Self {
            id: before.id,
            before_input_signature: before.input_signature.clone(),
            after_input_signature: after.input_signature.clone(),
            before_project_plan: before.project_plan,
            after_project_plan: after.project_plan,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SettingsDelta {
    default_model: Option<(String, String)>,
    research_config: Option<(ResearchConfig, ResearchConfig)>,
    model_profiles: Option<(Vec<ModelProfile>, Vec<ModelProfile>)>,
    active_model_profile: Option<(Option<String>, Option<String>)>,
    max_concurrent_runs: Option<(usize, usize)>,
    max_concurrent_automations: Option<(usize, usize)>,
    appearance: Option<(Appearance, Appearance)>,
    report_publishing: Option<(PublishingConfig, PublishingConfig)>,
    opencode_path: Option<(String, String)>,
}

impl SettingsDelta {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Validate every changed field before any mutation.
    fn validate(&self, store: &Store) -> Result<(), ScopedEditError> {
        let message = "Settings changed after this action. Undo would overwrite newer work.";
        macro_rules! check {
            ($field:ident, $current:expr) => {
                if let Some((_, after)) = &self.$field
                    && $current != after
                {
                    return Err(conflict(message));
                }
            };
        }
        check!(default_model, store.default_model());
        check!(research_config, store.research_config());
        check!(model_profiles, &store.model_profiles().to_vec());
        check!(active_model_profile, store.active_model_profile());
        check!(max_concurrent_runs, &store.max_concurrent_runs());
        check!(max_concurrent_automations, &store.max_concurrent_automations());
        check!(appearance, &store.appearance());
        check!(report_publishing, store.report_publishing());
        check!(opencode_path, store.opencode_path());
        Ok(())
    }

    /// Infallible after `validate`.
    fn apply(&self, store: &mut Store) {
        if let Some((before, _)) = &self.default_model {
            store.set_default_model(before.clone());
        }
        if let Some((before, _)) = &self.research_config {
            store.set_research_config(before.clone());
        }
        if let Some((before, _)) = &self.model_profiles {
            store.replace_model_profiles(before.clone());
        }
        if let Some((before, _)) = &self.active_model_profile {
            store.set_active_model_profile(before.clone());
        }
        if let Some((before, _)) = &self.max_concurrent_runs {
            store.set_max_concurrent_runs(*before);
        }
        if let Some((before, _)) = &self.max_concurrent_automations {
            store.set_max_concurrent_automations(*before);
        }
        if let Some((before, _)) = &self.appearance {
            store.set_appearance(*before);
        }
        if let Some((before, _)) = &self.report_publishing {
            store.set_report_publishing(before.clone());
        }
        if let Some((before, _)) = &self.opencode_path {
            store.set_opencode_path(before.clone());
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SettingsBefore {
    default_model: String,
    research_config: ResearchConfig,
    model_profiles: Vec<ModelProfile>,
    active_model_profile: Option<String>,
    max_concurrent_runs: usize,
    max_concurrent_automations: usize,
    appearance: Appearance,
    report_publishing: PublishingConfig,
    opencode_path: String,
}

impl SettingsBefore {
    fn capture(store: &Store) -> Self {
        Self {
            default_model: store.default_model().clone(),
            research_config: store.research_config().clone(),
            model_profiles: store.model_profiles().to_vec(),
            active_model_profile: store.active_model_profile().clone(),
            max_concurrent_runs: store.max_concurrent_runs(),
            max_concurrent_automations: store.max_concurrent_automations(),
            appearance: store.appearance(),
            report_publishing: store.report_publishing().clone(),
            opencode_path: store.opencode_path().clone(),
        }
    }

    fn delta(self, store: &Store) -> SettingsDelta {
        macro_rules! field {
            ($field:ident, $current:expr) => {
                ($current != &self.$field)
                    .then(|| (self.$field.clone(), (*$current).clone()))
            };
        }
        SettingsDelta {
            default_model: field!(default_model, store.default_model()),
            research_config: field!(research_config, store.research_config()),
            model_profiles: field!(model_profiles, &store.model_profiles().to_vec()),
            active_model_profile: field!(active_model_profile, store.active_model_profile()),
            max_concurrent_runs: field!(max_concurrent_runs, &store.max_concurrent_runs()),
            max_concurrent_automations: field!(
                max_concurrent_automations,
                &store.max_concurrent_automations()
            ),
            appearance: field!(appearance, &store.appearance()),
            report_publishing: field!(report_publishing, store.report_publishing()),
            opencode_path: field!(opencode_path, store.opencode_path()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DomainUndo {
    projects: Vec<ProjectDelta>,
    topics: Vec<TopicDelta>,
    tasks: Vec<TaskDelta>,
    requests: Vec<RequestDelta>,
    runs: Vec<RunDelta>,
    settings: SettingsDelta,
}

impl DomainUndo {
    /// Swap the write set so a failed durable save can restore the state the
    /// inverse just replaced, without cloning the Store.
    pub(crate) fn invert(&self) -> Self {
        Self {
            projects: self.projects.iter().map(invert_project).collect(),
            topics: self.topics.iter().map(invert_topic).collect(),
            tasks: self.tasks.iter().map(invert_task).collect(),
            requests: self.requests.iter().map(invert_request).collect(),
            runs: self
                .runs
                .iter()
                .map(|delta| RunDelta {
                    id: delta.id,
                    before_input_signature: delta.after_input_signature.clone(),
                    after_input_signature: delta.before_input_signature.clone(),
                    before_project_plan: delta.after_project_plan,
                    after_project_plan: delta.before_project_plan,
                })
                .collect(),
            settings: invert_settings(&self.settings),
        }
    }

    pub(crate) fn changed_entities(&self) -> usize {
        self.projects.len()
            + self.topics.len()
            + self.tasks.len()
            + self.requests.len()
            + self.runs.len()
            + usize::from(!self.settings.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DomainOutcome {
    pub undo: DomainUndo,
    pub created_topic: Option<u64>,
    pub created_project: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DomainCommand {
    ProjectRename { project: u64, name: String },
    ProjectDirectory { project: u64, directory: String },
    ProjectCreate { name: String, directory: String },
    TopicCreate { project: u64, name: String },
    TopicRename { topic: u64, name: String },
    TopicMove { topic: u64, project: u64 },
    TopicArchive { topic: u64, archived: bool },
    GoalTopic { goal: u64, topic: Option<u64> },
    TaskParent { task: u64, parent: Option<u64> },
    TaskTitle { task: u64, title: String },
    TaskLabels { task: u64, labels: Vec<String> },
    TaskArchive { task: u64, archived: bool },
    RestoreArchived { task: u64 },
    ModelAgent { stage: Option<usize>, agent: String },
    UseGlobalForStage { stage: usize },
    ModelValue { stage: Option<usize>, value: String },
    ModelsToGlobal,
    ApplyModelProfile { name: String },
    CreateModelProfile { name: String },
    UpdateModelProfile { name: String },
    DeleteModelProfile { name: String },
    Publishing { config: PublishingConfig },
    PublishingAutomatic { automatic: bool },
}

impl DomainCommand {
    pub(crate) fn execute(self, store: &mut Store) -> Result<DomainOutcome, String> {
        let mut undo = DomainUndo {
            projects: Vec::new(),
            topics: Vec::new(),
            tasks: Vec::new(),
            requests: Vec::new(),
            runs: Vec::new(),
            settings: SettingsDelta::default(),
        };
        let mut created_topic = None;
        let mut created_project = None;
        match self {
            Self::ProjectRename { project, name } => {
                let before = store.project(project).cloned().ok_or("Project not found.")?;
                let run_ids: Vec<u64> = store
                    .runs()
                    .iter()
                    .filter(|run| run.project_plan == Some(project))
                    .map(|run| run.id)
                    .collect();
                let runs_before = capture_runs(store, &run_ids);
                store.rename_project(project, &name)?;
                let after = store.project(project).cloned().ok_or("Project not found.")?;
                if before.name != after.name {
                    undo.projects.push(ProjectDelta::Name {
                        id: project,
                        before: before.name,
                        after: after.name,
                    });
                }
                undo.runs = changed_runs(store, runs_before);
            }
            Self::ProjectDirectory { project, directory } => {
                let before = store.project(project).cloned().ok_or("Project not found.")?;
                store
                    .project_mut(project)
                    .ok_or("Project not found.")?
                    .directory = directory;
                let after = store.project(project).cloned().ok_or("Project not found.")?;
                if before.directory != after.directory {
                    undo.projects.push(ProjectDelta::Directory {
                        id: project,
                        before: before.directory,
                        after: after.directory,
                    });
                }
            }
            Self::ProjectCreate { name, directory } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err("A project name is required".into());
                }
                let id = store
                    .projects()
                    .iter()
                    .map(|project| project.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                let project = Project {
                    id,
                    name,
                    key: format!("P{id}"),
                    directory,
                    research_brief: String::new(),
                    kind: crate::domain::ProjectKind::User,
                };
                store.push_project(project.clone());
                undo.projects.push(ProjectDelta::Created(project));
                created_project = Some(id);
            }
            Self::TopicCreate { project, name } => {
                let id = store.create_topic(project, name)?;
                let after = store.topic(id).cloned().ok_or("Topic not found.")?;
                undo.topics.push(TopicDelta::Created(after));
                created_topic = Some(id);
            }
            Self::TopicRename { topic, name } => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.rename_topic(topic, name)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before.name != after.name {
                    undo.topics.push(TopicDelta::Name {
                        id: topic,
                        before: before.name,
                        after: after.name,
                    });
                }
            }
            Self::TopicMove { topic, project } => {
                self::move_topic(store, topic, project, &mut undo)?;
            }
            Self::TopicArchive { topic, archived } => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.set_topic_archived(topic, archived)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before.archived != after.archived {
                    undo.topics.push(TopicDelta::Archived {
                        id: topic,
                        before: before.archived,
                        after: after.archived,
                    });
                }
            }
            Self::GoalTopic { goal, topic } => {
                let before = store.task(goal).cloned().ok_or("Task not found.")?;
                if before.research.is_some() {
                    store.set_goal_topic(goal, topic)?;
                } else {
                    let topic = topic.ok_or("Choose a topic.")?;
                    store.set_root_topic(goal, topic)?;
                }
                let after = store.task(goal).cloned().ok_or("Task not found.")?;
                if before.topic_id != after.topic_id {
                    undo.tasks.push(TaskDelta::Topic {
                        id: goal,
                        before: before.topic_id,
                        after: after.topic_id,
                    });
                }
            }
            Self::TaskParent { task, parent } => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                let max_topic = store.topics().iter().map(|topic| topic.id).max().unwrap_or(0);
                store.set_parent(task, parent)?;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                if before.parent_id != after.parent_id {
                    undo.tasks.push(TaskDelta::Parent {
                        id: task,
                        before: before.parent_id,
                        after: after.parent_id,
                    });
                }
                if before.topic_id != after.topic_id {
                    undo.tasks.push(TaskDelta::Topic {
                        id: task,
                        before: before.topic_id,
                        after: after.topic_id,
                    });
                }
                for topic in store.topics().iter().filter(|topic| topic.id > max_topic) {
                    if created_topic.is_none() {
                        created_topic = Some(topic.id);
                    }
                    undo.topics.push(TopicDelta::Created(topic.clone()));
                }
            }
            Self::TaskTitle { task, title } => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                store.task_mut(task).ok_or("Task not found.")?.title = title;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                if before.title != after.title {
                    undo.tasks.push(TaskDelta::Title {
                        id: task,
                        before: before.title,
                        after: after.title,
                    });
                }
            }
            Self::TaskLabels { task, labels } => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                store.task_mut(task).ok_or("Task not found.")?.labels = labels;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                if before.labels != after.labels {
                    undo.tasks.push(TaskDelta::Labels {
                        id: task,
                        before: before.labels,
                        after: after.labels,
                    });
                }
            }
            Self::TaskArchive { task, archived } => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                if archived {
                    store.archive(task)?;
                } else {
                    store.restore(task)?;
                }
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                if before.archived != after.archived {
                    undo.tasks.push(TaskDelta::Archived {
                        id: task,
                        before: before.archived,
                        after: after.archived,
                    });
                }
            }
            Self::RestoreArchived { task } => {
                self::restore_archived(store, task, &mut undo)?;
            }
            Self::ModelAgent { stage, agent } => {
                validate_stage(stage)?;
                let before = SettingsBefore::capture(store);
                apply_model_agent(store, stage, agent)?;
                undo.settings = before.delta(store);
            }
            Self::UseGlobalForStage { stage } => {
                validate_stage(Some(stage))?;
                let before = SettingsBefore::capture(store);
                let config = store.research_config_mut();
                config.agents[stage] = None;
                config.models[stage].clear();
                undo.settings = before.delta(store);
            }
            Self::ModelValue { stage, value } => {
                validate_stage(stage)?;
                let before = SettingsBefore::capture(store);
                match stage {
                    Some(stage) => store.research_config_mut().models[stage] = value,
                    None => store.set_default_model(value),
                }
                undo.settings = before.delta(store);
            }
            Self::ModelsToGlobal => {
                let before = SettingsBefore::capture(store);
                let config = store.research_config_mut();
                config.models = Default::default();
                config.agents = Default::default();
                undo.settings = before.delta(store);
            }
            Self::ApplyModelProfile { name } => {
                let before = SettingsBefore::capture(store);
                apply_profile(store, &name);
                undo.settings = before.delta(store);
            }
            Self::CreateModelProfile { name } => {
                let before = SettingsBefore::capture(store);
                create_profile(store, &name)?;
                undo.settings = before.delta(store);
            }
            Self::UpdateModelProfile { name } => {
                let before = SettingsBefore::capture(store);
                update_profile(store, &name);
                undo.settings = before.delta(store);
            }
            Self::DeleteModelProfile { name } => {
                let before = SettingsBefore::capture(store);
                delete_profile(store, &name);
                undo.settings = before.delta(store);
            }
            Self::Publishing { config } => {
                let before = SettingsBefore::capture(store);
                store.set_report_publishing(config);
                undo.settings = before.delta(store);
            }
            Self::PublishingAutomatic { automatic } => {
                let before = SettingsBefore::capture(store);
                store.report_publishing_mut().automatic = automatic;
                undo.settings = before.delta(store);
            }
        }
        Ok(DomainOutcome {
            undo,
            created_topic,
            created_project,
        })
    }
}

/// Bounded run preimage: only the two user-context fields a topic/project move
/// can change. Report bodies, logs, sessions and outputs are never copied.
struct RunPreimage {
    id: u64,
    input_signature: String,
    project_plan: Option<u64>,
}

/// One pass over run history. Order follows `ids` so deltas stay deterministic.
fn capture_runs(store: &Store, ids: &[u64]) -> Vec<RunPreimage> {
    let wanted: std::collections::HashSet<u64> = ids.iter().copied().collect();
    let mut found = std::collections::HashMap::with_capacity(wanted.len());
    for run in store.runs() {
        if wanted.contains(&run.id) {
            found.insert(
                run.id,
                RunPreimage {
                    id: run.id,
                    input_signature: run.input_signature.clone(),
                    project_plan: run.project_plan,
                },
            );
        }
    }
    ids.iter().filter_map(|id| found.remove(id)).collect()
}

/// One pass over run history for the preimage ids, then a deterministic apply.
fn changed_runs(store: &Store, before: Vec<RunPreimage>) -> Vec<RunDelta> {
    let wanted: std::collections::HashSet<u64> = before.iter().map(|run| run.id).collect();
    let mut after_by_id = std::collections::HashMap::with_capacity(wanted.len());
    for run in store.runs() {
        if wanted.contains(&run.id) {
            after_by_id.insert(run.id, (run.input_signature.clone(), run.project_plan));
        }
    }
    before
        .into_iter()
        .filter_map(|before| {
            let (after_signature, after_plan) = after_by_id.remove(&before.id)?;
            (after_signature != before.input_signature
                || after_plan != before.project_plan)
                .then(|| RunDelta {
                    id: before.id,
                    before_input_signature: before.input_signature,
                    after_input_signature: after_signature,
                    before_project_plan: before.project_plan,
                    after_project_plan: after_plan,
                })
        })
        .collect()
}

fn move_topic(
    store: &mut Store,
    topic: u64,
    project: u64,
    undo: &mut DomainUndo,
) -> Result<(), String> {
    let topic_before = store.topic(topic).cloned().ok_or("Topic not found.")?;
    if !store.projects().iter().any(|value| value.id == project) {
        return Err("Project not found.".into());
    }
    let task_ids: Vec<u64> = store
        .tasks()
        .iter()
        .filter(|task| {
            store
                .task_topic(task.id)
                .is_some_and(|task_topic| task_topic.id == topic)
        })
        .map(|task| task.id)
        .collect();
    let task_set: std::collections::HashSet<u64> = task_ids.iter().copied().collect();
    let request_ids: Vec<u64> = store
        .research_requests()
        .iter()
        .filter(|request| {
            request.topic_id == Some(topic)
                || request.goal_ids.iter().any(|id| task_set.contains(id))
        })
        .map(|request| request.id)
        .collect();
    let request_set: std::collections::HashSet<u64> = request_ids.iter().copied().collect();
    let run_ids: Vec<u64> = {
        let mut ids = Vec::new();
        for request in store
            .research_requests()
            .iter()
            .filter(|request| request_set.contains(&request.id))
        {
            if let Some(run) = request.planning_run_id {
                ids.push(run);
            }
        }
        // Latest non-simulated result per affected task, in one history pass.
        // This mirrors the move's reusable-run selection without rescanning the
        // history per task or recomputing canonical signatures.
        let mut seen = std::collections::HashSet::new();
        for run in store.runs().iter().rev() {
            if !task_set.contains(&run.task_id) || !seen.insert(run.task_id) {
                continue;
            }
            if run.status == "Succeeded" && !run.output.trim().is_empty() {
                ids.push(run.id);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    // Targeted ownership indexes: one pass each, then O(1) lookups.
    let task_index: std::collections::HashMap<u64, &Task> =
        store.tasks().iter().map(|task| (task.id, task)).collect();
    let request_index: std::collections::HashMap<u64, &ResearchRequest> = store
        .research_requests()
        .iter()
        .map(|request| (request.id, request))
        .collect();
    let tasks_before: Vec<Task> = task_ids
        .iter()
        .filter_map(|id| task_index.get(id).map(|task| (*task).clone()))
        .collect();
    let requests_before: Vec<ResearchRequest> = request_ids
        .iter()
        .filter_map(|id| {
            request_index
                .get(id)
                .map(|request| (*request).clone())
        })
        .collect();
    let runs_before = capture_runs(store, &run_ids);
    store.move_topic_project(topic, project)?;

    let topic_after = store.topic(topic).cloned().ok_or("Topic not found.")?;
    if topic_before.project_id != topic_after.project_id {
        undo.topics.push(TopicDelta::Project {
            id: topic,
            before: topic_before.project_id,
            after: topic_after.project_id,
        });
    }
    let task_after_index: std::collections::HashMap<u64, &Task> =
        store.tasks().iter().map(|task| (task.id, task)).collect();
    let request_after_index: std::collections::HashMap<u64, &ResearchRequest> = store
        .research_requests()
        .iter()
        .map(|request| (request.id, request))
        .collect();
    for before in tasks_before {
        if let Some(after) = task_after_index.get(&before.id) {
            if before.project_id != after.project_id {
                undo.tasks.push(TaskDelta::Project {
                    id: before.id,
                    before: before.project_id,
                    after: after.project_id,
                });
            }
            if before.approved_plan != after.approved_plan {
                undo.tasks.push(TaskDelta::ApprovedPlan {
                    id: before.id,
                    before: before.approved_plan,
                    after: after.approved_plan.clone(),
                });
            }
        }
    }
    for before in requests_before {
        if let Some(after) = request_after_index.get(&before.id) {
            if before.project_id != after.project_id {
                undo.requests.push(RequestDelta::Project {
                    id: before.id,
                    before: before.project_id,
                    after: after.project_id,
                });
            }
            if before.topic_id != after.topic_id {
                undo.requests.push(RequestDelta::Topic {
                    id: before.id,
                    before: before.topic_id,
                    after: after.topic_id,
                });
            }
        }
    }
    undo.runs = changed_runs(store, runs_before);
    Ok(())
}

fn restore_archived(store: &mut Store, id: u64, undo: &mut DomainUndo) -> Result<(), String> {
    let plan = plan_archive_restore(store, id)?;
    for owner in plan {
        match owner {
            crate::archive_presentation::ArchivedOwner::Topic(topic) => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.set_topic_archived(topic, false)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before.archived != after.archived {
                    undo.topics.push(TopicDelta::Archived {
                        id: topic,
                        before: before.archived,
                        after: after.archived,
                    });
                }
            }
            crate::archive_presentation::ArchivedOwner::Task(task) => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                store.restore(task)?;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                if before.archived != after.archived {
                    undo.tasks.push(TaskDelta::Archived {
                        id: task,
                        before: before.archived,
                        after: after.archived,
                    });
                }
            }
        }
    }
    if store.task_is_archived(id) {
        return Err("Some archived ownership remains unresolved. Review the goal and its topic before retrying.".into());
    }
    Ok(())
}

fn plan_archive_restore(
    store: &Store,
    id: u64,
) -> Result<Vec<crate::archive_presentation::ArchivedOwner>, String> {
    use crate::archive_presentation::ArchivedOwner;
    let mut tasks = std::collections::HashSet::new();
    let mut topics = std::collections::HashSet::new();
    let mut plan = Vec::new();
    loop {
        let Some(owner) = planned_restore_owner(store, id, &tasks, &topics) else {
            break;
        };
        match owner {
            ArchivedOwner::Topic(topic) => {
                if !topics.insert(topic) {
                    return Err("Archive ownership could not be resolved without a cycle. No further changes were made.".into());
                }
                plan.push(ArchivedOwner::Topic(topic));
            }
            ArchivedOwner::Task(task) => {
                if !tasks.insert(task) {
                    return Err("Archive ownership could not be resolved without a cycle. No further changes were made.".into());
                }
                plan.push(ArchivedOwner::Task(task));
            }
        }
    }
    if task_is_archived_with(store, id, &tasks, &topics) {
        return Err("Some archived ownership remains unresolved. Review the goal and its topic before retrying.".into());
    }
    Ok(plan)
}

fn planned_restore_owner(
    store: &Store,
    id: u64,
    tasks: &std::collections::HashSet<u64>,
    topics: &std::collections::HashSet<u64>,
) -> Option<crate::archive_presentation::ArchivedOwner> {
    use crate::archive_presentation::ArchivedOwner;
    if !task_is_archived_with(store, id, tasks, topics) {
        return None;
    }
    if let Some(topic) = store.task_topic(id)
        && topic.archived
        && !topics.contains(&topic.id)
    {
        return Some(ArchivedOwner::Topic(topic.id));
    }
    let mut current = Some(id);
    let mut visited = std::collections::HashSet::new();
    while let Some(task) = current.and_then(|task| store.task(task)) {
        if !visited.insert(task.id) {
            break;
        }
        if task.archived && !tasks.contains(&task.id) {
            return Some(ArchivedOwner::Task(task.id));
        }
        current = task.parent_id.or_else(|| {
            task.research
                .as_ref()
                .filter(|research| research.goal_id != task.id)
                .map(|research| research.goal_id)
        });
    }
    None
}

fn task_is_archived_with(
    store: &Store,
    id: u64,
    tasks: &std::collections::HashSet<u64>,
    topics: &std::collections::HashSet<u64>,
) -> bool {
    if let Some(topic) = store.task_topic(id)
        && topic.archived
        && !topics.contains(&topic.id)
    {
        return true;
    }
    let mut current = Some(id);
    let mut visited = std::collections::HashSet::new();
    while let Some(task) = current.and_then(|task| store.task(task)) {
        if !visited.insert(task.id) {
            break;
        }
        if task.archived && !tasks.contains(&task.id) {
            return true;
        }
        current = task.parent_id.or_else(|| {
            task.research
                .as_ref()
                .filter(|research| research.goal_id != task.id)
                .map(|research| research.goal_id)
        });
    }
    false
}

fn validate_stage(stage: Option<usize>) -> Result<(), String> {
    match stage {
        Some(stage) if stage >= 4 => Err("Choose a research stage between 1 and 4.".into()),
        _ => Ok(()),
    }
}

fn apply_model_agent(store: &mut Store, stage: Option<usize>, agent: String) -> Result<(), String> {
    match stage {
        Some(stage) => {
            let config = store.research_config_mut();
            config.agents[stage] = Some(agent);
            config.models[stage].clear();
        }
        None => {
            let config = store.research_config_mut();
            for stage in 0..4 {
                if !config.models[stage].is_empty() && config.agents[stage].is_none() {
                    config.agents[stage] = Some(config.default_agent.clone());
                }
            }
            config.default_agent = agent;
            store.set_default_model(String::new());
        }
    }
    Ok(())
}

fn capture_profile(store: &Store, name: String) -> ModelProfile {
    ModelProfile {
        name,
        default_agent: store.research_config().default_agent.clone(),
        default_model: store.default_model().clone(),
        agents: store.research_config().agents.clone(),
        models: store.research_config().models.clone(),
    }
}

fn apply_profile(store: &mut Store, name: &str) {
    let Some(profile) = store
        .model_profiles()
        .iter()
        .find(|profile| profile.name == name)
        .cloned()
    else {
        return;
    };
    let config = store.research_config_mut();
    config.default_agent = profile.default_agent;
    config.agents = profile.agents;
    config.models = profile.models;
    store.set_default_model(profile.default_model);
    store.set_active_model_profile(Some(name.to_owned()));
}

fn create_profile(store: &mut Store, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a profile name.".into());
    }
    if store
        .model_profiles()
        .iter()
        .any(|profile| profile.name.eq_ignore_ascii_case(name))
    {
        return Err("A profile with this name already exists.".into());
    }
    let profile = capture_profile(store, name.to_owned());
    store.push_model_profile(profile);
    store.set_active_model_profile(Some(name.to_owned()));
    Ok(())
}

fn update_profile(store: &mut Store, name: &str) {
    let replacement = capture_profile(store, name.to_owned());
    if store
        .model_profiles()
        .iter()
        .any(|profile| profile.name == name)
    {
        *store.model_profile_mut(name).unwrap() = replacement;
    }
}

fn delete_profile(store: &mut Store, name: &str) {
    store.retain_model_profiles(|profile| profile.name != name);
    if store.active_model_profile().as_deref() == Some(name) {
        store.set_active_model_profile(None);
    }
}

fn invert_project(delta: &ProjectDelta) -> ProjectDelta {
    match delta {
        ProjectDelta::Name { id, before, after } => ProjectDelta::Name {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        ProjectDelta::Directory { id, before, after } => ProjectDelta::Directory {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        ProjectDelta::Created(value) => ProjectDelta::Deleted(value.clone()),
        ProjectDelta::Deleted(value) => ProjectDelta::Created(value.clone()),
    }
}

fn invert_topic(delta: &TopicDelta) -> TopicDelta {
    match delta {
        TopicDelta::Name { id, before, after } => TopicDelta::Name {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        TopicDelta::Project { id, before, after } => TopicDelta::Project {
            id: *id,
            before: *after,
            after: *before,
        },
        TopicDelta::Archived { id, before, after } => TopicDelta::Archived {
            id: *id,
            before: *after,
            after: *before,
        },
        TopicDelta::Created(value) => TopicDelta::Deleted(value.clone()),
        TopicDelta::Deleted(value) => TopicDelta::Created(value.clone()),
    }
}

fn invert_task(delta: &TaskDelta) -> TaskDelta {
    match delta {
        TaskDelta::Title { id, before, after } => TaskDelta::Title {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        TaskDelta::Labels { id, before, after } => TaskDelta::Labels {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        TaskDelta::Parent { id, before, after } => TaskDelta::Parent {
            id: *id,
            before: *after,
            after: *before,
        },
        TaskDelta::Topic { id, before, after } => TaskDelta::Topic {
            id: *id,
            before: *after,
            after: *before,
        },
        TaskDelta::Project { id, before, after } => TaskDelta::Project {
            id: *id,
            before: *after,
            after: *before,
        },
        TaskDelta::ApprovedPlan { id, before, after } => TaskDelta::ApprovedPlan {
            id: *id,
            before: after.clone(),
            after: before.clone(),
        },
        TaskDelta::Archived { id, before, after } => TaskDelta::Archived {
            id: *id,
            before: *after,
            after: *before,
        },
    }
}

fn invert_request(delta: &RequestDelta) -> RequestDelta {
    match delta {
        RequestDelta::Project { id, before, after } => RequestDelta::Project {
            id: *id,
            before: *after,
            after: *before,
        },
        RequestDelta::Topic { id, before, after } => RequestDelta::Topic {
            id: *id,
            before: *after,
            after: *before,
        },
    }
}

fn invert_settings(delta: &SettingsDelta) -> SettingsDelta {
    fn swap<T: Clone>(value: &Option<(T, T)>) -> Option<(T, T)> {
        value
            .as_ref()
            .map(|(before, after)| (after.clone(), before.clone()))
    }
    SettingsDelta {
        default_model: swap(&delta.default_model),
        research_config: swap(&delta.research_config),
        model_profiles: swap(&delta.model_profiles),
        active_model_profile: swap(&delta.active_model_profile),
        max_concurrent_runs: swap(&delta.max_concurrent_runs),
        max_concurrent_automations: swap(&delta.max_concurrent_automations),
        appearance: swap(&delta.appearance),
        report_publishing: swap(&delta.report_publishing),
        opencode_path: swap(&delta.opencode_path),
    }
}

fn conflict(message: &str) -> ScopedEditError {
    ScopedEditError::Conflict(message.into())
}

/// Validate every recorded preimage before writing. A conflict therefore leaves
/// the entire pre-attempt state, including dirty tracking, untouched.
fn index_map<T>(items: &[T], id: impl Fn(&T) -> u64) -> std::collections::HashMap<u64, usize> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| (id(item), index))
        .collect()
}

pub(crate) fn apply_undo(store: &mut Store, undo: &DomainUndo) -> Result<(), ScopedEditError> {
    // One index pass per collection; every validation and write below is
    // O(1) per delta instead of rescanning the collection K times.
    let project_index = index_map(store.projects(), |project| project.id);
    let topic_index = index_map(store.topics(), |topic| topic.id);
    let task_index = index_map(store.tasks(), |task| task.id);
    let request_index = index_map(store.research_requests(), |request| request.id);
    // Ordinary field Undo (title/labels/comment context) carries no run deltas;
    // do not scan a large history for them.
    let run_index = if undo.runs.is_empty() {
        std::collections::HashMap::new()
    } else {
        index_map(store.runs(), |run| run.id)
    };

    validate_projects(store, &undo.projects, &project_index)?;
    validate_topics(store, &undo.topics, &topic_index)?;
    validate_tasks(store, &undo.tasks, &task_index)?;
    validate_requests(store, &undo.requests, &request_index)?;
    for delta in &undo.runs {
        let current = run_index
            .get(&delta.id)
            .and_then(|index| store.runs().get(*index))
            .ok_or_else(|| conflict("A run from this action is no longer available."))?;
        if current.input_signature != delta.after_input_signature
            || current.project_plan != delta.after_project_plan
        {
            return Err(conflict(
                "A run's research context changed after this action. Undo would overwrite newer context.",
            ));
        }
    }
    validate_created_parents(store, undo)?;
    undo.settings.validate(store)?;

    // Apply every field write against the stable preflight indices first.
    // Structural removals are deferred so they cannot shift the positions used
    // by later writes.
    for delta in &undo.tasks {
        apply_task(store, delta, &task_index);
    }
    for delta in &undo.requests {
        apply_request(store, delta, &request_index);
    }
    for delta in &undo.topics {
        apply_topic_changed(store, delta, &topic_index);
    }
    for delta in &undo.projects {
        apply_project_changed(store, delta, &project_index);
    }
    for delta in &undo.runs {
        if let Some(index) = run_index.get(&delta.id)
            && let Some(mut run) = store.run_at_index_mut(*index)
        {
            run.input_signature = delta.before_input_signature.clone();
            run.project_plan = delta.before_project_plan;
        }
    }
    undo.settings.apply(store);
    // Structural changes last: no indexed field write follows a removal.
    for delta in &undo.topics {
        apply_topic_structural(store, delta);
    }
    for delta in &undo.projects {
        apply_project_structural(store, delta);
    }
    Ok(())
}

fn validate_projects(
    store: &Store,
    deltas: &[ProjectDelta],
    index: &std::collections::HashMap<u64, usize>,
) -> Result<(), ScopedEditError> {
    let message = "The project changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let current = index
            .get(&delta.key())
            .and_then(|position| store.projects().get(*position));
        match delta {
            ProjectDelta::Name { after, .. } => {
                if current.map(|project| &project.name) != Some(after) {
                    return Err(conflict(message));
                }
            }
            ProjectDelta::Directory { after, .. } => {
                if current.map(|project| &project.directory) != Some(after) {
                    return Err(conflict(message));
                }
            }
            ProjectDelta::Created(created) => {
                if current != Some(created) {
                    return Err(conflict(message));
                }
            }
            ProjectDelta::Deleted(before) => {
                if let Some(project) = current
                    && project != before
                {
                    return Err(conflict(message));
                }
            }
        }
    }
    Ok(())
}

fn validate_topics(
    store: &Store,
    deltas: &[TopicDelta],
    index: &std::collections::HashMap<u64, usize>,
) -> Result<(), ScopedEditError> {
    let message = "The topic changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let current = index
            .get(&delta.key())
            .and_then(|position| store.topics().get(*position));
        match delta {
            TopicDelta::Name { after, .. } => {
                if current.map(|topic| &topic.name) != Some(after) {
                    return Err(conflict(message));
                }
            }
            TopicDelta::Project { after, .. } => {
                if current.map(|topic| topic.project_id) != Some(*after) {
                    return Err(conflict(message));
                }
            }
            TopicDelta::Archived { after, .. } => {
                if current.map(|topic| topic.archived) != Some(*after) {
                    return Err(conflict(message));
                }
            }
            TopicDelta::Created(created) => {
                if current != Some(created) {
                    return Err(conflict(message));
                }
            }
            TopicDelta::Deleted(before) => {
                if let Some(topic) = current
                    && topic != before
                {
                    return Err(conflict(message));
                }
            }
        }
    }
    Ok(())
}

fn validate_tasks(
    store: &Store,
    deltas: &[TaskDelta],
    index: &std::collections::HashMap<u64, usize>,
) -> Result<(), ScopedEditError> {
    let message = "The task changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let Some(task) = index
            .get(&delta.key())
            .and_then(|position| store.tasks().get(*position))
        else {
            return Err(conflict(message));
        };
        let matches = match delta {
            TaskDelta::Title { after, .. } => &task.title == after,
            TaskDelta::Labels { after, .. } => &task.labels == after,
            TaskDelta::Parent { after, .. } => task.parent_id == *after,
            TaskDelta::Topic { after, .. } => task.topic_id == *after,
            TaskDelta::Project { after, .. } => task.project_id == *after,
            TaskDelta::ApprovedPlan { after, .. } => &task.approved_plan == after,
            TaskDelta::Archived { after, .. } => task.archived == *after,
        };
        if !matches {
            return Err(conflict(message));
        }
    }
    Ok(())
}

fn validate_requests(
    store: &Store,
    deltas: &[RequestDelta],
    index: &std::collections::HashMap<u64, usize>,
) -> Result<(), ScopedEditError> {
    let message = "The research request changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let Some(request) = index
            .get(&delta.key())
            .and_then(|position| store.research_requests().get(*position))
        else {
            return Err(conflict(message));
        };
        let matches = match delta {
            RequestDelta::Project { after, .. } => request.project_id == *after,
            RequestDelta::Topic { after, .. } => request.topic_id == *after,
        };
        if !matches {
            return Err(conflict(message));
        }
    }
    Ok(())
}

fn apply_project_changed(
    store: &mut Store,
    delta: &ProjectDelta,
    index: &std::collections::HashMap<u64, usize>,
) {
    let (ProjectDelta::Name { before, .. } | ProjectDelta::Directory { before, .. }) = delta else {
        return;
    };
    // `tracked_mut_index` re-checks identity, so a stale index can never write
    // the wrong project.
    if let Some(project) = index
        .get(&delta.key())
        .and_then(|position| store.project_at_index_mut(*position))
    {
        match delta {
            ProjectDelta::Name { .. } => project.name = before.clone(),
            ProjectDelta::Directory { .. } => project.directory = before.clone(),
            _ => unreachable!("matched changed project"),
        }
    }
}

fn apply_project_structural(store: &mut Store, delta: &ProjectDelta) {
    match delta {
        ProjectDelta::Created(value) => {
            store.remove_project(value.id);
        }
        ProjectDelta::Deleted(value) => {
            if store.project(value.id).is_none() {
                store.push_project(value.clone());
            }
        }
        ProjectDelta::Name { .. } | ProjectDelta::Directory { .. } => {}
    }
}

fn apply_topic_changed(
    store: &mut Store,
    delta: &TopicDelta,
    index: &std::collections::HashMap<u64, usize>,
) {
    let before = match delta {
        TopicDelta::Name { before, .. } => before,
        TopicDelta::Project { before, .. } => {
            if let Some(topic) = index
                .get(&delta.key())
                .and_then(|position| store.topic_at_index_mut(*position))
            {
                topic.project_id = *before;
            }
            return;
        }
        TopicDelta::Archived { before, .. } => {
            if let Some(topic) = index
                .get(&delta.key())
                .and_then(|position| store.topic_at_index_mut(*position))
            {
                topic.archived = *before;
            }
            return;
        }
        TopicDelta::Created(_) | TopicDelta::Deleted(_) => return,
    };
    if let Some(topic) = index
        .get(&delta.key())
        .and_then(|position| store.topic_at_index_mut(*position))
    {
        topic.name = before.clone();
    }
}

fn apply_topic_structural(store: &mut Store, delta: &TopicDelta) {
    match delta {
        TopicDelta::Created(value) => {
            store.remove_topic(value.id);
        }
        TopicDelta::Deleted(value) => {
            if store.topic(value.id).is_none() {
                store.push_topic(value.clone());
            }
        }
        _ => {}
    }
}

fn apply_task(
    store: &mut Store,
    delta: &TaskDelta,
    index: &std::collections::HashMap<u64, usize>,
) {
    let Some(task) = index
        .get(&delta.key())
        .and_then(|position| store.task_at_index_mut(*position))
    else {
        return;
    };
    match delta {
        TaskDelta::Title { before, .. } => task.title = before.clone(),
        TaskDelta::Labels { before, .. } => task.labels = before.clone(),
        TaskDelta::Parent { before, .. } => task.parent_id = *before,
        TaskDelta::Topic { before, .. } => task.topic_id = *before,
        TaskDelta::Project { before, .. } => task.project_id = *before,
        TaskDelta::ApprovedPlan { before, .. } => task.approved_plan = before.clone(),
        TaskDelta::Archived { before, .. } => task.archived = *before,
    }
}

fn apply_request(
    store: &mut Store,
    delta: &RequestDelta,
    index: &std::collections::HashMap<u64, usize>,
) {
    let Some(request) = index
        .get(&delta.key())
        .and_then(|position| store.request_at_index_mut(*position))
    else {
        return;
    };
    match delta {
        RequestDelta::Project { before, .. } => request.project_id = *before,
        RequestDelta::Topic { before, .. } => request.topic_id = *before,
    }
}

/// A created parent may only be removed when nothing outside this inverse still
/// references it. Children this same inverse restores away are evaluated at
/// their recorded `before` value.
fn validate_created_parents(store: &Store, undo: &DomainUndo) -> Result<(), ScopedEditError> {
    for delta in &undo.topics {
        let TopicDelta::Created(created) = delta else {
            continue;
        };
        let referenced = store.tasks().iter().any(|task| {
            task_topic_after_undo(store, task.id, undo) == Some(created.id)
        }) || store
            .research_requests()
            .iter()
            .any(|request| request_topic_after_undo(store, request.id, undo) == Some(created.id));
        if referenced {
            return Err(conflict(
                "The newly created topic now contains work. Undo will not remove it.",
            ));
        }
    }
    for delta in &undo.projects {
        let ProjectDelta::Created(created) = delta else {
            continue;
        };
        let referenced = store
            .topics()
            .iter()
            .any(|topic| topic_project_after_undo(store, topic.id, undo) == created.id)
            || store
                .tasks()
                .iter()
                .any(|task| task_project_after_undo(store, task.id, undo) == created.id)
            || store
                .research_requests()
                .iter()
                .any(|request| request_project_after_undo(store, request.id, undo) == created.id);
        if referenced {
            return Err(conflict(
                "The newly created project now contains work. Undo will not remove it.",
            ));
        }
    }
    Ok(())
}

fn task_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> Option<Task> {
    let mut task = store.task(id)?.clone();
    for delta in &undo.tasks {
        if delta.key() != id {
            continue;
        }
        match delta {
            TaskDelta::Title { before, .. } => task.title = before.clone(),
            TaskDelta::Labels { before, .. } => task.labels = before.clone(),
            TaskDelta::Parent { before, .. } => task.parent_id = *before,
            TaskDelta::Topic { before, .. } => task.topic_id = *before,
            TaskDelta::Project { before, .. } => task.project_id = *before,
            TaskDelta::ApprovedPlan { before, .. } => task.approved_plan = before.clone(),
            TaskDelta::Archived { before, .. } => task.archived = *before,
        }
    }
    Some(task)
}

fn task_topic_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> Option<u64> {
    let project = task_after_undo(store, id, undo)?.project_id;
    let mut current = Some(id);
    let mut visited = std::collections::HashSet::new();
    loop {
        let task = task_after_undo(store, current?, undo)?;
        if task.project_id != project || !visited.insert(task.id) {
            return None;
        }
        if let Some(research) = &task.research {
            if research.kind == crate::research::ResearchKind::Goal {
                return store
                    .topics()
                    .iter()
                    .find(|topic| Some(topic.id) == task.topic_id && topic.project_id == project)
                    .map(|topic| topic.id);
            }
            current = Some(research.goal_id);
        } else if let Some(parent) = task.parent_id {
            current = Some(parent);
        } else {
            return store
                .topics()
                .iter()
                .find(|topic| Some(topic.id) == task.topic_id && topic.project_id == project)
                .map(|topic| topic.id);
        }
    }
}

fn request_topic_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> Option<u64> {
    let request = store
        .research_requests()
        .iter()
        .find(|request| request.id == id)?;
    let mut topic = request.topic_id;
    for delta in &undo.requests {
        if delta.key() == id
            && let RequestDelta::Topic { before, .. } = delta
        {
            topic = *before;
        }
    }
    topic
}

fn topic_project_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> u64 {
    let mut project = store.topic(id).map(|topic| topic.project_id).unwrap_or(u64::MAX);
    for delta in &undo.topics {
        if delta.key() == id
            && let TopicDelta::Project { before, .. } = delta
        {
            project = *before;
        }
    }
    project
}

fn task_project_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> u64 {
    task_after_undo(store, id, undo)
        .map(|task| task.project_id)
        .unwrap_or(u64::MAX)
}

fn request_project_after_undo(store: &Store, id: u64, undo: &DomainUndo) -> u64 {
    let mut project = store
        .research_requests()
        .iter()
        .find(|request| request.id == id)
        .map(|request| request.project_id)
        .unwrap_or(u64::MAX);
    for delta in &undo.requests {
        if delta.key() == id
            && let RequestDelta::Project { before, .. } = delta
        {
            project = *before;
        }
    }
    project
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Stage, Store};

    fn pending(
        store: &Store,
    ) -> (
        Vec<crate::persistence_journal::EntityChange>,
        Vec<crate::persistence_journal::MetadataDelta>,
    ) {
        (
            store.pending_entity_changes().unwrap(),
            store.pending_metadata_deltas(),
        )
    }

    #[test]
    fn topic_rename_inverse_restores_exact_topic() {
        let mut store = Store::empty();
        let topic = store.create_topic(1, "Original").unwrap();
        let other = store.create_topic(1, "Other").unwrap();
        let outcome = DomainCommand::TopicRename {
            topic,
            name: "Renamed".into(),
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(store.topic(topic).unwrap().name, "Renamed");
        apply_undo(&mut store, &outcome.undo).unwrap();
        assert_eq!(store.topic(topic).unwrap().name, "Original");
        assert_eq!(store.topic(other).unwrap().name, "Other");
    }

    #[test]
    fn field_inverse_survives_unrelated_task_edit() {
        let mut store = Store::empty();
        let task = store.create_task("Task", 1, Stage::Backlog, None).unwrap();
        let outcome = DomainCommand::TaskLabels {
            task,
            labels: vec!["Context".into()],
        }
        .execute(&mut store)
        .unwrap();
        // A later title edit is unrelated to the recorded labels field.
        store.task_mut(task).unwrap().title = "Later title".into();
        apply_undo(&mut store, &outcome.undo).unwrap();
        assert!(store.task(task).unwrap().labels.is_empty());
        assert_eq!(store.task(task).unwrap().title, "Later title");
    }

    #[test]
    fn inverse_conflicts_and_preserves_state_when_same_field_changes_again() {
        let mut store = Store::empty();
        let topic = store.create_topic(1, "Original").unwrap();
        let outcome = DomainCommand::TopicRename {
            topic,
            name: "Renamed".into(),
        }
        .execute(&mut store)
        .unwrap();
        store.rename_topic(topic, "Later edit").unwrap();
        let before_attempt = store.clone();
        let before_pending = pending(&store);
        assert!(matches!(
            apply_undo(&mut store, &outcome.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert_eq!(pending(&store), before_pending);
        assert_eq!(store.topic(topic).unwrap().name, "Later edit");
    }

    #[test]
    fn same_inverse_created_topic_is_removed_with_its_restored_child() {
        let mut store = Store::empty();
        let parent = store.create_task("Parent", 1, Stage::Backlog, None).unwrap();
        let task = store.create_task("Task", 1, Stage::Backlog, None).unwrap();
        store.set_parent(task, Some(parent)).unwrap();
        let outcome = DomainCommand::TaskParent { task, parent: None }
            .execute(&mut store)
            .unwrap();
        let created = outcome.created_topic.expect("a topic was created");
        assert_eq!(store.task(task).unwrap().topic_id, Some(created));
        apply_undo(&mut store, &outcome.undo).unwrap();
        assert!(store.topic(created).is_none());
        assert_eq!(store.task(task).unwrap().topic_id, None);
        assert_eq!(store.task(task).unwrap().parent_id, Some(parent));
    }

    #[test]
    fn later_unrelated_child_blocks_removal_of_created_topic() {
        let mut store = Store::empty();
        let outcome = DomainCommand::TopicCreate {
            project: 1,
            name: "Fresh".into(),
        }
        .execute(&mut store)
        .unwrap();
        let topic = outcome.created_topic.unwrap();
        let child = store
            .create_task("Child", 1, Stage::Backlog, None)
            .unwrap();
        store.set_root_topic(child, topic).unwrap();
        let before_attempt = store.clone();
        let before_pending = pending(&store);
        assert!(matches!(
            apply_undo(&mut store, &outcome.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert_eq!(pending(&store), before_pending);
        assert!(store.topic(topic).is_some());
    }

    #[test]
    fn invalid_stage_index_is_rejected_before_mutation() {
        let mut store = Store::empty();
        store.set_default_model("provider/base".to_string());
        let before = store.clone();
        let before_pending = pending(&store);
        assert!(DomainCommand::UseGlobalForStage { stage: 4 }
            .execute(&mut store)
            .is_err());
        assert_eq!(store, before);
        assert_eq!(pending(&store), before_pending);
        assert!(DomainCommand::ModelAgent {
            stage: Some(usize::MAX),
            agent: "Codex".into(),
        }
        .execute(&mut store)
        .is_err());
        assert_eq!(store, before);
        assert_eq!(pending(&store), before_pending);
    }

    #[test]
    fn cyclic_archive_ownership_terminates_without_partial_state() {
        let mut store = Store::empty();
        let a = store.create_task("A", 1, Stage::Backlog, None).unwrap();
        let b = store.create_task("B", 1, Stage::Backlog, None).unwrap();
        {
            let task = store.task_mut(a).unwrap();
            task.parent_id = Some(b);
            task.archived = true;
        }
        {
            let task = store.task_mut(b).unwrap();
            task.parent_id = Some(a);
            task.archived = true;
        }
        let before = store.clone();
        let before_pending = pending(&store);
        match (DomainCommand::RestoreArchived { task: a }).execute(&mut store) {
            Ok(outcome) => {
                assert!(!store.task(a).unwrap().archived);
                assert!(!store.task(b).unwrap().archived);
                apply_undo(&mut store, &outcome.undo).unwrap();
                assert!(store.task(a).unwrap().archived);
                assert!(store.task(b).unwrap().archived);
            }
            Err(_) => {
                assert_eq!(store, before);
                assert_eq!(pending(&store), before_pending);
            }
        }
    }

    const PLAN: &str = r#"{"tasks":[
        {"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},
        {"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},
        {"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}
    ]}"#;

    fn research_fixture() -> (Store, u64, u64, Vec<u64>) {
        let mut store = Store::empty();
        store.project_mut(1).unwrap().directory = "/tmp/t13-scoped-domain".into();
        store.research_config_mut().models = [
            "fixture/planner",
            "fixture/collector",
            "fixture/analyst",
            "fixture/adviser",
        ]
        .map(String::from);
        let topic = store.create_topic(1, "Topic").unwrap();
        let goal = store
            .create_research_goal("Goal".into(), "Brief".into(), 1)
            .unwrap();
        store.set_goal_topic(goal, Some(topic)).unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.set_stage(goal, Stage::Planned).unwrap();
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        // Applying the plan invalidates any earlier approval; reapprove the
        // complete goal before preparing child runs.
        store.set_stage(goal, Stage::Planned).unwrap();
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
        for step in &steps {
            store.set_stage(*step, Stage::Planned).unwrap();
            let mut run = store.prepare_run(*step).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Completed report for {step}");
            run.input_signature = store
                .research_signature(*step)
                .expect("the fixture has reusable inputs");
            store.push_run(run);
        }
        let destination = Project {
            id: 2,
            name: "Destination".into(),
            key: "P2".into(),
            directory: "/tmp/destination".into(),
            research_brief: String::new(),
            kind: crate::domain::ProjectKind::User,
        };
        store.push_project(destination);
        (store, topic, goal, steps)
    }

    #[test]
    fn move_topic_refreshes_exact_prospective_signatures() {
        let (mut store, topic, goal, steps) = research_fixture();
        let outcome = DomainCommand::TopicMove { topic, project: 2 }
            .execute(&mut store)
            .unwrap();
        let expected_goal = store.goal_plan_signature(goal);
        assert_eq!(store.task(goal).unwrap().approved_plan, expected_goal);
        for step in &steps {
            let signature = store.research_signature(*step);
            let run = store
                .runs()
                .iter()
                .rev()
                .find(|run| run.task_id == *step && run.status == "Succeeded")
                .unwrap();
            assert_eq!(
                signature.as_ref(),
                Some(&run.input_signature),
                "prospective signature must match the real signature for {step}"
            );
            assert!(
                store.research_result(*step).is_some(),
                "an already valid completed result must stay reusable after the move"
            );
        }
        apply_undo(&mut store, &outcome.undo).unwrap();
        assert_eq!(store.topic(topic).unwrap().project_id, 1);
        let restored_goal = store.goal_plan_signature(goal);
        assert_eq!(store.task(goal).unwrap().approved_plan, restored_goal);
        for step in &steps {
            assert_eq!(store.task(*step).unwrap().project_id, 1);
            assert!(
                store.research_result(*step).is_some(),
                "Undo must restore a reusable result for {step}"
            );
        }
    }

    #[test]
    fn malformed_dependency_cycle_is_rejected_before_move_writes() {
        let (mut store, topic, _goal, steps) = research_fixture();
        {
            let mut first = store.task_mut(steps[0]).unwrap();
            first.research.as_mut().unwrap().depends_on = vec![steps[1]];
        }
        {
            let mut second = store.task_mut(steps[1]).unwrap();
            second.research.as_mut().unwrap().depends_on = vec![steps[0]];
        }
        let before = store.clone();
        let before_pending = pending(&store);
        assert!(store.move_topic_project(topic, 2).is_err());
        assert_eq!(store, before);
        assert_eq!(pending(&store), before_pending);
        assert_eq!(store.topic(topic).unwrap().project_id, 1);
    }

    fn history_run(id: u64, task: u64) -> AgentRun {
        AgentRun {
            id,
            task_id: task,
            task_title: "History".into(),
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
            stage: Stage::Done,
            status: "Succeeded".into(),
            logs: Vec::new(),
            output: String::new(),
            attachments: Vec::new(),
            research_kind: None,
            sources: Vec::new(),
            input_signature: String::new(),
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
        }
    }

    /// Many independent research steps with one final report. Built directly so
    /// the fixture can exceed the planner's 12-step authoring limit while still
    /// using the real topic/result/approval semantics.
    fn bulk_research_fixture(affected: usize) -> (Store, u64, u64, Vec<u64>) {
        assert!(affected >= 2);
        let mut store = Store::empty();
        store.project_mut(1).unwrap().directory = "/tmp/t13-scoped-domain".into();
        store.research_config_mut().models = [
            "fixture/planner",
            "fixture/collector",
            "fixture/analyst",
            "fixture/adviser",
        ]
        .map(String::from);
        let topic = store.create_topic(1, "Topic").unwrap();
        let goal = store
            .create_research_goal("Goal".into(), "Brief".into(), 1)
            .unwrap();
        store.set_goal_topic(goal, Some(topic)).unwrap();
        store
            .task_mut(goal)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .requires_approval = true;
        store.set_stage(goal, Stage::Planned).unwrap();
        let mut steps = Vec::with_capacity(affected);
        for index in 0..affected {
            let id = goal + 1 + index as u64;
            let kind = if index + 1 == affected {
                crate::research::ResearchKind::Advise
            } else {
                crate::research::ResearchKind::Collect
            };
            store.push_task(Task {
                id,
                created_at: String::new(),
                automation_id: None,
                title: format!("Step {index}"),
                description: format!("Step {index}"),
                project_id: 1,
                topic_id: None,
                approved_plan: None,
                stage: Stage::Planned,
                priority: crate::domain::Priority::None,
                labels: Vec::new(),
                cycle: None,
                bucket: String::new(),
                due_date: String::new(),
                parent_id: None,
                comments: Vec::new(),
                activity: Vec::new(),
                archived: false,
                attachments: Vec::new(),
                research: Some(crate::research::ResearchTask {
                    requires_approval: false,
                    kind,
                    goal_id: goal,
                    depends_on: Vec::new(),
                    space_name: String::new(),
                }),
            });
            steps.push(id);
        }
        // Raw fixture pushes bypass id allocation; reserve the next id so a
        // later history task cannot collide with a step identity.
        store.set_next_id(goal + affected as u64 + 1);
        store.task_mut(goal).unwrap().approved_plan = store.goal_plan_signature(goal);
        let destination = Project {
            id: 2,
            name: "Destination".into(),
            key: "P2".into(),
            directory: "/tmp/destination".into(),
            research_brief: String::new(),
            kind: crate::domain::ProjectKind::User,
        };
        store.push_project(destination);
        (store, topic, goal, steps)
    }

    fn scale_case(
        affected: usize,
        total_history: usize,
    ) -> (
        std::time::Duration,
        std::time::Duration,
        std::time::Duration,
        std::time::Duration,
        u128,
        usize,
    ) {
        let (mut store, topic, goal, steps) = bulk_research_fixture(affected);
        let mut runs: Vec<AgentRun> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let mut run = history_run(1_000_000 + index as u64, *step);
                run.status = "Succeeded".into();
                run.output = format!("Completed report for {step}");
                run.input_signature = store
                    .research_signature(*step)
                    .expect("bulk fixture step has reusable inputs");
                run
            })
            .collect();
        let history_task = store
            .create_task("History task", 1, Stage::Done, None)
            .unwrap();
        let planning_run_id = 2_000_000u64;
        let mut planning = history_run(planning_run_id, history_task);
        planning.project_plan = Some(1);
        runs.push(planning);
        let mut next = 3_000_000u64;
        while runs.len() < total_history {
            runs.push(history_run(next, history_task));
            next += 1;
        }
        let mut live = history_run(4_000_000, history_task);
        live.status = "Running".into();
        live.output = "Kept runtime body".into();
        live.logs = vec!["[run] live log that Undo must not touch".into()];
        let live_snapshot = live.clone();
        runs.push(live);

        let mut fresh = Store::empty();
        for project in store.projects() {
            if let Some(existing) = fresh.project_mut(project.id) {
                *existing = project.clone();
            } else {
                fresh.push_project(project.clone());
            }
        }
        for topic_value in store.topics() {
            fresh.push_topic(topic_value.clone());
        }
        for task in store.tasks() {
            fresh.push_task(task.clone());
        }
        for request in store.research_requests() {
            fresh.push_request(request.clone());
        }
        fresh.push_request(ResearchRequest {
            id: 1,
            project_id: 1,
            topic_id: Some(topic),
            automation_id: None,
            expected_result: None,
            revision: 1,
            provisional_topic_name: None,
            input: "Plan".into(),
            submitted_at: String::new(),
            planning_run_id: Some(planning_run_id),
            goal_ids: Vec::new(),
            brief: None,
            understanding_required: false,
            understanding: None,
            understanding_confirmed: false,
            understanding_feedback: String::new(),
            understanding_source_signature: String::new(),
            understanding_turns: Vec::new(),
            understanding_draft: String::new(),
        });
        fresh.research_config_mut().models = store.research_config().models.clone();
        fresh.set_default_model(store.default_model().clone());
        let mut store = match fresh.with_loaded_tracked(runs, Vec::new(), Vec::new()) {
            Ok(restored) => restored,
            Err((_, error)) => panic!("scale fixture failed: {error}"),
        };
        assert_eq!(store.runs().len(), total_history + 1);
        for step in &steps {
            assert!(
                store.research_result(*step).is_some(),
                "fixture result must be valid before the move"
            );
        }

        let signature_started = std::time::Instant::now();
        for step in &steps {
            let _ = store.research_signature(*step);
        }
        let signature_elapsed = signature_started.elapsed();
        let blockers_started = std::time::Instant::now();
        let _ = store.topic_move_blockers(topic, None);
        let blockers_elapsed = blockers_started.elapsed();

        let mut probe = store.clone();
        let core_started = std::time::Instant::now();
        probe.move_topic_project(topic, 2).unwrap();
        let core_elapsed = core_started.elapsed();

        let started = std::time::Instant::now();
        let outcome = DomainCommand::TopicMove { topic, project: 2 }
            .execute(&mut store)
            .unwrap_or_else(|error| panic!("scale move failed: {error}"));
        let move_elapsed = started.elapsed();
        for step in &steps {
            assert!(
                store.research_result(*step).is_some(),
                "affected result {step} stays valid after the move"
            );
        }
        let undo_started = std::time::Instant::now();
        apply_undo(&mut store, &outcome.undo).unwrap();
        let undo_elapsed = undo_started.elapsed();
        for step in &steps {
            assert!(
                store.research_result(*step).is_some(),
                "Undo restores reusable result for {step}"
            );
        }
        assert_eq!(
            store.task(goal).unwrap().approved_plan,
            store.goal_plan_signature(goal)
        );
        let live_after = store
            .runs()
            .iter()
            .find(|run| run.id == 4_000_000)
            .unwrap();
        assert_eq!(live_after.status, live_snapshot.status);
        assert_eq!(live_after.output, live_snapshot.output);
        assert_eq!(live_after.logs, live_snapshot.logs);
        (
            move_elapsed,
            undo_elapsed,
            core_elapsed,
            blockers_elapsed,
            signature_elapsed.as_micros(),
            outcome.undo.changed_entities(),
        )
    }

    #[test]
    fn large_affected_topic_move_scales_across_workloads() {
        let history = 43_200usize;
        for affected in [3usize, 240, 800] {
            let (move_elapsed, undo_elapsed, core_elapsed, blockers, signature_us, deltas) =
                scale_case(affected, history);
            eprintln!(
                "scoped_domain scale: affected={affected} history={history} deltas={deltas} canonical_signature_scan={signature_us}us blockers={blockers:?} core_move={core_elapsed:?} command_move={move_elapsed:?} undo={undo_elapsed:?}"
            );
        }
    }

    #[test]
    fn large_history_move_and_undo_stay_bounded_and_preserve_runtime() {
        let (mut store, topic, goal, steps) = research_fixture();
        let history_task = store
            .create_task("History task", 1, Stage::Done, None)
            .unwrap();
        let mut runs = store.runs().to_vec();
        for index in 0..43_200u64 {
            runs.push(history_run(100_000 + index, history_task));
        }
        let mut live = history_run(999_999, history_task);
        live.status = "Running".into();
        live.output = "Kept runtime body".into();
        live.logs = vec!["[run] live log that Undo must not touch".into()];
        let live_snapshot = live.clone();
        runs.push(live);
        // Build a fresh store with a clean bulk-loaded history, then copy the
        // small task/project/topic request graph needed by the move.
        let mut fresh = Store::empty();
        for project in store.projects() {
            if let Some(existing) = fresh.project_mut(project.id) {
                *existing = project.clone();
            } else {
                fresh.push_project(project.clone());
            }
        }
        for topic_value in store.topics() {
            fresh.push_topic(topic_value.clone());
        }
        for task in store.tasks() {
            fresh.push_task(task.clone());
        }
        for request in store.research_requests() {
            fresh.push_request(request.clone());
        }
        fresh.research_config_mut().models = store.research_config().models.clone();
        fresh.set_default_model(store.default_model().clone());
        store = match fresh.with_loaded_tracked(runs, Vec::new(), Vec::new()) {
            Ok(restored) => restored,
            Err((_, error)) => panic!("large history fixture failed: {error}"),
        };
        assert_eq!(store.runs().len(), 43_204);
        for step in &steps {
            assert!(
                store.research_result(*step).is_some(),
                "fixture result must be valid before the move"
            );
        }

        let started = std::time::Instant::now();
        let outcome = DomainCommand::TopicMove { topic, project: 2 }
            .execute(&mut store)
            .unwrap_or_else(|error| panic!("large-history move failed: {error}"));
        let move_elapsed = started.elapsed();
        for step in &steps {
            if store.research_result(*step).is_none() {
                let signature = store.research_signature(*step);
                let run = store
                    .runs()
                    .iter()
                    .rev()
                    .find(|run| run.task_id == *step && run.status != "Simulated");
                panic!("step {step} invalid after move: signature={signature:?} run={:?}", run.map(|run| (&run.input_signature, &run.status, run.output.len())));
            }
        }
        let live_after = store
            .runs()
            .iter()
            .find(|run| run.id == 999_999)
            .unwrap();
        assert_eq!(live_after.status, live_snapshot.status);
        assert_eq!(live_after.output, live_snapshot.output);
        assert_eq!(live_after.logs, live_snapshot.logs);

        let started = std::time::Instant::now();
        apply_undo(&mut store, &outcome.undo).unwrap();
        let undo_elapsed = started.elapsed();
        assert_eq!(store.topic(topic).unwrap().project_id, 1);
        assert_eq!(store.task(goal).unwrap().approved_plan, store.goal_plan_signature(goal));
        for step in &steps {
            assert!(
                store.research_result(*step).is_some(),
                "Undo restores reusable results with 43,200 history records"
            );
        }
        let live_after = store
            .runs()
            .iter()
            .find(|run| run.id == 999_999)
            .unwrap();
        assert_eq!(live_after.status, live_snapshot.status);
        assert_eq!(live_after.output, live_snapshot.output);
        assert_eq!(live_after.logs, live_snapshot.logs);
        eprintln!(
            "scoped_domain large-history: move={move_elapsed:?} undo={undo_elapsed:?} history=43200"
        );
    }

    #[test]
    fn structural_removals_are_deferred_after_indexed_field_writes() {
        let mut store = Store::empty();
        // Created entities sit EARLIER in their vectors than the editable
        // entities, so a removal shifts the editable index. The old
        // remove-before-write order would miss the later field write.
        let created = store.create_topic(1, "Created").unwrap();
        let kept = store.create_topic(1, "After").unwrap();
        let created_project = Project {
            id: 9,
            name: "Created project".into(),
            key: "P9".into(),
            directory: String::new(),
            research_brief: String::new(),
            kind: crate::domain::ProjectKind::User,
        };
        store.push_project(created_project.clone());
        let kept_project = Project {
            id: 10,
            name: "After".into(),
            key: "P10".into(),
            directory: String::new(),
            research_brief: String::new(),
            kind: crate::domain::ProjectKind::User,
        };
        store.push_project(kept_project.clone());
        let created_topic = store.topic(created).unwrap().clone();
        let undo = DomainUndo {
            projects: vec![
                ProjectDelta::Created(created_project),
                ProjectDelta::Name {
                    id: kept_project.id,
                    before: "Before".into(),
                    after: kept_project.name.clone(),
                },
            ],
            topics: vec![
                TopicDelta::Created(created_topic),
                TopicDelta::Name {
                    id: kept,
                    before: "Before".into(),
                    after: "After".into(),
                },
            ],
            tasks: Vec::new(),
            requests: Vec::new(),
            runs: Vec::new(),
            settings: SettingsDelta::default(),
        };
        apply_undo(&mut store, &undo).unwrap();
        assert!(store.topic(created).is_none());
        assert!(store.project(9).is_none());
        assert_eq!(store.topic(kept).unwrap().name, "Before");
        assert_eq!(store.project(10).unwrap().name, "Before");
    }

    #[test]
    fn settings_conflict_validates_all_fields_before_any_entity_rollback() {
        let mut store = Store::empty();
        store.set_default_model("provider/after".to_string());
        store.set_appearance(Appearance::Dark);
        let topic = store.create_topic(1, "After").unwrap();
        let mut undo = DomainUndo {
            projects: Vec::new(),
            topics: vec![TopicDelta::Name {
                id: topic,
                before: "Before".into(),
                after: "After".into(),
            }],
            tasks: Vec::new(),
            requests: Vec::new(),
            runs: Vec::new(),
            settings: SettingsDelta {
                default_model: Some(("provider/before".into(), "provider/after".into())),
                appearance: Some((Appearance::Light, Appearance::Dark)),
                ..Default::default()
            },
        };
        // The second settings field was changed again: the whole inverse must be
        // rejected before the topic field or the first setting is touched.
        store.set_appearance(Appearance::Light);
        let before_attempt = store.clone();
        let before_pending = pending(&store);
        assert!(matches!(
            apply_undo(&mut store, &undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert_eq!(pending(&store), before_pending);
        assert_eq!(store.topic(topic).unwrap().name, "After");
        assert_eq!(store.default_model(), "provider/after");
        assert_eq!(store.appearance(), Appearance::Light);
        undo.settings.appearance = None;
        apply_undo(&mut store, &undo).unwrap();
        assert_eq!(store.topic(topic).unwrap().name, "Before");
        assert_eq!(store.default_model(), "provider/before");
        assert_eq!(store.appearance(), Appearance::Light);
    }
}
