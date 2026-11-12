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
        select::{Select, SelectState, SelectItem, SelectEvent, SearchableVec},
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};

fn topic_copy_tag(id: impl Into<SharedString>, cx: &App) -> AnyElement {
    chip("Copy", palette(cx).muted, cx)
        .id(id.into()).role(Role::Label).aria_label("Copy of another topic")
        .bg(rgb(palette(cx).tag_surface)).border_1().border_color(rgb(palette(cx).border_strong)).font_weight(FontWeight::NORMAL)
        .flex_shrink_0().whitespace_nowrap()
        .tooltip(|w, cx| component::tooltip::Tooltip::new("Created from another topic; open the topic to view its source.").build(w, cx))
        .map(observed)
}

fn topic_intake_tag(id: impl Into<SharedString>, status: crate::request_planning::TopicIntakeStatus, compact: bool, cx: &App) -> AnyElement {
    use crate::request_planning::TopicIntakeStatus;
    let colors = palette(cx);
    let color = match status {
        TopicIntakeStatus::Draft => colors.muted,
        TopicIntakeStatus::Understanding | TopicIntakeStatus::Planning => colors.accent,
        TopicIntakeStatus::NeedsReview | TopicIntakeStatus::Interrupted => colors.warning,
        TopicIntakeStatus::NeedsAttention => colors.danger,
    };
    let label = if compact {
        match status {
            TopicIntakeStatus::Understanding => "Aligning",
            TopicIntakeStatus::NeedsReview => "Review",
            TopicIntakeStatus::NeedsAttention => "Attention",
            TopicIntakeStatus::Interrupted => "Paused",
            _ => status.label(),
        }
    } else { status.label() };
    chip(label, color, cx)
        .when(compact, |d| d.px_1().text_size(px(11.)))
        .id(id.into()).role(Role::Label).aria_label(status.label())
        .tooltip(move |w, cx| component::tooltip::Tooltip::new(status.label()).build(w, cx))
        .bg(rgb(colors.tag_surface)).border_1().border_color(rgb(colors.border_strong)).font_weight(FontWeight::NORMAL)
        .flex_shrink_0().whitespace_nowrap().map(observed)
}

struct TopicTitleTags(Option<crate::request_planning::TopicIntakeStatus>);

impl gpui_kit::base::text::MarkdownPlugin for TopicTitleTags {
    fn name(&self) -> &str { "topic-title-tag" }

    fn parse(&self, node: &gpui_kit::base::text::markdown_ast::Node,
        _: &gpui_kit::base::text::MarkdownParseContext<'_>) -> Option<gpui_kit::base::text::MarkdownNode> {
        use gpui_kit::base::text::{MarkdownNode, markdown_ast::Node};
        let Node::Link(link) = node else { return None; };
        match link.url.as_str() {
            "topic-tag:copy" => Some(MarkdownNode::new(self.name(), true).text("Copy")),
            "topic-tag:status" => self.0.map(|status|
                MarkdownNode::new(self.name(), false).text(status.label())),
            _ => None,
        }
    }

    fn render_inline(&self, node: &gpui_kit::base::text::MarkdownNode,
        _: &gpui_kit::base::text::InlineRenderContext, _: &mut Window, cx: &mut App)
        -> Option<gpui_kit::base::text::InlineElement> {
        let tag = if *node.data::<bool>()? {
            topic_copy_tag("topic-copy-indicator", cx)
        } else {
            topic_intake_tag("topic-intake-status", self.0?, false, cx)
        };
        Some(gpui_kit::base::text::InlineElement::new(tag))
    }
}

fn topic_inline_title(topic: &Topic, status: Option<crate::request_planning::TopicIntakeStatus>) -> AnyElement {
    let mut text = String::new();
    if topic.replanned_from.is_some() { text.push_str("[Copy](topic-tag:copy) "); }
    if let Some(status) = status {
        text.push_str(&format!("[{}](topic-tag:status) ", status.label()));
    }
    // Treat the user-authored title as literal text, never Markdown syntax.
    for ch in topic.display_name().chars() {
        if ch.is_ascii_punctuation() { text.push('\\'); }
        text.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
    }
    gpui_kit::base::text::TextView::markdown("topic-inline-title", text)
        .plugin(TopicTitleTags(status))
        .text_lg().font_weight(FontWeight::SEMIBOLD)
        .into_any_element()
}

#[derive(Clone)]
pub(crate) struct ProjectChoice { pub id: u64, pub title: SharedString }
impl SelectItem for ProjectChoice {
    type Value = u64;
    fn title(&self) -> SharedString { self.title.clone() }
    fn value(&self) -> &u64 { &self.id }
}
pub(crate) struct TopicMoveDialog {
    topic: u64,
    pub(crate) projects: Entity<SelectState<SearchableVec<ProjectChoice>>>,
    has_destinations: bool,
}

impl TaskApp {
    pub(crate) fn request_topic_move(&mut self, topic: u64, w: &mut Window, cx: &mut Context<Self>) {
        self.remember_dialog_opener(w, cx);
        let Some(current) = self.store.topics().iter().find(|t| t.id == topic) else { return; };
        let choices: Vec<_> = self.store.visible_projects().filter(|p| p.id != current.project_id)
            .map(|p| ProjectChoice { id: p.id, title: format!("{} · {} (#{})", p.name, p.key, p.id).into() }).collect();
        let has_destinations = !choices.is_empty();
        let projects = cx.new(|cx| SelectState::new(SearchableVec::new(choices), None, w, cx).searchable(true));
        cx.subscribe(&projects, |_, _, _: &SelectEvent<SearchableVec<ProjectChoice>>, cx| cx.notify()).detach();
        self.clear_navigation_overlays();
        self.topic_move = Some(TopicMoveDialog { topic, projects, has_destinations });
        w.focus(&self.modal_focus, cx);
        cx.notify();
    }

    pub(crate) fn confirm_topic_move(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.topic_move else { return; };
        let topic = dialog.topic;
        let Some(project) = dialog.projects.read(cx).selected_value().copied() else { return; };
        // The modal also renders live blockers; repeat validation at the mutation boundary.
        let blockers = self.store.topic_move_blockers(topic, self.todo_batch.as_ref());
        if !blockers.is_empty() { cx.notify(); return; }
        let saved = self
            .apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TopicMove { topic, project },
                "Entire topic moved",
                cx,
            )
            .is_some();
        if saved {
            if self.research_topic == Some(topic) { self.research_project = project; }
            if self.planning_topic == Some(topic) {
                self.bump_planning_form_epoch();
                self.planning_project = Some(project);
                self.planning_destination = Some(
                    crate::navigation_context::CreationMode::ExistingTopic { project, topic },
                );
            }
            if !self.expanded_sidebar_projects.contains(&project) { self.expanded_sidebar_projects.push(project); }
            self.topic_move = None;
            w.focus(&self.focus, cx);
        }
    }

    pub(crate) fn topic_move_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let dialog = self.topic_move.as_ref().unwrap();
        let colors = palette(cx);
        let topic = self.store.topics().iter().find(|t| t.id == dialog.topic);
        let name = topic.map(|t| t.display_name()).unwrap_or_default();
        let source = topic.and_then(|t| self.store.projects().iter().find(|p| p.id == t.project_id)).map(|p| p.name.clone()).unwrap_or_default();
        let count = self.store.tasks().iter().filter(|t| t.automation_id.is_none() && self.store.task_topic(t.id).is_some_and(|t| t.id == dialog.topic)).count();
        let blockers = self.store.topic_move_blockers(dialog.topic, self.todo_batch.as_ref());
        let blocked = !blockers.is_empty();
        let selected = dialog.projects.read(cx).selected_value().copied();
        let destination = selected.and_then(|id| self.store.projects().iter().find(|p| p.id == id));
        let mut body = col().id("topic-move-body").min_h_0().max_h(px(360.)).overflow_y_scroll().gap_3()
            .child(label(format!("{name} · From {source} · {count} tasks"), cx));
        if blocked {
            body = body.child(muted(format!("{} blocking items. Resolve these before moving; no tasks will be stopped automatically.", blockers.len()), cx));
            for (index, blocker) in blockers.iter().enumerate() {
                let (title, remedy) = blocker.split_once('\n').unwrap_or((blocker, ""));
                body = body.child(col().p_3().gap_2().rounded(px(crate::appearance::GROUP_RADIUS)).bg(rgb(colors.side))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable(format!("{}. {title}", index + 1))))
                    .child(muted(remedy.trim().to_string(), cx)));
            }
        } else {
            body = body.child(muted("All goals, child tasks (including archived tasks), research requests and dependencies move together. Existing reports and materials stay at their current file locations.", cx));
            if dialog.has_destinations {
                body = body.child(label("Destination project", cx))
                    .child(Select::new(&dialog.projects).id("topic-move-project").accessibility_label("Destination project").w_full().menu_max_h(px(220.))
                        .placeholder("Choose a project…").search_placeholder("Search projects…"))
                    .child(muted(destination.map(|p| format!("Confirm to move the entire topic from {source} to {}.", p.name))
                        .unwrap_or("Select a destination, then confirm the move below.".into()), cx));
            } else {
                body = body.child(muted("No other projects available. Close this dialog and create a project first.", cx));
            }
        }
        if !self.error.is_empty() {
            body = body.child(crate::views::notice(
                crate::views::NoticeTone::Danger,
                self.error.clone(),
                cx,
            ));
        }
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center().p_4()
            .child(col().w(px(600.)).max_w_full().p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(if blocked { "Cannot move topic yet" } else { "Move topic" }))
                .child(body)
                .child(row().justify_end()
                    .child(button("cancel-topic-move", if blocked { "Close" } else { "Cancel" }).on_click(cx.listener(|a, _, w, cx| {
                        a.topic_move = None; a.error.clear(); a.restore_dialog_opener(w, cx); cx.notify();
                    })))
                    .when(!blocked, |d| d.child(primary_button("confirm-topic-move", "Move topic")
                        .disabled(destination.is_none())
                        .on_click(cx.listener(|a, _, w, cx| a.confirm_topic_move(w, cx)))))))
            .focus_trap("topic-move-focus", &self.modal_focus).into_any_element()
    }

    pub(crate) fn open_topic_editor(
        &mut self,
        project: u64,
        id: Option<u64>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = id
            .and_then(|id| self.store.topics().iter().find(|t| t.id == id))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        self.clear_navigation_overlays();
        self.remember_dialog_opener(w, cx);
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
        let outcome = match id {
            Some(topic) => self.apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TopicRename { topic, name },
                "Topic saved",
                cx,
            ),
            None => self.apply_domain_command_undoable(
                crate::scoped_domain::DomainCommand::TopicCreate { project, name },
                "Topic saved",
                cx,
            ),
        };
        if let Some(outcome) = outcome {
            let saved_id = id.or(outcome.created_topic);
            if let Some(id) = saved_id {
                self.navigate(Scope::Topic(id), w, cx);
            }
            if !self.expanded_sidebar_projects.contains(&project) {
                self.expanded_sidebar_projects.push(project);
            }
            self.topic_editor = None;
        }
    }

    pub(crate) fn topic_editor_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (_, id, input) = self.topic_editor.as_ref().unwrap();
        div().absolute().inset_0().occlude().bg(rgba(0x00000080)).flex().items_center().justify_center()
            .child(col().w(px(460.)).p_6().gap_4().map(|d| crate::appearance::floating_surface(d, cx))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(selectable(if id.is_some() { "Rename topic" } else { "New topic" })))
                .child(muted("Keep related research goals together around a company, subject or question.", cx))
                .child(Input::new(input).id("topic-name"))
                .when(!self.error.is_empty(), |d| d.child(crate::views::notice(
                    crate::views::NoticeTone::Danger,
                    self.error.clone(),
                    cx,
                )))
                .child(row().justify_end()
                    .child(button("cancel-topic", "Cancel").on_click(cx.listener(|a, _, w, cx| { a.topic_editor = None; a.error.clear(); a.restore_dialog_opener(w, cx); cx.notify(); })))
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
            .and_then(|id| self.store.topics().iter().find(|t| t.id == id))
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
            .topics()
            .iter()
            .filter(|t| t.project_id == project && !t.archived && t.automation_id.is_none())
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
                            a.apply_domain_command_undoable(
                                crate::scoped_domain::DomainCommand::GoalTopic { goal, topic: id },
                                "Topic updated",
                                cx,
                            );
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

    fn topic_plan_action(&self, topic: u64, cx: &mut Context<Self>) -> AnyElement {
        if self.store.can_replan_topic(topic) {
            return button("replan-topic", "Replan")
                .on_click(cx.listener(move |a, _, w, cx| a.open_topic_replan(topic, w, cx)))
                .into_any_element();
        }
        let requests: Vec<_> = self.store.research_requests().iter()
            .filter(|r| r.topic_id == Some(topic) && self.store.can_regenerate_request_plan(r.id))
            .map(|r| (r.id, r.input.clone())).collect();
        if requests.is_empty() { return div().into_any_element(); }
        let active = self.store.research_requests().iter().filter(|r| r.topic_id == Some(topic))
            .any(|r| r.planning_run_id.is_some_and(|id| self.store.runs().iter()
                .any(|run| run.id == id && matches!(run.status.as_str(), "Queued" | "Running" | "Cancelling"))));
        let action = button("regenerate-topic-plan", "Regenerate").disabled(active)
            .tooltip("Generate a replacement for an unapproved plan. Previous goals are archived only after the replacement is valid.");
        if requests.len() == 1 {
            let request = requests[0].0;
            return action.on_click(cx.listener(move |a, _, _, cx| a.retry_research_request(request, cx))).into_any_element();
        }
        let entity = cx.entity().downgrade();
        action.dropdown_menu(move |mut menu, _, _| {
            for (request, input) in &requests {
                let request = *request;
                let entity = entity.clone();
                menu = menu.item(PopupMenuItem::new(input.clone()).on_click(move |_, _, cx| {
                    let _ = entity.update(cx, |a, cx| a.retry_research_request(request, cx));
                }));
            }
            menu
        }).into_any_element()
    }

    pub(crate) fn topic_sidebar_row(&self, topic: &Topic, cx: &mut Context<Self>) -> AnyElement {
        let id = topic.id;
        let active = self.scope() == Scope::Topic(id)
            || self
                .selected()
                .or(self.task_result_context())
                .and_then(|id| self.store.task_topic(id))
                .is_some_and(|t| t.id == id);
        let status = self.store.topic_intake_status(id);
        let description = format!("{}{}{}", if topic.replanned_from.is_some() { "Copy · " } else { "" },
            topic.display_name(), status.map(|status| format!(" · {}", status.label())).unwrap_or_default());
        row()
            .items_center()
            .gap_0()
            .mt_1()
            .min_w_0()
            .rounded(px(crate::appearance::CONTROL_RADIUS))
            .hover(|d| d.bg(rgb(palette(cx).hover)))
            .child(
                row_button(format!("topic-{id}"), "")
                    .custom(sidebar_variant(active, cx))
                    .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                    .flex_1()
                    .min_w_0()
                    .h_8()
                    .tooltip(description.clone())
                    .accessibility_label(description)
                    .child(row().w_full().min_w_0().gap_1()
                        .when(topic.replanned_from.is_some(), |d| d.child(topic_copy_tag(format!("topic-copy-{id}"), cx)))
                        .when_some(status, |d, status| d.child(
                            topic_intake_tag(format!("topic-intake-{id}"), status, true, cx)))
                        .child(single_line_text(format!("topic-name-{id}"), topic.display_name()).map(observed)))
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
            .topics()
            .iter()
            .filter(|t| project.is_none_or(|id| t.project_id == id)
                && t.archived == project.is_none()
                // The Archive topic strip omits topics left without goals.
                // Deleting the last goal also removes the empty topic itself.
                && (project.is_some() || self.store.tasks().iter().any(|task|
                    task.parent_id.is_none()
                        && self.store.task_topic(task.id).is_some_and(|topic| topic.id == t.id))))
            .collect();
        if topics.is_empty() {
            return div().into_any_element();
        }
        let colors = palette(cx);
        col()
            .px_6()
            .pb_3()
            .gap_2()
            .min_w_0()
            .flex_shrink_0()
            .child(row().justify_between()
                .child(label(format!("TOPICS · {}", topics.len()), cx))
                .child(label("Scroll to browse · Click to open", cx)))
            .child(row()
                .id("topic-directory-scroll")
                .w_full()
                .min_w_0()
                .gap_2()
                .pb_2()
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .children(topics.into_iter().map(|t| {
                    let id = t.id;
                    let title = t.display_name();
                    let count = self.store.tasks().iter()
                        .filter(|task| task.parent_id.is_none()
                            && self.store.task_topic(task.id).is_some_and(|topic| topic.id == id))
                        .count();
                    let status = self.store.topic_planning_status(id)
                        .map(|s| format!(" · {s}")).unwrap_or_default();
                    button(format!("topic-link-{id}"), "")
                        .w(px(260.))
                        .h(px(64.))
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(rgb(colors.line))
                        .bg(rgb(colors.panel))
                        .tooltip(title.clone())
                        .accessibility_label(title.clone())
                        .child(col().w_full().min_w_0().gap_1()
                            .child(row().w_full().min_w_0().h_5().flex_shrink_0()
                                .child(single_line_text(format!("topic-directory-title-{id}"), title)))
                            .child(label(format!("{count} goals{}{status}",
                                if t.archived { " · Archived" } else { "" }), cx)))
                        .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(id), w, cx)))
                }))
                .map(observed))
            .into_any_element()
    }

    pub(crate) fn topic_view(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(topic) = self.store.topics().iter().find(|t| t.id == id) else {
            return div().child(selectable("Topic not found")).into_any_element();
        };
        let project = topic.project_id;
        let archived = topic.archived;
        let goals: Vec<_> = self
            .store
            .tasks()
            .iter()
            .filter(|t| t.topic_id == Some(id) && (archived || !self.store.task_is_archived(t.id)))
            .collect();
        let mut list = col().gap_1();
        for goal in &goals {
            let goal_id = goal.id;
            let archived_goal = self.store.task_is_archived(goal_id);
            let children = self.store.research_children(goal_id);
            let history = archived_goal.then(|| crate::archive_presentation::goal_history(&self.store, goal_id));
            let complete = history
                .as_ref()
                .map(|history| history.completed_steps)
                .unwrap_or_else(|| {
                    children
                        .iter()
                        .filter(|id| self.store.research_result_available_or_pending(**id))
                        .count()
                });
            let report = if archived_goal {
                crate::archive_presentation::result_target(&self.store, goal_id, true)
            } else {
                children
                    .iter()
                    .rev()
                    .find(|id| {
                        self.store
                            .task(**id)
                            .and_then(|t| t.research.as_ref())
                            .is_some_and(|r| r.kind == crate::research::ResearchKind::Advise)
                            && self.store.research_result_available_or_pending(**id)
                    })
                    .copied()
            };
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
                        row_button(format!("topic-goal-{goal_id}"), "")
                            .flex_1()
                            .min_w_0()
                            .h_8()
                            .px_2()
                            .tooltip(hint)
                            .accessibility_label(title.clone())
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.request_task_result_first(goal_id, w, cx)
                            }))
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w_4()
                                            .flex_shrink_0()
                                            .text_color(rgb(stage_color(goal.stage, cx)))
                                            .child(gpui_kit::component::Icon::new(crate::execution_status::ExecutionStatus::Stage(goal.stage).icon()).size_4()),
                                    )
                                    .child(single_line_text(
                                        format!("topic-goal-title-{goal_id}"),
                                        title,
                                    ))
                                    .when(archived_goal, |d| d.child(label("Archived", cx)))
                                    .child(div().flex_shrink_0().child(label(
                                        if children.is_empty() {
                                            "Not planned".into()
                                        } else {
                                            format!("{complete}/{} steps", children.len())
                                        },
                                        cx,
                                    )))
                                    .child(div().flex_shrink_0().child(row().gap_1().child(label("Open", cx)).child(gpui_kit::component::Icon::new(assets::IconName::ChevronRight).size_4()))),
                            ),
                    )
                    .when_some(report, |d, report| {
                        d.child(
                            button(format!("topic-report-{goal_id}"), "Report").icon(assets::IconName::ChevronRight)
                                .h_8()
                                .px_2()
                                .on_click(
                                    cx.listener(move |a, _, w, cx| {
                                        a.request_recent(
                                            report,
                                            crate::navigation_context::RecentSurface::Result,
                                            w,
                                            cx,
                                        )
                                    }),
                                ),
                        )
                    })
                    .when(!archived_goal && self.store.goal_tasks_complete(goal_id), |d| {
                        d.child(quiet_button(format!("topic-export-{goal_id}"), "Export")
                            .h_8().px_2()
                            .disabled(self.goal_busy(goal_id))
                            .tooltip("Save completed evidence, analysis and reports as a ZIP")
                            .on_click(cx.listener(move |a, _, _, cx| a.export_research_goal(goal_id, cx))))
                    }),
            );
        }
        let can_queue = !archived && self.store.todo_batch_items(project).iter().any(|goal|
            self.store.task_topic(*goal).is_some_and(|topic| topic.id == id)
                && self.todo_batch.as_ref().is_none_or(|batch| !batch.contains_pending(*goal)));
        let planning_status = self.store.topic_planning_status(id);
        let planning_only = goals.is_empty() && planning_status.is_some();
        let entity = cx.entity().downgrade();
        col().size_full()
            .child(row().px_6().pt_4().pb_2().gap_2().flex_wrap()
                .child(div().flex_1().min_w_0().child(topic_inline_title(topic, self.store.topic_intake_status(id))))
                .when(can_queue, |d| d.child(button("queue-topic", "Queue topic")
                    .tooltip("Queue all approved unfinished goals in this topic. Topics execute one at a time.")
                    .on_click(cx.listener(move |a, _, _, cx| a.start_topic_batch(id, cx)))))
                .child(self.topic_plan_action(id, cx))
                .child(self.topic_publish_button(id, cx))
                .child(primary_button("topic-new-goal", "New research").disabled(archived).on_click(cx.listener(move |a, _, w, cx| {
                    a.open_topic_creation(id, w, cx);
                })))
                .child(tool_icon("topic-actions-menu", assets::IconName::Ellipsis, "More topic actions")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let moving = entity.clone();
                        let menu = menu.item(PopupMenuItem::new("Move…").on_click(move |_, w, cx| {
                            let _ = moving.update(cx, |a, cx| a.request_topic_move(id, w, cx));
                        }));
                        let rename = entity.clone();
                        let archive = entity.clone();
                        let delete = entity.clone();
                        menu.item(PopupMenuItem::new("Rename topic").on_click(move |_, w, cx| {
                            let _ = rename.update(cx, |a, cx| a.open_topic_editor(project, Some(id), w, cx));
                        }))
                        .separator()
                        .item(PopupMenuItem::new(if archived { "Restore topic" } else { "Archive topic" }).on_click(move |_, w, cx| {
                            let _ = archive.update(cx, |a, cx| {
                                if archived {
                                    a.apply_domain_command_undoable(
                                        crate::scoped_domain::DomainCommand::TopicArchive {
                                            topic: id,
                                            archived: false,
                                        },
                                        "Topic restored",
                                        cx,
                                    );
                                } else {
                                    a.request_archive(crate::archive_app::ArchiveTarget::Topic(id), w, cx);
                                }
                            });
                        }))
                        .when(archived, |menu| menu.item(PopupMenuItem::new("Delete topic permanently…").on_click(move |_, w, cx| {
                            let _ = delete.update(cx, |a, cx| a.request_delete_archived(crate::archive_app::ArchiveTarget::Topic(id), w, cx));
                        })))
                    })))
            .child(row().px_6().pb_2().gap_3()
                .child(self.layout_switch(cx))
                .child(div().flex_1().min_w_0().child(label(format!("{} research goal{}{}", goals.len(), if goals.len() == 1 { "" } else { "s" }, if archived { " · Archived topic. Existing goals and results remain available." } else { "" }), cx)
                    .id("topic-goal-count")
                    .tooltip(|w, cx| component::tooltip::Tooltip::new("Open a goal to explore its steps and materials.").build(w, cx)))))
            .when_some(topic.replanned_from, |d, source| d.child(row().px_6().pb_2()
                .child(link_button("replan-source", self.store.topics().iter().find(|t| t.id == source)
                    .map(|t| format!("From {} · Open original topic", t.display_name()))
                    .unwrap_or_else(|| "Original topic unavailable".into()))
                    .disabled(!self.store.topics().iter().any(|t| t.id == source))
                    .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(source), w, cx))))))
            .child(self.topic_publication_panel(id, cx))
            .when_some(planning_status, |d, _| d.child(
                col().id("topic-planning-panel").px_6().py_3().gap_2()
                    .when(goals.is_empty(), |d| d.flex_1().min_h_0().overflow_y_scroll())
                    .child(col().flex_shrink_0().gap_2()
                        .child(col().id("topic-planning-requests")
                            .when(!goals.is_empty(), |d| d.max_h(px(260.)).overflow_y_scroll())
                            .child(self.request_history(Some(project), cx))))
                    .map(observed)
            ))
            .child(self.todo_batch_bar(Some(project), cx))
            .when(self.layout == Layout::Board && !planning_only, |d| d.child(self.board(&goals.iter().map(|t| (*t).clone()).collect::<Vec<_>>(), cx)))
            .when(self.layout == Layout::List && !planning_only, |d| d.child(col().id("topic-goals").track_scroll(&self.navigation.topic_scroll).px_6().pt_3().pb_5().overflow_y_scroll().flex_1().min_h_0()
                .when(goals.is_empty(), |d| d.child(muted("No research goals yet. Create one here or assign an existing goal using its Topic property.", cx)))
                .child(list)))
            .into_any_element()
    }
}
