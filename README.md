# What the hell are you planning?

A local-first desktop workspace for turning research questions into actionable plans, agent-powered tasks, and readable reports.

Define what you want to understand, let an agent propose a plan, and decide which tasks should move forward. Follow the work from evidence collection through analysis to a final report, with the sources, materials, and execution history available when you need them.

## Plan with an agent, stay in control

- **Projects and research goals.** Organize related questions into projects and break each goal into evidence collection, analysis, and reporting tasks.
- **Review before execution.** Agent-generated project topics land in **Backlog**. Move the topics you approve to **Todo**, then start their research when you are ready.
- **Editable plans.** Refine task descriptions, scope, dependencies, and supporting materials before running the next step.
- **Project-wide execution.** Start the current project's approved Todo work in one action. Existing valid results are reused, and dependent steps wait for their prerequisites.

## Execute through local OpenCode

The application uses your locally installed and configured **OpenCode** to plan and execute tasks. It discovers the models available in your OpenCode configuration and lets you choose a global default or override the model for individual stages.

Independent tasks can run concurrently, with up to three execution slots. Tasks sharing a research goal, browser Space, or overlapping working directory are queued to avoid conflicting changes. Queue messages explain what a task is waiting for.

Interrupted or unsuccessful work can reuse an eligible existing OpenCode session when started again. Resumption depends on compatible execution settings and working directory; a restart does not blindly replay all pending work. Run history retains the model, session, inputs, logs, and outputs for each attempt.

## Research with sources and context

The research workflow is designed for questions such as industry trends, market developments, and investment opportunities:

1. Define a research goal and its scope.
2. Generate and review the research steps.
3. Gather evidence using the local **ego(lite)** browser integration.
4. Analyze the findings and produce a final report.
5. Review the report, inspect sources, and add information for a follow-up run.

Browser work takes place in a dedicated ego(lite) **Space**, where you can inspect the pages and take over when necessary. The browser opens in its own window. Login, permission, and other user-intervention requirements remain visible rather than being silently bypassed.

Final reports are given priority in **Progress & results**. Supporting evidence and analysis remain accessible, while source lists and technical execution details can be expanded on demand. Reports render as readable Markdown with tables and links.

## Bring your own materials

Attach PDFs or screenshots to a goal or a research step. The application copies the files into the workspace and extracts text locally using PDF extraction and OCR on macOS. Parent-goal materials and relevant upstream results accompany downstream tasks.

When descriptions, comments, materials, or upstream results change, affected results are marked for refresh. Subsequent execution can update the analysis and final report using the revised context.

Extracted text is sent to the selected model provider. OCR and text extraction have limits, particularly for charts and image-only information; partial extraction is identified rather than treated as a complete reading.

## A workspace built around tasks

- **List and board views** with status filters, project grouping, and colored board cards.
- **Expandable project trees** with research goals and their related steps, plus identifiers that distinguish goals, evidence, analysis, and reports.
- **Resizable navigation panels** and structural breadcrumbs for moving between projects, tasks, and results.
- **Priorities, labels, due dates, comments, activity history, and archiving.**
- **Creation timestamps** shown to the second where recorded.
- **A compact weekly calendar** with previous/next-week navigation, a Today shortcut, and date-based due-task filtering.
- **Light, dark, and system appearance**, shared across the application.
- **Live progress and logs**, with clear running, queued, interrupted, and failed states.

## Your data stays in a chosen workspace

On first launch, choose a workspace root before continuing. Projects and their generated artifacts are managed underneath that root, so agent runs have an explicit working directory.

Structured application data is stored locally in **SQLite**. Attachments and generated research files remain ordinary files alongside the database. The workspace preserves projects, tasks, configuration, dependencies, run history, and source records. A small application preference records the workspace location.

For a complete backup, quit the application and copy the entire workspace root, not just the database. There is no built-in cloud synchronization. OpenCode authentication and browser profiles are managed by those tools separately, so moving the workspace alone does not transfer their credentials or environments.

## Current scope

This is an early desktop application, currently validated on Apple Silicon macOS. It uses Rust and GPUI / gpui-kit for its native interface. Local OpenCode and ego(lite) are separate prerequisites for agent execution and browser-based research.

The application provides research workflows, not trading or order execution. Generated conclusions and source records still require human review. A project working directory organizes agent output; it is not a filesystem sandbox.

## License

[GNU General Public License v3.0](LICENSE)
