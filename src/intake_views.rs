use crate::{
    app::{Layout, Scope, TaskApp},
    appearance::palette,
    domain::{Stage, Task},
    views::*,
};
use gpui_kit::{component::Disableable, prelude::FluentBuilder, *};

impl TaskApp {
    pub(crate) fn topic_grouped_cards(
        &self,
        cards: &[Task],
        stage: Stage,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if matches!(self.scope, Scope::Topic(_)) {
            return cards.iter().map(|t| self.goal_card(t, cx)).collect();
        }
        let mut result = Vec::new();
        let mut topics: Vec<_> = self.store.topics.iter().collect();
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
            result.push(
                quiet_button(
                    format!("board-topic-{}-{id}", stage.key()),
                    topic.display_name(),
                )
                .px_0()
                .self_start()
                .on_click(cx.listener(move |a, _, w, cx| a.navigate(Scope::Topic(id), w, cx)))
                .into_any_element(),
            );
            result.extend(goals.into_iter().map(|t| self.goal_card(t, cx)));
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
            .research_requests
            .iter()
            .rev()
            .filter(|r| project.is_none_or(|id| r.project_id == id) && !self.store.request_is_archived(r))
        {
            let id = request.id;
            let project = request.project_id;
            let run = request
                .planning_run_id
                .and_then(|id| self.store.runs.iter().find(|r| r.id == id));
            let active = run
                .is_some_and(|r| matches!(r.status.as_str(), "Queued" | "Running" | "Cancelling"));
            let planned = !request.goal_ids.is_empty();
            let status = if planned {
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
                    .child(div().child(request.input.clone()))
                    .child(label(format!("{} · {status}", request.submitted_at), cx))
                    .child(
                        row()
                            .flex_wrap()
                            .when(!planned, |d| {
                                d.child(
                                    button(
                                        format!("retry-request-{id}"),
                                        if run.is_some() {
                                            "Retry planning"
                                        } else {
                                            "Generate plan"
                                        },
                                    )
                                    .disabled(active)
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
                            })
                            .when_some(run.map(|r| r.id), |d, run| {
                                d.child(
                                    button(format!("request-run-{id}"), "Planning details")
                                        .on_click(cx.listener(move |a, _, w, cx| {
                                            a.navigate(Scope::Runs, w, cx);
                                            a.selected_run = Some(run);
                                            a.show_run_result = true;
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            );
        }
        col()
            .gap_2()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Saved research requests"),
            )
            .child(list)
            .into_any_element()
    }

    pub(crate) fn request_brief_view(&self, goal: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(request) = self
            .store
            .research_requests
            .iter()
            .find(|r| r.goal_ids.contains(&goal))
        else {
            return div().into_any_element();
        };
        let Some(brief) = &request.brief else {
            return div().into_any_element();
        };
        let colors = palette(cx);
        let expanded = self.expanded_request_brief == Some(goal);
        col()
            .gap_2()
            .child(
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
            .when(expanded, |d| {
                d.child(
                    col()
                        .id("request-brief-content")
                        .gap_3()
                        .pl_4()
                        .border_l_1()
                        .border_color(rgb(colors.line))
                        .child(label(
                            format!("ORIGINAL REQUEST · {}", request.submitted_at),
                            cx,
                        ))
                        .child(div().child(request.input.clone()))
                        .child(label("EXPECTED OUTPUT", cx))
                        .child(div().child(brief.deliverable.clone()))
                        .child(label("OBJECTIVE & SCOPE", cx))
                        .child(div().child(brief.objective.clone()))
                        .child(div().child(brief.scope.clone()))
                        .when(!brief.assumptions.is_empty(), |d| {
                            d.child(label("PLANNING ASSUMPTIONS", cx)).children(
                                brief
                                    .assumptions
                                    .iter()
                                    .map(|s| div().child(format!("• {s}"))),
                            )
                        })
                        .when(!brief.facts_to_verify.is_empty(), |d| {
                            d.child(label("FACTS TO VERIFY DURING RESEARCH", cx))
                                .children(
                                    brief
                                        .facts_to_verify
                                        .iter()
                                        .map(|s| div().child(format!("• {s}"))),
                                )
                        })
                        .map(observed),
                )
            })
            .into_any_element()
    }
}
