//! Shared typed payload for durable dispatch intents.
//!
//! Serialization and deserialization happen on the persistence worker thread so
//! large prompts/bodies never block the UI path.

use crate::automation::AutomationOccurrence;
use crate::domain::AgentRun;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct AdmissionParts {
    pub operation_key: String,
    pub run: AgentRun,
    pub occurrence: Option<AutomationOccurrence>,
    pub next_id: Option<u64>,
    /// Reservation floor changes caused by dispatching a new run. Older intent
    /// files deserialize with the field absent and no floor update.
    #[serde(default)]
    pub run_id_floor: Option<u64>,
    /// Narrow request-to-run mapping for a planning dispatch. Older intent
    /// files deserialize with the field absent and no request link.
    #[serde(default)]
    pub planning_request: Option<PlanningRequestLink>,
    /// Full immutable request revision, inserted atomically only when it is not
    /// already durable. Existing (possibly newer) request rows are never
    /// replaced by a stale dispatch snapshot.
    #[serde(default)]
    pub request_snapshot: Option<crate::request_planning::ResearchRequest>,
}

/// Immutable revision-bound link between a planning request and its run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlanningRequestLink {
    pub request_id: u64,
    pub revision: u32,
    pub run_id: u64,
}

impl AdmissionParts {
    pub(crate) fn effects(&self) -> Vec<u64> {
        vec![
            self.run.id,
            self.occurrence.as_ref().map(|value| value.id).unwrap_or(0),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AgentRun, Stage};

    fn run() -> AgentRun {
        AgentRun {
            automation_occurrence_id: None,
            planning_owner: None,
            deadline_at: None,
            auto_retry_attempt: 0,
            retry_not_before: None,
            project_plan: None,
            id: 1,
            task_title: String::new(),
            directory: String::new(),
            prompt: String::new(),
            started_at: String::new(),
            queued_at: String::new(),
            workspace_source: None,
            ended_at: None,
            session_id: None,
            exit_code: None,
            task_id: 1,
            agent: "OpenCode".into(),
            model: "fixture/model".into(),
            stage: Stage::Planned,
            status: "Queued".into(),
            logs: Vec::new(),
            output: String::new(),
            attachments: Vec::new(),
            research_kind: None,
            sources: Vec::new(),
            input_signature: String::new(),
        }
    }

    #[test]
    fn older_intent_files_deserialize_without_new_fields() {
        let parts = AdmissionParts {
            operation_key: "dispatch:None:run:1".into(),
            run: run(),
            occurrence: None,
            next_id: None,
            run_id_floor: None,
            planning_request: None,
            request_snapshot: None,
        };
        let mut value = serde_json::to_value(&parts).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("planning_request");
        value
            .as_object_mut()
            .unwrap()
            .remove("request_snapshot");
        let old: AdmissionParts = serde_json::from_value(value).unwrap();
        assert!(old.planning_request.is_none());
        assert!(old.request_snapshot.is_none());
    }
}
