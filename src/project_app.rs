use crate::{
    app::{Layout, Scope, TaskApp},
    domain::Stage,
};
use gpui_kit::*;

impl TaskApp {
    pub fn open_project_plan(&mut self, project: u64, w: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.store.projects.iter().find(|p| p.id == project) else {
            return;
        };
        let brief = p.research_brief.clone();
        self.navigate(Scope::Project(project), w, cx);
        self.layout = Layout::Board;
        self.planning_project = Some(project);
        self.project_brief.update(cx, |s, cx| {
            s.set_value(brief, w, cx);
            s.focus(w, cx);
        });
        cx.notify();
    }

    pub fn save_project_brief(&mut self, cx: &mut Context<Self>) -> bool {
        let brief = self.project_brief.read(cx).value().trim().to_string();
        if brief.len() > 12000 {
            self.error = "Keep the project research scope within 12,000 bytes.".into();
            cx.notify();
            return false;
        }
        let Some(index) = self
            .store
            .projects
            .iter()
            .position(|p| Some(p.id) == self.planning_project)
        else {
            return false;
        };
        if self.store.projects[index].research_brief != brief {
            self.snapshot();
            self.store.projects[index].research_brief = brief;
        }
        self.persist_if_changed(true);
        self.error = self.storage_error.clone();
        cx.notify();
        self.error.is_empty()
    }

    pub fn start_project_plan(&mut self, cx: &mut Context<Self>) {
        if !self.ready_to_run(cx) || !self.save_project_brief(cx) {
            return;
        }
        let Some(project) = self.planning_project else {
            return;
        };
        match self.store.prepare_project_plan(project) {
            Ok(run) => {
                self.planning_project = None;
                self.launch_run(run, vec![], cx);
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
        let result = if self.store.project_plan_signature(project).as_ref().ok()
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
                    "{} research topics in Backlog · move chosen topics to Todo to approve",
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
            self.toast = "Research approved · open the topic to generate its research steps".into();
        }
    }
}
