use crate::app::Scope;
use crate::appearance::palette;
pub(crate) use crate::controls::*;
use crate::domain::{Priority, Task};
use crate::{app::*, domain::*};
use gpui_kit::base::{FocusTrapElement, h_resizable, resizable_panel};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::ButtonVariants,
    checkbox::Checkbox,
    input::{IndentInline, Input, OutdentInline, Textarea},
};
use gpui_kit::prelude::FluentBuilder;
#[cfg(any(test, feature = "qa"))]
use gpui_kit::test::TestSupportExt;
use gpui_kit::*;

pub(crate) fn observed<E: Element<PrepaintState = Option<Hitbox>> + InteractiveElement>(
    element: E,
) -> AnyElement {
    #[cfg(any(test, feature = "qa"))]
    {
        element.test_support().into_any_element()
    }
    #[cfg(not(any(test, feature = "qa")))]
    {
        element.into_any_element()
    }
}
use crate::navigation::CrumbTarget;

enum CollectionRow {
    Project(u64, String, usize),
    Task(Box<Task>),
}

pub(crate) fn row() -> Div {
    div().flex().items_center().gap_2()
}
pub(crate) fn col() -> Div {
    div().flex().flex_col()
}
pub(crate) fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    let colors = palette(cx);
    div().text_color(rgb(colors.muted)).child(text.into())
}
pub(crate) fn label(text: impl Into<SharedString>, cx: &App) -> Div {
    muted(text, cx).text_xs()
}
pub(crate) fn chip(text: impl Into<SharedString>, color: u32, cx: &App) -> Div {
    let colors = palette(cx);
    div()
        .px_2()
        .py_0p5()
        .rounded(px(4.))
        .bg(rgb(colors.panel))
        .text_xs()
        .text_color(rgb(color))
        .child(text.into())
}
pub(crate) fn stage_color(stage: Stage, cx: &App) -> u32 {
    let colors = palette(cx);
    match stage {
        Stage::Backlog => colors.muted,
        Stage::Planned => colors.secondary,
        Stage::InProgress => colors.warning,
        Stage::Review => colors.review,
        Stage::Done => colors.success,
        Stage::Cancelled => colors.cancelled,
    }
}
fn task_state_icon(store: &Store, task: &Task, cx: &App) -> Div {
    let (symbol, color) = if task
        .research
        .as_ref()
        .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal)
    {
        let state = store.step_state(task.id);
        (state.symbol(), state.color(cx))
    } else {
        (task.stage.symbol(), stage_color(task.stage, cx))
    };
    div()
        .w_4()
        .flex_shrink_0()
        .text_color(rgb(color))
        .child(symbol)
}
// All hierarchy levels use the same semantic card surfaces, including hover states.
pub(crate) fn board_card_colors(stage: Stage, cx: &App) -> (u32, u32, u32) {
    match (gpui_kit::component::Theme::global(cx).is_dark(), stage) {
        (false, Stage::Backlog) => (0xe8ebf0, 0xdde2e9, 0xc6cdd8),
        (false, Stage::Planned) => (0xdeebfa, 0xd0e2f7, 0xb1cce9),
        (false, Stage::InProgress) => (0xffedc9, 0xffe4ae, 0xe4c47e),
        (false, Stage::Review) => (0xeee2fa, 0xe4d4f5, 0xcfb7e8),
        (false, Stage::Done) => (0xdcefe5, 0xcce7d9, 0xa8d2be),
        (false, Stage::Cancelled) => (0xf2e0e2, 0xead2d6, 0xdab6bc),
        (true, Stage::Backlog) => (0x282e38, 0x313a48, 0x434d5e),
        (true, Stage::Planned) => (0x20354e, 0x294361, 0x3b6086),
        (true, Stage::InProgress) => (0x49391e, 0x594625, 0x7d6132),
        (true, Stage::Review) => (0x3a2b4d, 0x49365f, 0x674b82),
        (true, Stage::Done) => (0x203e33, 0x294e40, 0x3e705c),
        (true, Stage::Cancelled) => (0x442c33, 0x543740, 0x754c57),
    }
}
fn stage_text(stage: Stage) -> String {
    format!("{}  {}", stage.symbol(), stage.label())
}
// Tree rows own hover feedback so the title and disclosure read as one item.
pub(crate) fn sidebar_variant(active: bool, cx: &App) -> gpui_kit::component::button::ButtonCustomVariant {
    let colors = palette(cx);
    gpui_kit::component::button::ButtonCustomVariant::new(cx)
        .foreground(
            rgb(if active {
                colors.sidebar_selected
            } else {
                colors.text
            })
            .into(),
        )
        .color(transparent_black())
        .hover(transparent_black())
        .active(transparent_black())
}

fn section(title: &str, cx: &App) -> Div {
    label(title.to_string(), cx).pt_3().pb_1().px_3()
}

impl TaskApp {
    pub(crate) fn layout_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        self.layout_switch_for(false, cx)
    }
    pub(crate) fn layout_switch_for(&self, steps: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let layout = if steps {
            self.steps_layout
        } else {
            self.layout
        };
        row()
            .flex_shrink_0()
            .gap_0()
            .p(px(2.))
            .rounded(px(7.))
            .bg(rgb(colors.side))
            .child(
                quiet_button(
                    if steps {
                        "steps-list-view"
                    } else {
                        "list-view"
                    },
                    "List",
                )
                .icon(assets::IconName::Menu)
                .selected(layout == Layout::List)
                .when(layout == Layout::List, |b| b.bg(rgb(colors.panel)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if steps {
                        this.steps_layout = Layout::List;
                    } else {
                        this.layout = Layout::List;
                    }
                    cx.notify();
                })),
            )
            .child(
                quiet_button(
                    if steps {
                        "steps-board-view"
                    } else {
                        "board-view"
                    },
                    "Board",
                )
                .icon(assets::IconName::LayoutDashboard)
                .selected(layout == Layout::Board)
                .when(layout == Layout::Board, |b| b.bg(rgb(colors.panel)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if steps {
                        this.steps_layout = Layout::Board;
                    } else {
                        this.layout = Layout::Board;
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn task_location_label(&self, task: &Task) -> String {
        let id = self.store.identifier(task);
        match task.research.as_ref().map(|r| r.kind) {
            Some(crate::research::ResearchKind::Goal) => format!("Research goal · {id}"),
            Some(crate::research::ResearchKind::Collect) => format!("Evidence · {id}"),
            Some(crate::research::ResearchKind::Analyze) => format!("Analysis · {id}"),
            Some(crate::research::ResearchKind::Advise) => format!("Final report · {id}"),
            None => id,
        }
    }

    fn save_feedback_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        if let Some(feedback) = &self.save_feedback {
            let (message, failed) = match feedback {
                Ok(message) => (message, false),
                Err(message) => (message, true),
            };
            return row()
                .id("save-feedback")
                .mx_4()
                .my_2()
                .flex_shrink_0()
                .p_4()
                .gap_3()
                .items_start()
                .rounded_lg()
                .bg(rgb(colors.panel))
                .border_1()
                .border_color(rgb(if failed {
                    colors.danger
                } else {
                    colors.success
                }))
                .child(
                    div()
                        .text_lg()
                        .text_color(rgb(if failed {
                            colors.danger
                        } else {
                            colors.success
                        }))
                        .child(if failed { "!" } else { "✓" }),
                )
                .child(
                    col()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(if failed {
                            "Not saved"
                        } else {
                            "Saved"
                        }))
                        .child(div().text_sm().child(message.clone())),
                )
                .child(
                    icon_button("dismiss-save-feedback", "×", "Dismiss save feedback").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.save_feedback = None;
                            cx.notify();
                        }),
                    ),
                )
                .map(observed)
                .into_any_element();
        }
        div().into_any_element()
    }
    pub fn set_appearance(
        &mut self,
        mode: crate::appearance::Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.store.appearance = mode;
        crate::appearance::apply(mode, Some(window), cx);
        self.persist_if_changed(true);
        cx.notify();
    }
    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let shell = col()
            .w_full()
            .flex_shrink_0()
            .h_full()
            .bg(rgb(colors.side))
            .border_r_1()
            .border_color(rgb(colors.line))
            .p_3();
        let mut side = col()
            .id("sidebar-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                row()
                    .min_h(px(60.))
                    .px_2()
                    .gap_3()
                    .child(
                        div()
                            .size_8()
                            .flex_shrink_0()
                            .rounded_md()
                            .bg(rgb(colors.accent_surface))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path("brand/planning.svg")
                                    .size(px(26.))
                                    .text_color(rgb(colors.accent)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(1.3))
                            .child("What the hell are you planning?"),
                    ),
            )
            .child(row().px_2().pb_3().child(label("Personal workspace", cx)));
        side = side.child(section("Workspace", cx));
        let count = self.store.tasks.iter()
            .filter(|t| !self.store.task_is_archived(t.id) && t.parent_id.is_none())
            .count();
        side = side.child(self.nav("all", "All", Scope::All, count, cx));
        let cycles: std::collections::BTreeSet<_> = self
            .store
            .tasks
            .iter()
            .filter(|t| !self.store.task_is_archived(t.id))
            .filter_map(|t| t.cycle.as_deref())
            .collect();
        if !cycles.is_empty() {
            side = side.child(section("Cycles", cx));
        }
        for (i, name) in cycles.into_iter().enumerate() {
            let scope = Scope::Cycle(name.to_string());
            let count = self
                .store
                .tasks
                .iter()
                .filter(|t| !self.store.task_is_archived(t.id) && t.cycle.as_deref() == Some(name))
                .count();
            side = side.child(self.nav(format!("cycle-{i}"), name, scope, count, cx));
        }
        side = side.child(
            row()
                .justify_between()
                .child(section("Projects", cx))
                .child(
                    tool_icon("new-project", assets::IconName::Plus, "Create project")
                        .tooltip("Create project")
                        .on_click(cx.listener(|this, _, window, cx| {
                            let input = cx.new(|cx| {
                                gpui_kit::component::input::InputState::new(window, cx)
                                    .placeholder("Project name")
                            });
                            input.update(cx, |s, cx| s.focus(window, cx));
                            this.new_project = Some(input);
                            this.error.clear();
                            cx.notify();
                        })),
                ),
        );
        for project in &self.store.projects {
            let count = self
                .store
                .tasks
                .iter()
                .filter(|t| {
                    !self.store.task_is_archived(t.id)
                        && t.parent_id.is_none()
                        && t.project_id == project.id
                        && !matches!(t.stage, Stage::Done | Stage::Cancelled)
                })
                .count();
            let pid = project.id;
            let expanded = self.expanded_sidebar_projects.contains(&pid);
            let active = self.sidebar_scope() == Scope::Project(pid);
            side = side.child(
                row()
                    .gap_0()
                    .rounded_md()
                    .hover(|d| d.bg(rgb(colors.hover)))
                    .child(
                        quiet_button(format!("project-{pid}"), "")
                            .custom(sidebar_variant(active, cx))
                            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                            .flex_1()
                            .min_w_0()
                            .h_8()
                            .px_3()
                            .tooltip(project.name.clone())
                            .accessibility_label(project.name.clone())
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(
                                        gpui_kit::component::Icon::new(assets::IconName::Folder)
                                            .size(px(20.))
                                            .flex_shrink_0(),
                                    )
                                    .child(single_line_text(
                                        format!("sidebar-project-title-{pid}"),
                                        project.name.clone(),
                                    ))
                                    .child(label(
                                        if count > 0 {
                                            count.to_string()
                                        } else {
                                            String::new()
                                        },
                                        cx,
                                    )),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.navigate(Scope::Project(pid), w, cx)
                            })),
                    )
                    .child(
                        tool_icon(
                            format!("expand-project-{pid}"),
                            if expanded {
                                assets::IconName::ChevronDown
                            } else {
                                assets::IconName::ChevronRight
                            },
                            if expanded {
                                "Collapse project tasks"
                            } else {
                                "Expand project tasks"
                            },
                        )
                        .custom(sidebar_variant(active, cx))
                        .h_8()
                        .flex_shrink_0()
                        .on_click(cx.listener(move |a, _, _, cx| {
                            if a.expanded_sidebar_projects.contains(&pid) {
                                a.expanded_sidebar_projects.retain(|id| *id != pid);
                            } else {
                                a.expanded_sidebar_projects.push(pid);
                            }
                            cx.notify();
                        })),
                    ),
            );
            if expanded {
                let project_tasks: Vec<_> = self
                    .store
                    .tasks
                    .iter()
                    .filter(|t| t.project_id == pid && !self.store.task_is_archived(t.id))
                    .collect();
                let mut list = col()
                    .gap_0()
                    .ml_5()
                    .pl_2()
                    .border_l_1()
                    .border_color(rgb(colors.line));
                let mut groups: Vec<_> = self
                    .store
                    .topics
                    .iter()
                    .filter(|t| t.project_id == pid && !t.archived)
                    .map(|t| Some(t.id))
                    .collect();
                groups.push(None);
                for topic_id in groups {
                    let tasks: Vec<_> = project_tasks
                        .iter()
                        .copied()
                        .filter(|t| self.store.task_topic(t.id).map(|topic| topic.id) == topic_id)
                        .collect();
                    if let Some(topic) =
                        topic_id.and_then(|id| self.store.topics.iter().find(|t| t.id == id))
                    {
                        list = list.child(self.topic_sidebar_row(topic, cx));
                        if self.collapsed_sidebar_topics.contains(&topic.id) {
                            continue;
                        }
                    }
                    if tasks.is_empty()
                        && topic_id.is_none()
                        && self.store.topics.iter().all(|t| t.project_id != pid)
                    {
                        list = list.child(div().px_2().py_2().child(label("No tasks yet", cx)));
                    }
                    // Preserve the parent tree for research steps and manually added subtasks.
                    let mut pending: Vec<_> = tasks
                        .iter()
                        .rev()
                        .filter(|t| {
                            t.parent_id
                                .is_none_or(|id| !tasks.iter().any(|p| p.id == id))
                        })
                        .map(|t| (*t, 0usize))
                        .collect();
                    let mut ordered = Vec::new();
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some((task, depth)) = pending.pop() {
                        if !visited.insert(task.id) {
                            continue;
                        }
                        ordered.push((task, depth));
                        if !self.collapsed_sidebar_goals.contains(&task.id) {
                            pending.extend(
                                tasks
                                    .iter()
                                    .rev()
                                    .filter(|t| t.parent_id == Some(task.id))
                                    .map(|t| (*t, depth + 1)),
                            );
                        }
                    }
                    for (task, depth) in ordered {
                        let nested = depth > 0;
                        let tid = task.id;
                        let has_steps = tasks.iter().any(|t| t.parent_id == Some(tid));
                        let hint = format!(
                            "{} · {}\n{}",
                            self.store.identifier(task),
                            task.stage.label(),
                            task.title
                        );
                        let hint = hint
                            .chars()
                            .collect::<Vec<_>>()
                            .chunks(32)
                            .map(|chunk| chunk.iter().collect::<String>())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let active = self.selected.or(self.task_result_context) == Some(tid);
                        let task_button = quiet_button(format!("sidebar-task-{tid}"), "")
                            .custom(sidebar_variant(active, cx))
                            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
                            .flex_1()
                            .min_w_0()
                            .h_7()
                            .px_2()
                            .tooltip(hint)
                            .accessibility_label(format!("{} · {}", task.title, task.stage.label()))
                            .child(
                                row()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(task_state_icon(&self.store, task, cx))
                                    .child(single_line_text(
                                        format!("sidebar-task-title-{tid}"),
                                        format!(
                                            "{} · {}",
                                            if nested {
                                                self.store
                                                    .identifier(task)
                                                    .rsplit('-')
                                                    .next()
                                                    .unwrap_or("")
                                                    .to_string()
                                            } else {
                                                self.store.identifier(task)
                                            },
                                            task.title
                                        ),
                                    )),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                a.navigate(Scope::Project(pid), w, cx);
                                a.open_task(tid, w, cx);
                            }));
                        list = list.child(
                            row()
                                .gap_0()
                                .min_w_0()
                                .ml(px((depth as f32
                                    + if topic_id.is_some() { 1. } else { 0. })
                                    * 12.))
                                .when(nested, |d| {
                                    d.pl_1().border_l_1().border_color(rgb(colors.line))
                                })
                                .when(!nested, |d| d.rounded_md())
                                .hover(|d| d.bg(rgb(colors.hover)))
                                .child(task_button)
                                .when(has_steps, |d| {
                                    d.child(
                                        tool_icon(
                                            format!("sidebar-goal-toggle-{tid}"),
                                            if self.collapsed_sidebar_goals.contains(&tid) {
                                                assets::IconName::ChevronRight
                                            } else {
                                                assets::IconName::ChevronDown
                                            },
                                            "Expand or collapse research steps",
                                        )
                                        .custom(sidebar_variant(active, cx))
                                        .h_7()
                                        .flex_shrink_0()
                                        .on_click(
                                            cx.listener(move |a, _, _, cx| {
                                                if a.collapsed_sidebar_goals.contains(&tid) {
                                                    a.collapsed_sidebar_goals
                                                        .retain(|id| *id != tid);
                                                } else {
                                                    a.collapsed_sidebar_goals.push(tid);
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                }),
                        );
                    }
                }
                side = side.child(list);
            }
        }
        side = side
            .child(section("Research", cx))
            .child(self.nav(
                "recovery",
                "Paused projects",
                Scope::Recovery,
                self.store.paused_project_goals().len(),
                cx,
            ))
            .child(self.nav(
                "runs",
                "Progress & results",
                Scope::Runs,
                self.store.runs.iter().filter(|r| !self.store.run_is_archived(r)).count(),
                cx,
            ))
            .child(div().flex_1())
            .child(self.nav(
                "archive",
                "Archive",
                Scope::Archive,
                self.store.tasks.iter().filter(|t| self.store.task_is_archived(t.id)).count(),
                cx,
            ));
        let appearance = self.store.appearance;
        shell
            .child(side.map(observed))
            .child(
                row()
                    .flex_shrink_0()
                    .gap_0()
                    .mt_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .child(
                        quiet_button("model-settings", "")
                            .accessibility_label("Settings")
                            .flex_1()
                            .min_w_0()
                            .h_8()
                            .px_3()
                            .custom(sidebar_variant(self.sidebar_scope() == Scope::Settings, cx).hover(rgb(colors.hover).into()))
                            .when(self.sidebar_scope() == Scope::Settings, |b| b.font_weight(FontWeight::SEMIBOLD))
                            .child(row().w_full().min_w_0().gap_2()
                                .child(gpui_kit::component::Icon::new(assets::IconName::Settings)
                                    .size(px(20.)).flex_shrink_0())
                                .child(div().text_sm().child("Settings")))
                            .on_click(
                                cx.listener(|this, _, w, cx| this.navigate(Scope::Settings, w, cx)),
                            ),
                    )
                    .child(
                        tool_icon(
                            "toggle-appearance",
                            match appearance {
                                crate::appearance::Appearance::Light => assets::IconName::Sun,
                                crate::appearance::Appearance::Dark => assets::IconName::Moon,
                                crate::appearance::Appearance::System => assets::IconName::Monitor,
                            },
                            match appearance {
                                crate::appearance::Appearance::System => {
                                    "Following system · click for Light"
                                }
                                crate::appearance::Appearance::Light => "Light · click for Dark",
                                crate::appearance::Appearance::Dark => {
                                    "Dark · click to follow system"
                                }
                            },
                        )
                        .h_8()
                        .on_click(cx.listener(|a, _, w, cx| {
                            a.set_appearance(a.store.appearance.next(), w, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
    fn nav(
        &self,
        id: impl Into<ElementId>,
        title: &str,
        scope: Scope,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = palette(cx);
        let active = self.sidebar_scope() == scope;
        let icon = match &scope {
            Scope::All => assets::IconName::Layers,
            Scope::Project(_) | Scope::Topic(_) => assets::IconName::Folder,
            Scope::Cycle(_) => assets::IconName::RefreshCw,
            Scope::Archive => assets::IconName::Archive,
            Scope::Workflow => assets::IconName::Workflow,
            Scope::Runs => assets::IconName::ChartNoAxesCombined,
            Scope::Recovery => assets::IconName::Pause,
            Scope::Research => assets::IconName::Search,
            Scope::Settings => assets::IconName::Settings,
        };
        button(id, "")
            .ghost()
            .accessibility_label(title.to_string())
            .tooltip(title.to_string())
            .w_full()
            .h_8()
            .px_3()
            .custom(sidebar_variant(active, cx).hover(rgb(colors.hover).into()))
            .when(active, |b| b.font_weight(FontWeight::SEMIBOLD))
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .gap_2()
                    .child(
                        gpui_kit::component::Icon::new(icon)
                            .size(px(20.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_sm()
                            .child(title.to_string()),
                    )
                    .child(
                        row()
                            .flex_shrink_0()
                            .gap_2()
                            .child(label(
                                if count > 0 {
                                    count.to_string()
                                } else {
                                    String::new()
                                },
                                cx,
                            ))
                            .child(
                                gpui_kit::component::Icon::new(assets::IconName::ChevronRight)
                                    .size(px(14.))
                                    .text_color(rgb(colors.muted)),
                            ),
                    ),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.navigate(scope.clone(), window, cx)),
            )
            .into_any_element()
    }
    pub(crate) fn navigation_path(&self) -> (Vec<(String, String, CrumbTarget)>, String) {
        let mut links = vec![];
        let result = self.selected.is_none() && self.scope == Scope::Runs;
        let run = result
            .then(|| self.displayed_run_id())
            .flatten()
            .and_then(|id| self.store.runs.iter().find(|r| r.id == id));
        let task = self
            .selected
            .or(self.task_result_context)
            .or_else(|| run.map(|r| r.task_id))
            .and_then(|id| self.store.task(id));
        let mut current = self.title();
        if self.scope != Scope::All || task.is_some() {
            links.push((
                "location-all".into(),
                "All".into(),
                CrumbTarget::View(Scope::All),
            ));
        }
        if let Some(task) = task {
            if let Some(project) = self.store.projects.iter().find(|p| p.id == task.project_id) {
                links.push((
                    "location-project".into(),
                    project.name.clone(),
                    CrumbTarget::View(Scope::Project(project.id)),
                ));
            }
            if let Some(topic) = self.store.task_topic(task.id) {
                links.push((
                    "location-topic".into(),
                    topic.display_name(),
                    CrumbTarget::View(Scope::Topic(topic.id)),
                ));
            }
            let mut ancestors = vec![];
            let mut seen = vec![task.id];
            let mut parent = task.parent_id;
            while let Some(t) = parent.and_then(|id| self.store.task(id)) {
                if seen.contains(&t.id) {
                    break;
                }
                seen.push(t.id);
                ancestors.push(t);
                parent = t.parent_id;
            }
            for t in ancestors.iter().rev() {
                let key = if Some(t.id) == task.parent_id {
                    "location-parent".into()
                } else {
                    format!("location-ancestor-{}", t.id)
                };
                links.push((key, self.task_location_label(t), CrumbTarget::Task(t.id)));
            }
            current = self.task_location_label(task);
            if result {
                links.push(("location-task".into(), current, CrumbTarget::Task(task.id)));
                current = if run
                    .is_some_and(|r| r.research_kind == Some(crate::research::ResearchKind::Goal))
                {
                    "Planning result"
                } else {
                    "Result"
                }
                .into();
            }
        } else if let Some(project) = run
            .and_then(|r| r.project_plan)
            .and_then(|id| self.store.projects.iter().find(|p| p.id == id))
        {
            links.push((
                "location-project".into(),
                project.name.clone(),
                CrumbTarget::View(Scope::Project(project.id)),
            ));
            current = "Project planning".into();
        } else if let Scope::Topic(id) = self.scope {
            if let Some(project) = self
                .store
                .topics
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| self.store.projects.iter().find(|p| p.id == t.project_id))
            {
                links.push((
                    "location-project".into(),
                    project.name.clone(),
                    CrumbTarget::View(Scope::Project(project.id)),
                ));
            }
        } else if self.scope == Scope::Workflow
            || (self.scope == Scope::Research && self.show_research_settings)
        {
            links.push((
                "location-settings".into(),
                "Settings".into(),
                CrumbTarget::View(Scope::Settings),
            ));
            if self.scope == Scope::Research {
                current = "Research models".into();
            }
        }
        (links, current)
    }

    fn navigation_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let (links, current) = self.navigation_path();
        let destination = links.last().map(|(_, title, _)| title.clone());
        let mut trail = row()
            .id("location-trail")
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap_1();
        for (id, title, target) in links {
            trail = trail
                .child(
                    quiet_button(id, title.clone())
                        .h_7()
                        .px_1()
                        .min_w_0()
                        .max_w(px(150.))
                        .text_color(rgb(colors.secondary))
                        .tooltip(format!("Open {title}"))
                        .on_click(cx.listener(move |a, _, w, cx| match &target {
                            CrumbTarget::View(scope) => a.navigate(scope.clone(), w, cx),
                            CrumbTarget::Task(id) => a.open_task(*id, w, cx),
                        })),
                )
                .child(label("/", cx));
        }
        trail = trail.child(
            single_line_text("location-current", current)
                .min_w(px(64.))
                .font_weight(FontWeight::MEDIUM)
                .map(observed),
        );
        let mut header = row()
            .id("navigation-header")
            .h(px(44.))
            .flex_shrink_0()
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(rgb(colors.line))
            .when(destination.is_some(), |d| {
                d.child(
                    quiet_button("navigation-back", "Back")
                        .icon(assets::IconName::ArrowLeft)
                        .h_7()
                        .px_1()
                        .flex_shrink_0()
                        .disabled(destination.is_none())
                        .tooltip(
                            destination
                                .map(|name| format!("Back to {name} · ⌘["))
                                .unwrap_or_else(|| "No previous page".into()),
                        )
                        .on_click(cx.listener(|a, _, w, cx| a.go_back(w, cx))),
                )
            })
            .child(trail.map(observed));
        let options: Vec<_> = self
            .tabs
            .iter()
            .rev()
            .filter_map(|id| {
                self.store
                    .task(*id)
                    .filter(|t| !self.store.task_is_archived(t.id))
                    .map(|t| (*id, format!("{} · {}", self.store.identifier(t), t.title)))
            })
            .collect();
        if !options.is_empty() {
            let entity = cx.entity().downgrade();
            let selected = self.selected;
            header = header.child(
                quiet_button("open-tasks-menu", format!("Open tasks · {}", options.len()))
                    .dropdown_caret(true)
                    .flex_shrink_0()
                    .text_color(rgb(colors.muted))
                    .tooltip("Open a task; Back returns to the page you are viewing now")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                        for (id, title) in &options {
                            let entity = entity.clone();
                            let id = *id;
                            menu = menu.item(
                                task_menu_item(title.clone())
                                    .checked(selected == Some(id))
                                    .on_click(move |_, w, cx| {
                                        let _ = entity.update(cx, |a, cx| a.open_task(id, w, cx));
                                    }),
                            );
                        }
                        if selected.is_some() {
                            let entity = entity.clone();
                            menu = menu.separator().item(
                                PopupMenuItem::new("Close current task").on_click(
                                    move |_, w, cx| {
                                        let _ =
                                            entity.update(cx, |a, cx| a.close_current_task(w, cx));
                                    },
                                ),
                            );
                        }
                        menu
                    }),
            );
        }
        header.into_any_element()
    }
    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let project = if let Scope::Project(id) = self.scope {
            Some(id)
        } else {
            None
        };
        let count = self.visible_tasks().len();
        row()
            .h(px(60.))
            .px_6()
            .gap_4()
            .flex_shrink_0()
            .child(
                col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        row().min_w_0().gap_2().child(
                            single_line_text("collection-title", self.title())
                                .h_6()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .map(observed),
                        ),
                    )
                    .child(label(
                        if let Some(id) = project {
                            let pending = self
                                .store
                                .tasks
                                .iter()
                                .filter(|t| {
                                    t.project_id == id
                                        && t.parent_id.is_none()
                                        && !t.archived
                                        && self.store.research_pending_approval(t.id)
                                })
                                .count();
                            if pending > 0 {
                                format!("{count} goals · {pending} awaiting approval")
                            } else {
                                format!("{count} goals")
                            }
                        } else {
                            format!(
                                "{count} goals · {}",
                                if self.layout == Layout::List {
                                    "grouped by project"
                                } else {
                                    "grouped by topic"
                                }
                            )
                        },
                        cx,
                    )),
            )
            .when_some(project, |d, id| {
                d.child(self.project_toolbar_actions(id, cx))
            })
            .when(project.is_none(), |d| {
                d.child(
                    button("new-task", "New research")
                        .icon(assets::IconName::Plus)
                        .when(project.is_some(), |b| b.ghost())
                        .when(project.is_none(), |b| b.primary())
                        .on_click(cx.listener(|this, _, w, cx| this.open_research_intake(w, cx))),
                )
            })
            .into_any_element()
    }
    fn all_status_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.all_status;
        let entity = cx.entity().downgrade();
        quiet_button("all-status-filter", selected.label())
            .dropdown_caret(true)
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette(cx).accent))
            .bg(rgb(palette(cx).accent_surface))
            .tooltip("Filter tasks by status")
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for filter in [AllStatus::All, AllStatus::Open]
                    .into_iter()
                    .chain(Stage::ALL.map(AllStatus::Stage))
                {
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(filter.label())
                            .checked(filter == selected)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    this.all_status = filter;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
    fn collection(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let tasks = self.visible_tasks();
        let mut main = col().size_full().child(self.toolbar(cx)).child(
            row()
                .px_6()
                .pb_4()
                .gap_3()
                .when(self.scope == Scope::All, |d| {
                    d.child(self.all_status_control(cx))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.search).id("task-search")),
                )
                .child(self.layout_switch(cx))
                .child(
                    tool_icon("display", assets::IconName::Settings2, "Display options")
                        .selected(self.show_display)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_display = !this.show_display;
                            cx.notify();
                        })),
                ),
        );
        if self.scope == Scope::Archive {
            main = main.child(self.topic_directory(None, cx));
        }
        if let Scope::Project(id) = self.scope {
            main = main.child(self.topic_directory(Some(id), cx));
            let pending = self
                .store
                .research_requests
                .iter()
                .filter(|r| r.project_id == id && r.goal_ids.is_empty() && !self.store.request_is_archived(r))
                .count();
            if pending > 0 {
                main = main.child(
                    row()
                        .px_6()
                        .pb_2()
                        .child(label(
                            format!("{pending} saved requests awaiting a plan"),
                            cx,
                        ))
                        .child(button("saved-requests", "Open requests").on_click(
                            cx.listener(|a, _, w, cx| a.navigate(Scope::Research, w, cx)),
                        )),
                );
            }
            main = main.child(self.todo_batch_bar(Some(id), cx));
        }
        if self.query.ends_with("status:") || self.query.ends_with("stage:") {
            main = main.child(
                row()
                    .px_6()
                    .pb_3()
                    .children(Stage::ALL.into_iter().map(|stage| {
                        button(format!("suggest-{}", stage.key()), stage.label()).on_click(
                            cx.listener(move |this, _, w, cx| {
                                let q = format!("{}{}", this.query, stage.key());
                                this.search.update(cx, |s, cx| s.set_value(q, w, cx));
                            }),
                        )
                    })),
            );
        }
        if self.show_display {
            main = main.child(
                row()
                    .px_6()
                    .pb_4()
                    .gap_5()
                    .when(self.scope != Scope::All, |d| {
                        d.child(
                            Checkbox::new("show-done")
                                .label("Show completed")
                                .checked(self.show_done)
                                .on_change(cx.listener(|this, v, _, cx| {
                                    this.show_done = *v;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Checkbox::new("sort-priority")
                            .label("Priority first")
                            .checked(self.sort_priority)
                            .on_change(cx.listener(|this, v, _, cx| {
                                this.sort_priority = *v;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("compact")
                            .label("Compact rows")
                            .checked(self.compact)
                            .on_change(cx.listener(|this, v, _, cx| {
                                this.compact = *v;
                                cx.notify();
                            })),
                    ),
            );
        }
        if tasks.is_empty() {
            return main
                .child(
                    col()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(div().text_xl().child("No tasks here"))
                        .child(muted("Try another filter, or create your first task.", cx))
                        .child(
                            button("clear-filters", "Reset filters").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.search.update(cx, |s, cx| s.set_value("", w, cx));
                                    this.show_done = true;
                                    if this.scope == Scope::All {
                                        this.all_status = AllStatus::All;
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .into_any_element();
        }
        if self.layout == Layout::Board {
            return main.child(self.board(&tasks, cx)).into_any_element();
        }
        let ordered = self.ordered_visible_tasks();
        let row_height = if self.compact { 32. } else { 38. };
        let mut rows = Vec::new();
        let grouped = !matches!(self.scope, Scope::Project(_));
        let mut last_project = None;
        for task in ordered {
            if grouped && last_project != Some(task.project_id) {
                let name = self
                    .store
                    .projects
                    .iter()
                    .find(|p| p.id == task.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Unassigned project".into());
                let count = tasks
                    .iter()
                    .filter(|t| t.project_id == task.project_id)
                    .count();
                rows.push(CollectionRow::Project(task.project_id, name, count));
                last_project = Some(task.project_id);
            }
            rows.push(CollectionRow::Task(Box::new(task)));
        }
        main.child(
            row()
                .px_6()
                .pb_2()
                .gap_3()
                .text_xs()
                .text_color(rgb(colors.muted))
                .child(div().w(px(156.)).flex_shrink_0().child("PRIORITY / ID"))
                .child(div().flex_1().min_w_0().child("TASK"))
                .child(div().w(px(100.)).flex_shrink_0().child("STAGE"))
                .child(div().w(px(84.)).flex_shrink_0().child("LABELS"))
                .child(div().w_16().flex_shrink_0().child("DUE"))
                .child(div().w(px(156.)).flex_shrink_0().child("CREATED")),
        )
        .child(
            uniform_list(
                "task-list",
                rows.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| match &rows[i] {
                            CollectionRow::Task(task) => this.task_row(task, row_height, cx),
                            CollectionRow::Project(id, name, count) => {
                                let id = *id;
                                quiet_button(format!("collection-project-{id}"), "")
                                    .h(px(row_height))
                                    .w_full()
                                    .px_6()
                                    .rounded_none()
                                    .bg(rgb(palette(cx).side))
                                    .accessibility_label(format!("Open project {name}"))
                                    .child(
                                        row()
                                            .w_full()
                                            .min_w_0()
                                            .gap_2()
                                            .child(
                                                gpui_kit::component::Icon::new(
                                                    assets::IconName::Folder,
                                                )
                                                .size_4(),
                                            )
                                            .child(
                                                single_line_text(
                                                    format!("collection-project-title-{id}"),
                                                    name.clone(),
                                                )
                                                .font_weight(FontWeight::SEMIBOLD),
                                            )
                                            .child(label(format!("{count} goals"), cx)),
                                    )
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.open_collection_project(id, w, cx)
                                    }))
                                    .into_any_element()
                            }
                        })
                        .collect()
                }),
            )
            .h_full()
            .flex_1(),
        )
        .into_any_element()
    }
    pub fn ordered_visible_tasks(&self) -> Vec<Task> {
        let tasks = self.visible_tasks();
        let ids: Vec<_> = tasks.iter().map(|t| t.id).collect();
        let mut ordered = vec![];
        for t in tasks
            .iter()
            .filter(|t| t.parent_id.is_none() || !ids.contains(&t.parent_id.unwrap()))
        {
            ordered.push(t.clone());
            if !self.collapsed.contains(&t.id) {
                append_children(&tasks, t.id, &self.collapsed, &mut ordered);
            }
        }
        if !matches!(self.scope, Scope::Project(_)) {
            ordered.sort_by_key(|t| t.project_id);
        }
        ordered
    }
    fn task_row(&self, t: &Task, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let id = t.id;
        let child_count = self
            .store
            .tasks
            .iter()
            .filter(|c| c.parent_id == Some(id) && !c.archived)
            .count();
        let done = self
            .store
            .tasks
            .iter()
            .filter(|c| c.parent_id == Some(id) && !c.archived && c.stage == Stage::Done)
            .count();
        row()
            .id(format!("task-row-{id}"))
            .h(px(height))
            .px_6()
            .gap_3()
            .border_b_1()
            .border_color(rgb(colors.subtle_line))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(colors.hover)))
            .on_click(cx.listener(move |this, _, w, cx| this.open_task(id, w, cx)))
            .child(
                div().w(px(156.)).flex_shrink_0().child(
                    row()
                        .child(
                            div()
                                .w_5()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(rgb(if t.priority == Priority::Urgent {
                                    colors.danger
                                } else {
                                    colors.muted
                                }))
                                .child(priority_symbol(t.priority)),
                        )
                        .child(label(format!("{} ›", self.store.identifier(t)), cx)),
                ),
            )
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .when(t.parent_id.is_some(), |d| d.pl_5())
                    .child(
                        row()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(rgb(stage_color(t.stage, cx)))
                                    .child(t.stage.symbol()),
                            )
                            .child(
                                single_line_text(format!("task-title-{id}"), t.title.clone())
                                    .when(matches!(t.stage, Stage::Done | Stage::Cancelled), |d| {
                                        d.text_color(rgb(colors.muted))
                                    })
                                    .when(child_count > 0, |d| d.font_weight(FontWeight::MEDIUM)),
                            ),
                    )
                    .when(child_count > 0, |d| {
                        d.child(
                            quiet_button(format!("expand-{id}"), format!("{done}/{child_count}"))
                                .icon(if self.collapsed.contains(&id) {
                                    assets::IconName::ChevronRight
                                } else {
                                    assets::IconName::ChevronDown
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if this.collapsed.contains(&id) {
                                        this.collapsed.retain(|v| *v != id);
                                    } else {
                                        this.collapsed.push(id);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(100.))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(stage_color(t.stage, cx)))
                    .child(t.stage.label()),
            )
            .child(
                div().w(px(84.)).flex_shrink_0().overflow_hidden().child(
                    row().children(
                        t.labels
                            .iter()
                            .take(1)
                            .map(|l| label(l.clone(), cx).truncate()),
                    ),
                ),
            )
            .child(
                div()
                    .w_16()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(
                        if !t.due_date.is_empty()
                            && t.due_date.as_str()
                                < chrono::Local::now().date_naive().to_string().as_str()
                        {
                            colors.danger
                        } else {
                            colors.muted
                        },
                    ))
                    .child(if t.due_date.is_empty() {
                        "".into()
                    } else {
                        t.due_date[5..].to_string()
                    }),
            )
            .child(
                quiet_button(format!("task-created-{id}"), "")
                    .w(px(156.))
                    .flex_shrink_0()
                    .p_0()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .tooltip(if t.created_at.is_empty() {
                        "Creation time was not recorded for this older task.".to_string()
                    } else {
                        format!("Created: {}", t.created_at)
                    })
                    .child(div().w_full().whitespace_nowrap().text_xs().child(
                        if t.created_at.is_empty() {
                            "Not recorded".to_string()
                        } else {
                            t.created_at
                                .split_whitespace()
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                        },
                    )),
            )
            .map(observed)
            .map(|element| self.task_context(element, id, cx))
            .into_any_element()
    }
    pub(crate) fn board(&self, tasks: &[Task], cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        row()
            .id("board-scroll")
            .items_start()
            .gap_4()
            .px_6()
            .pb_5()
            .flex_1()
            .min_h_0()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .children(Stage::ALL.into_iter().map(|stage| {
                let cards: Vec<_> = tasks
                    .iter()
                    .filter(|t| t.parent_id.is_none() && t.stage == stage)
                    .cloned()
                    .collect();
                col()
                    .id(format!("column-{}", stage.key()))
                    .w(px(292.))
                    .flex_shrink_0()
                    .h_full()
                    .gap_3()
                    .rounded_md()
                    .drag_over::<DragTask>(move |d, _, _, _| d.bg(rgb(colors.hover)))
                    .on_drop(cx.listener(move |this, drag: &DragTask, _, cx| {
                        this.change_stage(drag.id, stage, cx)
                    }))
                    .child(
                        row()
                            .justify_between()
                            .h_9()
                            .child(
                                row()
                                    .child(
                                        div()
                                            .text_color(rgb(stage_color(stage, cx)))
                                            .child(stage_text(stage)),
                                    )
                                    .child(label(cards.len().to_string(), cx)),
                            )
                            .child(
                                tool_icon(
                                    format!("add-{}", stage.key()),
                                    assets::IconName::Plus,
                                    "New research starts in Backlog",
                                )
                                .on_click(cx.listener(
                                    move |this, _, w, cx| this.open_research_intake(w, cx),
                                )),
                            ),
                    )
                    .child(
                        col()
                            .id(format!("cards-{}", stage.key()))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .flex_1()
                            .min_h_0()
                            .gap_3()
                            .children(self.topic_grouped_cards(&cards, stage, cx))
                            .when(cards.is_empty(), |d| {
                                d.child(
                                    div()
                                        .h_24()
                                        .border_1()
                                        .border_color(rgb(colors.line))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(label("Drop a goal here", cx)),
                                )
                            }),
                    )
                    .map(observed)
            }))
            .into_any_element()
    }
    pub(crate) fn goal_card(&self, t: &Task, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        // Keep each status recognizable even when a card is hovered.
        let dark = gpui_kit::component::Theme::global(cx).is_dark();
        let (surface, hover, border) = board_card_colors(t.stage, cx);
        let mut metadata = t.labels.iter().take(2).cloned().collect::<Vec<_>>();
        if !t.bucket.is_empty() {
            metadata.push(t.bucket.clone());
        }
        if let Some(cycle) = &t.cycle {
            metadata.push(format!("⟳ {cycle}"));
        }
        let id = t.id;
        let drag = DragTask {
            id,
            title: t.title.clone(),
        };
        col()
            .id(format!("card-{id}"))
            .p_3()
            .gap_2()
            .rounded(px(7.))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(surface))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(hover)))
            .on_click(cx.listener(move |this, _, w, cx| this.open_task(id, w, cx)))
            .on_drag(drag, |value, _, _, cx| cx.new(|_| value.clone()))
            .child(
                row()
                    .items_start()
                    .gap_2()
                    .child(
                        row()
                            .flex_1()
                            .min_w_0()
                            .flex_wrap()
                            .gap_1()
                            .child(
                                div()
                                    .rounded(px(4.))
                                    .px_1()
                                    .py(px(2.))
                                    .bg(rgba(if dark { 0xffffff14 } else { 0xffffff99 }))
                                    .text_xs()
                                    .text_color(rgb(colors.text))
                                    .child(self.store.identifier(t)),
                            )
                            .when(
                                !matches!(self.scope, Scope::Project(_) | Scope::Topic(_)),
                                |d| {
                                    d.child(
                                        div()
                                            .min_w_0()
                                            .rounded(px(4.))
                                            .px_1()
                                            .py(px(2.))
                                            .bg(rgba(if dark { 0xffffff14 } else { 0xffffff99 }))
                                            .text_xs()
                                            .text_color(rgb(colors.text))
                                            .child(
                                                self.store
                                                    .projects
                                                    .iter()
                                                    .find(|p| p.id == t.project_id)
                                                    .map(|p| p.name.clone())
                                                    .unwrap_or_else(|| "Unassigned project".into()),
                                            ),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(colors.secondary))
                            .child(format!(
                                "{}  {}",
                                t.stage.symbol(),
                                priority_symbol(t.priority)
                            )),
                    ),
            )
            .child(div().line_height(relative(1.4)).child(t.title.clone()))
            .when_some(
                self.store
                    .research_requests
                    .iter()
                    .find(|r| r.goal_ids.contains(&id))
                    .and_then(|r| r.brief.as_ref()),
                |d, brief| {
                    let summary: String = brief
                        .deliverable
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(90)
                        .collect();
                    d.child(
                        label(
                            format!(
                                "Output · {summary}{}",
                                if brief.deliverable.chars().count() > 90 {
                                    "…"
                                } else {
                                    ""
                                }
                            ),
                            cx,
                        )
                        .line_clamp(3),
                    )
                },
            )
            .when(
                t.research
                    .as_ref()
                    .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal),
                |d| {
                    d.child(
                        quiet_button(format!("goal-progress-{id}"), "")
                            .h_auto()
                            .px_0()
                            .py_1()
                            .w_full()
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_left()
                                    .child(self.store.step_summary(id)),
                            )
                            .on_click(cx.listener(move |a, _, w, cx| {
                                cx.stop_propagation();
                                a.open_goal_steps(id, w, cx);
                            })),
                    )
                },
            )
            .when(!metadata.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .line_height(relative(1.4))
                        .text_color(rgb(colors.secondary))
                        .child(metadata.join(" · ")),
                )
            })
            .child(
                quiet_button(format!("card-created-{id}"), "")
                    .self_start()
                    .h_5()
                    .px_0()
                    .py_0()
                    .text_xs()
                    .text_color(rgb(colors.secondary))
                    .tooltip(if t.created_at.is_empty() {
                        "Creation time was not recorded for this older task.".to_string()
                    } else {
                        format!("Created: {}", t.created_at)
                    })
                    .child(div().text_xs().child(format!(
                        "Created · {}",
                        if t.created_at.is_empty() {
                            "Not recorded".to_string()
                        } else {
                            t.created_at
                                .split_whitespace()
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                        }
                    ))),
            )
            .when(self.store.research_pending_approval(id), |d| {
                d.child(label("Awaiting approval", cx)).child(
                    quiet_button(
                        format!("approve-topic-{id}"),
                        if t.parent_id.is_some() {
                            "Approve parent goal →"
                        } else {
                            "Approve research →"
                        },
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.approve_research_topic(id, cx);
                    })),
                )
            })
            .when(
                t.research.as_ref().is_some_and(|r| r.requires_approval)
                    && t.stage == Stage::Planned,
                |d| {
                    d.child(label(
                        if self.store.has_research_plan(id) {
                            "Approved · open to run research"
                        } else {
                            "Approved · open to generate research steps"
                        },
                        cx,
                    ))
                },
            )
            .map(observed)
            .map(|element| self.task_context(element, id, cx))
            .into_any_element()
    }
    fn task_context(&self, element: AnyElement, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let archived = self.store.task_is_archived(id);
        div()
            .id(format!("task-context-{id}"))
            .child(element)
            .context_menu(move |mut menu, _, _| {
                let target = entity.clone();
                menu = menu.item(PopupMenuItem::new("Open task").on_click(move |_, w, cx| {
                    let _ = target.update(cx, |this, cx| this.open_task(id, w, cx));
                }));
                if !archived {
                    let target = entity.clone();
                    menu = menu.item(PopupMenuItem::new("Edit task").on_click(move |_, w, cx| {
                        let _ = target.update(cx, |this, cx| {
                            this.begin_draft(Some(id), None, Stage::Backlog, w, cx)
                        });
                    }));
                    menu = menu.separator();
                    for stage in Stage::ALL {
                        let target = entity.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!("Move to {}", stage.label())).on_click(
                                move |_, _, cx| {
                                    let _ = target
                                        .update(cx, |this, cx| this.change_stage(id, stage, cx));
                                },
                            ),
                        );
                    }
                    let target = entity.clone();
                    menu = menu
                        .separator()
                        .item(
                            PopupMenuItem::new("Archive task").on_click(move |_, w, cx| {
                                let _ = target.update(cx, |this, cx| this.archive_task(id, w, cx));
                            }),
                        );
                }
                menu
            })
            .into_any_element()
    }
    fn detail(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let Some(t) = self.store.task(id) else {
            return col().child("Task not found").into_any_element();
        };
        let t = t.clone();
        let is_goal = t
            .research
            .as_ref()
            .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal);
        let result_target = if is_goal {
            self.store
                .research_children(id)
                .into_iter()
                .rev()
                .find(|child| self.store.research_result(*child).is_some())
        } else {
            self.store.research_result(id).map(|_| id)
        };
        let result_is_report = result_target
            .and_then(|target| self.store.task(target))
            .and_then(|task| task.research.as_ref())
            .is_some_and(|r| r.kind == crate::research::ResearchKind::Advise);
        let kind_label = t
            .research
            .as_ref()
            .map(|r| match r.kind {
                crate::research::ResearchKind::Goal => "Research goal",
                crate::research::ResearchKind::Collect => "Evidence",
                crate::research::ResearchKind::Analyze => "Analysis",
                crate::research::ResearchKind::Advise => "Final report",
            })
            .unwrap_or("Task");
        let visible = self.ordered_visible_tasks();
        let position = visible.iter().position(|t| t.id == id);
        let previous = position
            .and_then(|i| i.checked_sub(1))
            .map(|i| visible[i].id);
        let next = position.and_then(|i| visible.get(i + 1)).map(|t| t.id);
        let task_menu_entity = cx.entity().downgrade();
        let editing_disabled = self.store.task_is_archived(id) || self.task_running(id);
        let archived = t.archived;
        let mut head = row()
            .min_h(px(40.))
            .py_1()
            .px_6()
            .flex_wrap()
            .border_b_1()
            .border_color(rgb(colors.line))
            .justify_between()
            .child(
                row()
                    .gap_3()
                    .flex_wrap()
                    .when(is_goal, |d| d.child(self.goal_tabs(id, cx)))
                    .when(is_goal, |d| d.child(div().w(px(1.)).h_5().flex_shrink_0()
                        .bg(rgb(colors.border_strong))))
                    .child(div().id("task-kind-badge")
                        .flex_shrink_0().px_2().py_0p5().rounded(px(4.))
                        .bg(rgb(colors.tag_surface)).text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(colors.tag_text)).child(kind_label).map(observed)),
            );
        head = head.child(
            row()
                .flex_wrap()
                .when(is_goal, |d| d.child(self.goal_run_button(id, "goal-resume", cx)))
                .when_some(result_target, |d, target| {
                    d.child(
                        primary_button(
                            if is_goal {
                                "research-task-result"
                            } else {
                                "task-view-result"
                            },
                            if result_is_report {
                                "View final report"
                            } else {
                                "View latest result"
                            },
                        )
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.view_task_result(target, cx)),
                        ),
                    )
                })
                .child(
                    tool_icon(
                        "task-actions-menu",
                        assets::IconName::Ellipsis,
                        "More task actions",
                    )
                    .dropdown_menu_with_anchor(
                        Anchor::TopRight,
                        move |menu, _, _| {
                            let edit = task_menu_entity.clone();
                            let copy = task_menu_entity.clone();
                            let prev = task_menu_entity.clone();
                            let following = task_menu_entity.clone();
                            let archive = task_menu_entity.clone();
                            menu.item(
                                PopupMenuItem::new("Edit task")
                                    .disabled(editing_disabled)
                                    .on_click(move |_, w, cx| {
                                        let _ = edit.update(cx, |a, cx| {
                                            a.begin_draft(Some(id), None, Stage::Backlog, w, cx)
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Copy task link").on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                        "wthayp://task/{id}"
                                    )));
                                    let _ = copy.update(cx, |a, cx| {
                                        a.toast = "Task link copied".into();
                                        cx.notify();
                                    });
                                }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Previous task")
                                    .disabled(previous.is_none())
                                    .on_click(move |_, w, cx| {
                                        if let Some(id) = previous {
                                            let _ = prev.update(cx, |a, cx| a.open_task(id, w, cx));
                                        }
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Next task")
                                    .disabled(next.is_none())
                                    .on_click(move |_, w, cx| {
                                        if let Some(id) = next {
                                            let _ = following
                                                .update(cx, |a, cx| a.open_task(id, w, cx));
                                        }
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new(if archived {
                                    "Restore task"
                                } else {
                                    "Archive task"
                                })
                                .on_click(move |_, w, cx| {
                                    let _ = archive.update(cx, |a, cx| {
                                        if a.store.task(id).is_some_and(|t| t.archived) {
                                            a.snapshot();
                                            a.store.task_mut(id).unwrap().archived = false;
                                            a.toast = "Task restored".into();
                                            cx.notify();
                                        } else {
                                            a.archive_task(id, w, cx);
                                        }
                                    });
                                }),
                            )
                        },
                    ),
                ),
        );
        if is_goal && self.goal_steps_tab == Some(id) {
            return col()
                .size_full()
                .child(head)
                .when_some(self.task_start_feedback.as_ref().filter(|(task, _)| *task == id), |d, (_, message)| {
                    d.child(div().px_6().py_2().text_sm().text_color(rgb(colors.danger)).child(message.clone()))
                })
                .child(self.goal_steps_view(id, cx))
                .into_any_element();
        }
        let children: Vec<_> = self
            .store
            .tasks
            .iter()
            .filter(|c| c.parent_id == Some(id) && !c.archived)
            .cloned()
            .collect();
        let show_children = is_goal || t.research.is_none() || !children.is_empty();
        let mut body = col()
            .flex_1()
            .min_w_0()
            .gap_3()
            .px_6()
            .py_4()
            .when_some(t.parent_id, |d, parent| {
                d.child(label(
                    format!(
                        "Subtask of {}",
                        self.store
                            .task(parent)
                            .map(|p| p.title.clone())
                            .unwrap_or_default()
                    ),
                    cx,
                ))
            })
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(relative(1.3))
                    .child(t.title.clone()),
            )
            .child(
                div()
                    .text_color(rgb(colors.secondary))
                    .line_height(relative(1.7))
                    .child(if t.description.is_empty() {
                        "Add a description to give this task more context.".into()
                    } else {
                        t.description.clone()
                    }),
            )
            .when(is_goal, |d| d.child(self.request_brief_view(id, cx)))
            .child(self.materials_panel(id, cx))
            .when(t.research.is_some(), |d| {
                d.child(self.research_task_panel(id, cx))
            })
            .when(show_children, |d| d.child(
                row().id("task-children-heading")
                    .mt_4()
                    .justify_between()
                    .child(muted(
                        format!(
                            "{}  {}/{}",
                            if is_goal {
                                "Research steps"
                            } else {
                                "Subtasks"
                            },
                            children.iter().filter(|t| t.stage == Stage::Done).count(),
                            children.len()
                        ),
                        cx,
                    ))
                    .when(t.research.is_none(), |d| {
                        d.child(
                            button("add-subtask", "+  Add subtask")
                                .disabled(t.archived || self.task_running(id))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.begin_draft(None, Some(id), Stage::Backlog, w, cx)
                                })),
                        )
                    }).map(observed),
            ));
        let mut subtask_list = col().gap_1();
        for child in children {
            let cid = child.id;
            let title = format!("{} · {}", self.task_location_label(&child), child.title);
            let hint = self.store.step_block_reason(cid).map(|reason| format!("{title}\n{reason}"))
                .unwrap_or_else(|| title.clone())
                .chars()
                .collect::<Vec<_>>()
                .chunks(36)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            subtask_list = subtask_list.child(
                quiet_button(format!("subtask-{cid}"), "")
                    .w_full()
                    .h_8()
                    .px_2()
                    .tooltip(hint)
                    .accessibility_label(title.clone())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_task(cid, w, cx)))
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .child(task_state_icon(&self.store, &child, cx))
                            .child(single_line_text(format!("subtask-title-{cid}"), title))
                            .when(
                                child.research.is_some()
                                    && self
                                        .store
                                        .runs
                                        .iter()
                                        .any(|r| r.task_id == cid && r.status == "Succeeded")
                                    && self.store.research_result(cid).is_none(),
                                |d| d.child(chip("Needs refresh", colors.accent, cx)),
                            )
                            .child(div().flex_shrink_0().child(label("Open ›", cx))),
                    ),
            );
        }
        body = body.when(show_children, |d| d.child(subtask_list));
        let mut activity_panel = col().gap_2();
        if self.show_task_activity {
            for activity in t.activity.iter().rev().take(8) {
                activity_panel = activity_panel.child(
                    row()
                        .items_start()
                        .child(div().text_color(rgb(colors.muted)).child("·"))
                        .child(label(activity_display(activity, &t.created_at), cx)),
                );
            }
            for (i, comment) in t.comments.iter().enumerate() {
                activity_panel = activity_panel.child(
                    col()
                        .id(format!("comment-{i}"))
                        .p_3()
                        .gap_2()
                        .rounded_md()
                        .bg(rgb(colors.panel))
                        .child(
                            row()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(comment.author.clone()),
                                )
                                .child(label(comment.created_at.clone(), cx)),
                        )
                        .child(comment.body.clone()),
                );
            }
            activity_panel = activity_panel
                .child(
                    div()
                        .id("comment-input")
                        .child(Textarea::new(&self.comment))
                        .map(observed),
                )
                .child(row().justify_end().child(
                    primary_button("post-comment", "Comment").on_click(cx.listener(
                        move |this, _, w, cx| {
                            let text = this.comment.read(cx).value().to_string();
                            if text.trim().is_empty() {
                                return;
                            }
                            this.snapshot();
                            let result = this.store.add_comment(id, text);
                            if result.is_ok() {
                                this.comment.update(cx, |s, cx| s.set_value("", w, cx));
                            }
                            this.finish(result, "Comment added", cx);
                        },
                    )),
                ));
        }
        body = body.child(
            col()
                .border_t_1()
                .border_color(rgb(colors.line))
                .pt_2()
                .gap_2()
                .child(
                    quiet_button("task-activity-toggle", "Activity")
                        .self_start()
                        .px_0()
                        .text_color(rgb(colors.muted))
                        .icon(if self.show_task_activity {
                            assets::IconName::ChevronDown
                        } else {
                            assets::IconName::ChevronRight
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_task_activity = !this.show_task_activity;
                            cx.notify();
                        })),
                )
                .when(self.show_task_activity, |d| d.child(activity_panel)),
        );
        let mut properties = col().w(px(240.)).flex_shrink_0().p_3().m_4().ml_0().gap_2()
            .child(self.step_execution_panel(id, cx));
        let goal_has_plan = t
            .research
            .as_ref()
            .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal)
            && self.store.has_research_plan(id);
        let has_result = t
            .research
            .as_ref()
            .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal)
            && self.store.research_result(id).is_some();
        let previously_completed = t.stage == Stage::Done
            || self
                .store
                .runs
                .iter()
                .any(|run| run.task_id == id && run.status == "Succeeded");
        let needs_refresh = previously_completed
            && !has_result
            && t.research
                .as_ref()
                .is_some_and(|r| r.kind != crate::research::ResearchKind::Goal);
        if !goal_has_plan && t.research.is_none() && t.parent_id.is_none() {
            properties = properties.child(
                button(
                    "execute-task",
                    if self.task_running(id) {
                        "Working…"
                    } else if t
                        .research
                        .as_ref()
                        .is_some_and(|r| r.kind == crate::research::ResearchKind::Goal)
                    {
                        "Plan research steps"
                    } else if previously_completed {
                        "Run again"
                    } else {
                        "Start task"
                    },
                )
                .when(!previously_completed, |b| b.primary())
                .disabled(
                    self.store.task_is_archived(id)
                        || self.store.research_pending_approval(id)
                        || t.stage == Stage::Cancelled
                        || self.task_running(id),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.start_run(id, cx))),
            );
        }
        if needs_refresh && !self.task_running(id) {
            properties = properties.child(
                col().gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(colors.accent)).child("Completed previously · update needed"))
                    .child(div().text_xs().text_color(rgb(colors.muted))
                        .child("No reusable result is available for the current inputs. Finish any prerequisite updates before running again.")),
            );
        }
        if self.task_running(id) {
            properties =
                properties
                    .child(button("task-progress", "View progress").on_click(
                        cx.listener(move |this, _, w, cx| {
                            if let Some(goal) = this.store.owning_research_goal(id) {
                                let layout = this.steps_layout;
                                this.open_goal_steps(goal, w, cx);
                                this.steps_layout = layout;
                            } else {
                                this.view_task_result(id, cx);
                            }
                        }),
                    ));
        }
        if let Some((task, message)) = &self.task_start_feedback {
            if *task == id {
                properties = properties.child(
                    col()
                        .gap_1()
                        .p_2()
                        .border_l_2()
                        .border_color(rgb(colors.accent))
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                            if previously_completed {
                                "Task could not run again"
                            } else {
                                "Task could not start"
                            },
                        ))
                        .child({
                            let related = self.store.tasks.iter().find_map(|task| {
                                let name =
                                    format!("{} · {}", self.store.identifier(task), task.title);
                                message.split_once(&name).map(|(before, after)| {
                                    (
                                        before.trim().to_string(),
                                        name.clone(),
                                        after.trim_start_matches('.').trim().to_string(),
                                    )
                                })
                            });
                            if let Some((before, name, after)) = related {
                                col()
                                    .gap_2()
                                    .child(
                                        div().text_sm().text_color(rgb(colors.muted)).child(before),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(colors.accent))
                                            .child(name),
                                    )
                                    .child(
                                        div().text_sm().text_color(rgb(colors.muted)).child(after),
                                    )
                                    .into_any_element()
                            } else {
                                div().text_sm().child(message.clone()).into_any_element()
                            }
                        }),
                );
            }
        }
        let missing_folder = self
            .store
            .projects
            .iter()
            .find(|p| p.id == t.project_id)
            .is_none_or(|p| p.directory.is_empty());
        let missing_model = if let Some(r) = &t.research {
            !crate::opencode::valid_model(self.store.research_model(r.kind))
        } else {
            self.store
                .policies
                .iter()
                .find(|p| p.stage == t.stage)
                .is_none_or(|p| {
                    p.agent != "Manual"
                        && !crate::opencode::valid_model(self.store.effective_model(&p.model))
                })
        };
        if missing_folder {
            properties = properties
                .child(label(
                    "Choose where this project's work will be saved before starting.",
                    cx,
                ))
                .child(
                    button("task-setup-folder", "Choose working folder…").on_click(
                        cx.listener(move |this, _, w, cx| this.open_directory(t.project_id, w, cx)),
                    ),
                );
        }
        if missing_model {
            properties =
                properties
                    .child(label("Choose a model in Settings before starting.", cx))
                    .child(button("task-setup-model", "Open settings").on_click(
                        cx.listener(|this, _, w, cx| this.navigate(Scope::Settings, w, cx)),
                    ));
        }
        properties = properties.child(div().h_2()).child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(colors.text))
                .id("task-properties-heading")
                .child("Task").map(observed),
        );
        if t.parent_id.is_none() {
            properties = properties.child(
                row()
                    .justify_between()
                    .min_w_0()
                    .child(label("Topic", cx))
                    .child(self.topic_picker(Some(id), t.project_id, cx)),
            );
        }
        for (prop, name, value) in [
            (Property::Stage, "Stage", stage_text(t.stage)),
            (Property::Priority, "Priority", t.priority.label().into()),
            (
                Property::Project,
                "Project",
                self.store
                    .projects
                    .iter()
                    .find(|p| p.id == t.project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
            ),
            (
                Property::Cycle,
                "Cycle",
                t.cycle.clone().unwrap_or("No cycle".into()),
            ),
            (
                Property::Bucket,
                "Bucket",
                if t.bucket.is_empty() {
                    "No bucket".into()
                } else {
                    t.bucket.clone()
                },
            ),
            (
                Property::Parent,
                "Parent",
                t.parent_id
                    .and_then(|pid| self.store.task(pid))
                    .map(|p| self.store.identifier(p))
                    .unwrap_or("No parent".into()),
            ),
        ] {
            if t.parent_id.is_some() && t.research.is_some() && prop == Property::Stage {
                continue;
            }
            if !self.show_task_details
                && matches!(prop, Property::Cycle | Property::Bucket | Property::Parent)
            {
                continue;
            }
            properties = properties.child(
                row()
                    .flex_wrap()
                    .justify_between()
                    .child(label(name, cx))
                    .child(
                        quiet_button(format!("property-{name}"), value.clone())
                            .px_2()
                            .h(px(28.))
                            .max_w(px(164.))
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(match prop {
                                Property::Stage => stage_color(t.stage, cx),
                                Property::Priority if t.priority == Priority::Urgent => {
                                    colors.danger
                                }
                                Property::Priority if t.priority == Priority::High => {
                                    colors.warning
                                }
                                _ => colors.accent,
                            }))
                            .tooltip(format!("Change {}: {value}", name.to_lowercase()))
                            .when(self.property == Some(prop), |b| {
                                b.bg(rgb(colors.accent_surface))
                            })
                            .dropdown_caret(true)
                            .open(self.property == Some(prop))
                            .disabled(t.archived || self.task_running(id))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.property = if this.property == Some(prop) {
                                    None
                                } else {
                                    Some(prop)
                                };
                                cx.notify();
                            })),
                    ),
            );
            if self.property == Some(prop) {
                properties = properties.child(self.property_options(id, prop, cx));
            }
        }
        properties = properties
            .child(
                quiet_button(
                    "task-details-toggle",
                    if self.show_task_details {
                        "Hide details"
                    } else {
                        "More details…"
                    },
                )
                .self_start()
                .px_0()
                .icon(if self.show_task_details {
                    assets::IconName::ChevronUp
                } else {
                    assets::IconName::ChevronDown
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_task_details = !this.show_task_details;
                    this.property = None;
                    cx.notify();
                })),
            );
        if self.show_task_details {
            properties = properties
                .child(div().h_2().border_b_1().border_color(rgb(colors.line)))
                .child(
                    row()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Labels"),
                        )
                        .child(
                            tool_icon("edit-labels", assets::IconName::Pencil, "Edit labels")
                                .disabled(t.archived || self.task_running(id))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.begin_draft(Some(id), None, Stage::Backlog, w, cx)
                                })),
                        ),
                )
                .child(
                    row()
                        .flex_wrap()
                        .gap_1()
                        .children(t.labels.iter().map(|name| {
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(colors.tag_surface))
                                .text_xs()
                                .text_color(rgb(colors.tag_text))
                                .child(name.clone())
                        }))
                        .when(t.labels.is_empty(), |d| d.child(label("No labels", cx))),
                )
                .child(
                    div()
                        .pt_2()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Execution settings"),
                );
            let execution_field = |name: &str, value: String| {
                row()
                    .items_start()
                    .min_w_0()
                    .child(
                        div()
                            .w(px(88.))
                            .flex_shrink_0()
                            .child(label(name.to_string(), cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(rgb(colors.text))
                            .whitespace_normal()
                            .overflow_hidden()
                            .child(value),
                    )
            };
            let model_fields = |model: &str| {
                let effective = self.store.effective_model(model);
                col()
                    .gap_2()
                    .child(execution_field(
                        "Model source",
                        if model.is_empty() {
                            "Global default"
                        } else {
                            "Stage override"
                        }
                        .into(),
                    ))
                    .child(execution_field(
                        "Model",
                        if effective.is_empty() {
                            "Not configured".into()
                        } else {
                            effective.to_string()
                        },
                    ))
            };
            if let Some(r) = &t.research {
                properties = properties
                    .child(model_fields(
                        &self.store.research_config.models[r.kind.index()],
                    ))
                    .child(execution_field(
                        "Browser space",
                        self.store
                            .research_space(id)
                            .unwrap_or_else(|| r.space_name.clone()),
                    ))
                    .child(button("configure-research", "Research settings…").on_click(
                        cx.listener(|this, _, w, cx| {
                            this.navigate(Scope::Research, w, cx);
                            this.show_research_settings = true;
                        }),
                    ));
            } else if let Some(p) = self.store.policies.iter().find(|p| p.stage == t.stage) {
                properties = properties.child(execution_field("Agent", p.agent.clone()));
                if p.agent == "Manual" {
                    properties = properties.child(execution_field("Execution", "Manual".into()));
                } else {
                    properties = properties.child(model_fields(&p.model));
                }
                properties = properties.child(button("configure-stage", "Stage rules…").on_click(
                    cx.listener(move |this, _, w, cx| {
                        this.navigate(Scope::Workflow, w, cx);
                        this.select_policy(t.stage, w, cx);
                    }),
                ));
            }
            properties = properties.child(button("task-folder", "Working folder…").on_click(
                cx.listener(move |this, _, w, cx| this.open_directory(t.project_id, w, cx)),
            ));
        }
        col()
            .size_full()
            .child(head)
            .child(
                row()
                    .id("detail-scroll")
                    .track_scroll(&self.detail_scroll)
                    .items_start()
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .child(body)
                    .child(properties)
                    .map(observed),
            )
            .into_any_element()
    }
    fn property_options(&self, id: u64, property: Property, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut options = col()
            .p_1()
            .gap_0()
            .bg(rgb(colors.panel))
            .border_1()
            .border_color(rgb(colors.line))
            .rounded_md();
        match property {
            Property::Stage => {
                for stage in Stage::ALL {
                    options = options.child(
                        property_choice(
                            format!("set-{}", stage.key()),
                            stage.label(),
                            Some(stage.symbol()),
                            self.store.task(id).is_some_and(|t| t.stage == stage),
                        )
                        .w_full()
                        .justify_start()
                        .when(self.store.task(id).is_some_and(|t| t.stage == stage), |b| {
                            b.bg(rgb(colors.accent_surface))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_stage(id, stage, cx);
                            this.property = None;
                        })),
                    );
                }
            }
            Property::Priority => {
                for priority in Priority::ALL {
                    options = options.child(
                        property_choice(
                            format!("priority-{}", priority.key()),
                            priority.label(),
                            None,
                            self.store.task(id).is_some_and(|t| t.priority == priority),
                        )
                        .w_full()
                        .justify_start()
                        .when(
                            self.store.task(id).is_some_and(|t| t.priority == priority),
                            |b| b.bg(rgb(colors.accent_surface)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.snapshot();
                            if let Some(t) = this.store.task_mut(id) {
                                t.priority = priority;
                            }
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Project => {
                for p in &self.store.projects {
                    let pid = p.id;
                    options = options.child(
                        property_choice(
                            format!("move-project-{pid}"),
                            p.name.clone(),
                            None,
                            self.store.task(id).is_some_and(|t| t.project_id == pid),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.snapshot();
                            let result = this.store.move_project(id, pid);
                            this.property = None;
                            this.finish(result, "Task and subtasks moved", cx);
                        })),
                    );
                }
            }
            Property::Cycle => {
                let cycles: std::collections::BTreeSet<_> = self
                    .store
                    .tasks
                    .iter()
                    .filter_map(|t| t.cycle.clone())
                    .collect();
                for value in std::iter::once("No cycle".to_string()).chain(cycles) {
                    options = options.child(
                        property_choice(
                            format!("set-cycle-{value}"),
                            value.clone(),
                            None,
                            self.store
                                .task(id)
                                .is_some_and(|t| t.cycle.as_deref().unwrap_or("No cycle") == value),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.snapshot();
                            if let Some(t) = this.store.task_mut(id) {
                                t.cycle = if value == "No cycle" {
                                    None
                                } else {
                                    Some(value.clone())
                                };
                            }
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Bucket => {
                for value in ["No bucket", "Now", "Next", "Later"] {
                    options = options.child(
                        property_choice(
                            format!("set-bucket-{value}"),
                            value,
                            None,
                            self.store.task(id).is_some_and(|t| {
                                if t.bucket.is_empty() {
                                    value == "No bucket"
                                } else {
                                    t.bucket == value
                                }
                            }),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.snapshot();
                            if let Some(t) = this.store.task_mut(id) {
                                t.bucket = if value == "No bucket" {
                                    String::new()
                                } else {
                                    value.into()
                                };
                            }
                            this.property = None;
                            cx.notify();
                        })),
                    );
                }
            }
            Property::Parent => {
                options = options.child(
                    property_choice(
                        "no-parent",
                        "No parent",
                        None,
                        self.store.task(id).is_some_and(|t| t.parent_id.is_none()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.snapshot();
                        let result = this.store.set_parent(id, None);
                        this.property = None;
                        this.finish(result, "Parent updated", cx);
                    })),
                );
                for task in self.store.tasks.iter().filter(|t| {
                    t.id != id
                        && !t.archived
                        && t.parent_id.is_none()
                        && self
                            .store
                            .task(id)
                            .is_some_and(|current| current.project_id == t.project_id)
                }) {
                    let parent = task.id;
                    options = options.child(
                        property_choice(
                            format!("set-parent-{parent}"),
                            task.title.clone(),
                            None,
                            self.store
                                .task(id)
                                .is_some_and(|t| t.parent_id == Some(parent)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.snapshot();
                            let result = this.store.set_parent(id, Some(parent));
                            this.property = None;
                            this.finish(result, "Parent updated", cx);
                        })),
                    );
                }
            }
        }
        options.into_any_element()
    }
    fn workflow(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let mut stages = col().w(px(300.)).flex_shrink_0().gap_2();
        for stage in Stage::ALL {
            let p = self.store.policies.iter().find(|p| p.stage == stage);
            stages = stages.child(
                col()
                    .id(format!("policy-{}", stage.key()))
                    .p_4()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if self.policy_stage == stage {
                        colors.accent
                    } else {
                        colors.line
                    }))
                    .bg(rgb(if self.policy_stage == stage {
                        colors.accent_surface
                    } else {
                        colors.panel
                    }))
                    .cursor_pointer()
                    .hover(|d| d.border_color(rgb(colors.accent)))
                    .on_click(cx.listener(move |this, _, w, cx| this.select_policy(stage, w, cx)))
                    .child(
                        row()
                            .justify_between()
                            .child(
                                div()
                                    .text_color(rgb(stage_color(stage, cx)))
                                    .child(stage_text(stage)),
                            )
                            .child(label("→", cx)),
                    )
                    .child(label(
                        p.map(|p| {
                            if p.agent == "Manual" {
                                "Manual · No agent execution".into()
                            } else {
                                format!("{} · {}", p.agent, self.model_source_label(&p.model))
                            }
                        })
                        .unwrap_or("Manual".into()),
                        cx,
                    ))
                    .map(observed),
            );
        }
        let editor = col()
            .flex_1()
            .min_w_0()
            .max_w(px(660.))
            .gap_5()
            .p_6()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(colors.line))
            .bg(rgb(colors.panel))
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} stage", self.policy_stage.label())),
                    )
                    .child(chip("LOCAL EXECUTION", colors.accent, cx)),
            )
            .child(muted(
                "Choose who works on this stage and how they should approach it.",
             cx))
            .child(label("AGENT", cx))
            .child(
                row()
                    .gap_3()
                    .children(["Manual", "Codex", "OpenCode"].into_iter().map(|agent| {
                        select_button(format!("agent-{agent}"), agent, self.policy_agent == agent)
                            .disabled(agent=="Codex")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.policy_agent = agent.into();
                                cx.notify();
                            }))
                    })),
            )
            .child(label("MODEL OVERRIDE · OPTIONAL", cx))
            .child(Input::new(&self.policy_model).id("policy-model"))
            .child(row().flex_wrap()
                .child(button("policy-use-global", "Use global model")
                    .disabled(self.policy_model.read(cx).value().is_empty())
                    .on_click(cx.listener(|this, _, w, cx| { this.policy_model.update(cx, |s, cx| s.set_value("", w, cx)); cx.notify(); })))
                .child(button("policy-global-settings", "Global model settings →").on_click(cx.listener(|this, _, w, cx| this.navigate(Scope::Settings, w, cx)))))
            .child(label(if self.policy_agent == "Manual" { "No agent execution while this stage is Manual".into() } else { self.model_source_label(self.policy_model.read(cx).value().trim()) }, cx))
            .child(button("choose-model","Choose from local models…").dropdown_caret(true).open(self.show_models).disabled(self.available_models.is_empty()).on_click(cx.listener(|this,_,_,cx|{this.show_models = !this.show_models;cx.notify();})))
            .when(self.show_models,|d|d.child(col().id("models-list").h(px(200.)).overflow_y_scroll().gap_1().children(self.available_models.iter().filter(|m|{let query=self.policy_model.read(cx).value().to_lowercase();query.is_empty()||query=="choose a model"||m.to_lowercase().contains(&query)}).map(|model|{let model=model.clone();button(format!("model-{model}"),model.clone()).on_click(cx.listener(move|this,_,w,cx|{this.policy_model.update(cx,|s,cx|s.set_value(model.clone(),w,cx));this.show_models=false;cx.notify();}))}))))
            .child(label("Leave empty to use the global model. A provider/model or provider/model#variant here overrides it after saving.", cx))
            .child(label("STAGE INSTRUCTIONS", cx))
            .child(
                div()
                    .id("policy-prompt")
                    .child(Textarea::new(&self.policy_prompt)),
            )
            .child(
                Checkbox::new("policy-auto")
                    .label("Automatic stage execution (coming later)").disabled(true)
                    .checked(self.policy_auto)
                    .on_change(cx.listener(|this, v, _, cx| {
                        this.policy_auto = *v;
                        cx.notify();
                    })),
            )
            .child(label(
                "This version starts real runs only when you click Run with OpenCode. Codex is not connected yet.",
             cx))
            .child(
                row().justify_end().child(
                    primary_button("save-policy", "Save stage configuration")
                        .on_click(cx.listener(|this, _, _, cx| this.save_policy(cx))),
                ),
            );
        col().size_full().child(row().h_16().px_8().border_b_1().border_color(rgb(colors.line)).child(div().font_weight(FontWeight::SEMIBOLD).child("Stage workflow")))
            .child(col().id("workflow-scroll").flex_1().min_h_0().overflow_y_scroll().p_8().gap_6()
                .child(col().gap_2().child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("The right agent for each stage."))
                    .child(muted("Plan with one model, build with another, and review before shipping.", cx)))
                .child(label(self.storage.as_ref().map(|s|format!("Data: {}",s.path().display())).unwrap_or_else(||"Preview workspace (not saved)".into()), cx))
                .child(col().p_4().gap_3().border_1().border_color(rgb(colors.line)).rounded_md()
                    .child(row().justify_between().child(label("LOCAL OPENCODE", cx)).child(label(self.connection_status.clone(), cx)))
                    .child(row().child(div().flex_1().child(Input::new(&self.executable_input).id("opencode-path"))).child(button("refresh-models","Refresh models").loading(self.discovering_models()).on_click(cx.listener(|this,_,_,cx|this.discover_models(cx)))))
                    .child(row().flex_wrap().children(self.store.projects.iter().map(|p|{let id=p.id;select_button(format!("connection-project-{id}"),p.name.clone(),self.connection_project==id).on_click(cx.listener(move|this,_,_,cx|{this.connection_project=id;cx.notify();}))})))
                    .child(row().child(label(self.store.projects.iter().find(|p|p.id==self.connection_project).map(|p|if p.directory.is_empty(){"Project folder is not set".into()}else{p.directory.clone()}).unwrap_or_default(), cx)).child(button("connection-folder","Set folder").on_click(cx.listener(|this,_,w,cx|this.open_directory(this.connection_project,w,cx))))))
                .child(row().items_start().gap_6().child(stages).child(editor)).map(observed)).into_any_element()
    }
    fn directory_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let name = self
            .store
            .projects
            .iter()
            .find(|p| Some(p.id) == self.directory_project)
            .map(|p| p.name.clone())
            .unwrap_or_default();
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
                    .w(px(620.))
                    .p_6()
                    .gap_4()
                    .bg(rgb(colors.panel))
                    .rounded_lg()
                    .child(div().text_lg().child(format!("Working folder · {name}")))
                    .child(muted(
                        "OpenCode will read and modify files in this folder when you run a task.",
                        cx,
                    ))
                    .child(Input::new(&self.directory_input).id("directory-path"))
                    .when(!self.error.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(rgb(colors.danger))
                                .child(self.error.clone()),
                        )
                    })
                    .child(
                        row()
                            .justify_end()
                            .child(button("cancel-directory", "Cancel").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.directory_project = None;
                                    this.error.clear();
                                    this.save_feedback = None;
                                    w.focus(&this.focus, cx);
                                    cx.notify();
                                },
                            )))
                            .child(
                                primary_button("save-directory", "Save folder").on_click(
                                    cx.listener(|this, _, _, cx| this.save_directory(cx)),
                                ),
                            ),
                    ),
            )
            .focus_trap("directory-focus", &self.modal_focus)
            .into_any_element()
    }
    fn editor_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = palette(cx);
        let Some(d) = &self.draft else {
            return div().into_any_element();
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
                    .w(px(680.))
                    .max_h(relative(0.92))
                    .id("draft-scroll")
                    .overflow_y_scroll()
                    .p_6()
                    .gap_4()
                    .rounded(px(12.))
                    .bg(rgb(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border_strong))
                    .child(
                        row()
                            .justify_between()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                                if d.id.is_some() {
                                    "Edit task"
                                } else if d.parent.is_some() {
                                    "New subtask"
                                } else {
                                    "New task"
                                },
                            ))
                            .child(
                                icon_button("cancel-draft", "×", "Close without saving")
                                    .tooltip("Close without saving")
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.draft = None;
                                        this.editing = false;
                                        this.error.clear();
                                        w.focus(&this.focus, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(label("TITLE", cx))
                    .child(Input::new(&d.title).id("draft-title"))
                    .child(label("DESCRIPTION", cx))
                    .child(
                        div()
                            .id("draft-description")
                            .child(Textarea::new(&d.description)),
                    )
                    .child(
                        row()
                            .gap_3()
                            .flex_wrap()
                            .children(Stage::ALL.into_iter().map(|stage| {
                                select_button(
                                    format!("draft-stage-{}", stage.key()),
                                    stage.label(),
                                    d.stage == stage,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(d) = &mut this.draft {
                                            d.stage = stage;
                                        }
                                        cx.notify();
                                    },
                                ))
                            })),
                    )
                    .child(
                        row()
                            .gap_3()
                            .flex_wrap()
                            .children(self.store.projects.iter().map(|p| {
                                let id = p.id;
                                select_button(
                                    format!("draft-project-{id}"),
                                    p.name.clone(),
                                    d.project == id,
                                )
                                .disabled(d.parent.is_some())
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(d) = &mut this.draft {
                                            d.project = id;
                                        }
                                        cx.notify();
                                    },
                                ))
                            })),
                    )
                    .child(
                        row()
                            .items_start()
                            .gap_4()
                            .child(
                                col()
                                    .flex_1()
                                    .gap_2()
                                    .child(label("LABELS, COMMA SEPARATED", cx))
                                    .child(Input::new(&d.labels).id("draft-labels")),
                            ),
                    )
                    .when(!self.error.is_empty(), |v| {
                        v.child(
                            div()
                                .text_color(rgb(colors.danger))
                                .child(self.error.clone()),
                        )
                    })
                    .child(
                        row()
                            .justify_end()
                            .gap_3()
                            .child(button("discard-draft", "Cancel").on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.draft = None;
                                    this.editing = false;
                                    this.error.clear();
                                    this.save_feedback = None;
                                    w.focus(&this.focus, cx);
                                    cx.notify();
                                },
                            )))
                            .child(
                                primary_button(
                                    "save-task",
                                    if d.id.is_some() {
                                        "Save changes"
                                    } else {
                                        "Create task"
                                    },
                                )
                                .on_click(cx.listener(|this, _, w, cx| this.save_draft(w, cx))),
                            ),
                    ),
            )
            .focus_trap("draft-focus", &self.modal_focus)
            .into_any_element()
    }
}
impl Render for TaskApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = palette(cx);
        if self.workspace_required {
            if let Some(path) = self.workspace_choice.take() {
                self.directory_input.update(cx, |input, cx| {
                    input.set_value(path.to_string_lossy().into_owned(), window, cx)
                });
            }
            if let Some(result) = self
                .workspace_setup
                .as_ref()
                .and_then(|rx| match rx.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                        "Workspace setup stopped unexpectedly. Please retry.".into(),
                    )),
                })
            {
                self.workspace_setup = None;
                match result {
                    Ok(root) => {
                        self.workspace_root = Some(root.clone());
                        self.attach_storage(
                            crate::persistence::Storage::new(
                                root.join("workspace/workspace.sqlite3"),
                            ),
                            window,
                            cx,
                        );
                        self.workspace_required =
                            self.storage.is_none() || !self.storage_error.is_empty();
                    }
                    Err(error) => self.storage_error = error,
                }
            }
            if self.workspace_required {
                return col().size_full().bg(rgb(colors.canvas)).text_color(rgb(colors.text))
                    .items_center().justify_center()
                    .child(col().w_full().max_w(px(680.)).p_8().gap_4()
                        .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Choose your workspace"))
                        .child(div().text_sm().text_color(rgb(colors.muted)).child("Choose a dedicated folder before continuing. Projects, reports, materials and local data will be stored in its workspace subfolder. Existing data will be copied; originals will be preserved."))
                        .child(row().gap_2()
                            .child(div().flex_1().min_w_0().child(Input::new(&self.directory_input).id("workspace-root")))
                            .child(button("browse-workspace", "Choose folder…").disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this, _, _, cx| this.choose_workspace(cx)))))
                        .child(div().text_sm().text_color(rgb(colors.muted)).child("Enter an existing absolute folder path, for example /Users/you/Documents/Research. Close any previous app instance before importing."))
                        .child(div().text_sm().text_color(rgb(colors.danger)).child(self.storage_error.clone()))
                        .child(button("setup-workspace", if self.workspace_setup.is_some() { "Preparing workspace…" } else { "Set workspace and continue" })
                            .primary().disabled(self.workspace_setup.is_some()).on_click(cx.listener(|this, _, _, cx| this.setup_workspace(cx)))))
                    .into_any_element();
            }
        }
        let content = if let Some(id) = self.selected {
            self.detail(id, cx)
        } else {
            match self.scope {
                Scope::Topic(id) => self.topic_view(id, cx),
                Scope::Workflow => self.workflow(cx),
                Scope::Runs => self.runs(cx),
                Scope::Recovery => self.recovery_view(cx),
                Scope::Research => self.research_view(cx),
                Scope::Settings => self.model_settings(cx),
                _ => self.collection(cx),
            }
        };
        let mut root = col()
            .id("app")
            .tab_group()
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(rgb(colors.canvas))
            .text_color(rgb(colors.text))
            .font_family(".AppleSystemUIFont")
            .text_sm()
            // Plain-text forms use Tab for navigation, not textarea indentation.
            // Dispatch the base actions to preserve modal focus containment.
            .capture_action(|_: &IndentInline, w, cx| {
                let action = cx
                    .build_action("root::Tab", None)
                    .expect("base Tab action registered");
                w.dispatch_action(action, cx);
                cx.stop_propagation();
            })
            .capture_action(|_: &OutdentInline, w, cx| {
                let action = cx
                    .build_action("root::TabPrev", None)
                    .expect("base TabPrev action registered");
                w.dispatch_action(action, cx);
                cx.stop_propagation();
            })
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, w, cx| {
                if event.keystroke.modifiers.platform && event.keystroke.key == "k" {
                    this.save_feedback = None;
                    this.selected = None;
                    this.draft = None;
                    this.editing = false;
                    this.new_project = None;
                    this.topic_editor = None;
                    this.planning_project = None;
                    if matches!(
                        this.scope,
                        Scope::Workflow
                            | Scope::Runs
                            | Scope::Recovery
                            | Scope::Research
                            | Scope::Settings
                            | Scope::Topic(_)
                    ) {
                        this.scope = Scope::All;
                    }
                    this.search.update(cx, |s, cx| s.focus(w, cx));
                    cx.notify();
                } else if event.keystroke.modifiers.platform
                    && event.keystroke.key == "["
                    && this.draft.is_none()
                    && this.new_project.is_none()
                    && this.topic_editor.is_none()
                    && this.planning_project.is_none()
                    && this.directory_project.is_none()
                {
                    this.go_back(w, cx);
                    cx.stop_propagation();
                } else if event.keystroke.modifiers.platform && event.keystroke.key == "n" {
                    this.open_research_intake(w, cx);
                } else if event.keystroke.key == "escape" {
                    this.save_feedback = None;
                    this.draft = None;
                    this.editing = false;
                    this.property = None;
                    this.new_project = None;
                    this.topic_editor = None;
                    this.planning_project = None;
                    this.directory_project = None;
                    this.error.clear();
                    w.focus(&this.focus, cx);
                    cx.notify();
                }
            }))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("workspace-split")
                        .with_state(&self.sidebar_split)
                        .child(
                            resizable_panel()
                                .size(px(260.))
                                .size_range(px(220.)..px(480.))
                                .fixed()
                                .child(self.sidebar(cx)),
                        )
                        .child(
                            resizable_panel().size_range(px(480.)..Pixels::MAX).child(
                                col()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .child(self.navigation_header(cx))
                                    .when(self.save_feedback.is_some(), |d| {
                                        d.child(self.save_feedback_view(cx))
                                    })
                                    .child(div().flex_1().min_h_0().child(content)),
                            ),
                        ),
                ),
            )
            .child(
                row()
                    .h(px(30.))
                    .px_4()
                    .bg(rgb(colors.side))
                    .border_t_1()
                    .border_color(rgb(colors.line))
                    .justify_between()
                    .text_xs()
                    .child(
                        row()
                            .child(div().size(px(5.)).rounded_full().bg(rgb(colors.accent)))
                            .child(muted(
                                if !self.storage_error.is_empty() {
                                    format!("NOT SAVED: {}", self.storage_error)
                                } else if self.error.is_empty() {
                                    self.toast.clone()
                                } else {
                                    self.error.clone()
                                },
                                cx,
                            )),
                    )
                    .child(
                        row()
                            .child(
                                button("undo", "Undo")
                                    .small()
                                    .h(px(24.))
                                    .px_2()
                                    .disabled(
                                        self.undo.is_empty()
                                            || !self.active_runs.is_empty()
                                            || !self.queued_runs.is_empty(),
                                    )
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.undo_change(w, cx);
                                    })),
                            )
                            .child(label("⌘K Search    ⌘N New research", cx)),
                    ),
            );
        if self.editing {
            root = root.child(self.editor_overlay(cx));
        }
        if self.planning_project.is_some() {
            root = root.child(self.project_plan_overlay(cx));
        }
        if self.directory_project.is_some() {
            root = root.child(self.directory_overlay(cx));
        }
        if self.topic_editor.is_some() {
            root = root.child(self.topic_editor_view(cx));
        }
        if let Some(input) = &self.new_project {
            let input = input.clone();
            root = root.child(
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
                            .w(px(420.))
                            .p_6()
                            .gap_4()
                            .bg(rgb(colors.panel))
                            .rounded_lg()
                            .child(div().text_lg().child("New project"))
                            .child(Input::new(&input).id("project-name"))
                            .child(muted("Create a project to organize topics. You can add research requests later.", cx))
                            .when(!self.error.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_color(rgb(colors.danger))
                                        .child(self.error.clone()),
                                )
                            })
                            .child(
                                row()
                                    .justify_end()
                                    .child(button("cancel-project", "Cancel").on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.new_project = None;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(
                                        primary_button("create-project", "Create project")
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                let name = input.read(cx).value().to_string();
                                                if name.trim().is_empty() {
                                                    this.error =
                                                        "A project name is required".into();
                                                    cx.notify();
                                                    return;
                                                }
                                                this.snapshot();
                                                let id = this
                                                    .store
                                                    .projects
                                                    .iter()
                                                    .map(|p| p.id)
                                                    .max()
                                                    .unwrap_or(0)
                                                    + 1;
                                                let directory = match this.managed_project_directory(id) {
                                                    Ok(path) => path,
                                                    Err(error) => { this.error = error; cx.notify(); return; }
                                                };
                                                this.store.projects.push(Project {
                                                    id,
                                                    name: name.trim().into(),
                                                    key: format!("P{id}"),
                                                    directory,
                                                    research_brief: String::new(),
                                                });
                                                this.new_project = None;
                                                this.navigate(Scope::Project(id), w, cx);
                                                this.confirm_save("Project created · add a topic or start a research request", cx);
                                            })),
                                    ),
                            ),
                    )
                    .focus_trap("project-focus", &self.modal_focus),
            );
        }
        root.into_any_element()
    }
}
fn priority_symbol(p: Priority) -> &'static str {
    match p {
        Priority::None => "−",
        Priority::Low => "↓",
        Priority::Medium => "=",
        Priority::High => "↑",
        Priority::Urgent => "!",
    }
}
fn append_children(tasks: &[Task], id: u64, collapsed: &[u64], out: &mut Vec<Task>) {
    for t in tasks.iter().filter(|t| t.parent_id == Some(id)) {
        out.push(t.clone());
        if !collapsed.contains(&t.id) {
            append_children(tasks, t.id, collapsed, out);
        }
    }
}

pub(crate) fn run_color(status: &str, cx: &App) -> u32 {
    let colors = palette(cx);
    match status {
        "Running" | "Cancelling" | "Recovering" => colors.warning,
        "Succeeded" => colors.success,
        "Failed" | "Interrupted" | "Blocked" | "Stale" => colors.danger,
        _ => colors.muted,
    }
}
