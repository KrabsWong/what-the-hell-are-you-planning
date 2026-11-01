//! Live execution status shared by the sidebar, list and board.
use crate::{
    batch::{BatchState, TodoBatch},
    domain::{Stage, Store},
    goal_steps::StepState,
    research::ResearchKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Queued,
    Retrying,
    Waiting,
    Ready,
    Paused,
    Blocked,
    Done,
    Approval,
    Stage(Stage),
}
impl ExecutionStatus {
    pub(crate) const ALL: [Self; 9] = [Self::Running, Self::Queued, Self::Retrying,
        Self::Waiting, Self::Ready, Self::Paused, Self::Blocked, Self::Approval, Self::Done];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Queued => "Queued",
            Self::Retrying => "Retrying",
            Self::Waiting => "Waiting",
            Self::Ready => "Ready",
            Self::Paused => "Paused",
            Self::Blocked => "Blocked",
            Self::Done => "Done",
            Self::Approval => "Review plan",
            Self::Stage(stage) => stage.label(),
        }
    }
    pub(crate) fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::Running | Self::Stage(Stage::InProgress) => IconName::CircleDot,
            Self::Queued => IconName::ListOrdered,
            Self::Retrying => IconName::RotateCw,
            Self::Waiting => IconName::Clock,
            Self::Paused => IconName::CirclePause,
            Self::Blocked => IconName::CircleAlert,
            Self::Done | Self::Stage(Stage::Done) => IconName::Check,
            Self::Approval | Self::Stage(Stage::Review) => IconName::ClipboardCheck,
            Self::Stage(Stage::Cancelled) => IconName::CircleX,
            _ => IconName::Circle,
        }
    }
    pub(crate) fn icon_view(self, cx: &gpui_kit::App) -> gpui_kit::Div {
        use gpui_kit::*;
        div().w(px(16.)).h(px(16.)).flex_shrink_0().flex().items_center().justify_center()
            .text_color(rgb(self.color(cx)))
            .child(component::Icon::new(self.icon()).size(px(16.)))
    }
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::Running => "Execution is active.",
            Self::Queued => {
                "Queued to start automatically when this topic or an execution slot becomes available."
            }
            Self::Retrying => {
                "Automatic retry is queued after a failure; waiting for the retry delay or an execution slot."
            }
            Self::Waiting => "Waiting for prerequisite tasks to finish successfully.",
            Self::Ready => {
                "Ready to run; not currently queued. Run the goal or add its topic to the queue."
            }
            Self::Paused => {
                "The topic queue is paused. Review the queue message and continue when ready."
            }
            Self::Blocked => {
                "This task or one of its prerequisites needs attention. Open the task for details."
            }
            Self::Done => "Completed result is available.",
            Self::Approval => "Review and approve the research plan before running.",
            Self::Stage(_) => "Workflow stage; no active execution.",
        }
    }
    pub(crate) fn color(self, cx: &gpui_kit::App) -> u32 {
        let p = crate::appearance::palette(cx);
        match self {
            Self::Running | Self::Retrying => p.warning,
            Self::Blocked => p.danger,
            Self::Done => p.success,
            Self::Queued => p.secondary,
            Self::Stage(stage) => crate::views::stage_color(stage, cx),
            _ => p.secondary,
        }
    }
}
impl Store {
    pub(crate) fn execution_status(&self, id: u64, batch: Option<&TodoBatch>) -> ExecutionStatus {
        use ExecutionStatus::*;
        let Some(task) = self.task(id) else {
            return Blocked;
        };
        let goal = self.owning_research_goal(id).unwrap_or(id);
        let is_goal = task
            .research
            .as_ref()
            .is_some_and(|r| r.kind == ResearchKind::Goal);
        // Goals aggregate real run records, rather than treating their workflow stage as activity.
        let ids = if is_goal {
            let mut ids = self.goal_step_ids(id);
            ids.push(id);
            ids
        } else {
            vec![id]
        };
        let latest: Vec<_> = ids
            .iter()
            .filter_map(|id| {
                self.runs
                    .iter()
                    .rev()
                    .find(|r| r.task_id == *id && r.project_plan.is_none())
            })
            .collect();
        if latest
            .iter()
            .any(|r| matches!(r.status.as_str(), "Running" | "Cancelling" | "Recovering"))
        {
            return Running;
        }
        if (is_goal && self.goal_tasks_complete(id))
            || (!is_goal && self.step_state(id) == StepState::Done)
        {
            return Done;
        }
        let pending = batch.filter(|b| b.contains_pending(goal));
        if pending.is_some_and(|b| matches!(b.state, BatchState::Paused(_))) {
            return Paused;
        }
        if latest
            .iter()
            .any(|r| r.status == "Queued" && r.auto_retry_attempt > 0)
        {
            return Retrying;
        }
        if latest.iter().any(|r| r.status == "Queued") {
            return Queued;
        }
        if (is_goal || task.research.is_none()) && pending.is_some_and(|b| !b.current_items(self).contains(&goal)) {
            return Queued;
        }
        if is_goal {
            if !self.goal_execution_approved(id) {
                return Approval;
            }
            let children = self.research_children(id);
            if children
                .iter()
                .any(|id| self.step_state(*id) == StepState::Blocked)
                || batch.is_some_and(|b| b.failed_items.contains(&id))
            {
                return Blocked;
            }
            return if self
                .ready_research_tasks(id)
                .is_ok_and(|ids| !ids.is_empty())
            {
                if pending.is_some() { Queued } else { Ready }
            } else {
                Waiting
            };
        }
        if task.research.is_some() {
            if self.step_state(id) == StepState::Blocked {
                return Blocked;
            }
            if self.research_pending_approval(id) {
                return Approval;
            }
            if task
                .research
                .as_ref()
                .unwrap()
                .depends_on
                .iter()
                .any(|dep| {
                    self.task(*dep)
                        .is_none_or(|t| t.stage != crate::domain::Stage::Done)
                        || self.research_result(*dep).is_none()
                })
            {
                return Waiting;
            }
            return if pending.is_some() { Queued } else { Ready };
        }
        if pending.is_some() { return Queued; }
        if !task.stage.is_closed() && latest.iter().any(|r| matches!(r.status.as_str(), "Failed" | "Blocked" | "Interrupted" | "Cancelled")) {
            return Blocked;
        }
        if matches!(task.stage, crate::domain::Stage::Planned | crate::domain::Stage::InProgress)
            && latest.iter().any(|r| r.status == "Succeeded" && r.stage == task.stage) {
            return Stage(crate::domain::Stage::Review);
        }
        Stage(task.stage)
    }
}
impl crate::app::TaskApp {
    pub(crate) fn execution_summary(&self, goal: u64) -> String {
        let states: Vec<_> = self.store.goal_step_ids(goal).into_iter()
            .map(|id| self.display_task_status(id).execution).collect();
        let mut parts = vec![format!("{} steps", states.len())];
        for status in ExecutionStatus::ALL.into_iter().chain(Stage::ALL.into_iter().map(ExecutionStatus::Stage)) {
            let count = states.iter().filter(|state| **state == status).count();
            if count > 0 { parts.push(format!("{count} {}", status.label().to_lowercase())); }
        }
        parts.join(" · ")
    }

    pub(crate) fn execution_indicator(&self, id: u64, cx: &gpui_kit::App) -> gpui_kit::AnyElement {
        use gpui_kit::*;
        let state = self.display_task_status(id);
        let status = state.execution;
        // A status is not an action: no button label slot, content gap or hover fill.
        // Reserve a non-shrinking 24px box for the shared 16px icon in cards and list rows.
        div().id(format!("execution-status-{id}"))
            .aria_label(state.label())
            .size(px(24.)).flex_shrink_0()
            .flex().items_center().justify_center()
            .child(status.icon_view(cx))
            .tooltip(move |window, cx| {
                component::tooltip::Tooltip::new(format!("{} · {}", state.label(), state.reason))
                    .build(window, cx)
            })
            .into_any_element()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Store, u64, Vec<u64>, TodoBatch) {
        let mut s = Store::empty();
        s.default_model = "fixture/model".into();
        s.projects[0].directory = std::env::temp_dir().to_string_lossy().into();
        let topic = s.create_topic(1, "Topic").unwrap();
        let goal = s
            .create_research_goal_in_topic("Goal".into(), "Scope".into(), 1, Some(topic))
            .unwrap();
        let ids = s.apply_research_plan(goal, r#"{"tasks":[{"title":"Collect","instructions":"Collect","kind":"collect","depends_on":[]},{"title":"Analyze","instructions":"Analyze","kind":"analyze","depends_on":[0]},{"title":"Report","instructions":"Report","kind":"advise","depends_on":[1]}]}"#).unwrap();
        let batch = TodoBatch {
            project_id: 1,
            items: vec![goal],
            completed: 0,
            runs_completed: 0,
            state: BatchState::Running,
            run_ids: Default::default(),
            done_items: vec![],
            failed_items: vec![],
            failed_tasks: vec![],
        };
        (s, goal, ids, batch)
    }
    #[test]
    fn distinguishes_admission_dependencies_runs_retries_and_pause() {
        use ExecutionStatus::*;
        let (mut s, goal, ids, mut batch) = fixture();
        assert_eq!(s.execution_status(ids[0], None), Ready);
        assert_eq!(s.execution_status(ids[1], None), Waiting);
        assert_eq!(s.execution_status(goal, Some(&batch)), Queued);
        assert_eq!(s.execution_status(ids[0], Some(&batch)), Queued);
        assert_eq!(s.execution_status(ids[1], Some(&batch)), Waiting);
        let mut run = s.prepare_run(ids[0]).unwrap();
        run.status = "Queued".into();
        s.runs.push(run);
        assert_eq!(s.execution_status(ids[0], None), Queued);
        s.runs[0].status = "Running".into();
        assert_eq!(s.execution_status(goal, Some(&batch)), Running);
        batch.state = BatchState::Paused("Paused".into());
        assert_eq!(s.execution_status(ids[0], Some(&batch)), Running);
        assert_eq!(s.execution_status(ids[1], Some(&batch)), Paused);
        batch.state = BatchState::Running;
        s.runs[0].status = "Queued".into();
        s.runs[0].auto_retry_attempt = 1;
        assert_eq!(s.execution_status(goal, Some(&batch)), Retrying);
        s.runs[0].status = "Failed".into();
        assert_eq!(s.execution_status(goal, Some(&batch)), Blocked);
        assert_eq!(s.execution_status(ids[1], Some(&batch)), Blocked);
        s.runs[0].status = "Succeeded".into();
        s.runs[0].output = "Verified result".into();
        s.task_mut(ids[0]).unwrap().stage = crate::domain::Stage::Done;
        assert_eq!(s.execution_status(ids[0], Some(&batch)), Done);
        assert_eq!(s.execution_status(ids[1], Some(&batch)), Queued);
    }
    #[test]
    fn future_topic_is_queued_without_any_run_records() {
        let (mut s, goal, ids, mut batch) = fixture();
        let first_topic = s.create_topic(1, "First").unwrap();
        let first = s
            .create_research_goal_in_topic(
                "First goal".into(),
                "Scope".into(),
                1,
                Some(first_topic),
            )
            .unwrap();
        batch.items.insert(0, first);
        assert_eq!(
            s.execution_status(goal, Some(&batch)),
            ExecutionStatus::Queued
        );
        assert_eq!(
            s.execution_status(ids[1], Some(&batch)),
            ExecutionStatus::Waiting
        );
        batch.failed_items.push(first);
        assert_eq!(
            s.execution_status(ids[1], Some(&batch)),
            ExecutionStatus::Waiting
        );
    }
}
