# 阶段 1 证据：领域模型与时间计算

日期：2026-11-05。前置阶段 0 记录见 [00-feasibility.md](00-feasibility.md)。

## 实现文件

| 文件 | 内容 |
| --- | --- |
| `src/automation_schedule.rs` | 周期类型、IANA 校验、下一次/下三次、到期决策、补跑汇总、DST、前一自然日窗口、预览 |
| `src/automation.rs` | 定义/版本/批准/实例/快照/状态机、触发键、策略校验、Store 读写辅助 |
| `src/domain.rs` | `Store.automations`、`Store.automation_occurrences`、`automation_schema_version`、Task/Topic/Run 来源字段 |
| `Cargo.toml` / `Cargo.lock` | `chrono-tz = "0.10"`（锁定 0.10.4） |

时区依赖选择：`chrono-tz` 提供编译期 IANA 数据库，支持 `LocalResult::None/Ambiguous`，无需自行维护 DST 表；`chrono` 已在项目中用于 UTC/本地时间。没有使用固定偏移或系统时区猜测。

## 验收项到测试映射

| 验收项 | 测试 / 证据 |
| --- | --- |
| 10:03 启用每 5 分钟 → 10:08/10:13/10:18，运行时长不推动序列 | `automation_schedule::tests::interval_preview_is_fixed_from_the_enable_anchor_not_completion` |
| 每日/每周跨月、跨年、闰日 | `daily_and_weekly_cross_month_year_and_leap_day` |
| 空星期集、无效时区、零/负/溢出拒绝 | `invalid_schedules_are_rejected`、`policy_validates_positive_bounded_values` |
| DST 不存在时刻跳过、重复时刻只一次 | `dst_nonexistent_time_is_skipped_and_ambiguous_time_occurs_once` |
| 系统时区变化不改变已存规则 | `stored_rules_ignore_the_process_timezone`、`serialization_round_trip_preserves_schedule` |
| 补跑只选最新且未过期；暂停不补历史；回拨不回退 | `catch_up_only_chooses_the_latest_trigger_and_summarizes_older_ones`、`expired_latest_is_flagged_but_cursor_still_advances`、`clock_rollback_never_replays_a_processed_trigger` |
| 月/年级错过算法有界、不产生大量实体 | `year_scale_missed_range_is_bounded_and_not_expanded_into_entities`（43,200 分钟级历史只生成 1 条跳过记录） |
| 定时键按 (automation, 时间) 去重，不随版本变化；手动键不同；不同自动化同一时刻不冲突 | `automation::tests::trigger_keys_differ_and_survive_version_changes`、`automation_runtime::tests::two_automations_due_at_the_same_instant_do_not_collide` |
| 前一自然日按任务时区；只有显式“昨天”才解析；分钟/上周/交易日保持未解析；重试相同 reference_time 相同窗口 | `previous_natural_day_uses_the_task_timezone`、`only_explicit_yesterday_resolves_a_window`、`occurrence_run_uses_snapshot_model_and_time_window`、`two_periods_have_distinct_reference_windows_paths_and_sessions` |
| 序列化往返、合法/非法状态转换 | `automation::tests::serialization_round_trip_and_legacy_defaults`、`occurrence_transitions_reject_completed_reentry_and_skip_preserves_reason`、`approval_is_bound_to_version_template_and_schedule` |
| 代码不触发进程或网络 | 模块只依赖 serde/chrono/hash；阶段 1 单元测试不启动 CLI |

## 实际命令与结果

```text
$ cargo test --quiet automation_schedule
running 12 tests ... test result: ok. 12 passed

$ cargo test --quiet automation::
running 6 tests ... test result: ok. 6 passed

$ cargo test --quiet
test result: ok. 273 passed; 0 failed; 2 ignored
```

（全量测试在阶段 2/3 集成后再次运行；上面的 270 是当前最终数量。）

## 语义说明

- 间隔：首触发 = 启用时刻 + N；之后固定 `anchor + k*N`，不按完成时间漂移；`u32` 分钟转换为 `i64` 秒并检查上限（10 年），避免溢出。
- 日历：每日/每周按当地 wall clock 解析；不存在时刻跳过，重复时刻取最早 UTC；搜索最多 16 天，不按分钟创建实例。
- 到期：`decide` 只在 `now >= cursor` 时给出候选，时钟回拨不会重放；最新候选前移时，旧触发合并为 `skipped_count/from/to` 摘要。
- 允许延迟：从计划触发点算到真正开始前；超时标记 `Expired` 并推进游标。
- `data_window`：只解析前一个自然日（半开区间 `[start, end)`），业务交易日不假设，状态可为 `Unresolved`。
- 批准快照：绑定定义版本、模板签名、调度签名、模型/agent、材料签名、权限配置；版本变化即失效。
- 状态机：定义 `Draft/Enabled/Paused/NeedsAttention/Archived`；实例 `Queued/Running/WaitingRetry/Verifying/Succeeded/Failed/Interrupted/Cancelled/Skipped`；终态不可重入，恢复核验不会伪装成功。

## 未在此阶段完成（现已由后续阶段接入）

- 阶段 1 本身没有 UI、Store 保存或真实进程；当前代码已在阶段 2–5 接入。未接入时不得把这些测试当作端到端能力。
