//! Structural navigation: every Back action opens the immediate breadcrumb parent.
use crate::app::{Scope, TaskApp};
use gpui_kit::*;

impl crate::domain::Store {
    /// Detail navigation follows the project tree, independently of list filters,
    /// task stages and collapsed rows. Wrap at the ends to keep both actions available.
    pub(crate) fn detail_task_neighbors(&self, id: u64) -> (u64, u64) {
        let Some(current) = self.task(id) else { return (id, id); };
        let archived = self.task_is_archived(id);
        let mut tasks: Vec<_> = self.tasks.iter().filter(|task|
            task.project_id == current.project_id && self.task_is_archived(task.id) == archived).collect();
        tasks.sort_by(|a, b| crate::display_order::tree_siblings_in_display_order(a, b));
        let mut roots: Vec<_> = tasks.iter().copied().filter(|task|
            task.parent_id.is_none_or(|parent| !tasks.iter().any(|t| t.id == parent))).collect();
        roots.sort_by(|a, b| self.compare_display_tasks(a, b));
        fn append(tasks: &[&crate::domain::Task], id: u64, ordered: &mut Vec<u64>) {
            if ordered.contains(&id) { return; }
            ordered.push(id);
            for child in tasks.iter().filter(|t| t.parent_id == Some(id)) {
                append(tasks, child.id, ordered);
            }
        }
        let mut ordered = vec![];
        for root in roots { append(&tasks, root.id, &mut ordered); }
        let Some(index) = ordered.iter().position(|task| *task == id) else { return (id, id); };
        (ordered[(index + ordered.len() - 1) % ordered.len()], ordered[(index + 1) % ordered.len()])
    }
}

#[derive(Clone)]
pub(crate) enum CrumbTarget {
    View(Scope),
    Task(u64),
}
impl TaskApp {
    pub(crate) fn displayed_run_id(&self) -> Option<u64> {
        self.selected_run
            .filter(|id| self.store.runs.iter().any(|r| r.id == *id))
            .or_else(|| self.store.runs.last().map(|r| r.id))
    }

    pub(crate) fn clear_navigation_overlays(&mut self) {
        self.save_feedback = None;
        self.draft = None;
        self.editing = false;
        self.property = None;
        self.new_project = None;
        self.topic_editor = None;
        self.project_rename = None;
        self.topic_move = None;
        self.planning_project = None;
        self.planning_topic = None;
        self.directory_project = None;
        self.error.clear();
    }

    pub(crate) fn go_back(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let (parents, _) = self.navigation_path();
        if let Some((_, _, target)) = parents.last() {
            match target {
                CrumbTarget::View(scope) => self.navigate(scope.clone(), w, cx),
                CrumbTarget::Task(id) => self.open_task(*id, w, cx),
            }
        }
    }
    pub(crate) fn close_current_task(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        self.tabs.retain(|task| *task != id);
        self.go_back(w, cx);
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{Stage, Store};

    #[test]
    fn detail_navigation_includes_completed_steps_and_wraps_across_topics() {
        let mut store = Store::empty();
        let topic_a = store.create_topic(1, "A").unwrap();
        let topic_b = store.create_topic(1, "B").unwrap();
        let a = store.create_research_goal_in_topic("A goal".into(), "Scope".into(), 1, Some(topic_a)).unwrap();
        // Create B before A's children: navigation must still follow the hierarchy.
        let b = store.create_research_goal_in_topic("B goal".into(), "Scope".into(), 1, Some(topic_b)).unwrap();
        let steps = store.apply_research_plan(a, r#"{"tasks":[{"title":"Evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        store.task_mut(steps[0]).unwrap().stage = Stage::Done;
        store.task_mut(steps[1]).unwrap().stage = Stage::InProgress;
        store.task_mut(steps[2]).unwrap().stage = Stage::Cancelled;
        assert_eq!(store.detail_task_neighbors(steps[0]), (a, steps[1]));
        assert_eq!(store.detail_task_neighbors(steps[1]), (steps[0], steps[2]));
        assert_eq!(store.detail_task_neighbors(steps[2]), (steps[1], b));
        assert_eq!(store.detail_task_neighbors(b), (steps[2], a));
        assert_eq!(store.detail_task_neighbors(a), (b, steps[0]));
    }

    #[test]
    fn single_task_navigation_stays_on_current_task() {
        let mut store = Store::empty();
        let id = store.create_research_goal("Only goal".into(), "Scope".into(), 1).unwrap();
        assert_eq!(store.detail_task_neighbors(id), (id, id));
    }
}
