use crate::appearance::palette;
use crate::{app::TaskApp, domain::AgentRun, research::ResearchKind, views::*};
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
        let mut goals = col().gap_1();
        for task in self
            .store
            .tasks
            .iter()
            .filter(|t| {
                !self.store.task_is_archived(t.id)
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
                .filter(|id| self.store.research_result(**id).is_some())
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
                quiet_button(format!("research-goal-{id}"), "")
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
                                    .child(task.stage.symbol()),
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
                            .child(div().flex_shrink_0().child(label("Open ›", cx))),
                    ),
            );
        }
        let mut settings = col()
            .w_full()
            .max_w(px(850.))
            .flex_shrink_0()
            .gap_3()
            .p_5()
            .rounded_md()
            .bg(rgb(colors.panel));
        settings = settings
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Research models"),
            )
            .child(label(
                "Stages use the global model unless an override is entered below. Task status stays separate.",
                cx,
            ))
            .child(button("research-global-settings", "Global model settings →").on_click(cx.listener(|this, _, w, cx| this.navigate(crate::app::Scope::Settings, w, cx))));
        for kind in ResearchKind::ALL {
            let i = kind.index();
            settings = settings
                .child(
                    row()
                        .justify_between()
                        .child(label(kind.label(), cx))
                        .child(
                            button(format!("pick-research-model-{i}"), "Choose")
                                .disabled(self.available_models.is_empty())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.research_model_picker =
                                        if this.research_model_picker == Some(i) {
                                            None
                                        } else {
                                            Some(i)
                                        };
                                    cx.notify();
                                })),
                        ),
                )
                .child(Input::new(&self.research_models[i]).id(format!("research-model-{i}")))
                .child(
                    button(format!("research-use-global-{i}"), "Use global model")
                        .disabled(self.research_models[i].read(cx).value().is_empty())
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.research_models[i].update(cx, |s, cx| s.set_value("", w, cx));
                            cx.notify();
                        })),
                )
                .child(label(
                    self.model_source_label(self.research_models[i].read(cx).value().trim()),
                    cx,
                ));
            if self.research_model_picker == Some(i) {
                settings = settings.child(
                    col()
                        .id(format!("research-models-{i}"))
                        .h(px(180.))
                        .overflow_y_scroll()
                        .gap_1()
                        .children(
                            self.available_models
                                .iter()
                                .filter(|model| {
                                    let q = self.research_models[i].read(cx).value().to_lowercase();
                                    q.is_empty() || model.to_lowercase().contains(&q)
                                })
                                .map(|model| {
                                    let model = model.clone();
                                    button(format!("research-model-option-{model}"), model.clone())
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.research_models[i].update(cx, |s, cx| {
                                                s.set_value(model.clone(), w, cx)
                                            });
                                            this.research_model_picker = None;
                                            cx.notify();
                                        }))
                                }),
                        )
                        .map(observed),
                );
            }
        }
        settings=settings.child(button("research-refresh-models","Refresh local models").loading(self.discovering_models()).on_click(cx.listener(|this,_,_,cx|{this.connection_project=this.research_project;this.discover_models(cx);}))).child(label(self.connection_status.clone(), cx))
            .child(label("Save after changing overrides. PDF text and screenshot OCR are extracted locally for the selected model.", cx))
            .child(div().h_3().border_b_1().border_color(rgb(colors.line)))
            .child(div().pt_2().font_weight(FontWeight::SEMIBOLD).child("Browser · ego(lite)"))
            .child(Input::new(&self.ego_input).id("ego-path"))
            .child(row().flex_wrap().child(button("check-ego","Check runtime").disabled(self.ego_probe.is_some()).on_click(cx.listener(|this,_,_,cx|this.check_ego(cx)))).child(button("open-ego","Open browser ↗").on_click(cx.listener(|this,_,_,cx|this.open_ego(cx)))))
            .child(label(self.ego_status.clone(), cx))
            .child(link_button("install-ego","Install ego(lite) ↗").on_click(|_,_,cx|cx.open_url("https://lite.ego.app")))
            .child(label("Pages open in a local Space. Use the browser's Space menu to watch or take over. First launch must be completed in ego(lite).", cx))
            .child(primary_button("save-research-settings", "Save research settings").on_click(cx.listener(|this,_,_,cx|this.save_research_settings(cx))));
        let composer=col().w_full().max_w(px(850.)).min_w_0().gap_4()
            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Start with one sentence"))
            .child(muted("Describe what you want to understand and the output you need. We will draft a complete plan in Backlog for your review.", cx))
            .child(label("RESEARCH QUESTION", cx))
            .child(Input::new(&self.research_title).id("research-title"))
            .child(label("EXTRA CONTEXT · OPTIONAL", cx))
            .child(div().id("research-brief").child(Textarea::new(&self.research_brief)).map(observed))
            .child(label("PROJECT", cx))
            .child(row().flex_wrap().children(self.store.projects.iter().map(|p|{let id=p.id;select_button(format!("research-project-{id}"),p.name.clone(),self.research_project==id).on_click(cx.listener(move|this,_,_,cx|{this.research_project=id;this.research_topic=None;cx.notify();}))})))
            .child(label("TOPIC", cx))
            .child(self.topic_picker(None, self.research_project, cx))
            .child(row().flex_wrap().child(button("research-folder","Set working folder…").on_click(cx.listener(|this,_,w,cx|this.open_directory(this.research_project,w,cx)))).child(primary_button("create-research", "Generate research plan").on_click(cx.listener(|this,_,w,cx|this.create_research(w,cx)))))
            .child(label("Your request is saved first. Planning does not run the research. Move selected goals to Todo, then click Run.", cx))
            .child(div().h_4()).child(row().child(div().font_weight(FontWeight::SEMIBOLD).child("Your research goals")))
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
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(
                        if self.show_research_settings {
                            "Research settings"
                        } else {
                            "Market research"
                        },
                    ))
                    .when(!self.show_research_settings, |d| {
                        d.child(
                            button("research-settings-toggle", "Advanced settings…").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.show_research_settings = true;
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
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
                    .when(!self.show_research_settings, |d| d.child(composer))
                    .when(self.show_research_settings, |d| d.child(settings))
                    .map(observed),
            )
            .into_any_element()
    }
    pub fn materials_panel(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let task = self.store.task(id).unwrap();
        let busy = self.attachment_import.is_some();
        let mut panel = col().id("task-materials").p_3().gap_1().rounded_md()
            .border_1().border_color(rgb(colors.line))
            .track_focus(&self.materials_focus)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| {
                w.focus(&this.materials_focus, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.platform && event.keystroke.key == "v"
                    && this.paste_attachments(id, cx) {
                    cx.stop_propagation();
                }
            }))
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(rgb(colors.accent)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                this.import_attachments(id, paths.paths().to_vec(), cx)
            }))
            .child(row().justify_between()
                .child(muted(format!("Materials · {}", task.attachments.len()), cx))
                .child(row().gap_2()
                    .child(quiet_button("add-attachments", if busy { "Adding…" } else { "+ Add Files" })
                        .h_7().px_1().text_sm().disabled(task.archived || busy)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_attachments(id, cx))))))
            .child(label(if task.attachments.is_empty() {
                "Drop PDFs or images here, or click here and press ⌘V to paste · up to 25 MB each"
            } else {
                "Drop files or ⌘V to add · click a card to open"
            }, cx).id("material-drop-hint").map(observed));
        if busy {
            let elapsed = self.attachment_started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            panel = panel.child(label(format!("{} · {elapsed}s elapsed", self.attachment_status), cx));
        }
        let mut cards = div().flex().flex_wrap().gap_2().min_w_0();
        for (index, attachment) in task.attachments.iter().enumerate() {
            let open = attachment.clone();
            let menu_attachment = attachment.clone();
            let menu_entity = cx.entity().downgrade();
            let archived = task.archived;
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
                        .child("Preview unavailable").into_any_element()))
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
                            .item(PopupMenuItem::new("Remove from goal").disabled(archived)
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
                        "Also includes {} material(s) from the parent →",
                        parent.attachments.len()
                    ),
                )
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
                .all(|child| self.store.research_result(*child).is_some());
        let browser = quiet_button("task-open-ego", "Open ego(lite) ↗")
            .tooltip("Opens the browser app. Select the research Space in ego(lite).")
            .on_click(cx.listener(|this, _, _, cx| this.open_ego(cx)));
        let mut panel = col().gap_2();
        if pending_approval {
            let goal = research.goal_id;
            panel = panel.child(col().p_4().gap_3().rounded_md().bg(rgb(colors.panel))
                .child(chip("AWAITING GOAL APPROVAL", colors.accent, cx))
                .child(muted("Review the complete plan and expected output. Move the goal to Todo to approve all its steps; click Run to execute.", cx))
                .when(research.kind == ResearchKind::Goal, |d| d.child(primary_button("approve-research-topic", "Move goal to Todo").disabled(task.archived)
                    .on_click(cx.listener(move |a, _, _, cx| a.approve_research_topic(goal, cx)))))
                .when(research.kind != ResearchKind::Goal, |d| d.child(button("review-parent-goal", "Review parent goal").on_click(cx.listener(move |a, _, w, cx| a.open_task(goal, w, cx))))));
        }

        if research.kind == ResearchKind::Goal {
            let completed = children
                .iter()
                .filter(|child| self.store.research_result(**child).is_some())
                .count();
            let status = if children.is_empty() {
                "Research steps not planned yet".into()
            } else if complete {
                format!("✓ Research complete · {completed}/{} steps", children.len())
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
                        !children.is_empty(),
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
                        format!("{} {}", if ready { "✓" } else { "○" }, title),
                    )
                    .disabled(self.store.task(dep).is_none())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_task(dep, w, cx))),
                );
            }
        }
        panel.into_any_element()
    }
    pub fn run_result_panel(&self, run: &AgentRun, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let output = run.output.clone();
        let copy = output.clone();
        let sources_open = self.expanded_run_sources == Some(run.id);
        let run_id = run.id;
        col()
            .flex_1()
            .min_h_0()
            .gap_1()
            .when(run.research_kind.is_some_and(|kind|kind!=ResearchKind::Goal)&&run.status=="Succeeded"&&self.store.research_result(run.task_id).is_none_or(|latest|latest.id!=run.id),|d|d.child(label("Historical result · task context or dependency results have changed. Run the task again to refresh.", cx)))
            .child(
                row().justify_between()
                    .child(label(format!("{} sources · {} materials", run.sources.len(), run.attachments.len()), cx))
                    .child(tool_icon("copy-result", assets::IconName::Copy, "Copy result")
                        .disabled(output.is_empty())
                        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
            )
            .child(
                col()
                    .id("research-result-viewport")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        col()
                        .id("research-result-scroll")
                        .track_scroll(&self.result_scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .gap_3()
                        .p_4()
                        .bg(rgb(colors.panel))
                        .rounded_md()
                        .child(
                            gpui_kit::base::text::TextView::markdown(
                                format!("result-{}", run.id),
                                if output.is_empty() {
                                    "The result will appear here when the model responds.".into()
                                } else {
                                    output
                                },
                            )
                            .w_full()
                            .flex_shrink_0()
                            .on_link_click(|url, _, _, cx| {
                                if url.starts_with("https://") || url.starts_with("http://") {
                                    cx.open_url(url);
                                }
                            }),
                        )
                        .when(!run.sources.is_empty(), |d| d.child(
                            col().flex_shrink_0().border_t_1().border_color(rgb(colors.line)).pt_3()
                                .child(quiet_button("toggle-result-sources", format!("Sources · {}", run.sources.len()))
                                    .self_start().px_0().text_color(rgb(colors.muted))
                                    .icon(if sources_open { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                                    .tooltip(if sources_open { "Hide source links" } else { "Show source links and publication dates" })
                                    .on_click(cx.listener(move |a, _, _, cx| {
                                        a.expanded_run_sources = if sources_open { None } else { Some(run_id) }; cx.notify();
                                    })))
                                .when(sources_open, |d| d.child(col().id("result-sources").gap_3().pt_2()
                                    .children(run.sources.iter().enumerate().map(|(i, source)| {
                                        let url = source.url.clone();
                                        col().flex_shrink_0().gap_1()
                                            .child(link_button(format!("result-source-{i}"), format!("{} ↗", source.title))
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
