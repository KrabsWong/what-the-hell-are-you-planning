//! Deterministic presentation order for automation periods.
//!
//! "Latest activity" means the latest attempt activity of a period, not the largest
//! occurrence ID and not a run completion time. Lists and history share one key, so
//! this module owns both the chosen timestamp and its stable tie-breakers.
//!
//! Within one occurrence the key is the first present stored timestamp in this
//! precedence: retry request, actual start, manual request, scheduled trigger, then
//! the frozen reference time. A retry starts a new attempt against the same period,
//! so its request time is the activity even when a backward wall-clock change makes
//! it earlier than the period's original start. The reference time is required and
//! remains untouched; epoch zero and negative timestamps are ordinary values.
//! Completion does not create a new period, so run start/end strings are not read.
//!
//! Across occurrences, order compares the recorded activity timestamps rather than
//! IDs. A backward clock change therefore stays visible as recorded times instead of
//! silently reinterpreting a period. Equal timestamps use activity kind (retry, start,
//! request, scheduled, reference) and then occurrence ID descending.
use crate::automation::AutomationOccurrence;
use std::cmp::Ordering;

/// The stored event that supplied an occurrence's activity timestamp.
///
/// Declaration order is the tie-breaker priority for equal timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActivityKind {
    Reference,
    Scheduled,
    ManualRequest,
    ActualStart,
    RetryRequested,
}

impl ActivityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::RetryRequested => "Retried",
            Self::ActualStart => "Started",
            Self::ManualRequest => "Requested",
            Self::Scheduled => "Scheduled",
            Self::Reference => "Reference",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccurrenceActivity {
    /// Epoch seconds shown in the list, history and result dialog.
    pub utc: i64,
    pub kind: ActivityKind,
    pub occurrence_id: u64,
}

impl OccurrenceActivity {
    fn order_key(self) -> (i64, ActivityKind, u64) {
        (self.utc, self.kind, self.occurrence_id)
    }
}

/// Resolve the activity key for one period. Struct fields are frozen by earlier
/// workflow stages; this helper only reads them.
pub fn occurrence_activity(occurrence: &AutomationOccurrence) -> OccurrenceActivity {
    let (utc, kind) = if let Some(utc) = occurrence.retry_requested_at {
        (utc, ActivityKind::RetryRequested)
    } else if let Some(utc) = occurrence.actual_started_at {
        (utc, ActivityKind::ActualStart)
    } else if let Some(utc) = occurrence.requested_at {
        (utc, ActivityKind::ManualRequest)
    } else if let Some(utc) = occurrence.scheduled_for {
        (utc, ActivityKind::Scheduled)
    } else {
        // `reference_time` is required; epoch zero and negative values are real.
        (occurrence.reference_time, ActivityKind::Reference)
    };
    OccurrenceActivity {
        utc,
        kind,
        occurrence_id: occurrence.id,
    }
}

/// Descending activity order for one automation's history or the global results list.
pub fn compare_newest_first(a: &AutomationOccurrence, b: &AutomationOccurrence) -> Ordering {
    occurrence_activity(b)
        .order_key()
        .cmp(&occurrence_activity(a).order_key())
}

/// Newest period by the shared activity key, independent of storage order.
pub fn newest<'a, I>(occurrences: I) -> Option<&'a AutomationOccurrence>
where
    I: IntoIterator<Item = &'a AutomationOccurrence>,
{
    occurrences
        .into_iter()
        .max_by(|a, b| occurrence_activity(a).order_key().cmp(&occurrence_activity(b).order_key()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationTemplate,
        OccurrenceStatus, TimeWindow, TriggerSource,
    };

    fn occurrence(id: u64) -> AutomationOccurrence {
        AutomationOccurrence {
            id,
            automation_id: 1,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: format!("scheduled:1:{id}"),
            scheduled_for: Some(100),
            requested_at: None,
            actual_started_at: None,
            timezone: "UTC".into(),
            reference_time: 100,
            data_window: TimeWindow::unresolved("test"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Track".into(),
                expected_result: "Note".into(),
                shape: None,
                template: AutomationTemplate {
                    goals: vec![],
                    window_rule: Default::default(),
                },
                models: Default::default(),
                agents: Default::default(),
                materials: vec![],
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "test".into(),
            },
            status: OccurrenceStatus::Queued,
            reason: None,
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: 90,
            deadline_at: None,
            retry_requested_at: None,
        }
    }

    #[test]
    fn activity_fallbacks_are_explicit_and_ordered() {
        let mut value = occurrence(1);
        value.scheduled_for = Some(100);
        value.reference_time = 100;
        assert_eq!(occurrence_activity(&value).kind, ActivityKind::Scheduled);
        assert_eq!(occurrence_activity(&value).utc, 100);

        value.requested_at = Some(110);
        assert_eq!(occurrence_activity(&value).kind, ActivityKind::ManualRequest);
        assert_eq!(occurrence_activity(&value).utc, 110);

        value.actual_started_at = Some(120);
        assert_eq!(occurrence_activity(&value).kind, ActivityKind::ActualStart);

        value.retry_requested_at = Some(130);
        assert_eq!(occurrence_activity(&value).kind, ActivityKind::RetryRequested);
        assert_eq!(occurrence_activity(&value).utc, 130);

        let mut reference = occurrence(2);
        reference.scheduled_for = None;
        reference.reference_time = 0;
        reference.created_at = 80;
        assert_eq!(occurrence_activity(&reference).kind, ActivityKind::Reference);
        assert_eq!(
            occurrence_activity(&reference).utc,
            0,
            "Unix epoch zero is a real reference time, not a missing value"
        );

        reference.reference_time = -90;
        assert_eq!(occurrence_activity(&reference).kind, ActivityKind::Reference);
        assert_eq!(occurrence_activity(&reference).utc, -90);
    }

    #[test]
    fn older_id_retry_becomes_latest_everywhere() {
        let mut old = occurrence(1);
        old.actual_started_at = Some(100);
        let mut newer = occurrence(2);
        newer.actual_started_at = Some(200);

        assert_eq!(newest([&old, &newer]).unwrap().id, 2);
        old.retry_requested_at = Some(300);
        let latest = newest([&old, &newer]).unwrap();
        assert_eq!(latest.id, 1);
        assert_eq!(occurrence_activity(latest).kind, ActivityKind::RetryRequested);
        assert_eq!(occurrence_activity(latest).utc, 300);

        let mut ordered = vec![&newer, &old];
        ordered.sort_by(|a, b| compare_newest_first(a, b));
        assert_eq!(ordered.iter().map(|value| value.id).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn negative_and_epoch_timestamps_keep_numeric_order() {
        let mut older = occurrence(1);
        older.actual_started_at = Some(-200);
        let mut newer = occurrence(2);
        newer.actual_started_at = Some(-100);
        assert!(compare_newest_first(&newer, &older).is_lt());
        assert_eq!(newest([&older, &newer]).unwrap().id, 2);

        let mut retried = occurrence(3);
        retried.retry_requested_at = Some(-300);
        assert_eq!(
            newest([&older, &retried]).unwrap().id,
            1,
            "a stored retry time is compared numerically, not treated as missing"
        );
    }

    #[test]
    fn equal_times_use_activity_kind_then_id() {
        let mut retried = occurrence(1);
        retried.retry_requested_at = Some(500);
        let mut started = occurrence(9);
        started.actual_started_at = Some(500);
        assert_eq!(newest([&started, &retried]).unwrap().id, 1);

        let mut same_a = occurrence(3);
        same_a.actual_started_at = Some(500);
        let mut same_b = occurrence(4);
        same_b.actual_started_at = Some(500);
        assert_eq!(newest([&same_a, &same_b]).unwrap().id, 4);
    }

    #[test]
    fn clock_rollback_uses_recorded_times_without_mutating_periods() {
        // A retry recorded after the wall clock moved back keeps its retry key,
        // but another period that recorded a later time still sorts above it.
        let mut retried = occurrence(1);
        retried.actual_started_at = Some(200);
        retried.retry_requested_at = Some(100);
        let mut later = occurrence(2);
        later.actual_started_at = Some(150);
        let before = (retried.clone(), later.clone());

        let activity = occurrence_activity(&retried);
        assert_eq!(activity.kind, ActivityKind::RetryRequested);
        assert_eq!(activity.utc, 100, "stored retry time is shown as recorded");
        assert_eq!(newest([&retried, &later]).unwrap().id, 2);
        let mut ordered = vec![&later, &retried];
        ordered.sort_by(|a, b| compare_newest_first(a, b));
        assert_eq!(ordered.iter().map(|value| value.id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(before.0.retry_requested_at, retried.retry_requested_at);
        assert_eq!(before.1.actual_started_at, later.actual_started_at);
    }
}
