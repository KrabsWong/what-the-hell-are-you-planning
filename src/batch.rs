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
    pub failed_tasks: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionTopic {
    Topic(u64),
    Task(u64),
}

impl Store {
    pub fn execution_topic(&self, task: u64) -> ExecutionTopic {
        self.task_topic(task).map(|topic| ExecutionTopic::Topic(topic.id))
            .unwrap_or_else(|| ExecutionTopic::Task(self.owning_research_goal(task).unwrap_or(task)))
    }
}

impl TodoBatch {
    pub fn pending_topics(&self, store: &Store) -> Vec<ExecutionTopic> {
        let mut topics = vec![];
        for item in &self.items {
            let topic = store.execution_topic(*item);
            if !topics.contains(&topic) && self.items.iter().any(|id|
                self.contains_pending(*id) && store.execution_topic(*id) == topic) { topics.push(topic); }
        }
        topics
    }

    pub fn current_items(&self, store: &Store) -> Vec<u64> {
        let Some(topic) = self.pending_topics(store).first().copied() else { return vec![]; };
        self.items.iter().copied().filter(|item|
            !self.done_items.contains(item) && !self.failed_items.contains(item)
                && store.execution_topic(*item) == topic).collect()
    }

    pub fn contains_pending(&self, goal: u64) -> bool {
        self.items.contains(&goal) && !self.done_items.contains(&goal) && !self.failed_items.contains(&goal)
    }
}

impl Store {
    pub fn todo_batch_items(&self, project: u64) -> Vec<u64> {
        self.tasks.iter().filter(|task| {
            task.project_id == project && !self.task_is_archived(task.id)
                && match &task.research {
                    Some(research) => matches!(task.stage, Stage::Planned | Stage::InProgress)
                        && research.kind == ResearchKind::Goal
                        && self.goal_execution_approved(task.id)
                        && (!self.has_research_plan(task.id) || self.next_research_task(task.id) != Ok(None)),
                    None => task.stage == Stage::Planned && task.parent_id.is_none(),
                }
        }).map(|task| task.id).collect()
    }

    pub fn goal_tasks_complete(&self, goal: u64) -> bool {
        self.has_research_plan(goal)
            && self.research_children(goal).iter().all(|id| self.task(*id).is_some_and(|t| t.stage == Stage::Done) && self.research_result(*id).is_some())
            && self.tasks.iter().filter(|t| !t.archived && t.research.is_none() && self.owning_research_goal(t.id) == Some(goal)).all(|t| t.stage == Stage::Done)
    }

    /// Internal steps inherit approval from their goal; they are never queue items themselves.
    pub fn ready_research_tasks(&self, goal: u64) -> Result<Vec<u64>, String> {
        if self.task_is_archived(goal) {
            return Err("Restore the archived topic or goal before executing.".into());
        }
        if !self.goal_execution_approved(goal) {
            return Err("Review this goal's complete plan and move it to Todo before executing.".into());
        }
        let children = self.research_children(goal);
        if children.is_empty() { return Ok(vec![goal]); }
        Ok(children.into_iter().filter(|id| {
            let task = self.task(*id).unwrap();
            task.stage != Stage::Cancelled && self.research_result(*id).is_none()
                && task.research.as_ref().unwrap().depends_on.iter().all(|dep| {
                    self.task(*dep).is_some_and(|t| !t.archived && t.stage == Stage::Done && t.research.as_ref().is_some_and(|r| r.goal_id == goal))
                        && self.research_result(*dep).is_some()
                })
        }).collect())
    }

    pub fn next_todo_batch_task(&self, project: u64, item: u64) -> Result<Option<u64>, String> {
        let task = self.task(item).ok_or("A queued goal was removed.")?;
        if self.task_is_archived(item) || task.project_id != project {
            return Err("The queued goal is no longer available in this project.".into());
        }
        if let Some(research) = &task.research {
            if research.kind != ResearchKind::Goal {
                return Err("Execute the parent goal, not an individual research step.".into());
            }
            if !self.goal_execution_approved(item) {
                return Err("Research approval was withdrawn. Review the goal and move it to Todo.".into());
            }
            if self.goal_tasks_complete(item) {
                return Ok(None);
            }
            if task.stage != Stage::Planned && task.stage != Stage::InProgress {
                return Err("Move the goal to Todo before continuing.".into());
            }
            let ready = self.ready_research_tasks(item)?;
            return ready.first().copied().map(Some).ok_or_else(|| "Waiting for research dependencies or a blocked step. Review the goal's progress.".into());
        }
        if task.stage == Stage::Done { return Ok(None); }
        if task.stage != Stage::Planned || task.parent_id.is_some() {
            return Err("Only approved root tasks and goals can be executed from Todo.".into());
        }
        Ok(Some(item))
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_groups_goals_by_topic_in_first_submission_order() {
        let mut store = Store::empty();
        let a = store.create_topic(1, "Topic A").unwrap();
        let b = store.create_topic(1, "Topic B").unwrap();
        let a1 = store.create_research_goal_in_topic("A1".into(), "Scope".into(), 1, Some(a)).unwrap();
        let b1 = store.create_research_goal_in_topic("B1".into(), "Scope".into(), 1, Some(b)).unwrap();
        let a2 = store.create_research_goal_in_topic("A2".into(), "Scope".into(), 1, Some(a)).unwrap();
        let mut batch = TodoBatch {
            project_id: 1, items: vec![a1, b1, a2], completed: 0, runs_completed: 0,
            state: BatchState::Running, run_ids: Default::default(), done_items: vec![], failed_items: vec![], failed_tasks: vec![],
        };
        assert_eq!(batch.pending_topics(&store), vec![ExecutionTopic::Topic(a), ExecutionTopic::Topic(b)]);
        assert_eq!(batch.current_items(&store), vec![a1, a2]);
        batch.done_items.push(a1);
        assert_eq!(batch.current_items(&store), vec![a2]);
        batch.failed_items.push(a2);
        assert_eq!(batch.current_items(&store), vec![b1]);
        assert!(!batch.contains_pending(a1));
        assert!(!batch.contains_pending(a2));
        assert!(batch.contains_pending(b1));
        batch.done_items.push(b1);
        assert!(batch.pending_topics(&store).is_empty());
    }
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
        assert_eq!(s.next_todo_batch_task(p, goal), Ok(Some(ids[0])));
        s.task_mut(goal).unwrap().stage = Stage::Backlog;
        assert!(s.next_todo_batch_task(p, goal).is_err());
        s.task_mut(goal).unwrap().stage = Stage::Planned;
        s.task_mut(goal).unwrap().archived = true;
        assert!(s.next_todo_batch_task(p, goal).is_err());
    }
    #[test]
    fn internal_steps_never_become_independent_todo_items() {
        let (mut s, p, goal) = setup();
        let ids = s.apply_research_plan(goal, PLAN).unwrap();
        s.task_mut(goal).unwrap().stage = Stage::InProgress;
        assert_eq!(s.todo_batch_items(p), vec![goal]);
        assert!(s.next_todo_batch_task(p, ids[0]).is_err());
        s.task_mut(goal).unwrap().stage = Stage::Planned;
        assert_eq!(s.todo_batch_items(p), vec![goal]);
        assert_eq!(s.ready_research_tasks(goal).unwrap(), vec![ids[0]]);
        complete(&mut s, ids[0]);
        assert_eq!(s.ready_research_tasks(goal).unwrap(), vec![ids[1]]);
    }
    #[test]
    fn interrupted_goal_remains_resumable_but_unvalidated_output_cannot_release_dependents() {
        let (mut s, p, goal) = setup();
        let ids = s.apply_research_plan(goal, PLAN).unwrap();
        s.set_stage(goal, Stage::InProgress).unwrap();
        assert_eq!(s.todo_batch_items(p), vec![goal]);
        let mut run = s.prepare_run(ids[0]).unwrap();
        run.status = "Succeeded".into();
        run.output = "Unparsed output awaiting validation".into();
        s.runs.push(run);
        s.set_stage(ids[0], Stage::InProgress).unwrap();
        assert!(s.ready_research_tasks(goal).unwrap().is_empty());
        assert!(!s.goal_tasks_complete(goal));
        s.set_stage(ids[0], Stage::Done).unwrap();
        assert_eq!(s.ready_research_tasks(goal).unwrap(), vec![ids[1]]);
        s.set_stage(goal, Stage::Backlog).unwrap();
        assert!(s.todo_batch_items(p).is_empty());
    }

    #[test]
    fn independent_sources_are_ready_together_but_analysis_waits() {
        let (mut s, _, goal) = setup();
        let ids = s.apply_research_plan(goal, r#"{"tasks":[{"title":"Official","instructions":"Official filings","kind":"collect","depends_on":[]},{"title":"Market","instructions":"Market evidence","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare","kind":"analyze","depends_on":[0,1]},{"title":"Report","instructions":"Conclude","kind":"advise","depends_on":[2]}]}"#).unwrap();
        assert_eq!(s.ready_research_tasks(goal).unwrap(), ids[..2]);
        complete(&mut s, ids[0]);
        assert_eq!(s.ready_research_tasks(goal).unwrap(), vec![ids[1]]);
        complete(&mut s, ids[1]);
        assert_eq!(s.ready_research_tasks(goal).unwrap(), vec![ids[2]]);
        s.task_mut(goal).unwrap().stage = Stage::Backlog;
        assert!(s.ready_research_tasks(goal).is_err());
    }
}
