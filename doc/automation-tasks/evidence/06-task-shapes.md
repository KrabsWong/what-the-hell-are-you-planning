# 阶段 6 验收证据：任务形态与自动化归属

日期：2026-11-05。分支：`feat/automation-shapes`。

## 变更摘要

| 领域 | 变更 |
| --- | --- |
| 形态 | `AutomationShape { Quick, Research }`；缺失字段按 Research 加载 |
| 计划 | `AutomationTemplate { goals: [{title, brief, steps}], window_rule }`；旧 `instructions + steps` 仍可反序列化 |
| 归属 | `ProjectKind { User, Automation }`；`ensure_automation_project` 生成隐藏容器 `<workspace>/automations` |
| 期次 | Research 每期新建 topic（`automation_id` 标记）；Quick 不建 topic |
| 规划 | 创建时一次（或手动 Replan）；`apply_automation_template` 冻结并升版本 |
| 执行 | 多 goal 顺序推进；全部 goal 完成且各有通过的最终报告才算成功 |
| 界面 | 类型选择、Quick 单屏、Research 复核、详情类型/计划/Replan、列表类型标签 |

## 命令与结果

```
$ cargo test --quiet
test result: ok. 280 passed; 0 failed; 2 ignored

$ cargo test --features qa --quiet
test result: ok. 280 passed; 0 failed; 2 ignored

$ cargo check --features qa
Finished (no errors)

$ cargo run --features qa -- --qa-automation
pure automation checks passed
automation planning chain passed (reviewed plan frozen, no topic or replanning)
real queue automation chain passed (fake CLI, 3 steps, isolated sessions, regeneration gated and timed out)
recovery automation chain passed (crash, reopen, session import, continuation, next period)
paused recovery chain passed (user pause survives restart verification)
regeneration recovery chain passed (late result discarded, slot released)
archived recovery chain passed (archive survives restart verification)
step entry routing passed (gate blocks, occurrence path owns runs)
queue chain passed (same-automation A2 waits, A1 completes, then A2)
Automation QA passed · .local/automation-qa/<pid>
```

回归：

```
$ cargo run --features qa -- --qa-intake        -> INTAKE QA PASSED
$ cargo run --features qa -- --qa-topic-queue   -> Topic queue QA passed
$ cargo run --features qa -- --qa-sidebar       -> Sidebar startup QA passed
$ cargo run --features qa -- --qa-settings      -> MODEL SETTINGS QA PASSED
$ python3 script/check_icons.py                 -> All explicit Lucide references are embedded (47)
$ cargo build --release --locked                -> Finished release profile
```

## 新增测试（`cargo test`）

- `quick_task_uses_one_step_and_needs_only_its_own_model`：Quick 只要求 Report 模型；不建 topic；一个 goal、一个 Advise 步骤。
- `research_periods_get_their_own_topic_and_the_plan_is_frozen`：两期两个 topic，均带 `automation_id`，goal 不共用。
- `freezing_a_reviewed_plan_versions_the_automation_without_planning_again`：首次冻结填名称与 goals；Replan 升版本并清审批。
- `research_planning_request_creates_no_topic_and_no_goals`：规划请求不产生 topic/goal；同一自动化复用同一 request。
- `multi_goal_period_runs_goals_in_order_without_replanning`：goal 1 完成后下一 run 属于 goal 2；不存在规划 run。
- `legacy_single_goal_templates_still_load`：旧模板两种历史结构均可加载。
- `automation_project_is_generated_hidden_and_reused`：容器只生成一次、不进用户项目列表、可持久化。

内联提示配色（2026-11-05 追加）：
- 新增共享 `views::notice` / `notice_panel` 与 `NoticeTone { Success, Info, Warning, Danger }`；背景为语义色与面板色的混合，边框和文字使用同一语义色，颜色全部取自 `Palette`，明暗主题自动适配。
- 应用范围：自动化反馈、构建限制、期次跳过/失败原因、规划/校验错误、详情与向导错误；全局保存反馈、任务启动反馈、启动页错误、项目/Topic 对话框错误。
- 新增明暗截图验证成功（绿）、警告（橙）与错误（红）提示：`gated-wizard/gated-step3-saved.png`、`gated-step3-saved-dark.png`、`gated-error-dark.png`；文案与配色规则同步到 `DESIGN.md`。
- 顺带修复 1000×680 下文本域长占位符导致的横向溢出（占位符按内容定宽，改为短句并给字段加 `w_full/min_w_0`）。

详情页反馈与构建限制（2026-11-05 追加修复）：
- 反馈统一渲染在内容顶部（标题下方），不再出现在操作行之后；保存/启用的成功反馈与 toast 二选一，避免同一信息出现三次。
- 构建限制改为概览卡片内的中性「Build limitation」，不再使用 warning 色，也不再与草稿提示重复。

QA 集成用例 `gated_wizard_check`：在生产构建的 `unattended=false` 状态下走完 Quick 三步，
断言最后一步仍有可点击的主操作（`Save draft & finish`）、不再显示不可用的 Enable，
点击后草稿落库并回到详情页；同时覆盖 1000×680 下操作行仍在窗口内。

QA 集成用例 `planning_chain`：保存的 planner 输出经 `complete_research_run` 走真实 finalize 路径，
断言模板冻结为 2 个 goal、名称取自计划、状态回到 Draft、没有新建 topic/goal、也没有第二个规划 run。

## 截图

`.local/automation-qa/<pid>/ui/`：`00-empty-list.png`、`00b-shape-choice.png`、`01-schedule-preview.png`、`02-plan-review.png`、`03-result.png`、`04-detail.png`。
`.local/automation-qa/<pid>/light-1440/list.png`、`dark-1000/` 等主题尺寸组合。

## 未完成 / 已知限制

1. 生产构建的无人值守执行仍由 `cfg!(feature = "qa")` 关闭：可创建、复核、保存自动化，但启用/运行需要权限门槛解除。
2. `--qa-affordance` 与 `--qa-recovery` 在当前工作树存在既有失败，失败点与本次改动无关：
   - affordance：点击 context menu 内的 `approve-topic-1` 报 not visible；改动前的 14:36 运行同样停在 `05-kanban.png`。
   - recovery：`step retry did not finish`。实测是另一 topic 的 Recovering run（session 检查为 Running）按既有 FIFO 规则阻塞了普通重试派发，属于本分支既有队列行为。
3. 期次 topic 尚未自动归档；按期界加载，不自动删除报告。
