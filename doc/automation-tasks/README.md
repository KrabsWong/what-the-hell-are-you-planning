# 定时自动化分阶段任务单

状态：任务形态（阶段 6）已实现，验收证据见 [evidence/06-task-shapes.md](evidence/06-task-shapes.md) 与 [evidence/05-acceptance.md](evidence/05-acceptance.md)。后续迭代与未完成项见 [07-backlog.md](07-backlog.md)。

实施记录：阶段 0–5 的代码与证据已落地；阶段 6 完成 Quick / Research 两种形态、Automations 容器与提示配色统一。权限执行层、真实模型 smoke 和分钟级历史冷保存预算仍有未完成项，详见 [05-acceptance.md](evidence/05-acceptance.md) 与 [07-backlog.md](07-backlog.md)。

产品与技术契约：[AUTOMATION_PLAN.md](../AUTOMATION_PLAN.md)。任务单补充该契约，不能自行扩大范围。先执行一个阶段、检查交付，再进入下一阶段；不要把全部任务一次性交给模型自由实现。

## 阶段入口

| 阶段 | 任务单 | 依赖 | 交付 |
| --- | --- | --- | --- |
| 0 | [执行边界与接入调查](00-feasibility.md) | 无 | 具体接入和权限验证报告，列明阻塞项 |
| 1 | [领域模型与时间计算](01-domain-and-schedule.md) | 0 | 纯模型、状态规则、时间函数与单元测试 |
| 2 | [持久化、调度与恢复](02-runtime-and-storage.md) | 1 | 原子记账、去重、空闲触发、恢复与 fake runner |
| 3 | [研究执行适配](03-research-execution.md) | 2，且 0 的权限门槛通过 | 模板审批、独立运行、时间上下文和报告 |
| 4 | [自动化管理界面](04-management-ui.md) | 3 | 完整管理流程和隔离 UI QA |
| 5 | [集成、性能与交付验收](05-acceptance.md) | 4 | 验收矩阵、回归证据、性能记录与准确文档 |
| 6 | [任务形态与自动化归属](06-task-shapes.md) | 5 | Quick / Research 两种形态、Automations 容器、每期新 topic、不重复规划 |
| — | [后续迭代清单](07-backlog.md) | 6 | 权限门槛、真实模型验收、归档策略与管理面缺口 |

## 可直接复制的派工提示词

替换 `<阶段任务单路径>` 后交给 OpenCode + DeepSeek。具体模型 ID 使用本地已配置的真实 ID，不在项目里硬编码猜测值。

```text
在当前项目实施定时自动化的一个阶段。

先阅读 AGENTS.md、doc/AUTOMATION_PLAN.md、
doc/automation-tasks/README.md 和 <阶段任务单路径>。
如果改变界面，先阅读 PRODUCT.md、DESIGN.md、doc/ICON_GUIDELINES.md。
检查 git status 和相关 diff，识别并保留已有改动。

只完成本阶段列出的范围。先确认前置阶段确实完成；不能把文档里的计划当成已实现能力。
给出简短实现步骤和验证方式，然后实现并运行本阶段要求的检查。
使用假时钟、假 CLI 和隔离工作区，默认不调用真实模型、用户浏览器、外部写接口。
不要修改全局配置、自动提交或推送，也不要重启用户正在运行的应用。
不能用放宽审批/权限、关闭校验、删除失败测试或吞掉错误的方式让检查通过。
如遇到会改变产品契约、数据兼容或权限边界的歧义，报告具体问题再推进依赖它的工作。

交付：改动文件及用途；验收项到测试的映射；实际命令、退出结果和证据路径；
未完成项、已知限制和下一阶段条件。没有运行的检查明确标记未运行。
完成本阶段后停止，不自动进入下一阶段。
```

## 通用工程约束

- 范围外代码只读；不要全仓格式化、升级无关依赖、改动现有工作流语义。
- 当前工作区含用户未提交修改。开始时记录基线，交付时区分自己新增的变化；禁止 reset/checkout 覆盖已有内容。
- 新代码保持现有 Rust/GPUI 风格；只有时间、执行器等真实测试接缝需要抽象，不建设通用框架。
- 调度时间由代码决定，模型只负责被授权的信息工作。数据落盘成功后才能执行。
- 新增依赖必须说明必要性并检查实际文档/API；不要凭记忆编造 OpenCode 参数或时区库接口。
- 不把提示词当权限沙箱；不绕过批准签名、工作区锁、资源冲突和发布隔离。
- 验收测试不能只镜像实现。优先独立预期值、故障注入、公开可见的事件记录和重启后读回结果。
- 非机械代码变更交付前做只读 diff review；检查回归、数据迁移、权限和生成产物。若之后用户要求提交，遵守详细 commit body 和提交前 review；推送必须先扫描完整 outgoing history。
- 无需为了完成阶段而提交 Git commit。后续阶段读取工作区和验收证据，不以「模型声称完成」为依据。

## 检查入口

以下入口在规划时已存在，实施时先复核参数及隔离行为，再运行。阶段按任务单选取相关检查，不要求每次运行全部 QA。

```sh
cargo test --quiet
cargo run --features qa -- --qa-concurrency
cargo run --features qa -- --qa-recovery
cargo run --features qa -- --qa-retries
cargo run --features qa -- --qa-topic-queue
cargo run --features qa -- --qa-batch
cargo run --features qa -- --qa-research
cargo run --features qa -- --qa-publishing
python3 script/check_icons.py
```

`cargo run --features qa -- --qa-automation` 已加入：隔离工作区、假执行器、受限时钟推进，不访问真实用户数据、不调用外部模型。覆盖项见 [evidence/04-management-ui.md](evidence/04-management-ui.md) 与 [evidence/05-acceptance.md](evidence/05-acceptance.md)。另有 `--qa-automation-perf` 生成 43,200 条分钟级夹具并记录性能。

真实 smoke test 仅在明确授权的模型、测试任务和工作区中执行。若尚未授权或条件不具备，其他验收照常完成，最终标明真实链路尚未验证，不伪造通过。

## 阶段交付记录模板

在 `doc/automation-tasks/evidence/` 下为完成阶段建立记录；这次规划不预建假完成报告。大量日志和截图保留在隔离 QA 产物目录，不把凭据、真实研究内容或无关生成物提交到仓库。

```text
阶段 / 日期 / 基线版本与已有修改：
实现文件与用途：
验收项 -> 测试名或操作 -> 证据路径：
实际命令 -> 退出结果：
diff review 发现及处理：
未运行/失败项及原因：
契约变更（没有则写无）：
已知限制 / 下一阶段是否可开始：
```
