//! Converts an approved automation snapshot into ordinary research steps.
//!
//! Each occurrence receives a new internal Topic, Goal, working directory, session
//! and browser Space. Normal research approval, dependency and result validation are
//! reused unchanged; automation approval is a separate snapshot.
use crate::{
    automation::{
        AutomationApproval, AutomationDefinition, AutomationGoal, AutomationMaterial,
        AutomationOccurrence, AutomationShape, AutomationTemplate,
        AutomationTemplateStep, DefinitionStatus, WindowRule,
    },
    automation::{TimeWindow},
    automation_schedule::{previous_local_day, Schedule, ScheduledTime},
    domain::{AgentRun, Stage, Store},
    research::ResearchKind,
};


/// Deterministic, reviewable template. The user's objective is not re-planned each
/// period; the app only substitutes the occurrence time window at dispatch.
/// Only unambiguously expressed ranges may be resolved by the scheduler. Other
/// requests keep an unresolved window for the collection step to verify.
pub fn infer_window_rule(text: &str) -> crate::automation::WindowRule {
    let lower = text.to_lowercase();
    let has_yesterday = ["昨天", "昨日", "yesterday"]
        .iter()
        .any(|needle| lower.contains(needle));
    if !has_yesterday {
        return crate::automation::WindowRule::Unresolved;
    }
    // Mixed or comparative requests need more than one window and must not be
    // narrowed to a single previous day.
    const OTHER_RANGES: &[&str] = &[
        "今天", "今日", "明天", "前天", "上周", "上星期", "本周", "这周", "上月", "上个月",
        "去年", "前年", "前一周", "上一周", "上一个月", "上一年", "前一个月", "前一年",
        "上一个交易日", "交易日", "季度", "同比", "环比", "对比", "比较",
        "trading day", "today", "tomorrow", "last week", "this week", "last month",
        "last year", "quarter", "year-over-year", "month-over-month", "compare", "versus",
        " vs ", "过去", "最近", "minutes", "分钟", "hours",
    ];
    if OTHER_RANGES.iter().any(|needle| lower.contains(needle)) {
        return crate::automation::WindowRule::Unresolved;
    }
    // Negation changes the requirement rather than selecting yesterday.
    if lower.contains("not yesterday")
        || lower.contains("isn't yesterday")
        || lower.contains("is not yesterday")
        || lower.contains("不是昨天")
        || lower.contains("并非昨天")
        || lower.contains("不只看昨天")
        || lower.contains("不要只看昨天")
    {
        return crate::automation::WindowRule::Unresolved;
    }
    for token in ["昨天", "昨日"] {
        if let Some(position) = lower.find(token) {
            if lower[..position]
                .chars()
                .rev()
                .take(6)
                .any(|character| matches!(character, '不' | '非' | '未' | '别' | '除'))
            {
                return crate::automation::WindowRule::Unresolved;
            }
        }
    }
    crate::automation::WindowRule::PreviousLocalDay
}

/// Research-shaped template used by QA fixtures and by drafts saved before plans
/// were generated from the planner. New research automations use `template_from_plan`.
#[allow(dead_code)]
pub fn build_template(objective: &str, expected: &str) -> Result<AutomationTemplate, String> {
    let (objective, expected) = validated_instruction(objective, expected)?;
    let goal = AutomationGoal {
        title: objective.chars().take(80).collect(),
        brief: format!(
            "Recurring task: {objective}\n\nExpected result: {expected}\n\nUse only the time window supplied with this occurrence. Do not send messages, publish, trade, delete files, or change external systems."
        ),
        steps: vec![
            AutomationTemplateStep {
                title: "Collect current evidence".into(),
                instructions: format!(
                    "Collect dated, verifiable evidence for: {objective}\n\nRecord original links, publication and observation times, retrieval time, relevant excerpts, and explicit gaps. Follow the occurrence time window; do not substitute today for it."
                ),
                kind: ResearchKind::Collect,
                depends_on: vec![],
            },
            AutomationTemplateStep {
                title: "Analyze against the task".into(),
                instructions: format!(
                    "Analyze the collected evidence for: {objective}\n\nSeparate observed facts from inference, carry forward evidence gaps, and explain what the evidence does and does not support. Expected result: {expected}"
                ),
                kind: ResearchKind::Analyze,
                depends_on: vec![0],
            },
            AutomationTemplateStep {
                title: "Write the recurring report".into(),
                instructions: format!(
                    "Write the requested result: {expected}\n\nOpen with the answer, keep citations beside material claims, and disclose missing evidence. Do not publish or send it anywhere."
                ),
                kind: ResearchKind::Advise,
                depends_on: vec![1],
            },
        ],
    };
    let template = AutomationTemplate::single_goal(
        goal,
        infer_window_rule(&format!("{objective}\n{expected}")),
    );
    template.validate(AutomationShape::Research)?;
    Ok(template)
}

/// A quick task is a single instruction plus a single read-only answer step. It
/// needs no planning, no goal tree and no separate report pass.
pub fn build_quick_template(objective: &str, expected: &str) -> Result<AutomationTemplate, String> {
    let (objective, expected) = validated_instruction(objective, expected)?;
    let template = AutomationTemplate::single_goal(
        AutomationGoal {
            title: objective.chars().take(80).collect(),
            brief: format!(
                "Recurring task: {objective}\n\nExpected result: {expected}\n\nAnswer directly with dated facts and sources. Do not send messages, publish, trade, delete files, or change external systems."
            ),
            steps: vec![AutomationTemplateStep {
                title: "Answer the recurring question".into(),
                instructions: format!(
                    "Answer this recurring question using current, verifiable sources: {objective}\n\nFollow the occurrence time window; never substitute today for it. Record original links, observation or publication times, the retrieval time, and any evidence gap. Separate observed facts from inference. Expected result: {expected}"
                ),
                kind: ResearchKind::Advise,
                depends_on: vec![],
            }],
        },
        infer_window_rule(&format!("{objective}\n{expected}")),
    );
    template.validate(AutomationShape::Quick)?;
    Ok(template)
}

/// Freeze a reviewed request plan into a repeatable template. Planning happens once;
/// every period instantiates this template instead of planning again.
pub fn template_from_plan(
    plan: &crate::request_planning::RequestPlan,
) -> Result<AutomationTemplate, String> {
    let goals = plan
        .goals
        .iter()
        .map(|goal| {
            let steps: Vec<AutomationTemplateStep> = goal
                .tasks
                .iter()
                .map(|task| AutomationTemplateStep {
                    title: task.title.clone(),
                    instructions: task.instructions.clone(),
                    kind: task.kind,
                    depends_on: task.depends_on.clone(),
                })
                .collect();
            AutomationGoal {
                title: goal.title.clone(),
                brief: goal.brief.clone(),
                steps,
            }
        })
        .collect::<Vec<_>>();
    let template = AutomationTemplate {
        goals,
        window_rule: WindowRule::Unresolved,
    };
    template.validate(AutomationShape::Research)?;
    Ok(template)
}

fn validated_instruction<'a>(
    objective: &'a str,
    expected: &'a str,
) -> Result<(&'a str, &'a str), String> {
    let objective = objective.trim();
    let expected = expected.trim();
    if objective.is_empty() || objective.len() > 12_000 {
        return Err("Describe what the automation should do in 1–12,000 bytes.".into());
    }
    if expected.is_empty() || expected.len() > 12_000 {
        return Err("Describe the expected result in 1–12,000 bytes.".into());
    }
    if let Some(reason) = unsupported_reason(objective) {
        return Err(reason);
    }
    Ok((objective, expected))
}

/// Actions the first version cannot perform unattended must fail at creation,
/// not silently become broader permissions.
pub fn unsupported_reason(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    const BLOCKED: &[(&str, &str)] = &[
        ("send email", "sending email"),
        ("发送邮件", "sending email"),
        ("发邮件", "sending email"),
        ("send message", "sending messages"),
        ("发送消息", "sending messages"),
        ("place order", "placing orders"),
        ("automatic trading", "automated trading"),
        ("自动交易", "automated trading"),
        ("下单", "placing orders"),
        ("自动买入", "automated trading"),
        ("自动卖出", "automated trading"),
        ("git push", "pushing to Git"),
        ("push to github", "pushing to Git"),
        ("自动发布", "publishing externally"),
        ("publish report to", "publishing externally"),
        ("删除文件", "deleting files"),
        ("rm -rf", "shell deletion"),
        ("run script", "running arbitrary scripts"),
        ("执行脚本", "running arbitrary scripts"),
    ];
    BLOCKED
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, action)| {
            format!(
                "This task asks for {action}, which the first automation version cannot perform. Remove that action and keep the task to information collection, analysis and a local report."
            )
        })
}

pub fn approval_for(
    store: &Store,
    definition: &AutomationDefinition,
    now: i64,
    materials: Vec<AutomationMaterial>,
) -> Result<AutomationApproval, String> {
    definition.validate_draft()?;
    let template = definition
        .template
        .as_ref()
        .ok_or("Generate or edit the recurring plan before enabling.")?;
    template.validate(definition.shape)?;
    let mut models = std::array::from_fn(|_| String::new());
    let mut agents: [Option<String>; 4] = Default::default();
    let required = template.kinds();
    for kind in ResearchKind::ALL {
        let index = kind.index();
        // Planning never runs inside a period, so its model is snapshotted for
        // completeness only. Every step the template really executes must be
        // configured for the unattended agent.
        if kind == ResearchKind::Goal || !required.contains(&kind) {
            models[index] = store.research_model(kind).to_string();
            agents[index] = Some(store.research_agent(kind).to_string());
            continue;
        }
        let model = store.research_model(kind).to_string();
        if model.trim().is_empty() || !store.valid_research_model(kind) {
            return Err(format!(
                "Choose an available model for the {} step in Settings before enabling.",
                kind.label()
            ));
        }
        models[index] = model;
        // Freeze the effective agent explicitly; `None` must not mean "follow the
        // future global default", which would silently change an enabled automation.
        agents[index] = Some(store.research_agent(kind).to_string());
    }
    Ok(AutomationApproval {
        version: definition.version,
        approved_at: now,
        template_signature: template.signature(),
        schedule_signature: definition.schedule.signature(),
        models,
        agents,
        materials,
    })
}

impl Store {
    /// Freeze a reviewed planner result into the automation that requested it.
    /// The first apply fills an empty draft; a later apply is a new version and
    /// requires explicit re-approval. Periods never plan again.
    pub fn apply_automation_template(
        &mut self,
        automation: u64,
        text: &str,
        now: i64,
    ) -> Result<usize, String> {
        let plan = crate::request_planning::parse_request_plan(text)?;
        let template = template_from_plan(&plan)?;
        let version = {
            let definition = self
                .automation(automation)
                .ok_or("Automation not found.")?
                .clone();
            let mut staged = definition;
            let revised = staged.template.is_some();
            if revised {
                staged.version = staged
                    .version
                    .checked_add(1)
                    .ok_or("No automation versions remain.")?;
            }
            // The planner names the research subject; a quick task never reaches here.
            staged.name = plan.topic_name.chars().take(160).collect();
            staged.template = Some(template);
            staged.approval = None;
            staged.status = DefinitionStatus::Draft;
            staged.cursor = None;
            staged.anchor = None;
            staged.attention_reason = None;
            staged.updated_at = now;
            staged.validate_draft()?;
            let version = staged.version;
            *self.automation_mut(automation).unwrap() = staged;
            version
        };
        if version > 1 {
            for occurrence in self
                .automation_occurrences
                .iter_mut()
                .filter(|occurrence| {
                    occurrence.automation_id == automation
                        && occurrence.status == crate::automation::OccurrenceStatus::Queued
                })
            {
                let _ = occurrence.transition(
                    crate::automation::OccurrenceStatus::Skipped,
                    Some(crate::automation::OccurrenceReason::new(
                        crate::automation::SkipCode::DefinitionChanged,
                        "The automation plan was revised; this request used an older version.",
                    )),
                );
            }
        }
        let goals = self
            .automation(automation)
            .and_then(|definition| definition.template.as_ref())
            .map(|template| template.goals.len())
            .unwrap_or(0);
        Ok(goals)
    }

    pub fn create_automation_draft(
        &mut self,
        project: u64,
        shape: AutomationShape,
        name: String,
        objective: String,
        expected_result: String,
        schedule: Schedule,
        policy: crate::automation::AutomationPolicy,
        template: Option<AutomationTemplate>,
        now: i64,
    ) -> Result<u64, String> {
        if !self.projects.iter().any(|p| p.id == project) {
            return Err("Choose an existing project.".into());
        }
        let id = self.next_automation_id()?;
        let definition = AutomationDefinition {
            id,
            version: 1,
            project_id: project,
            shape,
            name: name.trim().into(),
            objective: objective.trim().into(),
            expected_result: expected_result.trim().into(),
            schedule,
            status: DefinitionStatus::Draft,
            template,
            approval: None,
            policy,
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: now,
            updated_at: now,
        };
        definition.validate_draft()?;
        self.automations.push(definition);
        Ok(id)
    }

    /// Clear detailed logs for terminal occurrences older than the configured retention.
    /// Reports, sources, materials, sessions and dedupe records are never deleted.
    pub fn automation_log_cleanup(
        &mut self,
        automation: u64,
        now: i64,
        apply: bool,
    ) -> Result<(usize, usize, usize), String> {
        let definition = self.automation(automation).ok_or("Automation not found.")?;
        let before = now - i64::from(definition.policy.log_retention_days) * 86_400;
        let occurrence_ids: Vec<u64> = self
            .automation_occurrences
            .iter()
            .filter(|o| o.automation_id == automation && o.created_at < before && o.status.is_terminal())
            .map(|o| o.id)
            .collect();
        let mut occurrences = 0;
        let mut bytes = 0;
        let mut cleared = 0;
        for occurrence in &occurrence_ids {
            let mut touched = false;
            for run in self.runs.iter_mut().filter(|r| {
                r.automation_occurrence_id == Some(*occurrence)
            }) {
                if !run.logs.is_empty() {
                    bytes += run.logs.iter().map(String::len).sum::<usize>();
                    if apply {
                        run.logs.clear();
                        run.logs.push(
                            "[log] Detailed log cleared by the automation retention setting. The report, sources and session identity were kept."
                                .into(),
                        );
                        cleared += 1;
                    }
                    touched = true;
                }
            }
            if touched {
                occurrences += 1;
            }
        }
        Ok((occurrences, bytes, cleared))
    }

    pub fn revise_automation(
        &mut self,
        id: u64,
        project: u64,
        name: String,
        objective: String,
        expected_result: String,
        schedule: Schedule,
        policy: crate::automation::AutomationPolicy,
        template: Option<AutomationTemplate>,
        now: i64,
    ) -> Result<(), String> {
        let definition = self
            .automation(id)
            .ok_or("Automation not found.")?
            .clone();
        if definition.status == DefinitionStatus::Archived {
            return Err("Restore the archived automation before editing it.".into());
        }
        let mut staged = definition;
        if !self.projects.iter().any(|candidate| candidate.id == project) {
            return Err("Choose an existing project.".into());
        }
        staged.project_id = project;
        staged.name = name.trim().into();
        staged.objective = objective.trim().into();
        staged.expected_result = expected_result.trim().into();
        staged.schedule = schedule;
        staged.policy = policy;
        staged.template = template;
        staged.version = staged
            .version
            .checked_add(1)
            .ok_or("No automation versions remain.")?;
        staged.status = DefinitionStatus::Draft;
        staged.approval = None;
        staged.cursor = None;
        staged.anchor = None;
        staged.attention_reason = None;
        staged.updated_at = now;
        staged.validate_draft()?;
        *self.automation_mut(id).unwrap() = staged;
        // Every unstarted occurrence belongs to the old approved version.
        for occurrence in self
            .automation_occurrences
            .iter_mut()
            .filter(|o| {
                o.automation_id == id
                    && o.status == crate::automation::OccurrenceStatus::Queued
            })
        {
            let _ = occurrence.transition(
                crate::automation::OccurrenceStatus::Skipped,
                Some(crate::automation::OccurrenceReason::new(
                    crate::automation::SkipCode::DefinitionChanged,
                    "The automation was edited; this request was approved under an older version.",
                )),
            );
        }
        Ok(())
    }

    /// A successfully completed recovery clears only the verification block. User
    /// pauses and other attention reasons remain untouched.
    pub fn resume_after_verified_recovery(&mut self, id: u64, now: i64) -> bool {
        let Some(definition) = self.automation(id) else {
            return false;
        };
        if definition.status != DefinitionStatus::NeedsAttention {
            return false;
        }
        let reason = definition
            .attention_reason
            .clone()
            .unwrap_or_default()
            .to_lowercase();
        if !(reason.contains("session")
            || reason.contains("verification")
            || reason.contains("confirmed")
            || reason.contains("核验"))
        {
            return false;
        }
        self.resume_automation(id, now).is_ok()
    }

    /// A display-name change does not alter the approved plan, models, materials or schedule.
    pub fn rename_automation(&mut self, id: u64, name: &str, now: i64) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 160 {
            return Err("Automation name must be 1–160 characters.".into());
        }
        let definition = self.automation_mut(id).ok_or("Automation not found.")?;
        definition.name = name.into();
        definition.updated_at = now;
        Ok(())
    }


    /// Enable only with an explicit approved snapshot. The cursor starts strictly
    /// after this instant; enabling never backfills history.
    pub fn approve_and_enable(
        &mut self,
        id: u64,
        materials: Vec<AutomationMaterial>,
        now: i64,
    ) -> Result<AutomationApproval, String> {
        let definition = self.automation(id).ok_or("Automation not found.")?.clone();
        if definition.status == DefinitionStatus::Archived {
            return Err("Restore the archived automation before enabling it.".into());
        }
        if !self.projects.iter().any(|p| p.id == definition.project_id) {
            return Err("Choose an existing project.".into());
        }
        let approval = approval_for(self, &definition, now, materials)?;
        let first = definition.schedule.first_after(now)?;
        let definition = self.automation_mut(id).unwrap();
        definition.approval = Some(approval.clone());
        definition.status = DefinitionStatus::Enabled;
        definition.anchor = Some(now);
        definition.cursor = Some(first);
        definition.consecutive_failures = 0;
        definition.attention_reason = None;
        definition.updated_at = now;
        Ok(approval)
    }

    pub fn pause_automation(&mut self, id: u64, now: i64) -> Result<(), String> {
        let definition = self.automation(id).ok_or("Automation not found.")?;
        if !matches!(
            definition.status,
            DefinitionStatus::Enabled | DefinitionStatus::NeedsAttention
        ) {
            return Err("Only an enabled automation can be paused.".into());
        }
        for occurrence in self
            .automation_occurrences
            .iter_mut()
            .filter(|o| {
                o.automation_id == id
                    && o.status == crate::automation::OccurrenceStatus::Queued
            })
        {
            let _ = occurrence.transition(
                crate::automation::OccurrenceStatus::Skipped,
                Some(crate::automation::OccurrenceReason::new(
                    crate::automation::SkipCode::Paused,
                    "The automation was paused before this request started.",
                )),
            );
        }
        let definition = self.automation_mut(id).unwrap();
        definition.status = DefinitionStatus::Paused;
        definition.cursor = None;
        definition.updated_at = now;
        Ok(())
    }

    /// Resuming re-anchors intervals and never backfills time spent paused.
    pub fn resume_automation(&mut self, id: u64, now: i64) -> Result<(), String> {
        let definition = self.automation(id).ok_or("Automation not found.")?;
        if !matches!(
            definition.status,
            DefinitionStatus::Paused | DefinitionStatus::NeedsAttention
        ) {
            return Err("Only a paused automation can be resumed.".into());
        }
        if !definition.approval_current() {
            return Err("Review and approve the current version before resuming.".into());
        }
        if !self.projects.iter().any(|p| p.id == definition.project_id) {
            return Err("The automation's project is unavailable. Choose another project first.".into());
        }
        let first = definition.schedule.first_after(now)?;
        let definition = self.automation_mut(id).unwrap();
        definition.status = DefinitionStatus::Enabled;
        definition.anchor = Some(now);
        definition.cursor = Some(first);
        definition.consecutive_failures = 0;
        definition.attention_reason = None;
        definition.updated_at = now;
        Ok(())
    }

    /// Temporarily unexposed in the UI; the store capability stays for the
    /// list-level workflow that will own it.
    #[allow(dead_code)]
    pub fn archive_automation(&mut self, id: u64, now: i64) -> Result<(), String> {
        let definition = self.automation(id).ok_or("Automation not found.")?;
        if definition.status == DefinitionStatus::Archived {
            return Ok(());
        }
        for occurrence in self
            .automation_occurrences
            .iter_mut()
            .filter(|o| {
                o.automation_id == id
                    && o.status == crate::automation::OccurrenceStatus::Queued
            })
        {
            let _ = occurrence.transition(
                crate::automation::OccurrenceStatus::Skipped,
                Some(crate::automation::OccurrenceReason::new(
                    crate::automation::SkipCode::Archived,
                    "The automation was archived before this request started.",
                )),
            );
        }
        let definition = self.automation_mut(id).unwrap();
        definition.status = DefinitionStatus::Archived;
        definition.cursor = None;
        definition.updated_at = now;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn copy_automation(&mut self, id: u64, now: i64) -> Result<u64, String> {
        let source = self.automation(id).ok_or("Automation not found.")?.clone();
        let id = self.next_automation_id()?;
        self.automations.push(AutomationDefinition {
            id,
            version: 1,
            project_id: source.project_id,
            shape: source.shape,
            name: format!("{} (copy)", source.name),
            objective: source.objective,
            expected_result: source.expected_result,
            schedule: source.schedule,
            status: DefinitionStatus::Draft,
            template: source.template,
            approval: None,
            policy: source.policy,
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: now,
            updated_at: now,
        });
        Ok(id)
    }

    pub fn mark_automation_attention(&mut self, id: u64, reason: impl Into<String>) {
        if let Some(definition) = self.automation_mut(id) {
            definition.status = DefinitionStatus::NeedsAttention;
            definition.attention_reason = Some(reason.into());
            definition.cursor = None;
        }
    }

    pub fn note_automation_success(&mut self, id: u64) {
        if let Some(definition) = self.automation_mut(id) {
            definition.consecutive_failures = 0;
        }
    }

    /// Record a final failure and pause the schedule at the configured threshold.
    /// Skipped and cancelled occurrences are not failures.
    pub fn note_automation_failure(&mut self, id: u64, now: i64) -> Option<String> {
        let definition = self.automation_mut(id)?;
        definition.consecutive_failures = definition.consecutive_failures.saturating_add(1);
        definition.updated_at = now;
        (definition.consecutive_failures >= definition.policy.max_consecutive_failures).then(|| {
            let reason = format!(
                "Paused after {} consecutive failed runs. Resolve the cause, then resume.",
                definition.consecutive_failures
            );
            definition.status = DefinitionStatus::NeedsAttention;
            definition.attention_reason = Some(reason.clone());
            definition.cursor = None;
            reason
        })
    }

    /// A period's goal. Research periods group goals under their own topic; a
    /// quick task's goal is deliberately topic-less.
    fn create_automation_goal(
        &mut self,
        title: String,
        brief: String,
        project: u64,
        topic: Option<u64>,
        topicless: bool,
    ) -> Result<u64, String> {
        if brief.trim().is_empty() {
            return Err("Describe the research scope and questions first.".into());
        }
        let id = if topicless {
            self.create_root_task_without_topic(title, project, Stage::Planned)?
        } else {
            self.create_task_in_topic(title, project, Stage::Planned, None, topic)?
        };
        let task = self.task_mut(id).unwrap();
        task.description = brief;
        task.labels = vec!["Research".into()];
        task.research = Some(crate::research::ResearchTask {
            requires_approval: false,
            kind: ResearchKind::Goal,
            goal_id: id,
            depends_on: vec![],
            space_name: format!(
                "What the hell are you planning - research {id} {}",
                chrono::Utc::now().timestamp_millis()
            ),
        });
        Ok(id)
    }

    /// Instantiate one approved step for a quick task. This deliberately bypasses
    /// the 3-12 task research plan grammar used by multi-step research goals.
    fn apply_single_step(
        &mut self,
        goal: u64,
        step: &AutomationTemplateStep,
    ) -> Result<u64, String> {
        let goal_task = self
            .task(goal)
            .filter(|task| task.research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal))
            .ok_or("Choose an active research goal.")?;
        if self.tasks.iter().any(|task| {
            task.research
                .as_ref()
                .is_some_and(|r| r.goal_id == goal && r.kind != ResearchKind::Goal)
        }) {
            return Err("This goal already has an execution step.".into());
        }
        let project = goal_task.project_id;
        let space_name = goal_task
            .research
            .as_ref()
            .map(|research| research.space_name.clone())
            .unwrap_or_default();
        let id = self.create_task(step.title.clone(), project, Stage::Planned, Some(goal))?;
        let task = self.task_mut(id).unwrap();
        task.description = step.instructions.clone();
        task.labels = vec!["Research".into(), step.kind.label().into()];
        task.research = Some(crate::research::ResearchTask {
            requires_approval: false,
            kind: step.kind,
            goal_id: goal,
            depends_on: vec![],
            space_name,
        });
        self.task_mut(goal).unwrap().activity.push(format!(
            "Generated 1 automation step · {}",
            crate::opencode::timestamp()
        ));
        Ok(id)
    }

    /// Materialize the occurrence's research entities and store the occurrence.
    /// Callers commit this through the normal storage transaction; on error nothing
    /// from this mutation may be published to the live store.
    pub fn materialize_occurrence(
        &mut self,
        mut occurrence: AutomationOccurrence,
    ) -> Result<AutomationOccurrence, String> {
        if occurrence.id == 0 {
            occurrence.id = self.next_occurrence_id()?;
        }
        if self
            .automation_occurrence_by_key(&occurrence.trigger_key)
            .is_some()
        {
            return Err("This trigger already has an occurrence.".into());
        }
        let definition = self
            .automation(occurrence.automation_id)
            .ok_or("Automation not found.")?
            .clone();
        if definition.version != occurrence.definition_version {
            return Err("The automation changed before this occurrence was committed.".into());
        }
        if !self.projects.iter().any(|p| p.id == definition.project_id) {
            return Err("The automation's project is not available.".into());
        }
        let template = definition
            .template
            .clone()
            .ok_or("The automation has no approved plan.")?;
        let label = occurrence
            .scheduled_for
            .and_then(|value| ScheduledTime::new(value, &occurrence.timezone).ok())
            .map(|value| value.local)
            .unwrap_or_else(|| "Manual run".into());
        // Research periods get their own topic so each period stays a separate
        // result. Quick tasks have no topic at all; the period itself is the container.
        let topic = if definition.shape.is_quick() {
            None
        } else {
            let topic = self.create_topic(
                definition.project_id,
                format!("{} · {}", definition.name, label),
            )?;
            self.topics
                .iter_mut()
                .find(|candidate| candidate.id == topic)
                .unwrap()
                .automation_id = Some(definition.id);
            Some(topic)
        };
        let mut goals = Vec::with_capacity(template.goals.len());
        for (index, planned) in template.goals.iter().enumerate() {
            let goal_title = if template.goals.len() == 1 {
                format!("{} · {}", definition.name, label)
            } else {
                format!("{} · {} · {}", definition.name, planned.title, label)
            };
            let brief = if index == 0 {
                format!(
                    "{}\n\n{}\n\nExpected result:\n{}",
                    occurrence.snapshot.objective,
                    planned.brief,
                    occurrence.snapshot.expected_result
                )
            } else {
                planned.brief.clone()
            };
            let goal = self.create_automation_goal(
                goal_title,
                brief,
                definition.project_id,
                topic,
                definition.shape.is_quick(),
            )?;
            {
                let goal_task = self.task_mut(goal).unwrap();
                goal_task.automation_id = Some(definition.id);
                goal_task.stage = Stage::Planned;
                goal_task
                    .research
                    .as_mut()
                    .ok_or("Could not create the occurrence's research goal.")?
                    .space_name = format!(
                    "What the hell are you planning - automation {} occurrence {} goal {} {}",
                    definition.id,
                    occurrence.id,
                    index + 1,
                    chrono::Utc::now().timestamp_millis()
                );
            }
            if definition.shape.is_quick() {
                // A quick task intentionally has no multi-step plan; the research
                // plan parser requires 3-12 tasks, so instantiate its single step directly.
                let step = planned
                    .steps
                    .first()
                    .ok_or("The quick task has no step.")?
                    .clone();
                let id = self.apply_single_step(goal, &step)?;
                let step = self.task_mut(id).unwrap();
                step.automation_id = Some(definition.id);
                step.stage = Stage::Planned;
            } else {
                // The approved template is instantiated verbatim; no planning run is
                // prepared for a period, so a reviewed plan never changes silently.
                let ids = self.apply_research_plan(goal, &planned.plan_json()?)?;
                for id in &ids {
                    let step = self.task_mut(*id).unwrap();
                    step.automation_id = Some(definition.id);
                    step.stage = Stage::Planned;
                }
            }
            let approval_signature = self.goal_plan_signature(goal);
            {
                let goal_task = self.task_mut(goal).unwrap();
                goal_task.research.as_mut().unwrap().requires_approval = true;
                goal_task.approved_plan = approval_signature;
                goal_task
                    .activity
                    .push(crate::domain::activity_event("Automation occurrence approved"));
            }
            goals.push(goal);
        }
        occurrence.research_goal_id = goals.first().copied();
        occurrence.research_goal_ids = goals;
        occurrence.research_topic_id = topic;
        self.automation_occurrences.push(occurrence.clone());
        Ok(occurrence)
    }

    /// Select the next runnable step and prepare a run with the snapshot's models,
    /// agent and immutable occurrence time context.
    pub fn prepare_occurrence_run(
        &self,
        occurrence: &AutomationOccurrence,
        now: i64,
    ) -> Result<AgentRun, String> {
        if !matches!(
            occurrence.status,
            crate::automation::OccurrenceStatus::Queued
                | crate::automation::OccurrenceStatus::Running
                | crate::automation::OccurrenceStatus::WaitingRetry
                | crate::automation::OccurrenceStatus::Verifying
        ) {
            return Err("This occurrence is not ready to execute.".into());
        }
        if let Some(deadline) = occurrence.deadline_at
            && now >= deadline
        {
            return Err("The occurrence's execution time limit has been reached.".into());
        }
        let goals: Vec<u64> = if occurrence.research_goal_ids.is_empty() {
            occurrence.research_goal_id.into_iter().collect()
        } else {
            occurrence.research_goal_ids.clone()
        };
        if goals.is_empty() {
            return Err("The occurrence has no research goal.".into());
        }
        // Goals run in their approved order; a later goal starts only after every
        // earlier goal has completed, which is how the reviewed plan was approved.
        let mut selected = None;
        for goal in goals {
            match self.ready_research_tasks(goal) {
                Ok(ready) => {
                    if let Some(task) = ready.into_iter().next() {
                        selected = Some(task);
                        break;
                    }
                    if !self.goal_tasks_complete(goal) {
                        return Err(
                            "The next goal of this period is not runnable yet.".into()
                        );
                    }
                }
                Err(error) => return Err(error),
            }
        }
        let Some(task) = selected else {
            return Err("No runnable step remains for this occurrence.".into());
        };
        let mut snapshot = self.clone();
        snapshot.research_config.models = occurrence.snapshot.models.clone();
        snapshot.research_config.agents = occurrence.snapshot.agents.clone();
        // The occurrence snapshot may name an agent that is no longer the default;
        // keep the effective agent explicit so the run uses the approved route.
        for kind in ResearchKind::ALL {
            let index = kind.index();
            let agent = occurrence
                .snapshot
                .agents
                .get(index)
                .and_then(|value| value.clone())
                .unwrap_or_else(|| snapshot.research_config.default_agent.clone());
            snapshot.research_config.agents[index] = Some(agent);
        }
        let mut run = crate::research::prepare_run(&snapshot, task)?;
        run.automation_occurrence_id = Some(occurrence.id);
        run.prompt = with_time_context(run.prompt, occurrence);
        run.logs.push(format!(
            "[automation] Occurrence {} · {}",
            occurrence.id, occurrence.trigger_key
        ));
        Ok(run)
    }
}

/// The prompt block that makes relative dates in a period unambiguous.
pub fn with_time_context(prompt: String, occurrence: &AutomationOccurrence) -> String {
    let block = time_context_block(occurrence);
    let replacement = format!("Research time: {}\n", local_reference(occurrence));
    if let Some(index) = prompt.find("Research time:") {
        let line_end = prompt[index..]
            .find('\n')
            .map(|offset| index + offset + 1)
            .unwrap_or(prompt.len());
        let mut result = prompt;
        result.replace_range(index..line_end, &replacement);
        format!("{result}\n{block}")
    } else {
        format!("{prompt}\n\n{block}")
    }
}

fn local_reference(occurrence: &AutomationOccurrence) -> String {
    ScheduledTime::new(occurrence.reference_time, &occurrence.timezone)
        .map(|value| value.local)
        .unwrap_or_else(|_| occurrence.reference_time.to_string())
}

pub fn time_context_block(occurrence: &AutomationOccurrence) -> String {
    let local = |value: Option<i64>| {
        value
            .and_then(|value| ScheduledTime::new(value, &occurrence.timezone).ok())
            .map(|value| format!("{} ({})", value.utc, value.local))
            .unwrap_or_else(|| "none".into())
    };
    format!(
        "AUTOMATION OCCURRENCE CONTEXT (authoritative; overrides general current-time wording):\n- Automation: {}\n- Occurrence: {} ({})\n- Trigger: {}\n- Period scheduled for: {} UTC\n- Actual start: {}\n- Time zone: {}\n- Reference time: {}\n- Data window: {}\n- This is one period of an approved recurring task. Interpret “yesterday”, “last week” and similar phrases against the reference time and time zone above, never against the actual start date.\n- A retry or resumed step keeps this same reference time window.\n- This period was started by a schedule, so no one is watching the session. If a source requires sign-in, a CAPTCHA or a browser handoff, record it as an evidence gap and continue with accessible sources instead of waiting for input.
- Do not schedule, publish, send, trade, or modify external systems.",
        occurrence.snapshot.objective.chars().take(160).collect::<String>(),
        occurrence.id,
        occurrence.source.label(),
        occurrence.trigger_key,
        occurrence
            .scheduled_for
            .map(|value| ScheduledTime::new(value, &occurrence.timezone).map(|v| v.local).unwrap_or_else(|_| value.to_string()))
            .unwrap_or_else(|| "manual request".into()),
        local(occurrence.actual_started_at),
        occurrence.timezone,
        local(Some(occurrence.reference_time)),
        match &occurrence.data_window.state {
            crate::automation::TimeWindowState::Unresolved { reason } => format!("unresolved — {reason}"),
            crate::automation::TimeWindowState::Resolved { start_utc, end_utc, label } =>
                format!("resolved — {label} [{start_utc}, {end_utc}) UTC"),
        },
    )
}

pub(crate) fn window_for(
    template: &AutomationTemplate,
    reference: i64,
    timezone: &str,
) -> Result<TimeWindow, String> {
    match template.window_rule {
        crate::automation::WindowRule::PreviousLocalDay => {
            resolved_previous_day(reference, timezone)
        }
        crate::automation::WindowRule::Unresolved => Ok(TimeWindow::unresolved(
            "The requested time range is not derived by the scheduler; verify it against the reference time in the collection step.",
        )),
    }
}


pub fn resolved_previous_day(reference: i64, timezone: &str) -> Result<TimeWindow, String> {
    let tz = crate::automation_schedule::parse_timezone(timezone)?;
    let window = previous_local_day(reference, tz)?;
    Ok(TimeWindow::resolved(
        window,
        format!("Previous local day in {timezone} (start inclusive, end exclusive); business-day validity is verified by the collection step"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{
        AutomationOccurrenceSnapshot, AutomationPolicy, OccurrenceStatus, TriggerSource,
        TimeWindow, DEFAULT_EXECUTION_TIMEOUT_SECONDS,
    };
    use crate::automation_schedule::ScheduleSpec;
    use crate::domain::Store;

    fn store() -> Store {
        let mut store = Store::empty();
        store.projects[0].directory = std::env::temp_dir().to_string_lossy().into();
        store.default_model = "test/model".into();
        store
    }
    fn definition(store: &mut Store) -> u64 {
        let template = build_template("追踪昨天的公开信息", "A sourced note").unwrap();
        let schedule = Schedule {
            spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
            timezone: "Asia/Shanghai".into(),
        };
        let id = store.next_automation_id().unwrap();
        store.automations.push(AutomationDefinition {
            id,
            version: 1,
            project_id: 1,
            shape: AutomationShape::Research,
            name: "Daily topic".into(),
            objective: "Track a public topic".into(),
            expected_result: "A sourced note".into(),
            schedule,
            status: DefinitionStatus::Draft,
            template: Some(template),
            approval: None,
            policy: AutomationPolicy::default(),
            cursor: None,
            anchor: None,
            consecutive_failures: 0,
            attention_reason: None,
            created_at: 0,
            updated_at: 0,
        });
        id
    }
    fn occurrence(store: &Store, automation: u64, scheduled: i64) -> AutomationOccurrence {
        let definition = store.automation(automation).unwrap();
        let window = resolved_previous_day(scheduled, &definition.schedule.timezone).unwrap();
        AutomationOccurrence {
            id: store.next_occurrence_id().unwrap(),
            automation_id: automation,
            definition_version: definition.version,
            source: TriggerSource::Scheduled,
            trigger_key: crate::automation::scheduled_trigger_key(automation, scheduled),
            scheduled_for: Some(scheduled),
            requested_at: None,
            actual_started_at: Some(scheduled + 60),
            timezone: definition.schedule.timezone.clone(),
            reference_time: scheduled,
            data_window: window,
            snapshot: AutomationOccurrenceSnapshot {
                objective: definition.objective.clone(),
                expected_result: definition.expected_result.clone(),
                template: definition.template.clone().unwrap(),
                models: std::array::from_fn(|_| "test/model".to_string()),
                agents: Default::default(),
                materials: vec![],
                policy: definition.policy.clone(),
                approval_version: definition.version,
                approval_signature: "approval".into(),
            },
            status: OccurrenceStatus::Queued,
            reason: None,
            research_goal_id: None,
            research_goal_ids: vec![],
            research_topic_id: None,
            run_ids: vec![],
            final_run_id: None,
            unread: false,
            skipped_count: 0,
            skipped_from: None,
            skipped_to: None,
            created_at: scheduled,
            deadline_at: None,
        }
    }

    #[test]
    fn template_is_stable_and_rejects_external_actions() {
        let template = build_template("Track filings", "A cited summary").unwrap();
        template.validate(AutomationShape::Research).unwrap();
        assert_eq!(
            template.signature(),
            build_template("Track filings", "A cited summary")
                .unwrap()
                .signature()
        );
        assert!(template
            .goals
            .iter()
            .flat_map(|goal| &goal.steps)
            .any(|step| step.kind == ResearchKind::Advise));
        for text in ["send email to the team", "自动交易 BTC", "git push"] {
            assert!(unsupported_reason(text).is_some(), "{text}");
            assert!(build_template(text, "Result").is_err());
        }
    }

    #[test]
    fn approval_resolves_models_and_invalidates_on_version_change() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        let approval = store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        assert_eq!(approval.models[1], "test/collect");
        assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::Enabled);
        assert!(store.automation(id).unwrap().cursor.unwrap() > 1_000);
        store.revise_automation(
            id,
            1,
            "Renamed".into(),
            "Track a public topic".into(),
            "A sourced note".into(),
            store.automation(id).unwrap().schedule.clone(),
            AutomationPolicy::default(),
            store.automation(id).unwrap().template.clone(),
            2_000,
        )
        .unwrap();
        let revised = store.automation(id).unwrap();
        assert_eq!(revised.version, 2);
        assert_eq!(revised.status, DefinitionStatus::Draft);
        assert!(!revised.approval_current());
    }

    #[test]
    fn occurrence_materializes_isolated_research_entities() {
        let mut store = store();
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        let first = store
            .materialize_occurrence(occurrence(&store, id, 2000))
            .unwrap();
        let second = store
            .materialize_occurrence(occurrence(&store, id, 3000))
            .unwrap();
        assert_ne!(first.research_goal_id, second.research_goal_id);
        let first_goal = first.research_goal_id.unwrap();
        let second_goal = second.research_goal_id.unwrap();
        assert_eq!(store.task(first_goal).unwrap().automation_id, Some(id));
        assert_ne!(
            store.task(first_goal).unwrap().research.as_ref().unwrap().space_name,
            store.task(second_goal).unwrap().research.as_ref().unwrap().space_name
        );
        assert_eq!(store.research_children(first_goal).len(), 3);
        assert!(store.goal_execution_approved(first_goal));
        for child in store.research_children(first_goal) {
            assert!(!store.research_pending_approval(child));
            assert_eq!(store.task(child).unwrap().automation_id, Some(id));
        }
        assert!(store.topics.iter().any(|topic| topic.id == first.research_topic_id.unwrap() && topic.automation_id == Some(id)));
        // Duplicate trigger keys never create a second goal.
        assert!(store
            .materialize_occurrence(occurrence(&store, id, 2000))
            .is_err());
    }

    #[test]
    fn research_planning_request_creates_no_topic_and_no_goals() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Pending".into(),
                "Track the topic".into(),
                "A cited note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                None,
                1_000,
            )
            .unwrap();
        let topics_before = store.topics.len();
        let tasks_before = store.tasks.len();
        let request = store
            .create_automation_request(id, 1, "Track semiconductor capacity")
            .unwrap();
        assert_eq!(store.topics.len(), topics_before, "planning must not create a topic");
        assert_eq!(store.tasks.len(), tasks_before, "planning must not create goals");
        let run = store.prepare_request_plan(request).unwrap();
        assert_eq!(run.project_plan, Some(1));
        assert_eq!(run.task_id, 0);
        assert!(store
            .research_requests
            .iter()
            .find(|candidate| candidate.id == request)
            .is_some_and(|candidate| candidate.automation_id == Some(id)));
        // The same automation reuses one saved request instead of planning twice.
        assert_eq!(
            store
                .create_automation_request(id, 1, "Track semiconductor capacity")
                .unwrap(),
            request
        );
    }

    #[test]
    fn legacy_single_goal_templates_still_load() {
        let legacy = r#"{"goals":[{"title":"Daily","brief":"Brief","steps":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}],"window_rule":"unresolved"}"#;
        let template: AutomationTemplate = serde_json::from_str(legacy).unwrap();
        assert_eq!(template.goals.len(), 1);
        assert_eq!(template.goals[0].steps.len(), 3);
        // The pre-goal shape stored instructions plus a flat step list.
        let older = r#"{"instructions":"Brief","steps":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}],"window_rule":"unresolved"}"#;
        let template: AutomationTemplate = serde_json::from_str(older).unwrap();
        assert_eq!(template.goals.len(), 1);
        assert_eq!(template.goals[0].brief, "Brief");
        assert_eq!(template.goals[0].steps.len(), 3);
    }

    #[test]
    fn multi_goal_period_runs_goals_in_order_without_replanning() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let plan = r#"{"topic_name":"Supply chain","brief":{"objective":"Track supply","deliverable":"Sourced note","scope":"Latest quarter","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Foundry capacity","brief":"Track announced capacity","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare capacity","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]},{"title":"Equipment orders","brief":"Track equipment bookings","tasks":[{"title":"Collect","instructions":"Collect orders","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare orders","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]}]}"#;
        let parsed = crate::request_planning::parse_request_plan(plan).unwrap();
        let template = template_from_plan(&parsed).unwrap();
        let id = store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Supply chain".into(),
                "Track supply".into(),
                "A sourced note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                Some(template),
                1_000,
            )
            .unwrap();
        store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        let occurrence = store
            .materialize_occurrence(occurrence(&store, id, 2_000))
            .unwrap();
        assert_eq!(occurrence.research_goal_ids.len(), 2);
        let first = occurrence.research_goal_ids[0];
        let second = occurrence.research_goal_ids[1];
        assert_eq!(store.research_children(first).len(), 3);
        // Finish the first goal exactly like a fake/real period would.
        for task in store.research_children(first) {
            let mut run = store.prepare_run(task).unwrap();
            run.automation_occurrence_id = Some(occurrence.id);
            run.status = "Succeeded".into();
            run.output = "Goal one report".into();
            store.runs.push(run);
            store.set_stage(task, Stage::Done).unwrap();
        }
        store.sync_goal_progress(first);
        assert!(store.goal_tasks_complete(first));
        assert!(!store.occurrence_goals_complete(&occurrence));
        // The next runnable step belongs to the second approved goal; no planning runs.
        let run = store.prepare_occurrence_run(&occurrence, 2_060).unwrap();
        assert_eq!(store.owning_research_goal(run.task_id), Some(second));
        assert_eq!(run.research_kind, Some(ResearchKind::Collect));
        assert!(store
            .runs
            .iter()
            .all(|candidate| candidate.research_kind != Some(ResearchKind::Goal) || candidate.task_id != 0));
    }

    #[test]
    fn quick_task_uses_one_step_and_needs_only_its_own_model() {
        let mut store = store();
        // Only the report step is configured; a quick task must not demand
        // collection or analysis models it will never use.
        store.research_config.models = ["test/plan".into(), String::new(), String::new(), "test/advise".into()];
        let template = build_quick_template(
            "Check today's fuel price in Shenzhen",
            "Current price with source",
        )
        .unwrap();
        let id = store
            .create_automation_draft(
                1,
                AutomationShape::Quick,
                "Fuel price".into(),
                "Check today's fuel price in Shenzhen".into(),
                "Current price with source".into(),
                Schedule {
                    spec: ScheduleSpec::Daily { hour: 10, minute: 0 },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                Some(template),
                1_000,
            )
            .unwrap();
        store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        let topics_before = store.topics.len();
        let occurrence = store
            .materialize_occurrence(occurrence(&store, id, 2_000))
            .unwrap();
        assert!(occurrence.research_topic_id.is_none(), "quick tasks have no topic");
        assert_eq!(store.topics.len(), topics_before);
        let goal = occurrence.research_goal_id.unwrap();
        assert_eq!(occurrence.research_goal_ids, vec![goal]);
        assert_eq!(store.research_children(goal).len(), 1);
        assert_eq!(
            store
                .research_children(goal)
                .into_iter()
                .filter_map(|task| store.task(task).and_then(|t| t.research.as_ref()))
                .map(|research| research.kind)
                .collect::<Vec<_>>(),
            vec![ResearchKind::Advise]
        );
    }

    #[test]
    fn research_periods_get_their_own_topic_and_the_plan_is_frozen() {
        let mut store = store();
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        let first = store
            .materialize_occurrence(occurrence(&store, id, 2_000))
            .unwrap();
        let second = store
            .materialize_occurrence(occurrence(&store, id, 3_000))
            .unwrap();
        let first_topic = first.research_topic_id.unwrap();
        let second_topic = second.research_topic_id.unwrap();
        assert_ne!(first_topic, second_topic, "each period has its own topic");
        for topic in [first_topic, second_topic] {
            let topic = store.topics.iter().find(|candidate| candidate.id == topic).unwrap();
            assert_eq!(topic.automation_id, Some(id));
            assert!(topic.name.starts_with("Daily topic"));
        }
        assert_ne!(first.research_goal_id, second.research_goal_id);
    }

    #[test]
    fn freezing_a_reviewed_plan_versions_the_automation_without_planning_again() {
        let mut store = store();
        let id = store
            .create_automation_draft(
                1,
                AutomationShape::Research,
                "Pending".into(),
                "Track the topic".into(),
                "A cited note".into(),
                Schedule {
                    spec: ScheduleSpec::Daily { hour: 8, minute: 30 },
                    timezone: "Asia/Shanghai".into(),
                },
                AutomationPolicy::default(),
                None,
                1_000,
            )
            .unwrap();
        let plan = r#"{"topic_name":"Semiconductor supply","brief":{"objective":"Track supply","deliverable":"Sourced note","scope":"Latest quarter","assumptions":[],"facts_to_verify":[]},"goals":[{"title":"Foundry capacity","brief":"Track announced capacity","tasks":[{"title":"Collect","instructions":"Collect filings","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare capacity","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]},{"title":"Equipment orders","brief":"Track equipment bookings","tasks":[{"title":"Collect","instructions":"Collect orders","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Compare orders","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Write the note","kind":"advise","depends_on":[1]}]}]}"#;
        assert_eq!(store.apply_automation_template(id, plan, 2_000).unwrap(), 2);
        let frozen = store.automation(id).unwrap();
        assert_eq!(frozen.version, 1);
        assert_eq!(frozen.name, "Semiconductor supply");
        assert_eq!(frozen.template.as_ref().unwrap().goals.len(), 2);
        assert_eq!(frozen.status, DefinitionStatus::Draft);
        // The frozen plan can be approved and instantiated without planning again.
        store
            .approve_and_enable(
                id,
                vec![],
                2_100,
            )
            .unwrap();
        let occurrence = store
            .materialize_occurrence(occurrence(&store, id, 3_600))
            .unwrap();
        assert_eq!(occurrence.research_goal_ids.len(), 2);
        assert!(occurrence.research_topic_id.is_some());
        // A replan is a new version and must be reviewed again.
        assert_eq!(store.apply_automation_template(id, plan, 3_000).unwrap(), 2);
        let revised = store.automation(id).unwrap();
        assert_eq!(revised.version, 2);
        assert!(!revised.approval_current());
    }

    #[test]
    fn occurrence_run_uses_snapshot_model_and_time_window() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_000,
            )
            .unwrap();
        let mut occurrence = store
            .materialize_occurrence(occurrence(&store, id, 1_700_000_000))
            .unwrap();
        occurrence.snapshot.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        occurrence.snapshot.agents = std::array::from_fn(|_| Some("OpenCode".to_string()));
        *store.occurrence_mut(occurrence.id).unwrap() = occurrence.clone();
        occurrence.mark_running(1_700_000_060).unwrap();
        *store.occurrence_mut(occurrence.id).unwrap() = occurrence.clone();
        let run = store
            .prepare_occurrence_run(&occurrence, 1_700_000_060)
            .unwrap();
        assert_eq!(run.model, "test/collect");
        assert_eq!(run.automation_occurrence_id, Some(occurrence.id));
        assert!(run.prompt.contains("AUTOMATION OCCURRENCE CONTEXT"));
        assert!(run.prompt.contains("Reference time:"));
        assert!(run.prompt.contains("Previous local day"));
        assert_eq!(run.research_kind, Some(ResearchKind::Collect));
        // The approved snapshot still wins after a global model and agent change.
        store.default_model = "test/changed".into();
        store.research_config.default_agent = "Codex".into();
        let run = store
            .prepare_occurrence_run(&occurrence, 1_700_000_060)
            .unwrap();
        assert_eq!(run.model, "test/collect");
        assert_eq!(run.agent, "OpenCode");
    }

    #[test]
    fn only_explicit_yesterday_resolves_a_window() {
        use crate::automation::WindowRule;
        for (text, expected) in [
            ("总结昨天的新闻", WindowRule::PreviousLocalDay),
            ("昨日公告", WindowRule::PreviousLocalDay),
            ("yesterday's filing", WindowRule::PreviousLocalDay),
            ("过去五分钟的信息", WindowRule::Unresolved),
            ("上周总结", WindowRule::Unresolved),
            ("上一个交易日的公告", WindowRule::Unresolved),
            ("最新消息", WindowRule::Unresolved),
            ("上一个交易日走势＋昨天新闻", WindowRule::Unresolved),
            ("比较今天和昨天", WindowRule::Unresolved),
            ("昨天和前天的新闻", WindowRule::Unresolved),
            ("不要只看昨天的新闻", WindowRule::Unresolved),
            ("不是昨天的公告", WindowRule::Unresolved),
            ("not yesterday's filing", WindowRule::Unresolved),
            ("对比昨日与上月的公告数量", WindowRule::Unresolved),
            ("比较昨天和去年同日的销售额", WindowRule::Unresolved),
            ("不要昨日的公告", WindowRule::Unresolved),
        ] {
            assert_eq!(infer_window_rule(text), expected, "{text}");
        }
        let yesterday = build_template("总结昨天的新闻", "简报").unwrap();
        let resolved = window_for(&yesterday, 1_700_000_000, "Asia/Shanghai").unwrap();
        assert!(resolved.is_resolved());
        let minutes = build_template("过去五分钟的信息", "简报").unwrap();
        assert!(!window_for(&minutes, 1_700_000_000, "Asia/Shanghai")
            .unwrap()
            .is_resolved());
        let week = build_template("上周总结", "简报").unwrap();
        assert!(!window_for(&week, 1_700_000_000, "Asia/Shanghai")
            .unwrap()
            .is_resolved());
        let trading_day = build_template("上一个交易日的公告", "简报").unwrap();
        assert!(!window_for(&trading_day, 1_700_000_000, "Asia/Shanghai")
            .unwrap()
            .is_resolved());
    }

    #[test]
    fn time_context_never_resolves_a_business_calendar() {
        let mut store = store();
        let id = definition(&mut store);
        let occurrence = occurrence(&store, id, 1_700_000_000);
        let block = time_context_block(&occurrence);
        assert!(block.contains("business-day validity is verified"));
        assert!(!block.contains("last trading day"));
        let unresolved = AutomationOccurrence {
            data_window: TimeWindow::unresolved("source calendar not checked"),
            ..occurrence
        };
        assert!(time_context_block(&unresolved).contains("unresolved"));
    }

    #[test]
    fn deadline_blocks_new_steps_but_not_existing_validation() {
        let mut store = store();
        let id = definition(&mut store);
        let mut occurrence = store
            .materialize_occurrence(occurrence(&store, id, 1_700_000_000))
            .unwrap();
        occurrence.snapshot.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        occurrence.snapshot.agents = std::array::from_fn(|_| Some("OpenCode".to_string()));
        *store.occurrence_mut(occurrence.id).unwrap() = occurrence.clone();
        occurrence.mark_running(1_700_000_000).unwrap();
        *store.occurrence_mut(occurrence.id).unwrap() = occurrence.clone();
        let deadline = occurrence.actual_started_at.unwrap() + DEFAULT_EXECUTION_TIMEOUT_SECONDS;
        let error = store
            .prepare_occurrence_run(&occurrence, deadline)
            .unwrap_err();
        assert!(error.contains("time limit"));
        let snapshot: AutomationOccurrenceSnapshot = occurrence.snapshot.clone();
        assert_eq!(snapshot.approval_version, 1);
    }

    #[test]
    fn two_periods_have_distinct_reference_windows_paths_and_sessions() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_700_000_000,
            )
            .unwrap();
        let first_time = 1_700_000_000;
        let second_time = first_time + 86_400;
        let first = store
            .materialize_occurrence(occurrence(&store, id, first_time))
            .unwrap();
        let second = store
            .materialize_occurrence(occurrence(&store, id, second_time))
            .unwrap();
        let first_goal = first.research_goal_id.unwrap();
        let second_goal = second.research_goal_id.unwrap();
        let first_run = store
            .prepare_occurrence_run(&first, first_time + 60)
            .unwrap();
        let second_run = store
            .prepare_occurrence_run(&second, second_time + 60)
            .unwrap();
        let first_step = store.research_children(first_goal)[0];
        let second_step = store.research_children(second_goal)[0];
        assert_ne!(
            store.execution_directory(first_step).unwrap(),
            store.execution_directory(second_step).unwrap()
        );
        // Sessions are assigned by dispatch; each period starts without one, so a
        // retry can resume within its own period and never across periods.
        assert!(first_run.session_id.is_none());
        assert!(second_run.session_id.is_none());
        assert_ne!(
            store.task(first_goal).unwrap().research.as_ref().unwrap().space_name,
            store.task(second_goal).unwrap().research.as_ref().unwrap().space_name
        );
        assert!(first_run.prompt.contains("Reference time:"));
        assert!(!first_run.prompt.contains(&format!(
            "{}",
            crate::automation_schedule::ScheduledTime::new(second_time, "Asia/Shanghai")
                .unwrap()
                .local
        )));
        let first_window = match &first.data_window.state {
            crate::automation::TimeWindowState::Resolved { start_utc, .. } => *start_utc,
            _ => panic!("expected resolved window"),
        };
        let second_window = match &second.data_window.state {
            crate::automation::TimeWindowState::Resolved { start_utc, .. } => *start_utc,
            _ => panic!("expected resolved window"),
        };
        assert_eq!(second_window - first_window, 86_400);
        // A retry of the first period keeps the same reference time and goal.
        let mut failed = first_run.clone();
        failed.status = "Failed".into();
        store.runs.push(failed.clone());
        let retry = store.prepare_auto_retry(failed.id, first_time + 120).unwrap();
        assert_eq!(retry.automation_occurrence_id, Some(first.id));
        assert!(retry.prompt.contains("Reference time:"));
        assert_eq!(
            retry.prompt.matches("AUTOMATION OCCURRENCE CONTEXT").count(),
            first_run.prompt.matches("AUTOMATION OCCURRENCE CONTEXT").count()
        );
        assert_ne!(retry.id, failed.id);
        // Complete both periods independently with the observable fake runner.
        let mut runner = crate::automation_runtime::FakeAutomationRunner::default();
        for (occurrence, at) in [(&first, first_time), (&second, second_time)] {
            runner.start(occurrence);
            runner.complete(&mut store, occurrence.id, at + 300).unwrap();
        }
        for occurrence in [&first, &second] {
            let stored = store.occurrence(occurrence.id).unwrap();
            assert_eq!(stored.status, crate::automation::OccurrenceStatus::Succeeded);
            assert!(stored.unread);
            assert!(stored.final_run_id.is_some());
        }
        assert_eq!(runner.starts.len(), 2);
    }

    #[test]
    fn automation_topics_are_excluded_from_normal_publication() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_700_000_000,
            )
            .unwrap();
        let occurrence = store
            .materialize_occurrence(occurrence(&store, id, 1_700_000_000))
            .unwrap();
        let topic = occurrence.research_topic_id.unwrap();
        let mut runner = crate::automation_runtime::FakeAutomationRunner::default();
        store.occurrence_mut(occurrence.id).unwrap().mark_running(1_700_000_060).unwrap();
        runner.complete(&mut store, occurrence.id, 1_700_000_100).unwrap();
        assert_eq!(
            store.occurrence(occurrence.id).unwrap().status,
            crate::automation::OccurrenceStatus::Succeeded
        );
        assert!(store.topic_share_content(topic).is_none());
    }

    #[test]
    fn approval_uses_the_agent_selected_in_settings() {
        // Automations reuse the research agent choice; there is no separate
        // automation-only agent profile.
        let mut store = store();
        store.research_config.agents[1] = Some("Codex".into());
        store.research_config.models[1] = "fixture-codex".into();
        let id = definition(&mut store);
        let approval = store
            .approve_and_enable(id, vec![], 1_700_000_000)
            .unwrap();
        assert_eq!(approval.agents[1].as_deref(), Some("Codex"));
        assert_eq!(approval.models[1], "fixture-codex");
    }

    #[test]
    fn renaming_alone_keeps_the_approved_version() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_700_000_000,
            )
            .unwrap();
        let cursor = store.automation(id).unwrap().cursor;
        store.rename_automation(id, "Renamed only", 1_700_000_100).unwrap();
        let definition = store.automation(id).unwrap();
        assert_eq!(definition.name, "Renamed only");
        assert_eq!(definition.version, 1);
        assert_eq!(definition.cursor, cursor);
        assert!(definition.approval_current());
    }

    #[test]
    fn verified_recovery_resumes_schedule_but_never_user_pause() {
        let mut store = store();
        store.research_config.models = ["test/plan", "test/collect", "test/analyze", "test/advise"]
            .map(String::from);
        let id = definition(&mut store);
        store
            .approve_and_enable(
                id,
                vec![],
                1_700_000_000,
            )
            .unwrap();
        store.mark_automation_attention(id, "The previous session's state cannot be confirmed. Check the result before continuing; no work was replayed.");
        assert!(store.automation(id).unwrap().cursor.is_none());
        assert!(store.resume_after_verified_recovery(id, 1_700_000_100));
        let resumed = store.automation(id).unwrap();
        assert_eq!(resumed.status, DefinitionStatus::Enabled);
        assert!(resumed.cursor.is_some_and(|cursor| cursor > 1_700_000_100));
        assert!(resumed.attention_reason.is_none());
        // An unrelated attention reason is not cleared by a normal completion.
        store.mark_automation_attention(id, "The automation's project is unavailable.");
        assert!(!store.resume_after_verified_recovery(id, 1_700_000_200));
        assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::NeedsAttention);
        // A user pause is preserved.
        store.automation_mut(id).unwrap().status = DefinitionStatus::Paused;
        store.automation_mut(id).unwrap().attention_reason = None;
        assert!(!store.resume_after_verified_recovery(id, 1_700_000_300));
        assert_eq!(store.automation(id).unwrap().status, DefinitionStatus::Paused);
    }

    #[test]
    fn store_schema_version_is_explicit() {
        assert_eq!(crate::automation::AUTOMATION_SCHEMA_VERSION, 1);
        assert_eq!(
            store().automation_schema_version,
            crate::automation::AUTOMATION_SCHEMA_VERSION
        );
    }
}
