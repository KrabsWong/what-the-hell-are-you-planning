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

fn capture_runs(store: &Store, ids: &[u64]) -> Vec<AgentRun> {
    ids.iter()
        .filter_map(|id| store.runs().iter().find(|run| run.id == *id).cloned())
        .collect()
}

fn changed_runs(store: &Store, before: Vec<AgentRun>) -> Vec<RunDelta> {
    before
        .into_iter()
        .filter_map(|before| {
            store
                .runs()
                .iter()
                .find(|run| run.id == before.id)
                .filter(|after| {
                    after.input_signature != before.input_signature
                        || after.project_plan != before.project_plan
                })
                .map(|after| RunDelta::capture(&before, after))
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
    let request_ids: Vec<u64> = store
        .research_requests()
        .iter()
        .filter(|request| {
            request.topic_id == Some(topic)
                || request.goal_ids.iter().any(|id| task_ids.contains(id))
        })
        .map(|request| request.id)
        .collect();
    let run_ids: Vec<u64> = {
        let mut ids = Vec::new();
        for request in store
            .research_requests()
            .iter()
            .filter(|request| request_ids.contains(&request.id))
        {
            if let Some(run) = request.planning_run_id {
                ids.push(run);
            }
        }
        for task in task_ids.iter().copied() {
            if let Some(result) = store.research_result(task) {
                ids.push(result.id);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let tasks_before: Vec<Task> = task_ids
        .iter()
        .filter_map(|id| store.task(*id).cloned())
        .collect();
    let requests_before: Vec<ResearchRequest> = request_ids
        .iter()
        .filter_map(|id| {
            store
                .research_requests()
                .iter()
                .find(|request| request.id == *id)
                .cloned()
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
    for before in tasks_before {
        if let Some(after) = store.task(before.id).cloned() {
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
                    after: after.approved_plan,
                });
            }
        }
    }
    for before in requests_before {
        if let Some(after) = store
            .research_requests()
            .iter()
            .find(|request| request.id == before.id)
            .cloned()
        {
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
pub(crate) fn apply_undo(store: &mut Store, undo: &DomainUndo) -> Result<(), ScopedEditError> {
    validate_projects(store, &undo.projects)?;
    validate_topics(store, &undo.topics)?;
    validate_tasks(store, &undo.tasks)?;
    validate_requests(store, &undo.requests)?;
    for delta in &undo.runs {
        let current = store
            .runs()
            .iter()
            .find(|run| run.id == delta.id)
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

    // Children first so a parent created and a child restored by the same
    // inverse stay coherent.
    for delta in &undo.tasks {
        apply_task(store, delta);
    }
    for delta in &undo.requests {
        apply_request(store, delta);
    }
    for delta in &undo.topics {
        apply_topic(store, delta);
    }
    for delta in &undo.projects {
        apply_project(store, delta);
    }
    for delta in &undo.runs {
        if let Some(mut run) = store.run_mut(delta.id) {
            run.input_signature = delta.before_input_signature.clone();
            run.project_plan = delta.before_project_plan;
        }
    }
    undo.settings.apply(store);
    Ok(())
}

fn validate_projects(store: &Store, deltas: &[ProjectDelta]) -> Result<(), ScopedEditError> {
    let message = "The project changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let current = store.project(delta.key());
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

fn validate_topics(store: &Store, deltas: &[TopicDelta]) -> Result<(), ScopedEditError> {
    let message = "The topic changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let current = store.topic(delta.key());
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

fn validate_tasks(store: &Store, deltas: &[TaskDelta]) -> Result<(), ScopedEditError> {
    let message = "The task changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let Some(task) = store.task(delta.key()) else {
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

fn validate_requests(store: &Store, deltas: &[RequestDelta]) -> Result<(), ScopedEditError> {
    let message = "The research request changed after this action. Undo would overwrite newer work.";
    for delta in deltas {
        let Some(request) = store
            .research_requests()
            .iter()
            .find(|request| request.id == delta.key())
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

fn apply_project(store: &mut Store, delta: &ProjectDelta) {
    match delta {
        ProjectDelta::Name { id, before, .. } => {
            if let Some(project) = store.project_mut(*id) {
                project.name = before.clone();
            }
        }
        ProjectDelta::Directory { id, before, .. } => {
            if let Some(project) = store.project_mut(*id) {
                project.directory = before.clone();
            }
        }
        ProjectDelta::Created(value) => {
            store.remove_project(value.id);
        }
        ProjectDelta::Deleted(value) => {
            if store.project(value.id).is_none() {
                store.push_project(value.clone());
            }
        }
    }
}

fn apply_topic(store: &mut Store, delta: &TopicDelta) {
    match delta {
        TopicDelta::Name { id, before, .. } => {
            if let Some(topic) = store.topic_mut(*id) {
                topic.name = before.clone();
            }
        }
        TopicDelta::Project { id, before, .. } => {
            if let Some(topic) = store.topic_mut(*id) {
                topic.project_id = *before;
            }
        }
        TopicDelta::Archived { id, before, .. } => {
            if let Some(topic) = store.topic_mut(*id) {
                topic.archived = *before;
            }
        }
        TopicDelta::Created(value) => {
            store.remove_topic(value.id);
        }
        TopicDelta::Deleted(value) => {
            if store.topic(value.id).is_none() {
                store.push_topic(value.clone());
            }
        }
    }
}

fn apply_task(store: &mut Store, delta: &TaskDelta) {
    let Some(task) = store.task_mut(delta.key()) else {
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

fn apply_request(store: &mut Store, delta: &RequestDelta) {
    let Some(request) = store.request_mut(delta.key()) else {
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
