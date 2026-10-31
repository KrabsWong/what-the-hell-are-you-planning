# 本地 OpenCode 接入

当前实现已经接通 OpenCode v2.2.0 CLI。协议以本机 `opencode run --help` 为准；官方资料见 [CLI](https://opencode.ai/docs/cli/)。不依赖常驻 HTTP 服务。

## 执行链路

```mermaid
flowchart LR
  UI[Run with OpenCode] --> Snapshot[读取任务阶段 / 模型 / 指令 / 项目目录]
  Snapshot --> Save[先持久化运行快照]
  Save --> Worker[独立工作线程]
  Worker --> CLI[OpenCode standalone run]
  CLI --> JSON[JSON 事件 + stderr]
  JSON --> Queue[有界事件队列]
  Queue --> GPUI[80ms 消费事件 / 实时日志]
  GPUI --> Disk[合并保存 / 结束立即落盘]
```

实际调用等价于：

```text
opencode --standalone run --format json --model provider/model --agent build --title "task / stage" -- "完整请求"
```

Rust `Command` 分别传递参数，不经过 shell。工作目录和 `PWD` 同时设置为已规范化的项目目录；请求中也包含该目录。每次调用使用独立会话，没有隐式 `--continue`。下一阶段依靠项目文件和任务描述交接上下文，不自动把全部历史日志拼入提示词。

模型发现调用 `--version` 和 `models --standalone`，在后台线程执行并设 30 秒超时。选择列表支持筛选，仍可手工填写自定义模型标识。不同项目可能有不同的 OpenCode 配置；刷新前应选择对应项目。

## 状态与生命周期

`AgentRun` 保存 run ID、task ID/标题、阶段、agent、model、prompt、目录、开始/结束时间、session ID、退出码和日志。更新阶段策略不会改变历史快照。

运行状态包括 Running、Cancelling、Succeeded、Failed、Cancelled、Interrupted、Blocked、Stale；旧 Simulated 历史兼容读取，产品不再创建模拟运行。CLI 非零退出、顶层 error 事件或读流失败会记为 Failed。工具内部单次失败不一定表示整个 Agent 失败，因为 Agent 可能自行恢复。

Unix 子进程通过独立 process group 启动。取消立即通知该组，worker 回收主进程；正常结束清理仍持有管道的后代，读流收尾有时间上限。退出应用等待 worker 回收，应用尚未消费的日志/Session/Finished 事件仍会写回记录。强杀应用不提供后台作业恢复保证。

实际执行之前必须能保存运行快照。正在运行时禁止撤销和归档该任务；撤销其他内存修改时保留运行历史和单调任务 ID，不回滚文件操作。队列按 Topic 串行推进，当前 Topic 内最多并发 3 个资源独立且依赖就绪的任务。

## Topic 执行队列

`TodoBatch` 支持运行或暂停期间继续追加已批准目标，按 Topic 首次提交顺序分组。同一 Topic 的目标全部结束后才派发下一 Topic；同组可按依赖并发（最多 3 个），共享 session／目录／浏览器仍互斥。失败步骤先走自动重试，耗尽后记录在 failed_tasks／failed_items，当前 Topic 其他可运行分支结束后继续下一 Topic。重新提交已失败 Topic 加入队尾，不影响已完成结果。暂停不会重新激活历史失败项；停止清空整队，关闭应用仍不自动恢复执行。

`--qa-topic-queue` 使用临时数据库和本地假 CLI 验证满额追加、同 Topic 多 Goal、去重、顺序、重试耗尽、后续推进以及暂停／停止。

## 存储

### 任务失败自动重试

任务执行或结果校验得到 `Failed` 后，按原输入快照和会话排队，等待 30 秒再尝试。每次用户发起的执行链最多自动重试 3 次（加首次共 4 次）；次数与等待截止时间保存在运行记录中。重试队列复用并发上限、共享资源互斥、派发前审批及输入新鲜度校验，等待不占用执行名额。批量执行将原运行关联转交给重试，成功后继续推进，耗尽后按失败处理。

主动取消、归档、`Blocked`、`Interrupted`、`Stale` 不自动重试；项目／请求规划保留原有单次格式修复逻辑，不叠加任务重试。暂停批次会暂停重试派发，Stop task／Stop all 可取消等待，退出应用将排队工作标记为中断，重启不自动执行。用户重新手动启动会开启新的重试额度。界面显示倒计时、Retry N/3 和耗尽提示。

`Storage` 采用 `{ "version": 3, "workspace": ... }`（可读版本 1／2，在保存时迁移；旧二进制拒绝版本 3，以保留人工准入约束） JSON。新增字段使用 serde 默认值；未知版本和损坏内容明确拒绝。保存使用同目录临时文件、flush/fsync、rename；同目录 `workspace.lock` 的独占锁持有到应用退出。

所有被 UI 通知的 Store 修改由统一观察者保存。执行日志按约 500ms 合并，退出和终态强制保存。加载时将遗留 Running/Cancelling 标为 Interrupted，保留已收到的输出，不重新执行。

## 后续 Codex 接入

保留 `StagePolicy.agent` 和 `AgentRun.agent`。新增 Codex Adapter 时复用请求快照、生命周期事件与历史 UI，替换命令创建、协议解析和取消实现即可。不要在 `Store::set_stage` 内启动进程；阶段自动路由、交互批准、跨阶段会话复用和并行工作目录隔离应作为独立阶段实现。


## 研究执行路径

`ResearchTask.kind` 有 Goal / Collect / Analyze / Advise；`ResearchConfig.models` 分别保存四种角色的模型，不复用开发任务的 coding stage 指令。Goal 运行由模型返回 JSON 计划，校验数量、标题／指令长度、前向依赖和必要角色；全部合法才创建子任务，已有计划不覆盖。研究子任务保留固定 goal 归属。

下游请求包含父目标、当前任务描述／评论、直接依赖的已验证正文与结构化来源、当前和父级附件。通过输入签名和依赖 run ID 递归校验新鲜度，签名不含执行产生的活动和任务状态。运行期间输入改变会得到 Stale；退出路径同样运行校验，绝不自动续跑。

运行结果正文来自 OpenCode text 事件（保留最后一个完整文本 part；之前的说明仍在日志中）。研究非规划角色必须返回 `{status, report, sources}`，状态为 complete 或 blocked；报表解析成功才标记步骤 Done。`ResearchConfig` 的 Browser 路径只在 Collect 角色加入请求；OpenCode 读取本机 skill 后控制独立 Space。默认不修改 OpenCode 的全局权限，不添加 --auto；被权限阻止应显式报告 blocked。

`Run remaining` 在每个终态保存后调用同一个 next-ready-task 逻辑。失败、blocked、stale、取消、保存失败或没有可运行依赖时停止。连续意图仅在内存中，重启不会继续。新任务每次都重新取最新材料，已运行的请求保持快照。

## 可选 Longbridge MCP 查询

采集、分析、报告阶段的提示词包含 Longbridge MCP 发现指引；纯规划阶段不查询。需要股票、公司、行情、估值或行业信息时，OpenCode 检查当前会话暴露的服务与工具；必要时在运行目录执行 `opencode mcp list` 确认配置和连接状态，再根据实际工具描述与参数选择只读公共信息查询。复用已有充分证据，不固定工具名称或假设市场覆盖范围。

复用 OpenCode 自身的 MCP 配置，不安装服务、不修改凭证或权限。服务不可用或工具不适合时回退到浏览器及已有材料；仅当所有可用来源仍无法提供必要证据时阻塞研究。提示词禁止账户、持仓、订单、交易、资金和其他写操作；这是代理行为约束，实际工具权限仍由 OpenCode／MCP 服务配置决定。

报告记录实际查询工具、非敏感参数、标的／市场、观测与检索时间、币种／单位和数据局限，并向下游传递。工具未返回链接时在正文中注明查询来源，不伪造 URL。现有网页来源契约不变，采集完成仍至少需要一个实际访问的可核验网页来源；MCP 作为补充证据使用。

## 文件和浏览器边界

附件复制到数据目录 `attachments/`，使用唯一文件名、create_new 和 Unix 0600。默认目录 0700；类型检查为扩展名与 magic，不等于完整文档解码。路径解析限制在受管目录，拒绝越界与目录符号链接。移除引用不删除物理文件，运行快照／撤销不会引用到被自动清理的文件。

底层适配器保留 `--file` 能力；产品材料链路改为后台 PDFKit/Vision 提取、缓存指纹校验，再把带原文件名和页码的文本加入 prompt，不向文本模型传递 PDF／图片二进制。父目标与依赖任务的材料均传入下游，并要求 complete 报告引用每份材料名称。浏览器 Node 环境可能不继承 shell cwd／环境，prompt 要求使用绝对文件路径。

2026-10-28 已完成真实只读研究验收：读取安装版 skill guide，在同一个 ego Space 内实际导航并保存页面截图与 PDF，自动执行搜集 → 分析 → 简报，跨阶段切换模型、传递结果与来源，并验证持久化恢复。最终 Space 已正常 finish。真实模型出现“说明文字 + 单个 JSON 代码块”输出，解析器现在接受这种包装但仍严格校验 JSON、状态与来源；多块或不完整输出仍拒绝。首次解析失败记录保留，重试未重复搜集。真实接管场景尚未单独验证；补充材料通过本地 PDF／OCR 文本转录进入模型，原生读取已单独验证。浏览器源码示例会随版本变化，运行时以 `ego-browser skill` 为准。官方参考：[ego-browser](https://lite.ego.app/document/en/docs/ego-browser)、[当前 Skill](https://github.com/citrolabs/ego-lite/blob/main/skills/ego-browser/SKILL.md)。

## 项目候选题目规划

`project_planning.rs` 单独定义项目规划契约 `{tasks:[{title,brief}]}`，与研究阶段计划 `{title,instructions,kind,depends_on}` 分开。项目名称、可选范围、目录和现有题目快照构成输入签名。返回值先完整校验数量／长度／重复，随后才创建 Backlog 中 `requires_approval=true` 的研究 Goal。规划提示词只要求拆题，不要求联网或研究；本地 OpenCode 权限边界保持原样。

项目运行使用 `AgentRun.project_plan=Some(project_id)`；兼容旧序列化的 `task_id` 在这种情况下为 0，不作为真实任务使用。旧任务运行的 `project_plan` 默认为 None。两类运行复用 `launch_run`、Request/Event、日志、取消、异常退出恢复和启动前保存。项目运行历史跳转到项目看板，不查找虚构任务。

`requires_approval` 只用于项目生成的父 Goal，默认为 false 以兼容旧研究。Backlog 表示待批准；进入 Todo 批准；未批准／已取消题目不可直接跳到执行或完成状态。子任务通过 goal_id 继承准入约束；准备请求、计划应用、下一任务选择、结果输入签名均检查父目标。退回 Backlog 后当前外部进程不会立刻取消，但该时段完成的结果失效，后续链停止。没有引入新的调度器或状态变更自动调用模型。

候选题目在创建时复制项目范围，防止后续修改项目说明隐式改变已批准任务。一次成功规划不重复应用，保留用户编辑；运行中项目输入改变则 Stale，失败或取消不落卡片。
