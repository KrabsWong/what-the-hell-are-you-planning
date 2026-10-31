//! Automatic retries reuse the normal queue, resource locks and approval checks.
use crate::domain::{AgentRun, Stage, Store};

pub const MAX_AUTO_RETRIES: u8 = 3;
pub const RETRY_DELAY_SECONDS: i64 = 30;

impl AgentRun {
    pub fn retry_delay_remaining(&self, now: i64) -> i64 {
        self.retry_not_before
            .map_or(0, |deadline| (deadline - now).max(0))
    }
}

impl Store {
    pub fn prepare_auto_retry(&self, failed_id: u64, now: i64) -> Option<AgentRun> {
        let failed = self.runs.iter().find(|run| run.id == failed_id)?;
        if failed.status != "Failed"
            || failed.project_plan.is_some()
            || failed.auto_retry_attempt >= MAX_AUTO_RETRIES
            || self.run_is_archived(failed)
            || self.research_pending_approval(failed.task_id)
        {
            return None;
        }
        let task = self.task(failed.task_id)?;
        if task.stage == Stage::Cancelled
            || self
                .runs
                .iter()
                .rev()
                .find(|run| run.task_id == failed.task_id && run.project_plan.is_none())?
                .id
                != failed_id
        {
            return None;
        }
        // Retry the approved snapshot only. Dispatch checks it again after the delay.
        if failed.research_kind.is_some() {
            if self.research_signature(task.id).as_ref() != Some(&failed.input_signature) {
                return None;
            }
        } else if task.stage != failed.stage {
            return None;
        }
        let mut retry = failed.clone();
        retry.id = self.runs.iter().map(|run| run.id).max()?.checked_add(1)?;
        retry.auto_retry_attempt += 1;
        retry.retry_not_before = Some(now + RETRY_DELAY_SECONDS);
        retry.status = "Queued".into();
        retry.queued_at = crate::opencode::timestamp();
        retry.started_at.clear();
        retry.ended_at = None;
        retry.exit_code = None;
        retry.output.clear();
        retry.sources.clear();
        retry.logs = vec![format!(
            "[retry] Attempt {}/{} scheduled after {} seconds because run {} failed. Stop task cancels this retry.",
            retry.auto_retry_attempt, MAX_AUTO_RETRIES, RETRY_DELAY_SECONDS, failed.id
        )];
        // The snapshot already contains extracted materials, workspace instructions and
        // the session identity. No prompt expansion or fresh external action is needed.
        Some(retry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_task() -> (Store, u64) {
        let mut store = Store::mock();
        store.simulate_run(1).unwrap();
        let run = store.runs.last_mut().unwrap();
        run.status = "Failed".into();
        run.session_id = Some("existing-session".into());
        run.output = "Partial output".into();
        run.exit_code = Some(1);
        let id = run.id;
        (store, id)
    }

    #[test]
    fn three_delayed_retries_preserve_snapshot_and_session() {
        let (mut store, mut id) = failed_task();
        let original = store.runs.last().unwrap().clone();
        for attempt in 1..=3 {
            let mut retry = store.prepare_auto_retry(id, 100).unwrap();
            assert_eq!(retry.auto_retry_attempt, attempt);
            assert_eq!(retry.status, "Queued");
            assert_eq!(retry.retry_delay_remaining(100), 30);
            assert_eq!(retry.retry_delay_remaining(129), 1);
            assert_eq!(retry.retry_delay_remaining(130), 0);
            assert_eq!(retry.retry_delay_remaining(200), 0);
            assert_eq!(retry.session_id, original.session_id);
            assert_eq!(retry.prompt, original.prompt);
            assert_eq!(retry.directory, original.directory);
            assert!(retry.output.is_empty());
            assert!(retry.exit_code.is_none());
            retry.status = "Failed".into();
            id = retry.id;
            store.runs.push(retry);
            // Restart/serialization must not erase the retry budget.
            store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        }
        assert!(store.prepare_auto_retry(id, 100).is_none());
        assert!(store.prepare_auto_retry(original.id, 100).is_none());
    }

    #[test]
    fn only_failures_retry_and_archived_cancelled_or_changed_work_does_not() {
        for status in [
            "Succeeded",
            "Blocked",
            "Cancelled",
            "Cancelling",
            "Interrupted",
            "Stale",
            "Queued",
            "Running",
        ] {
            let (mut store, id) = failed_task();
            store.runs.last_mut().unwrap().status = status.into();
            assert!(store.prepare_auto_retry(id, 0).is_none(), "{status}");
        }
        let (mut store, id) = failed_task();
        store.task_mut(1).unwrap().archived = true;
        assert!(store.prepare_auto_retry(id, 0).is_none());
        store.task_mut(1).unwrap().archived = false;
        store.task_mut(1).unwrap().stage = Stage::Cancelled;
        assert!(store.prepare_auto_retry(id, 0).is_none());
        let (mut store, id) = failed_task();
        store.runs.last_mut().unwrap().project_plan = Some(1);
        assert!(store.prepare_auto_retry(id, 0).is_none());
    }

    #[test]
    fn changed_research_inputs_prevent_retry() {
        let mut store = Store::empty();
        store.projects[0].directory = "/tmp".into();
        store.default_model = "fixture/model".into();
        let goal = store
            .create_research_goal("Market research".into(), "Scope".into(), 1)
            .unwrap();
        let mut failed = store.prepare_run(goal).unwrap();
        failed.status = "Failed".into();
        let id = failed.id;
        store.runs.push(failed);
        assert!(store.prepare_auto_retry(id, 0).is_some());
        store
            .task_mut(goal)
            .unwrap()
            .description
            .push_str(" changed scope");
        assert!(store.prepare_auto_retry(id, 0).is_none());
    }

    #[test]
    fn legacy_runs_default_to_no_retry_metadata() {
        let (store, _) = failed_task();
        let mut value = serde_json::to_value(&store.runs[0]).unwrap();
        value.as_object_mut().unwrap().remove("auto_retry_attempt");
        value.as_object_mut().unwrap().remove("retry_not_before");
        let run: AgentRun = serde_json::from_value(value).unwrap();
        assert_eq!(run.auto_retry_attempt, 0);
        assert_eq!(run.retry_not_before, None);
    }
}
