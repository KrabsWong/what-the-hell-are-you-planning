# QA and live-verification commands

Developer-facing commands. They require the `qa` feature, use isolated workspaces, and never appear in a normal user workspace. Some commands call a real model or browser and may consume provider usage.

```bash
cargo run --features qa -- --materials-live
cargo run --features qa -- --projects-live
cargo run --features qa -- --qa-display-order
cargo run --features qa -- --qa-workflow
cargo run --features qa -- --qa-projects
cargo run --features qa -- --qa-affordance
cargo run --features qa -- --qa-workspace
cargo run --features qa -- --qa-attachments
cargo run --features qa -- --qa-research
cargo run --features qa -- --qa-automation
```

- `--materials-live`: adds real financial PDFs and screenshots to an existing local research task and runs a real model. Consumes usage; not a normal launch command.
- `--projects-live`: uses the saved Plan model in an isolated `.local/project-live-qa/` workspace to verify project planning and step planning for approved topics. Consumes model usage and does not run a full market study.
- `--qa-display-order`: verifies sidebar, List, Board and execution grouping order with new content first.
- `--qa-workflow`: verifies drag submission, duplicate-submission rejection, illegal transitions, configuration failure rollback, full dependency-chain execution, and board/list/detail/step screenshots.
- `--qa-projects`: verifies project creation, Kanban approval and dragging, unapproved blocking, a full test research chain, stale/failed/cancelled results, reopening and both themes.
- `--qa-affordance`: verifies buttons, links, disabled states and keyboard interaction at 1440 and 1000 width in light and dark themes without calling a model.
- `--qa-workspace`: verifies empty data, themes and project selection.
- `--qa-attachments`: verifies image paste, file drop, original file bytes, attachment inheritance and CLI `--file` delivery without calling a model.
- `--qa-research`: uses the test CLI to verify interaction, dependencies and failure paths, including original PDF/image delivery through `--file` without local extraction.
- `--qa-automation`: uses a fake clock and fake CLI in an isolated workspace to verify automation creation, enabling, scheduling, catch-up, recovery, queues, results and light/dark screenshots without calling a model.
