//! Presentation order only. Never use these helpers to schedule work or change dependencies.
use crate::domain::{Store, Task};
use std::cmp::{Ordering, Reverse};

pub fn newest_id(id: u64) -> Reverse<u64> {
    Reverse(id)
}

fn creation_time(task: &Task) -> Option<i64> {
    chrono::DateTime::parse_from_str(&task.created_at, "%Y-%m-%d %H:%M:%S %:z")
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(&task.created_at))
        .ok()
        .map(|date| date.timestamp_millis())
}

/// Known creation times first, newest first; equal/missing timestamps use the
/// monotonic task ID. Never reorder a parent behind its children.
pub fn tasks_newest_first(a: &Task, b: &Task) -> Ordering {
    creation_time(b)
        .cmp(&creation_time(a))
        .then_with(|| b.id.cmp(&a.id))
}

/// Sibling research steps follow the workflow; same-phase steps keep plan ID order.
/// Goals and ordinary tasks retain newest-first ordering. This is display-only.
pub fn siblings_in_display_order(a: &Task, b: &Task) -> Ordering {
    let phase = |t: &Task| t.research.as_ref().map(|r| match r.kind {
        crate::research::ResearchKind::Collect => 0,
        crate::research::ResearchKind::Analyze => 1,
        crate::research::ResearchKind::Advise => 2,
        crate::research::ResearchKind::Goal => 3,
    }).unwrap_or(3);
    phase(a).cmp(&phase(b)).then_with(|| {
        if phase(a) < 3 { a.id.cmp(&b.id) } else { tasks_newest_first(a, b) }
    })
}

/// Sort a tree's backing list without mixing sibling ordering across parents.
pub fn tree_siblings_in_display_order(a: &Task, b: &Task) -> Ordering {
    a.parent_id.cmp(&b.parent_id).then_with(|| siblings_in_display_order(a, b))
}

impl Store {
    /// Match the sidebar hierarchy: newest project, newest topic, newest sibling.
    /// Projects/topics do not have timestamps, so use their creation IDs.
    pub fn compare_display_tasks(&self, a: &Task, b: &Task) -> Ordering {
        b.project_id
            .cmp(&a.project_id)
            .then_with(|| {
                self.task_topic(b.id)
                    .map(|t| t.id)
                    .cmp(&self.task_topic(a.id).map(|t| t.id))
            })
            .then_with(|| tasks_newest_first(a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Stage;
    #[test]
    fn research_siblings_follow_phase_and_plan_order_despite_timestamps() {
        let mut s = Store::empty();
        let goal = s.create_research_goal("Research".into(), "Scope".into(), 1).unwrap();
        let ids = s.apply_research_plan(goal, r#"{"tasks":[{"title":"Evidence one","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis one","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Evidence two","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analysis two","instructions":"Analyze","kind":"analyze","depends_on":[2]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1,3]}]}"#).unwrap();
        for (index, id) in ids.iter().enumerate() {
            s.task_mut(*id).unwrap().created_at = format!("2026-11-0{}T10:00:00Z", index + 1);
        }
        s.tasks.reverse();
        let before = s.clone();
        let mut steps: Vec<_> = ids.iter().map(|id| s.task(*id).unwrap()).collect();
        steps.sort_by(|a, b| siblings_in_display_order(a, b));
        assert_eq!(steps.iter().map(|t| t.id).collect::<Vec<_>>(), vec![ids[0], ids[2], ids[1], ids[3], ids[4]]);
        assert_eq!(s, before, "Display sorting leaves dependencies and storage unchanged");
        let old = s.create_task("Old ordinary subtask", 1, Stage::Planned, Some(goal)).unwrap();
        let new = s.create_task("New ordinary subtask", 1, Stage::Planned, Some(goal)).unwrap();
        assert_eq!(siblings_in_display_order(s.task(new).unwrap(), s.task(old).unwrap()), Ordering::Less);
    }

    #[test]
    fn timestamp_order_handles_timezones_ties_and_legacy_records() {
        let mut s = Store::empty();
        let ids: Vec<_> = (0..5)
            .map(|i| {
                s.create_task(format!("Task {i}"), 1, Stage::Planned, None)
                    .unwrap()
            })
            .collect();
        for (id, time) in ids.iter().zip([
            "2026-11-01 10:00:00 +08:00",
            "2026-11-01T03:00:00Z",
            "2026-11-01 11:00:00 +08:00",
            "",
            "invalid",
        ]) {
            s.task_mut(*id).unwrap().created_at = time.into();
        }
        let mut tasks: Vec<_> = s.tasks.iter().collect();
        tasks.sort_by(|a, b| tasks_newest_first(a, b));
        assert_eq!(
            tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![ids[2], ids[1], ids[0], ids[4], ids[3]]
        );
    }
    #[test]
    fn newest_groups_and_siblings_are_independent_of_storage_order() {
        let mut s = Store::empty();
        let old = s.create_topic(1, "Old").unwrap();
        let new = s.create_topic(1, "New").unwrap();
        let a = s
            .create_task_in_topic("Old topic", 1, Stage::Planned, None, Some(old))
            .unwrap();
        let b = s
            .create_task_in_topic("New topic older task", 1, Stage::Planned, None, Some(new))
            .unwrap();
        let c = s
            .create_task_in_topic("New topic latest task", 1, Stage::Planned, None, Some(new))
            .unwrap();
        for t in &mut s.tasks {
            t.created_at = "2026-11-01 10:00:00 +08:00".into();
        }
        s.tasks.reverse();
        let before = s.clone();
        let mut tasks: Vec<_> = s.tasks.iter().collect();
        tasks.sort_by(|a, b| s.compare_display_tasks(a, b));
        assert_eq!(
            tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![c, b, a]
        );
        assert_eq!(
            s, before,
            "Display sorting must not mutate task or queue storage"
        );
    }
}
