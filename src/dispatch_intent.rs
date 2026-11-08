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
}

impl AdmissionParts {
    pub(crate) fn effects(&self) -> Vec<u64> {
        vec![
            self.run.id,
            self.occurrence.as_ref().map(|value| value.id).unwrap_or(0),
        ]
    }
}
