# What the hell are you planning?

A local-first desktop workspace for turning research questions into actionable plans, agent-powered tasks, and readable reports.

Define what you want to understand, let an agent propose a plan, and decide which tasks should move forward. Follow the work from evidence collection through analysis to a final report, with the sources, materials, and execution history available when you need them.

<img width="600" alt="image" src="https://github.com/user-attachments/assets/37aabf1a-e888-4e5b-997c-ff976864475b" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/147fe163-ffd1-4781-a401-4360345509bf" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/88d27ead-cd99-40a3-8bc8-2145432c117d" />

## Plan with an agent, stay in control

- **Start with one sentence.** Use **New research** in a Project or Topic. The original request and submission time are saved before planning; failed or interrupted planning can be retried from **Market research → Saved research requests**. Relative dates stay anchored to the original request.
- **Required Topics.** The structure is Project → Topic → Goal → research steps. An explicit Topic is reused; otherwise the planner proposes a new Topic. Older unclassified roots move into **待归类** without losing results. Topics organize content and do not automatically share research context.
- **Complete plans before approval.** The planner expands the request into an objective, deliverable, scope, assumptions, facts to verify, and independent Goals with complete dependency plans. Goals enter **Backlog**. Planning never starts the research.
- **Goal-wide review.** The board groups Goal cards by Topic. Internal steps appear inside Goals, not as separate approval cards. Move a Goal to **Todo** to approve its current plan, then click **Run goal** or **Run / resume**. Dragging alone never executes work.
- **Versioned approval.** Changes to research scope, step instructions, dependencies, comments or materials return approved research to Backlog. Existing runs remain available; reapproval and an explicit start reuse valid evidence and refresh affected downstream results.

## Execute through local OpenCode

The application uses your locally installed and configured **OpenCode** to plan and execute tasks. It discovers the models available in your OpenCode configuration and lets you choose a global default or override the model for individual stages.

Approved goals can be added to a FIFO topic queue even when all execution slots are occupied. One topic runs at a time, with up to three dependency-ready steps running concurrently within it. Exhausted retries or blocked goals retain their status while later topics continue. New request-generated Goals use separate step folders and browser Spaces. Shared sessions, Spaces and overlapping folders remain serialized; legacy research preserves its existing workspace. A failed branch blocks its dependents while independent approved work can continue.

Interrupted or unsuccessful work can reuse an eligible existing OpenCode session when started again. Resumption depends on compatible execution settings and working directory; a restart does not blindly replay all pending work. Run history retains the model, session, inputs, logs, and outputs for each attempt. Report validation failures show the specific reason; automatic retries and manually restarted tasks receive that feedback so the model can correct the rejected output.

## Research with sources and context

The research workflow is designed for questions such as industry trends, market developments, and investment opportunities:

1. Capture a research request in one sentence.
2. Review the complete plan in Backlog, approve selected Goals into Todo, then explicitly start them.
3. Gather evidence using the local **ego(lite)** browser integration.
4. Analyze the findings and produce a final report.
5. Review the report, inspect sources, and add information for a follow-up run.

Browser work takes place in a dedicated ego(lite) **Space**, where you can inspect the pages and take over when necessary. The browser opens in its own window. Login, permission, and other user-intervention requirements remain visible rather than being silently bypassed.

Final reports are given priority in **Progress & results**. Supporting evidence and analysis remain accessible, while source lists and technical execution details can be expanded on demand. Reports render as readable Markdown with tables and links.

While a research step runs, the progress panel lists the tasks in each phase, their statuses, and completion counts. Task details and the latest attempt's technical details are available beside each task. This outline remains visible when the app reconnects to a session still running in the background; completed attempts open their reports.

Small upstream results are included directly in the next task's prompt. When their combined context exceeds 180,000 bytes, the app instead writes complete Markdown reports and source-record JSON files into the task's `.research-inputs` folder. The prompt lists the files and instructs the agent to read them in bounded sections. This threshold selects a transfer method, not a model context limit; reading files still consumes model context. Original results remain in SQLite. Before dispatch, missing copies are regenerated and existing copies are checked against the saved results; modified copies block execution rather than silently changing the evidence.

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

This is an early desktop application, currently validated on Apple Silicon macOS. It uses Rust and GPUI / gpui-kit for its native interface. Local OpenCode and ego(lite) are separate prerequisites for agent execution and browser-based research.

The application provides research workflows, not trading or order execution. Generated conclusions and source records still require human review. A project working directory organizes agent output; it is not a filesystem sandbox.

## License

[GNU General Public License v3.0](LICENSE)

### Local macOS beta package

Run `./script/package_macos.sh` on Apple Silicon to build an optimized release and produce a DMG plus SHA-256 checksum in `target/distribution/`. The disk image includes `Planning.app`, an Applications shortcut, installation instructions, and the license. No workspace or credentials are packaged. OpenCode and ego(lite) remain separate prerequisites.

These beta packages use ad-hoc signing and are **not notarized**. Testers may need to allow the app in macOS Privacy & Security after attempting to open it. For ordinary trusted distribution without this exception, use a Developer ID Application certificate and Apple notarization. The current package is arm64 only; compatibility with older macOS versions has not been fully tested.
