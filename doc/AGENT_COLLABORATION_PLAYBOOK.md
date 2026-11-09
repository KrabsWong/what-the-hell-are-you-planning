# Project agent collaboration playbook

Version 1.0 — 2026-11-10. Scope: this repository only. These rules are an initial project workflow, not a validated cross-project standard or an installed global skill. This is a manual operating procedure, not an installed scheduler or an authorization to spend money, delegate, publish, or change production data. Existing user and repository instructions take precedence.

## Start here

1. Copy the task card below into this project's task document. Define one observable outcome and its evidence before selecting a model.
2. Choose direct execution or delegation using the routing table. Start with one representative task, then a batch of at most three independent tasks.
3. Set explicit root-task time and cost budgets. Include planning, execution, review, retries, child tasks and integration.
4. Deliver one reviewable change. The executor marks it `review_ready`; the reviewer checks both the intended behavior and the implementation.
5. Accept only evidenced cases. Classify failures before returning work; stop unchanged retry loops.
6. Record cost, first-pass acceptance and user corrections. Tune the next batch from these results.

Use one Markdown task ledger and linked evidence. A spreadsheet, fleet server, vector memory, dashboard and dedicated agents for every role are unnecessary for this version.

## Roles and model routing

Roles are responsibilities, not mandatory separate agents. A coordinator owns the result, dependency order, budget and integration. An executor implements a bounded change. A reviewer evaluates the result against the original behavior contract. The user decides genuinely unresolved product preferences; do not ask again about already-authorized work.

| Work | Default route | Reason to escalate |
| --- | --- | --- |
| Small, clear, local fix | One capable agent completes and checks it directly | Unexpected coupling or unclear behavior |
| Clear implementation with objective checks | Lower-cost model executes; coordinator reviews the diff and evidence | First failure reveals architectural or specification uncertainty |
| Navigation, interaction hierarchy, ambiguous product behavior | Strong model defines a representative behavior/prototype; implement directly or delegate after clarification | Competing user expectations remain unresolved |
| Persistence, migration, concurrency, security or broad refactor | Strong model owns key decisions; independent review of high-risk behavior | Missing rollback, invalid assumptions or insufficient evidence |
| Polling, counters, log filtering, status aggregation | Deterministic commands/scripts | Use a model only to interpret an actual anomaly |

Select actual models from measured results on this project's tasks, not permanent brand rankings. A cheap implementation followed by expensive repair is not a saving. Do not pay for an extra handoff when the current agent can finish the bounded task more cheaply.

## Define and split tasks

Split by independently verifiable user outcome or engineering invariant, not by file count or artificial plan/code/review stages. Each task should fit a focused work session and have a clear rollback boundary. Split a task further when it has unrelated acceptance outcomes, conflicting file ownership or cannot produce a checkpoint within its budget.

Before a comprehensive audit, inventory features and ordinary user journeys. Track each as `not_checked`, `checked`, `failed`, `blocked` or `out_of_scope`; record exclusions explicitly. Inspect primary journeys before exotic combinations. Coverage means every inventory item has a disposition, not that every combination has been tested.

Separate three kinds of claims:

- **Requirement:** user instruction or established product rule, with a source.
- **Observation:** reproduced behavior or code fact, with evidence.
- **Proposal:** an agent's suggested improvement, with uncertainty and expected benefit.

A proposal must not silently become a global design requirement. For consequential UX changes, validate one representative screen/flow before applying the pattern throughout the app. If user input is necessary, present the concrete alternatives and continue unrelated authorized work.

### Copyable task card

```markdown
## T-___ — Observable outcome

- Root/batch: ___
- Owner / executor / reviewer: ___ (roles may share an agent for low-risk work)
- Status: ready
- Priority / risk: ___
- Requirement and source: ___
- Observed problem and reproduction: ___
- User intent: ___
- Before → after behavior: ___
- In scope / out of scope: ___
- Proposed approach; assumptions to verify: ___
- Allowed files/modules; forbidden side effects: ___
- Dependencies: task IDs and exact prerequisite; or none
- Base commit / branch / isolated test workspace: ___
- Model route and why: ___
- Budget: active minutes ___; API-equivalent USD ___; cash cap ___
- Root remaining budget: ___; meter availability: measured / estimated / unavailable
- Checkpoint: ___; retry allowance: ___

Acceptance:
1. Given ___, when ___, then ___; verify with ___.
2. Negative/race/restart case appropriate to the change: ___.
3. Existing behavior that must remain intact: ___.

Return: commit/diff, changed behavior, exact checks and outcomes,
evidence paths, unverified cases, usage delta, and remaining risks.
```

For UI navigation specify source, action, destination, content identity, intermediate loading/error state, cancellation and Back behavior. For visual changes specify hierarchy and geometry at relevant window sizes, content lengths and themes. Do not encode a preferred CSS/control implementation as the acceptance criterion.

## Execution and handoff

Give the executor the task card, relevant specifications, file pointers and reproduction. Avoid copying the whole conversation or unrelated logs. Preserve a stable small contract; add only new evidence during repair. At a checkpoint, summarize current commit, remaining work, failed hypothesis and next check instead of replaying all history.

Use separate worktrees for simultaneous edits when appropriate. Default to at most two executors and one integration owner; serialize overlapping modules, database migrations and full builds. This is an initial operating limit, not a measured optimum. Run performance checks without competing builds.

Use targeted checks during iteration. Run the required broader regression suite once on the integrated candidate; rerun affected checks after new changes. Follow stricter repository requirements. Do not add tests that merely repeat implementation details or rerun an unchanged passing suite to create activity.

Before any app launch, verify the executable, arguments and isolated workspace/database path. Never assume a test flag isolates production storage. Preserve rollback data for migrations and verify restoration separately when authorized.

## State and acceptance

Normal path: `ready → running → review_ready → accepted`.

Other states: `waiting_environment`, `waiting_user`, `rework`, `blocked`, `deferred`, `cancelled`. Waiting stores the blocking condition, resume event/check and last good checkpoint. A child finishing wakes its parent for integration; it does not automatically accept the parent.

`review_ready` means implementation ended. `accepted` means the specified behavior was verified on the named candidate. A blocked required case prevents acceptance of that scope. A user-approved deferral changes the scope explicitly; it never turns an unexecuted check into a pass.

| Change | Minimum appropriate evidence |
| --- | --- |
| Local low-risk fix | Diff review and focused behavior check |
| State, storage or async logic | Focused positive/negative tests; relevant race/restart/rollback checks |
| Navigation | Exact source→destination and content identity; Back/cancellation; relevant loading/failure states |
| Visual hierarchy/layout | Actual candidate screenshots and interaction, relevant themes/sizes/long content; geometry where useful |
| Performance | Representative event path, workload, build profile and contention conditions; native observation when claiming native smoothness |
| Broad integration | Required project checks and selected end-to-end journeys on the final integrated candidate |

Test the user's ordinary intent before testing conformity to our proposed solution. An independent reviewer of medium/high-risk work should inspect the original task and diff, not only the executor's summary. A separate model is optional; independent evidence is mandatory for substantive acceptance claims.

### Copyable acceptance record

```markdown
Task / candidate commit and build: ___
Environment / fixture / timestamp: ___
Criterion | Expected | Observed | Evidence | Verdict
___ | ___ | ___ | ___ | pass / fail / blocked / not_run
Evidence origin: executor-reported / source-reviewed / independently-run / native-observed
Regression scope and excluded cases: ___
Decision: accepted / rework / blocked / explicitly deferred
Reviewer and rationale: ___
Usage and active-time delta: ___
```

Evidence applies only to its source and scope. Invalidate affected acceptance after a change; retain unaffected evidence with its provenance. Never infer full UX acceptance from test counts, screenshots alone or an agent's `done` response.

## Rework, escalation and stop-loss

These are starting defaults to tune after two or three batches, not benchmarks. Enforce them manually in the ledger until a scheduler exists.

| Trigger | Required action |
| --- | --- |
| First failed acceptance | Classify as specification, implementation, integration, environment/tooling or insufficient evidence; return a concrete counterexample |
| Specification/UX-direction failure | Coordinator revises the contract before more coding; do not ask a cheap executor to guess the product direction |
| Same criterion fails twice, or one repair cycle produces no new evidence | End the same-model retry loop; strong-model diagnosis, narrower task, or direct takeover |
| Two consecutive tool failures with the same root cause | Park that verification; record one blocker and resume condition; continue independent work |
| 30 active minutes without a reviewable checkpoint | Explain new evidence, remaining uncertainty and next bounded step; split/escalate if progress cannot be demonstrated |
| 75% of task or root budget consumed | Reforecast all remaining implementation and acceptance; reduce optional scope or route differently within existing authorization |
| 100% of either authorized active-time or cost cap | Stop launching dependent work; preserve patch and evidence, report outcome and remaining scope; any budget extension must be explicit |
| New nonessential issue discovered | Add to backlog with severity; do not silently enlarge the current batch |

Default repair allowance: initial implementation plus one bounded repair, then reassess. An escalation may continue within the existing root budget; it does not reset counters. Count every child, alternate model, retry, diagnosis and review against that root. Reserve approximately 25% of the starting budget for acceptance/integration; adjust from measured experience.

If cost metering is unavailable, record it as unknown and use an explicit time/attempt cap. Never claim a hard dollar limit is enforced without a meter. Waiting time is tracked separately from active work but does not erase deadline obligations.

Waiting on a human should resume on a response/event, not repeated model calls. For machine work, use a cheap bounded status command or event; notify on changed state. Repeated blockers share one root-cause record. Do not invent permission requirements for already-authorized reversible work.

### Copyable rework request

```markdown
Task / rejected candidate: ___
Failure class: specification / implementation / integration / environment / evidence
Failed criterion and reproduction: ___
Expected / actual: ___
Evidence and likely cause (mark hypotheses): ___
What remains valid: ___
Required correction and prohibited scope expansion: ___
Focused recheck plus affected regressions: ___
Attempt number / remaining root budget / escalation trigger: ___
```

## Measure value and improve routing

Keep three separate monetary columns: actual incremental cash paid, consistently allocated subscription cost, and API-equivalent usage. Never add API-equivalent dollars to subscription fees as if both were billed. Keep cached input separate from uncached input and avoid counting reasoning twice. See the [initial cost assessment](review-2026-11-08/COST_ASSESSMENT.md) for limitations.

Minimum ledger columns:

```text
root_id,task_id,attempt,role,model,session_id,base_commit,candidate_commit,
started_at,finished_at,active_minutes,waiting_minutes,
uncached_input,cache_read,cache_write,output,pricing_basis,
api_equivalent_usd,cash_usd,status,failure_class,evidence,user_correction_minutes
```

Use unknown rather than zero for missing data. Deduplicate requests or cumulative counters and account for inherited/forked history. Aggregate children once. Active agent time can overlap; elapsed completion time cannot be calculated by summing it.

After each batch inspect:

- First-pass acceptance: accepted without repair / reviewed tasks.
- Accepted-outcome cost: total batch cost, including failures, / accepted comparable outcomes.
- Rework and coordination share: their cost/time divided by total measured cost/time.
- End-to-end latency, environment waiting and user correction minutes.
- Escaped defects and user-reversed design decisions after acceptance.

Compare a few similar tasks completed directly by a strong model with delegated tasks. Keep scope, risk and acceptance comparable. Small samples guide routing; they do not establish universal savings. Do not optimize task counts by splitting work into trivial units.

## Learning and scope

Record user corrections as: rule, source/date, applicability, counterexample and confidence. Clearly distinguish confirmed preferences from proposals. Retire contradicted rules instead of accumulating instructions. Do not store raw secrets, private transcripts or machine-generated messages as user preferences.

This version intentionally does not implement automatic model selection, billing ingestion or self-modifying policy. Add automation only when repeated manual overhead justifies it.

## Sources and lessons

- [Project post-delivery review](review-2026-11-08/POST_DELIVERY_REVIEW.md): incorrect navigation expectations, mistaken universal border styling, async state omissions, layout gaps and overbroad acceptance.
- Noop, pinned at `e5a9a6132269a335a009b1a9c17c012d8bb35d80`: [proportional orchestration](https://github.com/huacnlee/noop/blob/e5a9a6132269a335a009b1a9c17c012d8bb35d80/templates/policies/orchestration.md), [task lifecycle](https://github.com/huacnlee/noop/blob/e5a9a6132269a335a009b1a9c17c012d8bb35d80/src/cli.ts), [wait rules](https://github.com/huacnlee/noop/blob/e5a9a6132269a335a009b1a9c17c012d8bb35d80/src/waits.ts), [root-cause supervisor](https://github.com/huacnlee/noop/blob/e5a9a6132269a335a009b1a9c17c012d8bb35d80/src/supervisor.ts).
- Independent acceptance, root-wide budgets and accepted-outcome metrics above are additions for our workflow, not claims that noop already enforces them.
