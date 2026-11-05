# 阶段 5 证据：集成、性能与交付验收

日期：2026-11-05。基线 `e9cbd88`；自动化变更未提交。

## 验收矩阵

| 类别 | 必验行为 | 测试/操作 | 结果 |
| --- | --- | --- | --- |
| 时间 | 分钟/小时间隔、每日、每周；时区、DST、跨月年、前后跳、唤醒；只解析**单一**明确“昨天”窗口，混合/比较/否定（含上月、去年、不要昨日）不归为昨天 | `automation_schedule` 测试；`only_explicit_yesterday_resolves_a_window`（含全部复核示例）；`--qa-automation` | 通过 |
| 记账 | 重复 tick、手动去重、多自动化同刻去重、普通入口与重新生成路由、事务回滚、启动前后崩溃、重复结果导入 | `due_tick...`、`two_automations_due...`、`manual_run...`、`entry_route_check`、`--qa-automation` 调度写入失败回滚、`reconcile_occurrence`、`complete_occurrence` | 通过（事务提交后启动前窗口由启动时 Queued 派发覆盖） |
| 生命周期 | 暂停/恢复、编辑、归档、项目不可用、工作区切换；重启不覆盖用户暂停/归档 | `pause_edit_archive...`、`pause_recovery_chain`、`archive_recovery_chain` | 通过（归档后不再产生下一期） |
| 执行 | 同一期重试、新一期隔离、有限排队、共享并发、过期、总超时、报告重新生成独立超时（含重启恢复过期结果）、同一自动化在途新增不阻塞当前链路 | `two_periods...`、`deadline...`、`automation_after_run`、`queue_chain`（同自动化 A1/A2 实际开始顺序）、`--qa-concurrency`、`--qa-topic-queue` | 通过 |
| 恢复 | 会话运行/完成/停止/未知；授权和快照不因重启丢失；核验结果推进自动化并恢复下一周期；用户暂停保留 | `recovery_covers...`、`recover_automations`、`apply_automation_recovery`、`resume_after_verified_recovery`、`recovery_chain`（验证下一周期） | 通过（未真实杀进程，使用持久化崩溃态 + 真实 CLI 会话核验路径） |
| 权限 | 工具拒绝、接管阻塞、普通入口和报告重新生成不能绕过、不自动发布、不改全局配置 | 配置层 `opencode debug agents`；`write_permission_profile` 联合测试；`entry_route_check`；`execution_chain` 重新生成断言；发布排除测试；产品构建默认关闭真实无人值守 | 配置层通过；执行层拒绝未实测，**生产派发和所有入口已按门槛禁用** |
| 数据 | 旧库升级/回滚、材料变化、报告/来源保留、日志清理边界 | `v2_upgrades_to_v3...`、`upgrade_verification_failure...`、`version_four...`、`automation_log_cleanup`、UI 清理预览 | 通过 |
| UI | 完整操作链、错误修复入口、四组主题/尺寸、键盘和长中文 | `--qa-automation` 断言 + 四组列表/详情截图 + 结果页与权限门槛截图 + 长中文标题 | 通过（键盘复用现有控件，未单独自动化） |
| 性能 | 30 天分钟级历史生成方法、实测数据和预算结论 | `--qa-automation-perf` | 暖保存 910–931ms；UI 同步保存阻塞 1069ms，**未达 100ms 交互预算**（见下） |

## 故障注入覆盖

- 写入失败：不可写目录的 `Storage.save`；UI 保存/启用失败回滚；到期调度 tick 的真实回滚断言（实例/游标/假 runner 均不变，恢复后同一点运行）。
- schema 升级失败：既有 SQLite trigger 测试；v2→v3 验证失败回滚并保留备份。
- 进程启动不明/会话未知：`reconcile_occurrence(SessionState::Unknown)` → `Verifying` + `NeedsAttention`，不重放；`recovery_chain` 模拟崩溃后的真实会话核验、结果导入和后续步骤继续。
- 长任务后续步骤：`execution_chain` 第一步耗时 2.2s、允许延迟 1s，验证允许延迟只限制首次启动，后续步骤不误判过期。
- 断网/限流：沿用现有研究有限重试；自动化实例层不叠加一轮重试。
- 浏览器接管：自动化 agent 禁用浏览器；`Blocked` 不进入自动重试。
- 超时：`deadline_at` 持久化，重试/重启不重置；派发和轮询检查。
- 结果损坏：沿用 `parse_report` 校验；最终成功要求所有步骤和最终报告。
- 取消：停止本次取消当前与后续重试，不改变未来定时。

## 性能

命令：

```text
$ cargo run --features qa -- --qa-automation-perf
```

结果（同一台 Apple Silicon macOS 机器）：

```text
43,200 occurrences (100 with full research entities) + representative reports
cold save: 1205 ms
warm saves: [910, 915, 931] ms
load: 486 ms
history page query: 0 ms
db: 171.9 MB
UI cold open incl. attach: 1532 ms
automation list frame: 0 ms
detail frame: 24 ms
synchronous UI-triggered save block: 1069 ms
```

生成方式：43,200 条一分钟实例（成功/失败/跳过混合）。前 100 个非跳过实例通过正常 `materialize_occurrence` 创建完整内部 Topic/Goal/3 个步骤，并写入每步约 6KB 报告、来源和 12 行日志；其余实例覆盖 30 天调度规模。不是空壳历史，但也没有为全部 43,200 期复制完整研究实体（数据量会达到数 GB，非本次验收目标）。

结论：

- 列表/详情帧 ≤24ms，满足 500ms 显示目标。
- 暖保存 910–931ms；冷保存 1205ms。
- **UI 回调内的同步保存实测阻塞 1069ms，远超 100ms 主线程连续阻塞预算。** 不能用“绘制 24ms”证明交互不阻塞；本次明确不宣称交互性能达标。
- 已做的局部优化：读取/写入文件标识缓存、单次 JSON 序列化、分块多行插入；没有改变持久化架构。
- 瓶颈：整库快照 + 每条 occurrence 保存完整不可变 snapshot（171.9MB）；每次保存重写全部历史行、序列化和 FULL fsync。
- 后续应做增量持久化（按 dirty 实体）、occurrence 快照去重/分表，或在 UI 中把保存移出主线程；这会改变持久化契约，需单独设计与迁移。**不宣布分钟级长期使用验收完成。**

## 命令结果

```text
cargo test --quiet
  ok. 273 passed; 0 failed; 2 ignored

--qa-automation
  pure automation checks passed
  real queue automation chain passed (fake CLI, 3 steps, isolated sessions, regeneration gated and timed out)
  recovery automation chain passed (crash, reopen, session import, continuation, next period)
  paused recovery chain passed (user pause survives restart verification)
  regeneration recovery chain passed (late result discarded, slot released)
  archived recovery chain passed (archive survives restart verification)
  step entry routing passed (gate blocks, occurrence path owns runs)
  queue chain passed (same-automation A2 waits, A1 completes, then A2)
  Automation QA passed · .local/automation-qa/98061

--qa-concurrency
  Concurrency QA passed (3 CLI sessions, 4 isolated chains, cancellation, retry)

--qa-topic-queue
  Topic queue QA passed

--qa-retries
  Retry QA passed

--qa-batch
  Todo batch QA passed

--qa-publishing
  PUBLISHING QA PASSED (no network)

--qa-settings
  MODEL SETTINGS QA PASSED

python3 script/check_icons.py
  All explicit Lucide references are embedded (47 registered icons)
```

既有失败（在本变更前的干净 `e9cbd88` worktree 复跑相同）：

```text
--qa-recovery
  panic: step retry did not finish (recovery_qa.rs:314)

--qa-research
  panic: missing ElementId "research"

--qa-affordance
  panic: ElementId "approve-topic-1" is not visible
```

这些失败未修复、未删除、未吞掉；它们不是本自动化变更引入，也没有被当作通过。由于它们存在，本次不声明完整仓库 QA 全绿。

## 真实 smoke test

未执行。原因：没有明确授权的真实模型、隔离任务和工作区；阶段 0 的执行层工具拒绝也未实测。因此：

- **真实执行链路尚未验证。**
- 产品构建默认关闭真实无人值守派发（启用按钮禁用，队列项不启动）；QA 构建才允许假 CLI 验收链。
- 不宣称报告内容正确性、真实浏览器边界或真实无人值守安全。
- 所有自动化验收均为假时钟、假 runner、隔离 SQLite、无模型/网络。

## diff review

- 新增模块与字段均有 serde 默认值；旧 JSON v4 补齐空自动化状态，旧 SQLite v1/v2 备份升级。
- 普通研究审批、`submitted_at` 时间基准、恢复策略、发布流程未被放宽；自动化通过独立 `automation_id` 来源和快照隔离。
- 未修改全局 OpenCode 配置；未自动提交/推送；未重启用户应用；未删除既有测试。
- 仅调整一个既有测试的等待方式：`recovery::tests::codex_recovery_uses_attempt_lease_even_before_session_event` 在 `drop(lease)` 后用 2 秒有界重试等待 fork/exec 文件描述符窗口，仍断言最终 `Stopped`；不吞掉持续失败。
- 已知风险：UI 同步保存阻塞 1069ms、执行层权限未实测、材料选择器缺失、Codex 不支持。

## 交付阻塞项

1. 真实模型工具拒绝/执行链路未验证；生产派发已默认禁用，需完成执行层验证后才能启用。
2. UI 回调内同步保存阻塞 1069ms，远超 100ms 预算；冷保存 1205ms——不能宣称交互或分钟级长期使用验收完成。
3. 基线已有 3 个 QA 入口失败——完整仓库 QA 非全绿。

## 已知限制

- 仅 OpenCode；Codex 无人值守被拒绝。
- 只有信息收集/分析/本地报告；无外部写操作、发信、发布、自动交易。
- 应用关闭/休眠/离线期间不运行；不唤醒电脑；错过按最近一次/跳过策略处理。
- 自动化结果是内部研究实体，只在自动化历史查看；不进入普通看板/任务树/全局发布。
- 材料选择器、条件分支、依赖节点、后台常驻服务不在首版。
