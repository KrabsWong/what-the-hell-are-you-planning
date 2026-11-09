# Product

## Register

product

## Users

Primarily desktop users without a developer background. They use the task manager to organize research projects, choose topics, add materials and follow research progress and results. The interface supports continuous operational work, with Chinese task content alongside the existing English action labels.

## Product Purpose

A native Rust / gpui-kit task workspace for automatic topic planning, manual Kanban admission, per-stage Agent and model selection, ego(lite) collection, PDF/image materials, logs and reports, plus recurring information research. Success means the user knows where to act next, approval and execution boundaries stay clear, and real data keeps saving.

## Brand Personality

Keep the current native desktop direction: simple, restrained and practical. No brand redesign. System fonts, compact layout, the existing warm neutral palette and light/dark themes are implementation constraints.

## Anti-references

Keep action labels and icons visible at rest and distinct from explanatory copy. Auxiliary actions and icon controls follow the main-branch appearance, without permanent button surfaces or borders on every control. Preserve hover and keyboard focus feedback. Status tags must not look like buttons.

## Design Principles

1. Actionable controls are identifiable at rest.
2. Primary actions, secondary actions, navigation and destructive actions each have a consistent appearance. Reserve the primary fill for the main advancing action in a local group; use secondary buttons or icon controls for supporting actions and a More menu for suitable low-frequency actions. Do not give every action equal prominence.
3. Preserve the layout, data and research workflow users already learned.
4. Disabled actions explain why; the user should not have to guess.
5. Default pages foreground tasks, progress, results and the next step; model paths, sessions, raw logs and stage rules stay in details and settings.
6. Theme switching, narrow windows, keyboard use and responsive scrolling through long readers, histories, boards, settings, materials and dialogs remain available. Scrolling smoothness under concurrent updates and saves is a release gate, not a benchmark aspiration.
7. Automations clearly distinguish "enabled" from "this period failed"; schedule, next trigger, latest activity time and unread results are visible in the list, the approved version is visible on the detail and in Settings, and run logs live in technical details.
8. Automation results do not enter the normal board, task tree or global publishing; they appear only in automation history.

## Automations

The sidebar has two domains: **Automations** above **Research**. Research contains **All research**, the project list, **Paused projects**, **Research results** and **Archive** for archived research. Automations contains **All automations** and **Automation results** directly, no longer alongside All or exposing two identically named result entries. Automation results are fully separate from project/topic/goal research results, and their header can filter the list to one automation's periods. A user describes recurring information work. Both shapes use one three-step wizard: task and instruction, schedule with three trigger previews, then plan and authorization review. A **Quick task** supplies one instruction, an optional expected answer and a manually set schedule; its final step is authorization, not a generated goal plan. A **Research automation** supplies a one-sentence request and an optional expected result; the planner generates reviewable goals once at creation, and each period repeats the approved template. Automation output lives in the generated Automations container and never enters normal projects or topics; Research creates a separate topic per period, while Quick creates no topic and places its one-step Goal directly under the period. A period's steps run strictly in phase order within each Goal — every collection step, then analysis, then the final report — and each step also waits for its declared prerequisites; a later phase never starts while that Goal's collection is unfinished. Goals run in their approved order, so an earlier Goal finishes before the next Goal starts, and only the step that actually failed is shown as Blocked. Every step states its actual eligibility: earlier unfinished phases, earlier goals in the same period and declared prerequisites produce Waiting with the blocking reason, while a queued period says it is waiting for an automation slot. Automation steps never suggest admission to the ordinary Research queue, and a paused definition does not pause an already-running period. Research plans are generated only at creation or on an explicit Replan; periods never re-plan. The objective and expected result form one immutable planning revision, so editing either field requires a newly reviewed plan; each frozen period keeps the revision and approved snapshot it started with. Leaving the creation page while a plan is still generating keeps a **Planning…** state on the saved draft and its detail page. A research period lists every split step with its full title, keeps Back inside the period, and offers **Continue period** for active unfinished work or **Retry period** for a failed, cancelled or interrupted period. Continue finishes the same period's remaining steps; Retry reopens that same period against its frozen approved version, keeps the original reference time and finished step results, and never resumes the schedule. **Resume schedule** changes only future triggers. Saving a draft or generating a plan does not run research; edits require a new version and fresh approval. Automation periods use a separate concurrency allowance (default 2, counting only running periods) and a separate queue. They consume no Research slots and never join the Research Topic FIFO or Research results; Research defaults to 3. The two pools do not affect each other, and only genuinely shared folders, browser Spaces or sessions still serialize. A result entry names what is available — **Final report**, **Partial period reports**, **Intermediate evidence** or **Live progress** — with evidence quality and freshness as separate labels; streaming text is never presented as a validated final answer. The maintained current/target route and action contract lives in [Route and action matrix](doc/ROUTE_ACTION_MATRIX.md).

Automation periods run only while the app is open and the computer is awake; there is no background daemon. On return, **Only the latest missed period** can run the newest missed period when it is within the allowed delay; a newest period older than that is skipped, and older triggers are summarized rather than replayed. **Skip all missed periods** skips every missed trigger and waits for the next future one. Pausing and resuming never backfills the paused interval, and **Run manually** creates one manual period without moving the schedule cursor. Agent and model are selected per stage in Settings, with both OpenCode and Codex supported. Automations are intended for information collection, analysis and local reports. Creation checks the instruction and expected-result fields, and validates planner-generated goal and step text, for obvious external-action requests, but those checks are input validation and prompt guidance, not a sandbox; approved steps use the selected agent's normal permissions. Review report content before acting. When an unattended period meets a sign-in, CAPTCHA or browser handoff, it records an evidence gap and continues instead of waiting for input.

## Accessibility & Inclusion

No specific certification standard was requested. Check text and control contrast, keep keyboard focus visible, give icons accessible names and tooltips, and never rely on color alone to distinguish buttons. Desktop pointer targets are about 32 px; compact windows may scroll; Chinese text and long labels need real-device testing.
