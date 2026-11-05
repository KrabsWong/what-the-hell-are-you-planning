# 阶段 2 证据：持久化、调度与恢复

日期：2026-11-05。前置：[01-domain-and-schedule.md](01-domain-and-schedule.md)。

## 实现文件

| 文件 | 作用 |
| --- | --- |
| `src/automation_runtime.rs` | 只读到期扫描、原子提交计划、手动去重、暂停/编辑/归档取消、成功/失败/恢复、时间限制、假时钟/假 runner、应用 timer 与队列交接 |
| `src/sqlite_storage.rs` | schema v3：`automations`、`automation_occurrences`；v1/v2 验证备份升级；未来版本拒绝；批量插入；单次序列化优化 |
| `src/persistence.rs` | JSON 版本 5；已读/已写文件标识缓存，外部变化时重新校验 |
| `src/domain.rs` | Store 自动化字段、Task/Topic/Run 来源字段、`AgentRun.automation_occurrence_id` |
| `src/app.rs` / `src/startup.rs` | 空闲 timer、工作区切换取消、启动恢复、事务失败回滚、队列取消 |

## 故障窗口到测试

| 故障窗口 | 测试 |
| --- | --- |
| 重复 tick 只启动一次；不同自动化同一时刻不冲突 | `automation_runtime::tests::due_tick_queues_once_and_advances_cursor`、`two_automations_due_at_the_same_instant_do_not_collide` |
| 事务前失败不启动；事务后/启动前崩溃 | `commit_automation_tick` 在克隆 Store 上执行；`automation_qa::pure_checks` 使用不可写路径验证 save 失败不改变 Store；阶段 2 的“保存后再入队”顺序在 `poll_automations` 中 |
| 注入写入失败后实例/游标/内存队列一致 | `pure_checks`（Storage 失败不写入）、UI QA 存储失败断言（不显示 Saved/Enabled，Store 回滚） |
| 重复结果导入不重复完成 | `reconcile_occurrence(SessionState::Completed)` + `complete_occurrence` 终态幂等判定；`automation_after_run` 对终态直接返回 |
| 重启：仍运行/已完成/确认停止/未知 | `recovery_covers_running_completed_stopped_and_unknown`；`recover_automations` 按运行日志区分确认停止与未知 |
| 大量错过有界合并 | `missed_periods_are_summarized_and_expiry_skips_without_entities_per_minute`、`year_scale...` |
| 过期项派发前拒绝 | `enqueue_automation_occurrence` 过期检查 + `expire_occurrence`；`dispatch_runs` 再次检查 |
| 手动项不被定时替换 | `manual_run_deduplicates_requests_and_never_moves_the_cursor` |
| 实际调度写入失败回滚 | `--qa-automation` 在到期 tick 注入不可写 Store：实例数/游标/假启动次数均不变，恢复存储后同一点可运行 |
| 暂停/编辑/归档取消未启动项；当前实例保留旧快照 | `pause_edit_archive_and_project_removal_cancel_unstarted_work`、`revise_automation` 测试、UI 操作断言 |
| 共享并发、FIFO、降上限不杀已有工作 | 复用 `dispatch_runs`/`execution_slots_used`；`--qa-concurrency`、`--qa-topic-queue` 通过 |
| 项目不可用/全局存储错误不派发 | `pause...project_removal`、`Storage` 失败测试、UI 存储失败测试 |
| 旧 schema 升级不丢数据；备份可读；失败保持原库 | `sqlite_storage::tests::v2_upgrades_to_v3_with_verified_backup_and_automation_tables`、`upgrade_verification_failure_rolls_back_and_keeps_each_backup`、`persistence::tests::version_four_workspaces_gain_empty_automation_state...` |

## 调度与队列语义

- 到期扫描 `Store::plan_automation_ticks(now)` 只读；`commit_automation_tick` 同时写实例、研究实体和游标。
- 应用先克隆 Store，提交后 `persist_if_changed(true)`；存储失败恢复旧 Store，不保留游标、不派发，并 30 秒退避。
- Timer：`arm_automation_timer` 在最近到期时间或最多 30 秒后唤醒；没有 80ms 常驻轮询。工作区切换/关闭递增 generation，旧 timer 无效。
- 同一自动化最多一个活动实例 + 一个未启动实例；新定时实例替代旧未启动定时实例，旧项记 `SupersededByNewer`；手动待执行时新定时项跳过并记录 `ManualPending`。
- 手动请求使用独立请求 ID；相同 ID 幂等返回，不同 ID 在已有未启动项时拒绝。
- 暂停/编辑/归档取消未启动项；当前运行及已授权重试继续。恢复/重试不跨周期。
- 连续最终失败达到阈值进入 `NeedsAttention`；跳过/取消不增加失败计数。

## 实际命令与结果

```text
$ cargo test --quiet
test result: ok. 273 passed; 0 failed; 2 ignored

$ cargo run --features qa -- --qa-automation
pure automation checks passed
Automation QA passed · .local/automation-qa/98061
```

`--qa-automation` 使用隔离 SQLite 工作区、假 runner、无模型/网络，覆盖到期、去重、手动、暂停/编辑/归档、失败阈值、恢复四态、存储失败和 500 条历史。

回归：

```text
--qa-concurrency   passed
--qa-topic-queue   passed
--qa-retries       passed
--qa-batch         passed
--qa-publishing    passed
--qa-settings      passed
python3 script/check_icons.py   47 registered icons embedded
```

既有未通过项（在基线 `e9cbd88` 干净 worktree 复跑同样失败，见阶段 5）：

```text
--qa-recovery  panic: step retry did not finish (src/recovery_qa.rs:314)
--qa-research  panic: missing ElementId "research"
--qa-affordance panic: ElementId "approve-topic-1" is not visible
```

## 已知限制

- 全量 Store 快照写入在 43,200 条分钟级历史下（其中 100 期含完整内部研究实体），冷约 1205ms；暖保存 910–931ms。UI 回调内同步保存实测阻塞 1069ms，远超 100ms 交互预算。详见阶段 5。
- 产品构建默认关闭真实无人值守派发；只有 QA 构建（假 CLI、隔离工作区）可以执行验收链。
- 恢复时“确认停止”依赖正常关闭写入的 `[interrupted] Application closed...` 日志；崩溃留下的 `Running` 归为未知并进入核验，不盲目重放。
- 阶段 2 的 fake runner 仅用于 QA；生产派发在阶段 3 接入真实 OpenCode 队列（已实现，但真实 smoke 未执行）。
