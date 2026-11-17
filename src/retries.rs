//! Automatic retries reuse the normal queue, resource locks and approval checks.
use crate::domain::{AgentRun, Stage, Store};

pub const MAX_AUTO_RETRIES: u8 = 3;
pub const RETRY_DELAY_SECONDS: i64 = 30;
pub const FALLBACK_AFTER_FAILED_ATTEMPTS: u8 = 2;

impl AgentRun {
    pub fn retry_delay_remaining(&self, now: i64) -> i64 {
        self.retry_not_before
            .map_or(0, |deadline| (deadline - now).max(0))
    }
}

impl Store {
    /// Planning attempts are manually retried, so reuse the fallback when the
    /// previous attempt failed because the primary model was unavailable.
    pub(crate) fn planning_route_after_failure(
        &self,
        failed: Option<&AgentRun>,
        agent: &str,
        model: &str,
    ) -> (String, String) {
        let use_fallback = failed
            .is_some_and(|run| {
                run.status == "Failed"
                    && run.logs.iter().any(|line| crate::run_errors::model_unavailable(line))
            })
            && self.valid_fallback_model();
        if use_fallback {
            (
                self.fallback_agent().to_owned(),
                self.fallback_model().to_owned(),
            )
        } else {
            (
                agent.to_owned(),
                model.to_owned(),
            )
        }
    }

    pub fn prepare_auto_retry(&self, failed_id: u64, now: i64) -> Option<AgentRun> {
        let failed = self.runs().iter().find(|run| run.id == failed_id)?;
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
                .runs()
                .iter()
                .rev()
                .find(|run| run.task_id == failed.task_id && run.project_plan.is_none())?
                .id
                != failed_id
        {
            return None;
        }
        // Retry the approved snapshot only. Dispatch checks it again after the delay.
        let historical_report_regeneration =
            crate::research::is_historical_report_regeneration(failed);
        if failed.research_kind.is_none()
            || (!historical_report_regeneration
                && self.research_signature(task.id).as_ref() != Some(&failed.input_signature))
        {
            return None;
        }
        let mut retry = failed.clone();
        retry.id = self.runs().iter().map(|run| run.id).max()?.checked_add(1)?;
        retry.auto_retry_attempt += 1;
        retry.retry_not_before = Some(now + RETRY_DELAY_SECONDS);
        retry.status = "Queued".into();
        retry.queued_at = crate::opencode::timestamp();
        retry.started_at.clear();
        retry.ended_at = None;
        retry.exit_code = None;
        if let Some(error) = failed.logs.iter().rev().find_map(|line| line.strip_prefix("[error] [validation] ")) {
            retry.prompt.push_str(&format!("\nAPPLICATION VALIDATION FEEDBACK (attempt {}):\n{}\nCorrect the report using the existing evidence and return the required JSON. Do not repeat the rejected format or invent sources.\n", retry.auto_retry_attempt, error));
        }
        retry.output.clear();
        retry.sources.clear();
        retry.logs = vec![format!(
            "[retry] Attempt {}/{} scheduled after {} seconds because run {} failed. Stop task cancels this retry.",
            retry.auto_retry_attempt, MAX_AUTO_RETRIES, RETRY_DELAY_SECONDS, failed.id
        )];
        retry.logs.extend(
            failed
                .logs
                .iter()
                .filter(|line| line.starts_with("[research] Regenerating report from run #"))
                .cloned(),
        );
        let model_unavailable = failed
            .logs
            .iter()
            .any(|line| crate::run_errors::model_unavailable(line));
        if model_unavailable && retry.auto_retry_attempt >= FALLBACK_AFTER_FAILED_ATTEMPTS {
            if self.valid_fallback_model()
                && (retry.agent != self.fallback_agent() || retry.model != self.fallback_model())
            {
                retry.logs.push(format!(
                    "[fallback] Primary model still unavailable after {} failed attempts. Retrying with {} · {} in a fresh session.",
                    FALLBACK_AFTER_FAILED_ATTEMPTS,
                    self.fallback_agent(),
                    self.fallback_model()
                ));
                retry.agent = self.fallback_agent().to_owned();
                retry.model = self.fallback_model().to_owned();
                retry.session_id = None;
            } else if !self.valid_fallback_model() {
                retry.logs.push(
                    "[fallback] No valid fallback model is configured; retrying the primary model.".into(),
                );
            }
        }
        // Preserve the original evidence and session, with actionable validation feedback.
        Some(retry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_task() -> (Store, u64) {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        let goal = store.create_research_goal("Goal".into(), "Scope".into(), 1).unwrap();
        store.push_run(store.prepare_run(goal).unwrap());
        let id = {
            let mut run = store.last_run_mut().unwrap();
            run.status = "Failed".into();
            run.session_id = Some("existing-session".into());
            run.output = "Partial output".into();
            run.exit_code = Some(1);
            run.id
        };
        (store, id)
    }

    #[test]
    fn validation_retry_explains_rejection_and_preserves_context() {
        let (mut store, id) = failed_task();
        store.last_run_mut().unwrap().logs.push("[error] [validation] Source 2 has an unsupported URL.".into());
        let original = store.runs().last().unwrap().clone();
        let retry = store.prepare_auto_retry(id, 100).unwrap();
        assert!(retry.prompt.starts_with(&original.prompt));
        assert!(retry.prompt.contains("Source 2 has an unsupported URL."));
        assert_eq!(retry.session_id, original.session_id);
        assert_eq!(retry.agent, original.agent);
        assert_eq!(retry.model, original.model);
        assert_eq!(retry.attachments, original.attachments);
        assert_eq!(crate::run_errors::user_message("[error] [validation] Invalid source"), "Invalid source");
    }

    #[test]
    fn three_delayed_retries_preserve_snapshot_and_session() {
        let (mut store, mut id) = failed_task();
        let original = store.runs().last().unwrap().clone();
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
            store.push_run(retry);
            // Restart/serialization must not erase the retry budget.
            store = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        }
        assert!(store.prepare_auto_retry(id, 100).is_none());
        assert!(store.prepare_auto_retry(original.id, 100).is_none());
    }

    #[test]
    fn availability_failure_switches_to_fallback_after_two_attempts() {
        let (mut store, mut id) = failed_task();
        store.research_config_mut().fallback_agent = "Codex".into();
        store.research_config_mut().fallback_model = "fallback-model".into();
        store.last_run_mut().unwrap().logs.push(
            r#"[error] {"type":"provider.free-limit","message":"Free usage limit reached","status":429}"#.into(),
        );
        let original = store.runs().last().unwrap().clone();

        let first = store.prepare_auto_retry(id, 100).unwrap();
        assert_eq!(first.auto_retry_attempt, 1);
        assert_eq!(first.agent, original.agent);
        assert_eq!(first.model, original.model);
        assert_eq!(first.session_id, original.session_id);
        let mut first_failed = first.clone();
        first_failed.status = "Failed".into();
        first_failed.logs.push(
            r#"[error] {"type":"provider.free-limit","message":"Free usage limit reached","status":429}"#.into(),
        );
        id = first_failed.id;
        store.push_run(first_failed);

        let second = store.prepare_auto_retry(id, 200).unwrap();
        assert_eq!(second.auto_retry_attempt, 2);
        assert_eq!(second.agent, "Codex");
        assert_eq!(second.model, "fallback-model");
        assert!(second.session_id.is_none());
        assert_eq!(second.input_signature, original.input_signature);
        assert!(second.logs.iter().any(|line| line.contains("fresh session")));
        let mut third = second.clone();
        third.status = "Failed".into();
        id = third.id;
        store.push_run(third);

        let continued = store.prepare_auto_retry(id, 300).unwrap();
        assert_eq!(continued.agent, "Codex");
        assert_eq!(continued.model, "fallback-model");
    }

    #[test]
    fn validation_failure_never_switches_to_fallback() {
        let (mut store, id) = failed_task();
        store.research_config_mut().fallback_agent = "Codex".into();
        store.research_config_mut().fallback_model = "fallback-model".into();
        store
            .last_run_mut()
            .unwrap()
            .logs
            .push("[error] [validation] Invalid source".into());
        let original = store.runs().last().unwrap().clone();
        let first = store.prepare_auto_retry(id, 100).unwrap();
        let mut first_failed = first.clone();
        first_failed.status = "Failed".into();
        store.push_run(first_failed);
        let second = store.prepare_auto_retry(first.id, 200).unwrap();
        assert_eq!(second.agent, original.agent);
        assert_eq!(second.model, original.model);
        assert_eq!(second.session_id, original.session_id);
        assert!(!second.logs.iter().any(|line| line.starts_with("[fallback]")));
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
            store.last_run_mut().unwrap().status = status.into();
            assert!(store.prepare_auto_retry(id, 0).is_none(), "{status}");
        }
        let (mut store, id) = failed_task();
        store.task_mut(1).unwrap().archived = true;
        assert!(store.prepare_auto_retry(id, 0).is_none());
        store.task_mut(1).unwrap().archived = false;
        store.task_mut(1).unwrap().stage = Stage::Cancelled;
        assert!(store.prepare_auto_retry(id, 0).is_none());
        let (mut store, id) = failed_task();
        store.last_run_mut().unwrap().project_plan = Some(1);
        assert!(store.prepare_auto_retry(id, 0).is_none());
    }

    #[test]
    fn changed_research_inputs_prevent_retry() {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = "/tmp".into();
        store.set_default_model("fixture/model".into());
        let goal = store
            .create_research_goal("Market research".into(), "Scope".into(), 1)
            .unwrap();
        let mut failed = store.prepare_run(goal).unwrap();
        failed.status = "Failed".into();
        let id = failed.id;
        store.push_run(failed);
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
        let mut value = serde_json::to_value(&store.runs()[0]).unwrap();
        value.as_object_mut().unwrap().remove("auto_retry_attempt");
        value.as_object_mut().unwrap().remove("retry_not_before");
        let run: AgentRun = serde_json::from_value(value).unwrap();
        assert_eq!(run.auto_retry_attempt, 0);
        assert_eq!(run.retry_not_before, None);
    }
}
