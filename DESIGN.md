# Task Manager interaction design

## Direction

沿用现有原生工作台布局、系统字体和暖中性色。浅色供日间阅读、深色供暗环境使用，两种主题都必须在静止状态呈现控件边界。默认页面服务非开发者的研究流程，技术配置与诊断按需进入。保留原有配色、按钮可发现性和全部执行能力。

## Components

- Primary：深暖色实底（深色主题采用浅暖色实底），用于创建、生成、保存、执行。一个局部操作组只强调一个推进动作。
- Secondary：始终可见的中性底色和完整边框，用于设置、编辑、复制、关闭或返回。不能默认 ghost。
- Selection：有边界的选择控件，选中状态使用强调色底／字和明确的视觉变化；切换控件同时设置 toggled 语义。
- Link：带下划线的强调色文字，用于跳转、来源和文件打开；正文网址遵守同一规则。设置动作仍用按钮。
- Icon：固定方形触达面积、完整边界、tooltip 和 accessibility_label；不要只放一个裸 × 或 +。
- Task tab：标题与关闭图标共用一个背景和边框，内部无独立按钮边框；关闭图标始终可见，保留独立触达区域、悬停和焦点反馈。长标题按可用宽度省略，悬停可读全名，关闭区域不被压缩。
- Danger：红色描边，用于中止运行或移除材料；普通关闭不使用危险色。
- Disabled：保留形状，弱化填充和文字，业务原因在按钮附近说明；不允许触发操作。
- Focus：保留组件库的焦点绘制、Tab/Enter/Space 语义；不能只用 hover。

## Geometry and typography

动作按钮统一约 32px 高，圆角 6px，系统字体 14px；底部状态栏的 Undo 使用 24px 紧凑按钮。工具栏保持一行，有长标题时截断标题，不压缩按钮。可横向滚动的看板保持现有列宽。表单/详情的操作行可换行。侧栏内容可滚动，主题切换固定在底部，不随项目增多被挤出窗口。

## Boundaries

非交互的 chip、任务状态和字段标签保持低对比、无按钮边框。可点列表/卡片需要悬停反馈和可见的打开箭头或操作，不为整篇正文添加按钮形状。快捷导航保持侧栏结构。

## Source of truth

`appearance.rs` 管理主题与组件色值。`controls.rs` 管理按钮/链接工厂。页面只选择操作语义和绑定行为，禁止重新引入全局 ghost 默认值。原生截图与操作验收覆盖 1440×920、1000×680 和明暗主题。

## Model settings

全局模型入口固定在侧栏底部。全局页集中默认模型、独立搜索的本地模型列表、保存操作与现有覆盖清单。阶段输入留空表示继承，字段旁明确显示有效模型及来源；覆盖清除只有保存后生效。全局页的“所有阶段使用此模型”是显式选项，保存不会启动任务或改变历史。项目规划窗口和任务详情显示同一套有效模型规则。

所有阶段已经继承时，复选框根据实际配置保持勾选，并作为只读状态展示；修改单个阶段覆盖后可重新统一。保存反馈放在主内容顶部的独立区域，不能悬浮遮挡编辑按钮。成功与失败同时使用图标、标题和文字说明，只有本地持久化成功才显示 Saved；反馈可以关闭，切换页面或放弃表单会清理旧提示。

内联提示统一使用语义色调：成功绿、警告橙、错误红、中性信息灰。每个提示是一个带色调表面的圆角框——背景为该色调与面板色的混合，边框和文字使用同一色调；不使用“白底 + 彩色线框”的弱样式。颜色全部来自 `Palette`，深色模式下自动取深色面板与同一语义色，禁止硬编码浅色背景。

## Automations

- 侧栏入口使用时钟图标，位于 Workspace 分组；列表行显示名称、周期、下次触发、最近状态和未读数。名称可换行，长中文不得侵入右侧状态区。
- 创建先选类型（Quick task / Research automation），再填一条指令或一句话需求；不再要求选择项目。Quick 单屏完成，Research 在复核页确认 goals 后启用。三步仍保留：任务 → 周期/时区/三次预览 → 计划与授权摘要。底部操作栏固定可见：Back、Save draft、Continue/Enable automation；保存草稿与启用是两个不同视觉等级的动作。
- 详情先给概览（状态、类型、版本、做什么、预期结果、下三次触发、运行边界），再给操作行（Pause/Resume、Run now、Stop this run、Edit、Replan、Copy、Archive），然后给批准快照、历史。历史结果展开在页面内，报告与来源优先，技术详情折叠。Research 每期一个 topic，历史期次显示其 topic 名称。
- 历史有界加载；未读用点标记；跳过、失败、运行中和已启用是不同维度，不用一个状态覆盖全部。
- 存储失败、无效表单、缺模型/项目/权限时，反馈用独立错误区域和原因文字，不显示成功状态。日志清理必须先预览，并说明保留结果/来源。
- 自动化内部 Task/Topic/Run 不在普通看板、侧栏树、Progress & results 中默认显示；从历史结果可显式打开步骤，面包屑返回 Automations。

## Progressive disclosure

- 日常导航使用 Progress & results；Settings → Agent & models 显示规划、收集、分析和报告使用的模型。
- 执行页优先任务标题、状态、进度与报告。运行历史是紧凑列表，长标题两行显示并可悬停查看全名。运行中的研究协议不作为报告展示。
- Technical details 内保留模型、目录、会话、时间、退出状态、Live logs 和 Saved output。一个选中的活动任务只显示一个停止按钮，批次控制始终在历史列表外可见。
- 任务页默认保留开始/查看结果、状态、优先级、项目和截止日期。More details 收起低频属性和执行设置；缺少模型或工作目录时直接显示修复入口。
- 研究页默认显示目标输入与已有研究；Advanced settings 进入配置页，Back to research 返回，输入保留。直接设置链接必须打开配置页。
- 已完成研究步骤以 View result 为主操作；重新执行使用次按钮。目标结果入口指向有效研究步骤，不把规划 JSON 当作最终报告。

## Icon source and implementation

All interface icons use [Lucide](https://lucide.dev/icons/) via the pinned gpui-kit assets. Follow [Icon Guidelines](doc/ICON_GUIDELINES.md) for semantic choices, embedding, accessibility, exported reports, and verification. Replanned topics use NotebookPen with an explicit Replanned tooltip.
