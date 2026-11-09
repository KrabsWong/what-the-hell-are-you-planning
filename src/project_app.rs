use crate::{
    app::{Scope, TaskApp},
    domain::Stage,
    navigation_context::{CreationIntent, CreationMode},
};
use gpui_kit::*;

impl TaskApp {
    pub fn open_project_plan(&mut self, project: u64, w: &mut Window, cx: &mut Context<Self>) {
        if !self.store.projects().iter().any(|p| p.id == project) {
            self.error = "That project is no longer available. Choose another destination.".into();
            cx.notify();
            return;
        }
        self.begin_planning(CreationMode::NewTopic { project }, w, cx);
    }

    /// Open the request form in place. The caller's route, filters, layout and
    /// scroll are untouched, so Cancel is an exact-origin operation.
    fn begin_planning(
        &mut self,
        mode: CreationMode,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.planning_destination = Some(mode);
        self.planning_destination_error = None;
        self.navigation.planning_baseline.clear();
        self.apply_planning_destination(mode);
        self.planning_opener_focus = w.focused(cx);
        self.project_brief.update(cx, |s, cx| {
            s.set_value("", w, cx);
            s.focus(w, cx);
        });
        cx.notify();
    }

    fn apply_planning_destination(&mut self, mode: CreationMode) {
        self.planning_project = Some(mode.project());
        match mode {
            CreationMode::NewTopic { .. } => {
                self.planning_topic = None;
                self.planning_replan_source = None;
            }
            CreationMode::ExistingTopic { topic, .. } => {
                self.planning_topic = Some(topic);
                self.planning_replan_source = None;
            }
            CreationMode::Replan { source_topic, .. } => {
                self.planning_topic = None;
                self.planning_replan_source = Some(source_topic);
            }
        }
    }

    fn clear_planning_destination(&mut self) {
        self.planning_destination = None;
        self.planning_destination_error = None;
        self.planning_project = None;
        self.planning_topic = None;
        self.planning_replan_source = None;
        self.navigation.planning_baseline.clear();
    }

    /// Close the request form without changing the page. Clean forms close
    /// immediately; dirty planning content is intercepted by the T10 decision.
    pub(crate) fn cancel_planning(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let opener = self.planning_opener_focus.take();
        self.clear_planning_destination();
        self.error.clear();
        if let Some(opener) = opener {
            w.focus(&opener, cx);
        } else {
            w.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Change the visible destination before saving or generating. A replan is
    /// fixed to its original project; an existing-topic choice becomes a new
    /// topic in the chosen project.
    pub fn retarget_planning_destination(
        &mut self,
        project: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mode) = self.planning_destination else {
            return false;
        };
        match crate::navigation_context::retarget_creation(&self.store, mode, project) {
            Ok(next) => {
                self.planning_destination = Some(next);
                self.planning_destination_error = None;
                self.apply_planning_destination(next);
                self.planning_project = Some(project);
                self.error.clear();
                w.focus(&self.modal_focus, cx);
                cx.notify();
                true
            }
            Err(error) => {
                self.planning_destination_error = Some(error.message());
                w.focus(&self.modal_focus, cx);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn open_topic_creation(
        &mut self,
        topic: u64,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match crate::navigation_context::resolve_creation_destination(
            &self.store,
            CreationIntent::TopicNewGoal { topic },
            &self.scope(),
            None,
        ) {
            Ok(mode) => self.begin_planning(mode, w, cx),
            Err(error) => {
                self.error = error.message();
                cx.notify();
            }
        }
    }

    pub(crate) fn open_topic_replan(&mut self, topic: u64, w: &mut Window, cx: &mut Context<Self>) {
        if !self.store.can_replan_topic(topic) {
            self.error = "Complete all goals in this topic before replanning.".into();
            cx.notify();
            return;
        }
        let Some(source) = self.store.topics().iter().find(|t| t.id == topic) else { return; };
        let _ = source;
        let input = self.store.topic_replan_input(topic);
        match crate::navigation_context::resolve_creation_destination(
            &self.store,
            CreationIntent::TopicReplan { topic },
            &self.scope(),
            None,
        ) {
            Ok(mode) => {
                self.begin_planning(mode, w, cx);
                self.project_brief.update(cx, |s, cx| s.set_value(input.clone(), w, cx));
                self.navigation.planning_baseline = input;
                cx.notify();
            }
            Err(error) => {
                self.error = error.message();
                cx.notify();
            }
        }
    }

    pub(crate) fn open_research_intake(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let context_task = self.selected().or(self.task_result_context());
        match crate::navigation_context::resolve_creation_destination(
            &self.store,
            CreationIntent::Global,
            &self.scope(),
            context_task,
        ) {
            Ok(mode) => self.begin_planning(mode, w, cx),
            Err(error) => {
                let message = error.message();
                self.planning_opener_focus = w.focused(cx);
                self.clear_planning_destination();
                self.planning_destination_error = Some(message.clone());
                self.error = message;
                // If another project exists, expose it in the selector instead
                // of leaving a dead form; the error explains the replacement.
                let fallback = self.store.visible_projects().map(|p| p.id).next();
                if let Some(project) = fallback {
                    self.planning_destination = Some(CreationMode::NewTopic { project });
                    self.apply_planning_destination(CreationMode::NewTopic { project });
                }
                self.project_brief.update(cx, |s, cx| {
                    s.set_value("", w, cx);
                    s.focus(w, cx);
                });
                cx.notify();
            }
        }
    }

    pub(crate) fn capture_research_request(
        &mut self,
        project: u64,
        topic: Option<u64>,
        input: String,
        start: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut staged = self.store.clone();
        let request = if let Some(source) = self.planning_replan_source {
            staged.create_topic_replan_request(source, input)
        } else {
            staged.create_research_request(project, topic, input)
        };
        let id = match request {
            Ok(id) => id,
            Err(e) => {
                self.error = e;
                cx.notify();
                return false;
            }
        };
        // Commit the request in one confirmed write. A failure leaves the live
        // store and the composer untouched, so Retry saves one request, not two.
        if let Err(error) = self.save_staged_store(&staged) {
            self.storage_error = error.clone();
            self.error = error;
            cx.notify();
            return false;
        }
        self.adopt_saved_store(staged);
        if !self.expanded_sidebar_projects.contains(&project) {
            self.expanded_sidebar_projects.push(project);
        }
        self.planning_project = None;
        self.planning_topic = None;
        self.planning_replan_source = None;
        self.planning_destination = None;
        self.planning_destination_error = None;
        self.planning_opener_focus = None;
        self.navigation.planning_baseline.clear();
        self.error.clear();
        self.toast = "Request saved · generate a plan when ready".into();
        if start {
            self.retry_research_request(id, cx);
        }
        cx.notify();
        true
    }

    pub(crate) fn submit_research_request(
        &mut self,
        project: u64,
        topic: Option<u64>,
        input: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.capture_research_request(project, topic, input, true, cx)
    }

    pub fn start_project_plan(&mut self, cx: &mut Context<Self>) {
        let Some(mode) = self.planning_destination else {
            self.error = "Choose a project before generating a plan.".into();
            cx.notify();
            return;
        };
        let input = self.project_brief.read(cx).value().to_string();
        let topic = match mode {
            CreationMode::ExistingTopic { topic, .. } => Some(topic),
            _ => None,
        };
        self.record_origin();
        self.submit_research_request(mode.project(), topic, input, cx);
    }

    pub(crate) fn retry_research_request(&mut self, id: u64, cx: &mut Context<Self>) {
        self.dispatch_research_request(id, true, cx);
    }

    /// Start a request whose save was just acknowledged without moving the
    /// user's route. Background/ack-driven work never changes navigation.
    pub(crate) fn retry_research_request_without_navigation(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_research_request(id, false, cx);
    }

    /// Automatic plan repair continues in the background without taking over the
    /// current page or selection.
    pub(crate) fn retry_research_request_background(&mut self, id: u64, cx: &mut Context<Self>) {
        self.dispatch_research_request(id, false, cx);
    }

    fn dispatch_research_request(&mut self, id: u64, navigate: bool, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) {
            return;
        }
        let Some(project_id) = self
            .store
            .research_requests()
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.project_id)
        else {
            return;
        };
        if self
            .store
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .is_some_and(|p| p.directory.is_empty())
        {
            match self.managed_project_directory(project_id) {
                Ok(path) => {
                    if let Some(p) = self.store.project_mut(project_id) {
                        p.directory = path;
                    }
                }
                Err(e) => {
                    self.error = e;
                    cx.notify();
                    return;
                }
            }
        }
        match self.store.prepare_request_plan(id) {
            Ok(run) => {
                let run_id = run.id;
                // The typed request-plan dispatch commits the request mapping and
                // queued run in one staged store.
                let dispatch = self.launch_run_with_intent(
                    run,
                    vec![],
                    crate::app::LaunchIntent::RequestPlan { request_id: id },
                    cx,
                );
                if navigate && let Some(dispatch) = dispatch {
                    self.open_run_progress(dispatch);
                }
                if !self.store.runs().iter().any(|r| r.id == run_id) {
                    self.store.request_mut(id).unwrap()
                        .planning_run_id = None;
                    self.persist_if_changed(true);
                }
            }
            Err(e) => {
                self.error = e;
                cx.notify();
            }
        }
    }

    pub fn finalize_project_plan(&mut self, index: usize, cx: Option<&mut Context<Self>>) {
        let run = self.store.runs()[index].clone();
        let project = run.project_plan.unwrap();
        if run.status != "Succeeded" {
            if let Some(automation) = self
                .store
                .request_for_run(run.id)
                .and_then(|request| request.automation_id)
                && self.automation_ui.planning_automation == Some(automation)
            {
                self.automation_ui.planning_automation = None;
                self.automation_ui.planning_run = None;
                self.automation_ui.feedback = Some(Err(
                    "Planning failed. The draft is unchanged; retry planning against the same request."
                        .into(),
                ));
            }
            return;
        }
        let mut planned_automation = None;
        let plan_automation = self
            .store
            .request_for_run(run.id)
            .and_then(|request| request.automation_id);
        let result = if let Some(request) = self.store.request_for_run(run.id) {
            let request_id = request.id;
            let automation = request.automation_id;
            let request_revision = request.revision;
            if self.store.request_plan_signature(request_id).ok().as_ref()
                != Some(&run.input_signature)
            {
                self.store.run_mut(self.store.runs()[index].id).unwrap().status = "Stale".into();
                Err("Research destination changed during planning. Review the saved request and generate again.".into())
            } else if automation.is_some()
                && !self.store.planning_attempt_is_current(run.id)
            {
                self.store.run_mut(self.store.runs()[index].id).unwrap().status = "Stale".into();
                Err("A newer planning attempt or revision is current. This saved plan was kept as history and was not applied.".into())
            } else if let Some(automation) = automation {
                // The reviewed plan becomes the frozen template for this automation.
                // No topic or goal is created here; each period instantiates them.
                let applied = self.store.apply_automation_template(
                    automation,
                    request_id,
                    request_revision,
                    &run.input_signature,
                    &run.output,
                    chrono::Utc::now().timestamp(),
                );
                if applied.is_ok() {
                    planned_automation = Some(automation);
                }
                applied.map(|_| vec![])
            } else if !request.goal_ids.is_empty() {
                self.store.regenerate_request_plan(request_id, &run.output)
            } else {
                self.store.apply_request_plan(request_id, &run.output)
            }
        } else if run.planning_owner.is_some() {
            self.store.run_mut(self.store.runs()[index].id).unwrap().status = "Stale".into();
            Err("The planning request for this response no longer exists. The output was kept as history and was not applied.".into())
        } else if self.store.project_plan_signature(project).as_ref().ok()
            != Some(&run.input_signature)
        {
            self.store.run_mut(self.store.runs()[index].id).unwrap().status = "Stale".into();
            Err(
                "Project context changed during planning. Review the output and generate again."
                    .into(),
            )
        } else {
            self.store.apply_project_plan(project, &run.output)
        };
        match result {
            Ok(ids) => {
                // A plan is a durable external result, not part of a UI undo entry.
                if let Some(automation) = planned_automation {
                    if let Some(cx) = cx {
                        self.automation_plan_ready(automation, run.id, cx);
                    }
                    self.error.clear();
                    return;
                }
                self.toast = format!(
                    "{} research goals in Backlog · open the project to review and approve",
                    ids.len()
                );
                self.error.clear();
            }
            Err(e) => {
                let stored = &mut self.store.run_mut(self.store.runs()[index].id).unwrap();
                if stored.status != "Stale" {
                    stored.status = "Failed".into();
                }
                stored.logs.push(format!("[project planning] {e}"));
                self.error = e;
                self.toast = "Project plan rejected · review output and retry".into();
                // Do not leave a saved draft stuck on "Planning…" after the
                // planner fails.
                if let Some(automation) = plan_automation
                    && self.automation_ui.planning_automation == Some(automation)
                {
                    self.automation_ui.planning_automation = None;
                    self.automation_ui.planning_run = None;
                    self.automation_ui.feedback = Some(Err(
                        "Planning failed. Open the draft and retry planning.".into(),
                    ));
                }
            }
        }
    }

    pub fn approve_research_topic(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(goal) = self
            .store
            .task(id)
            .and_then(|t| t.research.as_ref())
            .map(|r| r.goal_id)
        else {
            return;
        };
        self.change_stage(goal, Stage::Planned, cx);
        if self.error.is_empty() {
            self.toast = "Complete goal plan approved · click Run goal when ready".into();
        }
    }
}
