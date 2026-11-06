# 阶段 6：任务形态与自动化归属

前置：阶段 0–5 已落地。产品契约同步于 [AUTOMATION_PLAN.md](../AUTOMATION_PLAN.md) §2、§5。

## 背景

首版把「归属」和「执行深度」绑在一起：创建自动化必须选一个用户项目，每期固定实例化
Collect→Analyze→Advise 三步模板。对「每天 10 点看一下深圳油价」这类任务，成本远高于收益；
而需要严肃规划的任务又缺少「一句话 → 模型拆 goal → 人工复核 → 入队」的链路。

## 范围

1. **两种任务形态**：`Quick`（一条指令、一个只读步骤）与 `Research`（一次规划、多 goal 复核后冻结）。
2. **统一归属**：新增自动生成的 `Automation` 类型项目，目录为 `<workspace>/automations`，
   不出现在侧栏、项目选择器、研究项目默认值和项目规划入口。所有自动化产物都在 Automations 下。
3. **Research 每期新 topic**：每期在 Automations 项目内新建 topic（`automation_id` 标记），
   命名 `{自动化名} · {当地时间}`；Quick 不建 topic，结果直接挂在期次上。
4. **不重复规划**：计划只在创建（或手动 Replan）时生成一次，冻结为模板；每期只做实例化。
5. **创建界面**：`New automation` 先选类型；Quick 单屏（指令 + 频率，频率仍手动设置）、
   Research 走一次规划与复核；两者都不再要求选择项目。
6. **管理面**：详情页展示类型与已批准计划；Research 提供 `Replan`；改名不升版本，
   其余变更升版本并需重新启用。

## 实现要求

1. `AutomationDefinition` 增加 `shape`，`AutomationTemplate` 改为 `goals: [{title, brief, steps}]`；
   旧版 `instructions + steps` 结构仍可反序列化（单 goal 迁移）。
2. Quick 模板不经过 3–12 步研究计划校验；实例化走单步路径，`complete_occurrence` 支持多 goal。
3. 审批快照只要求模板真正使用的模型；Quick 只要求 Report 模型。
4. 规划请求通过 `ResearchRequest.automation_id` 绑定自动化，`apply_automation_template` 冻结计划；
   自动化请求不出现在普通「Saved research requests」和运行列表中。
5. 每期 topic 与 goal 均带 `automation_id`；发布扫描、侧栏、看板、Runs 列表继续按此过滤。
6. 旧工作区兼容：缺失 `shape` 视为 `Research`；`Quick` 只由新版创建路径写入。

## 验收

- [x] `cargo test --quiet`：Quick 单步校验、单步实例化、每期独立 topic、多 goal 顺序推进、
      没有规划 run、旧模板反序列化、规划请求不建 topic/goal。
- [x] `cargo run --features qa -- --qa-automation`：创建/启用/期次/结果/暂停/恢复/归档链路。
- [x] `--qa-intake`、`--qa-topic-queue`、`--qa-sidebar` 回归通过（项目与 topic 过滤未破坏）。
- [x] 截图：Automations 列表与 Quick 复核页位于 `.local/automation-qa/<pid>/ui/`。
- [x] 明暗主题下成功 / 警告 / 错误提示的填充配色与对比度（`gated-wizard/` 截图）。
- [ ] 真实模型端到端 Research 自动化验收（仍需权限门槛解除后单独执行）。

## 已知限制与后续

- 生产构建的无人值守执行仍由 `cfg!(feature = "qa")` 关闭，本阶段只保证结构与链路。
- 期次 topic 的自动归档策略尚未实现；当前按期次有界加载，不自动删除报告。
- 管理面仍缺版本历史、归档恢复与独立改名入口。
- 交接清单、既有 QA 失败与待确认决策见 [07-backlog.md](07-backlog.md)。
