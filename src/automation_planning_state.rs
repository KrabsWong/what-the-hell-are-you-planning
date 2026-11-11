//! Derived planning state for a saved automation draft.
//!
//! PRODUCT requires that leaving the creation page while a plan is still
//! generating keeps a visible **Planning…** state on the saved draft and its
//! detail page, and that the state remains discoverable after the wizard is
//! closed or the app is reopened. The wizard's in-memory token is a live hint,
//! not the source of truth: this pure helper derives the state from the durable
//! planning request and its immutable attempts.
//!
//! It reads T23/T25-owned data and never mutates it.

use crate::domain::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomationPlanningState {
    /// The request is being summarized before goal planning is permitted.
    Understanding,
    /// A generated understanding awaits explicit confirmation or clarification.
    UnderstandingReady,
    /// At least one attempt for the latest request is queued, running or
    /// cancelling.
    Planning,
    /// The latest attempt succeeded and produced a template that is not yet
    /// approved or enabled.
    PlanReady,
    /// A request exists but its latest attempt failed, went stale, or never
    /// started. The user must review or retry it.
    RetryNeeded,
    /// No planning request exists, or the current version is already approved.
    None,
}

impl AutomationPlanningState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Understanding => "Understanding research…",
            Self::UnderstandingReady => "Understanding ready to review",
            Self::Planning => "Planning…",
            Self::PlanReady => "Plan ready to review",
            Self::RetryNeeded => "Planning needs attention",
            Self::None => "",
        }
    }

    pub(crate) fn is_actionable(self) -> bool {
        matches!(self, Self::UnderstandingReady | Self::PlanReady | Self::RetryNeeded)
    }
}

fn attempt_is_active(status: &str) -> bool {
    matches!(status, "Queued" | "Running" | "Cancelling" | "Recovering")
}

/// Derive the planning state for one saved automation definition.
///
/// Rules (aligned with T23/T25 ownership):
/// - Only the **latest attempt of the latest request revision** is authoritative;
///   an older still-active attempt cannot override a newer terminal one.
/// - An active authoritative attempt is Planning, even when the previous
///   version is approved.
/// - A terminal attempt is not "ready" merely because a template exists. It must
///   be the current attempt for its immutable request, and the stored template
///   must carry a valid binding for that exact request/revision/signature/owner.
/// - `approval_current()` alone never hides a failed or unapplied newer request.
pub(crate) fn automation_planning_state(
    store: &Store,
    automation: u64,
) -> AutomationPlanningState {
    let Some(definition) = store.automation(automation) else {
        return AutomationPlanningState::None;
    };
    if definition.shape != crate::automation::AutomationShape::Research {
        return AutomationPlanningState::None;
    }
    let Some(request) = store.latest_planning_request(automation) else {
        // A binding that points at missing request history still needs review;
        // a legacy unbound template keeps its existing behavior.
        return if definition.plan_binding.is_some() {
            AutomationPlanningState::RetryNeeded
        } else {
            AutomationPlanningState::None
        };
    };
    if store.request_needs_understanding(request.id) {
        if store.request_understanding_ready(request.id) {
            return AutomationPlanningState::UnderstandingReady;
        }
        return if store.latest_request_attempt(request.id).is_some_and(|run| {
            crate::request_planning::is_understanding_run(run) && attempt_is_active(&run.status)
        }) {
            AutomationPlanningState::Understanding
        } else {
            AutomationPlanningState::RetryNeeded
        };
    }
    let Some(authoritative) = store.latest_request_attempt(request.id) else {
        return AutomationPlanningState::RetryNeeded;
    };
    if attempt_is_active(&authoritative.status) {
        return AutomationPlanningState::Planning;
    }
    if definition.approval_current() && store.plan_binding_problem(definition).is_none() {
        return AutomationPlanningState::None;
    }
    if authoritative.status != "Succeeded" {
        return AutomationPlanningState::RetryNeeded;
    }
    if !store.planning_attempt_is_current(authoritative.id) {
        return AutomationPlanningState::RetryNeeded;
    }
    if definition.template.is_none() || store.plan_binding_problem(definition).is_some() {
        return AutomationPlanningState::RetryNeeded;
    }
    AutomationPlanningState::PlanReady
}

/// A saved draft or its detail page shows a planning affordance when this is
/// true, regardless of whether the wizard is open.
pub(crate) fn planning_needs_attention(store: &Store, automation: u64) -> bool {
    automation_planning_state(store, automation) != AutomationPlanningState::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationApproval, AutomationPolicy, AutomationShape, AutomationTemplate,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};

    const PLAN: &str = r#"{"topic_name":"Filings","brief":{"objective":"Track public filings","deliverable":"A cited note","scope":"Public filings","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Filings review","brief":"Track filings","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze filings","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report filings","kind":"advise","depends_on":[1]}]}]}"#;

    fn store() -> Store {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        store.research_config_mut().models =
            ["test/plan", "test/collect", "test/analyze", "test/advise"].map(String::from);
        store
    }

    fn template() -> AutomationTemplate {
        crate::automation_execution::build_template("Track public filings", "A cited note").unwrap()
    }

    fn automation(store: &mut Store) -> u64 {
        store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Filings".into(),
                "Track public filings".into(),
                "A cited note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily {
                        hour: 8,
                        minute: 30,
                    },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                None,
                1_000,
            )
            .unwrap()
    }

    fn confirm_understanding(store: &mut Store, request: u64) {
        store.apply_request_understanding(request,
            r#"{"summary":"Track public filings and deliver a concise sourced note.","questions":[]}"#,
        ).unwrap();
        store.confirm_request_understanding(request).unwrap();
    }

    fn request(store: &mut Store, automation: u64) -> u64 {
        let id = store.create_automation_request(
            automation, 1, "Track public filings", "A cited note",
        ).unwrap();
        confirm_understanding(store, id);
        id
    }

    fn attempt(store: &mut Store, request: u64) -> u64 {
        let run = store.prepare_request_plan(request).unwrap();
        let id = run.id;
        store.push_run(run);
        store.request_mut(request).unwrap()
            .planning_run_id = Some(id);
        id
    }

    fn request_with(store: &mut Store, automation: u64, input: &str) -> u64 {
        store.automation_mut(automation).unwrap().objective = input.into();
        let id = store.create_automation_request(automation, 1, input, "A cited note").unwrap();
        confirm_understanding(store, id);
        id
    }

    /// Simulate the reviewed apply step so the definition carries a valid plan
    /// binding for this exact request revision and signature.
    fn apply_plan(store: &mut Store, automation: u64, request: u64) -> u64 {
        let mut run = store.prepare_request_plan(request).unwrap();
        run.output = PLAN.into();
        run.status = "Succeeded".into();
        let signature = run.input_signature.clone();
        let id = run.id;
        store.push_run(run);
        let revision = store
            .research_requests()
            .iter()
            .find(|value| value.id == request)
            .unwrap()
            .revision;
        store
            .apply_automation_template(automation, request, revision, &signature, PLAN, 2_000)
            .unwrap();
        store.request_mut(request).unwrap()
            .planning_run_id = Some(id);
        id
    }

    fn approve_current(store: &mut Store, automation: u64, request: u64) {
        let revision = store
            .research_requests()
            .iter()
            .find(|value| value.id == request)
            .unwrap()
            .revision;
        {
            let mut definition = store.automation_mut(automation).unwrap();
            let template = definition.template.clone().unwrap();
        definition.approval = Some(AutomationApproval {
            version: definition.version,
            approved_at: 2_000,
            template_signature: template.signature(),
            schedule_signature: definition.schedule.signature(),
            plan_request_id: Some(request),
            plan_revision: Some(revision),
            models: ["test/plan", "test/collect", "test/analyze", "test/advise"]
                .map(String::from),
            agents: Default::default(),
            materials: vec![],
            });
        }
        assert!(store.automation(automation).unwrap().approval_current());
    }

    fn set_status(store: &mut Store, run: u64, status: &str) {
        store.run_mut(run).unwrap().status = status.into();
    }

    #[test]
    fn understanding_is_reviewable_before_any_goal_plan_exists() {
        let mut store = store();
        let id = automation(&mut store);
        let request = store.create_automation_request(
            id, 1, "Track public filings", "A cited note",
        ).unwrap();
        let run = attempt(&mut store, request);
        assert_eq!(automation_planning_state(&store, id), AutomationPlanningState::Understanding);
        set_status(&mut store, run, "Succeeded");
        store.apply_request_understanding(request,
            r#"{"summary":"Track filings and explain material changes with sources.","questions":[]}"#,
        ).unwrap();
        assert_eq!(automation_planning_state(&store, id), AutomationPlanningState::UnderstandingReady);
        assert!(AutomationPlanningState::UnderstandingReady.is_actionable());
        assert!(store.automation(id).unwrap().template.is_none());
        assert!(store.prepare_request_plan(request).is_err());
        store.confirm_request_understanding(request).unwrap();
        let plan = store.prepare_request_plan(request).unwrap();
        assert!(!crate::request_planning::is_understanding_run(&plan));
    }

    #[test]
    fn changed_request_requires_a_new_understanding() {
        let mut store = store();
        let id = automation(&mut store);
        let original = request(&mut store, id);
        apply_plan(&mut store, id, original);
        approve_current(&mut store, id, original);
        let revised = store.create_automation_request(
            id, 1, "Compare filing risks", "A comparison",
        ).unwrap();
        assert!(store.request_needs_understanding(revised));
        assert_eq!(automation_planning_state(&store, id), AutomationPlanningState::RetryNeeded);
        let run = attempt(&mut store, revised);
        assert!(crate::request_planning::is_understanding_run(store.runs().iter().find(|r| r.id == run).unwrap()));
        assert_eq!(automation_planning_state(&store, id), AutomationPlanningState::Understanding);
    }

    #[test]
    fn no_automation_or_no_request_has_no_planning_state() {
        let store = store();
        assert_eq!(
            automation_planning_state(&store, 404),
            AutomationPlanningState::None
        );

        let mut store = store;
        let id = automation(&mut store);
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::None
        );
    }

    #[test]
    fn an_active_attempt_is_planning_even_before_the_wizard_reopens() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let run = attempt(&mut store, request);
        set_status(&mut store, run, "Queued");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::Planning
        );
        set_status(&mut store, run, "Running");
        assert!(planning_needs_attention(&store, id));
        set_status(&mut store, run, "Cancelling");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::Planning
        );
    }

    #[test]
    fn a_finished_attempt_needs_review_or_retry() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let run = attempt(&mut store, request);
        set_status(&mut store, run, "Failed");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );
        assert!(AutomationPlanningState::RetryNeeded.is_actionable());
    }

    #[test]
    fn a_saved_request_without_an_attempt_needs_attention() {
        let mut store = store();
        let id = automation(&mut store);
        let _ = request(&mut store, id);
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );
    }

    #[test]
    fn success_with_a_current_binding_is_ready_to_review() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let run = apply_plan(&mut store, id, request);
        assert!(store.planning_attempt_is_current(run));
        assert!(store.plan_binding_problem(store.automation(id).unwrap()).is_none());
        assert!(!store.automation(id).unwrap().approval_current());
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::PlanReady
        );
        assert_eq!(
            AutomationPlanningState::PlanReady.label(),
            "Plan ready to review"
        );
    }

    #[test]
    fn success_without_a_template_needs_attention() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let run = attempt(&mut store, request);
        set_status(&mut store, run, "Succeeded");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );
    }

    #[test]
    fn an_old_template_with_a_new_failed_request_needs_attention() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        apply_plan(&mut store, id, request);
        approve_current(&mut store, id, request);
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::None
        );

        let newer = request_with(&mut store, id, "A completely changed objective");
        let failed = attempt(&mut store, newer);
        set_status(&mut store, failed, "Failed");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded,
            "an old approval must not hide a failed newer request"
        );
    }

    #[test]
    fn successful_but_unapplied_output_is_not_plan_ready() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        apply_plan(&mut store, id, request);

        // A newer revision succeeded but its output was never applied to the
        // definition, so the template still belongs to the old request.
        let newer = request_with(&mut store, id, "A completely changed objective");
        let unapplied = attempt(&mut store, newer);
        set_status(&mut store, unapplied, "Succeeded");
        assert!(store.plan_binding_problem(store.automation(id).unwrap()).is_some());
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );
    }

    #[test]
    fn an_unapplied_success_without_a_binding_is_not_plan_ready() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let run = attempt(&mut store, request);
        set_status(&mut store, run, "Succeeded");
        store.automation_mut(id).unwrap().template = Some(template());
        assert!(store.plan_binding_problem(store.automation(id).unwrap()).is_some());
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );
    }

    #[test]
    fn an_approved_current_binding_reports_no_planning_state() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        apply_plan(&mut store, id, request);
        approve_current(&mut store, id, request);
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::None
        );
    }

    #[test]
    fn an_older_active_attempt_does_not_override_a_newer_terminal_attempt() {
        let mut store = store();
        let id = automation(&mut store);
        let request = request(&mut store, id);
        let older = attempt(&mut store, request);
        set_status(&mut store, older, "Failed");
        let newer = attempt(&mut store, request);
        set_status(&mut store, newer, "Failed");
        // Simulate a persisted anomalous older attempt that still claims to be
        // running. The newer terminal attempt is authoritative.
        set_status(&mut store, older, "Running");
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded
        );

        set_status(&mut store, newer, "Succeeded");
        store.automation_mut(id).unwrap().template = Some(template());
        assert_eq!(
            automation_planning_state(&store, id),
            AutomationPlanningState::RetryNeeded,
            "a still-active older attempt must not make an unbound success ready"
        );
    }

    #[test]
    fn labels_are_stable_and_english() {
        assert_eq!(AutomationPlanningState::Planning.label(), "Planning…");
        assert_eq!(
            AutomationPlanningState::RetryNeeded.label(),
            "Planning needs attention"
        );
        assert_eq!(AutomationPlanningState::None.label(), "");
        assert!(!AutomationPlanningState::Planning.is_actionable());
        assert!(AutomationPlanningState::PlanReady.is_actionable());
    }
}
