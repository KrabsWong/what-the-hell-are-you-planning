//! Archiving closes execution admission before cancelling associated work.
use crate::{app::TaskApp, domain::Store, opencode};
use gpui_kit::*;

#[derive(Clone, Copy)]
pub(crate) enum ArchiveTarget {
    Topic(u64),
    Task(u64),
}
impl ArchiveTarget {
    fn contains(self, store: &Store, id: u64) -> bool {
        match self {
            Self::Topic(topic) => store.task_topic(id).is_some_and(|t| t.id == topic),
            Self::Task(root) => {
                let mut current = Some(id);
                let mut seen = std::collections::HashSet::new();
                while let Some(id) = current {
                    if id == root { return true; }
                    if !seen.insert(id) { break; }
                    current = store.task(id).and_then(|t| t.parent_id.or_else(||
                        t.research.as_ref().map(|r| r.goal_id).filter(|g| *g != id)));
                }
                false
            }
        }
    }
    fn runs(self, store: &Store) -> Vec<u64> {
        store.runs.iter().filter(|r| {
            let related = if r.project_plan.is_some() {
                matches!(self, Self::Topic(topic) if store.request_for_run(r.id).is_some_and(|q| q.topic_id == Some(topic)))
            } else { self.contains(store, r.task_id) };
            related && (matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling" | "Recovering")
                || store.is_recovery_candidate(r))
        }).map(|r| r.id).collect()
    }
}
impl TaskApp {
    pub(crate) fn request_archive(&mut self, target: ArchiveTarget, w: &mut Window, cx: &mut Context<Self>) {
        let runs = target.runs(&self.store);
        if runs.is_empty() {
            self.confirm_archive(target, cx);
            return;
        }
        if w.has_active_prompt() { return; }
        let name = match target {
            ArchiveTarget::Topic(id) => self.store.topics.iter().find(|t| t.id == id).map(|t| t.display_name()),
            ArchiveTarget::Task(id) => self.store.task(id).map(|t| t.title.clone()),
        }.unwrap_or_default();
        let answer = w.prompt(PromptLevel::Warning, "Stop related work and archive?",
            Some(&format!("{name}\n{} running, queued or interrupted attempts will be stopped. Pending steps will not start. Saved results will be retained.", runs.len())),
            &["Cancel", "Stop and archive"], cx);
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |a, cx| a.confirm_archive(target, cx));
            }
        }).detach();
    }
    pub(crate) fn confirm_archive(&mut self, target: ArchiveTarget, cx: &mut Context<Self>) {
        // Re-evaluate after confirmation: execution may have advanced while the dialog was open.
        let runs = target.runs(&self.store);
        self.snapshot();
        let result = match target {
            ArchiveTarget::Topic(id) => self.store.set_topic_archived(id, true),
            ArchiveTarget::Task(id) => self.store.archive(id),
        };
        if let Err(error) = result { self.error = error; cx.notify(); return; }
        self.research_sequences.retain(|g| !target.contains(&self.store, *g));
        if let Some(batch) = &mut self.todo_batch {
            batch.items.retain(|g| !target.contains(&self.store, *g));
            batch.run_ids.retain(|id, _| !runs.contains(id));
            batch.done_items.retain(|g| batch.items.contains(g));
            batch.failed_items.retain(|g| batch.items.contains(g));
            batch.failed_tasks.retain(|id| !target.contains(&self.store, *id));
            batch.completed = batch.done_items.len();
            if batch.items.is_empty() { batch.state = crate::batch::BatchState::Finished; }
        }
        self.prune_archived_recovery_queue();
        // Remove all queued entries before callbacks can admit a dependent step.
        self.queued_runs.retain(|(id, _)| !runs.contains(id));
        let mut remote = vec![];
        for id in runs {
            let owned = self.active_runs.iter().any(|a| a.id == id);
            if owned { self.cancel_run_id(id, cx); }
            let run = self.store.runs.iter_mut().find(|r| r.id == id).unwrap();
            if !run.started_at.is_empty() && run.session_id.is_some() { remote.push(run.clone()); }
            run.status = if owned || remote.last().is_some_and(|r| r.id == id) { "Cancelling" } else { "Cancelled" }.into();
            if run.status == "Cancelled" { run.ended_at = Some(opencode::timestamp()); }
            run.logs.push("[archive] Stop requested; queued and subsequent work disabled.".into());
        }
        self.finish(Ok(()), "Archived · related work stopped or stopping; results retained", cx);
        if remote.is_empty() { return; }
        let executable = std::path::PathBuf::from(&self.store.opencode_path);
        let work = cx.background_executor().spawn(async move {
            remote.into_iter().map(|run| {
                let result = opencode::interrupt_session(&executable, &run);
                (run.id, result)
            }).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = work.await;
            let _ = this.update(cx, |a, cx| {
                for (id, result) in results {
                    let owned = a.active_runs.iter().any(|r| r.id == id);
                    if let Some(run) = a.store.runs.iter_mut().find(|r| r.id == id) {
                        match result {
                            Ok(()) => {
                                if !owned { run.status = "Cancelled".into(); run.ended_at = Some(opencode::timestamp()); }
                                run.logs.push("[archive] Background session interrupted.".into());
                            }
                            Err(error) => {
                                run.logs.push(format!("[archive] Could not confirm session stopped: {error}"));
                                a.error = format!("Archived, but run {id}'s background session could not be stopped: {error}. Check its technical details.");
                            }
                        }
                    }
                }
                a.persist_if_changed(true);
                cx.notify();
            });
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::ArchiveTarget;
    use crate::domain::Store;
    #[test]
    fn archive_scope_includes_descendants_and_planning_but_not_other_topics() {
        let mut store = Store::empty();
        let goal = store.create_research_goal("Goal".into(), "Brief".into(), 1).unwrap();
        let topic = store.task_topic(goal).unwrap().id;
        let child = store.create_task("Child", 1, crate::domain::Stage::Planned, Some(goal)).unwrap();
        let nested = store.create_task("Nested", 1, crate::domain::Stage::Planned, Some(child)).unwrap();
        let other = store.create_research_goal("Other".into(), "Brief".into(), 1).unwrap();
        assert!(ArchiveTarget::Task(goal).contains(&store, nested));
        assert!(ArchiveTarget::Topic(topic).contains(&store, nested));
        assert!(!ArchiveTarget::Topic(topic).contains(&store, other));
        let request = store.create_research_request(1, Some(topic), "Research this topic").unwrap();
        store.set_topic_archived(topic, true).unwrap();
        assert!(store.prepare_request_plan(request).unwrap_err().contains("archived"));
        assert!(store.prepare_run(nested).is_err());
        assert!(store.ready_research_tasks(goal).is_err());
        store.set_topic_archived(topic, false).unwrap();
        assert!(store.goal_execution_approved(goal));
    }
}
