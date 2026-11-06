# 自动化后续迭代清单

更新：2026-11-05。分支：`feat/automation-shapes`。用途：接手时先读本文件，再读
[AUTOMATION_PLAN.md](../AUTOMATION_PLAN.md) 与 [06-task-shapes.md](06-task-shapes.md)。

## 当前已交付

- 两种任务形态：`Quick`（一条指令、一个只读步骤）与 `Research`（一次规划、复核后冻结模板）。
- 所有自动化产物归属自动生成的 **Automations** 容器（隐藏 `ProjectKind::Automation`，
  目录 `<workspace>/automations`），不进入手动项目、Topic、看板、Runs 列表或发布扫描。
- Research 每期在 Automations 下新建 topic（`automation_id` 标记）；Quick 不建 topic。
- 计划只生成一次；期次只实例化模板。Research 支持显式 `Replan`（新版本 + 重新批准）。
- 管理面：暂停/恢复/立即运行/停止/编辑/复制/归档、期次历史、日志清理、类型与计划摘要。
- 语义化内联提示（成功绿 / 警告橙 / 错误红 / 信息灰，明暗主题自适应）。

验证命令与证据见 [evidence/06-task-shapes.md](evidence/06-task-shapes.md)。

## P0 · 交付前必须完成

1. **解除无人值守权限门槛**
   - 现状：`TaskApp::automation_unattended_enabled = cfg!(feature = "qa")`，`automation_runtime`
     在门禁关闭时把期次转入 `Verifying`。生产构建只能创建/复核草稿。
   - 待办：按 `AUTOMATION_PLAN.md §7` 验证执行层工具拒绝，形成证据；再切换生产默认值。
   - 验收：release 构建里 Quick 与 Research 各完成一次真实期次，且写入被拒绝、浏览器不可接管。

2. **真实模型端到端验收**
   - Quick：一次真实运行，结果与来源正确落在期次。
   - Research：一句话 → 规划 → 复核 → 启用 → 两期；断言第二期不产生规划 run、topic 不同。
   - 记录模型、耗时、token（若可计量）与失败恢复路径。

3. **期次保留策略**
   - 每期新 topic 会持续增长。实现“默认 90 天归档旧期次 topic、不删除报告”的设置与预览。
   - 列表已按期有界加载；归档需要与 `Scope::Automations` 的筛选联动。

## P1 · 体验缺口

4. **管理面粒度**
   - “只改频率/时区”：跳过计划复核，直接到 Schedule 确认（仍升版本、需重新启用）。
   - 版本历史：展示 version、批准时间与变更摘要，而不只是 `Version N`。
   - 归档恢复：`DefinitionStatus::Archived` 目前是终态；补 Restore 与“显示已归档”筛选。
   - 显式改名入口：保存路径已支持 rename-only，但界面没有独立入口。

5. **Research 模板编辑**
   - 当前只能整份 Replan。可考虑在复核页局部编辑 goal/step 文本（仍生成新版本）。
   - Replan 时展示新旧计划差异，避免用户盲目批准。

6. **Automations 总览**
   - 跨自动化的“最近结果”列表（未读优先），而不是逐个点进详情。
   - 搜索、排序与自定义分组（`AutomationGroup` 实体）留作扩展；当前只按类型 chip + 单列表。

## P2 · 可靠性与测试

7. **崩溃窗口测试**
   - “事务提交但未启动”“结果返回但未记账”“启动意图已保存但进程未知”三类窗口，
     目前契约存在但缺少显式测试。参考 `automation_runtime::reconcile_occurrence`。

8. **既有 QA 失败（非本功能引入，需单独处理）**
   - `--qa-affordance`：context menu 内 `approve-topic-1` 报 not visible；14:36 的旧运行同样停在
     `05-kanban.png`，与本功能无关。
   - `--qa-recovery`：`step retry did not finish`。原因是另一 topic 的 Recovering run（session 检查
     为 Running）按既有 FIFO 规则阻塞普通重试派发。
   - `--qa-topics`：`Ordinary subtasks retain manual completion` 失败，错误为
     “Cannot move Todo → Done. Follow the workflow order…”，属于工作流顺序校验的既有改动。
   - 处理时不要为了让测试通过而放宽审批/权限或删除断言。

9. **视觉回归**
   - 提示框配色已有明暗截图（`evidence/06`），但没有断言；后续可把关键颜色写入渲染测试。
   - 1000×680 下已修复文本域长占位符导致的横向溢出，仍需覆盖更多长文案。

## 待确认的产品决策

- 权限门槛解除后，Quick 是否允许“保存即启用”，还是保持显式启用。
- 期次 topic 的默认归档时长（现建议 90 天）与是否需要手动清理入口。
- 是否需要自定义分组；若要，按 `AutomationScope` 的扩展点新增 `Group(id)` 与配置界面。
- Quick 与 Research 是否都允许挂到项目下（当前统一 Unfiled / Automations 容器）。

## 接手步骤

```bash
git switch feat/automation-shapes
cargo test --quiet
cargo run --features qa -- --qa-automation
```

代码入口：
- 领域与模板：`src/automation.rs`
- 创建/实例化/审批：`src/automation_execution.rs`
- 期次调度与恢复：`src/automation_runtime.rs`
- 管理界面：`src/automation_views.rs`
- 共享提示组件：`src/views.rs`（`NoticeTone` / `notice` / `notice_panel`）
