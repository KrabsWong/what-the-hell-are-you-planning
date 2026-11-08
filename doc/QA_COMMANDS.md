# QA and live-verification commands

Developer-facing commands. Offline suites require the `qa` feature, use isolated PID-scoped workspaces, and never appear in a normal user workspace. Live commands call a real model or browser and may consume provider usage.

## Prerequisites

- macOS with the native GPUI host used by the repository.
- Rust toolchain and Cargo.
- Python 3 for the fake CLI fixtures used by several offline suites.
- Git for the report-publishing tests.
- Node.js for the report builder test. The release-tested version is **24.22.2**. The unit test resolves `node` from the process `PATH`; set `PLANNING_NODE` to an explicit executable when the default shell PATH does not contain the supported runtime. The test stays mandatory; it must not be marked ignored.

```bash
export PATH="/opt/homebrew/opt/node@24/bin:$PATH"   # this machine
cargo test --quiet
cargo build --features qa
```

`PLANNING_NODE` is the portable override; do not hard-code a Homebrew path in application code.

## Offline suites

Build once and invoke the dispatch binary directly. Each entry uses an isolated workspace; suites may be rerun with different process IDs. Run them sequentially and do not run them against the user's active app.

```bash
target/debug/what-the-hell-are-you-planning --qa-<entry>
```

All 44 offline `--qa*` entries in `src/main.rs` are listed below. Three entries share one intake-acceptance implementation: `--qa`, `--qa-intake` and `--qa-projects`. `--qa-workspace` is the empty-workspace/theme suite; the onboarding/upgrade suite is `--qa-onboarding`.

| Entry | Runner / purpose |
| --- | --- |
| `--qa` | Intake acceptance; alias of `--qa-intake` |
| `--qa-intake` | Intake acceptance from one sentence to an approved goal |
| `--qa-projects` | Offline alias of the same intake acceptance |
| `--qa-board-topics` | Topic-grouped board directory |
| `--qa-display-order` | Sidebar, List, Board and execution ordering |
| `--qa-board-scroll` | Board scrolling fixture |
| `--qa-workflow` | Drag submission, transitions, dependency chain and screenshots |
| `--qa-rich-markdown` | Markdown/Mermaid/LaTeX rendering frames |
| `--qa-brand` | Brand asset rendering |
| `--qa-attachments` | Paste, drop, original bytes, inheritance and CLI `--file` |
| `--qa-topic-queue` | Topic queue behavior |
| `--qa-retries` | Automatic retry and retry-wait behavior |
| `--qa-render-perf` | Render benchmark; requires `PLANNING_PERF_WORKSPACE` |
| `--qa-result-layout` | Result-first layout and technical-details disclosure |
| `--qa-recovery-scroll` | Recovery list scrolling |
| `--qa-recovery` | Interrupted-session inspection, recovery and resume |
| `--qa-sidebar` | Sidebar navigation and selection |
| `--qa-topic-migration` | Legacy topic migration |
| `--qa-topic-actions` | Goal and topic actions |
| `--qa-topics` | Topic navigation, editing, persistence and manual completion |
| `--qa-calendar` | Hidden date controls and preserved dates |
| `--qa-onboarding` | Workspace onboarding, upgrade/retry and lock transfer |
| `--qa-concurrency` | Research and automation concurrency pools |
| `--qa-execution-eligibility` | Shared automation phase/queue status labels at both window sizes and themes |
| `--qa-simplify` | Compact result-first interaction and toolbar alignment |
| `--qa-automation` | Fake-clock automation creation, scheduling, queues and recovery |
| `--qa-automation-perf` | 43,200-period persistence benchmark; run separately |
| `--qa-batch` | Approved goal batch execution |
| `--qa-publishing` | Offline publishing settings, status labels and verification |
| `--qa-settings` | Agent/model profiles, inheritance, paths and restart |
| `--qa-affordance` | Buttons, links, disabled states and keyboard focus |
| `--qa-workspace` | Empty startup, non-1 project IDs, themes and persistence |
| `--qa-handoff-progress` | Browser handoff progress contract |
| `--qa-research` | Offline research planning, attachments, reruns and failures |
| `--qa-archived-research` | Archived research visibility, restore and retained history |
| `--qa-command` | Typed command executor, durable failure, retry and stop safety |
| `--qa-global-search` | Global search overlay and navigation context |
| `--qa-header-layout` | Header layout regression fixture |
| `--qa-navigation-context` | Exit decisions, dirty drafts and preserved navigation context |
| `--qa-planning` | Automation planning attempt fixture |
| `--qa-research-inputs` | T21 impact confirmation and retained-version reader |
| `--qa-result-presentation` | Typed result presentation and freshness labels |
| `--qa-t11` | T11 interface regression fixture |
| `--qa-task-edit` | T13 real task-editor save/confirm/retry and scoped Undo |

`--qa-render-perf` does not create a workspace; it requires an isolated copy:

```bash
PLANNING_PERF_WORKSPACE=/path/to/isolated/workspace.sqlite3 \
  target/debug/what-the-hell-are-you-planning --qa-render-perf
```

Do not point `PLANNING_PERF_WORKSPACE` at the live workspace. `--qa-automation-perf` creates a large disposable fixture under `.local/automation-perf/<pid>`; run it separately from interaction suites.

## Live opt-ins

These are not part of the offline 33-entry sweep:

```bash
cargo run --features qa -- --materials-live
cargo run --features qa -- --projects-live
cargo run --features qa -- --research-live
```

- `--materials-live`: adds real financial PDFs and screenshots to an existing local research task and runs a real model. Consumes usage.
- `--projects-live`: uses the saved Plan model in an isolated `.local/project-live-qa/` workspace. Consumes model usage.
- `--research-live`: runs a real provider and browser against an explicitly prepared local brief.

Do not run these as substitutes for offline acceptance. Do not restart or operate the user's live app for a test result.

## Optional ignored tests

The default `cargo test --quiet` ignores two explicitly opt-in tests. Run them only when their prerequisites exist:

```bash
cargo test --quiet research::tests::live_planner_contract -- --ignored --exact
WTHAYP_TEST_SNAPSHOT=/path/to/disposable/copy \
WTHAYP_TEST_ORIGINAL_ROOT=/original/workspace/root \
  cargo test --quiet sqlite_storage::tests::local_workspace_snapshot_roundtrip_and_relocation -- --ignored --exact
```

- `live_planner_contract` calls a configured local model.
- `local_workspace_snapshot_roundtrip_and_relocation` requires a prepared disposable snapshot and must never reopen the live workspace.

## Failure categories

When a suite fails, classify the output before changing code:

- `[dependency]` — missing or unsupported external runtime (for example Node, Python or Git).
- `[fixture]` — test state no longer matches a current control or route.
- `[timeout]` — a bounded wait elapsed without its documented precondition being met.
- `[product]` — a current user-visible behavior violates the accepted contract.

Keep assertions and the failing scenario. Do not raise a timeout to hide a fixture or product failure.

## Supplemental focused entries

These are additional offline entries added during the current optimization work. They do not replace or renumber the original 33-entry baseline.

```bash
target/debug/what-the-hell-are-you-planning --qa-header-layout
target/debug/what-the-hell-are-you-planning --qa-result-presentation
target/debug/what-the-hell-are-you-planning --qa-execution-eligibility
target/debug/what-the-hell-are-you-planning --qa-planning
target/debug/what-the-hell-are-you-planning --qa-global-search
```

- `--qa-header-layout`: deep-report breadcrumb containment at 1000 and 1440 widths in both themes, including sidebar resizing and a downsized window.
- `--qa-result-presentation`: typed result presentation states across ordinary and automation runs.
- `--qa-execution-eligibility`: eligibility reason consistency across dispatch and display.
- `--qa-planning`: planning ownership/revision and stale-response boundaries.
- `--qa-global-search`: footer/⌘K search from every origin, domain filter, exact research/automation/period selection, deleted targets, dirty-form preservation, and Escape/focus restoration at 1440×920 and 1000×680 in both themes.
