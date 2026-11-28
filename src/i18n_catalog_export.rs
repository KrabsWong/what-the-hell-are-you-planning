//! Simplified Chinese translations for native, toast, and export-only copy.

pub(crate) static EXPORT_TRANSLATIONS: &[(&str, &str)] = &[
    (" · Report Something", " · 报告生成器"),
    ("Report Something", "报告生成器"),
    ("Skip to report", "跳到报告"),
    ("Find sections", "查找章节"),
    ("On this page", "本页目录"),
    ("Report contents", "报告目录"),
    ("RESEARCH REPORT", "研究报告"),
    (
        "Diagram preview unavailable. Mermaid source is preserved below.",
        "图表预览不可用，以下保留 Mermaid 源码。",
    ),
    ("Research report diagram", "研究报告图表"),
    ("View Mermaid source", "查看 Mermaid 源码"),
    ("Evidence limitations", "证据限制"),
    ("Report data table", "报告数据表"),
    (
        "Liquid Glass is unavailable while Reduce Transparency is enabled in macOS Accessibility settings.",
        "macOS 辅助功能设置中启用“减弱透明度”时，Liquid Glass 不可用。",
    ),
    (
        "Planning Glass Preview - isolated sample",
        "Planning Glass 预览 - 独立示例",
    ),
    ("Choose workspace", "选择工作区"),
    ("Add PDF or images", "添加 PDF 或图片"),
    ("Sources", "来源"),
    ("terminated", "已终止"),
    ("research", "研究"),
    ("Exported", "已导出"),
    ("Documents", "文档"),
    ("References", "参考资料"),
    (
        "No separately recorded web sources. See citations in the document above.",
        "没有单独记录的网页来源。请参阅上文文档中的引文。",
    ),
    (
        "- {}\n  - URL: <{}>\n  - Published: {}\n",
        "- {}\n  - 网址：<{}>\n  - 发布时间：{}\n",
    ),
    (
        "  - Run: {} · Started: {} · Finished: {}\n",
        "  - 运行：{} · 开始：{} · 完成：{}\n",
    ),
    ("unknown", "未知"),
    ("Uploaded resources", "已上传资源"),
    ("resource", "资源"),
    (
        "Each document contains the complete saved Markdown result and its recorded references. Uploaded original files are included in resources when present. Failed attempts, technical logs and other working-directory files are excluded.",
        "每份文档都包含完整保存的 Markdown 结果及其记录来源。已上传的原始文件会包含在资源中（如存在）。失败的尝试、技术日志和其他工作目录文件不会包含。",
    ),
    (
        "{}\nPermanently delete {} tasks, {} planning requests and {} runs, including saved results and local publication snapshots. This cannot be undone. Exported files, original materials and remote pages are not removed.{}",
        "{}\n将永久删除 {} 个任务、{} 个规划请求和 {} 次运行，包括已保存结果和本地发布快照。此操作无法撤销。导出文件、原始资料和远程页面不会被删除。{}",
    ),
    (
        "{name}\n{} running, queued or interrupted attempts will be stopped. Pending steps will not start. Saved results will be retained.",
        "{name}\n将停止 {} 个正在运行、排队或中断的尝试。待处理步骤不会启动。已保存结果会保留。",
    ),
    (
        "Use an HTTPS or SSH github.com repository URL without credentials.",
        "请使用不含凭据的 HTTPS 或 SSH github.com 仓库地址。",
    ),
    (
        "Enter a GitHub repository URL containing owner/repository.",
        "请输入包含 owner/repository 的 GitHub 仓库地址。",
    ),
    (
        "Enter a branch name, such as main.",
        "请输入分支名称，例如 main。",
    ),
    (
        "There are no report files to publish.",
        "没有可发布的报告文件。",
    ),
    (
        "Could not create the report publishing folder.",
        "无法创建报告发布文件夹。",
    ),
    (
        "Could not prepare the report checkout.",
        "无法准备报告检出。",
    ),
    (
        "The target branch does not exist. Create it in GitHub first to preserve the existing site.",
        "目标分支不存在。请先在 GitHub 上创建，以保留现有站点。",
    ),
    ("Could not create report directory.", "无法创建报告目录。"),
    ("Could not write report assets.", "无法写入报告资源。"),
    (
        "Could not create site entry files.",
        "无法创建站点入口文件。",
    ),
    ("Cannot read edgeone.json.", "无法读取 edgeone.json。"),
    (
        "Invalid edgeone.json; configure the report deployment before publishing.",
        "edgeone.json 无效；发布前请先配置报告部署。",
    ),
    (
        "edgeone.json must be an object.",
        "edgeone.json 必须是对象。",
    ),
    (
        "Cannot read build-reports.mjs.",
        "无法读取 build-reports.mjs。",
    ),
    (
        "Existing build-reports.mjs differs from the report publisher. Review it before retrying.",
        "现有 build-reports.mjs 与报告发布器不一致。请先检查，然后重试。",
    ),
    ("Cannot enumerate report directories.", "无法枚举报告目录。"),
    ("Cannot read report directory.", "无法读取报告目录。"),
    (
        "Cannot read existing report HTML.",
        "无法读取现有报告 HTML。",
    ),
    ("Untitled report", "未命名报告"),
    ("Cannot read README.md.", "无法读取 README.md。"),
    (
        "README report index markers are malformed; repair them before publishing.",
        "README 报告索引标记格式错误；发布前请修复。",
    ),
    (
        "Cannot write EdgeOne deployment configuration.",
        "无法写入 EdgeOne 部署配置。",
    ),
    (
        "Cannot write public report build script.",
        "无法写入公开报告构建脚本。",
    ),
    ("Cannot write report index.", "无法写入报告索引。"),
    ("Empty report asset path.", "报告资源路径为空。"),
    ("Unsafe report asset path.", "报告资源路径不安全。"),
    (
        "Hidden or unsafe report asset path.",
        "报告资源路径为隐藏路径或不安全路径。",
    ),
    (
        "Report destination contains a symbolic link.",
        "报告目标包含符号链接。",
    ),
    (
        "Could not determine outgoing object size.",
        "无法确定传出对象大小。",
    ),
    (
        "An outgoing file is too large to scan safely.",
        "传出文件过大，无法安全扫描。",
    ),
    (
        "Git is unavailable. Install Git and configure GitHub authentication.",
        "Git 不可用。请安装 Git 并配置 GitHub 身份验证。",
    ),
    (
        "Git operation timed out. Check network and GitHub authentication.",
        "Git 操作超时。请检查网络和 GitHub 身份验证。",
    ),
    ("Could not read Git output.", "无法读取 Git 输出。"),
    (
        "Complete the research goal \"{}\" before publishing. Current status: {}.",
        "发布前请完成研究目标“{}”。当前状态：{}。",
    ),
    (
        "The research goal \"{}\" has no final report step.",
        "研究目标“{}”没有最终报告步骤。",
    ),
    (
        "The final report \"{}\" does not have a current saved result. Wait for its rerun to finish, or rerun the report after its inputs change.",
        "最终报告“{}”没有当前保存的结果。请等待其重新运行完成，或在输入变化后重新运行报告。",
    ),
    (
        "Generate and approve a research plan for \"{goal_title}\" before publishing.",
        "发布前请为“{goal_title}”生成并批准研究计划。",
    ),
    (
        "Wait for the research step \"{}\" to finish before publishing. Current status: {}.",
        "发布前请等待研究步骤“{}”完成。当前状态：{}。",
    ),
    (
        "The research step \"{}\" does not have a current saved result. Rerun it after its inputs change, then publish.",
        "研究步骤“{}”没有当前保存的结果。请在输入变化后重新运行，然后发布。",
    ),
    (
        "The final report \"{}\" does not have a current saved result.",
        "最终报告“{}”没有当前保存的结果。",
    ),
    ("Online report: {}", "在线报告：{}"),
    ("Automation complete", "自动化已完成"),
    (
        "<div class=\"diagram-fallback\"><p>Diagram preview unavailable. Mermaid source is preserved below.</p>",
        "<div class=\"diagram-fallback\"><p>图表预览不可用，以下保留 Mermaid 源码。</p>",
    ),
    ("Diagram exceeds rendering limits", "图表超出渲染限制"),
    ("Diagram exceeds image limits", "图表超出图片限制"),
    (
        "A material changed while it was being verified. Nothing was attached.",
        "资料在验证期间发生了变化。未添加任何内容。",
    ),
    (
        "A newer planning attempt or revision is current. This saved plan was kept as history and was not applied.",
        "当前已有更新的规划尝试或版本。此已保存计划已保留为历史记录，但未应用。",
    ),
    (
        "A project plan must contain 1–8 independent research goals.",
        "项目计划必须包含 1–8 个独立研究目标。",
    ),
    (
        "A proposed title duplicates an existing task in this project. Retry with distinct goals.",
        "建议标题与本项目中的现有任务重复。请使用不同的目标重试。",
    ),
    (
        "A retained task references the reviewed sample tasks; nothing was removed.",
        "保留的任务引用了已审核的示例任务；未删除任何内容。",
    ),
    (
        "A task cannot be its own ancestor.",
        "任务不能成为自己的祖先。",
    ),
    (
        "Adding materials locally; progress is shown in Materials.",
        "正在本地添加资料；进度显示在“资料”中。",
    ),
    (
        "All related tasks must remain archived before deletion.",
        "删除前，所有相关任务必须保持归档状态。",
    ),
    (
        "Approve the complete research goal in Todo before running its steps.",
        "运行步骤前，请先在 Todo 中批准完整研究目标。",
    ),
    (
        "Approve this research topic by moving it to Todo first.",
        "请先将其移至 Todo，以批准此研究主题。",
    ),
    (
        "Automation period queued · waiting for an automation slot",
        "自动化时段已排队 · 等待自动化名额",
    ),
    (
        "Automation recovery is paused because the stop-intent file is malformed: {error}",
        "停止意图文件格式错误，自动化恢复已暂停：{error}",
    ),
    (
        "Automation-owned history cannot be deleted from Research Archive.",
        "无法从研究归档中删除自动化所有的历史记录。",
    ),
    (
        "Backup verification failed; original workspace was not changed",
        "备份验证失败；原始工作区未更改",
    ),
    (
        "Cannot save workspace to {}: {error}",
        "无法将工作区保存到 {}：{error}",
    ),
    (
        "Changes are only in memory. Workspace storage is unavailable.",
        "更改仅在内存中。工作区存储不可用。",
    ),
    (
        "Check the previous session in Paused projects before starting again.",
        "再次启动前，请在“已暂停的项目”中检查先前的会话。",
    ),
    (
        "Check this task’s previous session in Paused projects before starting again.",
        "再次启动前，请在“已暂停的项目”中检查此任务的先前会话。",
    ),
    ("Checking material before undo…", "撤销前正在检查资料…"),
    (
        "Choose a material that is still attached to this task.",
        "请选择仍附加到此任务的资料。",
    ),
    (
        "Choose a project before generating a plan.",
        "生成计划前请先选择项目。",
    ),
    ("Choose a project first.", "请先选择项目。"),
    (
        "Choose an active parent in the same project.",
        "请在同一个项目中选择活动的父级。",
    ),
    (
        "Choose an active task for these materials; restore this research first.",
        "请为这些资料选择活动任务；请先恢复此研究。",
    ),
    (
        "Choose an agent and model for Plan in Settings.",
        "请在设置中为“规划”选择代理和模型。",
    ),
    (
        "Complete goal plan approved · click Run goal when ready",
        "完整目标计划已批准 · 准备就绪后点击“运行目标”",
    ),
    (
        "Complete this goal's research steps and final report before marking it Done.",
        "标记完成前，请先完成此目标的研究步骤和最终报告。",
    ),
    (
        "Could not open the report export dialog.",
        "无法打开报告导出对话框。",
    ),
    (
        "Could not preserve research dependencies during migration.",
        "迁移期间无法保留研究依赖关系。",
    ),
    (
        "Delete the archived goal to remove its research steps together.",
        "删除已归档目标会一并移除其研究步骤。",
    ),
    (
        "Dispatch admission budget is full; the intent was retained and no run was queued.",
        "调度准入预算已满；意图已保留，但未排队运行。",
    ),
    (
        "Enter a project name between 1 and 160 characters.",
        "请输入 1–160 个字符的项目名称。",
    ),
    ("Enter a task title.", "请输入任务标题。"),
    (
        "Enter an existing absolute folder path.",
        "请输入现有的绝对文件夹路径。",
    ),
    (
        "File import stopped unexpectedly. Please try adding the files again.",
        "文件导入意外停止。请重新添加文件。",
    ),
    (
        "Fix local storage before adding files.",
        "添加文件前请先修复本地存储。",
    ),
    ("Goal \"{goal_title}\": {}", "目标“{goal_title}”：{}"),
    (
        "Goal title \"{goal_title}\": {}",
        "目标标题“{goal_title}”：{}",
    ),
    (
        "Goal {} needs a title of up to 160 characters and a non-empty brief of up to 12,000 bytes.",
        "目标 {} 需要一个不超过 160 个字符的标题和一个不超过 12,000 字节的非空简介。",
    ),
    (
        "Incremental persistence is unavailable. Nothing was written to a legacy file; retry the save when the worker is attached.",
        "增量持久化不可用。未写入旧版文件；工作线程连接后请重试保存。",
    ),
    ("Invalid relative workspace path", "工作区相对路径无效"),
    ("Invalid signature separator", "签名分隔符无效"),
    ("Invalid workspace path", "工作区路径无效"),
    (
        "Local storage must be available before starting a real run.",
        "启动真实运行前，本地存储必须可用。",
    ),
    (
        "Material cleanup finished after the workspace changed; copies remain tracked for their original workspace.",
        "工作区变化后资料清理已完成；副本仍按其原始工作区跟踪。",
    ),
    (
        "Material cleanup is still in progress. Try again shortly.",
        "资料清理仍在进行中。请稍后重试。",
    ),
    (
        "Material cleanup ownership is unknown; copies were retained.",
        "无法确定资料清理归属；副本已保留。",
    ),
    (
        "Material cleanup returned an incomplete result; copies were retained.",
        "资料清理返回了不完整的结果；副本已保留。",
    ),
    (
        "Material cleanup was paused because the workspace changed; copies were retained.",
        "工作区变化导致资料清理暂停；副本已保留。",
    ),
    ("Material undo did not run.", "资料撤销未执行。"),
    (
        "Material verification is still in progress. Try again shortly.",
        "资料验证仍在进行中。请稍后重试。",
    ),
    (
        "Material verification returned an incomplete result; copies were retained.",
        "资料验证返回了不完整的结果；副本已保留。",
    ),
    ("Missing signature field", "缺少签名字段"),
    (
        "Move the goal to Todo before continuing.",
        "继续前请将目标移至 Todo。",
    ),
    (
        "Move the research goal to move its complete task chain between projects.",
        "移动研究目标即可在项目之间移动其完整任务链。",
    ),
    (
        "No ready steps. Check results and dependencies.",
        "没有就绪步骤。请检查结果和依赖关系。",
    ),
    (
        "Only archived research can be permanently deleted.",
        "只能永久删除已归档的研究。",
    ),
    (
        "Only archived research topics can be permanently deleted.",
        "只能永久删除已归档的研究主题。",
    ),
    (
        "Open the task before editing it.",
        "请先打开任务，再进行编辑。",
    ),
    (
        "Other research still depends on this archived research.",
        "其他研究仍依赖此已归档研究。",
    ),
    (
        "Path upgrade changed workspace data; the transaction was rolled back",
        "路径升级更改了工作区数据；事务已回滚",
    ),
    ("Persistence reattached", "持久化已重新连接"),
    (
        "Persistence unavailable · changes are not being saved",
        "持久化不可用 · 更改未保存",
    ),
    (
        "Plan format incomplete · repairing once before review",
        "计划格式不完整 · 审核前自动修复一次",
    ),
    (
        "Planning failed. The draft is unchanged; retry planning against the same request.",
        "规划失败。草稿未更改；请针对同一请求重试规划。",
    ),
    (
        "Planning failed. Open the draft and retry planning.",
        "规划失败。请打开草稿并重试规划。",
    ),
    (
        "Preview unavailable: {error}\n\n{}",
        "预览不可用：{error}\n\n{}",
    ),
    (
        "Project context changed during planning. Review the output and generate again.",
        "规划期间项目上下文发生了变化。请查看输出并重新生成。",
    ),
    (
        "Project context is too large. Shorten its scope or existing task titles before planning.",
        "项目上下文过大。规划前请缩短范围或现有任务标题。",
    ),
    (
        "Project plan rejected · review output and retry",
        "项目计划被拒绝 · 请查看输出并重试",
    ),
    (
        "Provide a nonempty script of at most 256,000 bytes.",
        "请提供不超过 256,000 字节的非空脚本。",
    ),
    (
        "Queued behind the previous pending change. This retry will start when that change is saved or discarded.",
        "已排在前一个待处理更改之后。该更改保存或丢弃后，此重试将开始。",
    ),
    (
        "Related work is still running or awaiting recovery. Check its session status before deleting.",
        "相关工作仍在运行或等待恢复。删除前请检查其会话状态。",
    ),
    (
        "Research destination changed during planning. Review the saved request and generate again.",
        "规划期间研究目标位置发生了变化。请查看已保存请求并重新生成。",
    ),
    (
        "Research inputs changed during execution. Review the saved output and run again.",
        "执行期间研究输入发生了变化。请查看已保存输出并重新运行。",
    ),
    (
        "Research inputs changed · rerun required",
        "研究输入已变化 · 需要重新运行",
    ),
    ("Research is running", "研究正在运行"),
    (
        "Research paused · open the result for the required input",
        "研究已暂停 · 请打开结果以查看所需输入",
    ),
    (
        "Research reports require status complete/partial/blocked and non-empty report text.",
        "研究报告必须使用 complete/partial/blocked 状态并提供非空报告正文。",
    ),
    (
        "Research result rejected · review output and retry",
        "研究结果被拒绝 · 请查看输出并重试",
    ),
    (
        "Research tasks keep their goal as parent so materials and dependencies stay together.",
        "研究任务保留其目标作为父级，以便资料和依赖关系保持在同一个目标下。",
    ),
    (
        "Research understanding ready · confirm it before generating goals",
        "研究理解已就绪 · 生成目标前请先确认",
    ),
    (
        "Resolve the pending material change before adding more files.",
        "添加更多文件前，请先处理待处理的资料更改。",
    ),
    (
        "Resolve the pending material change before removing another material.",
        "移除其他资料前，请先处理待处理的资料更改。",
    ),
    (
        "Restore the archived task before changing its stage.",
        "更改阶段前请先恢复已归档任务。",
    ),
    ("Restore the archived task first.", "请先恢复已归档任务。"),
    (
        "Restore this task before moving it.",
        "移动前请先恢复此任务。",
    ),
    (
        "Review this goal's complete plan and move it to Todo before executing.",
        "执行前请审核此目标的完整计划，并将其移至 Todo。",
    ),
    (
        "Run context is too large. Reduce the research scope or prior reports.",
        "运行上下文过大。请缩小研究范围或减少先前报告。",
    ),
    (
        "Runtime workspace paths must be absolute",
        "运行时工作区路径必须为绝对路径",
    ),
    ("SQLite integrity check failed", "SQLite 完整性检查失败"),
    (
        "SQLite migration verification failed",
        "SQLite 迁移验证失败",
    ),
    (
        "SQLite workspace metadata is missing",
        "SQLite 工作区元数据缺失",
    ),
    ("Saved to local workspace", "已保存到本地工作区"),
    ("Saved · local storage recovered", "已保存 · 本地存储已恢复"),
    ("Saving comment…", "正在保存评论…"),
    ("Saving request…", "正在保存请求…"),
    ("Saving research understanding…", "正在保存研究理解…"),
    ("Saving task…", "正在保存任务…"),
    ("Saving undo…", "正在保存撤销…"),
    ("Saving…", "正在保存…"),
    (
        "Set an existing working folder for this project before generating goals.",
        "生成目标前，请为此项目设置现有的工作文件夹。",
    ),
    (
        "Set the path to your OpenCode executable.",
        "请设置 OpenCode 可执行文件路径。",
    ),
    (
        "Set this project's working folder before running.",
        "运行前请设置此项目的工作文件夹。",
    ),
    (
        "Set up a workspace before creating automations.",
        "创建自动化前请先设置工作区。",
    ),
    (
        "Set up a workspace before creating projects.",
        "创建项目前请先设置工作区。",
    ),
    (
        "Set up your workspace before running tasks.",
        "运行任务前请先设置工作区。",
    ),
    ("Set up your workspace first.", "请先设置工作区。"),
    (
        "Some material copies could not be cleaned up and were retained.",
        "部分资料副本无法清理，已保留。",
    ),
    (
        "Some material copies could not be verified and were retained for retry.",
        "部分资料副本无法验证，已保留以便重试。",
    ),
    (
        "Step \"{step_title}\": instructions must be 1–12,000 bytes.",
        "步骤“{step_title}”：指令必须为 1–12,000 字节。",
    ),
    (
        "Step title \"{step_title}\": {}",
        "步骤标题“{step_title}”：{}",
    ),
    (
        "Stop intents use newer version {}; the file was left unchanged.",
        "停止意图使用较新版本 {}；文件未更改。",
    ),
    (
        "Stop or finish submitted work before undoing changes.",
        "撤销更改前，请先停止或完成已提交的工作。",
    ),
    (
        "Storage unavailable · changes are not saved",
        "存储不可用 · 更改未保存",
    ),
    ("Submitted · {}", "已提交 · {}"),
    (
        "Task queued · waiting for a research execution slot",
        "任务已排队 · 等待研究执行名额",
    ),
    (
        "Task {id} contains real or unrecognized data; it was not removed.",
        "任务 {id} 包含真实或无法识别的数据；未删除。",
    ),
    (
        "That page is no longer available. Returned to a safe parent.",
        "该页面已不可用。已返回安全上级页面。",
    ),
    (
        "That project is no longer available. Choose another destination.",
        "该项目已不可用。请选择其他目标位置。",
    ),
    (
        "That task is no longer available. Returned to a safe parent.",
        "该任务已不可用。已返回安全上级页面。",
    ),
    (
        "The archived research changed. Review the deletion again.",
        "已归档研究发生变化。请重新检查删除操作。",
    ),
    (
        "The archived research shares a planning request with other research.",
        "此已归档研究与其他研究共享一个规划请求。",
    ),
    (
        "The edited task changed. Reopen its editor before saving.",
        "编辑中的任务已发生变化。保存前请重新打开编辑器。",
    ),
    (
        "The impact changed · review the updated effect before posting",
        "影响已变化 · 发布前请查看更新后的效果",
    ),
    (
        "The impact changed · review the updated effect before saving",
        "影响已变化 · 保存前请查看更新后的效果",
    ),
    ("The material could not be verified.", "资料无法验证。"),
    (
        "The material file changed or is missing. Undo did not run.",
        "资料文件已变化或缺失。撤销未执行。",
    ),
    (
        "The material list changed while it was being verified. Nothing was removed.",
        "验证期间资料列表发生变化。未删除任何内容。",
    ),
    (
        "The next goal of this period is not runnable yet.",
        "此时段的下一个目标尚不可运行。",
    ),
    (
        "The planning request for this response no longer exists. The output was kept as history and was not applied.",
        "此响应对应的规划请求已不存在。输出已保留为历史记录，但未应用。",
    ),
    (
        "The prepared run ownership does not match its dispatch intent.",
        "准备好的运行归属与其调度意图不匹配。",
    ),
    (
        "The previous live save is still retained by the persistence worker.",
        "持久化工作线程仍保留上一次实时保存。",
    ),
    (
        "The project plan contains duplicate research goals. Retry with distinct questions.",
        "项目计划包含重复的研究目标。请使用不同的问题重试。",
    ),
    (
        "The project plan is too large. Ask for up to 8 focused research goals.",
        "项目计划过大。请请求最多 8 个聚焦的研究目标。",
    ),
    (
        "The queued goal is no longer available in this project.",
        "排队目标在此项目中已不可用。",
    ),
    (
        "The save could not be completed: {reason}. Review and retry.",
        "无法完成保存：{reason}。请检查后重试。",
    ),
    (
        "The selected material changed while it was being verified.",
        "所选资料在验证期间发生了变化。",
    ),
    (
        "The workspace changed before materials were verified. Nothing was attached.",
        "验证资料前工作区发生了变化。未添加任何内容。",
    ),
    (
        "The workspace changed while materials were being verified. Nothing was attached.",
        "验证资料期间工作区发生了变化。未添加任何内容。",
    ),
    (
        "The workspace changed while this material change was pending. Return to the original workspace to finish it.",
        "资料更改待处理期间工作区发生了变化。请返回原工作区完成该操作。",
    ),
    (
        "This period already has a running or queued step.",
        "此时段已有正在运行或排队中的步骤。",
    ),
    (
        "This project already has research proposals. Edit the existing cards instead.",
        "此项目已有研究提案。请改为编辑现有卡片。",
    ),
    (
        "This project already has research proposals. Review and edit its existing goals in the list.",
        "此项目已有研究提案。请在列表中查看并编辑现有目标。",
    ),
    (
        "This property requires the task editor.",
        "此属性需要使用任务编辑器。",
    ),
    (
        "This retained attempt has no reader text to copy.",
        "此保留尝试没有可复制的阅读文本。",
    ),
    (
        "This run exceeds 24 files or 50 MiB of materials. Remove unneeded task/parent attachments before retrying.",
        "此次运行超过 24 个文件或 50 MiB 资料。重试前请移除不需要的任务/父级附件。",
    ),
    (
        "This task is already running or queued. Open View progress to follow it.",
        "此任务已在运行或排队中。请打开“查看进度”以跟踪。",
    ),
    ("Undoing…", "正在撤销…"),
    ("Unknown browser tool.", "未知浏览器工具。"),
    (
        "Unsupported automation schema version {} (expected {}). The workspace was left unchanged.",
        "不支持自动化架构版本 {}（应为 {}）。工作区未更改。",
    ),
    (
        "Unsupported workspace version in {} (expected {VERSION}, found {}). The file was left unchanged.",
        "{} 中的工作区版本不受支持（应为 {VERSION}，实际为 {}）。文件未更改。",
    ),
    (
        "Use a project name of up to 160 characters and scope of up to 12,000 bytes.",
        "请使用不超过 160 个字符的项目名称和不超过 12,000 字节的范围。",
    ),
    (
        "Use a valid date in YYYY-MM-DD format.",
        "请使用 YYYY-MM-DD 格式的有效日期。",
    ),
    (
        "Use an IANA time zone such as Asia/Shanghai, not a fixed offset.",
        "请使用 IANA 时区（如 Asia/Shanghai），而不是固定偏移。",
    ),
    ("Verifying material before removal…", "移除前正在验证资料…"),
    ("Verifying materials…", "正在验证资料…"),
    (
        "Wait for materials to finish importing before running.",
        "运行前请等待资料导入完成。",
    ),
    (
        "Wait for the current files to finish importing.",
        "请等待当前文件导入完成。",
    ),
    (
        "Wait for the current material import to finish.",
        "请等待当前资料导入完成。",
    ),
    ("Workspace data is missing", "工作区数据缺失"),
    (
        "Workspace path must not contain parent traversal",
        "工作区路径不得包含上级目录遍历",
    ),
    (
        "Workspace storage is unavailable. Undo did not run.",
        "工作区存储不可用。撤销未执行。",
    ),
    (
        "Workspace storage is unavailable; material cleanup was deferred.",
        "工作区存储不可用；资料清理已推迟。",
    ),
    (
        "Workspace storage root must be absolute",
        "工作区存储根目录必须为绝对路径",
    ),
    ("[stream error] {e}", "[流错误] {e}"),
    (
        "a group must contain a change or metadata delta",
        "组必须包含更改或元数据增量",
    ),
    (
        "a previous live save group is retained (epoch {pending_epoch})",
        "已保留先前的实时保存组（纪元 {pending_epoch}）",
    ),
    (
        "ambiguous planning attempt belongs to the Automations list",
        "不明确的规划尝试属于自动化列表",
    ),
    (
        "an automation task needs an explicit period owner",
        "自动化任务需要显式指定时段归属",
    ),
    (
        "automation planning is T23-owned history",
        "自动化规划是 T23 所有的历史记录",
    ),
    (
        "automation run cannot use a goal result owner",
        "自动化运行不能使用目标结果作为归属",
    ),
    (
        "automation run cannot use a task-only result owner",
        "自动化运行不能使用仅任务结果作为归属",
    ),
    (
        "automation run cannot use the ordinary results page",
        "自动化运行不能使用普通结果页面",
    ),
    (
        "cannot install v4 database: {error}",
        "无法安装 v4 数据库：{error}",
    ),
    (
        "cannot preserve old database sidecar: {error}",
        "无法保留旧数据库附属文件：{error}",
    ),
    (
        "cannot read dispatch intent {}: {error}",
        "无法读取调度意图 {}：{error}",
    ),
    ("cannot read {}: {error}", "无法读取 {}：{error}"),
    (
        "cannot remove dispatch intent {}: {error}",
        "无法移除调度意图 {}：{error}",
    ),
    (
        "cannot remove live recovery: {error}",
        "无法移除实时恢复数据：{error}",
    ),
    (
        "clean load requires a fresh empty store with no tracked changes",
        "干净加载需要一个没有跟踪更改的全新空存储",
    ),
    (
        "dispatch intent budget exceeded at startup: {bytes} > {max_bytes} bytes; the file was left intact",
        "启动时调度意图预算超限：{bytes} > {max_bytes} 字节；文件保持不变",
    ),
    (
        "dispatch intent byte budget exceeded: {next} > {} bytes",
        "调度意图字节预算超限：{next} > {} 字节",
    ),
    (
        "dispatch intent {} is not awaiting acknowledgement",
        "调度意图 {} 未等待确认",
    ),
    (
        "duplicate automation id {} before migration",
        "迁移前发现重复的自动化 ID {}",
    ),
    (
        "duplicate occurrence id {} before migration",
        "迁移前发现重复的时段 ID {}",
    ),
    (
        "duplicate project id {} before migration",
        "迁移前发现重复的项目 ID {}",
    ),
    (
        "duplicate run id {} before migration",
        "迁移前发现重复的运行 ID {}",
    ),
    (
        "duplicate task id {} before migration",
        "迁移前发现重复的任务 ID {}",
    ),
    (
        "duplicate trigger key {} for automation {} before migration",
        "迁移前自动化 {} 存在重复的触发键 {}",
    ),
    (
        "durable admission accepts DispatchRun, DispatchRequestPlan and DispatchRegeneration commands",
        "持久化准入接受 DispatchRun、DispatchRequestPlan 和 DispatchRegeneration 命令",
    ),
    (
        "durable intent storage is not configured",
        "未配置持久化意图存储",
    ),
    (
        "ego-browser did not return its complete skill guide. Update ego(lite) and finish onboarding.",
        "ego-browser 未返回完整的技能指南。请更新 ego(lite) 并完成首次设置。",
    ),
    (
        "ego-browser is not installed at this path. Install ego(lite), complete its first launch, then select ~/.local/bin/ego-browser.",
        "此路径未安装 ego-browser。请安装 ego(lite)、完成首次启动，然后选择 ~/.local/bin/ego-browser。",
    ),
    ("injected after-commit uncertainty", "注入提交后不确定性"),
    ("injected before-commit failure", "注入提交前失败"),
    (
        "injected failure after checkpoint/sidecar handling",
        "注入检查点/附属文件处理后的失败",
    ),
    (
        "injected failure after verified backup",
        "注入已验证备份后的失败",
    ),
    (
        "injected failure after verified v4 temporary write",
        "注入已验证 v4 临时写入后的失败",
    ),
    ("injected failure at generation {}", "在生成 {} 时注入失败"),
    ("injected mid-group failure", "注入组中失败"),
    (
        "installed v4 database did not round-trip the original state",
        "已安装的 v4 数据库未能往返保留原始状态",
    ),
    ("invalid zero incarnation in {}", "{} 中的零纪元无效"),
    ("live persistence is not ready", "实时持久化尚未就绪"),
    (
        "live persistence worker is closed",
        "实时持久化工作线程已关闭",
    ),
    ("live save group is empty", "实时保存组为空"),
    (
        "loaded data contains duplicate tracked entity identity {key:?}",
        "加载的数据包含重复的跟踪实体标识 {key:?}",
    ),
    (
        "migration backup does not match the original logical state",
        "迁移备份与原始逻辑状态不匹配",
    ),
    (
        "migration backup failed its integrity check",
        "迁移备份未通过完整性检查",
    ),
    (
        "named period run is not automation-owned",
        "命名时段运行不属于自动化",
    ),
    ("not ready", "尚未就绪"),
    ("only submit requests are sent here", "此处仅发送提交请求"),
    (
        "period base detail names another automation",
        "时段基础详情指向另一个自动化",
    ),
    ("period does not belong to automation", "时段不属于该自动化"),
    ("persistence worker disconnected", "持久化工作线程已断开"),
    (
        "persistence worker is not ready for resolution status",
        "持久化工作线程尚未准备好写入处理结果状态",
    ),
    (
        "planning attempt belongs to another automation",
        "规划尝试属于另一个自动化",
    ),
    (
        "previous version belongs to another task",
        "先前版本属于另一个任务",
    ),
    ("refusing to overwrite existing {}", "拒绝覆盖现有 {}"),
    (
        "results history limit must be positive",
        "结果历史限制必须为正数",
    ),
    ("run belongs to another task", "运行属于另一个任务"),
    (
        "run does not belong to the named goal",
        "运行不属于指定目标",
    ),
    (
        "run does not belong to the named period",
        "运行不属于指定时段",
    ),
    (
        "run is not the named request planning attempt",
        "运行不是指定的请求规划尝试",
    ),
    (
        "schema version {version} must first use the existing v1/v2 path-upgrade route; no file was changed",
        "架构版本 {version} 必须先使用现有 v1/v2 路径升级路线；未更改任何文件",
    ),
    (
        "seeded v4 database did not round-trip its store",
        "播种的 v4 数据库未能往返保留其存储",
    ),
    ("session is still running", "会话仍在运行"),
    (
        "source WAL checkpoint is busy (busy={busy}); no replacement was attempted",
        "源 WAL 检查点繁忙（busy={busy}）；未尝试替换",
    ),
    ("task is not in the named period", "任务不在指定时段中"),
    ("task is not owned by the automation", "任务不属于该自动化"),
    (
        "task/period identity no longer matches",
        "任务/时段标识不再匹配",
    ),
    (
        "the planning run belongs to an older request revision",
        "规划运行属于较旧的请求版本",
    ),
    (
        "the planning run belongs to another request",
        "规划运行属于另一个请求",
    ),
    (
        "the staged occurrence is missing the dispatch run link",
        "暂存的时段缺少调度运行链接",
    ),
    (
        "the staged planning request is missing its run link",
        "暂存的规划请求缺少运行链接",
    ),
    (
        "unsupported SQLite schema version {version} (expected {SCHEMA_VERSION})",
        "不支持的 SQLite 架构版本 {version}（应为 {SCHEMA_VERSION}）",
    ),
    (
        "unsupported future SQLite schema version {version}; the file was left unchanged",
        "不支持未来 SQLite 架构版本 {version}；文件未更改",
    ),
    (
        "v3 backup logical state differs from the original",
        "v3 备份的逻辑状态与原始状态不同",
    ),
    (
        "v4 backup logical state differs from the source",
        "v4 备份的逻辑状态与源不同",
    ),
    (
        "workspace database does not exist: {}",
        "工作区数据库不存在：{}",
    ),
    (
        "workspace incarnation mismatch: database {stored}, expected {incarnation}",
        "工作区纪元不匹配：数据库为 {stored}，应为 {incarnation}",
    ),
    (
        "workspace integrity check failed: {result}",
        "工作区完整性检查失败：{result}",
    ),
    (
        "workspace root must be absolute, got {}",
        "工作区根目录必须为绝对路径，实际为 {}",
    ),
    ("workspace v4 schema is incomplete", "工作区 v4 架构不完整"),
    (
        "{elapsed}s · materials copied · verifying before attachment",
        "已用时 {elapsed} 秒 · 资料已复制 · 附加前正在验证",
    ),
    (
        "{} research goals in Backlog · open the project to review and run",
        "待办池中有 {} 个研究目标 · 打开项目以审核并运行",
    ),
    (
        "{} research running · {} automation running · {} queued",
        "{} 个研究运行中 · {} 个自动化运行中 · {} 个排队中",
    ),
];
