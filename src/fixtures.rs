//! Deterministic sample data for tests and opt-in QA only; excluded from production builds.
use super::*;

impl Store {
    pub fn mock() -> Self {
        use Priority::{High, Low, Medium, Urgent};
        use Stage::*;
        let projects = vec![
            Project {
                id: 1,
                name: "Task Manager".into(),
                key: "TM".into(),
                directory: String::new(),
                research_brief: String::new(),
            },
            Project {
                id: 2,
                name: "Agent integrations".into(),
                key: "AG".into(),
                directory: String::new(),
                research_brief: String::new(),
            },
        ];
        let seed = [
            (
                "Polish task details and navigation",
                1,
                InProgress,
                High,
                "Improvement",
                None,
            ),
            (
                "Keep task context when switching tabs",
                1,
                Done,
                Medium,
                "Improvement",
                Some(1),
            ),
            (
                "Align properties with the task title",
                1,
                Done,
                Low,
                "Design",
                Some(1),
            ),
            (
                "Add keyboard navigation between tasks",
                1,
                Review,
                High,
                "Feature",
                Some(1),
            ),
            (
                "保留中文输入法的组合输入状态",
                1,
                Planned,
                High,
                "Bug",
                Some(1),
            ),
            (
                "Drag tasks between workflow stages",
                1,
                InProgress,
                High,
                "Feature",
                None,
            ),
            (
                "Restore card position when a drag is cancelled",
                1,
                Planned,
                Medium,
                "Bug",
                Some(6),
            ),
            (
                "Search by status, priority and label",
                1,
                Review,
                Medium,
                "Feature",
                None,
            ),
            (
                "Support quoted phrases in the filter bar",
                1,
                Backlog,
                Low,
                "Improvement",
                Some(8),
            ),
            (
                "Create a task from a project or cycle",
                1,
                Planned,
                Medium,
                "Feature",
                None,
            ),
            (
                "Show subtasks in the project list",
                1,
                Done,
                Medium,
                "Feature",
                None,
            ),
            ("把截止日期展示为本地日期", 1, Planned, High, "Bug", None),
            (
                "Make empty project views actionable",
                1,
                Backlog,
                Low,
                "Design",
                None,
            ),
            (
                "Persist the workspace between launches",
                1,
                InProgress,
                Urgent,
                "Feature",
                None,
            ),
            (
                "Remove the duplicate task status banner",
                1,
                Cancelled,
                Priority::None,
                "Design",
                None,
            ),
            (
                "Define the local agent adapter contract",
                2,
                InProgress,
                High,
                "Architecture",
                None,
            ),
            (
                "Add a Codex adapter configuration",
                2,
                Planned,
                High,
                "Feature",
                Some(16),
            ),
            (
                "Add an OpenCode adapter configuration",
                2,
                Planned,
                High,
                "Feature",
                Some(16),
            ),
            (
                "Route each stage to its selected model",
                2,
                Review,
                Urgent,
                "Feature",
                None,
            ),
            (
                "Require approval before starting an agent",
                2,
                Planned,
                High,
                "Safety",
                None,
            ),
            (
                "在任务详情中展示执行日志",
                2,
                Backlog,
                Medium,
                "Feature",
                None,
            ),
            (
                "Keep simulation separate from real execution",
                2,
                Done,
                High,
                "Architecture",
                None,
            ),
        ];
        let tasks = seed.into_iter().enumerate().map(|(i, (title, project_id, stage, priority, label, parent_id))| {
            Task {
                id: (i + 1) as u64, created_at: "2026-10-27 09:30:00 +08:00".into(), title: title.into(),
                description: match i {
                    0 => "Make the task detail view feel at home in the workspace. Keep the parent task visible, retain navigation context, and put editable properties within reach.\n\nAcceptance criteria\n• Open a task without losing the list filter.\n• Navigate to a child and return to its parent.\n• Keep the activity history consistent across views.".into(),
                    15 => "Define a small adapter boundary for local coding agents. A run request carries task context, stage, selected agent, model and prompt.\n\nThis prototype uses simulated runs only. No process is spawned, no credentials are read, and no source files are modified.".into(),
                    18 => "Each workflow stage has its own agent and model configuration. Preserve a snapshot of the policy when creating a run so later configuration changes do not alter its history.\n\n先选择阶段，再设置 agent 与模型。真实模型列表由后续适配器提供。".into(),
                    _ => format!("{}\n\nImplement the interaction in the local workspace and verify that list, board and detail views stay consistent.\n\n完成后检查键盘操作、中文输入与空状态。", title),
                },
                project_id, topic_id: None, approved_plan: None, stage, priority, labels: vec![label.into()],
                cycle: if i % 4 != 2 { Some("Cycle 1".into()) } else { None },
                bucket: if i % 3 == 0 { "Now" } else if i % 3 == 1 { "Next" } else { "Later" }.into(),
                due_date: if i % 5 == 0 { "2026-10-27" } else if i % 3 == 0 { "2026-10-30" } else { "" }.into(),
                parent_id, comments: if i == 0 { vec![Comment { author: "You".into(), body: "Keep the properties compact; the task description should remain the visual focus.".into(), created_at: "Sample · 27 Oct".into() }] } else { vec![] },
                activity: vec!["Sample task created · 27 Oct".into(), format!("Sample status: {}", stage.label())], archived: false, attachments: vec![], research: None,
            }
        }).collect();
        let mut store = Self {
            tasks,
            projects,
            topics: vec![],
            research_requests: vec![],
            policies: Store::empty().policies,
            runs: vec![],
            next_id: 23,
            opencode_path: String::new(),
            default_model: String::new(),
            research_config: Default::default(),
            appearance: crate::appearance::Appearance::Dark,
        };
        store.normalize_topics();
        store
    }
    #[cfg(test)]
    pub fn simulate_run(&mut self, id: u64) -> Result<(), String> {
        let task = self.task(id).ok_or("Task not found.")?;
        if task.archived || task.stage == Stage::Cancelled {
            return Err("Choose an active, non-cancelled task to preview a run.".into());
        }
        let policy = self
            .policies
            .iter()
            .find(|p| p.stage == task.stage)
            .ok_or("Configure this workflow stage first.")?;
        let identifier = self.identifier(task);
        let run = AgentRun {
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: self.runs.iter().map(|r|r.id).max().unwrap_or(0)+1,
            task_title:task.title.clone(),directory:String::new(),prompt:policy.prompt.clone(),queued_at: String::new(), workspace_source: None, started_at:String::new(),ended_at:None,session_id:None,exit_code:None,
            task_id: id, agent: policy.agent.clone(), model: policy.model.clone(), stage: task.stage,
            status: "Simulated".into(), output:String::new(), attachments:vec![],research_kind:None,sources:vec![],input_signature:String::new(),
            logs: vec![
                format!("[simulation] Prepared context for {identifier}: {}", task.title),
                format!("[simulation] Stage: {} | Agent: {} | Model: {}", task.stage.label(), policy.agent, policy.model),
                format!("[simulation] Instructions: {}", policy.prompt),
                "[simulation] Adapter is not connected. No agent was launched and no files were changed.".into(),
                "[simulation] Preview complete. Task stage is unchanged.".into(),
            ],
        };
        self.runs.push(run);
        self.task_mut(id)
            .unwrap()
            .activity
            .push("Agent run previewed · Simulation only".into());
        Ok(())
    }
}
