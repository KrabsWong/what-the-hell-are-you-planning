# 阶段 3 证据：研究执行适配

日期：2026-11-05。前置阶段 2。权限门槛：配置层通过、执行层未实测（见阶段 0）。

## 实现文件

| 文件 | 作用 |
| --- | --- |
| `src/automation_execution.rs` | 确定性模板、批准快照、实例化内部 Topic/Goal/步骤、时间上下文、受限 OpenCode 配置、材料签名 |
| `src/automation_runtime.rs` | 把批准的 occurrence 转为现有 research 队列运行、步进、重试状态、超时、恢复 |
| `src/research.rs` / `src/request_planning.rs` | 未改变普通研究语义；仅新增运行来源字段 |
| `src/opencode.rs` | `Request.agent` 可选，普通研究仍为 `build`；自动化使用 `automation-research` |
| `src/report_publish.rs` | 自动化 Topic 从 `topic_share_content` 排除，自动发布扫描无法选中 |
| `doc/RESEARCH_WORKFLOW_STRATEGY.md` | 新增自动化时间/批准/执行/限制说明 |

## 计划生成与批准

- `build_template` 是应用内确定性结构生成器（收集→分析→报告），不用模型在每期自由规划。用户的目标和预期结果进入固定步骤指令；日期和参考时间在派发时注入。
- 未批准模板不可运行。`Store::approve_and_enable` 要求模板通过现有 `research::parse_plan` 校验，并把有效模型、agent、材料签名、权限、调度签名绑定到批准版本。
- 非 OpenCode 的收集/分析/报告 agent 在批准时被拒绝；避免用提示词宣称 Codex 权限边界。
- 所有普通研究启动入口（步骤 `Run again/Retry step`、`Run goal`、`Run / resume`、批次、报告 `Regenerate`）都会先检查自动化归属；自动化所属步骤统一回到 occurrence 队列，`launch_run` 和派发前还有兜底拒绝。门禁关闭时任何入口都不启动。
- 报告重新生成在开启时保留 occurrence、模型/agent 快照、时间窗口和受限权限配置；本次执行使用独立时限，超时会取消进程并保留上一次成功报告。
- 产品构建默认禁用真实无人值守派发（未完成执行层验证）；QA 构建以假 CLI 验证链路。界面显示禁用原因，启用按钮不可用。
- 编辑目标、步骤、模型、材料、调度或策略会递增版本、清空批准、取消旧版未启动实例；当前运行继续使用旧快照。
- 全局模型变化不影响已启用自动化；`prepare_occurrence_run` 从 occurrence 快照恢复模型和 agent。

## 每期隔离与时间上下文

- `materialize_occurrence` 创建内部 Topic（`automation_id`）、独立 Goal、3 个步骤和独立 Space 名称；每个 Goal 有独立执行目录（`execution_directory`）。
- 时间窗口只在请求**只**明确说“昨天/昨日/yesterday”时解析为前一自然日；混合、比较、否定表达（如“上一个交易日走势＋昨天新闻”“比较今天和昨天”“不要只看昨天”）以及“过去五分钟”“上周”等保持未解析，由收集步骤按 reference_time 验证，避免篡改用户要求。
- `prepare_occurrence_run` 注入 `AUTOMATION OCCURRENCE CONTEXT`：
  - `scheduled_for`、`actual_started_at`、`timezone`、`reference_time`、`data_window`；
  - “yesterday”等相对日期以 reference_time 为准，不以实际开始日为准；
  - 重试保持同一期参考时间和窗口；
  - 无人值守：浏览器、交互提问、shell 被拒绝；只允许只读抓取/搜索和公开数据 MCP。
- 实例总时限从首次实际开始写入 `deadline_at`，重试/重启不重置；派发和轮询都检查。
- 每期最终成功要求所有步骤 Done 且最终 report 通过验证；进程 exit 0 不算成功。
- `--qa-automation` 的 `execution_chain` 通过真实队列与假 CLI 跑完 3 个步骤：独立 session、目录、权限配置、最终报告与 occurrence 成功；第一步故意耗时 2.2s、允许延迟 1s，后续步骤仍正常执行，验证允许延迟只限制首次启动。`recovery_chain` 模拟崩溃态、重开、真实会话核验、结果导入并继续后续步骤，再验证下一周期可执行。`entry_route_check` 验证门禁关闭时步骤入口不启动、开启后仍走 occurrence；`queue_chain` 用**同一自动化**的 A1/A2 验证 A2 等待、A1 完整执行后才启动 A2；`execution_chain` 同时验证报告重新生成的门禁与快照归属；`pause_recovery_chain` 验证运行中暂停后重启核验不会自动恢复；`archive_recovery_chain` 验证归档后重启核验仍为 `Archived` 且不产生下一期；`regeneration_recovery_chain` 验证重启后过期重新生成即使核验返回完成也不覆盖原报告并释放恢复槽。仍不调用真实模型。

## 权限配置

`write_permission_profile` 在每次运行目录写：

```json
{"agent":{"automation-research":{"mode":"primary","permission":{
  "*":"deny","read":"allow","glob":"allow","grep":"allow","list":"allow",
  "webfetch":"allow","websearch":"allow","lsp":"allow",
  "edit":"deny","bash":"deny","task":"deny","external_directory":"deny",
  "todowrite":"deny","question":"deny","skill":"deny","doom_loop":"deny"}}}}
```

派发时使用 `--agent automation-research`。文件已存在且内容不同则阻止运行并要求检查，不覆盖用户文件。全局 OpenCode 配置不改。

## 发布隔离

- 自动化 Topic 的 `automation_id` 非空；`topic_share_content` 返回 `None`。
- 自动发布扫描调用同一函数，因此重启后、开启全局自动发布时也无可发布内容。
- 测试：`automation_execution::tests::automation_topics_are_excluded_from_normal_publication`；普通发布回归由 `--qa-publishing` 通过。

## 验收项到测试/操作

| 验收项 | 证据 |
| --- | --- |
| 生成/保存草稿不启动；批准后至少两期独立完成 | `--qa-automation` 完整创建→批准→强制到期→fake 运行→结果；`two_periods_have_distinct_reference_windows_paths_and_sessions` |
| 两期 session/目录/Space/结果不混用 | 上述测试断言 goal、执行目录、Space 名不同；session 由派发新建，测试断言准备阶段无 session；同一期重试保留 occurrence id |
| “昨天”逐期变化、补跑/重试不漂移 | 两期窗口相差 86,400s；重试用例断言相同 reference time 与上下文块；`previous_natural_day...` |
| 自动化步骤的普通入口不能绕过实例/门禁 | `entry_route_check`（步骤）、`execution_chain` 的报告重新生成断言、`launch_run`/派发兜底；门禁关闭均不启动 |
| 同一自动化在途新增实例不阻塞当前实例 | `queue_chain`：同自动化 A1 收集中新增 A2，A2 等待，A1 完整 3 步后才开始 A2（校验实际开始时间） |
| 编辑内容/材料/配置使旧待执行项失效；当前实例不被改 | `revise_automation`、UI 编辑后版本+1/旧 pending 取消；occurrence 保存独立 snapshot |
| 未授权工具执行层拒绝 | **未实测**；配置层 `opencode debug agents` 已验证规则加载。真实模型调用未执行 |
| 报告重新生成不能绕过门禁/快照，并有独立超时；重启恢复也不接受过期结果 | `execution_chain`（门禁、快照、挂起进程超时）；`regeneration_recovery_chain`（重启、过期核验完成：discard late output、slot released、原报告不变） |
| 重启核验完成后恢复下一周期 | `recovery_chain`：核验成功 → 定义恢复 Enabled/游标 → 下一周期实际执行成功 |
| 用户主动暂停/归档不被恢复覆盖 | `pause_recovery_chain`、`archive_recovery_chain`：重启未知核验并完成当前实例后，仍为 `Paused`/`Archived`、`cursor=None`；归档不再产生下一期 |
| 浏览器接管进入需处理、不无限重试 | 自动化 agent 不启用浏览器；`Blocked` → `Verifying` + 定义 `NeedsAttention`，不进入自动重试 |
| 部分步骤失败、报告校验失败、超时、停止、重试耗尽状态准确 | `success_requires_final_report_not_process_exit`、`failure_threshold_pauses_and_success_clears_counter`、`deadline_blocks_new_steps...`、`stop_occurrence`、`automation_after_run` |
| 自动发布开启不发布自动化；普通发布不回归 | `automation_topics_are_excluded...`、`--qa-publishing` |
| 普通研究审批/submitted_at/恢复未放宽 | `cargo test` 全部既有研究测试通过；自动化只新增分支 |

## 实际命令与结果

```text
$ cargo test --quiet
test result: ok. 273 passed; 0 failed; 2 ignored

$ cargo run --features qa -- --qa-automation
pure automation checks passed
real queue automation chain passed (fake CLI, 3 steps, isolated sessions)
recovery automation chain passed (crash, reopen, session import, continuation, next period)
step entry routing passed (gate blocks, occurrence path owns runs)
queue chain passed (in-flight next instance waits, A1 completes, then A2)
Automation QA passed · .local/automation-qa/98061

$ cargo run --features qa -- --qa-publishing
PUBLISHING QA PASSED
```

## 未执行 / 限制

- 真实模型 smoke test 未执行：未获明确授权的模型/任务/工作区，也未证明工具拒绝在运行时发生。因此产品构建默认禁止真实无人值守运行；**真实执行链路尚未验证**。
- 材料选择器未接入 UI，批准材料为空；材料签名函数和快照字段存在。
- 不支持 Codex 无人值守；不支持外部写动作、发信、发布、交易。
- 模板是确定性结构，不是模型按任务定制；这是首版“不每期自由规划”的明确取舍。
