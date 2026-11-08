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

The user explicitly rejected buttons and links that disguise themselves as body text, interactions that only become visible on hover, and secondary actions that disappear into explanatory copy without a border or color. Status tags must not look like buttons.

## Design Principles

1. Actionable controls are identifiable at rest.
2. Primary actions, secondary actions, navigation and destructive actions each have a consistent appearance.
3. Preserve the layout, data and research workflow users already learned.
4. Disabled actions explain why; the user should not have to guess.
5. Default pages foreground tasks, progress, results and the next step; model paths, sessions, raw logs and stage rules stay in details and settings.
6. Theme switching, narrow windows and keyboard use remain available.
7. Automations clearly distinguish "enabled" from "this period failed"; schedule, approved version, next trigger and unread results are visible in the list, while run logs live in technical details.
8. Automation results do not enter the normal board, task tree or global publishing; they appear only in automation history.

## Automations

The sidebar separates **Research** and **Automations**. Research contains **All research**, the project list, **Paused projects** and **Research results**. Automations is a separate section containing **All automations** and **Automation results** directly, no longer alongside All or exposing two identically named result entries. Automation results are fully separate from project/topic/goal research results, and their header can filter the list to one automation's periods. A user describes recurring information work. Creation starts by choosing a shape: a **Quick task** needs one instruction and a manually set schedule; a **Research automation** expands a one-sentence request into reviewable goals and then repeats that approved plan. Automation output lives in the generated Automations container and never enters normal projects or topics; Research creates a separate topic per period, while Quick attaches directly to the period. A period's steps run strictly in phase order — every collection step, then analysis, then the final report — and each step also waits for its declared prerequisites; a later phase never starts while collection is unfinished, and only the step that actually failed is shown as Blocked. Plans are generated only at creation or on an explicit Replan; periods never re-plan. Leaving the creation page while a plan is still generating keeps a **Planning…** state on the saved draft and its detail page. A research period lists every split step with its full title, keeps Back inside the period, and offers **Resume** above the list for unfinished work; a failed or cancelled period is retried in place, keeping finished step results. Saving a draft or generating a plan does not run research; edits require a new version and fresh approval. Automation periods use a separate concurrency allowance (default 2, counting only running periods) and a separate queue. They consume no Research slots and never join the Research Topic FIFO or Research results; Research defaults to 3. The two pools do not affect each other, and only genuinely shared folders, browser Spaces or sessions still serialize.

Automation periods run only while the app is open and the computer is awake; there is no background daemon. Only the newest missed period is caught up by default, and it is skipped when older than the allowed delay; pausing does not backfill. A manual run does not move the schedule cursor. Agent and model are selected per stage in Settings, with both OpenCode and Codex supported. Automations do not send email or IM, publish, trade, run scripts or perform other external writes. When an unattended period meets a sign-in, CAPTCHA or browser handoff, it records an evidence gap and continues instead of waiting for input. Report content still needs human review.

## Accessibility & Inclusion

No specific certification standard was requested. Check text and control contrast, keep keyboard focus visible, give icons accessible names and tooltips, and never rely on color alone to distinguish buttons. Desktop pointer targets are about 32 px; compact windows may scroll; Chinese text and long labels need real-device testing.
