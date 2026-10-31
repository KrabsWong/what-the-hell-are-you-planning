use crate::{
    app::{Scope, TaskApp},
    batch::{BatchState, TodoBatch},
    controls::*,
    research::ResearchKind,
    views::{col, label, row},
};
use gpui_kit::{component::Disableable, prelude::FluentBuilder, *};

impl TaskApp {
    pub fn start_todo_batch(&mut self, project: u64, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) {
            return;
        }
        let items = self.store.todo_batch_items(project);
        if items.is_empty() {
            self.toast = "No pending Todo tasks in this project".into();
            cx.notify();
            return;
        }
        self.begin_goal_batch(project, items, cx);
    }

    pub fn start_goal_batch(&mut self, goal: u64, cx: &mut Context<Self>) {
        if self.store.task_is_archived(goal) {
            self.error = "Restore the archived topic or goal before executing.".into();
            self.task_start_feedback = Some((goal, self.error.clone()));
            cx.notify();
            return;
        }
        if self.todo_batch.as_ref().is_some_and(|b|
            b.contains_pending(goal) && matches!(b.state, BatchState::Paused(_))) {
            self.resume_todo_batch(cx);
            return;
        }
        if !self.ready_to_run(cx) {
            return;
        }
        let Some(task) = self.store.task(goal) else {
            return;
        };
        let project = task.project_id;
        if let Err(error) = self.store.ready_research_tasks(goal) {
            self.error = error.clone();
            self.task_start_feedback = Some((goal, error));
            cx.notify();
            return;
        }
        self.begin_goal_batch(project, vec![goal], cx);
    }

    pub(crate) fn begin_goal_batch(
        &mut self,
        project: u64,
        items: Vec<u64>,
        cx: &mut Context<Self>,
    ) {
        // Appending is an explicit Run action; approval alone never queues work.
        for id in &items {
            if self.store.task_is_archived(*id) || self.store.task(*id).is_none() {
                self.error = "Restore the selected research before queueing it.".into();
                cx.notify(); return;
            }
            if self.store.task(*id).unwrap().research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal)
                && !self.store.goal_execution_approved(*id) {
                self.error = "Review the goal and move it to Todo before queueing it.".into();
                cx.notify(); return;
            }
        }
        if self.todo_batch.as_ref().is_none_or(|b| b.state == BatchState::Finished) {
            self.todo_batch = Some(TodoBatch {
                project_id: project, items: vec![], completed: 0, runs_completed: 0,
                state: BatchState::Running, run_ids: Default::default(),
                done_items: vec![], failed_items: vec![], failed_tasks: vec![],
            });
        }
        let batch = self.todo_batch.as_mut().unwrap();
        let mut added = 0;
        for id in items {
            if batch.contains_pending(id) { continue; }
            if batch.done_items.contains(&id) && self.store.goal_tasks_complete(id) { continue; }
            let topic = self.store.execution_topic(id);
            if !batch.pending_topics(&self.store).contains(&topic) {
                // Re-submitting a finished/failed topic appends it behind pending topics.
                let previous: Vec<_> = batch.items.iter().copied().filter(|item| self.store.execution_topic(*item) == topic).collect();
                batch.items.retain(|item| !previous.contains(item));
                batch.items.extend(previous);
            }
            batch.items.retain(|item| *item != id);
            batch.done_items.retain(|item| *item != id);
            batch.failed_items.retain(|item| *item != id);
            batch.failed_tasks.retain(|task| *task != id && self.store.owning_research_goal(*task) != Some(id));
            batch.items.push(id);
            added += 1;
        }
        batch.completed = batch.done_items.len();
        self.error.clear();
        self.advance_todo_batch(cx);
        self.toast = if added > 0 { format!("Added {added} goals to the topic queue") }
            else { "Selected goals are already queued or completed".into() };
        self.start_pump(cx);
        cx.notify();
    }

    pub fn start_topic_batch(&mut self, topic: u64, cx: &mut Context<Self>) {
        let Some(project) = self.store.topics.iter().find(|t| t.id == topic && !t.archived).map(|t| t.project_id) else { return; };
        if !self.ready_to_run(cx) { return; }
        let items = self.store.todo_batch_items(project).into_iter()
            .filter(|id| self.store.task_topic(*id).is_some_and(|t| t.id == topic)).collect::<Vec<_>>();
        if items.is_empty() { self.toast = "No approved unfinished goals in this topic".into(); cx.notify(); return; }
        self.begin_goal_batch(project, items, cx);
    }

    pub fn pause_todo_batch(&mut self, cx: &mut Context<Self>) {
        self.stop_recovery_queue();
        if let Some(batch) = &mut self.todo_batch {
            if batch.state == BatchState::Running {
                batch.state = BatchState::Paused(
                    "Paused. Already submitted tasks will finish; no new steps will start.".into(),
                );
            }
        }
        cx.notify();
    }
    pub fn resume_todo_batch(&mut self, cx: &mut Context<Self>) {
        self.persist_if_changed(true);
        if !self.ready_to_run(cx) {
            return;
        }
        // Settle already completed work after durable saving, without executing it again.
        let completed: Vec<_> = self
            .todo_batch
            .as_ref()
            .map(|b| {
                b.run_ids
                    .keys()
                    .copied()
                    .filter(|id| {
                        self.store.runs.iter().any(|r| {
                            r.id == *id
                                && !matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut invalidated = false;
        for id in completed {
            invalidated |= self
                .store
                .runs
                .iter()
                .find(|r| r.id == id)
                .is_some_and(|r| {
                    r.status == "Succeeded"
                        && r.research_kind.is_some_and(|k| k != ResearchKind::Goal)
                        && self.store.research_result(r.task_id).is_none()
                });
            self.complete_todo_batch_run(id, cx);
        }
        if invalidated {
            if let Some(batch) = &mut self.todo_batch {
                batch.state = BatchState::Paused("Research inputs changed while saving. Review the task, then continue to rerun.".into());
            }
            cx.notify();
            return;
        }
        if let Some(batch) = &mut self.todo_batch {
            if !matches!(batch.state, BatchState::Paused(_)) {
                return;
            }
            batch.state = BatchState::Running;
        }
        self.advance_todo_batch(cx);
    }
    pub fn cancel_todo_batch(&mut self, cx: &mut Context<Self>) {
        self.stop_recovery_queue();
        // Detach first: cancellation callbacks must not schedule replacement work.
        if let Some(batch) = self.todo_batch.take() {
            for id in batch.run_ids.keys() {
                self.cancel_run_id(*id, cx);
            }
        }
        self.toast = "Batch stopped · saved results kept".into();
        cx.notify();
    }
    pub(crate) fn advance_todo_batch(&mut self, cx: &mut Context<Self>) {
        if self.todo_batch.as_ref().is_none_or(|b| b.state != BatchState::Running) { return; }
        if !self.storage_error.is_empty() {
            self.todo_batch.as_mut().unwrap().state = BatchState::Paused("Save failed. Restore local storage before continuing the topic queue.".into());
            cx.notify(); return;
        }
        loop {
            let items = self.todo_batch.as_ref().unwrap().current_items(&self.store);
            if items.is_empty() { break; }
            for item in &items {
                let item = *item;
                let Some(task) = self.store.task(item).filter(|task|
                    !self.store.task_is_archived(task.id) && matches!(task.stage,
                        crate::domain::Stage::Planned | crate::domain::Stage::InProgress | crate::domain::Stage::Done)) else {
                    self.todo_batch.as_mut().unwrap().failed_items.push(item); continue;
                };
                let project = task.project_id;
                let research_goal = task.research.as_ref().is_some_and(|r| r.kind == ResearchKind::Goal);
                let next = if research_goal { self.store.ready_research_tasks(item) }
                    else { self.store.next_todo_batch_task(project, item).map(|id| id.into_iter().collect()) };
                let candidates = match next {
                    Ok(ids) => ids,
                    Err(error) => {
                        self.error = error;
                        self.todo_batch.as_mut().unwrap().failed_items.push(item); continue;
                    }
                };
                let finished = if research_goal { self.store.goal_tasks_complete(item) } else { candidates.is_empty() };
                if finished {
                    if research_goal { self.store.sync_goal_progress(item); }
                    let batch = self.todo_batch.as_mut().unwrap();
                    batch.done_items.push(item);
                    batch.completed = batch.done_items.len();
                    continue;
                }
                if research_goal && self.store.task(item).unwrap().stage == crate::domain::Stage::Planned {
                    if let Err(error) = self.store.set_stage(item, crate::domain::Stage::InProgress) {
                        self.error = error;
                        self.todo_batch.as_mut().unwrap().failed_items.push(item); continue;
                    }
                }
                for id in candidates {
                    let batch = self.todo_batch.as_ref().unwrap();
                    // Multiple workers can finish in one poll. Their terminal events
                    // must all be finalized before a new attempt is admitted.
                    if self.task_running(id) || batch.failed_tasks.contains(&id)
                        || batch.run_ids.keys().any(|run_id| self.store.runs.iter().any(|run| run.id == *run_id && run.task_id == id))
                    { continue; }
                    let before = self.store.runs.len();
                    self.start_run(id, cx);
                    let submitted = self.store.runs.get(before).map(|r| (r.id, r.status.clone()));
                    let batch = self.todo_batch.as_mut().unwrap();
                    if let Some((run_id, _)) = submitted.filter(|(_, status)| matches!(status.as_str(), "Queued" | "Running")) {
                        batch.run_ids.insert(run_id, item);
                    } else {
                        // No attempt was created, so this is not an exhausted task.
                        // Preserve the topic and the reason instead of silently skipping it.
                        let reason = if self.error.is_empty() {
                            "The task could not be queued. Review this task and continue.".to_string()
                        } else { self.error.clone() };
                        batch.state = BatchState::Paused(format!("Could not start task {id}: {reason}"));
                        self.task_start_feedback = Some((id, reason.clone()));
                        if let Some(goal) = self.store.task_mut(item) {
                            goal.activity.push(crate::domain::activity_event(&format!("Topic queue paused at task {id}: {reason}")));
                        }
                        self.persist_if_changed(true);
                        cx.notify();
                        return;
                    }
                }
            }
            // Keep the entire topic active through dependency stages and retry delays.
            let busy = self.todo_batch.as_ref().unwrap().run_ids.values().any(|item| items.contains(item))
                || items.iter().any(|item| self.goal_busy(*item) || self.task_running(*item));
            if busy { break; }
            let has_failed_step = self.todo_batch.as_ref().unwrap().failed_tasks.iter().any(|id|
                items.contains(id) || self.store.owning_research_goal(*id).is_some_and(|goal| items.contains(&goal)));
            if !has_failed_step && items.iter().any(|id| self.todo_batch.as_ref().unwrap().contains_pending(*id)) {
                self.todo_batch.as_mut().unwrap().state = BatchState::Paused(
                    "This topic is unfinished but has no runnable steps. Review its dependencies, then continue.".into());
                cx.notify();
                return;
            }
            let batch = self.todo_batch.as_mut().unwrap();
            for item in items {
                if !batch.done_items.contains(&item) && !batch.failed_items.contains(&item) {
                    batch.failed_items.push(item);
                }
            }
            // Exhausted or blocked goals remain failed; independent later topics continue.
        }
        if let Some(batch) = &mut self.todo_batch {
            if batch.pending_topics(&self.store).is_empty() && batch.run_ids.is_empty() {
                batch.state = BatchState::Finished;
                self.toast = if batch.failed_items.is_empty() { "Research completed · final reports saved locally".into() }
                    else { format!("Queue finished · {} goals completed · {} need attention", batch.completed, batch.failed_items.len()) };
            }
        }
        cx.notify();
    }
    pub(crate) fn complete_todo_batch_run(&mut self, run_id: u64, cx: &mut Context<Self>) -> bool {
        if !self.storage_error.is_empty() {
            if let Some(batch) = &mut self.todo_batch {
                batch.state = BatchState::Paused("Save failed. Restore local storage, then continue; completed work will not be repeated.".into());
                return batch.run_ids.contains_key(&run_id);
            }
        }
        let item = self
            .todo_batch
            .as_mut()
            .and_then(|b| b.run_ids.remove(&run_id));
        let Some(item) = item else {
            // Completion of manually started work may release a batch dependency/resource.
            self.advance_todo_batch(cx);
            return false;
        };
        let run = self.store.runs.iter().find(|r| r.id == run_id).unwrap();
        let successful = run.status == "Succeeded"
            && self.storage_error.is_empty()
            && (run.research_kind.is_none_or(|k| k == ResearchKind::Goal)
                || self.store.research_result(run.task_id).is_some());
        let batch = self.todo_batch.as_mut().unwrap();
        if successful {
            batch.runs_completed += 1;
            let goal = self
                .store
                .task(item)
                .and_then(|t| t.research.as_ref())
                .is_some_and(|r| r.kind == ResearchKind::Goal);
            if !goal || self.store.next_todo_batch_task(self.store.task(item).map(|t| t.project_id).unwrap_or(batch.project_id), item) == Ok(None) {
                if !batch.done_items.contains(&item) {
                    batch.done_items.push(item);
                }
                batch.completed = batch.done_items.len();
            }
        } else {
            batch.failed_tasks.push(run.task_id);
        }
        self.advance_todo_batch(cx);
        true
    }

    pub fn todo_batch_action(&self, project: u64, cx: &mut Context<Self>) -> AnyElement {
        let count = self.store.todo_batch_items(project).into_iter().filter(|id|
            self.todo_batch.as_ref().is_none_or(|b| !b.contains_pending(*id))).count();
        let busy = self
            .todo_batch
            .as_ref()
            .is_some_and(|b| b.state != BatchState::Finished);
        primary_button("run-project-todo", format!("{} ({count})", if busy { "Add to queue" } else { "Run / resume" }))
            .icon(assets::IconName::Play)
            .disabled(count == 0 || !self.storage_error.is_empty())
            .tooltip("Queue approved goals. Topics run in order, with up to three concurrent steps within the current topic.")
            .on_click(cx.listener(move |this, _, _, cx| this.start_todo_batch(project, cx)))
            .into_any_element()
    }

    pub fn todo_batch_bar(&self, project: Option<u64>, cx: &mut Context<Self>) -> AnyElement {
        let Some(batch) = self
            .todo_batch
            .as_ref()
            .filter(|b| project.is_none_or(|p| b.items.iter().any(|id| self.store.task(*id).is_some_and(|t| t.project_id == p))))
        else {
            return div().into_any_element();
        };
        let colors = crate::appearance::palette(cx);
        let project_name = "Topic queue";
        let state = match batch.state {
            BatchState::Running => "Running",
            BatchState::Paused(_) => "Paused",
            BatchState::Finished if !batch.failed_items.is_empty() => "Finished with failures",
            BatchState::Finished => "Completed",
        };
        let mut actions = row().gap_1().flex_shrink_0();
        match batch.state {
            BatchState::Running => {
                actions = actions.child(
                    quiet_button("pause-todo-batch", "Pause after task")
                        .icon(assets::IconName::Pause)
                        .on_click(cx.listener(|this, _, _, cx| this.pause_todo_batch(cx))),
                )
            }
            BatchState::Paused(_) => {
                actions = actions.child(
                    primary_button("resume-todo-batch", "Continue")
                        .icon(assets::IconName::Play)
                        .on_click(cx.listener(|this, _, _, cx| this.resume_todo_batch(cx))),
                )
            }
            BatchState::Finished => {}
        }
        if project.is_some() {
            actions = actions.child(
                quiet_button("view-todo-batch", "Progress")
                    .icon(assets::IconName::ArrowRight)
                    .on_click(cx.listener(|this, _, w, cx| this.navigate(Scope::Runs, w, cx))),
            );
        }
        actions = actions.child(
            if batch.state == BatchState::Finished {
                tool_icon(
                    "clear-todo-batch",
                    assets::IconName::Close,
                    "Dismiss completed batch",
                )
            } else {
                quiet_button("clear-todo-batch", "Stop all").text_color(rgb(colors.danger))
            }
            .on_click(cx.listener(|this, _, _, cx| this.cancel_todo_batch(cx))),
        );
        col()
            .id("todo-batch-status")
            .px_6()
            .py_2()
            .gap_1()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(colors.line))
            .child(
                row()
                    .gap_3()
                    .justify_between()
                    .child(
                        label(
                            format!(
                                "{project_name} · {state} · {}/{} complete",
                                batch.completed,
                                batch.items.len()
                            ),
                            cx,
                        )
                        .flex_1()
                        .min_w_0(),
                    )
                    .child(actions),
            )
            .when(matches!(batch.state, BatchState::Paused(_)), |d| {
                if let BatchState::Paused(reason) = &batch.state {
                    d.child(label(reason.clone(), cx).text_color(rgb(colors.warning)))
                } else {
                    d
                }
            })
            .children(batch.pending_topics(&self.store).into_iter().take(5).enumerate().map(|(position, topic)| {
                let name = match topic {
                    crate::batch::ExecutionTopic::Topic(id) => self.store.topics.iter().find(|t| t.id == id).map(|t| t.display_name()).unwrap_or_else(|| "Missing topic".into()),
                    crate::batch::ExecutionTopic::Task(id) => self.store.task(id).map(|t| t.title.clone()).unwrap_or_else(|| "Missing goal".into()),
                };
                label(format!("{} · {} · {}", position + 1, if position == 0 { "Current" } else { "Queued" }, name), cx)
            }))
            .when(batch.pending_topics(&self.store).len() > 5, |d| d.child(label(format!("{} more topics queued", batch.pending_topics(&self.store).len() - 5), cx)))
            .when(!batch.failed_items.is_empty(), |d| d.child(label(format!("{} goals need attention; later topics continue automatically.", batch.failed_items.len()), cx).text_color(rgb(colors.warning))))
            .into_any_element()
    }
}
