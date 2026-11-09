//! Lifecycle-aware archived research presentation.
//!
//! Archive is an admission state, not a request for approval. These helpers may
//! read retained run output for history, but are never used to admit execution.
use crate::{
    domain::{AgentRun, Stage, Store},
    research::{ResearchKind, ResearchTask},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedGoalHistory {
    pub total_steps: usize,
    pub completed_steps: usize,
    pub pending_steps: usize,
    pub retained_steps: usize,
    pub complete: bool,
    pub retained_reports: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedPresentation {
    pub label: String,
    pub phase: Stage,
    pub complete: bool,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchivedOwner {
    Topic(u64),
    Task(u64),
}

/// Latest succeeded, non-empty run for a task, independent of current execution
/// validation. This is history access only and must not be used to run work.
pub fn retained_result<'a>(store: &'a Store, task: u64) -> Option<&'a AgentRun> {
    store
        .runs()
        .iter()
        .filter(|run| {
            run.task_id == task
                && run.project_plan.is_none()
                && run.automation_occurrence_id.is_none()
                && run.status == "Succeeded"
                && !run.output.trim().is_empty()
        })
        .max_by_key(|run| run.id)
}

fn research_child(task: &crate::domain::Task, goal: u64) -> Option<&ResearchTask> {
    task.research.as_ref().filter(|research| {
        research.goal_id == goal
            && research.kind != ResearchKind::Goal
            && task.automation_id.is_none()
    })
}

/// Signature-matched, quality-valid historical result. Archive admission is
/// ignored, but stale input revisions and malformed report envelopes are not
/// treated as completed outcomes.
pub fn validated_history_result<'a>(store: &'a Store, task: u64) -> Option<&'a AgentRun> {
    store.research_history_result(task)
}

/// Archived current result whose durable body is still lazy. This is not a
/// validated quality claim; it only prevents "no retained result" while the
/// saved body awaits hydration.
pub fn pending_history_result<'a>(store: &'a Store, task: u64) -> Option<&'a AgentRun> {
    store.research_history_result_pending(task)
}

/// Retained content (valid or invalid) plus a lazy current body. Used for
/// history affordances, never for execution or prompt/export content.
pub fn retained_or_pending_result<'a>(store: &'a Store, task: u64) -> Option<&'a AgentRun> {
    retained_result(store, task).or_else(|| pending_history_result(store, task))
}

pub fn goal_history(store: &Store, goal: u64) -> ArchivedGoalHistory {
    let steps: Vec<&crate::domain::Task> = store
        .tasks()
        .iter()
        .filter(|task| research_child(task, goal).is_some())
        .collect();
    let completed_steps = steps
        .iter()
        .filter(|task| validated_history_result(store, task.id).is_some())
        .count();
    let pending_steps = steps
        .iter()
        .filter(|task| pending_history_result(store, task.id).is_some())
        .count();
    let retained_steps = steps
        .iter()
        .filter(|task| retained_or_pending_result(store, task.id).is_some())
        .count();
    let retained_reports = steps
        .iter()
        .filter(|task| {
            task.research
                .as_ref()
                .is_some_and(|research| research.kind == ResearchKind::Advise)
                && retained_or_pending_result(store, task.id).is_some()
        })
        .count();
    ArchivedGoalHistory {
        total_steps: steps.len(),
        completed_steps,
        pending_steps,
        retained_steps,
        complete: !steps.is_empty() && completed_steps == steps.len(),
        retained_reports,
    }
}

pub fn presentation(store: &Store, id: u64) -> Option<ArchivedPresentation> {
    let task = store.task(id)?;
    if !store.task_is_archived(id) {
        return None;
    }
    let is_goal = task
        .research
        .as_ref()
        .is_some_and(|research| research.kind == ResearchKind::Goal);
    if is_goal {
        let history = goal_history(store, id);
        let label = if history.total_steps == 0 {
            "Archived · no research steps".to_string()
        } else if history.complete {
            format!(
                "Archived · {}/{} complete",
                history.completed_steps, history.total_steps
            )
        } else if history.pending_steps > 0 {
            format!(
                "Archived · {}/{} validated · {} saved",
                history.completed_steps, history.total_steps, history.pending_steps
            )
        } else {
            format!(
                "Archived · {}/{} validated",
                history.completed_steps, history.total_steps
            )
        };
        return Some(ArchivedPresentation {
            label,
            phase: if history.complete {
                Stage::Done
            } else if task.stage == Stage::Done {
                Stage::Review
            } else {
                task.stage
            },
            complete: history.complete,
            reason: if history.complete {
                "Archived history. Retained results remain available. Restore to continue this goal."
                    .into()
            } else if history.pending_steps > 0 {
                "Archived history. Saved results are available; open them to review. Some still need validation. Restore to continue this goal."
                    .into()
            } else if history.retained_steps > history.completed_steps {
                "Archived history. Some retained attempts remain readable but do not count toward the original completed revision. Restore to continue this goal."
                    .into()
            } else {
                "Archived history. Unfinished work will not run. Restore to continue this goal."
                    .into()
            },
        });
    }
    if task.research.is_some() {
        let pending = pending_history_result(store, id).is_some();
        let retained = retained_result(store, id).is_some();
        let validated = validated_history_result(store, id).is_some();
        return Some(ArchivedPresentation {
            label: if validated {
                "Archived · result retained".into()
            } else if pending {
                "Archived · saved result available".into()
            } else if retained {
                "Archived · retained attempt, not validated".into()
            } else {
                "Archived · no retained result".into()
            },
            phase: if validated {
                Stage::Done
            } else if task.stage == Stage::Done {
                Stage::Review
            } else {
                task.stage
            },
            complete: validated,
            reason: if validated {
                "Archived step history. The retained result remains available; restore the goal before running again."
                    .into()
            } else if pending {
                "Archived step history. A saved result is available; open it to review. Restore the goal before running again."
                    .into()
            } else if retained {
                "Archived step history. A saved attempt is readable but was not part of the validated completion; restore the goal before continuing."
                    .into()
            } else {
                "Archived step history. Restore the goal before continuing.".into()
            },
        });
    }
    Some(ArchivedPresentation {
        label: "Archived".into(),
        phase: task.stage,
        complete: false,
        reason: "Archived. Restore to continue.".into(),
    })
}

/// The narrowest owner whose archived flag must be cleared for this task to
/// become active again. Topic archive takes precedence over task flags.
pub fn restore_owner(store: &Store, id: u64) -> Option<ArchivedOwner> {
    if !store.task_is_archived(id) {
        return None;
    }
    if let Some(topic) = store.task_topic(id)
        && topic.archived
    {
        return Some(ArchivedOwner::Topic(topic.id));
    }
    let mut current = Some(id);
    let mut visited = std::collections::HashSet::new();
    while let Some(task) = current.and_then(|task| store.task(task)) {
        if !visited.insert(task.id) {
            break;
        }
        if task.archived {
            return Some(ArchivedOwner::Task(task.id));
        }
        current = task
            .parent_id
            .or_else(|| task.research.as_ref().filter(|r| r.goal_id != task.id).map(|r| r.goal_id));
    }
    None
}

/// Retained report target for an archived goal or step.
pub fn result_target(store: &Store, id: u64, is_goal: bool) -> Option<u64> {
    if !store.task_is_archived(id) {
        return None;
    }
    if !is_goal {
        return retained_or_pending_result(store, id).map(|_| id);
    }
    let steps: Vec<&crate::domain::Task> = store
        .tasks()
        .iter()
        .filter(|task| research_child(task, id).is_some())
        .collect();
    steps
        .iter()
        .filter(|task| {
            task.research
                .as_ref()
                .is_some_and(|research| research.kind == ResearchKind::Advise)
                && retained_or_pending_result(store, task.id).is_some()
        })
        .max_by_key(|task| task.id)
        .map(|task| task.id)
        .or_else(|| {
            steps
                .iter()
                .filter(|task| retained_or_pending_result(store, task.id).is_some())
                .max_by_key(|task| task.id)
                .map(|task| task.id)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Stage, Store};

    const PLAN: &str = r#"{"tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

    fn completed_goal() -> (Store, u64, Vec<u64>) {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.set_default_model("test/model".into());
        let goal = store
            .create_research_goal("Archived goal".into(), "Scope".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        for task in &steps {
            let mut run = store.prepare_run(*task).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Retained report for {task}");
            store.push_run(run);
            store.set_stage(*task, Stage::Done).unwrap();
        }
        (store, goal, steps)
    }

    #[test]
    fn archived_completion_retains_history_and_restores_current_completion() {
        let (mut store, goal, steps) = completed_goal();
        let report = retained_result(&store, steps[2]).unwrap().id;
        let report_body = retained_result(&store, steps[2]).unwrap().output.clone();
        let plan_signature = store.goal_plan_signature(goal);
        let approved_plan = store.task(goal).unwrap().approved_plan.clone();
        let stage = store.task(goal).unwrap().stage;
        assert!(store.goal_tasks_complete(goal));
        assert!(store.task_status(goal, None).label().contains("Done"));

        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.completed_steps, 3);
        assert_eq!(history.total_steps, 3);
        assert!(history.complete);
        let archived = presentation(&store, goal).unwrap();
        assert_eq!(archived.label, "Archived · 3/3 complete");
        assert_eq!(archived.phase, Stage::Done);
        assert!(store.task_status(goal, None).label().contains("Archived · 3/3 complete"));
        assert!(!store.goal_tasks_complete(goal), "archived work is not executable");
        assert_eq!(result_target(&store, goal, true), Some(steps[2]));
        assert_eq!(
            retained_result(&store, steps[2]).map(|run| run.output.as_str()),
            Some(report_body.as_str())
        );
        assert_eq!(store.task(goal).unwrap().approved_plan, approved_plan);
        assert_eq!(store.goal_plan_signature(goal), plan_signature);
        assert_eq!(store.task(goal).unwrap().stage, stage);

        store.restore(goal).unwrap();
        assert!(presentation(&store, goal).is_none());
        assert!(store.goal_tasks_complete(goal));
        assert_eq!(store.research_result(steps[2]).map(|run| run.id), Some(report));
    }

    #[test]
    fn malformed_success_does_not_claim_completed_outcome() {
        let (mut store, goal, steps) = completed_goal();
        let report_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[2])
            .unwrap()
            .id;
        store
            .run_mut(report_run)
            .unwrap()
            .output = r#"{"status":"complete","report":"unvalidated"#.into();
        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.completed_steps, 2);
        assert_eq!(history.retained_steps, 3);
        assert!(!history.complete);
        let archived = presentation(&store, goal).unwrap();
        assert_eq!(archived.label, "Archived · 2/3 validated");
        assert!(archived.reason.contains("retained attempts"));
        assert_eq!(result_target(&store, goal, true), Some(steps[2]));
        assert!(retained_result(&store, steps[2]).is_some());
    }

    #[test]
    fn stale_prior_input_does_not_claim_completed_outcome() {
        let (mut store, goal, steps) = completed_goal();
        store
            .task_mut(steps[2])
            .unwrap()
            .description
            .push_str(" changed after completion");
        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.completed_steps, 2);
        assert_eq!(history.retained_steps, 3);
        assert!(!history.complete);
        assert_eq!(presentation(&store, goal).unwrap().label, "Archived · 2/3 validated");
    }

    #[test]
    fn failed_regeneration_keeps_prior_validated_report() {
        let (mut store, goal, steps) = completed_goal();
        let prior = store.runs().iter().find(|run| run.task_id == steps[2]).unwrap().clone();
        let mut failed = crate::research::prepare_report_regeneration(&store, prior.id, "retry").unwrap();
        failed.status = "Failed".into();
        failed.output = r#"{"status":"complete","report":"unvalidated retry"#.into();
        store.push_run(failed);
        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.completed_steps, 3);
        assert!(history.complete);
        assert_eq!(validated_history_result(&store, steps[2]).map(|run| run.id), Some(prior.id));
    }

    #[test]
    fn archived_step_keeps_invalid_attempt_readable_but_not_done() {
        let (mut store, goal, steps) = completed_goal();
        let report_run = store
            .runs()
            .iter()
            .find(|run| run.task_id == steps[2])
            .unwrap()
            .id;
        store
            .run_mut(report_run)
            .unwrap()
            .output = r#"{"status":"complete","report":"unvalidated"#.into();
        store.archive(goal).unwrap();
        let step = presentation(&store, steps[2]).unwrap();
        assert!(!step.complete);
        assert_eq!(step.phase, Stage::Review);
        assert_eq!(step.label, "Archived · retained attempt, not validated");
        let status = store.task_status(steps[2], None);
        assert!(status.label().contains("Archived · retained attempt, not validated"));
        assert!(!status.label().contains("Done"));
        assert!(retained_result(&store, steps[2]).is_some());
        assert!(validated_history_result(&store, steps[2]).is_none());
    }

    #[test]
    fn archived_step_stale_input_is_not_done() {
        let (mut store, goal, steps) = completed_goal();
        store
            .task_mut(steps[2])
            .unwrap()
            .description
            .push_str(" changed after completion");
        store.archive(goal).unwrap();
        let step = presentation(&store, steps[2]).unwrap();
        assert!(!step.complete);
        assert_eq!(step.phase, Stage::Review);
        assert!(validated_history_result(&store, steps[2]).is_none());
        assert!(retained_result(&store, steps[2]).is_some());
    }

    #[test]
    fn archived_owner_prefers_topic_then_nearest_task() {
        let (mut store, goal, _) = completed_goal();
        let topic = store.task_topic(goal).unwrap().id;
        store.archive(goal).unwrap();
        assert_eq!(restore_owner(&store, goal), Some(ArchivedOwner::Task(goal)));
        store.restore(goal).unwrap();
        store.set_topic_archived(topic, true).unwrap();
        assert_eq!(restore_owner(&store, goal), Some(ArchivedOwner::Topic(topic)));
        store.set_topic_archived(topic, false).unwrap();
        assert_eq!(restore_owner(&store, goal), None);
    }

    #[test]
    fn nested_topic_and_goal_archive_restore_in_order() {
        let (mut store, goal, _) = completed_goal();
        let topic = store.task_topic(goal).unwrap().id;
        store.set_topic_archived(topic, true).unwrap();
        store.archive(goal).unwrap();
        assert_eq!(restore_owner(&store, goal), Some(ArchivedOwner::Topic(topic)));
        store.set_topic_archived(topic, false).unwrap();
        assert!(store.task_is_archived(goal));
        assert_eq!(restore_owner(&store, goal), Some(ArchivedOwner::Task(goal)));
        store.restore(goal).unwrap();
        assert!(!store.task_is_archived(goal));
        assert_eq!(restore_owner(&store, goal), None);
    }

    #[test]
    fn historical_validation_terminates_on_a_stored_dependency_cycle() {
        let (mut store, goal, steps) = completed_goal();
        store
            .task_mut(steps[0])
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .depends_on = vec![steps[2]];
        store
            .task_mut(steps[2])
            .unwrap()
            .research
            .as_mut()
            .unwrap()
            .depends_on = vec![steps[0]];
        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.completed_steps, 0);
        assert!(!history.complete);
    }

    #[test]
    fn historical_validation_reuses_shared_dependencies() {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.set_default_model("test/model".into());
        let goal = store
            .create_research_goal("Shared dependencies".into(), "Scope".into(), 1)
            .unwrap();
        let steps = store
            .apply_research_plan(
                goal,
                r#"{"tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze A","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Analyze B","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Analyze C","instructions":"Analyze","kind":"analyze","depends_on":[1]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[2,3]}]}"#,
            )
            .unwrap();
        for task in &steps {
            let mut run = store.prepare_run(*task).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Shared result {task}");
            store.push_run(run);
            store.set_stage(*task, Stage::Done).unwrap();
        }
        store.archive(goal).unwrap();
        let history = goal_history(&store, goal);
        assert_eq!(history.total_steps, 5);
        assert_eq!(history.completed_steps, 5);
        assert!(history.complete);
    }
    #[test]
    fn lazy_archived_bodies_report_loading_not_no_retained_result() {
        let root = std::env::temp_dir().join(format!(
            "planning-r170-archive-lazy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = root.to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        let goal = store
            .create_research_goal("Archived lazy goal".into(), "Scope".into(), 1)
            .unwrap();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        for task in &steps {
            let mut run = store.prepare_run(*task).unwrap();
            run.status = "Succeeded".into();
            run.output = format!("Retained report for {task}");
            store.push_run(run);
            store.set_stage(*task, Stage::Done).unwrap();
        }
        store.archive(goal).unwrap();
        let path = root.join("workspace.sqlite3");
        let database =
            crate::sqlite_v4::SqliteV4::create_seeded(&path, &root, 0x52_31_37_31, &store)
                .unwrap();
        let mut lazy = database.load_store_lazy().unwrap();
        let history = goal_history(&lazy, goal);
        assert_eq!(history.completed_steps, 0);
        assert_eq!(history.pending_steps, 3);
        assert_eq!(history.retained_steps, 3);
        assert!(!history.complete);
        let archived = presentation(&lazy, goal).unwrap();
        assert!(archived.label.contains("saved"));
        assert!(!archived.label.contains("no retained"));
        for step in &steps {
            let step_presentation = presentation(&lazy, *step).unwrap();
            assert!(step_presentation.label.contains("saved result available"));
            assert!(!step_presentation.label.contains("no retained"));
        }
        assert_eq!(result_target(&lazy, goal, true), Some(steps[2]));

        let final_run_id = lazy
            .runs()
            .iter()
            .find(|run| run.task_id == steps[2] && run.status == "Succeeded")
            .map(|run| run.id)
            .unwrap();
        let body = database.body(final_run_id).unwrap().unwrap();
        assert!(lazy.hydrate_run_body(final_run_id, body));
        let history = goal_history(&lazy, goal);
        assert_eq!(history.completed_steps, 0);
        assert_eq!(history.pending_steps, 3);
        assert!(!history.complete);
        assert!(presentation(&lazy, steps[2])
            .unwrap()
            .label
            .contains("saved result available"));
        assert!(presentation(&lazy, steps[0])
            .unwrap()
            .label
            .contains("saved result available"));
        assert!(!lazy.goal_tasks_complete(goal), "archived work remains non-executable");

        for step in &steps[..2] {
            let run_id = lazy
                .runs()
                .iter()
                .find(|run| run.task_id == *step && run.status == "Succeeded")
                .map(|run| run.id)
                .unwrap();
            let body = database.body(run_id).unwrap().unwrap();
            assert!(lazy.hydrate_run_body(run_id, body));
        }
        let history = goal_history(&lazy, goal);
        assert_eq!(history.completed_steps, 3);
        assert!(history.complete);
        assert_eq!(presentation(&lazy, goal).unwrap().label, "Archived · 3/3 complete");
        assert!(!lazy.goal_tasks_complete(goal));

        lazy.task_mut(steps[0]).unwrap().description.push_str(" changed scope");
        let history = goal_history(&lazy, goal);
        assert_eq!(history.completed_steps, 0);
        assert_eq!(history.pending_steps, 0);
        assert!(!history.complete);
        let _ = std::fs::remove_dir_all(root);
    }

}
