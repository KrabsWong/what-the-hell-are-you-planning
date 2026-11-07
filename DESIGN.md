# Task Manager interaction design

## Direction

Keep the existing native workspace layout, system fonts and warm neutral palette. Light mode serves daytime reading and dark mode serves dim environments; both must show control boundaries at rest. Default pages serve non-developer research workflows, with technical configuration and diagnostics on demand. Preserve the existing palette, button discoverability and all execution capabilities.

## Components

- Primary: solid deep warm fill (solid light warm fill in dark mode) for create, generate, save and run. Emphasize one advancing action per local action group.
- Secondary: always-visible neutral surface and full border for settings, edit, copy, close or back. Never make these ghost buttons by default.
- Selection: bordered selection control; the selected state uses an accent surface/text and a clear visual change. Toggle controls also set the toggled semantic state.
- Link: underlined accent text for navigation, sources and opening files; body-text URLs follow the same rule. Settings actions remain buttons.
- Icon: fixed square hit area, full boundary, tooltip and accessibility_label; never leave a bare × or +.
- Task tab: the title and close icon share one background and border, with no separate button border inside. The close icon is always visible and keeps its own hit area, hover and focus feedback. Long titles ellipsize to the available width, reveal the full name on hover, and never compress the close area.
- Danger: red outline for stopping a run or removing material; ordinary close does not use danger color.
- Disabled: keep the shape, weaken fill and text, and explain the business reason near the button; never trigger the action.
- Focus: keep the component library's focus drawing and Tab/Enter/Space semantics; hover alone is insufficient.

## Geometry and typography

Action buttons are about 32 px tall with 6 px radius and 14 px system font; Undo in the bottom status bar uses a compact 24 px button. Toolbars stay on one line and truncate long titles instead of compressing buttons. Horizontally scrollable boards keep their current column widths. Form/detail action rows may wrap. Sidebar content scrolls and the theme switch stays fixed at the bottom instead of being pushed out as projects grow.

## Boundaries

Non-interactive chips, task statuses and field labels keep low contrast and no button border. Clickable lists/cards need hover feedback and a visible open arrow or action; do not give body copy a button shape. Shortcut navigation keeps the sidebar structure.

Readable text must be drag-selectable and copyable with ⌘C: body copy, titles, descriptions, reports, setting values, automation summaries and history all render through `views::selectable` (reports use TextView's built-in selection). Button text, navigation items and compact labels manually ellipsized on canvas do not participate in selection, preserving click semantics and truncation accuracy.

## Source of truth

`appearance.rs` owns theme and component colors. `controls.rs` owns button/link factories. Pages choose action semantics and bind behavior only; do not reintroduce a global ghost default. Native screenshots and interaction acceptance cover 1440×920, 1000×680 and both light and dark themes.

## Model settings

The global model entry sits at the bottom of the sidebar. The global page combines the default model, an independently searchable local model list, save actions and the current override list. An empty stage input inherits the global setting, and the field shows the effective model and its source; clearing an override takes effect only after saving. "Use this model for all stages" on the global page is an explicit option; saving never starts a task or changes history. Project planning windows and task details show the same effective-model rules.

When every stage inherits, the checkbox stays checked according to the actual configuration and acts as a read-only state; changing one stage override allows unifying again. Save feedback sits in a dedicated area at the top of the main content and never floats over edit buttons. Success and failure combine an icon, title and explanation; Saved appears only after local persistence succeeds. Feedback can be dismissed, and changing pages or abandoning a form clears the old message.

Inline notices use semantic tones: success green, warning orange, error red, neutral information gray. Each notice is a rounded box with a tinted surface: the background blends the tone with the panel color, and the border and text use the same tone; avoid weak "white fill + colored outline" styling. All colors come from `Palette`; dark mode automatically uses the dark panel with the same semantic color, and hardcoded light backgrounds are forbidden.

## Automations

- The sidebar separates **Research** and **Automations**. Research contains **All research** (all non-automation root tasks/goals), the project list, **Paused projects** and **Research results**. Automations is a separate section containing **All automations** (the automation list) and **Automation results** (all automation period results) directly, not under Workspace alongside All, and with no expandable second-level parent. Automation results are fully separate from Research results; the latter contains only project/topic/goal research results. Automation periods use a separate concurrency pool (default 2, counting only running periods) and a separate queue; they consume no Research slots and never enter the Research Topic FIFO. Queue and slot explanations appear only on the Automations page and in period details. Both section titles are static labels; child items are directly visible and own their navigation. List rows use a compact two-line layout: the first line has the name plus type, status and unread labels; the second is a single-line summary of schedule, next trigger and latest result. Long names ellipsize on one line and never intrude into the right label area. The all-results list assigns a session-level background tint per automation; periods from the same automation share a color, and hover deepens the same hue. The palette is not persisted and only aids grouping in the current session.
- Creation starts by choosing a shape (Quick task / Research automation), then entering one instruction or one-sentence request; choosing a project is no longer required. Quick finishes on one screen; Research confirms goals on a review page before enabling. Three steps remain: task → schedule/time zone/three previews → plan and authorization summary. A fixed bottom action bar shows Back, Save draft and Continue/Enable automation; saving a draft and enabling are two different visual levels.
- The detail header gives status, type, version and owning project in one metadata line below the schedule instead of faking tabs with a chip row. Do not add a second back button: use the top breadcrumb Back, which shows Automations / current automation and returns to the Automations list. All automations and Automation results belong to the separate automation area and do not show Research's All research breadcrumb or a return-to-All-research button. The page prioritizes execution and results: the action row (Pause/Resume, Run manually, Stop this run, Settings; Copy and Archive are not exposed yet) follows the header; history is a compact single-line text list without per-row card borders or fills, ordered by actual execution time descending (planned or request time when not started). Each row shows only execution time, status, trigger source, cancel entry for an active period and Open entry; unread still uses a dot. Inline explanations for queued periods reference only the automation concurrency pool (running / limit), not Research concurrency or queues. History is fully collapsed by default; clicking any row opens that period's result in a dialog. Report, sources, steps and technical details all live in that dialog, technical details are collapsed, and a single run's logs scroll within a height limit. Definition, expected result, next three triggers, run boundaries (catch-up rule and per-run limits) and the approval snapshot live in the Settings dialog opened from the action row; Edit and Replan sit at the bottom of that dialog next to the settings they change, while the page itself keeps only execution actions. Dialogs use a modal scrim, focus trapping and Escape to close; long content scrolls inside the dialog. Quick periods use the short-answer contract: direct value or conclusion, as-of time, 2–5 sources and at most one or two material caveats; no appendix, methodology, evidence-requirement table, scenario analysis or next steps. Research periods keep the full report contract. Quick period time context shows only planned/actual start, time zone and reference time; the data window appears only when resolved. Pause and Run manually appear in the same position in every state: grayed out for drafts, with the unlock condition explained below the action row; only drafts cannot run before approval. Scheduled tasks and manual research share one execution path (same agent, working directory and output contract); do not maintain a second agent configuration and do not block enabling by build or by automation. Application-level run preconditions (runs only while the app is open and the computer is awake) sit at the end of the page, not mixed with per-automation information. While paused, Next runs shows a Paused marker and explains that resuming re-schedules, with no catch-up during the pause. Overview body text uses body size and primary color; explanatory text (Quick task description, run limits, disabled-time-zone explanation) uses smaller, muted text, and the two do not share a visual level. The next trigger shows local time only; the time zone already appears in the schedule summary, so do not repeat the abbreviation. Research creates one topic per period; the period topic name appears in the result dialog.
- History loads with a bound; unread uses a dot; skipped, failed, running and enabled are separate dimensions, not one status covering all.
- On storage failure, invalid forms or missing model/project/permission, use a dedicated error area with a reason and never show success. Log cleanup must preview first and state that results/sources are retained.
- Automation-internal Tasks/Topics/Runs are hidden from the normal board, sidebar tree and Research results by default; history results can explicitly open steps, and the breadcrumb returns to Automations.

## Progressive disclosure

- Daily navigation uses Research results; Settings → Agent & models shows the models used for planning, collection, analysis and reporting.
- The execution page prioritizes task title, status, progress and report. Run history is a compact list; long titles use two lines and reveal the full name on hover. In-progress research protocols never render as reports.
- Technical details keep model, directory, session, time, exit status, Live logs and Saved output. A selected active task shows only one stop button, and batch controls stay visible outside the history list.
- The task page keeps start/view result, status, priority, project and due date by default. More details collapses low-frequency properties and execution settings; missing model or working directory shows a direct fix entry.
- The research page shows goal input and existing research by default; Advanced settings opens the configuration page, Back to research returns, and input is preserved. Direct settings links must open the configuration page.
- Completed research steps use View result as the primary action; rerun is secondary. The goal result entry points to a valid research step and never treats planning JSON as a final report.

## Icon source and implementation

All interface icons use [Lucide](https://lucide.dev/icons/) via the pinned gpui-kit assets. Follow [Icon Guidelines](doc/ICON_GUIDELINES.md) for semantic choices, embedding, accessibility, exported reports, and verification. Replanned topics use NotebookPen with an explicit Replanned tooltip.
