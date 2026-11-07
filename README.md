# What the hell are you planning?

A local-first desktop workspace for turning research questions into actionable plans, agent-powered tasks, and readable reports.

Define what you want to understand, let an agent propose a plan, and decide which tasks should move forward. Follow the work from evidence collection through analysis to a final report, with the sources, materials, and execution history available when you need them.

<img width="600" alt="image" src="https://github.com/user-attachments/assets/a6b14a0b-b350-40aa-a0ca-e35eca932589" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/c621590b-3be1-4b1f-baf1-48b5622dc49a" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/017416ad-a4b9-44c9-ac9d-46387f0f94f8" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/8261de32-9850-4a07-b9a2-e9eccbed3b21" />

## Plan with an agent, stay in control

- **Start with one sentence.** Use **New research** in a Project or Topic. The original request and submission time are saved before planning; failed or interrupted planning can be retried from **Market research → Saved research requests**. Relative dates stay anchored to the original request.
- **Required Topics.** The structure is Project → Topic → Goal → research steps. An explicit Topic is reused; otherwise the planner proposes a new Topic. Older unclassified roots move into **待归类** without losing results. Topics organize content and do not automatically share research context.
- **Complete plans before approval.** The planner expands the request into an objective, deliverable, scope, assumptions, facts to verify, and independent Goals with complete dependency plans. Goals enter **Backlog**. Planning never starts the research.
- **Goal-wide review.** The board groups Goal cards by Topic. Internal steps appear inside Goals, not as separate approval cards. Move a Goal to **Todo** to approve its current plan, then click **Run goal** or **Run / resume**. Dragging alone never executes work.
- **Versioned approval.** Changes to research scope, step instructions, dependencies, comments or materials return approved research to Backlog. Existing runs remain available; reapproval and an explicit start reuse valid evidence and refresh affected downstream results.

## Execute through local OpenCode

The application uses your locally installed and configured **OpenCode** to plan and execute research. Settings → Research models shows the effective model for planning, collection, analysis and the final report. Choose a model for each stage or leave it blank to use the global default.

Approved goals can be added to a FIFO topic queue even when all execution slots are occupied. One topic runs at a time, with dependency-ready steps running concurrently within it. Settings → General lets you choose 1–10 simultaneous research tasks (default 3) and a separate 1–10 automation-period limit (default 2); the two pools never consume each other's slots. Changes are saved per workspace; lowering a limit lets running tasks finish before admitting more work. Start with 3, try 5 after checking stability, and use 6–10 only after validating provider limits and local resource usage. Higher concurrency can increase rate limiting, retries, and browser load. Exhausted retries or blocked goals retain their status while later topics continue. New request-generated Goals use separate step folders and browser Spaces. Shared sessions, Spaces and overlapping folders remain serialized; legacy research preserves its existing workspace. A failed branch blocks its dependents while independent approved work can continue.

Interrupted or unsuccessful work can reuse an eligible existing OpenCode session when started again. Resumption depends on compatible execution settings and working directory; a restart does not blindly replay all pending work. Run history retains the model, session, inputs, logs, and outputs for each attempt. Report validation failures show the specific reason; automatic retries and manually restarted tasks receive that feedback so the model can correct the rejected output.

## Research with sources and context

The research workflow is designed for questions such as industry trends, market developments, and investment opportunities:

1. Capture a research request in one sentence.
2. Review the complete plan in Backlog, approve selected Goals into Todo, then explicitly start them.
3. Gather evidence using the local **ego(lite)** browser integration.
4. Analyze the findings and produce a final report.
5. Review the report, inspect sources, and add information for a follow-up run.

Browser work takes place in a dedicated ego(lite) **Space**, where you can inspect the pages and take over when necessary. The browser opens in its own window. Login, permission, and other user-intervention requirements remain visible rather than being silently bypassed.

Research plans state the decision or exploratory question, scope, and evidence needed to support a conclusion. Collection links findings to those requirements and records their limitations; analysis distinguishes supported, limited, missing, and conflicting evidence. Final reports explain the supported answer or recommendation, remaining gaps, what would change the conclusion, and focused next steps. Methods follow the research type: market validation, investment research, technical comparisons, and general research do not share mandatory domain-specific sections. These are agent output instructions, not automatic truth verification or credibility scoring. Existing plans remain usable; their evidence requirements are inferred within the approved scope when absent.

Collection, analysis, and final reports share clear-writing and fidelity instructions: consistent terminology, explicit attribution, and preservation of numerical scope, conditions, quotations, uncertainty, and citations. Collection retains task-relevant evidence and conflicting findings; analysis retains key arguments, assumptions, calculations, and counterevidence. These upstream outputs have no final-report reading budget.

Final reports adapt to the intended reader and research purpose, leading with conclusions and supported recommendations (or key findings for exploratory work). By default the main body targets two to three pages of reading—roughly 1,200–1,800 Chinese characters or 800–1,200 English words, including tables and captions—with key citations and decision-changing caveats retained. Detailed evidence, calculations, source records, and validation protocols follow in an appendix within the same report. Explicit user length requests take precedence. This is an agent writing instruction, not fixed pagination or automatic truncation; collection and analysis retain their full detail.

Final reports are given priority in **Research results**. Supporting evidence and analysis remain accessible, while source lists and technical execution details can be expanded on demand. Reports render as readable Markdown with tables and links.

While a research step runs, the progress panel lists the tasks in each phase, their statuses, and completion counts. Task details and the latest attempt's technical details are available beside each task. This outline remains visible when the app reconnects to a session still running in the background; completed attempts open their reports.

Small upstream results are included directly in the next task's prompt. When their combined context exceeds 180,000 bytes, the app instead writes complete Markdown reports and source-record JSON files into the task's `.research-inputs` folder. The prompt lists the files and instructs the agent to read them in bounded sections. This threshold selects a transfer method, not a model context limit; reading files still consumes model context. Original results remain in SQLite. Before dispatch, missing copies are regenerated and existing copies are checked against the saved results; modified copies block execution rather than silently changing the evidence.

## Automate a recurring check

**Automations** in the sidebar turns a recurring information task into approved research periods. Describe the task and expected result, choose a project, then set an interval, a daily time or selected weekdays in an IANA time zone. The app previews the next three triggers with the same arithmetic it later uses, generates a fixed collection → analysis → report plan, and shows the effective models and permission boundary before you enable it.

Creating or saving a draft never runs research. Enabling approves that exact version; later edits to scope, steps, models, materials, schedule or limits create a new version that must be reviewed again. Each period creates its own internal research goal, working directory, session and browser Space, and appears only in the automation's history rather than the normal board or task tree.

Runs happen only while the app is open and the computer is awake. If periods were missed, only the newest one runs by default, and it is skipped when older than the configured allowed delay; older periods are summarized instead of replayed. Pausing never backfills. Manual runs use the same approval and automation queue but do not move the schedule. Each automation can have one running period and one waiting request, uses its own concurrency pool (default 2) instead of the research limit or Topic FIFO, and allows one immediate stop plus the existing bounded step retries. Automation and research queues are independent; only genuine shared folders, browser Spaces or sessions still serialize work.

The first version is deliberately narrow: OpenCode only, no email/IM, publishing, trading, system notifications, scripts or other external writes. A generated project-local OpenCode agent restricts the run to read-only information work; the app does not modify your global OpenCode configuration. **In this build, real unattended dispatch remains disabled until execution-layer tool denial is verified: drafts and plan review are available, but enabling/running is gated, and only offline QA builds execute the fake-CLI chains.** Report contents still need human review. See [the automation contract](doc/AUTOMATION_PLAN.md) and [implementation evidence](doc/automation-tasks/evidence/05-acceptance.md) for exact limits, including unverified runtime tool denial and measured large-history performance.

## Bring your own materials

Attach PDFs or screenshots to a goal or a research step. Drag files into Materials, paste an image with ⌘V after clicking the panel, or use Add Files. The application only copies the originals into the workspace; it does not perform local OCR or PDF text extraction. Parent-goal materials and relevant upstream results accompany downstream tasks.

When descriptions, comments, materials, or upstream results change, affected results are marked for refresh. Subsequent execution can update the analysis and final report using the revised context.

Original files are sent to OpenCode with `--file` when a task runs. Choose a model/provider that supports the attached image and PDF formats. Models must report a blocked result if they cannot read a material. Existing attachments remain usable without extraction caches. Reports cite materials by their original filenames; exact paths of attachments supplied to that run are accepted as local references and kept separate from the HTTP(S) web-source list.

## A workspace built around tasks

- **Goal list and board views** with status filters, Topic grouping, and colored Goal cards. Board columns progress from pending work toward completion, with **Done** at the far right. Step boards put **Waiting → Ready → Queued → Running** in execution order, with retry, pause, and blocked states shown when applicable.
- **Expandable project trees** with research goals and their related steps, plus identifiers that distinguish goals, evidence, analysis, and reports.
- **Resizable navigation panels** and structural breadcrumbs for moving between projects, tasks, and results.
- **Priorities, labels, due dates, comments, activity history, and archiving.**
- **Creation timestamps** shown to the second where recorded.
- **A compact weekly calendar** with previous/next-week navigation, a Today shortcut, and date-based due-task filtering.
- **Light, dark, and system appearance**, shared across the application.
- **Live progress and logs**, with clear running, queued, interrupted, and failed states.

## Your data stays in a chosen workspace

On first launch, choose a workspace root before continuing. Projects and their generated artifacts are managed underneath that root, so agent runs have an explicit working directory.

Structured application data is stored locally in **SQLite**. Attachments and generated research files remain ordinary files alongside the database. The workspace preserves projects, Topics, original requests, structured plans, approval snapshots, tasks, dependencies, run history, and source records. After an app restart, sessions still running in OpenCode are monitored and their completed results imported without starting duplicate runs. Other interrupted work requires explicit resumption; unapproved plans never run automatically. Local research pauses when the computer is asleep or offline. A small application preference records the workspace location.

For a complete backup, quit the application and copy the entire workspace root, not just the database. There is no built-in cloud synchronization. OpenCode authentication and browser profiles are managed by those tools separately, so moving the workspace alone does not transfer their credentials or environments.

## Current scope

This is an early desktop application, currently validated on Apple Silicon macOS. It uses Rust and GPUI / gpui-kit for its native interface. At least one local agent CLI (OpenCode or Codex) is required for agent execution; ego(lite) is required for browser-based research.

The application provides research workflows, not trading or order execution. Generated conclusions and source records still require human review. A project working directory organizes agent output; it is not a filesystem sandbox.

## License

[GNU General Public License v3.0](LICENSE)

### Local macOS beta package

Run `./script/package_macos.sh` on Apple Silicon to build an optimized release and produce a DMG plus SHA-256 checksum in `target/distribution/`. The disk image includes `Planning.app`, an Applications shortcut, installation instructions, and the license. No workspace or credentials are packaged. Install OpenCode or Codex, plus ego(lite), separately.

These beta packages use ad-hoc signing and are **not notarized**. Testers may need to allow the app in macOS Privacy & Security after attempting to open it. For ordinary trusted distribution without this exception, use a Developer ID Application certificate and Apple notarization. The current package is arm64 only; compatibility with older macOS versions has not been fully tested.

On macOS, Codex collection runs use an app-owned `planning_ego` stdio MCP bridge for ego(lite). Browser scripts run with desktop access while Codex shell commands retain the `workspace-write` sandbox. The bridge is configured per run, not in global Codex settings, and requires the configured local ego-browser executable. Process exit logs are separate from validated research completion or blocking.

### Publish reports through GitHub and EdgeOne

Settings → Publishing configures a dedicated GitHub repository (HTTPS or SSH), target branch, and public HTTPS site URL. Git must be installed and authenticated on this computer using its credential helper or SSH agent. Credentials are not stored in the workspace. Connect the repository and branch to an EdgeOne Git project separately; a directly uploaded EdgeOne project must be replaced by a Git-integrated project before migrating its domain. Import existing report directories first to preserve their links.

Click **Publish report** in a Topic, or enable **Publish completed Topics automatically** in settings. Automatic publishing also includes already completed Topics. All active research Goals and pending planning requests must be finished, with current results. A multi-Goal Topic becomes one HTML page containing each Goal's final reports and web sources. The page reuses the Report Something layout, responsive contents navigation and theme picker. Full Jinkai web fonts load from the existing font CDN, with system font fallback when unavailable. Markdown tables are supported. Mermaid diagrams are rendered to static SVG images with light/dark variants and expandable source; unsupported or oversized diagrams retain their source with a fallback notice. Math source remains code/text.

Each Topic keeps one permanent secure report URL: publishing changed content overwrites that URL in place so shared links keep working, while earlier versions remain in Git history. Retries preserve the URL. Publishing uses an isolated checkout, preserves remote report files and root pages, scans outgoing history for suspected credentials, and never force-pushes. A new empty repository receives a non-listing root page, 404 page and robots policy. Keep the report repository private: random URLs and noindex are not access control. Only final Markdown results and source records are selected; original attachments, intermediate results and execution logs are excluded. Local link targets are removed, but private information written into the report text itself still needs review.

Topic publishing status distinguishes a Git push from a verified live page. If deployment is slow or fails, **Check status** checks the public page without committing or pushing again. A failed push offers **Retry publishing**; unchanged online reports offer **View report**, and changed content offers **Publish update**. The single header action shows a spinner while busy. Interrupted or failed publications require explicit retry. Research completion is unaffected by publishing errors. Site publication is independent of local undo/archive: archiving a Topic does not remove its public pages. Remote deletion and EdgeOne setup are managed separately.

Each publication also maintains a repository-only report index in a managed section of `README.md`: Topic title, random directory and public link. Existing hash-directory reports are indexed from their HTML titles, and other README content is preserved. `edgeone.json` sets `node build-reports.mjs` as the build command and `report-public` as the output directory. The build copies only root entry/404/robots/favicon files, `assets/`, and 32-character lowercase hexadecimal report directories; the README, build script, configuration, and unrelated repository files are excluded. Existing conflicting build settings stop publication for review. EdgeOne must use this Git project's repository root so it reads that configuration. The repository index is refreshed on the next publish or failed-push retry; checking deployment status does not modify the repository.

## Research workflow strategy

See [Research Workflow Strategy](doc/RESEARCH_WORKFLOW_STRATEGY.md) for the maintained English reference covering planning, evidence collection, analysis, report writing and publication. Changes to these policies, prompts or output contracts must update that document in the same change set, including its review date and change history. Distinguish implemented checks from model guidance and future proposals.
