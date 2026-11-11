use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::palette,
    domain::Task,
    views::*,
};
use gpui_kit::{component::{Disableable, input::{InputEvent, Textarea, TextareaState}}, prelude::FluentBuilder, *};



fn clarification_context(text: &str) -> Div {
    let mut content = col().gap_2();
    let labels = ["Previous understanding:", "Clarification questions:", "User feedback:"];
    if !labels.iter().any(|label| text.starts_with(label)) {
        content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("User feedback")));
    }
    for line in text.lines() {
        if let Some(label) = labels.iter().find(|label| line.starts_with(**label)) {
            let value = line[label.len()..].trim_start();
            if *label == "Clarification questions:" && value.is_empty() { continue; }
            content = content.child(div().font_weight(FontWeight::SEMIBOLD)
                .child(selectable(label.trim_end_matches(':').to_string())));
            if !value.is_empty() {
                content = content.child(div().child(selectable(value.to_string())));
            }
        } else {
            content = content.child(div().child(selectable(line.to_string())));
        }
    }
    content
}

fn clarification_round(id: String, number: usize, content: Div, cx: &App) -> AnyElement {
    row().id(id.clone()).items_start().gap_3()
        .child(div().id(format!("{id}-number")).role(Role::Label).aria_label(format!("#{number}"))
            .w(px(32.)).flex_shrink_0().text_sm().font_weight(FontWeight::SEMIBOLD)
            .child(selectable(format!("#{number}"))).map(observed))
        .child(content.id(format!("{id}-content")).flex_1().min_w_0()
            .pl_4().border_l_2().border_color(rgb(palette(cx).subtle_line)).map(observed))
        .map(observed)
}

impl TaskApp {
    pub(crate) fn open_understanding_editor(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.store.research_requests().iter().find(|request| request.id == id) else { return; };
        let draft = request.understanding_draft.clone();
        let input = cx.new(|cx| TextareaState::new(window, cx).rows(3)
            .default_value(draft).placeholder("Answer a question or explain what should change…"));
        self.understanding_editor_subscription = Some(cx.subscribe(&input, move |a, input, event, cx| {
            if matches!(event, InputEvent::Change) && !a.understanding_submission_pending(id) {
                let value = input.read(cx).value().to_string();
                match a.store.save_understanding_draft(id, value) {
                    Ok(()) => { a.persist_if_changed(true); a.start_pump(cx); }
                    Err(error) => a.error = error,
                }
                cx.notify();
            }
        }));
        self.understanding_editor = Some((id, input));
        cx.notify();
    }

    fn understanding_submission_pending(&self, id: u64) -> bool {
        matches!(self.live_primary.as_ref().map(|pending| &pending.completion),
            Some(crate::live_save_app::LiveCompletion::ResearchUnderstanding { request, .. }) if *request == id)
    }

    fn close_understanding_editor(&mut self, cx: &mut Context<Self>) {
        if let Some((id, input)) = &self.understanding_editor {
            if self.understanding_submission_pending(*id) { return; }
            let value = input.read(cx).value().to_string();
            if let Err(error) = self.store.save_understanding_draft(*id, value) {
                self.error = error;
                cx.notify();
                return;
            }
            self.persist_if_changed(true);
            self.start_pump(cx);
        }
        self.understanding_editor = None;
        self.understanding_editor_subscription = None;
        cx.notify();
    }

    pub(crate) fn research_understanding_view(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(request) = self.store.research_requests().iter().find(|r| r.id == id) else {
            return div().into_any_element();
        };
        if !request.understanding_required { return div().into_any_element(); }
        let current_run = request.planning_run_id.and_then(|run| self.store.runs().iter().find(|r| r.id == run));
        let active = self.store.request_attempts(id).iter().any(|run|
            matches!(run.status.as_str(), "Queued" | "Running" | "Cancelling" | "Recovering"));
        let interrupted = current_run.is_some_and(|run| run.status == "Interrupted");
        let checked_stopped = current_run.is_some_and(|run|
            matches!(self.recovery.checks.get(&run.id), Some(crate::recovery::SessionCheck::Stopped)));
        let busy = active || self.live_has_pending_receipts();
        let confirmed = self.store.request_understanding_confirmed(id);
        let ready = self.store.request_understanding_ready(id);
        let colors = palette(cx);
        let mut body = col().gap_3().text_sm()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                .child(selectable(if ready { "Current understanding" } else { "Research understanding" })));
        if let Some(understanding) = request.understanding.as_ref().filter(|_| ready) {
            body = body.child(div().text_sm().child(selectable(understanding.summary.clone())))
                .when(!understanding.questions.is_empty(), |d| d.child(col().gap_2().pt_2()
                    .child(label("TO CLARIFY", cx))
                    .children(understanding.questions.iter().enumerate().map(|(index, question)|
                        div().pl_3().border_l_2().border_color(rgb(colors.warning))
                            .child(selectable(format!("{}. {question}", index + 1)))))));
            if confirmed {
                body = body.child(muted("Confirmed · the plan follows this understanding.", cx));
            } else if !understanding.questions.is_empty() {
                body = body.child(label("Answer these questions with Refine understanding before confirming.", cx));
            }
        } else {
            body = body.child(button(format!("understanding-retry-{id}"),
                    if active { "Understanding your request…" }
                    else if interrupted { "Resume understanding" } else { "Understand research" })
                .disabled(busy || (interrupted && !checked_stopped))
                .on_click(cx.listener(move |a, _, _, cx| {
                    if interrupted { a.resume_request_understanding(id, cx); }
                    else { a.retry_research_request_without_navigation(id, cx); }
                })))
                .when(!active || current_run.is_some_and(|run| run.status == "Recovering"), |d| d.child(muted(if current_run.is_some_and(|run| run.status == "Recovering") {
                    "The previous session is still running. Waiting for it without starting another request."
                } else if interrupted && !checked_stopped {
                    "Checking the previous session before resuming. Your submitted feedback is preserved."
                } else if interrupted {
                    "Understanding was interrupted. Resume with the saved request and feedback."
                } else { "Generate a short understanding first, then confirm the direction." }, cx)))
                .when(interrupted && !checked_stopped, |d| d.child(
                    button(format!("understanding-check-{id}"), "Check session status")
                        .on_click(cx.listener(|a, _, _, cx| a.check_interrupted_sessions(cx)))));
        }
        if !confirmed {
            if let Some((editor_id, input)) = &self.understanding_editor && *editor_id == id {
                let feedback = input.clone();
                body = body.child(div().id(format!("understanding-feedback-{id}"))
                        .child(Textarea::new(input).disabled(self.understanding_submission_pending(id))))
                    .child(muted(if !self.storage_error.is_empty() { "Draft could not be saved. Keep this window open and retry storage saving." }
                        else { "Draft is saved automatically. Update understanding submits this round." }, cx))
                    .child(row().gap_2().flex_wrap()
                        .child(primary_button(format!("understanding-refine-{id}"), "Update understanding")
                            // Background draft receipts do not block a primary submission.
                            .disabled(active || self.live_primary.is_some() || interrupted)
                            .on_click(cx.listener(move |a, _, _, cx| {
                                let value = feedback.read(cx).value().to_string();
                                a.save_research_understanding(id, Some(value), cx);
                            })))
                        .child(button(format!("understanding-cancel-{id}"), "Close editor")
                            .disabled(self.understanding_submission_pending(id))
                            .on_click(cx.listener(|a, _, _, cx| a.close_understanding_editor(cx)))));
            } else {
                body = body.child(row().gap_2().flex_wrap()
                    .when(ready, |d| d.child(primary_button(format!("understanding-confirm-{id}"), "Confirm and generate plan")
                        .disabled(busy || !request.understanding_draft.trim().is_empty()
                            || request.understanding.as_ref().is_some_and(|u| !u.questions.is_empty()))
                        .on_click(cx.listener(move |a, _, _, cx| a.save_research_understanding(id, None, cx)))))
                    .child(button(format!("understanding-edit-{id}"),
                        if request.understanding_draft.is_empty() { "Refine understanding" } else { "Continue editing" })
                        .disabled(busy)
                        .on_click(cx.listener(move |a, _, window, cx| a.open_understanding_editor(id, window, cx)))));
                if !request.understanding_draft.trim().is_empty() {
                    body = body.child(muted("Unsubmitted draft · continue editing to submit or clear it before confirming.", cx))
                        .child(div().child(selectable(request.understanding_draft.clone())));
                }
            }
        }
        body = col().gap_4().child(body.p_4().rounded(px(crate::appearance::GROUP_RADIUS))
            .bg(rgb(colors.side)).border_1().border_color(rgb(colors.subtle_line)));
        let has_history = !request.understanding_turns.is_empty() || !request.understanding_feedback.is_empty();
        if has_history {
            let expanded = self.understanding_history_expanded == Some(id);
            let caption = if request.understanding_turns.is_empty() { "Clarification history".to_string() }
                else { format!("Clarification history · {} round{}", request.understanding_turns.len(),
                    if request.understanding_turns.len() == 1 { "" } else { "s" }) };
            body = body.child(quiet_button(format!("understanding-history-{id}"), caption)
                .icon(if expanded { assets::IconName::ChevronDown } else { assets::IconName::ChevronRight })
                .self_start().on_click(cx.listener(move |a, _, _, cx| {
                    a.understanding_history_expanded = if expanded { None } else { Some(id) };
                    cx.notify();
                })));
            if expanded {
                let history = col().id(format!("understanding-turns-{id}")).ml_4().gap_4().flex_shrink_0();
                // Older versions saved only combined context; expose any prefix that
                // predates structured rounds without inventing timestamps.
                let recorded = request.understanding_turns.iter().map(|turn| {
                    if let Some(understanding) = &turn.previous_understanding {
                        format!("Previous understanding: {}\nClarification questions: {}\nUser feedback: {}",
                            understanding.summary, understanding.questions.join("\n"), turn.feedback)
                    } else { turn.feedback.clone() }
                }).collect::<Vec<_>>().join("\n\n");
                let earlier = request.understanding_feedback.strip_suffix(&recorded)
                    .unwrap_or(&request.understanding_feedback).trim();
                body = body.child(if request.understanding_turns.is_empty() {
                    history.child(clarification_round(format!("understanding-earlier-{id}"), 0,
                        clarification_context(&request.understanding_feedback), cx))
                } else {
                    history.children(request.understanding_turns.iter().enumerate().rev().map(|(index, turn)| {
                        let date = crate::time_display::timestamp(&turn.submitted_at);
                        clarification_round(format!("understanding-round-{id}-{index}"),
                            index + usize::from(!earlier.is_empty()), col().gap_2()
                            .child(label(date, cx))
                            .when_some(turn.previous_understanding.as_ref(), |d, understanding| d
                                .child(col().id(format!("understanding-snapshot-{id}-{index}")).gap_2()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Previous understanding")))
                                    .child(div().child(selectable(understanding.summary.clone())))
                                    .when(!understanding.questions.is_empty(), |d| d
                                        .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("Clarification questions")))
                                        .children(understanding.questions.iter().map(|q| div().child(selectable(q.clone())))))
                                    .map(observed)))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(selectable("User feedback")))
                            .child(div().id(format!("understanding-feedback-history-{id}-{index}"))
                                .child(selectable(turn.feedback.clone())).map(observed)), cx)
                    }))
                    .when(!earlier.is_empty(), |d| d.child(clarification_round(
                        format!("understanding-earlier-{id}"), 0, clarification_context(earlier), cx)))
                }.map(observed));
            }
        }
        body.into_any_element()
    }

    pub(crate) fn topic_grouped_cards(
        &self,
        cards: &[Task],
        group: &str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if matches!(self.scope(), Scope::Topic(_)) {
            return cards.iter().map(|t| self.goal_card(t, cx)).collect();
        }
        let mut result = Vec::new();
        let mut topics: Vec<_> = self
            .store
            .topics()
            .iter()
            .filter(|topic| topic.automation_id.is_none())
            .collect();
        topics.sort_by_key(|topic| (crate::display_order::newest_id(topic.project_id), crate::display_order::newest_id(topic.id)));
        for topic in topics {
            let goals: Vec<_> = cards
                .iter()
                .filter(|t| {
                    self.store
                        .task_topic(t.id)
                        .is_some_and(|v| v.id == topic.id)
                })
                .collect();
            if goals.is_empty() {
                continue;
            }
            let id = topic.id;
            let colors = palette(cx);
            result.push(
                col()
                    .id(format!("board-topic-group-{}-{id}", group))
                    .p_2()
                    .gap_2()
                    .rounded(px(crate::appearance::SURFACE_RADIUS))
                    .border_1()
                    .border_color(rgb(colors.subtle_line))
                    .bg(rgb(colors.panel))
                    .child(row_button(format!("board-topic-{}-{id}", group), "")
                        .w_full().min_w_0().px_1()
                        .tooltip(topic.display_name())
                        .accessibility_label(topic.display_name())
                        .child(single_line_text(
                            format!("board-topic-title-{}-{id}", group), topic.display_name()))
                        .child(label(goals.len().to_string(), cx))
                        .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(id), w, cx))))
                    .children(goals.into_iter().map(|t| self.goal_card(t, cx)))
                    .map(observed)
                    .into_any_element(),
            );
        }
        // Corrupt/imported legacy roots stay visible while migration is resolved.
        result.extend(
            cards
                .iter()
                .filter(|t| self.store.task_topic(t.id).is_none())
                .map(|t| self.goal_card(t, cx)),
        );
        result
    }

    pub(crate) fn request_history(
        &self,
        project: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let mut list = col().gap_3();
        for request in self
            .store
            .research_requests()
            .iter()
            .rev()
            .filter(|r| project.is_none_or(|id| r.project_id == id)
                && r.automation_id.is_none()
                && !self.store.request_is_archived(r)
                && match self.scope() { Scope::Topic(id) => r.topic_id == Some(id) && (r.goal_ids.is_empty() || self.store.can_regenerate_request_plan(r.id)), _ => true })
        {
            let id = request.id;
            let project = request.project_id;
            let run = request
                .planning_run_id
                .and_then(|id| self.store.runs().iter().find(|r| r.id == id));
            let active = run
                .is_some_and(|r| matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling" | "Recovering"));
            let planned = !request.goal_ids.is_empty();
            let needs_understanding = request.understanding_required && !self.store.request_understanding_confirmed(id);
            let awaiting_confirmation = needs_understanding && self.store.request_understanding_ready(id);
            let status = if active && needs_understanding {
                "Understanding request…"
            } else if awaiting_confirmation {
                "Understanding ready to review"
            } else if active {
                "Planning…"
            } else if planned {
                "Plan generated"
            } else {
                run.map(|r| r.status.as_str())
                    .unwrap_or("Saved · not planned")
            };
            list = list.child(
                col()
                    .gap_2()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(colors.line))
                    .child(div().child(selectable(request.input.clone())))
                    .child(row().gap_3().flex_wrap()
                        .child(label(format!("{} · {status}", crate::time_display::timestamp(&request.submitted_at)), cx))
                        .when_some(run.map(|r| r.id), |d, run| d.child(
                            quiet_button(format!("request-run-{id}"), "Planning details").h_6().text_xs()
                                .on_click(cx.listener(move |a, _, w, cx| {
                                    a.set_result_selection(None, Some(run));
                                    a.show_run_result = true;
                                    cx.notify();
                                })))))
                    .child(self.research_understanding_view(id, cx))
                    .when(!needs_understanding || planned, |d| d.child(
                        row()
                            .flex_wrap()
                            .when(!needs_understanding && (!planned || self.store.can_regenerate_request_plan(id)), |d| {
                                d.child(
                                    button(
                                        format!("retry-request-{id}"),
                                        if planned {
                                            "Regenerate plan"
                                        } else if run.is_some() {
                                            "Retry planning"
                                        } else {
                                            "Generate plan"
                                        },
                                    )
                                    .disabled(active)
                                    .tooltip(if planned { "Regenerates all unapproved goals from this request. The current plan stays available until a valid replacement is ready; previous goals are archived." } else { "Generate a reviewable plan from this saved request." })
                                    .on_click(cx.listener(
                                        move |a, _, _, cx| a.retry_research_request(id, cx),
                                    )),
                                )
                            })
                            .when(planned, |d| {
                                d.child(
                                    button(format!("open-request-{id}"), "Review goals").on_click(
                                        cx.listener(move |a, _, w, cx| {
                                            a.navigate(Scope::Project(project), w, cx);
                                            a.layout = Layout::Board;
                                            cx.notify();
                                        }),
                                    ),
                                )
                            }),
                    )),
            );
        }
        col()
            .gap_2()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(selectable("Saved research requests")),
            )
            .child(list)
            .into_any_element()
    }

    pub(crate) fn request_brief_view(&self, goal: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(request) = self
            .store
            .research_requests()
            .iter()
            .find(|r| r.goal_ids.contains(&goal))
        else {
            return div().into_any_element();
        };
        let Some(brief) = &request.brief else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let request_id = request.id;
        let regenerating = request.planning_run_id.is_some_and(|id| self.store.runs().iter()
            .any(|r| r.id == id && matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling")));
        let expanded = self.expanded_request_brief == Some(goal);
        col()
            .gap_2()
            .child(row().gap_2().flex_wrap().child(
                quiet_button("request-brief-toggle", "Original request & planning brief")
                    .self_start()
                    .icon(if expanded {
                        assets::IconName::ChevronDown
                    } else {
                        assets::IconName::ChevronRight
                    })
                    .on_click(cx.listener(move |a, _, _, cx| {
                        a.expanded_request_brief = if expanded { None } else { Some(goal) };
                        cx.notify();
                    })),
            )
            .when(self.store.can_regenerate_request_plan(request_id), |d| d.child(
                quiet_button("regenerate-request-plan", if regenerating { "Regenerating plan…" } else { "Regenerate plan" })
                    .disabled(regenerating)
                    .tooltip("Regenerates all Backlog goals from the original request. Previous goals are archived only after the new plan is valid.")
                    .on_click(cx.listener(move |a, _, _, cx| a.retry_research_request(request_id, cx))))))
            .when(expanded, |d| {
                d.child(
                    col()
                        .id("request-brief-content")
                        .gap_3()
                        .pl_4()
                        .border_l_1()
                        .border_color(rgb(colors.line))
                        .child(label(
                            format!("ORIGINAL REQUEST · {}", crate::time_display::timestamp(&request.submitted_at)),
                            cx,
                        ))
                        .child(div().child(selectable(request.input.clone())))
                        .child(label("EXPECTED OUTPUT", cx))
                        .child(div().child(selectable(brief.deliverable.clone())))
                        .child(label("OBJECTIVE & SCOPE", cx))
                        .child(div().child(selectable(brief.objective.clone())))
                        .child(div().child(selectable(brief.scope.clone())))
                        .when(!brief.assumptions.is_empty(), |d| {
                            d.child(label("PLANNING ASSUMPTIONS", cx)).children(
                                brief
                                    .assumptions
                                    .iter()
                                    .map(|s| div().child(selectable(format!("• {s}")))),
                            )
                        })
                        .when(!brief.facts_to_verify.is_empty(), |d| {
                            d.child(label("FACTS TO VERIFY DURING RESEARCH", cx))
                                .children(
                                    brief
                                        .facts_to_verify
                                        .iter()
                                        .map(|s| div().child(selectable(format!("• {s}")))),
                                )
                        })
                        .map(observed),
                )
            })
            .into_any_element()
    }
}
