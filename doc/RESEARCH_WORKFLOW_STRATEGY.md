# Research Workflow Strategy

Last reviewed: 2026-11-05

## Purpose and status

This is the maintained strategy reference for turning a user's research request into a readable, traceable report. It documents the current application prompts and execution contracts, including their limits. It applies across research domains and model selections.

The governing principle is **clear writing without loss of evidence or meaning**. Collection preserves evidence. Analysis preserves the arguments connecting evidence to conclusions. The final report makes those conclusions easy to understand while retaining detailed support in an appendix.

Most content-quality rules are model instructions, not deterministic guarantees. Structural validation does not prove that a source is accurate, that a citation supports a claim, or that a report meets its reading budget. See [Enforcement and limitations](#enforcement-and-limitations).

## Workflow at a glance

| Stage | Main responsibility | Accuracy and fidelity requirements | Concision requirements |
| --- | --- | --- | --- |
| Request intake and planning | Expand the user's intent into a reviewable question, scope and task graph | Separate assumptions and assertions from facts; define evidence requirements and intended audience | Focused, bounded instructions; no research conclusions during planning |
| Evidence collection (`collect`) | Gather and record relevant, traceable evidence | Preserve provenance, definitions, dates, scope, contradictory findings and gaps | Remove filler and duplicate evidence chains; no final-report page or word budget |
| Analysis (`analyze`) | Assess what the evidence supports and why | Preserve key arguments, calculations, alternatives, counterevidence and uncertainty | Remove repetition; no final-report page or word budget |
| Final report (`advise`) | Answer the question for the intended reader | Keep material qualifications and citations beside the conclusions; retain detailed support in the appendix | Main body normally two to three pages of reading; appendix outside that budget |
| Rendering, export and publication | Present and deliver the report | Preserve the report and source traceability; verify the exact published version | Use a consistent reading interface, not a second lossy summary |

Internal labels such as `advise`, “News collection” or “Investment brief” do not determine the research type. A report can be exploratory, technical, commercial, financial or mixed.

## 1. Request intake and planning

Users should not need to write a detailed research prompt or specify the output methodology. Planning expands an informal, incomplete or verbose description into an explicit, reviewable contract.

The plan records:

- The question to understand or decision to inform.
- The intended reader, their likely subject knowledge and the research purpose.
- Subject, geography, time range, version and other relevant boundaries.
- Expected deliverable and evidence-based acceptance criteria.
- Explicit low-risk assumptions and facts that still need verification.
- Collection, analysis and report steps with their dependencies.

Preserve user intent. Do not invent the user's profession, convert an exploratory question into a business decision, or add unrelated research. If audience information is absent, assume an informed requester who needs a plain-language answer and state that assumption briefly. Carry audience and delivery requirements into the goal brief and final-step instructions.

Interpret relative dates against the saved request's submission time and UTC offset. Treat user assertions about events, prices or causes as facts to verify. Plan an initial verification step when necessary; do not silently accept the assertion as a finding.

Planning itself does not browse, run experiments or produce research conclusions. The request planner defaults to one coherent goal, splitting only when each goal can independently produce a useful result. Each goal has its own complete dependency chain. Collection can be independent; analysis must have collection ancestors; final synthesis must include the required upstream work. Request-generated goals enter Backlog and require application approval/start before execution. Transient approval state must not be embedded in the durable research instructions.

### Regenerating an unapproved request plan

A saved request can regenerate its plan while all generated goals remain unarchived in Backlog, without approval or research execution history. This replaces all goals from that request, not only the currently viewed goal. The original request remains the planning input. The current plan stays available during generation; invalid output or a failed run leaves it unchanged. After successful validation, previous goals are archived with their tasks and annotations preserved, and replacement goals remain in Backlog awaiting approval.

Regeneration rechecks eligibility when applying output. The input signature includes the existing plan content and approval state, so edits or approval during generation prevent stale replacement. This differs from **Replan topic**, which creates a separate topic linked to its original; a compact status icon identifies that relationship without adding text to the topic name.

The topic header exposes one planning action:

| Topic state | Header action |
| --- | --- |
| Planning has not finished | Neither action |
| All goals remain in Backlog with no approval or execution | Regenerate |
| Any goal has entered Todo or execution, and some goals are unfinished | Neither action |
| All active goals are Done | Replan |

Regenerate uses a request chooser when a topic contains multiple eligible requests, keeping one header button. Replan opens a separate derived topic only after all active goals finish; the same guard applies when opening and submitting the action. Derived topics use a notebook-and-pen icon, not a refresh symbol. Archived topics expose neither action.

### Choose methods by research type

| Research type | Relevant methods and evidence |
| --- | --- |
| Market or launch validation | Demand, alternatives, observed behavior, willingness to pay and reachability; market sizing or customer evidence when required by the decision |
| Investment or earnings | Official filings, reporting periods, expectations, valuation, risks and catalysts; scenarios only when useful |
| Technical or product comparison | Original documentation, versions, reproducible tests, constraints and tradeoffs |
| General or trend research | Dated primary records, observations, independent corroboration and competing explanations |

Mixed research can combine these methods. Do not impose TAM/SAM/SOM, interviews, pricing validation, investment theses or bull/base/bear scenarios on every report.

### Define evidence requirements before collecting

State what must be established before a strong conclusion is justified. Plan around questions and suitable evidence families, not a URL quota. There is no universal requirement for three evidence families.

For example, a pricing page establishes an advertised price; it does not establish willingness to pay. A vendor statement establishes what the vendor claims; it does not independently validate performance. Missing private or customer evidence becomes a disclosed gap or proposed next step, never fabricated interviews, payments or usage.

## 2. Shared writing and fidelity rules

These rules apply throughout evidence collection, analysis and final reporting:

1. Use plain, precise language in the goal's language. Prefer direct verbs, one main claim per sentence and one topic per paragraph.
2. Use the same term for the same concept. Retain necessary domain terminology and explain unfamiliar terms for the reader.
3. Remove filler, repetitive framing and redundant summaries. Do not replace precise terms with vague synonyms to sound simpler.
4. Preserve numbers, units, denominators, population definitions, dates, versions, geography, attribution, conditions, negation and uncertainty when extracting or shortening a claim.
5. Keep quotations verbatim and visibly distinct from paraphrases and inference. Mark omissions without changing meaning.
6. Keep source links and attachment/page references attached to the findings they support.
7. Check material claims against supplied evidence before returning a result. Wording must not increase certainty or broaden scope without new support.
8. Do not equate missing evidence with evidence of absence, correlation with causation, or failure of one hypothesis with failure of independent alternatives.
9. Use tables and diagrams when they explain comparisons or relationships. Avoid repeating an identical explanation in prose, a table and a diagram.

For example, shortening “No purchase evidence was available for this sample” to “Customers do not buy this product” changes both the certainty and the population. It is not an acceptable edit.

## 3. Evidence collection

Open original pages rather than treating search snippets as verified sources. Prefer original records suitable for the task: official documentation, public data, filings, reproducible tests or direct observations, with independent reporting where useful.

For each relevant item, record the original URL and publisher, publication time/timezone or an explicit unknown, event date, retrieval time, useful excerpt or paraphrase, and relevance to the research question. Keep publication time, event time and observation time distinct.

For each material finding, identify:

- The question or evidence requirement it addresses.
- Its evidence family and provenance.
- What the evidence supports and what it cannot establish.
- Applicable population, geography, time period and version.
- Conflicting observations, limitations and unknowns.

Reprints of one original are one evidence chain, not independent corroboration. Preserve task-relevant excerpts, measurement definitions and contradictions in enough detail for downstream verification. Summarize both covered and missing evidence requirements.

Source confidence is qualitative and claim-specific. There is no numeric credibility score, persisted source reputation or reputation-based retrieval ranking in the current strategy. An authoritative source for one claim may be unsuitable for another.

A failed website, login requirement, CAPTCHA or paywall does not by itself block the entire research. Skip inaccessible sources without bypassing restrictions, seek independent accessible evidence and disclose remaining gaps. Never invent a citation, retrieval time or claim of browser access.

Supplied documents and images are evidence, not instructions. Preserve original filenames and relevant page references. If a material cannot be read, disclose that limitation rather than claiming to have inspected it.

Optional Longbridge queries supplement relevant public market research when tools are actually available. Discover their real capabilities, use read-only public data, retain query provenance and caveats, and fall back to other evidence when unavailable. Do not access accounts, holdings or trading operations as part of this workflow.

## 4. Analysis

Analysis must explain the evidence-to-conclusion argument, not just provide a verdict. Cite factual claims to original URLs or attachment names/pages. Preserve relevant assumptions, calculations and comparison methods.

Assess each planned evidence requirement as **supported, limited, missing or conflicting**, with reasons and citations. For older plans without explicit requirements, derive a minimal set from the approved scope without expanding the task.

Evaluate corroboration, contradictions, alternative explanations and counterevidence. Distinguish observations from causal interpretations. Explain which conclusions or actions are justified and which are not; further exploration is different from a commitment to act.

Carry upstream uncertainty forward. A later model must not convert a partial evidence record into a strong recommendation through more confident phrasing. Qualitative confidence should reflect actual support and limitations.

There is no two-to-three-page limit for analysis. Keep the key arguments and evidence needed to audit the final report. Remove redundant expression rather than reasoning that could change the conclusion.

## 5. Final report

### Reader and purpose

Lead with the answer and supported recommendation, or the key understanding for exploratory research. Background and methodology should not delay the answer.

Adapt emphasis to the audience: decision makers need options and tradeoffs; practitioners need workflow implications; technical readers need constraints and comparison results; exploratory readers need what is known and unknown. These are examples, not mandatory report sections.

The application supplies the title. Start the model-written report with substantive content instead of repeating the document title. Use level-two headings for major sections and level-three headings for subsections. Explain limitations to the reader in the goal language, not as instructions to another agent.

### Main-body reading budget

Unless the user explicitly requests another length, target roughly **two to three pages of reading**:

- About **1,200–1,800 Chinese characters** or **800–1,200 English words**.
- Include tables and captions in the main-body reading load.
- Exclude the appendix; use a comparable reading load for other languages.

This is a flexible reading target, not a hard limit, a minimum to fill or a promise of printed pagination. Natural, complete, readable explanations and evidence fidelity take priority over the default length; exceed it when needed. Shorten by removing repetition and moving detailed support to the appendix, not by deleting subjects, logical connections or necessary explanations. Do not mechanically truncate the report, reduce the evidence collected, or remove critical caveats to meet it. Older plans requesting exhaustive coverage normally place that detail in the appendix unless the user explicitly requested a long main body.

### Body and appendix responsibilities

| Keep in the main body | Put detailed support in the appendix |
| --- | --- |
| Direct answer and supported recommendations | Full claim-to-source mapping and provenance |
| Few findings needed to understand the answer | Full evidence-requirement assessment |
| Concise citations beside material claims | Detailed dates, calculations and evaluation protocols |
| Decision-changing risks and uncertainty | Long comparison tables and attachment observations |
| Conditions that would change the conclusion | Detailed qualitative confidence and invalidation rationale |
| Prioritized next steps, where useful | Audit details and supporting references |

Both parts belong in the same report Markdown field, separated by a clearly labeled Appendix in the goal language. Preserve required links and exact attachment references. The appendix enables verification; it should not repeat upstream reports wholesale or duplicate the body.

Write a connected explanation rather than a compressed analysis outline. Develop core findings with their supporting evidence and qualifications instead of crowding all analytical dimensions, prices and dates into the opening. Before returning, check that the opening answers the question and readers can follow the reasoning on one reading, with key evidence, citations and necessary qualifications attached to their claims. Preserve a complete explanation when it conflicts with the default length target. A critical unmet evidence requirement must remain visible in the body.

### Regenerate final-report wording

Completed final reports expose **Regenerate** in the result view. Users may supply writing preferences (audience, tone, structure or length) or leave them blank to apply the current clarity guidance. This creates one new final-report attempt using the validated dependency evidence/analysis and the selected prior report; it does not modify task instructions or start upstream tasks. Preferences are saved in that attempt's prompt. Earlier versions remain available through the run version picker, including after a failed regeneration.

Regeneration requires the selected report's input signature to still match current research inputs. Changed or invalid upstream results are rejected instead of silently triggering new research. Existing execution, material validation and result validation checks still apply. The prompt confines the model to existing evidence, preserving citations, findings and limitations; it forbids fresh browsing and provider queries. This is prompt guidance, not a separate tool-access sandbox or automated semantic equivalence check.

### Inspiration from Karpathy's X post

The reference is [Andrej Karpathy's post on understanding model outputs](https://x.com/karpathy/status/2105819303471976479), using the text and illustrative sheet supplied by the user in this project discussion. It is a design influence, not evidence that formatting guarantees factual correctness.

The post proposes clearer writing inspired by ASD-STE100, followed by diagrams/images, interactive HTML and bespoke explainer videos as ways to make model output easier to understand. It also describes a relaxed application of the writing standard.

Our current adaptation is deliberately specific:

- Borrow plain wording, consistent terminology and focused sentences/paragraphs across all research stages.
- Preserve technical meaning and uncertainty while simplifying expression.
- Do not claim ASD-STE100 compliance or impose its controlled English vocabulary, grammar or word-count rules on Chinese or other languages. The “80%” suggestion is inspiration, not a measurable quality threshold.
- Use Markdown tables and Mermaid diagrams when they clarify the evidence. Explain symbols and label diagrams; do not invent data or add decorative graphics.
- Render the report through the application's HTML template, including navigation and supported math/diagrams. The model returns Markdown, not arbitrary executable HTML.

Custom interactive explainers, generated illustrations and narrated videos are not implemented workflow requirements. They remain possible future output formats, not promised features. Clarity helps readers assess evidence; it does not replace evidence verification.

### Final-report editorial check

Final report execution now includes a local editorial check within the existing model task: identify specific comprehension problems, revise only the affected wording, and compare changed claims with the evidence before returning the normal result. It adds no separate model call, user confirmation step, output schema or detector dependency. Collection and analysis retain their full evidence and argument records.

The check targets compressed analytical shorthand, unexplained terminology, unclear comparisons, overloaded sentences and headings that overstate the supporting paragraph. Chinese prose should use ordinary complete sentences instead of forcing the reader to decode abstract noun clusters or slash-separated labels. Other languages should follow the intended audience's natural usage.

This report-specific policy protects genuine contrasts and logical transitions. It does not ban punctuation, numbered headings, questions or words because they resemble model output. It does not manufacture irregular sentence lengths or introduce anecdotes and personal voice. Quotations, table values, equations, code, diagram data, source URLs, numerical definitions and uncertainty remain protected. Explanatory appendix prose can be clarified without deleting its technical support. Unsupported or contradictory claims require evidence review, not cosmetic polishing.

The [Lieflat Less AI Tone repository](https://github.com/larashero3-dotcom/lieflat-less-ai-tone) informed the evaluation of this approach. Its README describes corpus comparison and its skill describes detection followed by constrained revision; it is not a pretrained rewriting model. We reviewed its [editing guidance](https://github.com/larashero3-dotcom/lieflat-less-ai-tone/blob/main/SKILL.md), [detection protocol](https://github.com/larashero3-dotcom/lieflat-less-ai-tone/blob/main/DETECT.md) and [license](https://github.com/larashero3-dotcom/lieflat-less-ai-tone/blob/main/LICENSE). The repository identifies its license as CC BY-NC 4.0, with separate commercial licensing offered by the author. No upstream scripts, corpus, metric thresholds or skill text are vendored here; this application uses its own report-focused instructions. Importing upstream assets would be a separate licensing and integration decision.

Its reported corpus findings have not been independently reproduced in this project and do not establish improved quality for our research reports. We do not adopt a target AI-tone score, infer authorship, or intentionally break coherent argument transitions. The current check is prompt guidance, not a deterministic before/after validator or a demonstrated model-quality improvement.

For acceptance, compare reports for the same research and audience: can readers explain the answer after one reading, identify the subject and comparison, and trace each material claim? Check that rewritten wording retains the original scope and confidence. Passing prompt-wiring tests establishes that the instruction reaches the correct phase, not that a generated report has passed an editorial evaluation.

## 6. Handoffs, result states and publication

Each execution step receives the goal, relevant instructions/comments, supplied materials and validated dependency evidence. Input signatures and dependency run IDs prevent changed inputs or stale upstream results from silently counting as current work. Where full upstream reports are provided as files, read them in bounded sections and preserve their source records; do not claim to have reviewed unread material.

Research results use a structured envelope containing `status`, Markdown `report`, optional `blocker`, and `sources` with title, URL and publication time. The readable content is in `report`; it must contain the useful result, not only a short summary.

| Result | Meaning |
| --- | --- |
| `complete` | The assigned work is fulfilled and the conclusion adequately supported; this can include a justified finding that no decision can yet be made |
| `partial` | Useful work exists but required evidence remains incomplete or unavailable; preserve findings and gaps so downstream work can continue |
| `blocked` | An execution-wide obstacle prevents meaningful completion, such as an unavailable browser runtime or required local input |

A completed piece of prose is not necessarily complete research. A single inaccessible site normally yields a gap, not an execution-wide blocker. An actual browser handoff stops browser actions; continue with existing evidence and disclose gaps until the user explicitly returns control. Do not bypass the handoff with another browser or background fetching.

Rendering and publishing are separate from research completion. The publication UI distinguishes successful GitHub synchronization, waiting for deployment, site-access errors and a verified online report. A successful push is not proof of public availability. Online verification checks the report version marker; verification errors retain the successful commit so the user can check again without another push.

## 7. Automation occurrences

Automations reuse the research workflow but have a separate approval and time model. The first version supports intervals, daily and weekly schedules in an IANA time zone; arbitrary Cron, external actions and background execution while the app is closed are not supported.

- **Two shapes.** Quick tasks run one answer step per period and return the smallest complete answer: the direct value(s) or short result, its as-of time, two to five original links and at most one or two material caveats. They do not add an appendix, methodology, evidence-requirement table, scenario analysis, background history or next-step plan unless the task explicitly asks for one. Research automations keep the full collection → analysis → report contract described above. Both shapes use the same execution path and JSON result envelope.
- **Separate approval.** A deterministic recurring template (collection → analysis → report) is reviewed and enabled by the user. Approval binds the definition version, template, schedule, effective models/agents and material signatures. Global model changes do not alter an enabled automation. Editing scope, plan, models, materials, agent or schedule creates a new version and requires approval again; running periods keep their old snapshot.
- **One occurrence = one period.** Each occurrence creates its own internal Topic, Goal, dependency steps, working directory, session and browser Space. It is excluded from the normal board, task tree, progress list and automatic publication by persisted source relations (`automation_id`, `automation_occurrence_id`), not by title.
- **Explicit time context.** A scheduled occurrence uses its scheduled trigger as `reference_time`; a manual run uses the request time. Retries keep the same reference time and data window. The prompt states scheduled/actual UTC times, time zone, `reference_time` and a resolved previous-local-day window; “yesterday” is interpreted against that reference, not the actual start date. Business trading days are verified by the collection step and remain unresolved if unknown.
- **Catch-up.** Only the newest missed trigger runs by default, and it is skipped when older than the allowed delay. Older triggers are summarized by count and range; no occurrence is created per minute. Pausing and resuming does not backfill. A manual run does not move the schedule cursor.
- **Limits.** One active occurrence and one unstarted occurrence per automation; sharing the normal concurrency limit, resource locks and Topic FIFO. Automatic retries reuse the existing step retry budget. A total occurrence deadline starts at the first actual start and survives retries, restarts and sleep. Consecutive final failures pause automatic triggering until the user resolves them.
- **Same execution path as a manual run.** A scheduled period reuses the ordinary research pipeline: the same `prepare_run`, the agent selected in Settings (OpenCode or Codex today), the same working-directory rules and the same JSON output envelope. Quick tasks substitute the short-answer guidance for the long-report guidance; Research tasks keep the full report contract. Automations do not ship a second agent configuration or a project-local restricted profile. Unattended periods also declare that no one is watching and require recording an evidence gap instead of waiting for input when a source needs sign-in, a CAPTCHA or a browser handoff. Requesting email, publishing, trading, deletion or scripts at creation is still rejected.
- **Recovery.** Startup distinguishes a verified running session, completed result, confirmed stop and unknown state. Completed work imports idempotently; confirmed stops may requeue the same period; unknown states enter verification and block dispatch instead of replaying. Reports, sources, sessions, materials and dedupe keys are never removed by log cleanup.

Automations run only while the app is open and the computer is awake. Closing the window requests process termination through the normal shutdown path; it does not keep a daemon alive.

## 8. Enforcement and limitations

The application validates structured plans, required dependency relationships, research-result schemas, source-field requirements, material citation requirements and input freshness. Completed collection requires at least one source. These checks catch contract failures, not all unsupported claims.

Source truthfulness, independence, evidence coverage, faithful paraphrasing, reasoning quality, audience fit and main-body length primarily depend on model adherence to the prompts. There is no automatic factual certification, hard pagination enforcement, source-scoring system or guarantee that every produced report follows the writing rules perfectly.

Model substitutions do not change this policy. Assess a workflow change by evidence fidelity and reader usefulness, not by output length alone. Useful checks include whether a reader can find the answer quickly, trace material claims, see what remains uncertain and understand what would change the conclusion.

## 9. Keeping this document current

Update this document in the **same change set** whenever planning behavior, research prompts, evidence rules, result contracts, report presentation or publication semantics change. Do not postpone documentation until after a release.

For each relevant change:

1. Inspect the affected implementation and tests; describe actual behavior rather than intention alone.
2. Update the affected stage, implementation map and limitations as necessary.
3. Separate implemented behavior, prompt guidance and future proposals.
4. Recheck that collection/analysis retain full support and that only the final body has the default reading budget.
5. Verify relative links and examples; update the review date and add a short change-history entry.

This is a contributor maintenance rule, not a background synchronization service. Keep substantive policy here and link to it from other documentation instead of maintaining competing copies.

### Implementation map

| Area | Source |
| --- | --- |
| Request expansion, assumptions and temporal anchoring | [request_planning.rs](../src/request_planning.rs) |
| Research type, evidence planning, shared writing and final delivery guidance | [research.rs](../src/research.rs): `RESEARCH_TYPE_GUIDANCE`, `RESEARCH_PLAN_GUIDANCE`, `CLEAR_RESEARCH_WRITING_GUIDANCE`, `REPORT_DELIVERY_GUIDANCE`, `REPORT_EDITORIAL_GUIDANCE` |
| Stage prompts, result parsing, material checks and dependency context | [research.rs](../src/research.rs) |
| Project-level planning entry point | [project_planning.rs](../src/project_planning.rs) |
| Report HTML generation | [report_html.rs](../src/report_html.rs) |
| Report visual presentation | [report assets](../assets/report/) |
| Export | [research_export.rs](../src/research_export.rs) |
| Publication state and exact-version verification | [report_publish.rs](../src/report_publish.rs) |
| Repository publication | [report_git.rs](../src/report_git.rs) |
| Automation definition, occurrence and approval state | [automation.rs](../src/automation.rs) |
| Automation schedule, preview, DST and catch-up arithmetic | [automation_schedule.rs](../src/automation_schedule.rs) |
| Automation dispatch, queue hand-off and recovery | [automation_runtime.rs](../src/automation_runtime.rs) |
| Automation template, snapshot and time-context injection | [automation_execution.rs](../src/automation_execution.rs) |
| Automation management interface | [automation_views.rs](../src/automation_views.rs) |

### Change history

- **2026-11-04:** Established the consolidated strategy from current planning and execution prompts, report rendering and publication behavior. Documented cross-stage clarity/fidelity, final-body reading limits, the Karpathy-inspired adaptation and the maintenance contract.

- **2026-11-04:** Added final-report-only editorial guidance for natural, explicit wording with evidence-preserving local edits. Recorded the Lieflat reference, licensing boundary and lack of an automated quality guarantee.

- **2026-11-04:** Documented request-plan regeneration before approval, atomic replacement with archived prior goals, and the distinction from replanning into a separate topic.

- **2026-11-05:** Added the automation occurrence contract: separate approval snapshot, per-period time context, bounded catch-up, occurrence limits, recovery states, publication exclusion and the OpenCode-only unattended boundary. Runtime tool-denial and real-model smoke tests remain unverified.

- **2026-11-04:** Unified header planning actions around the topic lifecycle and changed the derived-topic indicator to a notebook-and-pen icon.

- **2026-11-04:** Prioritized natural, complete explanations and evidence fidelity over the default final-body length target. Added guidance to expand analytical shorthand, remove repetition rather than sentence structure, and retain key evidence and necessary qualifications in the body.

- **2026-11-04:** Added final-report regeneration with optional writing preferences, unchanged upstream research, input freshness checks and retained report history.

- **2026-11-06:** Removed the automation-only restricted agent profile and the build gate that blocked enabling. Scheduled periods now reuse the manual research execution path and the agent chosen in Settings; the schedule is the only difference from a manual run.

- **2026-11-06:** Added the Quick task short-answer contract. Quick periods keep the same execution path and JSON envelope but return the direct result, as-of time, a few links and material caveats without the long-report appendix, methodology or scenario sections. Research automations keep the full report contract.
