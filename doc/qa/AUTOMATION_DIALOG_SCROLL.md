# Automation result dialog scrolling

The initial small-fixture checks below did **not** resolve the reported live
workspace stall. The follow-up investigation and real-workspace comparison at
the end supersede the initial performance conclusion.

## Cause and change

The result modal was built directly by `TaskApp::render`, outside the retained
content surface. Sidebar logo animation dirtied its ancestor and rebuilt the
modal even when its data had not changed. Each rebuild also computed the result
presentation twice: the header used the unindexed history path, while the body
used the run index. Both paths rebuilt report presentation data.

`AutomationResultSurface` now retains the full-size modal separately. Its single
indexed presentation is shared by the heading and body and reused during wheel
and animation frames. Every parent notification invalidates it, including lazy
report hydration and live status changes. Closing drops the surface. Existing
scroll handles, selection, links, focus trapping and close handlers are retained.

## Reproduce the checks

```sh
cargo test --quiet
cargo build --features qa --quiet
target/debug/what-the-hell-are-you-planning --qa-automation-dialog-scroll
PLANNING_DIALOG_DARK=1 PLANNING_DIALOG_COMPACT=1 \
  target/debug/what-the-hell-are-you-planning --qa-automation-dialog-native
```

Set either native flag to `0` for light mode or 1440 × 920. Compact mode is
1000 × 680. These modes generate their own fixtures, do not attach the user's
workspace, do not run agents and do not restart an existing application. Run
builds serially: `cargo test` can replace the ordinary target executable with a
non-QA build.

The deterministic check verifies 120 actual wheel dispatches per theme/size,
reverse displacement, zero additional presentation computations during wheel
and logo updates, invalidation after a semantic change, Escape, Enter/Space,
focus trapping and focus restoration. Screenshots are written under
`.local/automation-dialog-scroll`.

The native check opens a real macOS window with an animated sidebar and a long
report containing a Markdown table. It dispatches 240 timed wheel events,
checks displacement in both directions, reports input-plus-draw work and timer
intervals, and exits automatically. Timer intervals include scheduling overhead;
they are not display-present/GPU frame timings. This is a focused small-workspace
regression, not completion of the separate 1,000-record/concurrent-save release
matrix. No pre-fix timing comparison was captured.

## Verification on 2026-11-09

Device: Apple M1 Pro, built-in 3456 × 2234 Retina display. Refresh rate was not
reported by `system_profiler`; no refresh-rate compliance is claimed.

- `cargo test --quiet`: 743 passed, 2 ignored, 0 failed.
- `--qa-automation`: passed, including background updates while the period
  dialog is open, retry scope, navigation and planning completion.
- Deterministic wheel-plus-frame p95: 1.60–1.82 ms across both themes and sizes.
- Native input-plus-draw p95: 3.10–7.14 ms; maximum 8.54 ms. No sampled
  timer interval exceeded 50 ms in any of the four theme/size combinations.
  Raw measurements are retained in [the measurement log](AUTOMATION_DIALOG_SCROLL.txt).
- Light and dark deterministic screenshots inspected; report clipping, modal
  placement and fixed close controls remain correct.


## Follow-up: live workspace signature lookup

The user reported unchanged stuttering after installing the first change,
including a cancelled period with no report. Sampling the actual application
while scrolling found `sidebar_surface → progress_result_task_count →
run_is_automation_owned → request_for_run_record → request_plan_signature` on
the main thread. The legacy ownership fallback serialized every request's
entire goal tree for ordinary research steps with nonempty input signatures.
Reader polling also reached this fallback. Retaining the modal could not remove
this work when background notifications refreshed the shell.

The fix excludes typed research runs from signature-based request-planning
inference while preserving explicit owner metadata, recorded planning-run IDs
and the unique-signature fallback for untyped legacy planners. A regression
checks both the exclusion and explicit ownership precedence. Clean/throttled
persistence polls now return before full goal-approval reconciliation, and an
active worker with no events no longer takes a tracked mutable run reference
(which previously marked the run dirty and caused redundant save/ACK updates).
Forced persistence still reconciles immediately.

The native harness now accepts `PLANNING_DIALOG_WORKSPACE` (a closed SQLite
snapshot) and `PLANNING_DIALOG_PERIOD`. It loads through the existing read-only
snapshot loader, attaches no storage or agents, and sends a semantic notification
every fifth wheel frame to exercise background invalidation. The user's live
SQLite database was copied with SQLite's backup API; only the isolated copy was
checkpointed. No private workspace content or raw process samples are committed.

A snapshot with 184 tasks and 188 runs reproduced both reported dialogs. For
period 12, sidebar counting took 218.36 ms before and 0.05 ms after; for period 11,
72.58 ms before and 0.05 ms after. CPU contention differs between runs, so the
absolute comparison is diagnostic rather than a universal speedup claim.

| Period | Build | Input interval p95 | Maximum interval | Intervals > 50 ms |
| --- | --- | ---: | ---: | ---: |
| 12, final report | Before follow-up | 88.34 ms | 117.63 ms | 49 |
| 12, final report | After follow-up | 25.79 ms | 30.83 ms | 0 |
| 11, cancelled steps | Before follow-up | 87.33 ms | 161.41 ms | 54 |
| 11, cancelled steps | After follow-up | 28.84 ms | 33.48 ms | 0 |

Both tests used 240 timed bidirectional events, dark mode and 1440 × 920 windows.
These intervals include the 16 ms input timer and main-loop notifications;
they capture the stalls missed by timing only the immediate `draw` call.
They are not GPU presentation times. Bidirectional displacement passed in all
four cases. Raw timings are in [the follow-up log](AUTOMATION_DIALOG_LIVE.txt).
Concurrency QA passed with real local fake-worker overlap, lowering/raising
limits, dependency chains, independent cancellation and same-session retry.

Follow-up checks: `cargo test --quiet` passed 744 tests with 2 ignored;
`--qa-automation` and `--qa-concurrency` passed. The normal app was rebuilt,
signed and restarted, and its actual workspace/results page loaded successfully.
Native CUA could read that window, but subsequent click attempts returned
`noWindowsAvailable`; post-install dialog interaction evidence therefore remains
the isolated native snapshot comparison above, not a claimed live-user gesture
sign-off.

## Follow-up: first-open viewport freeze

The user found a remaining first-open freeze, especially with larger research
results. In the running app, wheel gestures left the visible content unchanged;
pressing Tab caused the content to jump to the accumulated scroll position.
The main-thread idle sample was waiting in the native event loop. This was a
paint invalidation failure, not evidence of a multi-second synchronous read.

A new native regression opens the modal over an already-rendered Automation
results page, scrolls immediately, and checks the painted marker position
relative to the viewport against the actual offset. It never calls `draw` or
`render_frame`, injects no semantic notifications and uses no post-open warmup.
It repeats close/open three times. Before the fix it reproduced an offset of
-168 px with zero painted displacement and no visible scrolling in the sample
window. The earlier tests asserted offsets and explicitly drew frames, so they
missed this failure.

The result body's native scroll handler now requests `Window::refresh`.
This requests a paint without notifying TaskApp or invalidating its report and
status data caches. The modal remains retained while idle, and normal scrolling,
text selection, focus trapping and report links stay on the native input path.
Simply adding another retained-view notification was insufficient in the
large-report regression, so the change explicitly requests the window paint.

```sh
cargo build --features qa --quiet
PLANNING_DIALOG_WORKSPACE=/path/to/closed-snapshot.sqlite3 \
PLANNING_DIALOG_PERIOD=11 \
  target/debug/what-the-hell-are-you-planning --qa-automation-dialog-first-scroll
```

`PLANNING_DIALOG_REPORT_REPEAT=8` enlarges report bodies only in the test's
in-memory snapshot. It does not write to the snapshot or live workspace. Tests
covered the cancelled period, a quick report, 49,913 bytes of research results
and an enlarged 399,304-byte variant. All twelve native opens produced visible
scrolling within 34–70 ms, including first-frame/layout scheduling. These are
input-to-observed-layout measurements, not GPU presentation timings or a promise
that arbitrarily large reports have finished parsing in that time.

The existing four theme/size dialog checks also passed, including zero extra
report presentations during wheel/animation frames and invalidation on actual
data changes. Raw first-scroll measurements are in
[AUTOMATION_FIRST_SCROLL.txt](AUTOMATION_FIRST_SCROLL.txt).

## Bounded long-report readers

Long reports (over 4 KiB or 40 lines) now use retained `TextViewState` and
`TextView::scrollable(true)`, which virtualizes Markdown blocks and supplies a
vertical scrollbar. Short reports keep natural height. The reading viewport is
45% of window height, bounded to 180–420 px. No report data is truncated.

A capture handler before the text component's bounce handler routes vertical
input to exactly one viewport. A clipped reader is first revealed in the outer
viewport; inside the visible reader only text moves. At either text boundary,
subsequent outward packets move the outer dialog. Horizontal packets and pointer
selection/scrollbar dragging are not intercepted. Each accepted wheel requests
paint without invalidating the report presentation. Reader state survives
semantic updates and is cleared on workspace replacement; only the most recent
long-report period is retained.

The native first-scroll QA additionally checks clipped-reader reveal, inner-only
movement, and handoff at both boundaries. The test uses a frozen workspace,
never the live database, and repeats close/open three times. A real report has
49,913 stored output bytes; an in-memory eightfold stress case has 399,304 bytes.

The warm native benchmark for the real report improved from 34.49 ms to
12.85 ms p95 wheel-plus-forced-frame work, with timer intervals over 50 ms falling
from 90 to zero over 240 packets. These are harness work/timer metrics, not GPU
frame times or a claim of a particular display refresh rate. Both-theme,
both-size retained-presentation and keyboard regressions passed. Unit and
integration results: 745 passed, two existing ignored tests.

After rebuilding and restarting the signed development app, native UI inspection
of the live Period 9 report confirmed the first wheel revealed the bounded
reader and the next wheel changed its table/paragraph content while the outer
notice and report heading stayed fixed. The article scrollbar was visible.
