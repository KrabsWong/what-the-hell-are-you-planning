# Shared scrolling audit

The automation modal investigation uncovered shared main-thread work, not a
wheel-input defect. Retained surfaces prevent animation from rebuilding sibling
views, but do not make expensive semantic updates cheap. This audit extends the
fix to shared status computation used by the sidebar, collections, boards,
steps, results and dialogs.

## Changes

- Resolve automation ownership before constructing automation dependency and
  phase eligibility. Ordinary research rendering previously performed that
  work and discarded its result. Explicit and occurrence-only legacy ownership
  continue to use the same eligibility rules.
- Skip status evaluation for the unfiltered All research scope. Build its
  visible task list once for the toolbar and rows instead of three times.
- Share raw execution projections between goal summaries and child statuses
  within a semantic render cycle. Business/admission APIs remain uncached.
- Keep presentation caches during wheel/animation frames even when a task
  queue exists. Store mutations, parent notifications (including queue changes
  and lazy hydration), and workspace replacement still invalidate both caches.
  Retry explanation countdowns remain uncached.

This complements the earlier fixes for legacy planning-signature inference,
empty runtime polling, persistence reconciliation and modal retention. It does
not replace native scrolling or impose another scroll-speed setting.

## Reproduction and limits

Use a closed, checkpointed SQLite snapshot, never the live database. The
snapshot loader works on a private temporary copy; no agents or storage writer
are attached to the test app.

```sh
cargo test --quiet
cargo build --features qa --quiet
target/debug/what-the-hell-are-you-planning --qa-board-scroll
PLANNING_PERF_WORKSPACE=/path/to/snapshot.sqlite3 \
  target/debug/what-the-hell-are-you-planning --qa-render-perf
PLANNING_SCROLL_DARK=1 PLANNING_PERF_WORKSPACE=/path/to/snapshot.sqlite3 \
  target/debug/what-the-hell-are-you-planning --qa-scroll-native
```

Set `PLANNING_SCROLL_DARK=0` for light mode. Native QA tests a 1000 × 680
window with expanded sidebar projects. It dispatches bidirectional wheel input
on the sidebar, research list, horizontal board, model settings, automation
results and an ordinary report. Displacement assertions prevent measuring an
unmoving viewport. Each surface runs both without semantic notifications and
with a notification every fifth input. Input intervals include the 16 ms timer
and main-loop scheduling; these are not GPU presentation/refresh-rate metrics.

The headless board check also retains a paused queue, asserts zero extra status
computations during wheel frames, checks cached versus uncached projections,
and checks both Store changes and queue-only changes invalidate the cache.
Full-refresh frame timings deliberately defeat retention and should not be
reported as ordinary scroll frame times. Small/empty forms and every possible
workspace size are not individually certified by this matrix.

## Verification on 2026-11-09

`cargo test --quiet`: 745 passed, 2 ignored, 0 failures. The headless board,
settings and sidebar check passed, including cache equivalence, zero additional
status computations while scrolling with a paused queue, Store invalidation,
and queue-only invalidation. Wheel-plus-draw p95 was 9.43 ms for the board,
1.53 ms for settings and 8.38 ms for the expanded sidebar.

The same 184-task / 188-run snapshot used in the automation investigation was
used for the application audit. Forced full-refresh mean time fell from
302.88 to 83.62 ms for All research, and from approximately 51–58 to 24–32 ms
for Recovery, Project, Topic, Steps and Report. These are stress measurements
that explicitly defeat view retention, not normal scrolling frame times.

Native QA passed all displacement assertions in both themes. It lets the OS
schedule display callbacks instead of forcing an extra draw after every input.
The `work` field in its output measures event dispatch only. The table reports
input-interval p95, including the 16 ms timer and actual main-loop scheduling.

| Surface | Dark, scrolling | Dark, background updates | Light, scrolling | Light, background updates |
| --- | ---: | ---: | ---: | ---: |
| Sidebar | 16.75 ms | 40.73 ms | 21.71 ms | 40.83 ms |
| Research list | 24.82 ms | 34.46 ms | 24.37 ms | 36.17 ms |
| Research board | 35.05 ms | 47.36 ms | 34.62 ms | 47.82 ms |
| Model settings | 16.69 ms | 23.71 ms | 25.22 ms | 21.98 ms |
| Automation results | 16.69 ms | 24.00 ms | 17.18 ms | 24.01 ms |
| Research report | 21.55 ms | 24.44 ms | 25.01 ms | 24.52 ms |

The board had one interval around 57 ms in each theme's scrolling pass. No
other native pass had an interval above 50 ms. Thus the shared stalls are
reduced, but this audit does not claim zero dropped frames or full refresh-rate
compliance on every surface. The board remains the heaviest tested surface.
Raw measurements are retained in [APPLICATION_SCROLL.txt](APPLICATION_SCROLL.txt).

Automation QA and the four theme/size deterministic dialog regressions also
passed. Native snapshot checks of cancelled period 11 and report period 12
passed bidirectional displacement with input-interval p95 of 25.08 and 29.70 ms
respectively, with no intervals above 50 ms. The dialog harness still includes
an explicit draw after dispatch, unlike the native application matrix.
