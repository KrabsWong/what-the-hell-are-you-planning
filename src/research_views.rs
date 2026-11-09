use crate::appearance::palette;
use crate::{
    app::TaskApp,
    domain::AgentRun,
    research::{self, ResearchKind},
    views::*,
};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::{
    component::{
        Disableable,
        input::{Input, Textarea},
    },
    prelude::FluentBuilder,
    *,
};
impl TaskApp {
    pub fn research_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let has_composer_draft = !self.research_title.read(cx).value().trim().is_empty()
            || !self.research_brief.read(cx).value().trim().is_empty();
        let mut goals = col().gap_1();
        for task in self
            .store
            .tasks()
            .iter()
            .filter(|t| {
                !self.store.task_is_archived(t.id)
                    && t.automation_id.is_none()
                    && t.research
                        .as_ref()
                        .is_some_and(|r| r.kind == ResearchKind::Goal)
            })
            .rev()
        {
            let id = task.id;
            let children = self.store.research_children(id);
            let completed = children
                .iter()
                .filter(|id| self.store.research_result_available_or_pending(**id))
                .count();
            let title = format!("{} · {}", self.store.identifier(task), task.title);
            let hint = title
                .chars()
                .collect::<Vec<_>>()
                .chunks(36)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            goals = goals.child(
                row_button(format!("research-goal-{id}"), "")
                    .w_full()
                    .h_9()
                    .px_2()
                    .tooltip(hint)
                    .accessibility_label(title.clone())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_task(id, w, cx)))
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .child(
                                div()
                                    .w_4()
                                    .flex_shrink_0()
                                    .text_color(rgb(stage_color(task.stage, cx)))
                                    .child(gpui_kit::component::Icon::new(crate::execution_status::ExecutionStatus::Stage(task.stage).icon()).size_4()),
                            )
                            .child(single_line_text(format!("research-goal-title-{id}"), title))
                            .when(self.store.research_pending_approval(id), |d| {
                                d.child(div().flex_shrink_0().child(label("Awaiting approval", cx)))
                            })
                            .child(div().flex_shrink_0().child(label(
                                if children.is_empty() {
                                    "Not planned".into()
                                } else {
                                    format!("{completed}/{} steps", children.len())
                                },
                                cx,
                            )))
                            .child(div().flex_shrink_0().child(row().gap_1().child(label("Open", cx)).child(gpui_kit::component::Icon::new(assets::IconName::ChevronRight).size_4()))),
                    ),
            );
        }
        let composer=col().w_full().max_w(px(850.)).min_w_0().gap_4()
            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(selectable("Start with one sentence")))
            .child(muted("Describe what you want to understand and the output you need. We will draft a complete plan in Backlog for your review.", cx))
            .child(label("RESEARCH QUESTION", cx))
            .child(Input::new(&self.research_title).id("research-title"))
            .child(label("EXTRA CONTEXT · OPTIONAL", cx))
            .child(div().id("research-brief").child(Textarea::new(&self.research_brief)).map(observed))
            .child(label("PROJECT", cx))
            .child(row().flex_wrap().children(self.store.visible_projects().map(|p|{let id=p.id;select_button(format!("research-project-{id}"),p.name.clone(),self.research_project==id,cx).on_click(cx.listener(move|this,_,_,cx|{this.research_project=id;this.research_topic=None;cx.notify();}))})))
            .child(label("TOPIC", cx))
            .child(self.topic_picker(None, self.research_project, cx))
            .child(
                row().flex_wrap()
                    .child(button("research-folder", "Set working folder…").on_click(cx.listener(|this, _, w, cx| this.open_directory(this.research_project, w, cx))))
                    .child(primary_button("create-research", "Generate research plan").on_click(cx.listener(|this, _, w, cx| this.create_research(w, cx))))
                    .when(has_composer_draft, |d| {
                        d.child(
                            button("discard-research-draft", "Discard draft")
                                .tooltip("Clear the parked research request title and context")
                                .on_click(cx.listener(|this, _, w, cx| this.discard_parked_composer(w, cx))),
                        )
                    }),
            )
            .child(label("Your request is saved first. Planning does not run the research. Move selected goals to Todo, then click Run.", cx))
            .child(div().h_4()).child(row().child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Your research goals"))))
            .child(goals)
            .child(self.request_history(None, cx));
        col()
            .size_full()
            .child(
                row()
                    .h_16()
                    .px_8()
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Market research")))
                    .child(button("research-settings-toggle", "Research settings…")
                        .on_click(cx.listener(|this, _, w, cx| this.open_model_settings(w, cx)))),
            )
            .child(
                col()
                    .id("research-scroll")
                    .items_start()
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .p_6()
                    .gap_6()
                    .child(composer)
                    .map(observed),
            )
            .into_any_element()
    }
    fn model_agent_picker(&self, stage: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        let agent = stage.map(|i| self.store.research_agent(ResearchKind::ALL[i]))
            .unwrap_or(&self.store.research_config().default_agent);
        let inherited = stage.is_some_and(|i| self.store.research_config().agents[i].is_none());
        let title = agent.to_owned();
        let entity = cx.entity().downgrade();
        button(format!("model-agent-{}", stage.map(|i| i.to_string()).unwrap_or("global".into())), title)
            .w(px(140.)).flex_shrink_0()
            .tooltip(if inherited { "Uses the global agent" } else { "Choose the agent for this model" })
            .icon(assets::IconName::ChevronDown)
            .dropdown_menu(move |menu, _, _| {
                let open_entity = entity.clone();
                let codex_entity = entity.clone();
                menu.item(PopupMenuItem::new("OpenCode").on_click(move |_, w, cx| {
                    let _ = open_entity.update(cx, |this, cx| this.set_model_agent(stage, "OpenCode", w, cx));
                })).item(PopupMenuItem::new("Codex").on_click(move |_, w, cx| {
                    let _ = codex_entity.update(cx, |this, cx| this.set_model_agent(stage, "Codex", w, cx));
                }))
            }).into_any_element()
    }

    fn research_model_field(&self, input: &Entity<component::input::InputState>, id: String, cx: &mut Context<Self>) -> Div {
        let colors = palette(cx);
        let value = input.read(cx).value();
        let open_input = input.clone();
        let cancel_input = input.clone();
        let mut field = div().relative().flex_1().min_w_0()
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                this.open_research_model_picker(&open_input, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, w, cx| {
                // Escape only dismisses the suggestions or cancels the local
                // draft. Letting it bubble would move focus to the window root,
                // so Shift+Tab after dismissing suggestions would skip the
                // preceding provider control.
                if event.keystroke.key == "escape" {
                    this.finish_model_edit(&cancel_input, false, w, cx);
                    cx.stop_propagation();
                }
            }))
            .child(Input::new(input).id(id.clone())
                .suffix(component::Icon::new(assets::IconName::ChevronDown)
                    .size(px(14.)).text_color(rgb(colors.muted))));
        if self.research_model_picker == Some(input.entity_id()) {
            let input_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
            let measured_bounds = input_bounds.clone();
            field = field.child(canvas(move |bounds, _, _| measured_bounds.set(bounds), |_, _, _, _| {})
                .absolute().top_0().left_0().size_full());
            let dismiss_input = input.clone();
            let query = if value != self.research_model_initial { value.to_lowercase() } else { String::new() };
            let codex = self.model_field_agent(input) == "Codex";
            let models = if codex { &self.codex_models } else { &self.available_models };
            let loading = if codex { self.discovering_codex_models() } else { self.discovering_models() };
            let matches: Vec<_> = models.iter()
                .filter(|m| m.to_lowercase().contains(&query)).collect();
            let mut list = col().id(format!("{id}-options"))
                .max_h(px(190.)).overflow_y_scroll().occlude()
                .rounded_md().border_1().border_color(rgb(colors.line))
                .bg(rgb(colors.panel)).shadow_md().p_1()
                .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                    if !input_bounds.get().contains(&event.position)
                        && this.research_model_picker == Some(dismiss_input.entity_id()) {
                        this.finish_model_search(&dismiss_input, w, cx);
                    }
                }));
            if matches.is_empty() {
                list = list.child(label(if loading { "Loading local models…" }
                    else if models.is_empty() { "No models loaded. Refresh this agent or enter a model ID and press Return." }
                    else { "No matches. Enter a model ID and press Return to use it." }, cx));
            }
            for model in matches {
                let model = model.clone();
                let input = input.clone();
                list = list.child(div().id(format!("research-model-option-{model}"))
                    .flex_shrink_0().px_2().py_1().cursor_pointer()
                    .hover(|d| d.bg(rgb(colors.line))).child(selectable(model.clone()))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, w, cx| {
                        w.prevent_default();
                        cx.stop_propagation();
                        input.update(cx, |s, cx| s.set_value(model.clone(), w, cx));
                        this.finish_model_edit(&input, true, w, cx);
                    })).map(observed));
            }
            field = field.child(div().absolute().top_full().left_0().right_0()
                .child(deferred(div().on_scroll(|_, _, cx| cx.stop_propagation()).child(list.map(observed))).with_priority(1)));
        }
        field
    }

    pub fn research_settings_panel(&self, cx: &mut Context<Self>) -> Div {
        let colors = palette(cx);
        let mut settings = col().gap_3()
            .child(col().gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Global default")))
                .child(row().child(self.model_agent_picker(None, cx)).child(self.research_model_field(
                    &self.global_model, "research-global-model".into(), cx,
                ))))
            .child(col().pt_2().gap_1()
                .child(row().justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Research stage models")))
                    .child(button("global-model-all-stages", "Use global for all stages")
                        .disabled(self.research_models.iter().all(|m| m.read(cx).value().is_empty()) && self.store.research_config().agents.iter().all(Option::is_none))
                        .on_click(cx.listener(|this, _, w, cx| {
                            if this
                                .apply_domain_command_undoable(
                                    crate::scoped_domain::DomainCommand::ModelsToGlobal,
                                    "All stages now use the global model",
                                    cx,
                                )
                                .is_none()
                            {
                                return;
                            }
                            this.refresh_model_placeholders(w, cx);
                            for input in &this.research_models { input.update(cx, |s, cx| s.set_value("", w, cx)); }
                            this.research_model_picker = None;
                            cx.notify();
                        }))))
                .child(label("Choose an agent and model for each stage, or keep the global default.", cx)));
        for kind in ResearchKind::ALL {
            let i = kind.index();
            let value = self.research_models[i].read(cx).value();
            let field = self.research_model_field(&self.research_models[i], format!("research-model-{i}"), cx);
            settings = settings.child(col().gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(selectable(kind.label())))
                .child(row().child(self.model_agent_picker(Some(i), cx)).child(field)
                    .child(button(format!("research-use-global-{i}"), "Use global")
                        .disabled(value.is_empty() && self.store.research_config().agents[i].is_none())
                        .on_click(cx.listener(move |this, _, w, cx| this.use_global_for_stage(i, w, cx)))))
                .when(value.is_empty() && self.store.research_agent(kind) != self.store.research_config().default_agent, |d| {
                    d.child(label("Choose a model for this agent before running this stage.", cx))
                }));
        }
        settings=settings
            .child(div().pt_4().font_weight(FontWeight::SEMIBOLD).child(selectable("Agent connections")))
            .child(label("Local runtimes shared by all profiles.", cx))
            .child(self.agent_settings_panel(cx))
            .child(div().h_3().border_b_1().border_color(rgb(colors.line)))
            .child(div().pt_2().font_weight(FontWeight::SEMIBOLD).child(selectable("Browser · ego(lite)")))
            .child(Input::new(&self.ego_input).id("ego-path"))
            .child(row().flex_wrap().child(button("check-ego","Check runtime").disabled(self.ego_probe.is_some()).on_click(cx.listener(|this,_,_,cx|this.check_ego(cx)))).child(button("open-ego","Open browser").icon(assets::IconName::ExternalLink).on_click(cx.listener(|this,_,_,cx|this.open_ego(cx)))))
            .child(
                label(self.ego_status.clone(), cx)
                    .id("ego-status")
                    .aria_label(self.ego_status.clone())
                    .map(observed),
            )
            .when(!crate::ego::is_installed(&self.ego_input.read(cx).value()), |d| d.child(link_button("install-ego","Install ego(lite)").icon(assets::IconName::ExternalLink).on_click(|_,_,cx|cx.open_url("https://lite.ego.app"))))
            .child(label("Pages open in a local Space. Use the browser's Space menu to watch or take over. First launch must be completed in ego(lite).", cx));
        settings
    }

    pub fn materials_panel(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let task = self.store.task(id).unwrap();
        let archived = self.store.task_is_archived(id);
        let busy = self.attachment_import.is_some();
        let mut panel = col().id("task-materials").p_3().gap_1().rounded_md()
            .border_1().border_color(rgb(colors.line))
            .track_focus(&self.materials_focus)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| {
                w.focus(&this.materials_focus, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if !archived && event.keystroke.modifiers.platform && event.keystroke.key == "v"
                    && this.paste_attachments(id, cx) {
                    cx.stop_propagation();
                }
            }))
            .when(!archived, |d| d
                .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(rgb(colors.accent)))
                .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                    this.import_attachments(id, paths.paths().to_vec(), cx)
                })))
            .child(row().justify_between()
                .child(muted(format!("Materials · {}", task.attachments.len()), cx))
                .child(row().gap_2()
                    .child(quiet_button("add-attachments", if busy { "Adding…" } else { "Add Files" }).icon(assets::IconName::Plus)
                        .h_7().px_1().text_sm().disabled(archived || busy)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_attachments(id, cx))))));
        let hint = if archived {
            "Archived materials are read-only. Existing files remain available; Restore this research to add or remove materials."
        } else if task.attachments.is_empty() {
            "Drop PDFs or images here, or click here and press ⌘V to paste · up to 25 MB each"
        } else {
            "Drop files or ⌘V to add · click a card to open"
        };
        panel = panel.child(
            label(hint, cx)
                .id("material-drop-hint")
                .aria_label(hint)
                .map(observed),
        );
        if busy {
            let elapsed = self.attachment_started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            panel = panel.child(label(format!("{} · {elapsed}s elapsed", self.attachment_status), cx));
        }
        let mut cards = div().flex().flex_wrap().gap_2().min_w_0();
        for (index, attachment) in task.attachments.iter().enumerate() {
            let open = attachment.clone();
            let menu_attachment = attachment.clone();
            let menu_entity = cx.entity().downgrade();
            let archived = self.store.task_is_archived(id);
            let group: SharedString = format!("material-card-{index}").into();
            let is_pdf = attachment.media_type == "application/pdf";
            let format = match attachment.media_type.as_str() {
                "application/pdf" => "PDF",
                "image/jpeg" => "JPEG",
                "image/webp" => "WebP",
                _ => "PNG",
            };
            let size = if attachment.size < 1_048_576 {
                format!("{} KB", attachment.size.div_ceil(1024))
            } else {
                format!("{:.1} MB", attachment.size as f64 / 1_048_576.)
            };
            let path = self.data_dir().and_then(|root| crate::attachments::resolve(&root, attachment)).ok();
            let preview = div().w_full().h(px(72.)).p_1().flex().items_center().justify_center()
                .bg(rgb(colors.panel));
            let preview = if is_pdf {
                preview.child(col().items_center().gap_2()
                    .child(gpui_kit::component::Icon::new(assets::IconName::FileText)
                        .size(px(28.)).text_color(rgb(colors.accent)))
                    .child(label("PDF document", cx)))
            } else if let Some(path) = path {
                preview.child(img(path).w_full().h_full().object_fit(ObjectFit::Contain)
                    .with_fallback(move || div().text_xs().text_color(rgb(colors.muted))
                        .child(selectable("Preview unavailable")).into_any_element()))
            } else {
                preview.child(label("File unavailable", cx))
            };
            let full_name = attachment.name.clone();
            let card = col().id(format!("material-card-{index}"))
                .group(group.clone()).relative().w(px(176.)).max_w_full().flex_shrink_0()
                .rounded_lg().border_1().border_color(rgb(colors.line))
                .bg(rgb(colors.canvas)).hover(|s| s.border_color(rgb(colors.accent)))
                .child(col().id(format!("attachment-{index}"))
                    .rounded_lg().overflow_hidden().cursor_pointer()
                    .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(full_name.clone()).build(w, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_attachment(&open, cx)))
                    .child(preview)
                    .child(col().p_2().gap_0()
                        .child(single_line_text(format!("material-name-{index}"), attachment.name.clone())
                            .h_4().text_xs().text_color(rgb(colors.text)).flex_none())
                        .child(label(format!("{format} · {size}"), cx))))
                .child(div().absolute().top_2().right_2().opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .child(tool_icon(format!("material-menu-{index}"), assets::IconName::Ellipsis, "Material actions")
                        .bg(rgb(colors.canvas)).border_1().border_color(rgb(colors.line)).rounded_md()
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                            let open_entity = menu_entity.clone();
                            let remove_entity = menu_entity.clone();
                            let open_attachment = menu_attachment.clone();
                            let remove_path = menu_attachment.relative_path.clone();
                            menu.item(PopupMenuItem::new("Open in default app").on_click(move |_, _, cx| {
                                let _ = open_entity.update(cx, |this, cx| this.open_attachment(&open_attachment, cx));
                            })).separator()
                            .item(PopupMenuItem::element(move |_, _| {
                                button(
                                    format!("material-remove-{index}"),
                                    "Remove from goal",
                                )
                                .w_full()
                                .justify_start()
                                .into_any_element()
                            })
                            .disabled(archived)
                            .on_click(move |_, _, cx| {
                                let _ = remove_entity.update(cx, |this, cx| {
                                    if let Some(index) = this.store.task(id).and_then(|t| t.attachments.iter()
                                        .position(|a| a.relative_path == remove_path)) {
                                        this.remove_attachment(id, index, cx);
                                    }
                                });
                            }))
                        })));
            cards = cards.child(card.map(observed));
        }
        if !task.attachments.is_empty() {
            panel = panel.child(cards);
        }
        if let Some(parent) = task.parent_id.and_then(|p| self.store.task(p))
            && !parent.attachments.is_empty()
        {
            let pid = parent.id;
            panel = panel.child(
                link_button(
                    "parent-materials",
                    format!(
                        "Also includes {} material(s) from the parent",
                        parent.attachments.len()
                    ),
                )
                .icon(assets::IconName::ArrowRight)
                .on_click(cx.listener(move |this, _, w, cx| this.open_task(pid, w, cx))),
            );
        }
        panel.map(observed)
    }
    pub fn research_task_panel(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let task = self.store.task(id).unwrap();
        let research = task.research.as_ref().unwrap();
        let pending_approval = self.store.research_pending_approval(id);
        let children = self.store.research_children(id);
        let complete = !children.is_empty()
            && children
                .iter()
                .all(|child| self.store.research_result_available_or_pending(*child));
        let browser = quiet_button("task-open-ego", "Open ego(lite)").icon(assets::IconName::ExternalLink)
            .tooltip("Opens the browser app. Select the research Space in ego(lite).")
            .on_click(cx.listener(|this, _, _, cx| this.open_ego(cx)));
        let next_model = self.store.research_model(research.kind);
        let archived = self.store.task_is_archived(id);
        let history = archived.then(|| crate::archive_presentation::goal_history(&self.store, id));
        let mut panel = col().gap_2();
        if !archived {
            panel = panel.child(label(
                format!("Next run · {} · Model · {}", self.store.research_agent(research.kind), if next_model.is_empty() { "Not configured" } else { next_model }),
                cx,
            ));
        }
        if archived {
            panel = panel.child(
                col().p_4().gap_2().rounded_md().bg(rgb(colors.panel))
                    .child(chip("ARCHIVED", colors.muted, cx).id("archived-research-badge").map(observed))
                    .child(muted(
                        crate::archive_presentation::presentation(&self.store, id)
                            .map(|value| value.reason)
                            .unwrap_or_else(|| "Archived history. Restore to continue.".into()),
                        cx,
                    )),
            );
        }
        if let Some(run) = self.store.runs().iter().rev().find(|run| run.project_plan.is_none() && run.task_id == id) {
            panel = panel.child(label(
                format!("{} #{} · {} · Model · {}", if archived { "Latest retained run" } else { "Latest run" }, run.id, run.agent,
                    if run.model.is_empty() { "Not recorded" } else { run.model.as_str() }),
                cx,
            ));
        }
        if pending_approval && !archived {
            let goal = research.goal_id;
            panel = panel.child(col().p_4().gap_3().rounded_md().bg(rgb(colors.panel))
                .child(chip("AWAITING GOAL APPROVAL", colors.accent, cx))
                .child(muted("Review the complete plan and expected output. Move the goal to Todo to approve all its steps; click Run to execute.", cx))
                .when(research.kind == ResearchKind::Goal, |d| d.child(primary_button("approve-research-topic", "Move goal to Todo").disabled(archived)
                    .on_click(cx.listener(move |a, _, _, cx| a.approve_research_topic(goal, cx)))))
                .when(research.kind != ResearchKind::Goal, |d| d.child(button("review-parent-goal", "Review parent goal").on_click(cx.listener(move |a, _, w, cx| a.open_task(goal, w, cx))))));
        }

        if research.kind == ResearchKind::Goal {
            let completed = history
                .as_ref()
                .map(|history| history.completed_steps)
                .unwrap_or_else(|| {
                    children
                        .iter()
                        .filter(|child| self.store.research_result_available_or_pending(**child))
                        .count()
                });
            let complete = history
                .as_ref()
                .map(|history| history.complete)
                .unwrap_or(complete);
            let status = if children.is_empty() {
                "Research steps not planned yet".into()
            } else if archived {
                format!(
                    "Archived history · {completed}/{} steps with retained validated results",
                    children.len()
                )
            } else if complete {
                format!("Research complete · {completed}/{} steps", children.len())
            } else {
                format!("Research progress · {completed}/{} steps", children.len())
            };
            panel = panel.child(
                row()
                    .flex_wrap()
                    .justify_between()
                    .gap_2()
                    .child(muted(status, cx))
                    .child(row().flex_wrap().gap_2().child(browser).when(
                        !archived && !children.is_empty(),
                        |d| d.child(self.goal_run_button(id, "research-all", cx)),
                    )),
            );
        } else {
            let has_dependencies = !research.depends_on.is_empty();
            panel = panel
                .when(has_dependencies, |d| {
                    d.border_t_1()
                        .border_b_1()
                        .border_color(rgb(colors.line))
                        .py_2()
                })
                .child(
                    row()
                        .flex_wrap()
                        .justify_between()
                        .when(has_dependencies, |d| {
                            d.child(muted("Upstream research", cx))
                        })
                        .child(browser),
                )
                .when(has_dependencies, |d| {
                    d.child(label("This task uses the results of the steps below.", cx))
                });
            let waiting = research
                .depends_on
                .iter()
                .filter(|dep| self.store.research_result(**dep).is_none())
                .count();
            if waiting > 0 {
                panel = panel.child(label(
                    format!("Waiting for {waiting} earlier step(s) before this task can start."),
                    cx,
                ));
            }
            for dep in &research.depends_on {
                let dep = *dep;
                let title = self
                    .store
                    .task(dep)
                    .map(|t| t.title.as_str())
                    .unwrap_or("Missing dependency");
                let ready = self.store.research_result(dep).is_some();
                panel = panel.child(
                    link_button(
                        format!("research-dependency-{dep}"),
                        title.to_string(),
                    )
                    .icon(if ready { assets::IconName::Check } else { assets::IconName::Circle })
                    .disabled(self.store.task(dep).is_none())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_task(dep, w, cx))),
                );
            }
        }
        panel.into_any_element()
    }
    pub fn run_result_panel(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let presentation = crate::result_presentation::present_run(&self.store, run);
        let output = presentation.reader_markdown.clone();
        let sources_open = self.expanded_run_sources == Some(run.id);
        let archived_run = self.store.task_is_archived(run.task_id);
        let run_id = run.id;
        col()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .gap_1()
            .when(archived_run, |d| {
                d.child(label(
                    "Archived history · this retained result is available for reading. Restore the goal to resume work.",
                    cx,
                ))
            })
            .when(!archived_run && presentation.freshness.regenerating, |d| {
                d.child(label(
                    "A new version of this report is being generated from existing evidence. The current report remains available until it is replaced.",
                    cx,
                ))
            })
            .when(
                !archived_run && presentation.freshness.stale_input,
                |d| d.child(label(
                    "Historical result · task context or dependency results have changed. Run the task again to refresh.",
                    cx,
                )),
            )
            .when(
                !archived_run && presentation.freshness.historical && !presentation.freshness.stale_input && !presentation.freshness.regenerating,
                |d| d.child(label(
                    "Historical result · a newer result is available.",
                    cx,
                )),
            )
            .child(
                col()
                    .id("research-result-viewport")
                    .relative()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        col()
                        .id("research-result-scroll")
                        .track_scroll(&self.result_scroll)
                        .w_full()
                        .min_w_0()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .gap_3()
                        .p_4()
                        .bg(rgb(colors.panel))
                        .rounded_md()
                        .when(presentation.notice.is_some(), |d| {
                            d.child(notice_panel(
                                NoticeTone::Warning,
                                "Evidence gaps",
                                presentation.notice.unwrap_or(research::EVIDENCE_GAP_NOTICE),
                                cx,
                            ))
                        })
                        .child(
                            gpui_kit::base::text::TextView::markdown(
                                format!("result-{}", run.id),
                                if output.is_empty() {
                                    "No final report yet. This attempt did not produce a validated reader report.".into()
                                } else {
                                    output
                                },
                            )
                            .markdown_extensions(crate::rich_markdown::extensions())
                            .w_full()
                            .min_w_0()
                            .flex_shrink_0()
                            .on_link_click(|url, _, _, cx| {
                                if url.starts_with("https://") || url.starts_with("http://") {
                                    cx.open_url(url);
                                }
                            }),
                        )
                        .when(!presentation.sources.is_empty(), |d| d.child(
                            col().flex_shrink_0().border_t_1().border_color(rgb(colors.line)).pt_3()
                                .child(quiet_button("toggle-result-sources", format!("Sources · {}", presentation.sources.len()))
                                    .self_start().px_2().text_color(rgb(colors.muted))
                                    .icon(if sources_open { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                                    .tooltip(if sources_open { "Hide source links" } else { "Show source links and publication dates" })
                                    .on_click(cx.listener(move |a, _, _, cx| {
                                        a.expanded_run_sources = if sources_open { None } else { Some(run_id) }; cx.notify();
                                    })))
                                .when(sources_open, |d| d.child(col().id("result-sources").gap_3().pt_2()
                                    .children(presentation.sources.iter().enumerate().map(|(i, source)| {
                                        let url = source.url.clone();
                                        col().flex_shrink_0().gap_1()
                                            .child(link_button(format!("result-source-{i}"), source.title.clone()).icon(assets::IconName::ExternalLink)
                                                .justify_start().on_click(move |_, _, cx| cx.open_url(&url)))
                                            .child(label(source.url.clone(), cx))
                                            .child(label(format!("Published: {}", source.published_at), cx))
                                    })).map(observed)))
                        ))
                        .map(observed),
                    )
                    .vertical_scrollbar(&self.result_scroll)
                    .map(observed),
            )
            .into_any_element()
    }
}
