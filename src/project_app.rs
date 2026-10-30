use crate::{
    app::{Layout, Scope, TaskApp},
    domain::Stage,
};
use gpui_kit::*;

impl TaskApp {
    pub fn open_project_plan(&mut self, project: u64, w: &mut Window, cx: &mut Context<Self>) {
        if !self.store.projects.iter().any(|p| p.id == project) {
            return;
        }
        self.navigate(Scope::Project(project), w, cx);
        self.layout = Layout::Board;
        self.planning_project = Some(project);
        self.planning_topic = None;
        self.project_brief.update(cx, |s, cx| {
            s.set_value("", w, cx);
            s.focus(w, cx);
        });
        cx.notify();
    }

    pub(crate) fn open_research_intake(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let topic = match self.scope {
            Scope::Topic(id) => Some(id),
            _ => None,
        };
        let project = topic
            .and_then(|id| self.store.topics.iter().find(|t| t.id == id))
            .map(|t| t.project_id)
            .or_else(|| {
                if let Scope::Project(id) = self.scope {
                    Some(id)
                } else {
                    None
                }
            })
            .unwrap_or(self.research_project);
        self.open_project_plan(project, w, cx);
        self.planning_topic = topic;
    }

    pub(crate) fn capture_research_request(
        &mut self,
        project: u64,
        topic: Option<u64>,
        input: String,
        start: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let id = match self.store.create_research_request(project, topic, input) {
            Ok(id) => id,
            Err(e) => {
                self.error = e;
                cx.notify();
                return false;
            }
        };
        self.persist_if_changed(true);
        if !self.storage_error.is_empty() {
            self.error = self.storage_error.clone();
            cx.notify();
            return false;
        }
        self.undo.clear();
        self.planning_project = None;
        self.planning_topic = None;
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
        let Some(project) = self.planning_project else {
            return;
        };
        let input = self.project_brief.read(cx).value().to_string();
        self.submit_research_request(project, self.planning_topic, input, cx);
    }

    pub(crate) fn retry_research_request(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) {
            return;
        }
        let Some(project_id) = self
            .store
            .research_requests
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.project_id)
        else {
            return;
        };
        if self
            .store
            .projects
            .iter()
            .find(|p| p.id == project_id)
            .is_some_and(|p| p.directory.is_empty())
        {
            match self.managed_project_directory(project_id) {
                Ok(path) => {
                    if let Some(p) = self.store.projects.iter_mut().find(|p| p.id == project_id) {
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
                self.store
                    .research_requests
                    .iter_mut()
                    .find(|r| r.id == id)
                    .unwrap()
                    .planning_run_id = Some(run_id);
                // launch_run persists the request mapping and queued run in one snapshot.
                // Never save a run ID before its run exists: another request could reuse it.
                self.launch_run(run, vec![], cx);
                if !self.store.runs.iter().any(|r| r.id == run_id) {
                    self.store
                        .research_requests
                        .iter_mut()
                        .find(|r| r.id == id)
                        .unwrap()
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

    pub fn finalize_project_plan(&mut self, index: usize) {
        let run = self.store.runs[index].clone();
        let project = run.project_plan.unwrap();
        if run.status != "Succeeded" {
            return;
        }
        let result = if let Some(request) = self.store.request_for_run(run.id) {
            let request_id = request.id;
            if self.store.request_plan_signature(request_id).ok().as_ref()
                != Some(&run.input_signature)
            {
                self.store.runs[index].status = "Stale".into();
                Err("Research destination changed during planning. Review the saved request and generate again.".into())
            } else {
                self.store.apply_request_plan(request_id, &run.output)
            }
        } else if self.store.project_plan_signature(project).as_ref().ok()
            != Some(&run.input_signature)
        {
            self.store.runs[index].status = "Stale".into();
            Err(
                "Project context changed during planning. Review the output and generate again."
                    .into(),
            )
        } else {
            self.store.apply_project_plan(project, &run.output)
        };
        match result {
            Ok(ids) => {
                // A plan is a durable external result, not part of an older UI undo snapshot.
                self.undo.clear();
                if self.scope == Scope::Runs
                    && self.selected.is_none()
                    && self.displayed_run_id() == Some(run.id)
                {
                    self.scope = Scope::Project(project);
                    self.layout = Layout::Board;
                    self.selected = None;
                }
                self.toast = format!(
                    "{} research goals in Backlog · move chosen goals to Todo to approve",
                    ids.len()
                );
                self.error.clear();
            }
            Err(e) => {
                let stored = &mut self.store.runs[index];
                if stored.status != "Stale" {
                    stored.status = "Failed".into();
                }
                stored.logs.push(format!("[project planning] {e}"));
                self.error = e;
                self.toast = "Project plan rejected · review output and retry".into();
            }
        }
        self.show_run_result = true;
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
