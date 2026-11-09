# Review validation ledger

Date: 2026-11-08. Source: `611a4586eedb56600a83c8b272a3af43ba95e4a4`.

The review built the QA-feature binary using `cargo run --quiet --features qa -- --qa-automation`. Subsequent offline suites ran through `target/debug/what-the-hell-are-you-planning --qa-<suite>`. Logs below are copied from this review, not old repository fixtures. The fake CLI tests do not establish real-provider output quality.

| Command/suite | Result | Evidence / interpretation |
| --- | --- | --- |
| `cargo test --quiet` | Failed: 289 passed, 1 failed, 2 ignored | [Summary](unit-test-summary.txt); Node missing on PATH, not classified as product failure |
| `python3 script/check_icons.py` | Passed | Tool output: all explicit Lucide references embedded, 51 registered icons |
| automation | Passed | [Log](automation.txt); includes recovery, independent pools and theme/size rendering |
| workflow | Passed | [Log](workflow.txt) |
| projects | Passed | [Log](projects.txt); current command delegates to intake acceptance |
| settings | Passed | [Log](settings.txt); profiles/inheritance/restart and both themes/sizes |
| attachments | Passed | [Log](attachments.txt); original bytes, paste/drop/inheritance |
| retries | Passed | [Log](retries.txt) |
| topic-queue | Passed | [Log](topic-queue.txt) |
| batch | Passed | [Log](batch.txt) |
| affordance | Failed | [Log](affordance.txt); approval element not visible, fixture/layout diagnosis needed |
| research | Failed | [Log](research.txt); obsolete/missing `research` element |
| workspace | Failed | [Log](workspace.txt); missing `save-task` element |
| recovery | Failed | [Log](recovery.txt), [isolated rerun](recovery-rerun.txt); retry does not finish within fixture deadline |
| publishing | Failed | [Log](publishing.txt), [isolated rerun](publishing-rerun.txt); actual View report versus expected Retry publishing |
| automation-perf | Completed | [Log](perf.txt); 43,200 periods, 172.4 MB, 1,350 ms UI-triggered synchronous save |

The recovery and publishing failures reproduced on a second sequential invocation. Root causes remain unclassified; do not treat them as harmless flakes or as proven production bugs. No source was changed to force tests green. Eight of thirteen functional QA suites passed; five failed. Performance and icons are separate checks.

## Retained visual evidence

These are newly generated **isolated QA screenshots**, not screenshots of the user's private live workspace. Titles are quoted fixture content. Sizes in filenames are logical window sizes; PNG pixel sizes are doubled by display scaling. All four were visually inspected during this review.

- [Automation detail, light, 1440×920](automation-detail-light-1440.png): clear main lifecycle row, low-discoverability cleanup action and contradictory catch-up footer.
- [Automation detail, dark, 1440×920](automation-detail-dark-1440.png): neutral history rows and theme consistency; trigger meaning remains icon-only.
- [Automation results, dark, 1000×680](automation-results-dark-1000.png): truncation and repeated action identity; small-window layout remains usable in this fixture.
- [Automation list, light, 1000×680](automation-list-light-1000.png): two-line summary and separate definition/unread metadata; long-title disclosure requires explicit acceptance.

Live Computer screenshots and UI observations remain in the review conversation. They were not copied into the repository. The original live instance had inconsistent screenshots and reported UI state and `noWindowsAvailable`. The follow-up used a uniquely identified isolated native app successfully; see [the full coverage ledger](../COVERAGE.md). Native observations are retained in the conversation, separately from these headless screenshots.

## Reproduction safety and interpretation

Use isolated workspaces and fake CLI commands for regression. Do not run `--materials-live`, `--projects-live` or `--research-live` as substitutes for these offline suites without an intentional live-provider test. Do not restart the user's working instance to resolve a test-tool limitation. A source finding is distinct from a live reproduction, and a generated screenshot is distinct from keyboard/pointer acceptance.

## Full-coverage follow-up

All 33 offline `--qa*` dispatch entries in `src/main.rs` were invoked. Final result: **26 passed / 7 failed**, including the performance entries and aliases. This is entry-point coverage, not a claim that all functional cases pass. `--qa`, `--qa-projects` and `--qa-intake` delegate to intake acceptance. Real-provider opt-ins were not substituted for offline tests.

| Additional entry | Final outcome | Evidence |
| --- | --- | --- |
| board-topics | Passed | [board-topics.txt](board-topics.txt) |
| display-order | Passed | [display-order.txt](display-order.txt) |
| board-scroll | Passed | [board-scroll.txt](board-scroll.txt) |
| rich-markdown | Passed | [rich-markdown.txt](rich-markdown.txt) |
| brand | Passed | [brand.txt](brand.txt) |
| render-perf | Passed after supplying PLANNING_PERF_WORKSPACE | [render-perf-rerun.txt](render-perf-rerun.txt) |
| result-layout | Passed | [result-layout.txt](result-layout.txt) |
| recovery-scroll | Passed | [recovery-scroll.txt](recovery-scroll.txt) |
| intake | Passed | [intake.txt](intake.txt) |
| sidebar | Passed | [sidebar.txt](sidebar.txt) |
| topic-migration | Passed | [topic-migration.txt](topic-migration.txt) |
| topic-actions | Passed | [topic-actions.txt](topic-actions.txt) |
| topics | Failed | [topics.txt](topics.txt) |
| calendar | Passed | [calendar.txt](calendar.txt) |
| onboarding | Passed | [onboarding.txt](onboarding.txt) |
| concurrency | Passed | [concurrency.txt](concurrency.txt) |
| simplify | Failed | [simplify.txt](simplify.txt) |
| handoff-progress | Passed | [handoff-progress.txt](handoff-progress.txt) |
| qa (alias) | Passed | [qa-alias.txt](qa-alias.txt) |

New failures: `topics` expects an ordinary child of a research goal to become Done, but it remains Planned (`topic_qa.rs:96`). `simplify` expects identical vertical position for the technical-details return control; actual 79.5 px versus expected 81.5 px (`simplify_qa.rs:202`). These are retained for T01 classification. The initial render-perf invocation lacked its documented environment argument; rerunning with the isolated review workspace passed at 2.06–3.17 ms/frame for its small dataset. This does not supersede the 43,200-period persistence benchmark.

Native ZIP export created `P2-G1_Review_research_goal.zip` in the isolated data root. Native restart preserved projects, themes, profiles, automations and histories. The fixture intentionally lacks a recoverable real provider session; the interrupted ordinary run remained guarded by session checking, so successful provider-session recovery is not claimed.

The [machine-readable entry-point index](qa-entrypoint-index.json) maps all 33 dispatch flags to a final outcome and retained log. Native harness modifications are limited to main.rs and workspace.rs plus package identity; no production UI/domain implementation was changed.
