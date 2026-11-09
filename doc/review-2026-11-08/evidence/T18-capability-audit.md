# T18 audit and scoped implementation plan

Date: 2026-11-08
Model: `opencode-go/deepseek-v4.1-flash`
Task: T18 / F16 — make unattended capability guarantees accurate
Mode: read-only preparation. No product code or maintained documentation was edited. The only file written is this report. No research, browsing, messaging, publishing, trading, deletion, or user-approval tool was used.

**Status:** audit complete. T18 implementation has **not** started and is **not** complete. The authoritative scope in `doc/review-2026-11-08/TASKS.md` still requires a separate implementation, verification, and acceptance pass after the current UI writers finish.

---

## 1. Executive finding

The product currently says automations do not send email or IM, publish, trade, run scripts, or perform other external writes. The application does **not** enforce that as a capability boundary. It only applies a small case-insensitive substring check to the **objective** field, and even that check is skipped on the research-planning path. The **expected-result** field is never scanned. Both OpenCode and Codex launch through the shared manual research pipeline with no separate read-only agent or tool policy:

- OpenCode launches with `agent: None`, which falls back to `--agent build` (`src/app.rs:1854`, `src/opencode.rs:279-280`).
- Codex launches with `--sandbox workspace-write`, `approval_policy="never"`, and `network_access=true` (`src/codex.rs:43-45`).
- The time-context and research prompts contain “do not send/publish/trade” text. That is prompt guidance, not enforcement (`src/automation_execution.rs:1047-1048`, `src/research.rs:835-836`).

Therefore the honest current contract is: **automations are intended for read-only information work; creation performs a heuristic input check; execution still uses the selected agent's normal permissions.** The absolute product claims must be corrected. No test should label the keyword check a sandbox.

This audit found no evidence of an unauthorized external write. F16 is an accuracy/enforcement-gap finding, not an incident finding.

---

## 2. Current claims and actual behavior

Working-tree line numbers are from this audit and may drift under the current uncommitted UI work.

| Location | Current claim or behavior | Actual enforcement |
| --- | --- | --- |
| `PRODUCT.md:38` | “Automations do not send email or IM, publish, trade, run scripts or perform other external writes.” | None at the tool boundary. Prompt text plus partial input check only. |
| `README.md:60` | Same absolute list; also correctly says automations reuse the shared execution path and there is no separate automation agent profile. | Correct about shared path; the external-write sentence reads as a guarantee. |
| `doc/AUTOMATION_PLAN.md:13` | “Does not send email or IM, publish, trade, run scripts or perform external-system writes.” | Same gap. |
| `doc/AUTOMATION_PLAN.md:21`, `USER_GUIDE.md:187` | Quick period “runs one read-only step”. | “Read-only” here describes the intended answer shape, not a sandbox. |
| `doc/AUTOMATION_PLAN.md:78-79` | “There is no second agent configuration”; manual and scheduled runs share approval/concurrency/permission rules. | Accurate and should be preserved. Do not add a separate automation agent just to mask F16. |
| `src/automation_execution.rs:192-193` | Comment: actions “cannot perform unattended must fail at creation, not silently become broader permissions.” | Implies a capability boundary that does not exist. |
| `src/automation_views.rs:943-966` | `automation_validate_step_one` scans only `objective` with `unsupported_reason`; checks `expected` for length only. | Expected-answer text bypasses the check. |
| `src/automation_execution.rs:174-190` | `validated_instruction` trims and length-checks both fields, then calls `unsupported_reason(objective)` only. | Same bypass in quick and legacy templates. |
| `src/automation_execution.rs:194-225` | `unsupported_reason` is a lowercased substring list. | Variant- and negation-blind; no word boundaries; no tool policy. |
| `src/automation.rs:432-441` | `AutomationDefinition::validate_draft` checks only name/objective/expected lengths. | Draft save and approval do not scan instructions. |
| `src/request_planning.rs:266-278`, `src/automation_views.rs:1077` | Research planning receives only the `objective` string. | `expected_result` is not part of the planning context. |
| `src/request_planning.rs:467-480` | Planner prompt says preserve the user's intent/deliverable and “do not infer permission for trading, paid access…” | Prompt guidance only; expected result is absent. |
| `src/automation_execution.rs:142-172` | `template_from_plan` copies planner goals/steps; `expected_result` is ignored. | Plan text is reviewed by the user but not scanned. |
| `src/automation_execution.rs:1047-1048` | Period context: “Do not schedule, publish, send, trade, or modify external systems.” | Prompt guidance only. |
| `src/research.rs:835-836` | Shared run prompt: “Do not trade, place orders, make payments or publish anything.” | Prompt guidance only. |
| `src/opencode.rs:30` | Comment: `agent` is “used to enforce tool permissions”. | Launch always passes `None`; the comment is stale and should be corrected. |
| `src/app.rs:1854`, `src/opencode.rs:279-280` | `agent: None`; OpenCode falls back to `build`. | No read-only agent selected. |
| `src/codex.rs:43-45` | `workspace-write`, approval `never`, network access `true`; browser MCP approvals are `approve`. | Local workspace writes and network are available. |

### Claim that is still true and must stay

Automations do **not** re-plan every period; they reuse the approved template. They also do not run while the app is closed. Those are scheduling/lifecycle guarantees, not capability guarantees.

---

## 3. Instruction paths, traced

### 3.1 Objective

1. User enters the objective in the automation form (`src/automation_views.rs:2968-2977`).
2. Step-one validation scans it (`src/automation_views.rs:944-950`).
3. Quick: `quick_task_text` (line 971) → `build_quick_template` (line 117) → `validated_instruction` → goal brief and step instructions (`src/automation_execution.rs:122-129`). The objective becomes an execution prompt.
4. Research: `save_automation_form` (line 1141) stores it; `start_automation_planning` (line 1069) passes it alone to `create_automation_request`; the planner expands it. The stored template, not the objective, drives later periods.
5. Legacy/demo path: `src/workspace.rs:401,417` uses the same quick/legacy builders.

### 3.2 Expected result

1. User enters it under “Expected answer (optional)” for Quick or “Expected result (optional)” for Research (`src/automation_views.rs:2978-3000`).
2. Step one checks length only (`src/automation_views.rs:955-958`).
3. Quick: inserted into the goal brief and the answer-step instructions (`src/automation_execution.rs:123,128`). It directly affects execution.
4. Research: stored on the definition, shown in overview (`src/automation_views.rs:2243-2244`) and frozen in the occurrence snapshot (`src/automation.rs:362-364`), but it is **not** passed to the planner and is **not** used by `template_from_plan`. The helper text under the field says “The planner splits it into goals…” (`src/automation_views.rs:2996-2999`), which overstates its role.
5. Editing it calls `revise_automation` → `validate_draft`, which currently performs no instruction scan.

### 3.3 Generated plan

1. Planner output is parsed by `parse_request_plan` (`src/request_planning.rs:89`).
2. `template_from_plan` copies goal briefs and task instructions into the immutable template (`src/automation_execution.rs:142-172`).
3. The user reviews and approves the plan; this review is the current control for plan text.
4. No scan is applied to generated plan text. That is a residual limit, not necessarily a defect: the plan is model output and is explicitly reviewed before enabling. It should be documented, not sold as validated.

### 3.4 Inherited instructions

- Quick/legacy template briefs carry “Do not send messages, publish, trade, delete files, or change external systems.” (`src/automation_execution.rs:78,123`).
- Every automation period appends the occurrence context block with the same prohibition (`src/automation_execution.rs:1039-1066`).
- Every shared research run appends the general “Do not trade, place orders, make payments or publish anything” sentence (`src/research.rs:835-836`).
- Browser steps add ego/browser restrictions (`src/ego.rs:40`) and the Codex browser bridge (`src/research_app.rs:459`).
- These are model instructions. They can be ignored or overridden by later model behavior; they are not checked after generation.

### 3.5 Model/agent launch

- Per-stage agent and model are resolved from Settings (`src/domain.rs:355-376`; `src/research.rs:907`).
- `launch_run` creates one `opencode::Request` with `agent: None` (`src/app.rs:1854`).
- OpenCode uses `--agent build` by default (`src/opencode.rs:279-280`).
- Codex uses workspace-write, network access, and approval never (`src/codex.rs:43-45`).
- There is no shared capability-policy object, no tool allowlist, and no post-run external-action audit.

### 3.6 Actual tool restrictions

- Input validation: only `unsupported_reason` on the objective.
- Prompt restrictions: several sentences; advisory only.
- Runtime restrictions: whatever the selected CLI/agent and local OpenCode configuration provide. The app does not add a read-only policy.
- Workflow restrictions: approval, concurrency, schedule, and deduplication. These govern when work starts, not what a started agent may do.

---

## 4. Enforcement limits to document

1. The check is substring-based. Variants such as “email the results”, “send the report to the team”, “transfer funds”, “place a trade”, “deploy the site”, or “remove the directory” can avoid the current list.
2. It has no negation awareness. “Do not send email; summarize the page” is currently rejected even though it asks only for reading.
3. It has no word-boundary handling. Broad or partial matches can create false positives.
4. It checks only the objective in the UI/template path and checks neither field at draft save/approval.
5. It does not inspect expected-result text, generated plan text, materials, comments, repair output, or model output.
6. It does not inspect tool calls. The agent can still run shell commands, write inside the workspace, and (for Codex) use the network.
7. Prompt prohibitions are not a sandbox. A model can deviate.
8. Approval freezes text and version, not capabilities.
9. Period deduplication is not exactly-once for external operations; `doc/AUTOMATION_PLAN.md:109` already says this and should stay.

---

## 5. Recommended truthful product contract

Use this wording as the target. It preserves the product intent without claiming enforcement.

> Automations repeat information-collection, analysis, and local report-writing instructions. Before saving, the app checks the instruction and expected-result fields for obvious requests to send messages, publish, trade, place orders, push code, delete files, or run scripts. That check is **input validation and guidance**, not a sandbox or a permission boundary. Approved steps run through the same OpenCode or Codex agent runtime, with the same permissions as manual research; network and local workspace access may be available. Give automations read-only information tasks only, and review every result before acting on it. If the product later needs a hard external-write guarantee, it must be implemented as a shared tool-boundary policy for both manual and scheduled execution and proven by denial tests.

Product/UI consequences:

- Remove absolute “automations do not …” claims from `PRODUCT.md`, `README.md`, and `doc/AUTOMATION_PLAN.md`; replace them with the contract above.
- Keep “read-only” only as a description of the intended task/answer shape, for example “one short answering step for a read-only information check”. Do not use it as a security guarantee.
- Correct the research-expected-result helper text so it does not imply the planner consumes a field that it does not consume. Preferred fix: pass the expected result into the planning context. Acceptable fallback: state that it is saved as acceptance context and is not used to generate the plan.
- Correct the stale `Request.agent` comment, or document that the app currently never selects a non-default agent.
- Do not add a separate automation agent configuration. That would hide the gap instead of fixing the contract.

### Strict-enforcement option (not recommended for T18)

If the product insists on “no external writes” as a hard guarantee, the implementation needs a shared capability policy used by manual and scheduled runs: an explicit allowlist for read/browser/query tools, denial of shell writes, network POST/mutation, email/IM, trading, publishing, and deletion; a recorded denial event; and tests that prove allowed evidence collection still works. That is a larger platform change and should be its own design/acceptance item, not a keyword filter.

---

## 6. Minimal shared validation boundary

### 6.1 New module

Add `src/automation_policy.rs` (and `mod automation_policy;` in `src/main.rs`). Keeping this in its own module avoids further growth of the heavily edited `automation.rs` and gives one owner for the contract.

Proposed API:

```rust
pub(crate) struct Rejection {
    pub action: &'static str,
}

/// Heuristic creation-time input validation. This is not a capability boundary.
pub(crate) fn rejection(text: &str) -> Option<Rejection>;

/// Validate both user-editable instruction fields with one rule set.
/// `expected_required` is false for the step-one form because Quick may
/// derive a default expected answer; true for saved drafts and templates.
pub(crate) fn validate_instruction_fields(
    objective: &str,
    expected: &str,
    expected_required: bool,
) -> Result<(), String>;

pub(crate) fn rejection_message(action: &str) -> String;
```

### 6.2 Matching rules

- Normalize with `to_lowercase()`; keep Chinese text intact.
- Keep a table of phrase variants grouped by action, for example:
  - sending email: `send email`, `send an email`, `email the`, `email it`, `发送邮件`, `发邮件`, `邮件发送`
  - sending messages: `send message`, `send a message`, `message the`, `post to slack`, `发送消息`, `发消息`
  - trading/orders: `place order`, `place a trade`, `buy shares`, `sell shares`, `transfer funds`, `wire funds`, `下单`, `买入`, `卖出`, `转账`, `自动交易`
  - publishing/deploying: `publish the`, `publish report`, `post publicly`, `deploy the`, `自动发布`, `发布报告`, `发布到`
  - git writes: `git push`, `push to github`, `提交到仓库`
  - deletion: `delete file`, `delete the file`, `remove files`, `rm -rf`, `删除文件`, `删除目录`
  - scripts/commands: `run script`, `run a script`, `execute script`, `shell command`, `执行脚本`, `运行脚本`
- Prefer specific phrases over bare words. Do not reject bare “publish”, “order”, “buy”, or “sell”; they occur in ordinary research topics (“publishing industry”, “order backlog”, “buy-side analysts”).
- For ASCII phrases, require a non-alphanumeric boundary before and after the match. For Chinese phrases, substring matching is acceptable.
- Scan every occurrence with `match_indices`; do not stop after the first.

### 6.3 Negation handling

- For each match, inspect the clause text immediately before it, from the previous sentence/clause delimiter (`. ! ? ; ,` newline, `。！？；，`) up to a bounded window (roughly 24 ASCII characters / 12 Chinese characters).
- If that window contains a negation marker, skip that occurrence and continue scanning.
- Negation markers: `do not`, `don't`, `does not`, `never`, `no need to`, `without`, `avoid`, `not`, `instead of`, `无需`, `不用`, `不要`, `不得`, `禁止`, `别`, `勿`, `不需要`.
- Negation applies only within the same clause. “Do not send email, but publish the report to Slack” must still reject because `publish` is outside the negation scope.
- Also skip “instead of sending email” phrasing, which expresses a preference, not a request.
- Document that negation handling reduces false positives; it does not make the scanner exhaustive or turn it into enforcement.

### 6.4 Wording

Replace the current “which the first automation version cannot perform” message. Proposed:

> This task appears to ask for {action}. Automation creation accepts information-collection and local-report work only; remove that action to continue. This is input validation, not a read-only sandbox.

Add a short muted note under the step-one fields:

> Automations are for read-only information checks and local reports. Obvious external-action requests are rejected here as input validation; approved steps still use your selected agent and its normal permissions.

---

## 7. Precise implementation changes

Do these after T01 and T04/T09 finish editing overlapping UI files. Preserve all current uncommitted work; do not reformat unrelated regions.

### 7.1 New policy module

- Add `src/automation_policy.rs` with the API and rules in section 6.
- Add `mod automation_policy;` in `src/main.rs` near the automation modules.
- Add unit tests in that module.

### 7.2 Domain draft validation

`src/automation.rs`, `AutomationDefinition::validate_draft` (around lines 432-441):

- Keep the name check.
- Replace the separate objective/expected empty and length checks with `automation_policy::validate_instruction_fields(&self.objective, &self.expected_result, true)`.
- This makes `create_automation_draft` (line 340), `revise_automation` (line 425), `api` plan application (line 280), and `approval_for` (line 227) reject unsupported text at one boundary.
- Behavior note: a legacy saved draft with unsupported text will now fail on enable with an actionable error. That is acceptable and should be covered by a test.

### 7.3 Template builders

`src/automation_execution.rs`:

- Remove or replace the local `validated_instruction` body (lines 174-190) with the shared validator; use `expected_required = true`.
- Make `unsupported_reason` a thin compatibility wrapper over `automation_policy::rejection`, or update its callers and delete it. Update the existing test at lines 1197-1200 accordingly.
- Update the comment at lines 192-193 to say “obvious requests are rejected as input validation; this is not an enforced capability boundary.”

### 7.4 Creation form

`src/automation_views.rs`, `automation_validate_step_one` (around lines 943-966):

- Read both objective and expected first.
- Call `validate_instruction_fields(&objective, &expected, false)`.
- Remove the direct `unsupported_reason(&objective)` call.
- Add the muted note from section 6.4 under the field group.
- Correct the research expected-result helper text (lines 2996-2999) as described in section 5. If the supervisor approves the planning-input change, see 7.5; otherwise change the copy.

### 7.5 Research planning input (recommended, small)

`src/request_planning.rs` and `src/automation_views.rs`:

- Extend `create_automation_request` to accept the expected result (or a small instruction-context struct).
- Include it in the planning context JSON and add one prompt sentence: preserve the expected deliverable as acceptance criteria.
- This makes the field truthful and ensures a clean objective with an unsupported expected result is rejected before planning. It also means an expected-result edit should trigger a new plan/visible replan requirement; state that explicitly rather than silently keeping the old template.
- If the supervisor wants strict T18 scope, skip this sub-step and only fix the helper text; do not leave the current misleading copy.

### 7.6 Documentation and comments

- `PRODUCT.md:38`: replace the absolute external-write sentence with the truthful contract.
- `README.md:60`: same; keep the shared-path and no-separate-agent statements.
- `doc/AUTOMATION_PLAN.md:13`: same; add a “Capability limits” note near section 1 or 5 that names the residual limits from section 4.
- `doc/AUTOMATION_PLAN.md:21` and `USER_GUIDE.md:187`: keep “read-only” only as task/answer intent, not security.
- `src/opencode.rs:30`: correct the `agent` comment.
- Coordinate with T17, which owns the broad documentation reconciliation. T18 should change only the external-write/capability claims.

### 7.7 Explicit non-goals

- Do not add a second automation agent configuration.
- Do not add a dead `read_only` config flag that nothing enforces.
- Do not claim post-run audit or sandboxing.
- Do not modify scheduling, retries, or dispatch semantics.

---

## 8. Tests and acceptance scenarios

### 8.1 Unit tests (`automation_policy.rs`)

- `rejects_expected_only_email_request`: clean objective, expected “Send email with the answer” → `Some(sending email)`.
- `rejects_wording_variants`: “email the results”, “send the report to the team”, “transfer funds”, “place a trade”, “deploy the site”, “remove the files”, “run a shell script” → rejected.
- `rejects_chinese_variants`: “发送邮件”, “自动交易”, “删除文件”, “执行脚本” → rejected.
- `accepts_negated_instructions`: “Do not send email; summarize public pages”, “Never publish, trade, or delete files”, “不要发送邮件，只汇总公开信息”, “No need to send messages” → accepted.
- `negation_does_not_hide_a_separate_positive_request`: “Do not send email, but publish the report to Slack” → rejected.
- `does_not_match_ordinary_research_vocabulary`: “Analyze the publishing industry and order backlog” → accepted.
- Add a module comment: these tests prove input validation, not a sandbox.

### 8.2 Template and domain tests

`src/automation_execution.rs`:

- Extend `template_is_stable_and_rejects_external_actions`:
  - `build_quick_template("Check public prices", "Send email with the result")` is `Err`.
  - `build_template("Track filings", "Publish the report to Slack")` is `Err`.
  - A clean quick/legacy template still builds and its instructions contain the expected text.
- Keep the existing accepted-text stability assertion.

`src/automation.rs`:

- `validate_draft` rejects expected-only unsupported text.
- A draft saved before this change with unsupported text fails `approval_for` with the new message.
- `create_automation_draft` rejection leaves `store.automations` unchanged.
- `revise_automation` rejection leaves the existing definition/version unchanged.

### 8.3 Offline QA with a harmless fake CLI

Add `instruction_validation_check(root)` to `src/automation_qa.rs` and call it from `run()` after `pure_checks`:

1. Create an isolated directory and a fake OpenCode executable that:
   - appends one line to a marker file inside that directory when invoked;
   - prints one valid JSON result;
   - performs no network, email, IM, publish, trade, or deletion.
2. Point `store.opencode_path` at the fake CLI.
3. Attempt to create a Quick draft with a clean objective and expected text “Send email the answer”. Assert:
   - the call returns `Err`;
   - `store.automations` is empty and `store.runs` is empty;
   - `plan_automation_ticks(now)` returns no plan;
   - the marker file does not exist.
4. Repeat for a Research draft with unsupported expected text; assert no draft, template, approval, occurrence, or run.
5. Assert a negated instruction is accepted and does not dispatch by itself.
6. Control case: create/enable a benign task, run the existing fake-CLI execution path, and assert the marker is written exactly once inside the isolated directory and the period completes. This proves the check did not break allowed evidence work; it does **not** prove the agent cannot write externally.
7. Add the same marker assertion to the existing `execution_chain` if it is cheaper than a new control case.

Do not label these tests “sandbox”, “read-only enforcement”, or “external-write denial”. Label them `input_validation_*` and add a comment that runtime capability is unchanged.

### 8.4 UI acceptance

- Step one shows the new input-validation note.
- Typing an unsupported expected answer and pressing Continue shows the rejection in the existing feedback area and does not advance to step two.
- Typing a negated instruction proceeds normally.
- Research helper copy matches what the planner actually consumes.
- The rejection message does not say “cannot perform” or imply a sandbox.

### 8.5 Commands

- `cargo test --quiet`
- `cargo run --features qa -- --qa-automation`
- If any UI copy/layout changed, the relevant isolated UI QA and screenshots. Icon checks are unnecessary unless an icon changes.
- Record the exact command output and any pre-existing unrelated failures; do not claim T18 acceptance from a partial run.

---

## 9. Acceptance checklist for the later implementation

- [ ] Unsupported action solely in expected-answer text is rejected for Quick.
- [ ] Unsupported action solely in expected-result text is rejected for Research at save/approval.
- [ ] Objective, expected result, and both fields together are checked at the same boundary.
- [ ] Wording variants and Chinese variants are covered.
- [ ] Negated examples are accepted; mixed negation plus a separate positive request is rejected.
- [ ] Tests explicitly state they are input validation, not a sandbox.
- [ ] A benign fake-CLI control run still completes with no external effects.
- [ ] No real email, IM, trade, publish, script, or delete is executed by tests.
- [ ] PRODUCT, README, AUTOMATION_PLAN, and USER_GUIDE no longer promise enforced external-write prevention.
- [ ] Residual limits are named in maintained documentation.
- [ ] No separate automation agent configuration or dead capability flag was added.
- [ ] T17 documentation ownership and T01/T04/T09 UI work are coordinated.
- [ ] Uncommitted work is preserved; only intended files change.

---

## 10. Coordination and preservation notes

Current working tree already has uncommitted changes in `src/automation.rs`, `src/automation_execution.rs`, `src/automation_views.rs`, `src/automation_runtime.rs`, `src/app.rs`, and several docs. T01 and T04/T09 are editing overlapping UI files. The later T18 implementation must:

1. Start from the post-writer tree and re-read the exact functions before editing.
2. Avoid whole-file rewrites and unrelated formatting.
3. Keep existing automation scheduling/retry semantics untouched.
4. Preserve the current uncommitted work; do not revert or overwrite it.
5. Leave the final acceptance and evidence recording to the implementation run. This audit is not a completion claim.
