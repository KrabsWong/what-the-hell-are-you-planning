//! Confirmed, local-only deletion of archived research and its saved history.
use crate::{archive_app::ArchiveTarget, domain::Store};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArchiveDeletion {
    pub name: String,
    pub topic: Option<u64>,
    /// Archived topics left with no tasks at all once this goal deletion runs.
    pub empty_topics: BTreeSet<u64>,
    pub tasks: BTreeSet<u64>,
    pub requests: BTreeSet<u64>,
    pub runs: BTreeSet<u64>,
    pub publication_topics: BTreeSet<u64>,
}

impl Store {
    pub(crate) fn archive_deletion(&self, target: ArchiveTarget) -> Result<ArchiveDeletion, String> {
        let (name, topic, mut tasks) = match target {
            ArchiveTarget::Topic(id) => {
                let topic = self.topic(id).ok_or("Topic no longer exists.")?;
                if !topic.archived || topic.automation_id.is_some() {
                    return Err("Only archived research topics can be permanently deleted.".into());
                }
                (topic.display_name(), Some(id), self.tasks().iter()
                    .filter(|task| self.task_topic(task.id).is_some_and(|topic| topic.id == id))
                    .map(|task| task.id).collect::<BTreeSet<_>>())
            }
            ArchiveTarget::Task(id) => {
                let task = self.task(id).ok_or("Task no longer exists.")?;
                if !self.task_is_archived(id) || task.automation_id.is_some() {
                    return Err("Only archived research can be permanently deleted.".into());
                }
                if task.research.as_ref().is_some_and(|research| research.goal_id != id) {
                    return Err("Delete the archived goal to remove its research steps together.".into());
                }
                (task.title.clone(), None, BTreeSet::from([id]))
            }
        };
        loop {
            let before = tasks.len();
            for task in self.tasks() {
                if task.parent_id.is_some_and(|id| tasks.contains(&id))
                    || task.research.as_ref().is_some_and(|research| tasks.contains(&research.goal_id))
                {
                    tasks.insert(task.id);
                }
            }
            if tasks.len() == before { break; }
        }
        if self.tasks().iter().any(|task| tasks.contains(&task.id) && task.automation_id.is_some()) {
            return Err("Automation-owned history cannot be deleted from Research Archive.".into());
        }
        if tasks.iter().any(|id| !self.task_is_archived(*id)) {
            return Err("All related tasks must remain archived before deletion.".into());
        }
        if self.tasks().iter().any(|task| !tasks.contains(&task.id)
            && task.research.as_ref().is_some_and(|research| research.depends_on.iter().any(|id| tasks.contains(id))))
        {
            return Err("Other research still depends on this archived research.".into());
        }
        // Deleting the last goal of an archived topic removes the empty topic.
        // A topic that still owns any other task stays, even when none of those
        // tasks remain in the Archive list.
        let mut empty_topics = BTreeSet::new();
        if topic.is_none() {
            let candidates: BTreeSet<u64> = tasks.iter()
                .filter_map(|id| self.task_topic(*id).map(|topic| topic.id)).collect();
            for candidate in candidates {
                let Some(model) = self.topic(candidate) else { continue };
                if !model.archived || model.automation_id.is_some() { continue; }
                let survives = self.tasks().iter().any(|task| !tasks.contains(&task.id)
                    && self.task_topic(task.id).is_some_and(|topic| topic.id == candidate));
                if !survives { empty_topics.insert(candidate); }
            }
        }
        let mut requests: BTreeSet<_> = self.research_requests().iter().filter(|request|
            topic.is_some_and(|id| request.topic_id == Some(id))
                || (!request.goal_ids.is_empty() && request.goal_ids.iter().all(|id| tasks.contains(id))))
            .map(|request| request.id).collect();
        for id in &empty_topics {
            requests.extend(self.research_requests().iter()
                .filter(|request| request.topic_id == Some(*id)).map(|request| request.id));
        }
        if self.research_requests().iter().any(|request| requests.contains(&request.id)
            && (request.automation_id.is_some() || request.goal_ids.iter().any(|id| !tasks.contains(id))))
        {
            return Err("The archived research shares a planning request with other research.".into());
        }
        let runs: BTreeSet<_> = self.runs().iter().filter(|run|
            tasks.contains(&run.task_id)
                || self.request_for_run_record(run).is_some_and(|request| requests.contains(&request.id)))
            .map(|run| run.id).collect();
        if self.runs().iter().any(|run| (runs.contains(&run.id)
            || self.request_for_run_record(run).is_some_and(|request| request.goal_ids.iter().any(|id| tasks.contains(id))))
            && (matches!(run.status.as_str(), "Queued" | "Running" | "Cancelling" | "Recovering")
                || self.is_recovery_candidate(run)))
        {
            return Err("Related work is still running or awaiting recovery. Check its session status before deleting.".into());
        }
        let mut publication_topics: BTreeSet<_> = tasks.iter()
            .filter_map(|id| self.task_topic(*id).map(|topic| topic.id)).collect();
        publication_topics.extend(topic);
        publication_topics.extend(empty_topics.iter().copied());
        Ok(ArchiveDeletion { name, topic, empty_topics, tasks, requests, runs, publication_topics })
    }

    pub(crate) fn delete_archived(&mut self, target: ArchiveTarget, reviewed: &ArchiveDeletion) -> Result<(), String> {
        if &self.archive_deletion(target)? != reviewed {
            return Err("The archived research changed. Review the deletion again.".into());
        }
        for id in &reviewed.runs { self.remove_run(*id); }
        for id in &reviewed.requests { self.remove_request(*id); }
        let requests: Vec<_> = self.research_requests().iter()
            .filter(|request| request.goal_ids.iter().any(|id| reviewed.tasks.contains(id)))
            .map(|request| request.id).collect();
        for id in requests {
            self.request_mut(id).unwrap().goal_ids.retain(|id| !reviewed.tasks.contains(id));
        }
        for id in &reviewed.tasks { self.remove_task(*id); }
        let mut removed_topics: Vec<u64> = reviewed.empty_topics.iter().copied().collect();
        removed_topics.extend(reviewed.topic);
        for id in removed_topics {
            self.remove_topic(id);
            let copies: Vec<_> = self.topics().iter().filter(|topic| topic.replanned_from == Some(id)).map(|topic| topic.id).collect();
            for copy in copies { self.topic_mut(copy).unwrap().replanned_from = None; }
        }
        // Local publication snapshots can contain deleted reports; remote pages
        // and user-managed exports are intentionally outside this operation.
        self.retain_publications(|publication| !reviewed.publication_topics.contains(&publication.topic));
        Ok(())
    }
}
