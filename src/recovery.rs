//! Read-only startup reconciliation and explicit resumption of previously started goals.
use crate::{
    app::{Scope, TaskApp},
    batch::BatchState,
    controls::*,
    domain::{AgentRun, Stage, Store},
    opencode,
    research::ResearchKind,
    views::*,
};
use gpui_kit::{
    component::{
        Disableable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};

fn recovery_title(id: impl Into<ElementId>, text: String, cx: &App) -> Button {
    let colors = crate::appearance::palette(cx);
    Button::new(id)
        .ghost()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(rgba(0).into())
                .foreground(rgb(colors.accent).into())
                .hover(rgb(colors.hover).into())
                .active(rgb(colors.accent_surface).into()),
        )
        .h_auto()
        .min_h(px(28.))
        .px_1()
        .py_1()
        .self_start()
        .max_w_full()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_left()
                .whitespace_normal()
                .child(text),
        )
}

use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SessionCheck {
    Checking,
    Running,
    Stopped,
    Complete(String),
    Unknown(String),
}
#[derive(Default)]
pub(crate) struct Recovery {
    pub checks: BTreeMap<u64, SessionCheck>,
    receiver: Option<mpsc::Receiver<(u64, SessionCheck)>>,
    next_check: Option<Instant>,
    queue: Vec<(u64, Vec<u64>)>,
}

// OpenCode 2.2 can store the CLI argument as a quoted string, twice. Match
// only exact encodings of this request; never accept substrings or unrelated turns.
fn request_matches(text: &str, prompt: &str) -> bool {
    if text == prompt {
        return true;
    }
    let quoted = format!("\"{}\"", prompt.replace('"', "\\\""));
    text == quoted || text == format!("{quoted} {quoted}")
}

// A session can contain several attempts. Never import a previous attempt's result.
fn parse_transcript(value: &Value, prompt: &str) -> Result<SessionCheck, String> {
    let value = value.get("data").unwrap_or(value);
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("Unsupported session transcript")?;
    let Some(start) = messages.iter().rposition(|m| {
        m["type"] == "user"
            && m["text"]
                .as_str()
                .is_some_and(|text| request_matches(text, prompt))
    }) else {
        return Err(
            "The saved request was not found in this session. Review before retrying.".into(),
        );
    };
    if messages[start + 1..].iter().any(|m| m["type"] == "user") {
        return Err("This session contains a newer request. Review before retrying.".into());
    }
    let tail = &messages[start + 1..];
    let Some(idle) = tail.last().filter(|m| m["type"] == "idle") else {
        return Ok(SessionCheck::Stopped);
    };
    if idle["outcome"] != "succeeded" {
        return Ok(SessionCheck::Stopped);
    }
    let output = tail
        .iter()
        .rev()
        .find(|m| m["type"] == "assistant")
        .and_then(|m| m["content"].as_array())
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p["type"] == "text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.trim().is_empty())
        .ok_or("Completed session has no final text")?;
    Ok(SessionCheck::Complete(output))
}
fn inspect(executable: &Path, run: &AgentRun) -> SessionCheck {
    if run.agent == "Codex" {
        return inspect_codex(run);
    }
    let Some(session) = &run.session_id else {
        return SessionCheck::Stopped;
    };
    // Queued work has no admitted prompt to reconcile.
    if run.started_at.is_empty() {
        return SessionCheck::Stopped;
    }
    let directory = Path::new(&run.directory);
    let result = (|| -> Result<SessionCheck, String> {
        let active: Value = serde_json::from_str(&opencode::capture(
            executable,
            directory,
            &["api", "session.active"],
        )?)
        .map_err(|e| e.to_string())?;
        let active = active["data"]
            .as_object()
            .ok_or("Unsupported session activity response")?;
        if active.contains_key(session) {
            return Ok(SessionCheck::Running);
        }
        let request = opencode::Request { agent: None, browser_executable: None,
            executable: executable.into(),
            directory: directory.canonicalize().map_err(|e| e.to_string())?,
            model: String::new(),
            prompt: String::new(),
            title: String::new(),
            files: vec![],
            session_id: Some(session.clone()),
        };
        // The child inherits this lease, including when its original UI process crashes.
        let _lease = match opencode::lock_session(&request) {
            Ok(lease) => lease,
            Err(error) if error.starts_with("This OpenCode session is still running") => {
                return Ok(SessionCheck::Running);
            }
            Err(error) => return Err(error),
        };
        let raw = opencode::capture(executable, directory, &["session", "export", session])?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        parse_transcript(&value, &run.prompt)
    })();
    result.unwrap_or_else(SessionCheck::Unknown)
}
// Every Codex child inherits an attempt lease, including before thread.started.
// Recovery deliberately avoids reading Codex's private rollout schema. Once stopped,
// the saved thread ID can be resumed explicitly by the normal dispatcher.
pub(crate) fn inspect_codex(run: &AgentRun) -> SessionCheck {
    if run.started_at.is_empty() { return SessionCheck::Stopped; }
    let directory = match Path::new(&run.directory).canonicalize() {
        Ok(path) => path,
        Err(error) => return SessionCheck::Unknown(format!("Cannot inspect Codex working directory: {error}")),
    };
    let request = opencode::Request { agent: None, browser_executable: None,
        executable: Default::default(), directory, model: run.model.clone(),
        prompt: run.prompt.clone(), title: run.task_title.clone(), files: vec![],
        session_id: run.session_id.clone(),
    };
    match crate::codex::lock_attempt(&request) {
        Ok(_lease) => SessionCheck::Stopped,
        Err(error) if error.contains("session is still running") => SessionCheck::Running,
        Err(error) => SessionCheck::Unknown(error),
    }
}

impl Store {
    // Superseded attempts are history, not live resource owners.
    pub(crate) fn is_recovery_candidate(&self, run: &AgentRun) -> bool {
        matches!(run.status.as_str(), "Interrupted" | "Recovering")
            && !self.runs().iter().any(|newer| {
                newer.id > run.id
                    && newer.task_id == run.task_id
                    && newer.project_plan == run.project_plan
            })
    }
    pub(crate) fn paused_project_goals(&self) -> Vec<(u64, Vec<u64>)> {
        let mut projects: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for task in self.tasks() {
            if self.task_is_archived(task.id)
                || task.automation_id.is_some()
                || task.stage.is_closed()
                || task.parent_id.is_some()
                || !task
                    .research
                    .as_ref()
                    .is_some_and(|r| r.kind == ResearchKind::Goal)
            {
                continue;
            }
            let interrupted = self
                .tasks()
                .iter()
                .filter(|t| t.research.as_ref().is_some_and(|r| r.goal_id == task.id))
                .any(|t| {
                    self.runs()
                        .iter()
                        .rev()
                        .find(|r| r.task_id == t.id && r.project_plan.is_none())
                        .is_some_and(|r| matches!(r.status.as_str(), "Interrupted" | "Recovering"))
                });
            if (task.stage == Stage::InProgress || interrupted)
                && !self.goal_tasks_complete(task.id)
            {
                projects.entry(task.project_id).or_default().push(task.id);
            }
        }
        projects.into_iter().collect()
    }
}
impl TaskApp {
    pub(crate) fn check_interrupted_sessions(&mut self, cx: &mut Context<Self>) {
        if self.recovery.receiver.is_some() {
            return;
        }
        let runs: Vec<_> = self
            .store
            .runs()
            .iter()
            .filter(|run| self.store.is_recovery_candidate(run))
            .cloned()
            .collect();
        if runs.is_empty() {
            self.recovery.next_check = None;
            return;
        }
        let executables: Vec<_> = runs.iter().map(|run| self.store.agent_path(&run.agent).to_owned()).collect();
        let (tx, rx) = mpsc::channel();
        for run in &runs {
            self.recovery.checks.insert(run.id, SessionCheck::Checking);
        }
        self.recovery.receiver = Some(rx);
        self.recovery.next_check = None;
        std::thread::spawn(move || {
            for (run, executable) in runs.into_iter().zip(executables) {
                let check = inspect(Path::new(&executable), &run);
                if tx.send((run.id, check)).is_err() {
                    break;
                }
            }
        });
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn recovery_blocks_run(&self, run: &AgentRun) -> bool {
        self.store
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == run.task_id && r.project_plan == run.project_plan)
            .is_some_and(|r| {
                matches!(r.status.as_str(), "Interrupted" | "Recovering")
                    && !matches!(self.recovery.checks.get(&r.id), Some(SessionCheck::Stopped))
            })
    }
    pub(crate) fn recovery_blocks_task(&self, task: u64) -> bool {
        self.store
            .runs()
            .iter()
            .rev()
            .find(|r| r.task_id == task && r.project_plan.is_none())
            .is_some_and(|r| {
                matches!(r.status.as_str(), "Interrupted" | "Recovering")
                    && !matches!(self.recovery.checks.get(&r.id), Some(SessionCheck::Stopped))
            })
    }
    pub(crate) fn recovery_pending(&self) -> bool {
        self.recovery.receiver.is_some()
            || self.recovery.next_check.is_some()
            || !self.recovery.queue.is_empty()
    }
    pub(crate) fn poll_recovery(&mut self, cx: &mut Context<Self>) {
        let mut updates = vec![];
        let mut disconnected = false;
        if let Some(rx) = &self.recovery.receiver {
            loop {
                match rx.try_recv() {
                    Ok(update) => updates.push(update),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected {
            self.recovery.receiver = None;
        }
        for (id, check) in updates {
            let current = self
                .store
                .runs()
                .iter()
                .find(|r| r.id == id)
                .is_some_and(|r| matches!(r.status.as_str(), "Interrupted" | "Recovering"));
            if !current {
                continue;
            }
            let expired_regeneration = self
                .store
                .runs()
                .iter()
                .find(|run| run.id == id)
                .is_some_and(|run| {
                    crate::automation_runtime::expired_regeneration(
                        &self.store,
                        run,
                        Self::automation_now(),
                    )
                });
            if expired_regeneration {
                if let Some(mut run) = self.store.run_mut(id) {
                    run.status = "Cancelled".into();
                    run.ended_at = Some(opencode::timestamp());
                    if !run.logs.iter().any(|line| line.contains("time limit")) {
                        run.logs.push(
                            "[automation] Recovered report regeneration exceeded its own time limit; the previous successful report is preserved."
                                .into(),
                        );
                    }
                }
                self.recovery.checks.remove(&id);
                self.persist_if_changed(true);
                cx.notify();
                continue;
            }
            if let SessionCheck::Complete(output) = &check {
                {
                    let mut run = self.store.run_mut(id).unwrap();
                    run.output = output.clone();
                    run.status = "Succeeded".into();
                    run.ended_at = Some(opencode::timestamp());
                    run.logs.push("[recovery] Imported completed session output; validating against the saved inputs.".into());
                }
                self.finalize_research_run(id, cx);
                self.publishing_scan_requested = true;
                self.persist_if_changed(true);
            }
            if !matches!(check, SessionCheck::Complete(_)) {
                let mut run = self.store.run_mut(id).unwrap();
                run.status = if check == SessionCheck::Running {
                    "Recovering"
                } else {
                    "Interrupted"
                }
                .into();
                if check == SessionCheck::Running {
                    run.ended_at = None;
                }
                let message = match &check {
                    SessionCheck::Running => format!("[recovery] Previous session is still running in {}. Monitoring without starting another run.", run.agent),
                    SessionCheck::Stopped => "[recovery] Previous session has stopped. Resume explicitly from Paused projects.".to_string(),
                    SessionCheck::Unknown(error) => format!("[recovery] Session check could not complete: {error}"),
                    _ => String::new(),
                };
                if !message.is_empty() && !run.logs.iter().any(|line| line == &message) {
                    run.logs.push(message);
                }
            }
            let automation_check = check.clone();
            self.recovery.checks.insert(id, check);
            self.apply_automation_recovery(id, &automation_check, cx);
            cx.notify();
        }
        if disconnected
            && self
                .recovery
                .checks
                .values()
                .any(|c| *c == SessionCheck::Running)
        {
            self.recovery.next_check = Some(Instant::now() + Duration::from_secs(10));
        }
        if self
            .recovery
            .next_check
            .is_some_and(|at| Instant::now() >= at)
        {
            self.check_interrupted_sessions(cx);
        }
        if !self.recovery.queue.is_empty()
            && self.active_runs.is_empty()
            && self.queued_runs.is_empty()
            && self
                .todo_batch
                .as_ref()
                .is_none_or(|b| b.state != BatchState::Running)
        {
            let (project, goals) = self.recovery.queue.remove(0);
            let goals: Vec<_> = goals
                .into_iter()
                .filter(|id| self.recoverable_goal(*id))
                .collect();
            if !goals.is_empty() {
                self.begin_goal_batch(project, goals, cx);
            }
            cx.notify();
        }
    }
    /// Session verification must advance the owning automation, not only update the
    /// research step. Unknown remains blocked for explicit review.
    fn apply_automation_recovery(
        &mut self,
        run_id: u64,
        check: &SessionCheck,
        cx: &mut Context<Self>,
    ) {
        let Some(occurrence) = self
            .store
            .runs()
            .iter()
            .find(|run| run.id == run_id)
            .and_then(|run| run.automation_occurrence_id)
        else {
            return;
        };
        let now = Self::automation_now();
        match check {
            SessionCheck::Complete(_) => {
                if let Err(error) =
                    crate::automation_runtime::confirm_complete_recovery(&mut self.store, occurrence)
                {
                    self.error = format!("Automation recovery could not advance: {error}");
                    self.store.mark_automation_attention(
                        self.store
                            .occurrence(occurrence)
                            .map(|value| value.automation_id)
                            .unwrap_or_default(),
                        self.error.clone(),
                    );
                    self.persist_if_changed(true);
                    cx.notify();
                    return;
                }
                self.automation_after_run(occurrence, run_id, cx);
            }
            SessionCheck::Running => {
                // A terminal period must never be reopened by a leftover session:
                // it already ended (for example at its total time limit) and the
                // session cannot extend it. Keep monitoring the run so the UI can
                // report that it is still shutting down, but leave the period alone.
                if self
                    .store
                    .occurrence(occurrence)
                    .is_some_and(|value| value.status.is_terminal())
                {
                    if let Some(mut run) = self.store.run_mut(run_id) {
                        let note = "[automation] The period already ended; its background session is still running and cannot extend the period.";
                        if !run.logs.iter().any(|line| line == note) {
                            run.logs.push(note.into());
                        }
                    }
                    self.persist_if_changed(true);
                    cx.notify();
                    return;
                }
                if let Some(mut value) = self.store.occurrence_mut(occurrence)
                    && value.status != crate::automation::OccurrenceStatus::Running
                {
                    let _ = value.transition(
                        crate::automation::OccurrenceStatus::Running,
                        Some(crate::automation::OccurrenceReason::new(
                            crate::automation::SkipCode::VerificationRequired,
                            "The original session is still running; monitoring without starting another run.",
                        )),
                    );
                }
                self.persist_if_changed(true);
            }
            SessionCheck::Stopped => {
                let retry = crate::automation_runtime::reconcile_occurrence(
                    &mut self.store,
                    occurrence,
                    crate::automation_runtime::SessionState::Stopped,
                    now,
                );
                if let Ok(commit) = retry {
                    self.persist_if_changed(true);
                    if self.storage_error.is_empty() {
                        for id in commit.retry {
                            self.enqueue_automation_occurrence(id, cx);
                        }
                    }
                }
            }
            SessionCheck::Checking => {}
            SessionCheck::Unknown(_) => {
                let _ = crate::automation_runtime::reconcile_occurrence(
                    &mut self.store,
                    occurrence,
                    crate::automation_runtime::SessionState::Unknown,
                    now,
                );
                self.persist_if_changed(true);
            }
        }
    }

    fn recoverable_goal(&self, goal: u64) -> bool {
        self.store
            .task(goal)
            .is_some_and(|t| !self.store.task_is_archived(t.id)
                && t.automation_id.is_none()
                && matches!(t.stage, Stage::Planned | Stage::InProgress))
            && self.store.goal_execution_approved(goal)
            && !self.store.goal_tasks_complete(goal)
            && !self.goal_busy(goal)
            && !self.store.tasks().iter().any(|t| {
                t.research.as_ref().is_some_and(|r| r.goal_id == goal)
                    && self.recovery_blocks_task(t.id)
            })
    }
    pub(crate) fn resume_projects(&mut self, project: Option<u64>, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) || self.recovery.receiver.is_some() {
            return;
        }
        if self
            .todo_batch
            .as_ref()
            .is_some_and(|b| b.state == BatchState::Running)
            || !self.recovery.queue.is_empty()
        {
            self.error = "Pause the current batch before resuming projects.".into();
            cx.notify();
            return;
        }
        self.recovery.queue = self
            .store
            .paused_project_goals()
            .into_iter()
            .filter(|(id, _)| project.is_none_or(|p| p == *id))
            .map(|(id, goals)| {
                (
                    id,
                    goals
                        .into_iter()
                        .filter(|g| self.recoverable_goal(*g))
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, goals)| !goals.is_empty())
            .collect();
        self.toast = format!(
            "{} projects queued for recovery · completed results will be reused",
            self.recovery.queue.len()
        );
        self.poll_recovery(cx);
        self.start_pump(cx);
        cx.notify();
    }
    pub(crate) fn prune_archived_recovery_queue(&mut self) {
        for (_, goals) in &mut self.recovery.queue {
            goals.retain(|id| !self.store.task_is_archived(*id));
        }
        self.recovery.queue.retain(|(_, goals)| !goals.is_empty());
    }
    pub(crate) fn stop_recovery_queue(&mut self) {
        self.recovery.queue.clear();
    }
    pub(crate) fn recovery_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut projects = self.store.paused_project_goals();
        let planning: Vec<_> = self
            .store
            .runs()
            .iter()
            .filter(|r| {
                r.project_plan.is_some()
                    && !self.store.run_is_archived(r)
                    && matches!(r.status.as_str(), "Interrupted" | "Recovering")
                    && (self.store.request_for_run(r.id).is_some()
                        || self.store.is_recovery_candidate(r))
            })
            .collect();
        for run in &planning {
            let project = run.project_plan.unwrap();
            if !projects.iter().any(|(id, _)| *id == project) {
                projects.push((project, vec![]));
            }
        }
        let checking = self.recovery.receiver.is_some();
        let can_resume = projects
            .iter()
            .any(|(_, goals)| goals.iter().any(|g| self.recoverable_goal(*g)))
            && !checking
            && self.recovery.queue.is_empty()
            && self
                .todo_batch
                .as_ref()
                .is_none_or(|b| b.state != BatchState::Running);
        let mut view = col().flex_shrink_0().p_6().gap_2()
            .child(row().justify_between().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(selectable("Paused projects")))
                .child(row().gap_2().child(button("refresh-recovery", if checking { "Checking sessions…" } else { "Check progress" }).disabled(checking)
                    .on_click(cx.listener(|a, _, _, cx| a.check_interrupted_sessions(cx))))
                    .child(primary_button("resume-all-projects", "Resume all").disabled(!can_resume)
                    .on_click(cx.listener(|a, _, _, cx| a.resume_projects(None, cx))))))
            .child(label("Previously started goals only. Background sessions are checked before resuming. Completed results are reused; approvals and dependencies still apply.", cx))
            .child(self.todo_batch_bar(None, cx));
        if projects.is_empty() {
            view = view.child(label("No paused projects", cx));
        }
        for (project, goals) in projects {
            let name = self
                .store
                .projects()
                .iter()
                .find(|p| p.id == project)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let ready = goals.iter().any(|g| self.recoverable_goal(*g));
            view = view.child(
                row()
                    .flex_wrap()
                    .gap_3()
                    .pt_3()
                    .child(
                        recovery_title(("recovery-project", project), name, cx)
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.navigate(Scope::Project(project), w, cx)
                            })),
                    )
                    .when(!goals.is_empty(), |d| {
                        d.child(
                            button(("resume-project", project), "Resume project")
                                .icon(assets::IconName::Play)
                                .disabled(!ready || !can_resume)
                                .tooltip("Resume approved, stopped goals. Running or unverified sessions cannot be restarted.")
                                .on_click(cx.listener(move |a, _, _, cx| {
                                    a.resume_projects(Some(project), cx)
                                })),
                        )
                    }),
            );
            let mut topics: BTreeMap<Option<u64>, Vec<u64>> = BTreeMap::new();
            for goal in goals {
                topics
                    .entry(self.store.task_topic(goal).map(|t| t.id))
                    .or_default()
                    .push(goal);
            }
            for (topic_id, goals) in topics {
                let title = topic_id
                    .and_then(|id| self.store.topics().iter().find(|t| t.id == id))
                    .map(|t| t.display_name())
                    .unwrap_or_else(|| "Topic unavailable".into());
                view = view.child(
                    row().pl_4().child(
                        recovery_title(
                            format!("recovery-topic-{}", topic_id.unwrap_or(0)),
                            title,
                            cx,
                        )
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .disabled(topic_id.is_none())
                        .on_click(cx.listener(move |a, _, w, cx| {
                            if let Some(id) = topic_id {
                                a.navigate(Scope::Topic(id), w, cx);
                            }
                        })),
                    ),
                );
                for goal in goals {
                    let task = self.store.task(goal).unwrap();
                    let mut status = if self.store.goal_execution_approved(goal) {
                        "Paused · ready to resume".to_string()
                    } else {
                        "Needs review · approval changed".to_string()
                    };
                    for run in self.store.runs().iter().filter(|r| {
                        self.run_goal(r.id) == Some(goal) && self.store.is_recovery_candidate(r)
                    }) {
                        match self.recovery.checks.get(&run.id) {
                            Some(SessionCheck::Unknown(error)) => {
                                status = format!("Check failed · {error}");
                                break;
                            }
                            Some(SessionCheck::Running) => {
                                status = format!("Still running in {} · monitoring", run.agent)
                            }
                            Some(SessionCheck::Checking) | None => {
                                status = "Checking previous session…".into()
                            }
                            _ => {}
                        }
                    }
                    if self
                        .active_runs
                        .iter()
                        .any(|r| self.run_goal(r.id) == Some(goal))
                        || self
                            .queued_runs
                            .iter()
                            .any(|(id, _)| self.run_goal(*id) == Some(goal))
                    {
                        status = "Running in this app".into();
                    }
                    let children = self.store.research_children(goal);
                    let mut group = col()
                        .pl_8()
                        .gap_1()
                        .pb_2()
                        .child(
                            recovery_title(
                                ("recovery-goal", goal),
                                format!("{} · {}", self.store.identifier(task), task.title),
                                cx,
                            )
                            .w_full()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .on_click(cx.listener(move |a, _, w, cx| a.open_task(goal, w, cx))),
                        )
                        .child(label(status, cx).pl_1());
                    for child in children {
                        let task = self.store.task(child).unwrap();
                        let state = self.store.execution_status(child, self.todo_batch.as_ref());
                        let title = format!("{} · {}", self.store.identifier(task), task.title);
                        let hint = self
                            .store
                            .step_block_reason(child)
                            .unwrap_or_else(|| title.clone());
                        group = group.child(
                            row()
                                .pl_4()
                                .gap_2()
                                .min_h(px(28.))
                                .child(
                                    state.icon_view(format!("recovery-step-icon-{child}"), cx),
                                )
                                .child(
                                    recovery_title(("recovery-step", child), title, cx)
                                        .text_sm()
                                        .min_w_0()
                                        .tooltip(hint)
                                        .on_click(cx.listener(move |a, _, w, cx| {
                                            a.open_task(child, w, cx)
                                        })),
                                )
                                .child(
                                    label(state.label(), cx)
                                        .flex_shrink_0()
                                        .text_color(rgb(state.color(cx))),
                                ),
                        );
                    }
                    view = view.child(group);
                }
            }
            let requests: Vec<_> = planning
                .iter()
                .filter(|r| r.project_plan == Some(project) && !self.store.run_is_automation_owned(r))
                .collect();
            if !requests.is_empty() {
                view = view.child(
                    div()
                        .pl_4()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(selectable("Planning requests · no plan generated yet")),
                );
            }
            for run in requests {
                let run_id = run.id;
                let request = self.store.request_for_run(run_id);
                let title = request
                    .map(|r| format!("Request {} · {}", r.id, r.input))
                    .unwrap_or_else(|| run.task_title.clone());
                let status = if run.status == "Recovering" {
                    "Planning in background"
                } else {
                    "Planning interrupted"
                };
                let detail = request
                    .map(|r| {
                        let date = chrono::DateTime::parse_from_rfc3339(&r.submitted_at)
                            .map(|date| date.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_else(|_| r.submitted_at.clone());
                        format!("{date} · {status}")
                    })
                    .unwrap_or_else(|| status.into());
                view = view.child(
                    col()
                        .pl_8()
                        .gap_1()
                        .pb_2()
                        .child(
                            row()
                                .gap_3()
                                .child(div().flex_1().min_w_0().whitespace_normal().child(selectable(title)))
                                .child(
                                    quiet_button(
                                        ("recovery-request", run_id),
                                        "Planning details",
                                    ).icon(assets::IconName::ArrowRight)
                                    .on_click(cx.listener(
                                        move |a, _, w, cx| {
                                            a.navigate(Scope::Runs, w, cx);
                                            a.selected_run = Some(run_id);
                                            a.show_run_result = true;
                                            a.show_run_details = false;
                                            a.reveal_selected_run();
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                        .child(label(detail, cx)),
                );
            }
        }
        col().id("recovery-scroll").size_full().min_h_0().overflow_y_scroll()
            .child(view).map(observed).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionCheck, parse_transcript, request_matches};
    use crate::domain::{Stage, Store};
    use serde_json::json;
    #[test]
    fn codex_recovery_uses_attempt_lease_even_before_session_event() {
        let mut store = Store::empty();
        store.project_at_mut(0).unwrap().directory = std::env::temp_dir().to_string_lossy().into_owned();
        store.set_default_model("test/model".into());
        let goal = store.create_research_goal("Recovery".into(), "Scope".into(), 1).unwrap();
        let mut run = store.prepare_run(goal).unwrap();
        run.agent = "Codex".into();
        run.prompt = crate::opencode::new_session_id();
        run.session_id = None;
        let request = crate::opencode::Request { agent: None, browser_executable: None,
            executable: Default::default(), directory: run.directory.clone().into(),
            model: run.model.clone(), prompt: run.prompt.clone(), title: run.task_title.clone(),
            files: vec![], session_id: None,
        };
        let lease = crate::codex::lock_attempt(&request).unwrap();
        let nonexistent = std::path::Path::new("/nonexistent/opencode-must-not-run");
        assert_eq!(super::inspect(nonexistent, &run), SessionCheck::Running);
        drop(lease);
        // Concurrent process tests can briefly inherit a lock descriptor between fork
        // and exec. Retry the release check for a bounded period (same pattern as the
        // workspace lock test) rather than treating that window as a leaked lease.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match super::inspect(nonexistent, &run) {
                SessionCheck::Stopped => break,
                other => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "Codex attempt lease was not released: {other:?}"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
        run.session_id = Some("codex-thread".into());
        assert_eq!(super::inspect(nonexistent, &run), SessionCheck::Stopped);
    }

    #[test]
    fn transcript_recovery_requires_matching_request_and_successful_end() {
        let transcript = json!({"messages":[{"type":"user","text":"old"},{"type":"assistant","content":[{"type":"text","text":"old output"}]},{"type":"idle","outcome":"succeeded"},{"type":"user","text":"current"}]});
        assert_eq!(
            parse_transcript(&transcript, "current").unwrap(),
            SessionCheck::Stopped
        );
        assert!(parse_transcript(&transcript, "old").is_err());
        assert!(parse_transcript(&transcript, "missing").is_err());
        let mut complete = transcript.clone();
        complete["messages"].as_array_mut().unwrap().extend([
            json!({"type":"assistant","content":[{"type":"text","text":"new output"}]}),
            json!({"type":"idle","outcome":"succeeded"}),
        ]);
        assert_eq!(
            parse_transcript(&complete, "current").unwrap(),
            SessionCheck::Complete("new output".into())
        );
        complete["messages"][5]["outcome"] = json!("interrupted");
        assert_eq!(
            parse_transcript(&complete, "current").unwrap(),
            SessionCheck::Stopped
        );
    }
    #[test]
    fn opencode_quoted_duplicate_request_is_recovered_without_accepting_other_requests() {
        let prompt = "Research\nSources: [{\"title\":\"Example\"}]\nReturn a report.";
        let quoted = format!("\"{}\"", prompt.replace('"', "\\\""));
        for text in [
            prompt.to_string(),
            quoted.clone(),
            format!("{quoted} {quoted}"),
        ] {
            assert!(request_matches(&text, prompt));
            let transcript = json!({"messages":[{"type":"user","text":text},{"type":"assistant","content":[{"type":"text","text":"completed report"}]},{"type":"idle","outcome":"succeeded"}]});
            assert_eq!(
                parse_transcript(&transcript, prompt).unwrap(),
                SessionCheck::Complete("completed report".into())
            );
        }
        assert!(!request_matches(&format!("prefix {prompt}"), prompt));
        assert!(!request_matches(&format!("{quoted} changed"), prompt));
    }
    #[test]
    fn recovery_excludes_unstarted_todo_archived_and_cancelled_goals() {
        let mut s = Store::empty();
        let a = s
            .create_research_goal("Started".into(), "Research".into(), 1)
            .unwrap();
        let b = s
            .create_research_goal("Todo only".into(), "Research".into(), 1)
            .unwrap();
        s.task_mut(a).unwrap().stage = Stage::InProgress;
        s.task_mut(b).unwrap().stage = Stage::Planned;
        assert_eq!(s.paused_project_goals(), vec![(1, vec![a])]);
        let topic = s.task_topic(a).unwrap().id;
        s.set_topic_archived(topic, true).unwrap();
        assert!(s.paused_project_goals().is_empty());
        assert!(s.task_is_archived(a));
        assert!(!s.task(a).unwrap().archived);
        assert!(!s.todo_batch_items(1).contains(&a));
        assert!(s.ready_research_tasks(a).is_err());
        assert!(s.prepare_run(a).is_err());
        s.set_topic_archived(topic, false).unwrap();
        assert_eq!(s.paused_project_goals(), vec![(1, vec![a])]);
        s.task_mut(a).unwrap().archived = true;
        assert!(s.paused_project_goals().is_empty());
        s.task_mut(a).unwrap().archived = false;
        s.task_mut(a).unwrap().stage = Stage::Cancelled;
        assert!(s.paused_project_goals().is_empty());
    }
}
