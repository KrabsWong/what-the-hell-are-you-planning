//! Deterministic sample data for tests and opt-in QA only; excluded from production builds.
use super::*;

impl Store {
    pub fn mock() -> Self {
        use Stage::*;
        let projects = vec![
            Project {
                id: 1,
                name: "Task Manager".into(),
                key: "TM".into(),
                directory: String::new(),
                research_brief: String::new(),
                kind: ProjectKind::User,
            },
            Project {
                id: 2,
                name: "Agent integrations".into(),
                key: "AG".into(),
                directory: String::new(),
                research_brief: String::new(),
                kind: ProjectKind::User,
            },
        ];
        let seed = [
            (
                "Polish task details and navigation",
                1,
                InProgress,
                "Improvement",
                None,
            ),
            (
                "Keep task context when switching tabs",
                1,
                Done,
                "Improvement",
                Some(1),
            ),
            (
                "Align properties with the task title",
                1,
                Done,
                "Design",
                Some(1),
            ),
            (
                "Add keyboard navigation between tasks",
                1,
                Review,
                "Feature",
                Some(1),
            ),
            (
                "Preserve Chinese IME composition state",
                1,
                InProgress,
                "Bug",
                Some(1),
            ),
            (
                "Move tasks between workflow stages",
                1,
                InProgress,
                "Feature",
                None,
            ),
            (
                "Restore list position when a move is cancelled",
                1,
                InProgress,
                "Bug",
                Some(6),
            ),
            ("Search by status and label", 1, Review, "Feature", None),
            (
                "Support quoted phrases in the filter bar",
                1,
                Backlog,
                "Improvement",
                Some(8),
            ),
            (
                "Create a task from a project",
                1,
                InProgress,
                "Feature",
                None,
            ),
            (
                "Show subtasks in the project list",
                1,
                Done,
                "Feature",
                None,
            ),
            (
                "Show due dates in the local timezone",
                1,
                InProgress,
                "Bug",
                None,
            ),
            (
                "Make empty project views actionable",
                1,
                Backlog,
                "Design",
                None,
            ),
            (
                "Persist the workspace between launches",
                1,
                InProgress,
                "Feature",
                None,
            ),
            (
                "Remove the duplicate task status banner",
                1,
                Cancelled,
                "Design",
                None,
            ),
            (
                "Define the local agent adapter contract",
                2,
                InProgress,
                "Architecture",
                None,
            ),
            (
                "Add a Codex adapter configuration",
                2,
                Done,
                "Architecture",
                None,
            ),
        ];
        let tasks = seed.into_iter().enumerate().map(|(i, (title, project_id, stage, label, parent_id))| {
            Task {
                id: (i + 1) as u64, created_at: "2026-10-27 09:30:00 +08:00".into(), automation_id: None, title: title.into(),
                description: match i {
                    0 => "Make the task detail view feel at home in the workspace. Keep the parent task visible, retain navigation context, and put editable properties within reach.\n\nAcceptance criteria\n• Open a task without losing the list filter.\n• Navigate to a child and return to its parent.\n• Keep the activity history consistent across views.".into(),
                    15 => "Define a small adapter boundary for local coding agents. A run request carries task context, stage, selected agent, model and prompt.\n\nThis prototype uses simulated runs only. No process is spawned, no credentials are read, and no source files are modified.".into(),
                    18 => "Each workflow stage has its own agent and model configuration. Preserve a snapshot of the policy when creating a run so later configuration changes do not alter its history.\n\nChoose the stage first, then set the agent and model. The real model list is supplied by later adapters.".into(),
                    _ => format!("{}\n\nImplement the interaction in the local workspace and verify that list and detail views stay consistent.\n\nCheck keyboard navigation and empty states after the change.", title),
                },
                project_id, topic_id: None, approved_plan: None, stage, labels: vec![label.into()],
                due_date: if i % 5 == 0 { "2026-10-27" } else if i % 3 == 0 { "2026-10-30" } else { "" }.into(),
                parent_id, comments: if i == 0 { vec![Comment { author: "You".into(), body: "Keep the properties compact; the task description should remain the visual focus.".into(), created_at: "Sample · 27 Oct".into() }] } else { vec![] },
                activity: vec!["Sample task created · 27 Oct".into(), format!("Sample status: {}", stage.label())], archived: false, attachments: vec![], research: None,
            }
        }).collect();
        let policies = Stage::ALL.into_iter().map(|stage| StagePolicy {
            stage,
            agent: if matches!(stage,Done|Cancelled) { "Manual" } else { "OpenCode" }.into(),
            model: String::new(),
            prompt: match stage {
                Backlog => "Clarify the task and identify missing requirements. Do not modify files.",
                InProgress => "Read the task and repository instructions. Propose an implementation plan and verification steps.",
                InProgress => "Implement the agreed task. Keep changes focused and run the relevant checks.",
                Review => "Review the change for correctness, regressions and missing tests. Report findings before modifying code.",
                Done => "Summarize the completed changes and verification evidence.",
                Cancelled => "Do not start work on cancelled tasks.",
            }.into(),
            auto_start: false,
        }).collect();
        let mut loader = Self::begin_load();
        loader
            .tasks(tasks)
            .expect("fixture tasks have unique identities");
        loader
            .projects(projects)
            .expect("fixture projects have unique identities");
        loader
            .policies(policies)
            .expect("fixture policies have unique identities");
        loader.set_metadata(crate::persistence_journal::MetadataDelta::NextId(23));
        loader.set_metadata(crate::persistence_journal::MetadataDelta::Appearance(
            crate::appearance::Appearance::Dark,
        ));
        let mut store = loader.finish();
        store.normalize_topics();
        store.clear_all_tracking();
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
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: self.runs().iter().map(|r|r.id).max().unwrap_or(0)+1,
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
        self.push_run(run);
        self.task_mut(id)
            .unwrap()
            .activity
            .push("Agent run previewed · Simulation only".into());
        Ok(())
    }
}
