use crate::{app::TaskApp, appearance::palette, views::*};
use gpui_kit::{
    base::FocusTrapElement,
    component::{
        Disableable,
        input::Textarea,
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};

impl TaskApp {
    pub fn project_toolbar_actions(&self, project: u64, cx: &mut Context<Self>) -> AnyElement {
        let planned = self.store.has_project_plan(project);
        let can_plan_first = !planned && self.store.todo_batch_items(project).is_empty();
        let entity = cx.entity().downgrade();
        row()
            .gap_1()
            .when(can_plan_first, |d| {
                d.child(
                    primary_button("plan-project", "Plan topics")
                        .icon(assets::IconName::Bot)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.open_project_plan(project, w, cx)
                        })),
                )
            })
            .when(!can_plan_first, |d| {
                d.child(self.todo_batch_action(project, cx))
            })
            .child(
                tool_icon(
                    "project-actions",
                    assets::IconName::Ellipsis,
                    "Project options",
                )
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let planning = entity.clone();
                    let folder = entity.clone();
                    menu.item(
                        PopupMenuItem::new(if planned {
                            "Planning details"
                        } else {
                            "Plan research topics"
                        })
                        .icon(assets::IconName::Bot)
                        .on_click(move |_, w, cx| {
                            let _ =
                                planning.update(cx, |a, cx| a.open_project_plan(project, w, cx));
                        }),
                    )
                    .item(
                        PopupMenuItem::new("Working folder…")
                            .icon(assets::IconName::Folder)
                            .on_click(move |_, w, cx| {
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
        let planned = self.store.has_project_plan(id);
        let model = self
            .store
            .research_model(crate::research::ResearchKind::Goal);
        let unavailable = if planned {
            Some("Topics already generated. Edit the existing Kanban cards.")
        } else if self.store.runs.iter().any(|r| {
            r.project_plan == Some(id)
                && matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")
        }) {
            Some("Wait for the active run to finish, or cancel it in Agent runs.")
        } else if project.directory.is_empty() {
            Some("To generate topics, choose a working folder above.")
        } else if !crate::opencode::valid_model(model) {
            Some(
                "To generate topics, choose a global model or a Plan override in Research settings.",
            )
        } else {
            None
        };
        div().absolute().inset_0().occlude().bg(rgba(0x00000080))
            .flex().items_center().justify_center()
            .child(col().w(px(580.)).max_h(relative(0.92)).p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                .child(row().justify_between()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Plan project research"))
                    .child(icon_button("close-project-plan", "×", "Close planning").tooltip("Close planning")
                        .on_click(cx.listener(|this, _, w, cx| { this.planning_project = None; this.save_feedback = None; this.error.clear(); w.focus(&this.focus, cx); cx.notify(); }))))
                .child(div().font_weight(FontWeight::MEDIUM).child(project.name.clone()))
                .child(label("The agent proposes independent research topics in Backlog. Move chosen topics to Todo to approve them; research starts only when you run it.", cx))
                .child(col().id("project-plan-scroll").overflow_y_scroll().min_h_0().gap_3()
                    .child(label("SCOPE & QUESTIONS · OPTIONAL", cx))
                    .child(div().id("project-plan-brief").child(Textarea::new(&self.project_brief)).map(observed))
                    .child(label("Leave blank to plan from the project name. Include markets, time horizon or exclusions to narrow the topics.", cx))
                    .child(row().justify_between().child(label("WORKING FOLDER", cx))
                        .child(button("project-plan-folder", "Set folder…").on_click(cx.listener(move |this, _, w, cx| this.open_directory(id, w, cx)))))
                    .child(div().text_xs().child(if project.directory.is_empty() { "Choose a folder before generating topics.".into() } else {project.directory.clone()}))
                    .child(row().justify_between().child(label("PLANNING MODEL · OPENCODE", cx))
                        .child(button("project-plan-model", "Settings →").on_click(cx.listener(move |this, _, w, cx| {
                            if this.save_project_brief(cx) {
                                this.planning_project = None;
                                this.research_project = id;
                                let scope = if this.store.research_config.models[0].is_empty() { crate::app::Scope::Settings } else { crate::app::Scope::Research };
                                let research_settings = scope == crate::app::Scope::Research;
                                this.navigate(scope, w, cx);
                                this.show_research_settings = research_settings;
                            }
                        }))))
                    .child(div().text_sm().child(self.model_source_label(&self.store.research_config.models[0])))
                    .when(planned, |d| d.child(label("This project already has a plan. Edit its Kanban topics directly. Updating this note does not change their saved scope.", cx)))
                    .map(observed))
                .when(!self.error.is_empty(), |d| d.child(div().text_sm().text_color(rgb(colors.danger)).child(self.error.clone())))
                .when(self.save_feedback.as_ref().is_some_and(Result::is_ok), |d| d.child(div().text_sm().text_color(rgb(colors.success)).child("✓ Project research scope saved locally")))
                .when_some(unavailable, |d, reason| d.child(label(reason, cx)))
                .child(row().justify_end()
                    .child(button("save-project-context", "Save scope").on_click(cx.listener(|this, _, _, cx| {
                        if this.save_project_brief(cx) {this.confirm_save("Project research scope saved to local workspace", cx);} else {this.show_save_error(this.error.clone(), cx);}
                    })))
                    .child(primary_button("generate-project-plan", if planned {"Topics generated"} else {"Generate topics"})
                        .disabled(planned || self.store.runs.iter().any(|r| r.project_plan == Some(id) && matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")) || project.directory.is_empty() || !crate::opencode::valid_model(model))
                        .on_click(cx.listener(|this, _, _, cx| this.start_project_plan(cx))))))
            .focus_trap("project-plan-focus", &self.modal_focus)
            .into_any_element()
    }
}
