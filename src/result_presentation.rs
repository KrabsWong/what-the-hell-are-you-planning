//! Typed result presentation shared by Research, Automation and reader surfaces.
//!
//! This module chooses what a result surface may show. It never changes execution,
//! scheduling, navigation or stored report text. Raw output remains available only
//! through technical diagnostics.
use crate::automation::{AutomationOccurrence, OccurrenceStatus};
use crate::domain::{AgentRun, Stage, Store};
use crate::research::{self, ResearchKind, Source};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    FinalReport,
    PartialPeriod,
    IntermediateEvidence,
    LiveProgress,
    PlanningAttempt,
    NonResearchResult,
    None,
}

impl ResultKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::FinalReport => "Final report",
            Self::PartialPeriod => "Partial period reports",
            Self::IntermediateEvidence => "Intermediate evidence",
            Self::LiveProgress => "Live progress",
            Self::PlanningAttempt => "Planning attempt",
            Self::NonResearchResult => "Task result",
            Self::None => "No report",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptStatus {
    Queued,
    Running,
    Recovering,
    Stopping,
    Verifying,
    Succeeded,
    Failed,
    Blocked,
    Stale,
    Cancelled,
    Interrupted,
    Unknown,
}

impl AttemptStatus {
    pub fn from_run_status(status: &str) -> Self {
        match status {
            "Queued" => Self::Queued,
            "Running" => Self::Running,
            "Recovering" => Self::Recovering,
            "Cancelling" => Self::Stopping,
            "Verifying" => Self::Verifying,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            "Blocked" => Self::Blocked,
            "Stale" => Self::Stale,
            "Cancelled" => Self::Cancelled,
            "Interrupted" => Self::Interrupted,
            _ => Self::Unknown,
        }
    }

    pub fn from_occurrence_status(status: OccurrenceStatus) -> Self {
        match status {
            OccurrenceStatus::Queued => Self::Queued,
            OccurrenceStatus::Running => Self::Running,
            OccurrenceStatus::WaitingRetry => Self::Queued,
            OccurrenceStatus::Verifying => Self::Verifying,
            OccurrenceStatus::Succeeded => Self::Succeeded,
            OccurrenceStatus::Failed => Self::Failed,
            OccurrenceStatus::Interrupted => Self::Interrupted,
            OccurrenceStatus::Cancelled => Self::Cancelled,
            OccurrenceStatus::Skipped => Self::Cancelled,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Running | Self::Recovering | Self::Stopping | Self::Verifying
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Freshness {
    pub historical: bool,
    pub stale_input: bool,
    pub previous_version: bool,
    pub regenerating: bool,
    pub legacy_contract: bool,
}

impl Freshness {
    pub fn is_current(self) -> bool {
        !self.historical && !self.stale_input && !self.regenerating
    }

    /// Every applicable state, so one flag never hides another.
    pub fn labels(self) -> Vec<&'static str> {
        let mut labels = vec![];
        if self.stale_input {
            labels.push("Stale input");
        }
        if self.regenerating {
            labels.push("Regenerating");
        }
        if self.previous_version {
            labels.push("Original approved version");
        }
        if self.historical {
            labels.push("Historical result");
        }
        if self.legacy_contract {
            labels.push("Legacy result");
        }
        labels
    }

    pub fn summary(self) -> Option<&'static str> {
        self.labels().into_iter().next()
    }

    fn merge(&mut self, other: Self) {
        self.historical |= other.historical;
        self.stale_input |= other.stale_input;
        self.previous_version |= other.previous_version;
        self.regenerating |= other.regenerating;
        self.legacy_contract |= other.legacy_contract;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceQuality {
    Complete,
    Partial,
    Blocked,
    UnverifiedLegacy,
    Rejected,
    NotValidated,
    PlainText,
    Incomplete,
    None,
}

impl EvidenceQuality {
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Complete => Some("Complete"),
            Self::Partial => Some("Evidence gaps"),
            Self::Blocked => Some("Needs input"),
            Self::UnverifiedLegacy => Some("Unverified legacy"),
            Self::Rejected => Some("Report rejected"),
            Self::NotValidated => Some("Not validated"),
            Self::PlainText => None,
            Self::Incomplete => Some("Incomplete"),
            Self::None => None,
        }
    }

    pub fn row_suffix(self) -> Option<&'static str> {
        match self {
            Self::Partial => Some("evidence gaps"),
            Self::Blocked => Some("needs input"),
            Self::UnverifiedLegacy => Some("unverified legacy"),
            Self::Rejected => Some("report rejected"),
            Self::Incomplete => Some("incomplete"),
            _ => None,
        }
    }

    pub fn is_valid_report(self) -> bool {
        matches!(self, Self::Complete | Self::Partial)
    }

    pub fn is_reader_body(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Partial | Self::Blocked | Self::UnverifiedLegacy | Self::PlainText
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProgressPresentation {
    pub run_id: u64,
    pub owner_label: String,
    pub step_label: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultPresentation {
    pub kind: ResultKind,
    pub attempt: AttemptStatus,
    pub freshness: Freshness,
    pub quality: EvidenceQuality,
    pub run_id: Option<u64>,
    pub step_label: Option<String>,
    pub reader_markdown: String,
    pub sources: Vec<Source>,
    pub notice: Option<&'static str>,
    pub progress: Vec<ProgressPresentation>,
    pub reports: Vec<OwnedReportPresentation>,
    pub has_final_report: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedReportPresentation {
    pub owner_label: String,
    pub presentation: ResultPresentation,
}

impl ResultPresentation {
    pub fn empty() -> Self {
        Self {
            kind: ResultKind::None,
            attempt: AttemptStatus::Unknown,
            freshness: Freshness::default(),
            quality: EvidenceQuality::None,
            run_id: None,
            step_label: None,
            reader_markdown: String::new(),
            sources: vec![],
            notice: None,
            progress: vec![],
            reports: vec![],
            has_final_report: false,
        }
    }

    /// Reader-facing copy and export text. Raw output is never returned here.
    pub fn reader_text(&self) -> String {
        reader_text(&self.reader_markdown, self.notice)
    }

    pub fn has_reader_body(&self) -> bool {
        !self.reader_markdown.trim().is_empty()
    }

    pub fn status_suffix(&self) -> Option<&'static str> {
        self.quality.row_suffix()
    }

    /// Compact visible state used by result surfaces and focused QA.
    pub fn state_label(&self) -> String {
        let mut label = self.kind.label().to_string();
        if let Some(quality) = self.quality.label() {
            label.push_str(&format!(" · {quality}"));
        }
        let freshness = self.freshness.labels();
        label.push_str(" · ");
        if freshness.is_empty() {
            label.push_str(if self.attempt.is_active() {
                "In progress"
            } else {
                "Current"
            });
        } else {
            label.push_str(&freshness.join(" · "));
        }
        label
    }

    pub fn primary_run_id(&self) -> Option<u64> {
        self.reports
            .iter()
            .find(|report| report.presentation.has_final_report)
            .or_else(|| self.reports.iter().find(|report| report.presentation.run_id.is_some()))
            .and_then(|report| report.presentation.run_id)
            .or(self.run_id)
            .or_else(|| self.progress.first().map(|progress| progress.run_id))
    }
}

/// Shared reader text, used by display, copy and export.
pub fn reader_text(markdown: &str, notice: Option<&str>) -> String {
    let markdown = markdown.trim_end();
    match notice {
        Some(notice) if !markdown.is_empty() => {
            format!("> **Evidence gaps:** {notice}\n\n{markdown}")
        }
        Some(notice) => format!("> **Evidence gaps:** {notice}"),
        None => markdown.to_string(),
    }
}

/// Historical validation for archived work. This uses the same report parsing
/// and quality rules as the reader selector, but treats a signature-matched run
/// as the current historical revision. It never admits execution.
pub(crate) fn historical_output_valid(store: &Store, run: &AgentRun) -> bool {
    let Some(kind) = run.research_kind else {
        return false;
    };
    if run.status != "Succeeded" {
        return false;
    }
    classify_research_output(store, run, kind, true)
        .quality
        .is_valid_report()
}

/// Ordinary Research, planning and non-research task presentation.
pub fn present_run(store: &Store, run: &AgentRun) -> ResultPresentation {
    let planning =
        run.project_plan.is_some() || run.research_kind == Some(ResearchKind::Goal);
    let attempt = AttemptStatus::from_run_status(&run.status);
    let step_label = store.task(run.task_id).map(|task| {
        format!("{} · {}", store.identifier(task), task.title)
    });

    if planning {
        return ResultPresentation {
            kind: ResultKind::PlanningAttempt,
            attempt,
            freshness: Freshness::default(),
            quality: EvidenceQuality::NotValidated,
            run_id: Some(run.id),
            step_label,
            progress: active_progress(run, "Planning", store),
            ..ResultPresentation::empty()
        };
    }

    let Some(kind) = run.research_kind else {
        return present_non_research(run, attempt, step_label);
    };

    let current_signature = store.research_signature(run.task_id);
    let regenerating = run.status == "Succeeded"
        && store.runs().iter().any(|other| {
            other.id != run.id
                && other.id > run.id
                && other.task_id == run.task_id
                && other.research_kind == Some(ResearchKind::Advise)
                && other.automation_occurrence_id == run.automation_occurrence_id
                && AttemptStatus::from_run_status(&other.status).is_active()
        });
    let current = store
        .research_result(run.task_id)
        .map(|result| result.id)
        == Some(run.id)
        // A regeneration temporarily owns the task's latest run. The previous
        // validated report remains the current reader answer while it runs.
        || (regenerating
            && !run.input_signature.is_empty()
            && current_signature.as_ref() == Some(&run.input_signature));
    let classified = classify_research_output(store, run, kind, current);
    let mut freshness = Freshness::default();
    freshness.historical = classified.quality.is_reader_body() && !current;
    freshness.stale_input = classified.quality.is_reader_body()
        && !current
        && run.status == "Succeeded"
        && match current_signature {
            Some(signature) => !run.input_signature.is_empty() && signature != run.input_signature,
            // Archived or removed context is retained as history; a live task with
            // unavailable approval/context cannot prove the old result current.
            None => store.task(run.task_id).is_some_and(|task| {
                !task.archived && !matches!(task.stage, Stage::Cancelled)
            }),
        };
    freshness.legacy_contract = classified.quality == EvidenceQuality::UnverifiedLegacy;
    freshness.regenerating = regenerating;

    let report_kind = match kind {
        ResearchKind::Advise
            if matches!(
                classified.quality,
                EvidenceQuality::Complete
                    | EvidenceQuality::Partial
                    | EvidenceQuality::Blocked
                    | EvidenceQuality::UnverifiedLegacy
            ) =>
        {
            ResultKind::FinalReport
        }
        ResearchKind::Collect | ResearchKind::Analyze
            if classified.quality.is_reader_body() =>
        {
            ResultKind::IntermediateEvidence
        }
        _ if attempt.is_active() && !run.output.trim().is_empty() => {
            ResultKind::LiveProgress
        }
        _ => ResultKind::None,
    };
    let mut presentation = ResultPresentation {
        kind: report_kind,
        attempt,
        freshness,
        quality: classified.quality,
        run_id: Some(run.id),
        step_label,
        reader_markdown: classified.body,
        sources: classified.sources,
        notice: classified.notice,
        progress: if attempt.is_active() {
            active_progress(run, "Live progress", store)
        } else {
            vec![]
        },
        reports: vec![],
        // A stale or unverified result remains inspectable, but it is not the
        // current validated answer.
        has_final_report: report_kind == ResultKind::FinalReport
            && classified.quality.is_valid_report()
            && !freshness.stale_input,
    };
    if presentation.kind == ResultKind::LiveProgress {
        presentation.reader_markdown.clear();
        presentation.sources.clear();
        presentation.notice = None;
    }
    presentation
}

fn present_non_research(
    run: &AgentRun,
    attempt: AttemptStatus,
    step_label: Option<String>,
) -> ResultPresentation {
    if run.status == "Succeeded" && !run.output.trim().is_empty() {
        // Ordinary non-research tasks have always shown plain agent text. Keep that
        // behavior and label it distinctly; do not impose the research JSON contract.
        return ResultPresentation {
            kind: ResultKind::NonResearchResult,
            attempt,
            quality: EvidenceQuality::PlainText,
            run_id: Some(run.id),
            step_label,
            reader_markdown: run.output.clone(),
            sources: run.sources.clone(),
            has_final_report: true,
            ..ResultPresentation::empty()
        };
    }
    ResultPresentation {
        kind: if attempt.is_active() && !run.output.trim().is_empty() {
            ResultKind::LiveProgress
        } else {
            ResultKind::None
        },
        attempt,
        quality: EvidenceQuality::NotValidated,
        run_id: Some(run.id),
        step_label,
        progress: if attempt.is_active() {
            vec![ProgressPresentation {
                run_id: run.id,
                owner_label: "Task".into(),
                step_label: "Live progress".into(),
                text: run.output.clone(),
            }]
        } else {
            vec![]
        },
        ..ResultPresentation::empty()
    }
}

fn active_progress(run: &AgentRun, label: &str, store: &Store) -> Vec<ProgressPresentation> {
    if run.output.trim().is_empty() {
        return vec![];
    }
    vec![ProgressPresentation {
        run_id: run.id,
        owner_label: store
            .task(run.task_id)
            .map(|task| store.identifier(task))
            .unwrap_or_else(|| format!("Run {}", run.id)),
        step_label: label.into(),
        text: run.output.clone(),
    }]
}

struct ClassifiedOutput {
    body: String,
    quality: EvidenceQuality,
    sources: Vec<Source>,
    notice: Option<&'static str>,
}

fn classify_research_output(
    store: &Store,
    run: &AgentRun,
    kind: ResearchKind,
    current: bool,
) -> ClassifiedOutput {
    let output = run.output.as_str();
    if output.trim().is_empty() {
        return ClassifiedOutput {
            body: String::new(),
            quality: EvidenceQuality::None,
            sources: vec![],
            notice: None,
        };
    }

    // Lifecycle gates provenance before any text shape. Streaming output and a
    // finished-but-unvalidated attempt are never promoted to a final answer.
    let attempt = AttemptStatus::from_run_status(&run.status);
    if attempt.is_active() || !matches!(run.status.as_str(), "Succeeded" | "Blocked") {
        return ClassifiedOutput {
            body: String::new(),
            quality: EvidenceQuality::NotValidated,
            sources: vec![],
            notice: None,
        };
    }

    if run.status == "Blocked" && report_envelope(output, kind).is_some() {
        // A blocked run's stored contract is reader prose. A raw envelope means
        // this attempt did not finish validation and must not become a report.
        return ClassifiedOutput {
            body: String::new(),
            quality: EvidenceQuality::NotValidated,
            sources: vec![],
            notice: None,
        };
    }

    if research::has_evidence_gap_note(output) {
        return ClassifiedOutput {
            body: research::reader_markdown(output).to_string(),
            quality: EvidenceQuality::Partial,
            sources: run.sources.clone(),
            notice: Some(research::EVIDENCE_GAP_NOTICE),
        };
    }

    if let Some(envelope) = report_envelope(output, kind) {
        return match envelope {
            Ok(report) => {
                let quality = match report.status.as_str() {
                    "partial" => EvidenceQuality::Partial,
                    "blocked" => EvidenceQuality::Blocked,
                    _ => EvidenceQuality::Complete,
                };
                ClassifiedOutput {
                    body: report.report,
                    quality,
                    // Stored sources are the reviewable contract for this run.
                    sources: run.sources.clone(),
                    notice: (quality == EvidenceQuality::Partial)
                        .then_some(research::EVIDENCE_GAP_NOTICE),
                }
            }
            Err(_) => ClassifiedOutput {
                body: String::new(),
                quality: EvidenceQuality::Rejected,
                sources: vec![],
                notice: None,
            },
        };
    }

    match run.status.as_str() {
        "Succeeded" if current => ClassifiedOutput {
            body: research::reader_markdown(output).to_string(),
            quality: EvidenceQuality::Complete,
            sources: run.sources.clone(),
            notice: None,
        },
        "Succeeded" => {
            // A signature match proves this superseded run was validated for the
            // same inputs. Do not use task stage as a proxy for validation.
            let signature_matches = !run.input_signature.is_empty()
                && store
                    .research_signature(run.task_id)
                    .is_some_and(|signature| signature == run.input_signature);
            ClassifiedOutput {
                body: research::reader_markdown(output).to_string(),
                quality: if signature_matches {
                    EvidenceQuality::Complete
                } else {
                    EvidenceQuality::UnverifiedLegacy
                },
                sources: run.sources.clone(),
                notice: None,
            }
        }
        "Blocked" => ClassifiedOutput {
            body: research::reader_markdown(output).to_string(),
            quality: EvidenceQuality::Blocked,
            sources: run.sources.clone(),
            notice: None,
        },
        _ => ClassifiedOutput {
            body: String::new(),
            quality: EvidenceQuality::NotValidated,
            sources: vec![],
            notice: None,
        },
    }
}

/// Detect a stored research report envelope. Plain Markdown that merely starts
/// with `{` is not an envelope and remains a legitimate reader body.
fn report_envelope(text: &str, kind: ResearchKind) -> Option<Result<research::Report, String>> {
    let trimmed = text.trim_start();
    let fenced = trimmed.starts_with("```");
    if !(trimmed.starts_with('{') || fenced) {
        return None;
    }
    match research::response_json(trimmed) {
        Ok(json) => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
                return envelope_candidate(trimmed).then(|| {
                    Err("The saved report envelope is not valid JSON.".into())
                });
            };
            let Some(object) = value.as_object() else {
                return None;
            };
            if !object.contains_key("status") || !object.contains_key("report") {
                return None;
            }
            Some(research::parse_report(trimmed, kind))
        }
        Err(error) => envelope_candidate(trimmed)
            .then_some(Err(format!("The saved report envelope is incomplete: {error}"))),
    }
}

fn envelope_candidate(text: &str) -> bool {
    text.contains("\"status\"") && text.contains("\"report\"")
}

/// Automation period presentation. A period is a complete answer only when every
/// required goal has its own validated final report.
pub fn present_occurrence(
    store: &Store,
    occurrence: &AutomationOccurrence,
) -> ResultPresentation {
    let definition = store.automation(occurrence.automation_id);
    let previous_version = definition
        .is_some_and(|definition| definition.version != occurrence.definition_version);
    let regenerating = occurrence.status == OccurrenceStatus::Succeeded
        && store.runs().iter().any(|run| {
            run.automation_occurrence_id == Some(occurrence.id)
                && run.research_kind == Some(ResearchKind::Advise)
                && AttemptStatus::from_run_status(&run.status).is_active()
        });
    let attempt = AttemptStatus::from_occurrence_status(occurrence.status);

    let goal_ids: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
        occurrence.research_goal_id.into_iter().collect()
    } else {
        occurrence.research_goal_ids.clone()
    };

    if goal_ids.is_empty() {
        return present_occurrence_without_goals(
            store,
            occurrence,
            attempt,
            previous_version,
            regenerating,
        );
    }

    let mut reports = vec![];
    let mut progress = vec![];
    let goal_count = goal_ids.len();
    let mut all_final = true;
    let mut any_final = false;
    let mut any_partial = false;
    let mut any_rejected = false;
    let mut any_unverified = false;
    let mut any_evidence = false;
    let mut freshness = Freshness {
        previous_version,
        regenerating,
        ..Freshness::default()
    };

    for goal in goal_ids {
        let owner_label = store
            .task(goal)
            .map(|task| format!("{} · {}", store.identifier(task), task.title))
            .unwrap_or_else(|| format!("Goal {goal}"));
        let advise_tasks: Vec<u64> = store
            .research_children(goal)
            .into_iter()
            .filter(|task| {
                store
                    .task(*task)
                    .and_then(|task| task.research.as_ref())
                    .is_some_and(|research| research.kind == ResearchKind::Advise)
            })
            .collect();

        // Every required final-report step in this goal must have its own
        // validated report. One successful Advise step cannot stand in for others.
        let mut goal_final = !advise_tasks.is_empty();
        let mut report_count = 0usize;
        for task in &advise_tasks {
            let step_label = store
                .task(*task)
                .map(|task| task.title.clone())
                .unwrap_or_else(|| format!("Report {task}"));
            let Some(run) = choose_goal_report_run(store, occurrence, *task) else {
                goal_final = false;
                if goal_count > 1 {
                    reports.push(OwnedReportPresentation {
                        owner_label: format!("{owner_label} / {step_label}"),
                        presentation: ResultPresentation::empty(),
                    });
                }
                continue;
            };
            let mut presentation = present_run(store, run);
            presentation.freshness.merge(Freshness {
                previous_version,
                regenerating,
                ..Freshness::default()
            });
            report_count += 1;
            if presentation.has_final_report {
                any_final = true;
                any_partial |= presentation.quality == EvidenceQuality::Partial;
            } else {
                goal_final = false;
                if presentation.quality.is_reader_body() {
                    any_evidence = true;
                    any_unverified |= presentation.quality == EvidenceQuality::UnverifiedLegacy;
                }
            }
            any_rejected |= presentation.quality == EvidenceQuality::Rejected;
            freshness.merge(presentation.freshness);
            reports.push(OwnedReportPresentation {
                owner_label: format!("{owner_label} / {step_label}"),
                presentation,
            });
        }

        for run in store.runs().iter().filter(|run| {
            run.automation_occurrence_id == Some(occurrence.id)
                && store.owning_research_goal(run.task_id) == Some(goal)
                && AttemptStatus::from_run_status(&run.status).is_active()
                && !run.output.trim().is_empty()
        }) {
            progress.push(ProgressPresentation {
                run_id: run.id,
                owner_label: owner_label.clone(),
                step_label: store
                    .task(run.task_id)
                    .map(|task| task.title.clone())
                    .unwrap_or_else(|| "Live progress".into()),
                text: run.output.clone(),
            });
        }

        // Without a final answer, keep completed evidence steps inspectable for
        // this goal. A failed or cancelled period must not hide earlier work.
        if !goal_final {
            for (step_label, mut presentation) in goal_evidence(store, occurrence, goal) {
                presentation.freshness.merge(Freshness {
                    previous_version,
                    regenerating,
                    ..Freshness::default()
                });
                any_evidence = true;
                any_unverified |= presentation.quality == EvidenceQuality::UnverifiedLegacy;
                freshness.merge(presentation.freshness);
                reports.push(OwnedReportPresentation {
                    owner_label: format!("{owner_label} / {step_label}"),
                    presentation,
                });
            }
            if report_count == 0 && goal_count > 1 {
                reports.push(OwnedReportPresentation {
                    owner_label,
                    presentation: ResultPresentation::empty(),
                });
            }
        }
        all_final &= goal_final;
    }

    // Older occurrences may store one explicit final run that is not a modern
    // Advise child of any goal. Keep its body and sources inspectable, but label
    // it as an unverified legacy result rather than a validated period answer.
    if let Some(final_id) = occurrence.final_run_id
        && !reports
            .iter()
            .any(|report| report.presentation.run_id == Some(final_id))
        && let Some(run) = store.runs().iter().find(|run| {
            run.id == final_id && run.automation_occurrence_id == Some(occurrence.id)
        })
    {
        let mut presentation = present_run(store, run);
        presentation.freshness.merge(Freshness {
            previous_version,
            regenerating,
            ..Freshness::default()
        });
        if presentation.quality.is_reader_body() {
            any_evidence = true;
            any_unverified |= presentation.quality == EvidenceQuality::UnverifiedLegacy;
        }
        any_rejected |= presentation.quality == EvidenceQuality::Rejected;
        freshness.merge(presentation.freshness);
        let owner_label = store
            .task(run.task_id)
            .map(|task| format!("{} · {}", store.identifier(task), task.title))
            .unwrap_or_else(|| format!("Period result · {}", run.task_title));
        reports.push(OwnedReportPresentation {
            owner_label,
            presentation,
        });
    }

    let kind = if all_final {
        ResultKind::FinalReport
    } else if any_final {
        ResultKind::PartialPeriod
    } else if !progress.is_empty() {
        ResultKind::LiveProgress
    } else if any_evidence {
        ResultKind::IntermediateEvidence
    } else {
        ResultKind::None
    };

    let quality = if all_final {
        if any_partial {
            EvidenceQuality::Partial
        } else {
            EvidenceQuality::Complete
        }
    } else if any_rejected {
        EvidenceQuality::Rejected
    } else if any_unverified {
        EvidenceQuality::UnverifiedLegacy
    } else if any_final || any_evidence {
        EvidenceQuality::Incomplete
    } else if !progress.is_empty() {
        EvidenceQuality::NotValidated
    } else {
        EvidenceQuality::None
    };

    let primary = reports
        .iter()
        .find(|report| report.presentation.has_final_report)
        .or_else(|| {
            reports
                .iter()
                .find(|report| report.presentation.has_reader_body())
        })
        .or_else(|| {
            reports
                .iter()
                .find(|report| report.presentation.run_id.is_some())
        })
        .map(|report| report.presentation.clone())
        .unwrap_or_else(ResultPresentation::empty);
    let (reader_markdown, sources) = if reports.len() == 1 {
        (primary.reader_markdown.clone(), primary.sources.clone())
    } else {
        (String::new(), vec![])
    };
    let (reader_markdown, sources) = if kind == ResultKind::LiveProgress {
        (String::new(), vec![])
    } else {
        (reader_markdown, sources)
    };
    let notice = (quality == EvidenceQuality::Partial)
        .then_some(research::EVIDENCE_GAP_NOTICE);

    ResultPresentation {
        kind,
        attempt,
        freshness,
        quality,
        run_id: primary.run_id,
        step_label: None,
        reader_markdown,
        sources,
        notice,
        progress,
        reports,
        has_final_report: all_final,
    }
}

fn present_occurrence_without_goals(
    store: &Store,
    occurrence: &AutomationOccurrence,
    attempt: AttemptStatus,
    previous_version: bool,
    regenerating: bool,
) -> ResultPresentation {
    let candidate = store
        .runs()
        .iter()
        .filter(|run| run.automation_occurrence_id == Some(occurrence.id))
        .max_by_key(|run| run.id);
    let Some(run) = candidate else {
        return ResultPresentation {
            attempt,
            freshness: Freshness {
                previous_version,
                regenerating,
                ..Freshness::default()
            },
            ..ResultPresentation::empty()
        };
    };
    let mut presentation = present_run(store, run);
    presentation.freshness.previous_version = previous_version;
    presentation.freshness.regenerating = regenerating;
    presentation.attempt = attempt;
    presentation
}

fn choose_goal_report_run<'a>(
    store: &'a Store,
    occurrence: &AutomationOccurrence,
    task: u64,
) -> Option<&'a AgentRun> {
    if let Some(final_id) = occurrence.final_run_id
        && let Some(run) = store.runs().iter().find(|run| {
            run.id == final_id
                && run.task_id == task
                && run.automation_occurrence_id == Some(occurrence.id)
        })
    {
        return Some(run);
    }
    if let Some(current) = store.research_result(task)
        && current.automation_occurrence_id == Some(occurrence.id)
    {
        return Some(current);
    }
    store
        .runs()
        .iter()
        .filter(|run| {
            run.automation_occurrence_id == Some(occurrence.id) && run.task_id == task
        })
        .max_by_key(|run| {
            (
                run.status == "Succeeded" && !run.output.trim().is_empty(),
                run.id,
            )
        })
}

/// Completed evidence or analysis steps for one goal, used only when the goal
/// has no validated final report. Sources and step labels travel with each item.
fn goal_evidence(
    store: &Store,
    occurrence: &AutomationOccurrence,
    goal: u64,
) -> Vec<(String, ResultPresentation)> {
    let mut evidence = vec![];
    for task in store.research_children(goal) {
        let Some(kind) = store
            .task(task)
            .and_then(|task| task.research.as_ref())
            .map(|research| research.kind)
        else {
            continue;
        };
        if !matches!(kind, ResearchKind::Collect | ResearchKind::Analyze) {
            continue;
        }
        let Some(run) = choose_goal_step_run(store, occurrence, task) else {
            continue;
        };
        let presentation = present_run(store, run);
        if !presentation.quality.is_reader_body() || presentation.freshness.stale_input {
            continue;
        }
        let label = store
            .task(task)
            .map(|task| task.title.clone())
            .unwrap_or_else(|| format!("Step {task}"));
        evidence.push((label, presentation));
    }
    evidence
}

fn choose_goal_step_run<'a>(
    store: &'a Store,
    occurrence: &AutomationOccurrence,
    task: u64,
) -> Option<&'a AgentRun> {
    if let Some(current) = store.research_result(task)
        && current.automation_occurrence_id == Some(occurrence.id)
    {
        return Some(current);
    }
    store
        .runs()
        .iter()
        .filter(|run| {
            run.automation_occurrence_id == Some(occurrence.id)
                && run.task_id == task
                && run.status == "Succeeded"
                && !run.output.trim().is_empty()
        })
        .max_by_key(|run| run.id)
}

/// Compatibility accessor for existing callers. Selection itself is typed above.
pub fn occurrence_report<'a>(
    store: &'a Store,
    occurrence: &AutomationOccurrence,
) -> Option<&'a AgentRun> {
    let presentation = present_occurrence(store, occurrence);
    presentation
        .primary_run_id()
        .and_then(|id| store.runs().iter().find(|run| run.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationDefinition, AutomationOccurrenceSnapshot, AutomationPolicy, AutomationShape,
        AutomationTemplate, DefinitionStatus, TimeWindow, TriggerSource,
    };
    use crate::automation_schedule::{Schedule, ScheduleSpec};
    use crate::domain::Stage;

    const PLAN: &str = r#"{"tasks":[{"title":"Collect evidence","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze findings","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Write report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#;

    fn workspace() -> (Store, u64) {
        let mut store = Store::mock();
        store.projects[0].directory = "/tmp".into();
        store.research_config.models =
            ["test/plan", "test/collect", "test/analyze", "test/advise"].map(String::from);
        let goal = store
            .create_research_goal(
                "T06 result presentation".into(),
                "Exercise typed result states without external services.".into(),
                1,
            )
            .unwrap();
        (store, goal)
    }

    fn add_run(
        store: &mut Store,
        task: u64,
        occurrence: Option<u64>,
        status: &str,
        output: &str,
    ) -> u64 {
        let mut run = store.prepare_run(task).unwrap();
        run.automation_occurrence_id = occurrence;
        run.status = status.into();
        run.output = output.into();
        run.sources = vec![Source {
            title: "Fixture source".into(),
            url: "https://example.com/source".into(),
            published_at: "2026-11-08".into(),
        }];
        let id = run.id;
        store.push_run(run);
        id
    }

    fn occurrence(
        id: u64,
        goals: Vec<u64>,
        status: OccurrenceStatus,
        final_run_id: Option<u64>,
        definition_version: u32,
    ) -> AutomationOccurrence {
        AutomationOccurrence {
            id,
            automation_id: 99,
            definition_version,
            source: TriggerSource::Manual,
            trigger_key: format!("manual:{id}"),
            scheduled_for: None,
            requested_at: Some(100),
            actual_started_at: Some(101),
            timezone: "UTC".into(),
            reference_time: 100,
            data_window: TimeWindow::unresolved("fixture"),
            snapshot: AutomationOccurrenceSnapshot {
                objective: "Fixture objective".into(),
                expected_result: "Fixture result".into(),
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
                approval_signature: "fixture".into(),
            },
            status,
            reason: None,
            research_goal_id: goals.first().copied(),
            research_goal_ids: goals,
            research_topic_id: None,
            run_ids: vec![],
            final_run_id,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: 99,
            deadline_at: None,
            retry_requested_at: None,
        }
    }

    fn add_definition(store: &mut Store, id: u64, version: u32) {
        store.push_automation(AutomationDefinition {
            id,
            version,
            project_id: 1,
            shape: AutomationShape::Research,
            name: "Typed result fixture".into(),
            objective: "Fixture objective".into(),
            expected_result: "Fixture result".into(),
            schedule: Schedule {
                spec: ScheduleSpec::Interval { minutes: 10 },
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
    }

    #[test]
    fn running_complete_envelope_stays_progress() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], Some(1), "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], Some(1), "Succeeded", "Analyzed findings");
        let running = add_run(
            &mut store,
            steps[2],
            Some(1),
            "Running",
            r#"{"status":"complete","report":"Premature answer","sources":[]}"#,
        );
        let presentation = present_run(&store, store.runs().iter().find(|run| run.id == running).unwrap());
        assert_eq!(presentation.kind, ResultKind::LiveProgress);
        assert_eq!(presentation.quality, EvidenceQuality::NotValidated);
        assert!(presentation.reader_markdown.is_empty());
        assert!(!presentation.has_final_report);
        assert_eq!(presentation.progress[0].text, r#"{"status":"complete","report":"Premature answer","sources":[]}"#);
    }

    #[test]
    fn failed_and_cancelled_envelopes_are_not_answers() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], Some(1), "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], Some(1), "Succeeded", "Analyzed findings");
        let failed = add_run(
            &mut store,
            steps[2],
            Some(1),
            "Failed",
            r#"{"status":"partial","report":"Failed partial envelope","sources":[]}"#,
        );
        let failed_presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == failed).unwrap());
        assert_eq!(failed_presentation.quality, EvidenceQuality::NotValidated);
        assert!(failed_presentation.reader_markdown.is_empty());
        assert!(!failed_presentation.has_final_report);

        let cancelled = add_run(
            &mut store,
            steps[2],
            Some(1),
            "Cancelled",
            &format!("{}\n\nCancelled report body", research::EVIDENCE_GAP_NOTE),
        );
        let cancelled_presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == cancelled).unwrap());
        assert_eq!(cancelled_presentation.quality, EvidenceQuality::NotValidated);
        assert!(cancelled_presentation.notice.is_none());
        assert!(cancelled_presentation.reader_markdown.is_empty());
    }

    #[test]
    fn succeeded_invalid_envelope_is_rejected_not_historical() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let malformed = add_run(
            &mut store,
            steps[2],
            None,
            "Succeeded",
            r#"{"status":"complete","report":"truncated"#,
        );
        let presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == malformed).unwrap());
        assert_eq!(presentation.quality, EvidenceQuality::Rejected);
        assert!(presentation.reader_markdown.is_empty());
        assert_eq!(presentation.kind, ResultKind::None);
        assert!(!presentation.has_final_report);
        assert!(!presentation.freshness.historical);
    }

    #[test]
    fn plain_markdown_starting_with_brace_is_not_a_report_envelope() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let text = "{not json} is a legitimate report heading, not an envelope";
        let run_id = add_run(&mut store, steps[2], None, "Succeeded", text);
        let presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == run_id).unwrap());
        assert_eq!(presentation.quality, EvidenceQuality::Complete);
        assert_eq!(presentation.kind, ResultKind::FinalReport);
        assert_eq!(presentation.reader_markdown, text);
        assert!(presentation.has_final_report);
    }

    #[test]
    fn changed_signature_marks_done_result_stale() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let report = add_run(&mut store, steps[2], None, "Succeeded", "Original final report");
        store.set_stage(steps[2], Stage::Done).unwrap();
        assert!(store.research_result(steps[2]).is_some());

        store.task_mut(steps[2]).unwrap().description.push_str(" changed scope");
        let presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == report).unwrap());
        assert!(presentation.freshness.stale_input);
        assert!(!presentation.has_final_report);
        // The old text remains inspectable, but it is not the current answer.
        assert!(!presentation.reader_markdown.is_empty());
        assert_eq!(presentation.quality, EvidenceQuality::UnverifiedLegacy);
    }

    #[test]
    fn legacy_plain_text_is_kept_but_not_blessed_as_final() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let report = add_run(&mut store, steps[2], None, "Succeeded", "Legacy markdown report");
        store.run_mut(report).unwrap().input_signature.clear();
        let presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == report).unwrap());
        assert_eq!(presentation.quality, EvidenceQuality::UnverifiedLegacy);
        assert!(!presentation.reader_markdown.is_empty());
        assert!(!presentation.has_final_report);
        assert!(presentation.freshness.legacy_contract);
    }

    #[test]
    fn multi_goal_partial_keeps_completed_report_and_evidence() {
        let (mut store, first_goal) = workspace();
        let first_steps = store.apply_research_plan(first_goal, PLAN).unwrap();
        add_run(&mut store, first_steps[0], Some(7), "Succeeded", "First evidence");
        add_run(&mut store, first_steps[1], Some(7), "Succeeded", "First analysis");
        let first_report = add_run(&mut store, first_steps[2], Some(7), "Succeeded", "First final report");

        let second_goal = store
            .create_research_goal(
                "Second goal".into(),
                "Second goal used to prove multi-goal aggregation.".into(),
                1,
            )
            .unwrap();
        let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
        add_run(&mut store, second_steps[0], Some(7), "Succeeded", "Second evidence");
        add_run(&mut store, second_steps[1], Some(7), "Succeeded", "Second analysis");

        let occurrence = occurrence(
            7,
            vec![first_goal, second_goal],
            OccurrenceStatus::Failed,
            Some(first_report),
            1,
        );
        let presentation = present_occurrence(&store, &occurrence);
        assert_eq!(presentation.kind, ResultKind::PartialPeriod);
        assert!(!presentation.has_final_report);
        assert!(presentation.reports.iter().any(|report| {
            report.presentation.has_final_report
                && report.presentation.reader_markdown == "First final report"
        }));
        assert!(presentation.reports.iter().any(|report| {
            report.presentation.kind == ResultKind::IntermediateEvidence
                && report.owner_label.contains("Second goal")
                && report.presentation.reader_markdown == "Second evidence"
        }));
        assert!(!presentation.reader_text().contains("{\"status\""));
    }

    #[test]
    fn all_goal_reports_required_for_complete_period() {
        let (mut store, first_goal) = workspace();
        let first_steps = store.apply_research_plan(first_goal, PLAN).unwrap();
        add_run(&mut store, first_steps[0], Some(8), "Succeeded", "First evidence");
        add_run(&mut store, first_steps[1], Some(8), "Succeeded", "First analysis");
        let first_report = add_run(&mut store, first_steps[2], Some(8), "Succeeded", "First final report");

        let second_goal = store
            .create_research_goal(
                "Second goal".into(),
                "Second goal used to prove multi-goal aggregation.".into(),
                1,
            )
            .unwrap();
        let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
        add_run(&mut store, second_steps[0], Some(8), "Succeeded", "Second evidence");
        add_run(&mut store, second_steps[1], Some(8), "Succeeded", "Second analysis");
        add_run(&mut store, second_steps[2], Some(8), "Succeeded", "Second final report");

        let occurrence = occurrence(
            8,
            vec![first_goal, second_goal],
            OccurrenceStatus::Succeeded,
            Some(first_report),
            1,
        );
        let presentation = present_occurrence(&store, &occurrence);
        assert_eq!(presentation.kind, ResultKind::FinalReport);
        assert!(presentation.has_final_report);
        assert_eq!(presentation.reports.len(), 2);
        assert_eq!(presentation.quality, EvidenceQuality::Complete);
    }

    #[test]
    fn partial_one_of_many_goals_is_not_complete_answer() {
        let (mut store, first_goal) = workspace();
        let first_steps = store.apply_research_plan(first_goal, PLAN).unwrap();
        add_run(&mut store, first_steps[0], Some(9), "Succeeded", "First evidence");
        add_run(&mut store, first_steps[1], Some(9), "Succeeded", "First analysis");
        let first_report = add_run(&mut store, first_steps[2], Some(9), "Succeeded", "First final report");

        let second_goal = store
            .create_research_goal(
                "Second goal".into(),
                "Second goal used to prove multi-goal aggregation.".into(),
                1,
            )
            .unwrap();
        let second_steps = store.apply_research_plan(second_goal, PLAN).unwrap();
        add_run(&mut store, second_steps[0], Some(9), "Succeeded", "Second evidence");

        let occurrence = occurrence(
            9,
            vec![first_goal, second_goal],
            OccurrenceStatus::Succeeded,
            Some(first_report),
            1,
        );
        let presentation = present_occurrence(&store, &occurrence);
        assert_eq!(presentation.kind, ResultKind::PartialPeriod);
        assert!(!presentation.has_final_report);
        assert_ne!(presentation.quality, EvidenceQuality::Complete);
    }

    #[test]
    fn failed_single_goal_keeps_completed_evidence_and_sources() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], Some(10), "Succeeded", "Collected evidence body");
        add_run(&mut store, steps[1], Some(10), "Succeeded", "Analyzed body");
        let occurrence = occurrence(10, vec![goal], OccurrenceStatus::Failed, None, 1);
        let presentation = present_occurrence(&store, &occurrence);
        assert_eq!(presentation.kind, ResultKind::IntermediateEvidence);
        assert!(!presentation.has_final_report);
        assert!(presentation.reports.iter().any(|report| {
            report.presentation.reader_markdown == "Collected evidence body"
                && report.presentation.sources.len() == 1
        }));
        assert!(!presentation.reader_text().contains("{\"status\""));
    }

    #[test]
    fn blocked_envelope_is_not_accepted_as_a_validated_report() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let blocked = add_run(
            &mut store,
            steps[2],
            None,
            "Blocked",
            r#"{"status":"blocked","report":"Blocked envelope body","sources":[]}"#,
        );
        let presentation =
            present_run(&store, store.runs().iter().find(|run| run.id == blocked).unwrap());
        assert_eq!(presentation.quality, EvidenceQuality::NotValidated);
        assert!(presentation.reader_markdown.is_empty());
        assert!(!presentation.has_final_report);
    }

    #[test]
    fn every_required_advise_output_is_needed_for_goal_completion() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], Some(13), "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], Some(13), "Succeeded", "Analyzed findings");
        let first_report = add_run(&mut store, steps[2], Some(13), "Succeeded", "First goal report");

        let second = store
            .create_task("Second required report", 1, Stage::Planned, Some(goal))
            .unwrap();
        let task = store.task_mut(second).unwrap();
        task.description = "A second required final output.".into();
        task.research = Some(crate::research::ResearchTask {
            requires_approval: false,
            kind: ResearchKind::Advise,
            goal_id: goal,
            depends_on: vec![steps[1]],
            space_name: "qa".into(),
        });

        let occurrence = occurrence(
            13,
            vec![goal],
            OccurrenceStatus::Succeeded,
            Some(first_report),
            1,
        );
        let before = present_occurrence(&store, &occurrence);
        assert!(!before.has_final_report);
        assert_eq!(before.kind, ResultKind::PartialPeriod);

        add_run(&mut store, second, Some(13), "Succeeded", "Second goal report");
        let after = present_occurrence(&store, &occurrence);
        assert!(after.has_final_report);
        assert_eq!(after.kind, ResultKind::FinalReport);
        assert_eq!(after.reports.len(), 2);
    }

    #[test]
    fn legacy_final_run_outside_advise_children_remains_inspectable() {
        let (mut store, goal) = workspace();
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], None, "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], None, "Succeeded", "Analyzed findings");
        let mut legacy = store.prepare_run(steps[2]).unwrap();
        legacy.task_id = goal;
        legacy.task_title = "Legacy goal title".into();
        legacy.automation_occurrence_id = Some(12);
        legacy.status = "Succeeded".into();
        legacy.output = "Legacy period report body".into();
        legacy.input_signature.clear();
        legacy.sources = vec![Source {
            title: "Legacy source".into(),
            url: "https://example.com/legacy".into(),
            published_at: "2026-01-01".into(),
        }];
        let legacy_id = legacy.id;
        store.push_run(legacy);
        let occurrence = occurrence(
            12,
            vec![goal],
            OccurrenceStatus::Succeeded,
            Some(legacy_id),
            1,
        );
        let presentation = present_occurrence(&store, &occurrence);
        assert!(!presentation.has_final_report);
        assert_eq!(presentation.quality, EvidenceQuality::UnverifiedLegacy);
        assert!(presentation.reports.iter().any(|report| {
            report.presentation.run_id == Some(legacy_id)
                && report.presentation.reader_markdown == "Legacy period report body"
                && report.presentation.sources.len() == 1
        }));
        assert_eq!(presentation.primary_run_id(), Some(legacy_id));
        assert_eq!(
            occurrence_report(&store, &occurrence).map(|run| run.id),
            Some(legacy_id)
        );
    }

    #[test]
    fn previous_version_regeneration_and_partial_flags_coexist() {
        let (mut store, goal) = workspace();
        add_definition(&mut store, 99, 2);
        let steps = store.apply_research_plan(goal, PLAN).unwrap();
        add_run(&mut store, steps[0], Some(11), "Succeeded", "Collected evidence");
        add_run(&mut store, steps[1], Some(11), "Succeeded", "Analyzed findings");
        let partial = add_run(
            &mut store,
            steps[2],
            Some(11),
            "Succeeded",
            &format!("{}\n\nPartial final report body", research::EVIDENCE_GAP_NOTE),
        );
        // A concurrent regeneration is active for the same final-report task.
        let regenerating = add_run(
            &mut store,
            steps[2],
            Some(11),
            "Running",
            "Rewriting the report from existing evidence",
        );
        assert_ne!(partial, regenerating);
        let occurrence = occurrence(
            11,
            vec![goal],
            OccurrenceStatus::Succeeded,
            Some(partial),
            1,
        );
        let presentation = present_occurrence(&store, &occurrence);
        assert_eq!(presentation.quality, EvidenceQuality::Partial);
        assert!(presentation.has_final_report);
        assert!(presentation.freshness.previous_version);
        assert!(presentation.freshness.regenerating);
        assert!(presentation.notice.is_some());
        assert!(presentation.reader_markdown.contains("Partial final report body"));
        assert!(!presentation.reader_markdown.contains(research::EVIDENCE_GAP_NOTE));
        assert_eq!(presentation.progress.len(), 1);
    }
}
