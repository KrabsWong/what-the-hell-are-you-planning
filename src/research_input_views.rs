//! T21 Phase 2 UI: edit-impact confirmation and retained-version history.
//!
//! Rendering only. Confirmed saves go back through `research_input_app` to the
//! existing save path; this module owns no mutation or persistence.

use crate::app::TaskApp;
use crate::appearance::palette;
use crate::controls::{button, primary_button, quiet_button, row_button, tool_icon};
use crate::research_input_app::{self, step_kind_label};
use crate::views::{col, label, muted, observed, row, selectable};
use gpui_kit::base::FocusTrapElement;
use gpui_kit::component::Disableable;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(crate) fn edit_impact_note(app: &TaskApp, id: u64, cx: &App) -> Option<AnyElement> {
    let task = app.store.task(id)?;
    let colors = palette(cx);
    let (heading, body) = if crate::research_input::automation_owned_task(&app.store, id) {
        (
            "Automation period input",
            "Period execution inputs are frozen. Edit the automation definition and replan future periods; labels, due date and cycle remain metadata.",
        )
    } else if task.research.is_some() {
        (
            "Research input",
            "Title, description, project, comments and materials change the research input. Moving the stage to Backlog or Cancelled withdraws approval as a lifecycle action. Labels, priority, due date and cycle are presentation metadata.",
        )
    } else {
        return None;
    };
    Some(
        div()
            .id("research-input-note")
            .p_3()
            .gap_1()
            .rounded(px(crate::appearance::GROUP_RADIUS))
            .bg(rgb(colors.panel))
            .border_1()
            .border_color(rgb(colors.border_strong))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(colors.warning))
                    .child(heading),
            )
            .child(muted(body, cx))
            .map(observed)
            .into_any_element(),
    )
}

pub(crate) fn save_impact_dialog(app: &TaskApp, cx: &mut Context<TaskApp>) -> AnyElement {
    let Some(pending) = app.research_save_impact.as_ref() else {
        return div().into_any_element();
    };
    let colors = palette(cx);
    let comment_review = matches!(
        pending.prepared,
        crate::research_input_app::PendingPreparedSave::Comment(_)
    );
    let material_review = matches!(
        pending.prepared,
        crate::research_input_app::PendingPreparedSave::Materials(_)
    );
    let material_add = matches!(
        pending.prepared,
        crate::research_input_app::PendingPreparedSave::Materials(
            crate::material_app::PreparedMaterialSave::Add(_)
        )
    );
    let impact = &pending.impact;
    let mut rows = col().gap_2();
    for step in &impact.steps {
        if step.input_state == crate::research_input::StepInputState::Unchanged {
            continue;
        }
        let state = match step.input_state {
            crate::research_input::StepInputState::Changed => "Changed input",
            crate::research_input::StepInputState::DownstreamAffected => "Downstream input",
            crate::research_input::StepInputState::Unchanged => unreachable!(),
        };
        rows = rows.child(
            div()
                .flex()
                .gap_2()
                .child(label(state, cx))
                .child(selectable(step.title.clone())),
        );
    }
    let stage_only = impact.review_required
        && impact.changed_input_steps.is_empty()
        && impact.affected_input_steps.is_empty();
    let mut details = col().gap_2();
    if stage_only {
        details = details.child(muted(
            "This stage change withdraws approval. No research input revision changed; execution waits until the plan is approved again.",
            cx,
        ));
    } else {
        details = details
            .child(muted(
                if impact.review_required {
                    "Saving withdraws approval for this goal. All execution waits until you move it to Todo again."
                } else {
                    "This goal is already awaiting review. The changed input graph still needs an explanation."
                },
                cx,
            ))
            .child(rows);
    }
    details = details.child(muted(
        format!(
            "{} current result(s) will wait for review; {} changed-input result(s); {} unchanged sibling result(s) keep matching inputs.",
            impact.current_results,
            impact.input_changed_results,
            impact.retained_matching_results
        ),
        cx,
    ));
    // This is a save consequences review, not a plan-review action. The goal
    // stays in its existing route and becomes review-pending through the normal
    // save; never imply that this button navigates to plan review.
    let primary_label = if comment_review {
        "Post comment"
    } else if material_review {
        if material_add {
            "Add material"
        } else {
            "Remove material"
        }
    } else {
        "Save changes"
    };
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(rgba(0x000000a0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            col()
                .id("research-save-impact")
                .w(px(640.))
                .max_h(relative(0.88))
                .overflow_y_scroll()
                .p_6()
                .gap_4()
                .map(|d| crate::appearance::floating_surface(d, cx))
                .border_color(rgb(colors.warning))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if comment_review {
                            "Review impact before posting"
                        } else if material_review {
                            "Review impact before changing materials"
                        } else {
                            "Review impact before saving"
                        }),
                )
                .child(label("NOTHING HAS BEEN SAVED YET", cx))
                .child(div().child(selectable(impact.goal_title.clone())))
                .child(details)
                .child(muted(
                    if comment_review {
                        "The comment changes the research input. Previous reports stay readable; execution waits for renewed approval."
                    } else if material_review {
                        "Materials change the research input. Previous reports stay readable; execution waits for renewed approval."
                    } else {
                        "Previous reports stay readable. Execution does not resume until the updated plan is approved."
                    },
                    cx,
                ))
                .when(!app.error.is_empty(), |dialog| {
                    dialog.child(
                        div()
                            .text_color(rgb(colors.danger))
                            .child(selectable(app.error.clone())),
                    )
                })
                .child(
                    row()
                        .justify_end()
                        .gap_3()
                        .child(
                            button("research-save-impact-cancel", "Cancel")
                                .track_focus(&app.dialog_first_focus)
                                .on_click(cx.listener(|app, _, w, cx| {
                                    app.cancel_research_save(w, cx);
                                })),
                        )
                        .child(
                            primary_button("research-save-impact-confirm", primary_label).on_click(
                                cx.listener(|app, _, w, cx| {
                                    let _ = app.confirm_research_save(w, cx);
                                }),
                            ),
                        ),
                )
                .map(observed),
        )
        .track_focus(&app.modal_focus)
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, w, cx| {
            if event.keystroke.key == "escape" {
                app.cancel_research_save(w, cx);
                cx.stop_propagation();
            }
        }))
        .focus_trap("research-save-impact-focus", &app.modal_focus)
        .into_any_element()
}

pub(crate) fn previous_versions_dialog(app: &TaskApp, cx: &mut Context<TaskApp>) -> AnyElement {
    let Some(task) = app.previous_versions_task else {
        return div().into_any_element();
    };
    let colors = palette(cx);
    let entries = app.previous_version_entries(task);
    let selected = app
        .previous_versions_run
        .or_else(|| entries.first().map(|entry| entry.version.run_id));
    let selected_entry =
        selected.and_then(|run| entries.iter().find(|entry| entry.version.run_id == run));
    let history = selected.and_then(|run| {
        selected_entry.and_then(|entry| {
            app.store
                .runs()
                .iter()
                .find(|candidate| candidate.id == run)
                .map(|run| research_input_app::history_presentation(&app.store, entry, run))
        })
    });
    let mut list = col()
        .id("previous-version-list")
        .gap_1()
        .h(px(210.))
        .flex_shrink_0()
        .overflow_y_scroll();
    for entry in &entries {
        let run = entry.version.run_id;
        let active = selected == Some(run);
        let status = if entry.version.execution_admissible {
            "Current execution result"
        } else if entry.version.inputs_match_current {
            "Inputs unchanged · retained"
        } else {
            "Previous input revision"
        };
        let digest = entry
            .version
            .revision_digest
            .as_deref()
            .unwrap_or("revision unavailable");
        let recorded = crate::time_display::timestamp(if entry.version.started_at.is_empty() {
            "time unavailable"
        } else {
            entry.version.started_at.as_str()
        });
        list = list.child(
            row_button(format!("previous-version-row-{run}"), "")
                .custom(
                    ButtonCustomVariant::new(cx)
                        .foreground(rgb(colors.text).into())
                        .hover(rgb(colors.hover).into())
                        .active(rgb(colors.hover).into()),
                )
                .w_full()
                .h_auto()
                .px_3()
                .py_2()
                .rounded(px(crate::appearance::CONTROL_RADIUS))
                .selected(active)
                .when(active, |button| button.bg(rgb(colors.accent_surface)))
                .tooltip(format!("Recorded {recorded} · input revision {digest}"))
                .accessibility_label(format!(
                    "{} · {} · Run #{run} · {recorded} · {status}",
                    entry.task_title,
                    step_kind_label(entry.kind)
                ))
                .child(
                    col()
                        .w_full()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .w_full()
                                .whitespace_normal()
                                .font_weight(FontWeight::MEDIUM)
                                .child(entry.task_title.clone()),
                        )
                        .child(label(
                            format!(
                                "{} · Run #{run} · {recorded} · {status}",
                                step_kind_label(entry.kind)
                            ),
                            cx,
                        )),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.select_previous_version(run, cx);
                })),
        );
    }
    let title = app
        .store
        .task(task)
        .map(|task| task.title.clone())
        .unwrap_or_else(|| "Research".into());
    let mut body = col()
        .id("previous-version-body-content")
        .w_full()
        .flex_shrink_0()
        .gap_2();
    if let Some(history) = &history {
        let facts = history.facts();
        if !facts.is_empty() {
            body = body.child(
                label(facts.join(" · "), cx)
                    .id("previous-version-facts")
                    .aria_label(facts.join(" · "))
                    .map(observed),
            );
        }
        body = body
            .child(div().child(selectable(history.context.clone())))
            .child(
                div()
                    .id(format!(
                        "previous-version-report-{}",
                        selected.unwrap_or_default()
                    ))
                    .w_full()
                    .flex_shrink_0()
                    .child(if let Some(state) = app.previous_version_text.as_ref() {
                        gpui_kit::base::text::TextView::new(state)
                            .markdown_extensions(crate::rich_markdown::extensions())
                            .scrollable(true)
                            .h(px(140.))
                            .w_full()
                            .on_link_click(|url, _, _, cx| {
                                if url.starts_with("https://") || url.starts_with("http://") {
                                    cx.open_url(url);
                                }
                            })
                            .into_any_element()
                    } else {
                        div()
                            .h(px(140.))
                            .child(muted("No retained report is selected.", cx))
                            .into_any_element()
                    })
                    .map(observed),
            );
        if !history.presentation.sources.is_empty() {
            body = body.child(label(
                format!(
                    "{} source(s) recorded · included in Copy and Export",
                    history.presentation.sources.len()
                ),
                cx,
            ));
        }
    } else {
        body = body.child(muted("No retained result is selected.", cx));
    }
    let run = selected.unwrap_or_default();
    let mut actions = row().justify_end().gap_2();
    if selected.is_some() {
        actions = actions
            .child(
                button("previous-version-copy", "Copy report")
                    .disabled(history.is_none())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.copy_previous_version(run, cx);
                    })),
            )
            .child({
                let entity = cx.entity().downgrade();
                tool_icon(
                    "previous-version-more",
                    assets::IconName::Ellipsis,
                    "More retained report actions",
                )
                .disabled(history.is_none())
                .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                    let entity = entity.clone();
                    menu = menu.item(PopupMenuItem::new("Export Markdown").on_click(
                        move |_, _, cx| {
                            let _ = entity.update(cx, |app, cx| {
                                app.export_previous_version(run, cx);
                            });
                        },
                    ));
                    menu
                })
                .into_any_element()
            });
    }
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(rgba(0x000000a0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            col()
                .id("previous-versions")
                .w(px(760.))
                .max_h(relative(0.96))
                .p_6()
                .pb_5()
                .gap_3()
                .map(|d| crate::appearance::floating_surface(d, cx))
                .child(
                    row()
                        .justify_between()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Version history · {title}")),
                        )
                        .child(
                            quiet_button("previous-version-close", "Close")
                                .track_focus(&app.dialog_first_focus)
                                .on_click(cx.listener(|app, _, w, cx| {
                                    app.close_previous_versions(w, cx)
                                })),
                        ),
                )
                .child(muted(
                    if app.store.task_is_archived(task) {
                        "Archived history · retained reports are read-only. Restore the goal to resume work; no approval or run action is offered here."
                    } else {
                        "Retained reports remain readable. Execution needs the current plan to be approved; viewing does not resume work."
                    },
                    cx,
                ))
                .child(list.map(observed))
                .child(
                    col()
                        .id("previous-version-body-scroll")
                        .w_full()
                        .flex_shrink_0()
                        .p_3()
                        .gap_2()
                        .rounded(px(crate::appearance::GROUP_RADIUS))
                        .bg(rgb(colors.canvas))
                        .child(body.map(observed))
                        .map(observed),
                )
                .child(actions)
                .map(observed),
        )
        .track_focus(&app.modal_focus)
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, w, cx| {
            if event.keystroke.key == "escape" {
                app.close_previous_versions(w, cx);
                cx.stop_propagation();
            }
        }))
        .focus_trap("previous-versions-focus", &app.modal_focus)
        .into_any_element()
}
