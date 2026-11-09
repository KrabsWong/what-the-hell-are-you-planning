# T08 handoff: global Search contract

Date: 2026-11-08. Working tree only; not committed or pushed.

## Outcome

Footer **Search** and **⌘K** now open one global overlay from every route. Opening
it does not rewrite the route, selection, list filters, scroll position or any
open draft. Closing restores the exact opener focus. Choosing a result is an
explicit navigation to its owning domain.

New files:

- `src/global_search.rs` — pure search domain/ranking/ownership, overlay UI,
  keyboard handling and unit tests.
- `src/global_search_qa.rs` — focused headless QA, `--qa-global-search`.

Narrow integration:

- `src/app.rs` — separate global-search input entity + overlay state, list-local
  `search`/`query` unchanged.
- `src/main.rs` — module registration and `--qa-global-search`.
- `src/views.rs` — footer Search opens the overlay; root Escape closes it and
  blocks background navigation shortcuts while it is open; overlay rendered
  above existing modals. `navigation_header` was not changed.
- `doc/QA_COMMANDS.md` — new supplemental entry.
- `.local/opencode-dispatch/T08-audit.md` — initial audit.

No execution, scheduling, persistence, planning, result-presentation, T13,
T23/T25 or navigation-header behavior was changed.

## Search contract implemented

- Domains: **All**, **Research**, **Automations**.
- Target kinds with visible labels and destinations:
  - **Research** → ordinary research task/goal/step, opens its project/topic
    task path;
  - **Automation** → definition, opens the automation detail;
  - **Period** → period metadata, opens that exact period result dialog under
    its owning automation.
- Sources:
  - non-automation, non-archived ordinary research titles plus identifier,
    project/topic and status context;
  - definition name, objective, expected result, shape, status and schedule;
  - period automation name, ID, status, trigger source, reference time and a
    neutral **saved result reference** from the linked run ID.
- Automation-owned tasks and tasks in the generated automation project never
  appear as ordinary Research results. Planning attempts are not searched.
- Report bodies, raw logs and JSON are never read. Unit tests inject secret
  report/log text and prove it is not searchable.
- Token-AND, case-insensitive matching with deterministic ranking (exact/prefix
  title first, then context, then metadata).
- Empty query shows an explicit prompt; no matches names the active domain;
  deleted targets keep the overlay open with an inline reason.
- No loading state: all data is already in memory.

## Keyboard and activation

- Up/Down/Escape are handled by the overlay panel's key capture, so they work
  while the query field owns focus.
- Enter is the input's real `InputEvent::PressEnter`; activation uses that
  submit event instead of duplicating a second key path.
- Pointer row click and Enter call the same `activate_global_search_target`.
- Escape closes only the search overlay, including when a settings/period modal
  is underneath, and restores the modal focus.

## Exact QA input-event root cause and fix

The first QA run typed into the search field and asserted results in the same
`update_window` turn. The `InputState` value was already updated, but the
`TaskApp` subscription that applies `InputEvent::Change` and refreshes the
cached hits runs after that turn. The assertion therefore read the previous
(empty-query) hit list and `w.find(...)` panicked even though the product path
was correct.

Fix: QA now types through the real window, then yields an event-loop turn with
`cx.run_until_parked()` before asserting or pressing Enter. It also clicks the
query field before typing, because a footer-button click can leave the pointer
target as the focus owner. No product assertion was weakened and no direct
state mutation replaced typing.

## Coverage (real headless windows)

`--qa-global-search` runs at **1440×920** and **1000×680** in **light and dark**.
Each combination covers:

- Research list origin: footer open, domain/filter/layout preserved, typed
  result, Up/Down selection, Escape, exact opener focus restoration;
- Research detail origin: ⌘K, close restores selection/scroll/focus, chosen
  research title opens its project/topic path;
- Research result origin: close keeps scope/result context/selected run/focus;
- Automation list origin: automation-owned task absent from Research,
  Automation domain filter, Enter selects the exact definition, Back returns to
  the Automations list;
- Automation results origin (the original F08 route): footer opens the visible
  overlay instead of a hidden Research field, Escape preserves the route, and a
  pointer row click selects the definition;
- Automation detail + settings modal: Escape closes only the search overlay,
  preserves the modal, restores modal focus; period query opens the exact
  period; Escape returns to the owning automation detail;
- Settings origin: Escape preserves the page/tab and restores focus;
- Dirty task editor: text and focus survive close; activation is refused with
  an inline reason instead of discarding the draft;
- Dirty automation create form: same refusal and preserved text;
- Clean automation form: activation is allowed (no hardblock);
- Empty/no-match state and a target deleted while the overlay is open.

Screenshots: `.local/global-search-qa/92085/` (44 PNGs, four theme/size
directories). Root independent run: `.local/global-search-qa/86721/`.

## Verification run

- `python3 script/check_icons.py` → `All explicit Lucide references are embedded
  (51 registered icons).`
- `cargo build --features qa` → success.
- `target/debug/what-the-hell-are-you-planning --qa-global-search` → exit 0,
  `Global search QA passed · .local/global-search-qa/92085`.
- `cargo test --quiet global_search::tests` → 6 passed.
- Full `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet` was green
  earlier in this source line: 465 passed, 2 ignored, 0 failed. The tree is now
  transiently broken by in-flight T10/T11 edits (`project_app.rs` borrow/button
  API and other active changes), so the final full-suite rerun belongs to the
  root integration gate after those changes settle.

Logs: `.local/opencode-dispatch/T08-global-search-qa.log`,
`.local/opencode-dispatch/T08-audit.md`.

## T10 coordination seams and known gaps

- T10's shared `navigation_context` / `draft_exit` now exists. T08 consumes
  `draft_exit::task_edit_dirty`, `automation_draft_dirty` and
  `text_draft_dirty` instead of defining a second dirty policy.
- Search activation still calls the existing `open_task` / `open_automation` /
  `open_occurrence_result` methods without pushing T10's shared origin stack.
  `activate_global_search_target` is the single seam for T10 to wrap with its
  shared capture/exit hook. This was deliberately left for root-coordinated
  integration rather than adding a competing stack.
- Until T10 wires its Save / Discard / Stay flow, a dirty surface refuses
  activation with an inline reason. Clean forms and unchanged editors are
  allowed.
- Period rows deliberately say **saved result reference**, not a validated
  final report; T06 remains the authority for typed result quality.
- Archived ordinary research and automation-project tasks are not in the
  Research domain.
- No loading indicator is shown because search is synchronous.
- No VoiceOver/screen-reader work; keyboard behavior is covered.

## Boundaries respected

No edits to `src/domain_command.rs`, automation planning/history/schema,
`navigation_header`, `src/navigation.rs`, execution eligibility, result
presentation internals, or any baseline/gate evidence directory. No commit,
push or app restart.
