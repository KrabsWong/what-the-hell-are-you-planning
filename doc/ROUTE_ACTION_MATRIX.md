# Route and action matrix

Last source review: 2026-11-09, through user commit `98b71d4`. Current correction evidence and verification scope: [POST_DELIVERY_REVIEW](review-2026-11-08/POST_DELIVERY_REVIEW.md). Older round-specific evidence below is historical, not independent acceptance of this candidate.

This is the single maintained description of navigation routes and lifecycle actions. Product and user documents link here instead of restating route behavior. Research prompt and evidence policy remains in [Research Workflow Strategy](RESEARCH_WORKFLOW_STRATEGY.md); task status and independent acceptance remain in [TASKS.md](review-2026-11-08/TASKS.md) and [IMPLEMENTATION.md](review-2026-11-08/IMPLEMENTATION.md).

## Status legend

| Status | Meaning |
| --- | --- |
| **Current** | Present in the reviewed working-tree candidate and inspected in domain code, UI code or tests. This is not release approval. |
| **Current, acceptance pending** | Implemented but its owning task has not completed independent acceptance. Do not describe it as a settled target. |
| **Target** | Required contract that is not yet implemented or verified. |

Navigation is still changing under T08 (global search), T10 (creation/draft policy), T12 (typed navigation controller) and T19 (native keyboard/layout and scrolling evidence). T08's full global-search QA passed at r34, and T19's full keyboard/visual suite passed at r34, but their T10 integration, native matrix and release gates remain pending. T12 now stores page identity, task/result selection, the automation period/settings/planning-attempt and task-editor overlays, Settings/query/goal-panel page values and live bounded Back return frames in typed `nav::NavState`. Automation planning-attempt transitions validate the run's immutable automation owner atomically, and stale, foreign or ambiguous runs cannot replace the current route/overlay or discard a draft. Settings is a workspace-level page with no All-research parent; Back appears only when a real return frame exists. Remaining internal page payload fields (global layout/status, automation filter/history, editor/search payloads) still use direct app fields and are tracked as a documentation/scoped-cleanup deferral, not a claimed completed extraction; those rows state current behavior rather than a completed target.

## Route matrix

| Route or surface | Entry point | Current behavior | Required target | Status |
| --- | --- | --- | --- | --- |
| Automations list | Sidebar **All automations**, in the **Automations** section above **Research** | Lists saved definitions with name, shape, status, schedule, next trigger, latest activity and unread state; an archived state appears if stored, and an unassigned legacy planning section appears only when ownership cannot be verified. **New automation** opens the wizard; a row opens the detail. | Keep the separate top-level section. A global-search hit may open the definition once T08 is accepted. | **Current**; T08 pending for search |
| Automation detail | An automation row | Header shows status, shape, version and owning project. A live definition exposes Pause or Resume schedule, Run manually, Stop this run and Settings; a Draft adds **Review and enable** and disables Pause and Run; an archived definition leaves Settings. Copy and Archive are not exposed. | Keep the same destinations and action scope. Do not add a second back button. | **Current** |
| Automation settings | Detail → **Settings** | Shows definition, expected result, next three triggers, run limits and the approved snapshot; plan versions are read-only; **Edit** and **Replan** sit at the bottom. | Preserve the initiating dialog and opener focus when a plan version or edit closes (T10). Replan is bound to the current request revision (T25 accepted). | **Current**, acceptance pending |
| Automation creation or edit wizard | **New automation** or **Edit** | Three steps: task and instruction; schedule, time zone and three trigger previews; review and authorization. Quick derives its one answer step after the schedule step, while Research generates a short understanding and waits for confirmation before goal planning. **Back** moves one wizard step and keeps the draft. | Keep the three-step flow. Dirty-exit policy and return origin follow T10; route state becomes typed under T12. | **Current**; T10/T12 pending |
| Automation results | Sidebar **Automation results** | Lists periods across automations, newest activity first, with an automation filter and a shown/total count. A row opens that period's result dialog. | Keep the separate results route. Global-search hits open the chosen period once T08 is accepted. | **Current**; T08 pending for search |
| Period result dialog | A history row, a results row, or **Open** | Shows the typed result, sources, full-title step list, **Continue period** or **Retry period** for unfinished work, and collapsed technical details. Opening marks the period read. | Preserve completed step results for failed periods and keep diagnostics in the dialog. Rendered matrix and copy/export parity are part of T06 acceptance. | **Current**, T06 accepted; T10 origin pending for the caller chain |
| Research (All research) | Sidebar **All research**, in the **Research** section below **Automations** | Status filter and list-local search operate on the current view; List/Board switch; project groups; **New research**. | Same view, plus the one global search overlay with explicit Research/Automations filters after T08. Back must preserve the originating filter, scroll and layout under T10/T12. | **Current**; T08/T10/T12 pending |
| Projects, Topics, Goals and steps | Sidebar tree, board, list and breadcrumbs | Project and Topic pages, Goal detail, step detail and result routes use structural breadcrumbs. The header contains Back and breadcrumbs without a recent-task menu. | Keep structural crumbs. Detail/drill-down Back returns to the initiating context, including filters and scroll, after T10/T12. Archived presentation follows the accepted T22 contract. | **Current**; T10/T12 pending |
| Research results | Sidebar **Research results** | Choose project and Goal; switch result versions; planning activity is separated; result entries are typed and sources expand on demand. One resolved selection drives the reader, breadcrumbs, history and typed route: an explicit task/Goal context admits only its own or its child-step runs (archived history stays readable), an explicit period context admits only that occurrence's runs, and global results admit only ordinary non-archived runs. A deleted or cross-context selection falls back within the context or shows an explicit empty state; it never jumps to an arbitrary global run. | Keep ordinary results separate from automation history. T10 origin restoration remains pending. | **Current**, acceptance pending |
| Paused projects | Sidebar **Paused projects** | Lists interrupted or recovery-related research that needs explicit continuation. | Keep the route; T10 return context applies. | **Current** |
| Archive | Sidebar **Archive**, in the Research section | Shows archived ordinary research tasks and Goals; completed Goals keep their completion count and retained results as read-only archived history. Automation history is not included. | Keep the accepted archived-history presentation and **Restore** without approval or execution controls; archive itself does not require reapproval. T10 return context still applies. | **Current, acceptance pending**; r138 native archived Board/Back passed, but fresh completed 3/3 archive/restore remains open |
| Goal completion count | Count beside a goal in All research or Archive | Opens that goal directly on **Steps**, preserving the caller for Back. The fixed right chevron means navigation, not inline disclosure. Dirty Save/Discard replays the same Steps target; Stay retains the draft. | Keep a compact count action rather than a primary button. | **Implemented**; isolated navigation QA passed, integrated/native rerun pending |
| Settings | Workspace identity menu → **Settings…** | Opens model, agent, stage and application settings; the workspace row shows the selected state. | Back from a settings sub-page or edit returns to Settings with unrelated input preserved after T10. | **Current**; T10 pending |
| Global search | Footer **Search** or **⌘K** | Full global-search QA passes at r34, including Stay/second Enter and deleted-target cases; the shared T10 dirty/origin integration and final T08 release gate remain pending. It is not claimed here as a settled released route. | One overlay invoked from every route and modal context, with explicit Research/Automations scope, visible target kinds, period-specific destinations, and Escape restoring caller, focus and draft. | **Current, acceptance pending** (T08; r34 QA pass) |
| Back and breadcrumbs | Top-left **← Back** (⌘[), clickable crumbs | Breadcrumbs are structural. Back first uses the bounded return frame recorded by a user drill-down (for example a period step → period, report entry → caller, **Open steps** → the initiating All/Project/Topic/Archive board or list with its layout and scroll), restoring the captured route, filter, layout, scroll and selected item where available, then falls back to the structural parent. Report back from results (simplify r25) and from an archived Goal (native r13) passed, as did Search Stay/Discard/Back. Opening a period pushes a typed overlay over the existing filtered results page; closing it preserves that page. Drilling into a step captures the period and under-page state for Back. The full chain has offline coverage; final native replay remains required. Background completion paths never record or consume an origin. | Detail and drill-down flows return to the initiating context with filters, scroll and selected item; breadcrumbs remain structural parents; if no valid origin exists, use the structural parent fallback. T12 must migrate live routes to typed state without changing these destinations. | **Current, acceptance pending** (T10/T12); structural fallback is **Current** |
| Previous/Next | Step detail and report header | Automation Previous/Next stays inside the current period and disables at boundaries; **Overview** is separate. Ordinary research keeps its wrapping project-tree order; from both details and readers, a completed step opens its current report and a fully completed goal opens its final report, including current lazy bodies after restart. Busy, stale or failed work keeps task details. | Keep this contract. Full keyboard/visual QA passes at r34; remaining native matrix, resize and scrolling acceptance stay with T19/T20. | **Current** |
| Deferred research result | Sidebar, ordinary Previous/Next, recent result, explicit result/version entry | Request a lazy body immediately while retaining the visible caller. Commit title, task context and reader together after the read. New navigation, Back, search, overlays and workspace changes cancel the pending destination; late body responses may hydrate data without moving the page. | Verify newest-intent behavior, failure/retry and confirmed-empty state; restored routes may show Loading saved report. | **Current, acceptance pending** (V01/V02) |
| New research | Footer **New research** (⌘N), project or Topic page | Opens the research creation form from the current destination with a visible **Project** selector, so a global request can change its target project before planning. Dirty text is retained when moving within the form. | Preserve the picker. Cancel and Back restore the original route and a dirty draft offers Save / Discard / Stay under T10. | **Current**; T10 acceptance pending |

## Action matrix

| Action | Visible label | Scope and effect | Status |
| --- | --- | --- | --- |
| Advance the creation wizard | **Continue**, or **Understand research** on the Research schedule step | Validates the current step. Research opens understanding review; explicit confirmation then starts goal planning; Quick derives its single answer step locally and opens authorization. | **Current** |
| Go back one wizard step | **Back** | Moves one step back inside the same draft and keeps entered values. It does not cancel the draft. | **Current** |
| Start a new automation | **New automation** | Opens the three-step wizard from a fresh explicit default model; it does not inherit the previous form's shape, schedule policy or plan. | **Current** |
| Start new research | **New research** / **⌘N** | Opens the research request form with a visible project destination. It does not run research before planning is reviewed. | **Current**; T10 acceptance pending |
| Generate project topics | **Generate topics** in project planning | Starts planning. Success keeps the current page, adds the generated Goals to **Backlog** and shows a toast prompting the user to open the project; it does not navigate or start research. | **Current** (T02 background-navigation contract) |
| Edit an automation | **Settings → Edit** | Opens a new draft version of the selected definition. Saving does not enable it; Enable requires fresh approval. | **Current** |
| Leave a dirty form | **Save … and continue** / **Discard and continue** / **Stay** | Offers an explicit choice before a user navigation or dismiss action can lose unsaved task-edit, automation-draft or research-request input. Generating a plan is separate from saving the request. | **Current**; T10 acceptance pending |
| Restore archived research | **Restore** | Returns archived ordinary research to its prior lifecycle and makes retained results available again; it does not require reapproval by itself. | **Current, acceptance pending** (T22 fresh completed-goal native restore remains open) |
| Keep an unfinished automation | **Save draft** / **Save draft & finish** | Saves the current definition without enabling it or starting research. A saved planning draft keeps its **Planning…** state. | **Current** |
| Approve and schedule | **Enable automation** | Requires a reviewed plan and binds the current definition version, template, schedule, models/agents, materials and planning revision. Disabled until the plan is ready. | **Current** |
| Replan a Research automation | **Replan** | Plans the current immutable objective plus optional expected result, creates a new reviewed revision and requires fresh approval. Frozen periods keep their original revision. | **Current, acceptance pending** (T25 focused offline checks pass; final native revision/history and T10 opener replay remain open) |
| Stop future triggers | **Pause** | Stops future triggers and cancels unstarted periods. A running period may finish; it is not silently cancelled. | **Current** |
| Resume future triggers | **Resume schedule** | Enables a paused definition from now: re-anchors the interval, computes the next future cursor and clears failure attention. It never retries history and never backfills the paused interval. | **Current** |
| Create one manual period | **Run manually** | Creates one period at request time with the current approved snapshot. It uses the automation queue but does not move the schedule cursor or change the definition state. | **Current** |
| Stop the active period | **Stop this run** | Cancels the active period and its scheduled retries. Future triggers continue unless the definition is also paused. | **Current** |
| Finish an active period | **Continue period** | Reopens only the active period's unfinished steps in place, using its original approved snapshot and reference time; finished step results are kept. Definition and schedule state do not change, and the page does not navigate. | **Current** |
| Retry a terminal period | **Retry period** | Reopens the same failed, cancelled or interrupted period's execution in place against its frozen approved snapshot and reference time; finished steps are kept and only unfinished work runs. It does not resume the schedule, move the cursor, substitute the current template or change the route. | **Current** |
| Cancel one period from results | **Cancel period** | Cancels that period and its scheduled retry while leaving the automation schedule unchanged. The explicit user cancellation is not automatically requeued by recovery. | **Current** |
| Open a period result | Row click or **Open** | Opens the result dialog and marks the period read. It does not start work or change the definition. | **Current** |
| Load older history | **Load more · N remaining** | Requests another indexed history page on the live v4 path. Selected saved report bodies load asynchronously with explicit loading/failure/retry states. Legacy/offline paths can retain an in-memory visual limit. | **Current, acceptance pending** (T15 final performance/native gates) |
| Global search | **Search** or **⌘K** | Full global-search QA passes at r34; Escape/Stay/Discard/Back and deleted-target feedback are exercised. Final T10 shared dirty/origin integration and T08 release acceptance remain pending. | **Current, acceptance pending** (T08; r34 QA pass) |
| Navigate back | **← Back** (⌘[) | Uses the recorded return frame for implemented drill-downs: report/task result → calling page, period step → period dialog with its under-page filter and scroll, **Open steps** → the initiating All/Project/Topic/Archive board or list with its layout and scroll, and global-search selection → caller (Stay/Discard/Back exercised natively). Otherwise falls back to the structural parent. A period overlay preserves its filtered results under-page; a step drill-down captures that overlay and page state. The research-settings shortcut records its origin before opening model settings. Final native coverage remains required. | Required target remains the initiating context with filters, scroll and selected item for every detail/drill-down flow, structural-parent fallback when no valid origin exists, and typed route state under T12. | **Current, acceptance pending** (T10/T12); structural fallback is **Current** |

## Action emphasis

This section makes target contract 11 inspectable without turning every page into a uniform row of buttons.

| Level | Current candidate | Required target |
| --- | --- | --- |
| Primary | Filled button for the local advancing action: **New automation**, **New research**, **Continue** / **Understand research** / **Confirm and generate plan**, **Enable automation**, **Resume schedule**, **Run manually** when it is the enabled state's advancing action, **Edit** in automation Settings, **Continue period** / **Retry period** when the period can reopen, **View final report** / **View latest result**, and **Regenerate** on the completed report reader before its neighbor arrows. | One primary fill per local action group. A group with two equally prominent actions must be split or one action demoted. |
| Secondary | Ghost supporting controls via `quiet_button`, such as Settings, Back, Close and contextual secondary actions; the primary choice depends on the page and state. | Keep labels visible and provide hover/focus feedback. A permanent border is not required for every action. |
| Icon | Ghost Lucide controls with fixed square hit area, tooltip and accessible name, such as **Project options**, display options, period open/close, and the theme switch. | Keep a full hit area and a meaningful label; icon-only is only for compact, well-understood actions. |
| More | ⋯ for low-frequency actions such as **Rename project…** and **Working folder…**. | Move suitable low-frequency actions here; never hide the page's main action or an important disabled reason. |
| Danger | Red outline and tint for **Stop this run** and **Cancel period**, and for removing material. | Keep danger separate from ordinary Close/Cancel; state the scope in the tooltip. |

Current source after the user correction: the project toolbar retains **New research** as the primary action and **Run / resume** as a supporting action; the automation history row keeps **Cancel period** inside the result dialog, and DESIGN now matches that placement instead of requiring a row control. These source changes still need native light/dark, compact-window and keyboard validation under T19/T20; a source edit is not native acceptance.

Open verification follow-up: confirm the project toolbar hierarchy and dialog-only cancellation on the isolated native app in both themes and required sizes. Native validation remains pending, not fully accepted. Fake or non-functional links remain unacceptable in any state.

## Lifecycle terms

Each term below has one meaning across the interface, logs and documentation.

| Term | Definition |
| --- | --- |
| **Definition** | The saved automation itself. Its state is Draft, Enabled, Paused, Needs attention or Archived. Definition state is independent of any period state. |
| **Version** | An approved definition revision. Changes to the task, plan, models/agents, materials, schedule or limits create a new version that requires approval before it can run. |
| **Planning revision** | For Research automations, the immutable pair of objective and optional expected result sent to the planner. Editing either field requires Replan and a new review; frozen periods and older reviews keep their original revision. |
| **Template** | The reviewed execution plan. Quick uses a locally derived single answer step; Research uses planner-generated Goals and steps. A period instantiates the approved template unchanged and never re-plans. |
| **Approval snapshot** | The models, agents, materials, policy and template bound to an approved definition version. It is copied into each new period snapshot. |
| **Period** (occurrence) | One execution of a definition. It has its own ID, trigger source, reference time, data window, snapshot, steps and result. A definition has at most one active period and one additional unstarted period. |
| **Period state** | Queued, Running, Waiting to retry, Checking after restart, Succeeded, Failed, Interrupted, Cancelled or Skipped. This is separate from definition state, step eligibility and evidence quality. |
| **Scheduled trigger** | A future time produced by the interval, daily or weekly schedule. It becomes due when the app is open, the computer is awake and the cursor has been reached. |
| **Manual period** | A period created by **Run manually** at request time. It does not consume or move the schedule cursor. |
| **Reference time** | The scheduled trigger for a scheduled period, or the request time for a manual period. Retrying keeps the original reference time. |
| **Data window** | The resolved previous local-day window, or an explicit unresolved reason. It is frozen with the period and is not recomputed by retry. |
| **Latest activity** | A period's display key: explicit retry request first, otherwise actual start, manual request, scheduled trigger or frozen reference time. Across periods, rows sort by that recorded timestamp, so a clock rollback can leave a retry below another period; equal timestamps use activity kind and ID. |
| **Catch-up** | Processing triggers that became due while the app was closed or asleep. See the catch-up rules below. |
| **Backfill** | Creating periods for time that was not observed, such as a paused interval. Pausing and resuming never backfills. This is different from LatestOnly catch-up, which may run the newest missed period. |
| **Allowed delay** | Lateness limit recorded in the period snapshot. It decides LatestOnly expiry for a missed trigger and first-start expiry for an unstarted queued period using its reference time, including a manual request. It does not expire a period after its first start or an explicit retry. |
| **Expired** | A LatestOnly outcome: the newest missed trigger is older than the allowed delay, so it is skipped and recorded. Under SkipAll a collapsed missed range is skipped as a catch-up skip instead. |
| **Continue period** | Finish an active period's remaining steps in place without navigating; definition and schedule state are unchanged. |
| **Retry period** | Reopen a failed, cancelled or interrupted period's execution in place with its frozen approved snapshot, reference time and finished step results; schedule state and route are unchanged. |
| **Resume schedule** | Enable a paused definition for future triggers from now. It does not retry history and does not backfill the paused interval. |
| **Run manually** | Create one new manual period without moving the schedule cursor. |
| **Final report** | A validated reader-facing report for the current input revision. |
| **Partial period reports** | At least one Goal has a validated final report, but the whole period is not complete. Finished evidence remains inspectable. |
| **Intermediate evidence** | Completed collection or analysis output without a validated final report for that Goal. It is not presented as an answer. |
| **Live progress** | Streaming or in-flight output. It is never styled as a validated final answer. |
| **Evidence quality** | Complete, Evidence gaps, Needs input, Unverified legacy, Report rejected, Not validated or Incomplete. It is independent of period state and freshness. |
| **Evidence gap** | A disclosed limitation or unverified item inside a report, or a gap recorded when an unattended period meets sign-in, CAPTCHA or browser handoff. A validated report can carry evidence gaps; a gap alone does not make the period failed. |
| **Freshness** | Stale input, Regenerating, Original approved version, Historical result or Legacy result. Applicable labels can appear together. |

## Catch-up rules

| Setting (UI label) | Internal value | Behavior on return |
| --- | --- | --- |
| **Only the latest missed period** | LatestOnly | Runs the newest missed trigger when it is within the allowed delay. If that trigger is expired, skips it and records the summarized range. Older triggers are summarized, not replayed. |
| **Skip all missed periods** | SkipAll | If any trigger was missed, skips the whole collapsed missed range and waits for the next future trigger; the range is recorded as a catch-up skip. |

The allowed delay is measured from the newest trigger. Under LatestOnly, expiry is checked before execution; under SkipAll the collapsed missed range is skipped regardless of age, so it is not separately recorded as expired. No per-minute or per-interval occurrences are created for a large gap.

The typed result presentation above is the accepted T06 contract for the reviewed candidate. It is current inspectable behavior, not a release approval; integration and native release gates remain.

## Performance and durability boundaries

The current implementation retains ordered v4 persistence and asynchronous history/body reads. Earlier checkpoint timing and native observations apply only to their named sources. The r194/r196 ledger and r195 live measurements are historical evidence, not performance results for the new profile/cache changes at `98b71d4`. See the [correction review](review-2026-11-08/POST_DELIVERY_REVIEW.md).

| Area | Current source contract | Remaining verification |
| --- | --- | --- |
| History and reader loading | Metadata and body loading stay separate. Current durable lazy results remain navigable; pending, failed and confirmed-empty bodies are distinct. Explicit navigation prepares the destination before committing it. | V01/V02: restart, exact result identity, newest intent, cancellation, scoped retry and absent-body handling. |
| Saving and dispatch | Ordered receipts and durable admission remain in place; no change to these guarantees is inferred from the new visual work. | Preserve prior crash/durability evidence with its scope; rerun affected checks if these paths change. |
| Rendering and scrolling | Status validation is reused across wheel redraws, invalidated on semantic changes/epoch/queue updates, and excludes retry countdown caching. Sidebar identity is fixed outside navigation scrolling. Dev/dependency opt-level is 3. | V05: actual candidate and normal input/draw path, correctness after cache invalidation, native repeated scrolling at ≤1000 records. Author-reported headless passes do not prove native smoothness. |
| Scale | ≤1000 records remains the required performance scope; isolated hitches may be named follow-ups under the user policy. Larger history/body stress remains diagnostic. | Retain build/profile/source provenance and frame distributions; sustained stalls, lost position and wrong displacement still fail. |

## Approval and capability boundary

Creation and edit validate the objective and expected-result fields, and Research templates validate generated Goal titles, step titles, briefs and instructions, for obvious external-action requests. The same checks run at Continue, create, revise, approve and enable. These checks are input validation and prompt guidance, not a runtime sandbox.

The automation name, material contents, comments, model output and generated report text are not exhaustively validated. Approved steps use the selected OpenCode or Codex agent with its normal permissions. Prompt wording that says not to send, publish, trade or delete is guidance and can be ignored by a model. Period deduplication is not exactly-once for external operations. Review reports before acting. When an unattended period meets a sign-in, CAPTCHA or browser handoff, it records an evidence gap and continues.

## Verification boundaries

The correction review inspected all changed implementation and QA source at `ce38de5`/`98b71d4`; commit-reported test results were not independently rerun during that documentation task. Current native acceptance remains pending under V01–V05. Prior evidence is not discarded, but ordinary result-first restart navigation, atomic loading transitions, resized geometry and fixed-sidebar behavior need their own current-case evidence. Real remote publication, physical sleep/DST/disk failure combinations and the prior zero-horizontal-delta Computer limitation remain explicit boundaries. Never relabel source assertions, author-reported test results or static screenshots as native smoothness proof.

## Related documents

- [PRODUCT.md](../PRODUCT.md) defines the product purpose and interaction principles.
- [DESIGN.md](../DESIGN.md) defines visual and interaction design.
- [USER_GUIDE.md](../USER_GUIDE.md) describes the user-visible flow.
- [RESEARCH_WORKFLOW_STRATEGY.md](RESEARCH_WORKFLOW_STRATEGY.md) defines planning, evidence, report and publication policy.
- [TASKS.md](review-2026-11-08/TASKS.md) and [IMPLEMENTATION.md](review-2026-11-08/IMPLEMENTATION.md) remain the acceptance authority.
