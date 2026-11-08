//! T13 S3e/f: bounded, command-targeted change sets for project/topic context,
//! directory, archive/restore and settings.
//!
//! Each command records only the entities and fields it is about to write. There
//! is no whole-Store snapshot or whole-domain diff. The inverse validates every
//! recorded preimage before touching anything, then applies a fixed write set.

use crate::appearance::Appearance;
use crate::domain::{activity_event, AgentRun, Project, ResearchRequest, Store, Task, Topic};
use crate::model_profiles::ModelProfile;
use crate::report_publish::PublishingConfig;
use crate::research::ResearchConfig;
use crate::scoped_edit::ScopedEditError;

#[derive(Clone, Debug, PartialEq)]
enum EntityDelta<T> {
    Changed { before: T, after: T },
    Created(T),
    Deleted(T),
}

impl<T> EntityDelta<T> {
    fn key<F: Fn(&T) -> u64>(&self, id: F) -> u64 {
        match self {
            Self::Changed { after, .. } => id(after),
            Self::Created(value) => id(value),
            Self::Deleted(value) => id(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RunDelta {
    before: AgentRun,
    after: AgentRun,
}

/// Optional field deltas. Each field is validated and restored independently.
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
    projects: Vec<EntityDelta<Project>>,
    topics: Vec<EntityDelta<Topic>>,
    tasks: Vec<EntityDelta<Task>>,
    requests: Vec<EntityDelta<ResearchRequest>>,
    runs: Vec<RunDelta>,
    settings: SettingsDelta,
}

impl DomainUndo {
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
    /// Validate and execute against the live Store. All validation happens
    /// before the first write; the returned undo is a fixed write set.
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
                if before != after {
                    undo.projects.push(EntityDelta::Changed { before, after });
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
                if before != after {
                    undo.projects.push(EntityDelta::Changed { before, after });
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
                undo.projects.push(EntityDelta::Created(project));
                created_project = Some(id);
            }
            Self::TopicCreate { project, name } => {
                let id = store.create_topic(project, name)?;
                let after = store.topic(id).cloned().ok_or("Topic not found.")?;
                undo.topics.push(EntityDelta::Created(after));
                created_topic = Some(id);
            }
            Self::TopicRename { topic, name } => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.rename_topic(topic, name)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before != after {
                    undo.topics.push(EntityDelta::Changed { before, after });
                }
            }
            Self::TopicMove { topic, project } => {
                self::move_topic(store, topic, project, &mut undo)?;
            }
            Self::TopicArchive { topic, archived } => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.set_topic_archived(topic, archived)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before != after {
                    undo.topics.push(EntityDelta::Changed { before, after });
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
                record_task_change(&mut undo, before, after);
            }
            Self::TaskParent { task, parent } => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                let max_topic = store.topics().iter().map(|topic| topic.id).max().unwrap_or(0);
                store.set_parent(task, parent)?;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                record_task_change(&mut undo, before, after);
                for topic in store.topics().iter().filter(|topic| topic.id > max_topic) {
                    undo.topics.push(EntityDelta::Created(topic.clone()));
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
                record_task_change(&mut undo, before, after);
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

/// QA-only compatibility capture for test helpers that still use a closure.
/// Production commands build their undo from the exact write set instead.
#[cfg(feature = "qa")]
pub(crate) fn capture_undo(before: &Store, after: &Store) -> Option<DomainUndo> {
    fn diff<T: Clone + PartialEq>(
        before: &[T],
        after: &[T],
        id: impl Fn(&T) -> u64,
    ) -> Vec<EntityDelta<T>> {
        let mut changes = Vec::new();
        for old in before {
            match after.iter().find(|value| id(value) == id(old)) {
                Some(new) if new != old => changes.push(EntityDelta::Changed {
                    before: old.clone(),
                    after: new.clone(),
                }),
                None => changes.push(EntityDelta::Deleted(old.clone())),
                Some(_) => {}
            }
        }
        for new in after {
            if !before.iter().any(|value| id(value) == id(new)) {
                changes.push(EntityDelta::Created(new.clone()));
            }
        }
        changes
    }
    let undo = DomainUndo {
        projects: diff(before.projects(), after.projects(), |value| value.id),
        topics: diff(before.topics(), after.topics(), |value| value.id),
        tasks: diff(before.tasks(), after.tasks(), |value| value.id),
        requests: diff(before.research_requests(), after.research_requests(), |value| {
            value.id
        }),
        runs: diff_runs(before.runs(), after.runs()),
        settings: {
            let before = SettingsState::capture(before);
            let after = SettingsState::capture(after);
            (before != after).then_some((before, after))
        },
    };
    (!undo.is_empty()).then_some(undo)
}

fn record_task_change(undo: &mut DomainUndo, before: Task, after: Task) {
    if before != after {
        undo.tasks.push(EntityDelta::Changed { before, after });
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
                .filter(|after| *after != &before)
                .map(|after| RunDelta {
                    before,
                    after: after.clone(),
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
    if topic_before != topic_after {
        undo.topics.push(EntityDelta::Changed {
            before: topic_before,
            after: topic_after,
        });
    }
    for before in tasks_before {
        if let Some(after) = store.task(before.id).cloned()
            && before != after
        {
            undo.tasks.push(EntityDelta::Changed { before, after });
        }
    }
    for before in requests_before {
        if let Some(after) = store
            .research_requests()
            .iter()
            .find(|request| request.id == before.id)
            .cloned()
            && before != after
        {
            undo.requests.push(EntityDelta::Changed { before, after });
        }
    }
    undo.runs = changed_runs(store, runs_before);
    Ok(())
}

fn restore_archived(store: &mut Store, id: u64, undo: &mut DomainUndo) -> Result<(), String> {
    // Preflight the whole ownership chain with a planning-only view of which
    // themes/tasks would be unarchived. A cycle or unresolved owner is rejected
    // before the first write.
    let plan = plan_archive_restore(store, id)?;
    for owner in plan {
        match owner {
            crate::archive_presentation::ArchivedOwner::Topic(topic) => {
                let before = store.topic(topic).cloned().ok_or("Topic not found.")?;
                store.set_topic_archived(topic, false)?;
                let after = store.topic(topic).cloned().ok_or("Topic not found.")?;
                if before != after {
                    undo.topics.push(EntityDelta::Changed { before, after });
                }
            }
            crate::archive_presentation::ArchivedOwner::Task(task) => {
                let before = store.task(task).cloned().ok_or("Task not found.")?;
                store.restore(task)?;
                let after = store.task(task).cloned().ok_or("Task not found.")?;
                record_task_change(undo, before, after);
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

fn conflict(message: &str) -> ScopedEditError {
    ScopedEditError::Conflict(message.into())
}

/// Validate every recorded preimage before writing. A conflict therefore leaves
/// the entire pre-attempt state, including dirty tracking, untouched.
pub(crate) fn apply_undo(store: &mut Store, undo: &DomainUndo) -> Result<(), ScopedEditError> {
    validate_entities(&undo.projects, store.projects().iter(), |v| v.id,
        "The project changed after this action. Undo would overwrite newer work.")?;
    validate_entities(&undo.topics, store.topics().iter(), |v| v.id,
        "The topic changed after this action. Undo would overwrite newer work.")?;
    validate_entities(&undo.tasks, store.tasks().iter(), |v| v.id,
        "The task changed after this action. Undo would overwrite newer work.")?;
    validate_entities(&undo.requests, store.research_requests().iter(), |v| v.id,
        "The research request changed after this action. Undo would overwrite newer work.")?;
    for delta in &undo.runs {
        let current = store
            .runs()
            .iter()
            .find(|run| run.id == delta.after.id)
            .ok_or_else(|| conflict("A run from this action is no longer available."))?;
        if current != &delta.after {
            return Err(conflict(
                "A run changed after this action. Undo would overwrite newer runtime history.",
            ));
        }
    }
    validate_created_parents(store, undo)?;
    undo.settings.validate(store)?;

    // Restore children first so creating parents from the same inverse can be
    // removed without treating those children as unrelated later work.
    for delta in &undo.tasks {
        match delta {
            EntityDelta::Changed { before, .. } => {
                if let Some(task) = store.task_mut(before.id) {
                    *task = before.clone();
                }
            }
            EntityDelta::Created(value) => {
                store.remove_task(value.id);
            }
            EntityDelta::Deleted(value) => store.push_task(value.clone()),
        }
    }
    for delta in &undo.requests {
        match delta {
            EntityDelta::Changed { before, .. } => {
                if let Some(request) = store.request_mut(before.id) {
                    *request = before.clone();
                }
            }
            EntityDelta::Created(value) => {
                store.remove_request(value.id);
            }
            EntityDelta::Deleted(value) => store.push_request(value.clone()),
        }
    }
    for delta in &undo.topics {
        match delta {
            EntityDelta::Changed { before, .. } => {
                if let Some(topic) = store.topic_mut(before.id) {
                    *topic = before.clone();
                }
            }
            EntityDelta::Created(value) => {
                store.remove_topic(value.id);
            }
            EntityDelta::Deleted(value) => store.push_topic(value.clone()),
        }
    }
    for delta in &undo.projects {
        match delta {
            EntityDelta::Changed { before, .. } => {
                if let Some(project) = store.project_mut(before.id) {
                    *project = before.clone();
                }
            }
            EntityDelta::Created(value) => {
                store.remove_project(value.id);
            }
            EntityDelta::Deleted(value) => store.push_project(value.clone()),
        }
    }
    for delta in &undo.runs {
        if let Some(run) = store.run_mut(delta.after.id) {
            *run = delta.before.clone();
        }
    }
    undo.settings.apply(store);
    Ok(())
}

fn validate_entities<'a, T, I, F>(
    deltas: &[EntityDelta<T>],
    current: I,
    id: F,
    message: &str,
) -> Result<(), ScopedEditError>
where
    T: Clone + PartialEq + 'a,
    I: Iterator<Item = &'a T> + Clone,
    F: Fn(&T) -> u64 + Copy,
{
    for delta in deltas {
        let key = delta.key(id);
        let existing = current.clone().find(|value| id(value) == key);
        match delta {
            EntityDelta::Changed { after, .. } | EntityDelta::Created(after) => {
                let existing = existing.ok_or_else(|| conflict(message))?;
                if existing != after {
                    return Err(conflict(message));
                }
            }
            EntityDelta::Deleted(before) => {
                if let Some(existing) = existing
                    && existing != before
                {
                    return Err(conflict(message));
                }
            }
        }
    }
    Ok(())
}

/// A created parent may only be removed when nothing outside this inverse still
/// references it. Children this same inverse restores away are evaluated at
/// their recorded `before` value.
fn validate_created_parents(store: &Store, undo: &DomainUndo) -> Result<(), ScopedEditError> {
    for delta in &undo.topics {
        let EntityDelta::Created(created) = delta else {
            continue;
        };
        let referenced = store
            .tasks()
            .iter()
            .any(|task| task_topic_after_undo(task, undo) == Some(created.id))
            || store
                .research_requests()
                .iter()
                .any(|request| request_topic_after_undo(request, undo) == Some(created.id));
        if referenced {
            return Err(conflict(
                "The newly created topic now contains work. Undo will not remove it.",
            ));
        }
    }
    for delta in &undo.projects {
        let EntityDelta::Created(created) = delta else {
            continue;
        };
        let referenced = store
            .topics()
            .iter()
            .any(|topic| topic_project_after_undo(topic, undo) == created.id)
            || store
                .tasks()
                .iter()
                .any(|task| task_project_after_undo(task, undo) == created.id)
            || store
                .research_requests()
                .iter()
                .any(|request| request_project_after_undo(request, undo) == created.id);
        if referenced {
            return Err(conflict(
                "The newly created project now contains work. Undo will not remove it.",
            ));
        }
    }
    Ok(())
}

fn task_topic_after_undo(task: &Task, undo: &DomainUndo) -> Option<u64> {
    undo.tasks
        .iter()
        .find(|delta| delta.key(|v| v.id) == task.id)
        .map(|delta| match delta {
            EntityDelta::Changed { before, .. } | EntityDelta::Deleted(before) => before.topic_id,
            EntityDelta::Created(_) => None,
        })
        .unwrap_or(task.topic_id)
}

fn request_topic_after_undo(request: &ResearchRequest, undo: &DomainUndo) -> Option<u64> {
    undo.requests
        .iter()
        .find(|delta| delta.key(|v| v.id) == request.id)
        .map(|delta| match delta {
            EntityDelta::Changed { before, .. } | EntityDelta::Deleted(before) => before.topic_id,
            EntityDelta::Created(_) => None,
        })
        .unwrap_or(request.topic_id)
}

fn topic_project_after_undo(topic: &Topic, undo: &DomainUndo) -> u64 {
    undo.topics
        .iter()
        .find(|delta| delta.key(|v| v.id) == topic.id)
        .map(|delta| match delta {
            EntityDelta::Changed { before, .. } | EntityDelta::Deleted(before) => before.project_id,
            EntityDelta::Created(_) => u64::MAX,
        })
        .unwrap_or(topic.project_id)
}

fn task_project_after_undo(task: &Task, undo: &DomainUndo) -> u64 {
    undo.tasks
        .iter()
        .find(|delta| delta.key(|v| v.id) == task.id)
        .map(|delta| match delta {
            EntityDelta::Changed { before, .. } | EntityDelta::Deleted(before) => before.project_id,
            EntityDelta::Created(_) => u64::MAX,
        })
        .unwrap_or(task.project_id)
}

fn request_project_after_undo(request: &ResearchRequest, undo: &DomainUndo) -> u64 {
    undo.requests
        .iter()
        .find(|delta| delta.key(|v| v.id) == request.id)
        .map(|delta| match delta {
            EntityDelta::Changed { before, .. } | EntityDelta::Deleted(before) => before.project_id,
            EntityDelta::Created(_) => u64::MAX,
        })
        .unwrap_or(request.project_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Stage, Store};

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
    fn inverse_conflicts_and_preserves_state_when_touched_entity_changed_again() {
        let mut store = Store::empty();
        let topic = store.create_topic(1, "Original").unwrap();
        let outcome = DomainCommand::TopicRename {
            topic,
            name: "Renamed".into(),
        }
        .execute(&mut store)
        .unwrap();
        // A later unrelated edit to the same entity must reject the inverse.
        store.rename_topic(topic, "Later edit").unwrap();
        let before_attempt = store.clone();
        assert!(matches!(
            apply_undo(&mut store, &outcome.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert_eq!(store.topic(topic).unwrap().name, "Later edit");
    }

    #[test]
    fn same_inverse_created_topic_is_removed_with_its_restored_child() {
        let mut store = Store::empty();
        let task = store.create_task("Task", 1, Stage::Backlog, None).unwrap();
        // set_parent(None) creates a topic and assigns it to the task.
        let outcome = DomainCommand::TaskParent { task, parent: None }
            .execute(&mut store)
            .unwrap();
        let created = outcome.created_topic.expect("a topic was created");
        assert_eq!(store.task(task).unwrap().topic_id, Some(created));
        apply_undo(&mut store, &outcome.undo).unwrap();
        assert!(store.topic(created).is_none());
        assert_eq!(store.task(task).unwrap().topic_id, None);
        assert_eq!(store.task(task).unwrap().parent_id, None);
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
        // A child added after the command is unrelated work. Associate it with
        // the topic through the topic API, not as a parent task.
        let child = store
            .create_task("Child", 1, Stage::Backlog, None)
            .unwrap();
        store.set_root_topic(child, topic).unwrap();
        let before_attempt = store.clone();
        assert!(matches!(
            apply_undo(&mut store, &outcome.undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert!(store.topic(topic).is_some());
    }

    #[test]
    fn settings_conflict_validates_all_fields_before_any_entity_rollback() {
        let mut store = Store::empty();
        store.set_default_model("provider/after");
        store.set_appearance(Appearance::Dark);
        let topic = store.create_topic(1, "After").unwrap();
        let mut undo = DomainUndo {
            projects: Vec::new(),
            topics: vec![EntityDelta::Changed {
                before: {
                    let mut before = store.topic(topic).unwrap().clone();
                    before.name = "Before".into();
                    before
                },
                after: store.topic(topic).unwrap().clone(),
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
        // rejected before the topic entity or the first setting is touched.
        store.set_appearance(Appearance::Light);
        let before_attempt = store.clone();
        assert!(matches!(
            apply_undo(&mut store, &undo),
            Err(ScopedEditError::Conflict(_))
        ));
        assert_eq!(store, before_attempt);
        assert_eq!(store.topic(topic).unwrap().name, "After");
        assert_eq!(store.default_model(), "provider/after");
        assert_eq!(store.appearance(), Appearance::Light);
        undo.settings.set_appearance(None);
        // With the conflicting field removed the same entity and first setting
        // restore together.
        apply_undo(&mut store, &undo).unwrap();
        assert_eq!(store.topic(topic).unwrap().name, "Before");
        assert_eq!(store.default_model(), "provider/before");
        assert_eq!(store.appearance(), Appearance::Light);
    }
}
