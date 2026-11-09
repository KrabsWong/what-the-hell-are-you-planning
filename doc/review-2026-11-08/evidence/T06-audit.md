# T06 audit — typed result presentation and reader consistency

Date: 2026-11-08
Task: [T06](../../doc/review-2026-11-08/TASKS.md#t06--introduce-typed-result-presentation)
Mode: read-only bounded audit. No code, workspace, fixture or application state was modified.
Related findings: F05, F06. Related tasks: T05 (eligibility), T23/T25 (planning ownership and replan), T01 (QA baseline), T16 (activity order).

## 1. Outcome in one paragraph

The app currently has two independent result paths. Ordinary Research filters a run through `readable`, strips the evidence-gap marker and shows a notice, but it still copies the raw stored output. Automation renders `occurrence_report(...)` directly, where the fallback is "latest non-empty run", not "validated final report"; it never strips the marker, never shows the quality notice, and can render live commentary, intermediate evidence or raw unvalidated JSON as the period answer. A single derived presentation model can remove all of these mismatches without changing scheduling, activity order, navigation or storage schema. T06 should own that model and its reader transformation; it should not own run/request selection (T23/T25), eligibility reasons (T05), or header/navigation work (T01/T12).

## 2. Current result-selection and rendering paths

### 2.1 Ordinary Research

- Selection for the results page: `src/run_views.rs:186-204`. It takes `selected_run` when it is not archived and not automation-owned, otherwise the newest such run. `run_is_automation` delegates to `Store::run_is_automation_owned` (`src/run_views.rs:86-88`).
- Readability gate: `src/run_views.rs:652-670`.
  - `planning` covers project plans and `ResearchKind::Goal` runs.
  - `readable = !planning && (status == "Succeeded" || (status == "Blocked" && !browser_blocked))`.
  - While running/queued/recovering/failed/cancelled the result panel is replaced by `run-waiting` (`src/run_views.rs:1112-1143`).
- Result rendering: `TaskApp::run_result_panel` in `src/research_views.rs:503-583`.
  - `has_evidence_gap_note` and `reader_markdown` are applied at `src/research_views.rs:505-506`.
  - A warning notice is inserted at `src/research_views.rs:532-539` using an app-owned non-English heading and `research::EVIDENCE_GAP_NOTICE`.
  - The historical-result label is derived by comparing `store.research_result(task_id)` with the displayed run id at `src/research_views.rs:513`. This is the only freshness check on a report surface.
- Copy: `src/run_views.rs:893,919-923` copies `run.output` verbatim, so the internal evidence-gap marker is copied even though it is hidden on screen.
- Technical copy: `src/run_views.rs:1228-1236` deliberately copies logs and raw output. This is the correct place for raw JSON and should remain raw and explicitly labelled "Saved output".
- Export: `src/research_export.rs:100-105` applies `reader_markdown` and prepends its own non-English notice; `src/research_export.rs:107` uses a bilingual generated heading; `src/research_export.rs:113-120` appends sources.
- Publication: `src/report_publish.rs:127-141` appends raw `run.output` for final reports and a non-English generated references heading. `src/report_html.rs:111-122` converts the internal marker into a public notice with a non-English aria-label/heading; `src/report_html.rs:243-255` asserts that non-English notice.

### 2.2 Automation periods

- Selection: `occurrence_report` in `src/automation_views.rs:1730-1752`.
  1. If `final_run_id` points to an existing run, return it unconditionally.
  2. Otherwise return the highest-id run owned by the occurrence with non-empty output.
  3. Otherwise return the highest-id owned run.
- Period dialog: `automation_result_dialog` in `src/automation_views.rs:2956-3022`; body is `automation_occurrence_detail`.
- Period detail: `src/automation_views.rs:2622-2762`.
  - Report text is rendered from `run.output.clone()` with no reader transformation at `src/automation_views.rs:2680-2695`.
  - Sources are rendered at `src/automation_views.rs:2697-2709`.
  - Step list and retry affordances follow at `src/automation_views.rs:2711-2750`.
- Result row: `automation_occurrence_row` in `src/automation_views.rs:2542-2670` shows only `occurrence.status.label()` plus activity kind/time. It has no quality, freshness or "no final report" dimension.
- Automation list summary: `src/automation_views.rs:1855-1892` uses `latest_occurrence` and shows `status · activity kind · time`; no final-report quality.
- Result-dialog header: `src/automation_views.rs:2976-2981` shows `Period N · occurrence status` only.
- Regeneration: `src/automation_runtime.rs:2112-2126` swaps `final_run_id` only after a successful Advise regeneration. Before that, the dialog continues to show the old report with no "Regenerating" indication.
- Completion: `Store::complete_occurrence` in `src/automation_runtime.rs:409-467` requires every goal, every step and every Advise result to be `Succeeded` and non-empty, and records `final_run_id` from the latest report run. This validates final success, but the presentation layer does not reuse the same evidence.
- Validation failure path: `src/research_app.rs:613-622` sets a run to `Failed` on report-validation error but leaves the raw unvalidated `run.output` in place. That is the source of the raw-JSON leak described below.
- Partial path: `src/research_app.rs:590-610` replaces `run.output` with the parsed report and prepends `EVIDENCE_GAP_NOTE` for `partial`. There is no persisted explicit quality field; quality is inferred from the marker plus run status.
- Failed period: `fail_occurrence` in `src/automation_runtime.rs:469-486` keeps prior runs. The detail therefore still has finished evidence available, but `occurrence_report` does not say which run it chose or whether it is a final report.
- No automation copy/export entry is currently exposed (`src/automation_views.rs:1615-1630,2188` and DESIGN "Copy and Archive are not exposed yet"). T06 should not add a new automation export entry point in this wave; it should make the ordinary copy/export and any future automation surface use one transformation.

## 3. Concrete current failures

Each item names the reproduction shape and the exact code path. These are presentation/selection defects; none is a scheduling or execution failure.

### F-A. Raw unvalidated JSON appears as the automation period's answer

- Path: validation fails in `src/research_app.rs:613-622`; run becomes `Failed` but `run.output` still holds the agent's raw JSON/protocol text.
- `occurrence_report` at `src/automation_views.rs:1739-1751` accepts any non-empty owned run, including this failed run.
- `automation_occurrence_detail` at `src/automation_views.rs:2680-2695` passes that text straight to Markdown.
- Result: a period can show `{"status":"complete",...}` (or a truncated JSON block) as report prose, violating "Raw JSON ... never appear as report prose" and "A successful process exit without a validated report is not presented as a complete answer".
- Quick and multi-goal Research automations both reach this path; multi-goal additionally has several step outputs eligible for the fallback.

### F-B. Live commentary impersonates the final answer (F05)

- While a run is `Running`, streaming `Event::Text` updates `run.output` (`src/app.rs:2085-2087`).
- `occurrence_report` picks the latest non-empty run regardless of `run.status` (`src/automation_views.rs:1739-1751`).
- The detail panel renders it in the same main report position as a final report, above the step list (`src/automation_views.rs:2676-2696`).
- Observed baseline behaviour matches ISSUES F05: an in-progress collection sentence appears as the period answer with no "Live progress" classification.

### F-C. Intermediate evidence is presented as the period result

- If a period has finished collection/analysis but no validated Advise report, `occurrence_report` returns the latest successful intermediate run.
- The detail displays it as the report body. The step list below still exists, but the body is not labelled "Intermediate evidence · step title" and has no "No final report yet" statement.
- Acceptance requires explicit "No final report yet" plus progress/step results in this state.

### F-D. Evidence gaps are invisible at the entry point and the internal marker leaks in automation

- Partial quality is encoded only by `EVIDENCE_GAP_NOTE` at the start of `run.output` (`src/research.rs:195-209`).
- Automation detail never calls `has_evidence_gap_note` or `reader_markdown`, so the marker can render as a blockquote containing internal downstream instructions. This is F06.
- The result row (`src/automation_views.rs:2542-2670`) and list summary (`src/automation_views.rs:1855-1892`) show `Succeeded` with no quality label. This is the native "Quick Period 4 returns partial; its dialog and list say Succeeded" observation in ISSUES F06.
- Ordinary Research strips the marker and shows a notice, so the two domains disagree.

### F-E. Freshness and version context are incomplete on automation results

- Ordinary Research labels a run "Historical result" when `store.research_result(task_id)` no longer points to it (`src/research_views.rs:513`).
- Automation detail only warns when `definition.version != occurrence.definition_version` (`src/automation_views.rs:2646-2657`). It does not label a chosen step result as current/historical/stale, and it does not verify that `final_run_id` still refers to a validated final report for the occurrence snapshot.
- A report whose step inputs changed after completion can therefore be displayed as the period answer with no stale/historical label.

### F-F. Regenerating state is indistinguishable from a completed answer

- `final_run_id` is replaced only on successful regeneration (`src/automation_runtime.rs:2112-2126`).
- During an active regeneration the period dialog shows the old report and `Succeeded` with no "Regenerating · showing previous version" state.
- Ordinary Research has regeneration UI (`src/run_views.rs:857-869,954-975`) but no presentation-level "regenerating" label on the old result.

### F-G. Copy, export, render and publish use different reader transformations

- Screen (ordinary): `reader_markdown` plus notice (`src/research_views.rs:505-539`).
- Copy (ordinary): raw `run.output` including the marker (`src/run_views.rs:893,921-923`).
- Export: `reader_markdown` plus a separately formatted notice (`src/research_export.rs:100-105`).
- Publish: raw `run.output` (`src/report_publish.rs:133`), then marker-to-HTML conversion in `src/report_html.rs:111-127`.
- Result: the same partial report has four different reader texts. Acceptance explicitly requires copy/export/render to share the reader transformation.

### F-H. App-owned reader strings violate the English-only repository rule

- Non-English app heading and notice: `src/research_views.rs:535-537`.
- Non-English export notice and bilingual generated heading: `src/research_export.rs:104,107`.
- Non-English public notice markup and aria-label: `src/report_html.rs:119`.
- Non-English generated references heading in publish content: `src/report_publish.rs:134`.
- User-authored/generated report bodies in Chinese must be preserved; only app-owned headings/notices change. T06 acceptance explicitly calls for "a shared English UI notice for evidence gaps".

### F-I. Failed/cancelled periods do not state what is missing

- `occurrence_report` can return a failed or cancelled run's raw output; the detail then presents whatever text exists without "No final report" or "This attempt did not produce a validated report".
- Finished step evidence remains reachable only through the step list; the body does not say that the period failed before a final report existed.
- This affects the required failed/cancelled matrix for Quick and multi-goal Research.

### F-J. Ordinary Research has no typed final-report distinction for non-research runs

- `run_content` treats any non-planning `Succeeded` run as readable (`src/run_views.rs:667-670`).
- For a run with `research_kind == None`, `run.output` is raw agent text rather than a validated research report.
- This may be acceptable for ordinary task execution, but T06 must state whether the "validated final report" contract applies only to `ResearchKind::Advise`/`Collect`/`Analyze`. Bounded follow-up check required; do not change task semantics in this task.

## 4. Proposed typed presentation selector

### 4.1 New module and ownership

Proposed new file: `src/result_presentation.rs`.
Owner: T06. No persisted schema change in this wave; derive everything from existing `Store`, `AgentRun`, `AutomationOccurrence` and `AutomationDefinition`.

Suggested public surface:

```rust
pub enum ResultKind {
    FinalReport,
    IntermediateEvidence,
    LiveProgress,
    PlanningAttempt, // routing/plan output, never a research answer
    None,
}

pub enum AttemptStatus {
    Queued, Running, Recovering, Stopping, Succeeded,
    Failed, Blocked, Stale, Cancelled, Interrupted, Unknown,
}

pub enum Freshness {
    Current,
    Historical,        // a validated but superseded result
    StaleInput,        // inputs changed; rerun required
    PreviousVersion,   // occurrence/definition version differs
    Regenerating,      // a newer report attempt is active
    Unknown,
}

pub enum EvidenceQuality {
    Complete,
    Partial,           // validated with limitations
    Rejected,          // process succeeded but report did not validate
    NotValidated,      // live/queued/intermediate text
    None,
}

pub struct ResultPresentation {
    pub kind: ResultKind,
    pub attempt: AttemptStatus,
    pub freshness: Freshness,
    pub quality: EvidenceQuality,
    pub run_id: Option<u64>,
    pub step_label: Option<String>,
    pub reader_markdown: String,
    pub sources: Vec<crate::research::Source>,
    pub notice: Option<()>, // shared English evidence-gap notice when Partial
    pub progress_text: Option<String>, // live/streaming content, separate section
    pub has_final_report: bool,
}
```

If the project prefers no new module in this wave, the same types can live at the bottom of `src/research.rs` and be re-exported, but a separate module keeps T06 from growing the 1,783-line research file and makes ownership explicit.

### 4.2 Selection rules (must be deterministic and shared)

1. Planning runs (`project_plan.is_some()` or `research_kind == Goal`) are `PlanningAttempt`; their output is never `reader_markdown` on a result surface. `planning_attempt_dialog` stays as-is.
2. Ordinary Research current result is exactly `store.research_result(task_id)`. If that is `Some(run)` and it matches the displayed run, `kind = FinalReport` (or `IntermediateEvidence` for Collect/Analyze), `freshness = Current`, `quality = Complete|Partial` from the marker.
3. Ordinary run that is `Succeeded` but not the current `research_result` is `Historical`, never `Current`.
4. Automation final report requires all of:
   - `occurrence.final_run_id == Some(run.id)`;
   - `run.automation_occurrence_id == Some(occurrence.id)`;
   - `run.status == "Succeeded"`;
   - `run.research_kind == Some(ResearchKind::Advise)`;
   - `!run.output.trim().is_empty()`;
   - the run is a validated result for its task (`store.research_result(run.task_id)` matches it) **or** the occurrence is terminal and the result is explicitly labelled historical.
5. Automation intermediate evidence is an owned run with `Succeeded`, non-empty output, and `research_kind` in `Collect|Analyze`; label it with the step title and `IntermediateEvidence`.
6. Automation live progress is an owned run whose status is `Queued|Running|Recovering|Stopping`; its text goes to `progress_text`, never `reader_markdown`. The main body shows "No final report yet" plus progress.
7. If no final or intermediate run exists, `kind = None`, quality `None`, and the body shows "No final report yet" plus the step list.
8. If `final_run_id` exists but the run is failed/cancelled/empty/not Advise, do not use it. Fall back to rule 5/6/7 and mark the period `quality = Rejected` when the run reached report validation and failed.
9. `reader_markdown` is `research::reader_markdown(output)` for validated results only. Never apply it to live progress or rejected raw output. Never return output that starts with a JSON object or JSON fence for a final/evidence kind.
10. Quality mapping:
    - `has_evidence_gap_note(output)` → `Partial`;
    - validated run without marker → `Complete`;
    - validation failure recorded in logs / run status Failed on an Advise attempt → `Rejected`;
    - active live text → `NotValidated`.
11. Freshness:
    - current result id matches `research_result` → `Current`;
    - a valid result exists but differs → `Historical`;
    - `store.research_result(task_id).is_none()` and the run is `Succeeded` → `StaleInput` (or `Historical` if an explicit previous-version label is available);
    - `definition.version != occurrence.definition_version` → `PreviousVersion` with the existing version notice;
    - an active/queued regeneration run for the same occurrence exists → `Regenerating` and keep the previous final report visible with that label.

### 4.3 Shared reader transformation

- Move the evidence-gap constants and transformation into one place used by all surfaces:
  - `EVIDENCE_GAP_NOTE` stays as the machine marker in stored output.
  - `EVIDENCE_GAP_NOTICE` becomes an English app-owned sentence, for example: `This report has evidence gaps. Read the limitations and unverified items in the body before relying on its conclusions.`
  - Add `pub fn reader_notice(quality: EvidenceQuality) -> Option<&'static str>` or equivalent, so view/export/publish cannot drift.
- Provide one `to_reader_markdown` / `reader_body` on `ResultPresentation`.
- Copy uses `presentation.reader_markdown` and appends the same notice text where the surface includes the notice.
- Export uses the same presentation and the same notice; do not reimplement marker stripping.
- Publish uses the same reader markdown; `report_html` converts the shared English notice into the existing `<aside class="evidence-notice">` instead of translating the machine marker. This also fixes the non-English publish notice and generated headings.
- Technical details remain the only surface allowed to show raw `run.output`; keep the "Saved output" label.
- Do not change stored report text or user/generated report language.

### 4.4 Presentation rules per state

| State | Kind | Quality | Body | Notice / label |
| --- | --- | --- | --- | --- |
| Quick/Running | LiveProgress | NotValidated | "No final report yet" + progress section | "Live progress from step X. Not a validated answer." |
| Quick partial | FinalReport | Partial | reader markdown | English evidence-gap notice + "Succeeded with evidence gaps" |
| Quick failed before report | None or IntermediateEvidence | Rejected / None | "No final report yet" + completed step results | period status + failure reason |
| Quick cancelled | None or IntermediateEvidence | None | "No final report yet" + completed step results | "Stopped before a final report" |
| Quick regenerating | FinalReport | Complete/Partial | previous reader markdown | "Regenerating · showing previous version" |
| Multi-goal one goal final, one running | FinalReport for one goal | Complete/Partial | per-goal labels; do not merge goals into one anonymous body | goal/step ownership visible |
| Multi-goal failed | None/Intermediate | Rejected | completed goal reports labelled by goal, not "the answer" | failed step only; downstream not presented as blocked |
| Ordinary running | LiveProgress | NotValidated | existing waiting/progress surface | no final report claim |
| Ordinary succeeded current | FinalReport | Complete/Partial | reader markdown | historical/current label, evidence notice |
| Ordinary succeeded historical | FinalReport | Complete/Partial | reader markdown | "Historical result · task context or dependency results have changed." (existing wording preserved) |
| Ordinary failed | None | Rejected/None | technical details only; no report body | "This attempt did not finish" |
| Planning attempt | PlanningAttempt | None | planning dialog only | "plan history, not a research report" |

## 5. Implementation boundaries (proposed ownership)

### T06 owns

- New `src/result_presentation.rs` (types, selection rules, reader transformation, unit tests).
- `src/research.rs`: shared English notice constant and any helper signatures; preserve `EVIDENCE_GAP_NOTE`, `reader_markdown`, `has_evidence_gap_note` compatibility for existing callers.
- `src/automation_views.rs`: replace `occurrence_report` with the selector at the detail call site; add quality/freshness/no-final labels to `automation_occurrence_detail`, `automation_result_dialog` header, `automation_occurrence_row` and `automation_list` summary. Do not change retry/continue actions, step ordering, activity ordering or row geometry beyond one bounded label.
- `src/research_views.rs`: route `run_result_panel` through the selector; remove the duplicated evidence-gap branch; use the shared English notice. Preserve the existing "Historical result" wording and source disclosure.
- `src/run_views.rs`: `Copy result` uses `reader_markdown`; add final/intermediate/progress labels in `run_content` only where the selector already requires it. Do not change run selection, navigation, queue text, retry UI or technical panel semantics.
- `src/research_export.rs`: consume the selector; use the shared notice.
- `src/report_publish.rs` and `src/report_html.rs`: consume the selector for final report markdown; replace marker translation with the shared notice. Preserve published URL, fingerprint, commit and verification behavior.
- Tests and QA: `src/automation_qa.rs`, `src/research_qa.rs`, `src/research_export.rs` tests, `src/report_html.rs` tests, new unit tests in `src/result_presentation.rs`.

### T06 does not own

- Run/request ownership and selection policy: T23/T25 (`Store::run_is_automation_owned`, `run_views` selection, breadcrumbs/counts). T06 may consume their chosen id but must not add a second selection policy.
- Replanning and request revision binding: T25.
- Shared execution eligibility, phase barriers, "Waiting for collection", queue reasons: T05. T06 presents the step label chosen by T05 and must not recompute readiness.
- Planning-UI ownership, planning attempt selection and planning dialogs: T23/T25.
- Navigation header, baseline QA/navigation fixes: T01/T12.
- Activity ordering: T16. Use `occurrence_activity`/`compare_newest_first` unchanged.
- Icon semantics and control factories: T11. Use existing `chip`, `notice`, `label`, `muted` and Lucide assets only; no new icon system.
- Archive lifecycle semantics: T22. T06 may read archived flags for freshness only; do not change eligibility.
- New automation copy/export controls: not in this wave; DESIGN says they are not exposed.

### Files that should not be touched by T06

`src/automation_activity.rs`, `src/automation_schedule.rs`, `src/automation_runtime.rs` scheduling/cursor paths, `src/navigation.rs`, `src/scheduler.rs`, persistence schema/migrations, `src/app.rs` route writes, `src/domain.rs` struct fields (unless a strictly derived helper is added; avoid schema changes).

## 6. Test matrix

### 6.1 Unit tests for the selector (new `result_presentation` tests)

Use plain `Store`/`AgentRun` fixtures; no GPUI.

1. `final_report_wins_over_later_intermediate_run` — `final_run_id` points to Advise, a later Collect run exists; selector returns the Advise run.
2. `missing_final_uses_latest_validated_step_and_says_no_final` — no final id, one Succeeded Collect; kind Intermediate, has_final false, body labelled with step.
3. `live_run_text_is_progress_not_reader_markdown` — Running run with non-empty output; kind LiveProgress, reader empty, progress non-empty.
4. `validation_failure_never_returns_raw_json` — Failed run output is `{"status":"complete",...}`; selector returns no reader body, quality Rejected.
5. `partial_marker_is_stripped_and_quality_partial` — marker + report; body excludes marker, quality Partial, notice present exactly once.
6. `internal_marker_never_in_reader_body` — same as above for every ResultKind path.
7. `historical_run_is_not_current` — `research_result(task)` returns a newer run; displayed older run freshness Historical.
8. `stale_input_is_not_current` — current signature mismatch; freshness StaleInput and no final claim.
9. `definition_version_change_is_previous_version` — occurrence v2, definition v3; freshness PreviousVersion and existing version notice text retained.
10. `active_regeneration_keeps_previous_report_with_label` — active Advise regeneration run; previous final body remains, freshness Regenerating.
11. `planning_run_is_never_a_report` — `research_kind == Goal` or project plan with JSON output; kind PlanningAttempt, no reader body.
12. `sources_are_preserved_for_every_final_and_intermediate_result` — sources returned unchanged; no synthetic sources.
13. `empty_output_is_no_report_not_empty_markdown` — Succeeded but empty; has_final false.
14. `failed_period_keeps_finished_step_results` — Failed occurrence with one Succeeded Collect; selector exposes it as Intermediate while the period body says no final report.

### 6.2 Export/copy/publish consistency tests

15. `copy_result_matches_reader_body` — the clipboard string equals `presentation.reader_markdown` (plus same notice placement), not raw output.
16. `export_markdown_strips_marker_and_uses_shared_notice` — exported file contains the English notice and never the marker; Chinese report body byte-for-byte retained.
17. `publish_markdown_uses_shared_notice` — publication markdown/HTML contains one English `<aside>` notice, no marker, and preserves the report body.
18. `report_html_still_normalizes_repeated_title` — existing `report_html` heading test continues to pass with the English notice (update only the expected notice text).
19. `raw_output_remains_available_in_technical_details` — no assertion removes `Copy output`/`Saved output`.

### 6.3 Isolated UI QA (`automation_qa`, `research_qa`)

Use fake/offline fixtures only; no live user workspace.

20. Quick running: open period dialog while collection streams; body shows "No final report yet" and a progress section; no streaming text in the final-report position.
21. Quick partial: period row and dialog both say Succeeded with evidence gaps; notice is English; marker absent; step success check retained.
22. Quick complete: final report shown; no notice.
23. Quick failed after report validation: raw JSON not shown; "No final report yet" plus completed steps; technical details still contain raw output.
24. Quick cancelled: same as failed without failure reason wording; finished steps inspectable.
25. Multi-goal partial: one goal final complete, another goal still running; each goal's body is labelled by goal; no anonymous merged answer.
26. Multi-goal failed: one goal has a completed report, another failed; completed report remains inspectable with its goal/step label; failed step only is Blocked (T05 owns eligibility text; verify no regression).
27. Regenerating: trigger report regeneration; old report remains visible with "Regenerating · showing previous version"; on success the new report replaces it and the label clears.
28. Stale/current: change task inputs after completion; period result is not presented as current; ordinary Research historical label still appears.
29. Ordinary Research running/failed: result panel does not claim a final answer; technical details still expose logs/output.
30. Copy/export parity: copy and export for the same partial detail produce the same reader body and notice.
31. Result row quality: Automation results row for a partial period shows a quality label without changing activity order (T16 frozen). Assert row activity text and order are unchanged.
32. Empty state: period with no runs shows "No final report yet" and step list if present, not a blank panel.

### 6.4 Regression guards

33. `cargo test --quiet` after adding selector tests.
34. `python3 script/check_icons.py` (no new icon expected; catches accidental asset changes).
35. Relevant isolated UI QA commands only; do not restart the user's running app and do not touch the active workspace.
36. Verify no new route, selection, schedule cursor or occurrence mutation from presentation code.

## 7. Remaining unknowns / bounded follow-up checks

These are the only open questions that could change implementation details. They are bounded to one focused pass each and do not justify broad reading now.

1. Confirm whether `final_run_id` can ever point to a non-Advise run after recovery/regeneration. If yes, rule 4 must also verify `research_kind == Advise`. Current evidence: `complete_occurrence` sets it from an Advise report; regeneration checks `research_kind == Advise`.
2. Confirm `store.research_result(task_id)` semantics for automation-owned steps after definition-version changes or archive. If it returns `None` for valid historical period results, freshness must use the occurrence snapshot rather than treating them as invalid. This is the main risk for rule 10/11.
3. Locate every remaining `run.output` read that renders a reader-facing report (beyond the files listed here). The earlier grep found the known call sites; a final `rg 'run\.output|\.output\.clone'` pass at implementation time is required to avoid a missed surface.
4. Confirm whether any ordinary non-research `Succeeded` run is intended to display raw text as a "result". If not, T06 should scope `FinalReport` to research kinds; this is a product-boundary check, not a broad refactor.
5. Confirm the publishing path is ordinary Research only. PRODUCT says automation results do not enter global publishing, so T06 should not add automation publishing; verify no automation run can reach `topic_share_content`.
6. Confirm the accepted English wording with the product owner. Proposed: "This report has evidence gaps. Read the limitations and unverified items in the body before relying on its conclusions." Do not translate user report bodies.
7. Confirm whether the marker can appear not at byte zero after recovery/import/export. `has_evidence_gap_note` uses `starts_with`; if any path prepends text, the selector needs a bounded detection rule. Do not broaden to arbitrary substring stripping without evidence.
8. Confirm T01 baseline QA status before running UI acceptance, so a T06 failure is not confused with an existing fixture/selector failure.
9. Confirm whether the automation results row has space for a quality label at 1000×680 with long names. T11 owns control styling; T06 should use a compact text suffix or existing chip and verify no clipping.
10. Confirm whether "No final report yet" belongs in the period dialog body only or also in the list row summary. Proposed: body explicit; row uses a short quality/status suffix, no long sentence.

## 8. Suggested implementation order (for the owning agent)

1. Add `result_presentation.rs` with types, rules and unit tests (no UI changes).
2. Move the shared English notice and reader helper; keep compatibility wrappers.
3. Switch `research_views::run_result_panel` to the selector; verify ordinary states.
4. Switch `automation_views::automation_occurrence_detail` and the dialog header to the selector; remove the raw `run.output` render.
5. Add bounded row/list quality and no-final labels.
6. Switch `run_views` Copy result to `reader_markdown`.
7. Switch export and publish/report_html to the shared notice; update the one expected-string test.
8. Run unit tests, icon check and isolated UI QA; record screenshots/evidence beside T06.

No code was modified and no commit was made. Awaiting implementation ownership confirmation.
