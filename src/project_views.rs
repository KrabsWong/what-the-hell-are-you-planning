use crate::{app::TaskApp, appearance::palette, views::*};
use gpui_kit::{
    base::FocusTrapElement,
    component::{
        input::{Input, InputEvent, InputState, Textarea},
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};

impl TaskApp {
    pub(crate) fn open_project_rename(&mut self, project: u64, w: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.store.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()) else { return; };
        self.clear_navigation_overlays();
        let input = cx.new(|cx| InputState::new(w, cx).placeholder("Project name").default_value(name));
        cx.subscribe(&input, |a, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && a.project_rename.is_some() {
                a.error.clear(); cx.notify();
            }
        }).detach();
        input.update(cx, |s, cx| s.focus(w, cx));
        self.project_rename = Some((project, input));
        cx.notify();
    }

    pub(crate) fn save_project_name(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some((project, input)) = &self.project_rename else { return; };
        let (project, name) = (*project, input.read(cx).value().to_string());
        let mut staged = self.store.clone();
        let result = staged.rename_project(project, &name);
        if result.is_ok() {
            if staged != self.store { self.snapshot(); self.store = staged; }
            self.project_rename = None;
            w.focus(&self.focus, cx);
        }
        self.finish(result, "Project renamed", cx);
    }

    pub(crate) fn project_rename_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (_, input) = self.project_rename.as_ref().unwrap();
        let colors = palette(cx);
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(col().w(px(460.)).max_w_full().p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Rename project"))
                .child(muted("Change the display name. Existing topics, tasks, files and running work stay connected.", cx))
                .child(Input::new(input).id("rename-project-name"))
                .when(!self.error.is_empty(), |d| d.child(div().text_color(rgb(colors.danger)).child(self.error.clone())))
                .child(row().justify_end()
                    .child(button("cancel-project-rename", "Cancel").on_click(cx.listener(|a, _, w, cx| {
                        a.project_rename = None; a.error.clear(); w.focus(&a.focus, cx); cx.notify();
                    })))
                    .child(primary_button("save-project-rename", "Save name").on_click(cx.listener(|a, _, w, cx| a.save_project_name(w, cx))))))
            .focus_trap("project-rename-focus", &self.modal_focus).into_any_element()
    }

    pub fn project_toolbar_actions(&self, project: u64, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        row()
            .gap_1()
            .child(
                primary_button("plan-project", "New research")
                    .tooltip("Describe a research question to automatically create its topic and plan.")
                    .on_click(cx.listener(move |a, _, w, cx| a.open_project_plan(project, w, cx))),
            )
            .child(self.todo_batch_action(project, cx))
            .child(
                tool_icon(
                    "project-actions",
                    assets::IconName::Ellipsis,
                    "Project options",
                )
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let folder = entity.clone();
                    let rename = entity.clone();
                    menu.item(PopupMenuItem::new("Rename project…").on_click(move |_, w, cx| {
                        let _ = rename.update(cx, |a, cx| a.open_project_rename(project, w, cx));
                    })).item(
                        PopupMenuItem::new("Working folder…").on_click(move |_, w, cx| {
                            let _ = folder.update(cx, |a, cx| a.open_directory(project, w, cx));
                        }),
                    )
                }),
            )
            .into_any_element()
    }

    pub fn project_plan_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let id = self.planning_project.unwrap();
        let Some(project) = self.store.projects.iter().find(|p| p.id == id) else {
            return div().into_any_element();
        };
        let destination = self
            .planning_topic
            .and_then(|id| self.store.topics.iter().find(|t| t.id == id))
            .map(|t| format!("{} / {}", project.name, t.display_name()))
            .unwrap_or_else(|| format!("{} · a new topic will be created from this request", project.name));
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center()
            .child(col().w(px(580.)).max_h(relative(0.92)).p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                .child(row().justify_between()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(if self.planning_topic.is_some() { "Plan research in this topic" } else { "Create a topic from one sentence" }))
                    .child(icon_button("close-project-plan", "×", "Close planning").on_click(cx.listener(|a, _, w, cx| { a.planning_project = None; a.planning_topic = None; w.focus(&a.focus, cx); cx.notify(); }))))
                .child(label(destination, cx))
                .child(muted("Describe your research question (required). The planner will suggest the scope and output.", cx))
                .child(div().id("project-plan-brief").child(Textarea::new(&self.project_brief)).map(observed))
                .child(label("Example: Analyze Intel’s latest earnings and market reaction using official disclosures and accessible analyst commentary. Give me a source-linked report.", cx))
                .child(muted("Goals and steps are saved under the topic. Goals appear in Backlog. Move a goal to Todo to approve all its steps, then click Run. Planning never starts the research.", cx))
                .when(!self.error.is_empty(), |d| d.child(div().text_color(rgb(colors.danger)).child(self.error.clone())))
                .child(row().justify_end()
                    .child(button("save-project-context", "Save request for later").on_click(cx.listener(move |a, _, _, cx| {
                        let input = a.project_brief.read(cx).value().to_string();
                        a.capture_research_request(id, a.planning_topic, input, false, cx);
                    })))
                    .child(primary_button("generate-project-plan", "Generate plan").on_click(cx.listener(|a, _, _, cx| a.start_project_plan(cx))))))
            .focus_trap("project-plan-focus", &self.modal_focus).into_any_element()
    }
}
