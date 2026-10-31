use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::palette,
    domain::Topic,
    views::*,
};
use gpui_kit::{
    base::FocusTrapElement,
    component::{
        Disableable,
        button::ButtonVariants,
        input::{Input, InputState},
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};

impl TaskApp {
    pub(crate) fn open_topic_editor(
        &mut self,
        project: u64,
        id: Option<u64>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = id
            .and_then(|id| self.store.topics.iter().find(|t| t.id == id))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        self.clear_navigation_overlays();
        let input = cx.new(|cx| {
            InputState::new(w, cx)
                .placeholder("Topic name")
                .default_value(name)
        });
        input.update(cx, |s, cx| s.focus(w, cx));
        self.topic_editor = Some((project, id, input));
        cx.notify();
    }

    pub(crate) fn save_topic(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some((project, id, input)) = &self.topic_editor else {
            return;
        };
        let (project, id, name) = (*project, *id, input.read(cx).value().to_string());
        self.snapshot();
        let result = match id {
            Some(id) => self.store.rename_topic(id, name).map(|_| id),
            None => self.store.create_topic(project, name),
        };
        match result {
            Ok(id) => {
                self.navigate(Scope::Topic(id), w, cx);
                if !self.expanded_sidebar_projects.contains(&project) {
                    self.expanded_sidebar_projects.push(project);
                }
                self.finish(Ok(()), "Topic saved", cx);
            }
            Err(error) => self.finish(Err(error), "", cx),
        }
    }

    pub(crate) fn topic_editor_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (_, id, input) = self.topic_editor.as_ref().unwrap();
        let colors = palette(cx);
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center()
            .child(col().w(px(460.)).p_6().gap_4().rounded_lg().bg(rgb(colors.panel))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(if id.is_some() { "Rename topic" } else { "New topic" }))
                .child(muted("Keep related research goals together around a company, subject or question.", cx))
                .child(Input::new(input).id("topic-name"))
                .when(!self.error.is_empty(), |d| d.child(div().text_color(rgb(colors.danger)).child(self.error.clone())))
                .child(row().justify_end()
                    .child(button("cancel-topic", "Cancel").on_click(cx.listener(|a, _, w, cx| { a.topic_editor = None; a.error.clear(); w.focus(&a.focus, cx); cx.notify(); })))
                    .child(primary_button("save-topic", "Save topic").on_click(cx.listener(|a, _, w, cx| a.save_topic(w, cx))))))
            .focus_trap("topic-focus", &self.modal_focus).into_any_element()
    }

    pub(crate) fn topic_picker(
        &self,
        goal: Option<u64>,
        project: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = goal
            .and_then(|id| self.store.task(id))
            .and_then(|t| t.topic_id)
            .or(if goal.is_none() {
                self.research_topic
            } else {
                None
            });
        let title = selected
            .and_then(|id| self.store.topics.iter().find(|t| t.id == id))
            .map(|t| {
                format!(
                    "{}{}",
                    t.display_name(),
                    if t.archived { " (archived)" } else { "" }
                )
            })
            .unwrap_or(if goal.is_some() {
                "Choose a topic".into()
            } else {
                "Propose a new topic".into()
            });
        let topics: Vec<_> = self
            .store
            .topics
            .iter()
            .filter(|t| t.project_id == project && !t.archived)
            .map(|t| (Some(t.id), t.display_name()))
            .collect();
        let entity = cx.entity().downgrade();
        quiet_button(
            if goal.is_some() {
                "goal-topic"
            } else {
                "research-topic"
            },
            title.clone(),
        )
        .px_2()
        .h(px(28.))
        .max_w(px(164.))
        .min_w_0()
        .tooltip(title.clone())
        .accessibility_label(title.clone())
        .disabled(goal.is_some_and(|id| self.store.task(id).is_some_and(|t| t.archived)))
        .dropdown_menu(move |mut menu, _, _| {
            for (id, name) in (if goal.is_none() {
                vec![(None, "Propose a new topic".to_string())]
            } else {
                vec![]
            })
            .into_iter()
            .chain(topics.clone())
            {
                let entity = entity.clone();
                menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, cx| {
                    let _ = entity.update(cx, |a, cx| {
                        if let Some(goal) = goal {
                            a.snapshot();
                            let result = if a.store.task(goal).is_some_and(|t| t.research.is_some())
                            {
                                a.store.set_goal_topic(goal, id)
                            } else {
                                id.ok_or("Choose a topic.".to_string())
                                    .and_then(|id| a.store.set_root_topic(goal, id))
                            };
                            a.finish(result, "Topic updated", cx);
                        } else {
                            a.research_topic = id;
                            cx.notify();
                        }
                    });
                }));
            }
            menu
        })
        .into_any_element()
    }

    pub(crate) fn topic_sidebar_row(&self, topic: &Topic, cx: &mut Context<Self>) -> AnyElement {
        let id = topic.id;
        let active = self.scope == Scope::Topic(id)
            || self
                .selected
                .or(self.task_result_context)
                .and_then(|id| self.store.task_topic(id))
                .is_some_and(|t| t.id == id);
        row()
            .gap_0()
            .mt_1()
            .min_w_0()
            .rounded_md()
            .hover(|d| d.bg(rgb(palette(cx).hover)))
            .child(
                quiet_button(format!("topic-{id}"), "")
                    .custom(sidebar_variant(active, cx))
                    .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                    .flex_1()
                    .min_w_0()
                    .h_8()
                    .child(single_line_text(
                        format!("topic-name-{id}"),
                        topic.display_name(),
                    ))
                    .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(id), w, cx))),
            )
            .child(
                tool_icon(
                    format!("topic-toggle-{id}"),
                    if self.collapsed_sidebar_topics.contains(&id) {
                        assets::IconName::ChevronRight
                    } else {
                        assets::IconName::ChevronDown
                    },
                    "Expand or collapse topic",
                )
                .custom(sidebar_variant(active, cx))
                .h_8()
                .on_click(cx.listener(move |a, _, _, cx| {
                    if a.collapsed_sidebar_topics.contains(&id) {
                        a.collapsed_sidebar_topics.retain(|t| *t != id);
                    } else {
                        a.collapsed_sidebar_topics.push(id);
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    pub(crate) fn topic_directory(&self, project: Option<u64>, cx: &mut Context<Self>) -> AnyElement {
        let topics: Vec<_> = self
            .store
            .topics
            .iter()
            .filter(|t| project.is_none_or(|id| t.project_id == id)
                && t.archived == project.is_none())
            .collect();
        if topics.is_empty() {
            return div().into_any_element();
        }
        row()
            .px_6()
            .pb_3()
            .flex_wrap()
            .child(label("TOPICS", cx))
            .children(topics.into_iter().map(|t| {
                let id = t.id;
                quiet_button(
                    format!("topic-link-{id}"),
                    format!(
                        "{}{}",
                        t.display_name(),
                        if t.archived { " · Archived" } else { "" }
                    ),
                )
                .px_0()
                .self_start()
                .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(id), w, cx)))
            }))
            .into_any_element()
    }

    pub(crate) fn topic_view(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(topic) = self.store.topics.iter().find(|t| t.id == id) else {
            return div().child("Topic not found").into_any_element();
        };
        let project = topic.project_id;
        let archived = topic.archived;
        let goals: Vec<_> = self
            .store
            .tasks
            .iter()
            .filter(|t| t.topic_id == Some(id) && (archived || !self.store.task_is_archived(t.id)))
            .collect();
        let mut list = col().gap_1();
        for goal in &goals {
            let goal_id = goal.id;
            let children = self.store.research_children(goal_id);
            let complete = children
                .iter()
                .filter(|id| self.store.research_result(**id).is_some())
                .count();
            let report = children
                .iter()
                .rev()
                .find(|id| {
                    self.store
                        .task(**id)
                        .and_then(|t| t.research.as_ref())
                        .is_some_and(|r| r.kind == crate::research::ResearchKind::Advise)
                        && self.store.research_result(**id).is_some()
                })
                .copied();
            let title = format!("{} · {}", self.store.identifier(goal), goal.title);
            let hint = title
                .chars()
                .collect::<Vec<_>>()
                .chunks(36)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            list = list.child(
                row()
                    .min_w_0()
                    .gap_1()
                    .child(
                        quiet_button(format!("topic-goal-{goal_id}"), "")
                            .flex_1()
                            .min_w_0()
                            .h_8()
                            .px_2()
                            .tooltip(hint)
                            .accessibility_label(title.clone())
                            .on_click(cx.listener(move |a, _, w, cx| a.open_task(goal_id, w, cx)))
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w_4()
                                            .flex_shrink_0()
                                            .text_color(rgb(stage_color(goal.stage, cx)))
                                            .child(goal.stage.symbol()),
                                    )
                                    .child(single_line_text(
                                        format!("topic-goal-title-{goal_id}"),
                                        title,
                                    ))
                                    .when(goal.archived, |d| d.child(label("Archived", cx)))
                                    .child(div().flex_shrink_0().child(label(
                                        if children.is_empty() {
                                            "Not planned".into()
                                        } else {
                                            format!("{complete}/{} steps", children.len())
                                        },
                                        cx,
                                    )))
                                    .child(div().flex_shrink_0().child(label("Open ›", cx))),
                            ),
                    )
                    .when_some(report, |d, report| {
                        d.child(
                            quiet_button(format!("topic-report-{goal_id}"), "Report ›")
                                .h_8()
                                .px_2()
                                .on_click(
                                    cx.listener(move |a, _, _, cx| a.view_task_result(report, cx)),
                                ),
                        )
                    }),
            );
        }
        let can_queue = !archived && self.store.todo_batch_items(project).iter().any(|goal|
            self.store.task_topic(*goal).is_some_and(|topic| topic.id == id)
                && self.todo_batch.as_ref().is_none_or(|batch| !batch.contains_pending(*goal)));
        let entity = cx.entity().downgrade();
        col().size_full()
            .child(row().px_6().pt_4().pb_2().flex_wrap().child(div().flex_1().min_w_0().text_lg().font_weight(FontWeight::SEMIBOLD).child(topic.display_name()))
                .when(can_queue, |d| d.child(button("queue-topic", "Queue topic")
                    .tooltip("Queue all approved unfinished goals in this topic. Topics execute one at a time.")
                    .on_click(cx.listener(move |a, _, _, cx| a.start_topic_batch(id, cx)))))
                .child(primary_button("topic-new-goal", "New research").disabled(archived).on_click(cx.listener(move |a, _, w, cx| {
                    a.open_project_plan(project, w, cx); a.planning_topic = Some(id); cx.notify();
                })))
                .child(tool_icon("topic-actions-menu", assets::IconName::Ellipsis, "More topic actions")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let rename = entity.clone();
                        let archive = entity.clone();
                        menu.item(PopupMenuItem::new("Rename topic").on_click(move |_, w, cx| {
                            let _ = rename.update(cx, |a, cx| a.open_topic_editor(project, Some(id), w, cx));
                        }))
                        .separator()
                        .item(PopupMenuItem::new(if archived { "Restore topic" } else { "Archive topic" }).on_click(move |_, w, cx| {
                            let _ = archive.update(cx, |a, cx| {
                                if archived {
                                    a.snapshot(); let result = a.store.set_topic_archived(id, false); a.finish(result, "Topic restored", cx);
                                } else {
                                    a.request_archive(crate::archive_app::ArchiveTarget::Topic(id), w, cx);
                                }
                            });
                        }))
                    })))
            .child(label(format!("{} research goal{}{}", goals.len(), if goals.len() == 1 { "" } else { "s" }, if archived { " · Archived topic. Existing goals and results remain available." } else { " · Open a goal to explore its steps and materials." }), cx).px_6())
            .child(self.todo_batch_bar(Some(project), cx))
            .child(row().px_6().pt_2().pb_3().justify_end().child(self.layout_switch(cx)))
            .when(self.layout == Layout::Board, |d| d.child(self.board(&goals.iter().map(|t| (*t).clone()).collect::<Vec<_>>(), cx)))
            .when(self.layout == Layout::List, |d| d.child(col().id("topic-goals").px_6().pb_5().overflow_y_scroll().flex_1().min_h_0()
                .when(goals.is_empty(), |d| d.child(muted("No research goals yet. Create one here or assign an existing goal using its Topic property.", cx)))
                .child(list)))
            .into_any_element()
    }
}
