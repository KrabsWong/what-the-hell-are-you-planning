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

The original 49-entry standalone offline baseline is listed below: 45 in the table and 4 named after it. Additional entries are documented under Supplemental focused entries and in the [application scrolling](qa/APPLICATION_SCROLL.md) and [automation dialog scrolling](qa/AUTOMATION_DIALOG_SCROLL.md) guides. The two crash-child modes are excluded from this list. Three entries share one intake-acceptance implementation: `--qa`, `--qa-intake` and `--qa-projects`. `--qa-workspace` is the empty-workspace/theme suite; the onboarding/upgrade suite is `--qa-onboarding`.

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
| `--qa-automation-perf` | Legacy attachment-latency diagnostic at 1,000 periods; correctness exit still gates |
| `--qa-live-performance` | Required live-backend 500/1,000 p95 gate; 50 ms per-operation threshold |
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

Four standalone entries are not repeated in the table: `--qa-dispatch-admission`, `--qa-keyboard-visual`, `--qa-live-persistence` and `--qa-set-read`. They are part of the release ledger; `--qa-live-performance` is in the table as the required p95 gate.

`--qa-render-perf` requires a closed, frozen, isolated workspace copy. It rejects WAL or journal sidecars; checkpoint and close the disposable source before copying it. Version 4 is loaded through a private temporary copy, leaving the supplied fixture unchanged:

```bash
PLANNING_PERF_WORKSPACE=/path/to/isolated/workspace.sqlite3 \
  target/debug/what-the-hell-are-you-planning --qa-render-perf
```

Do not point `PLANNING_PERF_WORKSPACE` at the live workspace. `--qa-automation-perf` creates a 1,000-period disposable fixture under `.local/automation-perf/<pid>/` and records one legacy attachment save-block sample. That sample is a non-gating diagnostic; only its nonzero process exit gates correctness. Required responsiveness is `--qa-live-performance`, which runs the 500- and 1,000-occurrence live fixtures and fails on any per-operation p95 at or above 50 ms.

## Release gate runner

`script/run_review_gate.py` runs the frozen-candidate standalone ledger. The runner writes only its own `--output` directory; existing index/evidence directories are read-only inputs and their raw measurements are not rewritten.

```bash
python3 script/run_review_gate.py --self-test
python3 script/run_review_gate.py \
  --output /path/to/new/gate \
  --source-root /path/to/frozen/source \
  --binary /path/to/reviewed/candidate \
  --expected-sha256 <64-hex-digest> \
  --fixture-db /path/to/closed-v4-fixture.sqlite3
```

- `--self-test` runs in-memory classification checks only; it launches no QA entry.
- `--qa-automation-perf` keeps every raw `ui_save_block_ms` and fixture-period count in `index.json`, classifies the sample as `diagnostic`, and does not gate on its latency.
- `--qa-live-performance` is required. The runner checks the explicit 50,000 µs target, actual 500/1,000 fixture occurrence counts, every required operation, and each numeric, finite, nonnegative `p95_us` strictly below 50,000. Missing coverage, nonnumeric/boolean values or any failed gate fails the ledger closed; the final result list must also contain the live entry.
- The final `GATE_SUMMARY` distinguishes `passed`, `passed_with_non_gating_diagnostics` and `failed`; a diagnostic is never reported as a pass. Any functional failure or required live-performance failure still exits nonzero.

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

`--qa-workspace-menu` checks workspace-menu open state, Escape and trigger dismissal,
rapid reversal, and reduced motion. It captures light/dark rotation frames at the
compact window size in an isolated workspace.

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
- `PLANNING_REFRESH_PREVIEW=1 target/debug/what-the-hell-are-you-planning --qa-result-presentation`: opens an interactive isolated app with a completed three-step goal loaded lazily from a synthetic SQLite fixture, for checking the absence of false **Needs refresh** labels. It does not open the user's workspace or start scheduled work.
- `--qa-execution-eligibility`: eligibility reason consistency across dispatch and display.
- `--qa-planning`: planning ownership/revision and stale-response boundaries.
- `--qa-global-search`: footer/⌘K search from every origin, domain filter, exact research/automation/period selection, deleted targets, dirty-form preservation, and Escape/focus restoration at 1440×920 and 1000×680 in both themes.

## Native Liquid Glass preview

Normal startup leaves Liquid Glass off unless enabled in Settings > General.
The switch is available only on macOS 26+ native windows without Reduce
Transparency; it uses the existing workspace and saved theme. Use `./script/build_and_run.sh` to build and restart the real app.

On macOS 26 or newer, run `./script/preview_native_glass.sh`. It builds a separate
`PlanningGlassPreview.app`, opens sample data without reading the real workspace
or starting agents, and leaves the normal app running. The sidebar, top
navigation header, bottom status bar and workspace gutters use one regular-style
native `NSGlassEffectView`; the rounded body uses a dense translucent wash. Inspect both active
and inactive windows over a text-heavy background to check readability and the
exposed material around the body corners.

- `./script/preview_native_glass.sh --glass-dark` starts in dark mode for reading-surface comparison.
- `./script/preview_native_glass.sh --glass-opaque` opens the same layout with the original opaque shell and body for comparison.
- `./script/preview_native_glass.sh --glass-verify` checks menu opening, Escape dismissal, material layout and forced-opaque fallback while exercising light, dark and compact states, and verifies the Settings switch, immediate updates and persisted values, then exits. Visual review is manual; this command does not capture the screen.
- Preview logs and isolated sample workspaces live under `.local/native-glass/`.
- Interactive previews refresh Reduce Transparency every 500 ms. Older systems or missing native handles retain opaque navigation. The forced-opaque case tests the fallback without changing system preferences.
- Headless snapshots cannot verify the native glass layer. Successful testing on a later macOS version does not establish macOS 26 device coverage or scrolling-performance acceptance.
