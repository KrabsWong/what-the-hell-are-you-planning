# 阶段 0 证据：执行边界与接入调查

日期：2026-11-05。基线：`e9cbd88`。开始时工作区只有 `doc/AUTOMATION_PLAN.md` 与 `doc/automation-tasks/` 为未跟踪文件；没有业务源码改动。

> 说明：本记录在实施期间整理，命令均在本机实际执行。阶段 1–5 的代码是在完成这些只读调查后接入的。未执行的运行时验证明确标为未验证。

## 1. 执行器与版本

```text
$ which opencode
/opt/homebrew/bin/opencode
$ opencode --version
opencode v2.2.5
$ opencode run --help
... --agent string   --auto   --session   --format   --file ...
```

应用当前启动参数固定为：

```text
opencode --standalone run --format json --model <model> --agent <agent> --title <title>
```

（`src/opencode.rs::start_adapter`；本次仅为自动化增加可选的 `--agent automation-research`，普通研究仍使用 `build`。）

## 2. 权限与无人值守边界

### 已实测

在临时目录写入项目级 `opencode.json`：

```json
{"agent":{"automation-research":{"mode":"primary","permission":{
  "*":"deny","read":"allow","webfetch":"allow","websearch":"allow"}}}}
```

在该目录执行：

```text
$ opencode debug agents
```

输出中的 `automation-research` 有效规则为：

```text
*         deny
read      allow
webfetch  allow
websearch allow
browser   deny   (build 默认规则)
skill/browser deny
```

结论（仅配置层）：OpenCode 确实会从**运行目录**加载项目级配置，并把 `permission` 映射并入该 agent 的有效规则；不需要修改 `~/.config/opencode/opencode.json`。实施中每个自动化运行目录由 `automation_execution::write_permission_profile` 生成该文件，并使用 `--agent automation-research`。

### 文档支持但未实测

- 实际工具调用的**执行层拒绝**：需要一次真实模型工具调用才能证明。本次没有调用真实模型，因此标为未验证。
- 浏览器/MCP 工具是否全部落入通配 `*` 拒绝：`debug agents` 显示 `browser` 被拒绝，但 MCP 子进程/账号级能力未做端到端验证。
- Codex：自动化审批拒绝非 OpenCode agent（见阶段 3），因此不宣称 Codex 无人值守权限边界。

### 无法保证 / 残余限制

- 禁止用提示词充当权限沙箱；本次依赖项目级 OpenCode 配置，但运行时不保证所有第三方 MCP 插件均遵守该规则。
- 自动化不能登录、接管浏览器、运行 shell、写文件或提问；需要这些能力时返回证据缺口或进入需处理。
- 不修改用户全局 OpenCode 配置，不自动升级 CLI。

**阶段 3 权限门槛结论：配置层通过；执行层拒绝未实测。** 因此自动化仅支持 OpenCode，真实 smoke test 未执行，产品构建默认**禁用真实无人值守派发**：`TaskApp.automation_unattended_enabled` 仅 QA 构建为真；普通构建中启用按钮禁用，排队项也不会启动，并进入需处理。不能宣称真实无人值守安全已完全验证。

## 3. 接入点（实际文件/符号）

| 接入点 | 实现位置 | 行为 |
| --- | --- | --- |
| 模板生成 | `automation_execution::build_template` | 确定性 3 步模板（收集→分析→报告）；不每期自由规划 |
| 批准与快照 | `automation_execution::approval_for` / `Store::approve_and_enable` | 固定版本、模板签名、调度签名、有效模型/agent、材料签名、权限配置 |
| 派发前复核 | `app::TaskApp::dispatch_runs` 自动化分支 | 复核定义版本/批准、项目、过期、总时限、权限文件；通过后才写运行状态 |
| 统一队列 | `TaskApp::queued_runs` / `dispatch_runs` | 与手动研究共享并发上限、目录/Topic/Space 资源冲突和 FIFO |
| 实例聚合 | `Store::automation_occurrences`、`occurrence_report` | 历史只从自动化页面进入，不混入日常看板 |
| 恢复 | `TaskApp::recover_automations` + `automation_runtime::reconcile_occurrence` | 运行/完成/确认停止/未知四类；未知进入核验并暂停派发 |
| 自动发布排除 | `report_publish::Store::topic_share_content` | `topic.automation_id.is_some()` 直接返回 `None`，自动扫描无内容可发 |
| 项目不可用 | `Store::automation_blocker` | 项目缺失/批准过期/连续失败进入需处理，不派发 |
| 普通列表过滤 | `TaskApp::matches_scope`、侧栏项目树、`run_views::run_projects`、`progress_result_task_count` | 自动化内部 Task/Topic/Run 默认不显示 |
| 应用关闭 | `TaskApp::shutdown` / `on_app_quit` / `on_release` | 停止活动进程、取消自动化 timer；队列和运行在下次启动核验 |
| 工作区切换 | `startup::begin_workspace_open` | 递增 generation 并取消旧 timer，旧 Store 不会再派发 |
| 普通研究兼容 | 未修改 `research::prepare_run` 的语义；自动化只增加运行快照覆盖和提示块 | 普通研究的 `submitted_at` 时间基准、审批和恢复策略保持不变 |

## 4. Store 单写者与事务

- 唯一写入口仍是 `persistence::Storage::save` → `sqlite_storage::save`；没有第二个数据库、旁路文件或第二写者。
- 自动化 tick 在内存克隆上完成“实例 + 关联 research 实体 + 下次游标”，再以整库事务写出；失败恢复旧 Store，不保留已前进游标，也不派发。
- SQLite `user_version` 从 2 升到 3，新增 `automations`、`automation_occurrences` 表。v1/v2 升级先做可读备份，再写入并校验；未知版本拒绝打开/写入。
- JSON 文件版本从 4 升到 5，旧字段由 serde 默认值补齐；未知新版本拒绝。
- `Storage` 缓存最近一次已读/已写文件标识，避免每次保存前重新全量读取；文件被外部修改时重新校验，仍拒绝覆盖损坏/未来版本数据。

## 5. ID 与来源关系

- `Task.automation_id`、`Topic.automation_id`：标记自动化内部研究实体。
- `AgentRun.automation_occurrence_id`：从运行回到唯一实例。
- `AutomationOccurrence.research_goal_id` / `research_topic_id` / `run_ids` / `final_run_id`：实例聚合、结果跳转和发布排除。
- 触发唯一键：定时 `scheduled:<automation-id>:<utc>`，手动 `manual:<automation-id>:<request-id>`；不同自动化的同一时刻互不冲突，定义版本不参与键。
- 普通研究审批签名未复用或伪造：自动化 occurrence 的真实 Goal 使用 `goal_plan_signature` 写入 `approved_plan`，来源是自动化批准快照。

## 6. 时间与时区

- 新增依赖 `chrono-tz 0.10.4`（IANA 时区数据），理由：固定 UTC 偏移不能满足 DST、本地日历和跨月/年规则。
- 所有时间函数显式接收 `now: i64`；应用层唯一取时点是 `TaskApp::automation_now()`（UTC 秒），测试使用固定时间。
- 预览和调度共用 `automation_schedule::next_after` / `latest_due_for`；不存在模型生成的下次时间。
- `data_window` 只解析“任务时区的前一个自然日”；业务交易日/财报期未验证时保留未解析，由收集步骤报告缺口。

## 7. 静态材料与模型快照

- 批准快照可保存 `AutomationMaterial { task_id, name, signature }`；签名由应用在批准时计算 SHA-256。
- 当前管理界面没有材料选择器，因此新建自动化默认材料列表为空；能力已在域模型/审批中实现，但 UI 未暴露，属于已知限制。
- 有效模型在批准时解析为明确字符串（不保留“继承”动态值）；全局模型变化不会改变已启用自动化。

## 8. 阻塞项与处理

| 问题 | 影响 | 处理 |
| --- | --- | --- |
| 真实模型工具拒绝未实测 | 不能宣称权限边界已端到端验证 | 自动化仅 OpenCode；生成受限配置；产品构建默认禁用真实无人值守派发（QA 构建才允许假 CLI 验收）；真实 smoke 未执行 |
| Codex 无同等项目级权限配置验证 | 不能安全无人值守 | 审批阶段拒绝非 OpenCode agent |
| 材料选择器未实现 | 批准快照材料为空 | 域模型与签名能力保留；后续阶段增加选择器 |
| 全量快照保存 43,200 条历史 | 暖保存 910–931ms；冷保存 1205ms；UI 回调内同步保存阻塞 1069ms | 已做缓存校验/单次序列化/批量插入；UI 同步保存仍远超 100ms 交互预算。需增量持久化/快照去重，单列设计影响 |
| 基线 QA `--qa-recovery`、`--qa-research`、`--qa-affordance` 失败 | 与自动化无关 | 在 `e9cbd88` 干净 worktree 复跑，失败点相同；未修改这些测试，记录为既有未通过项 |

## 9. 阶段结论

- 阶段 1 可以开始，且已完成。
- 阶段 3 **未被权限问题完全阻塞到不能实现**，但“执行层拒绝”未实测；实现限定 OpenCode、无 `--auto`、受限 agent、无外部写动作，并明确真实链路未验证。
