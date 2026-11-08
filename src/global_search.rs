//! T08: one global search surface shared by the footer command and ⌘K.
//!
//! The overlay owns its query, domain filter and selection. It deliberately does
//! not rewrite the current route, selected task, list filters or an open draft.
//! Choosing a result is the only navigation action and resolves to the owning
//! Research or Automations destination.
//!
//! Search is synchronous over the in-memory [`Store`]. It never reads report
//! bodies or run logs: period hits use period metadata and whether a final run
//! exists. Automation-owned tasks are not ordinary Research results.

use crate::app::TaskApp;
use crate::appearance::palette;
use crate::controls::{select_button, tool_icon};
use crate::domain::{Store, Task};
use crate::research::ResearchKind;
use crate::views::{col, label, muted, observed, row, selectable};
use gpui_kit::base::FocusTrapElement;
use gpui_kit::component::Icon;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, FocusHandle, FontWeight, KeyDownEvent, Role, ScrollHandle, Window, assets,
    div, px, relative, rgb, rgba,
};

pub(crate) const MAX_RESULTS: usize = 50;
const PANEL_WIDTH: f32 = 720.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchDomain {
    All,
    Research,
    Automations,
}

impl SearchDomain {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Research => "Research",
            Self::Automations => "Automations",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Research => "research",
            Self::Automations => "automations",
        }
    }

    pub(crate) fn matches(self, kind: SearchTargetKind) -> bool {
        match self {
            Self::All => true,
            Self::Research => kind == SearchTargetKind::Research,
            Self::Automations => {
                matches!(kind, SearchTargetKind::Automation | SearchTargetKind::Period)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SearchTargetKind {
    Research,
    Automation,
    Period,
}

impl SearchTargetKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Research => "Research",
            Self::Automation => "Automation",
            Self::Period => "Period",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Automation => "automation",
            Self::Period => "period",
        }
    }

    pub(crate) fn icon(self) -> assets::IconName {
        match self {
            Self::Research => assets::IconName::FileText,
            Self::Automation => assets::IconName::CalendarClock,
            Self::Period => assets::IconName::Clock,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchTarget {
    Research(u64),
    Automation(u64),
    Period(u64),
}

impl SearchTarget {
    pub(crate) fn id(self) -> u64 {
        match self {
            Self::Research(id) | Self::Automation(id) | Self::Period(id) => id,
        }
    }

}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchHit {
    pub target: SearchTarget,
    pub kind: SearchTargetKind,
    pub title: String,
    /// Kind, status and time context shown on the second line.
    pub detail: String,
    /// Where activating this result goes.
    pub destination: String,
}

#[derive(Default)]
pub(crate) struct GlobalSearchUi {
    pub open: bool,
    pub domain: SearchDomain,
    pub selected: usize,
    pub hits: Vec<SearchHit>,
    pub message: Option<String>,
    pub scroll: ScrollHandle,
    /// Focus to restore when the overlay closes. Captured before the overlay
    /// takes keyboard focus, so closing returns to the exact opener.
    pub previous_focus: Option<FocusHandle>,
}

impl Default for SearchDomain {
    fn default() -> Self {
        Self::All
    }
}

impl GlobalSearchUi {
    pub(crate) fn refresh(&mut self, store: &Store, query: &str) {
        self.hits = search(store, self.domain, query, MAX_RESULTS);
        if self.selected >= self.hits.len() {
            self.selected = 0;
        }
        if !self.hits.is_empty() {
            self.scroll.scroll_to_item(self.selected);
        }
    }
}

struct Candidate {
    target: SearchTarget,
    kind: SearchTargetKind,
    title: String,
    detail: String,
    destination: String,
    /// Text and weight. Weight 0 is the title, 1 is context, 2 is secondary
    /// metadata. Every whitespace token must match at least one field.
    fields: Vec<(String, u8)>,
}

/// Plain-text token-AND search across the requested domain. An empty query has
/// no results so the overlay can show an explicit prompt instead of dumping the
/// whole workspace.
pub(crate) fn search(
    store: &Store,
    domain: SearchDomain,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let mut scored = Vec::new();
    if domain.matches(SearchTargetKind::Research) {
        for task in store.tasks() {
            if task.automation_id.is_some()
                || store.task_is_archived(task.id)
                || task_belongs_to_automation_project(store, task)
            {
                continue;
            }
            let candidate = research_candidate(store, task);
            if let Some(score) = score(&candidate, &terms, query) {
                scored.push((score, candidate));
            }
        }
    }
    if domain.matches(SearchTargetKind::Automation) {
        for definition in store.automations() {
            let candidate = automation_candidate(definition);
            if let Some(score) = score(&candidate, &terms, query) {
                scored.push((score, candidate));
            }
        }
    }
    if domain.matches(SearchTargetKind::Period) {
        for occurrence in store.all_occurrences() {
            let candidate = period_candidate(store, occurrence);
            if let Some(score) = score(&candidate, &terms, query) {
                scored.push((score, candidate));
            }
        }
    }

    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| {
                left.title
                    .to_lowercase()
                    .cmp(&right.title.to_lowercase())
            })
            .then_with(|| left.target.id().cmp(&right.target.id()))
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, candidate)| SearchHit {
            target: candidate.target,
            kind: candidate.kind,
            title: candidate.title,
            detail: candidate.detail,
            destination: candidate.destination,
        })
        .collect()
}

fn research_candidate(store: &Store, task: &Task) -> Candidate {
    let project = store
        .projects()
        .iter()
        .find(|project| project.id == task.project_id)
        .map(|project| project.name.clone())
        .unwrap_or_else(|| "Unknown project".into());
    let topic = store
        .task_topic(task.id)
        .map(|topic| topic.display_name());
    let kind = research_kind_label(task);
    let status = store.task_status(task.id, None).label();
    let detail = match &topic {
        Some(topic) => format!("{kind} · {project} · {topic} · {status}"),
        None => format!("{kind} · {project} · {status}"),
    };
    let destination = match &topic {
        Some(topic) => format!("Open in {project} · {topic}"),
        None => format!("Open in {project}"),
    };
    let identifier = store.identifier(task);
    let mut fields = vec![(task.title.clone(), 0), (identifier, 1), (project, 1)];
    if let Some(topic) = &topic {
        fields.push((topic.clone(), 1));
    }
    fields.push((kind.to_string(), 2));
    fields.push((status, 2));
    if let Some(research) = &task.research {
        fields.push((research.space_name.clone(), 2));
    }
    Candidate {
        target: SearchTarget::Research(task.id),
        kind: SearchTargetKind::Research,
        title: task.title.clone(),
        detail,
        destination,
        fields,
    }
}

fn automation_candidate(definition: &crate::automation::AutomationDefinition) -> Candidate {
    let shape = definition.shape.label();
    let status = definition.status.label();
    let schedule = crate::automation_views::schedule_summary(&definition.schedule);
    let detail = format!("{shape} · {status} · {schedule}");
    Candidate {
        target: SearchTarget::Automation(definition.id),
        kind: SearchTargetKind::Automation,
        title: definition.name.clone(),
        detail,
        destination: "Open automation".into(),
        fields: vec![
            (definition.name.clone(), 0),
            (definition.objective.clone(), 1),
            (definition.expected_result.clone(), 1),
            (format!("automation {}", definition.id), 1),
            (shape.to_string(), 2),
            (status.to_string(), 2),
            (schedule, 2),
        ],
    }
}

fn period_candidate(
    store: &Store,
    occurrence: &crate::automation::AutomationOccurrence,
) -> Candidate {
    let definition = store.automation(occurrence.automation_id);
    let automation_name = definition
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| format!("Automation {}", occurrence.automation_id));
    let status = occurrence.status.label();
    let source = occurrence.source.label();
    let reference =
        crate::automation_views::format_in_zone(occurrence.reference_time, &occurrence.timezone);
    // A linked run is a saved-result reference, not proof of a validated final
    // report. T06 owns typed result quality; searching must not claim more than
    // the stored link proves.
    let saved_result = if occurrence.final_run_id.is_some() {
        "Saved result reference"
    } else {
        "No saved result reference"
    };
    let detail = format!("{status} · {source} · {reference}");
    let title = format!("{automation_name} · Period {}", occurrence.id);
    let mut fields = vec![
        (automation_name.clone(), 0),
        (title.clone(), 0),
        (format!("period {}", occurrence.id), 1),
        (format!("period {}", occurrence.id), 1),
        (status.to_string(), 1),
        (source.to_string(), 1),
        (reference, 1),
        (occurrence.snapshot.objective.clone(), 2),
        (occurrence.snapshot.expected_result.clone(), 2),
        (saved_result.to_string(), 2),
    ];
    if let Some(definition) = definition {
        fields.push((definition.objective.clone(), 2));
        fields.push((definition.expected_result.clone(), 2));
    }
    Candidate {
        target: SearchTarget::Period(occurrence.id),
        kind: SearchTargetKind::Period,
        title,
        detail,
        destination: format!("Open period in {automation_name}"),
        fields,
    }
}

fn task_belongs_to_automation_project(store: &Store, task: &Task) -> bool {
    store
        .projects()
        .iter()
        .find(|project| project.id == task.project_id)
        .is_some_and(|project| project.kind == crate::domain::ProjectKind::Automation)
}

fn research_kind_label(task: &Task) -> &'static str {
    match task.research.as_ref().map(|research| research.kind) {
        Some(ResearchKind::Goal) => "Research goal",
        Some(ResearchKind::Collect) => "Evidence step",
        Some(ResearchKind::Analyze) => "Analysis step",
        Some(ResearchKind::Advise) => "Report step",
        None => "Task",
    }
}

fn score(candidate: &Candidate, terms: &[String], query: &str) -> Option<i32> {
    let mut total = 0;
    for term in terms {
        let mut best = 0;
        for (text, weight) in &candidate.fields {
            let lower = text.to_lowercase();
            if let Some(position) = lower.find(term) {
                let value = match (*weight, position == 0) {
                    (0, true) => 300,
                    (0, false) => 200,
                    (1, _) => 120,
                    _ => 60,
                };
                best = best.max(value);
            }
        }
        if best == 0 {
            return None;
        }
        total += best;
    }
    let title = candidate.title.to_lowercase();
    let query = query.to_lowercase();
    if title == query {
        total += 1_000;
    } else if title.starts_with(&query) {
        total += 400;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationDefinition, AutomationOccurrence, AutomationOccurrenceSnapshot, AutomationPolicy,
        AutomationShape, AutomationTemplate, DefinitionStatus, OccurrenceStatus, TimeWindow,
        TriggerSource,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::Stage;

    fn automation_definition(store: &mut Store, name: &str, objective: &str) -> u64 {
        let id = store.automations().iter().map(|value| value.id).max().unwrap_or(0) + 1;
        store.push_automation(AutomationDefinition {
            id,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Research,
            name: name.into(),
            objective: objective.into(),
            expected_result: "A cited brief".into(),
            schedule: Schedule {
                spec: ScheduleSpec::Daily { hour: 9, minute: 15 },
                timezone: "UTC".into(),
            },
            status: DefinitionStatus::Enabled,
            template: Some(AutomationTemplate {
                goals: vec![],
                window_rule: Default::default(),
            }),
            plan_binding: None,
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
        });
        id
    }

    fn occurrence(store: &mut Store, automation: u64, id: u64, final_run: bool) -> u64 {
        store.push_occurrence(AutomationOccurrence {
            id,
            automation_id: automation,
            definition_version: 1,
            source: TriggerSource::Scheduled,
            trigger_key: format!("scheduled:{id}"),
            scheduled_for: Some(1_000),
            requested_at: None,
            actual_started_at: Some(1_001),
            timezone: "UTC".into(),
            reference_time: 1_000,
            data_window: TimeWindow::unresolved("QA fixture"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Track the public fuel price".into(),
                expected_result: "A short cited note".into(),
                shape: Some(AutomationShape::Research),
                template: AutomationTemplate {
                    goals: vec![],
                    window_rule: Default::default(),
                },
                models: Default::default(),
                agents: Default::default(),
                materials: vec![],
                policy: AutomationPolicy::default(),
                approval_version: 1,
                approval_signature: "qa".into(),
            },
            status: OccurrenceStatus::Succeeded,
            reason: None,
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: final_run.then_some(900 + id),
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: 999,
            deadline_at: None,
            retry_requested_at: None,
        });
        id
    }

    fn fixture() -> (Store, u64, u64, u64, u64) {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().name = "Semiconductor desk".into();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into();
        store.set_default_model("qa/model".into());
        store.research_config_mut().models =
            ["qa/plan", "qa/collect", "qa/analyze", "qa/advise"].map(String::from);
        let goal = store
            .create_research_goal("Semiconductor supply outlook".into(), "Compare supply".into(), 1)
            .unwrap();
        let automation = automation_definition(&mut store, "Fuel price watch", "Track fuel");
        let period = occurrence(&mut store, automation, 41, true);
        let owned = store
            .create_task("Semiconductor automation collection", 1, Stage::Planned, None)
            .unwrap();
        store.task_mut(owned).unwrap().automation_id = Some(automation);
        (store, goal, automation, period, owned)
    }

    #[test]
    fn research_domain_returns_only_ordinary_research_titles() {
        let (mut store, goal, automation, _period, owned) = fixture();
        let automation_project = store
            .ensure_automation_project(std::env::temp_dir().to_string_lossy().into())
            .unwrap();
        let container = store
            .create_task(
                "Semiconductor automation container task",
                automation_project,
                Stage::Planned,
                None,
            )
            .unwrap();
        let hits = search(&store, SearchDomain::Research, "semiconductor", 50);
        assert!(hits.iter().all(|hit| hit.kind == SearchTargetKind::Research));
        assert!(hits.iter().any(|hit| hit.target == SearchTarget::Research(goal)));
        assert!(!hits
            .iter()
            .any(|hit| matches!(hit.target, SearchTarget::Automation(id) if id == automation)));
        assert!(!hits.iter().any(|hit| hit.target == SearchTarget::Research(owned)));
        assert!(!hits.iter().any(|hit| hit.target == SearchTarget::Research(container)));
        assert!(hits.iter().any(|hit| hit.title.contains("Semiconductor supply outlook")));
    }

    #[test]
    fn automation_domain_returns_definitions_and_period_metadata() {
        let (store, _goal, automation, period, _owned) = fixture();
        let hits = search(&store, SearchDomain::Automations, "fuel", 50);
        assert!(hits.iter().any(|hit| hit.target == SearchTarget::Automation(automation)));
        assert!(hits.iter().any(|hit| hit.target == SearchTarget::Period(period)));
        assert!(hits.iter().all(|hit| {
            matches!(
                hit.kind,
                SearchTargetKind::Automation | SearchTargetKind::Period
            )
        }));
        let by_period = search(&store, SearchDomain::Automations, "period 41", 50);
        assert!(
            by_period
                .iter()
                .any(|hit| hit.target == SearchTarget::Period(period)),
            "period number is searchable metadata"
        );
        let result = search(&store, SearchDomain::Automations, "saved result reference", 50);
        assert!(result.iter().any(|hit| hit.target == SearchTarget::Period(period)));
        assert!(search(&store, SearchDomain::Automations, "final report available", 50).is_empty());
    }

    #[test]
    fn title_prefix_outranks_metadata_only_matches() {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().name = "Desk".into();
        let exact = store
            .create_task("Fuel price report", 1, Stage::Planned, None)
            .unwrap();
        let metadata = store
            .create_task("Weekly market brief", 1, Stage::Planned, None)
            .unwrap();
        store.task_mut(metadata).unwrap().labels.push("fuel".into());
        let hits = search(&store, SearchDomain::All, "fuel", 50);
        assert_eq!(hits.first().unwrap().target, SearchTarget::Research(exact));
    }

    #[test]
    fn tokens_are_anded_and_empty_query_is_explicit() {
        let (store, _goal, _automation, _period, _owned) = fixture();
        assert!(search(&store, SearchDomain::All, "   ", 50).is_empty());
        assert!(search(&store, SearchDomain::All, "fuel semiconductor", 50).is_empty());
        assert!(!search(&store, SearchDomain::All, "fuel price", 50).is_empty());
    }

    #[test]
    fn report_bodies_and_logs_are_not_search_sources() {
        let (mut store, _goal, _automation, _period, _owned) = fixture();
        let mut run = store.prepare_run(store.task_at_mut(0).unwrap().id).unwrap();
        run.output = "topsecretreportbody".into();
        run.logs = vec!["topsecretlogline".into()];
        store.push_run(run);
        assert!(search(&store, SearchDomain::All, "topsecretreportbody", 50).is_empty());
        assert!(search(&store, SearchDomain::All, "topsecretlogline", 50).is_empty());
    }

    #[test]
    fn domain_filter_keys_are_stable() {
        assert_eq!(SearchDomain::All.key(), "all");
        assert_eq!(SearchDomain::Research.key(), "research");
        assert_eq!(SearchDomain::Automations.key(), "automations");
        assert!(SearchDomain::All.matches(SearchTargetKind::Period));
        assert!(!SearchDomain::Research.matches(SearchTargetKind::Automation));
        assert!(SearchDomain::Automations.matches(SearchTargetKind::Automation));
    }
}

impl TaskApp {
    /// Footer Search and ⌘K share this entry point. It does not alter the page,
    /// selection, list filters, scroll position or any draft.
    pub(crate) fn open_global_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.global_search_ui.open {
            self.global_search_ui.open = true;
            self.global_search_ui.domain = SearchDomain::All;
            self.global_search_ui.message = None;
            self.global_search_ui.selected = 0;
            self.global_search_ui.previous_focus = window.focused(cx);
            self.global_search
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        let query = self.global_search.read(cx).value().to_string();
        self.global_search_ui.refresh(&self.store, &query);
        self.global_search
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn close_global_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.global_search_ui.open {
            return;
        }
        self.global_search_ui.open = false;
        self.global_search_ui.message = None;
        self.global_search_ui.hits.clear();
        self.global_search_ui.selected = 0;
        self.global_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        let restore = self.global_search_ui.previous_focus.take();
        if let Some(handle) = restore {
            window.focus(&handle, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn set_global_search_domain(
        &mut self,
        domain: SearchDomain,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.global_search_ui.domain = domain;
        self.global_search_ui.selected = 0;
        self.global_search_ui.message = None;
        let query = self.global_search.read(cx).value().to_string();
        self.global_search_ui.refresh(&self.store, &query);
        self.global_search
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn move_global_search_selection(
        &mut self,
        step: isize,
        cx: &mut Context<Self>,
    ) {
        let len = self.global_search_ui.hits.len();
        if len == 0 {
            return;
        }
        let current = self.global_search_ui.selected.min(len - 1);
        let next = (current as isize + step).rem_euclid(len as isize) as usize;
        self.global_search_ui.selected = next;
        self.global_search_ui.message = None;
        self.global_search_ui.scroll.scroll_to_item(next);
        cx.notify();
    }

    pub(crate) fn activate_selected_global_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.pending_exit.is_some() {
            // The overlay key handler and the Input submit path can both fire on
            // one Return; the decision already owns the next step.
            return;
        }
        let query = self.global_search.read(cx).value().to_string();
        let Some(hit) = self
            .global_search_ui
            .hits
            .get(self.global_search_ui.selected)
            .cloned()
        else {
            if self
                .global_search_ui
                .message
                .as_deref()
                .is_some_and(|message| message.contains("no longer exists"))
            {
                // A refresh after the target vanished must not replace the
                // precise "no longer exists" feedback with a generic line.
                return;
            }
            self.global_search_ui.message = Some(if query.trim().is_empty() {
                "Type a search term first.".into()
            } else {
                "No matching result is selected.".into()
            });
            cx.notify();
            return;
        };
        self.activate_global_search_target(hit.target, window, cx);
    }

    /// Single activation seam. Validation stays here; the T10 shared gate owns
    /// origin recording and the dirty decision, and closes Search only when the
    /// user proceeds.
    fn activate_global_search_target(
        &mut self,
        target: SearchTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let navigation = match target {
            SearchTarget::Research(id) => {
                let Some(task) = self.store.task(id) else {
                    self.missing_search_target(cx);
                    return;
                };
                if task.automation_id.is_some() || self.store.task_is_archived(id) {
                    self.missing_search_target(cx);
                    return;
                }
                crate::navigation_context::NavigationTarget::ResearchTask(id)
            }
            SearchTarget::Automation(id) => {
                if self.store.automation(id).is_none() {
                    self.missing_search_target(cx);
                    return;
                }
                crate::navigation_context::NavigationTarget::AutomationDetail(id)
            }
            SearchTarget::Period(id) => {
                let Some(occurrence) = self.store.occurrence(id).cloned() else {
                    self.missing_search_target(cx);
                    return;
                };
                if self
                    .store
                    .automation(occurrence.automation_id)
                    .is_none()
                {
                    self.missing_search_target(cx);
                    return;
                }
                crate::navigation_context::NavigationTarget::AutomationPeriod {
                    automation: occurrence.automation_id,
                    occurrence: id,
                }
            }
        };
        self.request_search_target(navigation, window, cx);
        if self.navigation.pending_exit.is_none() {
            window.focus(&self.focus, cx);
        }
    }

    fn missing_search_target(&mut self, cx: &mut Context<Self>) {
        let query = self
            .global_search_ui
            .hits
            .get(self.global_search_ui.selected)
            .map(|hit| hit.title.clone())
            .unwrap_or_default();
        let query = if query.is_empty() {
            "That result".to_string()
        } else {
            query
        };
        self.global_search_ui.message = Some(format!("{query} no longer exists."));
        let input = self.global_search.read(cx).value().to_string();
        self.global_search_ui.refresh(&self.store, &input);
        cx.notify();
    }

    fn global_search_row(
        &self,
        hit: &SearchHit,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let selected = index == self.global_search_ui.selected;
        let target = hit.target;
        let row_id = format!(
            "global-search-result-{}-{}",
            hit.kind.key(),
            hit.target.id()
        );
        observed(
            row()
                .id(row_id)
                .w_full()
                .flex_shrink_0()
                .items_start()
                .gap_3()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .cursor_pointer()
                .role(Role::ListBoxOption)
                .aria_selected(selected)
                .aria_label(format!("{} · {}", hit.kind.label(), hit.title))
                .aria_description(format!("{} · {}", hit.detail, hit.destination))
                .when(selected, |this| {
                    this.bg(rgb(colors.accent_surface))
                        .border_1()
                        .border_color(rgb(colors.accent))
                })
                .when(!selected, |this| this.hover(|style| style.bg(rgb(colors.hover))))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_global_search_target(target, window, cx);
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .pt(px(2.))
                        .text_color(rgb(if selected {
                            colors.accent
                        } else {
                            colors.muted
                        }))
                        .child(Icon::new(hit.kind.icon()).size_4()),
                )
                .child(
                    col()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .overflow_hidden()
                                .child(hit.title.clone()),
                        )
                        .child(
                            row()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .h(px(18.))
                                        .px_2()
                                        .rounded(px(4.))
                                        .bg(rgb(colors.tag_surface))
                                        .text_color(rgb(colors.tag_text))
                                        .text_xs()
                                        .child(hit.kind.label()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_xs()
                                        .text_color(rgb(colors.muted))
                                        .child(hit.detail.clone()),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(rgb(colors.accent))
                                        .child(hit.destination.clone()),
                                ),
                        ),
                ),
        )
    }

    pub(crate) fn global_search_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let query = self.global_search.read(cx).value().to_string();
        let mut panel = col()
            .id("global-search-panel")
            .w(px(PANEL_WIDTH))
            .max_w(relative(0.92))
            .h(px(520.))
            .max_h(relative(0.86))
            .p_3()
            .gap_3()
            .bg(rgb(colors.panel))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(colors.border_strong))
            .text_color(rgb(colors.text))
            .capture_key_down(cx.listener(
                |this, event: &KeyDownEvent, window, cx| {
                    if !this.global_search_ui.open {
                        return;
                    }
                    if event.keystroke.modifiers.platform {
                        return;
                    }
                    match event.keystroke.key.as_str() {
                        "up" => {
                            this.move_global_search_selection(-1, cx);
                            cx.stop_propagation();
                        }
                        "down" => {
                            this.move_global_search_selection(1, cx);
                            cx.stop_propagation();
                        }
                        "escape" => {
                            this.close_global_search(window, cx);
                            cx.stop_propagation();
                        }
                        "enter" => {
                            // The overlay listens for Enter as well as the Input
                            // submit path so a decision that temporarily moved
                            // focus cannot swallow the second activation.
                            this.activate_selected_global_search(window, cx);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                },
            ))
            .child(
                row()
                    .justify_between()
                    .gap_3()
                    .child(
                        col().gap_1().child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Search"),
                        ).child(muted(
                            "Research titles, automation definitions and periods.",
                            cx,
                        )),
                    )
                    .child(
                        tool_icon(
                            "global-search-close",
                            assets::IconName::Close,
                            "Close search",
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_global_search(window, cx);
                        })),
                    ),
            )
            .child(Input::new(&self.global_search).id("global-search-input"))
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(label("SEARCH IN", cx))
                    .children(
                        [
                            SearchDomain::All,
                            SearchDomain::Research,
                            SearchDomain::Automations,
                        ]
                        .into_iter()
                        .map(|domain| {
                            select_button(
                                format!("global-search-domain-{}", domain.key()),
                                domain.label(),
                                self.global_search_ui.domain == domain,
                                cx,
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_global_search_domain(domain, window, cx);
                            }))
                        }),
                    ),
            );

        let mut list = col()
            .id("global-search-results")
            .flex_1()
            .min_h_0()
            .gap_1()
            .overflow_y_scroll()
            .track_scroll(&self.global_search_ui.scroll);
        if self.global_search_ui.hits.is_empty() {
            list = list.child(
                col()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .gap_2()
                    .child(observed(
                        div()
                            .id("global-search-empty-title")
                            .aria_label(if query.trim().is_empty() {
                                "Search across Research and Automations"
                            } else {
                                "No matches"
                            })
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(if query.trim().is_empty() {
                                "Search across Research and Automations"
                            } else {
                                "No matches"
                            }),
                    ))
                    .child({
                        let hint = if query.trim().is_empty() {
                            "Type a title, automation name, period number or status.".to_string()
                        } else {
                            format!(
                                "Nothing in {} matches that query. Try fewer words or choose All.",
                                self.global_search_ui.domain.label()
                            )
                        };
                        observed(
                            div()
                                .id("global-search-empty-hint")
                                .aria_label(hint.clone())
                                .child(muted(hint, cx)),
                        )
                    }),
            );
        } else {
            for (index, hit) in self.global_search_ui.hits.iter().enumerate() {
                list = list.child(self.global_search_row(hit, index, cx));
            }
        }
        panel = panel.child(list);
        panel = panel.child(
            row()
                .justify_between()
                .gap_3()
                .text_xs()
                .child(muted("Up/Down select · Enter open · Esc close", cx))
                .when_some(self.global_search_ui.message.clone(), |this, message| {
                    this.child(observed(
                        div()
                            .id("global-search-message")
                            .aria_label(message.clone())
                            .text_color(rgb(colors.danger))
                            .child(selectable(message)),
                    ))
                }),
        );
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x000000a0))
            .flex()
            .items_start()
            .justify_center()
            .pt(px(64.))
            .child(observed(
                panel.focus_trap("global-search-focus", &self.global_search_focus),
            ))
            .into_any_element()
    }
}
