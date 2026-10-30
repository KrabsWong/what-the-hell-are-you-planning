//! Structural navigation: every Back action opens the immediate breadcrumb parent.
use crate::app::{Scope, TaskApp};
use gpui_kit::*;

#[derive(Clone)]
pub(crate) enum CrumbTarget {
    View(Scope),
    Task(u64),
}
impl TaskApp {
    pub(crate) fn displayed_run_id(&self) -> Option<u64> {
        self.selected_run
            .filter(|id| self.store.runs.iter().any(|r| r.id == *id))
            .or_else(|| self.store.runs.last().map(|r| r.id))
    }

    pub(crate) fn clear_navigation_overlays(&mut self) {
        self.save_feedback = None;
        self.draft = None;
        self.editing = false;
        self.property = None;
        self.new_project = None;
        self.topic_editor = None;
        self.planning_project = None;
        self.planning_topic = None;
        self.directory_project = None;
        self.error.clear();
    }

    pub(crate) fn go_back(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let (parents, _) = self.navigation_path();
        if let Some((_, _, target)) = parents.last() {
            match target {
                CrumbTarget::View(scope) => self.navigate(scope.clone(), w, cx),
                CrumbTarget::Task(id) => self.open_task(*id, w, cx),
            }
        }
    }
    pub(crate) fn close_current_task(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        self.tabs.retain(|task| *task != id);
        self.go_back(w, cx);
    }
}
