//! Execution resources and resume eligibility, independent of the UI event loop.
use crate::{
    domain::{AgentRun, Store},
    research::ResearchKind,
};
use std::path::{Path, PathBuf};

pub const MAX_CONCURRENT_RUNS: usize = 3;

impl Store {
    /// New one-sentence plans isolate each step; legacy goals retain their shared workspace.
    pub fn uses_step_workspace(&self, task_id: u64) -> bool {
        self.task(task_id).and_then(|t| t.research.as_ref()).is_some_and(|r| {
            r.kind != ResearchKind::Goal && self.research_requests.iter().any(|request| request.goal_ids.contains(&r.goal_id))
        })
    }

    pub fn research_space(&self, task_id: u64) -> Option<String> {
        let research = self.task(task_id)?.research.as_ref()?;
        Some(if self.uses_step_workspace(task_id) { format!("{} · step {task_id}", research.space_name) } else { research.space_name.clone() })
    }
    /// Keep an existing research workspace stable; isolate new goals from one another.
    pub fn previous_execution_directory(&self, task_id: u64) -> Result<PathBuf, String> {
        let task = self.task(task_id).ok_or("Task not found.")?;
        let project = self
            .projects
            .iter()
            .find(|p| p.id == task.project_id)
            .ok_or("Project not found.")?;
        if project.directory.is_empty() {
            return Err("Set this project's working folder before running.".into());
        }
        let Some(research) = &task.research else {
            return Ok(PathBuf::from(&project.directory));
        };
        if self.uses_step_workspace(task_id) {
            if let Some(run) = self.runs.iter().rev().find(|r| r.task_id == task_id && r.project_plan.is_none() && r.status != "Simulated" && !r.directory.is_empty()) {
                return Ok(PathBuf::from(run.workspace_source.as_ref().unwrap_or(&run.directory)));
            }
            return self.execution_directory(task_id);
        }
        let same_goal = |run: &&AgentRun| {
            run.project_plan.is_none()
                && run.status != "Simulated"
                && !run.directory.is_empty()
                && self
                    .task(run.task_id)
                    .and_then(|t| t.research.as_ref())
                    .is_some_and(|r| r.goal_id == research.goal_id)
        };
        // Evidence/analysis outputs own the goal's directory. A successful planner
        // returns JSON only, so its old project-root directory need not lock new steps.
        let previous = self
            .runs
            .iter()
            .rev()
            .filter(same_goal)
            .find(|r| r.research_kind != Some(ResearchKind::Goal))
            .or_else(|| {
                self.runs
                    .iter()
                    .rev()
                    .filter(same_goal)
                    .find(|r| research.kind == ResearchKind::Goal || r.status != "Succeeded")
            });
        if let Some(previous) = previous {
            return Ok(PathBuf::from(previous.workspace_source.as_ref().unwrap_or(&previous.directory)));
        }
        Ok(PathBuf::from(&project.directory)
            .join(".research")
            .join(format!("goal-{}", research.goal_id)))
    }

    pub fn execution_directory(&self, task_id: u64) -> Result<PathBuf, String> {
        let task = self.task(task_id).ok_or("Task not found.")?;
        let project = self.projects.iter().find(|p| p.id == task.project_id).ok_or("Project not found.")?;
        if project.directory.is_empty() { return Err("Set this project's working folder before running.".into()); }
        Ok(match &task.research {
            Some(research) => {
                let directory = PathBuf::from(&project.directory).join(".research").join(format!("goal-{}", research.goal_id));
                if self.uses_step_workspace(task_id) { directory.join(format!("step-{task_id}")) } else { directory }
            },
            None => PathBuf::from(&project.directory),
        })
    }

    pub fn workspace_source(&self, task_id: u64) -> Result<Option<String>, String> {
        let target = self.execution_directory(task_id)?;
        // A queued/failed migration still owns its original source, even after restart.
        if let Some(run) = self.runs.iter().rev().find(|r| r.task_id == task_id && r.project_plan.is_none()) {
            if let Some(source) = &run.workspace_source { return Ok(Some(source.clone())); }
        }
        let previous = self.previous_execution_directory(task_id)?;
        Ok((normalized_path(&previous.to_string_lossy()) != normalized_path(&target.to_string_lossy()))
            .then(|| previous.to_string_lossy().into_owned()))
    }

    /// Only the latest attempt may resume: never reach past a completed or stale attempt.
    pub fn resume_candidate(&self, request: &AgentRun) -> Option<&AgentRun> {
        let previous = self
            .runs
            .iter()
            .rev()
            .find(|run| run.status != "Simulated" && same_task(run, request))?;
        (matches!(
            previous.status.as_str(),
            "Interrupted" | "Failed" | "Cancelled" | "Blocked"
        ) && previous
            .session_id
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
            && previous.agent == request.agent
            && previous.model == request.model
            && previous.research_kind == request.research_kind
            && (request.research_kind.is_some() || previous.stage == request.stage)
            && (normalized_path(&previous.directory) == normalized_path(&request.directory)
                || request.workspace_source.as_ref().is_some_and(|source| normalized_path(source) == normalized_path(&previous.directory))
                || std::fs::read_to_string(Path::new(&request.directory).join(".workspace-migrated"))
                    .ok().is_some_and(|source| normalized_path(&source) == normalized_path(&previous.directory))))
        .then_some(previous)
    }

    pub fn run_conflicts(&self, candidate: &AgentRun, active: &AgentRun) -> bool {
        self.run_conflict_reason(candidate, active).is_some()
    }

    pub fn run_conflict_reason(&self, candidate: &AgentRun, active: &AgentRun) -> Option<&'static str> {
        if same_task(candidate, active) { return Some("same task"); }
        if let (Some(a), Some(b)) = (&candidate.session_id, &active.session_id) {
            if a == b { return Some("same OpenCode session"); }
        }
        if candidate.project_plan.is_none() && active.project_plan.is_none() {
            if let (Some(a), Some(b)) = (
                self.task(candidate.task_id).and_then(|t| t.research.as_ref()),
                self.task(active.task_id).and_then(|t| t.research.as_ref()),
            ) {
                if a.goal_id == b.goal_id && !(self.uses_step_workspace(candidate.task_id) && self.uses_step_workspace(active.task_id)) { return Some("same research goal"); }
                if !a.space_name.is_empty() && self.research_space(candidate.task_id) == self.research_space(active.task_id) { return Some("same browser Space"); }
            }
        }
        let overlaps = |a: &str, b: &str| {
            let a = normalized_path(a); let b = normalized_path(b);
            a.starts_with(&b) || b.starts_with(&a)
        };
        if let Some(source) = &candidate.workspace_source {
            if overlaps(source, &active.directory) { return Some("waiting to copy legacy research files: the original folder is still in use"); }
        }
        if overlaps(&candidate.directory, &active.directory) { return Some("overlapping working folders"); }
        None
    }

}

fn same_task(a: &AgentRun, b: &AgentRun) -> bool {
    match (a.project_plan, b.project_plan) {
        (Some(a), Some(b)) => a == b,
        (None, None) => a.task_id == b.task_id,
        _ => false,
    }
}

fn normalized_path(path: &str) -> PathBuf {
    let path = Path::new(path);
    // Resolve existing ancestors too: an isolated goal directory may not exist yet.
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let mut existing = path;
    let mut suffix = Vec::new();
    while let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) {
        suffix.push(name.to_owned());
        existing = parent;
        if let Ok(mut canonical) = existing.canonicalize() {
            for name in suffix.iter().rev() {
                canonical.push(name);
            }
            return canonical;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::Stage, research::ResearchKind};

    fn setup() -> (Store, u64, u64) {
        let mut store = Store::empty();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.default_model = "fixture/model".into();
        let project = store.projects[0].id;
        let a = store
            .create_research_goal("A".into(), "Scope A".into(), project)
            .unwrap();
        let b = store
            .create_research_goal("B".into(), "Scope B".into(), project)
            .unwrap();
        (store, a, b)
    }
    fn request(store: &Store, id: u64) -> AgentRun {
        let mut run = store.prepare_run(id).unwrap();
        run.directory = store
            .execution_directory(id)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        run
    }
    #[test]
    fn request_steps_have_independent_resources_and_keep_retry_paths() {
        let (mut store, goal, _) = setup();
        let steps = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Official","instructions":"Find filings","kind":"collect","depends_on":[]},{"title":"Market","instructions":"Find market evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare","kind":"analyze","depends_on":[0,1]},{"title":"Report","instructions":"Conclude","kind":"advise","depends_on":[2]}]}"#).unwrap();
        let request_id = store.create_research_request(1, None, "Analyze earnings").unwrap();
        store.research_requests.iter_mut().find(|r| r.id == request_id).unwrap().goal_ids.push(goal);
        let first = request(&store, steps[0]);
        let second = request(&store, steps[1]);
        assert_ne!(first.directory, second.directory);
        assert_ne!(store.research_space(steps[0]), store.research_space(steps[1]));
        assert!(!store.run_conflicts(&first, &second));
        assert_eq!(store.workspace_source(steps[0]).unwrap(), None);
        let mut failed = first.clone();
        failed.status = "Failed".into();
        failed.session_id = Some("isolated-retry".into());
        store.runs.push(failed);
        assert_eq!(store.previous_execution_directory(steps[0]).unwrap(), PathBuf::from(&first.directory));
        assert_eq!(store.resume_candidate(&first).unwrap().session_id.as_deref(), Some("isolated-retry"));
        assert!(!store.run_conflicts(&first, &second));
    }

    #[test]
    fn legacy_migration_waits_for_source_then_allows_independent_goals() {
        let (mut store, a, b) = setup();
        let mut legacy = request(&store, a);
        legacy.directory = store.projects[0].directory.clone();
        legacy.research_kind = Some(ResearchKind::Collect);
        legacy.status = "Interrupted".into();
        legacy.session_id = Some("existing-session".into());
        store.runs.push(legacy.clone());
        let mut next = legacy.clone();
        next.directory = store.execution_directory(a).unwrap().to_string_lossy().into_owned();
        next.workspace_source = store.workspace_source(a).unwrap();
        assert_eq!(next.workspace_source.as_ref(), Some(&legacy.directory));
        assert!(store.resume_candidate(&next).is_some());
        let independent = request(&store, b);
        assert!(store.run_conflict_reason(&next, &independent).unwrap().contains("copy legacy"));
        next.workspace_source = None;
        assert!(!store.run_conflicts(&next, &independent));
        // Pending migration remains discoverable after persistence/restart.
        next.workspace_source = Some(legacy.directory.clone());
        store.runs.push(next);
        let restored: Store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(restored.workspace_source(a).unwrap(), Some(legacy.directory));
    }

    #[test]
    fn migration_preserves_originals_excludes_goal_folders_and_is_idempotent() {
        let root = std::env::temp_dir().join(crate::opencode::new_session_id());
        std::fs::create_dir_all(root.join("evidence")).unwrap();
        std::fs::write(root.join("evidence/source.txt"), "verified evidence").unwrap();
        std::fs::create_dir_all(root.join(".research/goal-other")).unwrap();
        std::fs::write(root.join(".research/goal-other/report.md"), "other goal").unwrap();
        let target = root.join(".research/goal-new");
        migrate_workspace(root.to_str().unwrap(), target.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("evidence/source.txt")).unwrap(), "verified evidence");
        assert!(root.join("evidence/source.txt").exists());
        assert!(!target.join(".research").exists());
        std::fs::write(target.join("evidence/source.txt"), "continued research").unwrap();
        migrate_workspace(root.to_str().unwrap(), target.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("evidence/source.txt")).unwrap(), "continued research");
        let collision = root.join(".research/collision");
        std::fs::create_dir_all(&collision).unwrap();
        assert!(migrate_workspace(root.to_str().unwrap(), collision.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_goals_are_isolated_but_legacy_directory_is_retained() {
        let (mut store, a, b) = setup();
        let first = request(&store, a);
        let second = request(&store, b);
        assert!(!store.run_conflicts(&first, &second));
        assert!(store.run_conflicts(&first, &first));
        let mut legacy = first.clone();
        legacy.directory = store.projects[0].directory.clone();
        legacy.status = "Succeeded".into();
        legacy.research_kind = Some(ResearchKind::Collect);
        store.runs.push(legacy.clone());
        assert_eq!(
            store.previous_execution_directory(a).unwrap(),
            PathBuf::from(&legacy.directory)
        );
        assert!(store.run_conflicts(&legacy, &second));
    }
    #[test]
    fn completed_planning_does_not_pin_new_steps_to_shared_root() {
        let (mut store, goal, _) = setup();
        let mut planning = store.prepare_run(goal).unwrap();
        planning.status = "Succeeded".into();
        store.runs.push(planning.clone());
        let steps = store.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect","instructions":"Find sources","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        let isolated = PathBuf::from(&store.projects[0].directory)
            .join(".research")
            .join(format!("goal-{goal}"));
        assert_eq!(store.execution_directory(steps[0]).unwrap(), isolated);
        // The planner itself retains its directory for a failed-plan resume.
        store.runs[0].status = "Interrupted".into();
        assert_eq!(
            store.previous_execution_directory(goal).unwrap(),
            PathBuf::from(&planning.directory)
        );
        assert_eq!(
            store.previous_execution_directory(steps[0]).unwrap(),
            PathBuf::from(&planning.directory)
        );
        store.runs[0].status = "Succeeded".into();
        let mut evidence = store.prepare_run(steps[0]).unwrap();
        evidence.directory = isolated.to_string_lossy().into_owned();
        store.runs.push(evidence);
        assert_eq!(store.execution_directory(steps[1]).unwrap(), isolated);
    }

    #[test]
    fn only_latest_failed_attempt_with_matching_configuration_can_resume() {
        let (mut store, a, _) = setup();
        let next = request(&store, a);
        let mut previous = next.clone();
        previous.status = "Interrupted".into();
        previous.session_id = Some("ses_existing".into());
        store.runs.push(previous.clone());
        assert_eq!(
            store.resume_candidate(&next).unwrap().session_id.as_deref(),
            Some("ses_existing")
        );
        for status in ["Failed", "Cancelled", "Blocked"] {
            store.runs.last_mut().unwrap().status = status.into();
            assert!(store.resume_candidate(&next).is_some());
        }
        let mut changed = next.clone();
        changed.model = "fixture/other".into();
        assert!(store.resume_candidate(&changed).is_none());
        changed = next.clone();
        changed.directory.push_str("/different");
        assert!(store.resume_candidate(&changed).is_none());
        changed = next.clone();
        changed.research_kind = Some(ResearchKind::Analyze);
        assert!(store.resume_candidate(&changed).is_none());
        for status in ["Succeeded", "Stale", "Running", "Cancelling"] {
            previous.status = status.into();
            store.runs.push(previous.clone());
            assert!(store.resume_candidate(&next).is_none());
        }
    }
    #[test]
    fn browser_space_and_session_are_exclusive_across_directories() {
        let (mut store, a, b) = setup();
        let first = request(&store, a);
        let mut second = request(&store, b);
        second.session_id = Some("ses_shared".into());
        let mut with_session = first.clone();
        with_session.session_id = second.session_id.clone();
        assert!(store.run_conflicts(&with_session, &second));
        let space = store
            .task(a)
            .unwrap()
            .research
            .as_ref()
            .unwrap()
            .space_name
            .clone();
        store
            .task_mut(b)
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .space_name = space;
        assert!(store.run_conflicts(&first, &second));
    }
    #[test]
    fn ordinary_tasks_keep_project_directory_and_stage_specific_sessions() {
        let (mut store, _, _) = setup();
        let project = store.projects[0].id;
        let id = store
            .create_task("Task", project, Stage::Planned, None)
            .unwrap();
        let mut next = request(&store, id);
        assert_eq!(next.directory, store.projects[0].directory);
        next.status = "Interrupted".into();
        next.session_id = Some("ses_task".into());
        store.runs.push(next.clone());
        assert!(store.resume_candidate(&next).is_some());
        next.stage = Stage::Done;
        assert!(store.resume_candidate(&next).is_none());
    }
    #[test]
    fn project_planning_identity_is_scoped_to_project() {
        let (store, a, b) = setup();
        let mut first = request(&store, a);
        let mut second = request(&store, b);
        first.project_plan = Some(1);
        second.project_plan = Some(2);
        first.task_id = 0;
        second.task_id = 0;
        assert!(!store.run_conflicts(&first, &second));
        second.project_plan = Some(1);
        assert!(store.run_conflicts(&first, &second));
    }
}

/// Copy once via a staging directory; never alter legacy files or merge partial snapshots.
pub fn migrate_workspace(source: &str, destination: &str) -> Result<(), String> {
    let source = Path::new(source);
    let destination = Path::new(destination);
    let marker = destination.join(".workspace-migrated");
    if marker.is_file() && std::fs::read_to_string(&marker).ok().as_deref() == Some(source.to_string_lossy().as_ref()) { return Ok(()); }
    if destination.exists() { return Err("The isolated workspace already exists without a migration marker; original files were preserved. Review the folder before retrying.".into()); }
    let parent = destination.parent().ok_or("Invalid workspace destination")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let staging = parent.join(format!(".migration-{}-{}", std::process::id(), crate::opencode::new_session_id()));
    fn copy(source: &Path, target: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            // Do not recursively copy isolated workspaces or repository metadata.
            if name == ".research" || name == ".git" { continue; }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(std::io::Error::other(format!("Cannot safely migrate symbolic link: {}", entry.path().display())));
            }
            if kind.is_dir() { copy(&entry.path(), &target.join(name))?; }
            else if kind.is_file() { std::fs::copy(entry.path(), target.join(name))?; }
        }
        Ok(())
    }
    let result = (|| -> std::io::Result<()> {
        copy(source, &staging)?;
        std::fs::write(staging.join(".workspace-migrated"), source.to_string_lossy().as_bytes())?;
        std::fs::rename(&staging, destination)
    })();
    if result.is_err() { let _ = std::fs::remove_dir_all(&staging); }
    result.map_err(|e| format!("Could not copy legacy workspace: {e}. Original files were preserved."))
}
