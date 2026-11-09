# First-round remaining acceptance

Status: in progress; not release approval. Authority: [TASKS](TASKS.md). Current evidence and historical corrections: [IMPLEMENTATION](IMPLEMENTATION.md). The user accepts occasional isolated hitches; sustained stalls, incorrect scrolling/navigation, data loss and false status remain blockers. Preserve raw failures and named follow-ups.

## R1 — Finish current standalone regression

**Do:** complete the frozen 55-entry ledger on the provenance-verified `23240064…` executable, then independently replay any repaired entries.

**How:** retain the r194 runner output and per-entry logs. The automation entry currently fails because its range assertion reads the old ScrollHandle instead of the virtual list. Review the OpenCode QA repair and run the complete automation entry; retain actual wheel displacement, visible period selection, filter, and exact Back/Close restoration assertions. QA-only repairs do not invalidate unrelated passing product paths; product repairs require the affected regressions again.

**Acceptance:** all correctness entries pass, all fourteen required 500/1,000 live p95 gates pass, diagnostics remain explicitly diagnostic, and each original failure has a named verified replacement. Current unit/integration evidence stays separately identified. Reject stale executable provenance.

**Dependencies:** none. Already running. Failure: [automation regression](evidence/r194-automation-regression.json).

## R2 — Verify ordinary List and Board

**Do:** natively inspect the corrected 80-goal / 240-step ordinary fixture with unrelated 1,000-period automation history.

**How:** use the isolated r190 app and `.local/native-review/r187-ordinary-valid-1000`. Confirm Done/3-of-3 rows and a populated Done board. Scroll in both directions, checking item identity and position, in light/dark at 1440×920 and 1000×680. Check horizontal board movement when columns overflow. Do not reuse the invalid r183 hybrid fixture.

**Acceptance:** correct visible status and counts; board cards exist and remain reachable; wheel movement is useful and reading position is stable. No sustained freeze. Record observed boundaries without claiming every offscreen card was inspected.

**Dependencies:** R1 measurement activity must be finished before timed native capture. Structural fixture checks already [passed](evidence/r191-ordinary-fixture-verification.json).

## R3 — Complete missing compact/theme surfaces

**Do:** fill the missing Automation results, long reader, settings and material-page coverage.

**How:** use the existing isolated 1,000-period, long-report and 24-material fixtures. At 1000×680 in both themes, exercise results down/up, Load more, row Open and Close; scroll the long report to Sources and back; exercise settings-page/dialog and material-grid scrolling. Add the light 1440×920 reader check. Use observed window geometry, not assumed dimensions.

**Acceptance:** controls remain inside the viewport and clickable, statuses identify their scope, long content remains reachable, pagination retains the reading anchor, and Close/Back returns to the expected context. Retain occasional outliers under T19-F1/F2; repeated disruptive stalls still fail.

**Dependencies:** R1 quiet measurement completion. Existing large-window results, dark reader and material/settings evidence are retained; do not repeat their entire workflows.

## R4 — Save while scrolling

**Do:** verify one representative harmless save overlapping native scrolling.

**How:** use the isolated workspace and change a harmless preference or metadata field. Capture the action, persistence timing and subsequent scroll/visible position. Verify the eventual durable value. A save that finishes before scrolling does not establish overlap; use the existing bounded test mechanism if a deterministic pending save is necessary, without changing production data.

**Acceptance:** wheel movement and reading position remain correct while a save is pending; no sustained block; Saving/Saved reflects the actual durable acknowledgement. One representative overlap is enough for this first round.

**Dependencies:** R2 or R3 provides the scrollable surface. No real schedule execution is needed.

## R5 — Publish the full disposition

**Do:** reconcile all T01–T25 and all feature families into the final delivery decision.

**How:** preserve the existing accepted keyboard/focus, lifecycle, ownership, archive, materials, schedule and capability evidence. Combine R1–R4 with those records. List each deferred remainder with trigger, impact and follow-up acceptance, including T19-F1/F2, publishing-toggle affordance and archived waiting wording. External-service and rare environment boundaries must remain visible.

**Acceptance:** no unresolved core failure or unexplained test failure; no blocked/not-run case is called passed; exact source/build, test logs and native expected/actual outcomes are linked. State whether the branch is deliverable under the user's first-round policy.

**Dependencies:** R1–R4. Screenshots and runtime traces remain local, not committed.
