use crate::{app::TaskApp, views::*};
use gpui_kit::{
    base::FocusTrapElement,
    component::{
        Disableable,
        input::{Input, InputEvent, InputState, Textarea},
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};

impl TaskApp {
    pub(crate) fn open_project_rename(&mut self, project: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.remember_dialog_opener(w, cx);
        let Some(name) = self.store.projects().iter().find(|p| p.id == project).map(|p| p.name.clone()) else { return; };
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
        let saved = self
            .apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::ProjectRename { project, name },
                "Project renamed",
                cx,
            )
            .is_some();
        if saved {
            self.project_rename = None;
            w.focus(&self.focus, cx);
        }
    }

    pub(crate) fn project_rename_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (_, input) = self.project_rename.as_ref().unwrap();
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(col().w(px(460.)).max_w_full().p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable("Rename project")))
                .child(muted("Change the display name. Existing topics, tasks, files and running work stay connected.", cx))
                .child(Input::new(input).id("rename-project-name"))
                .when(!self.error.is_empty(), |d| d.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    self.error.clone(),
                    cx,
                )))
                .child(row().justify_end()
                    .child(button("cancel-project-rename", "Cancel").on_click(cx.listener(|a, _, w, cx| {
                        a.project_rename = None; a.error.clear(); a.restore_dialog_opener(w, cx); cx.notify();
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
        let Some(mode) = self.planning_destination else {
            let message = self
                .planning_destination_error
                .clone()
                .or_else(|| (!self.error.is_empty()).then(|| self.error.clone()))
                .unwrap_or_else(|| {
                    "Choose a project before describing this research request.".into()
                });
            return div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center()
                .child(col().w(px(520.)).max_w_full().p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable("New research")))
                    .child(crate::views::notice(crate::views::NoticeTone::Warning, message, cx))
                    .child(row().justify_end().child(button("planning-cancel", "Cancel").on_click(cx.listener(|a, _, w, cx| a.request_cancel_planning(w, cx)))))
                )
                .focus_trap("project-plan-focus", &self.modal_focus)
                .into_any_element();
        };
        let project = self
            .store
            .projects()
            .iter()
            .find(|p| p.id == mode.project());
        let project_name = project
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Project unavailable".into());
        let destination = match mode {
            crate::navigation_context::CreationMode::NewTopic { .. } => None,
            crate::navigation_context::CreationMode::ExistingTopic { topic, .. } => {
                let topic = self.store.topics().iter().find(|t| t.id == topic);
                topic
                    .map(|t| format!("Topic: {}", t.display_name()))
                    .or_else(|| Some("The selected topic is unavailable".into()))
            }
            crate::navigation_context::CreationMode::Replan { source_topic, .. } => {
                let source = self.store.topics().iter().find(|t| t.id == source_topic);
                source
                    .map(|t| format!("Source topic: {}", t.display_name()))
                    .or_else(|| Some("The source topic is unavailable".into()))
            }
        };
        let title = match mode {
            crate::navigation_context::CreationMode::NewTopic { .. } => "New research",
            crate::navigation_context::CreationMode::ExistingTopic { .. } => "Plan research in this topic",
            crate::navigation_context::CreationMode::Replan { .. } => "Replan topic",
        };
        let entity = cx.entity().downgrade();
        let projects: Vec<(u64, String)> = self
            .store
            .visible_projects()
            .map(|p| (p.id, p.name.clone()))
            .collect();
        let selector = if mode.can_change_project() {
            let entity = entity.clone();
            quiet_button("planning-destination", project_name.clone())
                .dropdown_caret(true)
                .tooltip("Change the project this request will be saved in")
                .accessibility_label(format!("Target project: {project_name}"))
                .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                    for (id, name) in &projects {
                        let id = *id;
                        let entity = entity.clone();
                        menu = menu.item(
                            PopupMenuItem::new(name.clone()).on_click(move |_, w, cx| {
                                let _ = entity.update(cx, |app, cx| {
                                    app.retarget_planning_destination(id, w, cx);
                                });
                            }),
                        );
                    }
                    menu
                })
                .into_any_element()
        } else {
            quiet_button("planning-destination", project_name.clone())
                .disabled(true)
                .tooltip("Replan keeps the source topic's project. Change the source topic instead.")
                .into_any_element()
        };
        let error = self
            .planning_destination_error
            .clone()
            .or_else(|| (!self.error.is_empty()).then(|| self.error.clone()));
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center()
            .child(col().w(px(580.)).max_h(relative(0.92)).p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                .child(row().justify_between()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable(title)))
                    .child(tool_icon("close-project-plan", assets::IconName::Close, "Close planning").on_click(cx.listener(|a, _, w, cx| a.request_cancel_planning(w, cx)))))
                .when(matches!(mode, crate::navigation_context::CreationMode::Replan { .. }), |d| d.child(muted("Creates a separate topic marked Replanned. The original topic, goals and results are preserved. Review the request below, including any dates, and add your new requirements.", cx)))
                .child(col().gap_2()
                    .child(div().font_weight(FontWeight::MEDIUM).child(selectable("What do you want to research?")))
                    .child(div().id("project-plan-brief").child(Textarea::new(&self.project_brief)).map(observed)))
                .child(col().gap_1()
                    .child(row().gap_2().items_center().flex_wrap()
                        .child(label("Project", cx))
                        .child(selector))
                    .when_some(destination, |d, destination| d.child(label(destination, cx))))
                .child(label("First review a short understanding of your request. Goals are generated only after you confirm.", cx))
                .when_some(error, |d, message| d.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    message,
                    cx,
                )))
                .child(row().justify_between().gap_2()
                    .child(button("planning-cancel", "Cancel").on_click(cx.listener(|a, _, w, cx| a.request_cancel_planning(w, cx))))
                    .child(row().gap_2()
                        .child(button("save-project-context", "Save for later").on_click(cx.listener(move |a, _, _, cx| {
                            let Some(mode) = a.planning_destination else { return; };
                            let input = a.project_brief.read(cx).value().to_string();
                            let topic = match mode {
                                crate::navigation_context::CreationMode::ExistingTopic { topic, .. } => Some(topic),
                                _ => None,
                            };
                            a.capture_research_request(mode.project(), topic, input, false, cx);
                        })))
                        .child(primary_button("generate-project-plan", "Understand research").on_click(cx.listener(|a, _, _, cx| a.start_project_plan(cx))))))
            )
            .focus_trap("project-plan-focus", &self.modal_focus).into_any_element()
    }
}
