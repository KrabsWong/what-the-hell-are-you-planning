# Recurring automations: product and technical contract

> **Status: historical implementation plan.** This file is retained for background and design rationale. Current user-facing behavior is defined by [PRODUCT.md](../PRODUCT.md), [DESIGN.md](../DESIGN.md), [USER_GUIDE.md](../USER_GUIDE.md), [Research Workflow Strategy](RESEARCH_WORKFLOW_STRATEGY.md) and the maintained [Route and action matrix](ROUTE_ACTION_MATRIX.md). Where this historical text differs, those current documents win.

This document describes the automation behavior at the time of the plan. The research workflow, report writing and publication semantics follow [Research Workflow Strategy](RESEARCH_WORKFLOW_STRATEGY.md); this document only adds the differences created by recurring execution.

## 1. Purpose and boundaries

Automations let a user describe recurring information work and repeat an approved template on a schedule. Investment research is one example; a task can also track technology, produce an industry brief or organize reference material.

Run boundaries:

- Runs only while the app is open and the computer is awake. Nothing runs while the app is closed, asleep or offline, and there is no background daemon.
- Supports fixed intervals, daily times and selected weekdays; arbitrary Cron is not supported.
- Intended for information collection, analysis and local reports. Creation rejects obvious external-action requests as input validation and prompt guidance; this is not a runtime sandbox, and approved steps use the selected agent's normal permissions.
- When an unattended period meets a sign-in, CAPTCHA or browser handoff, it records an evidence gap and continues instead of waiting for input.
- Report content still needs human review; automations do not claim factual correctness.

### Capability limits

- The objective and expected-result fields are checked heuristically when you continue, save, revise or enable. Generated goal titles, step titles, goal briefs and step instructions are checked before a template can be saved or approved. These checks are input validation, not a sandbox.
- Unchecked sources remain: the automation name, material contents, comments, model output and generated report text. New or future prompt text is not retroactively audited.
- Approved steps use the same OpenCode or Codex runtime and permissions as manual research. Codex launches with workspace writes and network access; OpenCode uses its configured default agent. Prompt statements that say not to send, publish, trade or delete are guidance and can be ignored by a model.
- Period deduplication is not exactly-once for external operations.

## 2. Two task shapes

Both shapes share the same execution, retry, queue and result contract.

- **Quick task:** one check instruction plus a schedule. It does not choose a project, create a goal tree or require plan review. Each period runs one answer step intended for an information check and returns the direct value or conclusion, an as-of time, 2–5 original sources and at most one or two material caveats. It does not add an appendix, methodology, evidence-requirement table, scenario analysis, background history or next steps unless the task explicitly asks for them.
- **Research automation:** a one-sentence request is expanded into reviewable goals. Each period instantiates the same approved template and returns the full report contract. Each period creates a separate internal topic; Quick does not create a topic.

Creation flow:

> **Historical note:** the current Quick form also captures an optional expected answer and reviews it in a three-step authorization flow. See [USER_GUIDE.md](../USER_GUIDE.md#5-scheduled-automations).

1. Choose a shape. Quick asks only for the instruction and schedule; Research asks for the request and an optional expected result.
2. Set the interval, time and IANA time zone. The page previews the next three triggers with the same arithmetic used by the scheduler. Frequency is always set manually, never inferred by a model.
3. Quick derives a deterministic single-step plan; Research calls the planning pipeline once at this point and produces reviewable goals/steps.
4. Save a draft, or explicitly enable. Saving a draft and generating a plan never run research.

The plan is generated only once. Each period instantiates the approved template and never re-plans; Research allows an explicit **Replan**, which creates a new version and requires fresh approval.

## 3. Schedule contract

| Item | Rule |
| --- | --- |
| Interval | Positive whole minutes or hours, minimum 1 minute; anchored to the enable time and computed at a fixed frequency, not drifting with completion time |
| Calendar | One local time per day; one or more weekdays plus a local time per week; the weekday set must be non-empty |
| Time zone | Store the IANA identifier, defaulting to the system zone at creation; a fixed UTC offset cannot impersonate a time zone |
| Time storage | Execution instants are stored in UTC; rules keep local time and zone and do not follow system time-zone changes |
| Preview | Shares the scheduler's calculation and shows date, time and time zone |
| Enable | Starts from the first trigger strictly later than the enable time; immediate run is a separate action |
| Immediate run | Creates a manual period and does not move the schedule cursor; uses the same approval, concurrency, permission and timeout rules |
| Overlap | At most one active period (including retry waiting or recovery verification) and one additional unstarted period per automation |
| Coalescing | A new scheduled period replaces an older unstarted scheduled period and records the skip reason; queued manual periods are not replaced |
| Pause | Stops future triggers and cancels unstarted periods; the current period and its authorized retries may finish; Stop this run separately cancels the current period and later retries |
| Re-enable | Recomputes the future cursor and re-anchors interval scheduling; no catch-up while paused |
| Edit | Changes to task, plan, materials, Agent/model, time/zone or run policy require a new version and re-enabling; the current period keeps its old snapshot; a name-only change does not affect approval |
| Copy/archive | Copy creates an unapproved draft without history or sessions; archive stops future triggers and cancels unstarted periods while keeping history |

### Catch-up and unusual clocks

> **Historical note:** the current UI exposes **Only the latest missed period** (LatestOnly) and **Skip all missed periods** (SkipAll). SkipAll skips the collapsed missed range and records a catch-up skip rather than an expired skip. See the maintained [Route and action matrix](ROUTE_ACTION_MATRIX.md#catch-up-rules).

- Catch up only the newest missed period by default; the allowed delay defaults to one nominal period capped at 24 hours and can be adjusted in advanced settings.
- The allowed delay starts at the planned trigger and is checked again before execution; older periods are marked skipped. Manual immediate runs also use this validity window.
- Large gaps keep only the newest candidate; skipped history is summarized with ranges and counts rather than creating per-minute entities.
- Skip a local time that does not exist during DST; for a repeated local time, choose only the first corresponding UTC instant.
- Clock rollback never replays a processed trigger; forward jumps and wake-ups follow the catch-up policy.

## 4. Per-period time context

Each period stores `scheduled_for`, `actual_started_at`, `timezone`, `reference_time` and a resolvable `data_window`.

- Scheduled and catch-up periods use the planned trigger as their reference; manual runs use the request time.
- Retries keep the same period reference, inputs and time range; a retry day never replaces them.
- "Yesterday" means the previous natural day in the scheduled time zone; prefer a start-inclusive, end-exclusive window.
- "Previous trading day" and similar terms are verified by the execution step using a reliable calendar or data source; the generic scheduler never assumes Monday–Friday are trading days.
- When a business time range is unverified, keep it unresolved and let the collection step explain the result; never invent a precise range.
- Each period uses a new session, output directory and browser Space; it does not automatically reuse the previous period's report as evidence and only carries the user-approved static material snapshot.

## 5. Approval, versions and execution

Enabling approves the current template and schedule for repeated execution. Automation approval uses its own source and version identifier; changing an old goal's state is not a way to bypass approval. Dispatch rechecks the period snapshot, authorization and lifecycle.

Automations reuse the normal research execution path:

- The same `prepare_run`, working-directory rules and JSON result contract.
- Agent and model are selected per stage in Settings, with both OpenCode and Codex supported.
- There is no second agent configuration, and enabling is not blocked by build type or by automation.
- Plan-generated periods and manual runs share approval, concurrency and permission rules; automation only adds a trigger.

Error handling:

| Error | Handling |
| --- | --- |
| Network, rate limiting and other transient errors | Reuse the bounded step retry, defaulting to at most 3 attempts 30 seconds apart; never retry early when the service gives an explicit wait time |
| Output validation failure | Bounded correction within the same step retry budget |
| Missing model, invalid identity, insufficient permission, browser handoff | Move to needs-attention; do not retry continuously |
| User stop | Cancel the current period and later retries; never retrigger the same period |
| Timeout | Stop the current period, record the reason and do not start new steps beyond that period's deadline |
| Consecutive final failures | Pause automatic triggering after 3 consecutive periods; success clears the count, and skipped/cancelled periods do not count |

The default per-period deadline is 30 minutes and can be adjusted in advanced settings. It starts at the first actual start and includes waits between steps and retries. The deadline is saved and survives restarts and sleep.

## 6. Results, history and concurrency

- Results show the summary, report, sources, query time range, actual collection time and missing or conflicting information. Source-field validation is not factual verification.
- Each period is viewed in automation history and never enters the normal board, project task tree, unfinished-research list or automatic publication.
- Targets for T15: lists load with a bound or page, and details load on demand. Current storage still deserializes all history and report bodies into memory; the route matrix records that boundary.
- Log cleanup handles only terminal periods and never deletes reports, sources, recovery inputs, session identifiers or deduplication evidence.
- Automation periods use a separate concurrency pool (default 2, counting only running periods) and a separate queue. They consume no Research slots and never join the Research Topic FIFO.
- The two queues wait on each other only for genuinely shared folders, browser Spaces or sessions.

## 7. Persistence and recovery

Target persistence contract: each trigger saves the coalesced period, snapshot, related entities and next-trigger cursor in one Store/SQLite transaction, and only a committed transaction may be queued for execution. The current candidate writes the workspace synchronously through `Storage::save`; T13 S1 and T14 ordered persistence acknowledgements are not accepted, so this paragraph is a target, not a claim about a background save worker.

- The scheduled uniqueness key is `(automation_id, scheduled_for_utc)`; manual periods use a separate request ID.
- Startup verifies recovery before dispatching new work.
- A still-running session keeps being monitored; a completed attempt imports only the matching result; a system-confirmed stopped or interrupted session may enter bounded recovery according to authorization, while an explicit user cancellation is not requeued; an unknown state blocks duplicate execution and moves to needs-attention.
- Period deduplication is not exactly-once for external operations, and the product does not promise that.

## 8. Implementation map

| Module | Responsibility |
| --- | --- |
| [automation.rs](../src/automation.rs) | Definitions, versions, periods, approval snapshots and state transitions |
| [automation_schedule.rs](../src/automation_schedule.rs) | Trigger calculation, previews, catch-up and validation |
| [automation_runtime.rs](../src/automation_runtime.rs) | Due scanning, transactional bookkeeping, queueing and recovery coordination |
| [automation_execution.rs](../src/automation_execution.rs) | Converts an approved template into research steps and links results |
| [automation_views.rs](../src/automation_views.rs) | List, creation, settings, detail and history |
| [research.rs](../src/research.rs) | Shared stage prompts, result parsing and dependency context |
