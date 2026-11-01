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
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Backlog => "◌",
            Self::Planned => "○",
            Self::InProgress => "◐",
            Self::Review => "◇",
            Self::Done => "✓",
            Self::Cancelled => "⊗",
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub research_brief: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Topic {
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRun {
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    #[serde(default)]
    pub topics: Vec<Topic>,
    #[serde(default)]
    pub research_requests: Vec<crate::request_planning::ResearchRequest>,
    pub policies: Vec<StagePolicy>,
    pub runs: Vec<AgentRun>,
    pub next_id: u64,
    #[serde(default)]
    pub opencode_path: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub research_config: crate::research::ResearchConfig,
    #[serde(default)]
    pub appearance: crate::appearance::Appearance,
}

impl Store {
    pub fn all_stages_use_global_model(&self) -> bool {
        !self.default_model.is_empty()
            && self.research_config.models.iter().all(String::is_empty)
            && self
                .policies
                .iter()
                .filter(|p| p.agent == "OpenCode")
                .all(|p| p.model.is_empty())
    }
    /// A non-empty stage override takes precedence over the workspace default.
    pub fn effective_model<'a>(&'a self, stage_override: &'a str) -> &'a str {
        if stage_override.is_empty() {
            &self.default_model
        } else {
            stage_override
        }
    }
    pub fn research_model(&self, kind: crate::research::ResearchKind) -> &str {
        self.effective_model(&self.research_config.models[kind.index()])
    }
    /// A new workspace contains configuration, never demonstration tasks or runs.
    pub fn empty() -> Self {
        use Stage::*;
        let policies = Stage::ALL.into_iter().map(|stage| StagePolicy {
            stage,
            agent: if matches!(stage,Done|Cancelled) { "Manual" } else { "OpenCode" }.into(),
            model: String::new(),
            prompt: match stage {
                Backlog => "Clarify the task and identify missing requirements. Do not modify files.",
                Planned => "Read the task and repository instructions. Propose an implementation plan and verification steps.",
                InProgress => "Implement the agreed task. Keep changes focused and run the relevant checks.",
                Review => "Review the change for correctness, regressions and missing tests. Report findings before modifying code.",
                Done => "Summarize the completed changes and verification evidence.",
                Cancelled => "Do not start work on cancelled tasks.",
            }.into(),
            auto_start: false,
        }).collect();
        Self {
            tasks: vec![],
            projects: vec![Project {
                id: 1,
                name: "Workspace".into(),
                key: "TASK".into(),
                directory: String::new(),
                research_brief: String::new(),
            }],
            topics: vec![],
            research_requests: vec![],
            policies,
            runs: vec![],
            next_id: 1,
            opencode_path: String::new(),
            default_model: String::new(),
            research_config: Default::default(),
            appearance: Default::default(),
        }
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
        self.topics.retain(|topic| {
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
        for task in &mut self.tasks {
            if task.approved_plan.as_deref() == Some(LEGACY_APPROVAL_MARKER) {
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
        self.topics.push(Topic {
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
        let topic = self
            .topics
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("Topic not found.")?;
        topic.name = name;
        Ok(())
    }
    pub fn set_topic_archived(&mut self, id: u64, archived: bool) -> Result<(), String> {
        let topic = self
            .topics
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("Topic not found.")?;
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
    /// Count tasks, not attempts: current work and saved completed results.
    pub fn progress_result_task_count(&self) -> usize {
        let mut latest = std::collections::HashSet::new();
        let mut included = std::collections::HashSet::new();
        for run in self.runs.iter().rev().filter(|run| {
            run.project_plan.is_none()
                && run.research_kind != Some(crate::research::ResearchKind::Goal)
                && self.task(run.task_id).is_some()
                && !self.run_is_archived(run)
        }) {
            let is_latest = latest.insert(run.task_id);
            let in_progress = is_latest && matches!(run.status.as_str(),
                "Queued" | "Running" | "Recovering" | "Cancelling");
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
        self.tasks.iter_mut().find(|t| t.id == id)
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
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            created_at: crate::opencode::timestamp(),
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
        Ok(id)
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
        for task in self.tasks.iter_mut().filter(|t| ids.contains(&t.id)) {
            task.project_id = project;
            task.topic_id = if task.id == id { Some(topic) } else { None };
            if task.id == id {
                task.parent_id = None;
            }
            task.activity.push(activity_event("Project changed"));
        }
        Ok(())
    }
    pub fn set_stage(&mut self, id: u64, stage: Stage) -> Result<(), String> {
        if stage == Stage::Done
            && self.task(id).is_some_and(|t| {
                t.research.as_ref().is_some_and(|r| {
                    r.kind == crate::research::ResearchKind::Goal && r.requires_approval
                })
            })
        {
            let children = self.research_children(id);
            if children.is_empty()
                || self.validate_goal_dependencies(id).is_err()
                || children.iter().any(|child| {
                    self.task(*child).is_none_or(|t| t.stage != Stage::Done)
                        || self.research_result(*child).is_none()
                })
            {
                return Err(
                    "Complete this goal's research steps and final report before marking it Done."
                        .into(),
                );
            }
        }
        let approval = if stage == Stage::Planned
            && self.task(id).is_some_and(|t| {
                t.research
                    .as_ref()
                    .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal)
            }) {
            self.validate_goal_dependencies(id)?;
            self.goal_plan_signature(id)
        } else {
            None
        };
        let task = self.task_mut(id).ok_or("Task not found.")?;
        if task.archived {
            return Err("Restore the archived task before changing its stage.".into());
        }
        if task.research.as_ref().is_some_and(|r| r.requires_approval)
            && matches!(task.stage, Stage::Backlog | Stage::Cancelled)
            && !matches!(stage, Stage::Backlog | Stage::Planned | Stage::Cancelled)
        {
            return Err("Approve this research topic by moving it to Todo first.".into());
        }
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
    pub fn prepare_run(&self, id: u64) -> Result<AgentRun, String> {
        if self.research_pending_approval(id) {
            return Err(
                "Approve the complete research goal in Todo before running its steps.".into(),
            );
        }
        if self.task(id).is_some_and(|t| t.research.is_some()) {
            return crate::research::prepare_run(self, id);
        }
        let task = self.task(id).ok_or("Task not found.")?;
        if self.task_is_archived(id) || task.stage == Stage::Cancelled {
            return Err("Restore or reopen this task before running.".into());
        }
        let policy = self
            .policies
            .iter()
            .find(|p| p.stage == task.stage)
            .ok_or("Configure this stage first.")?;
        if policy.agent != "OpenCode" {
            return Err(format!(
                "This stage uses {}. Select OpenCode in Workflow stages to execute it.",
                policy.agent
            ));
        }
        let model = self.effective_model(&policy.model);
        if !crate::opencode::valid_model(model) {
            return Err(
                "Choose a global model or a stage override in provider/model format.".into(),
            );
        }
        let project = self
            .projects
            .iter()
            .find(|p| p.id == task.project_id)
            .ok_or("Project not found.")?;
        if project.directory.is_empty() {
            return Err("Set this project's working folder before running.".into());
        }
        let prompt = format!(
            "Task: {} — {}\nStage: {}\nProject directory: {}\n\nStage instructions:\n{}\n\nTask description and acceptance criteria:\n{}\n\nWork in the selected project directory. Follow its repository instructions. Report your changes and verification when finished.",
            self.identifier(task),
            task.title,
            task.stage.label(),
            project.directory,
            policy.prompt,
            task.description
        );
        Ok(AgentRun {
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: self.runs.iter().map(|r| r.id).max().unwrap_or(0) + 1,
            task_id: id,
            task_title: task.title.clone(),
            agent: policy.agent.clone(),
            model: model.to_string(),
            stage: task.stage,
            status: "Running".into(),
            output: String::new(),
            attachments: vec![],
            research_kind: None,
            sources: vec![],
            input_signature: String::new(),
            directory: project.directory.clone(),
            prompt,
            queued_at: String::new(),
            workspace_source: None,
            started_at: crate::opencode::timestamp(),
            ended_at: None,
            session_id: None,
            exit_code: None,
            logs: vec![format!(
                "[run] {} · {} · {}",
                self.identifier(task),
                task.stage.label(),
                model
            )],
        })
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
        for (i, run) in self.runs.iter_mut().enumerate() {
            if run.id == 0 {
                run.id = (i + 1) as u64;
            }
            if matches!(
                run.status.as_str(),
                "Queued" | "Running" | "Cancelling" | "Recovering"
            ) {
                interrupted.push(run.id);
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
    fn progress_count_tracks_tasks_instead_of_attempts() {
        let mut store = Store::mock();
        store.runs.clear();
        store.projects[0].directory = "/tmp/test-project".into();
        for policy in &mut store.policies {
            policy.agent = "OpenCode".into();
            policy.model = "test/model".into();
        }
        store.set_stage(1, Stage::InProgress).unwrap();
        let template = store.prepare_run(1).unwrap();
        for index in 0..8 {
            let id = store.create_task(&format!("Step {index}"), 1, Stage::InProgress, None).unwrap();
            let mut run = template.clone();
            run.task_id = id;
            run.status = "Failed".into();
            store.runs.push(run.clone());
            run.status = if index == 7 { "Running" } else { "Succeeded" }.into();
            if index != 7 { run.output = "Saved result".into(); }
            store.runs.push(run);
        }
        assert_eq!(store.runs.len(), 16);
        assert_eq!(store.progress_result_task_count(), 8);
        let mut planning = template.clone();
        planning.research_kind = Some(crate::research::ResearchKind::Goal);
        store.runs.push(planning);
        let mut planning = template.clone();
        planning.project_plan = Some(1);
        store.runs.push(planning);
        assert_eq!(store.progress_result_task_count(), 8);

        // Retrying a task with a saved result must not count it twice.
        let mut retry = store.runs[1].clone();
        retry.status = "Running".into();
        retry.output.clear();
        store.runs.push(retry);
        assert_eq!(store.progress_result_task_count(), 8);
        // A later failure supersedes an old running record, but keeps saved results.
        let mut failed = store.runs[15].clone();
        failed.status = "Failed".into();
        store.runs.push(failed);
        assert_eq!(store.progress_result_task_count(), 7);
        let id = store.runs[1].task_id;
        store.task_mut(id).unwrap().archived = true;
        assert_eq!(store.progress_result_task_count(), 6);

        for status in ["Failed", "Blocked", "Interrupted", "Cancelled", "Succeeded"] {
            let mut run = template.clone();
            run.status = status.into();
            store.runs.push(run);
            assert_eq!(store.progress_result_task_count(), 6);
        }
        for status in ["Queued", "Running", "Recovering", "Cancelling"] {
            store.runs.last_mut().unwrap().status = status.into();
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
        assert_eq!(store.tasks[0].title, tasks[0].title);
        assert_eq!(store.tasks[0].topic_id, tasks[0].topic_id);
        let normalized = store.clone();
        store.normalize_topics();
        assert_eq!(store, normalized);
    }

    #[test]
    fn topic_membership_is_required_and_inherited_by_research_steps() {
        let mut store = Store::empty();
        let project = store.projects[0].id;
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
        let project = store.projects[0].id;
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
        let project = store.projects[0].id;
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
        let project = store.projects[0].id;
        let mut other = store.projects[0].clone();
        other.id = project + 1;
        store.projects.push(other);
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
        let project = store.projects[0].id;
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
        store.topics.clear();
        for task in &mut store.tasks {
            task.topic_id = None;
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
        let project = store.projects[0].id;
        let key = store.projects[0].key.clone();
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
        let project = store.projects[0].id;
        let key = store.projects[0].key.clone();
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
        let id = store
            .create_task("New task", store.projects[0].id, Stage::Planned, None)
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
        let store = Store::empty();
        assert!(store.tasks.is_empty());
        assert!(store.runs.is_empty());
        assert!(store.default_model.is_empty());
        assert_eq!(store.next_id, 1);
        assert_eq!(store.projects[0].name, "Workspace");
        assert!(store.projects[0].directory.is_empty());
        assert!(
            store
                .policies
                .iter()
                .all(|p| p.model.is_empty() && !p.auto_start)
        );
        assert_eq!(store.appearance, crate::appearance::Appearance::Light);
    }
    #[test]
    fn execution_snapshots_stage_model_prompt_and_project_folder() {
        let mut store = Store::mock();
        store.projects[0].directory = "/tmp/test-project".into();
        for policy in &mut store.policies {
            policy.agent = "OpenCode".into();
            policy.model = format!("test/{}", policy.stage.key());
        }
        store.set_stage(1, Stage::InProgress).unwrap();
        let run = store.prepare_run(1).unwrap();
        assert_eq!(run.model, "test/in-progress");
        assert!(run.prompt.contains("/tmp/test-project"));
        store.set_stage(1, Stage::Review).unwrap();
        let review = store.prepare_run(1).unwrap();
        assert_eq!(review.model, "test/review");
        assert_eq!(run.stage, Stage::InProgress);
    }
    #[test]
    fn global_model_fallback_and_stage_overrides_snapshot_requests() {
        let mut store = Store::mock();
        store.projects[0].directory = "/tmp/test-project".into();
        let index = store
            .policies
            .iter()
            .position(|p| p.stage == Stage::InProgress)
            .unwrap();
        store.policies[index].agent = "OpenCode".into();
        store.policies[index].model.clear();
        assert!(store.prepare_run(1).is_err());
        store.default_model = "missing-provider".into();
        assert!(store.prepare_run(1).is_err());
        store.default_model = "tencent-token-plan/deepseek/deepseek-flash".into();
        let run = store.prepare_run(1).unwrap();
        assert_eq!(run.model, store.default_model);
        assert!(run.logs[0].contains(&run.model));
        store.runs.push(run.clone());
        store.default_model = "test/new-default".into();
        assert_eq!(store.prepare_run(1).unwrap().model, "test/new-default");
        store.policies[index].model = "test/override".into();
        store.default_model.clear();
        assert_eq!(store.prepare_run(1).unwrap().model, "test/override");
        store.default_model = "test/default".into();
        store.policies[index].model = "invalid".into();
        assert!(store.prepare_run(1).is_err());
        store.policies[index].model.clear();
        store.policies[index].agent = "Manual".into();
        assert!(store.prepare_run(1).is_err());
        assert_eq!(store.runs, vec![run]);
    }
    #[test]
    fn research_execution_updates_stage_and_recovers_without_overriding_completion() {
        let mut store = Store::mock();
        store.simulate_run(1).unwrap();
        let id = store.runs[0].id;
        let original = store.task(1).unwrap().stage;
        store.runs[0].research_kind = Some(crate::research::ResearchKind::Collect);
        store.runs[0].status = "Running".into();
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
        store.runs[0].status = "Running".into();
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
        store.policies.iter_mut().for_each(|p| p.auto_start = true);
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
        store
            .policies
            .iter_mut()
            .find(|p| p.stage == stage)
            .unwrap()
            .model = "another-model".into();
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
