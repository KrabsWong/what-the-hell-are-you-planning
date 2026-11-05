# 阶段 4 证据：自动化管理界面

日期：2026-11-05。前置阶段 3。

## 实现文件

| 文件 | 内容 |
| --- | --- |
| `src/automation_views.rs` | 侧栏入口、列表、三步创建/编辑、详情、操作、历史、结果、技术详情、清理预览、截图可测 id |
| `src/views.rs` | `Scope::Automations`、侧栏导航、普通列表过滤、面包屑、状态色 |
| `src/app.rs` | UI 状态、自动化输入订阅、`Scope::Automations` 路由 |
| `src/automation_runtime.rs` | 页面操作对应的暂停/恢复/运行/停止/队列取消，不在 view 重新实现规则 |

## 创建与操作流程

1. 任务与项目：名称、任务描述、预期结果、项目。
2. 周期/时区/预览：间隔/每日/每周、IANA 时区、下三次触发（同一调度函数）；高级设置：允许延迟、总时限、连续失败阈值、日志保留。
3. 计划与授权：固定 3 步模板、依赖、下次触发、有效模型、权限边界、运行条件。
4. “保存草稿”“继续/启用”是不同操作；启用才写入批准快照并计算未来游标。

详情支持：暂停、恢复、立即运行、停止本次、编辑（新版本）、复制草稿、归档、标记已读；每个操作旁有 tooltip 说明对当前/待执行实例的影响。模型/权限/项目缺失或连续失败显示结构化原因与修复入口。

历史按需加载（初始 20 条，每次 +20），区分计划/实际时间、重试/新周期、跳过/失败；报告与来源优先展示，技术信息折叠。日志清理有预览，只清终态实例的详细日志，不删报告、来源、材料、会话标识和去重依据。

## 操作断言（`--qa-automation`）

| 操作 | 断言 |
| --- | --- |
| 空列表 → 新建 | `New automation` 打开三步向导 |
| 填写长中文标题/描述/预期 + 项目 | 进入第 2 步 |
| 周期/时区预览 | 第 2 步显示由同一函数计算的三次触发；第 3 步计划显示 |
| 启用 | 定义 `Enabled`、版本 1、批准有效、游标存在 |
| 强制 fake 到期 | occurrence `Succeeded`、`unread`、fake 启动事件 1 次、结果页面可读 |
| 查看结果并标记已读 | `selected_occurrence` 设置、`unread=false` |
| 编辑 | 版本 2、重新批准、名称更新 |
| 暂停/恢复 | 状态与游标变化；暂停不补历史 |
| 立即运行 | 手动 occurrence 成功，游标不移动 |
| 停止本次 | 活动 fake occurrence 变 `Cancelled`，runner 记录 cancel |
| 复制 | 新 id、`Draft`、无历史 |
| 归档 | `Archived`、从详情返回列表、历史保留 |
| 无效表单 | 空名称不能进入下一步/启用，反馈错误 |
| 存储失败 | 不显示 Saved/Enabled；Store 回滚；错误反馈可见 |
| 调度事务失败 | 到期 tick 遇不可写存储：实例 0、游标不变、假 runner 不启动；恢复后同一点运行 |
| 权限门槛 | 关闭 unattended 能力后详情隐藏 Run now，启用按钮含禁用原因；报告 Regenerate 同样被拒绝；截图 `05-permission-gate.png` |

## 截图

完整交互（1440×920 浅色）：

```text
.local/automation-qa/98061/ui/00-empty-list.png
.local/automation-qa/98061/ui/01-schedule-preview.png
.local/automation-qa/98061/ui/02-plan-review.png
.local/automation-qa/98061/ui/03-result.png        (滚动到结果、来源、时间段和步骤)
.local/automation-qa/98061/ui/04-detail.png
.local/automation-qa/98061/ui/05-permission-gate.png
```

四组主题/尺寸（列表 + 详情）：

```text
.local/automation-qa/98061/light-1440/list.png
.local/automation-qa/98061/light-1440/detail.png
.local/automation-qa/98061/dark-1440/list.png
.local/automation-qa/98061/dark-1440/detail.png
.local/automation-qa/98061/light-1000/list.png
.local/automation-qa/98061/light-1000/detail.png
.local/automation-qa/98061/dark-1000/list.png
.local/automation-qa/98061/dark-1000/detail.png
```

长中文标题在 1000 宽下换行显示，不与右侧返回按钮重叠；列表/详情可滚动；浅深主题对比正常。

## 图标

- 自动化使用 `Clock`（时间/调度语义），新建/运行/暂停/停止/复制/编辑/归档/来源/加载/清理等复用现有 Lucide 图标。
- 图标按钮都有 tooltip 和 accessibility label；`python3 script/check_icons.py` 通过（47 个注册图标全部嵌入）。
- 没有新增图标库或 Unicode/emoji 作为界面图标。

## 实际命令与结果

```text
$ cargo test --quiet
test result: ok. 273 passed; 0 failed; 2 ignored

$ cargo run --features qa -- --qa-automation
pure automation checks passed
real queue automation chain passed (fake CLI, 3 steps, isolated sessions, regeneration gated and timed out)
recovery automation chain passed (crash, reopen, session import, continuation, next period)
paused recovery chain passed (user pause survives restart verification)
regeneration recovery chain passed (late result discarded, slot released)
archived recovery chain passed (archive survives restart verification)
step entry routing passed (gate blocks, occurrence path owns runs)
queue chain passed (same-automation A2 waits, A1 completes, then A2)
Automation QA passed · .local/automation-qa/98061

$ cargo run --features qa -- --qa-settings
MODEL SETTINGS QA PASSED

$ python3 script/check_icons.py
All explicit Lucide references are embedded (47 registered icons).
```

## 未完成 / 限制

- 材料选择器未实现；创建时材料快照为空。
- 键盘 QA 只依赖现有控件语义（Tab/Enter/Space），本次没有新增独立键盘测试；自动化页面复用相同按钮/输入组件。
- 没有真实模型触发的 UI 验收；到期由假 runner 驱动。产品构建的无人值守开关默认关闭，因此 QA 截图中的可运行状态只代表隔离验收构建。
- 基线已有的 `--qa-affordance` 在 `approve-topic-1` 不可见处失败，与自动化无关（基线复现，见阶段 5）。
