# First-round delivery decision

## Current correction notice — user commits through 98b71d4

[POST_DELIVERY_REVIEW.md](POST_DELIVERY_REVIEW.md) supersedes the broad UX acceptance below for ordinary completed-result navigation, asynchronous report loading, visual hierarchy/layout and sidebar scrolling. The user's corrections reveal missed common cases and incorrect review prescriptions; they are not all new requirements or rare edge cases. Current source and added QA assertions were reviewed, but this documentation update did not independently rerun tests or native acceptance. V01–V06 define the remaining targeted verification. Historical passes remain valid only for their recorded source and scope.

Date: 2026-11-09. Branch: `feature/review-2026-11-08-optimization`.

**Historical decision at aee91f9; broad UX acceptance superseded after user corrections.** The r210 report recorded dispositions for T01–T25. Its broad no-wrong-destination/UX-completion claim was not justified for the common ordinary-research, loading and layout paths corrected by the user; those paths require V01–V05 in POST_DELIVERY_REVIEW. Unaffected evidence is retained without extending it to the new source. This is not a claim that every external service or environment combination was tested, and it does not deploy or replace the production app.

Authority: [TASKS](TASKS.md). Original findings: [ISSUES](ISSUES.md). Historical coverage: [COVERAGE](COVERAGE.md). Detailed chronology: [IMPLEMENTATION](IMPLEMENTATION.md). This document records the former r210 disposition; [POST_DELIVERY_REVIEW](POST_DELIVERY_REVIEW.md) now governs the corrected UX paths. Original failures remain historical evidence.

## Evidence and source scope

- The [r199 ledger](evidence/r199-offline-ledger.json) records the provenance-verified r194 frozen executable: 53/54 required entries initially passed plus one diagnostic. The remaining obsolete scroll assertion passed [independent r196 replay](evidence/r196-automation-replay.json); it was replaced with actual wheel displacement and navigation/anchor checks, not merely deleted. Effective required coverage is 54/54.
- [r191 unit/integration results](evidence/r191-full-tests.json) remain separately scoped. Later r200 wheel restoration passed its focused independent test, implementer QA suite and [native verification](evidence/r200-compact-results.json). The final suffix-only change passes [14 independent affected tests](evidence/r210-result-row-acceptance.json) and [native transition](evidence/r208-result-row-native.json). A frozen earlier gate is not described as a full rerun of later changes.
- All fourteen required 500/1,000-record live operation p95 gates pass at 4.559–18.755 ms ([r195](evidence/r195-current-live-performance.json)). Native evidence covers representative scrolling, reader/source access, pagination and return anchors, both sizes/themes, and [save overlap with durable reopen](evidence/r203-concurrent-save-scroll.json). Isolated outliers remain F2; this does not promise zero hitches.
- Native horizontal Board verification remains **Blocked by tool input**: [r207](evidence/r207-native-board-input.json) records zero deltas for CUA left/right with a nonzero vertical control. Three independent real rendered horizontal-event tests pass [r205](evidence/r205-board-input-replay.json). Root accepts F1 as a first-round verification deferral; physical-trackpad behavior is not claimed verified.
- Only isolated fixtures were used in recent acceptance. The earlier accidental production schema upgrade and authorized restoration are disclosed in IMPLEMENTATION r145; this report does not erase that incident.

## T01–T25 disposition

Task acceptance follows the first-round policy in TASKS. Individual checks remain Passed, Blocked or Not run; accepting a named verification deferral does not convert it to a pass.

| Task | Status | Concrete evidence | Residual |
| --- | --- | --- | --- |
| T01 | Passed | [r199 ledger](evidence/r199-offline-ledger.json): 53/54 required pass; [r196 replay](evidence/r196-automation-replay.json): exit 0 replaces only the obsolete assertion | Root accepts the r196 replacement; original failure retained |
| T02 | Passed | [T02 native state](evidence/T02-native-plan-state.json), T02 completion (local screenshot; excluded from Git): plan run succeeded while Automations stayed current | None |
| T03 | Passed | [T03 retry state](evidence/T03-native-retry-state.json): schedule paused while same occurrence retried; screenshot (local screenshot; excluded from Git) | Optional retry smoke deferred |
| T04 | Passed | [r143 Undo audit](evidence/r143-live-undo-write-audit.json): exact automation/task rows only; history tables untouched | None |
| T05 | Passed | [T05 visible QA](evidence/r210-retained-checks.json), [T05 automation](evidence/r210-retained-checks.json) | None |
| T06 | Historical acceptance superseded for corrected paths | [T06 result matrix](evidence/r210-retained-checks.json): reader/copy/export parity; typed states; [r208 native transition](evidence/r208-result-row-native.json) | C01/C03; V01/V02 required. The r208/r210 row transition did not prove reader loading behavior. |
| T07 | Historical acceptance superseded for corrected paths | [T07 sidebar](evidence/r210-retained-checks.json), T07 recent (local screenshot; excluded from Git) | C01; V01 required for ordinary result-first navigation after restart. |
| T08 | Passed | [T08 search](evidence/r210-retained-checks.json), [global search](evidence/r210-retained-checks.json) | Route×shortcut matrix deferred |
| T09 | Passed | [T09 unit checks](evidence/r210-retained-checks.json); T09 new defaults (local screenshot; excluded from Git), minute reset (local screenshot; excluded from Git) | None |
| T10 | Historical acceptance superseded for corrected paths | [T10 navigation](evidence/r210-retained-checks.json), Back preserves (local screenshot; excluded from Git) | C02; V02 required for atomic pending destinations and cancellation. |
| T11 | Historical acceptance superseded for corrected paths | [T11 state matrix](evidence/r210-retained-checks.json): 1440/1000, light/dark | C04–C08; V03/V04 required. The permanent-border prescription is withdrawn. |
| T12 | Historical acceptance superseded for corrected paths | [T12 architecture audit](evidence/T12-reviewed-navigation-architecture-audit.md), [T12 unit](evidence/r210-retained-checks.json) | C02; V02 required. Typed visible routes did not cover pending navigation intent. |
| T13 | Passed | [T13 scoped edit](evidence/r210-retained-checks.json), [task edit](evidence/r210-retained-checks.json) | None |
| T14 | Passed | [r199 live](evidence/r199-offline-ledger.json), [r195 live detail](evidence/r195-current-live-performance.json) | 43k/86k stress non-gating |
| T15 | Passed | Same live entry; [r195](evidence/r195-current-live-performance.json) history p95 10.5–12.5 ms; indexed rows | 30k-body XL non-gating |
| T16 | Passed | [T16 arithmetic](evidence/r210-retained-checks.json), [T16 schedule](evidence/r210-retained-checks.json) | None |
| T17 | Passed | [TASKS](TASKS.md), [DESIGN](../../DESIGN.md), [ROUTE_ACTION_MATRIX](../ROUTE_ACTION_MATRIX.md) | Stale status-table wording |
| T18 | Passed | [T18 policy](evidence/r210-retained-checks.json), Quick rejection (local screenshot; excluded from Git) | Capability is documented intent/validation, not sandbox |
| T19 | Historical acceptance superseded for corrected paths | Passed: [r208 result-row native transition](evidence/r208-result-row-native.json), [compact results r200](evidence/r200-compact-results.json), [compact reader/settings r201](evidence/r201-compact-reader-settings.json), [material/settings r199](evidence/r199-compact-material-settings.json), [ordinary List r197](evidence/r197-ordinary-native.json), [compact List r198](evidence/r198-compact-native.json), [large Light reader r204](evidence/r204-large-light-reader.json), [concurrent save r203](evidence/r203-concurrent-save-scroll.json), [keyboard r143](evidence/r143-native-focus.json), [count nav r156](evidence/r156-count-navigation-review.json) | C05–C09; V03–V05 required on the new source/profile; earlier native horizontal input boundary retained. |
| T20 | Historical acceptance superseded for corrected paths | Same evidence; feature-family rows below | V01–V06 must be recorded before a new integrated UX sign-off. |
| T21 | Passed | [T21 long history](evidence/r210-retained-checks.json), [T21 native cancel](evidence/r145-native-impact-cancel.json) | Post-Undo wording |
| T22 | Passed | [T22 archive](evidence/r210-retained-checks.json), T22 compact (local screenshot; excluded from Git) | Archived upstream-wait sentence |
| T23 | Passed | [T23/T25 requests](evidence/r210-retained-checks.json), planning attempt (local screenshot; excluded from Git) | >2 replans/legacy mixes |
| T24 | Passed | [T24 onboarding](evidence/r210-retained-checks.json): reconnect/relocation with real Quick and Research automations | None |
| T25 | Passed | [T25 negative](evidence/r210-retained-checks.json), [T23/T25 planning](evidence/r210-retained-checks.json) | Concurrent-edit timing variants |

## T20 feature-family cases

| Family / case | Status | Evidence | Residual |
| --- | --- | --- | --- |
| Workspace: new/open, invalid folder, ordinary and automation reconnect/relocation | Passed | [T24 onboarding](evidence/r210-retained-checks.json), [r199 ledger](evidence/r199-offline-ledger.json) | Physical disk-full and exhaustive native lock timing Not run; offline persistence/recovery and r203 controlled pending write passed |
| Research: creation, planning, approval, execution, saved request, retry | Passed | [r199 ledger](evidence/r199-offline-ledger.json), [r180 lifecycle](evidence/r180-lifecycle-native.json), [r196 replay](evidence/r196-automation-replay.json) | Optional queue/stop cross-products |
| Research List: ordinary Done 3/3, bidirectional scroll, both themes; compact and large | Passed | [r197 ordinary](evidence/r197-ordinary-native.json), [r198 compact](evidence/r198-compact-native.json) | None required |
| Research Board: Done reachability and horizontal input | **Blocked** | [r202 retry](evidence/r202-board-retry.json), [r207 probe](evidence/r207-native-board-input.json), [r205 event replay](evidence/r205-board-input-replay.json) | F1 zero-delta native input; no product fix justified |
| Automation: Quick/Research, schedule, draft/edit/replan/enable, pause/resume, manual/retry/recovery | Passed | [r199 ledger](evidence/r199-offline-ledger.json), [r196 replay](evidence/r196-automation-replay.json), [r180 lifecycle](evidence/r180-lifecycle-native.json) | Schedule/timezone/DST/sleep variants deferred |
| Navigation: sidebar, breadcrumbs/Back, period/step return, search | Passed | [r143 focus](evidence/r143-native-focus.json), [r156 count nav](evidence/r156-count-navigation-review.json), [r140 period return](evidence/r140-native-period-return.json) | Exhaustive cross-products deferred |
| Results: compact/large reader, sources, history/filter/read, Close/return, lazy-row transition | Passed | [r200](evidence/r200-compact-results.json), [r201](evidence/r201-compact-reader-settings.json), [r204 large Light](evidence/r204-large-light-reader.json), [r181](evidence/r181-native-capacity.json), [r208 native transition](evidence/r208-result-row-native.json) | [r210 portable tests](evidence/r210-result-row-acceptance.json) and native transition passed; F3 retains pre-hydration quality refinement |
| Settings/materials: agent/models/profiles, material grid/card, automation Settings | Passed | [r199 material/settings](evidence/r199-compact-material-settings.json), [r201 automation settings](evidence/r201-compact-reader-settings.json), [r189](evidence/r189-material-settings-native.json) | Paste/drop edge cross-products deferred |
| Archive/publication: archive/restore; local publication states | Passed | [T22 archive](evidence/r210-retained-checks.json), [r181 archive readers](evidence/r181-native-capacity.json), [r199 ledger](evidence/r199-offline-ledger.json) | Real remote publication **Not run** (F8) |
| Visual/keyboard: 1440/1000, light/dark, keyboard/focus | Passed | [T11 state](evidence/r210-retained-checks.json), [r143 focus](evidence/r143-native-focus.json), [r199 material/settings](evidence/r199-compact-material-settings.json) | Long/CJK/palette cross-products deferred |
| Concurrent save + scroll | Passed | [r203 concurrent save](evidence/r203-concurrent-save-scroll.json): Saving visible while goal27→63, same anchor after Saved, durable `system`, integrity `ok` | One overlap is sufficient first-round |

## Follow-up tasks retained explicitly

These are nonblocking first-round residuals, not secretly completed work. Dependencies below identify prerequisites for each follow-up, not requirements to rerun the entire first-round program.

| ID | Trigger and user impact | What and how | Acceptance | Dependencies |
| --- | --- | --- | --- | --- |
| F1 | CUA left/right emits zero delta; physical Board horizontal behavior lacks native proof | Use a physical trackpad or verified nonzero native event source; check header, empty column and card regions in both sizes/themes. Change product routing only if a delivered nonzero event fails | Done becomes reachable and return scrolling works; preserve r205/r207 counterevidence. Until then status is Blocked, never native Passed | Working input source; T19 |
| F2 | Occasional first-use result/reader frame outliers; no sustained stall established | Retain T19-F1/F2 measurements from r188/r190; profile repeated ordinary use at ≤1,000 records and optimize measured UI-thread work only | No sustained/repeated stall, wrong displacement or lost anchor; compare distributions and outliers, not averages | T14/T15; quiet capture fixture |
| F3 | Before first Open a durable body says saved report; exact quality is unknown until hydration | Persist a versioned lightweight quality summary with the body; invalidate it consistently and read it in indexed history rows without loading all bodies | Initial row identifies final/intermediate/quality correctly, matches reader after reload, preserves list performance | T06/T15; metadata migration design. Current availability→quality transition passed r208/r210 |
| F4 | Publishing Off toggle and low-frequency lifecycle controls can look too strong or ambiguous | Revisit action emphasis using existing secondary/icon/More conventions; recheck enabled/paused/draft/recovery at both sizes/themes | Actions distinguish scope and priority at rest, statuses do not resemble actions, controls remain reachable | T11; existing visual evidence r189/r199 |
| F5 | Archived upstream-wait and post-Undo wording can imply current work; older status tables can lag | Replace those bounded phrases with historical/current-context wording and reconcile maintained tables; preserve historical evidence sections | Archive never suggests admission to live queues; Undo wording matches effective approval; current docs match behavior | T17/T21/T22; archive and invalidation checks |
| F6 | Additional route×dirty-exit, multi-goal neighbor, >2 replan, legacy-mix, concurrent-edit and long-title cross-products not exhaustively run | Add focused cases only when changing those routes/revisions; retain same-domain neighbors and frozen ownership assertions | No lost draft/context, cross-domain neighbor or revision rebinding; record exact new combinations | T07/T08/T10/T23/T25 |
| F7 | OS sleep/DST, physical disk-full and exhaustive lock timing variants not natively injected; XL stress is outside required scale | Use disposable controlled environments and separate 43k/86k/30k-body stress diagnostics. Existing offline recovery, crash and schedule checks remain passed | Named failure yields truthful error/recovery without duplicate execution or lost durable records; report each environment separately | T14/T15/T16; controlled test host |
| F8 | Real remote publication was Not run; capability enforcement is intent validation, not a sandbox | Only in a separately authorized disposable remote, verify exact-version publish and error/retry. If sandboxing becomes a requirement, design an actual permission boundary separately | Only reviewed version reaches authorized target; no claim of external-write isolation without enforcement tests | T18/T20; explicit destination authorization |
| F9 | Additional navigation payload relocation and large-view splitting exceed the accepted extraction | Continue typed route/command ownership extraction when changing those modules; avoid cosmetic splitting without a boundary | Narrow state ownership, no duplicated route transitions, current navigation/Undo tests preserved | T12/T13; no first-round dependency |

## Explicit case-level boundaries

The feature-family table above combines native and automated evidence. It is not a claim that each listed case was clicked natively. The [full offline ledger](evidence/r199-offline-ledger.json) covers workspace/recovery/live-persistence, intake/research/planning, batch/retries/concurrency, automation/schedule, navigation/search/sidebar, result-presentation/layout, settings/attachments, archive/publishing and keyboard/layout entries. Native routes and actual resulting states are documented in the linked evidence and original coverage ledger.

- Workspace save failure/retry, lock/recovery, backup/restore: automated checks Passed; concurrent pending save and durable reopen Passed natively (r203). Physical disk-full and all failure-timing cross-products Not run (F7).
- Research global/project/topic intake, saved request, planning failure/retry, approval/drag, queue/stop/resume/selective rerun: relevant offline entries Passed. Native ordinary completion and List inspection are retained; exhaustive drag/stop timing combinations Not run (F6).
- Automation schedule kinds/timezone previews and queued/failed/skipped/recovering states: offline automation/schedule/state checks Passed. Native Quick/Research creation and lifecycle/replan evidence retained. Physical sleep/DST crossings Not run (F7).
- Navigation Previous/Next, recents, sidebar, breadcrumbs/Back, period-step return, Search and shortcuts: route tests Passed plus native focus/return evidence. Exhaustive route×shortcut×dirty-state combinations Not run (F6).
- Results final/intermediate/live/partial/stale, copy/export, regeneration and technical details: typed-presentation and reader parity checks Passed; native reader/source/Close and r208 status transition Passed. All format/state cross-products Not run (F6).
- Settings/materials inheritance/profiles, paths, paste/drop/remove and missing-config explanations: relevant settings/attachment/input checks Passed; native import/material grid/preview/settings evidence retained. Exhaustive OS clipboard/drop/error combinations Not run (F6/F7).
- Archive/restore and local exact-version publication: automated checks Passed and archive native evidence retained. Real remote publication Not run (F8).

## Historical sign-off (superseded for corrected UX paths)

The historical r210 decision accepted the implemented first-round scope and F1–F9 as the named follow-up work permitted by the user. T19 physical horizontal proof stays Blocked at case level; T20 accepts that explicit boundary rather than calling it passed. No failing affected test remains unexplained. Screenshots and raw runtime traces stay local; repository evidence consists of review summaries, hashes and test records. Future changes must rerun their affected checks and update this disposition.
