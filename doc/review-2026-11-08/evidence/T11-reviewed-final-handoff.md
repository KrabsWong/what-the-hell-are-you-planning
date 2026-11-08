# T11 handoff — shared control affordances and bounded visual consistency

Status: implementation complete in the working tree; root acceptance pending.
Maintained `DESIGN.md` Components/Boundaries wording was applied after explicit
authorization (T17 terminal); no unrelated paragraphs changed.
Root independently ran the QA binary on this candidate: `--qa-t11` EXIT 0
(`.local/t11-qa/99915`), `--qa-affordance` EXIT 0
(`.local/affordance-qa/1570`), `--qa-header-layout` EXIT 0
(`.local/header-layout-qa/1594`) and icon validation passed with 51 embedded
icons. The coherent full-unit gate is still blocked by concurrent
`research_input.rs` migration compile failures, not by T11. No commit, push,
production workspace write or app restart.

## Changed files

- `src/controls.rs` — shared factories: bordered `quiet_button` (secondary),
  documented `row_button` (compound rows/tabs/menus), bordered `tool_icon`,
  `danger_button` (red outline/tint), selection-safe `select_button` (accent
  surface/text + toggled state; gained `&App`), wrapped `labeled_menu_item` with
  full tooltip/AX value.
- `src/views.rs` — compound rows moved to `row_button`; passive task status is a
  `Role::Label` chip; Recently opened uses `labeled_menu_item`; selection calls
  pass `cx`; narrow style edits only.
- `src/automation_views.rs` — danger Stop/Cancel period; primary Run manually
  only in Enabled idle, primary Retry/Continue; visible Manual/Scheduled trigger
  labels; full-value tooltips for long names/status/step titles.
- `src/goal_steps.rs`, `src/run_views.rs` — task-tab/row exceptions and danger
  Stop task; selection calls pass `cx`; secondary actions bounded.
- `src/topic_views.rs`, `src/research_views.rs`, `src/settings_views.rs`,
  `src/report_publish.rs`, `src/batch_app.rs`, `src/intake_views.rs` — row/action
  factory migration, danger Stop all, selection `cx` call sites.
- `src/main.rs`, `src/t11_qa.rs` — isolated `--qa-t11` visual acceptance.
- `DESIGN.md` — narrow Components/Boundaries delta (secondary factory and
  documented row exceptions, selection vs primary, danger vs ordinary cancel,
  history/recent-long-value treatment). Task-tab rule unchanged.
- `.local/opencode-dispatch/T11-audit.md` — audit and documented exceptions.

## Behavior preserved

No changes to report validation, eligibility, schedules, persistence,
navigation authority, automation lifecycle handlers/domain commands, archive
presentation, or per-run planning opener focus. Factory/type signature changes
are style-only; callbacks and ids are unchanged. T08 global-search only gained
the new `select_button` argument.

## Documented exceptions (compound controls keep no nested border)

- Sidebar/tree/project/topic/goal rows and board cards (`row_button`).
- Segmented List/Board control, goal Overview/Steps tabs (`row_button`).
- Popup menu rows (`row_button` inside `labeled_menu_item`).
- Passive chips, stage badges, status text and timestamps stay `Div`s.

## Applied DESIGN delta (authorized; T17 terminal)

The following wording is now in `DESIGN.md`. The task-tab bullet and all other
sections are unchanged; no VoiceOver scope was added.

Under Components:

> **Secondary:** always-visible neutral surface and full border for settings,
> edit, copy, close or back. `controls::quiet_button` is the standard. Never
> make these ghost buttons by default. Compound rows, menu entries and task tabs
> are the only documented exception and use `controls::row_button`, because the
> container already supplies the surface and a nested border would read as a
> second control.

> **Icon:** fixed square hit area, full boundary, tooltip and
> `accessibility_label`; `controls::tool_icon` supplies that boundary. Never
> leave a bare × or +.

> **Selection:** bordered control whose selected state uses an accent
> surface/text and sets the toggled accessibility state. `controls::select_button`
> owns this appearance; selection never borrows the primary fill.

> **Danger:** red outline and tint (`controls::danger_button`) for stopping a
> run, cancelling an active period or removing material; ordinary close/cancel
> stays secondary.

Under Boundaries:

> Automation history rows keep one neutral surface and express state through
> semantic status text rather than a large row fill. Trigger source shows a
> compact Manual/Scheduled text label beside its icon, with the skipped-period
> detail in the tooltip. Long automation names and step titles ellipsize but keep
> the complete value in the tooltip and accessibility label. Recently opened
> menu labels wrap to the menu width and keep the complete value in the tooltip;
> they are never reduced to an unexplained short prefix.

## QA evidence (exact)

All commands from the repository root with Node 24 on PATH for the test suite.

- `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo check --features qa`
  → exit **0**.
- `PATH=/opt/homebrew/opt/node@24/bin:$PATH cargo test --quiet`
  → exit **0**, 483 passed, 2 documented opt-in ignores
  ([log](T11-unit.log)). A later root full-unit gate hit concurrent
  `research_input.rs` migration compile failures; T11 has no changes there and
  the coherent gate remains pending.
- `python3 script/check_icons.py`
  → exit **0**, “All explicit Lucide references are embedded (51 registered
  icons)” ([log](T11-icons.log)).
- `cargo run --features qa --quiet -- --qa-t11`
  → exit **0**; owner artifacts `.local/t11-qa/2328`
  ([log](T11-qa.log)); root independent rerun `.local/t11-qa/99915` also
  EXIT 0.
- `cargo run --features qa --quiet -- --qa-affordance`
  → exit **0**; artifacts `.local/affordance-qa/2841`
  ([log](T11-affordance.log)); root independent rerun
  `.local/affordance-qa/1570` EXIT 0.
- `cargo run --features qa --quiet -- --qa-header-layout`
  → exit **0**; artifacts `.local/header-layout-qa/4588`
  ([log](T11-header-layout.log)); root independent rerun
  `.local/header-layout-qa/1594` EXIT 0.

Root's earlier `--qa-t11` run (`EXIT 0`, `.local/t11-qa/85874`) used the first
fixture, where the stored Running period was reconciled on open. That screenshot
is the recovery state, not Enabled idle. The current run below uses separate
enabled-idle, draft, failed-period and recovery stores and asserts each state
explicitly. Root's independent rerun of this corrected fixture passed
(`.local/t11-qa/99915`).

`--qa-t11` covers, per size (1440×920, 1000×680) and theme (light, dark), with
long English and CJK user data and real saved storage:

- genuine Enabled idle: definition Enabled, no active period, Run manually
  primary, no Resume/Stop, 32 px control;
  `T11-enabled-idle-light-1440.png`, `T11-enabled-idle-dark-1000.png`.
- Draft: disabled Run manually/Pause keep shape; Review and enable is the only
  primary; `T11-draft-disabled-light-1000.png`.
- Failed period: Retry period primary, no Cancel period;
  `T11-failed-retry-light-1440.png`.
- Recovery fixture (labelled): Needs attention + Verifying, Resume primary,
  Stop this run danger, Cancel period danger in the dialog;
  `T11-recovery-active-dark-1000.png`, `T11-recovery-cancel-dark-1000.png`.
- Long recent-menu labels wrap and keep full AX value;
  `T11-recent-menu-light-1440.png`, `T11-recent-menu-dark-1000.png`.
- Passive status chip and bounded property/action controls;
  `T11-task-detail-dark-1000.png`.
- Selection state vs primary: Quick toggled true, Research false, click moves
  the state; Continue stays the primary action;
  `T11-selection-primary-light-1440.png`.
- Existing affordance coverage still passes (kanban/action surfaces);
  `T11-affordance-kanban-light-1000.png`.
- Header layout still passes at 1000/1440; the full recent label is visible in
  `T11-header-menu-light-1000.png`.

Durable copies of these screenshots are in
`doc/review-2026-11-08/evidence/T11-*.png`.

## Owner/dependency notes

- The T08 `global_search_qa` module is present and registered; I did not delete
  or stub it. The only T08-adjacent change is the required extra `cx` argument
  at its `select_button` call, which is signature-only.
- Full `--qa-automation` was not rerun on this highly concurrent tree; the
  changed surfaces are exercised by the focused T11 matrix plus the existing
  affordance/header suites; the owner's last unit run passed 483 tests, while
  the coherent root gate waits on the concurrent `research_input.rs` migration.
- T17 is terminal, so root explicitly authorized the narrow `DESIGN.md` delta
  above. No other maintained-document paragraph was edited.
