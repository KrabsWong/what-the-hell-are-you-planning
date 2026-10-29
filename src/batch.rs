//! A user-started, in-memory queue. Saved runs remain the source of research progress.
use crate::{
    domain::{Stage, Store},
    research::ResearchKind,
};

#[derive(Clone, Debug, PartialEq)]
pub enum BatchState {
    Running,
    Paused(String),
    Finished,
}

pub struct TodoBatch {
    pub project_id: u64,
    pub items: Vec<u64>,
    pub completed: usize,
    pub runs_completed: usize,
    pub state: BatchState,
    pub run_ids: std::collections::BTreeMap<u64, u64>,
    pub done_items: Vec<u64>,
    pub failed_items: Vec<u64>,
}

impl Store {
    pub fn todo_batch_items(&self, project: u64) -> Vec<u64> {
        let mut items: Vec<_> = self
            .tasks
            .iter()
            .filter(|task| {
                if task.project_id != project || task.archived || task.stage != Stage::Planned {
                    return false;
                }
                if let Some(research) = &task.research {
                    if self.research_pending_approval(task.id) {
                        return false;
                    }
                    if research.kind == ResearchKind::Goal {
                        return !self.has_research_plan(task.id)
                            || self.next_research_task(task.id) != Ok(None);
                    }
                    if self.task(research.goal_id).is_none_or(|goal| {
                        goal.archived || matches!(goal.stage, Stage::Backlog | Stage::Cancelled)
                    }) {
                        return false;
                    }
                    if self.research_result(task.id).is_some() {
                        return false;
                    }
                    // One goal owns its whole chain; do not enqueue its steps again.
                    if self.task(research.goal_id).is_some_and(|goal| {
                        !goal.archived && goal.project_id == project && goal.stage == Stage::Planned
                    }) {
                        return false;
                    }
                }
                true
            })
            .map(|task| task.id)
            .collect();
        // Validated plans create dependencies before their consumers.
        items.sort_unstable();
        items
    }

    pub fn next_todo_batch_task(&self, project: u64, item: u64) -> Result<Option<u64>, String> {
        let task = self.task(item).ok_or("A queued task was removed.")?;
        if !task.archived
            && task.project_id == project
            && task.stage == Stage::Done
            && self.research_result(item).is_some()
        {
            return Ok(None);
        }
        if task.archived || task.project_id != project || task.stage != Stage::Planned {
            return Err(format!(
                "{} is no longer in this project's Todo. Restore it to Todo or cancel the batch.",
                self.identifier(task)
            ));
        }
        if self.research_pending_approval(item) {
            return Err(
                "Research approval was withdrawn. Approve the topic before continuing.".into(),
            );
        }
        if let Some(r) = &task.research
            && self.task(r.goal_id).is_none_or(|goal| {
                goal.archived || matches!(goal.stage, Stage::Backlog | Stage::Cancelled)
            })
        {
            return Err("The research goal is no longer approved or available.".into());
        }
        let next = match &task.research {
            Some(r) if r.kind == ResearchKind::Goal && self.has_research_plan(item) => {
                self.next_research_task(item)?
            }
            Some(r) if r.kind != ResearchKind::Goal && self.research_result(item).is_some() => None,
            _ => Some(item),
        };
        if let Some(id) = next {
            let step = self.task(id).ok_or("A research step was removed.")?;
            // A selected goal authorizes refreshing stale Done results, but not Backlog steps.
            if step.project_id != project
                || step.archived
                || !matches!(step.stage, Stage::Planned | Stage::Done)
            {
                return Err(format!(
                    "Move {} to Todo before continuing this research chain.",
                    self.identifier(step)
                ));
            }
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Find sources","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze evidence","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write report","kind":"advise","depends_on":[1]}]}"#;
    fn setup() -> (Store, u64, u64) {
        let mut store = Store::empty();
        let project = store.projects[0].id;
        store.projects[0].directory = "/tmp".into();
        store.default_model = "fixture/model".into();
        let goal = store
            .create_research_goal("Goal".into(), "Research scope".into(), project)
            .unwrap();
        (store, project, goal)
    }
    fn complete(store: &mut Store, id: u64) {
        let mut run = store.prepare_run(id).unwrap();
        run.status = "Succeeded".into();
        run.output = "Saved evidence".into();
        store.runs.push(run);
        store.set_stage(id, Stage::Done).unwrap();
    }
    #[test]
    fn project_snapshot_excludes_backlog_and_deduplicates_steps() {
        let (mut s, p, goal) = setup();
        let steps = s.apply_research_plan(goal, PLAN).unwrap();
        let ordinary = s.create_task("Ordinary", p, Stage::Planned, None).unwrap();
        let backlog = s
            .create_research_goal("Backlog".into(), "Scope".into(), p)
            .unwrap();
        s.task_mut(backlog).unwrap().stage = Stage::Backlog;
        let other = s
            .create_research_goal("Other".into(), "Scope".into(), p)
            .unwrap();
        s.task_mut(other).unwrap().project_id = p + 1;
        s.tasks.reverse();
        assert_eq!(s.todo_batch_items(p), vec![goal, ordinary]);
        assert_eq!(s.next_todo_batch_task(p, goal), Ok(Some(steps[0])));
    }
    #[test]
    fn completed_chains_are_reused_and_changed_inputs_are_refreshed() {
        let (mut s, p, goal) = setup();
        assert_eq!(s.next_todo_batch_task(p, goal), Ok(Some(goal)));
        let ids = s.apply_research_plan(goal, PLAN).unwrap();
        for id in &ids {
            complete(&mut s, *id);
        }
        assert!(s.todo_batch_items(p).is_empty());
        assert_eq!(s.next_todo_batch_task(p, goal), Ok(None));
        s.add_comment(ids[1], "New information").unwrap();
        assert_eq!(s.todo_batch_items(p), vec![goal]);
        assert_eq!(s.next_todo_batch_task(p, goal), Ok(Some(ids[1])));
        assert!(s.research_result(ids[0]).is_some());
    }
    #[test]
    fn withdrawal_or_backlog_dependency_stops_execution() {
        let (mut s, p, goal) = setup();
        let ids = s.apply_research_plan(goal, PLAN).unwrap();
        s.task_mut(ids[0]).unwrap().stage = Stage::Backlog;
        assert!(s.next_todo_batch_task(p, goal).is_err());
        s.task_mut(goal).unwrap().stage = Stage::Backlog;
        assert!(s.next_todo_batch_task(p, goal).is_err());
        s.task_mut(goal).unwrap().stage = Stage::Planned;
        s.task_mut(goal).unwrap().archived = true;
        assert!(s.next_todo_batch_task(p, goal).is_err());
    }
    #[test]
    fn standalone_steps_keep_dependency_order() {
        let (mut s, p, goal) = setup();
        let ids = s.apply_research_plan(goal, PLAN).unwrap();
        s.task_mut(goal).unwrap().stage = Stage::InProgress;
        s.tasks.reverse();
        assert_eq!(s.todo_batch_items(p), ids);
        complete(&mut s, ids[0]);
        assert_eq!(s.next_todo_batch_task(p, ids[0]), Ok(None));
        assert_eq!(s.todo_batch_items(p), ids[1..]);
        assert!(s.prepare_run(ids[1]).is_ok());
    }
}
