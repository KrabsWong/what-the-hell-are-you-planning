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
        if self
            .todo_batch
            .as_ref()
            .is_some_and(|b| b.state != BatchState::Finished)
        {
            self.error = "Resume or cancel the existing Todo batch first.".into();
            cx.notify();
            return;
        }
        if !self.ready_to_run(cx) {
            return;
        }
        let items = self.store.todo_batch_items(project);
        if items.is_empty() {
            self.toast = "No pending Todo tasks in this project".into();
            cx.notify();
            return;
        }
        self.todo_batch = Some(TodoBatch {
            project_id: project,
            items,
            completed: 0,
            runs_completed: 0,
            state: BatchState::Running,
            run_ids: Default::default(),
            done_items: vec![],
            failed_items: vec![],
        });
        self.advance_todo_batch(cx);
    }
    pub fn pause_todo_batch(&mut self, cx: &mut Context<Self>) {
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
            batch.failed_items.clear();
            batch.state = BatchState::Running;
        }
        self.advance_todo_batch(cx);
    }
    pub fn cancel_todo_batch(&mut self, cx: &mut Context<Self>) {
        // Detach first: cancellation callbacks must not schedule replacement work.
        if let Some(batch) = self.todo_batch.take() {
            for id in batch.run_ids.keys() {
                self.cancel_run_id(*id, cx);
            }
        }
        self.toast = "Batch stopped · saved results kept".into();
        cx.notify();
    }
    fn advance_todo_batch(&mut self, cx: &mut Context<Self>) {
        let Some(batch) = &self.todo_batch else {
            return;
        };
        if batch.state != BatchState::Running {
            return;
        }
        let project = batch.project_id;
        let items = batch.items.clone();
        for item in items {
            let Some(batch) = &self.todo_batch else {
                break;
            };
            if batch.done_items.contains(&item)
                || batch.failed_items.contains(&item)
                || batch.run_ids.values().any(|id| *id == item)
            {
                continue;
            }
            // A manually running goal owns its workspace until that step finishes.
            if self.goal_busy(item) || self.task_running(item) {
                continue;
            }
            match self.store.next_todo_batch_task(project, item) {
                Ok(Some(id)) => {
                    if self.task_running(id) {
                        continue;
                    }
                    // Dependencies that are still running are waiting, not failed.
                    if self
                        .store
                        .task(id)
                        .and_then(|t| t.research.as_ref())
                        .is_some_and(|r| {
                            r.depends_on
                                .iter()
                                .any(|dep| self.store.research_result(*dep).is_none())
                        })
                    {
                        continue;
                    }
                    let before = self.store.runs.len();
                    self.start_run(id, cx);
                    let submitted = self
                        .store
                        .runs
                        .get(before)
                        .map(|r| (r.id, r.status.clone()));
                    let batch = self.todo_batch.as_mut().unwrap();
                    if let Some((run_id, status)) = submitted {
                        if matches!(status.as_str(), "Queued" | "Running") {
                            batch.run_ids.insert(run_id, item);
                        } else {
                            batch.failed_items.push(item);
                        }
                    } else {
                        batch.failed_items.push(item);
                    }
                }
                Ok(None) => {
                    let batch = self.todo_batch.as_mut().unwrap();
                    batch.done_items.push(item);
                    batch.completed = batch.done_items.len();
                }
                Err(error) => {
                    self.error = error;
                    self.todo_batch.as_mut().unwrap().failed_items.push(item);
                }
            }
        }
        if let Some(batch) = &mut self.todo_batch {
            if batch.done_items.len() == batch.items.len() {
                batch.state = BatchState::Finished;
                self.toast = "Todo batch completed · results saved locally".into();
            } else if batch.run_ids.is_empty()
                && self.active_runs.is_empty()
                && self.queued_runs.is_empty()
            {
                batch.state = BatchState::Paused(format!(
                    "{} tasks failed or dependencies are not ready. Check their results, then continue to retry.",
                    batch.items.len() - batch.done_items.len()
                ));
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
            if !goal || self.store.next_todo_batch_task(batch.project_id, item) == Ok(None) {
                batch.done_items.push(item);
                batch.completed = batch.done_items.len();
            }
        } else {
            batch.failed_items.push(item);
        }
        self.advance_todo_batch(cx);
        true
    }

    pub fn todo_batch_action(&self, project: u64, cx: &mut Context<Self>) -> AnyElement {
        let count = self.store.todo_batch_items(project).len();
        let busy = self
            .todo_batch
            .as_ref()
            .is_some_and(|b| b.state != BatchState::Finished);
        primary_button("run-project-todo", format!("Run Todo ({count})"))
            .icon(assets::IconName::Play)
            .disabled(count == 0 || busy || !self.storage_error.is_empty())
            .tooltip("Run all pending Todo in this project. Completed research is reused; search filters do not change the scope.")
            .on_click(cx.listener(move |this, _, _, cx| this.start_todo_batch(project, cx)))
            .into_any_element()
    }

    pub fn todo_batch_bar(&self, project: Option<u64>, cx: &mut Context<Self>) -> AnyElement {
        let Some(batch) = self
            .todo_batch
            .as_ref()
            .filter(|b| project.is_none_or(|p| p == b.project_id))
        else {
            return div().into_any_element();
        };
        let colors = crate::appearance::palette(cx);
        let project_name = self
            .store
            .projects
            .iter()
            .find(|p| p.id == batch.project_id)
            .map(|p| p.name.as_str())
            .unwrap_or("Project");
        let state = match batch.state {
            BatchState::Running => "Running",
            BatchState::Paused(_) => "Paused",
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
            .into_any_element()
    }
}
